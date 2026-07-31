//! Telling a real change apart from SMTC repeating itself.
//!
//! SMTC re-reports the current session several times a second while a track
//! plays, so "did anything happen?" has to be answered from the values rather
//! than from the fact that an event arrived. macOS never needed this: its
//! notifications fire only on an actual state change.

use super::models::TrackInfo;

/// How far the position may differ from where it would have drifted on its own
/// before it counts as a seek. Generous enough to absorb the lag between SMTC
/// sampling a position and the app reading it.
pub const SEEK_THRESHOLD_SECS: f64 = 3.0;

/// Whether both readings describe the same track (or both describe none).
///
/// Field by field rather than `a.identity() == b.identity()`, which would build
/// and discard two `String`s every time it is asked — and it is asked for every
/// source on every SMTC event, which is the most frequent thing this app does.
/// The test below pins the two to the same answer.
pub fn same_identity(a: Option<&TrackInfo>, b: Option<&TrackInfo>) -> bool {
    let (Some(a), Some(b)) = (a, b) else {
        return a.is_none() && b.is_none();
    };
    a.name == b.name && a.artist == b.artist && a.album == b.album
}

/// True when the new position is somewhere the old one could not have reached
/// by simply playing on — i.e. the user seeked, which the progress bar must
/// follow. Readings without a position, or of different tracks, are not seeks.
pub fn position_jumped(old: Option<&TrackInfo>, new: Option<&TrackInfo>) -> bool {
    let (Some(old), Some(new)) = (old, new) else {
        return false;
    };
    let (Some(old_position), Some(new_position)) = (old.position_sec, new.position_sec) else {
        return false;
    };
    // A clock that appears to run backwards is treated as no time passing,
    // which at worst reports a seek that did not happen.
    let elapsed = new
        .position_sampled_at
        .duration_since(old.position_sampled_at)
        .map(|d| d.as_secs_f64())
        .unwrap_or(0.0);
    (new_position - (old_position + elapsed)).abs() > SEEK_THRESHOLD_SECS
}

#[cfg(test)]
mod tests {
    use std::time::{Duration, SystemTime};

    use super::*;

    fn at(position: f64, sampled_at: SystemTime) -> TrackInfo {
        let mut track = TrackInfo::new("Song", "Artist", "Album");
        track.position_sec = Some(position);
        track.position_sampled_at = sampled_at;
        track
    }

    #[test]
    fn the_same_track_has_the_same_identity() {
        let a = TrackInfo::new("Song", "Artist", "Album");
        let b = TrackInfo::new("Song", "Artist", "Album");
        assert!(same_identity(Some(&a), Some(&b)));
    }

    #[test]
    fn a_different_album_is_a_different_track() {
        let a = TrackInfo::new("Song", "Artist", "Album");
        let b = TrackInfo::new("Song", "Artist", "Other");
        assert!(!same_identity(Some(&a), Some(&b)));
    }

    /// The property that lets `same_identity` skip building the strings: for
    /// every pair, it must answer exactly what comparing the two identities
    /// would have. A change to `identity` that this no longer mirrors fails
    /// here rather than by quietly calling two tracks one.
    #[test]
    fn same_identity_answers_what_comparing_identities_would() {
        fn track(name: &str, artist: &str, album: &str) -> TrackInfo {
            TrackInfo::new(name, artist, album)
        }
        let cases = [
            track("Song", "Artist", "Album"),
            track("Song", "Artist", "Other"),
            track("Song", "Other", "Album"),
            track("Other", "Artist", "Album"),
            track("Song", "", ""),
            track("", "", ""),
            // A separator inside a field, which is the one shape the joined
            // form is ambiguous about.
            track("Song\u{1F}Artist", "", "Album"),
        ];
        for a in &cases {
            for b in &cases {
                assert_eq!(
                    same_identity(Some(a), Some(b)),
                    a.identity() == b.identity(),
                    "{a:?}\nvs\n{b:?}"
                );
                // And the borrowed-key form the catalog matches answers with.
                assert_eq!(
                    a.matches_identity(&b.identity()),
                    a.identity() == b.identity(),
                    "matches_identity: {a:?}\nvs\n{b:?}"
                );
            }
        }
    }

    #[test]
    fn nothing_matches_nothing_but_not_something() {
        let a = TrackInfo::new("Song", "Artist", "Album");
        assert!(same_identity(None, None));
        assert!(!same_identity(Some(&a), None));
        assert!(!same_identity(None, Some(&a)));
    }

    #[test]
    fn ordinary_playback_is_not_a_seek() {
        let now = SystemTime::now();
        let old = at(30.0, now);
        // 20s later, 20s further in: exactly the drift of playing on.
        let new = at(50.0, now + Duration::from_secs(20));
        assert!(!position_jumped(Some(&old), Some(&new)));
    }

    #[test]
    fn a_small_sampling_lag_is_not_a_seek() {
        let now = SystemTime::now();
        let old = at(30.0, now);
        let new = at(52.0, now + Duration::from_secs(20));
        assert!(!position_jumped(Some(&old), Some(&new)));
    }

    #[test]
    fn jumping_forward_is_a_seek() {
        let now = SystemTime::now();
        let old = at(30.0, now);
        let new = at(150.0, now + Duration::from_secs(1));
        assert!(position_jumped(Some(&old), Some(&new)));
    }

    #[test]
    fn jumping_backward_is_a_seek() {
        let now = SystemTime::now();
        let old = at(150.0, now);
        let new = at(10.0, now + Duration::from_secs(1));
        assert!(position_jumped(Some(&old), Some(&new)));
    }

    #[test]
    fn a_missing_position_cannot_be_a_seek() {
        let now = SystemTime::now();
        let old = at(30.0, now);
        let mut new = at(200.0, now);
        new.position_sec = None;
        assert!(!position_jumped(Some(&old), Some(&new)));
        assert!(!position_jumped(None, Some(&old)));
        assert!(!position_jumped(Some(&old), None));
    }

    #[test]
    fn a_paused_track_read_twice_is_not_a_seek() {
        let now = SystemTime::now();
        // Paused: SMTC keeps reporting the same position and the same sample
        // time, however long the app waits between reads.
        let old = at(75.0, now);
        let new = at(75.0, now);
        assert!(!position_jumped(Some(&old), Some(&new)));
    }
}
