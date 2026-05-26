# `cherchi-rs::arrangements::FastTrimesh` MVP — Spike PR-CR11

## Goal

Port the **build-once / query-many** subset of Cherchi 2020 §4's
`FastTrimesh` — the adjacency-aware triangle-soup data structure that
mesh arrangement (§5) operates on. This is the **first non-predicate
work** in cherchi-rs.

The upstream class (`arrangements/code/fast_trimesh.{h,cpp}`,
1,154 LOC) is too large for a single PR. PR-CR11 ships *only* the
read path: a bulk constructor + every topology / adjacency query the
arrangement read phase needs. **All mutators** (`add_*`, `remove_*`,
`split_*`, `flip_*`), the `Tree`-driven symbolic-split tracking, the
`rev_vtx_map`, and the `Plane`-using 2D-orientation methods are
deferred to PR-CR12.

Pipeline position: `FastTrimesh` sits between the input triangle soup
and the arrangement algorithm. The arrangement walks edges,
identifies affected triangles via `adj_e2t`, and (in PR-CR13+)
mutates the mesh via splits. PR-CR11 covers the walk; PR-CR12 covers
the mutation; PR-CR13 covers the arrangement loop.

## Public API

### Types

```rust
/// Triangle-soup with vertex/edge/triangle storage + V↔E and E↔T
/// adjacency. Build via `from_soup`; query via the methods below.
/// Immutable after construction in PR-CR11 (mutators land in PR-CR12).
pub struct FastTrimesh { /* private fields */ }

/// Reference projection plane for the triangles. Used by PR-CR12+
/// 2D-orientation queries; stored but not consumed in PR-CR11.
#[derive(Copy, Clone, Debug, PartialEq, Eq)]
pub enum Plane { XY, YZ, ZX }

/// Bulk-load error.
#[derive(Debug, PartialEq, Eq)]
pub enum FastTrimeshError {
    /// `tris[tri].slot] = vid` but `vid >= verts.len()`.
    VertexIndexOutOfRange { tri: u32, slot: u8, vid: u32, n_verts: u32 },
    /// `tris[tri]` has two equal vertex indices.
    DegenerateTriangle { tri: u32, vids: [u32; 3] },
    /// Input exceeds `u32::MAX` count.
    TooManyVertices { count: usize },
    TooManyTriangles { count: usize },
}
```

### Constructor

```rust
pub fn from_soup(
    verts: &[Point3],
    tris: &[[u32; 3]],
    plane: Plane,
) -> Result<Self, FastTrimeshError>;
```

Validates inputs (count bounds + per-triangle index range + degeneracy),
copies `verts` into internal storage, derives the sorted-unique edge
list, builds `v2e` (vertex → edges) and `e2t` (edge → triangles)
adjacency, stores `plane`.

### Counts

```rust
pub fn num_verts(&self) -> u32;
pub fn num_edges(&self) -> u32;
pub fn num_tris(&self) -> u32;
pub fn ref_plane(&self) -> Plane;
```

### Vertex queries

```rust
pub fn vert(&self, v: u32) -> Point3;            // by-value (Point3 is Copy)
pub fn vert_info(&self, v: u32) -> u32;          // PR-CR11: always 0
pub fn vert_valence(&self, v: u32) -> u32;       // == adj_v2e(v).len()
pub fn adj_v2e(&self, v: u32) -> &[u32];         // edges incident to vertex
```

### Edge queries

```rust
pub fn edge(&self, e: u32) -> (u32, u32);        // (v0, v1) with v0 < v1
pub fn edge_vert_id(&self, e: u32, off: u32) -> u32; // off ∈ {0, 1}
pub fn edge_id(&self, u: u32, v: u32) -> Option<u32>; // unordered lookup
pub fn edge_is_constr(&self, e: u32) -> bool;    // PR-CR11: always false
pub fn edge_is_boundary(&self, e: u32) -> bool;  // adj_e2t(e).len() == 1
pub fn edge_is_manifold(&self, e: u32) -> bool;  // adj_e2t(e).len() <= 2
pub fn adj_e2t(&self, e: u32) -> &[u32];         // triangles incident to edge
```

### Triangle queries

```rust
pub fn tri(&self, t: u32) -> [u32; 3];                  // vertex IDs in storage order
pub fn tri_vert_id(&self, t: u32, off: u32) -> u32;     // off ∈ {0, 1, 2}
pub fn tri_vert(&self, t: u32, off: u32) -> Point3;
pub fn tri_vert_offset(&self, t: u32, v: u32) -> Option<u32>;  // returns off ∈ {0,1,2}
pub fn tri_contains_vert(&self, t: u32, v: u32) -> bool;
pub fn tri_edges(&self, t: u32) -> [u32; 3];            // 3 edge IDs in CCW order
pub fn tri_info(&self, t: u32) -> u32;                  // PR-CR11: always 0
```

## Invariants

1. **`vertices.len() == v2e.len()`** — each vertex has an adjacency slot, possibly empty.
2. **`edges.len() == e2t.len()`** — each edge has an adjacency slot.
3. **Edge endpoint ordering**: `edge.0 < edge.1` for every edge.
4. **Edge dedup**: distinct edges have distinct (sorted) endpoint pairs.
5. **`v2e` consistency**: for each edge `e = (v0, v1)`, `e ∈ v2e[v0] ∧ e ∈ v2e[v1]` exactly once.
6. **`e2t` consistency**: for each triangle `t` and each of its 3 derived edges `e`, `t ∈ e2t[e]` exactly once.
7. **No degenerate triangles**: every triangle has 3 distinct vertex indices.
8. **Identity invariant**: ∑|v2e[v]| = 2·num_edges and ∑|e2t[e]| = 3·num_tris.
9. **Determinism**: `from_soup` is single-threaded; same inputs → same outputs across runs and platforms.

Duplicate triangles (same 3-vertex set, possibly in different order) are NOT rejected by PR-CR11 — upstream's bulk constructor permits them, and detection would require additional storage. They will produce duplicate entries in `e2t`. Documented; banked.

Non-manifold edges (more than 2 incident triangles) are permitted. `edge_is_manifold` distinguishes them; `e2t[e]` may have length > 2.

## Error Contract

- **`from_soup` returns `Result`**: bulk-load failures are caller-supplied data errors (per cherchi-rs `CLAUDE.md` Hard Rule #6: no `panic!` in production code paths).
- **Query methods use `debug_assert!` for bounds**: out-of-range queries are programmer bugs, not data. `debug_assert!` compiles out in release; `cargo test` (debug) surfaces violations. Matches upstream `assert()` philosophy.
- **`edge_id(u, v)` returns `Option<u32>`**: unordered endpoint pair → edge ID; `None` if no such edge. (Upstream returns `int = -1` for missing — we use `Option` for type safety.)
- **`tri_vert_offset(t, v)` returns `Option<u32>`**: vertex offset within a triangle, `None` if `v` is not in the triangle.

## Deliberate Deviations from Upstream

1. **Explicit points only.** Upstream stores `const genericPoint*` to support implicit (LPI/TPI) points from the LGPL `Indirect_Predicates` library. cherchi-rs does NOT depend on LGPL code (paused; see project memory). PR-CR11 stores `Point3` by value. When the LGPL decision resolves, `Vertex` will gain an implicit-point variant; topology layer is unaffected.

2. **No parallel constructor.** Hard Rule #5 — single-threaded by default. We use the same sorted-unique algorithm the upstream parallel path uses, just without TBB.

3. **No `rev_vtx_map`, `Tree` integration, or `Plane`-using 2D-orientation queries.** Deferred to PR-CR12.

4. **No `addVert`/`addTri`/`removeTri`/`splitEdge`/etc.** Deferred to PR-CR12. PR-CR11 is `from_soup`-only.

5. **`Point3` stored by value.** Upstream uses `const genericPoint*` for polymorphism; `Point3` is `Copy` (24 B) and has no polymorphism, so by-value is cleaner.

6. **`info` fields included but read-only**, default 0. Setters land in PR-CR12. Saves a struct-layout change.

## Test Plan

5 groups, ~35 tests total, organized in `#[cfg(test)] mod tests` at the bottom of `fast_trimesh.rs`.

### Group 1 — Construction & basic counts

- Empty input → `Ok` with 0/0/0 counts.
- Single triangle → 3 verts, 3 edges, 1 tri.
- Two-tri quad (shared diagonal) → 4 verts, 5 edges, 2 tris.
- Tetrahedron (closed) → 4 verts, 6 edges, 4 tris.
- Icosahedron (closed manifold, V=12, E=30, F=20).
- Large random soup (deterministic seed-free fixture).

### Group 2 — Vertex / triangle accessors

- `vert(id)` returns the input `Point3` (by-value identity).
- `tri(id)` returns the input vertex triple in storage order.
- `tri_vert_id(t, off)` indirection round-trip.
- `tri_vert(t, off)` returns expected `Point3`.
- `tri_contains_vert(t, v)` true/false matrix on tetrahedron.
- `tri_vert_offset(t, v)` returns `Some(off)` or `None`.

### Group 3 — Edge derivation correctness

- Single tri: 3 edges, all with `v0 < v1`.
- Tetrahedron: 6 unique edges.
- Two-tri quad shares exactly 1 edge.
- Icosahedron has exactly 30 edges.
- `edge_id(u, v) == edge_id(v, u)` (argument-order independence).
- `edge_id(u, v)` returns `None` for non-adjacent vertices.

### Group 4 — Adjacency correctness

- Tetrahedron: every vertex valence is 3.
- Tetrahedron: every edge has `adj_e2t.len() == 2`.
- Two-tri quad shared diagonal: `adj_e2t.len() == 2`; boundary edges: 1.
- Icosahedron: every edge `adj_e2t.len() == 2` (closed manifold).
- Non-manifold mesh (3 tris sharing an edge): `edge_is_manifold(e) == false`.
- Property: ∑ valences == 2·num_edges.
- Property: ∑ |e2t| == 3·num_tris.
- `tri_edges(t)` returns 3 edge IDs that all reference `t` in their `adj_e2t`.

### Group 5 — Error / edge cases

- Out-of-range vertex index in triangle → `Err(VertexIndexOutOfRange)`.
- Degenerate triangle `[0, 1, 0]` → `Err(DegenerateTriangle)`.
- Isolated vertex (no triangle uses it) → `Ok`, valence 0.
- Empty triangle list with non-empty verts → `Ok`, zero edges.
- `edge_id` for non-existent edge → `None`.
- `tri_vert_offset` for vertex not in triangle → `None`.

### Sidecar exercise

**None in PR-CR11.** `FastTrimesh` is internal scratch storage; the sidecar runs end-to-end booleans on OBJ files. Sidecar exercise resumes when PR-CR13+ surfaces arrangement-result meshes.

## Banked for PR-CR12 (FastTrimesh Phase 2)

Upstream method → PR-CR12 status:

- `addVert(p, orig_id)`, `addVert(p)` → mutator + `rev_vtx_map`
- `addTri(v0, v1, v2)` → mutator + dedup via `triID` + `addEdge`
- `removeTri(t)`, `removeTris(ts)`, `removeEdge(e)` → swap-pop dance + adjacency repair
- `splitEdge(e, v)`, `splitEdge(e, v, &Tree)` → re-triangulation core
- `splitTri(t, v)`, `splitTri(t, v, &Tree)` → re-triangulation core
- `flipTri(t)` → mutator
- `vertOrigID`, `vertNewID`, `rev_vtx_map` → needs `addVert` path to populate
- `triOrientation`, `triVertsAreCCW` → needs `Plane` + axis-drop projection + `orient2d`
- `triNodeID`, `setTriNodeID` → `Tree` integration
- `setVertInfo`, `setTriInfo`, `resetVerticesInfo`, `resetTrianglesInfo` → info-field setters
- Parallel constructor → rayon opt-in (future feature flag)
- `adj_t2t`, `adj_v2t` → derived via double-hop; non-trivial to do correctly with multi-edge incidences

PR-CR12 unlocks split-driven re-triangulation, which mesh arrangement (PR-CR13+) consumes.

## References

- Cherchi et al. 2020 — "Fast and Robust Mesh Arrangements using Floating-point Arithmetic" §4 (mesh arrangement data structure). `refs/cherchi2020.pdf`.
- Upstream C++ source (MIT):
  - `/home/claude/cherchi2022/InteractiveAndRobustMeshBooleans/arrangements/code/fast_trimesh.h` (241 LOC)
  - `/home/claude/cherchi2022/InteractiveAndRobustMeshBooleans/arrangements/code/fast_trimesh.cpp` (913 LOC)
  - `arrangements/code/common.h:44` — `enum Plane { XY, YZ, ZX };`
- Bulk-constructor algorithm: upstream `fast_trimesh.cpp:78-128` (parallel branch — same sorted-unique algorithm, minus TBB).
- cherchi-rs port conventions established in PR-CR1..CR10 (test layout, attribution headers, RED→GREEN→attribution commits).
