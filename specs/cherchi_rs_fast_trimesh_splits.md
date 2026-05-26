# `cherchi-rs::arrangements::FastTrimesh` splits + Tree + Plane queries — Spike PR-CR12c

## Goal

Port the **final third** of the originally-banked PR-CR12 work. Covers:

- Re-triangulation: `split_edge`, `split_tri`, `flip_tri` (with and without `Tree` integration)
- New `Tree` data structure (`arrangements/tree.rs`) for symbolic-split provenance tracking
- `Plane`-using orientation queries: `tri_orientation` (uses CR10 `orient2d` after axis-drop), `tri_verts_are_ccw` (pure topology)
- Helpers: `tri_vert_opposite_to`, `tri_node_id`, `set_tri_node_id`

After PR-CR12c, **FastTrimesh is feature-complete** for the read+mutate cycle that mesh arrangement (PR-CR13+) needs. The C++ sidecar (PR-CR-S1) starts being exercised at PR-CR13+ when arrangement output round-trips through OBJ.

## Public API

### Tree (new module `arrangements/tree.rs`)

```rust
/// A node in the symbolic-split refinement tree. Three triangle
/// vertices + up to three children. Accessed via Tree::get_node.
pub struct Node {
    v: [u32; 3],
    children: [Option<u32>; 3],
}

impl Node {
    pub fn verts(&self) -> [u32; 3];
    pub fn children(&self) -> [Option<u32>; 3];
}

/// Append-only refinement tree. Tracks split provenance: a parent
/// node has 2 children (edge-split) or 3 children (tri-split).
pub struct Tree { /* nodes: Vec<Node> */ }

impl Tree {
    pub fn new() -> Self;
    pub fn with_capacity(cap: usize) -> Self;
    pub fn num_nodes(&self) -> u32;
    pub fn add_node(&mut self, v0: u32, v1: u32, v2: u32) -> u32;
    pub fn get_node(&self, id: u32) -> &Node;
    pub fn add_children(&mut self, parent: u32, children: &[u32]); // 2 or 3
}
```

### FastTrimesh extensions

```rust
// Storage queries
pub fn tri_node_id(&self, t: u32) -> Option<u32>;
pub fn set_tri_node_id(&mut self, t: u32, node_id: u32);
pub fn tri_vert_opposite_to(&self, t: u32, v0: u32, v1: u32) -> Option<u32>;

// Pure topology orientation
pub fn tri_verts_are_ccw(&self, t: u32, curr_v: u32, prev_v: u32) -> bool;

// Plane-using (analytic)
pub fn tri_orientation(&self, t: u32) -> Sign;

// Re-triangulation
pub fn flip_tri(&mut self, t: u32);
pub fn split_edge(&mut self, e: u32, v: u32);
pub fn split_edge_with_tree(&mut self, e: u32, v: u32, tree: &mut Tree);
pub fn split_tri(&mut self, t: u32, v: u32);
pub fn split_tri_with_tree(&mut self, t: u32, v: u32, tree: &mut Tree);
```

### Triangle struct change

```rust
#[derive(Copy, Clone, Debug)]
pub(crate) struct Triangle {
    v: [u32; 3],
    info: u32,
    node_id: Option<u32>,  // NEW (PR-CR12c)
}
```

`from_soup` and `add_tri` initialize `node_id: None`. Setter populates `Some(node_id)`. PR-CR11/CR12a/CR12b tests that touch `tri()` / `tri_info` continue passing unchanged.

## Algorithm

### `split_edge(e, v)` — upstream cpp:708-726

```text
1. (ev0, ev1) = endpoints of edge e.
2. For each t in e2t[e].clone():
   a. v_opp = tri_vert_opposite_to(t, ev0, ev1).unwrap()
   b. If tri_verts_are_ccw(t, ev0, ev1): swap ev0 and ev1.
      (Counter-intuitive but faithful — see CCW-fixup section.)
   c. add_tri(v_opp, ev0, v)
   d. add_tri(v_opp, v, ev1)
3. remove_tris(e2t[e].clone())
```

The clone in step 2 + step 3 mandatory: borrow checker rejects iterating `&self.e2t[e]` while calling `&mut self.add_tri`.

### `split_edge_with_tree(e, v, tree)` — upstream cpp:730-756

Identical to `split_edge` plus:
- After each `add_tri` pair, record the corresponding child nodes via `tree.add_node`
- Get parent's `node_id` via `tri_node_id(t).expect("split target has node_id")`
- `tree.add_children(parent_node, &[c0, c1])`
- `set_tri_node_id(t0, c0)`, `set_tri_node_id(t1, c1)`

### `split_tri(t, v)` — upstream cpp:760-770

Barycentric star subdivision:

```text
1. v0, v1, v2 = tri(t)
2. add_tri(v0, v1, v)
3. add_tri(v1, v2, v)
4. add_tri(v2, v0, v)
5. remove_tri(t)
```

No CCW-fixup needed; all 3 sub-tris use the same parent winding.

### `split_tri_with_tree(t, v, tree)` — upstream cpp:774-796

Identical to `split_tri` plus:
- Get parent's `node_id`
- 3× `tree.add_node`
- `tree.add_children(parent_node, &[c0, c1, c2])`
- 3× `set_tri_node_id`

### `flip_tri(t)` — upstream cpp:800-807

```text
let v = triangles[t].v;
triangles[t].v = [v[2], v[1], v[0]];
```

Adjacency unchanged. Edge set unaffected (edges are stored sorted: (min, max)).

### `tri_orientation(t)` — upstream cpp:549-558

```text
let v0 = self.tri_vert(t, 0); v1 = self.tri_vert(t, 1); v2 = self.tri_vert(t, 2);
match self.plane {
    Plane::XY => orient2d(Point2::new(v0.x(), v0.y()), Point2::new(v1.x(), v1.y()), Point2::new(v2.x(), v2.y())),
    Plane::YZ => orient2d(Point2::new(v0.y(), v0.z()), Point2::new(v1.y(), v1.z()), Point2::new(v2.y(), v2.z())),
    Plane::ZX => orient2d(Point2::new(v0.z(), v0.x()), Point2::new(v1.z(), v1.x()), Point2::new(v2.z(), v2.x())),
}
```

Uses CR10 `orient2d` — Sign return type.

### `tri_verts_are_ccw(t, curr_v, prev_v)` — upstream cpp:539-545

```text
prev_off = tri_vert_offset(t, prev_v).unwrap()
curr_off = tri_vert_offset(t, curr_v).unwrap()
curr_off == (prev_off + 1) % 3
```

### `tri_vert_opposite_to(t, v0, v1) -> Option<u32>` — upstream cpp:452-466

```text
for off in 0..3:
    let v = triangles[t].v[off]
    if v != v0 && v != v1: return Some(v)
return None
```

`debug_assert!(v0 != v1, ...)` matches upstream. Upstream's `assert(false)` final return becomes `None`.

### `tri_node_id(t)` / `set_tri_node_id(t, id)`

Trivial getter/setter on `Triangle.node_id`. `set_tri_node_id(t, u32)` wraps the argument in `Some()`; setter is set-only (no clearing API, matching `set_edge_constr` precedent).

## CCW-fixup detail (the subtle part of `split_edge`)

Upstream cpp:719:
```cpp
if(triVertsAreCCW(t_id, ev0_id, ev1_id)) std::swap(ev0_id, ev1_id);
```

Reading literally: "if the directed pair (ev0→ev1) appears CCW in t's cyclic order, swap them so they appear CW." The post-swap pair forms the basis for the two new triangles `(v_opp, ev0', v)` and `(v_opp, v, ev1')`.

Concrete trace — tetrahedron tri t = [0, 1, 2], splitting edge (0,1):
- v_opp = 2, ev0 = 0, ev1 = 1
- `tri_verts_are_ccw(t, 0, 1)` → off(0)=0, off(1)=1; 1 == (0+1)%3 → **true**
- Swap: ev0 = 1, ev1 = 0
- New tris: (2, 1, v) and (2, v, 0)

After swap, the directed pair (ev0', ev1') is CW within t, so the new triangles inherit a consistent winding that matches t's external neighbors across each un-split edge. **Tests in Group 5 verify the orientation-preservation property.** Faithfully mirror; do not try to "improve" the algorithm.

## Invariants

1. All PR-CR11/CR12a/CR12b invariants preserved post-split: sum |v2e| = 2·E, sum |e2t| = 3·T, edge endpoints sorted, no degenerate tris.
2. `split_edge(e, v)` and `split_tri(t, v)` both consume the target plus call `add_tri`/`remove_tri` — relying on PR-CR12a/CR12b correctness.
3. After `flip_tri(t)`, the multi-set of triangle vertices is preserved; the edge set is preserved; only winding (`v[0]` ↔ `v[2]`) changes.
4. `tri_orientation(t)` is deterministic (Shewchuk-adaptive predicate).
5. `tri_node_id` is `None` for all triangles created via `from_soup` and `add_tri` (without explicit `set_tri_node_id`). Splits with Tree variants populate it.
6. Tree node IDs are dense 0-based indices into `Tree.nodes`. `add_node` returns `(nodes.len() - 1) as u32`.

## Error Contract

- `debug_assert!` on all bounds (consistent with CR11/CR12a/CR12b).
- Mutators return void. Queries return `Option<u32>` for "missing" cases (matches CR11 `edge_id` / `tri_vert_offset` / CR12b `tri_edges_opt` precedent).
- `Tree::add_children`: `debug_assert!` on parent in range + `children.len() ∈ {2, 3}` + "no prior children" (upstream cpp:88).
- `Tree::get_node`: `debug_assert!` on bounds, panics in debug if out of range. Public method returning `&Node`.

## Deliberate Deviations from Upstream

Carry-forward from PR-CR11/CR12a/CR12b (1-14 still in effect).

New for PR-CR12c:

**15. Separate `Triangle.node_id: Option<u32>` field.** Upstream (cpp:436-448) overloads `iTri.info` to store both user data and Tree node IDs. Same foot-gun as the `orig_id` / `info` issue from CR12a — node ID 0 would collide with user-supplied info 0. We add a separate `node_id: Option<u32>` field; `info` stays user-controlled. Cost: 8 bytes per Triangle.

**16. `Tree::add_children(parent, &[u32])` slice form.** Upstream has two overloads (`addChildren(parent, c0, c1)` and `addChildren(parent, c0, c1, c2)`). Rust has no method overloading. The idiomatic Rust form is a single slice-taking method with a runtime debug_assert on `len() ∈ {2, 3}`. Two in-crate callers (`split_edge_with_tree` passes 2, `split_tri_with_tree` passes 3); the assert catches misuse.

**17. `tri_orientation` via explicit axis-drop + CR10 `orient2d`.** Upstream calls `genericPoint::orient2D{xy,yz,zx}` from the LGPL `Indirect_Predicates` library (cpp:554-556). cherchi-rs deliberately does NOT depend on LGPL (paused). We do the projection in Rust: `Plane::XY → orient2d(Point2(x,y), ...)`, `Plane::YZ → orient2d(Point2(y,z), ...)`, `Plane::ZX → orient2d(Point2(z,x), ...)`. Return type changes from upstream `int` (-1/0/+1) to our `Sign` enum (matching CR10).

**18. `set_tri_node_id(t, node_id: u32)` set-only.** Upstream takes `uint`; we match. No `(t, Option<u32>)` form. Splits always set, never clear. If clearing is ever needed, add `clear_tri_node_id` then. Same precedent as `set_edge_constr` (CR12a).

**19. Parallel constructor not ported.** Upstream `FastTrimesh(verts, tris, parallel=true)` uses TBB (`tbb::parallel_sort`, `tbb::parallel_for`, `tbb::spin_mutex`). Hard Rule #5: cherchi-rs is single-threaded by default. Rayon parallelism is a future opt-in feature flag, not a workspace dep. Deferred indefinitely; no consumer demand.

## Test Plan

7 groups, ~32 tests. Located in `#[cfg(test)] mod tests` at the bottom of `fast_trimesh.rs` (Tree tests get a separate `#[cfg(test)] mod tests` in `tree.rs`).

### Group T (in tree.rs) — Tree basics

- `Tree::new` produces empty tree
- `Tree::with_capacity` produces empty tree (capacity hint only)
- `num_nodes` reflects `add_node` count
- `add_node` returns sequential IDs starting from 0
- `get_node` returns expected verts + `[None, None, None]` children initially
- `add_children(parent, &[c0, c1])` → `get_node(parent).children() == [Some(c0), Some(c1), None]`
- `add_children(parent, &[c0, c1, c2])` → `get_node(parent).children() == [Some(c0), Some(c1), Some(c2)]`

### Group 1 — `flip_tri`

- After flip: `tri(t)` returns the reversed vertex triple `[v[2], v[1], v[0]]`
- `tri_edges(t)` unchanged (edge set invariant under winding reversal)
- Adjacency invariants preserved

### Group 2 — `tri_orientation` per Plane

- CCW tri in Plane::XY → Sign::Positive
- CW tri in Plane::XY → Sign::Negative
- Colinear tri in Plane::XY → Sign::Zero
- Same trio for Plane::YZ
- Same trio for Plane::ZX

### Group 3 — `tri_verts_are_ccw` + `tri_vert_opposite_to`

- `tri_verts_are_ccw(t, v[0], v[1])` → true
- `tri_verts_are_ccw(t, v[1], v[0])` → false
- `tri_vert_opposite_to(t, v[0], v[1])` → Some(v[2])
- `tri_vert_opposite_to(t, v[0], 99)` → None (v1 not in tri)

### Group 4 — `split_edge` (no Tree)

- Single-tri mesh: split sole edge (0,1) → 2 tris, 4 verts, 5 edges
- Two-tri quad: split diagonal (0,2) → 4 tris, 5 verts, 8 edges
- Tetrahedron: split edge (0,1) → 5 tris (2 affected become 4 + 2 unaffected)
- After split, sum invariants hold: sum |v2e| = 2·E, sum |e2t| = 3·T
- Orientations: every post-split tri has same `tri_orientation` sign as its parent had pre-split

### Group 5 — `split_tri` (no Tree)

- Single-tri mesh: split → 3 tris, 4 verts, 6 edges
- Tetrahedron: split tri 0 → 6 tris (3 unaffected + 3 sub-tris); sum invariants
- New vertex valence 3 (incident to all 3 sub-tris)

### Group 6 — Tree-integrated splits

- `split_edge_with_tree`: tetra parent tri 0 has `node_id = Some(p)`; after split, 2 new tris have new node IDs; tree.`get_node(p).children() == [Some(c0), Some(c1), None]`
- `split_tri_with_tree`: 3-child variant; `children() == [Some(c0), Some(c1), Some(c2)]`
- Multi-level: split, then split a child; verify tree depth = 2 (child node also has its own children)

### Group 7 — node_id storage independence

- `set_tri_node_id(t, 42)` does NOT affect `tri_info(t)` (regression for deviation #15)
- `set_tri_info(t, 99)` does NOT affect `tri_node_id(t)` (vice versa)
- `from_soup` initializes all triangles with `tri_node_id == None`
- `add_tri` (CR12a) also initializes with `tri_node_id == None`

## Sidecar exercise

**None.** FastTrimesh remains internal scratch storage. Sidecar exercise resumes at PR-CR13+ when arrangement output round-trips through OBJ.

## Banked for Future Work

- **Parallel constructor** — rayon opt-in feature flag. Hard Rule #5; deferred indefinitely.
- **`clear_tri_node_id`** — if a future PR needs to reset tree state. Not in scope; add when needed.

## References

- Cherchi et al. 2020 — "Fast and Robust Mesh Arrangements using Floating-point Arithmetic" §4.
- Upstream C++ (MIT):
  - `fast_trimesh.cpp:708-726` — `splitEdge`
  - `fast_trimesh.cpp:730-756` — `splitEdge` with Tree
  - `fast_trimesh.cpp:760-770` — `splitTri`
  - `fast_trimesh.cpp:774-796` — `splitTri` with Tree
  - `fast_trimesh.cpp:800-807` — `flipTri`
  - `fast_trimesh.cpp:539-545` — `triVertsAreCCW`
  - `fast_trimesh.cpp:549-558` — `triOrientation`
  - `fast_trimesh.cpp:452-466` — `triVertOppositeTo`
  - `fast_trimesh.cpp:436-448` — `triNodeID` / `setTriNodeID`
  - `tree.h:46-108` — Tree + Node
- `crates/cherchi-rs/src/predicates/orient.rs` — CR10 `orient2d` + `Sign`
- PR-CR11 spec: `specs/cherchi_rs_fast_trimesh_mvp.md`
- PR-CR12a spec: `specs/cherchi_rs_fast_trimesh_mutators.md`
- PR-CR12b spec: `specs/cherchi_rs_fast_trimesh_removal.md`
