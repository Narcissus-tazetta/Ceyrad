import XCTest

@testable import Ceyrad

final class YouTubeMusicTests: XCTestCase {
    // MARK: - ブラウザ

    func testKnownBrowsersAreBrowsersAndMusicIsNot() {
        XCTAssertTrue(YouTubeMusic.isBrowser(bundleId: "com.google.Chrome"))
        XCTAssertTrue(YouTubeMusic.isBrowser(bundleId: "com.apple.Safari"))
        XCTAssertTrue(YouTubeMusic.isBrowser(bundleId: "org.mozilla.firefox"))
        XCTAssertFalse(YouTubeMusic.isBrowser(bundleId: AppleMusic.bundleId))
    }

    // MARK: - タブ

    private func tab(_ url: String, _ title: String = "") -> BrowserTab {
        BrowserTab(url: url, title: title)
    }

    func testSingleWatchTabGivesACanonicalSongURL() {
        let tabs = [
            tab("https://github.com/Narcissus-tazetta/Ceyrad", "Ceyrad"),
            tab("https://music.youtube.com/watch?v=abc123&list=RDAMVMabc123&si=xyz", "pupa | YouTube Music"),
        ]
        XCTAssertEqual(
            YouTubeMusic.songURL(fromTabs: tabs, trackTitle: "pupa"),
            "https://music.youtube.com/watch?v=abc123")
    }

    func testNoSongURLWhenTheTabIsNotAWatchPage() {
        // 再生はホームや検索を開いたままでも続くので、そのURLは曲を指さない
        XCTAssertNil(
            YouTubeMusic.songURL(fromTabs: [tab("https://music.youtube.com/", "YouTube Music")], trackTitle: "pupa"))
        XCTAssertNil(
            YouTubeMusic.songURL(
                fromTabs: [tab("https://music.youtube.com/search?q=watch?v=abc")], trackTitle: "pupa"))
    }

    func testTheTabWhoseTitleMatchesTheTrackIsTheOneThatPlays() {
        let tabs = [
            tab("https://music.youtube.com/watch?v=aaa", "pupa | YouTube Music"),
            tab("https://music.youtube.com/watch?v=bbb", "劇場 | YouTube Music"),
        ]
        XCTAssertEqual(
            YouTubeMusic.songURL(fromTabs: tabs, trackTitle: "pupa"),
            "https://music.youtube.com/watch?v=aaa")
    }

    func testStaleTabsGiveASongURLOnlyWhenTheTitleHasCaughtUp() {
        // Firefox のセッションファイルは遅れて書かれ、曲が替わっても前の曲の URL が残る
        let stale = [tab("https://music.youtube.com/watch?v=old", "YouTube Music")]
        XCTAssertNil(YouTubeMusic.songURL(fromTabs: stale, trackTitle: "pupa", requireTitleMatch: true))
        let caughtUp = [tab("https://music.youtube.com/watch?v=new", "pupa | YouTube Music")]
        XCTAssertEqual(
            YouTubeMusic.songURL(fromTabs: caughtUp, trackTitle: "pupa", requireTitleMatch: true),
            "https://music.youtube.com/watch?v=new")
    }

    func testThumbnailComesFromTheSongPagesVideo() {
        XCTAssertEqual(
            YouTubeMusic.thumbnailURL(songURL: "https://music.youtube.com/watch?v=VbfpW0pbvaU"),
            "https://i.ytimg.com/vi/VbfpW0pbvaU/mqdefault.jpg")
        XCTAssertNil(YouTubeMusic.thumbnailURL(songURL: "https://music.youtube.com/watch"))
    }

    func testNoSongURLWhenSeveralTabsCannotBeToldApart() {
        let tabs = [
            tab("https://music.youtube.com/watch?v=aaa", "YouTube Music"),
            tab("https://music.youtube.com/watch?v=bbb", "YouTube Music"),
        ]
        XCTAssertNil(YouTubeMusic.songURL(fromTabs: tabs, trackTitle: "pupa"), "どのタブが鳴っているか分からない")
        XCTAssertEqual(YouTubeMusic.musicTabs(tabs).count, 2, "タブがあること自体は分かる")
    }

    func testOtherYouTubeHostsAreNotMusicTabs() {
        let tabs = [
            tab("https://www.youtube.com/watch?v=abc"),
            tab("https://music.youtube.com.evil.example/watch?v=abc"),
            tab("https://example.com/?u=music.youtube.com"),
        ]
        XCTAssertTrue(YouTubeMusic.musicTabs(tabs).isEmpty)
    }

    func testTitleMatchNeedsTheExactTrackAndTheSuffix() {
        let playing = tab("https://music.youtube.com/watch?v=abc", "pupa | YouTube Music")
        XCTAssertTrue(YouTubeMusic.titleMatches(playing, trackTitle: "pupa"))
        XCTAssertTrue(
            YouTubeMusic.titleMatches(
                tab("https://music.youtube.com/watch?v=abc", "pupa - YouTube Music"), trackTitle: "pupa"))
        XCTAssertFalse(YouTubeMusic.titleMatches(playing, trackTitle: "pup"))
        XCTAssertFalse(YouTubeMusic.titleMatches(playing, trackTitle: ""))
        XCTAssertFalse(
            YouTubeMusic.titleMatches(tab("https://music.youtube.com/", "YouTube Music"), trackTitle: "pupa"))
        // 通常のYouTube動画のタイトルは「動画名 - YouTube」で、サフィックスが違う
        XCTAssertFalse(
            YouTubeMusic.titleMatches(
                tab("https://www.youtube.com/watch?v=abc", "pupa - YouTube"), trackTitle: "pupa"))
    }

    // MARK: - macOS の判定

    func testAnAlbumWithAMusicTabIsYouTubeMusic() {
        let tabs = [tab("https://music.youtube.com/", "YouTube Music")]
        XCTAssertTrue(
            YouTubeMusic.isYouTubeMusic(isBrowser: true, album: "pupa", trackTitle: "pupa", tabs: tabs))
    }

    /// ページを読み込み直した直後の曲にはアルバム名が付かない（実機で確認）。
    func testNoAlbumIsRescuedByTheTabTitle() {
        let onPlayerPage = [tab("https://music.youtube.com/watch?v=abc", "pupa | YouTube Music")]
        XCTAssertTrue(
            YouTubeMusic.isYouTubeMusic(isBrowser: true, album: "", trackTitle: "pupa", tabs: onPlayerPage))

        // ホームを開いていてタイトルに曲名がないときは、拾わない（誤表示より取りこぼしを選ぶ）
        let onHome = [tab("https://music.youtube.com/", "YouTube Music")]
        XCTAssertFalse(
            YouTubeMusic.isYouTubeMusic(isBrowser: true, album: "", trackTitle: "pupa", tabs: onHome))
    }

    func testAPlainYouTubeVideoIsNeverRescued() {
        let tabs = [
            tab("https://www.youtube.com/watch?v=abc", "pupa - YouTube"),
            tab("https://music.youtube.com/", "YouTube Music"),
        ]
        XCTAssertFalse(
            YouTubeMusic.isYouTubeMusic(isBrowser: true, album: "", trackTitle: "pupa", tabs: tabs))
    }

    func testNothingIsRescuedWithoutAMusicTabOrABrowser() {
        let titleOnly = [tab("https://example.com/", "pupa | YouTube Music")]
        XCTAssertFalse(
            YouTubeMusic.isYouTubeMusic(isBrowser: true, album: "", trackTitle: "pupa", tabs: titleOnly))
        let tabs = [tab("https://music.youtube.com/watch?v=abc", "pupa | YouTube Music")]
        XCTAssertFalse(
            YouTubeMusic.isYouTubeMusic(isBrowser: false, album: "pupa", trackTitle: "pupa", tabs: tabs))
    }

    // MARK: - アダプタの出力

    /// 実機（macOS 27.2, Chrome）で `stream --no-artwork --no-diff` から取った行。
    private let playing = """
        {"type":"data","diff":false,"payload":{"artist":"natori","playbackRate":1,\
        "contentItemIdentifier":"E5E08094","title":"pupa","elapsedTime":3.1028410000000002,\
        "duration":208.76312899999999,"playing":true,"bundleIdentifier":"com.google.Chrome",\
        "album":"pupa","processIdentifier":93922,"timestamp":"2026-11-02T09:18:46Z"}}
        """

    func testParsesAPlayingTrack() throws {
        let payload = try XCTUnwrap(NowPlayingPayload.parse(line: playing))
        XCTAssertEqual(payload.bundleId, "com.google.Chrome")
        XCTAssertEqual(payload.title, "pupa")
        XCTAssertEqual(payload.artist, "natori")
        XCTAssertEqual(payload.album, "pupa")
        XCTAssertEqual(payload.playerState, .playing)

        let track = payload.trackInfo
        XCTAssertEqual(track.durationSec ?? 0, 208.763, accuracy: 0.001)
        XCTAssertEqual(track.positionSec ?? 0, 3.103, accuracy: 0.001)
        XCTAssertEqual(
            track.positionSampledAt, ISO8601DateFormatter().date(from: "2026-11-02T09:18:46Z"))
    }

    func testSafariIsReportedByItsParentApplication() throws {
        let line = """
            {"type":"data","diff":false,"payload":{"artist":"Mrs. GREEN APPLE","album":"POPS",\
            "title":"シンデレラ","playing":true,"bundleIdentifier":"com.apple.WebKit.GPU",\
            "parentApplicationBundleIdentifier":"com.apple.Safari"}}
            """
        let payload = try XCTUnwrap(NowPlayingPayload.parse(line: line))
        XCTAssertEqual(payload.bundleId, "com.apple.Safari")
    }

    func testPausedIsPlayingFalse() throws {
        let line = playing.replacingOccurrences(of: "\"playing\":true", with: "\"playing\":false")
        XCTAssertEqual(try NowPlayingPayload.parse(line: line)?.playerState, .paused)
    }

    func testEmptyPayloadMeansNothingIsPlaying() throws {
        XCTAssertNil(try NowPlayingPayload.parse(line: #"{"type":"data","diff":false,"payload":{}}"#))
    }

    /// 最初に届く更新は曲名だけで、アーティストやアルバムは数秒遅れる。
    func testTitleOnlyPayloadHasEmptyAlbum() throws {
        let line = """
            {"type":"data","diff":false,"payload":{"title":"pupa","contentItemIdentifier":"X",\
            "bundleIdentifier":"com.google.Chrome","playing":true,"timestamp":"2026-11-02T09:18:42Z"}}
            """
        let payload = try XCTUnwrap(NowPlayingPayload.parse(line: line))
        XCTAssertEqual(payload.album, "")
        XCTAssertNil(payload.durationSec)
    }

    func testIdentityIgnoresPlaybackButNotTheTrack() throws {
        let a = try XCTUnwrap(NowPlayingPayload.parse(line: playing))
        var b = a
        b.playing = false
        b.elapsedSec = 90
        XCTAssertEqual(a.identity, b.identity)
        b.title = "other"
        XCTAssertNotEqual(a.identity, b.identity)
    }

    /// ライブ配信などは長さが無限大や桁外れで届きうる。そのまま進捗バーに流すと整数変換で落ちる。
    func testImplausibleDurationsAndPositionsAreDropped() throws {
        var payload = try XCTUnwrap(NowPlayingPayload.parse(line: playing))
        for duration in [Double.infinity, 1e16, -5, 0] {
            payload.durationSec = duration
            XCTAssertNil(payload.trackInfo.durationSec, "\(duration)")
        }
        payload.durationSec = 3600
        XCTAssertEqual(payload.trackInfo.durationSec, 3600)

        for position in [Double.nan, Double.infinity, -1] {
            payload.elapsedSec = position
            XCTAssertNil(payload.trackInfo.positionSec, "\(position)")
        }
    }

    func testMalformedOutputIsAnErrorNotSilence() {
        XCTAssertThrowsError(try NowPlayingPayload.parse(line: "not json"))
        XCTAssertThrowsError(try NowPlayingPayload.parse(line: #"{"type":"data"}"#))
    }
}
