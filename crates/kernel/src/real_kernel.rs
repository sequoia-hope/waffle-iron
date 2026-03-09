//! RealKernel — clean-sheet B-Rep kernel (stub implementation).
//!
//! All methods currently return NotSupported. Implementation will be built up
//! incrementally, tracked by the assay score (target: 400/400).

use crate::traits::{Kernel, KernelIntrospect};
use crate::types::*;
use std::collections::HashMap;

/// Clean-sheet geometry kernel. Currently a stub — all operations return NotSupported.
pub struct RealKernel {
    _next_id: u64,
}

impl RealKernel {
    pub fn new() -> Self {
        Self { _next_id: 1 }
    }
}

impl Default for RealKernel {
    fn default() -> Self {
        Self::new()
    }
}

impl Kernel for RealKernel {
    fn extrude_face(
        &mut self,
        _face: KernelId,
        _direction: [f64; 3],
        _depth: f64,
    ) -> Result<KernelSolidHandle, KernelError> {
        Err(KernelError::NotSupported {
            operation: "extrude_face".to_string(),
        })
    }

    fn revolve_face(
        &mut self,
        _face: KernelId,
        _axis_origin: [f64; 3],
        _axis_direction: [f64; 3],
        _angle: f64,
    ) -> Result<KernelSolidHandle, KernelError> {
        Err(KernelError::NotSupported {
            operation: "revolve_face".to_string(),
        })
    }

    fn boolean_union(
        &mut self,
        _a: &KernelSolidHandle,
        _b: &KernelSolidHandle,
    ) -> Result<KernelSolidHandle, KernelError> {
        Err(KernelError::NotSupported {
            operation: "boolean_union".to_string(),
        })
    }

    fn boolean_subtract(
        &mut self,
        _a: &KernelSolidHandle,
        _b: &KernelSolidHandle,
    ) -> Result<KernelSolidHandle, KernelError> {
        Err(KernelError::NotSupported {
            operation: "boolean_subtract".to_string(),
        })
    }

    fn boolean_intersect(
        &mut self,
        _a: &KernelSolidHandle,
        _b: &KernelSolidHandle,
    ) -> Result<KernelSolidHandle, KernelError> {
        Err(KernelError::NotSupported {
            operation: "boolean_intersect".to_string(),
        })
    }

    fn fillet_edges(
        &mut self,
        _solid: &KernelSolidHandle,
        _edges: &[KernelId],
        _radius: f64,
    ) -> Result<KernelSolidHandle, KernelError> {
        Err(KernelError::NotSupported {
            operation: "fillet_edges".to_string(),
        })
    }

    fn chamfer_edges(
        &mut self,
        _solid: &KernelSolidHandle,
        _edges: &[KernelId],
        _distance: f64,
    ) -> Result<KernelSolidHandle, KernelError> {
        Err(KernelError::NotSupported {
            operation: "chamfer_edges".to_string(),
        })
    }

    fn shell(
        &mut self,
        _solid: &KernelSolidHandle,
        _faces_to_remove: &[KernelId],
        _thickness: f64,
    ) -> Result<KernelSolidHandle, KernelError> {
        Err(KernelError::NotSupported {
            operation: "shell".to_string(),
        })
    }

    fn tessellate(
        &mut self,
        _solid: &KernelSolidHandle,
        _tolerance: f64,
    ) -> Result<RenderMesh, KernelError> {
        Err(KernelError::NotSupported {
            operation: "tessellate".to_string(),
        })
    }

    fn extract_edges(
        &mut self,
        _solid: &KernelSolidHandle,
        _tolerance: f64,
    ) -> Result<EdgeRenderData, KernelError> {
        Err(KernelError::NotSupported {
            operation: "extract_edges".to_string(),
        })
    }

    fn make_faces_from_profiles(
        &mut self,
        _profiles: &[ClosedProfile],
        _plane_origin: [f64; 3],
        _plane_normal: [f64; 3],
        _plane_x_axis: [f64; 3],
        _positions: &HashMap<u32, (f64, f64)>,
    ) -> Result<Vec<KernelId>, KernelError> {
        Err(KernelError::NotSupported {
            operation: "make_faces_from_profiles".to_string(),
        })
    }
}

impl KernelIntrospect for RealKernel {
    fn list_faces(&self, _solid: &KernelSolidHandle) -> Vec<KernelId> {
        vec![]
    }

    fn list_edges(&self, _solid: &KernelSolidHandle) -> Vec<KernelId> {
        vec![]
    }

    fn list_vertices(&self, _solid: &KernelSolidHandle) -> Vec<KernelId> {
        vec![]
    }

    fn face_edges(&self, _face: KernelId) -> Vec<KernelId> {
        vec![]
    }

    fn edge_faces(&self, _edge: KernelId) -> Vec<KernelId> {
        vec![]
    }

    fn edge_vertices(&self, _edge: KernelId) -> (KernelId, KernelId) {
        (KernelId(0), KernelId(0))
    }

    fn face_neighbors(&self, _face: KernelId) -> Vec<KernelId> {
        vec![]
    }

    fn compute_signature(&self, _entity: KernelId, _kind: TopoKind) -> TopoSignature {
        TopoSignature::empty()
    }

    fn compute_all_signatures(
        &self,
        _solid: &KernelSolidHandle,
        _kind: TopoKind,
    ) -> Vec<(KernelId, TopoSignature)> {
        vec![]
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn real_kernel_returns_not_supported() {
        let mut k = RealKernel::new();
        let handle = KernelSolidHandle(1);
        let id = KernelId(1);

        assert!(matches!(
            k.extrude_face(id, [0.0, 0.0, 1.0], 1.0),
            Err(KernelError::NotSupported { .. })
        ));
        assert!(matches!(
            k.boolean_union(&handle, &handle),
            Err(KernelError::NotSupported { .. })
        ));
        assert!(matches!(
            k.tessellate(&handle, 0.1),
            Err(KernelError::NotSupported { .. })
        ));
    }

    #[test]
    fn real_kernel_introspect_returns_empty() {
        let k = RealKernel::new();
        let handle = KernelSolidHandle(1);

        assert!(k.list_faces(&handle).is_empty());
        assert!(k.list_edges(&handle).is_empty());
        assert!(k.list_vertices(&handle).is_empty());
    }
}
