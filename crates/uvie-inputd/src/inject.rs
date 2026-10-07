//! Text injection through a uinput virtual keyboard.
//!
//! uinput emits key *codes*, not characters, and the active keymap belongs
//! to the compositor — accented Vietnamese output can't be typed directly.
//! Layered strategy (`InjectionMode`):
//!
//!   * ASCII fast path — look up the char in the keymap's reverse map
//!     (level-0 or Shift level-1) and emit the real key code.
//!   * `unicode_hex` — Ctrl+Shift+U hex sequence (GTK/IBus-aware fields).
//!   * `clipboard` — write the string to the clipboard, emit Ctrl+V, then
//!     restore the previous clipboard contents (espanso-style fallback).

use std::collections::HashMap;
use std::io;
use std::sync::{Arc, Mutex};
use std::time::Duration;

use evdev::InputEvent;

use crate::vdev::Vdev;

use uvie_core::settings::{InjectionMode, RewriteMode};

use crate::keymap::{CharKey, Keymap};
use crate::keys_linux::*;

/// Name of the virtual input device — also used to recognize (and never
/// grab) our own device when scanning /dev/input.
pub const VIRTUAL_DEVICE_NAME: &str = "uvie virtual keyboard";

const CLIPBOARD_RESTORE_DELAY: Duration = Duration::from_millis(400);
/// Minimum spacing between clipboard pastes — the target app reads the
/// selection asynchronously, so back-to-back set_text+Ctrl+V can serve a
/// stale or next-in-line string (observed as "đơợc" at ~30ms/keystroke).
const PASTE_SETTLE: Duration = Duration::from_millis(60);

/// Clipboard state shared with the delayed-restore thread. The original
/// user clipboard is captured ONCE at the start of a paste burst; restores
/// are serialized by `gen` so only the last paste of a burst restores.
#[derive(Default)]
struct PasteState {
    /// Clipboard contents before the current burst (`None` = not in burst).
    saved: Option<String>,
    /// Bump on every paste; a restore fires only if gen is unchanged after
    /// the delay.
    gen: u64,
}

pub struct Injector {
    vdev: Vdev,
    char_map: HashMap<char, CharKey>,
    mode: InjectionMode,
    rewrite: RewriteMode,
    /// Focused app is Chromium-based — force SelectOverwrite so synthetic
    /// backspaces actually land (omnibox drops/reorders them otherwise).
    chromium_app: bool,
    clipboard: Option<arboard::Clipboard>,
    paste_state: Arc<Mutex<PasteState>>,
}

impl Injector {
    /// Create the virtual keyboard and build the char->key reverse map.
    pub fn new(keymap: &Keymap, mode: InjectionMode, rewrite: RewriteMode) -> io::Result<Self> {
        let vdev = Vdev::create(VIRTUAL_DEVICE_NAME)?;
        Ok(Self {
            vdev,
            char_map: keymap.char_map(),
            mode,
            rewrite,
            chromium_app: false,
            clipboard: arboard::Clipboard::new().ok(),
            paste_state: Arc::new(Mutex::new(PasteState::default())),
        })
    }

    /// Rebuild after settings/layout changes.
    pub fn configure(&mut self, keymap: &Keymap, mode: InjectionMode, rewrite: RewriteMode) {
        self.char_map = keymap.char_map();
        self.mode = mode;
        self.rewrite = rewrite;
    }

    /// Forward one physical event verbatim (pass-through key, SYN, MSC...).
    pub fn forward(&mut self, ev: &InputEvent) {
        let _ = self.vdev.emit(ev.event_type().0, ev.code(), ev.value());
    }

    /// Forward a raw (type, code, value) triple.
    pub fn emit_raw(&mut self, type_: u16, code: u16, value: i32) {
        let _ = self.vdev.emit(type_, code, value);
    }

    /// Emit one action from a dispatch plan.
    pub fn act(&mut self, action: &uvie_core::InputAction) {
        match action {
            uvie_core::InputAction::Backspace(n) => self.erase(*n),
            uvie_core::InputAction::ForwardDelete(n) => self.forward_delete(*n),
            uvie_core::InputAction::Text(text) => self.type_str(text),
        }
    }

    /// Emit every action in a plan, in order.
    pub fn apply_plan(&mut self, plan: &[uvie_core::InputAction]) {
        for action in plan {
            self.act(action);
        }
    }

    /// Press or release a raw key code on the virtual device.
    pub fn key(&mut self, code: u16, down: bool) {
        let _ = self.vdev.emit_key(code, down);
    }

    /// Full press+release of a key code.
    pub fn tap(&mut self, code: u16) {
        self.key(code, true);
        self.key(code, false);
    }

    /// Focused window is (or stopped being) a Chromium-based app.
    pub fn set_chromium_app(&mut self, is_chromium: bool) {
        self.chromium_app = is_chromium;
    }

    fn effective_rewrite(&self) -> RewriteMode {
        match self.rewrite {
            RewriteMode::SelectOverwrite => RewriteMode::SelectOverwrite,
            RewriteMode::Backspace if self.chromium_app => RewriteMode::SelectOverwrite,
            RewriteMode::Backspace => RewriteMode::Backspace,
        }
    }

    /// Erase `n` characters left of the caret.
    pub fn erase(&mut self, n: usize) {
        match self.effective_rewrite() {
            RewriteMode::Backspace => {
                for _ in 0..n {
                    self.tap(KEY_BACKSPACE);
                }
            }
            RewriteMode::SelectOverwrite => {
                // Shift+Left selects the chars; the following typed text
                // overwrites the selection — workaround for fields that
                // drop synthetic backspaces (Chromium omnibox).
                if n == 0 {
                    return;
                }
                self.key(KEY_LEFTSHIFT, true);
                for _ in 0..n {
                    self.tap(KEY_LEFT);
                }
                self.key(KEY_LEFTSHIFT, false);
            }
        }
    }

    /// Delete `n` characters right of the caret (mid-word edits).
    pub fn forward_delete(&mut self, n: usize) {
        for _ in 0..n {
            self.tap(KEY_DELETE);
        }
    }

    /// Type a whole string (one diff suffix).
    pub fn type_str(&mut self, text: &str) {
        if matches!(self.effective_mode(), InjectionMode::Clipboard) && !text.is_ascii() {
            self.paste_str(text);
            return;
        }
        for c in text.chars() {
            self.type_char(c);
        }
    }

    /// Resolve `Auto` to a concrete strategy: Ctrl+Shift+U hex only works
    /// in IBus-aware text fields; everywhere else it types literal garbage
    /// ("chaof" -> "che0<LF>o"), so fall back to clipboard pasting.
    fn effective_mode(&self) -> InjectionMode {
        if self.mode != InjectionMode::Auto {
            return self.mode;
        }
        let ibus = std::env::var_os("IBUS_ADDRESS").is_some()
            || ["GTK_IM_MODULE", "QT_IM_MODULE", "XMODIFIERS"]
                .iter()
                .filter_map(|k| std::env::var(k).ok())
                .any(|v| v.contains("ibus"));
        if ibus {
            InjectionMode::UnicodeHex
        } else {
            InjectionMode::Clipboard
        }
    }

    fn type_char(&mut self, c: char) {
        if let Some(ck) = self.char_map.get(&c).copied() {
            if ck.shift {
                self.key(KEY_LEFTSHIFT, true);
            }
            self.tap(ck.code);
            if ck.shift {
                self.key(KEY_LEFTSHIFT, false);
            }
        } else {
            self.unicode_char(c);
        }
    }

    /// Ctrl+Shift+U + hex + Enter — the IBus/GTK unicode input sequence.
    fn unicode_seq(&mut self, c: char) {
        self.key(KEY_LEFTCTRL, true);
        self.key(KEY_LEFTSHIFT, true);
        self.tap(22); // KEY_U
        self.key(KEY_LEFTSHIFT, false);
        self.key(KEY_LEFTCTRL, false);
        for d in format!("{:x}", c as u32).chars() {
            if let Some(ck) = self.char_map.get(&d) {
                self.tap(ck.code);
            }
        }
        self.tap(KEY_ENTER);
    }

    fn unicode_char(&mut self, c: char) {
        match self.effective_mode() {
            InjectionMode::Clipboard => self.paste_str(&c.to_string()),
            _ => self.unicode_seq(c),
        }
    }

    /// Paste `text` via the clipboard, then restore previous contents.
    ///
    /// A multi-keystroke burst stays in "paste mode": the user's original
    /// clipboard is captured once, and the restore is scheduled only after
    /// the last paste in the burst — otherwise a restore firing mid-burst
    /// serves the stale text to the next Ctrl+V.
    fn paste_str(&mut self, text: &str) {
        let Some(clip) = self.clipboard.as_mut() else {
            // No clipboard available (headless) — fall back to hex input.
            for c in text.chars() {
                self.unicode_seq(c);
            }
            return;
        };
        let gen = {
            let mut st = self.paste_state.lock().unwrap();
            if st.saved.is_none() {
                st.saved = clip.get_text().ok();
            }
            st.gen += 1;
            st.gen
        };
        if clip.set_text(text.to_string()).is_err() {
            {
                let mut st = self.paste_state.lock().unwrap();
                if st.gen == gen {
                    st.saved = None;
                }
            }
            for c in text.chars() {
                self.unicode_seq(c);
            }
            return;
        }
        self.key(KEY_LEFTCTRL, true);
        self.tap(47); // KEY_V
        self.key(KEY_LEFTCTRL, false);
        let state = Arc::clone(&self.paste_state);
        std::thread::spawn(move || {
            std::thread::sleep(CLIPBOARD_RESTORE_DELAY);
            let prev = {
                let mut st = state.lock().unwrap();
                if st.gen != gen {
                    return; // a newer paste superseded us
                }
                st.saved.take()
            };
            if let Some(prev) = prev {
                if let Ok(mut clip) = arboard::Clipboard::new() {
                    let _ = clip.set_text(prev);
                }
            }
        });
        // Give the app a window to actually read the selection before we
        // risk overwriting it with the next keystroke's set_text.
        std::thread::sleep(PASTE_SETTLE);
    }
}
