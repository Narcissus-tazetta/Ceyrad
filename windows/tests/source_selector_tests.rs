//! Port of Tests/CeyradTests/SourceSelectorTests.swift

use ceyrad::core::models::{MusicSourceId, PlayerState, SourceState, TrackInfo};
use ceyrad::core::source_selector::select_active_source;

fn source(running: bool, state: PlayerState, has_track: bool, event_ns: u64) -> SourceState {
    SourceState {
        running,
        player_state: state,
        track: if has_track {
            Some(TrackInfo::new("T", "A", "L"))
        } else {
            None
        },
        catalog: None,
        last_event_uptime_ns: event_ns,
    }
}

/// Defaults matching the Swift helper: running, stopped, has a track, event 0.
fn s(state: PlayerState) -> SourceState {
    source(true, state, true, 0)
}

fn s_at(state: PlayerState, event_ns: u64) -> SourceState {
    source(true, state, true, event_ns)
}

fn not_running() -> SourceState {
    source(false, PlayerState::Stopped, false, 0)
}

fn select(
    am: &SourceState,
    sp: &SourceState,
    current: Option<MusicSourceId>,
) -> Option<MusicSourceId> {
    select_active_source(am, sp, current)
}

#[test]
fn nothing_running_yields_none() {
    assert_eq!(select(&not_running(), &not_running(), None), None);
}

#[test]
fn single_playing_source_wins() {
    assert_eq!(
        select(&s(PlayerState::Playing), &not_running(), None),
        Some(MusicSourceId::AppleMusic)
    );
    assert_eq!(
        select(&not_running(), &s(PlayerState::Playing), None),
        Some(MusicSourceId::Spotify)
    );
}

#[test]
fn playing_beats_paused() {
    assert_eq!(
        select(&s(PlayerState::Paused), &s(PlayerState::Playing), None),
        Some(MusicSourceId::Spotify)
    );
    assert_eq!(
        select(
            &s(PlayerState::Paused),
            &s(PlayerState::Playing),
            Some(MusicSourceId::AppleMusic)
        ),
        Some(MusicSourceId::Spotify)
    );
    assert_eq!(
        select(
            &s(PlayerState::Playing),
            &s(PlayerState::Paused),
            Some(MusicSourceId::Spotify)
        ),
        Some(MusicSourceId::AppleMusic)
    );
}

#[test]
fn both_playing_most_recent_event_wins() {
    assert_eq!(
        select(
            &s_at(PlayerState::Playing, 100),
            &s_at(PlayerState::Playing, 200),
            None
        ),
        Some(MusicSourceId::Spotify)
    );
    assert_eq!(
        select(
            &s_at(PlayerState::Playing, 300),
            &s_at(PlayerState::Playing, 200),
            None
        ),
        Some(MusicSourceId::AppleMusic)
    );
}

#[test]
fn both_paused_sticks_to_current() {
    // Keep showing the current source so the presence doesn't flip.
    assert_eq!(
        select(
            &s_at(PlayerState::Paused, 100),
            &s_at(PlayerState::Paused, 200),
            Some(MusicSourceId::AppleMusic)
        ),
        Some(MusicSourceId::AppleMusic)
    );
    assert_eq!(
        select(
            &s_at(PlayerState::Paused, 200),
            &s_at(PlayerState::Paused, 100),
            Some(MusicSourceId::Spotify)
        ),
        Some(MusicSourceId::Spotify)
    );
}

#[test]
fn both_paused_without_current_picks_most_recent() {
    assert_eq!(
        select(
            &s_at(PlayerState::Paused, 100),
            &s_at(PlayerState::Paused, 200),
            None
        ),
        Some(MusicSourceId::Spotify)
    );
}

#[test]
fn current_terminated_falls_back_to_other_candidate() {
    assert_eq!(
        select(
            &not_running(),
            &s(PlayerState::Paused),
            Some(MusicSourceId::AppleMusic)
        ),
        Some(MusicSourceId::Spotify)
    );
}

#[test]
fn stopped_source_is_not_a_candidate() {
    let stopped = source(true, PlayerState::Stopped, false, 0);
    assert_eq!(
        select(&stopped, &stopped, Some(MusicSourceId::AppleMusic)),
        None
    );
    assert_eq!(
        select(
            &stopped,
            &s(PlayerState::Paused),
            Some(MusicSourceId::AppleMusic)
        ),
        Some(MusicSourceId::Spotify)
    );
}

#[test]
fn running_without_track_is_not_a_candidate() {
    assert_eq!(
        select(
            &source(true, PlayerState::Playing, false, 0),
            &not_running(),
            None
        ),
        None
    );
}
