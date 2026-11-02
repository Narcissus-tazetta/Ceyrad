import Foundation

/// メニューバーに出すステータス行の組み立て。値入力の関数としてテスト可能にする。
enum StatusLinesBuilder {
    struct Input {
        var music = MusicState()
        var musicNotAuthorized = false
        var youtube = MusicState()
        var youtubeEnabled = false
        /// 検知が有効なのに取得元が使えないときの理由。
        var youtubeUnavailable: String?
        var browserNotAuthorized = false
        var discordState: DiscordRPCClient.ConnState = .disconnected
        var language: AppLanguage = .en
    }

    static func lines(_ input: Input) -> [String] {
        var lines = [sourceLine(input)]
        lines.append(contentsOf: automationLines(input))
        lines.append(contentsOf: youtubeLines(input))
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

    /// 検知がオフのあいだは何も出さない（機能そのものを見せない）。
    private static func youtubeLines(_ input: Input) -> [String] {
        guard input.youtubeEnabled else { return [] }
        let name = YouTubeMusic.displayName
        let language = input.language
        var lines: [String] = []

        if let reason = input.youtubeUnavailable {
            lines.append(t(language, "\(name): Unavailable ⚠️", "\(name): 利用不可 ⚠️"))
            lines.append("(\(reason))")
        } else if !input.youtube.running {
            lines.append(t(language, "\(name): Starting…", "\(name): 起動中…"))
        } else if input.youtube.track != nil {
            let glyph = input.youtube.playerState == .paused ? "⏸" : "♪"
            lines.append("\(glyph) \(name): \(trackLine(input.youtube.track))")
        } else {
            lines.append(t(language, "\(name): Waiting for playback", "\(name): 再生待ち"))
        }

        if input.browserNotAuthorized {
            lines.append(t(language, "Browser Control: Not Authorized ⚠️", "ブラウザの操作: 未許可 ⚠️"))
            lines.append(
                t(
                    language,
                    "(System Settings > Privacy > Automation)",
                    "(システム設定 > プライバシーとセキュリティ > オートメーション)"
                )
            )
        }
        return lines
    }

    /// Discordに関する文言は常に英語。Discord側の表記と揃えるため翻訳しない。
    private static func discordLine(_ input: Input) -> String {
        guard input.music.running || input.youtube.isDisplayable else {
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
