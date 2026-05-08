# PR-Y23-OPEN-LOOP-EMISSION — F0020 [twin-oracle] residual via patch-boundary closed-loop invariant

**Author:** spec-z23 · **Date:** 2026-05-08 · **Plan:** `/home/claude/.claude/plans/optimized-wandering-wind.md` Phase 0b
**Predecessor:** PR-Y22-RECOVERY (commit `8de94e5`) closed the `[topo-extract] summary unpaired=N` 8 → 0 gate but did not fix the downstream `[twin-oracle] unpaired_count = 2` residual it banked (predecessor §5 R-b amendment).
**Canary:** `docs/audits/pr_y23_anchor_canary.md` (canary-z23) — H1' confirmed; anchor at `topology_extract.rs:913-963`; mechanism is the open-chain `loops.push` at L961 producing a circular `next`-ring whose wrap-back manufactures a phantom `(BV38 → BV27)` arena-traversal directed edge.
**Variant:** FIP §8 Bug Fix Variant.

---

## §1 Goal

Drive F0020 Extrude 3 `[twin-oracle] unpaired_count` from **2 → 0** and resolve `spotlight_f0020`'s validator panic:

```
half_edge[58].twin = None but arena contains a HE for the reverse
direction (38->27) — this is a missing-edge defect (Yang Step 6/7
boundary-classification dropped the reverse), not a legitimate
non-manifold edge. Banked PR-Y21+.
```

The defect is that `flood_fill_patches::Step 6` emits `loops.push(chain)` at `topology_extract.rs:961` unconditionally — including chains where `chain.last().v1 != chain.first().v0`. Yang 2025 §3 ("each edge shared by two adjacent faces") and Cherchi 2022 §3 ("surface patches are bounded by closed loops of non-manifold edges") both mandate that patch boundaries be closed loops; an open chain emitted as a "loop" violates this invariant. Step 7's circular `next`-ring at L1131-1146 then manufactures a wrap-back next-pointer that the `[twin-oracle]` correctly reports as an orphaned reverse direction, panicking the validator.

Per `feedback_no_last_bug.md`: PR-Y23 closes ONE layer (the open-chain emission). Further downstream layers (F0050 normals/Euler class; F0030 still-Failed status; the higher-n open chains' contribution to `[yang-diag] 39/169 unpaired`; eventual non-manifold-edge twin policy) remain banked for PR-Y24+. Per `feedback_yang_only.md`: this is not a fallback — this is restoring the paper's mandated closed-loop invariant.

---

## §2 Background

### §2.1 Predecessor baseline (PR-Y22-RECOVERY at `8de94e5`)

Predecessor §5 R-b amendment + adversary-22 §6 banked the F0020 `[twin-oracle] unpaired_count = 2` residual as PR-Y23+ territory after demonstrating it originates from a separate downstream layer in the in-process B-Rep build path, not the `[topo-extract]` topology layer. Post-PR-Y22 metrics on F0020 second boolean (canary §1, P3, P4 + canary "Twin-oracle confirmation"):

```
[topo-extract] summary: paired=65, unpaired=0, ambiguous=0
[twin-oracle] total_directed_edges=169
[twin-oracle] unpaired_count=2
[twin-oracle] offender he=58 ... origin=v27 dest=v38
[twin-oracle] offender he=59 ... origin=v38 dest=v27
```

Both HE 58 and HE 59 carry `twin = None`; both are present in `arena.half_edges`. The pairing pass at L1219-1380 saw them as a 2-cycle of HEs whose constructed-time directed keys are `(BV27, BV38)` and `(BV38, BV26)` (NOT mutual reverses). The `[twin-oracle]` at L1417-1545 sees them as a 2-cycle of arena-traversal directed edges `(27 → 38)` and `(38 → 27)` (ARE mutual reverses) — the disagreement between the construction-time view and the arena-traversal view is the H1' signature.

### §2.2 Mechanism (canary §1 P0–P4, §3 layer table)

**Layer 1 (PR-Y23 ANCHOR — `topology_extract.rs:913-963`).** The R3 ownership pre-pass at L810-863 (PR-Y19-MODE-B) strips the `(10 → 11)` direction from patch 7 because patch 6 also emits it and patch 6 wins the lex tie-break (both share `SourceFace = midA_face4`). Patch 7 is left with `(11 → 12)` and `(12 → 10)` only — an open path `11 → 12 → 10`, not a closed loop. The loop-chaining body at L928-958 walks this adjacency, builds `chain = [(11,12,_), (12,10,_)]`, exits the inner walk via `outgoing = None` at L949-951 (no edge from vertex 10 in patch 7's adjacency map). At L961, `loops.push(chain)` emits this open chain unconditionally. The pre-existing soft-break documented at L933-947 (PR-Y19-MODE-B's banked residual) handles the inner-walk termination correctly but does NOT reject the chain at the outer-emit boundary.

**Layer 2 (`topology_extract.rs:1131-1146`, downstream consumer — NOT anchor).** Step 7 reads each emitted loop and constructs `n = chain.len()` half-edges with `next: HalfEdgeIdx(he_base.0 + (i+1) % n)`. For the n=2 open chain at face=7, loop=7:
- HE 58 (i=0, v0_brep=27, v1_brep=38, next_idx=59) — `next.origin = 38` agrees with declared dest.
- HE 59 (i=1, v0_brep=38, v1_brep=26, next_idx=58) — wraps back to HE 58 whose origin is BV27, NOT HE 59's declared dest BV26.

The circular ring is correct for closed loops (where `chain.last().v1 == chain.first().v0`); fixing this layer would break legitimate closed loops. The defect is the input, not the construction policy.

**Layer 3 (`topology_extract.rs:1445-1449`, downstream consumer — NOT anchor).** The `[twin-oracle]` builds `arena_dir_edges` by `(he.origin.0, arena.half_edges[he.next.0].origin.0)` — i.e. arena traversal. For HE 58 this yields `(27, 38)`; for HE 59 the wrap-back yields `(38, 27)`. The oracle correctly reports each as having its reverse present in the arena, but neither has a construction-time twin in `directed_he` — the validator panics on the missing-edge defect at `yang_integration.rs:1300-1308`. The oracle is correct; the upstream input is wrong.

### §2.3 Why this is a paper-invariant violation, not a legacy hack

Yang 2025 §3 (line 252 of extracted text):

> "...with each edge shared by two adjacent faces."

This is the manifold-edge definition. A B-Rep face boundary is a closed loop of such edges; a half-edge structure encoding the boundary therefore must form a closed cycle.

Cherchi 2022 §3 (line 248 of extracted text):

> "the arrangement is guaranteed to be a well formed simplicial complex and surface patches are bounded by closed loops of non-manifold edges, namely the intersection lines."

This is the patch-boundary closed-loop output contract. PR-Y23 restores this contract at the layer that currently violates it (L961).

---

## §3 Parameters

None. PR-Y23 is a bug fix with no user-visible parameter.

---

## §4 Branch table

H1' is the sole confirmed mechanism (canary §2 hypothesis decision table refutes H2/H3/H4). One row, three candidate fix shapes.

| Mechanism | Anchor | Fix-shape candidates | Selected |
|---|---|---|---|
| H1' open-chain emission produces a circular `next`-ring whose wrap-back manufactures a phantom traversal-direction reverse | `crates/kernel/src/boolean/topology_extract.rs:913-963` (the `loops.push(chain)` at L961) | (a) Add a closure check at L961: drop chains where `chain.last().v1 != chain.first().v0`. **Most paper-faithful** — restores Yang §3 / Cherchi 2022 §3 patch-boundary closed-loop invariant at the violating layer. Risk: dropped chains' HEs are not constructed, so Step 7 emits fewer faces; cohort regression must be empirically bounded by adversary phase. (b) Strengthen R3 ownership pre-pass at L810-863 to never strip a direction from a patch whose loop cannot close otherwise. **More invasive** — modifies upstream classification policy and may shift R3's tie-break behavior across the entire corpus. (c) Promote PR-Y19's spec §5 I3 panic at L949 from soft-break to hard-panic. **Empirically refuted**: PR-Y19-MODE-B documented at L933-947 that this approach broke 12 kernel tests + F0020 + T-shape union; reverting that decision is a known regression. | **(a)** |

### §4.1 Selection reasoning

Selected: **(a) — closure check at L961 before `loops.push`**.

**Why (a) over (b):**
- (a) operates at the precise layer where the closed-loop invariant is violated (Yang §3 + Cherchi 2022 §3) — restoring the invariant where it is breached, not by modifying classifications upstream that have other correct-on-the-corpus behaviors.
- (a)'s blast radius is bounded by what canary §1 P0+P1 + §4 banked-finding #4 already characterized: open chains exist at multiple patches in F0020 boolean #2, but only the n=2 case manufactures a `[twin-oracle]`-visible orphan reverse. Higher-n open chains drop their wrap-back edges to vertices that do not coincide with any other open chain's first vertex, so they appear as legitimate-NMM under PR-Y22's M1 predicate — not as missing-defect orphans. Dropping these chains also drops their constructed HEs, but PR-Y22 had already classified them as NMM (twin=None, no unpaired increment); the corpus-impact delta is the absence of phantom faces that wouldn't have paired anyway.
- (b)'s anchor (R3 ownership at L810-863) is upstream of patch segmentation and tie-breaking. Modifying R3 to "never strip a direction from a patch whose loop cannot close otherwise" requires a forward-look that R3 does not currently perform; introducing it changes the contract between R3 and Step 6, with downstream-corpus effects that cannot be locally bounded without re-canarying R3.

**Why (a) over (c):**
- (c) is empirically refuted. PR-Y19-MODE-B's L933-947 commentary documents the original spec-writer-s's I3 invariant claim ("R3 produces well-formed loops") as "empirically wrong on F0020 + T-shape union + 11 other kernel tests." Soft-break was the chosen mitigation for those cases; promoting back to a panic re-opens that regression.

**Adversary obligation:** the plan §"If H1' confirmed" notes "Adversary phase verifies face-count + Euler stay within bounds (the dropped HEs may produce a face-count delta on contended patches)." This spec endorses that obligation. F0044 batch face-count + `[topo-extract]=0` + `[twin-oracle]=0` (PR-Y22 GREEN) MUST be preserved; F0030 / F0050 must not regress to a worse Failed mode.

---

## §5 Invariants (paper-cited, formal, measurable)

### I1 — twin/traversal agreement on unpaired half-edges (Cherchi 2022 §3)

**Statement.** For every `HE` in `arena.half_edges` with `twin == None`, the arena-traversal directed edge

```
(arena.half_edges[he].origin, arena.half_edges[arena.half_edges[he].next].origin)
```

equals the construction-time directed edge `(v0_brep, v1_brep)` recorded for that HE in `he_provenance`.

**Why.** Cherchi 2022 §3 (line 248 of extracted text) mandates the arrangement be a "well formed simplicial complex" with "surface patches bounded by closed loops of non-manifold edges". A disagreement between traversal and construction is impossible in a well-formed simplicial complex — it is the H1' wrap-back signature.

**Measurable.** For each F0020 boolean invocation, iterate `arena.half_edges`; for each HE with `twin == None`, the two values above must match. Pre-PR baseline (canary P3 + P4): HE 59 disagrees (constructed_dest=BV26, traversal_dest=BV27). Post-PR target: zero disagreements.

### I2 — patch boundary loops are closed (Yang 2025 §3 + §4.4.2; Cherchi 2022 §3)

**Statement.** Every chain pushed to `loops` in `flood_fill_patches::Step 6` (the `loops.push(chain)` at `topology_extract.rs:961`) satisfies `chain.last().v1 == chain.first().v0`.

**Why.** Yang 2025 §3 (line 252 of extracted text): "with each edge shared by two adjacent faces" — the manifold-edge definition implies a face's boundary is a cycle. Yang 2025 §4.4.2 (lines 588-595 of extracted text):

> "Starting from an inner triangle, i.e. not on the boundaries of each mesh patch, using it as a seed triangle for the patch, our algorithm expands the patch by including more neighboring inner triangles, until all the neighboring triangles of the patch are on the boundaries. The boundary curves can then be easily collected and mapped back to the parametric surfaces..."

For boundary curves to be "easily collected" and "mapped back" they must be closed; an open chain is not a curve mappable to a parametric face boundary.

Cherchi 2022 §3 (line 248): "surface patches are bounded by closed loops of non-manifold edges, namely the intersection lines."

**Measurable.** Probe gate at the `loops.push` site asserts `chain.last().v1 == chain.first().v0` for every chain entering `loops`. Pre-PR baseline (canary P1): patch 7 emits `chain_len=2 first_v0=11 last_v1=10 closed=false`. Post-PR target: every emitted chain has `closed=true` (open chains are dropped, not pushed).

### I3 — F0020 Extrude 3 [twin-oracle] residual GREEN (load-bearing)

**Statement.** F0020 Extrude 3 `[twin-oracle] unpaired_count == 0` for every `flood_fill_patches` invocation.

**Why.** Pre-PR-Y22 baseline: 8 (cleared by PR-Y22 M1 + M2 to 2 — but `[topo-extract]` layer, not `[twin-oracle]`). PR-Y22 v2 §5 R-b amendment: post-PR-Y22 `[twin-oracle]` is 2 — banked PR-Y23+. PR-Y23 closes this 2 → 0 by removing the open-chain artifact at its source (I2 enforces no open chains; I1 holds as a consequence; the validator panic on `(38→27)` cannot occur because no HE is ever constructed for the dropped chain).

**Measurable.** `pr_y23_f0020_twin_oracle_zero` regression test asserts `MAX(parsed [twin-oracle] unpaired_count across all F0020 invocations) == 0`. Pre-fix red baseline: `MAX == 2` on `8de94e5`.

---

## §6 Oracles

### §6.1 Spotlight gate (load-bearing, qualitative)

`spotlight_f0020` reports `Status:Passed` post-PR.

**Pre-PR baseline:** `Status:Failed` at `validate_yang_result_topology` panic on `half_edge[58].twin = None ... (38->27)`.

**Run command:**
```
YANG_BOOLEAN=1 cargo test -p test-harness --test assay_randomized -- \
    spotlight_f0020 --ignored --nocapture --test-threads=1
```

### §6.2 Layer counter (load-bearing, numeric)

For every `flood_fill_patches` invocation in F0020, `[twin-oracle] unpaired_count == 0`.

**Pre-PR baseline:** boolean #1 = 0; boolean #2 = 2.

**Run command:**
```
YANG_BOOLEAN=1 TWIN_DEBUG=1 cargo test -p test-harness \
    --test pr_y23_open_loop_emission_regression -- \
    --ignored --nocapture --test-threads=1
```

### §6.3 Corpus delta gates (per `feedback_validate_against_corpus.md`)

| # | Gate | Pre-PR baseline | Post-PR target | Action if fail |
|---|---|---|---|---|
| §6.3.1 | F0020 spotlight Status | Failed | Passed | Same `(38,27)` panic → ABORT, bank PR-Y24. Different panic → classify next-layer per `feedback_no_last_bug.md`; ship if not a regression. |
| §6.3.2 | F0020 `[twin-oracle] unpaired_count` (max across booleans) | 2 | 0 | NOT 0 → ABORT |
| §6.3.3 | F0044 batch `[topo-extract] summary unpaired=N` | 0 (PR-Y22 GREEN) | 0 | non-zero → ABORT (cohort regression) |
| §6.3.4 | F0044 batch `[twin-oracle] unpaired_count` | 0 (PR-Y22 GREEN) | 0 | non-zero → ABORT (cohort regression) |
| §6.3.5 | F0030 spotlight Status + `[twin-oracle]` metrics | Failed (12 unpaired/66; Euler V-E+F=3) | unchanged or improved | WORSE → cohort shift → ABORT |
| §6.3.6 | F0050 spotlight Status + `[twin-oracle]` metrics | Failed (39 unpaired/417 watertight) | unchanged or improved | WORSE → ABORT |
| §6.3.7 | Yang fast subset `yang_fast` ≥ 10/157 baseline | 10 (per CLAUDE.md memory) | ≥ 10 (expectation: ≥ 11 if F0020 returns; possibly ≥ 12 if F0051 returns) | decreases → ABORT |
| §6.3.8 | Kernel baseline `cargo test -p kernel` | 1250 / 29 ignored / 42 doc (per CLAUDE.md memory) | 1250 / 29 / 42 preserved | passing drops → must fix before ship; never `--no-verify` |
| §6.3.9 | `cargo clippy -p kernel` | clean | clean | new warnings → must fix before ship |
| §6.3.10 | `cargo fmt --check` | clean | clean | new diff → must fix before ship |

### §6.4 Worktree-based baseline comparison (per `feedback_adversary_no_destructive_git.md`)

For §6.3.8 kernel baseline:

```
git worktree add /tmp/y23-baseline-wt 8de94e5
(cd /tmp/y23-baseline-wt && cargo test -p kernel 2>&1 | tail -3)
cargo test -p kernel 2>&1 | tail -3
git worktree remove /tmp/y23-baseline-wt
```

Adversary MUST use this pattern (or `git show 8de94e5:<file>`) — never `git stash` / `checkout --` / `reset --hard` on the live tree.

---

## §7 Failure modes

### §7.1 Implementation fails to drop F0020 `[twin-oracle]` to 0

**Symptom.** F0020 Extrude 3 spotlight still `Status:Failed` post-implementation, with the **same** `validate_yang_result_topology` panic on `half_edge[58].twin = None ... (38->27)`.

**Action.** ABORT. The closure check at L961 was either applied at the wrong site or did not fire on the canary's reproducer chain. Bank as PR-Y24 with diagnostic: dump `chain.first().v0` and `chain.last().v1` for every chain entering `loops.push` to confirm the patch 7 chain is reaching the closure check.

### §7.2 Implementation lands but F0020 fails at a *different* panic string

**Symptom.** F0020 spotlight `Status:Failed` at a panic string that is NOT `validate_yang_result_topology` on `(38, 27)` — for example, an Euler check, a normal-orientation check, a face-count assertion in retessellation, or a downstream stage panic.

**Action.** Per `feedback_no_last_bug.md`: this is a *next-layer* outcome, not a regression. Adversary phase classifies whether the new failure mode is a regression introduced by PR-Y23 (e.g. dropped face caused a watertightness failure that wasn't there before) or a pre-existing layer that PR-Y23 simply unmasked. If pre-existing → ship; bank the new layer for PR-Y24+. If introduced by PR-Y23 → ABORT.

### §7.3 Cohort regression on F0044 / F0030 / F0050

**Symptom.** F0044 batch `[topo-extract]=0` or `[twin-oracle]=0` post-PR-Y22 metrics regress to non-zero; OR F0030 / F0050 transition to a worse Failed mode (e.g. F0030's 12 unpaired count grows; F0050's 39 unpaired count grows; either flips from a known Failed mode to a panic Failed mode).

**Action.** ABORT. The closure check has overshot — option (a) is dropping chains that legitimately resolve later in the pipeline. Bank as PR-Y24 with diagnostic: dump pre-/post-fix diff of dropped vs emitted chain counts on each cohort case; consider whether option (b) (strengthen R3 ownership) becomes the right anchor.

### §7.4 Kernel baseline regression

**Symptom.** `cargo test -p kernel` reports fewer passing tests than 1250.

**Action.** Must fix before ship. Never `--no-verify`. Per CLAUDE.md "P9-P10 Fix It Right or Don't Fix It" — if the cause cannot be explained, ABORT and bank.

### §7.5 Yang fast subset regression

**Symptom.** `yang_fast` Pass count drops below 10 / 157.

**Action.** ABORT. Bank with adversary's full diff of which cases regressed.

### §7.6 Implementation phase modifies the spec or test files

**Symptom.** `impl-z23`'s diff touches `specs/yang_pr_y23_open_loop_emission.md` or `crates/test-harness/tests/pr_y23_open_loop_emission_regression.rs`.

**Action.** Per FIP §5.1: implementation phase MUST NOT modify spec or test. ABORT, re-spawn from clean tree.

---

## §8 Research basis

### §8.1 Yang 2025 (paper-cited)

- **§3 Overview** (extracted at `/tmp/yang2025.txt:240-330`): the B-Rep manifold-edge definition is "with each edge shared by two adjacent faces" (line 252 of extracted text). This is the closed-loop contract for B-Rep face boundaries — every edge participates in exactly one cycle on each side. PR-Y23's invariant I2 ("every emitted chain is closed") is a direct mechanization of this definition at our half-edge construction site.
- **§4.4.2 Mesh and B-Rep Booleans** (extracted at `/tmp/yang2025.txt:574-605`): "Starting from an inner triangle, i.e. not on the boundaries of each mesh patch, using it as a seed triangle for the patch, our algorithm expands the patch by including more neighboring inner triangles, until all the neighboring triangles of the patch are on the boundaries. The boundary curves can then be easily collected and mapped back to the parametric surfaces by fitting the curve in the parametric domain. All the patches are found if all the triangles in the mesh Boolean results are accessed." (lines 588-595 of extracted text). The phrase "boundary curves can then be easily collected" presumes those curves are closed — only closed curves are mappable to parametric face boundaries via "fitting the curve in the parametric domain."

### §8.2 Cherchi 2022 (paper-cited)

- **§3 Overview** (extracted at `/tmp/cherchi2022.txt:232-290`): "the arrangement is guaranteed to be a well formed simplicial complex and surface patches are bounded by closed loops of non-manifold edges, namely the intersection lines." (line 248 of extracted text). This is the canonical statement of PR-Y23's I2 invariant. Open chains as patch boundaries are explicitly excluded by Cherchi's output contract.
- **§5 Inside/Outside Classification** (extracted at `/tmp/cherchi2022.txt:385-470`): provides context on the manifold-edge predicate (`edgeIsManifold` in the C++ reference at `booleans.cpp:412`; mirrored in Waffle at `topology_extract.rs:513-516` `edge_is_manifold`). Cherchi 2022 §5 is NOT load-bearing for PR-Y23's anchor — it describes the per-patch ray-cast classification that runs after patch boundaries have already been collected. The PR-Y23 anchor lies *upstream* of §5's domain (at boundary-loop emission), so §5's role here is contextual: it is a downstream consumer of the closed-loop invariant that I2 enforces.

### §8.3 Citation hygiene (per `feedback_external_coherence.md` + predecessor §5 R-a)

The phrase **"Yang §4.4.2 directional-symmetry"** that appears in PR-Y20-MODE-A's production comments (e.g. `topology_extract.rs:1287-1311` PR-Y22 M1 NMM predicate's contextual block) is editorial inference, not a paper quotation. Yang §4.4.2 does not contain the words "directional symmetry"; it describes "expands the patch by including more neighboring inner triangles" and "boundary curves can then be easily collected" without invoking a directional-symmetry term. The PR-Y22 v2 §5 audit flagged this as a citation-hygiene defect.

PR-Y23 adheres to the corrective discipline:

- §3 ("each edge shared by two adjacent faces") and §4.4.2 ("boundary curves can then be easily collected") are cited **separately** with paper-quoted phrases, not synthesized into a single editorial summary.
- All paper claims in this spec link to the extracted-text line ranges in `/tmp/yang2025.txt` and `/tmp/cherchi2022.txt`.
- The PR-Y23 implementation MUST NOT introduce new occurrences of "Yang §4.4.2 directional-symmetry" in production comments. Comments at the new closure check site MUST cite "Yang §3 (line: 'each edge shared by two adjacent faces')" and "Cherchi 2022 §3 (line: 'surface patches are bounded by closed loops of non-manifold edges')" verbatim.

### §8.4 Deviation statement (per FIP §3.2 #7)

PR-Y23 introduces no deviation from the published technique. The fix restores Yang 2025 §3 and Cherchi 2022 §3's mandated patch-boundary closed-loop invariant at the site (`topology_extract.rs:961`) where the existing implementation violates it. The fix is the published technique; the violation was the deviation.

### §8.5 Analytical vs. Approximate Method Justification (per FIP §3.2 #7a)

Not applicable. PR-Y23 is a topology-layer fix on a discrete half-edge data structure; no surface-surface intersection is performed. The closure check operates on integer canonical-vertex indices; no tolerance is involved.

---

## §9 Anti-scope (explicit OUT)

- **F0050 normals + Euler** — different defect class. Pre-PR-Y23 metrics: 39 unpaired / 417 watertight. PR-Y24+ candidate.
- **F0030 still-Failed status** — different defect class. Pre-PR-Y23 metrics: 12 unpaired / 66; Euler V-E+F=3. PR-Y23's I3 invariant does not promise F0030 movement; oracle §6.3.5 only requires non-regression.
- **F0044 batch `[topo-extract]=0` and `[twin-oracle]=0`** — already at target on PR-Y22 GREEN. PR-Y23 must NOT re-touch the PR-Y22 anchors at L468-491 (M2 canon-degen filter) or L1287-1311 (M1 NMM-incidence predicate). The anti-scope is enforced by oracle §6.3.3 and §6.3.4.
- **Higher-n open chains' contribution to `[yang-diag] 39/169 unpaired`** — separate concern banked for PR-Y24+. Canary §4 banked-finding #4 documented that F0020 boolean #2 produces open chains at multiple patches (13, 14, 15, 16, 10, 11, 12, 22, 23, etc.), but only the n=2 case at patch 7 manufactures a `[twin-oracle]`-visible orphan reverse. PR-Y23's closure check at L961 will drop ALL open chains, not just the n=2 case. The **side-effect** of dropping the higher-n open chains is in scope (it reduces face-count); but the **contribution-to-`[yang-diag]`** analysis is out of scope.
- **Layer 2 (Step 7 circular `next`-ring at L1131-1146)** — downstream consumer; correct on closed inputs. Per canary §3 layer table: "fixing layer 2 would break legitimate closed loops." Not anchor.
- **Layer 3 (`[twin-oracle]` arena-traversal keying at L1445-1449)** — downstream consumer; correct on closed inputs. Per canary §3 layer table: "the bug is upstream." Not anchor.
- **R3 ownership pre-pass at L810-863** — option (b) of branch table §4 considered modifying R3; option (a) was selected. R3 stays untouched. Adversary phase MUST verify no incidental drift in R3 metrics (e.g. R3 tie-break decision count).
- **PR-Y19-MODE-B soft-break at L948-958** — left in place. The soft-break correctly handles the inner-walk termination when `outgoing` is empty; it is the outer-emit (L961) that emits the resulting open chain. PR-Y23 does NOT modify L948-958.
- **PR-Y17-COPLANAR L264 panic, PR-Y18 L264-class, PR-Y20-MODE-A NMM `Option<HalfEdgeIdx>` type, PR-Y22 M1 NMM-incidence + M2 canon-degen** — all stay in force; PR-Y23 builds on them.
- **`stitch.rs` legacy path** — DEPRECATED per CLAUDE.md A15 deprecation block.
- **5 L264 panic cases (R0014/R0046/R0055/R0081/F0075)** — different mechanism; PR-Y18 territory.
- **F0086, F0031-F0040, R0020/R0021, R0071** — out of scope.
- **Reference C++ sidecar comparison (Cherchi 2022 reference repo)** — NOT load-bearing for PR-Y23. The fix is a one-line invariant restoration with paper-citation grounding (Yang §3 + Cherchi 2022 §3); internal `[twin-oracle]` measurement + canary's empirical anchor naming are sufficient. Sidecar parity remains in scope for upstream subdivision-correctness investigations; not for this PR.
- **Fillet / chamfer / shell** — DEFERRED INDEFINITELY per CLAUDE.md.

---

## §10 NO fallback paths (per `feedback_yang_only.md`)

- **No silent open-chain swallow.** The closure check at L961 EITHER pushes the closed chain OR drops the open chain. There is no "try to close" fixup pass, no synthetic edge insertion, no tolerance-based "close if endpoints are within ε". An open chain is a paper-invariant violation that PR-Y23 acknowledges by *not* propagating it; no recovery path is specified because no recovery path is paper-faithful.
- **No special-case branches per case.** The closure check fires for EVERY chain; F0020 patch 7's n=2 chain is not specially handled relative to F0020's higher-n chains or F0044's chains.
- **No tolerance widening.** The closure check is on integer canonical-vertex indices: `chain.last().v1 == chain.first().v0` is a strict integer equality.
- **Gated diagnostic logging is allowed.** A `TWIN_DEBUG=1`-gated `eprintln!` at the drop site, identifying which chain was dropped and from which patch / source-face, is permitted (mirrors PR-Y22 M1's gated logging convention). Not a fallback — diagnostic only.

---

## §11 FIP role table

Per `feedback_oracle_credibility_via_role_separation.md`: NO agent performs more than one sub-phase. Per `feedback_per_plan_cycle_team.md`: TeamCreate at execute-start; TeamDelete at ship/ABORT.

| Sub-phase | Agent | Reads required | Writes |
|---|---|---|---|
| 0a Canary | canary-z23 | Yang §3 + §4.4.2; Cherchi 2022 §3 + §5; `topology_extract.rs:404-1552`; `pr_y22_mode_a_missing_validation_v2.md`; plan §"Canary"; PR-Y19 spec | empirical probe + canary memo at `docs/audits/pr_y23_anchor_canary.md` (DONE — committed `990571c`) |
| 0b Spec | spec-z23 (this agent) | Same papers; canary memo from 0a; `feedback_yang_only.md`, `feedback_no_last_bug.md`; FIP §3 + §8 | this spec |
| 0c Test | test-z23 | Same papers; this spec; `pr_y22_mode_a_missing_regression.rs` as helper template; `feedback_validate_against_corpus.md` | RED regression test asserting I1 + I2 + I3 |
| 0d Implement | impl-z23 | Same papers; this spec; failing test from 0c; canary's named anchor; CLAUDE.md "P9-P10"; `feedback_implementer_anti_fabrication_diff.md` | Closure check at `topology_extract.rs:961` (production code only — NOT spec, NOT test) |
| 0e Adversary | adv-z23 | Same papers; all 0a-0d artifacts; `feedback_adversary_recommendations_need_canary.md`; `feedback_adversary_no_destructive_git.md` | Validation memo at `docs/audits/pr_y23_validation.md` |
| 0f Close-out | lead-z23 | All 0a-0e artifacts; DoD; CLAUDE.md WASM two-step | clippy/fmt + WASM rebuild + commit + TeamDelete |

**Spec-z23's role ENDS at writing this document.** Spec-z23 does not implement, test, or adversary. Per FIP §5.1 + plan §"Anti-rules": no agent performs more than one sub-phase.

---

## §12 Recommendations to test-z23

(Non-binding; test-z23 builds assertions from this spec's §5/§6, not from this section. Listed here so test-z23 has a starting point.)

1. **New test file:** `crates/test-harness/tests/pr_y23_open_loop_emission_regression.rs` — modeled on `pr_y22_mode_a_missing_regression.rs`. Reuse helpers `capture_stderr` / `max_twin_oracle_field` / `count_twin_oracle_lines` from the predecessor regression test.
2. **Two RED tests required:**
   - `pr_y23_f0020_twin_oracle_zero` — asserts MAX `[twin-oracle] unpaired_count` across F0020 invocations == 0. Pre-fix red baseline: MAX == 2 on `8de94e5`.
   - `pr_y23_f0044_twin_oracle_no_regression` — asserts F0044 batch `[twin-oracle] unpaired_count` stays at 0. Pre-fix should already pass on `8de94e5` (this is a non-regression bound, not a RED-then-GREEN test); test-z23 may treat this as a sanity assertion that runs alongside, or skip if the FIP §4.3 "no panic-only tests" rule disqualifies it. Recommend: keep, with explicit numeric assertion `unpaired_count == 0`.
3. **EXTEND `assay_randomized.rs` `spotlight_f0020`:** promote the assertion from current Status check to `Status::Passed` once impl phase confirms PR-Y23 will deliver it. Test-z23 may write the promotion as a comment marking "post-PR-Y23" and leave the test as it is on `8de94e5` if writing the failing assertion would mask other unrelated test failures.
4. **Per FIP §4.3:** numeric assertions only; no "does it panic?" tests. The `[twin-oracle] unpaired_count` is a numeric line in stderr; assertions against it satisfy §4.3.
5. **Red-phase log capture:** test-z23 MUST run the new tests on `8de94e5` (PR-Y22 baseline, before any PR-Y23 production code) and capture stderr verbatim to demonstrate RED phase. The captured log should be referenced (or quoted) in the close-out memo at `docs/audits/pr_y23_validation.md`.
6. **Test isolation:** new test file must be `#[ignore]`-gated and require `YANG_BOOLEAN=1` + `TWIN_DEBUG=1` env, mirroring predecessor pattern. The `--test-threads=1` requirement in §6 oracles applies here too (stderr capture is process-local; multi-threaded test runs interleave output).

---

## §13 Definition of Done (per `governance/DEFINITION_OF_DONE.md`)

`lead-z23` verifies before commit:

- [ ] Spec at `specs/yang_pr_y23_open_loop_emission.md` with all FIP §3.2 sections (§§1-9 + applicable optional)
- [ ] Test at `crates/test-harness/tests/pr_y23_open_loop_emission_regression.rs`
- [ ] Tests demonstrably failed pre-fix on `8de94e5` (red-phase log captured in close-out memo)
- [ ] Implementation did not modify spec or test files (FIP §5.1)
- [ ] All §6 Oracles gates pass (§6.3.1 through §6.3.10)
- [ ] Adversary ACCEPT in `docs/audits/pr_y23_validation.md`
- [ ] CI green
- [ ] WASM rebuilt per CLAUDE.md two-step build
- [ ] TeamDelete issued
