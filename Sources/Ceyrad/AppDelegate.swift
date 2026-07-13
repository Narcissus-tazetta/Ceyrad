import AppKit
import Sparkle

final class AppDelegate: NSObject, NSApplicationDelegate {
    private let settings = SettingsStore.shared
    private let lifecycle = AppLifecycleWatcher()
    private let appleMusicObserver = PlayerNotificationObserver(
        notificationName: MusicSourceDescriptor.appleMusic.notificationName,
        parse: MusicSourceDescriptor.appleMusic.parse
    )
    private let spotifyObserver = PlayerNotificationObserver(
        notificationName: MusicSourceDescriptor.spotify.notificationName,
        parse: MusicSourceDescriptor.spotify.parse
    )
    private let rpc = DiscordRPCClient()
    private let itunes = ITunesSearchClient()
    private let spotifyCatalog = SpotifyCatalogClient()
    private let debouncer = Debouncer(delay: 0.8)
    private var menuBar: MenuBarController!
    private var updaterController: SPUStandardUpdaterController!

    private var sources = SourceStates()
    /// 現在Discordに表示しているソース。nil = 表示なし。
    private var activeSource: MusicSourceID?
    /// 直近のconnectで使ったclient ID。ソース切替時の再ハンドシェイク要否の判定に使う。
    private var connectedClientId: String?
    /// ソース切替のための意図的な切断中。切断完了後にバックオフを挟まず即再接続する。
    private var pendingClientSwitch = false

    private var reconnectAttempt = 0
    private var reconnectWork: DispatchWorkItem?

    // 一時停止が設定分数続いたらステータスを消すためのタイマーとフラグ
    private var pauseHideWork: DispatchWorkItem?
    private var pausedTimedOut = false

    // MARK: - Lifecycle

    func applicationDidFinishLaunching(_: Notification) {
        updaterController = SPUStandardUpdaterController(
            startingUpdater: true, updaterDelegate: nil, userDriverDelegate: nil
        )

        menuBar = MenuBarController()
        menuBar.statusLines = { [weak self] in self?.statusLines() ?? [] }
        menuBar.onSettingsChanged = { [weak self] in self?.settingsChanged() }
        menuBar.onReconnectRequested = { [weak self] in
            self?.cancelReconnect()
            self?.attemptConnect()
        }
        menuBar.onCheckForUpdates = { [weak self] in
            self?.updaterController.checkForUpdates(nil)
        }

        appleMusicObserver.onUpdate = { [weak self] state, info in
            self?.handlePlayerUpdate(source: .appleMusic, state: state, info: info)
        }
        spotifyObserver.onUpdate = { [weak self] state, info in
            self?.handlePlayerUpdate(source: .spotify, state: state, info: info)
        }
        rpc.onStateChange = { [weak self] state in self?.handleRPCState(state) }

        lifecycle.onPlayerLaunch = { [weak self] source in self?.sourceLaunched(source) }
        lifecycle.onPlayerTerminate = { [weak self] source in self?.sourceTerminated(source) }
        lifecycle.onDiscordLaunch = { [weak self] in
            // Discordが後から起動したケース: バックオフを待たず即接続
            guard let self, self.sources.anyRunning else { return }
            self.cancelReconnect()
            self.attemptConnect()
        }
        lifecycle.start()
    }

    func applicationWillTerminate(_: Notification) {
        // asyncだとプロセス終了までに送信が走らないことがあるため、終了時だけ同期で行う
        rpc.shutdownSync(clearActivity: true)
    }

    private func observer(for source: MusicSourceID) -> PlayerNotificationObserver {
        source == .appleMusic ? appleMusicObserver : spotifyObserver
    }

    // MARK: - Player lifecycle

    private func sourceLaunched(_ source: MusicSourceID) {
        guard settings.isSourceEnabled(source), !sources[source].running else { return }
        sources[source].running = true
        observer(for: source).start()
        attemptConnect()
        // 起動直後はスクリプティングに応答しないことがあるため少し待ってから初期状態を取得。
        // 通知は状態変化時にしか飛ばないため、既に再生中だった場合はこの1回が必要。
        scheduleInitialFetch(source: source, attempt: 0)
    }

    /// 初期状態の取得。オートメーション権限のプロンプト待ちや、プレイヤー起動直後の
    /// スクリプティング無応答で失敗することがあるため、バックオフ付きでリトライする
    /// （2s→4s→8s→16s→32s、計約1分で打ち切り）。
    private func scheduleInitialFetch(source: MusicSourceID, attempt: Int) {
        let delay = 2.0 * pow(2.0, Double(attempt))
        DispatchQueue.main.asyncAfter(deadline: .now() + delay) { [weak self] in
            guard let self, self.sources[source].running, self.sources[source].track == nil
            else { return }
            let fetch =
                source == .appleMusic
                ? MusicAppleScript.currentState : SpotifyAppleScript.currentState
            fetch { [weak self] result in
                guard let self, self.sources[source].running, self.sources[source].track == nil
                else { return }
                if let (state, info) = result {
                    self.handlePlayerUpdate(source: source, state: state, info: info)
                } else if attempt < 4 {
                    self.scheduleInitialFetch(source: source, attempt: attempt + 1)
                }
            }
        }
    }

    private func sourceTerminated(_ source: MusicSourceID) {
        guard sources[source].running else { return }
        observer(for: source).stop()
        sources[source] = SourceState()
        guard sources.anyRunning else {
            // 最後のプレイヤーが終了: 全て止めて完全休止に戻す（常時接続しない）
            activeSource = nil
            connectedClientId = nil
            pendingClientSwitch = false
            cancelReconnect()
            cancelPauseTimer()
            debouncer.cancel()
            rpc.shutdown(clearActivity: true)
            return
        }
        let selection = SourceSelector.selectActiveSource(
            appleMusic: sources.appleMusic, spotify: sources.spotify, current: activeSource
        )
        if selection != activeSource {
            switchActiveSource(to: selection)
        }
    }

    // MARK: - Now playing

    private func handlePlayerUpdate(source: MusicSourceID, state: PlayerState, info: TrackInfo?) {
        sources[source].lastEventUptimeNs = DispatchTime.now().uptimeNanoseconds
        sources[source].playerState = state
        var trackChanged = false
        if state != .stopped, let info {
            trackChanged = sources[source].track.map { $0.identity != info.identity } ?? true
            if trackChanged { sources[source].catalog = nil }
            sources[source].track = info
            if sources[source].catalog == nil {
                resolveCatalog(source: source, for: info)
            }
            backfillPositionIfNeeded(source: source, info: info)
        } else {
            sources[source].track = nil
            sources[source].catalog = nil
        }

        let selection = SourceSelector.selectActiveSource(
            appleMusic: sources.appleMusic, spotify: sources.spotify, current: activeSource
        )
        if selection != activeSource {
            switchActiveSource(to: selection)
        } else if source == activeSource {
            // 一時停止したまま曲を替えた場合も「操作した」とみなし、
            // タイムアウト済みなら表示を復活させてタイマーを計り直す
            if trackChanged { cancelPauseTimer() }
            updatePauseTimer()
            pushActivity()
        }
        // 非アクティブソースのイベントで選択が変わらない場合は何もしない（送信も発生しない）
    }

    /// Apple Musicの通知には再生位置が含まれないため、通知発火時のみAppleScriptで補完
    /// （ポーリングなし）。Spotifyは通知に位置が入るため補完不要。
    /// 一時停止時も「どこで止めたか」の表示に使うため取得する。
    /// 位置なしで先にpushしても、補完がデバウンス窓(0.8s)内に返れば送信は1回にまとまる。
    private func backfillPositionIfNeeded(source: MusicSourceID, info: TrackInfo) {
        guard source == .appleMusic, info.positionSec == nil else { return }
        MusicAppleScript.playerPosition { [weak self] position in
            guard let self, let position,
                var current = self.sources.appleMusic.track, current.identity == info.identity
            else { return }
            current.positionSec = position
            self.sources.appleMusic.track = current
            if self.activeSource == .appleMusic {
                self.pushActivity()
            }
        }
    }

    private func resolveCatalog(source: MusicSourceID, for target: TrackInfo) {
        switch source {
        case .appleMusic:
            guard !target.name.isEmpty else { return }
            itunes.resolve(
                name: target.name, artist: target.artist, album: target.album
            ) { [weak self] result in
                self?.applyCatalog(source: source, target: target, result: result)
            }
        case .spotify:
            guard let trackId = target.trackId else { return }
            spotifyCatalog.resolve(trackId: trackId) { [weak self] result in
                self?.applyCatalog(source: source, target: target, result: result)
            }
        }
    }

    private func applyCatalog(source: MusicSourceID, target: TrackInfo, result: CatalogInfo?) {
        guard let current = sources[source].track, current.identity == target.identity
        else { return }
        sources[source].catalog = result
        // 先にアートなしで送信済みなので、解決できてかつ表示中のソースのときだけ再送する
        if result != nil, activeSource == source {
            pushActivity()
        }
    }

    // MARK: - Active source

    /// 表示ソースの切替。旧ソース向けのペンディング送信を破棄し、
    /// client IDが変わる場合はDiscordと再ハンドシェイクする。
    private func switchActiveSource(to newSource: MusicSourceID?) {
        activeSource = newSource
        debouncer.cancel()
        cancelPauseTimer()
        updatePauseTimer()
        guard let newSource else {
            // 稼働中のプレイヤーは残っているが表示するものがない: 接続は維持して表示だけ消す
            pushActivity()
            return
        }
        let clientId = MusicSourceDescriptor.descriptor(for: newSource).discordClientId
        if let connectedClientId, connectedClientId != clientId, rpc.state != .disconnected {
            // 「Listening to <アプリ名>」はApplication名で決まるため、client IDを替えて接続し直す。
            // 切断完了後、handleRPCStateがバックオフなしで即再接続する。
            pendingClientSwitch = true
            cancelReconnect()
            rpc.shutdown(clearActivity: true)
        } else {
            pushActivity()
        }
    }

    // MARK: - Activity

    private func pushActivity() {
        guard sources.anyRunning else { return }
        let activity: [String: Any]?
        if let source = activeSource, let track = sources[source].track,
            shouldShowActivity(sources[source].playerState)
        {
            activity = ActivityBuilder.build(
                track: track, playerState: sources[source].playerState,
                catalog: sources[source].catalog, settings: settings, source: source
            )
        } else {
            activity = nil
        }
        debouncer.schedule { [weak self] in
            self?.rpc.setActivity(activity)
        }
    }

    private func shouldShowActivity(_ playerState: PlayerState) -> Bool {
        switch playerState {
        case .playing: return true
        case .paused: return settings.pauseHideMinutes != 0 && !pausedTimedOut
        case .stopped: return false
        }
    }

    // MARK: - Pause timeout

    /// 一時停止が設定分数続いたらステータスを消す。再生再開・停止・曲操作・ソース切替でリセットされる。
    private func updatePauseTimer() {
        guard let source = activeSource, sources[source].playerState == .paused else {
            cancelPauseTimer()
            return
        }
        // すでにカウント中（またはタイムアウト済み）なら計り直さない
        guard pauseHideWork == nil, !pausedTimedOut else { return }
        let minutes = settings.pauseHideMinutes
        guard minutes > 0 else { return }  // 0=即時 / -1=消さない はタイマー不要
        let work = DispatchWorkItem { [weak self] in
            guard let self, let source = self.activeSource,
                self.sources[source].playerState == .paused
            else { return }
            self.pauseHideWork = nil
            self.pausedTimedOut = true
            self.pushActivity()
        }
        pauseHideWork = work
        DispatchQueue.main.asyncAfter(deadline: .now() + Double(minutes) * 60, execute: work)
    }

    private func cancelPauseTimer() {
        pauseHideWork?.cancel()
        pauseHideWork = nil
        pausedTimedOut = false
    }

    private func settingsChanged() {
        // ソースの有効/無効の切替を反映（無効化=終了扱い、有効化=起動中なら起動扱い）
        for source in MusicSourceID.allCases {
            let enabled = settings.isSourceEnabled(source)
            if !enabled, sources[source].running {
                sourceTerminated(source)
            } else if enabled, !sources[source].running,
                AppLifecycleWatcher.isRunning(
                    bundleId: MusicSourceDescriptor.descriptor(for: source).bundleId)
            {
                sourceLaunched(source)
            }
        }
        // 一時停止タイムアウトの設定変更を反映するため、タイマーを計り直す
        cancelPauseTimer()
        updatePauseTimer()
        pushActivity()
    }
}

// MARK: - Discord connection

extension AppDelegate {
    private func handleRPCState(_ state: DiscordRPCClient.ConnState) {
        switch state {
        case .connected:
            reconnectAttempt = 0
            // 接続処理中にソース切替が重なった場合の自己修復:
            // つながったclient IDがアクティブソースと違っていたら接続し直す
            if let source = activeSource,
                MusicSourceDescriptor.descriptor(for: source).discordClientId != connectedClientId
            {
                pendingClientSwitch = true
                rpc.shutdown(clearActivity: true)
            } else {
                pushActivity()
            }
        case .disconnected:
            connectedClientId = nil
            if pendingClientSwitch {
                // ソース切替のための意図的な切断: バックオフを挟まず新client IDで即接続
                pendingClientSwitch = false
                attemptConnect()
            } else {
                scheduleReconnect()
            }
        case .connecting:
            break
        }
    }

    private func attemptConnect() {
        guard sources.anyRunning, rpc.state == .disconnected else { return }
        // 呼び出し時点のアクティブソースからclient IDを導出する
        // （バックオフ経由の再接続でも自動的に正しいIDになる）
        let fallback: MusicSourceID = sources.appleMusic.running ? .appleMusic : .spotify
        let clientId = MusicSourceDescriptor.descriptor(for: activeSource ?? fallback)
            .discordClientId
        connectedClientId = clientId
        rpc.connect(clientId: clientId)
    }

    /// Discord未起動・再起動中に備えた指数バックオフ（1s→2s→…→上限60s）。
    /// プレイヤー稼働中のみ動き、全プレイヤー終了で完全停止する。
    private func scheduleReconnect() {
        guard sources.anyRunning else { return }
        reconnectWork?.cancel()
        let delay = min(60.0, pow(2.0, Double(reconnectAttempt)))
        reconnectAttempt = min(reconnectAttempt + 1, 6)
        let work = DispatchWorkItem { [weak self] in self?.attemptConnect() }
        reconnectWork = work
        DispatchQueue.main.asyncAfter(deadline: .now() + delay, execute: work)
    }

    private func cancelReconnect() {
        reconnectWork?.cancel()
        reconnectWork = nil
        reconnectAttempt = 0
    }

    // MARK: - Menu status

    private func statusLines() -> [String] {
        StatusLinesBuilder.lines(
            StatusLinesBuilder.Input(
                appleMusic: sources.appleMusic,
                spotify: sources.spotify,
                activeSource: activeSource,
                appleMusicEnabled: settings.isSourceEnabled(.appleMusic),
                spotifyEnabled: settings.isSourceEnabled(.spotify),
                appleMusicNotAuthorized: MusicAppleScript.notAuthorized,
                spotifyNotAuthorized: SpotifyAppleScript.notAuthorized,
                discordState: rpc.state
            )
        )
    }
}
