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
pub enum MusicSourceId {
    AppleMusic,
    Spotify,
}

impl MusicSourceId {
    pub const COUNT: usize = 2;
    pub const ALL: [MusicSourceId; Self::COUNT] =
        [MusicSourceId::AppleMusic, MusicSourceId::Spotify];

    /// Position in `ALL`, so per-source state can live in a fixed-size array
    /// instead of a map. Kept in step with `ALL` by the test below.
    pub const fn index(self) -> usize {
        match self {
            MusicSourceId::AppleMusic => 0,
            MusicSourceId::Spotify => 1,
        }
    }

    pub fn display_name(self) -> &'static str {
        match self {
            MusicSourceId::AppleMusic => "Apple Music",
            MusicSourceId::Spotify => "Spotify",
        }
    }

    /// Discord shows "Listening to <Application name>", and the name belongs to
    /// the Application the client id identifies — hence one id per source.
    pub fn discord_client_id(self) -> &'static str {
        match self {
            MusicSourceId::AppleMusic => "1525381518258606130",
            MusicSourceId::Spotify => "1526238417845751959",
        }
    }
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
    /// Spotify's `spotify:track:xxx`. `None` for Apple Music.
    pub track_id: Option<String>,
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
            track_id: None,
        }
    }

    pub fn identity(&self) -> String {
        match &self.track_id {
            Some(id) => id.clone(),
            None => format!(
                "{}{}{}{}{}",
                self.name, UNIT_SEPARATOR, self.artist, UNIT_SEPARATOR, self.album
            ),
        }
    }

    /// Whether `identity` would have produced exactly `candidate`.
    ///
    /// `self.identity() == candidate` without the `String` that gets built and
    /// thrown away to answer it. Asked on every SMTC event — several times a
    /// second — for any track the catalog had no answer for, which is every
    /// locally imported one. The test below pins it to `identity` itself.
    pub fn matches_identity(&self, candidate: &str) -> bool {
        match &self.track_id {
            Some(id) => id == candidate,
            None => {
                // Walks the join rather than splitting on the separator.
                // Splitting looks equivalent and is not: a field is free to
                // contain a separator of its own — nothing stops a title — and
                // the split would then hand the pieces back in the wrong
                // places. Consuming each field by its own length cannot.
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
        }
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
    /// the artist, the album and the track id, and this covers everything it
    /// does not. **A field added to this type belongs in one of those two
    /// places**, or it will silently stop being updated.
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

#[derive(Debug, Clone)]
pub struct SourceState {
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
    /// Monotonic timestamp of the last event, used to break ties when both
    /// sources are playing.
    pub last_event_uptime_ns: u64,
}

impl Default for SourceState {
    fn default() -> Self {
        Self {
            running: false,
            player_state: PlayerState::Stopped,
            track: None,
            catalog: None,
            catalog_requested_for: None,
            catalog_retry_at: None,
            last_event_uptime_ns: 0,
        }
    }
}

#[derive(Debug, Clone, Default)]
pub struct SourceStates {
    pub apple_music: SourceState,
    pub spotify: SourceState,
}

impl SourceStates {
    pub fn get(&self, id: MusicSourceId) -> &SourceState {
        match id {
            MusicSourceId::AppleMusic => &self.apple_music,
            MusicSourceId::Spotify => &self.spotify,
        }
    }

    pub fn get_mut(&mut self, id: MusicSourceId) -> &mut SourceState {
        match id {
            MusicSourceId::AppleMusic => &mut self.apple_music,
            MusicSourceId::Spotify => &mut self.spotify,
        }
    }

    pub fn any_running(&self) -> bool {
        self.apple_music.running || self.spotify.running
    }
}

#[cfg(test)]
mod tests {
    use std::time::Duration;

    use super::*;

    #[test]
    fn a_track_id_is_the_identity_on_its_own() {
        // Spotify supplies a stable id; when it is there, the display strings
        // must not participate — a re-tagged title is still the same track.
        let mut track = TrackInfo::new("Song", "Artist", "Album");
        track.track_id = Some("spotify:track:abc".into());
        assert_eq!(track.identity(), "spotify:track:abc");
    }

    #[test]
    fn without_a_track_id_the_fields_are_joined_by_a_unit_separator() {
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
    fn every_source_indexes_its_own_slot_in_all() {
        // `index` is what lets per-source state live in a `[T; COUNT]`; a
        // mapping that drifted from `ALL` would hand one source another's.
        for (position, source) in MusicSourceId::ALL.into_iter().enumerate() {
            assert_eq!(source.index(), position, "{source:?}");
        }
        assert_eq!(MusicSourceId::ALL.len(), MusicSourceId::COUNT);
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
}
