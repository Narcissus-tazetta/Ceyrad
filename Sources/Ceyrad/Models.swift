import Foundation

enum PlayerState {
    case playing
    case paused
    case stopped
}

/// 対応する音楽プレイヤー。宣言的な差分はMusicSourceDescriptor、
/// 振る舞いの差分（位置補完・カタログ解決）はAppDelegateのswitchに置く。
enum MusicSourceID: CaseIterable {
    case appleMusic
    case spotify
}

struct TrackInfo: Equatable {
    var name: String
    var artist: String
    var album: String
    var durationSec: Double?
    var positionSec: Double?
    /// positionSecを取得した時刻。再生中に古いpositionSecのままActivityを再送すると
    /// タイムスタンプが巻き戻ってDiscordの進捗バーがリセットされて見えるため、
    /// 送信時にここからの経過分を足して補正する。
    var positionSampledAt: Date = Date()
    /// Spotifyの"spotify:track:xxx"。Apple Musicはnil。
    var trackId: String?

    var identity: String {
        trackId ?? "\(name)\u{1F}\(artist)\u{1F}\(album)"
    }
}

/// カタログ情報（Apple Music: iTunes Search API / Spotify: Track ID + artwork url）
struct CatalogInfo {
    var songURL: String?
    var artistURL: String?
    var albumURL: String?
    var artworkURL: String?

    init(
        songURL: String? = nil, artistURL: String? = nil,
        albumURL: String? = nil, artworkURL: String? = nil
    ) {
        self.songURL = songURL
        self.artistURL = artistURL
        self.albumURL = albumURL
        self.artworkURL = artworkURL
    }
}
