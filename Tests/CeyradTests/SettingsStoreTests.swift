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
        XCTAssertEqual(settings.buttonType(.one), .song)
        XCTAssertEqual(settings.buttonLabel(.one), "Play on Apple Music")
        XCTAssertEqual(settings.buttonType(.two), .repository)
        XCTAssertEqual(settings.buttonLabel(.two), "About This App")
        XCTAssertEqual(settings.pauseHideMinutes, 5)
        XCTAssertEqual(settings.repositoryURL, SettingsStore.defaultRepositoryURL)
    }

    /// 2つのスロットは完全に対称でなければならない。
    /// 片方だけに効く設定が生まれると、メニューは同じ見た目のまま挙動だけが食い違う。
    func testBothSlotsBehaveIdentically() {
        for slot in ButtonSlot.allCases {
            settings.setButtonType(slot, .artist)
            XCTAssertEqual(settings.buttonType(slot), .artist, "スロット\(slot.number)")
            XCTAssertEqual(settings.buttonLabel(slot), "View Artist", "スロット\(slot.number)")

            settings.setButtonLabel(slot, "Mine")
            XCTAssertEqual(settings.buttonLabel(slot), "Mine", "スロット\(slot.number)")
        }
        // 片方への書き込みがもう片方に漏れていないこと
        settings.setButtonType(.one, .album)
        XCTAssertEqual(settings.buttonType(.two), .artist)
    }

    func testLabelFollowsTypeChangeWhenNotCustomized() {
        settings.setButtonType(.one, .artist)
        XCTAssertEqual(settings.buttonLabel(.one), "View Artist")
    }

    func testCustomLabelSurvivesTypeChange() {
        settings.setButtonLabel(.one, "My Label")
        settings.setButtonType(.one, .artist)
        XCTAssertEqual(settings.buttonLabel(.one), "My Label")
    }

    func testSettingLabelToDefaultResumesFollowing() {
        settings.setButtonLabel(.one, "My Label")
        // 既定値と同じ文字列を入れたらカスタム扱いを解除し、以後はリンク先に追従する
        settings.setButtonLabel(.one, settings.buttonType(.one).defaultLabel)
        settings.setButtonType(.one, .album)
        XCTAssertEqual(settings.buttonLabel(.one), "View Album")
    }

    func testEmptyLabelResumesFollowing() {
        settings.setButtonLabel(.one, "My Label")
        settings.setButtonLabel(.one, "")
        XCTAssertEqual(settings.buttonLabel(.one), settings.buttonType(.one).defaultLabel)
    }

    func testLabelIsTruncatedTo32Characters() {
        settings.setButtonLabel(.one, String(repeating: "x", count: 64))
        XCTAssertEqual(settings.buttonLabel(.one).count, 32)
    }

    /// 上限はUnicodeスカラー数で数える（Windows版と同じ単位）。
    /// Swiftの`prefix`は書記素クラスタ単位なので、家族絵文字だと同じ「32」が
    /// 7倍の長さになり、両OSで別物がDiscordに出てしまう。
    func testLabelTruncationCountsScalarsAndNeverSplitsACharacter() {
        let family = "👨‍👩‍👧"  // 👨 ZWJ 👩 ZWJ 👧 = 5スカラー
        settings.setButtonLabel(.one, String(repeating: family, count: 40))
        let label = settings.buttonLabel(.one)

        XCTAssertLessThanOrEqual(label.unicodeScalars.count, 32)
        XCTAssertNotEqual(label.unicodeScalars.last, "\u{200D}", "末尾にZWJが残っている")
        // 6家族（30スカラー）＋割れずに残った👨。壊れた家族の断片は出さない。
        XCTAssertEqual(label, String(repeating: family, count: 6) + "👨")
    }

    // MARK: - 言語

    /// 表示名がグローバルの言語設定ではなく引数だけで決まること。
    func testDisplayNamesDependOnlyOnTheGivenLanguage() {
        XCTAssertEqual(LinkType.song.displayName(.en), "Song Page")
        XCTAssertEqual(LinkType.song.displayName(.ja), "曲ページ")
        XCTAssertEqual(BadgeLabelType.artist.displayName(.en), "Artist Name")
        XCTAssertEqual(BadgeLabelType.artist.displayName(.ja), "アーティスト名")
        // 言語名そのものは表示言語に依存しない
        XCTAssertEqual(AppLanguage.ja.displayName, "日本語")
    }

    /// ボタンラベルはDiscord側に出るため、表示言語に関わらず英語で固定。
    func testButtonLabelsAreNeverLocalized() {
        settings.language = .ja
        XCTAssertEqual(settings.buttonLabel(.one), "Play on Apple Music")
    }
}
