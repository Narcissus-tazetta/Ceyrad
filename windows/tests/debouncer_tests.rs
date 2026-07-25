//! Port of Tests/CeyradTests/DebouncerTests.swift

use std::time::Duration;

use ceyrad::core::debouncer::Debouncer;

#[test]
fn only_last_scheduled_block_runs() {
    let mut debouncer = Debouncer::new(Duration::from_millis(800));
    let first = debouncer.schedule();
    let second = debouncer.schedule();

    assert!(!debouncer.should_fire(first), "first should be superseded");
    assert!(debouncer.should_fire(second), "second should run");
}

#[test]
fn cancel_prevents_execution() {
    let mut debouncer = Debouncer::new(Duration::from_millis(800));
    let token = debouncer.schedule();
    debouncer.cancel();

    assert!(!debouncer.should_fire(token));
    assert!(!debouncer.is_pending());
}

#[test]
fn a_token_fires_at_most_once() {
    let mut debouncer = Debouncer::new(Duration::from_millis(800));
    let token = debouncer.schedule();
    assert!(debouncer.should_fire(token));
    assert!(!debouncer.should_fire(token));
}
