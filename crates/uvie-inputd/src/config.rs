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
