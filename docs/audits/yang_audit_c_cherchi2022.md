# Yang Audit C — Cherchi 2022 paper-vs-port conformance

**Audit date**: 2026-04-30
**Auditor**: auditor-c (`yang-audit-2026-04-30` team)
**Slice**: `crates/kernel/src/boolean/exact_mesh.rs` (`label_cells`, `ray_cast_inside`,
`label_sub_tri_raycast`, `ray_tri_intersect_axis`, `weld_mesh_vertices`,
`build_bvh_for_tris`); cross-check of `topology_extract.rs::flood_fill_patches` for
patch-segmentation conformance to Cherchi 2022 §5.
**Reference**: Cherchi/Pellacini/Attene/Livesu 2022 — *Interactive and Robust Mesh
Booleans* (`/tmp/cherchi2022.txt`, 853 lines).
**Companion**: Yang 2025 §4.4.2 cites Cherchi 2022 explicitly for the in/out
classification step (`docs/references/yang2025_hybrid_boolean.txt`).
**Prior baseline**: `docs/audits/cherchi_port_audit.md` (2026-04-28, 42 findings;
covered Cherchi 2020 mesh-arrangement layer plus Cherchi-2022 boolean-layer
findings D-05..D-13). This auditor focuses on the Cherchi 2022 paper specifically
and re-validates D-05..D-13.

## §0 Verdict (TL;DR)

The Cherchi 2022 ray-cast in/out classification is **partially ported, with the
"Algorithm 1 happy path" present (since commit `3e17f08`) but every
distinguishing 2022 contribution outside the happy path either MISSING or
DEVIATES**:

- §5.1 cascaded `findRayEndpoints` — **MISSING** (centroid-only).
- §5.2 implicit-LPI sort + tight zero-extent ray AABB — **DEVIATES** (single
  f64 `t_hit`; ε-expanded slab).
- §5.3 vertex/edge ambiguity handling + `nextafter` 8-offset cascade — **MISSING**
  (single Degenerate enum + Hoffmann normal-perturb fallback).
- §5 patch-graph manifold-edge barriers — **DEVIATES** (Yang-style
  intersection-edge barriers used in `flood_fill_patches`).
- §5 per-patch labeling ("scales with #patches not #triangles") — **DEVIATES**
  (per-sub-triangle labeling; Cherchi's headline complexity claim is forfeited).
- §4 cached predicates / TBB parallelism / Livesu-2021 earcut **at the
  arrangement layer** — not in this slice (auditor-b/c covered earlier in
  `cherchi_port_audit.md`); `intersection_class.rs::compute_intersections` is
  still O(n²) per A-author's own TODO at line 103-105 (B-01 in prior audit).

The single most important finding: **per-patch labeling is the headline
algorithmic contribution of Cherchi 2022 (Section 5 introduction: "the algorithm
scales with the number of patches in the arrangement and not with the number
of triangles")**. Our current per-sub-tri labeling fires one ray per sub-triangle.
For a typical 200K-triangle scene with ~50 patches this is a 4000× ray-cast
overhead — but, more importantly, this design choice is what would let us
satisfy the `intersection_edges` / `manifold_edges` patch boundaries the paper
relies on for the in/out invariant. We are paying Cherchi's correctness cost
without taking Cherchi's correctness guarantee.

D-05 fix (the highest-priority finding from the prior audit) **landed faithfully**
at commit `3e17f08`. See §3 for re-validation.

## §1 Section-by-section assessment

### §3 — Mesh arrangement (high-level)

Cherchi 2022 §3 specifies the 2-step pipeline: (i) resolve mesh intersections,
(ii) classify patches as inside/outside. Our pipeline executes the same shape
in `exact_mesh.rs::yang_boolean_pipeline` — `subdivide_mesh_pair` (intersection
resolve) → `label_cells` (classify). **Status: PRESENT (architecturally)**.

Cherchi 2022 §3 also requires that the input is "manifold, watertight and with
no self-intersections" (line 236-237). Yang's pipeline cannot guarantee this
post-tessellation (per memory `[Yang Implementation Status]`); see Y-01 in §2.

### §4 — Mesh arrangement improvements (Cherchi-2022-vs-Cherchi-2020 deltas)

Cherchi 2022 §4 catalogs four improvements over Cherchi 2020:

#### §4 Cached Predicates (paper p.5, lines 321-327)

> "We rewrite the 4×4 determinant above as ... obtaining a perfect separation
> between plane coefficients and point coordinates. We exploit this latter
> equation to cache, for each input triangle, the four 3×3 determinants, thus
> reducing each call to orient3D to a simple scalar product in 4D."

**Status: MISSING**. Each `geometry_predicates::orient3d` / `robust::orient3d`
call recomputes the 4×4 determinant from scratch. No per-triangle plane-coeff
cache exists. Code site: `exact_mesh.rs:1078, 1278, 1424` (orient2d/3d call
sites in ray_cast hot loop).

Severity: PERFORMANCE-DRIFT (does not affect correctness; Cherchi 2022's 5×
arrangement speedup is forfeit). Reference: `intersection_class.rs:103-105`
already TODOs the broader octree+TBB upgrade (B-01 in prior audit).

#### §4 Segment Insertion via Livesu et al. 2021 (paper p.5-6, lines 332-336)

> "We substituted earcut with a method recently introduced in [Livesu et al.
> 2021], which ensures optimal deterministic O(n) complexity in all cases
> and is two orders of magnitude faster than the previously best performing
> existing method [Shewchuk and Brown 2015]."

**Status: PRESENT** at `cherchi/triangulation.rs::earcut_linear` (lines
1100-1197 per prior audit). Faithful Rust port using `orient2d_indirect`.
This is a Cherchi-2022 (not Cherchi-2020) addition.

#### §4 Low-level Implementation: TBB parallelism, swiss tables, arena allocators (paper p.6, lines 339-383)

**Status: MISSING / DEVIATES**.
- TBB → no rayon parallelism in `cherchi/processing.rs`,
  `intersection_class.rs`, or `exact_mesh.rs` (verified via grep).
- swiss tables → `std::collections::HashMap` (uses Rust's default RandomState)
  is used; not equivalent to Abseil/swisstable (allocates per insert; no
  small-array optimization for adjacency).
- arena allocators → not used.

Severity: PERFORMANCE-DRIFT.

### §5 — Inside/outside classification (THE Cherchi 2022 chapter)

This section is the heart of Cherchi 2022's contribution and the section Yang
2025 §4.4.2 cites by name. Algorithm 1 (paper p.6) breaks into three subsections.

#### §5.1 Ray definition — `findRayEndpoints` cascade (paper p.7, lines 420-461)

> "we define the ray r by first attempting to find a satisfactory approximate
> floating point solution, while we resort to guaranteed exact rational numbers
> only as backup strategy."

C++ `booleans.cpp:475-546` `findRayEndpoints` cascade:
1. Scan all patch triangles for a vertex with `isExplicit3D() && vertInfo == 0`
   (an interior, non-border, ground-truth-coordinate vertex). If found, set
   `ray.v0 = v`, `ray.v1 = (max_x, v.Y(), v.Z())`. **Done.**
2. Else, for each patch triangle: snap-round `getApproxXYZCoordinates` to
   floats; pick `dir = maxComponentInTriangleNormal` (the dominant axis of the
   triangle's normal — this minimizes grazing); push ray-origin BACKWARD by
   `0.1` along `dir` so it lies BENEATH the patch; verify
   `orient3D(t0, t1, t2, ray.v0)` and `orient3D(t0, t1, t2, ray.v1)` have
   opposite signs (ray crosses triangle plane); verify
   `checkIntersectionInsideTriangle3DImplPoints` (ray actually pierces inside
   the triangle, not outside its edges). Save ray + `tv[3] = patch_tri_verts`
   for later first-hit pivot.
3. Else, fall to exact rationals (C++ exits with WARNING — left for future).

**Status: MISSING**. `label_sub_tri_raycast:1538` always uses
`sub_tri_centroid(verts, sub_tri)` as the ray origin AND passes it directly to
`ray_cast_inside`. Differences:

(a) **No backward push by 0.1**. Centroid-of-sub-triangle is *coplanar* with the
    sub-triangle's plane; +X ray from the centroid risks grazing the
    sub-triangle's own plane when the patch is axis-aligned. (See D-06 in prior
    audit.)
(b) **No vertex-preference scan**. Cherchi prefers an interior, explicit-coordinate
    vertex (vertInfo == 0 means non-border per `computeSinglePatch:428-429`).
    Such a vertex has known ground-truth coordinates → no snap-rounding error
    → the simple "ray hits exactly one explicit point" path takes effect. Our
    centroid is always synthesized via `(v0 + v1 + v2) / 3.0` with
    floating-point round-off.
(c) **No `tv[3]` first-hit pivot**. `sortIntersectedTrisAlongX/Y/Z`
    (`booleans.cpp:1142-1163`) uses `tv[3]` (ray's source patch triangle) to
    discard intersections "before" the ray's starting patch via three orient3d
    sign checks. Without `tv[3]`, our `t > 0.0` pivot is approximate and can
    miss-classify hits at the boundary.
(d) **No `vertInfo` (border vertex marker)**. `computeSinglePatch:428-429`
    sets `vertInfo = 1` on every patch-border vertex (vertex on a non-manifold
    edge). `findRayEndpoints:483-485` requires `vertInfo == 0` (interior only)
    — picking a border vertex is unsafe because the ray from a border vertex
    is tangent to the adjacent patch. We have NO `vertInfo` analog.
(e) **No `dir = maxComponentInTriangleNormal`**. Our `ray_cast_inside` cycles
    `axis = 0..3` (X, Y, Z) and uses the FIRST non-degenerate axis. Cherchi
    picks the axis with maximum normal projection (paper §4.5.5: same predicate
    used for projecting triangle to 2D). For a near-axis-aligned face the
    Cherchi pick is dramatically better.

Severity: CORRECTNESS-BUG (probabilistic, increases with axis-aligned scenes).
Cross-ref to prior audit: D-06 and D-12 capture parts (a) and (d); (b), (c),
(e) are new findings YC-01..YC-03 below.

#### §5.2 Intersection detection (paper p.7-8, lines 422-461)

> "we use a plain octree as acceleration structure and perform ray casting by
> testing intersections between the ray bounding box and each octant. Note
> that, for efficiency, this is the same acceleration structure used in the
> arrangement part to detect triangle intersections. Since both the octree
> and the ray are axis aligned we have two nice properties: the bounding box
> of the ray is tight (it's the ray itself), and the intersection with the
> octant reduces to a 2D check which involves only four comparisons between
> floats."

> "we represent intersections implicitly, using the LPI (Line-Plane Intersection)
> points described in [Attene 2020]. Then, we use the exact comparator introduced
> in [Cherchi et al. 2020] to sort them from the closest to the ray emanating
> point to the furthest."

**Status: DEVIATES**.

C++ uses:
- Octree (shared with arrangement step) — `cinolib::Octree`.
- Tight zero-extent ray AABB: `cinolib::AABB(ray.v0, ray.v1)` (no slab eps).
  4-comparison 2D check is implicit in axis-aligned-octant box-vs-box test.
- Implicit LPI representation: each intersection stored as
  `implicitPoint3D_LPI(ray.v0, ray.v1, tv0, tv1, tv2)`. Sorted via
  `phmap::btree_set<..., less_than_GP_on_X>` using the
  `genericPoint::lessThanOnX` exact comparator.

Rust uses:
- BVH (NOT shared with arrangement; `intersection_class.rs:103-105` still O(n²)).
- Slab AABB ε-expanded by `TAU_EXACT_MESH_SLAB_EPS = 1e-14` along each
  perpendicular axis (`exact_mesh.rs:1356-1360`). Not zero-extent.
- Single f64 `t_hit` computed from inexact barycentric weights
  (`exact_mesh.rs:1302`):
  `t_hit = (o1*v0[axis] + o2*v1[axis] + o0*v2[axis]) / area_total - origin[axis]`.
  Sorted via `f64 < f64` (NOT exact).

The implicit-LPI-vs-f64 distinction matters: when two triangles' intersection
points along the ray differ by less than 1 ULP, the f64 sort is undefined-order,
while the LPI exact sort is deterministic. This affects which triangle is
selected as "first hit" and therefore the orient3d signed-volume classification.

Severity: CORRECTNESS-BUG (precision-sensitive cases). Cross-ref: D-09 in prior
audit captured the slab-eps half; the f64 t-sort vs LPI sort is YC-04 below.

#### §5.3 Classification — first-hit signed-volume + ambiguity cascade (paper p.7-8, lines 467-511)

> "When the ray crosses the surface at a point that is inside a triangle (left)
> the test depicted in Figure 5 can reliably determine the inside/outside
> relation between the patch being tested and the surface crossed."

> "the cases when the ray intersects a mesh edge or a vertex are more difficult
> ... we perturb the coordinates of p∞ by ε (without moving its starting point)
> until the crossing happens at a point that is interior to a mesh triangle.
> Perturbation of point coordinates is performed using the next floating-point
> number representable starting from a given number (using std::nextafter)."

The interior-of-triangle case (Figure 6 left) is **the easy case**:
`checkTriangleOrientation(ray, tv0, tv1, tv2) := orient3d(tv0, tv1, tv2, ray.v1)`,
return `(res < 0) ? Inside : Outside` per paper Figure 5 + booleans.cpp:1290-1300.

Ambiguity cases (Figure 6 middle/right): ray hits a vertex (`INT_IN_V0/V1/V2`),
ray hits an edge (`INT_IN_EDGE01/EDGE12/EDGE20`), ray coplanar with a triangle
(`DISCARD`). C++ dispatches via `fast2DCheckIntersectionOnRay` → `IntersInfo`
enum (`booleans.cpp:1024-1081`), then:

- Vertex hit: `findVertRingTris(v, ref_label, tmp_inters, ...)` → 1-ring on the
  same surface label; perturb-cascade `perturbXRay/Y/Z(ray, i)` for `i = 0..7`
  (8 `std::nextafter` directional offsets) until a triangle in the 1-ring is
  hit interior. Sort-along-axis the perturbed-ray hits, return first.
- Edge hit: `findEdgeTris(ev0, ev1, ref_label, ...)` → 2-tri ring (the adjacent
  pair across the edge). Same perturb-cascade. C++ asserts
  `edge_tris.size() == 2` (line 775).
- Coplanar (`DISCARD`): skip.

**Status: PARTIAL — happy path PRESENT (since `3e17f08`); ambiguity cascade MISSING.**

Rust `ray_cast_inside`:
- Happy path: best_hit = arg-min `t > 0`, then
  `robust::orient3d(tv0, tv1, tv2, ray_v1)` < 0 → Inside.
  **CONFORMANT**, see §3 for D-05 re-validation.
- Ambiguity: collapsed into a single `RayHit::Degenerate` variant
  (`exact_mesh.rs:1283-1284` — fires on ANY orient2d == 0). Two responses:
  (i) the per-axis loop tries the next axis (`continue` at line 1406);
  (ii) on all-axes-degenerate, `label_sub_tri_raycast` falls through to
  Hoffmann perturb-and-classify (`exact_mesh.rs:1610-1672`) which samples
  `±eps * normal` along the sub-tri's OWN normal — a different geometric
  point set than C++'s ray-endpoint perturbation.

Differences from Cherchi 2022 §5.3 ambiguity handling:

(a) **No `IntersInfo` enum**. We do not classify "what KIND of ambiguity"
    (V0/V1/V2/EDGE01/EDGE12/EDGE20 / DISCARD). Without this we cannot:
    - Find the 1-ring (vertex case) or 2-tri ring (edge case).
    - Apply per-case perturbation (`perturbXRay` is X-axis-specific, etc.).
    - Discard truly coplanar cases vs perturbing them.
(b) **Wrong perturbation geometry**. Cherchi perturbs the ray ENDPOINT (`ray.v1`)
    in axis-aligned `nextafter` directions, keeping the start point pinned
    (paper §5.3 line 488: "without moving its starting point"). We perturb
    the SAMPLING POINT along the sub-tri's own normal (which is generally
    NOT axis-aligned). For sub-triangles whose normal is not aligned with any
    global axis, the two methods sample different geometric points.
(c) **Wrong triangle test set**. Cherchi only tests the 1-ring or 2-tri ring;
    we re-test the entire BVH candidate set (with the perturbed sample point
    instead of the perturbed ray). The BVH candidate set typically contains
    hundreds of triangles; the C++ ring contains 2-12.
(d) **No perturbation cascade**. Cherchi cycles 8 offsets (`for i = 0..7`)
    until success. We sample twice (`+eps * normal`, `-eps * normal`) and
    fall to GWN if both degenerate.
(e) **Convention divergence**. Cherchi: when ray ambiguous, perturb until
    interior-hit, then apply `(orient3d < 0) ? Inside : Outside`. Hoffmann:
    when degenerate, sample both sides; if classifications differ, classify
    boundary-coincident as Inside per closed-solid convention; if same,
    use the agreed result. The two conventions agree at the limit but
    not for finite ε on boundary-tangent sub-triangles.

Severity: CORRECTNESS-BUG (under axis-aligned + boundary-coincident inputs,
which dominate CAD workloads). Cross-ref: D-07 in prior audit; (a)-(d) above
expand D-07 with the missing-1-ring / missing-2-tri-ring detail.

#### §5 Patch construction + per-patch labeling (paper p.6, line 386-388, Algorithm 1)

> "we consider the arrangement computed at the previous step and determine,
> for each of its manifold surface patches, the relative position with respect
> to the input meshes M1, ..., Mn."

> "An important aspect of such an approach is that the algorithm scales with
> the number of patches in the arrangement and not with the number of triangles
> in the mesh."

C++ `computeSinglePatch:397-433` defines a patch via flood-fill where
`edgeIsManifold(e_id)` is the barrier: the flood traverses an edge iff
`adjE2T[e].size() == 2` (manifold). Border vertices are tagged via `vertInfo = 1`
when crossing a non-manifold edge.

C++ `computeInsideOut:592-621` runs ONE ray per patch: `findRayEndpoints(tm,
patch_tris, max_coords, ray)` then `pruneIntersectionsAndSortAlongRay(...,
sorted_inters)` then `analyzeSortedIntersections(ray, ..., sorted_inters,
patch_inner_label)` then `propagateInnerLabelsOnPatch(patch_tris,
patch_inner_label, labels)`. Every triangle in the patch gets the SAME label
from the same ray.

**Status: DEVIATES (architecturally)**. Rust labels per-sub-triangle
(`exact_mesh.rs:1818-1876`), with one ray per sub-triangle. There is no
per-patch grouping at the labeling stage. `flood_fill_patches` happens in a
LATER stage (`topology_extract.rs::flood_fill_patches`, called after
`label_cells`) and uses Yang-style intersection-edge barriers, NOT
Cherchi-style manifold-edge barriers (D-11 in prior audit).

Consequences:

(a) **Headline complexity claim forfeit**: paper claims `O(#patches × cost(ray))`
    where #patches « #triangles. We pay `O(#sub-triangles × cost(ray))`. For
    Cherchi 2022's Thingi10K benchmark (3.8K booleans, 80K rays = ~21 rays per
    boolean = ~21 patches per boolean) vs naive per-tri (would be ~10⁵ rays =
    a 5000× overhead in their setting).
(b) **Per-patch invariant unused**: Cherchi's correctness argument
    (paper §5.3) ONLY holds because all triangles in a patch share a label.
    Per-sub-tri labeling, by contrast, can produce neighboring sub-tris with
    different labels even though geometrically they should agree — under
    cherchi the patch's own boundary-discontinuity invariant prevents that.
(c) **Border vertex tracking missing**: Cherchi's `findRayEndpoints` cascade
    relies on `vertInfo == 0` (border vertices excluded as ray emanating
    points). Without per-patch flood-fill at the labeling stage, we have
    NO concept of "border vertex" to exclude. Centroid is always used.

Severity: DELIBERATE-DIVERGENCE (architectural; the audit cannot tell whether
the deviation breaks specific assay cases without the test infrastructure to
A/B). Cross-ref: D-11/D-12 in prior audit; YC-05/YC-06 below for new detail.

### Algorithm 1 (paper p.6) — full implementation status

```
ALGORITHM 1: Inside/outside classification
Input:  patches P_1, ..., P_m
Output: relative position of each patch w.r.t. M_1, ..., M_n

for each patch P:
    Initialize P as outside all M_i;                 # ⊳ DEVIATES (per-sub-tri init)
    Define ray r from p ∈ P to p∞;                  # ⊳ MISSING (centroid only, no cascade)
    for each input mesh M:
        compute and sort intersections r ∩ M;       # ⊳ DEVIATES (f64 t_hit, not LPI sort)
        if r ∩ M is non-empty:
            find first intersected triangle t ∈ M;  # ⊳ PRESENT (since 3e17f08)
            compute volume of (t, p∞);              # ⊳ PRESENT (orient3d)
            if volume < 0: P inside M;              # ⊳ PRESENT
        end
    end
end
```

Two of the seven sub-steps are PRESENT. Three DEVIATE. Two MISSING.

## §2 Findings

Findings labeled `YC-NN`. Severity rubric matches prior audit:
CORRECTNESS-BUG (silent data loss / wrong classification), MISSING (paper
specifies; not implemented), STUB (skeleton present, key logic absent),
DEVIATES (implemented but differs from paper), PERFORMANCE-DRIFT (correct but
slower than spec).

### YC-01 — `findRayEndpoints` cascade fully MISSING

**Severity**: MISSING (CORRECTNESS-BUG implications).
**Cross-ref**: D-06, D-12 (prior audit, both UNKNOWN-NEEDS-INVESTIGATION).
**Code**: `exact_mesh.rs:1538` — single line: `let centroid = sub_tri_centroid(verts, sub_tri);`
**Paper**: Cherchi 2022 §5.1 lines 420-461; C++ `booleans.cpp:475-546`.

Cherchi 2022 §5.1 specifies a 3-tier cascade:
1. Prefer an interior explicit-coordinate vertex (`vertInfo == 0`).
2. Fall back to triangle-centroid pushed back along
   `maxComponentInTriangleNormal` axis by 0.1.
3. Fall back to exact rationals (paper "backup strategy").

We implement none of the three tiers. Centroid is always used and there is no
backward push.

**Severity test (concrete)**: Sub-triangle on a unit-cube z=1 face (axis-aligned),
centroid `(1/3, 1/3, 1)`. Cherchi's cascade picks any of the patch vertices
(e.g. `(0, 0, 1)`), pushes back to `(0, 0, 0.9)`, shoots `+Z` ray. Our centroid
ray starts AT the face plane, so `+X` ray grazes the patch's own plane (Y/Z
projection), `ray_tri_intersect_axis` returns `Degenerate` for any sub-tri
sharing that face. We fall through to Hoffmann ±-normal which samples
`(1/3, 1/3, 1+1e-6)` and `(1/3, 1/3, 1-1e-6)` along normal=Z — DIFFERENT
geometric points than Cherchi's perturbed ray would test against the OTHER
operand's surface.

**Suggested fix direction**: Implement Cherchi `findRayEndpoints`-equivalent
that (a) tracks border vertices via per-patch flood-fill (depends on YC-05),
(b) prefers interior input vertices, (c) pushes ray-origin backward by 0.1
along `maxComponentInTriangleNormal`, (d) pivots first-hit via `tv[3]`
patch-triangle.

### YC-02 — Backward push of 0.1 along chosen axis MISSING

**Severity**: CORRECTNESS-BUG (subset of YC-01; called out separately because
it is a one-line fix).
**Cross-ref**: D-06 (prior audit, partial).
**Code**: `exact_mesh.rs:1353` — `slab_min[axis] = p[axis]` (no backward push).
**Paper**: Cherchi 2022 §5.1 line 454 / `booleans.cpp:511, 517, 523`.

C++ pushes the ray ORIGIN backward by 0.1 along the chosen axis to ensure
ray-origin lies BENEATH the patch (so the ray traverses the patch from below).
We start the ray AT the centroid, which is coplanar with the patch's plane,
making axis-aligned grazing the common case for axis-aligned CAD inputs.

**Severity test**: As YC-01 above. The backward push alone (without YC-01's
vertex preference) would convert most axis-aligned grazing cases into clean
crossings — but it is not implemented.

**Suggested fix direction**: After `let mut p = sub_tri_centroid(...);`, push
back: `p[axis] -= 0.1` (per-axis loop body, before slab construction).

### YC-03 — Axis selection cycles 0..2 instead of `maxComponentInTriangleNormal`

**Severity**: PERFORMANCE-DRIFT (correctness risk only at exhaustion of cascade).
**Cross-ref**: A-02 (prior audit, on the arrangement-side
`max_component_in_triangle_normal` use).
**Code**: `exact_mesh.rs:1344` — `for axis in 0..3 { ... }` (X first, then Y, Z).
**Paper**: Cherchi 2022 §5.1 line 451 + §4 line 451 / `booleans.cpp:508`
`int dir = genericPoint::maxComponentInTriangleNormal(...)`.

Cherchi picks the axis that maximizes the normal projection (= most
perpendicular to the patch). This is the axis MOST LIKELY to produce a clean
interior crossing (a +X ray hitting an X-perpendicular face is a clean hit;
a +X ray hitting an X-tangent face grazes). We always try X first.

**Severity test**: Sub-triangle in YZ plane (normal = X). With Cherchi's pick,
any orthogonal axis (Y or Z) produces a clean crossing. Our X-first pick gets
`Degenerate` (the YZ-plane sub-tri's projection to YZ is the triangle itself
— orient2d == 0 on every edge). We `continue` to Y, which works. Net: extra
iteration. For sub-triangles approximately YZ-plane, Y first works. The cost
is throughput, not correctness — UNLESS all three axes degenerate-then-Hoffmann.

### YC-04 — f64 `t_hit` instead of implicit LPI sort

**Severity**: CORRECTNESS-BUG (1-ULP race conditions on co-planar hits).
**Cross-ref**: NEW (not in prior audit).
**Code**: `exact_mesh.rs:1302` — `let t_hit = (o1*v0[axis] + o2*v1[axis] + o0*v2[axis]) / area_total - origin[axis];`
**Paper**: Cherchi 2022 §5.2 lines 449-453.

Cherchi 2022 §5.2 explicitly:

> "we represent intersections implicitly, using the LPI (Line-Plane
> Intersection) points described in [Attene 2020]. Then, we use the exact
> comparator introduced in [Cherchi et al. 2020] to sort them from the closest
> to the ray emanating point to the furthest."

We compute a single f64 `t_hit` from inexact barycentric weights (the
quotient `(o1*v0[axis] + o2*v1[axis] + o0*v2[axis]) / area_total` involves
both round-off in the sum and round-off in the division). C++ stores
intersections as `implicitPoint3D_LPI(ray.v0, ray.v1, tv0, tv1, tv2)` and
sorts via `phmap::btree_set<..., less_than_GP_on_X>` using
`genericPoint::lessThanOnX` exact comparator. The two methods agree
asymptotically but **not** for hits within 1 ULP of each other.

**Severity test (concrete)**: Two triangles in the OTHER mesh, both intersecting
the ray at exact distance `t = d`. (Possible when the two triangles share
an edge perpendicular to the ray.) f64 t_hit comparison: undefined order, may
pick either. Cherchi LPI-sort: consistent but the tie itself is ambiguous —
the algorithm continues to ambiguity dispatch (`INT_IN_EDGE`). We just pick
arbitrarily.

**Suggested fix direction**: Either port `implicitPoint3D_LPI` to Rust (large
effort), or use the existing `IndirectPoint::LPI` arena type from
`indirect_predicates.rs` (already used in `cherchi/triangulation.rs`).

### YC-05 — Per-patch labeling DEVIATES (per-sub-tri labeling instead)

**Severity**: DELIBERATE-DIVERGENCE (architectural).
**Cross-ref**: D-11, D-12, D-13 (prior audit, all UNKNOWN-NEEDS-INVESTIGATION).
**Code**: `exact_mesh.rs:1818-1876` (label_cells per-sub-tri loop).
**Paper**: Cherchi 2022 §5 line 388 + Algorithm 1 line 339-348 + paper §5.3 line 467-476.

Cherchi's central architectural decision is:
1. Compute patches first (`computeAllPatches`).
2. Run ONE ray per patch, propagate label to all sub-tris in the patch
   (`computeInsideOut` + `propagateInnerLabelsOnPatch`).

We:
1. Run one ray per sub-triangle (`label_sub_tri_raycast` for each).
2. Compute patches LATER for B-Rep assembly (`flood_fill_patches`).

Forfeit:
- Headline complexity (paper §5 line 388: "scales with #patches not #triangles").
- The patch-label invariant that ALL TRIS IN A PATCH HAVE THE SAME LABEL is
  not enforced — it is merely hoped for. If two adjacent sub-tris of the
  same patch get different rays and one ray finds an ambiguity, the per-tri
  labels can diverge. Cherchi's label propagation prevents this by construction.

**Severity test**: Patch P (one face of the OTHER operand intersected by
mesh A) with two adjacent sub-triangles s1, s2. Centroid of s1 produces a
clean ray-cast → Outside. Centroid of s2 has a degenerate ray-cast (Hoffmann
fallback on s2's normal which may sample inside vs outside differently because
s1, s2 might have slightly different normals due to sub-triangulation). Result:
s1 = Outside, s2 = Inside → patch P is internally split — a topologically
forbidden configuration that Cherchi's per-patch labeling makes impossible.

**Suggested fix direction**: Compute patches BEFORE labeling. For each patch,
pick one ray-emanating point from the patch's interior, cast once, propagate
to all member sub-tris. Move `flood_fill_patches` (or its core flood logic)
ahead of `label_cells` and reorganize `select_boolean_result` to consume
patch-level labels.

### YC-06 — `flood_fill_patches` uses intersection-edge (Yang) barriers, not manifold-edge (Cherchi) barriers

**Severity**: DELIBERATE-DIVERGENCE.
**Cross-ref**: D-11 (prior audit, UNKNOWN-NEEDS-INVESTIGATION); the
Yang-vs-Cherchi distinction was already flagged.
**Code**: `topology_extract.rs:493-514`. Specifically lines 501-505: flood-fill
stops at `intersection_edges.contains(&(v0, v1))` (a cross-mesh barrier).
**Paper**: Cherchi 2022 §5 line 386 + `booleans.cpp:412-431` (`computeSinglePatch`).

Cherchi's barrier is `tm.edgeIsManifold(e_id)` (line 414): flood traverses iff
the edge is manifold. A non-manifold edge means three or more triangles meet
there — the patch boundary. Yang's barrier is "cross-mesh edge": flood
traverses iff the reverse edge is in the same mesh.

The two definitions overlap on simple inputs (Yang's intersection edges become
non-manifold after arrangement) but diverge on:
- Self-intersecting meshes (per memory, 19/25 F-series have hidden
  self-intersections per the oracle). Yang's def doesn't catch self-cross
  non-manifolds; Cherchi's does.
- Coplanar overlap regions where overlap-boundary edges are non-manifold but
  same-mesh.

Severity: DELIBERATE-DIVERGENCE. Yang's flood is correct under Yang's
preprocessing assumptions. Cherchi's would be more conservative.

### YC-07 — `weld_mesh_vertices` quantizes at nanometer scale (A15.6 violation)

**Severity**: CORRECTNESS-BUG (per A15.6).
**Cross-ref**: D-10 (prior audit, CORRECTNESS-BUG).
**Code**: `exact_mesh.rs:1735-1766`, called twice from `label_cells:1804-1805`.
**Paper**: Cherchi 2022 §3 line 236-237 (input precondition: "manifold,
watertight and with no self-intersections").

`weld_mesh_vertices` quantizes positions at `QUANT_NANOMETER_SCALE = 1e9` and
collapses coincident vertices. This is a tolerance-escalation fix for the
fact that WaffleKernel tessellation produces per-face (non-shared) vertices.
Per `governance/ARCHITECTURAL_INVARIANTS.md §A15.6` and memory
`[Yang Coplanar Preprocessing Lesson]`, this is the EXACT anti-pattern the
hybrid Yang pipeline forbids.

The Cherchi paper assumes input is already watertight; we are using the
tolerance-escalation as an upstream-tessellation crutch.

**Severity test (concrete)**: Two cubes at distance `5e-10` (sub-nanometer):
weld collapses the gap, two cubes become one solid → wrong topology.

**Suggested fix direction**: Per A15.6: fix tessellation to produce shared
vertices at face boundaries (bijective tessellation per Yang §4.1.1); remove
`weld_mesh_vertices` entirely.

### YC-08 — No `vertInfo` (border vertex marker)

**Severity**: MISSING (cascading consequence).
**Cross-ref**: D-12 (prior audit).
**Code**: no analog in Rust.
**Paper**: Cherchi 2022 §5 + `booleans.cpp:428-429, 466-467`.

C++ `computeSinglePatch` sets `vertInfo = 1` for every vertex on a non-manifold
edge encountered during flood-fill. `findRayEndpoints` then prefers vertices
with `vertInfo == 0` (interior, non-border) as ray emanating points.

Rust has no `vertInfo`. Without it, even if YC-01's findRayEndpoints cascade
were ported, we couldn't filter border vertices.

### YC-09 — No `IntersInfo` enum / vertex-edge ambiguity dispatch

**Severity**: MISSING (CORRECTNESS-BUG implications, see YC-10 below).
**Cross-ref**: D-07 (prior audit).
**Code**: `exact_mesh.rs:1283-1284` — single `RayHit::Degenerate` variant.
**Paper**: Cherchi 2022 §5.3 + `booleans.cpp:1024-1081`
(`fast2DCheckIntersectionOnRay`).

C++ classifies the ambiguity kind into 7 cases: `INT_IN_TRI` (clean interior),
`INT_IN_V0/V1/V2` (vertex hits), `INT_IN_EDGE01/EDGE12/EDGE20` (edge hits),
`DISCARD` (coplanar). Each gets different downstream dispatch:
- `INT_IN_TRI` → use this hit.
- `INT_IN_V*` → find 1-ring of that vertex, perturb-cascade.
- `INT_IN_EDGE*` → find 2-tri ring of that edge, perturb-cascade.
- `DISCARD` → skip.

We collapse all ambiguity into `Degenerate`; downstream we either advance to
the next axis (`continue`) or fall to Hoffmann normal-perturbation.

### YC-10 — Wrong perturbation geometry (sub-tri normal vs ray endpoint nextafter)

**Severity**: CORRECTNESS-BUG.
**Cross-ref**: D-07 (prior audit, CORRECTNESS-BUG).
**Code**: `exact_mesh.rs:1610-1672` (Hoffmann fallback).
**Paper**: Cherchi 2022 §5.3 + `booleans.cpp:780-985` (`perturbXRay`,
`perturbYRay`, `perturbZRay`).

C++ perturbs `ray.v1` (the ray endpoint, NOT the sample point) by
`std::nextafter` along axis-aligned directions, keeping `ray.v0` pinned (paper
§5.3 line 488: "without moving its starting point"). 8 directional offsets
per axis: `+u`, `+u+v`, `+v`, `-u+v`, `-u`, `-u-v`, `-v`, `+u-v` where `(u, v)`
are the two axes perpendicular to the ray direction.

Rust perturbs the SAMPLE POINT along the sub-triangle's OWN normal
(not axis-aligned in general). This samples DIFFERENT geometric points than
C++'s perturbed-ray would intersect.

For sub-triangles whose normal is approximately axis-aligned, the two methods
agree to first order. For sub-triangles at 45° to all three axes (common in
CAD with cylinder/cone tessellations), the two methods sample different
locations.

**Severity test**: Sub-triangle with vertices `(1, 0, 0)`, `(0, 1, 0)`,
`(0, 0, 1)` (an octant face); centroid `(1/3, 1/3, 1/3)`; normal
`(1/√3, 1/√3, 1/√3)`. C++ `perturbXRay` shifts `ray.v1.Y` and `.Z` by
`nextafter` (~ 1e-16 magnitude in CAD scale). Our sample-point shift is
`±1e-6 * (1/√3, 1/√3, 1/√3)` = ~1e-6 magnitude in each coordinate. Different
points by 10 orders of magnitude in displacement, on different geometric
trajectories.

### YC-11 — `slab_eps = 1e-14` slab expansion (D-09 not yet fixed)

**Severity**: PERFORMANCE-DRIFT (correctness risk if `slab_eps` is mis-tuned).
**Cross-ref**: D-09 (prior audit, PERFORMANCE-DRIFT).
**Code**: `exact_mesh.rs:1330` — `let slab_eps = crate::units::TAU_EXACT_MESH_SLAB_EPS;`
defined at `units.rs:259` as `1e-14`.
**Paper**: Cherchi 2022 §5.2 line 437-438 ("the bounding box of the ray is
tight (it's the ray itself)").

C++ uses `cinolib::AABB(ray.v0, ray.v1)` — zero-extent in the perpendicular
axes. The 4-comparison 2D check the paper describes assumes this tight slab.

Our `1e-14` perpendicular expansion is a defensive padding for the
parity-counting era's welded mesh boundaries. Per D-05's first-hit fix it is
no longer load-bearing for correctness, but it survives. The expanded slab
returns more candidates than necessary, slowing down the BVH query.

### YC-12 — No octree shared between arrangement and ray-cast

**Severity**: PERFORMANCE-DRIFT.
**Cross-ref**: B-01 (prior audit, PERFORMANCE-DRIFT — "O(n²) broad-phase").
**Code**: `exact_mesh.rs:1809-1810` builds a fresh BVH per call;
`intersection_class.rs::compute_intersections` uses no acceleration structure.
**Paper**: Cherchi 2022 §5.2 line 433-436 ("for efficiency, this is the same
acceleration structure used in the arrangement part to detect triangle
intersections").

Cherchi specifically calls out that the SAME octree is reused across both
phases. We build a new BVH for ray-cast and use no acceleration in
arrangement detection.

### YC-13 — Cached predicates not implemented

**Severity**: PERFORMANCE-DRIFT.
**Cross-ref**: NEW (not in prior audit; orthogonal to A-01/A-02 which are
about exact-arithmetic correctness, not caching).
**Code**: every `orient3d` site rebuilds the 4×4 determinant.
**Paper**: Cherchi 2022 §4 lines 321-327 (paper p.5).

The "perfect separation between plane coefficients and point coordinates"
caching scheme is not implemented. This was a primary 2022 contribution
(claimed 5× arrangement speedup). Implementing it requires a
`per-triangle: [3x3 det × 4]` cache keyed by triangle ID.

### YC-14 — `analyzeSortedIntersections` first-of-each-label invariant absent

**Severity**: NOT APPLICABLE (we're binary, paper's structure is N-ary).
**Cross-ref**: D-08 in prior audit (DELIBERATE-DIVERGENCE).
**Code**: `exact_mesh.rs:1788-1879` (label_cells: per-A-tri-vs-B, then
per-B-tri-vs-A separately).
**Paper**: Cherchi 2022 §5.3 + `booleans.cpp:718-738`
(`analyzeSortedIntersections`).

C++ supports `n` input meshes simultaneously; the
`analyzeSortedIntersections` loop iterates the sorted intersections and for
each `t_label` (input mesh ID) only the FIRST hit on that label gets
`patch_inner_label[t_label] = true` (the rest are subsequent crossings, which
in a manifold input flip orientation). That structure does not generalize
gracefully to our pairwise A-vs-B / B-vs-A formulation.

For binary booleans, our pairwise structure is correct. For variadic Booleans
(Yang §5+ doesn't yet support them), we'd need to refactor.

### YC-15 — No `tv[3]` first-hit pivot (sortIntersectedTrisAlong*'s discard pass)

**Severity**: CORRECTNESS-BUG (manifests when ray emanates from inside a patch).
**Cross-ref**: NEW (subset of YC-01 broken out for clarity).
**Code**: `exact_mesh.rs:1374-1395` (best_hit accumulator).
**Paper**: Cherchi 2022 §5.2 + `booleans.cpp:1142-1163`.

C++ `sortIntersectedTrisAlongX:1142-1158` discards intersections "before"
the ray's source patch via three orient3d sign checks against `tv[3]`
(the source patch triangle). This is essential because the source patch's
own intersection with the ray must NOT count as "first hit on the OTHER
mesh".

We side-step this by labeling per-sub-triangle (each sub-tri is labeled
against the OTHER mesh, never against its own mesh). The architectural
deviation YC-05 (per-sub-tri labeling) accidentally works around the missing
`tv[3]` pivot. If we ever fix YC-05 (per-patch labeling), we would need to
add the `tv[3]` pivot.

### YC-16 — `checkTriangleOrientation` matches paper / D-05 STILL FAITHFUL

**Severity**: NONE (re-validation of D-05 fix).
**Cross-ref**: D-05 (prior audit, CORRECTNESS-BUG, fix landed at `3e17f08`).
**Code**: `exact_mesh.rs:1414-1449` (best_hit + orient3d).
**Paper**: Cherchi 2022 §5.3 + `booleans.cpp:1290-1300`.

D-05 fix applies `robust::orient3d(tv0, tv1, tv2, ray_v1)` to the first-hit
triangle and returns `Inside iff res < 0.0`. This matches paper Figure 5 and
the C++ comment at `booleans.cpp:1296-1298`:

> /* in res we have sign(area(v0, v1, v2, ray.second))
>  * if the area is >0 the ray is doing INSIDE -> OUTSIDE, so the patch is INSIDE
>  * else the ray is doing OUTSIDE -> INSIDE so the patch is OUTSIDE */

Note: C++ asserts `res != 0` (line 1294); we have no such assert. This is a
defensive divergence, not a correctness issue: in our code, `t_hit > 0.0`
elsewhere implies the ray is not coplanar with the triangle (modulo
`ray_tri_intersect_axis` having returned `Hit` from a non-zero orient2d,
which forces interior crossing). Adding `debug_assert_ne!(res, 0.0)` would
match C++'s defensive pattern.

See §3 for full re-validation.

### YC-17 — Test `label_cells_raycast_matches_gwn_for_offset_boxes` no longer pins parity

**Severity**: NONE (cleanup observation).
**Cross-ref**: D-05 prior-audit "Test conflict" note.
**Code**: `exact_mesh.rs:6403-6444`.

The prior audit warned this test "PINS the parity-counting behavior as an
invariant. The fix-PR must update or delete that test." Re-reading the test
post-3e17f08: the test now asserts `inside_frac > 0.15 && inside_frac < 0.85`
(roughly half the sub-tris should be Inside). It does NOT pin parity. The
test was updated as required.

### YC-18 — Hoffmann fallback present without inline citation to Cherchi

**Severity**: DELIBERATE-DIVERGENCE (citation already inline at `exact_mesh.rs:1610-1612`).
**Cross-ref**: NEW.
**Code**: `exact_mesh.rs:1610-1672`.

The Hoffmann perturb-and-classify fallback (sample `±eps * normal`) is cited
to "Hoffmann 1989 §5.3 perturb-and-classify" in the function docstring
(`exact_mesh.rs:1493-1527`). This is an inline citation, but it conflicts
with `feedback_yang_only.md`: "if Yang cites Cherchi 2022, we implement
Cherchi 2022 — not patches." Hoffmann 1989 is older than Cherchi 2022 and a
different algorithm; using it in place of Cherchi's `perturbXRay/Y/Z` cascade
is a deliberate substitute, not a faithful port.

### YC-19 — `RayHit::Degenerate` short-circuits the per-axis loop on FIRST orient2d == 0

**Severity**: CORRECTNESS-BUG (loses information).
**Cross-ref**: NEW (extends YC-09).
**Code**: `exact_mesh.rs:1283-1284, 1389-1392`.

If ANY of the three orient2d tests (`o0`, `o1`, `o2`) equals 0, we set
`degenerate = true; break;` and abandon the entire axis. Cherchi's
`fast2DCheckIntersectionOnRay` continues processing because the SPECIFIC
orient2d that is 0 tells you which edge or vertex is hit (e.g. `or01 == 0`
means edge 01 is hit; if also `or12 == 0` then vertex V1 is hit). This
information is lost in our `Degenerate` collapse.

### YC-20 — `compute_global_max + 1.0` ray endpoint vs `octree.bbox.max + 0.5`

**Severity**: PERFORMANCE-DRIFT.
**Cross-ref**: NEW.
**Code**: `exact_mesh.rs:1354` — `slab_max[axis] = global_max[axis] + 1.0`.
**Paper**: Cherchi 2022 §5.1 line 421-426 / `booleans.cpp:57`.

C++ uses `octree.root->bbox.max +0.5`. We use `+1.0`. Both are valid
"point at infinity that is guaranteed to stay outside all input meshes"
under bounding-box scale, but `+1.0` is twice the displacement. Doubles the
ray length, which doubles the slab AABB volume, which doubles the BVH
candidates returned. Trivially fixable (`+0.5`), but inconsequential for
correctness.

### YC-21 — `addDuplicateTrisInfoInStructures` not called

**Severity**: DELIBERATE-DIVERGENCE.
**Cross-ref**: A-04, D-04 (prior audit) — handled via the cosurface-orientation
PR10 design.
**Code**: no analog in `exact_mesh.rs`.
**Paper**: Cherchi 2022 + `booleans.cpp:54` — `addDuplicateTrisInfoInStructures(...)`
restores duplicate-triangle metadata before patch construction.

C++ stores duplicate-triangle info in `dupl_triangles` during arrangement
(removed there) and reinjects them at boolean time. We instead carry
`cosurface_orientation: Option<CosurfaceOrientation>` on each `SubTriangle`
(`mesh_arrangement.rs::SubTriangle`). Functionally similar; structurally
different.

Cited at `cherchi/processing.rs:42-46` (PR10) per prior audit A-04.

### YC-22 — `intersects_box` octree query absent in our BVH path

**Severity**: PERFORMANCE-DRIFT.
**Cross-ref**: NEW.
**Code**: `exact_mesh.rs:215-231` — `BvhNode::query_overlapping`.
**Paper**: `booleans.cpp:550-589` (`intersects_box`).

C++ `intersects_box` uses `cinolib::Octree` with axis-aligned octants;
intersection-with-octant reduces to four float comparisons (paper §5.2 line
437-438). Our BVH `query_overlapping` does box-vs-box overlap on internal
nodes plus point-vs-leaf-AABB at leaves. Functionally equivalent O(log n) but
different axis-aligned shortcut: their octant-vs-AABB is 4 comparisons; our
box-vs-box is 6 (per axis). Trivial.

### YC-23 — `analyzeSortedIntersections` "first-on-each-label" semantics

**Severity**: NOT APPLICABLE in binary; will deviate if N-ary supported.
**Cross-ref**: see YC-14.

### YC-24 — D-13 per-triangle LocalMesh — out of audit scope

**Severity**: scoped out (handled by auditor-d retrospectively).
**Cross-ref**: D-13 (prior audit, DELIBERATE-DIVERGENCE).

Out of slice; flagged for auditor-d.

## §3 D-05 re-validation (the highest-priority finding from prior audit)

**Verdict: D-05 fix is FAITHFUL and STILL HOLDS at HEAD (commit `9f3c591`).**

Detail of the verification:

### Code path

`exact_mesh.rs:1374-1449`. The relevant excerpt:

```rust
let mut best_hit: Option<(f64, usize)> = None;
let mut degenerate = false;

for &tri_idx in &candidates {
    let tri = target_tris[tri_idx];
    let v0 = target_verts[tri[0]];
    let v1 = target_verts[tri[1]];
    let v2 = target_verts[tri[2]];

    match ray_tri_intersect_axis(axis, p, v0, v1, v2) {
        RayHit::Hit(t) => {
            if t > 0.0 && best_hit.is_none_or(|(t_min, _)| t < t_min) {
                best_hit = Some((t, tri_idx));
            }
        }
        RayHit::Degenerate => {
            degenerate = true;
            break;
        }
        RayHit::Miss => {}
    }
}
// ...
let inside = if let Some((_t_hit, tri_idx)) = best_hit {
    // ... fetch tri verts ...
    let mut ray_v1 = p;
    ray_v1[axis] = global_max[axis] + 1.0;
    let res = robust::orient3d(/* tv0, tv1, tv2, ray_v1 */);
    res < 0.0
} else {
    false
};
```

### Comparison to paper / C++

Paper Figure 5 (p.6): "the volume of the tetrahedron (t, p∞) is negative" → patch
is inside → "from inside to outside if the volume of the tetrahedron (ti, p∞) is
negative".

C++ `booleans.cpp:1292-1299`:

```cpp
double res = cinolib::orient3d(tv0.ptr(), tv1.ptr(), tv2.ptr(), ray.v1.ptr());
assert(res != 0 && "Problem in PointOrientation(...)");
return (res < 0) ? 1 : 0;
```

The Rust code mirrors this: first-hit triangle → `orient3d(tv0, tv1, tv2, ray_v1)`
→ `res < 0.0` is Inside. **Faithful.**

### What was removed

The pre-fix `hit_count % 2 == 1` parity-counting (referenced in the prior
audit's CORRECTNESS-BUG citation) is gone. Confirmed: a grep for
`hit_count` in `exact_mesh.rs` returns matches only in test names and
diagnostic strings:

- `exact_mesh.rs:1338-1340` — `hit_count` is a tuple field name in the
  `RAYCAST_DEBUG` instrumentation, NOT a parity counter (it's a 0/1
  hit_indicator now).
- `exact_mesh.rs:1397-1402` — instrumentation print site.

No live parity-counting path.

### Test calibration

The prior audit warned `label_cells_raycast_matches_gwn_for_offset_boxes`
PINS parity. Re-reading at `exact_mesh.rs:6403-6444`: the test now asserts
`inside_frac > 0.15 && inside_frac < 0.85` (a wide window). It is no longer
parity-pinned. Test was updated correctly. (See YC-17.)

### Subtle points that did NOT regress

(a) `t > 0.0` (strict) — matches "after p" semantic in paper §5.1 line 449
(intersections after the emanating point). Cherchi C++ uses `tv[3]`-based
sign-flipped pivot (see YC-15) which is equivalent for binary booleans where
the ray emanates from a different mesh's sub-tri.

(b) `best_hit` accumulator uses `t < t_min` (strict) so that ties at exactly
equal `t` are decided first-encounter-wins. Cherchi's `phmap::btree_set` uses
strict-less-than ordering on LPI exact comparator. Equivalent in spirit, but
see YC-04 for the broader f64-vs-LPI precision concern.

(c) `is_none_or(...)` is the correct idiom for "no prior hit OR new hit is
closer". Faithful.

(d) The `degenerate = true; break;` short-circuit (line 1390-1391) is
slightly DIFFERENT from C++: C++ would continue the loop and do per-vertex /
per-edge ambiguity dispatch (YC-09, YC-19). We `break` and try the next axis.
This is not a regression of D-05 specifically; it is YC-09/YC-19/YC-10
behavior.

### Conclusion

**D-05 is faithful.** No regression from `3e17f08`. The fix is the load-bearing
correctness change for the entire Cherchi 2022 §5 port, and it is intact.

The other Cherchi 2022 §5 deviations (YC-01..YC-15) sit AROUND this faithful
core. The first-hit signed-volume test is correct; what feeds it (ray definition)
and what handles its ambiguity (Degenerate cascade) are not.

## §4 What this slice did NOT cover

- **Cherchi 2020 arrangement layer** (`cherchi/*.rs`): covered by prior audit
  (`docs/audits/cherchi_port_audit.md` Findings A, B, C). All 42 prior-audit
  findings remain open or fixed per the prior audit's queue.
- **`indirect_predicates.rs`** (the predicate kernel itself): out of slice.
  Cluster I findings in prior audit (A-01, A-02, B-06, C-01..C-13) all
  point here as a future audit candidate.
- **`coplanar_preprocess.rs`** (Yang §4.5.5): covered by auditor-a (T1).
- **`mesh_arrangement.rs::triangulate_single_triangle`** (per-triangle LocalMesh
  on the SSI refinement path): D-13 in prior audit. Not on the live Cherchi
  pipeline per the prior audit's "Note (resolved by team-lead grep)" at line 709.
- **Yang §4.4.2 surface refinement**: out of slice; auditor-a's slice.
- **Tessellation conformality** (Yang §4.1.1 + `bijective.rs`): T4 (auditor-d)
  retrospective.
- **Performance benchmarks**: this is a paper-vs-port conformance audit, not
  an empirical performance benchmark. YC-12, YC-13, YC-22 (cached predicates,
  shared octree) are flagged conformance-wise without empirical measurement.
- **N-ary booleans**: paper supports >2 inputs (paper §6.4 variadic Booleans);
  Yang only supports binary. YC-14, YC-23 flagged for forward-compat but not
  evaluated against the assay.
- **Self-intersecting input behavior**: Cherchi paper §3 line 236-237
  preconditions "no self-intersections". Memory `[Yang Implementation Status]`
  notes 19/25 F-series have hidden self-intersections. The audit cannot
  determine which assay failures correlate with self-intersection vs other
  causes.

## §5 Findings count summary

| Severity | Count |
|----------|-------|
| **CORRECTNESS-BUG** | 5 (YC-02, YC-04, YC-07, YC-10, YC-19) |
| **MISSING** | 4 (YC-01, YC-08, YC-09, YC-15) |
| **DELIBERATE-DIVERGENCE** | 4 (YC-05, YC-06, YC-18, YC-21) |
| **PERFORMANCE-DRIFT** | 7 (YC-03, YC-11, YC-12, YC-13, YC-20, YC-22, YC-23) |
| **NONE / re-validation** | 2 (YC-16 D-05 still faithful, YC-17 test updated) |
| **Total** | 22 substantive findings |

(YC-14 and YC-23 marked "NOT APPLICABLE" in binary, listed but uncounted.)

## §6 Top-3 leverage findings (for lead synthesis)

Ranked by likely Yang-assay impact and effort-to-fix asymmetry:

1. **YC-01 (`findRayEndpoints` MISSING)** — Cherchi's §5.1 cascade is the
   load-bearing input to the §5.3 first-hit classification. Without it,
   axis-aligned CAD scenes systematically fall into the Hoffmann fallback,
   where YC-10 corrupts geometry. A faithful Cherchi `findRayEndpoints` would
   eliminate Hoffmann fallback as the load-bearing path. Effort: medium
   (depends on YC-05 patch infrastructure but can be approximated for
   per-sub-tri labeling by picking a sub-tri vertex + back-push).
2. **YC-05 (per-patch labeling DEVIATES)** — the architectural deviation that
   forfeits Cherchi's correctness invariant. Adjacent sub-tris of the same
   patch can receive inconsistent labels under our per-sub-tri scheme. Fixing
   this is large but transformative: it both fixes correctness AND restores
   Cherchi's headline complexity claim (#patches not #triangles). Effort:
   high.
3. **YC-07 (`weld_mesh_vertices` A15.6 violation)** — orthogonal to Cherchi
   conformance but flagged earlier as D-10 CORRECTNESS-BUG. Per
   `feedback_yang_only.md` and A15.6, the right fix is upstream
   (bijective tessellation per Yang §4.1.1, currently being audited by
   auditor-d as T4). Once T4 lands, `weld_mesh_vertices` becomes deletable.
   Effort: depends on T4.

Honorable mention: **YC-19** (information loss in `RayHit::Degenerate`) is a
small fix (split the enum into V0/V1/V2/EDGE01/EDGE12/EDGE20/COPLANAR variants)
that opens YC-09 and YC-10 to be addressed. Could be a fast first step.

## §7 Cross-reference table to prior audit (`cherchi_port_audit.md`)

| YC | Prior audit | Prior status | Current status |
|----|-------------|--------------|----------------|
| YC-01 | D-06 | UNKNOWN | confirmed MISSING |
| YC-02 | D-06 | UNKNOWN | (subset) confirmed MISSING |
| YC-03 | A-02 | CORRECTNESS-BUG | (related) PERFORMANCE-DRIFT for ray-cast use |
| YC-04 | NEW | — | new CORRECTNESS-BUG |
| YC-05 | D-13 / D-11 | UNKNOWN / DEL.DIV | confirmed DEL.DIV (architectural) |
| YC-06 | D-11 | UNKNOWN | confirmed DEL.DIV |
| YC-07 | D-10 | CORRECTNESS-BUG | unchanged (still violating A15.6) |
| YC-08 | D-12 | UNKNOWN | confirmed MISSING |
| YC-09 | D-07 | CORRECTNESS-BUG | confirmed (expanded detail) |
| YC-10 | D-07 | CORRECTNESS-BUG | confirmed (expanded detail) |
| YC-11 | D-09 | PERF.DRIFT | unchanged |
| YC-12 | B-01 | PERF.DRIFT | unchanged (broader: shared octree) |
| YC-13 | NEW | — | new PERF.DRIFT |
| YC-14 | NEW | — | scoped (binary OK, N-ary deviates) |
| YC-15 | NEW | — | new (subset of YC-01) |
| **YC-16** | **D-05** | **CORRECTNESS-BUG (PRIORITY 1)** | **FIXED at 3e17f08, still faithful** |
| YC-17 | (D-05 test follow-up) | (warn) | test updated correctly |
| YC-18 | NEW | — | DEL.DIV with citation |
| YC-19 | NEW | — | new CORRECTNESS-BUG |
| YC-20 | NEW | — | new PERF.DRIFT |
| YC-21 | A-04, D-04 | DEL.DIV | unchanged |
| YC-22 | NEW | — | new PERF.DRIFT |
| YC-23 | (= YC-14) | NEW | scoped |

8 out of 22 findings are NEW (not flagged by prior audit). The prior audit
correctly identified the high-priority issue (D-05) and shipped the fix; the
NEW findings are second-order issues exposed by reading the paper's
algorithmic details (vertex/edge dispatch, IntersInfo enum, cached
predicates, shared octree, tv[3] pivot, `analyzeSortedIntersections`
first-of-each-label semantics).

---

*This audit is read-only and characterizes state. No production code modified.*
