# Coplanar Curved-Face Fixes — Sprint 55

## Status: COMPLETE

## Overview

Sprint 54 recovered 4 of 7 ignored coplanar curved-face tests (CPE1, CPE2, CPC1, CPC3).
Three remain: CPC2, CPU1, CPB2. This spec documents root causes and fixes.

## Root Cause Analysis

### CPC2: Test fixture direction bug

**Geometry**: Outer cylinder r=5 h=20, inner cut r=2 depth=10 from top (z=20 down to z=10).

**Root cause**: `extrude_no_merge("inner", "inner_sk", 10.0)` does NOT reverse direction.
Sketch at z=20 with normal `[0,0,1]` extrudes UP to z=30, not down into the cylinder.
The `extrude_cut` API reverses direction (per `rebuild.rs:272-283`), but `extrude_no_merge`
does not — it extrudes along the sketch normal.

**Fix**: Flip sketch normal from `[0,0,1]` to `[0,0,-1]`:
```rust
m.circle_sketch("inner_sk", [0., 0., 20.], [0., 0., -1.], 0., 0., 2.)
```

This is a **test bug**, not a pipeline bug.

### CPU1: Coplanar union classification bug

**Geometry**: Outer cylinder r=5 h=20, inner cylinder r=2 h=20 (concentric, fully contained).
Union should equal the outer cylinder.

**Root cause**: Two interacting classification errors.

#### Error 1: Same-sense overlap returns Or for shell1

In `coplanar_overlay.rs:568-573`, when a face has same-sense coplanar overlap:
```rust
return Some(if is_shell0 {
    CoplanarAction::And    // shell0 face with overlap → inside other solid
} else {
    CoplanarAction::Or     // BUG: shell1 face with overlap → should also be And
});
```

Shell1's inner cap face overlaps (same-sense) with shell0's outer cap → classified Or1.
But the inner cap is INSIDE the outer solid → should be And1 (excluded from union).

**Why the asymmetry existed**: Historical — subtract uses anti-sense paths (flipped normals)
for shell1, so same-sense shell1 faces are never reached in subtract. The Or1 path was
unreachable for subtract but wrong for union.

#### Error 2: Disc face classified And0 (correct for subtract, wrong for union)

When containment injection splits the outer cap into ring + disc:
- Ring (outer, with hole): classified Or0 ✓
- Disc (inner region): classified And0 by the same-sense overlap logic

For subtract: And0 is correct (disc is in the overlap zone, should be removed).
For union: disc must be Or0 to pair with ring edges and form a complete cap face.

**Fix**: Union-only contained fixups (coplanar overlay left unchanged):
1. **Disc fixup** (and0 → or0): In union callers, move disc faces from and0 to or0.
   The disc pairs with the ring's hole edges (same Arc<Edge> from injection).
2. **Cap fixup** (or1 → and1): In union callers, move contained shell1 cap faces
   from or1 to and1. This prevents the inner caps (with mismatched edge IDs) from
   appearing in the union result.

**Why NOT modify coplanar_overlay**: The original plan proposed making same-sense
shell1 → And. This breaks subtract because ring hole edges and inner cap edges have
different Arc<Edge> IDs — the inner cap was never wired into the ring's topology.
For subtract, the old behavior (inner cap → Or1, excluded) + recovery (fill_loops)
produces valid results. The fix must be union-only.

### CPB2: 3-operation chain (downstream of CPU1)

**Geometry**: Box(10) + cylinder boss(r=3, h=5) + deep cut(r=1.5, depth=15).
First operation is union (boss onto cube).

**Root cause**: Downstream of CPU1. If union classification is fixed, the first operation
produces clean topology for the subsequent subtract. Re-test after CPU1 fix.

## Classification Truth Table

For concentric cylinders (inner contained in outer), union operation:

| Face | Shell | Expected | Pre-fix | Post-fix |
|------|-------|----------|---------|----------|
| outer lateral | 0 | Or0 | Or0 ✓ | Or0 ✓ |
| outer bottom cap | 0 | Or0 | Or0 ✓ | Or0 ✓ |
| outer top ring | 0 | Or0 | Or0 ✓ | Or0 ✓ |
| outer top disc | 0 | Or0 | And0 ✗ | Or0 ✓ (fixup) |
| inner lateral | 1 | And1 | Or1 ✗ | And1 ✓ (fix 2A) |
| inner bottom cap | 1 | And1 | Or1 ✗ | And1 ✓ (fix 2A) |
| inner top cap | 1 | And1 | Or1 ✗ | And1 ✓ (fix 2A) |

Union result = Or0 ∪ Or1 = outer lateral + outer bottom + ring + disc = complete outer cylinder ✓

## Edge-Pairing Invariant

Every edge in the result shell must have exactly 2 face references (manifold).
- Ring's inner hole edges pair with disc's outer edges (same Arc<Edge> from injection).
- With disc in Or0: ring hole edges + disc edges both in result → refs=2 ✓
- Without disc: ring hole edges have refs=1 → open boundary → invalid ✗

## Oracles

- **CPC2**: vol ≈ cyl(5,20) - cyl(2,10), V-E+F = 2 (genus-0 pocket)
- **CPU1**: vol ≈ cyl(5,20) (outer contains inner), V-E+F = 2 (simple solid)
- **CPB2**: vol ≈ 10³ + cyl(3,5) - cyl(1.5,15), V-E+F = 2 (through-hole)

## Files Modified

| File | Change |
|------|--------|
| `coplanar_overlay.rs:568-573` | Same-sense overlap → And for both shells |
| `integrate/mod.rs` | Add `contained1_ref_faces`, union disc fixup |
| `integrate/tests.rs` | Update concentric_cylinders_union assertions |
| `coplanar_curved.rs` | Fix CPC2 sketch normal, un-ignore CPU1/CPB2 |

## Risk Assessment

**LOW-MEDIUM**: Fix 2A only affects same-sense shell1 path (subtract uses anti-sense).
Fix 2B is new code in union callers only. CPC2 is a pure test fix.
