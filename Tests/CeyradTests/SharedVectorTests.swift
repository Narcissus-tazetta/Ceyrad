import XCTest

@testable import Ceyrad

/// `spec/activity_vectors.json` を読んで、macOS版の組み立てが仕様どおりか確かめる。
///
/// 同じファイルを `windows/tests/shared_vector_tests.rs` も読む。片方だけを直すと
/// もう片方が落ちる——それがこのテストの唯一の目的で、Activityの中身そのものは
/// `ActivityBuilderTests` が個別に見ている。
final class SharedVectorTests: XCTestCase {
    private struct Spec: Decodable {
        var cases: [Case]
    }

    private struct Case: Decodable {
        var name: String
        var track: TrackSpec
        var playerState: String
        var catalog: CatalogSpec?
        var settings: SettingsSpec
        var nowUnix: Double
        var expected: [String: JSONValue]
    }

    private struct TrackSpec: Decodable {
        var name: String
        var artist: String
        var album: String
        var durationSec: Double?
        var positionSec: Double?
        var positionSampledAtUnix: Double
    }

    private struct CatalogSpec: Decodable {
        var songURL: String?
        var artistURL: String?
        var albumURL: String?
        var artworkURL: String?
    }

    private struct SettingsSpec: Decodable {
        var button1Type: String?
        var button2Type: String?
        var button1Label: String?
        var button2Label: String?
        var customURL: String?
        var repositoryURL: String?
        var badgeLabel: String?
        var pauseHideMinutes: Int?
    }

    private static let suiteName = "CeyradTests.SharedVectors"

    /// リポジトリ内の `spec/` は、テストバンドルではなくソースからの相対で引く。
    /// SwiftPMのリソース宣言を足すとビルド設定がもう1つ増えるだけで、
    /// このファイルは実行時ではなくテスト時にしか要らない。
    private static var specURL: URL {
        URL(fileURLWithPath: #filePath)  // Tests/CeyradTests/SharedVectorTests.swift
            .deletingLastPathComponent()  // Tests/CeyradTests
            .deletingLastPathComponent()  // Tests
            .deletingLastPathComponent()  // <repo>
            .appendingPathComponent("spec/activity_vectors.json")
    }

    func testEveryVectorBuildsExactlyWhatTheSpecSays() throws {
        let data = try Data(contentsOf: Self.specURL)
        let spec = try JSONDecoder().decode(Spec.self, from: data)
        XCTAssertFalse(spec.cases.isEmpty, "ベクタが空: 読み込みが壊れている")

        let defaults = UserDefaults(suiteName: Self.suiteName)!
        defer { defaults.removePersistentDomain(forName: Self.suiteName) }

        for testCase in spec.cases {
            defaults.removePersistentDomain(forName: Self.suiteName)
            let settings = SettingsStore(defaults: defaults)
            apply(testCase.settings, to: settings)

            let activity = ActivityBuilder.build(
                track: track(from: testCase.track),
                playerState: playerState(testCase.playerState),
                catalog: testCase.catalog.map(catalog(from:)),
                settings: settings,
                now: Date(timeIntervalSince1970: testCase.nowUnix)
            )

            let expected = testCase.expected.mapValues(\.any)
            XCTAssertTrue(
                ActivityBuilder.isEquivalent(activity, expected)
                    && (activity as NSDictionary).isEqual(to: expected),
                """
                \(testCase.name)
                期待: \(sorted(expected))
                実際: \(sorted(activity))
                """
            )
        }
    }

    // MARK: - 組み立て

    private func track(from spec: TrackSpec) -> TrackInfo {
        var track = TrackInfo(
            name: spec.name, artist: spec.artist, album: spec.album,
            durationSec: spec.durationSec, positionSec: spec.positionSec
        )
        track.positionSampledAt = Date(timeIntervalSince1970: spec.positionSampledAtUnix)
        return track
    }

    private func catalog(from spec: CatalogSpec) -> CatalogInfo {
        CatalogInfo(
            songURL: spec.songURL, artistURL: spec.artistURL,
            albumURL: spec.albumURL, artworkURL: spec.artworkURL
        )
    }

    private func playerState(_ raw: String) -> PlayerState {
        switch raw {
        case "playing": return .playing
        case "paused": return .paused
        default: return .stopped
        }
    }

    private func apply(_ spec: SettingsSpec, to settings: SettingsStore) {
        applyButtons(spec, to: settings)
        if let url = spec.customURL { settings.customURL = url }
        if let url = spec.repositoryURL { settings.repositoryURL = url }
        if let raw = spec.badgeLabel { settings.badgeLabel = badgeLabel(raw) }
        if let minutes = spec.pauseHideMinutes { settings.pauseHideMinutes = minutes }
    }

    private func applyButtons(_ spec: SettingsSpec, to settings: SettingsStore) {
        let types = [(ButtonSlot.one, spec.button1Type), (.two, spec.button2Type)]
        for (slot, raw) in types {
            guard let raw else { continue }
            guard let type = LinkType(rawValue: raw) else {
                return XCTFail("未知のリンク先: \(raw)")
            }
            settings.setButtonType(slot, type)
        }
        let labels = [(ButtonSlot.one, spec.button1Label), (.two, spec.button2Label)]
        for (slot, label) in labels {
            if let label { settings.setButtonLabel(slot, label) }
        }
    }

    private func badgeLabel(_ raw: String) -> BadgeLabelType {
        switch raw {
        case "appName": return .appName
        case "artist": return .artist
        case "track": return .track
        default:
            XCTFail("未知のバッジ指定: \(raw)")
            return .artist
        }
    }

    private func sorted(_ activity: [String: Any]) -> String {
        activity.keys.sorted().map { "\($0)=\(activity[$0]!)" }.joined(separator: " ")
    }
}

/// JSONをそのまま`[String: Any]`に落とすための最小のデコーダ。
/// `JSONSerialization`だとNSNumberの取り回しが型ごとに変わるため、
/// Int/Double/String/配列/辞書を明示的に受ける。
private enum JSONValue: Decodable {
    case string(String)
    case int(Int)
    case double(Double)
    case bool(Bool)
    case array([JSONValue])
    case object([String: JSONValue])
    case null

    init(from decoder: Decoder) throws {
        let container = try decoder.singleValueContainer()
        if container.decodeNil() {
            self = .null
        } else if let value = try? container.decode(Bool.self) {
            self = .bool(value)
        } else if let value = try? container.decode(Int.self) {
            self = .int(value)
        } else if let value = try? container.decode(Double.self) {
            self = .double(value)
        } else if let value = try? container.decode(String.self) {
            self = .string(value)
        } else if let value = try? container.decode([JSONValue].self) {
            self = .array(value)
        } else {
            self = .object(try container.decode([String: JSONValue].self))
        }
    }

    var any: Any {
        switch self {
        case .string(let value): return value
        case .int(let value): return value
        case .double(let value): return value
        case .bool(let value): return value
        case .array(let values): return values.map(\.any)
        case .object(let values): return values.mapValues(\.any)
        case .null: return NSNull()
        }
    }
}
