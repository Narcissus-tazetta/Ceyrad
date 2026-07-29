//! Mapping SMTC's `SourceAppUserModelId` onto a music source.
//!
//! Unlike macOS, where a bundle id is a stable, documented constant, the AUMID
//! a Windows player reports depends on how it was installed: the Store build of
//! a player reports a packaged id (`Publisher.Package_hash!App`), a downloaded
//! build usually reports the executable name. Matching is therefore done on the
//! packaged-family prefix or the executable name, never on the full packaged id
//! — the hash in the middle is per-publisher and the `!App` suffix varies.
//!
//! Anything unrecognised is reported to the caller so it can be logged: that is
//! how a user on an install we have not seen finds out what to add.

use super::models::MusicSourceId;

/// Package-family prefixes, compared case-insensitively.
const APPLE_MUSIC_PACKAGE_PREFIXES: [&str; 2] = ["appleinc.applemusicwin", "appleinc.itunes"];
const SPOTIFY_PACKAGE_PREFIXES: [&str; 1] = ["spotifyab.spotifymusic"];

/// Executable names reported by the non-Store builds. Deliberately specific:
/// a generic name like `music.exe` would hand some unrelated player's session
/// to Discord as Apple Music, and a wrong presence is worse than a missing one.
const APPLE_MUSIC_EXECUTABLES: [&str; 2] = ["applemusic.exe", "itunes.exe"];
const SPOTIFY_EXECUTABLES: [&str; 1] = ["spotify.exe"];

/// Extra AUMIDs supplied at runtime, so an unrecognised install can be made to
/// work without a rebuild.
#[derive(Debug, Clone, Default)]
pub struct AumidOverrides {
    pub apple_music: Vec<String>,
    pub spotify: Vec<String>,
}

impl AumidOverrides {
    pub fn is_empty(&self) -> bool {
        self.apple_music.is_empty() && self.spotify.is_empty()
    }
}

pub fn source_for_aumid(aumid: &str, overrides: &AumidOverrides) -> Option<MusicSourceId> {
    let id = aumid.trim().to_ascii_lowercase();
    if id.is_empty() {
        return None;
    }

    // Overrides win, so a user can redirect an id we would otherwise misread.
    if matches_any(&id, &overrides.apple_music) {
        return Some(MusicSourceId::AppleMusic);
    }
    if matches_any(&id, &overrides.spotify) {
        return Some(MusicSourceId::Spotify);
    }

    if APPLE_MUSIC_EXECUTABLES.contains(&id.as_str())
        || APPLE_MUSIC_PACKAGE_PREFIXES
            .iter()
            .any(|prefix| id.starts_with(prefix))
    {
        return Some(MusicSourceId::AppleMusic);
    }
    if SPOTIFY_EXECUTABLES.contains(&id.as_str())
        || SPOTIFY_PACKAGE_PREFIXES
            .iter()
            .any(|prefix| id.starts_with(prefix))
    {
        return Some(MusicSourceId::Spotify);
    }
    None
}

/// An override matches either exactly or as a prefix, so a user can paste
/// either the whole AUMID from the probe or just the package family.
fn matches_any(id: &str, patterns: &[String]) -> bool {
    patterns.iter().any(|pattern| {
        let pattern = pattern.trim().to_ascii_lowercase();
        !pattern.is_empty() && (id == pattern || id.starts_with(&pattern))
    })
}
