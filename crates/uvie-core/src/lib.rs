//! uvie-core — Linux facade over the shared `uvie-host` crate plus the
//! Linux-specific settings schema and hotkey string round-trips.
//!
//! Dispatch, engine sessions, macros and per-app memory live in `uvie-host`
//! (same repo/tag as the `uvie` engine). This crate re-exports them under the
//! historical module paths `uvie-inputd` and `uvie-ui` already use, and owns
//! `settings` (evdev key codes, injection/rewrite modes) and `hotkey`.

pub use uvie_host::{dispatcher, engine_session, keys, macros, memory};
pub mod hotkey;
pub mod settings;

pub use dispatcher::{Context, Dispatch, Dispatcher, InputAction, InputLanguage};
pub use engine_session::{EngineOptions, EngineSession};
pub use hotkey::{format_hotkey, key_name, parse_hotkey, parse_key_name};
pub use keys::{KeyEvent, KeyKind};
pub use macros::{MacroEntry, MacroTable};
pub use memory::LanguageMemory;
pub use settings::{Hotkey, InjectionMode, LanguagePref, RewriteMode, Settings};
pub use uvie_host::InputMethod;

#[cfg(test)]
mod tests;
