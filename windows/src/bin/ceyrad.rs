//! Ceyrad for Windows.
//!
//! Lives in the notification area with no window and no console, the way the
//! macOS build lives in the menu bar. `ceyrad_dev` is the same orchestrator
//! with its reasoning printed instead, for when something needs watching.

// No console: a tray app that flashes up a black window on launch, and keeps
// it in the taskbar, is not one anybody wants running at login.
#![cfg_attr(windows, windows_subsystem = "windows")]

#[cfg(not(windows))]
fn main() {
    eprintln!("Ceyrad only runs on Windows.");
    std::process::exit(1);
}

#[cfg(windows)]
fn main() -> std::io::Result<()> {
    use windows::Win32::System::Com::{CoInitializeEx, COINIT_MULTITHREADED};
    use windows::Win32::UI::HiDpi::{
        SetProcessDpiAwarenessContext, DPI_AWARENESS_CONTEXT_PER_MONITOR_AWARE_V2,
    };

    use ceyrad::app::App;
    use ceyrad::tray::Tray;

    // Before any window exists. Without it the process is laid out at 96 DPI
    // and bitmap-stretched by the shell — visibly blurry dialog text at 150%,
    // and the 32px tray glyph downscaled instead of used as drawn.
    unsafe {
        let _ = SetProcessDpiAwarenessContext(DPI_AWARENESS_CONTEXT_PER_MONITOR_AWARE_V2);
    }

    // MTA, not STA, even with a window in play. The tray icon and its menu are
    // plain USER objects that need no apartment at all, whereas the media
    // session watcher is built by blocking on a WinRT async call — which is
    // the classic way to deadlock an STA. Keeping the process multithreaded
    // also leaves SMTC free to deliver its events on pool threads, where all
    // they do is set a flag and signal the loop.
    unsafe {
        let _ = CoInitializeEx(None, COINIT_MULTITHREADED);
    }

    let mut app = App::new()?;
    app.attach_tray(Tray::new()?);
    app.run()
}
