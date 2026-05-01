# Spec — PR12: Yang Stage 1 bijective face-pair contract (`BijectiveFacePairOracle`)

Per FIP §3.2. Gates T3 (red-phase tests), T4 (implementation).

Author: agent-spec, team `yang-stage1-bijective-pr12`.

References:
- Yang 2025 §4.1.1 (error-bounded triangulation; bijective mapping construction).
- Yang 2025 §4.1.2 (dealing with adjacency; CDT retriangulation around boundaries).
- Yang 2025 §4.5.5 (coplanar preprocessing — pre-tessellation B-Rep splits + post-tessellation injection).
- Cherchi 2022 §3 — input precondition: meshes "manifold, watertight and with no self-intersections".
- `specs/oracle_validity_audit.md` (PR10) — `BijectiveFacePairOracle` MEDIUM-confidence baseline (R0031, R0081 only).
- `specs/yang_per_patch_labeling.md` (PR11) — context for cascade unmasking.
- `specs/pipeline_oracles.md` (PR9) — oracle harness.
- `docs/audits/pr11_adversary_validation.md` §5 F1 — 15-case Stage-1 first-fail list.
- `crates/kernel/src/tessellation/bijective.rs` — oracle + `NonBijectivePair` diagnostic.
- `crates/kernel/src/boolean/pipeline_oracles.rs::BijectiveFacePairOracle` (lines 247-303) — wrapper.
- `crates/kernel/src/boolean/yang_integration.rs:600-770` — pipeline call site + Stage 1 snapshot capture.
- `crates/kernel/src/boolean/coplanar_preprocess.rs::{inject_identical_footprint_mesh, inject_partial_overlap_mesh, replace_face_triangles, inject_face_with_shared_first}` — coplanar injection paths.
- `crates/kernel/src/tessellation/mod.rs::{tessellate_solid_ext, tessellate_solid_ext_with_lod}` — tessellation entry.
- `crates/test-harness/tests/pr4_r0033_t_junction_diagnosis.rs` — diagnostic capture pattern.

---

## 1. Goal

**User-visible**: PR11 jumped Yang corpus pass rate from 9 → 84 / 157. Stage 1 first-fails moved from
2 (PR10: R0031, R0081 only) → **15** (the 2 originals + R0007, R0014, R0020, R0021, R0034, R0035,
R0046, R0063, R0095, F0016, F0018, F0019, F0076). PR11 inadvertently inflated the Stage-1 bucket OR
the Stage-4b labeling fix cascade-unmasked latent Stage-1 defects that previously hid behind
Stage-4b first-fails (`first_failing_stage` returns the minimum). PR11 adversary §5 F1 hypothesised
the latter without empirical validation.

**Goal**: reduce Stage-1 first-fails toward the PR10 baseline of 2 (R0031, R0081). The actual
target is set in §8 once T2 has classified the 15 cases by failure mode; depending on T2's
findings, the goal becomes either:

- (Branch I) Fix the dominant cluster: target ≤5 Stage-1 first-fails.
- (Branch II) No dominant cluster: narrow PoC fix on 1-2 cases; target ≤13.
- (Branch III) PR11 regression in `flat_arrays_to_render_mesh` or similar: target = PR10 baseline (2).

**Architectural**: Yang §4.1.1 requires that "the discretization results are bijective to the B-Rep
models" — every B-Rep edge between two faces must have **byte-identical reciprocal directed mesh
edges** (`(p, q)` on face A appears as `(q, p)` on face B with bitwise-equal f64 positions, no
welding tolerance) [`bijective.rs:142-155`]. This is the input precondition for Cherchi 2022 §3-4
mesh arrangement. PR1 implemented the oracle; PR9 wired it into the pipeline-oracle harness; this
spec finalizes its production reachability post-PR11.

**What this PR does NOT claim** (per `feedback_no_last_bug.md`):
- Does not assume the 13 newly-visible cases share one root cause. T2 will classify; some cases
  will defer to PR13+.
- Does not interpret "Stage 1 = OK after PR12" as the input mesh is correct overall — Stage 1 is one
  invariant of many; the F2 audit (`oracle_validity_audit.md` §F2) shows even Stage 0 had a
  reachability defect that was not caught by Stage 1.
- Success criterion is **histogram shift**, not absolute zero.

---

## 2. Parameters

The Stage 1 oracle reads three artifacts per operand. They are NOT the operation's user-facing
parameters; they are the contract surface between tessellation/Stage 0 and the downstream pipeline.

The wrapper signature (existing — not changed):
```rust
pub(crate) struct BijectiveFacePairOracle;
impl StageOracle for BijectiveFacePairOracle { /* ... */ }
```
delegates to `kernel::tessellation::bijective::check_face_pair_bijective(rendermesh, face_map, arena)`.

| Parameter | Meaning | Source | Valid range | Error condition |
|---|---|---|---|---|
| `rendermesh: &RenderMesh` | Per-operand tessellation output (vertices/indices/face_ranges). | `tessellate_waffle_solid(&solid_X_mod, lod)` — captured `mesh_a_for_snap`/`mesh_b_for_snap` BEFORE Stage 0b/0c flat-array injection. | `face_ranges` non-empty; `indices.len() % 3 == 0`; every `range.start_index ≤ range.end_index ≤ indices.len()`. | Empty `face_ranges` ⇒ oracle examines 0 pairs ⇒ vacuous PASS (silent false-negative; see B1). |
| `face_map: &BTreeMap<u64, FaceIdx>` | KernelId → FaceIdx mapping for the **post-Stage-0a** B-Rep. | `solid_X_mod.face_map.clone()`. | For every `range.face_id.0` in `rendermesh.face_ranges`, `face_map[range.face_id.0]` is `Some(_)` and points to a face that exists in `arena.faces`. | A KernelId in `face_ranges` not in `face_map` ⇒ triangles fall into `FaceIdx(usize::MAX)` sentinel ⇒ those triangles disappear from the boundary-edge enumeration. |
| `arena: &TopoArena` | Post-Stage-0a B-Rep topology (half-edges, edges, faces, loops). | `solid_X_mod.arena.clone()`. | Every B-Rep edge has two distinct half-edges with `twin.twin == self`; for every pair of distinct faces sharing a B-Rep edge, both faces have non-empty triangle ranges in `face_map`. | Stage 0a left a dangling half-edge or split a face without populating `face_map` for the new sub-face. |

Snapshot-capture site (informational): `yang_integration.rs:740-765`. The capture re-clones each
artifact; **production callers do NOT install a snapshot collector** so the oracle is dormant
outside the corpus runner — the only changes within scope of this spec are diagnostics, oracle
contract, and (per §8) the pipeline producer side that mints these artifacts.

---

## 3. Branch Table

The 7 candidate failure modes from Explore Agent B's research, each a hypothesis for what causes a
Stage 1 violation. T2's empirical classification will collapse this table to a dominant subset.

| # | Failure mode | Trigger condition | Action expected | Current observed action | T2 hypothesis cluster |
|---|---|---|---|---|---|
| **B1** | Empty `face_ranges` post-injection | `flat_arrays_to_render_mesh` produces RenderMesh with `face_ranges = Vec::new()` (yang_integration.rs:106). | If Stage 1 oracle ever read this artifact, every face boundary set is empty → oracle examines 0 pairs → vacuous PASS. | The Stage 1 snapshot reads `mesh_a` (PRE-injection), NOT this helper's output. The helper is used for the Stage 0 snapshot only. **Therefore B1 is unlikely to affect Stage 1**, but listed for completeness because PR11 introduced the helper and it's the most plausible Branch-III culprit. | Branch III (PR11 regression — unlikely but must be empirically refuted) |
| **B2** | Triangle count change post-injection without face_ranges rebuild | `inject_identical_footprint_mesh` / `inject_partial_overlap_mesh` mutate `verts_a/tris_a/bijective_a` but do NOT mutate `mesh_a.face_ranges`. | If Stage 1 oracle saw post-injection state, `face_ranges` would point to triangle indices that no longer exist or describe wrong triangles. | The Stage 1 snapshot reads PRE-injection `mesh_a`, so the oracle sees a self-consistent (face_ranges, indices) pair. **B2 cannot affect Stage 1 as currently snapshotted.** | Out of scope (Stage 0 oracle territory) |
| **B3** | `face_map` desync after B-Rep face splitting in Stage 0a | `split_brep_for_coplanar_pairs` performs `split_edge_at` + `mef` to create a new sub-face but fails to insert the new `KernelId` → `FaceIdx` mapping in `solid_X_mod.face_map`, OR splits a face for which an existing tessellation `FaceRange.face_id` points only to the parent. | New sub-face's triangles fall into `FaceIdx(usize::MAX)` sentinel; oracle's per-face boundary-edge map for the parent face contains triangles that should belong to the new sub-face → spurious unmatched edges. | If true, this is the primary cascade-unmasked pattern: every coplanar B-Rep split risks introducing a face_map desync that PR10 hid behind Stage-4b first-fails. | Branch I candidate |
| **B4** | `face_ranges` corruption (overlapping/gapped triangle ranges) | Tessellation produces two `FaceRange`s with `range_x.end_index > range_y.start_index` AND non-equal `face_id`, or a `FaceRange` whose `[start_index, end_index)` covers triangles already covered by another range. | Triangles get assigned to multiple faces by `BijectiveMap::from_render_mesh`'s `tri_face_ids[start..end].fill(face_idx)`; later `fill` overwrites earlier; oracle's boundary edges become corrupted. | Latent — never observed but possible via `tessellate_*` codepath defects. | Cluster Z (other) |
| **B5** | Partial-overlap injection defect (shared-first ordering breakage) | `inject_face_with_shared_first` (coplanar_preprocess.rs:1647) intends to append shared verts verbatim then snap exclusive verts to them, but a bug in vertex-snapping tolerance or ordering causes byte-divergence between A's and B's shared region. | Same as B2 — post-injection state, not seen by Stage 1 oracle currently. | Out of scope (Stage 0 oracle territory) | Out of scope |
| **B6** | Tessellation lost/synthesized faces (degenerate, self-loop detection failure) | `tessellate_solid_ext` early-returns on a face (e.g. degenerate planar projection) without pushing a `FaceRange` for it, OR pushes a self-loop-detected face into a wrong code path that emits 0 triangles. | A face declared in `arena.faces` and `face_map` has 0 triangles in the rendermesh; for each B-Rep edge that face shares with a neighbor, the oracle reads `bnd_a` empty and skips the pair (`bijective.rs:407`) — the violation is **silent** for that face but the neighbor's boundary edges are still flagged unmatched against any OTHER neighbor. | Latent — would manifest as "operand A has X non-bijective pairs, operand B has 0" with the missing face being one specific FaceIdx in every diagnostic. | Cluster Z (other) |
| **B7** | Stage 1 oracle snapshot sequence defect (stale `face_map`) | The snapshot's `face_map_a` was captured from `solid_a_mod.face_map` (post-Stage-0a) but the snapshot's `arena_a` was somehow captured from a different version (pre-Stage-0a, or a clone made before splitting). | Diff between arena's known faces and face_map's known faces causes some `FaceRange.face_id` to map to a sentinel — same effect as B3. | Read of yang_integration.rs:740-765 shows both `face_map_a_for_snap` and `arena_a_for_snap` are cloned from `solid_a_mod` after Stage 0a splitting and after `tessellate_waffle_solid`. **B7 is highly unlikely** unless a borrow-order bug exists. | Branch III (PR11 regression — refute) |

**Cluster taxonomy** for T2's classification (each empirical case maps to one cluster):
- **Cluster X (cascade)** — case fired stages 1/2/4b/6 in PR10. R0031 is the canonical anchor.
- **Cluster Y (decoupled)** — case fired stages 1/4b/6 in PR10 but skipped Stage 2. R0081 anchor.
- **Cluster Z (other)** — case did not fire Stage 1 in PR10 at all (the 13 newly-visible cases).

T2 will sub-classify Cluster Z by branch B1-B7 using `NonBijectivePair` diagnostics
(unmatched_a_count, unmatched_b_count, sample positions).

---

## 4. Invariants

Yang §4.1.1 / §4.1.2 establishes four invariants Stage 1 must enforce. The oracle currently checks
**only I2 directly**. I1, I3, I4 are caller-side contracts that, if violated, manifest as I2
violations downstream. PR12's fix scope (§8) targets whichever invariant T2 finds most violated.

### I1 — Tessellation produces complete bijective map (Yang §4.1.1)

> "We convert the surface patches of the B-Rep models into NURBS surfaces as inputs of this step…
> a bijective mapping between each surface patch and its discretization is constructed."

For every face `f ∈ arena.faces` with a corresponding `KernelId k` in `face_map` (i.e.
`face_map[k] == f.idx`), the rendermesh contains at least one `FaceRange` with `face_id == k` AND
that range's triangle indices `[start/3, end/3)` reference at least one valid triangle.

**Measurable**: for each `f` in `face_map.values()`, count rendermesh triangles assigned to `f` via
`BijectiveMap::from_render_mesh`. Count == 0 violates I1. (Currently testable via
`BijectiveMap::is_complete()` + per-face count, BUT the existing `is_complete()` checks only that no
triangle has the sentinel `FaceIdx(usize::MAX)`, NOT that every face has triangles.)

### I2 — Byte-identical reciprocal boundary edges along shared B-Rep edges (Yang §4.1.1)

> "the discretization results are bijective to the B-Rep models if the NURBS surfaces are regularly
> defined, such that we can use the mesh intersection results to provide proper initialization to
> solve the B-Rep intersections."

For every B-Rep edge `e ∈ arena.edges` with adjacent faces `f_A ≠ f_B`, every directed boundary
edge `(p, q)` on face A's tessellation that lies on `e` has a byte-identical reciprocal `(q, p)` on
face B's tessellation. "Byte-identical" = `p[k].to_bits() == q[k].to_bits()` for all `k ∈ {0,1,2}`,
no welding/tolerance.

**Measurable**: `check_face_pair_bijective` enumerates all face pairs sharing a B-Rep edge (BReP
mode) or position-coincident vertices (polygon-soup fallback), computes per-face boundary directed
edges restricted to the shared boundary, and counts unmatched pairs. `report.is_bijective() == true`
iff I2 holds for every examined pair.

This is the contract the existing oracle enforces. The diagnostic format
(`NonBijectivePair { unmatched_a_count, unmatched_b_count, sample_unmatched_a, sample_unmatched_b }`)
is what T2 reads for empirical classification.

### I3 — `face_ranges` and `face_map` consistent with the RenderMesh fed to the oracle

For every `range ∈ rendermesh.face_ranges`:
- `range.face_id.0` is a valid key in `face_map`.
- `face_map[range.face_id.0].0 < arena.faces.len()`.
- `range.start_index ≤ range.end_index ≤ rendermesh.indices.len()`.
- Two ranges' `[start_index, end_index)` intervals do not overlap (each triangle belongs to exactly
  one face).

**Measurable**: linear scan over `face_ranges`. Currently NOT checked by the Stage 1 oracle. A
violation will manifest as either a sentinel `FaceIdx(usize::MAX)` in `tri_face_ids` (B3) or
overlapping faces (B4) → corrupted boundary-edge sets.

### I4 — Post-injection state preserves I1-I3 (Yang §4.5.5)

Stage 0b (`inject_identical_footprint_mesh`) and Stage 0c (`inject_partial_overlap_mesh`) mutate
the flat arrays `verts_a/tris_a/bijective_a` but do NOT update `mesh_a.face_ranges`. Per the snapshot
sequencing in `yang_integration.rs:740-765`, the Stage 1 oracle reads PRE-injection `mesh_a`, so I4
is **deferred to Stage 0** in the current architecture (`CoplanarMeshIdenticalOracle`).

**Status**: Currently NOT a Stage 1 contract. Listed here because if T2 finds Stage-1 cases that
inspection shows are caused by injection-time defects, the snapshot site MUST be re-evaluated (and
either the Stage 1 oracle must read post-injection state, or a Stage 0.5 oracle must be added).

---

## 5. Oracles

### Primary — `BijectiveFacePairOracle` (existing)

Wrapper at `pipeline_oracles.rs:247-303`. Delegates to `check_face_pair_bijective`. Reports:
```
non-bijective face pairs: operand A {a_count} pair(s) of {a_total}, operand B {b_count} pair(s) of {b_total}
```
with `ViolationKind::ContractViolated`. Returns `Ok(())` if both operands' BijectivityReport
satisfy `is_bijective()`.

`raw_reports(&PipelineState) → Option<(BijectivityReport, BijectivityReport)>` exposes the
detailed `NonBijectivePair` records to T2's diagnostic probe. Each record carries:
- `face_a`, `face_b` — the FaceIdx pair.
- `edge: Option<EdgeIdx>` — the shared B-Rep edge in BRep mode; `None` in polygon-soup mode.
- `unmatched_a_count`, `unmatched_b_count` — totals.
- `sample_unmatched_a`, `sample_unmatched_b` — first 4 unmatched directed edges as `([f64;3], [f64;3])`.

### Secondary — `BijectiveMap::is_complete()` (existing, supplementary)

Currently used only as a hard error gate at `yang_integration.rs:685-689`:
```
if !bijective_a.is_complete() || !bijective_b.is_complete() {
    return Err(KernelError::NotSupported { ... "bijective map has unmapped triangles" });
}
```
Catches B3-style sentinel pollution before Stage 1 oracle even runs — but this is a HARD ERROR, not
an oracle. T2's diagnostic should also probe whether any of the 15 cases hit this guard (those would
short-circuit before reaching the oracle and would NOT appear in the Stage 1 first-fail bucket — so
this is a sanity check).

### Diagnostic format for T2 (T3 / T4 will reuse)

Per case, capture:
```
case_id, operand (A | B), n_pairs_examined, n_pairs_violated,
  per_pair: { face_a, face_b, edge?, unmatched_a, unmatched_b, sample_a[..4], sample_b[..4] }
```
Suggested anchor file pattern: `crates/test-harness/tests/pr12_stage1_diagnostic.rs` (T2's deliverable).

---

## 6. Failure Modes

For each branch B1–B7 from §3, define detection mechanism, fix surface area, and blast radius.

| Branch | Detection mechanism | Fix surface area | Blast radius |
|---|---|---|---|
| **B1** Empty `face_ranges` | T2 instruments `flat_arrays_to_render_mesh` callers to emit `face_ranges.len()` and confirms it's only used for Stage 0 snapshot. If Stage 1 oracle hits an empty rendermesh, the `boundary_cache` for every face is empty and `total_pairs_examined == 0`. | If confirmed: 10-30 LOC fix in `flat_arrays_to_render_mesh` to derive `face_ranges` from the `_template: &RenderMesh` argument (currently unused) plus the post-injection triangle count delta. | Low — only Stage 0 snapshot affected; Stage 1 snapshot already uses raw `mesh_a`. |
| **B2** Triangle count change post-injection | Outside Stage 1 scope (Stage 1 reads pre-injection state). | n/a | n/a |
| **B3** `face_map` desync after Stage 0a | T2 dumps `face_map` keys + `arena.faces` count BEFORE and AFTER `split_brep_for_coplanar_pairs`. A new face appearing in `arena.faces` without a corresponding `face_map` entry is a desync. | If confirmed: ~100-300 LOC in `split_brep_for_coplanar_pairs` to emit a fresh KernelId for each new sub-face and insert into `face_map`. The `mef` and `split_edge_at` code paths in coplanar_preprocess.rs already track face index changes; the missing piece is `face_map.insert(new_kid, new_face_idx)`. | Medium — this is a structural fix to Stage 0a. May affect cases that currently PASS Stage 1 but only by accident (e.g. tessellation populates from `arena` directly, not from `face_map`). |
| **B4** `face_ranges` corruption | T2 verifies invariant I3 directly: linear scan over `mesh_a.face_ranges`, detect overlapping/gapped intervals. Diagnostic: dump first 10 ranges per case. | If confirmed: bug in a specific tessellation codepath — could be cap-tessellation, polygon-fan, or bounded path. Surface depends on which face exhibits the corruption. | Variable — could be small (one tessellation function) or large (cross-cutting concern in tessellation). |
| **B5** Partial-overlap injection defect | Outside Stage 1 scope (Stage 1 reads pre-injection state). | n/a | n/a |
| **B6** Tessellation lost/synthesized faces | T2 cross-references `face_map` keys against `mesh_a.face_ranges` `face_id` keys: any FaceIdx in `face_map` without a corresponding `FaceRange` is a lost face. Diagnostic: dump these per case. | If confirmed: bug in `tessellate_solid_ext`'s per-face dispatch — likely a degenerate-face early-return that fails to push an empty `FaceRange { face_id, start_index: x, end_index: x }`. ~5-50 LOC fix. | Low — additive only (push empty range so the oracle can SEE the missing face). |
| **B7** Snapshot sequencing | T2 inspects `yang_integration.rs:740-765` for borrow-order or clone-order anomalies. | If confirmed: ≤10 LOC fix in snapshot capture. | Trivial. |

**Decision matrix for §8** (lead resolves after T2 lands):
- B3 dominates (>= 8/15 cases) → Branch I, fix `split_brep_for_coplanar_pairs` face_map sync.
- B4 dominates → Branch I, fix the specific tessellation codepath. May spill to PR13.
- B6 dominates → Branch I, fix tessellation early-return.
- No dominant pattern → Branch II, narrow PoC on R0031 + R0081 (the PR10 baseline cases).
- B1 confirmed AND inflation count drops to 2 with that fix → Branch III, fix `flat_arrays_to_render_mesh`.

---

## 7. Research Basis

### Yang 2025 §4.1.1 (verbatim, line 518-523 of `yang2025_hybrid_boolean.txt`)

> "Our method first discretizes each closed B-Rep model composed of multiple surface patches into a
> triangle mesh. Then a bijective mapping between each surface patch and its discretization is
> constructed. The generated triangle mesh is a closed, watertight manifold under a given
> surface-to-mesh distance tolerance dε from the original B-Rep model."

### Yang 2025 §4.1.2 (verbatim, line 592-602)

> "Discretizing each patch by sampling regularly in its u-v domain requires techniques such as
> integer optimization to ensure watertightness between patches. We find it unnecessary; instead, we
> discretize each surface patch independently without considering its neighbors, re-sample the
> boundary curves, and reconstruct the triangulation around the boundaries. For each surface, we
> first triangulate the rectangular u-v domain until reaching the given distance tolerance dε. Then,
> for each boundary curve, apply constrained Delaunay triangulation (CDT) in CGAL to retriangulate
> the two adjacent surfaces around the boundary, and remove the trimmed area as in Diazzi et al.
> 2023, if it's a trimmed surface, generating a watertight mesh."

> "The discretization results are bijective to the B-Rep models if the NURBS surfaces are regularly
> defined, such that we can use the mesh intersection results to provide proper initialization to
> solve the B-Rep intersections."

**Implication**: bijectivity is a property of the **per-pair** boundary discretization. Yang's
construction discretizes each face independently (rectangular u-v domain), then resamples shared
boundary curves once and runs CDT on both adjacent faces using that shared sample. The shared
sample makes "byte-identical reciprocal directed edges" automatic — both sides receive the same
list of boundary points in opposite orientation.

### Cherchi 2022 §3 (precondition)

> "Input meshes are always assumed to unambiguously enclose a volume, that is, they are manifold,
> watertight and with no self-intersections."

The Cherchi 2022 mesh-arrangement algorithm (Yang Stage 2) requires this precondition. Yang Stage 1
delivers it for arbitrary B-Rep models; the Stage 1 oracle is the gate that asserts this delivery.

### PR10 oracle validity audit findings (relevant subset)

`oracle_validity_audit.md` §3, row "Stage 1 — `BijectiveFacePairOracle` — MEDIUM":
> "Not exercised by mutation testing in Task A. Task C confirms it fires on the 2 Stage1 first-fail
> cases (R0031, R0081). No false-positives observed. Empirical floor only."

**Implication**: PR10 baseline of 2 Stage-1 first-fails was the empirical truth at that revision.
PR11 introduced 13 additional cases without any change to bijective.rs or pipeline_oracles.rs's
Stage 1 wrapper (verified by `git show --stat e69add9 b2f830d`). The change must therefore be
**cascade unmasking** OR a **PR11 side-effect on the producer side** (tessellation /
coplanar_preprocess / yang_integration). Both must be empirically distinguished by T2.

### PR11 adversary report §5 F1 (cascade-unmask hypothesis)

> "Either way: the cases were ALREADY broken in PR10 — making S4b pass exposed S1's pre-existing
> brokenness. Filing a follow-up to investigate which of these 13 are real S1 bugs vs. cascade
> artifacts is left to PR12."

Adversary explicitly punted to PR12. **T2's diagnostic IS that punted investigation**.

---

## 8. Fix Scope (FINALIZED 2026-05-01 — Branch II per T2 diagnosis at `69664ec`)

T2 (`docs/audits/pr12_stage1_diagnostic.md`) classified the 15 cases:

- **Cluster X (S1+S2+S6 fire)**: 4 cases — R0007, R0020, R0021, R0031.
  - X-coplanar (S0 stub): R0007, R0031.
  - **X-non-coplanar (no S0 stub, no flap)**: R0020, R0021. ← Cleanest target.
- **Cluster Y (S1+S6 fire, S2=Ok)**: 8 cases — R0035, R0063, R0081, R0095, F0016, F0018, F0019, F0076.
- **Cluster Z (flap-prone)**: 3 cases — R0014, R0034, R0046.

PR10-vs-PR12 attribution: 6 CASCADE / 6 PR11-INTRODUCED / 3 flap (mixed mechanism).

**Branch II** selected because: largest cluster (Y) is 8/15, below the ≥10/15 dominant
threshold. Cluster Y is internally heterogeneous (some fire S0 OracleStub, some don't).
Branch III in pure form is wrong because 6 cases are genuine pre-existing cascade unmask.

### PR12 narrow scope (two independent steps)

**Step 1 — Determinism fix** (separable, broad benefit):

Per T2 §8 question 2: 4 cases (R0014, R0034, R0046, F0076) flap S1 fire/no-fire across
runs because `face_boundary_directed_edges` and related tessellation surfaces use
`HashMap<...>` whose iteration order depends on RandomState. Replace with `BTreeMap` (or
sort iteration by stable position-key) per project convention
(`feedback_no_regression_chasing.md` notes BTreeMap-style determinism).

- **Anchor**: `crates/kernel/src/tessellation/bijective.rs::face_boundary_directed_edges` —
  inspect any `HashMap<...>` usages and convert to `BTreeMap` or sorted iteration. Likely
  also affects the count-aggregation surface T2 referenced.
- **Approach**: replace `HashMap<...>` with `BTreeMap<...>` (keys are already comparable
  position tuples / index tuples). Verify by running T2's probe 3-5 times consecutively
  and confirming verdicts match across runs.
- **Estimated LOC**: 5-50.
- **Validation**: T2's diagnostic probe (`pr12_stage1_diagnostic.rs`) re-run 3+ times
  shows identical per-case verdicts. T3 includes a determinism-stability test.

**Step 2 — Cluster X non-coplanar root-cause fix** (R0020, R0021):

Per T2: R0020/R0021 fire S1+S2+S6 with NO S0 OracleStub (no coplanar preprocessing
involvement). Pure tessellation defect — the bijective contract is violated by
`tessellate_waffle_solid` itself, independent of injection. R0020 reports "A 2 pair(s) of
19" non-bij; R0021 reports "A 7 pair(s) of 23" — both small ratios (~10-30%) and operand
A only.

- **Anchor**: TBD by agent-impl. Most likely `crates/kernel/src/tessellation/mod.rs`
  per-face dispatch (lines 284-545) — but the impl agent must root-cause via
  instrumentation. The defect is operand-A-asymmetric (per T2 §5: 14/15 cases violate
  on operand A) — investigate whether solid_a's geometry triggers a specific tessellation
  codepath (e.g., revolve cap with shared edge pool, T-junction at boss boundary).
- **Approach**: agent-impl reproduces R0020 via T2's probe; instruments
  `face_boundary_directed_edges` to dump the unmatched edge positions; identifies which
  B-Rep edge / face pair generates the non-reciprocal. Then traces back to the
  tessellation site and applies a targeted fix.
- **Estimated LOC**: 50-300 depending on root cause.
- **Validation**: T3 includes red tests that fail on current code for R0020/R0021 and
  pass after impl.

### Out of scope (PR13+)

Explicitly deferred per Branch II:
- **Cluster X-coplanar** (R0007, R0031): require Stage 0 partial-overlap full
  implementation per Yang §4.5.5. Separate concern from tessellation bijectivity.
- **Cluster Y entirely** (R0035, R0063, R0081, R0095, F0016, F0018, F0019, F0076):
  decoupled defect (S2=Ok despite S1 fire). T2 §8 question 1 hypothesizes this is
  Cherchi vertex-merge hiding the defect from Stage 2 measurement — root cause for
  Cluster Y is non-trivial.
- **Cluster Z stable behavior** (R0014, R0034, R0046): even with the determinism fix
  (Step 1), these cases may still have real S1 defects underneath the flap. Re-evaluate
  in PR13 once the flap is removed.

### Success criterion

PR11 baseline: S1 = 15 first-fails (with 4-case flap noise).
PR12 success target: **S1 ≤ 10 first-fails AND verdicts stable across 3 consecutive
probe runs**.

Specific case targets:
- After Step 1 (determinism): R0014, R0034, R0046, F0076 verdicts stabilize. Whatever
  they settle on, that becomes the new stable count.
- After Step 2 (R0020 + R0021 fix): those 2 cases drop from S1 to S6-only (cascade
  resolves) or AllPass.

The 6 deferred Cluster Y cases plus 2 X-coplanar cases = 8 cases will still fail S1
post-PR12. That's acceptable per `feedback_no_last_bug.md` honest framing.

### Mid-execution amendments (2026-05-01)

**Step 1 widened** (per agent-impl `7e119cc`): the determinism fix had to extend beyond
`bijective.rs` to four `boolean/` files because `extract_face_boundary_2d`,
`extract_trim_boundaries`, `intersection_class.rs`, and `intersection_opt.rs` each
contained their own HashMap/HashSet sources of non-determinism. T3's empirical Test 5
(R0014 in-process flap) drove this discovery — `bijective.rs` alone was insufficient.

**Step 1 — count flap deferred** (PR13+): after Step 1+1b, **binary verdicts** (X/Y/Z
classification) are stable across runs, but **counts within S1 messages** still flap
(e.g., R0014 reports `5/7/8 pair(s)` across 3 runs). Hunt for the remaining source
hits external dependencies (`earcutr`, `geometry-predicates`, `dashu`); unbounded
investigation territory. Binary stability is sufficient for adversary mutation testing
+ cluster classification; count stability is a deferred quality-of-life concern.
F0076 also remains partially flap-prone (Y/Z across runs).

**Step 2 — DEFERRED to PR13** (per agent-impl Step 2 abort): the R0020/R0021 root
cause traces back to `crates/kernel/src/boolean/topology_extract.rs::extract_trim_boundaries`
trim-loop chaining (~lines 1095-1200), NOT to `tessellation/`. Empirical anchor
preserved by agent-impl's instrumentation:
- For both R0020 and R0021, every non-bijective pair has `un_a[i] == un_b[i]`
  byte-identically — both faces emit the SAME directed edge `(p, q)` in the SAME
  direction along the shared B-Rep edge. Per Yang §4.1.1, twin half-edges should
  produce reciprocal mesh boundary edges; one face's loop must be walking the shared
  edge in the wrong direction.
- Twin pairing in `build_result_brep` is verified correct (twin.twin == self), so
  the half-edges themselves are paired correctly. The TRIM_LOOP edge sequence chosen
  by `extract_trim_boundaries` is feeding `build_result_brep` half-edges in a
  direction inconsistent with the adjacent face.
- Estimated fix scope: 200-500 LOC in trim-loop chaining logic (CW-angular sort at
  branch points making locally-correct but globally-inconsistent direction choices).
- Architectural caveat per `tessellate_solid_bounded:4307-4310`: removing degenerate
  earcut triangles is forbidden because the current architecture relies on their
  edges for pairing — a naive fix at the tessellation layer breaks a different
  invariant. PR13 should target the trim-loop chaining directly.

**PR12 effective deliverable**: Step 1 + Step 1b only. Binary verdict stability
delivered; R0020/R0021 fix and count stability deferred to PR13.

**Adjusted PR12 success criterion** (per these amendments):
- Binary cluster verdicts (X/Y/Z) stable across 3 consecutive probe runs: ≥14/15 cases.
- No regressions in PR11 baseline (84 AllPass cases preserved).
- `un_a[i] == un_b[i]` archaeological finding documented for PR13.

### Post-adversary correction (T5 finding F1)

Adversary's V1 measurement on `a884562` showed **12/15** cluster-stable (not ≥14/15
as the amendment claimed). The two additional cluster-flappers are F0018 and R0046,
both flapping their **Stage 2** binary verdict (residual non-determinism downstream
of Step 1+1b's S1 fix). Per `feedback_no_last_bug.md`: this is honest framing — the
S1 binary verdict IS stable on 14/15, but the broader X/Y/Z classification (which
incorporates S2) is only 12/15 stable.

Updated residual-flap watch list for PR13:
- F0076: binary S1 verdict still flaps Y/Z (1/3 runs).
- F0018, R0046: cluster classification flaps because Stage 2's binary verdict still
  has non-determinism — likely another HashMap/HashSet on the S2 input path that
  PR12's Step 1b widening missed. Adversary's V5 mutation test confirms PR12's
  determinism fix is structurally load-bearing (rendermesh diverges, not just
  iteration noise) — meaning at least one more upstream non-determinism source
  exists for these cases.

### 8a. Candidate fix surfaces (pre-resolved per branch — lead picks based on T2 cluster sizes)

The following are concrete file:line anchors so lead can update §8 quickly:

#### If T2 selects Branch I — B3 (Stage 0a face_map desync) is dominant
- **Anchor**: `crates/kernel/src/boolean/coplanar_preprocess.rs::split_brep_for_coplanar_pairs`
  (lines 180-700-ish, depending on which `mef` / `split_edge_at` calls are missing the
  `face_map.insert(...)` follow-up).
- **Approach**: every site that mutates `solid_a.arena` to introduce a new face must also mutate
  `solid_a.face_map` to insert a fresh `KernelId` → new `FaceIdx`. The `id_alloc` pattern from
  `yang_boolean_from_solids` (yang_integration.rs:575) provides KernelId allocation.
- **Estimated LOC**: 100-300.
- **Validation**: each new face_map entry implies one more `FaceRange` in the post-tessellation
  `mesh_a.face_ranges`. T3 should add a unit test that splits a face via Stage 0a and asserts
  `mesh_a.face_ranges.iter().any(|r| r.face_id.0 == new_kid)`.

#### If T2 selects Branch I — B4 (face_ranges corruption) is dominant
- **Anchor**: depends on which tessellation function — `tessellate_solid_ext` (mod.rs:191),
  `tessellate_solid_bounded` (mod.rs:4164), `tessellate_circular_cap` (mod.rs:958), etc.
- **Approach**: every `face_ranges.push(FaceRange { ... })` site must invariantly cover EXACTLY
  `(end_index - start_index)/3` triangles (no overlap with subsequent ranges, no gaps for valid
  faces). If T2's evidence points to a specific tessellation codepath, anchor there.
- **Estimated LOC**: 5-50 in the offending function.
- **Validation**: T3 unit test on the offending fixture asserts I3 invariants (non-overlapping
  ranges, sentinel-free `tri_face_ids`).

#### If T2 selects Branch I — B6 (tessellation lost faces) is dominant
- **Anchor**: per-face dispatch in `tessellate_solid_ext` (mod.rs:284-545, `for &(kid, face_idx) in &sorted_faces`).
- **Approach**: every `match geom { ... }` arm and every `continue` early-return must emit a
  `FaceRange { face_id: KernelId(kid), start_index, end_index: start_index }` (empty range with
  start == end is fine; the oracle will see "this face declared zero boundary edges" and skip the
  pair safely, but `BijectiveMap::is_complete()` will still flag the face). Better: implement a
  fallback "this face produced 0 triangles" path that logs and returns an explicit error.
- **Estimated LOC**: 5-50.
- **Validation**: T3 fixture intentionally crafts a face that previously hit the early-return path.

#### If T2 selects Branch III — B1 (PR11 regression in `flat_arrays_to_render_mesh`)
- **Anchor**: `crates/kernel/src/boolean/yang_integration.rs::flat_arrays_to_render_mesh` (lines
  85-108).
- **Approach**: derive `face_ranges` from the `_template: &RenderMesh` argument — currently
  ignored. Specifically, copy `template.face_ranges` and re-anchor `start_index`/`end_index` to
  reflect the post-injection `tris.len() * 3` total. (This requires per-face tracking of which
  triangles came from which original face_range, which the injection paths currently don't
  preserve. A cleaner approach: have the injection paths thread a parallel `face_ranges_out: &mut
  Vec<FaceRange>` argument and rebuild it as triangles are appended.)
- **Estimated LOC**: 10-30 in the helper + 30-100 across the two injection functions to thread the
  output parameter.
- **Validation**: the diagnostic probe re-runs after the fix and observes Stage-1 first-fails drop
  from 15 → 2.

  **Critical caveat**: Branch III is unlikely to apply UNLESS the snapshot site at
  `yang_integration.rs:740-765` is changed to use the post-injection mesh for Stage 1 (currently it
  uses pre-injection `mesh_a`). If T2 finds that the snapshot capture itself is wrong (B7), this
  branch becomes B7+B1 combined.

#### If T2 selects Branch II — no dominant pattern
- **Anchor**: pick R0031 (Cluster X anchor) + R0081 (Cluster Y anchor) and produce 1-2 case-specific
  fixes.
- **Approach**: defer 11+ cases to PR13; PR12 ships the cluster-anchor fixes only and explicitly
  documents the remainder.
- **Estimated LOC**: 50-200 case-specific.
- **Validation**: T3 includes targeted regression tests on R0031 and R0081 fixture meshes.

---

## 9. Honest framing & uncertainties

Per `feedback_no_last_bug.md` and `feedback_no_regression_chasing.md`:

1. **The 13 newly-visible cases may NOT be cascade unmasking.** PR11's adversary §5 F1 hypothesised
   it but did not validate. Possible alternative hypotheses (refute via T2):
   - **Unrelated coverage gain**: PR11's per-patch labeling may have caused the corpus runner to
     reach Stage 1 oracle on cases that previously errored before snapshot capture (e.g. via
     `yang_boolean_inner` early-return on `tessellation produced empty mesh`). T2 must check
     whether each of the 13 cases was reaching Stage 1 in PR10 at all.
   - **Snapshot-capture changes**: PR11's F2 fix moved the Stage 0 snapshot site (per
     `oracle_validity_audit.md` §F2 remediation). If the Stage 1 snapshot site was inadvertently
     moved too, snapshot ordering could change which artifacts are captured. (Inspection of
     `yang_integration.rs:740-765` shows the Stage 1 snapshot site reads the same `mesh_a` /
     `solid_a_mod` bindings it did pre-PR11; this hypothesis is weak but T2 should verify.)
   - **Genuine PR11-introduced breakage**: a hunk in `e69add9` or `b2f830d` may have produced
     side effects on tessellation or Stage 0a face_map. (Code review of those commits shows no
     direct change, but cross-cutting concerns through `KernelError::NotSupported` early-returns
     are possible.)

2. **The PR10 baseline of 2 cases (R0031, R0081) was itself only MEDIUM confidence**
   per `oracle_validity_audit.md` §3 (no mutation testing applied to Stage 1). Even reaching the
   PR10 baseline does not prove the oracle is fully validated — only that we've returned to a
   measured-but-unmutated state. PR12 should not claim "Stage 1 validated"; it should claim
   "Stage 1 first-fail count returned to PR10 baseline".

3. **Per-case fix appetite**: per `feedback_no_last_bug.md`, the success criterion is histogram
   shift, not absolute zero. If T2 finds that fixing the dominant cluster takes >500 LOC or risks
   regressions in Stages 2/4b/6, lead should pick Branch II and explicitly defer the rest to PR13.

4. **Per `feedback_validate_against_corpus.md`**: T3 / T4 must validate against the full corpus
   (all 15 cases plus a known-PASS spot-check), not just the unit fixture. PR2's experience —
   "unit-test green is not GREEN" — applies.

5. **Per `feedback_anchor_before_fix.md`**: T4 (impl) must add `eprintln!` at the planned anchor
   function and run the test on a representative case BEFORE writing the fix. If the planned
   anchor function is not invoked, the diagnosis was wrong; T4 must abort and report.

---

## 10. Out of scope (PR13+)

- Minority Stage 1 root causes not addressed by PR12's dominant fix.
- Mutation testing for the Stage 1 oracle (would upgrade `BijectiveFacePairOracle` confidence from
  MEDIUM to HIGH per `oracle_validity_audit.md` Task A's methodology).
- The 25 Stage 2 first-fails (degenerate sub-triangles in Cherchi arrangement output).
- The 29 Stage 6 first-fails (cascade unmask + intrinsic twin-asymmetry).
- Stage 0 oracle reachability for partial-overlap pairs (PR9 §2.1 stub — a separate audit
  finding).

---

## 11. Governance compliance

- **FIP §1 P5**: 6 distinct agents (manager + spec + diagnose + test + impl + adversary).
- **FIP §2 red-before-green**: T3 red tests committed before T4 impl.
- **FIP §3 spec phase**: this file. §8 finalized post-T2.
- **P8 (cite research)**: Yang §4.1.1, §4.1.2 verbatim cited; Cherchi 2022 §3 cited.
- **P9 (no hack-to-green)**: any tolerance widening or special-case branch in §8a Branch I
  candidate fixes is forbidden — fix the root cause in tessellation / coplanar_preprocess or
  abort.
- **P10 (stay in slice)**: each agent stays in its stream.
