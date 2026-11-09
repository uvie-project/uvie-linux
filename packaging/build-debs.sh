#!/usr/bin/env bash
# Build the uvie .deb packages with plain dpkg-deb (no debhelper).
# Expects: cargo build --release already done, engines built via cmake.
set -euo pipefail
cd "$(dirname "$0")/.."

# Version comes from the tag (or the latest tag on a manual dispatch).
# Strip the v prefix - Debian versions must start with a digit.
case "${1:-}" in
    v*|[0-9]*) VER="${1#v}" ;;
    *) VER="$(git describe --tags --abbrev=0)" ;;
esac
VER="${VER#v}"
ARCH=amd64
OUT=dist/debs
ROOT=dist/root
MAINTAINER="UVie Project <xuanthu404@gmail.com>"

rm -rf "$OUT" "$ROOT"
mkdir -p "$OUT" "$ROOT"

mkdeb() { # name depends recommends desc rootdir
    local name="$1" depends="$2" recommends="$3" desc="$4" root="$5"
    mkdir -p "$root/DEBIAN"
    cat > "$root/DEBIAN/control" <<EOF
Package: $name
Version: $VER
Architecture: $ARCH
Maintainer: $MAINTAINER
Section: utils
Priority: optional
Description: $desc
EOF
    if [ -n "$depends" ]; then
        sed -i "/^Section:/i Depends: $depends" "$root/DEBIAN/control"
    fi
    if [ -n "$recommends" ]; then
        sed -i "/^Section:/i Recommends: $recommends" "$root/DEBIAN/control"
    fi
    dpkg-deb --root-owner-group --build "$root" "$OUT/${name}_${VER}_${ARCH}.deb"
}

# ---- libuvie-ffi ------------------------------------------------------
r=$ROOT/libuvie-ffi
mkdir -p "$r/usr/lib"
cp target/release/libuvie_ffi.so "$r/usr/lib/"
mkdir -p "$r/DEBIAN"
cat > "$r/DEBIAN/postinst" <<'EOF'
#!/bin/sh
ldconfig
EOF
chmod +x "$r/DEBIAN/postinst"
mkdeb libuvie-ffi "" "" "UVie Vietnamese input engine - shared FFI library" "$r"

# ---- uvie-ui ----------------------------------------------------------
r=$ROOT/uvie-ui
mkdir -p "$r/usr/bin" "$r/usr/share/applications" \
    "$r/usr/share/icons/hicolor/256x256/apps"
cp target/release/uvie-ui "$r/usr/bin/"
cp packaging/uvie-ui.desktop "$r/usr/share/applications/"
cp assets/logo.png "$r/usr/share/icons/hicolor/256x256/apps/uvie.png"
mkdeb uvie-ui "" "" "UVie Vietnamese IME - settings window" "$r"

# ---- uvie-inputd ------------------------------------------------------
r=$ROOT/uvie-inputd
mkdir -p "$r/usr/bin" "$r/usr/lib/udev/rules.d" "$r/usr/lib/systemd/user"
cp target/release/uvie-inputd "$r/usr/bin/"
cp packaging/99-uvie.rules "$r/usr/lib/udev/rules.d/"
cp packaging/uvie-inputd.service "$r/usr/lib/systemd/user/"
mkdir -p "$r/DEBIAN"
cat > "$r/DEBIAN/postinst" <<'EOF'
#!/bin/sh
udevadm control --reload-rules 2>/dev/null || true
echo "uvie-inputd: enable with: systemctl --user enable --now uvie-inputd"
EOF
chmod +x "$r/DEBIAN/postinst"
mkdeb uvie-inputd "" "uvie-ui" "UVie Vietnamese IME - evdev input daemon (standalone)" "$r"

# ---- ibus-uvie --------------------------------------------------------
r=$ROOT/ibus-uvie
mkdir -p "$r/usr/lib/ibus" "$r/usr/share/ibus/component"
cp build-ibus/ibus-engine-uvie "$r/usr/lib/ibus/"
cp build-ibus/uvie.xml "$r/usr/share/ibus/component/"
mkdeb ibus-uvie "libuvie-ffi, ibus (>= 1.5)" "uvie-ui" "UVie Vietnamese IME - IBus engine" "$r"

# ---- fcitx5-uvie ------------------------------------------------------
r=$ROOT/fcitx5-uvie
mkdir -p "$r/usr/lib/fcitx5" "$r/usr/share/fcitx5/addon" \
    "$r/usr/share/fcitx5/inputmethod"
cp build-fcitx5/src/libuvie.so "$r/usr/lib/fcitx5/"
cp build-fcitx5/src/uvie-addon.conf "$r/usr/share/fcitx5/addon/uvie.conf"
cp build-fcitx5/src/uvie.conf "$r/usr/share/fcitx5/inputmethod/uvie.conf"
mkdeb fcitx5-uvie "libuvie-ffi, fcitx5" "uvie-ui" "UVie Vietnamese IME - Fcitx5 addon" "$r"

# ---- uvie (metapackage: standalone daemon path) ------------------------
r=$ROOT/uvie
mkdir -p "$r/usr/share/doc/uvie"
mkdeb uvie "uvie-inputd, uvie-ui" "" "UVie Vietnamese IME for Linux (daemon + settings UI)" "$r"

echo "Built debs in $OUT:"
ls -l "$OUT"
