#!/usr/bin/env python3
import argparse
import pathlib
import resource
import subprocess
import sys

repo_root = pathlib.Path(__file__).resolve().parent.parent
parser = argparse.ArgumentParser(description="Run the large-file perf benchmark and report child ru_maxrss")
parser.add_argument("--fixture", default=str(repo_root / "target/benchmark-fixtures/typescript-5.9.3.js"))
parser.add_argument("--iterations", type=int, default=30)
parser.add_argument("--repeat", type=int, default=5)
args = parser.parse_args()

bin_path = repo_root / "target/release/mini-gzip"
fixture_path = pathlib.Path(args.fixture)

if not bin_path.is_file():
    print(f"missing release binary at {bin_path}; run: cargo build --release", file=sys.stderr)
    sys.exit(1)

if not fixture_path.is_file():
    print(f"missing benchmark fixture at {fixture_path}; run: ./scripts/fetch_perf_fixture.sh", file=sys.stderr)
    sys.exit(1)

subprocess.run(
    [
        str(bin_path),
        "perf-file",
        "--path",
        str(fixture_path),
        "--iterations",
        str(args.iterations),
        "--repeat",
        str(args.repeat),
    ],
    cwd=repo_root,
    check=True,
)
print(f"ru_maxrss_kib={resource.getrusage(resource.RUSAGE_CHILDREN).ru_maxrss}")
