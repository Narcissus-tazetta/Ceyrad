import Foundation

/// 設定可能なDiscordボタン2つのうちどちらか。
///
/// ボタン1と2の設定は完全に対称なので、番号を値として持ち回ることで
/// 「1用」「2用」の同じコードを2本書かずに済ませる。
enum ButtonSlot: Int, CaseIterable {
    case one = 1
    case two = 2

    var number: Int { rawValue }

    /// 未設定時のリンク先。曲ページとリポジトリという既定の組み合わせ。
    var defaultType: LinkType {
        switch self {
        case .one: return .song
        case .two: return .repository
        }
    }

    fileprivate var typeKey: String { "button\(rawValue)Type" }
    fileprivate var labelKey: String { "button\(rawValue)Label" }
}

enum LinkType: String, CaseIterable {
    case song
    case artist
    case album
    case custom
    case repository
    case disabled

    /// メニューに表示するリンク先の選択肢。
    static let selectableCases: [LinkType] = [
        .song, .artist, .album, .custom, .repository, .disabled,
    ]

    func displayName(_ language: AppLanguage) -> String {
        switch self {
        case .song: return t(language, "Song Page", "曲ページ")
        case .artist: return t(language, "Artist Page", "アーティストページ")
        case .album: return t(language, "Album Page", "アルバムページ")
        case .custom: return t(language, "Custom URL", "カスタムURL")
        case .repository: return t(language, "Repository", "リポジトリ")
        case .disabled: return t(language, "Off", "オフ")
        }
    }

    /// リンク先に応じたボタンラベルの既定値。
    /// ユーザーが手動でラベルを変更するまではこれに追従する。Discord側に出る文言なので英語固定。
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

/// メンバーリスト等に出る簡易バッジ「〜を再生中」に何を表示するか。
/// rawValueはDiscordの`status_display_type`の値（0=name / 1=state / 2=details）に一致させる。
enum BadgeLabelType: Int, CaseIterable {
    case appName = 0
    case artist = 1
    case track = 2

    func displayName(_ language: AppLanguage) -> String {
        switch self {
        // Discord側の名前は接続中のクライアント（Apple Music）に依存する
        case .appName: return t(language, "App Name", "アプリ名")
        case .artist: return t(language, "Artist Name", "アーティスト名")
        case .track: return t(language, "Track Name", "曲名")
        }
    }
}

final class SettingsStore {
    static let shared = SettingsStore()
    static let defaultRepositoryURL = "https://github.com/Narcissus-tazetta/Ceyrad"
    /// このアプリ専用のDiscord Application ID（Application名: "Apple Music"）。
    /// ユーザーが変更する必要はないため固定値とする。
    static let discordClientId = "1525381518258606130"
    /// Discord RPCの制限。ラベルはここで切ってから送る。
    static let maxLabelCharacters = 32

    private let defaults: UserDefaults

    /// 通常は `shared` を使う。テストでは専用のUserDefaultsを注入する。
    init(defaults: UserDefaults = .standard) {
        self.defaults = defaults
    }

    // MARK: - ボタン

    func buttonType(_ slot: ButtonSlot) -> LinkType {
        LinkType(rawValue: defaults.string(forKey: slot.typeKey) ?? "") ?? slot.defaultType
    }

    /// ラベルが未カスタマイズ（＝旧リンク先の既定値のまま）なら、
    /// リンク先の変更に合わせてラベルも追従させる。ユーザーが選んだラベルは残す。
    func setButtonType(_ slot: ButtonSlot, _ newValue: LinkType) {
        if let stored = defaults.string(forKey: slot.labelKey),
            stored == buttonType(slot).defaultLabel
        {
            defaults.removeObject(forKey: slot.labelKey)
        }
        defaults.set(newValue.rawValue, forKey: slot.typeKey)
    }

    func buttonLabel(_ slot: ButtonSlot) -> String {
        defaults.string(forKey: slot.labelKey) ?? buttonType(slot).defaultLabel
    }

    /// 空文字・現在のリンク先の既定値と同じならカスタム扱いにせず削除し、以後もリンク先に追従させる。
    func setButtonLabel(_ slot: ButtonSlot, _ newValue: String) {
        // ActivityBuilderが送信時に切るのと同じ切り方。絵文字で終わるラベルが
        // ダイアログから半分だけ返ってこないようにする。
        let value = Text.truncate(newValue, max: Self.maxLabelCharacters)
        if value.isEmpty || value == buttonType(slot).defaultLabel {
            defaults.removeObject(forKey: slot.labelKey)
        } else {
            defaults.set(value, forKey: slot.labelKey)
        }
    }

    // MARK: - URL

    var customURL: String {
        get { defaults.string(forKey: "customURL") ?? "" }
        set { defaults.set(newValue, forKey: "customURL") }
    }

    var repositoryURL: String {
        get { defaults.string(forKey: "repositoryURL") ?? Self.defaultRepositoryURL }
        set { defaults.set(newValue, forKey: "repositoryURL") }
    }

    // MARK: - 表示

    /// バッジ表示（status_display_type）。integer(forKey:)は未設定時に0を返すため、
    /// 既定値をartistにできるようobjectで取り出す。
    var badgeLabel: BadgeLabelType {
        get { BadgeLabelType(rawValue: defaults.object(forKey: "badgeLabel") as? Int ?? 1) ?? .artist }
        set { defaults.set(newValue.rawValue, forKey: "badgeLabel") }
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
