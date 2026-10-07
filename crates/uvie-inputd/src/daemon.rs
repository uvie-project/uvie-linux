//! The daemon loop: raw evdev events in, dispatch decisions out.
//!
//! One thread owns everything — the `uvie` engine session (via
//! `uvie_core::Dispatcher`), the `uinput` injector, and the xkb state —
//! so ordering is trivially correct. Reader threads and the tray feed in
//! through a single channel.

use std::collections::HashSet;
use std::process;
use std::sync::mpsc::{Receiver, Sender};
use std::sync::{Arc, Mutex};
use std::time::Duration;

use evdev::{EventType, InputEvent};

const EV_KEY: u16 = 0x01;

use uvie_core::dispatcher::{Context, Dispatch, Dispatcher, InputLanguage};
use uvie_core::engine_session::{EngineOptions, EngineSession};
use uvie_core::settings::{LanguagePref, Settings};
use uvie_core::{KeyEvent, LanguageMemory, MacroTable};

use crate::config::{self, FileWatch};
use crate::devices::{DevMsg, DevicePool};
use crate::focus::Focus;
use crate::inject::Injector;
use crate::keymap::Keymap;
use crate::keys_linux;
use crate::tray::{TrayCmd, TrayShared};

/// Everything the daemon loop consumes.
pub enum Msg {
    Dev(DevMsg),
    Tray(TrayCmd),
}

const HOUSEKEEPING_INTERVAL: Duration = Duration::from_millis(500);
const HOTPLUG_INTERVAL: u32 = 4; // every 4th housekeeping tick ≈ 2 s

pub struct Daemon {
    dispatcher: Dispatcher,
    settings: Settings,
    injector: Injector,
    keymap: Keymap,
    focus: Focus,
    pool: DevicePool,
    /// Physical keys whose press we consumed — their release/repeat must be
    /// swallowed too so apps never see a dangling release.
    consumed: HashSet<(usize, u16)>,
    /// Currently-held physical modifier key codes.
    pressed_mods: HashSet<u16>,
    current_app: Option<String>,
    memory: LanguageMemory,
    watch_settings: FileWatch,
    watch_macros: FileWatch,
    tray_shared: Arc<Mutex<TrayShared>>,
    housekeeping_tick: u32,
}

impl Daemon {
    pub fn new(tx: Sender<Msg>, tray_shared: Arc<Mutex<TrayShared>>) -> std::io::Result<Self> {
        let settings = Settings::load(&config::settings_path());
        let macros = MacroTable::load(&config::macros_path());
        let memory = LanguageMemory::load(&config::memory_path());

        let Some(keymap) = Keymap::new() else {
            return Err(std::io::Error::other("could not compile an xkb keymap"));
        };
        let injector = Injector::new(&keymap, settings.injection_mode, settings.rewrite_mode)?;
        let mut dispatcher = Dispatcher::new(EngineSession::with_options(
            settings.input_method,
            engine_options(&settings),
        ));
        dispatcher.macro_enabled = settings.macro_enabled;
        dispatcher.auto_capitalize = settings.auto_capitalize;
        dispatcher.macros = macros;

        Ok(Self {
            dispatcher,
            settings,
            injector,
            keymap,
            focus: Focus::connect(),
            pool: DevicePool::new(tx),
            consumed: HashSet::new(),
            pressed_mods: HashSet::new(),
            current_app: None,
            memory,
            watch_settings: FileWatch::new(config::settings_path()),
            watch_macros: FileWatch::new(config::macros_path()),
            tray_shared,
            housekeeping_tick: 0,
        })
    }

    pub fn run(mut self, rx: Receiver<Msg>) -> ! {
        let grabbed = self.pool.rescan();
        if grabbed == 0 {
            eprintln!(
                "[uvie-inputd] no keyboards grabbed — add yourself to the `input` \
                 group and install packaging/99-uvie.rules for /dev/uinput access"
            );
        }

        loop {
            // Poll sooner while a clipboard-paste burst is pending so its
            // ~150ms quiet-time flush lands on schedule.
            let interval = if self.injector.has_pending() {
                Duration::from_millis(50)
            } else {
                HOUSEKEEPING_INTERVAL
            };
            match rx.recv_timeout(interval) {
                Ok(Msg::Dev(DevMsg::Event(ev))) => self.on_event(ev.dev, &ev.event),
                Ok(Msg::Dev(DevMsg::Gone(id))) => {
                    self.pool.remove(id);
                    self.consumed.retain(|(d, _)| *d != id);
                }
                Ok(Msg::Tray(cmd)) => {
                    if self.on_tray(cmd) {
                        process::exit(0);
                    }
                }
                Err(_) => self.housekeeping(),
            }
        }
    }

    // ------------------------------------------------------------------
    // Housekeeping: hotplug rescan, config reload, autostart file.
    // ------------------------------------------------------------------

    fn housekeeping(&mut self) {
        self.injector.flush_due();
        self.housekeeping_tick += 1;
        if self.housekeeping_tick % HOTPLUG_INTERVAL == 0 {
            self.pool.rescan();
        }
        if self.watch_settings.changed() {
            self.reload_settings();
        }
        if self.watch_macros.changed() {
            self.dispatcher.macros = MacroTable::load(&config::macros_path());
        }
        let _ = autostart_sync(&self.settings);
    }

    fn reload_settings(&mut self) {
        let new = Settings::load(&config::settings_path());
        self.dispatcher.session.set_input_method(new.input_method);
        self.dispatcher.session.apply_options(engine_options(&new));
        self.dispatcher.macro_enabled = new.macro_enabled;
        self.dispatcher.auto_capitalize = new.auto_capitalize;
        self.injector
            .configure(&self.keymap, new.injection_mode, new.rewrite_mode);
        self.settings = new;
        self.refresh_tray();
    }

    // ------------------------------------------------------------------
    // Event path
    // ------------------------------------------------------------------

    fn on_event(&mut self, dev: usize, ev: &InputEvent) {
        if ev.event_type() != EventType::KEY {
            // SYN/Misc/Rel/Abs events ride through untouched — ordering is
            // preserved because all events funnel through this one loop.
            self.injector.forward(ev);
            return;
        }
        match ev.value() {
            0 => self.on_release(dev, ev.code()),
            1 => self.on_press(dev, ev.code()),
            // Key repeat: forwarded passes repeat through; consumed keys
            // re-run dispatch so held letters keep feeding the engine —
            // matches uvie-win where a held key repeats feed() via the hook.
            2 if self.consumed.contains(&(dev, ev.code())) => {
                self.dispatch_key(dev, ev.code(), true)
            }
            2 => self.injector.forward(ev),
            _ => self.injector.forward(ev),
        }
    }

    fn on_release(&mut self, dev: usize, code: u16) {
        self.keymap.update(code, false);
        self.pressed_mods.remove(&code);
        if self.consumed.remove(&(dev, code)) {
            // The press was swallowed; the release must be too.
            return;
        }
        self.injector.emit_raw(EV_KEY, code, 0);
    }

    fn on_press(&mut self, dev: usize, code: u16) {
        // Update modifier/keymap state for this key before dispatching —
        // letters resolve against the already-held modifiers, so ordering
        // only matters for the modifiers themselves (which produce no char).
        self.keymap.update(code, true);
        if is_modifier(code) {
            self.pressed_mods.insert(code);
        }

        // Toggle hotkey chord.
        if keys_linux::chord_matches(
            &self.settings.hotkey,
            code,
            self.ctrl(),
            self.alt(),
            self.shift(),
        ) {
            self.toggle_language();
            self.consumed.insert((dev, code));
            return;
        }

        self.dispatch_key(dev, code, false);
    }

    fn dispatch_key(&mut self, dev: usize, code: u16, is_repeat: bool) {
        if !self.settings.enabled {
            self.consumed.remove(&(dev, code));
            self.forward_press(dev, code, is_repeat);
            return;
        }

        self.track_focus();

        let kind = keys_linux::classify(code, self.keymap.char_for(code));
        let key = KeyEvent {
            kind,
            ctrl: self.ctrl(),
            alt: self.alt(),
            injected: false,
        };
        let ctx = Context {
            language: None,
            app_excluded: self
                .current_app
                .as_deref()
                .map(|a| self.settings.is_excluded(a))
                .unwrap_or(false),
            non_latin_layout: false,
        };
        match self.dispatcher.handle(key, &ctx) {
            Dispatch::Pass(plan) => {
                self.injector.apply_plan(&plan);
                self.forward_press(dev, code, is_repeat);
            }
            Dispatch::Consume(plan) => {
                self.injector.apply_plan(&plan);
                if !is_repeat {
                    self.consumed.insert((dev, code));
                }
            }
        }
    }

    /// Forward a press or repeat event for a key that was not consumed.
    fn forward_press(&mut self, _dev: usize, code: u16, is_repeat: bool) {
        self.injector
            .emit_raw(EV_KEY, code, if is_repeat { 2 } else { 1 });
    }

    // ------------------------------------------------------------------
    // Per-app state
    // ------------------------------------------------------------------

    /// Refresh the focused-app identity (X11 only) and swap in the
    /// remembered language when the focus actually changed.
    fn track_focus(&mut self) {
        let app = self.focus.active_app().map(|s| s.to_string());
        if app == self.current_app {
            return;
        }
        if let Some(prev) = self.current_app.take() {
            if self.settings.per_app_language {
                self.memory.remember(
                    &prev,
                    match self.dispatcher.language {
                        InputLanguage::Vietnamese => LanguagePref::Vietnamese,
                        InputLanguage::English => LanguagePref::English,
                    },
                );
                let _ = self.memory.save(&config::memory_path());
            }
        }
        self.current_app = app.clone();
        self.injector.set_chromium_app(
            app.as_deref()
                .map(|a| self.settings.is_chromium(a))
                .unwrap_or(false),
        );
        if self.settings.per_app_language {
            if let Some(app) = app.as_deref() {
                if let Some(lang) = self.memory.recall(app) {
                    self.dispatcher.set_language(match lang {
                        LanguagePref::Vietnamese => InputLanguage::Vietnamese,
                        LanguagePref::English => InputLanguage::English,
                    });
                    self.refresh_tray();
                }
            }
        }
        // A focus change invalidates the composing word entirely.
        self.dispatcher.session.reset();
    }

    // ------------------------------------------------------------------
    // Modifier tracking + tray commands
    // ------------------------------------------------------------------

    fn ctrl(&self) -> bool {
        self.pressed_mods
            .iter()
            .any(|c| keys_linux::CTRL_KEYS.contains(c))
    }

    fn alt(&self) -> bool {
        self.pressed_mods
            .iter()
            .any(|c| keys_linux::ALT_KEYS.contains(c))
    }

    fn shift(&self) -> bool {
        self.pressed_mods
            .iter()
            .any(|c| keys_linux::SHIFT_KEYS.contains(c))
    }

    fn toggle_language(&mut self) {
        let next = match self.dispatcher.language {
            InputLanguage::Vietnamese => InputLanguage::English,
            InputLanguage::English => InputLanguage::Vietnamese,
        };
        self.dispatcher.set_language(next);
        self.refresh_tray();
    }

    fn on_tray(&mut self, cmd: TrayCmd) -> bool {
        match cmd {
            TrayCmd::ToggleEnabled => {
                self.settings.enabled = !self.settings.enabled;
                let _ = self.settings.save(&config::settings_path());
                self.dispatcher.session.reset();
            }
            TrayCmd::ToggleLanguage => self.toggle_language(),
            TrayCmd::SetMethod(m) => {
                self.settings.input_method = m;
                let _ = self.settings.save(&config::settings_path());
                self.dispatcher.session.set_input_method(m);
            }
            TrayCmd::OpenSettings => {
                let exe = std::env::current_exe()
                    .ok()
                    .and_then(|p| p.parent().map(|d| d.join("uvie-ui")))
                    .filter(|p| p.exists())
                    .unwrap_or_else(|| "uvie-ui".into());
                let _ = std::process::Command::new(exe).spawn();
            }
            TrayCmd::Quit => {
                let _ = self.memory.save(&config::memory_path());
                return true;
            }
        }
        self.refresh_tray();
        false
    }

    fn refresh_tray(&mut self) {
        if let Ok(mut shared) = self.tray_shared.lock() {
            shared.enabled = self.settings.enabled;
            shared.language = self.dispatcher.language;
            shared.method = self.dispatcher.session.input_method();
        }
    }
}

fn is_modifier(code: u16) -> bool {
    keys_linux::CTRL_KEYS.contains(&code)
        || keys_linux::ALT_KEYS.contains(&code)
        || keys_linux::SHIFT_KEYS.contains(&code)
        || keys_linux::META_KEYS.contains(&code)
        || code == keys_linux::KEY_CAPSLOCK
}

fn engine_options(s: &Settings) -> EngineOptions {
    EngineOptions {
        quick_start: s.quick_start,
        quick_telex: s.quick_telex,
        modern_orthography: s.modern_orthography,
        relaxed_coda: s.relaxed_coda,
        english_override: s.english_override,
    }
}

fn autostart_sync(settings: &Settings) -> std::io::Result<()> {
    crate::autostart::sync(settings.launch_at_login, &config::autostart_path())
}
