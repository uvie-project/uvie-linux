//! `uvie_dispatch_*` — C ABI over `uvie_host::Dispatcher`.
//!
//! The thin C/C++ frontends (ibus-uvie, fcitx5-uvie) feed already-classified
//! key presses and get back a consume/pass decision plus the `InputAction`
//! plan to apply. This keeps macros, auto-capitalize, escape-restore, and
//! caret/sentence behavior identical to `uvie-inputd` and uvie-win.

use std::ffi::c_int;
use std::os::raw::c_char;
use std::path::Path;

use uvie_host::dispatcher::{Context, Dispatch, Dispatcher, InputAction, InputLanguage};
use uvie_host::engine_session::{EngineOptions, EngineSession};
use uvie_host::keys::{KeyEvent, KeyKind};
use uvie_host::macros::MacroTable;
use uvie_host::InputMethod;

/// Opaque dispatcher handle.
pub struct UvieDispatcher {
    d: Dispatcher,
}

/// One `InputAction` in C form. `type` mirrors the plan order:
/// 0 = Backspace(n), 1 = ForwardDelete(n), 2 = Text(utf8).
#[repr(C)]
pub struct UvieAction {
    pub type_: c_int,
    pub n: c_int,
    pub text: [u8; 512],
}

impl Default for UvieAction {
    fn default() -> Self {
        Self {
            type_: 0,
            n: 0,
            text: [0; 512],
        }
    }
}

/// kind codes accepted by `uvie_dispatch_feed` (see uvie.h).
fn key_kind(kind: c_int, ch: u32) -> KeyKind {
    match kind {
        0 => KeyKind::Char(char::from_u32(ch).unwrap_or('\0')),
        1 => KeyKind::Backspace,
        2 => KeyKind::Enter,
        3 => KeyKind::Tab,
        4 => KeyKind::Escape,
        5 => KeyKind::Left,
        6 => KeyKind::Right,
        7 => KeyKind::Navigation,
        8 => KeyKind::ForwardDelete,
        9 => KeyKind::Break,
        _ => KeyKind::Other,
    }
}

/// Write one `InputAction` into an out slot. Overlong Text is truncated
/// on a UTF-8 boundary (same rule as `uvie::ffi` output buffers) rather
/// than dropped — a dropped Text after emitted Backspaces would erase
/// the word and type nothing (e.g. a macro expansion >511 bytes).
fn write_action(out: &mut UvieAction, action: &InputAction) -> bool {
    match action {
        InputAction::Backspace(n) => {
            out.type_ = 0;
            out.n = *n as c_int;
        }
        InputAction::ForwardDelete(n) => {
            out.type_ = 1;
            out.n = *n as c_int;
        }
        InputAction::Text(s) => {
            let bytes = s.as_bytes();
            let cap = out.text.len() - 1;
            let mut len = bytes.len().min(cap);
            while len > 0 && !s.is_char_boundary(len) {
                len -= 1;
            }
            out.type_ = 2;
            out.n = 0;
            out.text[..len].copy_from_slice(&bytes[..len]);
            out.text[len] = 0;
        }
    }
    true
}

/// # Safety
/// `d` must come from `uvie_dispatch_new` and stay alive across calls;
/// `out` must point to at least `*n_out` `UvieAction`s.
#[no_mangle]
pub unsafe extern "C" fn uvie_dispatch_feed(
    d: *mut UvieDispatcher,
    kind: c_int,
    ch: u32,
    mods: u32,
    ctx: u32,
    out: *mut UvieAction,
    n_out: *mut c_int,
) -> c_int {
    if d.is_null() || out.is_null() || n_out.is_null() {
        return 0;
    }
    let d = &mut (*d).d;
    let result = std::panic::catch_unwind(std::panic::AssertUnwindSafe(|| {
        feed_impl(d, kind, ch, mods, ctx, out, n_out)
    }));
    result.unwrap_or(0)
}

#[allow(clippy::too_many_arguments)]
fn feed_impl(
    d: &mut Dispatcher,
    kind: c_int,
    ch: u32,
    mods: u32,
    ctx: u32,
    out: *mut UvieAction,
    n_out: *mut c_int,
) -> c_int {
    let key = KeyEvent {
        kind: key_kind(kind, ch),
        ctrl: mods & 1 != 0,
        alt: mods & 2 != 0,
        shift: mods & 4 != 0,
        win: mods & 8 != 0,
        injected: false,
    };
    let context = Context {
        language: None,
        app_excluded: ctx & 1 != 0,
        non_latin_layout: ctx & 2 != 0,
    };
    let (consume, plan) = match d.handle(key, &context) {
        Dispatch::Pass(plan) => (0, plan),
        Dispatch::Consume(plan) => (1, plan),
    };
    // SAFETY: `out`/`n_out` were validated non-null by the extern caller;
    // `out` points to at least `cap` UvieActions by the API contract.
    unsafe {
        let cap = (*n_out).max(0) as usize;
        let mut n = 0;
        for action in plan.iter() {
            if n >= cap {
                break;
            }
            let slot = &mut *out.add(n);
            if !write_action(slot, action) {
                break;
            }
            n += 1;
        }
        *n_out = n as c_int;
    }
    consume
}

#[no_mangle]
pub extern "C" fn uvie_dispatch_new() -> *mut UvieDispatcher {
    let d = Dispatcher::new(EngineSession::new());
    Box::into_raw(Box::new(UvieDispatcher { d }))
}

/// # Safety
/// `d` must be a pointer returned by `uvie_dispatch_new` (or NULL).
#[no_mangle]
pub unsafe extern "C" fn uvie_dispatch_free(d: *mut UvieDispatcher) {
    if !d.is_null() {
        drop(Box::from_raw(d));
    }
}

/// Apply the engine options from `settings.json` (same field order as
/// `uvie_engine_set_*` chain in uvie-config.h).
#[no_mangle]
pub extern "C" fn uvie_dispatch_apply_options(
    d: *mut UvieDispatcher,
    input_method: c_int,
    quick_start: c_int,
    quick_telex: c_int,
    modern_orthography: c_int,
    relaxed_coda: c_int,
    english_override: c_int,
) {
    if d.is_null() {
        return;
    }
    let d = unsafe { &mut (*d).d };
    // Out-of-range values keep the current method, like
    // uvie_engine_set_input_method does.
    let method = match input_method {
        0 => InputMethod::Telex,
        1 => InputMethod::Vni,
        2 => InputMethod::SimpleTelex,
        _ => return,
    };
    d.session.set_input_method(method);
    d.session.apply_options(EngineOptions {
        quick_start: quick_start != 0,
        quick_telex: quick_telex != 0,
        modern_orthography: modern_orthography != 0,
        relaxed_coda: relaxed_coda != 0,
        english_override: english_override != 0,
    });
}

#[no_mangle]
pub extern "C" fn uvie_dispatch_set_language(d: *mut UvieDispatcher, english: c_int) {
    if d.is_null() {
        return;
    }
    let lang = if english != 0 {
        InputLanguage::English
    } else {
        InputLanguage::Vietnamese
    };
    unsafe { (*d).d.set_language(lang) };
}

#[no_mangle]
pub extern "C" fn uvie_dispatch_get_language(d: *const UvieDispatcher) -> c_int {
    if d.is_null() {
        return 0;
    }
    match unsafe { &(*d).d.language } {
        InputLanguage::English => 1,
        InputLanguage::Vietnamese => 0,
    }
}

#[no_mangle]
pub extern "C" fn uvie_dispatch_toggle_language(d: *mut UvieDispatcher) {
    if d.is_null() {
        return;
    }
    let d = unsafe { &mut (*d).d };
    let next = match d.language {
        InputLanguage::Vietnamese => InputLanguage::English,
        InputLanguage::English => InputLanguage::Vietnamese,
    };
    d.set_language(next);
}

#[no_mangle]
pub extern "C" fn uvie_dispatch_set_auto_capitalize(d: *mut UvieDispatcher, on: c_int) {
    if !d.is_null() {
        unsafe { (*d).d.auto_capitalize = on != 0 };
    }
}

#[no_mangle]
pub extern "C" fn uvie_dispatch_set_macro_enabled(d: *mut UvieDispatcher, on: c_int) {
    if !d.is_null() {
        unsafe { (*d).d.macro_enabled = on != 0 };
    }
}

/// Post-commit editing (LabanKey-style edits of the last committed word).
/// Off for the preedit-model frontends: they can't rewrite text already
/// committed to the app.
#[no_mangle]
pub extern "C" fn uvie_dispatch_set_edit_committed(d: *mut UvieDispatcher, on: c_int) {
    if !d.is_null() {
        unsafe { (*d).d.edit_committed = on != 0 };
    }
}

/// Load `macros.json` into the dispatcher's table. Returns the entry
/// count (0 on missing/corrupt file — same rule as the daemon).
///
/// # Safety
/// `path` must be a valid NUL-terminated UTF-8 string or NULL.
#[no_mangle]
pub unsafe extern "C" fn uvie_dispatch_load_macros(
    d: *mut UvieDispatcher,
    path: *const c_char,
) -> c_int {
    if d.is_null() {
        return 0;
    }
    let d = &mut (*d).d;
    let table = std::panic::catch_unwind(|| {
        if path.is_null() {
            MacroTable::default()
        } else {
            let cstr = std::ffi::CStr::from_ptr(path);
            MacroTable::load(Path::new(cstr.to_string_lossy().as_ref()))
        }
    })
    .unwrap_or_default();
    let n = table.entries.len();
    d.macros = table;
    n as c_int
}

#[no_mangle]
pub extern "C" fn uvie_dispatch_reset(d: *mut UvieDispatcher) {
    if !d.is_null() {
        unsafe { (*d).d.reset() };
    }
}

/// # Safety
/// `app` must be a valid NUL-terminated UTF-8 string or NULL ("" then).
#[no_mangle]
pub unsafe extern "C" fn uvie_dispatch_on_app_switch(d: *mut UvieDispatcher, app: *const c_char) {
    if d.is_null() {
        return;
    }
    let app = if app.is_null() {
        String::new()
    } else {
        std::ffi::CStr::from_ptr(app).to_string_lossy().into_owned()
    };
    (*d).d.on_app_switch(&app);
}

#[no_mangle]
pub extern "C" fn uvie_dispatch_on_pointer_down(d: *mut UvieDispatcher) {
    if !d.is_null() {
        unsafe { (*d).d.on_pointer_down() };
    }
}

#[no_mangle]
pub extern "C" fn uvie_dispatch_is_composing(d: *const UvieDispatcher) -> c_int {
    if d.is_null() {
        return 0;
    }
    unsafe { (*d).d.session.is_composing() as c_int }
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::ffi::CString;

    /// Feed one printable key (kind 0, no mods, no ctx) and apply the plan
    /// to `screen` the way a frontend would: consume or pass, the plan runs
    /// first; a passed-through key appends itself.
    fn feed_char(d: *mut UvieDispatcher, screen: &mut String, ch: char) -> c_int {
        let mut actions: [UvieAction; 16] = std::array::from_fn(|_| UvieAction::default());
        let mut n: c_int = actions.len() as c_int;
        let consumed =
            unsafe { uvie_dispatch_feed(d, 0, ch as u32, 0, 0, actions.as_mut_ptr(), &mut n) };
        apply(screen, &actions[..n.max(0) as usize]);
        if consumed == 0 {
            screen.push(ch);
        }
        consumed
    }

    fn apply(screen: &mut String, actions: &[UvieAction]) {
        for a in actions {
            match a.type_ {
                0 => {
                    for _ in 0..a.n {
                        screen.pop();
                    }
                }
                1 => {}
                2 => {
                    let len = a.text.iter().position(|&b| b == 0).unwrap_or(a.text.len());
                    screen.push_str(std::str::from_utf8(&a.text[..len]).unwrap());
                }
                _ => panic!("unknown action type {}", a.type_),
            }
        }
    }

    fn feed_text(d: *mut UvieDispatcher, screen: &mut String, text: &str) {
        for ch in text.chars() {
            feed_char(d, screen, ch);
        }
    }

    fn new_dispatch() -> *mut UvieDispatcher {
        uvie_dispatch_new()
    }

    #[test]
    fn telex_vectors_end_to_end() {
        let d = new_dispatch();
        // Default session = Telex with default EngineOptions (english_override
        // is OFF here, unlike the daemon - plain engine defaults).
        let mut screen = String::new();
        feed_text(d, &mut screen, "chaof ");
        assert_eq!(screen, "chào ");
        let mut screen = String::new();
        feed_text(d, &mut screen, "vieetj ");
        assert_eq!(screen, "việt ");
        let mut screen = String::new();
        feed_text(d, &mut screen, "dduwowcj ");
        assert_eq!(screen, "được ");
        unsafe { uvie_dispatch_free(d) };
    }

    #[test]
    fn feed_returns_consume_for_letters() {
        let d = new_dispatch();
        let mut screen = String::new();
        assert_eq!(feed_char(d, &mut screen, 'v'), 1);
        unsafe { uvie_dispatch_free(d) };
    }

    #[test]
    fn apply_options_switches_to_vni() {
        let d = new_dispatch();
        // method 1 = VNI, english_override on like the default settings.
        uvie_dispatch_apply_options(d, 1, 0, 0, 0, 0, 1);
        let mut screen = String::new();
        feed_text(d, &mut screen, "vie65t ");
        assert_eq!(screen, "việt ");
        unsafe { uvie_dispatch_free(d) };
    }

    #[test]
    fn apply_options_ignores_out_of_range_method() {
        let d = new_dispatch();
        uvie_dispatch_apply_options(d, 99, 0, 0, 0, 0, 0);
        let mut screen = String::new();
        feed_text(d, &mut screen, "chaof ");
        assert_eq!(screen, "chào ", "method must stay Telex");
        unsafe { uvie_dispatch_free(d) };
    }

    #[test]
    fn set_get_toggle_language() {
        let d = new_dispatch();
        assert_eq!(uvie_dispatch_get_language(d), 0);
        uvie_dispatch_set_language(d, 1);
        assert_eq!(uvie_dispatch_get_language(d), 1);
        uvie_dispatch_toggle_language(d);
        assert_eq!(uvie_dispatch_get_language(d), 0);
        uvie_dispatch_toggle_language(d);
        assert_eq!(uvie_dispatch_get_language(d), 1);
        unsafe { uvie_dispatch_free(d) };
    }

    #[test]
    fn english_mode_passes_keys_through() {
        let d = new_dispatch();
        uvie_dispatch_set_language(d, 1);
        let mut screen = String::new();
        for ch in "viets".chars() {
            assert_eq!(feed_char(d, &mut screen, ch), 0, "'{ch}' must pass");
        }
        assert_eq!(screen, "viets");
        unsafe { uvie_dispatch_free(d) };
    }

    #[test]
    fn is_composing_before_and_after_commit_space() {
        let d = new_dispatch();
        let mut screen = String::new();
        feed_text(d, &mut screen, "vie");
        assert_eq!(uvie_dispatch_is_composing(d), 1);
        feed_char(d, &mut screen, ' ');
        assert_eq!(uvie_dispatch_is_composing(d), 0);
        unsafe { uvie_dispatch_free(d) };
    }

    #[test]
    fn load_macros_and_expand() {
        let dir = std::env::temp_dir().join(format!("uvie-ffi-test-{}", std::process::id()));
        std::fs::create_dir_all(&dir).unwrap();
        let path = dir.join("macros.json");
        let mut t = MacroTable::default();
        t.add("sg", "Sài Gòn");
        t.save(&path).unwrap();

        let d = new_dispatch();
        uvie_dispatch_set_macro_enabled(d, 1);
        let cpath = CString::new(path.to_str().unwrap()).unwrap();
        let count = unsafe { uvie_dispatch_load_macros(d, cpath.as_ptr()) };
        assert_eq!(count, 1);
        let mut screen = String::new();
        feed_text(d, &mut screen, "sg ");
        assert_eq!(screen, "Sài Gòn");

        // A missing file loads zero entries.
        let missing = CString::new(dir.join("nope.json").to_str().unwrap()).unwrap();
        assert_eq!(unsafe { uvie_dispatch_load_macros(d, missing.as_ptr()) }, 0);
        unsafe { uvie_dispatch_free(d) };
        std::fs::remove_dir_all(&dir).ok();
    }

    #[test]
    fn reset_drops_composition() {
        let d = new_dispatch();
        let mut screen = String::new();
        feed_text(d, &mut screen, "vie");
        assert_eq!(uvie_dispatch_is_composing(d), 1);
        uvie_dispatch_reset(d);
        assert_eq!(uvie_dispatch_is_composing(d), 0);
        unsafe { uvie_dispatch_free(d) };
    }

    #[test]
    fn null_arguments_are_safe() {
        let mut actions: [UvieAction; 4] = std::array::from_fn(|_| UvieAction::default());
        let mut n: c_int = actions.len() as c_int;
        unsafe {
            assert_eq!(
                uvie_dispatch_feed(
                    std::ptr::null_mut(),
                    0,
                    'a' as u32,
                    0,
                    0,
                    actions.as_mut_ptr(),
                    &mut n,
                ),
                0
            );
        }
        let d = new_dispatch();
        // NULL out / n_out pointers must also bail out.
        unsafe {
            assert_eq!(
                uvie_dispatch_feed(d, 0, 'a' as u32, 0, 0, std::ptr::null_mut(), &mut n),
                0
            );
            assert_eq!(
                uvie_dispatch_feed(
                    d,
                    0,
                    'a' as u32,
                    0,
                    0,
                    actions.as_mut_ptr(),
                    std::ptr::null_mut(),
                ),
                0
            );
        }
        uvie_dispatch_apply_options(std::ptr::null_mut(), 0, 0, 0, 0, 0, 0);
        uvie_dispatch_set_language(std::ptr::null_mut(), 1);
        uvie_dispatch_toggle_language(std::ptr::null_mut());
        uvie_dispatch_set_auto_capitalize(std::ptr::null_mut(), 1);
        uvie_dispatch_set_macro_enabled(std::ptr::null_mut(), 1);
        uvie_dispatch_set_edit_committed(std::ptr::null_mut(), 1);
        uvie_dispatch_reset(std::ptr::null_mut());
        uvie_dispatch_on_pointer_down(std::ptr::null_mut());
        assert_eq!(uvie_dispatch_get_language(std::ptr::null()), 0);
        assert_eq!(uvie_dispatch_is_composing(std::ptr::null()), 0);
        unsafe {
            assert_eq!(
                uvie_dispatch_load_macros(std::ptr::null_mut(), std::ptr::null()),
                0
            );
            // NULL path on a live dispatcher: empty table, no crash.
            assert_eq!(uvie_dispatch_load_macros(d, std::ptr::null()), 0);
            uvie_dispatch_on_app_switch(d, std::ptr::null());
            uvie_dispatch_free(d);
            uvie_dispatch_free(std::ptr::null_mut());
        }
    }
}
