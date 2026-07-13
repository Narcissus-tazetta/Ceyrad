import Foundation

/// プレイヤーごとの宣言的な差分（通知名・バンドルID・client ID・通知パース）。
/// 振る舞いの差分（位置補完・カタログ解決）はAppDelegate側のswitchで扱う。
struct MusicSourceDescriptor {
    let id: MusicSourceID
    let bundleId: String
    let notificationName: Notification.Name
    let discordClientId: String
    /// メニュー表示等に使う名前。Discord側の「〜を再生中」の名前はApplication名で決まる。
    let displayName: String
    let parse: ([AnyHashable: Any]) -> (PlayerState, TrackInfo?)

    static let appleMusic = MusicSourceDescriptor(
        id: .appleMusic,
        bundleId: "com.apple.Music",
        notificationName: Notification.Name("com.apple.Music.playerInfo"),
        discordClientId: SettingsStore.discordClientId,
        displayName: "Apple Music",
        parse: AppleMusicNotification.parse
    )

    static let spotify = MusicSourceDescriptor(
        id: .spotify,
        bundleId: "com.spotify.client",
        notificationName: Notification.Name("com.spotify.client.PlaybackStateChanged"),
        discordClientId: SettingsStore.spotifyDiscordClientId,
        displayName: "Spotify",
        parse: SpotifyNotification.parse
    )

    static func descriptor(for id: MusicSourceID) -> MusicSourceDescriptor {
        switch id {
        case .appleMusic: return .appleMusic
        case .spotify: return .spotify
        }
    }
}

/// `com.apple.Music.playerInfo` のuserInfoパース。
/// 通知には再生位置が含まれないため、位置はAppleScriptで補完する（AppDelegate側）。
enum AppleMusicNotification {
    static func parse(_ info: [AnyHashable: Any]) -> (PlayerState, TrackInfo?) {
        let state: PlayerState
        switch (info["Player State"] as? String ?? "").lowercased() {
        case "playing": state = .playing
        case "paused": state = .paused
        default: state = .stopped
        }
        guard state != .stopped else { return (.stopped, nil) }
        var track = TrackInfo(
            name: info["Name"] as? String ?? "",
            artist: info["Artist"] as? String ?? "",
            album: info["Album"] as? String ?? "",
            durationSec: nil,
            positionSec: nil
        )
        if let ms = (info["Total Time"] as? NSNumber)?.doubleValue, ms > 0 {
            track.durationSec = ms / 1000.0
        }
        return (state, track)
    }
}

/// `com.spotify.client.PlaybackStateChanged` のuserInfoパース。
/// Apple Musicと違い再生位置（秒）が通知に含まれるため、AppleScriptでの位置補完は不要。
/// Durationはミリ秒、Playback Positionは秒で届く点に注意。
enum SpotifyNotification {
    static func parse(_ info: [AnyHashable: Any]) -> (PlayerState, TrackInfo?) {
        let state: PlayerState
        switch (info["Player State"] as? String ?? "").lowercased() {
        case "playing": state = .playing
        case "paused": state = .paused
        default: state = .stopped
        }
        guard state != .stopped else { return (.stopped, nil) }
        var track = TrackInfo(
            name: info["Name"] as? String ?? "",
            artist: info["Artist"] as? String ?? "",
            album: info["Album"] as? String ?? "",
            durationSec: nil,
            positionSec: (info["Playback Position"] as? NSNumber)?.doubleValue,
            trackId: info["Track ID"] as? String
        )
        if let ms = (info["Duration"] as? NSNumber)?.doubleValue, ms > 0 {
            track.durationSec = ms / 1000.0
        }
        return (state, track)
    }
}
