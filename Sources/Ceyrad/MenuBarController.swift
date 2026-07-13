import AppKit
import ServiceManagement

/// メニューバーUI。メニューは開かれるたびに組み立てるため、常駐分のメモリは最小限。
final class MenuBarController: NSObject, NSMenuDelegate {
    private let statusItem: NSStatusItem
    private let settings = SettingsStore.shared

    var statusLines: (() -> [String])?
    var onSettingsChanged: (() -> Void)?
    var onReconnectRequested: (() -> Void)?
    var onCheckForUpdates: (() -> Void)?

    override init() {
        statusItem = NSStatusBar.system.statusItem(withLength: NSStatusItem.squareLength)
        super.init()
        statusItem.button?.image = StatusIcon.make()
        let menu = NSMenu()
        menu.delegate = self
        statusItem.menu = menu
    }

    func menuNeedsUpdate(_ menu: NSMenu) {
        menu.removeAllItems()

        for line in statusLines?() ?? [] {
            menu.addItem(infoItem(line))
        }
        menu.addItem(.separator())

        // 各ボタンの設定（リンク先＋ラベル変更）はサブメニューにまとめる
        menu.addItem(
            buttonConfigItem(
                index: 1, type: settings.button1Type,
                label: settings.button1Label
            )
        )
        menu.addItem(
            buttonConfigItem(
                index: 2, type: settings.button2Type,
                label: settings.button2Label
            )
        )
        menu.addItem(.separator())

        menu.addItem(actionItem(t("Set Custom URL…", "カスタムURLを設定…"), #selector(editCustomURL)))
        menu.addItem(actionItem(t("Set Repository URL…", "リポジトリURLを設定…"), #selector(editRepositoryURL)))
        menu.addItem(badgeLabelItem())
        menu.addItem(pauseBehaviorItem())
        menu.addItem(languageItem())
        menu.addItem(launchAtLoginItem())
        menu.addItem(.separator())

        menu.addItem(actionItem("Reconnect to Discord", #selector(reconnect)))
        menu.addItem(.separator())
        menu.addItem(actionItem(t("Check for Updates…", "アップデートを確認…"), #selector(checkForUpdates)))
        menu.addItem(.separator())
        let quitItem = actionItem(t("Quit Ceyrad", "Ceyradを終了"), #selector(quit))
        quitItem.keyEquivalent = "q"
        menu.addItem(quitItem)
    }

    // MARK: - Item builders

    private func infoItem(_ title: String) -> NSMenuItem {
        let item = NSMenuItem(title: title, action: nil, keyEquivalent: "")
        item.isEnabled = false
        return item
    }

    private func actionItem(_ title: String, _ action: Selector) -> NSMenuItem {
        let item = NSMenuItem(title: title, action: action, keyEquivalent: "")
        item.target = self
        return item
    }

    /// 「Button 1: Song Page」のようなサブメニュー付き項目。
    /// サブメニューにはリンク先の選択肢と「ラベル変更」をまとめて入れる。
    private func buttonConfigItem(index: Int, type: LinkType, label: String) -> NSMenuItem {
        let item = NSMenuItem(
            title: t("Button \(index): \(type.displayName)", "ボタン\(index): \(type.displayName)"),
            action: nil, keyEquivalent: ""
        )
        let submenu = NSMenu()

        submenu.addItem(infoItem(t("Link Destination", "リンク先")))
        for candidate in LinkType.selectableCases {
            let sub = NSMenuItem(
                title: candidate.displayName,
                action: #selector(selectLinkType(_:)), keyEquivalent: ""
            )
            sub.target = self
            sub.representedObject = "\(index):\(candidate.rawValue)"
            sub.state = (candidate == type) ? .on : .off
            sub.indentationLevel = 1
            submenu.addItem(sub)
        }

        submenu.addItem(.separator())
        let labelItem = actionItem(
            t("Change Label… (\"\(label)\")", "ラベルを変更…（\"\(label)\"）"),
            index == 1
                ? #selector(editButton1Label)
                : #selector(editButton2Label)
        )
        submenu.addItem(labelItem)

        item.submenu = submenu
        return item
    }

    /// バッジ（「〜を再生中」）に何を表示するかを選ぶサブメニュー付き項目
    private func badgeLabelItem() -> NSMenuItem {
        let item = NSMenuItem(
            title: t(
                "Status Badge: \(settings.badgeLabel.displayName)",
                "ステータスバッジ: \(settings.badgeLabel.displayName)"
            ),
            action: nil, keyEquivalent: ""
        )
        let submenu = NSMenu()
        for candidate in BadgeLabelType.allCases {
            let sub = NSMenuItem(
                title: candidate.displayName,
                action: #selector(selectBadgeLabel(_:)), keyEquivalent: ""
            )
            sub.target = self
            sub.representedObject = candidate.rawValue
            sub.state = (candidate == settings.badgeLabel) ? .on : .off
            submenu.addItem(sub)
        }
        item.submenu = submenu
        return item
    }

    /// 一時停止時の挙動（表示継続 / 即消す / N分後に消す）を選ぶサブメニュー付き項目
    private func pauseBehaviorItem() -> NSMenuItem {
        let item = NSMenuItem(
            title: t(
                "When Paused: \(pauseChoiceName(settings.pauseHideMinutes))",
                "一時停止時: \(pauseChoiceName(settings.pauseHideMinutes))"
            ),
            action: nil, keyEquivalent: ""
        )
        let submenu = NSMenu()
        for minutes in SettingsStore.pauseHideChoices {
            let sub = NSMenuItem(
                title: pauseChoiceName(minutes),
                action: #selector(selectPauseHide(_:)), keyEquivalent: ""
            )
            sub.target = self
            sub.representedObject = minutes
            sub.state = (minutes == settings.pauseHideMinutes) ? .on : .off
            submenu.addItem(sub)
        }
        item.submenu = submenu
        return item
    }

    private func pauseChoiceName(_ minutes: Int) -> String {
        switch minutes {
        case -1: return t("Keep Showing", "表示し続ける")
        case 0: return t("Hide Immediately", "すぐに消す")
        case 1: return t("Hide After 1 Minute", "1分後に消す")
        default: return t("Hide After \(minutes) Minutes", "\(minutes)分後に消す")
        }
    }

    /// 設定画面（メニュー）の表示言語を選ぶサブメニュー付き項目。Discordの文言には影響しない。
    private func languageItem() -> NSMenuItem {
        let item = NSMenuItem(
            title: t("Language: \(settings.language.displayName)", "言語: \(settings.language.displayName)"),
            action: nil, keyEquivalent: ""
        )
        let submenu = NSMenu()
        for lang in AppLanguage.allCases {
            let sub = NSMenuItem(
                title: lang.displayName,
                action: #selector(selectLanguage(_:)), keyEquivalent: ""
            )
            sub.target = self
            sub.representedObject = lang.rawValue
            sub.state = (lang == settings.language) ? .on : .off
            submenu.addItem(sub)
        }
        item.submenu = submenu
        return item
    }

    // MARK: - Actions

    @objc private func selectLinkType(_ sender: NSMenuItem) {
        guard let encoded = sender.representedObject as? String else { return }
        let parts = encoded.split(separator: ":", maxSplits: 1)
        guard parts.count == 2, let type = LinkType(rawValue: String(parts[1])) else { return }
        if parts[0] == "1" {
            settings.button1Type = type
        } else {
            settings.button2Type = type
        }
        onSettingsChanged?()
    }

    @objc private func editButton1Label() {
        guard
            let value = prompt(
                title: t("Button 1 Label", "ボタン1のラベル"),
                message: t(
                    "Text shown on the Discord button (up to 32 characters)",
                    "Discordのボタンに表示されるテキスト（32文字まで）"
                ),
                current: settings.button1Label
            )
        else { return }
        settings.button1Label = value
        onSettingsChanged?()
    }

    @objc private func editButton2Label() {
        guard
            let value = prompt(
                title: t("Button 2 Label", "ボタン2のラベル"),
                message: t(
                    "Text shown on the Discord button (up to 32 characters)",
                    "Discordのボタンに表示されるテキスト（32文字まで）"
                ),
                current: settings.button2Label
            )
        else { return }
        settings.button2Label = value
        onSettingsChanged?()
    }

    @objc private func editCustomURL() {
        guard
            let value = prompt(
                title: t("Custom URL", "カスタムURL"),
                message: t(
                    "URL used when a button's link destination is \"Custom URL\" (http/https)",
                    "ボタンのリンク先が「カスタムURL」のときに使うURL（http/https）"
                ),
                current: settings.customURL,
                placeholder: "https://example.com"
            )
        else { return }
        if !value.isEmpty, !ActivityBuilder.isValidButtonURL(value) {
            showError(
                t(
                    "Enter a URL of 512 characters or fewer, starting with http:// or https://.",
                    "http:// または https:// で始まる512文字以内のURLを入力してください。"
                )
            )
            return
        }
        settings.customURL = value
        onSettingsChanged?()
    }

    @objc private func editRepositoryURL() {
        guard
            let value = prompt(
                title: t("Repository URL", "リポジトリURL"),
                message: t(
                    "Used for the Repository button and as a fallback when a link cannot be resolved",
                    "リポジトリボタンや、リンクが解決できないときのフォールバックに使用"
                ),
                current: settings.repositoryURL
            )
        else { return }
        if !ActivityBuilder.isValidButtonURL(value) {
            showError(
                t(
                    "Enter a URL of 512 characters or fewer, starting with http:// or https://.",
                    "http:// または https:// で始まる512文字以内のURLを入力してください。"
                )
            )
            return
        }
        settings.repositoryURL = value
        onSettingsChanged?()
    }

    @objc private func selectBadgeLabel(_ sender: NSMenuItem) {
        guard let raw = sender.representedObject as? Int,
            let type = BadgeLabelType(rawValue: raw)
        else { return }
        settings.badgeLabel = type
        onSettingsChanged?()
    }

    @objc private func selectPauseHide(_ sender: NSMenuItem) {
        guard let minutes = sender.representedObject as? Int else { return }
        settings.pauseHideMinutes = minutes
        onSettingsChanged?()
    }

    @objc private func selectLanguage(_ sender: NSMenuItem) {
        guard let raw = sender.representedObject as? String, let lang = AppLanguage(rawValue: raw) else { return }
        settings.language = lang
    }

    /// ログイン時の自動起動トグル。SMAppServiceは.appバンドルとして起動している場合のみ機能する。
    private func launchAtLoginItem() -> NSMenuItem {
        let item = actionItem(t("Launch at Login", "ログイン時に自動起動"), #selector(toggleLaunchAtLogin))
        item.state = SMAppService.mainApp.status == .enabled ? .on : .off
        return item
    }

    @objc private func toggleLaunchAtLogin() {
        do {
            if SMAppService.mainApp.status == .enabled {
                try SMAppService.mainApp.unregister()
            } else {
                try SMAppService.mainApp.register()
            }
        } catch {
            showError(
                t(
                    "Could not change the login item. If running the bare executable "
                        + "(not Ceyrad.app), use a LaunchAgent instead.\n\(error.localizedDescription)",
                    "ログイン項目を変更できませんでした。.appバンドルではなく素の実行ファイルで"
                        + "起動している場合はLaunchAgentを使ってください。\n\(error.localizedDescription)"
                )
            )
        }
    }

    @objc private func reconnect() {
        onReconnectRequested?()
    }

    @objc private func checkForUpdates() {
        onCheckForUpdates?()
    }

    @objc private func quit() {
        NSApp.terminate(nil)
    }

    // MARK: - Dialogs

    private func prompt(
        title: String, message: String,
        current: String, placeholder: String = ""
    ) -> String? {
        let alert = NSAlert()
        alert.messageText = title
        alert.informativeText = message
        let field = NSTextField(frame: NSRect(x: 0, y: 0, width: 340, height: 24))
        field.stringValue = current
        field.placeholderString = placeholder
        alert.accessoryView = field
        alert.addButton(withTitle: t("OK", "OK"))
        alert.addButton(withTitle: t("Cancel", "キャンセル"))
        alert.window.initialFirstResponder = field
        NSApp.activate(ignoringOtherApps: true)
        guard alert.runModal() == .alertFirstButtonReturn else { return nil }
        return field.stringValue.trimmingCharacters(in: .whitespacesAndNewlines)
    }

    private func showError(_ message: String) {
        let alert = NSAlert()
        alert.alertStyle = .warning
        alert.messageText = t("Invalid Input", "入力が無効です")
        alert.informativeText = message
        NSApp.activate(ignoringOtherApps: true)
        alert.runModal()
    }
}
