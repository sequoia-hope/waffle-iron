use serde::{Deserialize, Serialize};

// Re-export shared types from waffle-types
pub use waffle_types::{ClosedProfile, TopoKind, TopoSignature};

/// Opaque handle to a solid in the geometry kernel.
/// NEVER persisted. Valid only for the current kernel session.
#[derive(Debug, Clone)]
pub struct KernelSolidHandle(pub(crate) u64);

impl KernelSolidHandle {
    pub(crate) fn id(&self) -> u64 {
        self.0
    }
}

/// Transient kernel-internal entity identifier.
/// Stable within a single kernel session but NOT across rebuilds.
/// NEVER persisted — use GeomRef for persistent references.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub struct KernelId(pub u64);

/// Errors from kernel operations.
#[derive(Debug, Clone, thiserror::Error)]
pub enum KernelError {
    #[error("boolean operation failed: {reason}")]
    BooleanFailed { reason: String },

    #[error("fillet failed: {reason}")]
    FilletFailed { reason: String },

    #[error("shell failed: {reason}")]
    ShellFailed { reason: String },

    #[error("tessellation failed: {reason}")]
    TessellationFailed { reason: String },

    #[error("entity not found: {id:?}")]
    EntityNotFound { id: KernelId },

    #[error("operation not supported: {operation}")]
    NotSupported { operation: String },

    #[error("kernel error: {message}")]
    Other { message: String },
}

/// Tessellated triangle mesh for rendering in three.js.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct RenderMesh {
    /// Flat array of vertex positions [x0, y0, z0, x1, y1, z1, ...].
    pub vertices: Vec<f32>,
    /// Flat array of vertex normals [nx0, ny0, nz0, nx1, ny1, nz1, ...].
    pub normals: Vec<f32>,
    /// Triangle indices into the vertex array.
    pub indices: Vec<u32>,
    /// Mapping from triangle ranges to logical faces.
    pub face_ranges: Vec<FaceRange>,
}

/// Maps a contiguous range of triangles to a logical face.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct FaceRange {
    /// The KernelId of the face this range belongs to.
    pub face_id: KernelId,
    /// Start index in the indices array (inclusive).
    pub start_index: u32,
    /// End index in the indices array (exclusive).
    pub end_index: u32,
}

/// Sharp edge data for rendering edge overlays.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct EdgeRenderData {
    /// Flat array of edge vertex positions [x0, y0, z0, x1, y1, z1, ...].
    pub vertices: Vec<f32>,
    /// Mapping from vertex ranges to logical edges.
    pub edge_ranges: Vec<EdgeRange>,
}

/// Maps a contiguous range of edge line-segment vertices to a logical edge.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct EdgeRange {
    /// The KernelId of the edge this range belongs to.
    pub edge_id: KernelId,
    /// Start index in the vertices array (in floats, not vertices).
    pub start_vertex: u32,
    /// End index in the vertices array.
    pub end_vertex: u32,
}

// Custom Serialize/Deserialize for KernelId (needed for FaceRange/EdgeRange serialization)
impl Serialize for KernelId {
    fn serialize<S: serde::Serializer>(&self, serializer: S) -> Result<S::Ok, S::Error> {
        self.0.serialize(serializer)
    }
}

impl<'de> Deserialize<'de> for KernelId {
    fn deserialize<D: serde::Deserializer<'de>>(deserializer: D) -> Result<Self, D::Error> {
        u64::deserialize(deserializer).map(KernelId)
    }
}
