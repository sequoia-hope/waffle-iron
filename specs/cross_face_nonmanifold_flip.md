# Spec: Cross-Face Non-Manifold Edge Flip

## Goal

Extend the non-manifold edge flip repair in the tessellation pipeline to
work across face boundaries. Currently, `flip_nonmanifold_edges_position_based`
only flips diagonals within a single face range. When 3 triangles sharing a
non-manifold edge span different face ranges, the flip is not attempted.

This leaves 1-2 stubborn non-manifold edges in cases where two adjacent faces
independently tessellate with the same interior diagonal.

## Parameters

| Parameter | Type | Description |
|-----------|------|-------------|
| vertices | `&[f32]` | Mesh vertex positions |
| indices | `&mut [u32]` | Triangle index buffer |
| face_ranges | `&[FaceRange]` | Face-to-triangle mapping |

## Branch Table

| Scenario | Current Behavior | New Behavior |
|----------|-----------------|--------------|
| 3 tris share edge, 2 in same face | Flip within face | Same (no change) |
| 3 tris share edge, all different faces | Skip (no flip) | Try cross-face flip |
| 3 tris share edge, cross-face flip invalid | Skip | Skip (convexity check) |
| 2 tris share edge (normal manifold) | No action | No action |

## Invariants

1. **No new non-manifold edges**: The flipped diagonal must not create a new
   non-manifold edge (existing_count < 2 check).
2. **Convex quad check**: The quad formed by the two triangles must be convex
   for the flip to be valid.
3. **Winding consistency**: Flipped triangles preserve consistent winding
   direction with the face normal.
4. **Face range preservation**: Face ranges are NOT modified — the triangle
   remains assigned to its original face range even after flip.

## Oracles

- **Non-manifold count**: Number of non-manifold edges should decrease or stay same
- **Unpaired count**: Number of unpaired edges should not increase
- **Triangle count**: Total triangles unchanged (flip doesn't add/remove)
- **Watertight**: Mesh should be watertight after repair

## Failure Modes

1. **Non-convex quad**: Flip skipped, no change (safe).
2. **New non-manifold edge**: Flip skipped, no change (safe).
3. **Degenerate triangle**: Flip may create zero-area triangles (handled by
   subsequent degenerate removal pass).

## Research Basis

Standard mesh diagonal flip (Lawson flip). No novel algorithm.
Ref #33 Stroud: mesh topology repair via local operations.
