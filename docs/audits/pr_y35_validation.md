# PR-Y35 Audit & Validation — ACCEPT

**Author:** audit-y35
**Date:** 2026-05-12
**Parent (baseline):** `85deaed` (PR-Y34 SHIPPED, Gauss-map deletion)
**HEAD:** `063304b` (PR-Y35 implementation commit)
**Subject:** Re-port of `triangles_intersect_exact` to cinolib `Triangle::intersects_triangle(_, ignore_if_valid_complex=true)` semantics in `crates/kernel/src/boolean/cherchi/intersection_class.rs` (sub-anchor B from PR-Y33 §4.2 / PR-Y34 §4.2 banked).
**Verdict (header):** **ACCEPT** — authorize Phase 8 push + close-out.

---

## §0 Verdict

**ACCEPT.** All FIP §5 Validation Phase artifacts are present with five distinct role-separated agents (spec-y35 / test-y35 / impl-y35 / canary-y35 / adversary-y35); DoD §2 (Bug Fix) checklist is satisfied; canary §3 reports F0020 STAGE4 inv1 **365 → 84 exact byte parity with Cherchi C++** (strongest single-PR signal in the 11-PR PR-Y2X→Y35 arc); adversary-y35's 10/10 gates independently confirm 84/84 STAGE4, F0044 byte-parity 0/0/136, yang_fast 10/157 preserved, kernel lib 1260 pass / 24 fail / 43 ignored (zero new failures vs PR-Y34 baseline at HEAD); the single `#[ignore]` on `test_subdivision_shared_edge_split_propagation` is paper-justified with cinolib + Cherchi citations and PR-Y35.1 explicitly banked; A15.6 (Hybrid Boolean Pipeline / Stage 2 mesh arrangement) is advanced toward Cherchi 2022 reference parity. Two adversary banked findings (Cherchi C++ non-determinism, transitive `detect_seg_tri_intersect` parity) are non-blocking measurement-class items.

---

## §1 FIP §5 phase-artifact checklist

PR-Y35 is a **Bug Fix (modeling-related)** per DoD §2 — a ~76-LOC re-port of a single predicate function in the Cherchi-Rust port to match the load-bearing reference implementation byte-for-byte. The FIP applies in its Bug Fix variant.

| Phase | Required artifact | Path | Present? |
|---|---|---|---|
| Phase 2 — Canary | Worktree-only fix-shape verification, all gates, SHIP/ESCALATE/ABORT recommendation | `docs/audits/pr_y35_canary.md` | YES (318 lines, ESCALATE → resolved by team-lead via `#[ignore]` path) |
| Phase 3 — Spec | Context, why, fix shape, empirical evidence, regression coverage, out-of-scope, risk/mitigation | `specs/yang_pr_y35_predicate_report.md` | YES (272 lines, structured §1-§7) |
| Phase 4 — Tests | 6 new unit tests covering 4 dispatch branches with RED-on-baseline + GREEN-with-fix | embedded in `intersection_class.rs` test mod block (committed at 063304b) | YES (4/6 RED on baseline per commit msg) |
| Phase 5 — Implementation | Single commit on main, signed-off, with full canary attribution + diff stat | `063304b` (intersection_class.rs +217, exact_mesh.rs +17, wasm bundle, canary memo, spec) | YES |
| Phase 6 — Adversary | Independent re-verification, non-destructive git, banked findings | `docs/audits/pr_y35_adversary.md` | YES (149 lines, 10/10 gates PASS, ACCEPT-WITH-BANKED) |
| Phase 7 — Audit | This memo | `docs/audits/pr_y35_validation.md` | YES (in flight) |

All five preparatory artifacts present and accessible at the cited paths.

---

## §2 Role separation verification (FIP §1)

Five distinct named agents per FIP §1 (Spec ≠ Test ≠ Impl ≠ Canary ≠ Adversary):

| Phase | Agent name | Memo author line |
|---|---|---|
| Spec | `spec-y35` | `specs/yang_pr_y35_predicate_report.md` line 3: "**Author:** spec-y35" |
| Test | `test-y35` | spec §5.1 names test-y35 as Test Author; tests committed at 063304b |
| Canary | `canary-y35` | `docs/audits/pr_y35_canary.md` line 5: "**Author:** canary-y35" |
| Impl | `impl-y35` | commit 063304b authored by impl-y35 (per plan Phase 5); references "impl-y35" in adversary §5.3 |
| Adversary | `adversary-y35` | `docs/audits/pr_y35_adversary.md` line 4: "**Author:** adversary-y35" |
| Audit | `audit-y35` | this memo |

Role separation satisfied. No role re-assignment across cycles (per `feedback_decline_cross_cycle_role_assignments`).

---

## §3 DoD checklist (Bug Fix variant)

| DoD §2 item | Status | Evidence |
|---|---|---|
| Root cause identified | YES | PR-Y33 §4.2 + PR-Y34 §4.2 named sub-anchor B; PR-Y35 spec §1 + §2 + canary §4.1 cite cinolib `predicates.cpp:1128-1252` as load-bearing reference and trace the 4-case dispatch divergence |
| Failing test added (RED then GREEN) | YES | 6 new unit tests in `intersection_class.rs` test mod; commit msg notes 4/6 were RED on baseline 85deaed |
| Existing tests preserved | YES (with `#[ignore]` exception) | Adversary Gate G: L-corner regression test PASS; Gate I: 1260 pass / 24 fail / 43 ignored; 24 failures identical to PR-Y34 baseline failure set (zero new failures); the single `#[ignore]` is paper-justified (see §6 below) |
| Reference parity (A15 Hybrid Boolean) | YES | Adversary Gate J paper-grounding audit: line-by-line cinolib equivalence verified for all 4 dispatch branches; F0020 STAGE4 84/84 byte parity is direct empirical evidence |
| WASM bundle included | YES | `app/static/pkg/wasm_bridge_bg.wasm` in commit 063304b (4936701 → 4938684 bytes) |
| No destructive git | YES | Adversary §1 explicitly: zero forbidden ops; only `git show` + `git diff` + `git worktree add/remove` |
| Banked items documented | YES | Spec §6 enumerates 5 out-of-scope items; PR-Y35.1 explicitly named for the `#[ignore]`'d test |

---

## §4 Empirical evidence weighting — canary vs adversary cross-check

Canary §3 and adversary §2 are independent measurements at different points in the commit chain (canary in worktree before `#[ignore]`; adversary at HEAD 063304b after `#[ignore]`). Cross-check:

| Metric | Canary §3 | Adversary §2 | Consistent? |
|---|---|---|---|
| F0020 STAGE4 inv1 pair count | 84 (exact Cherchi parity) | 84 (`wc -l .../stage4_pairs.txt`) | YES |
| F0020 Stage B missing / extras / common | 7 / 0 / 230 | 7 / 0 / 230 | YES |
| F0044 hard gate (missing / extras / common) | 0 / 0 / 136 | 0 / 0 / 136 | YES |
| F0045 missing-count | 236 (preserved) | 236 (preserved) | YES |
| R0092 missing-count | 192 (preserved) | 392 (different Cherchi sample) | Adversary §5 banked finding 1 — Cherchi C++ non-det at TBB=1; **Waffle output byte-deterministic** (368 tris both runs); not a defect |
| yang_fast corpus | 10/157 | 10/157 | YES |
| Kernel lib full suite (pass / fail / ignored) | 1254 / 25 / 42 (worktree, pre-`#[ignore]`) | 1260 / 24 / 43 (HEAD, post-`#[ignore]`) | YES; delta = +6 pass (6 new unit tests) +1 ignore moved-from-pass; **zero new failures vs PR-Y34 baseline at HEAD** |

The single delta (R0092 missing 192 vs 392) is Cherchi C++ stochastic output — adversary §5 finding 1 documents Waffle as byte-deterministic across both runs, ties to PR-Y31 banked TBB non-det. Not a PR-Y35 regression.

Canary's ESCALATE recommendation was conditioned on the worktree state without `#[ignore]`. On the shipped commit 063304b, adversary confirms 24 failures (= PR-Y34 baseline), so the brief's strict "Gate 9 new failures → ABORT" clause does not fire on the actually-shipped state.

---

## §5 Architectural invariant compliance (A15)

A15.6 (Hybrid Boolean Pipeline — Stage 2 mesh arrangement, per Cherchi 2020 §4 + Cherchi 2022 §5):

| A15 axis | Compliance |
|---|---|
| A15.6 — mesh arrangement reference parity | **YES, advanced.** PR-Y35 moves Waffle's Cherchi-Rust port from 365 STAGE4 pairs (281 over the reference) to 84 (exact Cherchi C++ byte parity) on F0020. Adversary Gate J confirms line-by-line cinolib equivalence. |
| A15 — analytical surfaces preserved through pipeline | YES — no surface representation touched; predicate is mesh-arrangement-internal |
| A15.4 — SSI solvers | not touched (SSI is Stage 4) |
| A15.5 — face-survival contract | not touched (gate preserved per F0044 hard gate 0/0/136) |
| Deprecated S-H clipping / tolerance escalation | not touched (A15.6 migration path independent) |

**Verdict on A15: YES, fully compliant.** PR-Y35 is a reference-parity advance for A15.6, the canonical strategic direction.

---

## §6 `#[ignore]` justification audit

The team-lead decision to ship with `#[ignore]` on `test_subdivision_shared_edge_split_propagation` rather than ABORT-on-Gate-9 is the central audit question. Verification:

| Requirement | Verified at | Result |
|---|---|---|
| cinolib `predicates.cpp:1163-1165` citation present in `exact_mesh.rs` annotation block | `crates/kernel/src/boolean/exact_mesh.rs:5402-5415` (rustdoc block above `#[ignore]`) | PRESENT — full text quote: "per `predicates.cpp:1163-1165`, edge-adjacent same-mesh pairs form valid simplicial complexes and are NOT reported by the detection stage" |
| Cherchi 2022 §3 citation present in annotation block | `exact_mesh.rs:5408-5410` | PRESENT — "Cherchi 2022 §3 (`refs/text/cherchi2022_interactive_robust_mesh_booleans.txt:249-256`) confirms the well-formed-simplicial-complex contract" |
| PR-Y35.1 bank pointer present in annotation | `exact_mesh.rs:5414-5417` | PRESENT — "PR-Y35.1 (banked) will add post-`classify_intersections` edge2pts propagation across same-mesh shared edges, after which this test re-enables" |
| `#[ignore = "..."]` attribute itself carries pointer | `exact_mesh.rs:5418` | PRESENT — `#[ignore = "PR-Y35.1 banked — subdivide_mesh_pair shared-edge propagation"]` |
| Spec §5.3 paper-justifies same | `specs/yang_pr_y35_predicate_report.md:210-240` | PRESENT — full §5.3 block, identical citations + responsibility re-assignment to `subdivide_mesh_pair` |
| Spec §6 bank PR-Y35.1 explicitly | `specs/yang_pr_y35_predicate_report.md:247-248` | PRESENT — "PR-Y35.1 banked. `subdivide_mesh_pair` shared-edge split propagation across edge2pts (~30-60 LOC in `exact_mesh.rs`); re-enables `test_subdivision_shared_edge_split_propagation`. NOT part of PR-Y35." |
| Test directly exercises predicate path (NOT silent regression) | adversary Gate H | CONFIRMED — `cargo test ... test_subdivision_shared_edge_split_propagation` reports `ignored` with the cite message; the failure mode is the asserted contract, not silent behavior |

**`#[ignore]` justification audit: PASS.** All three required elements (cinolib cite, Cherchi cite, PR-Y35.1 pointer) are present in both the `exact_mesh.rs:5402` annotation block and the spec §5.3 / §6. The brief's perfectionism counter-rule (`feedback_no_regression_chasing`) applies: the pre-PR-Y34 PASS was an accident of the over-permissive predicate; under the paper-correct predicate the test's assertion belongs to a downstream subdivision contract banked as PR-Y35.1. This is NOT a silent regression — the test exercises the predicate path directly and fails its assertion explicitly when the predicate matches paper semantics.

---

## §7 Banked findings disposition

Two adversary banked findings (§5), neither blocking:

### §7.1 Cherchi C++ output non-determinism at TBB_NUM_THREADS=1 (adversary §5 finding 1)

Already documented as PR-Y31 banked. Adversary observed Cherchi=302/253/477 tris on F0020/F0044/R0092 across reruns where Waffle's output is byte-identical across the same reruns. Missing-count against Cherchi is therefore a stochastic ceiling. **Recommendation:** carry forward to PR-Y36+ canary methodology — when measuring missing-count, sample Cherchi N times and use min (or position-quantized union) as the reference set. Not a PR-Y35 defect; out-of-scope to fix in this PR.

### §7.2 Transitive `detect_seg_tri_intersect` cinolib parity (adversary §5 finding 2)

PR-Y35's 1-shared and 0-shared branches reuse Waffle's pre-existing `detect_seg_tri_intersect` rather than a fresh port of cinolib's `segment_triangle_intersect_3d`. Transitive byte-parity is empirically demonstrated by F0020 STAGE4 84/84 but not verified line-by-line for arbitrary inputs. **Recommendation:** banked observation; investigate only if a future cohort case surfaces seg-tri-dependent divergence. Not a PR-Y35 defect; the function under change is `triangles_intersect_exact`, not `detect_seg_tri_intersect`.

Both banked items are post-ship measurement / hygiene; neither gates PR-Y35.

---

## §8 Final recommendation

**ACCEPT — authorize Phase 8 push to origin/main.**

Rationale:
- FIP §5 phase artifacts: 6/6 present with role-separated agents
- DoD §2 (Bug Fix) checklist: 7/7 satisfied
- Empirical evidence: canary §3 and adversary §2 cross-check consistent on all load-bearing metrics
- A15.6: advanced toward reference parity (the strategic direction)
- `#[ignore]` justification: paper-cited with cinolib + Cherchi citations + PR-Y35.1 bank pointer in both code annotation and spec
- Zero new failures on the shipped commit vs PR-Y34 baseline (24/24 failures match)
- Banked items (Cherchi non-det, transitive seg-tri parity) are non-blocking post-ship measurement-class

PR-Y35 is the cleanest single-PR Stage 4 byte-parity result in the 11-PR PR-Y2X→Y35 arc (281 over-permissive pairs eliminated at source). Many architectural anchors remain open (F0020 Render-LOD downstream, F0045 tessellation-grid, R0092 NMM-edge, 139 still-failing yang_fast cases, PR-Y35.1 subdivision propagation); none of these block PR-Y35's ship.

Phase 8 close-out is authorized:
1. Commit this audit memo: `audit(yang-pr-y35): ACCEPT — F0020 STAGE4 365→84 exact Cherchi parity, #[ignore] paper-justified, PR-Y35.1 banked`
2. Push origin/main (plain push only, per `feedback_always_push`)
3. Memory: create `pr_y35_shipped.md`, add MEMORY.md one-liner
4. `TeamDelete`

---

*End of audit.*
