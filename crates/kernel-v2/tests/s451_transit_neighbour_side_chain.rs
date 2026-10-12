//! §4.5.1 corner transit — the NEIGHBOUR-SIDE certificate (P0040's shape,
//! 2026-10-12; spec `yang_451_corner_transit` §3aj and
//! `yang_45_boundary_point_domain_certificate` §9).
//!
//! Two plain cylinder bosses: a z-cylinder (r 450.9, h 540.4) and an
//! x-cylinder (r 290.1, x ∈ [−271.5, 39.0], axis at y −112.4, z 370.9) whose
//! start cap pokes out of the big lateral and above the top cap. Near the
//! corner where the small cylinder's start cap meets the big top cap, the big
//! rim's Stage-1 chord (sagitta 10.7) runs INSIDE the small cylinder's
//! footprint by 3.5 while the exact rim clears it by 12, so the arrangement
//! holds mesh triple points — {top cap, big lateral, small lateral} and {top
//! cap, big lateral, start-cap plane} — that the exact topology does not have.
//! Stage 4 solved both exactly on the EXTENDED surfaces: the first 15.4
//! outside the small lateral's x-range, and the §4.5.1 transit "repaired" it
//! onto the start-cap PLANE at the second, 9.9 outside that cap's disc (300.0
//! from its axis against r 290.09). The emission wrote the phantom; the top
//! cap's output loop ran the rim circle and the rectangular hole through one
//! vertex, and kernel-v2 refused the body: `InvalidBooleanOutput("an
//! undirected output edge is not used by exactly two directed edges")`.
//!
//! Two certificate gaps let it through. A step WITHIN a cap plane that leaves
//! the cap's rim had a zero residual against the rim circle's plane (the cap's
//! own plane) at both ends — the domain boundary of a planar face is RADIAL,
//! and its divider is the rim's cylinder. And the transit's neighbour-domain
//! postcondition could not see the crease it had just crossed: `p_trunc` lies
//! ON it, and an on-crease start is exempt from the sign-change test, so the
//! SIDE the corrected junction lands on is now certified against the B-Rep
//! face (a planar face lies inside its OUTER-loop circle). Both transits
//! decline, the §4.5 postcondition records the fires, and the §4.5.2 ladder's
//! first rung (d_ε/2, rim sagitta 2.7 < the 12 clearance) emits a body whose
//! mesh topology is the exact one.
//!
//! RED before the fix (the P0040 STOP, verbatim); GREEN with it: the union
//! validates as one shell with a proper volume between the big boss and the
//! sum of the two.

use cad_primitives::{BoolOp, Point2, Point3, Vector3};
use kernel_v2::{
    boolean_op, extrude, tessellate, validate_solid, BrepArena, Profile, RenderMesh, SolidId,
};

/// The big boss: a circle of radius R_BIG at the origin on the plane z = 0,
/// extruded H_BIG along +z.
const R_BIG: f64 = 450.8960897224892;
const H_BIG: f64 = 540.3786532902026;
/// The small boss: a circle of radius R_SMALL at O_SMALL on the plane x =
/// O_SMALL.x (normal +x), extruded H_SMALL along +x.
const O_SMALL: [f64; 3] = [-271.498950000855, -112.40362793885976, 370.94106542625053];
const R_SMALL: f64 = 290.08925833029934;
const H_SMALL: f64 = 310.5453979432958;

/// A +z plane's frame as the engine derives it: u = x̂ × ẑ = −ŷ, v = ẑ × u =
/// x̂ (a circle at the sketch origin is symmetric under it).
fn plus_z_frame() -> (Vector3, Vector3) {
    (Vector3::new(0.0, -1.0, 0.0), Vector3::new(1.0, 0.0, 0.0))
}

/// A +x plane's frame: u = ẑ × x̂ = ŷ, v = x̂ × ŷ = ẑ.
fn plus_x_frame() -> (Vector3, Vector3) {
    (Vector3::new(0.0, 1.0, 0.0), Vector3::new(0.0, 0.0, 1.0))
}

fn big_boss(a: &mut BrepArena) -> SolidId {
    let (u, v) = plus_z_frame();
    let p = Profile::circle(
        Point3::new(0.0, 0.0, 0.0),
        u,
        v,
        Point2::new(0.0, 0.0),
        R_BIG,
    )
    .expect("big boss profile");
    extrude(a, &p, Vector3::new(0.0, 0.0, 1.0), H_BIG)
        .expect("big boss extrude")
        .solid
}

fn small_boss(a: &mut BrepArena) -> SolidId {
    let (u, v) = plus_x_frame();
    let p = Profile::circle(
        Point3::new(O_SMALL[0], O_SMALL[1], O_SMALL[2]),
        u,
        v,
        Point2::new(0.0, 0.0),
        R_SMALL,
    )
    .expect("small boss profile");
    extrude(a, &p, Vector3::new(1.0, 0.0, 0.0), H_SMALL)
        .expect("small boss extrude")
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

#[test]
fn a_transit_onto_a_cap_plane_outside_its_disc_is_declined_and_the_ladder_converges() {
    let mut a = BrepArena::new();
    let big = big_boss(&mut a);
    let small = small_boss(&mut a);
    let v_big = mesh_signed_volume(&tessellate(&a, big).expect("big boss tessellates"));
    let v_small = mesh_signed_volume(&tessellate(&a, small).expect("small boss tessellates"));
    assert!(v_big > 0.0 && v_small > 0.0);

    // P0040's STOP: the union emitted a top-cap loop pinched through a
    // phantom {top cap, big lateral, start-cap plane} vertex and kernel-v2
    // refused it.
    let out = boolean_op(&mut a, big, small, BoolOp::Union)
        .unwrap_or_else(|e| panic!("big ∪ small failed: {e:?}"));
    let report = validate_solid(&a, out).expect("union validates");
    assert_eq!(report.shells, 1, "one connected shell");
    assert_eq!(report.euler_lhs, report.euler_rhs);
    let mesh = tessellate(&a, out).expect("union tessellates");
    let v_out = mesh_signed_volume(&mesh);
    assert!(
        v_out > v_big && v_out < v_big + v_small,
        "the union adds the small boss's part outside the big one: {v_big} + {v_small} → {v_out}"
    );
    // The phantom corner: nothing of the output may sit on the start-cap
    // plane x = −271.5 at the top-cap height OUTSIDE the small cylinder's
    // disc (the rim point (−271.5, −359.99, 540.38) is 300 from the small
    // axis against r 290.09).
    let on_phantom = mesh.positions.chunks_exact(3).any(|p| {
        (p[0] - O_SMALL[0]).abs() < 1e-6
            && (p[2] - H_BIG).abs() < 1e-6
            && ((p[1] - O_SMALL[1]).powi(2) + (p[2] - O_SMALL[2]).powi(2)).sqrt() > R_SMALL + 1.0
    });
    assert!(!on_phantom, "no output vertex on the phantom corner");
}
