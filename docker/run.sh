#!/usr/bin/env bash
# Build the uvie-linux dev image and run a command inside it.
#
# The repo is bind-mounted at /work. CARGO_TARGET_DIR and CARGO_HOME
# (registry + toolchain caches) live on named volumes so the host's
# target/ directory is never touched.
#
# Usage:
#   docker/run.sh                      # run the full check suite
#   docker/run.sh bash                 # open a shell
#   docker/run.sh cargo test --workspace
#   UVIE_DOCKER_PRIVILEGED=1 docker/run.sh ...   # pass --privileged (uinput e2e)
set -euo pipefail

IMAGE="uvie-linux-build:local"
DIR="$(cd "$(dirname "${BASH_SOURCE[0]}")" && pwd)"
REPO="$(cd "$DIR/.." && pwd)"

docker build -t "$IMAGE" -f "$DIR/Dockerfile" "$REPO"

RUN_ARGS=(-v "$REPO:/work")
if [[ "${UVIE_DOCKER_PRIVILEGED:-0}" == "1" ]]; then
    RUN_ARGS+=(--privileged)
fi

if [[ $# -eq 0 ]]; then
    set -- bash -c '
        set -e
        cargo fmt --all --check
        cargo clippy --workspace --all-targets -- -D warnings
        cargo test --workspace
        cargo build --release --workspace
        cmake -S ibus -B /cargo-target/build-ibus -DUVIE_FFI=/cargo-target/release
        cmake --build /cargo-target/build-ibus
        cmake -S fcitx5 -B /cargo-target/build-fcitx5 -DUVIE_FFI=/cargo-target/release
        cmake --build /cargo-target/build-fcitx5
    '
fi

exec docker run --rm \
    "${RUN_ARGS[@]}" \
    -v uvie-linux-cargo-target:/cargo-target \
    -v uvie-linux-cargo-home:/cargo-home \
    -e CARGO_TARGET_DIR=/cargo-target \
    -e CARGO_HOME=/cargo-home \
    -w /work \
    "$IMAGE" "$@"
