# Yang Pipeline Phase 3, Task 3b — Trim Boundary Extraction

**Parent spec**: `specs/yang_hybrid_migration.md` (Phase 3)
**Governance**: ARCHITECTURAL_INVARIANTS.md A15.6
**File**: `crates/kernel/src/boolean/topology_extract.rs`

---

## 1. Goal

For each surviving source face in the `FaceSurvivalMap`, extract the **trim
boundary** — the set of directed edges that form the boundary of the surviving
region on that face. These edges fall into two categories:

- **Intersection edges**: Edges where two sub-triangles from *different* source
  faces are adjacent. These are new edges created by the boolean operation.
- **Original boundary edges**: Edges of surviving sub-triangles that have no
  adjacent surviving sub-triangle (they were already on the original face boundary
  or are exposed by partial trimming).

The trim boundaries are closed loops of directed edges. A surviving face that is
fully preserved (not trimmed) has a single boundary loop matching the original
face outline. A partially trimmed face has boundary loops that include intersection
edges.

**User-visible behavior**: None (internal pipeline infrastructure). This connects
Task 3a output to Task 3c (connectivity extraction / B-Rep assembly).

---

## 2. Parameters

### Inputs

| Parameter | Type | Description |
|-----------|------|-------------|
| `subdivided` | `&SubdividedMesh` | Phase 2 subdivided mesh with all sub-triangles |
| `survival` | `&FaceSurvivalMap` | Phase 3a face survival groups |

### Output

| Parameter | Type | Description |
|-----------|------|-------------|
| `TrimBoundaryMap` | struct | Maps each SourceFace to its trim boundary loops |

### Constants

No new tolerance constants. This is topological (index-based) edge extraction.

---

## 3. Branch Table

| Condition | Behavior |
|-----------|----------|
| Edge shared by two surviving sub-tris in the SAME face group | Interior edge — skip |
| Edge shared by surviving sub-tri in face A and surviving sub-tri in face B | Intersection trim edge — include |
| Edge of surviving sub-tri with no adjacent surviving sub-tri | Boundary trim edge — include |
| Face group has zero trim edges (all interior) | Impossible for a valid triangulation (every face has a boundary) |
| Multiple boundary loops on one face (e.g., a face with a hole) | Return multiple TrimLoop entries |
| Flipped sub-triangles (Subtract B-faces) | Edge direction reversed to match outward winding |

---

## 4. Invariants

1. **Closed loops**: Every trim boundary loop must be closed — the endpoint of
   each directed edge must be the startpoint of the next edge in the loop.

2. **Completeness**: Every surviving face group produces at least one boundary
   loop.

3. **No interior edges**: No edge that is shared by two surviving sub-triangles
   from the same face group appears in the trim boundary.

4. **Euler compatibility**: For a simply-connected surviving face region, there
   is exactly one boundary loop. For a face with holes, there is one outer loop
   plus one inner loop per hole.

5. **Vertex sharing at intersections**: Trim boundary edges from adjacent
   surviving faces share vertex indices at intersection points (they reference
   the same vertices in `SubdividedMesh.verts`).

---

## 5. Oracles

| Test | Oracle |
|------|--------|
| Box-box subtract: partially trimmed face | At least one face has a boundary loop containing intersection edges |
| Boundary loop closure | For every loop, `edges.last().v1 == edges.first().v0` |
| Total boundary edges | Sum of boundary edges across all face groups == 2 * (number of intersection edges) + original boundary edges |
| No duplicate edges | No directed edge (v0, v1) appears twice in the same face's boundaries |
| Conservation | Every surviving sub-triangle contributes edges to exactly one face group's boundary extraction |

---

## 6. Failure Modes

| Failure | Handling |
|---------|----------|
| Empty FaceSurvivalMap | Return empty TrimBoundaryMap |
| Sub-triangle vertex index out of range | Panic (indicates corrupt Phase 2 data) |
| Boundary edges fail to form closed loops | Return the unchained edges and set a diagnostic flag — indicates a mesh topology issue from Phase 2 |

---

## 7. Research Basis

- **[#24] Yang, Jia & Yan (2025)**: Stage 3 topology extraction — trim boundaries
  are the interface between surviving face regions and define the new B-Rep edges.
- **[#9] Cherchi et al. (2020) §5 (arrangement)**: The subdivided mesh's edge
  adjacency enables exact determination of which edges are intersection curves
  vs original boundaries. (See also [#38] Cherchi 2022 §5 for the per-patch
  arrangement output that Yang 2025 stage 2 produces.)

### 7a. Analytical vs. Approximate Method Justification

**Method**: Exact (topological edge adjacency on integer indices).

This task performs no geometric computation. It identifies boundary edges by
checking adjacency in the sub-triangle connectivity. All operations are on
vertex indices, so no approximation is involved.

---

## 8. Data Structure Design

```rust
/// A directed edge in a trim boundary.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub(crate) struct TrimEdge {
    /// Start vertex index in SubdividedMesh.verts.
    pub v0: usize,
    /// End vertex index in SubdividedMesh.verts.
    pub v1: usize,
    /// Whether this edge is an intersection edge (new) or original boundary.
    pub is_intersection: bool,
}

/// A closed loop of directed trim edges bounding a surviving face region.
#[derive(Debug, Clone)]
pub(crate) struct TrimLoop {
    /// Ordered directed edges forming a closed loop.
    /// edges[i].v1 == edges[(i+1) % len].v0
    pub edges: Vec<TrimEdge>,
}

/// Maps each surviving source face to its trim boundary loops.
#[derive(Debug)]
pub(crate) struct TrimBoundaryMap {
    /// Keyed by SourceFace, value is the boundary loops for that face.
    pub boundaries: BTreeMap<SourceFace, Vec<TrimLoop>>,
}
```

## 9. Algorithm Sketch

1. **Build edge adjacency**: For all sub-triangles (surviving and non-surviving
   from both meshes), build a map from undirected edge `(min(v0,v1), max(v0,v1))`
   to the list of sub-triangles sharing that edge + their source face info.

2. **Mark surviving sub-triangles**: Create a set of all surviving sub-triangle
   indices (from the FaceSurvivalMap).

3. **Extract boundary edges per face group**: For each source face's surviving
   sub-triangles, for each edge of each sub-triangle:
   - If the adjacent sub-triangle is NOT in the same face group (either different
     face, different mesh, or not surviving), this is a boundary/trim edge.
   - Record the directed edge (respecting winding order of the sub-triangle,
     accounting for flipped winding in Subtract B-faces).

4. **Chain edges into loops**: For each face group's boundary edges, chain them
   into closed loops by following `v1 → v0` links. Multiple loops indicate holes.
