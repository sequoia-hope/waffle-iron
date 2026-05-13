# PR-Y40 — Adversarial Validation Memo

| Field | Value |
|---|---|
| Adversary | adversary-y40 |
| Date | 2026-05-13 |
| Live tree HEAD | `57bfe32` (PR-Y40 INFRA, not pushed) |
| Parent | `2752016` (PR-Y39 ABORT) |
| Class | INFRASTRUCTURE-CLASS (env-gated probe, 0 LOC production logic) |
| Discipline | Zero destructive git on live tree — `git worktree add /tmp/...` for baseline replay only |
| Verdict | **ACCEPT** |

---

## §0 Verdict (single paragraph)

PR-Y40 ships ~151 LOC additive instrumentation at the F.0 → F.1 site (`remove_winding_insensitive_duplicates` in `crates/kernel/src/tessellation/repair.rs`), default-off byte-identical, with empirical refutation of the PR-Y39 §2.5 attribution chain (predicted 16 D.1d-loser collisions at F.0→F.1; measured 4, off by 3-4×). All ten gates (A–J) pass in independent re-runs from the canary-y36 worktree against live HEAD: probe-off F0020 spotlight matches PR-Y39 baseline byte-for-byte (40 unpaired / 8 degen / 10 self-int), probe-on fires and emits the documented TSV schema, cohort independence holds (F0044=4, R0045=2), kernel-lib regression is 1262/24/42 (zero new failures), yang_fast holds 10/157, and the spec/canary cite Yang §4.4.1 + Cherchi 2022 §3 with explicit no-last-bug language. The probe is well-scoped, the canary memo distinguishes measurement from inference, and the PR-Y41 anchor is honestly banked (the cycle does NOT close Yang). Recommend **ACCEPT** without banked findings beyond those already enumerated in the canary's "Open beyond this PR" list.

---

## §1 Discipline — non-destructive git proof

All baseline-state inspection used `git show` and `git worktree add`. Zero `stash`, `checkout`, `reset`, or `restore` on the live tree.

| Action | Command class | Live-tree mutation |
|---|---|---|
| Inspect commit metadata | `git show 57bfe32 --stat` | None |
| Inspect production diff | `git diff 2752016..57bfe32 -- ...` | None |
| Inspect parent baseline source | `git worktree add -f /tmp/y40-adv-baseline 2752016` then `git worktree remove --force` | None on `/home/claude/workspace` |
| All probe runs | `cargo test ...` | None (read-only test execution + `/tmp/...` output) |

Per `feedback_adversary_no_destructive_git`. Confirmed.

---

## §2 Gate A–J verification table

| Gate | What | Result | Detail |
|---|---|---|---|
| A | Diff shape | **PASS** | 4 files staged: `crates/kernel/src/tessellation/repair.rs +151`, `docs/audits/pr_y40_canary.md +379`, `specs/yang_pr_y40_collision_probe.md +305`, `app/static/pkg/wasm_bridge_bg.wasm` (binary). `app/tests/cases/assay/results.json` modification correctly NOT staged. Brief listed path as `crates/kernel/src/boolean/tessellation/repair.rs`; actual project layout is `crates/kernel/src/tessellation/repair.rs` (the brief had a copy-error). No issue. |
| B | Probe-off byte parity | **PASS** | F0020 spotlight at HEAD `57bfe32` with `YANG_BOOLEAN=1 YANG_CONFORMAL_PROBE=1` (no Y40 vars): `Status:Failed`, `40 unpaired edges out of 188`, `8 of 113 triangles degenerate`, `10 inter-face triangle penetrations`. Identical to PR-Y39 baseline. Log at `/tmp/y40-adv-probeoff.log`. |
| C | Probe-on fires (F0020) | **PASS** | With `Y40_COLLISION_PROBE=1 Y40_COLLISION_PROBE_DIR=/tmp/y40-adv-probe`, 6 invocations emitted; `F0020_inv001..006_{collisions,histogram,summary}.tsv` all created. `Status:Failed`, `40 unpaired` unchanged (no test-outcome contamination from probe). |
| D | F0020 inv006 attribution (load-bearing) | **PASS** | inv006 collisions.tsv: 19 total rows. Losers in {218, 232, 233}: 1+1+2 = **4 collisions**. Matches canary §3.3 exactly. PR-Y39's predicted 16 is **REFUTED**. |
| E | F0020 inv006 winner distribution | **PASS** | The 4 D.1d-loser rows have winners {196, 198, 199, 233-self} — **4 distinct winners**, each at 1/4. Distributed pattern; matches canary §3.4 exactly. No concentrated-winner signature even at the load-bearing site. |
| F | Cohort independent | **PASS** | F0044 cohort run (`spotlight_f0044` batch = F0044 + F0045 + R0092): `F0044_inv003 = 4 collisions` (symmetric pairs 19→21 ×2, 20→22 ×2). R0045 run (`spotlight_r0045`): `R0045_inv003 = 2 collisions`. F0045 + R0092 each have one huge retess-pass invocation (13011, 13368 collisions) — canary §4.3 banked as DIFFERENT defect mechanism. All four cohort observations match canary §4.1 table exactly. |
| G | Baseline replay (non-destructive) | **PASS** | Worktree at `2752016`: `grep -c "Y40_COLLISION_PROBE\|y40_first_seen" crates/kernel/src/tessellation/repair.rs = 0`. Probe code is absent from parent. Worktree removed. |
| H | kernel lib regression | **PASS** | `cargo test -p kernel --lib`: `1262 passed; 24 failed; 42 ignored` — matches baseline exactly. Zero new failures. |
| I | yang_fast corpus | **PASS** | `YANG_BOOLEAN=1 ... yang_fast --test-threads=1`: `Yang fast: 10/157 passed, 139 failed, 8 errored (skipped 33 known timeouts)`. Meets ≥10/157 requirement. |
| J | Paper-grounding + no-last-bug | **PASS** | Only `closes yang/last gap/fixes yang/status.*passed` hit in spec is the **explicit negation**: `"No 'this closes Yang' or 'this is the last bug' language. We do not know how many bugs remain (feedback_no_last_bug)."` Spec §9 cites Yang 2025 §4.4.1 (`refs/text/yang2025_hybrid_boolean.txt:605-610`) and Cherchi 2022 §3 (`cherchi2022_interactive_robust_mesh_booleans.txt:240-320`) with the honest caveat that `remove_winding_insensitive_duplicates` is a Waffle Render-LOD-only operation outside both papers' scope and the probe IS the empirical reference. |

---

## §3 Independent F0020 inv006 attribution (Gates D + E)

Adversary-run TSVs at `/tmp/y40-adv-probe/F0020_inv006_*`.

### §3.1 inv006 summary

```
metric                       value
invocation                   6
n_tris_input                 138
total_collisions             19
distinct_winner_face_ids     9
distinct_loser_face_ids      9
```

138 → 119 = 19-tri drop ≡ stage-f sub=0 delta. Confirmed load-bearing invocation.

### §3.2 Loser histogram (independent re-run)

```
loser_face_id  count
212            1
218            1   ← D.1d (kid 218)
227            2
229            1
231            1
232            1   ← D.1d (kid 232)
233            2   ← D.1d (kid 233)
235            6
256            4
```

D.1d-loser collisions: 1 + 1 + 2 = **4**. PR-Y39 §2.5 predicted **16**. Refutation is sound at the level of the basic count, independent of any interpretation. The canary's tri-vs-index reconstruction (3+6+9 indices / 3 = 6 tris dispatched, of which 4 lose) is consistent with both PR-Y39 §2.3's downstream count (kid 218=0, 232=1, 233=1 surviving F.1) and PR-Y40's measured F.0→F.1 loss attribution.

### §3.3 Winner kids for the 4 D.1d-loser collisions (independent re-run)

From `awk 'NR>1{print $11, $14}' inv006_collisions.tsv | sort | uniq -c` filtered to D.1d losers:

```
loser  winner
218    196
232    199
233    198
233    233  (self-collision)
```

Four distinct winners (196, 198, 199, 233). Two of those are even self-collisions within the same kid 233. By the canary §3.4 verdict logic (≥80% top-3 → concentrated; ≥10 distinct → distributed): N=4 is **too small to support a source-attribution policy fix** (PR-Y41). The canary's §3.4 explicit caveat ("THE SAMPLE IS TOO SMALL TO BE LOAD-BEARING") is the disciplined conclusion.

### §3.4 Dominant mechanism at F.0→F.1 (PR-Y40's most important finding)

Canary §3.5 reports that 10 of 19 collisions at inv006 are **fully-degenerate** (zero-area triangles where all three quantized vertices coincide), dominated by kid 235 self-collisions and kid 235 → 256 cross-collisions at key `(65051,-15817,-36086) × 3`. This is consistent with the inv006 loser histogram (kid 235 = 6, kid 256 = 4, total 10/19 = 53%). The D.1d signature (4/19 = 21%) is **secondary**, not dominant. This reframes PR-Y41 from "source-attribution policy at F.0→F.1" to "upstream dispatch-loop degenerate emission" (the canary's primary recommendation).

---

## §4 Cohort verification (Gate F)

Independent re-runs at `/tmp/y40-adv-probe-f0044/` and `/tmp/y40-adv-probe-r0045/`:

| Case | Load-bearing invocation | Total collisions | D.1d signature? | Adversary verdict |
|---|---|---|---|---|
| F0044 (inv003) | n_tris=120 | **4** | No — 19→21 ×2, 20→22 ×2 (symmetric pairs) | Matches canary §4.2 |
| F0045 (inv010) | n_tris=13535 | **13011** | No — retess pathology (banked, DIFFERENT defect) | Matches canary §4.3 |
| R0045 (inv003) | n_tris=608 | **2** | No — single pair 476→477 | Matches canary §4.1 |
| R0092 (inv017) | n_tris=13692 | **13368** | No — retess pathology (same as F0045) | Matches canary §4.3 |

No D.1d signature in any cohort case. The PR-Y37 H1/H2/H3 finding that F0044/R0045 cohort is 0% D.1 holds at this measurement layer too. F0045/R0092 retess-pass pathology is correctly flagged as a separate mechanism for future investigation; the canary explicitly bans treating it as same-mechanism evidence.

---

## §5 Paper-grounding (Gate J)

Searched both spec and canary for last-bug / fixes-Yang / status-passed language. Only hit is the **explicit negation** in spec §10:

> "No 'this closes Yang' or 'this is the last bug' language. We do not know how many bugs remain (`feedback_no_last_bug`)."

Spec §9 explicitly cites:
- **Yang 2025 §4.4.1** (`refs/text/yang2025_hybrid_boolean.txt:605-610`) — Mesh-updating bijectivity-break (Diagnosis B from PR-Y25) as the long-term load-bearing context.
- **Cherchi 2022 §3** (`refs/text/cherchi2022_interactive_robust_mesh_booleans.txt:240-320`) — Two-step pipeline; `remove_winding_insensitive_duplicates` is downstream Render-LOD, outside Cherchi's scope.

The spec is honest about the limit of paper-grounding: no upstream paper section governs Waffle's canonical-key dedup, so PR-Y40's empirical measurement IS the reference (`feedback_external_coherence`).

Commit body honest about cycle position: "9 PR cycles on F0020 Render LOD (Y25/Y26/Y27/Y28 ABORTed; Y36/Y37/Y38 SHIPPED-INFRA; Y39 ABORTed; Y40 SHIPPED-INFRA with 6th-refutation)" and "Production fix on F0020 Render LOD (0 LOC shipped in 9 cycles)". This matches `feedback_no_last_bug` discipline.

---

## §6 Banked findings

No new banked findings beyond those the canary and commit body already enumerate. For completeness, the existing banked items (re-noted here so the audit log captures them):

1. **PR-Y41 primary anchor candidate** — re-canary F.−1 → F.0 (dispatch loop output) to find where the OTHER ~12 D.1d-emitted indices are lost (likely upstream degenerate-vert collapse). Per canary §3.5 / §6, the dominant F.0→F.1 mechanism (53%) is fully-degenerate triangles, not D.1d attribution.
2. **PR-Y41 explicit refutation** — Do NOT ship a source-attribution policy fix based on the N=4 D.1d-loser sample. Canary §3.4 explicitly warns N is too small.
3. **F0045/R0092 retess-pass pathology** — 13011/13368 collisions per invocation in fully-symmetric coplanar-overlap mass-dup. DIFFERENT defect mechanism (banked since PR-Y37; reaffirmed here).
4. **Strategic question for team-lead** — cumulative probe ROI is ~1038 LOC across `tessellation/mod.rs` + `oracle.rs` + `repair.rs`. Continued probe-refinement vs different diagnostic strategy (e.g., direct Cherchi-Rust port repair from PR-Y32 banked) is a strategic call. PR-Y40 does not preempt this; it produces a clean refutation point at which the question can be asked.
5. **Cherchi C++ TBB non-determinism** (PR-Y31 banked, unchanged) — F0020 reruns can still vary by missing-edge count under thread parallelism; use missing-count as gate, not extras.
6. **Brief path discrepancy** — Verification brief referenced `crates/kernel/src/boolean/tessellation/repair.rs`; actual file is `crates/kernel/src/tessellation/repair.rs`. Cosmetic-only; the commit footer is correct (`Spec`/`Canary` paths) and `git diff` resolves the correct file. No audit issue.

---

## §7 Recommendation

**ACCEPT.**

Rationale:
- All ten verification gates pass on independent re-runs from the canary-y36 worktree against live HEAD `57bfe32`.
- Probe is well-scoped, default-off byte-identical, additive-only, 0 LOC production logic.
- The canary's central empirical claim (PR-Y39 §2.5 attribution off by 3-4×) is independently reproducible: F0020 inv006 D.1d-loser count is exactly 4, winners are {196, 198, 199, 233-self}, dominant mechanism is fully-degenerate triangles (10/19 = 53%).
- Cohort independence holds: F0044 = 4, R0045 = 2, F0045/R0092 retess-pass pathology correctly isolated.
- Kernel lib regression test count is byte-identical (1262/24/42); yang_fast = 10/157.
- Spec and canary are paper-grounded (Yang 2025 §4.4.1, Cherchi 2022 §3) with honest scope-limits and explicit no-last-bug language.
- The 6th-refutation framing is disciplined: PR-Y40 narrows the search space without inferring a fix shape from a too-small (N=4) sample (`feedback_phase1_diagnosis_ranking_is_inference`, `feedback_no_last_bug`).

Recommend SHIP-INFRA. The cycle does NOT close Yang. PR-Y41 anchor is correctly re-aimed at the upstream dispatch-loop degenerate emission, with the canary's explicit caveat against shipping a source-attribution fix from the N=4 D.1d signal.

Zero destructive git operations on live tree confirmed.
