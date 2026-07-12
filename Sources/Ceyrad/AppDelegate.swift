import AppKit
import Sparkle

final class AppDelegate: NSObject, NSApplicationDelegate {
    private let settings = SettingsStore.shared
    private let lifecycle = AppLifecycleWatcher()
    private let nowPlaying = NowPlayingObserver()
    private let rpc = DiscordRPCClient()
    private let itunes = ITunesSearchClient()
    private let debouncer = Debouncer(delay: 0.8)
    private var menuBar: MenuBarController!
    private var updaterController: SPUStandardUpdaterController!

    private var musicRunning = false
    private var playerState: PlayerState = .stopped
    private var track: TrackInfo?
    private var catalog: CatalogInfo?

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

        nowPlaying.onUpdate = { [weak self] state, info in
            self?.handlePlayerUpdate(state: state, info: info)
        }
        rpc.onStateChange = { [weak self] state in self?.handleRPCState(state) }

        lifecycle.onMusicLaunch = { [weak self] in self?.musicLaunched() }
        lifecycle.onMusicTerminate = { [weak self] in self?.musicTerminated() }
        lifecycle.onDiscordLaunch = { [weak self] in
            // Discordが後から起動したケース: バックオフを待たず即接続
            guard let self, self.musicRunning else { return }
            self.cancelReconnect()
            self.attemptConnect()
        }
        lifecycle.start()
    }

    func applicationWillTerminate(_: Notification) {
        // asyncだとプロセス終了までに送信が走らないことがあるため、終了時だけ同期で行う
        rpc.shutdownSync(clearActivity: true)
    }

    // MARK: - Apple Music lifecycle

    private func musicLaunched() {
        guard !musicRunning else { return }
        musicRunning = true
        nowPlaying.start()
        attemptConnect()
        // 起動直後はスクリプティングに応答しないことがあるため少し待ってから初期状態を取得。
        // 通知は状態変化時にしか飛ばないため、既に再生中だった場合はこの1回が必要。
        scheduleInitialFetch(attempt: 0)
    }

    /// 初期状態の取得。オートメーション権限のプロンプト待ちや、Music起動直後の
    /// スクリプティング無応答で失敗することがあるため、バックオフ付きでリトライする
    /// （2s→4s→8s→16s→32s、計約1分で打ち切り）。
    private func scheduleInitialFetch(attempt: Int) {
        let delay = 2.0 * pow(2.0, Double(attempt))
        DispatchQueue.main.asyncAfter(deadline: .now() + delay) { [weak self] in
            guard let self, self.musicRunning, self.track == nil else { return }
            MusicAppleScript.currentState { [weak self] result in
                guard let self, self.musicRunning, self.track == nil else { return }
                if let (state, info) = result {
                    self.handlePlayerUpdate(state: state, info: info)
                } else if attempt < 4 {
                    self.scheduleInitialFetch(attempt: attempt + 1)
                }
            }
        }
    }

    private func musicTerminated() {
        guard musicRunning else { return }
        musicRunning = false
        nowPlaying.stop()
        cancelReconnect()
        cancelPauseTimer()
        debouncer.cancel()
        playerState = .stopped
        track = nil
        catalog = nil
        // Activityをクリアしてから切断（常時接続しない）
        rpc.shutdown(clearActivity: true)
    }

    // MARK: - Now playing

    private func handlePlayerUpdate(state: PlayerState, info: TrackInfo?) {
        playerState = state
        updatePauseTimer()
        guard state != .stopped, let info else {
            track = nil
            catalog = nil
            pushActivity()
            return
        }
        let changed = track.map { $0.identity != info.identity } ?? true
        if changed {
            catalog = nil
            // 一時停止したまま曲を替えた場合も「操作した」とみなし、
            // タイムアウト済みなら表示を復活させてタイマーを計り直す
            cancelPauseTimer()
            updatePauseTimer()
        }
        track = info
        if catalog == nil {
            resolveCatalogIfNeeded(for: info)
        }
        // 通知には再生位置が含まれないため、通知発火時のみAppleScriptで補完（ポーリングなし）。
        // 一時停止時も「どこで止めたか」の表示に使うため取得する。
        // 位置なしで先にpushしても、補完がデバウンス窓(0.8s)内に返れば送信は1回にまとまる。
        if info.positionSec == nil {
            MusicAppleScript.playerPosition { [weak self] position in
                guard let self, let position, var current = self.track,
                    current.identity == info.identity
                else { return }
                current.positionSec = position
                self.track = current
                self.pushActivity()
            }
        }
        pushActivity()
    }

    private func resolveCatalogIfNeeded(for target: TrackInfo) {
        guard !target.name.isEmpty else { return }
        itunes.resolve(name: target.name, artist: target.artist, album: target.album) { [weak self] result in
            guard let self, let current = self.track,
                current.identity == target.identity
            else { return }
            self.catalog = result
            // 先にアートなしで送信済みなので、解決できたときだけ再送する
            if result != nil {
                self.pushActivity()
            }
        }
    }

    // MARK: - Activity

    private func pushActivity() {
        guard musicRunning else { return }
        let activity: [String: Any]?
        if let track, shouldShowActivity {
            activity = ActivityBuilder.build(
                track: track, playerState: playerState,
                catalog: catalog, settings: settings
            )
        } else {
            activity = nil
        }
        debouncer.schedule { [weak self] in
            self?.rpc.setActivity(activity)
        }
    }

    private var shouldShowActivity: Bool {
        switch playerState {
        case .playing: return true
        case .paused: return settings.pauseHideMinutes != 0 && !pausedTimedOut
        case .stopped: return false
        }
    }

    // MARK: - Pause timeout

    /// 一時停止が設定分数続いたらステータスを消す。再生再開・停止・曲操作でリセットされる。
    private func updatePauseTimer() {
        guard playerState == .paused else {
            cancelPauseTimer()
            return
        }
        // すでにカウント中（またはタイムアウト済み）なら計り直さない
        guard pauseHideWork == nil, !pausedTimedOut else { return }
        let minutes = settings.pauseHideMinutes
        guard minutes > 0 else { return }  // 0=即時 / -1=消さない はタイマー不要
        let work = DispatchWorkItem { [weak self] in
            guard let self, self.playerState == .paused else { return }
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
        // 一時停止タイムアウトの設定変更を反映するため、タイマーを計り直す
        cancelPauseTimer()
        updatePauseTimer()
        pushActivity()
    }

    // MARK: - Discord connection

    private func handleRPCState(_ state: DiscordRPCClient.ConnState) {
        switch state {
        case .connected:
            reconnectAttempt = 0
            pushActivity()
        case .disconnected:
            scheduleReconnect()
        case .connecting:
            break
        }
    }

    private func attemptConnect() {
        guard musicRunning, rpc.state == .disconnected else { return }
        rpc.connect(clientId: SettingsStore.discordClientId)
    }

    /// Discord未起動・再起動中に備えた指数バックオフ（1s→2s→…→上限60s）。
    /// Apple Music稼働中のみ動き、Music終了で完全停止する。
    private func scheduleReconnect() {
        guard musicRunning else { return }
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
        var lines: [String] = []
        if !musicRunning {
            lines.append(t("Apple Music: Not Running", "Apple Music: 未起動"))
        } else {
            switch playerState {
            case .stopped:
                lines.append(t("Apple Music: Stopped", "Apple Music: 停止中"))
            case .playing:
                lines.append("♪ \(trackLine())")
            case .paused:
                lines.append("⏸ \(trackLine())")
            }
        }
        if musicRunning, MusicAppleScript.notAuthorized {
            lines.append(t("Music Control: Not Authorized ⚠️", "ミュージックの操作: 未許可 ⚠️"))
            lines.append(
                t(
                    "(System Settings > Privacy > Automation)",
                    "(システム設定 > プライバシーとセキュリティ > オートメーション)"
                )
            )
        }
        if !musicRunning {
            lines.append("Discord: Idle (connects when Music starts)")
        } else {
            switch rpc.state {
            case .connected:
                lines.append("Discord: Connected")
            case .connecting:
                lines.append("Discord: Connecting…")
            case .disconnected:
                lines.append("Discord: Disconnected (retrying)")
            }
        }
        return lines
    }

    private func trackLine() -> String {
        guard let track else { return "–" }
        let artist = track.artist.isEmpty ? "" : " — \(track.artist)"
        return "\(track.name)\(artist)"
    }
}
