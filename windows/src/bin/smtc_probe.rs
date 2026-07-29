//! Milestone-2 spike: what does SMTC actually report on this machine?
//!
//! Prints every media session's `SourceAppUserModelId`, metadata, playback
//! status and timeline, then watches for changes. Run it with Spotify and/or
//! Apple Music playing to pin down the AUMIDs the real app must match on, and
//! to confirm the units of position/duration before anything depends on them.

#[cfg(not(windows))]
fn main() {
    eprintln!("smtc_probe only runs on Windows.");
    std::process::exit(1);
}

#[cfg(windows)]
fn main() -> windows::core::Result<()> {
    imp::run()
}

#[cfg(windows)]
mod imp {
    use std::thread;
    use std::time::Duration;

    use windows::core::Result;
    use windows::Foundation::TypedEventHandler;
    use windows::Media::Control::{
        GlobalSystemMediaTransportControlsSession as Session,
        GlobalSystemMediaTransportControlsSessionManager as SessionManager,
        GlobalSystemMediaTransportControlsSessionPlaybackStatus as PlaybackStatus,
    };
    use windows::Win32::System::Com::{CoInitializeEx, COINIT_MULTITHREADED};

    /// How long to keep watching for events before exiting.
    const WATCH_SECS: u64 = 120;

    pub fn run() -> Result<()> {
        // MTA is enough for a console probe: WinRT delivers the events on pool
        // threads, so no message loop is needed here. The real app uses STA
        // plus a message loop because it also owns a tray icon.
        unsafe {
            let _ = CoInitializeEx(None, COINIT_MULTITHREADED);
        }

        // `join` is the blocking wait on a WinRT IAsyncOperation.
        let manager = SessionManager::RequestAsync()?.join()?;

        println!("=== sessions at startup ===");
        dump_all(&manager)?;

        println!("\n=== watching for {WATCH_SECS}s (play/pause/skip in your players) ===");
        manager.SessionsChanged(&TypedEventHandler::new(
            |manager: windows::core::Ref<SessionManager>, _| {
                println!("\n-- SessionsChanged --");
                if let Some(manager) = manager.as_ref() {
                    let _ = dump_all(manager);
                }
                Ok(())
            },
        ))?;

        // Subscribe to the sessions present now. A production build would also
        // (un)subscribe as sessions come and go; here it is enough to see that
        // the events fire at all.
        for session in manager.GetSessions()? {
            watch(&session)?;
        }

        thread::sleep(Duration::from_secs(WATCH_SECS));
        Ok(())
    }

    fn watch(session: &Session) -> Result<()> {
        let aumid = session.SourceAppUserModelId()?.to_string();

        let id = aumid.clone();
        session.MediaPropertiesChanged(&TypedEventHandler::new(
            move |s: windows::core::Ref<Session>, _| {
                println!("\n-- MediaPropertiesChanged [{id}] --");
                if let Some(s) = s.as_ref() {
                    let _ = dump_one(s);
                }
                Ok(())
            },
        ))?;

        let id = aumid.clone();
        session.PlaybackInfoChanged(&TypedEventHandler::new(
            move |s: windows::core::Ref<Session>, _| {
                println!("\n-- PlaybackInfoChanged [{id}] --");
                if let Some(s) = s.as_ref() {
                    let _ = dump_one(s);
                }
                Ok(())
            },
        ))?;

        let id = aumid;
        session.TimelinePropertiesChanged(&TypedEventHandler::new(
            move |s: windows::core::Ref<Session>, _| {
                println!("\n-- TimelinePropertiesChanged [{id}] --");
                if let Some(s) = s.as_ref() {
                    let _ = dump_timeline(s);
                }
                Ok(())
            },
        ))?;
        Ok(())
    }

    fn dump_all(manager: &SessionManager) -> Result<()> {
        let current = manager
            .GetCurrentSession()
            .ok()
            .and_then(|s| s.SourceAppUserModelId().ok())
            .map(|s| s.to_string());
        println!("current session: {current:?}");

        let sessions = manager.GetSessions()?;
        println!("session count: {}", sessions.Size()?);
        for session in sessions {
            println!();
            dump_one(&session)?;
        }
        Ok(())
    }

    fn dump_one(session: &Session) -> Result<()> {
        // The string the real app has to match on — copy this verbatim.
        println!("AUMID: {}", session.SourceAppUserModelId()?);

        match session.GetPlaybackInfo() {
            Ok(info) => {
                let status = info.PlaybackStatus()?;
                println!("  status: {} ({})", status_name(status), status.0);
            }
            Err(e) => println!("  status: <error {e}>"),
        }

        match session
            .TryGetMediaPropertiesAsync()
            .and_then(|op| op.join())
        {
            Ok(props) => {
                println!("  title:  {:?}", props.Title()?.to_string());
                println!("  artist: {:?}", props.Artist()?.to_string());
                println!("  album:  {:?}", props.AlbumTitle()?.to_string());
                println!("  album artist: {:?}", props.AlbumArtist()?.to_string());
                println!("  has thumbnail: {}", props.Thumbnail().is_ok());
            }
            Err(e) => println!("  media properties: <error {e}>"),
        }

        dump_timeline(session)
    }

    fn dump_timeline(session: &Session) -> Result<()> {
        match session.GetTimelineProperties() {
            Ok(timeline) => {
                // TimeSpan is in 100ns ticks. Printing both the raw ticks and
                // the derived seconds makes a unit mistake obvious: compare the
                // seconds against what the player shows on screen.
                let position = timeline.Position()?.Duration;
                let start = timeline.StartTime()?.Duration;
                let end = timeline.EndTime()?.Duration;
                println!(
                    "  position: {:.3}s (raw {})",
                    ticks_to_secs(position),
                    position
                );
                println!("  start:    {:.3}s (raw {})", ticks_to_secs(start), start);
                println!("  end:      {:.3}s (raw {})", ticks_to_secs(end), end);
                println!(
                    "  duration (end - start): {:.3}s",
                    ticks_to_secs(end - start)
                );
                if let Ok(updated) = timeline.LastUpdatedTime() {
                    println!("  last updated (UTC ticks): {}", updated.UniversalTime);
                }
            }
            Err(e) => println!("  timeline: <error {e}>"),
        }
        Ok(())
    }

    fn ticks_to_secs(ticks: i64) -> f64 {
        ticks as f64 / 10_000_000.0
    }

    fn status_name(status: PlaybackStatus) -> &'static str {
        match status {
            PlaybackStatus::Closed => "Closed",
            PlaybackStatus::Opened => "Opened",
            PlaybackStatus::Changing => "Changing",
            PlaybackStatus::Stopped => "Stopped",
            PlaybackStatus::Paused => "Paused",
            PlaybackStatus::Playing => "Playing",
            _ => "Unknown",
        }
    }
}
