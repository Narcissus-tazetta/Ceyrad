//! What Discord is currently showing.
//!
//! Kept so an unchanged rebuild is not re-sent: the loop rebuilds the activity
//! whenever anything moves, and the rate limit only allows a handful of updates
//! per 20 seconds. The subtlety is that this record has to be **forgotten**
//! every time the connection goes away or comes back — a fresh pipe shows
//! nothing, so a remembered send would make the first push after a reconnect
//! look redundant and the presence would never appear at all.
//!
//! Hence the two-level option: "nothing sent over this connection" and "sent
//! the instruction to clear" are different states, and conflating them is
//! exactly the bug this type exists to make impossible.

use serde_json::{Map, Value};

use super::activity_builder;

pub type Activity = Map<String, Value>;

#[derive(Debug, Default)]
pub struct Presence {
    last_sent: Option<Option<Activity>>,
}

impl Presence {
    /// Nothing is on screen any more, or nothing is known to be: a connection
    /// that dropped, or one that has just come up. Anything sent next counts
    /// as a change.
    pub fn forget(&mut self) {
        self.last_sent = None;
    }

    /// Whether Discord would render exactly what it is already showing.
    pub fn would_render_the_same(&self, activity: &Option<Activity>) -> bool {
        let Some(previous) = &self.last_sent else {
            return false;
        };
        match (previous, activity) {
            (None, None) => true,
            (Some(previous), Some(activity)) => activity_builder::is_equivalent(previous, activity),
            _ => false,
        }
    }

    /// Records what actually went out. Only ever called after a successful
    /// send: a push that failed or never left must not suppress the next one.
    pub fn note_sent(&mut self, activity: Option<Activity>) {
        self.last_sent = Some(activity);
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn activity(details: &str, start: i64) -> Activity {
        let mut map = Activity::new();
        map.insert("type".into(), Value::from(2));
        map.insert("details".into(), Value::from(details));
        map.insert(
            "timestamps".into(),
            serde_json::json!({ "start": start, "end": start + 200_000 }),
        );
        map
    }

    #[test]
    fn nothing_sent_yet_is_not_the_same_as_having_sent_clear() {
        let mut presence = Presence::default();
        assert!(!presence.would_render_the_same(&None));
        assert!(!presence.would_render_the_same(&Some(activity("Song", 0))));

        presence.note_sent(None);
        assert!(presence.would_render_the_same(&None), "cleared twice");
        assert!(!presence.would_render_the_same(&Some(activity("Song", 0))));
    }

    #[test]
    fn forgetting_makes_the_next_push_go_out_whatever_it_is() {
        // The reconnect case: Discord is showing nothing, so even a repeat of
        // the last activity has to be sent again.
        let mut presence = Presence::default();
        let current = activity("Song", 0);
        presence.note_sent(Some(current.clone()));
        assert!(presence.would_render_the_same(&Some(current.clone())));

        presence.forget();
        assert!(!presence.would_render_the_same(&Some(current)));
        assert!(!presence.would_render_the_same(&None));
    }

    #[test]
    fn an_unchanged_rebuild_is_recognised_despite_timestamp_drift() {
        let mut presence = Presence::default();
        presence.note_sent(Some(activity("Song", 1_000_000)));
        // The same playback, rebuilt a moment later.
        assert!(presence.would_render_the_same(&Some(activity("Song", 1_000_900))));
        // A seek is not drift.
        assert!(!presence.would_render_the_same(&Some(activity("Song", 1_030_000))));
        // Nor is a different track.
        assert!(!presence.would_render_the_same(&Some(activity("Other", 1_000_000))));
    }
}
