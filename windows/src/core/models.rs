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
    pub const ALL: [MusicSourceId; 2] = [MusicSourceId::AppleMusic, MusicSourceId::Spotify];

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
}
