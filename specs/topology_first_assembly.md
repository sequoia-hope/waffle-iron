# Topology-First Shell Assembly (Phase D)

**Status:** Implemented
**Sprint:** 48
**FIP Classification:** Refactor (DoD 3) — no behavior change for correctly-assembled shells; improved determinism for near-degenerate cases.

## Problem

The boolean pipeline's shell assembly stage (`assemble_boolean_shell_v2`) used a 4-level progressive tolerance weld to close shells after face classification:

- Level 0 (0.2x tau): Conservative weld — often leaves open edges
- Level 1 (0.4x tau): Slightly wider — still misses many edges
- Level 2 (5.0x tau): Aggressive — risks over-welding distinct edges
- Level 3 (`force_merge_open_edges`): Geometric endpoint matching — last resort

Each level is a heuristic. When the correct pairing falls between tolerance levels, assembly fails or merges wrong edges. The perturbation cascade then fires 50+ attempts.

## Solution

Replace tolerance-based welding with topology-first radial-sort edge pairing via `assemble_shell_radial()` in `radial_assembly.rs`.

### Algorithm

1. **Build global half-edge table:** For each face, enumerate edges recording face/wire/edge indices and vertex positions.

2. **Group by quantized vertex pair:** Hash edges by `(min_pos, max_pos)` using a `tau/100` grid. Groups all edges sharing the same geometric position regardless of direction.

3. **Validate manifold:**
   - Groups with exactly 2 half-edges -> direct pairing (fast path, most common)
   - Groups with >2 half-edges -> 3D radial sort needed
   - Groups with 1 half-edge -> open edge (assembly failure)

4. **3D radial sort** (for multi-edge groups): Project face normals onto plane perpendicular to shared edge direction. Sort by angle using `atan2(cross, dot)` relative to a reference half-edge. Pair adjacent half-edges (Sugihara-Iri linking).

5. **Edge pairing:** Replace duplicate Edge objects with shared canonical edges. Determines direction by geometric position comparison.

6. **Shell validation:** Build shell, attempt `Solid::try_new`. Fall back to `ShellCondition::Closed` or `Regular` with chi=2 validation.

### Integration

Wired into `finalize_boolean_shell` and `finalize_boolean_shell_with_recovery_v2` as the primary assembly path, with v2 progressive weld as fallback on failure.

## Files

| File | Change |
|------|--------|
| `vendor/truck/truck-shapeops/src/transversal/integrate/radial_assembly.rs` | **NEW** — Core module |
| `vendor/truck/truck-shapeops/src/transversal/integrate/mod.rs` | `mod radial_assembly`, wired into `finalize_boolean_shell` |
| `vendor/truck/truck-shapeops/src/transversal/mod.rs` | Export `radial_assembly_stats` |

## Counters

- `RADIAL_SUCCESS` / `RADIAL_FALLBACK`: Atomic counters tracking how often radial assembly succeeds vs falls back to v2
- Query via `radial_assembly_stats()` -> `(success, fallback)`

## Tests

10 unit tests in `radial_assembly::tests`:
- `test_build_half_edge_table` — correct face/edge indexing
- `test_build_half_edge_table_multi_face` — multi-face table construction
- `test_group_by_vertex_pair` — correct grouping with tolerance
- `test_quantized_pos_symmetry` — EdgeGroupKey is order-independent
- `test_is_same_direction` — direction detection
- `test_radial_sort_3d_two_faces` — 2 faces around shared edge
- `test_assemble_cube` — 6 independent faces assemble
- `test_assemble_shared_edge_cube` — shared vertices, independent edges
- `test_empty_faces` — empty input error
- `test_assemble_boolean_result` — real truck builder cube reassembly

## Verification

- truck-shapeops: 271 pass (up from 261), 1 pre-existing fillet fail
- boolean_properties: 27 pass, 1 ignored
- boolean_recovery: 14 pass, 1 ignored
- boolean_edge_cases: 8 pass
- boolean_workflows: 38 pass
- multi_op_chains: 5 pass, 1 ignored
- extrude_chains: 46 pass (1 intermittent cascade non-determinism, passes in isolation)
- revolve_boolean: 2 pass, 6 ignored
- Clippy: clean
- Format: clean
