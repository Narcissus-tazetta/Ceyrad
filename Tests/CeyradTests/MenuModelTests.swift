import XCTest

@testable import Ceyrad

/// メニューの中身をNSMenuを開かずに検証する。
/// 以前はAppKitに直接組み立てていたためここは丸ごとテスト不能だった。
final class MenuModelTests: XCTestCase {
    private static let suiteName = "CeyradTests.MenuModel"
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

    private func build(launchAtLogin: Bool = false) -> [MenuRow] {
        var music = MusicState()
        music.running = true
        music.playerState = .playing
        music.track = TrackInfo(name: "Song", artist: "Artist", album: "Album")
        return MenuModel.build(
            MenuModel.Input(
                status: StatusLinesBuilder.Input(
                    music: music, discordState: .connected, language: settings.language
                ),
                settings: settings,
                launchAtLogin: launchAtLogin
            )
        )
    }

    // MARK: - 走査ヘルパ

    /// 全階層のアクション付き行を平坦に集める。
    private func allActions(_ rows: [MenuRow]) -> [MenuAction] {
        rows.flatMap { row -> [MenuAction] in
            switch row {
            case .item(_, let action): return [action]
            case .choice(_, let action, _): return [action]
            case .submenu(_, let rows): return allActions(rows)
            case .info, .separator: return []
            }
        }
    }

    private func submenu(labelContaining needle: String, in rows: [MenuRow]) -> [MenuRow]? {
        for row in rows {
            if case .submenu(let label, let sub) = row, label.contains(needle) { return sub }
        }
        return nil
    }

    private func checkedLabels(_ rows: [MenuRow]) -> [String] {
        rows.compactMap { row in
            if case .choice(let label, _, let checked) = row, checked { return label }
            return nil
        }
    }

    // MARK: - 構造

    func testStatusLinesComeFirstAndAreNotClickable() {
        let rows = build()
        guard case .info(let first) = rows[0] else {
            return XCTFail("先頭はステータス行であるべき: \(rows[0])")
        }
        XCTAssertEqual(first, "♪ Apple Music: Song — Artist")
    }

    func testEveryActionIsReachable() {
        let actions = allActions(build())
        // 「メニューに出ていないアクション」があると、対応する処理が永久に呼ばれない
        XCTAssertTrue(actions.contains(.editCustomURL))
        XCTAssertTrue(actions.contains(.editRepositoryURL))
        XCTAssertTrue(actions.contains(.toggleLaunchAtLogin))
        XCTAssertTrue(actions.contains(.reconnect))
        XCTAssertTrue(actions.contains(.checkForUpdates))
        XCTAssertTrue(actions.contains(.quit))
        for slot in ButtonSlot.allCases {
            XCTAssertTrue(actions.contains(.editButtonLabel(slot)), "スロット\(slot.number)")
            for type in LinkType.selectableCases {
                XCTAssertTrue(actions.contains(.setButtonType(slot, type)))
            }
        }
        for badge in BadgeLabelType.allCases {
            XCTAssertTrue(actions.contains(.setBadgeLabel(badge)))
        }
        for minutes in SettingsStore.pauseHideChoices {
            XCTAssertTrue(actions.contains(.setPauseHideMinutes(minutes)))
        }
        for language in AppLanguage.allCases {
            XCTAssertTrue(actions.contains(.setLanguage(language)))
        }
    }

    /// 2つのボタンのサブメニューは、スロット番号以外まったく同じ形でなければならない。
    func testBothButtonSubmenusHaveTheSameShape() {
        let rows = build()
        let one = submenu(labelContaining: "Button 1", in: rows)
        let two = submenu(labelContaining: "Button 2", in: rows)
        XCTAssertNotNil(one)
        XCTAssertEqual(one?.count, two?.count)
    }

    // MARK: - チェック状態

    func testExactlyOneChoiceIsCheckedInEachOptionList() {
        let rows = build()
        for needle in ["Button 1", "Button 2", "Status Badge", "When Paused", "Language"] {
            guard let sub = submenu(labelContaining: needle, in: rows) else {
                return XCTFail("サブメニューが見つからない: \(needle)")
            }
            XCTAssertEqual(checkedLabels(sub).count, 1, "\(needle) のチェックは1つだけ")
        }
    }

    func testCheckedChoiceFollowsTheSetting() {
        settings.setButtonType(.one, .album)
        settings.badgeLabel = .track
        settings.pauseHideMinutes = -1
        let rows = build()

        XCTAssertEqual(checkedLabels(submenu(labelContaining: "Button 1", in: rows) ?? []), ["Album Page"])
        XCTAssertEqual(checkedLabels(submenu(labelContaining: "Status Badge", in: rows) ?? []), ["Track Name"])
        XCTAssertEqual(checkedLabels(submenu(labelContaining: "When Paused", in: rows) ?? []), ["Keep Showing"])
    }

    /// 自動起動のチェックはOSが返す値をそのまま映す（設定ではなく実際の状態を見せる）。
    func testLaunchAtLoginReflectsTheGivenState() {
        for enabled in [true, false] {
            let rows = build(launchAtLogin: enabled)
            let row = rows.first { row in
                if case .choice(_, .toggleLaunchAtLogin, _) = row { return true }
                return false
            }
            guard case .choice(_, _, let checked)? = row else {
                return XCTFail("自動起動の行がない")
            }
            XCTAssertEqual(checked, enabled)
        }
    }

    // MARK: - ラベル

    func testButtonRowNamesItsDestinationAndLabel() {
        settings.setButtonLabel(.one, "My Label")
        let rows = build()
        guard let sub = submenu(labelContaining: "Button 1", in: rows) else {
            return XCTFail("ボタン1のサブメニューがない")
        }
        XCTAssertTrue(
            sub.contains { row in
                if case .item(let label, .editButtonLabel(.one)) = row {
                    return label.contains("My Label")
                }
                return false
            },
            "現在のラベルが「ラベルを変更…」に出ていない"
        )
    }

    /// オフ中はラベルがどこにも出ないので、変更を持ちかけない（クリック不可の行にする）。
    func testDisabledButtonOffersNoLabelEditing() {
        settings.setButtonType(.one, .disabled)
        let rows = build()
        let sub = submenu(labelContaining: "Button 1", in: rows) ?? []
        XCTAssertFalse(allActions(sub).contains(.editButtonLabel(.one)))
        XCTAssertTrue(sub.contains { if case .info(let l) = $0 { return l.contains("Change Label") } else { return false } })
    }

    func testPauseChoiceNamesReadAsBehaviour() {
        XCTAssertEqual(MenuModel.pauseChoiceName(-1, .en), "Keep Showing")
        XCTAssertEqual(MenuModel.pauseChoiceName(0, .en), "Hide Immediately")
        XCTAssertEqual(MenuModel.pauseChoiceName(1, .en), "Hide After 1 Minute")
        XCTAssertEqual(MenuModel.pauseChoiceName(5, .en), "Hide After 5 Minutes")
        XCTAssertEqual(MenuModel.pauseChoiceName(1, .ja), "1分後に消す")
    }

    // MARK: - 言語

    /// 言語を変えると全行が変わる——ただしDiscordを名指しする行だけは変わらない。
    func testLanguageChangesEveryRowExceptTheDiscordOnes() {
        let english = build()
        settings.language = .ja
        let japanese = build()

        XCTAssertEqual(english.count, japanese.count, "行数は言語で変わらない")
        XCTAssertTrue(
            japanese.contains { if case .item("Reconnect to Discord", _) = $0 { return true } else { return false } },
            "Discordを名指しする文言は翻訳しない"
        )
        XCTAssertTrue(
            japanese.contains { if case .item(let l, .quit) = $0 { return l == "Ceyradを終了" } else { return false } }
        )
    }
}
