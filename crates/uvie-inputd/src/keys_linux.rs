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

/// evdev BTN_LEFT..BTN_TASK — pointer buttons that may appear on a
/// grabbed keyboard+pointer combo node.
pub const BTN_MOUSE_MIN: u16 = 0x110;
pub const BTN_MOUSE_MAX: u16 = 0x117;
pub const BTN_TOUCH: u16 = 0x14a;

/// True for pointer buttons — treated like a mouse click for
/// composition purposes (the click may have moved the caret).
pub fn is_pointer_button(code: u16) -> bool {
    (BTN_MOUSE_MIN..=BTN_MOUSE_MAX).contains(&code) || code == BTN_TOUCH
}

/// Classify a key press. `ch` is the resolved character when the key is
/// printable (from the xkb state).
pub fn classify(code: u16, ch: Option<char>) -> KeyKind {
    match code {
        KEY_BACKSPACE => return KeyKind::Backspace,
        KEY_ENTER | KEY_KPENTER => return KeyKind::Enter,
        KEY_TAB => return KeyKind::Tab,
        KEY_ESC => return KeyKind::Escape,
        KEY_LEFT => return KeyKind::Left,
        KEY_RIGHT => return KeyKind::Right,
        KEY_UP | KEY_DOWN | KEY_HOME | KEY_END | KEY_PAGEUP | KEY_PAGEDOWN | KEY_INSERT => {
            return KeyKind::Navigation
        }
        KEY_DELETE => return KeyKind::ForwardDelete,
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

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn classify_fixed_keys_before_char() {
        assert!(matches!(classify(KEY_BACKSPACE, None), KeyKind::Backspace));
        assert!(matches!(classify(KEY_ENTER, None), KeyKind::Enter));
        assert!(matches!(classify(KEY_KPENTER, None), KeyKind::Enter));
        assert!(matches!(classify(KEY_TAB, None), KeyKind::Tab));
        assert!(matches!(classify(KEY_ESC, None), KeyKind::Escape));
        assert!(matches!(classify(KEY_LEFT, None), KeyKind::Left));
        assert!(matches!(classify(KEY_RIGHT, None), KeyKind::Right));
        for code in [
            KEY_UP,
            KEY_DOWN,
            KEY_HOME,
            KEY_END,
            KEY_PAGEUP,
            KEY_PAGEDOWN,
            KEY_INSERT,
        ] {
            assert!(
                matches!(classify(code, None), KeyKind::Navigation),
                "code {code}"
            );
        }
        assert!(matches!(classify(KEY_DELETE, None), KeyKind::ForwardDelete));
    }

    #[test]
    fn classify_fixed_keys_win_over_char() {
        // A resolved char on a named key must not turn it into KeyKind::Char
        // (e.g. Enter resolving '\r').
        assert!(matches!(classify(KEY_ENTER, Some('\r')), KeyKind::Enter));
        assert!(matches!(
            classify(KEY_BACKSPACE, Some('x')),
            KeyKind::Backspace
        ));
    }

    #[test]
    fn classify_printable_char() {
        assert!(matches!(classify(30, Some('a')), KeyKind::Char('a')));
        // Control chars (e.g. Ctrl+key resolving to a control code) are Other.
        assert!(matches!(classify(30, Some('\u{1}')), KeyKind::Other));
        assert!(matches!(classify(30, None), KeyKind::Other));
        // Modifier codes produce no char.
        assert!(matches!(classify(KEY_LEFTSHIFT, None), KeyKind::Other));
    }

    #[test]
    fn chord_matches_exact_modifiers_only() {
        let h = Hotkey::default(); // Ctrl+Shift+Z (key 44)
        assert!(chord_matches(&h, 44, true, false, true));
        // Wrong key, missing modifier, extra modifier: no match.
        assert!(!chord_matches(&h, 45, true, false, true));
        assert!(!chord_matches(&h, 44, false, false, true));
        assert!(!chord_matches(&h, 44, true, true, true)); // extra Alt held
                                                           // A hotkey with no modifiers still requires none held.
        let bare = Hotkey {
            ctrl: false,
            alt: false,
            shift: false,
            key: 59,
        };
        assert!(chord_matches(&bare, 59, false, false, false));
        assert!(!chord_matches(&bare, 59, true, false, false));
    }

    #[test]
    fn is_pointer_button_range() {
        assert!(is_pointer_button(0x110)); // BTN_LEFT
        assert!(is_pointer_button(0x117)); // BTN_TASK
        assert!(is_pointer_button(0x14a)); // BTN_TOUCH
        assert!(!is_pointer_button(0x10f));
        assert!(!is_pointer_button(0x118));
        assert!(!is_pointer_button(30)); // KEY_A
        assert!(!is_pointer_button(KEY_LEFTCTRL));
    }
}
