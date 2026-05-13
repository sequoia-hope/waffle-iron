# PR-Y40 — Final Audit Memo

| Field | Value |
|---|---|
| Auditor | audit-y40 |
| Date | 2026-05-13 |
| Live tree HEAD | `57bfe32` (PR-Y40 INFRA, NOT pushed) |
| Parent | `2752016` (PR-Y39 ABORT) |
| Class | INFRASTRUCTURE-CLASS (env-gated probe, 0 LOC production logic) |
| Phase artifacts | Spec ✓ · Canary ✓ · Impl ✓ · Adversary ✓ |
| Verdict | **ACCEPT** |

---

## §0 Verdict (single paragraph)

PR-Y40 ships ~151 LOC additive env-gated instrumentation at `crates/kernel/src/tessellation/repair.rs::remove_winding_insensitive_duplicates` (the F.0→F.1 site) and produces a load-bearing empirical refutation of PR-Y39 §2.5's "16 D.1d-loser collisions" attribution chain (measured count: 4; off by 3-4× due to PR-Y39 conflating indices with triangles in its read of Y36's `indices_emitted_dispatch` field). All four FIP §5 phase artifacts exist with role separation across four distinct agents (spec-y40 / canary-y40 / impl-y40 / adversary-y40, per the INFRA-CLASS test-author waiver established by PR-Y29/Y33/Y36/Y37/Y38). DoD §1.5 adversarial gates all GREEN (probe-off byte parity preserved on F0020 spotlight, kernel lib 1262/24/42 stable, yang_fast 10/157 stable). A15.6 compliance is intact (Render-LOD-layer instrumentation, zero production behavior change). The commit body and spec carry honest no-last-bug language and explicitly own the cumulative 0-LOC production fix outcome across 9 cycles. The 6th-refutation finding is itself load-bearing positive value: PR-Y41 will not be scoped against the now-refuted N=16 frame. Recommend **ACCEPT** + Phase 8 push authorized.

---

## §1 FIP §5 phase-artifact checklist

| Phase | Artifact | Path | Status |
|---|---|---|---|
| 1 — Spec | `yang_pr_y40_collision_probe.md` (305 lines) | `specs/yang_pr_y40_collision_probe.md` | **GREEN** — load-bearing refutation, probe design, PR-Y41 anchor candidates (n)/(o)/(p), out-of-scope banked list, paper citations (Yang 2025 §4.4.1, Cherchi 2022 §3), risk/mitigation, no-last-bug language |
| 2 — Canary | `pr_y40_canary.md` (379 lines) | `docs/audits/pr_y40_canary.md` | **GREEN** — Gates 1-8 all measured; §3 F0020 attribution; §4 cohort independence; §6 PR-Y41 anchor banked; §7 verdict SHIP-INFRA/6th-refutation per plan logic |
| 3 — Tests | INFRA-CLASS waiver (no production logic change) | regression coverage = probe-off byte parity | **GREEN** — DoD §1.5 satisfied via canary Gates 2/7/8 + adversary Gates B/H/I |
| 4 — Impl | Commit `57bfe32` (4 files: repair.rs +151, canary memo, spec, wasm bundle) | `git show 57bfe32` | **GREEN** — additive env-gated; `seen: HashSet` production logic verbatim preserved; `results.json` correctly NOT staged |
| 5 — Adversary | `pr_y40_adversary.md` (166 lines, ACCEPT) | `docs/audits/pr_y40_adversary.md` | **GREEN** — Gates A-J all PASS on independent re-run from canary-y36 worktree against live HEAD `57bfe32`; non-destructive git confirmed |
| 6 — Audit | This memo | `docs/audits/pr_y40_validation.md` | **GREEN** (this audit) |

INFRA-CLASS waiver for test-author phase is consistent with PR-Y29 / Y33 / Y36 / Y37 / Y38 precedent. Default-off byte parity is the regression coverage; canary §5 Gate 2 + adversary §2 Gate B verify it independently.

---

## §2 Role separation — 4 distinct agents

| Role | Agent | Artifact ownership |
|---|---|---|
| Canary | `canary-y40` | `docs/audits/pr_y40_canary.md`; worktree probe build + Gates 1-8 measurement |
| Spec | `spec-y40` | `specs/yang_pr_y40_collision_probe.md`; verbatim from canary findings |
| Impl | `impl-y40` | Commit `57bfe32` (live tree, branch main); applied probe diff + WASM rebuild |
| Adversary | `adversary-y40` | `docs/audits/pr_y40_adversary.md`; independent re-run of Gates A-J; non-destructive git |

Per `feedback_oracle_credibility_via_role_separation`: canary built the probe and measured; adversary independently re-ran without inheriting canary's reasoning chain. The refutation count (PR-Y39 predicted 16, both canary and adversary measured 4) is reproducible across role-separated runs from independent shell sessions.

No test-author role per INFRA-CLASS waiver; this is consistent with the precedent chain of `pr_y29_shipped.md`, `pr_y33_shipped.md`, `pr_y36_shipped.md`, `pr_y37_shipped.md`, `pr_y38_shipped.md`.

---

## §3 DoD checklist — probe-off byte parity is load-bearing

| DoD §1.5 item | Status | Evidence |
|---|---|---|
| Pathological / near-tolerance inputs tested | **GREEN** | F0020 (degenerate-triangle cluster, NMM cohort) + F0044/R0045 (symmetric pairs) + F0045/R0092 (13K-collision retess pathology) — canary §3-§4, adversary §3-§4 |
| Degenerate geometry behavior validated | **GREEN** | 10/19 F0020 inv006 collisions are fully-degenerate (canary §3.5); behavior under probe is observation-only; production drop semantics unchanged |
| No NaN values introduced | **GREEN** | Probe operates on QPos quantized integers; no floating-point arithmetic added |
| No invalid topology produced | **GREEN** | Production `seen.insert(key)` path verbatim preserved; probe is parallel observation map |
| No regression in existing test suite | **GREEN** | Adversary Gate H: kernel lib 1262/24/42 IDENTICAL to PR-Y39 baseline; Gate I: yang_fast 10/157 IDENTICAL |
| Default-off byte parity (load-bearing) | **GREEN** | Adversary Gate B: F0020 spotlight at HEAD 57bfe32 with no Y40 env vars produces `Status:Failed; 40 unpaired (39 boundary, 1 NMM); 8 of 113 degenerate; 10 self-int` — IDENTICAL to PR-Y39 baseline |

The probe-off byte parity is the load-bearing DoD anchor for INFRA-CLASS work. Independent verification by both canary (worktree) and adversary (live HEAD) confirms it.

---

## §4 Empirical evidence cross-check (canary §3 vs adversary §3)

| Quantity | Canary §3.2-§3.4 | Adversary §3.1-§3.3 | Cross-check |
|---|---|---|---|
| F0020 inv006 n_tris_input | 138 | 138 | **byte-match** |
| F0020 inv006 total_collisions | 19 | 19 | **byte-match** |
| D.1d-loser collisions (kids 218/232/233) | 1+1+2 = 4 | 1+1+2 = 4 | **byte-match** |
| Distinct winners for D.1d losers | {196, 198, 199, 233-self} | {196, 198, 199, 233-self} | **byte-match** |
| Dominant mechanism share | 10/19 = 53% fully-degenerate | 10/19 (kid 235=6, kid 256=4) | **byte-match** |
| F0044 inv003 load-bearing collisions | 4 (sym pairs 19→21, 20→22) | 4 (sym pairs) | **byte-match** |
| R0045 inv003 collisions | 2 (single pair 476→477) | 2 | **byte-match** |
| F0045 retess-pass collisions | 13011 | 13011 | **byte-match** |
| R0092 retess-pass collisions | 13368 | 13368 | **byte-match** |

PR-Y39's predicted count (16) vs PR-Y40's measured count (4) is **off by 4×**; the refutation is empirically reproducible across canary + adversary independent runs. The root cause (indices-vs-triangles confusion: 3+6+9 = 18 indices ÷ 3 = 6 tris, 4 lose, 2 survive) is internally consistent with PR-Y39 §2.3's downstream observation that kids 218=0 / 232=1 / 233=1 survive into `remove_nonmanifold_topology_aware`.

This is the **load-bearing positive value** of PR-Y40: it forecloses PR-Y41 from being scoped against the wrong frame. The 6th-refutation framing is empirically grounded, not rhetorical.

---

## §5 A15 compliance

A15.6 (Hybrid B-Rep/Mesh Boolean Pipeline — Yang 2025) is the governing invariant. PR-Y40 instruments `remove_winding_insensitive_duplicates`, which sits in Waffle's Render-LOD repair layer downstream of Yang Stage 6 (B-Rep assembly + retessellation). The probe:

- **Does not alter pipeline behavior** — production `seen.insert(key)` drop logic verbatim preserved; probe-on/off byte parity verified at multiple gates.
- **Does not change analytical surface preservation** (A15.5) — operates on QPos quantized integers only; no geometric primitive modification.
- **Instruments the empirically-correct anchor** for the F.0→F.1 stage transition (canary §3.1: n_tris_input=138 byte-matches stage-f sub=0; total_collisions=19 byte-matches sub=0→sub=1 delta).
- **Respects A15.4 sequencing** — SSI solver work is independent and unaffected; PR-Y40 is Render-LOD layer only.

Spec §9 explicitly notes that `remove_winding_insensitive_duplicates` is a Waffle Render-LOD-only operation outside the Yang 2025 + Cherchi 2022 paper scopes; per `feedback_external_coherence`, the probe IS the empirical reference at this layer (no upstream paper-derived contract governs canonical-key collision attribution).

---

## §6 INFRA-CLASS framing audit

| Criterion | Status | Evidence |
|---|---|---|
| Production logic LOC = 0 | **GREEN** | `git diff 2752016..57bfe32 -- crates/kernel/src/tessellation/repair.rs` shows only additive env-gated blocks; `if seen.insert(key) { keep } else { drop }` flow verbatim preserved (canary §1.2) |
| Env-gated default-off | **GREEN** | `y40_collision_probe_enabled()` at L634-636 requires `Y40_COLLISION_PROBE=1`; all probe state writes guarded by `if y40_enabled` |
| Additive only | **GREEN** | 4 files: repair.rs +151, canary memo, spec, wasm bundle. No deletions in production code (1 deletion total is the doc bullet update) |
| WASM rebuild included | **GREEN** | `app/static/pkg/wasm_bridge_bg.wasm` regenerated (5046563 → 5059517 bytes); consistent with WASM workflow per CLAUDE.md |
| `results.json` correctly NOT staged | **GREEN** | Adversary Gate A confirms; `git status` shows it unstaged (yang test-result drift is a known background phenomenon; per project convention not auto-committed) |
| Cumulative probe complexity disclosed | **GREEN** | Spec §8.3 + commit body acknowledge ~1041 LOC cumulative probe LOC across `tessellation/mod.rs` + `oracle.rs` + `repair.rs`; explicit guard against unbounded probe-refinement loops |

INFRA-CLASS framing is intact. The probe is durable scaffolding for any future at-site measurement at the F.0→F.1 dedup site; the production drop behavior is unchanged.

---

## §7 Strategic context — 9-cycle trajectory; ~1041 LOC cumulative probe; ROI assessment

| PR | Outcome | Cycle role |
|---|---|---|
| Y25 | ABORT (canary) | Yang §4.4.1 mesh-updating refuted as immediate anchor |
| Y26 | ABORT (canary) | Cohort-wide missing-triangle defect; not the 3 plan candidates |
| Y27 | ABORT (canary) | flood_fill_patches drops 0 SourceFaces; D.1 split into 3 sub-mechanisms |
| Y28 | ABORT (canary) | D.1d kids 218/232/233 identified; fix-shape refused commit |
| Y36 | INFRA SHIP | Y36 inverse-probe source-face attribution (downstream) |
| Y37 | INFRA SHIP | H1/H2/H3 classification refined |
| Y38 | INFRA SHIP | Grid-sensitivity oracle gate; phantom-hypothesis refuted |
| Y39 | ABORT (canary) | F.1→F.2 anchor refuted; banked F.0→F.1 with N=16 attribution |
| **Y40** | **INFRA SHIP — 6th-refutation** | **PR-Y39 §2.5's N=16 attribution refuted; measured N=4; PR-Y41 frame foreclosed** |

**Cumulative cycle accounting:**
- 5 canary-stage ABORTs (Y25/Y26/Y27/Y28/Y39); 4 INFRA SHIPs (Y36/Y37/Y38/Y40); **0 production fix on F0020 Render LOD in 9 cycles**.
- Cumulative probe LOC: ~1041 (spec §8.3) — `tessellation/mod.rs` (+711 from Y36/Y37), `oracle.rs` (+179 from Y38), `repair.rs` (+151 from Y40).
- PR-Y40 contributes 151 LOC + a load-bearing refutation that forecloses one specific PR-Y41 wrong-anchor candidate (source-attribution policy at F.0→F.1 based on N=16 claim).

**ROI assessment:**
- **Positive:** PR-Y40 catches PR-Y39 §2.5's indices-vs-triangles conflation BEFORE PR-Y41 commits a production fix on a 4×-wrong premise. This is the discipline of `feedback_anchor_before_fix` paying off explicitly (cf. PR-Y39 catching the F.1→F.2 wrong anchor before PR-Y39 itself committed).
- **Negative-but-acknowledged:** 9 cycles without a production fix on F0020 Render LOD is non-trivial. Spec §8.3 explicitly flags this and recommends "if the next 2-3 cycles continue producing INFRA-only outcomes without converging on a load-bearing production fix, escalate to a different diagnostic frame (e.g., end-to-end Cherchi differential-diff at Render LOD, not just at Stage B)." That guard is appropriately placed.
- **Net:** ACCEPT under current discipline; team-lead should weigh the spec §8.3 escalation trigger before scoping PR-Y41 as another canary at the F.−1→F.0 upstream site. If PR-Y41 ABORTs at canary as well, the strategic-frame question becomes load-bearing.

Per `feedback_phase1_diagnosis_ranking_is_inference`: PR-Y40's refutation of PR-Y39's "16 D.1d collisions" is exactly the kind of inference-vs-measurement disambiguation that the rule mandates. The refutation is itself positive empirical progress.

Per `feedback_no_last_bug`: spec §7 and commit body explicitly carry "We do not know how many bugs remain" language; no closure claim.

---

## §8 Banked findings (from adversary §6)

1. **PR-Y41 primary anchor candidate** — re-canary F.−1 → F.0 (dispatch loop output) to find where the OTHER ~12 D.1d-emitted indices are lost (likely upstream degenerate-vert collapse). The dominant F.0→F.1 mechanism (53%) is fully-degenerate triangles, not D.1d positional duplicates.
2. **PR-Y41 explicit refutation guard** — Do NOT ship a source-attribution policy fix at `remove_winding_insensitive_duplicates` based on the N=4 D.1d-loser sample. Canary §3.4 explicitly warns N is too small to ground a policy.
3. **F0045/R0092 retess-pass pathology** — 13011/13368 collisions per invocation in fully-symmetric coplanar-overlap mass-dup. DIFFERENT defect mechanism (banked since PR-Y37; reaffirmed by PR-Y40 cohort §4).
4. **Strategic-frame escalation trigger** (spec §8.3) — if next 2-3 cycles continue INFRA-only without convergence, revisit diagnostic frame (e.g., end-to-end Cherchi differential-diff at Render LOD, or direct Cherchi-Rust port repair from PR-Y32 banked).
5. **Cherchi C++ TBB non-determinism** (PR-Y31 banked, unchanged) — F0020 reruns can still vary by missing-edge count under thread parallelism; use missing-count as gate, not extras.
6. **Brief path discrepancy** — Verification brief referenced `crates/kernel/src/boolean/tessellation/repair.rs`; actual file is `crates/kernel/src/tessellation/repair.rs`. Cosmetic-only; no audit issue.

All six items are correctly enumerated by the adversary; this audit confirms they are appropriately scoped as banked-not-blocking.

---

## §9 Final recommendation

**ACCEPT.**

Rationale:
- **FIP §5 GREEN** — 4-phase artifact chain complete with role separation across 4 distinct agents (spec / canary / impl / adversary).
- **DoD §1.5 GREEN** — probe-off byte parity is the load-bearing regression coverage; verified independently by canary Gate 2 + adversary Gate B.
- **INFRA-CLASS framing intact** — 0 LOC production logic; ~151 LOC additive env-gated probe; default-off byte-identical; production drop semantics verbatim preserved.
- **A15.6 compliant** — Render-LOD-layer instrumentation; no pipeline behavior change; no analytical surface impact (A15.5 unaffected).
- **Empirical evidence load-bearing** — PR-Y39 §2.5's N=16 D.1d attribution refuted to N=4 (off by 4×); measurement byte-matches across canary and adversary independent runs.
- **No-last-bug discipline GREEN** — adversary Gate J confirms only explicit negations in spec/canary/commit body; no closure language.
- **Strategic context honestly disclosed** — 9 cycles, 5 ABORTs, 4 INFRA SHIPs, 0 production fix on F0020 Render LOD is acknowledged in commit body and spec §8.3; PR-Y40 contributes a load-bearing refutation that forecloses one specific wrong PR-Y41 candidate.

**Phase 8 push authorized.** Recommend:
1. Commit this audit memo (`audit(yang-pr-y40): ACCEPT — ...`) + adversary memo.
2. Push origin main (plain push only per `feedback_always_push`; never force).
3. Memory update: `yang_pr_y40_shipped.md` + MEMORY.md one-liner noting INFRA-CLASS, 6th-refutation, PR-Y41 anchor banked at F.−1→F.0 upstream.
4. `TeamDelete pr-y40` per `feedback_per_plan_cycle_team`.

The cycle does NOT close Yang. PR-Y41 should be scoped as ANOTHER canary at the F.−1→F.0 upstream site, with explicit refutation guard against treating the N=4 D.1d-loser distribution at F.0→F.1 as policy-grade evidence. If PR-Y41 ABORTs as well, treat the spec §8.3 escalation trigger as load-bearing.
