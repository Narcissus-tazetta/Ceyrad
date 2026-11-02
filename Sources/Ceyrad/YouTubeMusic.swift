import Foundation

/// MediaRemote が報告する「いま再生中」から YouTube Music を見分ける。
///
/// 拡張機能を使わないので、手がかりはブラウザが OS に登録したメタデータだけになる。
/// そこに URL はないため、通常の YouTube 動画との区別は次の2つの併用で行う。
/// - アルバム名があること。YouTube Music の曲は Media Session にアルバムを載せ、通常の動画は載せない。
/// - 再生元のブラウザに `music.youtube.com` のタブがあること。
enum YouTubeMusic {
    static let displayName = "YouTube Music"
    static let host = "music.youtube.com"

    /// タブの URL を AppleScript で読めるブラウザ。Firefox はスクリプト対応がなく、タブを確認できない。
    /// 実機で確かめたのは Chrome だけ。
    static let browserBundleIds: Set<String> = [
        "com.google.Chrome", "com.google.Chrome.beta", "com.google.Chrome.dev",
        "com.google.Chrome.canary", "com.apple.Safari", "com.microsoft.edgemac",
        "com.brave.Browser", "company.thebrowser.Browser", "com.vivaldi.Vivaldi",
    ]

    static func isBrowser(bundleId: String) -> Bool {
        browserBundleIds.contains(bundleId)
    }

    /// `hasYouTubeMusicTab` が `nil` なら、そのOSにタブ確認の手段がないものとしてアルバムだけで決める。
    /// 共有ベクトル `spec/youtube_music_vectors.json` と同じ規則。
    static func matches(isBrowser: Bool, album: String, hasYouTubeMusicTab: Bool?) -> Bool {
        guard isBrowser, !album.trimmingCharacters(in: .whitespacesAndNewlines).isEmpty else {
            return false
        }
        return hasYouTubeMusicTab ?? true
    }

    /// YouTube Music のタブのタイトルは、曲のページでは「曲名 | YouTube Music」のようになる。
    private static let titleSuffixes = [" | YouTube Music", " - YouTube Music"]

    static func musicTabs(_ tabs: [BrowserTab]) -> [BrowserTab] {
        tabs.filter { URLComponents(string: $0.url)?.host?.lowercased() == host }
    }

    /// 再生中の曲名がそのタブのタイトルと一致するか。ホームや検索を開いているタブは一致しない。
    static func titleMatches(_ tab: BrowserTab, trackTitle: String) -> Bool {
        !trackTitle.isEmpty && titleSuffixes.contains { tab.title == trackTitle + $0 }
    }

    /// macOS での判定。共通の規則（`matches`）に加えて、アルバム名が付かない曲
    /// （ページを読み込み直した直後の曲など）を、曲名とタブのタイトルの一致で補う。
    /// この補強は共有ベクトルの対象外（Windows はタブを読めない）。
    static func isYouTubeMusic(
        isBrowser: Bool, album: String, trackTitle: String, tabs: [BrowserTab]
    ) -> Bool {
        let musicTabs = musicTabs(tabs)
        if matches(isBrowser: isBrowser, album: album, hasYouTubeMusicTab: !musicTabs.isEmpty) {
            return true
        }
        return isBrowser && musicTabs.contains { titleMatches($0, trackTitle: trackTitle) }
    }

    /// 鳴っているタブの曲ページ。
    ///
    /// タブが複数あるときは、曲名がタイトルと一致するものを鳴っているタブとみなす。それでも
    /// 決まらないとき、そして `/watch` 以外のページ（ホームや検索）を開いたままのときは、
    /// 再生は続いていてもURLは曲を指さないので「分からない」と答える。
    static func songURL(fromTabs tabs: [BrowserTab], trackTitle: String) -> String? {
        var candidates = musicTabs(tabs)
        if candidates.count > 1 {
            candidates = candidates.filter { titleMatches($0, trackTitle: trackTitle) }
        }
        guard candidates.count == 1, let components = URLComponents(string: candidates[0].url),
            components.path == "/watch",
            let id = components.queryItems?.first(where: { $0.name == "v" })?.value, !id.isEmpty
        else { return nil }

        var canonical = URLComponents()
        canonical.scheme = "https"
        canonical.host = host
        canonical.path = "/watch"
        canonical.queryItems = [URLQueryItem(name: "v", value: id)]
        return canonical.string
    }
}

/// `mediaremote-adapter stream` が1行ずつ出す JSON のうち、使う項目。
struct NowPlayingPayload: Equatable {
    var bundleId: String
    var title: String
    var artist: String
    var album: String
    var playing: Bool
    var durationSec: Double?
    var elapsedSec: Double?
    var timestamp: Date?

    /// 曲名・ブラウザ・アルバムが同じなら同じ曲。再生/一時停止やシークでは変わらない。
    var identity: String {
        "\(bundleId)\u{1F}\(title)\u{1F}\(artist)\u{1F}\(album)"
    }

    var playerState: PlayerState {
        playing ? .playing : .paused
    }

    /// これより長い曲はない。ライブ配信などが報告する無限大や桁外れの長さは、そのまま
    /// 進捗バーのタイムスタンプ（整数への変換）に流すと壊れるので、長さなしとして扱う。
    private static let maxPlausibleDurationSec = 24.0 * 60 * 60

    var trackInfo: TrackInfo {
        var track = TrackInfo(
            name: title, artist: artist, album: album,
            durationSec: durationSec.flatMap {
                (0...Self.maxPlausibleDurationSec).contains($0) && $0 > 0 ? $0 : nil
            },
            positionSec: elapsedSec.flatMap { $0.isFinite && $0 >= 0 ? $0 : nil }
        )
        if let timestamp { track.positionSampledAt = timestamp }
        return track
    }

    enum ParseError: Error {
        case notJSON
        case unexpectedShape
    }

    private static let timestampFormatter = ISO8601DateFormatter()

    /// `nil` は「何も再生していない」。曲名と再生元が揃わないものは再生中として扱わない
    /// （アダプタの仕様で、曲名のないメディアは無効）。
    static func parse(line: String) throws -> NowPlayingPayload? {
        guard let data = line.data(using: .utf8),
            let object = try? JSONSerialization.jsonObject(with: data)
        else { throw ParseError.notJSON }
        guard let envelope = object as? [String: Any],
            let payload = envelope["payload"] as? [String: Any]
        else { throw ParseError.unexpectedShape }

        guard let bundleId = payload["bundleIdentifier"] as? String,
            let title = payload["title"] as? String
        else { return nil }

        return NowPlayingPayload(
            bundleId: bundleId,
            title: title,
            artist: payload["artist"] as? String ?? "",
            album: payload["album"] as? String ?? "",
            playing: payload["playing"] as? Bool ?? false,
            durationSec: (payload["duration"] as? NSNumber)?.doubleValue,
            elapsedSec: (payload["elapsedTime"] as? NSNumber)?.doubleValue,
            timestamp: (payload["timestamp"] as? String).flatMap(timestampFormatter.date(from:))
        )
    }
}
