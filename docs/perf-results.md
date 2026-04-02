# Performance results

This file captures local, reproducible performance numbers for `mini-gzip`.

## How to reproduce

Microbenchmarks (Criterion):

```bash
cargo bench --bench inflate
```

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
