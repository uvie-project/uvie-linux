//! Text injection through a uinput virtual keyboard.
//!
//! uinput emits key *codes*, not characters, and the active keymap belongs
//! to the compositor — accented Vietnamese output can't be typed directly.
//! Layered strategy (`InjectionMode`):
//!
//!   * ASCII fast path — look up the char in the keymap's reverse map
//!     (level-0 or Shift level-1) and emit the real key code.
//!   * `unicode_hex` — Ctrl+Shift+U hex sequence (GTK/IBus-aware fields).
//!   * `clipboard` — accumulate the string, then write it to the clipboard
//!     once per burst and emit Ctrl+V (espanso-style fallback). Per-
//!     keystroke pastes race: the app fetches the selection asynchronously
//!     and a fetch can land after the *next* set_text, serving the wrong
//!     string (observed as "đơợc" at ~30-50ms/keystroke). Batching makes
//!     the whole burst a single selection hand-off.

use std::collections::HashMap;
use std::io;
use std::sync::{Arc, Mutex};
use std::time::{Duration, Instant};

use evdev::InputEvent;

use crate::vdev::Vdev;

use uvie_core::settings::{InjectionMode, RewriteMode};

use crate::keymap::{CharKey, Keymap};
use crate::keys_linux::*;

/// Name of the virtual input device — also used to recognize (and never
/// grab) our own device when scanning /dev/input.
pub const VIRTUAL_DEVICE_NAME: &str = "uvie virtual keyboard";

const EV_KEY: u16 = 0x01;

/// Restore the user's clipboard this long after the last flush — the app
/// must have fetched the selection by then (espanso's trick).
const CLIPBOARD_RESTORE_DELAY: Duration = Duration::from_millis(400);
/// Quiet time before a pending paste burst is flushed as one selection.
const PASTE_FLUSH_DELAY: Duration = Duration::from_millis(150);
/// Extra margin after emitting Ctrl+V before we may set_text again
/// (arboard/Wayland path only — on X11 we wait for the fetch itself).
const PASTE_SETTLE: Duration = Duration::from_millis(60);
/// How long `paste_owned` waits for the app to fetch the selection after
/// Ctrl+V before giving up (keeps input flowing on a stuck/slow app).
const PASTE_FETCH_WAIT: Duration = Duration::from_millis(250);

/// Clipboard state shared with the delayed-restore thread. The user's
/// clipboard contents are captured ONCE per burst; `gen` serializes
/// restores so only the last flush of a burst restores.
#[derive(Default)]
struct PasteState {
    /// Clipboard contents before our first set_text (`None` = restored).
    saved: Option<String>,
    /// Bumped on every flush; a restore fires only if gen is unchanged
    /// after the delay.
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
    /// Owned CLIPBOARD selection (X11) — unlike arboard we see the app's
    /// SelectionRequest fetches, so we can wait for the fetch before
    /// allowing the next set_text. `None` on Wayland / no DISPLAY.
    xsel: Option<crate::xsel::XSel>,
    /// Accumulated non-ASCII text awaiting its single-burst paste.
    pending_paste: String,
    paste_deadline: Option<Instant>,
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
            xsel: crate::xsel::XSel::connect(),
            pending_paste: String::new(),
            paste_deadline: None,
            paste_state: Arc::new(Mutex::new(PasteState::default())),
        })
    }

    /// Rebuild after settings/layout changes.
    pub fn configure(&mut self, keymap: &Keymap, mode: InjectionMode, rewrite: RewriteMode) {
        self.flush_pending();
        self.char_map = keymap.char_map();
        self.mode = mode;
        self.rewrite = rewrite;
    }

    /// Whether a paste burst is waiting to be flushed (daemon should poll
    /// sooner so the flush deadline is honored).
    pub fn has_pending(&self) -> bool {
        !self.pending_paste.is_empty()
    }

    /// Flush a pending paste burst if its quiet-time deadline elapsed.
    /// Called from the daemon's housekeeping tick.
    pub fn flush_due(&mut self) {
        if let Some(d) = self.paste_deadline {
            if Instant::now() >= d {
                self.flush_pending();
            }
        }
    }

    /// Serve any late selection requests (a user pasting our injected
    /// text manually, clipboard managers). Cheap; call each tick.
    pub fn pump_selection(&mut self) {
        if let Some(xsel) = self.xsel.as_mut() {
            xsel.pump();
        }
    }

    /// Forward one physical event verbatim (pass-through key, SYN, MSC...).
    /// Only flushes pending pastes before EV_KEY events — the SYN reports
    /// that terminate every event batch would otherwise flush the burst
    /// immediately and defeat batching.
    pub fn forward(&mut self, ev: &InputEvent) {
        if ev.event_type().0 == EV_KEY {
            self.flush_pending();
        }
        let _ = self.vdev.emit(ev.event_type().0, ev.code(), ev.value());
    }

    /// Forward a raw (type, code, value) triple.
    pub fn emit_raw(&mut self, type_: u16, code: u16, value: i32) {
        self.flush_pending();
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

    /// Press or release a raw key code on the virtual device. Internal:
    /// does NOT flush pending pastes (the paste itself emits keys).
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

    /// Erase `n` characters left of the caret. Backspaces first eat into
    /// the not-yet-flushed pending paste (they were issued against screen
    /// text that includes it), then emit real backspaces for the rest.
    pub fn erase(&mut self, n: usize) {
        let mut n = n;
        while n > 0 && !self.pending_paste.is_empty() {
            self.pending_paste.pop();
            n -= 1;
        }
        if self.pending_paste.is_empty() {
            self.paste_deadline = None;
        }
        if n == 0 {
            return;
        }
        self.flush_pending();
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
        self.flush_pending();
        for _ in 0..n {
            self.tap(KEY_DELETE);
        }
    }

    /// Type a whole string (one diff suffix).
    pub fn type_str(&mut self, text: &str) {
        if matches!(self.effective_mode(), InjectionMode::Clipboard) && !text.is_ascii() {
            self.queue_paste(text);
            return;
        }
        self.flush_pending();
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
            InjectionMode::Clipboard => self.queue_paste(&c.to_string()),
            _ => self.unicode_seq(c),
        }
    }

    /// Accumulate text into the pending paste burst; the burst is flushed
    /// as ONE set_text+Ctrl+V after `PASTE_FLUSH_DELAY` of quiet or before
    /// the next non-paste output.
    fn queue_paste(&mut self, text: &str) {
        self.pending_paste.push_str(text);
        self.paste_deadline = Some(Instant::now() + PASTE_FLUSH_DELAY);
    }

    /// Paste the accumulated burst via the clipboard, then schedule the
    /// delayed restore of the user's previous clipboard contents.
    fn flush_pending(&mut self) {
        let text = std::mem::take(&mut self.pending_paste);
        self.paste_deadline = None;
        if text.is_empty() {
            return;
        }
        let current_clip = self
            .clipboard
            .as_mut()
            .and_then(|clip| clip.get_text().ok());
        let gen = {
            let mut st = self.paste_state.lock().unwrap();
            if st.saved.is_none() {
                st.saved = current_clip;
            }
            st.gen += 1;
            st.gen
        };
        if self.paste_owned(&text) {
            self.schedule_restore(gen);
            return;
        }
        let Some(clip) = self.clipboard.as_mut() else {
            // No clipboard available (headless) — fall back to hex input.
            self.clear_saved(gen);
            for c in text.chars() {
                self.unicode_seq(c);
            }
            return;
        };
        if clip.set_text(&text).is_err() {
            self.clear_saved(gen);
            for c in text.chars() {
                self.unicode_seq(c);
            }
            return;
        }
        self.key(KEY_LEFTCTRL, true);
        self.tap(47); // KEY_V
        self.key(KEY_LEFTCTRL, false);
        self.schedule_restore(gen);
        // Give the app a window to actually read the selection before the
        // next flush's set_text could overwrite it (arboard path only —
        // the owned-selection path already waited for the fetch).
        std::thread::sleep(PASTE_SETTLE);
    }

    /// Paste through our own X selection and wait until a fetch has been
    /// served after Ctrl+V — ordering-safe on X11. Clipboard managers
    /// (Klipper) fetch right on ownership change, so we drain those
    /// requests BEFORE emitting Ctrl+V; a conversion arriving after V is
    /// the app's paste. Returns false if no owned selection is available
    /// (Wayland) or ownership failed.
    fn paste_owned(&mut self, text: &str) -> bool {
        let Some(mut xsel) = self.xsel.take() else {
            return false;
        };
        xsel.set_text(text);
        xsel.wait_fetched(Duration::from_millis(40));
        self.key(KEY_LEFTCTRL, true);
        self.tap(47); // KEY_V
        self.key(KEY_LEFTCTRL, false);
        // Wait for the app's fetch; bounded — a very slow app degrades to
        // the old timing rather than freezing input.
        xsel.wait_fetched(PASTE_FETCH_WAIT);
        self.xsel = Some(xsel);
        true
    }

    /// Drop the captured user clipboard when a flush never touched it
    /// (fallback paths), keeping the state consistent for the next burst.
    fn clear_saved(&mut self, gen: u64) {
        let mut st = self.paste_state.lock().unwrap();
        if st.gen == gen {
            st.saved = None;
        }
    }

    fn schedule_restore(&mut self, gen: u64) {
        let state = Arc::clone(&self.paste_state);
        std::thread::spawn(move || {
            std::thread::sleep(CLIPBOARD_RESTORE_DELAY);
            let prev = {
                let mut st = state.lock().unwrap();
                if st.gen != gen {
                    return; // a newer flush owns the pending restore
                }
                st.saved.take()
            };
            if let Some(prev) = prev {
                if let Ok(mut clip) = arboard::Clipboard::new() {
                    let _ = clip.set_text(prev);
                }
            }
        });
    }
}
