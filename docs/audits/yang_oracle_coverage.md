# Yang Pipeline Oracle Coverage Map

Maps each `YangStage` variant in `crates/kernel/src/boolean/pipeline_oracles.rs:67-84` to the oracle(s) that check its invariants. Identifies coverage gaps for future oracle PRs.

## Stage ↔ Oracle map

| Stage | Yang ref | Covered by | Status |
|---|---|---|---|
| `Stage0Coplanar` | §4.5.5 | `CoplanarMeshIdenticalOracle` | covered |
| `Stage1Bijective` | §4.1.1 | `BijectiveFacePairOracle` | covered |
| `Stage2Arrangement` | §4.2 | `MeshArrangementWellFormedOracle` | covered |
| `Stage3SsiRefinement` | §4.3 | **none** | **UNCOVERED** |
| `Stage4aMeshUpdating` | §4.4.1 | **none** | **UNCOVERED** |
| `Stage4bClassification` | §4.4.2 | `LabelConsistencyWithinPatchOracle` | covered |
| `Stage5PatchSegment` | §4.4 | `ManifoldPatchConservationOracle` | covered |
| `Stage6Assembly` | §4.5 | `TwinSymmetryOracle` | covered |

6 of 8 stages have at least one oracle. The two uncovered stages are:

## Uncovered stage 1: `Stage3SsiRefinement` (Yang §4.3)

**What this stage does**: refines mesh-arrangement intersection segments to true SSI curves using analytical quadric solvers (A15.1 corollary; the mesh boolean produces approximate intersection edges that get tightened to exact surface-surface intersection curves before B-Rep assembly).

**Minimal invariant sketch** (~50 LOC):
- For each refined intersection edge: distance from refined vertices to BOTH parent surfaces ≤ `d_epsilon` (Yang §4.3 error-bound contract)
- Curve continuity: refined polyline has no reversals (Yang §4.3.3)
- Vertex-position determinism: same input → same refined positions

**Existing closest reference**: `crates/kernel/src/boolean/ssi_refinement.rs::dispatch_ssi` — the refinement entry point. An oracle would inspect input edges and refined output edges and check the error bound + continuity.

## Uncovered stage 2: `Stage4aMeshUpdating` (Yang §4.4.1)

**What this stage does**: after SSI refinement moves intersection vertices, re-mesh adjacent triangles to restore bijectivity (Yang §4.4.1 mesh updating). Without this, refined positions break the bijective mapping between B-Rep faces and sub-triangles.

**Minimal invariant sketch** (~50 LOC):
- Post-update bijection holds: same property `BijectiveFacePairOracle` checks at Stage1, re-asserted after refinement
- Affected triangles list: every refined-edge tri is in the updated set
- Mesh validity preserved: no degenerate tris, no flipped winding

**Existing closest reference**: Currently no Stage4a code path in the kernel. Yang §4.4.1 is the "we should do this" stage that we haven't implemented. Banked under deviation D-something in `docs/yang_deviations.md`.

## Operational gap with covered stages

The 6 covered oracles correctly attribute bugs WHERE they cover. But on F0020:
- F0020 fails Stage1Bijective: 5 non-bijective pairs in operand A
- All other oracles PASS

So the oracle suite says "fix Stage 1 first." The end-to-end metrics (47 unpaired, 30 degen) are downstream effects of the Stage 1 violation. Without the oracle suite, we'd be investigating the unpaired edges (Stage 6/7) — that's exactly what produced 5 ABORTs.

## Coverage of late stages (Yang-specific assembly)

Stages 5-6 (`Stage5PatchSegment`, `Stage6Assembly`) are covered by `ManifoldPatchConservationOracle` and `TwinSymmetryOracle`. Both passing on F0020 means the patch segmentation and twin-pairing are CORRECT given their inputs — the bug they see is INHERITED from Stage 1.

This refutes the prior cycles' assumption that the cascade was in twin-pairing (path γ). Twin-pairing is well-formed; its INPUT (from Stage 1's broken bijection) is wrong.

## Reference parity (Cherchi C++) is separately needed

Even with all 8 stages covered by intrinsic invariants, **reference parity** against Cherchi C++ is a separate oracle class:
- Intrinsic invariants prove "the output satisfies its mathematical contract"
- Reference parity proves "the output matches the published reference implementation"

Both are valuable. PR-Y33's per-stage byte-diff harness is the reference-parity oracle for stages 3-6 (the mesh-arrangement core). Not currently integrated as a continuous test.

## Bank list for future oracle PRs

In rough priority order:

1. **`Stage3SsiRefinement` oracle** — closes Yang §4.3 coverage. Required before SSI solvers ship more cases.
2. **`Stage4aMeshUpdating` oracle** — partial: assertion that bijectivity is RE-established after refinement (the §4.4.1 mandate). Pairs with `BijectiveFacePairOracle` to provide pre-and-post checks.
3. **Reference parity for STAGE3/4/5/6** (Cherchi-Rust ↔ Cherchi C++) — promote PR-Y33's one-shot harness into a continuous oracle. Position-canonicalized diff with allow-list tolerances.
4. **Pre-Stage1 input validation oracle** — checks the input B-Rep itself is well-formed (closed manifold, no NaN coords, no degenerate tris). Currently implicit; making explicit prevents garbage-in cascades.

## How to interpret a violation

Each oracle's `OracleViolation` has:
- `kind`: `ContractViolated` (real bug), `StateMissing` (snapshot not captured — pipeline didn't reach this stage), `OracleStub` (oracle is a placeholder)
- `message`: human-readable violation description with locations/counts

When investigating a failing case:
1. Run `spotlight_<CASE>_oracles` (currently only `spotlight_f0020_oracles`; pattern extends per case)
2. Read the first `ContractViolated` verdict — that's the bug to fix
3. Lower-stage violations subsume higher-stage ones (fixing Stage 1 may resolve Stage 5 cascade)
4. If ALL passes but end-to-end metrics fail → bug is in an uncovered stage; consult this map

This document is the entry point for "where do I start looking when a Yang boolean fails."
