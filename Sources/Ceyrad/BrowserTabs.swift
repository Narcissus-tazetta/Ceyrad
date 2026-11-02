import Foundation
import os

struct BrowserTab: Equatable {
    var url: String
    var title: String
}

/// 再生元ブラウザのタブ（URL とタイトル）を AppleScript で読む。
///
/// 呼ぶのは「新しい曲が来たとき」だけで、定期実行はしない。実行は専用のシリアルキューで行う
/// （`MusicAppleScript` と同じ理由: NSAppleScript はスレッドセーフでなく、権限ダイアログ待ちで
/// UI を固めないため）。
enum BrowserTabs {
    enum Failure: Error, Equatable {
        /// オートメーション権限がない(-1743)。
        case notAuthorized
        case scriptFailed(Int)
        /// URL の一覧とタイトルの一覧の数が合わない。タブの対応が取れないので使わない。
        case mismatchedTabs
    }

    /// 直近の呼び出しが権限拒否で失敗したか。メニューの警告に使う。メインスレッドからのみ触る。
    private(set) static var notAuthorized = false

    private static let log = Logger(
        subsystem: Bundle.main.bundleIdentifier ?? "Ceyrad", category: "BrowserTabs"
    )
    private static let queue = DispatchQueue(label: "browser-tabs-applescript", qos: .userInitiated)
    /// キューの内側からのみ触る。
    private static var scripts: [String: NSAppleScript] = [:]

    /// `bundleId` のブラウザが開いているタブ。completion はメインスレッド。
    ///
    /// `bundleId` は AppleScript のソースに埋め込むので、許可済みのブラウザ以外は受け付けない。
    static func tabs(
        bundleId: String, completion: @escaping (Result<[BrowserTab], Failure>) -> Void
    ) {
        queue.async {
            let result = run(bundleId: bundleId)
            DispatchQueue.main.async {
                if case .failure(let failure) = result {
                    notAuthorized = failure == .notAuthorized
                } else {
                    notAuthorized = false
                }
                completion(result)
            }
        }
    }

    private static func run(bundleId: String) -> Result<[BrowserTab], Failure> {
        precondition(YouTubeMusic.isBrowser(bundleId: bundleId), "未許可のブラウザ: \(bundleId)")
        let script =
            scripts[bundleId]
            ?? NSAppleScript(
                source: "tell application id \"\(bundleId)\" to get {URL, title} of every tab of every window"
            )
        guard let script else { return .failure(.scriptFailed(0)) }
        scripts[bundleId] = script

        var error: NSDictionary?
        let result = script.executeAndReturnError(&error)
        if let error {
            let number = error[NSAppleScript.errorNumber] as? Int ?? 0
            log.error("AppleScript error: \(String(describing: error), privacy: .public)")
            // -1743 = errAEEventNotPermitted（オートメーション権限なし）
            return .failure(number == -1743 ? .notAuthorized : .scriptFailed(number))
        }
        // {URL, title} of every tab of every window は {URLの入れ子リスト, タイトルの入れ子リスト}。
        guard result.descriptorType == typeAEList, result.numberOfItems == 2,
            let urlList = result.atIndex(1), let titleList = result.atIndex(2)
        else { return .failure(.mismatchedTabs) }
        var urls: [String] = []
        var titles: [String] = []
        collectStrings(urlList, into: &urls)
        collectStrings(titleList, into: &titles)
        guard urls.count == titles.count else { return .failure(.mismatchedTabs) }
        return .success(zip(urls, titles).map { BrowserTab(url: $0, title: $1) })
    }

    /// 「ウィンドウごとのリスト」のリストを、順序を保ったまま1本に潰す。
    private static func collectStrings(_ descriptor: NSAppleEventDescriptor, into urls: inout [String]) {
        if descriptor.descriptorType == typeAEList {
            guard descriptor.numberOfItems > 0 else { return }
            for index in 1...descriptor.numberOfItems {
                if let item = descriptor.atIndex(index) { collectStrings(item, into: &urls) }
            }
        } else if let string = descriptor.stringValue {
            urls.append(string)
        }
    }
}
