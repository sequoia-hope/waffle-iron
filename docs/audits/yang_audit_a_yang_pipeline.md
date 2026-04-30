# Yang 2025 Pipeline Audit — Auditor A (Orchestrator + Coplanar + Topology)

**Audit date**: 2026-04-30
**Team**: `yang-audit-2026-04-30`, role `auditor-a`
**Slice**: Yang pipeline orchestrator + Stage 0/5/6 helpers
**Files audited**:
- `crates/kernel/src/boolean/yang_integration.rs` (4127 lines)
- `crates/kernel/src/boolean/mod.rs` (3380 lines — only the BoolOp dispatch / type surface; clipping body deliberately skipped, see §4)
- `crates/kernel/src/boolean/coplanar_preprocess.rs` (3302 lines — full read of detection + B-Rep splitting + identical-footprint + partial-overlap injection)
- `crates/kernel/src/boolean/topology_extract.rs` (7301 lines — `flood_fill_patches`, `face_survival_detect`, `yang_boolean_pipeline`, `build_result_brep`)
- Cross-checked into: `crates/kernel/src/boolean/exact_mesh.rs::label_cells` and `ray_cast_inside` (Stage 4b classification — needed because it’s consumed by `yang_boolean_pipeline`)

**Papers re-read in full**:
- `docs/references/yang2025_hybrid_boolean.txt` (lines 1–2363)
- `/tmp/cherchi2022.txt` (lines 1–852)
- `governance/ARCHITECTURAL_INVARIANTS.md` §A15.6 (in-tree)
- `governance/ENGINEERING_CONSTITUTION.md` §P5/P8/P9/P10
- `docs/audits/cherchi_port_audit.md` (42-finding prior audit; reused as cross-reference)
- `docs/audits/yang_2025_audit.md` (prior step-by-step verdict — see §3 for disagreement)

---

## §1 Stage-by-stage assessment

### Stage 0 — Coplanar preprocessing (Yang §4.5.5)

> Yang p. 1281–1292: "When conducting Boolean operations on B-Rep models, the coplanarity between the surfaces of the two models is a commonly encountered degenerative case. As our discretization method does not maintain coplanarity in triangle meshes because of floating-point error introduced in discretization, it may lead to incorrect Boolean operation results. Therefore, it is necessary to check coplanar planes and perform 2D Boolean operations **before mesh discretizations**. […] The overlapping part is replaced by a trimmed common planar surface, and identical meshes are generated for both models in this part."

**Status**: PARTIALLY PRESENT / DEVIATES.

**Code site**:
- Detection: `coplanar_preprocess.rs:75-127` (`detect_coplanar_face_pairs`)
- B-Rep split (pre-tess): `coplanar_preprocess.rs:180-399` (`split_brep_for_coplanar_pairs`) — calls `split_face_along_boundary` (L441) which uses `split_edge_at` + `mef`.
- Identical-footprint mesh injection (POST-tess): `coplanar_preprocess.rs:902-1025` (`inject_identical_footprint_mesh`)
- Partial-overlap mesh injection (POST-tess): `coplanar_preprocess.rs:1049-1275` (`inject_partial_overlap_mesh`)
- Orchestration: `yang_integration.rs:573-687` — three calls (split, identical-footprint inject, partial-overlap inject)

**Conformance assessment**:
- The detection (face-plane comparison + offset alignment) at `coplanar_preprocess.rs:92-122` looks correct and covers both same-direction (dot ≈ +1) and anti-parallel (dot ≈ -1).
- The B-Rep splitting (`split_brep_for_coplanar_pairs` at `coplanar_preprocess.rs:180`) DOES happen pre-tessellation, satisfying Yang's "before mesh discretizations" requirement *for the B-Rep topology*.
- HOWEVER, the actual generation of "identical meshes […] in this part" runs POST-tessellation via `inject_identical_footprint_mesh` and `inject_partial_overlap_mesh` (`yang_integration.rs:661-687`). The order in `yang_boolean_inner` is:
   1. Detect (L575) → 2. Split B-Rep (L578) → 3. Tessellate (L614-615) → 4. Inject identical footprint mesh into tessellation (L661) → 5. Inject partial overlap mesh into tessellation (L677).
- This is a "pre-split B-Rep + post-tess overlay" hybrid, NOT what Yang §4.5.5 prescribes. Yang's pipeline does the 2D Boolean BEFORE tessellation; the overlap surface is then a *single trimmed planar surface that both models share*, and a single tessellation pass produces identical triangles. Our pipeline tessellates the two solids INDEPENDENTLY, then patches the overlap region by replacing triangles afterward. Two functional consequences:
  - Boundaries of the canonical overlap mesh meet adjacent (non-coplanar) face triangulations at T-junctions; the code calls `repair_tjunctions_after_injection` (`coplanar_preprocess.rs:1263-1264`) to clean these up. Yang's pre-tessellation approach makes T-junctions structurally impossible.
  - `repair_tjunctions_after_injection` is a single-pair single-pass fix (see comment at L1042-1047). The author explicitly notes that "Multi-pair cascading is PR8" — i.e., when several coplanar pairs share vertices, T-junction repair is incomplete. Since Yang §4.5.5 Fig. 1(b)/(c) Boolean union of 24 coplanar elliptical cylinders is one of the paper's stated correctness benchmarks, this is a real gap.
- The B-Rep split (`split_face_along_boundary`, `coplanar_preprocess.rs:441-619`) iterates each overlap-boundary vertex looking for an existing face vertex (L473-494) or a face edge to split (L497-527). The else-branch silently increments `COPLANAR_VERTS_DROPPED` (L530) when neither match — a vertex is **dropped entirely** with no error. (Finding YA-04.)
- For overlay polygons containing holes (e.g., complex face boundaries), `coplanar_preprocess.rs:262-265` and `:271` only consume `overlap[0][0]`, while incrementing `COPLANAR_OVERLAY_HOLES_IGNORED` for any extra contours. (Finding YA-05.)
- Same-direction partial overlap is handled (since PR17 noted at L307–315), but the comment at L1039-1042 says it "requires cascading T-junction repair across coplanar pairs sharing vertices/edges, which is deferred to PR8". So same-direction partial overlap with adjacent coplanar pairs is silently incorrect.

---

### Stage 1 — Bijective tessellation (Yang §4.1 / §4.1.1)

> Yang p. 518–523: "Our method first discretizes each closed B-Rep model composed of multiple surface patches into a triangle mesh. Then a bijective mapping between each surface patch and its discretization is constructed. The generated triangle mesh is a closed, watertight manifold under a given surface-to-mesh distance tolerance 𝑑𝜖 from the original B-Rep model."

> Yang p. 592–598: "[…] we discretize each surface patch independently without considering its neighbors, re-sample the boundary curves, and reconstruct the triangulation around the boundaries."

**Status**: STUB / DEVIATES (out-of-slice in deep detail; flagged for auditor-d).

**Code site**:
- Tessellation entry from this slice: `yang_integration.rs:910-927` (`tessellate_waffle_solid`) → `tessellation::tessellate_solid_ext_with_lod`. Body lives in the `tessellation` crate, out of slice.
- Bijective mapping: `BijectiveMap::from_render_mesh` and `compute_vertex_params` (`yang_integration.rs:643-655`).
- Post-tessellation deduplication: `yang_integration.rs:1158-1190` (`dedup_mesh_vertices`) — quantizes at `QUANT_NANOMETER_SCALE` to merge per-face boundary vertices.

**Conformance assessment** (slice-bounded):
- `dedup_mesh_vertices` runs on each operand independently AFTER tessellation. This is the local "fix" for the per-face-vertex tessellation that produces non-shared vertices at face boundaries. Yang §4.1.2 prescribes a single CDT pass at NURBS boundaries to *generate* a watertight mesh; we instead tessellate per-face and then merge.
- The dedup uses a fixed nanometer scale (`yang_integration.rs:1160`). Yang's `𝑑𝜖` is scale-relative (10⁻²·d, see `yang_integration.rs:611`), so a model with a 10 m diagonal has `𝑑𝜖 = 0.1 m`, yet vertex dedup is at 1 nm — 8 orders of magnitude tighter. Aggressive dedup is harmless when input vertices are exact, but fragile when tessellation vertices have 𝑑𝜖-bounded error.
- A `bijective_a.is_complete()` guard (`yang_integration.rs:646-650`) returns NotSupported if any sub-triangle lacks a bijective face id. There is no recovery; freeform/NURBS faces would surface as bijective gaps.
- This whole stage's correctness against Yang §4.1.1 is an auditor-d slice; auditor-a only verifies that the orchestrator wires the boundary correctly. **Wiring is correct**: tessellation is called BEFORE Cherchi (`yang_integration.rs:614-615`), and bijective maps are built from the tessellation output (L643-644).

---

### Stage 2 — Mesh arrangement (Yang §4.2 / Cherchi 2022 §4)

> Yang p. 644–654: "In this step, we detect intersections and compute intersection lines between the two triangle meshes. We refer to [Cherchi et al. 2022; Livesu 2019] as the mesh intersection computation method used in our paper."

> Cherchi 2022 §4 (line 287-300): "From the perspective of the arrangement algorithm, the input meshes 𝑀1, 𝑀2, . . . , 𝑀𝑛 can be seen as a soup of possibly intersecting triangles. We therefore flatten all input triangles into a single array, associating to each triangle a tag […]"

**Status**: PRESENT (via Cherchi port) — extensive deviations documented in `cherchi_port_audit.md`.

**Code site**:
- Entry from this slice: `topology_extract.rs:1533` (`subdivide_mesh_pair` from `exact_mesh.rs:2129`).
- Body: `exact_mesh.rs:2147-2288` (`subdivide_mesh_pair_full_cherchi`) — merges both meshes into a single labeled soup and calls `cherchi::solve_intersections` (out-of-slice; covered by `cherchi_port_audit.md` and auditor-c's slice).

**Conformance assessment**:
- Architecture is correct: meshes are merged into `in_coords/in_tris/in_labels` (L2156-2173), label bit 0 = mesh A, label bit 1 = mesh B, label `3` = coplanar duplicate. This matches Cherchi 2022 §4's "soup with tags".
- Stage-2 conformality and predicate correctness are **not verified** in this slice — that's the cherchi_port_audit (42 findings, of which 12 CORRECTNESS-BUG). Cross-referenced where they impact orchestration.
- `cosurface_orientation` field is propagated through the pipeline (`exact_mesh.rs:2226, 2250, 2275`) to enable the Hoffmann annihilation rule in Stage 4b. This is a deliberate addition beyond Cherchi 2022 (cherchi_port_audit A-04 / D-04). It is **not in Yang** but is consistent with Hoffmann 1989 §5.3.

---

### Stage 3 — SSI refinement (Yang §4.3)

> Yang p. 824–834 + 937–950: "From the previous step, we obtained the intersection points of the meshes and their corresponding projections on the parametric domains. […] We use a numeric optimization method to obtain more accurate intersection curves and distinguish tangent points and small loops."

**Status**: PRESENT (basic structure).

**Code site**:
- `topology_extract.rs:1559-1616` — `optimize_intersection_vertices` + iterative `recover_failed_regions` loop (Yang §4.5.1).
- `topology_extract.rs:1622-1637` — `correct_reversed_intersections` (Yang §4.5.3).
- `yang_integration.rs:803-833` — `classify_intersection_edges` + `refine_intersection_edges` (Yang §4.3 method dispatch).
- `yang_integration.rs:858-860` — `refine_vertex_positions` (Yang §4.3 final projection).
- `yang_integration.rs:865-870` — `update_mesh_along_refined_curves` (Yang §4.4.1 mesh update).

**Conformance assessment**:
- The orchestration ordering is correct: optimize → recover → correct reversed → label → patch → refine vertex positions → CDT mesh update.
- Both Newton (Yang §4.3.1) and geometric-method (Yang §4.3.2) selection are present; depth audit out-of-slice (auditor-d's tessellation thread for the `intersection_opt.rs` body).
- The refine-edge step at `yang_integration.rs:803-833` swallows BOTH `Err(classify)` and `Err(refine)` and proceeds with an empty `EdgeRefinementMap`. Comment at L813-815 says "proceeding with mesh-derived geometry". Per A15.6 the SSI refinement IS the analytical preservation; failing to refine means analytical surfaces are silently abandoned. (Finding YA-09.)
- `refine_vertex_positions` (`ssi_refinement.rs:258-278`) projects each vertex independently to `curve.closest_point(p)`; vertices shared between multiple refined edges get tracked via a `refined: HashSet`, so each vertex moves to whichever edge happens to be processed first. (Finding YA-10.)
- Yang §4.3.4 curvature-based subdivision (h, l, α conditions) is referenced in prior audit (yang_2025_audit.md step 9) but I did not verify the actual h<𝑑𝑝×10², l<𝑑𝑝×10³, α<π/18 thresholds appear in `intersection_opt.rs`. Out-of-slice.

---

### Stage 4a — Mesh updating / re-meshing (Yang §4.4.1)

> Yang p. 970–987: "Therefore, in this step, we trim and update the meshes using the intersection curves to maintain a correct topology, bijectivity with the corresponding surfaces, and the 𝑑𝜖 constraints. The intersection curves on the parametric surfaces are mapped to the meshes 𝑀𝐴 and 𝑀𝐵 by mapping each intersection point r to r𝐴 ∈ 𝑀𝐴 and r𝐵 ∈ 𝑀𝐵, respectively. Then we set r𝐴 = r𝐵 = r, so that the two polylines in the meshes coincide with the intersection curve on the Bézier surfaces. Next, through CDT we obtain valid discretizations of the trimmed meshes […]"

**Status**: PRESENT (basic structure) — operates on the post-arrangement TopoArena, NOT on the parametric `(u,v)` mesh.

**Code site**:
- `yang_integration.rs:865-870` — calls `update_mesh_along_refined_curves`.
- `ssi_refinement.rs:292-…` — body. Uses `triangulate_single_triangle` from `mesh_arrangement.rs`.

**Conformance assessment**:
- The wiring (run AFTER refine_vertex_positions, AFTER labeling, AFTER patch segmentation, AFTER B-Rep build) is the correct ordering relative to Yang §4.4.1. But Yang's `r_A = r_B = r` step happens BEFORE the inside/outside classification (in Yang's flow §4.4 as a whole, "Mesh updating" is §4.4.1 BEFORE "Mesh and B-Rep Booleans" §4.4.2). Our code does it in the opposite order:
  - We ray-cast → patch-segment → BUILD B-REP → THEN re-triangulate the B-Rep faces along refined curves.
  - This means the labeling decisions in `label_cells` were made with the *unrefined* mesh-approximate intersection vertices, not Yang's refined surface-exact vertices. (Finding YA-11.)
- Yang §4.4.1 Fig. 11 explicitly shows mesh trimming in the **parametric domain** (panels (a)/(b)/(c)). Our `update_mesh_along_refined_curves` operates in **3D world coordinates** on the TopoArena. The bijectivity-preserving CDT in u,v space is not actually in our pipeline. Auditor-d coverage.

---

### Stage 4b — Inside/outside classification (Yang §4.4.2 ➝ Cherchi 2022 §4.4.2 / §5)

> Yang p. 988–995: "**Mesh Booleans.** After trimming the meshes using the intersection curves, we directly apply a standard inside/outside classification step [Cherchi et al. 2022] to identify the triangles that need to be retained, thus completing the mesh Boolean operation."

> Cherchi 2022 §5 (line 388-395): "Differently from the arrangement step, which is an amelioration of an existing technique, the computation of the inside/outside classification is entirely different from the topological approaches used in prior art. Our key insight is that the inside/outside relationship between **a patch** and an input mesh M can be determined by **casting a ray from any patch point** along an arbitrary direction […]"

> Cherchi 2022 Algorithm 1: "for each patch P do … Define a ray r starting at point p ∈ P … for each input mesh M do … find intersecting triangle t ∈ M … compute volume of tetrahedron …"

**Status**: PARTIALLY PRESENT / DEVIATES — labeling is per-sub-triangle, not per-patch.

**Code site**:
- Entry: `topology_extract.rs:1649` — `label_cells(&subdivided, …)`.
- Body: `exact_mesh.rs:1788-1879` (`label_cells`) — loops over EVERY `sub_tri` in `subdivided.tris_a` and `subdivided.tris_b`, calling `label_sub_tri_raycast` once per sub-triangle.
- Inner `ray_cast_inside`: `exact_mesh.rs:1320-1491` — first-hit signed-volume orientation per Cherchi 2022 §5.3 (the prior `cherchi_port_audit.md` D-05 finding has been fixed; this is the corrected version).

**Conformance assessment**:
- **Algorithm-1 patch-level labeling is NOT implemented.** Cherchi 2022 §5 is explicit: "the algorithm scales with the number of patches in the arrangement and not with the number of triangles in the mesh" (line 396-398). We label PER sub-triangle, which is strictly more work and doesn't enjoy Algorithm 1's scaling guarantee. (Finding YA-01, HIGH leverage.)
- The first-hit signed-volume orientation in `ray_cast_inside` (L1374-1446) IS faithful to Cherchi 2022 §5.3 / `booleans.cpp:1290-1300` — `res < 0.0` ⇒ Inside. Comment at L1370-1373 cites the C++ source. ✅
- Slab-eps (`TAU_EXACT_MESH_SLAB_EPS`) at `ray_cast_inside:1330` is a leftover from cherchi_port_audit D-09. Cherchi 2022 §5.2 uses tight zero-extent ray AABB. (Finding YA-02 / cross-ref C-D-09.)
- `label_cells` calls `weld_mesh_vertices` on both target meshes (`exact_mesh.rs:1804-1805`). The prior audit D-10 flagged this as A15.6 violation: tolerance-escalation as a workaround for non-watertight tessellation. Still present. (Finding YA-03 / cross-ref C-D-10, status: pending.)
- The cosurface short-circuit in `label_sub_tri_raycast:1553-1577` introduces a Hoffmann-style "AntiParallel ⇒ Inside / Parallel ⇒ A-side outside, B-side inside" rule that is NOT in Cherchi 2022 nor in Yang. It's grafted from Hoffmann 1989 §5.3 cosurface annihilation and propagated from the arrangement's STAGE2 dedup (cherchi_port_audit A-04). Not Yang-faithful, but not a regression; documented inline. (Finding YA-12.)
- The Hoffmann perturb-and-classify fallback (`label_sub_tri_raycast:1614-1672`) for centroids exactly on the surface is also a non-Yang grafted technique. The prior audit (D-07) flagged this as deviating from Cherchi's 8-offset `nextafter` perturbation cascade. Status: pending. (Cross-ref.)
- **Boundary case where this matters**: if a patch consists of multiple sub-triangles where some get labeled Inside and others Outside (because of the Hoffmann fallback boundary-coincident handling), the resulting `FaceSurvivalMap` will mix labels within a single source-face group, then `flood_fill_patches` will produce inconsistent-label patches. Cherchi 2022 §5 prevents this by labeling per patch, after segmentation. (See cross-stage observation §3.A.)

---

### Stage 5 — Patch segmentation / flood-fill (Yang §4.4.2 / Cherchi 2022)

> Yang p. 999–1008: "Our algorithm segments the mesh Boolean results into patches along the boundary curves, which correspond to either the original boundary curves or the intersection curves. Starting from an inner triangle, i.e. not on the boundaries of each mesh patch, using it as a seed triangle for the patch, our algorithm expands the patch by including more neighboring inner triangles, until all the neighboring triangles of the patch are on the boundaries."

**Status**: PRESENT — diverges from Yang in the choice of barriers.

**Code site**:
- `topology_extract.rs:351-952` (`flood_fill_patches`).

**Conformance assessment**:
- Step 5 (L472-521) flood-fill stops at intersection edges (cross-mesh) and exposed edges. Same-mesh source-face boundaries are NOT barriers. Comment at L472-476: "Per Yang, flood-fill stops only at intersection edges (cross-mesh boundaries), NOT at same-mesh source-face boundaries."
- This reading of Yang is questionable. Yang says "boundary curves, which correspond to either the original boundary curves or the intersection curves" (L1001-1003). "Original boundary curves" includes the original B-Rep edges between adjacent faces of the SAME solid — these ARE same-mesh source-face boundaries. Our flood-fill ignores them, then re-splits in Step 5a (L523-592). This works algorithmically but doesn't match Yang's text. (Finding YA-06.)
- The Step 5a connected-component refinement (L529-592) DOES use `boundary_edges` (which include same-mesh source-face boundaries) as barriers. Net result: a two-pass scheme that is functionally equivalent to "use boundary_edges as barriers from the start" but more code.
- Step 5a uses `directed_edge_to_tris.get(&(v1, v0))` — the *reverse* edge — for adjacency (L573). For non-manifold input or post-arrangement non-manifold patches, this picks up at most one neighbor per reverse edge in the multi-value `Vec`, so only the first reverse-tri gets queued. Subsequent reverse triangles in the same `Vec` are missed by `comp` if they aren't reached via another path. (Finding YA-07.)
- "Inner triangle" seed selection (Yang §4.4.2 line 1000-1002 "starting from an inner triangle") is NOT implemented. Our flood-fill seeds from any unvisited triangle in `0..all_tris.len()` order (L484). For boundary-touching patches, the seed is on the boundary, which is fine algorithmically but contradicts Yang's text. (Finding YA-08, low impact.)
- After flood-fill, twin pairing (L808-917) is a 1:1 deterministic pairing. No greedy fallback is used (PR3 removed it per the comment at L809-812). Unpaired/ambiguous edges are diagnosed (L876-915) and counted, but the function still returns a topology with unpaired half-edges — `validate_yang_result_topology` (`yang_integration.rs:979-1131`) is what rejects them downstream. (Cross-ref §3.B.)

---

### Stage 6 — B-Rep assembly + retessellation (Yang §4.4.2 / §4.5)

> Yang p. 1004–1009: "All the patches are found if all the triangles in the mesh Boolean results are accessed. The B-Rep model Boolean operations are finalized by restoring the corresponding parametric surfaces and boundary curves of them."

**Status**: PRESENT, with mesh-side retessellation.

**Code site**:
- B-Rep build: integrated into `flood_fill_patches:687-952` (Step 7 onwards inside the same function). Half-edges, edges, twin pairing, vertex sharing all done here.
- Solid → BooleanResult conversion: `yang_integration.rs:165-251` (`result_topology_to_waffle_solid`).
- Validation gate: `yang_integration.rs:886-890` (`validate_yang_result_topology`).
- Retessellation at Render LOD: `yang_integration.rs:899-901` — builds a fresh tessellation of the validated B-Rep at 64-segment resolution.

**Conformance assessment**:
- The "restoring the corresponding parametric surfaces" part is `result_topology_to_waffle_solid` at `yang_integration.rs:202-225`. **CRITICAL GAP**: it computes face geometry via Newell normal + centroid for EVERY result face, ignoring the `surface_map` (face provenance) entirely. Comment at L196-201 states "every face has geometry (enabling chained booleans)" and "source surface_map geometry may have wrong orientation after boolean".
- This means EVERY result face becomes `SurfaceGeom::Planar` regardless of whether its source face was Cylindrical/Spherical/Conical/Toroidal. Yang's whole "preserve analytical surfaces through the pipeline" claim (Yang §4.4 last paragraph) is silently violated — analytical surface tier is lost on every Yang boolean output. This also violates A15.5 (surface tier preservation) and A15.6 ("analytical surfaces preserved, only re-trimmed"). (Finding YA-13, HIGH leverage.)
- The validation at L979-1131 rejects partial topology (any boundary HE) per P9. This is correct.
- The retessellation at L900 uses `tessellation::TessellationLod::Render` (64 seg), but since all faces are now Planar, the render tessellation just triangulates flat polygons — there's no curvature to refine. The reason given for retessellation in the comments (L893-897, "16-segment Boolean LOD has chord error on curved surfaces") is moot when surface geometry is gone. (Finding YA-14.)
- For Subtract op, B's surviving sub-triangles get `flipped: true` (`topology_extract.rs:1273`). When `flood_fill_patches` rebuilds these (L410-413: `[tri.verts[0], tri.verts[2], tri.verts[1]]`), the directed-edge keys used for twin pairing flip too. This is necessary for outward-normal correctness but means twin-pairing has to operate post-flip. The implementation is consistent — observed working.

---

## §2 Findings list

Severity legend: **CORRECTNESS-BUG** (silent wrong result), **UNKNOWN** (concrete construction needed), **DEVIATES** (paper-vs-code mismatch with paper-cite), **PERFORMANCE-DRIFT** (slower but correct).

### YA-01 — `label_cells` labels per-sub-triangle, not per-patch
**Severity**: DEVIATES (potential CORRECTNESS-BUG via §3.A coupling).
**Code**: `exact_mesh.rs:1788-1879`, `topology_extract.rs:1649`.
**Paper-cite**: Cherchi 2022 §5 / Algorithm 1 (line 388-398, 332-336): "scales with the number of patches in the arrangement and not with the number of triangles". Yang §4.4.2 line 991-994 cites Cherchi 2022 directly for this step.
**Severity test**: A patch where Hoffmann boundary-coincident handling labels 3 of 5 sub-triangles Inside and 2 Outside; flood-fill assembles them into one patch but `face_survival_detect` filters by op (Union: keep Outside) → only 2 of 5 sub-tris survive, producing a holey patch.
**Fix direction (do not implement)**: After `flood_fill_patches`, label each PATCH by casting one ray from a single seed point, then update `FaceSurvivalMap` to filter by patch-label not sub-tri-label.
**Cross-ref**: cherchi_port_audit D-12, status: pending. (Yang seed-preference for "inner triangle" in §4.4.2 ties in.)

### YA-02 — `slab_eps` ray-AABB expansion
**Severity**: PERFORMANCE-DRIFT (correctness risk if mis-tuned).
**Code**: `exact_mesh.rs:1330` — `crate::units::TAU_EXACT_MESH_SLAB_EPS`.
**Paper-cite**: Cherchi 2022 §5.2 line 425-429: "Both the octree and the ray are axis aligned […] the bounding box of the ray is tight (it's the ray itself)".
**Cross-ref**: cherchi_port_audit D-09, status: pending. Vanishes if D-05 (already done) + D-10 (still pending) are both completed.

### YA-03 — `weld_mesh_vertices` violates A15.6
**Severity**: CORRECTNESS-BUG (governance-level: A15.6 explicit violation).
**Code**: `exact_mesh.rs:1804-1805` (called twice in `label_cells`); body at `exact_mesh.rs:1735+`.
**Paper-cite**: Yang §4.5.5 (and A15.6 in repo): coplanar preprocessing should be the source of identical meshes, not a downstream tolerance fix. Cherchi 2022 §4 "Cached Predicates" uses indirect predicates so the question of vertex-identity tolerance never arises.
**Severity test**: Two operands with `0.5 * 1/QUANT_NANOMETER_SCALE` separation: `weld_mesh_vertices` collapses into one mesh, ray-cast classification treats them as joined.
**Cross-ref**: cherchi_port_audit D-10, status: pending.

### YA-04 — `split_face_along_boundary` silently drops vertices
**Severity**: CORRECTNESS-BUG.
**Code**: `coplanar_preprocess.rs:530-536` (the `if !found_on_edge` branch increments `COPLANAR_VERTS_DROPPED` and continues).
**Paper-cite**: Yang §4.5.5 / Fig. 16(c)(d): the overlap polygon vertices are intersection points and MUST be inserted into both faces' B-Rep boundaries. Dropping a vertex means subsequent `mef` calls will not pair correctly across the boundary, producing a non-conformal split.
**Severity test**: Two coplanar faces where the overlap polygon has a vertex on a face vertex of A but B's face has a slightly larger boundary. The vertex matches A's L487 path but for B's face, has no nearest existing vertex AND no nearby edge — silently dropped. Subsequent `mef` constructs an incomplete polygon.
**Fix direction**: Replace the `else { COPLANAR_VERTS_DROPPED.fetch_add(...) }` with a hard error or `debug_assert!`.

### YA-05 — Coplanar overlay: only first contour and first group consumed
**Severity**: CORRECTNESS-BUG (latent).
**Code**: `coplanar_preprocess.rs:249-265, 271, 337` — uses `overlap[0][0]` only; `overlap` is `Vec<Vec<Vec<[f64;2]>>>` so `overlap[i]` is a disjoint group, `overlap[i][0]` is its outer boundary, `overlap[i][k>0]` is its k-th hole.
**Paper-cite**: Yang §4.5.5 implicit — applies to ANY i_overlay output (a face with holes, a multi-region intersection).
**Severity test**: Two coplanar L-shaped faces whose intersection is two disjoint rectangles → `overlap.len() == 2`. We process `overlap[0][0]` only. The second rectangle is silently lost, B-Rep split is incomplete. Counter `COPLANAR_OVERLAY_GROUPS` measures this but doesn't error.
**Fix direction**: Iterate all `overlap[i][k]` and treat each as a separate boundary.

### YA-06 — Yang §4.4.2 "boundary curves" interpretation in flood-fill
**Severity**: DEVIATES (refined to PERFORMANCE-DRIFT — Step 5a fixes the result).
**Code**: `topology_extract.rs:472-521` Step 5 vs L523-592 Step 5a.
**Paper-cite**: Yang §4.4.2 line 1003: "boundary curves, which correspond to either the original boundary curves or the intersection curves". Our Step 5 only respects intersection curves; Step 5a adds the source-face boundaries.
**Fix direction**: Combine `intersection_edges` + cross-source-face edges into a single barrier set; remove Step 5a.

### YA-07 — Step 5a connected-component BFS picks at most one neighbor per directed edge
**Severity**: UNKNOWN.
**Code**: `topology_extract.rs:573-580`. `directed_edge_to_tris.get(&(v1, v0))` returns a Vec of all reverse-edge tris; the inner `for &ni in neighbors` loop SHOULD enqueue all of them, but the check `tri_set.contains(&ni)` filters to only same-source-face triangles.
**Paper-cite**: Yang §4.4.2 / Mantyla §4.2 — for non-manifold input (post-arrangement multi-tri-per-edge), correct connected components require iterating ALL same-source neighbors.
**Severity test (concrete construction needed)**: Three same-source-face triangles meeting at a single intersection edge in the SAME direction. After Cherchi STAGE2 dedup, parent labels could allocate them all to the same source face; Step 5a's `tri_set` would include all 3, but only one gets queued from the v1→v0 reverse edge.
**Fix direction**: Audit edge multiplicity; verify the inner loop processes every Vec entry. The code looks like it does (L575) but I didn't trace a non-manifold construction.

### YA-08 — Yang "inner triangle" seed preference not implemented
**Severity**: DEVIATES (low impact).
**Code**: `topology_extract.rs:484` — seed = first unvisited in `0..all_tris.len()`.
**Paper-cite**: Yang §4.4.2 line 1000-1002: "Starting from an inner triangle, i.e. not on the boundaries of each mesh patch, using it as a seed triangle".
**Cross-ref**: cherchi_port_audit D-12, status: pending. Composes with YA-01 (per-patch ray-cast) — both need patch-aware vertex/triangle tracking.

### YA-09 — SSI refinement errors silently swallowed
**Severity**: CORRECTNESS-BUG (A15.6 governance).
**Code**: `yang_integration.rs:803-833`.
**Paper-cite**: A15.6 + Yang §4.3 — SSI refinement IS the analytical surface preservation step. Falling through to "mesh-derived geometry" with empty `EdgeRefinementMap` means analytical accuracy is lost for that operation.
**Severity test**: Construct a cyl-cyl boolean where one face's classification fails (returns Err). Compare result against analytical SSI: result will use mesh-approximate intersection edges, surface tier degrades.
**Fix direction**: Per P9, refinement errors should propagate. The `eprintln!("[A15.6 WARN] …")` is the symptom of a silent regression-to-mesh.

### YA-10 — `refine_vertex_positions` ordering-dependent for shared vertices
**Severity**: UNKNOWN (potential CORRECTNESS-BUG at corners).
**Code**: `ssi_refinement.rs:258-278`.
**Paper-cite**: Yang §4.3 — at a corner where 3+ faces meet, the vertex lies on the intersection of 3+ surface pairs. Closest-point projection to ANY single curve is generally not on the others.
**Severity test**: A box+box+sphere triple boolean creating a vertex shared by 3 SSI curves. Whichever curve is processed first in `refinement.edges` BTreeMap iteration order wins. Yang §4.5.1 corner-point handling (Fig. 13) prescribes Newton's method with multi-surface constraint, not single-curve projection.
**Fix direction**: Detect multi-curve vertices; use Newton on combined constraint as Yang Appendix C describes.

### YA-11 — Mesh updating runs AFTER labeling, not before
**Severity**: DEVIATES.
**Code**: `yang_integration.rs:712-731` (label_cells via yang_boolean_pipeline) vs `yang_integration.rs:865-870` (update_mesh_along_refined_curves).
**Paper-cite**: Yang §4.4 sub-section ordering: §4.4.1 "Mesh updating" then §4.4.2 "Mesh and B-Rep Booleans" (which contains the inside/outside classification). Yang's ordering is: refine → trim mesh along refined curves (now bijective again) → run inside/outside classification. Ours: classify → flood-fill → trim mesh.
**Severity test**: A sub-triangle whose vertex is mesh-approximate and projects to a refined position 5𝑑𝜖 away. Our labeling uses the unrefined vertex; if 5𝑑𝜖 is enough to flip the inside/outside relationship at a near-boundary case, the survival decision is on the wrong topology.
**Fix direction**: Swap order — run `refine_vertex_positions` + `update_mesh_along_refined_curves` BEFORE `label_cells`.

### YA-12 — Cosurface short-circuit not in Yang
**Severity**: DEVIATES (deliberate Hoffmann graft, citation present).
**Code**: `exact_mesh.rs:1553-1577`.
**Paper-cite**: Hoffmann 1989 §5.3 (cosurface annihilation), grafted onto Yang/Cherchi. Cited inline.
**Note**: Working as intended per cherchi_port_audit A-04/D-04. This is a documented deliberate divergence; flagging only so that future audits don't re-find.

### YA-13 — Result faces ALL planar, surface tier lost (HIGH leverage)
**Severity**: CORRECTNESS-BUG (A15.5 + A15.6 violation).
**Code**: `yang_integration.rs:202-225` (`result_topology_to_waffle_solid`).
**Paper-cite**: Yang §4.5 (B-Rep Boolean) — "the corresponding parametric surfaces". A15.5 (surface tier preservation): "When a face passes through a boolean operation without being split by an intersection curve, it retains its original `SurfaceGeom` variant". A15.6: "analytical surfaces preserved, only re-trimmed".
**Severity test**: Box minus cylindrical hole. Source faces include 6 planar caps + 1 cylindrical side. Yang result should have 6 planar + 1 cylindrical (re-trimmed). Our output: ALL planar (the cylindrical side becomes a faceted approximation with N planar facets equal to tessellation segment count). Chained booleans on this result lose all SSI dispatch opportunities and degrade further.
**Fix direction**: Change `result_topology_to_waffle_solid` to look up `surface_map.get(&(provenance.mesh_id, provenance.face_idx))` and propagate the `SurfaceGeom` for every face whose provenance is unmodified. Only synthesize Newell-normal Planar geometry for genuinely new intersection-derived faces.

### YA-14 — Render LOD retessellation argument moot under YA-13
**Severity**: PERFORMANCE-DRIFT (waste).
**Code**: `yang_integration.rs:899-901`.
**Paper-cite**: N/A — internal optimization. Comment at L893-897 cites "16-segment Boolean LOD has chord error on curved surfaces". Without curved surfaces (YA-13), 64-seg retessellation provides no chord-error benefit.
**Note**: Resolves naturally with YA-13 fix.

### YA-15 — Coplanar B-Rep splitting is pre-tess but mesh injection is post-tess
**Severity**: DEVIATES.
**Code**: `yang_integration.rs:573-687` (3-phase: split → tess → inject).
**Paper-cite**: Yang §4.5.5 line 1284-1286: "it is necessary to check coplanar planes and perform 2D Boolean operations BEFORE mesh discretizations".
**Functional impact**: T-junctions at the boundary between the canonical overlap region and adjacent face triangulations require `repair_tjunctions_after_injection`, which is a single-pass single-pair fix. Yang's pre-tessellation prescription makes T-junctions structurally impossible.

### YA-16 — Per-pair coplanar T-junction repair doesn't cascade
**Severity**: CORRECTNESS-BUG (latent).
**Code**: `coplanar_preprocess.rs:1042-1047` and `:1263-1264`.
**Paper-cite**: Yang §4.5.5 + Fig. 24(b) (24 coplanar elliptical cylinders). If multiple coplanar pairs share boundary vertices, repair after pair 1 is invalidated by pair 2.
**Severity test**: 3+ coplanar pairs sharing a single chain of edges; `repair_tjunctions_after_injection` is called per-pair sequentially; later pairs introduce new T-junctions in faces fixed by earlier pairs.

### YA-17 — `is_complete()` bijective guard masks freeform/NURBS gracefully
**Severity**: DEVIATES (low impact while NURBS unsupported).
**Code**: `yang_integration.rs:646-650`.
**Paper-cite**: Yang §4.1.1 — "[…] each closed B-Rep model composed of multiple surface patches into a triangle mesh" applies to NURBS too. We early-return NotSupported. Acceptable per repo's NURBS limitations memo, but contradicts yang_2025_audit.md step 1's "CORRECT*" note.

### YA-18 — `dedup_mesh_vertices` is per-operand only
**Severity**: UNKNOWN.
**Code**: `yang_integration.rs:620-621`.
**Paper-cite**: Yang §4.5.5: identical-footprint coplanar pairs need IDENTICAL vertex bits in the overlap region. Per-operand dedup at nanometer scale is independent for A and B; coincident vertices on the shared overlap will dedup to whatever bit-pattern each pre-dedup mesh happened to produce.
**Severity test**: Coplanar pair where A has 3 vertices in the overlap region at slightly different bit-patterns from B's 3 (e.g., (0.5, 0.5, 0.0) computed as 0.5⁄1.0 in A and 1⁄2 in B — both rationally 0.5 but possibly differing by 1 ULP after some computation). Dedup quantizes both to the same nm grid → same `[i64;3]` key → same dedup-target — so this finding may already be defended against by quantization. Unknown without a concrete construction.

### YA-19 — `validate_yang_result_topology` warns but accepts manifold violations when no boundary HEs
**Severity**: DEVIATES.
**Code**: `yang_integration.rs:1101-1119`.
**Paper-cite**: Mantyla §4.2 + Yang §4.4.3 — watertight inheritance from mesh boolean output requires manifoldness. The L1106 path emits "manifold warning: {n_he} half_edges != 2 * {n_edges} edges" but proceeds. If `n_he != 2 * n_edges` AND `n_boundary_he == 0`, the topology contains an edge with !=2 half-edges but no detection-friendly self-twin — likely an internal duplicate. Continuing produces a result that fails downstream.
**Severity test**: Construct a Yang result where two patches share an edge in the SAME direction (cosurface annihilation degenerate case); twin pairing leaves `n_he = 2*(n_edges - 1) + 2*1 = 2*n_edges` BUT one edge has 2 same-direction HEs and 0 reverse. The check `n_he == 2*n_edges` passes but topology is broken.
**Fix direction**: Add `assert n_he == 2 * n_edges`; treat as failure not warning.

### YA-20 — `count_connected_components` walks via twin without manifoldness verification
**Severity**: UNKNOWN.
**Code**: `yang_integration.rs:935-971`.
**Paper-cite**: Mantyla §4.2 — connected-components walking assumes valid twin pointers. After YA-19's "warning, not error" path, twins may not be reliable.
**Note**: Used only for the Euler characteristic check in `validate_yang_result_topology` (L1110). The Euler check itself is then a warning (L1113-1118), so this is double-soft.

### YA-21 — `extract_planar_faces` plane-membership tolerance is `100 × TAU_MODEL`
**Severity**: PERFORMANCE-DRIFT (correctness risk if mis-tuned).
**Code**: `coplanar_preprocess.rs:151`: `if dist.abs() > TAU_MODEL * 100.0`.
**Paper-cite**: A15.6 / Yang — coplanar detection should use the predicate kernel's tolerance, not a magic 100× scaling. Specific scenarios where this matters: vertex slightly off-plane due to sketch_solver round-off (~1e-9 m for an extruded face).
**Fix direction**: Use `TAU_MODEL` directly OR justify the 100× via inline comment.

### YA-22 — Empty topology fast-path returns empty BooleanResult, may be wrong for Subtract
**Severity**: UNKNOWN.
**Code**: `yang_integration.rs:838-854`.
**Paper-cite**: Yang Boolean ops table — `A − ∅ = A`, `∅ ∩ B = ∅`, `A ∪ ∅ = A`. The empty-topology branch returns empty for ALL ops.
**Severity test**: Construct a case where `pipeline_result.topology.face_provenance.is_empty()` but the input was not actually empty (e.g., labeling errors all sub-tris as Inside for Union → no survivors → empty topology). For `Subtract`, this should be A unchanged, not empty.
**Note**: This branch is AFTER the AABB-disjoint short-circuit at `topology_extract.rs:1509-1528`, so disjoint inputs don't hit it. But mid-pipeline labeling failures could.

### YA-23 — `yang_pipeline_result_for_disjoint` Union builds a dual-shell result with empty face groups
**Severity**: UNKNOWN.
**Code**: `topology_extract.rs:1361-1473`.
**Note**: Disjoint Union should produce two-shell B-Rep. The function uses `label_cells` on the combined mesh (which sees both originals → all sub-tris labeled Outside), then `face_survival_detect` (Union: keep Outside) → all survive → `flood_fill_patches`. Probably correct, but I did not trace through the multi-shell B-Rep assembly. Cherchi 2022 §6.4 supports multi-shell variadic ops; flagging for a separate trace.

### YA-24 — `intersect` on disjoint short-circuit returns empty `cached_render_mesh: None` (vs explicit empty)
**Severity**: UNKNOWN (low impact).
**Code**: `topology_extract.rs:1372-1391` returns `subdivided` with empty arrays but `remaining_failed_verts: 0`. The caller at `yang_integration.rs:838-853` produces a different empty-result struct. Two empty-result code paths.
**Note**: Cosmetic; the two paths agree on the main behavior. Just two places that need to stay consistent.

### YA-25 — `inject_partial_overlap_mesh` uses Earcut for triangulation; identical-footprint same
**Severity**: UNKNOWN.
**Code**: `coplanar_preprocess.rs:1281-1310` (`triangulate_polygon_with_holes` via earcutr crate).
**Paper-cite**: Yang §4.5.5 + Cherchi 2022 §4 cites Livesu et al. 2021 simplified linear-time CDT. Earcutr is O(n²) worst-case (cherchi_port_audit C-08 area). Yang requires "identical sampling points" but this is satisfied as long as we triangulate ONCE; the triangulation algorithm itself doesn't matter for correctness.
**Note**: Just flagging; not a correctness issue.

### YA-26 — d_epsilon retessellation refinement loop bypasses coplanar preprocessing
**Severity**: CORRECTNESS-BUG.
**Code**: `yang_integration.rs:733-774`.
**Paper-cite**: Yang §4.5.2 mesh refinement should preserve §4.5.5 coplanar guarantees. The MAX_REFINEMENT_ROUNDS loop re-tessellates from scratch but does NOT re-detect coplanar pairs nor re-call `inject_identical_footprint_mesh` / `inject_partial_overlap_mesh`. The `solid_a_mod` / `solid_b_mod` (post-B-Rep-split) are reused, so the B-Rep split persists, but the canonical overlap mesh injection at finer resolution never happens.
**Severity test**: A coplanar boolean that succeeds at d_epsilon=0.01 with identical-footprint injection, fails optimization, retries at d_epsilon=0.005 — second pass tessellates without injection, overlap region is not bitwise-identical between A and B, conformality is lost.
**Fix direction**: Either re-call inject functions in the refinement loop, or refactor to keep the canonical mesh in B-Rep space (not at tessellation level).

### YA-27 — `BoolOp` dispatch in mod.rs::boolean_op does NOT route to Yang
**Severity**: DEVIATES (intentional).
**Code**: `boolean/mod.rs:897-964` (`boolean_op`) goes straight to S-H clipping. Yang dispatch lives in `waffle_kernel.rs:1036-1101`.
**Paper-cite**: A15.6 — Yang is the target architecture. Routing via `do_boolean` (kernel-level) is correct; the polygon-clipping `boolean_op` is the legacy path. This is per the deprecation notice at `mod.rs:1-15`. Not actionable (deliberate) but flagging because the audit-scope description named `boolean/mod.rs` as a key file.

### YA-28 — `yang_boolean_inner` clones both solids unconditionally
**Severity**: PERFORMANCE-DRIFT.
**Code**: `yang_integration.rs:573-574` (`solid_a.clone()`, `solid_b.clone()`).
**Note**: Clone is required because `split_brep_for_coplanar_pairs` mutates the arena. Unavoidable without restructuring; flagging only.

### YA-29 — Coplanar `extract_planar_faces` validation rejects tilted planar faces silently
**Severity**: UNKNOWN.
**Code**: `coplanar_preprocess.rs:142-159` — `all_on_plane` check; if any vertex is more than `100 * TAU_MODEL` off the declared plane, the face is silently excluded from coplanar detection.
**Severity test**: A planar face whose `face_geometry` plane offset is wrong (e.g., from a chained boolean where Newell-normal recomputation introduced a 1e-7 m shift). This face is excluded from coplanar pair detection; if its actual plane IS coplanar with another solid's face, the pair is missed.
**Fix direction**: Either fail loudly (with diagnostic) or recompute plane from actual face vertices. Note the comment at L132-133 says "test solids with dummy normals" is the intent — but production recomputation could trigger this.

### YA-30 — Step 6 boundary loop closing does not handle disjoint loops correctly
**Severity**: UNKNOWN.
**Code**: `topology_extract.rs:643-684` (boundary chaining).
**Paper-cite**: Yang §4.4.2 / Mantyla — a single B-Rep face may have an outer loop AND multiple inner (hole) loops. The chaining at L650-678 picks any unprocessed start and walks; if the boundary contains two disjoint chains, we get two `loops` entries. But subsequent code at L729-802 builds **a separate B-Rep face per loop**, not one face with outer + inner loops.
**Severity test**: A planar face whose Yang-result is an annulus (square with a square hole). Outer + inner boundaries → 2 loops → 2 B-Rep faces, not 1 face with hole. Half-edge twin pairing across the inner boundary will then be cross-face (fine for connectivity) but the result B-Rep is topologically wrong (the annulus is a single face, not two).
**Fix direction**: Group loops by which face they bound (outer vs hole) before B-Rep face creation.

### YA-31 — Twin-pairing assumes 1:1 — fails for true non-manifold patches
**Severity**: DEVIATES.
**Code**: `topology_extract.rs:855-915`. The match arm `[the_one] => …` requires exactly one reverse candidate; `multiple` and `[]` paths just count ambiguous/unpaired without pairing.
**Paper-cite**: Mantyla §4.2 — open B-Rep allows half-edges with no twin (boundary). Yang/Cherchi-2020 ensure conformal so 1:1 is the post-condition. But our `flood_fill_patches` produces patches even when the underlying mesh has multi-edge non-manifold incidences (B-12 in cherchi_port_audit cluster). When that happens, twin pairing leaves these unpaired and the topology validation rejects.
**Note**: This is the documented PR3 design (no greedy fallback). Validation rejection is per-P9, which is correct. Just noting for completeness.

### YA-32 — `coplanar_preprocess::split_brep_for_coplanar_pairs` mutates BOTH solids' face_map
**Severity**: UNKNOWN.
**Code**: `coplanar_preprocess.rs:594` — `face_map.insert(next_face_id, new_face)`.
**Paper-cite**: Yang §4.5.5 — both solids' B-Rep should be split symmetrically. The face_map allocation uses `face_map.keys().max() + 1` per-solid (L562). If A and B's face_maps had overlapping ID ranges, no problem (one face_map per solid). Only flag if cross-solid id-disambiguation matters; I think it doesn't.
**Note**: Cosmetic.

### YA-33 — Stage 4b deadline check is per-100-sub-tri, not absolute
**Severity**: UNKNOWN.
**Code**: `exact_mesh.rs:1820-1829, 1851-1858` (deadline check every 100 sub-tris).
**Note**: Acceptable for typical inputs; for 500K-sub-tri meshes, deadline is checked 5K times — fine. For 50-sub-tri meshes, 0 checks → may run past deadline if BVH ray-cast is slow per-tri. Won't cause incorrect results, just sluggish timeout.

### YA-34 — `compute_vertex_params` happens BEFORE coplanar mesh injection
**Severity**: UNKNOWN (potential CORRECTNESS-BUG via SSI refinement).
**Code**: `yang_integration.rs:654-655` (params computed at original tess), then L661-687 (mesh injection replaces triangles).
**Paper-cite**: Yang §4.1.1 bijective mapping — every mesh vertex needs (u,v) on its source face. After `inject_identical_footprint_mesh` replaces triangles with new vertices from the i_overlay overlap polygon, those new vertices have NO bijective u,v param entry.
**Severity test**: SSI refinement on a coplanar boolean after injection — if `update_mesh_along_refined_curves` references `bijective.params_a[v]` for an injected vertex, panic or wrong projection.
**Fix direction**: Recompute `compute_vertex_params` after injection, OR populate params during injection using the analytical plane.

### YA-35 — `dedup_mesh_vertices` runs BEFORE coplanar injection
**Severity**: UNKNOWN.
**Code**: `yang_integration.rs:620-621` then L661-687. Dedup happens first; injection adds new vertices that may collide with dedup'd existing ones at nanometer precision but are NOT re-dedup'd.
**Note**: Probably benign because injection's `inject_face_with_shared_first` does its own snap-to-existing logic, but two layers of dedup at different stages is brittle.

---

## §3 Cross-stage observations

### §3.A — Per-sub-triangle labeling (YA-01) couples to flood-fill mixed-label patches

The most consequential cross-stage issue: `label_cells` labels each sub-triangle individually (Yang's literal text could read either way; Cherchi 2022's Algorithm 1 is unambiguously per-patch). Combined with the Hoffmann perturb-and-classify boundary-coincident handling (YA-12 cited deviation), a single patch can end up with mixed Inside/Outside labels. `face_survival_detect` then keeps only the Inside-or-Outside-as-appropriate sub-tris, producing a holey patch. `flood_fill_patches` Step 5a will treat the holes as boundaries, splitting one logical patch into multiple B-Rep faces. The downstream B-Rep is then over-fragmented.

This is the same hypothesis the cherchi_port_audit D-11 raised; YA-01 plus YA-12 plus YA-04/05 (coplanar dropped vertices feeding sub-tris where labeling can disagree) is the most likely root cause for the assay failures.

### §3.B — Validation gate (YA-19, YA-31) interacts with patch construction (YA-30)

The pipeline produces a topology that may have:
- Unpaired half-edges (YA-31 1:1 strict pairing) → validation rejects.
- Manifold-violating but boundary-free topology (YA-19 warning-only) → validation accepts, but downstream fails.
- Annulus-as-two-faces (YA-30) → validation passes, but B-Rep is semantically wrong.

The first case is the visible failure (P9-correct rejection), the latter two are silent errors that look like Yang correctness wins until someone tessellates the result.

### §3.C — Surface tier loss (YA-13) compounds across chained booleans

YA-13 is the highest-leverage correctness issue I found. Every Yang boolean output replaces analytical surface tier with `Planar`. A chained `box ∪ cyl − sphere` operation converts:
1. After step 1 (box ∪ cyl): cylindrical face → planar facets. SSI refinement step is skipped for that face (already mesh-derived).
2. After step 2 (… − sphere): sphere face → planar facets too.
3. Result: 100% planar faces, even though Yang's whole pitch (and A15.5, A15.6) is preservation of analytical surfaces.

This single fix (look up `surface_map` in `result_topology_to_waffle_solid`) would unlock A15.5/A15.6 compliance and make chained-Yang behave like the paper claims.

### §3.D — Coplanar preprocessing position (YA-15) creates downstream T-junction load (YA-16) that compounds with refinement-loop bypass (YA-26)

The "split B-Rep pre-tess, inject mesh post-tess" architecture is fragile in three ways: T-junctions appear (handled by single-pair repair only — YA-16), the refinement loop doesn't redo injection (YA-26), and bijective u,v params lag behind injection (YA-34). Yang's pre-tessellation prescription resolves all three structurally.

### §3.E — Newell-normal computation is the only face geometry signal post-Yang (YA-13 + YA-19 + YA-30)

Combined: result faces have synthesized `Plane` geometry, ambiguous manifold checks pass, hole faces become separate B-Rep faces. The output B-Rep looks valid by half-edge counts (12 / 8 vertices / 6 face Euler ⇒ 2 ✓) but the geometric content is wrong on multiple axes simultaneously. Fixing any one of these three is necessary but not sufficient to recover Yang correctness.

---

## §4 What this slice did NOT cover

Things I DEEPLY examined (line-by-line):
- `yang_integration.rs` lines 1-1255 (orchestrator: solid setup, tessellation routing, refinement loop, validation, retessellation).
- `coplanar_preprocess.rs` lines 1-1275 (detection, split, identical-footprint injection, partial-overlap injection).
- `topology_extract.rs` lines 1-1825 (`flood_fill_patches`, `face_survival_detect`, `yang_boolean_pipeline`, the disjoint short-circuit).
- `boolean/mod.rs` lines 1-1565 (BoolOp dispatch types, AABB fast-path; **NOT** the S-H clipping body, since per A15.6 it is deprecated and not on the Yang path).

Things I SPOT-CHECKED only:
- `exact_mesh.rs::label_cells` and `label_sub_tri_raycast` and `ray_cast_inside` — read in detail because Stage 4b labeling consumes them, but they are auditor-c's slice for Cherchi-port deviations.
- `ssi_refinement.rs::refine_vertex_positions` — read enough to understand vertex projection. The full body of `update_mesh_along_refined_curves` and `refine_intersection_edges` is auditor-d's tessellation thread.
- `intersection_opt.rs` — only verified function signatures match orchestrator call sites. Yang §4.3.1/§4.3.2/§4.3.3/§4.3.4 method-selection logic depth audit is auditor-d.
- `cherchi/*` — entirely cherchi_port_audit + auditor-c slice. All cross-references to that audit are in `[]` notation in findings.
- `tessellation::tessellate_solid_ext_with_lod` — black box from this slice.
- `mesh_arrangement.rs::triangulate_single_triangle` (used by `update_mesh_along_refined_curves`) — out of slice; cherchi_port_audit D-13 documented this is on the SSI refinement path.

Things I did NOT cover at all:
- Test files (`#[cfg(test)] mod tests`) in any of the audited files — the audit is on production code.
- `boolean/cherchi/*` (auditor-c).
- `boolean/intersection_opt.rs` body (auditor-d).
- `boolean/ssi_refinement.rs` body for §4.4.1 CDT correctness (auditor-d).
- `tessellation/` crate (auditor-d).
- Performance/scaling characteristics; I have NOT timed or profiled anything.
- Cherchi's actual `solve_intersections` (auditor-c).
- `wasm-bridge` interaction with Yang (out of scope for boolean correctness).

Severity I am most confident about:
- YA-01, YA-13, YA-15, YA-26 (paper-cite + clear paper deviation + concrete severity test).
- YA-04 (silent vertex drop) and YA-05 (silent contour loss) — these are obviously CORRECTNESS-BUG by §P9 (no hack-to-green).

Severity I am LEAST confident about:
- YA-07 (Step 5a non-manifold neighbor enumeration) — needs concrete construction.
- YA-22, YA-23, YA-30 — need specific failing assay cases or constructed inputs.
- YA-29 (silent coplanar-face exclusion at 100× tau) — depends on whether real face_geometry from chained booleans actually drifts that far.

Per `feedback_no_last_bug.md`: I'm not claiming any of these is "the last gap" or that fixing them all unblocks the Yang assay. I'm describing 35 places where what's in the code does not match what's in the paper or what the cross-stage data flow needs to make sense. The most likely high-leverage fixes are YA-13 (surface tier), YA-01 (per-patch labeling), YA-09 (refinement error swallowing), and YA-15+YA-26 (coplanar pre-tess vs post-tess architecture). Anything else may also matter.
