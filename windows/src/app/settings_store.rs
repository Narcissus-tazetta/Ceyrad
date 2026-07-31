//! Settings persistence.
//!
//! macOS keeps these in `UserDefaults`; on Windows the equivalent that survives
//! an uninstall-and-reinstall and is trivially inspectable is a JSON file under
//! `%APPDATA%`. `Settings` already round-trips through serde with defaults for
//! missing keys, so a partial or hand-edited file still loads.

use std::ffi::OsStr;
use std::fs::File;
use std::io;
use std::io::Write;
use std::path::{Path, PathBuf};

use crate::core::aumid::AumidOverrides;
use crate::core::settings_model::Settings;

const APP_DIR: &str = "Ceyrad";
const FILE_NAME: &str = "settings.json";

/// Comma-separated AUMIDs, for installs whose ids we do not recognise yet.
const ENV_APPLE_MUSIC_AUMID: &str = "CEYRAD_AUMID_APPLE_MUSIC";

pub fn settings_path() -> Option<PathBuf> {
    let appdata = std::env::var_os("APPDATA")?;
    Some(PathBuf::from(appdata).join(APP_DIR).join(FILE_NAME))
}

/// Missing or unreadable settings fall back to defaults rather than failing —
/// a corrupt file should not stop the presence from working.
pub fn load() -> (Settings, Option<String>) {
    let Some(path) = settings_path() else {
        return (Settings::default(), Some("APPDATA is not set".into()));
    };
    load_from(&path)
}

pub fn save(settings: &Settings) -> io::Result<PathBuf> {
    let path = settings_path()
        .ok_or_else(|| io::Error::other("APPDATA is not set, nowhere to save settings"))?;
    save_to(&path, settings)?;
    Ok(path)
}

/// The half of `load` that does not depend on the environment.
pub fn load_from(path: &Path) -> (Settings, Option<String>) {
    match std::fs::read_to_string(path) {
        Ok(text) => match serde_json::from_str(&text) {
            Ok(settings) => (settings, None),
            Err(e) => (
                Settings::default(),
                Some(format!(
                    "{} is not valid settings ({e}); using defaults",
                    path.display()
                )),
            ),
        },
        Err(e) if e.kind() == io::ErrorKind::NotFound => (Settings::default(), None),
        Err(e) => (
            Settings::default(),
            Some(format!(
                "could not read {} ({e}); using defaults",
                path.display()
            )),
        ),
    }
}

/// Writes via a sibling temporary file and a rename, so an interrupted save
/// leaves the previous settings intact rather than a half-written file that
/// `load_from` would reject on the next launch.
pub fn save_to(path: &Path, settings: &Settings) -> io::Result<()> {
    if let Some(parent) = path.parent() {
        std::fs::create_dir_all(parent)?;
    }
    let text = serde_json::to_string_pretty(settings).map_err(io::Error::other)?;

    let temp = temp_sibling(path);
    {
        // `fs::write` does not reach the disk, so a power loss just after the
        // rename can leave a zero-length settings.json on NTFS. The flush is
        // what makes the rename a promotion of durable bytes.
        let mut file = File::create(&temp)?;
        file.write_all(text.as_bytes())?;
        file.sync_all()?;
    }
    // `std::fs::rename` is `MoveFileEx` with `MOVEFILE_REPLACE_EXISTING`, so it
    // does clobber, atomically — there is no need to unlink the target first,
    // and doing so would be actively harmful: the realistic reason a rename
    // fails on Windows is a scanner or a sync client holding the file open, and
    // the retry would hit the same sharing violation having already deleted the
    // user's settings.
    if let Err(e) = std::fs::rename(&temp, path) {
        let _ = std::fs::remove_file(&temp);
        return Err(e);
    }
    Ok(())
}

fn temp_sibling(path: &Path) -> PathBuf {
    let mut name = path
        .file_name()
        .unwrap_or_else(|| OsStr::new(FILE_NAME))
        .to_os_string();
    name.push(".tmp");
    match path.parent() {
        Some(parent) => parent.join(name),
        None => PathBuf::from(name),
    }
}

pub fn aumid_overrides_from_env() -> AumidOverrides {
    AumidOverrides {
        apple_music: split_env(ENV_APPLE_MUSIC_AUMID),
    }
}

fn split_env(key: &str) -> Vec<String> {
    std::env::var(key)
        .unwrap_or_default()
        .split(',')
        .map(|s| s.trim().to_string())
        .filter(|s| !s.is_empty())
        .collect()
}

#[cfg(test)]
mod tests {
    use std::sync::atomic::{AtomicU32, Ordering};

    use super::*;
    use crate::core::settings_model::LinkType;

    /// A directory of this test's own, since these touch the real filesystem.
    fn scratch(label: &str) -> PathBuf {
        static COUNTER: AtomicU32 = AtomicU32::new(0);
        let unique = COUNTER.fetch_add(1, Ordering::SeqCst);
        let dir = std::env::temp_dir().join(format!(
            "ceyrad-settings-{label}-{}-{unique}",
            std::process::id()
        ));
        let _ = std::fs::remove_dir_all(&dir);
        dir
    }

    #[test]
    fn a_saved_file_loads_back_unchanged() {
        let path = scratch("roundtrip").join(FILE_NAME);
        let mut settings = Settings::default();
        settings.set_button1_type(LinkType::Album);
        settings.set_button2_label("My Label");
        settings.pause_hide_minutes = 10;

        save_to(&path, &settings).expect("save");
        let (loaded, warning) = load_from(&path);

        assert!(warning.is_none(), "{warning:?}");
        assert_eq!(loaded.button1_type, LinkType::Album);
        assert_eq!(loaded.button2_label(None), "My Label");
        assert_eq!(loaded.pause_hide_minutes, 10);

        let _ = std::fs::remove_dir_all(path.parent().unwrap());
    }

    #[test]
    fn saving_creates_the_directory_and_leaves_no_temp_behind() {
        let dir = scratch("mkdir");
        let path = dir.join(FILE_NAME);
        save_to(&path, &Settings::default()).expect("save");

        assert!(path.exists());
        assert!(!temp_sibling(&path).exists(), "temp file was not renamed");

        let _ = std::fs::remove_dir_all(&dir);
    }

    #[test]
    fn saving_twice_replaces_the_previous_file() {
        // The interesting half on Windows, where rename does not clobber.
        let dir = scratch("overwrite");
        let path = dir.join(FILE_NAME);

        save_to(&path, &Settings::default()).expect("first save");
        let mut settings = Settings::default();
        settings.pause_hide_minutes = 1;
        save_to(&path, &settings).expect("second save");

        assert_eq!(load_from(&path).0.pause_hide_minutes, 1);
        assert!(!temp_sibling(&path).exists());

        let _ = std::fs::remove_dir_all(&dir);
    }

    #[test]
    fn a_missing_file_is_not_a_problem() {
        let path = scratch("missing").join(FILE_NAME);
        let (settings, warning) = load_from(&path);
        assert!(warning.is_none());
        assert_eq!(settings.pause_hide_minutes, 5);
    }

    #[test]
    fn a_corrupt_file_falls_back_to_defaults_and_says_so() {
        let dir = scratch("corrupt");
        std::fs::create_dir_all(&dir).expect("mkdir");
        let path = dir.join(FILE_NAME);
        std::fs::write(&path, "{ this is not json").expect("write");

        let (settings, warning) = load_from(&path);
        assert_eq!(settings.button1_type, LinkType::Song);
        assert!(warning.is_some(), "a silent reset would be worse");

        let _ = std::fs::remove_dir_all(&dir);
    }

    #[test]
    fn a_partial_file_keeps_its_keys_and_defaults_the_rest() {
        let dir = scratch("partial");
        std::fs::create_dir_all(&dir).expect("mkdir");
        let path = dir.join(FILE_NAME);
        std::fs::write(&path, r#"{"pause_hide_minutes": 1}"#).expect("write");

        let (settings, warning) = load_from(&path);
        assert!(warning.is_none());
        assert_eq!(settings.pause_hide_minutes, 1);
        assert_eq!(settings.button2_type, LinkType::Repository);

        let _ = std::fs::remove_dir_all(&dir);
    }
}
