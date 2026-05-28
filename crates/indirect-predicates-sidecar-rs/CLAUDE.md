# indirect-predicates-sidecar-rs — Scope Rules

FFI sidecar wrapper around Marco Attene's `Indirect_Predicates` C++ library (LGPL-2.1, header-only). Provides the exact geometric predicates that `cherchi-rs` Stage 2 needs to implement Cherchi 2022 §6.4 boolean labeling (`orient3d_indirect_IIII`, `lambda3d_LPI/TPI_*`, `lessThanOnX/Y/Z_*`). PR-CR-IP1 ships only the scaffold + a single link-probe call; real predicate wrappers come in PR-CR-IP2..IP7.

## What this crate does

- Compiles a thin C++ shim (`src/wrapper.cpp`) that `#include`s the upstream header-only library and exposes `extern "C"` functions
- Uses `bindgen` against `src/wrapper.h` (pure C) to generate Rust FFI for the shim
- Resolves the library source via env var (`INDIRECT_PREDICATES_SRC`) or a default path
- Gracefully falls back to a no-op stub when the source is unavailable — build never fails

## What this crate does NOT do

- Any pure-Rust predicate implementation — that's `cherchi-rs`'s mission (long-term: clean-room Rust replacement)
- WASM target — compiles native C++ via `cc::Build`; emits `compile_error!` on `wasm32`
- Subprocess-based execution — this is FFI/link-time, NOT subprocess (contrast with `cherchi-sidecar-rs`)
- Anything beyond a thin Rust shim over the C++ functions

## Hard rules

1. **Workspace deps**: NONE for v1. This is the lowest layer; below `cad-primitives`. (PR-CR-IP2+ may add a dep on `cad-primitives` for `Point3` interop; document then.)
2. **Build-deps**: `cc` + `bindgen` only.
3. **NOT WASM-compatible**. `compile_error!` at `src/lib.rs` top-level when `target_arch == "wasm32"`. The user accepts a broken WASM build during the Yang validation phase; PR-CR-IP10 banked to restore via cfg-gating in consumers.
4. **LGPL-2.1 boundary**: this crate's source is MIT (workspace default). The C++ library it dynamically links is LGPL-2.1. Document in `LICENSE-THIRD-PARTY.md` at crate root. Distributors who statically embed must comply with LGPL obligations — that's their concern, not ours.
5. **`unsafe` contained in `mod ffi`** + one-line `pub fn` wrappers. No `unsafe` in `tests/` or doc examples.
6. **No `panic!` in production paths.** `link_probe()` returns sentinel values, never panics.
7. **Bindgen output** lives in `$OUT_DIR` (never checked in).
8. **No GMP**: do NOT define `USE_GNU_GMP_CLASSES`. The library has its own bigfloat impl. PR-CR-IP9 banked for opt-in.
9. **No SIMD**: do NOT define `USE_SIMD_INSTRUCTIONS`. PR-CR-IP8 banked.
10. **Missing source = graceful**: when the env var is unset and the default path is missing, build emits a `cargo:warning`, sets `cargo:rustc-cfg=ip_unavailable`, and compiles `src/stub.cpp` (which returns a sentinel). Tests self-skip.
11. **C++20 toolchain required** when source is available (`gcc ≥ 10` or `clang ≥ 10`). Documented in this file.

## When working on this crate

You may read:
- Everything inside `crates/indirect-predicates-sidecar-rs/`
- `crates/slvs-patch/slvs-0.6.0/build.rs` (cc + bindgen precedent)
- `crates/cherchi-sidecar-rs/` (non-WASM sidecar precedent — different mechanism but same scope-rules format)
- The upstream `Indirect_Predicates` headers at the env-configured path

You may NOT read:
- `crates/kernel/` (legacy; orthogonal)
- `crates/cherchi-rs/` (downstream consumer — its internal use is its concern)
- `crates/yang-rs/`, `crates/kernel-v2/`, `crates/wasm-bridge/` (further downstream)
