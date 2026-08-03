import XCTest

@testable import Ceyrad

final class URLPromptTests: XCTestCase {
    /// 用意した答えを順に返し、エラーが何回出たかを数える。
    private func run(
        _ answers: [String?], initial: String = "", allowEmpty: Bool = false
    ) -> (result: String?, errors: Int) {
        var next = 0
        var errors = 0
        let result = URLPrompt.promptValidURL(
            showPrompt: { _ in
                defer { next += 1 }
                return answers[next]
            },
            showError: { errors += 1 },
            initial: initial,
            allowEmpty: allowEmpty
        )
        return (result, errors)
    }

    func testValidURLIsAcceptedFirstTime() {
        let (result, errors) = run(["https://example.com"])
        XCTAssertEqual(result, "https://example.com")
        XCTAssertEqual(errors, 0)
    }

    func testInvalidURLIsReportedAndAskedAgain() {
        let (result, errors) = run(["nonsense", "https://example.com"])
        XCTAssertEqual(result, "https://example.com")
        XCTAssertEqual(errors, 1, "1回だけ知らせるべき")
    }

    /// 打ち間違いは打ち直すより直すほうが楽なので、弾いた文字列を入力欄に残して訊き直す。
    func testRejectedTextIsOfferedBackForEditing() {
        var seen: String?
        var answers = ["htp://typo.example", "http://typo.example"].makeIterator()
        _ = URLPrompt.promptValidURL(
            showPrompt: { current in
                seen = current
                return answers.next()
            },
            showError: {},
            initial: "",
            allowEmpty: false
        )
        XCTAssertEqual(seen, "htp://typo.example")
    }

    func testCancellingChangesNothing() {
        let (result, errors) = run([nil], initial: "https://example.com")
        XCTAssertNil(result)
        XCTAssertEqual(errors, 0)
    }

    func testCancellingAfterAnErrorStillChangesNothing() {
        let (result, errors) = run(["nope", nil])
        XCTAssertNil(result)
        XCTAssertEqual(errors, 1)
    }

    func testBlankClearsTheSettingOnlyWhereThatMakesSense() {
        // カスタムURL: 空文字は「カスタムURLなし」を意味する
        let (cleared, clearErrors) = run([""], initial: "https://old.example", allowEmpty: true)
        XCTAssertEqual(cleared, "")
        XCTAssertEqual(clearErrors, 0)

        // リポジトリURL: 空文字はただの無効なURL
        let (result, errors) = run(["", "https://example.com"], allowEmpty: false)
        XCTAssertEqual(result, "https://example.com")
        XCTAssertEqual(errors, 1)
    }

    /// Discordはhttp(s)のボタンしか描画しない。ステータスの下に壊れたリンクを出さない。
    func testNonHTTPSchemeIsRefused() {
        let (result, errors) = run(["file:///Users/secret", nil])
        XCTAssertNil(result)
        XCTAssertEqual(errors, 1)
    }
}
