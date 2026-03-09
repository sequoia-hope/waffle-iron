// Box-extrude pipeline tests for WaffleKernel.
// This file will be included into waffle_kernel.rs as:
//   #[cfg(test)] mod box_extrude_tests { include!("waffle_kernel_tests.rs"); }
//
// Tests exercise the full pipeline:
//   make_faces_from_profiles → extrude_face → tessellate → extract_edges → introspection

use super::*;
use crate::traits::{Kernel, KernelIntrospect};

// ── Test helpers ──────────────────────────────────────────────

/// Create a rectangular profile centered at (cx, cy) with width w and height h.
/// Returns (profiles, positions) suitable for make_faces_from_profiles.
///
/// Vertices are numbered 1..4, edges (entity_ids) are 10..13.
/// Winding: CCW when viewed from +Z (outer profile).
fn make_rect_profile(
    cx: f64,
    cy: f64,
    w: f64,
    h: f64,
) -> (Vec<ClosedProfile>, HashMap<u32, (f64, f64)>) {
    // 4 corners CCW: bottom-left, bottom-right, top-right, top-left
    let mut positions = HashMap::new();
    positions.insert(1, (cx - w / 2.0, cy - h / 2.0));
    positions.insert(2, (cx + w / 2.0, cy - h / 2.0));
    positions.insert(3, (cx + w / 2.0, cy + h / 2.0));
    positions.insert(4, (cx - w / 2.0, cy + h / 2.0));

    let profile = ClosedProfile {
        entity_ids: vec![10, 11, 12, 13],
        is_outer: true,
        circle: None,
        spline_segments: vec![],
    };

    (vec![profile], positions)
}

/// Compute mesh volume using the divergence theorem (signed volume of tetrahedra).
/// For a closed, consistently-oriented mesh, this equals the enclosed volume.
fn mesh_volume(mesh: &RenderMesh) -> f64 {
    let mut vol = 0.0;
    let n_tris = mesh.indices.len() / 3;
    for i in 0..n_tris {
        let i0 = mesh.indices[i * 3] as usize;
        let i1 = mesh.indices[i * 3 + 1] as usize;
        let i2 = mesh.indices[i * 3 + 2] as usize;

        let v0 = [
            mesh.vertices[i0 * 3] as f64,
            mesh.vertices[i0 * 3 + 1] as f64,
            mesh.vertices[i0 * 3 + 2] as f64,
        ];
        let v1 = [
            mesh.vertices[i1 * 3] as f64,
            mesh.vertices[i1 * 3 + 1] as f64,
            mesh.vertices[i1 * 3 + 2] as f64,
        ];
        let v2 = [
            mesh.vertices[i2 * 3] as f64,
            mesh.vertices[i2 * 3 + 1] as f64,
            mesh.vertices[i2 * 3 + 2] as f64,
        ];

        // Signed volume of tetrahedron formed by triangle and origin
        vol += v0[0] * (v1[1] * v2[2] - v2[1] * v1[2]) - v1[0] * (v0[1] * v2[2] - v2[1] * v0[2])
            + v2[0] * (v0[1] * v1[2] - v1[1] * v0[2]);
    }
    (vol / 6.0).abs()
}

/// Check if a mesh is watertight (every edge shared by exactly 2 triangles).
///
/// Uses position-based edge matching (not index-based) because the tessellation
/// may produce per-face vertices (non-shared). Vertex positions are quantized
/// to 1e-6 to form canonical edge keys.
fn check_watertight(mesh: &RenderMesh) -> bool {
    use std::collections::HashMap as Map;

    /// Quantize a vertex position to an integer triple for hashing.
    fn quantize(mesh: &RenderMesh, idx: u32) -> (i64, i64, i64) {
        let base = idx as usize * 3;
        (
            (mesh.vertices[base] as f64 * 1e6).round() as i64,
            (mesh.vertices[base + 1] as f64 * 1e6).round() as i64,
            (mesh.vertices[base + 2] as f64 * 1e6).round() as i64,
        )
    }

    let mut edge_count: Map<((i64, i64, i64), (i64, i64, i64)), u32> = Map::new();
    let n_tris = mesh.indices.len() / 3;
    for i in 0..n_tris {
        let tri = [
            mesh.indices[i * 3],
            mesh.indices[i * 3 + 1],
            mesh.indices[i * 3 + 2],
        ];
        for j in 0..3 {
            let pa = quantize(mesh, tri[j]);
            let pb = quantize(mesh, tri[(j + 1) % 3]);
            // Canonical ordering so (A,B) and (B,A) map to the same key
            let key = if pa <= pb { (pa, pb) } else { (pb, pa) };
            *edge_count.entry(key).or_insert(0) += 1;
        }
    }
    edge_count.values().all(|&c| c == 2)
}

/// Compute axis-aligned bounding box of a mesh.
/// Returns (min_corner, max_corner).
fn mesh_bbox(mesh: &RenderMesh) -> ([f64; 3], [f64; 3]) {
    let n = mesh.vertices.len() / 3;
    if n == 0 {
        return ([0.0; 3], [0.0; 3]);
    }
    let mut min = [f64::INFINITY; 3];
    let mut max = [f64::NEG_INFINITY; 3];
    for i in 0..n {
        for j in 0..3 {
            let v = mesh.vertices[i * 3 + j] as f64;
            if v < min[j] {
                min[j] = v;
            }
            if v > max[j] {
                max[j] = v;
            }
        }
    }
    (min, max)
}

/// Count unique vertex positions in a mesh (by quantized position).
fn unique_vertex_positions(mesh: &RenderMesh) -> usize {
    use std::collections::HashSet;
    let mut seen = HashSet::new();
    let n = mesh.vertices.len() / 3;
    for i in 0..n {
        let key = (
            (mesh.vertices[i * 3] as f64 * 1e6).round() as i64,
            (mesh.vertices[i * 3 + 1] as f64 * 1e6).round() as i64,
            (mesh.vertices[i * 3 + 2] as f64 * 1e6).round() as i64,
        );
        seen.insert(key);
    }
    seen.len()
}

/// Standard XY plane at origin for most tests.
const XY_ORIGIN: [f64; 3] = [0.0, 0.0, 0.0];
const XY_NORMAL: [f64; 3] = [0.0, 0.0, 1.0];
const XY_X_AXIS: [f64; 3] = [1.0, 0.0, 0.0];
const Z_DIR: [f64; 3] = [0.0, 0.0, 1.0];

/// Helper: create a unit box (1x1x1 at origin) and return (kernel, solid_handle).
fn make_unit_box() -> (WaffleKernel, KernelSolidHandle) {
    let mut k = WaffleKernel::new();
    let (profiles, positions) = make_rect_profile(0.5, 0.5, 1.0, 1.0);
    let face_ids = k
        .make_faces_from_profiles(&profiles, XY_ORIGIN, XY_NORMAL, XY_X_AXIS, &positions)
        .expect("make_faces_from_profiles should succeed for unit rect");
    let solid = k
        .extrude_face(face_ids[0], Z_DIR, 1.0)
        .expect("extrude_face should succeed for unit box");
    (k, solid)
}

/// Helper: create a box with given width, height, and depth.
fn make_scaled_box(w: f64, h: f64, depth: f64) -> (WaffleKernel, KernelSolidHandle) {
    let mut k = WaffleKernel::new();
    let (profiles, positions) = make_rect_profile(w / 2.0, h / 2.0, w, h);
    let face_ids = k
        .make_faces_from_profiles(&profiles, XY_ORIGIN, XY_NORMAL, XY_X_AXIS, &positions)
        .expect("make_faces_from_profiles should succeed");
    let solid = k
        .extrude_face(face_ids[0], Z_DIR, depth)
        .expect("extrude_face should succeed");
    (k, solid)
}

// ── Group A: make_faces_from_profiles ──────────────────────────

#[test]
fn a1_unit_rect_produces_one_face() {
    let mut k = WaffleKernel::new();
    let (profiles, positions) = make_rect_profile(0.5, 0.5, 1.0, 1.0);
    let face_ids = k
        .make_faces_from_profiles(&profiles, XY_ORIGIN, XY_NORMAL, XY_X_AXIS, &positions)
        .expect("unit rect should produce faces");
    assert_eq!(
        face_ids.len(),
        1,
        "One outer profile should produce exactly 1 face"
    );
}

#[test]
fn a2_zero_width_rect_errors() {
    let mut k = WaffleKernel::new();
    let (profiles, positions) = make_rect_profile(0.0, 0.0, 0.0, 1.0);
    let result = k.make_faces_from_profiles(&profiles, XY_ORIGIN, XY_NORMAL, XY_X_AXIS, &positions);
    assert!(result.is_err(), "Zero-width rect should produce an error");
}

#[test]
fn a3_zero_height_rect_errors() {
    let mut k = WaffleKernel::new();
    let (profiles, positions) = make_rect_profile(0.0, 0.0, 1.0, 0.0);
    let result = k.make_faces_from_profiles(&profiles, XY_ORIGIN, XY_NORMAL, XY_X_AXIS, &positions);
    assert!(result.is_err(), "Zero-height rect should produce an error");
}

// ── Group B: extrude_face ──────────────────────────────────────

#[test]
fn b1_unit_box_has_8v_12e_6f() {
    let (k, solid) = make_unit_box();
    let verts = k.list_vertices(&solid);
    let edges = k.list_edges(&solid);
    let faces = k.list_faces(&solid);
    assert_eq!(verts.len(), 8, "Box must have 8 vertices");
    assert_eq!(edges.len(), 12, "Box must have 12 edges");
    assert_eq!(faces.len(), 6, "Box must have 6 faces");
}

#[test]
fn b2_unit_box_euler_formula() {
    let (k, solid) = make_unit_box();
    let v = k.list_vertices(&solid).len() as i64;
    let e = k.list_edges(&solid).len() as i64;
    let f = k.list_faces(&solid).len() as i64;
    assert_eq!(
        v - e + f,
        2,
        "Euler formula V-E+F must equal 2 for genus-0 solid (got V={}, E={}, F={})",
        v,
        e,
        f
    );
}

#[test]
fn b3_scaled_box_topology() {
    let (k, solid) = make_scaled_box(2.0, 3.0, 5.0);
    let verts = k.list_vertices(&solid);
    let edges = k.list_edges(&solid);
    let faces = k.list_faces(&solid);
    assert_eq!(verts.len(), 8);
    assert_eq!(edges.len(), 12);
    assert_eq!(faces.len(), 6);
}

#[test]
fn b4_extrude_invalid_face_errors() {
    let mut k = WaffleKernel::new();
    let result = k.extrude_face(KernelId(99999), Z_DIR, 1.0);
    assert!(
        result.is_err(),
        "Extruding a nonexistent face must return an error"
    );
    // Specifically should be EntityNotFound
    if let Err(ref e) = result {
        assert!(
            matches!(e, KernelError::EntityNotFound { .. }),
            "Expected EntityNotFound, got {:?}",
            e
        );
    }
}

#[test]
fn b5_extrude_zero_depth_errors() {
    let mut k = WaffleKernel::new();
    let (profiles, positions) = make_rect_profile(0.5, 0.5, 1.0, 1.0);
    let face_ids = k
        .make_faces_from_profiles(&profiles, XY_ORIGIN, XY_NORMAL, XY_X_AXIS, &positions)
        .unwrap();
    let result = k.extrude_face(face_ids[0], Z_DIR, 0.0);
    assert!(result.is_err(), "Zero-depth extrude must produce an error");
}

// ── Group C: tessellate ────────────────────────────────────────

#[test]
fn c1_unit_box_volume() {
    let (mut k, solid) = make_unit_box();
    let mesh = k
        .tessellate(&solid, 0.01)
        .expect("tessellate should succeed for unit box");
    let vol = mesh_volume(&mesh);
    assert!(
        (vol - 1.0).abs() < 0.01,
        "Unit box volume should be ~1.0, got {}",
        vol
    );
}

#[test]
fn c2_scaled_box_volume() {
    let (mut k, solid) = make_scaled_box(2.0, 3.0, 5.0);
    let mesh = k
        .tessellate(&solid, 0.01)
        .expect("tessellate should succeed");
    let vol = mesh_volume(&mesh);
    assert!(
        (vol - 30.0).abs() < 0.3,
        "2x3x5 box volume should be ~30.0, got {}",
        vol
    );
}

#[test]
fn c3_unit_box_watertight() {
    let (mut k, solid) = make_unit_box();
    let mesh = k.tessellate(&solid, 0.01).unwrap();
    assert!(
        check_watertight(&mesh),
        "Unit box mesh must be watertight (every edge shared by exactly 2 triangles)"
    );
}

#[test]
fn c4_unit_box_bbox() {
    let (mut k, solid) = make_unit_box();
    let mesh = k.tessellate(&solid, 0.01).unwrap();
    let (min, max) = mesh_bbox(&mesh);
    let tol = 0.01;
    assert!(min[0].abs() < tol, "bbox min x ~ 0, got {}", min[0]);
    assert!(min[1].abs() < tol, "bbox min y ~ 0, got {}", min[1]);
    assert!(min[2].abs() < tol, "bbox min z ~ 0, got {}", min[2]);
    assert!((max[0] - 1.0).abs() < tol, "bbox max x ~ 1, got {}", max[0]);
    assert!((max[1] - 1.0).abs() < tol, "bbox max y ~ 1, got {}", max[1]);
    assert!((max[2] - 1.0).abs() < tol, "bbox max z ~ 1, got {}", max[2]);
}

#[test]
fn c5_unit_box_6_face_ranges() {
    let (mut k, solid) = make_unit_box();
    let mesh = k.tessellate(&solid, 0.01).unwrap();
    assert_eq!(
        mesh.face_ranges.len(),
        6,
        "Box mesh should have 6 face ranges"
    );
    // Each rectangular face needs at least 2 triangles
    for fr in &mesh.face_ranges {
        let tri_count = (fr.end_index - fr.start_index) / 3;
        assert!(
            tri_count >= 2,
            "Each face should have >= 2 triangles (quad fan), got {}",
            tri_count
        );
    }
}

#[test]
fn c6_unit_box_normals_present() {
    let (mut k, solid) = make_unit_box();
    let mesh = k.tessellate(&solid, 0.01).unwrap();
    // Normals array should match vertices array in length
    assert_eq!(
        mesh.normals.len(),
        mesh.vertices.len(),
        "Normals count must match vertices count"
    );
    // All normals should be unit-length (within tolerance)
    let n_verts = mesh.normals.len() / 3;
    for i in 0..n_verts {
        let nx = mesh.normals[i * 3] as f64;
        let ny = mesh.normals[i * 3 + 1] as f64;
        let nz = mesh.normals[i * 3 + 2] as f64;
        let len = (nx * nx + ny * ny + nz * nz).sqrt();
        assert!(
            (len - 1.0).abs() < 0.01,
            "Normal at vertex {} should be unit length, got {}",
            i,
            len
        );
    }
}

#[test]
fn c7_mesh_has_8_unique_positions() {
    let (mut k, solid) = make_unit_box();
    let mesh = k.tessellate(&solid, 0.01).unwrap();
    let unique = unique_vertex_positions(&mesh);
    assert_eq!(
        unique, 8,
        "Box mesh should have exactly 8 unique vertex positions, got {}",
        unique
    );
}

// ── Group D: extract_edges ─────────────────────────────────────

#[test]
fn d1_unit_box_12_edge_ranges() {
    let (mut k, solid) = make_unit_box();
    let edges = k
        .extract_edges(&solid, 0.01)
        .expect("extract_edges should succeed");
    assert_eq!(
        edges.edge_ranges.len(),
        12,
        "Box should have 12 edge ranges"
    );
}

// ── Group E: introspection ─────────────────────────────────────

#[test]
fn e1_list_faces_returns_6() {
    let (k, solid) = make_unit_box();
    let faces = k.list_faces(&solid);
    assert_eq!(faces.len(), 6);
}

#[test]
fn e2_each_face_has_4_neighbors() {
    let (k, solid) = make_unit_box();
    let faces = k.list_faces(&solid);
    for face in &faces {
        let neighbors = k.face_neighbors(*face);
        assert_eq!(
            neighbors.len(),
            4,
            "Box face {:?} should have 4 neighbors, got {}",
            face,
            neighbors.len()
        );
    }
}

#[test]
fn e3_each_face_has_4_edges() {
    let (k, solid) = make_unit_box();
    let faces = k.list_faces(&solid);
    for face in &faces {
        let edges = k.face_edges(*face);
        assert_eq!(
            edges.len(),
            4,
            "Box face {:?} should have 4 edges, got {}",
            face,
            edges.len()
        );
    }
}

#[test]
fn e4_each_edge_has_2_faces() {
    let (k, solid) = make_unit_box();
    let edges = k.list_edges(&solid);
    for edge in &edges {
        let faces = k.edge_faces(*edge);
        assert_eq!(
            faces.len(),
            2,
            "Box edge {:?} should have exactly 2 adjacent faces, got {}",
            edge,
            faces.len()
        );
    }
}

#[test]
fn e5_introspect_empty_for_invalid_handle() {
    let k = WaffleKernel::new();
    let handle = KernelSolidHandle(99999);
    assert!(k.list_faces(&handle).is_empty());
    assert!(k.list_edges(&handle).is_empty());
    assert!(k.list_vertices(&handle).is_empty());
}

// ── Group F: scale coverage ────────────────────────────────────

#[test]
fn f1_micro_box_volume() {
    let (mut k, solid) = make_scaled_box(1e-4, 1e-4, 1e-4);
    let mesh = k
        .tessellate(&solid, 1e-6)
        .expect("tessellate should succeed for micro box");
    let vol = mesh_volume(&mesh);
    let expected = 1e-12;
    let rel_err = (vol - expected).abs() / expected;
    assert!(
        rel_err < 0.01,
        "Micro box volume should be ~1e-12, got {} (rel_err={})",
        vol,
        rel_err
    );
}

#[test]
fn f2_macro_box_volume() {
    let (mut k, solid) = make_scaled_box(1e3, 1e3, 1e3);
    let mesh = k
        .tessellate(&solid, 1.0)
        .expect("tessellate should succeed for macro box");
    let vol = mesh_volume(&mesh);
    let expected = 1e9;
    let rel_err = (vol - expected).abs() / expected;
    assert!(
        rel_err < 0.01,
        "Macro box volume should be ~1e9, got {} (rel_err={})",
        vol,
        rel_err
    );
}

// ━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━
// Milestone 2: Circle-extrude (cylinder) pipeline tests
// ━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━

use std::f64::consts::PI;

// ── Circle helpers ─────────────────────────────────────────────

/// Create a circle profile for testing.
/// Returns (profiles, positions) suitable for make_faces_from_profiles.
fn make_circle_profile(
    cx: f64,
    cy: f64,
    r: f64,
) -> (Vec<ClosedProfile>, HashMap<u32, (f64, f64)>) {
    let mut positions = HashMap::new();
    positions.insert(1, (cx, cy));

    let profile = ClosedProfile {
        entity_ids: vec![1],
        is_outer: true,
        circle: Some(CircleProfile {
            center_u: cx,
            center_v: cy,
            radius: r,
        }),
        spline_segments: vec![],
    };

    (vec![profile], positions)
}

/// Helper: create a unit cylinder (r=1, depth=1) on XY plane.
fn make_unit_cylinder() -> (WaffleKernel, KernelSolidHandle) {
    let mut k = WaffleKernel::new();
    let (profiles, positions) = make_circle_profile(0.0, 0.0, 1.0);
    let face_ids = k
        .make_faces_from_profiles(&profiles, XY_ORIGIN, XY_NORMAL, XY_X_AXIS, &positions)
        .expect("make_faces_from_profiles should succeed for unit circle");
    let solid = k
        .extrude_face(face_ids[0], Z_DIR, 1.0)
        .expect("extrude_face should succeed for unit cylinder");
    (k, solid)
}

/// Helper: create a cylinder with given radius and depth on XY plane.
fn make_cylinder(r: f64, depth: f64) -> (WaffleKernel, KernelSolidHandle) {
    let mut k = WaffleKernel::new();
    let (profiles, positions) = make_circle_profile(0.0, 0.0, r);
    let face_ids = k
        .make_faces_from_profiles(&profiles, XY_ORIGIN, XY_NORMAL, XY_X_AXIS, &positions)
        .expect("make_faces_from_profiles should succeed");
    let solid = k
        .extrude_face(face_ids[0], Z_DIR, depth)
        .expect("extrude_face should succeed");
    (k, solid)
}

// ── Group CA: make_faces_from_profiles (circles) ───────────────

#[test]
fn ca1_circle_produces_one_face() {
    let mut k = WaffleKernel::new();
    let (profiles, positions) = make_circle_profile(0.0, 0.0, 5.0);
    let face_ids = k
        .make_faces_from_profiles(&profiles, XY_ORIGIN, XY_NORMAL, XY_X_AXIS, &positions)
        .expect("CircleProfile r=5 should produce faces");
    assert_eq!(
        face_ids.len(),
        1,
        "One circle profile should produce exactly 1 face"
    );
}

#[test]
fn ca2_zero_radius_errors() {
    let mut k = WaffleKernel::new();
    let (profiles, positions) = make_circle_profile(0.0, 0.0, 0.0);
    let result =
        k.make_faces_from_profiles(&profiles, XY_ORIGIN, XY_NORMAL, XY_X_AXIS, &positions);
    assert!(result.is_err(), "Zero-radius circle should produce an error");
}

#[test]
fn ca3_negative_radius_errors() {
    let mut k = WaffleKernel::new();
    let (profiles, positions) = make_circle_profile(0.0, 0.0, -1.0);
    let result =
        k.make_faces_from_profiles(&profiles, XY_ORIGIN, XY_NORMAL, XY_X_AXIS, &positions);
    assert!(
        result.is_err(),
        "Negative-radius circle should produce an error"
    );
}

// ── Group CB: extrude_face (cylinder topology) ─────────────────

#[test]
fn cb1_unit_cylinder_topology() {
    let (k, solid) = make_unit_cylinder();
    let verts = k.list_vertices(&solid);
    let edges = k.list_edges(&solid);
    let faces = k.list_faces(&solid);
    assert_eq!(verts.len(), 2, "Cylinder must have 2 vertices (pole points)");
    assert_eq!(edges.len(), 3, "Cylinder must have 3 edges (2 circles + 1 seam)");
    assert_eq!(faces.len(), 3, "Cylinder must have 3 faces (top + bottom + lateral)");
}

#[test]
fn cb2_unit_cylinder_euler() {
    let (k, solid) = make_unit_cylinder();
    let v = k.list_vertices(&solid).len() as i64;
    let e = k.list_edges(&solid).len() as i64;
    let f = k.list_faces(&solid).len() as i64;
    assert_eq!(
        v - e + f,
        2,
        "Euler formula V-E+F must equal 2 for cylinder (got V={}, E={}, F={})",
        v,
        e,
        f
    );
}

#[test]
fn cb3_scaled_cylinder_topology() {
    let (k, solid) = make_cylinder(5.0, 10.0);
    let verts = k.list_vertices(&solid);
    let edges = k.list_edges(&solid);
    let faces = k.list_faces(&solid);
    assert_eq!(verts.len(), 2, "Scaled cylinder must have 2 vertices");
    assert_eq!(edges.len(), 3, "Scaled cylinder must have 3 edges");
    assert_eq!(faces.len(), 3, "Scaled cylinder must have 3 faces");
}

// ── Group CC: tessellate (cylinder) ────────────────────────────

#[test]
fn cc1_unit_cylinder_volume() {
    let (mut k, solid) = make_unit_cylinder();
    let mesh = k
        .tessellate(&solid, 0.01)
        .expect("tessellate should succeed for unit cylinder");
    let vol = mesh_volume(&mesh);
    let expected = PI; // π·r²·h = π·1·1
    let rel_err = (vol - expected).abs() / expected;
    assert!(
        rel_err < 0.01,
        "Unit cylinder volume should be ~π ({:.6}), got {:.6} (rel_err={:.4})",
        expected,
        vol,
        rel_err
    );
}

#[test]
fn cc2_simple_cylinder_volume() {
    let (mut k, solid) = make_cylinder(5.0, 10.0);
    let mesh = k
        .tessellate(&solid, 0.01)
        .expect("tessellate should succeed");
    let vol = mesh_volume(&mesh);
    let expected = 250.0 * PI; // π·25·10
    let rel_err = (vol - expected).abs() / expected;
    assert!(
        rel_err < 0.01,
        "Cylinder r=5 d=10 volume should be ~250π ({:.2}), got {:.2} (rel_err={:.4})",
        expected,
        vol,
        rel_err
    );
}

#[test]
fn cc3_tall_rod_volume() {
    let (mut k, solid) = make_cylinder(1.0, 100.0);
    let mesh = k
        .tessellate(&solid, 0.01)
        .expect("tessellate should succeed");
    let vol = mesh_volume(&mesh);
    let expected = 100.0 * PI; // π·1·100
    let rel_err = (vol - expected).abs() / expected;
    assert!(
        rel_err < 0.01,
        "Tall rod volume should be ~100π ({:.2}), got {:.2} (rel_err={:.4})",
        expected,
        vol,
        rel_err
    );
}

#[test]
fn cc4_wide_short_volume() {
    let (mut k, solid) = make_cylinder(10.0, 1.0);
    let mesh = k
        .tessellate(&solid, 0.01)
        .expect("tessellate should succeed");
    let vol = mesh_volume(&mesh);
    let expected = 100.0 * PI; // π·100·1
    let rel_err = (vol - expected).abs() / expected;
    assert!(
        rel_err < 0.01,
        "Wide short cylinder volume should be ~100π ({:.2}), got {:.2} (rel_err={:.4})",
        expected,
        vol,
        rel_err
    );
}

#[test]
fn cc5_cylinder_watertight() {
    let (mut k, solid) = make_unit_cylinder();
    let mesh = k.tessellate(&solid, 0.01).unwrap();
    assert!(
        check_watertight(&mesh),
        "Cylinder mesh must be watertight (every edge shared by exactly 2 triangles)"
    );
}

#[test]
fn cc6_cylinder_bbox() {
    let r = 5.0;
    let d = 10.0;
    let (mut k, solid) = make_cylinder(r, d);
    let mesh = k.tessellate(&solid, 0.01).unwrap();
    let (min, max) = mesh_bbox(&mesh);
    let tol = 0.5; // cylinder tessellation with N=64 won't hit exact r at the bbox
    assert!((min[0] - (-r)).abs() < tol, "bbox min x ~ -{}, got {}", r, min[0]);
    assert!((min[1] - (-r)).abs() < tol, "bbox min y ~ -{}, got {}", r, min[1]);
    assert!(min[2].abs() < tol, "bbox min z ~ 0, got {}", min[2]);
    assert!((max[0] - r).abs() < tol, "bbox max x ~ {}, got {}", r, max[0]);
    assert!((max[1] - r).abs() < tol, "bbox max y ~ {}, got {}", r, max[1]);
    assert!((max[2] - d).abs() < tol, "bbox max z ~ {}, got {}", d, max[2]);
}

#[test]
fn cc7_cylinder_3_face_ranges() {
    let (mut k, solid) = make_unit_cylinder();
    let mesh = k.tessellate(&solid, 0.01).unwrap();
    assert_eq!(
        mesh.face_ranges.len(),
        3,
        "Cylinder mesh should have 3 face ranges (top + bottom + lateral)"
    );
}

// ── Group CD: extract_edges (cylinder) ─────────────────────────

#[test]
fn cd1_cylinder_3_edge_ranges() {
    let (mut k, solid) = make_unit_cylinder();
    let edges = k
        .extract_edges(&solid, 0.01)
        .expect("extract_edges should succeed for cylinder");
    assert_eq!(
        edges.edge_ranges.len(),
        3,
        "Cylinder should have 3 edge ranges (2 circles + 1 seam)"
    );
}

#[test]
fn cd2_circular_edge_has_many_vertices() {
    let (mut k, solid) = make_unit_cylinder();
    let edges = k
        .extract_edges(&solid, 0.01)
        .expect("extract_edges should succeed");
    // At least one edge range should span many vertices (a circle polyline, N>=16)
    let has_circle_edge = edges.edge_ranges.iter().any(|er| {
        let vert_count = (er.end_vertex - er.start_vertex) / 3;
        vert_count >= 16
    });
    assert!(
        has_circle_edge,
        "At least one edge should be a circular polyline with ≥16 vertices"
    );
}

// ── Group CE: introspection (cylinder) ─────────────────────────

#[test]
fn ce1_list_faces_returns_3() {
    let (k, solid) = make_unit_cylinder();
    let faces = k.list_faces(&solid);
    assert_eq!(faces.len(), 3, "Cylinder should have 3 faces");
}

#[test]
fn ce2_list_edges_returns_3() {
    let (k, solid) = make_unit_cylinder();
    let edges = k.list_edges(&solid);
    assert_eq!(edges.len(), 3, "Cylinder should have 3 edges");
}

// ── Group CF: scale coverage (cylinder) ────────────────────────

#[test]
fn cf1_micro_cylinder_volume() {
    let (mut k, solid) = make_cylinder(1e-4, 1e-4);
    let mesh = k
        .tessellate(&solid, 1e-6)
        .expect("tessellate should succeed for micro cylinder");
    let vol = mesh_volume(&mesh);
    let expected = PI * 1e-12; // π·(1e-4)²·(1e-4)
    let rel_err = (vol - expected).abs() / expected;
    assert!(
        rel_err < 0.01,
        "Micro cylinder volume should be ~π×1e-12 ({:.6e}), got {:.6e} (rel_err={:.4})",
        expected,
        vol,
        rel_err
    );
}

#[test]
fn cf2_macro_cylinder_volume() {
    let (mut k, solid) = make_cylinder(1e3, 1e3);
    let mesh = k
        .tessellate(&solid, 1.0)
        .expect("tessellate should succeed for macro cylinder");
    let vol = mesh_volume(&mesh);
    let expected = PI * 1e9; // π·(1e3)²·(1e3)
    let rel_err = (vol - expected).abs() / expected;
    assert!(
        rel_err < 0.01,
        "Macro cylinder volume should be ~π×1e9 ({:.2e}), got {:.2e} (rel_err={:.4})",
        expected,
        vol,
        rel_err
    );
}
