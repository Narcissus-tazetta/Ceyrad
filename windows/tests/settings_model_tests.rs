//! Port of Tests/CeyradTests/SettingsStoreTests.swift

use ceyrad::core::i18n::AppLanguage;
use ceyrad::core::models::MusicSourceId;
use ceyrad::core::settings_model::{
    BadgeLabelType, LinkType, Settings, DEFAULT_REPOSITORY_URL, PAUSE_HIDE_CHOICES,
};

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

// MARK: - The menu's data
//
// None of this has a caller until the tray UI exists, which is exactly why it
// is worth asserting: the failure mode is adding a variant and forgetting the
// list or the label, and nothing else would catch that.

#[test]
fn every_link_type_is_offered_in_the_menu() {
    // `Disabled` is in the list too: it is presented as a way to turn the
    // button off, not as a hidden state.
    assert_eq!(LinkType::SELECTABLE.len(), 6);
    for candidate in [
        LinkType::Song,
        LinkType::Artist,
        LinkType::Album,
        LinkType::Custom,
        LinkType::Repository,
        LinkType::Disabled,
    ] {
        assert!(
            LinkType::SELECTABLE.contains(&candidate),
            "{candidate:?} is missing from the menu"
        );
    }
}

#[test]
fn every_link_type_is_named_in_both_languages() {
    for candidate in LinkType::SELECTABLE {
        let en = candidate.display_name(AppLanguage::En);
        let ja = candidate.display_name(AppLanguage::Ja);
        assert!(!en.is_empty(), "{candidate:?} has no English name");
        assert!(!ja.is_empty(), "{candidate:?} has no Japanese name");
        assert_ne!(en, ja, "{candidate:?} was never translated");
    }
}

#[test]
fn every_link_type_declares_its_own_default_label_as_a_default() {
    // `set_type` relies on this to tell "the user picked this" apart from
    // "this is just what the previous type suggested".
    for candidate in LinkType::SELECTABLE {
        let default = candidate.default_label(None);
        assert!(
            candidate.default_labels().contains(&default),
            "{candidate:?} would treat its own default as a customization"
        );
    }
    // The song label varies by source, so both spellings have to count.
    assert!(LinkType::Song
        .default_labels()
        .contains(&LinkType::Song.default_label(Some(MusicSourceId::Spotify))));
}

#[test]
fn badge_label_raw_values_match_discords_status_display_type() {
    assert_eq!(BadgeLabelType::ALL.len(), 3);
    assert_eq!(BadgeLabelType::AppName.raw_value(), 0);
    assert_eq!(BadgeLabelType::Artist.raw_value(), 1);
    assert_eq!(BadgeLabelType::Track.raw_value(), 2);
}

#[test]
fn every_badge_label_is_named_in_both_languages() {
    for candidate in BadgeLabelType::ALL {
        assert_ne!(
            candidate.display_name(AppLanguage::En),
            candidate.display_name(AppLanguage::Ja),
            "{candidate:?} was never translated"
        );
    }
}

#[test]
fn pause_hide_choices_cover_the_two_special_cases_and_are_ordered() {
    // -1 never hides and 0 hides immediately; `should_show_activity` reads
    // both, so neither may drop out of the menu.
    assert!(PAUSE_HIDE_CHOICES.contains(&-1));
    assert!(PAUSE_HIDE_CHOICES.contains(&0));
    assert!(PAUSE_HIDE_CHOICES.contains(&Settings::default().pause_hide_minutes));
    assert!(PAUSE_HIDE_CHOICES.windows(2).all(|w| w[0] < w[1]));
}

#[test]
fn every_language_has_a_name_of_its_own() {
    assert_eq!(AppLanguage::ALL.len(), 2);
    assert_eq!(AppLanguage::En.display_name(), "English");
    assert_eq!(AppLanguage::Ja.display_name(), "日本語");
}
