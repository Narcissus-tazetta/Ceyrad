//! Port of Tests/CeyradTests/DebouncerTests.swift, plus the ceiling that the
//! macOS version has no need for (see `Debouncer`).

use std::time::{Duration, Instant};

use ceyrad::core::debouncer::Debouncer;

const DELAY: Duration = Duration::from_millis(800);
const MAX_DELAY: Duration = Duration::from_millis(2_000);

fn debouncer() -> Debouncer {
    Debouncer::new(DELAY, MAX_DELAY)
}

#[test]
fn only_the_last_schedule_of_a_burst_fires() {
    let mut debouncer = debouncer();
    let start = Instant::now();

    debouncer.schedule(start);
    // 400ms in, a second event pushes the trailing edge out.
    debouncer.schedule(start + Duration::from_millis(400));

    assert!(
        !debouncer.take_if_due(start + DELAY),
        "the first deadline was superseded"
    );
    assert!(debouncer.take_if_due(start + Duration::from_millis(1_200)));
}

#[test]
fn cancel_prevents_the_fire() {
    let mut debouncer = debouncer();
    let start = Instant::now();
    debouncer.schedule(start);
    debouncer.cancel();

    assert!(!debouncer.is_pending());
    assert!(!debouncer.take_if_due(start + Duration::from_secs(10)));
}

#[test]
fn a_scheduled_fire_happens_at_most_once() {
    let mut debouncer = debouncer();
    let start = Instant::now();
    debouncer.schedule(start);

    assert!(debouncer.take_if_due(start + DELAY));
    assert!(!debouncer.take_if_due(start + DELAY));
    assert!(!debouncer.is_pending());
}

#[test]
fn nothing_fires_before_the_deadline() {
    let mut debouncer = debouncer();
    let start = Instant::now();
    let deadline = debouncer.schedule(start);

    assert_eq!(deadline, start + DELAY);
    assert!(!debouncer.take_if_due(start + Duration::from_millis(799)));
    assert!(debouncer.is_pending());
}

#[test]
fn an_unbroken_stream_of_events_still_fires_at_the_ceiling() {
    let mut debouncer = debouncer();
    let start = Instant::now();

    // Apple Music for Windows raises a timeline event roughly every 280ms, so
    // a plain trailing edge would be pushed forward forever.
    let mut now = start;
    for _ in 0..20 {
        debouncer.schedule(now);
        now += Duration::from_millis(280);
    }

    assert!(
        debouncer.take_if_due(start + MAX_DELAY),
        "the ceiling is anchored to the first schedule of the burst"
    );
}

#[test]
fn the_ceiling_never_delays_a_fire_past_max_delay() {
    let mut debouncer = debouncer();
    let start = Instant::now();

    debouncer.schedule(start);
    let deadline = debouncer.schedule(start + Duration::from_millis(1_900));

    assert_eq!(deadline, start + MAX_DELAY, "clamped to the ceiling");
}

#[test]
fn a_new_burst_gets_a_full_ceiling_of_its_own() {
    let mut debouncer = debouncer();
    let start = Instant::now();

    debouncer.schedule(start);
    assert!(debouncer.take_if_due(start + DELAY));

    // Firing releases the ceiling, so the next burst is not measured from the
    // previous one and does not fire immediately.
    let later = start + Duration::from_secs(60);
    let deadline = debouncer.schedule(later);
    assert_eq!(deadline, later + DELAY);
}
