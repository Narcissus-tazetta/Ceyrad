//! Noticing that a newer release has been published.
//!
//! Structurally the same as `catalog`: a thread that does nothing but block on
//! one HTTP call, reporting back through a channel and nudging the event loop's
//! wake event. The orchestrator stays single-threaded; only blocking I/O gets a
//! thread of its own.
//!
//! What it deliberately does *not* do is download or install anything. See
//! `core::update_check` for why that stops here.

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
    /// `None` when the worker could not be started. Update checks are the most
    /// expendable thing this app does, so a thread that will not spawn costs
    /// the feature and nothing else.
    requests: Option<Sender<()>>,
    results: Receiver<Outcome>,
}

impl Updater {
    pub fn new(wake: Arc<WakeEvent>) -> Self {
        let (requests, request_rx) = channel::<()>();
        let (result_tx, results) = channel::<Outcome>();

        // Not `expect`: with `panic = "abort"` in a windowless binary that is a
        // silent process death at launch, no message and no log line, for a
        // feature the app runs fine without.
        let spawned = thread::Builder::new()
            .name("updater".into())
            .spawn(move || worker(request_rx, result_tx, wake));

        let requests = match spawned {
            Ok(_) => Some(requests),
            Err(e) => {
                log(&format!(
                    "updater: could not start the check thread ({e}); update checks are off"
                ));
                None
            }
        };

        Self { requests, results }
    }

    /// Asks for a check. Cheap to call more often than needed — the worker
    /// collapses a backlog into one request.
    pub fn check(&self) {
        if let Some(requests) = &self.requests {
            let _ = requests.send(());
        }
    }

    pub fn try_recv(&self) -> Option<Outcome> {
        self.results.try_recv().ok()
    }
}

fn worker(requests: Receiver<()>, results: Sender<Outcome>, wake: Arc<WakeEvent>) {
    let _com = ComApartment::enter();

    let Some(url) = update_check::latest_release_url(env!("CARGO_PKG_REPOSITORY")) else {
        // Only reachable if the repository field stops being a GitHub URL, in
        // which case there is nowhere to ask and no point holding the thread.
        log("updater: no GitHub repository to check; update checks are off");
        return;
    };

    let http = match Http::new() {
        Ok(http) => http,
        Err(e) => {
            log(&format!(
                "updater: no HTTP client ({e}); update checks are off"
            ));
            return;
        }
    };

    while requests.recv().is_ok() {
        // A backlog means the same question asked twice; one answer covers it.
        while requests.try_recv().is_ok() {}

        let outcome = match http.get_with_user_agent(&url, Some(USER_AGENT)) {
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
        };

        if results.send(outcome).is_err() {
            break;
        }
        let _ = wake.set();
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
