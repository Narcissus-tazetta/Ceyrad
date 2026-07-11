import Foundation

/// 設定画面（メニュー）の表示言語。
enum AppLanguage: String, CaseIterable {
    case en
    case ja

    var displayName: String {
        switch self {
        case .en: return "English"
        case .ja: return "日本語"
        }
    }
}

/// 設定画面（メニュー）のUI文字列を現在の言語で返す。
/// Discordに関する文言（接続ステータス・再接続など）は対象外で常に英語固定。
func t(_ en: String, _ ja: String) -> String {
    SettingsStore.shared.language == .ja ? ja : en
}
