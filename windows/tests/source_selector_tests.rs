//! Port of Tests/CeyradTests/SourceSelectorTests.swift

use ceyrad::core::models::{MusicSourceId, PlayerState, SourceState, TrackInfo};
use ceyrad::core::source_selector::select_active_source;

fn source(running: bool, state: PlayerState, has_track: bool) -> SourceState {
    SourceState {
        running,
        player_state: state,
        track: if has_track {
            Some(TrackInfo::new("T", "A", "L"))
        } else {
            None
        },
        ..SourceState::default()
    }
}

#[test]
fn nothing_running_yields_none() {
    let not_running = source(false, PlayerState::Stopped, false);
    assert_eq!(select_active_source(&not_running), None);
}

#[test]
fn a_playing_source_wins() {
    let playing = source(true, PlayerState::Playing, true);
    assert_eq!(
        select_active_source(&playing),
        Some(MusicSourceId::AppleMusic)
    );
}

#[test]
fn a_paused_source_still_counts() {
    // Keep showing the presence while merely paused, so it doesn't flip off.
    let paused = source(true, PlayerState::Paused, true);
    assert_eq!(
        select_active_source(&paused),
        Some(MusicSourceId::AppleMusic)
    );
}

#[test]
fn stopped_source_is_not_a_candidate() {
    let stopped = source(true, PlayerState::Stopped, true);
    assert_eq!(select_active_source(&stopped), None);
}

#[test]
fn running_without_track_is_not_a_candidate() {
    let no_track = source(true, PlayerState::Playing, false);
    assert_eq!(select_active_source(&no_track), None);
}
