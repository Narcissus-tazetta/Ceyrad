import Foundation

// MARK: - Discord connection
//
// 接続は Application（client ID）ごとで、Discord に出る名前もそれで決まる。表示するソースが
// 変わったら、いまの接続を閉じて新しいソースの client ID で繋ぎ直す。

extension AppDelegate {
    /// いま接続しておくべきソース。表示中のソースがあればそれ、なければ起動中のApple Music
    /// （再生が始まったらすぐ出せるよう繋いでおく）。どちらもなければ繋がない。
    var connectionSource: MusicSource? {
        displayedSource ?? (appleMusic.running ? .appleMusic : nil)
    }

    /// 接続を `connectionSource` に合わせる。繋ぐ必要がなくなったら休止に入る。
    func syncConnection() {
        guard let wanted = connectionSource else {
            goDormant()
            return
        }
        switch rpc.state {
        case .disconnected:
            // 再試行の予約があるなら、その時刻まで待つ（イベントのたびに繋ぎにいかない）
            if !reconnect.isPending { attemptConnect() }
        case .connecting:
            break  // 繋がったときに `handleRPCState` が合っているか確かめる
        case .connected:
            if connectedSource != wanted { switchConnection() }
        }
    }

    /// 別の client ID で繋ぎ直す。古い接続のActivityは閉じる前に消す。
    private func switchConnection() {
        connectedSource = nil
        reconnect.expectImmediateReconnect()
        rpc.shutdown(clearActivity: true)
    }

    /// 繋ぐものが何もなくなった: 全て止めて完全休止に戻す（常時接続しない）。
    ///
    /// 「止めるものを1つ書き忘れる」が起きないよう、休止に入る道はこれ1本だけにする。
    private func goDormant() {
        displaying = false
        connectedSource = nil
        cancelReconnect()
        pauseHide.cancel()
        debouncer.cancel()
        lastSent = .nothing
        rpc.shutdown(clearActivity: true)
    }

    /// デバウンス窓が明けた実際の送信。
    ///
    /// 未接続なら捨てる: 接続できたら新しく組み立て直したものが送られるので、そちらのほうが新しい。
    /// 別のソースの client ID で繋がっているあいだも捨てる——別の Application の名前で
    /// 表示されてしまうため。
    ///
    /// `setActivity`はソケット用キューに渡すだけで成否を返さないため、`lastSent`は
    /// 「Discordに届いた」ではなく「渡した」時点で更新される。それでも届かなかった内容が
    /// その後の送信を抑止し続けないのは、書き込みに失敗したソケットが`teardown`から
    /// `.disconnected`を通り、`handleRPCState`がそこで`lastSent`を捨てるため——
    /// つまり抑止が生き残るのは接続が生きている間だけ、という不変条件で担保している。
    /// （Windows版は同期送信なので戻り値を見て更新する。行き着く先は同じ）
    func flushActivity(_ activity: [String: Any]?, for source: MusicSource?) {
        guard rpc.state == .connected else { return }
        if activity != nil, connectedSource != source { return }
        guard !lastSent.isEquivalent(to: activity) else { return }
        rpc.setActivity(activity)
        lastSent = .sent(activity)
    }

    func handleRPCState(_ state: DiscordRPCClient.ConnState) {
        switch state {
        case .connected:
            reconnect.reset()
            // つながったばかりの接続には何も表示されていない。前回の送信を覚えたままだと
            // 1発目が「前と同じ」と判定されて、何も出ないまま終わる。
            lastSent = .nothing
            // 繋いでいる間にソースが替わっていたら、この接続は使えない。
            if let wanted = connectionSource, connectedSource != wanted {
                switchConnection()
                return
            }
            pushActivity()
        case .disconnected:
            lastSent = .nothing
            connectedSource = nil
            if reconnect.takeImmediate() {
                attemptConnect()
            } else if connectionSource != nil {
                // 繋ぐものがあるときだけ再試行する（常時接続しない）
                reconnect.schedule { [weak self] in self?.attemptConnect() }
            }
        case .connecting:
            break
        }
    }

    /// メニューの「Reconnect to Discord」。
    ///
    /// つながっていても一度切ってから繋ぎ直す。「Reconnect」と書かれたボタンは
    /// 見た目に問題がないときこそ押されるもので、そこで何も起きないのが一番困る。
    func reconnectNow() {
        reconnect.reset()
        guard rpc.state != .disconnected else {
            attemptConnect()
            return
        }
        reconnect.expectImmediateReconnect()
        rpc.shutdown(clearActivity: true)
    }

    func attemptConnect() {
        guard let wanted = connectionSource, rpc.state == .disconnected else { return }
        connectedSource = wanted
        rpc.connect(clientId: SettingsStore.clientId(for: wanted))
    }

    func cancelReconnect() {
        reconnect.reset()
    }
}
