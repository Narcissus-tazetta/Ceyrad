//! The rows the tray menu shows. macOS builds the same list in
//! `StatusLinesBuilder`; here it is also the dev build's only status surface,
//! so it is worth pinning down.

use ceyrad::core::i18n::AppLanguage;
use ceyrad::core::models::{ConnState, MusicState, PlayerState, TrackInfo};
use ceyrad::core::status_lines::{lines, Input};

fn playing(name: &str, artist: &str) -> MusicState {
    MusicState {
        running: true,
        player_state: PlayerState::Playing,
        track: Some(TrackInfo::new(name, artist, "Album")),
        ..MusicState::default()
    }
}

fn paused(name: &str, artist: &str) -> MusicState {
    MusicState {
        player_state: PlayerState::Paused,
        ..playing(name, artist)
    }
}

fn stopped() -> MusicState {
    MusicState {
        running: true,
        ..MusicState::default()
    }
}

fn input(music: &MusicState) -> Input<'_> {
    Input {
        music,
        discord_state: ConnState::Disconnected,
        language: AppLanguage::En,
    }
}

#[test]
fn not_running_reports_as_such() {
    let off = MusicState::default();
    let rows = lines(&input(&off));
    assert_eq!(
        rows,
        vec![
            "Apple Music: Not Running",
            "Discord: Idle (connects when a player starts)",
        ]
    );
}

#[test]
fn playing_and_paused_get_their_own_marks() {
    let am = playing("Song", "Artist");
    assert_eq!(lines(&input(&am))[0], "♪ Apple Music: Song — Artist");

    let am = paused("Song", "Artist");
    assert_eq!(lines(&input(&am))[0], "⏸ Apple Music: Song — Artist");
}

#[test]
fn a_running_but_stopped_player_says_so() {
    assert_eq!(lines(&input(&stopped()))[0], "Apple Music: Stopped");
}

#[test]
fn a_track_without_an_artist_drops_the_dash() {
    let am = playing("Song", "");
    assert_eq!(lines(&input(&am))[0], "♪ Apple Music: Song");
}

#[test]
fn the_discord_row_follows_the_connection_once_a_player_is_up() {
    let am = playing("Song", "Artist");

    for (state, expected) in [
        (ConnState::Connected, "Discord: Connected"),
        (ConnState::Connecting, "Discord: Connecting…"),
        (ConnState::Disconnected, "Discord: Disconnected (retrying)"),
    ] {
        let mut i = input(&am);
        i.discord_state = state;
        assert_eq!(lines(&i).last().unwrap(), expected);
    }
}

#[test]
fn the_discord_row_says_idle_while_nothing_is_playing() {
    let off = MusicState::default();
    let mut i = input(&off);
    // Even mid-handshake: with no player running there is nothing to report.
    i.discord_state = ConnState::Connecting;
    assert_eq!(
        lines(&i).last().unwrap(),
        "Discord: Idle (connects when a player starts)"
    );
}

#[test]
fn japanese_translates_the_source_row_but_never_the_discord_row() {
    let am = stopped();
    let mut i = input(&am);
    i.language = AppLanguage::Ja;
    i.discord_state = ConnState::Connected;

    let rows = lines(&i);
    assert_eq!(rows[0], "Apple Music: 停止中");
    // Discord-facing wording stays English on both platforms.
    assert_eq!(rows[1], "Discord: Connected");
}
