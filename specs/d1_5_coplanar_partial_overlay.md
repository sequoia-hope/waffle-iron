# D1.5: Coplanar Partial Overlap via 2D Polygon Overlay

## Status: DEFERRED — injection mechanism incompatible

## Research Basis

- **[#26] Yang & Jia (2025)** — Overlap is a 2D phenomenon with 1D boundary. Bilevel optimization computes the overlap boundary as a shared topological entity from the start. Our `add_independent_loop` approach creates isolated wires, which is structurally wrong.
- **[#8] Zhou et al. (2016)** — CDT clustering for coplanar mesh triangles.
- **[#3] OpenCASCADE** — Same-domain analysis and connexity chains for coplanar face pairs.
- **Deviation from [#26]**: We used iOverlay 2D polygon boolean instead of bilevel optimization. The 2D boolean correctly computes overlap regions, but `inject_overlay_fragments` fails because it creates topologically isolated wires rather than splicing into existing boundary wires.

## Root Cause

The boolean pipeline's `inject_coplanar_boundary_loops` in `loops_store/mod.rs` handles
three coplanar face configurations:

1. **Mutual containment** (identical extent): detected and classified directly
2. **Full containment** (j_in_i or i_in_j): boundary wires injected as holes
3. **Partial overlap** (neither fully contains the other): **unhandled** — the code
   does nothing, punting to adjacent-face intersection curves (ICs)

The partial overlap case fails because adjacent-face ICs produce figure-8 vertices
and unclosed shells. The boundary between overlapping and non-overlapping regions of
the coplanar faces is never explicitly computed.

## Solution (Original Plan)

Wire the existing `compute_coplanar_overlay()` (in `coplanar_overlay.rs`) into the
partial overlap branch. This function:

1. Projects both coplanar faces into a shared 2D coordinate system
2. Runs iOverlay polygon boolean (Intersect / Difference / InverseDifference)
3. Returns classified fragments (And = overlap region, Or = non-overlap region)

The fragments would be injected into loops_stores, providing geometrically exact
overlap boundaries. Adjacent-face ICs would then be skipped.

## What Was Implemented

### Infrastructure (COMPLETE)
- `partial_overlap_pairs` field in `LoopsStoreQuadruple` — detects and records
  partially overlapping coplanar face pairs for downstream use
- `inject_overlay_fragments` with closure-based API — generic injection function
  for `Alternative<C, IC>` curves without requiring `From<BSplineCurve>` on `C`
- `inject_overlay_fragments_poly` — parallel injection for PolylineCurve stores
- `contour_to_wire` / `contour_to_poly_wire` — 2D contour → 3D wire conversion
- `From<Line<V::Point>> for NurbsCurve<V>` — utility conversion in truck-geometry
- `Alternative::from_first_via` — wraps values through FirstType conversion
- Relaxed `S` bounds on `compute_coplanar_overlay` (doesn't use surface methods)

### Injection (DEFERRED)
The injection mechanism uses `add_independent_loop` which creates topologically
isolated wires (new vertices/edges) that don't share topology with the existing
face boundary. This causes shell assembly failures because:

1. **No vertex sharing**: overlay edges at the overlap boundary don't connect to
   edges on adjacent non-coplanar faces
2. **Adjacency skipping is too aggressive**: with multiple coplanar pairs (e.g.,
   same-sized boxes have 4+ coplanar face pairs), skipping adjacencies suppresses
   all ICs, leaving no topology to connect with
3. **Replace approach fails**: clearing IC wires and injecting overlay wires breaks
   the face network because adjacent faces still reference the cleared IC vertices

### What's Needed (Future Work)
To make overlay injection work, `inject_overlay_fragments` needs to **splice edges
into existing boundary wires** (like IC's `add_edge` does), sharing vertices at
intersection points. This requires:

1. Finding where overlay boundary edges intersect the face's existing boundary wire
2. Splitting the boundary wire at those intersection points
3. Creating overlay edges that share vertices with the split boundary
4. Maintaining consistent orientation and edge sharing with adjacent faces

This is a significant refactor of the injection mechanism.

## Affected Tests (Still Failing — Pre-existing)

- **CM1** (commutativity): 10x10x10 box + 8x8x8 box offset [4,4,0]
- **T3** (no NaN): 10x10x10 + 8x8x8 offset [5,5,0]
- **MV1** (inclusion-exclusion): 10x10x10 + 10x10x10 offset [5,5,0]

## Files Modified

| File | Change |
|------|--------|
| `vendor/truck/truck-shapeops/src/transversal/coplanar_overlay.rs` | Add `inject_overlay_fragments_poly`, `contour_to_poly_wire`, closure-based API |
| `vendor/truck/truck-shapeops/src/transversal/loops_store/mod.rs` | Add `partial_overlap_pairs` detection, `LoopsStoreQuadruple` field |
| `vendor/truck/truck-shapeops/src/transversal/integrate/mod.rs` | Receive `partial_overlap_pairs`, deferred injection code |
| `vendor/truck/truck-shapeops/src/alternative.rs` | Add `from_first_via` method |
| `vendor/truck/truck-geometry/src/nurbs/nurbscurve.rs` | Add `From<Line>` for `NurbsCurve` |
