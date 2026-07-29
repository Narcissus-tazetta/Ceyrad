//! Waiting on WinRT async operations with a deadline.
//!
//! A WinRT async operation has no timeout of its own and `join()` waits forever.
//! That is not acceptable anywhere in this app: on a worker thread it parks the
//! thread for good and queues every later request behind it, and on the event
//! loop's own thread it freezes the whole app — no tray menu, no Discord PING
//! replies, no way to quit. So every wait is done by hand and the operation is
//! cancelled when it overruns.
//!
//! Both async shapes the app touches are covered: `IAsyncOperation<T>` (SMTC)
//! and `IAsyncOperationWithProgress<T, P>` (HTTP).

use std::sync::{Arc, Condvar, Mutex};
use std::time::Duration;

use windows::core::{Result as WinResult, RuntimeType, HRESULT};
use windows::Win32::Foundation::ERROR_TIMEOUT;
use windows_future::{
    AsyncOperationCompletedHandler, AsyncOperationWithProgressCompletedHandler, IAsyncOperation,
    IAsyncOperationWithProgress,
};

/// A WinRT async operation this app knows how to wait on.
///
/// The two shapes differ only in the handler type they take, so unifying them
/// here keeps one timeout implementation rather than two that could drift.
pub trait Awaitable {
    type Output;

    /// Runs `on_done` once the operation completes, however it completes.
    fn on_completed(&self, on_done: impl Fn() + Send + 'static) -> WinResult<()>;
    fn cancel(&self);
    fn results(&self) -> WinResult<Self::Output>;
}

impl<T: RuntimeType + 'static> Awaitable for IAsyncOperation<T> {
    type Output = T;

    fn on_completed(&self, on_done: impl Fn() + Send + 'static) -> WinResult<()> {
        self.SetCompleted(&AsyncOperationCompletedHandler::new(move |_, _| {
            on_done();
            Ok(())
        }))
    }

    fn cancel(&self) {
        let _ = self.Cancel();
    }

    fn results(&self) -> WinResult<T> {
        self.GetResults()
    }
}

impl<T: RuntimeType + 'static, P: RuntimeType + 'static> Awaitable
    for IAsyncOperationWithProgress<T, P>
{
    type Output = T;

    fn on_completed(&self, on_done: impl Fn() + Send + 'static) -> WinResult<()> {
        self.SetCompleted(&AsyncOperationWithProgressCompletedHandler::new(
            move |_, _| {
                on_done();
                Ok(())
            },
        ))
    }

    fn cancel(&self) {
        let _ = self.Cancel();
    }

    fn results(&self) -> WinResult<T> {
        self.GetResults()
    }
}

/// Waits for a WinRT async operation, cancelling it if it overruns.
pub fn join_with_timeout<A: Awaitable>(operation: &A, timeout: Duration) -> WinResult<A::Output> {
    let gate = Arc::new((Mutex::new(false), Condvar::new()));
    let signal = Arc::clone(&gate);
    operation.on_completed(move || {
        let (done, ready) = &*signal;
        // A poisoned lock cannot happen — the only other holder is the wait
        // below, which does not panic while holding it — but a completion
        // handler must not unwind into WinRT either way.
        if let Ok(mut done) = done.lock() {
            *done = true;
        }
        ready.notify_all();
    })?;

    let (done, ready) = &*gate;
    let guard = done.lock().map_err(|_| timed_out())?;
    let (_guard, wait) = ready
        .wait_timeout_while(guard, timeout, |done| !*done)
        .map_err(|_| timed_out())?;

    if wait.timed_out() {
        // The operation is still outstanding; hand it back so the connection is
        // torn down rather than left dangling on this thread's behalf.
        operation.cancel();
        return Err(timed_out());
    }
    operation.results()
}

pub fn timed_out() -> windows::core::Error {
    windows::core::Error::new(
        HRESULT::from_win32(ERROR_TIMEOUT.0),
        "the request timed out",
    )
}
