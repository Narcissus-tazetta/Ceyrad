//! Re-sends are suppressed when Discord would render the same thing, because
//! SET_ACTIVITY is rate limited and SMTC fires far more often than the presence
//! actually changes.

use std::time::{Duration, SystemTime};

use ceyrad::core::activity_builder::{build, is_equivalent};
use ceyrad::core::models::{PlayerState, TrackInfo};
use ceyrad::core::settings_model::Settings;

fn playing_track() -> TrackInfo {
    let mut track = TrackInfo::new("Song", "Artist", "Album");
    track.duration_sec = Some(240.0);
    track.position_sec = Some(30.0);
    track
}

fn built_at(track: &TrackInfo, now: SystemTime) -> serde_json::Map<String, serde_json::Value> {
    build(track, PlayerState::Playing, None, &Settings::default(), now)
}

#[test]
fn rebuilding_unchanged_playback_is_equivalent() {
    let track = playing_track();
    let now = SystemTime::now();
    let first = built_at(&track, now);
    // Same playback, rebuilt later: the position has advanced by exactly the
    // time that passed, so the derived start stays put.
    let second = built_at(&track, now + Duration::from_secs(20));
    assert!(is_equivalent(&first, &second));
}

#[test]
fn a_seek_is_not_equivalent() {
    let track = playing_track();
    let now = SystemTime::now();
    let first = built_at(&track, now);

    let mut seeked = playing_track();
    seeked.position_sec = Some(150.0);
    let second = built_at(&seeked, now);
    assert!(!is_equivalent(&first, &second));
}

#[test]
fn a_different_track_is_not_equivalent() {
    let now = SystemTime::now();
    let first = built_at(&playing_track(), now);

    let mut other = playing_track();
    other.name = "Another Song".into();
    let second = built_at(&other, now);
    assert!(!is_equivalent(&first, &second));
}

#[test]
fn pausing_is_not_equivalent() {
    let track = playing_track();
    let now = SystemTime::now();
    let playing = built_at(&track, now);
    let paused = build(&track, PlayerState::Paused, None, &Settings::default(), now);
    assert!(!is_equivalent(&playing, &paused));
}

#[test]
fn a_missing_key_is_not_equivalent() {
    let now = SystemTime::now();
    let full = built_at(&playing_track(), now);
    let mut trimmed = full.clone();
    trimmed.remove("details");
    assert!(!is_equivalent(&full, &trimmed));
    assert!(!is_equivalent(&trimmed, &full));
}

#[test]
fn activities_without_timestamps_compare_exactly() {
    let now = SystemTime::now();
    let paused = build(
        &playing_track(),
        PlayerState::Paused,
        None,
        &Settings::default(),
        now,
    );
    assert!(!paused.contains_key("timestamps"));
    assert!(is_equivalent(&paused, &paused.clone()));
}
