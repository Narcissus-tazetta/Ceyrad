import Foundation
import os

/// Spotify向けAppleScript。呼び出しは「起動時の初期状態取得」と「アートワークURL取得」のみ。
/// 再生位置は通知（PlaybackStateChanged）に含まれるため、Apple Musicと違い位置補完は不要。
/// Spotifyが無応答でもApple Music側の補完をブロックしないよう、キューはMusicAppleScriptと分ける。
enum SpotifyAppleScript {
    /// 直近の呼び出しがオートメーション権限拒否(-1743)で失敗したか。
    /// メニューバーの警告表示に使う。メインスレッドからのみ触る。
    private(set) static var notAuthorized = false

    private static let log = Logger(
        subsystem: Bundle.main.bundleIdentifier ?? "Ceyrad", category: "SpotifyAppleScript"
    )
    private static let queue = DispatchQueue(label: "spotify-applescript", qos: .userInitiated)

    /// コンパイル結果を保持して使い回す（初回実行時にコンパイルされ、以後は再利用される）
    private static let stateScript = NSAppleScript(
        source: """
            tell application "Spotify"
                set sep to character id 31
                set ps to (player state as text)
                set out to ps
                try
                    set t to current track
                    set pos to "0"
                    try
                        set pos to (player position as text)
                    end try
                    set out to ps & sep & (name of t) & sep & (artist of t) & sep & ¬
                        (album of t) & sep & ((duration of t) as text) & sep & pos & sep & (id of t)
                end try
                return out
            end tell
            """
    )

    private static let artworkScript = NSAppleScript(
        source: """
            tell application "Spotify"
                set sep to character id 31
                try
                    set t to current track
                    return (id of t) & sep & (artwork url of t)
                on error
                    return ""
                end try
            end tell
            """
    )

    /// 本アプリ起動時にSpotifyが既に再生中だった場合の初期状態取得。
    /// 通知は状態変化時にしか飛ばないため、この1回だけ能動的に取得する。completionはメインスレッド。
    static func currentState(completion: @escaping ((PlayerState, TrackInfo?)?) -> Void) {
        queue.async {
            let raw = run(stateScript)
            let parsed = raw.map(parseState)
            DispatchQueue.main.async { completion(parsed) }
        }
    }

    /// 現在の曲のアートワークURL（i.scdn.coの直接CDN URL）。
    /// 通知〜実行の間に曲が替わっていた場合を弾くため、現在のtrack idが
    /// 要求と一致するときだけ返す。completionはメインスレッド。
    static func artworkURL(forTrackId trackId: String, completion: @escaping (String?) -> Void) {
        queue.async {
            let raw = run(artworkScript)
            let url = raw.flatMap { parseArtwork($0, expectedTrackId: trackId) }
            DispatchQueue.main.async { completion(url) }
        }
    }

    static func parseState(_ raw: String) -> (PlayerState, TrackInfo?) {
        let parts = raw.components(separatedBy: "\u{1F}")
        let state: PlayerState
        switch parts[0].lowercased() {
        case "playing": state = .playing
        case "paused": state = .paused
        default: state = .stopped
        }
        guard state != .stopped, parts.count >= 7 else { return (state, nil) }
        var track = TrackInfo(
            name: parts[1],
            artist: parts[2],
            album: parts[3],
            durationSec: nil,
            positionSec: MusicAppleScript.parseDouble(parts[5]),
            trackId: parts[6].isEmpty ? nil : parts[6]
        )
        // Apple Musicと違いdurationはミリ秒
        if let ms = MusicAppleScript.parseDouble(parts[4]), ms > 0 {
            track.durationSec = ms / 1000.0
        }
        return (state, track)
    }

    static func parseArtwork(_ raw: String, expectedTrackId: String) -> String? {
        let parts = raw.components(separatedBy: "\u{1F}")
        guard parts.count == 2, parts[0] == expectedTrackId,
            parts[1].lowercased().hasPrefix("https://")
        else { return nil }
        return parts[1]
    }

    private static func run(_ script: NSAppleScript?) -> String? {
        guard let script else { return nil }
        var error: NSDictionary?
        let result = script.executeAndReturnError(&error)
        // -1743 = errAEEventNotPermitted（オートメーション権限なし）
        let denied = (error?[NSAppleScript.errorNumber] as? Int) == -1743
        if let error {
            log.error("AppleScript error: \(String(describing: error), privacy: .public)")
        }
        DispatchQueue.main.async { notAuthorized = denied }
        return error == nil ? result.stringValue : nil
    }
}
