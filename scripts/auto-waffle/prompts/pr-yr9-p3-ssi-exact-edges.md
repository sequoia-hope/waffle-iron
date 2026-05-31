# PR-YR9 (P3) — yang-rs Stage 3: wire ssi-rs → EXACT intersection edges (cylinder ∪ box)

Context: P2c (PR-YR8) gave the first curved boolean — `cylinder ∪ box` flows
end-to-end, the analytic `Surface::Cylinder` survives, output is watertight
2-manifold. BUT the intersection edges where the cylinder cuts the box are still
**mesh-approximate polylines** (`Curve::LineSegment`). **This PR — the original
goal of the whole SSI effort — replaces them with the EXACT analytical curve from
`ssi-rs` (Yang 2025 Stage 3, §4.3).** This is the first real use of the `ssi-rs`
solvers inside the boolean.

For `cylinder ∪ box`, an intersection edge lies on a `Surface::Cylinder` (from one
solid) AND a `Surface::Plane` (a box face from the other). `ssi_rs::intersect`
of those two analytic surfaces is **plane∩cylinder** (PR-SSI2, already shipped) →
a `Circle` / `Ellipse` / `Line(s)`. The output B-Rep intersection edge becomes a
trimmed arc of that exact conic.

Read `crates/yang-rs/CLAUDE.md` (Hard rule #1 dep layering — `ssi-rs` IS allowed;
Stage development order item 6), `refs/text/yang2025_hybrid_boolean.txt` §4.3
(mapping intersection loops back to the B-Reps; the parametric-correspondence
optimization — note our case is far simpler than the paper's NURBS), and in
`crates/yang-rs/src/lib.rs`: `reconstruct_topology` (the curved branch from
PR-YR8; where `BRepEdge { curve: Curve::LineSegment }` is emitted), the
`TriangleAttributionMap` / per-patch `(InputId, face)` attribution (identifies
which edges sit between an A-patch and a B-patch — i.e. the intersection edges),
the `Surface` and `Curve` enums (PR-YR6), and `crates/ssi-rs/src/lib.rs`
(`intersect`, `QuadricSurface`, `SsiCurve` — field shapes were mirrored in PR-YR6
to make the conversion trivial).

## What to build

1. **Surface → `QuadricSurface` and `SsiCurve` → `Curve` conversions.** Map
   `yang::Surface::{Plane,Cylinder}` → `ssi_rs::QuadricSurface::{Plane,Cylinder}`
   and `ssi_rs::SsiCurve::{Circle,Ellipse,Line}` → `yang::Curve::{Circle,Ellipse,
   LineSegment}`. (Sphere/Cone are not produced by the cylinder∪box case — map
   them if trivial, else `unimplemented`-free loud error and note.)
2. **Identify intersection edges.** An output B-Rep edge is an *intersection*
   edge iff its two incident faces come from **different input solids** (one A,
   one B) — exactly where Stage 2 created new geometry. Use the patch
   attribution to find these. Original edges (both incident faces same solid, or
   cylinder rim / seam) keep their existing curve — do NOT touch them.
3. **Assign the exact curve (the Stage-3 core).** For each intersection edge:
   call `ssi_rs::intersect(surface_a, surface_b)`. It may return multiple curves
   (e.g. two circles); **select the one consistent with the edge's mesh polyline**
   — the curve passing within `d_ε` of the edge's mesh-derived sample points AND
   through the edge's two endpoint vertices. Set `edge.curve` to the converted
   exact `Curve`. The edge's start/end vertices already trim it to the correct
   arc (the `Curve` is the full conic; the endpoints bound it). If selection is
   ambiguous (no unique match, or `ssi-rs` returns `Err`/empty for a pair that
   geometrically must intersect), **STOP and report** (P9/P10) — do not guess or
   fall back to the polyline silently.

## Hard scope limits
- **`cylinder ∪ box` / `Cylinder∩Plane` only.** Do not attempt general degree-4,
  cyl∩cyl, sphere, or cone here. Sphere/Cone surfaces still reject loudly.
- Do not change the mesh, Stage 1/2, or the planar boolean path. This PR only
  upgrades the *curve geometry* carried on output intersection edges.
- The intersection edges remain topologically what P2c produced; only their
  `Curve` payload changes from `LineSegment` to the exact conic.

## Oracle (RED contract) — the exact curve lets us assert tighter than P2c
1. **Exact on BOTH surfaces (to `TAU`, not `d_ε`)**: sample the assigned exact
   `Curve` densely; every sample lies on BOTH incident analytic surfaces within
   `TAU_WORK`/`TAU_MODEL` (an analytical curve is exact — this is the whole point,
   and is a *strictly stronger* assertion than the `d_ε` mesh bound).
2. **Endpoints**: the exact curve passes through the edge's start and end vertices
   (within tolerance).
3. **Consistency with the mesh**: the exact curve stays within `d_ε` of the P2c
   mesh polyline it replaces (catches a wrong-conic selection).
4. **The result has exact edges**: `cylinder ∪ box` output has ≥1 intersection
   edge whose `Curve` is `Circle` or `Ellipse` (not `LineSegment`), with the
   parameters `ssi_rs::intersect` returned.
5. **Determinism**; **planar boolean unregressed** (`fuzz_boxes`); **sphere/cone
   still loud**; scope held (rim/seam edges keep their original curve).
   Provide a **sidecar-independent direct path** (hand-built attributed mesh →
   reconstruct → assert curve assignment) so the GREEN gate doesn't require the
   sidecar binary; env-gate the E2E parity with a LOUD skip.

## CI gate (FULL crate suite)
`cargo test -p yang-rs` (whole crate), `cargo fmt -p yang-rs -- --check`,
`cargo clippy -p yang-rs --all-targets -- -D warnings`, all clean.

On completion: update `docs/yang_functional_roadmap.md` — PR-YR9/P3 done (Stage 3
SSI wiring; cylinder ∪ box has analytically EXACT intersection edges; first use of
ssi-rs in the boolean). Note remaining: P2b (sphere tessellation), Stage 4 (CDT
remesh conforming to the exact curves), and broader surface/pair coverage.
