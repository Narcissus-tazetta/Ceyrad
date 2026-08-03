import AppKit

/// メニューバーUI。
///
/// 何を出すかの判断は`MenuModel`にあり、この型が受け持つのは行データをNSMenuに変換することだけ。
/// メニューは開かれるたびに組み立て直して閉じたら捨てるため、常駐分のメモリは最小限になる。
final class MenuBarController: NSObject, NSMenuDelegate {
    private let statusItem: NSStatusItem

    /// メニューを開いた時点の状態。開くたびに呼ばれる。
    var menuInput: (() -> MenuModel.Input)?
    var onAction: ((MenuAction) -> Void)?

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
        guard let input = menuInput?() else { return }
        render(MenuModel.build(input), into: menu, indentChoices: false)
    }

    /// 行データをNSMenuに写す。
    ///
    /// `indentChoices` は「見出し（`.info`）で始まるサブメニューの選択肢は
    /// その見出しにぶら下がっている」という関係を字下げで見せるためのもの。
    /// 見出しのないサブメニュー（バッジ・一時停止・言語）は字下げしない。
    private func render(_ rows: [MenuRow], into menu: NSMenu, indentChoices: Bool) {
        for row in rows {
            switch row {
            case .separator:
                menu.addItem(.separator())

            case .info(let label):
                // actionなし＝AppKitが自動でグレーアウトする
                let item = NSMenuItem(title: label, action: nil, keyEquivalent: "")
                item.isEnabled = false
                menu.addItem(item)

            case .item(let label, let action):
                let item = ActionMenuItem(title: label, menuAction: action, target: self)
                if action == .quit {
                    item.keyEquivalent = "q"
                }
                menu.addItem(item)

            case .choice(let label, let action, let checked):
                let item = ActionMenuItem(title: label, menuAction: action, target: self)
                item.state = checked ? .on : .off
                if indentChoices {
                    item.indentationLevel = 1
                }
                menu.addItem(item)

            case .submenu(let label, let rows):
                let item = NSMenuItem(title: label, action: nil, keyEquivalent: "")
                let submenu = NSMenu()
                render(rows, into: submenu, indentChoices: rows.first?.isInfo ?? false)
                item.submenu = submenu
                menu.addItem(item)
            }
        }
    }

    @objc fileprivate func performMenuAction(_ sender: NSMenuItem) {
        guard let item = sender as? ActionMenuItem else { return }
        onAction?(item.menuAction)
    }
}

/// クリックされた行が何を意味するかを、行そのものが持つ。
///
/// `representedObject`に`"1:song"`のような文字列を詰めて解釈し直すより、
/// 型のまま持たせるほうが壊れようがない。
private final class ActionMenuItem: NSMenuItem {
    let menuAction: MenuAction

    init(title: String, menuAction: MenuAction, target: MenuBarController) {
        self.menuAction = menuAction
        super.init(
            title: title,
            action: #selector(MenuBarController.performMenuAction(_:)),
            keyEquivalent: ""
        )
        self.target = target
    }

    @available(*, unavailable)
    required init(coder: NSCoder) {
        fatalError("メニューはコードから組み立てるためnib復元は使わない")
    }
}

extension MenuRow {
    fileprivate var isInfo: Bool {
        if case .info = self { return true }
        return false
    }
}
