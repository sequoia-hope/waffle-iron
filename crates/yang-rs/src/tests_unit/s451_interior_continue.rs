//! §4.5.1 for an interior point of a cone ∩ plane section (ledger N90)
//! (`refs/text/yang2025_hybrid_boolean.txt:652-686`).
//!
//! The geometry is P0005's v20, reproduced exactly: a 45° cone (apex
//! (200, −300, 400), axis −z) cut by two facets of a gear flank, B:131 and
//! its neighbour across edge 1077. The mesh intersection point lies on
//! B:131, 1.0 off the cone and 0.30 from the crease; its §4.3.2 step lands
//! 0.31 past the crease. Its two neighbours on the curve are converged: v18
//! on cone ∩ B:131 and v27 exactly at the triple point where the curve
//! crosses the crease. So the true curve between them runs on B:131, and
//! §4.5.1 replaces v20 by the midpoint of v18 and v27 before stepping
//! (`:676-680`). The midpoint's step stays on B:131. Continuing v20's OWN
//! step onto the neighbour put it past v27 — a reversal, not Fig. 12.
//!
//! The truncate-and-continue primitive is still pinned on this geometry:
//! it is what a bounded midpoint whose step leaves the face takes (`:680-686`).

#[allow(unused_imports)]
use super::*;

use crate::stage4_boundary_curve::{crease_divider, FaceCrease};
use crate::stage4_correct::{cone_section_continue_across_crease, cone_section_step_leaves_face};
use crate::stage4_relocate::{cone_plane_residual, project_onto_cone_section};
use crate::{BRepEdge, BRepVertex, Curve, Surface, Vector3};
use cad_primitives::Point3;

fn cone() -> (Point3, Vector3, f64) {
    (
        Point3::new(200.0, -300.0, 400.0),
        Vector3::new(-0.0, -0.0, -1.0),
        std::f64::consts::FRAC_PI_4,
    )
}
fn own() -> (Vector3, f64) {
    (
        Vector3::new(
            -0.464_017_036_622_307_14,
            0.885_758_329_768_470_1,
            0.010_971_370_471_492_763,
        ),
        550.092_190_027_122_2,
    )
}
fn other() -> (Vector3, f64) {
    (
        Vector3::new(
            -0.404_837_009_856_533_65,
            0.914_380_511_182_462_9,
            0.003_908_480_538_355_602,
        ),
        525.595_293_125_683_6,
    )
}
fn plane((n, d): (Vector3, f64)) -> Surface {
    Surface::Plane { normal: n, d }
}
fn pre() -> Point3 {
    Point3::new(572.214_000_340, -321.597_316_164, 25.735_740_889)
}
fn signed(x: Point3, (n, d): (Vector3, f64)) -> f64 {
    let (x, n) = (x.as_array(), n.as_array());
    x[0] * n[0] + x[1] * n[1] + x[2] * n[2] + d
}

/// The crease: the two facet planes' intersection line near v20, as a long
/// segment (the facet's own edge spans the flank height).
fn crease(neighbour: Surface) -> FaceCrease {
    let ((n1, d1), (n2, d2)) = (own(), other());
    let (a, b) = (n1.as_array(), n2.as_array());
    let dir = [
        a[1] * b[2] - a[2] * b[1],
        a[2] * b[0] - a[0] * b[2],
        a[0] * b[1] - a[1] * b[0],
    ];
    // Closest point of the line to v20: x = p − Nᵀ G⁻¹ r.
    let x = pre().as_array();
    let r = [signed(pre(), own()), signed(pre(), other())];
    let (g11, g12, g22) = (
        a.iter().zip(&a).map(|(u, v)| u * v).sum::<f64>(),
        a.iter().zip(&b).map(|(u, v)| u * v).sum::<f64>(),
        b.iter().zip(&b).map(|(u, v)| u * v).sum::<f64>(),
    );
    let det = g11 * g22 - g12 * g12;
    let (l1, l2) = (
        (g22 * r[0] - g12 * r[1]) / det,
        (g11 * r[1] - g12 * r[0]) / det,
    );
    let c = [0, 1, 2].map(|i| x[i] - l1 * a[i] - l2 * b[i]);
    let _ = (d1, d2);
    let dl = (dir[0] * dir[0] + dir[1] * dir[1] + dir[2] * dir[2]).sqrt();
    let end = |s: f64| BRepVertex {
        point: Point3::new(
            c[0] + s * dir[0] / dl,
            c[1] + s * dir[1] / dl,
            c[2] + s * dir[2] / dl,
        ),
    };
    let verts = vec![end(-50.0), end(50.0)];
    let e = BRepEdge {
        start: 0,
        end: 1,
        curve: Curve::LineSegment,
    };
    let (divider, extent) =
        crease_divider(&e, &verts, plane(own()), Some(neighbour)).expect("line crease");
    FaceCrease {
        divider,
        s_own: plane(own()),
        s_other: Some(neighbour),
        extent,
        face_other: None,
        edge: 1077,
    }
}

/// The §4.3.2 step on the own facet leaves it across the crease; the
/// continuation lands on cone ∩ neighbour, on the neighbour's side, near the
/// point — not back on the own facet's curve.
#[test]
fn p0005_v20_continues_onto_the_neighbouring_facet() {
    let (apex, axis, ha) = cone();
    let (n, d) = own();
    let proj = project_onto_cone_section(pre(), apex, axis, ha, n, d).expect("own step");
    assert!(
        signed(proj, other()) * signed(pre(), other()) < 0.0,
        "fixture: the own-facet step crosses the neighbour's plane"
    );
    let (q, crease) =
        cone_section_continue_across_crease(pre(), proj, cone(), own(), &[crease(plane(other()))])
            .expect("a plane-to-plane crease crossing continues");
    assert_eq!(crease.edge, 1077);
    let (n1, d1) = other();
    let res = cone_plane_residual(q, apex, axis, ha, n1, d1);
    assert!(
        res < 1e-9,
        "continued point on cone ∩ neighbour: residual {res:.3e}"
    );
    let off_own = signed(q, own()).abs();
    assert!(
        off_own > 1e-6,
        "continued point left the own facet's plane by {off_own:.3e}"
    );
    let dq = {
        let (x, y) = (q.as_array(), pre().as_array());
        ((x[0] - y[0]).powi(2) + (x[1] - y[1]).powi(2) + (x[2] - y[2]).powi(2)).sqrt()
    };
    // The neighbour's closest point is 1.070 away; the continuation from the
    // truncation point lands within the same local neighbourhood.
    assert!(dq < 1.5, "continued point {dq:.4} from the mesh point");
}

/// A crease whose far side is not a plane cannot continue as a cone section:
/// the helper declines and the certificate stays the loud reader.
#[test]
fn a_non_plane_neighbour_declines() {
    let (apex, axis, ha) = cone();
    let (n, d) = own();
    let proj = project_onto_cone_section(pre(), apex, axis, ha, n, d).expect("own step");
    let cyl = Surface::Cylinder {
        axis_point: Point3::new(600.0, -300.0, 0.0),
        axis_dir: Vector3::new(0.0, 0.0, 1.0),
        radius: 30.0,
    };
    assert!(
        cone_section_continue_across_crease(pre(), proj, cone(), own(), &[crease(cyl)]).is_none()
    );
}

/// No crossing, no continuation: a step inside the face returns `None`.
#[test]
fn a_step_inside_the_face_is_left_alone() {
    let (apex, axis, ha) = cone();
    let (n, d) = own();
    let proj = project_onto_cone_section(pre(), apex, axis, ha, n, d).expect("own step");
    // Truncate the step to its first tenth: still inside the own facet.
    let (a, b) = (pre().as_array(), proj.as_array());
    let short = Point3::new(
        a[0] + 0.1 * (b[0] - a[0]),
        a[1] + 0.1 * (b[1] - a[1]),
        a[2] + 0.1 * (b[2] - a[2]),
    );
    assert!(cone_section_continue_across_crease(
        pre(),
        short,
        cone(),
        own(),
        &[crease(plane(other()))]
    )
    .is_none());
}

fn v18() -> Point3 {
    Point3::new(
        567.858_859_563_539_5,
        -323.948_512_570_934_04,
        31.362_411_284_813_387,
    )
}
fn v27() -> Point3 {
    Point3::new(
        572.587_679_796_770_5,
        -321.414_709_237_246_2,
        26.797_415_726_985_307,
    )
}

/// The fixture: v18 and v27 are converged on cone ∩ B:131, v27 also on the
/// neighbour (the triple point), and v20's own step leaves B:131.
#[test]
fn p0005_v20_bounds_are_converged_and_its_own_step_leaves_the_facet() {
    let (apex, axis, ha) = cone();
    let (n, d) = own();
    for (name, x) in [("v18", v18()), ("v27", v27())] {
        let res = cone_plane_residual(x, apex, axis, ha, n, d);
        assert!(res < 1e-9, "{name} converged on cone ∩ B:131: {res:.3e}");
    }
    assert!(signed(v27(), other()).abs() < 1e-9, "v27 is on the crease");
    let proj = project_onto_cone_section(pre(), apex, axis, ha, n, d).expect("own step");
    assert!(cone_section_step_leaves_face(
        pre(),
        proj,
        own(),
        &[crease(plane(other()))]
    ));
}

/// §4.5.1: the midpoint of the bounds steps onto cone ∩ B:131 without
/// leaving it, and lands between the bounds along the curve.
#[test]
fn p0005_v20_from_the_midpoint_of_its_bounds_stays_on_its_own_facet() {
    let (apex, axis, ha) = cone();
    let (n, d) = own();
    let (a, b) = (v18().as_array(), v27().as_array());
    let m = Point3::new(
        0.5 * (a[0] + b[0]),
        0.5 * (a[1] + b[1]),
        0.5 * (a[2] + b[2]),
    );
    let proj = project_onto_cone_section(m, apex, axis, ha, n, d).expect("midpoint step");
    assert!(!cone_section_step_leaves_face(
        m,
        proj,
        own(),
        &[crease(plane(other()))]
    ));
    let res = cone_plane_residual(proj, apex, axis, ha, n, d);
    assert!(res < 1e-9, "on cone ∩ B:131: {res:.3e}");
    let x = proj.as_array();
    let chord = [b[0] - a[0], b[1] - a[1], b[2] - a[2]];
    let len2 = chord.iter().map(|c| c * c).sum::<f64>();
    let s = ((x[0] - a[0]) * chord[0] + (x[1] - a[1]) * chord[1] + (x[2] - a[2]) * chord[2]) / len2;
    assert!(
        s > 0.0 && s < 1.0,
        "between v18 and v27 along the chord: s = {s}"
    );
}
