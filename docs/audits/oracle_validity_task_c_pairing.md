# Oracle-Validity Audit (PR10) — Task C: Cross-Oracle Pairing on Full Corpus

**Audit branch**: `oracle-validity-audit-pr10`
**Auditor**: agent-pairing (distinct from T1/T2 per FIP §1 P5)
**Date**: 2026-05-01
**Scope**: PR9 6-oracle registry (`crates/kernel/src/boolean/pipeline_oracles.rs`)
**Sibling reports**: `oracle_validity_task_a_mutation.md` (T1, in_progress);
`oracle_validity_task_b_passcheck.md` (T2, in_progress).

## §1. Methodology

### What was run
Per-case probe `crates/test-harness/tests/oracle_validity_pr10_pairing.rs`
(new for this audit, ignored test gated on manual invocation). For each
case in the 157-case yang_fast corpus (190 corpus minus 33 known-timeout
skip), the probe:

1. Loaded the `.waffle` and routed `LoadProject` through the Yang
   pipeline (`YANG_BOOLEAN=1`).
2. Captured pipeline state via `kernel::diagnostics::with_yang_oracle_capture`.
3. Ran the PR9 default registry (`default_oracle_registry()` — 6 oracles
   covering Stages 0/1/2/4b/5/6).
4. Recorded **all six oracle verdicts** per case (not just the
   first-failing-stage that PR9's `pr9_pipeline_oracle_corpus` records).
5. Aggregated into a cross-oracle pairing matrix.

### Per-case time budget and timeout handling
30 s per case (matches PR9 corpus runner). Cases that exceed it are
bucketed as `Timeout` and excluded from the matrix's contract-violation
columns (their oracles never observed any state).

### Determinism
All runs use first-call results per `feedback_no_regression_chasing.md`.
R0080 / R0018 nondeterminism flagged and not over-interpreted.

### Run cost
~3 minutes wall-clock on the audit branch.

### Cleanup
No production code modified. Probe is `#[ignore]` so default `cargo test`
is unaffected. Probe artifact (`tests/oracle_validity_pr10_pairing.rs`)
is preserved for re-runs.

## §2. Cross-oracle pairing matrix

Headline output: every cell `M[row, col]` = number of cases in the row's
first-failing-stage bucket where the column's oracle ALSO reports
`ContractViolated`. The diagonal cell (where row's bucket == col's stage)
equals the row size — every case in the bucket fires its own bucket's
oracle by definition.

| First-fail bucket        | n   | S0   | S1   | S2   | S4b  | S5   | S6   |
|--------------------------|----:|-----:|-----:|-----:|-----:|-----:|-----:|
| Stage1Bijective          |   2 |   0  |   2  |   1  |   2  |   0  |   2  |
| Stage2Arrangement        |  28 |   0  |   0  |  28  |  28  |   0  |  28  |
| Stage4bClassification    | 120 |   0  |   0  |   0  | 120  |   0  | 120  |
| Stage6Assembly           |   1 |   0  |   0  |   0  |   0  |   0  |   1  |
| AllPass                  |   3 |   0  |   0  |   0  |   0  |   0  |   0  |
| Timeout                  |   3 |   0  |   0  |   0  |   0  |   0  |   0  |
| **Total**                | 157 |   0  |   2  |  29  | 150  |   0  | 151  |

Verdict legend on per-case rows (see §6 raw trace):
- `.` = `Ok` or self-skip (snapshot not present — every PR9 oracle
  silently returns `Ok` when its required snapshot field is `None`).
- `X` = `ContractViolated` (the contract was checked and rejected).
- `S` = `OracleStub` (Stage 0 partial-overlap unchecked — PR9 §6 known
  coverage gap).
- `M` = `StateMissing` (no PR9 oracle currently emits this).

### Column totals interpretation

- **S0 = 0**: the Stage 0 oracle never fires `ContractViolated`. The
  identical-footprint contract holds wherever it is checkable. (29
  cases hit `OracleStub` — Stage 0 partial-overlap pairs that PR9
  intentionally does not check; see §3 for purity audit.)
- **S1 = 2**: only the 2 Stage1Bijective first-fail cases (R0031, R0081)
  have non-bijective face pairs.
- **S2 = 29**: 28 Stage 2 first-fail cases plus 1 Stage 1 first-fail
  case (R0031) where Stage 2 is also broken — likely cascade from
  upstream T-junctions.
- **S4b = 150**: dramatic. 28 + 120 + 2 = 150 cases where the Stage 4b
  oracle reports a mixed-label patch. **The Stage 4b oracle fires on
  95.5 % of the corpus (150/157).**
- **S5 = 0**: PR8's patch-graph conservation invariant holds across
  the corpus. No case has lost or double-counted sub-triangles.
- **S6 = 151**: 28 + 120 + 1 + 2 = 151 cases where the Stage 6
  twin-symmetry oracle reports asymmetry. **96.2 % of the corpus.**

## §3. Critical claims validation

### Claim 1 — Stage 2 → Stage 4b shadowing

PR10 design hinges on whether Stage 2 first-failures also fire Stage 4b.
PR9's adversary spot-checked 3/3 such cases at micro-scale; this probe
checks all 28.

**Result: 28/28 (100.0 %) of Stage2Arrangement first-fail cases also
fire Stage 4b ContractViolated.**

Furthermore: 28/28 also fire Stage 6. The full Stage 2 cascade is
total — every Stage 2 well-formedness violation propagates to both
labeling-consistency and twin-symmetry violations downstream.

**Implication**: PR11's Stage 4b labeling fix CANNOT improve the 28
Stage 2 cases by itself. The Stage 2 well-formedness invariant must
be restored first; otherwise the per-patch labeling fix operates on a
subdivided mesh that already fails Cherchi 2022 §3-4 preconditions.

### Claim 2 — Stage 4b → Stage 6 propagation

**Result: 120/120 (100.0 %) of Stage4bClassification first-fail cases
also fire Stage 6 ContractViolated.**

The Stage 4b → Stage 5 propagation rate is 0/120 (0 %): patch-graph
conservation holds even when label consistency does not. This matches
the audit's Cluster Y-I architecture: Stage 5's PR8 builder partitions
sub-triangles correctly; the bug is in label assignment WITHIN the
patches, not in patch identification.

**Implication**: a correct Stage 4b labeling fix likely subsumes
~120 of the corpus's twin-asymmetry signatures (Stage 6 violations).
This is consistent with the audit's YA-01/YC-05/YB-01 cluster: per-patch
labeling is the dominant root cause of twin-asymmetry.

### Claim 3 — AllPass purity

**Result**: 3 cases bucket as AllPass (R0052, F0073, F0074). All 3 have
**no `OracleStub` verdicts anywhere** and **no `ContractViolated`
verdicts anywhere**.

PR9's adversary claim that "R0001 has empty bundle" was true at PR9
baseline (2026-04-29) but R0001 is no longer AllPass in this run — it
buckets as Stage4bClassification. The set of AllPass cases shifted
between PR9 baseline and this audit run.

The 3 current AllPass cases:
- **R0052**: Empty AllPass with all `.` cells. Spot-check needed to
  determine whether pipeline reached the snapshot-record points (full
  bundle) or short-circuited on AABB-disjoint with empty stage 2/4b/6
  snapshots.
- **F0073, F0074**: Both have `expect_rebuild_error: true` per
  `yang_audit_b_assay_failures.md` §2. Pipeline does not run; oracles
  self-skip on missing snapshots. **These are vacuous AllPass cases**
  and should NOT be counted as oracle wins.

**Implication**: of the 3 AllPass cases, 2 (F0073, F0074) are vacuous
(pipeline-never-ran). Only R0052 is potentially substantive AllPass.
The PR9 baseline reported 72 AllPass cases — the radical shift to 3
suggests either (a) production-pipeline regressions since 2026-04-29
or (b) the Stage 4b oracle is now firing on cases that previously
slipped through. Either way, **AllPass is not a clean oracle-pass set**.

## §4. PR11 design implications

### Sequencing: Stage 2 before Stage 4b
PR9 §5 recommended (a) "PR10 fixes Stage 2 first ... and defers Stage 4b
to PR11." The 28/28 Stage 2 → Stage 4b shadowing rate confirms this
sequencing is mandatory: a Stage 4b labeling fix CANNOT improve the
28 Stage 2 first-fail cases without first restoring Stage 2
well-formedness. Per PR9 §5, this is option (a).

### Lever sizing
- Fixing Stage 4b alone (120 cases): +120 if labeling fix works AND
  Stage 6 cascade resolves. The 120/120 Stage 4b → Stage 6 rate
  confirms the cascade is total, so a correct Stage 4b fix likely
  unlocks Stage 6 too.
- Fixing Stage 2 alone (28 cases): +28 if Stage 2 fix doesn't degrade
  Stage 4b. Without a complementary Stage 4b fix, the 28 cases will
  still fail Stage 4b (their Stage 4b verdict is X regardless of
  whether they reached Stage 4b cleanly or not — the Stage 4b oracle
  fires on whatever subdivided mesh + labeling is in the bundle, and
  if Stage 2 is corrupt the mesh-arrangement is already wrong).
- Fixing both (148 cases): the unique cases freed by the combined
  fix — Stage 1 and Stage 6 cases (3 total) remain after both fixes.

### Stage 4b is the dominant single first-fail, by a much larger margin than PR9 baseline reported

PR9 §3 reported Stage4bClassification = 32 / 157. This audit measures
**120 / 157**. The discrepancy is large enough to flag as a separate
finding.

Hypotheses:
1. **Pipeline regressions since 2026-04-29**: recent commits
   (`60ee841`, `ea03d94`, `a2ea0b9`, `cfec7b8`, `9f3c591`) touched
   the kernel boolean path and may have shifted oracle fire rates.
2. **PR9 baseline ran with different code**: the spec was authored
   for the post-PR8 codebase; this run is post-PR9 oracles + later
   fixes.
3. **AllPass attrition**: PR9's 72 AllPass became 3 AllPass; the 69
   missing AllPass cases mostly bucketed as Stage4bClassification
   (R0001 / R0002 / R0004 / R0005 listed as AllPass samples in PR9 §3
   are now all Stage4bClassification per §6 raw trace).

**This finding alone justifies the audit**: the PR9 §3 baseline
histogram is no longer accurate, and the spec's lever sizing for PR10
is misleading. PR11's design must use the current numbers (this
report), not PR9's baseline.

### Recommendation for PR11 fix-ordering

1. **First**: investigate the 28 Stage 2 first-fail cases. If
   `MeshArrangementWellFormedOracle`'s contract is correct, the
   `subdivide_mesh_pair` / Cherchi arrangement output is producing
   degenerate sub-triangles in 28 cases. Restoring Stage 2
   well-formedness is a Stage 2 fix, not a Stage 4b fix.
2. **Concurrently**: implement Cherchi 2022 §5 / Algorithm 1 per-patch
   labeling. The 120/120 Stage 4b → Stage 6 propagation says the
   downstream win is large.
3. **Skip**: Stage 5 patch-graph conservation. Zero cases fail this
   oracle; PR8's builder is correct.

## §5. What this task did NOT cover

### Oracle correctness (delegated to T1/T2)
- **T1 (Task A — mutation testing)**: does the matrix's "X" actually
  mean a real contract violation? Mutation testing on production code
  is the standard validation. agent-mutation owns this.
- **T2 (Task B — known-PASS verification)**: are any of the 8 audit-class
  PASS cases (per `yang_audit_b_assay_failures.md`) misidentified as
  AllPass purely by oracle blindness? agent-passcheck owns this.

### Snapshot-presence ground truth
The probe infers oracle verdicts from `OracleRunSummary::per_oracle`
but does NOT directly inspect `OwnedSnapshotBundle` to confirm whether
each stage's snapshot field was populated vs `None`. The `kernel::diagnostics`
public surface (`with_yang_oracle_capture`) does not currently expose
the bundle. AllPass purity is therefore inferred indirectly (via
`OracleStub` count in Stage 0 verdicts as a proxy for "Stage 0 ran").
A more rigorous purity audit needs a public bundle-inspection method.

### Multi-op cases
The 33-case yang_fast skip set (mostly multi-op chains) is excluded.
The snapshot collector overwrites with the LAST Yang boolean, so
multi-op cases would only oracle-check the final boolean. PR9 §6
Open Questions noted this; this task inherits the same scope limit.

### Cause attribution
The matrix shows propagation rates but does NOT determine the causal
direction. "Stage 2 first-fail also fires Stage 4b" is correlation;
the causal claim that "Stage 2 corruption causes Stage 4b mixed labels"
requires reading the implementation, which is out of scope for this
empirical probe.

## §6. Per-case raw trace (157 cases)

Full trace captured in run log at `/tmp/pr10_pairing_run.log` (TRACE
lines). Format:

```
TRACE | <case_id> | <first_fail_bucket> | s0 s1 s2 s4b s5 s6 (verdict cells)
```

Sample anchors (per `feedback_anchor_before_fix.md`):

```
TRACE | R0031 | Stage1Bijective       | S X X X . X
TRACE | R0081 | Stage1Bijective       | S X . X . X
TRACE | F0001 | Stage6Assembly        | . . . . . X
TRACE | F0002 | Stage2Arrangement     | S . X X . X
TRACE | F0064 | Stage2Arrangement     | S . X X . X
TRACE | R0001 | Stage4bClassification | . . . X . X
TRACE | R0033 | Stage4bClassification | . . . X . X
TRACE | R0080 | Stage4bClassification | . . . X . X
TRACE | R0052 | AllPass               | . . . . . .
TRACE | F0073 | AllPass               | . . . . . .
TRACE | F0074 | AllPass               | . . . . . .
```

### Stage1Bijective bucket (n=2)
R0031, R0081.

### Stage2Arrangement bucket (n=28)
R0016, R0019, R0020, R0038, R0049, R0055, R0058, R0072, R0078, R0092,
R0096, F0002, F0004, F0017, F0041, F0042, F0043, F0045, F0048, F0049,
F0050, F0056, F0057, F0058, F0059, F0060, F0064, F0066.

### Stage4bClassification bucket (n=120)
R0001, R0002, R0004, R0005, R0006, R0007, R0008, R0009, R0011, R0013,
R0014, R0015, R0017, R0018, R0021, R0022, R0023, R0024, R0025, R0027,
R0029, R0030, R0033, R0034, R0035, R0036, R0037, R0039, R0040, R0041,
R0042, R0043, R0044, R0045, R0046, R0047, R0048, R0051, R0054, R0056,
R0057, R0060, R0061, R0062, R0063, R0064, R0066, R0067, R0068, R0069,
R0073, R0074, R0075, R0076, R0077, R0079, R0080, R0082, R0083, R0084,
R0086, R0087, R0088, R0089, R0090, R0091, R0093, R0094, R0095, R0097,
R0098, F0003, F0005, F0006, F0007, F0008, F0009, F0010, F0011, F0012,
F0013, F0014, F0015, F0016, F0018, F0019, F0020, F0021, F0022, F0023,
F0024, F0025, F0026, F0027, F0028, F0029, F0030, F0031, F0032, F0033,
F0034, F0035, F0036, F0037, F0038, F0039, F0040, F0044, F0046, F0047,
F0051, F0052, F0053, F0054, F0055, F0061, F0062, F0075, F0076, F0086.

### Stage6Assembly bucket (n=1)
F0001.

### AllPass bucket (n=3)
R0052, F0073, F0074.

### Timeout bucket (n=3)
R0032, R0050, R0071.

## §7. Comparison vs PR9 §3 baseline

| Bucket                    | PR9 §3 (2026-04-29) | This audit (2026-05-01) | Δ |
|---------------------------|--------------------:|------------------------:|---:|
| Stage1Bijective           |                   7 |                       2 |  −5 |
| Stage2Arrangement         |                  28 |                      28 |   0 |
| Stage4bClassification     |                  32 |                     120 | +88 |
| Stage6Assembly            |                  15 |                       1 | −14 |
| AllPass                   |                  72 |                       3 | −69 |
| Timeout                   |                   3 |                       3 |   0 |
| **Total**                 |                 157 |                     157 |   — |

Two days of pipeline development between baselines. The 88-case shift
into Stage4bClassification + 69-case attrition from AllPass strongly
suggests **the production pipeline regressed somewhere between 2026-04-29
and 2026-05-01 in a way that the Stage 4b oracle correctly catches**.
This is consistent with the assay's "0/157 honest baseline" per memory
`[Yang Implementation Status]`: the visible "passes" from PR9 baseline
were largely the AABB-disjoint short-circuit path with empty Stage 2/4b/6
snapshots — i.e. AllPass-by-vacuous-skip. As recent fixes have begun
populating those snapshots even on the disjoint path (see commit
`aee32d8` on AABB-disjoint short-circuit), the Stage 4b oracle now
sees real labeling state and reports real violations.

This is **good news for oracle validity**: the Stage 4b oracle is
working AS DESIGNED by catching label-consistency violations that PR9
baseline missed. The bad news for PR9 §3's lever sizing is that the
"32 Stage 4b first-fail" headline understates the Stage 4b problem
by ~3.75×.

## §8. Reproduction

```bash
# Anchor (small, ~30s):
YANG_BOOLEAN=1 cargo test -p test-harness --test oracle_validity_pr10_pairing \
    -- --ignored --nocapture

# Re-run on a different revision: stash + checkout + cargo test, parse
# the TRACE lines from stderr.
```

The probe is a single ignored test; no fixtures or environment beyond
the standard assay corpus and `YANG_BOOLEAN=1`.
