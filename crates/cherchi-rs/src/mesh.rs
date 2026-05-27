//! Public mesh type for cross-backend boolean operations.
//!
//! `Mesh` is the lingua franca for `MeshBoolean` implementations:
//! input meshes go in, an output mesh comes back. Distinct from
//! `arrangements::FastTrimesh` (the internal adjacency-aware
//! structure used by the native arrangement port) — `FastTrimesh`
//! is too detailed for an external API, and `Mesh` is too simple
//! for arrangement bookkeeping.

use cad_primitives::Point3;

/// Indexed triangulated mesh.
///
/// `verts` are 3D points; `tris` are 0-indexed triplets into `verts`.
/// The 4-billion-triangle cap (u32 indices) is adequate for any
/// realistic CAD workload.
///
/// Inputs to `MeshBoolean::boolean` should be closed, manifold, and
/// non-self-intersecting for correct results; the trait does NOT
/// validate. Callers needing validation should do it upstream.
#[derive(Clone, Debug, PartialEq)]
pub struct Mesh {
    pub verts: Vec<Point3>,
    pub tris: Vec<[u32; 3]>,
}

impl Mesh {
    /// Empty mesh: zero vertices, zero triangles.
    pub const fn empty() -> Self {
        Self {
            verts: Vec::new(),
            tris: Vec::new(),
        }
    }

    /// Construct a mesh from owned vertex + triangle arrays.
    pub fn new(verts: Vec<Point3>, tris: Vec<[u32; 3]>) -> Self {
        Self { verts, tris }
    }

    pub fn num_verts(&self) -> usize {
        self.verts.len()
    }

    pub fn num_tris(&self) -> usize {
        self.tris.len()
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn p(x: f64, y: f64, z: f64) -> Point3 {
        Point3::new(x, y, z)
    }

    #[test]
    fn empty_is_zero_sized() {
        let m = Mesh::empty();
        assert_eq!(m.num_verts(), 0);
        assert_eq!(m.num_tris(), 0);
    }

    #[test]
    fn new_stores_inputs() {
        let v = vec![p(0.0, 0.0, 0.0), p(1.0, 0.0, 0.0), p(0.0, 1.0, 0.0)];
        let t = vec![[0u32, 1, 2]];
        let m = Mesh::new(v.clone(), t.clone());
        assert_eq!(m.verts, v);
        assert_eq!(m.tris, t);
    }

    #[test]
    fn clone_debug_partial_eq() {
        let v = vec![p(0.0, 0.0, 0.0), p(1.0, 0.0, 0.0), p(0.0, 1.0, 0.0)];
        let t = vec![[0u32, 1, 2]];
        let a = Mesh::new(v, t);
        let b = a.clone();
        assert_eq!(a, b);
        assert!(!format!("{:?}", a).is_empty());
    }
}
