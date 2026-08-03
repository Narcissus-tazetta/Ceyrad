import Foundation

/// 対応プレイヤーはApple Musicのみ。プレイヤー固有の値と通知パースをここに集約する。
enum AppleMusic {
    static let bundleId = "com.apple.Music"
    static let notificationName = Notification.Name("com.apple.Music.playerInfo")
    /// メニュー表示等に使う名前。Discord側の「〜を再生中」の名前はApplication名で決まる。
    static let displayName = "Apple Music"

    /// `com.apple.Music.playerInfo` のuserInfoパース。
    /// 通知には再生位置が含まれないため、位置はAppleScriptで補完する（AppDelegate側）。
    static func parse(_ info: [AnyHashable: Any]) -> (PlayerState, TrackInfo?) {
        let state: PlayerState
        switch (info["Player State"] as? String ?? "").lowercased() {
        case "playing": state = .playing
        case "paused": state = .paused
        default: state = .stopped
        }
        guard state != .stopped else { return (.stopped, nil) }
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
        return (state, track)
    }
}
