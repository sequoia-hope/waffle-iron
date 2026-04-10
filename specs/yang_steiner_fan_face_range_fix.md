# Spec: Steiner-Fan Retessellation face_range Ordering Fix

## Goal

Fix face_range ordering after `retessellate_nonmanifold_faces_with_steiner_fan()`
so that face_ranges are contiguous and sorted by start_index, eliminating
gaps detected by the `check_face_range_coverage` oracle.

## Root Cause

`retessellate_nonmanifold_faces_with_steiner_fan()` in `tessellation/repair.rs`
replaces non-manifold faces by blanking their old triangles and appending new
centroid-fan triangles to the END of the indices buffer. The face_range is updated
to point to the appended positions. Then `compact_blanked_indices()` removes blanked
entries and remaps all positions — but the face_ranges array order is not updated
to match the new buffer layout.

Concrete example: face A=[0,9), B=[9,18), C=[18,27). Face B is retessellated:
appended at [27,36), blanked at [9,18). After compaction: A=[0,9), C=[9,18),
B=[18,27). But the face_ranges array is still [A, B, C] = [0,9), [18,27), [9,18).
The oracle finds B.start(18) ≠ A.end(9) → gap.

This affects F0009 and likely contributes to self-intersection failures in the
94 failing Yang assay cases.

## Parameters

None — bug fix.

## Branch Table

| Retessellated faces? | Action |
|---|---|
| None | face_ranges already ordered, no change |
| Some | Sort face_ranges by start_index after compaction, remove empty ranges |

## Invariants

1. face_ranges sorted by start_index after any retessellation+compaction
2. face_ranges contiguous: `ranges[i+1].start_index == ranges[i].end_index`
3. Total coverage: `ranges.last().end_index == indices.len()`
4. No empty (0,0) ranges in output

## Oracles

- `check_face_range_coverage` passes (no gaps/overlaps)
- Face count matches expected (no faces lost)

## Failure Modes

- If face_ranges has empty entries that aren't removed, contiguity check fails
- If sort changes face_id ordering: acceptable — rendering code uses face_id field
  to identify faces, not array position

## Research Basis

Standard mesh data structure maintenance [#16 Mantyla]. face_ranges are an index
structure over the triangle buffer; they must be kept consistent with buffer layout.
