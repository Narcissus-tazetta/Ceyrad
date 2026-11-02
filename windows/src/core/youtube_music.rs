//! YouTube Music, recognised from a browser's media session.
//!
//! The macOS build keeps the same decision in `YouTubeMusic.swift`; the
//! two are pinned together by `spec/youtube_music_vectors.json`.

use std::time::Duration;

/// Shown in the status rows and the log. Not the name Discord displays — that
/// one belongs to the Application behind `DISCORD_CLIENT_ID`.
pub const DISPLAY_NAME: &str = "YouTube Music";

/// Placeholder. Discord shows "Listening to <Application name>", so YouTube
/// Music needs an Application of its own, named "YouTube Music".
///
/// Replace this with the Application ID from the Discord Developer Portal.
/// Until then Discord refuses the handshake and the connection is retried
/// with backoff for as long as YouTube Music is the source being shown.
pub const DISCORD_CLIENT_ID: &str = "000000000000000000";

/// Default text of the song button. Discord-facing, so never localized.
pub const SONG_BUTTON_LABEL: &str = "Play on YouTube Music";

/// How long a session that stopped looking like YouTube Music is still shown.
///
/// Metadata arrives piecemeal after a track change — the title first, the
/// album a moment later — and in between the session reads as a plain video.
/// Clearing at once would blink the presence off and on at every track change.
pub const LOSS_GRACE: Duration = Duration::from_millis(2_500);

/// Whether a media session is YouTube Music.
///
/// A browser reports no URL, so the album is what tells a song from a plain
/// video: YouTube Music fills it in and YouTube leaves it empty.
/// `has_youtube_music_tab` is `None` where the platform cannot look at the
/// browser's tabs (Windows), which leaves the album as the only evidence.
pub fn is_youtube_music(
    is_browser: bool,
    album: &str,
    has_youtube_music_tab: Option<bool>,
) -> bool {
    is_browser && !album.trim().is_empty() && has_youtube_music_tab.unwrap_or(true)
}
