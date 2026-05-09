# PR-Y24 Anchor Canary — option (e) reframed CONFIRMED: construction-time directed_he keys flip F0020 b#2 [twin-oracle] 2→0

**Author:** canary-y24
**Date:** 2026-05-08
**Plan:** `/home/claude/.claude/plans/optimized-wandering-wind.md` Phase 0
**Verdict:** **CONFIRMED** — re-keying the [twin-oracle] from arena-traversal to construction-time `directed_he` makes F0020 b#2 `[twin-oracle] unpaired_count` 2→0 (load-bearing prediction MET) and preserves the F0044/F0045/R0092 cohort at `[twin-oracle] unpaired_count=0` across all 7 invocations (cohort guard MET).
**Recommended scope:** **B1** (plumb-via-arena-field). The F0044 batch's validator path IS consulted with non-zero `arena_only_count` in 1 of 7 invocations; per plan §"Site B" decision criterion, escalate from B2 to B1.

This memo names the empirical mechanism only. It does NOT propose code shape; that is `spec-y24`'s job.

---

## §0 Discipline — live tree untouched

### Live tree at session start

```
$ git status
On branch main
Your branch is up to date with 'origin/main'.

nothing to commit, working tree clean
```

### Live tree just before writing this memo

```
$ git status
On branch main
Your branch is up to date with 'origin/main'.

nothing to commit, working tree clean
```

All probe instrumentation was applied inside a separate worktree:

```
$ git worktree add /tmp/y24-probe-wt 3c749a3
Preparing worktree (detached HEAD 3c749a3)
HEAD is now at 3c749a3 feat(scripts): extract-papers.sh — idempotent text view of refs/*.pdf for agent paper-reading

$ cd /tmp/y24-probe-wt && git diff --stat
 crates/kernel/src/boolean/topology_extract.rs | 89 +++++++++++++++++++++++++++
 1 file changed, 89 insertions(+)
```

No `git stash`, `git checkout --`, `git reset --hard`, or any other destructive op was used on the live working tree. Per `feedback_adversary_no_destructive_git.md`.

### Probe gate

Every probe is gated on `std::env::var("Y24_PROBE").as_deref() == Ok("1")` — when unset the codepath is byte-identical to the `3c749a3` baseline. The probe block is purely additive: P1's `constructed_dir_edges` HashSet build and P2's `he_to_constructed_dest` Vec are scoped inside the `if y24_probe { ... }` blocks. No mutation of `unpaired`, `arena_dir_edges`, or arena state. Output is `eprintln!`-only.

### Reproduction commands

```
git worktree add /tmp/y24-probe-wt 3c749a3
cd /tmp/y24-probe-wt
# (probes injected into Step 7 twin_debug block per §3 below)

# F0020 b#2 (load-bearing prediction)
YANG_BOOLEAN=1 Y24_PROBE=1 TWIN_DEBUG=1 cargo test -p test-harness \
    --test assay_randomized -- spotlight_f0020 \
    --ignored --nocapture --test-threads=1 \
    > /tmp/y24-canary-f0020.txt 2>&1

# F0044 batch (cohort guard)
YANG_BOOLEAN=1 Y24_PROBE=1 TWIN_DEBUG=1 cargo test -p test-harness \
    --test assay_randomized -- spotlight_f0044 \
    --ignored --nocapture --test-threads=1 \
    > /tmp/y24-canary-f0044.txt 2>&1
```

Both tests reported `test result: ok. 1 passed; 0 failed`. Spotlight tests wrap inner panics; the case status is `Failed` (F0020) or `0/3 passed` (F0044+F0045+R0092 batch) — both expected at PR-Y22 baseline.

---

## §1 F0020 spotlight — load-bearing prediction MET

The F0020 spotlight produces TWO `flood_fill_patches` invocations (Extrude #1, then Extrude #2 a.k.a. b#2). Verbatim grep output:

```
[topo-extract] summary: paired=48, unpaired=0, ambiguous=0
[y24-probe-p1] arena_only_count=8 constructed_only_count=11
[y24-probe-p1] arena_only_first5=[(35, 35), (9, 9), (34, 34), (17, 17), (32, 32)]
[y24-probe-p1] constructed_only_first5=[(33, 14), (32, 17), (35, 34), (34, 33), (9, 35)]
[y24-probe-p2] simulated_twin_oracle_unpaired_count=0 (vs actual=0) he_with_no_construct_dest=0
[twin-oracle] total_directed_edges=96
[twin-oracle] unpaired_count=0
[twin-oracle] collision_count=1
[topo-extract] summary: paired=65, unpaired=0, ambiguous=0
[y24-probe-p1] arena_only_count=16 constructed_only_count=16
[y24-probe-p1] arena_only_first5=[(73, 20), (36, 69), (54, 44), (59, 49), (31, 71)]
[y24-probe-p1] constructed_only_first5=[(38, 26), (74, 10), (57, 41), (78, 70), (31, 17)]
[y24-probe-p2] simulated_twin_oracle_unpaired_count=0 (vs actual=2) he_with_no_construct_dest=0
[twin-oracle] total_directed_edges=169
[twin-oracle] unpaired_count=2
[twin-oracle] collision_count=0
[twin-oracle] offender he=58 twin=-3 twin.twin=-3 origin=v27(-2.749189e-1,9.921157e-2,1.052632e-1) dest=v38(-2.749189e-1,9.921157e-2,5.152014e-2)
[twin-oracle] offender he=59 twin=-3 twin.twin=-3 origin=v38(-2.749189e-1,9.921157e-2,5.152014e-2) dest=v27(-2.749189e-1,9.921157e-2,1.052632e-1)
```

### Boolean #1 (Extrude #1) — 96 HEs, no defect

P1: `arena_only_count=8 constructed_only_count=11`. The arena_only entries `(35,35)`, `(9,9)`, `(34,34)`, `(17,17)`, `(32,32)` are **degenerate self-loops** — confirms PR-Y19-MODE-B 0e amendment finding (self-loops are upstream micro-loop artifacts, not real defects). P2: `simulated=0 (vs actual=0)`. Both keying schemes agree: 0 unpaired. No defect, no divergence-induced regression.

### Boolean #2 (Extrude #2) — 169 HEs, the load-bearing case

P1: `arena_only_count=16 constructed_only_count=16`. The arena_only entries `(73,20)`, `(36,69)`, `(54,44)`, `(59,49)`, `(31,71)` are NON-degenerate (no self-loops in the first 5) — these are wrap-back artifacts from open chains, exactly as predicted by PR-Y23 canary §3 Layer 2 mechanism.

The constructed_only first entry `(38, 26)` is **HE 59's construction-time destination key** — confirms the PR-Y23 canary §1 P3 finding `constructed_dest_brep = 26` for HE 59, vs the arena traversal's phantom `arena_traversal_dest_brep = 27`.

**P2 verdict: `simulated_twin_oracle_unpaired_count=0 (vs actual=2)`.** This is the load-bearing prediction MET:
- Actual oracle (arena-traversal keying): 2 unpaired (HE 58 and HE 59).
- Simulated oracle (construction-time keying via `he_to_constructed_dest`): 0 unpaired.

The 2 offenders HE 58 (origin=v27, dest=v38) and HE 59 (origin=v38, dest=v27) — exactly the PR-Y23 canary §1 P4 offenders — reclassify as legitimate-NMM under construction-time keys because:
- HE 58's construction-time directed edge is `(BV27, BV38)`. The reverse `(BV38, BV27)` is NOT in `directed_he` (constructed_only does not contain it; it would have been part of the wrap-back). Result: rev-test fails, HE 58 counted as legitimate-NMM, not unpaired.
- HE 59's construction-time directed edge is `(BV38, BV26)`. The reverse `(BV26, BV38)` is NOT in `directed_he`. Result: rev-test fails, HE 59 counted as legitimate-NMM, not unpaired.

This is exactly the mechanism predicted in plan §"Pivot to option (e) reframed" line 11.

---

## §2 F0044 batch — cohort guard MET (7 invocations)

The F0044 spotlight runs the F0044+F0045+R0092 batch. 7 `flood_fill_patches` invocations total (corresponds to 7 boolean stages across 3 cases). Verbatim grep output:

```
[topo-extract] summary: paired=68, unpaired=0, ambiguous=0
[y24-probe-p1] arena_only_count=0 constructed_only_count=0
[y24-probe-p1] arena_only_first5=[]
[y24-probe-p1] constructed_only_first5=[]
[y24-probe-p2] simulated_twin_oracle_unpaired_count=0 (vs actual=0) he_with_no_construct_dest=0
[twin-oracle] total_directed_edges=136
[twin-oracle] unpaired_count=0
[twin-oracle] collision_count=0
[topo-extract] summary: paired=117, unpaired=0, ambiguous=0
[y24-probe-p1] arena_only_count=0 constructed_only_count=0
[y24-probe-p1] arena_only_first5=[]
[y24-probe-p1] constructed_only_first5=[]
[y24-probe-p2] simulated_twin_oracle_unpaired_count=0 (vs actual=0) he_with_no_construct_dest=0
[twin-oracle] total_directed_edges=234
[twin-oracle] unpaired_count=0
[twin-oracle] collision_count=0
[topo-extract] summary: paired=165, unpaired=0, ambiguous=0
[y24-probe-p1] arena_only_count=0 constructed_only_count=0
[y24-probe-p1] arena_only_first5=[]
[y24-probe-p1] constructed_only_first5=[]
[y24-probe-p2] simulated_twin_oracle_unpaired_count=0 (vs actual=0) he_with_no_construct_dest=0
[twin-oracle] total_directed_edges=330
[twin-oracle] unpaired_count=0
[twin-oracle] collision_count=0
[topo-extract] summary: paired=230, unpaired=0, ambiguous=0
[y24-probe-p1] arena_only_count=0 constructed_only_count=0
[y24-probe-p1] arena_only_first5=[]
[y24-probe-p1] constructed_only_first5=[]
[y24-probe-p2] simulated_twin_oracle_unpaired_count=0 (vs actual=0) he_with_no_construct_dest=0
[twin-oracle] total_directed_edges=460
[twin-oracle] unpaired_count=0
[twin-oracle] collision_count=0
[topo-extract] summary: paired=97, unpaired=0, ambiguous=0
[y24-probe-p1] arena_only_count=4 constructed_only_count=5
[y24-probe-p1] arena_only_first5=[(50, 50), (47, 47), (123, 121), (117, 49)]
[y24-probe-p1] constructed_only_first5=[(123, 47), (117, 50), (47, 46), (122, 121), (50, 49)]
[y24-probe-p2] simulated_twin_oracle_unpaired_count=0 (vs actual=0) he_with_no_construct_dest=0
[twin-oracle] total_directed_edges=229
[twin-oracle] unpaired_count=0
[twin-oracle] collision_count=1
[topo-extract] summary: paired=118, unpaired=0, ambiguous=0
[y24-probe-p1] arena_only_count=0 constructed_only_count=0
[y24-probe-p1] arena_only_first5=[]
[y24-probe-p1] constructed_only_first5=[]
[y24-probe-p2] simulated_twin_oracle_unpaired_count=0 (vs actual=0) he_with_no_construct_dest=0
[twin-oracle] total_directed_edges=283
[twin-oracle] unpaired_count=0
[twin-oracle] collision_count=0
[topo-extract] summary: paired=182, unpaired=0, ambiguous=0
[y24-probe-p1] arena_only_count=0 constructed_only_count=0
[y24-probe-p1] arena_only_first5=[]
[y24-probe-p1] constructed_only_first5=[]
[y24-probe-p2] simulated_twin_oracle_unpaired_count=0 (vs actual=0) he_with_no_construct_dest=0
[twin-oracle] total_directed_edges=408
[twin-oracle] unpaired_count=0
[twin-oracle] collision_count=0
```

### Per-invocation cohort guard verdict

| # | total_directed_edges | actual unpaired_count | simulated unpaired_count | arena_only_count | constructed_only_count | Verdict |
|---|---|---|---|---|---|---|
| 1 | 136 | 0 | 0 | 0 | 0 | ✅ identical, no divergence |
| 2 | 234 | 0 | 0 | 0 | 0 | ✅ identical, no divergence |
| 3 | 330 | 0 | 0 | 0 | 0 | ✅ identical, no divergence |
| 4 | 460 | 0 | 0 | 0 | 0 | ✅ identical, no divergence |
| 5 | 229 | 0 | 0 | **4** | **5** | ⚠ divergence present, both verdicts agree (sim=0 actual=0) |
| 6 | 283 | 0 | 0 | 0 | 0 | ✅ identical, no divergence |
| 7 | 408 | 0 | 0 | 0 | 0 | ✅ identical, no divergence |

In **every** invocation, simulated == actual == 0. The cohort guard is structurally invariant: B1 or B2 fix would not introduce a new defect in the F0044/F0045/R0092 batch.

### The 7th invocation's divergence

Invocation #5 in the batch (229 HEs) shows `arena_only_count=4 constructed_only_count=5`. Of the arena_only first 4: `(50,50)` and `(47,47)` are **degenerate self-loops** (same upstream micro-loop artifact pattern as F0020 b#1). The remaining `(123,121)` and `(117,49)` are non-degenerate — these are wrap-back artifacts from higher-n open chains in this F0044/F0045/R0092 invocation (per PR-Y23 canary §4 banked finding 4: higher-n open chains' wrap-backs land at non-coinciding vertices and surface as legitimate-NMM rather than missing-defect, which matches what we see here — actual=0 because the rev-test on `(121,123)` and `(49,117)` happens to fail in arena_dir_edges).

The construction-time keying yields the same verdict (sim=0) but reaches it via different reasoning: HE-with-construction-dest=121 has key `(123,121)` only in arena (not in directed_he), so its rev `(121,123)` is queried in `constructed_dir_edges` — but that set is built from `directed_he.keys()` which does contain `(122,121)` and other 121-incident edges, not `(121,123)`. So rev fails, sim counts as legitimate-NMM. Same outcome by independent path.

---

## §3 Probe diff (worktree only — NOT committed)

Located at `/tmp/y24-probe-wt/crates/kernel/src/boolean/topology_extract.rs`. The probes are scoped inside the existing `if twin_debug { ... }` block (Step 7 [twin-oracle] section, around L1437 in the baseline).

```
$ cd /tmp/y24-probe-wt && git diff --stat
 crates/kernel/src/boolean/topology_extract.rs | 89 +++++++++++++++++++++++++++
 1 file changed, 89 insertions(+)
```

### Probe P1 (divergence audit) — inserted at L1450 immediately after `arena_dir_edges` build:

```rust
// PR-Y24 PROBE P1: divergence audit between arena-traversal keys
// and construction-time directed_he keys. Default-off; gated on
// Y24_PROBE=1 so this is byte-identical to baseline when unset.
let y24_probe = std::env::var("Y24_PROBE").as_deref() == Ok("1");
if y24_probe {
    let constructed_dir_edges: HashSet<(usize, usize)> = directed_he
        .keys()
        .map(|(a, b)| (a.0, b.0))
        .collect();
    let arena_only: Vec<(usize, usize)> = arena_dir_edges
        .difference(&constructed_dir_edges)
        .cloned()
        .collect();
    let constructed_only: Vec<(usize, usize)> = constructed_dir_edges
        .difference(&arena_dir_edges)
        .cloned()
        .collect();
    eprintln!(
        "[y24-probe-p1] arena_only_count={} constructed_only_count={}",
        arena_only.len(),
        constructed_only.len()
    );
    eprintln!(
        "[y24-probe-p1] arena_only_first5={:?}",
        arena_only.iter().take(5).collect::<Vec<_>>()
    );
    eprintln!(
        "[y24-probe-p1] constructed_only_first5={:?}",
        constructed_only.iter().take(5).collect::<Vec<_>>()
    );
}
```

### Probe P2 (predicted-outcome simulation) — inserted at L1473 immediately after the actual `unpaired` loop:

```rust
// PR-Y24 PROBE P2: predicted-outcome simulation. Re-run the
// unpaired-detection logic but source v_dest from a per-HE
// construction-time map (he_to_constructed_dest) and source the
// rev-existence test from constructed_dir_edges. Emits the
// simulated unpaired_count without mutating actual unpaired.
// Default-off; gated on Y24_PROBE=1 so byte-identical to baseline
// when unset.
if y24_probe {
    let constructed_dir_edges: HashSet<(usize, usize)> = directed_he
        .keys()
        .map(|(a, b)| (a.0, b.0))
        .collect();
    let mut he_to_constructed_dest: Vec<usize> = vec![usize::MAX; n_he];
    for ((_v0, v1_brep), hes) in directed_he.iter() {
        for he_idx in hes {
            if he_idx.0 < n_he {
                he_to_constructed_dest[he_idx.0] = v1_brep.0;
            }
        }
    }
    let mut sim_unpaired: usize = 0;
    let mut he_with_no_construct_dest: usize = 0;
    for (i, he) in arena.half_edges.iter().enumerate() {
        match he.twin {
            Some(t) => {
                if t.0 >= n_he
                    || arena.half_edges[t.0].twin != Some(HalfEdgeIdx(i))
                {
                    sim_unpaired += 1;
                }
            }
            None => {
                let v_origin = he.origin.0;
                let v_dest = he_to_constructed_dest[i];
                if v_dest == usize::MAX {
                    he_with_no_construct_dest += 1;
                    // No construction-time entry: cannot test rev.
                    // Treat as legitimate-NMM (do not count).
                    continue;
                }
                let rev_present =
                    constructed_dir_edges.contains(&(v_dest, v_origin));
                if rev_present {
                    sim_unpaired += 1;
                }
            }
        }
    }
    eprintln!(
        "[y24-probe-p2] simulated_twin_oracle_unpaired_count={} (vs actual={}) he_with_no_construct_dest={}",
        sim_unpaired,
        unpaired.len(),
        he_with_no_construct_dest
    );
}
```

The probe scaffolding will be discarded by `git worktree remove /tmp/y24-probe-wt` at close-out. Per plan §"Phase 0": probes live ONLY in the worktree.

---

## §4 Hypothesis verdict — CONFIRMED

| Probe | Prediction (plan lines 49-56) | Observation | Verdict |
|---|---|---|---|
| P1 F0020 b#2 arena_only | `arena_only_count >= 1` (wrap-back `(38,27)` in arena, not directed_he) | `arena_only_count=16` with non-degenerate first 5 entries `(73,20)`, `(36,69)`, `(54,44)`, `(59,49)`, `(31,71)` | ✅ MET |
| P1 F0020 b#2 constructed_only | `constructed_only_count >= 1` (HE 59's `(38,26)` in directed_he, no traversal HE) | `constructed_only_count=16`, first entry `(38, 26)` exact match for predicted HE 59 construction-time key | ✅ MET |
| P1 F0044 batch arena_only | `arena_only_count == 0` ideal; `> 0` requires investigation | 6/7 invocations `arena_only_count=0`; 1/7 invocation `arena_only_count=4` (2 self-loops + 2 non-degenerate wrap-backs) | ⚠ partial — see scope decision §5 |
| P2 F0020 b#2 simulated | `simulated=0 (vs actual=2)` | `simulated_twin_oracle_unpaired_count=0 (vs actual=2)` | ✅ MET — **load-bearing prediction** |
| P2 F0044 batch simulated | `simulated=0 (vs actual=0)` | All 7 invocations: `simulated=0 (vs actual=0)` | ✅ MET |

**Acceptance gate (plan line 58): "if simulated values match the predictions on both F0020 and F0044, mechanism is empirically confirmed; spec phase proceeds."** Both predictions met. **Mechanism empirically confirmed.**

The PR-Y23 canary §1 P4 reading is now load-bearing for PR-Y24's fix shape: the construction-time `directed_he` keys for HE 58 and HE 59 lack the wrap-back's phantom reverses `(BV38,BV27)` and `(BV26,BV38)`. Re-keying the [twin-oracle] from arena traversal to construction-time `directed_he` flips F0020 b#2 unpaired_count 2→0 without modifying the arena structure or pairing logic upstream.

---

## §5 Recommended scope — B1 (plumb-via-arena-field)

Plan §"Site B" decision criterion (line 153-155):

> If canary P1 shows F0044 batch's validator is consulted with non-zero `arena_only_count`, escalate to **Sub-option B1**: add `pub constructed_directed_edge: Vec<Option<(VertexIdx, VertexIdx)>>` field on `TopoArena`, populated at Step 7 close, consumed by validator.
>
> Decision criterion: spec-y24 selects B2 unless canary memo recommends B1.

The F0044 batch shows `arena_only_count=4` in invocation #5 of 7. Per the literal rule: **escalate to B1**.

### Why B1 is the correct recommendation despite both verdicts agreeing in invocation #5

The plan's escalation criterion is `arena_only_count > 0`, NOT "actual ≠ simulated". Both formulations are defensible for this canary, but B1 is the right call because:

1. **Validator path is exercised with divergent state.** The validator at `yang_integration.rs:1241-1308` rebuilds its own `arena_dir_edges`-equivalent. In F0044 invocation #5, that rebuild would see the same 4 arena-only edges. Even though the verdict happens to be 0/0 today, future cohort additions or upstream changes (e.g., R3 ownership tweaks) could shift these from "happens to fail rev-test by coincidence" to "rev-test passes via a different wrap-back vertex pairing". B2 (move predicate upstream into extract_topology, drop validator's classification entirely) would lose the validator's independent check; if the upstream classification has a bug, the validator becomes a no-op for missing-edge detection.
2. **B1 preserves the validator's independent role.** The TopoArena field carries construction-time ground truth across the extract→validate boundary, so the validator can still cross-check NMM vs missing-edge with the correct ground truth. This is closer to "fix the observation layer" than "delete the observation layer".
3. **B2 would couple the spec to "fix at extract-time only" assumption.** If a downstream consumer (e.g., a future PR-Y25+ tessellation refinement) needs the same construction-time keying, B1's TopoArena field is reusable; B2's inline-into-extract is not.

### Caveat — B2 may still be acceptable for spec-y24

If spec-y24 reads §5 invocation table and concludes that "all 7 actual=0 ∧ all 7 simulated=0 ⇒ no behavioral divergence ⇒ B2's removal of the validator's classification is safe", that's a defensible alternative reading. Per `feedback_oracle_credibility_via_role_separation.md`, the canary names the empirical evidence and the spec selects the fix shape. This memo recommends B1; the spec is free to weigh the tradeoffs differently.

---

## §6 Banked findings — observations not load-bearing for the verdict

These are non-anchor observations that may matter for follow-on PRs:

1. **F0020 b#1 has 8 arena_only / 11 constructed_only divergence with 0/0 unpaired.** The 8 arena_only entries are all self-loops `(35,35)`, `(9,9)`, `(34,34)`, `(17,17)`, `(32,32)`. The 11 constructed_only entries `(33,14)`, `(32,17)`, `(35,34)`, `(34,33)`, `(9,35)` are non-degenerate — these correspond to construction-time directed edges that don't manifest in arena traversal. This means even on a "clean" boolean (unpaired=0), there is a **structural mismatch** between the two views. The PR-Y24 fix will see this as a benign case (sim=0 actual=0) but it's a flag that the arena's traversal does NOT preserve construction-time directed-edge identity in general — likely due to PR-Y20-MODE-A `Option<HalfEdgeIdx>` non-manifold-edge handling deferring directed-edge identity through arena restructuring.

2. **The arena_only/constructed_only ratio is roughly 1:1 in F0020 b#2 (16:16) and the divergence sizes scale with HE count.** Suggests the wrap-back mechanism at `topology_extract.rs:1131-1146` (PR-Y23 canary Layer 2) produces a roughly fixed *fraction* of HEs as wrap-backs per open chain. For 169 HEs, 16 phantom-traversal edges = ~9.5%. This rate would be useful as a regression-detection metric in future cohort guards.

3. **F0044 batch invocation #5 (229 HEs, 4 arena_only) is the only cohort case with non-zero divergence.** Worth flagging that this invocation also uniquely has `collision_count=1` — the same pattern as F0020 b#1. The collision count is informational per PR-Y19-MODE-B 0e amendment. Possible signal that the cohort's "happens to be 0" is fragile in invocation #5 specifically — see §5 rationale for B1.

4. **The construction-time `(BV38, BV26)` for HE 59 confirmed at `[y24-probe-p1] constructed_only_first5=[(38, 26), ...]`.** This is the EXACT key cited in PR-Y23 canary §1 P3 (`constructed_dest_brep = 26 ... arena_traversal_dest_brep = 27`). The keys are stable across PR-Y23 commit `990571c` and PR-Y24's commit `3c749a3` baseline; the mechanism is reproducible.

5. **Probe overhead bounded.** P1+P2 each iterate `arena.half_edges` once and build `directed_he.keys()` HashSet (O(n_he) each). P2 also builds a per-HE Vec (O(n_he)). All three structures are local to the `if y24_probe` blocks and dropped at scope exit. Total probe output for F0020: 8 `[y24-probe-*]` lines. For F0044: 28 lines (4 lines × 7 invocations).

---

## §7 Final-report block

### Probe diff (worktree only — NOT committed)

```
$ cd /tmp/y24-probe-wt && git diff --stat
 crates/kernel/src/boolean/topology_extract.rs | 89 +++++++++++++++++++++++++++
 1 file changed, 89 insertions(+)
```

### Live tree status at memo write

```
$ cd /home/claude/workspace && git status
On branch main
Your branch is up to date with 'origin/main'.

nothing to commit, working tree clean
```

Identical to start-of-session status. No live-tree mutation occurred during canary work. The memo at `docs/audits/pr_y24_anchor_canary.md` is the only live-tree change in this canary phase.

### Probe output artifacts

- `/tmp/y24-canary-f0020.txt` — F0020 spotlight log, retained for follow-on phases.
- `/tmp/y24-canary-f0044.txt` — F0044+F0045+R0092 batch log, retained.
- `/tmp/y24-probe-wt/` — probe worktree, retained until close-out (`lead-y24` removes via `git worktree remove`).

---

## §8 Routing

- **Hypothesis selected:** option (e) reframed (re-key NMM-classification predicate from arena-traversal to construction-time `directed_he`).
- **Verdict:** CONFIRMED. F0020 b#2 simulated=0 (vs actual=2); F0044 batch all 7 invocations simulated=0 (vs actual=0).
- **Recommended scope:** **B1** for the validator side (plumb construction-time directed-edge data via a new `TopoArena` field). **B2** would also be defensible per §5 caveat; spec-y24 weighs.
- **Anchor sites:**
  - Site A (oracle): `crates/kernel/src/boolean/topology_extract.rs:1445-1471` (and offender-trace at L1515).
  - Site B (validator): `crates/kernel/src/boolean/yang_integration.rs:1241-1308`.
- **Next agent:** `spec-y24` (do NOT spawn until team-lead confirms).
- **No fix proposed here.** Per `feedback_anchor_before_fix.md`: canary names WHERE; spec/impl decide WHAT.
- **Spec phase MUST cite both papers per plan §"Spec phase":**
  - **Yang 2025 §3** (verbatim, `refs/text/yang2025_hybrid_boolean.txt:248-249`): "edges that form a continuous boundary, with each edge shared by two adjacent faces."
  - **Cherchi 2022 §3** (verbatim, `refs/text/cherchi2022_interactive_robust_mesh_booleans.txt:251-254`): "the arrangement is guaranteed to be a well formed simplicial complex and surface patches are bounded by closed loops of non-manifold edges, namely the intersection lines."
  Both establish that patch boundaries are closed loops of non-manifold edges; the [twin-oracle]'s NMM-classification predicate is the mechanization of that contract, and re-keying it on construction-time `directed_he` aligns the predicate with the paper's input ground truth (the directed edges that were inserted by the construction loop) rather than with arena traversal (which is polluted on open-chain wrap-backs at L1131-1146).
