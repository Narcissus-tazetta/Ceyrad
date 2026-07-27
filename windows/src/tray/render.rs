//! Turning menu rows into a real menu.
//!
//! Deliberately the whole of the platform-specific menu code: everything about
//! *what* the menu says lives in `core::menu_model`, so this only has to walk a
//! tree and append.

use muda::{CheckMenuItem, ContextMenu, IsMenuItem, Menu, MenuItem, PredefinedMenuItem, Submenu};

use crate::core::menu_model::{action_id, MenuRow};

pub fn build(rows: &[MenuRow]) -> muda::Result<Menu> {
    let menu = Menu::new();
    append(&menu, rows)?;
    Ok(menu)
}

/// `Menu` and `Submenu` do not share a trait covering `append`, so the one
/// generic step is passing the appender in.
fn append(into: &dyn Appendable, rows: &[MenuRow]) -> muda::Result<()> {
    for row in rows {
        match row {
            // Left with the id muda generates rather than one of ours, so a
            // status line or a heading can never be mistaken for an action.
            MenuRow::Info(text) => into.push(&MenuItem::new(text, false, None))?,
            MenuRow::Separator => into.push(&PredefinedMenuItem::separator())?,
            MenuRow::Item { label, action } => {
                into.push(&MenuItem::with_id(action_id(*action), label, true, None))?
            }
            MenuRow::Choice {
                label,
                action,
                checked,
            } => into.push(&CheckMenuItem::with_id(
                action_id(*action),
                label,
                true,
                *checked,
                None,
            ))?,
            MenuRow::Submenu { label, rows } => {
                let submenu = Submenu::new(label, true);
                append(&submenu, rows)?;
                into.push(&submenu)?;
            }
        }
    }
    Ok(())
}

trait Appendable {
    fn push(&self, item: &dyn IsMenuItem) -> muda::Result<()>;
}

impl Appendable for Menu {
    fn push(&self, item: &dyn IsMenuItem) -> muda::Result<()> {
        self.append(item)
    }
}

impl Appendable for Submenu {
    fn push(&self, item: &dyn IsMenuItem) -> muda::Result<()> {
        self.append(item)
    }
}

/// So the caller can hand the finished menu to the tray icon without naming
/// muda's trait itself.
pub fn into_context_menu(menu: Menu) -> Box<dyn ContextMenu> {
    Box::new(menu)
}
