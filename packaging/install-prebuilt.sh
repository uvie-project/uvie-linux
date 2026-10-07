#!/usr/bin/env bash
# UVie for Linux — install the prebuilt release tarball.
# Run from inside the extracted uvie-for-linux-* directory:
#
#   ./install.sh           # everything found (daemon + UI + IBus + Fcitx5)
#   ./install.sh --daemon  # daemon + tray + settings UI only
#
set -euo pipefail
cd "$(dirname "$0")"

WITH_IM=1
[ "${1:-}" = "--daemon" ] && WITH_IM=0

sudo install -Dm755 bin/uvie-inputd /usr/local/bin/uvie-inputd
sudo install -Dm755 bin/uvie-ui /usr/local/bin/uvie-ui
sudo install -Dm644 share/applications/uvie-ui.desktop \
    /usr/local/share/applications/uvie-ui.desktop
sudo install -Dm644 share/icons/hicolor/256x256/apps/uvie.png \
    /usr/local/share/icons/hicolor/256x256/apps/uvie.png
sudo gtk-update-icon-cache /usr/local/share/icons/hicolor 2>/dev/null || true

# udev rule + input group (the daemon needs both for uinput/evdev)
sudo install -Dm644 lib/udev/rules.d/99-uvie.rules /etc/udev/rules.d/99-uvie.rules
sudo udevadm control --reload-rules || true
sudo udevadm trigger || true
sudo usermod -aG input "$USER" || true

install -Dm644 lib/systemd/user/uvie-inputd.service \
    "$HOME/.config/systemd/user/uvie-inputd.service"
systemctl --user daemon-reload || true

if [ "$WITH_IM" = 1 ]; then
    sudo install -Dm755 lib/libuvie_ffi.so /usr/local/lib/libuvie_ffi.so
    sudo ldconfig || true

    if [ -f lib/ibus/ibus-engine-uvie ]; then
        sudo install -Dm755 lib/ibus/ibus-engine-uvie \
            /usr/local/libexec/ibus-engine-uvie
        sudo install -Dm644 share/ibus/component/uvie.xml \
            /usr/local/share/ibus/component/uvie.xml
    fi

    if [ -f lib/fcitx5/libuvie.so ]; then
        sudo install -Dm755 lib/fcitx5/libuvie.so /usr/local/lib/fcitx5/libuvie.so
        sudo install -Dm644 etc/fcitx5/addon/uvie-addon.conf \
            /usr/local/share/fcitx5/addon/uvie-addon.conf
        sudo install -Dm644 etc/fcitx5/conf.d/uvie.conf \
            /usr/local/share/fcitx5/inputmethod/uvie.conf
    fi
fi

cat <<'EOF'

Done. Next steps:
  * log out/in once (for the new `input` group to apply)
  * systemctl --user enable --now uvie-inputd
  * or just run `uvie-inputd` in a terminal to try it first

IBus:  ibus restart, then Settings → Keyboard → Input Sources →
        Vietnamese → Vietnamese (UVie)
Fcitx5: restart fcitx5, add "UVie" to input methods.
EOF
