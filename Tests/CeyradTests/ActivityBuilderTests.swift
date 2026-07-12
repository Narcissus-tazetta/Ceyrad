import XCTest

@testable import Ceyrad

final class ActivityBuilderTests: XCTestCase {
    private static let suiteName = "CeyradTests.ActivityBuilder"
    private var settings: SettingsStore!
    private var defaults: UserDefaults!

    override func setUp() {
        super.setUp()
        defaults = UserDefaults(suiteName: Self.suiteName)!
        defaults.removePersistentDomain(forName: Self.suiteName)
        settings = SettingsStore(defaults: defaults)
    }

    override func tearDown() {
        defaults.removePersistentDomain(forName: Self.suiteName)
        super.tearDown()
    }

    private func track(
        name: String = "Song", artist: String = "Artist", album: String = "Album",
        duration: Double? = 200, position: Double? = 50
    ) -> TrackInfo {
        TrackInfo(name: name, artist: artist, album: album, durationSec: duration, positionSec: position)
    }

    private func catalog(
        song: String? = "https://music.apple.com/song",
        artwork: String? = "https://example.com/art.jpg"
    ) -> CatalogInfo {
        CatalogInfo(songURL: song, artistURL: nil, albumURL: nil, artworkURL: artwork)
    }

    // MARK: - 基本フィールド

    func testListeningTypeAndBasicFields() {
        let activity = ActivityBuilder.build(
            track: track(), playerState: .playing, catalog: nil, settings: settings
        )
        XCTAssertEqual(activity["type"] as? Int, 2)
        XCTAssertEqual(activity["details"] as? String, "Song")
        XCTAssertEqual(activity["state"] as? String, "Artist")
    }

    func testShortStringsArePaddedToTwoCharacters() {
        let activity = ActivityBuilder.build(
            track: track(name: "A", artist: ""), playerState: .playing,
            catalog: nil, settings: settings
        )
        let details = activity["details"] as? String ?? ""
        XCTAssertEqual(details.count, 2)
        XCTAssertTrue(details.hasPrefix("A"))
        // 空白パディングはDiscord側でトリムされうるため使わない
        XCTAssertFalse(details.hasSuffix(" "))
    }

    func testLongStringsAreClampedTo128Characters() {
        let long = String(repeating: "あ", count: 300)
        let activity = ActivityBuilder.build(
            track: track(name: long), playerState: .playing, catalog: nil, settings: settings
        )
        XCTAssertEqual((activity["details"] as? String)?.count, 128)
    }

    // MARK: - タイムスタンプ

    func testTimestampsOnlyWhilePlaying() {
        let playing = ActivityBuilder.build(
            track: track(), playerState: .playing, catalog: nil, settings: settings
        )
        let stamps = playing["timestamps"] as? [String: Int]
        XCTAssertNotNil(stamps)
        let start = stamps?["start"] ?? 0
        let end = stamps?["end"] ?? 0
        // duration 200s ぶんの幅がある（msエポック）
        XCTAssertEqual(end - start, 200_000)

        settings.pauseHideMinutes = -1
        let paused = ActivityBuilder.build(
            track: track(), playerState: .paused, catalog: nil, settings: settings
        )
        XCTAssertNil(paused["timestamps"])
    }

    func testNoTimestampsWithoutPositionOrDuration() {
        let activity = ActivityBuilder.build(
            track: track(duration: nil, position: nil), playerState: .playing,
            catalog: nil, settings: settings
        )
        XCTAssertNil(activity["timestamps"])
    }

    // MARK: - 一時停止表示

    func testPausedLabelGoesToAlbumLineWithArtwork() {
        let activity = ActivityBuilder.build(
            track: track(position: 168), playerState: .paused,
            catalog: catalog(), settings: settings
        )
        let assets = activity["assets"] as? [String: Any]
        XCTAssertEqual(assets?["large_text"] as? String, "Album · ⏸ Paused at 2:48")
        XCTAssertEqual(activity["state"] as? String, "Artist")
    }

    func testPausedLabelGoesToArtistLineWithoutArtwork() {
        let activity = ActivityBuilder.build(
            track: track(position: 3725), playerState: .paused,
            catalog: nil, settings: settings
        )
        XCTAssertEqual(activity["state"] as? String, "⏸ Paused at 1:02:05 · Artist")
    }

    func testPausedWithoutPositionOmitsTime() {
        let activity = ActivityBuilder.build(
            track: track(position: nil), playerState: .paused,
            catalog: nil, settings: settings
        )
        XCTAssertEqual(activity["state"] as? String, "⏸ Paused · Artist")
    }

    // MARK: - ボタン

    func testDuplicateButtonURLsAreDeduplicated() {
        // 既定はbutton1=song / button2=repository。カタログ未解決だと
        // songはリポジトリURLへフォールバックし、両者が同一URLになる
        let activity = ActivityBuilder.build(
            track: track(), playerState: .playing, catalog: nil, settings: settings
        )
        let buttons = activity["buttons"] as? [[String: String]]
        XCTAssertEqual(buttons?.count, 1)
        XCTAssertEqual(buttons?.first?["url"], SettingsStore.defaultRepositoryURL)
    }

    func testTwoButtonsWithResolvedCatalog() {
        let activity = ActivityBuilder.build(
            track: track(), playerState: .playing, catalog: catalog(), settings: settings
        )
        let buttons = activity["buttons"] as? [[String: String]]
        XCTAssertEqual(buttons?.count, 2)
        XCTAssertEqual(buttons?.first?["url"], "https://music.apple.com/song")
        XCTAssertEqual(buttons?.first?["label"], "Play on Apple Music")
    }

    func testInvalidCustomURLIsSkipped() {
        settings.button1Type = .custom
        settings.button2Type = .disabled
        settings.customURL = "ftp://example.com"
        let activity = ActivityBuilder.build(
            track: track(), playerState: .playing, catalog: nil, settings: settings
        )
        XCTAssertNil(activity["buttons"])
    }

    func testButtonLabelIsTruncatedTo32Characters() {
        settings.button2Type = .disabled
        settings.button1Label = String(repeating: "x", count: 64)
        let activity = ActivityBuilder.build(
            track: track(), playerState: .playing, catalog: catalog(), settings: settings
        )
        let label = (activity["buttons"] as? [[String: String]])?.first?["label"]
        XCTAssertEqual(label?.count, 32)
    }

    // MARK: - URLバリデーション

    func testIsValidButtonURL() {
        XCTAssertTrue(ActivityBuilder.isValidButtonURL("https://example.com"))
        XCTAssertTrue(ActivityBuilder.isValidButtonURL("http://example.com/path?q=1"))
        XCTAssertFalse(ActivityBuilder.isValidButtonURL("ftp://example.com"))
        XCTAssertFalse(ActivityBuilder.isValidButtonURL("javascript:alert(1)"))
        XCTAssertFalse(ActivityBuilder.isValidButtonURL(""))
        XCTAssertFalse(
            ActivityBuilder.isValidButtonURL("https://example.com/" + String(repeating: "a", count: 512))
        )
    }
}
