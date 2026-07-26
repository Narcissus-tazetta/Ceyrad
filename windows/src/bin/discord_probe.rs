//! Milestone-3 spike: does the Discord IPC protocol work over a named pipe?
//!
//! Scans `\\.\pipe\discord-ipc-0..9`, handshakes, waits for READY, then pushes
//! a fixed activity so it can be eyeballed on a Discord profile. Answers PINGs
//! and clears the activity on exit, the same way the real client does.
//!
//! Usage: `cargo run --bin discord_probe [apple-music|spotify]`

#[cfg(not(windows))]
fn main() {
    eprintln!("discord_probe only runs on Windows.");
    std::process::exit(1);
}

#[cfg(windows)]
fn main() -> std::io::Result<()> {
    imp::run()
}

#[cfg(windows)]
mod imp {
    use std::io;
    use std::time::{Duration, Instant, SystemTime};

    use ceyrad::core::activity_builder;
    use ceyrad::core::models::{CatalogInfo, MusicSourceId, PlayerState, TrackInfo};
    use ceyrad::core::settings_model::Settings;
    use ceyrad::discord::pipe::{Pipe, Wakeup};
    use ceyrad::discord::protocol::handshake_payload;
    use ceyrad::discord::protocol::{set_activity_payload, FrameDecoder, Opcode, ProtocolError};
    use serde_json::Value;

    /// How long to leave the activity up before clearing it.
    const HOLD: Duration = Duration::from_secs(60);

    fn sample_activity(source: MusicSourceId) -> Value {
        let mut track = TrackInfo::new("Ceyrad Probe", "Test Artist", "Test Album");
        track.duration_sec = Some(240.0);
        track.position_sec = Some(30.0);

        let catalog = CatalogInfo {
            song_url: Some("https://example.com/song".into()),
            artwork_url: None,
            ..Default::default()
        };

        let activity = activity_builder::build(
            &track,
            PlayerState::Playing,
            Some(&catalog),
            &Settings::default(),
            source,
            SystemTime::now(),
        );
        Value::Object(activity)
    }

    pub fn run() -> io::Result<()> {
        let source = match std::env::args().nth(1).as_deref() {
            Some("spotify") => MusicSourceId::Spotify,
            _ => MusicSourceId::AppleMusic,
        };
        let client_id = source.discord_client_id();
        println!("source: {} / client_id: {client_id}", source.display_name());

        let pipe = Pipe::connect()?;
        println!("connected");
        pipe.send(Opcode::Handshake, &handshake_payload(client_id))?;

        let mut decoder = FrameDecoder::new();
        let mut buf = [0u8; 4096];
        let mut ready = false;
        let mut nonce = 0u64;

        let deadline = Instant::now() + HOLD;
        loop {
            let remaining = deadline.saturating_duration_since(Instant::now());
            if remaining.is_zero() {
                break;
            }

            // The deadline is honoured even when Discord sends nothing at all,
            // which is the normal state of an idle connection.
            match pipe.read_or_signal(&mut buf, None, Some(remaining))? {
                Wakeup::Timeout => break,
                Wakeup::Closed => {
                    println!("pipe closed by Discord");
                    return Ok(());
                }
                Wakeup::Signaled | Wakeup::Message => continue,
                Wakeup::Data(n) => decoder.push(&buf[..n]),
            }

            loop {
                match decoder.next_frame() {
                    Ok(None) => break,
                    Ok(Some(frame)) => {
                        let json = frame.payload_json().unwrap_or(Value::Null);
                        println!("<- {:?} {json}", frame.opcode);
                        match frame.opcode {
                            Opcode::Frame => {
                                if !ready && frame.event().as_deref() == Some("READY") {
                                    ready = true;
                                    nonce += 1;
                                    println!("-> SET_ACTIVITY");
                                    pipe.send(
                                        Opcode::Frame,
                                        &set_activity_payload(
                                            Some(sample_activity(source)),
                                            std::process::id(),
                                            &format!("probe-{nonce}"),
                                        ),
                                    )?;
                                    println!(
                                        "check your Discord profile — clearing in {}s",
                                        HOLD.as_secs()
                                    );
                                }
                            }
                            // Discord expects the same payload echoed back.
                            Opcode::Ping => pipe.send(Opcode::Pong, &json)?,
                            Opcode::Close => {
                                println!("Discord sent CLOSE: {json}");
                                return Ok(());
                            }
                            _ => {}
                        }
                    }
                    Err(ProtocolError::PayloadTooLarge(len)) => {
                        return Err(io::Error::other(format!("frame too large: {len}")));
                    }
                    Err(ProtocolError::UnknownOpcode(op)) => {
                        return Err(io::Error::other(format!("unknown opcode: {op}")));
                    }
                }
            }
        }

        if ready {
            nonce += 1;
            println!("-> SET_ACTIVITY null (clear)");
            pipe.send(
                Opcode::Frame,
                &set_activity_payload(None, std::process::id(), &format!("probe-{nonce}")),
            )?;
            // The write is only queued; give Discord a moment to drain it
            // before the handle closes.
            std::thread::sleep(Duration::from_millis(300));
        }
        Ok(())
    }
}
