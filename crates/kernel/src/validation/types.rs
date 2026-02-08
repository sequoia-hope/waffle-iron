//! Shared types for the B-Rep validation system.
//!
//! Defines error codes, severity levels, entity references, validation errors,
//! metrics, and the unified `ValidationReport`.

use std::fmt;

use crate::topology::brep::*;

/// Which validation levels to run.
#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord)]
pub enum ValidationLevel {
    /// Level 0-1: Topology only (Euler, twins, loops, normals, vertex-on-curve).
    Topology,
    /// Level 2: Geometric consistency (SameParameter, degenerate edges/faces).
    Geometry,
    /// Level 3: Spatial coherence (free edges, non-manifold, self-intersection).
    Spatial,
    /// All levels including continuity (G0/G1/G2).
    Full,
}

/// Severity of a validation finding.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Severity {
    /// Must be fixed for a valid solid.
    Error,
    /// Informational — may be intentional (e.g. sharp edges).
    Warning,
}

/// The kind of topological entity an error relates to.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum EntityType {
    Vertex,
    Edge,
    HalfEdge,
    Loop,
    Face,
    Shell,
    Solid,
}

/// A reference to a specific entity by its SlotMap key.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum EntityId {
    Vertex(VertexId),
    Edge(EdgeId),
    HalfEdge(HalfEdgeId),
    Loop(LoopId),
    Face(FaceId),
    Shell(ShellId),
    Solid(SolidId),
}

/// Enumeration of all validation error codes.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub enum ErrorCode {
    // --- Topology (Level 0-1) ---
    /// Vertex position does not lie on its edge's curve endpoint.
    InvalidPointOnCurve,
    /// Vertex position does not lie on an adjacent face's surface.
    InvalidPointOnSurface,
    /// Edge curve deviates from adjacent face surfaces (SameParameter check).
    SameParameterViolation,
    /// Edge has zero or near-zero length.
    ZeroLengthEdge,
    /// Edge is referenced by only one face (open boundary).
    FreeEdge,
    /// Edge is shared by more than two faces (non-manifold).
    InvalidMultiConnexity,
    /// A wire loop does not close (last vertex != first vertex).
    WireNotClosed,
    /// Half-edge traversal direction inconsistent with twin.
    InconsistentEdgeOrientation,
    /// Face has zero or near-zero area.
    ZeroAreaFace,
    /// Shell boundary is not closed (has free edges).
    ShellNotClosed,
    /// Face normals are not consistently oriented across the shell.
    BadOrientationOfFaces,
    /// Euler-Poincare formula V - E + F != 2 for a genus-0 shell.
    EulerPoincareViolation,
    /// Solid has negative volume (inverted normals).
    NegativeVolume,
    /// Geometry self-intersects.
    SelfIntersection,
    /// Tolerance hierarchy violated (vertex tol > edge tol, etc.).
    ToleranceHierarchyViolation,
    /// A tolerance value exceeds the maximum allowed.
    ExcessiveTolerance,
    /// G0 (positional) discontinuity across an edge.
    G0Discontinuity,
    /// G1 (tangent) discontinuity across an edge.
    G1Discontinuity,
    /// G2 (curvature) discontinuity across an edge.
    G2Discontinuity,
    /// Half-edge twin pointer does not point back to this half-edge.
    HalfEdgeTwinMismatch,
    /// A reference (vertex, edge, loop, face) points to a non-existent entity.
    DanglingReference,
    /// A face has no associated surface geometry.
    NoSurface,
    /// An edge has no 3D curve geometry.
    No3DCurve,
}

impl fmt::Display for ErrorCode {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        fmt::Debug::fmt(self, f)
    }
}

/// A single validation finding (error or warning).
#[derive(Debug, Clone)]
pub struct ValidationError {
    /// What kind of entity this error is about.
    pub entity_type: EntityType,
    /// Which specific entity.
    pub entity_id: EntityId,
    /// Parent entity (e.g. the face that owns a loop), if applicable.
    pub parent_id: Option<EntityId>,
    /// The error code classifying this issue.
    pub code: ErrorCode,
    /// Human-readable description.
    pub message: String,
    /// Severity: error or warning.
    pub severity: Severity,
    /// Measured numeric value (e.g. the gap distance for SameParameter).
    pub numeric_value: Option<f64>,
    /// The tolerance threshold that was exceeded.
    pub tolerance: Option<f64>,
}

impl fmt::Display for ValidationError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        let sev = match self.severity {
            Severity::Error => "ERROR",
            Severity::Warning => "WARN",
        };
        write!(f, "[{}] {:?} {:?}: {} (code: {})", sev, self.entity_type, self.entity_id, self.message, self.code)?;
        if let Some(val) = self.numeric_value {
            write!(f, " value={val:.2e}")?;
        }
        if let Some(tol) = self.tolerance {
            write!(f, " tol={tol:.2e}")?;
        }
        Ok(())
    }
}

/// Counts of topological entities in a solid.
#[derive(Debug, Clone, Copy, Default)]
pub struct EntityCounts {
    pub vertices: usize,
    pub edges: usize,
    pub half_edges: usize,
    pub faces: usize,
    pub shells: usize,
    pub loops: usize,
}

/// Statistics about tolerance values across the solid.
#[derive(Debug, Clone, Copy, Default)]
pub struct ToleranceStats {
    /// Maximum vertex tolerance encountered.
    pub max_vertex_tolerance: f64,
    /// Maximum gap between edge curve and adjacent face surface.
    pub max_edge_gap: f64,
    /// Mean gap between edge curve and adjacent face surface.
    pub mean_edge_gap: f64,
}

/// Aggregate metrics computed during validation.
#[derive(Debug, Clone, Default)]
pub struct ValidationMetrics {
    pub entity_counts: EntityCounts,
    pub tolerance_stats: ToleranceStats,
}

/// The unified validation report produced by `BRepValidator`.
#[derive(Debug, Clone)]
pub struct ValidationReport {
    /// Whether the solid passed all checks at the requested level.
    pub valid: bool,
    /// The highest level that was actually run.
    pub level_completed: ValidationLevel,
    /// All errors (severity = Error).
    pub errors: Vec<ValidationError>,
    /// All warnings (severity = Warning).
    pub warnings: Vec<ValidationError>,
    /// Computed metrics.
    pub metrics: ValidationMetrics,
}

impl ValidationReport {
    /// Filter errors by a specific error code.
    pub fn errors_of(&self, code: ErrorCode) -> Vec<&ValidationError> {
        self.errors.iter().filter(|e| e.code == code).collect()
    }

    /// Check that no errors of a specific code exist.
    pub fn no_errors_of(&self, code: ErrorCode) -> bool {
        !self.errors.iter().any(|e| e.code == code)
    }

    /// Total number of errors.
    pub fn error_count(&self) -> usize {
        self.errors.len()
    }

    /// Total number of warnings.
    pub fn warning_count(&self) -> usize {
        self.warnings.len()
    }
}

impl fmt::Display for ValidationReport {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        writeln!(f, "ValidationReport: valid={}, level={:?}, errors={}, warnings={}",
            self.valid, self.level_completed, self.errors.len(), self.warnings.len())?;
        for e in &self.errors {
            writeln!(f, "  {e}")?;
        }
        for w in &self.warnings {
            writeln!(f, "  {w}")?;
        }
        Ok(())
    }
}
