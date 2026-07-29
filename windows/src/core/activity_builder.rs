use std::time::{SystemTime, UNIX_EPOCH};

use serde_json::{json, Map, Value};

use super::format_time::format_time;
use super::models::{CatalogInfo, MusicSourceId, PlayerState, TrackInfo};
use super::settings_model::{BadgeLabelType, LinkType, Settings};

/// Discord requires string fields to be 2–128 characters.
const MIN_FIELD_CHARS: usize = 2;
const MAX_FIELD_CHARS: usize = 128;
const MAX_BUTTON_LABEL_CHARS: usize = 32;
const MAX_BUTTON_URL_CHARS: usize = 512;

/// Padding uses the braille blank (U+2800) rather than a space, which Discord
/// would trim away.
const PAD_CHAR: char = '\u{2800}';

pub fn build(
    track: &TrackInfo,
    player_state: PlayerState,
    catalog: Option<&CatalogInfo>,
    settings: &Settings,
    source: MusicSourceId,
    now: SystemTime,
) -> Map<String, Value> {
    let mut activity = Map::new();
    // type 2 = Listening
    activity.insert("type".into(), json!(2));

    let name = if track.name.is_empty() {
        "Unknown Track"
    } else {
        track.name.as_str()
    };
    activity.insert("details".into(), json!(clamp(name)));

    let artist = if track.artist.is_empty() {
        "Unknown Artist"
    } else {
        track.artist.as_str()
    };
    activity.insert("state".into(), json!(clamp(artist)));

    // While paused the position no longer advances, so show where it stopped.
    let pause_label = if player_state == PlayerState::Paused {
        Some(match track.position_sec {
            Some(position) => format!("⏸ Paused at {}", format_time(position)),
            None => "⏸ Paused".to_string(),
        })
    } else {
        None
    };

    let mut badge_label = settings.badge_label;

    match (catalog.and_then(|c| c.artwork_url.as_deref()), &pause_label) {
        (Some(artwork), _) => {
            let mut assets = Map::new();
            assets.insert("large_image".into(), json!(artwork));
            // Append the pause position to the album row, e.g. "Album · ⏸ Paused at 2:48".
            let mut large_text = track.album.clone();
            if let Some(pause_label) = &pause_label {
                large_text = if large_text.is_empty() {
                    pause_label.clone()
                } else {
                    format!("{large_text} · {pause_label}")
                };
            }
            if !large_text.is_empty() {
                assets.insert("large_text".into(), json!(clamp(&large_text)));
            }
            activity.insert("assets".into(), Value::Object(assets));
        }
        (None, Some(pause_label)) => {
            // Without artwork there is no album row, so the pause text goes on
            // the artist row instead.
            activity.insert(
                "state".into(),
                json!(clamp(&format!("{pause_label} · {artist}"))),
            );
            // That row now reads badly as a compact badge, so fall back to the
            // app name.
            if badge_label == BadgeLabelType::Artist {
                badge_label = BadgeLabelType::AppName;
            }
        }
        (None, None) => {}
    }
    activity.insert("status_display_type".into(), json!(badge_label.raw_value()));

    // The progress bar is playing-only. The sampled position goes stale on
    // re-sends triggered after it was taken (a settings change, say), so the
    // time elapsed since sampling is added back in — otherwise the bar rewinds.
    if player_state == PlayerState::Playing {
        if let (Some(position), Some(duration)) = (track.position_sec, track.duration_sec) {
            if duration > 0.0 {
                let elapsed_since_sample = now
                    .duration_since(track.position_sampled_at)
                    .map(|d| d.as_secs_f64())
                    .unwrap_or(0.0);
                let current_position = position + elapsed_since_sample;
                // Past the end there is no honest bar left to draw. Clamping
                // instead would pin `start` to `now - duration`, so both
                // timestamps would then slide with the wall clock and every
                // rebuild would read as a change — a re-send against Discord's
                // rate limit for a presence that has not moved. Reachable
                // whenever a player reports a too-short duration, which live
                // streams and some podcast apps do.
                if current_position < duration {
                    let now_unix = now
                        .duration_since(UNIX_EPOCH)
                        .map(|d| d.as_secs_f64())
                        .unwrap_or(0.0);
                    let start = now_unix - current_position;
                    activity.insert(
                        "timestamps".into(),
                        json!({
                            "start": (start * 1000.0).round() as i64,
                            "end": ((start + duration) * 1000.0).round() as i64,
                        }),
                    );
                }
            }
        }
    }

    let buttons = build_buttons(catalog, settings, source);
    if !buttons.is_empty() {
        activity.insert("buttons".into(), json!(buttons));
    }
    activity
}

/// Drift allowed between two builds of the same unchanged playback before they
/// count as different. Rebuilding an activity always recomputes `timestamps`
/// from the wall clock, so byte equality never holds — but a re-send that
/// Discord would render identically is wasted traffic against a rate limit that
/// only allows a handful of updates per 20 seconds.
const TIMESTAMP_DRIFT_MS: i64 = 2_000;

/// Whether Discord would render these two activities the same way.
pub fn is_equivalent(a: &Map<String, Value>, b: &Map<String, Value>) -> bool {
    if a.len() != b.len() {
        return false;
    }
    for (key, a_value) in a {
        let Some(b_value) = b.get(key) else {
            return false;
        };
        if key == "timestamps" {
            if !timestamps_equivalent(a_value, b_value) {
                return false;
            }
        } else if a_value != b_value {
            return false;
        }
    }
    true
}

fn timestamps_equivalent(a: &Value, b: &Value) -> bool {
    let field = |value: &Value, name: &str| value.get(name).and_then(Value::as_i64);
    match (
        field(a, "start"),
        field(b, "start"),
        field(a, "end"),
        field(b, "end"),
    ) {
        (Some(a_start), Some(b_start), Some(a_end), Some(b_end)) => {
            // A seek moves `start` far more than the drift window, so it still
            // reads as a change; a plain re-send moves it by the latency of the
            // round trip.
            (a_start - b_start).abs() <= TIMESTAMP_DRIFT_MS
                && (a_end - b_end).abs() <= TIMESTAMP_DRIFT_MS
        }
        _ => a == b,
    }
}

/// Discord allows at most 2 buttons, labels ≤ 32 chars, urls ≤ 512 chars.
fn build_buttons(
    catalog: Option<&CatalogInfo>,
    settings: &Settings,
    source: MusicSourceId,
) -> Vec<Value> {
    let mut buttons = Vec::new();
    let mut used_urls: Vec<String> = Vec::new();
    // An uncustomized label follows the source that is playing.
    let configs = [
        (settings.button1_type, settings.button1_label(Some(source))),
        (settings.button2_type, settings.button2_label(Some(source))),
    ];

    for (link_type, label) in configs {
        if link_type == LinkType::Disabled {
            continue;
        }
        let Some(url) = resolve_url(link_type, catalog, settings) else {
            continue;
        };
        if !is_valid_button_url(&url) || used_urls.iter().any(|u| u == &url) {
            continue;
        }
        let label: String = if label.is_empty() {
            "Link".to_string()
        } else {
            label.chars().take(MAX_BUTTON_LABEL_CHARS).collect()
        };
        buttons.push(json!({ "label": label, "url": url }));
        used_urls.push(url);
    }
    buttons
}

/// A button whose catalog url could not be resolved (a locally imported track,
/// say) is dropped rather than falling back to something else.
fn resolve_url(
    link_type: LinkType,
    catalog: Option<&CatalogInfo>,
    settings: &Settings,
) -> Option<String> {
    match link_type {
        LinkType::Song => catalog.and_then(|c| c.song_url.clone()),
        LinkType::Artist => catalog.and_then(|c| c.artist_url.clone()),
        LinkType::Album => catalog.and_then(|c| c.album_url.clone()),
        LinkType::Custom => Some(settings.custom_url.clone()),
        LinkType::Repository => Some(settings.repository_url.clone()),
        LinkType::Disabled => None,
    }
}

pub fn is_valid_button_url(string: &str) -> bool {
    if string.chars().count() > MAX_BUTTON_URL_CHARS {
        return false;
    }
    if string.chars().any(|c| c.is_whitespace() || c.is_control()) {
        return false;
    }
    let Some(colon) = string.find(':') else {
        return false;
    };
    let scheme = &string[..colon];
    if scheme.is_empty() {
        return false;
    }
    let scheme = scheme.to_ascii_lowercase();
    scheme == "http" || scheme == "https"
}

/// Whether `c` only exists as part of the character before it.
///
/// Cutting at a fixed scalar count can land in the middle of one of these — a
/// family emoji is seven scalars joined by ZWJ — and what Discord then renders
/// is a dangling joiner or a bare combining mark. Swift's `prefix` counts
/// grapheme clusters and never splits one; this is the same guarantee reached
/// from the other side, by dropping the incomplete tail.
fn is_continuation(c: char) -> bool {
    matches!(c,
        '\u{200D}'                  // zero-width joiner
        | '\u{FE0E}' | '\u{FE0F}'   // variation selectors
        | '\u{1F3FB}'..='\u{1F3FF}' // skin-tone modifiers
        | '\u{0300}'..='\u{036F}'   // combining diacritical marks
        | '\u{20D0}'..='\u{20F0}'   // combining marks for symbols
        | '\u{FE20}'..='\u{FE2F}'   // combining half marks
        | '\u{E0020}'..='\u{E007F}' // tag characters (flag sequences)
    )
}

fn clamp(s: &str) -> String {
    let mut value: String = s.chars().take(MAX_FIELD_CHARS).collect();
    // Only runs when the take above actually cut something off. Stripping the
    // trailing joiner is part of the same pass: a ZWJ only means anything
    // between two characters, so one left at the end goes with them.
    if s.chars().count() > MAX_FIELD_CHARS {
        while value.chars().next_back().is_some_and(is_continuation) {
            value.pop();
        }
    }
    while value.chars().count() < MIN_FIELD_CHARS {
        value.push(PAD_CHAR);
    }
    value
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn clamp_pads_with_braille_blank_not_space() {
        let padded = clamp("A");
        assert_eq!(padded.chars().count(), 2);
        assert!(padded.starts_with('A'));
        assert!(!padded.ends_with(' '));
        assert!(padded.ends_with(PAD_CHAR));
    }

    #[test]
    fn clamp_truncates_to_128_chars() {
        let long = "あ".repeat(300);
        assert_eq!(clamp(&long).chars().count(), 128);
    }
}
