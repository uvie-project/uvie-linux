//! Re-exports the uvie engine's C ABI (`uvie_engine_*` in `uvie::ffi`) as a
//! shared (`libuvie_ffi.so`) and static (`libuvie_ffi.a`) library, so the
//! C++ frontends — `ibus-uvie` and `fcitx5-uvie` — can link the engine
//! without a Rust toolchain in their build.

pub use uvie::ffi::*;
