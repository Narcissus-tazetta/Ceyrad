import XCTest

@testable import Ceyrad

final class YouTubeMusicFallbackTests: XCTestCase {
    func testOnlyTheArtworkIsBorrowedFromITunes() {
        let info = CatalogInfo(
            songURL: "https://music.apple.com/s", artistURL: "a", albumURL: "b", artworkURL: "art")
        XCTAssertEqual(
            AppDelegate.youtubeMusicFallback(.found(info), youtubeFailed: false),
            .found(CatalogInfo(artworkURL: "art")))
    }

    func testMissingIsFinalOnlyWhenYouTubeMusicAnswered() {
        XCTAssertEqual(AppDelegate.youtubeMusicFallback(.missing, youtubeFailed: false), .missing)
        // 片方が「訊けなかった」なら、もう片方が無かっただけでは確定した「無い」にしない
        XCTAssertEqual(AppDelegate.youtubeMusicFallback(.missing, youtubeFailed: true), .failed)
    }

    func testAFailedITunesLookupStaysFailed() {
        XCTAssertEqual(AppDelegate.youtubeMusicFallback(.failed, youtubeFailed: false), .failed)
    }
}
