use crate::t;

use super::i18n::{t, AppLanguage};
use super::models::{ConnState, MusicSourceId, PlayerState, SourceState, TrackInfo};

/// Values shown as the disabled info rows at the top of the tray menu.
///
/// The states are borrowed, not owned: this is rebuilt on every pass of the
/// event loop to notice a change, and SMTC wakes that loop several times a
/// second — cloning two `SourceState`s each time would be the app's largest
/// steady-state allocation for no gain.
///
/// Unlike macOS there are no automation-permission warnings: SMTC needs no
/// user grant, so those rows have no Windows counterpart.
#[derive(Debug, Clone)]
pub struct Input<'a> {
    pub apple_music: &'a SourceState,
    pub spotify: &'a SourceState,
    pub active_source: Option<MusicSourceId>,
    pub apple_music_enabled: bool,
    pub spotify_enabled: bool,
    pub discord_state: ConnState,
    pub language: AppLanguage,
}

pub fn lines(input: &Input) -> Vec<String> {
    let mut lines = source_lines(input);
    lines.push(discord_line(input));
    lines
}

fn source_lines(input: &Input) -> Vec<String> {
    let sources = enabled_sources(input);
    // Only point out which source is on display when more than one has a track.
    let mark_active = sources
        .iter()
        .filter(|s| state_for(**s, input).track.is_some())
        .count()
        >= 2;
    sources
        .iter()
        .map(|s| source_line(*s, input, mark_active))
        .collect()
}

fn source_line(source: MusicSourceId, input: &Input, mark_active: bool) -> String {
    let name = source.display_name();
    let s = state_for(source, input);
    if !s.running {
        return t!(input.language, "{name}: Not Running", "{name}: 未起動");
    }
    let mut line = match s.player_state {
        PlayerState::Stopped => t!(input.language, "{name}: Stopped", "{name}: 停止中"),
        PlayerState::Playing => format!("♪ {name}: {}", track_line(s.track.as_ref())),
        PlayerState::Paused => format!("⏸ {name}: {}", track_line(s.track.as_ref())),
    };
    if mark_active && Some(source) == input.active_source {
        line += &t(input.language, " (shown)", "（表示中）");
    }
    line
}

/// Always English — Discord-facing wording is not localized.
fn discord_line(input: &Input) -> String {
    if !input.apple_music.running && !input.spotify.running {
        return "Discord: Idle (connects when a player starts)".to_string();
    }
    match input.discord_state {
        ConnState::Connected => "Discord: Connected".to_string(),
        ConnState::Connecting => "Discord: Connecting…".to_string(),
        ConnState::Disconnected => "Discord: Disconnected (retrying)".to_string(),
    }
}

fn enabled_sources(input: &Input) -> Vec<MusicSourceId> {
    MusicSourceId::ALL
        .into_iter()
        .filter(|s| match s {
            MusicSourceId::AppleMusic => input.apple_music_enabled,
            MusicSourceId::Spotify => input.spotify_enabled,
        })
        .collect()
}

fn state_for<'a>(source: MusicSourceId, input: &Input<'a>) -> &'a SourceState {
    match source {
        MusicSourceId::AppleMusic => input.apple_music,
        MusicSourceId::Spotify => input.spotify,
    }
}

fn track_line(track: Option<&TrackInfo>) -> String {
    let Some(track) = track else {
        return "–".to_string();
    };
    if track.artist.is_empty() {
        track.name.clone()
    } else {
        format!("{} — {}", track.name, track.artist)
    }
}
