//! Discord IPC framing.
//!
//! Transport-independent: macOS uses a unix socket and Windows a named pipe,
//! but the frames on the wire are identical. Every frame is an 8-byte
//! little-endian header (opcode, then JSON byte length) followed by the JSON
//! body.

use serde_json::{json, Value};

/// A length past this is treated as a protocol violation rather than an
/// allocation request.
pub const MAX_PAYLOAD_LEN: u32 = 1_000_000;

pub const HEADER_LEN: usize = 8;

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Opcode {
    Handshake,
    Frame,
    Close,
    Ping,
    Pong,
}

impl Opcode {
    pub fn raw_value(self) -> u32 {
        match self {
            Opcode::Handshake => 0,
            Opcode::Frame => 1,
            Opcode::Close => 2,
            Opcode::Ping => 3,
            Opcode::Pong => 4,
        }
    }

    pub fn from_raw(raw: u32) -> Option<Self> {
        match raw {
            0 => Some(Opcode::Handshake),
            1 => Some(Opcode::Frame),
            2 => Some(Opcode::Close),
            3 => Some(Opcode::Ping),
            4 => Some(Opcode::Pong),
            _ => None,
        }
    }
}

#[derive(Debug, Clone, PartialEq)]
pub struct Frame {
    pub opcode: Opcode,
    pub payload: Vec<u8>,
}

impl Frame {
    /// Parsed body, or `None` when it isn't valid JSON. Malformed bodies are
    /// tolerated the way the macOS client tolerates them — a PING is echoed
    /// back regardless.
    pub fn payload_json(&self) -> Option<Value> {
        serde_json::from_slice(&self.payload).ok()
    }

    /// The `evt` field of a FRAME, e.g. `READY` or `ERROR`.
    pub fn event(&self) -> Option<String> {
        self.payload_json()?
            .get("evt")?
            .as_str()
            .map(|s| s.to_string())
    }
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum ProtocolError {
    /// Length header exceeds `MAX_PAYLOAD_LEN`; the connection must be torn down.
    PayloadTooLarge(u32),
    UnknownOpcode(u32),
}

pub fn encode_frame(opcode: Opcode, payload: &Value) -> Vec<u8> {
    let body = serde_json::to_vec(payload).expect("activity payload is always serializable");
    let mut out = Vec::with_capacity(HEADER_LEN + body.len());
    out.extend_from_slice(&opcode.raw_value().to_le_bytes());
    out.extend_from_slice(&(body.len() as u32).to_le_bytes());
    out.extend_from_slice(&body);
    out
}

pub fn handshake_payload(client_id: &str) -> Value {
    json!({ "v": 1, "client_id": client_id })
}

/// `activity` is `None` to clear the presence.
pub fn set_activity_payload(activity: Option<Value>, pid: u32, nonce: &str) -> Value {
    json!({
        "cmd": "SET_ACTIVITY",
        "args": {
            "pid": pid,
            "activity": activity.unwrap_or(Value::Null),
        },
        "nonce": nonce,
    })
}

/// Accumulates bytes off the transport and yields whole frames.
///
/// Reads are offset-based so a partially consumed buffer isn't shifted on
/// every frame; the consumed prefix is dropped once, after draining.
#[derive(Debug, Default)]
pub struct FrameDecoder {
    buf: Vec<u8>,
    offset: usize,
}

impl FrameDecoder {
    pub fn new() -> Self {
        Self::default()
    }

    pub fn push(&mut self, bytes: &[u8]) {
        self.buf.extend_from_slice(bytes);
    }

    /// Next complete frame, or `None` when more bytes are needed.
    pub fn next_frame(&mut self) -> Result<Option<Frame>, ProtocolError> {
        let available = self.buf.len() - self.offset;
        if available < HEADER_LEN {
            self.compact();
            return Ok(None);
        }
        let header = &self.buf[self.offset..self.offset + HEADER_LEN];
        let raw_opcode = u32::from_le_bytes([header[0], header[1], header[2], header[3]]);
        let len = u32::from_le_bytes([header[4], header[5], header[6], header[7]]);

        if len >= MAX_PAYLOAD_LEN {
            return Err(ProtocolError::PayloadTooLarge(len));
        }
        let len = len as usize;
        if available < HEADER_LEN + len {
            self.compact();
            return Ok(None);
        }

        let Some(opcode) = Opcode::from_raw(raw_opcode) else {
            return Err(ProtocolError::UnknownOpcode(raw_opcode));
        };

        let start = self.offset + HEADER_LEN;
        let payload = self.buf[start..start + len].to_vec();
        self.offset = start + len;
        self.compact();
        Ok(Some(Frame { opcode, payload }))
    }

    fn compact(&mut self) {
        if self.offset == 0 {
            return;
        }
        if self.offset == self.buf.len() {
            self.buf.clear();
        } else {
            self.buf.drain(..self.offset);
        }
        self.offset = 0;
    }
}
