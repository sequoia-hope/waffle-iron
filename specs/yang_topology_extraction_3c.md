# Yang Pipeline Phase 3, Task 3c — Connectivity Extraction

**Parent spec**: `specs/yang_hybrid_migration.md` (Phase 3)
**Governance**: ARCHITECTURAL_INVARIANTS.md A15.6
**File**: `crates/kernel/src/boolean/topology_extract.rs`

---

## 1. Goal

From the `TrimBoundaryMap` (task 3b) and `SubdividedMesh` (Phase 2), build the
half-edge B-Rep topology of the boolean result using Euler operators. This is the
critical step that converts the mesh boolean result back into a proper B-Rep
representation.

Each face in the result corresponds to a surviving source face (tracked via
`SourceFace` provenance). Each edge is classified as either an **original boundary
edge** (from an original B-Rep face boundary) or an **intersection edge** (newly
created by the boolean operation).

**User-visible behavior**: None (internal pipeline infrastructure). This connects
Task 3b output to Phase 4 (SSI refinement) and Phase 5 (B-Rep assembly).

---

## 2. Parameters

### Inputs

| Parameter | Type | Description |
|-----------|------|-------------|
| `trim_map` | `&TrimBoundaryMap` | Phase 3b trim boundary loops per face |
| `subdivided` | `&SubdividedMesh` | Phase 2 subdivided mesh with vertex positions |
| `survival` | `&FaceSurvivalMap` | Phase 3a face provenance |

### Output

| Parameter | Type | Description |
|-----------|------|-------------|
| `ResultTopology` | struct | Half-edge B-Rep + provenance + edge classification |

### Constants

No new tolerance constants. This is a purely topological construction using
integer vertex indices and Euler operators.

---

## 3. Branch Table

| Condition | Behavior |
|-----------|----------|
| Empty TrimBoundaryMap | Return empty ResultTopology |
| Single face, single loop | Build single-face solid (degenerate but valid) |
| Multiple faces, all single loops | Standard manifold construction via spanning tree + mef |
| Face with multiple loops (hole) | Outer loop defines face boundary; inner loops use kemr to create rings |
| Edge shared by exactly 2 faces | Normal manifold edge — mef creates it |
| Edge shared by 1 face only | Open boundary edge — should not occur for closed boolean results |
| Flipped sub-triangles (Subtract) | Edge directions already accounted for in TrimBoundaryMap winding |

---

## 4. Invariants

1. **Euler characteristic**: V - E + F = 2 for a closed manifold solid.

2. **Manifold**: Every edge in the result is shared by exactly 2 faces
   (each half-edge has a twin in a different face's loop).

3. **Face count conservation**: The number of faces in the ResultTopology equals
   the number of entries in the TrimBoundaryMap.

4. **Vertex conservation**: Every vertex referenced by any TrimEdge appears in
   the ResultTopology's vertex set.

5. **Edge conservation**: Every undirected edge in the TrimBoundaryMap corresponds
   to exactly one Edge in the ResultTopology.

6. **Provenance preservation**: Each result face maps to exactly one SourceFace.

7. **Intersection classification**: Each edge's `is_intersection` flag matches
   the TrimEdge classification from task 3b.

---

## 5. Oracles

| Test | Oracle |
|------|--------|
| Box-box subtract vertex count | V = number of unique vertices in all trim loops |
| Box-box subtract face count | F = number of entries in TrimBoundaryMap |
| Box-box subtract Euler | V - E + F = 2 |
| Edge sharing | Every EdgeIdx has exactly 2 half-edges in different face loops |
| Provenance | Every result face maps to a valid SourceFace |
| Empty input | Empty TrimBoundaryMap → empty ResultTopology |

---

## 6. Failure Modes

| Failure | Handling |
|---------|----------|
| Empty TrimBoundaryMap | Return empty ResultTopology (no faces, no edges) |
| Non-manifold edge (shared by ≠ 2 faces) | Return error diagnostic — indicates mesh boolean bug |
| Open boundary (edge shared by 1 face) | Return error diagnostic — indicates incomplete boolean |
| Vertex index out of range | Panic (indicates corrupt Phase 2 data) |

---

## 7. Research Basis

- **[#24] Yang, Jia & Yan (2025)**: Stage 3 — reconstruct B-Rep topology from
  mesh boolean result. The trim boundaries define face boundaries; edges shared
  between faces define the result's edge graph.
- **[#16] Mantyla (1988)**: Euler operators for B-Rep construction. The spanning
  tree approach (mvfs → mev → mef) is the standard method for building a manifold
  from its face-edge-vertex decomposition.
- **[#33] Stroud (2006)**: Half-edge data structure and topological validation.

### 7a. Analytical vs. Approximate Method Justification

**Method**: Exact (topological construction from integer vertex indices).

This task performs no geometric computation beyond looking up vertex positions
from SubdividedMesh. The topology is constructed purely from the combinatorial
structure of the trim boundaries. All Euler operator arguments are vertex/edge/loop
indices, not floating-point coordinates (positions are passed through unchanged).

---

## 8. Data Structure Design

```rust
/// Result of connectivity extraction — the B-Rep topology of the boolean result.
#[derive(Debug)]
pub(crate) struct ResultTopology {
    /// Half-edge topology of the result solid.
    pub arena: TopoArena,
    /// Maps each result face to its source (MeshId, FaceIdx).
    pub face_provenance: BTreeMap<FaceIdx, SourceFace>,
    /// Maps each result edge to whether it's an intersection edge.
    pub edge_is_intersection: BTreeMap<EdgeIdx, bool>,
}
```

---

## 9. Algorithm

### Phase A: Extract combinatorial structure

1. **Collect unique vertices**: Walk all TrimEdges across all faces. Build a
   dedup map: `mesh_vertex_index → result_vertex_id` (0-based sequential).
   Record each vertex's position from `SubdividedMesh.verts`.

2. **Build undirected edge map**: For each face's TrimEdges, record undirected
   edges `(min(v0,v1), max(v0,v1))` with their face associations and
   `is_intersection` flags.

3. **Build adjacency list**: vertex → list of adjacent vertices (from edges).

### Phase B: Euler operator construction

4. **Build spanning tree** of the vertex adjacency graph via BFS from vertex 0.
   The spanning tree has V-1 edges.

5. **mvfs** with vertex 0's position → creates solid, shell, face0, loop0,
   vertex v0.

6. **mev** for each spanning tree edge in BFS order: `mev(parent, loop0, child_position)`
   → creates child vertex and edge. All vertices now exist in loop0.

7. **Identify non-tree edges**: These are the E - (V-1) = F-1 edges not in the
   spanning tree. Each will become an mef call.

8. **Order mef calls**: For each non-tree edge (u, w), find the loop containing
   both u and w, then call `mef(u, w, that_loop)`. Each mef creates a new face
   and splits the loop. The order matters — process edges so that each face's
   boundary is completed before moving to the next.

### Phase C: Provenance and classification

9. **Map faces to SourceFace**: After all mef calls, each created face corresponds
   to one SourceFace from the TrimBoundaryMap. Use the edge-to-face associations
   from step 2 to determine which SourceFace each result face belongs to.

10. **Classify edges**: For each result Edge, look up whether the corresponding
    undirected edge in the TrimBoundaryMap had `is_intersection = true`.

### Phase D: Inner loops (if any)

11. For faces with multiple TrimLoops, the first loop is the outer boundary
    (handled by mef). Additional loops represent holes — use kemr to create
    inner rings on those faces.
