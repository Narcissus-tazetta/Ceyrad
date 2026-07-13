import XCTest

@testable import Ceyrad

final class AppleMusicNotificationTests: XCTestCase {
    func testPlayingWithAllKeys() {
        let (state, track) = AppleMusicNotification.parse([
            "Player State": "Playing",
            "Name": "Song",
            "Artist": "Artist",
            "Album": "Album",
            "Total Time": NSNumber(value: 200_786),
        ])
        XCTAssertEqual(state, .playing)
        XCTAssertEqual(track?.name, "Song")
        XCTAssertEqual(track?.artist, "Artist")
        XCTAssertEqual(track?.album, "Album")
        // Total Timeはミリ秒
        XCTAssertEqual(track?.durationSec ?? 0, 200.786, accuracy: 0.001)
        XCTAssertNil(track?.positionSec)
        XCTAssertNil(track?.trackId)
    }

    func testStoppedYieldsNoTrack() {
        let (state, track) = AppleMusicNotification.parse(["Player State": "Stopped"])
        XCTAssertEqual(state, .stopped)
        XCTAssertNil(track)
    }

    func testPausedWithMissingKeys() {
        let (state, track) = AppleMusicNotification.parse(["Player State": "Paused"])
        XCTAssertEqual(state, .paused)
        XCTAssertEqual(track?.name, "")
        XCTAssertNil(track?.durationSec)
    }
}

final class SpotifyNotificationTests: XCTestCase {
    private let fullInfo: [AnyHashable: Any] = [
        "Player State": "Playing",
        "Name": "Sorry",
        "Artist": "Justin Bieber",
        "Album": "Purpose (Deluxe)",
        "Duration": NSNumber(value: 200_786),
        "Playback Position": NSNumber(value: 70.335),
        "Track ID": "spotify:track:69bp2EbF7Q2rqc5N3ylezZ",
    ]

    func testPlayingWithAllKeys() {
        let (state, track) = SpotifyNotification.parse(fullInfo)
        XCTAssertEqual(state, .playing)
        XCTAssertEqual(track?.name, "Sorry")
        XCTAssertEqual(track?.artist, "Justin Bieber")
        XCTAssertEqual(track?.album, "Purpose (Deluxe)")
        // Durationはミリ秒、Playback Positionは秒
        XCTAssertEqual(track?.durationSec ?? 0, 200.786, accuracy: 0.001)
        XCTAssertEqual(track?.positionSec ?? 0, 70.335, accuracy: 0.001)
        XCTAssertEqual(track?.trackId, "spotify:track:69bp2EbF7Q2rqc5N3ylezZ")
    }

    func testTrackIdBecomesIdentity() {
        let (_, track) = SpotifyNotification.parse(fullInfo)
        XCTAssertEqual(track?.identity, "spotify:track:69bp2EbF7Q2rqc5N3ylezZ")
    }

    func testPaused() {
        var info = fullInfo
        info["Player State"] = "Paused"
        let (state, track) = SpotifyNotification.parse(info)
        XCTAssertEqual(state, .paused)
        XCTAssertNotNil(track)
    }

    func testStoppedYieldsNoTrack() {
        let (state, track) = SpotifyNotification.parse(["Player State": "Stopped"])
        XCTAssertEqual(state, .stopped)
        XCTAssertNil(track)
    }

    func testMissingPositionAndDuration() {
        let (state, track) = SpotifyNotification.parse([
            "Player State": "Playing",
            "Name": "Song",
        ])
        XCTAssertEqual(state, .playing)
        XCTAssertNil(track?.positionSec)
        XCTAssertNil(track?.durationSec)
        // Track IDなしはname/artist/album結合のidentityにフォールバック
        XCTAssertEqual(track?.identity, "Song\u{1F}\u{1F}")
    }

    func testAdTrackStillParses() {
        var info = fullInfo
        info["Track ID"] = "spotify:ad:000000012c603a6600000020317a4b12"
        let (state, track) = SpotifyNotification.parse(info)
        XCTAssertEqual(state, .playing)
        XCTAssertEqual(track?.trackId, "spotify:ad:000000012c603a6600000020317a4b12")
    }
}
