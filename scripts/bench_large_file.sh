#!/usr/bin/env bash
set -euo pipefail

repo_root=$(cd "$(dirname "${BASH_SOURCE[0]}")/.." && pwd)
bin="${repo_root}/target/release/mini-gzip"
fixture_default="${repo_root}/target/benchmark-fixtures/typescript-5.9.3.js"
fixture="${MINI_GZIP_PERF_FIXTURE:-$fixture_default}"
iterations="${MINI_GZIP_PERF_ITERATIONS:-30}"
repeat="${MINI_GZIP_PERF_REPEAT:-5}"

usage() {
  cat <<USAGE
Usage: $(basename "$0") [--fixture PATH] [--iterations N] [--repeat N]

Environment overrides:
  MINI_GZIP_PERF_FIXTURE
  MINI_GZIP_PERF_ITERATIONS
  MINI_GZIP_PERF_REPEAT
USAGE
}

while [[ $# -gt 0 ]]; do
  case "$1" in
    --fixture)
      fixture="$2"
      shift 2
      ;;
    --iterations)
      iterations="$2"
      shift 2
      ;;
    --repeat)
      repeat="$2"
      shift 2
      ;;
    -h|--help)
      usage
      exit 0
      ;;
    *)
      echo "unknown argument: $1" >&2
      usage >&2
      exit 1
      ;;
  esac
done

if [[ ! -x "$bin" ]]; then
  echo "missing release binary at $bin; run: cargo build --release" >&2
  exit 1
fi

if [[ ! -f "$fixture" ]]; then
  echo "missing benchmark fixture at $fixture; run: ./scripts/fetch_perf_fixture.sh" >&2
  exit 1
fi

exec "$bin" perf-file --path "$fixture" --iterations "$iterations" --repeat "$repeat"
