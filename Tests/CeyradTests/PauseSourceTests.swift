import XCTest

@testable import Ceyrad

final class PauseSourceTests: XCTestCase {
    func testOtherSourcesTrackChangeKeepsTimedOutPresenceHidden() {
        checkTrackChange(displayedTrackChanged: false)
    }

    func testDisplayedSourcesTrackChangeRestoresTimedOutPresence() {
        checkTrackChange(displayedTrackChanged: true)
    }

    private func checkTrackChange(displayedTrackChanged: Bool) {
        for displayed in [MusicSource.appleMusic, .youtubeMusic] {
            let timer = PauseHideTimer(secondsPerMinute: 0.01)
            let app = AppDelegate(rpc: SilentRPC(), pauseHide: timer)
            let previousMinutes = app.settings.pauseHideMinutes
            defer {
                app.settings.pauseHideMinutes = previousMinutes
                timer.cancel()
                app.debouncer.cancel()
            }
            app.settings.pauseHideMinutes = 1
            let earlier = Date(timeIntervalSince1970: 1)
            let later = Date(timeIntervalSince1970: 2)
            for source in [MusicSource.appleMusic, .youtubeMusic] {
                app.update(source) {
                    $0.running = true
                    $0.playerState = .paused
                    $0.track = TrackInfo(name: "Song", artist: "Artist", album: "Album")
                    $0.playingSince = source == displayed ? later : earlier
                }
            }
            app.reevaluate()
            XCTAssertEqual(app.displayedSource, displayed)
            let expired = expectation(description: "Pause timeout elapsed")
            DispatchQueue.main.asyncAfter(deadline: .now() + 0.05) { expired.fulfill() }
            wait(for: [expired], timeout: 1)
            XCTAssertTrue(timer.hasFired)
            let other: MusicSource = displayed == .appleMusic ? .youtubeMusic : .appleMusic
            app.reevaluate(trackChangedIn: displayedTrackChanged ? displayed : other)
            XCTAssertEqual(timer.hasFired, !displayedTrackChanged)
        }
    }
}
