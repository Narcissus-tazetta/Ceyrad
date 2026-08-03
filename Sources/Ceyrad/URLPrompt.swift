import Foundation

/// URLになるまで訊き直す。
///
/// エラーを出して諦めると入力した文字列が消えてしまう。打ち間違いは打ち直すより
/// 直すほうが楽なので、入力を残したまま訊き直す。ループをここに——ダイアログを
/// 2つのクロージャとして受け取る制御構造として——置くことで、ダイアログを1度も
/// 出さずにテストできる。
enum URLPrompt {
    /// 使える答えが得られるかユーザーが諦めるまで、入力→検証→再入力を繰り返す。
    /// `nil` はキャンセルを表す。
    ///
    /// `allowEmpty` は2つの挙動を分ける: カスタムURLは空文字が「設定なし」を意味するが、
    /// リポジトリURLに意味のある空状態はない。
    static func promptValidURL(
        showPrompt: (String) -> String?,
        showError: () -> Void,
        initial: String,
        allowEmpty: Bool
    ) -> String? {
        var current = initial
        while true {
            guard let entered = showPrompt(current) else { return nil }
            if (entered.isEmpty && allowEmpty) || ActivityBuilder.isValidButtonURL(entered) {
                return entered
            }
            showError()
            // 捨てずに提示し直す
            current = entered
        }
    }
}
