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

#[cfg(test)]
mod tests {
    use super::*;

    /// KernelError Display impls via thiserror produce expected messages.
    #[test]
    fn test_kernel_error_display() {
        let err = KernelError::BooleanFailed {
            reason: "test".to_string(),
        };
        assert_eq!(format!("{}", err), "boolean operation failed: test");

        let err = KernelError::FilletFailed {
            reason: "bad".to_string(),
        };
        assert_eq!(format!("{}", err), "fillet failed: bad");

        let err = KernelError::ShellFailed {
            reason: "thin".to_string(),
        };
        assert_eq!(format!("{}", err), "shell failed: thin");

        let err = KernelError::TessellationFailed {
            reason: "mesh".to_string(),
        };
        assert_eq!(format!("{}", err), "tessellation failed: mesh");

        let err = KernelError::EntityNotFound { id: KernelId(42) };
        assert_eq!(format!("{}", err), "entity not found: KernelId(42)");

        let err = KernelError::NotSupported {
            operation: "fillet".to_string(),
        };
        assert_eq!(format!("{}", err), "operation not supported: fillet");

        let err = KernelError::Other {
            message: "oops".to_string(),
        };
        assert_eq!(format!("{}", err), "kernel error: oops");
    }

    /// KernelSolidHandle Debug output.
    #[test]
    fn test_kernel_solid_handle_debug() {
        let handle = KernelSolidHandle(42);
        let debug = format!("{:?}", handle);
        assert!(debug.contains("42"), "Debug should contain the ID");
    }

    /// FaceRange Debug output contains face_id.
    #[test]
    fn test_face_range_debug() {
        let fr = FaceRange {
            face_id: KernelId(100),
            start_index: 0,
            end_index: 6,
        };
        let debug = format!("{:?}", fr);
        assert!(debug.contains("100"));
        assert!(debug.contains("0"));
        assert!(debug.contains("6"));
    }

    /// EdgeRange Debug output contains edge_id.
    #[test]
    fn test_edge_range_debug() {
        let er = EdgeRange {
            edge_id: KernelId(200),
            start_vertex: 0,
            end_vertex: 10,
        };
        let debug = format!("{:?}", er);
        assert!(debug.contains("200"));
    }

    /// RenderMesh can be constructed and has expected structure.
    #[test]
    fn test_render_mesh_construction() {
        let mesh = RenderMesh {
            vertices: vec![0.0, 1.0, 2.0, 3.0, 4.0, 5.0, 6.0, 7.0, 8.0],
            normals: vec![0.0, 0.0, 1.0, 0.0, 0.0, 1.0, 0.0, 0.0, 1.0],
            indices: vec![0, 1, 2],
            face_ranges: vec![FaceRange {
                face_id: KernelId(1),
                start_index: 0,
                end_index: 3,
            }],
        };
        assert_eq!(mesh.vertices.len(), 9);
        assert_eq!(mesh.normals.len(), 9);
        assert_eq!(mesh.indices.len(), 3);
        assert_eq!(mesh.face_ranges.len(), 1);
        assert_eq!(mesh.face_ranges[0].face_id, KernelId(1));
    }

    /// EdgeRenderData can be constructed and has expected structure.
    #[test]
    fn test_edge_render_data_construction() {
        let data = EdgeRenderData {
            vertices: vec![0.0, 0.0, 0.0, 1.0, 1.0, 1.0],
            edge_ranges: vec![EdgeRange {
                edge_id: KernelId(5),
                start_vertex: 0,
                end_vertex: 2,
            }],
        };
        assert_eq!(data.vertices.len(), 6);
        assert_eq!(data.edge_ranges.len(), 1);
        assert_eq!(data.edge_ranges[0].edge_id, KernelId(5));
    }

    /// KernelId Clone and Copy semantics.
    #[test]
    fn test_kernel_id_clone_copy() {
        let id = KernelId(7);
        let id2 = id; // Copy
        let id3 = id.clone(); // Clone
        assert_eq!(id, id2);
        assert_eq!(id, id3);
    }

    /// KernelId Hash produces consistent values.
    #[test]
    fn test_kernel_id_hash() {
        use std::collections::HashSet;
        let mut set = HashSet::new();
        set.insert(KernelId(1));
        set.insert(KernelId(2));
        set.insert(KernelId(1)); // duplicate
        assert_eq!(set.len(), 2);
    }

    /// KernelError Clone works correctly.
    #[test]
    fn test_kernel_error_clone() {
        let err = KernelError::Other {
            message: "test".to_string(),
        };
        let err2 = err.clone();
        assert_eq!(format!("{}", err), format!("{}", err2));
    }

    /// KernelSolidHandle id() accessor returns the inner value.
    #[test]
    fn test_kernel_solid_handle_id() {
        let handle = KernelSolidHandle(99);
        assert_eq!(handle.id(), 99);
    }
}
