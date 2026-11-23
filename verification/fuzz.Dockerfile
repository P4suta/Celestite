FROM rust:1.100.0-slim-bookworm@sha256:476f6598b8d14a1f1bc7e58efc67af206ac8a559c37a3401ce512db2582d0ff7
RUN rustup toolchain install nightly-2026-10-25 --profile minimal \
    && CARGO_BUILD_JOBS=2 cargo install --locked cargo-fuzz --version 0.13.2
RUN apt-get update \
    && apt-get install --yes --no-install-recommends g++=4:12.2.0-3 \
    && rm -rf /var/lib/apt/lists/*
WORKDIR /workspace
COPY Cargo.toml Cargo.lock /workspace/
COPY crates /workspace/crates
COPY src-tauri /workspace/src-tauri
COPY xtask /workspace/xtask
COPY docs /workspace/docs
COPY fuzz /workspace/fuzz
RUN cargo fetch --locked --manifest-path fuzz/Cargo.toml --target x86_64-unknown-linux-gnu
