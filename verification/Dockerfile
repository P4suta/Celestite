FROM rust:1.100.0-slim-bookworm@sha256:476f6598b8d14a1f1bc7e58efc67af206ac8a559c37a3401ce512db2582d0ff7
RUN apt-get update \
    && apt-get install --yes --no-install-recommends curl=7.88.1-10+deb12u15 \
    && rm -rf /var/lib/apt/lists/* \
    && CARGO_BUILD_JOBS=2 cargo install --locked kani-verifier --version 0.68.0 \
    && cargo kani setup
WORKDIR /workspace
