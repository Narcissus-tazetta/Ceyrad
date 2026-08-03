//! Every deadline the event loop waits on, in one place.
//!
//! The loop arms its wait with the earliest of them and then asks each in turn
//! whether it has come due. Splitting that across a hand-written list of
//! `Option<Instant>` fields meant two lists that had to agree: one enumerating
//! them for the wait, one firing them. A timer added to the first and forgotten
//! in the second fires late; forgotten in the first, it fires only when
//! something else happens to wake the loop — and neither shows up as anything
//! but "sometimes it takes a while".
//!
//! Here the deadlines live in an array indexed by [`Timer`], so [`Timers::next`]
//! covers every variant by construction. Only the *action* stays per-timer,
//! which is the part that genuinely differs.

use std::time::{Duration, Instant};

/// One deadline the loop can arm.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Timer {
    /// When a presence that has been paused long enough should be cleared.
    PauseHide,
    /// When to try Discord again after a connection dropped.
    Reconnect,
    /// How long Discord has left to answer a handshake with READY.
    Handshake,
    /// When to ask GitHub about a newer release.
    UpdateCheck,
    /// When a catalog lookup that failed outright may be retried.
    CatalogRetry,
}

impl Timer {
    pub const COUNT: usize = 5;
    pub const ALL: [Timer; Self::COUNT] = [
        Timer::PauseHide,
        Timer::Reconnect,
        Timer::Handshake,
        Timer::UpdateCheck,
        Timer::CatalogRetry,
    ];

    fn index(self) -> usize {
        match self {
            Timer::PauseHide => 0,
            Timer::Reconnect => 1,
            Timer::Handshake => 2,
            Timer::UpdateCheck => 3,
            Timer::CatalogRetry => 4,
        }
    }
}

#[derive(Debug, Default)]
pub struct Timers {
    deadlines: [Option<Instant>; Timer::COUNT],
}

impl Timers {
    pub fn arm_at(&mut self, timer: Timer, at: Instant) {
        self.deadlines[timer.index()] = Some(at);
    }

    pub fn arm_in(&mut self, timer: Timer, after: Duration) {
        self.arm_at(timer, Instant::now() + after);
    }

    pub fn cancel(&mut self, timer: Timer) {
        self.deadlines[timer.index()] = None;
    }

    pub fn is_armed(&self, timer: Timer) -> bool {
        self.deadlines[timer.index()].is_some()
    }

    /// Whether this deadline has passed, consuming it if so.
    ///
    /// Consuming is the point: a deadline read without being cleared stays in
    /// the past, and the loop then computes a zero-length wait forever — a
    /// pegged core with nothing in the log to explain it.
    pub fn take_if_due(&mut self, timer: Timer, now: Instant) -> bool {
        let slot = &mut self.deadlines[timer.index()];
        let due = slot.is_some_and(|at| at <= now);
        if due {
            *slot = None;
        }
        due
    }

    /// The earliest armed deadline, or `None` when the loop may sleep until
    /// something wakes it.
    pub fn next(&self) -> Option<Instant> {
        self.deadlines.iter().flatten().min().copied()
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn all_stays_in_step_with_count() {
        // `index` maps into a fixed-size array; a variant added without
        // widening it would panic on the first arm.
        assert_eq!(Timer::ALL.len(), Timer::COUNT);
        let mut seen: Vec<usize> = Timer::ALL.iter().map(|t| t.index()).collect();
        seen.sort_unstable();
        seen.dedup();
        assert_eq!(seen.len(), Timer::COUNT, "two timers share an index");
        assert!(seen.iter().all(|&i| i < Timer::COUNT));
    }

    #[test]
    fn nothing_armed_means_nothing_to_wait_for() {
        assert_eq!(Timers::default().next(), None);
    }

    #[test]
    fn next_is_the_earliest_of_everything_armed() {
        // The property the old hand-written list could get wrong: every
        // variant has to be considered, not just the ones someone remembered.
        let now = Instant::now();
        for timer in Timer::ALL {
            let mut timers = Timers::default();
            // Arm them all, with this one earliest.
            for (offset, other) in Timer::ALL.into_iter().enumerate() {
                timers.arm_at(other, now + Duration::from_secs(10 + offset as u64));
            }
            timers.arm_at(timer, now + Duration::from_secs(1));
            assert_eq!(
                timers.next(),
                Some(now + Duration::from_secs(1)),
                "{timer:?} was not considered"
            );
        }
    }

    #[test]
    fn a_due_deadline_is_consumed_exactly_once() {
        let now = Instant::now();
        let mut timers = Timers::default();
        timers.arm_at(Timer::Reconnect, now);

        assert!(timers.take_if_due(Timer::Reconnect, now));
        assert!(
            !timers.take_if_due(Timer::Reconnect, now),
            "a fired deadline must not fire again"
        );
        assert!(!timers.is_armed(Timer::Reconnect));
        assert_eq!(timers.next(), None, "a past deadline would spin the loop");
    }

    #[test]
    fn a_future_deadline_is_left_alone() {
        let now = Instant::now();
        let mut timers = Timers::default();
        timers.arm_at(Timer::PauseHide, now + Duration::from_secs(60));

        assert!(!timers.take_if_due(Timer::PauseHide, now));
        assert!(timers.is_armed(Timer::PauseHide));
    }

    #[test]
    fn timers_do_not_interfere_with_each_other() {
        let now = Instant::now();
        let mut timers = Timers::default();
        for timer in Timer::ALL {
            timers.arm_at(timer, now);
        }
        assert!(timers.take_if_due(Timer::Handshake, now));
        for timer in Timer::ALL {
            if timer != Timer::Handshake {
                assert!(timers.is_armed(timer), "{timer:?} was cleared too");
            }
        }
    }
}
