import Foundation
import XCTest

@testable import Ceyrad

final class CatalogSchedulingTests: XCTestCase {
    private var session: URLSession!

    override func setUp() {
        super.setUp()
        let configuration = URLSessionConfiguration.ephemeral
        configuration.protocolClasses = [CatalogURLProtocol.self]
        session = URLSession(configuration: configuration)
    }

    override func tearDown() {
        session.invalidateAndCancel()
        super.tearDown()
    }

    func testBothSourcesFinishUnderTheSharedProductionRateLimit() {
        let requests = RequestLog()
        CatalogURLProtocol.handle { request in
            requests.record(request)
            return .missing
        }
        let search = ITunesSearchClient(session: session)
        let done = expectation(description: "Both sources resolved")
        done.expectedFulfillmentCount = 2
        for source in [MusicSource.appleMusic, .youtubeMusic] {
            search.resolve(name: "Song", artist: "Artist", album: "", source: source) { outcome in
                XCTAssertEqual(outcome, .missing)
                done.fulfill()
            }
        }
        wait(for: [done], timeout: 6)
        let entries = requests.entries
        XCTAssertEqual(entries.count, 2)
        if entries.count == 2 {
            XCTAssertGreaterThanOrEqual(entries[1].time.timeIntervalSince(entries[0].time), 2.9)
        }
    }

    func testSkipsReplaceOnlyTheSameSourcesWaitingTrack() {
        let requests = RequestLog()
        CatalogURLProtocol.handle { request in
            requests.record(request)
            return .missing
        }
        let search = ITunesSearchClient(session: session, minRequestInterval: 0.1)
        prime(search)
        let done = expectation(description: "Both current tracks resolved")
        done.expectedFulfillmentCount = 2
        search.resolve(name: "Old", artist: "Artist", album: "") { _ in
            XCTFail("The skipped track must not resolve")
        }
        search.resolve(name: "YouTube", artist: "Artist", album: "", source: .youtubeMusic) { _ in done.fulfill() }
        search.resolve(name: "New", artist: "Artist", album: "") { _ in done.fulfill() }
        wait(for: [done], timeout: 2)
        XCTAssertEqual(requests.entries.map(\.term), ["Prime Artist", "YouTube Artist", "New Artist"])
    }

    func testOtherSourcesRequestDoesNotDiscardStorefrontFallback() {
        let requests = RequestLog()
        let firstYouTubeRequest = expectation(description: "First storefront requested")
        CatalogURLProtocol.handle { request in
            requests.record(request)
            if requests.entries.count == 1 { firstYouTubeRequest.fulfill() }
            return CatalogURLProtocol.Stub(delay: 0.05)
        }
        let search = ITunesSearchClient(session: session, minRequestInterval: 0.05)
        let youtube = expectation(description: "All YouTube storefronts searched")
        search.resolve(
            name: "YouTube", artist: "Artist", album: "", source: .youtubeMusic,
            fallbackStorefronts: ["US", "JP"]
        ) { _ in youtube.fulfill() }
        wait(for: [firstYouTubeRequest], timeout: 1)
        let apple = expectation(description: "Apple request resolved")
        search.resolve(name: "Apple", artist: "Artist", album: "") { _ in apple.fulfill() }
        wait(for: [apple, youtube], timeout: 2)
        let countries = Set(requests.entries.filter { $0.term == "YouTube Artist" }.map(\.country))
        XCTAssertTrue(countries.isSuperset(of: ["US", "JP"]))
    }

    func testCachedTrackSupersedesTheSameSourcesPendingSearch() {
        let requests = RequestLog()
        CatalogURLProtocol.handle { request in
            requests.record(request)
            return .missing
        }
        let search = ITunesSearchClient(session: session, minRequestInterval: 0.1)
        prime(search)
        search.resolve(name: "Skipped", artist: "Artist", album: "") { _ in
            XCTFail("The pending track was superseded by the cached track")
        }
        let cached = expectation(description: "Cached result returned")
        search.resolve(name: "Prime", artist: "Artist", album: "") { _ in cached.fulfill() }
        let settled = expectation(description: "Rate limit window elapsed")
        DispatchQueue.main.asyncAfter(deadline: .now() + 0.2) { settled.fulfill() }
        wait(for: [cached, settled], timeout: 1)
        XCTAssertEqual(requests.entries.map(\.term), ["Prime Artist"])
    }

    func testLateYouTubeFailureDoesNotQueueThePreviousTracksITunesFallback() {
        let requests = RequestLog()
        let oldSearch = expectation(description: "Old YouTube search started")
        CatalogURLProtocol.handle { request in
            if request.url?.host == "music.youtube.com" {
                let body = CatalogURLProtocol.body(of: request)
                if body.contains("Old") {
                    oldSearch.fulfill()
                    return CatalogURLProtocol.Stub(status: 503, delay: 0.08)
                }
                return CatalogURLProtocol.Stub(status: 503, delay: 0.01)
            }
            requests.record(request)
            return .missing
        }
        let itunes = ITunesSearchClient(session: session, minRequestInterval: 0.2)
        prime(itunes)
        let app = AppDelegate(
            rpc: SilentRPC(), itunes: itunes,
            youtubeSearch: YouTubeMusicSearchClient(session: session))
        defer { app.cancelCatalogRetry(.youtubeMusic) }
        app.youtube.running = true
        app.youtube.track = TrackInfo(name: "Old", artist: "Artist", album: "")
        app.requestMissingCatalog(.youtubeMusic)
        wait(for: [oldSearch], timeout: 1)
        app.youtube.clearCatalog()
        app.youtube.track = TrackInfo(name: "New", artist: "Artist", album: "")
        app.requestMissingCatalog(.youtubeMusic)
        let settled = expectation(description: "Fallback requests completed")
        DispatchQueue.main.asyncAfter(deadline: .now() + 0.8) { settled.fulfill() }
        wait(for: [settled], timeout: 2)
        let terms = requests.entries.map(\.term)
        XCTAssertTrue(terms.contains("New Artist"))
        XCTAssertFalse(terms.contains("Old Artist"))
    }

    private func prime(_ search: ITunesSearchClient) {
        let done = expectation(description: "Initial request resolved")
        search.resolve(name: "Prime", artist: "Artist", album: "") { _ in done.fulfill() }
        wait(for: [done], timeout: 1)
    }
}

private final class RequestLog {
    struct Entry {
        let term: String
        let country: String
        let time: Date
    }

    private let lock = NSLock()
    private var recorded: [Entry] = []

    var entries: [Entry] { lock.withLock { recorded } }

    func record(_ request: URLRequest) {
        let items = URLComponents(url: request.url!, resolvingAgainstBaseURL: false)!.queryItems!
        let entry = Entry(
            term: items.first { $0.name == "term" }!.value!,
            country: items.first { $0.name == "country" }!.value!, time: Date())
        lock.withLock { recorded.append(entry) }
    }
}

private final class CatalogURLProtocol: URLProtocol {
    struct Stub {
        var status = 200
        var delay: TimeInterval = 0
        static let missing = Stub()
    }

    private static let lock = NSLock()
    private static var handler: ((URLRequest) -> Stub)!
    private var delivery: DispatchWorkItem?

    static func handle(_ handler: @escaping (URLRequest) -> Stub) {
        lock.withLock { self.handler = handler }
    }

    static func body(of request: URLRequest) -> String {
        if let data = request.httpBody { return String(data: data, encoding: .utf8)! }
        let stream = request.httpBodyStream!
        stream.open()
        defer { stream.close() }
        var data = Data()
        var buffer = [UInt8](repeating: 0, count: 1024)
        while stream.hasBytesAvailable {
            let count = stream.read(&buffer, maxLength: buffer.count)
            precondition(count >= 0)
            if count == 0 { break }
            data.append(contentsOf: buffer.prefix(count))
        }
        return String(data: data, encoding: .utf8)!
    }

    override static func canInit(with _: URLRequest) -> Bool { true }
    override static func canonicalRequest(for request: URLRequest) -> URLRequest { request }

    override func startLoading() {
        let stub = Self.lock.withLock { Self.handler(request) }
        let work = DispatchWorkItem { [self] in
            client!.urlProtocol(
                self,
                didReceive: HTTPURLResponse(url: request.url!, statusCode: stub.status, httpVersion: nil, headerFields: nil)!,
                cacheStoragePolicy: .notAllowed)
            client!.urlProtocol(self, didLoad: Data("{\"results\":[]}".utf8))
            client!.urlProtocolDidFinishLoading(self)
        }
        delivery = work
        DispatchQueue.global().asyncAfter(deadline: .now() + stub.delay, execute: work)
    }

    override func stopLoading() { delivery?.cancel() }
}

final class SilentRPC: DiscordRPCConnection {
    var state: DiscordRPCClient.ConnState = .disconnected
    var onStateChange: ((DiscordRPCClient.ConnState) -> Void)?

    func connect(clientId _: String) {}
    func setActivity(_: [String: Any]?) {}
    func shutdown(clearActivity _: Bool) {}
    func shutdownSync(clearActivity _: Bool) {}
}
