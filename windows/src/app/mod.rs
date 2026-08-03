//! The orchestrator: SMTC in, Discord Rich Presence out.
//!
//! A port of the macOS `AppDelegate`, minus the tray UI. Everything runs on one
//! thread driven by a single wait: Discord's pipe, the SMTC wakeup event and
//! the app's own timers are all waited on together, so an idle app performs no
//! work at all — no polling loop, no periodic timer, no wakeups between songs.
//!
//! What this type does *not* own is as deliberate as what it does. The record
//! of what Discord is showing and the reconnect backoff live behind
//! [`connection::Connection`]; every deadline lives in [`Timers`]. Both were
//! loose fields here, and both carried invariants that had to be re-established
//! by hand at a dozen call sites.

pub mod connection;
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

use crate::app::connection::{Connection, Sent};
use crate::catalog::{Outcome, Request as CatalogRequest, Resolver};
use crate::core::activity_builder;
use crate::core::apple_music;
use crate::core::debouncer::Debouncer;
use crate::core::i18n::t;
use crate::core::menu_model::{self, MenuAction};
use crate::core::models::{ConnState, MusicState, PlayerState};
use crate::core::settings_model::{ButtonSlot, Settings};
use crate::core::status_lines;
use crate::core::timers::{Timer, Timers};
use crate::core::track_change::{position_jumped, same_identity};
use crate::core::update_check::Release;
use crate::core::url_prompt;
use crate::discord::client::Event as DiscordEvent;
use crate::discord::pipe::{Event as WakeEvent, Wakeup};
use crate::launch_at_login;
use crate::smtc::Watcher;
use crate::tray::{self, dialog, Tray};
use crate::updater::{self, Outcome as UpdateOutcome, Updater};

/// Matches the macOS build: collapses a burst of player events into one send.
const DEBOUNCE: Duration = Duration::from_millis(800);
/// Ceiling on how long that coalescing may postpone a send; see `Debouncer`.
const MAX_DEBOUNCE: Duration = Duration::from_millis(2_000);
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

/// Pause after a wait that failed outright, so a persistent failure degrades
/// into slow polling instead of a spin.
const WAIT_FAILURE_BACKOFF: Duration = Duration::from_secs(1);

/// Ceiling on the "already logged this unknown AUMID" memo.
const MAX_REMEMBERED_AUMIDS: usize = 64;

static SHUTDOWN: AtomicBool = AtomicBool::new(false);
/// The wake event, as a raw handle so the console handler — which cannot
/// capture anything — can nudge the event loop out of its wait.
static WAKE_HANDLE: AtomicIsize = AtomicIsize::new(0);

pub struct App {
    settings: Settings,
    music: MusicState,
    /// Whether anything is currently on Discord. The presence is cleared and
    /// rebuilt as this flips.
    displaying: bool,

    connection: Connection,
    watcher: Watcher,
    catalog: Resolver,
    updater: Updater,
    wake: Arc<WakeEvent>,
    /// Absent in the console build, which has no UI at all.
    tray: Option<Tray>,

    timers: Timers,
    debouncer: Debouncer,
    pending_activity: Option<Option<Map<String, Value>>>,

    paused_timed_out: bool,

    /// The newest release found so far, kept so the menu can offer it and the
    /// click knows where to send the browser.
    update_available: Option<Release>,
    /// Whether the user asked, so an "already up to date" answer is only
    /// reported back when someone is waiting for it.
    update_check_requested: bool,

    logged_unknown_aumids: HashSet<String>,
    last_status: Vec<String>,
    /// Whether anything the status rows are made of has moved since they were
    /// last built.
    ///
    /// Building them allocates a `String` per row, and this loop runs several
    /// times a second while a track plays — almost always to conclude that
    /// nothing changed. The flag is allowed to be set when nothing did (the
    /// cost is one wasted rebuild); it must never be clear when something has,
    /// so everything that writes `running`, `player_state` or `track`, or
    /// flips `displaying`, sets it. The connection is covered separately by
    /// `last_reported_conn`, which is a `Copy` scalar and can simply be
    /// compared.
    ///
    /// This is only about the *log*. The menu is built when it opens, so
    /// nothing here has to keep it in step.
    status_dirty: bool,
    last_reported_conn: ConnState,
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
                "AUMID overrides: apple music {:?}",
                overrides.apple_music
            ));
        }

        let wake = Arc::new(WakeEvent::new()?);
        WAKE_HANDLE.store(wake.handle().0 as isize, Ordering::SeqCst);

        let watcher = Watcher::new(Arc::clone(&wake), overrides).map_err(io::Error::other)?;

        // Nothing keeps the Run entry in step with a portable exe that moved, so
        // it is checked here rather than left to fail quietly at the next logon.
        if let Err(e) = launch_at_login::reconcile_path() {
            log(&format!("launch at login: could not update the path ({e})"));
        }

        let mut timers = Timers::default();
        timers.arm_in(Timer::UpdateCheck, FIRST_UPDATE_CHECK_DELAY);

        Ok(Self {
            settings,
            music: MusicState::default(),
            displaying: false,
            connection: Connection::new(),
            watcher,
            catalog: Resolver::new(Arc::clone(&wake)),
            updater: Updater::new(Arc::clone(&wake)),
            wake,
            tray: None,
            timers,
            debouncer: Debouncer::new(DEBOUNCE, MAX_DEBOUNCE),
            pending_activity: None,
            paused_timed_out: false,
            update_available: None,
            update_check_requested: false,
            logged_unknown_aumids: HashSet::new(),
            last_status: Vec::new(),
            // Nothing has been reported yet, so the first pass must.
            status_dirty: true,
            last_reported_conn: ConnState::Disconnected,
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
                log("quit requested");
                break;
            }
            // Consumed before the dirty flags are read: a handler that fires in
            // between leaves the flag set for this pass, or re-signals for the
            // next one. Neither loses an event.
            //
            // Logged rather than propagated: a `?` here would leave `run`
            // without ever reaching the teardown below, so a failure to reset
            // one event would cost the user a stale presence left on Discord.
            if let Err(e) = self.wake.reset() {
                log(&format!("wake: could not reset ({e})"));
            }

            // After the pump, which is what turns a click into an event. The
            // menu is built here, from the state as it stands, and torn down
            // again when it closes.
            if tray::take_menu_request() && !self.open_menu() {
                log("quit requested");
                break;
            }

            if self.watcher.is_dirty() {
                self.handle_smtc();
            }
            self.drain_catalog();
            self.drain_updates();
            self.fire_due_timers();
            while let Some(action) = tray::try_recv_action() {
                self.apply_menu_action(action);
            }
            self.report_status();

            let timeout = self
                .next_deadline()
                .map(|deadline| deadline.saturating_duration_since(Instant::now()));

            if self.connection.is_disconnected() {
                // A failed wait returns at once and would do so on every pass;
                // without the sleep this degrades into a pegged core on a
                // battery-powered machine with nothing in the log to explain it.
                if self.wake.wait(timeout) == Wakeup::Failed {
                    log(&format!("wait failed: {}", io::Error::last_os_error()));
                    std::thread::sleep(WAIT_FAILURE_BACKOFF);
                }
            } else {
                let mut events = Vec::new();
                let result = self
                    .connection
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
        self.connection.disconnect(true);
        Ok(())
    }

    // MARK: - Menu

    pub fn apply_menu_action(&mut self, action: MenuAction) {
        match action {
            MenuAction::SetButtonType { button, link_type } => {
                self.settings.set_button_type(button, link_type);
                self.settings_changed();
            }
            MenuAction::EditButtonLabel(button) => self.edit_button_label(button),
            MenuAction::EditCustomUrl => self.edit_custom_url(),
            MenuAction::EditRepositoryUrl => self.edit_repository_url(),
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
                // re-send to Discord — but the status rows are logged in the
                // same language, so they are worth saying again.
                self.persist_settings();
                self.status_dirty = true;
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
                self.connection.disconnect(true);
                self.timers.cancel(Timer::Handshake);
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
        // Not a `Settings` field, so there is nothing to save, and the row is
        // read from the registry the next time the menu opens.
    }

    fn edit_button_label(&mut self, button: ButtonSlot) {
        let language = self.settings.language;
        let number = button.number();
        let current = self.settings.button_label(button);
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
        self.settings.set_button_label(button, &text);
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

    /// macOS's `settingsChanged()`: save, then re-evaluate what should be on
    /// screen now that a setting has moved.
    fn settings_changed(&mut self) {
        self.persist_settings();
        self.status_dirty = true;

        if !self.music.running {
            self.go_dormant();
            return;
        }

        if self.music.is_displayable() != self.displaying {
            self.set_displaying(self.music.is_displayable());
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
        let snapshot = match self.watcher.take_snapshot() {
            Ok(snapshot) => snapshot,
            Err(e) => {
                log(&format!("smtc: {e}"));
                return;
            }
        };

        for aumid in &snapshot.unknown_aumids {
            // Memo only, to keep the log from repeating itself. Browsers mint a
            // fresh AUMID per profile and this process runs for months, so the
            // set is capped rather than allowed to grow for the life of the
            // app; going over it just means a line may be logged twice.
            if self.logged_unknown_aumids.len() >= MAX_REMEMBERED_AUMIDS {
                self.logged_unknown_aumids.clear();
            }
            if self.logged_unknown_aumids.insert(aumid.clone()) {
                log(&format!("smtc: ignoring unrecognised session {aumid}"));
            }
        }

        // SMTC re-reports a playing session several times a second, so "did
        // anything happen?" has to be decided from the values. Without this the
        // presence would be rebuilt and compared four times a second for a
        // track that is simply playing on.
        let mut changed = false;
        let mut track_changed = false;

        match snapshot.apple_music.as_ref() {
            Some(session) => {
                let same_track = same_identity(self.music.track.as_ref(), session.track.as_ref());
                let seeked = position_jumped(self.music.track.as_ref(), session.track.as_ref());
                changed = !self.music.running
                    || self.music.player_state != session.player_state
                    || !same_track
                    || seeked;

                if !self.music.running {
                    log(&format!("{}: session appeared", apple_music::DISPLAY_NAME));
                }
                self.music.running = true;
                self.music.player_state = session.player_state;

                if same_track {
                    // The overwhelmingly common case: the same track, a moment
                    // later. Only the position moved, and the three strings
                    // already held are equal to the ones on offer — so cloning
                    // the reading over them would allocate three times, several
                    // times a second, to change nothing.
                    if let (Some(held), Some(incoming)) =
                        (self.music.track.as_mut(), session.track.as_ref())
                    {
                        held.adopt_playback_from(incoming);
                    }
                } else {
                    self.music.track = session.track.clone();
                    self.reset_catalog();
                    track_changed = true;
                }
            }
            None => {
                if self.music.running {
                    log(&format!("{}: session gone", apple_music::DISPLAY_NAME));
                    changed = true;
                }
                self.music = MusicState::default();
                self.timers.cancel(Timer::CatalogRetry);
            }
        }

        // A seek moves no row, but everything else here does, and one wasted
        // rebuild costs less than reasoning about which is which every time a
        // row is added.
        self.status_dirty |= changed;

        // Artwork and links are not in SMTC; they have to be looked up, which
        // takes a network round trip. The request goes out here and the
        // presence is sent without them in the meantime, so a slow lookup never
        // delays the card.
        self.request_missing_catalog(Instant::now());

        if !self.music.running {
            // Nothing left to report. The macOS build goes fully dormant here
            // rather than holding an idle Discord connection open.
            self.go_dormant();
            return;
        }

        self.attempt_connect();

        if self.music.is_displayable() != self.displaying {
            self.set_displaying(self.music.is_displayable());
        } else if changed && self.displaying {
            // Skipping a track while paused counts as interacting with the
            // player, so a timed-out presence comes back and the clock restarts.
            if track_changed {
                self.cancel_pause_timer();
            }
            self.update_pause_timer();
            self.push_activity();
        }
        // An event that changes nothing on display needs nothing sent.
    }

    /// Starts or stops showing something on Discord.
    fn set_displaying(&mut self, displaying: bool) {
        log(if displaying {
            "now showing on Discord"
        } else {
            "nothing to show — clearing"
        });
        self.displaying = displaying;
        // The rows say what is on display, so this moves them.
        self.status_dirty = true;
        self.cancel_debounce();
        self.cancel_pause_timer();
        self.update_pause_timer();
        self.push_activity();
    }

    /// The player is gone: stop everything and go back to fully idle.
    ///
    /// The only way into that state, so "stop one more thing" is a one-line
    /// change rather than an edit repeated at every call site — which is what
    /// it used to be, in two places that had drifted apart.
    fn go_dormant(&mut self) {
        self.displaying = false;
        self.cancel_pause_timer();
        self.cancel_reconnect();
        self.cancel_debounce();
        self.timers.cancel(Timer::CatalogRetry);
        self.timers.cancel(Timer::Handshake);
        if !self.connection.is_disconnected() {
            log("no players left — disconnecting");
            self.connection.disconnect(true);
        }
    }

    // MARK: - Catalog

    /// Asks for the artwork and links of anything playing that has neither.
    ///
    /// Called after every SMTC snapshot and whenever a retry falls due. The
    /// `catalog_requested_for` guard is what keeps SMTC's event rate from
    /// turning into a request rate.
    fn request_missing_catalog(&mut self, now: Instant) {
        if !self.music.running || self.music.catalog.is_some() {
            return;
        }
        // A failed lookup is serving its cooling-off period.
        if self.music.catalog_retry_at.is_some_and(|at| at > now) {
            return;
        }
        let Some(track) = self.music.track.as_ref() else {
            return;
        };
        if track.name.is_empty() {
            return;
        }
        // Asked before the identity is built, not after: this is the guard
        // that holds for the whole of a track the catalog had no answer
        // for, so it runs on every SMTC event and must not allocate to say
        // "already asked".
        if self
            .music
            .catalog_requested_for
            .as_deref()
            .is_some_and(|requested| track.matches_identity(requested))
        {
            return;
        }
        let identity = track.identity();

        let request = CatalogRequest {
            key: identity.clone(),
            name: track.name.clone(),
            artist: track.artist.clone(),
            album: track.album.clone(),
        };
        // Logged as it goes out, not only when it comes back: a lookup that
        // was never asked for and one that never answered produce the same
        // silent, art-less card, and only this line tells them apart.
        log(&format!(
            "{}: looking up \"{}\" by \"{}\"",
            apple_music::DISPLAY_NAME,
            request.name,
            request.artist
        ));
        self.music.catalog_requested_for = Some(identity);
        self.music.catalog_retry_at = None;
        self.timers.cancel(Timer::CatalogRetry);
        self.catalog.request(request);
    }

    /// Applies whatever the lookup thread has finished. A late answer is only
    /// used when the track it belongs to is still the one playing.
    fn drain_catalog(&mut self) {
        while let Some(resolved) = self.catalog.try_recv() {
            let still_playing = self
                .music
                .track
                .as_ref()
                .is_some_and(|track| track.matches_identity(&resolved.key));
            if !still_playing {
                continue;
            }

            let name = apple_music::DISPLAY_NAME;
            let mut resend = false;
            match resolved.outcome {
                Outcome::Found(catalog) => {
                    self.music.catalog = Some(catalog);
                    self.music.catalog_retry_at = None;
                    log(&format!("{name}: catalog resolved"));
                    // The presence has already gone out without artwork, so
                    // only a hit is worth the re-send.
                    resend = self.displaying;
                }
                Outcome::Missing => {
                    self.music.catalog = None;
                    self.music.catalog_retry_at = None;
                    log(&format!("{name}: catalog not found"));
                }
                Outcome::Failed => {
                    // Not an answer. Clearing the guard lets the track be asked
                    // about again once the cooling-off period is up, instead of
                    // spending the rest of the song with no artwork.
                    self.music.catalog_requested_for = None;
                    self.music.catalog_retry_at = Some(Instant::now() + CATALOG_RETRY_DELAY);
                    self.timers.arm_in(Timer::CatalogRetry, CATALOG_RETRY_DELAY);
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

    /// Back to "nothing known about this track", with the pending retry — if
    /// any — dropped alongside it.
    fn reset_catalog(&mut self) {
        self.music.clear_catalog();
        self.timers.cancel(Timer::CatalogRetry);
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
                }
                UpdateOutcome::UpToDate => {
                    log(&format!("up to date ({})", updater::CURRENT_VERSION));
                    // A release that was newer and no longer is means this build
                    // was replaced while running; drop the stale offer.
                    self.update_available = None;
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
        if !self.music.running {
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
        if !self.displaying {
            return None;
        }
        let track = self.music.track.as_ref()?;
        if !self.should_show_activity(self.music.player_state) {
            return None;
        }
        Some(activity_builder::build(
            track,
            self.music.player_state,
            self.music.catalog.as_ref(),
            &self.settings,
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
        match self.connection.send(activity) {
            // Nothing to send it over yet, or Discord is already showing this.
            // Dropping it is safe: READY pushes a freshly built presence, which
            // is more current than this one.
            Sent::NotConnected | Sent::Unchanged => {}
            Sent::Sent(description) => log(&format!("-> {description}")),
            Sent::Failed(e) => log(&format!("discord: send failed ({e})")),
        }
    }

    // MARK: - Pause timeout

    fn update_pause_timer(&mut self) {
        if !self.displaying || self.music.player_state != PlayerState::Paused {
            self.cancel_pause_timer();
            return;
        }
        // Already counting, or already elapsed: do not restart the clock.
        if self.timers.is_armed(Timer::PauseHide) || self.paused_timed_out {
            return;
        }
        // 0 hides immediately and -1 never hides; neither needs a timer.
        if self.settings.pause_hide_minutes > 0 {
            let minutes = self.settings.pause_hide_minutes as u64;
            self.timers
                .arm_in(Timer::PauseHide, Duration::from_secs(minutes * 60));
        }
    }

    fn cancel_pause_timer(&mut self) {
        self.timers.cancel(Timer::PauseHide);
        self.paused_timed_out = false;
    }

    // MARK: - Connection

    fn attempt_connect(&mut self) {
        if !self.music.running || !self.connection.is_disconnected() {
            return;
        }
        match self.connection.connect() {
            Ok(()) => {
                log(&format!("connecting as {}…", apple_music::DISPLAY_NAME));
                self.timers.arm_in(Timer::Handshake, HANDSHAKE_TIMEOUT);
            }
            Err(e) => {
                log(&format!("discord: {e}"));
                self.schedule_reconnect();
            }
        }
    }

    fn schedule_reconnect(&mut self) {
        self.timers.cancel(Timer::Handshake);
        if !self.music.running {
            return;
        }
        let delay = self.connection.next_backoff();
        self.timers.arm_in(Timer::Reconnect, delay);
        log(&format!("discord: retrying in {}s", delay.as_secs()));
    }

    fn cancel_reconnect(&mut self) {
        self.timers.cancel(Timer::Reconnect);
        self.connection.reset_backoff();
    }

    fn handle_discord_event(&mut self, event: DiscordEvent) {
        match event {
            DiscordEvent::Ready => {
                log("discord: connected");
                self.timers.cancel(Timer::Handshake);
                self.connection.note_ready();
                self.push_activity();
            }
            DiscordEvent::Closed => {
                log("discord: disconnected");
                self.connection.note_closed();
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

        if self.timers.take_if_due(Timer::PauseHide, now)
            && self.displaying
            && self.music.player_state == PlayerState::Paused
        {
            log("paused long enough — hiding presence");
            self.paused_timed_out = true;
            self.push_activity();
        }

        if self.timers.take_if_due(Timer::Handshake, now)
            && self.connection.state() == ConnState::Connecting
        {
            log("discord: no READY within the handshake window — giving up on this pipe");
            self.connection.disconnect(false);
            self.schedule_reconnect();
        }

        if self.timers.take_if_due(Timer::Reconnect, now) {
            self.attempt_connect();
        }

        if self.timers.take_if_due(Timer::UpdateCheck, now) {
            // Re-armed before the answer arrives, so a check that fails still
            // leaves the next one scheduled rather than ending the series.
            self.timers
                .arm_in(Timer::UpdateCheck, UPDATE_CHECK_INTERVAL);
            log("checking for updates");
            self.updater.check();
        }

        // Consumed whether or not it leads to a request, so a deadline that is
        // already past can never keep the loop spinning.
        if self.timers.take_if_due(Timer::CatalogRetry, now) {
            self.music.catalog_retry_at = None;
            self.request_missing_catalog(now);
        }
    }

    fn next_deadline(&self) -> Option<Instant> {
        // `Timers::next` covers every variant by construction; the debouncer
        // keeps its own, which it derives from two intervals rather than one.
        [self.timers.next(), self.debouncer.deadline()]
            .into_iter()
            .flatten()
            .min()
    }

    /// The status rows, logged whenever they change.
    ///
    /// Only the log: the menu carries the same rows but is built when it opens,
    /// so a track change costs two `String`s and a comparison rather than a
    /// whole menu.
    fn report_status(&mut self) {
        // Ahead of building anything. This runs on every pass of a loop SMTC
        // wakes several times a second, and the rows are a `Vec<String>` — so
        // the question "could they have moved?" has to be answerable without
        // assembling them to find out. See `status_dirty` for the invariant.
        let conn = self.connection.state();
        if !self.status_dirty && conn == self.last_reported_conn {
            return;
        }
        self.status_dirty = false;
        self.last_reported_conn = conn;

        // Still compared against the last set: the flag above is allowed to be
        // pessimistic, and a rebuild that produced the same rows must not reach
        // the log.
        let lines = status_lines::lines(&self.status_input(conn));
        if lines == self.last_status {
            return;
        }
        for line in &lines {
            log(&format!("   {line}"));
        }
        self.last_status = lines;
    }

    fn status_input(&self, conn: ConnState) -> status_lines::Input<'_> {
        status_lines::Input {
            music: &self.music,
            discord_state: conn,
            language: self.settings.language,
        }
    }

    /// Builds the menu, shows it, and frees it again once it closes.
    ///
    /// macOS's `menuNeedsUpdate`, reached the long way round: the rows are
    /// assembled from the state as it stands at the click, so nothing has to
    /// keep a menu in step with the app between opens and nothing about it
    /// stays resident. `launch_at_login::is_enabled()` — a registry read — is
    /// part of that, and now happens per open rather than per track change.
    ///
    /// Returns `false` if the message pump saw `WM_QUIT`.
    fn open_menu(&mut self) -> bool {
        let rows = menu_model::build_menu(&menu_model::MenuInput {
            status: self.status_input(self.connection.state()),
            settings: &self.settings,
            launch_at_login: launch_at_login::is_enabled(),
            update_available: self.update_available.as_ref().map(|r| r.tag.as_str()),
        });

        let Some(tray) = &self.tray else {
            return true;
        };
        if let Err(e) = tray.show_menu(&rows) {
            log(&format!("tray: could not show the menu ({e})"));
            return true;
        }
        // `show_menu` returns once the menu is off screen, but the item the user
        // chose arrives afterwards as a `WM_COMMAND` posted to the tray window.
        // Dispatching it here is what turns it into a `MenuAction` before the
        // menu that names it is freed; the loop applies it later this pass.
        let running = tray::pump_messages();
        tray.release_menu();
        running
    }
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
