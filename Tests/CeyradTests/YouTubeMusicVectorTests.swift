import XCTest

@testable import Ceyrad

/// `spec/youtube_music_vectors.json` を読んで、macOS版の判定が仕様どおりか確かめる。
/// 同じファイルを `windows/tests/youtube_music_vector_tests.rs` も読む。
final class YouTubeMusicVectorTests: XCTestCase {
    private struct Spec: Decodable {
        var cases: [Case]
    }

    private struct Case: Decodable {
        var name: String
        var isBrowser: Bool
        var album: String
        var hasYouTubeMusicTab: Bool?
        var expected: Bool
    }

    private static var specURL: URL {
        URL(fileURLWithPath: #filePath)
            .deletingLastPathComponent()
            .deletingLastPathComponent()
            .deletingLastPathComponent()
            .appendingPathComponent("spec/youtube_music_vectors.json")
    }

    func testEveryVectorIsDecidedAsTheSpecSays() throws {
        let data = try Data(contentsOf: Self.specURL)
        let spec = try JSONDecoder().decode(Spec.self, from: data)
        XCTAssertFalse(spec.cases.isEmpty, "ベクタが空: 読み込みが壊れている")

        for testCase in spec.cases {
            XCTAssertEqual(
                YouTubeMusic.matches(
                    isBrowser: testCase.isBrowser, album: testCase.album,
                    hasYouTubeMusicTab: testCase.hasYouTubeMusicTab),
                testCase.expected, testCase.name
            )
        }
    }
}
