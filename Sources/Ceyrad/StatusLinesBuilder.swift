import Foundation

/// メニューバーに出すステータス行の組み立て。値入力の関数としてテスト可能にする。
enum StatusLinesBuilder {
    struct Input {
        var appleMusic = SourceState()
        var appleMusicNotAuthorized = false
        var discordState: DiscordRPCClient.ConnState = .disconnected
    }

    static func lines(_ input: Input) -> [String] {
        var lines = [sourceLine(input)]
        lines.append(contentsOf: automationLines(input))
        lines.append(discordLine(input))
        return lines
    }

    private static func sourceLine(_ input: Input) -> String {
        let name = MusicSourceDescriptor.appleMusic.displayName
        let s = input.appleMusic
        guard s.running else { return t("\(name): Not Running", "\(name): 未起動") }
        switch s.playerState {
        case .stopped:
            return t("\(name): Stopped", "\(name): 停止中")
        case .playing:
            return "♪ \(name): \(trackLine(s.track))"
        case .paused:
            return "⏸ \(name): \(trackLine(s.track))"
        }
    }

    private static func automationLines(_ input: Input) -> [String] {
        guard input.appleMusic.running, input.appleMusicNotAuthorized else { return [] }
        return [
            t("Music Control: Not Authorized ⚠️", "ミュージックの操作: 未許可 ⚠️"),
            t(
                "(System Settings > Privacy > Automation)",
                "(システム設定 > プライバシーとセキュリティ > オートメーション)"
            ),
        ]
    }

    private static func discordLine(_ input: Input) -> String {
        guard input.appleMusic.running else {
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
