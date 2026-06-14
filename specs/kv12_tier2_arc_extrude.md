# KV12 Tier 2 Spec — exact arc-segment profile extrude (cylinder side patches)

Status: Prototype-release Phase E **COMPLETE — E1–E4b DONE (2026-06-14)**.
Arc profiles (with or without holes) extrude with exact cylinder walls
end-to-end (kernel → adapter → app).
Scope: `crates/kernel-v2/src/{profile,construct,geom,validate}.rs`,
`crates/kernel-v2/src/adapter.rs` (wiring), `crates/cad-primitives` /
`kernel-v2/exact2d.rs` (new arc predicates). NO yang-rs / boolean change.

> **Goal.** Extrude a closed profile whose boundary mixes line segments and
> circular arcs into an *exact* B-Rep: planar caps bounded by line+arc loops,
> planar side walls for line edges, and a **cylinder patch** for each arc edge
> (an arc swept linearly along the normal IS a cylinder lateral). Replaces
> KV12 Tier 1's chord-polygon approximation with exact analytic surfaces that
> survive volume/tessellation. **Extrude-only** — using the result as a boolean
> operand is the KV7 curved partial-patch re-entry wall, out of scope here.

## 1. What already exists (build on, don't reinvent)

- **Data:** `ClosedProfile` carries `vertex_ids` (the chord-sampled polygon) AND
  `arc_segments[i] = { start_vertex_index, end_vertex_index, center_u, center_v,
  radius }`. The exact arc geometry is therefore already on the wire.
- **Surface/curve vocabulary:** `Surface::Cylinder`, `Curve::Arc` (construct.rs),
  used by KV5b cylinder patches and the KV6a revolve.
- **Assembler templates:** `extrude_circle` (construct.rs:1100 — caps + ONE
  cylinder lateral, direct arena assembly) and `build_partial_revolve`
  (construct.rs:574 — arc walls + arc-bearing caps, partial sweep).
- **Downstream is already curved-aware:** `geom::signed_volume` handles planar
  arc-bearing caps (KV6a `planar_arc_face_flux`) and cylinder patches (KV5a);
  tessellation handles both; `validate_solid` is the manifold/winding gate.

So the NEW work is (a) the mixed line/arc extrude **assembler**, and (b) **exact
arc-loop simplicity validation** (the only piece with no scaffolding).

## 2. Representation — `ProfileRegion::ArcPolygon`

Add a third `ProfileRegion` variant (profile.rs); the enum is already
`#[non_exhaustive]` for exactly this.

```
enum ProfileEdge {                 // (u,v) plane coordinates
    Line { a: Point2, b: Point2 },
    Arc  { a: Point2, b: Point2, center: Point2, radius: f64, ccw: bool },
}
ProfileRegion::ArcPolygon { outer: Vec<ProfileEdge>, holes: Vec<Vec<ProfileEdge>> }
```

`Profile::arc_polygon(origin, u, v, outer, holes)` validates and constructs (the
only way to obtain one), mirroring `Profile::new`. Arc edges store the EXACT
center/radius; the chord samples are not used.

## 3. Reconstructing edges from a `ClosedProfile` (adapter, E4)

Walk `vertex_ids`. A maximal run `[start_vertex_index ..= end_vertex_index]`
covered by an `arc_segment` collapses to one `ProfileEdge::Arc` (endpoints =
`positions[vertex_ids[start]]` / `[end]`, center/radius from the segment, `ccw`
from the cross product of the chord vs. the mid-sample). Every other consecutive
pair is a `ProfileEdge::Line`. Validate each arc segment's center/radius against
its endpoint samples (within the import band) BEFORE trusting it — else fall
back to the Tier-1 chord polygon (loud note, never silent).

## 4. The B-Rep target (single loop, sweep ⟂ plane)

For an outer loop of `k` edges extruded by `w = normal·distance`:

- **Bottom cap:** 1 planar face, surface = sketch plane, outer loop = the k
  edges as `Curve::LineSegment` / `Curve::Arc`.
- **Top cap:** the same loop translated by `w`, reversed.
- **Per line edge:** 1 planar quad side face (bottom line, seam-up, top line
  reversed, seam-down), surface = the wall `Plane`.
- **Per arc edge:** 1 **cylinder patch** side face — boundary (bottom arc,
  seam-up, top arc reversed, seam-down), `Surface::Cylinder { axis_point =
  embed(arc.center), axis_dir = ŵ, radius }`. (Arc swept along ŵ = cylinder
  lateral; identical shape to a KV5b partial patch.)
- **Seam:** at each boundary vertex, a bottom+top vertex and a seam line edge;
  shared (twinned) between the two adjacent side faces.

Euler check: V = 2k, E = 3k (k bottom + k top + k seam), F = k + 2, χ = 2 — the
same bookkeeping as `build_partial_revolve`. Holes add inner loops to both caps
and their own side faces (genus 0; the cap is multi-loop like a holed prism).

Half-edge/twin wiring mirrors `extrude_circle` generalized to k mixed edges:
each side face loop is `[bottom_edge, seam_up, top_edge_rev, seam_dn]`;
`bottom_edge` twins the bottom cap, `top_edge` the top cap, `seam_*` the
neighboring side face.

**Oblique sweep:** if `ŵ` is not ∥ the plane normal, an arc sweeps an *oblique*
cylinder (elliptical section) — gate `Err(ExtrudeObliqueArcUnsupported)` for now
(mirror `extrude_circle`'s `ExtrudeObliqueCircleUnsupported`); perpendicular
sweep only in Tier 2 v1.

## 5. Exact arc-loop simplicity validation (E3 — the hard piece, P9)

`Profile::arc_polygon` must reject a self-intersecting boundary EXACTLY (no
sampled approximation). New predicates (exact2d / cad-primitives):

- **arc ∩ segment** and **arc ∩ arc** intersection-existence tests, exact (dashu
  rationals; a circle is `(x−cx)²+(y−cy)² = r²`, intersect with a line / another
  circle → ≤2 candidate points, test each lies within both edges' angular/param
  spans). Reuse the filtered→exact cascade pattern.
- Pairwise non-touching over all outer+hole edges (the `loops_touch` analog,
  extended to arcs); hole-strictly-inside-outer with an arc-aware
  point-in-region (ray cast counting arc crossings).
- Arc validity: `|a−center| = |b−center| = radius` within band; non-degenerate
  (radius > MIN_FEATURE_SIZE, sweep angle ∈ (0, 2π)).

`Profile::arc_polygon` is the exact gate — like `Profile::new`, a constructed
value IS the evidence of simplicity.

## 6. Increments (each gates the next; RED→GREEN)

- **E1 — single-arc assembler + caps. ✅ DONE (2026-06-14).** `extrude` handles
  `ArcPolygon` for a one-arc-one-or-more-lines single loop. Representation
  (`ProfileEdge` Line/Arc + `ProfileRegion::ArcPolygon` + `Profile::arc_polygon`
  E1-level validation) in `profile.rs`; the direct assembler
  `extrude_arc_profile` in `construct.rs` mirrors `build_partial_revolve`'s
  half-edge/twin wiring with LINEAR seams (`af`/`ab` → `Curve::LineSegment`),
  per-edge `Line`/`Arc` cap+wall curves, and `Surface::Plane`/`Cylinder` walls;
  arc traversal sense (`±a` normal, cylinder `reversed`) derived from
  `sign(((A−C)×(B−C))·a)`. Test fixture is a **quarter-disk sector** (90° MINOR
  arc — the arena forbids the ambiguous semicircle the original spec named).
  `tests/kv12_tier2_arc_extrude.rs`: census V=6/E=9/F=5/χ=2, exact
  `signed_volume = πR²H/4` (≤1e-9 rel), watertight mesh, 1 cylinder patch,
  4 typed rejections (non-minor arc, broken chain, holes→E4, oblique sweep).
  NOT wired to `make_faces` yet (no app/WASM path).
- **E2 — general k-edge single loop. ✅ DONE (2026-06-14).** The E1 assembler
  was ALREADY k-general (it loops over all k edges, dispatching Line/Arc per
  edge) — so E2 added no kernel code, only richer fixtures proving the path:
  a rounded rectangle (4 lines + 4 convex arcs), a vesica lens (two
  CONSECUTIVE arcs at the minimal k=2 loop — exercises the modular seam wiring
  at its smallest), and a square with a CONCAVE arc bite (a cavity-sense
  `reversed` cylinder among line walls). Each: census V=2k/E=3k/F=k+2/χ=2,
  exact `signed_volume = area·H`, watertight mesh, surface census incl. the
  reversed-cylinder count. A literal involute gear tooth is NOT used: involutes
  are not circular arcs, so it would be an approximation adding no new
  code-path coverage beyond the convex/concave/consecutive cases above. All
  pass first run. `tests/kv12_tier2_arc_extrude.rs`.
- **E3 — exact simplicity validation. ✅ DONE (2026-06-14).** `Profile::arc_polygon`
  now rejects a self-intersecting line/arc boundary EXACTLY (`ProfileNotSimple`)
  and pairwise-touching distinct loops (`ProfileLoopsIntersect`).
  **Method (no algebraic-point computation):** every arc here is MINOR (E1
  guarantee), so "point on arc" ⟺ "strictly on the far side of the chord from
  the centre" — a rational orientation test. The ≤2 candidate intersection
  points of a line∩circle / circle∩circle are roots of a rational quadratic in
  one parameter, and segment-interior (`t∈(0,1)`) + arc-side are LINEAR in that
  parameter — so the decision reduces to "does a quadratic root satisfy strict
  linear/interval sign constraints," settled by an exact compare-root-vs-rational
  predicate over `dashu` `RBig` (`exact2d::{cmp_root, arc_segment_interior_cross,
  arc_arc_interior_cross, point_on_closed_arc, segments_properly_cross}`).
  Orchestration in `profile.rs` (`validate_arc_loop` pass 2 + `arc_loops_touch`):
  per-pair endpoint-incidence (non-junction vertex on the other edge) + interior
  crossing, with adjacency-aware permitted shared junctions; plus a two-line
  digon degeneracy guard. **Known narrow gap:** cocircular (concentric) arc
  overlap is reported as no-crossing — a measure-zero config the gear/rounded
  adapter inputs never produce (documented at `arc_arc_interior_cross`). Strict
  hole-inside-outer containment is deferred to E4 (where holes assemble; extrude
  rejects `ArcPolygon` holes until then, so nothing unchecked reaches geometry).
  Tests: 5 exact-predicate unit tests (incl. an adversarial ~1° near-touch and
  the √2 arc∩arc geometry) + 6 profile RED/GREEN cases (line bowtie, arc pierced
  by a diagonal, two-line digon, vertex pinch, hole crossing outer, valid
  loops accepted). `exact2d.rs` + `tests/kv12_tier2_arc_extrude.rs`.
- **E4 — wiring. ✅ DONE (2026-06-14).** `make_faces_from_profiles` reconstructs
  an `ArcPolygon` from `arc_segments` + the authored `vertex_ids` chord polygon
  (`reconstruct_arc_polygon_edges`): each arc run collapses to minor (`< π`)
  sub-arcs split at sample points (`push_minor_subarcs` — a semicircle → 2
  patches; handles arc runs that WRAP the closing vertex, e.g. a line-first
  D-shape), every other edge stays a line. Single arc loops route through the
  exact Tier-2 `Profile::arc_polygon`; **anything that declines — malformed
  segments, off-circle samples, failed simplicity, OR the presence of holes —
  falls back LOUDLY (`eprintln`) to the Tier-1 chord polygon**, so no input
  regresses. Validated: adapter tests (D-shape → 2 cylinder patches; holed-arc
  → Tier-1 fallback, 0 cylinder patches), the repurposed kv8
  `arc_segment_profile_extrudes_to_cylinder_walled_prism` (exact arc-bulge
  volume), and GUI `arc-profile-extrude.spec.js` (D-shape body now has ≤10
  faces — cylinder patches, not ~18 chord walls). WASM rebuilt.

- **E4b — holed arc Tier 2. ✅ DONE (2026-06-14).** A holed arc OUTER now
  extrudes through the exact Tier-2 path (cylinder walls on the outer AND arc
  holes), no longer falling back to Tier-1.
  - **Assembler** (`extrude_arc_profile`, generalized to multi-loop): each hole
    loop is wound CW-around-`+a` (the reverse of the outer), so the SAME
    per-edge generation yields a cap inner loop with the correct opposite
    winding and wall normals pointing INTO the cavity. The caps become annular
    faces (`inner_loops`); the shell genus is the hole count. `validate_solid` +
    exact annulus volume are the gates — it passed first run.
  - **Exact hole containment** (the E3 §5 deferred piece): `point_in_arc_region`
    — exact +x-ray crossing parity, reusing the E3 predicates via a new
    `arc_segment_interior_crossings` (0/1/2 count, tangent → degenerate). A
    boundary vertex on the ray or an arc tangency returns `None`; the caller
    retries other hole vertices (all share status once disjoint) and rejects
    loudly if all are indeterminate — never a silent wrong-containment.
    `Profile::arc_polygon` now enforces hole-inside-outer + non-nesting.
  - **Adapter**: `holes_for` keyed by index; holed `ArcPolygon` outers route
    through Tier-2 (arc holes keep their arcs; polygon/circle holes become line
    edges via `pts_to_line_edges`), falling back LOUDLY to Tier-1 on any decline.
  - Tests: kernel `holed_arc_extrude_annulus_volume_and_genus` (genus 1, exact
    annulus volume, 4 cylinder patches, watertight) + `rejects_hole_outside_outer`
    + `rejects_nested_arc_holes`; adapter `holed_arc_profile_extrudes_tier2_with_cylinder_wall`.
  - **Known narrow gap:** the exact containment ray-cast is decided generically;
    a hole whose every vertex is ray-degenerate vs the outer is rejected (→
    Tier-1 fallback in the adapter), not silently accepted.

## 7. Composition / no-regression

- KV12 Tier 1 (chord polygon) remains the fallback; Tier 2 only supersedes it
  when arcs reconstruct and validate. Existing arc-gear extrudes keep working.
- `make_faces_from_profiles` output stays aligned 1:1 with input profiles (the
  `profile_index` contract); KV14 hole assembly composes (ArcPolygon holes).
- signed_volume / tessellation / render: already curved-aware (no change).
- Booleans on the result: still the KV7 wall — Phase E is extrude-only.

## 8. Acceptance

- E1–E2 volume tests exact to the f32 render band; `validate_solid` green;
  Euler χ = 2 (genus 0).
- E3: exact rejection of self-intersection; no sampled tolerance.
- E4: GUI D-shape + gear extrude produce curved faces; no regression in the
  kv8 / arc / holed-extrude suites; WASM rebuilt.

## 9. Risks

- **E3 exact arc predicates** — the genuine unknown; the rest reuses templates.
  Mitigate: build the filtered→exact cascade incrementally with adversarial
  near-touch fixtures; if a config is beyond the predicate, gate loudly (fall
  back to Tier-1 chords) — never silently mis-validate.
- **Half-edge assembler** — delicate; `validate_solid` + exact volume are the
  gates, mirror `extrude_circle`/`build_partial_revolve` exactly.
- **Oblique sweep** — gated out in v1.
