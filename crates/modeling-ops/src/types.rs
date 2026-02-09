use kernel_fork::{KernelId, KernelSolidHandle, RenderMesh};
use waffle_types::{OutputKey, Role, TopoKind, TopoSignature};

/// Complete result of a modeling operation.
/// Contains everything feature-engine needs to update the model state
/// and maintain persistent naming.
#[derive(Debug, Clone)]
pub struct OpResult {
    /// The output bodies produced by this operation.
    pub outputs: Vec<(OutputKey, BodyOutput)>,
    /// Provenance: what entities were created, deleted, and modified.
    pub provenance: Provenance,
    /// Non-fatal warnings and timing information.
    pub diagnostics: Diagnostics,
}

/// A body output from an operation, with optional pre-computed mesh.
#[derive(Debug, Clone)]
pub struct BodyOutput {
    /// Handle to the solid in the kernel. Runtime-only, not persisted.
    pub handle: KernelSolidHandle,
    /// Pre-tessellated mesh, if available.
    pub mesh: Option<RenderMesh>,
}

/// Provenance tracking: what happened to topology during an operation.
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
#[derive(Debug, Clone, Default)]
pub struct Diagnostics {
    /// Warning messages.
    pub warnings: Vec<String>,
    /// Time taken for the kernel operation, in milliseconds.
    pub kernel_time_ms: f64,
    /// Time taken for tessellation, in milliseconds.
    pub tessellation_time_ms: f64,
}

/// Errors from modeling operations.
#[derive(Debug, Clone, thiserror::Error)]
pub enum OpError {
    #[error("kernel error: {0}")]
    Kernel(#[from] kernel_fork::KernelError),

    #[error("no profiles available for operation")]
    NoProfiles,

    #[error("invalid parameter: {reason}")]
    InvalidParameter { reason: String },
}
