set positional-arguments

default:
    @just --list

build:
    cargo build --workspace

check:
    cargo check --workspace
    cargo fmt --all -- --check
    cargo clippy --workspace --all-targets --all-features -- -D warnings
    cargo audit
    cargo deny check all
    cargo test --workspace

install:
    cargo install --path . --bin mini-gzip

run *args:
    cargo run --bin mini-gzip -- {{args}}

test:
    cargo test --workspace
