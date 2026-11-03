import Foundation

// MARK: - カタログ（アートワークとリンク）

extension AppDelegate {
    /// アートワークとリンクを持たない再生中の曲について照会を投げる。
    ///
    /// 通知のたびに呼ばれるので、`catalogRequestedFor` の番人が
    /// 「イベントの頻度」を「リクエストの頻度」に変えないようにしている。
    func requestMissingCatalog(_ source: MusicSource) {
        let current = state(source)
        guard current.running, current.catalog == nil,
            let track = current.track, !track.name.isEmpty
        else { return }
        // 失敗した照会が冷却期間中
        if let retryAt = current.catalogRetryAt, retryAt > Date() { return }
        guard current.catalogRequestedFor != track.identity else { return }

        update(source) {
            $0.catalogRequestedFor = track.identity
            $0.catalogRetryAt = nil
        }
        itunes.resolve(
            name: track.name, artist: track.artist, album: track.album,
            fallbackStorefronts: source == .youtubeMusic ? ["US", "JP"] : []
        ) { [weak self] outcome in
            self?.applyCatalog(source, target: track, outcome: outcome)
        }
    }

    /// 返ってきた照会結果を反映する。遅れて届いた答えは、その曲がまだ再生中のときだけ使う。
    private func applyCatalog(_ source: MusicSource, target: TrackInfo, outcome: CatalogOutcome) {
        guard let current = state(source).track, current.identity == target.identity else { return }
        switch outcome {
        case .found(let info):
            update(source) {
                $0.catalog = info
                $0.catalogRetryAt = nil
            }
            // 先にアートなしで送信済みなので、解決できてかつ表示中のときだけ再送する
            if displaying, displayedSource == source { pushActivity() }
        case .missing:
            // 曲の性質による確定した答え（ローカル取り込み等）。訊き直さない。
            update(source) {
                $0.catalog = nil
                $0.catalogRetryAt = nil
            }
        case .failed:
            // 答えではない。番人を外して冷却期間のあとに訊き直せるようにする。
            update(source) {
                $0.catalogRequestedFor = nil
                $0.catalogRetryAt = Date().addingTimeInterval(Self.catalogRetryDelay)
            }
            scheduleCatalogRetry(source)
        }
    }

    private func scheduleCatalogRetry(_ source: MusicSource) {
        catalogRetryWork[source]?.cancel()
        let work = DispatchWorkItem { [weak self] in
            guard let self else { return }
            self.catalogRetryWork[source] = nil
            self.update(source) { $0.catalogRetryAt = nil }
            self.requestMissingCatalog(source)
        }
        catalogRetryWork[source] = work
        DispatchQueue.main.asyncAfter(deadline: .now() + Self.catalogRetryDelay, execute: work)
    }

    func cancelCatalogRetry(_ source: MusicSource) {
        catalogRetryWork[source]?.cancel()
        catalogRetryWork[source] = nil
    }
}
