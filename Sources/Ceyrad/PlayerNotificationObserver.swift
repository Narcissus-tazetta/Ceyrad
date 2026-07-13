import Foundation

/// プレイヤーの分散通知の購読。完全イベント駆動でポーリングなし。
/// ソースごとに1インスタンス持ち、非稼働ソースの購読は止める（未使用時コストゼロ）。
final class PlayerNotificationObserver {
    var onUpdate: ((PlayerState, TrackInfo?) -> Void)?

    private let notificationName: Notification.Name
    private let parse: ([AnyHashable: Any]) -> (PlayerState, TrackInfo?)
    private var observer: NSObjectProtocol?

    init(
        notificationName: Notification.Name,
        parse: @escaping ([AnyHashable: Any]) -> (PlayerState, TrackInfo?)
    ) {
        self.notificationName = notificationName
        self.parse = parse
    }

    func start() {
        guard observer == nil else { return }
        observer = DistributedNotificationCenter.default().addObserver(
            forName: notificationName,
            object: nil, queue: .main
        ) { [weak self] note in
            guard let self else { return }
            let (state, track) = self.parse(note.userInfo ?? [:])
            self.onUpdate?(state, track)
        }
    }

    func stop() {
        if let observer {
            DistributedNotificationCenter.default().removeObserver(observer)
        }
        observer = nil
    }
}
