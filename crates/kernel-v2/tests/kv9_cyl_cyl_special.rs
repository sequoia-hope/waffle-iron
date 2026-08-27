//! PR-KV9 RED — cylinder × cylinder booleans, ANALYTIC SPECIAL cases.
//!
//! ssi-rs already solves both special configurations exactly
//! (Patrikalakis & Maekawa §5.8):
//! - PARALLEL axes → the cross-section circle∩circle: 0/1/2 ruling LINES
//!   (any radii);
//! - EQUAL radius, intersecting (coplanar) axes → the degree-4 intersection
//!   degenerates into TWO planar ellipses in the angle-bisecting planes
//!   (the Steinmetz solid boundary).
//!
//! What's missing is the yang Stage-3/Stage-4 plumbing for curves whose
//! incidence is (Cylinder, Cylinder):
//! - Stage-3 candidate membership uses unpropagated / single-owner chord
//!   bands, so geometrically-on-curve arrangement points fail the line and
//!   ellipse membership tests (`AmbiguousCurve { candidates: 2, matched: 0 }`
//!   — corpus F0041/F0043/F0045/F0058);
//! - Stage-4's line arm accepts only (Cylinder, Plane) incidence and the
//!   ellipse arm only (Cylinder|Cone, Plane), so cases that survive Stage 3
//!   STOP at `LocalRefinementRequired` (corpus F0042/F0056/F0057).
//!
//! Each Steinmetz ellipse lies in a KNOWN plane (its stored normal), so the
//! existing cylinder∩plane relocation closed form applies verbatim with the
//! plane derived from the stored curve. The parallel-case lines lie exactly
//! on both cylinders, so the perpendicular-foot relocation is exact.
//!
//! The IRREDUCIBLE degree-4 quartic (skew axes / unequal radii,
//! non-parallel) stays loudly walled — these tests pin that too.

use cad_primitives::{BoolOp, Point2, Point3, Vector3};
use kernel_v2::{boolean_op, extrude, tessellate, validate_solid, BrepArena, Profile, RenderMesh};

#[allow(clippy::too_many_arguments)]
fn cyl(
    a: &mut BrepArena,
    origin: [f64; 3],
    x_axis: [f64; 3],
    y_axis: [f64; 3],
    dir: [f64; 3],
    center: (f64, f64),
    r: f64,
    depth: f64,
) -> kernel_v2::SolidId {
    let p = Profile::circle(
        Point3::new(origin[0], origin[1], origin[2]),
        Vector3::new(x_axis[0], x_axis[1], x_axis[2]),
        Vector3::new(y_axis[0], y_axis[1], y_axis[2]),
        Point2::new(center.0, center.1),
        r,
    )
    .unwrap();
    extrude(a, &p, Vector3::new(dir[0], dir[1], dir[2]), depth)
        .unwrap()
        .solid
}

fn mesh_signed_volume(mesh: &RenderMesh) -> f64 {
    let p = |i: u32| {
        let k = (i as usize) * 3;
        [
            mesh.positions[k],
            mesh.positions[k + 1],
            mesh.positions[k + 2],
        ]
    };
    let mut six_v = 0.0;
    for t in mesh.indices.chunks_exact(3) {
        let (a, b, c) = (p(t[0]), p(t[1]), p(t[2]));
        six_v += a[0] * (b[1] * c[2] - b[2] * c[1])
            + a[1] * (b[2] * c[0] - b[0] * c[2])
            + a[2] * (b[0] * c[1] - b[1] * c[0]);
    }
    six_v / 6.0
}

/// Lens area of two overlapping circles (radii r1, r2, center distance d) —
/// the exact circle–circle intersection area.
fn lens_area(r1: f64, r2: f64, d: f64) -> f64 {
    let a1 = ((d * d + r1 * r1 - r2 * r2) / (2.0 * d * r1)).clamp(-1.0, 1.0);
    let a2 = ((d * d + r2 * r2 - r1 * r1) / (2.0 * d * r2)).clamp(-1.0, 1.0);
    let t1 = a1.acos();
    let t2 = a2.acos();
    r1 * r1 * (t1 - t1.sin() * t1.cos()) + r2 * r2 * (t2 - t2.sin() * t2.cos())
}

// ── parallel axes (cross-section circle∩circle → two ruling lines) ─────────

/// F0042-class: two parallel z-axis cylinders, unequal radii, overlapping
/// laterally; SUBTRACT. Exact volume: π·r1²·h − lens·overlap_height.
#[test]
fn parallel_cyl_subtract_exact_volume() {
    let mut a = BrepArena::new();
    let (r1, r2, d) = (0.30, 0.22, 0.35); // secant: |r1−r2| < d < r1+r2
    let c1 = cyl(
        &mut a,
        [0.0, 0.0, 0.0],
        [1.0, 0.0, 0.0],
        [0.0, 1.0, 0.0],
        [0.0, 0.0, 1.0],
        (0.0, 0.0),
        r1,
        1.0,
    );
    // Tool: overlaps z ∈ [0.4, 1.4] → removes lens × 0.6 from the body.
    let c2 = cyl(
        &mut a,
        [0.0, 0.0, 0.4],
        [1.0, 0.0, 0.0],
        [0.0, 1.0, 0.0],
        [0.0, 0.0, 1.0],
        (d, 0.0),
        r2,
        1.0,
    );
    let out = boolean_op(&mut a, c1, c2, BoolOp::Subtract)
        .unwrap_or_else(|e| panic!("parallel cyl − cyl: {e:?}"));
    validate_solid(&a, out).expect("validates");
    let vol = mesh_signed_volume(&tessellate(&a, out).expect("tessellate"));
    let expect = std::f64::consts::PI * r1 * r1 * 1.0 - lens_area(r1, r2, d) * 0.6;
    // Both surfaces are chord-tessellated; the band is the two cylinders'
    // chord under-fill, NOT a geometric tolerance.
    assert!(
        vol <= expect * 1.001 && vol >= 0.93 * expect,
        "volume {vol} vs {expect}"
    );
}

/// Same configuration, UNION (the F0041/F0043/F0045 class — these failed at
/// Stage 3 with AmbiguousCurve {candidates: 2, matched: 0}).
#[test]
fn parallel_cyl_union_exact_volume() {
    let mut a = BrepArena::new();
    let (r1, r2, d) = (0.2852, 0.1321, 0.30); // F0041-like radii, secant
    let c1 = cyl(
        &mut a,
        [0.0, 0.0, 0.0],
        [1.0, 0.0, 0.0],
        [0.0, 1.0, 0.0],
        [0.0, 0.0, 1.0],
        (0.0, 0.0),
        r1,
        0.8,
    );
    let c2 = cyl(
        &mut a,
        [0.0, 0.0, 0.3],
        [1.0, 0.0, 0.0],
        [0.0, 1.0, 0.0],
        [0.0, 0.0, 1.0],
        (d, 0.0),
        r2,
        0.8,
    );
    let out = boolean_op(&mut a, c1, c2, BoolOp::Union)
        .unwrap_or_else(|e| panic!("parallel cyl ∪ cyl: {e:?}"));
    validate_solid(&a, out).expect("validates");
    let vol = mesh_signed_volume(&tessellate(&a, out).expect("tessellate"));
    let lens = lens_area(r1, r2, d);
    let expect = std::f64::consts::PI * (r1 * r1 + r2 * r2) * 0.8 - lens * 0.5;
    assert!(
        vol <= expect * 1.001 && vol >= 0.93 * expect,
        "volume {vol} vs {expect}"
    );
}

// ── equal radius, perpendicular intersecting axes (two ellipses) ───────────

/// F0056-class: equal-radius perpendicular cylinders, axes intersecting —
/// the Steinmetz configuration; UNION. Exact: V = V1 + V2 − 16r³/3
/// (the bicylinder common volume), with both axes crossing mid-solid.
#[test]
#[ignore = "KV9-F1 (spec kv9_f1_tangency_inout_labels): layers 1+2 FIXED (N24 predicate \
            zero-certification; Increment 0c Stage-4 tangency-junction band). The union \
            now stops LOUDLY at Stage-6 s6-curved-degenerate-loop: extract_boundary_cycles \
            interleaves the top/bottom lens cycles at the 4-valent tangency junction into \
            a Newell-cancelling figure-eight — junction-aware boundary-walk continuation \
            is the next increment (spec §2c.5a)"]
fn steinmetz_union_exact_volume() {
    let mut a = BrepArena::new();
    let r = 0.3;
    // Cylinder 1: z-axis through origin, z ∈ [-0.6, 0.6].
    let c1 = cyl(
        &mut a,
        [0.0, 0.0, -0.6],
        [1.0, 0.0, 0.0],
        [0.0, 1.0, 0.0],
        [0.0, 0.0, 1.0],
        (0.0, 0.0),
        r,
        1.2,
    );
    // Cylinder 2: x-axis through origin, x ∈ [-0.6, 0.6].
    let c2 = cyl(
        &mut a,
        [-0.6, 0.0, 0.0],
        [0.0, 1.0, 0.0],
        [0.0, 0.0, 1.0],
        [1.0, 0.0, 0.0],
        (0.0, 0.0),
        r,
        1.2,
    );
    let out = boolean_op(&mut a, c1, c2, BoolOp::Union)
        .unwrap_or_else(|e| panic!("steinmetz union: {e:?}"));
    validate_solid(&a, out).expect("validates");
    let vol = mesh_signed_volume(&tessellate(&a, out).expect("tessellate"));
    let v_cyl = std::f64::consts::PI * r * r * 1.2;
    let bicyl = 16.0 * r * r * r / 3.0;
    let expect = 2.0 * v_cyl - bicyl;
    assert!(
        vol <= expect * 1.001 && vol >= 0.92 * expect,
        "volume {vol} vs {expect}"
    );
}

/// F0058-class: equal-radius perpendicular SUBTRACT — body minus the
/// crossing rod removes exactly the bicylinder volume.
#[test]
#[ignore = "KV9-F1 (spec kv9_f1_tangency_inout_labels): layers 1+2 FIXED (N24 predicate \
            zero-certification; Increment 0c Stage-4 tangency-junction band). The subtract \
            now CLEARS yang-rs (exact-volume oracle green in yang-rs \
            kv9f1_tangency_junction) and walls at kernel-v2 import NonManifoldVertex: four \
            elliptical arcs sharing BOTH endpoints (two per ellipse) defeat the \
            vertex-pair edge keying — the same-ellipse-bigon arc-keying increment \
            (spec §2c.5b, the M8 disc∩disc CurveKey lesson)"]
fn steinmetz_subtract_exact_volume() {
    let mut a = BrepArena::new();
    let r = 0.2;
    let c1 = cyl(
        &mut a,
        [0.0, 0.0, -0.45],
        [1.0, 0.0, 0.0],
        [0.0, 1.0, 0.0],
        [0.0, 0.0, 1.0],
        (0.0, 0.0),
        r,
        0.9,
    );
    let c2 = cyl(
        &mut a,
        [-0.45, 0.0, 0.0],
        [0.0, 1.0, 0.0],
        [0.0, 0.0, 1.0],
        [1.0, 0.0, 0.0],
        (0.0, 0.0),
        r,
        0.9,
    );
    let out = boolean_op(&mut a, c1, c2, BoolOp::Subtract)
        .unwrap_or_else(|e| panic!("steinmetz subtract: {e:?}"));
    validate_solid(&a, out).expect("validates");
    let vol = mesh_signed_volume(&tessellate(&a, out).expect("tessellate"));
    let v_cyl = std::f64::consts::PI * r * r * 0.9;
    let bicyl = 16.0 * r * r * r / 3.0;
    let expect = v_cyl - bicyl;
    assert!(
        vol <= expect * 1.005 && vol >= 0.90 * expect,
        "volume {vol} vs {expect}"
    );
}

/// M5 (`specs/m5_surface_pair_curve.md`) — REVISED at #173 (2026-07-17),
/// FLIPPED BACK 2026-08-26: the #173 revision pinned the N6 selfx STOP
/// (79 render-level penetrations) and named its root fix "[M5]/#172
/// territory (exact degree-4 trim + CONFORMAL SEAM SAMPLING)". The
/// kernel-v2 conformal grid-aligned arc sampling (spec
/// `yang_434_output_chord_refinement.md` inc-4) is that sampling half:
/// under it the emitted shell passes the selfx gate, so the union
/// completes and must now VALIDATE — watertight, and volume inside the
/// strict union bounds (max(V1,V2), V1+V2). History:
/// `unequal_perpendicular_walls_on_selfx_gate` ←
/// `unequal_perpendicular_now_supported` ←
/// `unequal_perpendicular_stays_walled`.
#[test]
fn unequal_perpendicular_now_supported() {
    let mut a = BrepArena::new();
    let (r1, r2) = (0.3_f64, 0.18_f64);
    let c1 = cyl(
        &mut a,
        [0.0, 0.0, -0.5],
        [1.0, 0.0, 0.0],
        [0.0, 1.0, 0.0],
        [0.0, 0.0, 1.0],
        (0.0, 0.0),
        r1,
        1.0,
    );
    let c2 = cyl(
        &mut a,
        [-0.5, 0.0, 0.0],
        [0.0, 1.0, 0.0],
        [0.0, 0.0, 1.0],
        [1.0, 0.0, 0.0],
        (0.0, 0.0),
        r2,
        1.0,
    );
    let out = boolean_op(&mut a, c1, c2, BoolOp::Union)
        .expect("[M5]/#172: under conformal seam sampling the union passes the selfx gate");
    kernel_v2::validate_solid(&a, out).expect("union output validates");
    let mesh = kernel_v2::tessellate(&a, out).expect("tessellates");
    let vol = mesh_signed_volume(&mesh);
    let (v1, v2) = (
        std::f64::consts::PI * r1 * r1 * 1.0,
        std::f64::consts::PI * r2 * r2 * 1.0,
    );
    assert!(
        vol > v1.max(v2) && vol < v1 + v2,
        "union volume {vol} outside ({}, {})",
        v1.max(v2),
        v1 + v2
    );
}

/// Determinism across identical builds.
#[test]
fn cyl_cyl_special_deterministic() {
    let build = || {
        let mut a = BrepArena::new();
        let c1 = cyl(
            &mut a,
            [0.0, 0.0, 0.0],
            [1.0, 0.0, 0.0],
            [0.0, 1.0, 0.0],
            [0.0, 0.0, 1.0],
            (0.0, 0.0),
            0.30,
            1.0,
        );
        let c2 = cyl(
            &mut a,
            [0.0, 0.0, 0.4],
            [1.0, 0.0, 0.0],
            [0.0, 1.0, 0.0],
            [0.0, 0.0, 1.0],
            (0.35, 0.0),
            0.22,
            1.0,
        );
        let out = boolean_op(&mut a, c1, c2, BoolOp::Subtract).expect("cut");
        let mesh = tessellate(&a, out).expect("tessellate");
        (a, mesh)
    };
    let (a1, m1) = build();
    let (a2, m2) = build();
    assert_eq!(a1, a2);
    assert_eq!(m1, m2);
}
