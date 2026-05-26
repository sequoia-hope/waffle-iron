# `cherchi-rs::arrangements::FastTrimesh` removal — Spike PR-CR12b

## Goal

Port the **removal half** of FastTrimesh's mutator API: `remove_tri`,
`remove_tris`, `remove_edge`, plus the private swap-pop dance helpers
(`tri_switch`, `edge_switch`, `remove_tri_unref`, `remove_edge_unref`,
`edge_contains_vert`, and a new `tri_edges_opt`). Second of the
three-PR split of the originally-banked PR-CR12.

This is the **algorithmically fragile** part of the FastTrimesh port:
deleting a triangle requires removing it from its edges' `e2t` lists,
auto-removing any newly-dangling edges, then swap-pop deletion with
index remapping (so all stale references to the popped slot get
rewritten). The same pattern applies to edge deletion via `v2e`.

After PR-CR12b, only PR-CR12c (re-triangulation + Tree + Plane-using
orientation) remains before FastTrimesh is feature-complete.

## Public API

### Removal

```rust
pub fn remove_tri(&mut self, t: u32);
pub fn remove_tris(&mut self, ts: Vec<u32>);
pub fn remove_edge(&mut self, e: u32);
```

All return `void`. `debug_assert!` bounds on input IDs. `remove_tris`
takes `Vec<u32>` by value because the borrow checker forces a clone
anyway (the only realistic call site is `remove_edge`, which passes
`self.e2t[e].clone()` — see borrow analysis below). This also matches
upstream's "copy-then-sort-then-iterate" semantics.

### Private helpers

```rust
fn tri_edges_opt(&self, t: u32) -> [Option<u32>; 3];
fn remove_tri_unref(&mut self, t: u32);
fn remove_edge_unref(&mut self, e: u32);
fn tri_switch(&mut self, t0: u32, t1: u32);
fn edge_switch(&mut self, e0: u32, e1: u32);
fn edge_contains_vert(&self, e: u32, v: u32) -> bool;
```

**Dropped from upstream API:** `remove_from_vec` — `Vec::retain(|&x| x != elem)` is the one-line Rust analog; no need for a named helper.

## Algorithm

### `remove_tri(t)` — orchestrator (upstream cpp:658-688)

```text
1. Get tri's 3 edges via tri_edges(t).
   (Public — edges MUST exist at this entry; mesh is well-formed.)
2. For each of the 3 edges: self.e2t[e].retain(|&x| x != t).
3. Identify dangling edges: those with e2t[e].is_empty().
4. Sort dangling edges DESCENDING by id.
5. For each dangling edge e in that order:
   a. Get (v0, v1) endpoints.
   b. self.v2e[v0].retain(|&x| x != e);
      self.v2e[v1].retain(|&x| x != e);
   c. self.remove_edge_unref(e);   // swap-pops the edge
6. self.remove_tri_unref(t);       // swap-pops the triangle
```

### `remove_tris(ts)` — batch (upstream cpp:692-704)

```text
1. ts.sort_unstable_by(|a, b| b.cmp(a));   // descending
2. for t in ts: self.remove_tri(t);
```

### `remove_edge(e)` — cascade (upstream cpp:650-654)

```text
let ts = self.e2t[e].clone();    // mandatory: we mutate self in the loop
self.remove_tris(ts);
```

The cloned `Vec<u32>` is consumed by `remove_tris`. The borrow checker rejects passing `&self.e2t[e]` because the loop body calls `&mut self`. **The cascade also auto-removes edge `e` itself** as a dangling edge during one of the `remove_tri` calls — `remove_edge` doesn't need to remove it directly.

### `remove_tri_unref(t)` — swap-pop the tri (upstream cpp:907-911)

```text
let last = self.num_tris() - 1;
self.tri_switch(t, last);
self.triangles.pop();
```

### `remove_edge_unref(e)` — swap-pop the edge (upstream cpp:897-903)

```text
self.e2t[e].clear();   // sanity (must already be empty)
let last = self.num_edges() - 1;
self.edge_switch(e, last);
self.edges.pop();
self.e2t.pop();   // parallel-vector lockstep with edges
```

### `tri_switch(t0, t1)` — index remap on triangle swap (upstream cpp:847-867)

```text
if t0 == t1 { return; }
self.triangles.swap(t0, t1);

// Collect edges that may have stale references. Use tri_edges_opt
// because partial-dismantle may leave some as None (the calling
// remove_tri has already popped some of t0's edges in step 5c).
let mut edges_to_fix: Vec<u32> = ...
    chain of tri_edges_opt(t0) + tri_edges_opt(t1), filter Some,
    sort_unstable + dedup.

for e in edges_to_fix:
    for slot in &mut self.e2t[e]:
        if *slot == t0 { *slot = t1; }
        else if *slot == t1 { *slot = t0; }
```

### `edge_switch(e0, e1)` — index remap on edge swap (upstream cpp:871-893)

```text
if e0 == e1 { return; }
self.edges.swap(e0, e1);
self.e2t.swap(e0, e1);   // parallel-vector lockstep

// After the swap, edges[e0] holds what was edges[e1] and vice
// versa. Collect 4 vertex IDs from current state.
let mut verts_to_fix: Vec<u32> = ...
    [edges[e0].v0, edges[e0].v1, edges[e1].v0, edges[e1].v1]
    sort_unstable + dedup.

for v in verts_to_fix:
    for slot in &mut self.v2e[v]:
        if *slot == e0 { *slot = e1; }
        else if *slot == e1 { *slot = e0; }
```

### `tri_edges_opt(t)` — partial-dismantle-tolerant lookup

```text
let v = self.tri(t);
[self.edge_id(v[0], v[1]), self.edge_id(v[1], v[2]), self.edge_id(v[2], v[0])]
```

Returns `[Option<u32>; 3]`. `None` for edges already popped by an in-flight `remove_tri`.

## Invariants

Post-removal:

1. All PR-CR11 invariants hold: edge endpoint ordering (`v0 < v1`), `v2e[v]` ↔ `e2t[e]` symmetry, no degenerate triangles, sum |v2e| = 2·E, sum |e2t| = 3·T.
2. Edges with `e2t.is_empty()` are automatically removed during `remove_tri`. After `remove_tri` returns, every remaining edge has `adj_e2t.len() >= 1`.
3. Vertices are **never** removed by this API. Isolated vertices (valence 0) persist. Compaction is a future concern.
4. After `remove_tri(t)` returns, no stale reference to the popped triangle index exists in any `e2t[*]` list (this is what `tri_switch` ensures).
5. After `remove_edge_unref(e)` returns, no stale reference to the popped edge index exists in any `v2e[*]` list (this is what `edge_switch` ensures).
6. `remove_tris(ts)` produces the same final canonical shape regardless of input order (when the input `ts` is a valid set of triangles). Achieved via the descending-sort discipline.
7. `remove_edge(e)` on an edge with `e2t.len() == k` removes exactly `k` triangles and the edge itself.

## Error Contract

- All mutators take `debug_assert!` for input bounds (consistent with PR-CR11/CR12a).
- `void` returns everywhere (matches upstream + CR12a's `add_tri` precedent).
- Out-of-range IDs are programmer bugs, not data errors — they trip in debug, compile out in release per Hard Rule #6.
- `remove_tri` / `remove_edge` on an out-of-range index → `debug_assert!` panic in debug.
- `remove_tris(vec![])` is a valid no-op.
- The "empty mesh + remove anything" case is a precondition violation; not tested (vacuous).

## Deliberate Deviations from Upstream

Carry-forward from PR-CR11/CR12a (still in effect):
1. Explicit points only (no LGPL `genericPoint*`)
2. No parallel constructor
3. `Point3` stored by value
4. `Option<u32>` returns from `edge_id` / `tri_id` / `tri_vert_offset` (no `int = -1` sentinel)
5. `Vec<Vec<u32>>` adjacency (no `absl::InlinedVector`)
6. Separate `Vertex.orig_id: Option<u32>` field
7. `std::collections::HashMap` for `rev_vtx_map` (no phmap)
8. Separate method names (no overloading)
9. `set_edge_constr` matches upstream "set-to-true only"
10. `edge_set_visited` writes to separate `visited` field (CR11 split)

New for PR-CR12b:

**11. `tri_edges_opt(t) -> [Option<u32>; 3]` private helper.**
Upstream's `triEdgeID(t, off)` returns `int` with `-1` for "edge no longer exists" (cpp:858-862, checked inside `triSwitch`). The public `tri_edges` in PR-CR11 returns `[u32; 3]` with `.expect()` semantics — it cannot represent the partial-dismantle window correctly. PR-CR12b adds a private `tri_edges_opt` that returns `[Option<u32>; 3]`, used exclusively by `tri_switch` to handle the window where some edges of `t0` have been popped but the triangle slot still references their vertex pairs. **This is the single most subtle aspect of the entire FastTrimesh port** and is the architectural choice that makes cascading swap-pop correct.

**12. `Vec<u32>` (owned) for `remove_tris`.** Upstream has two overloads (`std::vector<uint>` and `fmvector<uint>`); both immediately copy-then-sort. Rust's borrow checker forces a clone at the `remove_edge` call site anyway (we mutate `self.e2t` inside the loop). Taking `Vec<u32>` by value matches the upstream copy semantics literally and reads cleanly.

**13. `Vec::retain` inline (no `remove_from_vec` helper).** Upstream's `removeFromVec` (cpp:840-843) is C++'s verbose `erase-remove` idiom. Rust's `Vec::retain(|&x| x != elem)` is a one-liner; no need for a named helper.

## Test Plan

7 groups, ~25 tests, in `#[cfg(test)] mod tests` at the bottom of `fast_trimesh.rs`. New test helper:

```rust
fn canonical_shape(ft: &FastTrimesh) -> (u32, u32, u32, Vec<[u32;2]>, Vec<[u32;3]>, Vec<u32>) {
    // (num_verts, num_edges, num_tris, sorted_edge_endpoints,
    //  sorted_tri_vertex_sets, sorted_valences)
    // — captures shape independent of HashMap iteration / swap-pop ordering
}
```

### Group 1 — Single-triangle removal cascades

- Remove only tri from a 1-tri mesh → 3 isolated verts, 0 edges, 0 tris
- After removal, all 3 edges are gone (auto-cascade)

### Group 2 — Manifold (tetrahedron) removal

- Remove 1 tri from tetra → 3 tris, 6 edges (3 now boundary), 4 verts
- Remove all 4 tris one-by-one → 4 isolated verts, 0 edges, 0 tris
- Sum invariants hold after each step

### Group 3 — Non-manifold + boundary

- 3 tris sharing edge (0,1): remove 1 → edge still has 2 tris, not removed
- Two-tri quad: remove tri 0 → diagonal becomes boundary; 4 verts, 3 edges, 1 tri
- Two-tri quad: remove both tris → 4 verts, 0 edges, 0 tris

### Group 4 — Order independence (canonical_shape equality)

- Tetra: remove [0,2] vs [2,0] → same canonical shape
- Icosahedron: remove [5,10,15] in 2-3 different orders → same canonical shape

### Group 5 — Batch consistency

- `remove_tris([0,1])` ≡ `remove_tri(1); remove_tri(0)` in canonical shape
- `remove_tris(vec![])` is a no-op (shape unchanged)

### Group 6 — Edge removal

- Tetra: remove interior edge → 2 tris gone, 4 verts, 5 edges, 2 tris
- Two-tri quad: remove diagonal → 2 tris gone, 4 verts, 4 edges, 0 tris

### Group 7 — Index remapping correctness (load-bearing)

- **Swap-into-zero**: tetra. Remove tri 0. Verify OLD tri 3 (last) is now at slot 0. For every edge incident to OLD tri 3, `adj_e2t` contains 0, NOT 3.
- **Cascading swap-pop** (single most valuable test): tetra. Remove tri 0, then remove tri 0 again. Compare canonical_shape with a known-good ground truth (2-tri mesh built fresh from `from_soup`).
- **Sum invariants post-multi-remove**: icosahedron. Remove 3 tris in sequence. After each, `sum |v2e| == 2·num_edges` and `sum |e2t| == 3·num_tris`.

## Sidecar exercise

**None.** FastTrimesh remains internal scratch storage. Sidecar exercise resumes at PR-CR13+.

## Banked for PR-CR12c (re-triangulation + Tree + Plane queries)

Unchanged from PR-CR12a's banking:
- `split_edge` (with/without Tree), `split_tri` (with/without Tree)
- `flip_tri`
- New `Tree` struct in `arrangements/tree.rs`
- `tri_node_id` / `set_tri_node_id`
- `tri_orientation` (uses CR10 `orient2d` after `Plane` axis-drop)
- `tri_verts_are_ccw`
- Parallel constructor (rayon)

## References

- Cherchi et al. 2020 — "Fast and Robust Mesh Arrangements using Floating-point Arithmetic" §4
- Upstream C++ (MIT):
  - `fast_trimesh.cpp:658-688` — `removeTri`
  - `fast_trimesh.cpp:692-704` — `removeTris` (two overloads)
  - `fast_trimesh.cpp:650-654` — `removeEdge`
  - `fast_trimesh.cpp:907-911` — `removeTriUnref`
  - `fast_trimesh.cpp:897-903` — `removeEdgeUnref`
  - `fast_trimesh.cpp:847-867` — `triSwitch`
  - `fast_trimesh.cpp:871-893` — `edgeSwitch`
  - `fast_trimesh.cpp:831-836` — `edgeContainsVert`
  - `fast_trimesh.cpp:840-843` — `removeFromVec` (dropped from port)
  - `fast_trimesh.cpp:858-862` — the `-1 continue` partial-dismantle branch motivating `tri_edges_opt`
- PR-CR11 spec: `specs/cherchi_rs_fast_trimesh_mvp.md`
- PR-CR12a spec: `specs/cherchi_rs_fast_trimesh_mutators.md`
