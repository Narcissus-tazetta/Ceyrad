import XCTest

@testable import Cadence

final class DebouncerTests: XCTestCase {
    func testOnlyLastScheduledBlockRuns() {
        let debouncer = Debouncer(delay: 0.05)
        let first = expectation(description: "first block should be superseded")
        first.isInverted = true
        let second = expectation(description: "second block runs")

        debouncer.schedule { first.fulfill() }
        debouncer.schedule { second.fulfill() }

        wait(for: [first, second], timeout: 1.0)
    }

    func testCancelPreventsExecution() {
        let debouncer = Debouncer(delay: 0.05)
        let never = expectation(description: "cancelled block never runs")
        never.isInverted = true

        debouncer.schedule { never.fulfill() }
        debouncer.cancel()

        wait(for: [never], timeout: 0.3)
    }
}
