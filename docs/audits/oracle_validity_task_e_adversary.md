# PR10 Oracle Validity — Task E: Adversary Verification of Synthesis

> Owner: adversary (oracle-validity-audit-pr10)
> Date: 2026-05-01
> Subject: `specs/oracle_validity_audit.md` @ `cc07486`
> Source reports verified against:
> - `docs/audits/oracle_validity_task_a_mutation.md` (T1, agent-mutation)
> - `docs/audits/oracle_validity_task_b_passcheck.md` (T2, agent-passcheck)
> - `docs/audits/oracle_validity_task_c_pairing.md` (T3, agent-pairing)
> Per FIP §1 P5: distinct auditor; lead reads.

## §1 Methodology

Hard budget: 1 hour. Three spot-checks performed, headline numbers
cross-verified, caveat carry-through audited:

1. **Headline number verification** — every claim in synthesis §2
   matched against its cited source report for verbatim consistency.
2. **F1 (Stage 2 tautology)** — re-applied M2 mutation on
   `crates/kernel/src/boolean/exact_mesh.rs:2436` (append `sub_tris_a.pop();`),
   ran the corpus runner anchor on F0001, captured oracle verdicts,
   reverted via `git checkout --`.
3. **F2 (Stage 0 snapshot-capture)** — code inspection of
   `yang_integration.rs` lines 614-720 to verify three claims about the
   pre-injection / snapshot-record / flat-array-mutation sequencing.
4. **Task C cell spot-check** — re-ran the corpus anchor on F0002 with
   clean code; confirmed Stage2Arrangement bucket and X cells in s2,
   s4b, s6.
5. **Caveat carry-through** — read both source reports' "What this task
   did NOT cover" sections and synthesis §6 for omissions.

No production code modified at end of session; final `git status` clean
except for untracked `output.obj` (pre-existing).

## §2 Headline number verification

Every synthesis §2 claim verified verbatim against its cited source:

| Synthesis §2 claim                                       | Source quote                                                                                       | Match? |
|----------------------------------------------------------|----------------------------------------------------------------------------------------------------|:------:|
| Stage 2 → Stage 4b shadowing 28/28 (100 %)               | Task C §3 Claim 1: "28/28 (100.0 %) of Stage2Arrangement first-fail cases also fire Stage 4b"      |   ✓    |
| Stage 2 → Stage 6 propagation 28/28 (100 %)              | Task C §3 Claim 1: "28/28 also fire Stage 6"                                                       |   ✓    |
| Stage 4b → Stage 6 propagation 120/120 (100 %)           | Task C §3 Claim 2: "120/120 (100.0 %) of Stage4bClassification first-fail cases also fire Stage 6" |   ✓    |
| Stage 4b → Stage 5 propagation 0/120                     | Task C §3 Claim 2: "Stage 4b → Stage 5 propagation rate is 0/120 (0 %)"                            |   ✓    |
| 0 / 48 false-positives                                   | Task B §2: "0 `VIOLATION` cells / 48 cells total. 0 false-positives."                              |   ✓    |
| 3 / 5 mutations caught                                   | Task A §3: "5 mutations applied, 3 caught at the targeted oracle, 2 missed"                        |   ✓    |
| 2 / 5 mutations missed                                   | Task A §3: same line as above                                                                      |   ✓    |
| Stage 4b first-fail: 120/157 (was 32/157 in PR9)         | Task C §7 table: "Stage4bClassification: PR9=32; this audit=120; Δ=+88"                            |   ✓    |
| AllPass: 3/157 (was 72/157 in PR9)                       | Task C §7 table: "AllPass: PR9=72; this audit=3; Δ=−69"                                            |   ✓    |
| Synthesis §3 Stage 4b "fires on 150/157 corpus"          | Task C §2 column total S4b = 150; §2 also says "fires on 95.5 % of the corpus (150/157)"           |   ✓    |
| Synthesis §3 Stage 6 "fires on 151/157 corpus"           | Task C §2 column total S6 = 151; §2 also says "96.2 % of the corpus"                               |   ✓    |
| Synthesis §3 Stage 1: 2 first-fail (R0031, R0081)        | Task C §6 anchors: `R0031 | Stage1Bijective`, `R0081 | Stage1Bijective`                            |   ✓    |
| Synthesis §3 Stage 5: 0/157 violations                   | Task C §2 column total S5 = 0                                                                      |   ✓    |

**No discrepancies.** All headline numbers in the synthesis match their
source reports verbatim.

## §3 F1 spot-check — Stage 2 tautology (M2 re-applied)

Applied per Task A §2.M2:

```diff
@@ exact_mesh.rs subdivide_mesh_pair_full_cherchi @@
     let n_verts = result.coords.len();
+    sub_tris_a.pop();
     Ok(SubdividedMesh {
```

Built `cargo build -p kernel`: success (warnings only, no errors).

Ran `YANG_BOOLEAN=1 cargo test -p test-harness --test
pr9_pipeline_oracle_corpus pr9_corpus_runner_captures_snapshots_anchor
-- --ignored --nocapture`. F0001 verdicts (verbatim from stderr):

```
F0001 PR9 oracle run summary:
  first_failing  = Some(Stage6Assembly)
    Stage0Coplanar / CoplanarMeshIdenticalOracle → PASS / skipped
    Stage1Bijective / BijectiveFacePairOracle → PASS / skipped
    Stage2Arrangement / MeshArrangementWellFormedOracle → PASS / skipped
    Stage4bClassification / LabelConsistencyWithinPatchOracle → PASS / skipped
    Stage5PatchSegment / ManifoldPatchConservationOracle → PASS / skipped
    Stage6Assembly / TwinSymmetryOracle → ContractViolated:
        half_edge[1].twin = 0 but twin.twin = 17 (expected 1)
  bucket = Stage6Assembly
```

**Result**: matches Task A §2.M2 exactly. Stage 2 oracle reports PASS/
skipped despite the dropped triangle (cascade caught at Stage 6
instead). The conservation check is structurally tautological: dropping
a sub-triangle shrinks both `tris_a.len()` and the directed-edge total
proportionally, so the equation stays satisfied.

Reverted via `git checkout -- crates/kernel/src/boolean/exact_mesh.rs`.
`git diff` clean post-revert.

**F1 verdict: confirmed.** Stage 2 oracle is tautological. Synthesis
§4 F1 holds.

## §4 F2 spot-check — Stage 0 snapshot-capture (code inspection)

Read `crates/kernel/src/boolean/yang_integration.rs` lines 600-720.
Three claims verified by inspection:

### Claim 1 — `mesh_a` and `mesh_b` are bound from `tessellate_waffle_solid` BEFORE injection runs

Lines 614-615:
```rust
let mesh_a = tessellate_waffle_solid(&solid_a_mod, lod)?;
let mesh_b = tessellate_waffle_solid(&solid_b_mod, lod)?;
```

Injection at lines 661-687:
```rust
if !coplanar_pairs.is_empty() {
    crate::boolean::coplanar_preprocess::inject_identical_footprint_mesh(
        &coplanar_pairs,
        &mut verts_a, &mut tris_a, &mut bijective_a,
        &mut verts_b, &mut tris_b, &mut bijective_b,
    );
}
```

**Verified ✓.** Bindings are pre-injection.

### Claim 2 — Snapshot at line 702 records those pre-injection RenderMeshes

Lines 696-708:
```rust
let mesh_a_for_snap = mesh_a.clone();      // clone of pre-injection RenderMesh
let mesh_b_for_snap = mesh_b.clone();
...
crate::boolean::pipeline_oracles::record_snapshot(move |bundle| {
    bundle.stage_0_coplanar = Some(
        crate::boolean::pipeline_oracles::CoplanarPreprocessSnapshot {
            pairs: pairs_for_snap,
            mesh_a: Some(mesh_a_for_snap.clone()),
            mesh_b: Some(mesh_b_for_snap.clone()),
        },
    );
    ...
});
```

**Verified ✓.** Snapshot captures `mesh_a.clone()` and `mesh_b.clone()`
— RenderMeshes that have not been touched between line 614/615 and line
696/697.

### Claim 3 — `inject_identical_footprint_mesh` mutates flat arrays separately

Inspection of the call signature at line 662-670: parameters are
`&mut verts_a, &mut tris_a, &mut bijective_a, &mut verts_b, &mut tris_b,
&mut bijective_b`. The `mesh_a`/`mesh_b` RenderMesh values are **not**
passed to the injection function. The flat arrays
(`verts_a`/`tris_a`/`verts_b`/`tris_b`) and bijective maps are the
ones mutated.

**Verified ✓.** Injection does not touch `mesh_a`/`mesh_b` RenderMeshes.

### F2 verdict

All three claims hold by code inspection. The snapshot-capture site
records pre-injection state. The Stage 0 oracle is therefore comparing
RenderMeshes that have not been touched by `inject_identical_footprint_mesh`,
so it cannot detect byte-divergence introduced at injection time.

**F2 verdict: confirmed.** Synthesis §4 F2 holds.

## §5 Task C cell spot-check — F0002

Re-ran the corpus anchor with clean code (M2 reverted). F0002 verdicts:

```
F0002 PR9 oracle run summary:
  pipeline_error = None
  first_failing  = Some(Stage0Coplanar)
    Stage0Coplanar / CoplanarMeshIdenticalOracle → OracleStub: 2 partial-overlap pair(s) not checked
    Stage1Bijective / BijectiveFacePairOracle → PASS / skipped
    Stage2Arrangement / MeshArrangementWellFormedOracle → ContractViolated: tris_a[8] is degenerate
    Stage4bClassification / LabelConsistencyWithinPatchOracle → ContractViolated: patch 2 contains 2 distinct labels
    Stage5PatchSegment / ManifoldPatchConservationOracle → PASS / skipped
    Stage6Assembly / TwinSymmetryOracle → ContractViolated: half_edge[4].twin = 0 but twin.twin = 31
```

Compared against Task C §6 anchor: `F0002 | Stage2Arrangement | S . X X . X`:

| Cell  | Task C trace | Adversary observation                | Match? |
|-------|:------------:|:-------------------------------------|:------:|
| s0    | S            | OracleStub                           |   ✓    |
| s1    | .            | PASS / skipped                       |   ✓    |
| s2    | X            | ContractViolated (degenerate)        |   ✓    |
| s4b   | X            | ContractViolated (mixed labels)      |   ✓    |
| s5    | .            | PASS / skipped                       |   ✓    |
| s6    | X            | ContractViolated (twin asymmetry)    |   ✓    |
| bucket| Stage2Arrangement | Stage2Arrangement (1st X cell)  |   ✓    |

Note: `first_failing` reports `Stage0Coplanar` because Stage 0 returned
`OracleStub`, not `ContractViolated`. Task C's `bucket` aggregation
treats `OracleStub` as non-failing (the convention in Task C §2 legend:
"S = OracleStub" shown as a separate symbol from "X"). The Stage 2
ContractViolated is the first true failure, hence `bucket =
Stage2Arrangement`. Consistent with Task C's bucketing.

**Task C 28/28 claim has at least one independently-confirmed data
point (F0002).**

## §6 Findings

Per `feedback_no_last_bug.md` — be honest about what was and was not
verified.

### What was verified
- All 13 headline numbers in synthesis §2 and §3 match source reports
  verbatim.
- F1 (Stage 2 tautology) reproduces on F0001 with M2 re-applied.
- F2 (Stage 0 snapshot defect) holds by code inspection of all three
  load-bearing claims.
- One Stage 2 first-fail case (F0002) confirmed in Task C bucket and
  cell pattern.

### Minor caveats not load-bearing for PR11
- **Task C §3 caveat about F0073/F0074 vacuous AllPass**: Task C
  explicitly states "of the 3 AllPass cases, 2 (F0073, F0074) are
  vacuous (pipeline-never-ran). Only R0052 is potentially substantive
  AllPass." Synthesis §2 reports "AllPass = 3" without flagging the
  2/3 vacuous split. Synthesis does not rely on AllPass = 3 as a
  confidence anchor anywhere (§5.2 explicitly subtracts AllPass from
  lever-sized cases), so the omission is not load-bearing. Recommend
  the lead consider mentioning it in a future revision but not blocking
  on it.
- **Task B §3 cross-task signal on F0001 Stage 4b**: Task B observed
  that with M5 active, F0001 Stage 4b oracle fired (in addition to
  Stage 6). This is a partial cross-validation of Stage 4b oracle
  effectiveness that the synthesis does not explicitly cite. Task A's
  M3 already establishes Stage 4b detection; the Task B side-observation
  is supplementary, not load-bearing.
- **Synthesis F3 attribution**: Synthesis F3 attributes the
  distribution shift "most likely to the AABB-disjoint short-circuit
  (commit `aee32d8`)." Task C §7 lists multiple candidate commits
  (`60ee841`, `ea03d94`, `a2ea0b9`, `cfec7b8`, `9f3c591`) plus
  `aee32d8`. Synthesis picks the single most-cited cause; this is a
  reasonable distillation but slightly narrower than Task C's hedged
  language. Not a defect, but for a careful reader the synthesis is
  more decisive than the source.

### What was NOT verified
- Mutations 1, 3, 4 from Task A (only M2 re-applied).
- Task B's 8x6 matrix beyond synthesis quote check.
- Task C's other 27 Stage 2 cases, 120 Stage 4b cases, and AllPass
  bucket beyond F0002.
- Whether more mutations or PASS-cases would surface additional
  oracle defects beyond F1/F2. The synthesis acknowledges this in §8
  ("the two false-negatives found are the ones this budget discovered,
  not the ones the suite has").

### No findings warranting BLOCK
No internal inconsistencies. No headline numbers contradicting source.
No overclaimed implications for PR11. The two load-bearing findings
(F1, F2) reproduce. The one Task C cell spot-check matches.

## §7 Verdict

**BLESS.**

Synthesis at `specs/oracle_validity_audit.md` @ `cc07486` accurately
reports its source data. The two load-bearing findings (F1: Stage 2
tautology; F2: Stage 0 snapshot defect) reproduce under independent
verification. The headline numbers match Task A / B / C verbatim. The
Task C 28/28 Stage 2 → Stage 4b claim has at least one independently-
confirmed data point.

Lead may proceed to integration (task #6).

Per `feedback_no_last_bug.md`: this verification covered the load-
bearing claims. Future audits may surface additional gaps not exercised
by this 5-mutation / 8-PASS-case / 157-case-pairing budget. The
synthesis itself acknowledges this in §8.
