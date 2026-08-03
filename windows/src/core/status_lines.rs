use crate::t_fmt;

use super::apple_music;
use super::i18n::AppLanguage;
use super::models::{ConnState, MusicState, PlayerState, TrackInfo};

/// Values shown as the disabled info rows at the top of the tray menu.
///
/// The state is borrowed, not owned: this is rebuilt on every pass of the
/// event loop to notice a change, and SMTC wakes that loop several times a
/// second — cloning a `MusicState` each time would be the app's largest
/// steady-state allocation for no gain.
///
/// Unlike macOS there are no automation-permission warnings: SMTC needs no
/// user grant, so those rows have no Windows counterpart.
#[derive(Debug, Clone)]
pub struct Input<'a> {
    pub music: &'a MusicState,
    pub discord_state: ConnState,
    pub language: AppLanguage,
}

pub fn lines(input: &Input) -> Vec<String> {
    vec![source_line(input), discord_line(input)]
}

fn source_line(input: &Input) -> String {
    let name = apple_music::DISPLAY_NAME;
    let s = input.music;
    if !s.running {
        return t_fmt!(input.language, "{name}: Not Running", "{name}: 未起動");
    }
    match s.player_state {
        PlayerState::Stopped => t_fmt!(input.language, "{name}: Stopped", "{name}: 停止中"),
        PlayerState::Playing => format!("♪ {name}: {}", track_line(s.track.as_ref())),
        PlayerState::Paused => format!("⏸ {name}: {}", track_line(s.track.as_ref())),
    }
}

/// Always English — Discord-facing wording is not localized.
fn discord_line(input: &Input) -> String {
    if !input.music.running {
        return "Discord: Idle (connects when a player starts)".to_string();
    }
    match input.discord_state {
        ConnState::Connected => "Discord: Connected".to_string(),
        ConnState::Connecting => "Discord: Connecting…".to_string(),
        ConnState::Disconnected => "Discord: Disconnected (retrying)".to_string(),
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
