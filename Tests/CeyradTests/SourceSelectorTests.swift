import XCTest

@testable import Ceyrad

final class SourceSelectorTests: XCTestCase {
    private func source(
        running: Bool = true, state: PlayerState = .stopped,
        hasTrack: Bool = true
    ) -> SourceState {
        var s = SourceState()
        s.running = running
        s.playerState = state
        s.track = hasTrack ? TrackInfo(name: "T", artist: "A", album: "L") : nil
        return s
    }

    private func select(_ am: SourceState, current: MusicSourceID? = nil) -> MusicSourceID? {
        SourceSelector.selectActiveSource(appleMusic: am, current: current)
    }

    func testNothingRunningYieldsNil() {
        XCTAssertNil(select(source(running: false, hasTrack: false)))
    }

    func testPlayingIsACandidate() {
        XCTAssertEqual(select(source(state: .playing)), .appleMusic)
    }

    func testPausedIsStillACandidate() {
        // 一時停止も「表示すべき候補」ではある（消すかどうかはActivityBuilder側の判断）
        XCTAssertEqual(select(source(state: .paused)), .appleMusic)
    }

    func testStoppedSourceIsNotACandidate() {
        XCTAssertNil(select(source(state: .stopped, hasTrack: false)))
    }

    func testRunningWithoutTrackIsNotACandidate() {
        // 稼働していても曲情報がなければ表示しない（初期状態取得前など）
        XCTAssertNil(select(source(state: .playing, hasTrack: false)))
    }

    func testNotRunningIsNotACandidate() {
        XCTAssertNil(select(source(running: false, state: .playing)))
    }
}
