#!/usr/bin/env bash
set -euo pipefail

repo_root=$(cd "$(dirname "${BASH_SOURCE[0]}")/.." && pwd)
out_dir="${repo_root}/target/benchmark-fixtures"
out_file="${out_dir}/typescript-5.9.3.js"
version="5.9.3"
expected_sha256="3ae902c92cc44dace175c0e69e13a4b0899f6983c6121d76b9ab8dd5795e7675"

tmpdir=$(mktemp -d)
cleanup() {
  rm -rf "$tmpdir"
}
trap cleanup EXIT

mkdir -p "$out_dir"
curl -fsSL "https://registry.npmjs.org/typescript/-/typescript-${version}.tgz" -o "$tmpdir/typescript.tgz"
tar -xzf "$tmpdir/typescript.tgz" -C "$tmpdir"
cp "$tmpdir/package/lib/typescript.js" "$out_file"
actual_sha256=$(sha256sum "$out_file" | awk '{print $1}')

if [[ "$actual_sha256" != "$expected_sha256" ]]; then
  echo "fixture checksum mismatch: expected $expected_sha256 got $actual_sha256" >&2
  exit 1
fi

printf 'fixture=%s\nsha256=%s\n' "$out_file" "$actual_sha256"
