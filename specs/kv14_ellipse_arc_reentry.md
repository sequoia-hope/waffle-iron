# KV14 — Ellipse-arc boundary re-entry (degree-4 sub-branch, conic case)

**Milestone:** KV14 (curved partial-patch re-entry), degree-4 boundary sub-branch.
**Status:** BLOCKED — prototyped 2026-07-09, reverted (P9/P10). The yang-rs Stage-1
ellipse ingestion is correct in isolation (unit test `planar_ellipse_sector_
reenters_stage1`: a planar elliptical sector re-enters and tessellates watertight,
chorded area = analytic `½·a·b·Δt`). But **wiring it into kernel-v2 converts ZERO
assay cases and introduces a WRONG**, so it was reverted:

- **R0006 → SUPPORTED_WRONG** (was UNSUPPORTED). The boolean now COMPLETES but
  produces `χ=2` (one closed shell) where the oracle expects `χ=4` (2 shells).
  R0006 = rect boss + OBLIQUE circle cut (the ellipse) + oblique circle boss. The
  degree-4 wall was MASKING a downstream boolean-assembly / shell-count defect on
  ellipse-bounded bodies — re-entry unmasks it. Not diagnosed (deep; multi-body
  connectivity). Per P9 a WRONG cannot ship.
- **F0076, F0081, F0083 → ERROR** (InvalidBooleanOutput / BooleanFailed): route +
  build but fail a later boolean/validation.
- **R0095 (curved lateral) → ERROR**: routes but the unroll/assembly fails downstream.

**The `shared_edges` map IS shared across the planar + lateral conversion sites**
(boolean.rs:180) so a cap∩lateral ellipse arc is conformally shared (one yang edge,
one chain) — NOT the cause of the WRONG. The wall is in the boolean/reassembly of
ellipse-bounded topology, a SEPARATE milestone from Stage-1 ingestion.

**Next session:** diagnose R0006's `χ=2 vs 4` on LIVE code (is it a genuine
shell-merge bug, a non-conformal ellipse junction with the box faces, or an oracle
authoring error like R0099?) BEFORE re-wiring. The Stage-1 ingestion code is in the
git history of this session's exploration if needed. Do NOT re-wire kernel-v2 until
a case converts CORRECT with 0 WRONG (P4/DoD).

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
