#!/usr/bin/env bash
# Publish .deb files into a reprepro apt repository tree.
# Usage: publish-apt.sh <repo-dir> <key-id> <deb>...
set -euo pipefail

REPO="$1"
KEYID="$2"
shift 2

mkdir -p "$REPO/conf"
if [ ! -f "$REPO/conf/distributions" ]; then
    cat > "$REPO/conf/distributions" <<EOF
Origin: UVie Project
Label: UVie
Suite: stable
Codename: stable
Architectures: amd64
Components: main
Description: UVie Vietnamese IME for Linux
SignWith: $KEYID
EOF
fi

for deb in "$@"; do
    reprepro -b "$REPO" includedeb stable "$deb"
done

# Public key for users' signed-by= keyring.
cp "$(dirname "$0")/apt-repo.gpg" "$REPO/uvie-repo-key.asc" 2>/dev/null || true
