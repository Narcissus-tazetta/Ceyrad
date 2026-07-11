import AppKit

/// Apple Music / Discord の起動・終了を監視する唯一のトリガー。
/// これ以外のタイミングで他コンポーネントを動かさないことで、未使用時のリソース消費をゼロに保つ。
final class AppLifecycleWatcher {
    static let musicBundleId = "com.apple.Music"
    static let discordBundleIds: Set<String> = [
        "com.hnc.Discord",
        "com.hnc.DiscordPTB",
        "com.hnc.DiscordCanary",
    ]

    var onMusicLaunch: (() -> Void)?
    var onMusicTerminate: (() -> Void)?
    var onDiscordLaunch: (() -> Void)?

    private var observers: [NSObjectProtocol] = []

    func start() {
        let center = NSWorkspace.shared.notificationCenter
        observers.append(
            center.addObserver(
                forName: NSWorkspace.didLaunchApplicationNotification,
                object: nil, queue: .main
            ) { [weak self] note in
                guard let bundleId = Self.bundleId(from: note) else { return }
                if bundleId == Self.musicBundleId {
                    self?.onMusicLaunch?()
                } else if Self.discordBundleIds.contains(bundleId) {
                    self?.onDiscordLaunch?()
                }
            }
        )
        observers.append(
            center.addObserver(
                forName: NSWorkspace.didTerminateApplicationNotification,
                object: nil, queue: .main
            ) { [weak self] note in
                guard Self.bundleId(from: note) == Self.musicBundleId else { return }
                self?.onMusicTerminate?()
            }
        )

        // 本アプリより先にMusicが起動しているケース
        if Self.isRunning(bundleId: Self.musicBundleId) {
            onMusicLaunch?()
        }
    }

    static func isRunning(bundleId: String) -> Bool {
        NSWorkspace.shared.runningApplications.contains { $0.bundleIdentifier == bundleId }
    }

    private static func bundleId(from note: Notification) -> String? {
        let app = note.userInfo?[NSWorkspace.applicationUserInfoKey] as? NSRunningApplication
        return app?.bundleIdentifier
    }
}
