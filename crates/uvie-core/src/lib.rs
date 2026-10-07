//! uvie-core — platform-independent core of UVie for Linux.
//!
//! Mirrors uvie-mac's `Core/` + `Features/` layers (and is a direct port of
//! uvie-win's `uvie-core`): it owns the `uvie` engine session, decides how
//! each keystroke is dispatched, and holds the user settings / macros /
//! per-app memory that the input daemon (`uvie-inputd`) and the Slint
//! settings window (`uvie-ui`) share.
//!
//! The crate is intentionally free of Linux/display-server API calls so all
//! dispatch and typing behavior is unit-testable headlessly — the same split
//! that lets uvie-mac run `DispatcherTests`/`EngineTypingTests` headlessly.

pub mod dispatcher;
pub mod engine_session;
pub mod hotkey;
pub mod keys;
pub mod macros;
pub mod memory;
pub mod settings;

pub use dispatcher::{Context, Dispatch, Dispatcher, InputAction, InputLanguage};
pub use engine_session::{EngineOptions, EngineSession};
pub use hotkey::{format_hotkey, key_name, parse_hotkey, parse_key_name};
pub use keys::{KeyEvent, KeyKind};
pub use macros::{MacroEntry, MacroTable};
pub use memory::LanguageMemory;
pub use settings::{Hotkey, InjectionMode, LanguagePref, RewriteMode, Settings};
pub use uvie::InputMethod;

#[cfg(test)]
mod tests;
