# Mesh Euler Characteristic Oracle

**Status:** Implementing
**Author:** Claude Code
**Date:** 2026-03-25

## Goal

Detect interior/residual faces from failed boolean operations by computing the
Euler characteristic χ = V - E + F directly from the triangle mesh and comparing
against an expected value from test case metadata.

## Background

After a boolean subtract that creates a through-hole, the resulting solid has
genus 1 and Euler characteristic χ = 0 (not 2). Interior faces left behind by
an incomplete boolean shift χ away from the expected value.

The Euler-Poincaré formula for a closed orientable 2-manifold:
  χ = V - E + F = 2 - 2g
where g is the genus (number of through-holes).

## Parameters

- `expected_chi: i64` — from `.meta.json` `oracles.euler_target`
- Computed from operation list: each cut that fully penetrates the body adds genus 1

## Algorithm

1. Quantize vertex positions using scale-adaptive grid (reuse approach from
   `check_watertight_mesh`: `grid_size = max(max_abs * 1e-5, 1e-10)`)
2. Collect unique quantized positions → V (vertex count)
3. For each triangle, emit 3 sorted edge pairs into HashSet → E (edge count)
4. F = `mesh.indices.len() / 3` (face/triangle count)
5. χ = V - E + F
6. Pass if χ == expected_chi

## Genus Heuristic for Through-Hole Detection

A cut operation creates a through-hole (genus += 1) when `cut_depth >= boss_depth`
on the same axis. Since we cannot determine profile overlap without geometry,
use depth comparison as a conservative heuristic. Cases where a blind hole is
misclassified as a through-hole get a more lenient target (χ=0 instead of 2),
so the oracle won't false-positive.

## References

- Ref #33 Stroud — B-rep topology, Euler-Poincaré formula
- Ref #7 Jacobson — generalized winding numbers
