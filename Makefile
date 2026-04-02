.PHONY: test test-crosscheck fuzz-init fuzz-list fuzz-inflate fuzz-roundtrip

# Default tests (deterministic + proptest)

test:
	cargo test

# Optional cross-check against system `gzip -dc`

test-crosscheck:
	MINI_GZIP_CROSSCHECK=1 cargo test --test roundtrip optional_cross_validate_with_system_gzip

# Fuzzing requires nightly toolchain.

fuzz-init:
	cargo install cargo-fuzz --version 0.12.0
	cargo +nightly fuzz init || true

fuzz-list:
	cargo +nightly fuzz list

fuzz-inflate:
	cargo +nightly fuzz run inflate_gzip -- -runs=10000

fuzz-roundtrip:
	cargo +nightly fuzz run roundtrip_deflate -- -runs=10000
