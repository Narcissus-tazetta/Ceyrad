//! Ceyrad for Windows — headless dev build.
//!
//! The real app will live in the tray with no console. This build is the same
//! orchestrator with its decisions printed to stdout, so the behaviour can be
//! watched and argued with before any UI exists.
//!
//! Usage: `cargo run --bin ceyrad_dev`

#[cfg(not(windows))]
fn main() {
    eprintln!("ceyrad_dev only runs on Windows.");
    std::process::exit(1);
}

#[cfg(windows)]
fn main() -> std::io::Result<()> {
    use windows::Win32::System::Com::{CoInitializeEx, COINIT_MULTITHREADED};

    // WinRT needs an initialised apartment. MTA is right for a console build:
    // SMTC delivers its events on pool threads and nothing here owns a window.
    // The tray build will need STA plus a message loop instead.
    unsafe {
        let _ = CoInitializeEx(None, COINIT_MULTITHREADED);
    }

    ceyrad::app::App::new()?.run()
}
