import Foundation

enum LinkType: String, CaseIterable {
    case song
    case artist
    case album
    case custom
    case repository
    case disabled

    var displayName: String {
        switch self {
        case .song: return t("Song Page", "曲ページ")
        case .artist: return t("Artist Page", "アーティストページ")
        case .album: return t("Album Page", "アルバムページ")
        case .custom: return t("Custom URL", "カスタムURL")
        case .repository: return t("Repository", "リポジトリ")
        case .disabled: return t("Off", "オフ")
        }
    }

    /// メニューに表示するリンク先の選択肢。
    static var selectableCases: [LinkType] {
        [.song, .artist, .album, .custom, .repository, .disabled]
    }

    /// リンク先に応じたボタンラベルの既定値。ユーザーが手動でラベルを変更するまではこれに追従する。
    var defaultLabel: String {
        switch self {
        case .song: return "Play on Apple Music"
        case .artist: return "View Artist"
        case .album: return "View Album"
        case .custom: return "Open Link"
        case .repository: return "About This App"
        case .disabled: return ""
        }
    }
}

final class SettingsStore {
    static let shared = SettingsStore()
    static let defaultRepositoryURL = "https://github.com/Narcissus-tazetta/Cadence"
    /// このアプリ専用のDiscord Application ID。ユーザーが変更する必要はないため固定値とする。
    static let discordClientId = "1525381518258606130"

    private let defaults: UserDefaults

    /// 通常は `shared` を使う。テストでは専用のUserDefaultsを注入する。
    init(defaults: UserDefaults = .standard) {
        self.defaults = defaults
    }

    var button1Type: LinkType {
        get { LinkType(rawValue: defaults.string(forKey: "button1Type") ?? "") ?? .song }
        set { setType(newValue, typeKey: "button1Type", labelKey: "button1Label", old: button1Type) }
    }

    var button1Label: String {
        get { defaults.string(forKey: "button1Label") ?? button1Type.defaultLabel }
        set { setLabel(newValue, labelKey: "button1Label", type: button1Type) }
    }

    var button2Type: LinkType {
        get { LinkType(rawValue: defaults.string(forKey: "button2Type") ?? "") ?? .repository }
        set { setType(newValue, typeKey: "button2Type", labelKey: "button2Label", old: button2Type) }
    }

    var button2Label: String {
        get { defaults.string(forKey: "button2Label") ?? button2Type.defaultLabel }
        set { setLabel(newValue, labelKey: "button2Label", type: button2Type) }
    }

    /// ラベルが未カスタマイズ（＝旧リンク先の既定値のまま）なら、リンク先変更時にラベルも追従させる
    private func setType(_ newValue: LinkType, typeKey: String, labelKey: String, old: LinkType) {
        if defaults.string(forKey: labelKey) == old.defaultLabel {
            defaults.removeObject(forKey: labelKey)
        }
        defaults.set(newValue.rawValue, forKey: typeKey)
    }

    /// 既定値と同じ・空文字ならカスタムラベル扱いにせず削除し、以後もリンク先に追従させる
    private func setLabel(_ newValue: String, labelKey: String, type: LinkType) {
        let value = String(newValue.prefix(32))
        if value.isEmpty || value == type.defaultLabel {
            defaults.removeObject(forKey: labelKey)
        } else {
            defaults.set(value, forKey: labelKey)
        }
    }

    var customURL: String {
        get { defaults.string(forKey: "customURL") ?? "" }
        set { defaults.set(newValue, forKey: "customURL") }
    }

    var repositoryURL: String {
        get { defaults.string(forKey: "repositoryURL") ?? Self.defaultRepositoryURL }
        set { defaults.set(newValue, forKey: "repositoryURL") }
    }

    /// 一時停止が続いたときにステータスを消すまでの分数。
    /// 0 = 即時に消す、-1 = 消さない（表示継続）、それ以外 = その分数後に消す。
    static let pauseHideChoices = [-1, 0, 1, 3, 5, 10]

    var pauseHideMinutes: Int {
        get { defaults.object(forKey: "pauseHideMinutes") as? Int ?? 5 }
        set { defaults.set(newValue, forKey: "pauseHideMinutes") }
    }

    /// 設定画面（メニュー）の表示言語。Discord関連の文言には影響しない。
    var language: AppLanguage {
        get { AppLanguage(rawValue: defaults.string(forKey: "appLanguage") ?? "") ?? .en }
        set { defaults.set(newValue.rawValue, forKey: "appLanguage") }
    }
}
