import XCTest

@testable import Ceyrad

final class SettingsStoreTests: XCTestCase {
    private static let suiteName = "CeyradTests.SettingsStore"
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

    func testDefaults() {
        XCTAssertEqual(settings.button1Type, .song)
        XCTAssertEqual(settings.button1Label, "Play on Apple Music")
        XCTAssertEqual(settings.button2Type, .repository)
        XCTAssertEqual(settings.button2Label, "About This App")
        XCTAssertEqual(settings.pauseHideMinutes, 5)
        XCTAssertEqual(settings.repositoryURL, SettingsStore.defaultRepositoryURL)
    }

    func testLabelFollowsTypeChangeWhenNotCustomized() {
        settings.button1Type = .artist
        XCTAssertEqual(settings.button1Label, "View Artist")
    }

    func testCustomLabelSurvivesTypeChange() {
        settings.button1Label = "My Label"
        settings.button1Type = .artist
        XCTAssertEqual(settings.button1Label, "My Label")
    }

    func testSettingLabelToDefaultResumesFollowing() {
        settings.button1Label = "My Label"
        // 既定値と同じ文字列を入れたらカスタム扱いを解除し、以後はリンク先に追従する
        settings.button1Label = settings.button1Type.defaultLabel(for: nil)
        settings.button1Type = .album
        XCTAssertEqual(settings.button1Label, "View Album")
    }

    func testEmptyLabelResumesFollowing() {
        settings.button1Label = "My Label"
        settings.button1Label = ""
        XCTAssertEqual(settings.button1Label, settings.button1Type.defaultLabel(for: nil))
    }

    func testLabelIsTruncatedTo32Characters() {
        settings.button1Label = String(repeating: "x", count: 64)
        XCTAssertEqual(settings.button1Label.count, 32)
    }

    // MARK: - ソース別ラベル

    func testSongLabelFollowsSource() {
        XCTAssertEqual(settings.button1Label(for: .appleMusic), "Play on Apple Music")
        XCTAssertEqual(settings.button1Label(for: .spotify), "Play on Spotify")
        // カスタムラベルはソースに関係なく優先される
        settings.button1Label = "My Label"
        XCTAssertEqual(settings.button1Label(for: .spotify), "My Label")
    }

    func testSpotifyDefaultLabelIsNotTreatedAsCustom() {
        // "Play on Spotify" も.songの既定値扱いなので、カスタム扱いにならず追従が続く
        settings.button1Label = "Play on Spotify"
        settings.button1Type = .artist
        XCTAssertEqual(settings.button1Label, "View Artist")
    }

    func testSpotifyDefaultLabelIsClearedOnTypeChange() {
        // 何らかの経緯で保存済みの"Play on Spotify"も、リンク先変更時に既定値として掃除される
        defaults.set("Play on Spotify", forKey: "button1Label")
        settings.button1Type = .album
        XCTAssertEqual(settings.button1Label, "View Album")
    }

    // MARK: - ミュージックソースの有効/無効

    func testSourcesEnabledByDefault() {
        XCTAssertTrue(settings.isSourceEnabled(.appleMusic))
        XCTAssertTrue(settings.isSourceEnabled(.spotify))
    }

    func testSourceToggleIsPersistedPerSource() {
        settings.setSourceEnabled(.spotify, false)
        XCTAssertFalse(settings.isSourceEnabled(.spotify))
        XCTAssertTrue(settings.isSourceEnabled(.appleMusic))
        settings.setSourceEnabled(.spotify, true)
        XCTAssertTrue(settings.isSourceEnabled(.spotify))
    }
}
