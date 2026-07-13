import AppKit

// MARK: - Dialogs

extension MenuBarController {
    func prompt(
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

    func showError(_ message: String) {
        let alert = NSAlert()
        alert.alertStyle = .warning
        alert.messageText = t("Invalid Input", "入力が無効です")
        alert.informativeText = message
        NSApp.activate(ignoringOtherApps: true)
        alert.runModal()
    }
}
