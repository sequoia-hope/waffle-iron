# PR10 Oracle Validity — Task A: Mutation Testing

> Owner: agent-mutation
> Branch: `oracle-validity-audit-pr10`
> Date: 2026-04-29
> Per FIP §1 P5: distinct auditor; lead synthesizes.

## §1 Methodology

I applied 5 mutations from the menu to PR9 production code, ran the
targeted oracle via the corpus runner anchor (`R0033 + F0001 + F0002`),
captured oracle verdicts, and reverted each mutation immediately. The
goal: empirically validate whether each oracle catches the defect class
its contract is supposed to cover.

**Hard budget**: 1 hour. **Environment**: native cargo, `cargo build -p
kernel` + `cargo test -p test-harness --test pr9_pipeline_oracle_corpus
pr9_corpus_runner_captures_snapshots_anchor -- --ignored --nocapture`.

**Oracle-run mechanism**: the snapshot collector
(`pipeline_oracles::with_snapshot_collector`) installs a thread-local;
production calls `record_snapshot` at stage boundaries, which is a no-op
unless the collector is installed. The corpus runner installs one per
case, runs `LoadProject`, then runs the 6-oracle default registry on the
captured `OwnedSnapshotBundle`. Anchor cases used:
- **R0033** — AABB-disjoint, exercises only `yang_pipeline_result_for_disjoint`.
- **F0001** — overlapping boxes, exercises full Yang pipeline; baseline AllPass.
- **F0002** — coplanar bottoms, baseline first-fail at Stage 2 / Stage 4b.

Per per-mutation discipline:
1. Apply mutation (verify `git diff` clean + syntactically valid).
2. `cargo build -p kernel` (compile sanity).
3. Run anchor; capture each case's first-failing-stage and per-oracle verdict.
4. **Revert via `git checkout -- <file>`** before next mutation.

## §2 Per-mutation results

### Mutation 1 — Stage 6 twin asymmetry (PASS)

- **File/line**: `crates/kernel/src/boolean/topology_extract.rs:863`
- **Change**: `arena.half_edges[he_rev.0].twin = he_fwd;` →
  `arena.half_edges[he_rev.0].twin = HalfEdgeIdx(0);`
- **Targeted oracle**: `TwinSymmetryOracle` (Stage 6).
- **Test fixture**: F0001 (full pipeline run with successful baseline).
- **Result**: **PASS** — F0001 baseline=AllPass became
  `first_failing = Some(Stage6Assembly)`; oracle returned ContractViolated.
- **Diagnostic** (verbatim):
  > `half_edge[1].twin = 16 but twin.twin = 0 (expected 1)`
- **Verdict**: TwinSymmetryOracle correctly catches twin reflexivity
  violations on F0001 (baseline-AllPass → mutated Stage6 fail). The
  diagnostic identifies the violated half-edge index and observed twin
  values — sufficient for root-cause attribution.

### Mutation 2 — Stage 2 conservation violation via missing tri (FAIL)

- **File/line**: `crates/kernel/src/boolean/exact_mesh.rs:2436` (after
  `subdivide_mesh_pair_full_cherchi` builds `sub_tris_a`)
- **Change**: appended `sub_tris_a.pop();` to drop the last A
  sub-triangle (off-by-one).
- **Targeted oracle**: `MeshArrangementWellFormedOracle` (Stage 2) —
  conservation contract: `total_directed == 3 × (|tris_a| + |tris_b|)`.
- **Test fixture**: F0001.
- **Result**: **FAIL** — Stage 2 oracle reported `PASS / skipped` (in
  effect Ok) on F0001. Cascade caught at Stage 6 instead
  (`first_failing = Some(Stage6Assembly)`).
- **Diagnostic** (verbatim Stage 2 verdict): `Stage2Arrangement /
  MeshArrangementWellFormedOracle → PASS / skipped`.
- **Hypothesis for false-negative**: Stage 2's conservation check is
  `total_directed == 3 × (subdivided.tris_a.len() + subdivided.tris_b.len())`.
  Both sides come from the SAME snapshot (`subdivided`). When a
  triangle is dropped before the snapshot is taken, the snapshotted
  `tris_a.len()` shrinks, the directed-edge total also shrinks, and
  conservation remains satisfied as a tautology over the snapshot
  itself. The contract is internally self-consistent, not anchored to
  any external invariant (e.g., expected sub-tri count given input
  arrangement output). The oracle as currently written is structurally
  incapable of detecting "lost-during-emit" defects.
- **Severity**: this is the most valuable finding in this report.
  Stage 2's "conservation" check, despite the name, validates only
  internal consistency of the snapshot, not that the snapshot is
  complete relative to upstream Cherchi output. A real conservation
  oracle would compare against an upstream-known invariant — e.g., the
  pre-arrangement triangle count or a checksum from `solve_intersections`
  metadata.

### Mutation 3 — Stage 4b mixed-label patch (PASS, with caveat)

- **File/line**: `crates/kernel/src/boolean/exact_mesh.rs:2034` (end of
  `label_cells`, before constructing `CellLabeling`)
- **Change**: flipped `labels_a[0]` from whatever it is to the opposite
  binary value before returning.
- **Targeted oracle**: `LabelConsistencyWithinPatchOracle` (Stage 4b).
- **Test fixture**: R0033 (where it activated; F0001 caveat below).
- **Result on R0033**: **PASS** — R0033 baseline=AllPass became
  `first_failing = Some(Stage4bClassification)`; oracle returned
  ContractViolated.
- **Diagnostic** (verbatim, R0033):
  > `patch 0 contains 2 distinct labels [Inside, Outside] across 92 sub-tris (Cherchi 2022 §5 Algorithm 1 requires one label per patch); sample flat sub-tri indices: [0, 1, ...] (tris_a_count = 12)`
- **Caveat — F0001 reported Stage 4b skipped**: F0001 with mutation 3
  had `Stage4bClassification → PASS / skipped` despite the boolean
  pipeline running (diag confirmed `[yang-diag] after label_cells: A
  outside=105 inside=13`). This indicates the Stage 4b snapshot was
  not landed in the bundle for F0001 specifically — likely because
  F0001 was bucketed into a code path that does not call
  `record_stage_2_4b_6_snapshots` (the AABB-disjoint short-circuit at
  `topology_extract.rs:1467` and the main pipeline at `:1819` both
  call it; some other path may not). This is a **snapshot-coverage
  gap**, NOT an oracle false-negative — but worth flagging because
  the corpus runner can only judge what reaches it.
- **Verdict**: oracle catches mixed-label patches when reached. R0033
  confirms the empirical detection. The F0001 gap is a separate
  snapshot-recording defect that limits oracle coverage in the corpus.

### Mutation 4 — Stage 2 degenerate triangle (PASS, after retry)

- **First attempt** (per menu): `tessellation/mod.rs::tessellate_solid_bounded`
  — append a degenerate triangle [v0, v0, v1] after the per-face loop.
  - **Result**: **INCONCLUSIVE on the menu's targeted entry point**.
    F0001 reported AllPass; the degenerate triangle was filtered by
    upstream Cherchi preprocessing (`STAGE2 degenerate: 60 tris`
    rejects collinear/zero-area input) before it could reach the Stage 2
    oracle's snapshot. The mutation does not survive to the snapshot
    surface.
  - The mutation's actual effect is upstream of the oracle's input
    boundary; this is a coverage-mechanism issue, not an oracle defect.
- **Retried at the right level** (still per menu's contract): `exact_mesh.rs::subdivide_mesh_pair_full_cherchi` —
  appended `SubTriangle { verts: [0, 0, 1], parent_tri: 0,
  cosurface_orientation: None }` to `sub_tris_a` immediately before
  emitting the `SubdividedMesh`. This places the degenerate INSIDE the
  Stage 2 snapshot's input range.
  - **Result on F0001**: **PASS** — Stage 2 oracle returned
    ContractViolated.
  - **Diagnostic** (verbatim): `tris_a[12] is degenerate: verts = [0, 0,
    1], positions a=[-0.25, -0.25, 0.300000012] b=[-0.25, -0.25,
    0.300000012] c=[0.25, ...]`.
- **Verdict on Stage 2 oracle's degeneracy contract**: when a
  degenerate sub-tri lands in the snapshot, the oracle catches it
  precisely. The first-attempt failure is informative about coverage
  reach, not oracle correctness.

### Mutation 5 — Stage 0 byte divergence (FAIL — snapshot-capture defect)

- **File/line**: `crates/kernel/src/boolean/coplanar_preprocess.rs:1006`
  (just before `replace_face_triangles` for mesh B in
  `inject_identical_footprint_mesh`)
- **Change**: cloned `shared_3d` into `shared_3d_b` and added 1.0e-5 to
  the x-coordinate of the first vertex (above the `TAU_MODEL = 1e-7`
  weld tolerance in `replace_face_triangles`, so the mutation survives
  the welding pass).
  - **First attempt** used a 1-ULP f32 perturbation (~5.96e-8 for
    unit-scale values). This was **silently absorbed** by
    `replace_face_triangles`'s tol_sq=TAU_MODEL² weld snap — the
    perturbed vertex got mapped to an existing close vertex, neutralizing
    the mutation. Worth flagging: the welding logic in
    `replace_face_triangles` LITERALLY masks 1-ULP-precision divergence
    at the production layer.
- **Targeted oracle**: `CoplanarMeshIdenticalOracle` (Stage 0).
- **Test fixture**: F0001 (which I verified DOES enter
  `inject_identical_footprint_mesh` for 6 pairs via an instrumentation
  eprintln, despite the `[coplanar-tele]` summary reporting
  `identical_footprint=0` — the telemetry counter `snap_identical`
  delta does not match the actual entry count, an unrelated diagnostic
  anomaly).
- **Result on F0001**: **FAIL** — `Stage0Coplanar /
  CoplanarMeshIdenticalOracle → PASS / skipped`.
  `first_failing = Some(Stage6Assembly)` — cascade caught at Stage 6
  twin asymmetry rather than Stage 0 byte-identity.
- **Diagnostic** (verbatim Stage 0 verdict): `Stage0Coplanar /
  CoplanarMeshIdenticalOracle → PASS / skipped`.
- **Root cause** (very important — investigated, not just hypothesized):
  the snapshot-capture site at `yang_integration.rs:702` records
  `mesh_a.clone()` and `mesh_b.clone()` into the bundle. But `mesh_a`
  and `mesh_b` are bound at lines 614-615 from `tessellate_waffle_solid`
  output — **before** `inject_identical_footprint_mesh` runs (line 662).
  The injection mutates the SEPARATE flat arrays
  `verts_a`/`tris_a`/`verts_b`/`tris_b` — NOT the `mesh_a`/`mesh_b`
  RenderMeshes. Thus the Stage 0 oracle is comparing the
  PRE-injection RenderMeshes (which DO trivially agree, because they
  came from independent tessellations and the oracle's plane-triangle
  multiset filter at f32 precision can't even distinguish the
  pre-injection state's identity question).
- **Severity**: this is a **snapshot-capture bug masking the oracle's
  contract entirely** for the identical-footprint case. The oracle's
  unit tests (`identical_footprint_shifted_vertex_in_b_fails`) pass
  because they construct the snapshot directly with post-injection
  meshes; the production capture path never produces such a snapshot.
  PR10 should fix the capture site to reflect the post-injection mesh
  state — without that fix, the Stage 0 oracle is unreachable from the
  corpus runner regardless of how many cases trigger
  identical-footprint pairs.

## §3 Per-oracle false-negative summary

| Stage | Oracle | Mutations targeted | Caught | Missed | Inconclusive |
|------:|:-------|:-------------------|:------:|:------:|:------------:|
|     0 | `CoplanarMeshIdenticalOracle`        | 1 (M5) | 0 | 1 | 0 |
|     2 | `MeshArrangementWellFormedOracle`    | 2 (M2, M4) | 1 (M4-retry) | 1 (M2) | 1 (M4-first attempt at upstream entry point) |
|    4b | `LabelConsistencyWithinPatchOracle`  | 1 (M3) | 1 (R0033 path) | 0 | 0 (F0001 snapshot-recording gap, not oracle defect) |
|     5 | `ManifoldPatchConservationOracle`    | 0 | — | — | — |
|     6 | `TwinSymmetryOracle`                 | 1 (M1) | 1 | 0 | 0 |

**Aggregate**: 5 mutations applied, 3 caught at the targeted oracle, 2
missed (one true oracle false-negative on M2; one snapshot-capture
defect on M5), 1 inconclusive at the menu's first-named entry point
(M4) but caught when applied at the snapshot-input boundary.

## §4 Recommendations

### Stage 2 oracle — make conservation non-tautological (M2 finding)

The current conservation check
(`total_directed == 3 × (|tris_a| + |tris_b|)`) is a tautology over the
snapshot itself; any "lost-during-emit" defect remains undetectable.
PR10 should:

1. Snapshot the upstream Cherchi result tri count (e.g.
   `result.tris.len()` from `solve_intersections`) as a separate
   conservation anchor in `SubdividedMesh` or a sibling field.
2. Have the Stage 2 oracle assert
   `subdivided.tris_a.len() + subdivided.tris_b.len() == upstream_tri_count`
   in addition to the within-snapshot edge-count consistency check.

Without this anchor, off-by-one defects in the splitting pass are
invisible to Stage 2.

### Stage 0 oracle — fix snapshot-capture, not oracle (M5 finding)

The Stage 0 oracle's contract logic is correct (its unit tests
demonstrate this). The defect is at `yang_integration.rs:702`: the
RenderMesh snapshot is taken from the pre-injection bindings.
PR10 must:

1. Either re-derive `mesh_a` and `mesh_b` from the post-injection flat
   arrays (`verts_a`/`tris_a`/etc) before snapshotting, OR
2. Move the snapshot site below where flat-array→RenderMesh conversion
   would happen post-injection.

Until then, the corpus runner records `Stage0Coplanar = AllPass` even
for cases with severe byte-divergence in injected meshes. The oracle
is empirically unreachable for the identical-footprint code path.

### Snapshot coverage — Stage 4b on full-pipeline runs (M3 caveat)

F0001 with mutation 3 produced `Stage4bClassification → PASS /
skipped` despite `label_cells` running. Either the `record_stage_2_4b_6_snapshots`
call doesn't fire on F0001's actual code path, or the bundle is being
overwritten by a downstream call that nulls Stage 4b. Investigate
whether multiple booleans within one `LoadProject` cause the LAST
boolean to clobber Stage 4b but not Stage 6 fields (or similar).
Without this, Stage 4b oracle coverage in the histogram is
under-counted.

### Stage 2 oracle — degenerate detection works at the right level (M4)

No change needed for the contract itself. PR10 should be aware that
upstream Cherchi preprocessing scrubs degenerates from
`tessellate_solid_bounded` output before they reach the Stage 2
snapshot — the oracle only sees post-arrangement degenerates, not
pre-arrangement ones. This is correct by construction (Stage 2's
contract is on Cherchi output, not on tessellation output) but limits
the mutation space the oracle can exercise.

## §5 What this task did NOT cover

- **Stage 1 (`BijectiveFacePairOracle`)**: not exercised. The 7 corpus
  Stage 1 first-fails per the spec histogram are operand-side
  bijectivity failures; no mutation in this task targets that contract.
- **Stage 5 (`ManifoldPatchConservationOracle`)**: not exercised. PR8's
  patch-graph conservation is also a tautology-style contract over a
  derived field; M2's pattern (snapshot-internal consistency vs
  upstream anchor) likely applies here too but is unverified.
- **R0033 / F0001 / F0002 only**: every mutation was tested only on the
  three anchor cases. Other corpus cases with different topology
  (chained booleans, partial-overlap pairs, non-identical-footprint
  coplanar pairs, AABB-disjoint Subtract/Intersect variants) might
  exercise different code paths and reveal additional false-negatives.
- **Cascading interactions**: each mutation was applied in isolation. A
  multi-mutation scenario (e.g. M2 + M3 simultaneously) might reveal
  whether oracles interact correctly when multiple stages are
  corrupted. Not exercised.
- **Performance / robustness**: only the ContractViolated path was
  exercised; oracles' behavior on adversarial inputs (e.g. NaN/Inf
  positions, empty meshes within identical-footprint pairs, snapshots
  with mismatched lengths) was not separately tested beyond what the
  oracles' own unit tests cover.
- **Snapshot-capture coverage in general**: M3 and M5 both surfaced
  snapshot-capture defects (Stage 4b not landing on F0001; Stage 0
  capturing pre-injection state). A focused audit of every
  `record_snapshot` call site against every code path leading to it
  is out of scope here but appears warranted.

## §6 Verification

```bash
$ git diff
$ git status   # only output.obj + new files from teammates A/B/C; no
              # production-code modifications remain.
```

All 5 mutations were reverted. The deliverable is this report.

> Per `feedback_no_last_bug.md`: I did not exhaust the false-negative
> space. Two oracles missed mutations targeted at them; future work may
> reveal more gaps not surfaced here. The findings above are what this
> 5-mutation budget produced, no more.
