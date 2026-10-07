//! uvie-inputd — UVie Vietnamese input daemon for Linux.
//!
//! Grabs physical keyboards via evdev/EVIOCGRAB, feeds keystrokes through
//! the `uvie` engine, and replays transformed input through a uinput
//! virtual keyboard. Works identically on X11 and every Wayland compositor
//! (including GNOME) because it lives at the kernel input layer.
//!
//! Usage: `uvie-inputd` (no flags today — config lives in
//! `~/.config/uvie/settings.json`, edited by `uvie-ui`).

mod autostart;
mod config;
mod daemon;
mod devices;
mod focus;
mod inject;
mod keymap;
mod keys_linux;
mod tray;
mod vdev;
mod xsel;

use std::sync::mpsc;
use std::sync::{Arc, Mutex};

use daemon::{Daemon, Msg};
use tray::TrayShared;

fn main() {
    let (tx, rx) = mpsc::channel::<Msg>();
    let tray_shared = Arc::new(Mutex::new(TrayShared::default()));

    let daemon = match Daemon::new(tx.clone(), tray_shared.clone()) {
        Ok(d) => d,
        Err(e) => {
            eprintln!(
                "[uvie-inputd] init failed: {e}\n\
                 Need read access to /dev/input/event* (group `input`) and write \
                 access to /dev/uinput — see packaging/99-uvie.rules."
            );
            std::process::exit(1);
        }
    };

    tray::spawn_tray(tx, tray_shared);
    daemon.run(rx);
}
