import Foundation

/// 曲送り連打などでDiscord RPCのレート制限（約20秒に5回）に当たりにくいよう更新をまとめる。
/// 完全な保証ではない（1〜3秒間隔の操作を続ければ超えうる）が、超過時はDiscord側で
/// 更新が遅延するだけで実害はない。
final class Debouncer {
    private let delay: TimeInterval
    private var pending: DispatchWorkItem?

    init(delay: TimeInterval) {
        self.delay = delay
    }

    func schedule(_ block: @escaping () -> Void) {
        pending?.cancel()
        let item = DispatchWorkItem(block: block)
        pending = item
        DispatchQueue.main.asyncAfter(deadline: .now() + delay, execute: item)
    }

    func cancel() {
        pending?.cancel()
        pending = nil
    }
}
