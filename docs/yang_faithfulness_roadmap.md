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
4. **Corpus held:** WRONG = 0. `SUPPORTED_CORRECT` is the start value (360)
   less the *named dips*, and the prospector per-seed ERROR rate does not
   rise apart from them. A named dip is a case that goes from CORRECT to a
   typed ERROR because a substitute, or a mechanism that is not the paper's,
   was removed (owner ruling 2026-11-13: a small drop in the score is
   acceptable when what was removed was not faithful or not a good
   implementation). Each dip is named in the commit and in §5, against the
   milestone whose mechanism answers it. A dip is never a WRONG, and never
   the price of a change that is not a move toward the paper. At the finish
   line each named case is CORRECT again or adjudicated loud-by-design.
   Slower is acceptable (N67 amendment, "slow and correct first").

## 2. Scoreboard

Report these numbers at every milestone checkpoint, in the commit message
and in §5's log. The corpus score is reported, but it does not count as
progress.

| metric | 2026-10-27 baseline | how measured |
|---|---|---|
| ledger debt | **64** = 18 OPEN (N2, N62, N68, N74, N80–N93) + 44 DELEGATED + 2 PROPOSED (N64, N94) | `./scripts/faithfulness_score.sh` (`docs/yang_deviations.md` status index) |
| matrix rows not yet faithful | **100** of 194: 46 PARTIAL, 37 DEVIATES, 11 MISSING, 6 ORDER (67 FAITHFUL, 12 SILENT, 15 N/A) | `./scripts/faithfulness_score.sh --check` (`docs/audits/yang_2025_audit.md` row status column) |
| live substitutes | Stage-1: `phantom_min_rim_segments`, self-contact guard, `rim_junction_overrides` + P3a + tangent boost (N82), cross-patch folds `cyl_pair_phantom_n` / `face_rim_pair_phantom_n` / `CHART_REFINE_ROUNDS` / P0008 chord split (N88); Stage 0: `stage0/reloc.rs` (N86); Stage 4/5: `collapse_subtauwork_mesh_edges`, `repair_fan_cluster`, `remove_doubled_membranes`, `merge_same_plane_patches`, `solve_crease_transit`, `corner_transit_apply` (N87) | cleanup §2 |
| corpus | 360C / 0W / 9E / 5EE / 0T over 374 (2026-10-30, on-surface oracle in force; 2026-10-29: 357C / 3W — N100 item 3 in R0015, R0026, R0050, converted by Stage-5 emission typing, N60 amendment / N104) | canonical assay, release, 900 s; since 2026-10-29 "held" includes the on-surface oracle (remediation O1) |

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

**Order: F0 → F1 (I1, I3a) → F5a → F2 → F1's flip (I3b) → F3 → F4 → F5b →
F6 → F7.** The sections below are in that order. F5a comes before F2 because
§4.4.1 "can be totally operated in the parametric domain" (`:560-561`): the
per-patch update, the §4.3 optimization and the B-spline instance all need
the same substrate, and building F2 on charts derived from 3D positions
means building it twice.

**How the conversion is carried out: in place, in this order** (owner,
2026-11-16; reasons in `specs/yang_paper_path.md` §6). Each milestone lands
as wired increments on the production path. An increment holds the corpus
less its named dips (§1 item 4), runs the rigid-motion sweep of the cases its
mechanism touches first, and never admits a WRONG. The core changes tried
early each lost cases to a mechanism the paper puts in front of them (the
N85 d_ε scale and the I3b flip to F2/F3, the I2d retirement to F5b), so the
order above is what makes them land: F5a's substrate, then F2's update on
it, then I3b. The outer substitutes are retired in place on the same terms;
the rim boost, the cross-patch folds and the transits are gone, and
edge-in-plane arm 2 stays (it has a customer the corpus does not hold,
found by moving its anchoring case rigidly).

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
      — 2026-10-28/30 F2 Steps 1–2 cleared `b2_pipe::s_bend` (`17/17` in
      `0.37s`) and `R0063` (`SUPPORTED_CORRECT` in `4.33s`, advancing
      `smoke_corpus_boundary_categories` past `:904` to `F0060` at `:943`) under
      the reproducible uncommitted I3b trial; still gated on pre-CDT 2D
      `collapse_vertex` relabelling (`c0058`) and full-arrangement Steinmetz
      pinch (`F0060`, `N74`/`N78`); see
      `specs/yang_442_classify_after_update.md` §3(d) and N80/N81.)*
- **Resolves:** N80. **Consolidates:** N81's three §4.4.1 sites (finished
  in F2). **Unblocks:** F2 and F3's Case-IV rule-out (I2).
- **Gate:** corpus held, and the kept set equals today's set except where
  matrix §3(a)–(c) say the order changes the answer, with each such case
  anchored.

### F5a — The parameter-domain substrate (D14 first half; matrix 4.2-24, 4.3-05, 4.4-09)

The paper's hybrid representation lives in the parameter domain. Every mesh
vertex has (u, v) on its surface (`:371-374`), every intersection point has
(u, v) on both surfaces by barycentric coordinates (§4.2.3, `:510-519`), the
optimization moves (u, v, s, t) (§4.3, Appendix C), and the update
triangulates in the parametric domain (`:560-562`). The code holds 3D points
and tests them against implicit surfaces (`stage4_relocate.rs`
`relocate_onto_implicit_pair`); it forms no parameter coordinates, and its
patch charts cover planes, cylinders and cones only. That was recorded as
moot while surfaces were analytic-only (D14). With a B-spline instance in
kernel 1.0 it is the first thing to build: a B-spline has no implicit form,
and reaching it by closest-point projection from 3D is the slow, non-unique
route the paper never takes.

Every item here is result-neutral: the gate is the canonical geometry hash
(`WAFFLE_GEOMETRY_HASH_OUT`) identical on all 374 corpus cases.

- [x] **Design session** — the choice in "How the conversion is carried
      out" above (made 2026-11-16: in place), and the spec for the items
      below (`specs/yang_f5a_surface_interface.md`).
- [x] **The surface interface.** One trait every stage reaches a surface
      through: evaluate s(u, v), first and second derivatives, normal, the
      parameter domain with its periodicity and seams, and the two bounding
      operations the paper's guarantees rest on — the control-net distance
      of the sub-patch over a parameter rectangle (Theorem A, Fig. 6,
      `:358-368`) and the normal cone of a sub-patch (Theorem 4.1,
      `:453-460`). Both come from a rational Bézier net, so the interface
      exposes a surface as rational Bézier sub-patches; `stage4_dt.rs`
      already builds exact nets for the quadrics and the torus, and
      `nurbs-rs` gives them for a B-spline by knot insertion. Pointwise
      evaluation alone cannot give a conservative bound. Closest-point
      projection is not part of the interface's required path.
- [x] **The five analytic surfaces are its instances.** No new code matches
      on the surface type inside a general geometric rule. The existing
      matches (686 uses of a `Surface::` variant in 47 non-test files of
      yang-rs, 2026-11-12) are inventoried by rule, and each is either an
      instance method or a recorded closed-form fast path under N7.
      *(Inventory 2026-11-17, increment 4: `docs/audits/f5a_surface_matches.md`;
      47 class-(b) sites converted to `is_affine()`, `affine_plane()` and
      `SurfaceChart::u_period()`; 22 functions keep a class-(b) part, each
      named against F2, F3, F4, F5b or F6.)*
- [x] **Parameter coordinates carried.** Each mesh vertex holds (u, v) on
      every face it lies on, assigned at Stage 1, interpolated at Stage 2
      from the implicit points' barycentric coordinates (§4.2.3), and kept
      in lockstep through every Stage-4/5 mutator, as the per-triangle
      bound already is (N106 groundwork). Nothing reads them yet.
      *(Built 2026-11-17, increment 3: `param_surface/carried.rs`; the
      coordinates the barycentric step starts from come from the closed-form
      inverse, N109; 4.2-24 DEVIATES → PARTIAL.)*
- **Resolves:** matrix 4.2-24 (the barycentric map). **Unblocks:** F2's
  charts, §4.3 seeded from parameter coordinates (4.3-05), F5b.
- **Gate:** corpus byte-identical by canonical hash; the rewrite tier green.

### F2 — §4.4.1 as one per-patch update (N2, N81, N82, N86)

Merge-with-retriangulation replaces relocate-then-repair. Specs:
`specs/yang_441_phase2b_patch_update.md` (increments 2–3) and cleanup
Phase 2b/2c. Per-patch re-CDT has been live since 2026-10-16 on 85 % of
intersected patches.

- [ ] Every refusal class of `update_patch` is either built or named as a
      §4.5.2 site (F-2 did the crossing class). *(2026-10-31: the hole-first
      chart fixed; open — the self-touching split cycle (P0040) and the d(T)
      budget refusal, which is not the paper's operation at all — N106.
      Census 2026-11-21, every run of every corpus case, 16,368 patch
      updates: 14,675 rebuilt; 1,281 refused on the d(T) budget (170 cases,
      recorded nowhere); 349 as crossing cycles (37 cases, a recorded T3
      site); 25 boundary mismatches, 22 mixed attributions, 8 other CDT
      refusals, 7 plans keeping a moved degenerate triangle, 1 chart
      refusal. Where a d(T) refusal leaves in place a chain vertex that a
      relocated junction passed (P0027 moved rigidly), the update's chain
      reading takes it out by a local fan rebuild (N2).)*
- [ ] **The patch chart is the face's parameter domain** (F5a's interface),
      for every surface: whole-patch sphere and torus charts exist, and a
      cone apex is a point of the domain, not an `ApexInPatch` refusal
      (matrix 4.4-09). *(2026-11-18, spec `specs/yang_f2_patch_chart.md`
      increments 0–2: `stage4_patch_chart::domain_chart` charts any patch on
      its `ParamSurface` domain — disc, azimuth band, tube band, a cap round
      an apex or pole, a sphere zone — and the update takes every patch the
      `SurfaceChart` rebuild cannot; N122. Corpus held, 360C/0W/9E/5EE/0T;
      83 domain charts, 7 rebuilt (P0050, R0032, R0038), the rest refused by
      the d(T) budget (N106 → F5b) or named. An interior vertex relocated
      past the curve is removed, §4.4.1's near-curve removal (P0049's two
      cylinder bands now rebuild; corpus held). 2026-11-20, increment 3:
      the update rebuilds EVERY patch on the domain chart — plane, cylinder
      and cone too — with the seeded retry and the coincident-simplex
      re-plan on it; `SurfaceChart` is out of the update. Corpus held,
      360C/0W/9E/5EE/0T, no category move, every oracle detail identical;
      cone patches rebuilt 2,357 → 2,519, torus 7 → 20. The chain
      decimation's whole-patch fallback is on the domain chart too, and its
      `Surface::Sphere` skip is gone (corpus held). Open: the fan-local
      rebuilds (`SurfaceChart::new_local`, the decimation's primary path
      among them) and the construct pass, which go with their passes.)*
- **Moved to F5b:** retiring the d(T) refusal (N106). It waits on Stage-1
  interior sampling, which is F5b's one CDT per face; F2 closes without it.
- [ ] Retire the repairs it subsumes, one per corpus run, cheapest proof
      first: ~~the §4.4.1(a) unzip~~ *(retired 2026-10-30/31: the split runs
      inside `update_patches_ex` by patch CDT; the unzip's local arms run
      ONLY for the d(T)-budget refusal (N106 — R0059, P0005 need them), every
      other declined component STOPs loudly into §4.5.2; `4.4-12 →
      PARTIAL` — its trigger is still a collapsed triangle, N2)*, ~~`repair_fan_cluster`
      (N58)~~ *(retired 2026-11-24: the update charts a folded patch and,
      where the d(T) budget refuses the whole patch, replaces the lapping
      triangles alone by the CDT of their own boundary —
      `rebuild_patch_folds`, N2; with the pass off, all 1,870 rotated
      builds are line-identical)*,
      ~~the fold-merge corner merge~~ *(retired 2026-11-22, N2: the update
      reads the out-of-order boundary chain as Fig. 11(a) and the Stage-5
      pass then applies in none of 1,870 rotated builds)*, ~~absorb / rehome~~ *(retired 2026-11-21,
      N113: the three arms applied in six measured builds — R0003 unmoved and
      under four rigid motions, one seed-6 candidate — and each emits a
      bit-identical body without them)*, the construct pass's seam replacement,
      ~~`remove_doubled_membranes` (N41)~~ *(retired 2026-11-06: the N106
      inc-2 separation subsumes it, with 0 removals on the corpus, seed 6
      and the refusal-off trial; a residual fin is a loud gate failure and a
      T4 site)*, `collapse_subtauwork_mesh_edges` (N89 item 1) *(censused
      2026-11-25: NOT this milestone's to retire whole. Its edges are four
      populations — two intersection points an ulp apart (merged before
      the update since 2026-11-26; the pass's Stage-4 site now takes nothing
      on the corpus or the sweep), two rim points the boundary-curve relocation
      brings together after the update (R0015), a rim vertex beside a
      vertex inside a face (C0043, harmless), and two points of ONE
      operand's mesh (`pocket_operand_reenters_plain_boolean`, the class it
      was built for). The first two are the Fig. 11(b) item below and the
      Stage-4 split; the last is a Stage-1 defect the paper's update has no
      merge for, and goes with F5b. A merge with the paper's domain in
      place of the pass held the corpus, 1,864 of 1,870 sweep lines, seed 6
      and the always-on harness, and stopped in the rewrite tier on the
      operand-self twins; backed out.)* *(Census 2026-11-06, `YANG_REPAIR_CENSUS`, main corpus: the
      construct pass fires in 230 cases, fold-merge in 6, rehome in 1
      (R0003), `repair_fan_cluster` in 1 (P0020), the pinch split in 4,
      absorb in 0; on seed 6 absorb fires once. Census 2026-11-21 at
      `7caa1a89`, after the one-chart update: the construct pass in 89 cases,
      the corner merge in 4 (F0064, F0067, R0070, R0095), rehome in 1,
      `repair_fan_cluster` in 1, the pinch split in 4, absorb and the
      sub-τ collapse in 0. The same census over four rigid motions a case
      (1,496 builds) names the customers the corpus does not hold: the
      corner merge acts in P0027 and R0050 too, `repair_fan_cluster` in
      P0044, the pinch split in P0053, and the sub-τ collapse in five cases
      (C0043, F0055, F0059, P0041, R0015 — 10 builds), so "0 fires on the
      corpus" does not retire it. Each pass switched off on its own
      customers: the pinch split and `repair_fan_cluster` are load-bearing
      (F0058, F0060, P0057 and P0020 become typed ERRORs without them); the
      corner merge changes no unmoved body and one rotated one — P0027
      motion 0 builds a quietly different body without it (N2, pinned), so
      it stayed until a trigger read such a fold on the emitted body and
      the update put the chain in order in Stage 4. Both landed 2026-11-22.
      T3c reads a loop that folds back over itself along one curve
      (`output_loop_spurs`, N83); §4.5.2 refinement does not remove the
      fold. The merge's sites were two populations (N2): 87 of 113 sat on
      the intersection curve (the on-curve arm, §I13c) and were curve
      vertices §4.5.3 had left in place and recorded, in natural runs the
      loop discards; 26 sat on a crease of one operand. Each of the 26
      arrives at the update as a chain that walks out and back, and the
      update now splits the edge that contains the relocated point
      (Fig. 11(a), `seed_fig11a_chain_folds`). The Stage-5 pass and its
      selector are deleted.)*
- [ ] The Fig. 11(b) merge carries the paper's threshold (N89 item 7; N55
      re-adjudicated), and Fig. 11(c) insert is real, not inherited (M-5).
      *(Spec: `specs/yang_f2_fig11b_cdt_input_merge.md`. Threshold RULED by
      the owner 2026-11-25: "too close" is the same point within roundoff,
      `TAU_WORK·(1+scale)`; a quality threshold is a later, separately
      measured increment. The same-point classes are three paper statements
      — §4.3.3's same-loop removal in five merges of six, the figure's pair
      in 33 of 296 on the corpus, the near-curve removal — and one merge
      serves them all. **Increment 2a LANDED 2026-11-26:** the ordered N55
      scan is deleted; the classes are merged in its place, all at once, so
      the result has no order of pairs. F0060 is CORRECT with each pinch-line
      end one point per sheet (line pinch RULED 2026-11-26, gate 3.7); the
      sub-τ collapse no longer sees an intersection-point pair. Open:
      increment 2b — the classes as input of the update's CDT, which waits
      on N106's d(T) refusals; the survivor's face record; two points of two
      different curves at one position, which no mesh edge joins (F0059
      under one rotation, loud at the render gate — a tangent crossing,
      gate 3.7). F0060 is also the named blocker of F1's I3b.)*
- [ ] Split `stage4_relocate_and_correct_inner` along the paper's
      sub-steps: relocate → §4.3 dedup → §4.5.3 → `update_patch` →
      validate. The §4.3 removals move into optimization (N81). *(Measured
      2026-11-26, `specs/yang_f2_fig11b_cdt_input_merge.md` §8.5: the
      same-point pairs the relocation leaves are merged in three places —
      ahead of the §4.5.3 sweep, by the N55 scan and N47 weld behind it, by
      the sub-τ collapse after the update — and the sweep itself removes a
      third of the Fig. 11(b) pairs as reversals. Since increment 2a the
      place behind the sweep is one order-free merge and no pair reaches the
      update. First step here: form the classes once, after the relocation,
      so the sweep reads each class as one point.)*
- [ ] Re-site the Stage-5 `merge_conic_seam_runs` /
      ~~`decimate_intersection_runs`~~ into §4.4.1 *(2026-10-28/30:
      `stage5_chain_decimate` deleted and re-sited to
      `stage4_chain_decimate::decimate_patch_chains_441` with forward §4.3.4
      endpoint subdivision, operand-anchored closed loops, and `N105`
      class-(b) elision (the `Surface::Sphere` skip retired 2026-11-20);
      `merge_conic_seam_runs` open + operand-anchored closed runs made
      vertex-index-free, while `0`-operand-vertex closed circles carry a
      retained index rule, open debt — `R0021` passes by noise at
      `rel = 8.489e-5` vs `band = 8.55e-5`, `volume_composition` chord-sagitta
      deficit needs adjudicating — see `N81` / `N58` / `N105`)*. Review
      `merge_same_plane_patches` (cleanup 2c).
- [ ] **Stage-1 junction mints retire** into §4.4.1 junction identification
      (N82: `rim_junction_overrides`, P3a, tangent boost).
- [ ] **Stage-0 Fig-11 relocation retires** into the same update (N86,
      `stage0/reloc.rs`); the §4.5.5 overlay itself stays.
- **Re-adjudicate on touch:** N47, N55, N56 (the weld band), N30, N31,
  N40, N41, N22.
- **Resolves:** N2, N81, N82, N86, N89 item 7 (item 1 retires with F5b).

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
      L3. Customers: N68, and C0117, which today adopts only at the
      loop's ceiling (the Stage-1 rim boost that pre-empted it is deleted)
      and, moved rigidly, stops at that ceiling in 4 of 4 motions: a named
      dip of the boost's removal, outside the corpus.
- [x] Retire the crease and corner transits into the loop (N87): the paper
      sends boundary and corner points to §4.5.2 (`:635-657`, Fig. 13).
      Deleted 2026-11-13; the detection stays and the loop answers every
      case they touched.
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

### F5b — Stage 1 as §4.1 on the interface (N5, N61, N85, N88, N93, N106, D14)

One parameter-domain CDT per face at the Theorem-A chord bound, built on
F5a's interface. Cleanup Phase 3 and 2c. This goes last among the mechanism
milestones because the cross-patch folds protect cases that F3's local loop
must absorb first.

- [ ] d_ε from the B-Rep model's bounding box (`:391-395`, N85).
- [ ] d(T) computed and d_ε certified at Stage 1 (`d_of_t`, N93).
- [ ] One CDT per face with interior sampling, replacing the per-surface
      dispatch (tube, frustum, chart, universal cover, azimuth zipper), with
      cones included (N61), built on F5a's interface.
- [ ] **The d(T) refusal becomes the paper's per-triangle bound (N106):**
      accept the patch CDT, carry d(T) as each boundary triangle's d_ε(T)
      (§4.1.2 `:357-378`) into the consumers that read d_ε, retire I2d / I2e.
      *(2026-11-06: increment 2 LANDED — the owner-signed separation of an
      update simplex the other operand already holds; with the refusal off
      it converts all eight cases the refusal hid, corpus held. The
      retirement itself is BLOCKED: two pipeline-mesh volume pins lose
      20–23 % because a Stage-1 cylinder has no interior samples — the
      refusal stands in for §4.1 interior sampling, the item above; ledger N106.
      Owner ruling 2026-11-06: the refusal stays until the one CDT per
      face above lands; the retirement then re-runs from branch
      `n106-inc2-land`.)*
- [ ] **A first non-analytic instance** (a ruled / B-spline surface, P&T
      Ch. 10) passes through the pipeline end to end — the proof the
      interface is general, and the substrate for loft booleans
      (`specs/loft.md`, option (b)). Resolves D14.
- [ ] Retire the cross-patch folds (N88) and the self-contact guard (its
      switches go with it, cleanup §4) into the certificate and the loop.
- [ ] One operand's patches share their boundary samples bit for bit
      (§4.1.2 `:395-402`: "re-sample the boundary curves, and reconstruct
      the triangulation around the boundaries"), so its mesh holds no two points an ulp apart and
      the arrangement mints none of its own. Customer: the sub-τ collapse's
      operand-self twins (N89 item 1; pinned by
      `yang-rs/tests/stage6_arc_orientation.rs::pocket_operand_reenters_plain_boolean`,
      an operand that is the output of an earlier boolean);
      `collapse_subtauwork_mesh_edges` retires here.
- **Re-adjudicate on touch:** N5, N9, N27, N28, N34, N35.
- **Resolves:** N5, N61, N85, N88, N93, N106, D14.

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
- [ ] Stage-0 gap: two coplanar discs whose rims cross get no overlay
      (N108, found 2026-11-15). Build the lens through the §4.5.5 overlay:
      exact arc ∩ arc corners in both rims and their laterals, one mesh for
      both operands. Customers: F0094 moved rigidly (two bodies in 2 of 4
      motions, a failed auto-union in the other 2) and the loud subtract pin
      `disc_disc_crossing_subtract_is_loud_until_exact_arc_crossing`.
- [ ] R0023's coplanar cap residue (M8 follow-up; Stage 0's n-ary refusal, N25 — not N17, corrected 2026-10-27; it now fails first as a kernel-v2 cap segment face with < 3 vertices).
- **Re-adjudicate on touch:** N25, N26, N44, N64 (PROPOSED), N10, N12,
  N32, N36, N42, N43.

### F7 — Adjudicate what remains

A sign-off request is prose, with a paper cite and one recommendation,
never a menu (A15.7). One request may cover several entries when they share
one paper passage and one rationale; it names every ledger entry and matrix
row it covers, and each of them records the request it was signed under.
The classes expected: the corrections of bugs in the Cherchi reference
(N13–N24), and closed-form computation of an object the paper computes
numerically (the N7 family). Requests are drafted as each milestone lands,
so the signatures do not queue up behind F6. Expected: N13–N24 (corrections of
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
  milestone in §3's order (F5a is the earliest open one). A milestone may
  start before the previous one closes only where this file says its items
  are independent. F0's items and F4 do not depend on F1.

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
| 2026-10-29 | Remediation Phase 2 (not a milestone step): the harness on-surface oracle (O1) — corpus "held" now includes geometry; N100 item 3 measured in R0015 / R0026 / R0050 (chords on torus/cylinder faces), now WRONG and first in line | 61 | 132 | 357C / 3W / 9E / 5EE / 0T | 647a4736 |
| 2026-10-30 | WRONG tail (not a milestone step): R0015 / R0026 / R0050 CONVERTED — Stage-5 output typing types every chord between two different surfaces (carried curve, else the surfaces' ssi intersection; twin candidates within band are one; pair chords certified by bisection). N60 amended; N104 ledgered OPEN (Stage 3 leaves the perpendicular torus×plane section and Stage-0 overlay A×B curves untyped) | 62 | 132 | 360C / 0W / 9E / 5EE / 0T | (this commit) |
| 2026-10-30 | F2 Step 1 + review fixes (N81; N58 reopened; N105 added): Stage-5 polyline chain decimation re-sited into §4.4.1 (`stage4_chain_decimate`, forward §4.3.4, `DecimateStats` + loud `Stage4ChainDecimateFailed`; `Surface::Sphere` non-decimation stated), operand-anchored conic merge vertex-index-free (`0`-operand-vertex closed circle retains `min_by_key` index rule as open debt on `R0021`), Stage-1/`sphere.rs` unbundled, `developable.rs` hardened with `Result` + red-first pin (`curved_holed_lateral_reentry`), `4.4-23`/`4.4-28` updated (`stage5_topology.rs:1037`), class-(b) elision ledgered as N105 | 64 | 132 | 360C / 0W / 9E / 5EE / 0T (wall 524.8 s, 0 moved) | 3d98d89b, 41d3e61a |
| 2026-10-31 | F2 Step 2 (Antigravity; reworked after independent review): Fig. 11(a) split inside `update_patches_ex` by patch CDT; the `(3d)` unzip deleted and its local arms (first moved into the update for every refusal) narrowed to the N106 d(T)-budget refusal — any other declined component STOPs loudly into §4.5.2; hole-first patch chart fixed; `4.4-12` DEVIATES → PARTIAL (first committed as FAITHFUL; trigger still a collapsed triangle); N106 ledgered (the d(T) refusal is not the paper's) | 65 | 133 | 360C / 0W / 9E / 5EE / 0T (O1+O3 in force, no case moved — see commit) | (this commit) |
| 2026-11-12 | Plan revised after review (user: *"update the 1.0 plan and roadmap with those recommendations"*): F5 split into F5a (parameter-domain substrate: surface interface with the Theorem-A and Theorem-4.1 bounds, parameter coordinates carried, result-neutral) and F5b; order now F5a → F2 → I3b → F3 → F4 → F5b → F6 → F7; the N106 d(T) retirement moved from F2 to F5b; the conversion method (in place or a second path) is F5a's first item; class sign-offs in F7. No geometry changed | 64 | 132 | not re-run (no kernel change) | (this commit) |
| 2026-11-12 | Ablation experiment for the F5a design session (`specs/yang_paper_path.md`, branch `exp/paper-skeleton`): outer substitutes off → 354C / 0W, losses additive but for P0020; transits (N87), rim boost, cross-patch folds (N88) and edge-in-plane arm 2 (N64) have no corpus customer. No kernel change on main; the in-place / second-path choice stays open for the core | 64 | 132 | 360C / 0W / 9E / 5EE / 0T | (this commit) |
| 2026-11-13 | Outer-layer retirement 1 (`specs/yang_paper_path.md` §6, owner ruling on named dips): the cross-operand rim boost `phantom_min_rim_segments` deleted. The §4.5.2 loop answers its cases (F0088 at density 2, C0117 at 16). Named dip: none in the corpus; one kernel pin, a coaxial tube with a 1e-5 wall, now stops loudly (customer of F3 item I2). Seed 6 verdict-identical, 188C / 0W / 10E / 2T | 64 | 132 | 360C / 0W / 9E / 5EE / 0T | (this commit) |
| 2026-11-13 | Outer-layer retirement 2: the two a-priori Stage-1 folds deleted (`cyl_pair_phantom_n`, `face_rim_pair_phantom_n`; N88 amended, still OPEN for the chart-crossing re-run and the P0008 chord split). The re-run detects both configurations (§4.5.4). No named dip; thirteen outputs change, all CORRECT; seed 6 verdict-identical | 64 | 132 | 360C / 0W / 9E / 5EE / 0T | (this commit) |
| 2026-11-13 | Outer-layer retirement 3: the crease and corner transits deleted (`solve_crease_transit`, `corner_transit_apply`, `stage4_transit.rs`, `stage4_corridor.rs`, about 15,000 lines with tests; **N87 RESOLVED**). The detection stays and the §4.5.2 loop answers every case they touched. No named dip; every corpus output bit-identical; seed 6 verdict-identical; 1,440 rotated corpus runs verdict-identical with and without them. Edge-in-plane arm 2 (N64) was to go too and stays: 68 of 640 rigid motions of its anchoring pair fail without it, which the corpus, the seed and the fast tier could not see. A rigid-motion sweep now precedes a removal. Measured on the way: 67 of 1,440 rotated runs of CORRECT cases did not reproduce the unrotated result before these changes (`docs/yang_tail_triage.md`, 2026-11-13) | 63 | 132 | 360C / 0W / 9E / 5EE / 0T | (this commit) |
| 2026-11-13 | Rigid-motion sweep committed (`crates/test-harness/tests/rigid_motion_sweep.rs`) and run on the whole corpus at this tip: 1,371 of 1,440 rotated runs of CORRECT cases reproduce the unmoved result, 69 do not (62 loud, 7 quiet). **Named dip, late: C0117 stops at the §4.5.2 ceiling in 4 of 4 rigid motions without the rim boost** (retirement 1; the corpus holds it in one orientation), customer of F3 item I2. A wrong model found and fixed on the way (P0054 rotated; kernel-v2 `extent_along`, not a Yang mechanism) | 63 | 132 | 360C / 0W / 9E / 5EE / 0T | (this commit) |
| 2026-11-14 | Not a milestone step — a silent wrong model preempts: F0076 moved rigidly held material neither operand has (the Stage-1 mints, N82, turn a crossing into a touch; Stage 3 typed the touching chord as an arc outside the cylinder face). Trigger T9, `curve_domain_postcondition`, holds each typed intersection edge's arc to its faces' creases (§4.3.3 `:528-532` at §4.3.4's points) and the §4.5.2 loop builds the right solid; a certificate, not domain membership — N107 OPEN, F5a's parameter domain replaces it. Sweep: 1,372 of 1,440 reproduce (62 loud, 6 quiet), one verdict moved | 64 | 132 | 360C / 0W / 9E / 5EE / 0T | `fe13dfaf` |
| 2026-11-15 | Not a milestone step — T9 refused a correct build (CI red on `fe13dfaf`: test-harness `board_shape_kv2`, a hole beside a rounded plate corner). `face_sides` read a face's side of a curved divider from its mesh vertices; it now confirms the side against the mesh triangles (`divider_range_on_triangle`) and gives none where they extend to the other side. Withdraws certificates only; corpus `results.json` and all 374 sweep verdict files identical. N107 corrected, still OPEN | 64 | 132 | 360C / 0W / 9E / 5EE / 0T | (this commit) |
| 2026-11-16 | F5a design session, the choice: the core converts in place, in roadmap order (owner: *"in place, go ahead"*). `specs/yang_paper_path.md` §6 records why — the combined core trial needs the uncommitted I3b and N85 trials rebuilt, and every recorded trial loss names an earlier milestone. The hold on wiring Stage-4 increments is lifted. No geometry changed | 65 | 133 | not re-run (no kernel change) | (this commit) |
| 2026-11-16 | F5a increments 1–2 (`specs/yang_f5a_surface_interface.md`): `param_surface` — the `ParamSurface` interface, the five analytic instances, d(T) and the Theorem 4.1 normal cone generic over rational Bézier patches; `stage4_dt` now runs on it and the closed-form Gauss maps are pinned against the net cone. Result-neutral: geometry hash identical on all 374 cases against a control | 65 | 133 | 360C / 0W / 9E / 5EE / 0T | (this commit) |
| 2026-11-17 | F5a increment 3: (u, v) carried on every mesh vertex on every face it lies on — operand vertices by the closed-form inverse (N109 OPEN), arrangement vertices by §4.2.3 barycentrics in their source triangles, kept in lockstep through every Stage-4/5 vertex mutator, `settle` by incidence. Result-neutral: geometry hash identical on all 374 cases against a control. Census (`YANG_UV_CENSUS`, 315 cases): complete at every stage; largest \|s(u,v) − p\| / d_ε 0.999 at Stage 1 (Stage-0 chord corners), 23.3 at Stage 2, 1.005 at the Stage-5 exit. 4.2-24 DEVIATES → PARTIAL | 66 | 133 | 360C / 0W / 9E / 5EE / 0T | (this commit) |
| 2026-11-17 | F5a increment 4: the `Surface::` match inventory (`docs/audits/f5a_surface_matches.md`, generated, `--check` gates an unclassified site); 47 class-(b) sites converted in three batches — planarity → `is_affine()`, planar rules that read the plane → `affine_plane()`, the fan-rebuild θ unwrap → `SurfaceChart::u_period()`. Geometry hash identical on all 374 cases after batch 1 and after batches 1–3. 22 functions keep a class-(b) part, each named against F2/F3/F4/F5b/F6 | 66 | 133 | 360C / 0W / 9E / 5EE / 0T | (this commit) |
| 2026-11-17 | Matrix re-audit (not a milestone step): every row re-read at `244bb484` by a read-only Gemini swarm, each proposed change reviewed against the code before landing; 27 rows changed, none to FAITHFUL — 13 N/A NURBS rows re-graded under the reopened D14, 9 FAITHFUL rows with a defect in their own statement, 3 §4.3 rows DEVIATES → PARTIAL; summary table re-tallied. No geometry changed | 66 | 157 | 360C / 0W / 9E / 5EE / 0T | (this commit) |
| 2026-11-17 | Ledger catch-up (not a milestone step): N110–N118 — the Stage-6 envelope prepass (F3), the cylinder patch merge, `trim_beyond_corner_phantoms`, `run_absorption_attempt` (F2), the I6.5 wedge dedup, and four Stage-0 reconciliation passes (F6) had no entry; N100 item 6 (`SurfacePair` creases outside T2) measured on the corpus; item 10 (cone interiors deaf to §4.5.2) added | 75 | 157 | 360C / 0W / 9E / 5EE / 0T | (this commit) |
| 2026-11-17 | Ledger catch-up (not a milestone step): N119–N121 — the I6.6 pleat cancellation, the KV10/KV15 post-Stage-2 near-welds (F5a/F2), the PR-KV9 junction-twin collapse; N113 amended (`rehome_attempt` admits sites by surface-type count), N116 widened to the circle arms, N117 (post-overlay rim snap), N25 (n-ary 3D `MIN_FEATURE_SIZE` collapse). Two leads dismissed: the planar `merge_same_plane_patches` half is cleanup §2b / N53, the n-ary collapse is N25. No geometry changed | 78 | 157 | 360C / 0W / 9E / 5EE / 0T | (this commit) |
| 2026-11-15 | Not a milestone step — the quiet rotation mismatches read. R0057 moved is the unmoved solid (every edge on the unmoved curves to 7e-14; its torus face is rebuilt without a seam, which `signed_volume` does not read). The sweep probe reads place (`geo`): no mesh-only `OK` row is off by more than 1.2e-3 of its body. F0094 moved is two bodies because a crossing disc pair has no §4.5.5 overlay and the arrangement sees coplanar caps only on an axis-aligned plane — **N108 ledgered, OPEN, an F6 item**; matrix 4.5.5-07 FAITHFUL → PARTIAL. No kernel change | 65 | 133 | 360C / 0W / 9E / 5EE / 0T | (this commit) |
| 2026-11-18 | F2 item 2, increments 0–2 (`specs/yang_f2_patch_chart.md`): `stage4_patch_chart::domain_chart` charts a patch on its `ParamSurface` domain (disc, azimuth band, tube band, cap round an apex or pole, sphere zone); the update takes sphere and torus patches and cone caps on it. N122 ledgered. An interior vertex relocated past the curve is removed | 79 | 157 | 360C / 0W / 9E / 5EE / 0T | 150fc2f2, 1906b5e8 |
| 2026-11-20 | F2 item 2, increment 3: the update rebuilds every patch on the domain chart (plane, cylinder and cone too), with the seeded retry and the coincident-simplex re-plan on it. Both rebuilds compared on 16,249 patches first: plane and cylinder agree on every patch; of 563 cone differences 505 are rounding in the d(T) comparison (N106) and 58 are patches only the new chart rebuilds. No category move, every oracle detail identical, geometry hash identical on 364 of 374 | 79 | 157 | 360C / 0W / 9E / 5EE / 0T | (this commit) |
| 2026-11-20 | F2 item 2: the chain decimation's whole-patch fallback on the domain chart; the `Surface::Sphere` skip retired (N81, N122 amended). No category move, every oracle detail identical, geometry moved on R0019 and R0081 | 79 | 157 | 360C / 0W / 9E / 5EE / 0T | (this commit) |
| 2026-11-21 | F2 item 3, repair retirement: run absorption, group absorption and corner re-homing deleted (N113 RESOLVED; about 4,100 lines of code, 1,750 of tests; `stage4_rehome::classify`, a surface-type enumeration, gone). Measured first by a per-pass census and off-switch over the corpus, 1,496 rigid motions and seed 6. The corner merge stays: P0027 under one motion needs it (N2, pinned). Corpus hash file identical on all 374; seed 6 189C / 0W / 9E / 2T | 78 | 157 | 360C / 0W / 9E / 5EE / 0T | (this commit) |
| 2026-11-22 | F2 item 3: the fold the Stage-5 corner merge hides is read on the emitted body — T3c's adjacent-edge reading (`output_loop_spurs`, N83 amended, N2 amended). Censused print-only first over the corpus and 1,496 rigid motions (4,396 emitted bodies): straight corners fall at or below 4e-12 or at or above 1e-2 in the sine of the fold angle. As a trigger with the merge off, P0027 stops loudly under two motions and refinement does not answer it; the merge stays. No category move, every sweep line identical, geometry hash included | 78 | 157 | 360C / 0W / 9E / 5EE / 0T | (this commit) |
| 2026-11-22 | F2 item 3: the corner merge's on-curve arm (§I13c, `oncurve_terminal_overrun`) deleted. The merge's 113 applied sites over its six customers under rotation are 26 on a crease of one operand and 87 on the intersection curve; the 87 are curve vertices §4.5.3 left in place and recorded, in natural runs the loop discards, and without the arm T3c reads 176 conic folds in eleven such runs (R0070, R0095, F0064) and in no adopted body. Every sweep line identical; F0064's message reads 20 unresolved sites for 14. The crease arm stays (P0027, pinned) | 78 | 157 | 360C / 0W / 9E / 5EE / 0T | c1e01769 |
| 2026-11-22 | F2 item 3: the Stage-5 corner merge (`run_fold_merge_passes`, `stage4_fold_risk`, the construct/merge alternation) deleted. Each of its 26 crease sites arrives at the §4.4.1 update as a boundary chain that walks out to a vertex and back: a point relocated past the next chain vertex. The update now splits the constrained edge that contains the relocated point (Fig. 11(a), `seed_fig11a_chain_folds`, N2); a curved holder the d(T) budget refuses takes the local fan re-CDT. Under rotation: 32 splits seeded, 9 fan arms, the Stage-5 pass then applied in none of 1,870 builds; 6 sweep lines differ in the hash and the late digits of the render-mesh volume, verdicts as before (P0027, R0050, R0070): five natural runs the update refused and the loop refined are adopted as built. F0064's natural run has five plane patches rebuilt that the update refused, and its message reads 18 unresolved sites for 20. Matrix 4.4-12 stays PARTIAL: straight chains only | 78 | 157 | 360C / 0W / 9E / 5EE / 0T | 2888a53b |
| 2026-11-24 | F2 item 3: `repair_fan_cluster` (`retriangulate_collapsed_fan_regions`, its seam-run canonicalization and §4.3.4 sample drop) deleted (N58 amended). Its load-bearing folds are made by the beyond-corner trim's index-rewriting merges (N112; P0020: 152 merges, three edges left with four cylinder triangles each). The §4.4.1 update now holds them: the patch chart develops a folded patch, and a patch the d(T) budget refuses has its lapping triangles alone replaced by the CDT of their own boundary (`rebuild_patch_folds`, N2). With the pass switched off, 1,870 of 1,870 rotated builds line-identical, hash included; `results.json` byte-identical; seed 6 189C / 0W / 9E / 2T, every verdict and signature as before. P0020 moved rigidly stops under three of four motions before and after, on an unrelated face (open tail row) | 78 | 157 | 360C / 0W / 9E / 5EE / 0T | (this commit) |
| 2026-11-25 | F2 item 3, measured, nothing landed: `collapse_subtauwork_mesh_edges` censused (ledger N89 item 1). It acts in 10 of 1,496 rotated builds and in no corpus build; its edges are intersection-point pairs (F0059 needs it), rim points the late boundary-curve relocation brings together (R0015), a harmless rim-to-interior pair (C0043) and twins of one operand's own mesh (`pocket_operand_reenters_plain_boolean`). A merge with the paper's domain in its place held everything but the rewrite tier, which stopped on the operand-self twins: a Stage-1 defect, moved to F5b. Folding the N55 merge into the same domain makes F0060 a WRONG (the merges' order decides at a pinch); not done. The change was backed out | 78 | 157 | 360C / 0W / 9E / 5EE / 0T (kernel unchanged) | (this commit) |
| 2026-11-26 | F2 Fig. 11(b), measured, nothing wired: every close pair read by the faces its points lie on (`stage4_fig11b::pair_kind`, print-only). The N55 scan is §4.3.3's removal in five merges of six (259 of 294 corpus, 731 of 884 sweep), Fig. 11(b) in 33 and 147. F0060 read with every class merged: four closed solids whose pinch-line edges coincide, which the watertight oracle counts as two non-manifold edges — it is CORRECT today on two vertices an ulp apart at each line end. A line pinch is put to the owner (spec §8.3) | 78 | 157 | 360C / 0W / 9E / 5EE / 0T (`results.json` byte-identical; 1,870 sweep lines identical) | (this commit) |
| 2026-11-26 | F2 sub-step split (N81), measured, nothing wired: each same-point pair followed from the relocation to the update. §4.3.3's removal is made ahead of the §4.5.3 sweep (42 of 280 same-faces pairs on the corpus, 250 of 880 over the sweep), by the N55 scan and N47 weld behind it (238, 597) and by the sub-τ collapse (0, 21); the sweep removes 12 of 36 Fig. 11(b) pairs (78 of 179) as reversals | 78 | 157 | 360C / 0W / 9E / 5EE / 0T (`results.json` byte-identical; 1,870 sweep lines identical) | (this commit) |
| 2026-11-26 | F2 Fig. 11(b), increment 2a: the ordered N55 scan DELETED; the same-point classes merged in its place, all at once (`collapse::merge_vertex_classes_with_bounds`). 296 merges on the corpus (the scan's 294 + F0060's two pinch-line ends); no class reaches the update; the sub-τ collapse's Stage-4 site takes nothing on corpus or sweep (46 pairs in 10 rotated builds → 5 in 3, at its Stage-5 site). Sweep 1,379 OK / 106 ERR (was 1,378 / 107): F0059 m2 converts, F0059 m3 fails at the render gate on two crossing-point vertices no mesh edge joins; 39 lines differ in mesh hash only. Seed 6 unchanged | 78 | 157 | 360C / 0W / 9E / 5EE / 0T (`results.json`: one detail string, R0099) | (this commit) |
