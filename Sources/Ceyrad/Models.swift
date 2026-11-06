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

    /// Apple Musicの広告。実際の通知は未計測で、広告中のDiscord表示（曲名が数字だけで
    /// アーティスト不明）から推測した条件。本物の曲がこの形になることは通常ない。
    var isAdvertisement: Bool {
        !name.isEmpty && name.allSatisfy(\.isASCII) && name.allSatisfy(\.isNumber)
            && artist.isEmpty && album.isEmpty
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

/// Discordに表示できる音楽ソース。Discord側の表示名はApplication（client ID）ごとに決まるため、
/// ソースが変わると接続し直す。
enum MusicSource {
    case appleMusic
    case youtubeMusic
}

/// 1つのソースの現在状態。
struct MusicState {
    /// Apple Music: アプリが起動している。YouTube Music: 検知が有効で、取得元が動いている。
    var running = false
    var playerState: PlayerState = .stopped
    var track: TrackInfo?
    /// `.playing` に遷移した時刻。複数のソースが同時に鳴っているとき、後から始めた方を選ぶ根拠。
    var playingSince: Date?
    /// ソース自身の曲ページ（YouTube Musicではブラウザのタブから得たURL）。
    var pageURL: String?
    var catalog: CatalogInfo?
    /// カタログ照会をすでに投げた曲のidentity。通知が続いても同じ曲を訊き直さないための番人。
    var catalogRequestedFor: String?
    /// 失敗した照会を再試行してよくなる時刻。`nil` は待機中でないことを表す。
    var catalogRetryAt: Date?

    /// Discordに表示する候補か。稼働中 && 曲あり（広告でない）&& 停止中でない。
    /// 一時停止も候補ではある（実際に消すかどうかはpauseHideMinutes側の判断）。
    var isDisplayable: Bool {
        running && track.map { !$0.isAdvertisement } == true && playerState != .stopped
    }

    /// 曲が変わった・プレイヤーが止まった等でカタログを白紙に戻す。
    /// 照会の番人も一緒に落とさないと、次の曲が永久に訊かれない。
    mutating func clearCatalog() {
        catalog = nil
        catalogRequestedFor = nil
        catalogRetryAt = nil
    }
}

/// どのソースをDiscordに出すか。
///
/// 再生中は一時停止より優先し、同じ状態どうしなら後から再生を始めた方を選ぶ。
/// 状態だけから決まる純関数にして、切り替えの規則をテストで固定する。
enum SourceSelector {
    static func select(appleMusic: MusicState, youtubeMusic: MusicState) -> MusicSource? {
        switch (appleMusic.isDisplayable, youtubeMusic.isDisplayable) {
        case (false, false): return nil
        case (true, false): return .appleMusic
        case (false, true): return .youtubeMusic
        case (true, true): break
        }

        let appleIsPlaying = appleMusic.playerState == .playing
        if appleIsPlaying != (youtubeMusic.playerState == .playing) {
            return appleIsPlaying ? .appleMusic : .youtubeMusic
        }
        switch (appleMusic.playingSince, youtubeMusic.playingSince) {
        case (let apple?, let youtube?): return youtube > apple ? .youtubeMusic : .appleMusic
        case (nil, _?): return .youtubeMusic
        default: return .appleMusic
        }
    }
}
