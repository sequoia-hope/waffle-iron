# KV14 — Ellipse-arc boundary re-entry (degree-4 sub-branch, conic case)

**Milestone:** KV14 (curved partial-patch re-entry), degree-4 boundary sub-branch.
**Status:** SHIPPED 2026-07-09. The 2026-07-09 blocker ("wiring introduces a
WRONG") was resolved by diagnosis, not by code: **R0006's `χ=2 vs 4` was an
ORACLE AUTHORING ERROR (the R0099 pattern), not a kernel defect.** Slab/fiber
analysis of the meta's exact operations proves the oblique circle cut is a true
THROUGH-TUNNEL: the void (cut-cylinder ∩ box, convex ⇒ connected) breaches the
box surface in two disjoint openings on opposite faces (`u+` at cut-axis
t∈[0, 1.26], `u−` at t∈[29.65, 30.92]; both cut caps are only PARTIALLY
embedded — ~60% of cut fibers have box material beyond each cap) ⇒ the cut body
is genus-1 (χ=0). The third boss is fully disjoint (min distance 21.2) ⇒ second
shell. True total χ = 0 + 2 = 2 over 2 shells — exactly what the kernel
produced. `compute_euler_target` documents multi-plane cuts as unpredictable
and returns the genus-0 default, so the meta carried euler_target=2; fixed to
0 (R0006.meta.json), after which the euler oracle expects 2 + shell credit ⇒
R0006 = **SUPPORTED_CORRECT** end-to-end.

Shipped exactly per the design below (re-implemented, prototype was not
retained): yang-rs ellipse chain pre-pass + `loop_polyline` Ellipse arms +
lateral CDT gate admission; kernel-v2 EllipseArc→Ellipse conversion at both
`to_yang_brep` sites (twin-shared). `SurfacePair` stays the typed wall.
Tests: `planar_ellipse_sector_reenters_stage1`,
`planar_full_ellipse_cap_reenters_stage1`,
`lateral_oblique_ellipse_tube_reenters_stage1` (yang),
`ellipse_bounded_tunnel_reentry` (kernel-v2 E2E, the R0006 shape).

Downstream walls the other census cases advance to (UNSUPPORTED → typed
ERROR, the Slice B/C blemish precedent):
- F0076/F0084: `InvalidBooleanOutput("an undirected output edge is not used
  by exactly two directed edges")` / non-2-manifold reassembly.
- F0085: Stage-3 `AmbiguousCurve { candidates: 0, matched: 0 }` + heavy-model
  CDT failure (face 645).
- R0095: `holed lateral CDT failed: CDT backend failed to triangulate` —
  the ellipse-bounded lateral routes but its real-geometry unroll fails; next
  sub-wall of the holed-CDT path.

---
Original plan (retained for the re-implementation):


## Goal

Let a boolean output whose boundary carries an **ellipse-arc** edge
(`Curve::EllipseArc`, the oblique planar section of a cylinder/cone) re-enter
yang-rs Stage 1, so a subsequent boolean on that body succeeds instead of
returning `UnsupportedCurvedBoolean`.

Census finding (probe `KV14_D4_PROBE`): **every** curved-profile degree-4
UNSUPPORTED case is an `EllipseArc` — none are `SurfacePair`. Two face kinds:
- **planar-loop**: a `Surface::Plane` cap whose loop mixes LineSegment + one
  EllipseArc (R0006, F0076, F0081, F0083, F0085).
- **curved-lateral**: a cyl/cone lateral whose boundary carries an EllipseArc
  (R0095, R0061, F0084).

`SurfacePair` boundaries stay the typed wall (a separate, later slice).

## Design

kernel-v2 `Curve::EllipseArc { center, normal, major_axis, major_radius,
minor_radius }` maps **field-for-field** to yang input `Curve::Ellipse` with the
IDENTICAL parameterization `P(t) = C + a·cos t·m̂ + b·sin t·(n̂×m̂)`, CCW around
`normal` from the half-edge origin to its destination. kernel-v2 constructs only
**minor arcs (sweep < π)** or **full ellipses** (`start==end`) — so the
CCW-from-start sweep is unambiguous, exactly like `Curve::Circle` (start≠end =
arc, start==end = full rim). yang already evaluates `Curve::Ellipse` in
`eval_source` (`ellipse_point`), so a sampled interior vertex tagged
`BRepEdge { edge, t }` round-trips.

### yang-rs (Stage 1)
1. **Ellipse chain pre-pass** (mirror the `Curve::Circle` rim block): for each
   `Curve::Ellipse` edge build a shared sample chain in `rim_rings`:
   - arc (`start != end`): `t0 = ellipse_param(start)`, `t1 = ellipse_param(end)`,
     `sweep = (t1−t0) mod 2π` (< π by construction), `m` segments by the same
     chord rule as circles using `major_radius`, interior verts via
     `ellipse_point(·, t)` with `BRepEdge { edge, t }` sources; endpoints reuse
     the B-Rep vertices.
   - full (`start == end`): uniform `m_full` samples from the seam param.
   Self-contained chord bound `d_eps = 1e-2·major_radius` (independent of the
   circle block, since an ellipse-bounded cap may have no circle edge).
2. **`loop_polyline`**: add a `Curve::Ellipse` arm (splice the chain, both the
   single-full-loop and multi-edge-arc forms) — identical shape to `Curve::Circle`.

### kernel-v2 (`to_yang_brep`)
3. At BOTH conversion sites (planar `convert_loop`, lateral
   `convert_lateral_edge`) convert `Curve::EllipseArc` → yang `Curve::Ellipse`,
   twin-shared by `min(h, twin)` (watertight identical chains). `SurfacePair`
   stays the typed wall.

## Branch table

| face surface | loop edge | before | after |
|---|---|---|---|
| Plane | EllipseArc | UnsupportedCurvedBoolean | Ellipse chain → planar CDT |
| Cyl/Cone lateral | EllipseArc | UnsupportedCurvedBoolean | Ellipse chain → unroll CDT |
| any | SurfacePair | UnsupportedCurvedBoolean | UNCHANGED (typed wall) |
| Plane/lateral | Circle/Arc/Line | (unchanged) | (unchanged) |

## Invariants / Oracles

- **Watertight**: a shared ellipse-arc edge samples identically on both incident
  faces (one yang edge, one chain) — no T-junction. Unit test: positional edge
  multiplicity ∈ {1,2}; the ellipse boundary is the count-1 set.
- **On-surface**: every sampled ellipse vertex satisfies the ellipse implicit
  (radial deficit ≤ chord sagitta) and lies in the ellipse plane.
- **Area**: a planar cap bounded by a half/quarter ellipse arc tessellates to the
  analytic segment area within the chord tolerance (≤ analytic, ≥ 0.985·analytic).
- **Assay**: 0 WRONG, no CORRECT regressed. Targets advance out of
  UNSUPPORTED(curved-profile); any that stop at a separate downstream wall are
  ERROR-blemishes justified by the unit tests (the Slice B/C precedent).

## Failure modes

- Near-half arc (sweep ≈ π): kernel-v2 already rejects as ambiguous upstream —
  yang never receives it. yang still uses `rem_euclid` (CCW), correct for < π.
- Ellipse endpoint not on the ellipse (bad input): loud `MalformedTopology`.
