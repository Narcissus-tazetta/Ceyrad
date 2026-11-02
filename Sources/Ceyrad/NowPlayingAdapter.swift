import Foundation
import os

/// MediaRemote（システム全体の「いま再生中」）を `/usr/bin/perl` 経由で読む。
///
/// macOS 15.4 以降、MediaRemote は entitlement を持たないアプリには何も返さない。システムの perl
/// だけがその entitlement を持つので、同梱の [mediaremote-adapter](https://github.com/ungive/mediaremote-adapter)
/// を perl に読み込ませ、その標準出力を受け取る。フレームワークは perl が読むためのもので、リンクはしない。
///
/// 子プロセスは常駐し、出力は読み取り可能になったときにだけ処理する（ポーリングなし）。
/// 検知を有効にしている間だけ起動するので、無効のあいだの常駐コストはない。
final class NowPlayingAdapter {
    /// 本体（メインスレッド）。`nil` は何も再生していないこと。
    var onPayload: ((NowPlayingPayload?) -> Void)?
    /// 起動できなかった、または途中で落ちた。メインスレッドで1回だけ呼ぶ。
    var onFailure: ((String) -> Void)?

    private struct Paths {
        var script: URL
        var framework: URL
        var testClient: URL
    }

    private static let perl = URL(fileURLWithPath: "/usr/bin/perl")
    private static let log = Logger(
        subsystem: Bundle.main.bundleIdentifier ?? "Ceyrad", category: "NowPlayingAdapter"
    )

    private var process: Process?
    /// start/stop のたびに進める。古い世代の非同期の結果が、新しい状態を上書きしないための番号。
    private var generation = 0

    func start() {
        guard process == nil else { return }
        generation += 1
        let current = generation
        guard let paths = Self.locate() else {
            fail("the adapter is not bundled with this build")
            return
        }

        // テストと起動の間に stop() されたら捨てる。待つ間メインスレッドは塞がない。
        DispatchQueue.global(qos: .utility).async { [weak self] in
            let status = Self.runTest(paths)
            DispatchQueue.main.async {
                guard let self, self.generation == current else { return }
                if status == 0 {
                    self.launchStream(paths)
                } else {
                    self.fail("the system denied access to MediaRemote (test exit \(status))")
                }
            }
        }
    }

    func stop() {
        generation += 1
        guard let process else { return }
        process.terminationHandler = nil
        (process.standardOutput as? Pipe)?.fileHandleForReading.readabilityHandler = nil
        process.terminate()
        self.process = nil
    }

    // MARK: - Private

    /// 同梱の場所。`.app` でない素の実行ファイルでは見つからない。
    private static func locate() -> Paths? {
        let bundle = Bundle.main
        guard
            let framework = bundle.privateFrameworksURL?
                .appendingPathComponent("MediaRemoteAdapter.framework"),
            FileManager.default.fileExists(atPath: framework.path),
            let script = bundle.url(forResource: "mediaremote-adapter", withExtension: "pl"),
            let testClient = bundle.url(forResource: "MediaRemoteAdapterTestClient", withExtension: nil)
        else { return nil }
        return Paths(script: script, framework: framework, testClient: testClient)
    }

    /// アダプタが使えるかの確認（0 なら使える）。非公開 API なので OS の更新で壊れうる。
    private static func runTest(_ paths: Paths) -> Int32 {
        let test = Process()
        test.executableURL = perl
        test.arguments = [paths.script.path, paths.framework.path, paths.testClient.path, "test"]
        test.standardOutput = FileHandle.nullDevice
        test.standardError = FileHandle.nullDevice
        do {
            try test.run()
        } catch {
            log.error("could not run perl: \(error.localizedDescription, privacy: .public)")
            return -1
        }
        test.waitUntilExit()
        return test.terminationStatus
    }

    /// 前回のアプリが `stop()` を通らずに落ちると、子の perl だけが残る（出力先が閉じても、
    /// 次に何か書くまで終わらない）。同じフレームワークを指す `stream` を先に片付ける。
    private static func killStaleStreams(_ paths: Paths) {
        let pkill = Process()
        pkill.executableURL = URL(fileURLWithPath: "/usr/bin/pkill")
        // pkill -f は正規表現。パスの `.` や `(` を文字として扱わせる。
        pkill.arguments = [
            "-f",
            NSRegularExpression.escapedPattern(
                for: "\(paths.script.path) \(paths.framework.path) stream"),
        ]
        do {
            try pkill.run()
            pkill.waitUntilExit()  // 1 = 該当なし。それが普通なので終了コードは見ない
        } catch {
            log.error("could not run pkill: \(error.localizedDescription, privacy: .public)")
        }
    }

    private func launchStream(_ paths: Paths) {
        Self.killStaleStreams(paths)
        let stream = Process()
        stream.executableURL = Self.perl
        // artwork は画像のバイト列を毎回 base64 で運ぶ（数百KB）うえに、判定には使わない。
        // debounce は曲の切り替え時に連続して届く更新をまとめる。
        stream.arguments = [
            paths.script.path, paths.framework.path, "stream",
            "--no-artwork", "--no-diff", "--debounce=300",
        ]
        let output = Pipe()
        stream.standardOutput = output
        stream.standardError = FileHandle.nullDevice

        var buffer = Data()
        output.fileHandleForReading.readabilityHandler = { [weak self] handle in
            let chunk = handle.availableData
            guard !chunk.isEmpty else {
                handle.readabilityHandler = nil
                return
            }
            buffer.append(chunk)
            while let newline = buffer.firstIndex(of: 0x0A) {
                let bytes = buffer[buffer.startIndex..<newline]
                buffer.removeSubrange(buffer.startIndex...newline)
                guard let line = String(bytes: bytes, encoding: .utf8) else {
                    Self.log.error("adapter output is not valid UTF-8")
                    continue
                }
                self?.deliver(line: line)
            }
        }

        let current = generation
        stream.terminationHandler = { [weak self] finished in
            DispatchQueue.main.async {
                guard let self, self.generation == current else { return }
                self.process = nil
                self.fail("the adapter exited unexpectedly (status \(finished.terminationStatus))")
            }
        }

        do {
            try stream.run()
            process = stream
        } catch {
            output.fileHandleForReading.readabilityHandler = nil
            fail("could not start perl: \(error.localizedDescription)")
        }
    }

    /// 読み取りハンドラのキューから呼ばれる。JSON が壊れていたら黙って捨てずにログへ残す。
    private func deliver(line: String) {
        guard !line.isEmpty else { return }
        do {
            let payload = try NowPlayingPayload.parse(line: line)
            DispatchQueue.main.async { [weak self] in self?.onPayload?(payload) }
        } catch {
            Self.log.error("unparseable adapter output: \(String(describing: error), privacy: .public)")
        }
    }

    private func fail(_ reason: String) {
        Self.log.error("\(reason, privacy: .public)")
        onFailure?(reason)
    }
}
