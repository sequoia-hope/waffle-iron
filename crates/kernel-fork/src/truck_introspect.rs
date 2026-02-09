//! TruckIntrospect — KernelIntrospect implementation wrapping truck topology queries.

use crate::traits::KernelIntrospect;
use crate::truck_kernel::TruckKernel;
use crate::types::*;

use truck_modeling::geometry::Surface;
use truck_modeling::topology::{Edge, Face, Solid, Vertex};

/// KernelIntrospect implementation that delegates to TruckKernel's stored solids.
pub struct TruckIntrospect<'a> {
    kernel: &'a TruckKernel,
}

impl<'a> TruckIntrospect<'a> {
    pub fn new(kernel: &'a TruckKernel) -> Self {
        Self { kernel }
    }
}

impl KernelIntrospect for TruckIntrospect<'_> {
    fn list_faces(&self, solid: &KernelSolidHandle) -> Vec<KernelId> {
        list_faces_impl(self.kernel.get_solid(solid), solid)
    }

    fn list_edges(&self, solid: &KernelSolidHandle) -> Vec<KernelId> {
        list_edges_impl(self.kernel.get_solid(solid), solid)
    }

    fn list_vertices(&self, solid: &KernelSolidHandle) -> Vec<KernelId> {
        list_vertices_impl(self.kernel.get_solid(solid), solid)
    }

    fn face_edges(&self, face: KernelId) -> Vec<KernelId> {
        face_edges_impl(face, |h| self.kernel.get_solid(h))
    }

    fn edge_faces(&self, edge: KernelId) -> Vec<KernelId> {
        edge_faces_impl(edge, |h| self.kernel.get_solid(h))
    }

    fn edge_vertices(&self, edge: KernelId) -> (KernelId, KernelId) {
        edge_vertices_impl(edge, |h| self.kernel.get_solid(h))
    }

    fn face_neighbors(&self, face: KernelId) -> Vec<KernelId> {
        face_neighbors_impl(face, |h| self.kernel.get_solid(h))
    }

    fn compute_signature(&self, entity: KernelId, kind: TopoKind) -> TopoSignature {
        compute_signature_impl(entity, kind, |h| self.kernel.get_solid(h))
    }

    fn compute_all_signatures(
        &self,
        solid: &KernelSolidHandle,
        kind: TopoKind,
    ) -> Vec<(KernelId, TopoSignature)> {
        compute_all_signatures_impl(self, solid, kind)
    }
}

/// Direct KernelIntrospect implementation on TruckKernel.
/// This allows TruckKernel to satisfy the KernelBundle blanket impl (Kernel + KernelIntrospect).
impl KernelIntrospect for TruckKernel {
    fn list_faces(&self, solid: &KernelSolidHandle) -> Vec<KernelId> {
        list_faces_impl(self.get_solid(solid), solid)
    }

    fn list_edges(&self, solid: &KernelSolidHandle) -> Vec<KernelId> {
        list_edges_impl(self.get_solid(solid), solid)
    }

    fn list_vertices(&self, solid: &KernelSolidHandle) -> Vec<KernelId> {
        list_vertices_impl(self.get_solid(solid), solid)
    }

    fn face_edges(&self, face: KernelId) -> Vec<KernelId> {
        face_edges_impl(face, |h| self.get_solid(h))
    }

    fn edge_faces(&self, edge: KernelId) -> Vec<KernelId> {
        edge_faces_impl(edge, |h| self.get_solid(h))
    }

    fn edge_vertices(&self, edge: KernelId) -> (KernelId, KernelId) {
        edge_vertices_impl(edge, |h| self.get_solid(h))
    }

    fn face_neighbors(&self, face: KernelId) -> Vec<KernelId> {
        face_neighbors_impl(face, |h| self.get_solid(h))
    }

    fn compute_signature(&self, entity: KernelId, kind: TopoKind) -> TopoSignature {
        compute_signature_impl(entity, kind, |h| self.get_solid(h))
    }

    fn compute_all_signatures(
        &self,
        solid: &KernelSolidHandle,
        kind: TopoKind,
    ) -> Vec<(KernelId, TopoSignature)> {
        compute_all_signatures_impl(self, solid, kind)
    }
}

// ── Shared implementation functions ─────────────────────────────────────

fn list_faces_impl(truck_solid: Option<&Solid>, solid: &KernelSolidHandle) -> Vec<KernelId> {
    let Some(truck_solid) = truck_solid else {
        return Vec::new();
    };

    let mut ids = Vec::new();
    for shell in truck_solid.boundaries().iter() {
        for (i, _face) in shell.face_iter().enumerate() {
            ids.push(KernelId(solid.id() * 10000 + i as u64));
        }
    }
    ids
}

fn list_edges_impl(truck_solid: Option<&Solid>, solid: &KernelSolidHandle) -> Vec<KernelId> {
    let Some(truck_solid) = truck_solid else {
        return Vec::new();
    };

    let mut seen = std::collections::HashSet::new();
    let mut ids = Vec::new();
    let mut idx = 0u64;
    for shell in truck_solid.boundaries().iter() {
        for edge in shell.edge_iter() {
            let eid = edge.id();
            if seen.insert(eid) {
                ids.push(KernelId(solid.id() * 10000 + 1000 + idx));
                idx += 1;
            }
        }
    }
    ids
}

fn list_vertices_impl(truck_solid: Option<&Solid>, solid: &KernelSolidHandle) -> Vec<KernelId> {
    let Some(truck_solid) = truck_solid else {
        return Vec::new();
    };

    let mut seen = std::collections::HashSet::new();
    let mut ids = Vec::new();
    let mut idx = 0u64;
    for shell in truck_solid.boundaries().iter() {
        for v in shell.vertex_iter() {
            let vid = v.id();
            if seen.insert(vid) {
                ids.push(KernelId(solid.id() * 10000 + 2000 + idx));
                idx += 1;
            }
        }
    }
    ids
}

fn face_edges_impl<'a, F>(face: KernelId, get_solid: F) -> Vec<KernelId>
where
    F: Fn(&KernelSolidHandle) -> Option<&'a Solid>,
{
    let handle_id = face.0 / 10000;
    let face_idx = (face.0 % 10000) as usize;

    let handle = KernelSolidHandle(handle_id);
    let Some(truck_solid) = get_solid(&handle) else {
        return Vec::new();
    };

    for shell in truck_solid.boundaries().iter() {
        let faces: Vec<_> = shell.face_iter().collect();
        if face_idx >= faces.len() {
            continue;
        }
        let target_face = &faces[face_idx];

        // Collect unique shell edges with their indices
        let mut edge_id_to_idx = std::collections::HashMap::new();
        let mut idx = 0u64;
        let mut seen = std::collections::HashSet::new();
        for edge in shell.edge_iter() {
            let eid = edge.id();
            if seen.insert(eid) {
                edge_id_to_idx.insert(eid, idx);
                idx += 1;
            }
        }

        let mut result = Vec::new();
        for wire in target_face.boundaries() {
            for edge in wire.edge_iter() {
                if let Some(&ei) = edge_id_to_idx.get(&edge.id()) {
                    result.push(KernelId(handle_id * 10000 + 1000 + ei));
                }
            }
        }
        return result;
    }
    Vec::new()
}

fn edge_faces_impl<'a, F>(edge: KernelId, get_solid: F) -> Vec<KernelId>
where
    F: Fn(&KernelSolidHandle) -> Option<&'a Solid>,
{
    let handle_id = edge.0 / 10000;
    let edge_offset = (edge.0 % 10000).saturating_sub(1000) as usize;

    let handle = KernelSolidHandle(handle_id);
    let Some(truck_solid) = get_solid(&handle) else {
        return Vec::new();
    };

    let mut result = Vec::new();
    for shell in truck_solid.boundaries().iter() {
        // Build edge index -> EdgeID mapping
        let mut edge_ids = Vec::new();
        let mut seen = std::collections::HashSet::new();
        for e in shell.edge_iter() {
            let eid = e.id();
            if seen.insert(eid) {
                edge_ids.push(eid);
            }
        }

        if edge_offset >= edge_ids.len() {
            continue;
        }
        let target_edge_id = edge_ids[edge_offset];

        for (fi, face) in shell.face_iter().enumerate() {
            let has_edge = face
                .boundaries()
                .iter()
                .flat_map(|w| w.edge_iter())
                .any(|e| e.id() == target_edge_id);

            if has_edge {
                result.push(KernelId(handle_id * 10000 + fi as u64));
            }
        }
    }
    result
}

fn edge_vertices_impl<'a, F>(edge: KernelId, get_solid: F) -> (KernelId, KernelId)
where
    F: Fn(&KernelSolidHandle) -> Option<&'a Solid>,
{
    let handle_id = edge.0 / 10000;
    let edge_offset = (edge.0 % 10000).saturating_sub(1000) as usize;

    let handle = KernelSolidHandle(handle_id);
    let Some(truck_solid) = get_solid(&handle) else {
        return (KernelId(0), KernelId(0));
    };

    for shell in truck_solid.boundaries().iter() {
        let mut edge_list = Vec::new();
        let mut seen_edges = std::collections::HashSet::new();
        for e in shell.edge_iter() {
            let eid = e.id();
            if seen_edges.insert(eid) {
                edge_list.push(e);
            }
        }

        if edge_offset >= edge_list.len() {
            continue;
        }

        let target_edge = &edge_list[edge_offset];
        let front_vid = target_edge.front().id();
        let back_vid = target_edge.back().id();

        // Build vertex index mapping
        let mut vert_id_to_idx = std::collections::HashMap::new();
        let mut seen_verts = std::collections::HashSet::new();
        let mut idx = 0u64;
        for v in shell.vertex_iter() {
            let vid = v.id();
            if seen_verts.insert(vid) {
                vert_id_to_idx.insert(vid, idx);
                idx += 1;
            }
        }

        let v1 = vert_id_to_idx
            .get(&front_vid)
            .map(|&i| KernelId(handle_id * 10000 + 2000 + i))
            .unwrap_or(KernelId(0));
        let v2 = vert_id_to_idx
            .get(&back_vid)
            .map(|&i| KernelId(handle_id * 10000 + 2000 + i))
            .unwrap_or(KernelId(0));

        return (v1, v2);
    }

    (KernelId(0), KernelId(0))
}

fn face_neighbors_impl<'a, F>(face: KernelId, get_solid: F) -> Vec<KernelId>
where
    F: Fn(&KernelSolidHandle) -> Option<&'a Solid>,
{
    let edge_ids = face_edges_impl(face, &get_solid);
    let mut neighbors = Vec::new();
    let mut seen = std::collections::HashSet::new();

    for eid in &edge_ids {
        let faces = edge_faces_impl(*eid, &get_solid);
        for fid in faces {
            if fid != face && seen.insert(fid) {
                neighbors.push(fid);
            }
        }
    }
    neighbors
}

fn compute_signature_impl<'a, F>(entity: KernelId, kind: TopoKind, get_solid: F) -> TopoSignature
where
    F: Fn(&KernelSolidHandle) -> Option<&'a Solid>,
{
    let handle_id = entity.0 / 10000;
    let handle = KernelSolidHandle(handle_id);

    let Some(truck_solid) = get_solid(&handle) else {
        return TopoSignature::empty();
    };

    match kind {
        TopoKind::Face => {
            let face_idx = (entity.0 % 10000) as usize;
            for shell in truck_solid.boundaries().iter() {
                let faces: Vec<_> = shell.face_iter().collect();
                if face_idx < faces.len() {
                    return compute_face_signature(faces[face_idx]);
                }
            }
        }
        TopoKind::Edge => {
            let edge_offset = (entity.0 % 10000).saturating_sub(1000) as usize;
            for shell in truck_solid.boundaries().iter() {
                let mut unique_edges = Vec::new();
                let mut seen = std::collections::HashSet::new();
                for e in shell.edge_iter() {
                    if seen.insert(e.id()) {
                        unique_edges.push(e);
                    }
                }
                if edge_offset < unique_edges.len() {
                    return compute_edge_signature(&unique_edges[edge_offset]);
                }
            }
        }
        TopoKind::Vertex => {
            let vert_offset = (entity.0 % 10000).saturating_sub(2000) as usize;
            for shell in truck_solid.boundaries().iter() {
                let mut unique_verts = Vec::new();
                let mut seen = std::collections::HashSet::new();
                for v in shell.vertex_iter() {
                    if seen.insert(v.id()) {
                        unique_verts.push(v);
                    }
                }
                if vert_offset < unique_verts.len() {
                    return compute_vertex_signature(&unique_verts[vert_offset]);
                }
            }
        }
        _ => {}
    }
    TopoSignature::empty()
}

fn compute_all_signatures_impl(
    introspect: &dyn KernelIntrospect,
    solid: &KernelSolidHandle,
    kind: TopoKind,
) -> Vec<(KernelId, TopoSignature)> {
    let ids = match kind {
        TopoKind::Face => introspect.list_faces(solid),
        TopoKind::Edge => introspect.list_edges(solid),
        TopoKind::Vertex => introspect.list_vertices(solid),
        _ => Vec::new(),
    };
    ids.into_iter()
        .map(|id| {
            let sig = introspect.compute_signature(id, kind);
            (id, sig)
        })
        .collect()
}

fn compute_face_signature(face: &Face) -> TopoSignature {
    let surface = face.oriented_surface();
    let surface_type = classify_surface(&surface);
    let (centroid, normal) = sample_face_center(face, &surface);

    TopoSignature {
        surface_type: Some(surface_type),
        area: None,
        centroid: Some(centroid),
        normal: Some(normal),
        bbox: None,
        adjacency_hash: None,
        length: None,
    }
}

fn compute_edge_signature(edge: &Edge) -> TopoSignature {
    let front = edge.front().point();
    let back = edge.back().point();

    let centroid = [
        (front[0] + back[0]) / 2.0,
        (front[1] + back[1]) / 2.0,
        (front[2] + back[2]) / 2.0,
    ];

    let dx = back[0] - front[0];
    let dy = back[1] - front[1];
    let dz = back[2] - front[2];
    let length = (dx * dx + dy * dy + dz * dz).sqrt();

    TopoSignature {
        surface_type: Some("line".to_string()),
        area: None,
        centroid: Some(centroid),
        normal: None,
        bbox: None,
        adjacency_hash: None,
        length: Some(length),
    }
}

fn compute_vertex_signature(vertex: &Vertex) -> TopoSignature {
    let p = vertex.point();
    TopoSignature {
        surface_type: Some("point".to_string()),
        area: None,
        centroid: Some([p[0], p[1], p[2]]),
        normal: None,
        bbox: None,
        adjacency_hash: None,
        length: None,
    }
}

fn classify_surface(surface: &Surface) -> String {
    match surface {
        Surface::Plane(_) => "planar".to_string(),
        Surface::RevolutedCurve(_) => "revolved".to_string(),
        Surface::BSplineSurface(_) => "nurbs".to_string(),
        Surface::NurbsSurface(_) => "nurbs".to_string(),
    }
}

fn sample_face_center(face: &Face, surface: &Surface) -> ([f64; 3], [f64; 3]) {
    match surface {
        Surface::Plane(plane) => {
            let p = plane.origin();
            let n = plane.normal();
            ([p[0], p[1], p[2]], [n[0], n[1], n[2]])
        }
        _ => {
            // For non-planar surfaces, compute centroid from vertex positions
            let mut cx = 0.0;
            let mut cy = 0.0;
            let mut cz = 0.0;
            let mut count = 0.0;
            for wire in face.boundaries() {
                for v in wire.vertex_iter() {
                    let p = v.point();
                    cx += p[0];
                    cy += p[1];
                    cz += p[2];
                    count += 1.0;
                }
            }
            if count > 0.0 {
                ([cx / count, cy / count, cz / count], [0.0, 0.0, 1.0])
            } else {
                ([0.0, 0.0, 0.0], [0.0, 0.0, 1.0])
            }
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::primitives;

    #[test]
    fn test_introspect_box_faces() {
        let mut kernel = TruckKernel::new();
        let solid = primitives::make_box(1.0, 1.0, 1.0);
        let handle = kernel.store_solid(solid);

        let introspect = TruckIntrospect::new(&kernel);
        let faces = introspect.list_faces(&handle);
        let edges = introspect.list_edges(&handle);
        let vertices = introspect.list_vertices(&handle);

        assert_eq!(faces.len(), 6, "Box should have 6 faces");
        assert_eq!(edges.len(), 12, "Box should have 12 edges");
        assert_eq!(vertices.len(), 8, "Box should have 8 vertices");
    }

    #[test]
    fn test_introspect_face_edges_box() {
        let mut kernel = TruckKernel::new();
        let solid = primitives::make_box(1.0, 1.0, 1.0);
        let handle = kernel.store_solid(solid);

        let introspect = TruckIntrospect::new(&kernel);
        let faces = introspect.list_faces(&handle);

        for face in &faces {
            let edges = introspect.face_edges(*face);
            assert_eq!(edges.len(), 4, "Each box face should have 4 edges");
        }
    }

    #[test]
    fn test_introspect_face_signature() {
        let mut kernel = TruckKernel::new();
        let solid = primitives::make_box(1.0, 1.0, 1.0);
        let handle = kernel.store_solid(solid);

        let introspect = TruckIntrospect::new(&kernel);
        let faces = introspect.list_faces(&handle);

        for face in &faces {
            let sig = introspect.compute_signature(*face, TopoKind::Face);
            assert_eq!(sig.surface_type.as_deref(), Some("planar"));
            assert!(sig.centroid.is_some());
            assert!(sig.normal.is_some());
        }
    }

    #[test]
    fn test_introspect_face_neighbors_box() {
        let mut kernel = TruckKernel::new();
        let solid = primitives::make_box(1.0, 1.0, 1.0);
        let handle = kernel.store_solid(solid);

        let introspect = TruckIntrospect::new(&kernel);
        let faces = introspect.list_faces(&handle);

        for face in &faces {
            let neighbors = introspect.face_neighbors(*face);
            assert_eq!(neighbors.len(), 4, "Each box face should have 4 neighbors");
        }
    }

    /// TruckKernel directly implements KernelIntrospect (no TruckIntrospect wrapper needed).
    /// This means TruckKernel satisfies the KernelBundle blanket impl.
    #[test]
    fn test_truck_kernel_direct_introspect() {
        let mut kernel = TruckKernel::new();
        let solid = primitives::make_box(2.0, 3.0, 4.0);
        let handle = kernel.store_solid(solid);

        // Use KernelIntrospect methods directly on TruckKernel
        let faces = kernel.list_faces(&handle);
        let edges = kernel.list_edges(&handle);
        let vertices = kernel.list_vertices(&handle);

        assert_eq!(faces.len(), 6);
        assert_eq!(edges.len(), 12);
        assert_eq!(vertices.len(), 8);

        // Verify signatures work directly
        for face in &faces {
            let sig = kernel.compute_signature(*face, TopoKind::Face);
            assert_eq!(sig.surface_type.as_deref(), Some("planar"));
        }
    }
}
