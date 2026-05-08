# PR-Y23 ABORT — fix-shape (a) regresses F0044 cohort; bank for PR-Y24

**Date:** 2026-05-08
**Verdict:** **ABORT** per spec §7 ("F0044 / F0030 / F0050 cohort regresses → ABORT")
**Anchor identified:** H1' open-loop wrap-back at `topology_extract.rs:961` (commit `990571c` canary memo)
**Fix shape attempted:** (a) closure check before `loops.push(chain)`
**Fix shape outcome:** load-bearing PR-Y23 gate GREEN but cohort guard RED — banked for PR-Y24 redesign

This memo documents the empirical results of the PR-Y23 fix-shape (a) attempt, the failure mode that drives the ABORT decision, and the banked findings that PR-Y24 must address.

---

## §1 Cycle artifacts (kept on main)

PR-Y23 produced three useful artifacts that remain committed:
- **`docs/audits/pr_y23_anchor_canary.md`** (commit `990571c`) — H1' empirically confirmed; mechanism layered at `topology_extract.rs:913-963` (Layer 1 anchor) → L1131-1146 (Layer 2 wrap-back) → L1445-1449 (Layer 3 oracle keying). Probe data is the seed for any PR-Y24 attempt.
- **`specs/yang_pr_y23_open_loop_emission.md`** (commit `9647185`) — full FIP §3 spec with branch table evaluating fix shapes (a)/(b)/(c). The (a) selection reasoning is explicit and remains valid as a starting point; the empirical refutation here is an addendum to spec §7.
- **`crates/test-harness/tests/pr_y23_open_loop_emission_regression.rs`** (commit `770c5a2`) — RED on `8de94e5`. Test stays in tree as the load-bearing oracle for any PR-Y24 attempt; it is `#[ignore]`-gated so CI is not broken.

---

## §2 Fix-shape (a) attempts and outcomes

Two variants were attempted, both reverted (live tree clean per `git status`):

### §2.1 Variant 1 — full drop

```rust
let closed = chain.last().map(|&(_, v1, _)| v1)
    == chain.first().map(|&(v0, _, _)| v0);
if closed { loops.push(chain); }
// else: drop the open chain
```

Test outcome:
- ✅ `pr_y23_f0020_twin_oracle_zero` — PASS (max `[twin-oracle] unpaired_count` = 0; pre-fix = 2)
- ✅ `pr_y23_f0044_twin_oracle_no_regression` — PASS (max = 0)
- ❌ `pr_y22_f0044_b5_mode_a_missing_drops_by_2` — **FAIL** (max `[topo-extract] unpaired` = 9; PR-Y22 contract = 0)
- ❌ `pr_y22_f0020_mode_a_missing_zero` — **FAIL** (PR-Y22 baseline regression)

The full-drop variant **regresses PR-Y22's `[topo-extract] unpaired = 0` cohort guard** in the F0044/F0045/R0092 batch by 9 new MISSING-edge defects.

### §2.2 Variant 2 — n=2-only narrowing

```rust
if closed || chain.len() != 2 { loops.push(chain); }
// else (open AND n==2): drop
```

Reasoning: per canary §4 banked-finding 4, only n=2 wraps the `next` ring into a 2-cycle pseudo-pair that the [twin-oracle] reads as mutually-reverse. Higher-n open chains' wrap-backs land at non-coinciding vertices and surface as "no arena-traversal reverse → legitimate-NMM" rather than "reverse in arena → missing-defect."

Test outcome:
- ❌ `pr_y23_f0020_twin_oracle_zero` — **FAIL** (max = 1; new defect at F0020 Extrude 2 b#1: `half_edge[87].twin = None but arena contains a HE for the reverse direction (9->9)` — a self-loop phantom from a KEPT higher-n chain in F0020 b#1, which had `unpaired_count=0` pre-fix)
- ✅ `pr_y23_f0044_twin_oracle_no_regression` — PASS (max = 0)
- ❌ `pr_y22_f0044_b5_mode_a_missing_drops_by_2` — **FAIL** (still max = 9; n=2-only doesn't help the cohort because the F0044 batch's regressing chains are not n=2)
- ❌ `pr_y22_f0020_mode_a_missing_zero` — **FAIL**

Worse than Variant 1: regresses BOTH PR-Y23's own F0020 gate AND the cohort guard.

---

## §3 Why fix-shape (a) is fundamentally incompatible with the cohort

The closure check at L961 treats each patch's open chain in isolation, but in the F0044/F0045/R0092 batch, an open chain's half-edges can legitimately pair with closed-loop HEs in OTHER patches that share the same canonical directed-edge pair. Dropping the open chain leaves those other patches' HEs stranded as new MISSING-edge defects.

This is a structural property of the patch-segmentation graph: the chain-builder at L900-963 operates per-patch but the directed-edge canonical map (`directed_edge_to_tris` at Step 4) is global. An HE's pair-ability is determined globally; the closure check is local. The two views disagree on the F0044 cohort.

Variant 2 (n=2-only narrowing) attempts to bound the drop to the precise symptom seen in F0020 b#2 (canary §3 Layer 2 mechanism). But `[twin-oracle]`'s phantom-reverse mechanism is not n-bounded: in F0020 b#1, an n>2 open chain with self-incident first/last vertex creates a `(v→v)` self-loop traversal edge that the oracle also catches. The variant 2 patch keeps that chain and surfaces the previously-hidden `(9→9)` defect.

---

## §4 Bank for PR-Y24

The H1' anchor identification is correct. The fix shape needs redesign. PR-Y24 candidates:

### §4.1 Option (b) revisited — R3 ownership strengthening at L810-863

PR-Y23 spec rejected (b) without a canary. The reasoning was that R3 modification requires forward-look R3 doesn't currently perform, with corpus-wide effects.

PR-Y24 should canary option (b) empirically: instrument R3 to track which contended directions, when stripped, would leave the loser patch with an open chain. If it can be cheaply detected at R3 time, R3 can fall back to a different tie-break (e.g., assign ownership to the patch whose loop-closure depends on the contended direction). This re-frames the problem: "R3 must not strip a direction whose absence opens the loser patch's loop."

Per `feedback_anchor_before_fix.md`: R3 strengthening should NOT be coded without a canary that confirms the forward-look is feasible.

### §4.2 Option (d) — Layer 2 wrap-back fix (was out-of-scope per PR-Y23 spec §9)

When Step 7 at L1131-1146 forms HEs over a chain, replace the circular `next_idx = he_base + (i+1) % n` with a non-circular variant for open chains: HE[n-1].next = HE[n-1] itself (self-loop). The PR-Y19-MODE-B 0e amendment at L1477-1482 already skips self-loops in collision detection; symmetric treatment in `arena_dir_edges` (L1445-1449) would make the [twin-oracle] correctly classify self-next HEs as legitimate NMM rather than phantom-reverse offenders.

Pros: keeps the open chain's HEs in the arena (no cohort regression), surgical change at the layer where the wrap-back actually happens.
Cons: was out-of-scope per PR-Y23 spec §9 (Layer 2 listed as "downstream consumer; correct on closed inputs"); requires re-spec.

### §4.3 Option (e) — Targeted post-pairing reconciliation

After the pairing pass at L1219-1380 emits its `[topo-extract] summary`, walk the arena once more: for every HE with twin=None and arena_traversal-reverse exists (the [twin-oracle]'s exact predicate), check whether the arena_traversal-reverse is itself a wrap-back artifact. If yes, mark BOTH HEs as legitimate-NMM (twin stays None but arena_dir_edges insertion is suppressed) instead of letting the validator panic.

Pros: smallest behavioral change.
Cons: violates `feedback_yang_only.md` ("no fallback paths") — masks the upstream defect with a downstream bypass. Likely rejectable on principle.

PR-Y24 spec phase weighs these.

---

## §5 Discipline notes

- impl-z23 (the spawned implementer agent) stalled mid-cycle and never reported. Team-lead took over the patch and verification per `feedback_per_plan_cycle_team.md` close-out mandate. impl-z23 will be shut down with the team teardown.
- Two fix-shape attempts. Per `feedback_anchor_before_fix.md` the count of "wrong anchors" is 0 (the anchor itself — L961 — is correct; the FIX SHAPE is wrong). Strategic escalation threshold (3 wrong anchors → reference-parity build) is not yet hit.
- Live tree was reverted between attempts via `git checkout -- <file>` (the working-tree-only revert flavor; not destructive on history). Per `feedback_adversary_no_destructive_git.md`: no `git stash`, no `git reset --hard`, no `git checkout --` on uncommitted material that wasn't itself written by team-lead in this cycle. Both reverts were on team-lead-authored, uncommitted patches in `topology_extract.rs`.
- Final tree: `git status` clean; commits on main = canary (`990571c`) + spec (`9647185`) + test (`770c5a2`) + this abort memo.

---

## §6 Recommendation to user

PR-Y23 is correctly aborted per spec §7. The H1' anchor and mechanism understanding are valuable and committed; the test will fail in `#[ignore]` until PR-Y24 lands. Recommended next steps:

1. **Open PR-Y24** with a fresh canary phase that probes option (b) (R3 strengthening) — this is the most paper-faithful redesign and preserves the closed-loop invariant at the upstream cause rather than the downstream symptom.
2. **Alternative:** Open PR-Y24 with option (d) (Layer 2 wrap-back fix). Requires re-spec because it expands scope to Layer 2.
3. **Defer:** Bank the test as ignored and move on to F0030 / F0050 cohort siblings (different defect classes, may be cheaper to clear).

The team teardown (TeamDelete) closes this PR's cycle. PR-Y24 spawns a fresh team.
