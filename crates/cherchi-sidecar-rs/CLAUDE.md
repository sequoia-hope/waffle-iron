# cherchi-sidecar-rs — Scope Rules

Subprocess wrapper around the Cherchi 2022 `mesh_booleans` C++ binary. Provides a `MeshBoolean` implementation for the workspace until the native cherchi-rs port (Stages 2-4 of arrangement) ships.

## What this crate does

- Implements `cherchi_rs::MeshBoolean` for a `SidecarBoolean` struct
- Resolves the `mesh_booleans` binary via env var (`CHERCHI2022_BIN`) or default path
- Writes OBJ files to a tempdir, invokes the binary with a timeout, reads the output OBJ
- Exposes structured errors via `SidecarError`
- Exposes OBJ I/O publicly (`pub mod obj`) for debugging + fixture capture

## What this crate does NOT do

- Any pure-Rust boolean algorithm — that's `cherchi-rs`'s mission (Stages 2-4 still WIP)
- WASM target — subprocess + filesystem; document this loudly at crate-doc level
- Caching, parallelism orchestration, or anything beyond a thin Rust shim

## Hard rules

1. **Workspace deps**: ONLY `cad-primitives` and `cherchi-rs`. No others.
2. **External crates**: `std` only for v1. (Future: `thiserror` once `SidecarError` grows beyond ~6 variants.)
3. **NOT WASM-compatible**. Document at crate-doc level. The lib targets do not need a `#[cfg(not(target_arch = "wasm32"))]` gate because nothing else in the workspace depends on this crate transitively from WASM builds yet — but yang-rs / kernel-v2 wiring eventually needs to be feature-gated.
4. **No `unsafe`.**
5. **All errors via `Result<>`**. No `panic!` in production paths. No `eprintln!` side effects from library functions.
6. **`SidecarError` is concrete**; `MeshBoolean::boolean` boxes it as `Box<dyn Error + Send + Sync>`.

## When working on this crate

You may read:
- Everything inside `crates/cherchi-sidecar-rs/`
- `crates/cad-primitives/` and `crates/cherchi-rs/` (its dependencies)
- `docs/sidecar/cherchi2022_build_guide.md` (binary build recipe)
- `crates/cherchi-rs/tests/common/sidecar.rs` and `obj.rs` — algorithmic templates (deliberately duplicated; see spec for rationale)

You may NOT read:
- `crates/kernel/` (legacy; orthogonal)
- `crates/yang-rs/` / `crates/kernel-v2/` (downstream consumers; their internal use of this crate is their concern)
