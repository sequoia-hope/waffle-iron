# kernel-v2 — Scope Rules

Phase 4 of the clean-sheet kernel rewrite (see root `CLAUDE.md`). Clean re-implementation of the CAD kernel.

## What this crate does

- Half-edge B-Rep arena (vertices, edges, half-edges, faces, shells, solids — same shape as before, but with invariants enforced)
- Euler operators with manifoldness validation at every operator exit
- Primitive constructors: extrude, revolve, sphere, cylinder, cone, torus
- Render tessellation (single implementation, no LOD-conditional masking)
- Public API: `Kernel` + `KernelIntrospect` traits — refined from the legacy versions to drop dead methods and tighten contracts
- Delegates boolean ops to `yang-rs`

## What this crate does NOT do

- Boolean labeling — that's `cherchi-rs` via `yang-rs`
- Yang pipeline stages — that's `yang-rs`
- SSI solvers — that's `ssi-rs`
- Fillet / chamfer / shell — deferred indefinitely
- Sketch solving — that's `sketch-solver` (unchanged)
- File format — that's `file-format` (unchanged)
- WASM bridge — that's `wasm-bridge` (will be updated to use kernel-v2 at Phase 4 migration)

## Hard rules

1. **Dep layering enforced:** `cad-primitives`, `yang-rs` only. NO direct dep on `cherchi-rs`, `ssi-rs`, or the old `kernel` crate.
2. **Invariant: `face.surface_geom.normal ≡ Newell(face.outer_loop)`.** Enforce at every Euler operator's exit via `debug_assert!`. Constructor APIs that would violate it return `Err`.
3. **Invariant: 2-manifold topology.** Operations that would produce non-manifold edges/vertices return `Err(KernelError::NonManifoldTopology)`. No silent repair.
4. **No `unsafe`, no `panic!` in production paths, no `catch_unwind`.** All errors return `Result<>`. This keeps the WASM build on stable Rust (no nightly, no `-Zbuild-std`).
5. **Tessellation is a single canonical implementation.** No `tessellate_planar_face_bounded` plus `tessellate_polygon_face` plus a third path for cylinders plus a fourth for spheres. One per surface type, no `reverse_outer` masking, no `bulk_flip`, no force-aligning. The polygon walk direction IS the source of truth.
6. **No internal mutation pathways from one feature to another.** Each Euler op takes `&mut arena` and modifies arena state — no caching, no `cached_face_polys`, no `cached_render_mesh`, no `is_polygon_soup` flags.

## When working on this crate

You may read:
- Everything inside `crates/kernel-v2/`, `crates/yang-rs/`, `crates/cherchi-rs/`, `crates/ssi-rs/`, `crates/cad-primitives/`
- `crates/waffle-types/` (the public types crate)
- Reference papers in `refs/text/` as relevant (Mantyla for Euler ops, etc.)

You may NOT read:
- `crates/kernel/src/` (the legacy kernel being replaced) — this is the biggest discipline. The legacy kernel has accumulated patches and masking; reading it will steer you toward replicating those.
- `crates/feature-engine/` and `crates/wasm-bridge/` internals — they're CONSUMERS of the `Kernel` trait. The trait's design is determined by what they need, not by inspecting their implementation. (You may look at the public `Kernel` trait signature in `waffle-types` to know what you must implement.)
- Old audit memos under `docs/audits/yang_*`, `docs/audits/cherchi_*`, etc. — irrelevant.

## API refinement freedom

The `Kernel` trait can be refined from the legacy version. Concrete things to drop / change:

- Drop methods that always return `NotSupported` (fillet, chamfer, shell)
- Drop methods only used by legacy boolean dispatch
- Split the trait if natural (`KernelTopology` + `KernelTessellation` + `KernelIntrospect`)
- Tighten signatures (e.g., return concrete error variants instead of generic `KernelError`)

When refinement breaks `wasm-bridge` or `feature-engine`, those updates land in the Phase 5 migration PR — not piecemeal during kernel-v2 development.

## Phase ordering inside kernel-v2

1. B-Rep arena types + Euler operators (foundation; everything else depends on this)
2. Primitive constructors (extrude, revolve, sphere, ...)
3. Tessellation (per-surface-type implementations)
4. Boolean delegation to yang-rs (thin wrapper)
5. Trait implementation
6. Migration of consumers (wasm-bridge, feature-engine) — Phase 5 separate PR
