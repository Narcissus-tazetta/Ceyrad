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
        settings.button1Label = settings.button1Type.defaultLabel
        settings.button1Type = .album
        XCTAssertEqual(settings.button1Label, "View Album")
    }

    func testEmptyLabelResumesFollowing() {
        settings.button1Label = "My Label"
        settings.button1Label = ""
        XCTAssertEqual(settings.button1Label, settings.button1Type.defaultLabel)
    }

    func testLabelIsTruncatedTo32Characters() {
        settings.button1Label = String(repeating: "x", count: 64)
        XCTAssertEqual(settings.button1Label.count, 32)
    }
}
