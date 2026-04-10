# Spec: Yang Coplanar Interior Face Elimination

## Goal

Eliminate internal anti-parallel coplanar faces from Yang boolean Union results.
When two stacked solids share a cap plane (e.g., top of extrude₁ = bottom of
extrude₂), both caps are internal to the merged solid and should not survive.

## Root Cause

`face_survival_detect()` keeps A's `CoSurfaceInside` sub-triangles for Union
(line 1798), treating them as shared outer boundary. But for anti-parallel
coplanar faces (stacked boxes), the A cap is internal, not external.

The labeling for anti-parallel faces:
- A's top cap (normal +Z): offset by -Z → into B → `CoSurfaceInside` → survives
- B's bottom cap (normal -Z): `CoSurfaceInside` but B only keeps `Outside` → dropped

Result: A's internal cap persists, creating doubled geometry.

## Parameters

- `op: MeshBooleanOp` — new parameter to `merge_coplanar_face_groups()` (currently
  doesn't receive the operation type)

## Branch Table

| Coplanar pair | Normal relationship | Operation | Action |
|---|---|---|---|
| A + B faces on same plane | Anti-parallel (dot < -0.9) | Union | Remove all faces in bucket |
| A + B faces on same plane | Parallel (dot > 0.9) | Union | Merge as before (shared boundary) |
| A-only or B-only | N/A | Any | No change |
| Any | Any | Subtract/Intersect | No change |

## Invariants

1. For Union: no surviving face has anti-parallel coplanar partner from other mesh
2. Parallel coplanar faces (identical/overlapping boxes) still merge correctly
3. Subtract and Intersect behavior unchanged
4. Stacked box Union produces single closed solid without internal faces

## Oracles

- Stacked box Union: `face_provenance.len()` = 10 (combined box faces)
- No self-intersection oracle failures from doubled coplanar geometry
- Identical box Union: merged coplanar face groups still present

## Failure Modes

- If threshold too aggressive (dot < -0.5): could eliminate near-coplanar faces
  that aren't truly anti-parallel. Use tight threshold: dot < -0.9.
- If raw normals not computed correctly: could miss anti-parallel pairs.
  Use Newell normal from first sub-triangle (same as existing code).

## Research Basis

- Co-surface elimination in mesh boolean: [#24 Yang et al. 2025, Stage 3]
- Interior face detection via normal direction: standard B-Rep boolean
  [#1 Patrikalakis, Ch. 5]
