//! evdev key-code classification and naming (input-event-codes.h subset).

use uvie_core::settings::Hotkey;
use uvie_core::KeyKind;

// evdev key codes we care about (linux/input-event-codes.h).
pub const KEY_ESC: u16 = 1;
pub const KEY_BACKSPACE: u16 = 14;
pub const KEY_TAB: u16 = 15;
pub const KEY_ENTER: u16 = 28;
pub const KEY_LEFTCTRL: u16 = 29;
pub const KEY_LEFTSHIFT: u16 = 42;
pub const KEY_LEFTALT: u16 = 56;
pub const KEY_RIGHTSHIFT: u16 = 54;
pub const KEY_RIGHTALT: u16 = 100;
pub const KEY_RIGHTCTRL: u16 = 97;
pub const KEY_LEFTMETA: u16 = 125;
pub const KEY_RIGHTMETA: u16 = 126;
pub const KEY_CAPSLOCK: u16 = 58;
pub const KEY_HOME: u16 = 102;
pub const KEY_PAGEUP: u16 = 104;
pub const KEY_LEFT: u16 = 105;
pub const KEY_RIGHT: u16 = 106;
pub const KEY_END: u16 = 107;
pub const KEY_DOWN: u16 = 108;
pub const KEY_UP: u16 = 103;
pub const KEY_PAGEDOWN: u16 = 109;
pub const KEY_INSERT: u16 = 110;
pub const KEY_DELETE: u16 = 111;
pub const KEY_KPENTER: u16 = 96;

pub const CTRL_KEYS: &[u16] = &[KEY_LEFTCTRL, KEY_RIGHTCTRL];
pub const ALT_KEYS: &[u16] = &[KEY_LEFTALT, KEY_RIGHTALT];
pub const SHIFT_KEYS: &[u16] = &[KEY_LEFTSHIFT, KEY_RIGHTSHIFT];
pub const META_KEYS: &[u16] = &[KEY_LEFTMETA, KEY_RIGHTMETA];

/// Classify a key press. `ch` is the resolved character when the key is
/// printable (from the xkb state).
pub fn classify(code: u16, ch: Option<char>) -> KeyKind {
    match code {
        KEY_BACKSPACE => return KeyKind::Backspace,
        KEY_ENTER | KEY_KPENTER | KEY_TAB | KEY_ESC => return KeyKind::Break,
        KEY_LEFT | KEY_RIGHT | KEY_UP | KEY_DOWN | KEY_HOME | KEY_END | KEY_PAGEUP
        | KEY_PAGEDOWN | KEY_DELETE | KEY_INSERT => return KeyKind::Navigation,
        _ => {}
    }
    match ch {
        Some(c) if !c.is_control() => KeyKind::Char(c),
        _ => KeyKind::Other,
    }
}

/// Does this chord match the configured toggle hotkey?
/// `pressed` holds every currently-down modifier key code.
pub fn chord_matches(hotkey: &Hotkey, code: u16, ctrl: bool, alt: bool, shift: bool) -> bool {
    code == hotkey.key && ctrl == hotkey.ctrl && alt == hotkey.alt && shift == hotkey.shift
}
