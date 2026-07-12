# Yang/Cherchi Implementation Deviations Log

**Purpose:** authoritative record of known divergences between the Yang 2025 hybrid B-Rep/mesh boolean pipeline as specified in the paper, and the implementation. (The `D*` entries reference the now-DELETED legacy `crates/kernel/`; the live pipeline is the `N*` new-crate series in `yang-rs`/`cherchi-rs`.) Any deviation listed here MUST have either (a) a user sign-off with stated rationale, or (b) an active remediation tracked.

**Discipline:** per `CLAUDE.md` "Paper-Spec Compliance is MANDATORY," deviations are errors. Investigation on a component with an open deviation is blocked until either the deviation is fixed or signed off in writing.

**Posture (2026-06-26):** user directive — implement Yang faithfully and *generally*; the assay is a regression detector, not the objective (do not prioritize work by score). The live faithfulness backlog and the general §4.5.5 plan are in `docs/yang_functional_roadmap.md` §0.1–0.2 + M8. Status notes since these entries were written: **N1 is RESOLVED** (native `cherchi-rs` arrangement replaced the sidecar, M6/M7 complete), which **unblocks N4** (§4.2.3 barycentric provenance is now implementable — the native arrangement can expose per-triangle provenance). The substantive open paper-faithfulness deviations are: general Stage-0 §4.5.5 (the keystone), **N4** (face provenance), **N2** (Stage-4 CDT mesh updating), **N5** (unified §4.1 discretization), **N6** (§4.5.4 self-intersection removal). **N7** (closed-form SSI) is signed off; NURBS is a deferred scope milestone.

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

**Code location (refreshed 2026-07-12 — the god-module was decomposed; `lib.rs`
is now 161 lines):** the production Stage-4 path is
`crates/yang-rs/src/stage4_correct.rs::stage4_relocate_and_correct` (`:723`) plus
the relocation helpers in `crates/yang-rs/src/stage4_relocate.rs`. It relocates
existing intersection vertices only — there is still NO CDT/mesh-update call in
the production path (see "Increment status" below for the built-but-unwired
primitives and the partial closed-form remediation).

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
CDT. **Status (2026-07-12): PARTIALLY REMEDIATED, still OPEN.** The general CDT
mesh-update remains unwired, but a series of exact CLOSED-FORM junction handlers
has shipped that resolves the specific over-determined-junction sub-family that
was the dominant Stage-4 ERROR class (enumerated under "Partial remediation"
below). No user sign-off — the core §4.4.1 CDT deviation is not closed.

**Increment N2-1 landed (2026-07-01):** the faithful §4.4.1 mesh-updating
*primitive* now exists, unit-tested in isolation, but is **not yet wired** into
`stage4_relocate_and_correct` (so the deviation is not yet closed). Two pieces:
- `cherchi_rs::cdt_with_interior_constraints` — CDT of a planar patch that
  inserts an intersection polyline as interior constraint edges (Fig 11 `split`:
  each segment becomes a shared edge on both sides). Deterministic, no interior
  Steiner points, rejects crossing constraints (no silent Steiner split, P9/P10).
- `yang_rs::stage4_update::stage4_mesh_update` — the parametric-domain primitive
  implementing Fig 11 `split` (splice on-boundary points into the loop) / `merge`
  (fuse a patch vertex within `merge_tol` of a curve point, moving it onto the
  curve) / `insert` (a closed loop enclosing no patch vertex gets one interior
  centroid point), then calls the CDT. Invariants I1–I6 (constraint realized, no
  flips, boundary→boundary, area conservation, merge/insert monotonicity,
  determinism) are unit-tested.
Spec: `specs/n2_stage4_mesh_updating.md`.

**Increment N2-2 landed (2026-07-02):** the §4.1.2/Fig-6 per-triangle `d(T)`
recompute the §4.4.1 mesh update calls for ("we recalculate d(T) to maintain
controllable error") — `yang_rs::stage4_dt::{eval_uv, d_of_t}`. The paper's
control-net bound is implemented exactly, generalized from NURBS to our
analytic surfaces: every curved `Surface` is a surface of revolution with an
EXACT rational-Bézier patch representation (Piegl & Tiller ch. 8), so the
covering rectangle of the triangle's uv corners is subdivided to ≤90° spans
(positive weights) and `d(T) = max` control-point-to-triangle distance —
certified by the convex-hull property + convexity of point-to-triangle
distance. `eval_uv` pins the parametric embedding (ortho_basis frames; sphere
uses the canonical ẑ axis) that N2-3's patch extraction must share. Unit suite
(19 tests, I1–I7) + adversary suite (`tests/n2_dt_adversary.rs`, 23 probes)
incl. a mutation-kill matrix (arc-scale flip / subdivision skip / weight drop
all caught). Spec: `specs/n2_stage4_dt_recompute.md`.

**Increment N2-3a landed (2026-07-02) + N2-3 wiring grounding:** instrumenting
all four Stage-4 repair STOPs showed ZERO live hits (yang-rs suites, campaign,
194-case assay) — the "Stage 4 hits loud stops" premise is stale, so the
`stage4_mesh_update`/`d_of_t` wiring is deferred until a consumer exists. The
grounding trail instead found and fixed the live §4.5.5/§4.4.1-faithfulness
defect: Stage-0 minted overlay rim-chord subdivision vertices at CHORD
positions (off the exact circle by the sagitta — R0072's `VertexOffSurface`
class, silent in release). N2-3a mints them on the exact rim `Curve::Circle`
(radial projection / exact circle∩line for crossings) behind a fold-validity
gate; the gate's revert population (coarse rims where exact placement inverts
overlay triangles) is the first recorded LIVE consumer for overlay-level mesh
updating — the natural first wiring site for the N2-1/N2-2 primitives. Spec
(with the full grounding trail in §0): `specs/n2_stage4_junction_cluster_merge.md`.

**Remaining for sign-off:** wire the N2-1/N2-2 primitives into the first
measured consumer (the Stage-0 fold-gate revert population is the recorded
candidate; Stage-4 wiring only when a Stage-4 consumer appears), retiring the
residual chord-position mints behind watertight / reference-parity oracles.

**A live STAGE-4 consumer population now EXISTS (2026-07-08 diagnosis).** The
`Stage-4 relocation region … LocalRefinementRequired` STOP is the single dominant
assay ERROR mechanism — **12 of 41 ERROR cases** in the current baseline
(`target/assay_kv2_report.json`): R0004, R0015, R0017, R0019, R0032, R0044,
R0047, R0049, R0070, R0077, R0096, F0059. Per-site census (env-probe on the
`Stage4RegionInvalid` sites in `stage4_relocate_and_correct`) decomposes them:

- **Over-determined conic JUNCTION audits** (sites ~9801/9817/9832/9849/9867 —
  "a vertex shared by BOTH a circle and an ellipse edge … loud STOP"): the
  largest sub-family. F0059 (ellipse × circle), R0004/R0017/R0019 (cone-conic ×
  conic). These are **NOT genuine ambiguities** — the vertex's exact position is
  the transversal common point of the DISTINCT surfaces incident at it (proven:
  F0059 v5 → {plane y=−0.25, cyl A, cyl B}; R0019 v25 → {plane, cone₁, cone₂}).
  The torus block (~10560) already solves the identical problem for degree-4
  torus junctions via `relocate_onto_implicit_triple`; the conic side just lacks
  the equivalent handler. Design spec: `specs/yang_stage4_conic_triple_junction.md`.
- **Line+circle junction, line ∥ circle-plane** (site 10435): R0015 — degenerate
  (no transversal junction); genuinely hard.
- **torus∩torus** (site 10546, `tori.len() != 1`): R0096 — out of v1 scope.
- **Projection / solve failure** in a per-curve relocation helper: R0077.

**KEY FINDING (prototyped + reverted 2026-07-08):** a general conic
triple-surface junction handler (mirror the torus block: aggregate the ≤3
distinct incident surfaces from `inc0`, `relocate_onto_implicit_triple`, derived
1/sinθ displacement gate) is CORRECT and safe — it cleanly relocates the F0059
and R0019 junction vertices onto all three surfaces, holds **0 WRONG** and loses
no CORRECT across 78/294 assay cases, and only fires on mixed-curve junctions
(all currently ERROR ⇒ structurally zero-regression). **BUT it converts ZERO
cases on its own:** resolving the junction advances every target to a *deeper*
wall — F0059 → Stage-6 `NonManifoldOutput` (a T-junction, exactly the N2 CDT
gap), R0019 → a second junction, others unchanged. Moving a relocated vertex
without the Fig-11 CDT re-triangulation of its incident patch just relocates the
failure downstream. The prototype was therefore reverted (per P4/DoD: a
behavior-changing branch with no green reproduction test and 0 conversions is
not landable standalone).

**Conclusion — the junction handler and the N2 CDT mesh-update must land
TOGETHER.** The concrete next increment: (1) reinstate the general conic
triple-junction relocation (spec above), (2) wire `stage4_update::stage4_mesh_update`
(the built, unit-tested Fig-11 primitive) to locally re-CDT each moved vertex's
incident patch in the parametric domain so Stage 6 sees T-junction-free topology,
(3) recompute `d(T)` via `stage4_dt::d_of_t` for the new boundary triangles.
F0059 (cyl×cyl 90° union, ~0.4s, single clean junction family) is the canonical
red→green target. Oracle: assay 0 WRONG + F0059/R0019 ERROR→CORRECT.

**PARTIAL REMEDIATION — shipped closed-form junction handlers (2026-07-12
refresh).** The "junction handler + CDT must land together" conclusion above held
for the GENERAL mesh update, but the specific over-determined-junction sub-family
(a vertex lying on ≥3 exact surfaces, formerly the largest LRR sub-class) turned
out to be resolvable WITHOUT a local re-CDT, by relocating the vertex exactly onto
all its incident surfaces in closed form. These handlers shipped between tasks
#131–#146 and each converted its target cases to CORRECT with 0 WRONG. They
constitute the "partially remediated" status; the general §4.4.1 CDT remains
absent (see below):

- **Rim-junction insertion (Stage-1)** — `boolean.rs::rim_junctions_against`
  (`:1039`), dispatcher `rim_junction_overrides` (`:1632`), wired at
  `boolean.rs:1832`/`1855`, BRep constructors `brep.rs:322`/`:376`. Inserts exact
  lobe-corner junction vertices into the Stage-1 mesh before tessellation
  (spec `yang_rim_junction_insertion`).
- **circle∩line closed form** — `boolean.rs::circle_line_roots` (`:1171`,
  consumed `:1217`/`:1282`). The exact rim-plane quadratic for the junction point.
- **Triple relocation** — `stage4_relocate.rs::relocate_onto_implicit_triple`
  (`:251`): the torus-block Newton generalized to relocate a ≥3-surface vertex
  onto all constraints simultaneously (the reinstated general handler the 2026-07-08
  note prototyped; spec `yang_stage4_conic_triple_junction`).
- **circle × parallel-plane-line junction** — `stage4_relocate.rs::pp_line`
  (`:930`) + `pp_line_circle_junction` (`:981`), dedup `dedup_single_pp_line`
  (`stage4_correct.rs:3676`), map `vert_pp_circle_junction` (`stage4_correct.rs:1669`).
  See N30 (task #146).
- **cone-hyperbola junction (KV16)** — `stage4_relocate.rs::ConeHyperbolaReloc`
  (`:1076`), map `vert_cone_hyperbola` (`stage4_correct.rs:769`); geometry
  `geom.rs::{hyperbola_point,hyperbola_param}` (`:206`/`:259`).
- **cone-ellipse same-type junction (KV16/KV16b)** —
  `stage4_relocate.rs::ConeEllipseReloc` (`:1026`), `cone_ellipse_residual`
  (`:1210`), map `vert_cone_ellipse` (`stage4_correct.rs:761`), `same_type_junction`
  routing (`stage4_correct.rs:778`/`1739`). See N31 (task #127).

These share the exactness certificate `junction_certificate_band`
(`stage4_relocate.rs:70`/`:95`, `TAU_WORK.max(8·ε·L)`) and the scale-aware Newton
work floor `1e-13.max(8·ε·L)` (`stage4_relocate.rs:268`) — both documented as
tolerance decisions in N31.

**STILL OPEN — the general §4.4.1 CDT mesh-update remains UNWIRED.** The
faithful Fig-11 primitives built under N2-1/N2-2 —
`stage4_update::stage4_mesh_update` (`stage4_update.rs:89`) and
`stage4_dt::{eval_uv (:69), d_of_t (:101)}` — have NO production call sites at
HEAD; every caller is a `#[test]` (`stage4_update.rs` tests L469+, `stage4_dt.rs`
tests L649+, `tests/n2_dt_adversary.rs`). Stage-4 remains relocation-only: when
relocation + §4.5.3 correction cannot converge, the pipeline loudly STOPs with
`YangError::Stage4RegionInvalid { reason: LocalRefinementRequired }`
(`errors.rs:69`/`:123`) rather than doing the local CDT the paper prescribes —
~45 return sites across `stage4_correct.rs` plus `stage4_relocate.rs`
(`:826`/`:872`/`:1159`/`:1166`). The residual LRR cases (e.g. the true-degenerate
R0015 line∥circle-plane, torus∩torus R0096, and the mixed-curve junctions whose
relocation succeeds but whose incident patch then needs a T-junction-free
re-triangulation) are the remaining N2 work. Paper basis: §4.4.1 "Mesh updating"
(`refs/text/yang2025_hybrid_boolean.txt:605+`) + §4.5.2 "Local refinement"
(`:659+`).

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
provenance). **Progress (2026-07-01):** provenance attribution is now the PRIMARY
Stage-6 path (geometric is the fallback), fed by a per-triangle → face map
(`tri_face`) emitted by every Stage-0 producer: the inputs' own Stage-1
tessellation (1b), the planar coplanar overlay (2a), and — as of
commit for `specs/n4_coincident_cylinder_provenance.md` — the coincident-cylinder
membrane path (band-strip triangles attributed by azimuth to their arc-patch
face; `u32::MAX` sentinel → geometric fallback where a column has no covering
arc). Geometric attribution remains only for lineage-less / sidecar-backend
inputs; it can be RETIRED once those are the sole remaining consumers.
**RETIRED (2026-07-07, task #53, spec `specs/n4_retire_stage6_fallback.md`):**
the `YANG_N4_FALLBACK_PROBE` measurement proved ZERO fallback hits across the
full corpus on the native backend. A provenance MISS on a lineage-carrying
input (NoSourceEntry / too-short map / `u32::MAX` sentinel) is now a LOUD
`FaceResolutionFailed` — never a silent geometric guess. The geometric path
survives solely for LINEAGE-LESS attribution (documented contract): an
arrangement without `source` (the dev-only C++ sidecar oracle,
`tests/backend_parity.rs`; the in-crate mock-label fixtures) or an input
without a `tri_face` map (a yang boolean OUTPUT chained directly back in —
the yr27/F0066 pattern — or a `from_mesh` B-Rep).
**Sign-off:** resolved (provenance is the sole production path; geometric =
oracle/lineage-less contract only).

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

**UPDATE — single-coplanar-edge CONTAINED sub-config now CLASSIFIED (this PR):**
`checkSingleCoplanarEdgeIntersections` (cpp:422-657) is now ported for the
**edge-contained** sub-config of the single-coplanar-edge case. When exactly one
edge of one triangle lies in the other's plane (`singleCoplanarEdge`, orBA/orAB
sign triple with two zeros) AND both endpoints of that coplanar edge lie on the
**closed** other triangle (ON_VERT / ON_EDGE / STRICTLY_INSIDE) with NO proper
edge-edge crossing, `classify_pair` now emits the two endpoints as `Explicit`
intersection vertices (placed onto the other triangle's correct edge/interior by
`group_intersection_points`, joined into the symbolic segment by
`group_constraint_segments`) — replacing the loud `Deferred(SingleCoplanarEdge)`.
The whole-arrangement consequence: a contained single-coplanar-edge solid pair
that previously raised `ArrangementError::CoplanarPairDeferred` now builds the
arrangement. New clean-room finer predicates (`predicates::simplex_location`:
`point_in_segment_3d`, `point_in_triangle_3d_loc`, `segment_segment_intersect_3d`,
all EXACT via Shewchuk `orient2d`/`orient3d` + `dashu` betweenness) supply the
ON_VERT/ON_EDGE discrimination the coarse `point_in_triangle_3d` lacked
(resolving this entry's "Secondary note"). Reference parity verified at the
arrangement level (vertex-set Hausdorff-0 vs the C++ `mesh_booleans` sidecar) in
`crates/cherchi-rs/tests/single_coplanar_edge_parity.rs`.

**UPDATE 2 — single-coplanar-edge edge-CROSSING sub-config now CLASSIFIED
(this PR):** the **edge-CROSSING** sub-config is now ported. When the coplanar
edge enters/exits the other (convex) triangle through one of its edges, the
coplanar edge ∩ other-triangle is a single sub-segment `[P, Q]`; each of `P, Q`
is either a coplanar-edge endpoint on the closed other triangle (`Explicit`, the
contained path) or an in-plane crossing of the coplanar edge with one of the
other triangle's edges. The crossing is a new `IntersectionVertex::EdgeEdge { e,
f, jolly, approx }` (the C++ `addEdgeCrossEdgeInters` jolly-LPI, cpp:285-318/
557/589/621): its EXACT coordinates are the line `e` ∩ the plane through
`[f0, f1, jolly]` (geometrically **jolly-INDEPENDENT** — any out-of-plane jolly
gives the same in-plane e×f crossing; the jolly only makes the plane
non-degenerate and only affects the `approx` readback). The jolly is the FIRST
of the four regular-tetrahedron jolly directions (scaled generously relative to
the live edge magnitude) with `orient3d(f0, f1, j, f_other) != Zero`, matching
the C++ `noCoplanarJollyPointID` arg order (cpp:406-418).

`group_intersection_points` interns the `EdgeEdge` as `Lpi { line: e, plane:
[f0, f1, jolly] }` (so it shares one geometric id with any coincident `Explicit`/
`Lpi` via the exact-coordinate interner) and places it on the edge bucket of
BOTH owners — the coplanar edge `e` AND the other-triangle edge `f` (cpp
`addVertexInEdge(e0_id, ..) + addVertexInEdge(e1_id, ..)`), NOT through
`pierced_triangle` (the jolly plane is not a triangle). This resolves UPDATE 1's
"dual-edge placement not yet representable" note. The in-plane proper-crossing
test projects to the dominant-normal plane of the other triangle and uses exact
`orient2d` cross-side signs — avoiding the axis-aligned degenerate-projection
blind spot of the generic 3D `segment_segment_intersect_3d` (which collapses a
coplanar edge parallel to a dropped axis to a point). Reference parity verified
at the arrangement level (vertex-set Hausdorff-0 vs the C++ `mesh_booleans`
sidecar) in `crossing_single_coplanar_edge_arrangement_parity`.

**STILL DEFERRED (loud, P9/P10):** (1) sub-configs where an other-triangle
**vertex lies strictly inside the coplanar edge** (the C++ `tvX_in_edge`
symbolic-segment branches, cpp:545-547/570/602/634) or the coplanar edge is
**collinear / overlapping** with an other-triangle edge — these need the
cross-edge symbolic-segment bookkeeping not constructed here; `classify_single_
coplanar_edge` returns `None` → `Deferred(SingleCoplanarEdge)`. (2) the
**fully-coplanar** (`allCoplanarEdges`, orBA `0 0 0`) case (out of scope; Yang
Stage-0 / M8) → `Deferred(Coplanar)`. Never a guessed result.

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

**Deviation 2 (EE-bool asymmetry repair, faithful) — RESOLVED at PR-CR-M7c:**
`point_inside_segment` used to query the FFI `pointInInnerSegment` in **both**
endpoint orders and OR them, because the sidecar's `lessThanOnX/Y/Z`
explicit-explicit branch (`implicit_point.hpp:73/83/93`) returns a C++ `bool`
(0/1, never −1), making the single call endpoint-order-sensitive for explicit
segments. The clean-room native `point_in_inner_segment_indirect`
(`predicates::indirect`, PR-CR-M7b) is symmetric in `v1 ↔ v2` by construction
— every comparator arm, including explicit-explicit, is a true signed
−1/0/+1 — so the M7c consumer swap collapsed the fwd||rev pair to ONE call
with identical semantics (the FFI EE bool-quirk has no production call path
anymore; it remains documented in the dev-only sidecar smoke tests and is
mapped explicitly in `tests/indirect_catalog_ffi_parity.rs`). Relatedly, the
native orient3d uses the Shewchuk sign convention — the MIRROR of the FFI's —
which is an internal convention, not a deviation: both M7c production uses
(inside_out straddle / behind-seed-plane tests) are sign-relative and
annotated per-site.

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

**AR3c amendment (2026-06-10) — interning is now geometric at SOURCE.** The
post-hoc `canonicalize_points` pass ran AFTER `group_constraint_segments`, which
was too late: a pair whose intersection-segment endpoint lies ON an edge of the
pierced triangle can be re-derived under the swapped pair presentation with
DIFFERENT generator tuples (AR1's `li.size() > 1` early-out fires in only one
direction), so structural interning over-counted to 3 ids for 2 geometric
points and the `ids.len() != 2` guard SILENTLY dropped the pair's constraint
segment from BOTH triangles — making `mesh_arrangement` input-order-DEPENDENT
on closed intersection loops (4 of the through-cut's 16 fence edges unrealized
under reversed/swapped presentations → BL1 flood leaks, 6 patches → 2).
PR-CR-AR3c folded the exact-coordinate identity INTO the interner
(`aux_structure.rs::PointInterner`, keyed by pure-`dashu` exact rational
coordinates with the first-encountered tuple as representative, mirroring the
C++ `aux_structure.cpp:230 addVertexInSortedList` / `genericPoint::lessThan`
exact-geometric global vertex list), and `group_constraint_segments` resolves
endpoints by the same geometric keying. `canonicalize_points` was removed as
redundant. A `Transversal` pair resolving to >2 distinct GEOMETRIC endpoints is
now a loud typed error (`ConstraintSegmentError::TooManyGeometricEndpoints` →
`ArrangementError::TransversalEndpointOvercount`, mirroring the C++
`final_check` assert); 0/1 endpoints remain legitimate no-segment cases.
Oracles: `aux_structure::ar3c_tests` (minimal pair-order anchor),
`soup::ar3c_tests` (stage-level + end-to-end presentation invariance), and the
un-ignored `adversary_b_generated_ray_permutation_invariance` witness.

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

### N21 — PR-KV4-F1 in/out: rational-ray fallback where the C++ exits ("requires exact rationals")

`findRayEndpoints` (booleans.cpp:504) has two f64 origin strategies: an
explicit non-border patch vertex, else a generated ray from a patch
triangle's approximated centroid validated by exact straddle/strictly-
inside predicates. When both fail the C++ prints "WARNING: the arrangement
contains a fully implicit patch that requires exact rationals for
evaluation. This version of the code does not support rationals…" and
`std::exit(EXIT_FAILURE)` (booleans.cpp:578).

The port implements the missing branch (`rational_ray_inner_label`,
`labeling/inside_out.rs`): the patch origin is the EXACT rational centroid
of a patch triangle (explicit coords are exact f64→RBig; implicit coords
from the exact lambda tier) — strictly interior by positive exact area, so
the border restriction does not apply — and the axis-ray crossing
parameter, strictly-inside test, hit ordering, and nearest-hit orientation
rule (`inner ⇔ n_k > 0`, the exact-domain reduction of the f64 path's
`orient3d(tv, v1) == Negative`) are evaluated in pure rational arithmetic
over ALL `in_tris` (no octree — the fallback fires only on pathological
patches). Exact grazes retry the next axis (X→Y→Z); exhaustion is the loud
typed `RationalRayDegenerate`. Set semantics mirror the f64 sort: equal
exact parameters collapse to the first in ascending triangle order, and
`t ≤ 0` hits are discarded (N20 included).

Trigger class: sub-f64-resolution NEEDLE patches — an input edge piercing
a triangle femto-close to its corner mints an intersection point below one
ulp from an existing vertex (chained oblique planar inputs, the F0016
corpus family). Every explicit vertex of such a patch is a border vertex
and its f64-approximated triangle is too thin for any f64 segment to pass
strictly inside, so the C++ would exit on real Waffle corpus geometry.
Parity note: no differential target exists for this branch (the reference
terminates); correctness is carried by the needle-fixture oracles
(`inside_out.rs` Oracle #6), the orientation-convention pin test, and the
F0016-family corpus flips (5 cases → SUPPORTED_CORRECT).

### N22 — Stage-6 degenerate-arrangement children: fold-sliver exclusion + loop T-subdivision

Yang 2025 §4.5 topology extraction assumes clean same-face regions. The exact
mesh arrangement, however, keeps ZERO-AREA shim slivers along shared collinear
solid-edge chains (they pair their edges into the watertight result, so
dropping them would break edge pairing). The paper does not treat these
degenerate children. When `canonicalize_vertices_to_planes` aligns chained-
output vertices onto exact plane intersections, a parent input triangle's CDT
can emit such slivers with sign-of-zero winding that DUPLICATES a real
triangle's directed chord edge, folding the Stage-6 boundary walk into a
spurious `NonManifoldOutput` (measured F0016 Extrude-3 union; spec
`specs/yang_stage6_sliver_topology.md`).

Deviation (both parts Stage-6-local, no geometry moved, no tolerance invented):

- **Fold-sliver exclusion** (`patch_fold_slivers`, `patch_boundary_cycle`): a
  degenerate sliver (2·area < MIN_FEATURE_SIZE², the shared A14.3 threshold)
  whose sign-of-zero winding duplicates another patch triangle's directed edge
  (directed multiplicity ≥ 2) carries no information and is excluded from
  boundary derivation. A degenerate sliver whose edges instead pair
  anti-parallel with their neighbours — a femto-twin membrane welding two
  coincident vertices — is NOT a fold and is KEPT (excluding it would promote a
  legitimately-interior real edge to a false boundary and diverge from the C++
  reference arrangement, which does carry the membrane). This distinction is
  what keeps curved / twin output at reference parity.

- **Loop T-subdivision** (`subdivide_loops_at_shared_vertices`): after boundary
  cycles are built, a fold-sliver-bearing patch's un-subdivided chord (a,b) is
  split at every output vertex lying STRICTLY on segment a–b (pure rational
  collinearity + betweenness) that is used by some other output loop, so every
  segment of the shared solid edge is used by exactly two directed loop edges.
  This mirrors, at the B-Rep level, the render-side hybrid oracle's T-junction
  subdivision (`test-harness subdivide_t_junctions`).

Parity note: excluding a zero-area triangle from BOUNDARY derivation is not a
tolerance decision — the sign-of-zero winding is combinatorially arbitrary.
Corpus-improving (F0016/F0024 canon-wired and F0022 unwired flip
non-2-manifold ERROR → 9-oracle Passed); every non-sliver output is
byte-identical (A no-ops without a fold sliver; B no-ops without an on-segment
foreign vertex).

### N23 — Patch-label flood tolerates COMPATIBLE (subset) labels at coplanar-sheet borders; DISJOINT stays loud

Cherchi 2022 `computeSinglePatch` (booleans.cpp:426) floods triangles across
manifold edges into a patch and, in debug builds, asserts
`labels.surface[t_id] == ref_l` (seed's surface label). Our port
(`labeling/patches.rs::compute_all_patches`) hardened that debug `assert` into
a production `Err(PatchError::LabelMismatch)`. That is STRICTER than the release
reference: the shipped `mesh_booleans` binary is NDEBUG (assert compiled out),
so on a coplanar boolean it floods across the manifold border between a merged
`[A,B]` overlap sheet and the single-input `[A]` region it extends, and the
patch simply keeps the seed's label (`labels.surface[*patch_tris.begin()]`,
booleans.cpp:629). This false-rejected the R0046/R0088/F0063 class
(`specs/cherchi_patch_label_tolerance.md`, task #14) — measured by reference
parity: C++ SUCCEEDS on R0046's exact post-Stage-0 meshes (same 210-triangle
arrangement, 5 patches, patch 0 mixes 108 `[A]` + 8 `[A,B]` triangles) where our
port asserted. The `[A,B]` sheet's border to the `[A]` region is a 2-incident
MANIFOLD edge because the B-side coincident triangles are dedup'd into the
sheet, so the flood legitimately crosses it.

Deviation — L2a/L2b split (patch-label check, `patches.rs`):

- **L2a (tolerate compatible):** when a flooded triangle's canonical surface
  label is COMPATIBLE with the seed's (one set ⊆ the other — the `[A,B]` sheet
  extending `[A]`, either direction), continue flooding and keep the seed's
  label. This matches the RELEASE reference exactly on the measured class.
- **L2b (disjoint stays loud):** when the labels are DISJOINT (neither ⊆ — e.g.
  `[A]` vs `[B]`), keep the loud `Err(LabelMismatch)`. This is deliberately
  STRICTER than the release reference (which silently mixes): a disjoint label
  across a manifold edge is a genuine arrangement corruption, and the safe
  direction under crate P9 doctrine is to stay loud (the debug-build C++ would
  assert here too). Documented as a knowing, safe-direction deviation from
  release C++.

Correctness burden (not a tolerance hack): the mixed patch's label feeds only
the per-patch ray-cast in/out (BL2); coplanar `[A,B]` keep decisions are made by
the per-triangle rules (booleans.cpp:1430/1468), independent of the patch label
(`propagateInnerLabelsOnPatch` writes `inside`, never `surface`). Validated by
sidecar output parity on the R0046-class fixture (I1) plus the full assay
(0 WRONG, no SUPPORTED_CORRECT lost). Non-coplanar arrangements are
label-homogeneous and byte-identical (L1). Precedent: N20 (C++ release-behavior
analysis); the deviation-policy memo (`cherchi_rs_cpp_deviation_policy`).

### N24 — orient2d/orient3d: exact-rational zero-certification (Shewchuk underflow hole)

**Where:** `cherchi-rs/src/predicates/orient.rs` (`orient3d`, `orient2d`).
**C++ behavior:** the reference (and our former wrapper) trusts the
Shewchuk-style adaptive predicate's 0.0 as a certified Zero. Shewchuk's
exactness guarantee explicitly excludes UNDERFLOW: a determinant whose true
magnitude lies below the subnormal floor collapses to exactly 0.0 in the
expansion arithmetic.
**Measured trigger (KV9-F1, spec `kv9_f1_tangency_inout_labels` §2a):** the
steinmetz in/out ray's entry graze on a femto-skewed azimuth-π lateral edge.
All 8 ULP-perturbed rays produced true determinants ≈ 0.36·5e-324 —
underflowing to a FALSE Zero — so the graze resolver dropped a real
crossing, flipped the patch parity, and the boolean silently discarded ALL
of input B (the C++ escapes on this fixture only by making different
ray/vertex choices; the hole is present there too).
**Port behavior:** an adaptive tier may certify NONZERO signs; only exact
arithmetic certifies Zero. A 0.0 adaptive result is re-derived in dashu
rationals (identical formula orientation — `det[a−d, b−d, c−d]` /
`(b−a)×(c−a)` — preserving the Shewchuk sign conventions). Nonzero adaptive
results are returned untouched, so every non-degenerate call is
byte-identical; truly-degenerate inputs still certify Zero (now soundly).
**Exactness direction:** STRENGTHENING (never diverges from the true sign;
diverges from the C++ only where the C++ is unsound under underflow).
**Oracles:** `orient.rs` group-0 unit tests (measured 4-point fixture +
2D analog + true-coplanar guard); full cherchi suite + 18/18 sidecar
arrangement parity + fuzz differentials + full assay unchanged.

---

<!-- 2026-07-12 catch-up (task #150): entries N25–N35 backfill the M8 coplanar,
KV6 revolve, and Stage-4 junction campaign (tasks #131–#146) that landed between
commit d7da34ae and HEAD 3568db09. Per the 2026-07-12 governance amendment,
deviation entries are MERGE BLOCKERS per increment; these are the retroactive
records for increments that shipped before the amendment. Anchors verified by
`grep -n` at HEAD; any anchor that could not be confirmed is flagged inline as
"(anchor unverified — flagged in 2026-07-12 catch-up)". Several of these are
faithfulness *improvements* or new-capability constructors, not divergences —
each entry states its deviation status explicitly. -->

### N25 — Stage-0 §4.5.5 generalized to n-ary plane groups + tessellated (disc/annular/mixed) faces

**Where:** `crates/yang-rs/src/stage0/nary.rs` — `build_plane_groups` (`:53`,
connected components of the coplanar-pair graph), `PlaneGroup` (`:41`),
`overlay_nary_group` (`:142`); pure-line class wall probes at `:189`
(`nary-face-unsupported`) / `:217`,`:226` (`nary-mixed-orientation`); tessellated
class in the same file — `face_polygon_2d_tessellated` (`:262`),
`rim_chord_ctxs`/`mixed_chord_ctxs` (`:401`/`:392`), disc/annular gates (`:396`/`:397`),
sub-floor shared-mint collapse (`:481`), crossing propagation (`:726`,
`collect_rim_crossings` `:745`, `collect_mixed_crossings` `:737`), the B6 loud wall
`annular-hole-rim-crossing` (`:196`). Dispatch from `stage0/mod.rs`:
`build_plane_groups(&scan.cross)` (`:248`), per-group loop (`:308`),
`overlay_nary_group(` call (`:313`), typed wall `CoplanarFacesUnsupported` (`:205`).

**Mechanism:** §4.5.5 coplanar preprocessing formerly handled exactly one
coplanar A×B face pair; a face appearing in more than one pair tripped a loud
wall. This lifts that wall by grouping coplanar cross-pairs into PLANE GROUPS
(connected components joined by a shared face) and running ONE exact-rational
2D overlay per group (side A = all its A faces, side B = all its B faces) so a
repeated face is segmented against the union of its partners in a single
consistent triangulation. A 1-pair group runs the historical 1×1 path
byte-identically. Slice g (task #132) extends the group overlay from
pure-`LineSegment` faces to the DISC / ANNULAR / MIXED Line+Arc tessellated
classes the 1×1 path already supported, wiring exact Stage-1 rim rings, on-circle
chord-mint contexts, sub-floor shared-mint collapse over every group rim circle,
attribution-scoped per-face override triangulations, and per-face crossing
propagation into the laterals. A disc-rim × annular-hole-rim strict crossing
stays the loud 1×1 wall, applied pairwise across the group.

**Paper section:** faithful implementation of §4.5.5 "Handling coplanarity"
(`refs/text/yang2025_hybrid_boolean.txt:718-751`) — the set-level
A-only/B-only/overlap segmentation of the shared plane, generalized from a pair
to the connected group. NOT a divergence.

**Tolerance decisions:** none new. Per-pair detection uses the pre-existing YR24
weld band; no epsilon is introduced (mints are closed-form circle∩line / radial
projection). The volume oracle allows a 6% chord band for Stage-1 rim sag.

**Oracles:** e2e `crates/yang-rs/tests/m8_bridge_nary_overlay.rs`
(`narrow_bridge_union_is_genus1_frame` χ=0, `..subtract_leaves_u_exactly` χ=2,
`..intersect_is_empty`, `user_bridge_union_is_genus1_frame`);
`m8_nary_tessellated_overlay.rs` (`flush_pocket_subtract_and_union_partition`,
`plain_flush_pocket_still_succeeds`, `group_with_crossing_and_contained_rims_succeeds`,
`single_boss_crossing_1x1_regression`); engine unit `yr25_coplanar_overlay.rs`
(`nary_two_towers_vs_spanning_bridge`, `nary_singleton_delegation_is_bit_identical`,
`nary_overlapping_same_side_inputs_are_loud`). Assay: **C0101** pinned
`SupportedCorrect` (`assay_kv2.rs:1203`). **R0046** (slice-g driver) is NOT
explicitly pinned in `assay_kv2.rs`; the spec anticipates it lands CORRECT or on
the deeper `rim-lateral-none` torus-lateral wall (N28), and the catalog line
`R0046 — FAIL` appears stale relative to the current baseline (anchor unverified
— flagged in 2026-07-12 catch-up).

**Deviation status:** faithful §4.5.5; design decision = plane-group
partitioning. No sign-off required.

### N26 — Overlay f64-emission fused collapse (§4.5.5 identical-mesh at rounding resolution)

**Where:** `crates/yang-rs/src/coplanar_overlay.rs` — published `fused`
map (`:190`), step-6 emission gate (`:604`), `CollinearSliver` trigger
(`:634-638`), `fused_emission_repair` call (`:651`) and def (`:696`), loud
`RoundingCollapse` fallback (`:671-673`), eligibility ceiling `tau2` (`:708-710`),
candidate sort by exact squared length (`:769`), ceiling test `if len2 >= &tau2`
(`:772`), survivor selection (`:778-786`), exact link/fold validity gate (`:797`+).

**Mechanism:** at the overlay's f64 emission gate, when a triangle of three
distinct EXACT vertices rounds to a degenerate/collinear f64 image
(`CollinearSliver`), a constrained-edge-collapse repair runs instead of failing.
Worklist = non-`Positive` triangles ascending; each tries its edges in ascending
exact squared-length order (lexicographic tie-break). An edge is eligible only if
its exact squared length is below the ceiling (real-scale slivers stay loud). The
survivor is the input-loop vertex over a minted arrangement vertex, else the
smaller overlay index, keeping its OWN exact bits (never an average — the KV15b
precedent). A Hoppe-style link/fold validity gate remaps loser→survivor over all
live triangles in exact arithmetic: index-degenerate triangles are dropped, every
other remapped triangle must retain strictly positive exact area or the candidate
is rejected. A full pass committing nothing while a sliver remains ⇒ loud
`RoundingCollapse`. Publishes `fused: BTreeMap<u32,u32>` (fully resolved
loser→survivor). No-sliver inputs are byte-identical, `fused` empty.

**Paper section:** serves the §4.5.5 identical-overlap-mesh requirement
(`...txt:718-751`) at f64 resolution — coincident-rounded boundary chains must
fuse to keep both models' meshes identical. Method references [#51] Hoppe 1996
(edge collapse + link/fold) and [#52] Hobby 1999 (snap-rounding) are supporting,
not Yang sections.

**Tolerance decision (design choice, logged):** the fusion eligibility CEILING is
`TAU_MODEL` (`1e-7`, `crates/cad-primitives/src/lib.rs:23`), applied squared
(`tau2`, `coplanar_overlay.rs:708-710`). This is deliberately a fail-closed
ceiling on what MAY fuse, NOT a trigger (the trigger is exact f64 degeneracy) and
NOT `MIN_FEATURE_SIZE` (the R0091 revert lesson). Sign-off rationale: fusing only
sub-`TAU_MODEL` exact separations cannot merge a real feature.

**Oracles:** `crates/yang-rs/tests/m8_overlay_femto_slab_emission.rs`
(`c0048_mirrored_rim_slab_repair` verbatim C0048 pair, `synthetic_femto_slab_fuses`,
`needle_only_overlay_byte_identical_legacy`, `supra_tau_collinear_stays_loud`,
`fusion_survivors_prefer_input_loop_vertices`, `femto_slab_coexists_with_supra_hole_feature`,
`fused_output_is_finite_and_nondegenerate`, `nan_and_degenerate_inputs_rejected_before_gate`);
`yr25_coplanar_overlay.rs::rounding_stress_subresolution_sliver_fuses`. Assay:
F0067/C0048/R0053 leave the `RoundingCollapse` wall for their next honest wall
(C0048 → the kernel-v2 rim-override refusal that N27 retires; C0048 remains
`Unsupported(CoplanarBoolean)` pinned at `assay_kv2.rs:1169`). F0067/R0053 have no
explicit assay pin (anchor unverified — flagged in 2026-07-12 catch-up).

**Deviation status:** faithful §4.5.5 with a documented fail-closed tolerance
ceiling. Design decision, no separate sign-off.

### N27 — Stage-1 rim-override merge onto a coinciding uniform sample (producer/consumer of N26)

**Where:** `crates/yang-rs/src/stage1_tessellate.rs` —
`stage1_tessellate_with_rim_overrides` (`:81`), `stage1_tessellate_inner` (`:118`),
`inserted_rims` set (`:126`). Full-rim site: `uni_step`/`merge_tol` (`:521-522`),
coincidence test (`:564-565`), seam (k=0) B-Rep guard (`:567-580`), same-slot /
already-merged conflict (`:582-590`), TAU_MODEL identity check `d2 >= tau*tau`
(`:604-615`), MERGE bit-replace `slots[k_slot] = (…, RimSlot::Override(pt))`
(`:622`). Arc-chain site: `uni_step`/`merge_tol` (`:358-359`), `k_near` (`:395-396`),
endpoint/seam refusal (`:406`), same-slot conflict (`:419`), identity ceiling
(`:434`), real-scale refusal (`:436`).

**Mechanism:** a rim-crossing override point (the fused boundary point from N26)
that angularly COINCIDES with an interior uniform rim Steiner sample (slot k≠0)
and is a sub-`TAU_MODEL` 3D twin of that sample is DELIBERATELY MERGED — the
uniform slot's computed sample is replaced by the override's exact bits while the
slot keeps its uniform angular key + theta. Ring length is unchanged
(replacement, not insertion), so the uniform (N−k) lateral index-pairing stays
valid and the rim is kept out of `inserted_rims` (not routed to azimuth-merge).
This retires the old "coinciding override always = upstream bug ⇒ silent-merge
refused" wall. Fail-closed guards stay loud (`MalformedTopology`): real-scale
coincidence (3D distance ≥ TAU_MODEL), two distinct overrides on one slot, or a
differing-bits collision on the seam / arc endpoint (B-Rep vertices are
authoritative). Applies at both the full-circle rim and arc-chain sites.

**Paper section:** §4.5.5 shared-boundary-point propagation
(`...txt:718-751`) — the single fused overlap-boundary point must appear in this
body's rim sampling for the two meshes to stay identical. Faithful.

**Tolerance decisions (design choice, logged):** angular trigger
`merge_tol = uni_step * 1.0e-6` (`:359`, `:522`) — the pre-existing coincidence
band; identity ceiling = `TAU_MODEL` (`1e-7`), compared as squared 3D distance
`d2 >= tau*tau` (`:604`/`:614`, arc `:434`). No new constant; both from the
centralized policy. Same fail-closed rationale as N26.

**Oracles:** `crates/yang-rs/src/tests_unit/boolean_functional.rs` —
`rim_override_ulp_twin_merges_onto_uniform_sample` (`:687`: ring length unchanged,
twin bits present, displaced bits gone, edge NOT in inserted set, closed
2-manifold), `rim_override_bit_exact_uniform_merge_is_byte_identical` (`:731`),
`rim_override_real_scale_uniform_coincidence_stays_loud` (`:757`), plus existing
`rim_override_empty_is_byte_identical` / `rim_override_inserts_into_both_rims_no_t_junction`.
Assay: C0048 (shared driver with N26) still `Unsupported(CoplanarBoolean)` — the
wall keeps moving downstream one honest step at a time.

**Deviation status:** faithful §4.5.5; design decision. No sign-off required.

### N28 — Torus-profile rim crossings: CapLateral torus arm + poloidal opposite-rim projection

**Where:** `crates/yang-rs/src/stage0/rim_chords.rs` — `enum CapLateral`
(`:370`), `lateral_for_cap` (`:387`; cylinder arm `:424`, torus guard
`rim-lateral-torus-not-2profile` `:451`, torus arm `:459`, `rim-lateral-none`
`:482`), `collect_ring_crossings` (`:563`; torus arms `:578`/`:731`; poloidal
φ = atan2(τ, ρ−R) `:678-679`). Grid alignment in
`crates/yang-rs/src/stage1_tessellate.rs` — `tessellate_torus_face` (`:3204`),
`phi_slot` closure (`:3333`), `tessellate_torus_band` (`:4472`).

**Mechanism:** when a disc cap's rim edge is a torus profile circle
(revolved-circle output) and the coplanar overlap boundary crosses it, the
crossing must be mirrored onto the OPPOSITE profile circle so both rims of the
torus band keep matched sample counts. `lateral_for_cap` was generalized from a
cylinder-only classifier into a `CapLateral` enum with a TORUS arm that detects a
`Surface::Torus` face whose outer loop carries exactly two distinct full-circle
rims of radius ≈ minor. For a crossing on one profile circle it computes the
intrinsic poloidal angle φ = atan2(τ, ρ−R) and mints the opposite point exactly at
the same φ on the opposite circle via `c₁ + r₁(cos φ·u + sin φ·a)` (1:1, no grid
search). `tessellate_torus_face` then aligns its structured θ×φ grid columns by the
rings' actual intrinsic φ values (index-wise on sorted seam-anchored offsets)
instead of assuming uniform slots, keeping the mesh watertight against both seam
discs.

**Paper section:** faithful §4.5.5 (`...txt:718-751`) — overlap boundaries become
shared intersection curves and the boundary sampling must propagate into every
face sharing the subdivided edge.

**Tolerance decisions:** the ring-match uses a fixed band (index-wise on sorted
seam-anchored offsets); the spec explicitly REJECTS a min-gap-derived tolerance
(R0050's Δφ≈9e-16 vs a 4e-16 twin gap would collapse it). The exact `1e-9` literal
for this ring-match band and the `1e-9·(1+R+r)` minor-radius classification band
are documented in the spec but were NOT isolated to a unique source line in this
pass (anchor unverified — flagged in 2026-07-12 catch-up).

**Oracles:** `crates/yang-rs/tests/kv6d_torus_boolean.rs`
(`flush_box_crossing_seam_disc_union` RED→green, `flush_box_contained_in_seam_disc_union`);
unit `crates/yang-rs/src/tests_unit/stage0_rim_projection.rs`. Assay: **R0046**
UNSUPPORTED→CORRECT; **R0025/R0050** advance to typed Stage-4
LocalRefinementRequired (the N2 class); **R0085** was formerly masked by an
UNSUPPORTED(revolve) verdict and now surfaces the pre-existing CDT wall.

**Deviation status:** faithful §4.5.5. No sign-off required.

### N29 — §4.5.3 reversed-point correction via EXACT conic parameters (not the paper's discrete tangent-angle proxy) + near-tangent ellipse relocation

**Where:** `crates/yang-rs/src/stage4_correct.rs` —
`sweep_reversed_intersections` (`:3402`, call `:2978`), param-order test
`d1 * d2 < 0.0` (`:3567` in-sweep, `:3785` in `shared_conic_reversed`),
`conic_param_deltas` wrap closure (`:3788-3816`), `mixed_cycle_shared_conic`
(`:3824`), `conics_equal_up_to_normal_sign` (`:3705`), `is_reversed` (`:3968`),
`reversal_collapse_direction` (`:3894`). Near-tangent ellipse arm:
`project_onto_ellipse_via_cylinder` (call `:2082`, def `stage4_relocate.rs:789`),
`project_onto_ellipse_nearest` (call `:2101`, def `stage4_relocate.rs:865`),
`ellipse_residual` (def `stage4_relocate.rs:1266`, used `:2058`).

**Mechanism:** in a MIXED boundary cycle (solid edges + conic chain), a site `p_r`
whose two incident edges carry the SAME conic (Circle/Ellipse, identity up to
stored-normal sign) is tested for parameter-order reversal — compute the three
points' conic parameters, wrap consecutive deltas `d1 = t_r−t_b`, `d2 = t_n−t_r` to
(−π,π], and flag a backtrack iff `d1·d2 < 0`. The victim collapses onto the
parameter-NEARER bracketing neighbor (not `reversal_collapse_direction`, which
picks the far junction) under a `2·d_ε` gate. A second, deeper arm fixes the
near-tangent ellipse relocation: `project_onto_ellipse_via_cylinder` preserves
cylinder azimuth and amplifies by `1/(n·â)`, silently sliding a corridor vertex
macro-distances; when the azimuth move exceeds the per-site gate it is replaced by
the in-plane nearest point on the ellipse — found by BISECTION of
`f(t)=(a²−b²)cos·sin − |u|a·sin + |v|b·cos` on `[0,π/2]`, NOT Newton (the F0047
divergence) — accepted only if its move ≤ `2·gate/sinθ`.

**Paper section:** DEVIATION FROM LITERAL, faithful to INTENT. §4.5.3 "Correction
of reversed intersection points" (`...txt:679-751`, Fig. 15) prescribes a
discrete tangential-direction analysis (a 45°–135° angle band on sampled
tangents). This implements the same correction via EXACT conic parameters instead
of the discrete-tangent proxy — a strengthening (the exact parameter order can
never mis-diagnose a reversal the angle band would blur). Analogous in spirit to
N24: diverges from the literal method only where the literal method is a
resolution-limited approximation.

**Tolerance decisions (design choice, logged — the "2·d_ε/sinθ corridor budget"):**
the near-tangency relocation corridor budget `2·gate/sinθ` is the acceptance band
for a relocated point's move, first-order from the 1/sinθ distance-to-curve
amplification of a near-tangent surface pair. It appears at ~5 sites in
`stage4_correct.rs`: `:1782` (pp-planes∩cylinder junction, `2·d_eps/sin_theta`),
`:2032` (coplanar circle∩circle corner), `:2128` (single-ellipse relocation,
`2·gate/sin_theta`), `:2286` (line metric `2·d_eps/grad`), and the `:2529-2533`
residual gate. The `2·d_ε` collapse gate is at `:3625`/`:3630`. These are labeled
in-code as "the torus-block metric — NOT a tolerance widening": the budget is the
provable first-order bound on how far an exact junction can lie from the
inscribed-mesh estimate, not an accept-anything slack. `ellipse_residual` is a
SURFACE metric (a flat move gate over-rejects — the kv11 pin), and nearest-on-
ellipse is bisection, not Newton-from-atan2.

**Oracles:** `crates/yang-rs/src/tests_unit/m5_case_iv.rs`
(`s453d_shared_circle_backtrack_reversed`, `s453d_steep_ellipse_peak_monotone_is_healthy`,
`s453d_conic_identity_up_to_normal_sign`, `s453d_shared_conic_site_eligibility`,
`s453e_near_tangent_ellipse_nearest_projection_bounded`,
`s453_merge_survivor_prefers_exact_vertex`); regression pins in
`crates/yang-rs/tests/n2_rim_mint_adversary.rs`
(`corner_in_band_box`, `corner_in_band_reverts_keep_true_junction`). Assay:
**R0061** RED→GREEN driver, **R0059/R0072** joined CORRECT (class siblings
R0063/R0095/F0085); baseline 238C/0W/53E/4U/0T.

**Deviation status:** documented strengthening deviation from the §4.5.3 literal
method; the corridor-budget tolerance is a design decision with the provable
first-order rationale above. No user sign-off outstanding (strengthening
direction; identical to N24's posture).

### N30 — Circle × parallel-plane-line junction closed form (§4.4.1 relocation onto both incident curves)

**Where:** `crates/yang-rs/src/stage4_correct.rs` — `vert_pp_circle_junction`
map (`:1669`), rerouting pass (`:1672-1697`), `dedup_single_pp_line` (call
`:1684`, def `:3676`), relocation loop (`:2586`), `pp_line_circle_junction` call
(`:2588`). `crates/yang-rs/src/stage4_relocate.rs` — `pp_line` (`:930`),
`pp_line_circle_junction` (`:981`). Per-vertex maps `vert_circle` (`:753`),
`vert_pp_planes` (`:816`).

**Mechanism:** a vertex registered in BOTH `vert_circle` (a section circle) and
`vert_pp_planes` (an exact plane∩plane trace line) is a triple point where the
pp-line crosses the circle. A rerouting pass (after the KV11 ellipse×pp pass,
before PR-F3 line×circle) dedups the vertex's pp entries — exactly one distinct
line ⇒ remove from `vert_circle`, insert `(line, circle)` into
`vert_pp_circle_junction`; ≥2 distinct lines ⇒ loud `LocalRefinementRequired`.
The relocation arm solves the pp-line ∩ sphere(C,r) quadratic (chosen over the
transversal plane-piercing form because it is exact for BOTH in-plane and
transversal configs — PR-F3's piercing form is degenerate for the in-plane
class), picks the root nearer the current position that also passes the
circle-plane residual band, retags `t` via `project_onto_circle`, and STOPs loudly
if the displacement exceeds the junction gate. Sibling of the KV11 ellipse×pp
reroute, whose ellipse-only handling previously let the plain `vert_circle`
relocation slide the vertex along the circle off the pp-planes (a Newell-normal
disagreement one op downstream).

**Paper section:** faithful §4.4.1 "Mesh updating" (`...txt:605+`) — a point
terminating two intersection curves must satisfy both; also uses [#1]
Patrikalakis line–quadric closed forms.

**Tolerance decisions:** none new. Reuses the derived junction gate `2·d_ε/sinθ`
(N29 family; sinθ=0 ⇒ INFINITY) and the `OffCurveBeyondChordBand` reject.

**Oracles:** `crates/yang-rs/src/tests_unit/m5_case_iv.rs::s146_pp_line_circle_junction_closed_form`
(`:1272` — in-plane crossing, transversal, line-miss→None, tangent grazing; the
two-distinct-lines over-determined branch STOP asserted at `:1363`). Assay
drivers F0064 (×2 ops, RED→GREEN), R0051, F0067; baseline 241C/0W/50E/4U/0T.
NOTE: R0063 is claimed by both this spec and the N29 spec as a class sibling;
which pass actually resolves it could not be determined from code alone (anchor
unverified — flagged in 2026-07-12 catch-up).

**Deviation status:** faithful §4.4.1 junction relocation. No sign-off required.

### N31 — Cone-ellipse & cone-hyperbola SAME-TYPE junction routing to the triple relocation (KV16 / KV16b)

**Where:** `crates/yang-rs/src/stage4_correct.rs` — `vert_cone_ellipse` map
(`:761`, built `:1272`), `vert_cone_hyperbola` map (`:769`, built `:1092`),
`same_type_junction` set (`:778`), KV16b second-descriptor detection (`:1290`,
diff check `:1296`, `same_type_junction.insert(v)` `:1303`), triple-relocation
trigger `if n_maps < 2 && !same_type_junction.contains(&v)` (`:1739`),
`relocate_onto_implicit_triple` call (`:1756`). `crates/yang-rs/src/stage4_relocate.rs`
— `relocate_onto_implicit_triple` (`:251`), `ConeEllipseReloc` (`:1026`),
`cone_ellipse_residual` (`:1210`), `ConeHyperbolaReloc` (`:1076`).

**Mechanism:** a vertex at the junction of TWO conic sections of ONE cone cut by
two different planes gets both curves' Stage-4 scan arms calling
`vert_cone_ellipse.insert(v, …)` (or `vert_cone_hyperbola`); the second silently
overwrote the first, so `n_maps == 1`, the triple-junction trigger never fired,
and the single-curve relocation moved the vertex onto only the surviving conic —
the other output edge's endpoint was left off its curve and kernel-v2 rejected
("output ellipse-arc endpoint does not lie on its ellipse"). Fix: at the insert
site, detect a SECOND descriptor differing in any of apex / axis_dir / half_angle
/ plane_n / plane_d and add the vertex to `same_type_junction`, which the existing
multi-curve trigger honors — routing 3-surface vertices to
`relocate_onto_implicit_triple` (the exact cone∩planeC∩planeD point) while
≥4-surface vertices keep the loud audits. Field-for-field mirror of the KV16
`vert_cone_hyperbola` arm.

**Paper section:** faithful §4.3.3 / §4.4.1 (`...txt:518-574` / `605+`) — a
junction vertex lies on ALL incident intersection curves and relocation must
respect every constraint.

**Tolerance decisions (design choices, logged):** two constants live in
`relocate_onto_implicit_triple`/its certificate and are the honest homes for two
of the catch-up's flagged tolerance decisions:
- **8εL junction exactness certificate** — `stage4_relocate.rs:95`
  (`junction_certificate_band`, def `:70`): `TAU_WORK.max(8.0 * f64::EPSILON * l)`
  with `l = mag3(p) + refmag`. Doc-comment frames it as "exact to evaluation
  precision," NOT a tolerance widening — the strongest property float arithmetic
  can express (P9-clean).
- **1e-13.max(8εL) Newton work floor** — `stage4_relocate.rs:268`:
  `let tau = 1e-13_f64.max(8.0 * f64::EPSILON * l);`. The verbatim comment
  (`:258-266`) admits the intent honestly: the absolute 1e-13 floor is sub-ULP at
  coordinate magnitude ~4000 (the R0017 corpus scale) and could never converge
  there, so it takes the max with the same 8εL term as the certificate — and
  **"at unit scale 8εL ≈ 5e-15 < 1e-13, so the shipped torus-block behavior is
  byte-identical."** Logged as such: a scale-aware floor chosen to preserve
  existing torus-block byte output while unblocking the large-coordinate class.
  The sibling `relocate_onto_implicit_pair` uses a flat `1e-13` (`:212`, no 8εL
  max).

Same-type identity is an EXACT field comparison (apex/axis_dir/half_angle/plane_n/
plane_d), never a tolerance.

**Oracles:** `crates/yang-rs/tests/rim_junction_insertion.rs`
(`same_type_ellipse_edge_pierce_endpoints_on_curve` `:1232` — 30° frustum ∖
45°-rotated diamond prism, discriminating check = every output ellipse endpoint on
its own ellipse; sibling `same_type_hyperbola_edge_pierce_endpoints_on_curve`
`:998`). Assay drivers R0004/R0100 (junction FIXED here), R0009/R0091 (attributed
to a separate KV15b micro-scale mint-accuracy residue, not this handler). Commit
0893d5c0 dated 2026-07-11, POSTDATES N24 (2026-07-08) — no prior ledger coverage.

**Deviation status:** faithful §4.3.3/§4.4.1; the two Newton/certificate constants
are logged design decisions with the byte-identical rationale above.

### N32 — Stage-6 output arc orientation obeys the CCW-minor input convention

**Where:** `crates/yang-rs/src/stage5_topology.rs` — `orient_directed_curve`
(def `:301`, called at both push sites `:425` and `:628`), `emit_topology`
(`:361`), ambiguity-band posture comment (`:299`).

**Mechanism:** `emit_topology` created one directed edge copy per face loop but
copied the intersection curve (stored normal included) verbatim from the
undirected mesh-edge map; a clockwise traversal then declared the complementary
(~2π) arc, and Stage-1 sampled nearly a full circle for it (~90 unbalanced edges
⇒ `NonManifoldOutput` when the output re-entered a boolean). `orient_directed_curve`
fixes this: for a periodic Circle/Ellipse copy with start≠end whose CCW sweep
about the stored normal exceeds π, negate that copy's stored normal (the kernel-v2
twin convention: same point set, opposite traversal ⇒ always the minor side); a
sweep < π is copied unchanged; within 1e-6 of π is left ambiguous. Result: a yang
boolean OUTPUT is a valid yang boolean INPUT. The twin-chain bit-identity arm was
implemented then found UNNECESSARY (`ortho_basis(−n) = (e1, −e2)` exactly + atan2
odd symmetry weld the mirrored frames bit-exactly), so kernel-v2 `from_yang_brep`
re-derives the arc sense (<π as stored, >π negated), making the flips transparent.

**Paper section:** §4.4/§4.5 (`...txt:605+`) — intersection curves carry exact
geometry and shared-boundary sampling is the watertightness mechanism. This is an
internal I/O-convention fix (yang output = valid yang input), not a paper method
change.

**Tolerance decisions:** a `1e-6`-of-π arc-minor ambiguity band (`stage5_topology.rs`
near `:299`/`:301`); mirrors the kernel-v2 `boolean.rs` directional-normal twin
convention. No geometry is moved.

**Oracles:** `crates/yang-rs/tests/stage6_arc_orientation.rs`
(`output_arc_edges_satisfy_ccw_minor_convention` `:209` RED→GREEN,
`pocket_operand_reenters_plain_boolean` `:270`). No assay case ID — the driver is
yang-DIRECT chained booleans (production re-enters via kernel-v2 which re-derives);
per-case output is byte-identical.

**Deviation status:** internal-convention fix, no paper divergence. No sign-off
required.

### N33 — Disjoint-union passthrough (A ∪ B with A∩B=∅ is the disjoint sum — outside Yang's interacting-solid scope)

**Where:** `crates/yang-rs/src/boolean.rs` — `conservative_aabb` (`:1650`),
`union_operands_strictly_disjoint` (`:1697`, re-exported `lib.rs:96`),
`concat_breps` (`:1719`), fast-path dispatch (`:1773-1774`). kernel-v2 arena
merge: `crates/kernel-v2/src/boolean.rs:2945`.

**Mechanism:** a UNION whose operands' conservative AABBs are strictly disjoint is
the disjoint sum — emit the verbatim concatenation of the two input B-Reps
(indices offset, every curve/surface tag preserved) with no pipeline/tessellation
loss. Otherwise every full rim degrades to a `LineSegment` chord polyline and a
downstream boolean dies at the Stage-3 `chord_tol_for_curved_owner` →
`AmbiguousCurve{0,0}` fault. Conservative AABB = vertex hull expanded by
Circle/Ellipse radius, Sphere r, Torus R+r; returns `None` (no fast path) if any
edge carries Hyperbola/Parabola/SurfacePair. Two layers: the yang `boolean()`
passthrough (serves yang-direct chains) and a kernel-v2 arena-level shell merge
(native, preserves faces bit-for-bit — needed because yang's passthrough output is
seam-doubled input-convention topology that `from_yang_brep` will not re-ingest).

**Paper section:** OUT OF PAPER SCOPE. A∪B with A∩B=∅ has no arrangement to build;
Yang 2025 addresses interacting solids. Not a method divergence — a scope
completion.

**Tolerance decision (design choice, logged):** the disjointness band is
`1e-9·(1+scale)` (inside `union_operands_strictly_disjoint`/`conservative_aabb`,
`boolean.rs:1650-1717`). It MUST exceed the YR24 weld band
`2·max(TAU_MODEL, scale·TAU_WORK)` or the near-partial r=1e-8 weld class (yr27) is
stolen from Stage-0. The exact in-body arithmetic literal was confirmed by
function location but not quoted line-for-line in this pass (anchor unverified —
flagged in 2026-07-12 catch-up).

**Oracles:** `crates/yang-rs/tests/disjoint_union_passthrough.rs`
(`disjoint_union_preserves_circle_vocabulary` `:85` RED-first — 4 closed Circle
rims, watertight, volume = sum; `disjoint_union_output_reenters_boolean` `:136`
— the chained Stage-3 `AmbiguousCurve{0,0}` fixture). No new assay case ID;
per-case output byte-identical (baseline 237C/0W/49E/9U/0T at ship).

**Deviation status:** scope completion outside the paper; the disjointness band is
a logged design decision (must dominate the weld band). No user sign-off
outstanding.

### N34 — KV6a-tilted: full-turn revolve alternation gate narrowed to consecutive-annuli only

**Where (kernel-v2, NOT a yang-pipeline stage):** `crates/kernel-v2/src/construct.rs`
— `build_full_revolve` (`:1680`), consecutive-annuli typed reject (`:1695`,
`NotImplemented("PR-KV6a full-turn revolve with consecutive annular …edges")`),
`start_cap`/`end_cap` now `Option<FaceId>` (fields `:282`/`:286`, capless logic
`:1883-1930`), `REVOLVE_FULL_TURN_TOLERANCE = 1e-9` (`:316`).

**Mechanism:** a full-turn (360°) polygon revolve whose profile has consecutive
wall edges (parallel→cylinder or oblique→cone, no annulus between) died at an
alternation gate demanding strict wall/annulus alternation. The gate is narrowed:
every wall/annulus pairing is now supported EXCEPT two consecutive ANNULI (a
subdivided radial edge). Justification (P8, removes an artificial input
restriction): rim-normal consistency holds for any wall neighbor — adjacent walls
meet head-to-tail so their twin rim half-edges carry opposite ±â normals,
satisfying the curve-twin rule the gate protected. Caps are no longer guaranteed:
`start_cap`/`end_cap` became `Option`; an all-oblique tilted-axis rectangle (4
cone frusta, 0 annuli) builds a capless genus-1 cone-frustum ring with both caps
`None`.

**Paper section:** NONE — kernel-v2 constructor change (surface-of-revolution
vocabulary), not a Yang-pipeline stage. Cites Stroud 2006 §3.1.4 (single-fake-edge
closed curved edges) and P8. Logged here for completeness of the revolve campaign;
strictly this belongs to the kernel-v2 construction layer, not the Yang deviation
surface.

**Tolerance decisions:** none new — reuses `REVOLVE_FULL_TURN_TOLERANCE = 1e-9`.

**Oracles:** `crates/kernel-v2/tests/kv6a_revolve.rs`
(`full_turn_oblique_edge_builds_cone_frustum`, `full_revolve_topology_census_genus_one_washer`,
`full_volume_is_exactly_pi_r2sq_minus_r1sq_h`, `full_mesh_watertight_with_annular_caps`,
`revolve_face_partial_and_full_end_to_end`, `capability_walls_keep_notsupported_marker`).
Assay: **C0070** ERROR→CORRECT (with the meta genus correction euler_target 2→0).
C0070 is not explicitly pinned in `assay_kv2.rs`; the meta/gen_complexity edits
(`tracker(2,4.0)→(0,4.0)`, `C0070.meta.json`) were not re-verified at HEAD in this
pass (anchor unverified — flagged in 2026-07-12 catch-up).

**Deviation status:** kernel-v2 constructor P8 change; no Yang divergence.

### N35 — KV6d: closed-torus & on-axis-sphere full-turn revolve + Stage-4 bounded-face containment guard

**Where:** kernel-v2 constructors — `crates/kernel-v2/src/construct.rs`:
`build_torus_revolve` (`:1978`, full-turn dispatch `:399`), sphere sweep dispatch
(`:2059`, `Surface::Sphere` emission `:2444`), clearance
`REVOLVE_MIN_AXIS_CLEARANCE_REL = 1e-9` (`:311`); `geom::sphere_residual`
(`geom.rs:86`), `validate_sphere_face` (`validate.rs:1640`),
`tessellate_sphere_patch` (kernel-v2 `tessellate.rs:1916` → yang
`stage1_tessellate.rs:4145`). Stage-4 (yang) containment guard —
`crates/yang-rs/src/stage4_correct.rs`, inside `stage4_relocate_and_correct`
(`:723`): wedge gate `2.0*d_eps/sin_theta` (`:2815`), `OffCurveBeyondChordBand`
reject (`:2820`), containment guard block with the C0065 comment (`:2824`+),
per-planar-partner AABB hull loop (`:2838`+).

**Mechanism (two parts):** (1) CONSTRUCTORS — a full-turn revolve of a circle
profile about a strictly off-axis in-plane axis builds a CLOSED ring-torus
(genus 1, minimal-CW aba⁻¹b⁻¹ seam: V=1, E=2 closed circles, F=1 `Surface::Torus`);
a full-turn revolve of a circle centered ON the axis builds a CLOSED sphere
(genus 0, PR-YR12 contract: V=2 poles at center±r·ẑ in WORLD z — the sphere is
isotropic so the seam frame is CANONICAL z-up regardless of the revolve axis,
making `to_yang` a direct emission — E=1 meridian `Curve::Arc` twin, F=1
`Surface::Sphere`). Both re-enter the yang pipeline (Stage-1 doubly-periodic θ×φ
torus grid; sphere lat/long grid or `tessellate_sphere_patch` pole-cap/disk UV-CDT
for boolean-output patches). (2) A Stage-4 BOUNDED-FACE CONTAINMENT GUARD: near
tangency the wedge relocation gate `2·d_ε/sinθ` balloons, letting an inscribed
mesh close an intersection loop early INSIDE the partner's bounded face while the
implicit-pair Newton drags relocated points onto the infinite-surface curve
OUTSIDE that face. The guard rejects (loud STOP) any relocation escaping every
matching planar partner face's vertex-hull AABB (+d_ε), since a correct vertex must
lie on both bounded faces. Planes only — curved hulls under-bound closed seam
loops.

**Paper section:** the constructors are exact/analytic (Stroud 2006 §3.1.4 +
Mäntylä 1988 for the seam CW structure), not a Yang method. The containment guard
lives in the §4.3.3 near-tangency relocation region (`...txt:518-574`) and its
comment explicitly DEFERS the honest near-tangency fix to "the §4.3.3
near-tangency increment" (task #137). That deferral is the real DEVIATION in this
entry: a loud STOP substitutes for the §4.3.3 mesh-topology-matches-exact-topology
handling when the face gap falls under the sagitta.

**Tolerance decisions:** `REVOLVE_MIN_AXIS_CLEARANCE_REL = 1e-9` and
`REVOLVE_FULL_TURN_TOLERANCE = 1e-9` are pre-existing. The Stage-4 wedge gate
`2·d_ε/sin_theta` (`:2815`) and hull inflation `+d_ε` reuse the PR-YR10 chord-band
budget (the N29 corridor family) — no new tolerance-widening constant; the guard's
purpose is to stay LOUD, not to widen acceptance.

**Oracles:** `crates/kernel-v2/tests/kv6d_closed_torus.rs`
(`closed_torus_topology_census`, `closed_torus_mesh_watertight_with_pappus_volume`,
`closed_torus_near_tangent_shaft_stays_loud` — the containment-guard oracle,
`crossing_circle_full_turn_rejected_as_error`, `on_axis_circle_full_turn_builds_sphere`);
`kv6d_sphere_revolve.rs` (`closed_sphere_topology_census`,
`closed_sphere_mesh_watertight_with_ball_volume`,
`closed_sphere_boolean_equatorial_half_cut`, `partial_on_axis_circle_still_rejected`);
yang unit `tests_unit/boolean_functional.rs::torus_closed_full_turn_doubly_periodic`.
Assay: the revolve UNSUPPORTED bucket is now EMPTY (operand construction complete),
but the boolean CASES advance to deeper typed Stage-4 errors — **C0065** pinned
`Category::Error` (`assay_kv2.rs:1193`, near-tangent shaft containment guard);
**C0067** per campaign memory → Stage-4 LocalRefinementRequired (the N2 class),
not explicitly pinned (anchor unverified — flagged in 2026-07-12 catch-up).

**Deviation status:** constructors are faithful/out-of-scope; the containment
guard is an ACKNOWLEDGED deviation — a loud STOP standing in for the §4.3.3
near-tangency mesh-topology handling, remediation tracked as task #137. Not signed
off; actively tracked.

### N36 — Tolerance-vocabulary consolidation (TAU_EVAL) + named surviving divergences

**Date:** 2026-07-12 (design review F8). **Class:** refactor, value-identical —
no behavior change; every replaced literal keeps its exact prior value.

**What changed:**
- The unnamed `1e-9` rounding tier is now the named central
  `cad_primitives::TAU_EVAL` (f64 evaluation/rounding band, scale-relative
  for distances, direct for dimensionless residuals). Renamed-in-place
  consumers: kernel-v2 `YANG_NORMAL_AGREEMENT_TOLERANCE`,
  `validate::NORMAL_AGREEMENT_TOLERANCE`,
  `validate::PLANARITY_BOOLEAN_OUTPUT_TOLERANCE`, `recover::BAND`, four
  inline output-curve acceptance bands in `boolean.rs`, the adapter
  hole-nesting band; yang-rs `brep::MATCH_TOLERANCE`; waffle-types
  `TAU_COINCIDENT`.
- The §4.4.2 corridor formula `2·budget/divergence` (with the
  tangency→INFINITY arm) was duplicated at six sites in
  `stage4_correct.rs`; now one helper,
  `stage4_correct::tangent_plane_corridor` (paper
  refs/text/yang2025_hybrid_boolean.txt:494-537). Future corrections land
  there once.

**Surviving divergences, named honestly rather than unified blind (each a
behavior change requiring a full-assay measurement before touching):**
- `stage4_relocate::TORUS_RELOC_WORK_FLOOR = 1e-13` — 10× tighter than
  `TAU_WORK`; chosen at torus-block ship time for byte-identical behavior.
  Unify-to-TAU_WORK = banked measured debt.
- `stage4_relocate::AMP_TANGENCY_MIN_SIN_CIRCLE_PAIR` (= `MIN_FEATURE_SIZE`,
  1e-6) vs `AMP_TANGENCY_MIN_SIN_CYL_CYL` (= 1e-3): 1000× spread between
  two amplification forms' tangency cutoffs. Both trigger the SAFE fallback
  (`None` → tangent-direction discriminator), so this is conservatism
  spread, not fudge — but it is unjustified; unification = banked measured
  debt.

**Oracles:** value-identical by construction; yang-rs/kernel-v2/waffle-types
suites green post-change. **Sign-off:** refactor, signed off 2026-07-12.
