//! The orchestrator: SMTC in, Discord Rich Presence out.
//!
//! A port of the macOS `AppDelegate`, minus the tray UI. Everything runs on one
//! thread driven by a single wait: Discord's pipe, the SMTC wakeup event and
//! the app's own timers are all waited on together, so an idle app performs no
//! work at all — no polling loop, no periodic timer, no wakeups between songs.

pub mod settings_store;

use std::collections::HashSet;
use std::io;
use std::sync::atomic::{AtomicBool, AtomicIsize, Ordering};
use std::sync::Arc;
use std::time::{Duration, Instant, SystemTime};

use serde_json::{Map, Value};
use windows::core::BOOL;
use windows::Win32::Foundation::HANDLE;
use windows::Win32::System::Console::SetConsoleCtrlHandler;
use windows::Win32::System::SystemInformation::GetLocalTime;
use windows::Win32::System::Threading::SetEvent;

use crate::catalog::{Outcome, Request as CatalogRequest, Resolver};
use crate::core::activity_builder;
use crate::core::aumid::AumidOverrides;
use crate::core::debouncer::Debouncer;
use crate::core::i18n::t;
use crate::core::menu_model::{self, ButtonSlot, MenuAction};
use crate::core::models::{ConnState, MusicSourceId, PlayerState, SourceState, SourceStates};
use crate::core::settings_model::Settings;
use crate::core::status_lines;
use crate::core::track_change::{position_jumped, same_identity};
use crate::core::update_check::Release;
use crate::core::url_prompt;
use crate::discord::client::{Client, Event as DiscordEvent};
use crate::discord::pipe::Event as WakeEvent;
use crate::launch_at_login;
use crate::smtc::Watcher;
use crate::tray::{self, dialog, Tray};
use crate::updater::{self, Outcome as UpdateOutcome, Updater};

/// Matches the macOS build: collapses a burst of player events into one send.
const DEBOUNCE: Duration = Duration::from_millis(800);
/// Ceiling on how long that coalescing may postpone a send; see `Debouncer`.
const MAX_DEBOUNCE: Duration = Duration::from_millis(2_000);
/// Reconnect backoff, 1s doubling to this ceiling.
const MAX_RECONNECT_DELAY_SECS: u64 = 60;
/// How long Discord gets to answer a handshake with READY.
///
/// A pipe that opens but never replies would otherwise leave the client in
/// `Connecting` for good: nothing times it out, and the reconnect backoff only
/// arms on a connection that actually dropped.
const HANDSHAKE_TIMEOUT: Duration = Duration::from_secs(10);
/// How long to leave a failed catalog lookup alone before asking again. Long
/// enough that a spell offline does not turn into a request per SMTC event.
const CATALOG_RETRY_DELAY: Duration = Duration::from_secs(15);
/// How long after launch to ask about updates, so the check never competes with
/// getting a presence on screen.
const FIRST_UPDATE_CHECK_DELAY: Duration = Duration::from_secs(30);
/// Matching macOS's `SUScheduledCheckInterval`.
const UPDATE_CHECK_INTERVAL: Duration = Duration::from_secs(24 * 60 * 60);

static SHUTDOWN: AtomicBool = AtomicBool::new(false);
/// The wake event, as a raw handle so the console handler — which cannot
/// capture anything — can nudge the event loop out of its wait.
static WAKE_HANDLE: AtomicIsize = AtomicIsize::new(0);

pub struct App {
    settings: Settings,
    overrides: AumidOverrides,
    sources: SourceStates,
    active_source: Option<MusicSourceId>,

    discord: Client,
    watcher: Watcher,
    catalog: Resolver,
    updater: Updater,
    wake: Arc<WakeEvent>,
    started: Instant,
    /// Absent in the console build, which has no UI at all.
    tray: Option<Tray>,

    debouncer: Debouncer,
    pending_activity: Option<Option<Map<String, Value>>>,
    /// What Discord is currently showing, so an unchanged rebuild is not resent.
    last_sent: Option<Option<Map<String, Value>>>,

    pause_hide_at: Option<Instant>,
    paused_timed_out: bool,

    reconnect_at: Option<Instant>,
    reconnect_attempt: u32,
    handshake_deadline: Option<Instant>,

    update_check_at: Option<Instant>,
    /// The newest release found so far, kept so the menu can offer it and the
    /// click knows where to send the browser.
    update_available: Option<Release>,
    /// Whether the user asked, so an "already up to date" answer is only
    /// reported back when someone is waiting for it.
    update_check_requested: bool,

    logged_unknown_aumids: HashSet<String>,
    last_status: Vec<String>,
    /// The menu shows settings as well as status, so a change the status rows
    /// cannot express still has to reach it.
    menu_dirty: bool,
}

impl App {
    pub fn new() -> io::Result<Self> {
        let (settings, warning) = settings_store::load();
        if let Some(warning) = warning {
            log(&format!("settings: {warning}"));
        }
        match settings_store::settings_path() {
            Some(path) => log(&format!("settings: {}", path.display())),
            None => log("settings: APPDATA not set; changes will not persist"),
        }

        let overrides = settings_store::aumid_overrides_from_env();
        if !overrides.is_empty() {
            log(&format!(
                "AUMID overrides: apple music {:?}, spotify {:?}",
                overrides.apple_music, overrides.spotify
            ));
        }

        let wake = Arc::new(WakeEvent::new()?);
        WAKE_HANDLE.store(wake.handle().0 as isize, Ordering::SeqCst);

        let watcher = Watcher::new(Arc::clone(&wake)).map_err(io::Error::other)?;

        // Nothing keeps the Run entry in step with a portable exe that moved, so
        // it is checked here rather than left to fail quietly at the next logon.
        if let Err(e) = launch_at_login::reconcile_path() {
            log(&format!("launch at login: could not update the path ({e})"));
        }

        Ok(Self {
            settings,
            overrides,
            sources: SourceStates::default(),
            active_source: None,
            discord: Client::new(),
            watcher,
            catalog: Resolver::new(Arc::clone(&wake)),
            updater: Updater::new(Arc::clone(&wake)),
            wake,
            started: Instant::now(),
            tray: None,
            debouncer: Debouncer::new(DEBOUNCE, MAX_DEBOUNCE),
            pending_activity: None,
            last_sent: None,
            pause_hide_at: None,
            paused_timed_out: false,
            reconnect_at: None,
            reconnect_attempt: 0,
            handshake_deadline: None,
            update_check_at: Some(Instant::now() + FIRST_UPDATE_CHECK_DELAY),
            update_available: None,
            update_check_requested: false,
            logged_unknown_aumids: HashSet::new(),
            last_status: Vec::new(),
            menu_dirty: false,
        })
    }

    /// Gives the app a notification-area presence. Without this it runs
    /// headless, which is what the console build wants.
    pub fn attach_tray(&mut self, tray: Tray) {
        self.tray = Some(tray);
    }

    pub fn run(&mut self) -> io::Result<()> {
        install_ctrl_handler();
        log("watching for players — Ctrl+C to stop");

        while !SHUTDOWN.load(Ordering::SeqCst) {
            // First, because the wait below includes window messages: anything
            // left queued would wake it again at once. A thread with no window
            // has nothing to drain and this costs a syscall.
            if !tray::pump_messages() {
                log("session ending");
                break;
            }
            // Consumed before the dirty flags are read: a handler that fires in
            // between leaves the flag set for this pass, or re-signals for the
            // next one. Neither loses an event.
            self.wake.reset()?;

            if self.watcher.is_dirty() {
                self.handle_smtc();
            }
            self.drain_catalog();
            self.drain_updates();
            self.fire_due_timers();
            // After the pump, which is what turns a click into an event.
            while let Some(action) = tray::try_recv_action() {
                self.apply_menu_action(action);
            }
            self.report_status();

            let timeout = self
                .next_deadline()
                .map(|deadline| deadline.saturating_duration_since(Instant::now()));

            if self.discord.state() == ConnState::Disconnected {
                self.wake.wait(timeout);
            } else {
                let mut events = Vec::new();
                let result = self
                    .discord
                    .poll(Some(self.wake.handle()), timeout, &mut events);
                for event in events {
                    self.handle_discord_event(event);
                }
                if let Err(e) = result {
                    log(&format!("discord: {e}"));
                }
            }
        }

        log("clearing presence and shutting down");
        self.discord.disconnect(true);
        Ok(())
    }

    // MARK: - Menu

    pub fn apply_menu_action(&mut self, action: MenuAction) {
        match action {
            MenuAction::SetButtonType { button, link_type } => {
                match button {
                    ButtonSlot::One => self.settings.set_button1_type(link_type),
                    ButtonSlot::Two => self.settings.set_button2_type(link_type),
                }
                self.settings_changed();
            }
            MenuAction::EditButtonLabel(button) => self.edit_button_label(button),
            MenuAction::EditCustomUrl => self.edit_custom_url(),
            MenuAction::EditRepositoryUrl => self.edit_repository_url(),
            MenuAction::ToggleSource(source) => {
                let enabled = !self.settings.is_source_enabled(source);
                self.settings.set_source_enabled(source, enabled);
                self.settings_changed();
            }
            MenuAction::SetBadgeLabel(badge) => {
                self.settings.badge_label = badge;
                self.settings_changed();
            }
            MenuAction::SetPauseHideMinutes(minutes) => {
                self.settings.pause_hide_minutes = minutes;
                self.settings_changed();
            }
            MenuAction::SetLanguage(language) => {
                self.settings.language = language;
                // Only the menu's own wording changes, so there is nothing to
                // re-send to Discord — but the rows have to be rebuilt.
                self.persist_settings();
                self.menu_dirty = true;
            }
            MenuAction::ToggleLaunchAtLogin => self.toggle_launch_at_login(),
            MenuAction::CheckForUpdates => {
                log("checking for updates at your request");
                self.update_check_requested = true;
                self.updater.check();
            }
            MenuAction::OpenReleasePage => {
                if let Some(release) = &self.update_available {
                    log(&format!("opening {}", release.page_url));
                    if let Err(e) = updater::open_in_browser(&release.page_url) {
                        log(&format!("updater: {e}"));
                    }
                }
            }
            MenuAction::Reconnect => {
                // Tears the connection down first, unlike macOS, where the
                // menu item only helps if the app is already disconnected. A
                // button labelled "Reconnect" that does nothing when things
                // look fine is exactly the one a user reaches for when they
                // are not.
                log("reconnecting at your request");
                self.discord.disconnect(true);
                self.last_sent = None;
                self.handshake_deadline = None;
                self.cancel_reconnect();
                self.attempt_connect();
            }
            MenuAction::Quit => request_shutdown(),
        }
    }

    /// Flips the Run entry, then re-reads it.
    ///
    /// The menu shows what the registry actually says rather than what was just
    /// asked for, so a write that failed — a locked-down machine, a policy —
    /// leaves the checkmark off instead of lying about it.
    fn toggle_launch_at_login(&mut self) {
        let wanted = !launch_at_login::is_enabled();
        match launch_at_login::set_enabled(wanted) {
            Ok(()) => log(&format!(
                "launch at login: {}",
                if wanted { "on" } else { "off" }
            )),
            Err(e) => {
                log(&format!("launch at login: could not change it ({e})"));
                let language = self.settings.language;
                dialog::show_error(
                    &t(language, "Launch at Login", "ログイン時に自動起動"),
                    &t(
                        language,
                        "Windows would not let this setting be changed.",
                        "Windowsがこの設定の変更を許可しませんでした。",
                    ),
                );
            }
        }
        // Not a `Settings` field, so there is nothing to save — but the row has
        // to be rebuilt from the registry's new answer.
        self.menu_dirty = true;
    }

    fn edit_button_label(&mut self, button: ButtonSlot) {
        let language = self.settings.language;
        let number = match button {
            ButtonSlot::One => 1,
            ButtonSlot::Two => 2,
        };
        let current = match button {
            ButtonSlot::One => self.settings.button1_label(None),
            ButtonSlot::Two => self.settings.button2_label(None),
        };
        let Some(text) = dialog::prompt(
            &t(
                language,
                &format!("Button {number} Label"),
                &format!("ボタン{number}のラベル"),
            ),
            &t(
                language,
                "Text shown on the Discord button (up to 32 characters)",
                "Discordのボタンに表示されるテキスト（32文字まで）",
            ),
            &current,
        ) else {
            return;
        };
        // The setter truncates, and treats the type's own default as "not
        // customized" so the label keeps following the link destination.
        match button {
            ButtonSlot::One => self.settings.set_button1_label(&text),
            ButtonSlot::Two => self.settings.set_button2_label(&text),
        }
        self.settings_changed();
    }

    fn edit_custom_url(&mut self) {
        let language = self.settings.language;
        // Blank is meaningful here: it turns the custom link off.
        let Some(url) = self.prompt_for_url(
            &t(language, "Custom URL", "カスタムURL"),
            &t(
                language,
                "URL used when a button's link destination is \"Custom URL\" (http/https)",
                "ボタンのリンク先が「カスタムURL」のときに使うURL（http/https）",
            ),
            &self.settings.custom_url.clone(),
            true,
        ) else {
            return;
        };
        self.settings.custom_url = url;
        self.settings_changed();
    }

    fn edit_repository_url(&mut self) {
        let language = self.settings.language;
        let Some(url) = self.prompt_for_url(
            &t(language, "Repository URL", "リポジトリURL"),
            &t(
                language,
                "URL used when a button's link destination is \"Repository\"",
                "ボタンのリンク先が「リポジトリ」のときに使うURL",
            ),
            &self.settings.repository_url.clone(),
            false,
        ) else {
            return;
        };
        self.settings.repository_url = url;
        self.settings_changed();
    }

    fn prompt_for_url(
        &self,
        title: &str,
        message: &str,
        initial: &str,
        allow_empty: bool,
    ) -> Option<String> {
        let language = self.settings.language;
        url_prompt::prompt_valid_url(
            |current| dialog::prompt(title, message, current),
            || {
                dialog::show_error(
                    &t(language, "Invalid Input", "入力が無効です"),
                    &t(
                        language,
                        "Enter a URL of 512 characters or fewer, starting with http:// or https://.",
                        "http:// または https:// で始まる512文字以内のURLを入力してください。",
                    ),
                )
            },
            initial,
            allow_empty,
        )
    }

    /// macOS's `settingsChanged()`: save, then react as if the sources whose
    /// enabled flag just moved had appeared or vanished from SMTC.
    fn settings_changed(&mut self) {
        self.persist_settings();
        self.menu_dirty = true;

        // A source that was switched off is dropped as though its session had
        // gone; one switched on is picked up by the next snapshot, which the
        // watcher is asked for here rather than waited for.
        for source in MusicSourceId::ALL {
            if !self.settings.is_source_enabled(source) && self.sources.get(source).running {
                log(&format!("{}: no longer watched", source.display_name()));
                *self.sources.get_mut(source) = SourceState::default();
            }
        }
        self.watcher.mark_dirty();

        if !self.sources.any_running() {
            self.active_source = None;
            self.cancel_pause_timer();
            self.cancel_reconnect();
            self.cancel_debounce();
            if self.discord.state() != ConnState::Disconnected {
                log("no players left — disconnecting");
                self.discord.disconnect(true);
                self.last_sent = None;
            }
            return;
        }

        let selection = crate::core::source_selector::select_active_source(
            &self.sources.apple_music,
            &self.sources.spotify,
            self.active_source,
        );
        if selection != self.active_source {
            self.switch_active_source(selection);
        } else {
            self.attempt_connect();
            // The pause timeout may have been what changed, so restart it
            // rather than letting an old deadline stand.
            self.cancel_pause_timer();
            self.update_pause_timer();
            self.push_activity();
        }
    }

    fn persist_settings(&self) {
        match settings_store::save(&self.settings) {
            Ok(path) => log(&format!("settings: saved to {}", path.display())),
            Err(e) => log(&format!("settings: could not save ({e})")),
        }
    }

    // MARK: - Music

    fn handle_smtc(&mut self) {
        let snapshot = match self.watcher.take_snapshot(&self.overrides) {
            Ok(snapshot) => snapshot,
            Err(e) => {
                log(&format!("smtc: {e}"));
                return;
            }
        };

        for aumid in &snapshot.unknown_aumids {
            if self.logged_unknown_aumids.insert(aumid.clone()) {
                log(&format!("smtc: ignoring unrecognised session {aumid}"));
            }
        }

        let now = Instant::now();
        let now_ns = self.started.elapsed().as_nanos() as u64;
        let active = self.active_source;
        // SMTC re-reports a playing session several times a second, so "did
        // anything happen?" has to be decided from the values. Without this the
        // presence would be rebuilt and compared four times a second for a
        // track that is simply playing on.
        let mut active_changed = false;
        let mut active_track_changed = false;

        for source in MusicSourceId::ALL {
            let incoming = self
                .settings
                .is_source_enabled(source)
                .then(|| snapshot.get(source))
                .flatten();
            let state = self.sources.get_mut(source);

            match incoming {
                Some(session) => {
                    let same_track = same_identity(state.track.as_ref(), session.track.as_ref());
                    let seeked = position_jumped(state.track.as_ref(), session.track.as_ref());
                    let changed = !state.running
                        || state.player_state != session.player_state
                        || !same_track
                        || seeked;

                    if !state.running {
                        log(&format!("{}: session appeared", source.display_name()));
                    }
                    state.running = true;
                    state.player_state = session.player_state;
                    state.track = session.track.clone();
                    if !same_track {
                        state.catalog = None;
                        state.catalog_requested_for = None;
                        state.catalog_retry_at = None;
                    }

                    if changed {
                        state.last_event_uptime_ns = now_ns;
                        if Some(source) == active {
                            active_changed = true;
                            active_track_changed |= !same_track;
                        }
                    }
                }
                None => {
                    if state.running {
                        log(&format!("{}: session gone", source.display_name()));
                    }
                    *state = SourceState::default();
                }
            }
        }

        // Artwork and links are not in SMTC; they have to be looked up, which
        // takes a network round trip. The request goes out here and the
        // presence is sent without them in the meantime, so a slow lookup never
        // delays the card.
        self.request_missing_catalog(now);

        if !self.sources.any_running() {
            // Nothing left to report. The macOS build goes fully dormant here
            // rather than holding an idle Discord connection open.
            self.active_source = None;
            self.cancel_pause_timer();
            self.cancel_reconnect();
            self.cancel_debounce();
            if self.discord.state() != ConnState::Disconnected {
                log("no players left — disconnecting");
                self.discord.disconnect(true);
                self.last_sent = None;
            }
            return;
        }

        self.attempt_connect();

        let selection = crate::core::source_selector::select_active_source(
            &self.sources.apple_music,
            &self.sources.spotify,
            self.active_source,
        );
        if selection != self.active_source {
            self.switch_active_source(selection);
        } else if active_changed {
            // Skipping a track while paused counts as interacting with the
            // player, so a timed-out presence comes back and the clock restarts.
            if active_track_changed {
                self.cancel_pause_timer();
            }
            self.update_pause_timer();
            self.push_activity();
        }
        // An event from a source that is not on display, and that does not
        // change which source is, needs nothing sent.
    }

    /// Changing source usually means changing Discord application id, because
    /// "Listening to <name>" comes from the application behind the connection.
    fn switch_active_source(&mut self, new_source: Option<MusicSourceId>) {
        log(&format!(
            "active source: {}",
            new_source.map_or("none", MusicSourceId::display_name)
        ));
        self.active_source = new_source;
        self.cancel_debounce();
        self.cancel_pause_timer();
        self.update_pause_timer();

        let Some(new_source) = new_source else {
            // Players are still running, so the connection stays up; only the
            // presence is cleared.
            self.push_activity();
            return;
        };

        let wanted = new_source.discord_client_id();
        if self
            .discord
            .client_id()
            .is_some_and(|current| current != wanted)
        {
            self.discord.disconnect(true);
            self.last_sent = None;
            self.handshake_deadline = None;
            self.cancel_reconnect();
            self.attempt_connect();
        } else {
            self.push_activity();
        }
    }

    // MARK: - Catalog

    /// Asks for the artwork and links of anything playing that has neither.
    ///
    /// Called after every SMTC snapshot and whenever a retry falls due. The
    /// `catalog_requested_for` guard is what keeps SMTC's event rate from
    /// turning into a request rate.
    fn request_missing_catalog(&mut self, now: Instant) {
        for source in MusicSourceId::ALL {
            let state = self.sources.get_mut(source);
            if !state.running || state.catalog.is_some() {
                continue;
            }
            // A failed lookup is serving its cooling-off period.
            if state.catalog_retry_at.is_some_and(|at| at > now) {
                continue;
            }
            let Some(track) = state.track.as_ref() else {
                continue;
            };
            if track.name.is_empty() {
                continue;
            }
            let identity = track.identity();
            if state.catalog_requested_for.as_deref() == Some(identity.as_str()) {
                continue;
            }

            let request = CatalogRequest {
                source,
                key: identity.clone(),
                name: track.name.clone(),
                artist: track.artist.clone(),
                album: track.album.clone(),
            };
            state.catalog_requested_for = Some(identity);
            state.catalog_retry_at = None;
            self.catalog.request(request);
        }
    }

    /// Applies whatever the lookup thread has finished. A late answer is only
    /// used when the track it belongs to is still the one playing.
    fn drain_catalog(&mut self) {
        while let Some(resolved) = self.catalog.try_recv() {
            let state = self.sources.get_mut(resolved.source);
            let still_playing = state
                .track
                .as_ref()
                .is_some_and(|track| track.identity() == resolved.key);
            if !still_playing {
                continue;
            }

            let name = resolved.source.display_name();
            let mut resend = false;
            match resolved.outcome {
                Outcome::Found(catalog) => {
                    state.catalog = Some(catalog);
                    state.catalog_retry_at = None;
                    log(&format!("{name}: catalog resolved"));
                    // The presence has already gone out without artwork, so
                    // only a hit is worth the re-send.
                    resend = Some(resolved.source) == self.active_source;
                }
                Outcome::Missing => {
                    state.catalog = None;
                    state.catalog_retry_at = None;
                    log(&format!("{name}: catalog not found"));
                }
                Outcome::Failed => {
                    // Not an answer. Clearing the guard lets the track be asked
                    // about again once the cooling-off period is up, instead of
                    // spending the rest of the song with no artwork.
                    state.catalog_requested_for = None;
                    state.catalog_retry_at = Some(Instant::now() + CATALOG_RETRY_DELAY);
                    log(&format!(
                        "{name}: catalog lookup failed — retrying in {}s",
                        CATALOG_RETRY_DELAY.as_secs()
                    ));
                }
            }
            if resend {
                self.push_activity();
            }
        }
    }

    // MARK: - Updates

    /// Applies whatever the update thread found.
    ///
    /// Every outcome is logged — the log is the only place the reasoning shows,
    /// and "did the check even run?" has to be answerable. The *dialogs* are
    /// what stay quiet unless the user asked: a scheduled check that finds
    /// nothing, or cannot reach GitHub, has no business interrupting anyone.
    fn drain_updates(&mut self) {
        while let Some(outcome) = self.updater.try_recv() {
            let asked = std::mem::take(&mut self.update_check_requested);
            let language = self.settings.language;
            match outcome {
                UpdateOutcome::Available(release) => {
                    log(&format!(
                        "update available: {} (this build is {})",
                        release.tag,
                        updater::CURRENT_VERSION
                    ));
                    self.update_available = Some(release);
                    self.menu_dirty = true;
                }
                UpdateOutcome::UpToDate => {
                    log(&format!("up to date ({})", updater::CURRENT_VERSION));
                    // A release that was newer and no longer is means this build
                    // was replaced while running; drop the stale offer.
                    if self.update_available.take().is_some() {
                        self.menu_dirty = true;
                    }
                    if asked {
                        dialog::show_error(
                            &t(language, "Check for Updates", "アップデートを確認"),
                            &t(
                                language,
                                &format!(
                                    "Ceyrad {} is the latest version.",
                                    updater::CURRENT_VERSION
                                ),
                                &format!(
                                    "Ceyrad {} は最新バージョンです。",
                                    updater::CURRENT_VERSION
                                ),
                            ),
                        );
                    }
                }
                UpdateOutcome::Failed => {
                    // The worker already logged why; this says what it means.
                    log("could not check for updates");
                    if asked {
                        dialog::show_error(
                            &t(language, "Check for Updates", "アップデートを確認"),
                            &t(
                                language,
                                "Could not reach GitHub to check for updates.",
                                "GitHubに接続できず、アップデートを確認できませんでした。",
                            ),
                        );
                    }
                }
            }
        }
    }

    // MARK: - Activity

    fn push_activity(&mut self) {
        if !self.sources.any_running() {
            return;
        }
        self.pending_activity = Some(self.build_activity());
        self.debouncer.schedule(Instant::now());
    }

    fn cancel_debounce(&mut self) {
        self.debouncer.cancel();
        self.pending_activity = None;
    }

    fn build_activity(&self) -> Option<Map<String, Value>> {
        let source = self.active_source?;
        let state = self.sources.get(source);
        let track = state.track.as_ref()?;
        if !self.should_show_activity(state.player_state) {
            return None;
        }
        Some(activity_builder::build(
            track,
            state.player_state,
            state.catalog.as_ref(),
            &self.settings,
            source,
            SystemTime::now(),
        ))
    }

    fn should_show_activity(&self, player_state: PlayerState) -> bool {
        match player_state {
            PlayerState::Playing => true,
            PlayerState::Paused => self.settings.pause_hide_minutes != 0 && !self.paused_timed_out,
            PlayerState::Stopped => false,
        }
    }

    fn flush_activity(&mut self) {
        let Some(activity) = self.pending_activity.take() else {
            return;
        };
        if self.discord.state() != ConnState::Connected {
            // Nothing to send it over yet. Dropping it is safe: READY pushes a
            // freshly built presence, which is more current than this one.
            return;
        }
        if let Some(previous) = &self.last_sent {
            let unchanged = match (previous, &activity) {
                (None, None) => true,
                (Some(previous), Some(activity)) => {
                    activity_builder::is_equivalent(previous, activity)
                }
                _ => false,
            };
            if unchanged {
                return;
            }
        }

        let payload = activity.clone().map(Value::Object);
        match self.discord.set_activity(payload) {
            Ok(()) => {
                log(&format!("-> {}", describe(&activity)));
                self.last_sent = Some(activity);
            }
            Err(e) => log(&format!("discord: send failed ({e})")),
        }
    }

    // MARK: - Pause timeout

    fn update_pause_timer(&mut self) {
        let paused = self
            .active_source
            .map(|source| self.sources.get(source).player_state == PlayerState::Paused)
            .unwrap_or(false);
        if !paused {
            self.cancel_pause_timer();
            return;
        }
        // Already counting, or already elapsed: do not restart the clock.
        if self.pause_hide_at.is_some() || self.paused_timed_out {
            return;
        }
        // 0 hides immediately and -1 never hides; neither needs a timer.
        if self.settings.pause_hide_minutes > 0 {
            let minutes = self.settings.pause_hide_minutes as u64;
            self.pause_hide_at = Some(Instant::now() + Duration::from_secs(minutes * 60));
        }
    }

    fn cancel_pause_timer(&mut self) {
        self.pause_hide_at = None;
        self.paused_timed_out = false;
    }

    // MARK: - Connection

    fn attempt_connect(&mut self) {
        if !self.sources.any_running() || self.discord.state() != ConnState::Disconnected {
            return;
        }
        let fallback = if self.sources.apple_music.running {
            MusicSourceId::AppleMusic
        } else {
            MusicSourceId::Spotify
        };
        let source = self.active_source.unwrap_or(fallback);
        match self.discord.connect(source.discord_client_id()) {
            Ok(()) => {
                log(&format!("connecting as {}…", source.display_name()));
                self.handshake_deadline = Some(Instant::now() + HANDSHAKE_TIMEOUT);
            }
            Err(e) => {
                log(&format!("discord: {e}"));
                self.schedule_reconnect();
            }
        }
    }

    fn schedule_reconnect(&mut self) {
        self.handshake_deadline = None;
        if !self.sources.any_running() {
            return;
        }
        let delay = MAX_RECONNECT_DELAY_SECS.min(1u64 << self.reconnect_attempt.min(6));
        self.reconnect_attempt = (self.reconnect_attempt + 1).min(6);
        self.reconnect_at = Some(Instant::now() + Duration::from_secs(delay));
        log(&format!("discord: retrying in {delay}s"));
    }

    fn cancel_reconnect(&mut self) {
        self.reconnect_at = None;
        self.reconnect_attempt = 0;
    }

    fn handle_discord_event(&mut self, event: DiscordEvent) {
        match event {
            DiscordEvent::Ready => {
                log("discord: connected");
                self.reconnect_attempt = 0;
                self.handshake_deadline = None;
                // A fresh connection shows nothing, so the previous send must
                // not suppress the first push.
                self.last_sent = None;
                self.push_activity();
            }
            DiscordEvent::Closed => {
                log("discord: disconnected");
                self.last_sent = None;
                self.schedule_reconnect();
            }
            // Discord's own account of what it refused — an invalid client id,
            // a malformed activity, a rate limit. Nothing else reports it, and
            // with no tray UI this log is the only place it can surface.
            DiscordEvent::Error(message) => log(&format!("discord: {message}")),
        }
    }

    // MARK: - Loop plumbing

    fn fire_due_timers(&mut self) {
        let now = Instant::now();

        if self.debouncer.take_if_due(now) {
            self.flush_activity();
        }

        if self.pause_hide_at.is_some_and(|at| at <= now) {
            self.pause_hide_at = None;
            let still_paused = self
                .active_source
                .map(|source| self.sources.get(source).player_state == PlayerState::Paused)
                .unwrap_or(false);
            if still_paused {
                log("paused long enough — hiding presence");
                self.paused_timed_out = true;
                self.push_activity();
            }
        }

        if self.handshake_deadline.is_some_and(|at| at <= now) {
            self.handshake_deadline = None;
            if self.discord.state() == ConnState::Connecting {
                log("discord: no READY within the handshake window — giving up on this pipe");
                self.discord.disconnect(false);
                self.last_sent = None;
                self.schedule_reconnect();
            }
        }

        if self.reconnect_at.is_some_and(|at| at <= now) {
            self.reconnect_at = None;
            self.attempt_connect();
        }

        if self.update_check_at.is_some_and(|at| at <= now) {
            // Re-armed before the answer arrives, so a check that fails still
            // leaves the next one scheduled rather than ending the series.
            self.update_check_at = Some(now + UPDATE_CHECK_INTERVAL);
            log("checking for updates");
            self.updater.check();
        }

        // A due retry is consumed here whether or not it leads to a request, so
        // a deadline that is already past can never keep the loop spinning.
        let mut catalog_retry_due = false;
        for source in MusicSourceId::ALL {
            let state = self.sources.get_mut(source);
            if state.catalog_retry_at.is_some_and(|at| at <= now) {
                state.catalog_retry_at = None;
                catalog_retry_due = true;
            }
        }
        if catalog_retry_due {
            self.request_missing_catalog(now);
        }
    }

    fn next_deadline(&self) -> Option<Instant> {
        [
            self.debouncer.deadline(),
            self.pause_hide_at,
            self.reconnect_at,
            self.handshake_deadline,
            self.update_check_at,
            self.sources.apple_music.catalog_retry_at,
            self.sources.spotify.catalog_retry_at,
        ]
        .into_iter()
        .flatten()
        .min()
    }

    /// The status rows, logged and pushed to the tray whenever they change.
    ///
    /// Rebuilding the menu here rather than when it opens is the Windows
    /// stand-in for macOS's `menuNeedsUpdate`: the shell pops the menu from
    /// its own message handler with no hook to run first, so the rows are kept
    /// current instead. The work lands on state changes, which are rarer than
    /// the SMTC events that provoke them.
    fn report_status(&mut self) {
        let status = status_lines::Input {
            apple_music: &self.sources.apple_music,
            spotify: &self.sources.spotify,
            active_source: self.active_source,
            apple_music_enabled: self.settings.apple_music_enabled,
            spotify_enabled: self.settings.spotify_enabled,
            discord_state: self.discord.state(),
            language: self.settings.language,
        };
        let lines = status_lines::lines(&status);
        let status_changed = lines != self.last_status;
        if !status_changed && !self.menu_dirty {
            return;
        }
        if status_changed {
            for line in &lines {
                log(&format!("   {line}"));
            }
            self.last_status = lines;
        }
        if let Some(tray) = &self.tray {
            let rows = menu_model::build_menu(&menu_model::MenuInput {
                status,
                settings: &self.settings,
                launch_at_login: launch_at_login::is_enabled(),
                update_available: self.update_available.as_ref().map(|r| r.tag.as_str()),
            });
            if let Err(e) = tray.set_menu(&rows) {
                log(&format!("tray: could not update the menu ({e})"));
            }
        }
        self.menu_dirty = false;
    }
}

fn describe(activity: &Option<Map<String, Value>>) -> String {
    let Some(activity) = activity else {
        return "cleared".to_string();
    };
    let field = |key: &str| activity.get(key).and_then(Value::as_str).unwrap_or("");
    format!("{} — {}", field("details"), field("state"))
}

fn install_ctrl_handler() {
    unsafe {
        let _ = SetConsoleCtrlHandler(Some(ctrl_handler), true);
    }
}

/// Returning TRUE claims the signal, which is what buys the loop enough time to
/// clear the presence instead of leaving a ghost "Listening to" on the profile.
unsafe extern "system" fn ctrl_handler(_ctrl_type: u32) -> BOOL {
    request_shutdown();
    BOOL(1)
}

/// Ends the loop from anywhere — the console handler, the Quit menu item —
/// without waiting for whatever it is currently blocked on.
///
/// Deliberately the only way out: one flag and one nudge means the shutdown
/// path that clears the Discord presence is the same one every time.
pub fn request_shutdown() {
    SHUTDOWN.store(true, Ordering::SeqCst);
    let raw = WAKE_HANDLE.load(Ordering::SeqCst);
    if raw != 0 {
        unsafe {
            let _ = SetEvent(HANDLE(raw as *mut std::ffi::c_void));
        }
    }
}

/// Timestamped line on stdout. Failure is ignored on purpose: a closed stdout
/// is not worth taking the app down for, and the tray build will have none.
pub(crate) fn log(message: &str) {
    use std::io::Write;

    let now = unsafe { GetLocalTime() };
    let mut out = io::stdout().lock();
    let _ = writeln!(
        out,
        "[{:02}:{:02}:{:02}] {message}",
        now.wHour, now.wMinute, now.wSecond
    );
}
