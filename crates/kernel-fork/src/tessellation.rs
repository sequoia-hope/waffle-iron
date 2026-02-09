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
) -> std::result::Result<RenderMesh, KernelError> {
    let meshed_solid = solid.triangulation(tolerance);

    let mut all_vertices: Vec<f32> = Vec::new();
    let mut all_normals: Vec<f32> = Vec::new();
    let mut all_indices: Vec<u32> = Vec::new();
    let mut face_ranges: Vec<FaceRange> = Vec::new();

    // Iterate the meshed solid's shells and faces
    for shell in meshed_solid.boundaries().iter() {
        for face in shell.face_iter() {
            let face_id = KernelId(*next_id);
            *next_id += 1;

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
        return tessellate_solid_merged(solid, tolerance, next_id);
    }

    Ok(RenderMesh {
        vertices: all_vertices,
        normals: all_normals,
        indices: all_indices,
        face_ranges,
    })
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

/// Fallback tessellation: merge everything into a single PolygonMesh.
fn tessellate_solid_merged(
    solid: &TruckSolid,
    tolerance: f64,
    next_id: &mut u64,
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

    let face_id = KernelId(*next_id);
    *next_id += 1;

    let face_ranges = vec![FaceRange {
        face_id,
        start_index: 0,
        end_index: indices.len() as u32,
    }];

    Ok(RenderMesh {
        vertices,
        normals: norms,
        indices,
        face_ranges,
    })
}
