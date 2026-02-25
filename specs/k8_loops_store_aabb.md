# Spec: K8 Fix — Degenerate Wire Filtering + AABB Culling in loops_store

**Status**: RESOLVED (Sprint 36). K8 passes with 3 bosses + 3 cuts.
Sprint 35 fixed face division (biangle, wire split, AABB culling, merge+splice).
Sprint 36 fixed classification (8-ray robust ray-cast, edge-neighbor propagation)
and perturbation cascade (scale-expand-first for complex shells >30 faces).

## Goal

Fix the k8 test case (`k8_three_bosses_then_three_cuts`) where the 3rd cut operation on a 31-face shell fails. The fix addresses three layered root causes: degenerate biangle wires from IC vertex insertion, non-simple wire handling in face division, and O(f²) face-pair iteration without spatial filtering.

## Background

### Geometry
- 10×10×10 base cube
- 3 bosses: 3×3 rects at (0.5,0.5), (4.0,4.0), (7.0,0.5), each extruded 3.0 upward from z=10
- 3 cuts: 3×3 rects at (0.5,7.0), (4.0,0.5), (7.0,7.0), each cut 4.0 deep from z=10
- Cuts c0 and c1 succeed (Sprint 34 overlay fix). Cut c2 fails on the resulting 31-face shell.

### Diagnostic Evidence (from running k8 with `--ignored`)

**State entering c2**: 31-face shell, Euler chi=4 (expected 2 — already non-manifold from c1's perturbation recovery).

**Failure 1 — Degenerate biangle wires in face division**:
```
[boolean] Face::try_new failed in divide_one_face: NotSimpleWire
  wire[1]: 2 edges, closed=true, simple=true
    edge[0]: fid=...280 bid=...030 fp=(10,-10,10) bp=(10,-7,10) ori=false
    edge[1]: fid=...280 bid=...030 fp=(10,-10,10) bp=(10,-7,10) ori=true
```
Wires 1, 2, 3 are "biangle" wires — the same edge traversed forward then backward. These form degenerate zero-area faces. They arise when IC vertex insertion splits a face boundary at a point that already exists as a vertex, creating duplicate edge references.

**Failure 2 — Self-intersecting IC wire**:
```
shell1 face 4: w0[10e,st=And]: (7,-10,10)->(7,-10,5.9)->(7,-10,10)->(7,-7,10)...
```
A 10-edge wire visits vertex (7,-10,10) three times — the wire is non-simple (self-intersecting). This corrupt IC topology propagates to `divide_one_face` which produces 0 fragments.

**Failure 3 — 14 unknown faces**:
```
[classify] shell0: and=0, or=18, unknown=14
```
After face division fails for some faces, 14 of 31 shell0 faces can't be classified (And/Or). The fallback upgrades some to `and=2, or=30`, but this is insufficient — the cut should subtract significant material. Result: 28 open edges, shell closure impossible.

**Cascade exhaustion**: Direct attempt takes 85.4s, 2 composite attempts take ~24.7s each, timeout at 120s after 3 attempts.

### Root Cause Chain

1. **Non-manifold input shell (chi=4)**: The c1 result (after perturbation recovery + `new_unchecked`) has non-manifold topology. Face boundaries share edges in ways that create "pinch points" at vertices like (7,-10,10).

2. **IC vertex insertion creates biangle wires**: When `add_polygon_vertex` / `add_geom_vertex` inserts an IC endpoint at an existing face boundary vertex, the edge-splitting logic can produce two half-edges that form a degenerate 2-edge closed wire (same edge forward+backward = biangle). `Face::try_new` correctly rejects these as `NotSimpleWire`.

3. **Self-intersecting loop store wires**: Face 4 of the tool shell gets a 10-edge wire that visits (7,-10,10) three times. This happens because multiple IC curves from different shell0 faces converge at the same tool-face vertex, and the wire assembly concatenates them into a single non-simple loop.

4. **0-fragment face division cascades to Unknown classification**: When `divide_one_face` fails (NotSimpleWire → 0 fragments), the face becomes Unknown. With 14 unknowns out of 31 faces, classification has insufficient information to construct a valid shell.

5. **28 open edges → cascade exhaustion**: The broken shell has 28 open edges. None of the 6 finalize_boolean_shell repair strategies can close it because the underlying face topology is wrong, not just edge welding.

### Performance Context
The O(f²) loop is the #1 bottleneck in the boolean pipeline. AABB culling reduces unnecessary IC computations and — critically — reduces the number of IC vertex insertions that can create degenerate wires. Fewer face-pair interactions = fewer opportunities for topology corruption.

## Parameters

| Parameter | Value | Rationale |
|-----------|-------|-----------|
| AABB inflation | `tol * 2.0` | Ensure faces within tolerance of each other aren't missed |
| Face AABB source | Polygon mesh positions | Already available as `poly_shell` argument |
| R-tree library | `rstar` | Already in Cargo.toml, used by `bvh.rs` |

## Branch Table

| # | Condition | Behavior | Test |
|---|-----------|----------|------|
| B1 | Face AABBs overlap (inflated by 2×tol) | Compute intersection curves (existing path) | k8 test, existing IC tests |
| B2 | Face AABBs don't overlap | Skip pair entirely | New unit test: disjoint faces skipped |
| B3 | Both faces coplanar and AABBs overlap | Existing coplanar path (check_coplanar_faces) | Existing coplanar tests |
| B4 | Biangle wire detected (2 edges, same edge fwd+bwd) | Skip/remove degenerate wire | New unit test: biangle wire filtered |
| B5 | Non-simple wire from IC vertex insertion | Split at repeated vertices, form simple sub-wires | New unit test: self-intersecting wire split |
| B6 | Face division produces 0 fragments | Preserve original face (don't discard) | New unit test: 0-fragment face preserved |
| B7 | 31+ face shell with 6-face tool | AABB culling + robust wire handling | k8 integration test |

## Invariants

1. **Conservative AABB culling**: Every face pair that *would* produce a valid IC must pass the AABB test. False positives acceptable; false negatives NOT.

2. **No biangle wires**: After IC vertex insertion, no wire in the loop store should consist of 2 edges that are the same edge with opposite orientations.

3. **Simple wires only**: Every wire passed to `Face::try_new` must be simple (no repeated vertices except closure). Non-simple wires must be split into simple sub-wires first.

4. **No 0-fragment face loss**: If `divide_one_face` produces 0 fragments, the original undivided face must be preserved (classified via ray-cast), not silently dropped.

5. **Determinism**: AABB construction, wire filtering, and wire splitting are all deterministic.

6. **No regression**: All existing `extrude_chains`, `boolean_shell_closure`, `boolean_workflows`, and `boolean_determinism` tests must continue passing.

7. **Volume oracle**: k8: each boss increases volume, each cut decreases volume.

## Oracles

| Oracle | Expected | Tolerance |
|--------|----------|-----------|
| Volume after base cube | 1000 | exact mesh volume |
| Volume after each boss | strictly > previous | - |
| Volume after each cut | strictly < previous | - |
| Shell condition after each feature | Closed | exact |
| Euler characteristic | V-E+F = 2 | exact |
| Unknown faces after classification | 0 | exact |

## Failure Modes

| ID | Failure | Detection | Mitigation |
|----|---------|-----------|------------|
| F1 | AABB too tight — valid IC pair rejected | Existing tests regress | Inflate AABB by `tol * 2.0` (conservative) |
| F2 | Biangle wire not detected | `Face::try_new` → `NotSimpleWire` | Check for same-edge-both-orientations pattern |
| F3 | Wire splitting creates degenerate sub-wires | Sub-wire has <3 edges | Skip sub-wires with <3 edges |
| F4 | 0-fragment face dropped → Unknown cascade | `unknown` count > 0 after classification | Preserve original face, classify via ray-cast |
| F5 | Non-manifold input (chi≠2) amplifies errors | Euler check diagnostic | Accept chi≠2 input but track it; ensure output chi=2 |
| F6 | Perturbation still needed after fixes | k8 c2 requires >1 attempt | Acceptable if it converges; track attempt count |

## Implementation Strategy

### Sprint A: Biangle Wire Detection + Removal (Root Cause #1)

**Files**: `vendor/truck/truck-shapeops/src/transversal/loops_store/mod.rs`, `divide_face/mod.rs`

The most impactful fix. Biangle wires (2 edges, same edge fwd+bwd) cause `NotSimpleWire` → 0 fragments → 14 unknown faces.

1. Add `is_biangle_wire(wire) -> bool` helper: checks if wire has exactly 2 edges where edge[0] and edge[1] reference the same underlying Edge (same vertex pair, opposite orientation)
2. In `divide_one_face`, before calling `Face::try_new`, filter out biangle wires from the wire list
3. In `create_loops_stores`, after IC vertex insertion, detect and remove biangle boundary wires from the loops store before they reach face division
4. Unit test: construct a face with an injected biangle wire, verify it's filtered

### Sprint B: Non-Simple Wire Splitting (Root Cause #2)

**Files**: `vendor/truck/truck-shapeops/src/transversal/loops_store/mod.rs`, `divide_face/mod.rs`

Handle the 10-edge self-intersecting wire that visits (7,-10,10) three times.

1. Add `split_non_simple_wire(wire) -> Vec<Wire>`: detect repeated vertices, split into simple sub-wires at repeated vertices (reuse existing `split_wire_recursive` from Sprint 22)
2. Wire the splitting into face division: before `Face::try_new`, check each wire for simplicity; split non-simple wires into simple sub-wires
3. Filter out degenerate sub-wires (< 3 edges, zero area, collapsed to a point)
4. Unit test: construct a self-intersecting wire, verify it splits into valid simple wires

### Sprint C: Face AABB Culling (Performance + Robustness)

**File**: `vendor/truck/truck-shapeops/src/transversal/loops_store/mod.rs`

Reduce face-pair count to minimize IC vertex insertion opportunities that create degenerate wires.

1. Add `compute_face_aabb(poly_face) -> ([f64;3], [f64;3])` helper
2. Build face AABBs for both shells at top of `create_loops_stores`
3. Add overlap check at line 814: skip pairs where inflated AABBs don't overlap
4. Coplanar pre-scan (lines 700-714) is unaffected (runs before AABB-filtered loop)
5. Diagnostic: `pairs_total`, `pairs_culled`, `pairs_computed` (debug_assertions only)

### Sprint D: Zero-Fragment Face Preservation (Root Cause #3)

**File**: `vendor/truck/truck-shapeops/src/transversal/faces_classification/mod.rs`

When `divide_one_face` produces 0 fragments, the face is currently lost → Unknown.

1. In `classify_faces`, when a face produces 0 fragments from division, preserve the original undivided face
2. Classify the preserved face via `ray_cast_classify` (sample interior point, cast ray against other shell)
3. This eliminates the "unknown=14" cascade that makes the boolean irrecoverable
4. Unit test: face with corrupt loop store that produces 0 fragments → verify it's preserved and classified

### Sprint E: Un-ignore k8 + Integration Test

**File**: `crates/test-harness/tests/extrude_chains.rs`

1. Remove `#[ignore]` from `k8_three_bosses_then_three_cuts`
2. Volume-monotonicity assertions (already present)
3. Add Euler characteristic check after each feature
4. Add face-pair culling stats diagnostic

## Files

| File | Role |
|------|------|
| `vendor/truck/truck-shapeops/src/transversal/loops_store/mod.rs` | AABB culling in `create_loops_stores`, IC endpoint validation |
| `vendor/truck/truck-shapeops/src/transversal/loops_store/tests.rs` | Unit tests for AABB culling |
| `vendor/truck/truck-shapeops/src/transversal/bvh.rs` | Existing BVH infra (reference, may extend) |
| `crates/test-harness/tests/extrude_chains.rs` | k8 test case (un-ignore) |
| `specs/k8_loops_store_aabb.md` | This spec |

## Agent Team Structure

| Role | Agent | Responsibility |
|------|-------|----------------|
| Manager | team-lead | Orchestrate FIP phases, enforce DoD, route tasks |
| Spec Writer | (this spec) | Already complete |
| Test Author | test-author | Write failing tests for AABB culling (Sprint A tests) |
| Implementer | implementer | Implement AABB culling + IC validation (Sprints A-B) |
| Adversary | adversary | Pathological inputs, near-tolerance face pairs, regression sweep |
