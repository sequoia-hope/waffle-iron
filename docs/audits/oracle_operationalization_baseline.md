# Oracle Operationalization Baseline — F0020 Stage 1 bijection failure surfaces

## Status: Phase 1-3 COMPLETE; ready to drive subsequent fix cycles oracle-first

## Methodological context

The prior ~20 investigation cycles (canary-driven, hypothesis-based) produced 5 consecutive ABORTs (PR-Y25 through PR-Y28) and several refuted fix shapes (PR-Y34 cinolib refactor reverted; Tier B path γ refuted by Y57). The pattern was: infer mechanism from downstream symptoms, build canary to test, refute, pivot, repeat. Cumulative success rate ≈ 1 in 6.

The user's diagnosis ("our method is broken; the hypothesis is never right"): hypothesis-driven investigation without ground truth at each stage. Each canary tests a guess; pivots reset rather than accumulate.

**Structural fix shipped this cycle**: operationalize the existing `default_oracle_registry` so failures attribute to a Yang pipeline stage's invariant violation, not to end-to-end metrics. The oracle infrastructure (`pipeline_oracles.rs`, 6 oracles, `with_yang_oracle_capture`) already existed but was `#[ignore]`-gated and only produced aggregate histograms.

## Phase 1 baseline (157-case corpus, current main `ed656d6`)

| Bucket | Count | Sample IDs |
|---|---|---|
| `Stage1Bijective` first-fail | 24 | R0007, R0015, R0020, R0023 |
| `Stage2Arrangement` first-fail | 29 | R0016, R0019, R0038, R0046 |
| **AllPass** (all 6 oracles pass) | **100** | R0001, R0002, R0004, R0005 |
| Timeout | 4 | R0032, R0044, R0071, R0081 |

**Observation**: 100 of 157 cases pass all 6 oracles but only 12 pass `yang_fast` end-to-end → ~88 cases have failures in UNCOVERED stages. The oracle suite covers the first 2 stages strongly but misses the dominant failure modes in stages 3+ (SSI refinement, mesh updating).

## Phase 2 F0020 spotlight oracle verdict (NEW)

Test `spotlight_f0020_oracles` (added to `crates/test-harness/tests/assay_randomized.rs`) runs F0020 through `with_yang_oracle_capture` and emits per-oracle verdicts.

```
=== F0020 Oracle Verdict (Phase 2) ===
case_id = F0020
first_failing_stage = Some(Stage1Bijective)
  [Stage0Coplanar]      CoplanarMeshIdenticalOracle    : PASS
  [Stage1Bijective]     BijectiveFacePairOracle        : FAIL  (non-bijective face pairs: operand A 5 pair(s) of 33, operand B 0 pair(s) of 12)
  [Stage2Arrangement]   MeshArrangementWellFormedOracle: PASS
  [Stage4bClassification] LabelConsistencyWithinPatchOracle: PASS
  [Stage5PatchSegment]  ManifoldPatchConservationOracle: PASS
  [Stage6Assembly]      TwinSymmetryOracle             : PASS
Oracle verdict: 1 contract violation(s); fix order = lowest stage first
```

**The actual F0020 root cause**: operand A has **5 of 33 non-bijective face pairs** at Stage 1 (Yang §4.1.1 bijectivity contract). All subsequent stages PASS at the current oracle resolution.

This is the answer the prior ~20 cycles missed. Every canary we built (Y48-Y57) was investigating downstream symptoms (37 collisions, 47 unpaired edges, 30 degenerate triangles) that are CASCADES of the Stage 1 bijection failure. Twin-pairing (Tier B path γ) passes its oracle — the prior cycle's framing that "twin-pairing has same-direction pairs" was a downstream mis-attribution.

## Phase 3 coverage map

See `docs/audits/yang_oracle_coverage.md` for the full Stage ↔ Oracle table. Summary:

- **Covered**: 6 of 8 Yang pipeline stages (`Stage0`, `Stage1`, `Stage2`, `Stage4b`, `Stage5`, `Stage6`)
- **Uncovered**: `Stage3SsiRefinement` (§4.3), `Stage4aMeshUpdating` (§4.4.1)
- **Reference parity** (Cherchi C++ comparison) is a separate oracle class; PR-Y33's one-shot harness exists but isn't continuous

The ~88 AllPass-but-fail cases likely have bugs in uncovered Stage 3/4a OR in downstream code (tessellation, render-mesh dedup) not yet oracle'd.

## Decision gate — fix order for subsequent cycles

Each subsequent plan cycle picks ONE failing oracle and writes a fix plan that:
1. Cites the oracle violation message as the bug being fixed
2. Includes a regression test that asserts the violation count goes to zero
3. Verifies cohort-wide impact (does fixing this oracle for F0020 also fix it for the 23 other Stage1Bijective-failing cases?)

**Immediate next target**: `Stage1Bijective: BijectiveFacePairOracle` failing on F0020. The violation message says "operand A 5 pair(s) of 33" — that's 5 specific face_idx pairs in operand A's mesh whose bijection breaks. The fix anchor is wherever the bijective tessellation produces these breakages.

This replaces the planned "Tier B path δ" (over-fragmented coplanar same-mesh faces) as the next anchor. Path δ would have been investigating Stage 5/6 symptoms — but Stages 5/6 pass on F0020. The bug is upstream of where path δ was scoping.

## Lessons banked

1. **The oracles were sitting there** the whole time. The methodological failure wasn't lack of oracles; it was failure to consult them. The `pr9_pipeline_oracle_corpus` test had this data; it just wasn't surfaced per-case.

2. **Mechanism inference is the antipattern**. Inferring "twin-pairing creates same-direction pairs" from a histogram of face-pair patterns is mechanism inference. The corresponding oracle (`TwinSymmetryOracle`) was PASSING the whole time — directly contradicting the inference. We didn't check.

3. **End-to-end metrics are the wrong attribution level**. "47 unpaired edges in render mesh" doesn't say WHERE the bug is. Per-stage oracle verdicts do.

4. **Cumulative knowledge accumulates via oracles, not via canary refutations**. Each canary cycle's "Phase 1 hypothesis (a-d)" was disposable; each oracle's verdict is permanent and reusable.

5. **Coverage gaps are explicit now**. We know exactly which stages lack oracles (Stage 3, Stage 4a). Future Yang work fills these gaps explicitly, not implicitly.

## What this enables for future work

- **Spotlight oracle tests** for each cohort case (F0030, F0044, F0045, R0092 — pattern is identical to F0020)
- **Continuous oracle running** — promote `pr9_pipeline_oracle_corpus` out of `#[ignore]` once a baseline GREEN cohort exists
- **Stage-specific test gating** — once Stage 1 is GREEN across the corpus, gate PRs on it; same for Stage 2, etc.
- **Reference-parity oracle** (next major infrastructure shift) — promote PR-Y33's per-stage byte-diff into a continuous oracle

## Verification commands

```bash
cd /home/claude/workspace

# Baseline (already run; logged)
YANG_BOOLEAN=1 cargo test -p test-harness --test pr9_pipeline_oracle_corpus -- --ignored --nocapture

# Per-case attribution (the new entry point)
YANG_BOOLEAN=1 cargo test -p test-harness --test assay_randomized -- spotlight_f0020_oracles --ignored --nocapture

# Regression checks unchanged
cargo test -p kernel --lib                                              # expect 1249/34/42
YANG_BOOLEAN=1 cargo test -p test-harness --test assay_randomized -- spotlight_f0020 --ignored --nocapture
# expect: 47 unpaired, 30 degen, 175 tris (unchanged — Phase 2 is instrumentation only)
```

## Files changed in this cycle

- `crates/test-harness/tests/assay_randomized.rs` — added `spotlight_f0020_oracles` (~95 LOC test + helper)
- `docs/audits/yang_oracle_coverage.md` — NEW (Phase 3 coverage map)
- `docs/audits/oracle_operationalization_baseline.md` — NEW (this memo)
- `CLAUDE.md` — section update pointing to oracle-first investigation
- Memory `MEMORY.md` — operational shift entry

No production code modified. WASM bundle unchanged.
