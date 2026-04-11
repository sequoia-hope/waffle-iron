# Spec: Yang Stage 0 — Coplanar Face Preprocessing

## Goal

Implement Yang 2025 Section 4.5.5 coplanar face preprocessing. Before the
mesh boolean, detect coplanar face pairs between the two operand solids and
generate identical mesh triangulations in the overlap region. This eliminates
conformal edge explosions (F0063: 3,357 shared edges → timeout) and incorrect
face survival for coplanar geometry.

## Research Basis

- Yang et al. 2025 [#24] Section 4.5.5: "it is necessary to check coplanar
  planes and perform 2D Boolean operations before mesh discretizations"
- Yang & Jia 2025 [#26]: Overlap region extraction for B-Rep models
- Ref [#33] Stroud Ch.4: Newell normal, polygon representation

## Parameters

None — this is pipeline infrastructure with no user-facing parameters.

## Algorithm

### Step 0a: Detect Coplanar Face Pairs

For each planar face in solid_a and each planar face in solid_b, compare plane
equations (normal direction + signed offset). Two faces are coplanar if:
- Normals are parallel or anti-parallel: `|dot(n_a, n_b)| > 1 - TAU_PARALLEL`
- Planes are coincident: `|offset_a - offset_b| < TAU_MODEL` (after sign alignment)

Use existing `classify_coplanarity()` from `clip.rs` or direct comparison.

### Step 0b: 2D Polygon Boolean (i_overlay)

For each coplanar pair, project both face boundaries into the shared plane's 2D
coordinate system using `compute_plane_basis()`. Use `i_overlay` to compute:

- **Overlap region**: intersection of polygon_a and polygon_b
- **A-only region**: polygon_a - polygon_b (subtraction)
- **B-only region**: polygon_b - polygon_a (subtraction)

Per the Yang paper Fig. 16: "Two coplanar planes will be segmented into three
parts after a Boolean operation in 2D."

### Step 0c: Shared Conformal Triangulation

Triangulate the UNION boundary of both face polygons using `earcutr`. Both meshes
receive identical triangles for the overlap region. The overlap boundary vertices
become shared between the A-only, B-only, and overlap triangulations — ensuring
conformal mesh boundaries.

### Step 0d: Mesh Replacement

Replace original mesh_a and mesh_b triangles for the coplanar faces with the
new shared triangulation. Update bijective maps to reflect the new triangle→face
mapping.

## Branch Table

| Face pair type | Action |
|---|---|
| Anti-parallel coplanar planar (stacked caps) | Detect, 2D boolean, conformal inject |
| Same-direction coplanar planar (overlapping) | Detect, 2D boolean, conformal inject |
| Non-coplanar | Skip |
| Curved coplanar (cylinder-cylinder, etc.) | Skip (future: Yang [#26]) |

## Invariants

1. After preprocessing, coplanar face pairs have vertex-identical mesh triangulations
2. Bijective maps correctly reflect shared triangulations
3. No conformal edge explosion: mesh boolean sees identical triangles → zero intersection computation on coplanar regions
4. Non-coplanar faces are completely unaffected
5. The i_overlay 2D boolean produces watertight region boundaries

## Oracles

- Stacked box Union: conformal cross-mesh shared edges = 0 on shared plane
- Stacked box Union: correct face count, euler=2
- F0063 pattern (5 stacked extrusions): completes within 30s timeout (was >90s)

## Failure Modes

- If face_geometry is incomplete (no Planar entry): skip the face pair (not all faces
  have geometry, especially post-boolean results)
- If i_overlay produces empty overlap: the faces are coplanar but don't overlap.
  Skip — no shared mesh needed
- If earcutr fails on the union polygon (non-simple polygon): fall back to fan
  triangulation from centroid

## Dependencies

- `i_overlay = "4.4"` added to `crates/kernel/Cargo.toml`
- Existing: `earcutr = "0.4"` (triangulation), `compute_plane_basis` (vecmath.rs)
