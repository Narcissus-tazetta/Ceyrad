import Foundation

/// ユーザーがメニューの行をクリックして要求したこと。
///
/// `edit*` は「どのダイアログを開くか」だけを表す。入力された文字列は
/// AppKit側から戻ってきて、対応するsetterがそこで呼ばれる。
enum MenuAction: Equatable {
    case setButtonType(ButtonSlot, LinkType)
    case editButtonLabel(ButtonSlot)
    case editCustomURL
    case editRepositoryURL
    case setBadgeLabel(BadgeLabelType)
    case setPauseHideMinutes(Int)
    case setLanguage(AppLanguage)
    case toggleLaunchAtLogin
    case reconnect
    case checkForUpdates
    case quit
}

/// メニューの1行。表示順に並べる。
indirect enum MenuRow: Equatable {
    /// グレーアウトされた行。クリック不可。ステータス行・見出し・今は無意味な項目がここに来る。
    case info(String)
    case separator
    /// ふつうのコマンド。
    case item(label: String, action: MenuAction)
    /// 選択肢の1つ。`checked` が現在の状態を表す。
    case choice(label: String, action: MenuAction, checked: Bool)
    case submenu(label: String, rows: [MenuRow])
}

/// メニューの中身を「データ」として決める層。
///
/// AppKitの呼び出しを一切含まないので、「いま何が出るか」がメニューを開かずに
/// ユニットテストで答えられる。実際のNSMenuへの変換は`MenuBarController`が行う。
/// Windows版の`core::menu_model`と対になっており、両者の内容は揃えてある。
enum MenuModel {
    struct Input {
        var status: StatusLinesBuilder.Input
        var settings: SettingsStore
        /// 設定ではなくOSから読む。ユーザーがシステム設定側で切れるため、
        /// メニューは常にOSの言い分に従う。
        var launchAtLogin: Bool
    }

    static func build(_ input: Input) -> [MenuRow] {
        let settings = input.settings
        let language = settings.language
        var rows: [MenuRow] = StatusLinesBuilder.lines(input.status).map(MenuRow.info)
        rows.append(.separator)

        for slot in ButtonSlot.allCases {
            rows.append(buttonSubmenu(slot, settings, language))
        }
        rows.append(.separator)

        rows.append(
            .item(
                label: t(language, "Set Custom URL…", "カスタムURLを設定…"),
                action: .editCustomURL
            )
        )
        rows.append(
            .item(
                label: t(language, "Set Repository URL…", "リポジトリURLを設定…"),
                action: .editRepositoryURL
            )
        )
        rows.append(badgeSubmenu(settings, language))
        rows.append(pauseSubmenu(settings, language))
        rows.append(languageSubmenu(language))
        rows.append(
            .choice(
                label: t(language, "Launch at Login", "ログイン時に自動起動"),
                action: .toggleLaunchAtLogin,
                checked: input.launchAtLogin
            )
        )
        rows.append(.separator)

        // Discordを名指しする文言は翻訳しない（Discord側の表記に合わせる）
        rows.append(.item(label: "Reconnect to Discord", action: .reconnect))
        rows.append(.separator)
        rows.append(
            .item(
                label: t(language, "Check for Updates…", "アップデートを確認…"),
                action: .checkForUpdates
            )
        )
        rows.append(.separator)
        rows.append(.item(label: t(language, "Quit Ceyrad", "Ceyradを終了"), action: .quit))
        return rows
    }

    /// 「ボタン1: 曲ページ」のようなサブメニュー付き項目。
    /// サブメニューにリンク先の選択肢と「ラベルを変更」をまとめる。
    private static func buttonSubmenu(
        _ slot: ButtonSlot, _ settings: SettingsStore, _ language: AppLanguage
    ) -> MenuRow {
        let current = settings.buttonType(slot)
        let number = slot.number
        var rows: [MenuRow] = [.info(t(language, "Link Destination", "リンク先"))]

        for candidate in LinkType.selectableCases {
            // 「オフ」はリンク先の一種ではなく「ボタンを消す」操作なので区切って目立たせる
            if candidate == .disabled {
                rows.append(.separator)
            }
            rows.append(
                .choice(
                    label: candidate.displayName(language),
                    action: .setButtonType(slot, candidate),
                    checked: candidate == current
                )
            )
        }

        rows.append(.separator)
        if current == .disabled {
            // オフ中はラベルがどこにも出ないため、変更を持ちかけても誤解を招くだけ
            rows.append(.info(t(language, "Change Label…", "ラベルを変更…")))
        } else {
            let label = settings.buttonLabel(slot)
            rows.append(
                .item(
                    label: t(
                        language,
                        "Change Label… (\"\(label)\")",
                        "ラベルを変更…（\"\(label)\"）"
                    ),
                    action: .editButtonLabel(slot)
                )
            )
        }

        let typeName = current.displayName(language)
        return .submenu(
            label: t(language, "Button \(number): \(typeName)", "ボタン\(number): \(typeName)"),
            rows: rows
        )
    }

    /// バッジ（「〜を再生中」）に何を表示するかを選ぶサブメニュー付き項目
    private static func badgeSubmenu(
        _ settings: SettingsStore, _ language: AppLanguage
    ) -> MenuRow {
        let current = settings.badgeLabel
        let name = current.displayName(language)
        return .submenu(
            label: t(language, "Status Badge: \(name)", "ステータスバッジ: \(name)"),
            rows: BadgeLabelType.allCases.map { candidate in
                .choice(
                    label: candidate.displayName(language),
                    action: .setBadgeLabel(candidate),
                    checked: candidate == current
                )
            }
        )
    }

    /// 一時停止時の挙動（表示継続 / 即消す / N分後に消す）を選ぶサブメニュー付き項目
    private static func pauseSubmenu(
        _ settings: SettingsStore, _ language: AppLanguage
    ) -> MenuRow {
        let current = settings.pauseHideMinutes
        let name = pauseChoiceName(current, language)
        return .submenu(
            label: t(language, "When Paused: \(name)", "一時停止時: \(name)"),
            rows: SettingsStore.pauseHideChoices.map { minutes in
                .choice(
                    label: pauseChoiceName(minutes, language),
                    action: .setPauseHideMinutes(minutes),
                    checked: minutes == current
                )
            }
        )
    }

    /// 番兵値の -1 と 0 を数字ではなく振る舞いとして読ませる。
    static func pauseChoiceName(_ minutes: Int, _ language: AppLanguage) -> String {
        switch minutes {
        case -1: return t(language, "Keep Showing", "表示し続ける")
        case 0: return t(language, "Hide Immediately", "すぐに消す")
        case 1: return t(language, "Hide After 1 Minute", "1分後に消す")
        default: return t(language, "Hide After \(minutes) Minutes", "\(minutes)分後に消す")
        }
    }

    /// 設定画面（メニュー）の表示言語を選ぶサブメニュー付き項目。Discordの文言には影響しない。
    private static func languageSubmenu(_ language: AppLanguage) -> MenuRow {
        .submenu(
            label: t(language, "Language: \(language.displayName)", "言語: \(language.displayName)"),
            rows: AppLanguage.allCases.map { candidate in
                .choice(
                    label: candidate.displayName,
                    action: .setLanguage(candidate),
                    checked: candidate == language
                )
            }
        )
    }
}
