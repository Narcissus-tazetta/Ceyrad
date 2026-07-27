//! The notification-area icon and its menu.
//!
//! `tray-icon` owns a hidden window for the icon and answers `TaskbarCreated`
//! on it, so an Explorer restart re-registers the icon instead of losing it.
//! That window's messages land on whichever thread built it — this one — which
//! is why the event loop pumps messages every pass (see `pump_messages`).
//!
//! Opening the menu runs a nested message loop inside `TrackPopupMenu` for as
//! long as it is on screen, so SMTC and Discord events are noticed late rather
//! than promptly while the user has it open. Nothing is lost: the events that
//! drive this app are kernel handles and flags that stay set until read. macOS
//! has the same property while a menu or a modal alert is up.

pub mod dialog;
mod render;

use std::io;

use muda::MenuEvent;
use tray_icon::{Icon, TrayIcon, TrayIconBuilder};
use windows::Win32::UI::WindowsAndMessaging::{
    DispatchMessageW, PeekMessageW, TranslateMessage, MSG, PM_REMOVE, WM_QUIT,
};

use crate::core::menu_model::{parse_action_id, MenuAction, MenuRow};
use crate::core::tray_icon_glyph::glyph_rgba;

/// Drawn once at a size the shell can scale down cleanly for any DPI; the
/// notification area asks for 16px at 100% and 32px at 200%.
const ICON_SIZE: u32 = 32;

pub struct Tray {
    icon: TrayIcon,
}

impl Tray {
    pub fn new() -> io::Result<Self> {
        let glyph = Icon::from_rgba(glyph_rgba(ICON_SIZE), ICON_SIZE, ICON_SIZE)
            .map_err(io::Error::other)?;
        let icon = TrayIconBuilder::new()
            .with_icon(glyph)
            .with_tooltip("Ceyrad")
            // Left click opens the menu too: with no window to bring forward,
            // there is nothing else for it to usefully do.
            .with_menu_on_left_click(true)
            .build()
            .map_err(io::Error::other)?;
        Ok(Self { icon })
    }

    /// Replaces the menu wholesale. Called when the state behind it changes,
    /// which is the Windows stand-in for macOS rebuilding on every open.
    pub fn set_menu(&self, rows: &[MenuRow]) -> io::Result<()> {
        let menu = render::build(rows).map_err(io::Error::other)?;
        self.icon.set_menu(Some(render::into_context_menu(menu)));
        Ok(())
    }
}

/// Drains this thread's message queue. Returns `false` once the session is
/// ending, which is the one message the caller has to act on rather than
/// forward.
///
/// Must run on every pass: the loop waits with `QS_ALLINPUT`, so a message
/// left in the queue would wake it again immediately and spin.
pub fn pump_messages() -> bool {
    let mut message = MSG::default();
    while unsafe { PeekMessageW(&mut message, None, 0, 0, PM_REMOVE) }.as_bool() {
        if message.message == WM_QUIT {
            return false;
        }
        unsafe {
            let _ = TranslateMessage(&message);
            DispatchMessageW(&message);
        }
    }
    true
}

/// The next menu click, if one is waiting.
///
/// Clicks arrive as ordinary window messages, so this is only ever called
/// after `pump_messages` has dispatched them. Rows carrying an id we did not
/// mint — the status lines — are skipped rather than treated as an action.
pub fn try_recv_action() -> Option<MenuAction> {
    loop {
        let event = MenuEvent::receiver().try_recv().ok()?;
        if let Some(action) = parse_action_id(event.id().as_ref()) {
            return Some(action);
        }
    }
}
