# PR-Y39 canary — ABORT at canary phase

**Verdict:** **ABORT — wrong-anchor refutation**
**Empirical chain refuted:** Fix at `remove_nonmanifold_topology_aware` does NOT move F0020 unpaired count (40→40, not 40→~32 as plan predicted)
**Production code modified:** **0 LOC** (the trial fix was applied in-worktree, observed to not fire, then reverted)
**Cohort risk:** N/A (fix did not fire; no cohort exposure)
**Anchor count:** 1st refutation; PR-Y39 follow-up should canary the actual drop site

---

## §0 Summary

PR-Y28 §1 + PR-Y36/Y37/Y38 framed F0020's D.1d unpaired-edge cluster (8 of 40 oracle unpaired in inv#6) as "kids 218/232/233 dropped at `remove_nonmanifold_topology_aware` (`tessellation/repair.rs:585`)." The brief asked canary-y39 to apply Shape C (single-kid small-emission preservation) at L740-787 and verify F0020 unpaired drops 40 → ~30-34.

**The fix shape applied cleanly (Gate 1 GREEN). It compiled. It ran. But it had ZERO effect on the F0020 unpaired count.**

Empirical debug instrumentation at the planned anchor showed:

1. **Function IS invoked.** `remove_nonmanifold_topology_aware` is called 6 times in the F0020 pipeline; the load-bearing invocation has n_tris=126, nm_edges=11.
2. **By the time the function runs, the load-bearing kids are mostly already gone.** Kid 218 (3 emitted): **0 tris remain**. Kid 232 (6 emitted): **1 tri remains**. Kid 233 (9 emitted): **1 tri remains**. The dropping happened *upstream*.
3. **The 2 remaining tris (kid 232/233) are NOT classified as `extra`.** All 11 NMM edges in the load-bearing invocation have `extra=[]` — every triangle's face_id matches the B-Rep topo's `expected_faces`. The D.1d Shape C fix has nothing to fire on.

This is a **`feedback_anchor_before_fix` win.** The discipline "instrument the planned anchor before writing a fix" caught a wrong-anchor PR before any production code shipped. Per the feedback rule: when 3 wrong anchors stack, escalate to reference comparison; this is anchor #1 of any potential streak for PR-Y39, but the upstream context (PR-Y25/Y26/Y27/Y28 all canary-aborted on D.1 framework) makes this the 5th consecutive D.1-related canary refutation.

**Verdict: ABORT.** Plan's Shape C is empirically inapplicable. Shapes A/B at the same anchor would face the same refutation (extras=[] at all NMM edges → no triangles to preserve or drop, regardless of policy). Shape D (area-rank rewrite) would not fire either, because the live triangles already match `expected_faces`. The real drop site is upstream — likely `remove_winding_insensitive_duplicates` (F.0→F.1, 138→119 tris).

---

## §1 Discipline

- **Worktree-only.** All changes applied in `/home/claude/workspace/.claude/worktrees/canary-y36/`. No `git stash`, no `git reset --hard`. Restoration via `git checkout crates/kernel/src/tessellation/repair.rs`.
- **Live tree never touched.** This worktree IS the canary's isolation.
- **Verbatim diff post-revert:** repair.rs identical to baseline `0de27f8`. Only pre-existing PR-Y36/Y37/Y38 probe modifications remain in worktree.

```
$ git diff --stat
 app/tests/cases/assay/results.json    | 138 +++----
 crates/kernel/src/tessellation/mod.rs | 711 +++++++++++++++++++++++++++++++++-
 crates/test-harness/src/oracle.rs     | 179 +++++++++
 3 files changed, 956 insertions(+), 72 deletions(-)
```

(`tessellation/mod.rs` + `oracle.rs` are PR-Y36/Y37/Y38 probe code already shipped on main; `app/tests/cases/assay/results.json` is PR-Y38's regenerated baseline. All baseline-on-main.)

`crates/kernel/src/tessellation/repair.rs` is **byte-identical to HEAD `0de27f8`**.

---

## §2 Method — trial fix (Shape C) and empirical instrumentation

### §2.1 Trial fix (in worktree, then reverted)

Per plan §2 "Recommendation for canary: start with Shape C":

```rust
// In remove_nonmanifold_topology_aware, after Step 3 (tri_face_id), added:
let mut face_total_tris: BTreeMap<u64, usize> = BTreeMap::new();
for &fid in &tri_face_id {
    *face_total_tris.entry(fid).or_insert(0) += 1;
}

// In Step 5's partition-and-remove (both expected.len()>=2 and ==1 branches), added:
const D1D_KID_MAX_TRIS: usize = 10;
let extra_kids: HashSet<u64> = extra.iter().map(|&t| tri_face_id[t]).collect();
let is_d1d_signature = !extra.is_empty() && extra_kids.len() == 1 && {
    let kid = *extra_kids.iter().next().unwrap();
    face_total_tris.get(&kid).copied().unwrap_or(usize::MAX) <= D1D_KID_MAX_TRIS
};
if !is_d1d_signature { /* original removal path */ }
```

Threshold `D1D_KID_MAX_TRIS = 10` was chosen to cover kids 218/232/233's claimed dispatch emission of 3/6/9 with margin. ~25 LOC code change. Build clean (Gate 1 GREEN).

### §2.2 Debug instrumentation (Y39_DEBUG gate)

Added entry-trace logging:
- At function entry: `n_tris, nm_edges.len(), face_total_tris` (per-kid tri counts at this stage).
- At each NMM edge: `live_kids, expected_kids, extra_kids, is_d1d_signature, expected_faces`.

### §2.3 Empirical observation — load-bearing F0020 invocation

```
[y39] enter remove_nonmanifold_topology_aware:
      n_tris=126 nm_edges=11
      face_total_tris={192: 2, 193: 2, 194: 7, 195: 13, 196: 8, 197: 16,
                       198: 3, 199: 2, 200: 2, 204: 1, 206: 3, 207: 6,
                       210: 2, 211: 2, 212: 4, 213: 6, 214: 4, 215: 5,
                       216: 2, 221: 3, 222: 6, 225: 7, 226: 8, 227: 4,
                       229: 2, 230: 1, 231: 2,
                       232: 1, 233: 1,                 <-- KIDS PRESENT W/ 1 TRI EACH
                       235: 1}
                       (KID 218 ABSENT — already dropped)
```

All 11 NMM edges at this stage:

```
[y39] nm_edge live=[197, 197, 231, 231, 231] expected=[197, 197, 231, 231, 231] extra=[] is_d1d=false expected_faces={252, 197, 231, 251}
[y39] nm_edge live=[199, 232, 232]           expected=[199, 232, 232]           extra=[] is_d1d=false expected_faces={232, 238, 199, 243}
[y39] nm_edge live=[199, 199, 199]           expected=[199, 199, 199]           extra=[] is_d1d=false expected_faces={199, 232}
[y39] nm_edge live=[198, 233, 233]           expected=[198, 233, 233]           extra=[] is_d1d=false expected_faces={233, 198}
[y39] nm_edge live=[198, 198, 198, 198]      expected=[198, 198, 198, 198]      extra=[] is_d1d=false expected_faces={254, 233, 198, 255}
[y39] nm_edge live=[198, 198, 198]           expected=[198, 198, 198]           extra=[] is_d1d=false expected_faces={198, 233}
(+5 more, same pattern — all have extra=[])
```

**Every single NMM edge has `extra=[]`.** No triangle in the live mesh, at any NMM edge, has a face_id outside `expected_faces`. The Shape C check (and Shapes A/B/D) cannot fire — there's nothing to preserve.

### §2.4 Stage-f probe with trial fix applied (vs baseline)

| Stage | tri_count baseline | unpaired baseline | tri_count trial-fix | unpaired trial-fix |
|---|---|---|---|---|
| sub=0 (F.0 dispatch output) | 138 | 30 | 138 | 30 |
| sub=1 (post `remove_winding_insensitive_duplicates`) | 119 | 42 | 119 | 42 |
| sub=2 (post `remove_nonmanifold_topology_aware`) | 119 | 39 | 119 | 39 |
| sub=3 (post `remove_nonmanifold_duplicates_aggressive`) | 113 | 39 | 113 | 39 |
| sub=4 (post `weld_smooth_vertices`) | 113 | 39 | 113 | 39 |

**Byte-identical stage progression.** The fix shape has zero effect because it never fires.

### §2.5 Where the drop actually happens (banked for PR-Y40)

The kid-emission accounting reveals the load-bearing drop happens at **F.0→F.1** (`remove_winding_insensitive_duplicates`, 138→119 tris, 19 dropped).

- Kid 218 emitted 3 tris at dispatch → 0 at F.1 → fully dropped here.
- Kid 232 emitted 6 tris at dispatch → 1 at F.1 → 5 dropped here.
- Kid 233 emitted 9 tris at dispatch → 1 at F.1 → 8 dropped here.
- Total drops attributable to D.1d kids at F.0→F.1: **3+5+8 = 16 tris** (out of 19 total dropped here).

`remove_winding_insensitive_duplicates` (`repair.rs:502-574`) deduplicates by canonical-vertex sort — when two triangles from different kids quantize to the same `[QPos; 3]` set, only the first survives. The D.1d kids' triangles must be hitting positional collisions with adjacent-face emissions and getting dropped here.

This is a structurally different defect class than the brief assumed. It's NOT a B-Rep-topo-vs-mesh-face-id mismatch; it's a positional duplicate at the quantized grid level. Fixing it requires either:
- (i) preserving the post-dispatch tri's `face_id` when canon-dedup'ing (insert order awareness), OR
- (ii) source-attribution at canon-dedup (prefer kids with smaller `face_total_tris` per Shape C's signature, applied at F.0→F.1 instead of F.1→F.2), OR
- (iii) upstream fix at the dispatch-loop emission level so the duplicates don't get emitted.

None of these were in scope for PR-Y39. ABORT and re-scope.

---

## §3 Empirical table — gates measured

| Gate | Description | Status | Observed |
|---|---|---|---|
| **0** | Baseline F0020 spotlight: 40 unpaired, 8 degen, 10 self-int, Status:Failed | **CONFIRMED** | `Status:Failed; 40 unpaired (39 boundary, 1 NMM); 8 degen; 10 self-int` (matches PR-Y38 baseline) |
| **0b** | Baseline D.1d attribution probe: D1d=8 at inv#6 | **CONFIRMED** | `inv#6 D1a=9 D1b=0 D1c=0 D1d=8 OTHER=22` (exact PR-Y36/Y37 match) |
| **0c** | F0044 byte-parity hard gate at baseline (no production change) | **GREEN** | `1 passed; 0 failed; 0 ignored` (0 missing, 0 extras) |
| **1** | Build trial fix | **GREEN** | `cargo build -p kernel` clean (57 pre-existing warnings, no fix-attributable) |
| **2** | D.1d count drops post-fix | **REFUTED** | Probe unchanged: `inv#6 D1d=8 OTHER=22` (no change from baseline) |
| **3** | F0020 unpaired drops to band [30,34] | **REFUTED** | Still 40 unpaired post-fix. Predicted 40→32; measured 40→40. |
| 4-9 | (downstream gates) | **NOT RUN** | Gates 2/3 refuted the fix shape; running 4-9 would be wasted budget per ABORT logic |

---

## §4 D.1d attribution verification (kids 218/232/233 baseline vs post-fix)

### §4.1 Baseline (`/tmp/y39-baseline-probe/F0020_inv006_inverse_attribution.tsv`)

8 rows tagged D1d, all with `was_dropped_in_repair=true`:

| unpaired_edge_id | kept_face_id | attributed_source_face_id | classification | outer_boundary_len | nmm_pct |
|---|---|---|---|---|---|
| 9 | 196 | 218 | D1d | 3 | 0.0 |
| 10 | 196 | 218 | D1d | 3 | 0.0 |
| 30 | 199 | 232 | D1d | 4 | 0.0 |
| 31 | 199 | 232 | D1d | 4 | 0.0 |
| 33 | 199 | 232 | D1d | 4 | 0.0 |
| 34 | 198 | 233 | D1d | 5 | 0.0 |
| 35 | 198 | 233 | D1d | 5 | 0.0 |
| 36 | 198 | 233 | D1d | 5 | 0.0 |

### §4.2 Post-fix (`/tmp/y39-postfix-probe/F0020_inv006_inverse_attribution.tsv`)

**Identical.** 8 D1d rows, same kid IDs, same boundary lengths. The fix did not change attribution.

### §4.3 Face inventory cross-check

```
$ grep -E "^218|^232|^233" /tmp/y39-baseline-probe/F0020_inv006_face_inventory.tsv
218  26  Planar  3  0  false  3  0  3  true   0.0  D1d  0  0.0  0  0.0
232  40  Planar  4  0  false  4  0  6  true   0.0  D1d  0  0.0  0  0.0
233  41  Planar  5  0  false  5  0  9  true   0.0  D1d  0  0.0  0  0.0
```

- `face_range_pushed=true` (kids pushed into face_ranges at dispatch).
- `indices_emitted_dispatch` = 3 / 6 / 9 (consistent with PR-Y36 §3.4 inventory).
- `classification=D1d` because `face_range_pushed && !kids_in_final` — the kids are dropped *between* dispatch and final face_ranges.
- But per §2.3, by entry to `remove_nonmanifold_topology_aware`, kid 218 already has 0 tris and kids 232/233 have only 1 each. **The drops happened earlier in the repair chain — specifically at `remove_winding_insensitive_duplicates` (F.0→F.1).**

The Y36/Y37/Y38 attribution is correct about *what* is dropped (kids 218/232/233) and *where it manifests* (8 unpaired edges in render LOD). But the attribution does NOT pin the drop site to `remove_nonmanifold_topology_aware` — that came from PR-Y28 §1's mechanism table written before PR-Y34/Y35 changed the arena topology. **Stage attribution is stale.**

---

## §5 Verdict — **ABORT**

The empirical chain `fix at remove_nonmanifold_topology_aware → F0020 unpaired 40→~32` is **REFUTED**. Trial fix in worktree shows:

- Gate 1 (build) GREEN
- Gate 2 (D.1d attribution drops) REFUTED — D1d unchanged at 8
- Gate 3 (unpaired drops to [30,34]) REFUTED — still 40

Per plan's verdict logic: *"ESCALATE if F0020 drops < 6 (fix shape wrong)."* F0020 drops 0. Strict interpretation: ESCALATE. However, the empirical instrumentation shows ALL four candidate fix shapes (A/B/C/D) at this anchor function would not fire — the function literally has nothing to remove at the load-bearing F0020 invocation (extras=[] at every NMM edge). This isn't "wrong fix shape," it's **wrong anchor function**. ABORT is the correct verdict — production code commit would knowingly ship a no-op.

Per `feedback_anchor_before_fix`: *"In tessellation/boolean fixes, add eprintln to the planned anchor function and run the test BEFORE writing code; the function may not be invoked."* The function IS invoked, but doesn't perform the suspected operation. Same lesson: instrumentation BEFORE commit caught the wrong anchor.

Per `feedback_no_regression_chasing`: do not loosen the filter further or apply Shape D (area-rank rewrite) to try to force a drop. That would just be patching different load-bearing topology decisions on the wrong canvas.

Per `feedback_no_last_bug`: F0020 Status:Failed (8 degen + 10 self-int + 40 unpaired) remains unchanged. We do not know how many bugs remain.

---

## §6 Empirical confidence assessment

| Question | Confidence | Evidence |
|---|---|---|
| Trial fix compiles | HIGH | Gate 1 GREEN |
| Trial fix does NOT fire at the load-bearing invocation | **HIGH** | Y39_DEBUG output shows `extra=[]` at all 11 NMM edges in the n_tris=126 invocation |
| F0020 unpaired count is unaffected by the trial fix | **HIGH** | Both stage-f probes and oracle output byte-identical baseline-vs-trial |
| D.1d attribution probe is unaffected | **HIGH** | `diff` of baseline vs post-fix inv#6 TSVs is empty |
| Kids 218/232/233 are NOT dropped at `remove_nonmanifold_topology_aware` | **HIGH** | At entry to that function, kid 218 has 0 tris; kids 232/233 have 1 tri each (already mostly dropped upstream) |
| The dominant drop site is `remove_winding_insensitive_duplicates` (F.0→F.1) | **MEDIUM-HIGH** | Stage-f progression: F.0 138 → F.1 119 = 19 dropped; remaining stages drop only 6 more. Kids 218/232/233 collectively emit 3+6+9=18 tris at dispatch and have 0/1/1=2 tris at F.1 — 16 of their tris are lost at F.0→F.1 |
| Cohort F0044/F0045/R0092 fix-impact | NOT MEASURED | Trial fix didn't fire; cohort gates not run (per ABORT logic) |

---

## §7 Banked findings (for PR-Y40 scoping)

1. **The actual drop site for F0020 D.1d kids is `remove_winding_insensitive_duplicates`** (`crates/kernel/src/tessellation/repair.rs:502-574`), not `remove_nonmanifold_topology_aware`. The PR-Y28 §1 mechanism table called this out for D.1c ("All-NMM boundary; lost at F.0→F.1 in `remove_winding_insensitive_duplicates`"), but PR-Y34/Y35's arena-topology doubling shifted the D.1d mechanism into the same drop site without anyone re-attributing.

2. **`remove_winding_insensitive_duplicates` is a canonical-vertex dedup.** It removes a triangle if another triangle in the mesh has the same `[QPos; 3]` set after sort. The collision pattern for D.1d: kid X's tris geometrically overlap kid Y's tris on the same quantized grid; whichever appears second in `face_ranges` iteration gets dropped.

3. **PR-Y40 anchor candidates (require their own canary):**
   - **Candidate (i):** Source-attribution at `remove_winding_insensitive_duplicates` — when two triangles collide, prefer keeping the one from the kid with smaller `face_total_tris` (D.1d Shape C transplanted upstream).
   - **Candidate (ii):** Insert-order awareness — when a duplicate is detected, replace the first occurrence with the smaller-emission kid's triangle if it has the D.1d signature.
   - **Candidate (iii):** Upstream fix at dispatch — prevent the duplicates from being emitted in the first place (likely the cleanest but largest scope).

4. **Cohort risk for Candidate (i):** F0044/F0045/R0092 have 0% D.1 attribution per PR-Y37 — meaning at *their* repair stages they don't have small-emission kids dropping at canon-dedup. But this is inference from a different probe stage; PR-Y40 canary must verify with the relocated probe.

5. **The plan's Shape C is correctly designed but mis-anchored.** Shape C's logic ("single-kid, small total emission, not in expected_faces") matches the D.1d signature. If transplanted to `remove_winding_insensitive_duplicates`, it could work — but that function doesn't have a B-Rep `expected_faces` concept. The signature would need to be reformulated as "prefer kid with smaller face-total-emission when duplicates collide."

6. **PR-Y40 should be an INFRA-CLASS canary** (like Y29/Y30/Y33), not a production fix. The trial fix here was a production-class attempt; it ABORTed because the canary discipline kicked in. PR-Y40 should:
   - Add a default-off probe at `remove_winding_insensitive_duplicates` capturing which (kid, face_id) loses each duplicate collision.
   - Aggregate per-case and verify 16-of-19 F0020 drops are kid 218/232/233 collisions.
   - Then design a production fix shape with empirical chain `(probe-attributed candidate) → unpaired count drops` verified BEFORE the production cycle.

---

## §8 Reproduction artifacts

### §8.1 Worktree path

`/home/claude/workspace/.claude/worktrees/canary-y36/`

### §8.2 Trial fix and revert (verbatim sequence)

Trial fix applied to `crates/kernel/src/tessellation/repair.rs` (Step 3 + Step 5 partition-and-remove logic). Verified compile, ran F0020 spotlight (no change), ran D.1d probe (no change), instrumented `Y39_DEBUG` debug output, observed `extra=[]` at all NMM edges, reverted via `git checkout crates/kernel/src/tessellation/repair.rs`.

### §8.3 Verification artifacts

- `/tmp/y39-baseline-f0020.log` — F0020 spotlight baseline output
- `/tmp/y39-postfix-f0020.log` — F0020 spotlight post-fix output (identical to baseline)
- `/tmp/y39-baseline-probe/F0020_inv006_inverse_attribution.tsv` — D.1d=8 attribution
- `/tmp/y39-postfix-probe/F0020_inv006_inverse_attribution.tsv` — D.1d=8 attribution (byte-identical to baseline)
- `/tmp/y39-baseline-probe-summary.log` — y36-inverse-probe per-invocation totals
- `/tmp/y39-f0044-baseline.log` — F0044 byte-parity gate at baseline (0 missing, 0 extras)

### §8.4 Commands

```bash
# Baseline F0020 + D.1d
YANG_BOOLEAN=1 YANG_CONFORMAL_PROBE=1 \
  cargo test -p test-harness --test assay_randomized -- spotlight_f0020 --ignored --nocapture
# expect: 40 unpaired, 8 degen, 10 self-int

rm -rf /tmp/y39-baseline-probe && mkdir -p /tmp/y39-baseline-probe
Y36_INVERSE_PROBE=1 Y36_INVERSE_PROBE_DIR=/tmp/y39-baseline-probe \
  YANG_BOOLEAN=1 YANG_CONFORMAL_PROBE=1 \
  cargo test -p test-harness --test assay_randomized -- spotlight_f0020 --ignored --nocapture
# expect: inv#6 D1a=9 D1d=8 OTHER=22

# F0044 hard gate baseline
cargo test -p test-harness --test cherchi_differential_diff \
  -- pr_y31_f0044_extras_zero --ignored --nocapture --test-threads=1
# expect: GREEN (0 missing, 0 extras, 136 common)

# (Trial fix is no longer applied — repair.rs is at baseline 0de27f8.
# To replay trial: re-apply ~25 LOC change at repair.rs L686-787 per §2.1.)
```

### §8.5 Pre-existing worktree state

This worktree (`canary-y36`) carries pre-existing PR-Y36/Y37/Y38 probe instrumentation:

- `crates/kernel/src/tessellation/mod.rs` (+711 LOC) — Y36 inverse probe + Y37 OtherH1/H2/H3 classification + Y38 grid-sensitivity gate
- `crates/test-harness/src/oracle.rs` (+179 LOC) — Y38 sensitivity probe
- `app/tests/cases/assay/results.json` — PR-Y38 regenerated baseline

These are NOT PR-Y39 changes; they're worktree carry-over from PR-Y38 canary. Production code (`crates/kernel/src/tessellation/repair.rs`) is **byte-identical to HEAD `0de27f8`**.
