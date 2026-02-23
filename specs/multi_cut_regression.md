# Spec: Multi-Cut Disappearing Body Regression

**Status**: Phase 2 — Regression tests written, awaiting implementation fix.

## Goal

Detect the failure mode where a cut extrude applied to a merged (auto-union) body
causes earlier body geometry to vanish from the final solid.

## Background

After fixing HP-1/HP-2 (consumption tracking in `rebuild.rs`), the "Example Multi Cut"
scenario shows correct body count (1), but the **first extrude's geometry disappears
completely** from the result. The cut only affects the second extrude's body.

**Scenario**: Two boss extrudes auto-union into a merged body. A cut is applied. The cut
should subtract from the MERGED body (containing both bosses). Instead, the first boss
vanishes from the final solid.

## Geometry (All-Box Variant — Q1)

All boxes use axis-aligned rectangles for analytical predictability.

- **e1**: 10x10x10 box at x=[0,10] (sketch at origin [0,0,0], normal [1,0,0], rect (-5,-5) to (5,5))
  - Volume: 1000
- **e2**: 10x10x10 box at x=[10,20] (sketch at origin [10,0,0], same rect), auto-union with e1
  - Merged volume: 2000
- **e3**: 4x4 rect cut from sketch at [20,0,0], normal [1,0,0], depth=20
  - Cut tool reversal: `should_reverse_for_cut=true`, `cut_eps=0.1`
  - Tool origin: [20.1, 0, 0], direction [-1, 0, 0], depth 20.2
  - Tool spans x=[-0.1, 20.1] — covers entire merged body
  - Removes 4x4x20 = 320 from the merged body
  - Expected final volume: 1680

## Branch Table

| Step | Bodies | Volume | BB min x | BB max x |
|------|--------|--------|----------|----------|
| After e1 | 1 | 1000 | 0 | 10 |
| After e2 (union) | 1 | 2000 | 0 | 20 |
| After e3 (cut) | 1 | 1680 | 0 | 20 |

## Invariants

1. `bb_min.x < 0.5` after cut — **THE BUG DETECTOR** (first body present)
2. Volume within 10% of analytical prediction
3. Body count = 1 at every step
4. Consumed features: {e1} after union, {e1, e2} after cut

## Failure Modes

| ID | Failure | Detection |
|----|---------|-----------|
| F1 | First body vanishes | `bb_min.x > 1.0` after cut |
| F2 | Bodies fragment | body count > 1 |
| F3 | Union fails | body count > 1 after e2 |
| F4 | Cut fails entirely | no solid output |

## Test Matrix

| Test | Geometry | Primary Oracle |
|------|----------|----------------|
| q1_multi_cut_preserves_first_body | Two abutting 10x10x10 boxes + 4x4 rect cut | bb_min.x < 0.5, volume ≈ 1680 |
| q2_multi_cut_box_cylinder_variant | 10x10x10 box + r=8 cylinder + r=6 circle cut | bb_min.x < 0.5, volume > 500 |
| q3_multi_cut_three_bodies_then_cut | Three abutting 10x10x10 boxes + 4x4 rect cut | bb_min.x < 0.5, volume ≈ 2520 |

## Files

| File | Role |
|------|------|
| `crates/test-harness/tests/saved_test_cases.rs` | Q1-Q3 regression tests |
| `specs/multi_cut_regression.md` | This spec |
