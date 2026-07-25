//! Framing tests for the Discord IPC protocol (no counterpart in the Swift
//! suite, which only exercised this end-to-end against a live client).

use serde_json::json;

use ceyrad::discord::protocol::{
    encode_frame, handshake_payload, set_activity_payload, Frame, FrameDecoder, Opcode,
    ProtocolError, HEADER_LEN, MAX_PAYLOAD_LEN,
};

#[test]
fn frame_round_trips() {
    let payload = json!({ "v": 1, "client_id": "1525381518258606130" });
    let bytes = encode_frame(Opcode::Handshake, &payload);

    let mut decoder = FrameDecoder::new();
    decoder.push(&bytes);
    let frame = decoder.next_frame().unwrap().expect("a whole frame");

    assert_eq!(frame.opcode, Opcode::Handshake);
    assert_eq!(frame.payload_json().unwrap(), payload);
    assert!(decoder.next_frame().unwrap().is_none());
}

#[test]
fn header_is_little_endian_opcode_then_length() {
    let bytes = encode_frame(Opcode::Frame, &json!({}));
    assert_eq!(&bytes[0..4], &1u32.to_le_bytes());
    assert_eq!(&bytes[4..8], &2u32.to_le_bytes()); // "{}"
    assert_eq!(&bytes[HEADER_LEN..], b"{}");
}

#[test]
fn partial_frame_yields_nothing_until_complete() {
    let bytes = encode_frame(Opcode::Frame, &json!({ "evt": "READY" }));
    let mut decoder = FrameDecoder::new();

    decoder.push(&bytes[..4]);
    assert!(decoder.next_frame().unwrap().is_none());

    decoder.push(&bytes[4..HEADER_LEN + 2]);
    assert!(decoder.next_frame().unwrap().is_none());

    decoder.push(&bytes[HEADER_LEN + 2..]);
    let frame = decoder.next_frame().unwrap().expect("a whole frame");
    assert_eq!(frame.event().as_deref(), Some("READY"));
}

#[test]
fn multiple_frames_drain_from_one_buffer() {
    let mut bytes = encode_frame(Opcode::Ping, &json!({ "n": 1 }));
    bytes.extend(encode_frame(Opcode::Frame, &json!({ "evt": "READY" })));

    let mut decoder = FrameDecoder::new();
    decoder.push(&bytes);

    let first = decoder.next_frame().unwrap().unwrap();
    assert_eq!(first.opcode, Opcode::Ping);
    let second = decoder.next_frame().unwrap().unwrap();
    assert_eq!(second.opcode, Opcode::Frame);
    assert!(decoder.next_frame().unwrap().is_none());
}

#[test]
fn oversized_length_is_a_protocol_violation() {
    let mut bytes = Vec::new();
    bytes.extend_from_slice(&Opcode::Frame.raw_value().to_le_bytes());
    bytes.extend_from_slice(&MAX_PAYLOAD_LEN.to_le_bytes());

    let mut decoder = FrameDecoder::new();
    decoder.push(&bytes);
    assert_eq!(
        decoder.next_frame(),
        Err(ProtocolError::PayloadTooLarge(MAX_PAYLOAD_LEN))
    );
}

#[test]
fn unknown_opcode_is_rejected() {
    let mut bytes = Vec::new();
    bytes.extend_from_slice(&99u32.to_le_bytes());
    bytes.extend_from_slice(&2u32.to_le_bytes());
    bytes.extend_from_slice(b"{}");

    let mut decoder = FrameDecoder::new();
    decoder.push(&bytes);
    assert_eq!(decoder.next_frame(), Err(ProtocolError::UnknownOpcode(99)));
}

#[test]
fn malformed_json_body_is_tolerated() {
    // A PING must still be echoed back even if its body doesn't parse.
    let frame = Frame {
        opcode: Opcode::Ping,
        payload: b"not json".to_vec(),
    };
    assert!(frame.payload_json().is_none());
    assert!(frame.event().is_none());
}

#[test]
fn handshake_payload_shape() {
    let payload = handshake_payload("1525381518258606130");
    assert_eq!(payload["v"], 1);
    assert_eq!(payload["client_id"], "1525381518258606130");
}

#[test]
fn set_activity_payload_shape() {
    let activity = json!({ "type": 2, "details": "Song" });
    let payload = set_activity_payload(Some(activity.clone()), 4321, "nonce-1");
    assert_eq!(payload["cmd"], "SET_ACTIVITY");
    assert_eq!(payload["args"]["pid"], 4321);
    assert_eq!(payload["args"]["activity"], activity);
    assert_eq!(payload["nonce"], "nonce-1");
}

#[test]
fn set_activity_payload_clears_with_null() {
    let payload = set_activity_payload(None, 4321, "nonce-2");
    assert!(payload["args"]["activity"].is_null());
}
