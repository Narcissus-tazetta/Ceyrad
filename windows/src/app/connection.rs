//! The Discord connection: the pipe, what it is showing, and when to retry.
//!
//! Thin on purpose. The two pieces of state that used to be loose fields of
//! `App` — the record of what Discord shows and the reconnect attempt count —
//! live in `core::presence` and `core::backoff`, which are platform-free and
//! carry their own tests. What is left here is the wiring, so that every path
//! through the pipe passes through one place that keeps them in step.

use std::io;
use std::time::Duration;

use windows::Win32::Foundation::HANDLE;

use crate::core::apple_music;
use crate::core::backoff::Backoff;
use crate::core::models::ConnState;
use crate::core::presence::{Activity, Presence};
use crate::discord::client::{Client, Event};
use crate::discord::pipe::Wakeup;

/// What became of a send.
#[derive(Debug)]
pub enum Sent {
    /// Discord would render exactly what it is already showing.
    Unchanged,
    /// Went out. Carries the one-line description for the log.
    Sent(String),
    /// No pipe to send it over. The caller drops it: whatever is built after
    /// READY is more current than anything held from before.
    NotConnected,
    Failed(io::Error),
}

#[derive(Default)]
pub struct Connection {
    client: Client,
    presence: Presence,
    backoff: Backoff,
}

impl Connection {
    pub fn new() -> Self {
        Self::default()
    }

    pub fn state(&self) -> ConnState {
        self.client.state()
    }

    pub fn is_disconnected(&self) -> bool {
        self.client.state() == ConnState::Disconnected
    }

    /// Opens a pipe to Discord. The caller arms the handshake timeout on `Ok`.
    pub fn connect(&mut self) -> io::Result<()> {
        self.presence.forget();
        self.client.connect(apple_music::DISCORD_CLIENT_ID)
    }

    pub fn disconnect(&mut self, clear_activity: bool) {
        self.client.disconnect(clear_activity);
        self.presence.forget();
    }

    pub fn poll(
        &mut self,
        wake: Option<HANDLE>,
        timeout: Option<Duration>,
        events: &mut Vec<Event>,
    ) -> io::Result<Wakeup> {
        self.client.poll(wake, timeout, events)
    }

    /// Discord answered the handshake. A fresh connection shows nothing, so the
    /// previous send must not suppress the first push over this one.
    pub fn note_ready(&mut self) {
        self.backoff.reset();
        self.presence.forget();
    }

    pub fn note_closed(&mut self) {
        self.presence.forget();
    }

    /// Sends unless Discord would render the same thing.
    pub fn send(&mut self, activity: Option<Activity>) -> Sent {
        if self.client.state() != ConnState::Connected {
            return Sent::NotConnected;
        }
        if self.presence.would_render_the_same(&activity) {
            return Sent::Unchanged;
        }

        let payload = activity.clone().map(serde_json::Value::Object);
        match self.client.set_activity(payload) {
            Ok(()) => {
                let description = crate::core::activity_builder::describe(&activity);
                // Only after the write succeeded: a send that never left must
                // not suppress the next one.
                self.presence.note_sent(activity);
                Sent::Sent(description)
            }
            Err(e) => Sent::Failed(e),
        }
    }

    /// How long to wait before trying again, doubling each time.
    pub fn next_backoff(&mut self) -> Duration {
        self.backoff.next_delay()
    }

    pub fn reset_backoff(&mut self) {
        self.backoff.reset();
    }
}
