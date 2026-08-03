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
    /// positionSecを取得した時刻。再生中に古いpositionSecのままActivityを再送すると
    /// タイムスタンプが巻き戻ってDiscordの進捗バーがリセットされて見えるため、
    /// 送信時にここからの経過分を足して補正する。
    var positionSampledAt = Date()

    var identity: String {
        "\(name)\u{1F}\(artist)\u{1F}\(album)"
    }
}

/// カタログ情報（iTunes Search APIの検索結果から得られるURL群）
struct CatalogInfo: Equatable {
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

/// カタログ照会の結果。
///
/// 「見つからなかった」と「訊けなかった」を区別する。前者は曲の性質（ローカル取り込み等）
/// なので確定した答えだが、後者は答えではないので、その曲がアートワークを永久に失わない
/// よう再試行に値する。
enum CatalogOutcome: Equatable {
    case found(CatalogInfo)
    case missing
    case failed
}

/// Apple Musicの現在状態。プレイヤーは1つなので、これがアプリの音楽側の全状態になる。
struct MusicState {
    var running = false
    var playerState: PlayerState = .stopped
    var track: TrackInfo?
    var catalog: CatalogInfo?
    /// カタログ照会をすでに投げた曲のidentity。通知が続いても同じ曲を訊き直さないための番人。
    var catalogRequestedFor: String?
    /// 失敗した照会を再試行してよくなる時刻。`nil` は待機中でないことを表す。
    var catalogRetryAt: Date?

    /// Discordに表示する候補か。稼働中 && 曲あり && 停止中でない。
    /// 一時停止も候補ではある（実際に消すかどうかはpauseHideMinutes側の判断）。
    var isDisplayable: Bool {
        running && track != nil && playerState != .stopped
    }

    /// 曲が変わった・プレイヤーが止まった等でカタログを白紙に戻す。
    /// 照会の番人も一緒に落とさないと、次の曲が永久に訊かれない。
    mutating func clearCatalog() {
        catalog = nil
        catalogRequestedFor = nil
        catalogRetryAt = nil
    }
}
