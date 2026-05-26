# `cherchi-rs::arrangements::FastTrimesh` mutators — Spike PR-CR12a

## Goal

Add the **addition mutator family** (vertices, triangles), **info/flag setters**,
**resetters**, and **derived adjacency queries** to PR-CR11's `FastTrimesh`. First
of three PRs splitting the originally-banked PR-CR12 work; isolates the
algorithmically interesting **removal swap-pop** (PR-CR12b) and **re-triangulation
+ Tree + Plane-using orientation** (PR-CR12c) into separate reviews.

Bundled into one PR, the full deferred set would be ~1000 LOC; the removal
swap-pop is fragile enough to deserve isolated review. PR-CR12a is the
dependency-light prefix.

Pipeline position: arrangement (PR-CR13+) inserts segment intersection points
into the mesh via `add_vert` and re-triangulates affected triangles using
splits. PR-CR12a covers the additive half; PR-CR12c covers splits (which call
`add_tri`).

## Public API

### Struct extensions

```rust
#[derive(Copy, Clone, Debug)]
pub(crate) struct Vertex {
    point: Point3,
    info: u32,
    orig_id: Option<u32>,  // NEW
}

pub struct FastTrimesh {
    /* PR-CR11 fields */
    rev_vtx_map: HashMap<u32, u32>,  // NEW — orig_id → new vertex id
}
```

`from_soup` initializes `orig_id: None` for every input vertex and
`rev_vtx_map: HashMap::new()` (no orig-mesh-ID source at bulk-load).

### Vertex addition

```rust
pub fn add_vert(&mut self, p: Point3) -> u32;
pub fn add_vert_with_orig_id(&mut self, p: Point3, orig_id: u32) -> u32;
```

Both return the newly-added vertex's `u32` ID (= previous `num_verts()`).
`add_vert_with_orig_id` additionally inserts `(orig_id, new_v_id)` into
`rev_vtx_map`.

### Vertex orig-ID queries

```rust
pub fn vert_orig_id(&self, v: u32) -> Option<u32>;
pub fn vert_new_id(&self, orig_id: u32) -> Option<u32>;
```

Round-trip identity: `vert_new_id(vert_orig_id(v).unwrap()) == Some(v)` for
vertices added via `add_vert_with_orig_id`.

### Triangle addition + lookup

```rust
pub fn add_tri(&mut self, v0: u32, v1: u32, v2: u32) -> u32;
pub fn tri_id(&self, v0: u32, v1: u32, v2: u32) -> Option<u32>;
```

`add_tri` dedups via `tri_id`: if a triangle already exists with the same
3-vertex set (any order — upstream's `triID` checks all rotations), returns
its existing ID. Otherwise creates 3 edges via the private `add_edge`,
appends the triangle, and updates `e2t`.

### Info / flag setters

```rust
pub fn set_vert_info(&mut self, v: u32, info: u32);
pub fn set_tri_info(&mut self, t: u32, info: u32);
pub fn set_edge_constr(&mut self, e: u32);            // unconditionally true
pub fn edge_set_visited(&mut self, e: u32, vis: bool);
```

`set_edge_constr` takes only the edge ID and sets `constr = true`
(matches upstream cpp:320-324). No clearing API — bank if needed later.

`edge_set_visited` writes to the **separate `visited` field** (PR-CR11
deviation: upstream reuses the `constr` storage for both flags;
cherchi-rs splits them).

### Bulk resetters

```rust
pub fn reset_vertices_info(&mut self);
pub fn reset_triangles_info(&mut self);
```

Loop-zero of the `info` field across all vertices / triangles. Does NOT
touch `orig_id`, `constr`, `visited`, or geometry.

### Derived adjacency queries

```rust
pub fn adj_t2t(&self, t: u32) -> Vec<u32>;
pub fn adj_v2t(&self, v: u32) -> Vec<u32>;
```

Double-hop derivations over PR-CR11's `v2e` + `e2t`:
- `adj_t2t(t)`: for each `e` in `tri_edges(t)`, for each `nbr_t` in `e2t[e]`, if `nbr_t != t`, push.
- `adj_v2t(v)`: for each `e` in `v2e[v]`, for each `t` in `e2t[e]`, push; dedup at end.

Returned `Vec<u32>` is owned (derived data, not stored). Order unspecified;
`adj_v2t` deduplicates (a tri sharing 2 edges with a vertex appears once).

## Invariants

PR-CR11's invariants extended for the post-mutation state:

1. **All CR11 invariants still hold post-add**: edge endpoints sorted, `v2e[v]`/`e2t[e]` symmetry, sum |v2e| = 2·E, sum |e2t| = 3·T, no degenerate triangles.
2. **`add_tri` dedup**: calling `add_tri(v0, v1, v2)` with any rotation of the same 3 vertices returns the same existing tri ID (no duplicate triangles added).
3. **`rev_vtx_map` ⇔ `orig_id` correspondence**: for every entry `rev_vtx_map[orig_id] = new_v_id`, `vertices[new_v_id].orig_id == Some(orig_id)`.
4. **Setter independence**: `set_edge_constr(e)` does not affect `edge_is_visited(e)`; `edge_set_visited(e, _)` does not affect `edge_is_constr(e)` (CR11 deviation: separate fields).
5. **Reset scope**: `reset_vertices_info` / `reset_triangles_info` only zero the `info` field. `orig_id`, edge flags, geometry untouched.

## Error Contract

- **Mutator preconditions checked via `debug_assert!`** — consistent with PR-CR11 query convention. Hard Rule #6 (no `panic!` in production) holds: `debug_assert!` compiles out in release; `cargo test` (debug) surfaces violations.
- **`add_vert*` cannot fail** — appending always succeeds.
- **`add_tri` debug-asserts non-degenerate** (`v0 != v1 && v1 != v2 && v0 != v2`) and in-range. Upstream `assert()`s the same.
- **`tri_id` / `vert_orig_id` / `vert_new_id` return `Option<u32>`** for "not found" — consistent with CR11's `edge_id`.

## Deliberate Deviations from Upstream

Carry-forward from PR-CR11 (still in effect):
- Explicit points only (no `genericPoint*`; LGPL paused)
- No parallel constructor
- `Point3` stored by value
- `info` fields are user-controlled u32
- `edge_id` / `tri_id` return `Option<u32>` not `int = -1`
- `Vec<Vec<u32>>` not `absl::InlinedVector<uint, 16>`

New for PR-CR12a:

1. **Separate `orig_id: Option<u32>` field on `Vertex`.** Upstream's
   `addVert(p, orig_id)` overloads `iVtx.info` to store `orig_id`,
   with 0 as the "no orig_id" sentinel (cpp:603-612). But 0 is a
   valid input vertex ID — foot-gun. We use a separate
   `Option<u32>` field; `info` stays user-controlled.

2. **`HashMap<u32, u32>` for `rev_vtx_map`.** Upstream uses
   `phmap::flat_hash_map`. `std::collections::HashMap` is good
   enough at this scale; swapping to a faster map later is a
   one-field-type change.

3. **Separate method names instead of overloading**:
   `add_vert` (no orig_id) vs `add_vert_with_orig_id`. Rust has no
   method overloading.

4. **`Vec<u32>` by value for `adj_t2t` / `adj_v2t` returns**
   (matches upstream `fmvector<uint>` by value).

5. **`set_edge_constr` matches upstream "set to true only"** — no
   `(e, bool)` form. If clearing is needed, add `clear_edge_constr`
   then. Don't preemptively invent API.

6. **`edge_set_visited` writes to the separate `visited` field**.
   Upstream cpp:371 reuses the `constr` field for both flags
   (cpp:371 `edges[e_id].constr = vis;` — yes, named `constr` but
   semantically `visited`). PR-CR11 already split them; PR-CR12a
   adds the regression tests.

## Test Plan

6 groups, ~30 tests, in the existing `#[cfg(test)] mod tests` at the bottom of `fast_trimesh.rs`.

### Group 1 — Vertex addition + rev_vtx_map

- `add_vert(p)` returns new u32 ID equal to previous `num_verts()`
- `add_vert(p)` then `vert(new_id) == p`
- `add_vert(p)` does NOT populate `rev_vtx_map` → `vert_orig_id(new_id) == None`
- `add_vert_with_orig_id(p, 42)` → `vert_orig_id(new_id) == Some(42)`
- `add_vert_with_orig_id(p, 42)` → `vert_new_id(42) == Some(new_id)`
- `vert_new_id(99)` for unknown orig_id → `None`
- After successive `add_vert` calls, `vert_valence(new_v)` is 0 (no edges yet)

### Group 2 — Triangle addition + dedup

- On empty mesh, after 3 `add_vert` + 1 `add_tri(0,1,2)`, counts are 3/3/1
- `add_tri(0,1,2)` returns new u32 = previous `num_tris()`
- `add_tri(0,1,2)` then `tri(new_id) == [0,1,2]`
- `add_tri(0,1,2)` then `add_tri(0,1,2)` returns same ID (dedup)
- `add_tri(0,1,2)` then `add_tri(1,2,0)` returns same ID (rotation-invariant dedup)
- `add_tri(0,1,2)` then `add_tri(0,1,3)` shares edge (0,1) → `adj_e2t(edge_id(0,1)).len() == 2`
- Sum invariants hold after add: sum |v2e| == 2·E, sum |e2t| == 3·T
- `tri_id(0,1,2)` returns `Some(t_id)` after add; `tri_id(0,1,99)` returns `None`

### Group 3 — Info setters round-trip

- `set_vert_info(0, 42)` → `vert_info(0) == 42`
- `set_tri_info(0, 42)` → `tri_info(0) == 42`
- `set_edge_constr(0)` → `edge_is_constr(0) == true`
- `edge_set_visited(0, true)` → `edge_is_visited(0) == true`
- `edge_set_visited(0, false)` → `edge_is_visited(0) == false`
- **Regression**: `set_edge_constr(0)` does NOT change `edge_is_visited(0)`
- **Regression**: `edge_set_visited(0, true)` does NOT change `edge_is_constr(0)`

### Group 4 — Reset semantics

- `set_vert_info(0, 42); reset_vertices_info()` → `vert_info(0) == 0`
- `set_tri_info(0, 42); reset_triangles_info()` → `tri_info(0) == 0`
- Reset does NOT change `vert_orig_id` (different field)
- Reset does NOT change `edge_is_constr` / `edge_is_visited`

### Group 5 — Derived adjacency

- Tetrahedron: `adj_t2t(t).len() == 3` for every tri
- Tetrahedron: `adj_v2t(v).len() == 3` for every vertex (each shares all 4 - 1 = 3 incident tris)
- Two-tri quad: `adj_t2t(0).len() == 1` (only the other tri across the shared diagonal)
- Two-tri quad: `adj_v2t(corner_v).len() == 1` (only the tri using that corner)
- Isolated vertex: `adj_v2t(iso_v).len() == 0`
- Icosahedron: `adj_t2t(t).len() == 3` for every tri (closed manifold)
- Icosahedron: `adj_v2t(v).len() == 5` for every vertex (icosahedron is regular, valence 5)
- `adj_v2t` deduplicates: each tri appears at most once even if the vertex touches 2 of its edges

### Group 6 — Mutator + query interaction

- After `add_tri`, `tri_edges(new_t)` returns 3 edge IDs with `t` in their `adj_e2t`
- After `add_tri`, `edge_id(v0, v1)` finds the edge
- `add_vert_with_orig_id` round-trip: `vert_new_id(vert_orig_id(v).unwrap()) == Some(v)`
- `from_soup` produces `rev_vtx_map.len() == 0` (no orig-IDs at bulk-load)

## Sidecar exercise

**None.** `FastTrimesh` remains internal scratch storage. The sidecar runs full
booleans on OBJ files. Sidecar exercise resumes when PR-CR13+ surfaces an
arrangement-result mesh that round-trips through OBJ.

## Banked for PR-CR12b (Removal swap-pop)

| Upstream method | LOC | Notes |
|---|---|---|
| `removeTri(t)` | 30 | Unlink + dangling-edge sweep + swap-pop |
| `removeTris(ts)` | 13 | Sort descending + loop `removeTri` |
| `removeEdge(e)` | 5 | Removes all tris on the edge |
| `removeEdgeUnref(e)` | 7 | Private: swap-pop edge already unlinked |
| `removeTriUnref(t)` | 5 | Private: swap-pop tri already unlinked |
| `triSwitch(t0, t1)` | 21 | **Index remapping** — rewrites e2t[] entries |
| `edgeSwitch(e0, e1)` | 23 | **Index remapping** — rewrites v2e[] entries + swaps e2t[] slots |
| `removeFromVec`, `edgeContainsVert` | 10 | Trivial helpers |

## Banked for PR-CR12c (Re-triangulation + Tree + Plane queries)

| Upstream method | LOC | Notes |
|---|---|---|
| `splitEdge(e, v)` | 19 | 1 edge → 2; affected tris × 2 sub-tris each |
| `splitEdge(e, v, &Tree)` | 27 | Adds Tree node tracking |
| `splitTri(t, v)` | 11 | 1 tri → 3 sub-tris (barycentric) |
| `splitTri(t, v, &Tree)` | 23 | Adds Tree node tracking |
| `flipTri(t)` | 8 | Reverse vertex winding |
| `Tree` struct | ~40 | New file `arrangements/tree.rs` |
| `tri_node_id` / `set_tri_node_id` | 6 | Tree-id storage on triangles |
| `tri_orientation(t)` | 10 | Needs Plane axis-drop + CR10 `orient2d` |
| `tri_verts_are_ccw(t, c, p)` | 7 | Pure topology |

PR-CR12c unlocks split-driven re-triangulation, which mesh arrangement (PR-CR13+) consumes.

## References

- Cherchi et al. 2020 — "Fast and Robust Mesh Arrangements using Floating-point Arithmetic" §4.
- Upstream C++ (MIT):
  - `fast_trimesh.cpp:603-620` — `addVert` overloads
  - `fast_trimesh.cpp:813-827` — private `addEdge`
  - `fast_trimesh.cpp:624-646` — `addTri`
  - `fast_trimesh.cpp:395-407` — `triID`
  - `fast_trimesh.cpp:202-220` — `vertOrigID` / `vertNewID`
  - `fast_trimesh.cpp:261-265, 593-597, 320-324, 368-372` — info/flag setters
  - `fast_trimesh.cpp:255-259, 156-160` — resetters
  - `fast_trimesh.cpp:520-535, 238-251` — derived adjacency
- PR-CR11 spec: `specs/cherchi_rs_fast_trimesh_mvp.md`
