# `cherchi-rs::arrangements::detect_intersecting_pairs` — Spike PR-CR13

## Goal

Stage 1 of the mesh arrangement algorithm (Cherchi 2020 §5): given a
single triangle soup, find all pairs of triangles whose pairwise
intersection is non-empty. This is the **first arrangement PR** in
cherchi-rs and the first stage that uses CR9's
`triangle_intersects_triangle_3d`.

Upstream's full pipeline (`solve_intersections.cpp:40-67`) decomposes
into four stages:

| Stage | Upstream | LGPL deps | CR13? |
|---|---|---|---|
| 1. Pair detection | `detectIntersections` | **No** — Shewchuk only | **YES** |
| 2. Classification + LPI/TPI | `classifyIntersections` | Heavy | No |
| 3. CDT + earcut re-triangulation | `triangulation` | Via implicit points | No |
| 4. Topology assembly | implicit | n/a | No |

Stage 1 is the only stage that doesn't depend on the LGPL
`Indirect_Predicates` library. CR14+ will hit the **gating decision**:
LGPL indirect predicates vs `dashu::RBig` exact rationals for
intersection-point representation. CR13 surfaces this gate by
producing pair candidates downstream stages will consume.

## Public API

```rust
// crates/cherchi-rs/src/arrangements/intersection_detection.rs

/// Detect all pairs of intersecting triangles in a single mesh.
///
/// Returns a list of `(t_a, t_b)` pairs with `t_a < t_b` where
/// CR9's `triangle_intersects_triangle_3d` reports either
/// `Intersects` or `Coplanar` (filters out `Disjoint`).
///
/// **Algorithm**: O(n²) pairwise iteration with AABB pre-pruning.
/// Per-triangle AABBs are computed once upfront; each pair gets a
/// cheap 6-component overlap check before the expensive triangle-
/// triangle predicate. BVH/Octree is banked for a future PR (Hard
/// Rule #1: no workspace deps).
///
/// **Coplanar pairs are included** in the output, treated uniformly
/// with `Intersects`. Downstream classification (CR14+) consumes
/// both via the same path.
///
/// **Output invariants**:
/// - Every pair satisfies `pair.0 < pair.1`.
/// - Each unordered pair appears at most once.
/// - The list contains exactly the non-`Disjoint` pairs.
pub fn detect_intersecting_pairs(soup: &FastTrimesh) -> Vec<(u32, u32)>;
```

Private helpers:
```rust
fn tri_aabb(soup: &FastTrimesh, t: u32) -> (Point3, Point3);
fn aabbs_overlap(
    a_min: Point3, a_max: Point3,
    b_min: Point3, b_max: Point3,
) -> bool;
```

## Algorithm

1. If `soup.num_tris() < 2`, return empty `Vec`.
2. Pre-compute per-triangle AABBs: `aabbs: Vec<(Point3, Point3)>` of length `n`.
3. For each ordered pair `(i, j)` with `i < j`:
   - If `!aabbs_overlap(aabbs[i], aabbs[j])`, continue.
   - Call `triangle_intersects_triangle_3d(...)` with the 6 vertices of triangles `i` and `j`.
   - If result is `Disjoint`, continue.
   - Else (`Intersects` or `Coplanar`), push `(i as u32, j as u32)` to output.
4. Return the pair vec.

`aabbs_overlap` is the standard 3D box-box overlap predicate:
```text
!(a_max.x < b_min.x || a_min.x > b_max.x
  || a_max.y < b_min.y || a_min.y > b_max.y
  || a_max.z < b_min.z || a_min.z > b_max.z)
```

`tri_aabb(soup, t)` reads `soup.tri_vert(t, 0..2)` and returns the
component-wise min and max.

## Invariants

1. **Total function**: `detect_intersecting_pairs` works on any
   `FastTrimesh`, including empty meshes and single-triangle meshes
   (both return `vec![]`).
2. **Sorted pairs**: every `(a, b)` satisfies `a < b`.
3. **No duplicates**: each unordered pair appears at most once.
4. **No false positives**: every returned pair has CR9 returning
   `Intersects` or `Coplanar`.
5. **No false negatives**: every non-`Disjoint` pair (per CR9) is in
   the output. The property test in Group 7 enforces this by
   comparing against an AABB-free brute force.
6. **Deterministic**: CR9 is Shewchuk-adaptive-deterministic; AABB
   pruning is pure arithmetic; the loop order is fixed.

## Error Contract

No `Result`. The function is total. No `debug_assert!` on inputs —
empty or single-tri inputs are valid (they just produce an empty
result).

## Deliberate Deviations from Upstream

Carry-forward from CR11–CR12c (deviations 1-19 still in effect).

New for PR-CR13:

**20. No spatial index** — upstream uses `cinolib::Octree` for
`O(n log n)` average pair pruning (`intersection_classification.cpp:47-94`).
Cherchi-rs starts with O(n²) + AABB pre-pruning. Justification:
Hard Rule #1 forbids workspace deps (`bvh` crate would violate); a
hand-rolled BVH is its own substantial PR (~150 LOC + own correctness
oracle); not on critical path — O(n²) handles meshes up to ~5k
triangles in tolerable time. Yang-rs callers operate on per-patch
tessellations which are far smaller.

**21. Coplanar pairs included alongside Intersects** — upstream's
`classifyIntersections` consumes both uniformly. Filtering at CR13
would force downstream re-detection. CR9's `TriangleIntersection`
enum exposes the discriminant if a future caller needs to distinguish.

## Test Plan

7 groups, ~26 tests, in `#[cfg(test)] mod tests` at the bottom of
`intersection_detection.rs`.

### Group 1 — Boundary conditions (3)

- Empty mesh → empty pair list
- Single triangle → empty pair list
- Two AABBs that don't overlap at all → empty pair list

### Group 2 — Disjoint triangles (3)

- Two triangles far apart in space
- Overlapping AABBs but actually disjoint geometry (proves AABB doesn't false-positive)
- Two parallel triangles in different planes

### Group 3 — Intersecting triangles (4)

- Two triangles crossing through each other → 1 pair
- Edge-touching pair (CR9 returns Intersects per the docstring discovery) → 1 pair
- Vertex-touching pair → 1 pair
- T-junction (vertex of A on edge of B) → 1 pair

### Group 4 — Coplanar pairs (3)

- Two overlapping coplanar triangles → 1 pair
- Two coplanar triangles sharing an edge → 1 pair
- Two coplanar triangles touching at a vertex → 1 pair

### Group 5 — Multi-triangle meshes (5)

- Tetrahedron (closed manifold) → empty pair list (all face pairs share an edge but CR9 returns Intersects for them — see note below)
- Cube-with-cube-offset-0.5 → known multi-pair count
- Star-of-david style (two coplanar overlapping triangles) → multiple pairs
- 5 random tris with no intersections → empty pair list
- 3-tri non-manifold edge → all adjacent tris paired

### Group 6 — Pair invariants (3)

- All pairs satisfy `pair.0 < pair.1`
- No duplicate pairs in output
- Empty pair list iff no intersecting pairs

### Group 7 — Property test against brute-force (1, multi-fixture)

For 5-10 deterministic fixtures of varying complexity, assert that
`detect_intersecting_pairs(soup)` returns the same set as an
AABB-free brute-force enumeration. Guards against AABB false-negatives
— the main risk class for pre-pruning.

**Note on Group 5 tetrahedron**: CR9 treats shared-edge pairs as
`Intersects` (the entire shared edge IS the intersection). So a
tetrahedron's 4 faces, each sharing edges with all 3 others, produces
6 pairs (one per shared edge). This matches Group 3's edge-touching
test. The test name will say "tetrahedron faces all pairwise share edges"
to make the count clear.

## Sidecar exercise

**None in CR13.** Output is a pair list, not OBJ. Sidecar exercise
(diff against upstream `mesh_booleans` binary) resumes at Stage 3+
when re-triangulated mesh becomes OBJ-comparable (likely PR-CR15+).

## Banked for Future Work

- **BVH or Octree spatial index** — swap O(n²) → O(n log n) average.
  Hand-rolled BVH (~150 LOC) or eventual workspace dep allowance.
  Trigger: meshes >5k triangles become common.
- **Two-soup signature** — `detect_cross_pairs(soup_a, soup_b)` skipping
  within-soup pairs. Useful when boolean operands are individually
  self-non-intersecting. Defer unless a real consumer needs it.
- **`CandidatePair { a, b, kind }` exposing the discriminant** — only
  if downstream needs `Intersects` vs `Coplanar` without re-running CR9.
- **Stage 2: classification + intersection-point insertion** — the
  next arrangement PR (PR-CR14). **GATING DECISION**: LGPL
  `Indirect_Predicates` vs `dashu::RBig` exact rationals for
  intersection-point representation. Must be resolved before CR14.

## References

- Cherchi et al. 2020 — "Fast and Robust Mesh Arrangements using
  Floating-point Arithmetic" §5 (the arrangement algorithm).
- Upstream C++ (MIT):
  - `solve_intersections.cpp:40-67` — full pipeline
  - `intersection_classification.cpp:47-94` — `detectIntersections`
- `crates/cherchi-rs/src/predicates/triangle_intersect.rs` — CR9
  `triangle_intersects_triangle_3d` + `TriangleIntersection` enum.
- `crates/cherchi-rs/src/arrangements/fast_trimesh.rs` — input type.
- CR12c spec: `specs/cherchi_rs_fast_trimesh_splits.md` (FastTrimesh
  feature-complete; CR13 is the first algorithmic consumer).
