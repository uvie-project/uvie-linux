# AGENTS.md — notes for coding agents working on uvie-linux

## Build quick reference

```bash
cargo build                            # all 4 crates
cargo test -p uvie-core                # headless tests
cargo clippy --workspace --all-targets -- -D warnings
cargo fmt --all --check

cargo build --release -p uvie-ffi      # libuvie_ffi.{so,a} for the engines
cmake -S ibus -B build-ibus && cmake --build build-ibus
cmake -S fcitx5 -B build-fcitx5 && cmake --build build-fcitx5
```

System deps: `pkg-config libxkbcommon-dev libxcb1-dev libfontconfig1-dev
libudev-dev libibus-1.0-dev fcitx5-modules-dev libfcitx5core-dev gettext`.

## Architecture

- `uvie-core` is a port of uvie-win's `uvie-core`: engine session +
  dispatcher (Pass/Consume plans of `InputAction`s) + settings/macros/
  memory. Keep it free of Linux/display-server calls so it stays
  unit-testable headlessly.
- `uvie-inputd` owns the event path: evdev reader threads → one daemon
  loop → `uinput` virtual keyboard. Raw evdev codes everywhere; xkb
  keycode = evdev code + 8. Letters follow QWERTY row order
  (Q-P=16-25, A-L=30-38, Z-M=44-50), NOT alphabet order.
- uinput emits key *codes* only. Accented text: ASCII via xkb reverse map,
  else Ctrl+Shift+U hex, else clipboard-paste (`InjectionMode` setting).
  `RewriteMode::SelectOverwrite` (Shift+Left then type over) is forced for
  `chromium_apps` — same workaround as uvie-mac.
- The daemon "consumes" keys: a pressed key's release/repeat is swallowed
  too (`consumed` set) or apps see a dangling release.
- `ibus-uvie` + `fcitx5-uvie` are thin C/C++ shells over `include/uvie.h`
  → `target/release/libuvie_ffi.so`. They keep a `preedit` buffer mirroring
  engine output since last commit; diffs (backspaces, suffix) apply to
  that buffer, real text commits on word boundaries. They share
  `uvie-config.h`, a dumb JSON-substring reader of the same
  `~/.config/uvie/settings.json`.
- Settings live in `~/.config/uvie/settings.json` (serde_json pretty);
  `uvie-inputd` hot-reloads on mtime change; `uvie-ui` is the writer.

## Hard-won constraints

- **evdev 0.12 has no `uinput` feature** — we drive `/dev/uinput` directly
  in `vdev.rs` (ioctls UI_SET_EVBIT/UI_SET_KEYBIT/UI_DEV_SETUP + write()).
- **ksni is pulled by Slint with the `async-io` backend** — uvie-inputd
  must request `default-features=false, features=["async-io"]`; enabling
  `tokio` there hard-errors at compile time.
- `evdev::InputEvent::event_type()` returns an `EventType` newtype — use
  `.0` for the raw u16.
- xcb `Window` has no `none()`; compare `resource_id() == 0`
  (`use xcb::Xid`).
- `add_fcitx5_addon` exists only inside fcitx5's source tree; we use plain
  `add_library(MODULE)`.
- `G_DECLARE_FINAL_TYPE` for IBusEngine needs a manual
  `G_DEFINE_AUTOPTR_CLEANUP_FUNC(IBusEngine, g_object_unref)` — ibus 1.5
  headers ship no autoptr for IBusEngine.
- In fcitx5 `UvieEngine` (C++ class) collides with `UvieEngine` (C opaque
  from uvie.h) — use `::UvieEngine` for the FFI handle.
- Grab = exclusive. uvie-inputd sees real keyboards only because other
  readers keep their fds; injected keys come from our own device
  (`VIRTUAL_DEVICE_NAME`) which is skipped during rescan.
- Wayland hides focused-window identity — `per_app_language` and
  `excluded_apps`/`chromium_apps` only work on X11 (WM_CLASS via xcb).
- `gh pr merge` is blocked — humans merge PRs.

## CI

`.github/workflows/ci.yml` on ubuntu-latest: fmt, clippy `-D warnings`,
`cargo test --workspace`, `cargo build --release --workspace`, then both
cmake engine builds; uploads binaries as artifacts.
