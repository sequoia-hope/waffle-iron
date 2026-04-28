# Yang Pipeline B-Rep Reassembly Fix

## Goal

Fix `build_result_brep()` in `topology_extract.rs` so that the B-Rep topology
produced by the Yang hybrid boolean pipeline satisfies:

1. **Euler characteristic**: V - E + F = 2 for closed manifold solids
2. **Manifold edges**: every edge has exactly 2 half-edges (HE = 2 × E)
3. **Valid twin pairing**: every half-edge has a valid twin

Currently, `build_result_brep()` produces V=14, E=10, F=10 (V-E+F=14) for an
overlapping box-box subtract, with 32 half-edges instead of the expected 20.

## Parameters

No new parameters. This is a bug fix to the existing topology construction.

## Branch Table

| Condition | Expected Behavior |
|-----------|------------------|
| Two overlapping boxes, Subtract | V-E+F=2, HE=2×E, all twins paired |
| Two overlapping boxes, Union | V-E+F=2, HE=2×E, all twins paired |
| Two overlapping boxes, Intersect | V-E+F=2, HE=2×E, all twins paired |
| Non-overlapping boxes, Intersect | Empty topology (0 faces) |
| Empty trim map | Empty topology (existing, works) |

## Invariants

1. For any closed manifold result: V - E + F = 2 (Euler's formula for genus-0)
2. HE = 2 × E (every edge has twin)
3. Every half-edge's twin.twin = self
4. Every half-edge's origin = twin.next.origin (manifold consistency)
5. Face provenance covers all faces
6. Edge classification covers all edges

## Oracles

- `test_brep_euler_characteristic`: V-E+F=2 for box-box subtract
- `test_brep_all_ops`: V-E+F=2 for all three ops
- `test_brep_manifold_edges`: HE=2×E for box-box subtract

## Root Cause Analysis

The twin-pairing step (Step 5 of `build_result_brep`) matches half-edges by
looking up `directed_he[(v0,v1)]` and `directed_he[(v1,v0)]`. A match requires
that both the forward and reverse directed edges exist in the global map.

Failure modes:
1. **Vertex index mismatch**: Two faces share a geometric edge but reference
   different vertex indices for the same point (subdivision duplicated vertices)
2. **Missing reverse edge**: A face's boundary includes an edge that no other
   face references in the opposite direction (open boundary)
3. **Duplicate directed edges**: Two faces reference the same (v0,v1) pair,
   causing the HashMap to overwrite the first entry

## Failure Modes

| Condition | Expected Error |
|-----------|---------------|
| Non-manifold edge (>2 faces) | Return Err (topology validation) |
| Open boundary (unpaired edge) | Return Err (not a closed solid) |

## Research Basis

- [#24] Yang et al. 2025 — Stage 3 B-Rep reconstruction requires conformal mesh
- [#16] Mantyla 1988 — Euler formula V-E+F=2 for closed manifold solids
- [#33] Stroud 2006 — B-Rep topological consistency validation
- [#9] Cherchi 2020 §5 (arrangement) — conformal subdivision ensures vertex
  sharing at boundaries. The whole-pipeline 2022 variant ([#38] Cherchi 2022)
  is what Yang 2025 §4.2/§4.4.2 cites.

## Analytical vs. Approximate Method Justification

This fix improves the exact topology construction — no approximation involved.
The analytical primacy invariant (A15.1) is unaffected.
