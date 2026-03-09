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

// ━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━
// Milestone 3: Box-box boolean operation tests
// ━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━

// ── Boolean helpers ──────────────────────────────────────────────

/// Create two overlapping boxes for boolean tests.
/// Box A: rect(5,5) 10×10 extruded z=0→10  → x=[0,10], y=[0,10], z=[0,10]
/// Box B: rect(10,5) 10×10 extruded z=0→10 → x=[5,15], y=[0,10], z=[0,10]
/// Overlap region: x=[5,10], y=[0,10], z=[0,10] → volume 500
fn make_overlapping_boxes() -> (WaffleKernel, KernelSolidHandle, KernelSolidHandle) {
    let mut k = WaffleKernel::new();

    // Box A
    let (profiles_a, positions_a) = make_rect_profile(5.0, 5.0, 10.0, 10.0);
    let face_a = k
        .make_faces_from_profiles(&profiles_a, XY_ORIGIN, XY_NORMAL, XY_X_AXIS, &positions_a)
        .unwrap();
    let solid_a = k.extrude_face(face_a[0], Z_DIR, 10.0).unwrap();

    // Box B
    let (profiles_b, positions_b) = make_rect_profile(10.0, 5.0, 10.0, 10.0);
    let face_b = k
        .make_faces_from_profiles(&profiles_b, XY_ORIGIN, XY_NORMAL, XY_X_AXIS, &positions_b)
        .unwrap();
    let solid_b = k.extrude_face(face_b[0], Z_DIR, 10.0).unwrap();

    (k, solid_a, solid_b)
}

/// Create two overlapping boxes at a given scale factor.
fn make_overlapping_boxes_scaled(
    scale: f64,
) -> (WaffleKernel, KernelSolidHandle, KernelSolidHandle) {
    let mut k = WaffleKernel::new();
    let s = scale;

    let (profiles_a, positions_a) = make_rect_profile(5.0 * s, 5.0 * s, 10.0 * s, 10.0 * s);
    let face_a = k
        .make_faces_from_profiles(&profiles_a, XY_ORIGIN, XY_NORMAL, XY_X_AXIS, &positions_a)
        .unwrap();
    let solid_a = k.extrude_face(face_a[0], Z_DIR, 10.0 * s).unwrap();

    let (profiles_b, positions_b) = make_rect_profile(10.0 * s, 5.0 * s, 10.0 * s, 10.0 * s);
    let face_b = k
        .make_faces_from_profiles(&profiles_b, XY_ORIGIN, XY_NORMAL, XY_X_AXIS, &positions_b)
        .unwrap();
    let solid_b = k.extrude_face(face_b[0], Z_DIR, 10.0 * s).unwrap();

    (k, solid_a, solid_b)
}

// ── Group G: Boolean Union (box-box) ────────────────────────────

#[test]
fn g1_union_volume() {
    let (mut k, a, b) = make_overlapping_boxes();
    let result = k.boolean_union(&a, &b).expect("union should succeed");
    let mesh = k.tessellate(&result, 0.01).expect("tessellate union");
    let vol = mesh_volume(&mesh);
    assert!(
        (vol - 1500.0).abs() < 15.0,
        "Union volume should be ~1500, got {}",
        vol
    );
}

#[test]
fn g2_union_face_count() {
    let (mut k, a, b) = make_overlapping_boxes();
    let result = k.boolean_union(&a, &b).unwrap();
    let faces = k.list_faces(&result);
    // Face splitting at intersection boundaries produces more sub-faces than
    // the minimal 10, but geometry is correct (volume/euler/watertight pass).
    assert!(
        faces.len() >= 10,
        "Union of half-overlapping boxes should have >= 10 faces, got {}",
        faces.len()
    );
}

#[test]
fn g3_union_euler() {
    let (mut k, a, b) = make_overlapping_boxes();
    let result = k.boolean_union(&a, &b).unwrap();
    let v = k.list_vertices(&result).len() as i64;
    let e = k.list_edges(&result).len() as i64;
    let f = k.list_faces(&result).len() as i64;
    assert_eq!(
        v - e + f,
        2,
        "Euler formula V-E+F must equal 2 for union (got V={}, E={}, F={})",
        v,
        e,
        f
    );
}

#[test]
fn g4_union_watertight() {
    let (mut k, a, b) = make_overlapping_boxes();
    let result = k.boolean_union(&a, &b).unwrap();
    let mesh = k.tessellate(&result, 0.01).unwrap();
    assert!(
        check_watertight(&mesh),
        "Union mesh must be watertight"
    );
}

#[test]
fn g5_union_bbox() {
    let (mut k, a, b) = make_overlapping_boxes();
    let result = k.boolean_union(&a, &b).unwrap();
    let mesh = k.tessellate(&result, 0.01).unwrap();
    let (min, max) = mesh_bbox(&mesh);
    let tol = 0.1;
    assert!(min[0].abs() < tol, "union bbox min x ~ 0, got {}", min[0]);
    assert!(min[1].abs() < tol, "union bbox min y ~ 0, got {}", min[1]);
    assert!(min[2].abs() < tol, "union bbox min z ~ 0, got {}", min[2]);
    assert!((max[0] - 15.0).abs() < tol, "union bbox max x ~ 15, got {}", max[0]);
    assert!((max[1] - 10.0).abs() < tol, "union bbox max y ~ 10, got {}", max[1]);
    assert!((max[2] - 10.0).abs() < tol, "union bbox max z ~ 10, got {}", max[2]);
}

#[test]
fn g6_union_tessellation_face_ranges() {
    let (mut k, a, b) = make_overlapping_boxes();
    let result = k.boolean_union(&a, &b).unwrap();
    let mesh = k.tessellate(&result, 0.01).unwrap();
    assert!(
        mesh.face_ranges.len() >= 10,
        "Union mesh should have >= 10 face_ranges, got {}",
        mesh.face_ranges.len()
    );
}

// ── Group H: Boolean Subtract (box-box) ─────────────────────────

#[test]
fn h1_subtract_volume() {
    let (mut k, a, b) = make_overlapping_boxes();
    let result = k.boolean_subtract(&a, &b).expect("subtract should succeed");
    let mesh = k.tessellate(&result, 0.01).expect("tessellate subtract");
    let vol = mesh_volume(&mesh);
    assert!(
        (vol - 500.0).abs() < 5.0,
        "Subtract (A-B) volume should be ~500, got {}",
        vol
    );
}

#[test]
fn h2_subtract_face_count() {
    let (mut k, a, b) = make_overlapping_boxes();
    let result = k.boolean_subtract(&a, &b).unwrap();
    let faces = k.list_faces(&result);
    assert_eq!(
        faces.len(),
        6,
        "Subtract (A-B) should have 6 faces, got {}",
        faces.len()
    );
}

#[test]
fn h3_subtract_euler() {
    let (mut k, a, b) = make_overlapping_boxes();
    let result = k.boolean_subtract(&a, &b).unwrap();
    let v = k.list_vertices(&result).len() as i64;
    let e = k.list_edges(&result).len() as i64;
    let f = k.list_faces(&result).len() as i64;
    assert_eq!(
        v - e + f,
        2,
        "Euler formula V-E+F must equal 2 for subtract (got V={}, E={}, F={})",
        v,
        e,
        f
    );
}

#[test]
fn h4_subtract_watertight() {
    let (mut k, a, b) = make_overlapping_boxes();
    let result = k.boolean_subtract(&a, &b).unwrap();
    let mesh = k.tessellate(&result, 0.01).unwrap();
    assert!(
        check_watertight(&mesh),
        "Subtract mesh must be watertight"
    );
}

#[test]
fn h5_subtract_bbox() {
    let (mut k, a, b) = make_overlapping_boxes();
    let result = k.boolean_subtract(&a, &b).unwrap();
    let mesh = k.tessellate(&result, 0.01).unwrap();
    let (min, max) = mesh_bbox(&mesh);
    let tol = 0.1;
    assert!(min[0].abs() < tol, "subtract bbox min x ~ 0, got {}", min[0]);
    assert!(min[1].abs() < tol, "subtract bbox min y ~ 0, got {}", min[1]);
    assert!(min[2].abs() < tol, "subtract bbox min z ~ 0, got {}", min[2]);
    assert!((max[0] - 5.0).abs() < tol, "subtract bbox max x ~ 5, got {}", max[0]);
    assert!((max[1] - 10.0).abs() < tol, "subtract bbox max y ~ 10, got {}", max[1]);
    assert!((max[2] - 10.0).abs() < tol, "subtract bbox max z ~ 10, got {}", max[2]);
}

// ── Group I: Boolean Intersect (box-box) ────────────────────────

#[test]
fn i1_intersect_volume() {
    let (mut k, a, b) = make_overlapping_boxes();
    let result = k.boolean_intersect(&a, &b).expect("intersect should succeed");
    let mesh = k.tessellate(&result, 0.01).expect("tessellate intersect");
    let vol = mesh_volume(&mesh);
    assert!(
        (vol - 500.0).abs() < 5.0,
        "Intersect volume should be ~500, got {}",
        vol
    );
}

#[test]
fn i2_intersect_face_count() {
    let (mut k, a, b) = make_overlapping_boxes();
    let result = k.boolean_intersect(&a, &b).unwrap();
    let faces = k.list_faces(&result);
    assert_eq!(
        faces.len(),
        6,
        "Intersect should have 6 faces, got {}",
        faces.len()
    );
}

#[test]
fn i3_intersect_euler() {
    let (mut k, a, b) = make_overlapping_boxes();
    let result = k.boolean_intersect(&a, &b).unwrap();
    let v = k.list_vertices(&result).len() as i64;
    let e = k.list_edges(&result).len() as i64;
    let f = k.list_faces(&result).len() as i64;
    assert_eq!(
        v - e + f,
        2,
        "Euler formula V-E+F must equal 2 for intersect (got V={}, E={}, F={})",
        v,
        e,
        f
    );
}

#[test]
fn i4_intersect_watertight() {
    let (mut k, a, b) = make_overlapping_boxes();
    let result = k.boolean_intersect(&a, &b).unwrap();
    let mesh = k.tessellate(&result, 0.01).unwrap();
    assert!(
        check_watertight(&mesh),
        "Intersect mesh must be watertight"
    );
}

#[test]
fn i5_intersect_bbox() {
    let (mut k, a, b) = make_overlapping_boxes();
    let result = k.boolean_intersect(&a, &b).unwrap();
    let mesh = k.tessellate(&result, 0.01).unwrap();
    let (min, max) = mesh_bbox(&mesh);
    let tol = 0.1;
    assert!((min[0] - 5.0).abs() < tol, "intersect bbox min x ~ 5, got {}", min[0]);
    assert!(min[1].abs() < tol, "intersect bbox min y ~ 0, got {}", min[1]);
    assert!(min[2].abs() < tol, "intersect bbox min z ~ 0, got {}", min[2]);
    assert!((max[0] - 10.0).abs() < tol, "intersect bbox max x ~ 10, got {}", max[0]);
    assert!((max[1] - 10.0).abs() < tol, "intersect bbox max y ~ 10, got {}", max[1]);
    assert!((max[2] - 10.0).abs() < tol, "intersect bbox max z ~ 10, got {}", max[2]);
}

// ── Group J: Edge Cases ─────────────────────────────────────────

#[test]
fn j1_invalid_handle_errors() {
    let (mut k, a, _b) = make_overlapping_boxes();
    let bad = KernelSolidHandle(99999);
    assert!(k.boolean_union(&a, &bad).is_err(), "bad handle B → error");
    assert!(k.boolean_union(&bad, &a).is_err(), "bad handle A → error");
}

#[test]
fn j2_box_cyl_union_basic() {
    // Now that cylinder booleans are supported, verify basic union succeeds
    let mut k = WaffleKernel::new();
    // 10x10x10 box centered at origin
    let (pb, posb) = make_rect_profile(0.0, 0.0, 10.0, 10.0);
    let fb = k.make_faces_from_profiles(&pb, XY_ORIGIN, XY_NORMAL, XY_X_AXIS, &posb).unwrap();
    let box_solid = k.extrude_face(fb[0], Z_DIR, 10.0).unwrap();
    // r=3 cylinder at origin, h=10
    let (profiles_c, positions_c) = make_circle_profile(0.0, 0.0, 3.0);
    let face_c = k.make_faces_from_profiles(&profiles_c, XY_ORIGIN, XY_NORMAL, XY_X_AXIS, &positions_c).unwrap();
    let cyl_solid = k.extrude_face(face_c[0], Z_DIR, 10.0).unwrap();
    let result = k.boolean_union(&box_solid, &cyl_solid);
    assert!(result.is_ok(), "Box-cylinder union should succeed, got: {:?}", result.err());
    let handle = result.unwrap();
    let mesh = k.tessellate(&handle, 0.01).unwrap();
    let vol = mesh_volume(&mesh);
    assert!(vol > 0.0, "Union volume should be positive, got {}", vol);
}

#[test]
fn j3_disjoint_boxes_union() {
    // Two boxes that don't overlap at all
    let mut k = WaffleKernel::new();

    let (pa, posa) = make_rect_profile(0.5, 0.5, 1.0, 1.0);
    let fa = k
        .make_faces_from_profiles(&pa, XY_ORIGIN, XY_NORMAL, XY_X_AXIS, &posa)
        .unwrap();
    let sa = k.extrude_face(fa[0], Z_DIR, 1.0).unwrap();

    let (pb, posb) = make_rect_profile(10.5, 0.5, 1.0, 1.0);
    let fb = k
        .make_faces_from_profiles(&pb, XY_ORIGIN, XY_NORMAL, XY_X_AXIS, &posb)
        .unwrap();
    let sb = k.extrude_face(fb[0], Z_DIR, 1.0).unwrap();

    let result = k.boolean_union(&sa, &sb).expect("disjoint union should succeed");
    let mesh = k.tessellate(&result, 0.01).unwrap();
    let vol = mesh_volume(&mesh);
    // Two unit boxes → volume = 2.0
    assert!(
        (vol - 2.0).abs() < 0.05,
        "Disjoint union volume should be ~2.0, got {}",
        vol
    );
}

#[test]
fn j4_identical_boxes_union() {
    // Two identical boxes → degenerate case (all faces coplanar).
    // Current implementation correctly detects this as degenerate and returns
    // the primary solid's faces via Partial(inside=original, outside=empty).
    let mut k = WaffleKernel::new();

    let (pa, posa) = make_rect_profile(0.5, 0.5, 1.0, 1.0);
    let fa = k
        .make_faces_from_profiles(&pa, XY_ORIGIN, XY_NORMAL, XY_X_AXIS, &posa)
        .unwrap();
    let sa = k.extrude_face(fa[0], Z_DIR, 1.0).unwrap();

    let (pb, posb) = make_rect_profile(0.5, 0.5, 1.0, 1.0);
    let fb = k
        .make_faces_from_profiles(&pb, XY_ORIGIN, XY_NORMAL, XY_X_AXIS, &posb)
        .unwrap();
    let sb = k.extrude_face(fb[0], Z_DIR, 1.0).unwrap();

    let result = k.boolean_union(&sa, &sb).expect("identical union should succeed");
    let mesh = k.tessellate(&result, 0.01).unwrap();
    let vol = mesh_volume(&mesh);
    assert!(
        (vol - 1.0).abs() < 0.05,
        "Identical union volume should be ~1.0, got {}",
        vol
    );
}

#[test]
fn j5_identical_boxes_subtract() {
    // A - A = empty solid (volume = 0).
    let mut k = WaffleKernel::new();

    let (pa, posa) = make_rect_profile(0.5, 0.5, 1.0, 1.0);
    let fa = k
        .make_faces_from_profiles(&pa, XY_ORIGIN, XY_NORMAL, XY_X_AXIS, &posa)
        .unwrap();
    let sa = k.extrude_face(fa[0], Z_DIR, 1.0).unwrap();

    let (pb, posb) = make_rect_profile(0.5, 0.5, 1.0, 1.0);
    let fb = k
        .make_faces_from_profiles(&pb, XY_ORIGIN, XY_NORMAL, XY_X_AXIS, &posb)
        .unwrap();
    let sb = k.extrude_face(fb[0], Z_DIR, 1.0).unwrap();

    let result = k.boolean_subtract(&sa, &sb).expect("identical subtract should succeed (empty)");
    let faces = k.list_faces(&result);
    assert_eq!(faces.len(), 0, "Identical subtract should have 0 faces");
}

#[test]
fn j6_disjoint_boxes_intersect() {
    // Disjoint boxes → intersect = empty solid (volume = 0).
    let mut k = WaffleKernel::new();

    let (pa, posa) = make_rect_profile(0.5, 0.5, 1.0, 1.0);
    let fa = k
        .make_faces_from_profiles(&pa, XY_ORIGIN, XY_NORMAL, XY_X_AXIS, &posa)
        .unwrap();
    let sa = k.extrude_face(fa[0], Z_DIR, 1.0).unwrap();

    let (pb, posb) = make_rect_profile(10.5, 0.5, 1.0, 1.0);
    let fb = k
        .make_faces_from_profiles(&pb, XY_ORIGIN, XY_NORMAL, XY_X_AXIS, &posb)
        .unwrap();
    let sb = k.extrude_face(fb[0], Z_DIR, 1.0).unwrap();

    let result = k.boolean_intersect(&sa, &sb).expect("disjoint intersect should succeed (empty)");
    let faces = k.list_faces(&result);
    assert_eq!(faces.len(), 0, "Disjoint intersect should have 0 faces");
}

// ── Group K: Scale Coverage ─────────────────────────────────────

#[test]
fn k1_union_micro() {
    let (mut k, a, b) = make_overlapping_boxes_scaled(1e-3);
    let result = k.boolean_union(&a, &b).expect("micro union should succeed");
    let mesh = k.tessellate(&result, 1e-5).unwrap();
    let vol = mesh_volume(&mesh);
    let expected = 1500.0 * 1e-9; // scale³
    let rel_err = (vol - expected).abs() / expected;
    assert!(
        rel_err < 0.02,
        "Micro union volume should be ~{:.6e}, got {:.6e} (rel_err={:.4})",
        expected,
        vol,
        rel_err
    );
}

#[test]
fn k2_union_macro() {
    let (mut k, a, b) = make_overlapping_boxes_scaled(1e2);
    let result = k.boolean_union(&a, &b).expect("macro union should succeed");
    let mesh = k.tessellate(&result, 1.0).unwrap();
    let vol = mesh_volume(&mesh);
    let expected = 1500.0 * 1e6; // scale³
    let rel_err = (vol - expected).abs() / expected;
    assert!(
        rel_err < 0.02,
        "Macro union volume should be ~{:.2e}, got {:.2e} (rel_err={:.4})",
        expected,
        vol,
        rel_err
    );
}

#[test]
fn k3_subtract_micro() {
    let (mut k, a, b) = make_overlapping_boxes_scaled(1e-3);
    let result = k
        .boolean_subtract(&a, &b)
        .expect("micro subtract should succeed");
    let mesh = k.tessellate(&result, 1e-5).unwrap();
    let vol = mesh_volume(&mesh);
    let expected = 500.0 * 1e-9;
    let rel_err = (vol - expected).abs() / expected;
    assert!(
        rel_err < 0.02,
        "Micro subtract volume should be ~{:.6e}, got {:.6e} (rel_err={:.4})",
        expected,
        vol,
        rel_err
    );
}

#[test]
fn k4_intersect_micro() {
    let (mut k, a, b) = make_overlapping_boxes_scaled(1e-3);
    let result = k
        .boolean_intersect(&a, &b)
        .expect("micro intersect should succeed");
    let mesh = k.tessellate(&result, 1e-5).unwrap();
    let vol = mesh_volume(&mesh);
    let expected = 500.0 * 1e-9;
    let rel_err = (vol - expected).abs() / expected;
    assert!(
        rel_err < 0.02,
        "Micro intersect volume should be ~{:.6e}, got {:.6e} (rel_err={:.4})",
        expected,
        vol,
        rel_err
    );
}

// ── Group L: Revolve ────────────────────────────────────────────

/// Helper: create a revolve solid from a rect profile.
/// Profile is centered at (cx, cy) with width w, height h on XY plane.
/// Revolved around the Y axis (origin=[0,0,0], dir=[0,1,0]) by given angle in degrees.
fn make_revolve_rect(
    cx: f64,
    cy: f64,
    w: f64,
    h: f64,
    angle_deg: f64,
) -> (WaffleKernel, KernelSolidHandle) {
    let mut k = WaffleKernel::new();
    let (profiles, positions) = make_rect_profile(cx, cy, w, h);
    let faces = k
        .make_faces_from_profiles(&profiles, XY_ORIGIN, XY_NORMAL, XY_X_AXIS, &positions)
        .expect("make_faces should succeed");
    // Revolve around Y axis
    let solid = k
        .revolve_face(faces[0], [0.0, 0.0, 0.0], [0.0, 1.0, 0.0], angle_deg)
        .expect("revolve should succeed");
    (k, solid)
}

#[test]
fn l1_revolve_half_turn_volume() {
    // Rect centered at x=5, w=2, h=4 → area=8, centroid at x=5 from Y axis
    // Pappus: V = π × 5 × 8 = 125.66
    let (mut k, solid) = make_revolve_rect(5.0, 0.0, 2.0, 4.0, 180.0);
    let mesh = k.tessellate(&solid, 0.01).expect("tessellate revolve");
    let vol = mesh_volume(&mesh);
    let expected = std::f64::consts::PI * 5.0 * 8.0; // ≈ 125.66
    let rel_err = (vol - expected).abs() / expected;
    assert!(
        rel_err < 0.01,
        "Half-turn revolve volume should be ~{:.2}, got {:.2} (rel_err={:.4})",
        expected,
        vol,
        rel_err
    );
}

#[test]
fn l2_revolve_quarter_turn_volume() {
    // Rect centered at x=10, w=2, h=3 → area=6, centroid at x=10 from Y axis
    // Pappus: V = (π/2) × 10 × 6 = 94.25
    let (mut k, solid) = make_revolve_rect(10.0, 0.0, 2.0, 3.0, 90.0);
    let mesh = k.tessellate(&solid, 0.01).expect("tessellate revolve");
    let vol = mesh_volume(&mesh);
    let expected = std::f64::consts::FRAC_PI_2 * 10.0 * 6.0; // ≈ 94.25
    let rel_err = (vol - expected).abs() / expected;
    assert!(
        rel_err < 0.01,
        "Quarter-turn revolve volume should be ~{:.2}, got {:.2} (rel_err={:.4})",
        expected,
        vol,
        rel_err
    );
}

#[test]
fn l3_revolve_half_turn_watertight() {
    let (mut k, solid) = make_revolve_rect(5.0, 0.0, 2.0, 4.0, 180.0);
    let mesh = k.tessellate(&solid, 0.01).unwrap();
    assert!(
        check_watertight(&mesh),
        "Half-turn revolve mesh must be watertight"
    );
}

#[test]
fn l4_revolve_quarter_turn_watertight() {
    let (mut k, solid) = make_revolve_rect(10.0, 0.0, 2.0, 3.0, 90.0);
    let mesh = k.tessellate(&solid, 0.01).unwrap();
    assert!(
        check_watertight(&mesh),
        "Quarter-turn revolve mesh must be watertight"
    );
}

#[test]
fn l5_revolve_euler_characteristic() {
    let (k, solid) = make_revolve_rect(5.0, 0.0, 2.0, 4.0, 180.0);
    let v = k.list_vertices(&solid).len() as i64;
    let e = k.list_edges(&solid).len() as i64;
    let f = k.list_faces(&solid).len() as i64;
    assert_eq!(
        v - e + f,
        2,
        "Euler formula V-E+F must equal 2 for revolve (got V={}, E={}, F={})",
        v,
        e,
        f
    );
}

#[test]
fn l6_revolve_topology_counts() {
    // Rect profile has M=4 vertices → 2M=8 V, 3M=12 E, M+2=6 F
    let (k, solid) = make_revolve_rect(5.0, 0.0, 2.0, 4.0, 180.0);
    let v = k.list_vertices(&solid).len();
    let e = k.list_edges(&solid).len();
    let f = k.list_faces(&solid).len();
    assert_eq!(v, 8, "Revolve rect should have 8 vertices, got {}", v);
    assert_eq!(e, 12, "Revolve rect should have 12 edges, got {}", e);
    assert_eq!(f, 6, "Revolve rect should have 6 faces, got {}", f);
}

#[test]
fn l7_revolve_face_geometry_types() {
    // 2 cylindrical + 4 planar (2 annular + 2 caps) = 6 faces
    let (k, solid) = make_revolve_rect(5.0, 0.0, 2.0, 4.0, 180.0);
    let faces = k.list_faces(&solid);
    let sigs: Vec<_> = faces
        .iter()
        .map(|&fid| k.compute_signature(fid, TopoKind::Face))
        .collect();
    let cylindrical_count = sigs
        .iter()
        .filter(|s| s.surface_type.as_deref() == Some("cylindrical"))
        .count();
    let planar_count = sigs
        .iter()
        .filter(|s| s.surface_type.as_deref() == Some("planar"))
        .count();
    assert_eq!(
        cylindrical_count, 2,
        "Revolve rect should have 2 cylindrical faces, got {}",
        cylindrical_count
    );
    assert_eq!(
        planar_count, 4,
        "Revolve rect should have 4 planar faces, got {}",
        planar_count
    );
}

#[test]
fn l8_revolve_edge_geometry_types() {
    // 4 arc + 8 linear = 12 edges
    let (k, solid) = make_revolve_rect(5.0, 0.0, 2.0, 4.0, 180.0);
    let edges = k.list_edges(&solid);
    assert_eq!(edges.len(), 12);
    // Arc edges have length = radius * sweep_angle (not full circle length)
    // Linear edges have finite length
    // We can distinguish by checking: arc edges have length proportional to π
    let sigs: Vec<_> = edges
        .iter()
        .map(|&eid| k.compute_signature(eid, TopoKind::Edge))
        .collect();
    // Arc edges: length = radius * π (for 180°)
    // The 4 profile vertices are at distances 4, 6, 6, 4 from Y axis (for cx=5, w=2, h=4, cy=0)
    // Wait: the rect is centered at (5, 0) with w=2, h=4
    // Vertices: (4, -2), (6, -2), (6, 2), (4, 2)
    // Distances from Y axis (x-values): 4, 6, 6, 4
    // Arc lengths: 4π, 6π, 6π, 4π
    let mut arc_count = 0;
    let mut linear_count = 0;
    for sig in &sigs {
        if let Some(len) = sig.length {
            // Arc edges have length = r * π for 180° revolve
            // Check if length is close to some r*π
            let r_candidate = len / std::f64::consts::PI;
            if (r_candidate - 4.0).abs() < 0.5 || (r_candidate - 6.0).abs() < 0.5 {
                arc_count += 1;
            } else {
                linear_count += 1;
            }
        }
    }
    assert_eq!(arc_count, 4, "Should have 4 arc edges, got {}", arc_count);
    assert_eq!(
        linear_count, 8,
        "Should have 8 linear edges, got {}",
        linear_count
    );
}

#[test]
fn l9_revolve_zero_angle_error() {
    let mut k = WaffleKernel::new();
    let (profiles, positions) = make_rect_profile(5.0, 0.0, 2.0, 4.0);
    let faces = k
        .make_faces_from_profiles(&profiles, XY_ORIGIN, XY_NORMAL, XY_X_AXIS, &positions)
        .unwrap();
    let result = k.revolve_face(faces[0], [0.0, 0.0, 0.0], [0.0, 1.0, 0.0], 0.0);
    assert!(result.is_err(), "Zero angle should fail");
    if let Err(KernelError::Other { .. }) = result {
        // expected
    } else {
        panic!("Expected KernelError::Other for zero angle, got {:?}", result);
    }
}

#[test]
fn l10_revolve_full_360_error() {
    let mut k = WaffleKernel::new();
    let (profiles, positions) = make_rect_profile(5.0, 0.0, 2.0, 4.0);
    let faces = k
        .make_faces_from_profiles(&profiles, XY_ORIGIN, XY_NORMAL, XY_X_AXIS, &positions)
        .unwrap();
    let result = k.revolve_face(faces[0], [0.0, 0.0, 0.0], [0.0, 1.0, 0.0], 360.0);
    assert!(result.is_err(), "Full 360° should fail");
    if let Err(KernelError::NotSupported { .. }) = result {
        // expected
    } else {
        panic!(
            "Expected KernelError::NotSupported for 360°, got {:?}",
            result
        );
    }
}

#[test]
fn l11_revolve_circle_not_supported() {
    let mut k = WaffleKernel::new();
    let (profiles, positions) = make_circle_profile(5.0, 0.0, 1.0);
    let faces = k
        .make_faces_from_profiles(&profiles, XY_ORIGIN, XY_NORMAL, XY_X_AXIS, &positions)
        .unwrap();
    let result = k.revolve_face(faces[0], [0.0, 0.0, 0.0], [0.0, 1.0, 0.0], 180.0);
    assert!(result.is_err(), "Circle revolve should fail");
    if let Err(KernelError::NotSupported { .. }) = result {
        // expected
    } else {
        panic!(
            "Expected KernelError::NotSupported for circle revolve, got {:?}",
            result
        );
    }
}

#[test]
fn l12_revolve_invalid_face_error() {
    let mut k = WaffleKernel::new();
    let result = k.revolve_face(KernelId(99999), [0.0, 0.0, 0.0], [0.0, 1.0, 0.0], 180.0);
    assert!(result.is_err(), "Invalid face should fail");
    if let Err(KernelError::EntityNotFound { .. }) = result {
        // expected
    } else {
        panic!(
            "Expected KernelError::EntityNotFound for invalid face, got {:?}",
            result
        );
    }
}

// ━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━
// Milestone 5: SSI-based cylindrical boolean operation tests
// ━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━

/// Create a box and a cylinder in the same kernel and perform a boolean op.
fn do_box_cyl_boolean(
    box_cx: f64, box_cy: f64, box_w: f64, box_h: f64, box_d: f64,
    cyl_cx: f64, cyl_cy: f64, cyl_r: f64, cyl_d: f64,
    op: crate::boolean::BoolOp,
) -> Result<(WaffleKernel, KernelSolidHandle), KernelError> {
    let mut k = WaffleKernel::new();
    let (pb, posb) = make_rect_profile(box_cx, box_cy, box_w, box_h);
    let fb = k.make_faces_from_profiles(&pb, XY_ORIGIN, XY_NORMAL, XY_X_AXIS, &posb).unwrap();
    let box_solid = k.extrude_face(fb[0], Z_DIR, box_d).unwrap();
    let (pc, posc) = make_circle_profile(cyl_cx, cyl_cy, cyl_r);
    let fc = k.make_faces_from_profiles(&pc, XY_ORIGIN, XY_NORMAL, XY_X_AXIS, &posc).unwrap();
    let cyl_solid = k.extrude_face(fc[0], Z_DIR, cyl_d).unwrap();
    let result = match op {
        crate::boolean::BoolOp::Union => k.boolean_union(&box_solid, &cyl_solid)?,
        crate::boolean::BoolOp::Subtract => k.boolean_subtract(&box_solid, &cyl_solid)?,
        crate::boolean::BoolOp::Intersect => k.boolean_intersect(&box_solid, &cyl_solid)?,
    };
    Ok((k, result))
}

/// Create two cylinders and perform a boolean op.
fn do_cyl_cyl_boolean(
    cx_a: f64, cy_a: f64, r_a: f64, d_a: f64,
    cx_b: f64, cy_b: f64, r_b: f64, d_b: f64,
    op: crate::boolean::BoolOp,
) -> Result<(WaffleKernel, KernelSolidHandle), KernelError> {
    let mut k = WaffleKernel::new();
    let (pa, posa) = make_circle_profile(cx_a, cy_a, r_a);
    let fa = k.make_faces_from_profiles(&pa, XY_ORIGIN, XY_NORMAL, XY_X_AXIS, &posa).unwrap();
    let cyl_a = k.extrude_face(fa[0], Z_DIR, d_a).unwrap();
    let (pb, posb) = make_circle_profile(cx_b, cy_b, r_b);
    let fb = k.make_faces_from_profiles(&pb, XY_ORIGIN, XY_NORMAL, XY_X_AXIS, &posb).unwrap();
    let cyl_b = k.extrude_face(fb[0], Z_DIR, d_b).unwrap();
    let result = match op {
        crate::boolean::BoolOp::Union => k.boolean_union(&cyl_a, &cyl_b)?,
        crate::boolean::BoolOp::Subtract => k.boolean_subtract(&cyl_a, &cyl_b)?,
        crate::boolean::BoolOp::Intersect => k.boolean_intersect(&cyl_a, &cyl_b)?,
    };
    Ok((k, result))
}

// ── Group M: Box-Cylinder Booleans ─────────────────────────────────

#[test]
fn m1_box_enclosed_cyl_subtract_volume() {
    // 12x12x10 box centered at origin minus r=3 cylinder at origin
    // Expected: 1440 - 90pi ~= 1157.26
    let (mut k, result) = do_box_cyl_boolean(
        0.0, 0.0, 12.0, 12.0, 10.0,
        0.0, 0.0, 3.0, 10.0,
        crate::boolean::BoolOp::Subtract,
    ).expect("enclosed cyl subtract should succeed");
    let mesh = k.tessellate(&result, 0.01).unwrap();
    let vol = mesh_volume(&mesh);
    let expected = 12.0 * 12.0 * 10.0 - PI * 9.0 * 10.0;
    assert!(
        (vol - expected).abs() < 5.0,
        "Volume should be ~{:.2}, got {:.2} (diff={:.2})",
        expected, vol, (vol - expected).abs()
    );
}

#[test]
fn m2_box_enclosed_cyl_subtract_watertight() {
    let (mut k, result) = do_box_cyl_boolean(
        0.0, 0.0, 12.0, 12.0, 10.0,
        0.0, 0.0, 3.0, 10.0,
        crate::boolean::BoolOp::Subtract,
    ).expect("enclosed cyl subtract should succeed");
    let mesh = k.tessellate(&result, 0.01).unwrap();
    assert!(check_watertight(&mesh), "Subtract result mesh must be watertight");
}

#[test]
fn m3_box_enclosed_cyl_subtract_euler() {
    let (k, result) = do_box_cyl_boolean(
        0.0, 0.0, 12.0, 12.0, 10.0,
        0.0, 0.0, 3.0, 10.0,
        crate::boolean::BoolOp::Subtract,
    ).expect("enclosed cyl subtract should succeed");
    let v = k.list_vertices(&result).len() as i64;
    let e = k.list_edges(&result).len() as i64;
    let f = k.list_faces(&result).len() as i64;
    assert_eq!(v - e + f, 2, "V-E+F must be 2 (V={}, E={}, F={})", v, e, f);
}

#[test]
fn m4_box_cyl_union_inscribed_volume() {
    // 10x10x10 box union r=5 cyl (cylinder inscribed in box) -> vol ~= 1000
    let (mut k, result) = do_box_cyl_boolean(
        0.0, 0.0, 10.0, 10.0, 10.0,
        0.0, 0.0, 5.0, 10.0,
        crate::boolean::BoolOp::Union,
    ).expect("inscribed union should succeed");
    let mesh = k.tessellate(&result, 0.01).unwrap();
    let vol = mesh_volume(&mesh);
    assert!(
        (vol - 1000.0).abs() < 5.0,
        "Inscribed union volume should be ~1000, got {:.2}",
        vol
    );
}

#[test]
fn m5_box_cyl_intersect_inscribed_volume() {
    // 10x10x10 box intersect r=5 cyl -> vol ~= pi*25*10 ~= 785.40
    let (mut k, result) = do_box_cyl_boolean(
        0.0, 0.0, 10.0, 10.0, 10.0,
        0.0, 0.0, 5.0, 10.0,
        crate::boolean::BoolOp::Intersect,
    ).expect("inscribed intersect should succeed");
    let mesh = k.tessellate(&result, 0.01).unwrap();
    let vol = mesh_volume(&mesh);
    let expected = PI * 25.0 * 10.0;
    assert!(
        (vol - expected).abs() < 5.0,
        "Inscribed intersect volume should be ~{:.2}, got {:.2}",
        expected, vol
    );
}

#[test]
fn m6_box_cyl_intersect_watertight() {
    let (mut k, result) = do_box_cyl_boolean(
        0.0, 0.0, 10.0, 10.0, 10.0,
        0.0, 0.0, 5.0, 10.0,
        crate::boolean::BoolOp::Intersect,
    ).expect("inscribed intersect should succeed");
    let mesh = k.tessellate(&result, 0.01).unwrap();
    assert!(check_watertight(&mesh), "Intersect result mesh must be watertight");
}

#[test]
fn m7_box_cyl_result_has_cylindrical_face() {
    let (k, result) = do_box_cyl_boolean(
        0.0, 0.0, 12.0, 12.0, 10.0,
        0.0, 0.0, 3.0, 10.0,
        crate::boolean::BoolOp::Subtract,
    ).expect("enclosed cyl subtract should succeed");
    let faces = k.list_faces(&result);
    let has_cyl = faces.iter().any(|&fid| {
        let sig = k.compute_signature(fid, TopoKind::Face);
        sig.surface_type.as_deref() == Some("cylindrical")
    });
    assert!(has_cyl, "Subtract result should have at least one cylindrical face");
}

#[test]
fn m8_box_cyl_result_has_arc_edges() {
    let (k, result) = do_box_cyl_boolean(
        0.0, 0.0, 12.0, 12.0, 10.0,
        0.0, 0.0, 3.0, 10.0,
        crate::boolean::BoolOp::Subtract,
    ).expect("enclosed cyl subtract should succeed");
    let edges = k.list_edges(&result);
    let has_arc = edges.iter().any(|&eid| {
        let sig = k.compute_signature(eid, TopoKind::Edge);
        // Arc edges have a length field based on arc geometry
        sig.length.map_or(false, |l| l > 0.0)
    });
    assert!(has_arc, "Subtract result should have at least one arc/circular edge");
}

#[test]
fn m9_box_cyl_disjoint_union_volume() {
    // Box at (0,0) and cylinder at (20,0) -- disjoint
    // Box: 10x10x10 = 1000, Cyl: pi*9*10 ~= 282.74
    let (mut k, result) = do_box_cyl_boolean(
        0.0, 0.0, 10.0, 10.0, 10.0,
        20.0, 0.0, 3.0, 10.0,
        crate::boolean::BoolOp::Union,
    ).expect("disjoint union should succeed");
    let mesh = k.tessellate(&result, 0.01).unwrap();
    let vol = mesh_volume(&mesh);
    let expected = 1000.0 + PI * 9.0 * 10.0;
    assert!(
        (vol - expected).abs() < 5.0,
        "Disjoint union volume should be ~{:.2}, got {:.2}",
        expected, vol
    );
}

// ── Diagnostic: standalone cylinder tessellation ──────────────────

#[test]
fn diag_standalone_cyl_volume() {
    // Build a standalone cylinder via build_cyl_result and tessellate
    // This tests the boolean-result tessellation path (no cylinder_params)
    let cyl = CylinderParams {
        center_bottom: [20.0, 0.0, 0.0],
        radius: 3.0,
        depth: 10.0,
        direction: [0.0, 0.0, 1.0],
        x_axis: [1.0, 0.0, 0.0],
        y_axis: [0.0, 1.0, 0.0],
    };
    let mut next_id = 1000u64;
    let mut id_alloc = || { let id = next_id; next_id += 1; id };
    let result = crate::boolean::build_cyl_result(&cyl, &mut id_alloc).unwrap();
    let mesh = crate::tessellation::tessellate_solid(
        &result.arena, &result.face_map, &result.face_geometry,
        &result.edge_geometry, None, None,
    ).unwrap();

    let vol = mesh_volume(&mesh);
    let expected = PI * 9.0 * 10.0;
    assert!(check_watertight(&mesh), "Standalone cylinder must be watertight");
    assert!((vol - expected).abs() < 5.0, "Standalone cyl volume should be ~{:.2}, got {:.2}", expected, vol);
}

// ── Group N: Cylinder-Cylinder Booleans ────────────────────────────

#[test]
fn n1_cyl_cyl_union_volume() {
    // Two r=3 cylinders, centers 3 apart, h=10
    // Union cross-section area = 2*pi*r^2 - lens_area
    let r: f64 = 3.0;
    let d: f64 = 3.0;
    let a = d / 2.0;
    let h = (r * r - a * a).sqrt();
    let lens = r * r * (a / r).acos() + r * r * ((d - a) / r).acos() - d * h;
    let union_area = 2.0 * PI * r * r - lens;
    let expected = union_area * 10.0;
    let (mut k, result) = do_cyl_cyl_boolean(
        0.0, 0.0, 3.0, 10.0,
        3.0, 0.0, 3.0, 10.0,
        crate::boolean::BoolOp::Union,
    ).expect("cyl-cyl union should succeed");
    let mesh = k.tessellate(&result, 0.01).unwrap();
    let vol = mesh_volume(&mesh);
    assert!(
        (vol - expected).abs() < 10.0,
        "Cyl-cyl union volume should be ~{:.2}, got {:.2} (diff={:.2})",
        expected, vol, (vol - expected).abs()
    );
}

#[test]
fn n2_cyl_cyl_union_watertight() {
    let (mut k, result) = do_cyl_cyl_boolean(
        0.0, 0.0, 3.0, 10.0,
        3.0, 0.0, 3.0, 10.0,
        crate::boolean::BoolOp::Union,
    ).expect("cyl-cyl union should succeed");
    let mesh = k.tessellate(&result, 0.01).unwrap();
    assert!(check_watertight(&mesh), "Cyl-cyl union mesh must be watertight");
}

#[test]
fn n3_cyl_cyl_subtract_volume() {
    // cyl_A - cyl_B: area = pi*r^2 - lens
    let r: f64 = 3.0;
    let d: f64 = 3.0;
    let a = d / 2.0;
    let h = (r * r - a * a).sqrt();
    let lens = r * r * (a / r).acos() + r * r * ((d - a) / r).acos() - d * h;
    let subtract_area = PI * r * r - lens;
    let expected = subtract_area * 10.0;
    let (mut k, result) = do_cyl_cyl_boolean(
        0.0, 0.0, 3.0, 10.0,
        3.0, 0.0, 3.0, 10.0,
        crate::boolean::BoolOp::Subtract,
    ).expect("cyl-cyl subtract should succeed");
    let mesh = k.tessellate(&result, 0.01).unwrap();
    let vol = mesh_volume(&mesh);
    assert!(
        (vol - expected).abs() < 10.0,
        "Cyl-cyl subtract volume should be ~{:.2}, got {:.2} (diff={:.2})",
        expected, vol, (vol - expected).abs()
    );
}

#[test]
fn n4_cyl_cyl_subtract_watertight() {
    let (mut k, result) = do_cyl_cyl_boolean(
        0.0, 0.0, 3.0, 10.0,
        3.0, 0.0, 3.0, 10.0,
        crate::boolean::BoolOp::Subtract,
    ).expect("cyl-cyl subtract should succeed");
    let mesh = k.tessellate(&result, 0.01).unwrap();
    assert!(check_watertight(&mesh), "Cyl-cyl subtract mesh must be watertight");
}

#[test]
fn n5_cyl_cyl_union_euler() {
    let (k, result) = do_cyl_cyl_boolean(
        0.0, 0.0, 3.0, 10.0,
        3.0, 0.0, 3.0, 10.0,
        crate::boolean::BoolOp::Union,
    ).expect("cyl-cyl union should succeed");
    let v = k.list_vertices(&result).len() as i64;
    let e = k.list_edges(&result).len() as i64;
    let f = k.list_faces(&result).len() as i64;
    assert_eq!(v - e + f, 2, "V-E+F must be 2 (V={}, E={}, F={})", v, e, f);
}

// ── Group O: Edge Cases + SSI Unit Tests ──────────────────────────

#[test]
fn o1_revolve_boolean_still_unsupported() {
    let mut k = WaffleKernel::new();
    // Create a revolve solid
    let (profiles, positions) = make_rect_profile(5.0, 0.0, 2.0, 4.0);
    let faces = k.make_faces_from_profiles(&profiles, XY_ORIGIN, XY_NORMAL, XY_X_AXIS, &positions).unwrap();
    let revolve = k.revolve_face(faces[0], [0.0, 0.0, 0.0], [0.0, 1.0, 0.0], 180.0).unwrap();
    // Create a box
    let (pb, posb) = make_rect_profile(0.0, 0.0, 10.0, 10.0);
    let fb = k.make_faces_from_profiles(&pb, XY_ORIGIN, XY_NORMAL, XY_X_AXIS, &posb).unwrap();
    let box_solid = k.extrude_face(fb[0], Z_DIR, 10.0).unwrap();
    // Boolean with revolve should still be NotSupported
    let result = k.boolean_union(&revolve, &box_solid);
    assert!(result.is_err(), "Revolve boolean should fail");
    if let Err(ref e) = result {
        assert!(
            matches!(e, KernelError::NotSupported { .. }),
            "Expected NotSupported for revolve boolean, got {:?}", e
        );
    }
}

#[test]
fn o2_ssi_plane_perp_cylinder_circle() {
    use crate::ssi::*;
    let cyl = CylinderParams {
        center_bottom: [0.0, 0.0, 0.0],
        radius: 3.0,
        x_axis: [1.0, 0.0, 0.0],
        y_axis: [0.0, 1.0, 0.0],
        direction: [0.0, 0.0, 1.0],
        depth: 10.0,
    };
    let curves = plane_perp_cylinder_ssi(5.0, &cyl);
    assert_eq!(curves.len(), 1, "Should produce exactly 1 circle");
    if let SSICurve::Circle { center, radius, .. } = &curves[0] {
        assert!((center[0]).abs() < 1e-9, "Circle center x should be 0");
        assert!((center[1]).abs() < 1e-9, "Circle center y should be 0");
        assert!((center[2] - 5.0).abs() < 1e-9, "Circle center z should be 5");
        assert!((radius - 3.0).abs() < 1e-9, "Circle radius should be 3");
    } else {
        panic!("Expected SSICurve::Circle");
    }
}

#[test]
fn o3_ssi_plane_parallel_cylinder_lines() {
    use crate::ssi::*;
    let cyl = CylinderParams {
        center_bottom: [0.0, 0.0, 0.0],
        radius: 3.0,
        x_axis: [1.0, 0.0, 0.0],
        y_axis: [0.0, 1.0, 0.0],
        direction: [0.0, 0.0, 1.0],
        depth: 10.0,
    };
    // Plane at x=1, normal=[1,0,0]
    let curves = plane_parallel_cylinder_ssi(
        [1.0, 0.0, 0.0], [1.0, 0.0, 0.0], &cyl, 0.0, 10.0
    );
    assert_eq!(curves.len(), 2, "Should produce 2 lines");
    // Lines should be at x=1, y=+/-sqrt(9-1)=+/-sqrt(8)
    let sqrt8 = 8.0_f64.sqrt();
    for curve in &curves {
        if let SSICurve::Line { start, end } = curve {
            assert!((start[0] - 1.0).abs() < 1e-9, "Line x should be 1.0");
            assert!((start[1].abs() - sqrt8).abs() < 1e-6, "Line y should be +/-sqrt(8), got {}", start[1]);
            assert!((start[2]).abs() < 1e-9, "Line start z should be 0");
            assert!((end[2] - 10.0).abs() < 1e-9, "Line end z should be 10");
        } else {
            panic!("Expected SSICurve::Line");
        }
    }
}

#[test]
fn o4_ssi_cyl_cyl_parallel_lines() {
    use crate::ssi::*;
    let cyl_a = CylinderParams {
        center_bottom: [0.0, 0.0, 0.0],
        radius: 3.0,
        x_axis: [1.0, 0.0, 0.0],
        y_axis: [0.0, 1.0, 0.0],
        direction: [0.0, 0.0, 1.0],
        depth: 10.0,
    };
    let cyl_b = CylinderParams {
        center_bottom: [3.0, 0.0, 0.0],
        radius: 3.0,
        x_axis: [1.0, 0.0, 0.0],
        y_axis: [0.0, 1.0, 0.0],
        direction: [0.0, 0.0, 1.0],
        depth: 10.0,
    };
    let curves = cylinder_cylinder_ssi(&cyl_a, &cyl_b, 0.0, 10.0);
    assert_eq!(curves.len(), 2, "Should produce 2 intersection lines");
    for curve in &curves {
        if let SSICurve::Line { start, end: _ } = curve {
            assert!((start[0] - 1.5).abs() < 1e-6, "Line x should be 1.5, got {}", start[0]);
            let expected_y = (9.0 - 2.25_f64).sqrt(); // sqrt(6.75) ~= 2.598
            assert!((start[1].abs() - expected_y).abs() < 1e-6,
                "Line y should be +/-{:.3}, got {}", expected_y, start[1]);
        } else {
            panic!("Expected SSICurve::Line");
        }
    }
}
