//! Persistent user settings — the Linux port of uvie-mac's AppDefaults.
//! Stored as JSON at `~/.config/uvie/settings.json`; both `uvie-inputd`
//! (reader) and `uvie-ui` (writer) resolve the path the same way.

use serde::{Deserialize, Serialize};
use std::path::Path;
use uvie::InputMethod;

/// Vi/En preference remembered per app when `per_app_language` is on.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "lowercase")]
pub enum LanguagePref {
    Vietnamese,
    English,
}

/// Global keyboard shortcut binding (e.g. the Vi/En toggle).
/// `key` is an evdev key code (e.g. KEY_Z = 44, see input-event-codes.h);
/// serialized as e.g. `"Ctrl+Shift+Z"` by the host.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct Hotkey {
    pub ctrl: bool,
    pub alt: bool,
    pub shift: bool,
    /// Linux evdev key code (KEY_* constants).
    pub key: u16,
}

impl Default for Hotkey {
    fn default() -> Self {
        // Ctrl+Shift+Z — same default spirit as uvie-mac's toggle chord.
        // evdev KEY_Z = 44.
        Self {
            ctrl: true,
            alt: false,
            shift: true,
            key: 44,
        }
    }
}

/// How non-ASCII text (Vietnamese accented output) is typed on screen.
/// uinput can only emit key *codes*, not characters, and the active keymap
/// belongs to the display server — accented chars need an indirect path.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum InjectionMode {
    /// Ctrl+Shift+U hex sequence (GTK/Qt/IBus-aware apps — the common case).
    UnicodeHex,
    /// Set the clipboard then paste (Ctrl+V) — fallback for apps that do
    /// not understand the unicode sequence (restores the clipboard after).
    Clipboard,
    /// ASCII chars via keycodes, unicode via the hex sequence (default).
    #[default]
    Auto,
}

/// How deletions inside a composing word are emitted.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum RewriteMode {
    /// Send N Backspace key events (default).
    #[default]
    Backspace,
    /// Select N chars (Shift+Left) then type over them — workaround for
    /// Chromium-based fields that drop/reorder synthetic backspaces, the
    /// same trick uvie-mac applies to Chromium browsers.
    SelectOverwrite,
}

#[derive(Clone, PartialEq, Serialize, Deserialize)]
#[serde(default)]
pub struct Settings {
    /// Master on/off switch for the IME.
    pub enabled: bool,
    /// Telex or VNI.
    #[serde(with = "input_method_serde")]
    pub input_method: InputMethod,
    /// `j`→`gi`, `f`→`ph`, `w`→`qu` on word start.
    pub quick_start: bool,
    /// `cc`→`ch` style shorthand.
    pub quick_telex: bool,
    /// Modern orthography tone placement.
    pub modern_orthography: bool,
    /// Relaxed coda rules.
    pub relaxed_coda: bool,
    /// English dictionary override (real English words pass through).
    pub english_override: bool,
    /// Remember Vi/En choice per application (X11 only — Wayland does not
    /// expose focused-window identity).
    pub per_app_language: bool,
    /// Capitalize first letter of sentences.
    pub auto_capitalize: bool,
    /// Launch the daemon with the session (XDG autostart).
    pub launch_at_login: bool,
    /// App identifiers the IME ignores entirely — matched case-insensitively
    /// against process name / X11 WM_CLASS (e.g. `chromium`, `code`).
    pub excluded_apps: Vec<String>,
    /// Chromium-based apps whose text fields misbehave with plain synthetic
    /// backspaces (omnibox drops/reorders them). For these the injector uses
    /// `rewrite_mode` when set — the same workaround uvie-mac applies to
    /// Chromium browsers.
    pub chromium_apps: Vec<String>,
    /// Text macros: expand an abbreviation on Space/Enter.
    pub macro_enabled: bool,
    /// Vi/En toggle hotkey.
    pub hotkey: Hotkey,
    /// How accented text is injected (see `InjectionMode`).
    pub injection_mode: InjectionMode,
    /// How rewrites erase characters (see `RewriteMode`).
    pub rewrite_mode: RewriteMode,
}

/// Chromium apps (Linux process / WM_CLASS names, lowercase) that get the
/// select-and-overwrite workaround by default — the Linux counterpart of
/// `AppDefaults.chromiumBrowsers` in uvie-mac.
pub const DEFAULT_CHROMIUM_APPS: &[&str] = &[
    "google-chrome",
    "chromium",
    "chromium-browser",
    "brave",
    "brave-browser",
    "vivaldi",
    "vivaldi-bin",
    "microsoft-edge",
    "opera",
    "thorium",
    "arc",
];

impl Default for Settings {
    fn default() -> Self {
        Self {
            enabled: true,
            input_method: InputMethod::Telex,
            quick_start: false,
            quick_telex: false,
            modern_orthography: false,
            relaxed_coda: false,
            english_override: true,
            per_app_language: true,
            auto_capitalize: false,
            launch_at_login: false,
            excluded_apps: Vec::new(),
            chromium_apps: DEFAULT_CHROMIUM_APPS
                .iter()
                .map(|s| s.to_string())
                .collect(),
            macro_enabled: false,
            hotkey: Hotkey::default(),
            injection_mode: InjectionMode::Auto,
            rewrite_mode: RewriteMode::Backspace,
        }
    }
}

impl Settings {
    pub fn load(path: &Path) -> Self {
        match std::fs::read_to_string(path) {
            Ok(text) => serde_json::from_str(&text).unwrap_or_default(),
            Err(_) => Self::default(),
        }
    }

    pub fn save(&self, path: &Path) -> std::io::Result<()> {
        if let Some(dir) = path.parent() {
            std::fs::create_dir_all(dir)?;
        }
        let text = serde_json::to_string_pretty(self).map_err(std::io::Error::other)?;
        std::fs::write(path, text)
    }

    pub fn is_excluded(&self, app_name: &str) -> bool {
        self.excluded_apps
            .iter()
            .any(|e| e.eq_ignore_ascii_case(app_name))
    }

    pub fn is_chromium(&self, app_name: &str) -> bool {
        self.chromium_apps
            .iter()
            .any(|e| e.eq_ignore_ascii_case(app_name))
    }
}

impl std::fmt::Debug for Settings {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("Settings")
            .field("enabled", &self.enabled)
            .field("quick_start", &self.quick_start)
            .field("quick_telex", &self.quick_telex)
            .field("modern_orthography", &self.modern_orthography)
            .field("relaxed_coda", &self.relaxed_coda)
            .field("english_override", &self.english_override)
            .field("per_app_language", &self.per_app_language)
            .field("auto_capitalize", &self.auto_capitalize)
            .field("launch_at_login", &self.launch_at_login)
            .field("excluded_apps", &self.excluded_apps)
            .field("chromium_apps", &self.chromium_apps)
            .field("macro_enabled", &self.macro_enabled)
            .field("hotkey", &self.hotkey)
            .field("injection_mode", &self.injection_mode)
            .field("rewrite_mode", &self.rewrite_mode)
            .finish_non_exhaustive()
    }
}

// InputMethod isn't serde-aware in uvie-rs; serialize as its display name.
mod input_method_serde {
    use serde::{Deserialize, Deserializer, Serializer};
    use uvie::InputMethod;

    pub fn serialize<S: Serializer>(m: &InputMethod, s: S) -> Result<S::Ok, S::Error> {
        s.serialize_str(match m {
            InputMethod::Telex => "telex",
            InputMethod::Vni => "vni",
            InputMethod::SimpleTelex => "simple-telex",
        })
    }

    pub fn deserialize<'de, D: Deserializer<'de>>(d: D) -> Result<InputMethod, D::Error> {
        match <&str>::deserialize(d)?.to_ascii_lowercase().as_str() {
            "vni" => Ok(InputMethod::Vni),
            "simple-telex" | "simpletelex" | "simple" => Ok(InputMethod::SimpleTelex),
            _ => Ok(InputMethod::Telex),
        }
    }
}
