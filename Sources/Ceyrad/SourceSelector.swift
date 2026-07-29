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

/// 全ソースの状態。ソースは1つだが、AppDelegate側はMusicSourceIDで引く形を保つ。
struct SourceStates {
    var appleMusic = SourceState()

    subscript(id: MusicSourceID) -> SourceState {
        get { appleMusic }
        set { appleMusic = newValue }
    }

    var anyRunning: Bool { appleMusic.running }
}

/// どのソースをDiscordに表示するかの選択ロジック。純粋関数としてテスト可能にする。
enum SourceSelector {
    /// 候補 = 稼働中 && 曲あり && 停止中でない。
    /// 候補でなくなったら（曲が終わった、アプリが終了した等）表示を外す。
    static func selectActiveSource(
        appleMusic: SourceState, current: MusicSourceID?
    ) -> MusicSourceID? {
        let isCandidate =
            appleMusic.running && appleMusic.track != nil
            && appleMusic.playerState != .stopped
        return isCandidate ? .appleMusic : nil
    }
}
