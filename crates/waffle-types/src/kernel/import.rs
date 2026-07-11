//! Neutral contract types for externally-imported B-Rep bodies (STEP import,
//! task #138 — `docs/step_import_roadmap.md`).
//!
//! An imported body is a **mesh-backed** body: per-face triangle meshes plus
//! analytic surface classification (with full parameters for planes — the
//! data sketch-on-face and offset planes consume). It is NOT an exact kernel
//! solid — real-world STEP files contain b-spline surfaces and are only
//! closed to the writing kernel's tolerance, so they cannot pass kernel-v2's
//! exactness invariants. The kernel stores these bodies beside its arena
//! solids behind ordinary `KernelSolidHandle`s (SI1); exact ingestion of
//! analytic-only imports is roadmapped (SI5).
//!
//! These types are RUNTIME-ONLY (like `RenderMesh`): the persisted artifact
//! is the compressed STEP text inside the feature parameters, re-parsed on
//! rebuild. No serde.

/// Surface classification of an imported face, in world coordinates
/// (meters). Planes carry full parameters — `compute_signature` reports
/// them (`surface_type == "planar"`, `normal`), which is what gates
/// sketch-on-face and offset-face datum planes. Curved kinds carry only
/// their classification in SI1; axis/radius parameters arrive with their
/// first consumer (SI3 datum axes).
#[derive(Debug, Clone, Copy, PartialEq)]
pub enum ImportedSurface {
    /// Planar face: a point on the plane + unit OUTWARD normal.
    Plane {
        origin: [f64; 3],
        normal: [f64; 3],
    },
    Cylindrical,
    Conical,
    Spherical,
    Toroidal,
    /// B-spline / NURBS / swept / otherwise non-elementary surface: the mesh
    /// is the only representation carried across the boundary.
    Freeform,
}

impl ImportedSurface {
    /// The `TopoSignature.surface_type` string for this surface, matching the
    /// vocabulary the sketch-plane resolver expects ("planar" gates
    /// sketch-on-face and offset-face datum planes).
    pub fn surface_type_str(&self) -> &'static str {
        match self {
            ImportedSurface::Plane { .. } => "planar",
            ImportedSurface::Cylindrical => "cylindrical",
            ImportedSurface::Conical => "conical",
            ImportedSurface::Spherical => "spherical",
            ImportedSurface::Toroidal => "toroidal",
            ImportedSurface::Freeform => "freeform",
        }
    }
}

/// One face of an imported body: its own triangle mesh + surface descriptor
/// + the indices (into the owning shell's `edges`) of its boundary edges.
#[derive(Debug, Clone)]
pub struct ImportedFaceData {
    pub surface: ImportedSurface,
    /// Flat vertex positions `[x0,y0,z0, x1,...]`, meters, world coords.
    pub positions: Vec<f64>,
    /// Flat per-vertex normals, same layout, unit length, outward.
    pub normals: Vec<f64>,
    /// Triangle indices into `positions`/`normals` (CCW seen from outside).
    pub indices: Vec<u32>,
    /// Indices into the owning `ImportedShellData::edges` of this face's
    /// boundary edges (outer + hole loops, unordered, deduplicated).
    pub edge_indices: Vec<u32>,
}

/// One edge of an imported body, as a rendering/reference polyline.
#[derive(Debug, Clone)]
pub struct ImportedEdgeData {
    /// Polyline vertices, meters, world coords. At least 2 points.
    pub polyline: Vec<[f64; 3]>,
}

/// One shell (connected face set) of an imported body.
#[derive(Debug, Clone, Default)]
pub struct ImportedShellData {
    pub faces: Vec<ImportedFaceData>,
    pub edges: Vec<ImportedEdgeData>,
}

/// A complete imported body: every solid/shell from every assembly path of
/// the source file, baked into world coordinates (meters) — the "composite
/// wrap" of a multi-body STEP.
#[derive(Debug, Clone, Default)]
pub struct ImportedBodyData {
    /// Source file stem, for diagnostics.
    pub source_name: String,
    pub shells: Vec<ImportedShellData>,
    /// Human-readable notes accumulated during conversion (skipped entities,
    /// unit fallbacks). Surfaced as feature warnings, never silent.
    pub warnings: Vec<String>,
}

impl ImportedBodyData {
    pub fn face_count(&self) -> usize {
        self.shells.iter().map(|s| s.faces.len()).sum()
    }

    pub fn is_empty(&self) -> bool {
        self.shells.iter().all(|s| s.faces.is_empty())
    }

    /// Apply a rigid placement: rotate by intrinsic X→Y→Z Euler angles
    /// (degrees) about the model origin, then translate (meters). Plane
    /// descriptors transform exactly; mesh vertices/normals numerically.
    pub fn apply_placement(&mut self, rotation_deg: [f64; 3], translation_m: [f64; 3]) {
        let r = rotation_matrix_xyz_deg(rotation_deg);
        let t = translation_m;
        let rot_p = |p: [f64; 3]| mat3_mul_vec(&r, p);
        let place_p = |p: [f64; 3]| {
            let q = mat3_mul_vec(&r, p);
            [q[0] + t[0], q[1] + t[1], q[2] + t[2]]
        };
        for shell in &mut self.shells {
            for face in &mut shell.faces {
                for chunk in face.positions.chunks_exact_mut(3) {
                    let p = place_p([chunk[0], chunk[1], chunk[2]]);
                    chunk.copy_from_slice(&p);
                }
                for chunk in face.normals.chunks_exact_mut(3) {
                    let n = rot_p([chunk[0], chunk[1], chunk[2]]);
                    chunk.copy_from_slice(&n);
                }
                if let ImportedSurface::Plane { origin, normal } = face.surface {
                    face.surface = ImportedSurface::Plane {
                        origin: place_p(origin),
                        normal: rot_p(normal),
                    };
                }
            }
            for edge in &mut shell.edges {
                for p in &mut edge.polyline {
                    *p = place_p(*p);
                }
            }
        }
    }
}

/// Row-major 3×3 rotation matrix for intrinsic X→Y→Z Euler angles in degrees
/// (i.e. `R = Rz(rz)·Ry(ry)·Rx(rx)` applied to column vectors).
pub fn rotation_matrix_xyz_deg(deg: [f64; 3]) -> [[f64; 3]; 3] {
    let [rx, ry, rz] = deg.map(f64::to_radians);
    let (sx, cx) = rx.sin_cos();
    let (sy, cy) = ry.sin_cos();
    let (sz, cz) = rz.sin_cos();
    // Rz·Ry·Rx, row-major.
    [
        [cz * cy, cz * sy * sx - sz * cx, cz * sy * cx + sz * sx],
        [sz * cy, sz * sy * sx + cz * cx, sz * sy * cx - cz * sx],
        [-sy, cy * sx, cy * cx],
    ]
}

fn mat3_mul_vec(m: &[[f64; 3]; 3], v: [f64; 3]) -> [f64; 3] {
    [
        m[0][0] * v[0] + m[0][1] * v[1] + m[0][2] * v[2],
        m[1][0] * v[0] + m[1][1] * v[1] + m[1][2] * v[2],
        m[2][0] * v[0] + m[2][1] * v[1] + m[2][2] * v[2],
    ]
}

#[cfg(test)]
mod tests {
    use super::*;

    fn face_with_point(p: [f64; 3]) -> ImportedFaceData {
        ImportedFaceData {
            surface: ImportedSurface::Plane {
                origin: p,
                normal: [0.0, 0.0, 1.0],
            },
            positions: p.to_vec(),
            normals: vec![0.0, 0.0, 1.0],
            indices: vec![],
            edge_indices: vec![],
        }
    }

    #[test]
    fn placement_identity_is_noop() {
        let mut data = ImportedBodyData {
            source_name: "t".into(),
            shells: vec![ImportedShellData {
                faces: vec![face_with_point([1.0, 2.0, 3.0])],
                edges: vec![ImportedEdgeData {
                    polyline: vec![[1.0, 2.0, 3.0], [4.0, 5.0, 6.0]],
                }],
            }],
            warnings: vec![],
        };
        data.apply_placement([0.0, 0.0, 0.0], [0.0, 0.0, 0.0]);
        assert_eq!(data.shells[0].faces[0].positions, vec![1.0, 2.0, 3.0]);
        assert_eq!(data.shells[0].edges[0].polyline[1], [4.0, 5.0, 6.0]);
    }

    #[test]
    fn placement_rotates_then_translates() {
        let mut data = ImportedBodyData {
            source_name: "t".into(),
            shells: vec![ImportedShellData {
                faces: vec![face_with_point([1.0, 0.0, 0.0])],
                edges: vec![],
            }],
            warnings: vec![],
        };
        // 90° about Z: (1,0,0) -> (0,1,0); then +10 in x.
        data.apply_placement([0.0, 0.0, 90.0], [10.0, 0.0, 0.0]);
        let p = &data.shells[0].faces[0].positions;
        assert!((p[0] - 10.0).abs() < 1e-12);
        assert!((p[1] - 1.0).abs() < 1e-12);
        assert!(p[2].abs() < 1e-12);
        // Plane origin translates; the normal only rotates.
        let ImportedSurface::Plane { origin, normal } = data.shells[0].faces[0].surface else {
            panic!("plane expected");
        };
        assert!((origin[0] - 10.0).abs() < 1e-12 && (origin[1] - 1.0).abs() < 1e-12);
        assert!((normal[2] - 1.0).abs() < 1e-12);
    }

    #[test]
    fn surface_type_strings_match_signature_vocabulary() {
        assert_eq!(
            ImportedSurface::Plane {
                origin: [0.0; 3],
                normal: [0.0, 0.0, 1.0]
            }
            .surface_type_str(),
            "planar"
        );
        assert_eq!(ImportedSurface::Freeform.surface_type_str(), "freeform");
    }
}
