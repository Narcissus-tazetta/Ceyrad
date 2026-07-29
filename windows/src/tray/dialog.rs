//! The two modal dialogs: ask for a line of text, and report a bad answer.
//!
//! The macOS counterparts are four lines of `NSAlert` with a text field
//! accessory. Windows has no built-in "prompt for a string", so the dialog is
//! described as an in-memory `DLGTEMPLATE` and handed to the system. The
//! alternative — a `.rc` script — would mean adding a resource compiler to the
//! build for three trivial dialogs, which is a worse trade than the byte
//! layout below.
//!
//! Everything here is modal and runs on the event loop's own thread, which is
//! what makes the thread-local handoff safe: there can only ever be one
//! prompt in flight, because the one thread that could open a second is
//! blocked inside the first.

use std::cell::RefCell;

use crate::app::log;
use windows::core::HSTRING;
use windows::Win32::Foundation::{HWND, LPARAM, WPARAM};
use windows::Win32::UI::Input::KeyboardAndMouse::SetFocus;
use windows::Win32::UI::WindowsAndMessaging::{
    DialogBoxIndirectParamW, EndDialog, GetDlgItem, GetDlgItemTextW, MessageBoxW, SendMessageW,
    SetDlgItemTextW, SetForegroundWindow, DLGTEMPLATE, MB_ICONWARNING, MB_OK, MB_SETFOREGROUND,
    WM_CLOSE, WM_COMMAND, WM_INITDIALOG,
};

/// Selects a range in an edit control. Spelled out rather than pulling in the
/// whole common-controls binding for one constant.
const EM_SETSEL: u32 = 0x00B1;

/// Caps what an edit control will accept, in characters.
const EM_SETLIMITTEXT: u32 = 0x00C5;

/// Longest answer accepted. Comfortably past Discord's 512-character url
/// ceiling, so the limit that matters is the one that reports a reason.
const MAX_INPUT_CHARS: usize = 1024;

const ID_OK: i32 = 1;
const ID_CANCEL: i32 = 2;
const ID_EDIT: i32 = 100;

thread_local! {
    /// The prompt currently on screen. `None` whenever one is not.
    static PROMPT: RefCell<Option<Prompt>> = const { RefCell::new(None) };
}

struct Prompt {
    initial: String,
    answer: Option<String>,
}

/// Asks for one line of text, pre-filled and selected. `None` if cancelled.
pub fn prompt(title: &str, message: &str, initial: &str) -> Option<String> {
    PROMPT.with(|slot| {
        *slot.borrow_mut() = Some(Prompt {
            initial: initial.to_string(),
            answer: None,
        });
    });

    let template = on_dword_boundary(build_template(title, message));
    let accepted = unsafe {
        DialogBoxIndirectParamW(
            None,
            template.as_ptr() as *const DLGTEMPLATE,
            None,
            Some(dialog_proc),
            LPARAM(0),
        )
    };

    // -1 is an outright failure and 0 an invalid parent; both would otherwise
    // be indistinguishable from Cancel, leaving a menu item that silently does
    // nothing forever with no clue as to why.
    if accepted <= 0 {
        log(&format!(
            "dialog: could not open ({})",
            std::io::Error::last_os_error()
        ));
    }

    PROMPT.with(|slot| {
        let prompt = slot.borrow_mut().take();
        if accepted == ID_OK as isize {
            prompt.and_then(|prompt| prompt.answer)
        } else {
            None
        }
    })
}

/// Re-homes the template in a buffer the system can actually read.
///
/// A `DLGTEMPLATE` must *start* on a DWORD boundary — the same rule
/// `align_to_dword` already enforces for every item *inside* the buffer.
/// `Vec<u16>` only promises two-byte alignment; that it happens to come back 8-
/// or 16-byte aligned today is the allocator's habit, not a guarantee, and the
/// failure mode is a dialog that silently never appears.
fn on_dword_boundary(template: Vec<u16>) -> Vec<u32> {
    let mut aligned = vec![0u32; template.len().div_ceil(2)];
    unsafe {
        std::ptr::copy_nonoverlapping(
            template.as_ptr(),
            aligned.as_mut_ptr().cast::<u16>(),
            template.len(),
        );
    }
    aligned
}

pub fn show_error(title: &str, message: &str) {
    unsafe {
        MessageBoxW(
            None,
            &HSTRING::from(message),
            &HSTRING::from(title),
            MB_OK | MB_ICONWARNING | MB_SETFOREGROUND,
        );
    }
}

unsafe extern "system" fn dialog_proc(
    dialog: HWND,
    message: u32,
    wparam: WPARAM,
    _lparam: LPARAM,
) -> isize {
    match message {
        WM_INITDIALOG => {
            let initial = PROMPT.with(|slot| slot.borrow().as_ref().map(|p| p.initial.clone()));
            if let Some(initial) = initial {
                let _ = unsafe { SetDlgItemTextW(dialog, ID_EDIT, &HSTRING::from(initial)) };
            }
            if let Ok(edit) = unsafe { GetDlgItem(Some(dialog), ID_EDIT) } {
                // Focus has to be placed explicitly, because returning FALSE
                // below tells the dialog manager not to place it — and it is
                // the box the user came here to type in.
                let _ = unsafe { SetFocus(Some(edit)) };
                // Selected, so typing replaces it. That matters most on a
                // re-prompt, where the text handed back is the one that was
                // just rejected.
                unsafe { SendMessageW(edit, EM_SETSEL, Some(WPARAM(0)), Some(LPARAM(-1))) };
                // Refuse the keystrokes rather than truncating silently in
                // `read_edit`: a cut there would also be free to land between
                // the halves of a surrogate pair and turn it into U+FFFD.
                unsafe {
                    SendMessageW(
                        edit,
                        EM_SETLIMITTEXT,
                        Some(WPARAM(MAX_INPUT_CHARS - 1)),
                        Some(LPARAM(0)),
                    )
                };
            }
            // The tray has no window to inherit activation from, so the dialog
            // has to claim the foreground itself or it opens behind whatever
            // the user was doing.
            let _ = unsafe { SetForegroundWindow(dialog) };
            // FALSE: the focus set above stands, rather than being moved to
            // the first tab stop.
            0
        }
        WM_COMMAND => match (wparam.0 & 0xFFFF) as i32 {
            ID_OK => {
                let answer = unsafe { read_edit(dialog) };
                PROMPT.with(|slot| {
                    if let Some(prompt) = slot.borrow_mut().as_mut() {
                        prompt.answer = Some(answer);
                    }
                });
                let _ = unsafe { EndDialog(dialog, ID_OK as isize) };
                1
            }
            ID_CANCEL => {
                let _ = unsafe { EndDialog(dialog, ID_CANCEL as isize) };
                1
            }
            _ => 0,
        },
        WM_CLOSE => {
            let _ = unsafe { EndDialog(dialog, ID_CANCEL as isize) };
            1
        }
        _ => 0,
    }
}

unsafe fn read_edit(dialog: HWND) -> String {
    let mut buffer = [0u16; MAX_INPUT_CHARS];
    let len = unsafe { GetDlgItemTextW(dialog, ID_EDIT, &mut buffer) } as usize;
    String::from_utf16_lossy(&buffer[..len]).trim().to_string()
}

// MARK: - The template
//
// A dialog template is a packed struct followed by its items, each aligned to
// a DWORD. Sizes are in dialog units, which the system scales by the font
// below, so the layout stays right at any DPI without measuring anything.

const DS_SETFONT: u32 = 0x40;
const DS_MODALFRAME: u32 = 0x80;
const DS_CENTER: u32 = 0x0800;
const WS_POPUP: u32 = 0x8000_0000;
const WS_CAPTION: u32 = 0x00C0_0000;
const WS_SYSMENU: u32 = 0x0008_0000;
const WS_CHILD: u32 = 0x4000_0000;
const WS_VISIBLE: u32 = 0x1000_0000;
const WS_TABSTOP: u32 = 0x0001_0000;
const WS_BORDER: u32 = 0x0080_0000;
const ES_AUTOHSCROLL: u32 = 0x0080;
const BS_DEFPUSHBUTTON: u32 = 0x0001;

/// The window-class atoms the dialog manager understands without registering
/// anything.
const ATOM_BUTTON: u16 = 0x0080;
const ATOM_EDIT: u16 = 0x0081;
const ATOM_STATIC: u16 = 0x0082;

/// Stops a static control reading `&` as a mnemonic prefix. The current strings
/// contain none, but the label is a translated sentence and one could appear.
const SS_NOPREFIX: u32 = 0x0080;

const DIALOG_WIDTH: i16 = 260;
const MARGIN: i16 = 8;

fn build_template(title: &str, message: &str) -> Vec<u16> {
    let mut out: Vec<u16> = Vec::new();

    push_u32(
        &mut out,
        DS_SETFONT | DS_MODALFRAME | DS_CENTER | WS_POPUP | WS_CAPTION | WS_SYSMENU,
    );
    push_u32(&mut out, 0); // no extended style
    out.push(4); // item count
    push_i16(&mut out, 0); // x, y: DS_CENTER places it
    push_i16(&mut out, 0);
    push_i16(&mut out, DIALOG_WIDTH);
    push_i16(&mut out, 86);
    out.push(0); // no menu
    out.push(0); // default dialog class
    push_str(&mut out, title);
    // DS_SETFONT: point size then typeface. "MS Shell Dlg" is the alias the
    // system maps to whatever the current UI font actually is.
    out.push(9);
    push_str(&mut out, "MS Shell Dlg");

    let body_width = DIALOG_WIDTH - MARGIN * 2;
    push_item(
        &mut out,
        WS_CHILD | WS_VISIBLE | SS_NOPREFIX,
        MARGIN,
        MARGIN,
        body_width,
        26,
        u16::MAX, // no id: it is never read back
        ATOM_STATIC,
        message,
    );
    push_item(
        &mut out,
        WS_CHILD | WS_VISIBLE | WS_BORDER | WS_TABSTOP | ES_AUTOHSCROLL,
        MARGIN,
        38,
        body_width,
        14,
        ID_EDIT as u16,
        ATOM_EDIT,
        "",
    );
    push_item(
        &mut out,
        WS_CHILD | WS_VISIBLE | WS_TABSTOP | BS_DEFPUSHBUTTON,
        DIALOG_WIDTH - MARGIN - 108,
        62,
        50,
        14,
        ID_OK as u16,
        ATOM_BUTTON,
        "OK",
    );
    push_item(
        &mut out,
        WS_CHILD | WS_VISIBLE | WS_TABSTOP,
        DIALOG_WIDTH - MARGIN - 50,
        62,
        50,
        14,
        ID_CANCEL as u16,
        ATOM_BUTTON,
        "Cancel",
    );
    out
}

#[allow(clippy::too_many_arguments)]
fn push_item(
    out: &mut Vec<u16>,
    style: u32,
    x: i16,
    y: i16,
    width: i16,
    height: i16,
    id: u16,
    class_atom: u16,
    text: &str,
) {
    align_to_dword(out);
    push_u32(out, style);
    push_u32(out, 0); // no extended style
    push_i16(out, x);
    push_i16(out, y);
    push_i16(out, width);
    push_i16(out, height);
    out.push(id);
    out.push(0xFFFF); // "an atom follows", rather than a class name
    out.push(class_atom);
    push_str(out, text);
    out.push(0); // no creation data
}

fn push_u32(out: &mut Vec<u16>, value: u32) {
    out.push(value as u16);
    out.push((value >> 16) as u16);
}

fn push_i16(out: &mut Vec<u16>, value: i16) {
    out.push(value as u16);
}

fn push_str(out: &mut Vec<u16>, text: &str) {
    out.extend(text.encode_utf16());
    out.push(0);
}

/// Each item must start on a DWORD boundary, and the buffer is counted in
/// u16s, so an odd length needs one word of padding.
fn align_to_dword(out: &mut Vec<u16>) {
    if out.len() % 2 != 0 {
        out.push(0);
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    /// Words before the title: style, extended style, item count, four
    /// coordinates, the menu ordinal and the class ordinal.
    const HEADER_WORDS: usize = 11;
    /// Where the item count sits within that header.
    const ITEM_COUNT_INDEX: usize = 4;
    /// Words per item before its class and text: style, extended style, four
    /// coordinates, the id — then the 0xFFFF marker and the atom.
    const ITEM_WORDS: usize = 9 + 2;

    const TYPEFACE: &str = "MS Shell Dlg";
    /// The static, the edit, and the two buttons, in the order they are added.
    const ITEM_TEXTS: [&str; 4] = ["A message", "", "OK", "Cancel"];

    /// The rule the dialog manager will not forgive: each item begins on a
    /// DWORD boundary, which in a buffer counted in words means an even index.
    /// Getting this wrong yields a dialog that silently fails to appear, so it
    /// is worth walking the bytes the way the system will.
    #[test]
    fn every_item_starts_on_a_dword_boundary() {
        let template = build_template("Title", ITEM_TEXTS[0]);

        let mut cursor = HEADER_WORDS;
        cursor += "Title".encode_utf16().count() + 1;
        cursor += 1; // font point size
        cursor += TYPEFACE.encode_utf16().count() + 1;

        for text in ITEM_TEXTS {
            if cursor % 2 != 0 {
                cursor += 1; // the padding word the builder inserts
            }
            assert_eq!(cursor % 2, 0, "item at word {cursor} is misaligned");
            cursor += ITEM_WORDS;
            cursor += text.encode_utf16().count() + 1;
            cursor += 1; // no creation data
        }
        assert_eq!(cursor, template.len(), "template length disagrees");
    }

    #[test]
    fn the_header_declares_the_items_that_follow() {
        let template = build_template("T", "M");
        assert_eq!(
            template[ITEM_COUNT_INDEX] as usize,
            ITEM_TEXTS.len(),
            "the count must match what the builder actually appends"
        );
    }

    #[test]
    fn text_is_stored_as_null_terminated_utf16() {
        // Non-ASCII on purpose: the Japanese menu passes titles like this one
        // straight through, and a byte-oriented mistake would only show there.
        let template = build_template("Ceyradの設定", "M");
        let title: Vec<u16> = "Ceyradの設定".encode_utf16().collect();
        let end = HEADER_WORDS + title.len();
        assert_eq!(&template[HEADER_WORDS..end], title.as_slice());
        assert_eq!(template[end], 0, "the title must be terminated");
    }

    #[test]
    fn the_ok_button_is_the_default() {
        // Enter has to submit: it is the only reason the prompt is bearable.
        let template = build_template("T", "M");
        assert!(
            template.windows(2).any(|pair| {
                let style = pair[0] as u32 | ((pair[1] as u32) << 16);
                style & BS_DEFPUSHBUTTON != 0 && style & WS_TABSTOP != 0
            }),
            "no default push button in the template"
        );
    }
}
