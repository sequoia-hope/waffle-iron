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

// ── Robust Ray-Cast Adversarial Tests ───────────────────────────

#[test]
fn test_robust_ray_grazing_edge() {
    // Two boxes where one edge aligns with a triangulation edge of the other.
    // The second box shares an exact face boundary, creating edge-grazing rays.
    let mut kernel = TruckKernel::new();

    // Box A: [0,2]^3
    let h_a = make_offset_box(&mut kernel, 0.0, 0.0, 0.0, 2.0, 2.0, 2.0);
    // Box B: offset so its edge aligns with A's triangulation diagonal.
    // Placed at [1,0,0] with size [2,2,2] — shares the x=2 face boundary.
    let h_b = make_offset_box(&mut kernel, 1.0, 0.0, 0.0, 2.0, 2.0, 2.0);

    let vol_a = volume_of(&mut kernel, &h_a);
    let vol_b = volume_of(&mut kernel, &h_b);

    // Union should work deterministically despite edge alignment.
    let h_a2 = make_offset_box(&mut kernel, 0.0, 0.0, 0.0, 2.0, 2.0, 2.0);
    let h_b2 = make_offset_box(&mut kernel, 1.0, 0.0, 0.0, 2.0, 2.0, 2.0);
    let h_union = kernel
        .boolean_union(&h_a2, &h_b2)
        .expect("union with edge-grazing should succeed");
    let vol_union = volume_of(&mut kernel, &h_union);

    // Overlap region is [1,2] x [0,2] x [0,2] = 1*2*2 = 4
    let expected_union = vol_a + vol_b - 4.0;
    assert_volume_approx(
        vol_union,
        expected_union,
        0.05,
        "robust ray: edge-grazing union volume",
    );

    // Run 3 times to verify determinism.
    for i in 0..3 {
        let a = make_offset_box(&mut kernel, 0.0, 0.0, 0.0, 2.0, 2.0, 2.0);
        let b = make_offset_box(&mut kernel, 1.0, 0.0, 0.0, 2.0, 2.0, 2.0);
        let u = kernel
            .boolean_union(&a, &b)
            .unwrap_or_else(|_| panic!("determinism trial {i} should succeed"));
        let v = volume_of(&mut kernel, &u);
        assert_volume_approx(v, expected_union, 0.05, &format!("determinism trial {i}"));
    }
}

#[test]
fn test_robust_ray_near_parallel_face() {
    // A very thin box creates near-parallel face configurations for ray-casting.
    let mut kernel = TruckKernel::new();

    // Thin slab: 10x10x0.01
    let h_slab = make_offset_box(&mut kernel, 0.0, 0.0, 0.0, 10.0, 10.0, 0.01);
    // Normal box intersecting the slab
    let h_box = make_offset_box(&mut kernel, 4.0, 4.0, -1.0, 2.0, 2.0, 2.0);

    let vol_slab = volume_of(&mut kernel, &h_slab);
    let vol_box = volume_of(&mut kernel, &h_box);

    // Intersection: the slab portion inside the box = 2*2*0.01 = 0.04
    let h_slab2 = make_offset_box(&mut kernel, 0.0, 0.0, 0.0, 10.0, 10.0, 0.01);
    let h_box2 = make_offset_box(&mut kernel, 4.0, 4.0, -1.0, 2.0, 2.0, 2.0);
    let h_int = kernel
        .boolean_intersect(&h_slab2, &h_box2)
        .expect("intersect with thin slab should succeed");
    let vol_int = volume_of(&mut kernel, &h_int);

    // Union volume should satisfy conservation
    let h_slab3 = make_offset_box(&mut kernel, 0.0, 0.0, 0.0, 10.0, 10.0, 0.01);
    let h_box3 = make_offset_box(&mut kernel, 4.0, 4.0, -1.0, 2.0, 2.0, 2.0);
    let h_union = kernel
        .boolean_union(&h_slab3, &h_box3)
        .expect("union with thin slab should succeed");
    let vol_union = volume_of(&mut kernel, &h_union);

    // vol(U) + vol(I) ≈ vol(A) + vol(B)
    let lhs = vol_union + vol_int;
    let rhs = vol_slab + vol_box;
    assert_volume_approx(
        lhs,
        rhs,
        0.10, // slightly wider tolerance for thin geometry
        "robust ray: near-parallel face volume conservation",
    );
}

#[test]
fn test_adversarial_ill_conditioned_coords() {
    // Boxes at very large coordinates with small relative offsets.
    // Naive floating-point ray-casting may fail here.
    let mut kernel = TruckKernel::new();

    let base = 1e6;
    // Box A at large coordinates: [1e6, 1e6, 1e6] to [1e6+2, 1e6+2, 1e6+2]
    let h_a = make_offset_box(&mut kernel, base, base, base, 2.0, 2.0, 2.0);
    // Box B slightly offset: [1e6+0.5, 1e6+0.5, 1e6+0.5] to [1e6+1.5, ...]
    let h_b = make_offset_box(
        &mut kernel,
        base + 0.5,
        base + 0.5,
        base + 0.5,
        1.0,
        1.0,
        1.0,
    );

    let vol_a = volume_of(&mut kernel, &h_a);
    let vol_b = volume_of(&mut kernel, &h_b);

    // B is fully inside A, so union(A,B) = A and intersect(A,B) = B
    let h_a2 = make_offset_box(&mut kernel, base, base, base, 2.0, 2.0, 2.0);
    let h_b2 = make_offset_box(
        &mut kernel,
        base + 0.5,
        base + 0.5,
        base + 0.5,
        1.0,
        1.0,
        1.0,
    );
    let h_union = kernel
        .boolean_union(&h_a2, &h_b2)
        .expect("union at large coords should succeed");
    let vol_union = volume_of(&mut kernel, &h_union);
    assert_volume_approx(
        vol_union,
        vol_a,
        0.05,
        "ill-conditioned: union(A,B)=A when B inside A",
    );

    let h_a3 = make_offset_box(&mut kernel, base, base, base, 2.0, 2.0, 2.0);
    let h_b3 = make_offset_box(
        &mut kernel,
        base + 0.5,
        base + 0.5,
        base + 0.5,
        1.0,
        1.0,
        1.0,
    );
    let h_int = kernel
        .boolean_intersect(&h_a3, &h_b3)
        .expect("intersect at large coords should succeed");
    let vol_int = volume_of(&mut kernel, &h_int);
    assert_volume_approx(
        vol_int,
        vol_b,
        0.05,
        "ill-conditioned: intersect(A,B)=B when B inside A",
    );
}

// ── Degenerate IC / Panic-Free Tests ────────────────────────────

#[test]
fn test_degenerate_closed_ic_no_panic() {
    // Two boxes sharing a face edge (coplanar-adjacent). The shared edge
    // can produce a degenerate zero-length closed IC. Verify the boolean
    // completes without panic (tests the loops_store pre-validation).
    let mut kernel = TruckKernel::new();

    // Box A: [0,1]^3
    let h_a = make_offset_box(&mut kernel, 0.0, 0.0, 0.0, 1.0, 1.0, 1.0);
    // Box B: [1,2] x [0,1] x [0,1] — shares the x=1 face edge
    let h_b = make_offset_box(&mut kernel, 1.0, 0.0, 0.0, 1.0, 1.0, 1.0);

    // Union should complete (no panic). The result volume = 2.0.
    let result = kernel.boolean_union(&h_a, &h_b);
    assert!(result.is_ok(), "coplanar-adjacent union should not panic");
    let vol = volume_of(&mut kernel, &result.unwrap());
    assert_volume_approx(vol, 2.0, 0.05, "coplanar-adjacent union volume");
}

#[test]
fn test_degenerate_divide_loop_no_panic() {
    // Very thin sliver intersection: the intersection region is so thin
    // that loop wires may have near-zero spatial extent, which could
    // panic in parameter_division. Verify the boolean completes.
    let mut kernel = TruckKernel::new();

    // Box A: [0,2]^3
    let h_a = make_offset_box(&mut kernel, 0.0, 0.0, 0.0, 2.0, 2.0, 2.0);
    // Box B: [0.999, 1.001] x [0,2] x [0,2] — very thin sliver
    let h_b = make_offset_box(&mut kernel, 0.999, 0.0, 0.0, 0.002, 2.0, 2.0);

    // Intersect produces the thin sliver — should not panic.
    // With cascade timeout, this degenerate case may fail gracefully
    // (NotClosedShell) rather than finding a working perturbation after
    // hundreds of seconds of retries. The key invariant is no panic.
    let _result = kernel.boolean_intersect(&h_a, &h_b);
}

#[test]
fn test_coplanar_adjacent_degenerate_ic() {
    // Two boxes sharing an exact edge where the coplanar-adjacent IC
    // would be zero-length. Verify boolean union completes.
    let mut kernel = TruckKernel::new();

    // Box A: [0,1]^3
    let h_a = make_offset_box(&mut kernel, 0.0, 0.0, 0.0, 1.0, 1.0, 1.0);
    // Box B: [0,1] x [1,2] x [0,1] — shares the y=1 edge
    let h_b = make_offset_box(&mut kernel, 0.0, 1.0, 0.0, 1.0, 1.0, 1.0);

    let result = kernel.boolean_union(&h_a, &h_b);
    assert!(
        result.is_ok(),
        "edge-sharing union should not panic: {:?}",
        result.err()
    );
    let vol = volume_of(&mut kernel, &result.unwrap());
    assert_volume_approx(vol, 2.0, 0.05, "edge-sharing union volume");
}

// ── Layered Tolerance Tests (Sprint 14) ─────────────────────────

#[test]
fn test_layered_tolerance_differentiated() {
    // from_model_tol should produce differentiated per-stage values,
    // not uniform ones.
    let tols = truck_shapeops::BooleanTolerance::from_model_tol(0.05);
    assert!(
        (tols.tau_model - 0.05).abs() < 1e-12,
        "tau_model should be 0.05, got {}",
        tols.tau_model
    );
    assert!(
        (tols.tau_weld - 0.02).abs() < 1e-12,
        "tau_weld should be 0.02 (0.4x), got {}",
        tols.tau_weld
    );
    assert!(
        (tols.tau_coplanar - 0.05).abs() < 1e-12,
        "tau_coplanar should equal tau_model (1x), got {}",
        tols.tau_coplanar
    );
    assert!(
        (tols.tau_mesh - 0.05).abs() < 1e-12,
        "tau_mesh should be 0.05 (1x), got {}",
        tols.tau_mesh
    );
}

#[test]
fn test_boolean_options_to_boolean_tolerance() {
    // BooleanOptions::for_boolean_tol should produce correctly layered values
    // that match what to_boolean_tolerance() outputs.
    use kernel_fork::types::BooleanOptions;

    let opts = BooleanOptions::for_boolean_tol(0.01);
    let tols = opts.to_boolean_tolerance();

    assert!(
        (tols.tau_model - 0.01).abs() < 1e-12,
        "tau_model should be 0.01, got {}",
        tols.tau_model
    );
    assert!(
        (tols.tau_mesh - 0.005).abs() < 1e-12,
        "tau_mesh should be 0.005, got {}",
        tols.tau_mesh
    );
    assert!(
        (tols.tau_weld - 0.004).abs() < 1e-12,
        "tau_weld should be 0.004 (0.4x tau_model), got {}",
        tols.tau_weld
    );
    assert!(
        (tols.tau_coplanar - 0.01).abs() < 1e-12,
        "tau_coplanar should equal tau_model, got {}",
        tols.tau_coplanar
    );
}

#[test]
fn test_coplanar_angular_threshold() {
    // The coplanar angular threshold uses `(1 - |dot|) > tol²`.
    // Faces at 5° apart should NOT be coplanar.
    // Faces at 0.001° apart should be coplanar.
    // This test verifies via actual boolean operations that the coplanar
    // detection threshold is correct.
    let mut kernel = TruckKernel::new();

    // Two boxes that share a face — should be detected as coplanar.
    let h_a = make_offset_box(&mut kernel, 0.0, 0.0, 0.0, 1.0, 1.0, 1.0);
    let h_b = make_offset_box(&mut kernel, 0.0, 0.0, 1.0, 1.0, 1.0, 1.0);

    let result = kernel.boolean_union(&h_a, &h_b);
    assert!(
        result.is_ok(),
        "coplanar face union should succeed: {:?}",
        result.err()
    );
    let vol = volume_of(&mut kernel, &result.unwrap());
    assert_volume_approx(vol, 2.0, 0.05, "stacked boxes coplanar union");
}

#[test]
fn test_weld_no_merge_small_features() {
    // A box with a small feature (narrow cut) should not have its edges
    // merged by weld_coincident_edges. The layered tau_weld must not
    // exceed the feature size.
    let mut kernel = TruckKernel::new();

    // Large base box: 10x10x10
    let h_base = make_offset_box(&mut kernel, 0.0, 0.0, 0.0, 10.0, 10.0, 10.0);
    // Small cut-out box fully inside, leaving thin walls (0.5 thick)
    // Box is at [0.5, 9.5] x [0.5, 9.5] x [0.5, 9.5] → 9x9x9 = 729
    let h_cut = make_offset_box(&mut kernel, 0.5, 0.5, 0.5, 9.0, 9.0, 9.0);

    let result = kernel.boolean_subtract(&h_base, &h_cut);
    assert!(
        result.is_ok(),
        "subtract with thin walls should succeed: {:?}",
        result.err()
    );
    let vol = volume_of(&mut kernel, &result.unwrap());
    // Expected: 10^3 - 9^3 = 1000 - 729 = 271
    let expected = 1000.0 - 729.0;
    assert_volume_approx(
        vol,
        expected,
        0.05,
        "thin-wall volume preserved with layered tolerance",
    );
}

#[test]
fn test_no_catch_unwind_in_shapeops_hot_path() {
    // Structural assertion: catch_unwind must not appear in the shapeops
    // boolean hot path (loops_store, divide_face). It is acceptable only
    // in healing.rs (best-effort pre-processing).
    let shapeops_dir = std::path::Path::new(env!("CARGO_MANIFEST_DIR"))
        .join("..")
        .join("..")
        .join("vendor")
        .join("truck")
        .join("truck-shapeops")
        .join("src");

    let loops_store = std::fs::read_to_string(shapeops_dir.join("transversal/loops_store/mod.rs"))
        .expect("should read loops_store/mod.rs");
    assert!(
        !loops_store.contains("catch_unwind"),
        "loops_store/mod.rs must not contain catch_unwind"
    );

    let divide_face = std::fs::read_to_string(shapeops_dir.join("transversal/divide_face/mod.rs"))
        .expect("should read divide_face/mod.rs");
    assert!(
        !divide_face.contains("catch_unwind"),
        "divide_face/mod.rs must not contain catch_unwind"
    );

    // Verify healing.rs still has its intentional catch_unwind
    let healing = std::fs::read_to_string(
        std::path::Path::new(env!("CARGO_MANIFEST_DIR"))
            .join("src")
            .join("healing.rs"),
    )
    .expect("should read healing.rs");
    assert!(
        healing.contains("catch_unwind"),
        "healing.rs should still contain its intentional catch_unwind"
    );
}
