# Performance results

This file captures local, reproducible performance numbers for `mini-gzip`.

## How to reproduce

Microbenchmarks (Criterion):

```bash
cargo bench --bench inflate
```

## Baseline (size matrix)

Recorded: 2026-04-02

```bash
rm -rf target/criterion
cargo bench --bench inflate -- --save-baseline size-matrix-base
```

| case | mean | 95% CI |
|---|---:|---:|
| small_text | 0.826 µs | [0.762, 0.891] µs |
| repetitive_4k | 11.19 µs | [10.28, 12.17] µs |
| random_4k | 11.68 µs | [10.54, 12.91] µs |
| repetitive_16k | 29.93 µs | [29.87, 29.99] µs |
| random_16k | 29.15 µs | [28.67, 29.88] µs |
| repetitive_64k | 128.39 µs | [119.76, 138.41] µs |
| random_64k | 183.18 µs | [169.54, 197.06] µs |
| textish_64k | 126.85 µs | [119.95, 135.24] µs |
| repetitive_256k | 530.46 µs | [493.88, 571.84] µs |
| random_256k | 607.38 µs | [557.58, 658.47] µs |
| textish_256k | 456.67 µs | [454.31, 459.81] µs |
| repetitive_1m | 1.93 ms | [1.82, 2.06] ms |
| random_1m | 2.68 ms | [2.45, 2.94] ms |
| textish_1m | 2.11 ms | [1.96, 2.27] ms |
| repetitive_16m | 33.58 ms | [31.10, 36.35] ms |
| random_16m | 43.47 ms | [41.49, 45.43] ms |
| textish_16m | 37.25 ms | [34.26, 40.42] ms |

## Baseline (upstream-main)

Recorded: 2026-04-02

```bash
cargo bench --bench inflate
```

- inflate_gzip/small_text: 736 ns .. 896 ns (median ~805 ns)
  - throughput: 43.6 .. 53.1 MiB/s
- inflate_gzip/random_64k: 134 µs .. 170 µs (median ~151 µs)
  - throughput: 368 .. 468 MiB/s
- inflate_gzip/repetitive_256k: 458 µs .. 571 µs (median ~515 µs)
  - throughput: 438 .. 546 MiB/s
- inflate_gzip/random_1m: 2.665 ms .. 3.141 ms (median ~2.873 ms)
  - throughput: 318 .. 375 MiB/s

## Optimization: inline match-copy in `codes()`

Change: 2026-04-02

Patch summary:
- Inline the inner match-copy loop in `codes()` (avoid calling `State::output` for each byte and avoid recomputing the match source index each iteration).

```bash
cargo bench --bench inflate
```

Results (compared to previous criterion baseline in `target/criterion`):

- inflate_gzip/small_text: 688 ns .. 798 ns (median ~734 ns)
  - change: -14.3% median time (improved)
- inflate_gzip/random_64k: 125 µs .. 142 µs (median ~132 µs)
  - change: within noise threshold
- inflate_gzip/repetitive_256k: 498 µs .. 612 µs (median ~554 µs)
  - change: no change detected
- inflate_gzip/random_1m: 2.528 ms .. 2.720 ms (median ~2.632 ms)
  - change: -13.8% median time (improved)
