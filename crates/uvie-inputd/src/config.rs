//! Config file locations and hot-reload support.

use std::path::PathBuf;
use std::time::SystemTime;

/// `~/.config/uvie` honoring XDG_CONFIG_HOME.
pub fn config_dir() -> PathBuf {
    if let Ok(dir) = std::env::var("XDG_CONFIG_HOME") {
        if !dir.is_empty() {
            return PathBuf::from(dir).join("uvie");
        }
    }
    let home = std::env::var("HOME").unwrap_or_else(|_| ".".into());
    PathBuf::from(home).join(".config").join("uvie")
}

pub fn settings_path() -> PathBuf {
    config_dir().join("settings.json")
}

pub fn macros_path() -> PathBuf {
    config_dir().join("macros.json")
}

pub fn memory_path() -> PathBuf {
    config_dir().join("memory.json")
}

/// `~/.config/autostart/uvie-inputd.desktop` honoring XDG_CONFIG_HOME.
pub fn autostart_path() -> PathBuf {
    let base = std::env::var("XDG_CONFIG_HOME")
        .ok()
        .filter(|s| !s.is_empty())
        .map(PathBuf::from)
        .unwrap_or_else(|| {
            let home = std::env::var("HOME").unwrap_or_else(|_| ".".into());
            PathBuf::from(home).join(".config")
        });
    base.join("autostart").join("uvie-inputd.desktop")
}

/// Polls file mtimes to detect config writes from the settings UI.
pub struct FileWatch {
    path: PathBuf,
    last: Option<SystemTime>,
}

impl FileWatch {
    pub fn new(path: PathBuf) -> Self {
        Self {
            last: std::fs::metadata(&path).and_then(|m| m.modified()).ok(),
            path,
        }
    }

    /// True when the file's mtime advanced since the last check.
    pub fn changed(&mut self) -> bool {
        let now = std::fs::metadata(&self.path)
            .and_then(|m| m.modified())
            .ok();
        let changed = now.is_some() && now != self.last;
        if now.is_some() {
            self.last = now;
        }
        changed
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::time::Duration;

    fn temp_path(name: &str) -> PathBuf {
        std::env::temp_dir().join(format!("uvie-config-test-{}-{name}", std::process::id()))
    }

    #[test]
    fn file_watch_unchanged_is_quiet() {
        let path = temp_path("quiet.json");
        std::fs::write(&path, "{}").unwrap();
        let mut w = FileWatch::new(path.clone());
        assert!(!w.changed());
        assert!(!w.changed());
        std::fs::remove_file(&path).ok();
    }

    #[test]
    fn file_watch_detects_newer_mtime() {
        let path = temp_path("newer.json");
        std::fs::write(&path, "{}").unwrap();
        let mut w = FileWatch::new(path.clone());
        // Rewrite and force a newer mtime explicitly - 1s filesystem mtime
        // granularity would otherwise make this racy.
        std::fs::write(&path, r#"{"x":1}"#).unwrap();
        let f = std::fs::File::options().write(true).open(&path).unwrap();
        f.set_modified(SystemTime::now() + Duration::from_secs(60))
            .unwrap();
        assert!(w.changed());
        // The new mtime is latched: a second poll is quiet.
        assert!(!w.changed());
        std::fs::remove_file(&path).ok();
    }

    #[test]
    fn file_watch_missing_file_is_quiet() {
        let path = temp_path("missing.json");
        let mut w = FileWatch::new(path.clone());
        assert!(!w.changed());
        // A file appearing later still reports a change.
        std::fs::write(&path, "{}").unwrap();
        assert!(w.changed());
        std::fs::remove_file(&path).ok();
    }

    #[test]
    fn file_watch_deleted_file_reports_no_change() {
        let path = temp_path("deleted.json");
        std::fs::write(&path, "{}").unwrap();
        let mut w = FileWatch::new(path.clone());
        std::fs::remove_file(&path).ok();
        // `now` is None -> changed() stays false and `last` is not updated.
        assert!(!w.changed());
    }
}
