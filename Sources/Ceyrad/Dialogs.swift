import AppKit

/// メニューから開くモーダル。
///
/// 呼び出し側（AppDelegateのメニュー処理）はこれを2つのクロージャとして`URLPrompt`等に
/// 渡すため、ロジック側はAppKitを知らずに済む。言語も引数で受け取り、グローバルを読まない。
enum Dialogs {
    static func prompt(
        language: AppLanguage,
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
        alert.addButton(withTitle: "OK")
        alert.addButton(withTitle: t(language, "Cancel", "キャンセル"))
        alert.window.initialFirstResponder = field
        NSApp.activate(ignoringOtherApps: true)
        guard alert.runModal() == .alertFirstButtonReturn else { return nil }
        return field.stringValue.trimmingCharacters(in: .whitespacesAndNewlines)
    }

    static func showError(title: String, message: String) {
        let alert = NSAlert()
        alert.alertStyle = .warning
        alert.messageText = title
        alert.informativeText = message
        NSApp.activate(ignoringOtherApps: true)
        alert.runModal()
    }
}
