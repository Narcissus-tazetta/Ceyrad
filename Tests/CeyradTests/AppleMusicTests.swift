import XCTest

@testable import Ceyrad

final class AppleMusicTests: XCTestCase {
    func testPlayingWithAllKeys() {
        let (state, track) = AppleMusic.parse([
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
    }

    func testStoppedYieldsNoTrack() {
        let (state, track) = AppleMusic.parse(["Player State": "Stopped"])
        XCTAssertEqual(state, .stopped)
        XCTAssertNil(track)
    }

    func testPausedWithMissingKeys() {
        let (state, track) = AppleMusic.parse(["Player State": "Paused"])
        XCTAssertEqual(state, .paused)
        XCTAssertEqual(track?.name, "")
        XCTAssertNil(track?.durationSec)
    }

    func testIdentityJoinsFieldsWithAUnitSeparator() {
        // トラック変更検知・カタログ再解決の要求がここに依存するため、
        // 結合フォーマット自体を固定しておく。
        let (_, track) = AppleMusic.parse([
            "Player State": "Playing",
            "Name": "Song",
            "Artist": "Artist",
            "Album": "Album",
        ])
        XCTAssertEqual(track?.identity, "Song\u{1F}Artist\u{1F}Album")
    }
}
