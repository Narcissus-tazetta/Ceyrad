import XCTest

@testable import Ceyrad

final class MusicStateTests: XCTestCase {
    private func state(
        running: Bool = true, playerState: PlayerState = .stopped, hasTrack: Bool = true
    ) -> MusicState {
        var s = MusicState()
        s.running = running
        s.playerState = playerState
        s.track = hasTrack ? TrackInfo(name: "T", artist: "A", album: "L") : nil
        return s
    }

    func testNothingRunningIsNotDisplayable() {
        XCTAssertFalse(state(running: false, hasTrack: false).isDisplayable)
    }

    func testPlayingIsDisplayable() {
        XCTAssertTrue(state(playerState: .playing).isDisplayable)
    }

    func testPausedIsStillDisplayable() {
        // 一時停止も「表示すべき候補」ではある（消すかどうかはpauseHideMinutes側の判断）
        XCTAssertTrue(state(playerState: .paused).isDisplayable)
    }

    func testStoppedIsNotDisplayable() {
        XCTAssertFalse(state(playerState: .stopped, hasTrack: false).isDisplayable)
    }

    func testRunningWithoutTrackIsNotDisplayable() {
        // 稼働していても曲情報がなければ表示しない（初期状態取得前など）
        XCTAssertFalse(state(playerState: .playing, hasTrack: false).isDisplayable)
    }

    func testNotRunningIsNotDisplayable() {
        XCTAssertFalse(state(running: false, playerState: .playing).isDisplayable)
    }

    // MARK: - カタログの番人

    /// カタログを白紙に戻すときは照会の番人も一緒に落とす。
    /// 片方だけ残すと、次の曲が「もう訊いた」と誤判定されて永久にアートワークを得られない。
    func testClearingCatalogAlsoClearsTheRequestGuard() {
        var s = state(playerState: .playing)
        s.catalog = CatalogInfo(artworkURL: "https://example.com/a.jpg")
        s.catalogRequestedFor = "T\u{1F}A\u{1F}L"
        s.catalogRetryAt = Date()

        s.clearCatalog()

        XCTAssertNil(s.catalog)
        XCTAssertNil(s.catalogRequestedFor)
        XCTAssertNil(s.catalogRetryAt)
    }
}
