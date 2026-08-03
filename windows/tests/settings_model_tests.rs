//! Port of Tests/CeyradTests/SettingsStoreTests.swift

use ceyrad::core::i18n::AppLanguage;
use ceyrad::core::settings_model::{
    BadgeLabelType, ButtonSlot, LinkType, Settings, DEFAULT_REPOSITORY_URL, PAUSE_HIDE_CHOICES,
};

#[test]
fn defaults() {
    let settings = Settings::default();
    assert_eq!(settings.button_type(ButtonSlot::One), LinkType::Song);
    assert_eq!(
        settings.button_label(ButtonSlot::One),
        "Play on Apple Music"
    );
    assert_eq!(settings.button_type(ButtonSlot::Two), LinkType::Repository);
    assert_eq!(settings.button_label(ButtonSlot::Two), "About This App");
    assert_eq!(settings.pause_hide_minutes, 5);
    assert_eq!(settings.repository_url, DEFAULT_REPOSITORY_URL);
}

#[test]
fn label_follows_type_change_when_not_customized() {
    let mut settings = Settings::default();
    settings.set_button_type(ButtonSlot::One, LinkType::Artist);
    assert_eq!(settings.button_label(ButtonSlot::One), "View Artist");
}

#[test]
fn custom_label_survives_type_change() {
    let mut settings = Settings::default();
    settings.set_button_label(ButtonSlot::One, "My Label");
    settings.set_button_type(ButtonSlot::One, LinkType::Artist);
    assert_eq!(settings.button_label(ButtonSlot::One), "My Label");
}

#[test]
fn setting_label_to_default_resumes_following() {
    let mut settings = Settings::default();
    settings.set_button_label(ButtonSlot::One, "My Label");
    // Writing the current default back drops the customization, so the label
    // follows the link type again.
    let default_label = settings.button_type(ButtonSlot::One).default_label();
    settings.set_button_label(ButtonSlot::One, default_label);
    settings.set_button_type(ButtonSlot::One, LinkType::Album);
    assert_eq!(settings.button_label(ButtonSlot::One), "View Album");
}

#[test]
fn empty_label_resumes_following() {
    let mut settings = Settings::default();
    settings.set_button_label(ButtonSlot::One, "My Label");
    settings.set_button_label(ButtonSlot::One, "");
    assert_eq!(
        settings.button_label(ButtonSlot::One),
        settings.button_type(ButtonSlot::One).default_label()
    );
}

#[test]
fn label_is_truncated_to_32_characters() {
    let mut settings = Settings::default();
    settings.set_button_label(ButtonSlot::One, &"x".repeat(64));
    assert_eq!(settings.button_label(ButtonSlot::One).chars().count(), 32);
}

/// The two slots have to behave identically. A rule that only reaches one of
/// them leaves the menu looking the same while the buttons disagree.
#[test]
fn both_slots_behave_identically() {
    let mut settings = Settings::default();
    for slot in ButtonSlot::ALL {
        settings.set_button_type(slot, LinkType::Artist);
        assert_eq!(settings.button_type(slot), LinkType::Artist);
        assert_eq!(settings.button_label(slot), "View Artist");

        settings.set_button_label(slot, "Mine");
        assert_eq!(settings.button_label(slot), "Mine");
    }
    // A write to one slot must not leak into the other.
    settings.set_button_type(ButtonSlot::One, LinkType::Album);
    assert_eq!(settings.button_type(ButtonSlot::Two), LinkType::Artist);
}

/// Labels go to Discord, so they stay English whatever the menu is set to.
#[test]
fn button_labels_are_never_localized() {
    let mut settings = Settings::default();
    settings.language = AppLanguage::Ja;
    assert_eq!(
        settings.button_label(ButtonSlot::One),
        "Play on Apple Music"
    );
}

#[test]
fn round_trips_through_json_with_defaults_for_missing_keys() {
    let mut settings = Settings::default();
    settings.set_button_label(ButtonSlot::One, "My Label");
    let encoded = serde_json::to_string(&settings).expect("serialize");
    let decoded: Settings = serde_json::from_str(&encoded).expect("deserialize");
    assert_eq!(decoded.button_label(ButtonSlot::One), "My Label");

    // An empty file yields the same defaults as a fresh install.
    let empty: Settings = serde_json::from_str("{}").expect("deserialize empty");
    assert_eq!(empty.button_type(ButtonSlot::Two), LinkType::Repository);
    assert_eq!(empty.pause_hide_minutes, 5);
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

/// Writing a type's own default label back must never read as a customization,
/// or the label would stop following the destination for good.
#[test]
fn writing_a_types_own_default_is_not_a_customization() {
    for candidate in LinkType::SELECTABLE {
        let mut settings = Settings::default();
        settings.set_button_type(ButtonSlot::One, candidate);
        settings.set_button_label(ButtonSlot::One, candidate.default_label());
        settings.set_button_type(ButtonSlot::One, LinkType::Album);
        assert_eq!(
            settings.button_label(ButtonSlot::One),
            "View Album",
            "{candidate:?} treated its own default as a customization"
        );
    }
}

/// Every link type needs a distinct default label, since the label is what
/// tells a customization from a leftover.
#[test]
fn enabled_link_types_have_distinct_default_labels() {
    let mut seen: Vec<&str> = Vec::new();
    for candidate in LinkType::SELECTABLE {
        if candidate == LinkType::Disabled {
            continue;
        }
        let label = candidate.default_label();
        assert!(!label.is_empty(), "{candidate:?} has no default label");
        assert!(!seen.contains(&label), "{candidate:?} reuses {label:?}");
        seen.push(label);
    }
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
