import Foundation

enum ActivityBuilder {
    /// Discordの文字列フィールドは2〜128文字が必須。
    private static let minFieldCharacters = 2
    private static let maxFieldCharacters = 128
    private static let maxButtonURLCharacters = 512

    /// 埋め草には、Discord側で末尾トリムされうる空白ではなく
    /// 空白扱いされない点字ブランク（U+2800）を使う。
    private static let padCharacter = "\u{2800}"

    static func build(
        track: TrackInfo, playerState: PlayerState,
        catalog: CatalogInfo?, settings: SettingsStore,
        now: Date = Date()
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

        // プログレスバーは再生中のみ。positionは通知発火時にAppleScriptで補完済みだが、
        // ボタン設定変更などposition取得後に発生した再送では値が古くなっているため、
        // 取得時刻からの経過分を足して現在位置を推定する（そうしないと進捗バーが巻き戻る）。
        if playerState == .playing,
            let position = track.positionSec,
            let duration = track.durationSec, duration > 0
        {
            let elapsedSinceSample = max(0, now.timeIntervalSince(track.positionSampledAt))
            let currentPosition = min(position + elapsedSinceSample, duration)
            let start = now.timeIntervalSince1970 - currentPosition
            activity["timestamps"] = [
                "start": Int((start * 1000).rounded()),
                "end": Int(((start + duration) * 1000).rounded()),
            ]
        }

        let buttons = buildButtons(catalog: catalog, settings: settings)
        if !buttons.isEmpty {
            activity["buttons"] = buttons
        }
        return activity
    }

    // MARK: - 再送の抑止

    /// 同じ再生状態から2回組み立てたActivityが「違う」と見なされるまでに許すズレ。
    ///
    /// 組み立てのたびにtimestampsを壁時計から計算し直すためバイト等価には決してならないが、
    /// Discordが同じに描画する内容の再送は、20秒に数回しか許されないレート制限を
    /// 無駄に食うだけになる。
    private static let timestampDriftMs = 2000

    /// Discordがこの2つを同じように描画するか。`nil` は「表示なし」を表す。
    static func isEquivalent(_ a: [String: Any]?, _ b: [String: Any]?) -> Bool {
        guard let a, let b else { return a == nil && b == nil }
        guard a.count == b.count else { return false }
        for (key, aValue) in a {
            guard let bValue = b[key] else { return false }
            if key == "timestamps" {
                guard timestampsEquivalent(aValue, bValue) else { return false }
            } else if !valuesEqual(aValue, bValue) {
                return false
            }
        }
        return true
    }

    private static func timestampsEquivalent(_ a: Any, _ b: Any) -> Bool {
        guard let a = a as? [String: Int], let b = b as? [String: Int],
            let aStart = a["start"], let bStart = b["start"],
            let aEnd = a["end"], let bEnd = b["end"]
        else { return valuesEqual(a, b) }
        // シークはドリフト窓よりはるかに大きくstartを動かすので変更として残る。
        // 単なる再送が動かすのは組み立て間の実時間ぶんだけ。
        return abs(aStart - bStart) <= timestampDriftMs && abs(aEnd - bEnd) <= timestampDriftMs
    }

    /// JSONに入れる値（String / Int / 配列 / 辞書）の等価判定。
    /// Foundationのブリッジに任せることで、配列・辞書も中身まで比較される。
    private static func valuesEqual(_ a: Any, _ b: Any) -> Bool {
        (a as AnyObject).isEqual(b)
    }

    // MARK: - ボタン

    /// Discord RPC仕様: ボタンは最大2個、label<=32文字、url<=512文字
    private static func buildButtons(
        catalog: CatalogInfo?, settings: SettingsStore
    ) -> [[String: String]] {
        var buttons: [[String: String]] = []
        var usedURLs = Set<String>()
        for slot in ButtonSlot.allCases {
            let type = settings.buttonType(slot)
            guard type != .disabled,
                let url = resolveURL(type: type, catalog: catalog, settings: settings),
                isValidButtonURL(url),
                !usedURLs.contains(url)
            else { continue }
            let label = settings.buttonLabel(slot)
            buttons.append([
                "label": label.isEmpty
                    ? "Link" : Text.truncate(label, max: SettingsStore.maxLabelCharacters),
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
        guard string.count <= maxButtonURLCharacters,
            let url = URL(string: string),
            let scheme = url.scheme?.lowercased()
        else { return false }
        return scheme == "http" || scheme == "https"
    }

    // MARK: - 整形

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

    private static func clamp(_ s: String) -> String {
        var value = Text.truncate(s, max: maxFieldCharacters)
        // 全部数えるのではなく2つあるかだけ見る（valueは128スカラー持ちうる）
        while Text.scalarCount(value, upTo: minFieldCharacters) < minFieldCharacters {
            value += padCharacter
        }
        return value
    }
}
