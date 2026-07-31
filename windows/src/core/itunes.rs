//! iTunes Search API: everything about the lookup that is not I/O.
//!
//! A hand port of the macOS `ITunesSearchClient`'s pure half. The API needs no
//! authentication and answers song URL, artist URL, album URL and artwork in a
//! single request, which is why the macOS build chose it over MusicKit — and
//! why the Windows build can reach it with nothing but an HTTP GET.
//!
//! Matching is deliberately strict. A wrong link under someone's presence is
//! worse than no link, so a result is only accepted when it agrees with the
//! track on one of a few ranked tiers; anything below them is discarded.

use serde::Deserialize;

use super::models::CatalogInfo;

/// The macOS build asks for 10 and picks from them; fewer would drop the right
/// answer for tracks with many re-releases.
const RESULT_LIMIT: u32 = 10;

/// `artworkUrl100` is a 100px thumbnail. Discord renders the large image far
/// bigger than that, and the same URL serves any size Apple has.
const ARTWORK_SOURCE_SIZE: &str = "100x100bb";
const ARTWORK_WANTED_SIZE: &str = "512x512bb";

#[derive(Debug, Clone, Default, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct SearchResult {
    pub track_name: Option<String>,
    pub artist_name: Option<String>,
    pub collection_name: Option<String>,
    pub track_view_url: Option<String>,
    pub artist_view_url: Option<String>,
    pub collection_view_url: Option<String>,
    #[serde(rename = "artworkUrl100")]
    pub artwork_url100: Option<String>,
}

#[derive(Debug, Default, Deserialize)]
pub struct SearchResponse {
    #[serde(default)]
    pub results: Vec<SearchResult>,
}

/// The URL for one lookup. `country` picks the storefront, which decides both
/// which catalog is searched and which regional URLs come back.
pub fn search_url(name: &str, artist: &str, country: &str) -> String {
    // Local `feat.` spellings often differ from the catalog's, and the term is
    // matched loosely by the API anyway, so the search is given the plain title.
    let term = format!("{} {}", strip_featuring(name), artist);
    format!(
        "https://itunes.apple.com/search?term={}&media=music&entity=song&limit={}&country={}",
        percent_encode(term.trim()),
        RESULT_LIMIT,
        percent_encode(country)
    )
}

pub fn parse_response(body: &str) -> Option<SearchResponse> {
    serde_json::from_str(body).ok()
}

pub fn catalog_from(result: &SearchResult) -> CatalogInfo {
    CatalogInfo {
        song_url: result.track_view_url.clone(),
        artist_url: result.artist_view_url.clone(),
        album_url: result.collection_view_url.clone(),
        artwork_url: result
            .artwork_url100
            .as_deref()
            .map(|url| url.replace(ARTWORK_SOURCE_SIZE, ARTWORK_WANTED_SIZE)),
    }
}

/// The best result, or `None` when nothing agrees closely enough to be safe.
pub fn pick_best<'a>(
    results: &'a [SearchResult],
    name: &str,
    artist: &str,
    album: &str,
) -> Option<&'a SearchResult> {
    let wanted_track = norm(name);
    let wanted_artist = norm(artist);
    let wanted_album = norm(&strip_album_suffix(album));
    let wanted_track_no_feat = norm(&strip_featuring(name));

    struct Candidate<'a> {
        result: &'a SearchResult,
        track: String,
        track_no_feat: String,
        artist: String,
        album: String,
    }

    // Normalising is not free, so each result is folded once rather than once
    // per tier.
    let candidates: Vec<Candidate> = results
        .iter()
        .map(|result| Candidate {
            result,
            track: norm(result.track_name.as_deref().unwrap_or("")),
            track_no_feat: norm(&strip_featuring(result.track_name.as_deref().unwrap_or(""))),
            artist: norm(result.artist_name.as_deref().unwrap_or("")),
            album: norm(&strip_album_suffix(
                result.collection_name.as_deref().unwrap_or(""),
            )),
        })
        .collect();

    let tiers: [&dyn Fn(&Candidate) -> bool; 5] = [
        &|c: &Candidate| {
            c.track == wanted_track && c.artist == wanted_artist && c.album == wanted_album
        },
        &|c: &Candidate| c.track == wanted_track && c.artist == wanted_artist,
        &|c: &Candidate| c.track_no_feat == wanted_track_no_feat && c.artist == wanted_artist,
        &|c: &Candidate| c.track == wanted_track,
        &|c: &Candidate| c.track_no_feat == wanted_track_no_feat,
    ];

    for tier in tiers {
        if let Some(candidate) = candidates.iter().find(|c| tier(c)) {
            return Some(candidate.result);
        }
    }
    None
}

// MARK: - Absorbing spelling differences

/// Drops `(feat. X)`, `[featuring X]` and a trailing `feat. X`. Returns the
/// input unchanged when stripping would leave nothing behind.
pub fn strip_featuring(s: &str) -> String {
    const BRACKETED: [&str; 4] = ["featuring", "feat", "ft", "with"];
    const TRAILING: [&str; 3] = ["featuring", "feat", "ft"];

    let chars: Vec<char> = s.chars().collect();
    let mut out: Vec<char> = Vec::with_capacity(chars.len());
    let mut i = 0;
    while i < chars.len() {
        let c = chars[i];
        if (c == '(' || c == '[') && starts_with_keyword(&chars[i + 1..], &BRACKETED) {
            // `[^)\]]*[)\]]`: the group ends at whichever bracket comes first.
            // An unclosed one is not a group at all, so it is left alone.
            if let Some(offset) = chars[i + 1..].iter().position(|&c| c == ')' || c == ']') {
                // Swallow the whitespace that led into the group too.
                while out.last().is_some_and(|c| c.is_whitespace()) {
                    out.pop();
                }
                i += offset + 2;
                continue;
            }
        }
        if c.is_whitespace() && starts_with_keyword(&chars[i + 1..], &TRAILING) {
            break;
        }
        out.push(c);
        i += 1;
    }

    let stripped: String = out.into_iter().collect();
    let stripped = stripped.trim();
    if stripped.is_empty() {
        s.to_string()
    } else {
        stripped.to_string()
    }
}

/// True when `rest` opens with one of `keywords` followed by `.` or whitespace.
fn starts_with_keyword(rest: &[char], keywords: &[&str]) -> bool {
    keywords.iter().any(|keyword| {
        if rest.len() <= keyword.len() {
            return false;
        }
        let matches = rest
            .iter()
            .zip(keyword.chars())
            .all(|(a, b)| a.to_ascii_lowercase() == b);
        let next = rest[keyword.len()];
        matches && (next == '.' || next.is_whitespace())
    })
}

/// Drops a trailing `- Single` or `- EP`, which the catalog appends to album
/// names and players usually do not. Returns the input when nothing is left.
pub fn strip_album_suffix(s: &str) -> String {
    let trimmed = s.trim_end();
    for suffix in ["single", "ep"] {
        let Some(head) = trim_suffix_ignore_case(trimmed, suffix) else {
            continue;
        };
        let head = head.trim_end();
        let Some(head) = head.strip_suffix('-') else {
            continue;
        };
        let head = head.trim();
        if !head.is_empty() {
            return head.to_string();
        }
    }
    s.trim().to_string()
}

fn trim_suffix_ignore_case<'a>(s: &'a str, suffix: &str) -> Option<&'a str> {
    let split = s.len().checked_sub(suffix.len())?;
    if !s.is_char_boundary(split) {
        return None;
    }
    let (head, tail) = s.split_at(split);
    tail.eq_ignore_ascii_case(suffix).then_some(head)
}

/// Case-, width- and diacritic-insensitive folding, so `ＣＡＦÉ` and `cafe`
/// compare equal.
///
/// Unlike the macOS build, one side of every comparison here is *not* from
/// Apple's catalog: SMTC reports whatever the local file's tags say, and for
/// imported music those routinely disagree with the catalog on accents. A
/// missed fold is not a lost tier but a lost match — no artwork and no catalog
/// buttons — so the mapping is spelled out rather than pulled in as a
/// dependency for decomposition.
fn norm(s: &str) -> String {
    s.chars()
        // An accent written as a separate combining mark — `e` + U+0301 rather
        // than `é`. Both spellings reach here and must fold to the same thing.
        .filter(|c| !matches!(*c as u32, 0x0300..=0x036F))
        .map(|c| match c as u32 {
            // Fullwidth ASCII (U+FF01–U+FF5E) onto its halfwidth twin.
            code @ 0xFF01..=0xFF5E => char::from_u32(code - 0xFEE0).unwrap_or(c),
            // Ideographic space, which width folding does not cover.
            0x3000 => ' ',
            _ => fold_diacritic(c),
        })
        .flat_map(char::to_lowercase)
        .collect::<String>()
        .trim()
        .to_string()
}

/// Precomposed Latin-1 and Latin Extended-A onto their base letters.
///
/// Covers the alphabets the iTunes catalog actually carries for Latin-script
/// titles; anything outside it is left alone.
fn fold_diacritic(c: char) -> char {
    match c {
        'À'..='Å' | 'Ā' | 'Ă' | 'Ą' => 'A',
        'Æ' => 'A', // folded to its first letter, as `Æ` vs `AE` never matches anyway
        'Ç' | 'Ć' | 'Ĉ' | 'Ċ' | 'Č' => 'C',
        'Ď' | 'Đ' => 'D',
        'È'..='Ë' | 'Ē' | 'Ĕ' | 'Ė' | 'Ę' | 'Ě' => 'E',
        'Ĝ'..='Ģ' => 'G',
        'Ĥ' | 'Ħ' => 'H',
        'Ì'..='Ï' | 'Ĩ' | 'Ī' | 'Ĭ' | 'Į' | 'İ' => 'I',
        'Ĵ' => 'J',
        'Ķ' => 'K',
        'Ĺ' | 'Ļ' | 'Ľ' | 'Ŀ' | 'Ł' => 'L',
        'Ñ' | 'Ń' | 'Ņ' | 'Ň' | 'Ŋ' => 'N',
        'Ò'..='Ö' | 'Ø' | 'Ō' | 'Ŏ' | 'Ő' => 'O',
        'Ŕ' | 'Ŗ' | 'Ř' => 'R',
        'Ś' | 'Ŝ' | 'Ş' | 'Š' => 'S',
        'Ţ' | 'Ť' | 'Ŧ' => 'T',
        'Ù'..='Ü' | 'Ũ' | 'Ū' | 'Ŭ' | 'Ů' | 'Ű' | 'Ų' => 'U',
        'Ŵ' => 'W',
        'Ý' | 'Ŷ' | 'Ÿ' => 'Y',
        'Ź' | 'Ż' | 'Ž' => 'Z',
        'à'..='å' | 'ā' | 'ă' | 'ą' => 'a',
        'æ' => 'a',
        'ç' | 'ć' | 'ĉ' | 'ċ' | 'č' => 'c',
        'ď' | 'đ' => 'd',
        'è'..='ë' | 'ē' | 'ĕ' | 'ė' | 'ę' | 'ě' => 'e',
        'ĝ'..='ģ' => 'g',
        'ĥ' | 'ħ' => 'h',
        'ì'..='ï' | 'ĩ' | 'ī' | 'ĭ' | 'į' | 'ı' => 'i',
        'ĵ' => 'j',
        'ķ' => 'k',
        'ĺ' | 'ļ' | 'ľ' | 'ŀ' | 'ł' => 'l',
        'ñ' | 'ń' | 'ņ' | 'ň' | 'ŉ' | 'ŋ' => 'n',
        'ò'..='ö' | 'ø' | 'ō' | 'ŏ' | 'ő' => 'o',
        'ŕ' | 'ŗ' | 'ř' => 'r',
        'ś' | 'ŝ' | 'ş' | 'š' => 's',
        'ţ' | 'ť' | 'ŧ' => 't',
        'ù'..='ü' | 'ũ' | 'ū' | 'ŭ' | 'ů' | 'ű' | 'ų' => 'u',
        'ŵ' => 'w',
        'ý' | 'ÿ' | 'ŷ' => 'y',
        'ź' | 'ż' | 'ž' => 'z',
        _ => c,
    }
}

/// Percent-encodes everything outside the unreserved set, so a term containing
/// `&`, `#` or a space cannot reshape the query.
fn percent_encode(s: &str) -> String {
    let mut out = String::with_capacity(s.len());
    for byte in s.as_bytes() {
        match byte {
            b'A'..=b'Z' | b'a'..=b'z' | b'0'..=b'9' | b'-' | b'_' | b'.' | b'~' => {
                out.push(*byte as char)
            }
            _ => out.push_str(&format!("%{byte:02X}")),
        }
    }
    out
}

#[cfg(test)]
mod tests {
    use super::*;

    fn result(track: &str, artist: &str, album: &str) -> SearchResult {
        SearchResult {
            track_name: Some(track.into()),
            artist_name: Some(artist.into()),
            collection_name: Some(album.into()),
            track_view_url: Some(format!("https://music.apple.com/{track}")),
            ..Default::default()
        }
    }

    #[test]
    fn search_url_escapes_the_term() {
        let url = search_url("R&B Song", "A/B", "JP");
        assert!(url.contains("term=R%26B%20Song%20A%2FB"), "{url}");
        assert!(url.ends_with("&country=JP"));
    }

    #[test]
    fn search_url_drops_featuring_from_the_term() {
        let url = search_url("Song (feat. Someone)", "Artist", "US");
        assert!(url.contains("term=Song%20Artist"), "{url}");
    }

    #[test]
    fn artwork_is_upscaled() {
        let catalog = catalog_from(&SearchResult {
            artwork_url100: Some("https://is1.mzstatic.com/a/100x100bb.jpg".into()),
            ..Default::default()
        });
        assert_eq!(
            catalog.artwork_url.as_deref(),
            Some("https://is1.mzstatic.com/a/512x512bb.jpg")
        );
    }

    #[test]
    fn exact_album_match_beats_an_earlier_loose_one() {
        let results = [
            result("Brand New", "Mrs. GREEN APPLE", "Greatest Hits"),
            result("Brand New", "Mrs. GREEN APPLE", "Brand New - Single"),
        ];
        let best = pick_best(
            &results,
            "Brand New",
            "Mrs. GREEN APPLE",
            "Brand New - Single",
        );
        assert_eq!(
            best.and_then(|r| r.collection_name.as_deref()),
            Some("Brand New - Single")
        );
    }

    #[test]
    fn a_different_song_is_rejected_outright() {
        let results = [result("Something Else", "Another Artist", "Whatever")];
        assert!(pick_best(&results, "Brand New", "Mrs. GREEN APPLE", "").is_none());
    }

    #[test]
    fn matching_survives_a_featuring_difference() {
        let results = [result("Song (feat. Guest)", "Artist", "Album")];
        let best = pick_best(&results, "Song", "Artist", "Album");
        assert!(best.is_some());
    }

    #[test]
    fn matching_is_width_and_case_insensitive() {
        let results = [result("ＢＲＡＮＤ ＮＥＷ", "Mrs. GREEN APPLE", "x")];
        assert!(pick_best(&results, "brand new", "mrs. green apple", "x").is_some());
    }

    #[test]
    fn strip_featuring_handles_both_shapes() {
        assert_eq!(strip_featuring("Song (feat. Guest)"), "Song");
        assert_eq!(strip_featuring("Song [Featuring Guest]"), "Song");
        assert_eq!(strip_featuring("Song feat. Guest"), "Song");
        assert_eq!(strip_featuring("Song ft Guest"), "Song");
        assert_eq!(strip_featuring("Song (with Guest)"), "Song");
    }

    #[test]
    fn strip_featuring_keeps_a_title_that_is_only_a_credit() {
        assert_eq!(strip_featuring("feat. Guest"), "feat. Guest");
    }

    #[test]
    fn strip_featuring_leaves_unrelated_brackets_and_words() {
        assert_eq!(strip_featuring("Song (Remix)"), "Song (Remix)");
        assert_eq!(strip_featuring("Software Update"), "Software Update");
    }

    #[test]
    fn strip_album_suffix_drops_single_and_ep_only() {
        assert_eq!(strip_album_suffix("Brand New - Single"), "Brand New");
        assert_eq!(strip_album_suffix("Something - ep"), "Something");
        assert_eq!(strip_album_suffix("Brand New"), "Brand New");
        assert_eq!(strip_album_suffix("- Single"), "- Single");
        assert_eq!(strip_album_suffix("Live in Japan"), "Live in Japan");
        // "Re-Single Album" ends in neither suffix; the hyphen inside a word
        // must not be read as the separator.
        assert_eq!(strip_album_suffix("Re-Single Album"), "Re-Single Album");
    }

    #[test]
    fn an_artist_match_beats_an_earlier_name_only_one() {
        // The whole point of the tier list: a cover version listed first must
        // lose to the track by the artist actually playing.
        let results = [
            result("Song", "Cover Band", "Tribute"),
            result("Song", "Artist", "Album"),
        ];
        let best = pick_best(&results, "Song", "Artist", "Album").expect("a match");
        assert_eq!(best.artist_name.as_deref(), Some("Artist"));
    }

    #[test]
    fn matching_ignores_case_width_and_diacritics() {
        // One side comes from SMTC — the local file's tags — so accents and
        // fullwidth forms routinely disagree with the catalog.
        let results = [result("CAFE SONG", "Artist", "Album")];
        for (name, artist) in [
            ("ＣＡＦÉ　ＳＯＮＧ", "Ａｒｔｉｓｔ"),
            ("café song", "artist"),
            // The same accent spelled as a combining mark rather than as a
            // precomposed character.
            ("cafe\u{301} song", "artist"),
        ] {
            assert!(
                pick_best(&results, name, artist, "Album").is_some(),
                "{name:?} / {artist:?}"
            );
        }
    }

    #[test]
    fn featuring_matches_whichever_side_carries_it() {
        let results = [result("Song", "Artist", "Album")];
        // Catalog is plain, query carries the notation.
        assert!(pick_best(&results, "Song (feat. Guest)", "Artist", "Album").is_some());

        // And the other direction.
        let with_feat = [result("Song feat. Guest", "Artist", "Album")];
        assert!(pick_best(&with_feat, "Song", "Artist", "Album").is_some());
    }
}
