# kernel-v2 — CDT triangulation core for render tessellation (sliver root fix)

**Status:** spec (FIP Phase 1). **Change class:** bug fix (modeling-related).
**Crate:** `kernel-v2` (`tessellate.rs`), plus a mechanical re-export in
`yang-rs`. Follow-up of `kv2_patch_render_degeneracy_gate.md` §6b (the
measured sliver root-cause chain) — this is the "dedicated cycle" that spec
scoped out.

## 1. Goal

The render triangulation cores must not MINT sub-f32 slivers from healthy
boundaries. Today both f64 triangulation cores do:

- **Cylinder patch** (`tessellate_cylinder_patch` passes 3–4): greedy exact
  ear-clip keeps a femto-off-collinear corner as a sliver ear; the f64
  `delaunay_flip` cannot reliably remove it (its incircle is plain f64 —
  catastrophically ill-conditioned exactly on slivers); LEPP refinement then
  propagates it into dozens of B2 twins. Measured on F0047-with-canon
  (§6b): 64 degenerate triangles; today the loud render-degeneracy gate
  correctly rejects the face, so the CASE fails loudly instead of building.
  The failing 27-half-edge boundary ring (healthy: min pair 7.2e-3) is
  banked as a fixture (this cycle's instrumentation, 2026-07-02).
- **Planar face** (`tessellate_planar_face`): the same greedy exact
  ear-clip emits a triangle spanning three near-collinear boundary vertices
  (f64 height 5.4e-13 across a ~647-unit gear profile). No refinement
  cascade and NO gate on this path today → ONE silently degenerate render
  triangle. Measured on R0064-with-canon: FaceId(289), a 280-vertex
  all-segment planar gear loop at coordinate scale ~572, banked as a
  fixture. This is §6b item 5's "different tessellation path", now measured.

Both slivers are triangulation-AVOIDABLE (the boundary rings are healthy;
triangulations without f32-degenerate triangles exist — the baseline
ear-clip found one before the femto perturbation). The root fix replaces
the greedy-ear-clip + f64-flip core with **constrained Delaunay
triangulation** (`cherchi_rs::triangulation::cdt_polygon_with_holes`,
spade-backed): CDT is the max-min-angle triangulation of the constrained
point set, so if any triangulation avoids the sliver, CDT avoids it; its
flip decisions use exact predicates (`robust` incircle), not f64.

This unblocks the re-wire decision for the banked world-space vertex
canonicalization (`m8_shared_boundary_identity` §8a): with both minting
paths fixed, F0047/R0064-with-canon should tessellate CORRECTLY rather
than fail (F0047) or silently wreck (R0064).

## 2. Parameters

No new user-facing parameters. Internal: the CDT primitive consumes the
same 2D rings the ear-clip consumed (unrolled (u,h) for the patch;
dominant-axis projection for planar), with hole loops passed NATIVELY
(no bridge corridors — corridor-doubled vertices would be rejected by the
CDT as coincident). `n_seg` / chord tolerance semantics unchanged (LEPP
refinement of the patch is untouched).

## 3. Branch table

| # | Configuration | Behavior |
|---|---|---|
| C1 | Cylinder patch, 0 wrapping loops, no holes | CCW outer ring → CDT → LEPP refinement (unchanged) → emit |
| C2 | Cylinder patch, 0 wrapping loops, hole loops | Holes passed natively to CDT (bridge corridors deleted from this path) |
| C3 | Cylinder patch, 2 wrapping loops (barrel seam) | Seam-cut ring assembly UNCHANGED; cut ring + remaining chains as native holes → CDT |
| C4 | CDT rejects the ring (coincident verts / crossing constraints / zero area) | Loud typed `TessellationFailed` (new reason string), never a fallback |
| P1 | Planar face, no holes | Projected polygon → CDT → emit (winding follows the ring, as today) |
| P2 | Planar face with holes | Native CDT holes (no bridging) |
| P3 | Planar CDT rejects | Loud typed `TessellationFailed` |
| G1 | Planar emitted triangle B2/B3-degenerate at f32 | Loud `TessellationFailed { reason: "planar triangle collapsed at render precision" }` — the cylinder gate's predicate applied to the planar path (the §6 "follow-up sweep" of the gate spec), always-on |
| G0 | (existing) cylinder-patch B2/B3 gate | UNCHANGED — still the loud boundary for input-forced sub-f32 features |

## 4. Invariants

- I1 (root fix): the banked F0047 patch ring and R0064 planar ring
  tessellate successfully with ZERO f32-degenerate triangles (B2+B3 = 0
  under the gate predicate) and a watertight per-face triangulation
  (every boundary edge exactly once, interior edges exactly twice, in the
  local index space).
- I2 (conformality): the CDT stage adds no Steiner points and does not
  split constraint edges — the boundary vertex set in = vertex set out.
  Patch LEPP splits keep the existing chord-safe kind rules (untouched).
- I3 (exact partition): triangle areas sum exactly (rationally) to the
  polygon-with-holes area — CDT without Steiner points is an exact
  partition, same as ear-clip; the existing KV3 area/volume oracles keep
  passing unchanged.
- I4 (winding): emitted triangle winding follows the ring (planar normals
  ≡ face Newell normal; patch fold tripwire unchanged).
- I5 (determinism): byte-identical output for identical input (the CDT
  primitive canonicalizes its output; ring assembly order unchanged).
- I6 (input-forced degeneracy stays loud): a boundary carrying a genuine
  sub-f32 feature still fails the gates loudly — the existing cylinder
  gate suite must pass unchanged, and the new planar gate gets the
  equivalent twin-vertex RED case.
- I7 (regression gates): rewrite + fast tiers green; full assay category
  counts unchanged with 0 WRONG (quiet box; 30s-cap flips long-cap
  verified per the M8 campaign protocol).

## 5. Oracles

- Unit RED (root fix, cylinder): the banked 27-half-edge F0047 FaceId(17)
  ring (EllipseArc + Line curves, oblique axis, n_seg=71) — today fails
  with the render-degeneracy gate; after the fix must return `Ok` with
  B2+B3 = 0 and a closed local boundary (edge-pairing scan).
- Unit RED (root fix, planar): the banked 280-vertex R0064 FaceId(289)
  gear loop — today `Ok` with exactly one f32-zero-cross triangle
  (silent); after the fix must return `Ok` with zero.
- Unit RED (planar gate G1): a planar loop with two vertices spaced below
  f32 resolution at its coordinate magnitude (the planar analogue of
  `build_cylinder_patch(true)`) — today tessellates silently degenerate;
  after: loud typed failure.
- Unit guards: every existing tessellation test (kv5b patch fixtures,
  the cylinder gate suite incl. its adversary block, planar/KV3 suites,
  torus patch) passes unchanged.
- E2E: full assay per I7. The with-canon configurations of F0047/R0064
  are re-measured in the Phase-4 re-wire experiment (separate increment,
  decision recorded in `m8_shared_boundary_identity` §8a).

## 6. Failure modes

- A ring the CDT cannot triangulate (self-intersecting projection,
  coincident vertices — e.g. loops touching at a vertex, previously
  silently tolerated by bridging): loud typed `TessellationFailed`. If the
  full assay shows a population of such cases, the increment stops and the
  population is recorded (P10) — no silent fallback to ear-clip.
- Hole/exterior classification in the CDT primitive is f64 centroid
  parity: a pathologically slit-thin region could misclassify and DROP
  triangles (a slit). Residual risk documented; gated corpus-wide by the
  assay watertightness/volume oracles, and locally by the fixture
  edge-pairing scans. (The refined variant's flood-fill emit exists in the
  primitive if this ever fires — not wired in this cycle.)
- Input-forced sub-f32 boundaries: unchanged loud gates (G0 existing, G1
  new).
- Dead code after the swap (`bridge_hole`, `ear_clip`, `delaunay_flip` if
  no callers remain): deleted in the same increment as their last caller,
  with their test coverage retargeted at the new core where the property
  still applies (tests are permanent; property-level, not
  implementation-level).

## 6a. GREEN-phase finding (2026-07-03, Manager adjudication): KV9-F3 seam femto-twin

The new G1 planar gate unmasked a pre-existing B-Rep OUTPUT defect: the
parallel cyl×cyl secant subtract (kv9 fixture r1=0.30, r2=0.22, d=0.35)
emits a cap loop with TWO adjacent vertices at the tool cylinder's seam
point — (0.13, 0) exact and (0.13, 5.38844591624835605e-18) — bridged by a
degenerate 5.4e-18 Arc edge (measured, `KV2_G1_DUMP`). Any triangulation of
that forced boundary edge is f32-degenerate; the old ungated path emitted
it silently, the gate now rejects it loudly (correct per I6/P9). The two
kv9 unit tests are quarantined `#[ignore = "KV9-F3 …"]`; the fix is
output-side vertex identity (the `m8_shared_boundary_identity` follow-up
class), NOT a gate exception.

Corpus spot-check vs the pre-cycle baseline (same box): F0041/F0045/F0058
fail identically at baseline (pre-existing walls), F0043 passes both,
**F0042 improved** (baseline Errored "no active features with solids" →
Passed 9 oracles under the CDT core). No corpus regression in the class;
the Phase-4 full-assay per-case diff is the binding gate.

## 7. Research basis

- Constrained Delaunay triangulation and its max-min-angle optimality:
  REFERENCES.md #10 (Lévy 2025, exact-predicate mesh CSG with CDT), #12
  (Barki 2015, regularized booleans on CDT), #11 (Cherchi 2020 §"three-
  phase algorithm" — per-triangle CDT of arrangements; the in-repo
  `cherchi-rs` triangulation module is this cycle's primitive and is
  already the production CDT for yang-rs Stage-1 and the torus UV patch).
- Exact predicates for the Delaunay flip decision: #4 (Shewchuk 1997) —
  spade's `robust` backend implements exactly these; the defect being
  fixed is a plain-f64 incircle making wrong flip decisions precisely on
  near-degenerate slivers.
- LEPP/longest-edge bisection quality preservation from the initial mesh
  (Rivara 1984, cited in-module): unchanged; the fix improves its INITIAL
  mesh from "greedy ear-clip output" to "max-min-angle CDT", which is the
  hypothesis under which Rivara's angle bound is meaningful.
- Ear clipping as a boundary-sliver liability is already project doctrine
  for the exact pipeline (`docs/yang_deviations.md` D1 forbids it in
  yang-rs Stage-1); this cycle aligns kernel-v2's render cores with it.

### 7a. Analytical vs approximate

No SSI involved. The change is confined to the 2D triangulation of
exactly-sampled boundary rings for the RENDER channel; surface geometry,
boundary sampling, and modeling truth are untouched. Method: exact
constraint insertion + exact-predicate Delaunay (spade/robust), f64
coordinates in = f64 coordinates out.

## 8. Architecture note (dependency path)

kernel-v2 must not depend on cherchi-rs directly (crate rule 1). yang-rs
already re-exports cherchi backend types and hosts the torus UV-CDT
consumer; this cycle adds a mechanical re-export
(`pub use cherchi_rs::triangulation::{cdt_polygon_with_holes, CdtError}`)
in yang-rs — same pattern as `NativeBoolean`. The module-doc decision
record in `tessellate.rs` (ear-clip was chosen because "yang-rs does not
re-export the CDT") is superseded and must be updated in the same PR.
