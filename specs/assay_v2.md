# Spec: Assay v2 — Generative CAD Scenario Testing

**Status:** Draft
**Crates:** `test-harness` (primary), `waffle-types`, `feature-engine`, `kernel-fork`
**Extends:** `crates/test-harness/src/assay/` (Levels 0–5)

## Goal

Expand the assay module from simple box/circle profile generation to full generative CAD scenario testing that exercises every engine path: arbitrary sketch planes, complex sketch elements (lines, arcs, splines), iOverlay-based region decomposition, random operations (extrude/revolve/boolean), multi-step modeling chains, and persistent corpus storage with visualization. The result is a proptest-driven fuzzer that discovers edge cases human-authored tests miss.

## Motivation

The current assay module (Levels 0–5) generates only rectangular and circular profiles on the XY plane, producing simple extruded boxes and cylinders. Real CAD failures occur with:
- Non-axis-aligned sketch planes
- Complex multi-region profiles (L-shapes, nested holes, gear teeth)
- Revolve operations (especially partial-angle)
- Multi-step chains mixing extrude, revolve, and boolean
- Coincident geometry from random placement (the hardest bugs)

Assay v2 closes this gap by generating truly random scenarios across the full operation space.

## Parameters

### Global Generation Controls

| Parameter | Type | Default | Range | Description |
|-----------|------|---------|-------|-------------|
| `scale_envelope` | `f64` | 100.0 | 10.0–500.0 | Bounding cube side length for all geometry |
| `min_feature_size` | `f64` | 1.0 | 0.5–5.0 | Minimum dimension for any profile element |
| `max_sketch_elements` | `usize` | 20 | 3–50 | Cap on entities per sketch |
| `max_chain_length` | `usize` | 5 | 1–10 | Maximum operations in a modeling chain |
| `proptest_cases` | `u32` | 50 | 10–500 | Cases per proptest invocation |
| `proptest_timeout_ms` | `u64` | 30000 | 5000–120000 | Per-case timeout |

### Bias Controls

| Parameter | Type | Default | Description |
|-----------|------|---------|-------------|
| `intersection_bias` | `f64` | 0.7 | Probability that body B overlaps body A |
| `coplanar_bias` | `f64` | 0.2 | Probability of shared sketch plane between bodies |
| `coincident_edge_bias` | `f64` | 0.15 | Probability of aligned edges between bodies |
| `multi_body_rate` | `f64` | 0.3 | Probability of generating multi-body (no-merge) extrudes |
| `revolve_rate` | `f64` | 0.3 | Probability of revolve vs extrude for solid creation |
| `complex_profile_rate` | `f64` | 0.4 | Probability of multi-region profile vs simple rect/circle |

## Strategy Hierarchy

Eight levels, extending the existing six:

```
Level 0: ScaleEnvelope           (existing — dimension ranges)
Level 1: SketchPlane             (NEW — arbitrary origin + normal)
Level 2: SketchElements          (NEW — lines, arcs, circles, splines)
Level 3: ClosedRegions           (NEW — iOverlay region decomposition)
Level 4: RegionSelection         (NEW — pick profile(s) from regions)
Level 5: OperationSpec           (EXTENDED — extrude + revolve + cut)
Level 6: ModelingChain           (EXTENDED — multi-op with booleans)
Level 7: GenerativeScenario      (NEW — full scenario with metadata)
```

### Level 0: ScaleEnvelope (existing)

No changes. Reuse `dim_range()` (0.5–50.0) and `offset_range()` (-25.0–25.0).

### Level 1: SketchPlane (new)

```rust
struct SketchPlaneSpec {
    origin: [f64; 3],   // within scale_envelope
    normal: [f64; 3],   // unit vector
}
```

Strategies:
- `axis_aligned_plane()` — XY/XZ/YZ at random offset
- `tilted_plane()` — random normal via spherical coordinates (theta ∈ [0, π], phi ∈ [0, 2π])
- `sketch_plane_any()` — 60% axis-aligned, 40% tilted (axis-aligned is more likely to trigger coplanar degeneracies)

**Invariant:** Normal is always unit-length. Origin components stay within `[-scale_envelope/2, scale_envelope/2]`.

### Level 2: SketchElements (new)

Generate raw sketch entities before region decomposition.

```rust
struct SketchElementSet {
    entities: Vec<SketchEntity>,     // Points, Lines, Arcs, Circles
    positions: HashMap<u32, (f64, f64)>,  // Solved positions
}
```

Strategies:
- `random_polygon(n_sides: 3..8)` — convex polygon with random vertex positions
- `random_star(points: 4..8, inner_r, outer_r)` — star shape (non-convex, self-intersecting possible)
- `random_line_soup(n_lines: 3..max_sketch_elements)` — random line segments that may intersect
- `random_arcs(n_arcs: 1..5, base_polygon)` — replace polygon edges with arcs
- `random_circles(n_circles: 1..3)` — standalone circles (potential holes)
- `composite_sketch()` — polygon + optional arcs + optional inner circles

Entity ID assignment: points start at 1, lines/arcs/circles start at 100. Construction entities (centers) get `construction: true`.

**Constraint:** All positions lie within a 2D bounding box derived from `scale_envelope` and `min_feature_size`. No zero-length edges. No duplicate point positions (within tolerance 1e-6).

### Level 3: ClosedRegions (new — iOverlay integration)

Use `i_overlay` to decompose arbitrary sketch element sets into closed regions.

```rust
struct RegionDecomposition {
    regions: Vec<ClosedRegion>,
    source_elements: SketchElementSet,
}

struct ClosedRegion {
    outer: Vec<[f64; 2]>,           // outer contour (CCW)
    holes: Vec<Vec<[f64; 2]>>,      // hole contours (CW)
    area: f64,                       // signed area
    profile: ClosedProfile,          // waffle-types ClosedProfile
}
```

**iOverlay API usage** (matching patterns from `coplanar_overlay.rs`):

```rust
use i_overlay::core::fill_rule::FillRule;
use i_overlay::core::overlay_rule::OverlayRule;
use i_overlay::float::single::SingleFloatOverlay;

fn decompose_regions(elements: &SketchElementSet) -> Vec<ClosedRegion> {
    // 1. Convert sketch entities to Vec<Vec<[f64; 2]>> contours
    // 2. Build overlay shape via build_overlay_shape(outer, holes)
    // 3. Use .overlay(&empty, OverlayRule::Union, FillRule::EvenOdd)
    //    to decompose self-intersecting geometry into clean regions
    // 4. Filter regions by min_feature_size (area > min_feature_size^2)
    // 5. Classify outer vs hole contours by signed area (CCW = outer, CW = hole)
    // 6. Nest holes into their containing outer contours
}
```

Shape construction follows `build_overlay_shape()`:
```rust
fn build_overlay_shape(outer: &[[f64; 2]], holes: &[Vec<[f64; 2]>]) -> Vec<Vec<[f64; 2]>> {
    let mut shape = Vec::with_capacity(1 + holes.len());
    shape.push(outer.to_vec());        // first contour = outer boundary
    for hole in holes {
        shape.push(hole.clone());      // remaining = holes
    }
    shape
}
```

Result extraction: `overlay()` returns `Vec<Vec<Vec<[f64; 2]>>>` where outer vec = shapes, middle vec = contours (first = outer, rest = holes), inner vec = vertices.

**Fallback:** If iOverlay returns zero regions (degenerate geometry), fall back to `rect_profile()` or `circle_profile()` from existing helpers.

### Level 4: RegionSelection (new)

Pick which region(s) from the decomposition to use as extrude/revolve profiles.

```rust
struct RegionSelectionSpec {
    profile_index: usize,            // which region to use
    all_regions: Vec<ClosedRegion>,   // available regions
}
```

Strategies:
- `largest_region()` — pick the region with maximum area (most reliable)
- `random_region()` — uniform random selection
- `smallest_non_tiny(min_area)` — smallest region above threshold (stress-tests small features)

**Constraint:** Selected region must have area ≥ `min_feature_size^2`.

### Level 5: OperationSpec (extended)

Extend beyond simple extrude to include revolve, cut, and directed extrude.

```rust
enum OperationSpec {
    Extrude {
        depth: f64,                     // 1.0–50.0
        cut: bool,
        merge: bool,
        direction: Option<[f64; 3]>,    // None = plane normal
        symmetric: bool,
        depth_mode: DepthMode,          // Blind only for v2
    },
    Revolve {
        axis_origin: [f64; 3],
        axis_direction: [f64; 3],       // unit vector in sketch plane
        angle_deg: f64,                 // 30.0–360.0
    },
    BooleanOp {
        operation: BoolOp,              // Union, Subtract, Intersect
        target_name: String,            // named feature to boolean against
    },
}
```

Strategies:
- `extrude_spec()` — random depth, 30% cut, 70% merge
- `revolve_spec(plane: &SketchPlaneSpec)` — axis in sketch plane, random angle
- `boolean_spec(existing_features: &[String])` — pick operation + target
- `operation_any()` — weighted: 50% extrude, 20% revolve, 30% boolean (only if prior bodies exist)

**Revolve axis constraint:** Axis must lie in the sketch plane and not pass through the profile. Generated by picking a point outside the profile bounding box and a direction parallel to one sketch-plane axis.

### Level 6: ModelingChain (extended)

Multi-step operation sequences.

```rust
struct ModelingChain {
    steps: Vec<ChainStep>,
}

struct ChainStep {
    name: String,                       // feature name ("step_0", "step_1", ...)
    plane: SketchPlaneSpec,
    elements: SketchElementSet,
    regions: Vec<ClosedRegion>,
    selected_region: usize,
    operation: OperationSpec,
}
```

Strategies:
- `simple_chain(len: 2..5)` — extrude-only chain, each step booleans against previous
- `mixed_chain(len: 2..5)` — mix of extrude + revolve + boolean
- `stress_chain(len: 3..max_chain_length)` — deliberately overlapping geometry for degeneracy

Chain construction rules:
1. Step 0 always creates a base body (extrude, no boolean)
2. Steps 1+ may create new bodies or boolean against existing
3. If boolean fails at step N, chain truncates — partial chains are valid test cases
4. Each step's sketch plane is independent (different orientations exercise more paths)

### Level 7: GenerativeScenario (new)

Top-level wrapper with metadata for corpus storage and replay.

```rust
#[derive(Serialize, Deserialize, Clone, Debug)]
struct GenerativeScenario {
    id: String,                         // "gen-{hash}" auto-generated
    seed: u64,                          // proptest seed for reproduction
    chain: ModelingChain,
    bias_config: BiasConfig,
    expected_outcome: ExpectedOutcome,
    tags: Vec<String>,                  // ["coplanar", "revolve", "multi-body"]
}

#[derive(Serialize, Deserialize, Clone, Debug)]
struct BiasConfig {
    intersection_bias: f64,
    coplanar_bias: f64,
    coincident_edge_bias: f64,
    multi_body_rate: f64,
    revolve_rate: f64,
    complex_profile_rate: f64,
}

#[derive(Serialize, Deserialize, Clone, Debug)]
enum ExpectedOutcome {
    Success,                            // chain should complete
    KnownFailure { bug_id: String },    // known bug
    Unknown,                            // newly generated, outcome TBD
}
```

## Branch Table

### Operation × Profile Complexity

| Profile | Extrude | Extrude-Cut | Revolve | Boolean-Union | Boolean-Subtract | Boolean-Intersect |
|---------|---------|-------------|---------|---------------|------------------|-------------------|
| Rectangle | L0-existing | L5-new | L5-new | L3-existing | L3-existing | L3-existing |
| Circle | L0-existing | L5-new | L5-new | L3-existing | L3-existing | L3-existing |
| Convex polygon | L2-new | L5-new | L5-new | L6-new | L6-new | L6-new |
| Non-convex polygon | L2-new | L5-new | L5-new | L6-new | L6-new | L6-new |
| Polygon + arcs | L2-new | L5-new | L5-new | L6-new | L6-new | L6-new |
| Multi-region (holes) | L3-new | L5-new | L5-new | L6-new | L6-new | L6-new |
| Gear profile | L2-existing | L5-new | L5-new | L6-new | L6-new | L6-new |

### Sketch Plane × Operation

| Plane | Extrude | Revolve | Boolean (2-body) | Chain (3+ ops) |
|-------|---------|---------|-------------------|----------------|
| XY (Z-normal) | Baseline | New | Existing | Extended |
| XZ (Y-normal) | New | New | New | New |
| YZ (X-normal) | New | New | New | New |
| Tilted (arbitrary) | New | New | New | New |

### Degeneracy × Operation (extended from existing 36-cell matrix)

| Degeneracy | Union | Subtract | Intersect | Chain-2 | Chain-3+ |
|------------|-------|----------|-----------|---------|----------|
| None (general position) | Existing | Existing | Existing | New | New |
| Coplanar faces | Existing | Existing | Existing | New | New |
| Coincident edges | Existing | Existing | Existing | New | New |
| Vertex-on-face | Existing | Existing | Existing | New | New |
| Tangential | Stub | Stub | Stub | New | New |
| Near-miss (< tolerance) | New | New | New | New | New |

### Chain Length × Complexity

| Chain Length | Simple profiles | Complex profiles | Mixed operations |
|-------------|-----------------|------------------|------------------|
| 1 (single op) | Existing | New | N/A |
| 2 | Existing | New | New |
| 3 | Extended | New | New |
| 4–5 | Extended | New | New |
| 6–10 (stress) | New | New | New |

## Invariants

### Geometric Invariants (all operations)

1. **I1 — Non-empty result:** Every successful operation produces a solid with V ≥ 4, E ≥ 6, F ≥ 4 (minimum tetrahedron).
2. **I2 — Euler's formula:** V - E + F = 2 for every genus-0 result. For genus-g results, V - E + F = 2 - 2g.
3. **I3 — Manifold edges:** Every edge in the result is shared by exactly 2 faces.
4. **I4 — Face validity:** Every face has ≥ 3 bounding edges.
5. **I5 — Watertight mesh:** Every triangle edge in the tessellation is shared by exactly 2 triangles (no boundary edges).
6. **I6 — Consistent normals:** Geometric winding direction matches stored normal direction for all triangles.
7. **I7 — No degenerate triangles:** All triangles have area > 1e-12.
8. **I8 — Unit normals:** All normal vectors have magnitude within 1% of 1.0.

### Volume Invariants (boolean operations)

9. **I9 — Union monotonicity:** vol(A ∪ B) ≥ max(vol(A), vol(B)) (within 5% tolerance).
10. **I10 — Subtract bound:** vol(A - B) ≤ vol(A) (within 5% tolerance).
11. **I11 — Intersect bound:** vol(A ∩ B) ≤ min(vol(A), vol(B)) (within 5% tolerance).
12. **I12 — Volume conservation:** vol(A ∪ B) + vol(A ∩ B) = vol(A) + vol(B) (within 10% tolerance — accounts for tessellation error).

### Bounding Box Invariants

13. **I13 — Union bbox containment:** bbox(A ∪ B) ⊇ bbox(A) and bbox(A ∪ B) ⊇ bbox(B) (within 0.5 tolerance).
14. **I14 — Subtract bbox bound:** bbox(A - B) ⊆ bbox(A) (within 0.5 tolerance).
15. **I15 — Intersect bbox bound:** bbox(A ∩ B) ⊆ bbox(A) ∩ bbox(B) (within 0.5 tolerance).

### Determinism Invariants

16. **I16 — Topology determinism:** Same scenario run N times produces identical (V, E, F) counts every time.
17. **I17 — Volume determinism:** Same scenario run N times produces volumes within 1e-6 of each other.

### Chain Invariants

18. **I18 — Monotonic feature count:** Feature tree length equals number of completed chain steps.
19. **I19 — Partial chain validity:** If step N fails, steps 0..N-1 must all have valid solids.

### Region Decomposition Invariants

20. **I20 — Area conservation:** Sum of decomposed region areas equals total enclosed area of input contours (within 1% tolerance).
21. **I21 — Non-overlapping regions:** Pairwise intersection of decomposed regions has zero area.
22. **I22 — Minimum region size:** All regions passed to operations have area ≥ `min_feature_size^2`.

## Oracles

### Existing Oracles (reused from `oracle.rs`)

| # | Oracle | Function | Checks Invariant |
|---|--------|----------|------------------|
| O1 | Euler formula | `check_euler_formula()` | I2 |
| O2 | Manifold edges | `check_manifold_edges()` | I3 |
| O3 | Face validity | `check_face_validity()` | I4 |
| O4 | Watertight mesh | `check_watertight_mesh()` | I5 |
| O5 | Consistent normals | `check_consistent_normals()` | I6 |
| O6 | No degenerate triangles | `check_no_degenerate_triangles()` | I7 |
| O7 | Unit normals | `check_unit_normals()` | I8 |
| O8 | Valid indices | `check_valid_indices()` | — |
| O9 | Face range coverage | `check_face_range_coverage()` | — |
| O10 | Outward normals | `check_outward_normals()` | — |
| O11 | Bounding box | `check_bounding_box()` | I13–I15 |
| O12 | Topology counts | `check_topology_counts()` | I2 (exact) |
| O13 | Role exists | `check_role_exists()` | — |

### Existing Assay Oracles (reused from `properties.rs`)

| # | Oracle | Function | Checks Invariant |
|---|--------|----------|------------------|
| O14 | Volume monotonicity | `check_volume_monotonicity()` | I9–I11 |
| O15 | Euler invariant | `check_euler_invariant()` | I2 |
| O16 | Bbox containment | `check_bbox_containment()` | I13–I15 |
| O17 | Watertight (assay) | `check_watertight()` | I5 |
| O18 | Manifold mesh | `check_manifold_mesh()` | I3 |

### New Oracles

| # | Oracle | Checks Invariant | Description |
|---|--------|------------------|-------------|
| O19 | Body count | I1 | Result solid has V ≥ 4, E ≥ 6, F ≥ 4 |
| O20 | Non-empty result | I1 | Tessellation produces > 0 triangles |
| O21 | Volume upper bound | I9–I11 | vol(result) ≤ vol(scale_envelope^3) |
| O22 | Volume conservation | I12 | vol(A∪B) + vol(A∩B) ≈ vol(A) + vol(B) |
| O23 | Determinism (N-run) | I16, I17 | N identical runs, compare topology + volume |
| O24 | Chain monotonic features | I18 | Feature count equals completed steps |
| O25 | Partial chain validity | I19 | All completed steps have valid solids |
| O26 | Region area conservation | I20 | Sum(region areas) ≈ total enclosed area |
| O27 | Region non-overlap | I21 | Pairwise region intersection = 0 |
| O28 | Region min size | I22 | All used regions ≥ min_feature_size^2 |

### Oracle Application Matrix

| Scenario Type | Oracles Applied |
|---------------|----------------|
| Single extrude | O1–O10, O19, O20, O21 |
| Single revolve | O1–O10, O19, O20, O21 |
| Boolean (2-body) | O1–O21, O22 |
| Chain (3+ ops) | O1–O21, O24, O25 |
| Determinism check | O23 (standalone, 10 runs per case) |
| Region decomposition | O26, O27, O28 (pre-operation validation) |

## Persistence

### Corpus Entry Format

Extend existing `CorpusEntry` with `GenerativeScenario` serialization:

```rust
// Existing corpus entry — no changes
struct CorpusEntry {
    id: String,
    description: String,
    status: CorpusStatus,           // Pass | Fail | Ignore
    scenario: serde_json::Value,    // flexible JSON
    expected_topology: Option<(usize, usize, usize)>,
    expected_volume: Option<f64>,
}
```

New scenario JSON schema (stored in `corpus/generative/`):

```json
{
  "id": "gen-a1b2c3",
  "seed": 12345678,
  "chain": {
    "steps": [
      {
        "name": "step_0",
        "plane": { "origin": [0, 0, 0], "normal": [0, 0, 1] },
        "profile_type": "convex_polygon",
        "profile_params": { "n_sides": 5, "vertices": [[...]] },
        "selected_region": 0,
        "operation": {
          "type": "Extrude",
          "depth": 10.0,
          "cut": false,
          "merge": true
        }
      },
      {
        "name": "step_1",
        "plane": { "origin": [5, 0, 0], "normal": [0, 0, 1] },
        "profile_type": "circle",
        "profile_params": { "cx": 0, "cy": 0, "r": 3.0 },
        "selected_region": 0,
        "operation": {
          "type": "BooleanOp",
          "operation": "Subtract",
          "target_name": "step_0"
        }
      }
    ]
  },
  "bias_config": {
    "intersection_bias": 0.7,
    "coplanar_bias": 0.2
  },
  "expected_outcome": "Success",
  "tags": ["polygon", "boolean-subtract", "chain-2"]
}
```

### Auto-save on Failure

When proptest finds a failing case:
1. Serialize the `GenerativeScenario` to JSON
2. Save to `corpus/generative/gen-{hash}.json` with `status: Fail`
3. Log the proptest seed for `PROPTEST_SEED` replay
4. Tag with detected degeneracy families and operation types

### Replay

```rust
fn replay_generative_scenario(path: &Path) -> ReplayResult {
    let json = std::fs::read_to_string(path)?;
    let scenario: GenerativeScenario = serde_json::from_str(&json)?;
    let mut builder = ModelBuilder::truck();
    for step in &scenario.chain.steps {
        // Reconstruct sketch from stored profile params
        // Apply operation
        // Run oracles on intermediate result
    }
    // Run full oracle suite on final result
}
```

## Coverage Matrix

### Current Coverage (v1): 60 cells

```
DegeneracyFamily (4) × BoolOp (3) × PrimitivePair (5) = 60
```

(Tracked by existing `coverage.rs`)

### Extended Coverage (v2): 60 + 252 = 312 cells

New dimensions:

```
ProfileComplexity (6) × OperationType (6) × ChainLength (3) × SketchPlane (2) = 216
ProfileComplexity (6) × DegeneracyFamily (6) = 36
```

ProfileComplexity levels:
1. Simple rect
2. Simple circle
3. Convex polygon (3–8 sides)
4. Non-convex polygon
5. Polygon with arc edges
6. Multi-region (with holes)

OperationType levels:
1. Extrude (merge)
2. Extrude (cut)
3. Extrude (no-merge)
4. Revolve (full 360)
5. Revolve (partial angle)
6. Boolean (any)

ChainLength levels:
1. Single operation
2. Short chain (2–3 ops)
3. Long chain (4–10 ops)

SketchPlane levels:
1. Axis-aligned
2. Tilted (arbitrary normal)

### Coverage Tracking Extension

```rust
struct CoverageMatrixV2 {
    v1: CoverageMatrix,                // existing 60-cell matrix
    profile_op: BTreeSet<(ProfileComplexity, OperationType)>,
    profile_op_chain: BTreeSet<(ProfileComplexity, OperationType, ChainLength)>,
    profile_plane: BTreeSet<(ProfileComplexity, SketchPlaneKind)>,
    profile_degeneracy: BTreeSet<(ProfileComplexity, DegeneracyFamily)>,
}

impl CoverageMatrixV2 {
    fn total_cells(&self) -> usize { 312 }
    fn tested_cells(&self) -> usize { /* union of all sets */ }
    fn format_report(&self) -> String { /* gap analysis */ }
}
```

## Visualization

### STL Export Per Step

For debugging failing scenarios, export intermediate geometry:

```rust
fn export_chain_stl(builder: &mut ModelBuilder, chain: &ModelingChain, out_dir: &Path) {
    for (i, step) in chain.steps.iter().enumerate() {
        if let Ok(stl_bytes) = builder.export_stl(&step.name) {
            let path = out_dir.join(format!("step_{:02}_{}.stl", i, step.name));
            std::fs::write(&path, stl_bytes).ok();
        }
    }
}
```

### HTML Gallery (manual process)

Not part of automated test runs. A standalone script that:
1. Reads all corpus entries from `corpus/generative/`
2. Renders each STL using three.js STLLoader
3. Generates an `index.html` with thumbnails, status badges, and tag filters

### Screenshot Capture (optional)

For scenarios that load in the full app:
1. Start dev server
2. Load scenario via `__waffle` test API
3. Capture screenshot via Playwright
4. Save alongside corpus entry

This is a manual investigation tool, not part of CI.

## Failure Modes

### Expected Failures (handle gracefully)

| Failure | Cause | Handling |
|---------|-------|----------|
| Boolean cascade error | Truck kernel rejects degenerate intersection | Log + skip (not a property violation). Record in corpus as `KnownFailure`. |
| Zero regions from iOverlay | Degenerate or self-intersecting sketch | Fall back to `rect_profile()`. Tag scenario with `"ioverlay-fallback"`. |
| Revolve axis through profile | Invalid axis placement | Retry with different axis. If 3 retries fail, fall back to extrude. |
| Tessellation failure | Kernel cannot mesh degenerate solid | Log + skip step. Partial chain remains valid (I19). |
| Proptest timeout | Complex scenario exceeds `proptest_timeout_ms` | proptest marks case as discard, not failure. |
| Region too small | Generated profile below `min_feature_size` | Filter out region (O28). If no valid regions remain, fall back to simple profile. |

### Unexpected Failures (property violations — these are real bugs)

| Failure | Oracle | Priority |
|---------|--------|----------|
| Volume monotonicity violation | O14 | P1 — fundamental boolean algebra broken |
| Euler formula violation | O1, O15 | P1 — topology corruption |
| Non-manifold edges | O2, O18 | P1 — open shell, boolean didn't close properly |
| Non-deterministic topology | O23 | P1 — hash-order or floating-point instability |
| Watertight failure | O4, O17 | P2 — tessellation or shell closure issue |
| Degenerate triangles | O6 | P3 — tessellation quality, not correctness |
| Outward normal drift | O10 | P3 — cosmetic, affects rendering not geometry |

### Error Reporting

Every property violation produces:
1. `OracleVerdict` with `passed: false`, oracle name, detail string, optional numeric value
2. Auto-saved corpus entry with full scenario JSON
3. proptest shrinking attempts to find minimal failing case
4. Log line: `ASSAY VIOLATION: {oracle} failed on {scenario_id} (seed={seed}): {detail}`

## Delivery Phases

### Phase 1: Constructive Polygons + Extrude

- Implement Levels 1–2 (SketchPlane, SketchElements) for convex polygons only
- Implement Level 5 OperationSpec for basic extrude (no revolve, no cut)
- Wire through ModelBuilder: polygon sketch → extrude → run oracles O1–O10, O19–O21
- Add `generative_single_extrude` proptest in `assay_generative.rs`
- 50 proptest cases, axis-aligned planes only

### Phase 2: Full Sketch Elements + iOverlay

- Add non-convex polygons, arcs, circles to Level 2
- Implement Level 3 (ClosedRegions) with iOverlay decomposition
- Implement Level 4 (RegionSelection)
- Add oracles O26–O28 for region validation
- Extend proptest to include complex profiles with holes

### Phase 3: Revolve + Chains + Booleans

- Add revolve to Level 5 OperationSpec
- Implement Level 6 (ModelingChain) with boolean operations
- Add oracles O22, O24, O25
- Add `generative_chain` proptest with 2–5 step chains
- Enable tilted sketch planes (40% rate)

### Phase 4: Bias + Coverage + Visualization

- Implement BiasConfig and biased generation strategies
- Implement CoverageMatrixV2 tracking
- Add STL export per chain step
- Add HTML gallery script
- Tune bias parameters based on Phase 1–3 failure patterns

### Phase 5: CI Integration

- Add `generative_corpus_replay` test that replays all `corpus/generative/*.json`
- Integrate with `./scripts/test.sh full` tier
- Set proptest case count based on CI vs local (CI: 200 cases, local: 50)
- Add coverage gap report to CI output
- Auto-save failing cases to corpus with git-committable JSON

## Implementation Notes

### ModelBuilder Integration

All scenario execution goes through the existing `ModelBuilder` API:

```rust
// Polygon profile via manual sketch API
builder.begin_sketch();
for entity in &elements.entities {
    match entity {
        SketchEntity::Point { id, x, y, .. } => builder.add_point(*id, *x, *y),
        SketchEntity::Line { id, start_id, end_id, .. } => builder.add_line(*id, *start_id, *end_id),
        SketchEntity::Arc { id, center_id, start_id, end_id, .. } => builder.add_arc(*id, *center_id, *start_id, *end_id),
        SketchEntity::Circle { id, center_id, radius, .. } => builder.add_circle_entity(*id, *center_id, *radius),
        _ => {},
    }
}
builder.finish_sketch_manual("sk", plane.origin, plane.normal, positions, profiles)?;

// Operations
match &step.operation {
    OperationSpec::Extrude { depth, cut, merge, .. } => {
        if *cut { builder.extrude_cut(name, sketch, *depth)?; }
        else if !*merge { builder.extrude_no_merge(name, sketch, *depth)?; }
        else { builder.extrude(name, sketch, *depth)?; }
    }
    OperationSpec::Revolve { axis_origin, axis_direction, angle_deg } => {
        builder.revolve(name, sketch, *axis_origin, *axis_direction, *angle_deg)?;
    }
    OperationSpec::BooleanOp { operation, target_name } => {
        match operation {
            BoolOp::Union => builder.boolean_union(name, target_name, body_name)?,
            BoolOp::Subtract => builder.boolean_subtract(name, target_name, body_name)?,
            BoolOp::Intersect => builder.boolean_intersect(name, target_name, body_name)?,
        };
    }
}
```

### Proptest Configuration

```rust
proptest! {
    #![proptest_config(ProptestConfig {
        cases: 50,
        max_shrink_iters: 100,
        timeout: 30000,
        ..ProptestConfig::default()
    })]

    #[test]
    fn generative_single_extrude(scenario in generative_extrude_scenario()) {
        let result = execute_generative_scenario(&scenario);
        match result {
            Ok(mut builder) => {
                let verdicts = run_generative_oracles(&mut builder, &scenario);
                for v in &verdicts {
                    prop_assert!(v.passed, "{}: {}", v.oracle_name, v.detail);
                }
            }
            Err(e) if is_known_kernel_limitation(&e) => {
                // Expected failure — skip, don't count as property violation
            }
            Err(e) => {
                prop_assert!(false, "Unexpected execution error: {}", e);
            }
        }
    }
}
```

### File Organization

```
crates/test-harness/src/assay/
├── mod.rs                    # Module declarations (updated)
├── strategies.rs             # Levels 0–5 (existing, extended)
├── strategies_v2.rs          # Levels 1–4, 6–7 (new)
├── properties.rs             # Oracles O14–O18 (existing)
├── properties_v2.rs          # Oracles O19–O28 (new)
├── corpus.rs                 # Corpus management (existing, extended)
├── coverage.rs               # Coverage matrix (existing)
├── coverage_v2.rs            # Extended coverage (new)
├── determinism.rs            # Determinism checks (existing)
├── regions.rs                # iOverlay integration (new)
└── visualization.rs          # STL export + gallery (new)

crates/test-harness/tests/
├── assay_box_box.rs          # Existing proptest
├── assay_determinism.rs      # Existing proptest
├── assay_regression.rs       # Existing corpus replay
├── assay_generative.rs       # NEW: generative single-op tests
├── assay_generative_chain.rs # NEW: generative chain tests
└── assay_corpus_replay.rs    # NEW: generative corpus replay

corpus/
├── *.json                    # Existing hand-curated entries
└── generative/               # NEW: auto-saved generative failures
    └── gen-*.json
```
