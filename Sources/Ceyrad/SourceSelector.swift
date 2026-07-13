import Foundation

/// ソースごとの稼働・再生状態。AppDelegateがソース別に1つずつ保持する。
struct SourceState {
    var running = false
    var playerState: PlayerState = .stopped
    var track: TrackInfo?
    var catalog: CatalogInfo?
    /// 最後にイベントを受けた時刻（単調時計）。両方再生中のときの優先判定に使う。
    var lastEventUptimeNs: UInt64 = 0
}

/// 全ソースの状態。Dictionaryにしない（optional購読とボクシングを避ける）。
struct SourceStates {
    var appleMusic = SourceState()
    var spotify = SourceState()

    subscript(id: MusicSourceID) -> SourceState {
        get { id == .appleMusic ? appleMusic : spotify }
        set {
            switch id {
            case .appleMusic: appleMusic = newValue
            case .spotify: spotify = newValue
            }
        }
    }

    var anyRunning: Bool { appleMusic.running || spotify.running }
}

/// どのソースをDiscordに表示するかの選択ロジック。純粋関数としてテスト可能にする。
enum SourceSelector {
    /// ポリシー:
    /// 1. 候補 = 稼働中 && 曲あり && 停止中でない
    /// 2. 片方だけ再生中ならそれが勝つ（一時停止側は再生側に譲る）
    /// 3. 両方再生中なら直近にイベントを発した方
    /// 4. どちらも再生中でなければ、表示中ソースが候補である限り維持
    ///    （両方一時停止でプレゼンスがフリップしないためのスティッキネス）
    static func selectActiveSource(
        appleMusic: SourceState, spotify: SourceState, current: MusicSourceID?
    ) -> MusicSourceID? {
        func isCandidate(_ s: SourceState) -> Bool {
            s.running && s.track != nil && s.playerState != .stopped
        }
        let amCandidate = isCandidate(appleMusic)
        let spCandidate = isCandidate(spotify)
        let amPlaying = amCandidate && appleMusic.playerState == .playing
        let spPlaying = spCandidate && spotify.playerState == .playing

        func mostRecent() -> MusicSourceID {
            spotify.lastEventUptimeNs > appleMusic.lastEventUptimeNs ? .spotify : .appleMusic
        }

        switch (amPlaying, spPlaying) {
        case (true, false): return .appleMusic
        case (false, true): return .spotify
        case (true, true): return mostRecent()
        case (false, false):
            if let current {
                let currentIsCandidate = current == .appleMusic ? amCandidate : spCandidate
                if currentIsCandidate { return current }
            }
            switch (amCandidate, spCandidate) {
            case (true, false): return .appleMusic
            case (false, true): return .spotify
            case (true, true): return mostRecent()
            case (false, false): return nil
            }
        }
    }
}
