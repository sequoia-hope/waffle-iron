# `yang-rs::boolean` output BRep topology reconstruction — PR-YR5

## Goal

Populate the output `BRep`'s `vertices` / `edges` / `faces` from
PR-YR4's `TriangleAttributionMap`:

1. **Flood-fill** same-attribution triangles into patches via
   connectivity (triangle adjacency through shared edges).
2. **Recover boundary cycle** per patch — directed edges traversed in
   patch-triangle CCW order, walked into a closed sequence.
3. **Inherit surface** from the input face that the patch is attributed
   to.
4. **Construct `BRepFace`** per patch (one face per connected
   component); each face owns its own `BRepEdge`s (no dedup in v1).
5. **Vertices** are 1:1 with `mesh.verts` (preserves the bijection
   from PR-YR3 / PR-YR4).

After PR-YR5, `boolean()` returns a `BRep` with non-empty topology
covering the "kept" portions of input faces — surviving regions of
faces that the boolean operation didn't entirely remove. Cut-surface
faces (where None-attributed triangles live) are intentionally
omitted; the output is **deliberately NOT 2-manifold** in v1.

## Architectural reality (same constraint as PR-YR3 / PR-YR4)

Real Yang Stage 6 needs per-triangle labels from Stage 2's
arrangement; the C++ `mesh_booleans` binary doesn't expose them.
PR-YR5 substitutes patch grouping over PR-YR4 majority-vote
attribution + surface inheritance + boundary cycle recovery. The
result is honest about being a partial deliverable.

## Public API (no changes)

`BRep::vertices()`, `edges()`, `faces()` accessors already exist; they
return empty slices when `BRep` came from `BRep::new` / `BRep::from_mesh`
(unchanged from PR-YR4). Only `boolean()`'s output is affected:
previously empty, now populated.

`boolean()` signature unchanged:
```rust
pub fn boolean(
    a: &BRep,
    b: &BRep,
    op: BoolOp,
    backend: &dyn MeshBoolean,
) -> Result<BRep, YangError>;
```

The `NonManifoldOutput` and `MalformedTopology` error variants
(already declared) become potential errors from `boolean()` in
PR-YR5 (previously not emitted).

## Algorithm

```text
reconstruct_topology(mesh, attribution, a, b) -> Result<(Vec<BRepVertex>, Vec<BRepEdge>, Vec<BRepFace>), YangError>:

    // (1) Vertices: 1:1 with mesh.verts
    vertices = mesh.verts.iter().map(|p| BRepVertex { point: p }).collect()

    // (2) Triangle adjacency (BTreeMap-keyed for determinism)
    adjacency = triangle_adjacency(mesh)
        // BTreeMap<(u32 min, u32 max), Vec<u32 tri>> grouping tris sharing an edge;
        // expanded to Vec<Vec<u32>> per-tri neighbors (ascending order from BTree).

    // (3) Flood-fill same-attribution patches
    patches = flood_fill_patches(mesh, attribution, adjacency)
        // BFS from lowest-index Some-attributed unvisited tri;
        // expand to neighbors that share the same attribution.
        // None-attributed tris are skipped (cut surfaces — PR-YR6).

    // (4) Per-patch boundary cycle + face construction
    edges = Vec::new()
    faces = Vec::new()
    for patch in patches:
        cycle = patch_boundary_cycle(patch, mesh)?
            // Vec<(u32 start, u32 end)> in walked order.
            // Returns Err(NonManifoldOutput) on T-junction / unclosed / multi-cycle.

        edge_start_idx = edges.len()
        for (s, e) in cycle:
            edges.push(BRepEdge { start: s, end: e, curve: Curve::LineSegment })
        outer_loop = (edge_start_idx..edges.len() as u32).collect()

        input_brep = match patch.attribution.input { A => a, B => b }
        if (patch.attribution.face as usize) >= input_brep.faces().len():
            return Err(MalformedTopology("attribution.face out of range"))
        surface = input_brep.faces()[patch.attribution.face as usize].surface.clone()
        faces.push(BRepFace { surface, outer_loop })

    Ok((vertices, edges, faces))


triangle_adjacency(mesh):
    edge_to_tris: BTreeMap<(u32, u32), Vec<u32>> = {}
    for (t, tri) in mesh.tris.iter().enumerate():
        for (i, j) in [(0,1), (1,2), (2,0)]:
            let key = canonical_edge(tri[i], tri[j])  // (min, max)
            edge_to_tris.entry(key).or_default().push(t as u32)
    neighbors: Vec<Vec<u32>> = vec![Vec::new(); mesh.tris.len()]
    for (_, sharing) in &edge_to_tris:
        for &t1 in sharing:
            for &t2 in sharing:
                if t1 != t2 && !neighbors[t1].contains(&t2):
                    neighbors[t1].push(t2)
    neighbors


flood_fill_patches(mesh, attribution, adjacency):
    visited: Vec<bool> = vec![false; mesh.tris.len()]
    patches = []
    for seed in 0..mesh.tris.len() as u32:
        if visited[seed]: continue
        let Some(seed_attr) = attribution.lookup(seed) else: {
            visited[seed] = true; continue
        }
        queue = VecDeque::from([seed])
        tri_indices = Vec::new()
        while let Some(t) = queue.pop_front():
            if visited[t]: continue
            let Some(t_attr) = attribution.lookup(t) else: continue
            if t_attr != seed_attr: continue
            visited[t] = true
            tri_indices.push(t)
            for &n in &adjacency[t]:
                if !visited[n]: queue.push_back(n)
        patches.push(Patch { attribution: seed_attr, tri_indices })
    patches


patch_boundary_cycle(patch, mesh):
    patch_set: HashSet<u32> = patch.tri_indices.iter().collect()

    // Collect directed boundary edges from each patch tri's 3 CCW edges
    directed_boundary: Vec<(u32, u32)> = []
    for &t in &patch.tri_indices:
        tri = mesh.tris[t]
        for (i, j) in [(0,1), (1,2), (2,0)]:
            let edge_dir = (tri[i], tri[j])
            let edge_key = canonical_edge(edge_dir.0, edge_dir.1)
            // Boundary iff no OTHER patch tri shares this edge
            let other_in_patch = mesh.tris.iter().enumerate().any(|(t2, tri2)|
                t2 as u32 != t && patch_set.contains(&(t2 as u32)) &&
                tri_has_edge(tri2, edge_key))
            if !other_in_patch:
                directed_boundary.push(edge_dir)

    if directed_boundary.is_empty(): return Ok(vec![])  // defensive

    // Walk into cycle
    by_start: BTreeMap<u32, Vec<u32>> = collect (start, end) pairs grouping by start;
                                        each Vec<u32> sorted ascending.
    let start = *by_start.keys().next().unwrap()
    let mut current = start
    let mut cycle = Vec::new()
    loop:
        let next_vec = by_start.get_mut(&current)
            .ok_or(NonManifoldOutput(f"dead-end at vertex {current}"))?
        if next_vec.is_empty():
            return Err(NonManifoldOutput(f"dead-end at vertex {current}"))
        let next = next_vec.remove(0)  // lowest end first → deterministic
        cycle.push((current, next))
        current = next
        if current == start: break
        if cycle.len() > directed_boundary.len():
            return Err(NonManifoldOutput("T-junction or multi-cycle exceeded edge count"))

    if cycle.len() != directed_boundary.len():
        return Err(NonManifoldOutput(f"patch has multiple boundary cycles ({...}); inner loops unsupported"))

    Ok(cycle)
```

Complexity:
- `triangle_adjacency`: O(T) via BTreeMap insertion.
- `flood_fill_patches`: O(T).
- `patch_boundary_cycle`: O(T·P) where P = patch size (the
  `other_in_patch` scan walks all triangles). For v1, acceptable.
  Optimization (precomputing edge-occurrence count) banked for PR-YR5b.

## Invariants

1. `output.vertices().len() == output_mesh.num_verts()`
2. `output.vertices()[i].point == output_mesh.verts[i]`
3. For each `BRepFace f` in `output.faces()`, `f.outer_loop` lists
   sequential `BRepEdge` indices forming a closed cycle:
   `edges[outer_loop[i]].end == edges[outer_loop[(i+1) % len]].start`.
4. Each patch corresponds to ONE `BRepFace`. Disconnected
   attribution components → multiple faces.
5. `output.faces().len() ==` number of distinct connected components
   of Some-attributed triangles (one face per component).
6. `output.faces()[i].surface == input.faces()[attribution.face].surface.clone()`
   where `input` is `a` or `b` per `attribution.input`.
7. None-attributed triangles contribute NO BRepEdges and NO BRepFaces
   (cut surfaces banked PR-YR6).
8. PR-YR3 / PR-YR4 invariants preserved.
9. **Output is intentionally NOT 2-manifold in v1.** Documented
   rule-4 deviation; PR-YR6 will close.

## Edge cases

- **All-None attribution** (both inputs `from_mesh`): no patches → no
  faces / no edges. Vertices still populated 1:1.
- **Disconnected attribution components**: flood-fill produces
  separate patches → separate faces with the same inherited surface.
- **Inner-loop patches** (hole-bearing): currently → `Err(NonManifoldOutput)`.
  Banked PR-YR5c.
- **T-junction in boundary cycle** (vertex with >2 incident boundary
  edges from same patch): → `Err(NonManifoldOutput)`. Indicates
  non-manifold patch boundary, unsupported.
- **`face_idx` out of range** in input's `faces()`: → `Err(MalformedTopology)`.
  Defense in depth — naturally-occurring attributions shouldn't trigger.

## Error contract

- `YangError::NonManifoldOutput`: now ACTUALLY emitted from `boolean()`.
  Diagnostic message format:
    - "patch {N} boundary edges do not form a closed cycle (dead-end at vertex {V})"
    - "patch {N} has T-junction or multi-cycle (cycle length exceeded boundary edge count)"
    - "patch {N} has multiple boundary cycles ({K}); inner loops not supported (PR-YR5c)"
- `YangError::MalformedTopology(String)`: emitted when attribution
  references an out-of-range input face. Defense-in-depth only.

## Limitations (banked)

1. **No cut surfaces** — None-attributed triangles skipped. Output
   has open boundaries. PR-YR6 closes this and re-enables 2-manifold
   contract.
2. **No edge deduplication** — each face owns its own `BRepEdge`s.
   For 2 adjacent faces sharing an edge, two `BRepEdge`s exist with
   the same (or reversed) start/end. PR-YR5b.
3. **No inner-loop / hole support** — multi-cycle patches → error.
   PR-YR5c.
4. **Curve recovery: `LineSegment` only** — intersection curves are
   not analytically recovered. PR-YR7+.
5. **No surface refitting** — surfaces are .clone()'d from input
   attribution; not re-fit from patch geometry.

## Test plan (~9 tests, 2 groups)

### Group A — lib unit tests via mock backend (7 tests)
1. **Single-triangle round-trip** — `triangle_brep` mock returns A → 1 face, 3 edges
2. **Two-face round-trip** — `two_face_shared_vertex_brep` → 2 faces
3. **Disconnected components → separate faces** (regression vs naive bucketing)
4. **None-attributed tris omitted** from faces
5. **Vertex count matches mesh** + per-vertex Point3 equality
6. **Surface inheritance** equals input
7. **Empty input → empty topology** (vertices populated; edges/faces empty)

### Group B — sidecar regression (2 tests)
8. **Intersect produces faces** — two cubes via real sidecar; faces().len() > 0
9. **Face count matches attribution-component count** (self-consistency)

## Honest framing

PR-YR5 is **not** real Yang Stage 6. It's a sidecar-feasible
substitute that groups same-attribution triangles into patches,
inherits surfaces from inputs, and walks patch boundary cycles to
recover edges. Output BRep topology is partial — cut surfaces
(PR-YR6), inner loops (PR-YR5c), and edge dedup (PR-YR5b) are
banked. The output is intentionally NOT 2-manifold in v1, documented
as a rule-4 deviation that PR-YR6 closes.

## References

- Yang 2025 §4.4.2 — `refs/text/yang2025_hybrid_boolean.txt:574-700`
- PR-YR3 spec — `specs/yang_rs_vertex_provenance.md`
- PR-YR4 spec — `specs/yang_rs_triangle_attribution.md`
