import XCTest

@testable import Ceyrad

/// `spec/advertisement_vectors.json` を読んで、macOS版の広告判定が仕様どおりか確かめる。
/// 同じファイルを `windows/tests/advertisement_vector_tests.rs` も読む。
final class AdvertisementVectorTests: XCTestCase {
    private struct Spec: Decodable {
        var cases: [Case]
    }

    private struct Case: Decodable {
        var name: String
        var track: Track
        var expected: Bool
    }

    private struct Track: Decodable {
        var name: String
        var artist: String
        var album: String
    }

    private static var specURL: URL {
        URL(fileURLWithPath: #filePath)
            .deletingLastPathComponent()
            .deletingLastPathComponent()
            .deletingLastPathComponent()
            .appendingPathComponent("spec/advertisement_vectors.json")
    }

    func testEveryVectorIsDecidedAsTheSpecSays() throws {
        let data = try Data(contentsOf: Self.specURL)
        let spec = try JSONDecoder().decode(Spec.self, from: data)
        XCTAssertFalse(spec.cases.isEmpty, "ベクタが空: 読み込みが壊れている")

        for testCase in spec.cases {
            let track = TrackInfo(
                name: testCase.track.name, artist: testCase.track.artist,
                album: testCase.track.album)
            XCTAssertEqual(track.isAdvertisement, testCase.expected, testCase.name)
        }
    }
}
