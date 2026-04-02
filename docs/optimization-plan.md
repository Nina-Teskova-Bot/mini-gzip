# Optimization measurement plan (do not implement yet)

The goal is to optimize *after* correctness is protected.

## 1) Benchmarks

Add Criterion benches (future):

- `inflate_gzip` throughput for:
  - small text (~1KiB)
  - medium JSON (~100KiB)
  - large repeated (~10MiB)
  - incompressible random (~1MiB)
- measure both:
  - wall time
  - bytes/sec

Suggested bench harness:

- pre-generate gzip payloads with `flate2` at different compression levels.
- include the gz bytes as `include_bytes!()` fixtures to avoid IO variance.

## 2) Profiles

Use:

- `cargo build --release` + `perf record/report` (Linux)
- `cargo flamegraph` (if available)

Collect:

- hottest functions (likely `bits`, `decode`, `codes` loops)
- branch-mispredict / cache metrics if using `perf stat`

## 3) Optimization ideas to test (later)

Only after benchmark+profile baselines:

- reduce bounds checks in `State::nextbyte` and in window indexing
- avoid repeated `Vec::push` reallocations by pre-sizing output when possible
- optimize Huffman decode loop (table-based decode for small bit widths)
- reduce conversions between i32/u32/usize

## 4) Regression protection

- keep proptests and fuzz targets in place
- add a small set of golden gz fixtures and expected outputs
- ensure `cargo test` stays fast; gate heavier checks behind env vars
