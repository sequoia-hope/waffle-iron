//! Algebraic property tests for boolean operations.
//!
//! Verifies that the TruckKernel boolean pipeline satisfies fundamental
//! algebraic and topological invariants: idempotence, commutativity,
//! volume conservation, Euler's formula, and watertightness.

use kernel_fork::types::{KernelSolidHandle, RenderMesh};
use kernel_fork::{Kernel, KernelIntrospect, TruckIntrospect, TruckKernel};

/// Compute the volume of a closed triangle mesh using the signed tetrahedra method.
fn compute_volume(mesh: &RenderMesh) -> f64 {
    let mut vol = 0.0_f64;
    for tri in mesh.indices.chunks(3) {
        if tri.len() < 3 {
            continue;
        }
        let i0 = tri[0] as usize * 3;
        let i1 = tri[1] as usize * 3;
        let i2 = tri[2] as usize * 3;
        if i0 + 2 >= mesh.vertices.len()
            || i1 + 2 >= mesh.vertices.len()
            || i2 + 2 >= mesh.vertices.len()
        {
            continue;
        }
        let v0 = [
            mesh.vertices[i0] as f64,
            mesh.vertices[i0 + 1] as f64,
            mesh.vertices[i0 + 2] as f64,
        ];
        let v1 = [
            mesh.vertices[i1] as f64,
            mesh.vertices[i1 + 1] as f64,
            mesh.vertices[i1 + 2] as f64,
        ];
        let v2 = [
            mesh.vertices[i2] as f64,
            mesh.vertices[i2 + 1] as f64,
            mesh.vertices[i2 + 2] as f64,
        ];
        let cx = v1[1] * v2[2] - v1[2] * v2[1];
        let cy = v1[2] * v2[0] - v1[0] * v2[2];
        let cz = v1[0] * v2[1] - v1[1] * v2[0];
        vol += v0[0] * cx + v0[1] * cy + v0[2] * cz;
    }
    (vol / 6.0).abs()
}

/// Assert two volumes are approximately equal within a relative tolerance.
fn assert_volume_approx(actual: f64, expected: f64, tolerance: f64, label: &str) {
    if expected.abs() < 1e-12 {
        assert!(
            actual.abs() < tolerance,
            "{label}: expected ~0, got {actual}"
        );
        return;
    }
    let rel_error = (actual - expected).abs() / expected;
    assert!(
        rel_error < tolerance,
        "{label}: expected {expected:.4}, got {actual:.4} (rel error: {rel_error:.4})"
    );
}

/// Create a box solid using truck builder sweeps and store it in the kernel.
/// Box extends from (ox, oy, oz) to (ox+w, oy+h, oz+d).
fn make_offset_box(
    kernel: &mut TruckKernel,
    ox: f64,
    oy: f64,
    oz: f64,
    w: f64,
    h: f64,
    d: f64,
) -> KernelSolidHandle {
    use truck_modeling::builder;
    use truck_modeling::{Point3, Vector3};

    let v = builder::vertex(Point3::new(ox, oy, oz));
    let e = builder::tsweep(&v, Vector3::new(w, 0.0, 0.0));
    let f = builder::tsweep(&e, Vector3::new(0.0, h, 0.0));
    let solid = builder::tsweep(&f, Vector3::new(0.0, 0.0, d));
    kernel.store_solid(solid)
}

/// Tessellate and compute volume.
fn volume_of(kernel: &mut TruckKernel, handle: &KernelSolidHandle) -> f64 {
    let mesh = kernel
        .tessellate(handle, 0.05)
        .expect("tessellate should succeed");
    compute_volume(&mesh)
}

// ── Idempotence ─────────────────────────────────────────────────

#[test]
fn test_idempotence_union_self() {
    let mut kernel = TruckKernel::new();

    // Two identical boxes at the same position.
    // union(A, A) should produce the same volume as A.
    let h_a = make_offset_box(&mut kernel, 0.0, 0.0, 0.0, 2.0, 3.0, 4.0);
    let h_a2 = make_offset_box(&mut kernel, 0.0, 0.0, 0.0, 2.0, 3.0, 4.0);

    let vol_a = volume_of(&mut kernel, &h_a);
    let expected = 2.0 * 3.0 * 4.0;
    assert_volume_approx(vol_a, expected, 0.05, "Box A volume");

    let h_union = kernel
        .boolean_union(&h_a, &h_a2)
        .expect("union(A,A) should succeed");
    let vol_union = volume_of(&mut kernel, &h_union);
    assert_volume_approx(vol_union, expected, 0.05, "union(A,A) volume");
}

// ── Commutativity ───────────────────────────────────────────────

#[test]
fn test_commutativity_union() {
    let mut kernel = TruckKernel::new();

    // Box A: [0,2]^3
    let h_a = make_offset_box(&mut kernel, 0.0, 0.0, 0.0, 2.0, 2.0, 2.0);
    // Box B: [0.5,1.5]^3 — fully inside A, offset
    let h_b = make_offset_box(&mut kernel, 0.5, 0.5, 0.5, 1.0, 1.0, 1.0);

    // union(A, B)
    let h_ab = kernel
        .boolean_union(&h_a, &h_b)
        .expect("union(A,B) should succeed");
    let vol_ab = volume_of(&mut kernel, &h_ab);

    // Fresh copies for union(B, A)
    let h_a2 = make_offset_box(&mut kernel, 0.0, 0.0, 0.0, 2.0, 2.0, 2.0);
    let h_b2 = make_offset_box(&mut kernel, 0.5, 0.5, 0.5, 1.0, 1.0, 1.0);

    let h_ba = kernel
        .boolean_union(&h_b2, &h_a2)
        .expect("union(B,A) should succeed");
    let vol_ba = volume_of(&mut kernel, &h_ba);

    assert_volume_approx(vol_ab, vol_ba, 0.05, "union commutativity");
}

#[test]
fn test_commutativity_intersect() {
    let mut kernel = TruckKernel::new();

    // Box A: [0,2]^3
    let h_a = make_offset_box(&mut kernel, 0.0, 0.0, 0.0, 2.0, 2.0, 2.0);
    // Box B: [0.5,1.5]^3 — fully inside A
    let h_b = make_offset_box(&mut kernel, 0.5, 0.5, 0.5, 1.0, 1.0, 1.0);

    // intersect(A, B)
    let h_ab = kernel
        .boolean_intersect(&h_a, &h_b)
        .expect("intersect(A,B) should succeed");
    let vol_ab = volume_of(&mut kernel, &h_ab);

    // Fresh copies for intersect(B, A)
    let h_a2 = make_offset_box(&mut kernel, 0.0, 0.0, 0.0, 2.0, 2.0, 2.0);
    let h_b2 = make_offset_box(&mut kernel, 0.5, 0.5, 0.5, 1.0, 1.0, 1.0);

    let h_ba = kernel
        .boolean_intersect(&h_b2, &h_a2)
        .expect("intersect(B,A) should succeed");
    let vol_ba = volume_of(&mut kernel, &h_ba);

    assert_volume_approx(vol_ab, vol_ba, 0.05, "intersect commutativity");
}

// ── Volume Conservation ─────────────────────────────────────────

#[test]
fn test_volume_conservation_union_intersect() {
    // For overlapping boxes A, B:
    // vol(union(A,B)) + vol(intersect(A,B)) ≈ vol(A) + vol(B)
    let mut kernel = TruckKernel::new();

    // Box A: [0,2]^3, vol = 8
    let h_a = make_offset_box(&mut kernel, 0.0, 0.0, 0.0, 2.0, 2.0, 2.0);
    let vol_a = volume_of(&mut kernel, &h_a);

    // Box B: [0.5,1.5]^3, vol = 1 — fully inside A
    let h_b = make_offset_box(&mut kernel, 0.5, 0.5, 0.5, 1.0, 1.0, 1.0);
    let vol_b = volume_of(&mut kernel, &h_b);

    // Union
    let h_a2 = make_offset_box(&mut kernel, 0.0, 0.0, 0.0, 2.0, 2.0, 2.0);
    let h_b2 = make_offset_box(&mut kernel, 0.5, 0.5, 0.5, 1.0, 1.0, 1.0);
    let h_union = kernel
        .boolean_union(&h_a2, &h_b2)
        .expect("union should succeed");
    let vol_union = volume_of(&mut kernel, &h_union);

    // Intersect
    let h_a3 = make_offset_box(&mut kernel, 0.0, 0.0, 0.0, 2.0, 2.0, 2.0);
    let h_b3 = make_offset_box(&mut kernel, 0.5, 0.5, 0.5, 1.0, 1.0, 1.0);
    let h_intersect = kernel
        .boolean_intersect(&h_a3, &h_b3)
        .expect("intersect should succeed");
    let vol_intersect = volume_of(&mut kernel, &h_intersect);

    let lhs = vol_union + vol_intersect;
    let rhs = vol_a + vol_b;

    assert_volume_approx(
        lhs,
        rhs,
        0.05,
        "volume conservation: vol(U)+vol(I) ≈ vol(A)+vol(B)",
    );
}

#[test]
fn test_volume_conservation_partial_overlap() {
    // Partially overlapping boxes (not fully contained).
    let mut kernel = TruckKernel::new();

    // Box A: [0,2]^3, vol = 8
    let h_a = make_offset_box(&mut kernel, 0.0, 0.0, 0.0, 2.0, 2.0, 2.0);
    let vol_a = volume_of(&mut kernel, &h_a);

    // Box B: [1,3]^3, vol = 8 — partial overlap with A
    let h_b = make_offset_box(&mut kernel, 1.0, 1.0, 1.0, 2.0, 2.0, 2.0);
    let vol_b = volume_of(&mut kernel, &h_b);

    // Union
    let h_a2 = make_offset_box(&mut kernel, 0.0, 0.0, 0.0, 2.0, 2.0, 2.0);
    let h_b2 = make_offset_box(&mut kernel, 1.0, 1.0, 1.0, 2.0, 2.0, 2.0);
    let h_union = kernel
        .boolean_union(&h_a2, &h_b2)
        .expect("union should succeed");
    let vol_union = volume_of(&mut kernel, &h_union);

    // Intersect
    let h_a3 = make_offset_box(&mut kernel, 0.0, 0.0, 0.0, 2.0, 2.0, 2.0);
    let h_b3 = make_offset_box(&mut kernel, 1.0, 1.0, 1.0, 2.0, 2.0, 2.0);
    let h_intersect = kernel
        .boolean_intersect(&h_a3, &h_b3)
        .expect("intersect should succeed");
    let vol_intersect = volume_of(&mut kernel, &h_intersect);

    let lhs = vol_union + vol_intersect;
    let rhs = vol_a + vol_b;

    assert_volume_approx(lhs, rhs, 0.05, "volume conservation (partial overlap)");
}

// ── Euler's Formula ─────────────────────────────────────────────

/// Count unique vertices, edges, and faces via TruckIntrospect.
fn euler_counts(kernel: &TruckKernel, handle: &KernelSolidHandle) -> (i64, i64, i64) {
    let introspect = TruckIntrospect::new(kernel);
    let faces = introspect.list_faces(handle);
    let edges = introspect.list_edges(handle);
    let vertices = introspect.list_vertices(handle);

    (
        vertices.len() as i64,
        edges.len() as i64,
        faces.len() as i64,
    )
}

#[test]
fn test_euler_formula_box() {
    let mut kernel = TruckKernel::new();
    let h = make_offset_box(&mut kernel, 0.0, 0.0, 0.0, 1.0, 1.0, 1.0);
    let (v, e, f) = euler_counts(&kernel, &h);

    assert_eq!(v - e + f, 2, "Euler V-E+F=2 for box: V={v}, E={e}, F={f}");
}

#[test]
fn test_euler_formula_union_result() {
    let mut kernel = TruckKernel::new();

    let h_a = make_offset_box(&mut kernel, 0.0, 0.0, 0.0, 2.0, 2.0, 2.0);
    let h_b = make_offset_box(&mut kernel, 0.5, 0.5, 0.5, 1.0, 1.0, 1.0);

    let h_union = kernel
        .boolean_union(&h_a, &h_b)
        .expect("union should succeed");

    let (v, e, f) = euler_counts(&kernel, &h_union);
    assert_eq!(
        v - e + f,
        2,
        "Euler V-E+F=2 for union result: V={v}, E={e}, F={f}"
    );
}

#[test]
fn test_euler_formula_intersect_result() {
    let mut kernel = TruckKernel::new();

    let h_a = make_offset_box(&mut kernel, 0.0, 0.0, 0.0, 2.0, 2.0, 2.0);
    let h_b = make_offset_box(&mut kernel, 0.5, 0.5, 0.5, 1.0, 1.0, 1.0);

    let h_int = kernel
        .boolean_intersect(&h_a, &h_b)
        .expect("intersect should succeed");

    let (v, e, f) = euler_counts(&kernel, &h_int);
    assert_eq!(
        v - e + f,
        2,
        "Euler V-E+F=2 for intersect result: V={v}, E={e}, F={f}"
    );
}

#[test]
fn test_euler_formula_subtract_result() {
    let mut kernel = TruckKernel::new();

    let h_a = make_offset_box(&mut kernel, 0.0, 0.0, 0.0, 2.0, 2.0, 2.0);
    let h_b = make_offset_box(&mut kernel, 0.5, 0.5, 0.5, 1.0, 1.0, 1.0);

    let h_sub = kernel
        .boolean_subtract(&h_a, &h_b)
        .expect("subtract should succeed");

    let (v, e, f) = euler_counts(&kernel, &h_sub);
    // Subtract of fully-contained box creates cavity: 2 shells → V-E+F = 2*2 = 4
    let euler = v - e + f;
    assert!(
        euler == 2 || euler == 4,
        "Euler V-E+F should be 2 (single shell) or 4 (cavity): V={v}, E={e}, F={f}, got {euler}"
    );
}

// ── Watertightness ──────────────────────────────────────────────

/// Verify that every edge is shared by exactly 2 faces (closed manifold shell).
fn assert_watertight(kernel: &TruckKernel, handle: &KernelSolidHandle, label: &str) {
    let introspect = TruckIntrospect::new(kernel);
    let edges = introspect.list_edges(handle);

    for edge_id in &edges {
        let faces = introspect.edge_faces(*edge_id);
        assert_eq!(
            faces.len(),
            2,
            "{label}: edge {edge_id:?} should be shared by exactly 2 faces, got {}",
            faces.len()
        );
    }
}

#[test]
fn test_watertight_box() {
    let mut kernel = TruckKernel::new();
    let h = make_offset_box(&mut kernel, 0.0, 0.0, 0.0, 1.0, 1.0, 1.0);
    assert_watertight(&kernel, &h, "box");
}

#[test]
fn test_watertight_union() {
    let mut kernel = TruckKernel::new();
    let h_a = make_offset_box(&mut kernel, 0.0, 0.0, 0.0, 2.0, 2.0, 2.0);
    let h_b = make_offset_box(&mut kernel, 0.5, 0.5, 0.5, 1.0, 1.0, 1.0);

    let h_union = kernel
        .boolean_union(&h_a, &h_b)
        .expect("union should succeed");
    assert_watertight(&kernel, &h_union, "union result");
}

#[test]
fn test_watertight_intersect() {
    let mut kernel = TruckKernel::new();
    let h_a = make_offset_box(&mut kernel, 0.0, 0.0, 0.0, 2.0, 2.0, 2.0);
    let h_b = make_offset_box(&mut kernel, 0.5, 0.5, 0.5, 1.0, 1.0, 1.0);

    let h_int = kernel
        .boolean_intersect(&h_a, &h_b)
        .expect("intersect should succeed");
    assert_watertight(&kernel, &h_int, "intersect result");
}

// ── Subtract Volume Correctness ─────────────────────────────────

#[test]
fn test_subtract_volume_correctness() {
    // vol(A - B) ≈ vol(A) - vol(intersect(A, B))
    // When B is fully inside A: vol(A - B) ≈ vol(A) - vol(B)
    let mut kernel = TruckKernel::new();

    // Box A: [0,2]^3, vol = 8
    let h_a = make_offset_box(&mut kernel, 0.0, 0.0, 0.0, 2.0, 2.0, 2.0);
    let vol_a = volume_of(&mut kernel, &h_a);

    // Box B: [0.5,1.5]^3, vol = 1 — fully inside A
    let h_b = make_offset_box(&mut kernel, 0.5, 0.5, 0.5, 1.0, 1.0, 1.0);
    let vol_b = volume_of(&mut kernel, &h_b);

    // subtract(A, B)
    let h_a2 = make_offset_box(&mut kernel, 0.0, 0.0, 0.0, 2.0, 2.0, 2.0);
    let h_b2 = make_offset_box(&mut kernel, 0.5, 0.5, 0.5, 1.0, 1.0, 1.0);
    let h_sub = kernel
        .boolean_subtract(&h_a2, &h_b2)
        .expect("subtract should succeed");
    let vol_sub = volume_of(&mut kernel, &h_sub);

    let expected = vol_a - vol_b;
    assert_volume_approx(
        vol_sub,
        expected,
        0.05,
        "subtract volume: vol(A-B) ≈ vol(A) - vol(B)",
    );
}

// ── Intersect Volume Correctness ────────────────────────────────

#[test]
fn test_intersect_volume_fully_contained() {
    // When B is fully inside A, intersect(A, B) = B.
    let mut kernel = TruckKernel::new();

    let h_b = make_offset_box(&mut kernel, 0.5, 0.5, 0.5, 1.0, 1.0, 1.0);
    let vol_b = volume_of(&mut kernel, &h_b);

    let h_a = make_offset_box(&mut kernel, 0.0, 0.0, 0.0, 2.0, 2.0, 2.0);
    let h_b2 = make_offset_box(&mut kernel, 0.5, 0.5, 0.5, 1.0, 1.0, 1.0);

    let h_int = kernel
        .boolean_intersect(&h_a, &h_b2)
        .expect("intersect should succeed");
    let vol_int = volume_of(&mut kernel, &h_int);

    assert_volume_approx(vol_int, vol_b, 0.05, "intersect(A,B)=B when B inside A");
}

// ── Union Volume Correctness ────────────────────────────────────

#[test]
fn test_union_volume_fully_contained() {
    // When B is fully inside A, union(A, B) = A.
    let mut kernel = TruckKernel::new();

    let h_a = make_offset_box(&mut kernel, 0.0, 0.0, 0.0, 2.0, 2.0, 2.0);
    let vol_a = volume_of(&mut kernel, &h_a);

    let h_a2 = make_offset_box(&mut kernel, 0.0, 0.0, 0.0, 2.0, 2.0, 2.0);
    let h_b = make_offset_box(&mut kernel, 0.5, 0.5, 0.5, 1.0, 1.0, 1.0);

    let h_union = kernel
        .boolean_union(&h_a2, &h_b)
        .expect("union should succeed");
    let vol_union = volume_of(&mut kernel, &h_union);

    assert_volume_approx(vol_union, vol_a, 0.05, "union(A,B)=A when B inside A");
}
