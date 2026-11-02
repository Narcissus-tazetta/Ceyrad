//! Reads `spec/youtube_music_vectors.json` and checks the Windows build against it.
//!
//! The macOS build reads the same file from
//! `Tests/CeyradTests/YouTubeMusicVectorTests.swift`.

use std::path::PathBuf;

use serde::Deserialize;

use ceyrad::core::youtube_music::is_youtube_music;

#[derive(Deserialize)]
struct Spec {
    cases: Vec<Case>,
}

#[derive(Deserialize)]
#[serde(rename_all = "camelCase")]
struct Case {
    name: String,
    is_browser: bool,
    album: String,
    #[serde(rename = "hasYouTubeMusicTab")]
    has_youtube_music_tab: Option<bool>,
    expected: bool,
}

/// The spec lives beside the two implementations, not inside either of them.
fn spec_path() -> PathBuf {
    PathBuf::from(env!("CARGO_MANIFEST_DIR"))
        .parent()
        .expect("windows/ has a parent")
        .join("spec/youtube_music_vectors.json")
}

#[test]
fn every_vector_is_decided_exactly_as_the_spec_says() {
    let raw = std::fs::read_to_string(spec_path()).expect("read spec/youtube_music_vectors.json");
    let spec: Spec = serde_json::from_str(&raw).expect("parse spec/youtube_music_vectors.json");
    assert!(!spec.cases.is_empty(), "no vectors: the read is broken");

    for case in &spec.cases {
        assert_eq!(
            is_youtube_music(case.is_browser, &case.album, case.has_youtube_music_tab),
            case.expected,
            "{}",
            case.name
        );
    }
}
