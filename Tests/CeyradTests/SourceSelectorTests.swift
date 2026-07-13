import XCTest

@testable import Ceyrad

final class SourceSelectorTests: XCTestCase {
    private func source(
        running: Bool = true, state: PlayerState = .stopped,
        hasTrack: Bool = true, eventNs: UInt64 = 0
    ) -> SourceState {
        var s = SourceState()
        s.running = running
        s.playerState = state
        s.track = hasTrack ? TrackInfo(name: "T", artist: "A", album: "L") : nil
        s.lastEventUptimeNs = eventNs
        return s
    }

    private func select(
        _ am: SourceState, _ sp: SourceState, current: MusicSourceID? = nil
    ) -> MusicSourceID? {
        SourceSelector.selectActiveSource(appleMusic: am, spotify: sp, current: current)
    }

    func testNothingRunningYieldsNil() {
        XCTAssertNil(select(source(running: false, hasTrack: false), source(running: false, hasTrack: false)))
    }

    func testSinglePlayingSourceWins() {
        XCTAssertEqual(select(source(state: .playing), source(running: false, hasTrack: false)), .appleMusic)
        XCTAssertEqual(select(source(running: false, hasTrack: false), source(state: .playing)), .spotify)
    }

    func testPlayingBeatsPaused() {
        XCTAssertEqual(select(source(state: .paused), source(state: .playing)), .spotify)
        XCTAssertEqual(
            select(source(state: .paused), source(state: .playing), current: .appleMusic),
            .spotify
        )
        XCTAssertEqual(select(source(state: .playing), source(state: .paused), current: .spotify), .appleMusic)
    }

    func testBothPlayingMostRecentEventWins() {
        XCTAssertEqual(
            select(source(state: .playing, eventNs: 100), source(state: .playing, eventNs: 200)),
            .spotify
        )
        XCTAssertEqual(
            select(source(state: .playing, eventNs: 300), source(state: .playing, eventNs: 200)),
            .appleMusic
        )
    }

    func testBothPausedSticksToCurrent() {
        // 両方一時停止では表示中ソースを維持する（フリップ防止）
        XCTAssertEqual(
            select(source(state: .paused, eventNs: 100), source(state: .paused, eventNs: 200), current: .appleMusic),
            .appleMusic
        )
        XCTAssertEqual(
            select(source(state: .paused, eventNs: 200), source(state: .paused, eventNs: 100), current: .spotify),
            .spotify
        )
    }

    func testBothPausedWithoutCurrentPicksMostRecent() {
        XCTAssertEqual(
            select(source(state: .paused, eventNs: 100), source(state: .paused, eventNs: 200)),
            .spotify
        )
    }

    func testCurrentTerminatedFallsBackToOtherCandidate() {
        // 表示中だったソースが終了（running=false）したら残りの候補へ
        XCTAssertEqual(
            select(source(running: false, hasTrack: false), source(state: .paused), current: .appleMusic),
            .spotify
        )
    }

    func testStoppedSourceIsNotACandidate() {
        XCTAssertNil(select(source(state: .stopped, hasTrack: false), source(state: .stopped, hasTrack: false), current: .appleMusic))
        XCTAssertEqual(
            select(source(state: .stopped, hasTrack: false), source(state: .paused), current: .appleMusic),
            .spotify
        )
    }

    func testRunningWithoutTrackIsNotACandidate() {
        // 稼働していても曲情報がなければ表示しない（初期状態取得前など）
        XCTAssertNil(select(source(state: .playing, hasTrack: false), source(running: false, hasTrack: false)))
    }
}
