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

**Paper-first complement (2026-10-16):** `docs/audits/yang_2025_audit.md`
walks the paper in order. It found mechanisms this table does not list — the
Stage-0 Fig-11 relocation subsystem (U-2), the §4.5.1 transits on the
boundary class (U-3), Stage-1 junction mints (O-3), and the
classify-before-update order (O-1) — and three paths where a known failure
leaves without a STOP or a refinable site (F-1 to F-3). Its "What this
changes" section lists the proposed changes to the phases below; none is
adopted until this spec says so.

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
| ~~rim × plane graze~~ (+ its detect-then-refine retry) | ~~`rim_plane_graze_n`, `rim_plane_graze_local_rim_overrides`~~ | ~~`YANG_195_LOCAL`~~ | §4.5.2 | **RETIRED 2026-10-15** once every non-manifold STOP names faces (T4 Euler, T5 scope) and T3c reads every face's edges |
| Case-IV phantom boost (cross pairs) | `phantom_min_rim_segments` | — | §4.3 Case IV (`:546-560`): the optimization finds no point on both surfaces and the phantom is ruled out — a Stage-4 verdict the arm pre-computes at Stage 1 | **measured OFF 2026-10-15: 355C / 14E** — protects C0117 and F0088. F0088 recovers once Stage 3's solving failure names its faces; **C0117 cannot be recovered by refinement within the loop's ceiling** (the gap demands N = 315 on natural rims 10 / 8, ≈ 32–40×; the loop stops at density 8 and each doubling yields MORE phantom faces, 12 → 24 → 48 → 64). The paper does not refine Case IV away: its optimization RULES OUT the phantom points and its labels are per B-Rep patch, so a phantom mesh overlap never cuts a patch. Our exact arrangement cuts along the phantom before Stage 3 sees it, so the faithful replacement is §4.4.1-level (Phase 2): a mesh curve with no SSI solution is not a patch boundary. **Retire with Phase 2**; the intra-solid folds in `stage1_tessellate` are §4.1 d_ε and go with the self-contact guard (Phase 3) |
| rim × plane lens (P0031) | `rim_plane_lens` | `YANG_452_LOCAL` | **this one IS §4.5.2's local form** (traversed faces + one ring, re-sample, re-run) | generalize into the mechanism (Phase 1 step 1) |
| ~~edge graze~~ | ~~`edge_graze_sites`, `edge_graze_local_rim_overrides`~~ | ~~`YANG_433_GUARD`~~ | §4.5.2 | **RETIRED 2026-10-14** with T6 + T3c (step 2) |
| ~~segment × face graze~~ (its edge-cluster consumer) | `segment_face_graze_demand` (still read by the guards below) | ~~same~~ | §4.5.2 | **RETIRED 2026-10-14** as an arm |
| ~~segment × face pierce dip (P0057)~~ | ~~`segment_face_pierce_dip_demand`~~ | ~~same~~ | §4.5.2 | **RETIRED 2026-10-14** |
| ~~wedge (P0044/54/58)~~ | ~~`segment_risk_intervals` + wedge demand~~ | ~~same~~ | §4.5.2 | **RETIRED 2026-10-14** |
| self-contact guard (P0037) | `stage1_tessellate/self_contact.rs` | `YANG_S1_SELF_CONTACT*` | §4.1 d_ε: a chart whose own chord bound is violated by self-contact is a discretization-accuracy question, Theorem A | fold into the §4.1 certificate (Phase 3) |

Not arms, kept: the §4.3.3 tangent-point mint (`YANG_433_TANGENT_INSERT`,
spec `yang_433_tangent_point_mesh_update.md`) is the literal "set r_A = r_B
= r" for a tangency, paper-faithful; Stage-0 §4.5.5 (plane weld,
edge-in-plane, overlay) is the paper's own pre-discretization step.

### 2b. Stage-4/5 post-hoc repair passes

| pass | file | stands in for | fate |
|---|---|---|---|
| `weld_coincident_relocated` (N47) | stage4_correct.rs:2025 | §4.3 "remove a point if it is too close to another point on the same loop" (`:535-536`) | keep as §4.3; the criterion gains carried-surface CONTAINMENT (Phase 2a) |
| `collapse_subresolution_intersection_segments` (N56) | :3818 | §4.3 same | same (already has the incidence-richer survivor rule) |
| `collapse_subtauwork_mesh_edges` | :3983 | §4.4.1(b) "merge p with q" | fold into the §4.4.1 merge-with-retriangulation (Phase 2b) |
| `repair_fan_cluster` (inc-4c, N58) | :2609 | §4.4.1 "the triangulation is updated accordingly" — the paper never produces the fold this repairs, because it retriangulates AS it merges | replace by merge-with-retriangulation (Phase 2b) |
| `merge_same_plane_patches` | :1339 | §4.4.2 patch segmentation + §4.5.5 | review in Phase 2c |
| `replan_degenerate_cylinder_patches`, `remesh_nonmanifold_patches`, `weld_f32_render_twins` | gated OFF, no production caller | banked experiments | **deleted Phase 0** (git history keeps them; Phase 2 builds from the paper) |
| §4.5.3 sweep victim RANKING (N29 §3b–§3e) | stage4_correct/reversal.rs | §4.5.3 "remove its next point and reconnect … repetitively … if no such next point can be found, remove p_r and restart from p_b" (`:746-751`) — an ORDERED removal, no ranking | replace the rank by the paper's order (Phase 2a) |
| `merge_conic_seam_runs`, `decimate_intersection_runs` (N58 addendum) | stage5_seam_merge.rs, stage5_chain_decimate.rs | §4.3.4 acceptance test applied at the output | keep (paper criterion); re-site into §4.4.1 when it exists (Phase 2c) |

### 2c. Stage 1 itself (N5, N61)

| dispatch | stands in for | fate |
|---|---|---|
| uniform cylinder tube, cone frustum, chart CDT, universal cover (P0038), azimuth zipper (P0044) | §4.1 "triangulate the rectangular u-v domain until reaching d_ε", §4.1.2 "discretize each surface patch independently … re-sample the boundary curves, and reconstruct the triangulation around the boundaries [by CDT]" (`:397-410`) | ONE parameter-domain CDT per face at the Theorem-A chord bound; the zipper is the first step (Phase 3) |

### 2d. Substitutes the compliance matrix found (2026-10-17)

The paper-ordered matrix (`docs/audits/yang_2025_audit.md`) found four
substitutes this table did not list; ledgered as N82, N86, N87, N88. They
retire under the same rule as 2a–2c.

| substitute | entry point | stands in for | fate |
|---|---|---|---|
| Stage-1 exact junction mints (N82): rim-junction insertion, P3a conformal sampling, tangent boost | `boolean.rs` `boolean_once` (the "Increment 2" block, P3a #146, the boost), `boolean/rim_junction.rs` `rim_junction_overrides` | §4.4.1 junction identification AFTER optimization (`:546-571`) | retire into the §4.4.1 update (Phase 2) |
| Stage-0 Fig-11 mesh updating (N86) | `stage0/reloc.rs` (~2.4k production lines: cavity relocation, Lawson flips, Fig-11(a) split / slide, Fig-11(b) backtrack) | §4.3 + §4.4.1 applied to overlay mints before Stage 1 | retire into the one §4.4.1 update (Phase 2); the §4.5.5 overlay itself stays |
| crease and corner transits (N87) | `stage4_boundary_curve.rs` `solve_crease_transit`, `stage4_correct.rs` `corner_transit_apply` / `stage4_transit.rs` | §4.5.2 — the paper sends boundary and corner points there (`:635-657`, Fig. 13); §4.5.1 is for interior points | retire into the loop once its locality lands (Phase 1, N62 reopened); was listed under §5 "not touched" |
| Stage-1 cross-patch folds (N88): cylinder-pair phantom fold, thin-band rim fold, chart-crossing re-run, P0008 planar-neighbour chord split | `stage1_tessellate.rs` (`cyl_pair_phantom_n`, `face_rim_pair_phantom_n`, `CHART_REFINE_ROUNDS`), `planar_neighbor_chords.rs` | §4.1 "discretize each patch independently" (`:397-402`) + the d_ε certificate | fold into the §4.1 certificate (Phase 3) |

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
- [x] Knob policy (§4) recorded; default-ON kill switches of mechanisms that STAY (Stage 0, §4.4.2 restoration, torus pairs, §4.5.1 transit, Stage-1 charts) deleted after one clean corpus run; kill switches of arms slated for retirement go WITH the arm. **Done 2026-10-16** (yang-rs + kernel-v2's boolean side): **32 kill switches deleted with their off-branches** — `YANG_433_RULEOUT`, `YANG_433_TANGENT_INSERT`, `YANG_434_DECIMATE`, `YANG_434_INSERT`, `YANG_434_MERGE`, `YANG_441_CONE_CHART`, `YANG_441_TORUS_CHART`, `YANG_441_FAN_OF_ONE`, `YANG_441_FOLD_MERGE`, `YANG_441_MERGE_CARRIER`, `YANG_441_ONCURVE_MERGE`, `YANG_451_TRANSIT`, `YANG_451_TRANSIT_EMIT`, `YANG_EDGE_IN_PLANE`, `YANG_EDGE_PINCH_SPLIT`, `YANG_EDGE_PINCH_ENTRY_ONLY`, `YANG_JUNCTION_LINE`, `YANG_JUNCTION_SLAB`, `YANG_JUNCTION_SAMPLING_ENABLE` (with its `edge` / `face` half modes), `YANG_LINE_PAIR_CORNER`, `YANG_P3B_PIERCE_ENABLE`, `YANG_PLANE_WELD`, `YANG_S0_SUBRES_FUSE`, `YANG_S1_CHART_REFINE`, `YANG_S1_CHART_SEED`, `YANG_S1_COVER_UNROLL`, `YANG_S5_ENVELOPE_ENABLE`, `YANG_TORUS_PAIR`, `YANG_TORUS_PLANE_PAIR`, `YANG_SEAM_COMPONENT`, `KV2_ELLIPSE_SAG`, `KV2_PATCH_LIFT_REFINE`; **two gated-OFF experiments deleted with their code** — `YANG_S4_TRIPLE_POINT_ENABLE` (`plan_triple_point_reseats`) and `KV2_ARC_CONFORM_CURVES` (`boundary_curve_pool_samples`); the `0|off` value dropped from nine multi-valued knobs whose other modes stay (`YANG_434_OUT`, `YANG_441_RUN_ABSORB`, `YANG_441_GROUP_ABSORB`, `YANG_441_REHOME`, `YANG_451`, `YANG_451_TRIPLE_DOMAIN`, `YANG_451_TRANSIT_ANATOMY`, `YANG_S45_BOUNDARY_DOMAIN`, `YANG_S4_CARRIER_DOMAIN`); the I5 bisection caps `YANG_441_APPLY_BOOL_CAP` / `_SEAM_CAP` made debug-only (class 1). Pins: off-arms removed, on-arms kept (n77 seam anchoring, s434 typed-rim mint, the torus fan chart, the cone chartability pin, the s188 envelope fixture; the arc-conform test keeps its LOUD fold-wall half); the two Stage-1 test-only thread overrides (`CHART_SEED_OVERRIDE`, `COVER_UNROLL_OVERRIDE`) stay as the red halves of their pins, read by no env var. Net about −775 lines. **Updated 2026-10-24:** `YANG_441_OPEN_CONIC_PARAM` deleted with its off-branch (open-conic parameter reversal is always-on for both `Parabola` and `Hyperbola`), and the result-changing `census` / `stop` modes (`YANG_434_OUT=census`, the three I13 `census` modes, `YANG_451=census`, `YANG_451_TRIPLE_DOMAIN=1|on`, `YANG_S45_BOUNDARY_DOMAIN=stop|1|on`, `YANG_S4_CARRIER_DOMAIN=census`) are gated behind `cfg!(debug_assertions)`. **Still open under this item** (`docs/kernel_probe_registry.md`): the self-contact guard's switches (`YANG_S1_SELF_CONTACT`, `YANG_S1_SELF_CONTACT_CHART` — go with Phase 3).

### Phase 1 — §4.5.2 as the one mechanism (N62 → RESOLVED; arms retired)
- [x] **1c (2026-10-14):** `boolean::refine_loop` — the paper's one loop over T1/T2/T4/T5 + T3b; the two ladders, the lens, the rung tables, the op-wide rung and three knobs deleted (`specs/yang_452_refinement_loop.md` §6, §10 step 3). **N62 → RESOLVED.** Corpus 350C / 0W / 18E / 5EE / 0T + 1U over 374: P0046 / P0050 / P0052 / P0062 CONVERTED through the loop, zero regressions, wall 967.6 s (baseline 971.5 s). Three runs to land (STOP-path monitor; history monitor; ceiling corrected to density 8 = the old d_ε/64).
- [x] **1c follow-up (2026-10-25, user-signed "slow and correct"):** the signature-repeat monitor (both paths) is deleted and the ceiling is density 16 (ledger N67 amendment). P0056 / F0067 failure sets stay flat through ×8 and converge at ×16; a cheaper loop is a later recorded deviation.
- [x] **1e (2026-10-14):** the §4.5.4 census on a shared AABB octree (`cherchi_rs::aabb_tree`; `TriOctree` a wrapper); measured affordable on every result (≤ 1.5 s on the largest op); **and measured that every improper pair on CORRECT outputs is a transversal chord crossing of the un-retriangulated Stage-4 mesh** ⇒ the mesh census is a record until Phase 2, and 1c's output-level trigger is T3b (two faces' exact boundary curves crossing on one surface).
- [x] **1b (2026-10-14):** the failure-site RECORD `boolean::failures` — T1/T2/T4 recorded where detected with their faces, T3 scanned on an emitted body, `failures()` / `region()` (faces + one ring by shared vertex); pinned in `tests_unit/s452_failure_sites.rs`; record-only, byte-identical.
- [x] **1a (2026-10-14):** the refinement UNIT — `BRep::face_refine` / `refined_at` (density ×2 per face, rims at `N·k`, bounds at `d_ε/k²`, standing across rebuilds, honored by Stage 0's ring readers), pinned in `tests_unit/s452_face_refine.rs`; byte-identical (no production reader until 1c).
1. Generalize the P0031 lens into the paper's loop: a Stage-4 failure certificate names the faces it traversed; those faces and their one-ring re-discretize at d_ε/2 (then /4 … to the ceiling, `:667-668`); the op re-runs. Spec: **`specs/yang_452_refinement_loop.md`** (one trigger set T1–T5 including §4.5.4 illegal intersections, per-FACE chord factors, regions = named faces + one ring, adoption = empty failure set).
- [x] **1f (2026-10-14):** T6, the §4.3 near-miss check — `shell_contact::contact_pairs` (disjoint coarse pairs whose exact surfaces CROSS deeper than the model band at size-matched leaves; exact / tangent-plane `surface_deviation`) + `failures::joined_triangle_pairs` / `RingCapture` (the arrangement's intersection edges read back to their parent triangles; the one-ring capture runs first) + `failures::missed_intersection_sites`, recorded after Stage 2. **P0049 CONVERTED; R0023 UNSUPPORTED → ERROR (anchored, open); corpus 351C / 0W / 18E / 5EE / 0T over 374, wall 1035.9 s** (`docs/yang_tail_triage.md` §"2026-10-14 (Phase 1f)"). Kernel-v2's P0021 default-path pin asserts the fused body. **Arms-off run (same day): 349C / 0W / 20E — P0044 and P0047 still fail and T6 refines nothing on them:** their meshes met and the arrangement joined them; the defect is a MIS-joined intersection whose output kernel-v2 rejects while yang-rs's failure set is empty. Step 2 (arm retirement) now waits on a yang-side trigger for that invalid output region (anchors: P0044's cylinder patch with ≠ 1 material-CCW loop, P0047's CDT-rejected ring).
- [x] **1g (2026-10-14):** T3c, the output-level §4.5.4 check WITHIN one face (`failures::output_loop_crossings`: loops read on their exact curves in the face's chart — plane, unrolled cylinder — must not cross and must wind with the material side), and the rim × plane retry may no longer adopt a body with output-level sites. Corpus arms ON 351C / 0W / 18E / 5EE / 0T (wall 1040.5 s); **arms OFF the same 351C** — P0044 and P0047 convert through T3c + the loop. The §4.3.3 cluster (wedge, pierce dip, segment-face graze, edge graze) protects nothing on the corpus: step 2 starts.
- [x] **Step 2, first retirement (2026-10-14):** the §4.3.3 cluster DELETED with its kill switch (net −1,376 lines); corpus 351C / 0W / 18E / 5EE / 0T, identical to the arms-off measurement. Remaining arms: cylinder graze / pinch (`YANG_172_EXTENT`), rim × plane graze (`YANG_195_LOCAL`, now gated by T3b + T3c at adoption), self-contact guard (Phase 3). **Cylinder graze / pinch measured OFF the same day: 352C / 17E — protects P0034 only (its `SurfacePair` edge bridges two curve branches, a T3c named gap) and blocks C0118 and P0051; retirable once T3c reads `SurfacePair` edges** (`docs/yang_tail_triage.md` §"2026-10-14 (measurement)"). **T3c reads them since 2026-10-15: arm OFF 353C / 16E** (P0034 holds, C0118 and P0051 convert) — retirement next.
- [x] **Step 2, second retirement (2026-10-15):** the cylinder-pair graze / pinch arm and its N75 extent ladder DELETED (−647 lines + twenty unit tests), with one loop rule: a refined run's faceless STOP re-refines the same region (kernel-v2's P0034 pin). Corpus **353C / 0W / 16E / 5EE / 0T**, C0118 and P0051 CONVERTED. Remaining Stage-1 arms: rim × plane graze (`YANG_195_LOCAL`, its retry now gated by T3b + T3c), the Case-IV phantom boost (`phantom_min_rim_segments`), self-contact guard (Phase 3).
- [x] **Step 2, third retirement (2026-10-15):** the rim × plane graze arm, both forms, and its detect-then-refine retry in `boolean()` DELETED. Arm off it protected P0029 / P0017 / P0020 / R0032 — all four gaps in what the failure set NAMED, none in the loop: T4 now also names the Euler condition's over-covered edges, every Stage-5/6 `NonManifoldOutput` names its patch's face (T5), T3c reads every face's edges, and the pair / triple Newton floor reads the surface anchor (yang-rs + kernel-v2). Corpus **357C / 0W / 12E / 5EE / 0T over 374**; P0042 / P0043 / P0053 / P0055 CONVERTED. Remaining Stage-1 arms: the Case-IV phantom boost (cross pairs), self-contact guard (Phase 3).
1f. **T6, the §4.3 near-miss check** (`yang_452_refinement_loop.md` §3 "T6"): measured 2026-10-14 with the edge-graze cluster off, the loop recovers every protected case but P0044 and P0047, and on those the loop never ran — the class is a mesh that MISSES an intersection, which no trigger sees. The paper detects it after the arrangement ("if the two meshes do not intersect but are within d_ε … Newton", `:525-530`); T6 is that detection in closed form (`ssi_rs::intersect` on the two faces' surfaces, `shell_contact::triangle_distance` as the within-d_ε census), naming both faces for the loop. The arms are its Stage-1 prediction and retire after it.
2. Retire arms cheapest first — wedge, pierce dip, segment-face graze, edge graze, rim-plane graze, cylinder pinch, cylinder graze — one per corpus run, **after 1f**. For each: arm OFF, corpus; every previously protected case recovers through the loop or the loop is fixed; then delete the arm, its knob, its pins' off-arms (pins keep their on-arm as a §4.5.2 pin).
3. Record the wall-time cost honestly: the loop re-runs the arrangement where an arm used to pre-empt it.
- [x] **The loud ceiling (2026-10-16, user sign-off; matrix F-1):** the loop adopts only an EMPTY failure set; surviving failures STOP as `YangError::RefinementExhausted` (ledger N66 amendment). Corpus 358C → 350C, eight recorded failures that used to ship — the worklist for the items below.
4. **Reopened by the compliance matrix (2026-10-17).** N62 is OPEN again: (a) §4.5.2 locality — compute the intersections only in the refined regions and splice the curve between p_f and p_b, not a whole-op re-run (M-1); (b) the region is the surfaces traversed by C_p, not the faces at the STOP vertex (M-1); (c) record-and-continue at Stage 4's ~50 first-fire returns (N84, O-5); (d) a refused §4.4.1 patch records a site (matrix F-2) and a §4.5.3 failure names faces (F-3); (e) §4.5.1 for interior points (N90, M-2). (d) first — small, and each is a known failure leaving without a refinable site. **(d) LANDED 2026-10-17 (211f9b9a). (a)'s cost measured 2026-10-17 on R0044 — it was a quadratic cherchi-rs Stage 2 (ledger N95, RESOLVED, c9f014f8), not the recompute; (a) itself is deferred until a customer needs it (`specs/yang_452_cp_region.md` §5). (b) L1 LANDED 2026-10-17 for the collected T1 failures and every T2 fire** (`IsectGraph::cp_span`, spec `yang_452_cp_region.md` §3): active on four of the seven loud-ceiling worklist cases (R0070: 149 of 215 fires' regions grew, one run of 19 failing points 3 → 17 faces), corpus unchanged — the worklist's failures survive density 8 on the correct region, the M-2 / M-3 boundary-glide class. (c) is L2.

### Phase 2 — §4.4.1 mesh updating (N2 → RESOLVED)
- **Prerequisite found 2026-10-16 (N80, matrix O-1): classification runs before §4.3 and §4.4.1.** The paper trims and updates BOTH meshes and then classifies (`:574-578`); we keep the op's triangles at Stage 2 and update the kept submesh. 2b builds §4.4.1 on that submesh, so a design spec for the paper's order is due before 2b widens further — a pipeline restructure, not an increment. C0117's phantom (2a's Case-IV row), r_A = r_B = r on two meshes and the three §4.4.1 sites (N81) trace to it.
- 2a. §4.5.3 by the paper's order (remove-next-and-reconnect, restart from p_b); every §4.3 collapse carries containment or incidence; N29/N47/N55/N56 amended, N29's victim ranking deleted.
  - [x] **2a-i (2026-10-16): the §4.5.3 sweep takes the paper's ORDER.** `reversal_collapse_direction_ranked` and `YANG_453_RANK` deleted (with the `YANG_453_PAIR` / `_SPAIR` / `_OVERTAKE` kill switches): the sweep walks each curve C from an endpoint (`curve_endpoint` — curve / pair change, run end, ≥ 3 surfaces or a Stage-1 mint — the one definition that replaces the five ranking amendments §3–§3e), and `paper_order_removal` removes `p_n` once a later point of C resolves the reversal, else `p_r` (`:746-751`). Detectors and the 2·d_ε gate unchanged. Corpus 357C / 0W / 12E / 5EE, identical but R0044 (a loaded-host TIMEOUT; alone 515.5 s vs 510.2 s at HEAD once the per-vertex rank read is hoisted per pass). N29's victim rule RESOLVED.
  - [x] **2a-ii (2026-10-16): every §4.3 collapse carries CONTAINMENT.** N47's `weld_coincident_relocated` gains §4-I8 (`carried_surfaces` victim ⊆ survivor; the richer survives; neither ⇒ two distinct points, both kept) — **P0048 CONVERTED** (two triple junctions 4.96e-8 apart had been welded); N56 `subres` had it (KV15b I1c); N55's identity criterion satisfies it by construction. Measured together with the Phase-0 knob deletion: corpus **358C / 0W / 11E / 5EE / 0T**, P0048 the one move, attributed by `YANG_MOVED_WELD_PROBE` and a forced-off replay. Next: 2b, merge-with-retriangulation.
- 2b. **Design: `specs/yang_441_phase2b_patch_update.md`** (2026-10-16 — one per-patch `update_patch`, `rebuild_patch_planar` generalized and re-sited before the repairs; census first). Merge-with-retriangulation: `collapse_vertex` + `repair_fan_cluster` become one §4.4.1 operation that splits/merges/inserts and re-CDTs the touched region in the parameter domain (`:620-640`, Fig. 11); `stage4_relocate_and_correct_inner` is split along the paper's sub-steps as the passes go.
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
R0023) — its overlay, not the Fig-11 relocation subsystem in
`stage0/reloc.rs`, which is §2d — the §4.3.3 tangent-point mint, the
incidence-count triple gate, the §4.4.2 boundary restoration and its
§4.3.4 acceptance test. (Until 2026-10-17 this list also named the §4.5.1
corner transit; the compliance matrix found it is §4.5.1 applied to the
class the paper sends to §4.5.2 — N87, now in §2d.)

## 6. Ledger

Every phase step amends the entries it touches and moves them OPEN →
RESOLVED or DELEGATED → RESOLVED/PERMANENT in the same commit. Progress is
reported as the debt count at the foot of the ledger's status index and the
removal numbers of §3, never as a corpus score alone.
