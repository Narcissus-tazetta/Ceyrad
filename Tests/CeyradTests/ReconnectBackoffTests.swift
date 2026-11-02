import XCTest

@testable import Ceyrad

final class ReconnectBackoffTests: XCTestCase {
    func testAScheduledRetryIsPendingUntilItRunsOrIsReset() {
        let backoff = ReconnectBackoff()
        XCTAssertFalse(backoff.isPending)

        backoff.schedule {}
        XCTAssertTrue(backoff.isPending)

        backoff.reset()
        XCTAssertFalse(backoff.isPending)
    }

    func testRunningTheRetryClearsThePending() {
        let backoff = ReconnectBackoff()
        let ran = expectation(description: "retry ran")
        backoff.schedule { ran.fulfill() }
        wait(for: [ran], timeout: 3)
        XCTAssertFalse(backoff.isPending, "実行済みの予約が残ると、以後ずっと繋ぎにいかなくなる")
    }
}
