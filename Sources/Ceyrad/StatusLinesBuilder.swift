import Foundation

/// メニューバーに出すステータス行の組み立て。値入力の関数としてテスト可能にする。
enum StatusLinesBuilder {
    struct Input {
        var appleMusic = SourceState()
        var spotify = SourceState()
        var activeSource: MusicSourceID?
        var appleMusicEnabled = true
        var spotifyEnabled = true
        var appleMusicNotAuthorized = false
        var spotifyNotAuthorized = false
        var discordState: DiscordRPCClient.ConnState = .disconnected
    }

    static func lines(_ input: Input) -> [String] {
        var lines = sourceLines(input)
        lines.append(contentsOf: automationLines(input))
        lines.append(discordLine(input))
        return lines
    }

    private static func sourceLines(_ input: Input) -> [String] {
        let sources = enabledSources(input)
        // 複数ソースに曲があるときだけ、どちらを表示中か明示する
        let markActive = sources.filter { state(for: $0, input).track != nil }.count >= 2
        return sources.map { sourceLine(for: $0, input: input, markActive: markActive) }
    }

    private static func sourceLine(
        for source: MusicSourceID, input: Input, markActive: Bool
    ) -> String {
        let name = MusicSourceDescriptor.descriptor(for: source).displayName
        let s = state(for: source, input)
        guard s.running else { return t("\(name): Not Running", "\(name): 未起動") }
        var line: String
        switch s.playerState {
        case .stopped:
            line = t("\(name): Stopped", "\(name): 停止中")
        case .playing:
            line = "♪ \(name): \(trackLine(s.track))"
        case .paused:
            line = "⏸ \(name): \(trackLine(s.track))"
        }
        if markActive, source == input.activeSource {
            line += t(" (shown)", "（表示中）")
        }
        return line
    }

    private static func automationLines(_ input: Input) -> [String] {
        var warnings: [String] = []
        if input.appleMusic.running, input.appleMusicNotAuthorized {
            warnings.append(t("Music Control: Not Authorized ⚠️", "ミュージックの操作: 未許可 ⚠️"))
        }
        if input.spotify.running, input.spotifyNotAuthorized {
            warnings.append(t("Spotify Control: Not Authorized ⚠️", "Spotifyの操作: 未許可 ⚠️"))
        }
        guard !warnings.isEmpty else { return [] }
        warnings.append(
            t(
                "(System Settings > Privacy > Automation)",
                "(システム設定 > プライバシーとセキュリティ > オートメーション)"
            )
        )
        return warnings
    }

    private static func discordLine(_ input: Input) -> String {
        guard input.appleMusic.running || input.spotify.running else {
            return "Discord: Idle (connects when a player starts)"
        }
        switch input.discordState {
        case .connected: return "Discord: Connected"
        case .connecting: return "Discord: Connecting…"
        case .disconnected: return "Discord: Disconnected (retrying)"
        }
    }

    private static func enabledSources(_ input: Input) -> [MusicSourceID] {
        MusicSourceID.allCases.filter {
            $0 == .appleMusic ? input.appleMusicEnabled : input.spotifyEnabled
        }
    }

    private static func state(for source: MusicSourceID, _ input: Input) -> SourceState {
        source == .appleMusic ? input.appleMusic : input.spotify
    }

    private static func trackLine(_ track: TrackInfo?) -> String {
        guard let track else { return "–" }
        let artist = track.artist.isEmpty ? "" : " — \(track.artist)"
        return "\(track.name)\(artist)"
    }
}
