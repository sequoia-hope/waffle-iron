# Roadmap to a faithful Yang 2025 implementation

**Status: STANDING PRIORITY from 2026-10-27** (user directive: *"build a
roadmap to faithfulness and let's make that the standing priority"*).
Precedence: governance A15.7 → **this roadmap (sequence and finish line)** →
`specs/yang_faithful_cleanup.md` (substitution table, method, knob policy)
→ the per-mechanism specs each milestone names. Paper cites are line numbers
in `refs/text/yang2025_hybrid_boolean.txt` (`./scripts/extract-papers.sh`).

## 0. Why this is the priority now

The corpus is at **360C / 0W / 9E / 5EE / 0T over 374** (2026-10-25). One
actionable ERROR is left (F0064). The other eight are loud by design or
adjudicated. More conversions can no longer measure progress. The code
still does a large share of the paper's work by substitute mechanisms:
Stage-1 arms that predict where the mesh will fail, Stage-4 passes that
repair what in-place relocation broke, a Stage-0 copy of Fig. 11, and an
order of operations the paper does not use. From here, progress means
**the debt goes down while the corpus holds**.

Working the tail is now SUBORDINATE to this roadmap (§4).

## 1. The finish line

The implementation is *faithful* when all four hold:

1. **Ledger:** the debt count at the foot of `docs/yang_deviations.md`'s
   status index (OPEN + DELEGATED + PROPOSED) is **0**. Every surviving
   divergence is PERMANENT, signed in prose with a rationale and a paper
   cite (A15.7 point 3), or HISTORICAL.
2. **Matrix:** every row of `docs/audits/yang_2025_audit.md` is FAITHFUL,
   N/A by a signed scope decision, SILENT with a recorded choice, or
   DEVIATES/PARTIAL with a PERMANENT ledger id. No row is MISSING or ORDER.
3. **Substitutes:** every entry in cleanup §2 (2a Stage-1 arms, 2b repair
   passes, 2c per-surface Stage 1, 2d matrix-found substitutes) is either
   RETIRED or re-graded "keep" because it *is* the paper's operation, with
   that row moved to FAITHFUL.
4. **Corpus held:** WRONG = 0, `SUPPORTED_CORRECT` not below the start
   value (360), and prospector per-seed ERROR rate not rising. Slower is
   acceptable (N67 amendment, "slow and correct first").

## 2. Scoreboard

Report these numbers at every milestone checkpoint, in the commit message
and in §5's log. The corpus score is reported, but it does not count as
progress.

| metric | 2026-10-27 baseline | how measured |
|---|---|---|
| ledger debt | **64** = 18 OPEN (N2, N62, N68, N74, N80–N93) + 44 DELEGATED + 2 PROPOSED (N64, N94) | `./scripts/faithfulness_score.sh` (`docs/yang_deviations.md` status index) |
| matrix rows not yet faithful | **100** of 194: 46 PARTIAL, 37 DEVIATES, 11 MISSING, 6 ORDER (67 FAITHFUL, 12 SILENT, 15 N/A) | `./scripts/faithfulness_score.sh --check` (`docs/audits/yang_2025_audit.md` row status column) |
| live substitutes | Stage-1: `phantom_min_rim_segments`, self-contact guard, `rim_junction_overrides` + P3a + tangent boost (N82), cross-patch folds `cyl_pair_phantom_n` / `face_rim_pair_phantom_n` / `CHART_REFINE_ROUNDS` / P0008 chord split (N88); Stage 0: `stage0/reloc.rs` (N86); Stage 4/5: `collapse_subtauwork_mesh_edges`, `repair_fan_cluster`, `remove_doubled_membranes`, the fold-merge / absorb / rehome family, `merge_same_plane_patches`, `solve_crease_transit`, `corner_transit_apply` (N87) | cleanup §2 |
| corpus | 360C / 0W / 9E / 5EE / 0T over 374 (2026-10-29: **357C / 3W** — the on-surface oracle exposed N100 item 3 in R0015, R0026, R0050) | canonical assay, release, 900 s; since 2026-10-29 "held" includes the on-surface oracle (remediation O1) |

## 3. Milestones, in order

Each milestone lists what it builds, what it retires, the ledger entries it
resolves or must re-adjudicate, and its gate. Every increment follows the
cleanup method: spec → gated-off primitive with red/green pins → one wired
increment → corpus run → flip → **retire the substitute in the same
series**. The ledger and the matrix rows move in the same commit (P2).
The "home" of each DELEGATED entry is where its mechanism is next
touched. That is when it gets re-adjudicated (A15.7): resolved, or argued
for and sent to the user in prose for a signature. An entry whose home turns
out to be wrong is moved, not skipped.

### F0 — Bookkeeping (no geometry change)

- [x] This roadmap, and CLAUDE.md priority #1 pointing at it.
- [x] Refresh the matrix's Summary table to the live row tallies (`./scripts/faithfulness_score.sh --check`, 2026-10-27).
- [ ] Fix the stale code and paper cites (matrix L-6, L-7) as each entry is
      touched. This is not a separate sweep.
- [x] A scoreboard script (`scripts/faithfulness_score.sh`) that prints §2's
      first two rows from the ledger and matrix, so checkpoints quote a
      computed number.

### F1 — Classify after the update (§4.4.2 order; N80, matrix O-1)

The deepest ORDER finding. The paper trims and updates BOTH meshes, then
classifies (`:572-578`). We classify the raw arrangement and update only
the kept submesh. Every §4.4.1 increment built before this is fixed lands
on the wrong substrate. Spec: `specs/yang_442_classify_after_update.md`.

- [x] **I1:** carry the per-triangle side labels through every Stage-4
      mutator (byte-identical; a debug assertion re-derives today's kept
      set). *(Landed 2026-10-27.)*
- [x] **Measure:** Stage 4 on the full arrangement costs 1.6–1.7× the
      triangles and up to ~2.7× on the O(T²) passes. Time it on the corpus
      before the flip (`YANG_STAGE_TIME_PROBE`, `ASSAY_JOBS=1`). *(Measured
      2026-10-27 across C0058, P0003, P0053, R0085, R0044, F0067 — largest
      cases R0044 430.91s at 1.018× and F0067 547.56s at 1.14× stay >340s
      below the 900s ceiling; see `specs/yang_442_classify_after_update.md` §5
      and N80.)*
- [ ] **I3:** Stage 4 on the full welded arrangement; keep, winding flip and
      membrane rule after it. The flood key becomes `(attribution, cherchi
      patch)`, curve incidence is deduped per operand, and a per-operand
      closed-manifold gate replaces the vacuous half-edge balance. This
      gives r_A = r_B = r on two trimmed meshes. *(I3a full-arrangement
      primitives + 9 unit pins landed 2026-10-27; I3b `boolean_once` flip
      stopped per P9/P10 on `R0063`, `tangency_pinch_split`, and `b2_pipe::s_bend`
      — gated on F2 retiring pre-CDT 2D `collapse_vertex` relabelling, post-hoc
      `(3d)` unzip, and Stage-5 index-parity decimation; see
      `specs/yang_442_classify_after_update.md` §3(d) and N80/N81.)*
- **Resolves:** N80. **Consolidates:** N81's three §4.4.1 sites (finished
  in F2). **Unblocks:** F2 and F3's Case-IV rule-out (I2).
- **Gate:** corpus held, and the kept set equals today's set except where
  matrix §3(a)–(c) say the order changes the answer, with each such case
  anchored.

### F2 — §4.4.1 as one per-patch update (N2, N81, N82, N86)

Merge-with-retriangulation replaces relocate-then-repair. Specs:
`specs/yang_441_phase2b_patch_update.md` (increments 2–3) and cleanup
Phase 2b/2c. Per-patch re-CDT has been live since 2026-10-16 on 85 % of
intersected patches.

- [ ] Every refusal class of `update_patch` is either built or named as a
      §4.5.2 site (F-2 did the crossing class).
- [ ] Retire the repairs it subsumes, one per corpus run, cheapest proof
      first: the §4.4.1(a) unzip, `repair_fan_cluster` (N58),
      fold-merge / absorb / rehome, the construct pass's seam replacement,
      `remove_doubled_membranes` (N41), `collapse_subtauwork_mesh_edges`
      (N89 item 1).
- [ ] The Fig. 11(b) merge carries the paper's threshold (N89 item 7; N55
      re-adjudicated), and Fig. 11(c) insert is real, not inherited (M-5).
- [ ] Split `stage4_relocate_and_correct_inner` along the paper's
      sub-steps: relocate → §4.3 dedup → §4.5.3 → `update_patch` →
      validate. The §4.3 removals move into optimization (N81).
- [ ] Re-site the Stage-5 `merge_conic_seam_runs` /
      `decimate_intersection_runs` into §4.4.1. Review
      `merge_same_plane_patches` (cleanup 2c).
- [ ] **Stage-1 junction mints retire** into §4.4.1 junction identification
      (N82: `rim_junction_overrides`, P3a, tangent boost).
- [ ] **Stage-0 Fig-11 relocation retires** into the same update (N86,
      `stage0/reloc.rs`); the §4.5.5 overlay itself stays.
- **Re-adjudicate on touch:** N47, N55, N56 (the weld band), N30, N31,
  N40, N41, N22.
- **Resolves:** N2, N81, N82, N86, N89 items 1 and 7.

### F3 — §4.5 failure handling complete (N62, N84, N68, N87, N90, N83)

The paper's answer to every failure is local refinement and a local
re-run. Specs: `specs/yang_452_cp_region.md` (L2, L3),
`specs/yang_442_classify_after_update.md` (I2), cleanup Phase 1 item 4.

- [ ] **L2:** collect every T1 failure instead of returning at the first
      fire (N84; about 50 `LocalRefinementRequired` returns).
- [ ] **L3:** recompute the intersections only in the refined region and
      splice the curve between p_f and p_b (`:659-671`; N62 locality).
- [ ] **I2:** Case-IV rule-out on the full arrangement: merge across the
      phantom, relabel by one new ray, re-keep, with the sag re-meshed by
      L3. Retires `phantom_min_rim_segments`. Customers: C0117, N68.
- [ ] Retire the crease and corner transits into the loop (N87): the paper
      sends boundary and corner points to §4.5.2 (`:635-657`, Fig. 13).
- [ ] Finish §4.5.1 for interior points (N90): truncation at C_b,
      continuation on S1 (pinned, unwired), and q1/q2. Every surface pair,
      not only cone+plane and torus.
- [ ] §4.5.4 detection on the updated mesh for every surface type (N83).
      After F2 the mesh census stops being a record-only check.
- **Re-adjudicate on touch:** N57, N74, N6 (removal half).
- **Resolves:** N62, N68, N83, N84, N87, N90.

### F4 — §4.3 optimization complete (N91, N92, M-8)

- [x] Tangency by Newton (`:489-492`), small-loop tracing (`:598-601`),
      Fig. 10(c) seeding, and contacts below the model band (N91), with
      the §4.2.2 Gauss map (N92) as the small-loop admission test. The
      cylinder×cylinder Stage-1 tangent mint retires. *(Landed 2026-10-27
      in F4 Steps 1–4; N92 RESOLVED; cross-referenced Stage-1 non-cylinder
      0D tangent mesh insertion and post-collapse single-survivor census
      remain under N82 / N81.)*
- [x] ~~§4.2.2 Gauss-map overlap filter (N92)~~ — FOLDED into N91
      (user, 2026-10-27): built and measured on the parked branch
      `agy/f4-n92-gaussmap` (corpus byte-identical; it only pre-empts the
      exact crossing test), so it lands with N91 as the small-loop
      admission test, not as a standalone pruner. *(RESOLVED 2026-10-27 in
      F4 Step 3 with zero trim-boundary exemption.)*
- [ ] §4.3.4 refinement for every curve, optimizing from the surfaces'
      parameter midpoints, with no 4096 cap (matrix M-8, N7 amendment).
- [ ] T6 detection floor from the paper's d_ε (N89 item 2).
- **Re-adjudicate on touch:** N11, N38, N39, N45, N46 (Stage-3 membership
  bands), N29 (exact conic parameters versus the tangent-angle proxy).

### F5 — Stage 1 as §4.1 on a generic surface (N5, N61, N85, N88, N93, D14)

One parameter-domain CDT per face at the Theorem-A chord bound. Cleanup
Phase 3 and 2c. This goes last among the mechanism milestones because the
cross-patch folds protect cases that F3's local loop must absorb first.

- [ ] d_ε from the B-Rep model's bounding box (`:391-395`, N85).
- [ ] d(T) computed and d_ε certified at Stage 1 (`d_of_t`, N93).
- [ ] **A generic parametric-surface interface first (user, 2026-10-27;
      resolves D14 together with the item below).** Stage 1 and every later
      stage reach a surface only through one interface — evaluate s(u, v),
      first and second derivatives, normal, parameter domain with its
      periodicity and seams, closest-point projection — which is all the
      paper's pipeline needs (§4.1 parameter-domain discretization with the
      Theorem-A bound `:304-395`, §4.2.2 Gauss map from normals, §4.3
      Newton on s(u, v)). The five analytic surfaces become its instances.
      No new code may match on the surface type inside a general geometric
      rule; the existing matches are inventoried and retired with the item
      below. Closed-form SSI (N7, A15.1) stays as the exact path where a
      solver exists, the paper's Newton refinement is the general path.
- [ ] One CDT per face, replacing the per-surface dispatch (tube, frustum,
      chart, universal cover, azimuth zipper), with cones included (N61),
      built on that interface.
- [ ] **A first non-analytic instance** (a ruled / B-spline surface, P&T
      Ch. 10) passes through the pipeline end to end — the proof the
      interface is general, and the substrate for loft booleans
      (`specs/loft.md`, option (b)). Resolves D14.
- [ ] Retire the cross-patch folds (N88) and the self-contact guard (its
      switches go with it, cleanup §4) into the certificate and the loop.
- **Re-adjudicate on touch:** N5, N9, N27, N28, N34, N35.
- **Resolves:** N5, N61, N85, N88, N93, D14.

### F6 — Stage 0 and output emission (§4.5.5, §4.4.2 restoration)

- [x] Stage-0 wall: the ellipse-bounded planar face (the paper's own Fig.
      24(b), N89 item 4 — resolved 2026-10-27).
- [x] Stage-0 wall: the coincident cylinder (N89 item 5 → **N103** PROPOSED 2026-10-27: audited against §4.5.5 in `specs/yang_455_cosurface_cylinder.md`; shared-frame axial extents and identical overlap meshes on both operands fixed and pinned in `f6_coincident_cylinder.rs`. Open: unify with planar Stage 0 — co-dispatch, pre-Stage-1 weld with the #178 refusal, a 2D (rθ, z) overlay).
- [x] Stage-0 coplanar scan bounds every curve by its own extent (N98 — RESOLVED 2026-10-27 via `geom::curve_aabb`).
- [ ] *(Low priority, user 2026-10-27.)* Unify the coincident-cylinder
      Stage 0 with planar Stage 0 (N103, OPEN — accepted in principle):
      co-dispatch, pre-Stage-1 weld with the #178 refusal (retire PR-6's
      post-arrangement weld and `zband` bucketing), and a 2D (rθ, z) overlay
      in place of the axial-containment special case.
- [ ] R0023's coplanar cap residue (M8 follow-up; Stage 0's n-ary refusal, N25 — not N17, corrected 2026-10-27; it now fails first as a kernel-v2 cap segment face with < 3 vertices).
- **Re-adjudicate on touch:** N25, N26, N44, N64 (PROPOSED), N10, N12,
  N32, N36, N42, N43.

### F7 — Adjudicate what remains

One prose sign-off request per entry, with a paper cite and one
recommendation, never a menu (A15.7). Expected: N13–N24 (corrections of
bugs in the Cherchi reference, parity-tested) signed PERMANENT, N33
(disjoint-sum passthrough, outside the paper's interacting-solid scope),
N7 already on the A15.1 basis. (D14 is no longer a scope signature: the
user reopened it 2026-10-27 and F5 resolves it; N94's p-curves are
re-adjudicated with F5's interface.) Entries still DELEGATED here are the ones F1–F6 did not touch.

### After the finish line — cost

Speedups come only after the finish line, each one a recorded deviation
measured against the faithful loop (N67 amendment). The §4.5.3 sweep's
whole-mesh `compute_phase_a` per removal (`yang_452_cp_region.md` §6) is
the first known item.

## 4. How the rest of the kernel work fits

- **A WRONG result always comes first.** A new silent-wrong is fixed or made
  loud (P10) immediately, in the paper's form.
- **New ERRORs (corpus, prospector, user files)** are anchored to their
  un-minimized lineage and attached to the milestone whose mechanism
  answers them, as that milestone's *customer*. A case-specific fix that no
  milestone names is not built (cleanup §3: "a substitute that would
  convert a case is not built").
- **F0064** and the loud-ceiling worklist are customers, not milestones.
  Record which milestone each one waits on in `docs/yang_tail_triage.md`.
- **Prospector seeds** keep running as the regression net. The
  "when the tail empties, run the next seed" loop no longer sets the work
  order.
- **Work order:** take the first unchecked item of the earliest open
  milestone. A milestone may start before the previous one closes only
  where this file says its items are independent. F0's items, F2's
  refusal-class item and F4 do not depend on F1.

## 5. Checkpoint log

| date | milestone step | debt | matrix not-faithful | corpus | commit |
|---|---|---|---|---|---|
| 2026-10-27 | roadmap adopted (baseline) | 64 | 100 | 360C / 0W / 9E / 5EE / 0T | — |
| 2026-10-27 | F0 scoreboard script; F1 I1 side labels through Stage 4 (byte-identical, wall 928.7 s) | 64 | 100 | 360C / 0W / 9E / 5EE / 0T | ed1c3658, 13f7fe05 |
| 2026-10-27 | F6 Stage-0 ellipse-bounded planar face (N89 item 4; 5.5-03 → FAITHFUL; Fig. 24(b) 4- and 24-cylinder unions closed solids, exact volume); N92 parked into N91 | 64 | 99 | 360C / 0W / 9E / 5EE / 0T | 8f538a57, 0943381b, d0bd7be4 |
| 2026-10-27 | D14 reopened by the user → OPEN, resolved by F5 (generic parametric-surface interface + first non-analytic instance); row 3-06 N/A → MISSING | 64 | 100 | 360C / 0W / 9E / 5EE / 0T | (this commit) |
| 2026-10-27 | Literature check: every matrix row re-read against the paper and the code, each change independently reviewed (not-faithful 100 → 139, no geometry changed); N13/N15/N17 RESOLVED; N89 item 3 REOPENED; N98, N99 (bugs) and N100 (eight possible silent paths, census needed) added | 64 | 139 | 360C / 0W / 9E / 5EE / 0T | (this commit) |
| 2026-10-27 | F5 Steps 1–2: Stage-1 d_ε census vs model AABB; d(T) computed and certified per triangle at Stage 1 (N93; 4.1-24 ORDER → FAITHFUL, 4.1-26 MISSING → PARTIAL). Step 3 (N85, d_ε from the model AABB) trialled and PARKED: it regresses P0046, R0032, P0007, F0084 until F2/F3/Stage-4 land | 64 | 138 | 360C / 0W / 9E / 5EE / 0T | 9919de2c, 9b76ad7d, 718d4f0c, 6240cf30 |
| 2026-10-27 | UL-02 / UL-06 silent-wrong FIXED in cherchi-rs ray cast (N101, N102 RESOLVED): the C++ eight one-ULP perturbations first, then coprime multi-ULP shells, then an exact full-disk certificate routes an unresolved crossing to the rational classifier; the generated ray now checks foreign-plane grazes | 64 | 138 | 360C / 0W / 9E / 5EE / 0T | 2cb63916 |
| 2026-10-27 | N99 FIXED: cherchi-rs projection-axis filter certified on edge differences with a doubled margin and subnormal guard, exact fallback otherwise; UL-04: every orient2d routes through the N24 exact-zero re-check | 63 | 138 | 360C / 0W / 9E / 5EE / 0T | 8b5072cf |
| 2026-10-27 | F6 coincident-cylinder Stage 0 audited against §4.5.5 and ledgered as N103 (PROPOSED, N89 item 5; spec `specs/yang_455_cosurface_cylinder.md`); three defects fixed and pinned — shared-frame axial extents, identical overlap meshes on both operands (revolved bores), exact flush-rim check | 64 | 138 | 360C / 0W / 9E / 5EE / 0T | (this commit) |
| 2026-10-27 | N98 RESOLVED: the Stage-0 coplanar scan bounds every loop edge by its own swept extent (`geom::curve_aabb`, all Curve variants), 4.5.5-01 → FAITHFUL; N89 item 3 re-resolved: §4.5.3 sweep admits cycles by incidence, open-conic loops now reversal-tested (sweep-level pins) | 63 | 137 | 360C / 0W / 9E / 5EE / 0T | 12bc1594,ea6ecf24 |
| 2026-10-27 | User sign-offs: N24 PERMANENT ("accept n24"); N103 accepted in principle, PROPOSED → OPEN with a low-priority F6 unification item | 62 | 137 | 360C / 0W / 9E / 5EE / 0T | (this commit) |
| 2026-10-27 | F1 Measure + I3a: full-arrangement Stage-4 primitives landed UNWIRED (side-label scoping, per-operand manifold gate, 4-owner seams; byte-identical production path). The I3b flip is blocked on two F2 items (the §4.4.1(a) unzip, N2; index-parity chain decimation, N81) — F2 agent at work. Corpus 360C held; F0067/F0072 confirmed CORRECT by 1800 s single_case, equal to main | 62 | 137 | 360C / 0W / 9E / 5EE / 0T | 20749329..HEAD |
| 2026-10-27 | F4 N91 + N92: §4.3 tangency Newton, Stage-1 cyl×cyl 0D mint retired, §4.2.2 Gauss-map small-loop admission (N92 RESOLVED) + Bajaj tracing + Fig. 10(c) seeding + curve redundancy | 61 | 132 | 360C / 0W / 9E / 5EE / 0T | agy/f4-n91-tangency |
| 2026-10-28 | F4: §4.3 tangency by Newton (pair optimization first, then compare normals), Stage-1 `cyl_cyl_tangent_points` retired (kept as a test oracle, 180-config sweep), §4.2.2 Gauss-map admission live for small loops (N92 RESOLVED), Bajaj loop tracing, Fig. 10(c) seeding, :502-505 redundancy; independent review fixes (no snap fallback, no type enumeration, failures recorded, C0118 sub-feature twin sheets declined). N100 item 9 found and ledgered (§4.5 adoption does not test loop vertices on surfaces) | 61 | 132 | 360C / 0W / 9E / 5EE / 0T (wall 636 s) | ..6714be2f |
| 2026-10-29 | Remediation Phase 2 (not a milestone step): the harness on-surface oracle (O1) — corpus "held" now includes geometry; N100 item 3 measured in R0015 / R0026 / R0050 (chords on torus/cylinder faces), now WRONG and first in line | 61 | 132 | 357C / 3W / 9E / 5EE / 0T | (this commit) |
