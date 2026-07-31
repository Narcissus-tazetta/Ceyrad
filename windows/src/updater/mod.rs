//! Noticing that a newer release has been published.
//!
//! Like `catalog`, the blocking HTTP call gets a thread of its own and reports
//! back through a channel, nudging the event loop's wake event; the
//! orchestrator stays single-threaded. Unlike `catalog`, the thread lasts only
//! as long as the request. A check happens twice on a good day — once at launch
//! and once every 24 hours after — and carries nothing between runs worth
//! keeping: a thread parked on a channel for a day, holding a COM apartment and
//! an `HttpClient` open, would be resident cost with nothing to show for it.
//! Spawning costs some tens of microseconds, once a day.
//!
//! What it deliberately does *not* do is download or install anything. See
//! `core::update_check` for why that stops here.

use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::mpsc::{channel, Receiver, Sender};
use std::sync::Arc;
use std::thread;

use crate::app::log;
use crate::core::update_check::{self, Release};
use crate::discord::pipe::Event as WakeEvent;
use crate::winhttp::{ComApartment, Http};

/// GitHub rejects a request that does not identify itself, so this is required
/// rather than polite.
const USER_AGENT: &str = concat!("Ceyrad/", env!("CARGO_PKG_VERSION"), " (Windows)");

/// The version this build reports itself as. Stamped from the git tag by the
/// release workflow, so a locally built binary compares as its Cargo version.
pub const CURRENT_VERSION: &str = env!("CARGO_PKG_VERSION");

/// What the check found. Only `Available` is worth showing the user; the rest
/// exist so the log can say what happened.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum Outcome {
    Available(Release),
    UpToDate,
    Failed,
}

pub struct Updater {
    wake: Arc<WakeEvent>,
    /// Kept so each check can hand a clone to the thread it spawns.
    sender: Sender<Outcome>,
    results: Receiver<Outcome>,
    /// Whether a check is already on its way. The scheduled check and the menu
    /// item share one answer, so the second ask joins the first rather than
    /// opening a second connection to GitHub.
    in_flight: Arc<AtomicBool>,
}

impl Updater {
    pub fn new(wake: Arc<WakeEvent>) -> Self {
        let (sender, results) = channel::<Outcome>();
        Self {
            wake,
            sender,
            results,
            in_flight: Arc::new(AtomicBool::new(false)),
        }
    }

    /// Asks for a check. Cheap to call more often than needed — an ask made
    /// while one is in flight is answered by the one already running.
    pub fn check(&self) {
        if self.in_flight.swap(true, Ordering::SeqCst) {
            return;
        }

        let results = self.sender.clone();
        let wake = Arc::clone(&self.wake);
        let in_flight = Arc::clone(&self.in_flight);

        // Not `expect`: a windowless binary that dies here dies silently, with
        // no message and no log line, for the most expendable feature it has.
        let spawned = thread::Builder::new()
            .name("updater".into())
            .spawn(move || {
                let outcome = run_check();
                let _ = results.send(outcome);
                // Cleared before the nudge, so the event loop cannot wake, read
                // the answer, and be refused the next check by a flag belonging
                // to a thread that has already finished.
                in_flight.store(false, Ordering::SeqCst);
                let _ = wake.set();
            });

        if let Err(e) = spawned {
            self.in_flight.store(false, Ordering::SeqCst);
            log(&format!("updater: could not start the check ({e})"));
        }
    }

    pub fn try_recv(&self) -> Option<Outcome> {
        self.results.try_recv().ok()
    }
}

/// The whole of one check, on the thread that was spawned for it.
///
/// Every path returns an `Outcome` — including the two that can only fail
/// before the network is reached — because a user who picked "Check for
/// Updates" is owed an answer either way.
fn run_check() -> Outcome {
    // WinRT apartments are per-thread, so this one declares its own rather than
    // leaning on whatever the main thread happened to choose.
    let _com = ComApartment::enter();

    let Some(url) = update_check::latest_release_url(env!("CARGO_PKG_REPOSITORY")) else {
        // Only reachable if the repository field stops being a GitHub URL, in
        // which case there is nowhere to ask.
        log("updater: no GitHub repository to check");
        return Outcome::Failed;
    };

    let http = match Http::new() {
        Ok(http) => http,
        Err(e) => {
            log(&format!("updater: no HTTP client ({e})"));
            return Outcome::Failed;
        }
    };

    match http.get_with_user_agent(&url, Some(USER_AGENT)) {
        Ok(body) => match update_check::parse_latest(&body) {
            Some(release) if update_check::is_newer(&release.version, CURRENT_VERSION) => {
                Outcome::Available(release)
            }
            Some(_) => Outcome::UpToDate,
            None => {
                log("updater: no usable release in the response");
                Outcome::Failed
            }
        },
        Err(e) => {
            log(&format!("updater: check failed ({e})"));
            Outcome::Failed
        }
    }
}

/// Hands a URL to the default browser.
///
/// The one action this module takes on the user's behalf, and it is the same one
/// clicking a link anywhere else would take: no download, no replace, nothing
/// that needs a signature to be trustworthy.
pub fn open_in_browser(url: &str) -> std::io::Result<()> {
    use windows::core::HSTRING;
    use windows::Win32::UI::Shell::ShellExecuteW;
    use windows::Win32::UI::WindowsAndMessaging::SW_SHOWNORMAL;

    // `ShellExecuteW` with "open" is a shell dispatch, not a browser call: a
    // non-http string would route to whatever protocol handler, UNC path or
    // local executable matches it. The URL this app opens comes from a field in
    // GitHub's JSON, and the app already validates the URLs its own user types
    // — trusting a remote document more than the person at the keyboard would
    // be the wrong way round.
    if !url.starts_with("https://") {
        return Err(std::io::Error::other(format!(
            "refusing to open {url}: not an https URL"
        )));
    }

    let operation = HSTRING::from("open");
    let target = HSTRING::from(url);
    let result = unsafe { ShellExecuteW(None, &operation, &target, None, None, SW_SHOWNORMAL) };
    // ShellExecuteW reports failure as a pseudo-HINSTANCE of 32 or less.
    if result.0 as usize > 32 {
        Ok(())
    } else {
        Err(std::io::Error::other(format!(
            "could not open {url} (ShellExecute returned {})",
            result.0 as usize
        )))
    }
}
