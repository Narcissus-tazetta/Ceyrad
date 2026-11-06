//! Reads `spec/youtube_music_search_vectors.json` and checks the Windows build against it.
//!
//! The macOS build reads the same file from
//! `Tests/CeyradTests/YouTubeMusicSearchVectorTests.swift`.

use std::path::PathBuf;

use serde::Deserialize;
use serde_json::{json, Value};

use ceyrad::core::ytmusic::{catalog_from, languages_to_try, parse_songs, pick_best};

#[derive(Deserialize)]
struct Spec {
    languages: Vec<LanguageCase>,
    picks: Vec<PickCase>,
}

#[derive(Deserialize)]
#[serde(rename_all = "camelCase")]
struct LanguageCase {
    name: String,
    track: String,
    artist: String,
    app_language: String,
    expected: Vec<String>,
}

#[derive(Deserialize)]
#[serde(rename_all = "camelCase")]
struct PickCase {
    name: String,
    rows: Vec<Row>,
    track: String,
    artist: String,
    duration_sec: Option<f64>,
    expected_video_id: Option<String>,
    expected_song_url: Option<String>,
    expected_artwork_url: Option<String>,
}

#[derive(Deserialize)]
#[serde(rename_all = "camelCase")]
struct Row {
    title: String,
    byline: Vec<String>,
    video_id: String,
}

fn spec() -> Spec {
    let path = PathBuf::from(env!("CARGO_MANIFEST_DIR"))
        .parent()
        .expect("windows/ has a parent")
        .join("spec/youtube_music_search_vectors.json");
    let raw = std::fs::read_to_string(path).expect("read spec/youtube_music_search_vectors.json");
    serde_json::from_str(&raw).expect("parse spec/youtube_music_search_vectors.json")
}

/// The shape of a real search response, reduced to what the parser reads.
fn response(rows: &[Row]) -> String {
    let rows: Vec<Value> = rows
        .iter()
        .map(|row| {
            let runs = |texts: Vec<&str>| {
                json!({"musicResponsiveListItemFlexColumnRenderer":
                    {"text": {"runs": texts.iter().map(|t| json!({"text": t})).collect::<Vec<_>>()}}})
            };
            json!({"musicResponsiveListItemRenderer": {
                "flexColumns": [
                    runs(vec![&row.title]),
                    runs(row.byline.iter().map(String::as_str).collect()),
                ],
                "playlistItemData": {"videoId": row.video_id},
                "thumbnail": {"musicThumbnailRenderer": {"thumbnail": {"thumbnails": [
                    {"url": "https://lh3.googleusercontent.com/abc=w60-h60-l90-rj"},
                    {"url": "https://lh3.googleusercontent.com/abc=w120-h120-l90-rj"},
                ]}}},
            }})
        })
        .collect();
    json!({"contents": {"tabs": [{"shelf": {"contents": rows}}]}}).to_string()
}

#[test]
fn every_language_vector_is_ordered_as_the_spec_says() {
    let spec = spec();
    assert!(!spec.languages.is_empty(), "no vectors: the read is broken");
    for case in &spec.languages {
        assert_eq!(
            languages_to_try(&case.track, &case.artist, &case.app_language),
            case.expected,
            "{}",
            case.name
        );
    }
}

#[test]
fn every_pick_vector_is_decided_exactly_as_the_spec_says() {
    let spec = spec();
    assert!(!spec.picks.is_empty(), "no vectors: the read is broken");
    for case in &spec.picks {
        let songs = parse_songs(&response(&case.rows)).expect("valid json");
        let best = pick_best(&songs, &case.track, &case.artist, case.duration_sec);
        assert_eq!(
            best.map(|s| s.video_id.as_str()),
            case.expected_video_id.as_deref(),
            "{}",
            case.name
        );
        if let Some(song) = best {
            let catalog = catalog_from(song);
            if let Some(url) = &case.expected_song_url {
                assert_eq!(catalog.song_url.as_ref(), Some(url), "{}", case.name);
            }
            if let Some(url) = &case.expected_artwork_url {
                assert_eq!(catalog.artwork_url.as_ref(), Some(url), "{}", case.name);
            }
        }
    }
}
