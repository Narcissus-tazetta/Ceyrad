use ceyrad::core::aumid::{source_for_aumid, AumidOverrides};
use ceyrad::core::models::MusicSourceId;

fn none() -> AumidOverrides {
    AumidOverrides::default()
}

#[test]
fn packaged_apple_music_matches_on_family_prefix() {
    assert_eq!(
        source_for_aumid("AppleInc.AppleMusicWin_nzyj5cx40ttqa!App", &none()),
        Some(MusicSourceId::AppleMusic)
    );
}

#[test]
fn desktop_builds_match_on_executable_name() {
    assert_eq!(
        source_for_aumid("iTunes.exe", &none()),
        Some(MusicSourceId::AppleMusic)
    );
}

#[test]
fn matching_is_case_insensitive() {
    assert_eq!(
        source_for_aumid("ITUNES.EXE", &none()),
        Some(MusicSourceId::AppleMusic)
    );
}

#[test]
fn unrelated_sessions_are_ignored() {
    // The opaque id a browser reports, seen on a real machine while a video
    // was playing. Nothing about it may be allowed to look like a music app.
    assert_eq!(source_for_aumid("F0DC299D809B9700", &none()), None);
    assert_eq!(source_for_aumid("chrome.exe", &none()), None);
    assert_eq!(source_for_aumid("msedge.exe", &none()), None);
    assert_eq!(source_for_aumid("firefox.exe", &none()), None);
    assert_eq!(source_for_aumid("vlc.exe", &none()), None);
    assert_eq!(source_for_aumid("spotify.exe", &none()), None);
    assert_eq!(source_for_aumid("", &none()), None);
    assert_eq!(source_for_aumid("   ", &none()), None);
}

/// A name generic enough to belong to some other player must not be claimed:
/// showing the wrong thing on someone's profile is worse than showing nothing.
#[test]
fn generic_names_are_not_claimed() {
    for id in ["music.exe", "player.exe", "media.exe", "Music"] {
        assert_eq!(source_for_aumid(id, &none()), None, "{id} should not match");
    }
}

#[test]
fn an_override_adds_an_unknown_id() {
    let overrides = AumidOverrides {
        apple_music: vec!["F0DC299D809B9700".into()],
    };
    assert_eq!(
        source_for_aumid("F0DC299D809B9700", &overrides),
        Some(MusicSourceId::AppleMusic)
    );
}

#[test]
fn an_override_may_be_a_prefix() {
    let overrides = AumidOverrides {
        apple_music: vec!["MyCorp.Player".into()],
    };
    assert_eq!(
        source_for_aumid("MyCorp.Player_abc123!App", &overrides),
        Some(MusicSourceId::AppleMusic)
    );
}

/// The prefix test compares bytes rather than folding a lowercase copy of
/// every candidate. For UTF-8 that is the same question — but only if a prefix
/// can never end part-way through a character, which is what these pin.
#[test]
fn a_non_ascii_override_prefix_matches_whole_characters_only() {
    let overrides = AumidOverrides {
        apple_music: vec!["日本語".into()],
    };
    assert_eq!(
        source_for_aumid("日本語版.exe", &overrides),
        Some(MusicSourceId::AppleMusic)
    );
    // Shares its first two characters — six of the nine bytes — with the
    // pattern, and must still be refused.
    assert_eq!(source_for_aumid("日本人.exe", &overrides), None);
    // Shorter than the pattern: there is no prefix to compare at all.
    assert_eq!(source_for_aumid("日本", &overrides), None);
}

#[test]
fn an_empty_override_entry_matches_nothing() {
    let overrides = AumidOverrides {
        apple_music: vec![String::new(), "  ".into()],
    };
    assert_eq!(source_for_aumid("anything.exe", &overrides), None);
}
