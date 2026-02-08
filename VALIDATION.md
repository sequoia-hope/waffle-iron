# BREP Kernel Verification System — Implementation Plan for Waffle Iron

## Context and Problem Statement

Waffle Iron is a Rust BREP CAD kernel being built by AI coding agents (Claude Code). The agents write kernel code (fillet operations, booleans, etc.) and then write tests to validate that code. The core problem: **agents report success on operations like fillets, but the geometry is actually broken.** The agents lack a programmatic verification system that can definitively tell them whether the geometry their code produces is valid.

This plan describes a verification subsystem that serves two audiences simultaneously:

1. **End users of the kernel** who call `solid.validate()` to check their models
2. **The coding agents themselves** who call the same validation in test assertions to prove their implementations are correct

The verification system is the foundation. Without it, no amount of agent effort on fillets, booleans, or any other operation can be trusted. Every operation implementation should be developed *test-first against the validator*, not the other way around.

---

## What Waffle Iron Already Has

Based on the repository (branch `claude/parametric-cad-system-5tsGg`):

- **Half-edge data structure** with SlotMap arena allocation — good foundation
- **Euler-Poincaré validation** (V - E + F = 2) — present but only for genus-0
- **Topology auditing** — closed loops, twin consistency, dangling vertices, normal consistency
- **Mesh validation** — boundary edges, non-manifold edges, Euler characteristic, signed volume
- **Vertex welding** — quantize-based spatial hashing for watertight meshes
- **Winding repair** — BFS propagation + signed volume orientation
- **330 tests** including 27 property-based tests

This is a solid starting point. The gaps are in *geometric* verification (as opposed to topological), *continuity* checks, *tolerance management*, and crucially — **a unified validation API that returns machine-readable diagnostics the agents can assert against.**

---

## Architecture: The Validation Trait and Result Types

### Core Design Principle

Every check returns a structured result, not a bool. The agent needs to know *what* failed, *where* it failed, and *how badly* it failed, so it can fix the implementation — not just retry blindly.

### Proposed API

```rust
/// The central validation entry point
pub struct BRepValidator {
    config: ValidationConfig,
}

/// What level of checking to perform
pub struct ValidationConfig {
    pub level: ValidationLevel,        // How deep to check
    pub tolerance: ToleranceConfig,     // Global tolerance settings
    pub sampling_density: u32,         // Points per edge for geometric checks
    pub check_continuity: bool,        // Whether to run G1/G2 checks
    pub check_self_intersection: bool, // Expensive but definitive
}

pub enum ValidationLevel {
    /// Levels 0-1: Structural + topological (microseconds)
    Topology,
    /// Levels 0-2: + geometric consistency (milliseconds)
    Geometry,
    /// Levels 0-3: + spatial coherence, self-intersection (tens of ms)
    Spatial,
    /// Levels 0-5: Everything including mesh-based checks (hundreds of ms)
    Full,
}

/// The structured result an agent can parse and assert against
pub struct ValidationReport {
    pub valid: bool,
    pub level_completed: ValidationLevel,
    pub errors: Vec<ValidationError>,
    pub warnings: Vec<ValidationWarning>,
    pub metrics: ValidationMetrics,
}

pub struct ValidationError {
    pub entity_type: EntityType,       // Vertex, Edge, Wire, Face, Shell, Solid
    pub entity_id: EntityId,           // Which specific entity
    pub parent_id: Option<EntityId>,   // Context (e.g., which face this edge is on)
    pub code: ErrorCode,               // Specific failure (see enum below)
    pub message: String,               // Human-readable description
    pub severity: Severity,            // Error vs Fatal
    pub numeric_value: Option<f64>,    // e.g., the actual gap distance
    pub tolerance: Option<f64>,        // e.g., the allowed tolerance
}

pub struct ValidationMetrics {
    pub volume: Option<f64>,
    pub surface_area: Option<f64>,
    pub bounding_box: Option<Aabb>,
    pub euler_poincare: EulerPoincare,
    pub tolerance_stats: ToleranceStats,
    pub entity_counts: EntityCounts,
}
```

### Error Code Enum (Modeled on OpenCascade's 37 BRepCheck_Status codes)

This is the *language* the validator speaks. Every error has exactly one code. The agents learn what these mean and can pattern-match on them.

```rust
pub enum ErrorCode {
    // === Vertex checks ===
    InvalidPointOnCurve,          // Vertex not within tolerance of adjacent edge curve
    InvalidPointOnSurface,        // Vertex not within tolerance of adjacent face surface

    // === Edge checks ===
    No3DCurve,                    // Edge has no geometric curve
    Multiple3DCurve,              // Edge has more than one 3D curve
    Invalid3DCurve,               // 3D curve is degenerate or invalid
    NoCurveOnSurface,             // Missing pcurve for an adjacent face
    InvalidCurveOnSurface,        // Pcurve is degenerate or invalid
    SameParameterViolation,       // |C(t) - S(P(t))| > tolerance (THE critical check)
    InvalidRange,                 // Edge parameter range is empty or inverted
    InvalidDegeneratedEdge,       // Degenerate edge is geometrically wrong
    FreeEdge,                     // Edge used by only one face (gap/hole)
    InvalidMultiConnexity,        // Edge used by 3+ faces (non-manifold)
    ZeroLengthEdge,               // Edge shorter than tolerance

    // === Wire checks ===
    WireNotClosed,                // Edges don't form a closed loop
    SelfIntersectingWire,         // Edges cross within a wire
    InconsistentEdgeOrientation,  // Edge directions don't chain correctly

    // === Face checks ===
    NoSurface,                    // Face has no underlying surface
    InvalidImbricationOfWires,    // Inner/outer wire nesting is wrong
    IntersectingWires,            // Wires on the same face cross each other
    UnorientableFace,             // Face surface is non-orientable
    ZeroAreaFace,                 // Face has negligible area (sliver)

    // === Shell checks ===
    ShellNotClosed,               // Shell has free/boundary edges
    BadOrientationOfFaces,        // Adjacent faces have inconsistent normals
    UnorientableShell,            // Cannot assign consistent orientation

    // === Solid checks ===
    InvalidImbricationOfShells,   // Shells overlap or nest incorrectly
    EulerPoincareViolation,       // V - E + F ≠ 2(S - G) + (L - F)
    NegativeVolume,               // Signed volume is negative (inside-out)
    SelfIntersection,             // Faces pass through each other

    // === Tolerance checks ===
    ToleranceHierarchyViolation,  // Tol(vertex) < Tol(edge) or similar
    ExcessiveTolerance,           // Tolerance exceeds configured maximum
    ToleranceGrowth,              // Tolerance increased beyond threshold after operation

    // === Continuity checks (especially relevant for fillets) ===
    G0Discontinuity,              // Positional gap at surface boundary
    G1Discontinuity,              // Tangent/normal mismatch at boundary
    G2Discontinuity,              // Curvature mismatch at boundary
}
```

---

## The Six-Level Check Hierarchy

Each level only runs if the previous level passed. This is critical for agent efficiency — don't burn tokens diagnosing self-intersection when the topology is corrupt.

### Level 0 — Structural Integrity (O(1) per entity, microseconds total)

These are data structure invariants. Many are enforced by the half-edge structure itself, but they should still be verified explicitly since the agents are *building* the data structure code.

**Checks:**
- Every half-edge has a twin, and `twin.twin == self`
- Every half-edge has a next, and following `next` returns to start
- Every half-edge references a valid vertex, edge, and face
- Every edge has at least 2 half-edges
- Every face has at least 1 wire (loop of half-edges)
- Every shell contains at least 1 face
- No null/dangling references in the SlotMap arena

**Why agents need this:** When implementing a new operation (e.g., fillet), the agent modifies the half-edge structure. If it introduces a dangling reference or breaks a twin linkage, Level 0 catches it immediately. Without this, the agent gets mysterious crashes or wrong results downstream and can't localize the bug.

**Test pattern for agents:**
```rust
#[test]
fn fillet_preserves_halfedge_invariants() {
    let solid = make_box(10.0, 10.0, 10.0);
    let edge = select_edge(&solid, ...);
    let filleted = fillet(&solid, edge, 1.0).unwrap();

    let report = BRepValidator::new(ValidationConfig::topology())
        .validate(&filleted);
    assert!(report.errors.is_empty(),
        "Structural errors after fillet: {:?}", report.errors);
}
```

### Level 1 — Topological Invariants (O(V+E+F), microseconds to milliseconds)

Pure graph-theory checks on the topology. No curve or surface evaluation.

**Checks:**
- **Euler-Poincaré**: `V - E + F - (L - F) - 2(S - G) = 0`
  - The existing implementation only checks `V - E + F = 2` (genus 0, single shell). Must be generalized for fillets that may create higher-genus topology or multi-shell results.
- **Edge valence**: Every edge shared by exactly 2 faces (for manifold solids)
- **Orientation consistency**: Propagate orientation across shared edges; contradiction = non-orientable
- **Wire closure**: Every wire is a closed loop of edges
- **Connectivity**: Every shell is a single connected component

**Why agents need this:** After implementing boolean operations or fillets, the topology is surgically modified. Euler-Poincaré is a checksum — if it's wrong, the agent introduced or removed an entity without maintaining consistency. Edge valence catches the common bug of accidentally creating non-manifold edges during face splitting.

**Generalized Euler-Poincaré implementation note:** The existing `V - E + F = 2` must be extended to `V - E + F = 2 - 2G + H` where G is genus (through-holes) and H accounts for inner loops. For the agent, the pragmatic approach: compute V, E, F, L (total loops) from the topology directly, then verify the formula. If the agent doesn't know the expected genus, at minimum verify `V - E + F` is even and positive.

**Test pattern for agents:**
```rust
#[test]
fn boolean_union_topology_valid() {
    let a = make_box(10.0, 10.0, 10.0);
    let b = make_cylinder(3.0, 15.0)
        .translated(vec3(5.0, 5.0, 0.0));
    let result = boolean_union(&a, &b).unwrap();

    let report = BRepValidator::new(ValidationConfig::topology())
        .validate(&result);

    assert!(report.errors.is_empty());
    // Also verify entity counts make sense
    assert!(report.metrics.entity_counts.faces > 6,
        "Union should have more faces than a box");
}
```

### Level 2 — Geometric Consistency (O(n) with curve/surface evaluation, milliseconds)

This is where Waffle Iron's current validation has the biggest gaps, and where fillets most commonly fail silently.

**Checks:**

**SameParameter (THE critical check for fillets):**
For every edge, sample N points (at least 23, the OCCT default) along the edge's parametric range. At each sample point `t`:
1. Evaluate the edge's 3D curve: `P_3d = curve_3d(t)`
2. For each adjacent face, get the pcurve parameter: `(u, v) = pcurve(t)`
3. Evaluate the face's surface: `P_surf = surface(u, v)`
4. Check: `|P_3d - P_surf| ≤ tolerance_edge`

If this fails, the edge's 3D curve doesn't lie on the face's surface. This is the most common fillet failure: the agent constructs a blend surface and trim curves that don't actually agree in 3D space.

**Vertex-on-curve and vertex-on-surface:**
Every vertex must lie within `tolerance_vertex` of all its adjacent edge curves (at the curve endpoints) and all its adjacent face surfaces.

**Tolerance hierarchy:**
`Tol(Vertex) ≥ Tol(Edge) ≥ Tol(Face)` for all adjacent entity pairs. Violation means the tolerance model is inconsistent.

**Degenerate geometry detection:**
- Edges shorter than `tolerance_edge` (zero-length)
- Faces with area below threshold (sliver faces from aggressive fillets)
- Faces with extreme aspect ratios (bounding box ratio > 1000:1)

**Missing geometry:**
- Every edge must have a 3D curve
- Every edge must have a pcurve for each adjacent face
- Every face must have an underlying surface

**Why agents need this:** When implementing fillets, the agent constructs new surfaces (rolling ball envelopes) and new trim curves. The SameParameter check verifies that these new geometric entities are *mutually consistent*. A fillet can have perfect topology but completely wrong geometry — the blend surface doesn't actually connect to the base faces. This is exactly the failure mode described in the problem statement: "agents report success but fillets are still broken."

**Test pattern for agents:**
```rust
#[test]
fn fillet_edges_lie_on_surfaces() {
    let solid = make_box(10.0, 10.0, 10.0);
    let edge = select_edge(&solid, ...);
    let filleted = fillet(&solid, edge, 1.0).unwrap();

    let report = BRepValidator::new(ValidationConfig::geometry())
        .validate(&filleted);

    // No SameParameter violations
    let sp_errors: Vec<_> = report.errors.iter()
        .filter(|e| matches!(e.code, ErrorCode::SameParameterViolation))
        .collect();
    assert!(sp_errors.is_empty(),
        "SameParameter violations: {:?}", sp_errors);
}
```

### Level 3 — Spatial Coherence (O(n log n), tens of milliseconds)

**Checks:**

**Wire self-intersection (2D):**
For each face, project all wire edges into the face's parameter space and check for intersections using an AABB tree. Adjacent edges sharing a vertex are excluded.

**Free boundary detection:**
Scan all edges; any edge adjacent to only one face is a free boundary (gap/hole in the shell). For a valid closed solid, this count must be zero. The existing mesh validation likely has this, but it must also work at the BREP level, not just the tessellated mesh level.

**Face-face intersection (targeted):**
For operations like fillets and booleans, check that newly created faces don't intersect existing faces that they shouldn't intersect. Full O(n²) face-face intersection is expensive; use AABB broad-phase to only check face pairs whose bounding boxes overlap.

**Self-intersection via tessellation:**
Tessellate the solid, build an AABB tree over all triangles, and check for triangle-triangle intersections between non-adjacent triangles. This is the most reliable way to catch self-intersecting fillet surfaces.

**Why agents need this:** Self-intersection is the hardest fillet failure to detect without this kind of check. A fillet surface can have valid topology, valid edge-surface consistency (Level 2), but still fold through itself in 3D space. Only spatial checks catch this.

**Test pattern for agents:**
```rust
#[test]
fn fillet_no_self_intersection() {
    let solid = make_box(10.0, 10.0, 10.0);
    // Fillet with radius approaching half the edge length
    let edge = select_edge(&solid, ...);
    let filleted = fillet(&solid, edge, 4.5).unwrap();

    let report = BRepValidator::new(ValidationConfig::spatial())
        .validate(&filleted);

    let si_errors: Vec<_> = report.errors.iter()
        .filter(|e| matches!(e.code, ErrorCode::SelfIntersection))
        .collect();
    assert!(si_errors.is_empty(),
        "Self-intersection detected: {:?}", si_errors);
}
```

### Level 4 — Physical Plausibility (O(n), but requires tessellation)

**Checks:**

**Volume computation and comparison:**
Compute signed volume via the divergence theorem on the tessellated mesh:
`V = (1/6) Σ (T₀ · (T₁ × T₂))` summed over all triangles.
- Volume must be positive (negative = inside-out normals)
- Volume must be non-zero
- Volume should be compared to pre-operation volume:
  - Fillet on convex edge: volume decreases by approximately `(1 - π/4) × r² × edge_length`
  - Fillet on concave edge: volume increases by similar amount
  - Boolean union: `V(A∪B) ≤ V(A) + V(B)`
  - Boolean difference: `V(A-B) ≤ V(A)`

**Surface area computation and comparison:**
Similar sanity checks on total surface area.

**Bounding box containment:**
The result's bounding box should be a reasonable superset/subset of the input's bounding box depending on the operation.

**Why agents need this:** Volume comparison is the highest-value single check for catching "something went wrong but I can't tell what" scenarios. If a fillet produces a solid with zero volume, or the volume changed by 10x, the implementation is wrong even if all topological checks pass. This is the check that catches the case where the agent's fillet implementation produces geometry that "looks like it might be okay" topologically but is fundamentally broken spatially.

**Test pattern for agents:**
```rust
#[test]
fn fillet_volume_changes_correctly() {
    let solid = make_box(10.0, 10.0, 10.0);
    let pre_volume = compute_volume(&solid);

    let edge = select_edge(&solid, ...);
    let filleted = fillet(&solid, edge, 1.0).unwrap();

    let report = BRepValidator::full().validate(&filleted);
    assert!(report.valid);

    let post_volume = report.metrics.volume.unwrap();

    // Convex fillet removes material
    assert!(post_volume < pre_volume,
        "Convex fillet should reduce volume: {} >= {}", post_volume, pre_volume);

    // Volume change should be reasonable (not 0, not half the box)
    let volume_ratio = post_volume / pre_volume;
    assert!(volume_ratio > 0.9 && volume_ratio < 1.0,
        "Volume change unreasonable: ratio = {}", volume_ratio);
}
```

### Level 5 — Continuity Verification (targeted, for fillet/blend operations)

This level runs only when specifically requested or after operations known to create surface blends.

**Checks:**

**G0 continuity (positional):**
At N sample points along each new boundary edge (where a fillet surface meets a base surface), evaluate position from both surfaces. The gap must be ≤ the positional tolerance.
- Metric: `max_gap = max(|P_surface1(t) - P_surface2(t)|)` over all samples
- Typical tolerance: 0.001 mm for engineering, 0.01 mm for prototyping

**G1 continuity (tangent):**
At the same sample points, compute surface normals from both sides. The angle between normals must be ≤ the angular tolerance.
- Metric: `max_angle = max(arccos(n₁ · n₂))` over all samples
- Typical tolerance: 1.0° for general engineering, 0.1° for automotive

**G2 continuity (curvature):**
Compute principal curvatures from both sides using the second fundamental form. The relative curvature deviation must be within tolerance.
- Metric: `max_curv_dev = max(|κ₁ - κ₂| / (κ₁ + κ₂))` over all samples
- Typical tolerance: 10% for engineering, 2% for Class A surfaces
- Note: Standard constant-radius fillets only guarantee G1, not G2. G2 requires variable-radius or special blend types.

**Why agents need this:** This is the fillet-specific check that catches the most subtle failures. A fillet can have valid topology, valid SameParameter, even valid self-intersection checks, but still have a visible crease at the blend boundary because the normals don't match. The agent implementing fillets needs this feedback to know whether its rolling-ball algorithm is computing the correct envelope.

**Test pattern for agents:**
```rust
#[test]
fn fillet_is_tangent_continuous() {
    let solid = make_box(10.0, 10.0, 10.0);
    let edge = select_edge(&solid, ...);
    let filleted = fillet(&solid, edge, 1.0).unwrap();

    let report = BRepValidator::new(ValidationConfig {
        level: ValidationLevel::Full,
        check_continuity: true,
        ..Default::default()
    }).validate(&filleted);

    let g1_errors: Vec<_> = report.errors.iter()
        .filter(|e| matches!(e.code, ErrorCode::G1Discontinuity))
        .collect();
    assert!(g1_errors.is_empty(),
        "G1 discontinuities at fillet boundaries: {:?}", g1_errors);
}
```

---

## Tolerance Management

### The Problem

Every modeling operation introduces numerical error. Tolerances grow across a chain of operations. Professional kernels track this per-entity. Without explicit tolerance management, the validator can't distinguish "this is within acceptable approximation error" from "this is broken geometry."

### Recommended Implementation

```rust
pub struct ToleranceConfig {
    /// Smallest meaningful distance (below this, points are identical)
    pub resolution: f64,           // Default: 1e-7
    /// Maximum allowed vertex tolerance
    pub max_vertex_tol: f64,       // Default: 1e-3
    /// Maximum allowed edge tolerance
    pub max_edge_tol: f64,         // Default: 1e-4
    /// Default angular tolerance for G1 checks (radians)
    pub angular_tol: f64,          // Default: 0.017 (≈ 1°)
    /// Maximum tolerance growth factor per operation
    pub max_growth_factor: f64,    // Default: 10.0
}
```

Every vertex, edge, and face should carry a local tolerance value. The validator checks:
1. `vertex.tolerance ≥ edge.tolerance ≥ face.tolerance` for adjacent entities
2. No tolerance exceeds `max_*_tol`
3. After an operation, tolerance growth doesn't exceed `max_growth_factor × pre_operation_tolerance`

### Why agents need this

Without tolerance tracking, the agent implementing fillets will write code that introduces unbounded approximation error and never know. The agent's fillet might "work" on a 10mm box but fail on a 0.1mm feature because tolerances grew past the point where adjacent edges swallowed each other. Explicit tolerance tracking makes this failure visible and diagnosable.

---

## Fillet-Specific Verification Checklist

When the agent implements or modifies fillet operations, every fillet test should run this complete sequence:

### Pre-operation
1. Validate the input solid at Level 2+ (geometry). The input must be valid or the fillet implementation can't be blamed for bad output.
2. Record input volume, surface area, bounding box, and entity counts.
3. Record the edge(s) being filleted and their lengths.

### Post-operation
4. Check the fillet operation returned `Ok` (not a library-level error).
5. Run full validation (Level 0 through Level 4) on the result.
6. Run G1 continuity checks along all new fillet-to-base-face boundary edges.
7. Compare volume: for a convex fillet of radius r on an edge of length L, the removed volume is approximately `(1 - π/4) × r² × L`. The actual change should be within 2x of this estimate.
8. Verify face count increased (a fillet replaces edge-adjacent faces with trimmed versions plus new blend faces).
9. Verify no new free edges exist that weren't there before.
10. For each new fillet face, verify its surface normal points outward (dot with centroid-to-center-of-mass vector should be positive for convex solids).

### Edge cases that must be tested
- Fillet radius approaching half the shortest adjacent edge → should either succeed with valid geometry or fail gracefully
- Fillet radius larger than an adjacent face can accommodate → must return an error, not broken geometry
- Fillet at a vertex where 3 edges meet → vertex blend required
- Variable-radius fillet (if supported)
- Fillet on a curved edge (not just straight edges)
- Chained fillets on multiple edges of the same solid
- Fillet after boolean (the most common real-world sequence)

---

## Implementation Phases

### Phase 1: Unify and Extend Existing Checks (estimated: 1-2 agent sessions)

The repo already has topology auditing and mesh validation scattered across modules. Phase 1 consolidates these into the `BRepValidator` API with the structured `ValidationReport` return type.

**Tasks:**
- Define the `ValidationReport`, `ValidationError`, `ErrorCode` types
- Wrap existing Euler-Poincaré check into Level 1
- Wrap existing half-edge consistency checks into Level 0
- Wrap existing mesh validation (boundary edges, non-manifold, signed volume) into Level 4
- Add `validate()` method to Solid that returns a `ValidationReport`
- Write tests that validate known-good primitives (box, cylinder, sphere) pass all levels
- Write tests with intentionally broken geometry that verify the correct `ErrorCode` is returned

**Critical:** The agent building this must *also* write negative tests — construct invalid solids and verify the validator catches them. A validator that passes everything is worthless.

### Phase 2: Geometric Consistency (estimated: 2-3 agent sessions)

Add Level 2 checks that the repo currently lacks.

**Tasks:**
- Implement the SameParameter check: sample edge curves and verify they lie on adjacent face surfaces
- Implement vertex-on-curve and vertex-on-surface checks
- Add tolerance tracking to vertices, edges, and faces
- Implement degenerate geometry detection (zero-length edges, sliver faces)
- Implement tolerance hierarchy verification
- Write tests against box, cylinder, sphere primitives (should all pass)
- Write tests with manually perturbed geometry (offset a control point, verify SameParameter catches it)

### Phase 3: Spatial Checks via AABB Tree (estimated: 2-3 agent sessions)

Add Level 3 self-intersection and spatial coherence checks.

**Tasks:**
- Build (or adapt from existing code) an AABB tree over tessellated triangles
- Implement triangle-triangle intersection detection for non-adjacent pairs
- Implement free-boundary detection at the BREP level (not just mesh level)
- Implement wire self-intersection detection in face parameter space
- Write tests with known self-intersecting geometry (e.g., a surface folded back on itself)

### Phase 4: Continuity Verification (estimated: 1-2 agent sessions)

Add Level 5 continuity checks for fillet validation.

**Tasks:**
- Implement G0 gap measurement at surface boundaries
- Implement G1 normal angle measurement at surface boundaries
- Implement G2 curvature deviation measurement
- Identify which edges are "blend boundaries" (where a new fillet surface meets an original surface) automatically from the operation history
- Write tests: fillet a box edge, verify G1 continuity along all blend boundaries
- Write negative tests: construct a blend with intentionally wrong normals, verify G1Discontinuity is flagged

### Phase 5: Operation-Level Test Harness (estimated: 1 agent session)

Build the `pre-operation → operation → post-operation` test harness that wraps the validation checks into a reusable pattern the agents use for every operation they implement.

**Tasks:**
- Create an `OperationTestHarness` that records pre-operation metrics, runs the operation, and runs post-operation validation
- Implement volume comparison logic with per-operation expected bounds
- Implement entity count sanity checks (e.g., fillet should add faces)
- Create a macro or helper: `assert_operation_valid!(input, |solid| fillet(solid, edge, radius))`
- Apply this harness to all existing operation tests as a demonstration

---

## How Agents Should Use This System

### When implementing a new operation (e.g., fillet)

1. **First**, write the test using the validation harness — before writing the operation code
2. The test should assert: no validation errors at Level 2+, volume is reasonable, continuity is G1 at blend boundaries
3. **Then** implement the operation
4. **Run the test.** If it fails, read the `ValidationError` codes and numeric values to understand what's wrong
5. The error codes tell you exactly what to fix: `SameParameterViolation` with `numeric_value: 0.5` means your edge curve is 0.5 units away from the surface — not a small tolerance issue but a fundamental algorithmic bug
6. **Never** mark a task as complete until the validation test passes

### When debugging a failing operation

1. Run at `ValidationLevel::Topology` first. If this fails, the bug is in topology construction (half-edge surgery), not geometry
2. If topology passes, run at `ValidationLevel::Geometry`. `SameParameterViolation` on a specific edge tells you exactly which curve/surface pair disagrees
3. If geometry passes, run at `ValidationLevel::Spatial`. Self-intersection means the surface is valid locally but folds globally
4. If all levels pass but the visual result looks wrong, check continuity (G1/G2) — the geometry is technically valid but has visible creases

### Golden rule

**If you can't write a validation check that distinguishes your correct implementation from a broken one, you don't understand the operation well enough to implement it.** The verification system is not just a safety net — it's a specification. Defining what "valid fillet output" means in terms of concrete checks (SameParameter ≤ 1e-6, G1 angle ≤ 1°, volume decreased, no free edges) IS the spec that makes the operation implementable.

---

## Reference: How Professional Kernels Validate

### OpenCascade (OCCT)

`BRepCheck_Analyzer` creates entity-specific checkers (BRepCheck_Vertex, BRepCheck_Edge, etc.) and runs them bottom-up. Results are stored as a map from sub-shape to a list of status codes (37 distinct codes). Supports topology-only or topology+geometry modes. The SameParameter check uses 23 equidistant sample points per edge. Also has `ShapeAnalysis_*` for read-only diagnostics and `ShapeFix_*` for repair — these are separated by design.

### Parasolid

`PK_BODY_check` runs check groups sequentially — each group only runs if the previous group passed. Critically, Parasolid supports **local checking** via `local_check` on modeling operations — only validate entities changed by the specific operation. This is very relevant for agent workflows and should be an eventual goal for Waffle Iron.

### ACIS

`api_check_entity` with configurable levels from 10 (fast) to 70 (full face-face intersection). Level 20 is the default. ACIS's blend documentation explicitly acknowledges that vertex blends (where 3+ filleted edges meet) are the hardest problem and uses special Gregory polygon surfaces.

### Key Insight from All Three

Validation is not a single boolean. It's a hierarchical, configurable system with dozens of specific failure modes. The value is not just in passing/failing but in *which specific check failed and by how much*, because that information tells the implementer (or the agent) exactly what to fix.
