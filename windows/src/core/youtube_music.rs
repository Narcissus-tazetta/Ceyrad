//! YouTube Music, recognised from a browser's media session.
//!
//! The macOS build keeps the same decision in `YouTubeMusic.swift`; the
//! two are pinned together by `spec/youtube_music_vectors.json`.

use std::time::Duration;

/// Shown in the status rows and the log. Not the name Discord displays — that
/// one belongs to the Application behind `DISCORD_CLIENT_ID`.
pub const DISPLAY_NAME: &str = "YouTube Music";

/// Discord shows "Listening to <Application name>", so YouTube Music has an
/// Application of its own, named "YouTube Music".
pub const DISCORD_CLIENT_ID: &str = "1566818184563269745";

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

/// Whether any browser window title shows `track_name` on a YouTube Music page.
///
/// The fallback for tracks that report no album (YouTube Music's video versions
/// do). Window titles only expose a window's active tab; requiring the track
/// name keeps an unrelated YouTube Music tab from claiming a plain video.
pub fn window_titles_show_youtube_music<S: AsRef<str>>(track_name: &str, titles: &[S]) -> bool {
    let name = track_name.trim().to_lowercase();
    if name.is_empty() {
        return false;
    }
    titles.iter().any(|title| {
        let title = title.as_ref().to_lowercase();
        title.contains("youtube music") && title.contains(&name)
    })
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn a_youtube_music_title_with_the_track_matches() {
        assert!(window_titles_show_youtube_music(
            "夜の踊り子",
            &["夜の踊り子 - YouTube Music - Google Chrome"]
        ));
    }

    #[test]
    fn a_plain_youtube_title_does_not_match() {
        assert!(!window_titles_show_youtube_music(
            "夜の踊り子",
            &["夜の踊り子 - YouTube - Google Chrome"]
        ));
    }

    #[test]
    fn another_track_does_not_match() {
        assert!(!window_titles_show_youtube_music(
            "夜の踊り子",
            &["別の曲 - YouTube Music - Google Chrome"]
        ));
    }

    #[test]
    fn an_empty_track_name_never_matches() {
        assert!(!window_titles_show_youtube_music(
            "  ",
            &["YouTube Music - Google Chrome"]
        ));
    }
}
