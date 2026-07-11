import Darwin
import Foundation
import os

/// Discord IPC（Unixドメインソケット）クライアント。
/// ソケットは `$TMPDIR/discord-ipc-{0..9}` にある（Application Supportではない点に注意）。
/// 公開APIはメインスレッドから呼ぶ。ソケットI/Oは専用キューで行う。
final class DiscordRPCClient {
    enum ConnState {
        case disconnected
        case connecting
        case connected
    }

    private(set) var state: ConnState = .disconnected
    var onStateChange: ((ConnState) -> Void)?

    private let log = Logger(
        subsystem: Bundle.main.bundleIdentifier ?? "Cadence", category: "DiscordRPC"
    )
    private let queue = DispatchQueue(label: "discord-rpc")
    private var fd: Int32 = -1
    private var readSource: DispatchSourceRead?
    private var recvBuffer = Data()

    // MARK: - Public API (main thread)

    func connect(clientId: String) {
        guard state == .disconnected else { return }
        setState(.connecting)
        queue.async { [weak self] in
            self?.doConnect(clientId: clientId)
        }
    }

    func setActivity(_ activity: [String: Any]?) {
        queue.async { [weak self] in
            self?.sendActivityLocked(activity)
        }
    }

    /// Activityをクリアしてから切断する（Apple Music終了時）
    func shutdown(clearActivity: Bool) {
        queue.async { [weak self] in
            guard let self, self.fd >= 0 else { return }
            if clearActivity {
                self.sendActivityLocked(nil)
            }
            self.teardown()
        }
    }

    /// 本アプリ終了時用。asyncだとプロセス終了までにキューが走らないことがあるため、
    /// クリアの送信までを同期的に済ませてから戻る。
    func shutdownSync(clearActivity: Bool) {
        queue.sync {
            guard fd >= 0 else { return }
            if clearActivity {
                sendActivityLocked(nil)
            }
            teardown()
        }
    }

    // MARK: - Socket (rpc queue)

    private func doConnect(clientId: String) {
        guard fd < 0 else { return }
        let tmpDir = ProcessInfo.processInfo.environment["TMPDIR"] ?? NSTemporaryDirectory()
        var sock: Int32 = -1
        for i in 0..<10 {
            let path = (tmpDir as NSString).appendingPathComponent("discord-ipc-\(i)")
            sock = Self.openUnixSocket(path: path)
            if sock >= 0 { break }
        }
        guard sock >= 0 else {
            setState(.disconnected)
            return
        }
        fd = sock
        // 切断済みソケットへのwriteでプロセスが落ちないようにする
        var one: Int32 = 1
        setsockopt(fd, SOL_SOCKET, SO_NOSIGPIPE, &one, socklen_t(MemoryLayout<Int32>.size))

        let source = DispatchSource.makeReadSource(fileDescriptor: fd, queue: queue)
        source.setEventHandler { [weak self] in self?.readAvailable() }
        source.setCancelHandler { close(sock) }
        readSource = source
        source.resume()

        sendFrame(op: 0, payload: ["v": 1, "client_id": clientId])
    }

    private static func openUnixSocket(path: String) -> Int32 {
        let sock = socket(AF_UNIX, SOCK_STREAM, 0)
        guard sock >= 0 else { return -1 }
        var addr = sockaddr_un()
        addr.sun_family = sa_family_t(AF_UNIX)
        let maxLen = MemoryLayout.size(ofValue: addr.sun_path)
        let copied: Bool = path.withCString { cstr in
            guard strlen(cstr) < maxLen else { return false }
            _ = withUnsafeMutablePointer(to: &addr.sun_path) { ptr in
                ptr.withMemoryRebound(to: CChar.self, capacity: maxLen) { dst in
                    strlcpy(dst, cstr, maxLen)
                }
            }
            return true
        }
        guard copied else {
            close(sock)
            return -1
        }
        let result = withUnsafePointer(to: &addr) { ptr in
            ptr.withMemoryRebound(to: sockaddr.self, capacity: 1) {
                Darwin.connect(sock, $0, socklen_t(MemoryLayout<sockaddr_un>.size))
            }
        }
        guard result == 0 else {
            close(sock)
            return -1
        }
        return sock
    }

    private func readAvailable() {
        guard fd >= 0 else { return }
        var buf = [UInt8](repeating: 0, count: 65536)
        let n = read(fd, &buf, buf.count)
        guard n > 0 else {
            teardown()
            return
        }
        recvBuffer.append(contentsOf: buf[0..<n])
        while recvBuffer.count >= 8 {
            let op = leUInt32(at: 0)
            let length = Int(leUInt32(at: 4))
            guard length < 1_000_000 else {
                teardown()
                return
            }
            let total = 8 + length
            guard recvBuffer.count >= total else { break }
            let payload = recvBuffer.subdata(in: 8..<total)
            recvBuffer.removeSubrange(0..<total)
            handleFrame(op: op, payload: payload)
        }
    }

    private func handleFrame(op: UInt32, payload: Data) {
        let json = (try? JSONSerialization.jsonObject(with: payload)) as? [String: Any]
        switch op {
        case 1:  // FRAME
            guard let json else { return }
            switch json["evt"] as? String {
            case "READY":
                setState(.connected)
            case "ERROR":
                log.error(
                    "Discord RPC error: \(String(data: payload, encoding: .utf8) ?? "", privacy: .public)"
                )
            default:
                break  // SET_ACTIVITYの応答など
            }
        case 2:  // CLOSE（Client ID不正など）
            log.error(
                "Discord closed connection: \(String(data: payload, encoding: .utf8) ?? "", privacy: .public)"
            )
            teardown()
        case 3:  // PING
            sendFrame(op: 4, payload: json ?? [:])
        default:
            break
        }
    }

    private func sendActivityLocked(_ activity: [String: Any]?) {
        guard fd >= 0 else { return }
        var args: [String: Any] = ["pid": Int(ProcessInfo.processInfo.processIdentifier)]
        args["activity"] = activity ?? NSNull()
        sendFrame(
            op: 1,
            payload: [
                "cmd": "SET_ACTIVITY",
                "args": args,
                "nonce": UUID().uuidString,
            ]
        )
    }

    private func sendFrame(op: UInt32, payload: [String: Any]) {
        guard fd >= 0,
            JSONSerialization.isValidJSONObject(payload),
            let body = try? JSONSerialization.data(withJSONObject: payload)
        else { return }
        var frame = Data(capacity: 8 + body.count)
        withUnsafeBytes(of: op.littleEndian) { frame.append(contentsOf: $0) }
        withUnsafeBytes(of: UInt32(body.count).littleEndian) { frame.append(contentsOf: $0) }
        frame.append(body)

        var offset = 0
        frame.withUnsafeBytes { (raw: UnsafeRawBufferPointer) in
            while offset < frame.count {
                let n = write(fd, raw.baseAddress!.advanced(by: offset), frame.count - offset)
                guard n > 0 else { break }
                offset += n
            }
        }
        if offset < frame.count {
            teardown()
        }
    }

    private func teardown() {
        guard fd >= 0 else { return }
        readSource?.cancel()  // cancelハンドラがclose(fd)する
        readSource = nil
        fd = -1
        recvBuffer.removeAll()
        setState(.disconnected)
    }

    private func leUInt32(at offset: Int) -> UInt32 {
        var value: UInt32 = 0
        _ = withUnsafeMutableBytes(of: &value) {
            recvBuffer.copyBytes(to: $0, from: offset..<(offset + 4))
        }
        return UInt32(littleEndian: value)
    }

    private func setState(_ new: ConnState) {
        DispatchQueue.main.async { [weak self] in
            guard let self, self.state != new else { return }
            self.state = new
            self.onStateChange?(new)
        }
    }
}
