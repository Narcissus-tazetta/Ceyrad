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

    func testAdvertisementExplainsWhyNothingIsShown() {
        let ad = TrackInfo(name: "1648108987", artist: "", album: "")
        let lines = StatusLinesBuilder.lines(input(playerState: .paused, track: ad))
        XCTAssertEqual(lines.first, "Apple Music: Advertisement (not shown)")
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
    // MARK: - YouTube Music

    private func youtubeInput(
        enabled: Bool = true, running: Bool = true, unavailable: String? = nil,
        playerState: PlayerState = .playing,
        track: TrackInfo? = TrackInfo(name: "pupa", artist: "natori", album: "pupa"),
        browserNotAuthorized: Bool = false
    ) -> StatusLinesBuilder.Input {
        var youtube = MusicState()
        youtube.running = running
        youtube.playerState = playerState
        youtube.track = track
        return StatusLinesBuilder.Input(
            youtube: youtube, youtubeEnabled: enabled, youtubeUnavailable: unavailable,
            browserNotAuthorized: browserNotAuthorized
        )
    }

    /// 検知がオフなら、機能の存在そのものをメニューに出さない。
    func testDisabledYouTubeMusicAddsNoLines() {
        let off = StatusLinesBuilder.lines(youtubeInput(enabled: false))
        XCTAssertFalse(off.contains { $0.contains("YouTube Music") })
    }

    func testYouTubeMusicStates() {
        XCTAssertTrue(
            StatusLinesBuilder.lines(youtubeInput()).contains("♪ YouTube Music: pupa — natori"))
        XCTAssertTrue(
            StatusLinesBuilder.lines(youtubeInput(playerState: .paused))
                .contains("⏸ YouTube Music: pupa — natori"))
        XCTAssertTrue(
            StatusLinesBuilder.lines(youtubeInput(track: nil))
                .contains("YouTube Music: Waiting for playback"))
        XCTAssertTrue(
            StatusLinesBuilder.lines(youtubeInput(running: false))
                .contains("YouTube Music: Starting…"))
    }

    /// 使えない理由は黙って落とさず、メニューに残す。
    func testUnavailableYouTubeMusicSaysWhy() {
        let lines = StatusLinesBuilder.lines(
            youtubeInput(running: false, unavailable: "the adapter is not bundled"))
        XCTAssertTrue(lines.contains("YouTube Music: Unavailable ⚠️"))
        XCTAssertTrue(lines.contains("(the adapter is not bundled)"))
    }

    func testBrowserAuthorizationWarning() {
        let lines = StatusLinesBuilder.lines(youtubeInput(browserNotAuthorized: true))
        XCTAssertTrue(lines.contains("Browser Control: Not Authorized ⚠️"))
    }

    /// Firefox はオートメーションではなく「ファイルとフォルダ」の許可なので、案内先が違う。
    func testFirefoxDataAuthorizationWarningPointsToFilesAndFolders() {
        var input = youtubeInput()
        input.firefoxDataNotAuthorized = true
        let lines = StatusLinesBuilder.lines(input)
        XCTAssertTrue(lines.contains("Firefox Data: Not Authorized ⚠️"))
        XCTAssertTrue(lines.contains("(System Settings > Privacy > Files & Folders)"))
        XCTAssertFalse(lines.contains("Browser Control: Not Authorized ⚠️"))
    }

    /// YouTube Music だけが再生中でも、Discord は「待機」ではなく接続状態を出す。
    func testDiscordIsNotIdleWhileYouTubeMusicPlays() {
        var input = youtubeInput()
        input.discordState = .connected
        XCTAssertEqual(StatusLinesBuilder.lines(input).last, "Discord: Connected")
    }
}
