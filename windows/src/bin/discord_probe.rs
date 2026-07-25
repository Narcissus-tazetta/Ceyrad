//! Milestone-3 spike: does the Discord IPC protocol work over a named pipe?
//!
//! Scans `\\.\pipe\discord-ipc-0..9`, handshakes, waits for READY, then pushes
//! a fixed activity so it can be eyeballed on a Discord profile. Answers PINGs
//! and clears the activity on exit, the same way the real client will.
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
    use std::thread;
    use std::time::{Duration, SystemTime};

    use ceyrad::core::activity_builder;
    use ceyrad::core::models::{CatalogInfo, MusicSourceId, PlayerState, TrackInfo};
    use ceyrad::core::settings_model::Settings;
    use ceyrad::discord::protocol::{
        encode_frame, handshake_payload, set_activity_payload, FrameDecoder, Opcode, ProtocolError,
    };
    use serde_json::Value;

    use windows::core::PCWSTR;
    use windows::Win32::Foundation::{CloseHandle, HANDLE};
    use windows::Win32::Storage::FileSystem::{
        CreateFileW, ReadFile, WriteFile, FILE_ATTRIBUTE_NORMAL, FILE_SHARE_MODE, OPEN_EXISTING,
    };

    /// How long to leave the activity up before clearing it.
    const HOLD_SECS: u64 = 60;

    const GENERIC_READ: u32 = 0x8000_0000;
    const GENERIC_WRITE: u32 = 0x4000_0000;

    struct Pipe(HANDLE);

    impl Drop for Pipe {
        fn drop(&mut self) {
            unsafe {
                let _ = CloseHandle(self.0);
            }
        }
    }

    impl Pipe {
        /// Tries `discord-ipc-0` through `-9` and keeps the first that opens.
        fn connect() -> io::Result<Self> {
            let mut last_err = None;
            for i in 0..10 {
                let path = format!(r"\\.\pipe\discord-ipc-{i}");
                let wide: Vec<u16> = path.encode_utf16().chain(std::iter::once(0)).collect();
                let handle: windows::core::Result<HANDLE> = unsafe {
                    CreateFileW(
                        PCWSTR(wide.as_ptr()),
                        GENERIC_READ | GENERIC_WRITE,
                        FILE_SHARE_MODE(0),
                        None,
                        OPEN_EXISTING,
                        FILE_ATTRIBUTE_NORMAL,
                        None,
                    )
                };
                match handle {
                    Ok(handle) if !handle.is_invalid() => {
                        println!("connected: {path}");
                        return Ok(Self(handle));
                    }
                    Ok(_) => {}
                    Err(e) => last_err = Some(e),
                }
            }
            Err(io::Error::other(format!(
                "no discord-ipc pipe found (is Discord running?); last error: {last_err:?}"
            )))
        }

        fn write_all(&self, mut buf: &[u8]) -> io::Result<()> {
            while !buf.is_empty() {
                let mut written = 0u32;
                unsafe { WriteFile(self.0, Some(buf), Some(&mut written), None) }
                    .map_err(io::Error::other)?;
                if written == 0 {
                    return Err(io::Error::other("pipe closed during write"));
                }
                buf = &buf[written as usize..];
            }
            Ok(())
        }

        fn read_some(&self, buf: &mut [u8]) -> io::Result<usize> {
            let mut read = 0u32;
            unsafe { ReadFile(self.0, Some(buf), Some(&mut read), None) }
                .map_err(io::Error::other)?;
            Ok(read as usize)
        }

        fn send(&self, opcode: Opcode, payload: &Value) -> io::Result<()> {
            self.write_all(&encode_frame(opcode, payload))
        }
    }

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
        pipe.send(Opcode::Handshake, &handshake_payload(client_id))?;

        let mut decoder = FrameDecoder::new();
        let mut buf = [0u8; 4096];
        let mut ready = false;
        let mut nonce = 0u64;

        let deadline = SystemTime::now() + Duration::from_secs(HOLD_SECS);
        while SystemTime::now() < deadline {
            let n = pipe.read_some(&mut buf)?;
            if n == 0 {
                println!("pipe closed by Discord");
                break;
            }
            decoder.push(&buf[..n]);

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
                                        "check your Discord profile — clearing in {HOLD_SECS}s"
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
            thread::sleep(Duration::from_millis(300));
        }
        Ok(())
    }
}
