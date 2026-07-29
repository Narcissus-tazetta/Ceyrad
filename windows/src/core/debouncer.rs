use std::time::{Duration, Instant};

/// Trailing-edge debounce with a ceiling on how long it may postpone a send.
///
/// The macOS build debounces distributed notifications, which arrive only when
/// something actually happens, so a plain trailing edge always finds its moment
/// of quiet. SMTC does not work that way: Apple Music for Windows raises
/// `TimelinePropertiesChanged` roughly every 280ms for the whole length of a
/// track. That is faster than `delay`, so a deadline pushed forward on every
/// event would never be reached and the presence would freeze on whatever went
/// out first. `max_delay` — anchored to the first `schedule` of a burst, not to
/// the latest — bounds the total wait while keeping the burst collapsing.
///
/// The timer itself belongs to the caller: this owns the deadline and hands it
/// out, so a single-threaded event loop can fold it into one wait alongside
/// everything else it watches.
#[derive(Debug)]
pub struct Debouncer {
    delay: Duration,
    max_delay: Duration,
    deadline: Option<Instant>,
    /// The point `deadline` may not be pushed past, fixed by the first
    /// `schedule` of the current burst.
    ceiling: Option<Instant>,
}

impl Debouncer {
    pub fn new(delay: Duration, max_delay: Duration) -> Self {
        Self {
            delay,
            max_delay,
            deadline: None,
            ceiling: None,
        }
    }

    /// Arms, or re-arms, the trailing edge and returns when it will come due.
    pub fn schedule(&mut self, now: Instant) -> Instant {
        let ceiling = *self.ceiling.get_or_insert(now + self.max_delay);
        let deadline = (now + self.delay).min(ceiling);
        self.deadline = Some(deadline);
        deadline
    }

    /// Disarms without firing, and releases the ceiling so the next burst gets
    /// a full window of its own.
    pub fn cancel(&mut self) {
        self.deadline = None;
        self.ceiling = None;
    }

    /// When the armed fire is due, for callers folding this into a wait.
    pub fn deadline(&self) -> Option<Instant> {
        self.deadline
    }

    pub fn is_pending(&self) -> bool {
        self.deadline.is_some()
    }

    /// True exactly once, on the first call at or after the armed deadline.
    /// Disarms, so a burst collapses to a single fire.
    pub fn take_if_due(&mut self, now: Instant) -> bool {
        match self.deadline {
            Some(deadline) if deadline <= now => {
                self.cancel();
                true
            }
            _ => false,
        }
    }
}
