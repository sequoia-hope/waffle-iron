---
PR: Y16-FIX-ARCH
Stage: 0b spec
Author: spec-writer-n@pr-y14a-conformal-oracle
Reads: refs/cherchi2022_*.pdf §3 §4 §5 + Algorithm 1; refs/yang2025_hybrid_boolean.pdf §4.4.2;
       refs/cherchi2020_*.pdf §4; docs/audits/{pr_y16_fix_arch_canary,pr_y16_inv_validation,
       pr_y16_inv_f0020_discovery,yang_audit_c_cherchi2022}.md;
       crates/kernel/src/boolean/{exact_mesh,topology_extract}.rs;
       crates/test-harness/{tests/pr11_per_patch_labeling.rs,
       tests/cherchi2022_reference_parity.rs, src/cherchi_sidecar.rs};
       /home/claude/cherchi2022/InteractiveAndRobustMeshBooleans/code/booleans.cpp
Roles: spec-writer-n, test-author-e, implementer-r, adversary-14, team-lead
---

# PR-Y16-FIX-ARCH — Cherchi 2022 §5 per-patch labeling, full architectural alignment

Scope decision (A) — per canary memo §4: per-patch refactor only. F0020/F0030/F0050
inputs all PASS Cherchi `mesh_booleans_inputcheck` (Stage 1 is healthy); the
defect is the architectural deviation YC-05 + YC-06 in `flood_fill_patches` /
`label_cells`. No Stage 1 cleanup expansion.

## §1 Goal

Yang 2025 §4.4.2 delegates the in/out classification stage to Cherchi 2022 §5
without modification: *"This is exactly Cherchi 2022 §5 Algorithm 1."* Cherchi
2022 §5 (paper p. 6, line 386):

> "An important aspect of such an approach is that the algorithm scales with
> the number of patches in the arrangement and not with the number of triangles
> in the mesh."

This PR aligns `flood_fill_patches` + `label_cells` with Cherchi 2022 §5 +
Algorithm 1. Three invariants are committed (§2). Per `feedback_yang_only.md`
the paper IS the spec; no fallbacks, no alternative paths if the manifold-edge
flood does not converge.

## §2 Reference parity contract

Three invariants. Each lists (a) paper citation, (b) reference C++ site,
(c) what current Rust does instead, (d) test that catches violation.

### I-A — Manifold-edge barrier

`edge_is_patch_boundary(e) iff incident_tri_count(e) != 2`.

- **Paper**: Cherchi 2022 §5 p. 6 line 386–388; Algorithm 1; computeSinglePatch
  pseudocode quoting `edgeIsManifold(e_id)` as the flood predicate.
- **Reference C++**: `booleans.cpp:412` — `if(tm.edgeIsManifold(e_id))` then
  recurse, else `else // e_id is not manifold -> stop flooding` at L425; and
  L428–429 mark patch-border vertices via `setVertInfo(...,1)`.
- **Current Yang Rust** (`topology_extract.rs:545`): flood stops at
  `intersection_edges.contains(&(v0, v1))` (cross-mesh edges only — YC-06).
  A same-mesh manifold-edge that happens to be cross-mesh becomes a barrier;
  a same-mesh non-manifold edge (3+ incident tris from one mesh, or a self-cross)
  is NOT a barrier. The two predicates disagree even on F0020's PASSING
  boolean (canary memo §3: 28 yang-barrier edges vs 0 manifold-barrier edges).
- **Catches**: `pr11_per_patch_labeling::per_patch_label_uniformity_red_phase`
  (Test 1) + `spotlight_f0020/f0030/f0050` + sidecar parity tests (§6).

### I-B — One ray per patch

`label_cells` ray-casts at most `len(graph.patches)` times per invocation,
NOT `len(subdivided.tris_a) + len(subdivided.tris_b)` times.

- **Paper**: Algorithm 1 (paper p. 6) — outer loop `for each patch P`, inner
  body `Define ray r from p ∈ P to p∞ ... compute and sort intersections r ∩ M`.
  §5 line 388 explicitly: "scales with the number of patches ... not with the
  number of triangles".
- **Reference C++**: `booleans.cpp:597–620` — `tbb::parallel_for` over patches;
  inside, exactly one `findRayEndpoints` + one `intersects_box` + one
  `analyzeSortedIntersections` + one `propagateInnerLabelsOnPatch` per patch.
- **Current Yang Rust**: `exact_mesh.rs:2079` already iterates `graph.patches`
  per-patch with one `classify_flat(representative)` call per patch (PR11
  shipped this). The loop SHAPE is correct; the gap is that
  `flood_fill_patches` builds its OWN local `patches: Vec<Patch>` (Step 5,
  L526–565) using the Yang barrier — it does not consume the
  `ManifoldPatchGraph` that fed `label_cells`. Refactor: replace Step 5's
  local patch construction with a call into `build_manifold_patch_graph`
  semantics (or directly reuse `ManifoldPatchGraph` via a shared helper) so
  Stage 4b labels and Stage 5/6 patch boundaries see the same partition.
- **Catches**: a debug-assert in `label_cells` that `patches.len() <=
  subdivided.tris_a.len() + subdivided.tris_b.len()` (already implicit by
  construction); empirically: spotlight cohort GREEN.

### I-C — Per-patch label propagation

Every triangle in a patch shares its patch's label, set in a single
propagation pass.

- **Paper**: Cherchi 2022 §5.3 Figure 5 + line 467: "the test is performed on
  the patch using a single ray ... propagate to all member triangles".
  Algorithm 1 line: `propagateInnerLabelsOnPatch`.
- **Reference C++**: `booleans.cpp:619` — single call
  `propagateInnerLabelsOnPatch(patch_tris, patch_inner_label, labels)`;
  body at L1304–1307: `for(uint t_id : patch_tris) labels.inside[t_id] = patch_inner_label;`.
- **Current Yang Rust**: `exact_mesh.rs:2113–2118` already propagates the
  representative's label to every member of the `ManifoldPatchGraph` patch via
  per-side slot writes. This invariant holds in `label_cells` today by PR11.
  Gap: the *downstream* `flood_fill_patches` Step 6 boundary collection
  (L688–L708) reconstructs its own per-patch boundary from the local Yang
  patch — even though `label_cells`'s per-patch label exists, the boundary
  builder ignores it. Refactor: Step 6 must consume the same
  manifold-edge-barrier patch partition that produced the labels.
- **Catches**: `pr11_per_patch_labeling::per_patch_representative_pick_anchor_red_phase`
  (Test 2) + `LabelConsistencyWithinPatchOracle` Stage 4b verdict (drops to 0
  fires post-refactor); `[twin-oracle]` block at end of `flood_fill_patches`
  (PR-Y16-INV deliverable; STAYS, see §9) reports `unpaired_count=0` for the
  cohort.

## §3 Data structures (existing, reuse)

- **`ManifoldPatchGraph`** (`exact_mesh.rs:1829–1839`, PR8 commit `542b4a2`):
  `patch_of: Vec<usize>` + `patches: Vec<Vec<usize>>` + `tris_a_count: usize`.
  Flat indexing convention: `[0..n_a)` are A sub-tris, `[n_a..n_a+n_b)` are
  B sub-tris. **NO STRUCT CHANGES** — the existing fields are exactly what
  Cherchi's `phmap::flat_hash_set<uint>` patch list provides. `build_manifold_patch_graph`
  (`exact_mesh.rs:1856`) already implements `computeAllPatches`-equivalent BFS
  with `incidents.len() != 2` (manifold) as the barrier — ALREADY CHERCHI-FAITHFUL
  at this layer.
- **`CellLabeling`** (`exact_mesh.rs:1189`): output of `label_cells`. Signature
  unchanged from PR11.
- **`ResultTopology`** (`topology_extract.rs:141–148`): `arena: TopoArena +
  face_provenance + edge_is_intersection`. Output shape unchanged.

If implementer-r discovers `ManifoldPatchGraph` needs a field (e.g.,
per-patch `source: SourceFace` because Step 5a previously split-by-source),
escalate to team-lead BEFORE editing the struct.

## §4 Functions to refactor

### `label_cells` — `exact_mesh.rs:1964` (~20 LOC delta, mostly comment cleanup)

- **Current**: per-patch loop already shipped (L2079–L2120). One ray per
  patch, label propagated to all members.
- **Target**: same shape; cite Cherchi 2022 §5 Algorithm 1 in code comment;
  remove any "PR8 phase 1 of 3" / "consumed by PR9" stale comments now that
  the refactor is COMPLETE. Verify `#[allow(dead_code)]` is dropped from the
  function once `flood_fill_patches` consumes the same graph.
- **Note for implementer-r**: if `label_cells` IS already correct (PR11
  shipped per-patch), the LOC delta here is near-zero. Most of this PR's
  weight is in `flood_fill_patches`.

### `flood_fill_patches` — `topology_extract.rs:395` (~150–250 LOC delta)

- **Current Step 4 (L489–L514)**: builds `boundary_edges` + `intersection_edges`
  using the cross-mesh predicate (`has_diff_mesh`).
- **Current Step 5 (L516–L565)**: BFS flood with barrier predicate at L545:
  `intersection_edges.contains(&(v0, v1)) || !directed_edge_to_tris.contains_key(&(v1, v0))`.
- **Current Step 5a (L567–~660)**: split-by-source-face post-pass (a workaround
  for Step 5's same-mesh-spanning patches; becomes unnecessary under
  manifold-edge barrier).
- **Current Step 6 (L688–L763)**: per-patch boundary collection + loop chaining.
- **Target**:
  1. **Replace Step 4 + Step 5** with manifold-edge barrier: build an
     `undirected_incidence: BTreeMap<(usize,usize), usize>` over `all_tris`'s
     undirected edges (canonical key `(min(v0,v1), max(v0,v1))`); flood
     traverses an edge iff `undirected_incidence.get(&key) == Some(&2)`.
     This matches Cherchi 2022 §5 (paper p. 6, line 414 ref-impl `edgeIsManifold`).
  2. **Eliminate Step 5a** (split-by-source-face): under manifold-edge barriers,
     the patch IS a manifold component by construction (Cherchi §5 invariant).
     Per-source splitting is no longer needed for B-Rep face mapping IF Step 6
     correctly maps each patch to its dominant `SourceFace` via majority-vote
     across the patch's member sub-tris (B-Rep face provenance is a weaker
     output requirement than Cherchi-correctness; one face = one
     analytical-surface mapping is preserved as long as the flood stays on a
     single source face — which it now does, because cross-source edges in the
     same mesh that are non-manifold are barriers, and same-source manifold
     edges are interior).
  3. **Step 6 boundary collection** (L688–L708): the `is_boundary` predicate
     STAYS — it is now consistent with the manifold-edge barrier (an edge is a
     patch boundary iff its incidence is != 2 OR all reverse-direction
     neighbors lie in different patches).
  4. **Step 6 loop chaining** (L724–L757): unchanged.
- **Anchor canary site (mandatory pre-impl per `feedback_anchor_before_fix.md`)**:
  per canary memo §5, implementer-r MUST add an `eprintln!` at
  `topology_extract.rs:~L477` BEFORE writing refactor code. Build
  `undirected_incidence` over `all_tris`; print
  `manifold-barrier-count = count(incidence != 2)` and
  `yang-barrier-count = intersection_edges.len()`. Run
  `cargo test -p test-harness --test assay_randomized -- spotlight_f0020 \
  --ignored --nocapture` and confirm the canary FIRES with non-zero
  manifold-barrier-count for F0020's failing boolean (canary memo recorded 10).
  **ABORT** if 0 fires. Remove the canary before real impl.

### Deletion candidate (out of scope this PR; bank for PR-Y17)

The Step 5 + Step 5a Yang-barrier code (L516–~660) becomes dead code once
the manifold-edge flood replaces it. PR-Y16-FIX-ARCH leaves the dead code in
place via `#[allow(dead_code)]` to keep the diff focused on the new path; a
follow-up PR (PR-Y17-CLEANUP) deletes it after corpus validation.

## §5 Test scaffold reuse

`crates/test-harness/tests/pr11_per_patch_labeling.rs` (818 LOC) is the
binding RED suite. Five tests; spec mandates test-author-e UNIGNORES all 5
and adds NO new RED tests for behavior covered there:

| Test | Invariant | Today | Post-refactor |
|---|---|---|---|
| `per_patch_label_uniformity_red_phase` | I-A + I-C (within-patch label uniformity, no S6 cascade) | RED on candidate slice | GREEN |
| `per_patch_representative_pick_anchor_red_phase` | I-B + I-C (rep-pick equivalence; no S4b violator and no S6-only violator) | RED | GREEN |
| `f1_upstream_conservation_anchor_red_phase` | F1 conservation; doc-only | trivial pass | trivial pass |
| `f2_post_injection_oracle_anchor_red_phase` | F2 post-injection capture; doc-only | trivial pass | trivial pass |
| `per_patch_labeling_determinism_red_phase` | I5 determinism (same case 2× → byte-identical verdicts) | passes | passes |

**New tests added by test-author-e (sub-phase 0c)**:

1. `spotlight_f0030` — mirror of `spotlight_f0020` at
   `assay_randomized.rs`, ~20 LOC. Asserts `Status == Passed` post-refactor.
   Confirm RED on current main.
2. `spotlight_f0050` — same shape. Confirm RED on current main.
3. `cherchi2022_reference_parity.rs` — UNIGNORE existing `f0002_*` if cohort
   covers it; ADD parity tests for F0020/F0030/F0050 + 3 controls
   (F0001/F0061/F0099). See §6 for criteria.

## §6 Sidecar parity oracle

Differential test against `mesh_booleans` per `cherchi_sidecar.rs` +
`cherchi2022_reference_parity.rs`. Per case:

1. Run Waffle Yang pipeline on the case; capture boolean-output mesh as OBJ
   (existing `YANG_DUMP_OBJ_BASE` mechanism, or post-refactor result).
2. Run `mesh_booleans union <a>.obj <b>.obj <out>.obj` (30 s timeout).
3. Compare canonicalized outputs.

**Canonicalization** (verify the harness uses these; if not, override per-test):
- Vertex bag: round to 1e-6 absolute, sort lexicographically. Compare bags.
- Triangle bag: each tri → canonical vertex tuple (sort 3 tuple by canonical
  vertex idx); sort triangles. Compare.
- Tolerance: 1e-6 absolute on coordinates (Waffle's Yang d_epsilon-equivalent;
  explicitly NOT byte equality).

**Cohort cases**: F0020, F0030, F0050.
**Control cases**: F0001, F0061, F0099 (currently passing — guard against
silent regression on healthy slice).

**F0030 lower bar (per canary memo §4 footnote — mandatory carve-out)**:
Cherchi's OWN union output on F0030 is non-manifold + locally-misoriented
(canary memo §2: "Manifold + Local Orient FAIL on Cherchi's output";
watertight + global orient + intersection-free PASS). F0030 has an intrinsic
geometric ambiguity (Cherchi 2022 §6 boolean-evaluate edge-case discussion).
Therefore F0030's sidecar parity test asserts ONLY:

- Watertight ✓
- Intersection-free ✓
- Global orientation consistent ✓

NOT manifold, NOT local orientation. F0020 and F0050 + the 3 controls assert
the full 5-check `is_well_formed` invariant. This lower bar is documented in
the test docstring with a citation to canary memo §4 footnote — it is CONTRACT,
not aspirational.

## §7 Test plan (FIP §4.2)

| Test surface | Pre-refactor | Post-refactor (gate) |
|---|---|---|
| `pr11_per_patch_labeling.rs` (5 tests, unignored) | 5 RED | 5 GREEN |
| `assay_randomized::spotlight_f0020/f0030/f0050` | 3 RED | 3 GREEN |
| `cherchi2022_reference_parity.rs` (cohort + controls) | cohort RED, controls GREEN | all GREEN (F0030 lower bar) |
| Yang fast subset 157 cases (`yang_fast --ignored`) | 11/157 baseline | ≥11/157, target improvement; document delta |
| Existing 183 kernel + test-harness tests | pass | NO regression |
| `cargo clippy -p kernel -p test-harness` | clean | no NEW warnings |
| `cargo fmt --check` | clean | clean |

Determinism: `per_patch_labeling_determinism_red_phase` (run case twice; verdict
vectors byte-identical) is the regression canary against representative-pick
non-determinism.

## §8 Adversary-13 amendments addressed

1. **F0030 `collision_count=2`** (validation memo §3): per-patch labeling
   architecturally preempts collisions because the boundary edges (Step 6)
   are derived from the SAME patch partition that fed the labels —
   collisions arose from Yang-barrier patches dropping forward HEs whose
   reverse lay in a different patch (investigator-a hypothesis (a)).
   Manifold-edge barrier guarantees that for every patch boundary edge, the
   reverse half-edge exists on a triangle in EITHER the same patch (interior
   loop) or another patch's boundary loop — collisions are
   architecturally-impossible under §5 invariant. **Empirical gate**:
   `spotlight_f0030` GREEN + `[twin-oracle]` `collision_count=0`. If F0030
   still shows `collision_count > 0` post-refactor, escalate to team-lead;
   per the adversary memo §5 this would mean a SECOND defect mode coexists
   beyond the per-patch-labeling gap (likely tied to F0030's intrinsic
   geometry — see §6 lower-bar parity).

2. **F0050 silent oracle fire** (validation memo §3.5 implicit): the oracle
   fires but `validate_yang_result_topology` does not. Per-patch labeling
   addresses the SOURCE (twin asymmetry from missing reverse HE in some
   patch); F0050 post-refactor MUST: (a) `[twin-oracle] unpaired_count = 0`
   on both Extrude 2 and Extrude 3 booleans, (b) `Status: Passed`. **Empirical
   gate**: `spotlight_f0050` GREEN. If oracle still fires silently
   post-refactor, the architectural fix is incomplete for F0050 and
   adversary-14 reports it as a finding (not a blocker IF cohort sweep
   confirms reduced violation rate elsewhere).

3. **Cherchi 2022 §5 conformance — LOCAL or ARCHITECTURAL**: this PR commits
   to **ARCHITECTURAL** (validation memo §5 alternative). Rationale:
   user directive "we must follow yang and cherchi completely" + canary
   memo §4 confirms scope (A); the LOCAL fix is rejected because it
   preserves YC-06 deviation indefinitely, costing future correctness
   guarantees as the corpus grows.

4. **PR-Y15c-fix-2 cascade ruling-out**: validation memo §4 confirms NO
   cascade — F0020 surfaces in `flood_fill_patches`, NOT in
   `result_topology_to_waffle_solid`'s `surface_map`-panic path. This PR
   does not touch `yang_integration.rs` or `surface_map`.

## §9 Anti-scope (explicit OUT)

- **Performance** — YC-12 cached predicates, YC-13 TBB parallelism, YC-22
  swiss tables. Defer to PR-Y17-PERF.
- **Cherchi precision/robustness extensions** — YC-04 LPI sort exact
  comparator (different defect class), YC-01 `findRayEndpoints` cascade
  (axis-aligned robustness, not blocking F0020 cohort), YC-08 vertex/edge
  ambiguity dispatch.
- **Stage 1 cleanup** — YC-07 `weld_mesh_vertices` (separate PR-Y19);
  Stage 1 manifoldness (canary §1 PASS confirms not needed for F0020/F0030/F0050).
- **Infrastructure changes**:
  - Modifying `ManifoldPatchGraph` struct (use as-is per §3).
  - Modifying `cherchi_sidecar.rs` harness (use as-is per §6).
  - Removing PR-Y16-INV's post-pairing `[twin-oracle]` block at
    `flood_fill_patches`'s end — it STAYS as the regression canary that
    catches if the refactor reintroduces twin defects.
  - Removing the deprecated S-H clipping pipeline (separate, larger PR).
- **Adjacent issues**: F0031–F0040 cylindrical quad-strip; R0020/R0021 render-LOD;
  R0071 kernel hang; deferred fillet/chamfer/shell.
- **Fallback paths**: per `feedback_yang_only.md` + Engineering Constitution
  P9–P10, implementer-r MUST NOT add fallback paths if patches do not flood
  cleanly. If a corpus case fails the manifold-edge flood (e.g., produces
  zero patches or panics on empty boundary), implementer-r escalates to
  team-lead via heartbeat; team-lead may invoke ABORT-and-rescope. Tolerance
  widening, special-case branches, "if patches.is_empty() return ..."
  stubs: FORBIDDEN.

## §10 FIP role table + acceptance gates

| Sub-phase | Agent | Inputs | Output | Acceptance gate |
|---|---|---|---|---|
| 0a (DONE) | canary-runner | sidecar harness; F0020/F0030/F0050 | `pr_y16_fix_arch_canary.md` | scope decision (A) shipped |
| 0b (THIS) | spec-writer-n | 0a memo + adversary-13 amendments + papers + scaffold | `yang_pr_y16_fix_arch_per_patch_cherchi.md` | §1–§10 non-empty; §2 4-tuple cited; §6 F0030 lower bar; §8 amendments addressed |
| 0c | test-author-e | this spec | unignore PR11 5/5; +`spotlight_f0030`; +`spotlight_f0050`; sidecar parity unignores + cohort + controls | RED suite confirmed RED on current main; controls GREEN; NO impl code touched |
| 0d | implementer-r | this spec + 0c tests + Cherchi 2022 §5 + ref C++ + `ManifoldPatchGraph` | refactor `flood_fill_patches` + (~no-op cleanup) `label_cells` | §4 anchor canary FIRES on F0020 BEFORE writing impl code; PR11 5/5 GREEN; spotlights GREEN; sidecar parity GREEN; 183 existing tests no regression; `cargo clippy + fmt` clean; Yang fast 157 baseline ≥11 |
| 0e | adversary-14 | all 0a–0d + sidecar harness | `pr_y16_fix_arch_validation.md` (~250 LOC) | independent re-run byte-equivalent; full 157 corpus sweep delta documented; sidecar parity on the 11 currently-passing cases (no silent regression); F0050 silent-oracle question answered; verdict ACCEPT/AMEND/REJECT |
| 0f | team-lead | all 0a–0e | clippy/fmt + WASM rebuild + memory updates + commit | `yang-debug-pane.spec.js` 4/4 GREEN post-WASM-rebuild; memory + commit pushed |

Sub-phase 0c/0d/0e/0f acceptance is gated by the row's gate column. 0d's
canary-FIRES-before-coding is the load-bearing pre-impl gate per
`feedback_anchor_before_fix.md`; canary-runner has pre-verified it FIRES with
data on F0020/F0030/F0050 (canary memo §3 table; §5 site
`topology_extract.rs:~L477`). Treat as PRE-VERIFIED, not candidate.

---

*Spec ready. Routing to test-author-e for sub-phase 0c.*
