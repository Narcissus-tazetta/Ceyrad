//! Discord IPC client: one connection's worth of state machine.
//!
//! Owns the pipe, the frame decoder and the handshake/READY dance. It has no
//! timers and no reconnect policy of its own — the caller drives it from its
//! own event loop, which is what lets a single thread wait on Discord traffic
//! and on music events at the same time.

use std::io;
use std::time::Duration;

use serde_json::Value;
use windows::Win32::Foundation::HANDLE;

use super::pipe::{Pipe, Wakeup};
use super::protocol::{
    handshake_payload, set_activity_payload, FrameDecoder, Opcode, ProtocolError,
};
use crate::core::models::ConnState;

const READ_BUF_LEN: usize = 4096;

/// Something the caller has to react to. Ordinary traffic (PONGs, command
/// replies) is handled internally and reported as nothing at all.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum Event {
    /// The handshake completed; activities sent from now on will stick.
    Ready,
    /// Discord went away. The client is already back to `Disconnected`.
    Closed,
    /// Discord rejected something — a bad client id, a malformed activity, a
    /// rate limit. Diagnostic only; the connection state is reported
    /// separately, and an ERROR frame does not by itself end the connection.
    Error(String),
}

pub struct Client {
    pipe: Option<Pipe>,
    decoder: FrameDecoder,
    buf: Vec<u8>,
    state: ConnState,
    /// The id used for the live connection. "Listening to <app name>" is
    /// resolved from the Discord application behind it, so switching sources
    /// means reconnecting under a different id.
    client_id: Option<&'static str>,
    nonce: u64,
}

impl Default for Client {
    fn default() -> Self {
        Self::new()
    }
}

impl Client {
    pub fn new() -> Self {
        Self {
            pipe: None,
            decoder: FrameDecoder::new(),
            buf: vec![0; READ_BUF_LEN],
            state: ConnState::Disconnected,
            client_id: None,
            nonce: 0,
        }
    }

    pub fn state(&self) -> ConnState {
        self.state
    }

    pub fn client_id(&self) -> Option<&'static str> {
        self.client_id
    }

    /// Opens the pipe and sends the handshake. Success here only means
    /// `Connecting`; the connection is usable once `poll` yields `Ready`.
    pub fn connect(&mut self, client_id: &'static str) -> io::Result<()> {
        self.disconnect(false);

        let pipe = Pipe::connect()?;
        pipe.send(Opcode::Handshake, &handshake_payload(client_id))?;

        self.pipe = Some(pipe);
        self.decoder = FrameDecoder::new();
        self.state = ConnState::Connecting;
        self.client_id = Some(client_id);
        Ok(())
    }

    /// Drops the connection. `clear_activity` sends a final null activity so a
    /// deliberate teardown doesn't leave a stale presence behind.
    pub fn disconnect(&mut self, clear_activity: bool) {
        if let Some(pipe) = &self.pipe {
            if clear_activity && self.state == ConnState::Connected {
                let payload =
                    set_activity_payload(None, std::process::id(), &next_nonce(&mut self.nonce));
                let _ = pipe.send(Opcode::Frame, &payload);
                // The write is queued in the kernel, not delivered; give
                // Discord a moment before the handle closes under it.
                std::thread::sleep(Duration::from_millis(150));
            }
        }
        self.pipe = None;
        self.state = ConnState::Disconnected;
        self.client_id = None;
    }

    /// `None` clears the presence. Fails only if the pipe broke, in which case
    /// the caller sees `Closed` from the next `poll`.
    pub fn set_activity(&mut self, activity: Option<Value>) -> io::Result<()> {
        let Some(pipe) = &self.pipe else {
            return Ok(());
        };
        if self.state != ConnState::Connected {
            return Ok(());
        }
        let payload =
            set_activity_payload(activity, std::process::id(), &next_nonce(&mut self.nonce));
        pipe.send(Opcode::Frame, &payload)
    }

    /// Waits for Discord traffic, for `wake` to be signalled, or for `timeout`.
    ///
    /// Only valid while connected — a disconnected caller has no pipe to wait
    /// on and should wait on its own wakeup handle instead.
    pub fn poll(
        &mut self,
        wake: Option<HANDLE>,
        timeout: Option<Duration>,
        events: &mut Vec<Event>,
    ) -> io::Result<Wakeup> {
        let Some(pipe) = &self.pipe else {
            return Ok(Wakeup::Timeout);
        };

        let wakeup = match pipe.read_or_signal(&mut self.buf, wake, timeout) {
            Ok(wakeup) => wakeup,
            Err(e) => {
                self.state = ConnState::Disconnected;
                self.pipe = None;
                self.client_id = None;
                events.push(Event::Closed);
                return Err(e);
            }
        };

        match wakeup {
            Wakeup::Data(n) => {
                self.decoder.push(&self.buf[..n]);
                self.drain_frames(events)?;
            }
            Wakeup::Closed => {
                self.state = ConnState::Disconnected;
                self.pipe = None;
                self.client_id = None;
                events.push(Event::Closed);
            }
            // Someone else's business: the caller woke us to do something of
            // its own, or to drain its message queue.
            Wakeup::Signaled | Wakeup::Message | Wakeup::Timeout => {}
        }
        Ok(wakeup)
    }

    fn drain_frames(&mut self, events: &mut Vec<Event>) -> io::Result<()> {
        loop {
            let frame = match self.decoder.next_frame() {
                Ok(None) => return Ok(()),
                Ok(Some(frame)) => frame,
                Err(ProtocolError::PayloadTooLarge(len)) => {
                    self.fail(events);
                    return Err(io::Error::other(format!("frame too large: {len}")));
                }
                Err(ProtocolError::UnknownOpcode(op)) => {
                    self.fail(events);
                    return Err(io::Error::other(format!("unknown opcode: {op}")));
                }
            };

            match frame.opcode {
                Opcode::Frame => match frame.event().as_deref() {
                    Some("READY") if self.state == ConnState::Connecting => {
                        self.state = ConnState::Connected;
                        events.push(Event::Ready);
                    }
                    Some("ERROR") => events.push(Event::Error(frame.error_message())),
                    // A SET_ACTIVITY reply, or a READY we already acted on.
                    _ => {}
                },
                // Discord expects the payload echoed back verbatim.
                Opcode::Ping => {
                    if let Some(pipe) = &self.pipe {
                        let json = frame.payload_json().unwrap_or(Value::Null);
                        pipe.send(Opcode::Pong, &json)?;
                    }
                }
                Opcode::Close => {
                    events.push(Event::Error(format!("closed: {}", frame.error_message())));
                    self.fail(events);
                    return Ok(());
                }
                Opcode::Handshake | Opcode::Pong => {}
            }
        }
    }

    fn fail(&mut self, events: &mut Vec<Event>) {
        self.pipe = None;
        self.client_id = None;
        self.state = ConnState::Disconnected;
        events.push(Event::Closed);
    }
}

fn next_nonce(counter: &mut u64) -> String {
    *counter += 1;
    format!("ceyrad-{counter}")
}
