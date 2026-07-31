use super::models::{MusicSourceId, PlayerState, SourceState};

/// Policy: Apple Music is the only source, so it is active whenever it is
/// running, has a track, and is not stopped (a paused source still counts, so
/// the presence doesn't disappear the moment playback is paused).
pub fn select_active_source(apple_music: &SourceState) -> Option<MusicSourceId> {
    let candidate = apple_music.running
        && apple_music.track.is_some()
        && apple_music.player_state != PlayerState::Stopped;
    candidate.then_some(MusicSourceId::AppleMusic)
}
