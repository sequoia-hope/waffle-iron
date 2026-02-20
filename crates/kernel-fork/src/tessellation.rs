//! Tessellation wrapper with face-range metadata.
//!
//! Wraps truck-meshalgo to produce RenderMesh with FaceRange entries
//! that map triangle index ranges to logical faces for GPU picking.

use crate::types::*;
use truck_meshalgo::prelude::*;
use truck_meshalgo::tessellation::MeshableShape;

type TruckSolid = truck_modeling::Solid;

/// Tessellate a truck Solid into a RenderMesh with per-face tracking.
///
/// Each face is tessellated as part of the solid, then we iterate
/// the meshed faces to extract per-face triangle ranges.
pub fn tessellate_solid(
    solid: &TruckSolid,
    tolerance: f64,
    next_id: &mut u64,
    solid_handle: &KernelSolidHandle,
) -> std::result::Result<RenderMesh, KernelError> {
    let meshed_solid = solid.triangulation(tolerance);

    let mut all_vertices: Vec<f32> = Vec::new();
    let mut all_normals: Vec<f32> = Vec::new();
    let mut all_indices: Vec<u32> = Vec::new();
    let mut face_ranges: Vec<FaceRange> = Vec::new();

    // Iterate the meshed solid's shells and faces
    // Use the same face_idx counter as list_faces_impl in truck_introspect.rs
    // so that face IDs match between tessellation and introspection.
    let mut face_idx: u64 = 0;
    for shell in meshed_solid.boundaries().iter() {
        for face in shell.face_iter() {
            let face_id = KernelId(solid_handle.id() * 10000 + face_idx);
            face_idx += 1;

            // Each meshed face's surface is Option<PolygonMesh>
            let maybe_mesh: Option<PolygonMesh> = face.surface();
            let Some(face_mesh) = maybe_mesh else {
                continue;
            };

            // If face is inverted, the mesh needs inversion too
            let face_mesh = if !face.orientation() {
                let mut m = face_mesh;
                m.invert();
                m
            } else {
                face_mesh
            };

            let start_index = all_indices.len() as u32;
            let base_vertex = (all_vertices.len() / 3) as u32;

            let positions = face_mesh.positions();
            let normals = face_mesh.normals();
            let tri_faces = face_mesh.tri_faces();

            for pos in positions {
                all_vertices.push(pos[0] as f32);
                all_vertices.push(pos[1] as f32);
                all_vertices.push(pos[2] as f32);
            }

            if normals.is_empty() {
                for _ in 0..positions.len() {
                    all_normals.push(0.0);
                    all_normals.push(0.0);
                    all_normals.push(1.0);
                }
            } else {
                for norm in normals {
                    all_normals.push(norm[0] as f32);
                    all_normals.push(norm[1] as f32);
                    all_normals.push(norm[2] as f32);
                }
            }

            for tri in tri_faces {
                for v in tri.iter() {
                    all_indices.push(v.pos as u32 + base_vertex);
                }
            }

            let end_index = all_indices.len() as u32;
            if end_index > start_index {
                face_ranges.push(FaceRange {
                    face_id,
                    start_index,
                    end_index,
                });
            }
        }
    }

    // Fallback if nothing was tessellated
    if all_vertices.is_empty() {
        return tessellate_solid_merged(solid, tolerance, next_id, solid_handle);
    }

    let mut mesh = RenderMesh {
        vertices: all_vertices,
        normals: all_normals,
        indices: all_indices,
        face_ranges,
    };

    // truck's tsweep can produce inside-out solids when the sweep direction
    // is antiparallel to the face normal. Detect this via signed volume
    // (negative = inverted winding) and fix by flipping normals + winding.
    fix_inverted_mesh(&mut mesh);

    Ok(mesh)
}

/// If the mesh has negative signed volume (inside-out winding), flip all
/// normals and reverse triangle winding order so faces render correctly
/// with front-face culling.
fn fix_inverted_mesh(mesh: &mut RenderMesh) {
    let signed_vol = mesh_signed_volume(mesh);
    if signed_vol >= 0.0 {
        return;
    }

    // Flip all normals
    for n in mesh.normals.iter_mut() {
        *n = -*n;
    }

    // Reverse triangle winding (swap indices 1 and 2 in each triangle)
    for tri in mesh.indices.chunks_exact_mut(3) {
        tri.swap(1, 2);
    }
}

/// Compute the signed volume of a triangle mesh using the divergence theorem.
/// Positive = outward normals (correct), negative = inverted.
fn mesh_signed_volume(mesh: &RenderMesh) -> f64 {
    let verts = &mesh.vertices;
    let mut volume = 0.0f64;

    for tri in mesh.indices.chunks(3) {
        if tri.len() < 3 {
            continue;
        }
        let (i0, i1, i2) = (
            tri[0] as usize * 3,
            tri[1] as usize * 3,
            tri[2] as usize * 3,
        );
        if i0 + 2 >= verts.len() || i1 + 2 >= verts.len() || i2 + 2 >= verts.len() {
            continue;
        }

        let (x0, y0, z0) = (verts[i0] as f64, verts[i0 + 1] as f64, verts[i0 + 2] as f64);
        let (x1, y1, z1) = (verts[i1] as f64, verts[i1 + 1] as f64, verts[i1 + 2] as f64);
        let (x2, y2, z2) = (verts[i2] as f64, verts[i2 + 1] as f64, verts[i2 + 2] as f64);

        volume += x0 * (y1 * z2 - y2 * z1) + x1 * (y2 * z0 - y0 * z2) + x2 * (y0 * z1 - y1 * z0);
    }

    volume / 6.0
}

/// Extract edge polylines from a solid for rendering edge overlays.
///
/// Each edge curve is sampled into a polyline at the given tolerance.
/// Returns `EdgeRenderData` with flat vertex arrays and per-edge ranges.
pub fn extract_edges(solid: &TruckSolid, tolerance: f64, next_id: &mut u64) -> EdgeRenderData {
    use std::collections::HashSet;
    use truck_modeling::{BoundedCurve, ParameterDivision1D};

    let mut vertices: Vec<f32> = Vec::new();
    let mut edge_ranges: Vec<EdgeRange> = Vec::new();
    let mut seen_edges = HashSet::new();

    for shell in solid.boundaries().iter() {
        for edge in shell.edge_iter() {
            // Deduplicate edges (each edge appears in two faces)
            let eid = edge.id();
            if !seen_edges.insert(eid) {
                continue;
            }

            let edge_id = KernelId(*next_id);
            *next_id += 1;

            let curve = edge.oriented_curve();
            let range = curve.range_tuple();

            let start_vertex = (vertices.len() / 3) as u32;

            // Sample points along the edge curve
            let (_params, points) = curve.parameter_division(range, tolerance);

            for pt in &points {
                vertices.push(pt[0] as f32);
                vertices.push(pt[1] as f32);
                vertices.push(pt[2] as f32);
            }

            let end_vertex = (vertices.len() / 3) as u32;

            if end_vertex > start_vertex {
                edge_ranges.push(EdgeRange {
                    edge_id,
                    start_vertex,
                    end_vertex,
                });
            }
        }
    }

    EdgeRenderData {
        vertices,
        edge_ranges,
    }
}

/// Expose signed volume for tests (same file = private access to fix_inverted_mesh).
#[cfg(test)]
fn test_mesh_signed_volume(mesh: &RenderMesh) -> f64 {
    mesh_signed_volume(mesh)
}

/// Fallback tessellation: merge everything into a single PolygonMesh.
fn tessellate_solid_merged(
    solid: &TruckSolid,
    tolerance: f64,
    _next_id: &mut u64,
    solid_handle: &KernelSolidHandle,
) -> std::result::Result<RenderMesh, KernelError> {
    use truck_meshalgo::tessellation::MeshedShape;

    let meshed = solid.triangulation(tolerance);
    let mesh = meshed.to_polygon();

    let positions = mesh.positions();
    let normals = mesh.normals();
    let tri_faces = mesh.tri_faces();

    let mut vertices = Vec::with_capacity(positions.len() * 3);
    let mut norms = Vec::with_capacity(normals.len() * 3);
    let mut indices = Vec::new();

    for pos in positions {
        vertices.push(pos[0] as f32);
        vertices.push(pos[1] as f32);
        vertices.push(pos[2] as f32);
    }

    for norm in normals {
        norms.push(norm[0] as f32);
        norms.push(norm[1] as f32);
        norms.push(norm[2] as f32);
    }

    for tri in tri_faces {
        for v in tri.iter() {
            indices.push(v.pos as u32);
        }
    }

    // Merged fallback: single face covering all triangles, use face_idx=0
    let face_id = KernelId(solid_handle.id() * 10000);

    let face_ranges = vec![FaceRange {
        face_id,
        start_index: 0,
        end_index: indices.len() as u32,
    }];

    let mut mesh = RenderMesh {
        vertices,
        normals: norms,
        indices,
        face_ranges,
    };

    // Same fix as the per-face path: detect inside-out winding via signed
    // volume and flip normals + winding if negative.
    fix_inverted_mesh(&mut mesh);

    Ok(mesh)
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::primitives;
    use crate::truck_kernel::TruckKernel;

    /// Build a simple correctly-wound unit cube mesh (outward normals, positive volume).
    fn correct_cube_mesh() -> RenderMesh {
        // 8 vertices of a unit cube
        let vertices = vec![
            0.0f32, 0.0, 0.0, // 0
            1.0, 0.0, 0.0, // 1
            1.0, 1.0, 0.0, // 2
            0.0, 1.0, 0.0, // 3
            0.0, 0.0, 1.0, // 4
            1.0, 0.0, 1.0, // 5
            1.0, 1.0, 1.0, // 6
            0.0, 1.0, 1.0, // 7
        ];
        // Outward normals per vertex (approximate — each vertex used by 3 faces)
        let normals = vec![
            -0.577f32, -0.577, -0.577, // 0
            0.577, -0.577, -0.577, // 1
            0.577, 0.577, -0.577, // 2
            -0.577, 0.577, -0.577, // 3
            -0.577, -0.577, 0.577, // 4
            0.577, -0.577, 0.577, // 5
            0.577, 0.577, 0.577, // 6
            -0.577, 0.577, 0.577, // 7
        ];
        // CCW winding when viewed from outside (outward normals)
        let indices = vec![
            // Front face (z=1): 4,5,6, 4,6,7
            4, 5, 6, 4, 6, 7, // Back face (z=0): 0,3,2, 0,2,1
            0, 3, 2, 0, 2, 1, // Right face (x=1): 1,2,6, 1,6,5
            1, 2, 6, 1, 6, 5, // Left face (x=0): 0,4,7, 0,7,3
            0, 4, 7, 0, 7, 3, // Top face (y=1): 3,7,6, 3,6,2
            3, 7, 6, 3, 6, 2, // Bottom face (y=0): 0,1,5, 0,5,4
            0, 1, 5, 0, 5, 4,
        ];
        RenderMesh {
            vertices,
            normals,
            indices,
            face_ranges: vec![],
        }
    }

    /// Build an inverted cube mesh (all winding reversed, normals inward).
    fn inverted_cube_mesh() -> RenderMesh {
        let mut mesh = correct_cube_mesh();
        // Flip all normals
        for n in mesh.normals.iter_mut() {
            *n = -*n;
        }
        // Reverse winding of every triangle
        for tri in mesh.indices.chunks_exact_mut(3) {
            tri.swap(1, 2);
        }
        mesh
    }

    #[test]
    fn test_fix_inverted_mesh_flips_negative_volume() {
        let mut mesh = inverted_cube_mesh();

        // Confirm it starts with negative signed volume
        let vol_before = test_mesh_signed_volume(&mesh);
        assert!(
            vol_before < 0.0,
            "Inverted mesh should have negative signed volume, got {:.4}",
            vol_before
        );

        // Save original normals for comparison
        let normals_before: Vec<f32> = mesh.normals.clone();
        let indices_before: Vec<u32> = mesh.indices.clone();

        fix_inverted_mesh(&mut mesh);

        // After fix: positive signed volume
        let vol_after = test_mesh_signed_volume(&mesh);
        assert!(
            vol_after > 0.0,
            "Fixed mesh should have positive signed volume, got {:.4}",
            vol_after
        );

        // Normals should be negated
        for (i, (before, after)) in normals_before.iter().zip(mesh.normals.iter()).enumerate() {
            assert!(
                (*before + *after).abs() < 1e-6,
                "Normal[{}]: before={}, after={} should be negated",
                i,
                before,
                after
            );
        }

        // Winding should be reversed (indices 1 and 2 swapped in each triangle)
        for (tri_idx, (before, after)) in indices_before
            .chunks(3)
            .zip(mesh.indices.chunks(3))
            .enumerate()
        {
            assert_eq!(
                before[0], after[0],
                "Triangle {} index 0 should be unchanged",
                tri_idx
            );
            assert_eq!(
                before[1], after[2],
                "Triangle {} index 1 should swap with 2",
                tri_idx
            );
            assert_eq!(
                before[2], after[1],
                "Triangle {} index 2 should swap with 1",
                tri_idx
            );
        }
    }

    #[test]
    fn test_fix_inverted_mesh_noop_for_positive_volume() {
        let mut mesh = correct_cube_mesh();

        let vol_before = test_mesh_signed_volume(&mesh);
        assert!(
            vol_before > 0.0,
            "Correct mesh should have positive signed volume, got {:.4}",
            vol_before
        );

        let normals_before = mesh.normals.clone();
        let indices_before = mesh.indices.clone();

        fix_inverted_mesh(&mut mesh);

        // Should be unchanged
        assert_eq!(mesh.normals, normals_before, "Normals should be unchanged");
        assert_eq!(mesh.indices, indices_before, "Indices should be unchanged");
    }

    #[test]
    fn test_flipped_extrude_needs_fix() {
        use truck_modeling::{builder, Point3, Vector3};

        // Create a face in XY plane via sweeps (same approach as make_box first two sweeps)
        let v = builder::vertex(Point3::new(0.0, 0.0, 0.0));
        let edge = builder::tsweep(&v, Vector3::new(10.0, 0.0, 0.0));
        let face = builder::tsweep(&edge, Vector3::new(0.0, 10.0, 0.0));

        // Extrude in ANTIPARALLEL direction (-Z) — this is the flipped case
        let flipped_solid = builder::tsweep(&face, Vector3::new(0.0, 0.0, -10.0));

        let mut kernel = TruckKernel::new();
        let handle = kernel.store_solid(flipped_solid);

        // Test the per-face path (tessellate_solid)
        let mesh = tessellate_solid(kernel.get_solid(&handle).unwrap(), 0.05, &mut 1u64, &handle)
            .expect("tessellate_solid should succeed");

        let vol = test_mesh_signed_volume(&mesh);
        assert!(
            vol > 0.0,
            "tessellate_solid: flipped extrude should have positive signed volume after fix (got {:.4})",
            vol
        );

        // Test the merged fallback path (tessellate_solid_merged)
        let mesh_merged =
            tessellate_solid_merged(kernel.get_solid(&handle).unwrap(), 0.05, &mut 1u64, &handle)
                .expect("tessellate_solid_merged should succeed");

        let vol_merged = test_mesh_signed_volume(&mesh_merged);
        assert!(
            vol_merged > 0.0,
            "tessellate_solid_merged: flipped extrude should have positive signed volume after fix (got {:.4})",
            vol_merged
        );
    }

    #[test]
    fn test_flipped_cylinder_extrude_needs_fix() {
        // Circle profile extruded in -Z — matches what the user does in the app
        let cylinder_up = primitives::make_cylinder(5.0, 10.0);

        // For -Z cylinder, build from scratch: circle face + tsweep(-Z)
        use truck_modeling::{builder, EuclideanSpace, Point3, Rad, Vector3};
        let v = builder::vertex(Point3::new(5.0, 0.0, 0.0));
        let wire = builder::rsweep(
            &v,
            Point3::origin(),
            Vector3::unit_z(),
            Rad(2.0 * std::f64::consts::PI),
            3,
        );
        let face = builder::try_attach_plane(&[wire]).expect("Failed to create circular face");
        let cylinder_down = builder::tsweep(&face, Vector3::new(0.0, 0.0, -10.0));

        let mut kernel = TruckKernel::new();

        // Test +Z cylinder (should already be correct)
        let handle_up = kernel.store_solid(cylinder_up);
        let mesh_up = tessellate_solid(
            kernel.get_solid(&handle_up).unwrap(),
            0.05,
            &mut 1u64,
            &handle_up,
        )
        .expect("tessellate +Z cylinder");
        let vol_up = test_mesh_signed_volume(&mesh_up);
        assert!(
            vol_up > 0.0,
            "+Z cylinder should have positive signed volume (got {:.4})",
            vol_up
        );

        // Test -Z cylinder (flipped — needs fix)
        let handle_down = kernel.store_solid(cylinder_down);
        let mesh_down = tessellate_solid(
            kernel.get_solid(&handle_down).unwrap(),
            0.05,
            &mut 1u64,
            &handle_down,
        )
        .expect("tessellate -Z cylinder");
        let vol_down = test_mesh_signed_volume(&mesh_down);
        assert!(
            vol_down > 0.0,
            "-Z cylinder should have positive signed volume after fix (got {:.4})",
            vol_down
        );

        // Volumes should be similar magnitude
        let ratio = vol_up / vol_down;
        assert!(
            (0.5..2.0).contains(&ratio),
            "+Z and -Z cylinder volumes should be similar (ratio={:.2})",
            ratio
        );
    }
}
