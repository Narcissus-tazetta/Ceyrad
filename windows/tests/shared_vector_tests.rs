//! Reads `spec/activity_vectors.json` and checks the Windows build against it.
//!
//! The macOS build reads the same file from
//! `Tests/CeyradTests/SharedVectorTests.swift`. Changing one implementation and
//! not the other makes this fail — which is the only purpose of the file. What
//! each field means is covered case by case in `activity_builder_tests.rs`.

use std::path::PathBuf;
use std::time::{Duration, SystemTime, UNIX_EPOCH};

use serde::Deserialize;
use serde_json::{Map, Value};

use ceyrad::core::activity_builder::build;
use ceyrad::core::models::{CatalogInfo, PlayerState, TrackInfo};
use ceyrad::core::settings_model::{BadgeLabelType, ButtonSlot, LinkType, Settings};

#[derive(Deserialize)]
struct Spec {
    cases: Vec<Case>,
}

#[derive(Deserialize)]
#[serde(rename_all = "camelCase")]
struct Case {
    name: String,
    track: TrackSpec,
    player_state: String,
    catalog: Option<CatalogSpec>,
    #[serde(default)]
    settings: SettingsSpec,
    now_unix: f64,
    expected: Map<String, Value>,
}

#[derive(Deserialize)]
#[serde(rename_all = "camelCase")]
struct TrackSpec {
    name: String,
    artist: String,
    album: String,
    duration_sec: Option<f64>,
    position_sec: Option<f64>,
    position_sampled_at_unix: f64,
}

#[derive(Deserialize)]
#[serde(rename_all = "camelCase")]
struct CatalogSpec {
    #[serde(default, rename = "songURL")]
    song_url: Option<String>,
    #[serde(default, rename = "artistURL")]
    artist_url: Option<String>,
    #[serde(default, rename = "albumURL")]
    album_url: Option<String>,
    #[serde(default, rename = "artworkURL")]
    artwork_url: Option<String>,
}

#[derive(Default, Deserialize)]
#[serde(rename_all = "camelCase")]
struct SettingsSpec {
    #[serde(default)]
    button1_type: Option<String>,
    #[serde(default)]
    button2_type: Option<String>,
    #[serde(default)]
    button1_label: Option<String>,
    #[serde(default)]
    button2_label: Option<String>,
    #[serde(default, rename = "customURL")]
    custom_url: Option<String>,
    #[serde(default, rename = "repositoryURL")]
    repository_url: Option<String>,
    #[serde(default)]
    badge_label: Option<String>,
    #[serde(default)]
    pause_hide_minutes: Option<i32>,
}

/// The spec lives beside the two implementations, not inside either of them.
fn spec_path() -> PathBuf {
    PathBuf::from(env!("CARGO_MANIFEST_DIR"))
        .parent()
        .expect("windows/ has a parent")
        .join("spec/activity_vectors.json")
}

#[test]
fn every_vector_builds_exactly_what_the_spec_says() {
    let raw = std::fs::read_to_string(spec_path()).expect("read spec/activity_vectors.json");
    let spec: Spec = serde_json::from_str(&raw).expect("parse spec/activity_vectors.json");
    assert!(!spec.cases.is_empty(), "no vectors: the read is broken");

    for case in &spec.cases {
        let activity = build(
            &track(&case.track),
            player_state(&case.player_state),
            case.catalog.as_ref().map(catalog).as_ref(),
            &settings(&case.settings),
            unix(case.now_unix),
        );
        assert_eq!(
            Value::Object(activity),
            Value::Object(case.expected.clone()),
            "{}",
            case.name
        );
    }
}

fn unix(seconds: f64) -> SystemTime {
    UNIX_EPOCH + Duration::from_secs_f64(seconds)
}

fn track(spec: &TrackSpec) -> TrackInfo {
    let mut track = TrackInfo::new(&spec.name, &spec.artist, &spec.album);
    track.duration_sec = spec.duration_sec;
    track.position_sec = spec.position_sec;
    track.position_sampled_at = unix(spec.position_sampled_at_unix);
    track
}

fn catalog(spec: &CatalogSpec) -> CatalogInfo {
    CatalogInfo {
        song_url: spec.song_url.clone(),
        artist_url: spec.artist_url.clone(),
        album_url: spec.album_url.clone(),
        artwork_url: spec.artwork_url.clone(),
    }
}

fn player_state(raw: &str) -> PlayerState {
    match raw {
        "playing" => PlayerState::Playing,
        "paused" => PlayerState::Paused,
        _ => PlayerState::Stopped,
    }
}

fn settings(spec: &SettingsSpec) -> Settings {
    let mut settings = Settings::default();
    if let Some(raw) = &spec.button1_type {
        settings.set_button_type(ButtonSlot::One, link_type(raw));
    }
    if let Some(raw) = &spec.button2_type {
        settings.set_button_type(ButtonSlot::Two, link_type(raw));
    }
    if let Some(label) = &spec.button1_label {
        settings.set_button_label(ButtonSlot::One, label);
    }
    if let Some(label) = &spec.button2_label {
        settings.set_button_label(ButtonSlot::Two, label);
    }
    if let Some(url) = &spec.custom_url {
        settings.custom_url = url.clone();
    }
    if let Some(url) = &spec.repository_url {
        settings.repository_url = url.clone();
    }
    if let Some(raw) = &spec.badge_label {
        settings.badge_label = match raw.as_str() {
            "appName" => BadgeLabelType::AppName,
            "artist" => BadgeLabelType::Artist,
            "track" => BadgeLabelType::Track,
            other => panic!("unknown badge label: {other}"),
        };
    }
    if let Some(minutes) = spec.pause_hide_minutes {
        settings.pause_hide_minutes = minutes;
    }
    settings
}

fn link_type(raw: &str) -> LinkType {
    match raw {
        "song" => LinkType::Song,
        "artist" => LinkType::Artist,
        "album" => LinkType::Album,
        "custom" => LinkType::Custom,
        "repository" => LinkType::Repository,
        "disabled" => LinkType::Disabled,
        other => panic!("unknown link type: {other}"),
    }
}
