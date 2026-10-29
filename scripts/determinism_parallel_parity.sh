#!/usr/bin/env bash
# Remediation plan O7: the determinism suites must produce the SAME canonical
# geometry hashes with cherchi-rs's `parallel` feature off (the default, and
# every WASM build) and on (`--features cherchi-rs/parallel`, opt-in for
# native embedders). A cargo feature cannot be toggled inside one test
# binary, so this script builds the two determinism binaries twice, has each
# append `<scenario> <hash>` lines to a file (WAFFLE_GEOMETRY_HASH_OUT), and
# diffs the files. Each build also asserts its own runs are bit-identical.
#
# Usage: scripts/determinism_parallel_parity.sh   (extra args go to cargo)
set -euo pipefail
cd "$(dirname "$0")/.."

out=$(mktemp -d)
trap 'rm -rf "$out"' EXIT
bins=(--test assay_determinism --test boolean_determinism)

# Prove the feature really reaches cherchi-rs in the second configuration
# (and not in the first), so the diff compares two different kernels.
if cargo tree -p test-harness -e features -i cherchi-rs 2>/dev/null \
    | grep -q 'cherchi-rs feature "parallel"'; then
  echo "FAIL: cherchi-rs/parallel is already on in the default configuration" >&2
  exit 1
fi
if ! cargo tree -p test-harness --features cherchi-rs/parallel -e features -i cherchi-rs 2>/dev/null \
    | grep -q 'cherchi-rs feature "parallel"'; then
  echo "FAIL: --features cherchi-rs/parallel did not enable the feature" >&2
  exit 1
fi

WAFFLE_GEOMETRY_HASH_OUT="$out/serial.txt" \
  cargo test --release -p test-harness "${bins[@]}" "$@"
WAFFLE_GEOMETRY_HASH_OUT="$out/parallel.txt" \
  cargo test --release -p test-harness --features cherchi-rs/parallel "${bins[@]}" "$@"

# The 10x tests append once per test; sort -u makes each file a set.
sort -u "$out/serial.txt" > "$out/serial.sorted"
sort -u "$out/parallel.txt" > "$out/parallel.sorted"
n=$(wc -l < "$out/serial.sorted")
if [ "$n" -lt 13 ]; then
  echo "FAIL: only $n hashes recorded (expected 10 proptest scenarios + 3 fixed)" >&2
  cat "$out/serial.sorted" >&2
  exit 1
fi
if ! diff -u "$out/serial.sorted" "$out/parallel.sorted"; then
  echo "FAIL: geometry hashes differ between cherchi-rs serial and parallel builds" >&2
  exit 1
fi
echo "OK: $n scenario hashes identical with cherchi-rs/parallel off and on"
cat "$out/serial.sorted"
