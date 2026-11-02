import AppKit
import Sparkle

final class AppDelegate: NSObject, NSApplicationDelegate {
    /// カタログ照会が失敗したあと、訊き直してよくなるまでの間隔。
    /// 少しオフラインだっただけで通知1回につき1リクエスト、とならない程度に長くとる。
    static let catalogRetryDelay: TimeInterval = 15
    /// YouTube Music でなくなったと見えてから、表示を消すまでの猶予。
    /// 曲が替わる瞬間はアルバム名が遅れて届くため、見えた瞬間に消すと点滅する。
    static let youtubeLossGrace: TimeInterval = 2.5

    let settings = SettingsStore.shared
    private let lifecycle = AppLifecycleWatcher()
    private let musicObserver = PlayerNotificationObserver(
        notificationName: AppleMusic.notificationName,
        parse: AppleMusic.parse
    )
    let rpc = DiscordRPCClient()
    let itunes = ITunesSearchClient()
    let debouncer = Debouncer(delay: 0.8)
    private var menuBar: MenuBarController!
    private var updaterController: SPUStandardUpdaterController!

    var appleMusic = MusicState()
    var youtube = MusicState()
    /// いまDiscordに何かを表示しているか。
    var displaying = false
    /// 表示しているソース。接続すべき client ID はこれで決まる。
    var displayedSource: MusicSource?
    /// いまの接続（または接続中）が使っている client ID のソース。
    var connectedSource: MusicSource?

    /// 直近でDiscordに送った内容。同じものの再送を抑えるために持つ。
    var lastSent: LastSent = .nothing

    let reconnect = ReconnectBackoff()
    let pauseHide = PauseHideTimer()
    var catalogRetryWork: [MusicSource: DispatchWorkItem] = [:]

    // YouTube Music（AppDelegate+YouTubeMusic.swift）
    let nowPlaying = NowPlayingAdapter()
    /// 検知が有効なのに使えない理由。`nil` なら使える（または無効）。
    var youtubeUnavailable: String?
    var latestPayload: NowPlayingPayload?
    /// 同じ曲のあいだ AppleScript を繰り返さないための、直近のタブ確認の結果。
    var youtubeTabVerdict: (identity: String, tabs: [BrowserTab])?
    var youtubeLossWork: DispatchWorkItem?

    /// この接続でDiscordに送った最後の内容。
    ///
    /// 「まだ何も送っていない」と「表示を消すよう送った」は別物で、混ぜると
    /// 再接続直後の1発目が「前と同じ」と誤判定されて何も表示されなくなる。
    enum LastSent {
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
            self?.handlePlayerUpdate(.appleMusic, state: state, info: info)
        }
        rpc.onStateChange = { [weak self] state in self?.handleRPCState(state) }

        lifecycle.onPlayerLaunch = { [weak self] in self?.playerLaunched() }
        lifecycle.onPlayerTerminate = { [weak self] in self?.playerTerminated() }
        lifecycle.onDiscordLaunch = { [weak self] in
            // Discordが後から起動したケース: バックオフを待たず即接続
            guard let self, self.connectionSource != nil else { return }
            self.cancelReconnect()
            self.attemptConnect()
        }
        configureYouTubeMusic()
        lifecycle.start()
    }

    func applicationWillTerminate(_: Notification) {
        nowPlaying.stop()
        // asyncだとプロセス終了までに送信が走らないことがあるため、終了時だけ同期で行う
        rpc.shutdownSync(clearActivity: true)
    }

    // MARK: - Source state

    func state(_ source: MusicSource) -> MusicState {
        switch source {
        case .appleMusic: return appleMusic
        case .youtubeMusic: return youtube
        }
    }

    func update(_ source: MusicSource, _ body: (inout MusicState) -> Void) {
        switch source {
        case .appleMusic: body(&appleMusic)
        case .youtubeMusic: body(&youtube)
        }
    }

    // MARK: - Apple Music lifecycle

    private func playerLaunched() {
        guard !appleMusic.running else { return }
        appleMusic.running = true
        musicObserver.start()
        syncConnection()
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
            guard let self, self.appleMusic.running, self.appleMusic.track == nil else { return }
            MusicAppleScript.currentState { [weak self] result in
                guard let self, self.appleMusic.running, self.appleMusic.track == nil else { return }
                if let (state, info) = result {
                    self.handlePlayerUpdate(.appleMusic, state: state, info: info)
                } else if attempt < 4 {
                    self.scheduleInitialFetch(attempt: attempt + 1)
                }
            }
        }
    }

    private func playerTerminated() {
        guard appleMusic.running else { return }
        musicObserver.stop()
        appleMusic = MusicState()
        cancelCatalogRetry(.appleMusic)
        reevaluate()
    }

    // MARK: - Now playing

    func handlePlayerUpdate(
        _ source: MusicSource, state newState: PlayerState, info: TrackInfo?,
        pageURL: String? = nil
    ) {
        var trackChanged = false
        var catalogReset = false
        update(source) { current in
            let wasPlaying = current.playerState == .playing
            current.playerState = newState
            if newState != .stopped, let info {
                trackChanged = current.track.map { $0.identity != info.identity } ?? true
                catalogReset = trackChanged
                if trackChanged { current.clearCatalog() }
                current.track = info
                current.pageURL = pageURL
            } else {
                current.track = nil
                current.pageURL = nil
                current.clearCatalog()
                catalogReset = true
            }
            if newState == .playing, !wasPlaying { current.playingSince = Date() }
        }
        if catalogReset { cancelCatalogRetry(source) }
        if let info, newState != .stopped {
            requestMissingCatalog(source)
            if source == .appleMusic { backfillPositionIfNeeded(info) }
        }
        reevaluate(trackChanged: trackChanged)
    }

    /// どのソースを出すか・どの client ID で繋ぐかを決め直す。状態が動いたら必ずここを通る。
    func reevaluate(trackChanged: Bool = false) {
        let active = SourceSelector.select(appleMusic: appleMusic, youtubeMusic: youtube)
        let sourceChanged = active != displayedSource
        displayedSource = active
        syncConnection()

        let shouldDisplay = active != nil
        if shouldDisplay != displaying || sourceChanged {
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
                var current = self.appleMusic.track, current.identity == info.identity
            else { return }
            (current.positionSec, current.positionSampledAt) = (position, Date())
            self.appleMusic.track = current
            if self.displayedSource == .appleMusic {
                self.pushActivity()
            }
        }
    }

    // MARK: - Activity

    private func setDisplaying(_ newValue: Bool) {
        displaying = newValue
        debouncer.cancel()
        pauseHide.cancel()
        updatePauseTimer()
        pushActivity()
    }

    func pushActivity() {
        guard connectionSource != nil else { return }
        let source = displayedSource
        let activity = buildActivity()
        debouncer.schedule { [weak self] in self?.flushActivity(activity, for: source) }
    }

    private func buildActivity() -> [String: Any]? {
        guard displaying, let source = displayedSource else { return nil }
        let current = state(source)
        guard let track = current.track, shouldShowActivity(current.playerState) else { return nil }
        var catalog = current.catalog
        if source == .youtubeMusic {
            // アーティスト/アルバムのページはApple Musicのものなので、曲ページ（タブのURL）と
            // アートワークだけを引き継ぐ。
            catalog = CatalogInfo(songURL: current.pageURL, artworkURL: current.catalog?.artworkURL)
        }
        return ActivityBuilder.build(
            track: track, playerState: current.playerState,
            catalog: catalog, settings: settings, source: source
        )
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
    func updatePauseTimer() {
        guard displaying, displayedSource.map({ state($0).playerState }) == .paused else {
            pauseHide.cancel()
            return
        }
        pauseHide.arm(minutes: settings.pauseHideMinutes) { [weak self] in
            guard let self, self.displayedSource.map({ self.state($0).playerState }) == .paused
            else { return }
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
                music: appleMusic,
                musicNotAuthorized: MusicAppleScript.notAuthorized,
                youtube: youtube,
                youtubeEnabled: settings.youtubeMusicEnabled,
                youtubeUnavailable: youtubeUnavailable,
                browserNotAuthorized: BrowserTabs.notAuthorized,
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
