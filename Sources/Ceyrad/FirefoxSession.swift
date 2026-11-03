import Compression
import Foundation

/// Firefox（と Zen など同じ系統のブラウザ）が書き出すセッションの復元用ファイル（`recovery.jsonlz4`）からタブを読む。
///
/// Firefox は AppleScript でタブを読めないので、これが唯一の手がかりになる。ファイルは
/// 約15秒おきに書かれるため、曲が替わってもしばらくは前の曲のタブ（URL・タイトル）を指す。
/// プライベートウィンドウのタブは書かれない。
enum FirefoxSession {
    enum Failure: Error, Equatable {
        /// 「他のアプリのデータへのアクセス」が許可されていない。macOS は Firefox のプロファイルを
        /// 他のアプリから守っていて、許可は システム設定 > ファイルとフォルダ で出す。
        case notAuthorized
        case noSessionFile
        case unreadable
        case badMagic
        case badSize
        case decompressionFailed
        case unexpectedJSON
    }

    struct SessionFile: Equatable {
        var url: URL
        var modified: Date
    }

    /// 起動中の Firefox が使っているプロファイルのファイル。どのプロファイルが起動中かは
    /// profiles.ini からは決まらない（起動時に選べる）ので、いちばん新しく書かれたものとみなす。
    static func recoveryFile(profilesPath: String) throws(Failure) -> SessionFile {
        let profilesDirectory = FileManager.default
            .homeDirectoryForCurrentUser
            .appendingPathComponent("Library/Application Support/\(profilesPath)", isDirectory: true)
        let profiles: [URL]
        do {
            profiles = try FileManager.default.contentsOfDirectory(
                at: profilesDirectory, includingPropertiesForKeys: nil)
        } catch CocoaError.fileReadNoPermission {
            throw .notAuthorized
        } catch {
            throw .noSessionFile
        }
        let newest =
            profiles
            .map { $0.appendingPathComponent("sessionstore-backups/recovery.jsonlz4") }
            .compactMap { url -> SessionFile? in
                guard
                    let modified = try? url.resourceValues(forKeys: [.contentModificationDateKey])
                        .contentModificationDate
                else { return nil }
                return SessionFile(url: url, modified: modified)
            }
            .max { $0.modified < $1.modified }
        guard let newest else { throw .noSessionFile }
        return newest
    }

    private static let magic = Data("mozLz40\0".utf8)
    /// 展開後の大きさの上限。ヘッダの値をそのまま確保に使うので、壊れたファイルで巨大な確保をしない。
    private static let maxDecodedSize = 256 * 1024 * 1024

    /// mozlz4: 8バイトのマジック、展開後の大きさ(UInt32 LE)、LZ4 のブロック（フレームなし）。
    static func decodeMozLz4(_ data: Data) throws(Failure) -> Data {
        let headerSize = magic.count + 4
        guard data.count > headerSize, data.prefix(magic.count) == magic else { throw .badMagic }
        let size = data.dropFirst(magic.count).prefix(4).reversed()
            .reduce(0) { $0 << 8 | Int($1) }
        guard size > 0, size <= maxDecodedSize else { throw .badSize }

        let source = data.dropFirst(headerSize)
        var output = Data(count: size)
        let written = output.withUnsafeMutableBytes { out in
            source.withUnsafeBytes { input in
                compression_decode_buffer(
                    out.bindMemory(to: UInt8.self).baseAddress!, size,
                    input.bindMemory(to: UInt8.self).baseAddress!, source.count,
                    nil, COMPRESSION_LZ4_RAW)
            }
        }
        guard written == size else { throw .decompressionFailed }
        return output
    }

    /// 開いているウィンドウの各タブの、いま表示しているページ。閉じたウィンドウ・タブは含めない。
    static func tabs(fromSessionJSON data: Data) throws(Failure) -> [BrowserTab] {
        guard let root = try? JSONSerialization.jsonObject(with: data) as? [String: Any],
            let windows = root["windows"] as? [[String: Any]]
        else { throw .unexpectedJSON }
        return windows.flatMap { window in
            (window["tabs"] as? [[String: Any]] ?? []).compactMap { tab -> BrowserTab? in
                // `index` は `entries`（そのタブの戻る/進む履歴）のうち表示中のものを指す、1始まりの番号
                guard let entries = tab["entries"] as? [[String: Any]], let index = tab["index"] as? Int,
                    entries.indices.contains(index - 1),
                    let url = entries[index - 1]["url"] as? String
                else { return nil }
                return BrowserTab(url: url, title: entries[index - 1]["title"] as? String ?? "")
            }
        }
    }

    static func readTabs(at url: URL) throws(Failure) -> [BrowserTab] {
        let data: Data
        do {
            data = try Data(contentsOf: url)
        } catch CocoaError.fileReadNoPermission {
            throw .notAuthorized
        } catch {
            throw .unreadable
        }
        return try tabs(fromSessionJSON: decodeMozLz4(data))
    }
}
