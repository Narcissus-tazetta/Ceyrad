import Compression
import XCTest

@testable import Ceyrad

final class FirefoxSessionTests: XCTestCase {
    /// Firefox と同じ形式（マジック・展開後の大きさ・フレームなしの LZ4 ブロック）に包む。
    private func mozLz4(_ json: String) -> Data {
        let input = Data(json.utf8)
        var compressed = Data(count: input.count + 1024)
        let written = compressed.withUnsafeMutableBytes { out in
            input.withUnsafeBytes { source in
                compression_encode_buffer(
                    out.bindMemory(to: UInt8.self).baseAddress!, out.count,
                    source.bindMemory(to: UInt8.self).baseAddress!, input.count,
                    nil, COMPRESSION_LZ4_RAW)
            }
        }
        var data = Data("mozLz40\0".utf8)
        withUnsafeBytes(of: UInt32(input.count).littleEndian) { data.append(contentsOf: $0) }
        data.append(compressed.prefix(written))
        return data
    }

    private let session = """
        {"windows":[
          {"tabs":[
            {"index":2,"entries":[
              {"url":"https://music.youtube.com/","title":"YouTube Music"},
              {"url":"https://music.youtube.com/watch?v=abc","title":"pupa | YouTube Music"}]},
            {"index":1,"entries":[{"url":"about:blank"}]}]},
          {"tabs":[{"index":1,"entries":[{"url":"https://example.com/","title":"Example"}]}]}],
         "_closedWindows":[{"tabs":[{"index":1,"entries":[
           {"url":"https://music.youtube.com/watch?v=closed","title":"closed | YouTube Music"}]}]}]}
        """

    func testReadsTheCurrentPageOfEveryOpenTab() throws {
        let tabs = try FirefoxSession.tabs(fromSessionJSON: FirefoxSession.decodeMozLz4(mozLz4(session)))
        XCTAssertEqual(
            tabs,
            [
                BrowserTab(url: "https://music.youtube.com/watch?v=abc", title: "pupa | YouTube Music"),
                BrowserTab(url: "about:blank", title: ""),
                BrowserTab(url: "https://example.com/", title: "Example"),
            ])
    }

    func testRejectsFilesThatAreNotMozLz4() {
        XCTAssertThrowsError(try FirefoxSession.decodeMozLz4(Data(#"{"windows":[]}"#.utf8))) {
            XCTAssertEqual($0 as? FirefoxSession.Failure, .badMagic)
        }
    }

    func testRejectsATruncatedBlock() {
        let data = mozLz4(session)
        XCTAssertThrowsError(try FirefoxSession.decodeMozLz4(data.prefix(data.count - 20))) {
            XCTAssertEqual($0 as? FirefoxSession.Failure, .decompressionFailed)
        }
    }
}
