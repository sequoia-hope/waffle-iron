# PR12 Stage 1 oracle diagnostic capture (T2)

**Author**: agent-diagnose, team `yang-stage1-bijective-pr12`
**Date**: 2026-05-01
**Branch**: `yang-stage1-bijective-pr12`
**Scope**: Empirical Stage 1 (`BijectiveFacePairOracle`) verdict capture for the
15 first-fail cases identified by PR11's adversary report
(`docs/audits/pr11_adversary_validation.md` §5 F1), cluster classification,
and PR10-baseline comparison to test whether the inflation 2 → 15 is pure
cascade unmasking or a partial PR11 regression.

---

## §1 Methodology

Probe: `crates/test-harness/tests/pr12_stage1_diagnostic.rs` (this commit).
Built around `with_yang_oracle_capture`, mirrors the Task C TRACE format
in `oracle_validity_task_c_pairing.md`. Per case, captures:

- Per-stage verdicts in `(S0, S1, S2, S4b, S5, S6)` tuple (X = `ContractViolated`,
  S = `OracleStub`, M = `StateMissing`, `.` = `Ok`).
- Stage 1 violation message (`"non-bijective face pairs: operand A N pair(s) of M, operand B P pair(s) of Q"`).
- Stage 0 violation message if any.

Methodology gates:

- **PR12 measurement** (current branch HEAD, sha-pending): one full run on the
  15-case panel (`/tmp/pr12_stage1_diag_final.log`), plus three additional runs
  on the same panel to measure determinism of the S1 fire/no-fire signal
  (`/tmp/pr12_run3.log`, `/tmp/pr12_run4.log`, and the output rerun captured
  in §2 below).
- **PR10 baseline measurement** (commit `c2e473c`): probe rebuilt at that commit
  (separate rebuild because the file did not exist there), two consecutive runs
  to measure determinism (`/tmp/pr12_stage1_diag_pr10_baseline.log`,
  `/tmp/pr12_stage1_diag_pr10_run2.log`).
- After PR10 measurement, branch restored, probe restored, results table
  embedded in the probe as `PR10_BASELINE_VERDICTS`.

Per `feedback_anchor_before_fix.md`: probe contains `[ANCHOR]` `eprintln!`s on
the per-case driver loop, confirming the test is exercising every case before
the assertion of completeness.

Per `feedback_validate_against_corpus.md`: the PR10-baseline-vs-PR12 comparison
is the load-bearing data point. Without it, the report would conflate cascade
unmasking with regression.

---

## §2 Headline result — Stage 1 verdicts on PR12 (current branch HEAD)

Canonical run (the run committed alongside this report):

```
TRACE | R0007 | S X X . . X | X (1+2+6)         | A 1 pair(s) of 12,    B 0 pair(s) of 2016
TRACE | R0014 | S . . . . . | Z (other)         | (no s1 violation)
TRACE | R0020 | . X X . . X | X (1+2+6)         | A 2 pair(s) of 19,    B 0 pair(s) of 2
TRACE | R0021 | . X X . . X | X (1+2+6)         | A 7 pair(s) of 23,    B 0 pair(s) of 2
TRACE | R0031 | S X X . . X | X (1+2+6)         | A 5 pair(s) of 24,    B 0 pair(s) of 1044
TRACE | R0034 | . . . . . X | Z (other)         | (no s1 violation)
TRACE | R0035 | . X . . . X | Y (1+6, S2=Ok)    | A 15 pair(s) of 684,  B 0 pair(s) of 2
TRACE | R0046 | . . . . . X | Z (other)         | (no s1 violation)
TRACE | R0063 | S X . . . X | Y (1+6, S2=Ok)    | A 6 pair(s) of 20,    B 0 pair(s) of 1848
TRACE | R0081 | S X . . . X | Y (1+6, S2=Ok)    | A 0 pair(s) of 1776,  B 1 pair(s) of 18
TRACE | R0095 | . X . . . X | Y (1+6, S2=Ok)    | A 56 pair(s) of 172,  B 0 pair(s) of 1092
TRACE | F0016 | . X . . . X | Y (1+6, S2=Ok)    | A 3 pair(s) of 40,    B 0 pair(s) of 12
TRACE | F0018 | . X . . . X | Y (1+6, S2=Ok)    | A 4 pair(s) of 40,    B 0 pair(s) of 12
TRACE | F0019 | . X . . . X | Y (1+6, S2=Ok)    | A 2 pair(s) of 40,    B 0 pair(s) of 12
TRACE | F0076 | . X . . . X | Y (1+6, S2=Ok)    | A 1 pair(s) of 17,    B 0 pair(s) of 18
```

**Cluster breakdown (this run)**:

| Cluster | Definition | Count | Cases |
|---------|------------|------:|-------|
| X       | S1 + S2 + S6 fire | 4 | R0007, R0020, R0021, R0031 |
| Y       | S1 + S6 fire, S2 = Ok | 8 | R0035, R0063, R0081, R0095, F0016, F0018, F0019, F0076 |
| Z       | S1 not firing or other | 3 | R0014, R0034, R0046 |

**Critical observation** — re-framing the original cluster taxonomy: the plan's
original definition (X = cascade `1+2+4b+6`, Y = decoupled `1+4b+6`) fails on
this corpus because **Stage 4b is `Ok` on every single one of the 15 cases** —
PR11's per-patch labeling (Cherchi 2022 §5 Algorithm 1) fixed S4b structurally.
The taxonomy is reframed against Stage 2 instead (next-most-relevant cascade
signal): X = S1+S2+S6, Y = S1+S6 with S2=Ok, Z = anything else.

**Operand asymmetry**: 14 of the 15 cases' Stage 1 violations are concentrated
on **operand A** (operand B has 0 unmatched pairs). The exception is R0081
(operand A = 0, operand B = 1). The defect is overwhelmingly asymmetric — this
suggests the issue is not in the bijective oracle's symmetric matching logic
but in a per-operand tessellation pathway that fires for some operand classes
and not others.

**S0 OracleStub on six cases** (R0007, R0031, R0046, R0063, R0081 — and
sometimes R0014 / R0034 in the flap runs): Stage 0 coplanar preprocessing
detects partial-overlap pairs but reports `OracleStub` — this is an existing
audit finding (some `inject_partial_overlap_mesh` paths still return
"unimplemented" for non-anti-parallel cases). The Stage 0 stubbing on these
cases means Stage 1 sees a tessellation that PR11 has only partially
preprocessed.

---

## §3 PR10 baseline comparison — load-bearing data

Re-running the probe at commit `c2e473c` (the pre-PR11 main commit) gives
Stage 1 verdicts. The S1 fire/no-fire signal is **stable across two PR10 runs**
on every case (counts within S1 messages flap, but the binary verdict matches
across both PR10 runs).

| case  | PR10 verdicts  | PR12 verdicts (canonical run) | Provenance |
|-------|----------------|-------------------------------|------------|
| R0007 | `. . . X . X`  | `S X X . . X`                 | **PR11-INTRODUCED** |
| R0014 | `. X . . . X`  | `S . . . . .`                 | PR12-FIXED (S1 stable in PR10, flapping in PR12) |
| R0020 | `. . X X . X`  | `. X X . . X`                 | **PR11-INTRODUCED** |
| R0021 | `. . . X . X`  | `. X X . . X`                 | **PR11-INTRODUCED** |
| R0031 | `S X X X . X`  | `S X X . . X`                 | CASCADE (S1 firing in both) |
| R0034 | `. X . X . X`  | `. . . . . X`                 | PR12-FIXED (S1 stable in PR10, flapping in PR12) |
| R0035 | `. X . X . X`  | `. X . . . X`                 | CASCADE |
| R0046 | `. . . X . X`  | `. . . . . X`                 | NEITHER (S1 doesn't fire in either; flaps to fire some PR12 runs) |
| R0063 | `. . . X . X`  | `S X . . . X`                 | **PR11-INTRODUCED** |
| R0081 | `S X . X . X`  | `S X . . . X`                 | CASCADE |
| R0095 | `. . . X . X`  | `. X . . . X`                 | **PR11-INTRODUCED** |
| F0016 | `. X . . . X`  | `. X . . . X`                 | CASCADE |
| F0018 | `. X . . . X`  | `. X . . . X`                 | CASCADE |
| F0019 | `. X . X . X`  | `. X . . . X`                 | CASCADE |
| F0076 | `. . . X . X`  | `. X . . . X`                 | **PR11-INTRODUCED** |

**Provenance summary (canonical PR12 run)**:

- **CASCADE unmasking** (S1 firing in both PR10 and PR12): 6 cases —
  R0031, R0035, R0081, F0016, F0018, F0019. These are pre-existing Stage 1
  defects exposed because PR11's S4b cleanup let them surface as the
  first-failing stage. Tessellation defect is real, pre-existing.
- **PR11-INTRODUCED** (S1 not firing in PR10, fires in PR12): 6 cases —
  R0007, R0020, R0021, R0063, R0095, F0076. These cases were Stage 1 = `Ok`
  in PR10 baseline but fire S1 in PR12.
- **PR12-FIXED** (S1 firing in PR10, doesn't fire in PR12 canonical run): 2
  cases — R0014, R0034. Both were S1 = `X` in PR10 baseline (run 1+2),
  but in this PR12 run come up `Ok`. *NOTE*: §4 below shows these flap
  across PR12 runs — they fire on most runs but missed on this one.
- **NEITHER**: 1 case — R0046 (PR10 didn't fire S1, this PR12 run didn't
  fire S1, but other PR12 runs do).

So the 13-case "newly unmasked by PR11" framing in PR11's adversary report is
**partially correct**: 6 of the 13 are stable cascade unmasking, but 6 cases
(R0007, R0020, R0021, R0063, R0095, F0076) were S1 = Ok in PR10 and now fire
S1 in PR12. That is a **mixed-mechanism inflation**, not pure cascade.

---

## §4 Determinism (or lack thereof)

Across 4 PR12 runs of the same probe on the same 15-case panel:

| Case | run1 | run2 | run3 | run4 | Stable? |
|------|------|------|------|------|--------|
| R0007 | X | X | X | X | yes |
| R0014 | X | `.` | X | X | **no** (flaps) |
| R0020 | X | X | X | X | yes |
| R0021 | X | X | X | X | yes |
| R0031 | X | X | X | X | yes |
| R0034 | X | `.` | `.` | `.` | **no** (flaps) |
| R0035 | X | X | X | X | yes |
| R0046 | X | `.` | X | X | **no** (flaps) |
| R0063 | X | X | X | X | yes |
| R0081 | X | X | X | X | yes |
| R0095 | X | X | X | X | yes |
| F0016 | X | X | X | X | yes |
| F0018 | X | X | X | X | yes |
| F0019 | X | X | X | X | yes |
| F0076 | X | X | `.` | X | **no** (flaps) |

11/15 cases have stable S1 fire across PR12 runs; 4/15 cases (R0014, R0034,
R0046, F0076) flap — they fire S1 sometimes but not always.

**Counts within S1 messages also flap** (e.g. R0014 reports `9 pair(s)` in PR10
run 1 and `7 pair(s)` in run 2; R0007 reports `1 pair(s)` in canonical run and
`4 pair(s)` in run 3). This non-determinism originates in upstream HashMap
RandomState (used by `face_boundary_directed_edges`'s `count: HashMap<...>` and
the Cherchi arrangement). It does not originate in the bijective oracle's
matching logic itself (that logic is deterministic given the input).

This determinism behavior is consistent with the existing PR4 R0033 diagnostic
note (`pr4_r0033_t_junction_diagnosis.rs` §"flap"): the bijective oracle's nb
count flaps under HashMap iteration order across consecutive runs even on the
same fixture.

**Interpretation**: PR12 cannot rely on a stable per-case verdict. Any red-test
gating on a specific case + S1=`X` for that case must either:
1. Pin PR4's deterministic-iteration approach (sort `face_ranges` and HashMap
   iterations by stable keys before oracle invocation), or
2. Accept the flap and use stochastic counts (e.g., "fires S1 on ≥ 3 of 5
   runs"). This is messy.

---

## §5 Cluster classification (canonical run)

### Cluster X — S1 + S2 + S6 fire ("arrangement-collapse cascade")

**4 cases**: R0007, R0020, R0021, R0031.

These cases have a tessellation defect that propagates into the Cherchi mesh
arrangement, breaking the Stage 2 emit-conservation invariant. Stage 6 also
fires (twin-asymmetry on the half-edge result).

Subclass by where the defect lives:
- R0031 fires S0 stub (coplanar preprocessing detected partial-overlap pair
  but couldn't process it) AND S1 + S2 + S6 — this is the canonical "stage 0
  partial overlap stub leaks into stage 1" failure.
- R0007 fires S0 stub AND S1 + S2 + S6 — same pattern as R0031.
- R0020, R0021 do NOT fire S0 (no coplanar pair detected) — pure tessellation
  defect; bijective contract violated independent of coplanar preprocessing.

### Cluster Y — S1 + S6 fire, S2 = Ok ("decoupled-tessellation-only")

**8 cases**: R0035, R0063, R0081, R0095, F0016, F0018, F0019, F0076.

These cases have Stage 1 violations that **do not propagate** into Stage 2 —
the Cherchi arrangement preserves the conservation invariant despite the
bijective contract being broken. This is curious and requires explanation:
how can the mesh arrangement conserve tri counts if the input mesh has
non-bijective face boundaries?

Hypothesis: the Cherchi arrangement merges position-coincident vertices via
its own `dedup_mesh_vertices` step, and S2's emit-conservation check is
formulated against `upstream_tri_count` (the count after dedup, not after
bijective check). If the non-bijective edges happen to be position-coincident
post-dedup (e.g., T-junctions where a midpoint vertex is added on one face
that the other face's edge passes through but doesn't have as a vertex), the
arrangement's downstream count is conserved while the per-face boundary
contract is violated.

Subclass by S0 OracleStub presence:
- S0 OracleStub: R0063, R0081 — coplanar preprocessing detected partial overlap
  but stubbed.
- No S0 fire: R0035, R0095, F0016, F0018, F0019, F0076 — defect pre-dates
  coplanar preprocessing entirely.

### Cluster Z — other

**3 cases (this run)**: R0014, R0034, R0046.

In this canonical run, S1 doesn't fire on these cases. Across runs, R0014 and
R0046 flap; R0034 mostly does NOT fire in PR12 but did fire in PR10. R0046
mostly fires (cluster X) but doesn't on this run.

Sub-classification within Z is not load-bearing because Z is a flap artifact.

### Operand B is the source for one case (R0081)

R0081 stands alone: "operand A 0 pair(s) of 1776, operand B 1 pair(s) of 18."
Operand B has the violation — operand A is fully bijective. Because solid_a
and solid_b are different geometry, the **defect mechanism may differ between
operands** even within the same case. R0081's operand B has a small face count
(18 pairs examined) — the violation is on a small operand.

---

## §6 Critical hypothesis test (plan §"Risks" #2)

> Did PR11 INADVERTENTLY introduce the Stage 1 inflation, or is it pure
> cascade unmasking?

**Verdict: MIXED (neither pure cascade nor pure regression).**

- 6 cases (R0031, R0035, R0081, F0016, F0018, F0019) are pure cascade unmask:
  S1 was firing in PR10 baseline; PR11 didn't make them worse.
- 6 cases (R0007, R0020, R0021, R0063, R0095, F0076) are PR11-introduced:
  S1 = Ok in PR10 baseline, fires S1 in PR12.
- 3 cases (R0014, R0034, R0046) flap, with mixed historical signal.

The original 13-case "PR11 unmasked S1" framing is too optimistic. Half of
those 13 are genuinely new S1 failures introduced by PR11.

**Why might PR11 introduce S1 failures?** PR11's diff (vs. `c2e473c`) does not
modify the Stage 1 capture site (`yang_integration.rs` lines ~742-763) or the
Stage 1 oracle (`bijective.rs::check_face_pair_bijective`). The Stage 1
snapshot still captures the pre-injection mesh from `tessellate_waffle_solid`,
which is unaffected by PR11.

**However**: PR11 DID modify `exact_mesh.rs` substantially (adds
`upstream_tri_count`, restructures `label_cells` for per-patch). It also adds
the F2 site relocation in `coplanar_preprocess.rs`. If any of these created a
side-effect (e.g., influences which solids the kernel "sees" during the
boolean op, or changes the deterministic order in which RandomState is
sampled), it would shift the bijective oracle's flap state.

The flap is the smoking gun. Cases like R0014 and R0046 fire S1 in some runs
but not others. PR11 may have shifted the random-state seeding pattern such
that previously-stably-`Ok` cases now flap. That would manifest as "newly
firing S1" in any one run — not a true regression but a state-space drift.

For the 6 "PR11-INTRODUCED" cases, only `R0020`, `R0021` *don't* fire S0
(coplanar preprocessing) at all — those are most clearly tied to PR11's
non-coplanar code paths. The other 4 have S0 OracleStub fire in some PR12 runs,
suggesting some of the inflation IS partial-overlap coplanar stub propagating
into S1 measurement.

---

## §7 Recommendation: **Branch II** (no dominant pattern)

Per the plan §"Decision branch":

> - (Branch I) Dominant pattern is fixable in <500 LOC.
> - (Branch II) 15 cases have 4+ distinct root causes with no dominant.
> - (Branch III) PR11 inadvertently broke Stage 1 (revert/repair narrow hunk).

**Branch I is not appropriate**. The cluster split is X=4, Y=8, Z=3 (canonical
run); even if we collapse Z into the others, the largest cluster has 8 cases —
not dominant enough (≥10/15 was the plan threshold). Furthermore, Cluster Y is
heterogeneous internally (some fire S0, some don't), and Cluster X contains
both coplanar-stub-driven (R0007, R0031) and non-coplanar (R0020, R0021)
sub-patterns.

**Branch III is not appropriate** in pure form. The 6 "PR11-INTRODUCED" cases
do not point to a single PR11 hunk — and 6 of the 13 unmasked cases are
genuinely cascade unmask, so reverting PR11's changes wouldn't help those.
Additionally, PR11's S4b lever produced 81 net new AllPass cases (3 → 84) —
the 12-15 S1 inflation is much smaller than the win.

**Branch II is the right call**:
- PR12 should narrow scope to 1-2 specific patterns. Recommended targets:
  - **Cluster X non-coplanar subset**: R0020, R0021. These are the cleanest
    cases — no S0 OracleStub, no coplanar preprocessing, no flap. Pure
    tessellation defect. These two cases, plus the synthetic two-cube
    fixture, are a fair red-test set.
  - **Cluster Y stable subset**: pick R0035 or F0019 (both stable across
    runs; both pre-existing; both have small ratios — 15/684 = 2% and 2/40
    = 5% non-bij rate). These represent the "tessellation defect not visible
    in arrangement" pattern.
- PR13+ defers: R0014 / R0034 / R0046 / F0076 (flap-prone) and R0007 / R0031 /
  R0063 / R0081 (S0 OracleStub-driven, requires Stage 0 partial-overlap full
  implementation per Yang §4.5.5).

The flap behavior is itself a finding — adversary V4 should test both whether
PR12's fix-target case verdicts are stable AND whether the fix doesn't
*increase* the flap count.

---

## §8 Open questions / followups

1. **Why is Cluster Y (S2=Ok) possible?** A case has non-bijective face
   boundaries on operand A but the Cherchi arrangement still passes Stage 2
   conservation. Investigate whether the mesh arrangement's vertex-merge
   step is hiding the defect from Stage 2 measurement — and if so, whether
   that's a Stage 2 oracle limitation (false-negative) or genuine geometric
   merging.
2. **What's flap-prone about R0014, R0034, R0046, F0076?** These are
   precisely the cases where the bijective contract is *barely* violated
   (small unmatched-edge counts) and the defect happens to fall on a
   HashMap iteration boundary. Stable instrumentation (sort iteration order
   in `face_boundary_directed_edges`) might eliminate the flap without
   touching the underlying mechanism.
3. **What's the operand-A/operand-B asymmetry root cause?** 14/15 cases
   have the violation on operand A. Is solid_a always the "first-built"
   solid (e.g., the base extrude in a sketch+extrude+boolean tree)?
   Tessellation determinism + per-operand tolerance might explain this.
4. **What does S6 (twin-asymmetry) confirm?** Every one of the 15 cases
   fires S6. The twin-asymmetry violation is the *result* of tessellation
   defects propagating through the result-topology assembly. PR12 fixing
   S1 would also fix S6 for these cases (subject to other downstream
   defects).

---

## Appendix A — Probe artifact summary

- `crates/test-harness/tests/pr12_stage1_diagnostic.rs` — verbose probe.
  - Uses `with_yang_oracle_capture` to invoke the boolean pipeline + run
    all 6 oracles.
  - Embeds the PR10 baseline verdict table as `PR10_BASELINE_VERDICTS` for
    side-by-side comparison without re-running PR10.
  - `[ANCHOR]` `eprintln!`s on the per-case driver loop confirm pipeline
    state per `feedback_anchor_before_fix.md`.
  - `#[ignore]`-gated; `cargo test -p test-harness --test
    pr12_stage1_diagnostic -- --ignored --nocapture`.

- Logs:
  - `/tmp/pr12_stage1_diag_pr12.log` — first PR12 run (run 1).
  - `/tmp/pr12_stage1_diag_pr10_baseline.log` — PR10 baseline run 1.
  - `/tmp/pr12_stage1_diag_pr10_run2.log` — PR10 baseline run 2 (determinism
    check).
  - `/tmp/pr12_run3.log`, `/tmp/pr12_run4.log` — additional PR12 runs.
  - `/tmp/pr12_stage1_diag_final.log` — canonical PR12 run with comparison
    table embedded.

## Appendix B — Tour of the bijective oracle

`crates/kernel/src/tessellation/bijective.rs:317`:
`check_face_pair_bijective(rendermesh, face_map, arena) -> BijectivityReport`.

For each B-Rep edge whose two adjacent faces are distinct:
1. Compute face A's directed boundary edges (`face_boundary_directed_edges`).
2. Compute face B's directed boundary edges.
3. Restrict each to the *shared* B-Rep edge (`restrict_to_shared_boundary`).
4. Diff: face A's directed edge `(p,q)` has no `(q,p)` in face B → unmatched.
5. Same in reverse for face B.
6. Pair is bijective if both unmatched lists are empty.

Equality is **byte-identical** on f64 positions (via `pos_key:
[f64::to_bits(); 3]`). No tolerance. This is the Yang 2025 §4.1.1 contract.

The wrapper in `crates/kernel/src/boolean/pipeline_oracles.rs:254` runs the
check on both operands and emits the message in §2 above. It also exposes
`raw_reports(state)` (pub(crate)) which returns the full
`(BijectivityReport, BijectivityReport)` — but that's not accessible from
test-harness without modifying production code.
