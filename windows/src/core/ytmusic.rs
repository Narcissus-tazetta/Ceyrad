//! YouTube Music's own search: everything about the lookup that is not I/O.
//!
//! For a track playing on YouTube Music this is a better source than the iTunes
//! catalog: it is the same catalog the track is playing from, so it also has
//! releases iTunes has not indexed yet and uploads that were never on Apple
//! Music, its artwork is the one on the user's screen, and it yields the video
//! id the "Play on YouTube Music" button links to.
//!
//! The endpoint is the one the YouTube Music web client itself calls. It needs
//! no key, but it is not a published API, so its answer is parsed defensively:
//! anything not in the expected shape is skipped rather than trusted, and the
//! caller falls back to iTunes.

use serde_json::{json, Value};

use super::itunes::{norm, strip_featuring};
use super::models::CatalogInfo;

pub const SEARCH_URL: &str = "https://music.youtube.com/youtubei/v1/search?prettyPrint=false";

/// The web client the request claims to be. The endpoint has kept accepting
/// versions years old, so this is not something that needs to track releases.
const CLIENT_NAME: &str = "WEB_REMIX";
const CLIENT_VERSION: &str = "1.20241101.01.00";

/// The "Songs" filter of the search page. Without it the top of the results is
/// whatever YouTube Music ranks first — often the music video, whose thumbnail
/// is a 16:9 still rather than the cover.
const SONGS_FILTER: &str = "EgWKAQIIAWoMEA4QChADEAQQCRAF";

/// Thumbnails arrive at 60 or 120 px. The size lives in the URL's `=` suffix,
/// and the same URL serves any size up to the original.
const ARTWORK_SIZE_SUFFIX: &str = "w544-h544-l90-rj";

/// How far YouTube Music's duration may be from the player's. They round
/// differently (the player says 3:19 where the search says 3:20).
const DURATION_TOLERANCE_SEC: f64 = 3.0;

/// The interface languages to search in, most likely first.
///
/// YouTube Music answers in the language of the request, and spells artists and
/// sometimes titles accordingly (`キタニタツヤ` / `Tatsuya Kitani`). What the
/// player reported is in the user's YouTube Music language, which nothing on the
/// machine records — the Windows locale was observed to be English for a user
/// whose YouTube Music is Japanese. Kana or Hangul in the track itself settles
/// it; otherwise the app's own language is the best guess, and English is the
/// last resort.
pub fn languages_to_try(name: &str, artist: &str, app_language: &str) -> Vec<&'static str> {
    let text = name.chars().chain(artist.chars());
    let mut script_language = None;
    for c in text {
        match c as u32 {
            0x3040..=0x30FF | 0x31F0..=0x31FF | 0xFF66..=0xFF9F => {
                script_language = Some("ja");
                break;
            }
            0x1100..=0x11FF | 0x3130..=0x318F | 0xAC00..=0xD7AF => {
                script_language = Some("ko");
                break;
            }
            _ => {}
        }
    }
    let app_language = match app_language {
        "ja" => "ja",
        _ => "en",
    };
    let mut order: Vec<&'static str> = Vec::with_capacity(3);
    for language in [script_language, Some(app_language), Some("en")]
        .into_iter()
        .flatten()
    {
        if !order.contains(&language) {
            order.push(language);
        }
    }
    order
}

/// A song row from the search results.
#[derive(Debug, Clone, PartialEq)]
pub struct Song {
    pub title: String,
    /// The artist runs as shown, separators included (`A、B`, `A & B`).
    pub artists: String,
    pub first_artist: String,
    pub duration_sec: Option<f64>,
    pub video_id: String,
    pub thumbnail_url: String,
}

/// The request body. `language` and `region` decide which spelling artist names
/// come back in, so they should be the user's, matching what the player shows.
pub fn search_body(name: &str, artist: &str, language: &str, region: &str) -> String {
    json!({
        "context": {
            "client": {
                "clientName": CLIENT_NAME,
                "clientVersion": CLIENT_VERSION,
                "hl": language,
                "gl": region,
            }
        },
        "query": format!("{} {}", strip_featuring(name), artist).trim(),
        "params": SONGS_FILTER,
    })
    .to_string()
}

/// Every song row in a search response, in the order shown.
pub fn parse_songs(body: &str) -> Option<Vec<Song>> {
    let value: Value = serde_json::from_str(body).ok()?;
    let mut songs = Vec::new();
    collect_songs(&value, &mut songs);
    Some(songs)
}

fn collect_songs(value: &Value, out: &mut Vec<Song>) {
    match value {
        Value::Object(map) => {
            if let Some(row) = map.get("musicResponsiveListItemRenderer") {
                if let Some(song) = song_from_row(row) {
                    out.push(song);
                }
                return;
            }
            for child in map.values() {
                collect_songs(child, out);
            }
        }
        Value::Array(items) => {
            for child in items {
                collect_songs(child, out);
            }
        }
        _ => {}
    }
}

fn song_from_row(row: &Value) -> Option<Song> {
    let columns = row.get("flexColumns")?.as_array()?;
    let runs = |index: usize| -> Vec<&str> {
        columns
            .get(index)
            .and_then(|c| c.pointer("/musicResponsiveListItemFlexColumnRenderer/text/runs"))
            .and_then(Value::as_array)
            .map(|runs| {
                runs.iter()
                    .filter_map(|r| r.get("text")?.as_str())
                    .collect()
            })
            .unwrap_or_default()
    };

    let title = runs(0).concat();
    // `artists • album • 3:20`. The artists may span several runs.
    let byline = runs(1);
    let mut segments = byline.split(|run| run.trim() == "•");
    let artist_runs = segments.next().unwrap_or_default();
    let artists = artist_runs.concat().trim().to_string();
    let first_artist = artist_runs.first().map(|s| s.trim().to_string())?;
    let duration_sec = segments
        .last()
        .and_then(|seg| parse_duration(&seg.concat()));

    let video_id = row
        .pointer("/playlistItemData/videoId")
        .and_then(Value::as_str)?
        .to_string();
    let thumbnail_url = row
        .pointer("/thumbnail/musicThumbnailRenderer/thumbnail/thumbnails")
        .and_then(Value::as_array)?
        .last()?
        .get("url")?
        .as_str()?
        .to_string();

    if title.is_empty() || artists.is_empty() {
        return None;
    }
    Some(Song {
        title,
        artists,
        first_artist,
        duration_sec,
        video_id,
        thumbnail_url,
    })
}

/// `3:20` or `1:02:03`.
fn parse_duration(text: &str) -> Option<f64> {
    let mut total = 0.0;
    let mut parts = 0;
    for part in text.trim().split(':') {
        if part.is_empty() || !part.bytes().all(|b| b.is_ascii_digit()) {
            return None;
        }
        total = total * 60.0 + part.parse::<f64>().ok()?;
        parts += 1;
    }
    (2..=3).contains(&parts).then_some(total)
}

/// The row that is this track, or `None` when no row agrees closely enough.
///
/// The title has to match, and so does the artist: a wrong cover under someone's
/// presence is worse than none. The duration only breaks ties between rows that
/// already match, since remasters and live versions share both.
pub fn pick_best<'a>(
    songs: &'a [Song],
    name: &str,
    artist: &str,
    duration_sec: Option<f64>,
) -> Option<&'a Song> {
    // Folded before the credit is stripped, not after: YouTube Music writes
    // Japanese titles with fullwidth brackets (`（feat. X）`), which
    // `strip_featuring` only recognises in their ASCII form.
    let without_feat = |title: &str| norm(&strip_featuring(&norm(title)));
    let wanted_title = norm(name);
    let wanted_title_no_feat = without_feat(name);
    let wanted_artist = norm(artist);
    if wanted_artist.is_empty() {
        return None;
    }

    // The player and the search can spell a collaborator differently — the
    // player showed `キタニタツヤ、natori` where the search says
    // `キタニタツヤ、なとり` — so the lead artist alone is enough to agree on.
    let wanted_lead = norm(lead_artist(artist));
    let artist_matches = |song: &Song| {
        norm(&song.artists) == wanted_artist || norm(&song.first_artist) == wanted_lead
    };
    let duration_matches = |song: &Song| match (song.duration_sec, duration_sec) {
        (Some(a), Some(b)) => (a - b).abs() <= DURATION_TOLERANCE_SEC,
        _ => false,
    };

    let tiers: [&dyn Fn(&Song) -> bool; 3] = [
        &|s| norm(&s.title) == wanted_title && artist_matches(s) && duration_matches(s),
        &|s| norm(&s.title) == wanted_title && artist_matches(s),
        &|s| without_feat(&s.title) == wanted_title_no_feat && artist_matches(s),
    ];
    tiers
        .iter()
        .find_map(|tier| songs.iter().find(|song| tier(song)))
}

/// The first name in a joined artist string, using the separators YouTube Music
/// joins collaborators with.
fn lead_artist(artists: &str) -> &str {
    artists
        .split(['、', ',', '，', '&'])
        .next()
        .unwrap_or(artists)
        .trim()
}

pub fn catalog_from(song: &Song) -> CatalogInfo {
    CatalogInfo {
        song_url: Some(format!(
            "https://music.youtube.com/watch?v={}",
            song.video_id
        )),
        artist_url: None,
        album_url: None,
        artwork_url: Some(upscale_artwork(&song.thumbnail_url)),
    }
}

/// Swaps the size suffix of a Google-hosted thumbnail. Anything else — a
/// `i.ytimg.com` still, say — is left as it is.
fn upscale_artwork(url: &str) -> String {
    let is_google_image = url.contains(".googleusercontent.com/") || url.contains(".ggpht.com/");
    match url.rfind('=') {
        Some(index) if is_google_image => format!("{}={ARTWORK_SIZE_SUFFIX}", &url[..index]),
        _ => url.to_string(),
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn row(title: &str, byline: &[&str], video_id: &str) -> Value {
        json!({
            "musicResponsiveListItemRenderer": {
                "flexColumns": [
                    {"musicResponsiveListItemFlexColumnRenderer": {"text": {"runs": [{"text": title}]}}},
                    {"musicResponsiveListItemFlexColumnRenderer": {"text": {"runs":
                        byline.iter().map(|t| json!({"text": t})).collect::<Vec<_>>()}}},
                ],
                "playlistItemData": {"videoId": video_id},
                "thumbnail": {"musicThumbnailRenderer": {"thumbnail": {"thumbnails": [
                    {"url": "https://lh3.googleusercontent.com/abc=w60-h60-l90-rj"},
                    {"url": "https://lh3.googleusercontent.com/abc=w120-h120-l90-rj"},
                ]}}},
            }
        })
    }

    fn response(rows: Vec<Value>) -> String {
        json!({"contents": {"tabs": [{"shelf": {"contents": rows}}]}}).to_string()
    }

    #[test]
    fn rows_are_read_wherever_they_sit() {
        let body = response(vec![
            row(
                "シンカ",
                &["Ado", " • ", "シンカ", " • ", "3:20"],
                "q0OGymI3LMs",
            ),
            row(
                "唱",
                &[
                    "Ado",
                    "、",
                    "ジャックス・ジョーンズ",
                    " • ",
                    "唱",
                    " • ",
                    "4:26",
                ],
                "x",
            ),
        ]);
        let songs = parse_songs(&body).expect("valid json");
        assert_eq!(songs.len(), 2);
        assert_eq!(songs[0].title, "シンカ");
        assert_eq!(songs[0].artists, "Ado");
        assert_eq!(songs[0].duration_sec, Some(200.0));
        assert_eq!(songs[1].artists, "Ado、ジャックス・ジョーンズ");
        assert_eq!(songs[1].first_artist, "Ado");
    }

    #[test]
    fn a_row_missing_its_video_id_is_skipped() {
        let mut broken = row("Song", &["Artist", " • ", "Album", " • ", "3:00"], "id");
        broken["musicResponsiveListItemRenderer"]
            .as_object_mut()
            .unwrap()
            .remove("playlistItemData");
        assert!(parse_songs(&response(vec![broken])).unwrap().is_empty());
    }

    #[test]
    fn the_matching_song_wins_over_an_earlier_unrelated_one() {
        let songs = parse_songs(&response(vec![
            row("リベリオン", &["Ado", " • ", "残夢", " • ", "2:59"], "a"),
            row("シンカ", &["Ado", " • ", "シンカ", " • ", "3:20"], "b"),
        ]))
        .unwrap();
        let best = pick_best(&songs, "シンカ", "Ado", Some(199.0)).expect("a match");
        assert_eq!(best.video_id, "b");
    }

    #[test]
    fn duration_breaks_a_tie_between_versions() {
        let songs = parse_songs(&response(vec![
            row("Song", &["Artist", " • ", "Live", " • ", "5:37"], "live"),
            row("Song", &["Artist", " • ", "Album", " • ", "3:05"], "studio"),
        ]))
        .unwrap();
        assert_eq!(
            pick_best(&songs, "Song", "Artist", Some(184.0))
                .unwrap()
                .video_id,
            "studio"
        );
        // Without a duration the first matching row is taken.
        assert_eq!(
            pick_best(&songs, "Song", "Artist", None).unwrap().video_id,
            "live"
        );
    }

    #[test]
    fn another_artists_song_of_the_same_name_is_rejected() {
        let songs = parse_songs(&response(vec![row(
            "Song",
            &["Cover Band", " • ", "Tribute", " • ", "3:00"],
            "x",
        )]))
        .unwrap();
        assert!(pick_best(&songs, "Song", "Artist", None).is_none());
    }

    #[test]
    fn no_artist_means_no_match() {
        let songs = parse_songs(&response(vec![row(
            "Song",
            &["Artist", " • ", "A", " • ", "3:00"],
            "x",
        )]))
        .unwrap();
        assert!(pick_best(&songs, "Song", "", None).is_none());
    }

    #[test]
    fn featuring_and_collaborators_still_match() {
        let songs = parse_songs(&response(vec![row(
            "Radio Noise（feat. Ado）",
            &["ミコ吉", " • ", "Radio Noise", " • ", "4:49"],
            "x",
        )]))
        .unwrap();
        assert!(pick_best(&songs, "Radio Noise", "ミコ吉", None).is_some());

        let songs = parse_songs(&response(vec![row(
            "唱",
            &[
                "Ado",
                "、",
                "ジャックス・ジョーンズ",
                " • ",
                "唱",
                " • ",
                "4:26",
            ],
            "x",
        )]))
        .unwrap();
        assert!(pick_best(&songs, "唱", "Ado、ジャックス・ジョーンズ", None).is_some());
        assert!(pick_best(&songs, "唱", "Ado", None).is_some());
    }

    #[test]
    fn a_collaborator_spelled_differently_still_matches_on_the_lead() {
        let songs = parse_songs(&response(vec![
            row(
                "上書きしちゃった",
                &["なとり", "、", "キタニタツヤ", " • ", "x", " • ", "2:44"],
                "other",
            ),
            row(
                "いらないもの",
                &["キタニタツヤ", "、", "なとり", " • ", "x", " • ", "3:06"],
                "it",
            ),
        ]))
        .unwrap();
        let best = pick_best(&songs, "いらないもの", "キタニタツヤ、natori", Some(185.0));
        assert_eq!(best.unwrap().video_id, "it");
        // The lead has to agree; a shared collaborator is not enough.
        assert!(pick_best(&songs, "いらないもの", "natori、キタニタツヤ", None).is_none());
    }

    #[test]
    fn catalog_links_to_youtube_music_with_large_artwork() {
        let songs = parse_songs(&response(vec![row(
            "S",
            &["A", " • ", "B", " • ", "3:00"],
            "vid",
        )]))
        .unwrap();
        let catalog = catalog_from(&songs[0]);
        assert_eq!(
            catalog.song_url.as_deref(),
            Some("https://music.youtube.com/watch?v=vid")
        );
        assert_eq!(
            catalog.artwork_url.as_deref(),
            Some("https://lh3.googleusercontent.com/abc=w544-h544-l90-rj")
        );
        assert!(catalog.artist_url.is_none() && catalog.album_url.is_none());
    }

    #[test]
    fn a_video_still_is_not_resized() {
        let url = "https://i.ytimg.com/vi/x/hqdefault.jpg?sqp=abc=";
        assert_eq!(upscale_artwork(url), url);
    }

    #[test]
    fn the_script_of_the_track_decides_the_first_language() {
        assert_eq!(
            languages_to_try("エメラルド", "キタニタツヤ", "en"),
            ["ja", "en"]
        );
        assert_eq!(
            languages_to_try("Dynamite", "방탄소년단", "ja"),
            ["ko", "ja", "en"]
        );
        assert_eq!(
            languages_to_try("Shape of You", "Ed Sheeran", "ja"),
            ["ja", "en"]
        );
        assert_eq!(languages_to_try("Shape of You", "Ed Sheeran", "en"), ["en"]);
    }

    #[test]
    fn durations_parse_in_both_shapes() {
        assert_eq!(parse_duration("3:20"), Some(200.0));
        assert_eq!(parse_duration("1:02:03"), Some(3723.0));
        assert_eq!(parse_duration("再生回数 5 回"), None);
        assert_eq!(parse_duration("2020"), None);
    }

    #[test]
    fn the_body_carries_the_query_and_the_songs_filter() {
        let body: Value =
            serde_json::from_str(&search_body("Song (feat. X)", "Artist", "ja", "JP")).unwrap();
        assert_eq!(body["query"], "Song Artist");
        assert_eq!(body["params"], SONGS_FILTER);
        assert_eq!(body["context"]["client"]["hl"], "ja");
        assert_eq!(body["context"]["client"]["gl"], "JP");
    }
}
