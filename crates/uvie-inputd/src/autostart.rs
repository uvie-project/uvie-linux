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
    std::fs::write(path, desktop)
}
