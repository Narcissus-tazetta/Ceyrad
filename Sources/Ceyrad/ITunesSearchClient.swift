import Foundation

/// iTunes Search API（認証不要）でCatalog URLとアートワークURLを解決する。
/// MusicKitと違いApple Developer Program加入が不要で、1リクエストで
/// 曲/アーティスト/アルバムURL + アートワークが全て取れる。
final class ITunesSearchClient {
    private struct SearchResponse: Decodable {
        let results: [SearchResult]
    }

    struct SearchResult: Decodable {
        let trackName: String?
        let artistName: String?
        let collectionName: String?
        let trackViewUrl: String?
        let artistViewUrl: String?
        let collectionViewUrl: String?
        let artworkUrl100: String?
    }

    private let queue = DispatchQueue(label: "itunes-search")
    // 見つからなかった結果（ローカル取り込み曲など）もキャッシュして再検索の連発を防ぐ
    private var cache: [String: CatalogInfo?] = [:]
    private var cacheOrder: [String] = []
    private let cacheCapacity = 300
    // 非公式に約20req/分の制限があるため、最低間隔を空ける
    private var nextAllowedRequest = Date.distantPast
    private let minRequestInterval: TimeInterval = 3.0
    // レート制限待ちの検索。曲が変わったら古い検索は破棄する（曲送り連打対策）
    private var pendingSearch: DispatchWorkItem?
    private let session: URLSession

    init() {
        let config = URLSessionConfiguration.ephemeral
        config.timeoutIntervalForRequest = 10
        config.timeoutIntervalForResource = 15
        session = URLSession(configuration: config)
    }

    func resolve(
        name: String, artist: String, album: String,
        completion: @escaping (CatalogInfo?) -> Void
    ) {
        let key = [name, artist, album].joined(separator: "\u{1F}").lowercased()
        queue.async { [weak self] in
            guard let self else { return }
            if let cached = self.cache[key] {
                DispatchQueue.main.async { completion(cached) }
                return
            }
            // 新しい曲のリクエストで、まだ実行されていない古い検索を置き換える。
            // 破棄した分はレート制限の枠を消費しない（枠の確保は実行時に行う）。
            self.pendingSearch?.cancel()
            let work = DispatchWorkItem { [weak self] in
                self?.performSearch(
                    key: key, name: name, artist: artist, album: album,
                    completion: completion
                )
            }
            self.pendingSearch = work
            let delay = max(0, self.nextAllowedRequest.timeIntervalSinceNow)
            self.queue.asyncAfter(deadline: .now() + delay, execute: work)
        }
    }

    private func performSearch(
        key: String, name: String, artist: String, album: String,
        completion: @escaping (CatalogInfo?) -> Void
    ) {
        nextAllowedRequest = Date().addingTimeInterval(minRequestInterval)
        var components = URLComponents(string: "https://itunes.apple.com/search")!
        components.queryItems = [
            // ローカルのfeat.表記がカタログと違ってもヒットしやすいよう、除去して検索する
            URLQueryItem(name: "term", value: "\(Self.stripFeaturing(name)) \(artist)"),
            URLQueryItem(name: "media", value: "music"),
            URLQueryItem(name: "entity", value: "song"),
            URLQueryItem(name: "limit", value: "10"),
            URLQueryItem(name: "country", value: Locale.current.region?.identifier ?? "US"),
        ]
        guard let url = components.url else {
            DispatchQueue.main.async { completion(nil) }
            return
        }
        session.dataTask(with: url) { [weak self] data, _, _ in
            guard let self else { return }
            var info: CatalogInfo?
            var gotResponse = false
            if let data,
                let response = try? JSONDecoder().decode(SearchResponse.self, from: data)
            {
                gotResponse = true
                if let best = Self.pickBest(
                    from: response.results,
                    name: name, artist: artist, album: album
                ) {
                    info = CatalogInfo(
                        songURL: best.trackViewUrl,
                        artistURL: best.artistViewUrl,
                        albumURL: best.collectionViewUrl,
                        artworkURL: best.artworkUrl100?
                            .replacingOccurrences(of: "100x100bb", with: "512x512bb")
                    )
                }
            }
            self.queue.async {
                // ネットワークエラー時はキャッシュしない（一時的なオフラインを恒久化しない）
                if gotResponse { self.store(key: key, info: info) }
                DispatchQueue.main.async { completion(info) }
            }
        }.resume()
    }

    /// 誤マッチで無関係な曲のリンクを出さないよう、一致度の高い順にだけ採用する。
    /// feat.表記やアルバムの「- Single」サフィックスの揺れは除去した上でも比較する。
    static func pickBest(
        from results: [SearchResult],
        name: String, artist: String, album: String
    ) -> SearchResult? {
        func norm(_ s: String?) -> String {
            (s ?? "")
                .folding(
                    options: [.diacriticInsensitive, .widthInsensitive, .caseInsensitive],
                    locale: nil
                )
                // widthInsensitiveは全角スペース(U+3000)を折りたたまないため個別に吸収する
                .replacingOccurrences(of: "\u{3000}", with: " ")
                .trimmingCharacters(in: .whitespacesAndNewlines)
        }
        let n = norm(name)
        let a = norm(artist)
        let al = norm(stripAlbumSuffix(album))
        let ns = norm(stripFeaturing(name))
        struct Candidate {
            let result: SearchResult
            let track: String
            let trackNoFeat: String
            let artist: String
            let album: String
        }
        // 正規化はUnicode折りたたみと正規表現を伴い軽くないため、
        // ティアごとに繰り返さず結果1件につき1回で済ませる
        let candidates = results.map { r in
            Candidate(
                result: r,
                track: norm(r.trackName),
                trackNoFeat: norm(stripFeaturing(r.trackName ?? "")),
                artist: norm(r.artistName),
                album: norm(stripAlbumSuffix(r.collectionName ?? ""))
            )
        }
        let tiers: [(Candidate) -> Bool] = [
            { $0.track == n && $0.artist == a && $0.album == al },
            { $0.track == n && $0.artist == a },
            { $0.trackNoFeat == ns && $0.artist == a },
            { $0.track == n },
            { $0.trackNoFeat == ns },
        ]
        for tier in tiers {
            if let c = candidates.first(where: tier) { return c.result }
        }
        return nil
    }

    // MARK: - 表記揺れの吸収

    private static let featParenRegex =
        #/\s*[(\[](?:featuring|feat|ft|with)[.\s][^)\]]*[)\]]/#.ignoresCase()
    private static let featTrailingRegex =
        #/\s+(?:featuring|feat|ft)\.?\s.*$/#.ignoresCase()
    private static let albumSuffixRegex =
        #/\s*-\s*(?:Single|EP)\s*$/#.ignoresCase()

    /// 「(feat. X)」「[feat. X]」や末尾の「feat. X」を除去する。
    /// 除去すると空になる場合は元の文字列を返す。
    static func stripFeaturing(_ s: String) -> String {
        let stripped =
            s
            .replacing(featParenRegex, with: "")
            .replacing(featTrailingRegex, with: "")
            .trimmingCharacters(in: .whitespaces)
        return stripped.isEmpty ? s : stripped
    }

    /// アルバム名末尾の「- Single」「- EP」を除去する。除去すると空になる場合は元の文字列を返す。
    static func stripAlbumSuffix(_ s: String) -> String {
        let stripped =
            s
            .replacing(albumSuffixRegex, with: "")
            .trimmingCharacters(in: .whitespaces)
        return stripped.isEmpty ? s : stripped
    }

    private func store(key: String, info: CatalogInfo?) {
        if cache.index(forKey: key) == nil {
            cacheOrder.append(key)
        }
        cache[key] = info
        while cacheOrder.count > cacheCapacity {
            cache.removeValue(forKey: cacheOrder.removeFirst())
        }
    }
}
