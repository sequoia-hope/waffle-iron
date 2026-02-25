# Spec: Boolean Determinism

**Status**: Implemented

## Goal

Boolean operations (And, Or, Difference) must produce identical results for identical inputs, regardless of memory allocation patterns or hash map iteration order.

## Root Cause

`construct_polylines()` builds a graph using `FxHashMap<PointIndex, Node>`. Two sources of non-determinism:

1. **`get_one()`** picks `self.iter().next().unwrap()` — arbitrary FxHashMap entry
2. **`pop_one_adjacency()`** picks `self.adjacency.iter().next().unwrap()` — arbitrary HashSet entry
3. **Closed polylines** are not direction-canonicalized (only open polylines are fixed)

The polyline direction feeds into `from_is_curve() → leader().der(t)`, which determines And vs Or face classification. Reversing a polyline negates `der(t)`, flipping the classification.

## Parameters

- Input solids (vertices, faces, topology)
- Tolerance (adaptive or explicit)
- Boolean operation type (And, Or, Difference)

## Branch Table

| Polyline Type | Before Fix | After Fix |
|---------------|-----------|-----------|
| Open | Canonicalized (lex-smallest endpoint first) | No change |
| Closed (>=3 unique vertices) | NOT canonicalized — non-deterministic | Canonical rotation + direction |
| Closed (<3 unique vertices) | Degenerate, filtered upstream | No change |

## Fix: Deterministic Graph Traversal

1. Add `Ord` to `PointIndex` (lexicographic on `[i64; 3]`)
2. `get_one()` returns lexicographically smallest `PointIndex`
3. `pop_one_adjacency()` returns lexicographically smallest adjacent vertex

## Fix: Closed Polyline Direction Canonicalization

For closed polylines (first vertex == last vertex, N >= 3 unique vertices):
1. Find vertex with lexicographically smallest `(x, y, z)` coordinates
2. Rotate polyline so that vertex is first
3. Compare the two neighbors of the minimum vertex; pick direction where the second vertex is lex-smaller
4. This selects a unique representative from 2N equivalent forms (N rotations x 2 directions)

## Fix: Deterministic `split_wire_recursive`

Sort repeated vertex candidates by first occurrence index instead of FxHashMap iteration order.

## Invariants

- Same inputs → same polyline vertex sequences → same And/Or classification → same output topology
- Face count, volume, and topology digest are identical across N runs of the same boolean

## Oracles

- Run boolean N times (N >= 10), compare face counts — must be identical
- Compare vertex positions across runs — must match within tolerance

## Failure Modes

- Polyline with < 3 unique vertices: degenerate, cannot canonicalize direction (should not occur in practice)
- Floating-point tie in lexicographic comparison: use `partial_cmp` with `Equal` fallback (identical coordinates are fine)
