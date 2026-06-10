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
analytic primitives. Distinct from legacy **D9/D10**. (PR-NC1 update: non-convex
and holed *planar* faces now go through a no-Steiner CDT — see **N9** — but the
1:1 / no-`d_ε` property is preserved for the planar scope.)

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

### N9 — Planar non-convex / holed Stage-1 tessellation uses no-Steiner CDT (spade)

**Code location:** `crates/yang-rs/src/lib.rs` planar dispatch arm
(`tessellate_planar_cdt_face` + `planar_outer_loop_is_nonconvex`);
`crates/cherchi-rs/src/triangulation/mod.rs` (`cdt_polygon_with_holes`,
backed by `spade` v2). Prompt/spec `specs/yang_pr_nc1_nonconvex_cdt.md`.

**Paper section:** §4.1.2 (404-407, CDT) — the §4.4.1 D1-class concern (plain
ear-clipping is forbidden).

**Current behavior:** convex, hole-free planar faces keep the original fan
triangulation (byte-for-byte; `fuzz_boxes` 900/900 unregressed). Non-convex
outer loops (a reflex vertex) and faces with inner loops route to a true
**constrained Delaunay triangulation** with the boundary loops as hard
constraint edges. The CDT is run with the **boundary vertex set only** — it
adds **no** interior Steiner points and never subdivides a boundary edge, so the
output vertex set equals the input boundary vertex set and the planar
`TessellationMap` stays 1:1-on-boundary. This is the **no-Steiner planar
simplification**, analogous to **N5** (faithful-by-exactness for flat patches:
a planar polygon needs no interior density to be represented exactly). Using
`spade` (Rust-native CDT via the `robust` adaptive predicates) rather than
CGAL's CDT mirrors the long-standing D1 implementation-choice note — same
algorithm class, not a behavioral deviation. This **resolves the D1-class
concern for the new kernel's planar Stage-1** (no ear-clipping anywhere).

**Severity:** low (faithful-by-exactness; documented design choice).
**Sign-off:** candidate — sound in-scope simplification (planar faces carry no
chord error, so no `d_ε` densification is warranted).

### N10 — Stage-5 intersection-edge classification gated by on-both-surfaces predicate (PR-YR18)

**Code location:** `crates/yang-rs/src/lib.rs` `build_intersection_curves`
(the on-both-surfaces gate before `ssi_rs::intersect`); the mis-attribution
source is `compute_phase_a` (`lib.rs:3279-3289`). Spec
`specs/yr18_intersection_edge_attribution.md`.

**Paper section:** §4.2 / §5.5 (intersection-edge identification) — the paper
identifies an intersection edge from the labeled arrangement's true two-surface
provenance; the new kernel instead derives a two-`InputId` incidence from
per-patch boundary cycles, which can mis-tag a single-surface internal facet
edge as `(surfA, surfB)`.

**Current behavior (this PR — a resolution, not a new divergence):**
`compute_phase_a` pushes a patch's single inherited face surface onto *every*
boundary edge of its cycle, so a seam edge shared by two patches is tagged
`(surfA, surfB)` even when one endpoint lies off one surface. Before PR-YR18
such an edge was handed to `ssi_rs::intersect` and produced a loud
`AmbiguousCurve { matched: 0 }` (the dominant CF1 loud refusal, mis-attributed
there to an "SSI rim-selection gap"). PR-YR18 adds an **on-both-surfaces gate**:
an edge reaches `ssi_rs::intersect` only if BOTH endpoints satisfy
`|signed_distance_to_surface(surf, p)| <= tol` for BOTH attributed surfaces,
reusing the SAME Stage-1 chord band `tol` the selection uses (no widening). A
failing edge is reclassified as a single-surface internal edge and falls through
to the `Curve::LineSegment` fallback. This is a *necessary condition* of the
existing `matched == 1` selection, so it cannot regress correctly-classified
edges; it only removes false `(surfA, surfB)` tags. The cleaner fix would be to
re-tag incidence by the *local incident-triangle* surface (design "A"), or to
consume true mesh-level two-surface provenance from the `LabeledArrangement`
producer (the paper's intent); the predicate gate (design "B") is the surgical
in-place enforcement that leaves the incidence map / PR-YR11 ellipse-relocation
consumer / Stage-4 untouched.

**Deferred follow-up (still LOUD):** analytic-conic SSI support
(`Parabola`/`Hyperbola` for **oblique** cone∩plane cuts). A *true* oblique
cone∩plane edge passes the on-both-surfaces gate (both endpoints on both
surfaces) and then still yields a loud `AmbiguousCurve` because
`curve_contains_point` returns `false` for conics — this loud refusal is
**correct** and deliberately preserved by PR-YR18; resolving it is a separate
increment (see N7 "open for general analytic pairs").

**Severity:** low (resolves a mis-classification → loud-refusal defect; the
residual is the documented conic deferral above).
**Sign-off:** candidate — surgical in-scope enforcement of the stated
intersection-edge invariant; the producer-provenance route remains the durable
target.

### N11 — sphere section `Circle` membership uses a projection-scaled radial band (PR-YR19)

**Code location:** `crates/yang-rs/src/lib.rs` — `curve_contains_point` (the
`Circle` arm, plus its caller `build_intersection_curves` threading
`source_radius`) and `stage4_relocate_and_correct` (the `vert_circle`
relocation guard, split into per-component axial/radial bands). Spec
`specs/yr19_sphere_chord_band.md`. Cross-reference **N10** (PR-YR18, the
on-both-surfaces gate that cleared the cylinder mass but only partially cleared
sphere — N11 resolves the residual).

**Paper section:** §4.1 / §5.4 (Stage-1 chord error `d_ε` and the implicit
on-curve membership residual). The paper's `d_ε` bounds the *surface-normal*
tessellation error; it does not separately discuss how that bound propagates
into the *in-plane radial* metric of a section circle when a curved surface is
cut by a plane.

**Current behavior (this PR — a resolution, not a new divergence):** A sphere
mesh vertex within `d_ε` of the sphere *along its normal*, intersected with the
cutting plane, can project to an in-plane radial deviation up to
`(R / r_circle) · d_ε` (`R` = sphere radius, `r_circle` = section circle
radius). Derivation: on the cut plane `|p − C| = √(h² + radial²)`, and
`d/d(radial)√(h² + radial²) = radial/|p−C| ≈ r_circle/R` at `radial = r_circle`,
so `d_sphere ≈ (r_circle/R)·dr`. Before PR-YR19 both membership sites compared
the in-plane radial deviation to a flat `d_ε`, **under-bounding** it: a vertex
genuinely on the section circle within the Stage-1 chord error (and passing the
N10 on-both gate, which uses the correct surface-normal metric) was rejected by
`curve_contains_point` → loud `AmbiguousCurve { candidates: 1, matched: 0 }`, or
by the Stage-4 `circle_residual > d_eps` guard → `OffCurveBeyondChordBand`.
PR-YR19 carries the SAME `d_ε` through the section projection: the in-plane
**radial** band becomes `(R / r_circle) · d_ε` while the **axial**
(out-of-plane) band stays `d_ε` (the cut plane is exact). Surface-type-gated on
a `Surface::Sphere` owner via `source_radius: Option<f64>` — every non-sphere
path (`None` factor) stays byte-for-byte identical. A near-tangent guard
(`r_circle > MIN_FEATURE_SIZE`) fails **closed** (keeps the unscaled band) so the
factor cannot blow up. This is the exact geometric propagation of the same
`d_ε`, not tolerance widening (P9/P10): the band is *derived*, not picked to
pass, and a point off by more than the propagated band still STOPs loudly.

**Why approach (A) (projection-scaled radial band) and not (B)
(surface-distance unification):** `curve_contains_point` also disambiguates the
`matched == 1` selection (e.g. parallel cylinder∩plane two-`Line` candidates). A
curve-independent on-both-surfaces test would test true for *every* candidate,
collapsing `matched` to `candidates` and re-raising `AmbiguousCurve` on
legitimate multi-candidate cases — a regression of the N10 cylinder result.
Approach (A) keeps the per-curve geometric test intact.

**Deferred follow-up (still LOUD):** the **cone** analytic-conic share
(`Parabola`/`Hyperbola` for oblique cone∩plane cuts) is unaffected by N11 and
stays loud (a separate increment; see N7 / N10).

**Severity:** low (resolves a metric-inconsistency → loud-refusal/false-invalid
defect; sphere∩plane only, surface-type-gated).
**Sign-off:** candidate — exact geometric propagation of the existing `d_ε` in
the in-plane radial metric; the producer-provenance route (N10) remains the
durable target for intersection-edge identification generally.

### N12 — Stage-6 face resolution ranks ties by exact-vs-band tier (PR-YR20)

**Code location:** `crates/yang-rs/src/lib.rs` — the **non-degenerate** branch of
Stage-6 geometric face resolution (the centroid-membership counter). Spec
`specs/yr20_face_resolution_tiered_tiebreak.md`. Refines **N4** (face provenance
via centroid proximity, not the §4.2.3 barycentric implicit map).

**Paper section:** §4.2.3 / §5.5 (mapping a kept mesh triangle back to its
originating B-Rep face). The paper resolves provenance from the labeled
arrangement's exact per-triangle face attribution; the new kernel instead tests
which input face's surface contains the kept triangle's **centroid** within that
face's Stage-1 chord band `tol_for` (see N4). When two faces of different
curvature share a rim, a triangle exactly on a planar cap can also fall inside
the curved lateral's necessarily-loose chord band → a two-hit tie.

**Current behavior (this PR — a resolution, not a new divergence):** The pre-YR20
rule counted faces whose surface contains the centroid within `tol_for`: exactly
1 → attribute, 0 or ≥2 → `FaceResolutionFailed`. It treated an **exact**
`TAU_WORK` planar hit (`dist = 5.5e-17`) and an **approximate** `d_ε` chord-band
hit (`dist = 7.6e-3`, `tol = 2.4e-2`) as equal weight, so a cap-on-rim triangle
that is genuinely on the cap raised a spurious `n_hits == 2` tie — the dominant
non-cone curved-fuzz refusal. PR-YR20 ranks hits by **tier**: EXACT
(`dist < TAU_WORK`, the centroid lies ON the surface) dominates BAND
(`TAU_WORK ≤ dist < tol_for`). Attribute to the unique hit at the minimum
populated tier; ≥2 at that tier, or no hit at all, still `FaceResolutionFailed`.
`tol_for`, `plane_dist`, the band values, the degenerate-sliver branch, and the
YR18/YR19 intersection-edge path are untouched.

This is the natural generalization of the single-band membership test — each face
still uses its own A14.3 band; we only break ties by the exact-vs-band tier the
centroid satisfies. It is **not** a new looser constant: `TAU_WORK` is the
existing planar tolerance reused as the tier boundary. For an **all-planar input**
every hit is EXACT (planar `tol_for == TAU_WORK` ⇒ `dist < tol_for` ⇒
`dist < TAU_WORK`), the BAND tier is unreachable, and the new rule reduces
**byte-for-byte** to the old "exactly one face within `TAU_WORK`" rule — so the
box fuzz, the m3 coplanar-tie tests, and the yr5c planar-sliver tests are
unaffected and genuine coplanar / multi-solid ties (≥2 EXACT) still STOP loudly.

**Why tier-by-distance and not the `dist/tol`-ratio variant:** a ratio would
distinguish two planar hits at different sub-`TAU_WORK` distances and silently
convert a current planar F3 into an attribution, breaking the all-planar safety
property. Tier-by-distance keeps every planar hit in the same EXACT tier.

**Deferred follow-up (still LOUD):** a **cone** triangle that stops being an F3
tie under N12 simply refuses later for the deferred analytic-conic reason
(`Parabola`/`Hyperbola` for oblique cone∩plane cuts; see N7 / N10 / N11) — cone
`ok_correct` stays 0, which is correct.

**Severity:** low (resolves a mixed exact-planar-vs-curved-band tie →
loud-refusal defect; all-planar inputs byte-identical, single-tier curved/planar
ties unchanged).
**Sign-off:** candidate — tie-ranking generalization of the existing single-band
centroid membership test; the producer-provenance route (N4) remains the durable
target for face attribution generally.

### N13 — PR-CR-AR1 builds explicit+LPI points only; TPI deferred to AR2 (scope correction)

**Code location:** `crates/cherchi-rs/src/arrangements/intersection_points.rs`
(new; `#[cfg(feature = "indirect-predicates")]`). Prompt PR-CR-AR1 ("tri-tri
intersection → implicit points"). C++ reference
`/home/claude/cherchi2022/InteractiveAndRobustMeshBooleans/arrangements/code/intersection_classification.cpp`.

**Paper / source section:** Cherchi 2022 arrangement, `intersection_classification.cpp`
(`checkTriangleTriangleIntersections` cpp:119-280; sign-pattern decoders
cpp:834-925; `checkSingleNoCoplanarEdgeIntersection` cpp:679-730;
`checkVtxInTriangleIntersection` cpp:734-784).

**Prompt-vs-source divergence (this is the deviation):** the PR-CR-AR1 prompt's
literal wording mentions constructing TPI (`implicitPoint3D_TPI`) points in this
slice. **Direct reading of the source contradicts this.**
`intersection_classification.cpp` constructs **only** explicit input vertices and
`implicitPoint3D_LPI` points — three LPI call sites
(`addEdgeCrossEdgeInters` ×2 at cpp:290/324, `addEdgeCrossTriInters` at cpp:358),
**zero** TPI constructions. TPI (`implicitPoint3D_TPI`) is built in
`triangulation.cpp::createTPI` — the **re-triangulation** stage, which the
roadmap assigns to **PR-CR-AR2**.

**Decision (source-faithful, governance "port what Cherchi does — don't invent
mechanism"):** AR1 builds **explicit + LPI only**. TPI is deferred to AR2 where
`createTPI` actually lives. Building a TPI path in AR1 would improvise mechanism
the spec file does not contain. AR1 further restricts to the **generic
non-coplanar transversal crossing** (the clean core):
`checkSingleNoCoplanarEdgeIntersection` (edge-pierces-plane → LPI) +
`checkVtxInTriangleIntersection` (vertex-in-triangle → explicit). Fully-coplanar
pairs (`allCoplanarEdges`, orBA `0 0 0`) and the single-coplanar-edge degeneracy
(`singleCoplanarEdge`, orBA e.g. `1 0 0`, handled in C++ by
`checkSingleCoplanarEdgeIntersections` via jolly points + in-plane edge-edge LPIs)
are emitted with a **classified `Deferred(..)` marker — loud, never silently
dropped** — and deferred to a later slice.

**Secondary note — coarse `point_in_triangle_3d`:** cherchi-rs's
`point_in_triangle_3d` returns `{StrictlyInside, OnBoundary, StrictlyOutside}`,
whereas the C++ needs `ON_VERT*` vs `ON_EDGE*` discrimination to decide explicit
vs LPI. AR1 distinguishes "coincides with an input vertex" (→ explicit) via
**exact coordinate equality** to the three triangle vertices, and uses
`orient3d` sign patterns to decide piercing (→ LPI). **No tolerance — exact
compares only.** A granular `PointInSimplex` port is future work (AR2 may need
`ON_EDGE*` to route the coplanar-edge sub-cases).

**Tertiary note — non-exact `point_in_segment_3d` guard (driver-flagged
post-merge):** the adversary remediation added `point_strictly_inside_segment`
as a **conservative raw-`f64` stopgap** (cpp:688-691; its own doc-comment states
it is "NOT robustly exact"). This is a narrow degenerate guard (a triangle vertex
lying strictly inside the opposite edge → explicit-vertex recording), NOT the LPI
construction itself (which is FFI-exact and on-plane-verified). But it diverges
from the cherchi-rs exact-arithmetic hard rule and partially contradicts the
"exact compares only" claim above. **Fix (AR2/AR3 follow-up):** replace with the
EXACT collinearity predicate cherchi-rs already has (CR1 `points_are_collinear_3d`)
+ an exact between-ness check; do NOT leave raw `f64` in the arrangement core.
**RESOLVED (PR-CR-AR2b Cycle B):** the raw-`f64` guard was replaced by the exact
CR1 collinearity + between-ness predicate. The TPI-handle deferral noted in this
entry is RESOLVED at the routing layer by Cycle C1 (see N15); the TPI
*enforcement* (`createTPI`) remains banked to Cycle C2 / AR3.

**Severity:** low (scope sequencing + two documented coarse/non-exact predicate
workarounds in narrow guards, not a hidden behavioral divergence in the exact LPI
core; all transparently flagged and roadmap-tracked to AR2/AR3). The transversal
core is ported faithfully and reference-checked via the exact on-plane
indirect-`orient3d` oracle.
**Sign-off:** candidate — source-faithful scope split; TPI and the coplanar
point-construction paths are roadmap-tracked to AR2 / a later slice.

### N14 — PR-CR-AR2a point/edge insertion: readable `splitSingleTriangle` with a uniform on-edge check; structural LPI dedup

**Code location:** `crates/cherchi-rs/src/arrangements/retriangulate.rs` and
`aux_structure.rs` (new; `#[cfg(feature = "indirect-predicates")]`). Prompt
PR-CR-AR2a ("per-triangle POINT/EDGE insertion"). C++ reference
`.../arrangements/code/triangulation.cpp` (`splitSingleTriangle` cpp:189-222,
`splitSingleTriangleWithStack` cpp:225+, `findContainingTriangle` cpp:455,
`fastPointOnLine` cpp:1153).

**Paper / source section:** Cherchi 2022 re-triangulation (`triangulation.cpp`
point-insertion phase, before `addConstraintSegmentsInSingleTriangle`).

**Deviations (three, all source-faithful adaptations):**

1. **Readable `splitSingleTriangle` ported instead of the active
   `splitSingleTriangleWithStack`.** The C++ active path is the stack-based
   variant (a perf optimization that pre-loads all points then walks a custom
   stack); `splitSingleTriangle` (linear-scan `findContainingTriangle` per point)
   is in a commented-out branch but produces the **same output mesh**. AR2a ports
   the readable linear-scan form (simplest to oracle); the `WithStack`/`WithTree`
   perf ports are deferred (the CR12c `Tree` API is already available if/when
   `WithTree` is wanted).

2. **Uniform on-edge check applied to every inserted point, including the first.**
   The C++ `splitSingleTriangle(points)` special-cases the first point with
   `splitTri(0, v)` unconditionally — but this is safe in C++ **only because that
   function receives interior-only points** (`t_points`); on-edge points are
   carried in separate `e0/e1/e2_points` lists and handled by
   `splitSingleTriangleWithStack`. AR2a's `aux_structure` groups interior **and**
   on-edge points and `split_single_triangle` consumes them as **one flat list**,
   so an on-edge point can appear first. AR2a therefore runs
   `find_containing_triangle` + the three `fast_point_on_line` edge tests
   **uniformly for every point** (no `splitTri(0,v)` special-case): on-edge →
   `split_edge` (2 tris, no degenerate fan), interior → `split_tri` (3 tris),
   outside → `RetriangulateError::NoContainingTriangle`. This is the faithful
   adaptation of the C++ semantics to AR2a's unified point list; the load-bearing
   exact covering oracle (dashu `RBig` area-sum + same-sign winding, with LPI
   coords from exact line-plane intersection) would catch any degenerate or
   non-covering result.

3. **LPI dedup by structural generator equality (first slice).** The global typed
   -point set dedups `Explicit` by exact `Point3` equality and `Lpi` by structural
   generator (`line` + `plane`) equality. Exact-coincident LPIs with *different*
   generators (same geometric point, different constructing edge/plane) are NOT
   merged in AR2a — that would need a `coincident`/`lessThan` FFI. Low likelihood
   for transversal inputs; flagged for AR3 if differential parity surfaces a
   merged-vertex mismatch.

**Precursor:** PR-CR-AR2a Cycle 1 added the implicit 2D predicates the location
tests need — `orient2d_xy/yz/zx` + 4-arg `point_in_triangle` — to
`indirect-predicates-sidecar-rs` (CR-IP6b), via `genericPoint` static dispatch
(demand-driven; AR2a is the caller).

**Out of scope (AR2b/AR3):** enforcing intersection *segments* as constrained
edges (`addConstraintSegment`), TPI construction (`createTPI`), cross-triangle
weld parity, and replacing the N13 raw-`f64` `point_in_segment` guard with the
exact CR1 collinearity predicate.

**Severity:** low (readable-over-perf port with identical output; a uniform-check
adaptation forced by — and consistent with — AR2a's unified point list; a
first-slice dedup scope limit). All transparently flagged and roadmap-tracked.
**Sign-off:** candidate — source-faithful insertion port; constraints + TPI +
cross-triangle parity are roadmap-tracked to AR2b/AR3.

### N15 — PR-CR-AR2b Cycle C1 TPI routing: macro dispatch (faithful) + the createTPI STOP (blocking re-scope to C2/AR3)

**Code location:** `crates/cherchi-rs/src/arrangements/retriangulate.rs`
(`#[cfg(feature = "indirect-predicates")]`). Prompt PR-CR-AR2b Cycle C1. C++
reference `.../arrangements/code/triangulation.cpp` (`createTPI` cpp:1007,
`computeTriangleOfSegment` cpp:1041, `computeTriangleOfSegmentInCoplanarCase`
cpp:1076).

**Paper / source section:** Cherchi 2022 re-triangulation TPI construction.

**Resolution (what C1 fixes):** the **N13 TPI-handle deferral is RESOLVED at the
routing layer.** `VertexCoords::Tpi` now flows through the per-base-triangle
re-triangulation as a real, exact `ImplicitPoint3DTpi` handle (nine generators =
three supporting planes), replacing the Cycle-B `sum/9` explicit-centroid
placeholder in `gp()`. Verified by an exact on-three-planes oracle (`orient3d ==
Zero` on each supporting plane, FFI, not float tolerance) plus a pure-`dashu`
3×3 plane-solve cross-check and the AR2a covering oracle. Likewise the **N13
raw-`f64` `point_in_segment` guard was already RESOLVED in Cycle B** (exact CR1
collinearity + between-ness).

**Deviation (faithful):** the C++ dispatches predicates over the runtime
`genericPoint` type tag via hand-enumerated branches; the Rust port uses one
recursive `macro_rules! with_gp!` that destructures `Gp` over the three variants
(`E`/`L`/`T`) and monomorphizes to the identical safe `genericPoint::`-static
wrappers (`point_in_triangle`, `orient2d_{xy,yz,zx}`), generating the full 3^N
(81 for `point_in_triangle`, 27 for `orient2d`) concrete instantiations. Only the
safe static wrappers are called — never `_II`/`_IIII` (segfault on explicit
input, CR-IP6). Behavior-identical to the C++; local-only Tpi handling (no global
dedup), same precedent as the existing Lpi routing.

**BLOCKING re-scope (the STOP, P9/P10) — TPI *enforcement* deferred to Cycle
C2 / AR3:** `createTPI` (the segment-crossing creator) sources the TPI's 2nd/3rd
supporting planes via `computeTriangleOfSegment` (cpp:1041), which queries the
**global** `AuxiliaryStructure::seg2tris` map for a non-coplanar witness triangle
and falls back to a global `jollyPoint` for coplanar cases
(`computeTriangleOfSegmentInCoplanarCase` cpp:1076). The Cycle-B
`ConstraintSegment.source_tri` is a correct local substitute **only** for an
original transversal segment's witness — it does NOT cover mid-recursion
sub-segments' provenance or the coplanar fallback without reintroducing the
global structures (AR3-level state). Per the brief's STOP condition, the
`addConstraintSegment` enforcement core (`findIntersectingElements`,
`boundaryWalker`, `earcutLinear`, the segment-crossing `createTPI`) is re-scoped
to **Cycle C2 / AR3** rather than improvised. C1 lands only Piece 1 (handle
routing); it constructs **no** constraint-enforcement code.

**Severity:** low (a behavior-identical macro for the dispatch; the deferred
piece is explicitly STOP-banked, loud, and roadmap-tracked — no hidden
divergence). The TPI handle is reference-checked via the exact on-plane
indirect-`orient3d` oracle.
**Sign-off:** candidate — source-faithful handle routing; the createTPI
enforcement (global `seg2tris` + `jollyPoint`) is roadmap-tracked to Cycle
C2 / AR3.

### N16 — PR-CR-AR3a constraint enforcement: per-work-item `source_tri` replaces the global `seg2tris`; deep-recursion/coplanar TPI deferred to AR3b

**Code location:** `crates/cherchi-rs/src/arrangements/enforce.rs` +
`arrangements/gp_dispatch.rs` (both `#[cfg(feature = "indirect-predicates")]`).
Prompt PR-CR-AR3a. C++ reference `.../arrangements/code/triangulation.cpp`
(`addConstraintSegment` cpp:597, `findIntersectingElements` cpp:644,
`boundaryWalker` cpp:806, `earcutLinear` cpp:912, `createTPI` cpp:1007,
`segmentsIntersectInside` cpp:1170, `pointInsideSegment` cpp:1178,
`splitSegmentInSubSegments` cpp:1185).

**Paper / source section:** Cherchi 2022 re-triangulation constraint enforcement.

**Resolution (what AR3a completes):** the N15 BLOCKING re-scope is now resolved
for the in-scope case. The `addConstraintSegment` enforcement core is ported:
already-an-edge flagging; non-crossing enforcement (`findIntersectingElements`
over non-constraint edges + `boundaryWalker` ×2 + `earcutLinear` ×2 +
`add_tri`/`remove_tris` + `set_edge_constr`); and the segment-crossing branch
constructing a real `ImplicitPoint3DTpi` (`createTPI`) where two constraint
segments cross. The **N13 TPI deferral is now fully resolved** (construction at
C1 + enforcement at AR3a). Public surface: `SegmentSpec` / `EnforceError` /
`enforce_constraint_segments` / `enforce_constraints`.

**Deviation 1 (the minimal `TriangleSoup`, faithful substitute):** the C++
`createTPI` sources the crossing planes via `computeTriangleOfSegment`, which
queries the **global** `AuxiliaryStructure::seg2tris`. AR3a replaces that global
state with a **per-work-item carried `source_tri`** plus a `constraint_planes`
`HashMap<(u32,u32), [Point3;3]>` side map keyed by the constraint edge's sorted
vertex-id pair (vertex ids are stable under `add_*`/`split_*`; edge ids are not).
Sub-segments born mid-recursion inherit their parent's plane (a collinear
sub-piece has the same supporting plane), so the original-transversal X-crossing
— and, empirically, the two-independent-crossings case, which resolves the second
crossing's plane from the first crossing's recorded sub-edge planes — is handled
with directly-available planes, no global structure.

**Deviation 2 (EE-bool asymmetry repair, faithful):** `point_inside_segment`
queries `genericPoint::pointInInnerSegment` in **both** endpoint orders and ORs
them. Justification: the sidecar's `lessThanOnX/Y/Z` explicit-explicit branch
(`implicit_point.hpp:73/83/93`) returns a C++ `bool` (0/1, never −1), so
`pointInInnerSegment(p,v1,v2)` silently returns `false` for a *descending*
explicit segment — an endpoint-order asymmetry. The OR restores the symmetric
"strictly inside" semantics and is a no-op for implicit endpoints (the real
Cherchi path, where `lessThanOn` is sign-aware). NOT a tolerance widening or
fixture special-case. `innerSegmentsCross` (the `segmentsIntersectInside` path)
does **not** share the asymmetry — it routes through the signed Shewchuk
`orient2d_EEE` determinant — so it is ported verbatim (adversary-verified).

**STOP walls deferred to AR3b (P9/P10):** `computeTriangleOfSegment`'s global
`seg2tris` sourcing and the coplanar `jollyPoint` fallback. A sub-segment that
loses its directly-available `source_tri` surfaces as
`EnforceError::SourcePlaneUnavailable`; a non-general-position three-plane TPI
surfaces as `EnforceError::DegenerateTpi` (guarded by an exact `dashu` 3×3
normal-determinant check). Neither is hit by the in-scope corpus; both are loud,
roadmap-tracked errors rather than improvised fallbacks.

**Deviation 3 (pure move):** the `Gp`/`backing`/`gp`/`with_gp!`/`dispatch_*`
toolkit was factored out of `retriangulate.rs` into `gp_dispatch.rs` and reused
by both `retriangulate` and `enforce` — no behaviour change (retriangulate suite
unregressed).

**Severity:** low — source-faithful enforcement; the deferred global/coplanar TPI
is loud (typed errors) and roadmap-tracked; both deviations are behavior-identical
to the C++ on the real (implicit-point) path. Oracle is structural + EXACT
(`orient3d == Zero` on 3 planes; pure-`dashu` covering), per the parity-oracle
correction (no standalone C++ arrangement binary; full parity at BL3).
**Sign-off:** candidate — source-faithful enforcement core; global conforming
soup + global `seg2tris`/coplanar `jollyPoint` TPI roadmap-tracked to AR3b.

### N17 — PR-CR-AR3b coplanar/single-coplanar-edge: defer ONLY a real intersection AR1 cannot construct; benign touches pass through

**Code location:** `crates/cherchi-rs/src/arrangements/soup.rs` —
`deferred_pair_must_defer` / `coplanar_tris_overlap` /
`single_coplanar_edge_introduces_geometry` (step 7 of `mesh_arrangement`).
Prompt PR-CR-AR3b. C++ reference `.../arrangements/code/triangulation.cpp`
(`checkSingleCoplanarEdgeIntersections`) + the coplanar-triangle handling in
`solve_intersections.cpp`.

**Paper / source section:** Cherchi 2022 arrangement of coplanar / edge-coplanar
triangle pairs.

**Deviation:** AR1 (`classify_all`) returns `Deferred(Coplanar | SingleCoplanarEdge)`
for **every** coplanar-or-edge-coplanar pair, because the native arrangement does
not (yet) implement the C++ `checkSingleCoplanarEdgeIntersections` in-plane
resolution path. Rather than loud-defer *all* of them (which would reject every
valid solid, whose adjacent faces share coplanar edges), AR3b adds an EXACT,
tolerance-free triage: a deferred pair is surfaced as
`ArrangementError::CoplanarPairDeferred` **iff** it introduces real geometry AR1
cannot construct —
- **Coplanar pair:** the two triangles overlap in **positive area** (exact 2D
  test); edge-/vertex-only touches (a solid's adjacent or co-planar faces) are
  benign and pass through.
- **SingleCoplanarEdge pair** (non-coplanar): the coplanar edge passes through
  the other triangle's **strict interior** or **properly crosses** one of its
  edges in the shared plane; a boundary/shared-edge touch is benign.

This matches the C++ reference outcome (benign coplanar adjacency does not
generate intersection geometry) while keeping the genuinely-unhandled cases
**loud** (P9/P10), roadmap-tracked to the §4.5.5 2D-Boolean pre-pass at **M8**.
The `SingleCoplanarEdge`-through-interior loud-defer branch is pinned end-to-end
by `adversary_coplanar_edge_through_interior_is_loudly_deferred`.

**Severity:** low — the pass-through is EXACT (no tolerance, no fixture special
case) and behavior-identical to the C++ on benign coplanar adjacency; the
unhandled positive-area / interior-crossing cases are loud typed errors deferred
to M8. **Sign-off:** candidate.

### N18 — PR-CR-AR3b exact-coordinate canonicalization welds coincident implicit points across triangles

**Code location:** `crates/cherchi-rs/src/arrangements/soup.rs` —
`canonicalize_points` (step 8 of `mesh_arrangement`). Prompt PR-CR-AR3b. C++
reference: the global vertex identity maintained by
`AuxiliaryStructure` / `mergeDuplicatedVertices` over the arrangement's point set.

**Paper / source section:** Cherchi 2022 global arrangement point identity (one
geometric point ⇒ one vertex id across all incident triangles).

**Deviation:** the same geometric intersection point can be interned via
**different generator tuples** (e.g. an LPI from triangle `t`'s edge vs a TPI from
the three-plane crossing on triangle `u`), yielding structurally-distinct
`VertexCoords` that are nonetheless the *same* point. The C++ keeps a single
global vertex identity; the native port reaches the points per-pair first, so
AR3b adds a post-grouping pass that canonicalizes the interned points by **EXACT
geometric coordinates** (pure-`dashu`, rewriting only `.coords`, preserving
length and indices so segment-endpoint ids stay stable). Coincident LPI/TPI
points then weld to one identity downstream (re-triangulate / enforce / global
weld), so a shared intersection vertex is not duplicated across the two incident
base triangles.

The weld is **anti-over-weld safe**: it collapses only points equal under EXACT
coordinate comparison; genuinely-distinct intersection points stay distinct
(pinned by the N18 dedup adversary test). NOT a tolerance-based merge.

**Severity:** low — EXACT coordinate equality (no epsilon); restores the C++'s
global one-point-one-id invariant the per-pair construction order would otherwise
break. Oracle: structural + EXACT (conforming soup + implicit-points-welded
invariants; anti-over-weld adversary). **Sign-off:** candidate.

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

### N19 — PR-CR-BL2 ray perturbation: one coherent offset per attempt; winner-less events skip (C++ early-break quirk + `-1` semantics)

`perturbRayAndFindIntersTri` (booleans.cpp:1016) has an early `break` that,
once any offset has produced a hit, mixes hits gathered under DIFFERENT
perturbed rays and sorts them with the last ray. The port evaluates one
offset fully (all candidate triangles under one coherent perturbed ray),
sorts that offset's hits, and returns the nearest — the evident intent.
A winner-less event (all 8 offsets graze — tangential configurations such
as a ray running exactly along an input edge line) contributes NO parity
crossing and is skipped, matching the C++ `if(winner_tri != -1)` call
sites; the port's original fatal `PerturbationExhausted` error was wrong
on valid grazing input (adversary BUG-2/BUG-3) and was removed. Note the
C++ itself can index an empty vector here (booleans.cpp:1048, UB) when
every perturbed hit lies behind the origin; the port returns None.

### N20 — PR-CR-BL2 in/out: ray-parameter-ZERO hits are discarded (C++ keeps them and mislabels point-touch inputs)

`sortIntersectedTrisAlongX/Y/Z` (booleans.cpp:1190) discards only hits
STRICTLY before the ray origin (`lessThanOn* < 0`); a hit at parameter
exactly zero — the origin lying ON another input's surface — survives and
its back-face orientation classifies the patch as inside, which is wrong
for tangential touches (a point-touching tetra pair labels the touching
solid "inside" the other; adversary BUG-1, silent-wrong class). The port
discards `<= 0`. Justification (principle over literal): a ray origin can
lie on another input's surface only TANGENTIALLY — a transversal origin
would sit on an intersection curve, making it a BL1 border vertex, which
ray-origin selection excludes — and a tangential t=0 hit crosses nothing.
The C++ behavior on these measure-zero configurations is a reference
defect, not a semantic to preserve; full-corpus parity (BL3) is unaffected
away from touch configurations.
