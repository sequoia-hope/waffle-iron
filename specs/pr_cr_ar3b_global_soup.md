# PR-CR-AR3b — cherchi-rs: global conforming soup + orchestration (M6)

Spec of record for the FIP cycle. Pins the public surface, types, orchestration
step order, the interner/dedup contract, the input-prep contract, the in-scope
vs loud-deferred boundary, and the oracle invariants + hand corpus. Ported from
Cherchi 2020/2022 (`solve_intersections.cpp` `meshArrangementPipeline`,
`triangle_soup.{h,cpp}`, `processing.cpp` prep, `triangulation.cpp` assembly
loop). MIT.

## 1. What this PR delivers

The **global assembly** that ties the existing per-stage native-arrangement
pieces (CR13 detect, AR1 classify, AR2a split, AR2b/C1 TPI, AR3a enforce) into
the C++ `meshArrangementPipeline`: input → multiplier → input-vertex dedup +
degenerate/dup-triangle removal → build the global soup → detect → classify →
group points/segments → per intersecting base triangle build a 1-triangle
submesh, `split_single_triangle` + `enforce_constraints`, then assemble each
output sub-triangle into a **global** `out_tris`/`out_labels` (welding shared
vertices to one global id, interning new implicit points with structural dedup)
→ append jolly points.

Outcome: a native arrangement that produces a non-self-intersecting,
intersection-conforming triangle soup with consistent per-triangle solid labels
for the **non-recursive common case** (transversal, non-coplanar inputs:
tetrahedra-pairs, interpenetrating boxes). First time `cherchi-rs` produces a
native mesh arrangement end-to-end. This is the input BL1 (patch flood-fill)
consumes next. It is the conforming soup **before** in/out + patch_id — those
are BL1/BL2, NOT here.

## 2. Module + gating

New module `crates/cherchi-rs/src/arrangements/soup.rs`, gated
`#[cfg(feature = "indirect-predicates")]` (it calls AR1/AR2a/AR3a, all FFI-gated).
MIT attribution header naming the four ported upstream sources. Re-exported from
`arrangements/mod.rs`; the public entry re-exported from `lib.rs` (also gated).

## 3. Public surface (pin exactly)

```rust
/// Per-output-triangle "which input solid(s) it lies on" — the set-of-solids
/// label. Reuses the existing `InputId` newtype (labeled_arrangement.rs).
/// Stored as a sorted-unique Vec<InputId> (the C++ std::bitset<NBIT>, OR-merged
/// across duplicate input triangles in prep, carried verbatim onto every output
/// sub-triangle of a parent base triangle).
pub type Label = Vec<InputId>;

/// The complete conforming triangle soup produced by the native arrangement.
/// `verts` holds every global vertex as typed coordinates (Explicit input
/// corners + interned Lpi/Tpi implicit points), with the 5 jolly points
/// appended at the tail. `tris` indexes into `verts`; `labels` is 1:1 with
/// `tris`. This is the pre-in/out, pre-patch_id soup (BL1 consumes it).
pub struct ArrangementSoup {
    pub verts: Vec<VertexCoords>,
    pub tris: Vec<[u32; 3]>,
    pub labels: Vec<Label>,
    /// Count of jolly points appended at the tail of `verts` (always 5). The
    /// real arrangement vertices are `verts[..verts.len() - jolly_count]`.
    pub jolly_count: u32,
}

/// Loud failure surface — never silent (P9/P10). Wraps the deferred walls.
pub enum ArrangementError {
    /// A candidate pair is coplanar / single-coplanar-edge (AR1
    /// `Deferred(Coplanar | SingleCoplanarEdge)`) — Stage 0 / M8.
    CoplanarPairDeferred { ta: u32, tb: u32, reason: DeferReason },
    /// AR1 flagged a degenerate configuration that slipped past prep.
    DegeneratePairDeferred { ta: u32, tb: u32 },
    /// Point insertion located a point outside its base triangle (AR2a
    /// `RetriangulateError::NoContainingTriangle`).
    Retriangulate { base_tri: u32, point_id: u32 },
    /// Constraint enforcement hit the AR3a global-state wall: a crossed
    /// constraint edge has no recorded supporting plane / TPI planes not in
    /// general position. THIS is the N16 deep-recursion / coplanar-jollyPoint
    /// deferral. Wraps `EnforceError::{SourcePlaneUnavailable, DegenerateTpi,
    /// SegmentNotLocatable, EndpointNotInSubmesh}`.
    DeepRecursionRequired { base_tri: u32, detail: EnforceError },
    /// Malformed caller input (bad triangle index, count overflow) surfaced by
    /// the global-soup `FastTrimesh::from_soup`.
    Input(FastTrimeshError),
    /// `labels.len()` != input triangle count.
    LabelCountMismatch { tris: usize, labels: usize },
}

/// Build the native mesh arrangement for one triangle soup with per-triangle
/// input-solid labels.
///
/// `coords`: flat xyz triples (len % 3 == 0). `tris`: index triples into the
/// vertex list. `in_labels`: 1:1 with `tris`, each the set of input solids that
/// triangle belongs to (for a binary A∪B: A's tris carry `[InputId(0)]`, B's
/// `[InputId(1)]`).
pub fn mesh_arrangement(
    coords: &[f64],
    tris: &[[u32; 3]],
    in_labels: &[Label],
) -> Result<ArrangementSoup, ArrangementError>;
```

Re-export `ArrangementSoup`, `ArrangementError`, `Label`, and `mesh_arrangement`
from `arrangements/mod.rs` and `lib.rs` (gated).

## 4. Orchestration step order (port of `meshArrangementPipeline`)

1. `init_fpu()` (the FFI predicates require it; AR1/AR2a/AR3a already call it,
   call once up front too).
2. **Multiplier**: `let m = compute_multiplier(coords);` then a scaled copy
   `let mut sc = coords.to_vec(); multiply_coordinates(&mut sc, m);`
   (CR2/CR3 — reuse, do not re-port). All downstream geometry uses `sc`.
3. **`merge_duplicated_vertices`** (prep, §5): coincident scaled-input verts →
   one global id; remap `tris`. Returns `(verts: Vec<Point3>, remapped_tris:
   Vec<[u32;3]>)` over the deduped vertex list.
4. **`remove_degenerate_and_duplicated_triangles`** (prep, §5): drop
   exact-collinear tris (CR1 `points_are_collinear_3d`); dedup sorted-vertex
   tris, OR-merging their labels. Returns `(kept_tris, kept_labels)`.
5. **Build the global soup** `FastTrimesh::from_soup(&verts, &kept_tris,
   plane?)`. The per-triangle reference plane is computed per base triangle
   (`max_component_in_triangle_normal` → `Plane`, §6); `from_soup` takes ONE
   plane, so pass any (e.g. `Plane::XY`) for the global soup — the per-triangle
   submeshes get their own correct plane (step 9). Map any
   `FastTrimeshError` → `ArrangementError::Input`.
6. **Detect**: `detect_intersecting_pairs(&soup)` (CR13).
7. **Classify**: `classify_all(&soup, &pairs)` (AR1). Scan results: any
   `Deferred(Coplanar | SingleCoplanarEdge)` → `CoplanarPairDeferred`;
   `Deferred(Degenerate)` → `DegeneratePairDeferred` (loud, return Err — do NOT
   skip silently).
8. **Group**: `group_intersection_points(&soup, &classified)` →
   `(points, buckets)`; `group_constraint_segments(&soup, &classified, &points)`
   → `segments_per_tri`.
9. **Per base triangle `t` in `0..num_tris`** (serial loop — Hard Rule #5):
   - **Fast path** (`triangulation.cpp:147-156`): if `t` has no intersection
     points (its bucket interior+edges all empty) AND no constraint segments,
     emit it straight through — push `soup.tri(t)` (already global ids) to
     `out_tris`, push `soup_labels[t]` to `out_labels`. Continue.
   - **Split path**: build a 1-triangle submesh
     `FastTrimesh::from_soup(&[c0,c1,c2], &[[0,1,2]], plane_t)` where `c0..c2 =
     soup.tri_vert(t, 0..2)` and `plane_t` = step-6 plane for `t`.
     - `split_single_triangle(&mut subm, &flat_points_for_t)` (AR2a). The flat
       point list is the bucket's interior ++ each edge's points, as a
       `&[TypedPoint]` resolved from the global `points` Vec by id (match AR2a's
       own test convention — interior and on-edge fed as ONE flat slice).
       Map `RetriangulateError` → `ArrangementError::Retriangulate`.
     - `enforce_constraints(&mut subm, &segments_per_tri[t], &points)` (AR3a).
       Map `EnforceError` → `ArrangementError::DeepRecursionRequired` (the loud
       N16 wall).
     - **Assemble** (the `vertOrigID` step, `triangulation.cpp:123-130`): for
       each sub-triangle `subm.tri(st)`, map every submesh vertex → a global id
       (§7 interner), push the global triple to `out_tris`, push
       `soup_labels[t]` to `out_labels` (every sub-tri inherits its parent base
       triangle's label).
10. **Append jolly points** (`appendJollyPoints`, §8): push 5 explicit jolly
    points (`init_jolly_points` constants × `m`) to the global `verts`; set
    `jolly_count = 5`.
11. Return `ArrangementSoup { verts, tris: out_tris, labels: out_labels,
    jolly_count }`.

> The global `verts` accumulates: first the deduped input corners (Explicit,
> §3), then interned implicit points (Lpi/Tpi) appended on demand during step-9
> assembly, then the 5 jolly points. `out_tris` indexes into this list.

## 5. Input-prep contract

### `merge_duplicated_vertices(coords_scaled, tris) -> (Vec<Point3>, Vec<[u32;3]>)`
Port of `processing.cpp:67-119` (serial branch). Insertion-ordered dedup by
exact `[f64;3]` equality (the C++ `flat_hash_map<array<double,3>, uint>`): walk
each triangle's three indices; the first time a coordinate triple is seen it
gets the next global id and is pushed to `verts`; every occurrence remaps to
that id. Bit-exact `f64` equality (no tolerance) — coordinates are post-scale.
Only vertices referenced by some triangle survive (matches C++, which iterates
`in_tris`).

### `remove_degenerate_and_duplicated_triangles(verts, tris, labels) -> (Vec<[u32;3]>, Vec<Label>)`
Port of `processing.cpp:124-172`. For each triangle in order:
- **Degenerate**: if `points_are_collinear_3d(verts[v0], verts[v1], verts[v2])`
  (CR1 — exact), drop it (and its label).
- **Duplicate**: key by the **sorted** `[v0,v1,v2]`. First occurrence keeps the
  triangle (original winding) + its label, recorded at a running output index.
  A later duplicate is dropped but its label is **OR-merged** (set-union of
  `InputId`s, kept sorted-unique) into the first occurrence's label.

Output `tris` preserves first-seen order; `labels` is 1:1 with it.

## 6. Per-triangle reference plane

`Plane` for base triangle `t` = map of `max_component_in_triangle_normal(c0,c1,
c2)` (predicates::orientation, returns `Axis`) via the C++ `intToPlane`
composition with `maxComponentInTriangleNormal`'s 0/1/2 = X/Y/Z:
`Axis::X → Plane::YZ`, `Axis::Y → Plane::ZX`, `Axis::Z → Plane::XY`
(drop the dominant-normal axis; `common.h:46`). Used as the submesh plane in
step 9 so AR2a/AR3a's reference-plane `orient2d` is correct.

## 7. Interner / global-id contract (the load-bearing weld)

Each output submesh vertex maps to a global id as follows (port of
`subm.vertOrigID`, but reconstructed because the Rust `FastTrimesh` assigns only
submesh-local ids with `orig_id = None`):

- **Input corner** — `subm.vert_coords(v)` is `Explicit(p)` AND `p` equals one
  of the base triangle's three corners `soup.tri_vert(t, 0..2)`: map to that
  corner's **global input id** `soup.tri(t)[k]` (the global vertex id, already
  in `verts`). Match by exact `Point3` equality against the three corners.
- **New implicit / interior-or-edge point** — any other vertex (`Lpi`, `Tpi`,
  or an `Explicit` that is not a base corner, e.g. an input vertex of the OTHER
  solid piercing this face): intern into a global `HashMap<VertexCoords, u32>`
  (keyed by structural `VertexCoords` equality — the SAME dedup
  `group_intersection_points` uses). First sight appends the `VertexCoords` to
  the global `verts` and assigns the next id; repeat sights reuse it.

**Why this welds shared intersection vertices to one id:** AR1 emits ONE shared
`TypedPoint` (identical `Lpi { line, plane }` / `Tpi { v, w, u }` generators)
for an intersection vertex regardless of which of the two intersecting triangles
it is processed under. Both triangles' submeshes therefore carry a vertex with
byte-identical `VertexCoords`, so both map to the same global id via the
structural-equality interner. This satisfies oracle invariant #2/#3 (shared
intersection vertices have consistent global ids).

`VertexCoords` is `Copy + PartialEq` but not `Hash`/`Eq` (it holds `f64`). The
interner therefore uses a `Vec<(VertexCoords, u32)>` linear-probe OR a wrapper
providing bit-exact hash/eq over the `f64` generators. Bit-exact, no tolerance.

## 8. Jolly points

Port of `triangle_soup.cpp:381-388` `initJollyPoints` + `appendJollyPoints`. The
5 explicit points (each component × multiplier `m`):
```
( 0.94280904158,  0.0,          -0.333333333) * m
(-0.47140452079,  0.81649658092,-0.333333333) * m
(-0.47140452079, -0.81649658092,-0.333333333) * m
( 0.0,            0.0,           1.0)          * m
( 1.0,            0.0,           0.0)          * m   (= (m, 0, 0))
```
Appended to `verts` as `VertexCoords::Explicit`. They are not referenced by any
triangle in this PR (BL-stage ray-cast consumes them); `jolly_count = 5`.

## 9. In-scope vs loud-deferred (P9/P10)

**In scope** — the non-recursive common case (transversal, non-coplanar):
tetrahedra-pairs, interpenetrating boxes (axis-aligned and rotated).

**Loud-deferred — return a classified `ArrangementError`, NEVER silent:**
- **N16 deep recursion** — a constraint segment crossing MULTIPLE existing
  constraints needing the global `seg2tris` + coplanar `jollyPoint`. Surfaces as
  `EnforceError::{SourcePlaneUnavailable, DegenerateTpi}` →
  `DeepRecursionRequired`.
- **Coplanar pairs** (AR1 `Deferred`) — Stage 0 / M8 →
  `CoplanarPairDeferred`.
- Boolean labeling (BL1/BL2/BL3) — not here.
- The `tbb` parallel path — single-threaded serial loop only.

**STOP-and-report condition:** if wiring the *common* corpus (tetra-pair, two-box
overlap) reveals deep recursion is needed for the common case — or the existing
per-stage pieces do not compose on real multi-triangle input — STOP and report.
Do NOT improvise the global `seg2tris` or any fallback. Land the orchestration +
soup for the non-recursive common case (committed/pushed), defer the rest.

## 10. Oracle invariants (RED tests — structural + EXACT, no float tolerance)

Use the established pure-`dashu` exact helpers (`to_r`, `exact_signed_area2_xy`,
exact LPI line∩plane coords) from `retriangulate.rs` / `enforce.rs` test
modules; add an exact TPI 3-plane rational solve where needed. Tests inline in
the new module's `#[cfg(test)] mod tests`.

1. **Conforming soup (load-bearing):** no two output triangles intersect in
   their interiors — checked in EXACT rational arithmetic (compute each global
   vertex's exact coords; exact tri-tri interior-intersection test). Not float
   tolerance.
2. **Every detected intersection realized:** each CR13 intersecting pair's
   intersection appears in the soup as shared/constraint edges (both inputs are
   conformed along it; the LPI/TPI vertices are present with shared global ids).
3. **Topology sanity:** consistent shared-vertex ids (coincident implicit points
   share one id); valid triangles (non-degenerate, exact area > 0); Euler /
   edge-incidence sanity on closed inputs.
4. **Input-prep correctness:** duplicated input vertices merged;
   degenerate/dup input triangles removed (exact), labels OR-merged.
5. **Hand cases:** two tetrahedra sharing a face/edge; a small two-box overlap
   (axis-aligned AND a rotated case) → conforming soup with the expected shared
   intersection edges; a non-intersecting pair → soup == inputs (modulo prep).

## 11. CI gate (must be clean before done)

- `cargo test -p cherchi-rs` (DEFAULT — FFI-free / WASM-clean; prior tests
  unregressed)
- `cargo test -p cherchi-rs --features indirect-predicates`
- `cargo fmt -p cherchi-rs -- --check`
- `cargo clippy -p cherchi-rs --all-targets --features indirect-predicates -- -D
  warnings` (+ default)
- No `unsafe` / `panic!` in production; single-threaded; MIT attribution header
  on the new module.

## 12. Faithful reuse (do NOT re-port)

| Need | Reuse | File |
|---|---|---|
| candidate pairs | `detect_intersecting_pairs(&FastTrimesh)` | `arrangements/intersection_detection.rs` |
| classify pairs | `classify_all(&FastTrimesh, &[(u32,u32)])` | `arrangements/intersection_points.rs` |
| point grouping | `group_intersection_points` → `(Vec<TypedPoint>, Vec<TriangleAuxPoints>)` | `arrangements/aux_structure.rs` |
| segment grouping | `group_constraint_segments` → `Vec<Vec<ConstraintSegment>>` | `arrangements/aux_structure.rs` |
| point insertion | `split_single_triangle(&mut FastTrimesh, &[TypedPoint])` | `arrangements/retriangulate.rs` |
| constraint enforcement | `enforce_constraints(&mut FastTrimesh, &[ConstraintSegment], &[TypedPoint])` | `arrangements/enforce.rs` |
| submesh + typed verts + `vert_orig_id`/`vert_coords`/`tri` | `FastTrimesh`, `VertexCoords` | `arrangements/fast_trimesh.rs` |
| multiplier | `compute_multiplier` / `multiply_coordinates` | `processing/multiplier.rs` |
| exact collinearity | CR1 `points_are_collinear_3d` | `predicates/collinearity.rs` |
| reference plane | `max_component_in_triangle_normal` → `Axis` | `predicates/orientation.rs` |
| label newtype | `InputId` | `labeled_arrangement.rs` |
