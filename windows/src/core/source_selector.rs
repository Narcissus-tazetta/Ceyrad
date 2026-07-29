use super::models::{MusicSourceId, PlayerState, SourceState};

/// Policy:
/// 1. Candidate = running && has a track && not stopped
/// 2. If exactly one source is playing it wins (a paused source yields)
/// 3. If both are playing, the one with the more recent event wins
/// 4. If neither is playing, keep the current source while it stays a
///    candidate, so the presence doesn't flip when both are merely paused
pub fn select_active_source(
    apple_music: &SourceState,
    spotify: &SourceState,
    current: Option<MusicSourceId>,
) -> Option<MusicSourceId> {
    fn is_candidate(s: &SourceState) -> bool {
        s.running && s.track.is_some() && s.player_state != PlayerState::Stopped
    }

    let am_candidate = is_candidate(apple_music);
    let sp_candidate = is_candidate(spotify);
    let am_playing = am_candidate && apple_music.player_state == PlayerState::Playing;
    let sp_playing = sp_candidate && spotify.player_state == PlayerState::Playing;

    // Ties resolve to Apple Music.
    let most_recent = || {
        if spotify.last_event_uptime_ns > apple_music.last_event_uptime_ns {
            MusicSourceId::Spotify
        } else {
            MusicSourceId::AppleMusic
        }
    };

    match (am_playing, sp_playing) {
        (true, false) => Some(MusicSourceId::AppleMusic),
        (false, true) => Some(MusicSourceId::Spotify),
        (true, true) => Some(most_recent()),
        (false, false) => {
            if let Some(current) = current {
                let current_is_candidate = match current {
                    MusicSourceId::AppleMusic => am_candidate,
                    MusicSourceId::Spotify => sp_candidate,
                };
                if current_is_candidate {
                    return Some(current);
                }
            }
            match (am_candidate, sp_candidate) {
                (true, false) => Some(MusicSourceId::AppleMusic),
                (false, true) => Some(MusicSourceId::Spotify),
                (true, true) => Some(most_recent()),
                (false, false) => None,
            }
        }
    }
}
