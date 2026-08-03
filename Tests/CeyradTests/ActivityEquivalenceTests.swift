import XCTest

@testable import Ceyrad

/// 「Discordが同じに描画するか」の判定。
/// これが甘いと同一内容を送り続けてレート制限を食い、厳しすぎると更新が届かなくなる。
final class ActivityEquivalenceTests: XCTestCase {
    private func activity(
        details: String = "Song", state: String = "Artist",
        start: Int = 1_000_000, end: Int = 1_200_000
    ) -> [String: Any] {
        [
            "type": 2,
            "details": details,
            "state": state,
            "status_display_type": 1,
            "timestamps": ["start": start, "end": end],
        ]
    }

    func testNothingSentAndClearedAreNotTheSame() {
        XCTAssertTrue(ActivityBuilder.isEquivalent(nil, nil))
        XCTAssertFalse(ActivityBuilder.isEquivalent(nil, activity()))
        XCTAssertFalse(ActivityBuilder.isEquivalent(activity(), nil))
    }

    func testIdenticalActivitiesAreEquivalent() {
        XCTAssertTrue(ActivityBuilder.isEquivalent(activity(), activity()))
    }

    /// 組み立てのたびにtimestampsを壁時計から計算し直すのでバイト等価には決してならない。
    /// 描画が変わらない程度のズレは「同じ」と見なす。
    func testSmallTimestampDriftIsIgnored() {
        let a = activity(start: 1_000_000, end: 1_200_000)
        let b = activity(start: 1_000_900, end: 1_200_900)
        XCTAssertTrue(ActivityBuilder.isEquivalent(a, b))
    }

    /// シークはドリフト窓よりはるかに大きくstartを動かすので、変更として残らなければならない。
    func testASeekIsNotIgnored() {
        let a = activity(start: 1_000_000, end: 1_200_000)
        let b = activity(start: 1_030_000, end: 1_230_000)
        XCTAssertFalse(ActivityBuilder.isEquivalent(a, b))
    }

    func testDifferentTrackIsNotEquivalent() {
        XCTAssertFalse(
            ActivityBuilder.isEquivalent(activity(details: "A"), activity(details: "B"))
        )
    }

    func testAnAddedKeyIsNotEquivalent() {
        var withArt = activity()
        withArt["assets"] = ["large_image": "https://example.com/a.jpg"]
        XCTAssertFalse(ActivityBuilder.isEquivalent(activity(), withArt))
    }

    /// ネストした辞書・配列も中身まで比較されること。
    /// 参照比較に退化していると、アートワークが差し替わっても再送されない。
    func testNestedValuesAreComparedByContent() {
        var a = activity()
        a["assets"] = ["large_image": "https://example.com/a.jpg", "large_text": "Album"]
        var b = activity()
        b["assets"] = ["large_image": "https://example.com/a.jpg", "large_text": "Album"]
        XCTAssertTrue(ActivityBuilder.isEquivalent(a, b))

        var c = activity()
        c["assets"] = ["large_image": "https://example.com/b.jpg", "large_text": "Album"]
        XCTAssertFalse(ActivityBuilder.isEquivalent(a, c))
    }

    func testButtonsAreComparedByContent() {
        var a = activity()
        a["buttons"] = [["label": "One", "url": "https://example.com/1"]]
        var b = activity()
        b["buttons"] = [["label": "One", "url": "https://example.com/1"]]
        XCTAssertTrue(ActivityBuilder.isEquivalent(a, b))

        var c = activity()
        c["buttons"] = [["label": "Two", "url": "https://example.com/1"]]
        XCTAssertFalse(ActivityBuilder.isEquivalent(a, c))
    }

    /// timestampsがない側とある側（再生中→一時停止）を取り違えない。
    func testMissingTimestampsIsADifference() {
        var paused = activity()
        paused["timestamps"] = nil
        XCTAssertFalse(ActivityBuilder.isEquivalent(activity(), paused))
    }

    /// 実際に組み立てた2つが、同じ再生状態なら「同じ」と判定されること。
    func testTwoBuildsOfTheSamePlaybackAreEquivalent() {
        let defaults = UserDefaults(suiteName: "CeyradTests.Equivalence")!
        defaults.removePersistentDomain(forName: "CeyradTests.Equivalence")
        let settings = SettingsStore(defaults: defaults)
        defer { defaults.removePersistentDomain(forName: "CeyradTests.Equivalence") }

        let track = TrackInfo(
            name: "Song", artist: "Artist", album: "Album",
            durationSec: 200, positionSec: 50
        )
        let now = Date()
        let first = ActivityBuilder.build(
            track: track, playerState: .playing, catalog: nil, settings: settings, now: now
        )
        // 1秒後に無関係な理由で組み立て直した
        let second = ActivityBuilder.build(
            track: track, playerState: .playing, catalog: nil, settings: settings,
            now: now.addingTimeInterval(1)
        )
        XCTAssertTrue(ActivityBuilder.isEquivalent(first, second), "1秒の再送は抑止されるべき")
    }
}
