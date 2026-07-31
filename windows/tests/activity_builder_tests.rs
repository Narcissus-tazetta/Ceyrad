//! Port of Tests/CeyradTests/ActivityBuilderTests.swift

use std::time::{Duration, SystemTime};

use serde_json::{Map, Value};

use ceyrad::core::activity_builder::{build, is_valid_button_url};
use ceyrad::core::models::{CatalogInfo, MusicSourceId, PlayerState, TrackInfo};
use ceyrad::core::settings_model::{BadgeLabelType, LinkType, Settings, DEFAULT_REPOSITORY_URL};

type Activity = Map<String, Value>;

fn track() -> TrackInfo {
    let mut t = TrackInfo::new("Song", "Artist", "Album");
    t.duration_sec = Some(200.0);
    t.position_sec = Some(50.0);
    t
}

fn track_with(name: &str, artist: &str, position: Option<f64>, duration: Option<f64>) -> TrackInfo {
    let mut t = TrackInfo::new(name, artist, "Album");
    t.duration_sec = duration;
    t.position_sec = position;
    t
}

fn catalog() -> CatalogInfo {
    CatalogInfo {
        song_url: Some("https://music.apple.com/song".into()),
        artist_url: None,
        album_url: None,
        artwork_url: Some("https://example.com/art.jpg".into()),
    }
}

fn build_now(
    track: &TrackInfo,
    state: PlayerState,
    catalog: Option<&CatalogInfo>,
    settings: &Settings,
) -> Activity {
    build(
        track,
        state,
        catalog,
        settings,
        MusicSourceId::AppleMusic,
        SystemTime::now(),
    )
}

fn string_field<'a>(activity: &'a Activity, key: &str) -> &'a str {
    activity[key]
        .as_str()
        .unwrap_or_else(|| panic!("{key} not a string"))
}

fn buttons(activity: &Activity) -> Option<&Vec<Value>> {
    activity.get("buttons").and_then(|b| b.as_array())
}

// MARK: - Basic fields

#[test]
fn listening_type_and_basic_fields() {
    let activity = build_now(&track(), PlayerState::Playing, None, &Settings::default());
    assert_eq!(activity["type"], 2);
    assert_eq!(string_field(&activity, "details"), "Song");
    assert_eq!(string_field(&activity, "state"), "Artist");
}

#[test]
fn short_strings_are_padded_to_two_characters() {
    let activity = build_now(
        &track_with("A", "", Some(50.0), Some(200.0)),
        PlayerState::Playing,
        None,
        &Settings::default(),
    );
    let details = string_field(&activity, "details");
    assert_eq!(details.chars().count(), 2);
    assert!(details.starts_with('A'));
    // Space padding would be trimmed by Discord, so it must not be used.
    assert!(!details.ends_with(' '));
}

#[test]
fn long_strings_are_clamped_to_128_characters() {
    let long = "あ".repeat(300);
    let activity = build_now(
        &track_with(&long, "Artist", Some(50.0), Some(200.0)),
        PlayerState::Playing,
        None,
        &Settings::default(),
    );
    assert_eq!(string_field(&activity, "details").chars().count(), 128);
}

#[test]
fn clamping_does_not_leave_a_dangling_joiner() {
    // A family emoji is seven scalars held together by zero-width joiners, so
    // a cut at a fixed scalar count lands inside one. What Discord would then
    // render is a lone figure followed by a stray joiner.
    let long = "\u{1F468}\u{200D}\u{1F469}\u{200D}\u{1F467}\u{200D}\u{1F466}".repeat(40);
    let activity = build_now(
        &track_with(&long, "Artist", Some(50.0), Some(200.0)),
        PlayerState::Playing,
        None,
        &Settings::default(),
    );
    let details = string_field(&activity, "details");
    assert!(details.chars().count() <= 128);
    assert!(!details.ends_with('\u{200D}'), "{details:?}");
}

#[test]
fn clamping_does_not_orphan_a_combining_mark() {
    // Same cut, spelled with combining marks rather than joiners: two on every
    // letter, so the 128th character is the first of a pair and the second is
    // on the far side of the cut. Half an accent is not the letter that was in
    // the title, so what is left of it goes.
    let long = "e\u{301}\u{300}".repeat(200);
    let activity = build_now(
        &track_with(&long, "Artist", Some(50.0), Some(200.0)),
        PlayerState::Playing,
        None,
        &Settings::default(),
    );
    let details = string_field(&activity, "details");
    assert!(details.chars().count() <= 128);
    assert!(details.ends_with('e'), "{details:?}");
}

#[test]
fn clamping_keeps_an_accent_that_fits_whole() {
    // The other side of the same rule: a mark whose letter is still there is a
    // character sitting on the limit, not a leftover, and dropping it would
    // spell the last word wrong for no reason.
    let long = "e\u{301}".repeat(200);
    let activity = build_now(
        &track_with(&long, "Artist", Some(50.0), Some(200.0)),
        PlayerState::Playing,
        None,
        &Settings::default(),
    );
    let details = string_field(&activity, "details");
    assert_eq!(details.chars().count(), 128);
    assert!(details.ends_with("e\u{301}"), "{details:?}");
}

// MARK: - Timestamps

#[test]
fn timestamps_are_clamped_once_the_position_has_run_past_the_end() {
    // A player reporting a too-short duration (live streams and some podcast
    // apps do) should still show a bar pinned at the end rather than none at
    // all — matching the macOS build, which clamps the same way.
    let mut t = track_with("Song", "Artist", Some(500.0), Some(200.0));
    t.position_sampled_at = SystemTime::now();
    let activity = build_now(&t, PlayerState::Playing, None, &Settings::default());
    let stamps = activity["timestamps"].as_object().expect("timestamps");
    let start = stamps["start"].as_i64().unwrap();
    let end = stamps["end"].as_i64().unwrap();
    // Clamped to the 200s duration rather than the (out of range) 500s position.
    assert_eq!(end - start, 200_000);
}

#[test]
fn timestamps_only_while_playing() {
    let settings = Settings::default();
    let playing = build_now(&track(), PlayerState::Playing, None, &settings);
    let stamps = playing["timestamps"].as_object().expect("timestamps");
    let start = stamps["start"].as_i64().unwrap();
    let end = stamps["end"].as_i64().unwrap();
    // 200s wide, in epoch milliseconds.
    assert_eq!(end - start, 200_000);

    let mut settings = Settings::default();
    settings.pause_hide_minutes = -1;
    let paused = build_now(&track(), PlayerState::Paused, None, &settings);
    assert!(paused.get("timestamps").is_none());
}

/// Regression: an unrelated re-send after the position was sampled (a button
/// label change, say) must not rewind the progress bar.
#[test]
fn stale_position_sample_is_corrected_for_elapsed_time() {
    let now = SystemTime::now();
    let mut stale = track();
    stale.position_sec = Some(50.0);
    stale.position_sampled_at = now - Duration::from_secs(10);

    let activity = build(
        &stale,
        PlayerState::Playing,
        None,
        &Settings::default(),
        MusicSourceId::AppleMusic,
        now,
    );
    let start = activity["timestamps"]["start"].as_i64().unwrap();

    let expected_position = 60.0; // 50s sample + 10s elapsed
    let now_unix = now
        .duration_since(SystemTime::UNIX_EPOCH)
        .unwrap()
        .as_secs_f64();
    let expected_start = ((now_unix - expected_position) * 1000.0).round() as i64;
    assert!(
        (start - expected_start).abs() < 500,
        "elapsed time was not added back into the position"
    );
}

#[test]
fn no_timestamps_without_position_or_duration() {
    let activity = build_now(
        &track_with("Song", "Artist", None, None),
        PlayerState::Playing,
        None,
        &Settings::default(),
    );
    assert!(activity.get("timestamps").is_none());
}

// MARK: - Paused display

#[test]
fn paused_label_goes_to_album_line_with_artwork() {
    let activity = build_now(
        &track_with("Song", "Artist", Some(168.0), Some(200.0)),
        PlayerState::Paused,
        Some(&catalog()),
        &Settings::default(),
    );
    let assets = activity["assets"].as_object().expect("assets");
    assert_eq!(
        assets["large_text"].as_str().unwrap(),
        "Album · ⏸ Paused at 2:48"
    );
    assert_eq!(string_field(&activity, "state"), "Artist");
}

#[test]
fn paused_label_goes_to_artist_line_without_artwork() {
    let activity = build_now(
        &track_with("Song", "Artist", Some(3725.0), Some(200.0)),
        PlayerState::Paused,
        None,
        &Settings::default(),
    );
    assert_eq!(
        string_field(&activity, "state"),
        "⏸ Paused at 1:02:05 · Artist"
    );
}

#[test]
fn paused_without_position_omits_time() {
    let activity = build_now(
        &track_with("Song", "Artist", None, Some(200.0)),
        PlayerState::Paused,
        None,
        &Settings::default(),
    );
    assert_eq!(string_field(&activity, "state"), "⏸ Paused · Artist");
}

// MARK: - Badge (status_display_type)

#[test]
fn status_display_type_defaults_to_artist() {
    let activity = build_now(&track(), PlayerState::Playing, None, &Settings::default());
    assert_eq!(
        activity["status_display_type"],
        BadgeLabelType::Artist.raw_value()
    );
}

#[test]
fn status_display_type_follows_setting() {
    let mut settings = Settings::default();
    settings.badge_label = BadgeLabelType::Track;
    let activity = build_now(&track(), PlayerState::Playing, None, &settings);
    assert_eq!(
        activity["status_display_type"],
        BadgeLabelType::Track.raw_value()
    );

    settings.badge_label = BadgeLabelType::AppName;
    let activity = build_now(&track(), PlayerState::Playing, None, &settings);
    assert_eq!(
        activity["status_display_type"],
        BadgeLabelType::AppName.raw_value()
    );
}

#[test]
fn artist_badge_falls_back_to_app_name_when_pause_label_is_on_artist_line() {
    // Without artwork the paused state text pollutes `state`, so an artist
    // badge falls back to the app name.
    let mut settings = Settings::default();
    let paused = build_now(&track(), PlayerState::Paused, None, &settings);
    assert_eq!(
        paused["status_display_type"],
        BadgeLabelType::AppName.raw_value()
    );

    // With artwork `state` stays clean, so no fallback.
    let paused_with_art = build_now(&track(), PlayerState::Paused, Some(&catalog()), &settings);
    assert_eq!(
        paused_with_art["status_display_type"],
        BadgeLabelType::Artist.raw_value()
    );

    // A track badge reads `details`, so it never needs the fallback.
    settings.badge_label = BadgeLabelType::Track;
    let paused_track = build_now(&track(), PlayerState::Paused, None, &settings);
    assert_eq!(
        paused_track["status_display_type"],
        BadgeLabelType::Track.raw_value()
    );
}

// MARK: - Buttons

#[test]
fn duplicate_button_urls_are_deduplicated() {
    let mut settings = Settings::default();
    settings.set_button1_type(LinkType::Repository);
    settings.set_button2_type(LinkType::Repository);
    let activity = build_now(&track(), PlayerState::Playing, None, &settings);
    let buttons = buttons(&activity).expect("buttons");
    assert_eq!(buttons.len(), 1);
    assert_eq!(buttons[0]["url"], DEFAULT_REPOSITORY_URL);
}

#[test]
fn unresolved_catalog_drops_button_instead_of_fallback() {
    // Defaults are button1 = song, button2 = repository. With no catalog the
    // song button is dropped and only the repository button remains.
    let activity = build_now(&track(), PlayerState::Playing, None, &Settings::default());
    let buttons = buttons(&activity).expect("buttons");
    assert_eq!(buttons.len(), 1);
    assert_eq!(buttons[0]["label"], "About This App");
    assert_eq!(buttons[0]["url"], DEFAULT_REPOSITORY_URL);
}

#[test]
fn both_buttons_off_omits_buttons_key() {
    let mut settings = Settings::default();
    settings.set_button1_type(LinkType::Disabled);
    settings.set_button2_type(LinkType::Disabled);
    let activity = build_now(&track(), PlayerState::Playing, Some(&catalog()), &settings);
    assert!(activity.get("buttons").is_none());
}

#[test]
fn button2_promoted_when_button1_is_off() {
    let mut settings = Settings::default();
    settings.set_button1_type(LinkType::Disabled);
    let activity = build_now(&track(), PlayerState::Playing, Some(&catalog()), &settings);
    let buttons = buttons(&activity).expect("buttons");
    assert_eq!(buttons.len(), 1);
    assert_eq!(buttons[0]["label"], "About This App");
    assert_eq!(buttons[0]["url"], DEFAULT_REPOSITORY_URL);
}

#[test]
fn two_buttons_with_resolved_catalog() {
    let activity = build_now(
        &track(),
        PlayerState::Playing,
        Some(&catalog()),
        &Settings::default(),
    );
    let buttons = buttons(&activity).expect("buttons");
    assert_eq!(buttons.len(), 2);
    assert_eq!(buttons[0]["url"], "https://music.apple.com/song");
    assert_eq!(buttons[0]["label"], "Play on Apple Music");
}

#[test]
fn invalid_custom_url_is_skipped() {
    let mut settings = Settings::default();
    settings.set_button1_type(LinkType::Custom);
    settings.set_button2_type(LinkType::Disabled);
    settings.custom_url = "ftp://example.com".into();
    let activity = build_now(&track(), PlayerState::Playing, None, &settings);
    assert!(activity.get("buttons").is_none());
}

#[test]
fn song_button_label_follows_source() {
    let mut settings = Settings::default();
    settings.set_button2_type(LinkType::Disabled);

    let apple_music = build(
        &track(),
        PlayerState::Playing,
        Some(&catalog()),
        &settings,
        MusicSourceId::AppleMusic,
        SystemTime::now(),
    );
    assert_eq!(
        buttons(&apple_music).unwrap()[0]["label"],
        "Play on Apple Music"
    );
}

#[test]
fn custom_button_label_wins_over_source_default() {
    let mut settings = Settings::default();
    settings.set_button2_type(LinkType::Disabled);
    settings.set_button1_label("My Label");
    let apple_music = build(
        &track(),
        PlayerState::Playing,
        Some(&catalog()),
        &settings,
        MusicSourceId::AppleMusic,
        SystemTime::now(),
    );
    assert_eq!(buttons(&apple_music).unwrap()[0]["label"], "My Label");
}

#[test]
fn button_label_is_truncated_to_32_characters() {
    let mut settings = Settings::default();
    settings.set_button2_type(LinkType::Disabled);
    settings.set_button1_label(&"x".repeat(64));
    let activity = build_now(&track(), PlayerState::Playing, Some(&catalog()), &settings);
    let label = buttons(&activity).unwrap()[0]["label"].as_str().unwrap();
    assert_eq!(label.chars().count(), 32);
}

// MARK: - URL validation

#[test]
fn is_valid_button_url_cases() {
    assert!(is_valid_button_url("https://example.com"));
    assert!(is_valid_button_url("http://example.com/path?q=1"));
    assert!(!is_valid_button_url("ftp://example.com"));
    assert!(!is_valid_button_url("javascript:alert(1)"));
    assert!(!is_valid_button_url(""));
    assert!(!is_valid_button_url(&format!(
        "https://example.com/{}",
        "a".repeat(512)
    )));
}
