import AppKit
import Sparkle

final class AppDelegate: NSObject, NSApplicationDelegate {
    /// カタログ照会が失敗したあと、訊き直してよくなるまでの間隔。
    /// 少しオフラインだっただけで通知1回につき1リクエスト、とならない程度に長くとる。
    private static let catalogRetryDelay: TimeInterval = 15

    let settings = SettingsStore.shared
    private let lifecycle = AppLifecycleWatcher()
    private let musicObserver = PlayerNotificationObserver(
        notificationName: AppleMusic.notificationName,
        parse: AppleMusic.parse
    )
    private let rpc = DiscordRPCClient()
    private let itunes = ITunesSearchClient()
    private let debouncer = Debouncer(delay: 0.8)
    private var menuBar: MenuBarController!
    private var updaterController: SPUStandardUpdaterController!

    private var music = MusicState()
    /// いまDiscordに何かを表示しているか。
    private var displaying = false

    /// 直近でDiscordに送った内容。同じものの再送を抑えるために持つ。
    private var lastSent: LastSent = .nothing

    private let reconnect = ReconnectBackoff()
    private let pauseHide = PauseHideTimer()
    private var catalogRetryWork: DispatchWorkItem?

    /// この接続でDiscordに送った最後の内容。
    ///
    /// 「まだ何も送っていない」と「表示を消すよう送った」は別物で、混ぜると
    /// 再接続直後の1発目が「前と同じ」と誤判定されて何も表示されなくなる。
    private enum LastSent {
        case nothing
        case sent([String: Any]?)

        func isEquivalent(to activity: [String: Any]?) -> Bool {
            guard case .sent(let previous) = self else { return false }
            return ActivityBuilder.isEquivalent(previous, activity)
        }
    }

    // MARK: - Lifecycle

    func applicationDidFinishLaunching(_: Notification) {
        updaterController = SPUStandardUpdaterController(
            startingUpdater: true, updaterDelegate: nil, userDriverDelegate: nil
        )

        menuBar = MenuBarController()
        menuBar.menuInput = { [weak self] in
            guard let self else {
                return MenuModel.Input(
                    status: StatusLinesBuilder.Input(), settings: SettingsStore.shared,
                    launchAtLogin: false
                )
            }
            return self.menuInput()
        }
        menuBar.onAction = { [weak self] action in self?.apply(action) }

        musicObserver.onUpdate = { [weak self] state, info in
            self?.handlePlayerUpdate(state: state, info: info)
        }
        rpc.onStateChange = { [weak self] state in self?.handleRPCState(state) }

        lifecycle.onPlayerLaunch = { [weak self] in self?.playerLaunched() }
        lifecycle.onPlayerTerminate = { [weak self] in self?.playerTerminated() }
        lifecycle.onDiscordLaunch = { [weak self] in
            // Discordが後から起動したケース: バックオフを待たず即接続
            guard let self, self.music.running else { return }
            self.cancelReconnect()
            self.attemptConnect()
        }
        lifecycle.start()
    }

    func applicationWillTerminate(_: Notification) {
        // asyncだとプロセス終了までに送信が走らないことがあるため、終了時だけ同期で行う
        rpc.shutdownSync(clearActivity: true)
    }

    // MARK: - Player lifecycle

    private func playerLaunched() {
        guard !music.running else { return }
        music.running = true
        musicObserver.start()
        attemptConnect()
        // 起動直後はスクリプティングに応答しないことがあるため少し待ってから初期状態を取得。
        // 通知は状態変化時にしか飛ばないため、既に再生中だった場合はこの1回が必要。
        scheduleInitialFetch(attempt: 0)
    }

    /// 初期状態の取得。オートメーション権限のプロンプト待ちや、プレイヤー起動直後の
    /// スクリプティング無応答で失敗することがあるため、バックオフ付きでリトライする
    /// （2s→4s→8s→16s→32s、計約1分で打ち切り）。
    private func scheduleInitialFetch(attempt: Int) {
        let delay = 2.0 * pow(2.0, Double(attempt))
        DispatchQueue.main.asyncAfter(deadline: .now() + delay) { [weak self] in
            guard let self, self.music.running, self.music.track == nil else { return }
            MusicAppleScript.currentState { [weak self] result in
                guard let self, self.music.running, self.music.track == nil else { return }
                if let (state, info) = result {
                    self.handlePlayerUpdate(state: state, info: info)
                } else if attempt < 4 {
                    self.scheduleInitialFetch(attempt: attempt + 1)
                }
            }
        }
    }

    private func playerTerminated() {
        guard music.running else { return }
        musicObserver.stop()
        music = MusicState()
        goDormant()
    }

    /// プレイヤーが終了した: 全て止めて完全休止に戻す（常時接続しない）。
    ///
    /// 「止めるものを1つ書き忘れる」が起きないよう、休止に入る道はこれ1本だけにする。
    private func goDormant() {
        displaying = false
        cancelReconnect()
        pauseHide.cancel()
        cancelCatalogRetry()
        debouncer.cancel()
        lastSent = .nothing
        rpc.shutdown(clearActivity: true)
    }

    // MARK: - Now playing

    private func handlePlayerUpdate(state: PlayerState, info: TrackInfo?) {
        music.playerState = state
        var trackChanged = false
        if state != .stopped, let info {
            trackChanged = music.track.map { $0.identity != info.identity } ?? true
            if trackChanged { resetCatalog() }
            music.track = info
            requestMissingCatalog()
            backfillPositionIfNeeded(info)
        } else {
            music.track = nil
            resetCatalog()
        }

        let shouldDisplay = music.isDisplayable
        if shouldDisplay != displaying {
            setDisplaying(shouldDisplay)
        } else if displaying {
            // 一時停止したまま曲を替えた場合も「操作した」とみなし、
            // タイムアウト済みなら表示を復活させてタイマーを計り直す
            if trackChanged { pauseHide.cancel() }
            updatePauseTimer()
            pushActivity()
        }
        // 表示していない状態のままなら送るものはない
    }

    /// Apple Musicの通知には再生位置が含まれないため、通知発火時のみAppleScriptで補完
    /// （ポーリングなし）。一時停止時も「どこで止めたか」の表示に使うため取得する。
    /// 位置なしで先にpushしても、補完がデバウンス窓(0.8s)内に返れば送信は1回にまとまる。
    private func backfillPositionIfNeeded(_ info: TrackInfo) {
        guard info.positionSec == nil else { return }
        MusicAppleScript.playerPosition { [weak self] position in
            guard let self, let position,
                var current = self.music.track, current.identity == info.identity
            else { return }
            (current.positionSec, current.positionSampledAt) = (position, Date())
            self.music.track = current
            if self.displaying {
                self.pushActivity()
            }
        }
    }

    // MARK: - Catalog

    /// アートワークとリンクを持たない再生中の曲について照会を投げる。
    ///
    /// 通知のたびに呼ばれるので、`catalogRequestedFor` の番人が
    /// 「イベントの頻度」を「リクエストの頻度」に変えないようにしている。
    private func requestMissingCatalog() {
        guard music.running, music.catalog == nil,
            let track = music.track, !track.name.isEmpty
        else { return }
        // 失敗した照会が冷却期間中
        if let retryAt = music.catalogRetryAt, retryAt > Date() { return }
        guard music.catalogRequestedFor != track.identity else { return }

        music.catalogRequestedFor = track.identity
        music.catalogRetryAt = nil
        itunes.resolve(
            name: track.name, artist: track.artist, album: track.album
        ) { [weak self] outcome in
            self?.applyCatalog(target: track, outcome: outcome)
        }
    }

    /// 返ってきた照会結果を反映する。遅れて届いた答えは、その曲がまだ再生中のときだけ使う。
    private func applyCatalog(target: TrackInfo, outcome: CatalogOutcome) {
        guard let current = music.track, current.identity == target.identity else { return }
        switch outcome {
        case .found(let info):
            music.catalog = info
            music.catalogRetryAt = nil
            // 先にアートなしで送信済みなので、解決できてかつ表示中のときだけ再送する
            if displaying { pushActivity() }
        case .missing:
            // 曲の性質による確定した答え（ローカル取り込み等）。訊き直さない。
            music.catalog = nil
            music.catalogRetryAt = nil
        case .failed:
            // 答えではない。番人を外して冷却期間のあとに訊き直せるようにする。
            music.catalogRequestedFor = nil
            music.catalogRetryAt = Date().addingTimeInterval(Self.catalogRetryDelay)
            scheduleCatalogRetry()
        }
    }

    private func scheduleCatalogRetry() {
        catalogRetryWork?.cancel()
        let work = DispatchWorkItem { [weak self] in
            guard let self else { return }
            self.catalogRetryWork = nil
            self.music.catalogRetryAt = nil
            self.requestMissingCatalog()
        }
        catalogRetryWork = work
        DispatchQueue.main.asyncAfter(deadline: .now() + Self.catalogRetryDelay, execute: work)
    }

    private func cancelCatalogRetry() {
        catalogRetryWork?.cancel()
        catalogRetryWork = nil
    }

    /// カタログ状態を白紙に戻す。予約済みの再試行も一緒に落とす。
    private func resetCatalog() {
        music.clearCatalog()
        cancelCatalogRetry()
    }

    // MARK: - Activity

    private func setDisplaying(_ newValue: Bool) {
        displaying = newValue
        debouncer.cancel()
        pauseHide.cancel()
        updatePauseTimer()
        pushActivity()
    }

    private func pushActivity() {
        guard music.running else { return }
        let activity = buildActivity()
        debouncer.schedule { [weak self] in self?.flushActivity(activity) }
    }

    private func buildActivity() -> [String: Any]? {
        guard displaying, let track = music.track, shouldShowActivity(music.playerState)
        else { return nil }
        return ActivityBuilder.build(
            track: track, playerState: music.playerState,
            catalog: music.catalog, settings: settings
        )
    }

    /// デバウンス窓が明けた実際の送信。
    ///
    /// 未接続なら捨てる: 接続できたら新しく組み立て直したものが送られるので、そちらのほうが新しい。
    ///
    /// `setActivity`はソケット用キューに渡すだけで成否を返さないため、`lastSent`は
    /// 「Discordに届いた」ではなく「渡した」時点で更新される。それでも届かなかった内容が
    /// その後の送信を抑止し続けないのは、書き込みに失敗したソケットが`teardown`から
    /// `.disconnected`を通り、`handleRPCState`がそこで`lastSent`を捨てるため——
    /// つまり抑止が生き残るのは接続が生きている間だけ、という不変条件で担保している。
    /// （Windows版は同期送信なので戻り値を見て更新する。行き着く先は同じ）
    private func flushActivity(_ activity: [String: Any]?) {
        guard rpc.state == .connected else { return }
        guard !lastSent.isEquivalent(to: activity) else { return }
        rpc.setActivity(activity)
        lastSent = .sent(activity)
    }

    private func shouldShowActivity(_ playerState: PlayerState) -> Bool {
        switch playerState {
        case .playing: return true
        case .paused: return settings.pauseHideMinutes != 0 && !pauseHide.hasFired
        case .stopped: return false
        }
    }

    // MARK: - Pause timeout

    /// 一時停止が続いているなら消すまでのカウントを始める。
    /// 一時停止でなくなったら、計測も「消した」という事実も取り消す。
    private func updatePauseTimer() {
        guard displaying, music.playerState == .paused else {
            pauseHide.cancel()
            return
        }
        pauseHide.arm(minutes: settings.pauseHideMinutes) { [weak self] in
            guard let self, self.music.playerState == .paused else { return }
            self.pushActivity()
        }
    }

    /// 設定が動いたので、いま出ているべきものを組み立て直す。
    func settingsChanged() {
        // 一時停止タイムアウトの設定変更を反映するため、タイマーを計り直す
        pauseHide.cancel()
        updatePauseTimer()
        pushActivity()
    }

    // MARK: - Menu

    private func menuInput() -> MenuModel.Input {
        MenuModel.Input(
            status: StatusLinesBuilder.Input(
                music: music,
                musicNotAuthorized: MusicAppleScript.notAuthorized,
                discordState: rpc.state,
                language: settings.language
            ),
            settings: settings,
            launchAtLogin: LaunchAtLogin.isEnabled
        )
    }

    func checkForUpdates() {
        updaterController.checkForUpdates(nil)
    }
}

// MARK: - Discord connection

extension AppDelegate {
    private func handleRPCState(_ state: DiscordRPCClient.ConnState) {
        switch state {
        case .connected:
            reconnect.reset()
            // つながったばかりの接続には何も表示されていない。前回の送信を覚えたままだと
            // 1発目が「前と同じ」と判定されて、何も出ないまま終わる。
            lastSent = .nothing
            pushActivity()
        case .disconnected:
            lastSent = .nothing
            if reconnect.takeImmediate() {
                attemptConnect()
            } else if music.running {
                // プレイヤーが動いているときだけ再試行する（常時接続しない）
                reconnect.schedule { [weak self] in self?.attemptConnect() }
            }
        case .connecting:
            break
        }
    }

    /// メニューの「Reconnect to Discord」。
    ///
    /// つながっていても一度切ってから繋ぎ直す。「Reconnect」と書かれたボタンは
    /// 見た目に問題がないときこそ押されるもので、そこで何も起きないのが一番困る。
    func reconnectNow() {
        reconnect.reset()
        guard rpc.state != .disconnected else {
            attemptConnect()
            return
        }
        reconnect.expectImmediateReconnect()
        rpc.shutdown(clearActivity: true)
    }

    func attemptConnect() {
        guard music.running, rpc.state == .disconnected else { return }
        rpc.connect(clientId: SettingsStore.discordClientId)
    }

    func cancelReconnect() {
        reconnect.reset()
    }
}
