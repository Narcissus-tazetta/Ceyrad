import Foundation

/// Discord未起動・再起動中に備えた指数バックオフ（1s→2s→…→上限60s）。
///
/// 試行回数と予約中の再試行はセットでしか意味をなさないので、まとめて持つ。
/// プレイヤー稼働中のみ動き、プレイヤー終了で`reset()`され完全停止する。
/// メインスレッド専用。
final class ReconnectBackoff {
    private static let maxDelay: TimeInterval = 60
    /// 2^6 = 64秒で上限に達するため、それ以上数えても意味がない。
    private static let maxAttempt = 6

    private var attempt = 0
    private var work: DispatchWorkItem?
    /// 意図的な切断中で、切断が完了したらバックオフを挟まず即つなぎ直す。
    private var immediate = false

    /// 次の再接続を予約する。予約済みのものは置き換える。
    func schedule(_ connect: @escaping () -> Void) {
        work?.cancel()
        let delay = min(Self.maxDelay, pow(2.0, Double(attempt)))
        attempt = min(attempt + 1, Self.maxAttempt)
        let item = DispatchWorkItem(block: connect)
        work = item
        DispatchQueue.main.asyncAfter(deadline: .now() + delay, execute: item)
    }

    /// 予約を取り消し、次の失敗を1回目として数え直す。
    func reset() {
        work?.cancel()
        work = nil
        attempt = 0
        immediate = false
    }

    /// 次の切断は自分から起こしたものなので、待たずにつなぎ直す。
    func expectImmediateReconnect() {
        immediate = true
    }

    /// 切断が通知されたときに呼ぶ。`true` なら待たずに繋ぎ直してよい。
    func takeImmediate() -> Bool {
        defer { immediate = false }
        return immediate
    }
}
