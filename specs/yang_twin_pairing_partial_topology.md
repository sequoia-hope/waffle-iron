# Spec: Yang Twin-Pairing Partial Topology Acceptance

## Goal

Replace the all-or-nothing unpaired half-edge discard in `build_result_brep_from_mesh()`
with partial face removal, preserving valid topology when only a few faces have
pairing failures.

## Root Cause

`build_result_brep_from_mesh()` in `topology_extract.rs:1064-1070` returns an empty
`ResultTopology` when ANY half-edge is unpaired, even if only 1-2 of hundreds are
affected. This discards all valid faces and produces empty boolean results.

Root causes of unpaired HEs include:
- Asymmetric T-junction subdivision at perpendicular face junctions
- Greedy matching heuristic failure at multi-entry directed edges
- Open-chain reconciliation (Step 5d) skipping chains with `on_line_verts.len() < 2`

This affects **56/190** assay cases with `YANG_BOOLEAN=1` (some may overlap with
the face_geometry bug — reassess after that fix).

## Parameters

None — this is a bug fix with no new user-facing parameters.

## Branch Table

| Unpaired HEs | Affected faces vs total | After face removal | Action |
|---|---|---|---|
| 0 | N/A | N/A | Accept full topology (unchanged) |
| >0 | < total faces | 0 unpaired remaining | Accept partial topology, log removed faces |
| >0 | < total faces | Still unpaired | Return empty topology (unchanged) |
| >0 | = total faces | N/A | Return empty topology (unchanged) |

## Invariants

1. After partial face removal, ALL remaining HEs satisfy twin symmetry:
   `arena.half_edges[arena.half_edges[i].twin.0].twin.0 == i`
2. `face_provenance` only contains entries for surviving faces
3. `edge_is_intersection` only contains entries for surviving edges
4. Diagnostic log reports how many faces were removed and how many survive

## Oracles

- **Non-empty result**: Cases that previously produced 0 faces now produce >0 faces
- **Twin symmetry**: All remaining HEs pass the twin-symmetry check
- **Euler check**: `validate_yang_result_topology` passes on the partial result
  (per-component Euler characteristic)

## Failure Modes

- If ALL faces have unpaired HEs, returns empty topology (unchanged behavior)
- If face removal creates new unpaired HEs (transitive effect), re-validate and
  fall back to empty if needed
- Partial topology may have fewer faces than expected — this is acceptable if the
  remaining faces form a valid manifold

## Research Basis

- Half-edge twin pairing: standard B-Rep construction [#16 Mantyla, Ch. 3]
- Partial mesh repair by face removal: common in mesh boolean pipelines
  [#24 Yang et al. 2025, Section 4.3]
- P9 compliance: this is NOT masking the bug — unpaired HEs genuinely represent
  faces where mesh subdivision didn't produce matching boundary segments.
  Removing those specific faces and keeping valid ones is correct behavior.
