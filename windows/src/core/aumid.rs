//! Deciding whether an SMTC `SourceAppUserModelId` belongs to Apple Music.
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

/// Package-family prefixes, compared case-insensitively.
const APPLE_MUSIC_PACKAGE_PREFIXES: [&str; 2] = ["appleinc.applemusicwin", "appleinc.itunes"];

/// Executable names reported by the non-Store builds. Deliberately specific:
/// a generic name like `music.exe` would hand some unrelated player's session
/// to Discord as Apple Music, and a wrong presence is worse than a missing one.
const APPLE_MUSIC_EXECUTABLES: [&str; 2] = ["applemusic.exe", "itunes.exe"];

/// Extra AUMIDs supplied at runtime, so an unrecognised install can be made to
/// work without a rebuild.
#[derive(Debug, Clone, Default)]
pub struct AumidOverrides {
    pub apple_music: Vec<String>,
}

impl AumidOverrides {
    pub fn is_empty(&self) -> bool {
        self.apple_music.is_empty()
    }
}

/// Everything here compares in place rather than folding a lowercase copy
/// first. This is asked once per media session on the machine every time the
/// session list moves — browsers mint one per profile and per tab — and an
/// allocation per candidate to answer "no" is the wrong shape for a question
/// whose answer is almost always no.
pub fn is_apple_music(aumid: &str, overrides: &AumidOverrides) -> bool {
    let id = aumid.trim();
    if id.is_empty() {
        return false;
    }

    // Overrides win, so a user can redirect an id we would otherwise misread.
    matches_any(id, &overrides.apple_music)
        || APPLE_MUSIC_EXECUTABLES
            .iter()
            .any(|executable| id.eq_ignore_ascii_case(executable))
        || APPLE_MUSIC_PACKAGE_PREFIXES
            .iter()
            .any(|prefix| starts_with_ignore_ascii_case(id, prefix))
}

/// An override matches either exactly or as a prefix, so a user can paste
/// either the whole AUMID from the probe or just the package family.
fn matches_any(id: &str, patterns: &[String]) -> bool {
    patterns.iter().any(|pattern| {
        let pattern = pattern.trim();
        !pattern.is_empty()
            && (id.eq_ignore_ascii_case(pattern) || starts_with_ignore_ascii_case(id, pattern))
    })
}

/// `str::starts_with` that ignores ASCII case, without building a folded copy.
///
/// Compared as bytes, which for UTF-8 is the same question: a byte prefix can
/// only end on a character boundary if the characters themselves matched, so
/// this never claims a match that a character-wise comparison would refuse.
/// Only ASCII case is folded, exactly as the `to_ascii_lowercase` this replaced
/// did — a Turkish dotless i in an AUMID stays what it is.
fn starts_with_ignore_ascii_case(id: &str, prefix: &str) -> bool {
    id.as_bytes()
        .get(..prefix.len())
        .is_some_and(|head| head.eq_ignore_ascii_case(prefix.as_bytes()))
}
