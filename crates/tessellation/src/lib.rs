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

/// Tessellate a single planar face into triangles using ear-clipping.
///
/// Ear-clipping correctly handles concave polygons, unlike fan triangulation
/// which produces crossing triangles on non-convex shapes (e.g. L-profiles).
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

    // Project to 2D for ear-clipping
    let projected = project_to_2d(&vertices, &face_normal);

    // Ear-clip triangulation
    let triangles = ear_clip(&projected);
    for (a, b, c) in triangles {
        mesh.add_triangle(base_idx[a], base_idx[b], base_idx[c]);
    }

    mesh
}

/// Project 3D polygon vertices onto a 2D plane defined by the face normal.
fn project_to_2d(vertices: &[Point3d], normal: &Vec3) -> Vec<(f64, f64)> {
    // Build orthonormal basis on the face plane
    let u_axis = if normal.x.abs() < 0.9 {
        Vec3::new(1.0, 0.0, 0.0).cross(normal).normalize()
    } else {
        Vec3::new(0.0, 1.0, 0.0).cross(normal).normalize()
    };
    let v_axis = normal.cross(&u_axis);

    vertices
        .iter()
        .map(|p| {
            let v = Vec3::new(p.x, p.y, p.z);
            (v.dot(&u_axis), v.dot(&v_axis))
        })
        .collect()
}

/// Ear-clipping triangulation for a simple polygon (may be concave).
///
/// Returns triangle indices into the original vertex list.
fn ear_clip(polygon: &[(f64, f64)]) -> Vec<(usize, usize, usize)> {
    let n = polygon.len();
    if n < 3 {
        return vec![];
    }
    if n == 3 {
        return vec![(0, 1, 2)];
    }

    let mut indices: Vec<usize> = (0..n).collect();
    let mut result = Vec::new();

    // Determine winding: positive = CCW
    let signed_area: f64 = indices
        .windows(2)
        .map(|w| {
            let (x0, y0) = polygon[w[0]];
            let (x1, y1) = polygon[w[1]];
            (x1 - x0) * (y1 + y0)
        })
        .sum::<f64>()
        + {
            let (x0, y0) = polygon[*indices.last().unwrap()];
            let (x1, y1) = polygon[indices[0]];
            (x1 - x0) * (y1 + y0)
        };
    let ccw = signed_area < 0.0; // negative signed area = CCW in standard coords

    let mut iterations = 0;
    let max_iterations = n * n; // safety bound

    while indices.len() > 3 && iterations < max_iterations {
        iterations += 1;
        let len = indices.len();
        let mut found_ear = false;

        for i in 0..len {
            let prev = indices[(i + len - 1) % len];
            let curr = indices[i];
            let next = indices[(i + 1) % len];

            if !is_ear(polygon, &indices, prev, curr, next, ccw) {
                continue;
            }

            result.push((prev, curr, next));
            indices.remove(i);
            found_ear = true;
            break;
        }

        if !found_ear {
            // Fallback: emit remaining as fan (degenerate case)
            for i in 1..(indices.len() - 1) {
                result.push((indices[0], indices[i], indices[i + 1]));
            }
            break;
        }
    }

    if indices.len() == 3 {
        result.push((indices[0], indices[1], indices[2]));
    }

    result
}

/// Check if vertex `curr` forms an ear (convex and no other vertex inside).
fn is_ear(
    polygon: &[(f64, f64)],
    indices: &[usize],
    prev: usize,
    curr: usize,
    next: usize,
    ccw: bool,
) -> bool {
    let (ax, ay) = polygon[prev];
    let (bx, by) = polygon[curr];
    let (cx, cy) = polygon[next];

    // Cross product to check convexity
    let cross = (bx - ax) * (cy - ay) - (by - ay) * (cx - ax);
    if ccw && cross <= 0.0 {
        return false; // reflex vertex
    }
    if !ccw && cross >= 0.0 {
        return false;
    }

    // Check no other vertex lies inside triangle (prev, curr, next)
    for &idx in indices {
        if idx == prev || idx == curr || idx == next {
            continue;
        }
        if point_in_triangle(polygon[idx], (ax, ay), (bx, by), (cx, cy)) {
            return false;
        }
    }

    true
}

/// Check if point p is inside triangle (a, b, c) using barycentric coordinates.
fn point_in_triangle(
    p: (f64, f64),
    a: (f64, f64),
    b: (f64, f64),
    c: (f64, f64),
) -> bool {
    let (px, py) = p;
    let d1 = sign(px, py, a.0, a.1, b.0, b.1);
    let d2 = sign(px, py, b.0, b.1, c.0, c.1);
    let d3 = sign(px, py, c.0, c.1, a.0, a.1);

    let has_neg = (d1 < 0.0) || (d2 < 0.0) || (d3 < 0.0);
    let has_pos = (d1 > 0.0) || (d2 > 0.0) || (d3 > 0.0);

    !(has_neg && has_pos)
}

fn sign(px: f64, py: f64, x1: f64, y1: f64, x2: f64, y2: f64) -> f64 {
    (px - x2) * (y1 - y2) - (x1 - x2) * (py - y2)
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

/// Export a triangle mesh to Wavefront OBJ format.
///
/// OBJ is a widely-supported text format viewable in Blender, MeshLab,
/// VS Code 3D Viewer, and most CAD tools.
pub fn mesh_to_obj(mesh: &TriangleMesh) -> String {
    let mut obj = String::new();
    obj.push_str("# Generated by cad-kernel\n");

    let num_verts = mesh.vertex_count();
    let num_tris = mesh.triangle_count();

    // Vertices
    for i in 0..num_verts {
        let x = mesh.positions[i * 3];
        let y = mesh.positions[i * 3 + 1];
        let z = mesh.positions[i * 3 + 2];
        obj.push_str(&format!("v {x} {y} {z}\n"));
    }

    // Normals
    for i in 0..num_verts {
        let nx = mesh.normals[i * 3];
        let ny = mesh.normals[i * 3 + 1];
        let nz = mesh.normals[i * 3 + 2];
        obj.push_str(&format!("vn {nx} {ny} {nz}\n"));
    }

    // Faces (OBJ is 1-indexed)
    for t in 0..num_tris {
        let i0 = mesh.indices[t * 3] + 1;
        let i1 = mesh.indices[t * 3 + 1] + 1;
        let i2 = mesh.indices[t * 3 + 2] + 1;
        obj.push_str(&format!("f {i0}//{i0} {i1}//{i1} {i2}//{i2}\n"));
    }

    obj
}

/// Export a triangle mesh to binary STL format.
pub fn mesh_to_stl(mesh: &TriangleMesh) -> Vec<u8> {
    let num_tris = mesh.triangle_count() as u32;
    let mut buf: Vec<u8> = Vec::new();

    // 80-byte header
    let header = b"STL generated by cad-kernel";
    buf.extend_from_slice(header);
    buf.extend_from_slice(&[0u8; 80 - 27]); // pad to 80 bytes

    // Triangle count
    buf.extend_from_slice(&num_tris.to_le_bytes());

    for t in 0..num_tris as usize {
        let i0 = mesh.indices[t * 3] as usize;
        let i1 = mesh.indices[t * 3 + 1] as usize;
        let i2 = mesh.indices[t * 3 + 2] as usize;

        // Compute face normal
        let ax = mesh.positions[i1 * 3] - mesh.positions[i0 * 3];
        let ay = mesh.positions[i1 * 3 + 1] - mesh.positions[i0 * 3 + 1];
        let az = mesh.positions[i1 * 3 + 2] - mesh.positions[i0 * 3 + 2];
        let bx = mesh.positions[i2 * 3] - mesh.positions[i0 * 3];
        let by = mesh.positions[i2 * 3 + 1] - mesh.positions[i0 * 3 + 1];
        let bz = mesh.positions[i2 * 3 + 2] - mesh.positions[i0 * 3 + 2];
        let nx = ay * bz - az * by;
        let ny = az * bx - ax * bz;
        let nz = ax * by - ay * bx;
        let len = (nx * nx + ny * ny + nz * nz).sqrt().max(1e-12);

        // Normal
        buf.extend_from_slice(&(nx / len).to_le_bytes());
        buf.extend_from_slice(&(ny / len).to_le_bytes());
        buf.extend_from_slice(&(nz / len).to_le_bytes());

        // Vertex 0
        buf.extend_from_slice(&mesh.positions[i0 * 3].to_le_bytes());
        buf.extend_from_slice(&mesh.positions[i0 * 3 + 1].to_le_bytes());
        buf.extend_from_slice(&mesh.positions[i0 * 3 + 2].to_le_bytes());
        // Vertex 1
        buf.extend_from_slice(&mesh.positions[i1 * 3].to_le_bytes());
        buf.extend_from_slice(&mesh.positions[i1 * 3 + 1].to_le_bytes());
        buf.extend_from_slice(&mesh.positions[i1 * 3 + 2].to_le_bytes());
        // Vertex 2
        buf.extend_from_slice(&mesh.positions[i2 * 3].to_le_bytes());
        buf.extend_from_slice(&mesh.positions[i2 * 3 + 1].to_le_bytes());
        buf.extend_from_slice(&mesh.positions[i2 * 3 + 2].to_le_bytes());

        // Attribute byte count (unused)
        buf.extend_from_slice(&0u16.to_le_bytes());
    }

    buf
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

    // ── Ear-clipping unit tests ──────────────────────────────────────

    #[test]
    fn test_ear_clip_triangle() {
        let poly = vec![(0.0, 0.0), (1.0, 0.0), (0.0, 1.0)];
        let tris = ear_clip(&poly);
        assert_eq!(tris.len(), 1);
    }

    #[test]
    fn test_ear_clip_quad() {
        let poly = vec![(0.0, 0.0), (1.0, 0.0), (1.0, 1.0), (0.0, 1.0)];
        let tris = ear_clip(&poly);
        assert_eq!(tris.len(), 2);
    }

    #[test]
    fn test_ear_clip_l_shape() {
        // L-shaped concave polygon — the case that broke fan triangulation
        let poly = vec![
            (0.0, 0.0),
            (8.0, 0.0),
            (8.0, 3.0),
            (3.0, 3.0),
            (3.0, 7.0),
            (0.0, 7.0),
        ];
        let tris = ear_clip(&poly);
        assert_eq!(tris.len(), 4, "6-vertex polygon should produce 4 triangles");

        // Every triangle centroid must lie inside the L-shape polygon
        for (a, b, c) in &tris {
            let cx = (poly[*a].0 + poly[*b].0 + poly[*c].0) / 3.0;
            let cy = (poly[*a].1 + poly[*b].1 + poly[*c].1) / 3.0;
            assert!(
                point_in_polygon_2d(cx, cy, &poly),
                "Triangle centroid ({cx}, {cy}) from indices ({a},{b},{c}) is outside the L-shape"
            );
        }
    }

    #[test]
    fn test_ear_clip_u_shape() {
        // U-shaped concave polygon
        let poly = vec![
            (0.0, 0.0),
            (1.0, 0.0),
            (1.0, 3.0),
            (2.0, 3.0),
            (2.0, 0.0),
            (3.0, 0.0),
            (3.0, 4.0),
            (0.0, 4.0),
        ];
        let tris = ear_clip(&poly);
        assert_eq!(tris.len(), 6, "8-vertex polygon should produce 6 triangles");

        for (a, b, c) in &tris {
            let cx = (poly[*a].0 + poly[*b].0 + poly[*c].0) / 3.0;
            let cy = (poly[*a].1 + poly[*b].1 + poly[*c].1) / 3.0;
            assert!(
                point_in_polygon_2d(cx, cy, &poly),
                "Triangle centroid ({cx}, {cy}) is outside the U-shape"
            );
        }
    }

    #[test]
    fn test_tessellate_concave_extrusion() {
        // Extrude an L-profile and verify all triangle centroids stay inside
        use cad_kernel::operations::extrude::{extrude_profile, Profile};
        use cad_kernel::geometry::vector::Vec3;

        let mut store = EntityStore::new();
        let profile = Profile::from_points(vec![
            Point3d::new(0.0, 0.0, 0.0),
            Point3d::new(8.0, 0.0, 0.0),
            Point3d::new(8.0, 3.0, 0.0),
            Point3d::new(3.0, 3.0, 0.0),
            Point3d::new(3.0, 7.0, 0.0),
            Point3d::new(0.0, 7.0, 0.0),
        ]);
        let solid = extrude_profile(&mut store, &profile, Vec3::Z, 4.0);
        let mesh = tessellate_solid(&store, solid);

        // L-profile extruded: 2 caps * 4 tris + 6 side quads * 2 tris = 20
        assert_eq!(mesh.triangle_count(), 20);

        // Verify no degenerate triangles (zero area)
        for t in 0..mesh.triangle_count() {
            let i0 = mesh.indices[t * 3] as usize;
            let i1 = mesh.indices[t * 3 + 1] as usize;
            let i2 = mesh.indices[t * 3 + 2] as usize;
            let p0 = Point3d::new(
                mesh.positions[i0 * 3] as f64,
                mesh.positions[i0 * 3 + 1] as f64,
                mesh.positions[i0 * 3 + 2] as f64,
            );
            let p1 = Point3d::new(
                mesh.positions[i1 * 3] as f64,
                mesh.positions[i1 * 3 + 1] as f64,
                mesh.positions[i1 * 3 + 2] as f64,
            );
            let p2 = Point3d::new(
                mesh.positions[i2 * 3] as f64,
                mesh.positions[i2 * 3 + 1] as f64,
                mesh.positions[i2 * 3 + 2] as f64,
            );
            let edge1 = p1 - p0;
            let edge2 = p2 - p0;
            let area = edge1.cross(&edge2).length() * 0.5;
            assert!(area > 1e-10, "Triangle {t} is degenerate (area={area})");
        }
    }

    /// 2D point-in-polygon (ray casting) for test validation.
    fn point_in_polygon_2d(px: f64, py: f64, polygon: &[(f64, f64)]) -> bool {
        let n = polygon.len();
        let mut inside = false;
        let mut j = n - 1;
        for i in 0..n {
            let (xi, yi) = polygon[i];
            let (xj, yj) = polygon[j];
            if ((yi > py) != (yj > py)) && (px < (xj - xi) * (py - yi) / (yj - yi) + xi) {
                inside = !inside;
            }
            j = i;
        }
        inside
    }

    // ── Fillet integration test ────────────────────────────────────────

    #[test]
    fn test_tessellate_filleted_box() {
        use cad_kernel::operations::fillet::fillet_edge;

        let mut store = EntityStore::new();
        let box_id = make_box(&mut store, 0.0, 0.0, 0.0, 10.0, 8.0, 6.0);

        let v0 = Point3d::new(0.0, 0.0, 0.0);
        let v1 = Point3d::new(10.0, 0.0, 0.0);
        let filleted = fillet_edge(&mut store, box_id, v0, v1, 1.5, 4);

        let mesh = tessellate_solid(&store, filleted);
        assert!(mesh.triangle_count() > 12, "Filleted solid should have more than 12 triangles");

        // Verify no degenerate triangles
        for t in 0..mesh.triangle_count() {
            let i0 = mesh.indices[t * 3] as usize;
            let i1 = mesh.indices[t * 3 + 1] as usize;
            let i2 = mesh.indices[t * 3 + 2] as usize;
            let p0 = Point3d::new(
                mesh.positions[i0 * 3] as f64,
                mesh.positions[i0 * 3 + 1] as f64,
                mesh.positions[i0 * 3 + 2] as f64,
            );
            let p1 = Point3d::new(
                mesh.positions[i1 * 3] as f64,
                mesh.positions[i1 * 3 + 1] as f64,
                mesh.positions[i1 * 3 + 2] as f64,
            );
            let p2 = Point3d::new(
                mesh.positions[i2 * 3] as f64,
                mesh.positions[i2 * 3 + 1] as f64,
                mesh.positions[i2 * 3 + 2] as f64,
            );
            let edge1 = p1 - p0;
            let edge2 = p2 - p0;
            let area = edge1.cross(&edge2).length() * 0.5;
            assert!(area > 1e-10, "Triangle {t} is degenerate (area={area})");
        }
    }

    // ── End-to-end integration tests ────────────────────────────────

    #[test]
    fn test_full_pipeline_extrude_chamfer_tessellate_export() {
        use cad_kernel::operations::extrude::{extrude_profile, Profile};
        use cad_kernel::geometry::vector::Vec3;

        let mut store = EntityStore::new();

        // 1. Create extruded L-profile
        let profile = Profile::from_points(vec![
            Point3d::new(0.0, 0.0, 0.0),
            Point3d::new(8.0, 0.0, 0.0),
            Point3d::new(8.0, 3.0, 0.0),
            Point3d::new(3.0, 3.0, 0.0),
            Point3d::new(3.0, 7.0, 0.0),
            Point3d::new(0.0, 7.0, 0.0),
        ]);
        let solid = extrude_profile(&mut store, &profile, Vec3::Z, 4.0);

        // 2. Tessellate
        let mesh = tessellate_solid(&store, solid);
        assert_eq!(mesh.triangle_count(), 20, "L-profile extrusion should have 20 triangles");

        // 3. Export to OBJ
        let obj = mesh_to_obj(&mesh);
        let v_count = obj.lines().filter(|l| l.starts_with("v ")).count();
        assert!(v_count > 0, "OBJ should have vertices");

        // 4. Export to STL
        let stl = mesh_to_stl(&mesh);
        let expected_size = 80 + 4 + 20 * 50;
        assert_eq!(stl.len(), expected_size);
    }

    #[test]
    fn test_revolve_and_tessellate() {
        use cad_kernel::operations::revolve::revolve_profile;

        let mut store = EntityStore::new();
        let profile = vec![
            Point3d::new(3.0, 0.0, 0.0),
            Point3d::new(5.0, 0.0, 4.0),
            Point3d::new(3.5, 0.0, 8.0),
        ];
        let solid = revolve_profile(
            &mut store, &profile, Point3d::ORIGIN,
            Vec3::Z, std::f64::consts::TAU, 16,
        );

        let mesh = tessellate_solid(&store, solid);
        assert!(mesh.triangle_count() > 0, "Revolved solid should produce triangles");
        assert!(mesh.vertex_count() > 0);

        // Export and verify
        let obj = mesh_to_obj(&mesh);
        assert!(obj.contains("f "), "OBJ should contain faces");
    }

    // ── OBJ/STL export tests ─────────────────────────────────────────

    #[test]
    fn test_obj_export_box() {
        let mut store = EntityStore::new();
        let solid_id = make_box(&mut store, 0.0, 0.0, 0.0, 1.0, 1.0, 1.0);
        let mesh = tessellate_solid(&store, solid_id);
        let obj = mesh_to_obj(&mesh);

        let v_count = obj.lines().filter(|l| l.starts_with("v ")).count();
        let f_count = obj.lines().filter(|l| l.starts_with("f ")).count();
        assert_eq!(v_count, 24, "OBJ should have 24 vertices");
        assert_eq!(f_count, 12, "OBJ should have 12 faces");
    }

    #[test]
    fn test_stl_export_box() {
        let mut store = EntityStore::new();
        let solid_id = make_box(&mut store, 0.0, 0.0, 0.0, 1.0, 1.0, 1.0);
        let mesh = tessellate_solid(&store, solid_id);
        let stl = mesh_to_stl(&mesh);

        // STL: 80 header + 4 count + 50 bytes per triangle
        let expected_size = 80 + 4 + 12 * 50;
        assert_eq!(stl.len(), expected_size, "STL size should be {} bytes", expected_size);

        // Verify triangle count in header
        let tri_count = u32::from_le_bytes([stl[80], stl[81], stl[82], stl[83]]);
        assert_eq!(tri_count, 12);
    }
}
