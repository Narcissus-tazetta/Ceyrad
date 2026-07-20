import Foundation

enum ActivityBuilder {
    static func build(
        track: TrackInfo, playerState: PlayerState,
        catalog: CatalogInfo?, settings: SettingsStore,
        source: MusicSourceID = .appleMusic
    ) -> [String: Any] {
        // type 2 = Listening（「〜を再生中」表示）
        var activity: [String: Any] = ["type": 2]

        let name = track.name.isEmpty ? "Unknown Track" : track.name
        activity["details"] = clamp(name)

        let artist = track.artist.isEmpty ? "Unknown Artist" : track.artist
        activity["state"] = clamp(artist)

        // 一時停止中は停止位置を明示する（positionは停止イベント時に取得済みで進まない）
        let pauseLabel: String?
        if playerState == .paused {
            if let position = track.positionSec {
                pauseLabel = "⏸ Paused at \(formatTime(position))"
            } else {
                pauseLabel = "⏸ Paused"
            }
        } else {
            pauseLabel = nil
        }

        // バッジ（「〜を再生中」）の表示元フィールドを指定する
        var badgeLabel = settings.badgeLabel

        if let artwork = catalog?.artworkURL {
            var assets: [String: Any] = ["large_image": artwork]
            // アルバム行の末尾に停止位置を続ける（例: 幻燈 · ⏸ Paused at 2:48）
            var largeText = track.album
            if let pauseLabel {
                largeText = largeText.isEmpty ? pauseLabel : "\(largeText) · \(pauseLabel)"
            }
            if !largeText.isEmpty {
                assets["large_text"] = clamp(largeText)
            }
            activity["assets"] = assets
        } else if let pauseLabel {
            // アートワークなしだとアルバム行自体が表示されないため、アーティスト行に載せる
            activity["state"] = clamp("\(pauseLabel) · \(artist)")
            // ポーズ文字列入りのstateがバッジに出ると読みにくいため、アプリ名表示へ退避
            if badgeLabel == .artist {
                badgeLabel = .appName
            }
        }
        activity["status_display_type"] = badgeLabel.rawValue

        // プログレスバーは再生中のみ。positionは通知発火時にAppleScriptで補完済み。
        if playerState == .playing,
            let position = track.positionSec,
            let duration = track.durationSec, duration > 0
        {
            let start = Date().timeIntervalSince1970 - position
            activity["timestamps"] = [
                "start": Int((start * 1000).rounded()),
                "end": Int(((start + duration) * 1000).rounded()),
            ]
        }

        let buttons = buildButtons(catalog: catalog, settings: settings, source: source)
        if !buttons.isEmpty {
            activity["buttons"] = buttons
        }
        return activity
    }

    /// Discord RPC仕様: ボタンは最大2個、label<=32文字、url<=512文字
    private static func buildButtons(
        catalog: CatalogInfo?, settings: SettingsStore, source: MusicSourceID
    ) -> [[String: String]] {
        var buttons: [[String: String]] = []
        var usedURLs = Set<String>()
        // 未カスタマイズのラベルは再生中のソースに追従する（"Play on Spotify" 等）
        let configs: [(LinkType, String)] = [
            (settings.button1Type, settings.button1Label(for: source)),
            (settings.button2Type, settings.button2Label(for: source)),
        ]
        for (type, label) in configs {
            guard type != .disabled,
                let url = resolveURL(type: type, catalog: catalog, settings: settings),
                isValidButtonURL(url),
                !usedURLs.contains(url)
            else { continue }
            buttons.append([
                "label": label.isEmpty ? "Link" : String(label.prefix(32)),
                "url": url,
            ])
            usedURLs.insert(url)
        }
        return buttons
    }

    /// Catalog URLが解決できなかった場合（ローカル取り込み曲など）はそのボタンを出さない
    private static func resolveURL(
        type: LinkType, catalog: CatalogInfo?,
        settings: SettingsStore
    ) -> String? {
        switch type {
        case .song: return catalog?.songURL
        case .artist: return catalog?.artistURL
        case .album: return catalog?.albumURL
        case .custom: return settings.customURL
        case .repository: return settings.repositoryURL
        case .disabled: return nil
        }
    }

    static func isValidButtonURL(_ string: String) -> Bool {
        guard string.count <= 512,
            let url = URL(string: string),
            let scheme = url.scheme?.lowercased()
        else { return false }
        return scheme == "http" || scheme == "https"
    }

    /// 秒数を 0:53 / 12:04 / 1:02:30 形式に整形
    private static func formatTime(_ seconds: Double) -> String {
        let total = max(0, Int(seconds.rounded()))
        let h = total / 3600
        let m = (total % 3600) / 60
        let s = total % 60
        if h > 0 {
            return String(format: "%d:%02d:%02d", h, m, s)
        }
        return String(format: "%d:%02d", m, s)
    }

    /// Discordの文字列フィールドは2〜128文字が必須。
    /// 埋め草には、Discord側で末尾トリムされうる空白ではなく
    /// 空白扱いされない点字ブランク（U+2800）を使う。
    private static func clamp(_ s: String) -> String {
        var value = String(s.prefix(128))
        while value.count < 2 {
            value += "\u{2800}"
        }
        return value
    }
}
