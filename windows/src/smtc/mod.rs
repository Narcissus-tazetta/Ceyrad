//! System Media Transport Controls watcher.
//!
//! This is the Windows counterpart to the macOS app's distributed notifications
//! plus AppleScript: SMTC reports what every media app is playing, with no
//! per-app permission prompt and no polling.
//!
//! The event handlers run on WinRT pool threads, so they do the least possible
//! work — raise a flag and wake the event loop. Everything that touches session
//! objects happens on the loop's own thread, which keeps the subscription list
//! free of locks.

use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::Arc;
use std::time::{Duration, SystemTime, UNIX_EPOCH};

use windows::Foundation::TypedEventHandler;
use windows::Media::Control::{
    GlobalSystemMediaTransportControlsSession as Session,
    GlobalSystemMediaTransportControlsSessionManager as SessionManager,
    GlobalSystemMediaTransportControlsSessionPlaybackStatus as PlaybackStatus,
};

use crate::core::aumid::{source_for_aumid, AumidOverrides};
use crate::core::models::{MusicSourceId, PlayerState, TrackInfo};
use crate::core::track_metadata::split_combined_artist;
use crate::discord::pipe::Event as WakeEvent;
use crate::winrt::join_with_timeout;

/// 100ns ticks between 1601-01-01 (the WinRT epoch) and 1970-01-01.
const TICKS_TO_UNIX_EPOCH: i64 = 116_444_736_000_000_000;
const TICKS_PER_SEC: f64 = 10_000_000.0;

/// Budget for one `TryGetMediaPropertiesAsync`. Generous for an RPC that
/// normally returns in single-digit milliseconds, short enough that a wedged
/// player costs one sluggish snapshot rather than the app.
const MEDIA_PROPERTIES_TIMEOUT: Duration = Duration::from_secs(1);

/// Budget for the one-time `SessionManager::RequestAsync` at startup. Without a
/// bound, a wedged SMTC means the tray icon never appears at all.
const SESSION_MANAGER_TIMEOUT: Duration = Duration::from_secs(5);

/// Longest track length this app will believe. Anything past it is a player
/// reporting nonsense (or a live stream with no real end), and a bogus duration
/// produces a bogus progress bar.
const MAX_PLAUSIBLE_DURATION_SEC: f64 = 24.0 * 60.0 * 60.0;

/// What one player is doing right now.
#[derive(Debug, Clone)]
pub struct SessionState {
    pub player_state: PlayerState,
    pub track: Option<TrackInfo>,
}

#[derive(Debug, Clone, Default)]
pub struct Snapshot {
    pub apple_music: Option<SessionState>,
    pub spotify: Option<SessionState>,
    /// AUMIDs that matched no known player, for logging. Every media app on the
    /// machine shows up here — browsers, games, video players.
    pub unknown_aumids: Vec<String>,
}

impl Snapshot {
    pub fn get(&self, source: MusicSourceId) -> Option<&SessionState> {
        match source {
            MusicSourceId::AppleMusic => self.apple_music.as_ref(),
            MusicSourceId::Spotify => self.spotify.as_ref(),
        }
    }
}

/// WinRT event registrations are plain `i64` tokens in this binding.
type Token = i64;

/// The handlers registered against one session.
///
/// Each token is `None` until its registration succeeds, so a `Subscription`
/// dropped partway through construction hands back exactly what it took. A
/// token left unregistered would keep its handler — and the session it closes
/// over — alive for the life of the process.
struct Subscription {
    session: Session,
    media: Option<Token>,
    playback: Option<Token>,
    timeline: Option<Token>,
}

impl Subscription {
    fn new(session: Session) -> Self {
        Self {
            session,
            media: None,
            playback: None,
            timeline: None,
        }
    }
}

impl Drop for Subscription {
    fn drop(&mut self) {
        if let Some(token) = self.media {
            let _ = self.session.RemoveMediaPropertiesChanged(token);
        }
        if let Some(token) = self.playback {
            let _ = self.session.RemovePlaybackInfoChanged(token);
        }
        if let Some(token) = self.timeline {
            let _ = self.session.RemoveTimelinePropertiesChanged(token);
        }
    }
}

pub struct Watcher {
    manager: SessionManager,
    wake: Arc<WakeEvent>,
    /// Some session changed; the loop should take a fresh snapshot.
    dirty: Arc<AtomicBool>,
    /// The session list itself changed, so subscriptions must be rebuilt.
    sessions_changed: Arc<AtomicBool>,
    subscriptions: Vec<Subscription>,
    sessions_token: Token,
}

impl Drop for Watcher {
    fn drop(&mut self) {
        // The manager outlives this object — it is a system-wide singleton —
        // so the registration has to be handed back explicitly, exactly as
        // each per-session `Subscription` does.
        let _ = self.manager.RemoveSessionsChanged(self.sessions_token);
    }
}

impl Watcher {
    /// Signals `wake` whenever anything changes.
    pub fn new(wake: Arc<WakeEvent>) -> windows::core::Result<Self> {
        let manager = join_with_timeout(&SessionManager::RequestAsync()?, SESSION_MANAGER_TIMEOUT)?;

        let dirty = Arc::new(AtomicBool::new(true));
        let sessions_changed = Arc::new(AtomicBool::new(true));

        let token = {
            let dirty = Arc::clone(&dirty);
            let sessions_changed = Arc::clone(&sessions_changed);
            let wake = Arc::clone(&wake);
            manager.SessionsChanged(&TypedEventHandler::new(move |_, _| {
                sessions_changed.store(true, Ordering::SeqCst);
                dirty.store(true, Ordering::SeqCst);
                let _ = wake.set();
                Ok(())
            }))?
        };

        Ok(Self {
            manager,
            wake,
            dirty,
            sessions_changed,
            subscriptions: Vec::new(),
            sessions_token: token,
        })
    }

    /// True when something has changed since the last `take_snapshot`.
    pub fn is_dirty(&self) -> bool {
        self.dirty.load(Ordering::SeqCst)
    }

    /// Asks for a fresh reading even though nothing was reported.
    ///
    /// Needed when a source is switched back on: its session never stopped, so
    /// SMTC has nothing new to say, and without this the player would go
    /// unnoticed until it next changed track.
    pub fn mark_dirty(&self) {
        self.dirty.store(true, Ordering::SeqCst);
    }

    /// Reads every session, re-subscribing first if the session list moved.
    ///
    /// Both flags are cleared only once the work behind them has succeeded. A
    /// transient WinRT failure (an audio-service or Explorer restart returning
    /// `RPC_E_DISCONNECTED`) would otherwise consume the flags and leave the
    /// watcher with no subscriptions and nothing to re-arm it — a presence
    /// frozen on an old track, silent until the next `SessionsChanged`.
    pub fn take_snapshot(&mut self, overrides: &AumidOverrides) -> windows::core::Result<Snapshot> {
        if self.sessions_changed.swap(false, Ordering::SeqCst) {
            if let Err(e) = self.resubscribe() {
                self.sessions_changed.store(true, Ordering::SeqCst);
                self.dirty.store(true, Ordering::SeqCst);
                return Err(e);
            }
        }

        let sessions = match self.manager.GetSessions() {
            Ok(sessions) => sessions,
            Err(e) => {
                self.dirty.store(true, Ordering::SeqCst);
                return Err(e);
            }
        };
        self.dirty.store(false, Ordering::SeqCst);

        let mut snapshot = Snapshot::default();
        for session in sessions {
            let Ok(aumid) = session.SourceAppUserModelId() else {
                continue;
            };
            let aumid = aumid.to_string();
            match source_for_aumid(&aumid, overrides) {
                Some(MusicSourceId::AppleMusic) => {
                    snapshot.apple_music = Some(read_session(&session, MusicSourceId::AppleMusic));
                }
                Some(MusicSourceId::Spotify) => {
                    snapshot.spotify = Some(read_session(&session, MusicSourceId::Spotify));
                }
                None => snapshot.unknown_aumids.push(aumid),
            }
        }
        Ok(snapshot)
    }

    /// Drops every handler and re-registers against the current session list.
    /// Sessions are re-created wholesale by SMTC when players come and go, so
    /// there is nothing to diff against.
    fn resubscribe(&mut self) -> windows::core::Result<()> {
        self.subscriptions.clear();

        for session in self.manager.GetSessions()? {
            // Built in place so that a failure on the second or third
            // registration still unregisters the first: `subscription` is
            // dropped on the way out and hands its tokens back.
            let mut subscription = Subscription::new(session);
            subscription.media = Some(
                subscription
                    .session
                    .MediaPropertiesChanged(&self.handler())?,
            );
            subscription.playback =
                Some(subscription.session.PlaybackInfoChanged(&self.handler())?);
            subscription.timeline = Some(
                subscription
                    .session
                    .TimelinePropertiesChanged(&self.handler())?,
            );
            self.subscriptions.push(subscription);
        }
        Ok(())
    }

    fn handler<T: windows::core::RuntimeType + 'static, U: windows::core::RuntimeType + 'static>(
        &self,
    ) -> TypedEventHandler<T, U> {
        let dirty = Arc::clone(&self.dirty);
        let wake = Arc::clone(&self.wake);
        TypedEventHandler::new(move |_, _| {
            dirty.store(true, Ordering::SeqCst);
            let _ = wake.set();
            Ok(())
        })
    }
}

fn read_session(session: &Session, source: MusicSourceId) -> SessionState {
    let player_state = session
        .GetPlaybackInfo()
        .and_then(|info| info.PlaybackStatus())
        .map(player_state_from)
        .unwrap_or(PlayerState::Stopped);

    let track = if player_state == PlayerState::Stopped {
        None
    } else {
        read_track(session, source)
    };

    SessionState {
        player_state,
        track,
    }
}

fn read_track(session: &Session, source: MusicSourceId) -> Option<TrackInfo> {
    // Served by the media app's own process over RPC, on the event loop's
    // thread. A wedged, suspended or crashing player would never complete it,
    // and an unbounded wait here is not a slow app but a dead one — no menu, no
    // Discord replies, no way to quit. A stale title for one snapshot is the
    // far better trade.
    let props = session
        .TryGetMediaPropertiesAsync()
        .and_then(|op| join_with_timeout(&op, MEDIA_PROPERTIES_TIMEOUT))
        .ok()?;

    let name = props.Title().map(|s| s.to_string()).unwrap_or_default();
    let artist = props.Artist().map(|s| s.to_string()).unwrap_or_default();
    let album = props
        .AlbumTitle()
        .map(|s| s.to_string())
        .unwrap_or_default();
    if name.is_empty() && artist.is_empty() {
        return None;
    }

    // Only Apple Music reports the combined shape; Spotify fills both fields in
    // properly and must be left as it is.
    let (artist, album) = match source {
        MusicSourceId::AppleMusic => split_combined_artist(&artist, &album),
        MusicSourceId::Spotify => (artist, album),
    };

    let mut track = TrackInfo::new(name, artist, album);

    if let Ok(timeline) = session.GetTimelineProperties() {
        let start = timeline.StartTime().map(|t| t.Duration).unwrap_or(0);
        let end = timeline.EndTime().map(|t| t.Duration).unwrap_or(0);
        let position = timeline.Position().map(|t| t.Duration).unwrap_or(0);

        // These are arbitrary tick counts written by whatever app owns the
        // session, and negative TimeSpans are legal — so the subtractions are
        // checked, exactly as `system_time_from_ticks` below is. Unchecked they
        // panic on the event loop in a debug build and wrap silently in
        // release, feeding a nonsense progress bar to Discord.
        let duration = ticks_to_secs(end.checked_sub(start).unwrap_or(0));
        if (0.0..=MAX_PLAUSIBLE_DURATION_SEC).contains(&duration) && duration > 0.0 {
            track.duration_sec = Some(duration);
            // Position is measured from StartTime, which is not always zero
            // (a chapter or a stream window can begin partway in).
            track.position_sec =
                Some(ticks_to_secs(position.checked_sub(start).unwrap_or(0)).max(0.0));
            // The position belongs to the moment SMTC last refreshed it, not to
            // now. Recording that instant is what lets the progress bar stay
            // correct when the value is minutes stale by the time it is sent.
            track.position_sampled_at = timeline
                .LastUpdatedTime()
                .ok()
                .and_then(|t| system_time_from_ticks(t.UniversalTime))
                .unwrap_or_else(SystemTime::now);
        }
    }
    Some(track)
}

fn player_state_from(status: PlaybackStatus) -> PlayerState {
    match status {
        PlaybackStatus::Playing => PlayerState::Playing,
        PlaybackStatus::Paused => PlayerState::Paused,
        _ => PlayerState::Stopped,
    }
}

fn ticks_to_secs(ticks: i64) -> f64 {
    ticks as f64 / TICKS_PER_SEC
}

/// `None` for a timestamp that is zero, absurd or in the future — a clock we
/// cannot trust is worse than falling back to "now".
///
/// Every step is checked because the input comes from another process: a
/// garbage `LastUpdatedTime` must degrade to "now", never panic.
fn system_time_from_ticks(ticks: i64) -> Option<SystemTime> {
    let unix_ticks = ticks.checked_sub(TICKS_TO_UNIX_EPOCH)?;
    let nanos = u64::try_from(unix_ticks).ok()?.checked_mul(100)?;
    if nanos == 0 {
        return None;
    }
    let sampled = UNIX_EPOCH.checked_add(Duration::from_nanos(nanos))?;
    (sampled <= SystemTime::now()).then_some(sampled)
}
