#!/usr/bin/env bash
# Print a single hash over every tracked source file that feeds the deployed
# WASM bundle.
#
# `scripts/build-wasm.sh` records this value in `app/static/pkg/.build-fingerprint`
# after a successful build; `.githooks/pre-commit` recomputes it and refuses a
# commit when the two disagree. That is the mechanical form of CLAUDE.md's
# "include the updated WASM bundle in the same commit as the Rust changes" —
# which was violated twice on 2026-07-28 (707618f7, 441d969c shipped a kernel
# the deployment did not have) precisely because nothing enforced it.
#
# Scope note: this hashes the WORKING TREE, not the index. The failure mode it
# exists to stop is "forgot to rebuild at all", which the worktree catches. A
# deliberate partial stage (rebuild, then stage only some of the sources) would
# slip past; that is a knowingly accepted gap, not an oversight.
set -euo pipefail
cd "$(git rev-parse --show-toplevel)"

# The wasm-bridge dependency closure (its Cargo.toml deps, transitively), plus
# the build inputs that change codegen without being Rust source. Keep in sync
# with crates/wasm-bridge/Cargo.toml — a new dependency crate belongs here.
PATHS=(
    'crates/wasm-bridge/src'
    'crates/wasm-bridge/Cargo.toml'
    'crates/feature-engine/src'
    'crates/feature-engine/Cargo.toml'
    'crates/kernel-v2/src'
    'crates/kernel-v2/Cargo.toml'
    'crates/modeling-ops/src'
    'crates/modeling-ops/Cargo.toml'
    'crates/step-import/src'
    'crates/step-import/Cargo.toml'
    'crates/sketch-solver/src'
    'crates/sketch-solver/Cargo.toml'
    'crates/file-format/src'
    'crates/file-format/Cargo.toml'
    'crates/waffle-types/src'
    'crates/waffle-types/Cargo.toml'
    'crates/yang-rs/src'
    'crates/yang-rs/Cargo.toml'
    'crates/cherchi-rs/src'
    'crates/cherchi-rs/Cargo.toml'
    'crates/ssi-rs/src'
    'crates/ssi-rs/Cargo.toml'
    'crates/cad-primitives/src'
    'crates/cad-primitives/Cargo.toml'
    'Cargo.lock'
    '.cargo/config.toml'
)

# `git ls-files` keeps this to TRACKED files only, so a stray scratch file in a
# src/ dir cannot silently change the fingerprint. Sorted for determinism.
git ls-files -z -- "${PATHS[@]}" \
    | sort -z \
    | xargs -0 sha256sum \
    | sha256sum \
    | cut -d' ' -f1
