import XCTest

@testable import Ceyrad

final class SpotifyCatalogClientTests: XCTestCase {
    func testTrackURLFromValidTrackId() {
        XCTAssertEqual(
            SpotifyCatalogClient.trackURL(fromTrackId: "spotify:track:69bp2EbF7Q2rqc5N3ylezZ"),
            "https://open.spotify.com/track/69bp2EbF7Q2rqc5N3ylezZ"
        )
    }

    func testAdAndLocalIdsYieldNil() {
        XCTAssertNil(SpotifyCatalogClient.trackURL(fromTrackId: "spotify:ad:000000012c603a66"))
        XCTAssertNil(SpotifyCatalogClient.trackURL(fromTrackId: "spotify:local:artist:album:song:123"))
        XCTAssertNil(SpotifyCatalogClient.trackURL(fromTrackId: "spotify:episode:abc123"))
    }

    func testMalformedIdsYieldNil() {
        XCTAssertNil(SpotifyCatalogClient.trackURL(fromTrackId: ""))
        XCTAssertNil(SpotifyCatalogClient.trackURL(fromTrackId: "spotify:track:"))
        // URLに埋め込むためID部は英数字のみ許可する
        XCTAssertNil(SpotifyCatalogClient.trackURL(fromTrackId: "spotify:track:abc/../def"))
        XCTAssertNil(SpotifyCatalogClient.trackURL(fromTrackId: "notaspotifyid"))
    }
}

final class SpotifyAppleScriptTests: XCTestCase {
    func testParseStatePlaying() {
        let sep = "\u{1F}"
        let raw = ["playing", "Song", "Artist", "Album", "200786", "70,335", "spotify:track:abc"]
            .joined(separator: sep)
        let (state, track) = SpotifyAppleScript.parseState(raw)
        XCTAssertEqual(state, .playing)
        XCTAssertEqual(track?.name, "Song")
        // durationはミリ秒→秒。positionはロケールによりカンマ小数点になり得る
        XCTAssertEqual(track?.durationSec ?? 0, 200.786, accuracy: 0.001)
        XCTAssertEqual(track?.positionSec ?? 0, 70.335, accuracy: 0.001)
        XCTAssertEqual(track?.trackId, "spotify:track:abc")
    }

    func testParseStateStopped() {
        let (state, track) = SpotifyAppleScript.parseState("stopped")
        XCTAssertEqual(state, .stopped)
        XCTAssertNil(track)
    }

    func testParseStateWithMissingFieldsYieldsNoTrack() {
        let (state, track) = SpotifyAppleScript.parseState("playing\u{1F}Song")
        XCTAssertEqual(state, .playing)
        XCTAssertNil(track)
    }

    func testParseArtworkRequiresMatchingTrackIdAndHTTPS() {
        let sep = "\u{1F}"
        XCTAssertEqual(
            SpotifyAppleScript.parseArtwork(
                "spotify:track:abc\(sep)https://i.scdn.co/image/xyz", expectedTrackId: "spotify:track:abc"
            ),
            "https://i.scdn.co/image/xyz"
        )
        // 曲が替わっていたら返さない
        XCTAssertNil(
            SpotifyAppleScript.parseArtwork(
                "spotify:track:other\(sep)https://i.scdn.co/image/xyz", expectedTrackId: "spotify:track:abc"
            )
        )
        // https以外は返さない
        XCTAssertNil(
            SpotifyAppleScript.parseArtwork(
                "spotify:track:abc\(sep)http://i.scdn.co/image/xyz", expectedTrackId: "spotify:track:abc"
            )
        )
        // エラー時の空文字
        XCTAssertNil(SpotifyAppleScript.parseArtwork("", expectedTrackId: "spotify:track:abc"))
    }
}
