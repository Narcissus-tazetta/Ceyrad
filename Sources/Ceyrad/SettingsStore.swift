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
    /// 曲ページのみ再生中のソースに応じて変わる（nil＝表示用の既定はApple Music）。
    func defaultLabel(for source: MusicSourceID?) -> String {
        switch self {
        case .song: return source == .spotify ? "Play on Spotify" : "Play on Apple Music"
        case .artist: return "View Artist"
        case .album: return "View Album"
        case .custom: return "Open Link"
        case .repository: return "About This App"
        case .disabled: return ""
        }
    }

    /// このリンク先の既定ラベルとして扱う文字列すべて。「未カスタマイズか」の判定に使う。
    var defaultLabels: [String] {
        switch self {
        case .song: return ["Play on Apple Music", "Play on Spotify"]
        default: return [defaultLabel(for: nil)]
        }
    }
}

/// メンバーリスト等に出る簡易バッジ「〜を再生中」に何を表示するか。
/// rawValueはDiscordの`status_display_type`の値（0=name / 1=state / 2=details）に一致させる。
enum BadgeLabelType: Int, CaseIterable {
    case appName = 0
    case artist = 1
    case track = 2

    var displayName: String {
        switch self {
        // Discord側の名前は接続中のクライアント（Apple Music / Spotify）に依存する
        case .appName: return t("App Name", "アプリ名")
        case .artist: return t("Artist Name", "アーティスト名")
        case .track: return t("Track Name", "曲名")
        }
    }
}

final class SettingsStore {
    static let shared = SettingsStore()
    static let defaultRepositoryURL = "https://github.com/Narcissus-tazetta/Ceyrad"
    /// このアプリ専用のDiscord Application ID（Application名: "Apple Music"）。
    /// ユーザーが変更する必要はないため固定値とする。
    static let discordClientId = "1525381518258606130"
    /// Spotify再生時に使うDiscord Application ID（Application名: "Spotify"）。
    static let spotifyDiscordClientId = "1526238417845751959"

    private let defaults: UserDefaults

    /// 通常は `shared` を使う。テストでは専用のUserDefaultsを注入する。
    init(defaults: UserDefaults = .standard) {
        self.defaults = defaults
    }

    var button1Type: LinkType {
        get { LinkType(rawValue: defaults.string(forKey: "button1Type") ?? "") ?? .song }
        set { setType(newValue, typeKey: "button1Type", labelKey: "button1Label", old: button1Type) }
    }

    /// メニュー表示用（既定はApple Music表記）。Discordへ送る実ラベルは`button1Label(for:)`。
    var button1Label: String {
        get { button1Label(for: nil) }
        set { setLabel(newValue, labelKey: "button1Label", type: button1Type) }
    }

    func button1Label(for source: MusicSourceID?) -> String {
        defaults.string(forKey: "button1Label") ?? button1Type.defaultLabel(for: source)
    }

    var button2Type: LinkType {
        get { LinkType(rawValue: defaults.string(forKey: "button2Type") ?? "") ?? .repository }
        set { setType(newValue, typeKey: "button2Type", labelKey: "button2Label", old: button2Type) }
    }

    /// メニュー表示用（既定はApple Music表記）。Discordへ送る実ラベルは`button2Label(for:)`。
    var button2Label: String {
        get { button2Label(for: nil) }
        set { setLabel(newValue, labelKey: "button2Label", type: button2Type) }
    }

    func button2Label(for source: MusicSourceID?) -> String {
        defaults.string(forKey: "button2Label") ?? button2Type.defaultLabel(for: source)
    }

    /// ラベルが未カスタマイズ（＝旧リンク先の既定値のまま）なら、リンク先変更時にラベルも追従させる
    private func setType(_ newValue: LinkType, typeKey: String, labelKey: String, old: LinkType) {
        if let stored = defaults.string(forKey: labelKey), old.defaultLabels.contains(stored) {
            defaults.removeObject(forKey: labelKey)
        }
        defaults.set(newValue.rawValue, forKey: typeKey)
    }

    /// 既定値と同じ・空文字ならカスタムラベル扱いにせず削除し、以後もリンク先に追従させる
    private func setLabel(_ newValue: String, labelKey: String, type: LinkType) {
        let value = String(newValue.prefix(32))
        if value.isEmpty || type.defaultLabels.contains(value) {
            defaults.removeObject(forKey: labelKey)
        } else {
            defaults.set(value, forKey: labelKey)
        }
    }

    // MARK: - Music sources

    /// ソース別の有効/無効。無効にしたソースは起動していても監視しない。
    func isSourceEnabled(_ source: MusicSourceID) -> Bool {
        defaults.object(forKey: Self.sourceKey(source)) as? Bool ?? true
    }

    func setSourceEnabled(_ source: MusicSourceID, _ enabled: Bool) {
        defaults.set(enabled, forKey: Self.sourceKey(source))
    }

    private static func sourceKey(_ source: MusicSourceID) -> String {
        switch source {
        case .appleMusic: return "sourceAppleMusicEnabled"
        case .spotify: return "sourceSpotifyEnabled"
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
