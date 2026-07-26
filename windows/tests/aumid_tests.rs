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
fn packaged_spotify_matches_on_family_prefix() {
    assert_eq!(
        source_for_aumid("SpotifyAB.SpotifyMusic_zpdnekdrzrea0!Spotify", &none()),
        Some(MusicSourceId::Spotify)
    );
}

#[test]
fn desktop_builds_match_on_executable_name() {
    assert_eq!(
        source_for_aumid("Spotify.exe", &none()),
        Some(MusicSourceId::Spotify)
    );
    assert_eq!(
        source_for_aumid("iTunes.exe", &none()),
        Some(MusicSourceId::AppleMusic)
    );
}

#[test]
fn matching_is_case_insensitive() {
    assert_eq!(
        source_for_aumid("sPoTiFy.ExE", &none()),
        Some(MusicSourceId::Spotify)
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
        spotify: Vec::new(),
    };
    assert_eq!(
        source_for_aumid("F0DC299D809B9700", &overrides),
        Some(MusicSourceId::AppleMusic)
    );
}

#[test]
fn an_override_may_be_a_prefix() {
    let overrides = AumidOverrides {
        apple_music: Vec::new(),
        spotify: vec!["MyCorp.Player".into()],
    };
    assert_eq!(
        source_for_aumid("MyCorp.Player_abc123!App", &overrides),
        Some(MusicSourceId::Spotify)
    );
}

#[test]
fn an_override_wins_over_a_built_in_match() {
    let overrides = AumidOverrides {
        apple_music: vec!["Spotify.exe".into()],
        spotify: Vec::new(),
    };
    assert_eq!(
        source_for_aumid("Spotify.exe", &overrides),
        Some(MusicSourceId::AppleMusic)
    );
}

#[test]
fn an_empty_override_entry_matches_nothing() {
    let overrides = AumidOverrides {
        apple_music: vec![String::new(), "  ".into()],
        spotify: Vec::new(),
    };
    assert_eq!(source_for_aumid("anything.exe", &overrides), None);
}
