FROM ubuntu:24.04

ENV DEBIAN_FRONTEND=noninteractive

RUN apt-get update && apt-get install -y --no-install-recommends \
        build-essential \
        cmake \
        pkg-config \
        git \
        curl \
        ca-certificates \
        gettext \
        python3 \
        strace \
        libxkbcommon-dev \
        libxcb1-dev \
        libfontconfig1-dev \
        libudev-dev \
        libibus-1.0-dev \
        fcitx5-modules-dev \
        libfcitx5core-dev \
    && rm -rf /var/lib/apt/lists/*

RUN curl --proto '=https' --tlsv1.2 -sSf https://sh.rustup.rs \
        | sh -s -- -y --profile minimal --default-toolchain stable \
    && /root/.cargo/bin/rustup component add clippy rustfmt

ENV PATH=/root/.cargo/bin:$PATH

WORKDIR /work
