import Foundation

enum PlayerState {
    case playing
    case paused
    case stopped
}

struct TrackInfo: Equatable {
    var name: String
    var artist: String
    var album: String
    var durationSec: Double?
    var positionSec: Double?

    var identity: String {
        "\(name)\u{1F}\(artist)\u{1F}\(album)"
    }
}

/// iTunes Search APIで解決したカタログ情報
struct CatalogInfo {
    var songURL: String?
    var artistURL: String?
    var albumURL: String?
    var artworkURL: String?
}
