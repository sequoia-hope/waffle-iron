//! M8-mixed E2E (spec `m8_mixed_loop_coplanar_overlay` oracle 5): a chained
//! boolean whose intermediate carries a MIXED Line+Arc planar cap, then a
//! FLUSH stacked union on that cap — the R0021 auto-union shape. Before the
//! mixed overlay admission this walled `NotSupported` ("coplanar input face
//! pair", the `face-unsupported` Stage-0 residue).

use cad_primitives::{BoolOp, Point2, Point3, Vector3};
use kernel_v2::{boolean_op, extrude, tessellate, validate_solid, BrepArena, Profile, RenderMesh};

fn boxx(a: &mut BrepArena, x: (f64, f64), y: (f64, f64), z: (f64, f64)) -> kernel_v2::SolidId {
    let p = Profile::new(
        Point3::new(0.0, 0.0, z.0),
        Vector3::new(1.0, 0.0, 0.0),
        Vector3::new(0.0, 1.0, 0.0),
        vec![
            Point2::new(x.0, y.0),
            Point2::new(x.1, y.0),
            Point2::new(x.1, y.1),
            Point2::new(x.0, y.1),
        ],
        vec![],
    )
    .unwrap();
    extrude(a, &p, Vector3::new(0.0, 0.0, 1.0), z.1 - z.0)
        .unwrap()
        .solid
}

fn cyl(a: &mut BrepArena, cx: f64, cy: f64, r: f64, z: (f64, f64)) -> kernel_v2::SolidId {
    let p = Profile::circle(
        Point3::new(0.0, 0.0, z.0),
        Vector3::new(1.0, 0.0, 0.0),
        Vector3::new(0.0, 1.0, 0.0),
        Point2::new(cx, cy),
        r,
    )
    .unwrap();
    extrude(a, &p, Vector3::new(0.0, 0.0, 1.0), z.1 - z.0)
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
    for t in mesh.indices.chunks(3) {
        let (a, b, c) = (p(t[0]), p(t[1]), p(t[2]));
        six_v += a[0] * (b[1] * c[2] - b[2] * c[1]) - a[1] * (b[0] * c[2] - b[2] * c[0])
            + a[2] * (b[0] * c[1] - b[1] * c[0]);
    }
    six_v / 6.0
}

fn volume_of(a: &BrepArena, s: kernel_v2::SolidId) -> f64 {
    mesh_signed_volume(&tessellate(a, s).expect("tessellate"))
}

/// Box − full-height corner cylinder (top cap becomes segments + one arc,
/// the minimal chained MIXED face) → flush unit-box union on that cap, away
/// from the arc. The union interfaces are planar, so its increment is EXACT.
#[test]
fn mixed_cap_flush_stack_union() {
    let mut a = BrepArena::new();
    let base = boxx(&mut a, (0.0, 4.0), (0.0, 4.0), (0.0, 2.0));
    // Corner column at (0,0), r=1, through the full height: the survivor's
    // top cap outer loop mixes LineSegments with one quarter arc.
    let column = cyl(&mut a, 0.0, 0.0, 1.0, (-0.5, 2.5));
    let notched = boolean_op(&mut a, base, column, BoolOp::Subtract).expect("box - corner column");
    validate_solid(&a, notched).expect("notched box validates");
    let v1 = volume_of(&a, notched);
    // Removed quarter column = (π/4)·2; the faceted (inscribed) tool removes
    // slightly less, so v1 sits just above the analytic difference.
    let analytic1 = 32.0 - std::f64::consts::FRAC_PI_2;
    assert!(
        v1 >= analytic1 - 1e-9 && v1 <= analytic1 + 0.02 * std::f64::consts::FRAC_PI_2,
        "notched box volume {v1} vs analytic {analytic1}"
    );

    // Flush stacked union on the MIXED top cap, footprint far from the arc —
    // the R0021 auto-union shape (was the coplanar NotSupported wall).
    let boss = boxx(&mut a, (2.0, 3.0), (2.0, 3.0), (2.0, 3.0));
    let out = boolean_op(&mut a, notched, boss, BoolOp::Union)
        .unwrap_or_else(|e| panic!("flush union on mixed cap: {e:?}"));
    validate_solid(&a, out).expect("stacked result validates");
    let v2 = volume_of(&a, out);
    assert!(
        (v2 - v1 - 1.0).abs() < 1e-9,
        "flush union must add exactly the 1.0 boss: v1={v1} v2={v2}"
    );
}
