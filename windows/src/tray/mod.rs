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
//!
//! The menu is built when it is asked for and destroyed again when it closes,
//! which is what macOS's `menuNeedsUpdate` does and the reason nothing about
//! the menu is resident between opens. `tray-icon` would otherwise pop a menu
//! handed to it in advance, straight from its own window procedure with no hook
//! to run first — so both of its automatic click-to-open paths are turned off
//! and the click is taken as an event instead.

pub mod dialog;
mod render;

use std::io;

use muda::MenuEvent;
use tray_icon::{Icon, MouseButtonState, TrayIcon, TrayIconBuilder, TrayIconEvent};
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
            // Both clicks open the menu — with no window to bring forward there
            // is nothing else for a left click to usefully do — but neither is
            // left to the crate, which would show whatever menu it was given
            // last. `take_menu_request` turns the click into an open instead.
            .with_menu_on_left_click(false)
            .with_menu_on_right_click(false)
            .build()
            .map_err(io::Error::other)?;
        Ok(Self { icon })
    }

    /// Builds these rows into a real menu and pops it at the icon.
    ///
    /// Does not return until the menu closes: `TrackPopupMenu` runs its own
    /// message loop. The menu stays attached afterwards because the chosen item
    /// arrives as a `WM_COMMAND` posted to the tray window — `release_menu` is
    /// what frees it, once that message has been dispatched.
    pub fn show_menu(&self, rows: &[MenuRow]) -> io::Result<()> {
        let menu = render::build(rows).map_err(io::Error::other)?;
        self.icon.set_menu(Some(render::into_context_menu(menu)));
        self.icon.show_menu();
        Ok(())
    }

    /// Drops the menu built for the last open, so the only thing this app keeps
    /// resident for its UI is the icon.
    pub fn release_menu(&self) {
        self.icon.set_menu(None);
    }
}

/// Whether the user just clicked the icon, asking for the menu.
///
/// Always drains the queue, whatever it finds: `tray-icon` posts `Enter`,
/// `Move` and `Leave` into an unbounded channel as the cursor crosses the icon,
/// and a reader that only looked when it expected a click would leave those to
/// accumulate for the life of the process.
pub fn take_menu_request() -> bool {
    let mut wanted = false;
    while let Ok(event) = TrayIconEvent::receiver().try_recv() {
        // On release, which is where Windows opens a context menu — and it also
        // means a press that turns into a drag never opens one.
        if let TrayIconEvent::Click {
            button_state: MouseButtonState::Up,
            ..
        } = event
        {
            wanted = true;
        }
    }
    wanted
}

/// Drains this thread's message queue. Returns `false` on `WM_QUIT`.
///
/// Nothing in this app posts `WM_QUIT` today — the Quit row goes through
/// `MenuAction::Quit`, and Windows *sends* `WM_QUERYENDSESSION`/`WM_ENDSESSION`
/// at logoff rather than posting anything this loop can see. The check is kept
/// as a guard in case a predefined Quit item is ever added, not as the shutdown
/// path.
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
