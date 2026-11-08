//! Linux settings-schema tests. Dispatch, engine, macro and per-app memory
//! tests live in the shared `uvie-host` crate.

use crate::settings::Settings;
use uvie_host::InputMethod;

#[test]
fn settings_json_roundtrip() {
    let s = Settings {
        input_method: InputMethod::Vni,
        quick_telex: true,
        excluded_apps: vec!["notepad".into()],
        macro_enabled: true,
        chromium_apps: vec!["mybrowser".into()],
        ..Default::default()
    };
    let text = serde_json::to_string(&s).unwrap();
    let back: Settings = serde_json::from_str(&text).unwrap();
    assert_eq!(back, s);
    assert!(back.is_excluded("NOTEPAD"));
}

#[test]
fn settings_defaults_ship_chromium_list() {
    // Mirrors uvie-mac's `defaultChromiumBrowsers`: a fresh profile knows
    // the mainstream Chromium browsers and gets the overwrite workaround.
    // (Linux process names, not .exe - matched against WM_CLASS on X11.)
    let s = Settings::default();
    for exe in ["google-chrome", "microsoft-edge", "brave", "arc"] {
        assert!(s.is_chromium(exe), "{exe} should default to chromium list");
    }
    assert!(!s.is_chromium("notepad"));
    assert!(!s.is_chromium("code"));
    // Case-insensitive, like is_excluded.
    let s = Settings {
        chromium_apps: vec!["MyBrowser".into()],
        ..Default::default()
    };
    assert!(s.is_chromium("mybrowser"));
}
