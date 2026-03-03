# D2: Shrunk Ranges

**Status:** SPEC ONLY (2026-03-03) — No implementation until D1 Phase 5 is stable.
**Depends on:** D1 (pave block integration complete and stable)
**Replaces:** The 7-tolerance system (`tau_weld`, `tau_boundary`, `tau_edge_cluster`, `tau_area`, `tau_coplanar`, `tau_mesh`, `tau_model`)

---

## Problem

`BooleanTolerance` derives 7 tolerance values from `tau_model` via hand-tuned scaling factors:

| Field | Scale Factor | Purpose | Issue |
|-------|-------------|---------|-------|
| `tau_model` | 1.0x | Base tolerance | OK |
| `tau_weld` | 0.4x | Vertex merging | Merges across narrow bosses |
| `tau_boundary` | 0.5x | IC filtering | Too tight for coarse IC approximation |
| `tau_edge_cluster` | 5.0x | Edge pairing | Destroys fine features |
| `tau_area` | tau^2 | Face area threshold | Scale-dependent |
| `tau_coplanar` | 1.0x | Coplanar detection | OK |
| `tau_mesh` | min(tol, TOLERANCE) | Mesh resolution | OK |

These interact unpredictably:
- V2 assembly Level 2 (currently 2.0x after C2 reduction) merges vertices across features
- `tau_boundary = 0.5x` is too tight for analytically-generated IC polylines
- `tau_edge_cluster = 5.0x` finds false matches, destroying fine geometry

## Solution: Per-Pave-Block Shrunk Ranges

Each `PaveBlock` computes a **shrunk range** — the parametric interval reduced by
the tolerance spheres of its bounding vertices:

```
shrunk_start: C(t) where dist(C(t), V_front) = Tol(V_front) + Tol(C)
shrunk_end:   C(t) where dist(C(t), V_back)  = Tol(V_back)  + Tol(C)
```

The shrunk range defines where interference can actually occur. Portions inside
tolerance spheres are topologically part of the vertex, not the edge.

### Empty Shrunk Range

If tolerance spheres consume the entire edge (shrunk_start > shrunk_end), the
bounding vertices merge into a single same-domain vertex:

```
V_front --[tol sphere]-- edge --[tol sphere]-- V_back
         \____overlap____/  → vertices merge
```

### Data Model Addition

```rust
pub struct PaveBlock<C> {
    // ... existing fields ...
    pub shrunk_range: Option<(f64, f64)>,  // None = not yet computed
    pub vertex_tol_front: f64,             // Tolerance of front vertex
    pub vertex_tol_back: f64,              // Tolerance of back vertex
    pub edge_tol: f64,                     // Tolerance of edge curve
}
```

## Tolerance Replacements

| Current | Replaced By | Mechanism |
|---------|------------|-----------|
| `tau_weld` | Shrunk range emptiness | If shrunk range is empty, vertices merge |
| `tau_boundary` | Shrunk range filtering | IC only interferes within shrunk range |
| `tau_edge_cluster` | Shrunk range overlap | Edges pair when shrunk ranges overlap |
| `tau_area` | Face-level aggregate | Sum of edge shrunk ranges defines face tolerance |

After D2, `BooleanTolerance` simplifies to:

```rust
pub struct BooleanTolerance {
    pub tau_model: f64,    // Base geometric tolerance
    pub tau_mesh: f64,     // Mesh resolution for tessellation
    pub tau_coplanar: f64, // Coplanar face detection
}
```

## Algorithm: `fill_shrunk_data()`

For each `PaveBlock`:

1. Compute vertex tolerances:
   - `Tol(V) = max(tau_model, max_over_incident_edges(dist(V, edge_midpoint) * epsilon))`
   - Simplified: `Tol(V) = tau_model` initially (vertex-specific tolerances deferred)

2. Compute edge tolerance:
   - `Tol(C) = max(tau_model, max_deviation_from_original_curve)`
   - For IC curves: deviation from analytical intersection
   - For boundary curves: typically 0 (exact)

3. Binary search for shrunk parameters:
   - `shrunk_start = argmin_t { dist(C(t), V_front) > Tol(V_front) + Tol(C) }`
   - `shrunk_end = argmax_t { dist(C(t), V_back) > Tol(V_back) + Tol(C) }`

4. Validate: `shrunk_start < shrunk_end` (non-empty)

## Implementation Plan

| Step | Description | Depends On |
|------|-------------|-----------|
| D2.1 | Add `shrunk_range`, `vertex_tol_*`, `edge_tol` to `PaveBlock` | D1 stable |
| D2.2 | Implement `fill_shrunk_data()` | D2.1 |
| D2.3 | Replace `tau_weld` with shrunk-range vertex merging | D2.2 |
| D2.4 | Replace `tau_boundary` with shrunk-range IC filtering | D2.2 |
| D2.5 | Replace `tau_edge_cluster` with shrunk-range edge pairing | D2.2 |
| D2.6 | Remove unused tolerance fields from `BooleanTolerance` | D2.3-5 |
| D2.7 | Add tolerance sensitivity tests | D2.6 |

## Verification

- All boolean tests pass
- `BooleanTolerance` has 3 fields (was 7)
- No 5.0x escalation in V2 assembly
- Tolerance sensitivity tests: 0.001-unit and 10000-unit scale models produce valid results

## References

- OCCT: Shrunk ranges in `IntTools_ShrunkRange`, computed per `IntTools_Range` (pave block parametric interval)
- OCCT: `BOPAlgo_PaveFiller::PerformSZ()` — fills shrunk data after pave block creation
- OCCT: `BOPTools_AlgoTools::ComputeVV()` — vertex-vertex interference using vertex tolerances
