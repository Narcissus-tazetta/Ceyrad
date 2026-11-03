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
//!
//! Browsers are recognised the same way, for YouTube Music. A browser's AUMID is
//! far less predictable than a music app's: one has been seen reporting an
//! opaque hex id (see `tests/aumid_tests.rs`), so only the Chromium-family names
//! below are built in and every other browser comes in through an override.

/// Package-family prefixes, compared case-insensitively.
const APPLE_MUSIC_PACKAGE_PREFIXES: [&str; 2] = ["appleinc.applemusicwin", "appleinc.itunes"];

/// Executable names reported by the non-Store builds. Deliberately specific:
/// a generic name like `music.exe` would hand some unrelated player's session
/// to Discord as Apple Music, and a wrong presence is worse than a missing one.
const APPLE_MUSIC_EXECUTABLES: [&str; 2] = ["applemusic.exe", "itunes.exe"];

/// Prefixes of the app-model id Chromium derives for Chrome ("Chrome",
/// "Chromium", and the channel and per-user variants that extend them): the
/// browser model id is the install mode's base id plus a per-user suffix, in
/// `ShellUtil::GetBrowserModelId` and `install_static`.
///
/// Edge, Brave, Vivaldi and Opera build on the same install_static code with
/// their own base ids. Firefox is absent on purpose: its id is an opaque hash of
/// the install path (as in the `F0DC299D809B9700` case in the tests), so it can
/// only be matched through `AumidOverrides::browsers`.
///
/// **Unverified on a real machine:** that any of these browsers' SMTC sessions
/// report these ids rather than something else has not been observed.
const BROWSER_PACKAGE_PREFIXES: [&str; 6] =
    ["chrome", "chromium", "msedge", "brave", "vivaldi", "opera"];

/// Extra AUMIDs supplied at runtime, so an unrecognised install can be made to
/// work without a rebuild.
#[derive(Debug, Clone, Default)]
pub struct AumidOverrides {
    pub apple_music: Vec<String>,
    pub browsers: Vec<String>,
}

impl AumidOverrides {
    pub fn is_empty(&self) -> bool {
        self.apple_music.is_empty() && self.browsers.is_empty()
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

/// Whether an SMTC session belongs to a web browser. Only asked once YouTube
/// Music detection is switched on, so the common case of it being off never
/// pays for it.
pub fn is_browser(aumid: &str, overrides: &AumidOverrides) -> bool {
    let id = aumid.trim();
    if id.is_empty() {
        return false;
    }

    matches_any(id, &overrides.browsers)
        || BROWSER_PACKAGE_PREFIXES
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
