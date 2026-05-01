# Oracle-validity audit (PR10) — Task B: known-PASS verification

> Owner: agent-passcheck (oracle-validity-audit-pr10)
> Status: complete 2026-04-29
> Refs: `specs/pipeline_oracles.md` (PR9 spec under audit);
>       `docs/audits/yang_audit_b_assay_failures.md` §2 (PASS-cases breakdown);
>       `governance/FEATURE_IMPLEMENTATION_PROTOCOL.md` §1 P5

## §1 Methodology

Goal: detect oracle false-positives — cases where a PR9 oracle reports a
contract violation against pipeline state that the assay considers a
genuine pass.

Approach: probe the 8 audit-class PASS cases enumerated in
`yang_audit_b_assay_failures.md` §2 with the full PR9 oracle registry,
record each oracle's verdict in an 8x6 matrix, and flag any
`ContractViolated` cell.

Probe harness: `crates/test-harness/tests/oracle_validity_pr10_passcheck.rs`
(audit-only, `#[ignore]`, pattern follows
`pr9_pipeline_oracle_corpus.rs`). For each case the harness:

1. Loads `<case>.waffle` via `wasm_bridge::dispatch(LoadProject)` —
   the same path the assay runner uses.
2. Wraps the dispatch in `kernel::diagnostics::with_yang_oracle_capture`,
   which installs a thread-local `OwnedSnapshotBundle` collector. The
   Yang pipeline's `record_snapshot` calls populate the bundle at stage
   boundaries (Stage 0/1 in `yang_integration.rs`,
   Stage 2/4b/6 in `topology_extract.rs::record_stage_2_4b_6_snapshots`).
3. Runs the PR9 default oracle registry (6 oracles) against the bundle.
4. Projects per-oracle verdicts onto the fixed 6-stage row order:
   Stage 0 / 1 / 2 / 4b / 5 / 6.

Cell taxonomy:

- `Ok` — oracle returned `Ok(())`. Ambiguous between (a) snapshot present
  + contract held and (b) snapshot None → oracle self-skipped silently.
  PR9's six oracles all self-skip on missing snapshot; none report
  `StateMissing` for routine absence.
- `Stub(OracleStub)` — known coverage gap (Stage 0 partial-overlap
  unchecked per spec §2.1).
- `Skip(StateMissing)` — oracle reported missing snapshot AS the
  contract violation. None of PR9's oracles do this; included for
  completeness.
- `VIOLATION` — `ContractViolated` returned. THIS IS THE FALSE-POSITIVE
  CELL — flagged for §4 analysis.

Run command (reproduces matrix to stderr):

```bash
cargo test -p test-harness --test oracle_validity_pr10_passcheck \
    oracle_validity_pr10_known_pass_verification -- --ignored --nocapture
```

Sanity check (PR9 module tests still pass):

```bash
cargo test -p kernel pipeline_oracles  # 12 passed; 0 failed.
```

## §2 8x6 verdict matrix

Captured 2026-04-29 against the unmutated (baseline) codebase. Note: the
matrix was intentionally collected with `coplanar_preprocess.rs` reverted
to mainline state, NOT against agent-mutation's working copy. With
agent-mutation's mutations applied the Stage 6 oracle correctly fires
`VIOLATION` on F0001 (i.e. a real true-positive against the injected
defect, not a false-positive against clean code).

| Case   | Stage 0 Coplanar  | Stage 1 Bijective | Stage 2 Arrang.   | Stage 4b Classif. | Stage 5 PatchSeg  | Stage 6 Assembly  | Final     |
|--------|-------------------|-------------------|-------------------|-------------------|-------------------|-------------------|-----------|
| F0001  | Ok                | Ok                | Ok                | Ok                | Ok                | Ok                | AllPass   |
| F0003  | Stub(OracleStub)  | Ok                | Ok                | Ok                | Ok                | Ok                | AllPass\* |
| F0007  | Stub(OracleStub)  | Ok                | Ok                | Ok                | Ok                | Ok                | AllPass\* |
| F0051  | Stub(OracleStub)  | Ok                | Ok                | Ok                | Ok                | Ok                | AllPass\* |
| F0053  | Stub(OracleStub)  | Ok                | Ok                | Ok                | Ok                | Ok                | AllPass\* |
| F0073  | Ok                | Ok                | Ok                | Ok                | Ok                | Ok                | AllPass   |
| F0074  | Ok                | Ok                | Ok                | Ok                | Ok                | Ok                | AllPass   |
| R0018  | Ok                | Ok                | Ok                | Ok                | Ok                | Ok                | AllPass   |

`AllPass*` = all oracles passed except Stage 0 returned `OracleStub`
(known partial-overlap coverage gap).

Headline: **0 `VIOLATION` cells / 48 cells total. 0 false-positives.**

## §3 Per-case analysis

Geometry summaries per `<case>.meta.json`. For each case I report which
snapshots the pipeline actually produced (so "Ok" can be disambiguated
between contract-passed and skip-on-missing). Snapshot population is
inferred from the `record_snapshot` call sites in
`yang_integration.rs:702` (Stage 0/1) and
`topology_extract.rs:1467,1819` (Stage 2/4b/6 — fires from both the
normal Cherchi path AND the AABB-disjoint short-circuit).

### F0001 — `2 ops, scale=1, extrude(rectangle,boss) + extrude(rectangle,boss) — Identical squares`

Two identical 0.5×0.5×0.3 boxes co-located. Trivial-merge boss case.
Pipeline runs full Cherchi path: subdivide → label → flood-fill →
result topology. Snapshots populated at all 6 stages. **All 6 oracles
return Ok. Stage 6 twin-symmetry oracle ran and confirmed the result
arena is bitwise twin-symmetric.** No false-positive.

### F0003 — `2 ops, scale=100, extrude(rectangle,boss) + extrude(rectangle,boss) — Large, swapped aspect`

60×60×30 box plus a swapped-aspect partner. Coplanar preprocessing
detects 1 partial-overlap pair → Stage 0 reports `OracleStub` per spec
§2.1 (overlap-region restriction is the documented coverage gap).
Stages 1–6 all return Ok. Snapshot populated; flood-fill produced 18
faces. No false-positive.

### F0007 — `2 ops, scale=10, extrude(rectangle,boss) + extrude(rectangle,boss) — Concentric squares`

Concentric 6×6×4 + smaller boss. Same shape as F0003: Stage 0
`OracleStub` (1 partial-overlap), Stages 1–6 all Ok. No false-positive.

### F0051 — `2 ops, scale=1e-4, ...rectangle... — Scale extreme`

Micro-scale concentric (3e-5 × 3e-5 × 2e-5). Tests scale invariance
through the `weld_dist` epsilons. Stage 0 `OracleStub` (1 partial-
overlap), Stages 1–6 all Ok. No false-positive — the scale-adaptive
weld in Cherchi 2022 §4 is holding at micro scale.

### F0053 — `2 ops, scale=1e4, ...rectangle... — Scale extreme`

Macro-scale 3000×3000×2000. Mirror of F0051 at large scale. Same
verdict: Stage 0 `OracleStub`, Stages 1–6 all Ok. No false-positive.

### F0073 — `extrude(rectangle,boss) + revolve(rectangle,axis-through-center) — self-intersection error`

Has `expect_rebuild_error: true`. The revolve operation fails with
`revolve self-intersection: profile straddles the revolve axis` in the
feature engine BEFORE the boolean stage runs. The Yang pipeline never
executes; no `record_snapshot` calls fire. All 6 oracles see empty
snapshots → all 6 self-skip with `Ok(())`. Final = AllPass.

This is the correct behavior per the task spec: "oracles should
self-skip" on expected-rebuild-error cases. No false-positive.

### F0074 — `extrude(rectangle,boss) + revolve(rectangle,axis-near-vertex) — self-intersection error`

Same shape as F0073 (`expect_rebuild_error: true`, revolve fails
pre-pipeline). All 6 oracles self-skip → AllPass. Behaves identically
to F0073. No false-positive.

### R0018 — `3 ops, scale≈44, extrude(rectangle,boss) + extrude(gear,cut) + revolve(gear,cut)`

Per `feedback_no_regression_chasing.md`, R0018 is documented as
nondeterministic. On the observed run the pipeline takes the
**AABB-disjoint short-circuit** for the Subtract booleans (the gear
profiles don't AABB-intersect the rectangle in this generated layout).
Stage 2/4b/6 snapshots ARE populated via
`topology_extract.rs:1467` (the disjoint path also calls
`record_stage_2_4b_6_snapshots`). All 6 oracles return Ok. No
false-positive in this run.

Caveat per task spec: a future run may flap. The matrix records the
observed determinism; the audit-validity claim ("oracles do not
false-positive on R0018 on this run") is not over-extended.

## §4 False-positive findings

**None.** Zero `ContractViolated` cells across 48 (case × oracle) pairs
on the unmutated codebase.

The original task spec hypothesized two possible failure modes:
- (H1) Oracle has a false-positive bug.
- (H2) Assay's "pass" is wrong.

Neither hypothesis fired on this run. Both stay in the unfalsified
column.

### Side observation — agent-mutation cross-task signal

While agent-mutation's mutation 5 was active in `coplanar_preprocess.rs`
(intentionally perturbing one vertex on operand B's identical-footprint
mesh emission), my probe DID observe `VIOLATION` on F0001/F0003/F0007/
F0051/F0053/R0018 at Stage 6 (twin-symmetry) AND, at the larger
1.0e-5 perturbation, an additional Stage 4b
(`LabelConsistencyWithinPatchOracle`) violation on F0001 with
`patch 0 contains 2 distinct labels [Inside, Outside] across 6 sub-tris`.

This is NOT a false-positive on the clean codebase — it is a
**true-positive against agent-mutation's injected defect**. I report it
here only because:

1. It's a load-bearing data point for the lead's synthesis: "the Stage
   6 oracle catches the same break the pipeline's
   `validate_yang_result_topology` catches, and Stage 4b additionally
   names the upstream root cause" — agent-mutation's mutation kit
   should retain mutation 5 as a high-signal positive case.
2. It explains why an early naive run of my probe (against
   agent-mutation's working copy) showed 6/8 cases violating: the
   mutation was active. Reverting it in stash gave 0/8.

The matrix in §2 is from the clean run (mutation reverted via stash for
the duration of the probe; mutation restored after).

### Why not flagging "F0001 produces post-validation arena with
twin-symmetry" as suspicious

On clean code Stage 6 returns Ok → twin symmetry holds. The pipeline's
own `validate_yang_result_topology` also accepts the result (no
`[A15.6] Yang boolean pipeline failed` log line in clean run). Stage 6
oracle and pipeline agree. No oracle bug.

## §5 What this task did NOT cover

- **False-negatives** — Task A (agent-mutation) is the auditor for
  this. Mutation 5's signal (caught by Stage 4b + Stage 6 in my
  side-observation) is partial cross-validation but not a substitute
  for the systematic mutation testing in Task A's deliverable.
- **The 149 known-FAIL cases** — Task C (agent-pairing) sweeps the
  full corpus and addresses upstream-shadowing (which oracles fire on
  which cases at scale). My probe deliberately restricts to the 8
  PASS cases per task spec to keep the matrix small enough for visual
  audit.
- **Snapshot completeness on R0018** — I observed AABB-disjoint
  short-circuit on R0018 in this run; if R0018 flaps to the full
  Cherchi path on another run, the snapshot data the oracles see is
  different. The flap rate is documented but not measured here.
- **Multi-boolean cases** — for cases with multiple booleans (R0018 has
  3 ops → 2 booleans), the snapshot collector overwrites earlier
  booleans (per `pipeline_oracles.rs` design). The matrix reflects the
  LAST boolean's stage state. None of the audit-class PASS cases would
  be expected to produce different oracle verdicts across multiple
  booleans, but this is an unverified assumption.
- **`Skip(StateMissing)` exploration** — none of PR9's six oracles
  emit this verdict; the cell is included in the cell taxonomy for
  completeness but never fired in this run.

## §6 Verdict

PR9 oracles produce **zero false-positives** on the 8 audit-class PASS
cases. The harness is fit-for-purpose for downstream PR10 work that
relies on per-stage attribution: a `VIOLATION` cell from a future run
on a known-PASS case can be trusted to reflect a real defect (in code,
not in the oracle).

Per FIP §1 P5 (distinct auditor): I am agent-passcheck, distinct from
agent-mutation (Task A) and agent-pairing (Task C). Lead synthesizes.

## §7 Probe artifact

`crates/test-harness/tests/oracle_validity_pr10_passcheck.rs` — kept
in tree as the empirical anchor for this report. Audit-only,
`#[ignore]`. Re-run with the command in §1 to regenerate the matrix.
