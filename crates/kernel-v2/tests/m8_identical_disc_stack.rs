//! M8 identical disc pair — the flush SAME-RADIUS cylinder stack through the
//! real extrude path (the C0044 shape: `extrude(circle)` + `extrude(circle)`
//! on the first top cap + an axial bore through both).
//!
//! Yang §4.5.5: the two caps at the shared plane are the SAME disc, so the
//! overlap is the whole disc and both solids must carry one identical mesh on
//! it with identical sampling points on its boundary — here BOTH rims. Stage 1
//! samples each cap with its own in-plane basis and seam phase, so the two
//! rims differed by ulps; handed to the arrangement as two ulp-different
//! N-gons, a stray cap fan triangle survived and the union reassembled
//! non-2-manifold (C0044 ERROR, 2026-09-12). The Stage-0 identical-disc branch
//! (`stage0::disc_pair::build_identical_discs`) now emits one shared fan over
//! the merged rim ring to both caps and the merged ring to both laterals.

use cad_primitives::{BoolOp, Point2, Point3, Vector3};
use kernel_v2::{boolean_op, extrude, tessellate, validate_solid, BrepArena, Profile, RenderMesh};

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

/// Two r=1, h=1 cylinders stacked cap-to-cap (union), then an r=0.3 bore
/// through both (subtract) — C0044's three ops. The union must be one valid
/// solid of twice the cylinder's volume; the bore must remove its own volume
/// and leave a valid tube.
#[test]
fn flush_same_radius_cylinder_stack_then_bore() {
    let mut a = BrepArena::new();
    let lower = cyl(&mut a, 0.0, 0.0, 1.0, (0.0, 1.0));
    let v_cyl = volume_of(&a, lower);
    let upper = cyl(&mut a, 0.0, 0.0, 1.0, (1.0, 2.0));
    let stack = boolean_op(&mut a, lower, upper, BoolOp::Union)
        .unwrap_or_else(|e| panic!("flush same-radius cylinder stack union: {e:?}"));
    validate_solid(&a, stack).expect("stacked cylinder validates");
    let v_stack = volume_of(&a, stack);
    // Same footprint, same render density: the stack is exactly two cylinders.
    assert!(
        (v_stack - 2.0 * v_cyl).abs() < 1e-6 * v_cyl,
        "stack volume {v_stack} vs 2 × cylinder {v_cyl}"
    );

    let bore = cyl(&mut a, 0.0, 0.0, 0.3, (-2.0, 3.0));
    let tube = boolean_op(&mut a, stack, bore, BoolOp::Subtract)
        .unwrap_or_else(|e| panic!("axial bore through the stack: {e:?}"));
    validate_solid(&a, tube).expect("bored stack validates");
    let v_tube = volume_of(&a, tube);
    // The bore removes π·0.3²·2 = 0.565 of the 2π stack (the corpus meta's
    // expected 5.7177); the faceted (inscribed) render sits within its chord
    // band of the analytic figure.
    let analytic = std::f64::consts::PI * 2.0 - std::f64::consts::PI * 0.09 * 2.0;
    assert!(
        (v_tube - analytic).abs() / analytic < 0.05,
        "tube volume {v_tube} vs analytic {analytic}"
    );
    assert!(v_tube < v_stack, "the bore must remove material");
}
