import XCTest

@testable import Ceyrad

final class StatusLinesBuilderTests: XCTestCase {
    private func input(
        running: Bool = true, playerState: PlayerState = .playing,
        track: TrackInfo? = TrackInfo(name: "Song", artist: "Artist", album: "Album"),
        notAuthorized: Bool = false,
        discord: DiscordRPCClient.ConnState = .connected,
        language: AppLanguage = .en
    ) -> StatusLinesBuilder.Input {
        var music = MusicState()
        music.running = running
        music.playerState = playerState
        music.track = track
        return StatusLinesBuilder.Input(
            music: music, musicNotAuthorized: notAuthorized,
            discordState: discord, language: language
        )
    }

    func testNotRunning() {
        let lines = StatusLinesBuilder.lines(input(running: false))
        XCTAssertEqual(lines.first, "Apple Music: Not Running")
        // プレイヤーが動いていなければ接続もしない
        XCTAssertEqual(lines.last, "Discord: Idle (connects when a player starts)")
    }

    func testPlayingShowsTrackAndArtist() {
        XCTAssertEqual(StatusLinesBuilder.lines(input()).first, "♪ Apple Music: Song — Artist")
    }

    func testPausedUsesItsOwnGlyph() {
        XCTAssertEqual(
            StatusLinesBuilder.lines(input(playerState: .paused)).first,
            "⏸ Apple Music: Song — Artist"
        )
    }

    func testTrackWithoutArtistOmitsTheDash() {
        let track = TrackInfo(name: "Song", artist: "", album: "")
        XCTAssertEqual(StatusLinesBuilder.lines(input(track: track)).first, "♪ Apple Music: Song")
    }

    func testRunningWithoutTrackShowsAPlaceholder() {
        XCTAssertEqual(StatusLinesBuilder.lines(input(track: nil)).first, "♪ Apple Music: –")
    }

    /// オートメーション未許可の警告は、プレイヤーが動いているときだけ意味を持つ。
    func testAuthorizationWarningOnlyWhileRunning() {
        let warned = StatusLinesBuilder.lines(input(notAuthorized: true))
        XCTAssertTrue(warned.contains { $0.contains("Not Authorized") })
        XCTAssertEqual(warned.count, 4, "警告は2行（本文＋設定場所）")

        let idle = StatusLinesBuilder.lines(input(running: false, notAuthorized: true))
        XCTAssertFalse(idle.contains { $0.contains("Not Authorized") })
    }

    /// Discordを名指しする文言は翻訳しない。Discord側の表記に合わせるため。
    func testDiscordLinesAreNeverLocalized() {
        for state in [DiscordRPCClient.ConnState.connected, .connecting, .disconnected] {
            let en = StatusLinesBuilder.lines(input(discord: state, language: .en)).last
            let ja = StatusLinesBuilder.lines(input(discord: state, language: .ja)).last
            XCTAssertEqual(en, ja)
        }
    }

    func testPlayerLinesAreLocalized() {
        XCTAssertEqual(
            StatusLinesBuilder.lines(input(running: false, language: .ja)).first,
            "Apple Music: 未起動"
        )
    }
}
