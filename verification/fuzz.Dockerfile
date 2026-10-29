FROM rust:1.99.0-slim-bookworm@sha256:ba81bc3eaa4422af576c0262515d96b0111a628a6ccc2c86557cf55c9a4bbee0
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
