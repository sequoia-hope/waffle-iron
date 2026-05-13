# PR-Y35.1 Audit & Validation — ACCEPT

**Author:** audit-y35-1
**Date:** 2026-05-13
**Parent (baseline):** `248dae7` (PR-Y35 audit — ACCEPT, cinolib semantics re-port validated)
**HEAD:** `0d93b8d` (PR-Y35.1 implementation commit, not pushed)
**Subject:** Widen `triangulation` gate at `crates/kernel/src/boolean/cherchi/triangulation.rs:155-180` so triangles with non-empty `edge2pts` on any of their 3 edges are added to `tris_to_split`; re-enable `test_subdivision_shared_edge_split_propagation` (`crates/kernel/src/boolean/exact_mesh.rs:5403-5469`) which PR-Y35 had to `#[ignore]` due to the cinolib-correct predicate rejecting same-mesh shared-edge pairs as SIMPLICIAL_COMPLEX (sub-anchor banked at PR-Y35 §5.3).
**Verdict (header):** **ACCEPT** — authorize Phase 8 push + close-out.

---

## §0 Verdict

**ACCEPT.** All FIP §5 Validation Phase artifacts are present with five distinct role-separated agents (spec-y35-1 / test-y35-1 / impl-y35-1 / canary-y35-1 / adversary-y35-1). DoD §2 (Bug Fix) is satisfied: RED-on-baseline + GREEN-with-fix is independently verified by adversary §2 Gate D via non-destructive worktree replay; the re-enabled test PASSES at HEAD with the exact mechanism the canary §4.1 traced; the impl-added unit test `test_gate_widening_edge2pts_propagates_split_to_sibling` also PASSES; F0020 STAGE4 inv1 byte parity with Cherchi C++ holds at **84/84** (PR-Y35 win strictly preserved); F0044 byte-parity hard gate is preserved at **0 missing / 0 extras / 136 common**; kernel lib full suite is **1262 / 24 / 42** (adversary measurement matches the plan's Phase 6 Gate I prediction; canary memo §3's 1261 figure was a measurement-side omission of the impl-added unit test, not a production defect); failed-name 24-set is byte-identical to PR-Y35 baseline (zero new RED); yang_fast 10/157 preserved. A15.6 (Hybrid Boolean Pipeline / Stage 6 triangulation) advances toward Cherchi 2022 §3 segment-insertion contract via a paper-grounded strict superset of Cherchi C++'s observed gate predicate. Two banked findings (Cherchi C++ TBB non-determinism — already banked at PR-Y31; canary memo's kernel-lib count off by 1) are non-blocking.

---

## §1 FIP §5 phase-artifact checklist

PR-Y35.1 is a **Bug Fix (modeling-related)** per DoD §2 — a +35/-19-LOC production-code widening of a single gate predicate in the Cherchi-Rust port's triangulation stage, paper-cited per Cherchi 2022 §3. The FIP applies in its Bug Fix variant.

| Phase | Required artifact | Path | Agent | Present? |
|---|---|---|---|---|
| Phase 2 — Canary | Worktree-only fix-shape verification with all gates + SHIP/ESCALATE/ABORT recommendation | `docs/audits/pr_y35_1_canary.md` | `canary-y35-1` | YES (254 lines, SHIP, 11/11 gates GREEN) |
| Phase 3 — Spec | Context, why, fix shape, empirical evidence, regression coverage, out-of-scope, risk/mitigation | `specs/yang_pr_y35_1_triangulation_gate.md` | `spec-y35-1` | YES (268 lines, structured §1–§7) |
| Phase 4 — Tests | Re-enable `test_subdivision_shared_edge_split_propagation` + impl-added isolated unit test on the gate behavior with RED-on-baseline + GREEN-with-fix | `crates/kernel/src/boolean/exact_mesh.rs` (`#[ignore]` removal) + `crates/kernel/src/boolean/cherchi/triangulation.rs` (new `test_gate_widening_edge2pts_propagates_split_to_sibling`) — committed at `0d93b8d` | `test-y35-1` | YES (RED-on-baseline empirically replayed by adversary Gate D in throwaway worktree) |
| Phase 5 — Implementation | Single commit on main with full canary attribution + diff stat per `feedback_implementer_anti_fabrication_diff` | `0d93b8d` (`triangulation.rs` +115/-2 net, `exact_mesh.rs` +10/-6 net, wasm bundle, canary memo, spec) | `impl-y35-1` | YES |
| Phase 6 — Adversary | Independent re-verification, non-destructive git, banked findings | `docs/audits/pr_y35_1_adversary.md` | `adversary-y35-1` | YES (268 lines, 10/10 gates PASS, ACCEPT) |
| Phase 7 — Audit | This memo | `docs/audits/pr_y35_1_validation.md` | `audit-y35-1` | YES (in flight) |

All six preparatory artifacts present and accessible at the cited paths.

---

## §2 Role separation verification (FIP §1)

Five distinct named agents per FIP §1 (Spec ≠ Test ≠ Impl ≠ Canary ≠ Adversary), plus the audit role:

| Phase | Agent name | Authorship evidence |
|---|---|---|
| Canary | `canary-y35-1` | `docs/audits/pr_y35_1_canary.md` line 5: "**Author:** canary-y35-1" |
| Spec | `spec-y35-1` | `specs/yang_pr_y35_1_triangulation_gate.md` line 3: "**Authors:** spec-y35-1, canary-y35-1" |
| Test | `test-y35-1` | Plan Phase 4 names test-y35-1; `#[ignore]` removal + new unit test committed at `0d93b8d` |
| Impl | `impl-y35-1` | Commit `0d93b8d` per plan Phase 5; referenced in spec §3 and canary §0 |
| Adversary | `adversary-y35-1` | `docs/audits/pr_y35_1_adversary.md` line 4: "**Author:** adversary-y35-1" |
| Audit | `audit-y35-1` | this memo |

Role separation satisfied. No role re-assignment across cycles (per `feedback_decline_cross_cycle_role_assignments`). Spec is co-authored with canary, but canary remained the empirical authority and spec the prose authority — boundary preserved.

---

## §3 DoD §2 (Bug Fix) checklist

Per DoD §2, a bug fix must reproduce-RED → implement → regression-test → adversarial-validation:

| DoD criterion | Evidence | Status |
|---|---|---|
| Reproduce bug with failing test first | `test_subdivision_shared_edge_split_propagation` was `#[ignore]`'d at PR-Y35 (acknowledging the regression) and explicitly banked at PR-Y35 §5.3 as a known defect | YES |
| Confirm failing test on parent | Adversary §2 Gate D: in throwaway worktree at parent `248dae7`, removed only the `#[ignore]` line; test FAILS with `parent T0 has 4 sub-tris, T1 has 1 sub-tris` — exactly the canary §4.1 predicted mechanism | YES (independently verified) |
| Implement fix | Commit `0d93b8d`: gate widened with `has_edge_split` closure consulting `aux.edge_points_list` on each of the triangle's 3 edges | YES |
| Add regression test | (a) Re-enabled end-to-end `test_subdivision_shared_edge_split_propagation` (load-bearing FIP §4); (b) impl-added isolated `test_gate_widening_edge2pts_propagates_split_to_sibling` | YES (two layers: end-to-end + isolated) |
| Adversarial validation | Adversary §2 10/10 gates PASS (re-enabled test PASS, new unit test PASS, F0020 STAGE4 84/84, F0044 0/0/136, kernel lib 1262/24/42 with failed-name set byte-identical, yang_fast 10/157, paper-grounding audit verbatim-verified) | YES |
| Geometry health (DoD §1.3-ish) | Byte parity with Cherchi C++ on F0044 (136/136, well_formed=true, χ=4) preserved | YES |
| No regression in existing test suite | Failed-name 24-set is byte-identical to PR-Y35 24-name baseline; canary §3 lists all 24 verbatim; adversary §2 Gate I `diff`-verifies the set | YES |

All criteria satisfied.

---

## §4 Empirical evidence weighting (canary vs adversary cross-check)

Canary and adversary measurements agree on every load-bearing quantity with one cosmetic discrepancy.

| Gate | Canary measurement | Adversary measurement | Agreement | Load-bearing? |
|---|---|---|---|---|
| Re-enabled test at HEAD | PASS | PASS | YES | YES (Gate 2 / B — the acceptance gate) |
| Re-enabled test at parent (RED proof) | not performed | FAIL with predicted signature | adversary-only | YES (DoD §2 reproduce-RED) |
| New unit test at HEAD | not measured (not in canary's diff) | PASS | adversary-only | YES (FIP §4 isolated coverage) |
| F0020 STAGE4 inv1 pair count | 84 | 84 | YES | YES (PR-Y35 win preservation) |
| F0020 STAGE4 inv0 pair count | 20 | 20 | YES | YES |
| F0020 Stage B common | 230 | 230 | YES | YES (Waffle-side determinism check) |
| F0020 Stage B missing | 7 | 54 / 7 (TBB non-det; banked PR-Y31) | non-det upstream | NO (use Cherchi-deterministic STAGE4 pair count + common as load-bearing gates) |
| F0044 missing/extras/common | 0 / 0 / 136 | 0 / 0 / 136 | YES | YES (hard gate) |
| F0045 missing/extras | 236 / 466 | (deferred to aggregate Gate H) | aggregate-only | YES (cohort preservation) |
| R0092 missing/extras | 192 / 368 | (deferred to aggregate Gate H) | aggregate-only | YES |
| yang_fast pass/fail/error | 10/139/8 | 10/139/8 | YES | YES |
| Kernel lib pass/fail/ignore | **1261 / 24 / 42** | **1262 / 24 / 42** | DISCREPANCY (+1 in adversary) | YES (zero-new-RED) |
| Failed-name 24-set | listed verbatim | byte-identical (diff'd against canary list) | YES | YES |

**Discrepancy reconciliation (kernel lib pass count).** The plan's Phase 6 Gate I explicitly predicted **1262 / 24 / 42** (`PR-Y35 baseline 1260/24/43 + re-enabled test moves ignored→passed + impl-y35-1 added new unit test = +2 pass / -1 ignored`). Adversary measured 1262 at HEAD — matches the plan. Canary measured 1261 — off by one. Adversary §5.2 attributes this to the canary's worktree state predating the impl-added unit test (the canary tested the gate widening + `#[ignore]` removal but ran before test-y35-1's optional new unit test was added). This is a memo-side measurement omission, not a production defect: the zero-new-RED claim (24 named failures byte-identical to PR-Y35 baseline) holds on both measurements. The adversary's number aligns with the plan and with the actual HEAD state; the canary's table is internally consistent but stale by one new passing test. Accepted as banked (see §6.2 below) and reconciled here.

The adversary's independent RED-on-baseline replay (Gate D) is the load-bearing strengthener over canary: it converts "the test was `#[ignore]`'d in PR-Y35" into "the test empirically fails at parent with the predicted defect signature" — exactly the DoD §2 reproduce-RED requirement.

---

## §5 Architectural invariant compliance (A15.6)

PR-Y35.1 modifies Stage 6 (triangulation) of the Yang hybrid boolean pipeline as defined in A15.6 (`governance/ARCHITECTURAL_INVARIANTS.md:474-530`). The change advances Stage 6 toward Cherchi 2022 §3 segment-insertion contract:

> *"Inserting a segment amounts to eliminating, from the current tessellation, all triangles that conflict with it, and then re-triangulate the so-generated polygonal pocket, while making sure that the wanted segment is part of the new tessellation."* — Cherchi 2022 §3 lines 315-319 (adversary §4.1 verbatim-verified)

The pre-PR-Y35.1 gate (consulting only `triangle_has_intersections || triangle_has_coplanars`) excludes a triangle whose edge has been split by a sibling's intersection — but that triangle DOES conflict with the segment by definition (it shares the edge being subdivided). The widening adds the `edge2pts` consultation that the paper's contract requires for global segment propagation. Cinolib `triangulation.cpp:145-150` (adversary §4.2 verbatim-verified) does NOT widen the gate explicitly; it relies on redundant cross-mesh flagging in real corpus geometry, which the canary §4.3 and adversary §4.2 both confirm preserves correct shared-edge conformal output for F0020/F0044 (84/84 + 136/136 byte parity).

PR-Y35.1's edge2pts-widening is therefore a **paper-grounded strict superset** of Cherchi C++'s observed behavior:
- Never excludes a triangle Cherchi C++ would include (the original `flagged` predicate is preserved as a disjunct).
- Includes triangles whose `edge2pts` already has data from a sibling's classification call — i.e. the "triangles that conflict with" the segment per the paper's contract.
- Cannot produce invalid output: adding a triangle with non-empty `edge2pts` to `tris_to_split` only routes it through `triangulate_single_triangle`, which already correctly consults `edge_points_list` at L262-264.
- Empirically zero impact on real corpus: F0020 STAGE4 84/84 + F0044 136/136 + yang_fast 10/157 + cohort missing-counts all preserved.

This is an acceptable A15.6 advance: we align Stage 6 with the paper-contract, not with the C++ accident of redundant flagging. The judgment is paper-aligned per `feedback_yang_only` and `feedback_no_regression_chasing` (this is a paper-correct fix that UNWINDS PR-Y35's `#[ignore]`).

Other A15 invariants unaffected: A15.1/A15.2 (no mesh fallback for quadric pairs) — not touched; A15.3 (rationale) — preserved; A15.4 (SSI implementation sequence) — not touched; A15.5 (surface tier preservation) — not touched.

---

## §6 Banked findings disposition

### §6.1 Cherchi C++ TBB non-determinism on F0020 Stage B missing (already banked at PR-Y31, non-blocking)

Adversary §5.1: two reruns of `f0020_cherchi_diff_baseline` at HEAD produced Cherchi-side outputs of 302 tris vs 253 tris, yielding missing counts of 54 vs 7. Both runs used `TBB_NUM_THREADS=1` per PR-Y31 banked guidance. Waffle output deterministic at 246 tris with 230 common in both runs. This is an upstream Cherchi C++ TBB non-determinism issue that PR-Y31 already banked (memory: `yang_pr_y31_shipped.md` — "Cherchi non-det survives TBB pin in some F0020 reruns — use missing-count (deterministic) as gate, not extras"). PR-Y35.1 does not alter Cherchi's behavior; the variance is upstream of the change. **Recommendation for PR-Y36+:** Adopt min(missing) over N reruns, or rely on the STAGE4 pair count gate (deterministic at 84 in both runs) and the `common` count (deterministic at 230 in both runs) as the load-bearing gates instead of `missing`. **Disposition: non-blocking; continued bank.**

### §6.2 Canary memo's kernel-lib total (1261) is +1 short of the plan-predicted and adversary-measured 1262 (cosmetic, non-blocking)

Adversary §5.2: canary §3 reports **1261 / 24 / 42**; the plan's Phase 6 Gate I predicts **1262 / 24 / 42** (because impl-y35-1 added the new unit test `test_gate_widening_edge2pts_propagates_split_to_sibling`); adversary measured **1262 / 24 / 42** at HEAD. Likely cause: the canary's worktree state predated the impl-added unit test (canary §1 notes "production code in `triangulation.rs` + `exact_mesh.rs`", suggesting the gate widening + ignore-removal but not test-y35-1's optional new unit test). This is a memo-side measurement omission, not a production defect: the zero-new-RED claim is preserved on both measurements (the failed-name 24-set is byte-identical in both canary §3 and adversary §2 Gate I). **Disposition: non-blocking; recorded here for completeness; future canary-on-impl-output protocol may want a re-measurement step after impl commits the optional artifacts.**

Neither banked finding blocks ship.

---

## §7 Final recommendation

**ACCEPT.** Authorize Phase 8 close-out:

1. Commit this audit memo as `audit(yang-pr-y35-1): ACCEPT — triangulation gate widening, edge2pts-driven conformal subdivision restored`.
2. Push origin main (plain push only; never `--force`; per `feedback_always_push`).
3. Write memory: `yang_pr_y35_1_shipped.md` + one-line MEMORY.md index entry.
4. `TeamDelete pr-y35-1` (per `feedback_per_plan_cycle_team`).

**Open work remains** (per `feedback_no_last_bug` — this PR does NOT close Yang). Banked, unchanged from PR-Y35 §6 / spec §6:

1. F0020 Render-LOD downstream Status:Failed (~40 unpaired edges, same defect class as F0044 Failed since PR-Y22) — rightful PR-Y36+ anchor
2. F0020 Stage B missing=7 residual — same Render-LOD layer
3. F0045 tessellation-grid divergence (Yang §4.1.1) — missing=236, extras=466; Stage 1 tessellation grid
4. R0092 NMM-edge tessellation gap (PR-Y27 §D.3) — missing=192
5. 139 still-failing yang_fast cases — corpus aggregate 10/157 preserved
6. 24 pre-existing kernel lib failures — all downstream of cherchi triangulation pass
7. Cherchi C++ TBB non-determinism (PR-Y31 banked, recapitulated above §6.1)

PR-Y35.1 closes one sub-anchor within one stage (Stage 6, triangulation) of one pipeline (Yang hybrid boolean) of one feature class (mesh booleans). The validation evidence is the strongest in this PR's class: 0/1 wrong-anchor count (first clean single-cycle PR in 10+ cycles per canary §6); independent RED-on-baseline replay; byte-parity preservation on both Cherchi-reference gates (F0020 STAGE4 + F0044); zero-new-RED in the full kernel lib suite with the failed-name set byte-identical to the PR-Y35 baseline.

Default-ACCEPT criteria from team-lead's brief are satisfied: FIP §5 GREEN, DoD GREEN, role separation holds, A15 (Stage 6 / triangulation gate widening) is paper-grounded strict superset of Cherchi C++, banked findings non-blocking.

---

*End of validation memo.*
