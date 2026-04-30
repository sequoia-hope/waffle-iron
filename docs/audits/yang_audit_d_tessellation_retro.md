# Auditor-D — Tessellation Thread Retrospective + Yang §4.1.1 Bijective Tessellation Gap

Audit slice: PR1-PR7 tessellation thread (commits `5f5423c`, `0d6fb3a`,
`f01dd68`, `d2eb72b`, `a445c18`, `c4f0fcb`, `8ad64b5`, `720fa8d`,
`7ee4805`, `436ed37`, `7607256`, `08127ad`, `3af7fd6`) plus the
Yang 2025 §4.1.1 bijective error-bounded triangulation contract vs.
the current `tessellate_solid_bounded` + `repair.rs` implementation.

Read-only audit. Production code unchanged.

Anchor reading: `docs/references/yang2025_hybrid_boolean.txt` §4.1–4.4,
`/tmp/cherchi2022.txt`, `governance/ENGINEERING_CONSTITUTION.md` P5/P8/P9/P10,
`governance/ARCHITECTURAL_INVARIANTS.md` §A15.6,
`docs/audits/cherchi_port_audit.md` D-10,
`specs/tessellation_bounded_residuals.md` §1–§11,
`specs/tessellation_pr3_corpus_dump.md`,
`specs/tessellation_bounded_gate.md`,
plus the five memory feedback files
(`feedback_yang_only.md`, `feedback_no_last_bug.md`,
`feedback_no_regression_chasing.md`, `feedback_anchor_before_fix.md`,
`feedback_validate_against_corpus.md`).

---

## §1 PR1-7 retrospective

### Headline

**Seven PRs, two production fixes, one corpus measurement, four documentation
closures. The Yang assay did not move (7-9/157 throughout). The thread shipped
real diagnostic infrastructure (oracle, classifier, corpus baselines) but did
not advance the bijective tessellation contract.**

| PR | Commit | Type | Goal | Outcome |
|----|--------|------|------|---------|
| PR1 | `5f5423c` + `d2eb72b` + `a445c18` | infra + measurement | Bijective oracle + corpus baseline | Shipped. Oracle now flags non-bijective face pairs across the corpus. 67 nb pairs across 14 cases on the linear-bounded path. |
| PR2 | `f01dd68` (after `9a2ec5f` revert via `dbe2fcf`) | fix | Share cap-to-lateral boundary vertex IDs in revolve primitive | Shipped. Fixes full-360° revolve cases. PR3 corpus measurement (`c4f0fcb`) confirms shift in baseline. Did NOT fix partial-revolve / oblique-axis (R0033 stays nb-positive). |
| PR3 | `8ad64b5` + `720fa8d` | docs (falsified hypothesis) | Dedup B-Rep vertices in `discretize_edges` | Falsified pre-implementation: oracle keys on f32 byte-pattern, not pool indices. Shipped as `tessellation_bounded_residuals.md` §1-§7 + corpus dump §8.1-§8.3. |
| PR4 | `7ee4805` + `436ed37` | diag + docs | Anchor R0033 fix in B-Rep assembly (`stitch.rs` / `analytical.rs`) | Falsified twice: first commit anchored on `boolean/yang_integration.rs` Step 9 (wrong subsystem); second commit (`436ed37`) corrected to revolve primitive after discovering AABB-disjoint short-circuit. Shipped as `pr4_r0033_t_junction_diagnosis.rs` RED test + spec §8.4-§8.5. |
| PR5 | `7607256` | docs (falsified anchor) | Extend `RevolvePool` to `tessellate_revolve_cap_polygon` for partial revolves | Falsified pre-implementation: AABB-disjoint Subtract short-circuit calls `flood_fill_patches` which strips `revolve_params=None`, so the cap-polygon function is never invoked. Implementer pivoted to "option 1" (Newell-reverse desync); also falsified empirically (`dot_ns=1.0000` for all 6 R0033 faces). Shipped as spec §9. |
| PR6 | `08127ad` | diag + docs | Investigate twin-pairing in `flood_fill_patches` | Reproducer `test_flood_fill_patches_twin_pairing_disjoint_subtract` shipped RED. The function correctly reports input-mesh non-bijectivity; defect is upstream. Shipped as spec §10. |
| PR7 | `3af7fd6` | infra + docs | Classify each R0033 nb pair into one of 4 mechanisms | Classifier shipped (`pr7_classify.rs`). Findings: 1 of 2 pairs is `PoolNotShared`, 1 is `Other` (oracle heuristic false positive). Three independent stop conditions fire (5th class detected, multi-mechanism, multi-module fix site). Shipped as spec §11. |

### Per-PR brief

#### PR1 — bijective oracle + corpus baseline
- **Goal:** Establish a measurable bijectivity contract for the bounded path.
- **Outcome:** `bijective.rs::check_face_pair_bijective` ships. The oracle keys
  directed mesh edges on f32 byte patterns (`pos_key`, `bijective.rs:219`) and
  reports per-face-pair counts via `restrict_to_shared_boundary`
  (`bijective.rs:334`).
- **Permanent value:** All subsequent PRs use this oracle. The corpus baselines
  in `d2eb72b`/`a445c18` partition the 157 non-skipped assay cases by gate
  class and produce stable nb counts. PR3 corpus dump (`720fa8d`) extends this
  with per-case nb counts and a top-13 anchor list.
- **Verdict:** Net positive. Diagnostic infrastructure that survives the rest
  of the thread and is the foundation for the four falsifications that
  followed.

#### PR2 — revolve cap-to-lateral pool sharing
- **Goal:** Eliminate non-bijectivity at the cap-lateral boundary of revolve
  primitive output.
- **Outcome:** `f01dd68` ships after the original `9a2ec5f` was reverted
  (`dbe2fcf`). The fix shares boundary vertex IDs between the revolve cap
  polygon and the revolve lateral surface ring.
- **Permanent value:** Full-360° revolve cases now bijective on shared
  cap-lateral boundaries. PR3 corpus measurement (`c4f0fcb`) is the
  post-fix baseline.
- **Limitation:** The fix is scoped to revolve cases that route through the
  per-primitive tessellator (`tessellate_revolve_*`). PR4 §8.4-§8.5 discovered
  that R0033 — the canonical anchor for the partial-revolve case — does NOT
  route through the per-primitive tessellator at runtime: AABB-disjoint
  Subtract short-circuits via `yang_pipeline_result_for_disjoint`, which calls
  `flood_fill_patches`, which calls `result_topology_to_waffle_solid`, which
  strips `revolve_params=None` (`yang_integration.rs:243`). R0033 routes via
  `tessellate_solid_bounded` (linear-bounded class), so PR2's fix is
  inapplicable to it.
- **Verdict:** Partial fix. Real for the cases it covers; not the broad
  bijective-tessellation invariant.

#### PR3 — dedup hypothesis falsification + corpus dump
- **Goal:** Implement `BTreeMap<VertexIdx, usize>` cache in `discretize_edges`
  so shared B-Rep vertices emit one pool entry, not N.
- **Outcome:** Falsified pre-implementation. The oracle keys on f32
  byte-patterns, not pool indices. Two pool indices referencing
  byte-identical f64 values cast to byte-identical f32 → oracle reports
  bijective regardless. The proposed dedup changes pool indices but not
  what the oracle measures (`tessellation_bounded_residuals.md` §1–§3).
- **Permanent value:** The corpus dump (`720fa8d`) ranks all 157 cases by
  nb-pair count and identifies R0033 as the smallest multi-nb case. This
  is what every subsequent PR uses as the canonical anchor.
- **Verdict:** Wrong fix avoided. Net positive: shipped diagnostic
  infrastructure (corpus dump) instead of a no-op fix.

#### PR4 — R0033 diagnostic + corrected anchor
- **Goal:** Pin R0033's mechanism and recommend a PR5 anchor.
- **Outcome:** First commit (`7ee4805`) named `boolean/yang_integration.rs`
  Step 9 / `topology_extract.rs::flood_fill_patches::assemble_brep` /
  `boolean/cherchi/` twin construction as candidate fix sites. Implementer
  re-traced the dispatch and found the AABB-disjoint short-circuit at
  `topology_extract.rs:1515`. Commit `436ed37` corrects the anchor
  recommendation to the revolve primitive tessellator.
- **Permanent value:** `pr4_r0033_t_junction_diagnosis.rs` is the RED
  end-to-end test that stays RED through PR5/PR6/PR7. Spec §8.3 documents
  two empirical findings (Finding A: 12 only-in-A vs 12 only-in-B vertices;
  Finding B: same-forward-direction directed edges).
- **Cost:** The first-commit anchor was wrong, then re-traced. The errata
  in §8.5 documents the trace.
- **Verdict:** Diagnostic infrastructure shipped. Two anchor recommendations
  (commit-1 and commit-2) both later proved wrong by PR5's re-tracing.

#### PR5 — falsified-anchor cascade
- **Goal:** Implement PR4 commit-2's anchor (extend revolve cap-pool to
  partial revolves).
- **Outcome:** Implementer immediately found that `tessellate_revolve_cap_polygon`
  is dispatched only when `revolve_params.is_some()` (`mod.rs:476`); R0033's
  post-flood-fill solid has `revolve_params=None`, so the cap polygon
  function is never invoked. Pivoted to "option 1" (per-face Newell-reverse
  desync in `tessellate_planar_face_bounded`). `PR5_DEBUG=1` instrumentation
  showed `dot(natural_newell, stored_normal) = 1.0` for all 6 R0033 faces —
  the original `reverse_outer = dot < 0.0` check never fires. Option-1 patch
  is a behavioral no-op for R0033. Reverted.
- **Permanent value:** `feedback_anchor_before_fix.md` exists because of this
  PR. The instrumentation pattern (`eprintln!`/`PR_DEBUG=1` before writing
  code) is now project doctrine.
- **Verdict:** Wrong fix avoided after partial implementation. Net cost is
  half-implementation effort plus revert; net value is the feedback memory
  + doctrine that prevents future occurrences.

#### PR6 — `flood_fill_patches` Phase A
- **Goal:** Investigate `flood_fill_patches` twin-pairing as the R0033
  defect site.
- **Outcome:** Kernel-internal reproducer `pr6_box_with_unbijective_front_top_edge`
  ships RED with 3 unpaired half-edges (TWIN_DEBUG output captured in
  spec §10.2). Empirical mechanism: front face emits `V6→V8→V7` via midpoint M;
  top face emits `V7→V6` directly without M; no reciprocal pairing possible.
  The function correctly reports input-mesh non-bijectivity. Defect is
  upstream in the tessellator (R0033 case) or in the synthetic mesh
  construction (reproducer).
- **Permanent value:** `flood_fill_patches` is now confirmed correct under
  the "garbage in, garbage out" semantics — it faithfully reports the
  upstream bijectivity violation rather than masking it. Three options
  considered for local Phase B fix were rejected (self-twin placeholder,
  synthetic reverse half-edge, looser quantization) — all three are the
  S-H-clipping anti-pattern A15.6 deprecates.
- **Verdict:** Confirmed-correct null result. The function under
  investigation is not the defect site; the diagnostic infrastructure
  (TWIN_DEBUG + reproducer) is the deliverable.

#### PR7 — mechanism classification + multi-stop pivot
- **Goal:** Classify R0033's nb pairs into one of {`arena-missing-edge`,
  `pool-not-shared`, `positional-drift`, `direction-reciprocity`}, ship a
  fix only if local AND simple.
- **Outcome:** Classifier `pr7_classify.rs` (508 lines) ships. R0033's
  pair #0 → `PoolNotShared` (Mechanism #1: face A's triangulation routes
  via interior subdivision vertices that face B doesn't have). Pair #1 →
  `Other` (Mechanism #2: oracle's `restrict_to_shared_boundary` heuristic
  matches edges on the *interior* of each face's loop that happen to be
  position-coincident — a measurement artifact, not a tessellation defect).
- **Three independent stop conditions fire:**
  1. 5th class detected (`Other` is outside the four-class taxonomy).
  2. Multi-mechanism (pair #0 is `PoolNotShared`, pair #1 is `Other`).
  3. Root cause spans 5+ post-processing repair functions across
     multiple files.
- **Permanent value:** §11.5 names six post-processing stages
  (`fix_winding_consistency`, `remove_winding_insensitive_duplicates`,
  `flip_nonmanifold_interior_diagonals`,
  `retessellate_nonmanifold_faces_with_steiner_fan`,
  `remove_nonmanifold_topology_aware`,
  `remove_nonmanifold_duplicates_aggressive`). The classifier identifies
  Steiner-fan retessellation (`repair.rs:1413`) as the primary suspect for
  the interior-subdivision asymmetry mechanism.
- **Verdict:** Classifier infrastructure shipped. Fix deferred per stop
  condition. Recommends pivoting to Cluster II rather than continuing
  the thread (§11.7 Option B).

### Quantified shipping summary

- **Production code changed (lines, approximate):**
  - PR1: ~1500 lines (`bijective.rs` oracle + 4 face-pair tests).
  - PR2: ~80 lines (revolve cap-lateral pool sharing in `mod.rs`).
  - PR7: ~500 lines (`pr7_classify.rs` + `edge_geometry_for` accessor).
  - PR3, PR5, PR6 production: 0 lines (PR3 stub reverted, PR5 option-1
    reverted, PR6 Phase B not implemented).
- **Tests added:** 4 face-pair tests (PR1), `pr2_partial_revolve` (PR2),
  `pr4_r0033_t_junction_diagnosis` (PR4), `pr6_flood_fill_patches_*` (PR6),
  `pr7_r0033_mechanism_classification` (PR7). Of these, PR4 and PR6 stay
  RED through the whole thread.
- **Specs added:** `tessellation_bounded_gate.md` (PR1 prep),
  `tessellation_bounded_residuals.md` (§1-§11, ~1000 lines, 7 amendments),
  `tessellation_pr3_corpus_dump.md` (~290 lines).
- **Yang assay delta:** 0. The fast-runner reports 7-9/157 throughout
  per CLAUDE.md and per spec context.

---

## §2 Lessons learned (blunt)

### 2.1 The 5-revision pattern is real and structural

Five anchor revisions in a single thread is not a fluke; it is a structural
feature of the Yang/Cherchi/Yang-integration code path. From `feedback_anchor_before_fix.md`:

> The Yang/Cherchi pipeline has many short-circuits, dispatch branches, and
> post-processing rewrites (e.g. AABB-disjoint short-circuit calls
> `flood_fill_patches` which rebuilds the arena and strips primitive params
> via `result_topology_to_waffle_solid`). Plan-time hypotheses based on
> reading the code top-down miss these.

Concrete instances of "the planned anchor was not actually invoked":
- PR4 v1 anchored on `boolean/yang_integration.rs` Step 9 — function never
  reached because of AABB-disjoint short-circuit at `topology_extract.rs:1515`.
- PR5 brief anchored on `tessellate_revolve_cap_polygon` — function never
  reached because `revolve_params=None` after `result_topology_to_waffle_solid`
  strips it.
- PR5 option 1 anchored on `tessellate_planar_face_bounded` Newell-reverse
  branch — branch never fires because `compute_newell_normal` is computed
  per-face from the same arena loop, guaranteeing `dot=1.0`.

The structural problem: **dispatch decisions are tag-driven**
(`is_polygon_soup`, `revolve_params.is_none()`, `cylinder_params.is_none()`),
the tags are *rewritten* by post-processing stages in the boolean pipeline,
and the rewrites change which tessellator branch fires. A reading of the
code top-down does not surface which branch a specific assay case actually
takes; that information is only obtainable by instrumenting and running.

### 2.2 Hypothesis falsification is the productive output of this thread

Five hypotheses were generated; five were falsified. PR3 falsified pre-impl,
PR5 brief falsified pre-impl, PR4 v1 falsified by re-trace, PR5 option-1
falsified by `PR5_DEBUG=1`. Only PR2 produced a working production fix.

Per `feedback_no_last_bug.md`:

> we don't claim "the last gap"; we ship the empirical lesson.

This thread followed that doctrine — every PR3-PR7 closure ships a spec
amendment that documents what was learned, so the next implementer doesn't
re-anchor on the same falsified hypothesis. From the trace pattern alone,
the project's planning capacity for this code path is below what is needed
to correctly anchor a fix on the first try; building falsification specs is
a substitute for that capacity.

### 2.3 The post-processing repair pipeline IS the deprecated S-H pattern

§11.5 names six post-processing repair functions in `tessellation/repair.rs`:

| Function | Lines (in repair.rs) | Purpose |
|----------|----------------------|---------|
| `fix_winding_consistency` | ~31-500 | Re-orient triangles so all share the consistent normal direction |
| `remove_winding_insensitive_duplicates` | ~502-583 | Deduplicate triangles that overlap regardless of winding |
| `flip_nonmanifold_interior_diagonals` | ~830-1410 | Edge-flip repair for non-manifold edges from earcut diagonal conflicts |
| `retessellate_nonmanifold_faces_with_steiner_fan` | ~1413-2168 | Replace earcut output with centroid-fan when prior repair fails |
| `remove_nonmanifold_topology_aware` | ~585-828 | Remove triangles whose face_id contradicts the B-Rep edge→face map |
| `remove_nonmanifold_duplicates_aggressive` | ~2170-2890 | Aggressively prune any remaining non-manifold edges |
| `fill_boundary_holes` | ~2892-3078 | (FAN PATH) Fill boundary holes after fan-path tessellation |
| `close_near_boundary_chains` | ~3080-3514 | (FAN PATH) Close chains near boundary on the fan path |
| `remove_isolated_triangles` | ~3516-end | Remove triangles isolated from the main mesh |

Total: ~3700 lines of post-processing repair just in `repair.rs`. Six of
these (the first six above) run on the bounded path. They run sequentially
in `tessellate_solid_bounded` (`mod.rs:4278-4330`), each rewriting the
triangulation produced by the prior stage.

Per A15.6:

> **DEPRECATED — do not improve, do not delete yet**: The S-H clipping +
> tolerance escalation pipeline (`classify_face`, `stitch.rs` progressive
> pairing, tessellation repair loops, `fill_boundary_holes`,
> `close_near_boundary_chains`). These mask classification errors with up
> to 5000× tolerance widening and synthetic fill triangles.

The repair pipeline on the bounded path is structurally the same pattern
applied to tessellation rather than to S-H clipping. Each stage rewrites
the prior stage's output to mask a defect that should not have been produced
in the first place. Yang §4.1.1 + §4.4.1 do not describe any post-processing
"repair" stages — the bijective tessellation contract is supposed to hold
*by construction* from the discretization step (§4.1.1) and is preserved by
re-meshing along intersection curves (§4.4.1). There is no post-discretization
repair stage in Yang's algorithm.

The PR7 §11.5 finding is direct evidence that the repair pipeline is NOT
preserving the bijective contract: face A's triangulation gets rewritten
by Steiner-fan to use a centroid + interior subdivisions; face B's
triangulation is rewritten differently or not at all; their shared boundary
discretization no longer matches.

### 2.4 What `feedback_no_regression_chasing.md` adds

> Build Yang faithfully, don't accommodate the legacy repair pipeline.

PR1-7 wrote new code (`bijective.rs`, `pr7_classify.rs`, `repair.rs`
edit history is older but PR1-7 sits on top of it without modifying it).
The repair pipeline is legacy from the pre-Yang fan-path stack. Each
falsified anchor in PR3-PR7 is downstream of the repair pipeline either
spatially (the function being patched is consumed by repair) or temporally
(the function runs after repair has rewritten the geometry). The repair
pipeline is the load-bearing reason why `revolve_params=None` after
`flood_fill_patches`, why per-face Newell agrees with stored normal
trivially, and why interior subdivisions appear on one face and not its
neighbor.

The thread implicitly accommodated the repair pipeline by trying to fix
its symptoms instead of removing it. `feedback_no_regression_chasing.md`
is doctrine the thread did not fully apply — it mostly applied at PR7
when the recommendation pivoted to Cluster II rather than continuing
to patch repair stages.

### 2.5 Was PR1-PR7 the right priority given assay impact?

**No.**

The reason: Yang assay did not move. The thread's stated success criterion
(per CLAUDE.md "Hybrid boolean pipeline (Yang 2025) — This is the #1 priority.
The goal is `YANG_BOOLEAN=1` passing more assay cases (currently 9/157
non-timeout)") was unmet for 7 PRs.

The thread DID produce diagnostic infrastructure (oracle, classifier,
corpus dump, two RED tests, five spec amendments). That infrastructure has
permanent value — but per `feedback_validate_against_corpus.md` and the
A15.6 acceptance criterion, "more assay cases passing" is the only signal
that matters. Diagnostic-only PRs deferred against that criterion.

The blunt restatement: the thread spent 7 PRs of effort to produce 1 working
fix (PR2, partial coverage) and 6 documentation closures. Continuing this
shape of work is incompatible with the project's stated #1 priority.

What should have been prioritized instead (per CLAUDE.md priority order
and PR7 §11.7 Option B):
1. The actual `pool-not-shared` fix at `retessellate_nonmanifold_faces_with_steiner_fan`
   — but per PR7 §11.6, that's 200-500 lines and a multi-module refactor.
2. Removing the repair pipeline entirely and replacing it with a Yang-§4.1.1-faithful
   bijective tessellator that produces the contract by construction
   (the "Cluster II" pivot suggested in PR7 §11.7).
3. Higher-priority assay work that lives elsewhere in the Yang stack
   (e.g., the SSI solvers that feed §4.3 refinement, the Cherchi 2022
   integration that produces watertight intersection input to §4.4.1).

The seven PRs anchored on R0033 specifically. R0033 is one of 14
linear-bounded cases with non-bijective pairs (out of 81 linear-bounded,
out of 157 total). The single-case anchor strategy means even a successful
fix would close 1-of-157 (or possibly 14-of-157 if the mechanism
generalizes) — well below the broader Yang pipeline failure population
(110 of 157 in `?` class, all errored before tessellation per
`pr3_corpus_dump.md` §subtotals).

### 2.6 Would a Yang-§4.1.1-faithful tessellator look fundamentally different?

**Yes. See §3.**

---

## §3 Yang §4.1.1 contract analysis

### 3.1 The contract (verbatim)

From `docs/references/yang2025_hybrid_boolean.txt:518-642`:

> 4.1.1 Error-bounded triangulation. Our method first discretizes
> each closed B-Rep model composed of multiple surface patches into
> a triangle mesh. Then a bijective mapping between each surface
> patch and its discretization is constructed. The generated triangle
> mesh is a closed, watertight manifold under a given surface-to-mesh
> distance tolerance d_ε from the original B-Rep model.

And §4.1.2 on adjacency (lines 592-642):

> Discretizing each patch by sampling regularly in its u-v domain
> requires techniques such as integer optimization to ensure
> watertightness between patches. We find it unnecessary; instead, we
> discretize each surface patch independently without considering its
> neighbors, re-sample the boundary curves, and reconstruct the
> triangulation around the boundaries.
>
> For each surface, we first triangulate the rectangular u-v domain
> until reaching the given distance tolerance d_ε. Then, for each
> boundary curve, apply constrained Delaunay triangulation (CDT) in
> CGAL to retriangulate the two adjacent surfaces around the boundary,
> and remove the trimmed area as in [Diazzi et al. 2023], if it's a
> trimmed surface, generating a watertight mesh.
>
> ...
>
> The discretization results are bijective to the B-Rep models if the
> NURBS surfaces are regularly defined, such that we can use the mesh
> intersection results to provide proper initialization to solve the
> B-Rep intersections.

The contract has four explicit parts:
1. **Closure / watertightness**: every interior edge has exactly two
   incident triangles.
2. **Manifold**: every vertex has a topological disk neighborhood.
3. **Bounded distance**: Hausdorff distance from any mesh triangle to its
   source surface ≤ d_ε.
4. **Bijection**: each mesh triangle corresponds to exactly one B-Rep
   surface patch; the (u, v) parameters lift back to the patch.

§4.1.2 prescribes a specific construction:
- (a) Triangulate the u-v rectangle of each NURBS patch independently,
  refining until d_ε is met.
- (b) Re-sample each boundary curve.
- (c) For each boundary curve, run CDT on the two adjacent surfaces'
  parametric domains so the boundary discretizations agree.
- (d) Remove the trimmed area (Diazzi 2023 method).

The CDT step (c) is what guarantees adjacency. Both adjacent faces' u-v
triangulations are rebuilt with the same boundary samples as constraints,
so the two faces' boundary mesh edges are identical by construction. There
is no post-processing "repair" stage; bijectivity is enforced by CDT during
construction.

### 3.2 Current implementation

The bounded path in `tessellation/mod.rs` operates as follows:

1. `discretize_edges(arena, edge_geometry)` (`mod.rs:3136`) — produces
   one shared `disc.positions[]` pool. For each B-Rep edge, computes the
   sequence of pool indices the edge contributes (Linear: 2 indices;
   Circular/Arc: N+1 indices for N segments).
2. For each face independently, `tessellate_planar_face_bounded`
   (`mod.rs:3291`) collects the face's outer + inner loop boundaries as
   sequences of pool indices, then runs `earcut`-style triangulation of
   the boundary polygon.
3. Six post-processing repair stages run sequentially
   (`mod.rs:4278-4330`):
   1. `fix_winding_consistency` — flip CW→CCW per face
   2. `remove_winding_insensitive_duplicates` — drop duplicates
   3. `flip_nonmanifold_interior_diagonals` — edge-flip repair
   4. `retessellate_nonmanifold_faces_with_steiner_fan` — replace
      earcut with centroid-fan when prior steps fail
   5. `remove_nonmanifold_topology_aware` — drop triangles whose
      face label contradicts B-Rep
   6. `remove_nonmanifold_duplicates_aggressive` — drop any
      remaining non-manifold-edge-incident triangles

Step 1 + step 2 attempt to satisfy the bijective contract by *sharing pool
indices for boundary vertices*. The mechanism is structurally weaker than
§4.1.2 (c)'s CDT-with-constraints because:

- It shares only **endpoint** vertices of B-Rep edges, not interior
  subdivision vertices added during face triangulation. Yang §4.1.2 (c)
  shares **all boundary samples** by re-running CDT on both faces with
  the boundary curve as a constraint.
- It assumes the per-face earcut triangulation does not introduce
  interior subdivisions on the boundary. Earcut on a non-convex polygon
  can fail this assumption (the same polygon may be triangulated with
  different diagonals on the two faces, producing different "boundary"
  edges in the rendermesh).
- It does not propagate boundary subdivisions added by repair stages
  (step 3.4 Steiner-fan adds a centroid; the centroid is interior on
  this face but the centroid's *radial edges* land on the boundary,
  potentially creating an interior subdivision the neighboring face
  doesn't have). This is exactly the PR7 §11.3 mechanism #1.

Steps 3.1-3.6 attempt to *recover* manifoldness post-hoc when steps 1+2
have already produced a non-manifold mesh. Each step rewrites the
triangulation in a way that breaks the bijective contract on a different
face boundary.

### 3.3 Comparison

| Yang §4.1.1 + §4.1.2 contract | Current implementation |
|---|---|
| Each B-Rep face triangulated independently in u-v | Each B-Rep face triangulated independently using earcut on the 3D boundary polygon |
| Boundary samples re-used as CDT constraints on adjacent face | Boundary endpoints (only) shared via pool indices in `disc.positions`; interior subdivisions NOT shared |
| Trimmed area removed via Diazzi 2023 | Inner loops collected and earcut-triangulated as holes |
| Bijection enforced by construction (CDT with constraints) | Bijection attempted via vertex-sharing but broken by per-face earcut diagonal choice + post-processing rewrites |
| No post-processing repair stages | Six post-processing repair stages (~3700 lines `repair.rs`) |
| Output is closed, watertight, manifold by construction | Output is non-bijective on 14 of 81 linear-bounded cases; 67 nb pairs total (PR3 corpus dump) |
| "We find it unnecessary" (re: integer optimization for watertightness) | Implicitly relies on tolerance-based welding (`weld_mesh_vertices`, audit D-10) on the fan path; the bounded path's repair pipeline is the same pattern |

The fundamental gap: **Yang's algorithm uses CDT with the boundary curve
as an explicit constraint to make adjacent faces' boundary discretizations
identical by construction. The current implementation tries to share
boundary endpoints via pool indices and hope that per-face earcut
produces matching boundary edges; when it does not, post-processing
repair attempts to recover. This is a fundamentally different
algorithm.**

### 3.4 Is the gap tactical or architectural?

**Architectural.**

The reasoning:

A tactical fix is one where the current algorithm's output can be made
correct by changing a parameter, adding a check, or fixing a localized
function. The PR7 classifier identifies the source of the `PoolNotShared`
mechanism as the post-processing repair pipeline (§11.5). A tactical fix
would be patching one or more of those six repair functions.

But:
- PR7's §11.6 stop conditions explicitly fired on this option ("multi-module,
  >200 lines, multi-mechanism, root cause spans 5+ post-processing
  functions").
- Patching repair stages to preserve bijectivity is a recursive trap:
  each stage operates on the prior stage's output, so making stage N
  bijectivity-preserving requires that stage N-1 be bijectivity-preserving,
  which requires that stage N-2 be bijectivity-preserving, etc. This is
  the S-H clipping pattern that A15.6 deprecates.
- The earcut step itself does not give the algorithm enough freedom
  to maintain shared boundary discretizations under arbitrary face
  shapes. Yang §4.1.2 picks CDT specifically because boundary curves
  can be passed as explicit edge constraints.

An architectural fix replaces the per-face earcut + post-processing repair
pipeline with a §4.1.2 (c)-faithful CDT-with-boundary-constraints
tessellator. This means:
- Each face triangulated in its u-v parametric domain, not in 3D directly.
- Boundary curves discretized once into a sequence of u-v points per face.
- CDT (Livesu 2021 simplified earcut, per CLAUDE.md research refs)
  with the boundary curve as a constraint.
- The discretization is bijective by construction (no post-processing).
- The repair pipeline is removed.

This is approximately the work PR7 §11.7 Option B describes as "Cluster II":
"The bounded-path post-processing repair pipeline is the wrong abstraction
— it's operating per-face when the bijective contract is per-edge."

The architectural fix touches:
- `tessellate_solid_bounded` — replaces post-processing pipeline.
- `tessellate_planar_face_bounded` — switches to u-v + CDT.
- `discretize_edges` — produces per-face boundary samples in u-v
  rather than per-pool indices.
- `repair.rs` — substantial portions removed (~3000 lines deletable).
- `weld_mesh_vertices` removal (D-10) becomes possible after.

This is a multi-PR architectural refactor, not a tactical one. PR7 §11.7
Option B's recommendation to pivot to Cluster II rather than continue
tactical patching is consistent with this assessment.

---

## §4 Findings

Severity scale: **High** = blocks Yang assay improvement; **Medium** = correctness/clarity concern; **Low** = polish/info.

| ID | Severity | Title | Direction |
|----|----------|-------|-----------|
| YD-01 | High | Repair pipeline IS the S-H anti-pattern A15.6 deprecates | Replace with §4.1.2 CDT |
| YD-02 | High | Per-face earcut on 3D boundary polygons does not preserve §4.1.1 bijection under non-trivial topology | Replace with u-v CDT |
| YD-03 | High | `tessellate_solid_bounded` runs 6 sequential rewrites; bijection not maintained at any stage | Remove all 6; rebuild from §4.1.2 |
| YD-04 | High | The bounded path covers only the gate's narrow center (no arcs, no primitive params, no polygon-soup) | Widen the gate by making bounded path the universal tessellator per Yang §4.1.1 |
| YD-05 | Medium | PR7's `Pr7Classification::Other` indicates oracle's `restrict_to_shared_boundary` heuristic produces false positives | Tighten oracle: restrict to arena edge endpoint set rather than undirected position coincidence |
| YD-06 | Medium | The 5-revision pattern + `feedback_anchor_before_fix.md` indicates the team's planning capacity is structurally below the code path's complexity | Either add `PR_DEBUG=1` instrumentation as a pre-impl checklist item per FIP, or simplify the code path so dispatch is observable from a single-file read |
| YD-07 | Medium | R0033 anchor singleton: 7 PRs anchored on one assay case; no evidence the fix generalizes to the other 13 nb cases | Future PRs anchor on ≥3 nb cases of distinct shape classes |
| YD-08 | Medium | PR2 fix is partially correct (covers 360° revolves) but blocked from R0033 by AABB-disjoint short-circuit; widening the fix is non-trivial | The disjoint short-circuit calls `flood_fill_patches` which strips revolve params — investigate whether the short-circuit should preserve primitive params, or whether it should not call `flood_fill_patches` at all |
| YD-09 | Medium | The bounded-path gate (`mod.rs:217-235`) was characterized in `tessellation_bounded_gate.md` but never widened | Widening the gate (PR2 punch-list option a/b/c per spec §4) is independent of fixing the bijective contract within bounded path |
| YD-10 | Medium | Steiner-fan retessellation (`repair.rs:1413+`) introduces face-local centroid + radial edges; centroid is unique per face so neighboring faces' boundaries must include those radial edges as boundary subdivisions, but the function does not propagate them | Either remove Steiner-fan + ship correct tessellator upstream, or explicitly propagate boundary-incident interior subdivisions to neighbors (deeply unattractive — fits the S-H pattern) |
| YD-11 | Low | `repair.rs` is 3730 lines; bounded path uses 6 of 9 functions (3 are fan-path-only). Per FIP/P10 dead-or-decoupled code should be removed; the fan-path-only functions could be moved out | Move `fill_boundary_holes`, `close_near_boundary_chains`, `remove_isolated_triangles` to a `repair_fan.rs` to clarify scope |
| YD-12 | Low | `bijective.rs` oracle correctly keys on f32 byte-pattern (PR3 finding) but the rendermesh produces f32 by casting f64 — exact f32 reproducibility depends on identical f64 inputs casting to identical f32 | The oracle's contract is correct for the bounded path's vertex-sharing strategy. If the tessellator switches to u-v CDT (YD-02), the oracle should re-key on (face_id, u, v) tuples per Yang §4.1's parametric coordinates |
| YD-13 | Low | The seven specs (`tessellation_bounded_residuals.md` §1-§11 + companion specs) are dense and chronologically structured; they are not a maintainable knowledge base | Once the architectural fix lands, retire these specs into an audit-trail subdirectory and document the new contract in `ARCHITECTURE.md` |
| YD-14 | Low | The `Pr7Classification::Other` reason string ("oracle's `restrict_to_shared_boundary` heuristic ... must be from other position-coincident boundary segments") is the actual classification; future readers will be confused that "Other" is a real bug class. Rename to `OracleHeuristicFalsePositive` | Rename for clarity |
| YD-15 | Info | Cross-ref to auditor-c slice: the input-mesh-non-bijectivity that `flood_fill_patches` correctly reports is the upstream tessellation defect; it is also the ¬ §4.1.1 contract Yang's Cherchi 2022 input expects. The defect blocks Cherchi conformance from the input side, not the algorithm side | Auditor-c should flag whether Cherchi 2022 expects bijective input as a precondition (it does — `cherchi2022.txt` describes "explicit-arrangement" inputs) |

### Cross-references to prior audits

- **`cherchi_port_audit.md` D-10** — `weld_mesh_vertices` violation. YD-01,
  YD-02, YD-03 are all aligned with D-10's directive: "fix upstream
  tessellation to produce shared vertex IDs at face boundaries (bijective
  tessellation per Yang §4.1.1); remove `weld_mesh_vertices` from
  `label_cells`."
- **`cherchi_port_audit.md` Cluster I** (defensive guards mask upstream
  bugs) — YD-01 is the same anti-pattern in tessellation rather than
  Cherchi.
- **`yang_2025_audit.md`** — pre-existing per-step Yang audit; YD-04, YD-08
  cross with the upstream stages this audit covered.

---

## §5 What this slice did NOT cover

Honest scope limits:

1. **Non-`pool-not-shared` mechanisms in other linear-bounded cases.** PR7
   classified R0033 only. The other 13 linear-bounded cases with nb pairs
   (R0044, R0098, F0024, F0020, F0025, F0021, F0022, R0057, R0064, F0023,
   R0060, R0067, R0086) were not classified. R0044 with 30 nb pairs is
   the heaviest hitter; its mechanism breakdown is unknown. The conclusion
   that "the architectural gap is universal" assumes generalization from
   R0033 — this is plausible (per §3.3 the per-face-earcut mechanism is
   structural) but not empirically demonstrated.
2. **The 47 `?` cases that error before tessellation.** These are upstream
   of the tessellation slice — they fail in `yang_boolean: result
   validation failed`, `revolve self-intersection`, or
   `yang_boolean: triangle-plane`. Auditor-a (Yang pipeline conformance)
   and auditor-b (Yang assay failure-pattern analysis) cover these.
3. **The fan path.** `tessellate_solid_fan` and the three fan-path repair
   functions (`fill_boundary_holes`, `close_near_boundary_chains`,
   `remove_isolated_triangles`) are not in this slice. The fan path is
   the legacy S-H pipeline; A15.6 marks it for removal once Yang ships.
4. **Cherchi 2022 conformance.** The PR6 reproducer demonstrates that
   `flood_fill_patches` correctly reports input-mesh non-bijectivity, but
   whether Cherchi 2022's full algorithm matches the paper's specification
   is auditor-c's slice. YD-15 flags the cross-cut.
5. **NURBS surface patches.** Yang §4.1.1 + §4.1.2 are written for NURBS
   patches with u-v parametric domains. The current implementation handles
   `SurfaceGeom::Planar`, `SurfaceGeom::Cylindrical`, and a few others.
   None of these are NURBS-general. The architectural fix proposed in §3.4
   either restricts the contract to the surface types we actually use
   (planar + cylindrical + cone/sphere/torus) or extends the discretization
   path to NURBS. The latter is a substantially larger scope.
6. **The `tessellate_cylindrical_face_bounded` arc-loop limitation.** Per
   `tessellation_bounded_gate.md` §4, the bounded path's cylindrical
   tessellator does not handle arc-trimmed cylindrical loops; that is
   why the gate has `has_arcs` as an exclusion clause. Lifting `has_arcs`
   is one of three options (a)/(b)/(c) in spec §4; this audit does not
   recommend among them.
7. **Performance characteristics.** Bounded path performance vs fan path
   performance is not measured here. If the architectural fix replaces the
   3700-line repair pipeline with a u-v CDT, the runtime profile changes
   substantially. This is a downstream concern.
8. **Tooling for empirical anchor verification.** YD-06 calls for
   `PR_DEBUG=1` instrumentation as pre-impl checklist; building that
   tooling (and a corresponding skill or hook) is out of scope.

---

## §6 References

- `docs/references/yang2025_hybrid_boolean.txt` §4.1.1 (lines 518-642),
  §4.1.2, §4.4.1
- `/tmp/cherchi2022.txt` — Cherchi 2022 paper for cross-ref to auditor-c
- `governance/ENGINEERING_CONSTITUTION.md` §P5, §P8, §P9, §P10
- `governance/FEATURE_IMPLEMENTATION_PROTOCOL.md`
- `governance/ARCHITECTURAL_INVARIANTS.md` §A15.6
- `docs/audits/cherchi_port_audit.md` Cluster I + D-10
- `docs/audits/yang_2025_audit.md` — prior step-by-step audit
- `specs/tessellation_bounded_residuals.md` §1-§11 (~1000 lines, 7 PR amendments)
- `specs/tessellation_pr3_corpus_dump.md`
- `specs/tessellation_bounded_gate.md`
- `specs/tessellation_nonmanifold_fix.md`, `specs/tessellation_nonmanifold_repair.md`,
  `specs/tessellation_watertight_repair.md`
- Memory: `feedback_yang_only.md`, `feedback_no_last_bug.md`,
  `feedback_no_regression_chasing.md`, `feedback_anchor_before_fix.md`,
  `feedback_validate_against_corpus.md`
- `crates/kernel/src/tessellation/mod.rs:217-235` — bounded-path gate
- `crates/kernel/src/tessellation/mod.rs:3136-3289` — `discretize_edges`
- `crates/kernel/src/tessellation/mod.rs:3291-4163` — `tessellate_planar_face_bounded`
- `crates/kernel/src/tessellation/mod.rs:4164-4346` — `tessellate_solid_bounded`
- `crates/kernel/src/tessellation/repair.rs:31-3700` — six bounded-path repair stages + three fan-path stages
- `crates/kernel/src/tessellation/bijective.rs:209-475` — oracle (`pos_key`, `check_face_pair_bijective`, `restrict_to_shared_boundary`)
- `crates/kernel/src/tessellation/pr7_classify.rs` — PR7 4-class classifier
- Commits: `5f5423c` (PR1), `f01dd68` (PR2), `8ad64b5`+`720fa8d` (PR3),
  `7ee4805`+`436ed37` (PR4), `7607256` (PR5), `08127ad` (PR6), `3af7fd6` (PR7)
