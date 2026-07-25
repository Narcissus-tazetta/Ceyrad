use std::time::Duration;

/// Trailing-edge debounce bookkeeping.
///
/// The platform layer owns the actual timer; this tracks which pending fire is
/// still live. `schedule` hands back a token, and only the newest un-cancelled
/// token is allowed to run — so a burst of scheduling collapses to its last
/// call, matching the macOS `DispatchWorkItem` version.
#[derive(Debug)]
pub struct Debouncer {
    delay: Duration,
    pending: Option<u64>,
    next_token: u64,
}

impl Debouncer {
    pub fn new(delay: Duration) -> Self {
        Self {
            delay,
            pending: None,
            next_token: 0,
        }
    }

    pub fn delay(&self) -> Duration {
        self.delay
    }

    /// Invalidates any pending fire and returns the token for the new one.
    pub fn schedule(&mut self) -> u64 {
        self.next_token += 1;
        let token = self.next_token;
        self.pending = Some(token);
        token
    }

    pub fn cancel(&mut self) {
        self.pending = None;
    }

    pub fn is_pending(&self) -> bool {
        self.pending.is_some()
    }

    /// True only for the newest scheduled token that hasn't been cancelled.
    /// Consumes the pending state, so a token fires at most once.
    pub fn should_fire(&mut self, token: u64) -> bool {
        if self.pending == Some(token) {
            self.pending = None;
            true
        } else {
            false
        }
    }
}
