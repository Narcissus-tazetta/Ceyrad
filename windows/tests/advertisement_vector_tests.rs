//! Reads `spec/advertisement_vectors.json` and checks the Windows build against it.
//!
//! The macOS build reads the same file from
//! `Tests/CeyradTests/AdvertisementVectorTests.swift`.

use std::path::PathBuf;

use serde::Deserialize;

use ceyrad::core::models::TrackInfo;

#[derive(Deserialize)]
struct Spec {
    cases: Vec<Case>,
}

#[derive(Deserialize)]
struct Case {
    name: String,
    track: Track,
    expected: bool,
}

#[derive(Deserialize)]
struct Track {
    name: String,
    artist: String,
    album: String,
}

fn spec_path() -> PathBuf {
    PathBuf::from(env!("CARGO_MANIFEST_DIR"))
        .parent()
        .expect("windows/ has a parent")
        .join("spec/advertisement_vectors.json")
}

#[test]
fn every_vector_is_decided_exactly_as_the_spec_says() {
    let raw = std::fs::read_to_string(spec_path()).expect("read spec/advertisement_vectors.json");
    let spec: Spec = serde_json::from_str(&raw).expect("parse spec/advertisement_vectors.json");
    assert!(!spec.cases.is_empty(), "no vectors: the read is broken");

    for case in &spec.cases {
        let track = TrackInfo::new(&case.track.name, &case.track.artist, &case.track.album);
        assert_eq!(track.is_advertisement(), case.expected, "{}", case.name);
    }
}
