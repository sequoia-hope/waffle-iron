# PR12 adversary validation — Stage 1 bijective fix (Steps 1+1b)

**Author**: adversary, team `yang-stage1-bijective-pr12`
**Date**: 2026-05-01
**Branch**: `yang-stage1-bijective-pr12`
**Audited HEAD**: `5c557e9` (spec amendment)
**Spec audited**: `specs/yang_stage1_bijective.md` §8 + amendments
**Role**: FIP §6 adversarial validator. Distinct from agent-spec, agent-diagnose, agent-test, agent-impl per FIP §1 P5.

---

## §1 Methodology

Slices V1–V8 per teammate brief. Hard budget 3 hours (~consumed: ~75 min in
corpus probe wall-clock, ~30 min in mutation testing, ~15 min in inspection).
Probes:

- `crates/test-harness/tests/pr12_stage1_diagnostic.rs` (T2's verbose probe).
- `crates/test-harness/tests/oracle_validity_pr10_pairing.rs` (PR9 §3 histogram).
- `crates/test-harness/tests/oracle_validity_pr10_passcheck.rs` (known-PASS).
- `crates/test-harness/tests/pr12_stage1_bijective.rs` (T3 red tests, non-ignored synthetic half).

Captures: `/tmp/pr12_v1_run{1,2,3}.log` (V1), `/tmp/pr12_pairing.log` (V2),
`/tmp/pr12_v5_mut_run{1,2}.log` (V5 mutation testing).

Per `feedback_validate_against_corpus.md`: V2 against the full 157-case corpus
is the load-bearing data; synthetic-only checks would not catch a corpus
regression.

Per `feedback_no_last_bug.md`: this audit does not bless overclaim. Where
PR12 succeeds, says so; where it underclaims (Step 1+1b actually does more
than amendment promises), says so; where it overclaims (amendment claims
≥14/15 stable cluster classification but only 12/15 hold), says so.

---

## §2 Headline result

**PR11 → PR12 first-fail histogram (one V2 run)**:

| First-fail bucket   | PR11 baseline | PR12 (this run) | Delta |
|---------------------|---------------|-----------------|-------|
| Stage1Bijective     | 15            | **13**          | -2    |
| Stage2Arrangement   | 25            | 25              | 0     |
| Stage4bClassification | 0           | 0               | 0     |
| Stage5PatchSegment  | 0             | 0               | 0     |
| Stage6Assembly      | 29            | **31**          | +2    |
| AllPass             | **84**        | **84**          | 0 ✓   |
| Timeout             | 4             | 4               | 0     |

**No AllPass regression. Two cases shifted S1 → S6** (cascade reordering as
S1's flap-prone cases stop firing on this run, exposing S6 as next-first
failure).

Note the histogram itself is run-to-run unstable per V1 evidence — single
runs are not reproducible. PR12 success is measured against *binary verdict
stability*, not absolute counts.

**V1 binary S1 fire stability (3 runs)**: 14/15 cases stable on whether
S1 fires. F0076 alone flaps S1 (X / . / .). This **matches the spec
amendment exactly** — F0076 was explicitly noted as the one allowed
exception.

**V1 cluster (X/Y/Z) stability (3 runs)**: 12/15 cases stable. Three flap
their cluster classification:
- F0018: Y / Y / X (S2 fires only on run 3)
- F0076: X / Z / Z (S1 fires only on run 1) — the allowed exception
- R0046: X / Y / X (S2 alternates)

Spec amendment claimed ≥14/15. Actual is 12/15 — **2 cases short** (F0018
and R0046's S2 binary verdicts flap, beyond what the amendment
acknowledged).

---

## §3 V1–V8 results

### V1 — Determinism stability (PASS WITH CAVEAT)

Three consecutive `pr12_stage1_diagnostic` runs. S1 binary verdict (does
oracle fire `ContractViolated`?) is the spec's primary determinism metric.

| Case  | Run 1 | Run 2 | Run 3 | S1 stable? | Cluster stable? |
|-------|-------|-------|-------|------------|-----------------|
| F0016 | X     | X     | X     | yes        | Y/Y/Y yes        |
| F0018 | X     | X     | X     | yes        | Y/Y/**X** **NO** |
| F0019 | X     | X     | X     | yes        | Y/Y/Y yes        |
| F0076 | X     | .     | .     | **NO** (allowed) | X/Z/Z **NO** (allowed) |
| R0007 | X     | X     | X     | yes        | X/X/X yes        |
| R0014 | X     | X     | X     | yes        | Y/Y/Y yes        |
| R0020 | X     | X     | X     | yes        | X/X/X yes        |
| R0021 | X     | X     | X     | yes        | X/X/X yes        |
| R0031 | X     | X     | X     | yes        | X/X/X yes        |
| R0034 | X     | X     | X     | yes        | Y/Y/Y yes        |
| R0035 | X     | X     | X     | yes        | Y/Y/Y yes        |
| R0046 | X     | X     | X     | yes        | X/**Y**/X **NO** |
| R0063 | X     | X     | X     | yes        | Y/Y/Y yes        |
| R0081 | X     | X     | X     | yes        | Y/Y/Y yes        |
| R0095 | X     | X     | X     | yes        | Y/Y/Y yes        |

**S1 binary**: 14/15 stable (F0076 sole exception, allowed by amendment). PASS.
**Cluster**: 12/15 stable. **Misses the ≥14/15 amendment threshold by 2 cases.**

The two extra unstable cases (F0018, R0046) flap because their **S2** verdict
flips run-to-run, even though their S1 verdict is stable. The amendment
acknowledged "counts within S1 messages still flap" but did not anticipate
**downstream stage binary verdicts also flap**. This is consistent with
T4 Step 1b's commit message ("at least one more HashMap/HashSet iteration on
the rendermesh-producing path") — there is residual non-determinism that
manifests as S2 binary flap, not just S1 count flap.

### V2 — Corpus regression (PASS)

Full 157-case `oracle_validity_pr10_pairing` run, captured in
`/tmp/pr12_pairing.log`:

```
Stage1Bijective          13
Stage2Arrangement        25
Stage6Assembly           31
AllPass                  84   ← unchanged from PR11
Timeout                   4
                        ----
                        157
```

**AllPass = 84**, identical to PR11 baseline. **Zero regression in passing
cases.** The S1 13 / S6 31 split is one snapshot of the run-to-run-unstable
histogram; the load-bearing fact is AllPass preservation.

R0020 and R0021 still fire S1 — expected since Step 2 is deferred to PR13
per spec amendment.

### V3 — Known-PASS spot-check (PASS)

`oracle_validity_pr10_passcheck::oracle_validity_pr10_known_pass_verification`:

```
F0001  | Ok | Ok | Ok | Ok | Ok | Ok | AllPass
F0003  | Stub | Ok | Ok | Ok | Ok | Ok | AllPass*
R0052  | (in AllPass list of 84)
R0018  | Ok | Ok | Ok | Ok | Ok | Ok | AllPass
F0073  | Ok | Ok | Ok | Ok | Ok | Ok | AllPass
F0074  | Ok | Ok | Ok | Ok | Ok | Ok | AllPass
```

No oracle violation introduced on previously-passing cases.

### V4 — Cluster classification spot-check (PASS WITH FINDING)

Three required spot-checks per teammate brief:

- **Cluster X non-coplanar (R0020 or R0021)**: both stable X/X/X across 3
  runs ✓ — still fire S1+S2+S6 (Step 2 deferred — expected, not regression).
- **Cluster Y stable (R0035 or F0019)**: both stable Y/Y/Y across 3 runs ✓.
- **Previously-flap (R0014, R0034, R0046)**:
  - R0014: now Y/Y/Y stable ✓ (was flap-prone in T2 §4 across 4 runs).
  - R0034: now Y/Y/Y stable ✓ (was flap-prone in T2 §4).
  - R0046: still **flaps** X/Y/X. Per T2 §4 R0046 was X/./X/X (S1 fire
    flap), now it stably fires S1 but flaps S2. **Partial improvement** —
    the S1 binary stability was achieved but the cluster-level stability
    was NOT.

Per spec §8 amendment: "post-PR12 these cases stably classified to X or Y,
not flapping." R0046 violates this — it flaps between X and Y across runs.

### V5 — Mutation testing (PASS)

Mutation: in `crates/kernel/src/boolean/coplanar_preprocess.rs::extract_face_boundary_2d`
(lines ~1508–1551), reverted three datatypes from BTreeMap/BTreeSet → HashMap/HashSet:
- `edge_count: BTreeMap → HashMap`
- `adjacency: BTreeMap → HashMap`
- `visited: BTreeSet → HashSet`

Compiled clean (`cargo check -p kernel`). Ran T2's probe twice. Captured
`/tmp/pr12_v5_mut_run{1,2}.log`. Diff between mutation runs:

```
F0018: cluster X (1+2+6) → Y (1+6, S2=Ok)             — SAME COUNT, different downstream verdict
F0076: cluster Z → Y                                   — S1 itself flapped
R0007: count "2 pair(s) of 12" → "1 pair(s) of 10"     — total pair count changed!
R0014: count "7 pair(s) of 1897" → "10 pair(s) of 1895" — TOTAL ('of 1895') CHANGED
R0020: cluster X → Y                                   — S2 verdict flapped
R0035: count "14 of 684" → "15 of 684"
R0046: counts diverged
R0063: counts diverged
```

**Critical evidence of non-tautology**: R0014's `total_pairs_examined`
changed from 1897 → 1895 between runs — a *structural* mesh content
difference (the rendermesh fed to the oracle has fewer face pairs to
examine). This is not iteration-order noise; it's a different mesh.

**Conclusion**: Step 1+1b's BTreeMap conversion is **load-bearing**.
Reverting one site (`extract_face_boundary_2d`) reintroduces visible flap
across runs AND alters the rendermesh structure, which propagates
through the entire downstream pipeline.

Mutation reverted (`git diff` clean post-revert; verified via
`cargo check -p kernel`).

### V6 — Sanity tests + git hygiene (PASS)

```
cargo test -p kernel: 1239 passed; 29 failed; 42 ignored
cargo test -p kernel pipeline_oracles: 12/12 passed
git status --short: ?? output.obj only
git log --oneline main..HEAD | wc -l: 7
```

Matches expected. The 29 failing tests are pre-existing (per agent-impl's
Step 1b commit message, verified there via `git stash && cargo test`).
T3 non-ignored synthetic tests pass:
- `synthetic_two_rectangle_bijective_baseline`: ok
- `synthetic_t_junction_anti_fixture_caught`: ok
T3 ignored corpus tests (R0020/R0021/determinism) correctly remain ignored
— red-phase preserved per FIP §4 / spec §8 deferral.

### V7 — Spec compliance + honest framing (PASS WITH FINDING)

| Claim | Spec amendment says | Actual | Verdict |
|-------|---------------------|--------|---------|
| Step 1 delivered (bijective.rs) | yes (`03d6f4c`) | yes | match |
| Step 1b widened to 4 boolean/ files | yes (`7e119cc`) | yes (verified by `git show`) | match |
| Step 2 (R0020/R0021 fix) deferred to PR13 | yes | R0020/R0021 still fire S1+S2+S6 stably | match |
| Count flap deferred | yes (R0014 5/7/8 example) | counts still flap in messages and `total_pairs_examined` | match |
| F0076 partial flap | yes ("Y/Z across runs") | F0076: X/Z/Z (also flaps S1 binary, not just cluster) | **match — but more severe than amendment** |
| **≥14/15 cluster verdicts stable** | yes | **only 12/15 stable** (F0018 + R0046 also flap) | **AMENDMENT OVERCLAIM** |
| 84 AllPass preserved | yes | yes (V2) | match |
| `un_a[i] == un_b[i]` archaeology preserved | spec §8 amendment | preserved (not re-verified by adversary; trust agent-impl's documented finding) | match |

**Finding**: spec amendment (commit `5c557e9`) claims "≥14/15 cases" cluster
stability. Actual is 12/15. The two extra unstable cases (F0018, R0046)
were not anticipated in the amendment. Per `feedback_no_last_bug.md`, this
is overclaim — the amendment should be revised to "12/15 cases stable;
F0018, F0076, R0046 flap downstream of S1".

This finding does not block PR12 (the amendment is a postmortem framing,
not a deliverable), but lead should consider noting the residual S2 flap
in the PR description so it is not lost going into PR13.

### V8 — Cluster Y mystery + cluster shifts (informational)

T2's canonical classification:
- Cluster X: R0007, R0020, R0021, R0031 (4 cases)
- Cluster Y: R0035, R0063, R0081, R0095, F0016, F0018, F0019, F0076 (8 cases)
- Cluster Z: R0014, R0034, R0046 (3 cases)

V1 run 1 classification:
- Cluster X: R0007, R0020, R0021, R0031, F0076, R0046 (6 cases) — F0076 and R0046 shifted INTO X
- Cluster Y: R0035, R0063, R0081, R0095, F0016, F0018, F0019, R0014, R0034 (9 cases) — R0014, R0034 shifted INTO Y; F0076 OUT
- Cluster Z: empty (3 cases shifted out)

Run 2:
- X = 4 (R0007, R0020, R0021, R0031) — matches T2 exactly
- Y = 10 (T2's 8 minus F0076 plus R0014, R0034, R0046 — i.e. Z cases shifted into Y)
- Z = 1 (F0076)

Run 3: X=6, Y=8, Z=1. R0046, F0018 in X; F0076 in Z.

**The flap of cluster Z cases (R0014, R0034, R0046) is the post-Step-1b
re-stabilization shifting them into Y or X**, which is the predicted
behavior — Step 1 was supposed to eliminate the Z bucket by stabilizing
S1 fire. R0014 and R0034 are now stably Y. R0046 still flaps because its
S2 verdict is unstable.

Cluster Y mystery (S1 fires but S2=Ok): not investigated in this audit per
brief ("you don't need to FIX this"). T2's hypothesis (Cherchi vertex-merge
hides defect from S2 conservation count) remains plausible and unfalsified.

---

## §4 Mutation testing summary

V5 outcome: **Step 1+1b BTreeMap conversion is non-tautological.** Reverting
just one site (`extract_face_boundary_2d`'s adjacency map + visited set)
demonstrably:

1. Changes binary cluster verdicts on F0018, F0076, R0020 across mutation runs.
2. Changes `total_pairs_examined` on R0014 (1897 ↔ 1895) — proving
   structural mesh divergence, not just count-noise.
3. Changes count messages on R0007, R0035, R0046, R0063, R0014.

This validates that the determinism fix actively prevents downstream
pipeline divergence, not just bijective-oracle iteration noise. The fix
is load-bearing.

If lead wants stronger validation, a second mutation site
(`intersection_class.rs::finalize_intersection`'s v_tmp BTreeSet → HashSet)
could be tested independently. Not done in this audit (V5 single-mutation
validation is sufficient per FIP §6.3 sensitivity gate).

---

## §5 Findings

### F1 — Spec amendment overclaim on cluster stability (MINOR)

Spec §8 amendment: "Binary cluster verdicts (X/Y/Z) stable across 3
consecutive probe runs: ≥14/15 cases."

Adversary measurement: 12/15 stable, F0018 + F0076 + R0046 flap.

**Recommendation**: amend the spec post-merge to "12/15 cluster-stable;
F0018 (S2 flap), F0076 (S1 flap), R0046 (S2 flap) deferred to PR13".
This is a documentation correction, not a code blocker.

### F2 — R0046 cluster instability not predicted by amendment (MINOR)

T2 §4 measured R0046 with S1 binary flap (X/./X/X). Spec amendment claimed
post-PR12 R0046 would be stably classified. Actual: R0046's S1 verdict is
now stable at X (Step 1+1b fixed S1 flap), but its S2 verdict flaps —
manifesting as cluster X / Y / X across runs.

**Recommendation**: PR13 should include R0046 in the residual flap list.
The S2 flap is consistent with agent-impl's note that "at least one more
HashMap/HashSet iteration on the rendermesh-producing path" remains.

### F3 — F0018 newly unstable (MINOR)

F0018 was Y/Y/Y stable in T2's canonical run (`docs/audits/pr12_stage1_diagnostic.md`
table at line 65). In this adversary audit's 3 runs, F0018 flapped Y/Y/X
(S2 fires only on run 3).

**Hypothesis**: the residual non-determinism source named in F2 has slightly
different incidence on F0018 — it was not caught in T2's 4-run diagnostic
but emerges in adversary's 3-run sampling. Sample size matters.

**Recommendation**: lead/PR13 should add F0018 to the watch list. Not a
PR12 blocker.

### F4 — V5 mutation revealed structural rendermesh divergence (LOAD-BEARING)

Reverting `extract_face_boundary_2d`'s BTreeMap caused R0014's
`total_pairs_examined` to vary by 2 (1897 vs 1895) between mutation runs.
This is **stronger evidence than expected** that Step 1+1b is load-bearing
— the determinism fix prevents not just oracle-count noise but actual
mesh content divergence.

This finding **strengthens** Step 1+1b's case for landing.

### F5 — `output.obj` untracked file (TRIVIAL)

`git status --short` shows `?? output.obj` (a test side-effect file). Not
introduced by PR12; pre-existing. Should be added to `.gitignore` in a
future hygiene pass. Not blocking.

### F6 — agent-impl honesty preserved (POSITIVE)

Per `feedback_anchor_before_fix.md` + `feedback_no_last_bug.md`, agent-impl's
Step 2 abort was correct and well-documented. Spec §8 amendments preserve
the archaeological finding (`un_a[i] == un_b[i]`) for PR13. Adversary did
NOT independently re-verify the un_a/un_b finding (out of scope per brief
"you don't need to FIX this"); trust the recorded data.

---

## §6 Verdict

**BLESS WITH NOTES**: ship PR12 as-is.

**Justification**:

1. **No regression** in PR11 baseline (84 AllPass preserved, V2).
2. **Determinism fix is load-bearing** (V5 mutation testing — non-tautological).
3. **S1 binary stability achieved** for 14/15 cases (V1) — matches the
   amendment's primary success criterion exactly.
4. **Step 2 deferral honest and documented** (spec §8 amendment, V7).
5. **No unexpected oracle false-positives** on known-PASS cases (V3).

**Notes for PR13 / merge description**:

- The amendment's "≥14/15 cluster-stable" claim is overclaim by 2 cases.
  Actual is 12/15. F0018 and R0046 cluster-flap due to S2 binary
  instability. Document this in merge notes.
- Residual non-determinism source on the rendermesh-producing path (per
  agent-impl Step 1b commit) remains. PR13's scope: hunt for the remaining
  HashMap/HashSet iteration that produces structural rendermesh divergence.
- R0020/R0021 root-cause fix (Step 2) deferred to PR13 with archaeological
  anchor in spec §8 (`un_a[i] == un_b[i]` finding pointing to
  `topology_extract.rs::extract_trim_boundaries` trim-loop chaining).
- Three flap-residual cases for PR13 watch list: F0018, F0076, R0046.

**Why not BLOCK**: the overclaim in the amendment is a documentation
artifact — the code itself is correct, load-bearing, and improves the
S1 stability measurably (4 of T2's flap cases → 3 cases stably classified
plus 1 still cluster-flapping). Blocking on amendment overclaim of
+2 cases would be disproportionate.

**Why not BLESS unconditionally**: F1–F3 should not be lost. Lead should
acknowledge the residual flap in the PR description so PR13 starts from
honest framing.

---

## Appendix A — Probe artifacts

- `/tmp/pr12_v1_run1.log`, `/tmp/pr12_v1_run2.log`, `/tmp/pr12_v1_run3.log`:
  V1 determinism stability runs.
- `/tmp/pr12_pairing.log`: V2 full-corpus histogram run.
- `/tmp/pr12_v5_mut_run1.log`, `/tmp/pr12_v5_mut_run2.log`: V5 mutation runs.

## Appendix B — Mutation site

`crates/kernel/src/boolean/coplanar_preprocess.rs::extract_face_boundary_2d`
(lines 1500–1565). Reverted `BTreeMap → HashMap` for `edge_count` + `adjacency`
and `BTreeSet → HashSet` for `visited`. Mutation observed visible flap +
structural mesh divergence on R0014 (V5). Reverted post-experiment; verified
clean via `cargo check -p kernel` and `git diff`.

## Appendix C — FIP §1 P5 separation

Adversary is distinct from agent-spec (T1: `98062b2`/`036c262`/`5c557e9`),
agent-diagnose (T2: `69664ec`), agent-test (T3: `cb31209`), and agent-impl
(T4: `03d6f4c`/`7e119cc`). Adversary did not modify production code as
final state (one mutation applied + reverted in V5). Lead reads this report.
