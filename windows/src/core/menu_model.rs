//! What the tray menu contains, as data.
//!
//! The same split `status_lines` uses: this decides the rows and their labels
//! with no OS call in sight, and a thin renderer turns them into a real menu.
//! That keeps "what does the menu say right now" answerable by a unit test
//! rather than only by opening it.
//!
//! macOS rebuilds its menu from scratch on every open so nothing is kept
//! resident between them. Windows has no "about to open" hook — the shell pops
//! the menu synchronously from the tray window's own message handler — so the
//! rows are rebuilt whenever the state behind them changes instead. Same
//! result, and the work lands on state changes rather than on the click.

use super::i18n::{t, AppLanguage};
use super::models::MusicSourceId;
use super::settings_model::{BadgeLabelType, LinkType, Settings, PAUSE_HIDE_CHOICES};
use super::status_lines;

/// Which of the two configurable Discord buttons a row is about.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum ButtonSlot {
    One,
    Two,
}

impl ButtonSlot {
    pub const ALL: [ButtonSlot; 2] = [ButtonSlot::One, ButtonSlot::Two];

    fn number(self) -> u8 {
        match self {
            ButtonSlot::One => 1,
            ButtonSlot::Two => 2,
        }
    }
}

/// Something the user asked for by clicking a row.
///
/// The `Edit*` variants only say which dialog to open: the text comes back
/// through the platform layer, which then applies the matching setter.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum MenuAction {
    SetButtonType {
        button: ButtonSlot,
        link_type: LinkType,
    },
    EditButtonLabel(ButtonSlot),
    EditCustomUrl,
    EditRepositoryUrl,
    ToggleSource(MusicSourceId),
    SetBadgeLabel(BadgeLabelType),
    SetPauseHideMinutes(i32),
    SetLanguage(AppLanguage),
    ToggleLaunchAtLogin,
    Reconnect,
    CheckForUpdates,
    /// Opens the release page for an update already known to exist.
    OpenReleasePage,
    Quit,
}

/// One row of the menu, in display order.
#[derive(Debug, Clone, PartialEq)]
pub enum MenuRow {
    /// A greyed-out line; not clickable. Status lines, headings, and options
    /// that are currently meaningless all land here.
    Info(String),
    Separator,
    /// A plain command.
    Item {
        label: String,
        action: MenuAction,
    },
    /// One of a set the user picks from; `checked` marks the current state.
    Choice {
        label: String,
        action: MenuAction,
        checked: bool,
    },
    Submenu {
        label: String,
        rows: Vec<MenuRow>,
    },
}

pub struct MenuInput<'a> {
    pub status: status_lines::Input<'a>,
    pub settings: &'a Settings,
    /// Read from the OS rather than from settings — the user can turn this off
    /// from Task Manager, and the menu should agree with whatever it says.
    pub launch_at_login: bool,
    /// The tag of a newer release, once one has been found.
    pub update_available: Option<&'a str>,
}

pub fn build_menu(input: &MenuInput) -> Vec<MenuRow> {
    let language = input.settings.language;
    let mut rows: Vec<MenuRow> = status_lines::lines(&input.status)
        .into_iter()
        .map(MenuRow::Info)
        .collect();
    rows.push(MenuRow::Separator);

    for slot in ButtonSlot::ALL {
        rows.push(button_submenu(slot, input.settings, language));
    }
    rows.push(MenuRow::Separator);

    rows.push(MenuRow::Item {
        label: t(language, "Set Custom URL…", "カスタムURLを設定…"),
        action: MenuAction::EditCustomUrl,
    });
    rows.push(MenuRow::Item {
        label: t(language, "Set Repository URL…", "リポジトリURLを設定…"),
        action: MenuAction::EditRepositoryUrl,
    });
    rows.push(sources_submenu(input.settings, language));
    rows.push(badge_submenu(input.settings, language));
    rows.push(pause_submenu(input.settings, language));
    rows.push(language_submenu(language));
    rows.push(MenuRow::Choice {
        label: t(language, "Launch at Login", "ログイン時に自動起動"),
        action: MenuAction::ToggleLaunchAtLogin,
        checked: input.launch_at_login,
    });
    rows.push(MenuRow::Separator);

    rows.push(MenuRow::Item {
        // Not translated, matching macOS: everything naming Discord stays in
        // Discord's own language.
        label: "Reconnect to Discord".to_string(),
        action: MenuAction::Reconnect,
    });
    rows.push(MenuRow::Separator);

    // An available update gets its own row above the check, so the news is not
    // hidden behind the same wording that means "go and look".
    match input.update_available {
        Some(tag) => rows.push(MenuRow::Item {
            label: t(
                language,
                &format!("Update Available: {tag}"),
                &format!("アップデートがあります: {tag}"),
            ),
            action: MenuAction::OpenReleasePage,
        }),
        None => rows.push(MenuRow::Item {
            label: t(language, "Check for Updates…", "アップデートを確認…"),
            action: MenuAction::CheckForUpdates,
        }),
    }
    rows.push(MenuRow::Separator);

    rows.push(MenuRow::Item {
        label: t(language, "Quit Ceyrad", "Ceyradを終了"),
        action: MenuAction::Quit,
    });
    rows
}

fn button_submenu(slot: ButtonSlot, settings: &Settings, language: AppLanguage) -> MenuRow {
    let current = button_type(slot, settings);
    let number = slot.number();
    let type_name = current.display_name(language);

    let mut rows = vec![MenuRow::Info(t(language, "Link Destination", "リンク先"))];
    for candidate in LinkType::SELECTABLE {
        // "Off" is not another destination but a way to drop the button, so it
        // is set apart the way macOS sets it apart.
        if candidate == LinkType::Disabled {
            rows.push(MenuRow::Separator);
        }
        rows.push(MenuRow::Choice {
            label: candidate.display_name(language),
            action: MenuAction::SetButtonType {
                button: slot,
                link_type: candidate,
            },
            checked: candidate == current,
        });
    }

    rows.push(MenuRow::Separator);
    if current == LinkType::Disabled {
        // Nothing is showing the label, so offering to change it would only
        // mislead.
        rows.push(MenuRow::Info(t(language, "Change Label…", "ラベルを変更…")));
    } else {
        let label = button_label(slot, settings);
        rows.push(MenuRow::Item {
            label: t(
                language,
                &format!("Change Label… (\"{label}\")"),
                &format!("ラベルを変更…（\"{label}\"）"),
            ),
            action: MenuAction::EditButtonLabel(slot),
        });
    }

    MenuRow::Submenu {
        label: t(
            language,
            &format!("Button {number}: {type_name}"),
            &format!("ボタン{number}: {type_name}"),
        ),
        rows,
    }
}

fn sources_submenu(settings: &Settings, language: AppLanguage) -> MenuRow {
    MenuRow::Submenu {
        label: t(language, "Music Sources", "ミュージックソース"),
        rows: MusicSourceId::ALL
            .into_iter()
            .map(|source| MenuRow::Choice {
                label: source.display_name().to_string(),
                action: MenuAction::ToggleSource(source),
                checked: settings.is_source_enabled(source),
            })
            .collect(),
    }
}

fn badge_submenu(settings: &Settings, language: AppLanguage) -> MenuRow {
    let current = settings.badge_label;
    let name = current.display_name(language);
    MenuRow::Submenu {
        label: t(
            language,
            &format!("Status Badge: {name}"),
            &format!("ステータスバッジ: {name}"),
        ),
        rows: BadgeLabelType::ALL
            .into_iter()
            .map(|candidate| MenuRow::Choice {
                label: candidate.display_name(language),
                action: MenuAction::SetBadgeLabel(candidate),
                checked: candidate == current,
            })
            .collect(),
    }
}

fn pause_submenu(settings: &Settings, language: AppLanguage) -> MenuRow {
    let current = settings.pause_hide_minutes;
    let name = pause_choice_name(current, language);
    MenuRow::Submenu {
        label: t(
            language,
            &format!("When Paused: {name}"),
            &format!("一時停止時: {name}"),
        ),
        rows: PAUSE_HIDE_CHOICES
            .into_iter()
            .map(|minutes| MenuRow::Choice {
                label: pause_choice_name(minutes, language),
                action: MenuAction::SetPauseHideMinutes(minutes),
                checked: minutes == current,
            })
            .collect(),
    }
}

/// Names the two sentinel values as behaviours rather than as numbers, which
/// is the only way "-1" and "0" read as anything.
pub fn pause_choice_name(minutes: i32, language: AppLanguage) -> String {
    match minutes {
        -1 => t(language, "Keep Showing", "表示し続ける"),
        0 => t(language, "Hide Immediately", "すぐに消す"),
        1 => t(language, "Hide After 1 Minute", "1分後に消す"),
        n => t(
            language,
            &format!("Hide After {n} Minutes"),
            &format!("{n}分後に消す"),
        ),
    }
}

fn language_submenu(current: AppLanguage) -> MenuRow {
    MenuRow::Submenu {
        label: t(
            current,
            &format!("Language: {}", current.display_name()),
            &format!("言語: {}", current.display_name()),
        ),
        rows: AppLanguage::ALL
            .into_iter()
            .map(|candidate| MenuRow::Choice {
                label: candidate.display_name().to_string(),
                action: MenuAction::SetLanguage(candidate),
                checked: candidate == current,
            })
            .collect(),
    }
}

fn button_type(slot: ButtonSlot, settings: &Settings) -> LinkType {
    match slot {
        ButtonSlot::One => settings.button1_type,
        ButtonSlot::Two => settings.button2_type,
    }
}

/// The label as the menu shows it, which is the Apple Music wording for an
/// uncustomized song button — same as macOS's menu-facing getter.
fn button_label(slot: ButtonSlot, settings: &Settings) -> String {
    match slot {
        ButtonSlot::One => settings.button1_label(None),
        ButtonSlot::Two => settings.button2_label(None),
    }
}

// MARK: - Row identity
//
// A renderer attaches these to rows and hands them back on click, so no
// numeric command ids have to be tracked anywhere. The vocabulary is owned
// here rather than borrowed from the settings file's serde names: the two
// serve different readers and should be free to diverge.

pub fn action_id(action: MenuAction) -> String {
    match action {
        MenuAction::SetButtonType { button, link_type } => {
            format!("button{}.type.{}", button.number(), link_slug(link_type))
        }
        MenuAction::EditButtonLabel(button) => format!("button{}.label", button.number()),
        MenuAction::EditCustomUrl => "url.custom".into(),
        MenuAction::EditRepositoryUrl => "url.repository".into(),
        MenuAction::ToggleSource(source) => format!("source.{}", source_slug(source)),
        MenuAction::SetBadgeLabel(badge) => format!("badge.{}", badge_slug(badge)),
        MenuAction::SetPauseHideMinutes(minutes) => format!("pause.{minutes}"),
        MenuAction::SetLanguage(language) => format!("language.{}", language_slug(language)),
        MenuAction::ToggleLaunchAtLogin => "launch_at_login".into(),
        MenuAction::Reconnect => "reconnect".into(),
        MenuAction::CheckForUpdates => "update.check".into(),
        MenuAction::OpenReleasePage => "update.open".into(),
        MenuAction::Quit => "quit".into(),
    }
}

pub fn parse_action_id(id: &str) -> Option<MenuAction> {
    match id {
        "url.custom" => return Some(MenuAction::EditCustomUrl),
        "url.repository" => return Some(MenuAction::EditRepositoryUrl),
        "launch_at_login" => return Some(MenuAction::ToggleLaunchAtLogin),
        "reconnect" => return Some(MenuAction::Reconnect),
        "update.check" => return Some(MenuAction::CheckForUpdates),
        "update.open" => return Some(MenuAction::OpenReleasePage),
        "quit" => return Some(MenuAction::Quit),
        _ => {}
    }

    let (head, tail) = id.split_once('.')?;
    match head {
        "button1" | "button2" => {
            let button = if head == "button1" {
                ButtonSlot::One
            } else {
                ButtonSlot::Two
            };
            if tail == "label" {
                return Some(MenuAction::EditButtonLabel(button));
            }
            let link_type = link_from_slug(tail.strip_prefix("type.")?)?;
            Some(MenuAction::SetButtonType { button, link_type })
        }
        "source" => Some(MenuAction::ToggleSource(source_from_slug(tail)?)),
        "badge" => Some(MenuAction::SetBadgeLabel(badge_from_slug(tail)?)),
        // Parsed rather than matched against the list: the choices are the
        // menu's business, and a value that is no longer offered is still a
        // value this can apply.
        "pause" => Some(MenuAction::SetPauseHideMinutes(tail.parse().ok()?)),
        "language" => Some(MenuAction::SetLanguage(language_from_slug(tail)?)),
        _ => None,
    }
}

fn link_slug(link_type: LinkType) -> &'static str {
    match link_type {
        LinkType::Song => "song",
        LinkType::Artist => "artist",
        LinkType::Album => "album",
        LinkType::Custom => "custom",
        LinkType::Repository => "repository",
        LinkType::Disabled => "disabled",
    }
}

fn link_from_slug(slug: &str) -> Option<LinkType> {
    LinkType::SELECTABLE
        .into_iter()
        .find(|candidate| link_slug(*candidate) == slug)
}

fn source_slug(source: MusicSourceId) -> &'static str {
    match source {
        MusicSourceId::AppleMusic => "apple_music",
        MusicSourceId::Spotify => "spotify",
    }
}

fn source_from_slug(slug: &str) -> Option<MusicSourceId> {
    MusicSourceId::ALL
        .into_iter()
        .find(|candidate| source_slug(*candidate) == slug)
}

fn badge_slug(badge: BadgeLabelType) -> &'static str {
    match badge {
        BadgeLabelType::AppName => "app_name",
        BadgeLabelType::Artist => "artist",
        BadgeLabelType::Track => "track",
    }
}

fn badge_from_slug(slug: &str) -> Option<BadgeLabelType> {
    BadgeLabelType::ALL
        .into_iter()
        .find(|candidate| badge_slug(*candidate) == slug)
}

fn language_slug(language: AppLanguage) -> &'static str {
    match language {
        AppLanguage::En => "en",
        AppLanguage::Ja => "ja",
    }
}

fn language_from_slug(slug: &str) -> Option<AppLanguage> {
    AppLanguage::ALL
        .into_iter()
        .find(|candidate| language_slug(*candidate) == slug)
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::core::models::{ConnState, SourceState};

    fn menu(settings: &Settings, source: &SourceState) -> Vec<MenuRow> {
        menu_with(settings, source, false, None)
    }

    fn menu_with(
        settings: &Settings,
        source: &SourceState,
        launch_at_login: bool,
        update_available: Option<&str>,
    ) -> Vec<MenuRow> {
        build_menu(&MenuInput {
            status: status_lines::Input {
                apple_music: source,
                spotify: source,
                active_source: None,
                apple_music_enabled: settings.apple_music_enabled,
                spotify_enabled: settings.spotify_enabled,
                discord_state: ConnState::Disconnected,
                language: settings.language,
            },
            settings,
            launch_at_login,
            update_available,
        })
    }

    fn default_menu() -> Vec<MenuRow> {
        menu(&Settings::default(), &SourceState::default())
    }

    /// Every row anywhere in the tree, flattened.
    fn walk(rows: &[MenuRow], out: &mut Vec<MenuRow>) {
        for row in rows {
            out.push(row.clone());
            if let MenuRow::Submenu { rows, .. } = row {
                walk(rows, out);
            }
        }
    }

    fn all_rows(rows: &[MenuRow]) -> Vec<MenuRow> {
        let mut out = Vec::new();
        walk(rows, &mut out);
        out
    }

    fn submenu<'a>(rows: &'a [MenuRow], label_starts_with: &str) -> &'a [MenuRow] {
        rows.iter()
            .find_map(|row| match row {
                MenuRow::Submenu { label, rows } if label.starts_with(label_starts_with) => {
                    Some(rows.as_slice())
                }
                _ => None,
            })
            .unwrap_or_else(|| panic!("no submenu starting with {label_starts_with:?}"))
    }

    fn checked_labels(rows: &[MenuRow]) -> Vec<&str> {
        rows.iter()
            .filter_map(|row| match row {
                MenuRow::Choice {
                    label,
                    checked: true,
                    ..
                } => Some(label.as_str()),
                _ => None,
            })
            .collect()
    }

    #[test]
    fn the_status_rows_come_first_and_are_not_clickable() {
        let settings = Settings::default();
        let source = SourceState::default();
        let rows = menu(&settings, &source);
        let expected = status_lines::lines(&status_lines::Input {
            apple_music: &source,
            spotify: &source,
            active_source: None,
            apple_music_enabled: true,
            spotify_enabled: false,
            discord_state: ConnState::Disconnected,
            language: AppLanguage::En,
        });

        for (row, line) in rows.iter().zip(&expected) {
            assert_eq!(row, &MenuRow::Info(line.clone()));
        }
        assert_eq!(rows[expected.len()], MenuRow::Separator);
    }

    #[test]
    fn every_action_round_trips_through_its_id() {
        // Covers the whole live menu rather than a hand-written list, so a row
        // added without an id mapping fails here.
        let mut seen = 0;
        for row in all_rows(&default_menu()) {
            let action = match row {
                MenuRow::Item { action, .. } | MenuRow::Choice { action, .. } => action,
                _ => continue,
            };
            let id = action_id(action);
            assert_eq!(parse_action_id(&id), Some(action), "id {id:?}");
            seen += 1;
        }
        assert!(seen > 20, "expected a full menu, saw {seen} actions");
    }

    #[test]
    fn ids_are_unique_across_the_whole_menu() {
        // Two rows sharing an id would silently trigger each other's action.
        let mut ids: Vec<String> = all_rows(&default_menu())
            .iter()
            .filter_map(|row| match row {
                MenuRow::Item { action, .. } | MenuRow::Choice { action, .. } => {
                    Some(action_id(*action))
                }
                _ => None,
            })
            .collect();
        let total = ids.len();
        ids.sort();
        ids.dedup();
        assert_eq!(ids.len(), total, "duplicate menu ids");
    }

    #[test]
    fn an_unknown_id_is_rejected_rather_than_guessed() {
        // Ids muda mints for rows we did not label — the status lines — arrive
        // here too, and must not resolve to an action.
        for id in [
            "",
            "1000",
            "Quit",
            "button3.label",
            "badge.nonsense",
            "pause.",
        ] {
            assert_eq!(parse_action_id(id), None, "id {id:?}");
        }
    }

    #[test]
    fn each_button_submenu_marks_its_current_destination() {
        let mut settings = Settings::default();
        settings.set_button1_type(LinkType::Album);
        let rows = menu(&settings, &SourceState::default());

        let button1 = submenu(&rows, "Button 1:");
        assert_eq!(checked_labels(button1), vec!["Album Page"]);
        // Button 2 keeps its own default, untouched by button 1.
        assert_eq!(
            checked_labels(submenu(&rows, "Button 2:")),
            vec!["Repository"]
        );
    }

    #[test]
    fn the_button_submenu_is_titled_with_its_destination() {
        let rows = default_menu();
        assert!(rows.iter().any(|row| matches!(
            row,
            MenuRow::Submenu { label, .. } if label == "Button 1: Song Page"
        )));
    }

    #[test]
    fn the_label_row_shows_the_label_it_would_change() {
        let rows = default_menu();
        let button1 = submenu(&rows, "Button 1:");
        assert!(button1.iter().any(|row| matches!(
            row,
            MenuRow::Item { label, action: MenuAction::EditButtonLabel(ButtonSlot::One) }
                if label == "Change Label… (\"Play on Apple Music\")"
        )));
    }

    #[test]
    fn a_disabled_button_offers_no_label_to_change() {
        let mut settings = Settings::default();
        settings.set_button1_type(LinkType::Disabled);
        let rows = menu(&settings, &SourceState::default());
        let button1 = submenu(&rows, "Button 1:");

        assert!(
            !button1.iter().any(|row| matches!(
                row,
                MenuRow::Item {
                    action: MenuAction::EditButtonLabel(_),
                    ..
                }
            )),
            "the label is not shown anywhere, so it must not be editable"
        );
        assert!(button1.contains(&MenuRow::Info("Change Label…".to_string())));
    }

    #[test]
    fn off_is_set_apart_from_the_real_destinations() {
        let rows = default_menu();
        let button1 = submenu(&rows, "Button 1:");
        let off = button1
            .iter()
            .position(|row| matches!(row, MenuRow::Choice { label, .. } if label == "Off"))
            .expect("an Off row");
        assert_eq!(button1[off - 1], MenuRow::Separator);
    }

    #[test]
    fn sources_show_which_are_being_watched() {
        let rows = default_menu();
        // Spotify is off by default: Discord ships its own integration.
        assert_eq!(
            checked_labels(submenu(&rows, "Music Sources")),
            vec!["Apple Music"]
        );
    }

    #[test]
    fn the_pause_submenu_names_behaviours_not_numbers() {
        let rows = default_menu();
        let pause = submenu(&rows, "When Paused:");
        let labels: Vec<&str> = pause
            .iter()
            .filter_map(|row| match row {
                MenuRow::Choice { label, .. } => Some(label.as_str()),
                _ => None,
            })
            .collect();
        assert_eq!(
            labels,
            vec![
                "Keep Showing",
                "Hide Immediately",
                "Hide After 1 Minute",
                "Hide After 3 Minutes",
                "Hide After 5 Minutes",
                "Hide After 10 Minutes",
            ]
        );
        assert_eq!(checked_labels(pause), vec!["Hide After 5 Minutes"]);
    }

    #[test]
    fn japanese_translates_the_menu_but_never_discord_wording() {
        let mut settings = Settings::default();
        settings.language = AppLanguage::Ja;
        let rows = menu(&settings, &SourceState::default());
        let flat = all_rows(&rows);

        let has = |text: &str| {
            flat.iter().any(|row| match row {
                MenuRow::Info(label)
                | MenuRow::Item { label, .. }
                | MenuRow::Choice { label, .. }
                | MenuRow::Submenu { label, .. } => label == text,
                MenuRow::Separator => false,
            })
        };
        assert!(has("ボタン1: 曲ページ"));
        assert!(has("ミュージックソース"));
        assert!(has("一時停止時: 5分後に消す"));
        assert!(has("Ceyradを終了"));
        // Discord's own name and state stay in English on both platforms.
        assert!(has("Reconnect to Discord"));
    }

    #[test]
    fn launch_at_login_reflects_the_os_not_the_settings_file() {
        let settings = Settings::default();
        let source = SourceState::default();
        let off = menu_with(&settings, &source, false, None);
        let on = menu_with(&settings, &source, true, None);

        let checked = |rows: &[MenuRow]| {
            rows.iter().any(|row| {
                matches!(
                    row,
                    MenuRow::Choice {
                        action: MenuAction::ToggleLaunchAtLogin,
                        checked: true,
                        ..
                    }
                )
            })
        };
        assert!(!checked(&off));
        assert!(checked(&on));
    }

    #[test]
    fn with_no_update_known_the_row_offers_to_look() {
        let rows = default_menu();
        assert!(rows.iter().any(|row| matches!(
            row,
            MenuRow::Item { label, action: MenuAction::CheckForUpdates }
                if label == "Check for Updates…"
        )));
    }

    #[test]
    fn a_known_update_replaces_the_check_row_and_names_the_version() {
        let rows = menu_with(
            &Settings::default(),
            &SourceState::default(),
            false,
            Some("v0.2.0"),
        );
        assert!(rows.iter().any(|row| matches!(
            row,
            MenuRow::Item { label, action: MenuAction::OpenReleasePage }
                if label == "Update Available: v0.2.0"
        )));
        // Two rows meaning "updates" would read as a choice the user does not
        // actually have.
        assert!(!rows.iter().any(|row| matches!(
            row,
            MenuRow::Item {
                action: MenuAction::CheckForUpdates,
                ..
            }
        )));
    }

    #[test]
    fn the_language_row_names_each_language_in_itself() {
        let rows = default_menu();
        let languages = submenu(&rows, "Language:");
        let labels: Vec<&str> = languages
            .iter()
            .filter_map(|row| match row {
                MenuRow::Choice { label, .. } => Some(label.as_str()),
                _ => None,
            })
            .collect();
        // Not translated: someone who cannot read the current language has to
        // be able to find their own.
        assert_eq!(labels, vec!["English", "日本語"]);
    }
}
