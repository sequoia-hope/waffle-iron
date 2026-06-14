# KV12 Tier 2 Spec — exact arc-segment profile extrude (cylinder side patches)

Status: PLANNED (spec). Prototype-release Phase E.
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

- **E1 — single-arc assembler + caps.** `extrude` handles `ArcPolygon` for a
  one-arc-one-or-more-lines single loop (the D-shape: diameter line + semicircle
  arc). Direct kernel test: `validate_solid` green, watertight, **volume =
  planar cap area × depth EXACT** (cap area via `geom::signed_volume` of the
  arc-bearing planar face), 1 cylinder patch present (`Surface::Cylinder` with
  exact radius). NOT wired to `make_faces` yet.
- **E2 — general k-edge single loop.** Multiple arcs + lines (a gear flank built
  from arcs). Generalize the seam/twin loop to k edges. Test on a rounded
  polygon and a real arc-built gear tooth.
- **E3 — exact simplicity validation** (§5). RED self-intersecting arc loops →
  `ProfileNotSimple`; GREEN valid ones. Adversarial: a near-touching arc pair.
- **E4 — wiring + holes.** Adapter reconstructs `ArcPolygon` from `arc_segments`
  (§3) and routes through Tier 2 when reconstruction+validation succeed, else
  the KV12 Tier-1 chord polygon (loud fallback). Arc-bearing holes (KV14 path,
  ArcPolygon holes). GUI E2E: the `arc-profile-extrude.spec.js` D-shape now
  yields a body with a cylinder face (face count / curved face check), and a
  real gear extrudes with cylindrical fillet walls.

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
