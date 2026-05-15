# PR-Y43 adversary — independent re-run + scrutiny

**Verdict:** **ACCEPT-PROVISIONAL** — load-bearing histogram (Case B=14, Case C=0, Case D dominant) byte-confirmed across 4 independent reruns. Case A and Case D counts shift with Cherchi non-det mode (42 vs 47); canary's load-bearing claim (4/14/0/24 at 42-mode) reproduced exactly in 1/4 of my runs. Two factual defects in canary memo flagged: (1) the "5 distinct off-vertex positions account for 11 of 14 Case B entries" claim in §4.2 is wrong (actual: **10 distinct**, of which **3 distinct positions account for 7 of 14**, remaining 7 are unique); (2) the canary's §6.2 framing that Case D dominantly means "3-of-3 verts at 1×" is logically inferred but **not directly measured** by the probe (the code's Case D includes a 2nd sub-class with match_at_1x ∈ {0,1} AND match_at_5x==2). Both defects are framing / accounting issues, not methodology / probe-correctness defects; gates remain GREEN; verdict still SHIP-INFRA + D-dominant. PR-Y44 anchor scope should be tightened per §5.

---

## §1 Mandate + worktree state

**Mandate** (per teammate-message): independent re-run of PR-Y43 canary memo claims byte-for-byte. No destructive git. Read-only verification — no production code or harness modification.

**Worktree state:**
- Path: `/home/claude/workspace/.claude/worktrees/canary-y36/`
- Branch: `worktree-canary-y36`
- HEAD: `b0009bd5e06511d3e3f55575ed111bb03869196a` (PR-Y42 SHIP-INFRA commit)
- Uncommitted (mirror of impl-y43 `f335efc` content):
  - `M app/tests/cases/assay/results.json` (regenerated artifact; 71/71 — was 70/70 in canary §1.3 due to one extra spotlight rerun on my side)
  - `M crates/test-harness/tests/cherchi_differential_diff.rs` (1520 lines; +438 over base 1082)
  - `?? docs/audits/pr_y43_canary.md`
  - `?? specs/yang_pr_y43_nearest_attribution.md`
- `git log --all --oneline | head -1` confirms `f335efc infra(yang-pr-y43): ...` exists in main reflog (impl-y43's commit).
- `wc -l` on harness = **1520** (canary §1.5 exact match).
- `git diff HEAD --numstat` on harness shows **438 0** insertions/deletions (canary §1.3 exact match).
- Cherchi sidecar binary present: `/home/claude/cherchi2022/InteractiveAndRobustMeshBooleans/build/mesh_booleans` (used for Gates C/D/E/F/H).

Independent re-run confirms the diff stat and harness line count claims in canary §1.2-§1.5 exactly.

---

## §2 8-gate independent results

All gates run from this worktree, no canary log re-use. Logs at `/tmp/adversary-y43-*.log`.

| Gate | Description | Expected (canary) | Observed (adversary) | Status |
|---|---|---|---|---|
| **A** | `cargo build -p test-harness --test cherchi_differential_diff` | Clean; 58 pre-existing kernel warnings | Clean; finished in 0.04s; 58 kernel + 1 slvs pre-existing warnings | **GREEN** |
| **B** | F0020 spotlight default-off byte parity | `Status:Failed; 40 unpaired (39 boundary, 1 NMM); 8 of 113 degen; 10 self-int` + `[stage-f] 138→119→119→113→113 + unpaired 30→42→39→39→39` | EXACT match: 40 unpaired (39 boundary, 1 non-manifold); 8 of 113 degen; 10 self-int. `[stage-f]` trace byte-identical | **GREEN** |
| **C1** | F0020 A/B/C/D run 1 | 4/14/0/24 at target=42 (75%) OR 7/14/0/26 at target=47 (25%) | target=**47**; A=7, B=14, C=0, D=26 | GREEN (47-mode) |
| **C2** | F0020 A/B/C/D run 2 | (idem) | target=**47**; A=7, B=14, C=0, D=26 | GREEN (47-mode) |
| **C3** | F0020 A/B/C/D run 3 | (idem) | target=**42**; A=4, B=14, C=0, D=24 ← **canary's load-bearing claim reproduced exactly** | GREEN (42-mode) |
| **C4** | F0020 A/B/C/D run 4 | (idem) | target=**47**; A=7, B=14, C=0, D=26 | GREEN (47-mode) |
| **C-aggregate** | Cherchi non-det mode mix | canary §3.3 reported 3/4 runs at 42-mode (75% / 25%) | Adversary saw **1/4 at 42-mode** (25% / 75%) — non-det split is wider than canary stated, though the **load-bearing invariants HOLD** in all modes | GREEN with caveat |
| **D** | F0020 Case B 14-entry table | Specific (off_idx, C_pos, W_pos, cell_dist) per canary §4 | All 14 entries byte-match canary §4 in 42-mode run (R3). Spot-checked b[0] cell_dist=12,661 ✓, b[1] cell_dist=1,238 ✓, b[3] cell_dist=12,793 ✓, b[9] cell_dist=815 ✓, b[13] cell_dist=6,884 ✓ | **GREEN** |
| **E** | Cohort F0044 / F0045 / R0092 | F0044 target=16, B=8 (50%), D=8 (50%); F0045 target=4, B=2, D=2; R0092 target=0 | F0044 target=16, A=0, B=8 (50.0%), C=0, D=8 (50.0%); F0045 target=4, A=0, B=2 (50.0%), C=0, D=2 (50.0%); R0092 target=0 (vacuous all-zero) | **GREEN** |
| **F** | PR-Y42 baseline `f0020_render_lod_diff_baseline` | common=36, attribution 20/40 = 50.0%, target_tris=42; missing 194 (or 201 off-mode) | common=36, attribution 20/40 = 50.0%, target_tris=42, missing=194, extras=76 | **GREEN** |
| **G1** | `cargo test -p kernel --lib` | 1262 / 24 / 42 | 1262 / 24 / 42 — IDENTICAL | **GREEN** |
| **G2** | `YANG_BOOLEAN=1 yang_fast` | 10/157 | 10/157 passed, 139 failed, 8 errored (skipped 33 known timeouts) | **GREEN** |
| **H** | PR-Y31 hard gate `pr_y31_f0044_extras_zero` | F0044 Stage B `missing=0, extras=0, common=136` | F0044 Stage B `missing=0, extras=0, common=136`; well_formed=true, χ=4 | **GREEN** |

**8/8 gates GREEN** (counting C-aggregate as one gate). Canary's load-bearing histogram (4/14/0/24 at target=42) reproduced byte-exact in 1 of 4 adversary reruns. Canary's secondary mode (7/14/0/26 at target=47) reproduced in 3 of 4 adversary reruns. **Load-bearing invariants (Case B=14, Case C=0, D-dominant in BOTH modes) hold across all 4 adversary runs.**

### §2.1 Cherchi non-det mode split — adversary observation diverges from canary

Canary §3.3 reports 4 reruns: 1 in 47-mode, 3 in 42-mode (75% 42-mode). Canary §8 banked observation #4 calls the split "~75/25" in favor of 42-mode.

Adversary 4 reruns: 1 in 42-mode, 3 in 47-mode (25% 42-mode). **My sample contradicts the canary's claim that 42-mode is the dominant rerun mode.** Combined evidence (8 reruns: canary 3+1, adversary 1+3) gives 4 of 8 in 42-mode (50/50) — closer to "two stable modes both reachable" than canary's "~75/25 toward 42-mode."

**This is NOT a verdict-blocking finding** because:
1. The load-bearing claims (Case B=14, Case C=0, D dominates) hold in both modes.
2. Cherchi TBB non-det is banked from PR-Y31; canary explicitly addresses both modes.
3. The numerical histogram changes (4→7 for A; 24→26 for D) but the verdict-threshold logic doesn't flip (D still ≥40% in both modes; C still 0%).

**But:** canary §10 verification commands say "expect: A=4 (9.5%), B=14 (33.3%), C=0 (0.0%), D=24 (57.1%) at 42-mode" without flagging that 42-mode may be a minority outcome. PR-Y44 should pin Cherchi tighter (`TBB_NUM_THREADS=1` did not suffice in either canary's or adversary's runs) or use `target_tris=42` and `Case B=14` as the deterministic gate.

---

## §3 Code review findings (+438 LOC harness extension)

Code reviewed at `crates/test-harness/tests/cherchi_differential_diff.rs:1082-1520`.

### §3.1 GREEN — what's correct

1. **f32 round-trip parity preserved**: `build_waffle_vert_sets_at_grids` (L1126-1148), `cherchi_vert_matches_waffle_at_grids` (L1157-1174), `nearest_waffle_vert_at_base_grid` (L1180-1211), and the inline Cherchi-side requantization in `run_nearest_attribution_for_case` (L1390-1392) all use the **identical** pattern as production `oracle_quantize_waffle_obj` (L731-779) and `oracle_quantize_cherchi_vert` (L785-792): `((v as f32) as f64 * inv_grid).round() as i64`. No drift between probe and production oracle.

2. **Multi-grid quantization implemented per spec**: factors at L1130 = `[1u32, 2, 5, 10]`; cell size at L1132 = `base_grid * factor` (e.g., 1×=base, 2×=2×base, ...). Uses `.round()` (NOT `.floor()`) per spec §3.3 prose. **Correct.**

3. **Filter to 42 target tris uses identical edge-traversal logic to PR-Y42** (L1346-1358). Re-uses the same `unpaired_edges` set (HashSet of `OraclePosEdge`) and the same `oracle_quantize_cherchi_vert` lossy-1e-6-grid path. F0020 reproduces 42 target tris byte-exactly in 42-mode (47 in 47-mode), confirming the filter is identical to PR-Y42's.

4. **Off-vertex selection (L1400-1408) is deterministic and well-guarded**: when `match_at_1x == 2`, exactly 1 of the 3 verts is unmatched-at-1×, so `unmatched_at_1x.len() == 1` and `off_idx = Some(that_index)`. When `match_at_1x != 2`, `off_idx = None`. The Case B branch at L1421 dispatches only on `cls == "B"` AND `off_idx.is_some()` (via `if let Some(off) = off_idx`). No ambiguity, no fallthrough.

5. **Cherchi C++ data parsed from the correct file** (L1307): `parse_obj(&path_cherchi_out)` where `path_cherchi_out` comes from `dumps.workdir.join(format!("{}_cherchi_{}.obj", case_id.to_ascii_lowercase(), op_str))`. The op-string is from `read_first_boolean_op(case_id)` → matches PR-Y31's op-plumb fix. F0020 is `union`, F0044 is `subtraction`, etc.

6. **Classification logic correctness check** (L1215-1225):
   - A (sub-grid drift): `match_at_5x == 3 ∧ match_at_1x < 3` — fires when all 3 verts AT looser-but-not-tightest grid.
   - B (partial match): `match_at_1x == 2` — exactly 2-at-tightest-grid (and not A; so `match_at_5x ∈ {2}` since matches are monotone in grid scale and A would've fired at `match_at_5x == 3`).
   - C (no proximity): `match_at_5x ≤ 1` (and not A, B; so `match_at_1x ≤ 1`).
   - D (residual): catch-all.
   - **Priority is mutually exclusive and exhaustive.** No silent overlap.

### §3.2 RED / framing flaws in canary memo (NOT in code)

**Defect 1 (canary §4.2 / §6.4)**: the claim "5 distinct off-vertex positions account for 11 of 14 Case B entries" is **incorrect**.

I parsed the Case B dump from my R3 log (42-mode) AND from canary's own `/tmp/y43-f0020-attribution.log`:

```
$ grep "b\[" /tmp/adversary-y43-f0020-r3.log | sed -E 's/.*C_pos=([^ ]+).*/\1/' | sort | uniq -c | sort -rn
      3 (+2.151060e-1,-1.095520e-1,-1.135960e-1)    ← b[9-11] share this off-vert
      2 (-2.471870e-1,+1.040060e-1,-2.269840e-1)    ← b[1-2] share this off-vert
      2 (+1.501730e-1,-1.660400e-1,+2.210230e-1)    ← b[6-7] share this off-vert
      1 (+2.995890e-1,-1.236960e-1,+9.937600e-2)
      1 (+2.749190e-1,-9.921200e-2,+2.045370e-1)
      1 (-2.749190e-1,+9.921200e-2,+1.052630e-1)
      1 (+2.041150e-1,-1.699610e-1,-1.053620e-1)
      1 (+1.421790e-1,-1.221610e-1,+2.744700e-2)
      1 (-1.421790e-1,+1.221610e-1,-1.232230e-1)
      1 (+1.421790e-1,-1.221610e-1,-1.208200e-1)
```

**Actual: 10 distinct off-vertex positions, of which 3 distinct positions account for 7 of 14 entries; the remaining 7 entries each have a unique position.** Canary's claim of "5 positions / 11 entries" is over-reduced by 2× — the canary §4.2 narrative correctly identifies the 3 shared groups (b[9-11], b[1-2], b[6-7]) but then incorrectly aggregates "b[4] near b[8]" as a fifth share (they are merely close, not at the same position — canary §4.2's own prose says "near" not "same"). The summary in §4.2's bullet ("5 distinct off-vertex positions account for 11 of 14 Case B entries") is a misstatement.

**Impact**: §6.4 builds on this and says "5 distinct positions" again. PR-Y44 secondary anchor is described as compact ("~5 positions rather than 14 triangles") in §6.4 and §9.1 banked item #2. The TRUE compactness is 10 positions, not 5 — **PR-Y44 anchor data is 2× larger than canary states**. Still a reasonably compact data set for investigation, but the claim should be corrected.

**Defect 2 (canary §6.2)**: the framing "for 24 of the 42 missing-from-Waffle Cherchi triangles, ALL THREE of their vertex positions appear somewhere in Waffle's Render LOD vertex set at the base grid" is logically inferred, **not measured**.

Case D = `(NOT A) ∧ (NOT B) ∧ (NOT C)` = the residual after A, B, C fail. Tracing the priority predicate:
- A fires when `match_at_5x == 3 ∧ match_at_1x < 3`.
- B fires when `match_at_1x == 2` AND NOT A (so `match_at_5x < 3`; since matches are monotone, `match_at_5x == 2`).
- C fires when `match_at_5x ≤ 1`.
- D = NOT(A,B,C) = `(match_at_1x ∈ {0,1,3}) ∧ (match_at_5x ≥ 2) ∧ ¬(match_at_5x == 3 ∧ match_at_1x < 3)`.

D's possible match_at_1x values:
- **match_at_1x = 3** → all 3 verts present at every grid level → D fires. ← canary's claimed "3-of-3 at 1×" sub-class.
- **match_at_1x = 0** with match_at_5x = 2 → D fires. (NOT C because match_at_5x > 1.)
- **match_at_1x = 1** with match_at_5x = 2 → D fires. (NOT B because match_at_1x ≠ 2.)
- **match_at_1x = 0** with match_at_5x = 3 → A fires (NOT D).
- **match_at_1x = 1** with match_at_5x = 3 → A fires (NOT D).

**So Case D has at least two distinct sub-mechanisms.** The probe does NOT print per-Case-D match counts. The canary's "all 24 are 3-of-3 at 1×" framing is an unverified inference. PR-Y44 anchor design depends on whether D is dominated by sub-class (a) — vertex positions all present, only the triangle missing → topology/indexing/winding bug — or sub-class (b) — 2-of-3 verts at 5× but not at 1× → quantization-noise interaction with edge-pair geometry.

**Impact**: the canary §7.4 candidate (α) "investigate F.0's `remove_winding_insensitive_duplicates`" implicitly assumes sub-class (a) dominates. If Case D is mixed (e.g., 12 sub-class (a) + 12 sub-class (b)), then PR-Y44 anchor (α) only addresses half of Case D. The canary should have either:
- Modified the probe to emit per-Case-D `(match_at_1x, match_at_5x)` distribution, OR
- Hedged §6.2 / §6.3 more strongly: "Case D includes 3-of-3 at 1× (canary inference) and 0-or-1-at-1× + 2-at-5× (residual catch-all); proportions unmeasured."

The §8.2 open question #1 ("F0020 Case D is '3-of-3 at 1× but tri missing'; cohort Case D may include '1-or-2 at 1× + 1-or-2 at 5×'") implicitly acknowledges sub-class (b) exists in cohort but assumes it doesn't exist in F0020. This is an unverified asymmetry assumption.

### §3.3 Minor — non-blocking observations

3. **`run_nearest_attribution_for_case` calls `oracle_quantize_waffle_obj` twice** (once at L1322 to get `base_grid` and `unpaired_edges`, and again indirectly via `build_waffle_vert_sets_at_grids` at L1368 which re-quantizes). Both calls use the same f32 round-trip, so result is identical. Minor perf nit; not a correctness issue.

4. **`case_b_dumps` sort** at L1471-1472 sorts by Cherchi off-vert position (`.partial_cmp` on `[f64; 3]`). Stable order — but uses `Ordering::Equal` fallback for NaN, which is fine since all positions are finite f64.

5. **`pct` div-by-zero guard** at L1441 (`n = target_tris.len().max(1)`) prevents NaN on cohort R0092 (target_tris=0). Correctly emits `0.0%` for all four cases on R0092.

6. **`#[allow(dead_code)]` on `NearestVertAttribution` fields** at L1106 and `NearestAttributionResult` at L1230 — acceptable since fields are inspected via `Debug` and the struct is intentionally a record type for future asserts. Acknowledged in the canary §6 Gate 1.

### §3.4 Case A scrutiny — adversary verifies "why so small (9.5%)?"

Adversary's R3 (42-mode) has 4 Case A entries. These are triangles where all 3 verts quantize to keys present in Waffle at 5× grid (27 µm cell) but at least one vert is NOT-present at 1× grid (5.4 µm). With PR-Y38's confirmed grid stability under 1e-5, you'd expect Case A ≈ 0; the 4 entries deserve scrutiny.

The probe doesn't dump per-Case-A specifics. A reasonable adversarial inference (consistent with what the canary's Case A definition implies) is that these 4 represent vertices Cherchi computed near-but-not-at a Waffle vertex within 27 µm (about 5× the 5.4 µm base grid). Note: Cherchi's input mesh was already f32-quantized via the OBJ round-trip dump in PR-Y29, so any sub-grid drift is in the post-arrangement subdivision step. **Plausible** that Case A entries are sub-grid-stable from Waffle's side but subdivided to slightly different positions by Cherchi.

Not enough data here to conclude "Case A is zero / four / contaminated." Adversary accepts Case A=4 as a small but real residual; does not contest the canary's framing of Case A as "sub-grid drift."

---

## §4 Verdict — **ACCEPT-PROVISIONAL**

### §4.1 What's accepted

- **All 8 gates GREEN** (with caveat on adversary's non-det mode split).
- **Load-bearing histogram invariants** hold across 4 adversary reruns:
  - Case B count = 14 (BYTE-STABLE).
  - Case C count = 0 (BYTE-STABLE).
  - Case D dominant (24/42 = 57.1% in 42-mode, 26/47 = 55.3% in 47-mode; ≥40% in both).
  - D-dominant verdict holds in both modes.
- **Production code unchanged** (canary §1.2 verified at 0 LOC; 438 LOC isolated to test-harness file). Default-off byte parity preserved.
- **Cherchi C++ reference oracle continues to function** (Gate F, H pass byte-clean; F0044 Stage B remains 0/0/136).
- **kernel + yang_fast baselines preserved** (1262/24/42 + 10/157 byte-identical).

### §4.2 What's provisional (the canary memo should be patched, not the production code)

1. **Defect 1**: `5 positions / 11 entries` claim in canary §4.2, §6.4, §9.1 is wrong. **Correct is `10 positions / 7-shared+7-unique`.**
2. **Defect 2**: Canary §6.2 framing "for 24 ... ALL THREE of their vertex positions appear" is unverified inference, not measurement. Case D includes at least two sub-mechanisms that the probe does not distinguish.
3. Cherchi non-det 42-mode/47-mode split: canary's "~75/25 toward 42-mode" is contradicted by adversary's "1/4 toward 42-mode." Combined 8-rerun evidence suggests "50/50 with both modes stable."

**None of these defects invalidate the SHIP-INFRA + D-dominant verdict** — the verdict logic does not depend on the corrected numbers (Case D is still ≥40% in both modes; Case B count is still 14; Case C is still 0). They DO impact PR-Y44 anchor design (see §5).

### §4.3 Recommendation for memo patching

Audit-y43 or close-out should patch canary memo as follows:
- **§4.2**: replace "5 distinct off-vertex positions account for 11 of 14" with "**3 distinct off-vertex positions are shared by 7 of 14** Case B entries (b[1-2], b[6-7], b[9-11]); the remaining 7 entries each have a unique off-vertex; **10 distinct positions total**."
- **§6.2**: insert hedge "the dominant Case D sub-mechanism (3-of-3 at 1×; vertex positions all present, triangle missing) is **inferred from the priority-ordered classification, not directly measured**. Case D also includes a residual catch-all sub-class with `match_at_1x ∈ {0,1}` AND `match_at_5x == 2`. PR-Y44 should bisect."
- **§9.1 banked item #2**: correct the off-vert count from "5" to "10."
- **§3.3**: correct the per-rerun stability table's mode-mix narrative from "~75/25 toward 42-mode" to "split observed 50/50 across 8 combined canary+adversary reruns."

---

## §5 PR-Y44 anchor stress-test

### §5.1 Does the canary's (α)/(β)/(γ) ranking hold?

**Candidate (α)** — `remove_winding_insensitive_duplicates` at F.0 (drops 19 tris). Canary says PRIMARY.

Adversary scrutiny:
- The "F.0 drops 19 + F.3 drops 6 ≈ Case D = 24/26" count-coincidence is at most **suggestive**, as the canary §7.3 hedge says. The 19+6=25 is a TOTAL drop count across the ENTIRE F-stage pipeline, not specifically at unpaired-edge positions.
- The 24 (or 26) Case D triangles are a SUBSET of the 194 (or 201) missing-from-Waffle Cherchi triangles. The F.0/F.3 removals are at the Waffle side — these are TRIANGLES WAFFLE DROPS, not missing-from-Waffle Cherchi triangles. The semantic mapping is "Waffle's F.0 dropped a triangle whose 3 quantized vertices match a Cherchi triangle that's still in Cherchi's mesh — so the Cherchi triangle becomes a 'missing-from-Waffle' at the diff."
- **For (α) to be the load-bearing PR-Y44 anchor**, you'd need to verify: of the 19 triangles F.0 drops, how many have all-3 verts matching Cherchi-only-missing triangles? Canary admits this is "PR-Y44 canary must bisect."

**Candidate (β)** — `remove_nonmanifold_duplicates_aggressive` at F.3 (drops 6 tris). Canary says SECONDARY.

Same caveat as (α) — count-coincidence only.

**Candidate (γ)** — Pre-F.0 Boolean LOD → Render LOD re-tessellation (drops ~108 tris from 246 to 138). Canary says TERTIARY.

Adversary observation: **γ has the largest tri-drop magnitude** (108 vs 19 vs 6). If the Case D sub-class (a) (3-of-3 at 1× / pure topology defect) is the dominant mechanism for Case D, the proportional likelihood is that γ explains MORE of Case D than α + β combined, not less. The canary's ranking puts γ tertiary because "it's at a pre-F.0 layer banked as an investigation target without localizing." This reasoning is weak — the 108-drop magnitude argues (γ) is at least co-equal with (α).

**Adversary's recommended PR-Y44 anchor ranking:**
- **PR-Y44 Phase 1 must be a probe extension**, not a fix: extend `f0020_render_lod_nearest_attribution` to emit per-Case-D `(match_at_1x, match_at_2x, match_at_5x, match_at_10x)` tuples for all 24 (or 26) Case D entries. This separates sub-class (a) from sub-class (b) empirically. Without this, anchor selection is inference, not measurement.
- **Conditional on the sub-class split**:
  - If sub-class (a) ≥80% of Case D → anchor candidates (α)/(β)/(γ) by tri-removal magnitude (γ first by argument above; α/β second by paper-citation alignment).
  - If sub-class (b) ≥40% of Case D → anchor shifts to vertex-production (similar to Case B) for the 0-or-1-at-1× sub-class.
  - If mixed (e.g., 60/40) → two-pronged investigation.

### §5.2 What I would change about the canary memo's PR-Y44 framing

1. **Demote (α) from PRIMARY to "co-equal with (γ)"** — the count-coincidence argument is too weak to rank α above γ when γ has 5.7× the tri-drop magnitude.
2. **Make PR-Y44 Phase 1 a sub-class measurement probe**, not a fix attempt. The current memo §6.3 candidates are presented as fix anchors; they should be presented as "candidates contingent on Phase 1 sub-class measurement."
3. **Reframe §6.5 "refutes Option C pause" more cautiously**. Canary says Case C = 0 refutes "defect is upstream of Render LOD." But the Case D sub-class (b) (`match_at_1x ∈ {0,1}` AND `match_at_5x == 2`) IS a partial-proximity defect — closer to Case C than Case A. If sub-class (b) is non-trivial in Case D, **part of the F0020 defect is still upstream of Render LOD** (or at least its Boolean LOD → Render LOD upgrade). Option C is not as cleanly refuted as the canary §6.5 claims.

### §5.3 What I do not change about the canary's verdict

- **SHIP-INFRA** stands. INFRA-class PRs without production code changes are intrinsically low-risk; the 8 gates verify default-off byte parity. ACCEPT.
- **D-dominant outcome** stands. Even with sub-class (b) potentially diluting "3-of-3 at 1×", Case D is the dominant *bucket* and a real signal worth a PR-Y44 investigation.
- **PR-Y44 anchor = Render LOD layer (or just-pre-)** stands as a coarse claim. Sub-stage attribution (α vs β vs γ) needs Phase 1 measurement.

---

## §6 Open / banked

### §6.1 Banked for audit-y43

1. **Patch canary memo defects 1 + 2** per §4.3. These should be corrected before PR-Y43 is closed out, not just papered over in adversary's memo.
2. **Patch canary §3.3 non-det mode mix narrative** from "~75/25 toward 42-mode" to combined 50/50.
3. **PR-Y44 plan should start with a probe extension** (per §5.1) not a fix attempt. The current PR-Y44 candidate ranking should be soft-stated.

### §6.2 Open for PR-Y44

1. **Sub-class (a) vs (b) split of Case D**: extend probe to emit per-triangle match-count tuples. Adversary estimate (unverified): if F0020 follows the cohort pattern (which has dense vertex sets even with `common=0`), sub-class (a) likely dominates Case D, but the proportion needs measurement.
2. **Cherchi non-det 42/47 mode pinning**: PR-Y44 should pin Cherchi tighter; `TBB_NUM_THREADS=1` did not produce determinism in canary's OR adversary's runs.
3. **The "108 pre-F.0 tris dropped" cascade** is canary's (γ) candidate. Adversary argues this is at least co-equal with (α). PR-Y44 should bisect this layer first or in parallel with F.0/F.3.

### §6.3 Adversary process notes

- **Read-only verification preserved**: no production code or harness code modified throughout this session.
- **Worktree-only**: no `git stash` / `git checkout --` / `git reset` used.
- **Independent re-run**: all 4 F0020 attribution runs executed from worktree HEAD; canary's `/tmp/y43-*.log` files cross-referenced (Gate D, §3.2 defect 1) but not used as primary evidence.
- **Cherchi non-det characterized** across 4 fresh runs; canary's mode-mix claim is the only directly-contradicted point but is not verdict-blocking.
- **Code review** caught two framing/accounting defects in the canary memo (not in the harness code). Probe correctness is **GREEN**.

---

## §7 Verdict — final

**ACCEPT-PROVISIONAL.** Ship PR-Y43 INFRA. Two canary memo factual defects (5-vs-10 off-vertex count; Case D sub-class inference) should be corrected by audit-y43 or close-out; both are framing issues, not probe-correctness defects. All 8 gates GREEN. Load-bearing invariants byte-stable across 4 adversary reruns. PR-Y44 anchor (canary's α primary) is over-confident; adversary recommends Phase 1 probe extension to measure Case D sub-class split before anchor selection.

The strategic-pivot ROI claim in canary §7.8 ("paying off for F0020 ... 90% explainable as Case D + Case B = 90%") is **still supportable** even with defects corrected:
- Case D ≥ 55% (in both 42-mode and 47-mode) → primary cluster.
- Case B = 14 = 29.8% to 33.3% → secondary cluster.
- Combined 85–90% of attributable tris in one of two specific buckets — this is real progress and refutes Option C *coarsely* (the defect IS at the Render LOD layer / boundary, just with more sub-mechanism complexity than canary states).

Adversary signs off.
