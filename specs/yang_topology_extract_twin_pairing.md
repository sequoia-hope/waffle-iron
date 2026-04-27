# Spec: Yang Topology Extract — Twin-Pairing Investigation Thread

## Goal

Document the half-edge twin-pairing failure mode that PR10's Phase D
validator identified as the dominant Yang assay failure (133 of 221
Yang failures match `half_edge[X].twin = 0 but twin.twin = expected_X`),
and capture PR11's investigation finding that this is a **downstream
symptom of an upstream `label_cells` misclassification**, not a
twin-pairing bug.

This is a fresh investigation thread distinct from
`yang_555_identical_footprint.md` (the §4.5.5 cosurface annihilation
thread closed by PR3-PR10). The two threads may interact (PR10's
correct cosurface classification ought to make this surface more
clearly), but the root causes are architecturally distinct.

## Background — what the failure looks like

The Yang `validate_yang_result_topology` enforces half-edge twin
symmetry: for every HE `i`, `arena.half_edges[i].twin = X` must imply
`arena.half_edges[X].twin = i`. The error format is

```
half_edge[{i}].twin = {twin_idx} but twin.twin = {actual} (expected {i})
```

Index 0 is a real half-edge, not a sentinel. Default
`twin: HalfEdgeIdx(0)` aliases real HE 0. So `half_edge[1].twin = 0
but twin.twin = 0` typically means HE 1 is unset (defaulted to 0) AND
HE 0 is also unset (its twin also defaults to 0). Both unpaired.

Twin assignment happens in
`crates/kernel/src/boolean/topology_extract.rs:flood_fill_patches`
(lines 776-778). The pairing loop at lines 754-805 iterates undirected
edges and matches forward/reverse half-edge candidates from a
`directed_he` map keyed on directed vertex pairs.

## PR11 Phase A finding — R0002 root cause

R0002 is a 2-feature assay part: Revolve cut=false (Union), then
Revolve cut=true (Subtract). The Subtract step fails. Trace
(`TWIN_DEBUG=1` instrumentation):

```
[yang-diag] after subdivide: tris_a=650, tris_b=12872, verts=4403
[yang-diag] after label_cells: A outside=650 inside=0 cosurface=0,
                                B outside=12872 inside=0 cosurface=0
[yang-diag] after survival: 6 groups, 650 tris (ALL mesh_id=A)
[topo-extract] summary: paired=65, unpaired=12, ambiguous=0
```

For Subtract, the survival keep-table is `keep_a = Outside, keep_b =
Inside`. But `label_cells` reports zero B sub-tris classified Inside —
so zero B sub-tris survive. All 6 patches are A-only.

The 12 unpaired half-edges sit on **A-face boundary edges** — places
where two adjacent B-Rep faces of mesh A meet (e.g., FaceIdx(0)'s
outer ring meeting FaceIdx(2)/3/4/5's side faces). Each unpaired edge
has the signature `fwd_count=2, rev_count=0` — two A-A boundary HEs
going the same direction, no B-side reverse partner. The missing
reverse partners *should* be B's interior wall-tris (B revolve cuts
into A; B's interior surface inside A should classify Inside under
the Subtract keep-table). With zero B sub-tris in survival, the
A-side boundary edges have nothing to pair against.

**This is a label_cells misclassification, not a twin-pairing bug.**

### PR10 is innocent

PR11 engineer-a re-ran R0002 against PR9 head (commit `0e526ef`).
Result: identical `A outside=650 inside=0, B outside=12872 inside=0`,
identical 12 unpaired HEs, identical failing HE signature. PR10's
cosurface fix did not cause R0002. The assay regression count
(~9 → 2) is partially attributable to PR10 for cosurface-affected
cases, but R0002-class failures pre-existed.

### Hypothesis verdicts (PR11's 4-way + 5th unanticipated)

a. **Cosurface annihilation asymmetry — NO.** Zero cosurface tris in
   R0002 (`cosurface=0` for both A and B). All 22785 provenance emits
   in the batch trace show `cosurface_orientation = None`.

b. **Parallel per-side over-drop — NO.** No `Some(Parallel)` anywhere
   in this case.

c. **Pre-existing bug exposed by PR10 — YES.** Confirmed by PR9 head
   re-run.

d. **Winding-direction mismatch — PARTIAL.** The fwd=2/rev=0 signature
   IS present, but it's a *consequence* of the upstream
   misclassification (zero B sub-tris in survival), not a Cherchi
   STAGE2-style winding bug.

**Conclusion: 5th unanticipated cause — upstream `label_cells`
misclassifies all 12872 B sub-tris as Outside.** Twin-pairing failure
is a downstream symptom. Fixing twin pairing would not resolve R0002.

## PR12 scope — investigate label_cells misclassification

The fix lives in `crates/kernel/src/boolean/exact_mesh.rs` —
specifically `label_sub_tri_raycast` and its supporting BVH /
generalized winding number code. PR12 will investigate why every
B-sub-tri in R0002's Subtract returns `CellLabel::Outside` from
`ray_cast_inside` against mesh A.

Hypotheses for PR12 to test:

1. **Axis-pick degeneracy on revolve geometry**: Revolve produces
   surfaces with rotational symmetry. If the BVH ray-cast picks an
   axis (`+x`/`+y`/`+z`) that's parallel to the revolve axis, hits
   may skim the surface in a way that returns degenerate parity. The
   PR9 instrumentation showed primary returns from any non-degenerate
   axis; verify here whether the chosen axis is degenerate for the
   revolve case.

2. **Tolerance / epsilon mismatch**: PR9-PR10 work didn't touch the
   `effective_eps` value. If revolve generates near-coplanar surface
   tris that hit the epsilon threshold differently than box tris, the
   misclassification could be tolerance-driven.

3. **BVH construction over revolve mesh has a bug**: e.g., empty
   sub-trees, incorrect AABB bounds, missed triangles.

4. **GWN fallback mis-fires**: when ray-cast returns `None` (all axes
   degenerate), the Hoffmann perturb-and-classify or GWN fallback
   from PR6 may return the wrong sign on revolve geometry.

5. **`subdivide_mesh_pair_full_cherchi` returns wrong B-sub-tri
   positions**: less likely (cherchi is well-tested), but possible if
   the bijective mapping `bijective_b.tri_face_ids[parent_tri]` has
   an off-by-one for revolve features.

PR12's investigation should:
- Pick a single B-sub-tri from R0002 that should classify Inside
  (e.g., one whose centroid is geometrically inside A's solid body).
- Trace `label_sub_tri_raycast` for that single sub-tri with
  `CHERCHI_DEBUG=1` (PR9 instrumentation already exists).
- Identify whether primary returns wrong, or fallback fires wrong, or
  position is wrong upstream.
- Memo lead, lock fix scope, ship Phase B-D.

## What PR11 ships — instrumentation only

### Files modified

- `crates/kernel/src/boolean/topology_extract.rs`:
  - `let twin_debug = std::env::var("TWIN_DEBUG").as_deref() == Ok("1");` at top of `flood_fill_patches`.
  - `SurvivingSubTri` augmented with diagnostic-only fields
    `cosurface_orientation: Option<CosurfaceOrientation>` and
    `parent_tri: usize`. Derived `Default`. Production constructors
    populate from `SubTriangle`. 16 in-file test constructors updated
    to `..Default::default()`.
  - `FlatSubTri` (struct internal to `flood_fill_patches`) gets the
    same two fields, populated from `SurvivingSubTri`.
  - New per-HE insert trace at insertion site (`[twin-debug] insert
    HE[i] (vN→vM) source=… parent_tri=… cosurface=…`).
  - New per-edge candidate-count trace in pairing loop
    (`[twin-debug] edge (lo,hi) fwd_count=N rev_count=M fwd_hes=[…]
    rev_hes=[…]`).
  - New `[twin-debug] paired HE[a] ↔ HE[b]` on success.
  - New `[twin-debug] UNPAIRED HE[i] (...): provenance=…` and
    `[twin-debug] AMBIGUOUS HE[i] (...): fwd_prov=… rev_prov=…`.
  - All existing `[topo-extract] unpaired forward HE`, `[topo-extract]
    ambiguous twin`, `[topo-extract] summary`, `[flood_fill DIAG
    Step5a]`, and `[yang-diag] flood_fill_patches: N unpaired` emits
    gated behind `twin_debug` (previously always-on).

- `crates/kernel/src/boolean/yang_integration.rs`:
  - At validation FAIL site (~L1028), new `[twin-debug] FAIL HE[i]`
    emit gated on `TWIN_DEBUG=1`. Production `Err(...)` message
    format unchanged.

### Verification

- `TWIN_DEBUG` unset = no behavior change (smoke check confirmed: 0
  twin-debug emits, 0 [topo-extract] emits across `batch_2op_extrude`
  with TWIN_DEBUG unset).
- `cargo test -p kernel --lib boolean::topology_extract`: matches
  baseline (PR10).
- `cargo test -p kernel --lib boolean::yang_integration`: matches
  baseline.
- `cargo clippy -p kernel --no-deps -- -D warnings`: no new warnings
  vs origin/main (93 baseline).
- `cargo fmt --check -p kernel`: clean.

### Out of scope for PR11

- Pre-existing red tests in `topology_extract.rs` (e.g.,
  `test_flood_fill_two_overlapping_boxes`) with nondeterministic
  unpaired counts varying 16-19 between runs. Pre-existing red,
  unrelated to PR11.
- The `label_cells` misclassification fix — that's PR12.

## References

- Yang et al. 2025 [#24] §4.4 (binary classification), §4.5 (B-rep
  extraction from labeled mesh), §4.5.5 (coplanar handling).
- Cherchi et al. 2020 §5.1 (Algorithm 1), §5.4 (coplanar pocket map),
  §5.5 (region growing — relevant to flood_fill_patches).
- ARCHITECTURAL_INVARIANTS A15.6 (Yang hybrid pipeline; flood-fill
  patch segmentation; do NOT use boundary-edge-chaining or greedy
  twin-pairing — already followed correctly in `flood_fill_patches`).
- Hoffmann 1989 §5.3 (perturb-and-classify; not directly relevant to
  this thread but cited in PR6 and continues to live in
  `label_sub_tri_raycast`).
- PR1-PR10 commit refs: `9fccebc`, `6695be1`, `12cf789`, `35ee814`,
  `17216a7`, `2b625f2`, `3d88fab`, `14f402f`, `0e526ef`, `5801571`.
- PR11 commit ref: `f2272d5`.
- PR12 commit ref: TBD (this commit).

## PR12 outcome — `label_cells` is innocent for R0002; PR11's diagnosis was wrong

PR12 (this commit) instrumented `ray_cast_inside` and
`label_sub_tri_raycast` with `RAYCAST_DEBUG=1` per-axis traces and
`fallback_path` tagging, then ran R0002 with full instrumentation
(`RAYCAST_DEBUG=1 CHERCHI_DEBUG=1 TWIN_DEBUG=1`).

**Finding**: PR11's R0002 diagnosis ("B's revolve cuts into A; B's
interior wall-tris should be Inside A") was based on a false premise.
**A and B are spatially disjoint operands.**

```
A sub-tri centroids (n=650):  x∈[-2.278,-0.958] y∈[1.400,2.697] z∈[1.514,2.356]
B sub-tri centroids (n=12872): x∈[ 0.507,2.300] y∈[-1.259,0.573] z∈[-1.488,0.303]
```

The two revolve operands sit ~4.6 m apart per the meta JSON; profile
sizes ~0.97 m and ~0.81 m respectively. A and B can't touch. Their
bounding boxes are disjoint on all three axes.

For Subtract of disjoint operands, the geometric truth is:
- All 650 A-sub-tris are Outside B → all classify Outside ✓ correct
- All 12872 B-sub-tris are Outside A → all classify Outside ✓ correct
- Survival keep_a=Outside, keep_b=Inside → 650 A-tris kept, 0 B-tris kept
- Result: A unchanged. **That is the correct answer for `A − B` when
  A∩B = ∅.**

`label_cells` is innocent. The output is geometrically correct.

### Fallback-path distribution (R0002, 13522 ray-casts)

```
fallback_path=primary       13522
fallback_path=hoffmann_*        0
fallback_path=gwn               0
fallback_path=cosurface_*       0

primary=Some(true)              0
primary=Some(false)         13522
primary=None                    0

axis=0(+x): used 13522 times, degenerate=N, candidates=0 → hit_count=0
axis=1, axis=2: never reached (early-return on first non-degenerate axis)
```

Every ray query received zero BVH triangles because the ray's slab
(1e-14 thick around centroid y/z) lay entirely outside the target
mesh's y/z bbox. The candidates=0 result is correct — there ARE no
triangles to test against.

ULP analysis: at |x|=2.3, slab_eps 1e-14 ≈ 40 ULPs — well-resolved,
not tolerance-degenerate. None of the 5 PR12 hypotheses (axis-pick
degeneracy, tolerance/epsilon mismatch, BVH bug, GWN misfire,
position bug) fires.

### The 6th unanticipated cause

**The actual bug is upstream of `label_cells`** in one of:

1. **Revolve tessellation produces self-intersecting input meshes.**
   The R0002 assay error message itself reports
   `no_self_intersection: 10 inter-face triangle penetrations,
   face pairs: (0,1), (0,1), (0,1), (0,2), (0,2), ...` — A's own
   faces (face 0 vs face 1, face 0 vs face 2) penetrate each other in
   the OUTPUT mesh. With A and B spatially disjoint, the boolean is
   a no-op pass-through; if the OUTPUT mesh has self-intersections,
   they came from A's INPUT mesh (or from Cherchi's processing of
   A's input).

2. **Pipeline orchestration: missing AABB-disjoint short-circuit.**
   For Subtract of disjoint operands, the boolean should return A
   unchanged without running Cherchi at all. Currently, R0002's
   pipeline pushes 650 A-tris + 12872 B-tris through
   `subdivide_mesh_pair_full_cherchi`, which reports STAGE4: 2253
   intersecting pairs and STAGE5: 1519 with_intersections. With A and
   B spatially disjoint, ALL of those pairs must be intra-A or intra-B
   (input-mesh self-intersections) — Cherchi is correctly faithful to
   pathological input.

The 12 unpaired half-edges PR11 detected (signature `fwd_count=2,
rev_count=0`, all on A-vs-A boundaries) are A's own intra-mesh
boundary collisions from input self-intersections — NOT missing
B-side reverse counterparts as PR11 hypothesized.

### PR13+ scope

Two parallel investigation threads:

1. **Revolve tessellation audit**. `crates/kernel/src/feature/revolve.rs`
   (and its triangulation pipeline) needs auditing for inter-face
   self-intersections. The `no_self_intersection` oracle in the assay
   captures this — count how many R-series cases have non-zero
   inter-face penetrations even before any boolean is run. Fix the
   tessellation source if the count is non-trivial.

2. **AABB-disjoint short-circuit in pipeline orchestration**.
   `crates/kernel/src/boolean/yang_integration.rs`'s
   `yang_boolean_pipeline()` should compute A's bbox vs B's bbox; if
   disjoint, return per-op:
   - Union: A and B unchanged, side-by-side merge.
   - Subtract: A unchanged.
   - Intersect: empty.
   No Cherchi pass needed. ~30-60 lines, single-file fix. PR13
   candidate.

### Generalization caveat (per `feedback_no_last_bug.md`)

PR12's finding is specific to R0002 — disjoint operands. **Other
R-series cases may have different root causes**: genuine `label_cells`
bugs (operands DO overlap but classification is wrong), genuine
cherchi STAGE bugs, or genuine twin-pairing bugs.

PR13+'s investigation should pick a case where bounding boxes
ACTUALLY overlap before assuming `label_cells` is suspect. Quick
filter: look at the assay results.json `no_self_intersection: 0
inter-face triangle penetrations` cases — those have clean input
meshes, so any boolean failure is genuinely in the boolean pipeline,
not the input. Conversely, cases with non-zero `inter-face triangle
penetrations` are revolve-tessellation problems that any downstream
fix can't address without first fixing the input.

### What PR12 ships — instrumentation only

#### Files modified

- `crates/kernel/src/boolean/exact_mesh.rs`:
  - `ray_cast_inside`: per-axis trace recorded into a fixed-size
    `[Option<(degenerate, candidates_len, hit_count)>; 3]` and emitted
    on `RAYCAST_DEBUG=1`. Behavior preserved (early-return on first
    non-degenerate axis is unchanged).
  - `label_sub_tri_raycast`: `fallback_path` tagging on each emit
    site (`primary`, `hoffmann_above_below`, `hoffmann_one_side`,
    `gwn`, `cosurface_antiparallel`, `cosurface_parallel`). Gated
    on `CHERCHI_DEBUG=1 OR RAYCAST_DEBUG=1`.

#### Verification

- `RAYCAST_DEBUG` and `CHERCHI_DEBUG` unset = no behavior change.
  `cargo test -p kernel --lib boolean::exact_mesh`: 95 passes / 6
  fails / 2 ignored — matches PR11 baseline exactly.
- `cargo fmt --check -p kernel`: clean.
- Clippy: 93 errors (matches PR11 baseline; zero new warnings).

## PR13 Phase A — Investigation + red-phase evidence

### Insertion-point investigation

- **Yang pipeline entry**: `crates/kernel/src/boolean/topology_extract.rs`,
  `yang_boolean_pipeline` at L1340 (signature L1340–1360, body starts L1361).
  Short-circuit must slot in before `subdivide_mesh_pair` at L1363.
- **Analytical-path precedent**: `crates/kernel/src/boolean/mod.rs:1304-1329`
  inside `boolean_op_from_polys_inner`. Pattern verified — per-axis bbox
  disjoint check with `tau` margin, then per-op trivial result construction.

### Caller chain — outcome (b): Yang path is fully separate

The dispatch in `crates/kernel/src/waffle_kernel.rs:do_boolean` (L978) tries
the Yang path FIRST inside `catch_unwind` (L1008–1015), via
`yang_boolean_from_solids` → `yang_boolean_inner` → `yang_boolean_pipeline`.

`yang_boolean_from_solids` at `yang_integration.rs:532` gates on the
`YANG_BOOLEAN=1` env var: when set, the Yang pipeline runs and any error
*other than* the "not enabled" gate aborts the request (per A15.6, line
1042–1049 in `waffle_kernel.rs` — Yang errors must not silently degrade).
When `YANG_BOOLEAN=1`, the analytical short-circuit at `mod.rs:1310` is
**never reached** — control returns from `do_boolean` before the legacy
dispatch.

PR13 is therefore **outcome (b)** in the plan's risk register: the Yang-path
short-circuit is the only line of defense for disjoint operands when
`YANG_BOOLEAN=1`. Full scope, not belt-and-suspenders.

### `de1_disjoint_boxes_union` baseline

- **Default path** (no env var): `cargo test -p test-harness --test
  boolean_edge_cases -- de1_disjoint_boxes_union --nocapture` → **PASS**
  (analytical short-circuit at `mod.rs:1310` fires for 5×5×5 boxes 15
  units apart, returns combined-faces compound).
- **Yang path** (`YANG_BOOLEAN=1`): same test → **PASS** (slow path —
  Cherchi runs but reports STAGE4 pairs=0 and STAGE5 with_intersections=0
  for clean disjoint box meshes; flood-fill produces 12 faces / 24 tris
  forming two valid disconnected manifolds).

The Yang path passes *for clean disjoint boxes* because the input mesh
has no self-intersections to amplify. R0002's failure mode is specific
to revolve-tessellated input that already contains inter-face penetrations
(per PR12 spec note above): in that case Cherchi faithfully reports
intra-A and intra-B self-intersection pairs, which propagate to
twin-pairing failures downstream.

### Red tests written (Phase A artifact)

Four tests added. All four are RED via compile failure (FIP §8 bug-fix
variant accepts compile-failure red).

| # | Test                                              | Location                                           | Red signal             |
|---|---------------------------------------------------|----------------------------------------------------|------------------------|
| 1 | `test_yang_disjoint_union_returns_two_bodies`     | `topology_extract.rs` `mod tests`                  | Compile-blocked by #4  |
| 2 | `test_yang_disjoint_subtract_returns_a_unchanged` | `topology_extract.rs` `mod tests`                  | Compile-blocked by #4  |
| 3 | `test_yang_disjoint_intersect_returns_empty`      | `topology_extract.rs` `mod tests`                  | Compile-blocked by #4  |
| 4 | `test_aabb_compute_from_mesh`                     | `exact_mesh.rs` `mod tests`                        | `Aabb::from_mesh` does not exist |

```
$ cargo test -p kernel --lib boolean::exact_mesh::tests::test_aabb_compute_from_mesh 2>&1 | grep "^error\[E0599\]" | head -3
error[E0599]: no function or associated item named `from_mesh` found for struct `exact_mesh::Aabb` in the current scope
error[E0599]: no function or associated item named `from_mesh` found for struct `exact_mesh::Aabb` in the current scope
error[E0599]: no function or associated item named `from_mesh` found for struct `exact_mesh::Aabb` in the current scope
```

Phase A probe (Test 4 body temporarily commented out, then restored):
Tests 1, 2, and 3 currently **PASS** under the slow path on clean
disjoint inputs (de1-class boxes 9 units apart). Their assertions are
tight enough that a Phase B short-circuit must reproduce the same
counts: Union → 6 A-faces + 6 B-faces, Euler 4, 0 unpaired HEs;
Subtract → 6 A-faces + 0 B-faces, Euler 2, 0 unpaired HEs; Intersect →
0 faces / 0 edges / empty `face_provenance`. Tests 1–3 therefore lock
in the **target green** for both the slow path on clean inputs and the
short-circuit Phase B will add. The red-phase signal lives in Test 4
(compile failure), which is FIP §8-acceptable. Red-phase log:
`/tmp/pr13_red_phase.log`.

### Recommended scope for Phase B (implementer)

- **Helpers needed**:
  1. `Aabb::from_mesh(verts: &[[f64; 3]]) -> Option<Aabb>` in `exact_mesh.rs`
     (single-pass min/max scan, returns None on empty input). Plan body
     is ~10 LOC.
  2. `yang_pipeline_result_for_disjoint(...)` in `topology_extract.rs`
     (or inline) — per-op trivial `YangPipelineResult`. Two viable shapes:
     - **Shape A — call existing helpers**: build a `SubdividedMesh`
       manually (Subtract: A only, B-empty; Union: both, with B verts
       offset; Intersect: empty), then run `label_cells` →
       `face_survival_detect` → `flood_fill_patches` → `build_result_brep`.
       Re-uses the production assembly. ~30–40 LOC.
     - **Shape B — direct `ResultTopology` construction**: build the
       arena directly from the input mesh tris via `build_result_brep`
       on synthetic trim boundaries. Smaller for Subtract/Intersect,
       grows for Union (two disconnected solids). ~50+ LOC.
- **Insertion-point**: top of `yang_boolean_pipeline`
  (`topology_extract.rs:1361`), before the `subdivide_mesh_pair` call
  at L1363.
- **Tau choice**: match `mod.rs:1310` — uses the `tau` returned by
  `compute_adaptive_tau_weld`. The Yang path doesn't currently compute
  this; safer choice is `crate::units::TAU_MODEL = 1e-7` (Yang-paper
  `d_p` per `yang_integration.rs:541`). Document the rationale in the
  short-circuit comment.
- **Estimated lines**: ~50–80 LOC total (helper + short-circuit +
  per-op trivial construction). Per the plan risk register (line 333),
  if the Union path grows beyond ~100 LOC scope-creep should be
  flagged to lead.
- **Blocker for implementer (Union path complexity)**: the Phase B
  plan flags a question — does `flood_fill_patches` correctly handle a
  `SubdividedMesh` containing two disconnected components (no
  cross-mesh intersections)? On clean disjoint box probes during Phase
  A, the existing slow path *does* succeed: Cherchi reports 0 STAGE4
  pairs, label_cells gives all-Outside, flood-fill produces 12 valid
  faces. So Shape A above (call existing helpers with a manually
  constructed `SubdividedMesh`) is the path of least resistance — the
  short-circuit's only job is to skip Cherchi.

## PR13 outcome — short-circuit shipped; +2 yang_fast, 57 short-circuit firings

PR13 (this commit) shipped the AABB-disjoint short-circuit at
`yang_boolean_pipeline` per the locked Phase A design. Path A-shape
implemented as recommended.

### What landed

- **`Aabb::from_mesh(verts) -> Option<Aabb>`** in `exact_mesh.rs`
  near the existing Aabb methods. Single-pass min/max scan; returns
  `None` on empty input. ~22 LOC including doc comment.
- **`Aabb` struct + fields** elevated from private to `pub(crate)` so
  `topology_extract.rs` can read `min`/`max` in the disjointness
  predicate.
- **AABB-disjoint short-circuit** at the top of
  `yang_boolean_pipeline` body (`topology_extract.rs:1361+`). Pattern
  mirrors the analytical-path precedent at `mod.rs:1310-1329`:
  ```rust
  if let (Some(aabb_a), Some(aabb_b)) = (Aabb::from_mesh(verts_a), Aabb::from_mesh(verts_b)) {
      let tau = crate::units::TAU_MODEL;
      let disjoint = (0..3).any(|i| aabb_a.max[i] + tau < aabb_b.min[i] || aabb_b.max[i] + tau < aabb_a.min[i]);
      if disjoint { return yang_pipeline_result_for_disjoint(...); }
  }
  ```
- **`yang_pipeline_result_for_disjoint` helper** in
  `topology_extract.rs`. Per-op:
  - **Union**: build a `SubdividedMesh` containing A-tris and B-tris
    with offset vertex indices, no cross-mesh intersections; reuse
    existing `label_cells → face_survival_detect → flood_fill_patches → build_result_brep`.
    Both bodies survive (each is Outside the other).
  - **Subtract**: pass A only with empty B; existing empty-target
    path at `exact_mesh.rs:1738-1752` returns Outside for all A.
    Survival: A only. Topology: A's BRep unchanged.
  - **Intersect**: return empty `YangPipelineResult` directly. No
    SubdividedMesh needed.
- ~150 LOC total (~108 helper + ~17 short-circuit + ~22 `Aabb::from_mesh`).

### Test results

| Test | Result |
|------|--------|
| `test_yang_disjoint_union_returns_two_bodies` | GREEN |
| `test_yang_disjoint_subtract_returns_a_unchanged` | GREEN |
| `test_yang_disjoint_intersect_returns_empty` | GREEN |
| `test_aabb_compute_from_mesh` (6 cases) | GREEN |
| canary `test_stacked_box_union_correct_topology` | GREEN |
| `boolean::indirect_predicates` | 68/68 |
| `boolean::cherchi` | 59/59 |
| **kernel total** | **1175p/28f → 1179p/28f (+4 net)** |
| Clippy | 93 errors → 93 errors (zero new warnings) |
| Fmt | clean |

### Yang_fast assay

- PR12 baseline: **2 passing** of 157 non-timeout cases.
- PR13 post: **4 passing**, 151 failed, 2 errored, 33 known timeouts.
- **Net +2 passes.**
- Short-circuit fired **57 times** across the assay run (visible via
  `[yang-diag] AABB-disjoint short-circuit: skipping Cherchi for X`).
  This means 57 disjoint-pair operations now skip Cherchi entirely.

### Adversarial mutation results (validator Phase C)

- **Mutation #1** (invert disjoint check `any` → `all`): the 4 disjoint
  tests still PASS — Phase A test fixture is disjoint on ALL three
  axes simultaneously, so `any` and `all` produce identical results
  for this fixture. The disjoint check is mathematically correct
  (per Yang §4.2.1 + analytical-path precedent at `mod.rs:1310`); the
  test suite simply doesn't differentiate. **Test-coverage gap, not
  bug.** Optional follow-up PR could add a "disjoint on x only,
  overlapping on y/z" probe.
- **Mutation #2** (Union ↔ Subtract paths): 1/3 disjoint tests fail
  (Union test). Subtract still passes because flipping is_union under
  Subtract just adds B's mesh which gets labeled Outside under
  Subtract's keep-table — still 6 A-faces, 0 B-faces. Intersect
  bypasses this branch (early return). **Per-op semantics ARE
  load-bearing for the Union path.**
- **Mutation #3** (bypass short-circuit entirely): 4/4 still pass via
  slow path. Confirms slow-path-equivalence regression guarantee for
  clean disjoint inputs.

### R0002 status — still failing, root cause unchanged from PR12

R0002's failure mode after PR13 is consistent with PR12's diagnosis:
input mesh A has intra-A self-intersections (revolve tessellation
produces inter-face penetrations for face pairs (0,1)x3, (0,2)x2,
etc.). Trace:

```
[yang-diag] AABB-disjoint short-circuit: skipping Cherchi for Subtract
result validation failed: half_edge[1].twin = 0 but twin.twin = 0 (expected 1)
```

The short-circuit fires correctly (Cherchi is skipped). The
`flood_fill_patches` invocation on A's input alone faithfully reports
the 12 unpaired half-edges from A's intra-mesh boundary collisions.
**PR13 is innocent for R0002**; the upstream root cause (revolve
tessellation) remains a PR14+ concern.

Per `feedback_no_last_bug.md` and `feedback_no_regression_chasing.md`:
PR13's win is closing the boolean-pipeline-amplification contribution
for ALL future disjoint-operand cases. The 57 short-circuit firings
in the assay represent 57 fewer cases where Cherchi might amplify
upstream input pathologies. PR13 does NOT claim to fix R0002.

### Edge case behavior

- **Touching operands** (A=[0,1]³, B=[1,2]³ sharing x=1 face): NOT
  disjoint per `1+1e-7 < 1` = false. Falls through to Cherchi.
  Correct (cosurface case must run through full pipeline).
- **tau-close gap** (gap of 5e-8, below tau=1e-7): NOT disjoint.
  Falls through. Correct.
- **tau-far gap** (gap of 1.0, far above tau): IS disjoint.
  Short-circuit fires. Correct.

### PR14+ scope

The dominant remaining yang_fast failure mode is unchanged from PR12:
input mesh self-intersections from `revolve` tessellation. Two parallel
follow-up threads:

1. **Revolve tessellation audit** — fix `crates/kernel/src/feature/revolve.rs`
   to not produce inter-face penetrations. Likely scope: investigate
   cap+side stitching, axis-of-rotation handling, profile self-intersection
   detection in the input.
2. **Optional follow-up**: add a "partial-axis disjoint" red test
   (operands disjoint on x but overlapping on y and z) to close the
   Mutation #1 test-coverage gap surfaced by validator.

## PR14 outcome — kernel revolve_polygon rejects axis-straddling profiles

PR14 (this commit) shipped Defect 1 of two defects identified by Phase
A investigation. The kernel `revolve_polygon` validator at
`waffle_kernel.rs:1407-1486` now rejects profiles that straddle the
revolve axis with `KernelError::Other { message: "revolve
self-intersection: profile straddles the revolve axis" }`.

### What landed

- **Phase A instrumentation** in `crates/kernel/src/tessellation/mod.rs`
  (`tessellate_revolve_lateral` per-triangle emits, `tessellate_polygon_face`
  per-cap emits gated on `REVOLVE_DEBUG=1`). Per-emit format:
  `[revolve-tess] face={kid} face_kind=lateral|cap parent_edge={idx}
  ring_idx={i} verts=[v0,v1,v2] coords=[(x,y,z),(x,y,z),(x,y,z)]`.
- **Phase A reproducer**: `pr14_r0002_repro` module in `waffle_kernel.rs`
  with two `#[ignore]` tests:
  `pr14_r0002_first_revolve_repro` (R0002's exact rectangle revolve at
  347.34° — produces 5280 inter-face penetrations on pre-fix code) and
  `pr14_r0002_control_shifted_profile` (same axis/plane/angle, profile
  shifted to one side — 0 penetrations). Documentary evidence; not the
  red-before-green test.
- **Phase C red-before-green tests** in `pr14_validator_tests` module:
  `test_revolve_axis_straddling_profile_rejected` (asserts `Err(...)`
  with substring "self-intersection"/"straddle"/"axis") and
  `test_revolve_one_sided_profile_succeeds` (regression guard:
  `Ok(...)` + non-empty + watertight).
- **Phase D validator fix** at `waffle_kernel.rs:1407-1486`. After
  the existing on-axis check (Check 2: `dist < TAU_MODEL`), compute
  signed projection of each profile vertex's perpendicular component
  onto a reference direction `ref_dir = axis_dir × plane_normal`
  (with fallback to `perps[0]` when the cross product degenerates).
  If any vertex has `signed > +TAU_MODEL` AND any other has
  `signed < -TAU_MODEL`, the profile straddles the axis — return
  `KernelError::Other`.
- ~70 LOC of new validator code, additive only. Cite Yang 2025 §4.1
  (tessellation requires bijective mapping between B-Rep face and
  triangle mesh; straddling violates this).

### Reference-direction choice — `axis_dir × plane_normal`

Geometrically intrinsic: the in-plane direction perpendicular to the
axis. Stable under profile rotation/reordering. Validator
adversarial-mutation test confirmed this choice is load-bearing —
collapsing `ref_dir = axis_dir` zeros all signed projections (since
`perp ⊥ axis_dir` by Check 2 construction), and the new check never
fires.

### Phase A trace evidence

R0002 first revolve (rectangle profile, signed perpendicular distances
`[-0.31, -0.58, +0.31, +0.58]`) produces 5280 inter-face penetrations
across face-range pairs:
- `(start_cap, end_cap) = 3` (matches assay's `(0,1)×3` exactly)
- `(start_cap, lateral_*) = 178`
- `(end_cap, lateral_*) = 159`
- `(lateral_bottom, lateral_top) = 2276` (opposite rectangle edges)
- `(lateral_bottom, lateral_left) = 1332`
- `(lateral_right, lateral_top) = 1332`

Control test (rectangle shifted to `v ∈ [0.7, 1.7]`, profile fully on
one side of axis): 0 penetrations. Differential test confirmed root
cause.

### Test results

| Test | Result |
|------|--------|
| `pr14_validator_tests::test_revolve_axis_straddling_profile_rejected` | GREEN (was RED) |
| `pr14_validator_tests::test_revolve_one_sided_profile_succeeds` | GREEN |
| `waffle_kernel::tests::l1_revolve_half_turn_volume` | GREEN |
| `waffle_kernel::tests::l3_revolve_half_turn_watertight` | GREEN |
| `waffle_kernel::tests::l5_revolve_euler_characteristic` | GREEN |
| `waffle_kernel::tests::l6_revolve_topology_counts` | GREEN |
| `waffle_kernel::tests::l7_revolve_face_geometry_types` | GREEN |
| `waffle_kernel::tests::rc1_revolve_start_cap_normal_outward` | GREEN |
| canary `test_stacked_box_union_correct_topology` | GREEN |
| `boolean::indirect_predicates` | 68/68 |
| `boolean::cherchi` | 59/59 |
| **kernel total** | **1179p/28f → 1181p/28f (+2 net)** |
| Clippy | 93 errors → 93 errors (zero new warnings; +2 instrumentation lints fixed in Phase F integration) |
| Fmt | clean (Phase F applied `cargo fmt -p kernel` mechanically) |

### Adversarial mutation results (validator Phase E)

- **Mutation #1** (invert sign-check `!(saw_pos && saw_neg)`): Test 1
  fails. Conjunction is the actual rejection criterion.
- **Mutation #2** (remove the entire check block): Test 1 fails. The
  new check is the only line of defense.
- **Mutation #3** (`ref_dir = axis_dir`): Test 1 fails. Cross-product
  reference direction is geometrically load-bearing — collapsing it
  zeros all signed projections.

### Yang_fast assay impact

PR13 baseline: 2/157 passing.
PR14: 2/157 passing (unchanged), 141 failed, 13 newly errored.

**13 R-series cases shifted from silent broken output to explicit
"revolve self-intersection: profile straddles" error**: R0002, R0004,
R0005, R0020, R0033, R0038, R0044, R0045, R0049, R0050, R0093, R0094,
R0096.

This is the expected category shift per `feedback_no_regression_chasing.md`:
the kernel is now correctly rejecting invalid input. The corpus
generator (Defect 2 at `gen.rs:614-628`) produces axis-straddling
profiles by mistake; once PR15 fixes the generator and regenerates
the corpus, most of these 13 cases should turn into actual passes.

### PR15 scope — assay generator axis_offset bug (already documented in PR14 Phase A memo)

Engineer-a's Phase A investigation also identified Defect 2 at
`crates/test-harness/src/assay/gen.rs:614-628`:

> "axis_direction = tangent AND axis_origin = op_origin + axis_offset
> × tangent — i.e., axis_origin is offset along the axis line itself,
> which does NOT shift the line in space. The 1.5× offset is
> geometrically a no-op."

Fix: offset along an in-plane perpendicular to `tangent`. ~5 LOC change
in test-harness, plus corpus regeneration (rerun `assay_gen --seed 42
--count 100`). Track for PR15.

PR14 must land first because PR15's fix removes the input that
exercises PR14's red test (axis-straddling rectangle). Order matters.

### Known caveat (per `feedback_no_last_bug.md`)

PR14 fixes ONE defect in revolve geometry validation. It does NOT
claim to fix all R-series failures. The remaining 28 boolean-pipeline
failures pre-exist and are unrelated to revolve. PR15 (generator fix)
will produce inputs the kernel now accepts; whether those inputs
produce CORRECT boolean output is a separate question (potentially
testing other downstream bugs).

## PR15 outcome — assay generator axis_offset fix + corpus regeneration

PR15 (this commit set) fixed Defect 2 from PR14 Phase A
(`crates/test-harness/src/assay/gen.rs:614-628`): the revolve
`axis_origin` was offset along `tangent`, which IS the axis
direction itself, so the offset was a geometric no-op. The
intended 1.5×profile_size offset that should keep the profile
on one side of the axis never moved the axis line.

### What landed

- **gen.rs:614-660** (~26 LOC): replaced `axis_offset * tangent`
  with `axis_offset * perp_unit` where
  `perp_unit = normalize(cross3(op_normal, tangent))`. The cross
  product produces the in-plane direction perpendicular to the
  axis line (since `op_normal ⊥ tangent` by construction at L599-612).
- **Phase A red-before-green test** in gen.rs mod tests:
  `test_revolve_axis_offset_perpendicular_to_tangent` asserts
  `dot(offset, axis_direction).abs() < 1e-9`. Pre-fix dot product
  was 1.46 (~9 orders of magnitude failure margin); post-fix is
  ~1e-16.
- **Corpus regeneration** (`cargo run -p test-harness --bin assay_gen
  -- --seed 42 --count 100`): 200 R-series files (.waffle +
  .meta.json) overwritten. RNG state preserved exactly — only the
  `axis_origin` field shifts; `axis_direction`, profile, plane,
  angle, sketch_id all byte-identical pre/post. R-series .meta.json
  files are byte-identical (no axis_origin field at meta level).

### Spot-check (all 6 verified)

| Case | profile_size | dot(offset, dir) pre | dot(offset, dir) post | \|offset\| post |
|------|--------------|----------------------|-----------------------|-----------------|
| R0002 rev[0] | 0.973 | +1.4595 | 1.67e-16 | 1.4595 = 1.5 × 0.973 ✓ |
| R0002 rev[1] | 0.807 | +1.21 | 1.39e-17 | 1.21 = 1.5 × 0.807 ✓ |
| R0044 rev[0] | 924.5 | +1386.7 | 0.0 | 1386.7 = 1.5 × 924.5 ✓ |
| R0044 rev[1] | 1590.8 | +2386.2 | 0.0 | 2386.2 = 1.5 × 1590.8 ✓ |
| R0093 rev[0] | 0.00196 | +0.00294 | -8.81e-20 | 0.00294 = 1.5 × 0.00196 ✓ |
| R0093 rev[1] | 0.00543 | +0.00561 | +1.08e-19 | 0.00815 = 1.5 × 0.00543 ✓ |

### Yang_fast assay impact (validator Phase C)

PR14 baseline: 2 passing, 156 failed, 13 errored ("straddles axis").
PR15 post:     **6 passing**, 149 failed, 2 errored.

- **+4 net passing** (R0080 + 3 others including F0073-F0074
  passing because their `expect_rebuild_error=true` is now correctly
  satisfied by PR14's Check 3 rejection).
- **"straddles axis" errors: 13 → 0** for 12 of the 13. R0004
  retains 1 such error — see "Residual: R0004 basis-consistency
  bug" below.
- **2 remaining errored cases** (R0050, R0071) are pre-existing
  30s timeouts unrelated to PR15.

### Adversarial mutation (validator Phase C)

Reverting PR15's fix to `axis_offset * tangent[i]` → Phase A test
fails with "dot(offset, axis_direction) = 1.4595..." Restoration
verified clean via byte-diff.

### Caveats and follow-up scope

PR15 surfaced three additional bugs that are OUT of PR15 scope:

1. **R0004 basis-consistency bug** (PR16+): the generator computes
   `tangent` using `compute_plane_basis`-style basis (helper = least-
   aligned world axis). The kernel's `tangent_x_from_normal` at
   `feature-engine/src/rebuild.rs:1009` uses helper = Z if |nz|<0.99
   else X. The two functions produce DIFFERENT in-plane bases, so
   the profile's stored 2D coordinates land on different 3D
   positions when interpreted by the kernel vs the generator. R0004
   has signed distances that are one-sided in `compute_plane_basis`
   but straddling in `tangent_x_from_normal`. Fix: pick one basis
   convention and use it everywhere; OR ensure the generator queries
   the kernel for the canonical basis.

2. **F0011-F0015 oracle regression** (PR16): the generator's
   `disjoint` heuristic at gen.rs:L1256 sets
   `expect_rebuild_error=true` for AABB-disjoint operands. This
   was correct pre-PR13 (kernel errored on disjoint operands) but
   PR13's AABB-disjoint short-circuit made the kernel handle them
   correctly. Commit ef70415 (2026-03-28) hand-edited F0011-F0015's
   oracle to `false`, but PR15's regeneration overwrites that
   hand-edit. Fix: remove the obsolete `disjoint` heuristic from
   gen.rs, OR re-apply ef70415's hand-edits as a permanent override
   table.

3. **F-series UUID/timestamp churn** (PR17+, tied to determinism
   conversation): `Uuid::new_v4()` in the generator is non-
   deterministic. Each regeneration produces 90 F-series .waffle
   files with new UUIDs but identical geometric content. The diff
   is mechanically derived; the geometric content is preserved.
   Fix: use seed-derived UUIDs (e.g., UUID v5 with a deterministic
   namespace).

Per `feedback_no_last_bug.md`, PR15 doesn't claim to close all
revolve-related failures. The +4 yang_fast passes + 12/13 "straddles"
elimination is the concrete win.

### PR15 commit set

- `fix(test-harness): assay generator offsets revolve axis perpendicular to tangent`
- `chore(assay): regenerate R-series corpus post-PR15`
- `chore(assay): update results.json post-PR15`
