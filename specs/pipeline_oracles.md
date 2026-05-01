# PR9 — Per-Stage Pipeline Oracles for the Yang 2025 Hybrid Pipeline

> Status: shipped 2026-04-29
> Owner: pipeline-oracle-harness-pr9 team
> Refs: Yang 2025 §4; Cherchi 2022 §3-5; `docs/audits/yang_audit_2026-04-30.md`

## §1 Overview

PR9 ships per-stage contract oracles for the Yang 2025 hybrid B-Rep/mesh
boolean pipeline (`crates/kernel/src/boolean/yang_integration.rs`). Each
oracle observes pipeline state at a specific Yang stage and reports whether
the contract derived from Yang or Cherchi paper sections holds. The
corpus runner sweeps the assay corpus, captures snapshots from the LAST
boolean executed during each case's `LoadProject` replay, and tallies
**first-failing-stage** counts into a histogram. Earliest-stage attribution
beats end-of-pipeline rubble: it names the upstream stage where the contract
first cracks instead of leaving the post-mortem to infer it from a downstream
twin-asymmetry blast pattern.

What ships in PR9:

- **Harness** — `crates/kernel/src/boolean/pipeline_oracles.rs` —
  `StageOracle` trait, `PipelineState`, `OracleViolation`, runner.
- **Six oracles** wrapping existing or new contracts:
  - Stage 0 — `CoplanarMeshIdenticalOracle` (NEW, agent A — PR9 T2)
  - Stage 1 — `BijectiveFacePairOracle` (wraps PR1 — harness T1)
  - Stage 2 — `MeshArrangementWellFormedOracle` (NEW, agent B — PR9 T3)
  - Stage 4b — `LabelConsistencyWithinPatchOracle` (NEW, agent C — PR9 T4)
  - Stage 5 — `ManifoldPatchConservationOracle` (wraps PR8 — harness T1)
  - Stage 6 — `TwinSymmetryOracle` (mirrors `validate_yang_result_topology` — harness T1)
- **Public diagnostic surface** — `kernel::diagnostics` — re-exports
  `YangStage`, `OracleViolation`, `ViolationKind`, plus an entry point
  `with_yang_oracle_capture` that installs a thread-local snapshot
  collector around an arbitrary closure (the corpus runner uses
  `dispatch(LoadProject)` as the closure).
- **Corpus runner** —
  `crates/test-harness/tests/pr9_pipeline_oracle_corpus.rs` —
  ignored `#[test]` that sweeps the 157-case yang_fast subset (190
  total minus 33 known timeouts) and emits the §3 histogram to stderr.

PR9 does NOT change production behavior. Snapshot collection is gated on
a thread-local that only the corpus runner installs; production callers
of `yang_boolean_inner` see a single null check at each stage boundary.

## §2 Per-oracle contracts

Each oracle wraps a paper-section invariant. Contracts are summarized
here; the source modules carry the full citation block.

### Stage 0 — `CoplanarMeshIdenticalOracle` (Yang 2025 §4.5.5)

**Contract**: when the coplanar preprocessing step detects an
identical-footprint coplanar pair between operand A and operand B, the
post-injection meshes must carry byte-identical triangulation over the
overlap region. Yang §4.5.5 (p. 1281–1292): _"the overlapping part is
replaced by a trimmed common planar surface, and identical meshes are
generated for both models in this part."_

**Implementation note**: the oracle compares plane-coincident
sub-triangle multisets under canonical-form keys (sorted f32 vertex
bits). Partial-overlap pairs are reported as `OracleStub` — restricting
to the overlap region requires re-running i_overlay and is left to a
follow-up. (Per §3, Stage 0 first-fails are 0 in the corpus; the
identical-footprint contract holds where it applies. The
`OracleStub` cases ride atop other-stage failures.)

### Stage 1 — `BijectiveFacePairOracle` (Yang 2025 §4.1.1)

**Contract**: every face's boundary directed edges must reciprocate
byte-identically as `(q, p)` on adjacent faces sharing the same B-Rep
edge. This is the PR1 oracle (`tessellation::bijective`) wrapped into
the per-stage harness; checks both operands.

### Stage 2 — `MeshArrangementWellFormedOracle` (Cherchi 2022 §3-4)

**Contract**: the post-arrangement sub-triangle mesh produced by
`subdivide_mesh_pair` must satisfy the Cherchi §3-4 well-formedness
preconditions every downstream consumer assumes:

1. Every undirected edge has 1 / 2 / N≥3 directed-edge sides
   (forward + backward = undirected count).
2. No degenerate sub-tris (zero cross-product / collinear).
3. Vertex indices are in range.
4. Directed-edge total equals `3 × (|tris_a| + |tris_b|)` (conservation).

### Stage 4b — `LabelConsistencyWithinPatchOracle` (Cherchi 2022 §5 + Algorithm 1)

**Contract**: within each manifold patch (per `ManifoldPatchGraph`),
every sub-triangle must share one `CellLabel`. Cherchi 2022 §5's
complexity claim ("scales with patches, not triangles") rests on this:
one ray-cast per patch propagates one label across the patch via the
manifold-edge graph. Mixed-label patches forfeit the claim and feed
incorrect half-edge pairing into `flood_fill_patches`.

This oracle is the audit's predicted dominant first-fail per Cluster Y-I
(YA-01 / YC-05 / YB-01).

### Stage 5 — `ManifoldPatchConservationOracle` (Yang 2025 §4.4 + Cherchi 2022 §5)

**Contract**: `Σ |patches[i]| == |tris_a| + |tris_b|`. Wraps PR8's
`build_manifold_patch_graph` to verify the patch graph partitions the
sub-triangle soup (no sub-triangle lost or double-counted).

### Stage 6 — `TwinSymmetryOracle` (Mantyla 1988 §4.2)

**Contract**: for every half-edge `i`,
`arena.half_edges[arena.half_edges[i].twin].twin == i`. Mirrors
`validate_yang_result_topology` in `yang_integration.rs`. The 92-case
YB-01 failure bucket in the audit is the downstream symptom of this
contract failing.

## §3 First-failing-stage histogram

Captured 2026-04-29 from `cargo test -p test-harness --test pr9_pipeline_oracle_corpus pr9_pipeline_oracle_corpus -- --ignored --nocapture`. Run duration 261s; snapshot capture is the live thread-local
collector during `LoadProject` replay (each case's collector overwrites
with the LAST Yang boolean executed, which for 2-op assay cases is the
final boolean).

| Bucket                | Count | Notes                                                             |
|-----------------------|------:|-------------------------------------------------------------------|
| Stage1Bijective       |     7 | Operand-side bijectivity (Yang §4.1.1)                            |
| Stage2Arrangement     |    28 | Degenerate sub-tris in Cherchi arrangement output                 |
| Stage4bClassification |    32 | Mixed-label patches — predicted dominant per Cluster Y-I          |
| Stage6Assembly        |    15 | Twin-asymmetry survives Stage 4b → cascades through Stages 5/6   |
| AllPass               |    72 | ≈40 are AABB-disjoint short-circuits; remainder genuinely clean   |
| Timeout               |     3 | 30 s per-case budget exceeded (R0032, R0050, R0071)               |
| **Total**             |   157 | 190 corpus cases minus 33 yang_fast skip list                     |

Sample case ids (first four per bucket, deterministic order):

- Stage1Bijective: R0014, R0031, R0035, R0081
- Stage2Arrangement: R0016, R0019, R0020, R0038
- Stage4bClassification: R0007, R0008, R0013, R0021
- Stage6Assembly: R0009, R0015, R0027, R0043
- AllPass: R0001, R0002, R0004, R0005
- Timeout: R0032, R0050, R0071

`OracleStub` verdicts (Stage 0 partial-overlap unchecked) do NOT count
toward the first-fail bucket — they coexist with whatever
`ContractViolated` first fires downstream. This is the correct behavior:
the histogram measures reachable contract violations, not coverage gaps.

## §4 Comparison vs YB-01 prediction

The 2026-04-30 audit's YB-01 finding (`yang_audit_b_assay_failures.md`)
identified 92 cases (~58.6 %) where `validate_yang_result_topology`
rejected the result with twin asymmetry. The audit's hypothesis (Cluster
Y-I) was: those 92 cases are the downstream symptom of mixed-label
patches at Stage 4b — the true root-cause stage.

PR9 directly tests the hypothesis. The histogram says:

- **Stage 4b first-fail count: 32 / 157 (20.4 %)** — confirms Stage 4b
  is the dominant root cause stage.
- **Stage 6 first-fail count: 15 / 157 (9.6 %)** — additional
  twin-asymmetry cases where Stage 4b passed but Stage 6 still failed
  (a different upstream defect, e.g. flood-fill grouping).
- **Stage 2 first-fail count: 28 / 157 (17.8 %)** — degenerate sub-tris
  upstream of labeling. These cases never get a chance to fail at
  Stage 4b because Stage 2 already cracked.

The 92-case YB-01 prediction is partially confirmed (Stage 4b is
dominant among root causes), but the picture is more textured than YB-01
modelled:

- Many YB-01 cases are actually **Stage 2 root-causes** — the
  arrangement produces a degenerate sub-tri, downstream labeling /
  flood-fill / twin-pairing all fail in cascade. PR10 should NOT
  attempt to fix labeling without first ensuring Stage 2 is producing a
  well-formed sub-mesh.
- Some YB-01 cases are **AABB-disjoint AllPass** under the corrected
  per-op selection in `yang_pipeline_result_for_disjoint`. The audit
  was reading the disjoint cases as YB-01 because the legacy disjoint
  path was producing different topology; PR9's snapshot capture in the
  disjoint short-circuit shows they pass cleanly.
- Some YB-01 cases bucket as **Stage 1 first-fail (operand-side
  bijectivity)** — the operands themselves carry T-junctions that PR4–8
  did not catch because they only tested on the result solid, not the
  inputs. This is a 7-case slice that PR10 will inherit if the fix lands
  before its operand-side preconditions are tightened.

In short: YB-01 was directionally correct ("twin-symmetry is downstream
of an upstream labeling defect"), but the upstream defect is at multiple
stages, not just Stage 4b.

## §5 PR10 target recommendation

**Stage 4b is the largest single first-fail bucket (32 cases) and
matches the audit's prediction. PR10 should target it.**

Concretely, the Cherchi 2022 §5 / Algorithm 1 fix is to replace the
per-sub-triangle ray-casting in `label_cells` with per-patch labeling:

1. Build the manifold patch graph (PR8's `build_manifold_patch_graph`).
2. For each patch, ray-cast the **first** sub-triangle in the patch.
3. Propagate the label to every other sub-triangle in the same patch
   via the manifold-edge graph.

This is the Cherchi 2022 headline complexity result and the direct
implementation of what `LabelConsistencyWithinPatchOracle` checks.

**Important: do not start PR10 without first verifying Stage 2.** The
28 Stage-2 first-fail cases may break PR10 in regression even with a
correct Stage 4b implementation, because PR10 inputs corrupted Stage 2
output. Either:

- (a) PR10 fixes Stage 2 first (subdivide_mesh_pair degeneracy) and
  defers Stage 4b to PR11; or
- (b) PR10 fixes Stage 4b but the spec doc owner explicitly notes that
  Stage 2 first-fails will not improve.

Recommend (a) — even if it shrinks PR10's headline win, it ensures the
fix doesn't regress on cases where the Stage 4b oracle wasn't even
reached because Stage 2 already broke.

## §6 Open questions / scope limits

- **Stage 3 (SSI refinement) and Stage 4a (mesh updating)** are stubbed
  in tree per the 2026-04-30 audit (YA-19, YA-20). PR9 ships no oracle
  for these stages — the harness leaves space (`YangStage::Stage3SsiRefinement`,
  `Stage4aMeshUpdating`) so a future PR can wire one when those stages
  are implemented.
- **Snapshot capture is per-call, not per-boolean-in-multi-op-cases**.
  Cases with multiple booleans (mostly the 33-case yang_fast skip set)
  record only the LAST boolean's snapshot. The skip set already excludes
  these from PR9's run; if PR10's adversary T6 wants to include them,
  a richer collector that buffers per-call snapshots is needed.
- **`OracleStub` cases (Stage 0 partial-overlap)** are silent in the
  histogram. The runner's filter excludes them by design; if PR10
  needs to track partial-overlap cases, the filter can be relaxed to
  bucket them into a `Stage0Stub` category.
- **Timeout cases (3 / 157)**: the 30 s per-case budget excludes these
  from oracle attribution. The yang_fast skip set was computed at 90 s;
  the corpus runner uses 30 s because it adds the per-stage capture
  cost (mesh / arena / topology clones). Three cases (R0032, R0050,
  R0071) marginally exceeded 30 s.
- **Determinism**: per `feedback_no_regression_chasing.md`, R0080 /
  R0018 nondeterminism is documented; the runner uses first-call
  results. The histogram counts above are first-call.

## §7 How to reproduce

```bash
# Anchor (R0033 + F0001 + F0002, ~30s):
cargo test -p test-harness --test pr9_pipeline_oracle_corpus \
    pr9_corpus_runner_captures_snapshots_anchor -- --ignored --nocapture

# Full corpus (~5 min):
cargo test -p test-harness --test pr9_pipeline_oracle_corpus \
    pr9_pipeline_oracle_corpus -- --ignored --nocapture
```

The corpus runner is `#[ignore]` (long-running) so it doesn't run
during `cargo test` by default. Module-level oracle tests (34 in
`kernel`) DO run by default and verify each oracle's contract on
synthetic fixtures.
