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
    let bytes = encode_frame(Opcode::Handshake, &payload).expect("encodes");

    let mut decoder = FrameDecoder::new();
    decoder.push(&bytes);
    let frame = decoder.next_frame().unwrap().expect("a whole frame");

    assert_eq!(frame.opcode, Opcode::Handshake);
    assert_eq!(frame.payload_json().unwrap(), payload);
    assert!(decoder.next_frame().unwrap().is_none());
}

#[test]
fn header_is_little_endian_opcode_then_length() {
    let bytes = encode_frame(Opcode::Frame, &json!({})).expect("encodes");
    assert_eq!(&bytes[0..4], &1u32.to_le_bytes());
    assert_eq!(&bytes[4..8], &2u32.to_le_bytes()); // "{}"
    assert_eq!(&bytes[HEADER_LEN..], b"{}");
}

#[test]
fn partial_frame_yields_nothing_until_complete() {
    let bytes = encode_frame(Opcode::Frame, &json!({ "evt": "READY" })).expect("encodes");
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
    let mut bytes = encode_frame(Opcode::Ping, &json!({ "n": 1 })).expect("encodes");
    bytes.extend(encode_frame(Opcode::Frame, &json!({ "evt": "READY" })).expect("encodes"));

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

// MARK: - Diagnostics
//
// An ERROR frame or a CLOSE is the only place Discord explains an invalid
// client id, a refused activity or a rate limit — and with no tray UI, the log
// line built from it is the only place a user would ever see the reason.

fn frame_with(payload: serde_json::Value) -> Frame {
    Frame {
        opcode: Opcode::Frame,
        payload: serde_json::to_vec(&payload).expect("serialize"),
    }
}

#[test]
fn an_error_frame_reports_message_and_code() {
    let frame = frame_with(json!({
        "evt": "ERROR",
        "data": { "code": 4000, "message": "Invalid Client ID" },
    }));
    assert_eq!(frame.event().as_deref(), Some("ERROR"));
    assert_eq!(frame.error_message(), "Invalid Client ID (code 4000)");
}

#[test]
fn a_close_reports_its_top_level_reason() {
    let frame = Frame {
        opcode: Opcode::Close,
        payload: serde_json::to_vec(&json!({ "code": 4001, "message": "Invalid Origin" }))
            .expect("serialize"),
    };
    assert_eq!(frame.error_message(), "Invalid Origin (code 4001)");
}

#[test]
fn a_reason_with_only_one_half_still_reads() {
    assert_eq!(
        frame_with(json!({ "data": { "message": "Rate limited" } })).error_message(),
        "Rate limited"
    );
    assert_eq!(
        frame_with(json!({ "data": { "code": 4000 } })).error_message(),
        "code 4000"
    );
}

#[test]
fn an_unhelpful_body_falls_back_to_what_arrived() {
    // Never empty: something the user can paste is better than silence.
    let frame = Frame {
        opcode: Opcode::Close,
        payload: b"not json at all".to_vec(),
    };
    assert_eq!(frame.error_message(), "not json at all");

    let frame = frame_with(json!({ "evt": "ERROR" }));
    assert!(frame.error_message().contains("ERROR"));
}

#[test]
fn a_zero_length_payload_is_a_valid_frame() {
    // Legal on the wire, and it exercises the branch where the decoder
    // consumes the whole buffer and clears rather than drains.
    let mut bytes = Vec::new();
    bytes.extend_from_slice(&Opcode::Pong.raw_value().to_le_bytes());
    bytes.extend_from_slice(&0u32.to_le_bytes());

    let mut decoder = FrameDecoder::new();
    decoder.push(&bytes);
    let frame = decoder.next_frame().unwrap().expect("a frame");
    assert_eq!(frame.opcode, Opcode::Pong);
    assert!(frame.payload.is_empty());
    assert!(decoder.next_frame().unwrap().is_none());
}

#[test]
fn a_header_split_at_any_offset_still_reassembles() {
    // The 4-byte split is already covered; a transport is free to break it
    // anywhere, and every offset takes the `available < HEADER_LEN` path
    // through `compact` with a different amount already buffered.
    let bytes = encode_frame(Opcode::Frame, &json!({ "evt": "READY" })).expect("encodes");
    for split in 1..HEADER_LEN {
        let mut decoder = FrameDecoder::new();
        decoder.push(&bytes[..split]);
        assert!(decoder.next_frame().unwrap().is_none(), "split at {split}");
        decoder.push(&bytes[split..]);
        let frame = decoder.next_frame().unwrap().expect("a frame");
        assert_eq!(frame.opcode, Opcode::Frame, "split at {split}");
    }
}

#[test]
fn the_size_limit_is_exclusive_and_does_not_preallocate() {
    // One below the cap is accepted as a length, and the decoder waits for the
    // body rather than reserving a megabyte on the strength of a header.
    let mut bytes = Vec::new();
    bytes.extend_from_slice(&Opcode::Frame.raw_value().to_le_bytes());
    bytes.extend_from_slice(&(MAX_PAYLOAD_LEN - 1).to_le_bytes());

    let mut decoder = FrameDecoder::new();
    decoder.push(&bytes);
    assert_eq!(decoder.next_frame(), Ok(None));
}

#[test]
fn a_stream_of_frames_does_not_grow_the_buffer_without_bound() {
    // Pushed in chunks that do not line up with frame boundaries, which is the
    // realistic shape of pipe reads, to prove `compact` actually reclaims.
    let one = encode_frame(Opcode::Ping, &json!({ "n": 1 })).expect("encodes");
    let mut stream = Vec::new();
    for _ in 0..200 {
        stream.extend_from_slice(&one);
    }

    let mut decoder = FrameDecoder::new();
    let mut drained = 0;
    for chunk in stream.chunks(7) {
        decoder.push(chunk);
        while decoder.next_frame().unwrap().is_some() {
            drained += 1;
        }
    }
    assert_eq!(drained, 200);
}

#[test]
fn a_data_field_that_is_not_an_object_falls_back_rather_than_failing() {
    let frame = Frame {
        opcode: Opcode::Close,
        payload: br#"{"data": 5}"#.to_vec(),
    };
    assert!(frame.error_message().contains('5'));
}
