//! Mesh-backed imported bodies (STEP import SI1, task #138 —
//! `docs/step_import_roadmap.md` §2).
//!
//! An imported body lives BESIDE the exact B-Rep arena, behind an ordinary
//! `KernelSolidHandle`: per-face triangle meshes plus analytic surface
//! classification, flattened from the neutral `ImportedBodyData` contract.
//! It is first-class for rendering, introspection, and signatures; exact
//! operations (booleans) are typed `NotSupported` walls until the SI2
//! mesh-path boolean lands. It deliberately does NOT enter the arena: real
//! STEP geometry (b-spline faces, OCC-tolerance closure) cannot satisfy the
//! arena's exactness invariants.

use waffle_types::kernel::{ImportedBodyData, ImportedSurface, TopoSignature};

/// One face: its own mesh slice + cached signature ingredients.
#[derive(Debug, Clone)]
pub struct ImportedFace {
    pub surface: ImportedSurface,
    /// Flat f64 world-coordinate vertex positions (meters).
    pub positions: Vec<f64>,
    pub normals: Vec<f64>,
    pub indices: Vec<u32>,
    /// Indices into [`ImportedBody::edges`] of this face's boundary edges.
    pub edge_indices: Vec<u32>,
    // Cached signature ingredients (computed once at ingestion).
    pub area: f64,
    pub centroid: [f64; 3],
    pub bbox: [f64; 6],
}

/// One edge: reference/render polyline + reverse adjacency.
#[derive(Debug, Clone)]
pub struct ImportedEdge {
    pub polyline: Vec<[f64; 3]>,
    /// Faces (indices into [`ImportedBody::faces`]) bounded by this edge.
    pub face_indices: Vec<u32>,
    /// Indices into [`ImportedBody::vertices`] of the polyline endpoints.
    pub endpoints: (u32, u32),
    pub length: f64,
}

/// A complete imported body: the composite of every shell in the source
/// file, flattened (shell boundaries only matter to the exact kernel, which
/// this body never enters).
#[derive(Debug, Clone, Default)]
pub struct ImportedBody {
    pub source_name: String,
    pub faces: Vec<ImportedFace>,
    pub edges: Vec<ImportedEdge>,
    /// Deduplicated edge-endpoint positions (meters).
    pub vertices: Vec<[f64; 3]>,
    pub warnings: Vec<String>,
}

impl ImportedBody {
    /// Flatten the neutral contract into the adapter's store: shells
    /// concatenate (edge indices re-based), reverse adjacency and endpoint
    /// vertices are derived, and signature ingredients are cached.
    pub fn from_data(data: &ImportedBodyData) -> Self {
        let mut body = ImportedBody {
            source_name: data.source_name.clone(),
            warnings: data.warnings.clone(),
            ..Default::default()
        };

        for shell in &data.shells {
            let edge_base = body.edges.len() as u32;
            for edge in &shell.edges {
                let first = *edge.polyline.first().unwrap_or(&[0.0; 3]);
                let last = *edge.polyline.last().unwrap_or(&[0.0; 3]);
                let v0 = body.intern_vertex(first);
                let v1 = body.intern_vertex(last);
                let length = edge
                    .polyline
                    .windows(2)
                    .map(|w| dist(w[0], w[1]))
                    .sum::<f64>();
                body.edges.push(ImportedEdge {
                    polyline: edge.polyline.clone(),
                    face_indices: Vec::new(),
                    endpoints: (v0, v1),
                    length,
                });
            }
            for face in &shell.faces {
                let face_idx = body.faces.len() as u32;
                let edge_indices: Vec<u32> = face
                    .edge_indices
                    .iter()
                    .map(|&e| e + edge_base)
                    .filter(|&e| (e as usize) < body.edges.len())
                    .collect();
                for &e in &edge_indices {
                    body.edges[e as usize].face_indices.push(face_idx);
                }
                let (area, centroid, bbox) = mesh_face_stats(&face.positions, &face.indices);
                body.faces.push(ImportedFace {
                    surface: face.surface,
                    positions: face.positions.clone(),
                    normals: face.normals.clone(),
                    indices: face.indices.clone(),
                    edge_indices,
                    area,
                    centroid,
                    bbox,
                });
            }
        }
        body
    }

    fn intern_vertex(&mut self, p: [f64; 3]) -> u32 {
        // Exact-bits dedup: endpoints of adjacent edges come from the same
        // source vertex, so their coordinates are bit-identical.
        for (i, v) in self.vertices.iter().enumerate() {
            if *v == p {
                return i as u32;
            }
        }
        self.vertices.push(p);
        (self.vertices.len() - 1) as u32
    }

    pub fn face_signature(&self, face_idx: usize) -> TopoSignature {
        let Some(face) = self.faces.get(face_idx) else {
            return TopoSignature::empty();
        };
        let normal = match face.surface {
            ImportedSurface::Plane { normal, .. } => Some(normal),
            _ => None,
        };
        TopoSignature {
            surface_type: Some(face.surface.surface_type_str().to_string()),
            area: Some(face.area),
            centroid: Some(face.centroid),
            normal,
            bbox: Some(face.bbox),
            ..TopoSignature::empty()
        }
    }

    pub fn edge_signature(&self, edge_idx: usize) -> TopoSignature {
        let Some(edge) = self.edges.get(edge_idx) else {
            return TopoSignature::empty();
        };
        let mut bbox = [
            f64::INFINITY,
            f64::INFINITY,
            f64::INFINITY,
            f64::NEG_INFINITY,
            f64::NEG_INFINITY,
            f64::NEG_INFINITY,
        ];
        let mut centroid = [0.0; 3];
        for p in &edge.polyline {
            for k in 0..3 {
                bbox[k] = bbox[k].min(p[k]);
                bbox[k + 3] = bbox[k + 3].max(p[k]);
                centroid[k] += p[k];
            }
        }
        let n = edge.polyline.len().max(1) as f64;
        centroid.iter_mut().for_each(|c| *c /= n);
        TopoSignature {
            length: Some(edge.length),
            centroid: Some(centroid),
            bbox: Some(bbox),
            ..TopoSignature::empty()
        }
    }

    pub fn vertex_signature(&self, vertex_idx: usize) -> TopoSignature {
        let Some(p) = self.vertices.get(vertex_idx) else {
            return TopoSignature::empty();
        };
        TopoSignature {
            centroid: Some(*p),
            bbox: Some([p[0], p[1], p[2], p[0], p[1], p[2]]),
            ..TopoSignature::empty()
        }
    }
}

fn dist(a: [f64; 3], b: [f64; 3]) -> f64 {
    ((b[0] - a[0]).powi(2) + (b[1] - a[1]).powi(2) + (b[2] - a[2]).powi(2)).sqrt()
}

/// Area, area-weighted centroid, and bbox of a triangle mesh.
fn mesh_face_stats(positions: &[f64], indices: &[u32]) -> (f64, [f64; 3], [f64; 6]) {
    let mut bbox = [
        f64::INFINITY,
        f64::INFINITY,
        f64::INFINITY,
        f64::NEG_INFINITY,
        f64::NEG_INFINITY,
        f64::NEG_INFINITY,
    ];
    let p = |i: u32| -> [f64; 3] {
        let i = i as usize * 3;
        [positions[i], positions[i + 1], positions[i + 2]]
    };
    for chunk in positions.chunks_exact(3) {
        for k in 0..3 {
            bbox[k] = bbox[k].min(chunk[k]);
            bbox[k + 3] = bbox[k + 3].max(chunk[k]);
        }
    }
    let mut area = 0.0;
    let mut centroid = [0.0; 3];
    for tri in indices.chunks_exact(3) {
        let (a, b, c) = (p(tri[0]), p(tri[1]), p(tri[2]));
        let u = [b[0] - a[0], b[1] - a[1], b[2] - a[2]];
        let v = [c[0] - a[0], c[1] - a[1], c[2] - a[2]];
        let cx = u[1] * v[2] - u[2] * v[1];
        let cy = u[2] * v[0] - u[0] * v[2];
        let cz = u[0] * v[1] - u[1] * v[0];
        let t_area = 0.5 * (cx * cx + cy * cy + cz * cz).sqrt();
        area += t_area;
        for k in 0..3 {
            centroid[k] += t_area * (a[k] + b[k] + c[k]) / 3.0;
        }
    }
    if area > 0.0 {
        centroid.iter_mut().for_each(|c| *c /= area);
    } else if !positions.is_empty() {
        let n = (positions.len() / 3) as f64;
        for chunk in positions.chunks_exact(3) {
            for k in 0..3 {
                centroid[k] += chunk[k] / n;
            }
        }
    }
    (area, centroid, bbox)
}

#[cfg(test)]
mod tests {
    use super::*;
    use waffle_types::kernel::{ImportedEdgeData, ImportedFaceData, ImportedShellData};

    /// A unit right triangle in the z=0 plane with one boundary edge.
    fn tri_shell() -> ImportedShellData {
        ImportedShellData {
            faces: vec![ImportedFaceData {
                surface: ImportedSurface::Plane {
                    origin: [0.0; 3],
                    normal: [0.0, 0.0, 1.0],
                },
                positions: vec![0.0, 0.0, 0.0, 1.0, 0.0, 0.0, 0.0, 1.0, 0.0],
                normals: vec![0.0, 0.0, 1.0, 0.0, 0.0, 1.0, 0.0, 0.0, 1.0],
                indices: vec![0, 1, 2],
                edge_indices: vec![0],
            }],
            edges: vec![ImportedEdgeData {
                polyline: vec![[0.0, 0.0, 0.0], [1.0, 0.0, 0.0]],
            }],
        }
    }

    #[test]
    fn flatten_computes_signature_caches_and_adjacency() {
        let data = ImportedBodyData {
            source_name: "tri".into(),
            shells: vec![tri_shell(), tri_shell()],
            warnings: vec![],
        };
        let body = ImportedBody::from_data(&data);
        assert_eq!(body.faces.len(), 2);
        assert_eq!(body.edges.len(), 2);
        // Second shell's face points at the REBASED edge index.
        assert_eq!(body.faces[1].edge_indices, vec![1]);
        assert_eq!(body.edges[1].face_indices, vec![1]);
        // Identical endpoints across shells dedupe to the same vertices.
        assert_eq!(body.vertices.len(), 2);
        assert!((body.faces[0].area - 0.5).abs() < 1e-12);
        assert!((body.edges[0].length - 1.0).abs() < 1e-12);

        let sig = body.face_signature(0);
        assert_eq!(sig.surface_type.as_deref(), Some("planar"));
        assert_eq!(sig.normal, Some([0.0, 0.0, 1.0]));
        assert!((sig.area.unwrap() - 0.5).abs() < 1e-12);
        let c = sig.centroid.unwrap();
        assert!((c[0] - 1.0 / 3.0).abs() < 1e-12 && (c[1] - 1.0 / 3.0).abs() < 1e-12);
    }
}
