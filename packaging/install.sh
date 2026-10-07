#!/usr/bin/env bash
# UVie Linux — build + install helper.
#
#   ./packaging/install.sh           # build + install everything found
#   ./packaging/install.sh --daemon  # daemon + tray only (no IBus/Fcitx5)
#
set -euo pipefail
cd "$(dirname "$0")/.."

WITH_IM=1
[ "${1:-}" = "--daemon" ] && WITH_IM=0

echo "==> cargo build --release"
cargo build --release --workspace

sudo install -Dm755 target/release/uvie-inputd /usr/local/bin/uvie-inputd
sudo install -Dm755 target/release/uvie-ui /usr/local/bin/uvie-ui
sudo install -Dm644 packaging/uvie-ui.desktop \
    /usr/local/share/applications/uvie-ui.desktop

# udev rule + input group (daemon needs both)
sudo install -Dm644 packaging/99-uvie.rules /etc/udev/rules.d/99-uvie.rules
sudo udevadm control --reload-rules || true
sudo udevadm trigger || true
sudo usermod -aG input "$USER" || true

install -Dm644 packaging/uvie-inputd.service \
    "$HOME/.config/systemd/user/uvie-inputd.service"
systemctl --user daemon-reload || true

if [ "$WITH_IM" = 1 ]; then
    echo "==> ibus-uvie"
    cmake -S ibus -B build-ibus -DCMAKE_BUILD_TYPE=Release
    cmake --build build-ibus
    sudo cmake --install build-ibus

    echo "==> fcitx5-uvie"
    cmake -S fcitx5 -B build-fcitx5 -DCMAKE_BUILD_TYPE=Release
    cmake --build build-fcitx5
    sudo cmake --install build-fcitx5
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
