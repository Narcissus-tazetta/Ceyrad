import Foundation
import os

/// YouTube Music自身の検索。I/O以外はすべてここ（`windows/src/core/ytmusic.rs` と対）。
///
/// YouTube Musicで再生中の曲には、iTunesより再生元そのもののカタログのほうが良い情報源になる。
/// iTunesがまだ索引していない新曲やApple Musicに無いアップロードも引け、アートワークは
/// 利用者の画面にあるものと同じで、ボタンのリンク先になる動画IDも得られる。
///
/// エンドポイントはYouTube Musicのウェブクライアントが叩くもので、鍵は要らないが公開APIではない。
/// そのため応答は防御的に読む: 期待した形でないものは信用せず読み飛ばし、呼び出し側がiTunesに退く。
enum YouTubeMusicSearch {
    static let searchURL = URL(
        string: "https://music.youtube.com/youtubei/v1/search?prettyPrint=false")!

    /// 応答の大きさの上限。実際は200KB前後。
    static let maxResponseBytes = 1024 * 1024

    /// リクエストが名乗るウェブクライアント。数年前のバージョンも受け付け続けているので、
    /// リリースに追従させる必要はない。
    private static let clientName = "WEB_REMIX"
    private static let clientVersion = "1.20241101.01.00"

    /// 検索ページの「曲」フィルタ。無いと先頭は検索順位1位のもの——多くはミュージックビデオで、
    /// そのサムネイルはジャケットではなく16:9の静止画になる。
    private static let songsFilter = "EgWKAQIIAWoMEA4QChADEAQQCRAF"

    /// サムネイルは60か120pxで届く。サイズはURLの `=` 以降にあり、同じURLで原寸まで引ける。
    private static let artworkSizeSuffix = "w544-h544-l90-rj"

    /// 再生元と検索結果で長さの丸めが違う（プレイヤーは3:19、検索は3:20）ので許す差。
    private static let durationToleranceSec = 3.0

    struct Song: Equatable {
        var title: String
        /// 表示どおりのアーティスト（区切り文字込み。`A、B` / `A & B`）。
        var artists: String
        var firstArtist: String
        var durationSec: Double?
        var videoID: String
        var thumbnailURL: String
    }

    // MARK: - 言語

    /// 検索に使う表示言語を、可能性の高い順に。
    ///
    /// YouTube Musicはリクエストの言語で応え、アーティスト名（ときに曲名も）の表記をそれに合わせる
    /// （`キタニタツヤ` / `Tatsuya Kitani`）。プレイヤーが報告するのは利用者のYouTube Musicの言語での
    /// 表記だが、それを記録している場所はマシンに無い。曲自体のかな・ハングルがあれば決まり、
    /// なければアプリの表示言語が次善、最後に英語。
    static func languagesToTry(name: String, artist: String, appLanguage: String) -> [String] {
        var scriptLanguage: String?
        for scalar in (name + artist).unicodeScalars {
            switch scalar.value {
            case 0x3040...0x30FF, 0x31F0...0x31FF, 0xFF66...0xFF9F:
                scriptLanguage = "ja"
            case 0x1100...0x11FF, 0x3130...0x318F, 0xAC00...0xD7AF:
                scriptLanguage = "ko"
            default:
                continue
            }
            break
        }
        var order: [String] = []
        for language in [scriptLanguage, appLanguage == "ja" ? "ja" : "en", "en"] {
            if let language, !order.contains(language) { order.append(language) }
        }
        return order
    }

    // MARK: - リクエスト

    /// `language` と `region` で、アーティスト名がどの表記で返るかが決まる。プレイヤーの表示に合わせて
    /// 利用者のものを渡す。
    static func searchBody(
        name: String, artist: String, language: String, region: String
    ) throws -> Data {
        let query = "\(ITunesSearchClient.stripFeaturing(name)) \(artist)"
            .trimmingCharacters(in: .whitespaces)
        let client: [String: String] = [
            "clientName": clientName, "clientVersion": clientVersion, "hl": language, "gl": region,
        ]
        let body: [String: Any] = ["context": ["client": client], "query": query, "params": songsFilter]
        return try JSONSerialization.data(withJSONObject: body, options: [.sortedKeys])
    }

    // MARK: - 応答

    /// 応答に含まれる曲の行を、表示順に。JSONとして読めなければ `nil`。
    static func parseSongs(_ data: Data) -> [Song]? {
        guard let root = try? JSONSerialization.jsonObject(with: data) else { return nil }
        var songs: [Song] = []
        collectSongs(root, into: &songs)
        return songs
    }

    private static func collectSongs(_ value: Any, into out: inout [Song]) {
        if let object = value as? [String: Any] {
            if let row = object["musicResponsiveListItemRenderer"] {
                if let song = song(fromRow: row) { out.append(song) }
                return
            }
            for child in object.values { collectSongs(child, into: &out) }
        } else if let array = value as? [Any] {
            for child in array { collectSongs(child, into: &out) }
        }
    }

    private static func song(fromRow row: Any) -> Song? {
        guard let row = row as? [String: Any], let columns = row["flexColumns"] as? [[String: Any]]
        else { return nil }
        func runs(_ index: Int) -> [String] {
            guard columns.indices.contains(index),
                let renderer = columns[index]["musicResponsiveListItemFlexColumnRenderer"]
                    as? [String: Any],
                let text = renderer["text"] as? [String: Any],
                let runs = text["runs"] as? [[String: Any]]
            else { return [] }
            return runs.compactMap { $0["text"] as? String }
        }

        let title = runs(0).joined()
        // `アーティスト • アルバム • 3:20`。アーティストは複数のrunにまたがりうる。
        let segments = runs(1).split(
            omittingEmptySubsequences: false, whereSeparator: { $0.trimmingCharacters(in: .whitespaces) == "•" })
        guard let artistRuns = segments.first, let firstRun = artistRuns.first else { return nil }
        let artists = artistRuns.joined().trimmingCharacters(in: .whitespaces)
        let firstArtist = firstRun.trimmingCharacters(in: .whitespaces)
        let durationSec = segments.dropFirst().last.flatMap { parseDuration($0.joined()) }

        guard let data = row["playlistItemData"] as? [String: Any],
            let videoID = data["videoId"] as? String,
            let thumbnail = row["thumbnail"] as? [String: Any],
            let renderer = thumbnail["musicThumbnailRenderer"] as? [String: Any],
            let inner = renderer["thumbnail"] as? [String: Any],
            let thumbnails = inner["thumbnails"] as? [[String: Any]],
            let thumbnailURL = thumbnails.last?["url"] as? String
        else { return nil }

        guard !title.isEmpty, !artists.isEmpty else { return nil }
        return Song(
            title: title, artists: artists, firstArtist: firstArtist, durationSec: durationSec,
            videoID: videoID, thumbnailURL: thumbnailURL)
    }

    /// `3:20` または `1:02:03`。
    private static func parseDuration(_ text: String) -> Double? {
        var total = 0.0
        var parts = 0
        for part in text.trimmingCharacters(in: .whitespaces).split(
            separator: ":", omittingEmptySubsequences: false)
        {
            guard !part.isEmpty, part.allSatisfy({ $0.isASCII && $0.isNumber }),
                let value = Double(part)
            else { return nil }
            total = total * 60 + value
            parts += 1
        }
        return (2...3).contains(parts) ? total : nil
    }

    // MARK: - 照合

    /// この曲にあたる行。十分に一致する行が無ければ `nil`。
    ///
    /// 曲名もアーティストも一致が要る: 誰かの表示に違うジャケットが出るくらいなら出さないほうがいい。
    /// 長さは、すでに一致している行どうしの決め手にだけ使う（リマスターやライブは両方が同じ）。
    static func pickBest(
        from songs: [Song], name: String, artist: String, durationSec: Double?
    ) -> Song? {
        // featの除去は折りたたみの前後どちらにも要る: YouTube Musicは日本語の曲名を全角括弧
        // （`（feat. X）`）で書き、`stripFeaturing` が認識するのはASCIIの括弧だけ。
        func withoutFeat(_ title: String) -> String {
            ITunesSearchClient.norm(
                ITunesSearchClient.stripFeaturing(ITunesSearchClient.norm(title)))
        }
        let wantedTitle = ITunesSearchClient.norm(name)
        let wantedTitleNoFeat = withoutFeat(name)
        let wantedArtist = ITunesSearchClient.norm(artist)
        guard !wantedArtist.isEmpty else { return nil }

        // プレイヤーと検索で共演者の表記が違いうる（`キタニタツヤ、natori` と `キタニタツヤ、なとり`）
        // ので、筆頭アーティストだけが一致すればよいことにする。
        let wantedLead = ITunesSearchClient.norm(leadArtist(artist))
        func artistMatches(_ song: Song) -> Bool {
            ITunesSearchClient.norm(song.artists) == wantedArtist
                || ITunesSearchClient.norm(song.firstArtist) == wantedLead
        }
        func durationMatches(_ song: Song) -> Bool {
            guard let a = song.durationSec, let b = durationSec else { return false }
            return abs(a - b) <= durationToleranceSec
        }

        let tiers: [(Song) -> Bool] = [
            { ITunesSearchClient.norm($0.title) == wantedTitle && artistMatches($0) && durationMatches($0) },
            { ITunesSearchClient.norm($0.title) == wantedTitle && artistMatches($0) },
            { withoutFeat($0.title) == wantedTitleNoFeat && artistMatches($0) },
        ]
        for tier in tiers {
            if let song = songs.first(where: tier) { return song }
        }
        return nil
    }

    /// 連名のアーティスト文字列の先頭。YouTube Musicが共演者をつなぐ区切り文字で切る。
    private static func leadArtist(_ artists: String) -> String {
        let separators: Set<Character> = ["、", ",", "，", "&"]
        let lead = artists.split(
            maxSplits: 1, omittingEmptySubsequences: false, whereSeparator: { separators.contains($0) }
        ).first
        return lead.map { $0.trimmingCharacters(in: .whitespaces) } ?? artists
    }

    static func catalog(from song: Song) -> CatalogInfo {
        CatalogInfo(
            songURL: "https://music.youtube.com/watch?v=\(song.videoID)",
            artworkURL: upscaleArtwork(song.thumbnailURL))
    }

    /// Google配信のサムネイルのサイズ接尾辞を差し替える。それ以外（`i.ytimg.com` の静止画など）はそのまま。
    private static func upscaleArtwork(_ url: String) -> String {
        let isGoogleImage = url.contains(".googleusercontent.com/") || url.contains(".ggpht.com/")
        guard isGoogleImage, let index = url.lastIndex(of: "=") else { return url }
        return "\(url[..<index])=\(artworkSizeSuffix)"
    }
}

/// `YouTubeMusicSearch` を実際に叩く。1回の照会で言語ごとに最大3リクエスト。
final class YouTubeMusicSearchClient {
    private struct Query {
        let name: String
        let artist: String
        let durationSec: Double?
        let region: String
    }

    private static let log = Logger(
        subsystem: Bundle.main.bundleIdentifier ?? "Ceyrad", category: "YouTubeMusicSearch"
    )
    private let session: URLSession

    init(session: URLSession? = nil) {
        let config = URLSessionConfiguration.ephemeral
        config.timeoutIntervalForRequest = 10
        config.timeoutIntervalForResource = 15
        self.session = session ?? URLSession(configuration: config)
    }

    /// 結果は `CatalogOutcome`: 全言語で答えが得られて行が無ければ `.missing`、答えが得られなければ `.failed`。
    /// completionはメインスレッド。
    func resolve(
        name: String, artist: String, durationSec: Double?, appLanguage: String,
        completion: @escaping (CatalogOutcome) -> Void
    ) {
        let query = Query(
            name: name, artist: artist, durationSec: durationSec,
            region: Locale.current.region?.identifier ?? "US")
        let languages = YouTubeMusicSearch.languagesToTry(
            name: name, artist: artist, appLanguage: appLanguage)
        attempt(languages[...], query) { outcome in
            DispatchQueue.main.async { completion(outcome) }
        }
    }

    private func attempt(
        _ languages: ArraySlice<String>, _ query: Query,
        completion: @escaping (CatalogOutcome) -> Void
    ) {
        guard let language = languages.first else { return completion(.missing) }
        var request = URLRequest(url: YouTubeMusicSearch.searchURL)
        request.httpMethod = "POST"
        request.setValue("application/json", forHTTPHeaderField: "Content-Type")
        do {
            request.httpBody = try YouTubeMusicSearch.searchBody(
                name: query.name, artist: query.artist, language: language, region: query.region)
        } catch {
            Self.log.error("search body could not be encoded: \(error.localizedDescription)")
            return completion(.failed)
        }
        session.dataTask(with: request) { [weak self] data, response, _ in
            guard let self, let data, data.count <= YouTubeMusicSearch.maxResponseBytes,
                (response as? HTTPURLResponse)?.statusCode == 200
            else { return completion(.failed) }
            // 読めない応答は「この曲の行が無い」とは違うが、Windows版と同じく打ち切って
            // iTunesに任せる（形が変わったエンドポイントを叩き直しても同じ結果になる）。
            guard let songs = YouTubeMusicSearch.parseSongs(data) else { return completion(.missing) }
            if let song = YouTubeMusicSearch.pickBest(
                from: songs, name: query.name, artist: query.artist, durationSec: query.durationSec)
            {
                return completion(.found(YouTubeMusicSearch.catalog(from: song)))
            }
            self.attempt(languages.dropFirst(), query, completion: completion)
        }.resume()
    }
}
