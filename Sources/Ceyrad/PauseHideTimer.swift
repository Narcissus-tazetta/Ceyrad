import Foundation

/// 一時停止が設定分数続いたらステータスを消すためのタイマー。
///
/// 「計測中か」「もう消したか」の2つの状態がフラグとして散らばると、片方だけ戻し忘れて
/// 表示が復活しなくなる。ひとまとめにして`cancel()`が唯一の巻き戻し口になるようにする。
/// メインスレッド専用。
final class PauseHideTimer {
    /// 一時停止が続いたので表示を消した状態か。
    private(set) var hasFired = false
    private var work: DispatchWorkItem?

    /// 一時停止中なら計り始める。
    ///
    /// すでに計測中、または発火済みなら計り直さない——「一時停止のまま何かが起きた」たびに
    /// 時計が振り出しに戻ると、いつまでも消えないことになる。
    /// `minutes` が 0（即時に消す）と -1（消さない）はタイマー不要なので何もしない。
    func arm(minutes: Int, onFire: @escaping () -> Void) {
        guard work == nil, !hasFired, minutes > 0 else { return }
        let item = DispatchWorkItem { [weak self] in
            guard let self else { return }
            self.work = nil
            self.hasFired = true
            onFire()
        }
        work = item
        DispatchQueue.main.asyncAfter(deadline: .now() + Double(minutes) * 60, execute: item)
    }

    /// 計測も「消した」という事実も取り消す。再生再開・曲操作・表示切替で呼ばれる。
    func cancel() {
        work?.cancel()
        work = nil
        hasFired = false
    }
}
