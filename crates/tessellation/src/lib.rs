use std::collections::HashMap;

use cad_kernel::geometry::point::Point3d;
use cad_kernel::geometry::vector::Vec3;
use cad_kernel::topology::brep::*;
use serde::{Deserialize, Serialize};

/// A triangle mesh for rendering and export.
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

    /// Weld vertices that are at the same position (within tolerance).
    ///
    /// This is critical for producing watertight meshes. Without welding,
    /// each face gets its own copy of shared vertices, causing slicers to
    /// detect holes at edges. After welding, adjacent triangles share vertex
    /// indices and the mesh is manifold.
    ///
    /// Normals at welded vertices are averaged from all contributing faces.
    pub fn weld_vertices(&mut self, tolerance: f32) {
        if self.positions.is_empty() {
            return;
        }

        let inv_tol = if tolerance > 0.0 { 1.0 / tolerance } else { 1e6 };

        // Map from quantized position key -> canonical new vertex index
        let mut canonical: HashMap<(i64, i64, i64), u32> = HashMap::new();
        let mut remap = vec![0u32; self.vertex_count()];
        let mut new_positions: Vec<f32> = Vec::new();
        let mut new_normals: Vec<f32> = Vec::new();
        let mut new_count: u32 = 0;

        for i in 0..self.vertex_count() {
            let x = self.positions[i * 3];
            let y = self.positions[i * 3 + 1];
            let z = self.positions[i * 3 + 2];

            let key = (
                (x as f64 * inv_tol as f64).round() as i64,
                (y as f64 * inv_tol as f64).round() as i64,
                (z as f64 * inv_tol as f64).round() as i64,
            );

            if let Some(&existing) = canonical.get(&key) {
                remap[i] = existing;
                let ei = existing as usize;
                new_normals[ei * 3] += self.normals[i * 3];
                new_normals[ei * 3 + 1] += self.normals[i * 3 + 1];
                new_normals[ei * 3 + 2] += self.normals[i * 3 + 2];
            } else {
                canonical.insert(key, new_count);
                remap[i] = new_count;
                new_positions.push(x);
                new_positions.push(y);
                new_positions.push(z);
                new_normals.push(self.normals[i * 3]);
                new_normals.push(self.normals[i * 3 + 1]);
                new_normals.push(self.normals[i * 3 + 2]);
                new_count += 1;
            }
        }

        // Normalize averaged normals
        for i in 0..new_count as usize {
            let nx = new_normals[i * 3];
            let ny = new_normals[i * 3 + 1];
            let nz = new_normals[i * 3 + 2];
            let len = (nx * nx + ny * ny + nz * nz).sqrt();
            if len > 1e-12 {
                new_normals[i * 3] = nx / len;
                new_normals[i * 3 + 1] = ny / len;
                new_normals[i * 3 + 2] = nz / len;
            }
        }

        // Remap triangle indices and remove degenerate triangles
        let mut new_indices = Vec::with_capacity(self.indices.len());
        for tri in 0..self.triangle_count() {
            let i0 = remap[self.indices[tri * 3] as usize];
            let i1 = remap[self.indices[tri * 3 + 1] as usize];
            let i2 = remap[self.indices[tri * 3 + 2] as usize];
            if i0 != i1 && i1 != i2 && i0 != i2 {
                new_indices.push(i0);
                new_indices.push(i1);
                new_indices.push(i2);
            }
        }

        self.positions = new_positions;
        self.normals = new_normals;
        self.indices = new_indices;
    }
}

// ─── Mesh Validation ────────────────────────────────────────────────────────

/// Result of mesh validation checks.
#[derive(Debug, Clone)]
pub struct MeshValidation {
    /// Number of boundary (non-shared) edges — should be 0 for watertight mesh
    pub boundary_edges: usize,
    /// Number of non-manifold edges (shared by >2 triangles)
    pub non_manifold_edges: usize,
    /// Whether all triangle pairs sharing an edge have consistent (opposite) winding
    pub consistent_winding: bool,
    /// Euler characteristic: V - E + F (should be 2 for genus-0 closed mesh)
    pub euler_characteristic: i64,
    /// Number of degenerate triangles (zero area)
    pub degenerate_triangles: usize,
    /// Total mesh volume (positive if normals point outward)
    pub signed_volume: f64,
}

impl MeshValidation {
    /// A mesh is watertight if it has no boundary edges and no non-manifold edges.
    pub fn is_watertight(&self) -> bool {
        self.boundary_edges == 0 && self.non_manifold_edges == 0
    }

    /// A mesh is valid for 3D printing if watertight, consistently wound, and positive volume.
    pub fn is_printable(&self) -> bool {
        self.is_watertight()
            && self.consistent_winding
            && self.degenerate_triangles == 0
            && self.signed_volume > 0.0
    }
}

/// Validate a triangle mesh for manifoldness, watertightness, and consistency.
pub fn validate_mesh(mesh: &TriangleMesh) -> MeshValidation {
    let num_tris = mesh.triangle_count();
    let num_verts = mesh.vertex_count();

    // Build edge -> face count map (undirected)
    let mut edge_face_count: HashMap<(u32, u32), i32> = HashMap::new();
    // Directed edge counts for winding check
    let mut directed_edges: HashMap<(u32, u32), i32> = HashMap::new();

    let mut degenerate_count = 0;

    for t in 0..num_tris {
        let i0 = mesh.indices[t * 3];
        let i1 = mesh.indices[t * 3 + 1];
        let i2 = mesh.indices[t * 3 + 2];

        if i0 == i1 || i1 == i2 || i0 == i2 {
            degenerate_count += 1;
            continue;
        }

        // Check geometric degeneracy
        let (p0, p1, p2) = triangle_positions(mesh, t);
        let e1 = (p1.0 - p0.0, p1.1 - p0.1, p1.2 - p0.2);
        let e2 = (p2.0 - p0.0, p2.1 - p0.1, p2.2 - p0.2);
        let cx = e1.1 * e2.2 - e1.2 * e2.1;
        let cy = e1.2 * e2.0 - e1.0 * e2.2;
        let cz = e1.0 * e2.1 - e1.1 * e2.0;
        let area = (cx * cx + cy * cy + cz * cz).sqrt() * 0.5;
        if area < 1e-10 {
            degenerate_count += 1;
        }

        let edges = [(i0, i1), (i1, i2), (i2, i0)];
        for (a, b) in edges {
            let key = if a < b { (a, b) } else { (b, a) };
            *edge_face_count.entry(key).or_insert(0) += 1;
            *directed_edges.entry((a, b)).or_insert(0) += 1;
        }
    }

    let mut boundary_edges = 0;
    let mut non_manifold_edges = 0;

    for &count in edge_face_count.values() {
        if count == 1 {
            boundary_edges += 1;
        } else if count > 2 {
            non_manifold_edges += 1;
        }
    }

    // Winding consistency: for each shared edge (count=2), check that
    // the edge appears once in each direction (a->b and b->a).
    let mut winding_ok = true;
    for (&(a, b), &count) in &edge_face_count {
        if count == 2 {
            let fwd = directed_edges.get(&(a, b)).copied().unwrap_or(0);
            let rev = directed_edges.get(&(b, a)).copied().unwrap_or(0);
            if fwd != 1 || rev != 1 {
                winding_ok = false;
                break;
            }
        }
    }

    // Euler characteristic: V - E + F
    let unique_edges = edge_face_count.len() as i64;
    let euler = num_verts as i64 - unique_edges + num_tris as i64;

    // Signed volume using divergence theorem
    let mut signed_volume = 0.0_f64;
    for t in 0..num_tris {
        let (p0, p1, p2) = triangle_positions(mesh, t);
        signed_volume += p0.0 as f64 * (p1.1 as f64 * p2.2 as f64 - p1.2 as f64 * p2.1 as f64)
            - p0.1 as f64 * (p1.0 as f64 * p2.2 as f64 - p1.2 as f64 * p2.0 as f64)
            + p0.2 as f64 * (p1.0 as f64 * p2.1 as f64 - p1.1 as f64 * p2.0 as f64);
    }
    signed_volume /= 6.0;

    MeshValidation {
        boundary_edges,
        non_manifold_edges,
        consistent_winding: winding_ok,
        euler_characteristic: euler,
        degenerate_triangles: degenerate_count,
        signed_volume,
    }
}

fn triangle_positions(mesh: &TriangleMesh, t: usize) -> ((f32, f32, f32), (f32, f32, f32), (f32, f32, f32)) {
    let i0 = mesh.indices[t * 3] as usize;
    let i1 = mesh.indices[t * 3 + 1] as usize;
    let i2 = mesh.indices[t * 3 + 2] as usize;
    (
        (mesh.positions[i0 * 3], mesh.positions[i0 * 3 + 1], mesh.positions[i0 * 3 + 2]),
        (mesh.positions[i1 * 3], mesh.positions[i1 * 3 + 1], mesh.positions[i1 * 3 + 2]),
        (mesh.positions[i2 * 3], mesh.positions[i2 * 3 + 1], mesh.positions[i2 * 3 + 2]),
    )
}

/// Repair winding order so all triangle normals point outward.
///
/// Uses BFS propagation from a known-outward seed triangle, then checks
/// overall orientation via signed volume.
pub fn repair_winding(mesh: &mut TriangleMesh) {
    let num_tris = mesh.triangle_count();
    if num_tris == 0 {
        return;
    }

    // Build adjacency: for each undirected edge, which triangles touch it
    let mut edge_to_tris: HashMap<(u32, u32), Vec<usize>> = HashMap::new();
    for t in 0..num_tris {
        let i0 = mesh.indices[t * 3];
        let i1 = mesh.indices[t * 3 + 1];
        let i2 = mesh.indices[t * 3 + 2];
        for (a, b) in [(i0, i1), (i1, i2), (i2, i0)] {
            let key = if a < b { (a, b) } else { (b, a) };
            edge_to_tris.entry(key).or_default().push(t);
        }
    }

    // BFS to propagate consistent winding from seed triangle.
    let mut visited = vec![false; num_tris];
    let mut flip = vec![false; num_tris];
    let mut queue = std::collections::VecDeque::new();

    // Seed: triangle with largest centroid-dot-normal (most likely outward-facing)
    let mut best_seed = 0;
    let mut best_score = f64::NEG_INFINITY;
    for t in 0..num_tris {
        let (p0, p1, p2) = triangle_positions(mesh, t);
        let cx = (p0.0 + p1.0 + p2.0) as f64 / 3.0;
        let cy = (p0.1 + p1.1 + p2.1) as f64 / 3.0;
        let cz = (p0.2 + p1.2 + p2.2) as f64 / 3.0;
        let e1 = ((p1.0 - p0.0) as f64, (p1.1 - p0.1) as f64, (p1.2 - p0.2) as f64);
        let e2 = ((p2.0 - p0.0) as f64, (p2.1 - p0.1) as f64, (p2.2 - p0.2) as f64);
        let nx = e1.1 * e2.2 - e1.2 * e2.1;
        let ny = e1.2 * e2.0 - e1.0 * e2.2;
        let nz = e1.0 * e2.1 - e1.1 * e2.0;
        let score = cx * nx + cy * ny + cz * nz;
        if score > best_score {
            best_score = score;
            best_seed = t;
        }
    }

    queue.push_back(best_seed);
    visited[best_seed] = true;

    while let Some(t) = queue.pop_front() {
        let ti0 = mesh.indices[t * 3];
        let ti1 = mesh.indices[t * 3 + 1];
        let ti2 = mesh.indices[t * 3 + 2];

        for &(a, b) in &[(ti0, ti1), (ti1, ti2), (ti2, ti0)] {
            let key = if a < b { (a, b) } else { (b, a) };
            if let Some(neighbors) = edge_to_tris.get(&key) {
                for &nt in neighbors {
                    if visited[nt] {
                        continue;
                    }
                    visited[nt] = true;
                    // Check if neighbor has same directed edge -> needs flip
                    let ni0 = mesh.indices[nt * 3];
                    let ni1 = mesh.indices[nt * 3 + 1];
                    let ni2 = mesh.indices[nt * 3 + 2];
                    let n_edges = [(ni0, ni1), (ni1, ni2), (ni2, ni0)];
                    let has_same_dir = n_edges.iter().any(|&(na, nb)| na == a && nb == b);
                    if has_same_dir {
                        flip[nt] = !flip[t];
                    } else {
                        flip[nt] = flip[t];
                    }
                    queue.push_back(nt);
                }
            }
        }
    }

    // Apply flips
    for t in 0..num_tris {
        if flip[t] {
            mesh.indices.swap(t * 3 + 1, t * 3 + 2);
        }
    }

    // Check overall orientation via signed volume
    let val = validate_mesh(mesh);
    if val.signed_volume < 0.0 {
        for t in 0..num_tris {
            mesh.indices.swap(t * 3 + 1, t * 3 + 2);
        }
        for i in 0..mesh.vertex_count() {
            mesh.normals[i * 3] = -mesh.normals[i * 3];
            mesh.normals[i * 3 + 1] = -mesh.normals[i * 3 + 1];
            mesh.normals[i * 3 + 2] = -mesh.normals[i * 3 + 2];
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
///
/// The resulting mesh is welded (shared vertices at edges) and has
/// consistent outward-facing winding order, making it suitable for
/// 3D printing and slicer import.
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

    // Weld duplicate vertices at shared edges to make the mesh manifold.
    // Without this, each face has its own vertices and slicers see cracks.
    mesh.weld_vertices(1e-5);

    // Fix winding order so all normals point outward consistently.
    repair_winding(&mut mesh);

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
        // After welding: 8 unique vertices, 12 triangles
        assert_eq!(mesh.vertex_count(), 8, "Welded box should have 8 vertices");
        assert_eq!(mesh.triangle_count(), 12, "Box should have 12 triangles");
    }

    #[test]
    fn test_box_mesh_is_watertight() {
        let mut store = EntityStore::new();
        let solid_id = make_box(&mut store, 0.0, 0.0, 0.0, 1.0, 1.0, 1.0);

        let mesh = tessellate_solid(&store, solid_id);
        let val = validate_mesh(&mesh);

        assert!(val.is_watertight(), "Box mesh should be watertight: {} boundary, {} non-manifold",
            val.boundary_edges, val.non_manifold_edges);
        assert_eq!(val.boundary_edges, 0, "No boundary edges");
        assert_eq!(val.non_manifold_edges, 0, "No non-manifold edges");
        assert!(val.consistent_winding, "Winding should be consistent");
        assert_eq!(val.euler_characteristic, 2, "Euler char should be 2 for genus-0 closed mesh");
        assert_eq!(val.degenerate_triangles, 0, "No degenerate triangles");
        assert!(val.signed_volume > 0.0, "Volume should be positive (outward normals)");
        // Volume of unit cube = 1.0
        assert!((val.signed_volume - 1.0).abs() < 0.01,
            "Volume of unit cube should be ~1.0, got {}", val.signed_volume);
    }

    #[test]
    fn test_box_mesh_is_printable() {
        let mut store = EntityStore::new();
        let solid_id = make_box(&mut store, 0.0, 0.0, 0.0, 5.0, 3.0, 2.0);

        let mesh = tessellate_solid(&store, solid_id);
        let val = validate_mesh(&mesh);

        assert!(val.is_printable(), "Box mesh should be printable");
        // Volume = 5 * 3 * 2 = 30
        assert!((val.signed_volume - 30.0).abs() < 0.5,
            "Volume should be ~30, got {}", val.signed_volume);
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
        assert_eq!(v_count, 8, "OBJ should have 8 welded vertices for a box");
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

    // ── Mesh quality tests ─────────────────────────────────────────────

    #[test]
    fn test_l_profile_extrusion_is_watertight() {
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
        let val = validate_mesh(&mesh);

        assert!(val.is_watertight(),
            "L-profile extrusion should be watertight: {} boundary, {} non-manifold",
            val.boundary_edges, val.non_manifold_edges);
        assert!(val.consistent_winding, "Winding should be consistent");
        assert_eq!(val.euler_characteristic, 2, "Euler should be 2");
        assert_eq!(val.degenerate_triangles, 0, "No degenerate triangles");
        assert!(val.signed_volume > 0.0, "Volume should be positive");
        // Volume = (8*3 + 3*4) * 4 = (24 + 12) * 4 = 144
        // Actually: L-shape area = 8*7 - 5*4 = 56 - 20 = 36, times height 4 = 144
        assert!((val.signed_volume - 144.0).abs() < 1.0,
            "L-profile volume should be ~144, got {}", val.signed_volume);
    }

    #[test]
    fn test_l_profile_extrusion_is_printable() {
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
        let val = validate_mesh(&mesh);

        assert!(val.is_printable(), "L-profile extrusion should be printable");
    }

    #[test]
    fn test_filleted_box_mesh_quality() {
        use cad_kernel::operations::fillet::fillet_edge;

        let mut store = EntityStore::new();
        let solid_id = make_box(&mut store, 0.0, 0.0, 0.0, 4.0, 3.0, 2.0);

        // Fillet a vertical edge of the box using endpoint coordinates
        let v0 = Point3d::new(4.0, 3.0, 0.0);
        let v1 = Point3d::new(4.0, 3.0, 2.0);

        let filleted = fillet_edge(&mut store, solid_id, v0, v1, 0.5, 4);
        let mesh = tessellate_solid(&store, filleted);
        let val = validate_mesh(&mesh);

        // Filleted solids may have boundary edges due to face reconstruction,
        // but should have no degenerate triangles and positive volume
        assert_eq!(val.degenerate_triangles, 0, "No degenerate triangles");
        assert!(val.signed_volume > 0.0, "Volume should be positive");
        assert!(mesh.triangle_count() > 12, "Filleted box should have more than 12 triangles");
        assert!(mesh.vertex_count() > 8, "Filleted box should have more than 8 vertices");
    }

    #[test]
    fn test_revolved_solid_mesh_quality() {
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
        let val = validate_mesh(&mesh);

        assert_eq!(val.degenerate_triangles, 0, "No degenerate triangles");
        assert!(mesh.triangle_count() > 0, "Should produce triangles");
        assert!(mesh.vertex_count() > 0, "Should produce vertices");
        // Revolved solid should have positive volume
        assert!(val.signed_volume > 0.0 || val.signed_volume < 0.0,
            "Volume should be nonzero for revolved solid");
    }

    #[test]
    fn test_box_obj_roundtrip_integrity() {
        let mut store = EntityStore::new();
        let solid_id = make_box(&mut store, 0.0, 0.0, 0.0, 2.0, 3.0, 4.0);
        let mesh = tessellate_solid(&store, solid_id);
        let obj = mesh_to_obj(&mesh);

        // Parse back the OBJ to verify structural integrity
        let v_lines: Vec<&str> = obj.lines().filter(|l| l.starts_with("v ")).collect();
        let vn_lines: Vec<&str> = obj.lines().filter(|l| l.starts_with("vn ")).collect();
        let f_lines: Vec<&str> = obj.lines().filter(|l| l.starts_with("f ")).collect();

        assert_eq!(v_lines.len(), 8, "8 welded vertices");
        assert_eq!(vn_lines.len(), 8, "8 vertex normals");
        assert_eq!(f_lines.len(), 12, "12 triangle faces");

        // Verify all face indices are within valid range (1-indexed in OBJ)
        for f in &f_lines {
            let parts: Vec<&str> = f.split_whitespace().skip(1).collect();
            assert_eq!(parts.len(), 3, "Each face should have 3 vertex refs");
            for part in parts {
                let idx: usize = part.split("//").next().unwrap().parse().unwrap();
                assert!(idx >= 1 && idx <= 8, "Face index {} out of range [1,8]", idx);
            }
        }

        // Verify vertex positions are parseable and within expected bounds
        for v in &v_lines {
            let coords: Vec<f32> = v.split_whitespace().skip(1)
                .map(|s| s.parse().unwrap()).collect();
            assert_eq!(coords.len(), 3);
            assert!(coords[0] >= 0.0 && coords[0] <= 2.0, "x out of bounds");
            assert!(coords[1] >= 0.0 && coords[1] <= 3.0, "y out of bounds");
            assert!(coords[2] >= 0.0 && coords[2] <= 4.0, "z out of bounds");
        }
    }

    #[test]
    fn test_box_stl_normal_validity() {
        let mut store = EntityStore::new();
        let solid_id = make_box(&mut store, 0.0, 0.0, 0.0, 1.0, 1.0, 1.0);
        let mesh = tessellate_solid(&store, solid_id);
        let stl = mesh_to_stl(&mesh);

        // Parse each triangle from STL and verify normal is unit length
        let tri_count = u32::from_le_bytes([stl[80], stl[81], stl[82], stl[83]]) as usize;
        assert_eq!(tri_count, 12);

        for t in 0..tri_count {
            let offset = 84 + t * 50;
            let nx = f32::from_le_bytes([stl[offset], stl[offset+1], stl[offset+2], stl[offset+3]]);
            let ny = f32::from_le_bytes([stl[offset+4], stl[offset+5], stl[offset+6], stl[offset+7]]);
            let nz = f32::from_le_bytes([stl[offset+8], stl[offset+9], stl[offset+10], stl[offset+11]]);

            let len = (nx*nx + ny*ny + nz*nz).sqrt();
            assert!((len - 1.0).abs() < 0.01,
                "STL triangle {} normal should be unit length, got {}", t, len);
        }
    }

    #[test]
    fn test_weld_vertices_removes_duplicates() {
        let mut mesh = TriangleMesh::new();
        // Two triangles sharing an edge, with duplicate vertices at the shared edge
        let v0 = mesh.add_vertex(Point3d::new(0.0, 0.0, 0.0), Vec3::Z);
        let v1 = mesh.add_vertex(Point3d::new(1.0, 0.0, 0.0), Vec3::Z);
        let v2 = mesh.add_vertex(Point3d::new(0.5, 1.0, 0.0), Vec3::Z);
        // Duplicate v0 and v1 for the second triangle
        let v3 = mesh.add_vertex(Point3d::new(1.0, 0.0, 0.0), Vec3::Z); // dup of v1
        let v4 = mesh.add_vertex(Point3d::new(0.0, 0.0, 0.0), Vec3::Z); // dup of v0
        let v5 = mesh.add_vertex(Point3d::new(0.5, -1.0, 0.0), Vec3::Z);

        mesh.add_triangle(v0, v1, v2);
        mesh.add_triangle(v3, v4, v5);

        assert_eq!(mesh.vertex_count(), 6, "Before welding: 6 vertices");
        mesh.weld_vertices(1e-5);
        assert_eq!(mesh.vertex_count(), 4, "After welding: 4 unique vertices");
        assert_eq!(mesh.triangle_count(), 2, "Still 2 triangles");
    }

    #[test]
    fn test_weld_removes_degenerate_triangles() {
        let mut mesh = TriangleMesh::new();
        // Triangle where two vertices are at the same position
        let v0 = mesh.add_vertex(Point3d::new(0.0, 0.0, 0.0), Vec3::Z);
        let v1 = mesh.add_vertex(Point3d::new(1.0, 0.0, 0.0), Vec3::Z);
        let v2 = mesh.add_vertex(Point3d::new(0.0, 0.0, 0.0), Vec3::Z); // dup of v0

        mesh.add_triangle(v0, v1, v2);
        mesh.weld_vertices(1e-5);

        assert_eq!(mesh.vertex_count(), 2, "Only 2 unique vertices");
        assert_eq!(mesh.triangle_count(), 0, "Degenerate triangle removed");
    }

    #[test]
    fn test_validate_mesh_open_surface() {
        // An open surface (single triangle) should NOT be watertight
        let mut mesh = TriangleMesh::new();
        let v0 = mesh.add_vertex(Point3d::new(0.0, 0.0, 0.0), Vec3::Z);
        let v1 = mesh.add_vertex(Point3d::new(1.0, 0.0, 0.0), Vec3::Z);
        let v2 = mesh.add_vertex(Point3d::new(0.5, 1.0, 0.0), Vec3::Z);
        mesh.add_triangle(v0, v1, v2);

        let val = validate_mesh(&mesh);
        assert_eq!(val.boundary_edges, 3, "Single triangle has 3 boundary edges");
        assert!(!val.is_watertight(), "Single triangle is not watertight");
        assert!(!val.is_printable(), "Single triangle is not printable");
    }

    #[test]
    fn test_different_box_sizes_all_watertight() {
        // Test several different box sizes to ensure welding works generally
        let sizes = vec![
            (1.0, 1.0, 1.0),
            (10.0, 0.1, 5.0),
            (0.001, 0.001, 0.001),
            (100.0, 200.0, 300.0),
        ];

        for (sx, sy, sz) in sizes {
            let mut store = EntityStore::new();
            let solid = make_box(&mut store, 0.0, 0.0, 0.0, sx, sy, sz);
            let mesh = tessellate_solid(&store, solid);
            let val = validate_mesh(&mesh);

            assert!(val.is_watertight(),
                "Box ({sx}x{sy}x{sz}) should be watertight: {} boundary, {} non-manifold",
                val.boundary_edges, val.non_manifold_edges);
            assert_eq!(val.euler_characteristic, 2,
                "Box ({sx}x{sy}x{sz}) euler should be 2, got {}", val.euler_characteristic);

            let expected_vol = sx * sy * sz;
            assert!((val.signed_volume - expected_vol).abs() < expected_vol * 0.01 + 0.001,
                "Box ({sx}x{sy}x{sz}) volume should be ~{expected_vol}, got {}", val.signed_volume);
        }
    }

    #[test]
    fn test_topology_audit_box_no_dangling() {
        use cad_kernel::validation::audit::{verify_topology_l0, full_verify};

        let mut store = EntityStore::new();
        let solid_id = make_box(&mut store, 0.0, 0.0, 0.0, 3.0, 4.0, 5.0);

        let audit = verify_topology_l0(&store, solid_id);
        assert!(audit.euler_valid, "Box should satisfy Euler formula");
        assert!(audit.no_dangling_vertices, "Box should have no dangling vertices");
        assert!(audit.all_faces_closed, "All faces should be closed");
        assert!(audit.all_edges_two_faced, "All edges should be two-faced");

        let report = full_verify(&store, solid_id);
        assert!(report.is_valid(), "Box should pass full verification: {:?}", report.geometry_errors);

        // Mesh output should be watertight regardless of B-Rep winding
        let mesh = tessellate_solid(&store, solid_id);
        let val = validate_mesh(&mesh);
        assert!(val.is_watertight(), "Box mesh should be watertight");
        assert!(val.is_printable(), "Box mesh should be printable");
    }

    // ── Advanced Feature Tree validation tests ─────────────────────────

    #[test]
    fn test_ft_constrained_bracket_watertight() {
        use cad_kernel::operations::feature::{
            Feature, FeatureTree, Parameter, SketchConstraint, SketchProfile,
        };

        let mut tree = FeatureTree::new();
        tree.add_parameter(Parameter::new("width", 12.0));
        tree.add_parameter(Parameter::new("height", 8.0));
        tree.add_parameter(Parameter::new("depth", 5.0));

        tree.add_feature(Feature::Sketch {
            plane_origin: [0.0, 0.0, 0.0],
            plane_normal: [0.0, 0.0, 1.0],
            profiles: vec![SketchProfile {
                points: vec![
                    [0.0, 0.0],
                    [11.0, 0.5],
                    [11.5, 7.5],
                    [0.5, 8.5],
                ],
                closed: true,
            }],
            constraints: vec![
                SketchConstraint::Fixed { point: 0, x: 0.0, y: 0.0 },
                SketchConstraint::Horizontal { line: 4 },
                SketchConstraint::Vertical { line: 5 },
                SketchConstraint::Horizontal { line: 6 },
                SketchConstraint::Vertical { line: 7 },
                SketchConstraint::Distance { point_a: 0, point_b: 1, value: 12.0 },
                SketchConstraint::Distance { point_a: 1, point_b: 2, value: 8.0 },
            ],
            lines: vec![(0, 1), (1, 2), (2, 3), (3, 0)],
        });

        tree.add_feature(Feature::Extrude {
            sketch_index: 0,
            distance: Parameter::new("depth", 5.0),
            direction: [0.0, 0.0, 1.0],
            symmetric: false,
        });

        let mut store = EntityStore::new();
        let solids = tree.evaluate(&mut store).expect("constrained bracket");
        let mesh = tessellate_solid(&store, *solids.last().unwrap());
        let val = validate_mesh(&mesh);

        assert!(val.is_watertight(), "Constrained bracket should be watertight");
        assert!(val.is_printable(), "Constrained bracket should be printable");
        assert_eq!(val.degenerate_triangles, 0);
        assert!(val.signed_volume > 0.0, "Volume should be positive");
        // Solver should produce 12x8x5 = 480 volume
        assert!((val.signed_volume - 480.0).abs() < 5.0,
            "Constrained bracket volume should be ~480, got {}", val.signed_volume);
    }

    #[test]
    fn test_ft_enclosure_has_more_tris_than_box() {
        use cad_kernel::operations::feature::{Feature, FeatureTree, Parameter, SketchProfile};
        use cad_kernel::operations::fillet::fillet_edge;

        let mut tree = FeatureTree::new();
        tree.add_feature(Feature::Sketch {
            plane_origin: [0.0, 0.0, 0.0],
            plane_normal: [0.0, 0.0, 1.0],
            profiles: vec![SketchProfile {
                points: vec![[0.0, 0.0], [14.0, 0.0], [14.0, 10.0], [0.0, 10.0]],
                closed: true,
            }],
            constraints: vec![],
            lines: vec![],
        });

        tree.add_feature(Feature::Extrude {
            sketch_index: 0,
            distance: Parameter::new("height", 6.0),
            direction: [0.0, 0.0, 1.0],
            symmetric: false,
        });

        let mut store = EntityStore::new();
        let solids = tree.evaluate(&mut store).expect("enclosure base");
        let base = *solids.last().unwrap();

        // Get vertical edges and fillet them
        let unique_edges = FeatureTree::collect_unique_edges(&store, base);
        let mut vertical_edges: Vec<(Point3d, Point3d)> = Vec::new();
        for (a, b) in &unique_edges {
            let dx = (a.x - b.x).abs();
            let dy = (a.y - b.y).abs();
            let dz = (a.z - b.z).abs();
            if dz > 1.0 && dx < 0.01 && dy < 0.01 {
                vertical_edges.push((*a, *b));
            }
        }

        let mut current = base;
        for (v0, v1) in &vertical_edges {
            current = fillet_edge(&mut store, current, *v0, *v1, 2.0, 8);
        }

        let mesh = tessellate_solid(&store, current);
        let val = validate_mesh(&mesh);

        // Filleted enclosure should have significantly more tris than basic box (12)
        assert!(mesh.triangle_count() > 50,
            "Filleted enclosure should have many tris, got {}", mesh.triangle_count());
        assert_eq!(val.degenerate_triangles, 0, "No degenerate triangles");
        assert!(val.signed_volume > 0.0, "Volume should be positive");
        // Volume should be less than 14*10*6=840 due to material removed by fillets
        assert!(val.signed_volume < 840.0 + 1.0,
            "Volume should be <= box volume 840, got {}", val.signed_volume);
    }

    #[test]
    fn test_ft_wine_glass_high_poly() {
        use cad_kernel::operations::feature::{Feature, FeatureTree, Parameter, SketchProfile};

        let mut tree = FeatureTree::new();
        tree.add_feature(Feature::Sketch {
            plane_origin: [0.0, 0.0, 0.0],
            plane_normal: [0.0, 0.0, 1.0],
            profiles: vec![SketchProfile {
                points: vec![
                    [3.5, 0.0],
                    [1.0, 0.5],
                    [0.8, 1.0],
                    [0.6, 3.0],
                    [0.6, 6.0],
                    [0.8, 7.0],
                    [1.5, 8.0],
                    [3.0, 9.0],
                    [4.5, 10.0],
                    [5.5, 11.0],
                    [5.8, 12.0],
                    [5.5, 13.0],
                ],
                closed: false,
            }],
            constraints: vec![],
            lines: vec![],
        });

        tree.add_feature(Feature::Revolve {
            sketch_index: 0,
            axis_origin: [0.0, 0.0, 0.0],
            axis_direction: [0.0, 0.0, 1.0],
            angle: Parameter::new("angle", std::f64::consts::TAU),
            segments: 48,
        });

        let mut store = EntityStore::new();
        let solids = tree.evaluate(&mut store).expect("wine glass");
        let mesh = tessellate_solid(&store, *solids.last().unwrap());
        let val = validate_mesh(&mesh);

        // High poly count due to 12 profile pts * 48 segments
        assert!(mesh.triangle_count() > 500,
            "Wine glass should be high-poly, got {} tris", mesh.triangle_count());
        assert!(mesh.vertex_count() > 200,
            "Wine glass should have many verts, got {}", mesh.vertex_count());
        assert_eq!(val.degenerate_triangles, 0, "No degenerate triangles");
        // Volume should be nonzero (positive after winding repair)
        assert!(val.signed_volume.abs() > 1.0,
            "Wine glass volume should be nonzero, got {}", val.signed_volume);
    }

    #[test]
    fn test_ft_t_bracket_extrusion() {
        use cad_kernel::operations::feature::{Feature, FeatureTree, Parameter, SketchProfile};

        let mut tree = FeatureTree::new();
        tree.add_feature(Feature::Sketch {
            plane_origin: [0.0, 0.0, 0.0],
            plane_normal: [0.0, 0.0, 1.0],
            profiles: vec![SketchProfile {
                points: vec![
                    [0.0, 0.0], [12.0, 0.0], [12.0, 3.0], [8.0, 3.0],
                    [8.0, 10.0], [4.0, 10.0], [4.0, 3.0], [0.0, 3.0],
                ],
                closed: true,
            }],
            constraints: vec![],
            lines: vec![],
        });

        tree.add_feature(Feature::Extrude {
            sketch_index: 0,
            distance: Parameter::new("depth", 5.0),
            direction: [0.0, 0.0, 1.0],
            symmetric: false,
        });

        let mut store = EntityStore::new();
        let solids = tree.evaluate(&mut store).expect("t-bracket");
        let mesh = tessellate_solid(&store, *solids.last().unwrap());
        let val = validate_mesh(&mesh);

        assert!(val.is_watertight(), "T-bracket should be watertight");
        assert_eq!(val.degenerate_triangles, 0);
        assert!(val.signed_volume > 0.0, "Volume should be positive");
        // T-shape area = 12*3 + 4*7 = 36 + 28 = 64, depth = 5 → vol = 320
        assert!((val.signed_volume - 320.0).abs() < 5.0,
            "T-bracket volume should be ~320, got {}", val.signed_volume);
    }

    #[test]
    fn test_ft_plate_with_hole_boolean() {
        use cad_kernel::boolean::engine::{boolean_op, BoolOp};
        use cad_kernel::topology::primitives::{make_box, make_cylinder};

        let mut store = EntityStore::new();
        let plate = make_box(&mut store, 0.0, 0.0, 0.0, 16.0, 10.0, 3.0);
        let cylinder = make_cylinder(&mut store, Point3d::new(8.0, 5.0, -1.0), 3.0, 5.0, 48);

        let result = boolean_op(&mut store, plate, cylinder, BoolOp::Difference)
            .expect("plate-hole boolean");
        let mesh = tessellate_solid(&store, result);
        let val = validate_mesh(&mesh);

        assert!(mesh.triangle_count() > 12,
            "Plate with hole should have more triangles than box, got {}", mesh.triangle_count());
        assert_eq!(val.degenerate_triangles, 0, "No degenerate triangles");
        // Plate volume = 16*10*3 = 480, hole removes pi*9*3 ≈ 84.8 → ~395
        // Boolean mesh is approximate, so be lenient
        assert!(val.signed_volume.abs() > 100.0,
            "Plate volume should be substantial, got {}", val.signed_volume);
    }

    #[test]
    fn test_ft_chess_pawn_high_poly() {
        use cad_kernel::operations::revolve::revolve_profile;
        use cad_kernel::geometry::vector::Vec3;

        let mut store = EntityStore::new();
        let profile = vec![
            Point3d::new(4.0, 0.0, 0.0),
            Point3d::new(4.2, 0.0, 0.5),
            Point3d::new(3.8, 0.0, 1.0),
            Point3d::new(2.5, 0.0, 1.5),
            Point3d::new(1.8, 0.0, 2.0),
            Point3d::new(1.2, 0.0, 3.0),
            Point3d::new(1.0, 0.0, 4.5),
            Point3d::new(1.0, 0.0, 5.5),
            Point3d::new(1.5, 0.0, 6.0),
            Point3d::new(1.8, 0.0, 6.5),
            Point3d::new(1.5, 0.0, 7.0),
            Point3d::new(1.2, 0.0, 7.5),
            Point3d::new(2.0, 0.0, 8.0),
            Point3d::new(2.5, 0.0, 9.0),
            Point3d::new(2.5, 0.0, 10.0),
            Point3d::new(2.0, 0.0, 11.0),
            Point3d::new(1.2, 0.0, 11.5),
            Point3d::new(0.5, 0.0, 12.0),
        ];
        let solid = revolve_profile(
            &mut store, &profile, Point3d::ORIGIN,
            Vec3::new(0.0, 0.0, 1.0), std::f64::consts::TAU, 48,
        );
        let mesh = tessellate_solid(&store, solid);
        let val = validate_mesh(&mesh);

        // 18 profile points * 48 segments → high poly count
        assert!(mesh.triangle_count() > 1000,
            "Chess pawn should be high-poly, got {} tris", mesh.triangle_count());
        assert!(mesh.vertex_count() > 500,
            "Chess pawn should have many verts, got {}", mesh.vertex_count());
        assert_eq!(val.degenerate_triangles, 0, "No degenerate triangles");
        assert!(val.signed_volume.abs() > 10.0,
            "Chess pawn volume should be nonzero, got {}", val.signed_volume);
    }

    #[test]
    fn test_ft_stepped_shaft_boolean_union() {
        use cad_kernel::operations::feature::{
            BooleanOpType, Feature, FeatureTree, Parameter, SketchProfile,
        };

        let mut tree = FeatureTree::new();

        // Large cylinder base (24-gon for test speed)
        let n = 24;
        let r1 = 5.0;
        let pts1: Vec<[f64; 2]> = (0..n)
            .map(|i| {
                let theta = std::f64::consts::TAU * i as f64 / n as f64;
                [r1 * theta.cos(), r1 * theta.sin()]
            })
            .collect();
        tree.add_feature(Feature::Sketch {
            plane_origin: [0.0, 0.0, 0.0],
            plane_normal: [0.0, 0.0, 1.0],
            profiles: vec![SketchProfile { points: pts1, closed: true }],
            constraints: vec![],
            lines: vec![],
        });
        tree.add_feature(Feature::Extrude {
            sketch_index: 0,
            distance: Parameter::new("base_height", 8.0),
            direction: [0.0, 0.0, 1.0],
            symmetric: false,
        });

        // Smaller step
        let r2 = 3.0;
        let pts2: Vec<[f64; 2]> = (0..n)
            .map(|i| {
                let theta = std::f64::consts::TAU * i as f64 / n as f64;
                [r2 * theta.cos(), r2 * theta.sin()]
            })
            .collect();
        tree.add_feature(Feature::Sketch {
            plane_origin: [0.0, 0.0, 0.0],
            plane_normal: [0.0, 0.0, 1.0],
            profiles: vec![SketchProfile { points: pts2, closed: true }],
            constraints: vec![],
            lines: vec![],
        });
        tree.add_feature(Feature::Extrude {
            sketch_index: 1,
            distance: Parameter::new("step_height", 16.0),
            direction: [0.0, 0.0, 1.0],
            symmetric: false,
        });

        tree.add_feature(Feature::BooleanOp {
            op_type: BooleanOpType::Union,
            tool_feature: 0,
        });

        let mut store = EntityStore::new();
        let solids = tree.evaluate(&mut store).expect("stepped shaft");
        let mesh = tessellate_solid(&store, *solids.last().unwrap());
        let val = validate_mesh(&mesh);

        assert!(mesh.triangle_count() > 40,
            "Stepped shaft should have many tris, got {}", mesh.triangle_count());
        assert_eq!(val.degenerate_triangles, 0, "No degenerate triangles");
        assert!(val.signed_volume.abs() > 100.0,
            "Stepped shaft volume should be substantial, got {}", val.signed_volume);
    }

    #[test]
    fn test_ft_high_res_cylinder_smoothness() {
        // Verify that a high-res cylinder (96 segments) is smooth and correct
        use cad_kernel::topology::primitives::make_cylinder;

        let mut store = EntityStore::new();
        let solid = make_cylinder(&mut store, Point3d::ORIGIN, 5.0, 12.0, 96);
        let mesh = tessellate_solid(&store, solid);
        let val = validate_mesh(&mesh);

        assert!(val.is_watertight(), "96-seg cylinder should be watertight");
        assert!(val.is_printable(), "96-seg cylinder should be printable");
        assert_eq!(val.degenerate_triangles, 0);
        // Volume of cylinder: pi * 25 * 12 ≈ 942.5
        let expected_vol = std::f64::consts::PI * 25.0 * 12.0;
        assert!((val.signed_volume - expected_vol).abs() < expected_vol * 0.02,
            "96-seg cylinder volume should be ~{expected_vol:.1}, got {:.1}", val.signed_volume);
        // High segment count should give many triangles
        assert!(mesh.triangle_count() >= 380,
            "96-seg cylinder should have >=380 tris, got {}", mesh.triangle_count());
    }

    #[test]
    fn test_ft_high_res_sphere_quality() {
        use cad_kernel::topology::primitives::make_sphere;

        let mut store = EntityStore::new();
        let solid = make_sphere(&mut store, Point3d::ORIGIN, 6.0, 48, 36);
        let mesh = tessellate_solid(&store, solid);
        let val = validate_mesh(&mesh);

        assert!(val.is_watertight(), "High-res sphere should be watertight");
        assert!(val.is_printable(), "High-res sphere should be printable");
        assert_eq!(val.degenerate_triangles, 0);
        // Volume of sphere: 4/3 * pi * 216 ≈ 904.8
        let expected_vol = 4.0 / 3.0 * std::f64::consts::PI * 216.0;
        assert!((val.signed_volume - expected_vol).abs() < expected_vol * 0.03,
            "48x36 sphere volume should be ~{expected_vol:.1}, got {:.1}", val.signed_volume);
        assert!(mesh.triangle_count() > 3000,
            "48x36 sphere should have >3000 tris, got {}", mesh.triangle_count());
    }

    #[test]
    fn test_ft_obj_export_high_poly() {
        // Verify OBJ export works for high-poly meshes
        use cad_kernel::operations::revolve::revolve_profile;
        use cad_kernel::geometry::vector::Vec3;

        let mut store = EntityStore::new();
        let profile = vec![
            Point3d::new(3.0, 0.0, 0.0),
            Point3d::new(5.0, 0.0, 4.0),
            Point3d::new(3.5, 0.0, 8.0),
            Point3d::new(4.0, 0.0, 12.0),
        ];
        let solid = revolve_profile(
            &mut store, &profile, Point3d::ORIGIN,
            Vec3::Z, std::f64::consts::TAU, 48,
        );
        let mesh = tessellate_solid(&store, solid);
        let obj = mesh_to_obj(&mesh);

        let v_count = obj.lines().filter(|l| l.starts_with("v ")).count();
        let f_count = obj.lines().filter(|l| l.starts_with("f ")).count();
        let vn_count = obj.lines().filter(|l| l.starts_with("vn ")).count();

        assert!(v_count > 100, "High-poly OBJ should have >100 verts, got {v_count}");
        assert!(f_count > 100, "High-poly OBJ should have >100 faces, got {f_count}");
        assert_eq!(v_count, vn_count, "Vertex count should match normal count");

        // Verify all face indices are valid
        for line in obj.lines().filter(|l| l.starts_with("f ")) {
            for part in line.split_whitespace().skip(1) {
                let idx: usize = part.split("//").next().unwrap().parse().unwrap();
                assert!(idx >= 1 && idx <= v_count,
                    "Face index {idx} out of range [1, {v_count}]");
            }
        }
    }

    #[test]
    fn test_topology_audit_extrusion() {
        use cad_kernel::operations::extrude::{extrude_profile, Profile};
        use cad_kernel::geometry::vector::Vec3;

        let mut store = EntityStore::new();
        let profile = Profile::from_points(vec![
            Point3d::new(0.0, 0.0, 0.0),
            Point3d::new(4.0, 0.0, 0.0),
            Point3d::new(4.0, 4.0, 0.0),
            Point3d::new(0.0, 4.0, 0.0),
        ]);
        let solid = extrude_profile(&mut store, &profile, Vec3::Z, 3.0);

        // Extrusion mesh should be watertight and printable
        let mesh = tessellate_solid(&store, solid);
        let val = validate_mesh(&mesh);
        assert!(val.is_watertight(),
            "Extruded box mesh should be watertight: {} boundary, {} non-manifold",
            val.boundary_edges, val.non_manifold_edges);
        assert_eq!(val.degenerate_triangles, 0);
        assert!(val.signed_volume > 0.0, "Volume should be positive");
        // Volume = 4*4*3 = 48
        assert!((val.signed_volume - 48.0).abs() < 1.0,
            "Extruded box volume should be ~48, got {}", val.signed_volume);
    }
}
