# PR-NC1 — Non-convex / holed planar Stage-1 tessellation via CDT (spade)

**Status:** spec / FIP note (Manager phase). Role-separated TDD cycle (P5).
**Crates touched:** `cherchi-rs` (new CDT primitive), `yang-rs` (Stage-1 wiring).
**No** boolean / SSI / sidecar behavior changes.

## Problem

`yang-rs` Stage 1 tessellates planar B-Rep faces with a **fan from the first
boundary vertex** (`crates/yang-rs/src/lib.rs:606-638`, the
`Surface::Plane { .. } if all_line` arm). A fan triangulation is valid only for
**convex, hole-free** polygons. The `BRep::new` doc (`lib.rs:374-379`) states the
limitation explicitly: *"Convex faces only … No inner loops."*

- A **non-convex** outer loop (a reflex vertex — L/U-shaped extrude cap) emits
  fan triangles that fall **outside** the polygon.
- A face with **inner loops** (`BRepFace::inner_loops`, `lib.rs:204`) has its
  holes ignored entirely; the fan triangulates across the hole.

This is the `docs/yang_deviations.md` **D1 trap**: the legacy kernel used plain
`earcutr` here and burned 15 PR cycles (PR-Y32–Y46). The fix is a true
**Constrained Delaunay Triangulation** with boundary loops as hard constraint
edges. **Plain ear-clipping is FORBIDDEN** (D1). Backend: the **`spade` v2**
crate (already in `Cargo.lock`, WASM-clean), hosted in `cherchi-rs` (whose
curation bar admits exact-predicate deps) and consumed by `yang-rs` Stage 1
(preserving yang-rs's `cherchi-rs`/`ssi-rs`/`cad-primitives`-only layering).

## Intended outcome

Non-convex and holed planar faces tessellate with exact coverage, watertight, no
out-of-polygon triangles — while convex/box faces stay **byte-for-byte** on the
existing fan path (`fuzz_boxes` 900/900 unchanged, all prior yang-rs tests
unregressed). **No interior Steiner points; no boundary subdivision** — the
output vertex set equals the input boundary vertex set, so the `TessellationMap`
bijection (1:1 on boundary) is preserved.

## Part A — `cherchi-rs` CDT primitive

**Files:** new `crates/cherchi-rs/src/triangulation/mod.rs` (+ `cdt.rs` if split);
register `pub mod triangulation;` and re-export the public fn in
`crates/cherchi-rs/src/lib.rs`.

**Dep:** add `spade = "2"` to `crates/cherchi-rs/Cargo.toml` with a curation
comment mirroring the `dashu` / `geometry-predicates` annotations (WASM-clean,
exact predicates via `robust`, deterministic — no threads / rand / time).

**API:**
```rust
pub fn cdt_polygon_with_holes(
    verts: &[Point2],
    outer: &[u32],
    holes: &[Vec<u32>],
) -> Result<Vec<[u32; 3]>, CdtError>
```
- Boundary loops (outer + each hole) inserted as **hard constraint edges**.
- Return **only interior** triangles: inside the outer loop AND outside every
  hole. Filter by testing each triangle centroid against the loops
  (point-in-polygon), or by spade's face classification if exposed —
  interior-membership is the load-bearing rule.
- **No interior Steiner points; no boundary subdivision** — output vertex set ==
  input boundary vertices; triangles index into `verts`.
- **Deterministic** (byte-identical across runs): spade uses `foldhash`
  fixed-seed + exact `robust` predicates; insertion order is the caller's array
  order. Two calls → identical `Vec<[u32; 3]>`.
- `CdtError` enum: degenerate / duplicate input, loop index out of range,
  triangulation failure. **No panics** in production paths (`Result`-typed).
- MIT-attribution header + cite the algorithm class (CDT; Cherchi 2022 §4
  coplanar handler will reuse this at M6).

### Part A oracle (RED)
1. Exact **coverage**: Σ interior-triangle area == (outer area − Σ hole areas)
   within TAU.
2. Every **constraint (boundary) edge** appears as a triangle edge in output.
3. No triangle lies outside the outer loop or inside a hole.
4. **Determinism**: two calls → identical `Vec<[u32;3]>`.
5. A **non-convex (L/U)** case AND a **with-hole** case (e.g. square-in-square).

## Part B — `yang-rs` Stage-1 wiring

**File:** `crates/yang-rs/src/lib.rs`, the planar dispatch in `BRep::new` /
per-face loop (around `lib.rs:599-655`).

- Split the `Surface::Plane { .. } if all_line` arm: route to CDT **iff** the
  face is **non-convex** (a reflex vertex on its outer loop) **OR**
  `!inner_loops.is_empty()`. Otherwise keep the **existing fan path verbatim**
  (do NOT refactor it — `fuzz_boxes` byte-identity depends on it).
- **Reflex test:** build `face_verts` from `outer_loop[i].start` (as today),
  compute the Newell normal, and check the sign of consecutive 2D cross products
  in the face plane; any sign opposite the polygon orientation ⇒ reflex ⇒
  non-convex. (Convex + hole-free ⇒ fan, unchanged.)
- **CDT path:**
  1. Compute the plane basis from the face normal (intrinsic frame; reuse the
     same projection convention so adjacent faces agree — analogous to legacy
     D5/D9 plane-intrinsic origin). Project each outer + inner-loop boundary
     vertex (existing `out_verts` indices) to `Point2`.
  2. Build `verts: Vec<Point2>`, `outer: Vec<u32>`, `holes: Vec<Vec<u32>>` as
     **local** indices; keep a `local → global out_verts index` map.
  3. Call `cherchi_rs::cdt_polygon_with_holes`.
  4. Map local tri indices back to global `out_verts` indices; orient each
     triangle so its normal matches `Plane.normal` (same sign rule the fan path
     uses); `out_tris.push([..])`.
  - No new vertices pushed to `out_verts` / `sources` ⇒ `TessellationMap` stays
    1:1-on-boundary (bijection preserved).
- **Adjacency / watertightness:** CDT adds only interior diagonals → no boundary
  edge is split → adjacent faces share intact straight B-Rep boundary edges →
  watertight via shared verts.

### Part B oracle (RED)
1. **Non-convex face** (L-profile cap): exact coverage (mesh area == face area),
   watertight 2-manifold, NO triangle outside the polygon. (Pure `BRep::new`;
   no sidecar — Stage 1 is sidecar-free.)
2. **Face with a hole**: exact coverage (mesh area == face − hole).
3. **No boundary subdivision**: every B-Rep boundary edge appears unsplit as a
   mesh edge (adjacency preserved).
4. **Bijection** round-trips (every emitted vert maps to a boundary
   `BRepVertex` / `BRepEdge`).
5. **Determinism**.
6. **Convex/box byte-for-byte**: `fuzz_boxes` 900/900 unchanged; all prior
   yang-rs tests unregressed.

## STOP-and-report triggers (P9/P10 — do NOT improvise)
- spade fails to build for `wasm32-unknown-unknown` in-tree.
- spade output is not deterministic across runs.
- Coverage cannot be made exact within TAU.
- The convex fan path cannot be kept byte-for-byte.
- The plan's non-convex-detection diagnosis turns out wrong.
→ Report. The fallback is porting Livesu's CDT, **never** plain ear-clipping (D1).

## CI gate (FULL suites, BOTH crates) — clean before done
- `cargo test -p cherchi-rs` and `cargo test -p yang-rs`
- `cargo fmt --check` on both; `cargo clippy --all-targets -- -D warnings` on both
- `cargo build -p cherchi-rs --target wasm32-unknown-unknown` (spade must not
  break the WASM build)

## Docs to update on completion
- `docs/yang_functional_roadmap.md` — non-convex Stage-1 tessellation now uses
  CDT via spade (advances Phase 2 / M5 "non-convex profile triangulation").
- `docs/yang_deviations.md` — record the **no-Steiner planar simplification** as
  a documented deviation analogous to **N5**; note CDT-via-spade resolves the
  D1-class concern for the new kernel's planar Stage-1.

## Execution phases (each = one commit on `main`)
1. **Docs/Spec (Manager):** this note. `docs(...)`.
2. **RED (test sub-agent A):** add `spade` dep + empty `triangulation` stub
   returning `Err` so tests compile and **fail**; author both oracle suites.
   `test(...): PR-NC1 RED`. Test author writes NO production logic.
3. **GREEN (impl sub-agent B, distinct):** implement `cdt_polygon_with_holes`
   and the yang-rs wiring until all RED tests pass. Implementer edits NO tests.
   `feat(...): PR-NC1 GREEN`.
4. **Adversary (sub-agent C, distinct):** independent witness — verify exact
   coverage with an independently-computed area, byte-identity of the fan path
   on a convex fixture, a mutation that should break coverage, determinism
   across a fresh process. `test(...): PR-NC1 ADVERSARY`.
