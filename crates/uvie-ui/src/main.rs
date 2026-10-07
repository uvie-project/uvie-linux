//! uvie-ui — Slint settings window for UVie Linux.
//!
//! Edits `~/.config/uvie/settings.json` and `macros.json`; uvie-inputd
//! watches both files and hot-reloads, so "Lưu" applies immediately.

use std::cell::RefCell;
use std::rc::Rc;

use slint::{ModelRc, SharedString, VecModel};
use uvie_core::settings::{InjectionMode, RewriteMode, Settings};
use uvie_core::{format_hotkey, parse_hotkey, InputMethod, MacroTable};

slint::include_modules!();

fn config_dir() -> std::path::PathBuf {
    std::env::var("XDG_CONFIG_HOME")
        .map(std::path::PathBuf::from)
        .unwrap_or_else(|_| {
            std::path::PathBuf::from(std::env::var("HOME").unwrap_or_default()).join(".config")
        })
        .join("uvie")
}

fn method_index(m: InputMethod) -> i32 {
    match m {
        InputMethod::Telex => 0,
        InputMethod::Vni => 1,
        InputMethod::SimpleTelex => 2,
    }
}

fn injection_index(m: InjectionMode) -> i32 {
    match m {
        InjectionMode::UnicodeHex => 0,
        InjectionMode::Clipboard => 1,
        InjectionMode::Auto => 2,
    }
}

fn main() -> Result<(), slint::PlatformError> {
    let dir = config_dir();
    let settings_path = dir.join("settings.json");
    let macros_path = dir.join("macros.json");

    let settings = Rc::new(RefCell::new(Settings::load(&settings_path)));
    let macros = Rc::new(RefCell::new(MacroTable::load(&macros_path)));

    let app = AppWindow::new()?;
    app.set_version(env!("CARGO_PKG_VERSION").into());

    // ---- push current state into the UI ----
    {
        let s = settings.borrow();
        app.set_enabled(s.enabled);
        app.set_method(method_index(s.input_method));
        app.set_hotkey(format_hotkey(&s.hotkey).into());
        app.set_launch_at_login(s.launch_at_login);
        app.set_quick_start(s.quick_start);
        app.set_quick_telex(s.quick_telex);
        app.set_modern_orthography(s.modern_orthography);
        app.set_relaxed_coda(s.relaxed_coda);
        app.set_english_override(s.english_override);
        app.set_auto_capitalize(s.auto_capitalize);
        app.set_per_app_language(s.per_app_language);
        app.set_macro_enabled(s.macro_enabled);
        app.set_injection_mode(injection_index(s.injection_mode));
        app.set_rewrite_mode(if s.rewrite_mode == RewriteMode::SelectOverwrite {
            1
        } else {
            0
        });
        app.set_excluded_apps(ModelRc::new(VecModel::from(
            s.excluded_apps
                .iter()
                .map(|a| SharedString::from(a.as_str()))
                .collect::<Vec<_>>(),
        )));
    }
    push_macros(&app, &macros.borrow());

    // ---- macro editor callbacks ----
    {
        let app_weak = app.as_weak();
        let macros = macros.clone();
        app.on_add_macro(move |trigger, expansion| {
            if trigger.trim().is_empty() {
                return;
            }
            let mut table = macros.borrow_mut();
            table.add(trigger.trim().to_string(), expansion.to_string());
            drop(table);
            if let Some(app) = app_weak.upgrade() {
                push_macros(&app, &macros.borrow());
            }
        });
    }
    {
        let app_weak = app.as_weak();
        let macros = macros.clone();
        app.on_remove_macro(move |index| {
            let mut table = macros.borrow_mut();
            if let Some(entry) = table.entries.get(index as usize) {
                let key = entry.trigger.clone();
                table.remove(&key);
            }
            drop(table);
            if let Some(app) = app_weak.upgrade() {
                push_macros(&app, &macros.borrow());
            }
        });
    }
    {
        let app_weak = app.as_weak();
        let settings = settings.clone();
        app.on_add_excluded(move |name| {
            let name = name.trim().to_string();
            if name.is_empty() {
                return;
            }
            let mut s = settings.borrow_mut();
            if !s
                .excluded_apps
                .iter()
                .any(|a| a.eq_ignore_ascii_case(&name))
            {
                s.excluded_apps.push(name);
            }
            drop(s);
            refresh_excluded(&app_weak, &settings.borrow());
        });
    }
    {
        let app_weak = app.as_weak();
        let settings = settings.clone();
        app.on_remove_excluded(move |index| {
            let mut s = settings.borrow_mut();
            if (index as usize) < s.excluded_apps.len() {
                s.excluded_apps.remove(index as usize);
            }
            drop(s);
            refresh_excluded(&app_weak, &settings.borrow());
        });
    }

    app.on_open_repo(|| {
        let _ = open::that_detached("https://github.com/uvie-project/uvie-linux");
    });

    // ---- save ----
    {
        let settings = settings.clone();
        let macros = macros.clone();
        let app_weak = app.as_weak();
        app.on_save_clicked(move || {
            let Some(app) = app_weak.upgrade() else {
                return;
            };
            let mut s = settings.borrow_mut();
            s.enabled = app.get_enabled();
            s.input_method = match app.get_method() {
                1 => InputMethod::Vni,
                2 => InputMethod::SimpleTelex,
                _ => InputMethod::Telex,
            };
            s.hotkey = parse_hotkey(&app.get_hotkey()).unwrap_or(s.hotkey.clone());
            s.launch_at_login = app.get_launch_at_login();
            s.quick_start = app.get_quick_start();
            s.quick_telex = app.get_quick_telex();
            s.modern_orthography = app.get_modern_orthography();
            s.relaxed_coda = app.get_relaxed_coda();
            s.english_override = app.get_english_override();
            s.auto_capitalize = app.get_auto_capitalize();
            s.per_app_language = app.get_per_app_language();
            s.macro_enabled = app.get_macro_enabled();
            s.injection_mode = match app.get_injection_mode() {
                0 => InjectionMode::UnicodeHex,
                1 => InjectionMode::Clipboard,
                _ => InjectionMode::Auto,
            };
            s.rewrite_mode = match app.get_rewrite_mode() {
                1 => RewriteMode::SelectOverwrite,
                _ => RewriteMode::Backspace,
            };
            if let Err(e) = s.save(&settings_path) {
                eprintln!("[uvie-ui] save settings: {e}");
            }
            if let Err(e) = macros.borrow().save(&macros_path) {
                eprintln!("[uvie-ui] save macros: {e}");
            }
        });
    }

    app.run()
}

fn push_macros(app: &AppWindow, table: &MacroTable) {
    let rows: Vec<MacroRow> = table
        .entries
        .iter()
        .map(|e| MacroRow {
            trigger: SharedString::from(e.trigger.as_str()),
            expansion: SharedString::from(e.expansion.as_str()),
        })
        .collect();
    app.set_macros(ModelRc::new(VecModel::from(rows)));
}

fn refresh_excluded(app: &slint::Weak<AppWindow>, s: &Settings) {
    if let Some(app) = app.upgrade() {
        app.set_excluded_apps(ModelRc::new(VecModel::from(
            s.excluded_apps
                .iter()
                .map(|a| SharedString::from(a.as_str()))
                .collect::<Vec<_>>(),
        )));
    }
}
