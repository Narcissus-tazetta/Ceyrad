//! Exponential backoff for reconnecting to Discord.
//!
//! Discord may be closed, restarting, or updating. Retrying at a fixed interval
//! either gives up too soon or hammers a machine that is busy doing something
//! else, so the wait doubles and then holds. Matches the macOS build's
//! `ReconnectBackoff`.

use std::time::Duration;

/// 1s doubling to this ceiling.
const MAX_DELAY_SECS: u64 = 60;
/// 2^6 = 64s is already past the ceiling, so counting further buys nothing —
/// and keeps the shift below from ever overflowing.
const MAX_ATTEMPT: u32 = 6;

#[derive(Debug, Default)]
pub struct Backoff {
    attempt: u32,
}

impl Backoff {
    /// How long to wait before the next attempt, and count this one.
    pub fn next_delay(&mut self) -> Duration {
        let delay = MAX_DELAY_SECS.min(1u64 << self.attempt);
        self.attempt = (self.attempt + 1).min(MAX_ATTEMPT);
        Duration::from_secs(delay)
    }

    /// Something worked, so the next failure starts over at one second.
    pub fn reset(&mut self) {
        self.attempt = 0;
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn the_delay_doubles_and_then_holds_at_the_ceiling() {
        let mut backoff = Backoff::default();
        let delays: Vec<u64> = (0..9).map(|_| backoff.next_delay().as_secs()).collect();
        assert_eq!(delays, vec![1, 2, 4, 8, 16, 32, 60, 60, 60]);
    }

    #[test]
    fn a_successful_connection_starts_the_series_over() {
        let mut backoff = Backoff::default();
        for _ in 0..5 {
            backoff.next_delay();
        }
        backoff.reset();
        assert_eq!(backoff.next_delay().as_secs(), 1);
    }

    /// The shift in `next_delay` would overflow past 63 attempts; the cap is
    /// what keeps it in range however long Discord stays away.
    #[test]
    fn the_attempt_counter_cannot_run_away() {
        let mut backoff = Backoff::default();
        for _ in 0..1_000 {
            backoff.next_delay();
        }
        assert_eq!(backoff.attempt, MAX_ATTEMPT);
        assert_eq!(backoff.next_delay().as_secs(), MAX_DELAY_SECS);
    }
}
