import XCTest

@testable import Cadence

final class MusicAppleScriptTests: XCTestCase {
    private let sep = "\u{1F}"

    func testParsePlayingState() {
        let raw = ["playing", "Song", "Artist", "Album", "200.5", "12.25"].joined(separator: sep)
        let (state, track) = MusicAppleScript.parseState(raw)
        XCTAssertEqual(state, .playing)
        XCTAssertEqual(track?.name, "Song")
        XCTAssertEqual(track?.artist, "Artist")
        XCTAssertEqual(track?.album, "Album")
        XCTAssertEqual(track?.durationSec, 200.5)
        XCTAssertEqual(track?.positionSec, 12.25)
    }

    func testParsePausedStateWithCommaDecimals() {
        // ロケールによってAppleScriptの実数はカンマ小数点で返る
        let raw = ["paused", "Song", "Artist", "Album", "200,5", "12,25"].joined(separator: sep)
        let (state, track) = MusicAppleScript.parseState(raw)
        XCTAssertEqual(state, .paused)
        XCTAssertEqual(track?.durationSec, 200.5)
        XCTAssertEqual(track?.positionSec, 12.25)
    }

    func testParseStoppedStateHasNoTrack() {
        let (state, track) = MusicAppleScript.parseState("stopped")
        XCTAssertEqual(state, .stopped)
        XCTAssertNil(track)
    }

    func testUnknownStateFallsBackToStopped() {
        let (state, track) = MusicAppleScript.parseState("fast forwarding")
        XCTAssertEqual(state, .stopped)
        XCTAssertNil(track)
    }

    func testPlayingWithMissingFieldsHasNoTrack() {
        // トラック情報の取得に失敗した場合（try節が空振り）は状態のみ
        let (state, track) = MusicAppleScript.parseState("playing")
        XCTAssertEqual(state, .playing)
        XCTAssertNil(track)
    }

    func testParseDouble() {
        XCTAssertEqual(MusicAppleScript.parseDouble("3.5"), 3.5)
        XCTAssertEqual(MusicAppleScript.parseDouble("3,5"), 3.5)
        XCTAssertEqual(MusicAppleScript.parseDouble("0"), 0)
        XCTAssertNil(MusicAppleScript.parseDouble("abc"))
        XCTAssertNil(MusicAppleScript.parseDouble(""))
    }
}
