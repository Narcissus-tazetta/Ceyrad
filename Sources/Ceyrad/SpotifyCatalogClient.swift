import Foundation

private struct OEmbedResponse: Decodable {
    let thumbnailUrl: String?

    enum CodingKeys: String, CodingKey {
        case thumbnailUrl = "thumbnail_url"
    }
}

/// SpotifyのカタログURLとアートワークURLを解決する。
/// 曲URLはTrack IDから機械的に作れるためI/O不要。アートワークはまずAppleScriptの
/// `artwork url`（ネットワーク不要のCDN URL）を使い、失敗時のみoEmbed APIへフォールバックする。
final class SpotifyCatalogClient {
    private let queue = DispatchQueue(label: "spotify-catalog")
    // 解決済みの結果をTrack IDごとにキャッシュして再取得の連発を防ぐ
    private var cache: [String: CatalogInfo?] = [:]
    private var cacheOrder: [String] = []
    private let cacheCapacity = 300
    // oEmbedフォールバック用。曲送り連打でリクエストが連発しないよう最低間隔を空ける
    private var nextAllowedRequest = Date.distantPast
    private let minRequestInterval: TimeInterval = 1.0
    // レート制限待ちの取得。曲が変わったら古い取得は破棄する（曲送り連打対策）
    private var pendingFetch: DispatchWorkItem?
    private let session: URLSession

    init() {
        let config = URLSessionConfiguration.ephemeral
        config.timeoutIntervalForRequest = 10
        config.timeoutIntervalForResource = 15
        session = URLSession(configuration: config)
    }

    /// "spotify:track:xxx" → 共有URL。広告(spotify:ad:)やローカル曲(spotify:local:)はnil。
    static func trackURL(fromTrackId trackId: String) -> String? {
        let prefix = "spotify:track:"
        guard trackId.hasPrefix(prefix) else { return nil }
        let id = trackId.dropFirst(prefix.count)
        guard !id.isEmpty, id.allSatisfy({ $0.isLetter || $0.isNumber }) else { return nil }
        return "https://open.spotify.com/track/\(id)"
    }

    func resolve(trackId: String, completion: @escaping (CatalogInfo?) -> Void) {
        queue.async { [weak self] in
            guard let self else { return }
            if let cached = self.cache[trackId] {
                DispatchQueue.main.async { completion(cached) }
                return
            }
            guard let songURL = Self.trackURL(fromTrackId: trackId) else {
                // 構造的に曲でないID（広告・ローカル曲）は結果が変わらないためネガティブキャッシュする
                self.store(key: trackId, info: nil)
                DispatchQueue.main.async { completion(nil) }
                return
            }
            // まずAppleScriptでCDN URLを取得（ネットワーク不要）。completionはメインスレッドで届く。
            SpotifyAppleScript.artworkURL(forTrackId: trackId) { [weak self] artwork in
                guard let self else { return }
                if let artwork {
                    let info = CatalogInfo(songURL: songURL, artworkURL: artwork)
                    self.queue.async { self.store(key: trackId, info: info) }
                    completion(info)
                } else {
                    // 曲替わり・権限拒否・artwork url不調時はoEmbedへフォールバック
                    self.queue.async {
                        self.scheduleOEmbedFetch(
                            trackId: trackId, songURL: songURL, completion: completion
                        )
                    }
                }
            }
        }
    }

    // MARK: - oEmbed fallback (queue)

    private func scheduleOEmbedFetch(
        trackId: String, songURL: String,
        completion: @escaping (CatalogInfo?) -> Void
    ) {
        // 新しい曲のリクエストで、まだ実行されていない古い取得を置き換える
        pendingFetch?.cancel()
        let work = DispatchWorkItem { [weak self] in
            self?.performOEmbedFetch(trackId: trackId, songURL: songURL, completion: completion)
        }
        pendingFetch = work
        let delay = max(0, nextAllowedRequest.timeIntervalSinceNow)
        queue.asyncAfter(deadline: .now() + delay, execute: work)
    }

    private func performOEmbedFetch(
        trackId: String, songURL: String,
        completion: @escaping (CatalogInfo?) -> Void
    ) {
        nextAllowedRequest = Date().addingTimeInterval(minRequestInterval)
        var components = URLComponents(string: "https://open.spotify.com/oembed")!
        components.queryItems = [URLQueryItem(name: "url", value: trackId)]
        guard let url = components.url else {
            DispatchQueue.main.async { completion(CatalogInfo(songURL: songURL)) }
            return
        }
        session.dataTask(with: url) { [weak self] data, _, _ in
            guard let self else { return }
            var info = CatalogInfo(songURL: songURL)
            var gotResponse = false
            if let data,
                let response = try? JSONDecoder().decode(OEmbedResponse.self, from: data)
            {
                gotResponse = true
                info.artworkURL = response.thumbnailUrl
            }
            self.queue.async {
                // ネットワークエラー時はキャッシュしない（一時的なオフラインを恒久化しない）。
                // 曲URL自体は完了時に常に返す（ボタンは出せる）。
                if gotResponse { self.store(key: trackId, info: info) }
                DispatchQueue.main.async { completion(info) }
            }
        }.resume()
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
