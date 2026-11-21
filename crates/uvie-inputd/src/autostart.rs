//! XDG autostart integration (`~/.config/autostart/uvie-inputd.desktop`).

use std::io;
use std::path::Path;

/// Write or remove the autostart entry to mirror `launch_at_login`.
pub fn sync(enabled: bool, path: &Path) -> io::Result<()> {
    if !enabled {
        if path.exists() {
            std::fs::remove_file(path)?;
        }
        return Ok(());
    }
    if let Some(dir) = path.parent() {
        std::fs::create_dir_all(dir)?;
    }
    let exe = std::env::current_exe()
        .map(|p| p.display().to_string())
        .unwrap_or_else(|_| "uvie-inputd".into());
    let desktop = format!(
        "[Desktop Entry]\n\
         Type=Application\n\
         Name=UVie Vietnamese Input\n\
         Comment=Telex/VNI input method daemon\n\
         Exec={exe}\n\
         Icon=uvie\n\
         Terminal=false\n\
         X-GNOME-Autostart-enabled=true\n"
    );
    // This runs every housekeeping tick — skip the write when nothing
    // changed rather than touching the file twice a second forever.
    match std::fs::read_to_string(path) {
        Ok(existing) if existing == desktop => Ok(()),
        _ => std::fs::write(path, desktop),
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn temp_path(name: &str) -> std::path::PathBuf {
        std::env::temp_dir()
            .join(format!("uvie-autostart-test-{}", std::process::id()))
            .join(name)
    }

    #[test]
    fn sync_true_creates_desktop_file_with_exec() {
        let path = temp_path("on/uvie-inputd.desktop");
        sync(true, &path).unwrap();
        let text = std::fs::read_to_string(&path).unwrap();
        assert!(text.starts_with("[Desktop Entry]"));
        assert!(text.contains("Type=Application"));
        assert!(text.contains("Name=UVie Vietnamese Input"));
        let exec = text.lines().find(|l| l.starts_with("Exec=")).unwrap();
        assert!(!exec["Exec=".len()..].is_empty());
        std::fs::remove_dir_all(path.parent().unwrap()).ok();
    }

    #[test]
    fn sync_true_is_idempotent() {
        let path = temp_path("idem/uvie-inputd.desktop");
        sync(true, &path).unwrap();
        let first = std::fs::read_to_string(&path).unwrap();
        sync(true, &path).unwrap();
        assert_eq!(std::fs::read_to_string(&path).unwrap(), first);
        std::fs::remove_dir_all(path.parent().unwrap()).ok();
    }

    #[test]
    fn sync_false_removes_file_and_is_idempotent() {
        let path = temp_path("off/uvie-inputd.desktop");
        sync(true, &path).unwrap();
        assert!(path.exists());
        sync(false, &path).unwrap();
        assert!(!path.exists());
        // Removing a missing file is a no-op, not an error.
        sync(false, &path).unwrap();
        std::fs::remove_dir_all(path.parent().unwrap()).ok();
    }

    #[test]
    fn sync_true_rewrites_stale_content() {
        let path = temp_path("stale/uvie-inputd.desktop");
        sync(true, &path).unwrap();
        std::fs::write(&path, "stale").unwrap();
        sync(true, &path).unwrap();
        assert!(std::fs::read_to_string(&path).unwrap().contains("Exec="));
        std::fs::remove_dir_all(path.parent().unwrap()).ok();
    }
}
