# PR-CR-AR2b — cherchi-rs arrangement: constraint segments + TPI

**Status:** in progress (M6, second half of PR-CR-AR2).
**Plan of record:** `docs/yang_functional_roadmap.md` §M6.
**C++ reference:** `/home/claude/cherchi2022/InteractiveAndRobustMeshBooleans/arrangements/code/triangulation.cpp`.

This spec is the Manager's planning artifact for the role-separated FIP cycles
(Spec → RED → GREEN → Adversary, distinct sync sub-agents per role). It records
the load-bearing design decisions so each sub-agent shares one source of truth.

## Goal

AR2a inserts every AR1 intersection **point** into a per-base-triangle submesh
(valid covering sub-triangulation). AR2b **enforces the intersection SEGMENTS as
constraint-flagged mesh edges**, constructs the N13-deferred **TPI** points where
two constraint segments cross, and **replaces the N13 raw-`f64`
`point_in_segment` stopgap with an exact predicate**. Cross-triangle welding /
global conforming soup (AR3) and boolean labeling (BL*) are OUT of scope.

## Load-bearing design (validated against the C++)

The C++ `createTPI` (cpp:1007) finds each segment's supporting plane via the
global `g.segmentTrianglesList(seg)`, but that list only ever holds the segment's
originating `(tA,tB)` pair (`addTrianglesInSegment`, aux_structure.cpp:119-133).
Skipping the base triangle (`vectorsAreEqual(tv1, ref_t) → continue`, cpp:1050)
leaves exactly the **other** triangle of the AR1 pair — its 3 corners are the
supporting plane, and it is non-coplanar with the base by construction (AR1 defers
all coplanar pairs to N13, so the jolly-point coplanar fallback,
`computeTriangleOfSegmentInCoplanarCase` cpp:1076, is **unreachable** in AR2b
scope). Therefore `create_tpi` needs **no global state**: each constraint segment
carries its source triangle (3 `Point3`) directly, threaded into the per-base-tri
segment list and propagated to sub-segments on split (local analog of C++
`sub_segs_map` → `seg2tris`, collapsed to one `sub_segment → source_tri` map).
TPI points are added to the **local submesh only** with **submesh-local**
structural dedup (no global vertex weld — that is AR3).

**STOP condition (P9/P10):** if implementation reveals a segment whose supporting
plane is genuinely coplanar with the base, or that a TPI plane needs a triangle
from another base triangle's arrangement, STOP and report rather than improvise.

---

## Cycle A — FFI segment predicates (`indirect-predicates-sidecar-rs`)

Demand-driven (Cycle C is the caller). `ImplicitPoint3DTpi<'a>` + `AsGenericPoint`
already exist (lib.rs:640-705,780). Only the segment predicates are missing.

C++ `genericPoint` static methods (implicit_point.h:201-218):
- `innerSegmentsCross(A, B, P, Q)` — segments {A,B} and {P,Q} cross at a point
  strictly interior to **both** segments.
- `pointInInnerSegment(p, v1, v2)` — `p` lies strictly between `v1` and `v2`.
- `pointInSegment(p, v1, v2)` — `p` lies on `[v1,v2]` (endpoints included).

Work items:
- `src/wrapper.h` / `src/wrapper.cpp`: three `extern "C"` bridges over the
  `ip_point_in_triangle` precedent (wrapper.cpp:313) →
  `genericPoint::innerSegmentsCross` / `pointInInnerSegment` / `pointInSegment`,
  each via `*(const genericPoint*)` static dispatch, returning `int` 0/1.
- `src/stub.cpp`: three matching sentinel stubs returning `0`.
- `src/lib.rs`: `pub fn inner_segments_cross(a,b,p,q: &impl AsGenericPoint) -> bool`,
  `point_in_inner_segment(p,v1,v2)`, `point_in_segment(p,v1,v2)`.

**Oracle:** unit tests on Explicit + LPI + TPI handles vs hand-verified
crossings / non-crossings; `AVAILABLE`-gated fail-loud (panic if `!AVAILABLE`,
matching the crate convention — `point_in_triangle` tests precedent).

---

## Cycle B — exact `point_in_segment` + constraint-segment extraction (`cherchi-rs`)

1. **Exact predicate (closes N13 `f64` deviation).** Add
   `point_strictly_inside_segment_3d(w, p, q: Point3) -> bool` = CR1
   `points_are_collinear_3d` (predicates/collinearity.rs) + exact `dashu`
   betweenness. Replace the raw-`f64` `point_strictly_inside_segment` at
   `arrangements/intersection_points.rs:~390`. Callers feed **explicit-only**
   points (`any_triangle_vertex_strictly_inside_segment`), so no FFI needed.
   Faithful migration: preserve every structural assertion of affected tests;
   change only expected outcomes where the exact predicate legitimately differs.
2. **`VertexCoords::Tpi` variant** (data only) in `arrangements/fast_trimesh.rs`:
   `Tpi { v: [Point3;3], w: [Point3;3], u: [Point3;3] }` (9 generators matching
   `ImplicitPoint3DTpi::new`). Update exhaustive matches in `vert_point` (Tpi →
   `lambda3d_tpi_interval` midpoint, bookkeeping only — never oracle-checked,
   mirroring the Lpi approx convention) and the `tri_orientation` debug_assert.
3. **Constraint-segment extraction** in `arrangements/aux_structure.rs`: type
   `ConstraintSegment { endpoints: (u32, u32), source_tri: [Point3; 3] }` and a
   per-base-triangle `Vec<Vec<ConstraintSegment>>` (new `group_constraint_segments`).
   For each `Transversal { vertices }` of pair `(ta,tb)`: the two intersection
   vertices form the segment (interned ids reused from the point set); in `ta`'s
   submesh `source_tri` is `tb`'s 3 corners and vice-versa.

**Oracle:** exact betweenness agrees with the removed `f64` stopgap on
non-degenerate cases AND is correct on a collinear-but-just-outside case the
`f64` version could misjudge; extraction produces one constraint segment per
transversal pair per base triangle with the correct opposite-triangle `source_tri`.

---

## Cycle C — `addConstraintSegment` driver + `createTPI` + walk + earcut (`cherchi-rs`)

New module `arrangements/add_constraints.rs` (MIT header). Faithful port:

- `pub fn add_constraint_segments_in_single_triangle(subm, segments) -> Result<…>`
  (cpp:576): LIFO loop, per-triangle `sub_segs_map` analog (`sub_segment → source_tri`).
- `add_constraint_segment` (cpp:597): Branch A edge-exists → `set_edge_constr`;
  Branch B else `find_intersecting_elements` → `boundary_walker` ×2 →
  `earcut_linear` ×2 → `add_tri` new → `remove_tris` strip → flag the realized edge.
- `find_intersecting_elements` (cpp:644): star bootstrap + topology walk; on
  crossing an existing **constraint** edge → `create_tpi`, `split_edge`, flag both
  halves, re-queue + `split_segment_in_subsegments`; on a vertex-on-segment →
  split + re-queue.
- `create_tpi` (cpp:1007): build `ImplicitPoint3DTpi` from base-tri corners + the
  two crossing segments' `source_tri`s; add as `VertexCoords::Tpi` to the local
  submesh with **submesh-local structural dedup** (NOT global AR3 weld).
- `compute_triangle_of_segment` (cpp:1041) → return segment's `source_tri`
  directly (no global `segmentTrianglesList`; jolly-point coplanar fallback
  omitted — unreachable; assert/STOP if a source_tri is coplanar w/ base).
- `segments_intersect_inside` (cpp:1170) → Cycle-A `inner_segments_cross`;
  `point_inside_segment` (cpp:1178) → Cycle-A `point_in_inner_segment`;
  `split_segment_in_subsegments` (cpp:1185) → propagate `source_tri` to children.
- `earcut_linear` (cpp:912; NOT the non-linear `earcut`) over ref-plane `orient2d`;
  `boundary_walker` (cpp:806).
- **FastTrimesh thin accessors:** `edge_opp_to_vert(t,v)` (cpp:328),
  `tri_opp_to_edge(e,t) -> Option<u32>` (cpp:470). Others already exist.
- **Gp dispatch expansion:** `retriangulate.rs` `Backing`/`gp`/`dispatch_*` gain
  the `Tpi` arm → `point_in_triangle` 16→81 arms, `orient2d` 8→27, plus new
  `dispatch_inner_segments_cross` (81) + `dispatch_point_in_inner_segment` (27). A
  declarative macro may generate the arms. Always read `vert_coords` (exact),
  never the `vert()` midpoint.

**Oracle:**
1. Every intersection segment present post-AR2b as a chain of **constraint-flagged**
   edges covering it end-to-end (no gap; no segment crossing a non-vertex edge interior).
2. TPI exactness: each TPI lies on all three supporting planes via exact
   `orient3d == Zero` (FFI, NOT float tolerance).
3. AR2a covering invariant still holds post-constraint (exact `orient2d`, pure-`dashu`).
4. Hand cases: two crossing segments → exactly one TPI on all three planes; a
   segment coinciding with an existing edge → flagged, no new vertex.
Document that full cross-triangle C++-binary parity is AR3.

---

---

## Cycle C RE-SCOPE — C1 (real TPI routing) lands; C2 (enforcement) STOP→AR3

**Status:** the original Cycle C above is **split**. Investigation (this section
is the report) confirmed the anticipated STOP: the C++ `createTPI`
(triangulation.cpp:1007) sources the TPI's 2nd/3rd supporting planes via
`computeTriangleOfSegment` (cpp:1041), which queries the **global**
`AuxiliaryStructure::seg2tris` map for a non-coplanar witness triangle and falls
back to a global `jollyPoint` for coplanar cases. The Cycle-B
`ConstraintSegment.source_tri` is a correct local substitute **only** for an
original transversal segment's witness — it does NOT cover mid-recursion
sub-segments' provenance or the coplanar fallback without reintroducing the
global structures. That is AR3-level state. Per the brief's P9/P10 STOP
condition, the `addConstraintSegment` / `createTPI` enforcement core is
**re-scoped to Cycle C2 / AR3** — it is NOT improvised here.

### Cycle C1 (THIS PR) — Piece 1 only: real `ImplicitPoint3DTpi` handle routing

Make `VertexCoords::Tpi` points route through the per-base-triangle
re-triangulation machinery as **real, exact** `ImplicitPoint3DTpi` handles,
replacing the Cycle-B centroid (`sum/9`) placeholder, with predicate dispatch
covering the Tpi arm. This **resolves the N13 TPI-deferral at the
predicate/handle layer** and de-risks C2; it does NOT realize segments as
constrained edges (that needs the global state above → C2).

All edits in `crates/cherchi-rs/src/arrangements/retriangulate.rs`. The IP FFI
already exposes everything needed (`ImplicitPoint3DTpi::new(v1..v3,w1..w3,u1..u3)`
at lib.rs:648, `orient3d`/`point_in_triangle`/`orient2d_*` accept all handle
types via the sealed `AsGenericPoint`) — **no new FFI wrapper**.

Work items (GREEN):
1. Import `ImplicitPoint3DTpi` into the sidecar `use` list.
2. `Backing.gens` for the `Tpi` arm: the 9 explicit generators in
   `v1,v2,v3,w1,w2,w3,u1,u2,u3` order (mirrors `ImplicitPoint3DTpi::new`).
3. `Gp<'a>` enum: add `T(ImplicitPoint3DTpi<'a>)`.
4. `gp()` `Tpi` arm: build `Gp::T(ImplicitPoint3DTpi::new(&b.gens[0], … &b.gens[8]))`
   — delete the `sum/9` centroid stand-in.
5. `dispatch_point_in_triangle` (E/L → E/L/T, 16→81) and `dispatch_orient2d`
   (8→27): a `macro_rules!` (`with_gp!`) that nests the 3-variant destructure,
   monomorphizing to the identical concrete `point_in_triangle` / `orient2d_*`
   calls (these are already generic over `&impl AsGenericPoint`).

**Safety (load-bearing):** call ONLY the safe `genericPoint::`-static wrappers
(`point_in_triangle`, `orient2d_*`). NEVER `_II`/`_IIII` variants — they segfault
on explicit input (CR-IP6 memory).

### Cycle C1 test surface (RED)

Exercise routing through the **public** `split_single_triangle` by inserting a
`VertexCoords::Tpi` interior point — no new private hook. Construct a known
3-plane configuration whose exact intersection is a known interior point of the
base triangle, build the `Tpi` generators, insert it.

**Oracle (exact, NOT float tolerance):** build the real `ImplicitPoint3DTpi`
handle + explicit corner handles; assert the inserted TPI lies on **all three**
supporting planes via `indirect_predicates_sidecar_rs::orient3d == Sign::Zero`
(each plane = three of the nine generators). The placeholder centroid does NOT
lie on the planes → `orient3d != Zero`, so this is RED before C1, GREEN after.
Add an independent pure-`dashu` 3×3 plane-solve cross-check of the point, and
reuse the existing exact covering-triangulation oracle (signed-area-sum +
same-sign winding) to confirm a valid covering survives Tpi insertion. Group in
the established 5-group structure; mirror the AR2a hand-case naming.

### Cycle C2 (deferred → AR3) — OUT of scope here

`addConstraintSegmentsInSingleTriangle` / `addConstraintSegment` /
`findIntersectingElements` / `createTPI` (the segment-crossing creator) /
`computeTriangleOfSegment` / `segmentsIntersectInside` /
`splitSegmentInSubSegments` / `boundaryWalker` / `earcutLinear`; global
conforming soup / cross-triangle welding; boolean labeling; the C++ `tbb`
parallel path. N13's `f64`-guard was resolved in Cycle B; the **N13 TPI-handle
deferral is RESOLVED by C1** at the routing layer; the TPI *enforcement*
(segment-crossing → createTPI) remains open at AR3.

---

## Verification / CI gate (each cycle, before close-out)

- `cargo test -p cherchi-rs` (DEFAULT — FFI-free/WASM-clean; arrangement modules
  cfg'd out; prior tests unregressed) **and**
  `cargo test -p cherchi-rs --features indirect-predicates`.
- `cargo test -p indirect-predicates-sidecar-rs` (Cycle A).
- `cargo fmt -p cherchi-rs -- --check`; `cargo clippy -p cherchi-rs --all-targets
  --features indirect-predicates -- -D warnings` (and default).
- No `unsafe` / `panic!` in cherchi-rs production paths. Single-threaded.

## Docs on completion

- `docs/yang_functional_roadmap.md` §M6: PR-CR-AR2b done; AR3 next; N13 TPI +
  `point_in_segment` deviations RESOLVED.
- `docs/yang_deviations.md`: N13 TPI / `f64`-guard sub-notes → resolved.
- `crates/cherchi-rs/LICENSE-THIRD-PARTY.md`: add `add_constraints.rs`.
- Memory: PR-CR-AR2b topic file + MEMORY.md pointer.
