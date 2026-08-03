import Foundation

/// 文字を割らずに長さで切る。
///
/// Swiftの`prefix`は書記素クラスタ単位なので絶対に文字を割らないが、数える単位が
/// Windows版（Unicodeスカラー単位）と違う。家族絵文字1個は7スカラーなので、
/// 「32文字」の解釈が両者で7倍ずれ、同じ設定から違う長さのラベルがDiscordに出る。
/// Discordの上限はスカラー寄りなので、こちらをWindows版に合わせる。
///
/// アルゴリズムは`windows/src/core/text.rs`の`truncate`と一対一に対応させてある。
/// 片方を触るときはもう片方も。共有ベクタ（spec/activity_vectors.json）が
/// 両者の一致をCIで押さえている。
enum Text {
    private static let zwj: Unicode.Scalar = "\u{200D}"

    /// そのスカラーが「直前の文字の一部としてしか存在しない」ものか。
    ///
    /// 本物の`Grapheme_Extend`プロパティ（数千レンジ）ではなく手書きの表なのは
    /// Windows版と同じ理由——曲名とボタンラベルに出るものは覆えるし、漏れても
    /// 「もともと切り詰められている文字列の末尾が1文字おかしく見える」だけで済む。
    ///
    /// 地域指示子は意図的に入れていない。国旗は指示子の**対**であり、
    /// 「完全な国旗の後半」と「壊れた国旗の前半」を1文字だけ見て区別する方法はない。
    /// 継続扱いにすると、ちょうど上限で終わる国旗を壊してしまう。
    private static func isContinuation(_ scalar: Unicode.Scalar) -> Bool {
        switch scalar.value {
        case 0x200D,  // zero-width joiner
            0xFE00...0xFE0F,  // variation selectors
            0x1F3FB...0x1F3FF,  // skin-tone modifiers
            0x0300...0x036F,  // combining diacritical marks
            0x1AB0...0x1AFF,  // …extended
            0x1DC0...0x1DFF,  // …supplement
            0x20D0...0x20F0,  // combining marks for symbols
            0xFE20...0xFE2F,  // combining half marks
            0xE0020...0xE007F,  // tag characters (flag sequences)
            0xE0100...0xE01EF:  // variation selectors supplement
            return true
        default:
            return false
        }
    }

    /// `s` を最大 `max` スカラーに切る。切り口は読み手が文字の切れ目と認めるところ。
    ///
    /// 上限に収まっていれば1文字も歩かずにそのまま返す。
    static func truncate(_ s: String, max: Int) -> String {
        let scalars = s.unicodeScalars
        guard
            let cut = scalars.index(scalars.startIndex, offsetBy: max, limitedBy: scalars.endIndex),
            cut != scalars.endIndex
        else { return s }

        var end = cut
        // 切り口が文字を割ったかどうかは「捨てた側」で決まる。上限にちょうど乗っている
        // 完全な文字はそのまま残す。次のものが同じ文字の一部だったときだけ、
        // 残った尻尾は意味を失うので、直前の完全な文字まで戻す。
        if isContinuation(scalars[cut]) {
            while end > scalars.startIndex {
                let previous = scalars.index(before: end)
                if !isContinuation(scalars[previous]) { break }
                end = previous
            }
        }

        // 結合子は何かの終わりには決してならない。繋いでいた相手は切り口の向こう側なので、
        // どちらにせよ落とす。
        while end > scalars.startIndex, scalars[scalars.index(before: end)] == zwj {
            end = scalars.index(before: end)
        }

        return String(String.UnicodeScalarView(scalars[scalars.startIndex..<end]))
    }

    /// スカラー数。長さ比較のためだけに文字列を歩き切らないよう、`limit`で打ち切れる。
    static func scalarCount(_ s: String, upTo limit: Int) -> Int {
        var count = 0
        for _ in s.unicodeScalars {
            count += 1
            if count >= limit { break }
        }
        return count
    }
}
