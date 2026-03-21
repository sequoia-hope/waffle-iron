# Steiner-Fan Re-tessellation for Non-Manifold Interior Diagonals

## Goal

Eliminate non-manifold interior edges in bounded tessellation by re-tessellating
affected faces with centroid-fan triangulation. When earcut independently
creates the same interior diagonal in two adjacent faces (sharing corner
vertices but no B-Rep edge), the resulting 3-triangle-per-edge non-manifold
condition is resolved by replacing one face's earcut output with a fan from the
face centroid — a point unique to each face, guaranteeing no shared interior
edges.

## Parameters

| Parameter | Type | Default | Description |
|-----------|------|---------|-------------|
| N/A | — | — | Automatic: activates only when non-manifold interior edges detected |

No user-facing parameters. The algorithm runs automatically as a post-processing
step in `tessellate_solid_bounded`.

## Branch Table

| Condition | Action | Expected Outcome |
|-----------|--------|------------------|
| No non-manifold interior edges | Skip (no-op) | Mesh unchanged |
| Non-manifold interior edge, face centroid inside polygon | Re-tessellate face with centroid fan | Non-manifold edge eliminated |
| Non-manifold interior edge, face centroid outside polygon | Keep earcut output (fallback to flip/aggressive) | No change; existing repair passes handle |
| Re-tessellation introduces new non-manifold | Not possible (centroids are unique per face) | N/A |
| Face has inner loops (holes) | Skip re-tessellation (centroid fan doesn't handle holes) | Fallback to existing repair |

## Invariants

1. **Triangle count conservation (per face)**: Centroid-fan produces exactly N
   triangles for an N-vertex boundary (vs N-2 from earcut). Net triangle count
   increase is exactly 2 per re-tessellated face.

2. **Surface area preservation**: Fan triangles cover the same polygon area as
   earcut triangles (both triangulate the same boundary polygon). Total area
   delta < TAU_WORK.

3. **No shared interior diagonals**: Given face A with centroid C_A and face B
   with centroid C_B, where C_A ≠ C_B (guaranteed for faces with different
   shapes/positions), the interior edges {C_A → V_i} and {C_B → V_j} never
   coincide. Shared edges only occur on the boundary {V_i → V_{i+1}}, which are
   B-Rep edges with exactly 2 adjacent faces.

4. **B-Rep boundary preservation**: Boundary edges (between consecutive boundary
   vertices) are unchanged. Only interior diagonals are replaced.

5. **Winding consistency**: Fan triangles are wound consistently with the face
   normal (centroid → V_i → V_{i+1} follows the boundary winding direction).

## Oracles

- **Primary**: Unpaired edge count = 0 for closed solids (watertight check).
- **Secondary**: Every mesh edge has exactly 2 adjacent triangles (manifold check).
- **Regression**: Total triangle count ≥ original earcut count (centroid-fan
  adds 2 triangles per re-tessellated face).
- **Geometry**: Bounding box unchanged (vertices are subset of original + centroid
  which is interior).

## Failure Modes

1. **Centroid outside polygon** (non-convex face): Centroid-fan would produce
   inverted triangles. **Mitigation**: Point-in-polygon test using winding
   number; skip re-tessellation if centroid is outside.

2. **Face with holes**: Centroid-fan doesn't handle inner loops. **Mitigation**:
   Skip faces with inner_boundaries.len() > 0; these are rare in boolean results.

3. **Degenerate face** (collinear vertices): Centroid on the line; fan produces
   zero-area triangles. **Mitigation**: Check polygon area > TAU_WORK before
   re-tessellation.

## Research Basis

- **Ear clipping** [O'Rourke 1998, REFERENCES.md #35]: Standard polygon
  triangulation. Non-unique diagonal choice is the root cause of cross-face
  conflicts.
- **Steiner point insertion** [Shewchuk 1997, REFERENCES.md #4]: Adding interior
  points to control triangulation topology. The centroid is the simplest
  Steiner point that guarantees uniqueness across faces.
- **Mesh repair** [Cherchi et al. 2025, REFERENCES.md #31]: Non-manifold edge
  elimination in mesh arrangements. Our approach is simpler (no edge collapse)
  because we can re-tessellate from the B-Rep boundary.

## Analytical vs. Approximate Method Justification

- **Method**: This is a tessellation repair technique, not a surface-surface
  intersection operation. No SSI is involved.
- **Surface pairs**: N/A — tessellation operates on already-computed B-Rep faces.
- **A15 compliance**: No impact on analytical primacy; boolean operations remain
  exact SSI for quadric surfaces.
