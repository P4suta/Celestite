FROM rust:1.100.0-slim-bookworm@sha256:476f6598b8d14a1f1bc7e58efc67af206ac8a559c37a3401ce512db2582d0ff7
RUN CARGO_BUILD_JOBS=2 cargo install --locked cargo-mutants --version 27.1.0
WORKDIR /workspace
COPY Cargo.toml Cargo.lock /workspace/
COPY crates /workspace/crates
COPY src-tauri /workspace/src-tauri
COPY xtask /workspace/xtask
RUN cargo fetch --locked --target x86_64-unknown-linux-gnu
