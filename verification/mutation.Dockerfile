FROM rust:1.99.0-slim-bookworm@sha256:ba81bc3eaa4422af576c0262515d96b0111a628a6ccc2c86557cf55c9a4bbee0
RUN CARGO_BUILD_JOBS=2 cargo install --locked cargo-mutants --version 27.1.0
WORKDIR /workspace
COPY Cargo.toml Cargo.lock /workspace/
COPY crates /workspace/crates
COPY src-tauri /workspace/src-tauri
COPY xtask /workspace/xtask
RUN cargo fetch --locked --target x86_64-unknown-linux-gnu
