import Foundation

/// 設定画面（メニュー）の表示言語。
enum AppLanguage: String, CaseIterable {
    case en
    case ja

    /// 言語名は「その言語での呼び名」なので、現在の表示言語には依存しない。
    var displayName: String {
        switch self {
        case .en: return "English"
        case .ja: return "日本語"
        }
    }
}

/// 設定画面（メニュー）のUI文字列を指定言語で返す。
///
/// 言語をグローバルから読まず引数で受け取るのは、これを使う`displayName`等が
/// 暗黙のシングルトンに依存しないようにするため。おかげでメニュー全体を
/// 両言語でテストに固定できる。
/// Discordに関する文言（接続ステータス・再接続など）は対象外で常に英語固定。
func t(_ language: AppLanguage, _ en: String, _ ja: String) -> String {
    language == .ja ? ja : en
}
