import AppKit

/// Apple Music / Discord の起動・終了を監視する唯一のトリガー。
/// これ以外のタイミングで他コンポーネントを動かさないことで、未使用時のリソース消費をゼロに保つ。
final class AppLifecycleWatcher {
    static let discordBundleIds: Set<String> = [
        "com.hnc.Discord",
        "com.hnc.DiscordPTB",
        "com.hnc.DiscordCanary",
    ]

    var onPlayerLaunch: (() -> Void)?
    var onPlayerTerminate: (() -> Void)?
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
                if bundleId == AppleMusic.bundleId {
                    self?.onPlayerLaunch?()
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
                guard Self.bundleId(from: note) == AppleMusic.bundleId else { return }
                self?.onPlayerTerminate?()
            }
        )

        // 本アプリより先にプレイヤーが起動しているケース
        if Self.isRunning(bundleId: AppleMusic.bundleId) {
            onPlayerLaunch?()
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
