### mini-gzip

gzip decompression in under 300 lines of rust

## Reproducible large-file benchmark

The repo ships a frozen `perf-file` workflow for the large TypeScript fixture used in prior performance checks.
The fixture is pinned to `typescript@5.9.3` from the public npm tarball, extracted to
`target/benchmark-fixtures/typescript-5.9.3.js`, and verified against this SHA-256:

- `3ae902c92cc44dace175c0e69e13a4b0899f6983c6121d76b9ab8dd5795e7675`

From a clean checkout:

```bash
cargo build --release
./scripts/fetch_perf_fixture.sh
./scripts/bench_large_file.sh
./scripts/bench_large_file_rss.py
```

Default benchmark parameters match the frozen comparison flow used by Cody task tracking:

- `--iterations 30`
- `--repeat 5`

The helper scripts accept overrides if you want to experiment:

```bash
./scripts/bench_large_file.sh --iterations 50 --repeat 10
./scripts/bench_large_file_rss.py --iterations 50 --repeat 10
```

If you already have the fixture somewhere else, point the shell wrapper at it with:

```bash
MINI_GZIP_PERF_FIXTURE=/path/to/typescript.js ./scripts/bench_large_file.sh
./scripts/bench_large_file_rss.py --fixture /path/to/typescript.js
```
