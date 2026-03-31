//! Bijective tessellation mapping — maps each mesh triangle to its source B-Rep face.
//!
//! This is stage 1 infrastructure for the Yang 2025 hybrid B-Rep/mesh boolean
//! pipeline [#24]. After exact mesh boolean (stage 2), the bijective map enables
//! topology extraction (stage 3): determining which original B-Rep faces survive
//! and how they are trimmed.
//!
//! Ref #24: Yang, Jia & Yan (2025) — Hybrid B-Rep/mesh boolean pipeline.
//! Ref #9: Cherchi et al. (2020) — Fast exact mesh arrangements.

use crate::topology::half_edge::FaceIdx;
use crate::types::RenderMesh;
use std::collections::BTreeMap;

/// Maps each mesh triangle to its source B-Rep face.
///
/// Invariant: `tri_face_ids.len() == mesh.indices.len() / 3` — every triangle
/// maps to exactly one face (bijective property).
#[derive(Debug, Clone)]
pub struct BijectiveMap {
    /// For triangle `i`, `tri_face_ids[i]` is the source B-Rep face index.
    pub tri_face_ids: Vec<FaceIdx>,
}

impl BijectiveMap {
    /// Derive a `BijectiveMap` from a `RenderMesh` and its `face_map`.
    ///
    /// Each `FaceRange` in the mesh tells us which triangle index range belongs
    /// to which `KernelId`. We invert the `face_map` (KernelId → FaceIdx) to
    /// recover the B-Rep `FaceIdx` for each triangle.
    ///
    /// Triangles whose `KernelId` is not found in `face_map` are mapped to
    /// `FaceIdx(usize::MAX)` as a sentinel (should not happen in correct usage).
    pub fn from_render_mesh(mesh: &RenderMesh, face_map: &BTreeMap<u64, FaceIdx>) -> Self {
        let tri_count = mesh.indices.len() / 3;
        let mut tri_face_ids = vec![FaceIdx(usize::MAX); tri_count];

        // Invert face_map: KernelId(u64) → FaceIdx
        let kid_to_face: BTreeMap<u64, FaceIdx> = face_map.clone();

        for range in &mesh.face_ranges {
            let face_idx = kid_to_face
                .get(&range.face_id.0)
                .copied()
                .unwrap_or(FaceIdx(usize::MAX));

            // FaceRange uses index offsets into the indices array.
            // Each triangle is 3 consecutive indices.
            let start_tri = range.start_index as usize / 3;
            let end_tri = range.end_index as usize / 3;

            let end = end_tri.min(tri_count);
            if start_tri < end {
                tri_face_ids[start_tri..end].fill(face_idx);
            }
        }

        BijectiveMap { tri_face_ids }
    }

    /// Number of triangles in this map.
    pub fn tri_count(&self) -> usize {
        self.tri_face_ids.len()
    }

    /// Check the bijective invariant: every triangle maps to a valid face
    /// (not the sentinel value).
    pub fn is_complete(&self) -> bool {
        self.tri_face_ids.iter().all(|f| f.0 != usize::MAX)
    }

    /// Return the set of distinct face indices referenced by this map.
    pub fn referenced_faces(&self) -> Vec<FaceIdx> {
        let mut seen = std::collections::BTreeSet::new();
        for &f in &self.tri_face_ids {
            if f.0 != usize::MAX {
                seen.insert(f);
            }
        }
        seen.into_iter().collect()
    }

    /// Count triangles belonging to a specific face.
    pub fn tri_count_for_face(&self, face: FaceIdx) -> usize {
        self.tri_face_ids.iter().filter(|&&f| f == face).count()
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::types::{FaceRange, KernelId};

    #[test]
    fn empty_mesh_produces_empty_map() {
        let mesh = RenderMesh {
            vertices: vec![],
            normals: vec![],
            indices: vec![],
            face_ranges: vec![],
        };
        let face_map = BTreeMap::new();
        let bmap = BijectiveMap::from_render_mesh(&mesh, &face_map);
        assert_eq!(bmap.tri_count(), 0);
        assert!(bmap.is_complete());
        assert!(bmap.referenced_faces().is_empty());
    }

    #[test]
    fn single_triangle_maps_to_face() {
        // One triangle (3 indices), one face
        let mesh = RenderMesh {
            vertices: vec![0.0, 0.0, 0.0, 1.0, 0.0, 0.0, 0.0, 1.0, 0.0],
            normals: vec![0.0, 0.0, 1.0, 0.0, 0.0, 1.0, 0.0, 0.0, 1.0],
            indices: vec![0, 1, 2],
            face_ranges: vec![FaceRange {
                face_id: KernelId(42),
                start_index: 0,
                end_index: 3,
            }],
        };
        let mut face_map = BTreeMap::new();
        face_map.insert(42, FaceIdx(7));

        let bmap = BijectiveMap::from_render_mesh(&mesh, &face_map);
        assert_eq!(bmap.tri_count(), 1);
        assert!(bmap.is_complete());
        assert_eq!(bmap.tri_face_ids[0], FaceIdx(7));
        assert_eq!(bmap.referenced_faces(), vec![FaceIdx(7)]);
        assert_eq!(bmap.tri_count_for_face(FaceIdx(7)), 1);
    }

    #[test]
    fn multiple_faces_map_correctly() {
        // 4 triangles across 2 faces: face A gets tri 0,1; face B gets tri 2,3
        let mesh = RenderMesh {
            vertices: vec![0.0; 5 * 3], // 5 vertices (enough for 4 triangles sharing verts)
            normals: vec![0.0; 5 * 3],
            indices: vec![0, 1, 2, 1, 2, 3, 0, 3, 4, 3, 4, 2], // 4 triangles = 12 indices
            face_ranges: vec![
                FaceRange {
                    face_id: KernelId(10),
                    start_index: 0,
                    end_index: 6, // tri 0, 1
                },
                FaceRange {
                    face_id: KernelId(20),
                    start_index: 6,
                    end_index: 12, // tri 2, 3
                },
            ],
        };
        let mut face_map = BTreeMap::new();
        face_map.insert(10, FaceIdx(0));
        face_map.insert(20, FaceIdx(1));

        let bmap = BijectiveMap::from_render_mesh(&mesh, &face_map);
        assert_eq!(bmap.tri_count(), 4);
        assert!(bmap.is_complete());
        assert_eq!(bmap.tri_face_ids[0], FaceIdx(0));
        assert_eq!(bmap.tri_face_ids[1], FaceIdx(0));
        assert_eq!(bmap.tri_face_ids[2], FaceIdx(1));
        assert_eq!(bmap.tri_face_ids[3], FaceIdx(1));
        assert_eq!(bmap.tri_count_for_face(FaceIdx(0)), 2);
        assert_eq!(bmap.tri_count_for_face(FaceIdx(1)), 2);

        let faces = bmap.referenced_faces();
        assert_eq!(faces.len(), 2);
    }

    // ── Integration tests with real kernel primitives ────────────

    use crate::traits::Kernel;
    use crate::waffle_kernel::WaffleKernel;
    use std::collections::HashMap;

    const XY_ORIGIN: [f64; 3] = [0.0, 0.0, 0.0];
    const XY_NORMAL: [f64; 3] = [0.0, 0.0, 1.0];
    const XY_X_AXIS: [f64; 3] = [1.0, 0.0, 0.0];
    const Z_DIR: [f64; 3] = [0.0, 0.0, 1.0];

    /// Build a face_map from a RenderMesh's face_ranges by assigning each
    /// unique KernelId a sequential FaceIdx. This mirrors what the kernel
    /// stores internally, letting us test BijectiveMap without accessing
    /// private kernel fields.
    fn face_map_from_mesh(mesh: &crate::types::RenderMesh) -> BTreeMap<u64, FaceIdx> {
        let mut map = BTreeMap::new();
        let mut next_idx = 0usize;
        for range in &mesh.face_ranges {
            map.entry(range.face_id.0).or_insert_with(|| {
                let idx = FaceIdx(next_idx);
                next_idx += 1;
                idx
            });
        }
        map
    }

    fn make_box_kernel(
        w: f64,
        h: f64,
        depth: f64,
    ) -> (WaffleKernel, crate::types::KernelSolidHandle) {
        use crate::types::ClosedProfile;
        let mut k = WaffleKernel::new();
        let mut positions = HashMap::new();
        positions.insert(1, (0.0, 0.0));
        positions.insert(2, (w, 0.0));
        positions.insert(3, (w, h));
        positions.insert(4, (0.0, h));
        let profile = ClosedProfile {
            entity_ids: vec![10, 11, 12, 13],
            is_outer: true,
            vertex_ids: vec![],
            circle: None,
            spline_segments: vec![],
            arc_segments: vec![],
        };
        let face_ids = k
            .make_faces_from_profiles(&[profile], XY_ORIGIN, XY_NORMAL, XY_X_AXIS, &positions)
            .expect("make_faces_from_profiles for box");
        let solid = k
            .extrude_face(face_ids[0], Z_DIR, depth)
            .expect("extrude_face for box");
        (k, solid)
    }

    fn make_cylinder_kernel(r: f64, depth: f64) -> (WaffleKernel, crate::types::KernelSolidHandle) {
        use crate::types::{CircleProfile, ClosedProfile};
        let mut k = WaffleKernel::new();
        let mut positions = HashMap::new();
        positions.insert(1, (0.0, 0.0));
        let profile = ClosedProfile {
            entity_ids: vec![1],
            is_outer: true,
            vertex_ids: vec![],
            circle: Some(CircleProfile {
                center_u: 0.0,
                center_v: 0.0,
                radius: r,
            }),
            spline_segments: vec![],
            arc_segments: vec![],
        };
        let face_ids = k
            .make_faces_from_profiles(&[profile], XY_ORIGIN, XY_NORMAL, XY_X_AXIS, &positions)
            .expect("make_faces_from_profiles for cylinder");
        let solid = k
            .extrude_face(face_ids[0], Z_DIR, depth)
            .expect("extrude_face for cylinder");
        (k, solid)
    }

    #[test]
    fn box_bijective_map_is_complete() {
        let (mut k, solid) = make_box_kernel(1.0, 1.0, 1.0);
        let mesh = k.tessellate(&solid, 0.1).expect("tessellate box");
        let face_map = face_map_from_mesh(&mesh);
        let bmap = BijectiveMap::from_render_mesh(&mesh, &face_map);

        assert!(
            bmap.is_complete(),
            "Box bijective map must be complete (no sentinel values)"
        );
    }

    #[test]
    fn box_bijective_map_references_six_faces() {
        let (mut k, solid) = make_box_kernel(2.0, 3.0, 4.0);
        let mesh = k.tessellate(&solid, 0.1).expect("tessellate box");
        let face_map = face_map_from_mesh(&mesh);
        let bmap = BijectiveMap::from_render_mesh(&mesh, &face_map);

        let faces = bmap.referenced_faces();
        assert_eq!(
            faces.len(),
            6,
            "Box should reference exactly 6 faces, got {}",
            faces.len()
        );
    }

    #[test]
    fn box_bijective_map_tri_count_matches_mesh() {
        let (mut k, solid) = make_box_kernel(1.0, 1.0, 1.0);
        let mesh = k.tessellate(&solid, 0.1).expect("tessellate box");
        let expected_tris = mesh.indices.len() / 3;
        let face_map = face_map_from_mesh(&mesh);
        let bmap = BijectiveMap::from_render_mesh(&mesh, &face_map);

        assert_eq!(
            bmap.tri_count(),
            expected_tris,
            "BijectiveMap tri_count ({}) must match mesh triangle count ({})",
            bmap.tri_count(),
            expected_tris
        );
    }

    #[test]
    fn box_every_face_has_at_least_two_triangles() {
        let (mut k, solid) = make_box_kernel(1.0, 1.0, 1.0);
        let mesh = k.tessellate(&solid, 0.1).expect("tessellate box");
        let face_map = face_map_from_mesh(&mesh);
        let bmap = BijectiveMap::from_render_mesh(&mesh, &face_map);

        for face in bmap.referenced_faces() {
            let count = bmap.tri_count_for_face(face);
            assert!(
                count >= 2,
                "Box face {:?} should have >= 2 triangles (quad -> 2 tris), got {}",
                face,
                count
            );
        }
    }

    #[test]
    fn box_triangle_sum_equals_total() {
        let (mut k, solid) = make_box_kernel(5.0, 3.0, 2.0);
        let mesh = k.tessellate(&solid, 0.1).expect("tessellate box");
        let face_map = face_map_from_mesh(&mesh);
        let bmap = BijectiveMap::from_render_mesh(&mesh, &face_map);

        let sum: usize = bmap
            .referenced_faces()
            .iter()
            .map(|&f| bmap.tri_count_for_face(f))
            .sum();
        assert_eq!(
            sum,
            bmap.tri_count(),
            "Sum of per-face triangle counts ({}) must equal total tri_count ({})",
            sum,
            bmap.tri_count()
        );
    }

    #[test]
    fn cylinder_bijective_map_is_complete() {
        let (mut k, solid) = make_cylinder_kernel(5.0, 10.0);
        let mesh = k.tessellate(&solid, 0.1).expect("tessellate cylinder");
        let face_map = face_map_from_mesh(&mesh);
        let bmap = BijectiveMap::from_render_mesh(&mesh, &face_map);

        assert!(
            bmap.is_complete(),
            "Cylinder bijective map must be complete (no sentinel values)"
        );
    }

    #[test]
    fn cylinder_bijective_map_references_three_faces() {
        let (mut k, solid) = make_cylinder_kernel(5.0, 10.0);
        let mesh = k.tessellate(&solid, 0.1).expect("tessellate cylinder");
        let face_map = face_map_from_mesh(&mesh);
        let bmap = BijectiveMap::from_render_mesh(&mesh, &face_map);

        let faces = bmap.referenced_faces();
        assert_eq!(
            faces.len(),
            3,
            "Cylinder should reference 3 faces (top cap, bottom cap, side), got {}",
            faces.len()
        );
    }

    #[test]
    fn cylinder_bijective_map_tri_count_matches_mesh() {
        let (mut k, solid) = make_cylinder_kernel(5.0, 10.0);
        let mesh = k.tessellate(&solid, 0.1).expect("tessellate cylinder");
        let expected_tris = mesh.indices.len() / 3;
        let face_map = face_map_from_mesh(&mesh);
        let bmap = BijectiveMap::from_render_mesh(&mesh, &face_map);

        assert_eq!(
            bmap.tri_count(),
            expected_tris,
            "Cylinder BijectiveMap tri_count ({}) must match mesh triangle count ({})",
            bmap.tri_count(),
            expected_tris
        );
    }

    #[test]
    fn cylinder_triangle_sum_equals_total() {
        let (mut k, solid) = make_cylinder_kernel(5.0, 10.0);
        let mesh = k.tessellate(&solid, 0.1).expect("tessellate cylinder");
        let face_map = face_map_from_mesh(&mesh);
        let bmap = BijectiveMap::from_render_mesh(&mesh, &face_map);

        let sum: usize = bmap
            .referenced_faces()
            .iter()
            .map(|&f| bmap.tri_count_for_face(f))
            .sum();
        assert_eq!(
            sum,
            bmap.tri_count(),
            "Sum of per-face triangle counts ({}) must equal total tri_count ({})",
            sum,
            bmap.tri_count()
        );
    }

    #[test]
    fn sphere_bijective_map_is_complete() {
        let mut k = WaffleKernel::new();
        let solid = k
            .make_sphere([0.0, 0.0, 0.0], 1.0)
            .expect("make_sphere should succeed");
        let mesh = k.tessellate(&solid, 0.01).expect("tessellate sphere");
        let face_map = face_map_from_mesh(&mesh);
        let bmap = BijectiveMap::from_render_mesh(&mesh, &face_map);

        assert!(
            bmap.is_complete(),
            "Sphere bijective map must be complete (no sentinel values)"
        );
    }

    #[test]
    fn sphere_bijective_map_tri_count_matches_mesh() {
        let mut k = WaffleKernel::new();
        let solid = k
            .make_sphere([0.0, 0.0, 0.0], 1.0)
            .expect("make_sphere should succeed");
        let mesh = k.tessellate(&solid, 0.01).expect("tessellate sphere");
        let expected_tris = mesh.indices.len() / 3;
        let face_map = face_map_from_mesh(&mesh);
        let bmap = BijectiveMap::from_render_mesh(&mesh, &face_map);

        assert_eq!(
            bmap.tri_count(),
            expected_tris,
            "Sphere BijectiveMap tri_count ({}) must match mesh triangle count ({})",
            bmap.tri_count(),
            expected_tris
        );
    }

    #[test]
    fn sphere_triangle_sum_equals_total() {
        let mut k = WaffleKernel::new();
        let solid = k
            .make_sphere([0.0, 0.0, 0.0], 1.0)
            .expect("make_sphere should succeed");
        let mesh = k.tessellate(&solid, 0.01).expect("tessellate sphere");
        let face_map = face_map_from_mesh(&mesh);
        let bmap = BijectiveMap::from_render_mesh(&mesh, &face_map);

        let sum: usize = bmap
            .referenced_faces()
            .iter()
            .map(|&f| bmap.tri_count_for_face(f))
            .sum();
        assert_eq!(
            sum,
            bmap.tri_count(),
            "Sum of per-face triangle counts ({}) must equal total tri_count ({})",
            sum,
            bmap.tri_count()
        );
    }

    #[test]
    fn missing_face_in_map_uses_sentinel() {
        let mesh = RenderMesh {
            vertices: vec![0.0; 3 * 3],
            normals: vec![0.0; 3 * 3],
            indices: vec![0, 1, 2],
            face_ranges: vec![FaceRange {
                face_id: KernelId(99),
                start_index: 0,
                end_index: 3,
            }],
        };
        // face_map does NOT contain KernelId(99)
        let face_map = BTreeMap::new();
        let bmap = BijectiveMap::from_render_mesh(&mesh, &face_map);
        assert_eq!(bmap.tri_count(), 1);
        assert!(!bmap.is_complete()); // sentinel present
    }
}
