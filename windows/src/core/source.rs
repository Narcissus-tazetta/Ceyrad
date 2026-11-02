//! Which player the presence is about.
//!
//! The macOS build keeps the same rule in `MusicSource` / `SourceSelector`;
//! both choose the same source for the same situation.

use super::apple_music;
use super::models::{MusicState, PlayerState};
use super::youtube_music;

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Source {
    AppleMusic,
    YouTubeMusic,
}

impl Source {
    pub const ALL: [Source; 2] = [Source::AppleMusic, Source::YouTubeMusic];

    pub fn display_name(self) -> &'static str {
        match self {
            Source::AppleMusic => apple_music::DISPLAY_NAME,
            Source::YouTubeMusic => youtube_music::DISPLAY_NAME,
        }
    }

    /// The Discord Application this source is shown under.
    pub fn client_id(self) -> &'static str {
        match self {
            Source::AppleMusic => apple_music::DISCORD_CLIENT_ID,
            Source::YouTubeMusic => youtube_music::DISCORD_CLIENT_ID,
        }
    }

    pub fn song_button_label(self) -> &'static str {
        match self {
            Source::AppleMusic => "Play on Apple Music",
            Source::YouTubeMusic => youtube_music::SONG_BUTTON_LABEL,
        }
    }
}

/// The source worth showing, if either is.
///
/// Playing beats paused. Between two that are both playing, or both paused, the
/// one that started playing more recently wins; a source that never started
/// loses to one that did, and Apple Music takes an exact tie so the answer never
/// depends on call order.
pub fn select_source(apple_music: &MusicState, youtube_music: &MusicState) -> Option<Source> {
    match (apple_music.is_displayable(), youtube_music.is_displayable()) {
        (false, false) => None,
        (true, false) => Some(Source::AppleMusic),
        (false, true) => Some(Source::YouTubeMusic),
        (true, true) => {
            let apple_playing = apple_music.player_state == PlayerState::Playing;
            let youtube_playing = youtube_music.player_state == PlayerState::Playing;
            let youtube_wins = if apple_playing != youtube_playing {
                youtube_playing
            } else {
                youtube_music.playing_since > apple_music.playing_since
            };
            Some(if youtube_wins {
                Source::YouTubeMusic
            } else {
                Source::AppleMusic
            })
        }
    }
}

#[cfg(test)]
mod tests {
    use std::time::{Duration, Instant};

    use super::*;
    use crate::core::models::TrackInfo;

    fn state(player_state: PlayerState, playing_since: Option<Instant>) -> MusicState {
        MusicState {
            running: true,
            player_state,
            track: Some(TrackInfo::new("Song", "Artist", "Album")),
            playing_since,
            ..MusicState::default()
        }
    }

    fn gone() -> MusicState {
        MusicState::default()
    }

    #[test]
    fn nothing_displayable_selects_nothing() {
        assert_eq!(select_source(&gone(), &gone()), None);

        let mut stopped = state(PlayerState::Stopped, None);
        stopped.track = None;
        assert_eq!(select_source(&stopped, &gone()), None);
    }

    #[test]
    fn the_only_displayable_source_is_selected() {
        let now = Instant::now();
        let playing = state(PlayerState::Playing, Some(now));
        assert_eq!(select_source(&playing, &gone()), Some(Source::AppleMusic));
        assert_eq!(select_source(&gone(), &playing), Some(Source::YouTubeMusic));
    }

    #[test]
    fn playing_beats_paused_whichever_started_later() {
        let early = Instant::now();
        let late = early + Duration::from_secs(60);

        let paused_late = state(PlayerState::Paused, Some(late));
        let playing_early = state(PlayerState::Playing, Some(early));
        assert_eq!(
            select_source(&paused_late, &playing_early),
            Some(Source::YouTubeMusic)
        );
        assert_eq!(
            select_source(&playing_early, &paused_late),
            Some(Source::AppleMusic)
        );
    }

    #[test]
    fn of_two_playing_the_later_start_wins() {
        let early = Instant::now();
        let late = early + Duration::from_secs(60);
        let first = state(PlayerState::Playing, Some(early));
        let second = state(PlayerState::Playing, Some(late));

        assert_eq!(select_source(&first, &second), Some(Source::YouTubeMusic));
        assert_eq!(select_source(&second, &first), Some(Source::AppleMusic));
    }

    #[test]
    fn of_two_paused_the_later_start_wins() {
        let early = Instant::now();
        let late = early + Duration::from_secs(60);
        let first = state(PlayerState::Paused, Some(early));
        let second = state(PlayerState::Paused, Some(late));

        assert_eq!(select_source(&first, &second), Some(Source::YouTubeMusic));
        assert_eq!(select_source(&second, &first), Some(Source::AppleMusic));
    }

    #[test]
    fn a_tie_goes_to_apple_music_in_either_order() {
        let now = Instant::now();
        let a = state(PlayerState::Playing, Some(now));
        let b = state(PlayerState::Playing, Some(now));
        assert_eq!(select_source(&a, &b), Some(Source::AppleMusic));

        let never_a = state(PlayerState::Playing, None);
        let never_b = state(PlayerState::Playing, None);
        assert_eq!(select_source(&never_a, &never_b), Some(Source::AppleMusic));
    }

    #[test]
    fn a_source_that_never_started_loses_to_one_that_did() {
        let started = state(PlayerState::Playing, Some(Instant::now()));
        let never = state(PlayerState::Playing, None);
        assert_eq!(select_source(&never, &started), Some(Source::YouTubeMusic));
        assert_eq!(select_source(&started, &never), Some(Source::AppleMusic));
    }
}
