//! Re-exports the uvie engine's C ABI (`uvie_engine_*` in `uvie::ffi`) as a
//! shared (`libuvie_ffi.so`) and static (`libuvie_ffi.a`) library, so the
//! C++ frontends — `ibus-uvie` and `fcitx5-uvie` — can link the engine
//! without a Rust toolchain in their build.
//!
//! `dispatch` adds `uvie_dispatch_*`: the shared `uvie-host` dispatcher
//! (same behavior layer as `uvie-inputd` and uvie-win) over the same ABI.

pub use uvie::ffi::*;

mod dispatch;
