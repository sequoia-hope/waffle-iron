# Spec — PR13: Yang trim-loop chaining bijective fix (R0020/R0021)

Per FIP §3.2. Gates T3 (red-phase tests) and T4 (implementation).

Author: agent-spec, team `yang-trim-loop-chaining-pr13`.

References:
- Yang 2025 §4.1.1 (error-bounded triangulation; bijective mapping construction).
- Yang 2025 §4.1.2 (dealing with adjacency; CDT retriangulation around boundaries).
- Cherchi 2022 §3 (input precondition: manifold, watertight, no self-intersections).
- `specs/yang_stage1_bijective.md` (PR12 spec) — particularly §8 amendments capturing the
  archaeological anchor (`un_a[i] == un_b[i]` byte-identical) deferred to PR13.
- `specs/yang_per_patch_labeling.md` (PR11 spec) — context for cascade unmasking.
- `specs/oracle_validity_audit.md` (PR10) — `BijectiveFacePairOracle` MEDIUM-confidence baseline.
- `docs/audits/pr12_stage1_diagnostic.md` (PR12 T2) — Cluster X classification of R0020/R0021.
- `docs/audits/pr12_adversary_validation.md` (PR12 V4) — R0020/R0021 stable X/X/X across 3 runs.
- `crates/kernel/src/boolean/topology_extract.rs::extract_trim_boundaries` (lines 967–1224) — the
  function being fixed.
- `crates/kernel/src/boolean/topology_extract.rs::build_result_brep` (lines 121–333) — twin pairing,
  verified correct.
- `crates/kernel/src/tessellation/bijective.rs::check_face_pair_bijective` (line 322) and
  `face_boundary_directed_edges` (line 240) — the oracle.
- `crates/kernel/src/boolean/pipeline_oracles.rs::BijectiveFacePairOracle` — pipeline wrapper.

---

## 1. Goal

**User-visible**: PR12 reduced Stage 1 first-fails to 13–15 (binary verdicts now stable per V4)
but explicitly deferred the R0020/R0021 root cause to PR13. R0020 (`A 2 pair(s) of 19`) and R0021
(`A 7 pair(s) of 23`) are the cleanest non-bijective cases per PR12 T2: stable Cluster X
(`. X X . . X` — S1+S2+S6 fire), no S0 OracleStub, no flap.

**Goal**: fix the bijective-contract violation for R0020 and R0021 by repairing
`extract_trim_boundaries`'s trim-loop chaining so that adjacent faces sharing a B-Rep edge emit
**byte-identical reciprocal directed boundary edges** (`(p, q)` on face A appears as `(q, p)` on
face B with bitwise-equal f64 positions). Both cases should drop S1 first-fail; cascade should
also resolve their S2 + S6 violations, lifting them to AllPass.

**Architectural**: per Yang §4.1.1, the discretization must be bijective to the B-Rep model — at
shared B-Rep edges, the two adjacent face triangulations must agree on the boundary directed-edge
sequence, oriented oppositely. PR12 archaeology proved the anchor: every non-bijective pair on
R0020/R0021 has `un_a[i] == un_b[i]` (face A's loop on the shared edge is walked in the SAME
direction as face B's loop — twins should walk opposite). The defect is in the trim-loop chaining
choice at branch points in `extract_trim_boundaries`, NOT in `build_result_brep` twin pairing
(verified `twin.twin == self`) and NOT in tessellation (the per-face sub-triangle sets ARE
correctly identified by `face_survival_detect`).

**What this PR does NOT claim** (per `feedback_no_last_bug.md`):
- Does not assume the 9 violations (2 R0020 + 7 R0021) share one identical micro-trigger. T2
  diagnoses; some sub-cases may defer to PR14+.
- Does not claim "trim-loop chaining is now correct" — only that R0020 and R0021 specifically
  pass the Stage 1 oracle, and adjacent-face reciprocity holds for the configurations they
  exercise. Other Stage 1 cases (Cluster Y, Cluster X-coplanar, Cluster Z residual flap) are
  out of scope and remain in PR14+.
- Does not assume the fix is Yang-faithful by construction — if T2 finds the fix can only be
  done as a post-hoc cross-check (Approach B), §8 amendment must explicitly flag that as a
  Yang deviation requiring future re-architecting.
- Success criterion is **histogram shift** on R0020/R0021 + no regression in PR12's 84 AllPass.

---

## 2. Parameters

`extract_trim_boundaries` is an internal pipeline function, not a user-facing API. Its inputs and
outputs constitute the contract between Stage 4 (face survival selection) and Stage 5 (B-Rep
assembly).

The signature (existing — not changed by PR13):
```rust
pub(crate) fn extract_trim_boundaries(
    subdivided: &SubdividedMesh,
    survival: &FaceSurvivalMap,
) -> TrimBoundaryMap
```

| Parameter | Meaning | Source | Valid range | Error condition |
|---|---|---|---|---|
| `subdivided: &SubdividedMesh` | Cherchi-arrangement output: vertex positions, sub-triangles per parent. | Output of `subdivide_mesh_pair` (`mesh_arrangement.rs`). | Non-empty `verts`; sub-triangles index valid vertex indices. | Verts empty OR triangle indices out of bounds → caller's responsibility. |
| `survival: &FaceSurvivalMap` | Per-`SourceFace` group of `SurvivingSubTri` after boolean cell selection. | Output of `face_survival_detect` (`topology_extract.rs:1231`). | Each group lists at least one `SurvivingSubTri` with `verts: [usize; 3]` and `flipped: bool`. | Empty groups → returned `TrimBoundaryMap` has empty boundaries (early-return at line 971). |

Output: `TrimBoundaryMap { boundaries: BTreeMap<SourceFace, Vec<TrimLoop>> }` where each
`TrimLoop` is a vec of `TrimEdge { v0, v1, is_intersection }` with v0/v1 as `subdivided.verts`
indices and `is_intersection` flagging edges shared with a different `SourceFace` group.

Output contract (PR13 strengthens this):
- For every `(face_A, face_B)` adjacent across a B-Rep edge, the directed edges on the shared
  portion of `face_A`'s trim loops must reciprocate `face_B`'s directed edges on the same
  portion: if face A emits `(p, q)`, face B must emit `(q, p)` byte-identically.
- This contract is the input precondition for `build_result_brep`'s twin pairing
  (`directed_he.get(&(cv0, cv1))` + `directed_he.get(&(cv1, cv0))` lookup at lines 283–284).
  When the contract is violated, both faces' lookups land in `directed_he` under the SAME key,
  the second overwrites the first, and `face_b`'s reciprocal lookup at `(cv1, cv0)` fails →
  half-edges go unpaired → `build_result_brep` returns empty topology (line 320–325) → caller
  falls back / fails Stage 6.

---

## 3. Branch Table

The four candidate fix approaches, each a hypothesis for what the correct construction-time
algorithm should look like at branch points. T2's empirical diagnostic will collapse this table
to the dominant approach.

**Background on the current algorithm** (line 1095–1224 of `topology_extract.rs`):

For each `SourceFace` group (i.e., one face of one operand surviving the boolean), the existing
code:
1. Aggregates surviving sub-triangles into `directed_edges` respecting per-triangle winding
   (with a sign flip when `tri.flipped`).
2. Computes `interior` set: undirected edges appearing ≥2 times within this face's group (these
   are interior to the face and must be discarded from the boundary).
3. Builds a face-local 2D frame `(face_u, face_v)` with `face_u × face_v = face_normal`, where
   `face_normal` is the area-weighted accumulated normal of the face's sub-triangles.
4. Builds adjacency `adj: BTreeMap<v0, Vec<(v1, is_int)>>` over the boundary directed edges.
5. Chains the loops: at each step, pick the next outgoing edge by **CW-angular-minimum sort
   from the reverse incoming direction in the face-local 2D frame** (lines 1143–1194).

**The defect**: step 5 uses **face-local** geometry. Adjacent faces sharing a B-Rep edge have
DIFFERENT local frames. If both faces walk the shared edge starting from the same branch point
with their own face-local CW-angular sort, both can independently pick the same outgoing
direction — producing `un_a[i] == un_b[i]`, the PR12 archaeological anchor.

| # | Approach | Trigger condition for adoption | Action expected | Current observed action | Fix surface | Blast radius | Yang-faithful? |
|---|---|---|---|---|---|---|---|
| **A** | **Edge-canonical reference** | T2 shows the issue is consistently at well-conditioned branch points (face normals well-separated, no degenerate sub-triangles). The face-local CW sort is geometrically valid but globally inconsistent. | At a branch point lying on a shared B-Rep edge, sort outgoing candidates using a **canonical reference direction** that both adjacent faces would compute identically — e.g., the edge's canonical direction (lower-id vertex → higher-id vertex), or the cross product of the shared edge tangent with a globally-consistent up-vector. Both faces use the same tie-breaker; reciprocal direction emerges naturally because each face's normal is opposite the other's at the edge. | Each face uses its own `face_u, face_v` frame (lines 1077–1100). At branch points, the face-local CW-minimum is computed independently per face. Because face A and face B have different normals (and thus different `face_v`), their CW-minima can pick the SAME directed edge along a shared boundary. | `topology_extract.rs:extract_trim_boundaries` chaining inner block, lines 1143–1194 (the `cw_angle` closure + `min_by` selection). Replace face-local `face_u, face_v` reference with edge-canonical reference at branch points where the incoming edge lies on a shared B-Rep edge. | Medium — touches the loop-chaining hot path; risk of breaking single-face cases where the face-local frame was correct. Mitigation: only invoke edge-canonical reference WHEN the branch point lies on a shared B-Rep edge (i.e., `is_intersection == true` for the incoming edge). Other branch points keep face-local sort. | YES — construction-time, by-design reciprocity. |
| **B** | **Global manifold invariant (post-hoc cross-check)** | T2 shows: branch points are well-conditioned + face-local picks are individually correct + global consistency requires post-hoc repair. (Or: A and C are intractable for some structural reason.) | After `extract_trim_boundaries` produces all loops, scan adjacent face pairs: for each shared B-Rep edge, verify directed edges reciprocate. If face A and B emit the same direction `(p, q)`, flip face B's loop on that segment. | Loops are emitted as final per-face output; no cross-check happens. | New post-processing pass at end of `extract_trim_boundaries`. ~100–200 LOC. | Low — additive, no change to existing logic. | **NO** — anti-Yang. Yang §4.1.1 framing is "by construction": discretizations are bijective because both faces resample the SAME boundary curve and run CDT independently (§4.1.2). Post-hoc flip is a band-aid on a chaining algorithm that is generating wrong output. Adopt only as last resort and document deviation explicitly. |
| **C** | **Twin-edge lookup at chaining** | T2 shows: chaining can correctly pick reciprocal direction if it has access to what the adjacent face has already chosen, but face-local geometry alone is insufficient. | Process faces in a deterministic order (e.g., by `SourceFace` ordering). For each face after the first, when chaining at a branch point on a shared B-Rep edge, look up which directed edge the previously-processed adjacent face emitted; pick the reciprocal direction in this face's loop. | Faces processed independently; no cross-face lookup. | New cross-face state passed through chaining — likely a `BTreeMap<(undirected_edge_key, SourceFace), DirectedEdge>` populated as faces are processed. ~150–300 LOC. | Medium — introduces processing-order dependency. Test that swapping operand A/B produces the SAME boundaries (modulo opposite orientation), not a different result. | Partially — construction-time, but order-dependent (Yang's algorithm processes faces independently per §4.1.2). Acceptable as a refinement layer, but flag the order dependency. |
| **D** | **Surgical bug fix** | T2 shows one specific local trigger (e.g., near-zero `face_normal.length()` after the `if len > TAU_WORK` guard at line 1069 succeeds with marginal length; sliver sub-triangles where the area-weighted accumulation cancels; phantom boundary edge from a degenerate sub-triangle that should have been pruned). | Targeted fix without algorithmic restructure: e.g., raise `TAU_WORK` floor for face-normal validity, prune sub-triangles whose area is below threshold from the directed-edge aggregation, or handle the specific tri.flipped sign-cancellation case. | The current code uses `[0.0, 0.0, 1.0]` as a fallback when `face_normal` is below `TAU_WORK` (line 1072), which destroys reciprocity if the actual face normal is nontrivial but happened to cancel. | Scope depends on T2 finding. 5–100 LOC at the specific local trigger site. | Low — minimal change, but only fixes the specific micro-trigger; does not address the algorithmic defect. | YES if the trigger is genuinely a degenerate edge-case (e.g., zero-area sub-triangles which Yang would have rejected from the discretization upstream). NO if the trigger is "this fixes the symptom but the fundamental face-local-vs-shared-boundary problem remains for other configurations." |

**Decision matrix for §8** (lead resolves after T2 lands):

- A dominant (≥7/9 violations diagnosed as well-conditioned face-local-sort divergence) → adopt A.
- D dominant (≥7/9 violations diagnosed as one specific local trigger like degenerate sub-triangle) → adopt D.
- A + D mixed (e.g., 5 well-conditioned, 4 degenerate) → adopt A as the structural fix; D handled as
  a guarded special-case at the same fix site.
- C dominant (only resolvable by cross-face state, A inadequate) → adopt C with explicit
  order-dependency documentation.
- B is the FALLBACK only if A, C, D are all empirically inadequate — flag deviation per
  `feedback_yang_only.md`.
- No dominant pattern (each violation needs different approach) → narrow to R0020 only (1 case)
  via D, defer R0021 to PR14, document the heterogeneity honestly.

---

## 4. Invariants

Per Yang §4.1.1 and Cherchi 2022 §3, the trim-loop chaining must satisfy four invariants. The
current code enforces I3 (face-local consistency: each face's loops form valid closed cycles) but
does NOT enforce I1 (cross-face reciprocity).

### I1 — Cross-face reciprocity at shared B-Rep edges (Yang §4.1.1, line 518–523)

> "Then a bijective mapping between each surface patch and its discretization is constructed.
> The generated triangle mesh is a closed, watertight manifold under a given surface-to-mesh
> distance tolerance dε from the original B-Rep model."

Operationally: for two adjacent faces `f_A, f_B ∈ arena.faces` sharing a B-Rep edge `e`, every
directed boundary edge `(p, q)` on face A's trim loop that lies on `e` must reciprocate as
`(q, p)` byte-identically (`p[k].to_bits() == q[k].to_bits()` for all k) on face B's trim loop.

**Measurable**: `check_face_pair_bijective` reports zero non-bijective pairs.

### I2 — Byte-identical positions (Yang §4.1.1)

The reciprocal edge match is byte-identical, not tolerance-based. Both faces draw their boundary
vertices from the SAME `subdivided.verts` array via the SAME vertex indices, so byte equality is
trivial AS LONG AS the trim-loop chaining selects the same vertex-index pair `(v_p, v_q)` on
both sides (oriented opposite). This is automatic from I1 once we walk the shared edge correctly.

### I3 — Per-face closed cycles (existing, not changed by PR13)

Each face's trim-loop emission produces edges that form one or more closed cycles (no dangling
endpoints, no self-crossings within the planar parameterization). The current code attempts
this via the CW-angular sort and dead-end detection (`eprintln!` at line 1125).

PR13 must NOT regress I3 while fixing I1.

### I4 — Twin pairing succeeds in `build_result_brep` (Cherchi 2022 §3 manifold precondition)

> "Input meshes are always assumed to unambiguously enclose a volume, that is, they are
> manifold, watertight and with no self-intersections."

After `extract_trim_boundaries`, `build_result_brep` (line 121) iterates the trim edges and
populates `directed_he: HashMap<(canon_v0, canon_v1), HalfEdgeIdx>` (line 192). For each
canonical undirected edge key `(min, max)`, both directed half-edges `(cv0, cv1)` and `(cv1, cv0)`
must be present in `directed_he` (lines 283–284). After PR13, this lookup succeeds for every
edge on every shared B-Rep boundary, and the unpaired-half-edge counter at lines 309–326
remains zero.

**Measurable**: `build_result_brep`'s post-condition `unpaired_count == 0` for the R0020/R0021
fixtures. PR12 verified `twin.twin == self` is correct WHEN pairing succeeds; PR13 must ensure
pairing actually succeeds (i.e., the lookup at line 283/284 finds both directions).

---

## 5. Oracles

### Primary — `BijectiveFacePairOracle` (existing, PR9)

Wrapper at `pipeline_oracles.rs:247-303`. Delegates to `check_face_pair_bijective`. Reports:
```
non-bijective face pairs: operand A {a_count} pair(s) of {a_total}, operand B {b_count} pair(s) of {b_total}
```
After PR13:
- R0020 should report `A 0 pair(s) of 19, B 0 pair(s) of 2` (currently `A 2 of 19, B 0 of 2`).
- R0021 should report `A 0 pair(s) of 23, B 0 pair(s) of 2` (currently `A 7 of 23, B 0 of 2`).

The oracle reads `face_boundary_directed_edges` per face, restricts to the shared portion via
`restrict_to_shared_boundary` (line 339), and runs `diff_boundaries` (line 288). Note that the
oracle currently runs against the **input** rendermesh (pre-boolean tessellation), NOT the
post-boolean output. PR13's fix in `extract_trim_boundaries` affects the post-boolean B-Rep
assembly. To validate the fix end-to-end, the test must check:
- Pre-boolean Stage 1 oracle (input bijectivity — independent of PR13, should remain whatever
  it was on R0020/R0021 going in).
- The downstream cascade resolution: after PR13, S2 + S6 should also flip to Ok (per Cluster X
  expectation from PR12 §8 amendments).

### Secondary — `LabelConsistencyWithinPatchOracle` (existing, PR9 / PR11)

Per-patch labeling consistency. Should still pass post-PR13 (PR11 fix is orthogonal). Adversary
spot-checks for regression.

### Tertiary — `TwinSymmetryOracle` (existing, PR9)

Validates `twin.twin == self` in the result B-Rep. Should pass post-PR13 (cascade resolution —
the twin pairing in `build_result_brep` will now find both halves of every directed edge).

### Quaternary — `MeshArrangementWellFormedOracle` (existing, PR9 + F1 anchor)

Should pass post-PR13 (cascade resolution; current S2 fire is downstream of S1 violation).

### Diagnostic format for T2

Per non-bijective pair, capture (T2 instruments at the chaining decision site):
```
case_id, face_a_source, face_b_source,
  branch_point_vertex_index, branch_point_position [3 f64],
  shared_brep_edge_canonical [endpoints],
  face_a_normal [3 f64], face_a_u [3 f64], face_a_v [3 f64],
  face_b_normal [3 f64], face_b_u [3 f64], face_b_v [3 f64],
  candidates_at_branch: list of (target_v, cw_angle_in_a_frame, cw_angle_in_b_frame, position),
  face_a_picked: target_v, face_b_picked: target_v,
  observed_emit: face_a "(p,q)", face_b "(p,q)"  // expected: "(q,p)" on B
  classification: { well-conditioned | near-degenerate | sliver | other }
```

T2 anchors instrumentation at `topology_extract.rs:1185–1194` (the `min_by` block) to dump these
values; reverts before commit.

---

## 6. Failure Modes

For each candidate approach, define detection mechanism, fix surface area, and blast radius.

| Approach | Detection mechanism (T2) | Fix surface area | Blast radius |
|---|---|---|---|
| **A — Edge-canonical reference** | T2 logs `face_a_normal · face_b_normal` for each violating branch point. If consistently `< -0.1` (well-separated, both faces "look outward"), face-local CW sorts are valid; need shared reference. | 50–150 LOC in `extract_trim_boundaries` chaining inner block. Replace face-local frame with a canonical 2D frame derived from the shared B-Rep edge's tangent direction at the branch point. The shared-edge detection requires `is_intersection` flag (already computed at line 1034) and the `global_edge_faces` map (already built at line 982–994). | Medium — affects every trim-loop emission. Mitigation: trigger edge-canonical sort ONLY at branch points where the incoming edge has `is_int == true` AND `outgoing.len() > 1` (i.e., genuinely on a shared boundary with a real branching choice). Single-face configurations and non-shared-edge branches keep face-local sort. |
| **B — Global manifold invariant** | T2 logs face-local CW-angle of the actually-emitted edge AND the cw-angle of the reciprocal partner that should have been emitted. If both faces' local sorts are individually correct (both pick the geometrically smallest CW angle in their frame), only post-hoc repair can flip one. | 100–200 LOC post-processing pass. Build per-shared-edge directed-edge map as faces are emitted; for each pair, compare A's vs B's directed walks; flip mismatched walks. | Low — additive only, but architecturally weakens the fix. Document deviation. |
| **C — Twin-edge lookup at chaining** | T2 logs whether processing face B AFTER face A could have used face A's already-emitted boundary as a reference. Likely yes if the configurations are local (single branch point per shared edge). | 150–300 LOC. Thread a `&mut BTreeMap<UndirectedKey, EmittedDirection>` through chaining; when face B reaches a branch point on a shared B-Rep edge, query the map for face A's already-emitted direction and pick the reciprocal. | Medium — adds processing-order dependency. Test order-invariance via fixture run with operand A/B swapped. |
| **D — Surgical bug fix** | T2 narrows the trigger: e.g., `face_normal.length() < 1e-9` (above `TAU_WORK = 1e-12` but below a stricter floor → fallback to `[0.0, 0.0, 1.0]` triggers spuriously); OR sub-triangle area < `TAU_AREA_FLOOR` (sliver triangles polluting `face_normal` accumulation). | 5–100 LOC. Either tighten the `TAU_WORK` guard at line 1069, prune sliver sub-triangles from `face_normal` accumulation, or handle a specific `tri.flipped` sign-cancellation case. | Low — minimal change. But: per `feedback_no_last_bug.md`, do NOT widen tolerance to mask a structural defect. Acceptable only if T2 confirms the trigger is a genuinely degenerate sub-triangle that the Cherchi-arrangement upstream should NOT have produced (in which case the fix is also documented as "this masks a Cherchi-arrangement defect that should be filed for PR14+"). |

**Decision matrix for §8** finalization:

- T2 evidence dominantly well-conditioned (face normals well-separated, no slivers) → A.
- T2 evidence dominantly degenerate (slivers, near-zero face normals) → D + file Cherchi defect.
- T2 evidence shows order-dependence is acceptable + simpler than A → C.
- T2 evidence shows construction-time fix infeasible → B with explicit deviation flag (lead and
  user must approve given `feedback_yang_only.md`).

---

## 7. Research Basis

### Yang 2025 §4.1.1 (verbatim, line 518–523 of `yang2025_hybrid_boolean.txt`)

> "Our method first discretizes each closed B-Rep model composed of multiple surface patches into
> a triangle mesh. Then a bijective mapping between each surface patch and its discretization is
> constructed. The generated triangle mesh is a closed, watertight manifold under a given
> surface-to-mesh distance tolerance dε from the original B-Rep model."

### Yang 2025 §4.1.2 (verbatim, line 592–602)

> "Discretizing each patch by sampling regularly in its u-v domain requires techniques such as
> integer optimization to ensure watertightness between patches. We find it unnecessary; instead,
> we discretize each surface patch independently without considering its neighbors, re-sample the
> boundary curves, and reconstruct the triangulation around the boundaries. For each surface, we
> first triangulate the rectangular u-v domain until reaching the given distance tolerance dε.
> Then, for each boundary curve, apply constrained Delaunay triangulation (CDT) in CGAL to
> retriangulate the two adjacent surfaces around the boundary, and remove the trimmed area as in
> Diazzi et al. 2023, if it's a trimmed surface, generating a watertight mesh."

**Implication for PR13**: Yang's bijectivity is constructed by **resampling shared boundary
curves once** and running CDT independently on both faces using that shared sample. The shared
sample makes byte-identical reciprocal directed edges automatic — both sides receive the SAME
list of boundary points, just walked in opposite orientations.

PR13 operates at a different layer: the post-Cherchi-arrangement trim-loop extraction. Cherchi
2020/2022 already produces a globally consistent subdivided mesh where the SAME mesh edge
indices `(v_a, v_b)` appear in both faces' sub-triangle sets. The PR12 archaeological evidence
(`un_a[i] == un_b[i]` byte-identical) confirms this: the vertex positions are byte-identical
because they're drawn from the SAME `subdivided.verts` array. The defect is purely in **which
vertex pair the chaining selects at branch points** — not in vertex positions themselves.

This makes PR13 a *chaining algorithm fix*, not a *resampling fix*. Yang §4.1.2's CDT-based
reconstruction is upstream architecture; PR13 fixes a downstream extraction bug.

### Cherchi 2022 §3 (precondition)

> "Input meshes are always assumed to unambiguously enclose a volume, that is, they are
> manifold, watertight and with no self-intersections."

The Cherchi 2022 mesh-arrangement (Yang Stage 2) requires this precondition; PR13 ensures it is
maintained through the trim-loop extraction (Stage 5 input).

### Cherchi 2022 §4–5 (manifold/non-manifold edge classification)

After mesh arrangement, edges are classified as manifold (exactly two incident faces with
opposite orientations) or non-manifold (≥3 incident faces). At a manifold edge between two
faces, walking the edge produces reciprocal directed edges. PR13's fix at branch points must
preserve this classification for shared B-Rep edges.

### PR12 archaeological anchor (binding)

From PR12 agent-impl's investigation (preserved in `specs/yang_stage1_bijective.md` §8
amendments, lines 401–419):

> "For both R0020 and R0021, every non-bijective pair has `un_a[i] == un_b[i]` byte-identically
> — both faces emit the SAME directed edge `(p, q)` in the SAME direction along the shared
> B-Rep edge. … Twin pairing in `build_result_brep` is verified correct (twin.twin == self), so
> the half-edges themselves are paired correctly. The TRIM_LOOP edge sequence chosen by
> `extract_trim_boundaries` is feeding `build_result_brep` half-edges in a direction
> inconsistent with the adjacent face."

This is the load-bearing evidence that pins the defect to chaining direction at branch points,
not to twin pairing or to vertex deduplication.

### Architectural caveat from `tessellate_solid_bounded:4307-4310`

Per PR12 §8 amendments: "removing degenerate earcut triangles is forbidden because the current
architecture relies on their edges for pairing — a naive fix at the tessellation layer breaks a
different invariant. PR13 should target the trim-loop chaining directly."

PR13 MUST NOT modify tessellation to filter degenerate triangles. The fix is constrained to
`extract_trim_boundaries` chaining logic.

### Deviations from published approach (to flag in code comments per FIP §5.2)

- Yang §4.1.2 prescribes per-surface CDT around shared boundary curves; the current Waffle
  implementation uses Cherchi 2020 mesh arrangement followed by `extract_trim_boundaries`. This
  is a known architectural deviation already documented in `specs/yang_hybrid_migration.md`.
- PR13's edge-canonical reference (Approach A, if adopted) is a Waffle-specific refinement that
  Yang does not explicitly describe (Yang's CDT-based shared-resampling makes branch-point
  chaining trivial). The deviation must be flagged in the implementation comment with a back-
  reference to PR13 spec.

---

## 8. Fix Scope (FINALIZED 2026-05-01 — Approach A on the CORRECTED anchor per T2 at `a26c913`)

T2's empirical instrumentation (`docs/audits/pr13_trim_loop_diagnostic.md`) revealed that
**the PR12 archaeological anchor named the wrong production function**. The fix surface is
NOT `extract_trim_boundaries` (test-only code, never invoked by production) but
`flood_fill_patches::Step 6` at `crates/kernel/src/boolean/topology_extract.rs:607-684`.

This invalidates §8a's anchor lines but the approach taxonomy still applies. Lead selects
**Approach A (edge-canonical reference)** translated to `flood_fill_patches::Step 6`.

### T2 empirical findings (binding)

- **9/9 violations** have `byte_eq=true / reciprocal=false` — both faces emit the SAME
  directed edge `(p,q)` along the shared B-Rep edge (PR12 anchor confirmed at higher
  abstraction level).
- **Two clusters with shared root cause** (Step 6 has no inter-face direction consistency):
  - **D1** (5/9, R0021 dominant): duplicate boundary edges + naive `outgoing.pop()` LIFO
    pick at `n_cands=3` branch points. cand[0] is geometrically correct; cand[2] (a
    duplicate of cand[1] targeting same canonical vertex) is what LIFO picks.
  - **D2** (4/9, R0020 + R0021 #0,#1): no branch points fire in Step 6; the bug is in the
    START-vertex pick via `adj.iter().find(...)` on a non-deterministic HashMap. Adjacent
    faces' loops walk shared edge in same direction because their independent start picks
    are uncorrelated.
- **PR12 missed `flood_fill_patches`**: `boolean/topology_extract.rs:644` HashMap is a
  confirmed PR12-residual non-determinism source, contributing to R0021's count flap (5/6/7
  NB pairs across runs).

### Approach A — edge-canonical reference (selected)

Rationale: addresses both D1 and D2 with one structural change. D1's "duplicate edges" and
D2's "independent start picks" are surface-level effects of the same architectural defect
— Step 6 makes locally-arbitrary choices that aggregate to globally-inconsistent boundary
directions. A canonical-edge reference forces both adjacent faces to walk the shared B-Rep
edge consistently with the canonical (lo→hi) vertex ordering; reciprocal emissions emerge
by construction.

**Why not B**: post-hoc cross-check is anti-Yang per `feedback_yang_only.md`.
**Why not C**: `arena.half_edges` don't exist until Step 7 (line 686+); twin-edge lookup
during Step 6 requires either a two-pass refactor or a parallel twin-tracking structure —
larger refactor than warranted.
**Why not D**: D1 and D2 are surface-level symptoms; a surgical fix would suppress one
without addressing the architecture defect.

### Concrete fix work for T4

In `crates/kernel/src/boolean/topology_extract.rs::flood_fill_patches::Step 6` (lines
607-684):

1. **Replace HashMap with deterministic adjacency** (line 644): `HashMap<usize, Vec<...>>`
   → `BTreeMap<usize, Vec<...>>`. This alone fixes D2's non-determinism; without it,
   start-vertex picks vary across runs. (Residual PR12 work — was missed in `7e119cc`.)

2. **Build canonical-edge reference map after Step 5a**: after patches are constructed but
   before chaining begins, build
   `(v0_canon, v1_canon) → (patch_id, direction_flag)` for every directed boundary edge.
   The canonical edge direction is `(min(v0, v1), max(v0, v1))`; both adjacent patches'
   chaining must respect it.

3. **Replace `outgoing.pop()` with canonical-direction-aware picker** (line 664): when
   chaining's current vertex has multiple outgoing candidates, pick the one whose
   `(current, target)` direction aligns with the patch's expected canonical orientation
   (derived from the patch's `source` SourceFace winding). This forces D1's cand[0]
   selection (geometrically correct) over LIFO cand[2].

4. **Replace `adj.iter().find(...)` with deterministic start picker** (line 651): pick the
   start vertex whose canonical edge ordering is "lowest valid start" — eliminates D2's
   uncoordinated start picks.

### Estimated LOC

50-200 LOC in `flood_fill_patches::Step 6`. Most is data-structure / API plumbing for the
canonical-edge map; the actual decision logic in steps 3+4 is small.

### Architectural caveats

Per `tessellate_solid_bounded:4307-4310`: do NOT filter degenerate earcut triangles in the
tessellation layer (load-bearing for cross-face edge pairing). The fix MUST be at
`flood_fill_patches` Step 6 layer — agreed; this fix doesn't touch tessellation.

The PR12 §8 amendment's `extract_trim_boundaries:1095-1200` LOC anchor is **stale**;
ignore it. Future PRs that touch trim-loop chaining should consult this spec's §8 anchor.

### 8a. Candidate fix surfaces (pre-resolved per branch — lead picks based on T2 dominant approach)

The following are concrete file:line anchors so lead can update §8 quickly:

#### If T2 selects Approach A (edge-canonical reference)
- **Anchor**: `crates/kernel/src/boolean/topology_extract.rs::extract_trim_boundaries`, the inner
  branch-point block at lines 1137–1194 (the `else if let Some(prev) = prev_vertex { ... }`
  arm).
- **Approach**: when the incoming directed edge `(prev, current)` lies on a shared B-Rep edge
  (i.e., `global_edge_faces[(prev.min(current), prev.max(current))]` contains a `SourceFace`
  other than this group's), compute the CW reference using a canonical direction derived from
  the shared edge:
  - The shared edge's canonical direction: from `verts[k_lo]` to `verts[k_hi]` where
    `k_lo, k_hi = (key.0, key.1)` from the `global_edge_faces` map.
  - Use this canonical direction as `face_u`-equivalent reference; derive `face_v`-equivalent
    by `face_normal × canonical_dir`. Both adjacent faces compute the SAME canonical
    direction; reciprocal walking emerges because `face_a_normal ≈ -face_b_normal` makes
    `face_a_v ≈ -face_b_v`, flipping the CW sort sense.
  - Apply edge-canonical sort ONLY when `is_int(incoming)` AND `outgoing.len() > 1` (real
    branch point on a shared boundary). Other branches keep face-local sort.
- **Estimated LOC**: 50–150 (new closure variant `cw_angle_canonical`; conditional dispatch in
  `min_by` block).
- **Validation**: T3's red Tests 3 + 4 (R0020 + R0021 corpus) flip green; PR12's red-phase
  `pr12_stage1_bijective.rs::r0020_cluster_x_non_coplanar_red_phase` /
  `r0021_cluster_x_non_coplanar_red_phase` also flip green (cascade resolution).

#### If T2 selects Approach B (post-hoc cross-check)
- **Anchor**: end of `extract_trim_boundaries`, after `boundaries.insert(...)` populates the
  full map. New post-processing pass before returning `TrimBoundaryMap`.
- **Approach**: build undirected-edge → list of `(SourceFace, DirectedEmit)` map across all
  loops. For each undirected edge with two distinct faces emitting in the SAME direction
  (mismatch), flip face B's directed edge in its loop. Update neighboring loop edges to
  preserve closed-cycle structure (this is the hard part — flipping one edge in a closed loop
  may require reordering).
- **Estimated LOC**: 100–200.
- **Validation**: same as Approach A; plus document the deviation per `feedback_yang_only.md`.

#### If T2 selects Approach C (twin-edge lookup)
- **Anchor**: `extract_trim_boundaries` outer loop over `survival.groups`. Add cross-face
  state.
- **Approach**:
  - Process `SourceFace` groups in deterministic order (already by `BTreeMap` iteration).
  - Build `previously_emitted: BTreeMap<UndirectedEdgeKey, (SourceFace, DirectedEdge)>` as
    each face's loops are completed.
  - In the chaining inner loop (line 1143–1194 area), when at a branch point on a shared
    boundary, look up `previously_emitted[(min, max)]`. If present and emitted by another
    face, pick the reciprocal direction.
- **Estimated LOC**: 150–300.
- **Validation**: red Tests + verify `swap(operand_a, operand_b)` produces structurally
  equivalent boundaries (modulo overall orientation flip).

#### If T2 selects Approach D (surgical fix)
- **Anchor**: depends on T2 finding. Likely candidates:
  - `face_normal` accumulation block, lines 1054–1074: tighten `TAU_WORK` guard at line 1069
    to a stricter `1e-9` floor; OR replace fallback `[0.0, 0.0, 1.0]` with an error return.
  - Sub-triangle area filter: prune sub-triangles with area below threshold from
    `face_normal` accumulation only (NOT from `directed_edges` collection — see architectural
    caveat).
  - `tri.flipped` sign handling: T2 may show that flipped sub-triangles' edge winding is
    being inverted incorrectly at line 1007–1013.
- **Estimated LOC**: 5–100, scoped to the specific local trigger.
- **Validation**: red Tests; plus T3 unit fixture intentionally re-creating the local trigger
  (small sliver / near-degenerate face normal) to lock the fix in.

#### Mixed (A + D)
If T2 shows e.g. 5/9 well-conditioned (A) and 4/9 sliver (D), implement A as the structural
fix and D as a guarded special-case at the same anchor site (lines 1137–1194). LOC estimate
60–250 combined. Lead documents both in §8 finalization.

### Out of scope (PR14+)

Explicitly deferred per `feedback_no_last_bug.md` honest framing:
- **Cluster Y entirely** (R0035, R0063, R0081, R0095, F0016, F0018, F0019, F0076): decoupled
  defect (S2=Ok despite S1 fire). Different root cause per PR12 T2 §8 question 1 — likely
  Cherchi vertex-merge hides the defect from Stage 2 measurement. Separate concern from
  trim-loop chaining.
- **Cluster X-coplanar** (R0007, R0031): require Stage 0 partial-overlap full implementation
  per Yang §4.5.5. The S0 OracleStub on these cases means the input mesh is already not in
  the form `extract_trim_boundaries` expects.
- **Cluster Z residual flap** (F0018, R0046, F0076 still flapping cluster verdicts post-PR12
  per `pr12_adversary_validation.md` §V4): residual non-determinism downstream of Stage 1 fix.
  Hunt for HashMap/HashSet on Stage 2 input path. PR13 may incidentally address some via
  cascade.
- **Other Stage 1 cases not yet identified**: PR12 T2 panel was 15 cases. Additional cases
  may exist in the rest of the corpus that PR12 oracle did not flag because they failed Stage
  0 or earlier. Out of scope for PR13.

### Success criterion

PR12 baseline: S1 = 13–15 first-fails (binary stable per V4 finding).

PR13 success target:
- **Primary**: R0020 + R0021 both report Stage 1 oracle = `Ok` post-fix (T3 Tests 3 + 4 pass;
  PR12's red-phase tests flip green). S1 first-fails drop to 11–13.
- **Cascade**: ideally R0020 + R0021 reach AllPass (PR12 T2 §8 amendments classify them as
  Cluster X firing S1+S2+S6; trim-loop chaining fix should resolve all three stages
  simultaneously since S2/S6 are downstream of S1 in this case). AllPass increases by 2.
- **No regression**: PR12 baseline 84 AllPass preserved; PR12's 12/15 binary cluster stability
  preserved or improved.
- **Determinism**: T3 Test 6 (re-run 3 times, verdicts identical) passes — no new
  HashMap/HashSet introduced by the fix.

Stretch: cascade may also resolve R0007 (Cluster X with S0 stub) — adversary V5 to report.

---

## 9. Honest framing & uncertainties

Per `feedback_no_last_bug.md`, `feedback_no_regression_chasing.md`,
`feedback_validate_against_corpus.md`, `feedback_anchor_before_fix.md`:

1. **The 9 violations may NOT share one micro-trigger.** R0020 has 2 violations and R0021 has
   7. T2 must classify each independently and report whether they collapse to one root cause
   or split. PR13 may unlock R0020 fully but only partially R0021 (or vice versa). Lead and
   adversary must report honestly.

2. **Even if the fix lands cleanly on R0020/R0021, the "trim-loop chaining is correct"
   claim is NOT supportable.** PR13's fix scope is "trim-loop chaining now produces
   reciprocal walks for the configurations R0020/R0021 exercise" — not "all configurations".
   Other Stage 1 cases (Cluster Y, X-coplanar, Z residual) are explicitly out of scope and
   their chaining may still be wrong for reasons PR13 does not address.

3. **Yang-faithful construction is preferred over post-hoc repair (Approach B).**
   `feedback_yang_only.md` explicitly demands no adapt-to-fit-legacy-code workarounds. Lead
   must adopt B only if T2 empirically demonstrates A, C, D are all inadequate. If B is
   adopted, code comment must explicitly flag deviation with a back-reference to this spec
   §3 (Approach B) and §7 (Cherchi 2022 §3 manifold precondition).

4. **Per `feedback_anchor_before_fix.md`**: T2 MUST add `[ANCHOR]` `eprintln!` at the planned
   fix site (line 1137–1194 for Approach A; whichever local site for D) and run R0020
   through the pipeline before T4 writes any production code. If the anchor function is not
   invoked on R0020, the diagnosis is wrong and PR13 must abort or amend §8. PR12 saw this
   exact pattern fail 5 times before empirical instrumentation caught it.

5. **Per `feedback_validate_against_corpus.md`**: T3 must validate against R0020 + R0021
   corpus fixtures (the actual `.waffle` files via the runner), NOT just synthetic
   two-rectangle unit fixtures. Unit-test-green is not GREEN. Synthetic fixtures are
   acceptable for the construction-time invariant baseline (Test 1) and the anti-fixture
   (Test 2), but the load-bearing tests are 3–6 against corpus.

6. **Per `feedback_no_regression_chasing.md`**: if the fix incidentally moves test counts
   (e.g., R0014 flap reappears or other Cluster Z cases destabilize), the response is NOT to
   revert PR13. The response is to investigate honestly: is PR13's fix correct and the new
   flap a separate defect? Or is PR13's fix introducing non-determinism? Adversary V5
   mutation testing should distinguish.

7. **PR10 baseline of 2 cases (R0031, R0081) was MEDIUM confidence** per
   `oracle_validity_audit.md` §3. PR13 success at "R0020 + R0021 unlock" does not prove the
   bijective oracle is fully validated — only that the empirical floor moves. Lead should
   not claim "Stage 1 validated"; lead claims "Cluster X non-coplanar resolved".

8. **Cascade resolution is a hypothesis, not a guarantee.** PR12 §8 amendments classify
   R0020/R0021 as firing S1+S2+S6 simultaneously and predict cascade resolution post-fix.
   This prediction is empirically falsifiable: if PR13 lands the S1 fix but S2 or S6 still
   fire, the cascade hypothesis is partially wrong, and a deeper defect lurks in S2/S6 that
   was masked by S1 first-fail. Adversary V5 must report cascade outcome honestly.

---

## 10. Out of scope (PR14+)

- Cluster Y root cause (8 cases — independent decoupled defect; likely Cherchi vertex-merge).
- Cluster X-coplanar (R0007, R0031 — require Stage 0 partial-overlap implementation).
- Cluster Z residual cluster-flap (F0018, R0046, F0076 — Stage 2 input path
  HashMap/HashSet).
- Mutation testing for the trim-loop chaining algorithm beyond V5 spot-check (would upgrade
  confidence from MEDIUM to HIGH per PR10 audit methodology).
- Stage 2 root cause for any case (~25 cases of degenerate sub-triangles in Cherchi
  arrangement output) — separate concern.
- Stage 6 root cause for cases where S1 is Ok but S6 still fires (~28 cases).
- Replacing `extract_trim_boundaries` entirely with a Yang §4.1.2-faithful CDT-based
  shared-resampling pipeline (a much larger architectural rewrite).

---

## 11. Governance compliance

- **FIP §1 P5**: 6 distinct agents (manager + spec + diagnose + test + impl + adversary).
- **FIP §2 red-before-green**: T3 red tests committed before T4 impl makes them green.
- **FIP §3 spec phase**: this file. §8 finalized post-T2.
- **P8 (cite research)**: Yang §4.1.1, §4.1.2 verbatim cited; Cherchi 2022 §3 cited; PR12
  archaeological anchor cited.
- **P9 (no hack-to-green)**: any tolerance widening, special-case branch, or post-hoc flip
  in §8a candidate fixes that does not address root cause is forbidden — fix the chaining
  algorithm correctly or abort. If Approach B is selected, document deviation explicitly in
  code comment with back-reference to §3 (and lead must approve given `feedback_yang_only.md`).
- **P10 (stay in slice)**: each agent stays in its stream. agent-impl operates only on
  `topology_extract.rs::extract_trim_boundaries` (and possibly `build_result_brep` if
  Approach B is adopted); agent-test operates only in `crates/test-harness/tests/`.
