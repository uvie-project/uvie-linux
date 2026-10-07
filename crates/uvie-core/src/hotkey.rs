//! evdev key-code naming + `Hotkey` string round-trips ("Ctrl+Shift+Z").
//! Shared by `uvie-inputd` (chord matching) and `uvie-ui` (display/edit).

use crate::settings::Hotkey;

/// Human-readable name of an evdev key code, for hotkey display/parse.
pub fn key_name(code: u16) -> String {
    match code {
        1 => "Escape".into(),
        14 => "Backspace".into(),
        15 => "Tab".into(),
        28 => "Enter".into(),
        96 => "KpEnter".into(),
        58 => "CapsLock".into(),
        42 | 54 => "Shift".into(),
        29 | 97 => "Ctrl".into(),
        56 | 100 => "Alt".into(),
        125 | 126 => "Super".into(),
        105 => "Left".into(),
        106 => "Right".into(),
        103 => "Up".into(),
        108 => "Down".into(),
        102 => "Home".into(),
        107 => "End".into(),
        104 => "PageUp".into(),
        109 => "PageDown".into(),
        110 => "Insert".into(),
        111 => "Delete".into(),
        // evdev letter keys follow QWERTY row order, not alphabet order:
        // KEY_Q..KEY_P = 16..25, KEY_A..KEY_L = 30..38, KEY_Z..KEY_M = 44..50.
        c @ (16..=25) => (b"QWERTYUIOP"[(c - 16) as usize] as char).to_string(),
        c @ (30..=38) => (b"ASDFGHJKL"[(c - 30) as usize] as char).to_string(),
        c @ (44..=50) => (b"ZXCVBNM"[(c - 44) as usize] as char).to_string(),
        c @ (2..=10) => ((b'1' + (c - 2) as u8) as char).to_string(),
        11 => "0".into(),
        12 => "-".into(),
        13 => "=".into(),
        26 => "[".into(),
        27 => "]".into(),
        39 => ";".into(),
        40 => "'".into(),
        41 => "`".into(),
        43 => "\\".into(),
        51 => ",".into(),
        52 => ".".into(),
        53 => "/".into(),
        57 => "Space".into(),
        c @ (59..=68) => format!("F{}", c - 58),
        c => format!("Key{c}"),
    }
}

/// Parse a key name back to an evdev code (inverse of `key_name` for the
/// common keys; returns None for anything unrecognized).
pub fn parse_key_name(name: &str) -> Option<u16> {
    let name = name.trim();
    let lower = name.to_ascii_lowercase();
    match lower.as_str() {
        "escape" | "esc" => return Some(1),
        "backspace" => return Some(14),
        "tab" => return Some(15),
        "enter" | "return" => return Some(28),
        "capslock" => return Some(58),
        "left" => return Some(105),
        "right" => return Some(106),
        "up" => return Some(103),
        "down" => return Some(108),
        "home" => return Some(102),
        "end" => return Some(107),
        "pageup" => return Some(104),
        "pagedown" => return Some(109),
        "insert" => return Some(110),
        "delete" => return Some(111),
        "space" => return Some(57),
        _ => {}
    }
    if name.len() == 1 {
        let c = name.chars().next().unwrap().to_ascii_uppercase();
        match c {
            'A'..='Z' => return letter_to_code(c),
            '1'..='9' => return Some(1 + (c as u16 - b'0' as u16)),
            '0' => return Some(11),
            '-' => return Some(12),
            '=' => return Some(13),
            '[' => return Some(26),
            ']' => return Some(27),
            ';' => return Some(39),
            '\'' => return Some(40),
            '`' => return Some(41),
            '\\' => return Some(43),
            ',' => return Some(51),
            '.' => return Some(52),
            '/' => return Some(53),
            _ => {}
        }
    }
    if let Some(rest) = lower.strip_prefix('f') {
        if let Ok(n) = rest.parse::<u16>() {
            if (1..=12).contains(&n) {
                return Some(58 + n);
            }
        }
    }
    None
}

fn letter_to_code(c: char) -> Option<u16> {
    // KEY_Q..KEY_P = 16..25, KEY_A..KEY_L = 30..38, KEY_Z..KEY_M = 44..50
    if let Some(i) = b"QWERTYUIOP".iter().position(|&l| l == c as u8) {
        return Some(16 + i as u16);
    }
    if let Some(i) = b"ASDFGHJKL".iter().position(|&l| l == c as u8) {
        return Some(30 + i as u16);
    }
    if let Some(i) = b"ZXCVBNM".iter().position(|&l| l == c as u8) {
        return Some(44 + i as u16);
    }
    None
}

/// Format a Hotkey like "Ctrl+Shift+Z".
pub fn format_hotkey(h: &Hotkey) -> String {
    let mut parts = Vec::new();
    if h.ctrl {
        parts.push("Ctrl".to_string());
    }
    if h.alt {
        parts.push("Alt".to_string());
    }
    if h.shift {
        parts.push("Shift".to_string());
    }
    parts.push(key_name(h.key));
    parts.join("+")
}

/// Parse "Ctrl+Shift+Z" into a Hotkey. Unknown pieces are ignored; the
/// last non-modifier token becomes the key.
pub fn parse_hotkey(text: &str) -> Option<Hotkey> {
    let mut h = Hotkey {
        ctrl: false,
        alt: false,
        shift: false,
        key: 0,
    };
    let mut saw_key = false;
    for token in text.split('+').map(|t| t.trim()).filter(|t| !t.is_empty()) {
        match token.to_ascii_lowercase().as_str() {
            "ctrl" | "control" => h.ctrl = true,
            "alt" => h.alt = true,
            "shift" => h.shift = true,
            _ => {
                if let Some(code) = parse_key_name(token) {
                    h.key = code;
                    saw_key = true;
                }
            }
        }
    }
    saw_key.then_some(h)
}
