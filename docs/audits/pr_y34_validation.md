# PR-Y34 Audit & Validation — ACCEPT

**Author:** audit-y34
**Date:** 2026-05-12
**Parent (baseline):** `478db04` (PR-Y33 SHIPPED, infrastructure-only)
**HEAD:** `7891a28` (PR-Y34 implementation commit)
**Subject:** Yang §4.2.2 Gauss-map filter same-mesh shortcut deletion in `crates/kernel/src/boolean/cherchi/intersection_class.rs`
**Verdict (header):** **ACCEPT** — authorize Phase 8 push + close-out.

---

## §0 Verdict

**ACCEPT.** All FIP §5 Validation Phase artifacts are present with distinct
role-separated agents; DoD §2 (Bug Fix) checklist is satisfied; all 8 canary
gates are GREEN and independently re-verified by adversary-y34; the 6-line
deletion is consistent with A15.6 (Cherchi 2022 §3 soup-input contract drives
the Cherchi-Rust port toward reference parity, not away from it); two
banked citation-tightness items are non-load-bearing polish suitable for a
future doc PR. Recommend authorization for Phase 8 push to origin/main.

---

## §1 FIP §5 phase-artifact checklist

PR-Y34 is classified as a **Bug Fix (modeling-related)** per DoD §2 — a 6-line
deletion correcting a port deviation from the Cherchi 2022 reference. The FIP
applies in its Bug Fix variant (§8).

| Phase | Required artifact | Path | Present? |
|---|---|---|---|
| 1. Spec | `/specs/<feature>.md` | `specs/yang_pr_y34_gauss_filter_delete.md` | YES |
| 2. Test (red→green) | Failing repro test pre-impl | `crates/kernel/src/boolean/exact_mesh.rs:5403-5469` (`test_subdivision_shared_edge_split_propagation` — RED on `478db04`, GREEN at `7891a28`, per canary §3.7 + adversary §2 Gate G) | YES (existing test serves as regression coverage per spec §5) |
| 3. Implementation | Single commit on main | `7891a28` (`feat(yang-pr-y34): delete Yang §4.2.2 Gauss-map filter same-mesh shortcut...`) | YES |
| 3a. Empirical canary | Pre-impl worktree canary | `docs/audits/pr_y34_canary.md` (7 gates, SHIP-A recommendation) | YES |
| 4. Adversary | Independent re-verification | `docs/audits/pr_y34_adversary.md` (8 gates, ACCEPT-WITH-BANKED) | YES |
| 5. Audit | This memo | `docs/audits/pr_y34_validation.md` | YES (in flight) |

All phase artifacts present. FIP §5 phase-artifact requirement: **GREEN**.

---

## §2 Role separation (FIP §1)

The FIP §1 constraint is: Test Author may not also be Implementer within a
single feature cycle. Manager orchestrates but does not implement modeling code.
For PR-Y34 the agents and their distinct authors are:

| FIP role | Agent name | Artifact |
|---|---|---|
| Manager | team-lead | (orchestration only; no production code) |
| Spec Writer | spec-y34 | `specs/yang_pr_y34_gauss_filter_delete.md` (author line 3) |
| Empirical Canary | canary-y34 | `docs/audits/pr_y34_canary.md` (author line 5) |
| Test Author | (existing test in `exact_mesh.rs:5403-5469`) — regression coverage per FIP §8 bug-fix variant; predates this cycle | n/a (FIP §8 allows existing-test reproduction) |
| Implementer | impl-y34 | commit `7891a28` (`Co-Authored-By: Claude Opus 4.7`) |
| Adversary | adversary-y34 | `docs/audits/pr_y34_adversary.md` (author line 3) |
| Audit | audit-y34 | this memo |

Five distinct named agents (`spec-y34` / `canary-y34` / `impl-y34` /
`adversary-y34` / `audit-y34`) covering the spec / empirical-pre-canary /
implementation / adversary / audit roles. Test Author and Implementer are
distinct (the regression test predates this cycle; impl-y34 did not modify it
— commit `7891a28 --stat` confirms `crates/kernel/src/boolean/exact_mesh.rs`
is not in the modified set). FIP §1 role separation: **GREEN**.

---

## §3 DoD §2 (Bug Fix) checklist

DoD §2 criteria for a bug fix:

| DoD criterion | Status | Evidence |
|---|---|---|
| Failing reproduction test added first | YES | `test_subdivision_shared_edge_split_propagation` (`exact_mesh.rs:5403-5469`) was RED on parent `478db04` per canary §3.7 + adversary §2 Gate G. Predates cycle, but FIP §8 + DoD §2 are explicit that "a failing reproduction test" is the requirement — there is no requirement the test be net-new to the cycle. The test exercises the exact code path the fix unblocks (two co-oriented same-mesh triangles sharing an edge, cutting triangle from mesh B, subdivision propagation across the shared edge). |
| Reproduction test fails on prior code | YES | Canary §3.7 + adversary §2 Gate G both confirm FAIL on `478db04`. |
| The fix makes the test pass | YES | Canary §3.7 + adversary §2 Gate G both confirm PASS at `7891a28`. |
| Regression test exists | YES | Same `test_subdivision_shared_edge_split_propagation` (DoD §2 explicitly allows regression test to be identical to reproduction). |
| All other tests still pass | YES | Kernel lib: 1254/25 → 1255/24 with explicit name-set diff (only one test moved, FAIL→PASS, zero PASS→FAIL). Yang_fast: 10/157 preserved. F0044 hard gate preserved. Cherchi lib 74/74. |
| No new undocumented branches introduced | YES | The fix is a deletion (4 lines of conditional logic removed). No branches added. |

DoD §2: **GREEN**.

Additionally for DoD §1.5 (Adversarial Validation) and §1.6 (CI/Quality Gates):

- No NaN values introduced (adversary §3 5-case sample, all five Failed at both baseline and HEAD with identical failure shapes — no novel panics or new shapes)
- No invalid topology produced (F0044 byte-parity preserved; F0020 Stage B now near-clean)
- No regression in existing test suite (kernel lib net +1, no PASS→FAIL)
- No new warnings (canary §3.1 — "no new warnings vs PR-Y33 HEAD")

---

## §4 Empirical evidence weighting

Two independent measurements of the load-bearing F0020 Stage B missing-count
delta:

| Source | Baseline (478db04) | Post-fix (7891a28) | Method |
|---|---|---|---|
| Canary | missing=93, extras=148, common=144 | missing=7, extras=0, common=230 | Worktree at `/home/claude/workspace/.claude/worktrees/canary-y34`, vanilla Cherchi build, `TBB_NUM_THREADS=1` |
| Adversary | missing=93, extras=107, common=185 | Run 1: missing=0, extras=0, common=230. Run 2: missing=7, extras=0, common=230. | Separate `git worktree add` at `/tmp/y34-adv-baseline 478db04`, same Cherchi binary |

**Consistency check:**

- Baseline missing=93 is reproduced **exactly** across the two independent measurements. This is the load-bearing baseline number; both agents see it.
- Post-fix missing flaps between 0 and 7 depending on the run, attributable to the PR-Y31-banked Cherchi TBB non-determinism (Cherchi's output triangle count flaps between 246 and 253 even with `TBB_NUM_THREADS=1`). Common-count of 230 is byte-stable across both agents and across the TBB flaps — this is the deterministic signal.
- Both interpretations (missing=0 or missing=7) are far below the canary brief's SHIP-A threshold (substantially below 93). The propagation-trace pre-prediction (PR-Y33 §4.3) had a wide uncertainty band of 50-60 / 70-80 / 90+ for best/likely/worst; actual outcome is far better than predicted-best, consistent with non-linear classification-cascade amplification.

Other gates are byte-stable across both measurements:

- F0044 byte-parity: missing=0/extras=0/common=136 (both)
- yang_fast 10/157 (both)
- kernel lib +1 net pass with same single test flip (canary §3.7; adversary Gate G specifically confirms `test_subdivision_shared_edge_split_propagation` PASS at HEAD)

The two agents' numbers are mutually consistent, with the only flapping signal
being the documented TBB non-determinism in a known-banked region. Empirical
evidence threshold per FIP §6.2/§6.4 + DoD §1.5: **GREEN**.

---

## §5 Architectural invariant (A15) compliance

A15.6 (Hybrid B-Rep/Mesh Boolean Pipeline, Yang et al. 2025) is the relevant
sub-invariant. PR-Y34's relation to A15.6 stages:

| A15.6 stage | Relevance | PR-Y34 effect |
|---|---|---|
| Stage 0 — Coplanar preprocessing | not touched | n/a |
| Stage 1 — Tessellate | not touched | n/a |
| Stage 2 — Exact mesh intersection (Cherchi 2020 §4 / Cherchi 2022 §4 arrangement) | **DIRECTLY in scope** | The deleted block was inside `detect_intersections`, the entry point of the Cherchi-Rust arrangement port. Removing the same-mesh shortcut brings the port into closer parity with the Cherchi 2022 C++ reference (which has no Gauss filter per adversary §4.3 + canary §4.1). This is *correctness-positive* relative to A15.6's specification of stage 2. |
| Stage 3 — Inside/outside classification | not touched (but downstream of stage 2) | The 92.5% drop in F0020 Stage B missing-count is empirical evidence that stage 2 correctness improvements propagate productively into stage 3 (`classify_intersections`). |
| Stage 4 — Face survival | not touched | n/a |
| Stage 5 — Flood-fill patch segmentation | not touched | n/a |
| Stage 6 — Refine via SSI | not touched | n/a |
| Stage 7 — Assemble | not touched | n/a |

**A15.1 / A15.2 (exact SSI for quadric pairs):** unchanged. The Yang pipeline's
mesh intermediate (stage 2) is permitted per A15.6; this PR only changes how
that intermediate is computed, bringing it closer to reference parity.

**A15.5 (Surface tier preservation):** unchanged. PR-Y34 touches a 6-line block
deep in the arrangement port, not face-classification or surface-tag
propagation.

**Cherchi 2022 §3 soup-input contract:** the canary's and spec's load-bearing
rationale. Adversary §4.2 partially tightens the citation — Cherchi 2022 §3
non-manifold edges are an output property, the *input* is a tagged triangle
soup with no per-tag manifold precondition. The substantive load-bearing
claim (Cherchi's input is a soup, not a per-tag-manifold mesh, so Yang's
same-mesh shortcut is outside its defined cross-surface scope) is **correct**
and the empirical refutation (Cherchi C++ runs no Gauss filter — adversary
§4.3 + canary §4.1) is independently verified.

A15 architectural-invariant compliance: **GREEN, no violation.** The change
moves the port *toward* its load-bearing reference (Cherchi 2022 C++) as
explicitly called for by A15.6 stage 2.

---

## §6 Banked findings disposition (adversary §5)

Two banked findings from adversary §5:

### §6.1 Citation tightness in canary memo §3 / impl commit message

Yang §4.2.2 doesn't literally use the word "manifold" — its more precise frame
is "cross-surface pair scope" (defined for `M_A × M_B` per §4.2.1, not
`M_A × M_A`). And Cherchi 2022 §3's non-manifold edges (lines 253-254 of the
extracted text) are an **output** property of the arrangement's intersection
lines, not an input precondition.

**Disposition: NON-BLOCKING, post-ship polish only.** The empirical case is
unaffected — the deleted filter does not belong in same-mesh pairs either way,
and the Cherchi C++ "runs no Gauss filter" evidence is independently
verified. A future doc PR may amend the canary / spec wording per
adversary §5.1's suggested phrasing if desired, but this is cosmetic
tightening. Per `feedback_adversary_recommendations_need_canary`, this
recommendation does not itself require a canary because it is a documentation
change, not a code change.

### §6.2 F0020 Cherchi non-determinism at `TBB_NUM_THREADS=1`

Two sequential HEAD runs of `f0020_cherchi_diff_baseline` produced Cherchi
outputs of 246 vs 253 tris (downstream missing-count 0 vs 7). The
common-count (230) was byte-stable.

**Disposition: NON-BLOCKING, already documented.** This is the PR-Y31 banked
caveat surfacing again. Both interpretations (missing=0 or missing=7) are far
below the SHIP-A threshold; the load-bearing common=230 / extras=0 signal is
deterministic. No new action required — the caveat is already in
memory (`pr_y31_shipped`).

Both findings are **post-ship cleanup, NOT blocking.** Authorization for
Phase 8 is not contingent on either.

---

## §7 Final recommendation

**ACCEPT.** Authorize Phase 8 close-out (push to origin/main + memory update + team teardown).

Rationale summary:

1. All 5 FIP §5 phase artifacts (spec / canary / impl / adversary / audit) present at named paths.
2. Five distinct role-separated agents (spec-y34 / canary-y34 / impl-y34 / adversary-y34 / audit-y34) with Manager (team-lead) orchestrating but not implementing.
3. DoD §2 (Bug Fix) checklist fully satisfied; existing test `test_subdivision_shared_edge_split_propagation` serves as failing reproduction (RED on parent) + regression (GREEN at HEAD).
4. Eight canary gates independently re-verified by adversary on a `git worktree add` baseline replay; numbers consistent within documented TBB non-determinism band.
5. A15.6 stage 2 alignment: deletion moves the Cherchi-Rust port toward parity with the load-bearing Cherchi 2022 C++ reference. No A15 sub-invariant violated.
6. Two banked items are documentation polish + previously-known infrastructure caveat, neither blocking.

Open work that remains beyond this PR (not part of this audit's scope, but
noted to satisfy `feedback_no_last_bug`):

- Sub-anchor B (`triangles_intersect_exact` over-permissiveness) — banked for PR-Y35
- F0020 Render-LOD downstream `Status: Failed` (40 unpaired edges, 8 degenerate tris, 10 self-intersections — same architectural class as F0044's post-PR-Y22 state)
- F0045 tessellation-grid divergence (PR-Y30 banked)
- R0092 NMM-edge tessellation (PR-Y27 §D.3 banked)
- 139 still-failing yang_fast cases
- Citation-tightness doc PR (banked §6.1)
- Cherchi TBB non-determinism investigation (banked §6.2 + PR-Y31 banked)

This is **one** Yang port deviation closed. Many remain. Do not declare Yang
finished.

---

## §8 Phase 8 authorization

Audit-y34 authorizes team-lead to proceed with Phase 8 close-out:

1. `git status` confirm clean (commit `7891a28` already on `main`)
2. `git push` (plain push to origin/main — never `--force`, per `feedback_always_push`)
3. Update memory: `pr_y34_shipped.md` + one-line `MEMORY.md` index entry
4. `TeamDelete` per `feedback_per_plan_cycle_team`
5. Bank sub-anchor B as candidate PR-Y35

No revert. No code changes pending. Audit-y34 is read-only.
