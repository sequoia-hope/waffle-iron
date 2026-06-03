# PR-NC1 — non-convex planar Stage-1 tessellation via CDT (cherchi-rs CDT + yang-rs wiring)

Context: yang-rs Stage-1 fan-triangulates planar faces from the first vertex —
**correct only for CONVEX faces**; a non-convex profile (reflex vertex) or a face
with holes produces triangles OUTSIDE the polygon. This PR adds proper
**Constrained Delaunay Triangulation (CDT)** for non-convex planar faces + holes.

**CRITICAL — this is the `docs/yang_deviations.md` D1 trap. Use CDT, NOT plain
ear-clipping.** Yang §4.1.2 / §4.4.1 specify CDT (boundary loops as hard
constraint edges). The legacy port used plain `earcutr` and burned 15 PR cycles on
the resulting cross-face-divergence defect. Plain ear-clipping is FORBIDDEN here.
(NB: Livesu's "simplified earcut" IS a CDT — but we are using `spade`, below.)

## Decision (already made — implement it, don't re-litigate)
The CDT backend is the **`spade`** crate (v2), vetted as clearing the dep bar:
WASM-compatible (`wasm32-unknown-unknown` builds), MIT/Apache-2.0, uses the
`robust` crate (Shewchuk exact predicates — not inexact float geometry), and has
no threads/rand/time (deterministic; `foldhash` fixed-seed hasher).

Host the CDT in **`cherchi-rs`** (its curation bar already admits `dashu` /
`geometry-predicates`; the native arrangement M6 will reuse a CDT). yang-rs Stage-1
consumes it — so yang-rs keeps its `cherchi-rs`/`ssi-rs`/`cad-primitives`-only
layering (no new yang-rs external dep).

## Part A — `cherchi-rs` CDT primitive
- Add `spade = "2"` to `crates/cherchi-rs/Cargo.toml` (per its external-crate
  curation: WASM-clean, exact-predicate, deterministic — note this in the dep
  comment, mirroring the `dashu`/`geometry-predicates` annotations).
- New `cherchi-rs` module (e.g. `triangulation/cdt.rs`) exposing a constrained
  Delaunay triangulation of a **planar polygon with holes**:
  `cdt_polygon_with_holes(verts: &[Point2], outer: &[u32], holes: &[Vec<u32>])
  -> Result<Vec<[u32;3]>, …>` — boundary loops are **hard constraint edges**;
  return ONLY interior triangles (outside the outer loop and inside any hole are
  excluded). **No interior Steiner points** (planar = exact at any triangulation;
  boundary vertices only — so no boundary subdivision). Deterministic
  (byte-identical across runs). Follow cherchi-rs conventions (attribution header,
  test groups, no panic in production, Result errors).
- cherchi-rs oracle: exact **coverage** (Σ triangle area == polygon-minus-holes
  area within TAU); every constraint (boundary) edge appears in the output; no
  triangle lies outside the outer loop or inside a hole; **determinism**
  (identical output across two runs); a non-convex (L/U) case AND a with-hole case.

## Part B — yang-rs Stage-1 wiring
- For a planar face that is **non-convex** (has a reflex vertex) OR has **inner
  loops (holes)**, tessellate via `cherchi_rs::cdt_polygon_with_holes` (project the
  face to its plane → 2D, CDT, lift back). **Convex, hole-free faces keep the
  existing fan path BYTE-FOR-BYTE** (do NOT route them through CDT — the planar
  box-boolean `fuzz_boxes` 900/900 must stay identical).
- Adjacency/watertightness: CDT adds only **interior** diagonals (no boundary
  subdivision), so adjacent faces share their straight B-Rep boundary edges intact
  → watertight via shared verts. Assert this (no boundary edge is split).
- Bijection: each emitted vertex is a boundary `BRepVertex`/`BRepEdge` source
  (no new interior verts), so the `TessellationMap` stays 1:1-on-boundary.

## Oracle (yang-rs RED contract)
1. **Non-convex face** (e.g. an L-profile extrude cap) tessellates with **exact
   coverage** (mesh area == face area), **watertight 2-manifold**, NO triangle
   outside the polygon. 2. **Face with a hole** likewise (mesh area == face−hole).
3. **No boundary subdivision** (every B-Rep boundary edge appears as a mesh edge,
   unsplit) ⇒ adjacency preserved. 4. **Bijection** round-trips. 5. **Determinism**.
6. **Convex/box paths byte-for-byte** — `fuzz_boxes` 900/900 unchanged; all prior
   yang-rs tests unregressed.

## CI gate (FULL suites, both crates)
`cargo test -p cherchi-rs`, `cargo test -p yang-rs`, plus `cargo fmt --check` and
`cargo clippy --all-targets -- -D warnings` on BOTH crates. Confirm the wasm32
build of cherchi-rs still works (`cargo build -p cherchi-rs --target
wasm32-unknown-unknown`) — spade must not break it.

**STOP-and-report (P9/P10)** if spade turns out NOT to build for wasm32 in-tree, or
its output isn't deterministic, or coverage can't be made exact — do NOT fall back
to plain ear-clipping (that's D1); report and we'll port Livesu's CDT instead.

On completion: update `docs/yang_functional_roadmap.md` + `docs/yang_deviations.md`
(non-convex Stage-1 tessellation now uses CDT via spade — resolves the D1-class
concern for the new kernel; note the no-Steiner planar simplification as a
documented deviation, analogous to N5). NO boolean/ssi changes.
