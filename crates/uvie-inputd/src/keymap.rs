//! xkb keymap state: resolves evdev key codes to characters for the active
//! layout, and builds the reverse map (char -> key code + shift) the
//! injector needs to type plain ASCII.

use std::collections::HashMap;
use xkbcommon::xkb;

/// evdev key codes sit 8 below xkb keycodes (KEY_ESC: evdev 1 -> xkb 9).
const EVDEV_OFFSET: u32 = 8;

/// First evdev key code worth probing for characters (KEY_ESC and below
/// have no printable output anyway).
const MIN_EVDEV_KEY: u32 = 1;
const MAX_EVDEV_KEY: u32 = 255;

/// A key code plus the shift level needed to type it.
#[derive(Debug, Clone, Copy)]
pub struct CharKey {
    pub code: u16,
    pub shift: bool,
}

pub struct Keymap {
    _ctx: xkb::Context,
    keymap: xkb::Keymap,
    state: xkb::State,
}

impl Keymap {
    /// Build from the RMLVO environment (XKB_DEFAULT_* vars / system
    /// defaults — usually `us`).
    pub fn new() -> Option<Self> {
        let ctx = xkb::Context::new(xkb::CONTEXT_NO_FLAGS);
        let keymap =
            xkb::Keymap::new_from_names(&ctx, "", "", "", "", None, xkb::KEYMAP_COMPILE_NO_FLAGS)?;
        let state = xkb::State::new(&keymap);
        Some(Self {
            _ctx: ctx,
            keymap,
            state,
        })
    }

    /// Update the tracked modifier/depressed state. Feed every physical
    /// press (Down) and release (Up) — consumed or not.
    pub fn update(&mut self, evdev_code: u16, down: bool) {
        let dir = if down {
            xkb::KeyDirection::Down
        } else {
            xkb::KeyDirection::Up
        };
        self.state.update_key(
            xkb::Keycode::from(u32::from(evdev_code) + EVDEV_OFFSET),
            dir,
        );
    }

    /// The character this physical key press would produce in the current
    /// state (empty for modifiers, dead keys, F-keys, ...).
    pub fn char_for(&self, evdev_code: u16) -> Option<char> {
        let s = self
            .state
            .key_get_utf8(xkb::Keycode::from(u32::from(evdev_code) + EVDEV_OFFSET));
        let mut chars = s.chars();
        match (chars.next(), chars.next()) {
            (Some(c), None) => Some(c),
            _ => None,
        }
    }

    /// Reverse map: char -> (key code, shift?) for every single char the
    /// layout can type at level 0 or Shift level 1.
    pub fn char_map(&self) -> HashMap<char, CharKey> {
        let mut map = HashMap::new();
        let min = self.keymap.min_keycode().raw();
        let max = self.keymap.max_keycode().raw();
        for kc in min..=max {
            let evdev = kc.saturating_sub(EVDEV_OFFSET);
            if !(MIN_EVDEV_KEY..=MAX_EVDEV_KEY).contains(&evdev) {
                continue;
            }
            for level in 0..2u32 {
                let syms = self
                    .keymap
                    .key_get_syms_by_level(xkb::Keycode::from(kc), 0, level);
                if syms.len() != 1 {
                    continue;
                }
                let text = xkb::keysym_to_utf8(syms[0]);
                let mut chars = text.chars();
                let (Some(c), None) = (chars.next(), chars.next()) else {
                    continue;
                };
                if c.is_ascii() {
                    map.entry(c).or_insert(CharKey {
                        code: evdev as u16,
                        shift: level == 1,
                    });
                }
            }
        }
        map
    }
}
