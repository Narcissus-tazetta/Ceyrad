import XCTest

@testable import Ceyrad

final class SourceSelectorTests: XCTestCase {
    private func state(
        _ playerState: PlayerState?, since: TimeInterval? = nil
    ) -> MusicState {
        var state = MusicState()
        guard let playerState else { return state }
        state.running = true
        state.playerState = playerState
        state.track = TrackInfo(name: "Song", artist: "Artist", album: "Album")
        state.playingSince = since.map { Date(timeIntervalSince1970: $0) }
        return state
    }

    private func select(_ apple: MusicState, _ youtube: MusicState) -> MusicSource? {
        SourceSelector.select(appleMusic: apple, youtubeMusic: youtube)
    }

    func testNothingDisplayableSelectsNothing() {
        XCTAssertNil(select(state(nil), state(nil)))
        XCTAssertNil(select(state(.stopped), state(.stopped)))
    }

    func testTheOnlyDisplayableSourceWins() {
        XCTAssertEqual(select(state(.paused), state(nil)), .appleMusic)
        XCTAssertEqual(select(state(nil), state(.paused)), .youtubeMusic)
    }

    func testPlayingBeatsPausedWhateverTheOrder() {
        XCTAssertEqual(select(state(.paused, since: 200), state(.playing, since: 100)), .youtubeMusic)
        XCTAssertEqual(select(state(.playing, since: 100), state(.paused, since: 200)), .appleMusic)
    }

    func testTheLaterPlayerWinsWhenBothPlay() {
        XCTAssertEqual(select(state(.playing, since: 100), state(.playing, since: 200)), .youtubeMusic)
        XCTAssertEqual(select(state(.playing, since: 200), state(.playing, since: 100)), .appleMusic)
    }

    func testTheMoreRecentlyPlayedWinsWhenBothArePaused() {
        XCTAssertEqual(select(state(.paused, since: 100), state(.paused, since: 200)), .youtubeMusic)
        XCTAssertEqual(select(state(.paused, since: 200), state(.paused, since: 100)), .appleMusic)
    }

    func testASourceThatNeverPlayedLosesATie() {
        XCTAssertEqual(select(state(.paused), state(.paused, since: 100)), .youtubeMusic)
        XCTAssertEqual(select(state(.paused, since: 100), state(.paused)), .appleMusic)
        XCTAssertEqual(select(state(.paused), state(.paused)), .appleMusic)
    }
}
