//! Port of Tests/CeyradTests/SettingsStoreTests.swift

use ceyrad::core::models::MusicSourceId;
use ceyrad::core::settings_model::{LinkType, Settings, DEFAULT_REPOSITORY_URL};

#[test]
fn defaults() {
    let settings = Settings::default();
    assert_eq!(settings.button1_type, LinkType::Song);
    assert_eq!(settings.button1_label(None), "Play on Apple Music");
    assert_eq!(settings.button2_type, LinkType::Repository);
    assert_eq!(settings.button2_label(None), "About This App");
    assert_eq!(settings.pause_hide_minutes, 5);
    assert_eq!(settings.repository_url, DEFAULT_REPOSITORY_URL);
}

#[test]
fn label_follows_type_change_when_not_customized() {
    let mut settings = Settings::default();
    settings.set_button1_type(LinkType::Artist);
    assert_eq!(settings.button1_label(None), "View Artist");
}

#[test]
fn custom_label_survives_type_change() {
    let mut settings = Settings::default();
    settings.set_button1_label("My Label");
    settings.set_button1_type(LinkType::Artist);
    assert_eq!(settings.button1_label(None), "My Label");
}

#[test]
fn setting_label_to_default_resumes_following() {
    let mut settings = Settings::default();
    settings.set_button1_label("My Label");
    // Writing the current default back drops the customization, so the label
    // follows the link type again.
    let default_label = settings.button1_type.default_label(None);
    settings.set_button1_label(default_label);
    settings.set_button1_type(LinkType::Album);
    assert_eq!(settings.button1_label(None), "View Album");
}

#[test]
fn empty_label_resumes_following() {
    let mut settings = Settings::default();
    settings.set_button1_label("My Label");
    settings.set_button1_label("");
    assert_eq!(
        settings.button1_label(None),
        settings.button1_type.default_label(None)
    );
}

#[test]
fn label_is_truncated_to_32_characters() {
    let mut settings = Settings::default();
    settings.set_button1_label(&"x".repeat(64));
    assert_eq!(settings.button1_label(None).chars().count(), 32);
}

#[test]
fn song_label_follows_source() {
    let mut settings = Settings::default();
    assert_eq!(
        settings.button1_label(Some(MusicSourceId::AppleMusic)),
        "Play on Apple Music"
    );
    assert_eq!(
        settings.button1_label(Some(MusicSourceId::Spotify)),
        "Play on Spotify"
    );
    // A custom label wins regardless of source.
    settings.set_button1_label("My Label");
    assert_eq!(
        settings.button1_label(Some(MusicSourceId::Spotify)),
        "My Label"
    );
}

#[test]
fn spotify_default_label_is_not_treated_as_custom() {
    let mut settings = Settings::default();
    settings.set_button1_label("Play on Spotify");
    settings.set_button1_type(LinkType::Artist);
    assert_eq!(settings.button1_label(None), "View Artist");
}

#[test]
fn spotify_default_label_is_cleared_on_type_change() {
    // A stored "Play on Spotify" is still a default for the song type, so
    // changing the link type cleans it up rather than keeping it as a custom label.
    let mut settings: Settings =
        serde_json::from_str(r#"{"button1_label":"Play on Spotify"}"#).expect("deserialize");
    settings.set_button1_type(LinkType::Album);
    assert_eq!(settings.button1_label(None), "View Album");
}

#[test]
fn sources_enabled_by_default() {
    // Spotify is off by default: Discord ships its own Spotify integration.
    let settings = Settings::default();
    assert!(settings.is_source_enabled(MusicSourceId::AppleMusic));
    assert!(!settings.is_source_enabled(MusicSourceId::Spotify));
}

#[test]
fn source_toggle_is_persisted_per_source() {
    let mut settings = Settings::default();
    settings.set_source_enabled(MusicSourceId::Spotify, true);
    assert!(settings.is_source_enabled(MusicSourceId::Spotify));
    assert!(settings.is_source_enabled(MusicSourceId::AppleMusic));
    settings.set_source_enabled(MusicSourceId::Spotify, false);
    assert!(!settings.is_source_enabled(MusicSourceId::Spotify));
}

#[test]
fn round_trips_through_json_with_defaults_for_missing_keys() {
    let mut settings = Settings::default();
    settings.set_button1_label("My Label");
    settings.set_source_enabled(MusicSourceId::Spotify, true);
    let encoded = serde_json::to_string(&settings).expect("serialize");
    let decoded: Settings = serde_json::from_str(&encoded).expect("deserialize");
    assert_eq!(decoded.button1_label(None), "My Label");
    assert!(decoded.is_source_enabled(MusicSourceId::Spotify));

    // An empty file yields the same defaults as a fresh install.
    let empty: Settings = serde_json::from_str("{}").expect("deserialize empty");
    assert_eq!(empty.button2_type, LinkType::Repository);
    assert_eq!(empty.pause_hide_minutes, 5);
    assert!(empty.is_source_enabled(MusicSourceId::AppleMusic));
    assert!(!empty.is_source_enabled(MusicSourceId::Spotify));
}
