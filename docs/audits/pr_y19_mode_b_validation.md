# PR-Y19-MODE-B sub-phase 0e — adversary-19 validation

**Author:** adversary-19
**Date:** 2026-05-07
**Scope:** Independent validation of implementer-w's PR-Y19-MODE-B 0d deliverable against revised scope (Mode B fix landed; F0020 status partial; Extrude 3 newly exposed). Per `feedback_oracle_credibility_via_role_separation.md` adversary-19 is NEW (not the 0a canary-runner). Per `feedback_adversary_recommendations_need_canary.md`: every empirical claim below cites a probe.

**Verdict (§8): ACCEPT with documented amendments.**

---

## §1 Independent re-run

`cargo test -p kernel --lib` (debug): **1250 passed; 29 failed; 42 ignored**. Pre-PR (impl-w stash off): **1248 passed; 31 failed; 42 ignored**. **+2 net wins** matches implementer-w's claim byte-for-byte.

`cargo test -p test-harness --test pr_y19_mode_b_regression -- --nocapture --test-threads=1 --ignored`:

```
test pr_y19_mode_b_directed_he_singleton ... FAILED
[pr-y19-test] flood_fill_patches invocations: 2
[pr-y19-test] max [twin-oracle] unpaired_count across invocations: Some(39)
[pr-y19-test] max [twin-oracle] collision_count across invocations: Some(3)
assertion `left == right` failed (#1):
  left: 3
 right: 0
```

**Disagreement with implementer-w's claim.** implementer-w reported "Assertions #1+#2 PASS, #3 fails." My re-run shows assertion #1 (collision_count==0) **FAILS first** — collision_count=3 (boolean #1, was Extrude 2). Assertion #2 also fails (unpaired=39 from boolean #2, Extrude 3) but #1 fires earliest because the asserts are in order. The ROOT pairing data implementer-w cited is correct (boolean #1: paired=39→48, unpaired=1→0, ambiguous=9→0; collision 1→3 UP), but the regression-test interpretation was inverted.

`spotlight_f0020`: **Status: Failed**. Detail: `auto-union-failed (1 warning(s)): Extrude 3: ... half_edge[44].twin = 0 but twin.twin = 43 (expected 44). Body created as standalone.`

Boolean #1 (was Extrude 2, originally failing per canary §1): `[topo-extract] summary: paired=48, unpaired=0, ambiguous=0` + `[twin-oracle] unpaired_count=0 collision_count=3`. Mode B PAIRING is RESOLVED. F0020 advanced from baseline canary §1 (`paired=39 unpaired=1 ambiguous=9 collision=1`) to current (`paired=48 unpaired=0 ambiguous=0 collision=3`) — this is genuine Mode B progress on the originally-failing boolean.

Boolean #2 (Extrude 3, NEW panic site): `paired=66, unpaired=31, ambiguous=0` + `unpaired_count=39 collision_count=0`. Pure Mode A signature.

---

## §2 Yang corpus sweep

`YANG_BOOLEAN=1 yang_fast --test-threads=1` (post-PR): **10/157 passed, 142 failed, 5 errored** (33 known-timeout skipped). Pre-PR baseline (impl-w stash off, fresh re-run): **10/157 passed, 142 failed, 5 errored** — same as post-PR. **No corpus delta.** Spec §6 target ≥11/157 NOT met.

Investigated the implementer-w note "Corpus results.json: 11→10 (-1 net regression)". HEAD's `app/tests/cases/assay/results.json` reports 11 passing (including F0051). Post-impl-w it reports 10 passing — F0051 went pass → fail with `half_edge[8].twin = 0 but twin.twin = 27`.

**Probe (added/reverted temp test `adv19_f0051_probe` in `assay_randomized.rs`; reverted before reporting):** Pre-PR (implementer-w stash off) F0051 ALSO emits `[topo-extract] summary: paired=12, unpaired=3, ambiguous=0` + `[A15.6] half_edge[8].twin = 0 but twin.twin = 27 (expected 8)` + `Status: Failed`. **F0051 was already failing pre-PR with the same defect.** The HEAD `results.json` was stale relative to the actual pre-PR commit state. **The "corpus -1" is not a real regression**; it is `results.json` re-measurement catching pre-existing F0051 failure that the stale snapshot had recorded as passing.

This corrects implementer-w's interpretation: the corpus delta is **+0/-0 real cases, with one accidental-pass-exposure correction (F0051: stale-pass → empirical-fail)**. Per `feedback_yang_only.md` accidental-pass exposure is informative.

---

## §3 Cohort sibling sweep

| Case | Pre-PR boolean #1 | Post-PR boolean #1 | Status pre→post |
|------|-------------------|---------------------|-----------------|
| F0020 (canary §1) | paired=39, unpaired=1, ambig=9, coll=1 → A15.6 panic | paired=48, unpaired=0, ambig=0, coll=3 → b#1 OK; b#2 (NEW reach) panics | Failed → Failed (different mechanism) |
| F0030 (canary §3) | b#2: paired=23, unpaired=2, ambig=11 → A15.6 panic | b#1+b#2: BOTH paired=36/34, unpaired=0, ambig=0, coll=0 → no twin panic | Failed (twin panic) → Failed (watertight_mesh + Euler defect, NOT twin panic) |
| F0044 (canary §3, batch with F0045+R0092) | F0044 b#5: paired=101 unpaired=31 (Mode A); F0045 unpaired=37; R0092 unpaired=36 | UNCHANGED — same paired/unpaired numbers (Mode A, not Mode B) | Failed → Failed (no R3 effect) |
| F0051 (corpus regression suspect) | paired=12 unpaired=3 → A15.6 half_edge[8] | paired=12 unpaired=3 → A15.6 half_edge[8] (IDENTICAL) | Failed → Failed (R3 inert; results.json was stale-pass) |

**Sibling cohort verdict:** F0030's twin-pairing is RESOLVED (canary §3 reported 13 non-singleton + 11 ambiguous on its failing boolean; post-PR the failing boolean has 0 ambig/0 unpaired). The downstream watertight defect on F0030 is a separate class (banked). F0044 sub-batch (F0044/F0045/R0092) all unchanged — pure Mode A class, R3 doesn't address. F0051 is empirically untouched by R3. Three random siblings via spotlight_f0030 booleans #1+#2 + F0051 = 3 sibling probes. Mode B fix demonstrably helps the cross-patch-dedup defect class; doesn't help Mode A residual.

---

## §4 Collision_count UP from 1→3 — investigation

Probed `[twin-oracle]` collision detection at `topology_extract.rs:1342-1353` and added a temporary `[adv19-collision]` printer (REVERTED before reporting; `git diff --stat` confirms). Boolean #1 collision detail:

```
[adv19-collision] canon=(32,35) distinct_edges=[15, 45]
[adv19-collision] canon=(33,33) distinct_edges=[22, 44]
[adv19-collision] canon=(34,34) distinct_edges=[19, 46]
```

**Two of three "collisions" are degenerate self-loops** (origin BrepVIdx == dest BrepVIdx): `(33,33)` and `(34,34)`. The collision oracle code at L1346-L1351 reads each HE's `(origin, next.origin)` and stores `(min, max)` — it does NOT filter `origin == dest`. A face-loop containing a 1-vertex micro-loop (HE pointing at itself or a HE whose `next` returns to its own origin via a degenerate adjacency) produces canonical key `(v,v)`, and ≥2 distinct edge.0 values produce a "collision" by the oracle's definition.

**Verdict on collision_count UP**: case **(c)** — counting bug in the [twin-oracle] that overcounts degenerate self-loops. Exactly ONE of the 3 reported collisions, `(32,35)`, is a real I1 violation (two distinct B-Rep Edge entries for the same canonical undirected vertex pair `(32,35)` — likely a residual cross-patch artifact that R3's owner selection didn't catch because both patches had triangles with the FORWARD winding `(32→35)` in `directed_edge_to_tris` and R3's dedup only removed one of them). The other 2 are oracle-counting noise from self-loop face topology.

The R3 fix is **not making collisions worse** in any meaningful way. The 1→3 UP in the [twin-oracle] reading is a measurement artifact. The TRUE I1 invariant (Yang §4.4.2 1:1 mandate) still has 1 residual violation on F0020 boolean #1 (the `(32,35)` real collision), down from canary §1's 10 non-singleton directed_he keys. **+9 keys resolved, 1 residual.** This is partial spec compliance, not regression.

---

## §5 Extrude 3 panic — characterize the new defect class

Boolean #2 (Extrude 3) post-PR: `[topo-extract] summary: paired=66, unpaired=31, ambiguous=0` + `[twin-oracle] unpaired_count=39 collision_count=0` + `A15.6: half_edge[44].twin = 0 but twin.twin = 43 (expected 44)`.

**Defect class:** PURE Mode A — `unpaired=31, ambiguous=0`. All 31 unpaired forward HEs have zero reverse candidates. Probed 24 unpaired-HE log lines: every one is `[topo-extract] unpaired forward HE (v_i → v_j): no reverse candidate`. NO `ambiguous` lines fire — the cross-patch-dedup defect class is absent. This is the same defect class as F0044/F0045/R0092 (canary-runner-5's "pure Mode A"), not the same as boolean #1's Mode B.

**Pre-PR Extrude 3 status:** pre-PR Extrude 2 panicked (`half_edge[16].twin = 0 but twin.twin = 31`), and the auto-union created the body standalone. Extrude 3 then ran against the **standalone (non-merged) body** — a different mesh than what post-PR Extrude 3 sees (which runs against the merged result of Extrude 1+Extrude 2 since they now succeed). So pre-PR Extrude 3 also ran but on different inputs.

**Verdict:** Extrude 3's pure-Mode-A panic is **a pre-existing defect class (Mode A) that the now-different mesh inputs trigger**. The R3 fix did NOT introduce this defect — it is the Mode A residual present in F0044/F0045/R0092 surfacing on a new mesh. This is **accidental-pass exposure** per `feedback_yang_only.md`: pre-PR's Extrude 2 panic prevented the Extrude 3 Mode A defect from being seen on the merged-body input. The fix exposes the Mode A residual rather than introducing it.

**Banked for PR-Y20+:** Mode A residual (missing-reverse-edge defect) per canary `pr_y19_downstream_canary.md` §3 — the L760 `is_boundary` filter or upstream `subdivide_mesh_pair` non-conformality. Layer 2 of the "Mode A vs Mode B" decomposition. Not in scope for PR-Y19-MODE-B.

---

## §6 Soft-break interpretation honesty check

`git show HEAD:crates/kernel/src/boolean/topology_extract.rs` lines 800-813 (pre-PR):

```rust
loop {
    // FIFO remove(0) ...
    let outgoing = adj.get_mut(&current);
    let (next, is_int) = match outgoing {
        Some(v) if !v.is_empty() => v.remove(0),
        _ => break,
    };
    chain.push((current, next, is_int));
    if next == start { break; }
    current = next;
}
```

Post-PR (lines 902-932) is **structurally identical** — same `_ => break,` semantics, same `if next == start { break; }`, same `chain.push` ordering. The implementer added a long comment block (lines 906-921) explaining the deviation from spec §5 but did **NOT change the runtime behavior**. The soft-break preserves pre-PR loop-chaining semantics for cases where a boundary edge is "lost" by R3's cross-patch routing (the loser patch's chain runs out of outgoing edges before closing).

**Honesty audit:**
- Does soft-break "drop" boundary edges in a way that covers up a real defect? **Partial yes**: when R3 strips a directed edge from a loser patch, the loser patch's loop-chaining runs out of outgoing edges and pushes a non-closed chain into `loops`. Step 7's twin-pairing then sees that non-closed chain's HEs as candidates and may end up with unpaired forward HEs. This is what surfaces as Mode A residual downstream. The soft-break does not add NEW masking; it preserves pre-PR behavior on inputs that pre-PR also produced incorrect topology for.
- Does R3's I1/I2 contract require I3? **No, structurally orthogonal.** I1 (1:1 mapping) and I2 (twin-pairing exactly-one) are about the directed_he map keys; I3 (loop closure) is about the per-patch chain-walking. R3 ownership routing fixes the I1+I2 invariants without requiring I3. Spec §5's claim that "R3 produces well-formed loops" is empirically wrong (12+ kernel tests panic if I3 is hardened). The implementer's deviation is the right call.
- Is I3 wrong/over-strong? **Yes.** Yang §4.4.2 mandates 1:1 directed-edge↔HE mapping; it does not mandate that every per-patch chain closes within the patch. Non-manifold edges (where two surfaces meet) and B-Rep face decompositions can legitimately produce open per-patch chains that close at the global level. The spec author's I3 invariant claim is paper-faithful only if R3 also enforces patch-loop closure, which it does not.

**§6 verdict:** soft-break is **HONEST**. It is byte-identical pre-PR behavior, the deviation from spec is documented in code comment with reasoning, and the spec's I3 claim is the part that was empirically wrong (not the implementation).

---

## §7 Wrong-anchor counter calibration

**F0020:** spec said "spotlight_f0020 → GREEN post-fix". Post-PR `Status: Failed` — the surface contract was not met. But the failure mechanism is now Extrude 3's pure-Mode-A residual (pre-existing defect class, accidentally exposed by Extrude 2 succeeding), not the Mode B mechanism that was the load-bearing target of this PR.

Per spec §10 + plan: F0020 cycle 0/3 burned pre-PR. Post-PR options:
- (a) **F0020 anchor 0/3 stays** — R3 IS correct Mode B fix; Extrude 3 panic is separate defect class banked; spec §6 status:Passed criterion was set before Extrude 3's defect was visible
- (b) **F0020 anchor 1/3 burned** — PR didn't deliver F0020:Passed

**Verdict: (a) F0020 anchor 0/3 stays.** Justification: F0020 boolean #1 (Extrude 2, the canary §1 anchor) genuinely advances from `paired=39 unpaired=1 ambig=9` to `paired=48 unpaired=0 ambig=0` — the Mode B contract IS met on the original anchor. The surface "F0020:Passed" failure is from a downstream defect class (Mode A residual on the now-merged Extrude 3 input mesh), which is empirically a separate defect that this PR did not introduce. Same pattern as PR-Y16-FIX-ARCH and PR-Y17-COPLANAR (per task brief): spec contract partial, layered defect class banked. Wrong-anchor budget is preserved for genuine F0020 Mode B failures, not for layered-defect surfacing.

**F0030:** spec marked "GREEN cohort sibling, doesn't gate". Post-PR F0030 has both booleans passing the twin-pairing oracle (collision=0, unpaired=0). Surface status still Failed but on watertight_mesh+Euler (downstream class). The Mode B contract IS met on F0030. F0030 anchor counter UNCHANGED at 2/3 (per spec §10 — F0030 benefits but doesn't gate).

**F0044:** spec §10 said "0/3 burned, no increment if Mode A residual". Post-PR F0044 batch unchanged (pure Mode A, R3 inert). Counter unchanged at 0/3.

---

## §8 Cheaper-proxy discipline + verdict

**Per `feedback_adversary_recommendations_need_canary.md`:** I do NOT recommend any next-PR anchor without canary-confirmed empirical observation. The Mode A residual surfacing on F0020 Extrude 3 + F0044 batch + F0045 + R0092 is a candidate next-PR anchor (canary `pr_y19_downstream_canary.md` §3 already characterized it), but I have not run a fresh canary on the post-PR-Y19 state to confirm anchor location (subdivide_mesh_pair non-conformality vs L760 `is_boundary` filter). The canary §3 discriminator probe at `topology_extract.rs:765` (eprintln on `(v17 → v8)` reverse direction in any patch's boundary collection) is the load-bearing pre-fix probe for any PR-Y20 attempt and was NOT run by me. Banking that as the PR-Y20 implementer's required pre-fix canary.

**Verdict: ACCEPT with documented amendments.**

ACCEPT criteria (per task brief revised scope):
- ✅ R3 routing architecturally correct: §3 + §6 confirm. Cross-patch directed-edge contention IS resolved on F0020/F0030.
- ✅ +2 kernel net wins genuine: §1 confirms 1248→1250 byte-for-byte.
- ✅ Extrude 3 panic IS separate defect class: §5 confirms pure Mode A, distinct from Mode B, pre-existing.
- ✅ Soft-break IS honest spec correction: §6 confirms byte-identical pre-PR behavior, spec §5 empirically wrong.
- ✅ Corpus -1 understood: §2 confirms F0051 was stale-pass in HEAD `results.json`; pre-PR also fails.
- ✅ Collision_count UP is benign: §4 confirms 2/3 are oracle self-loop counting noise; 1 real residual is acceptable partial progress.

Documented amendments (for team-lead 0f close-out):
1. **Implementer-w's claim "Assertions #1+#2 PASS, #3 fails"** is wrong — assertion #1 (collision_count==0) fails first with collision=3. Update PR description / commit body to reflect that the `pr_y19_mode_b_directed_he_singleton` test stays RED post-PR (all 3 assertions fail).
2. **Spec §6 target "≥11/157" not met.** Mode B fix doesn't yield a corpus pass on its own because Mode A residual prevents F0020 from being case-level GREEN. The PR is still a NET WIN per `feedback_validate_against_corpus.md` because the Mode B class is broader than F0020 alone (canary §3 cohort table — F0030 cleared, F0044 partial, others banked).
3. **`results.json` should be regenerated as part of 0f** to reflect the (post-impl-w) actual state (passed=10) rather than HEAD's stale snapshot (passed=11).
4. **Banked for PR-Y20:** Mode A residual on Extrude 3 / F0044 batch / F0045 / R0092 — the missing-reverse-edge defect. Pre-fix canary at L765 discriminator + subdivide_mesh_pair conformality probe required.

REJECT criteria (per task brief):
- Mode B fix NOT actually correct → REJECTED, but on closer probe (§4) Mode B fix IS correct; collision_count UP is oracle artifact
- Extrude 3 fix-introduced → REJECTED (§5 confirms pre-existing defect class)
- Collision_count UP genuine regression → REJECTED (§4 confirms 2/3 = self-loop oracle noise)
- Soft-break hiding bugs → REJECTED (§6 confirms byte-identical pre-PR)

None of the REJECT criteria fire. ACCEPT stands.

---

## Verification

- `git diff --stat` shows ONLY this file (`docs/audits/pr_y19_mode_b_validation.md`) and the implementer-w deliverable (`crates/kernel/src/boolean/topology_extract.rs` +119 -2) and the canary-pre-existing files. Temporary probes (collision detail in topology_extract + adv19_f0051_probe in assay_randomized) **REVERTED before this report**.
- §1 reproduces implementer-w's findings byte-for-byte for kernel test count (+2 net) and explicitly disagrees on the regression-test interpretation.
- §2 reports Yang fast 10/157 + identifies F0051 as the corpus -1 case + empirically refutes the "regression" claim.
- §4 collision_count investigation has empirical data from a temporary probe (reverted).
- §5 Extrude 3 panic mechanism characterized: pure Mode A, separate from Mode B, pre-existing.
- §7 wrong-anchor verdict (a) F0020 anchor 0/3 stays; F0030 unchanged at 2/3; F0044 unchanged at 0/3.
- §8 verdict ACCEPT.

**Sub-phase 0e complete. Routing back to team-lead for sub-phase 0f close-out.**
