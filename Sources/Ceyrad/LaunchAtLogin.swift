import ServiceManagement

/// ログイン時の自動起動。SMAppServiceは.appバンドルとして起動している場合のみ機能する。
///
/// メニューは要求した値ではなくOSが実際に返す値を表示する。ユーザーはシステム設定側でも
/// 切れるし、書き込みが失敗することもあるので、そこで嘘をつかないようにする。
enum LaunchAtLogin {
    static var isEnabled: Bool {
        SMAppService.mainApp.status == .enabled
    }

    static func setEnabled(_ enabled: Bool) throws {
        if enabled {
            try SMAppService.mainApp.register()
        } else {
            try SMAppService.mainApp.unregister()
        }
    }
}
