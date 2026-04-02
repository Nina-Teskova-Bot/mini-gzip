# Optimization re-run: inline match-copy loop in codes()

Repro:
```bash
git checkout speed-matrix-20260402
rm -rf target/criterion
cargo bench --bench inflate -- --save-baseline size-matrix-base
cargo bench --bench inflate -- --baseline size-matrix-base
```

## Before/After table (mean + 95% CI)

| case | baseline mean | baseline 95% CI | optimized mean | optimized 95% CI | Δ mean |
|---|---:|---:|---:|---:|---:|
| random_16k | 32.61 µs | [30.53 µs, 34.94 µs] | 43.55 µs | [38.03 µs, 49.47 µs] | +33.6% |
| random_16m | 56.45 ms | [52.30 ms, 60.77 ms] | 48.63 ms | [45.71 ms, 51.96 ms] | -13.9% |
| random_1m | 3.05 ms | [2.82 ms, 3.31 ms] | 2.63 ms | [2.55 ms, 2.71 ms] | -13.8% |
| random_256k | 446.14 µs | [441.01 µs, 454.35 µs] | 648.67 µs | [597.26 µs, 705.24 µs] | +45.4% |
| random_4k | 9.69 µs | [9.31 µs, 10.16 µs] | 8.97 µs | [8.79 µs, 9.19 µs] | -7.4% |
| random_64k | 112.26 µs | [110.70 µs, 114.49 µs] | 139.86 µs | [129.20 µs, 152.42 µs] | +24.6% |
| repetitive_16k | 29.86 µs | [29.83 µs, 29.90 µs] | 29.99 µs | [29.87 µs, 30.12 µs] | +0.4% |
| repetitive_16m | 33.96 ms | [31.52 ms, 36.65 ms] | 28.69 ms | [28.62 ms, 28.77 ms] | -15.5% |
| repetitive_1m | 2.02 ms | [1.90 ms, 2.16 ms] | 1.79 ms | [1.79 ms, 1.80 ms] | -11.3% |
| repetitive_256k | 447.63 µs | [446.45 µs, 449.10 µs] | 448.11 µs | [446.86 µs, 449.73 µs] | +0.1% |
| repetitive_4k | 10.10 µs | [9.53 µs, 10.77 µs] | 8.88 µs | [8.86 µs, 8.90 µs] | -12.1% |
| repetitive_64k | 121.71 µs | [115.77 µs, 129.03 µs] | 119.32 µs | [116.07 µs, 123.04 µs] | -2.0% |
| small_text | 612 ns | [601 ns, 626 ns] | 718 ns | [663 ns, 777 ns] | +17.4% |
| textish_16m | 32.31 ms | [30.25 ms, 34.66 ms] | 28.47 ms | [28.45 ms, 28.49 ms] | -11.9% |
| textish_1m | 1.82 ms | [1.79 ms, 1.88 ms] | 1.86 ms | [1.80 ms, 1.95 ms] | +2.0% |
| textish_256k | 469.99 µs | [451.81 µs, 494.34 µs] | 476.55 µs | [466.33 µs, 494.73 µs] | +1.4% |
| textish_64k | 152.25 µs | [139.30 µs, 165.99 µs] | 161.93 µs | [149.11 µs, 175.60 µs] | +6.4% |

## Discussion of changes

This re-run shows the candidate change is **workload dependent**:
- Improves some compressible/streaming tiers (`repetitive_4k`, `repetitive_1m`, and all `*_16m`).
- Regresses several mid-size random tiers (`random_16k`, `random_64k`, `random_256k`) and `small_text`.

Interpretation: inlining the match-copy loop reduces overhead in match-heavy paths, but may worsen performance when match-copy is not dominant (random-ish inputs), likely due to extra bookkeeping/cache effects. Treat this as a candidate until we decide target workloads or adjust the implementation to avoid the mid-size regressions.