import Foundation

/// `com.apple.Music.playerInfo` 分散通知の購読。完全イベント駆動でポーリングなし。
/// 通知には再生位置が含まれないため、位置はAppleScriptで補完する（AppDelegate側）。
final class NowPlayingObserver {
    var onUpdate: ((PlayerState, TrackInfo?) -> Void)?

    private var observer: NSObjectProtocol?

    func start() {
        guard observer == nil else { return }
        observer = DistributedNotificationCenter.default().addObserver(
            forName: NSNotification.Name("com.apple.Music.playerInfo"),
            object: nil, queue: .main
        ) { [weak self] note in
            self?.handle(note)
        }
    }

    func stop() {
        if let observer {
            DistributedNotificationCenter.default().removeObserver(observer)
        }
        observer = nil
    }

    private func handle(_ note: Notification) {
        let info = note.userInfo ?? [:]
        let state: PlayerState
        switch (info["Player State"] as? String ?? "").lowercased() {
        case "playing": state = .playing
        case "paused": state = .paused
        default: state = .stopped
        }
        guard state != .stopped else {
            onUpdate?(.stopped, nil)
            return
        }
        var track = TrackInfo(
            name: info["Name"] as? String ?? "",
            artist: info["Artist"] as? String ?? "",
            album: info["Album"] as? String ?? "",
            durationSec: nil,
            positionSec: nil
        )
        if let ms = (info["Total Time"] as? NSNumber)?.doubleValue, ms > 0 {
            track.durationSec = ms / 1000.0
        }
        onUpdate?(state, track)
    }
}
