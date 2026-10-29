advisory_options := ""

default:
    @just --list

dev:
    mise x -- pnpm tauri dev

check: format lint typecheck test security

format:
    mise x -- cargo fmt --all --check
    mise x -- cargo fmt --manifest-path verification/Cargo.toml --check
    mise x -- cargo fmt --manifest-path fuzz/Cargo.toml --check
    mise x -- pnpm exec biome format .

lint:
    mise x -- cargo clippy --workspace --all-targets --locked -- --deny=warnings
    mise x -- pnpm lint

typecheck:
    mise x -- pnpm typecheck

test:
    mise x -- cargo test --workspace --locked
    mise x -- pnpm test

security:
    mise x -- cargo deny {{ advisory_options }} check --deny warnings advisories
    mise x -- pnpm audit
    mise x -- gitleaks dir . --redact --no-banner

ui:
    mise x -- pnpm test:ui

build:
    mise x -- pnpm tauri build --bundles app -- --locked

build-assurance:
    mise x -- pnpm tauri build --config src-tauri/tauri.assurance.conf.json --bundles app -- --locked

proofs:
    mise x -- cargo run --target-dir target/xtask --locked -p xtask -- proofs

lean:
    mise x -- cargo run --target-dir target/xtask --locked -p xtask -- lean

fuzz:
    mise x -- cargo run --target-dir target/xtask --locked -p xtask -- fuzz

[positional-arguments]
archive *arguments:
    mise x -- cargo run --locked -p celestite-archive -- "$@"

mutations-list:
    mise x -- cargo run --target-dir target/xtask --locked -p xtask -- mutations-list

mutations:
    mise x -- cargo run --target-dir target/xtask --locked -p xtask -- mutations

verify: check ui proofs lean fuzz mutations
