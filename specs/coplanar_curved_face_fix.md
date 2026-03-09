# Coplanar Curved-Face Boolean Fix

**Status**: In Progress
**Priority**: P1 (boolean reliability)
**Tests**: CPE1, CPE2, CPC1, CPC2, CPC3, CPB2, CPU1 (7 ignored tests in `coplanar_curved.rs`)

## Goal

Fix boolean operations between solids with coplanar curved faces (e.g., concentric
cylinder subtraction where top/bottom caps share the same plane). All 7 ignored
tests in `crates/test-harness/tests/coplanar_curved.rs` should pass.

## Failure Analysis

### CPE1 Diagnostic (concentric cylinders subtract)

Two concentric cylinders: outer r=5 h=20, inner r=2 h=20. `boolean_subtract` calls
`crate::not(&outer, &inner, tol)` internally.

**Observed**: `and0=18, or0=2` (shell0 correct), `and1=0, or1=18` (shell1 all Or).
64 open edges in result assembly.

**Expected**: Shell1 (inner cylinder after `not()`) should have ~16 lateral faces
classified as `And` (they're inside the outer cylinder and form the hole walls).

### Root Cause

**H-A: Anti-sense cap `Remove` prevents classification seeding.**

In `classify_coplanar_via_overlay` (coplanar_overlay.rs:539-553), anti-sense overlap
unconditionally returns `CoplanarAction::Remove`. For subtraction:

- Shell1 = `not(inner)` — inner cylinder with flipped normals
- Top cap of outer (+z normal) is anti-sense to top cap of not(inner) (-z normal)
- `Remove` drops shell1's cap faces entirely
- Without classified cap faces, edge-neighbor propagation cannot seed `And` for
  the 16 lateral faces of shell1
- All 16 laterals fall to winding number / ray-cast, which may fail or misclassify

**H-C: Contained fixup only matches same-sense pairs.**

In integrate/mod.rs line 1008, the contained fixup uses:
```rust
if let Some(true) = coplanar::check_coplanar(&fi, &fj, tols.tau_coplanar)
```

This only matches same-sense coplanar pairs. For subtraction, the inner cap (after
`not()`) is anti-sense to the outer cap — `check_coplanar` returns `Some(false)`,
which doesn't match `Some(true)`.

## Classification Branch Table

For `classify_coplanar_via_overlay`, the correct action depends on the operation
context and which shell the face belongs to:

| Overlap | is_shell0 | Correct Action | Current Action |
|---------|-----------|---------------|----------------|
| Same-sense | true  | And           | And            |
| Same-sense | false | Or            | Or             |
| Anti-sense | true  | And           | Remove         |
| Anti-sense | false | And           | Remove         |

**Key insight**: Anti-sense coplanar overlap means the two faces occupy the same
region with opposite normals. In a boolean result, one of these faces is needed
(it becomes the boundary). `Remove` drops BOTH, leaving a hole.

The correct behavior for anti-sense overlap is `And` — the face is inside the
other solid. The boolean assembly then selects the correct faces: for union,
And faces are discarded; for subtraction (and), And faces from shell1 are kept
with inverted normals.

## Fixes

### Fix A: Anti-sense → And (not Remove)

Change coplanar_overlay.rs line 551 from:
```rust
return Some(CoplanarAction::Remove);
```
to:
```rust
return Some(CoplanarAction::And);
```

Anti-sense faces that overlap ARE inside the other solid. They should be classified
as `And` and let the boolean assembly (and0+and1 for intersection, or0+or1 for
union) handle them correctly.

### Fix B: Contained fixup anti-sense support

Change integrate/mod.rs line 1008 from:
```rust
if let Some(true) = coplanar::check_coplanar(&fi, &fj, tols.tau_coplanar) {
```
to:
```rust
if coplanar::check_coplanar(&fi, &fj, tols.tau_coplanar).is_some() {
```

This allows the contained fixup to also reclassify anti-sense coplanar inner
faces from `or1` to `and1`.

## Invariants

1. **Closed shell**: Result must have 0 open edges
2. **Euler characteristic**: Tube (genus-1) → V-E+F = 0; pocket (genus-0) → V-E+F = 2
3. **Volume conservation**: |vol_result - vol_expected| < 10% * vol_expected
4. **Finite mesh**: All vertices and normals must be finite (no NaN/Inf)
5. **No regression**: All existing passing tests must continue to pass

## Oracles

- `approx_cylinder_volume(r, h)` = r^2 * 16 * sin(2pi/16) / 2 * h (16-segment polygon)
- Euler characteristic: V - E + F = 2 - 2g (g = genus)
- Open edge count via `Shell::open_edges()` (must be 0 for valid solid)

## Risk Assessment

LOW risk. Both fixes are in classification logic, not topology/geometry:
- Fix A: Changes Remove → And for anti-sense faces (less dropping, not more)
- Fix B: Widens Some(true) to is_some() (allows existing fixup to also correct anti-sense)
- 400+ regression tests catch unintended changes
