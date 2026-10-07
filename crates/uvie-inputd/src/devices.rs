//! Keyboard discovery, grabbing, and reader threads.
//!
//! We enumerate `/dev/input/event*`, keep the devices that look like real
//! keyboards (EV_KEY with the full letter range), and `EVIOCGRAB` them so
//! the compositor only ever sees our virtual keyboard. Each grabbed device
//! gets a blocking reader thread that forwards raw events into the daemon's
//! event channel; a periodic rescan picks up hot-plugged keyboards and
//! drops disconnected ones.

use std::collections::HashMap;
use std::path::PathBuf;
use std::sync::mpsc::Sender;

use evdev::{Device, InputEvent, Key};

use crate::daemon::Msg;
use crate::inject::VIRTUAL_DEVICE_NAME;

/// One raw event from a grabbed physical device.
pub struct DeviceEvent {
    /// Stable id of the source device for this daemon run.
    pub dev: usize,
    pub event: InputEvent,
}

/// Messages the device layer feeds into the daemon loop.
pub enum DevMsg {
    Event(DeviceEvent),
    /// The device disconnected (or its reader hit a fatal error).
    Gone(usize),
}

/// Live set of grabbed keyboards: id -> path (the `Device` itself lives in
/// its reader thread; dropping the pool entry just untracks it).
pub struct DevicePool {
    paths: HashMap<usize, PathBuf>,
    next_id: usize,
    tx: Sender<Msg>,
}

/// Does this device look like a keyboard we should grab? Requires the
/// alphanumeric range plus typical keyboard keys — skips mice, knobs,
/// power buttons, and remotes that expose a couple of KEY_ codes.
fn is_keyboard(device: &Device) -> bool {
    let Some(keys) = device.supported_keys() else {
        return false;
    };
    let letters = (16u16..=25u16) // KEY_Q..KEY_P
        .all(|c| keys.contains(Key::new(c)))
        && (30u16..=38u16).all(|c| keys.contains(Key::new(c))) // KEY_A..KEY_L
        && (44u16..=50u16).all(|c| keys.contains(Key::new(c))); // KEY_Z..KEY_M
    letters
        && keys.contains(Key::new(crate::keys_linux::KEY_ENTER))
        && keys.contains(Key::new(crate::keys_linux::KEY_BACKSPACE))
        && keys.contains(Key::new(crate::keys_linux::KEY_LEFTSHIFT))
}

impl DevicePool {
    pub fn new(tx: Sender<Msg>) -> Self {
        Self {
            paths: HashMap::new(),
            next_id: 0,
            tx,
        }
    }

    /// Scan /dev/input, grab newly-seen keyboards. Returns the number of
    /// keyboards currently held.
    pub fn rescan(&mut self) -> usize {
        for (path, mut device) in evdev::enumerate() {
            if self.paths.values().any(|p| *p == path) {
                continue;
            }
            if device.name() == Some(VIRTUAL_DEVICE_NAME) {
                continue;
            }
            if !is_keyboard(&device) {
                continue;
            }
            if device.grab().is_err() {
                // No grab permission (input group / udev rule missing) —
                // leave the device alone rather than double-processing it.
                continue;
            }
            let name = device_name(&device).to_string();
            let id = self.next_id;
            self.next_id += 1;
            let tx = self.tx.clone();
            if std::thread::Builder::new()
                .name(format!("uvie-input-{}", path.display()))
                .spawn(move || reader_loop(id, device, tx))
                .is_ok()
            {
                eprintln!("[uvie-inputd] grabbed {} ({})", path.display(), name);
                self.paths.insert(id, path);
            }
        }
        self.paths.len()
    }

    /// Drop bookkeeping for a dead device (grab releases on fd close).
    pub fn remove(&mut self, id: usize) {
        if let Some(p) = self.paths.remove(&id) {
            eprintln!("[uvie-inputd] keyboard gone: {}", p.display());
        }
    }
}

fn device_name(device: &Device) -> &str {
    device.name().unwrap_or("?")
}

/// Read events until the device goes away. Runs one thread per keyboard.
fn reader_loop(id: usize, mut device: Device, tx: Sender<Msg>) {
    loop {
        match device.fetch_events() {
            Ok(events) => {
                for event in events {
                    if tx
                        .send(Msg::Dev(DevMsg::Event(DeviceEvent { dev: id, event })))
                        .is_err()
                    {
                        return; // daemon shutting down
                    }
                }
            }
            Err(_) => {
                let _ = tx.send(Msg::Dev(DevMsg::Gone(id)));
                return;
            }
        }
    }
}
