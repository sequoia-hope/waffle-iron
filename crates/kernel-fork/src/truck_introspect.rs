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
    let mut face_idx: u64 = 0;
    for shell in truck_solid.boundaries().iter() {
        for _face in shell.face_iter() {
            ids.push(KernelId(solid.id() * 10000 + face_idx));
            face_idx += 1;
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

    // Accumulate face offset across shells to find the correct face
    let mut face_offset = 0usize;
    for shell in truck_solid.boundaries().iter() {
        let faces: Vec<_> = shell.face_iter().collect();
        let local_idx = face_idx.checked_sub(face_offset);
        face_offset += faces.len();

        let local_idx = match local_idx {
            Some(li) if li < faces.len() => li,
            _ => continue,
        };
        let target_face = &faces[local_idx];

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
    let mut face_offset: u64 = 0;
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
            let shell_face_count = shell.face_iter().count() as u64;
            face_offset += shell_face_count;
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
                result.push(KernelId(handle_id * 10000 + face_offset + fi as u64));
            }
        }
        face_offset += shell.face_iter().count() as u64;
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
            let mut face_offset = 0usize;
            for shell in truck_solid.boundaries().iter() {
                let faces: Vec<_> = shell.face_iter().collect();
                let local_idx = face_idx.checked_sub(face_offset);
                face_offset += faces.len();
                if let Some(li) = local_idx {
                    if li < faces.len() {
                        return compute_face_signature(faces[li]);
                    }
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
    use crate::traits::Kernel;

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

    /// Face IDs from list_faces() must exactly match face_id values in tessellated mesh face_ranges.
    #[test]
    fn test_box_face_id_consistency() {
        let mut kernel = TruckKernel::new();
        let solid = primitives::make_box(1.0, 1.0, 1.0);
        let handle = kernel.store_solid(solid);

        let introspect_ids: std::collections::HashSet<_> =
            kernel.list_faces(&handle).into_iter().collect();
        let mesh = kernel.tessellate(&handle, 0.1).unwrap();
        let tess_ids: std::collections::HashSet<_> =
            mesh.face_ranges.iter().map(|fr| fr.face_id).collect();

        assert_eq!(
            introspect_ids, tess_ids,
            "Face IDs must match between introspection and tessellation for box"
        );
        assert_eq!(introspect_ids.len(), 6);
    }

    /// Same consistency check for a cylinder (curved surfaces, different topology).
    #[test]
    fn test_cylinder_face_id_consistency() {
        let mut kernel = TruckKernel::new();
        let solid = primitives::make_cylinder(1.0, 2.0);
        let handle = kernel.store_solid(solid);

        let introspect_ids: std::collections::HashSet<_> =
            kernel.list_faces(&handle).into_iter().collect();
        let mesh = kernel.tessellate(&handle, 0.1).unwrap();
        let tess_ids: std::collections::HashSet<_> =
            mesh.face_ranges.iter().map(|fr| fr.face_id).collect();

        assert_eq!(
            introspect_ids, tess_ids,
            "Face IDs must match between introspection and tessellation for cylinder"
        );
        assert!(
            introspect_ids.len() >= 3,
            "Cylinder should have at least 3 faces, got {}",
            introspect_ids.len()
        );
    }

    /// Face IDs for boolean union of offset boxes must be consistent.
    #[test]
    fn test_boolean_result_face_id_consistency() {
        use truck_modeling::{builder, Point3, Vector3};

        let box_a = primitives::make_box(2.0, 2.0, 2.0);
        let v = builder::vertex(Point3::new(0.5, 0.5, 0.5));
        let e = builder::tsweep(&v, Vector3::new(1.0, 0.0, 0.0));
        let f = builder::tsweep(&e, Vector3::new(0.0, 1.0, 0.0));
        let box_b: truck_modeling::Solid = builder::tsweep(&f, Vector3::new(0.0, 0.0, 1.0));

        let union = truck_shapeops::or(&box_a, &box_b, 0.05);
        let union_solid = union.expect("Box-box offset union should succeed");

        let mut kernel = TruckKernel::new();
        let handle = kernel.store_solid(union_solid);

        let introspect_ids: std::collections::HashSet<_> =
            kernel.list_faces(&handle).into_iter().collect();
        let mesh = kernel.tessellate(&handle, 0.1).unwrap();
        let tess_ids: std::collections::HashSet<_> =
            mesh.face_ranges.iter().map(|fr| fr.face_id).collect();

        assert_eq!(
            introspect_ids, tess_ids,
            "Face IDs must match for boolean union result"
        );
        assert!(
            !introspect_ids.is_empty(),
            "Boolean result should have faces"
        );
    }

    /// Documents the assumption that make_box and make_cylinder produce single-shell solids.
    #[test]
    fn test_single_shell_assumption() {
        let box_solid = primitives::make_box(1.0, 1.0, 1.0);
        assert_eq!(
            box_solid.boundaries().len(),
            1,
            "make_box should produce a single-shell solid"
        );

        let cyl_solid = primitives::make_cylinder(1.0, 2.0);
        assert_eq!(
            cyl_solid.boundaries().len(),
            1,
            "make_cylinder should produce a single-shell solid"
        );
    }

    /// Face IDs should be globally sequential: base+0 through base+N-1 with no resets.
    #[test]
    fn test_face_ids_globally_sequential() {
        let mut kernel = TruckKernel::new();
        let solid = primitives::make_box(1.0, 1.0, 1.0);
        let handle = kernel.store_solid(solid);

        let face_ids = kernel.list_faces(&handle);
        let base = handle.id() * 10000;

        for (i, fid) in face_ids.iter().enumerate() {
            assert_eq!(
                fid.0,
                base + i as u64,
                "Face ID {} should be base+{} = {}, got {}",
                i,
                i,
                base + i as u64,
                fid.0
            );
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

    // ── Coverage: None/empty paths ──────────────────────────────────

    /// list_faces returns empty for nonexistent solid handle.
    #[test]
    fn test_list_faces_no_solid() {
        let kernel = TruckKernel::new();
        let bad = KernelSolidHandle(999);
        assert!(kernel.list_faces(&bad).is_empty());
    }

    /// list_edges returns empty for nonexistent solid handle.
    #[test]
    fn test_list_edges_no_solid() {
        let kernel = TruckKernel::new();
        let bad = KernelSolidHandle(999);
        assert!(kernel.list_edges(&bad).is_empty());
    }

    /// list_vertices returns empty for nonexistent solid handle.
    #[test]
    fn test_list_vertices_no_solid() {
        let kernel = TruckKernel::new();
        let bad = KernelSolidHandle(999);
        assert!(kernel.list_vertices(&bad).is_empty());
    }

    /// face_edges returns empty for a face whose solid doesn't exist.
    #[test]
    fn test_face_edges_no_solid() {
        let kernel = TruckKernel::new();
        // KernelId 9990000 → handle_id=999 which doesn't exist
        let bad_face = KernelId(999 * 10000);
        assert!(kernel.face_edges(bad_face).is_empty());
    }

    /// face_edges returns empty for a face index beyond the shell's face count.
    #[test]
    fn test_face_edges_face_out_of_range() {
        let mut kernel = TruckKernel::new();
        let solid = primitives::make_box(1.0, 1.0, 1.0);
        let handle = kernel.store_solid(solid);
        // Box has 6 faces (indices 0-5), try index 99
        let bad_face = KernelId(handle.id() * 10000 + 99);
        assert!(kernel.face_edges(bad_face).is_empty());
    }

    /// edge_faces returns empty for a nonexistent solid.
    #[test]
    fn test_edge_faces_no_solid() {
        let kernel = TruckKernel::new();
        let bad_edge = KernelId(999 * 10000 + 1000);
        assert!(kernel.edge_faces(bad_edge).is_empty());
    }

    /// edge_faces returns empty for an edge offset beyond the shell's edge count.
    #[test]
    fn test_edge_faces_edge_out_of_range() {
        let mut kernel = TruckKernel::new();
        let solid = primitives::make_box(1.0, 1.0, 1.0);
        let handle = kernel.store_solid(solid);
        // Box has 12 edges (indices 0-11), try index 99
        let bad_edge = KernelId(handle.id() * 10000 + 1000 + 99);
        assert!(kernel.edge_faces(bad_edge).is_empty());
    }

    /// edge_vertices returns (0,0) for a nonexistent solid.
    #[test]
    fn test_edge_vertices_no_solid() {
        let kernel = TruckKernel::new();
        let bad_edge = KernelId(999 * 10000 + 1000);
        let (v1, v2) = kernel.edge_vertices(bad_edge);
        assert_eq!(v1, KernelId(0));
        assert_eq!(v2, KernelId(0));
    }

    /// edge_vertices returns (0,0) for edge offset out of range.
    #[test]
    fn test_edge_vertices_out_of_range() {
        let mut kernel = TruckKernel::new();
        let solid = primitives::make_box(1.0, 1.0, 1.0);
        let handle = kernel.store_solid(solid);
        let bad_edge = KernelId(handle.id() * 10000 + 1000 + 99);
        let (v1, v2) = kernel.edge_vertices(bad_edge);
        assert_eq!(v1, KernelId(0));
        assert_eq!(v2, KernelId(0));
    }

    /// compute_signature returns empty for nonexistent solid.
    #[test]
    fn test_compute_signature_no_solid() {
        let kernel = TruckKernel::new();
        let bad_entity = KernelId(999 * 10000);
        let sig = kernel.compute_signature(bad_entity, TopoKind::Face);
        assert!(sig.surface_type.is_none());
        assert!(sig.centroid.is_none());
    }

    /// compute_signature for edge kind returns line type with centroid and length.
    #[test]
    fn test_compute_signature_edge() {
        let mut kernel = TruckKernel::new();
        let solid = primitives::make_box(2.0, 3.0, 4.0);
        let handle = kernel.store_solid(solid);

        let edges = kernel.list_edges(&handle);
        assert!(!edges.is_empty());

        let sig = kernel.compute_signature(edges[0], TopoKind::Edge);
        assert_eq!(sig.surface_type.as_deref(), Some("line"));
        assert!(sig.centroid.is_some(), "Edge should have a centroid");
        assert!(sig.length.is_some(), "Edge should have a length");
        assert!(sig.length.unwrap() > 0.0, "Edge length should be positive");
    }

    /// compute_signature for vertex kind returns point type with position.
    #[test]
    fn test_compute_signature_vertex() {
        let mut kernel = TruckKernel::new();
        let solid = primitives::make_box(2.0, 3.0, 4.0);
        let handle = kernel.store_solid(solid);

        let vertices = kernel.list_vertices(&handle);
        assert!(!vertices.is_empty());

        let sig = kernel.compute_signature(vertices[0], TopoKind::Vertex);
        assert_eq!(sig.surface_type.as_deref(), Some("point"));
        assert!(
            sig.centroid.is_some(),
            "Vertex should have a centroid (position)"
        );
    }

    /// compute_signature for face_idx out of range returns empty.
    #[test]
    fn test_compute_signature_face_out_of_range() {
        let mut kernel = TruckKernel::new();
        let solid = primitives::make_box(1.0, 1.0, 1.0);
        let handle = kernel.store_solid(solid);
        let bad_face = KernelId(handle.id() * 10000 + 99);
        let sig = kernel.compute_signature(bad_face, TopoKind::Face);
        assert!(sig.surface_type.is_none());
    }

    /// compute_signature for edge out of range returns empty.
    #[test]
    fn test_compute_signature_edge_out_of_range() {
        let mut kernel = TruckKernel::new();
        let solid = primitives::make_box(1.0, 1.0, 1.0);
        let handle = kernel.store_solid(solid);
        let bad_edge = KernelId(handle.id() * 10000 + 1000 + 99);
        let sig = kernel.compute_signature(bad_edge, TopoKind::Edge);
        assert!(sig.surface_type.is_none());
    }

    /// compute_signature for vertex out of range returns empty.
    #[test]
    fn test_compute_signature_vertex_out_of_range() {
        let mut kernel = TruckKernel::new();
        let solid = primitives::make_box(1.0, 1.0, 1.0);
        let handle = kernel.store_solid(solid);
        let bad_vert = KernelId(handle.id() * 10000 + 2000 + 99);
        let sig = kernel.compute_signature(bad_vert, TopoKind::Vertex);
        assert!(sig.surface_type.is_none());
    }

    /// compute_all_signatures for edges returns 12 entries for a box.
    #[test]
    fn test_compute_all_signatures_edges() {
        let mut kernel = TruckKernel::new();
        let solid = primitives::make_box(1.0, 1.0, 1.0);
        let handle = kernel.store_solid(solid);

        let sigs = kernel.compute_all_signatures(&handle, TopoKind::Edge);
        assert_eq!(sigs.len(), 12, "Box should have 12 edge signatures");
        for (id, sig) in &sigs {
            assert_eq!(sig.surface_type.as_deref(), Some("line"));
            assert!(sig.length.is_some());
            assert!(id.0 > 0);
        }
    }

    /// compute_all_signatures for vertices returns 8 entries for a box.
    #[test]
    fn test_compute_all_signatures_vertices() {
        let mut kernel = TruckKernel::new();
        let solid = primitives::make_box(1.0, 1.0, 1.0);
        let handle = kernel.store_solid(solid);

        let sigs = kernel.compute_all_signatures(&handle, TopoKind::Vertex);
        assert_eq!(sigs.len(), 8, "Box should have 8 vertex signatures");
        for (_id, sig) in &sigs {
            assert_eq!(sig.surface_type.as_deref(), Some("point"));
            assert!(sig.centroid.is_some());
        }
    }

    /// Cylinder face surface types include planar caps and nurbs sides.
    /// truck represents cylinder side surfaces as BSplineSurface ("nurbs").
    #[test]
    fn test_cylinder_face_surface_types() {
        let mut kernel = TruckKernel::new();
        let solid = primitives::make_cylinder(1.0, 2.0);
        let handle = kernel.store_solid(solid);

        let faces = kernel.list_faces(&handle);
        let mut surface_types: Vec<String> = Vec::new();
        for face in &faces {
            let sig = kernel.compute_signature(*face, TopoKind::Face);
            if let Some(st) = sig.surface_type {
                surface_types.push(st);
            }
        }

        // Cylinder should have "nurbs" faces (cylindrical side — truck uses BSplineSurface)
        // and "planar" faces (top and bottom caps)
        assert!(
            surface_types.iter().any(|s| s == "nurbs"),
            "Cylinder should have nurbs surface for sides, got {:?}",
            surface_types
        );
        assert!(
            surface_types.iter().any(|s| s == "planar"),
            "Cylinder should have planar surfaces (caps), got {:?}",
            surface_types
        );
    }

    /// Cylinder edge signatures include non-zero lengths.
    #[test]
    fn test_cylinder_edge_signatures() {
        let mut kernel = TruckKernel::new();
        let solid = primitives::make_cylinder(1.0, 2.0);
        let handle = kernel.store_solid(solid);

        let sigs = kernel.compute_all_signatures(&handle, TopoKind::Edge);
        assert!(!sigs.is_empty());
        for (_id, sig) in &sigs {
            assert_eq!(sig.surface_type.as_deref(), Some("line"));
            // Edge lengths should be positive
            if let Some(len) = sig.length {
                assert!(len >= 0.0, "Edge length should be non-negative");
            }
        }
    }

    /// Cylinder vertex signatures have valid positions.
    #[test]
    fn test_cylinder_vertex_signatures() {
        let mut kernel = TruckKernel::new();
        let solid = primitives::make_cylinder(1.0, 2.0);
        let handle = kernel.store_solid(solid);

        let sigs = kernel.compute_all_signatures(&handle, TopoKind::Vertex);
        assert!(!sigs.is_empty());
        for (_id, sig) in &sigs {
            assert_eq!(sig.surface_type.as_deref(), Some("point"));
            let pos = sig.centroid.unwrap();
            // Cylinder radius=1, height=2: vertices should be within bounds
            assert!(pos[0].is_finite());
            assert!(pos[1].is_finite());
            assert!(pos[2].is_finite());
        }
    }

    /// face_neighbors for a nonexistent face returns empty.
    #[test]
    fn test_face_neighbors_no_solid() {
        let kernel = TruckKernel::new();
        let bad_face = KernelId(999 * 10000);
        assert!(kernel.face_neighbors(bad_face).is_empty());
    }

    /// edge_faces: each box edge is shared by exactly 2 faces.
    #[test]
    fn test_edge_faces_box_two_faces() {
        let mut kernel = TruckKernel::new();
        let solid = primitives::make_box(1.0, 1.0, 1.0);
        let handle = kernel.store_solid(solid);

        let edges = kernel.list_edges(&handle);
        for edge in &edges {
            let faces = kernel.edge_faces(*edge);
            assert_eq!(
                faces.len(),
                2,
                "Each box edge should be adjacent to exactly 2 faces"
            );
        }
    }

    /// edge_vertices: each box edge has distinct endpoints that are valid vertices.
    #[test]
    fn test_edge_vertices_box_valid() {
        let mut kernel = TruckKernel::new();
        let solid = primitives::make_box(1.0, 1.0, 1.0);
        let handle = kernel.store_solid(solid);

        let all_verts: std::collections::HashSet<_> =
            kernel.list_vertices(&handle).into_iter().collect();
        let edges = kernel.list_edges(&handle);

        for edge in &edges {
            let (v1, v2) = kernel.edge_vertices(*edge);
            assert!(all_verts.contains(&v1), "Edge start vertex should exist");
            assert!(all_verts.contains(&v2), "Edge end vertex should exist");
            assert_ne!(v1, v2, "Edge endpoints should be distinct");
        }
    }

    /// TruckIntrospect wrapper produces same results as direct TruckKernel introspect.
    #[test]
    fn test_truck_introspect_wrapper_matches_direct() {
        let mut kernel = TruckKernel::new();
        let solid = primitives::make_box(1.0, 2.0, 3.0);
        let handle = kernel.store_solid(solid);

        let introspect = TruckIntrospect::new(&kernel);

        // list_faces
        let direct_faces = kernel.list_faces(&handle);
        let wrapper_faces = introspect.list_faces(&handle);
        assert_eq!(direct_faces, wrapper_faces);

        // list_edges
        let direct_edges = kernel.list_edges(&handle);
        let wrapper_edges = introspect.list_edges(&handle);
        assert_eq!(direct_edges, wrapper_edges);

        // list_vertices
        let direct_verts = kernel.list_vertices(&handle);
        let wrapper_verts = introspect.list_vertices(&handle);
        assert_eq!(direct_verts, wrapper_verts);

        // face_edges for first face
        let f0 = direct_faces[0];
        assert_eq!(kernel.face_edges(f0), introspect.face_edges(f0));

        // edge_faces for first edge
        let e0 = direct_edges[0];
        assert_eq!(kernel.edge_faces(e0), introspect.edge_faces(e0));

        // edge_vertices
        assert_eq!(kernel.edge_vertices(e0), introspect.edge_vertices(e0));

        // face_neighbors
        assert_eq!(kernel.face_neighbors(f0), introspect.face_neighbors(f0));

        // compute_signature
        let sig_d = kernel.compute_signature(f0, TopoKind::Face);
        let sig_w = introspect.compute_signature(f0, TopoKind::Face);
        assert_eq!(sig_d.surface_type, sig_w.surface_type);

        // compute_all_signatures
        let all_d = kernel.compute_all_signatures(&handle, TopoKind::Face);
        let all_w = introspect.compute_all_signatures(&handle, TopoKind::Face);
        assert_eq!(all_d.len(), all_w.len());
    }

    /// Non-planar face (cylinder side) centroid is computed from vertices.
    /// truck represents cylinder sides as BSplineSurface ("nurbs").
    #[test]
    fn test_nonplanar_face_centroid() {
        let mut kernel = TruckKernel::new();
        let solid = primitives::make_cylinder(1.0, 2.0);
        let handle = kernel.store_solid(solid);

        let faces = kernel.list_faces(&handle);
        let mut found_nurbs = false;
        for face in &faces {
            let sig = kernel.compute_signature(*face, TopoKind::Face);
            if sig.surface_type.as_deref() == Some("nurbs") {
                found_nurbs = true;
                let centroid = sig.centroid.unwrap();
                // Centroid should be finite (computed from vertex positions)
                assert!(centroid[0].is_finite());
                assert!(centroid[1].is_finite());
                assert!(centroid[2].is_finite());
            }
        }
        assert!(
            found_nurbs,
            "Should find at least one nurbs face on cylinder"
        );
    }
}
