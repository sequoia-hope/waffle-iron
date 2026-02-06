use cad_kernel::geometry::point::Point3d;
use cad_kernel::geometry::vector::Vec3;
use cad_kernel::topology::brep::*;
use serde::{Deserialize, Serialize};

/// A triangle mesh for rendering.
#[derive(Debug, Clone, Default, Serialize, Deserialize)]
pub struct TriangleMesh {
    /// Vertex positions [x, y, z, x, y, z, ...]
    pub positions: Vec<f32>,
    /// Vertex normals [nx, ny, nz, ...]
    pub normals: Vec<f32>,
    /// Triangle indices [i0, i1, i2, ...]
    pub indices: Vec<u32>,
}

impl TriangleMesh {
    pub fn new() -> Self {
        Self::default()
    }

    pub fn vertex_count(&self) -> usize {
        self.positions.len() / 3
    }

    pub fn triangle_count(&self) -> usize {
        self.indices.len() / 3
    }

    pub fn add_vertex(&mut self, pos: Point3d, normal: Vec3) -> u32 {
        let idx = self.vertex_count() as u32;
        self.positions.push(pos.x as f32);
        self.positions.push(pos.y as f32);
        self.positions.push(pos.z as f32);
        self.normals.push(normal.x as f32);
        self.normals.push(normal.y as f32);
        self.normals.push(normal.z as f32);
        idx
    }

    pub fn add_triangle(&mut self, i0: u32, i1: u32, i2: u32) {
        self.indices.push(i0);
        self.indices.push(i1);
        self.indices.push(i2);
    }

    pub fn merge(&mut self, other: &TriangleMesh) {
        let offset = self.vertex_count() as u32;
        self.positions.extend_from_slice(&other.positions);
        self.normals.extend_from_slice(&other.normals);
        for &idx in &other.indices {
            self.indices.push(idx + offset);
        }
    }
}

/// Tessellate a single planar face into triangles using fan triangulation.
pub fn tessellate_planar_face(store: &EntityStore, face_id: FaceId) -> TriangleMesh {
    let face = &store.faces[face_id];
    let loop_data = &store.loops[face.outer_loop];
    let mut mesh = TriangleMesh::new();

    if loop_data.half_edges.len() < 3 {
        return mesh;
    }

    // Compute face normal
    let normal = face.surface.normal_at(0.0, 0.0);
    let face_normal = if face.same_sense { normal } else { -normal };

    // Collect polygon vertices
    let vertices: Vec<Point3d> = loop_data
        .half_edges
        .iter()
        .map(|&he_id| {
            let he = &store.half_edges[he_id];
            store.vertices[he.start_vertex].point
        })
        .collect();

    // Add vertices to mesh
    let base_idx: Vec<u32> = vertices
        .iter()
        .map(|p| mesh.add_vertex(*p, face_normal))
        .collect();

    // Fan triangulation (works for convex polygons)
    for i in 1..(vertices.len() - 1) {
        mesh.add_triangle(base_idx[0], base_idx[i], base_idx[i + 1]);
    }

    mesh
}

/// Tessellate an entire solid into a triangle mesh.
pub fn tessellate_solid(store: &EntityStore, solid_id: SolidId) -> TriangleMesh {
    let solid = &store.solids[solid_id];
    let mut mesh = TriangleMesh::new();

    for &shell_id in &solid.shells {
        let shell = &store.shells[shell_id];
        for &face_id in &shell.faces {
            let face_mesh = tessellate_planar_face(store, face_id);
            mesh.merge(&face_mesh);
        }
    }

    mesh
}

/// Tessellate a parametric surface by sampling on a UV grid.
pub fn tessellate_surface_grid(
    surface: &cad_kernel::geometry::surfaces::Surface,
    u_range: (f64, f64),
    v_range: (f64, f64),
    u_divisions: usize,
    v_divisions: usize,
) -> TriangleMesh {
    let mut mesh = TriangleMesh::new();

    // Create grid of vertices
    let mut indices_grid = vec![vec![0u32; v_divisions + 1]; u_divisions + 1];

    for i in 0..=u_divisions {
        for j in 0..=v_divisions {
            let u = u_range.0 + (u_range.1 - u_range.0) * (i as f64 / u_divisions as f64);
            let v = v_range.0 + (v_range.1 - v_range.0) * (j as f64 / v_divisions as f64);

            let pos = surface.evaluate(u, v);
            let normal = surface.normal_at(u, v);
            indices_grid[i][j] = mesh.add_vertex(pos, normal);
        }
    }

    // Create triangles
    for i in 0..u_divisions {
        for j in 0..v_divisions {
            let i00 = indices_grid[i][j];
            let i10 = indices_grid[i + 1][j];
            let i01 = indices_grid[i][j + 1];
            let i11 = indices_grid[i + 1][j + 1];

            mesh.add_triangle(i00, i10, i11);
            mesh.add_triangle(i00, i11, i01);
        }
    }

    mesh
}

#[cfg(test)]
mod tests {
    use super::*;
    use cad_kernel::topology::primitives::make_box;

    #[test]
    fn test_tessellate_box() {
        let mut store = EntityStore::new();
        let solid_id = make_box(&mut store, 0.0, 0.0, 0.0, 1.0, 1.0, 1.0);

        let mesh = tessellate_solid(&store, solid_id);
        assert!(mesh.vertex_count() > 0);
        assert!(mesh.triangle_count() > 0);
        // Each face of a box has 2 triangles, 6 faces = 12 triangles
        assert_eq!(mesh.triangle_count(), 12);
    }

    #[test]
    fn test_tessellate_surface_grid() {
        use cad_kernel::geometry::surfaces::{Sphere, Surface};
        use cad_kernel::geometry::point::Point3d;

        let sphere = Surface::Sphere(Sphere::new(Point3d::ORIGIN, 1.0));
        let mesh = tessellate_surface_grid(
            &sphere,
            (0.0, std::f64::consts::TAU),
            (-std::f64::consts::FRAC_PI_2, std::f64::consts::FRAC_PI_2),
            16,
            8,
        );
        assert!(mesh.vertex_count() > 0);
        assert!(mesh.triangle_count() > 0);
    }
}
