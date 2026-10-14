# Yang faithful cleanup — plan of record

**Status:** ACTIVE from 2026-10-14 (user directive: *"ad hoc patches will
destroy a kernel like this. the fixes must be principled. the principles are
the paper"*). Governs every change to `crates/yang-rs` and to the boolean
side of `crates/kernel-v2` until the DELEGATED set in
`docs/yang_deviations.md` is empty. Precedence: governance A15.7 → this
spec → the per-mechanism specs it names. Paper citations are line numbers
in `refs/text/yang2025_hybrid_boolean.txt` (`./scripts/extract-papers.sh`).

## 1. The shape of the debt

The 2026-10-14 audit (ledger directive, governance A15.7) found that the
kernel's accretion is not forty-five independent mistakes. Two paper
mechanisms are missing, and each has been compensated by a family of
substitutes that grew one corpus case at a time:

| missing mechanism | paper | what grew in its place | where |
|---|---|---|---|
| **§4.5.2 local refinement as THE response to mesh inadequacy** — "increase the mesh resolution of the parametric surfaces associated with the erroneous regions … the surfaces traversed by C_p and the neighbors of a ring of them … compute the intersections between the meshes only in the refined regions … repeated if optimization failure persists … guaranteed to terminate" (`:659-690`, `:646-652`) | §4.5, §4.5.2 | nine PRE-EMPTIVE Stage-1 sample arms that predict where the mesh will be inadequate and inject rim samples before the arrangement runs | `boolean/rim_junction.rs`, `RimAngleOverrides` (`boolean.rs`) |
| **§4.4.1 mesh updating** — "we trim and update the meshes using the intersection curves … set r_A = r_B = r … through CDT we obtain valid discretizations of the trimmed meshes … if an endpoint p of the split edge is too close to q, we merge p with q … if there are no other mesh vertices within an intersection loop, we insert one point … recalculate d(T)" (`:605-660`, Fig. 11) | §4.4.1 | eight POST-HOC repair passes that relocate first and mend the damage afterwards, plus a victim-RANKED §4.5.3 sweep and two Stage-5 seam passes | `stage4_correct.rs` (15.9k lines; `stage4_relocate_and_correct_inner` 5.5k), `stage4_correct/reversal.rs`, `stage5_seam_merge.rs`, `stage5_chain_decimate.rs` |

Measured 2026-10-14: 194 distinct environment knobs in yang-rs (78
behaviour switches, 116 debug probes); 45 DELEGATED ledger entries; the
Stage-1 arms and Stage-4 passes below.

## 2. Substitution table

Every arm and pass is listed with the paper mechanism it stands in for and
its fate. "Retire" means: the paper mechanism is built or generalized
first, the arm is switched off, the corpus is run, every case the arm
protected must be recovered by the paper mechanism (slower is acceptable),
and the arm's code and knob are deleted in the same series. An arm whose
cases the paper mechanism cannot recover names a gap in the paper
mechanism, never a reason to keep the arm.

### 2a. Stage-1 pre-emptive sample arms (`boolean/rim_junction.rs`)

| arm | entry point | knob | stands in for | fate |
|---|---|---|---|---|
| cylinder-pair graze (Case III) | `cyl_pair_graze_demand` | `YANG_433_GUARD`, `YANG_172_EXTENT` | §4.3.3 Case III/IV verdict (`:546-560`) is a STAGE-4 verdict on optimization failure; the paper then refines locally (§4.5.2). The arm pre-computes the demand at Stage 1. | retire into §4.5.2 (Phase 1) |
| cylinder-pair pinch (Case V) | `cyl_pair_pinch_demand` | same | same | retire (Phase 1) |
| rim × plane graze | `rim_plane_graze_n`, `rim_plane_graze_local_rim_overrides` | `YANG_195_LOCAL` | same | retire (Phase 1) |
| rim × plane lens (P0031) | `rim_plane_lens` | `YANG_452_LOCAL` | **this one IS §4.5.2's local form** (traversed faces + one ring, re-sample, re-run) | generalize into the mechanism (Phase 1 step 1) |
| edge graze | `edge_graze_sites`, `edge_graze_local_rim_overrides` | `YANG_433_GUARD` | §4.5.2 | retire (Phase 1) |
| segment × face graze | `segment_face_graze_demand` | same | §4.5.2 | retire (Phase 1) |
| segment × face pierce dip (P0057) | `segment_face_pierce_dip_demand` | same | §4.5.2 | retire (Phase 1, first) |
| wedge (P0044/54/58) | `segment_risk_intervals` + wedge demand | same | §4.5.2 | retire (Phase 1, first) |
| self-contact guard (P0037) | `stage1_tessellate/self_contact.rs` | `YANG_S1_SELF_CONTACT*` | §4.1 d_ε: a chart whose own chord bound is violated by self-contact is a discretization-accuracy question, Theorem A | fold into the §4.1 certificate (Phase 3) |

Not arms, kept: the §4.3.3 tangent-point mint (`YANG_433_TANGENT_INSERT`,
spec `yang_433_tangent_point_mesh_update.md`) is the literal "set r_A = r_B
= r" for a tangency, paper-faithful; Stage-0 §4.5.5 (plane weld,
edge-in-plane, overlay) is the paper's own pre-discretization step.

### 2b. Stage-4/5 post-hoc repair passes

| pass | file | stands in for | fate |
|---|---|---|---|
| `weld_coincident_relocated` (N47) | stage4_correct.rs:2797 | §4.3 "remove a point if it is too close to another point on the same loop" (`:535-536`) | keep as §4.3; the criterion gains carried-surface CONTAINMENT (Phase 2a) |
| `collapse_subresolution_intersection_segments` (N56) | :4634 | §4.3 same | same (already has the incidence-richer survivor rule) |
| `collapse_subtauwork_mesh_edges` | :4799 | §4.4.1(b) "merge p with q" | fold into the §4.4.1 merge-with-retriangulation (Phase 2b) |
| `repair_fan_cluster` (inc-4c, N58) | :3343 | §4.4.1 "the triangulation is updated accordingly" — the paper never produces the fold this repairs, because it retriangulates AS it merges | replace by merge-with-retriangulation (Phase 2b) |
| `merge_same_plane_patches` | :1329 | §4.4.2 patch segmentation + §4.5.5 | review in Phase 2c |
| `replan_degenerate_cylinder_patches`, `remesh_nonmanifold_patches`, `weld_f32_render_twins` | gated OFF, no production caller | banked experiments | **deleted Phase 0** (git history keeps them; Phase 2 builds from the paper) |
| §4.5.3 sweep victim RANKING (N29 §3b–§3e) | stage4_correct/reversal.rs | §4.5.3 "remove its next point and reconnect … repetitively … if no such next point can be found, remove p_r and restart from p_b" (`:727-732`) — an ORDERED removal, no ranking | replace the rank by the paper's order (Phase 2a) |
| `merge_conic_seam_runs`, `decimate_intersection_runs` (N58 addendum) | stage5_seam_merge.rs, stage5_chain_decimate.rs | §4.3.4 acceptance test applied at the output | keep (paper criterion); re-site into §4.4.1 when it exists (Phase 2c) |

### 2c. Stage 1 itself (N5, N61)

| dispatch | stands in for | fate |
|---|---|---|
| uniform cylinder tube, cone frustum, chart CDT, universal cover (P0038), azimuth zipper (P0044) | §4.1 "triangulate the rectangular u-v domain until reaching d_ε", §4.1.2 "discretize each surface patch independently … re-sample the boundary curves, and reconstruct the triangulation around the boundaries [by CDT]" (`:397-410`) | ONE parameter-domain CDT per face at the Theorem-A chord bound; the zipper is the first step (Phase 3) |

## 3. Phases and acceptance

Method for every increment (unchanged from the project's practice):
spec → gated-off primitive with red/green pins → one wired increment →
corpus run (release, 8 jobs, 900 s) → flip → **retire the substitute in
the same series**. The deviations ledger moves in the same commit (P2).

**Acceptance metric for the cleanup is REMOVAL with the corpus held**:
arms, passes, knobs and lines deleted; the prospector per-seed ERROR rate
(30 → 10.5 → 16 → 7 → 5 % for seeds 1–5) not rising; corpus
`SUPPORTED_CORRECT` not falling and `WRONG` at zero. A faithful mechanism
that converts no new case still ships (roadmap §0.1). A substitute that
"would convert a case" is not built.

### Phase 0 — bookkeeping (no geometry change)
- [x] Governance A15.7; ledger states; this spec.
- [x] Ledger catch-up for the 38 conversions of 2026-10-08 → 10-14 (ledger §"Catch-up").
- [x] Mechanical P2 check in `.githooks/pre-commit` (check 3).
- [x] Delete every gated-OFF dead branch and the primitives only they call. Done 2026-10-14: 11 gates deleted (`YANG_MESHUP_ENABLE`, `_RECDT`, `YANG_N2_RECDT_ENABLE`, `YANG_441_BOUNDARY_EXIT` / `_INPUT_REFINE` / `_CORNER_MERGE` / `_RIM_TRIM`, `YANG_S4_FIG11_MERGE`, `YANG_452_REQUIRE_CLEAN`, `YANG_S4_FOLD_RISK`) with `replan_degenerate_cylinder_patches`, `remesh_nonmanifold_patches`, `run_meshup_splice_passes`, `weld_f32_render_twins`, the `stage4_splice` loop, all of `stage4_update` but `Patch`, the I2c input-edge refinement, the N2-3a fold-risk PLANNER (the selector layer the live Fig-11 merge reads stays); 10 gates kept as print-only probes (`YANG_MESHUP_REGION`, `YANG_S423_INCIDENCE`, `YANG_433_PHANTOM`, `YANG_451_CHI`, `YANG_S4_RIM_SNAP_TARGET`, `YANG_441_CONSTRUCT` verbosity, `YANG_S451_ALL_MOVED`, `YANG_S451_JOINT`, `YANG_S5_CHAIN`, `YANG_S6_LOOP_SIMPLICITY`). Net −7,216 lines, 71 tests on deleted code removed, 0 added; yang-rs lib 1076/0, 75 binaries 0 failed; clippy clean with two pre-existing `dead_code` allowances gone. Corpus proof: see the triage ledger §"2026-10-14 (Phase 0)".
- [ ] Knob policy (§4) recorded; default-ON kill switches of mechanisms that STAY (Stage 0, §4.4.2 restoration, torus pairs, §4.5.1 transit, Stage-1 charts) deleted after one clean corpus run; kill switches of arms slated for retirement go WITH the arm.

### Phase 1 — §4.5.2 as the one mechanism (N62 → RESOLVED; arms retired)
- [x] **1c (2026-10-14):** `boolean::refine_loop` — the paper's one loop over T1/T2/T4/T5 + T3b; the two ladders, the lens, the rung tables, the op-wide rung and three knobs deleted (`specs/yang_452_refinement_loop.md` §6, §10 step 3). **N62 → RESOLVED.** Corpus 350C / 0W / 18E / 5EE / 0T + 1U over 374: P0046 / P0050 / P0052 / P0062 CONVERTED through the loop, zero regressions, wall 967.6 s (baseline 971.5 s). Three runs to land (STOP-path monitor; history monitor; ceiling corrected to density 8 = the old d_ε/64).
- [x] **1e (2026-10-14):** the §4.5.4 census on a shared AABB octree (`cherchi_rs::aabb_tree`; `TriOctree` a wrapper); measured affordable on every result (≤ 1.5 s on the largest op); **and measured that every improper pair on CORRECT outputs is a transversal chord crossing of the un-retriangulated Stage-4 mesh** ⇒ the mesh census is a record until Phase 2, and 1c's output-level trigger is T3b (two faces' exact boundary curves crossing on one surface).
- [x] **1b (2026-10-14):** the failure-site RECORD `boolean::failures` — T1/T2/T4 recorded where detected with their faces, T3 scanned on an emitted body, `failures()` / `region()` (faces + one ring by shared vertex); pinned in `tests_unit/s452_failure_sites.rs`; record-only, byte-identical.
- [x] **1a (2026-10-14):** the refinement UNIT — `BRep::face_refine` / `refined_at` (density ×2 per face, rims at `N·k`, bounds at `d_ε/k²`, standing across rebuilds, honored by Stage 0's ring readers), pinned in `tests_unit/s452_face_refine.rs`; byte-identical (no production reader until 1c).
1. Generalize the P0031 lens into the paper's loop: a Stage-4 failure certificate names the faces it traversed; those faces and their one-ring re-discretize at d_ε/2 (then /4 … to the ceiling, `:650-652`); the op re-runs. Spec: **`specs/yang_452_refinement_loop.md`** (one trigger set T1–T5 including §4.5.4 illegal intersections, per-FACE chord factors, regions = named faces + one ring, adoption = empty failure set).
2. Retire arms cheapest first — wedge, pierce dip, segment-face graze, edge graze, rim-plane graze, cylinder pinch, cylinder graze — one per corpus run. For each: arm OFF, corpus; every previously protected case recovers through the loop or the loop is fixed; then delete the arm, its knob, its pins' off-arms (pins keep their on-arm as a §4.5.2 pin).
3. Record the wall-time cost honestly: the loop re-runs the arrangement where an arm used to pre-empt it.

### Phase 2 — §4.4.1 mesh updating (N2 → RESOLVED)
- 2a. §4.5.3 by the paper's order (remove-next-and-reconnect, restart from p_b); every §4.3 collapse carries containment or incidence; N29/N47/N55/N56 amended, N29's victim ranking deleted.
- 2b. Merge-with-retriangulation: `collapse_vertex` + `repair_fan_cluster` become one §4.4.1 operation that splits/merges/inserts and re-CDTs the touched region in the parameter domain (`:620-640`, Fig. 11); `stage4_relocate_and_correct_inner` is split along the paper's sub-steps as the passes go.
- 2c. Re-site the Stage-5 seam/decimate passes into §4.4.1; review `merge_same_plane_patches`.

### Phase 3 — Stage 1 as §4.1 (N5, N61 → RESOLVED)
One parameter-domain CDT per face at the chord bound; the per-surface dispatch and the self-contact guard fold into the certificate.

### Phase 4 — re-adjudicate the remaining DELEGATED entries
One prose sign-off request each (A15.7 point 3). Expected: N13–N24 (Cherchi reference-bug corrections, parity-tested) signed PERMANENT; N7 already on the A15.1 basis; D14 a scope signature.

## 4. Knob policy

A knob is one of three things, and each has one rule:

1. **Debug probe** (`*_PROBE`, `*_DUMP`, `*_LOG`, `*_TRACE`, `*_CENSUS`, `cfg!(debug_assertions)`-only adversary knobs): prints or perturbs under a developer's hand, never changes a shipped result. Allowed; registered in `docs/kernel_probe_registry.md`.
2. **Kill switch of a landed mechanism** (`!matches!(env, "0"|"off")`, default ON): legitimate for ONE corpus run after landing. After that run it is deleted with its off-branch; a red/green pin that set it to "0" to show the old failure is rewritten to mutate the INPUT, or its off-arm is removed (the on-arm stays as the pin). A kill switch of an arm slated for retirement is deleted with the arm.
3. **Gated-OFF experiment** (`var_os(..).is_some()` enabling an extra pass): dead in production. Deleted in Phase 0; git history is the bank. A future primitive is developed on a branch or behind a pin-tested gate that is flipped or deleted within its own series.

## 5. What is not touched

cherchi-rs Stage 2 (parity-tested against the C++ reference), closed-form
SSI (A15.1), kernel-v2's B-Rep and Euler operators, the analytic-only
scope (D14), Stage-0 §4.5.5 and its n-ary extension (one honest customer,
R0023), the §4.3.3 tangent-point mint, the §4.5.1 corner transit and the
incidence-count triple gate, the §4.4.2 boundary restoration and its
§4.3.4 acceptance test.

## 6. Ledger

Every phase step amends the entries it touches and moves them OPEN →
RESOLVED or DELEGATED → RESOLVED/PERMANENT in the same commit. Progress is
reported as the debt count at the foot of the ledger's status index and the
removal numbers of §3, never as a corpus score alone.
