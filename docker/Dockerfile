# Build and test environment for anki_mining (Tauri v2 + Rust + Node).
# Ubuntu 22.04 is used on purpose: building the AppImage against an older glibc
# keeps it compatible with more distributions.
FROM ubuntu:22.04

ENV DEBIAN_FRONTEND=noninteractive \
    RUSTUP_HOME=/usr/local/rustup \
    PATH=/usr/local/cargo/bin:$PATH \
    CARGO_HOME=/cache/cargo \
    APPIMAGE_EXTRACT_AND_RUN=1 \
    NO_STRIP=true

RUN apt-get update && apt-get install -y --no-install-recommends \
        build-essential \
        ca-certificates \
        curl \
        file \
        git \
        libayatana-appindicator3-dev \
        libfuse2 \
        libgtk-3-dev \
        librsvg2-dev \
        libssl-dev \
        libwebkit2gtk-4.1-dev \
        libx11-dev \
        libxdo-dev \
        libxrandr-dev \
        patchelf \
        pkg-config \
        imagemagick \
        wget \
        x11-utils \
        xdg-utils \
        xdotool \
        xvfb \
    && rm -rf /var/lib/apt/lists/*

# Node.js 22 (same major version used on the host).
RUN curl -fsSL https://deb.nodesource.com/setup_22.x | bash - \
    && apt-get install -y --no-install-recommends nodejs \
    && rm -rf /var/lib/apt/lists/*

# Rust toolchain installed system-wide; the cargo registry lives in /cache (bind mount).
RUN curl --proto '=https' --tlsv1.2 -sSf https://sh.rustup.rs \
        | CARGO_HOME=/usr/local/cargo sh -s -- -y --profile minimal --default-toolchain stable \
           --component clippy --component rustfmt \
    && chmod -R a+rX /usr/local/rustup /usr/local/cargo

WORKDIR /workspace
