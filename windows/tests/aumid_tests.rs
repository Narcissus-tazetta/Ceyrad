use ceyrad::core::aumid::{is_apple_music, AumidOverrides};

fn none() -> AumidOverrides {
    AumidOverrides::default()
}

#[test]
fn packaged_apple_music_matches_on_family_prefix() {
    assert!(is_apple_music(
        "AppleInc.AppleMusicWin_nzyj5cx40ttqa!App",
        &none()
    ));
}

#[test]
fn desktop_builds_match_on_executable_name() {
    assert!(is_apple_music("iTunes.exe", &none()));
}

#[test]
fn matching_is_case_insensitive() {
    assert!(is_apple_music("ITUNES.EXE", &none()));
}

#[test]
fn unrelated_sessions_are_ignored() {
    // The opaque id a browser reports, seen on a real machine while a video
    // was playing. Nothing about it may be allowed to look like a music app.
    assert!(!is_apple_music("F0DC299D809B9700", &none()));
    assert!(!is_apple_music("chrome.exe", &none()));
    assert!(!is_apple_music("msedge.exe", &none()));
    assert!(!is_apple_music("firefox.exe", &none()));
    assert!(!is_apple_music("vlc.exe", &none()));
    assert!(!is_apple_music("spotify.exe", &none()));
    assert!(!is_apple_music("", &none()));
    assert!(!is_apple_music("   ", &none()));
}

/// A name generic enough to belong to some other player must not be claimed:
/// showing the wrong thing on someone's profile is worse than showing nothing.
#[test]
fn generic_names_are_not_claimed() {
    for id in ["music.exe", "player.exe", "media.exe", "Music"] {
        assert!(!is_apple_music(id, &none()), "{id} should not match");
    }
}

#[test]
fn an_override_adds_an_unknown_id() {
    let overrides = AumidOverrides {
        apple_music: vec!["F0DC299D809B9700".into()],
    };
    assert!(is_apple_music("F0DC299D809B9700", &overrides));
}

#[test]
fn an_override_may_be_a_prefix() {
    let overrides = AumidOverrides {
        apple_music: vec!["MyCorp.Player".into()],
    };
    assert!(is_apple_music("MyCorp.Player_abc123!App", &overrides));
}

/// The prefix test compares bytes rather than folding a lowercase copy of
/// every candidate. For UTF-8 that is the same question — but only if a prefix
/// can never end part-way through a character, which is what these pin.
#[test]
fn a_non_ascii_override_prefix_matches_whole_characters_only() {
    let overrides = AumidOverrides {
        apple_music: vec!["日本語".into()],
    };
    assert!(is_apple_music("日本語版.exe", &overrides));
    // Shares its first two characters — six of the nine bytes — with the
    // pattern, and must still be refused.
    assert!(!is_apple_music("日本人.exe", &overrides));
    // Shorter than the pattern: there is no prefix to compare at all.
    assert!(!is_apple_music("日本", &overrides));
}

#[test]
fn an_empty_override_entry_matches_nothing() {
    let overrides = AumidOverrides {
        apple_music: vec![String::new(), "  ".into()],
    };
    assert!(!is_apple_music("anything.exe", &overrides));
}
