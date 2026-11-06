import XCTest

@testable import Ceyrad

/// `spec/youtube_music_search_vectors.json` を読んで、macOS版の検索の照合が仕様どおりか確かめる。
/// 同じファイルを `windows/tests/ytmusic_search_vector_tests.rs` も読む。
final class YouTubeMusicSearchVectorTests: XCTestCase {
    private struct Spec: Decodable {
        var languages: [LanguageCase]
        var picks: [PickCase]
    }

    private struct LanguageCase: Decodable {
        var name: String
        var track: String
        var artist: String
        var appLanguage: String
        var expected: [String]
    }

    private struct PickCase: Decodable {
        var name: String
        var rows: [Row]
        var track: String
        var artist: String
        var durationSec: Double?
        var expectedVideoId: String?
        var expectedSongURL: String?
        var expectedArtworkURL: String?
    }

    private struct Row: Decodable {
        var title: String
        var byline: [String]
        var videoId: String
    }

    private static func spec() throws -> Spec {
        let url = URL(fileURLWithPath: #filePath)
            .deletingLastPathComponent()
            .deletingLastPathComponent()
            .deletingLastPathComponent()
            .appendingPathComponent("spec/youtube_music_search_vectors.json")
        return try JSONDecoder().decode(Spec.self, from: Data(contentsOf: url))
    }

    /// 実際の検索応答の形を、パーサが読む部分だけに削ったもの。
    private static func response(_ rows: [Row]) throws -> Data {
        func column(_ texts: [String]) -> [String: Any] {
            let text: [String: Any] = ["runs": texts.map { ["text": $0] }]
            return ["musicResponsiveListItemFlexColumnRenderer": ["text": text]]
        }
        let thumbnails = [
            ["url": "https://lh3.googleusercontent.com/abc=w60-h60-l90-rj"],
            ["url": "https://lh3.googleusercontent.com/abc=w120-h120-l90-rj"],
        ]
        let thumbnail: [String: Any] = [
            "musicThumbnailRenderer": ["thumbnail": ["thumbnails": thumbnails]]
        ]
        let items: [[String: Any]] = rows.map { row in
            let renderer: [String: Any] = [
                "flexColumns": [column([row.title]), column(row.byline)],
                "playlistItemData": ["videoId": row.videoId],
                "thumbnail": thumbnail,
            ]
            return ["musicResponsiveListItemRenderer": renderer]
        }
        let shelf: [String: Any] = ["contents": items]
        return try JSONSerialization.data(withJSONObject: ["contents": ["tabs": [["shelf": shelf]]]])
    }

    func testEveryLanguageVectorIsOrderedAsTheSpecSays() throws {
        let spec = try Self.spec()
        XCTAssertFalse(spec.languages.isEmpty, "ベクタが空: 読み込みが壊れている")
        for testCase in spec.languages {
            XCTAssertEqual(
                YouTubeMusicSearch.languagesToTry(
                    name: testCase.track, artist: testCase.artist,
                    appLanguage: testCase.appLanguage),
                testCase.expected, testCase.name)
        }
    }

    func testEveryPickVectorIsDecidedAsTheSpecSays() throws {
        let spec = try Self.spec()
        XCTAssertFalse(spec.picks.isEmpty, "ベクタが空: 読み込みが壊れている")
        for testCase in spec.picks {
            let songs = try XCTUnwrap(
                YouTubeMusicSearch.parseSongs(Self.response(testCase.rows)), testCase.name)
            let best = YouTubeMusicSearch.pickBest(
                from: songs, name: testCase.track, artist: testCase.artist,
                durationSec: testCase.durationSec)
            XCTAssertEqual(best?.videoID, testCase.expectedVideoId, testCase.name)
            guard let best else { continue }
            let catalog = YouTubeMusicSearch.catalog(from: best)
            if let url = testCase.expectedSongURL {
                XCTAssertEqual(catalog.songURL, url, testCase.name)
            }
            if let url = testCase.expectedArtworkURL {
                XCTAssertEqual(catalog.artworkURL, url, testCase.name)
            }
        }
    }
}
