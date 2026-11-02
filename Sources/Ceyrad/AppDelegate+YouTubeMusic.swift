import Foundation
import os

// MARK: - YouTube Music
//
// ブラウザの再生は MediaRemote（`NowPlayingAdapter`）から届く。その中から YouTube Music だけを
// 取り出す: ブラウザで、アルバム名があり、再生元のブラウザに music.youtube.com のタブがあること。

extension AppDelegate {
    private static let log = Logger(
        subsystem: Bundle.main.bundleIdentifier ?? "Ceyrad", category: "YouTubeMusic"
    )

    func configureYouTubeMusic() {
        nowPlaying.onPayload = { [weak self] in self?.handleNowPlaying($0) }
        nowPlaying.onFailure = { [weak self] in self?.youtubeFailed($0) }
        if settings.youtubeMusicEnabled { startYouTubeMusic() }
    }

    func startYouTubeMusic() {
        youtubeUnavailable = nil
        youtube.running = true
        nowPlaying.start()
    }

    func stopYouTubeMusic() {
        nowPlaying.stop()
        resetYouTubeMusic()
    }

    /// 取得元が使えない。動いていないものを「動いている」ことにしないが、理由はメニューに残す。
    private func youtubeFailed(_ reason: String) {
        resetYouTubeMusic()
        youtubeUnavailable = reason
        reevaluate()
    }

    private func resetYouTubeMusic() {
        cancelYouTubeLoss()
        latestPayload = nil
        youtubeTabVerdict = nil
        youtubeUnavailable = nil
        youtube = MusicState()
        cancelCatalogRetry(.youtubeMusic)
        reevaluate()
    }

    /// アダプタから届いた「いま再生中」。`nil` は何も再生していないこと。
    private func handleNowPlaying(_ payload: NowPlayingPayload?) {
        guard youtube.running else { return }
        latestPayload = payload

        // アーティストが空なのは、曲が替わった直後にメタデータが揃いきる前の更新。
        // 曲名だけでカタログを引くと別の曲に当たりうるので、揃うまで待つ。
        guard let payload, YouTubeMusic.isBrowser(bundleId: payload.bundleId),
            !payload.artist.isEmpty
        else {
            scheduleYouTubeLoss()
            return
        }

        // 同じ曲のあいだは再生/一時停止やシークが何度来ても、前回のタブ確認を使い回す。
        if let verdict = youtubeTabVerdict, verdict.identity == payload.identity {
            applyYouTubeMusic(payload, tabs: verdict.tabs)
            return
        }
        BrowserTabs.tabs(bundleId: payload.bundleId) { [weak self] result in
            // 確認しているあいだに曲が替わった/止まったなら、この答えは古い
            guard let self, self.youtube.running,
                let latest = self.latestPayload, latest.identity == payload.identity
            else { return }
            switch result {
            case .success(let tabs):
                self.youtubeTabVerdict = (payload.identity, tabs)
                let musicTabs = YouTubeMusic.musicTabs(tabs)
                let titleMatch = musicTabs.contains {
                    YouTubeMusic.titleMatches($0, trackTitle: payload.title)
                }
                // 曲名などは載せない。判定が「なぜそうなったか」だけを残す。
                Self.log.info(
                    """
                    tab check: browser=\(payload.bundleId, privacy: .public) \
                    tabs=\(tabs.count) musicTabs=\(musicTabs.count) \
                    hasAlbum=\(!payload.album.isEmpty) \
                    titleMatch=\(titleMatch) \
                    songURL=\(YouTubeMusic.songURL(fromTabs: tabs, trackTitle: payload.title) != nil)
                    """)
                self.applyYouTubeMusic(latest, tabs: tabs)
            case .failure(let failure):
                Self.log.error("tab check failed: \(String(describing: failure), privacy: .public)")
                self.scheduleYouTubeLoss()
            }
        }
    }

    private func applyYouTubeMusic(_ payload: NowPlayingPayload, tabs: [BrowserTab]) {
        guard
            YouTubeMusic.isYouTubeMusic(
                isBrowser: true, album: payload.album, trackTitle: payload.title, tabs: tabs)
        else {
            scheduleYouTubeLoss()
            return
        }
        cancelYouTubeLoss()
        handlePlayerUpdate(
            .youtubeMusic, state: payload.playerState, info: payload.trackInfo,
            pageURL: YouTubeMusic.songURL(fromTabs: tabs, trackTitle: payload.title)
        )
    }

    /// YouTube Music でなくなった（別の動画に替わった・タブを閉じた）。すぐには消さず、
    /// 猶予のあいだに YouTube Music の更新が来れば取り消す。
    private func scheduleYouTubeLoss() {
        guard youtube.track != nil, youtubeLossWork == nil else { return }
        let work = DispatchWorkItem { [weak self] in
            guard let self else { return }
            self.youtubeLossWork = nil
            self.youtubeTabVerdict = nil
            self.handlePlayerUpdate(.youtubeMusic, state: .stopped, info: nil)
        }
        youtubeLossWork = work
        DispatchQueue.main.asyncAfter(deadline: .now() + Self.youtubeLossGrace, execute: work)
    }

    private func cancelYouTubeLoss() {
        youtubeLossWork?.cancel()
        youtubeLossWork = nil
    }
}
