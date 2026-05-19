# Y57 Phase 1 Canary — Tier B Path γ EMPIRICALLY REFUTED

## Status: PATH γ REFUTED — Phase 2 SKIPPED per plan decision gate

## What shipped

Env-gated counter `Y57_SAME_DIR_PAIR=1` in `crates/kernel/src/boolean/topology_extract.rs::flood_fill_patches` twin-pairing loop. At each successful pairing in the `[the_one]` branch, checks whether `he_fwd` and `he_rev` walk OPPOSITE canonical directions (the Yang §4.4.2 invariant) or SAME direction (the Defect-1 hypothesis from the Tier A memo).

Default-off byte-identical: F0020 spotlight metrics unchanged (47 unpaired, 30 degen, 175 tris). Kernel 1249/34/42 baseline preserved.

## Empirical result (F0020 spotlight)

```
[y57-summary] paired_count=48  Y57_OPPOSITE_PAIRS=48  Y57_SAME_DIR_PAIRS=0  (boolean 1)
[y57-summary] paired_count=111 Y57_OPPOSITE_PAIRS=111 Y57_SAME_DIR_PAIRS=0  (boolean 2)
```

159 total pairings across F0020's 2 booleans. **ALL 159 are opposite-direction.** Zero same-direction pairings.

## What this refutes

The Tier A memo (`docs/audits/y55_cascade_attribution.md` §Defect-1) claimed:

> Likely cause: in boolean 1's output B-Rep assembly (`flood_fill_patches::twin_pairing`), the algorithm pairs HEs by canonical (origin, dest) vertex positions without enforcing that paired HEs walk OPPOSITE directions. When two faces both walk (v0, v1) forward in their outer loops, twin-pairing finds them and links them — but the geometric reality (they should walk opposite directions) is silently violated.

**This is empirically false.** Twin-pairing's `[the_one]` branch (`topology_extract.rs:1417-1437`) is opposite-by-construction:
- `he_fwd` is iterated from `fwd_hes = directed_he[(lo, hi)]` (walks lo→hi)
- `he_rev` is filtered from `rev_hes = directed_he[(hi, lo)]` (walks hi→lo)

The Yang §4.4.2 directional symmetry mandate is already enforced by the iteration structure. Y57's measurement confirms: 0 violations in 159 pairings.

## What this implies for the 37 collisions

Path γ (twin-pairing direction-check) would address ZERO of F0020's 37 boolean-2 input directed-edge collisions. The collisions come from a different mechanism.

The plan-phase deeper analysis (per `snappy-humming-hejlsberg.md` Context section) identified that 30 of 37 colliding face-pairs do NOT share arena edges at all. The colliding directed edges must be CDT-internal diagonals at coincident 3D positions after `dedup_mesh_vertices` (`yang_integration.rs:1530-1562` nanometer-quantized dedup).

Most-likely actual mechanism: **same-mesh coplanar B-Rep face over-fragmentation in boolean 1's output**. Two or more B-Rep faces of mesh A that should geometrically be one face (or share more boundary than the arena records) produce independent CDTs whose internal diagonals coincide in position. After dedup, those become the 37 collisions.

This is closest to Phase 1's mechanism (c) from the original Tier A plan ("Multiple B-Rep faces on the same side of a shared edge — over-fragmented patches that should have merged") but at the position/geometry level, not the arena-twin level.

## Decision gate route

Per plan:
> `Y57_SAME_DIR_PAIRS == 0`: Path γ is empirically refuted. ABORT path γ implementation. Document the refutation in a new memo. Pivot to Tier B path δ: Y56 canary to identify the over-fragmentation source. Path δ canary is its own subsequent plan.

**Phase 2 (path γ implementation) is SKIPPED.**

**Path δ is BANKED for the next plan cycle.** Scope (preliminary): Y56 canary in `tessellate_planar_face_bounded` and/or `tessellate_solid_bounded` that, for each boolean-2 input collision, dumps:
- The 3D positions of the colliding directed edge endpoints
- Whether each endpoint coincides with a boundary vertex of two distinct B-Rep faces
- The source-face provenance of both colliding tris

Then a fix shape (Tier B path δ): same-mesh coplanar face merge in boolean output B-Rep assembly. Likely anchor in `flood_fill_patches::Step 5a` (per `topology_extract.rs:633-718`) — Step 5a splits manifold components by source face for B-Rep provenance, but doesn't enforce that adjacent same-source-face patches stay merged when geometrically coplanar.

## Path δ is NOT in scope of this PR

Per the plan: "Path δ canary is its own subsequent plan." The Y56 canary requires a separate plan cycle with user sign-off. This memo documents the path γ refutation cleanly so the next plan starts from confirmed empirical ground.

## Discipline outcome

The 15-25 LOC Y57 canary saved 40-80 LOC of misdirected path γ implementation. This is the same canary-first pattern that succeeded for the chain-builder fix (commit `26d9094`) and was prefigured by `feedback_anchor_before_fix.md`.

The Tier A memo's path γ recommendation was based on a hasty interpretation of the Y55 histogram. Closer code-reading (which I performed during the path γ plan phase) revealed the iteration structure already enforces the invariant. Y57 confirms the code-reading.

**Lesson banked:** When Tier A surfaces a candidate fix shape via inference from histogram data, the next plan cycle's Phase 1 MUST include a direct empirical test of the fix shape's premise — not just acceptance of the Tier A's framing. The Y57 canary cost 15-25 LOC; misdirected Phase 2 would have cost 40-80 LOC plus the WASM rebuild cycle plus the ABORT documentation cost.

## DoD checklist (Bug Fix variant — Phase 1 only)

- [x] Default-off byte parity verified (F0020 spotlight unchanged)
- [x] Kernel `cargo test -p kernel --lib` baseline preserved: 1249/34/42
- [x] Phase 1 canary produces `Y57_SAME_DIR_PAIRS` count (`0` on F0020 across 159 total pairings)
- [x] Decision-gate route documented (this memo)
- [N/A] Phase 2 implementation — SKIPPED per gate

## Verification

```bash
cd /home/claude/workspace

# Default-off byte parity
YANG_BOOLEAN=1 cargo test -p test-harness --test assay_randomized -- spotlight_f0020 --ignored --nocapture 2>&1 | grep "Detail:"
# expect: 47 unpaired, 30 degen, 175 tris (unchanged from main)

# Y57 measurement (the load-bearing result)
Y57_SAME_DIR_PAIR=1 YANG_BOOLEAN=1 \
  cargo test -p test-harness --test assay_randomized -- spotlight_f0020 --ignored --nocapture 2>&1 \
  | grep "y57-summary"
# expect:
#   [y57-summary] paired_count=48 Y57_OPPOSITE_PAIRS=48 Y57_SAME_DIR_PAIRS=0
#   [y57-summary] paired_count=111 Y57_OPPOSITE_PAIRS=111 Y57_SAME_DIR_PAIRS=0

# Kernel regression
cargo test -p kernel --lib 2>&1 | tail -3 # expect 1249/34/42
```
