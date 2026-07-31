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
//!
//! # What this deliberately does not watch
//!
//! SMTC is system-wide: a browser mints a session per profile, and games,
//! video players and messaging apps all appear in the same list. Subscribing to
//! all of them would mean this app woke up — and asked every session what it
//! was doing — every time anyone's video advanced a frame's worth of timeline,
//! with Apple Music closed. So only the sessions belonging to a source the user
//! is actually watching are subscribed to, and the rest are noted once and
//! ignored. With no player running, the only thing that can wake this app is
//! `SessionsChanged`, which is also how a player that starts later is noticed.
//!
//! That places the whole burden of discovery on `SessionsChanged`. It is the
//! same event the subscription list already depended on, and it is what the
//! macOS build's `NSWorkspace` launch notification does there.
//!
//! # Why a timeline tick is cheap
//!
//! Apple Music for Windows raises `TimelinePropertiesChanged` roughly every
//! 280ms for the whole length of a track — the event that catches a seek, and
//! there is no other way to catch one. What it cannot do is change the title,
//! the artist or the album, so it must not pay for `TryGetMediaPropertiesAsync`,
//! which is a call into the *player's* process and by far the most expensive
//! thing this module does. Those strings are cached per subscription and reused
//! until something that can actually have changed them says so.

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

/// How far a session's reported duration may move before the cached title is
/// treated as belonging to a different track.
///
/// The safety net under the metadata cache, not the mechanism: a track change
/// is supposed to arrive as `MediaPropertiesChanged`, and this is what notices
/// one that did not. A whole second of slack because a duration can settle
/// slightly as a track loads, and re-reading over that would put the expensive
/// call back on the 280ms path it was taken off.
const DURATION_EPSILON_SEC: f64 = 1.0;

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
    /// machine shows up here — browsers, games, video players — so it is filled
    /// only on a pass that rebuilt the subscriptions, which is when the session
    /// list actually moved, and is empty on every other pass.
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

/// The strings one `TryGetMediaPropertiesAsync` produced.
#[derive(Debug, Clone)]
struct Metadata {
    name: String,
    artist: String,
    album: String,
}

/// The last answer a session gave about what it is playing.
///
/// Three states, not two: a session that answered with nothing is giving a real
/// answer and must not be asked again on every tick, which is exactly the
/// mistake `Option<Metadata>` alone would invite.
#[derive(Debug, Clone, Default)]
enum Cached {
    /// Never read, or read against a subscription that has since been rebuilt.
    #[default]
    Unknown,
    /// The session answered and had no track to describe.
    Empty,
    Track(Metadata),
}

/// Everything remembered about a session between reads.
#[derive(Debug, Clone, Default)]
struct SessionCache {
    metadata: Cached,
    /// The status the last read saw. A status that has moved is the other thing
    /// that accompanies a track change, and it is read on every pass anyway —
    /// so comparing it costs nothing and does not depend on which event fired.
    player_state: Option<PlayerState>,
    /// The duration the timeline reported when `metadata` was read.
    ///
    /// Kept beside the metadata rather than inside it so the check covers a
    /// session that answered with no title at all: that answer can go stale the
    /// same way a title can, and it is the one case with no strings to notice
    /// it by.
    duration_sec: Option<f64>,
}

/// The handlers registered against one session, and what has been learned from
/// it since.
///
/// Each token is `None` until its registration succeeds, so a `Subscription`
/// dropped partway through construction hands back exactly what it took. A
/// token left unregistered would keep its handler — and the session it closes
/// over — alive for the life of the process.
struct Subscription {
    source: MusicSourceId,
    session: Session,
    media: Option<Token>,
    playback: Option<Token>,
    timeline: Option<Token>,
    /// Per subscription rather than per source, so two sessions claiming the
    /// same source — iTunes and Apple Music open together — cannot be served
    /// each other's title.
    cache: SessionCache,
}

impl Subscription {
    fn new(source: MusicSourceId, session: Session) -> Self {
        Self {
            source,
            session,
            media: None,
            playback: None,
            timeline: None,
            cache: SessionCache::default(),
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
    /// Which AUMIDs count as which player. Owned here because it is only ever
    /// consulted while deciding what to subscribe to.
    overrides: AumidOverrides,
    /// Which sources the user is watching, indexed by `MusicSourceId::index`.
    /// A source switched off is not subscribed to at all, so it stops waking
    /// this app rather than being read and discarded.
    watched: [bool; MusicSourceId::COUNT],
    /// Some session changed; the loop should take a fresh snapshot.
    dirty: Arc<AtomicBool>,
    /// The session list itself changed, so subscriptions must be rebuilt.
    sessions_changed: Arc<AtomicBool>,
    /// A session said its metadata moved, so the cached titles are stale.
    metadata_dirty: Arc<AtomicBool>,
    subscriptions: Vec<Subscription>,
    /// Sessions the last rebuild did not recognise, handed to the next snapshot
    /// so they are logged once per change rather than once per event.
    unknown_aumids: Vec<String>,
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
    pub fn new(
        wake: Arc<WakeEvent>,
        overrides: AumidOverrides,
        watched: [bool; MusicSourceId::COUNT],
    ) -> windows::core::Result<Self> {
        let manager = join_with_timeout(&SessionManager::RequestAsync()?, SESSION_MANAGER_TIMEOUT)?;

        let dirty = Arc::new(AtomicBool::new(true));
        let sessions_changed = Arc::new(AtomicBool::new(true));
        let metadata_dirty = Arc::new(AtomicBool::new(true));

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
            overrides,
            watched,
            dirty,
            sessions_changed,
            metadata_dirty,
            subscriptions: Vec::new(),
            unknown_aumids: Vec::new(),
            sessions_token: token,
        })
    }

    /// True when something has changed since the last `take_snapshot`.
    pub fn is_dirty(&self) -> bool {
        self.dirty.load(Ordering::SeqCst)
    }

    /// Asks for a fresh reading even though nothing was reported.
    ///
    /// The wake matters as much as the flag. The caller is the event loop
    /// itself, on its way back to a wait whose deadline is whatever timer
    /// happens to be armed — with no player running that is the daily update
    /// check, so a flag raised without a nudge would sit unread for up to a
    /// day. Signalling here is also why this is safe to call from anywhere in
    /// a pass: the flag is read after the loop resets the event, so a nudge
    /// that lands too late for this pass simply starts the next one.
    pub fn mark_dirty(&self) {
        self.dirty.store(true, Ordering::SeqCst);
        let _ = self.wake.set();
    }

    /// Changes which sources are subscribed to.
    ///
    /// A source switched back on has a session that never stopped, so SMTC has
    /// nothing new to say about it — the resubscribe forced here is what finds
    /// it, and the wake is what makes that happen now rather than whenever the
    /// next timer falls due.
    pub fn set_watched(&mut self, watched: [bool; MusicSourceId::COUNT]) {
        if self.watched != watched {
            self.watched = watched;
            self.sessions_changed.store(true, Ordering::SeqCst);
        }
        self.mark_dirty();
    }

    /// Reads every watched session, re-subscribing first if the session list
    /// moved.
    ///
    /// Both flags are cleared only once the work behind them has succeeded. A
    /// transient WinRT failure (an audio-service or Explorer restart returning
    /// `RPC_E_DISCONNECTED`) would otherwise consume the flags and leave the
    /// watcher with no subscriptions and nothing to re-arm it — a presence
    /// frozen on an old track, silent until the next `SessionsChanged`.
    pub fn take_snapshot(&mut self) -> windows::core::Result<Snapshot> {
        if self.sessions_changed.swap(false, Ordering::SeqCst) {
            // The caches live on the subscriptions, so rebuilding them throws
            // the titles away; say so before the read below decides it can
            // reuse something that no longer exists.
            self.metadata_dirty.store(true, Ordering::SeqCst);
            if let Err(e) = self.resubscribe() {
                self.sessions_changed.store(true, Ordering::SeqCst);
                self.dirty.store(true, Ordering::SeqCst);
                return Err(e);
            }
        }
        self.dirty.store(false, Ordering::SeqCst);

        // Read after the resubscribe above, which is one of the things that
        // sets it.
        let refresh = self.metadata_dirty.swap(false, Ordering::SeqCst);
        let mut retry_metadata = false;

        let mut snapshot = Snapshot {
            unknown_aumids: std::mem::take(&mut self.unknown_aumids),
            ..Default::default()
        };
        for subscription in &mut self.subscriptions {
            // Destructured so the session can be read while its own cache is
            // written; they are separate fields of the same subscription.
            let Subscription {
                source,
                session,
                cache,
                ..
            } = subscription;
            let state = read_session(session, *source, cache, refresh, &mut retry_metadata);
            match source {
                MusicSourceId::AppleMusic => snapshot.apple_music = Some(state),
                MusicSourceId::Spotify => snapshot.spotify = Some(state),
            }
        }

        if retry_metadata {
            // Re-armed, but deliberately without a wake and without re-arming
            // `dirty`: the call that just failed is the one with a one-second
            // deadline, so anything that retries it eagerly spends whole
            // seconds of the event loop's thread doing it. A player that is
            // playing raises another event within a few hundred milliseconds
            // and the retry rides along with that; one that has gone quiet is
            // paused or stopped, and the next thing the user does raises one
            // too. Which is also where the previous code recovered from this.
            self.metadata_dirty.store(true, Ordering::SeqCst);
        }
        Ok(snapshot)
    }

    /// Drops every handler and re-registers against the sessions worth
    /// watching. Sessions are re-created wholesale by SMTC when players come
    /// and go, so there is nothing to diff against.
    fn resubscribe(&mut self) -> windows::core::Result<()> {
        self.subscriptions.clear();
        self.unknown_aumids.clear();

        for session in self.manager.GetSessions()? {
            let Ok(aumid) = session.SourceAppUserModelId() else {
                continue;
            };
            let aumid = aumid.to_string();
            let Some(source) = source_for_aumid(&aumid, &self.overrides) else {
                self.unknown_aumids.push(aumid);
                continue;
            };
            if !self.watched[source.index()] {
                continue;
            }

            // Built in place so that a failure on the second or third
            // registration still unregisters the first: `subscription` is
            // dropped on the way out and hands its tokens back.
            let mut subscription = Subscription::new(source, session);
            subscription.media = Some(
                subscription
                    .session
                    .MediaPropertiesChanged(&self.metadata_handler())?,
            );
            // Playback and timeline both take the cheap handler. What a
            // playback change means is decided from the status itself, which
            // every read looks at anyway — so a player that raises this event
            // more often than it changes anything costs nothing.
            subscription.playback = Some(
                subscription
                    .session
                    .PlaybackInfoChanged(&self.plain_handler())?,
            );
            subscription.timeline = Some(
                subscription
                    .session
                    .TimelinePropertiesChanged(&self.plain_handler())?,
            );
            self.subscriptions.push(subscription);
        }
        Ok(())
    }

    /// Wakes the loop for a fresh reading, nothing more.
    fn plain_handler<
        T: windows::core::RuntimeType + 'static,
        U: windows::core::RuntimeType + 'static,
    >(
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

    /// As `plain_handler`, and additionally retires the cached titles.
    fn metadata_handler<
        T: windows::core::RuntimeType + 'static,
        U: windows::core::RuntimeType + 'static,
    >(
        &self,
    ) -> TypedEventHandler<T, U> {
        let dirty = Arc::clone(&self.dirty);
        let metadata_dirty = Arc::clone(&self.metadata_dirty);
        let wake = Arc::clone(&self.wake);
        TypedEventHandler::new(move |_, _| {
            // The two flags are written here in the opposite order to the one
            // `take_snapshot` reads them in — it clears `dirty` and then
            // consumes `metadata_dirty` — and that is what makes a snapshot
            // landing between these two stores harmless. Either the swap sees
            // `metadata_dirty` and this reading is the fresh one, or it does
            // not, in which case the store below re-arms the `dirty` that
            // snapshot just cleared and another pass follows. Written the other
            // way round there is an interleaving where `dirty` is cleared, the
            // titles are read stale, and nothing is left set to ask again.
            metadata_dirty.store(true, Ordering::SeqCst);
            dirty.store(true, Ordering::SeqCst);
            let _ = wake.set();
            Ok(())
        })
    }
}

/// What the timeline says, or nothing when it says something implausible.
#[derive(Debug, Clone, Copy, Default)]
struct Timeline {
    duration_sec: Option<f64>,
    position_sec: Option<f64>,
    sampled_at: Option<SystemTime>,
}

fn read_session(
    session: &Session,
    source: MusicSourceId,
    cache: &mut SessionCache,
    refresh: bool,
    retry_metadata: &mut bool,
) -> SessionState {
    let player_state = session
        .GetPlaybackInfo()
        .and_then(|info| info.PlaybackStatus())
        .map(player_state_from)
        .unwrap_or(PlayerState::Stopped);

    let status_moved = cache.player_state != Some(player_state);
    cache.player_state = Some(player_state);

    if player_state == PlayerState::Stopped {
        // Nothing to describe, and whatever plays next arrives with metadata of
        // its own.
        cache.metadata = Cached::Unknown;
        cache.duration_sec = None;
        return SessionState {
            player_state,
            track: None,
        };
    }

    // Read before the metadata, because the duration it carries is what catches
    // a track change whose `MediaPropertiesChanged` never arrived. Sync, and
    // answered by the session object rather than by the player's process.
    let timeline = read_timeline(session);

    let stale = refresh
        || status_moved
        || matches!(cache.metadata, Cached::Unknown)
        || duration_moved(cache.duration_sec, timeline.duration_sec);
    // Recorded whether or not the read below happens, and whether or not it
    // succeeds: this is "what the timeline said when we last looked", so a
    // duration that settles by a fraction on each tick cannot creep past the
    // threshold, and a read that failed is brought back by `retry_metadata`
    // rather than by this.
    cache.duration_sec = timeline.duration_sec;

    if stale {
        match read_metadata(session, source) {
            Some(answer) => cache.metadata = answer,
            // The call failed. Keeping whatever was cached and asking again
            // next pass beats dropping the track off the card for a tick, and
            // beats pinning a title that is now wrong for the rest of the song.
            None => *retry_metadata = true,
        }
    }

    let Cached::Track(metadata) = &cache.metadata else {
        return SessionState {
            player_state,
            track: None,
        };
    };

    let mut track = TrackInfo::new(
        metadata.name.clone(),
        metadata.artist.clone(),
        metadata.album.clone(),
    );
    track.duration_sec = timeline.duration_sec;
    track.position_sec = timeline.position_sec;
    // The position belongs to the moment SMTC last refreshed it, not to now.
    // Recording that instant is what lets the progress bar stay correct when
    // the value is minutes stale by the time it is sent; a timestamp we cannot
    // trust leaves `TrackInfo::new`'s "now", which is the honest fallback.
    if let Some(sampled_at) = timeline.sampled_at {
        track.position_sampled_at = sampled_at;
    }
    SessionState {
        player_state,
        track: Some(track),
    }
}

/// `None` when the call itself failed, which is not the same as a session that
/// answered with nothing — one is worth retrying and the other is an answer.
fn read_metadata(session: &Session, source: MusicSourceId) -> Option<Cached> {
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
        return Some(Cached::Empty);
    }

    // Only Apple Music reports the combined shape; Spotify fills both fields in
    // properly and must be left as it is.
    let (artist, album) = match source {
        MusicSourceId::AppleMusic => split_combined_artist(&artist, &album),
        MusicSourceId::Spotify => (artist, album),
    };
    Some(Cached::Track(Metadata {
        name,
        artist,
        album,
    }))
}

fn read_timeline(session: &Session) -> Timeline {
    let Ok(timeline) = session.GetTimelineProperties() else {
        return Timeline::default();
    };
    let start = timeline.StartTime().map(|t| t.Duration).unwrap_or(0);
    let end = timeline.EndTime().map(|t| t.Duration).unwrap_or(0);
    let position = timeline.Position().map(|t| t.Duration).unwrap_or(0);

    // These are arbitrary tick counts written by whatever app owns the session,
    // and negative TimeSpans are legal — so the subtractions are checked,
    // exactly as `system_time_from_ticks` below is. Unchecked they panic on the
    // event loop in a debug build and wrap silently in release, feeding a
    // nonsense progress bar to Discord.
    let duration = ticks_to_secs(end.checked_sub(start).unwrap_or(0));
    if !(0.0..=MAX_PLAUSIBLE_DURATION_SEC).contains(&duration) || duration <= 0.0 {
        return Timeline::default();
    }

    Timeline {
        duration_sec: Some(duration),
        // Position is measured from StartTime, which is not always zero (a
        // chapter or a stream window can begin partway in).
        position_sec: Some(ticks_to_secs(position.checked_sub(start).unwrap_or(0)).max(0.0)),
        sampled_at: timeline
            .LastUpdatedTime()
            .ok()
            .and_then(|t| system_time_from_ticks(t.UniversalTime)),
    }
}

/// Whether two duration readings are far enough apart to mean different tracks.
///
/// A duration appearing or vanishing counts: both happen when a track is
/// swapped for one the player cannot measure yet, and reading the title again
/// is the cheap way to be right about it.
fn duration_moved(cached: Option<f64>, current: Option<f64>) -> bool {
    match (cached, current) {
        (Some(cached), Some(current)) => (cached - current).abs() > DURATION_EPSILON_SEC,
        (None, None) => false,
        _ => true,
    }
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
