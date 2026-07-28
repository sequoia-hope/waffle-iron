#!/usr/bin/env bash
# Build the WASM bundle and install it into the app — the canonical recipe from
# CLAUDE.md, plus the fingerprint that lets the pre-commit hook verify the
# deployed bundle actually matches the committed sources.
#
# Standard stable wasm-pack since the Phase 6 migration: no nightly, no
# -Zbuild-std, no panic=unwind. The enlarged 4MB wasm stack lives in
# .cargo/config.toml and must stay.
#
# Usage:  ./scripts/build-wasm.sh
set -euo pipefail
cd "$(git rev-parse --show-toplevel)"

echo "==> wasm-pack build (release, target web, no default features)"
wasm-pack build crates/wasm-bridge --release --target web --no-default-features

echo "==> installing into app/static/pkg/"
cp crates/wasm-bridge/pkg/wasm_bridge_bg.wasm \
   crates/wasm-bridge/pkg/wasm_bridge.js \
   app/static/pkg/

# Record WHAT was built, so the hook can tell a fresh bundle from a stale one.
# Written last: if the build or the copy fails, the old fingerprint stays and
# the hook keeps complaining, which is the safe direction to fail.
./scripts/wasm-fingerprint.sh > app/static/pkg/.build-fingerprint
echo "==> fingerprint $(cat app/static/pkg/.build-fingerprint)"

echo
echo "WASM bundle updated. Commit app/static/pkg/ together with the Rust change."
