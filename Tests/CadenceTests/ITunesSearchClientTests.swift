import XCTest

@testable import Cadence

final class ITunesSearchClientTests: XCTestCase {
    private func result(
        track: String?, artist: String? = nil, album: String? = nil
    ) -> ITunesSearchClient.SearchResult {
        .init(
            trackName: track, artistName: artist, collectionName: album,
            trackViewUrl: nil, artistViewUrl: nil, collectionViewUrl: nil, artworkUrl100: nil
        )
    }

    // MARK: - pickBest

    func testExactTripleMatchIsPreferred() {
        let results = [
            result(track: "Song", artist: "Artist", album: "Other Album"),
            result(track: "Song", artist: "Artist", album: "Album"),
        ]
        let best = ITunesSearchClient.pickBest(
            from: results, name: "Song", artist: "Artist", album: "Album"
        )
        XCTAssertEqual(best?.collectionName, "Album")
    }

    func testMatchingIsCaseWidthAndDiacriticInsensitive() {
        let results = [result(track: "ＣＡＦÉ　ＳＯＮＧ", artist: "ARTIST", album: "")]
        let best = ITunesSearchClient.pickBest(
            from: results, name: "cafe song", artist: "artist", album: ""
        )
        XCTAssertNotNil(best)
    }

    func testNoMatchReturnsNil() {
        let results = [result(track: "Completely Different", artist: "Someone")]
        XCTAssertNil(
            ITunesSearchClient.pickBest(from: results, name: "Song", artist: "Artist", album: "")
        )
    }

    func testFeaturingNotationDifferenceStillMatches() {
        // ローカル側にfeat.があり、カタログ側にない
        let catalogPlain = [result(track: "Song", artist: "Artist")]
        XCTAssertNotNil(
            ITunesSearchClient.pickBest(
                from: catalogPlain, name: "Song (feat. Guest)", artist: "Artist", album: ""
            )
        )
        // カタログ側にfeat.があり、ローカル側にない
        let catalogFeat = [result(track: "Song (feat. Guest)", artist: "Artist")]
        XCTAssertNotNil(
            ITunesSearchClient.pickBest(from: catalogFeat, name: "Song", artist: "Artist", album: "")
        )
    }

    func testAlbumSingleSuffixDifferenceStillMatchesAsTopTier() {
        let results = [
            result(track: "Song", artist: "Artist", album: "Song"),
            result(track: "Song", artist: "Artist", album: "Greatest Hits"),
        ]
        let best = ITunesSearchClient.pickBest(
            from: results, name: "Song", artist: "Artist", album: "Song - Single"
        )
        XCTAssertEqual(best?.collectionName, "Song")
    }

    func testArtistMatchIsPreferredOverNameOnly() {
        let results = [
            result(track: "Song", artist: "Cover Band"),
            result(track: "Song", artist: "Artist"),
        ]
        let best = ITunesSearchClient.pickBest(
            from: results, name: "Song", artist: "Artist", album: ""
        )
        XCTAssertEqual(best?.artistName, "Artist")
    }

    // MARK: - 表記揺れの除去

    func testStripFeaturing() {
        XCTAssertEqual(ITunesSearchClient.stripFeaturing("Song (feat. A)"), "Song")
        XCTAssertEqual(ITunesSearchClient.stripFeaturing("Song [feat. A & B]"), "Song")
        XCTAssertEqual(ITunesSearchClient.stripFeaturing("Song (with A)"), "Song")
        XCTAssertEqual(ITunesSearchClient.stripFeaturing("Song feat. A"), "Song")
        XCTAssertEqual(ITunesSearchClient.stripFeaturing("Song ft. A"), "Song")
        XCTAssertEqual(ITunesSearchClient.stripFeaturing("Song (FEAT. A)"), "Song")
        XCTAssertEqual(ITunesSearchClient.stripFeaturing("Plain Song"), "Plain Song")
        // 除去で空になる場合は元の文字列を返す
        XCTAssertEqual(ITunesSearchClient.stripFeaturing("(feat. A)"), "(feat. A)")
    }

    func testStripAlbumSuffix() {
        XCTAssertEqual(ITunesSearchClient.stripAlbumSuffix("Album - Single"), "Album")
        XCTAssertEqual(ITunesSearchClient.stripAlbumSuffix("Album - EP"), "Album")
        XCTAssertEqual(ITunesSearchClient.stripAlbumSuffix("Album - single"), "Album")
        XCTAssertEqual(ITunesSearchClient.stripAlbumSuffix("Album"), "Album")
        // 曲中の「-」は誤除去しない
        XCTAssertEqual(ITunesSearchClient.stripAlbumSuffix("Re-Single Album"), "Re-Single Album")
        XCTAssertEqual(ITunesSearchClient.stripAlbumSuffix("- Single"), "- Single")
    }
}
