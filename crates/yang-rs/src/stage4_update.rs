//! The 2D parametric-domain patch type shared by the chart layer
//! (`stage4_project`, which extracts one from a patch's 3D boundary cycles) and
//! the §4.4.1 construct pass (`stage4_construct`, which re-triangulates it with
//! a CDT in that domain — "The triangulation can be totally operated in the
//! parametric domain", Yang 2025 §4.4.1).

use cad_primitives::Point2;

/// A mesh patch in a 2D parametric domain, already triangulated upstream (Stage
/// 1). Only its boundary topology is needed to re-triangulate with an inserted
/// intersection curve.
#[derive(Debug, Clone, PartialEq)]
pub struct Patch {
    /// Parametric-domain vertex pool.
    pub verts: Vec<Point2>,
    /// Outer boundary loop, CCW, indices into `verts`.
    pub boundary: Vec<u32>,
    /// Inner boundary loops (existing holes), indices into `verts`.
    pub holes: Vec<Vec<u32>>,
}
