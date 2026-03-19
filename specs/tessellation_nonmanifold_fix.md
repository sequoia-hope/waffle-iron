# Tessellation Non-Manifold Edge Fix

Fix for unpaired edges in tessellated boolean results.

**Status**: In progress
**References**: [#16] Mantyla (half-edge B-Rep), [#33] Stroud (topology repair)
**Classification**: Bug fix (modeling-related)

---

## Goal

Reduce unpaired edges in tessellated multi-operation boolean results. Currently,
cases with 2-5 unpaired edges out of thousands fail the watertight oracle.
Two specific root causes have been identified:

1. The anti-parallel direction check in B-Rep stitch twin pairing is too strict
   for short edges near boolean intersection curves.
2. The `close_near_boundary_chains` tessellation repair skips boundary components
   larger than 8 vertices.

---

## Parameters

### Fix 1: Relax anti-parallel threshold in stitch twin pairing

| Parameter | Current | Proposed |
|-----------|---------|----------|
| `cos_angle` threshold | -0.5 (120° max deviation) | -0.3 (107° max deviation) |
| Edge length guard | none | For edges shorter than `tol`, skip the direction check entirely |

**Location**: `crates/kernel/src/boolean/stitch.rs`, Step 3d (~line 455)

### Fix 2: Increase boundary chain size limit

| Parameter | Current | Proposed |
|-----------|---------|----------|
| Max component size | 8 | 24 |

**Location**: `crates/kernel/src/tessellation/mod.rs`, `close_near_boundary_chains` (~line 4036)

---

## Branch Table

| Fix | Branch | Expected Behavior |
|-----|--------|-------------------|
| Fix 1a | Short edge (len < tol) | Skip anti-parallel check, pair by proximity only |
| Fix 1b | Normal edge, cos > -0.3 | Skip (not anti-parallel) |
| Fix 1c | Normal edge, cos ≤ -0.3 | Pair as twin (existing behavior) |
| Fix 2a | Component size 3-24 | Attempt boundary chain closure |
| Fix 2b | Component size > 24 | Skip (existing behavior for >8) |

---

## Invariants

1. **No regression**: All 537 existing kernel tests must pass.
2. **No regression**: All 25 F-series assay cases must pass.
3. **Improvement**: R-series watertight failures should decrease.
4. **Topology preservation**: Paired edges must form valid twin pairs
   (opposite direction, shared vertices).
5. **No false positives**: The relaxed threshold must not pair edges
   that are genuinely not twins (e.g., edges from different faces that
   happen to be nearby but non-adjacent).

---

## Oracles

1. **Unpaired edge count**: For each test case, count boundary + non-manifold
   edges in the tessellated mesh. Assert fewer than before the fix.
2. **Watertight check**: `check_watertight()` — quantized edge pairing.
3. **Volume positivity**: Signed volume must remain positive.
4. **Euler formula**: V - E + F = 2 for the B-Rep solid.

---

## Failure Modes

1. **Over-relaxed threshold pairs wrong edges**: If `cos_angle > -0.3` is still
   too strict or `-0.3` pairs non-twin edges, the fix should be tuned. The
   endpoint proximity check (`fwd_dist < tol_sq`) provides a second guard.
2. **Large boundary chains produce wrong triangulation**: Components > 8 vertices
   may have complex topology. The existing triangle-hole and quad-hole logic
   handles specific patterns; larger components may need generic ear-clipping.

---

## Research Basis

- [#16] Mantyla: Half-edge B-Rep twin pairing semantics
- [#33] Stroud: Topology repair after boolean operations
- S-H clipping tolerance analysis: Independent polygon clipping produces
  geometrically identical but numerically distinct intersection points.
  The twin pairing must tolerate this discrepancy.

---

## Analytical vs. Approximate Method Justification

Not applicable — this fix addresses tessellation mesh repair, not SSI.

---

*Last updated: 2026-03-19*
