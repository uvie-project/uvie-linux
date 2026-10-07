# UVie for Linux

Bộ gõ tiếng Việt (Telex / VNI / Simple Telex) cho Linux — X11 **và** mọi
Wayland compositor, kể cả GNOME. Same `uvie` Rust engine as
[uvie-mac](https://github.com/uvie-project/uvie-mac) and
[uvie-win](https://github.com/uvie-project/uvie-win).

## Kiến trúc

| Thành phần | Vai trò | Nền tảng |
|---|---|---|
| `uvie-core` | Engine session, dispatch, settings, macros — platform-free Rust | shared |
| `uvie-inputd` | Daemon: evdev `EVIOCGRAB` trên bàn phím vật lý → `uvie` engine → `uinput` virtual keyboard. Tray (StatusNotifierItem), hot-reload config | **mọi** X11 + Wayland |
| `uvie-ui` | Cửa sổ cài đặt (Slint) — ghi `~/.config/uvie/*.json` | X11 + Wayland |
| `ibus-uvie` | IBus engine (C + libuvie FFI) — preedit thật trên GNOME | IBus |
| `fcitx5-uvie` | Fcitx5 addon (C++ + libuvie FFI) | Fcitx5 |

`uvie-inputd` là con đường phủ sóng 100%: nó hoạt động ở tầng kernel input
nên không phụ thuộc Wayland protocol nào (GNOME không hỗ trợ
`input-method` v1/v2). Hệ quả đã chấp nhận: không có preedit/underline —
text được viết lại ngay khi gõ (giống ibus-bamboo forward mode). Muốn
preedit thật thì dùng `ibus-uvie` (GNOME/IBus) hoặc `fcitx5-uvie` (KDE …).

Unicode injection: uinput chỉ phát được key *codes*, nên ký tự có dấu đi
qua ASCII fast-path → `Ctrl+Shift+U` hex → clipboard-paste (phục hồi
clipboard sau 400ms). Tuỳ chọn trong Cài đặt → Nâng cao.

## Cài đặt

```bash
# gói build sẵn (release page)
tar -xzf uvie-for-linux-x86_64.tar.gz
cd uvie-for-linux-x86_64
./install.sh            # everything
./install.sh --daemon   # chỉ daemon + UI
```

## Build từ source

```bash
# deps (Ubuntu/Debian)
sudo apt install build-essential cargo pkg-config cmake gettext \
    libxkbcommon-dev libxcb1-dev libfontconfig1-dev libudev-dev \
    libibus-1.0-dev fcitx5-modules-dev libfcitx5core-dev

./packaging/install.sh            # everything
./packaging/install.sh --daemon   # chỉ daemon + UI
```

## Run

```bash
sudo usermod -aG input $USER      # một lần, rồi log out/in
systemctl --user enable --now uvie-inputd   # hoặc chạy thẳng `uvie-inputd`
```

Config: `~/.config/uvie/{settings,macros,memory}.json` — sửa bằng `uvie-ui`
hoặc tray icon (GNOME cần extension AppIndicator). Daemon tự nạp lại khi
file đổi.

## Đã biết

- Session Wayland không có per-app language memory / exclusion list
  (compositor giấu window identity; X11 vẫn có qua `WM_CLASS`).
- App chạy quyền cao hơn daemon (root, gamescope) sẽ không qua được grab —
  giống giới hạn "elevated process" của uvie-win.
- Macro chỉ trên đường `uvie-inputd`; các engine IBus/Fcitx5 chưa đọc
  `macros.json`.

## Repo layout

```
crates/uvie-core   shared session/dispatch/settings (unit-tested)
crates/uvie-inputd evdev grab → engine → uinput daemon + SNI tray
crates/uvie-ui     Slint settings window
crates/uvie-ffi    libuvie_ffi.{so,a} — re-export uvie::ffi for C/C++
include/uvie.h     C header for the two framework engines
ibus/              IBus engine + component XML
fcitx5/            Fcitx5 addon + .conf files
packaging/         udev rule, systemd user unit, .desktop, install.sh,
                   install-prebuilt.sh (release tarball)
```
