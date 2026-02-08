# Waffle Iron — Cross-Project Interface Definitions

All cross-project Rust type contracts. These types will become actual Rust code — they are syntactically valid Rust. Every sub-project agent reads this file before starting work.

**Serialization rules:**
- All persisted types use `#[serde(tag = "type")]` for forward-compatible enum tagging.
- Types containing `KernelSolidHandle` or `KernelId` are NEVER persisted — they exist only at runtime.
- Features store `GeomRef` for geometry references, never kernel-internal IDs.

---

## A) Geometry Reference System

**Producer:** feature-engine, modeling-ops
**Consumer:** feature-engine, modeling-ops, sketch-ui, ui-chrome, file-format

These types form the persistent naming system — the mechanism by which features reference geometry that survives parametric rebuilds.

```rust
use std::fmt;

/// Opaque handle to a solid in the geometry kernel.
/// NEVER persisted. Valid only for the current kernel session.
/// Obtained from kernel operations, passed back to kernel for queries/mutations.
///
/// Producer: kernel-fork
/// Consumer: modeling-ops, feature-engine (runtime only)
#[derive(Debug, Clone)]
pub struct KernelSolidHandle(pub(crate) u64);

/// Transient kernel-internal entity identifier.
/// Derived from truck's VertexID/EdgeID/FaceID (Arc-pointer-based).
/// Stable within a single kernel session but NOT across rebuilds or serialization.
/// NEVER persisted — use GeomRef for persistent references.
///
/// Producer: kernel-fork (via KernelIntrospect)
/// Consumer: modeling-ops (for topology diffing), feature-engine (for resolution, runtime only)
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub struct KernelId(pub u64);

/// The kind of topological entity.
///
/// Producer: kernel-fork
/// Consumer: all crates that work with topology
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, serde::Serialize, serde::Deserialize)]
#[serde(tag = "type")]
pub enum TopoKind {
    Vertex,
    Edge,
    Face,
    Shell,
    Solid,
}

/// Persistent geometry reference. The core of the persistent naming system.
/// A GeomRef identifies a specific topological entity across parametric rebuilds.
///
/// Resolution algorithm:
/// 1. Find the anchor feature's current OpResult.
/// 2. Apply the selector to find matching KernelId(s).
/// 3. If Role-based selector works → use it (fast, stable).
/// 4. If Role fails → fall back to Signature matching.
/// 5. Apply ResolvePolicy (Strict = fail on ambiguity, BestEffort = closest match + warn).
///
/// Producer: feature-engine (when user selects geometry for an operation)
/// Consumer: feature-engine (during rebuild), modeling-ops (as operation inputs), file-format (persisted)
#[derive(Debug, Clone, serde::Serialize, serde::Deserialize)]
pub struct GeomRef {
    /// What kind of topological entity this references.
    pub kind: TopoKind,
    /// Which feature's output contains this entity.
    pub anchor: Anchor,
    /// How to find the specific entity within the anchor's output.
    pub selector: Selector,
    /// What to do when resolution is ambiguous or fails.
    pub policy: ResolvePolicy,
}

/// Identifies which feature output contains the target entity.
///
/// Producer: feature-engine
/// Consumer: feature-engine (during rebuild)
#[derive(Debug, Clone, serde::Serialize, serde::Deserialize)]
#[serde(tag = "type")]
pub enum Anchor {
    /// References an output of a specific feature in the tree.
    FeatureOutput {
        /// UUID of the feature that produced this geometry.
        feature_id: uuid::Uuid,
        /// Which output of the feature (most features have one "Main" output).
        output_key: OutputKey,
    },
    /// References a datum (construction plane, axis, or point).
    Datum {
        /// UUID of the datum.
        datum_id: uuid::Uuid,
    },
}

/// Identifies which output of a feature to look in.
/// Most features produce a single "Main" body, but some produce multiple outputs.
///
/// Producer: modeling-ops (in OpResult)
/// Consumer: feature-engine
#[derive(Debug, Clone, PartialEq, Eq, Hash, serde::Serialize, serde::Deserialize)]
#[serde(tag = "type")]
pub enum OutputKey {
    /// The primary solid body output.
    Main,
    /// A secondary body (e.g., from boolean split).
    Body { index: usize },
    /// A sketch profile (closed loop suitable for extrusion).
    Profile { index: usize },
    /// A datum plane/axis/point output.
    Datum { name: String },
}

/// How to find a specific entity within a feature's output.
/// Tried in order: Role first (fast), then Signature (robust fallback), then Query.
///
/// Producer: feature-engine
/// Consumer: feature-engine (during rebuild)
#[derive(Debug, Clone, serde::Serialize, serde::Deserialize)]
#[serde(tag = "type")]
pub enum Selector {
    /// Select by semantic role assigned during the operation.
    /// Fast and stable as long as the operation's topology doesn't change.
    Role {
        role: Role,
        /// Index within entities sharing this role (e.g., SideFace index 2 of 4).
        index: usize,
    },

    /// Select by geometric signature matching.
    /// Used as fallback when role-based selection fails due to topology changes.
    Signature {
        signature: TopoSignature,
    },

    /// Select by user-specified geometric query.
    /// For advanced selections like "face with largest area" or "face nearest point."
    Query {
        query: TopoQuery,
    },
}

/// What to do when GeomRef resolution is ambiguous or fails.
///
/// Producer: feature-engine
/// Consumer: feature-engine (during rebuild)
#[derive(Debug, Clone, Copy, PartialEq, Eq, serde::Serialize, serde::Deserialize)]
#[serde(tag = "type")]
pub enum ResolvePolicy {
    /// Fail the rebuild if the reference cannot be uniquely resolved.
    Strict,
    /// Use the closest match and emit a warning. Prefer this for interactive use.
    BestEffort,
}

/// Semantic role assigned to topological entities by modeling operations.
/// Roles provide stable, meaningful names for geometry that survive topology changes.
///
/// Producer: modeling-ops (in Provenance)
/// Consumer: feature-engine (for role-based selection)
#[derive(Debug, Clone, PartialEq, Eq, Hash, serde::Serialize, serde::Deserialize)]
#[serde(tag = "type")]
pub enum Role {
    /// The face on the positive extrusion direction end.
    EndCapPositive,
    /// The face on the negative extrusion direction end (original sketch plane face).
    EndCapNegative,
    /// A lateral face created by sweeping a profile edge.
    SideFace { index: usize },
    /// The face at the start of a revolution.
    RevStartFace,
    /// The face at the end of a revolution (if not full 360).
    RevEndFace,
    /// A face created by a fillet operation.
    FilletFace { index: usize },
    /// A face created by a chamfer operation.
    ChamferFace { index: usize },
    /// An inner face created by a shell operation.
    ShellInnerFace { index: usize },
    /// The original profile face (sketch plane) of an extrude/revolve.
    ProfileFace,
    /// An instance in a pattern operation.
    PatternInstance { index: usize },
    /// A face from the first body in a boolean operation.
    BooleanBodyAFace { index: usize },
    /// A face from the second body in a boolean operation.
    BooleanBodyBFace { index: usize },
}

/// Geometric signature of a topological entity.
/// Used for signature-based matching when role-based resolution fails.
/// All fields are optional — matching uses available fields with weighted scoring.
///
/// Producer: kernel-fork (via KernelIntrospect::compute_signature)
/// Consumer: feature-engine (for signature-based GeomRef resolution)
#[derive(Debug, Clone, serde::Serialize, serde::Deserialize)]
pub struct TopoSignature {
    /// Surface type (planar, cylindrical, conical, spherical, toroidal, nurbs).
    pub surface_type: Option<String>,
    /// Surface area (for faces).
    pub area: Option<f64>,
    /// Centroid position [x, y, z].
    pub centroid: Option<[f64; 3]>,
    /// Outward-pointing normal at centroid (for faces).
    pub normal: Option<[f64; 3]>,
    /// Axis-aligned bounding box [min_x, min_y, min_z, max_x, max_y, max_z].
    pub bbox: Option<[f64; 6]>,
    /// Hash of the adjacency structure (which other entities are neighbors).
    /// Provides topological context beyond pure geometry.
    pub adjacency_hash: Option<u64>,
    /// Edge length (for edges).
    pub length: Option<f64>,
}

/// User-specified geometric query for selecting entities.
///
/// Producer: feature-engine (from user interaction)
/// Consumer: feature-engine (during rebuild)
#[derive(Debug, Clone, serde::Serialize, serde::Deserialize)]
pub struct TopoQuery {
    /// Filters to narrow down candidate entities.
    pub filters: Vec<Filter>,
    /// How to break ties if multiple entities match.
    pub tie_break: Option<TieBreak>,
}

/// Filter predicate for TopoQuery.
#[derive(Debug, Clone, serde::Serialize, serde::Deserialize)]
#[serde(tag = "type")]
pub enum Filter {
    /// Entity's surface/curve type must match.
    SurfaceType { surface_type: String },
    /// Entity's normal must be within `tolerance` radians of `direction`.
    NormalDirection { direction: [f64; 3], tolerance: f64 },
    /// Entity must be within `distance` of `point`.
    NearPoint { point: [f64; 3], distance: f64 },
    /// Entity's area must be in range [min, max].
    AreaRange { min: f64, max: f64 },
}

/// Tie-breaking strategy when multiple entities match a query.
#[derive(Debug, Clone, serde::Serialize, serde::Deserialize)]
#[serde(tag = "type")]
pub enum TieBreak {
    /// Pick the entity with the largest area.
    LargestArea,
    /// Pick the entity nearest to the given point.
    NearestTo { point: [f64; 3] },
    /// Pick the entity with the smallest index (arbitrary but deterministic).
    SmallestIndex,
}
```

---

## B) Operation Result Contract

**Producer:** modeling-ops
**Consumer:** feature-engine

Every modeling operation returns an `OpResult` containing the geometry output, complete provenance for persistent naming, and diagnostics.

```rust
/// Complete result of a modeling operation.
/// Contains everything feature-engine needs to update the model state
/// and maintain persistent naming.
///
/// Producer: modeling-ops
/// Consumer: feature-engine
#[derive(Debug, Clone)]
pub struct OpResult {
    /// The output bodies produced by this operation.
    pub outputs: Vec<(OutputKey, BodyOutput)>,
    /// Provenance: what entities were created, deleted, and modified.
    /// This is the foundation of persistent naming.
    pub provenance: Provenance,
    /// Non-fatal warnings and timing information.
    pub diagnostics: Diagnostics,
}

/// A body output from an operation, with optional pre-computed mesh.
///
/// Producer: modeling-ops
/// Consumer: feature-engine, wasm-bridge (for mesh transfer)
#[derive(Debug, Clone)]
pub struct BodyOutput {
    /// Handle to the solid in the kernel. Runtime-only, not persisted.
    pub handle: KernelSolidHandle,
    /// Pre-tessellated mesh, if available. Avoids redundant tessellation.
    pub mesh: Option<RenderMesh>,
}

/// Provenance tracking: what happened to topology during an operation.
/// This is the data that makes persistent naming work.
///
/// Producer: modeling-ops (by diffing before/after topology via KernelIntrospect)
/// Consumer: feature-engine (for GeomRef resolution and role assignment)
#[derive(Debug, Clone)]
pub struct Provenance {
    /// Entities that exist in the result but not in the input.
    pub created: Vec<EntityRecord>,
    /// Entities that existed in the input but not in the result.
    pub deleted: Vec<EntityRecord>,
    /// Entities that changed between input and result.
    pub modified: Vec<Rewrite>,
    /// Semantic role assignments for created/surviving entities.
    pub role_assignments: Vec<(KernelId, Role)>,
}

/// Record of a topological entity with its kernel ID and signature.
///
/// Producer: modeling-ops
/// Consumer: feature-engine
#[derive(Debug, Clone)]
pub struct EntityRecord {
    /// The kernel-internal ID. Runtime-only.
    pub kernel_id: KernelId,
    /// What kind of entity (Vertex, Edge, Face).
    pub kind: TopoKind,
    /// Geometric signature for fallback matching.
    pub signature: TopoSignature,
}

/// Record of a topological entity that was modified by an operation.
///
/// Producer: modeling-ops
/// Consumer: feature-engine (to update runtime ID mappings)
#[derive(Debug, Clone)]
pub struct Rewrite {
    /// The entity's ID before the operation.
    pub before: KernelId,
    /// The entity's ID after the operation.
    pub after: KernelId,
    /// Why the entity was modified.
    pub reason: RewriteReason,
}

/// Why a topological entity was modified during an operation.
#[derive(Debug, Clone, PartialEq, Eq, serde::Serialize, serde::Deserialize)]
#[serde(tag = "type")]
pub enum RewriteReason {
    /// Face/edge was trimmed by an intersecting operation.
    Trimmed,
    /// Edge was split into multiple edges.
    Split,
    /// Multiple entities were merged into one.
    Merged,
    /// Entity was moved/transformed but retains identity.
    Moved,
}

/// Non-fatal diagnostics from an operation.
///
/// Producer: modeling-ops
/// Consumer: feature-engine, ui-chrome (for display)
#[derive(Debug, Clone, Default)]
pub struct Diagnostics {
    /// Warning messages (e.g., "boolean nearly failed, tolerance was loosened").
    pub warnings: Vec<String>,
    /// Time taken for the kernel operation, in milliseconds.
    pub kernel_time_ms: f64,
    /// Time taken for tessellation, in milliseconds.
    pub tessellation_time_ms: f64,
}
```

---

## C) Sketch System Types

**Producer:** sketch-solver, sketch-ui
**Consumer:** sketch-solver, sketch-ui, feature-engine, wasm-bridge

```rust
/// A 2D sketch on a plane. Contains geometric entities and constraints.
/// The sketch is the input to the constraint solver.
///
/// Producer: sketch-ui (user draws entities and adds constraints)
/// Consumer: sketch-solver (solves), feature-engine (stores in feature tree)
/// Serde: persisted as part of the Sketch operation in the feature tree.
#[derive(Debug, Clone, serde::Serialize, serde::Deserialize)]
pub struct Sketch {
    /// Unique identifier for this sketch.
    pub id: uuid::Uuid,
    /// The plane this sketch lies on, referenced via GeomRef.
    /// Can be a face of an existing solid or a datum plane.
    pub plane: GeomRef,
    /// Geometric entities in this sketch.
    pub entities: Vec<SketchEntity>,
    /// Constraints between entities.
    pub constraints: Vec<SketchConstraint>,
    /// Current solve status (updated after each solve).
    pub solve_status: SolveStatus,
}

/// A geometric entity in a sketch.
/// Each entity has a unique ID (u32) for referencing in constraints.
///
/// Producer: sketch-ui
/// Consumer: sketch-solver, feature-engine
#[derive(Debug, Clone, serde::Serialize, serde::Deserialize)]
#[serde(tag = "type")]
pub enum SketchEntity {
    Point {
        id: u32,
        x: f64,
        y: f64,
        /// Construction geometry is not included in profiles.
        construction: bool,
    },
    Line {
        id: u32,
        start_id: u32,
        end_id: u32,
        construction: bool,
    },
    Circle {
        id: u32,
        center_id: u32,
        radius: f64,
        construction: bool,
    },
    Arc {
        id: u32,
        center_id: u32,
        start_id: u32,
        end_id: u32,
        construction: bool,
    },
}

/// A constraint between sketch entities.
/// Maps directly to libslvs constraint types via the slvs crate.
///
/// Producer: sketch-ui (user applies constraints)
/// Consumer: sketch-solver (passed to libslvs)
#[derive(Debug, Clone, serde::Serialize, serde::Deserialize)]
#[serde(tag = "type")]
pub enum SketchConstraint {
    /// Two points occupy the same position.
    Coincident { point_a: u32, point_b: u32 },
    /// A line or two points are horizontal (parallel to sketch X axis).
    Horizontal { entity: u32 },
    /// A line or two points are vertical (parallel to sketch Y axis).
    Vertical { entity: u32 },
    /// Two lines are parallel.
    Parallel { line_a: u32, line_b: u32 },
    /// Two lines are perpendicular.
    Perpendicular { line_a: u32, line_b: u32 },
    /// A line is tangent to a curve (arc or circle).
    Tangent { line: u32, curve: u32 },
    /// Two entities have equal length/radius.
    Equal { entity_a: u32, entity_b: u32 },
    /// Two entities are symmetric about a line.
    Symmetric { entity_a: u32, entity_b: u32, symmetry_line: u32 },
    /// Two points are symmetric about the horizontal axis.
    SymmetricH { point_a: u32, point_b: u32 },
    /// Two points are symmetric about the vertical axis.
    SymmetricV { point_a: u32, point_b: u32 },
    /// A point lies at the midpoint of a line.
    Midpoint { point: u32, line: u32 },
    /// Distance between two points or a point and a line.
    Distance { entity_a: u32, entity_b: u32, value: f64 },
    /// Angle between two lines.
    Angle { line_a: u32, line_b: u32, value_degrees: f64 },
    /// Radius of a circle or arc.
    Radius { entity: u32, value: f64 },
    /// Diameter of a circle or arc.
    Diameter { entity: u32, value: f64 },
    /// A point lies on an entity (line, circle, or arc).
    OnEntity { point: u32, entity: u32 },
    /// Solver should keep this point as close to its current position as possible.
    /// Used for interactive drag-to-constrain workflows.
    Dragged { point: u32 },
    /// Two angles are equal.
    EqualAngle { line_a: u32, line_b: u32, line_c: u32, line_d: u32 },
    /// Two lengths have a fixed ratio.
    Ratio { entity_a: u32, entity_b: u32, value: f64 },
    /// Distance from a point to a line equals distance between two other points.
    EqualPointToLine { point_a: u32, point_b: u32, line: u32 },
    /// Two normals/orientations are the same.
    SameOrientation { entity_a: u32, entity_b: u32 },
}

/// Result of running the constraint solver.
///
/// Producer: sketch-solver
/// Consumer: sketch-ui (for visual feedback), feature-engine
#[derive(Debug, Clone, serde::Serialize, serde::Deserialize)]
#[serde(tag = "type")]
pub enum SolveStatus {
    /// All constraints satisfied, zero degrees of freedom.
    FullyConstrained,
    /// All constraints satisfied, but geometry can still move.
    UnderConstrained {
        /// Remaining degrees of freedom.
        dof: u32,
    },
    /// Constraints are contradictory.
    OverConstrained {
        /// IDs of the conflicting constraints.
        conflicts: Vec<u32>,
    },
    /// Solver failed to converge.
    SolveFailed {
        /// Human-readable reason.
        reason: String,
    },
}

/// Output of the constraint solver: solved positions and extracted profiles.
///
/// Producer: sketch-solver
/// Consumer: feature-engine (profiles for extrusion), sketch-ui (display positions)
#[derive(Debug, Clone)]
pub struct SolvedSketch {
    /// Solved positions for all points. Key is point entity ID.
    pub positions: std::collections::HashMap<u32, (f64, f64)>,
    /// Closed profiles extracted from the solved geometry.
    /// Each profile is a list of entity IDs forming a closed loop.
    pub profiles: Vec<ClosedProfile>,
    /// Solve status.
    pub status: SolveStatus,
}

/// A closed loop of sketch entities suitable for extrusion or revolution.
///
/// Producer: sketch-solver (profile extraction)
/// Consumer: feature-engine (passed to modeling-ops for extrude/revolve)
#[derive(Debug, Clone, serde::Serialize, serde::Deserialize)]
pub struct ClosedProfile {
    /// Ordered entity IDs forming the closed loop.
    pub entity_ids: Vec<u32>,
    /// Whether the profile winds counter-clockwise (outward) or clockwise (hole).
    pub is_outer: bool,
}
```

---

## D) Feature Tree Types

**Producer:** feature-engine
**Consumer:** feature-engine, ui-chrome, file-format

```rust
/// The ordered list of modeling features. The core data structure of parametric CAD.
/// Features are replayed in order during rebuild. The active_index controls rollback.
///
/// Producer: feature-engine
/// Consumer: ui-chrome (display), file-format (persistence)
#[derive(Debug, Clone, serde::Serialize, serde::Deserialize)]
pub struct FeatureTree {
    /// Ordered list of features. Index 0 is the first feature.
    pub features: Vec<Feature>,
    /// Features after this index are suppressed during rebuild.
    /// None means all features are active.
    pub active_index: Option<usize>,
}

/// A single feature in the parametric feature tree.
///
/// Producer: feature-engine
/// Consumer: ui-chrome, file-format
#[derive(Debug, Clone, serde::Serialize, serde::Deserialize)]
pub struct Feature {
    /// Unique identifier.
    pub id: uuid::Uuid,
    /// User-visible name (e.g., "Extrude 1", "Sketch 2").
    pub name: String,
    /// The modeling operation this feature performs.
    pub operation: Operation,
    /// Whether this feature is suppressed (skipped during rebuild).
    pub suppressed: bool,
    /// GeomRefs to geometry that this feature depends on.
    /// Used for rebuild dependency tracking.
    pub references: Vec<GeomRef>,
}

/// A parametric modeling operation with its parameters.
/// Each variant stores all parameters needed to replay the operation.
///
/// Producer: feature-engine (from user actions)
/// Consumer: modeling-ops (executes the operation), file-format (persistence)
#[derive(Debug, Clone, serde::Serialize, serde::Deserialize)]
#[serde(tag = "type")]
pub enum Operation {
    /// A 2D sketch on a plane.
    Sketch {
        sketch: Sketch,
    },

    /// Linear extrusion of a sketch profile.
    Extrude {
        params: ExtrudeParams,
    },

    /// Revolution of a sketch profile around an axis.
    Revolve {
        params: RevolveParams,
    },

    /// Fillet (round) edges.
    Fillet {
        params: FilletParams,
    },

    /// Chamfer (bevel) edges.
    Chamfer {
        params: ChamferParams,
    },

    /// Shell (hollow out) a solid by removing faces.
    Shell {
        params: ShellParams,
    },

    /// Mirror geometry across a plane.
    Mirror {
        params: MirrorParams,
    },

    /// Linear pattern (repeat along a direction).
    LinearPattern {
        params: LinearPatternParams,
    },

    /// Circular pattern (repeat around an axis).
    CircularPattern {
        params: CircularPatternParams,
    },

    /// Boolean combination of two bodies.
    BooleanCombine {
        params: BooleanParams,
    },
}

/// Parameters for an extrude operation.
#[derive(Debug, Clone, serde::Serialize, serde::Deserialize)]
pub struct ExtrudeParams {
    /// The sketch containing the profile to extrude.
    pub sketch_id: uuid::Uuid,
    /// Index of the profile within the sketch's solved profiles.
    pub profile_index: usize,
    /// Extrusion depth (positive = along face normal).
    pub depth: f64,
    /// Direction override. None = face normal.
    pub direction: Option<[f64; 3]>,
    /// Extrude symmetrically in both directions.
    pub symmetric: bool,
    /// Whether to subtract this extrusion from an existing body.
    pub cut: bool,
    /// The body to cut from (if cut is true).
    pub target_body: Option<GeomRef>,
}

/// Parameters for a revolve operation.
#[derive(Debug, Clone, serde::Serialize, serde::Deserialize)]
pub struct RevolveParams {
    /// The sketch containing the profile to revolve.
    pub sketch_id: uuid::Uuid,
    /// Index of the profile within the sketch's solved profiles.
    pub profile_index: usize,
    /// Axis of revolution: origin point.
    pub axis_origin: [f64; 3],
    /// Axis of revolution: direction vector (normalized).
    pub axis_direction: [f64; 3],
    /// Angle of revolution in radians. 2*PI = full revolution.
    pub angle: f64,
}

/// Parameters for a fillet operation.
#[derive(Debug, Clone, serde::Serialize, serde::Deserialize)]
pub struct FilletParams {
    /// Edges to fillet, referenced via GeomRef.
    pub edges: Vec<GeomRef>,
    /// Fillet radius.
    pub radius: f64,
}

/// Parameters for a chamfer operation.
#[derive(Debug, Clone, serde::Serialize, serde::Deserialize)]
pub struct ChamferParams {
    /// Edges to chamfer, referenced via GeomRef.
    pub edges: Vec<GeomRef>,
    /// Chamfer distance.
    pub distance: f64,
}

/// Parameters for a shell operation.
#[derive(Debug, Clone, serde::Serialize, serde::Deserialize)]
pub struct ShellParams {
    /// Faces to remove (openings), referenced via GeomRef.
    pub faces_to_remove: Vec<GeomRef>,
    /// Shell wall thickness.
    pub thickness: f64,
}

/// Parameters for a mirror operation.
#[derive(Debug, Clone, serde::Serialize, serde::Deserialize)]
pub struct MirrorParams {
    /// The body or features to mirror.
    pub source: GeomRef,
    /// The mirror plane, referenced via GeomRef.
    pub mirror_plane: GeomRef,
    /// Whether to keep the original (true) or only produce the mirror.
    pub keep_original: bool,
}

/// Parameters for a linear pattern operation.
#[derive(Debug, Clone, serde::Serialize, serde::Deserialize)]
pub struct LinearPatternParams {
    /// The body or features to pattern.
    pub source: GeomRef,
    /// Pattern direction vector.
    pub direction: [f64; 3],
    /// Number of instances (including the original).
    pub count: usize,
    /// Spacing between instances.
    pub spacing: f64,
}

/// Parameters for a circular pattern operation.
#[derive(Debug, Clone, serde::Serialize, serde::Deserialize)]
pub struct CircularPatternParams {
    /// The body or features to pattern.
    pub source: GeomRef,
    /// Axis of rotation: origin point.
    pub axis_origin: [f64; 3],
    /// Axis of rotation: direction vector.
    pub axis_direction: [f64; 3],
    /// Number of instances (including the original).
    pub count: usize,
    /// Total angle span in radians. 2*PI = full circle.
    pub total_angle: f64,
}

/// Parameters for a boolean combine operation.
#[derive(Debug, Clone, serde::Serialize, serde::Deserialize)]
pub struct BooleanParams {
    /// The target body.
    pub body_a: GeomRef,
    /// The tool body.
    pub body_b: GeomRef,
    /// Boolean operation type.
    pub operation: BooleanOp,
}

/// Boolean operation type.
#[derive(Debug, Clone, Copy, PartialEq, Eq, serde::Serialize, serde::Deserialize)]
#[serde(tag = "type")]
pub enum BooleanOp {
    Union,
    Subtract,
    Intersect,
}
```

---

## E) WASM Bridge Protocol

**Producer:** ui-chrome, sketch-ui (UiToEngine); feature-engine, sketch-solver (EngineToUi)
**Consumer:** wasm-bridge (serializes/deserializes both directions)

```rust
/// Messages from the UI (JavaScript main thread) to the engine (WASM Worker).
/// Serialized as JSON for postMessage transfer.
///
/// Producer: ui-chrome, sketch-ui (via JS)
/// Consumer: wasm-bridge (dispatches to engine crates)
#[derive(Debug, Clone, serde::Serialize, serde::Deserialize)]
#[serde(tag = "type")]
pub enum UiToEngine {
    // -- Sketch operations --
    /// Enter sketch mode on a face or datum plane.
    BeginSketch { plane: GeomRef },
    /// Add a geometric entity to the active sketch.
    AddSketchEntity { entity: SketchEntity },
    /// Add a constraint to the active sketch.
    AddConstraint { constraint: SketchConstraint },
    /// Run the constraint solver on the active sketch.
    SolveSketch,
    /// Exit sketch mode and commit the sketch as a feature.
    FinishSketch,

    // -- Feature operations --
    /// Add a new feature to the feature tree.
    AddFeature { operation: Operation },
    /// Edit an existing feature's parameters.
    EditFeature { feature_id: uuid::Uuid, operation: Operation },
    /// Delete a feature from the tree.
    DeleteFeature { feature_id: uuid::Uuid },
    /// Suppress/unsuppress a feature.
    SuppressFeature { feature_id: uuid::Uuid, suppressed: bool },
    /// Set the rollback index (features after this index are suppressed).
    SetRollbackIndex { index: Option<usize> },

    // -- History --
    Undo,
    Redo,

    // -- Selection --
    /// User selected an entity in the viewport.
    SelectEntity { geom_ref: GeomRef },
    /// User is hovering over an entity in the viewport.
    HoverEntity { geom_ref: Option<GeomRef> },

    // -- File operations --
    SaveProject,
    LoadProject { data: String },
    ExportStep,
}

/// Messages from the engine (WASM Worker) to the UI (JavaScript main thread).
/// ModelUpdated includes mesh data transferred as TypedArray views (not serialized as JSON).
///
/// Producer: feature-engine, sketch-solver (via wasm-bridge)
/// Consumer: ui-chrome, sketch-ui, 3d-viewport (via JS)
#[derive(Debug, Clone, serde::Serialize, serde::Deserialize)]
#[serde(tag = "type")]
pub enum EngineToUi {
    /// The model has been rebuilt. Includes updated meshes and feature tree state.
    ModelUpdated {
        /// Updated feature tree for display.
        feature_tree: FeatureTree,
        /// Meshes for each visible body. Transferred as TypedArray views.
        meshes: Vec<RenderMesh>,
        /// Edge overlay data for each visible body.
        edges: Vec<EdgeRenderData>,
    },

    /// Sketch constraint solver completed.
    SketchSolved {
        /// Solved positions and profiles.
        solved: SolvedSketch,
    },

    /// The hovered entity changed.
    HoverChanged {
        geom_ref: Option<GeomRef>,
    },

    /// The selection changed.
    SelectionChanged {
        geom_refs: Vec<GeomRef>,
    },

    /// An error occurred in the engine.
    Error {
        /// Human-readable error message.
        message: String,
        /// Which feature caused the error (if applicable).
        feature_id: Option<uuid::Uuid>,
    },

    /// STEP export is ready.
    ExportReady {
        /// STEP file contents as a string.
        step_data: String,
    },
}
```

---

## F) Render Mesh Types

**Producer:** kernel-fork (tessellation)
**Consumer:** wasm-bridge (transfer), 3d-viewport (rendering), sketch-ui (picking)

```rust
/// Tessellated triangle mesh for rendering in three.js.
/// Vertex/normal/index data is transferred as TypedArray views into WASM memory
/// for near-zero-copy performance.
///
/// Producer: kernel-fork (tessellation)
/// Consumer: wasm-bridge → 3d-viewport (three.js rendering)
#[derive(Debug, Clone, serde::Serialize, serde::Deserialize)]
pub struct RenderMesh {
    /// Flat array of vertex positions [x0, y0, z0, x1, y1, z1, ...].
    pub vertices: Vec<f32>,
    /// Flat array of vertex normals [nx0, ny0, nz0, nx1, ny1, nz1, ...].
    pub normals: Vec<f32>,
    /// Triangle indices into the vertex array.
    pub indices: Vec<u32>,
    /// Mapping from triangle ranges to logical faces.
    /// Enables GPU picking: given a triangle index from raycasting,
    /// binary-search face_ranges to find the owning GeomRef.
    pub face_ranges: Vec<FaceRange>,
}

/// Maps a contiguous range of triangles to a logical face.
/// face_ranges are sorted by start_index and non-overlapping.
///
/// Producer: kernel-fork (during tessellation)
/// Consumer: 3d-viewport (for picking), feature-engine (for selection)
#[derive(Debug, Clone, serde::Serialize, serde::Deserialize)]
pub struct FaceRange {
    /// Persistent reference to the logical face this range belongs to.
    pub geom_ref: GeomRef,
    /// Start index in the indices array (inclusive).
    pub start_index: u32,
    /// End index in the indices array (exclusive).
    pub end_index: u32,
}

/// Sharp edge data for rendering edge overlays in three.js.
/// Displayed as line segments on top of the shaded mesh.
///
/// Producer: kernel-fork (edge extraction)
/// Consumer: wasm-bridge → 3d-viewport (three.js line rendering)
#[derive(Debug, Clone, serde::Serialize, serde::Deserialize)]
pub struct EdgeRenderData {
    /// Flat array of edge vertex positions [x0, y0, z0, x1, y1, z1, ...].
    /// Each pair of vertices forms one line segment.
    pub vertices: Vec<f32>,
    /// Mapping from vertex ranges to logical edges.
    pub edge_ranges: Vec<EdgeRange>,
}

/// Maps a contiguous range of edge line-segment vertices to a logical edge.
///
/// Producer: kernel-fork
/// Consumer: 3d-viewport (for edge picking)
#[derive(Debug, Clone, serde::Serialize, serde::Deserialize)]
pub struct EdgeRange {
    /// Persistent reference to the logical edge.
    pub geom_ref: GeomRef,
    /// Start index in the vertices array (in floats, not vertices).
    pub start_vertex: u32,
    /// End index in the vertices array.
    pub end_vertex: u32,
}
```

---

## G) Kernel Abstraction Traits

**Implementor:** kernel-fork (TruckKernel, MockKernel)
**Consumer:** modeling-ops, feature-engine

```rust
/// Core geometry kernel trait. Provides all shape construction and modification operations.
/// Implemented by TruckKernel (wraps real truck) and MockKernel (deterministic test double).
///
/// All methods take &mut self because kernel operations mutate internal state.
/// Methods return Result to handle kernel failures gracefully.
///
/// Implementor: kernel-fork
/// Consumer: modeling-ops
pub trait Kernel {
    /// Extrude a planar face along a direction vector.
    /// Returns the resulting solid handle.
    fn extrude_face(
        &mut self,
        face: KernelId,
        direction: [f64; 3],
        depth: f64,
    ) -> Result<KernelSolidHandle, KernelError>;

    /// Revolve a planar face around an axis.
    /// angle is in radians. 2*PI for full revolution.
    fn revolve_face(
        &mut self,
        face: KernelId,
        axis_origin: [f64; 3],
        axis_direction: [f64; 3],
        angle: f64,
    ) -> Result<KernelSolidHandle, KernelError>;

    /// Boolean union of two solids.
    fn boolean_union(
        &mut self,
        a: &KernelSolidHandle,
        b: &KernelSolidHandle,
    ) -> Result<KernelSolidHandle, KernelError>;

    /// Boolean subtraction: a minus b.
    fn boolean_subtract(
        &mut self,
        a: &KernelSolidHandle,
        b: &KernelSolidHandle,
    ) -> Result<KernelSolidHandle, KernelError>;

    /// Boolean intersection of two solids.
    fn boolean_intersect(
        &mut self,
        a: &KernelSolidHandle,
        b: &KernelSolidHandle,
    ) -> Result<KernelSolidHandle, KernelError>;

    /// Fillet (round) the specified edges with the given radius.
    fn fillet_edges(
        &mut self,
        solid: &KernelSolidHandle,
        edges: &[KernelId],
        radius: f64,
    ) -> Result<KernelSolidHandle, KernelError>;

    /// Chamfer (bevel) the specified edges with the given distance.
    fn chamfer_edges(
        &mut self,
        solid: &KernelSolidHandle,
        edges: &[KernelId],
        distance: f64,
    ) -> Result<KernelSolidHandle, KernelError>;

    /// Shell a solid by removing faces and offsetting remaining faces inward.
    fn shell(
        &mut self,
        solid: &KernelSolidHandle,
        faces_to_remove: &[KernelId],
        thickness: f64,
    ) -> Result<KernelSolidHandle, KernelError>;

    /// Tessellate a solid to a triangle mesh.
    /// tolerance controls chordal deviation from true surface.
    fn tessellate(
        &mut self,
        solid: &KernelSolidHandle,
        tolerance: f64,
    ) -> Result<RenderMesh, KernelError>;

    /// Create a solid from closed sketch profiles on a plane.
    /// Returns face IDs for the created planar faces.
    fn make_faces_from_profiles(
        &mut self,
        profiles: &[ClosedProfile],
        plane_origin: [f64; 3],
        plane_normal: [f64; 3],
        plane_x_axis: [f64; 3],
        positions: &std::collections::HashMap<u32, (f64, f64)>,
    ) -> Result<Vec<KernelId>, KernelError>;
}

/// Topology introspection trait. Provides read-only queries on kernel geometry.
///
/// Implementor: kernel-fork
/// Consumer: modeling-ops (for topology diffing and provenance), feature-engine (for GeomRef resolution)
pub trait KernelIntrospect {
    /// List all faces of a solid.
    fn list_faces(&self, solid: &KernelSolidHandle) -> Vec<KernelId>;

    /// List all edges of a solid.
    fn list_edges(&self, solid: &KernelSolidHandle) -> Vec<KernelId>;

    /// List all vertices of a solid.
    fn list_vertices(&self, solid: &KernelSolidHandle) -> Vec<KernelId>;

    /// Get the edges bounding a face.
    fn face_edges(&self, face: KernelId) -> Vec<KernelId>;

    /// Get the faces adjacent to an edge.
    fn edge_faces(&self, edge: KernelId) -> Vec<KernelId>;

    /// Get the vertices at the ends of an edge.
    fn edge_vertices(&self, edge: KernelId) -> (KernelId, KernelId);

    /// Get the faces sharing an edge or vertex with the given face.
    fn face_neighbors(&self, face: KernelId) -> Vec<KernelId>;

    /// Compute the geometric signature of a single entity.
    fn compute_signature(&self, entity: KernelId, kind: TopoKind) -> TopoSignature;

    /// Compute signatures for all entities of a given kind in a solid.
    fn compute_all_signatures(
        &self,
        solid: &KernelSolidHandle,
        kind: TopoKind,
    ) -> Vec<(KernelId, TopoSignature)>;
}

/// Errors from kernel operations.
///
/// Producer: kernel-fork
/// Consumer: modeling-ops, feature-engine
#[derive(Debug, Clone, thiserror::Error)]
pub enum KernelError {
    /// Boolean operation failed (common with truck).
    #[error("boolean operation failed: {reason}")]
    BooleanFailed { reason: String },

    /// Fillet operation failed.
    #[error("fillet failed: {reason}")]
    FilletFailed { reason: String },

    /// Shell operation failed.
    #[error("shell failed: {reason}")]
    ShellFailed { reason: String },

    /// Tessellation failed.
    #[error("tessellation failed: {reason}")]
    TessellationFailed { reason: String },

    /// Invalid entity reference.
    #[error("entity not found: {id:?}")]
    EntityNotFound { id: KernelId },

    /// Operation not supported by this kernel implementation.
    #[error("operation not supported: {operation}")]
    NotSupported { operation: String },

    /// Generic kernel error.
    #[error("kernel error: {message}")]
    Other { message: String },
}
```
