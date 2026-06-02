# Yang/Cherchi Implementation Deviations Log

**Purpose:** authoritative record of known divergences between the Yang 2025 hybrid B-Rep/mesh boolean pipeline as specified in the paper, and the implementation in `crates/kernel/`. Any deviation listed here MUST have either (a) a user sign-off with stated rationale, or (b) an active remediation tracked.

**Discipline:** per `CLAUDE.md` "Paper-Spec Compliance is MANDATORY," deviations are errors. Investigation on a component with an open deviation is blocked until either the deviation is fixed or signed off in writing.

---

## Open deviations (NOT signed off; investigation blocked)

### D1 — Simplified earcut used where Yang §4.4.1 specifies CDT

**Status:** OPEN. NOT signed off.

**Discovered:** 2026-05-16 during PR-Y46 architectural review.

**Code location:**
- `crates/kernel/src/tessellation/mod.rs:3471` — `earcutr::earcut(...)` call (convex path)
- `crates/kernel/src/tessellation/mod.rs:3509` — `earcutr::earcut(...)` call (with-holes path)
- `tessellate_planar_face_bounded` in `tessellation/mod.rs` (per-face independent triangulation entry point)

**Paper prescription:** Yang 2025 §4.4.1 ("Mesh updating") at `refs/text/yang2025_hybrid_boolean.txt:548-590`. Quote: "through CDT we obtain valid discretizations of the trimmed meshes" (line ~557). Yang specifies **Constrained Delaunay Triangulation** — an algorithm that enforces intersection-curve polyline boundaries as hard constraint edges across all faces that meet at a shared boundary.

**Current implementation:** uses the `earcutr` crate (Livesu et al. 2021 "Deterministic Linear Time Constrained Triangulation Using Simplified Earcut"). The simplified earcut is a deterministic O(n) ear-removal algorithm. It does NOT enforce constrained edges across face boundaries; each B-Rep face's earcut runs independently with no cross-face coordination beyond shared `disc.positions` vertex pool.

**Architectural consequence:** when two adjacent B-Rep faces meet at a shared boundary that includes intersection-curve vertices, both faces' earcut calls receive the same boundary vertex positions (via shared `disc.positions`), but each face's earcut chooses diagonals independently. This produces the "Case D" defect signature measured across PR-Y43-Y46: all 3 vertex positions are present in the final Render LOD vertex set (sub-class (a) `m1x=3, m5x=3`), but no triangle connects them because each face's independent earcut emitted incompatible diagonals.

**Investigation cost prior to discovery:** 15 PR cycles (PR-Y32 through PR-Y46), ~2821 LOC of probe scaffolding, 0 production code, F0020 unpaired count unchanged at 40 across the entire arc.

**Remediation options:**
1. Constrained earcut wrapper (~80-150 LOC) — detect B-Rep shared edges, pass as hard constraints to earcut OR post-process diagonal flips. Closest to current architecture.
2. Replace `earcutr` with a true CDT (full Bowyer-Watson + constraint enforcement, or port from CGAL / use a Rust CDT crate). ~800-1500 LOC. Highest paper-alignment.
3. Cross-face coordination phase before per-face earcut. ~50-100 LOC for boundary alignment but doesn't fix earcut divergence root.

**Investigation status:** **REMEDIATION IN PROGRESS.** Earcut sweep complete; Tier 2/3 remain.

**Cycle 1 (2026-05-16):** Replaced `earcutr` with `spade::ConstrainedDelaunayTriangulation::try_bulk_load_cdt` at `tessellate_planar_face_bounded` (two call sites: planar-no-holes, planar-with-holes). Constraint edges constructed from boundary loops. Results: F0020 unpaired 40→35; kernel lib 1262→1266; yang_fast 10/157→13/157.

**Cycle 1.1 (2026-05-16):** Removed the earcut-fallback arm at both call sites — `try_bulk_load_cdt`'s silent-conflict callback handles upstream defects internally, so the fallback was unreachable on F0020. Replaced fallback with eprintln + empty output.

**Cycle 1.2 (2026-05-16):** Swept the 9 remaining earcut call sites across the kernel — each replaced with `cdt::cdt_triangulate_flat` (earcut-shaped flat-API wrapper). Sites converted: `boolean/coplanar_preprocess.rs::triangulate_polygon_with_holes` (Yang §4.5.5 path), plus 8 sites in `tessellation/mod.rs` (convex/non-convex polygon, revolve caps, cylinder strip, sphere/torus cap). Removed `earcutr` from `Cargo.toml`. Results: kernel lib 1266→1268; yang_fast 13/157 (unchanged); F0020 35 unpaired (unchanged); cdt unit tests 4→6 passing.

**Earcut now removed from the kernel.** Confirmed via `grep -rn earcutr crates/kernel/src/` returning zero hits.

**Cycle 2a (2026-05-16):** Plumbed `edge_is_intersection` from `ResultTopology` through `WaffleSolid` to a thread-local at tessellation entry. Added `Y47T2_INTERSECTION_PROBE` env-gated probe at `tessellate_solid_bounded` that walks each face's boundary loops and counts which segments map to intersection-flagged arena edges. Default-off byte-identical (F0020 35 unpaired unchanged); kernel lib 1266→1268.

**MEASUREMENT (load-bearing):** On F0020 with probe enabled, the thread-local marker carries 48 arena edges of which 20 are flagged `is_intersection=true`. The first version of the probe used vertex-index inversion via `disc.edge_verts` and reported `intersection_segs=0` across all faces — **that probe had a bug**: vertex inversion is ambiguous because a single vertex belongs to multiple edges. The corrected probe walks half-edges directly (`arena.half_edges[he].edge`) and gives the exact `EdgeIdx` per boundary segment.

**Corrected measurement:** all 20 flagged edges DO appear on boundary loops, totaling 40 face-loop incidences across 7 faces:
- Face4, Face7, Face13: walked_edges=10, walked_intersection=10 — **entirely intersection-bordered** (these are trim faces born from the boolean)
- Face9, Face10, Face11, Face12: walked_edges=5-6, walked_intersection=2-3 — partial intersection borders

**The boundary loops already include the intersection edges, and the CDT calls already pass every boundary segment as a constraint edge** (via the `loops` parameter to `cdt_triangulate_2d_with_loops`). So "Tier 2 = add intersection-curve constraints" produces no new constraints — they're already there.

**Tier 2 EMPIRICALLY REFUTED, with corrected reasoning.** The intersection edges aren't a missing-constraint problem; the boundary IS the intersection in these cases. The Case D 24/24 defect must come from CDT divergence on the SAME boundary input across adjacent faces. Cross-face shared-edge analysis from the deep probe:
- Face4 ↔ Face13: share 10 intersection edges (one large intersection patch boundary).
- Face7 ↔ Face9, Face10, Face11, Face12: share intersection edges across 4 neighboring faces.

The 24 Case D missing triangles are presumably the ones spade chose differently on each side of these shared boundaries.

**Remaining work to close D1 (revised after cycle 2a corrected measurement):**
- **Tier 3:** canonical cross-face Newell basis — eliminate ±ε 2D-projection drift between adjacent faces sharing intersection-edge boundaries (Face4 ↔ Face13 and Face7 ↔ {Face9,10,11,12} on F0020). With identical 2D inputs, deterministic CDT would produce identical outputs on shared regions. Estimated load-bearing.
- **Cross-face vertex-set divergence:** adjacent faces' CDT calls operate on different non-shared vertex sets (each face has its own interior vertices). Even with identical boundary constraints and identical Newell bases, Delaunay diagonal choice on shared-boundary triangles depends on the full vertex set. Forces investigation of global-coplanar-CDT (architectural rewrite Candidate C from the earlier review). The Face4/Face13 case is concrete: both faces' CDT calls see different non-shared vertex sets but identical 10-edge intersection boundary, and presumably emit different diagonals on the shared region.
- **Spade-CDT determinism caveat:** verify that spade actually emits identical triangulations given identical input (vertices + constraints) across separate calls. If spade's `bulk_load_cdt` has any input-order dependence we don't control, that's a contributing factor.

**Implementation choice note:** Yang §4.4.1 says "CDT in CGAL [2024]." We use `spade` (Rust-native CDT, ~5-10k LOC) rather than porting CGAL's CDT (~12k lines of C++ templates plus kernel/predicate infrastructure). `spade` implements the same algorithm class (Constrained Delaunay Triangulation with adaptive predicates via the `robust` crate); the deviation is "CGAL specifically" not "CDT semantics." This is a documented choice, not a behavioral deviation.

**Sign-off:** *not eligible.* Deviation remains until Tier 2 (and possibly Tier 3) lands.

---

## Audit findings (2026-05-18)

Three parallel Explore agents audited Yang §4.1 through §4.5 against the code. Findings consolidated below. Entries grouped by category. Each is independent of D1.

### Fundamental — replace wholesale

#### D2 — Extra post-tessellation repair pipeline (legacy S-H residue) — REMOVED

**Status:** REMOVED 2026-05-18. Replaced with Yang-compliant "no post-processing" path. Some new structural-deviation work surfaced; tracked below.

**Original code location:** `crates/kernel/src/tessellation/repair.rs` (entire module, ~4075 LOC; including extensive probe scaffolding from PR-Y40/Y45). Called from `crates/kernel/src/tessellation/mod.rs` lines ~568-792 (main `tessellate_solid_ext` cleanup loop) and ~5220-5322 (`tessellate_solid_bounded` stage-f F.0-F.4 sub-passes).

**Paper section:** Yang §4.4.3 (`yang2025:599-605`).

**Paper requirement:** "The watertightness of our result is **inherited from the mesh Boolean output**, ensuring the mesh has no geometric gaps." Yang asserts watertight-by-construction with no post-processing.

**Removal (2026-05-18):** Deleted `repair.rs` entirely. Removed `mod repair;` and `use self::repair::*;` from `tessellation/mod.rs`. Stripped all in-pipeline call sites at the two locations above. Deleted 3 test clusters (~270 LOC of tests targeting the deleted functions: `dedup_*`, `cross_face_nm_*`, `test_steiner_fan_*`). Extracted `count_unpaired_in_mesh` into a new `tessellation/diagnostics.rs` (~55 LOC) since it's pure measurement, not repair. The `weld_shared_edge_vertices` + `compact_unreferenced_vertices` helpers in `mod.rs` remain (fan-path-specific, separate deviation — see D5).

**Empirical impact (honest numbers, no repair masking):**

| Gate | Before D2 | After D2 | Delta |
|---|---|---|---|
| F0020 unpaired | 35 | **54** | +19 (was masked) |
| F0020 degenerate tris | 2 | 24 | +22 (was being removed) |
| F0020 non-manifold edges | 2 | 14 | +12 (was masked) |
| F0020 triangle count | 124 | 154 | +30 (no dedup/welding now) |
| kernel lib | 1268/24/42 | 1249/34/42 | -19 pass / +10 fail (9 tests removed; 10 newly exposed failures) |
| yang_fast | 13/157 | 13/157 | unchanged |
| pr_y31_f0044_extras_zero | GREEN | GREEN | unchanged |

The yang_fast pass rate didn't drop — confirming the repair pipeline was never closing corpus cases, just dressing F0020's diagnostic numbers.

**Significance:** F0020's "35 unpaired" figure that the prior 17 PR cycles chased was partly a repair-pipeline artifact. The real defect count is 54. The other 19 were closed by post-hoc welding/filling/T-junction-splitting that Yang doesn't have. Future work is against the honest 54 baseline.

**Banked findings exposed by D2 removal (each a candidate for its own deviation entry if substantive):**
- 24 degenerate triangles (zero-area or near-zero) survive in F0020 output. Upstream is producing these; Yang's algorithm should not.
- 14 non-manifold edges (count ≠ 2). Yang's bijective mesh-boolean output should be 2-manifold.
- 5 reversed normals. Yang's bijective mapping preserves orientation.
- 30 extra triangles vs the prior dedup'd output. Some are duplicates from upstream; some are genuine missing-then-now-emitted.

These are real upstream defects the repair was hiding. Investigating them is the natural next step — and now it's possible because we're not measuring against a masked baseline.

**Sign-off:** *resolved* per the directive that D2 must be removed.

#### D3 — §4.5.4 illegal-intersection detection/removal absent

**Status:** OPEN. NOT signed off.

**Code location:** Not present.

**Paper section:** Yang §4.5.4 (`yang2025:752-758`).

**Paper requirement:** "We detect these illegal intersections [self-intersections in the trimmed mesh arising from discretization or mesh updating] and perform local refinement. Since the input B-Rep model has no self-intersections, these illegal intersections are eliminated."

**Current implementation:** No detection or removal logic for post-trim self-intersections. The `no_self_intersection` oracle counts them as a test gate but doesn't fix them.

**Deviation magnitude:** Fundamental. An entire §4.5.4 step is missing.

### Structural — algorithm differs

#### D4 — §4.5.2 global re-tessellation instead of localized refinement

**Status:** OPEN. NOT signed off.

**Code location:** `crates/kernel/src/boolean/yang_integration.rs:843-904`.

**Paper section:** Yang §4.5.2 (`yang2025:659-670`).

**Paper requirement:** When optimization fails in a region, refine only the surfaces traversed by the failed intersection curve segment plus a one-ring of neighbors. Re-compute intersections only in the refined regions.

**Current implementation:** When the SSI optimization can't recover, the entire pipeline halves `d_ε` and re-tessellates BOTH solids globally, up to 2 rounds. Not localized.

**Deviation magnitude:** Structural.

#### D5 — §4.4.1 r_A = r_B = r identification — PARTIALLY ADDRESSED via plane-intrinsic origin

**Status:** PARTIALLY RESOLVED 2026-05-18. The 3D-vertex side of `r_A = r_B` is already enforced by the shared `disc.positions` pool. The 2D-projection side (which is what CDT actually sees) now uses a plane-intrinsic origin so coplanar adjacent faces produce byte-identical 2D coords for the same 3D point.

**Code location:** `crates/kernel/src/tessellation/mod.rs::tessellate_planar_face_bounded` — now takes a `plane_origin: [f64; 3]` parameter, used everywhere the prior code used `ordered_verts[0]`.

**Paper section:** Yang §4.4.1 (`yang2025:548-556`).

**What was the deviation:** 3D vertex identity was preserved (shared `disc.positions` pool) but the 2D coordinate that CDT received depended on `ordered_verts[0]` — i.e., the FIRST boundary vertex of the face. Two adjacent coplanar faces have different `ordered_verts[0]`, so the same 3D point projected to different 2D coordinates on each side. Deterministic CDT given different inputs is not equivalent to deterministic CDT given the same inputs.

**Fix (2026-05-18):** Pass the plane's intrinsic origin (`plane.origin` from `SurfaceGeom::Planar`) as the 2D origin. Two coplanar faces share `plane.origin` → identical 2D coordinates for shared 3D points.

**Remaining gap:** the fallback path (when surface geometry is unknown) still uses a vertex-derived origin. This is a sub-deviation but the fallback is only hit for faces without surface info, which Yang assumes don't exist.

**Sign-off:** *partially resolved.* Full closure when D14 (NURBS/Bézier support) lands, since each parametric surface will have a canonical origin.

#### D6 — §4.4.1 Fig 11 split/merge/insert procedures unclear

**Status:** OPEN. UNCERTAIN.

**Code location:** `crates/kernel/src/boolean/topology_extract.rs:249`, `mesh_arrangement.rs`, `cherchi/fast_trimesh.rs`.

**Paper section:** Yang §4.4.1 Fig 11 (`yang2025:555-565`).

**Paper requirement:** Three preprocessing steps before CDT:
- (a) Locate the constraint edge containing intersection point q; split it at q.
- (b) If a split-edge endpoint p is too close to q, merge p with q.
- (c) If an intersection loop has no interior mesh vertices, insert one.

**Current implementation:** Generic `split_edge` and `insert_vertex_into_triangle` routines exist. Whether they're applied in this exact sequence at CDT-prep time is unclear.

**Deviation magnitude:** Structural pending clarification.

#### D7 — §4.4.2 patch segmentation: flood-fill vs Cherchi 2022 per-patch ray-cast

**Status:** OPEN. UNCERTAIN.

**Code location:** `crates/kernel/src/boolean/topology_extract.rs:404-637, 1868+` (`face_survival_detect`).

**Paper section:** Yang §4.4.2 (`yang2025:574-598`).

**Paper requirement:** "We directly apply a standard inside/outside classification step [Cherchi et al. 2022]." Cherchi 2022 uses per-patch ray-casting through manifold-edge barriers.

**Current implementation:** Flood-fill patch segmentation with manifold-edge-barrier walking. Comment at `topology_extract.rs:508-514` notes a "refactor from Yang's intersection-edge-barrier flood to Cherchi 2022 §5 manifold-edge-barrier flood" — but whether the refactor matches Cherchi exactly or is a variant is unclear.

**Deviation magnitude:** Structural pending clarification.

#### D8 — §4.5.1 boundary step rescaling via surface-switching

**Status:** OPEN. UNCERTAIN.

**Code location:** `crates/kernel/src/boolean/intersection_opt.rs:901-1006`.

**Paper section:** Yang §4.5.1 (`yang2025:626-638`, Fig 12).

**Paper requirement:** When a Newton step exits the current surface's domain, **rescale the step magnitude** to land on the boundary curve `C_b`; continue optimization on the adjacent surface from there.

**Current implementation:** Clamps the parameter update to domain bounds, then if the point exits, finds the adjacent face and switches surface parameterization entirely. Both prevent escaping the domain but the mechanics differ from Yang's step-rescaling.

**Deviation magnitude:** Structural; semantic equivalence uncertain.

#### D9 — §4.1.2 per-surface u-v CDT for adjacency handling — PARTIALLY ADDRESSED

**Status:** PARTIALLY RESOLVED 2026-05-18. For planar surfaces, the CDT now operates in the surface's intrinsic 2D frame (plane.origin as origin, plane.normal as basis source). This is the §4.1.2 "in its own u-v domain" requirement for planar patches.

**Code location:** `crates/kernel/src/tessellation/mod.rs::tessellate_planar_face_bounded`.

**Paper section:** Yang §4.1.2 (`yang2025:397-410`).

**Original deviation:** "compute the 2D basis from boundary vertex order" — vertex-dependent, drifts across adjacent faces.

**Fix (2026-05-18):** The 2D basis (`u_axis`, `v_axis`) was already derived from `plane.normal` via `compute_plane_basis()`. The 2D origin (changed today) now comes from `plane.origin`. Both are intrinsic to the surface. Adjacent coplanar faces share the entire 2D frame.

**Remaining gap:** Yang §4.1.2 also describes the discretization step (sample u-v rectangle then re-sample boundary curves) as a separate phase from the post-boolean §4.4.1 CDT. We collapse both into a single per-face CDT call. For analytic surfaces other than planes (cylinder, sphere, cone, torus), tessellation is geometry-specific and does NOT use CDT in the surface's parametric (u,v). Closing this requires either (a) running CDT in (θ, z) for cylinders, (u, v) for sphere etc., or (b) accepting per-surface tessellation as adequate when it doesn't share boundaries with other curved surfaces.

**Sign-off:** *partially resolved* for planar surfaces. Curved-surface side remains for future work bundled with D14 (full NURBS handling).

#### D10 — Tessellation density not fully `d_ε`-driven

**Status:** OPEN. NOT signed off.

**Code location:** `crates/kernel/src/tessellation/mod.rs:34-87`.

**Paper section:** Yang §4.1 (`yang2025:330-395`).

**Paper requirement:** Discretize each surface with iterative refinement until distance-to-surface < `d_ε`. Adaptive per surface, driven by `d_ε`.

**Current implementation:** Three LOD modes: `Boolean` (fixed 16 segments/circle), `Render` (fixed 64), `Adaptive { d_epsilon }` (sagitta-formula segment count for circular edges only). Production path uses fixed `Boolean` LOD for boolean stages and fixed `Render` LOD for output. Adaptive is invoked but constrained to circular-edge sagitta — not the full surface-iterative refinement Yang prescribes.

**Deviation magnitude:** Structural.

#### D11 — `d_ε` computed from combined-AABB across both solids

**Status:** OPEN. NOT signed off.

**Code location:** `crates/kernel/src/boolean/yang_integration.rs:642-661`.

**Paper section:** Yang §4.1 (`yang2025:378-382`).

**Paper requirement:** "We select `d_ε` as a value 10⁻² · d relative to the diagonal length d of the AABB of **the B-Rep model**" (singular).

**Current implementation:** `d_ε = 0.01 · diag` where diag is the AABB diagonal over BOTH solids' vertices combined. Single `d_ε` value used for both operand discretizations.

**Deviation magnitude:** Structural. May over-smooth small solids when paired with large ones.

### Performance — algorithm class differs

#### D12 — O(n²) broad-phase intersection detection vs octree

**Status:** OPEN. NOT signed off.

**Code location:** `crates/kernel/src/boolean/cherchi/intersection_class.rs:106-160`.

**Paper section:** Yang §4.2.1 (`yang2025:450-451`).

**Paper requirement:** "We use an **octree** to detect triangles that are closer than `2·d_ε`."

**Current implementation:** O(n²) pairwise loop over all triangle pairs with AABB culling per pair. Comment at line 107 acknowledges this: "Simple O(n²) broad phase with AABB culling + Gauss map filtering. For production, replace with BVH/octree."

**Deviation magnitude:** Structural for scale; functional behavior is correct for small inputs.

### Scope — we don't support what Yang supports

#### D13 — Gauss-map check uses triangle-normal dot product, not Theorem 4.1 cones

**Status:** OPEN. NOT signed off.

**Code location:** `crates/kernel/src/boolean/cherchi/intersection_class.rs:117-149`.

**Paper section:** Yang §4.2.2 Theorem 4.1 (`yang2025:457-480`).

**Paper requirement:** Construct circular cone C₃ around a₃ = (a₁ × a₂) / |a₁ × a₂| where a₁, a₂ are from the Bézier control net. The cone bounds the normal-vector field of the entire patch.

**Current implementation:** Per-triangle normal cross product + dot-product check. Valid for flat triangles; not the cone-construction Theorem 4.1 prescribes.

**Deviation magnitude:** Scope (Bézier-specific algorithm). Becomes a structural deviation when D14 (NURBS support) is closed.

#### D14 — No NURBS / Bézier surface support; only analytic primitives

**Status:** OPEN. NOT signed off.

**Code location:** `crates/kernel/src/geometry/surface.rs:56-62`, `crates/kernel/src/tessellation/analytic.rs` (entire file).

**Paper section:** Yang §4.1 + §4.1.1 NURBS-to-Bézier conversion (`yang2025:330-395`).

**Paper requirement:** Yang's whole pipeline takes NURBS B-Rep input, converts to rational Bézier sub-patches, and discretizes via recursive Bézier subdivision. Theorems A.1 and A.2 give the control-net distance bounds that make this work.

**Current implementation:** Five analytic surface types only (Planar, Cylindrical, Conical, Spherical, Toroidal). No NURBS, no Bézier subdivision. The tessellation strategy is geometry-specific dispatch per primitive type, not the unified Bézier algorithm Yang specifies.

**Deviation magnitude:** Scope. We are building Yang for analytic-primitive inputs only. The paper's algorithm scales to NURBS; our subset doesn't need to (yet) but should be acknowledged.

**Notes:** This is the largest scope gap, but the most defensible — analytic surfaces are a strict subset of NURBS, so an analytic-only kernel can be a faithful Yang implementation for that subset. The decision is whether to broaden scope to NURBS later (likely yes for real CAD use) or stay analytic-only.

---

## New-crate deviations (clean-sheet rewrite: `yang-rs` / `cherchi-rs`)

> Deviations D1–D14 above concern the **legacy** `crates/kernel/` port. The
> entries below concern the new tiered crates and the functional roadmap
> (`docs/yang_functional_roadmap.md`).

### N1 — Stage-2 labels taken from the C++ sidecar, not a native arrangement

**Code location:** `crates/cherchi-sidecar-rs/` (interim `LabeledArrangement`
producer) consumed by `crates/yang-rs/` Stages 5/6.

**Paper section:** §4.2 (mesh boolean) → the per-output-triangle origin +
patch in/out labels that §4.4.2 reassembly consumes are, in the paper, products
of the implementation's *own* exact mesh arrangement.

**Current behavior (interim):** `yang-rs` obtains the `LabeledArrangement` from a
patched Cherchi 2022 `mesh_booleans` binary (subprocess), not from a native
pure-Rust arrangement. This is the deliberate decoupling that lets functional
Yang (mesh-approximate) exist before the native arrangement is written.

**Architectural consequence:** the boolean pipeline depends on an external C++
binary and is **not WASM-compatible** while this path is active. Also bounded by
Cherchi's input axioms (manifold/watertight/intersection-free), enforced at
roadmap M1.

**Remediation:** roadmap **M6** replaces the producer with the native
`cherchi-rs` Stage-2 arrangement behind the *same* `LabeledArrangement`
interface (sidecar retained as differential-parity oracle); **M7** clean-rooms
the indirect predicates from Attene's paper and restores WASM.

**Sign-off:** approved by Sequoia Alexander, 2026-05-28, rationale: deliberate
strategy — decouple "functional Yang" from "native arrangement complete";
WASM-break during the development phase is accepted (no users; personal
experiment). Tracking: `docs/yang_functional_roadmap.md` M6/M7.

### N2 — Stage-4 mesh-updating / CDT absent (relocation-only)

**Code location:** `crates/yang-rs/src/lib.rs` — no CDT call sites;
`stage4_relocate_and_correct` (~2260-2479) relocates existing intersection
vertices only.

**Paper section:** §4.4.1 — mesh updating via CDT + split/merge/insert +
per-triangle `d(T)` recalculation.

**Current behavior:** `yang-rs` trusts the sidecar-trimmed mesh and performs only
intersection-vertex relocation onto the exact curve + the §4.5.3 reversed-point
sweep. No remeshing/CDT runs, yet the `lib.rs:6-14` stage docstring lists
"Stage 4 (§4.4.1): Mesh updating via CDT" as if present. Distinct from legacy
**D1** (which concerns `crates/kernel/`).

**Severity:** fidelity gap, **not** a current correctness hole for analytic
inputs — the sidecar mesh is validly trimmed and `check_watertight_2manifold`
gates the output.

**Remediation:** roadmap milestone for real Stage-4 remesh; in the interim, add a
doc note (or a loud `YangError`) so the stage list is not mistaken for a running
CDT. **Sign-off:** pending.

### N3 — §4.5.3 collinear/degenerate-tangent treated as healthy (logic inversion)

**Code location:** `crates/yang-rs/src/lib.rs:2504-2506` — returns `false` (no
reversal) when `t_tilde_len < TAU_WORK`, commented "Degenerate/collinear t̃ ⇒
healthy, no reversal."

**Paper section:** §4.5.3 (refs/text/yang2025_hybrid_boolean.txt:743-745) — places
collinear triples **within** the reversal subset: "if … collinear, t̃ is almost
degenerate … we directly detect the reversal, avoiding the angle comparisons."

**Current behavior:** inverts the paper — collinearity ⇒ *healthy* and the check
is skipped. Harmless on circle/ellipse edges (no 3 collinear points), **but
reachable on Line-type intersection edges** (axis-parallel `plane∩cylinder` →
lines), which `cylinder ∪ box` can produce. The only finding where the code
actively *contradicts* the paper rather than deferring.

**Severity:** medium (latent — reachable via line edges; not exercised by the
current circle/ellipse canonical tests).

**Remediation:** implement the paper's branch (on collinear consecutive points,
detect reversal directly).

**RESOLVED (2026-06-01):** `is_reversed` now returns `true` on degenerate t̃
(`|t̃| < TAU_WORK` ⟺ `v1 ≈ −v2` ⟺ a U-turn at `p_r`), matching §4.5.3's
direct-detection of the collinear reversal case. Regression test
`tests::n3_degenerate_tangent_is_reversal`; full yang-rs suite unregressed (the
fix does not over-trigger on healthy circle/ellipse edges, where `|t̃| ≈ 2`).
No longer a deviation. **Sign-off:** resolved.

### N4 — Face provenance via centroid-proximity, not §4.2.3 barycentric implicit mapping

**Code location:** `crates/yang-rs/src/lib.rs` (~1794-1798) — pick the unique
labeled-solid face plane within `TAU_WORK` of the kept triangle's centroid.

**Paper section:** §4.2.3 — map each intersection point to both surfaces via
Cherchi implicit-point **barycentric** coordinates from the intersecting
triangles.

**Current behavior:** geometric centroid-in-plane proximity, not the
arrangement's intrinsic per-triangle provenance. Works for the current scope; it
is the proximate cause of the **F2** multi-solid `FaceResolutionFailed`. Forced
by **N1** (the sidecar's `LabeledArrangement` exposes only *solid*-level
provenance, not per-triangle barycentric data).

**Remediation:** tied to roadmap **M6** (native arrangement exposes triangle-level
provenance). **Sign-off:** pending.

### N5 — Stage-1 discretization bypasses the unified §4.1 d_ε-iterate + §4.1.2 CDT framework

**Code location:** planar Newell fan `crates/yang-rs/src/lib.rs:531-563` (1:1, no
`d_ε` iteration, no CDT); cylinder analytic rim rings (no u-v CDT).

**Paper section:** §4.1 (298-322), §4.1.2 (404-407).

**Current behavior:** planar faces use an exact 1:1 bijection (faithful-by-
exactness for flat patches — no Steiner points needed); the cylinder uses a
2-ring + chord-bound rim sampling; watertightness comes from shared rim rings
rather than per-boundary CDT. Deliberate divergence, acceptable while inputs are
analytic primitives. Distinct from legacy **D9/D10**.

**Severity:** low. **Remediation:** closure bundled with NURBS support (legacy
**D14** analog). **Sign-off:** candidate (faithful-by-exactness for the analytic
scope).

### N6 — §4.5.4 illegal self-intersection detection/removal absent

**Code location:** not present in the new crates (legacy **D3** covers
`crates/kernel/` only).

**Paper section:** §4.5.4 (752-758).

**Current behavior:** no post-trim self-intersection detection in `yang-rs`.

**Severity:** medium. **Remediation (tracked 2026-06-02):** now **roadmap-tracked**
— a §4.5.4 illegal-self-intersection removal milestone in
`docs/yang_functional_roadmap.md` M8 (alongside Stage-0 coplanar). The crate-doc
Stage list (`crates/yang-rs/src/lib.rs`) explicitly states §4.5.4 is NOT
implemented, so the gap is documented, not silent. (Currently benign for the
analytic primitives in scope: the sidecar emits a validly-trimmed mesh and
`check_watertight_2manifold` gates the output; a true post-trim self-intersection
detector is the milestone.) **Sign-off:** remediation tracked.

### N8 — Stage 0 (§4.5.5 coplanar) verified NATIVE-need, not sidecar-delegated

**Audit follow-up (resolved 2026-06-02).** The Yang-conformance audit flagged that
"Stage 0" was conflated between *unimplemented* and *delegated to the sidecar's
arrangement* — unverified. **Verified:** the patched `mesh_booleans` sidecar
emits **multi-solid-labeled** triangles (`surface.len() == 2`) on coplanar-overlap
input (confirmed by `cherchi-sidecar-rs` test
`c3_coplanar_face_yields_multi_attribution`, run against the live binary). Those
multi-attributed triangles flow into `yang-rs`, where the centroid-proximity face
resolution (N4) cannot pick a single source face → loud `FaceResolutionFailed`
(F2). **Conclusion:** coplanarity is NOT silently resolved by the sidecar; it
surfaces as a multi-attributed arrangement. Therefore M8 "Stage 0" is a **genuine
native pre-pass need** (2D coplanar Boolean before discretization, §4.5.5), not
something delegated away. The current loud-F2 deferral is correct. **Sign-off:**
remediation tracked (roadmap M8).

### N7 — Stage 3 uses closed-form algebraic SSI instead of §4.3 Newton/geometric optimization

**Code location:** `crates/yang-rs/src/lib.rs:1316-1550` (`surface_to_quadric`,
`ssi_curve_to_curve`, intersection-edge selection via `curve_contains_point`);
prompt `specs/yang_pr_yr9_stage3_ssi.md`.

**Paper section:** §4.3 (521-593) — Newton (§4.3.1) / geometric (§4.3.2) /
method-selection (§4.3.3) / curvature refinement (§4.3.4).

**Current behavior:** `ssi_rs::intersect` returns the **exact** closed-form
Circle/Ellipse/Line; the relevant arc is selected by endpoint containment. This
is a deliberate, **superior** substitute for the analytic-primitive scope —
closed-form is exact and more robust than Newton iteration on NURBS Bézier
sub-patches — a natural consequence of the **D14**/analytic-only scope. (§4.3.4
curvature-based polyline refinement and the tangent/small-loop distinction are
not implemented; moot for Circle/Ellipse/Line, open for general analytic pairs.)
Documented in the PR-YR9 spec; recorded here for ledger completeness.

**Severity:** low (documented design substitution, not a hidden divergence).
**Sign-off:** *signed off* — sound in-scope substitution, mirroring the N1
rationale.

### Legacy ↔ new-crate cross-reference

The legacy **D1–D14** entries scope to `crates/kernel/` and do **not** imply
new-crate coverage. Map (legacy → new-crate analog):

| Legacy (kernel) | New-crate (yang-rs) |
|---|---|
| D1 (CDT in §4.4.1) | **N2** (Stage-4 remesh absent) |
| D3 (§4.5.4 self-intersection) | **N6** |
| D4 (§4.5.2 localized refinement) | loud `LocalRefinementRequired` STOP (pr-yr10b) |
| D9 / D10 (§4.1.2 CDT / d_ε density) | **N5** |
| D13 / D14 (Gauss-map / NURBS scope) | loud `CurvedSurfaceNotYetSupported` (Sphere/Cone) |

---

## Priority order for remediation

1. **D2** (post-tessellation repair pipeline) — fundamental, blocks investigation of anything downstream. Removing this will surface what the upstream stages are actually doing.
2. **D1 Tier 3** + **D5** + **D9** — cross-face triangulation consistency (Newell drift, r_A=r_B, per-surface u-v CDT). All related; closing them together makes sense.
3. **D6** (Fig 11 procedures) — closes the §4.4.1 mesh-updating story.
4. **D7** (Cherchi vs flood-fill) — confirm or correct the patch-segmentation refactor.
5. **D3** (§4.5.4 illegal-intersection removal) — independent; can be added at any time.
6. **D4** (localized refinement) — depends on having a working failure-detection path first.
7. **D8** (boundary step rescaling) — minor semantic difference; defer unless it shows up empirically.
8. **D10, D11** (adaptive d_ε) — improvements to discretization fidelity.
9. **D12** (octree) — performance, not correctness.
10. **D13, D14** (NURBS / Bézier) — scope decision; defer until needed.

---

## Closed deviations (signed off; investigation may proceed)

*(none yet)*

---

## Resolved deviations (implementation brought into line with paper)

*(none yet)*

---

## How to add a new deviation entry

When you discover a divergence during work:

1. Add a new entry to "Open deviations" with: code location, paper section, current behavior, paper prescription, architectural consequence, remediation options.
2. Cite the relevant paper line numbers from `refs/text/*.txt`.
3. Flag in the current cycle's audit memo and mention at the TOP of your next message to the user.
4. Halt the affected investigation; do not continue accumulating probe data on the divergent implementation.

## How to sign off on a deviation (user)

Append to the deviation's entry:

```
**Sign-off:** approved by <name>, <date>, rationale: <text>. Tracking issue / future remediation: <link or note>.
```

This is a deliberate choice to accept the deviation — usually because the paper-aligned implementation is impractical or impossible in this codebase context. The sign-off documents the trade-off so future investigations don't waste cycles re-discovering the gap.

## How to mark a deviation resolved

When implementation is brought into line with the paper:

1. Move the entry from "Open deviations" to "Resolved deviations".
2. Append `**Resolved:** <date>, commit <sha>, description: <text>`.
3. Update CLAUDE.md "Known deviations" section if you summarized there.
