//! Starting with Windows, as macOS's "Launch at Login" does.
//!
//! macOS has `SMAppService.mainApp`, which registers the `.app` bundle itself.
//! The Windows equivalent with the same properties — per user, no elevation, no
//! installer — is a value under the `Run` key. The Startup folder would work
//! too but means writing a `.lnk` through COM for the same effect, and Task
//! Scheduler is built for admin-managed, condition-rich, possibly elevated
//! tasks: both are more machinery than this needs.
//!
//! The state is read from the registry every time rather than cached, matching
//! macOS reading `.status` live: whatever else changed it — the user, another
//! install, a cleanup tool — the menu should still be telling the truth.

use std::io;
use std::path::Path;

use windows::core::{HSTRING, PCWSTR};
use windows::Win32::Foundation::ERROR_FILE_NOT_FOUND;
use windows::Win32::System::Registry::{
    RegCloseKey, RegDeleteValueW, RegGetValueW, RegSetKeyValueW, HKEY, HKEY_CURRENT_USER,
    KEY_SET_VALUE, REG_SZ, RRF_RT_REG_SZ,
};

const RUN_KEY: &str = r"Software\Microsoft\Windows\CurrentVersion\Run";
/// Also the name shown in Task Manager's Startup tab, so it is the app's name
/// rather than anything more technical.
const VALUE_NAME: &str = "Ceyrad";

/// Whether Windows will start this app at logon.
///
/// Only asks whether an entry exists, not whether it points here — see
/// `reconcile_path` for why those are separate questions.
pub fn is_enabled() -> bool {
    stored_command(VALUE_NAME).is_some()
}

pub fn set_enabled(enabled: bool) -> io::Result<()> {
    if enabled {
        let command = command_for(&std::env::current_exe()?);
        write_command(VALUE_NAME, &command)
    } else {
        clear_command(VALUE_NAME)
    }
}

/// Points an existing entry at wherever the exe is now.
///
/// This ships as a single portable exe with no installer, so nothing else keeps
/// the registry in step when the user moves it — and a `Run` entry naming a path
/// that no longer exists fails silently at logon, which is the worst way for a
/// feature to stop working. Called once at startup: if the app is running from
/// somewhere new and was set to auto-start, it re-registers from here.
pub fn reconcile_path() -> io::Result<()> {
    let Some(stored) = stored_command(VALUE_NAME) else {
        // Not enabled; nothing to keep in step.
        return Ok(());
    };
    let wanted = command_for(&std::env::current_exe()?);
    if needs_rewrite(&stored, &wanted) {
        write_command(VALUE_NAME, &wanted)?;
    }
    Ok(())
}

/// The exact string to store, quoted so a path containing spaces survives the
/// shell's argument splitting — `C:\Program Files\…` otherwise reads as a
/// command called `C:\Program` with arguments.
fn command_for(exe: &Path) -> String {
    format!("\"{}\"", exe.display())
}

/// Whether the stored command has drifted from what it should be.
///
/// Compared case-insensitively and ignoring quoting, because Windows paths are
/// case-insensitive and an entry written by an older build (or by hand) may be
/// unquoted while meaning the same file. Rewriting an entry that already works
/// is harmless but pointless; the comparison exists to keep it quiet.
fn needs_rewrite(stored: &str, wanted: &str) -> bool {
    !unquote(stored).eq_ignore_ascii_case(unquote(wanted))
}

fn unquote(command: &str) -> &str {
    let trimmed = command.trim();
    trimmed
        .strip_prefix('"')
        .and_then(|rest| rest.strip_suffix('"'))
        .unwrap_or(trimmed)
        .trim()
}

/// The stored command, or `None` when there is no entry (or it is unreadable,
/// which for this purpose is the same thing).
///
/// The value name is a parameter so the tests can round-trip against a scratch
/// entry instead of the one that governs the user's actual startup — the same
/// seam `settings_store` uses to test its file handling for real.
fn stored_command(value_name: &str) -> Option<String> {
    let subkey = HSTRING::from(RUN_KEY);
    let name = HSTRING::from(value_name);

    // Ask for the size first: the value is a path, and there is no sensible
    // fixed buffer for one.
    let mut size: u32 = 0;
    let status = unsafe {
        RegGetValueW(
            HKEY_CURRENT_USER,
            PCWSTR(subkey.as_ptr()),
            PCWSTR(name.as_ptr()),
            RRF_RT_REG_SZ,
            None,
            None,
            Some(&mut size),
        )
    };
    if status.is_err() || size == 0 {
        return None;
    }

    let mut buffer = vec![0u16; (size as usize).div_ceil(2)];
    let mut size = (buffer.len() * 2) as u32;
    let status = unsafe {
        RegGetValueW(
            HKEY_CURRENT_USER,
            PCWSTR(subkey.as_ptr()),
            PCWSTR(name.as_ptr()),
            RRF_RT_REG_SZ,
            None,
            Some(buffer.as_mut_ptr().cast()),
            Some(&mut size),
        )
    };
    if status.is_err() {
        return None;
    }

    let chars = (size as usize / 2).min(buffer.len());
    let value = String::from_utf16_lossy(&buffer[..chars]);
    let value = value.trim_end_matches('\0').to_string();
    (!value.is_empty()).then_some(value)
}

fn write_command(value_name: &str, command: &str) -> io::Result<()> {
    let subkey = HSTRING::from(RUN_KEY);
    let name = HSTRING::from(value_name);
    let value = HSTRING::from(command);
    // Includes the terminator: `RegSetKeyValueW` takes bytes, and a REG_SZ
    // without one is technically malformed even though most readers cope.
    let bytes = ((value.len() + 1) * 2) as u32;

    let status = unsafe {
        RegSetKeyValueW(
            HKEY_CURRENT_USER,
            PCWSTR(subkey.as_ptr()),
            PCWSTR(name.as_ptr()),
            REG_SZ.0,
            Some(value.as_ptr().cast()),
            bytes,
        )
    };
    status.ok().map_err(io::Error::other)
}

fn clear_command(value_name: &str) -> io::Result<()> {
    let subkey = HSTRING::from(RUN_KEY);
    let name = HSTRING::from(value_name);

    let mut key = HKEY::default();
    let status = unsafe {
        windows::Win32::System::Registry::RegOpenKeyExW(
            HKEY_CURRENT_USER,
            PCWSTR(subkey.as_ptr()),
            None,
            KEY_SET_VALUE,
            &mut key,
        )
    };
    if status.is_err() {
        // No key at all means nothing to switch off.
        return Ok(());
    }

    let status = unsafe { RegDeleteValueW(key, PCWSTR(name.as_ptr())) };
    unsafe {
        let _ = RegCloseKey(key);
    }
    // Already absent is the state being asked for, not a failure.
    if status == ERROR_FILE_NOT_FOUND {
        return Ok(());
    }
    status.ok().map_err(io::Error::other)
}

#[cfg(test)]
mod tests {
    use super::*;

    /// A value name of this test's own, under the same key the real entry lives
    /// in. Touching `Ceyrad` itself would change whether the machine running the
    /// tests starts the app at logon, which no test is entitled to do.
    fn scratch_name(label: &str) -> String {
        format!("CeyradTest-{label}-{}", std::process::id())
    }

    /// Removes the scratch entry however the test ended.
    struct Scratch(String);

    impl Drop for Scratch {
        fn drop(&mut self) {
            let _ = clear_command(&self.0);
        }
    }

    #[test]
    fn a_written_command_reads_back() {
        let name = scratch_name("roundtrip");
        let _cleanup = Scratch(name.clone());

        assert_eq!(stored_command(&name), None, "started out clean");
        write_command(&name, "\"C:\\apps\\ceyrad.exe\"").expect("write");
        assert_eq!(
            stored_command(&name).as_deref(),
            Some("\"C:\\apps\\ceyrad.exe\"")
        );
    }

    #[test]
    fn writing_twice_replaces_rather_than_appends() {
        let name = scratch_name("overwrite");
        let _cleanup = Scratch(name.clone());

        write_command(&name, "\"C:\\old\\ceyrad.exe\"").expect("first write");
        write_command(&name, "\"C:\\new\\ceyrad.exe\"").expect("second write");
        assert_eq!(
            stored_command(&name).as_deref(),
            Some("\"C:\\new\\ceyrad.exe\"")
        );
    }

    #[test]
    fn clearing_removes_the_entry() {
        let name = scratch_name("clear");
        let _cleanup = Scratch(name.clone());

        write_command(&name, "\"C:\\apps\\ceyrad.exe\"").expect("write");
        clear_command(&name).expect("clear");
        assert_eq!(stored_command(&name), None);
    }

    #[test]
    fn clearing_something_absent_is_not_a_failure() {
        // Switching off what is already off is the state being asked for.
        let name = scratch_name("absent");
        clear_command(&name).expect("clearing an absent value");
        clear_command(&name).expect("and again");
    }

    #[test]
    fn a_long_path_survives_the_size_negotiation() {
        // The read asks for the size first and then allocates; a path longer
        // than any fixed buffer is what that is for.
        let name = scratch_name("long");
        let _cleanup = Scratch(name.clone());

        let long = format!("\"C:\\{}\\ceyrad.exe\"", "nested".repeat(60));
        write_command(&name, &long).expect("write");
        assert_eq!(stored_command(&name).as_deref(), Some(long.as_str()));
    }

    #[test]
    fn a_path_with_spaces_and_non_ascii_survives() {
        let name = scratch_name("unicode");
        let _cleanup = Scratch(name.clone());

        let path = "\"C:\\Users\\clown\\デスクトップ\\Program Files\\ceyrad.exe\"";
        write_command(&name, path).expect("write");
        assert_eq!(stored_command(&name).as_deref(), Some(path));
    }

    #[test]
    fn a_command_is_quoted_so_a_path_with_spaces_survives() {
        let command = command_for(Path::new(r"C:\Program Files\Ceyrad\ceyrad.exe"));
        assert_eq!(command, "\"C:\\Program Files\\Ceyrad\\ceyrad.exe\"");
    }

    #[test]
    fn an_unchanged_path_is_not_rewritten() {
        let wanted = command_for(Path::new(r"C:\apps\ceyrad.exe"));
        assert!(!needs_rewrite(&wanted, &wanted));
    }

    #[test]
    fn a_moved_exe_is_rewritten() {
        let stored = command_for(Path::new(r"C:\old\ceyrad.exe"));
        let wanted = command_for(Path::new(r"C:\new\ceyrad.exe"));
        assert!(needs_rewrite(&stored, &wanted));
    }

    #[test]
    fn case_alone_is_not_drift() {
        // Windows paths are case-insensitive, so rewriting over this would
        // churn the registry on every launch for no reason.
        assert!(!needs_rewrite(
            "\"C:\\Apps\\Ceyrad.exe\"",
            "\"c:\\apps\\ceyrad.exe\""
        ));
    }

    #[test]
    fn quoting_alone_is_not_drift() {
        // An entry written by hand, or by a build before quoting was added,
        // still names the same file.
        assert!(!needs_rewrite(
            r"C:\apps\ceyrad.exe",
            "\"C:\\apps\\ceyrad.exe\""
        ));
    }

    #[test]
    fn surrounding_whitespace_is_not_drift() {
        assert!(!needs_rewrite("  \"C:\\a\\b.exe\"  ", "\"C:\\a\\b.exe\""));
    }
}
