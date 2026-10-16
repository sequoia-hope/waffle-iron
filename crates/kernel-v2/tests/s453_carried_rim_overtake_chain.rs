//! Stage-4 §4.5.3 OVERTAKE on a CARRIED rim circle (P0036's shape,
//! 2026-10-12; spec `yang_453_line_overtake` §6).
//!
//! A circle (r 4e-4) revolved 300° about an in-plane axis 7e-4 away is a
//! bent tube whose START cap is a disc in the sketch plane z = −1e-4. A
//! vertical cylinder (r 1.25e-3) cuts it. Stage 1 renders the cap rim as a
//! 13-gon and the cutter as a 12-gon whose chords recede from its circle by
//! a 4.26e-5 sagitta; one rim vertex sits 3.4e-5 INSIDE the exact cutter and
//! outside its chord polygon, so Stage 2 keeps it, and the arrangement's rim
//! × cutter corner lands 1° beyond it. Stage 4 relocates that corner in
//! closed form onto the exact rim ∩ cutter point, 5.2° back along the rim —
//! PAST the kept rim vertex — and the cap loop walks forward-back along one
//! circle: kernel-v2 rejected the ring (`TessellationFailed "ring rejected
//! by CDT (degenerate/self-intersecting)"`).
//!
//! Yang §4.5.3 names the remedy — "a reverse sequence of points after
//! convergence" is removed — and the sweep's straight-run OVERTAKE arm
//! (R0059) already applied it to a relocated junction overtaking unmoved
//! run vertices; the carried-curve arm applies the same rule on an
//! operand's own typed B-Rep edge, tested by the curve's exact parameter
//! order. Before the arm (its `YANG_453_OVERTAKE` switch was deleted
//! 2026-10-16) the cut STOPped as P0036; with it: the cut validates, and no cap rim vertex survives inside
//! the cutter.

use cad_primitives::{BoolOp, Point2, Point3, Vector3};
use kernel_v2::{
    boolean_op, extrude, revolve, tessellate, validate_solid, BrepArena, Profile, RenderMesh,
    SolidId,
};

/// The revolve: sketch plane z = Z0 (normal +z), circle of radius R_TUBE at
/// the sketch origin O_TUBE, axis through A_AXIS along +x, swept 300°.
const Z0: f64 = -1.0e-4;
const O_TUBE: [f64; 3] = [-2.0e-4, -1.0e-4, Z0];
const R_TUBE: f64 = 4.0e-4;
const A_AXIS: [f64; 3] = [-2.0e-4, 6.0e-4, Z0];
const SWEEP_DEG: f64 = 300.0;
/// The cutter: circle of radius R_CUT at O_CUT on a +z plane, extruded
/// DEPTH_CUT along +z (spans the whole tube).
const O_CUT: [f64; 3] = [-6.25e-5, 1.04e-3, -1.43e-3];
const R_CUT: f64 = 1.25e-3;
const DEPTH_CUT: f64 = 2.9e-3;

/// The sketch basis the engine derives for a +z plane: x = X̂ × ẑ = −ŷ,
/// y = ẑ × x = x̂ (a circle at the origin is symmetric under it).
fn plus_z_profile_circle(origin: [f64; 3], radius: f64) -> Profile {
    Profile::circle(
        Point3::new(origin[0], origin[1], origin[2]),
        Vector3::new(0.0, -1.0, 0.0),
        Vector3::new(1.0, 0.0, 0.0),
        Point2::new(0.0, 0.0),
        radius,
    )
    .expect("circle profile")
}

fn tube(a: &mut BrepArena) -> SolidId {
    let p = plus_z_profile_circle(O_TUBE, R_TUBE);
    revolve(
        a,
        &p,
        Point3::new(A_AXIS[0], A_AXIS[1], A_AXIS[2]),
        Vector3::new(1.0, 0.0, 0.0),
        SWEEP_DEG.to_radians(),
    )
    .expect("tube revolve")
    .solid
}

fn cutter(a: &mut BrepArena) -> SolidId {
    let p = plus_z_profile_circle(O_CUT, R_CUT);
    extrude(a, &p, Vector3::new(0.0, 0.0, 1.0), DEPTH_CUT)
        .expect("cutter extrude")
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

/// Distance from the cutter axis minus its radius: negative INSIDE the cut.
fn cutter_residual(p: [f64; 3]) -> f64 {
    ((p[0] - O_CUT[0]).powi(2) + (p[1] - O_CUT[1]).powi(2)).sqrt() - R_CUT
}

/// Distance from the start cap's centre minus the tube radius: zero ON the
/// cap rim.
fn rim_residual(p: [f64; 3]) -> f64 {
    ((p[0] - O_TUBE[0]).powi(2) + (p[1] - O_TUBE[1]).powi(2)).sqrt() - R_TUBE
}

#[test]
fn a_cylinder_cutting_a_revolve_cap_leaves_no_rim_vertex_inside_the_cut() {
    let mut a = BrepArena::new();
    let t = tube(&mut a);
    let v_tube = mesh_signed_volume(&tessellate(&a, t).expect("tube tessellates"));
    let c = cutter(&mut a);
    let out = boolean_op(&mut a, t, c, BoolOp::Subtract)
        .unwrap_or_else(|e| panic!("tube − cylinder failed: {e:?}"));
    let report = validate_solid(&a, out).expect("output validates");
    assert_eq!(report.shells, 1, "one connected shell");
    assert_eq!(report.euler_lhs, report.euler_rhs);

    let mesh = tessellate(&a, out).expect("output tessellates");
    let vol = mesh_signed_volume(&mesh);
    assert!(
        vol > 0.0 && vol < v_tube,
        "the cut keeps a proper part of the tube: {v_tube} → {vol}"
    );

    // The exact answer keeps the cap rim only OUTSIDE the cutter: a rim
    // vertex inside it is the overtaken Stage-1 chord vertex §4.5.3
    // removes. The rim band is far below the rim's own chord spacing and
    // the depth gate (1e-6) is far below the 3.4e-5 overshoot measured.
    let rim_tol = 1e-9;
    let depth_tol = 1e-6;
    let mut rim_vertices = 0usize;
    for p in mesh.positions.chunks_exact(3) {
        let p = [p[0], p[1], p[2]];
        if (p[2] - Z0).abs() <= rim_tol && rim_residual(p).abs() <= rim_tol {
            rim_vertices += 1;
            assert!(
                cutter_residual(p) >= -depth_tol,
                "cap rim vertex {p:?} survives {:.3e} inside the cutter",
                -cutter_residual(p)
            );
        }
    }
    assert!(rim_vertices >= 3, "the cap rim is rendered: {rim_vertices}");
}
