import Foundation

/// メニューバーに出すステータス行の組み立て。値入力の関数としてテスト可能にする。
enum StatusLinesBuilder {
    struct Input {
        var music = MusicState()
        var musicNotAuthorized = false
        var discordState: DiscordRPCClient.ConnState = .disconnected
        var language: AppLanguage = .en
    }

    static func lines(_ input: Input) -> [String] {
        var lines = [sourceLine(input)]
        lines.append(contentsOf: automationLines(input))
        lines.append(discordLine(input))
        return lines
    }

    private static func sourceLine(_ input: Input) -> String {
        let name = AppleMusic.displayName
        let language = input.language
        let s = input.music
        guard s.running else { return t(language, "\(name): Not Running", "\(name): 未起動") }
        switch s.playerState {
        case .stopped:
            return t(language, "\(name): Stopped", "\(name): 停止中")
        case .playing:
            return "♪ \(name): \(trackLine(s.track))"
        case .paused:
            return "⏸ \(name): \(trackLine(s.track))"
        }
    }

    private static func automationLines(_ input: Input) -> [String] {
        guard input.music.running, input.musicNotAuthorized else { return [] }
        let language = input.language
        return [
            t(language, "Music Control: Not Authorized ⚠️", "ミュージックの操作: 未許可 ⚠️"),
            t(
                language,
                "(System Settings > Privacy > Automation)",
                "(システム設定 > プライバシーとセキュリティ > オートメーション)"
            ),
        ]
    }

    /// Discordに関する文言は常に英語。Discord側の表記と揃えるため翻訳しない。
    private static func discordLine(_ input: Input) -> String {
        guard input.music.running else {
            return "Discord: Idle (connects when a player starts)"
        }
        switch input.discordState {
        case .connected: return "Discord: Connected"
        case .connecting: return "Discord: Connecting…"
        case .disconnected: return "Discord: Disconnected (retrying)"
        }
    }

    private static func trackLine(_ track: TrackInfo?) -> String {
        guard let track else { return "–" }
        let artist = track.artist.isEmpty ? "" : " — \(track.artist)"
        return "\(track.name)\(artist)"
    }
}
