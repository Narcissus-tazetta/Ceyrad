import Foundation
import os

/// AppleScript呼び出しは「通知発火時 or Music起動直後の初期化時」のみ。定期ポーリングはしない。
/// 実行は専用シリアルキューで行い、メインスレッドをブロックしない
/// （Music無応答時やオートメーション許可ダイアログ待ちでUIが固まらないように）。
/// NSAppleScriptはスレッドセーフでないため、生成・実行ともこのキューに閉じ込める。
enum MusicAppleScript {
    /// 直近の呼び出しがオートメーション権限拒否(-1743)で失敗したか。
    /// メニューバーの警告表示に使う。メインスレッドからのみ触る。
    private(set) static var notAuthorized = false

    private static let log = Logger(
        subsystem: Bundle.main.bundleIdentifier ?? "Cadence", category: "AppleScript"
    )
    private static let queue = DispatchQueue(label: "music-applescript", qos: .userInitiated)

    /// コンパイル結果を保持して使い回す（初回実行時にコンパイルされ、以後は再利用される）
    private static let positionScript = NSAppleScript(
        source: "tell application \"Music\" to get (player position as text)"
    )

    private static let stateScript = NSAppleScript(
        source: """
            tell application "Music"
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
                        (album of t) & sep & ((duration of t) as text) & sep & pos
                end try
                return out
            end tell
            """
    )

    /// 現在の再生位置（秒）。通知に含まれないためここでのみ補完する。completionはメインスレッド。
    static func playerPosition(completion: @escaping (Double?) -> Void) {
        queue.async {
            let raw = run(positionScript)
            DispatchQueue.main.async { completion(raw.flatMap(parseDouble)) }
        }
    }

    /// 本アプリ起動時にMusicが既に再生中だった場合の初期状態取得。
    /// 通知は状態変化時にしか飛ばないため、この1回だけ能動的に取得する。completionはメインスレッド。
    static func currentState(completion: @escaping ((PlayerState, TrackInfo?)?) -> Void) {
        queue.async {
            let raw = run(stateScript)
            let parsed = raw.map(parseState)
            DispatchQueue.main.async { completion(parsed) }
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
        guard state != .stopped, parts.count >= 6 else { return (state, nil) }
        let track = TrackInfo(
            name: parts[1],
            artist: parts[2],
            album: parts[3],
            durationSec: parseDouble(parts[4]),
            positionSec: parseDouble(parts[5])
        )
        return (state, track)
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

    /// AppleScriptの実数→文字列変換はロケールによって小数点がカンマになる
    static func parseDouble(_ string: String) -> Double? {
        Double(string.replacingOccurrences(of: ",", with: "."))
    }
}
