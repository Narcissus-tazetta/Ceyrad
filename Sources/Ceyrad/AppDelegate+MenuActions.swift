import AppKit

// MARK: - メニュー操作の適用
//
// `MenuAction`は「何を要求されたか」しか表さない。ダイアログを出して値を受け取り、
// 対応するsetterを呼ぶのはここ。メニューの組み立て（MenuModel）とは完全に分かれている。

extension AppDelegate {
    func apply(_ action: MenuAction) {
        switch action {
        case .setButtonType(let slot, let linkType):
            settings.setButtonType(slot, linkType)
            settingsChanged()

        case .setBadgeLabel(let badge):
            settings.badgeLabel = badge
            settingsChanged()

        case .setPauseHideMinutes(let minutes):
            settings.pauseHideMinutes = minutes
            settingsChanged()

        case .setLanguage(let language):
            // 変わるのはメニュー自身の文言だけなので、Discordに送り直すものはない
            settings.language = language

        case .reconnect:
            reconnectNow()

        case .checkForUpdates:
            checkForUpdates()

        case .quit:
            NSApp.terminate(nil)

        // モーダルを開くもの。ユーザーの入力を待つあいだメニューは既に閉じている。
        case .editButtonLabel, .editCustomURL, .editRepositoryURL, .toggleLaunchAtLogin:
            applyModal(action)
        }
    }

    private func applyModal(_ action: MenuAction) {
        switch action {
        case .editButtonLabel(let slot): editButtonLabel(slot)
        case .editCustomURL: editCustomURL()
        case .editRepositoryURL: editRepositoryURL()
        case .toggleLaunchAtLogin: toggleLaunchAtLogin()
        default: break
        }
    }

    private func editButtonLabel(_ slot: ButtonSlot) {
        let language = settings.language
        let number = slot.number
        guard
            let value = Dialogs.prompt(
                language: language,
                title: t(language, "Button \(number) Label", "ボタン\(number)のラベル"),
                message: t(
                    language,
                    "Text shown on the Discord button (up to 32 characters)",
                    "Discordのボタンに表示されるテキスト（32文字まで）"
                ),
                current: settings.buttonLabel(slot)
            )
        else { return }
        // setterが長さを切り、リンク先の既定値と同じなら「未カスタマイズ」に戻す
        settings.setButtonLabel(slot, value)
        settingsChanged()
    }

    private func editCustomURL() {
        let language = settings.language
        // 空文字は意味を持つ: カスタムリンクをオフにする
        guard
            let url = promptForURL(
                title: t(language, "Custom URL", "カスタムURL"),
                message: t(
                    language,
                    "URL used when a button's link destination is \"Custom URL\" (http/https)",
                    "ボタンのリンク先が「カスタムURL」のときに使うURL（http/https）"
                ),
                initial: settings.customURL,
                placeholder: "https://example.com",
                allowEmpty: true
            )
        else { return }
        settings.customURL = url
        settingsChanged()
    }

    private func editRepositoryURL() {
        let language = settings.language
        guard
            let url = promptForURL(
                title: t(language, "Repository URL", "リポジトリURL"),
                message: t(
                    language,
                    "URL used when a button's link destination is \"Repository\"",
                    "ボタンのリンク先が「リポジトリ」のときに使うURL"
                ),
                initial: settings.repositoryURL,
                allowEmpty: false
            )
        else { return }
        settings.repositoryURL = url
        settingsChanged()
    }

    /// URLになるまで訊き直す。ループ自体は`URLPrompt`にあり、ここはAppKit側の2つの穴を埋めるだけ。
    private func promptForURL(
        title: String, message: String, initial: String,
        placeholder: String = "", allowEmpty: Bool
    ) -> String? {
        let language = settings.language
        return URLPrompt.promptValidURL(
            showPrompt: { current in
                Dialogs.prompt(
                    language: language, title: title, message: message,
                    current: current, placeholder: placeholder
                )
            },
            showError: {
                Dialogs.showError(
                    title: t(language, "Invalid Input", "入力が無効です"),
                    message: t(
                        language,
                        "Enter a URL of 512 characters or fewer, starting with http:// or https://.",
                        "http:// または https:// で始まる512文字以内のURLを入力してください。"
                    )
                )
            },
            initial: initial,
            allowEmpty: allowEmpty
        )
    }

    /// 自動起動を反転する。失敗したらチェックは付かない——メニューはOSの言い分に従う。
    private func toggleLaunchAtLogin() {
        do {
            try LaunchAtLogin.setEnabled(!LaunchAtLogin.isEnabled)
        } catch {
            let language = settings.language
            Dialogs.showError(
                title: t(language, "Launch at Login", "ログイン時に自動起動"),
                message: t(
                    language,
                    "Could not change the login item. If running the bare executable "
                        + "(not Ceyrad.app), use a LaunchAgent instead.\n\(error.localizedDescription)",
                    "ログイン項目を変更できませんでした。.appバンドルではなく素の実行ファイルで"
                        + "起動している場合はLaunchAgentを使ってください。\n\(error.localizedDescription)"
                )
            )
        }
    }
}
