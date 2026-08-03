use std::time::{Instant, SystemTime};

/// Separator used to join track fields into an identity string (U+001F).
const UNIT_SEPARATOR: char = '\u{1F}';

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum PlayerState {
    Playing,
    Paused,
    Stopped,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum ConnState {
    Disconnected,
    Connecting,
    Connected,
}

#[derive(Debug, Clone, PartialEq)]
pub struct TrackInfo {
    pub name: String,
    pub artist: String,
    pub album: String,
    pub duration_sec: Option<f64>,
    pub position_sec: Option<f64>,
    /// When `position_sec` was sampled. Re-sending an activity with a stale
    /// sample would rewind Discord's progress bar, so the elapsed time since
    /// this instant is added back in at build time.
    pub position_sampled_at: SystemTime,
}

impl TrackInfo {
    pub fn new(
        name: impl Into<String>,
        artist: impl Into<String>,
        album: impl Into<String>,
    ) -> Self {
        Self {
            name: name.into(),
            artist: artist.into(),
            album: album.into(),
            duration_sec: None,
            position_sec: None,
            position_sampled_at: SystemTime::now(),
        }
    }

    pub fn identity(&self) -> String {
        format!(
            "{}{}{}{}{}",
            self.name, UNIT_SEPARATOR, self.artist, UNIT_SEPARATOR, self.album
        )
    }

    /// Whether `identity` would have produced exactly `candidate`.
    ///
    /// `self.identity() == candidate` without the `String` that gets built and
    /// thrown away to answer it. Asked on every SMTC event — several times a
    /// second — for any track the catalog had no answer for, which is every
    /// locally imported one. The test below pins it to `identity` itself.
    pub fn matches_identity(&self, candidate: &str) -> bool {
        // Walks the join rather than splitting on the separator. Splitting
        // looks equivalent and is not: a field is free to contain a separator
        // of its own — nothing stops a title — and the split would then hand
        // the pieces back in the wrong places. Consuming each field by its
        // own length cannot.
        let Some(rest) = candidate.strip_prefix(self.name.as_str()) else {
            return false;
        };
        let Some(rest) = rest.strip_prefix(UNIT_SEPARATOR) else {
            return false;
        };
        let Some(rest) = rest.strip_prefix(self.artist.as_str()) else {
            return false;
        };
        let Some(rest) = rest.strip_prefix(UNIT_SEPARATOR) else {
            return false;
        };
        rest == self.album
    }

    /// Takes on `other`'s playback position, leaving the strings alone.
    ///
    /// For the case that dominates everything else this app does: the same
    /// track, a moment later. SMTC re-reports a playing session several times a
    /// second, and every one of those readings agrees with the last about the
    /// name, the artist and the album — so cloning a whole `TrackInfo` over the
    /// one already held is three heap allocations to arrive back where we were.
    ///
    /// Only sound when the two are the same track: `identity` covers the name,
    /// the artist and the album, and this covers everything it does not.
    /// **A field added to this type belongs in one of those two places**, or it
    /// will silently stop being updated.
    pub fn adopt_playback_from(&mut self, other: &TrackInfo) {
        self.duration_sec = other.duration_sec;
        self.position_sec = other.position_sec;
        self.position_sampled_at = other.position_sampled_at;
    }
}

#[derive(Debug, Clone, Default, PartialEq)]
pub struct CatalogInfo {
    pub song_url: Option<String>,
    pub artist_url: Option<String>,
    pub album_url: Option<String>,
    pub artwork_url: Option<String>,
}

/// What Apple Music is doing right now. There is one player, so this is the
/// whole of the app's music-side state.
#[derive(Debug, Clone)]
pub struct MusicState {
    pub running: bool,
    pub player_state: PlayerState,
    pub track: Option<TrackInfo>,
    pub catalog: Option<CatalogInfo>,
    /// The `TrackInfo::identity` a catalog lookup has already been asked for.
    /// SMTC reports several events a second while a track plays, so without
    /// this the same track would be looked up over and over.
    pub catalog_requested_for: Option<String>,
    /// When a lookup that failed outright — offline, or the API refused — may
    /// be tried again. A failure is not an answer, so unlike a catalogue miss
    /// it must not silently cost the track its artwork for good.
    pub catalog_retry_at: Option<Instant>,
}

impl Default for MusicState {
    fn default() -> Self {
        Self {
            running: false,
            player_state: PlayerState::Stopped,
            track: None,
            catalog: None,
            catalog_requested_for: None,
            catalog_retry_at: None,
        }
    }
}

impl MusicState {
    /// Whether there is something to put on Discord: running, with a track, and
    /// not stopped. A paused player still counts — whether the presence is
    /// actually cleared is `pause_hide_minutes`'s decision, not this one.
    pub fn is_displayable(&self) -> bool {
        self.running && self.track.is_some() && self.player_state != PlayerState::Stopped
    }

    /// Wipes the catalog back to "nothing known", for a track change or a
    /// player that went away. The request guard goes with it: leaving it set
    /// would tell the next track it had already been asked about.
    pub fn clear_catalog(&mut self) {
        self.catalog = None;
        self.catalog_requested_for = None;
        self.catalog_retry_at = None;
    }
}

#[cfg(test)]
mod tests {
    use std::time::Duration;

    use super::*;

    #[test]
    fn fields_are_joined_by_a_unit_separator() {
        // Pinned because this value gates catalog lookups and change
        // detection: a separator that appeared inside a field would let two
        // different tracks collide.
        let track = TrackInfo::new("Song", "Artist", "Album");
        assert_eq!(track.identity(), "Song\u{1F}Artist\u{1F}Album");

        let empty = TrackInfo::new("Song", "", "");
        assert_eq!(empty.identity(), "Song\u{1F}\u{1F}");
    }

    #[test]
    fn tracks_differing_only_in_album_have_different_identities() {
        let a = TrackInfo::new("Song", "Artist", "Album One");
        let b = TrackInfo::new("Song", "Artist", "Album Two");
        assert_ne!(a.identity(), b.identity());
    }

    #[test]
    fn adopting_playback_leaves_nothing_a_clone_would_have_carried() {
        // The contract that makes skipping the clone safe: for two readings of
        // the same track, adopting must be indistinguishable from cloning. A
        // field added to `TrackInfo` and covered by neither `identity` nor
        // `adopt_playback_from` fails here.
        let mut held = TrackInfo::new("Song", "Artist", "Album");
        held.duration_sec = Some(200.0);
        held.position_sec = Some(10.0);

        let mut incoming = TrackInfo::new("Song", "Artist", "Album");
        incoming.duration_sec = Some(201.0);
        incoming.position_sec = Some(42.5);
        incoming.position_sampled_at = SystemTime::now() + Duration::from_secs(30);

        assert_eq!(held.identity(), incoming.identity(), "the same track");
        held.adopt_playback_from(&incoming);
        assert_eq!(held, incoming);
    }

    fn playing_with_track() -> MusicState {
        MusicState {
            running: true,
            player_state: PlayerState::Playing,
            track: Some(TrackInfo::new("Song", "Artist", "Album")),
            ..MusicState::default()
        }
    }

    #[test]
    fn nothing_is_displayable_until_a_running_player_has_a_track() {
        assert!(!MusicState::default().is_displayable());
        assert!(playing_with_track().is_displayable());

        let mut no_track = playing_with_track();
        no_track.track = None;
        assert!(!no_track.is_displayable(), "no track yet");

        let mut gone = playing_with_track();
        gone.running = false;
        assert!(!gone.is_displayable(), "player gone");

        let mut stopped = playing_with_track();
        stopped.player_state = PlayerState::Stopped;
        assert!(!stopped.is_displayable(), "stopped");

        // Paused is still a candidate; hiding it is a separate decision.
        let mut paused = playing_with_track();
        paused.player_state = PlayerState::Paused;
        assert!(paused.is_displayable());
    }

    #[test]
    fn clearing_the_catalog_also_clears_the_request_guard() {
        // Leaving the guard behind would tell the next track it had already
        // been asked about, costing it its artwork for the whole song.
        let mut state = playing_with_track();
        state.catalog = Some(CatalogInfo::default());
        state.catalog_requested_for = Some("Song\u{1F}Artist\u{1F}Album".into());
        state.catalog_retry_at = Some(Instant::now());

        state.clear_catalog();

        assert!(state.catalog.is_none());
        assert!(state.catalog_requested_for.is_none());
        assert!(state.catalog_retry_at.is_none());
    }
}
