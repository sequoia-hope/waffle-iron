# PR-YR9 (P3) — yang-rs Stage 3: wire `ssi-rs` → EXACT intersection edges (cylinder ∪ box)

> Manager spec of record for the role-separated FIP cycle (P5): Spec (this doc)
> → RED (test-author sub-agent) → GREEN (implementer sub-agent) → Adversary
> (third sub-agent). The implementer never edits tests; the test author never
> writes production code. Stay on `main`; commit each phase; push at end.

## 1. Objective

PR-YR8/P2c gave the first curved boolean: `cylinder ∪ box` flows end-to-end, the
analytic `Surface::Cylinder` survives, output is watertight 2-manifold. BUT the
intersection edges where the cylinder pierces the box caps are still
**mesh-approximate polylines** (`Curve::LineSegment`). This PR — the original
goal of the whole SSI effort — replaces them with the **EXACT analytical conic**
from `ssi-rs` (Yang 2025 Stage 3, §4.3). It is the **first real use of `ssi-rs`
inside the boolean.**

For `cylinder ∪ box`, an output intersection edge lies on a `Surface::Cylinder`
(one solid) AND a `Surface::Plane` (a box cap of the other). `ssi_rs::intersect`
of those two analytic surfaces is plane∩cylinder (PR-SSI2, shipped) → a `Circle`
(axis ⊥ cap, the canonical case), `Ellipse` (oblique), or `Line`s (parallel).
The output B-Rep intersection edge's `Curve` payload changes from `LineSegment`
to the exact conic; its start/end vertices already trim it.

## 2. Hard scope limits

- **`cylinder ∪ box` / `Cylinder∩Plane` only.** No degree-4, cyl∩cyl, sphere, or
  cone. Sphere/Cone surfaces still reject loudly.
- Do NOT change the mesh, Stage 1/2, the planar boolean path, or the
  normal-flip / Newell / E2/E3 machinery. Only the `Curve` payload on output
  **intersection** edges changes.
- Same-input boundaries (cylinder rim/seam A↔A, plane↔plane B↔B, hull edges)
  keep their existing `Curve::LineSegment`.

## 3. Current state (verified against `crates/yang-rs/src/lib.rs`)

- `crates/yang-rs/Cargo.toml` already deps `ssi-rs` + `cad-primitives`. CLAUDE.md
  Hard rule #1 allows `ssi-rs`; Stage dev order item 6 = "Stage 3/4 SSI".
- `reconstruct_topology(mesh, attribution, a, b)` (`src/lib.rs:1634`) already
  receives both input BReps — no signature change to reach the surfaces. It
  flood-fills single-input patches and emits `BRepEdge`s via two `push_loop`
  closures (Cylinder branch ~1681, planar branch ~1826), today hardcoding
  `Curve::LineSegment`.
- Field shapes mirror field-for-field: `Surface::{Plane,Cylinder}` ↔
  `QuadricSurface::{Plane,Cylinder}`; `ssi_rs::SsiCurve::{Circle,Ellipse,Line}` ↔
  `Curve::{Circle,Ellipse,LineSegment}`.
- **Plane→QuadricSurface::Plane**: `point = -d * normal` (unit normal; matches
  the `signed_distance_to_surface` / `plane_dist` convention `n·x + d = 0`).
- `curved_chord_bound(edges)` (`src/lib.rs:1088`) returns `Some(d_ε)` for a solid
  with `Curve::Circle` rims, `None` for all-planar. The YR8 oracle's `d_eps`
  recomputes the identical `1e-2 × AABB_diag` from the rim circles.

## 4. Definitions

- An **output intersection edge** = an undirected mesh boundary edge incident to
  **two patches with different `InputId`**. `ssi_rs::intersect` is symmetric in
  arg order and returns a deterministic `Vec`.
- **Plane∩Plane** (planar A↔B edges) → ssi `Line` → `LineSegment`, so the planar
  `fuzz_boxes` corpus output stays all-`LineSegment` (no regression).
- **Selection tolerance = the Stage-1 `d_ε`**, reusing the single-source helper
  `curved_chord_bound(cyl_input.edges())` where `cyl_input` is whichever of
  `a`/`b` owns the `Surface::Cylinder` of the edge (A14.3 single-source; no new
  literal).

## 5. Implementation (GREEN) — all edits in `crates/yang-rs/src/lib.rs`

1. **Error variants** (near `YangError` ~1197; `Display` arm ~1238; leave
   `Error::source` unchanged — `SsiError` is carried by value, not boxed):
   ```rust
   SsiRefinementFailed { edge: (u32, u32), reason: SsiRefinementError },
   ```
   plus a public sibling enum `SsiRefinementError` with arms:
   `IntersectFailed(ssi_rs::SsiError)`, `AmbiguousCurve { candidates: usize, matched: usize }`,
   `UnsupportedCurve` (Parabola/Hyperbola — defensive),
   `UnsupportedSurfaceForSsi` (Sphere/Cone — defensive). All loud (P9), no panic.

2. **`fn surface_to_quadric(s: Surface) -> Result<ssi_rs::QuadricSurface, SsiRefinementError>`**
   (near `signed_distance_to_surface` ~1157): Plane→Plane (`point = -d*normal`),
   Cylinder→Cylinder (direct), Sphere/Cone→`Err(UnsupportedSurfaceForSsi)`.

3. **`fn ssi_curve_to_curve(c: ssi_rs::SsiCurve) -> Result<Curve, SsiRefinementError>`**:
   Circle→Circle, Ellipse→Ellipse (field-for-field), Line→`LineSegment`;
   Parabola/Hyperbola→`Err(UnsupportedCurve)`.

4. **`fn curve_contains_point(c: &ssi_rs::SsiCurve, p: Point3, tol: f64) -> bool`** —
   implicit on-curve distance (no parameter solving):
   - Circle: `|axial| ≤ tol ∧ |radial − radius| ≤ tol` (axial/radial split about
     unit `normal`).
   - Line: perpendicular distance `≤ tol`.
   - Ellipse: in-plane (`|w·n̂| ≤ tol`) ∧ normalized radial residual
     `|√((u/a)²+(v/b)²) − 1| · min(a,b) ≤ tol`, where `(u,v)` are the components
     of `w = p − center` along `major_axis` and `normal × major_axis`.
   - Parabola/Hyperbola → `false`.

5. **`fn build_intersection_curves(incidence, mesh, a, b) -> Result<BTreeMap<(u32,u32),Curve>, YangError>`**:
   for each canonical undirected edge whose incidence list has exactly **two
   entries with different `InputId`**: convert both surfaces, call
   `ssi_rs::intersect`, derive `tol` via `curved_chord_bound` on the
   cylinder-owning input's edges, select the **unique** returned curve passing
   `curve_contains_point` within `tol` for **both** endpoints. `matched != 1` ⇒
   `Err(SsiRefinementFailed { reason: AmbiguousCurve { candidates, matched } })`;
   intersect `Err` ⇒ `IntersectFailed`; surface conversion `Err` ⇒ wrap.
   Insert the converted exact `Curve` keyed by the canonical edge. (All N edges
   of one ring feed the same `(Cylinder,Plane)` pair and the same deterministic
   vec ⇒ select the same circle; keying by undirected edge + computing once is
   correct.)

   - For a Plane∩Plane edge the unique surviving curve is a `Line` → `LineSegment`,
     so the map entry equals the fallback and the planar corpus is unchanged.
   - For a same-input edge (both incidence entries same `InputId`) NO entry is
     produced: it keeps `Curve::LineSegment`.
   - When BOTH endpoints lie within `tol` of NO returned curve, or of ≥2 → STOP
     (`AmbiguousCurve`). Never silently fall back to the polyline on a genuine
     selection failure.

6. **Refactor `reconstruct_topology` (1634–1852) into two passes (minimal diff):**
   - **First pass** builds `Vec<PatchInfo { cycles, input, inherited, face_idx }>`
     — moves `patch_boundary_cycle` + the face-range check + `inherited` lookup
     here, so the range-check error path lives in **exactly one place**.
   - Build `incidence: BTreeMap<(u32,u32), Vec<(InputId, Surface)>>` over all
     patch boundary edges (canonical key), using `info.inherited` (UNFLIPPED —
     the conic is sign-invariant; Union has no flip anyway).
   - `let intersection_curves = build_intersection_curves(&incidence, mesh, a, b)?;`
   - **Emission loop** unchanged except: iterate `&infos`, read `info.*`, and both
     `push_loop` closures set
     `curve: intersection_curves.get(&canonical).copied().unwrap_or(Curve::LineSegment)`,
     where `canonical = (min(s,e), max(s,e))`. The Newell/flip/E2/E3 logic is
     untouched (it reads `cycles`/`signed_areas`, never the per-edge `curve`).

## 6. STOP-and-report conditions (P9/P10 — do NOT improvise)

- `ssi_rs::intersect` returns `Err` on a pair we expect to intersect → return
  `SsiRefinementFailed { IntersectFailed }`. Do not catch-and-fall-back.
- Zero or ≥2 candidate curves pass `curve_contains_point` for both endpoints →
  `SsiRefinementFailed { AmbiguousCurve }`. Do not pick the first / nearest.
- A Parabola/Hyperbola is selected → `UnsupportedCurve`. (Cannot occur for
  Cylinder∩Plane; defensive only.)
- If the cylinder-owning input has `curved_chord_bound == None` (no circle rims)
  → that is a producer fault, return loud (reuse the carrying error). Never
  default to `TAU_WORK` for a curved selection.

## 7. Oracle (RED contract) — `tests/yr9_stage3_ssi.rs`

Mirror the YR8 harness (reuse `cylinder_brep`, `unit_cube_brep_offset_at`,
`hand_built_tube_arrangement`, `LabelMock`, `d_eps`, canonical config). The
existing YR8 tests must stay GREEN unchanged. New oracles:

1. **Exact on BOTH surfaces**: densely sample each assigned exact `Curve` (≥32
   pts); every sample lies on the incident cylinder (`|dist_to_axis−r|`) AND the
   incident plane (`|n·x+d|`) within `TAU_MODEL` (strictly stronger than `d_ε`).
2. **Endpoints**: the exact curve passes through the edge's start/end vertices
   within `d_ε`.
3. **Consistency**: the exact curve stays within `d_ε` of the P2c mesh polyline
   it replaces (catches wrong-conic selection).
4. **Has exact edges**: `cylinder ∪ box` (mock direct path) output has ≥1
   intersection edge whose `Curve` is `Circle` (the two cap rings), with
   center/normal/radius equal to what `ssi_rs::intersect(Cylinder, Plane)`
   returns for that cap. Both rings present (bottom z=0, top z=1) with distinct
   centers.
5. **Scope held**: planar `fuzz_boxes` output unregressed (all `LineSegment`);
   `Plane∩Plane`→`LineSegment`; same-input boundaries keep `LineSegment`;
   sphere/cone still loud (`BRep::new` reject); determinism (identical inputs →
   identical edge curves). Provide a **sidecar-independent direct path** (mock
   arrangement → `boolean()` → assert curve assignment) as the GREEN gate;
   env-gate the real-sidecar E2E with a LOUD skip (`CHERCHI2022_BIN`).
6. **STOP path**: a construction that forces `intersect` Err or an ambiguous
   selection returns `Err(YangError::SsiRefinementFailed { .. })` — not a silent
   polyline.

## 8. Adversary (third sub-agent)

Independently audit: (a) the conic is truly exact (re-derive the circle from
geometry, compare params), not re-fit from mesh; (b) selection cannot silently
fall back to `LineSegment` on failure (grep for any `unwrap_or` / `ok()` that
swallows an intersect error in the new path); (c) no weakened tolerances vs YR8;
(d) determinism; (e) scope — confirm same-input/rim/seam edges and the planar
corpus are byte-unchanged. Adversary strengthens tests, does not touch
production.

## 9. CI gate (FULL crate suite, must be clean)

```
cargo test -p yang-rs
cargo fmt -p yang-rs -- --check
cargo clippy -p yang-rs --all-targets -- -D warnings
```
Real-sidecar E2E (optional, LOUD-skip without binary):
`CHERCHI2022_BIN=... cargo test -p yang-rs --test yr9_stage3_ssi`.

## 10. Deviations from Yang 2025

None introduced. This is the first faithful realization of Stage 3 (§4.3) for
the Cylinder∩Plane pair: the intersection edge carries the analytic conic, not a
polyline. Stage 4 (CDT remesh conforming to the exact curve) remains future
work; the mesh tessellation is unchanged this PR (the edge's *curve metadata* is
exact even though the underlying triangulation is still faceted — the B-Rep edge
is the truth, governance A15).
