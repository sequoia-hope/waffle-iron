# PR-Y40 canary — SHIP-INFRA (6th-refutation framing)

**Verdict:** **SHIP-INFRA — 6th-refutation framing**
**PR-Y39 §4's attribution chain refuted by direct measurement at the empirically-correct anchor.**
**Production code modified:** **0 LOC** (probe is env-gated, default-off byte-identical)
**Probe LOC:** ~155 LOC added to `crates/kernel/src/tessellation/repair.rs` (worktree only)
**Wrong-anchor count this cycle:** N/A — canary is INFRA-class, measures the load-bearing site directly. PR-Y39's BANKED finding (16 D.1d-tris lost at F.0→F.1) is the chain being tested.
**PR-Y41 anchor recommendation:** **PR-Y41 must REVISE its premise.** The F.0→F.1 canonical-key dedup drops only **4 D.1d tris**, not 16. Of those 4, winners are 4 distinct kids (196, 198, 199, 233-self) — fully distributed. The remaining 14 of 18 D.1d-emitted indices (= 6 D.1d tris, of which 2 survive) are lost UPSTREAM of `remove_winding_insensitive_duplicates`, not by it. Re-canary F.−1 → F.0 (post-dispatch raw mesh vs `remove_winding_insensitive_duplicates` entry) to localize.

---

## §0 Summary

PR-Y39 ABORTed at canary phase (commit `2752016`) with a banked claim that *F0020 inv006's F.0→F.1 stage (138→119 tris, 19 dropped) drops 16 tris attributable to D.1d kids 218/232/233* (3+5+8 by §2.5's emission accounting). PR-Y40 was scoped to confirm this attribution at the actual function and answer "WHICH kids win the 16 collisions" so PR-Y41 could design a source-attribution policy.

**The probe REFUTES PR-Y39's attribution chain. The actual count is 4, not 16.**

Empirical findings on F0020 inv006 (the load-bearing invocation, n_tris_input=138, total_collisions=19 — matches the F.0→F.1 19-tri drop bit-for-bit):

| Quantity | PR-Y39 §4 predicted | PR-Y40 measured |
|---|---|---|
| Total collisions at F.0→F.1 (inv006) | 19 (implicit, matches stage-f delta) | **19** ✓ |
| D.1d-loser collisions (kids 218/232/233 as losers) | 16 (3 + 5 + 8) | **4** (1 + 1 + 2) |
| Other-kid losers | implied 3 | **15** |

**Root cause of the off-by-4x:** PR-Y39 §2.3's accounting confused INDICES with TRIANGLES. The Y36 inverse probe reports `indices_emitted_dispatch` (kid 218=3, 232=6, 233=9), and PR-Y39 §4 §2.5 interpreted these as triangle counts. But each triangle = 3 indices, so kids 218/232/233 actually emit **1 + 2 + 3 = 6 triangles** at dispatch, not 18. Of those 6, the probe shows **4 lose** at F.0→F.1 canonical-key dedup; 2 survive (kid 232's 2nd, kid 233's 3rd). This MATCHES Y39 §2.3's downstream observation (kid 218=0/kid 232=1/kid 233=1 at entry to next function) — but the lossage *at this site* is 4, not 16. The other 14 indices (= ~4-5 dispatch-tri losses, depending on which-tri loses) come from a DIFFERENT mechanism (likely degenerate-vert collapse at dispatch, where a single emitted tri has zero indices because all three quantize to the same point).

**Cohort context:** F0044, F0045, R0045, R0092 all show very few collisions in their "topology-extract size" invocations (4 for F0044 load-bearing, 2 for R0045, 0 for all others). One outlier in each of F0045 (inv010, 13011 collisions) and R0092 (inv017, 13368) is a `tessellate_solid_bounded` of the RETESSELLATED final solid pass — symmetric duplicate emission between coplanar overlap regions (kids 19↔20, 25↔26 pairs each losing 5000+ collisions). This is a DIFFERENT defect mechanism (coplanar overlap re-emission, not D.1d signature) and is irrelevant to the F0020 cohort question.

**Verdict: SHIP-INFRA + revised-premise framing.** The probe successfully measured the load-bearing site (Gate 4 quantitative answer), refuted PR-Y39's specific 16-tri attribution, and discovered an off-by-3x accounting bug in PR-Y39 §2.5. PR-Y41 cannot proceed against the now-refuted "16 collisions, who wins" frame; it must first re-canary where the OTHER ~12-14 D.1d-emitted indices are lost (likely UPSTREAM of F.0, in the dispatch loop's per-face triangulation).

Per `feedback_no_last_bug`: this does NOT close Render LOD. F0020 Status:Failed remains (40 unpaired, 8 degen, 10 self-int). Per `feedback_anchor_before_fix`: the probe operating at the empirically-correct anchor function caught an upstream-attribution error before a wasted production cycle. Per `feedback_phase1_diagnosis_ranking_is_inference`: PR-Y39 §2.5's recommendation to "transplant Shape C upstream to F.0→F.1" was based on an inference (the 16-tri count) that was never empirically measured at the actual function; PR-Y40 measured it, and the inference is wrong.

---

## §1 Discipline

- **Worktree-only.** All changes in `/home/claude/workspace/.claude/worktrees/canary-y36/`. No `git stash`, no `git reset --hard`. Production code (`crates/kernel/src/tessellation/repair.rs`) has the probe inserted; this is the canary's isolated worktree.
- **Live tree never touched.** Implementation (Phase 5) will re-apply the diff fresh from this memo onto `main`.
- **Default-off byte parity verified.** Gate 2 baseline + Gate 2 default-off post-probe-insertion produce IDENTICAL `Status:Failed; 40 unpaired (39 boundary, 1 NMM); 8 degen; 10 self-int` AND identical stage-f progression (sub=0/138/30, sub=1/119/42, sub=2/119/39, sub=3/113/39, sub=4/113/39).

### §1.1 Verbatim git diff

```
$ git diff HEAD --stat
 app/tests/cases/assay/results.json       | 144 +++----
 crates/kernel/src/tessellation/mod.rs    | 711 ++++++++++++++++++++++++++++++-
 crates/kernel/src/tessellation/repair.rs | 151 ++++++-
 crates/test-harness/src/oracle.rs        | 179 ++++++++
 4 files changed, 1109 insertions(+), 76 deletions(-)
```

`results.json` + `tessellation/mod.rs` + `oracle.rs` are pre-existing PR-Y36/Y37/Y38 worktree carry-over (not PR-Y40 changes). **PR-Y40's actual production change = `repair.rs` only (+151 lines, all env-gated probe; the production filter `seen.insert(key)` logic is untouched).**

```
$ git diff HEAD -- crates/kernel/src/tessellation/repair.rs | wc -l
180
```

Probe additions:
- `Y40_COLLISION_PROBE` env gate
- `Y40FirstSeen` + `Y40Collision` structs
- Thread-local invocation counter `Y40_INVOCATION_COUNTER`
- `y40_write_collisions` writer (per-invocation: collisions.tsv + histogram.tsv + summary.tsv)
- Single env-gated block inside `remove_winding_insensitive_duplicates` populating `y40_first_seen: HashMap<[QPos;3], Y40FirstSeen>` in parallel to the production `seen: HashSet<[QPos;3]>`

### §1.2 Repair.rs filter preserved verbatim

```rust
// PRODUCTION PATH (untouched):
if seen.insert(key) {
    new_indices.push(indices[base]);
    new_indices.push(indices[base + 1]);
    new_indices.push(indices[base + 2]);
    if y40_enabled { /* record winner */ }
} else if y40_enabled {
    /* record loser */
}
```

The `if seen.insert(key) { keep_tri }` branch is untouched. The `else` branch is empty in the production path — `else if y40_enabled` only runs when the probe is enabled.

---

## §2 Method — collision probe at `remove_winding_insensitive_duplicates`

### §2.1 Probe design

For each candidate triangle:
1. Compute canonical key `[qa, qb, qc].sort()`.
2. `seen.insert(key)`: production behavior (drives keep/drop). UNTOUCHED.
3. **When `Y40_COLLISION_PROBE=1` AND insert returned `true`:** record (key → winner = (face_id.0, range_idx, tri_offset)) in `y40_first_seen`.
4. **When `Y40_COLLISION_PROBE=1` AND insert returned `false`:** look up `y40_first_seen[key]` (the winner) and append `Y40Collision { key, winner, loser }` to `y40_collisions`.
5. After the function's outer loop completes: `y40_write_collisions` (if enabled) emits per-invocation TSVs.

Probe output files (per invocation, in `$Y40_COLLISION_PROBE_DIR`):
- `{case}_inv{NNN}_collisions.tsv` — one row per collision: `collision_idx, key (3 quantized points × 3 coords), winner_face_id, winner_range_idx, winner_tri_off, loser_face_id, loser_range_idx, loser_tri_off`
- `{case}_inv{NNN}_histogram.tsv` — per-(winner_face_id, loser_face_id) pair counts
- `{case}_inv{NNN}_summary.tsv` — invocation, n_tris_input, total_collisions, distinct_winner_face_ids, distinct_loser_face_ids, plus per-loser and per-winner kid histograms

### §2.2 Cohort included

- F0020 (PR-Y39 spotlight; 6 invocations)
- F0044, R0045 (PR-Y37 H1/H2/H3 cohort; load-bearing topology-extract pass)
- F0045, R0092 (also banked PR-Y37 cohort)

R0045 vs F0045 distinction: brief said "F0044, R0045", actual test functions are `spotlight_f0044` (also runs F0045 + R0092 batch internally) and `spotlight_r0045`.

### §2.3 Default-off invariant

When `Y40_COLLISION_PROBE` is unset (or any value other than "1"):
- `y40_enabled = false`
- Both `y40_first_seen.insert(...)` and `y40_collisions.push(...)` blocks are unreachable (guarded by `if y40_enabled`)
- `y40_write_collisions` is unreachable (final `if y40_enabled` guard)
- The HashMap + Vec are allocated but never populated (~0 cost; could be hoisted further behind `if` but kept compact for clarity)

Empirically validated: Gate 2 baseline log vs Gate 2 default-off log produce byte-identical `Status:`, `Detail:`, stage-f, and conformal-probe outputs.

---

## §3 Empirical findings — F0020 inv006 (load-bearing)

### §3.1 Per-invocation summary

| invocation | n_tris_input | total_collisions | distinct_winners | distinct_losers |
|---|---|---|---|---|
| 1 | 12 | 1 | 1 | 1 |
| 2 | 12 | 1 | 1 | 1 |
| 3 | 60 | 8 | 2 | 2 |
| 4 | 60 | 8 | 2 | 2 |
| 5 | 12 | 1 | 1 | 1 |
| **6** | **138** | **19** | **9** | **9** |

Invocations 1, 2, 5 are tiny single-operand pre-passes; 3, 4 are duplicate intermediate calls; **inv006 is the F.0→F.1 boolean-result repair pass** (n_tris=138 byte-matches stage-f sub=0 of the load-bearing pipeline run). 19 collisions byte-matches the 138→119 drop.

### §3.2 inv006 collision-loser distribution

```
loser_face_id  count
212            1
218            1   ← D.1d (kid 218)
227            2   (kid 227 self-collisions)
229            1
231            1
232            1   ← D.1d (kid 232)
233            2   ← D.1d (kid 233 — one cross-face, one self)
235            6   (kid 235 self-collisions, all fully-degenerate (single point))
256            4   (kid 256 ← kid 235 cross-face, all fully-degenerate)
TOTAL          19
```

### §3.3 D.1d kid attribution (the LOAD-BEARING question)

| Kid | indices_emitted | tris dispatched (= indices/3) | tris lost as colliders (F.0→F.1) | tris winning (F.0→F.1) | tris surviving F.1 (predicted: dispatched - lost) |
|---|---|---|---|---|---|
| 218 | 3 | 1 | 1 | 0 | 0 |
| 232 | 6 | 2 | 1 | 0 | 1 |
| 233 | 9 | 3 | 2 | 1 (self-col, row 8) | 1 |
| **TOTAL D.1d** | **18** | **6** | **4** | **1** | **2** |

`tris surviving F.1` matches PR-Y39 §2.3's downstream observation (kid 218=0, kid 232=1, kid 233=1 at entry to `remove_nonmanifold_topology_aware`). The PR-Y40 measurement is INTERNALLY CONSISTENT with PR-Y39's downstream count — but reveals PR-Y39 §2.5's tri-vs-index conflation.

### §3.4 D.1d-loser collision winner-kid histogram

| Winner kid | D.1d loser collisions won | % of 4 D.1d-loser collisions |
|---|---|---|
| 196 | 1 | 25.0% |
| 198 | 1 | 25.0% |
| 199 | 1 | 25.0% |
| 233 (self) | 1 | 25.0% |
| **Total** | **4** | **100%** |

**Distributed pattern — 4 distinct winner kids each with 1 collision, including 1 self-collision within kid 233.**

By the verdict logic in plan §2 ("concentrated if top-3 winners ≥80%; distributed if ≥10 different kids"): 4 distinct winners is between the two, leaning distributed. With kid 233 self-collision excluded as intra-kid (not a "different kid"), the cross-kid winners are exactly 3 (196, 198, 199), each at 1/3 = 33% — which would qualify as "concentrated" by a permissive reading.

**BUT THE SAMPLE IS TOO SMALL TO BE LOAD-BEARING.** N=4 D.1d-loser events. A "source-attribution policy preferring smaller `face_total_tris` kids" cannot be designed from N=4 observations across only 3 distinct cross-kid winners (196, 198, 199). PR-Y41 cannot ship a fix based on this signal alone.

### §3.5 Non-D.1d collisions (the OTHER 15 of 19)

The other 15 collisions at inv006 break down as:
- **6 kid-235 self-collisions** with key `(65051,-15817,-36086, 65051,-15817,-36086, 65051,-15817,-36086)` — **fully degenerate** (all three vertices identical). Kid 235 has 7 tris but 6 are degenerate-point collapses.
- **4 cross-collisions 235→256** with the same fully-degenerate key. Kid 256 also emits 4 degenerate tris that collide with kid 235's first degenerate tri.
- **2 kid-227 self-collisions** (rows 2, 3) — partially degenerate, last two vertices coincide.
- **1 kid 226→229** (row 4) — partially degenerate.
- **1 kid 195→212** (row 0) — fully distinct vertices.
- **1 kid 197→231** (row 5) — partially degenerate, first two vertices coincide.

**The DOMINANT mechanism at F.0→F.1 (10 of 19 = 53%) is fully-degenerate triangles** (zero-area collapses of three coincident grid points emitted by upstream dispatch). The D.1d 4-collision attribution is a SECONDARY pattern (21%), and the remaining 5 collisions are partially-degenerate or fully-distinct one-offs.

The fully-degenerate cluster (kids 235, 256 at key (65051,-15817,-36086)) suggests the dispatch loop is emitting many tris where all 3 vertices quantize to the same point — these are degenerate emissions, NOT positional duplicates between different geometric triangles. Source: likely a planar-face dispatch path for cylinder caps or boss tops where the tessellation collapses to a single corner.

---

## §4 Cohort findings (Gate 6)

### §4.1 Cohort summary table

| Case | invocations | total_collisions (max) | total_collisions (sum) | smallest n_tris_input with collisions | D.1d signature present? |
|---|---|---|---|---|---|
| F0044 | 3 (load-bearing topo pass + 2 small) | 4 | 4 | 120 (load-bearing) | No — 2 distinct winners (19, 20) vs 2 losers (21, 22), 2-collision symmetric pairs |
| F0045 | 10 (including a 13535-tri retess pass) | 13011 | 13011 | 13535 | No — 6 distinct kids in symmetric coplanar overlap mass-dup |
| R0045 | 3 | 2 | 2 | 608 | No — single pair 476→477, 2-collisions (axis-aligned mirror?) |
| R0092 | 17 (including a 13692-tri retess pass) | 13368 | 13368 | 13692 | No — same pattern as F0045 retess |

### §4.2 F0044 cohort observation (most relevant to PR-Y31's GREEN-extras case)

F0044 has 0 missing-from-Cherchi triangles (PR-Y31 baseline). Its `remove_winding_insensitive_duplicates` load-bearing invocation has 4 collisions, in 2 symmetric pairs:
- 19 → 21 (×2)
- 20 → 22 (×2)

This is a low-volume, symmetric pattern. Not the D.1d signature (which would be 1-tri-emitter kids losing to many-tri-emitter kids). Consistent with PR-Y37's H1/H2/H3 finding that F0044 cohort is 0% D.1.

### §4.3 F0045 + R0092 retess pass observation

The 13011 / 13368 collision counts are PATHOLOGICAL — these come from a `tessellate_solid_bounded` invocation on the FULL retessellated solid mesh (n_tris=13535 / 13692). Looking at F0045_inv010_summary:

```
loser_face_id  count
20             5118
22             1295
23             1252
25             5342
26             2

winner_face_id  count
19             2
20             5118  ← winner=loser for same face
22             1295  ← winner=loser
23             1253
24             2
25             5341  ← winner=loser
```

The pattern is **(N-1)-self-collision** within each face — kid 20 has ~5119 tris (massive face, e.g., a large planar boss), of which the first survives and 5118 lose to it. This means kid 20 is emitting 5119 tris that all quantize to the SAME canonical key. **This is the same fully-degenerate signature as F0020's kid 235 — but at massive scale.** The mechanism: a giant planar face is being tessellated into many sub-triangles, but when the boolean pipeline retesselates the result for `Render LOD`, all sub-triangles project to a single quantized cell (because the face is huge and the grid quantum is large relative to subdivision).

This is **not D.1d**, not the "small-emitter loses to large-emitter" signature, and not relevant to F0020 — it's a DIFFERENT defect (Render-LOD over-coarse quantization, or retessellation emitting too many degenerate sub-tris). Banked as a separate signal; not the focus of PR-Y40.

---

## §5 Empirical table — gates measured

| Gate | Description | Status | Observed |
|---|---|---|---|
| **1** | Build with probe | **GREEN** | `cargo build -p kernel` clean (57 pre-existing warnings; no probe-attributable). |
| **2** | F0020 default-off byte parity | **GREEN** | `Status:Failed; 40 unpaired (39 boundary, 1 NMM); 8 degen; 10 self-int` IDENTICAL to PR-Y38/Y39 baseline. Stage-f progression byte-identical (138→119→119→113→113; unpaired 30→42→39→39→39). |
| **3** | Probe fires (F0020) | **GREEN** | 6 invocations × 3 files = 18 TSV files in `/tmp/y40-probe`. inv006 has 19 collisions byte-matching stage-f F.0→F.1 delta of 138-119. |
| **4** | F0020 16-collision attribution | **REFUTED (LOAD-BEARING FINDING)** | inv006 D.1d-loser count = 4 (kid 218=1, kid 232=1, kid 233=2), NOT 16. PR-Y39 §2.5's accounting confused INDICES (3+6+9=18) with TRIANGLES (1+2+3=6). 4 of 6 D.1d tris lose at F.0→F.1; 2 survive. Survival count consistent with PR-Y39 §2.3 downstream observation. |
| **5** | F0020 winner-kid distribution | **DISTRIBUTED-SMALL** | 4 D.1d-loser collisions split across 4 distinct winners (196, 198, 199, 233-self) — each at 25%. Sample too small to bottom out concentrated-vs-distributed strongly; cross-kid winners are exactly 3 (196, 198, 199) each at 33%, but N=4 cannot ground a source-attribution policy. |
| **6** | Cohort F0044/F0045/R0045/R0092 | **VERIFIED** | F0044 load-bearing: 4 collisions (2 symmetric pairs, NOT D.1d). R0045: 2 collisions (single pair). F0045 + R0092 each have one giant retess-pass invocation (13011/13368 collisions) — DIFFERENT defect (fully-degenerate Render LOD), banked. No D.1d signature in cohort. |
| **7** | kernel lib regression | **GREEN** | `1262 passed; 24 failed; 42 ignored` — IDENTICAL to baseline. |
| **8** | yang_fast corpus | **GREEN** | `10/157 passed, 139 failed, 8 errored (skipped 33)` — matches banked baseline. |

---

## §6 PR-Y41 anchor recommendation

### §6.1 Recommendation: PR-Y41 cannot proceed on the PR-Y39 §7 banked frame as-is

The PR-Y39 §7 banked candidates were:
- **(i) Source-attribution at canon-dedup** preferring smaller `face_total_tris` kids
- **(ii) Insert-order awareness** swapping first occurrence with smaller-emission
- **(iii) Upstream dispatch-loop fix** preventing duplicate emission

PR-Y40's measurement at the SAME anchor function shows:
- The "16-collision" frame is wrong. There are 4 D.1d-loser collisions, not 16.
- Of those 4: 1 is intra-kid (kid 233 self-collision); the other 3 cross-kid winners (196, 198, 199) have face-total-tri counts 8, 8, 2. Two are LARGER than D.1d losers; one (199, 2 tris) is the SAME size as kid 232 (2 tris).
- The fix shape "prefer smaller `face_total_tris`" would fire on 2-3 collisions but lacks a clean signal (kid 199 with 2 tris is not a "large predator" of kid 232 with 2 tris).
- **The dominant mechanism at F.0→F.1 (10/19 = 53%) is fully-degenerate triangles, not D.1d.** A source-attribution policy here cannot reduce F0020 unpaired count meaningfully (and probably also cannot at all without targeting the degenerate-emission root cause upstream).

### §6.2 Banked PR-Y41 candidates (require their own canary)

1. **(n) Post-dispatch degenerate-triangle filter (UPSTREAM of `remove_winding_insensitive_duplicates`).** Detect "all three quantized vertices coincide" at dispatch exit (F.−1) and drop. Would eliminate the 10/19 fully-degenerate collisions at inv006. UNPROVEN to reduce unpaired count (these degenerate tris being dropped vs kept by canon-dedup probably doesn't change the final mesh boundary, since they don't contribute valid edges). Run a canary first to verify.

2. **(o) UPSTREAM lossage canary — where the 14 D.1d-emitted indices go.** D.1d kids emit 18 indices (6 tris) at dispatch; 4 lose at F.0→F.1; 2 survive. The other 14 indices' fate is unknown. PR-Y41 canary: stage-h instrumentation between dispatch exit (F.−1) and `remove_winding_insensitive_duplicates` entry (F.0). May reveal `remove_duplicate_triangles` (winding-sensitive) or `remove_degenerate_triangles` as the upstream drop site. Worktree-only; no production fix.

3. **(p) Re-examine PR-Y39 §2.3 face_total_tris dict.** The dict at entry to `remove_nonmanifold_topology_aware` shows D.1d kids' triangle counts. PR-Y40 probe confirms 2 survive — but face_total_tris is reported per-kid AFTER prior repair passes. Cross-reference the F.0 (= entry-to-our-function) face_ranges; sum tri counts per kid by `(range.end_index - range.start_index) / 3`. Tells us EXACTLY how many tris each kid contributes BEFORE canon-dedup, validating the indices-vs-tris correction.

### §6.3 Bottom line

PR-Y40 ships INFRA only. **PR-Y41 must be scoped as ANOTHER canary** (recommendation 2 above), targeting the UPSTREAM site (F.−1 → F.0) where D.1d's other 14 indices (= 4-5 tris) are lost. The probe-output here is the empirical reference for any future "source-attribution at canon-dedup" proposal.

---

## §7 Verdict — **SHIP-INFRA + 6th-refutation framing**

By the plan's verdict logic:
> **SHIP-INFRA + 6th-refutation framing** if Gate 4's collision count ≠ 16. PR-Y39's attribution itself was wrong; need to re-canary the stage-f progression more carefully.

Gate 4 measured 4 (NOT 16). **6th-refutation framing applies.**

The probe is sound (Gates 1/2/3/7/8 all GREEN; Gate 6 cohort validates expected low-D.1d-signal pattern). The refutation is of PR-Y39 §2.5's INTERPRETATION of its own Y36-inverse-probe data, not of the Y36 probe or of the F.0→F.1 site itself.

Per `feedback_anchor_before_fix`: empirical instrumentation at the planned anchor caught the wrong attribution before a production cycle. Per `feedback_validate_against_corpus`: Gate 6 confirms the cohort has no D.1d signature (consistent with PR-Y37), so PR-Y41 should not extend a D.1d-targeted fix to cohort cases. Per `feedback_no_last_bug`: F0020 Status:Failed unchanged; we do not know how many bugs remain.

Strategic context: **5 consecutive D.1-related canary-stage ABORTs** (Y25/Y26/Y27/Y28/Y39) + **3 INFRA SHIPs** (Y36/Y37/Y38) + **PR-Y40 INFRA SHIP** with a 6th-refutation finding. Per `feedback_anchor_before_fix`'s escalation rule (3 wrong anchors → reference comparison), the discipline has already paid off — PR-Y29 built the Cherchi differential-diff harness in response. PR-Y41's correct anchor is now PROBABLY upstream of `remove_winding_insensitive_duplicates`, not at it. Continuing infra investment at empirically-correct sites is the disciplined response.

---

## §8 Empirical confidence assessment

| Question | Confidence | Evidence |
|---|---|---|
| Probe operates at the load-bearing site (F0020 inv006, n_tris=138) | **HIGH** | n_tris_input=138 byte-matches stage-f sub=0; total_collisions=19 byte-matches sub=0→sub=1 delta of 138→119 |
| Default-off byte parity preserved | **HIGH** | Gates 2/7/8 produce IDENTICAL baseline observations including stage-f progression |
| D.1d-loser collision count at F.0→F.1 = 4 | **HIGH** | Direct measurement; collisions.tsv inv006 rows 1, 6, 7, 8 with loser_face_id ∈ {218, 232, 233} |
| PR-Y39 §2.5 had indices-vs-tris conflation | **HIGH** | indices_emitted (18 = 3+6+9) vs measured tris-emitted (6 = 1+2+3) — Y36 face_inventory.tsv `indices_emitted_dispatch` field is INDICES (verified at mod.rs:4984 `end_index - start_index`) |
| 4 of 6 D.1d tris lose at F.0→F.1 (rest survive) | **HIGH** | Per-kid breakdown matches PR-Y39 §2.3 downstream observation (218→0, 232→1, 233→1 surviving) |
| Cohort F0044/F0045/R0045/R0092 has no D.1d signature at F.0→F.1 | **HIGH** | F0044 load-bearing 4 collisions in symmetric pairs (19↔21, 20↔22); R0045 2 collisions (476→477); 0 in most invocations |
| Fully-degenerate collisions dominate F0020 inv006 | **HIGH** | 10 of 19 (53%) have collision key with all three coords identical (rows 9-18) |
| F0045/R0092 retess passes (13K-tri pathology) are a DIFFERENT defect | **MEDIUM-HIGH** | Symmetric per-face N-self-collisions on huge faces suggests Render-LOD overdraw; banked but unproven separate mechanism |
| PR-Y41 source-attribution-at-canon-dedup will fix F0020 | **LOW** (refuted by data) | 4 D.1d-loser collisions are not enough to ground a policy; the OTHER 12-14 D.1d-emitted indices are lost UPSTREAM |
| The UPSTREAM site (F.−1 → F.0) is the load-bearing one for D.1d | **MEDIUM** | Implied by accounting: 6 tris emitted, 4 lost here, 2 survive; but probe was at F.0→F.1, not at F.−1 |

---

## §9 Reproduction artifacts

### §9.1 Worktree path

`/home/claude/workspace/.claude/worktrees/canary-y36/`

### §9.2 Verification artifacts

- `/tmp/y40-baseline.log` — F0020 spotlight baseline (pre-probe-insertion)
- `/tmp/y40-defaultoff.log` — F0020 spotlight default-off post-probe-insertion (byte-identical to baseline)
- `/tmp/y40-probe/F0020_inv006_collisions.tsv` — 19-row collision log (LOAD-BEARING)
- `/tmp/y40-probe/F0020_inv006_histogram.tsv` — pair (winner_kid, loser_kid) counts
- `/tmp/y40-probe/F0020_inv006_summary.tsv` — per-loser and per-winner histograms
- `/tmp/y40-probe/F0020_inv00{1-5}_*.tsv` — non-load-bearing pre-passes
- `/tmp/y40-cohort/*` — F0044, F0045, R0045, R0092 cohort probe output

### §9.3 Commands

```bash
# Gate 2 + 3: Default-off byte parity + probe fire on F0020
rm -rf /tmp/y40-probe && mkdir -p /tmp/y40-probe
Y40_COLLISION_PROBE=1 Y40_COLLISION_PROBE_DIR=/tmp/y40-probe \
  YANG_BOOLEAN=1 YANG_CONFORMAL_PROBE=1 \
  cargo test -p test-harness --test assay_randomized -- spotlight_f0020 --ignored --nocapture
# expect: Status:Failed; 40 unpaired; 18 TSV files in /tmp/y40-probe

# Gate 4: D.1d attribution verification
cat /tmp/y40-probe/F0020_inv006_summary.tsv
# expect: total_collisions=19; loser kid 218=1, kid 232=1, kid 233=2 (D.1d total=4)

# Gate 5: Winner kid histogram for D.1d losers
awk -F'\t' '$14 ~ /^(218|232|233)$/ {print $11}' /tmp/y40-probe/F0020_inv006_collisions.tsv | sort | uniq -c
# expect: 1× 196, 1× 198, 1× 199, 1× 233 (self)

# Gate 6: Cohort
rm -rf /tmp/y40-cohort && mkdir -p /tmp/y40-cohort
Y40_COLLISION_PROBE=1 Y40_COLLISION_PROBE_DIR=/tmp/y40-cohort \
  YANG_BOOLEAN=1 YANG_CONFORMAL_PROBE=1 \
  cargo test -p test-harness --test assay_randomized -- spotlight_f0044 spotlight_r0045 --ignored --nocapture
ls /tmp/y40-cohort/*_summary.tsv | while read f; do \
  total=$(grep "^total_collisions" $f | awk '{print $2}'); \
  ntris=$(grep "^n_tris_input" $f | awk '{print $2}'); \
  echo "$f total=$total ntris=$ntris"; done
# expect: F0044 load-bearing inv003 has 4 collisions; F0045/R0092 small except for one large retess invocation

# Gate 7: kernel lib regression
cargo test -p kernel --lib
# expect: 1262 passed, 24 failed, 42 ignored

# Gate 8: yang_fast
YANG_BOOLEAN=1 cargo test -p test-harness --test assay_randomized -- yang_fast --ignored --nocapture --test-threads=1
# expect: 10/157
```

### §9.4 Pre-existing worktree state

This worktree (`canary-y36`) carries pre-existing PR-Y36/Y37/Y38 probe instrumentation (already shipped on main; carry-over within the worktree):

- `crates/kernel/src/tessellation/mod.rs` (+711 LOC, NOT a PR-Y40 change)
- `crates/test-harness/src/oracle.rs` (+179 LOC, NOT a PR-Y40 change)
- `app/tests/cases/assay/results.json` (PR-Y38 regenerated baseline, NOT a PR-Y40 change)

**PR-Y40's only production change**: `crates/kernel/src/tessellation/repair.rs` (+151 LOC, all env-gated probe code).
