# Correctness

This project is a tiny gzip inflater.

## Test layers

### 1) Deterministic roundtrip tests

`tests/roundtrip.rs` contains basic, deterministic cases (empty, small text, large repeated input).

Run:

```bash
make test
```

### 2) Property tests (proptest)

`tests/roundtrip.rs` also includes a `proptest` property:

- generate random byte vectors (0..64KiB)
- compress with `flate2` (known-good)
- decompress with `mini_gzip::inflate_gzip`
- assert equality

Run:

```bash
make test
```

### 3) Optional cross-validation with system gzip

If you want to validate our output matches `gzip -dc` for a known input, enable it via env var:

```bash
make test-crosscheck
```

This is optional because CI environments may not have `gzip` installed.

### 4) Fuzzing

Fuzz targets live in `fuzz/fuzz_targets/`:

- `inflate_gzip`: feed arbitrary bytes to `inflate_gzip` and ensure it never crashes (returns `Ok` or an error).
- `roundtrip_deflate`: generate gzip data with `flate2`, decompress with us, assert equality.

**Note:** `cargo-fuzz` requires a nightly toolchain (`-Zsanitizer`).

```bash
make fuzz-list
make fuzz-inflate
make fuzz-roundtrip
```

## What is (not yet) checked

- gzip trailer (CRC32 + ISIZE) is currently ignored.
- full coverage of all gzip header flag combinations is limited.

The current harness focuses on protecting against panics/UB and ensuring roundtrip correctness for data produced by a known-good gzip implementation.
