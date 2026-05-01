# Oracle Validity Audit (PR10)

> Branch: `oracle-validity-audit-pr10`
> Synthesized: 2026-04-29 by lead
> Source reports (committed on branch):
> - `docs/audits/oracle_validity_task_a_mutation.md` — agent-mutation
> - `docs/audits/oracle_validity_task_b_passcheck.md` — agent-passcheck
> - `docs/audits/oracle_validity_task_c_pairing.md` — agent-pairing
> Audited artifact: `specs/pipeline_oracles.md` (PR9, commit `ee873fb`)
> Refs: Yang 2025 §4; Cherchi 2022 §3-5; FIP §1 P5, §2; ARCH §A15.6

## §1 Purpose

PR9 shipped a per-stage pipeline-oracle harness (6 oracles, corpus runner,
first-failing-stage histogram). PR9 verified oracles work on **synthetic
fixtures** (FIP §2 red-before-green), but did NOT verify:

1. Oracle false-negatives — does any oracle silently accept real defects?
2. Oracle false-positives — does any oracle fire on genuinely-passing cases?
3. Cross-oracle consistency at scale — adversary spot-checked 3/3 Stage 2
   cases shadow Stage 4b. Does that hold for all 28? And the analogous
   Stage 4b → Stage 6 cascade?

Until those are measured, PR9's histogram is *probably* accurate but not
*empirically* accurate. PR11 picking a fix target on unmeasured oracles
is the same epistemic mistake the methodology pivot was supposed to
replace.

This audit answers all three questions empirically. **No production code
changes.** Three findings are load-bearing for PR11.

## §2 Headline numbers

| Question | Measurement | Source |
|----------|-------------|--------|
| Stage 2 → Stage 4b shadowing | **28/28 (100.0 %)** | Task C |
| Stage 2 → Stage 6 propagation | **28/28 (100.0 %)** | Task C |
| Stage 4b → Stage 6 propagation | **120/120 (100.0 %)** | Task C |
| Stage 4b → Stage 5 propagation | **0/120 (0 %)** | Task C |
| False-positives on known-PASS (8 cases × 6 oracles) | **0 / 48** | Task B |
| Mutations caught at the targeted oracle | **3 / 5** | Task A |
| Mutations missed (true false-negatives or coverage defects) | **2 / 5** | Task A |
| Stage 4b first-fail bucket size | **120 / 157** (was 32 / 157 in PR9) | Task C |
| AllPass bucket size | **3 / 157** (was 72 / 157 in PR9) † | Task C |

† Per Task C §3 Claim 3: 2 of the 3 AllPass cases (F0073, F0074) are
**vacuous** — `expect_rebuild_error: true`, the Yang pipeline never
runs, and oracles self-skip on missing snapshots. Only R0052 is
substantive AllPass. PR11 should treat AllPass as ≤1 substantive case,
not 3.

## §3 Per-oracle confidence verdict

Confidence = how much PR11 can rely on this oracle's verdicts without
caveats. Verdicts are at the oracle's CURRENT contract; recommendations
in §4 may change them.

| Stage | Oracle | Confidence | Why |
|------:|:-------|:----------:|:----|
|     0 | `CoplanarMeshIdenticalOracle`        | **LOW** | Snapshot-capture defect (Task A M5): the production capture site at `yang_integration.rs:702` records `mesh_a`/`mesh_b` BEFORE `inject_identical_footprint_mesh` mutates the flat arrays. The oracle is **unreachable** for the identical-footprint code path in production. Unit tests pass because they synthesize post-injection bundles directly. Plus the documented OracleStub gap on partial-overlap pairs (PR9 §2.1). |
|     1 | `BijectiveFacePairOracle`            | **MEDIUM** | Not exercised by mutation testing in Task A. Task C confirms it fires on the 2 Stage1 first-fail cases (R0031, R0081). No false-positives observed. Empirical floor only. |
|     2 | `MeshArrangementWellFormedOracle`    | **LOW** | Conservation contract is a tautology (Task A M2): `total_directed == 3 × (|tris_a| + |tris_b|)` is internally self-consistent over the snapshot, structurally incapable of detecting "lost-during-emit" defects. Degeneracy detection works once a degenerate sub-tri lands in the snapshot (Task A M4 retry). Stage 2 verdicts are reliable for degeneracy but unreliable for completeness. |
|    4b | `LabelConsistencyWithinPatchOracle`  | **HIGH** | Catches synthetic mixed-label patches (Task A M3 on R0033). 0 false-positives across 8 known-PASS cases (Task B). Fires on 150/157 corpus cases (Task C); 120/120 Stage 4b first-fails also fire Stage 6 — the cascade is empirically total. F0001 had a snapshot-coverage caveat (Task A §2.M3) that affects mutation-only runs, not the corpus baseline. |
|     5 | `ManifoldPatchConservationOracle`    | **MEDIUM** | Not exercised by mutation testing in Task A. Task C: 0/157 corpus violations. PR8's patch-graph builder appears correct; the oracle agrees but cannot be distinguished from a tautology without mutation testing. |
|     6 | `TwinSymmetryOracle`                 | **HIGH** | Catches synthetic twin asymmetry (Task A M1) with informative diagnostic (`half_edge[i].twin = j but twin.twin = k`). 0 false-positives (Task B). Fires on 151/157 corpus cases — heavy use, no observed misfires. |

## §4 Critical findings (load-bearing for PR11)

### F1 — Stage 2 oracle is tautological (Task A, M2)

**Defect class**: A "lost-during-emit" off-by-one in `subdivide_mesh_pair`
(append `sub_tris_a.pop()` immediately before `SubdividedMesh` is built)
is **not detected** by the Stage 2 oracle. The conservation check
`total_directed == 3 × (|tris_a| + |tris_b|)` shrinks proportionally
when a triangle is dropped, so the equation stays satisfied as a
tautology.

**Why it matters**: Stage 2 is the gate for Cherchi 2022 §3-4
preconditions. PR9 §3 nominates Stage 2 as a fix target and the audit
confirms 28/28 cases on this stage shadow Stage 4b, but **a Stage 2
verdict of "Ok" does not imply Stage 2 is well-formed** — it implies
only that the snapshot is internally self-consistent. PR11 reading
Stage 2 verdicts as a green light may build on a false floor.

**Remediation** (out of scope for PR10; PR11+ must address): anchor
conservation to an upstream invariant — e.g., snapshot the
`solve_intersections` output tri count and assert
`subdivided.tris_a.len() + subdivided.tris_b.len() == upstream_tri_count`.

### F2 — Stage 0 oracle is unreachable in production (Task A, M5)

**Defect class**: A 1.0e-5 perturbation of one vertex's x-coordinate on
operand B's identical-footprint mesh, applied just before
`replace_face_triangles` runs (above the `TAU_MODEL = 1e-7` weld
tolerance, so the mutation survives welding) is **not detected** by the
Stage 0 oracle. The corpus runner records
`Stage0Coplanar / CoplanarMeshIdenticalOracle → PASS / skipped`.

**Root cause** (investigated, not hypothesized): the snapshot-capture
site at `yang_integration.rs:702` records `mesh_a.clone()` and
`mesh_b.clone()`, but those bindings come from
`tessellate_waffle_solid` output **before**
`inject_identical_footprint_mesh` runs (line 662). The injection mutates
the SEPARATE flat arrays `verts_a`/`tris_a`/`verts_b`/`tris_b`, NOT the
RenderMeshes the oracle sees. Stage 0's contract is correct (its unit
tests demonstrate this), but the oracle is **fed the wrong artifact in
production**.

**Why it matters**: the corpus-wide `Stage 0 = Ok` for non-stub cases is
a **vacuous** Ok. Any byte-divergence defect injected at injection time
is invisible to the oracle. PR11 should not interpret Stage 0 as
covering identical-footprint correctness.

**Remediation** (out of scope for PR10): either re-derive `mesh_a` and
`mesh_b` from the post-injection flat arrays before snapshotting, OR
move the snapshot site below the flat-array→RenderMesh conversion that
would happen post-injection. Until then, the Stage 0 oracle is dead
weight in the corpus runner.

### F3 — Distribution shift since PR9 baseline (Task C, §7)

PR9 §3 baseline (2026-04-29):    Stage 4b = 32, AllPass = 72.
PR10 audit run (2026-05-01):      Stage 4b = 120, AllPass = 3.
Δ Stage 4b = +88; Δ AllPass = −69.

**Why the shift**: most likely the AABB-disjoint short-circuit (commit
`aee32d8`) now populates Stage 2/4b/6 snapshots that PR9 baseline missed.
The 69 PR9 AllPass cases were largely vacuous (snapshots empty, oracles
self-skip silently to `Ok`). Once snapshots are populated on the
disjoint path, the Stage 4b oracle correctly reports the labeling
violations that were always there.

**Why it matters for PR11**:
- PR9's "32 Stage 4b" headline understated the Stage 4b lever by ~3.75×.
- PR11's lever sizing must use Task C's numbers (120, not 32).
- The "72 AllPass" was not a clean pass set — it was a measurement
  artifact of incomplete snapshot coverage.

**This is good news for oracle validity**: the Stage 4b oracle is
working as designed; the previous AllPass numbers were inflated by
snapshot-capture gaps that have since been (partially) closed.

## §5 PR11 implications

### S5.1 Sequencing — Stage 2 first, Stage 4b concurrent

Confirmed by Task C: 28/28 Stage 2 first-fail cases also fire Stage 4b
(and Stage 6). A pure Stage 4b labeling fix CANNOT improve those 28
cases; the Stage 2 well-formedness invariant must be restored first or
the per-patch labeling operates on input that already fails Cherchi
2022 §3-4 preconditions.

### S5.2 Lever sizing (with Task A caveats)

| Fix scope | Cases potentially unlocked | Caveat |
|-----------|---------------------------:|--------|
| Stage 2 only | 28 | Stage 2 oracle's Ok verdict is unreliable (F1); cases may still fail downstream contracts not exercised by current oracle. |
| Stage 4b only | 120 | Best-case if labeling fix works AND Stage 6 cascade resolves. 120/120 Stage 4b → Stage 6 propagation supports this. |
| Both | 148 | Stage 1 (2) + Stage 6-only (1) + AllPass (3) + Timeout (3) = 9 remain unaddressed. |

The 120-case Stage 4b lever is the single largest. Task C's 0/120
Stage 4b → Stage 5 rate confirms PR8's patch-graph builder is correct;
the labeling bug is WITHIN the patches, not in patch identification.

### S5.3 Skip Stage 5

PR8's `ManifoldPatchConservationOracle` reports 0/157 violations across
the corpus. No Stage 5 work is warranted in PR11.

### S5.4 Stage 0 must not be a confidence anchor

Per F2, a Stage 0 = Ok verdict provides no signal in production. PR11
must not use Stage 0 oracle results to claim coplanar-identical
correctness; either fix the snapshot capture site as part of PR11 or
treat Stage 0 verdicts as untrusted until that fix lands.

## §6 What this audit did NOT cover

- **Mutation coverage of Stages 1 and 5**: Task A applied 0 mutations to
  these. The MEDIUM confidence in §3 reflects this gap.
- **Multi-boolean cases**: snapshot collector overwrites with the LAST
  Yang boolean per `pipeline_oracles.rs` design. Multi-op cases were
  excluded from Task C's matrix (33 cases). PR9 §6 noted this; PR10
  inherits the scope limit.
- **Reference comparison vs Cherchi 2022 C++**: out of scope.
  `MeshBooleansLib` would be the gold standard for cross-validation.
  Possible future audit.
- **Cherchi 2022 §5 / Algorithm 1 oracle**: PR9 oracles cover the
  surface contracts (well-formedness, label consistency, twin symmetry)
  but not Algorithm 1's per-patch traversal directly. A Stage 4b
  oracle that re-runs Algorithm 1 from the captured patch graph and
  cross-checks would be a stronger contract than mixed-label detection.
- **R0080 / R0018 nondeterminism**: documented; observed runs match
  expectations on this branch but flap rate not separately measured.
- **Snapshot-presence ground truth**: Task C inferred `Ok` vs `skipped`
  from `OracleRunSummary::per_oracle` rather than direct
  `OwnedSnapshotBundle` inspection. The `kernel::diagnostics` public
  surface does not expose the bundle. AllPass purity is therefore
  inferred indirectly via Stage 0 OracleStub presence.

## §7 Outcome summary

Per the plan's three-outcome framing in `feedback_no_last_bug.md`:

> 1. **All oracles validated cleanly** — PR11 proceeds with confidence.
> 2. **Some oracles have false-negatives** — directional use OK, caveat for PR11.
> 3. **Some oracles have false-positives** — oracle defect must precede PR11.

This audit landed in **outcome 2**, with a twist: the oracle SUITE had
no false-positives on known-PASS cases (Task B: 0/48), but **two
oracles (Stage 0, Stage 2) have false-negatives or coverage defects
that disqualify their Ok verdicts as confidence anchors** (Task A: F1,
F2). The remaining four oracles (Stages 1, 4b, 5, 6) are at MEDIUM-HIGH
confidence per §3.

**PR11 can proceed**, but its design must:

1. Treat Stage 0 / Stage 2 Ok verdicts as **unreliable confidence
   signals**. Address F1 and F2 either as part of PR11 or before relying
   on those stages' "passes" for prioritization.
2. Use Task C's numbers (Stage 4b = 120, AllPass = 3), not PR9 §3's
   numbers (Stage 4b = 32, AllPass = 72). The latter underestimate the
   Stage 4b lever by 3.75× because of snapshot-coverage gaps that have
   since been (partially) closed.
3. Sequence Stage 2 fix before Stage 4b labeling fix per §5.1, with
   the understanding that the Stage 2 oracle's signal of "Stage 2 fixed"
   is structurally weak (F1).

## §8 Honest framing

Per `feedback_no_last_bug.md`: this audit applied 5 mutations and
covered 8 known-PASS cases. The conclusions hold for the contracts
exercised. They do NOT establish that the oracle suite catches every
class of defect, only the classes tested. Future PRs may surface more
gaps. The two false-negatives found (F1, F2) are the ones this budget
discovered, not the ones the suite has.

The most uncomfortable finding was Task A's M5: Stage 0's contract was
believed to be a quality gate against byte-identity violations. In
production, it is not — the snapshot-capture site silently feeds the
oracle the wrong artifact. PR9's spec did not catch this because PR9's
spec was authored from the unit-test side, not the production-runner
side. That is the kind of gap an audit exists to surface.

## §9 Reproduction

```bash
# Task A — mutation testing (manual; deliverable is the report).
git -C $REPO log -- crates/kernel/src/boolean/oracles/                  # PR9 oracles under audit
# See docs/audits/oracle_validity_task_a_mutation.md §2 for per-mutation
# diff, oracle invocation command, and verdict.

# Task B — known-PASS verification.
cargo test -p test-harness --test oracle_validity_pr10_passcheck \
    oracle_validity_pr10_known_pass_verification -- --ignored --nocapture

# Task C — cross-oracle pairing.
YANG_BOOLEAN=1 cargo test -p test-harness --test oracle_validity_pr10_pairing \
    -- --ignored --nocapture

# Sanity check (PR9 module tests still pass; this audit changed no production code).
cargo test -p kernel pipeline_oracles
```

The two probe artifacts (`oracle_validity_pr10_passcheck.rs` and
`oracle_validity_pr10_pairing.rs`) are kept on the branch as durable
re-run anchors for any future audit that wants to compare verdicts on
a different revision.

## §10 Governance compliance

- **FIP §1 P5**: 5 distinct agents (lead + agent-mutation +
  agent-passcheck + agent-pairing + adversary). Each audit task ran in
  its own slice.
- **FIP §2 red-before-green**: doesn't strictly apply (audit, not
  feature). Each task's deliverable IS the test result.
- **P8 (cite research)**: each finding cites Yang 2025 §4 / Cherchi
  2022 §3-5 and the audit anchor (e.g., F0001 for Stage 6, R0033 for
  Stage 4b, F0002 for Stage 2).
- **P9 (no hack-to-green)**: no production code changes; the audit's
  product is empirical evidence, not patches.
- **P10 (stay in slice)**: each agent stayed in its task slice; lead
  synthesizes, adversary verifies.
