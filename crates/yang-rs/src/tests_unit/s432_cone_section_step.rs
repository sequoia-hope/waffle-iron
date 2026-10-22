//! N97 (R0049): the cone ∩ plane relocation is Yang §4.3.2's geometric step,
//! the closest point of the two tangent planes' intersection line
//! (`refs/text/yang2025_hybrid_boolean.txt:493-502`), not an azimuth-
//! preserving slide along the cone generator.
//!
//! The geometry is R0049's, reproduced exactly: a revolve's 0.015-rad cone
//! (A:5) cut by two facets of an extruded gear flank (B:134 and its
//! neighbour across edge 1651, 2.9° apart). The facet planes nearly contain
//! the cone's generators (`|n·g| ≈ 6e-3`). The generator slide amplified a
//! 1.4e-6 radial residual into a 1.6e-4 move along the hyperbola, past the
//! {cone, facet, facet} corner 8.4e-6 away, which made the §4.5 domain
//! certificate fire and §4.4.1 refuse the facet patch. The exact foot point
//! lies 1.8e-6 off, inside B:134. The mesh vertices below are the three
//! measured pre-relocation positions (one per §4.5.2 pass).

#[allow(unused_imports)]
use super::*;

use crate::stage4_relocate::{cone_plane_residual, project_onto_cone_section};

fn apex() -> Point3 {
    Point3::new(
        0.056_628_494_876_270_47,
        -0.002_949_533_774_593_982_5,
        0.118_124_151_386_932_33,
    )
}
fn axis() -> Vector3 {
    Vector3::new(-0.419_529_321_067_308_6, 0.0, -0.907_741_785_291_832_4)
}
const HALF_ANGLE: f64 = 0.015_115_691_352_450_817;
/// B:134, the facet the vertices belong to.
fn own() -> (Vector3, f64) {
    (
        Vector3::new(
            0.490_545_918_044_316_85,
            -0.840_082_833_466_794_4,
            -0.231_571_879_131_428_1,
        ),
        -0.002_186_321_417_483_872,
    )
}
/// The facet across edge 1651.
fn across() -> (Vector3, f64) {
    (
        Vector3::new(
            0.488_538_854_077_312_07,
            -0.853_188_566_711_179_4,
            -0.182_754_090_761_739_46,
        ),
        -0.002_099_250_637_292_554,
    )
}
fn signed(p: Point3, (n, d): (Vector3, f64)) -> f64 {
    let (a, n) = (p.as_array(), n.as_array());
    a[0] * n[0] + a[1] * n[1] + a[2] * n[2] + d
}
fn dist(a: Point3, b: Point3) -> f64 {
    let (a, b) = (a.as_array(), b.as_array());
    ((a[0] - b[0]).powi(2) + (a[1] - b[1]).powi(2) + (a[2] - b[2]).powi(2)).sqrt()
}
fn measured() -> [Point3; 3] {
    [
        Point3::new(0.001_739_364, -0.001_036_182, -0.001_997_674),
        Point3::new(0.001_737_759, -0.001_037_049, -0.001_997_926),
        Point3::new(0.001_738_700, -0.001_036_541, -0.001_997_778),
    ]
}

/// The fixture is the measured regime: the facet nearly contains the cone's
/// generators, so a generator slide would amplify by ≈ 1/|n·g| ≫ 100.
#[test]
fn fixture_is_the_generator_parallel_regime() {
    let (n, _) = own();
    let (a, n) = (normalize3(axis().as_array()), n.as_array());
    let p = measured()[1].as_array();
    let ap = apex().as_array();
    let w = [p[0] - ap[0], p[1] - ap[1], p[2] - ap[2]];
    let h = w[0] * a[0] + w[1] * a[1] + w[2] * a[2];
    let r = [w[0] - h * a[0], w[1] - h * a[1], w[2] - h * a[2]];
    let rl = (r[0] * r[0] + r[1] * r[1] + r[2] * r[2]).sqrt();
    let (c, s) = (HALF_ANGLE.cos(), HALF_ANGLE.sin());
    let g = [
        c * a[0] + s * r[0] / rl,
        c * a[1] + s * r[1] / rl,
        c * a[2] + s * r[2] / rl,
    ];
    let n_dot_g = n[0] * g[0] + n[1] * g[1] + n[2] * g[2];
    assert!(n_dot_g.abs() < 1e-2, "n·g = {n_dot_g:.3e}");
}

/// The relocation lands on both surfaces, moves by a small multiple of the
/// vertex's own residual (not along the curve), and stays on B:134's side of
/// the crease, as the exact foot point does.
#[test]
fn r0049_vertices_relocate_by_their_residual_and_stay_in_domain() {
    let (n, d) = own();
    for p in measured() {
        let res = cone_plane_residual(p, apex(), axis(), HALF_ANGLE, n, d);
        let q = project_onto_cone_section(p, apex(), axis(), HALF_ANGLE, n, d)
            .expect("transversal cone ∩ facet: the geometric step converges");
        assert!(
            cone_plane_residual(q, apex(), axis(), HALF_ANGLE, n, d) < 1e-12,
            "relocated point is on both surfaces"
        );
        let moved = dist(p, q);
        // The cone and the facet meet at ≈ 48°: the foot point is within
        // 1/sin(48°) ≈ 1.35 × residual. The generator slide moved 9–120 ×.
        assert!(
            moved < 2.0 * res,
            "moved {moved:.3e} for residual {res:.3e}"
        );
        assert!(
            signed(q, across()) * signed(p, across()) > 0.0,
            "relocation crossed crease 1651: {:.3e} -> {:.3e}",
            signed(p, across()),
            signed(q, across())
        );
    }
}

/// The on-axis refusal stays loud.
#[test]
fn on_axis_point_is_a_loud_stop() {
    let (n, d) = own();
    let a = normalize3(axis().as_array());
    let ap = apex().as_array();
    let p = Point3::new(ap[0] + 0.1 * a[0], ap[1] + 0.1 * a[1], ap[2] + 0.1 * a[2]);
    assert_eq!(
        project_onto_cone_section(p, apex(), axis(), HALF_ANGLE, n, d),
        Err(Stage4InvalidReason::OnAxis)
    );
}
