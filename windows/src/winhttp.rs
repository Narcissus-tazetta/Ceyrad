//! Blocking HTTP over WinRT's stack, plus the COM apartment a caller thread
//! needs to use it.
//!
//! WinRT is already in the process for SMTC, so reaching the network this way
//! costs no extra dependency. What it does not give us is a deadline: a
//! WinRT async operation has no timeout of its own, and `join()` waits forever.
//! A stalled request would therefore park its worker thread for good and every
//! later lookup would queue behind it — so the wait is done by hand and the
//! operation is cancelled when it overruns.
//!
//! Shared by `catalog` and `updater`, which each own a thread that does nothing
//! but block on one of these.

use std::sync::{Arc, Condvar, Mutex};
use std::time::{Duration, Instant};

use windows::core::{Result as WinResult, RuntimeType, HRESULT, HSTRING};
use windows::Foundation::Uri;
use windows::Web::Http::{HttpClient, HttpMethod, HttpRequestMessage};
use windows::Win32::Foundation::ERROR_TIMEOUT;
use windows::Win32::System::Com::{CoInitializeEx, CoUninitialize, COINIT_MULTITHREADED};
use windows_future::{AsyncOperationWithProgressCompletedHandler, IAsyncOperationWithProgress};

/// Ceiling on one request, matching the macOS build's `timeoutIntervalForRequest`.
pub const REQUEST_TIMEOUT: Duration = Duration::from_secs(10);
/// Ceiling on request plus body, matching `timeoutIntervalForResource`.
pub const RESOURCE_TIMEOUT: Duration = Duration::from_secs(15);

/// Holds this thread's COM apartment for as long as it is alive.
///
/// WinRT apartments are per-thread. A worker that skips this survives only on
/// whatever the process happens to have already, which is a dependency on the
/// main thread's choice that would break the moment that choice changed.
pub struct ComApartment;

impl ComApartment {
    pub fn enter() -> Self {
        // MTA: a worker owns no window and pumps no messages, and every WinRT
        // call it makes is made from this one thread.
        unsafe {
            let _ = CoInitializeEx(None, COINIT_MULTITHREADED);
        }
        Self
    }
}

impl Drop for ComApartment {
    fn drop(&mut self) {
        unsafe { CoUninitialize() };
    }
}

pub struct Http {
    client: HttpClient,
}

impl Http {
    pub fn new() -> WinResult<Self> {
        Ok(Self {
            client: HttpClient::new()?,
        })
    }

    /// Blocks this thread until the response is complete, or until the timeouts
    /// run out. Safe only because nothing else runs on a worker thread — the
    /// event loop is elsewhere.
    pub fn get(&self, url: &str) -> WinResult<String> {
        self.get_with_user_agent(url, None)
    }

    /// As `get`, with a `User-Agent`. Some APIs — GitHub's among them — refuse
    /// a request that does not identify itself, so the header is not optional
    /// there even though HTTP says it is.
    pub fn get_with_user_agent(&self, url: &str, user_agent: Option<&str>) -> WinResult<String> {
        let deadline = Instant::now() + RESOURCE_TIMEOUT;
        let uri = Uri::CreateUri(&HSTRING::from(url))?;

        let request = HttpRequestMessage::Create(&HttpMethod::Get()?, &uri)?;
        if let Some(user_agent) = user_agent {
            request
                .Headers()?
                .Append(&HSTRING::from("User-Agent"), &HSTRING::from(user_agent))?;
        }

        let response = join_with_timeout(
            &self.client.SendRequestAsync(&request)?,
            REQUEST_TIMEOUT.min(remaining(deadline)),
        )?;
        response.EnsureSuccessStatusCode()?;

        let body = join_with_timeout(
            &response.Content()?.ReadAsStringAsync()?,
            remaining(deadline),
        )?;
        Ok(body.to_string())
    }
}

fn remaining(deadline: Instant) -> Duration {
    deadline.saturating_duration_since(Instant::now())
}

/// Waits for a WinRT async operation, cancelling it if it overruns.
pub fn join_with_timeout<T, P>(
    operation: &IAsyncOperationWithProgress<T, P>,
    timeout: Duration,
) -> WinResult<T>
where
    T: RuntimeType + 'static,
    P: RuntimeType + 'static,
{
    let gate = Arc::new((Mutex::new(false), Condvar::new()));
    let signal = Arc::clone(&gate);
    operation.SetCompleted(&AsyncOperationWithProgressCompletedHandler::new(
        move |_, _| {
            let (done, ready) = &*signal;
            // A poisoned lock cannot happen — the only other holder is the wait
            // below, which does not panic while holding it — but a completion
            // handler must not unwind into WinRT either way.
            if let Ok(mut done) = done.lock() {
                *done = true;
            }
            ready.notify_all();
            Ok(())
        },
    ))?;

    let (done, ready) = &*gate;
    let guard = done.lock().map_err(|_| timed_out())?;
    let (_guard, wait) = ready
        .wait_timeout_while(guard, timeout, |done| !*done)
        .map_err(|_| timed_out())?;

    if wait.timed_out() {
        // The operation is still outstanding; hand it back so the connection is
        // torn down rather than left dangling on this thread's behalf.
        let _ = operation.Cancel();
        return Err(timed_out());
    }
    operation.GetResults()
}

pub fn timed_out() -> windows::core::Error {
    windows::core::Error::new(
        HRESULT::from_win32(ERROR_TIMEOUT.0),
        "the request timed out",
    )
}
