//! Stage-4 §4.4.1 boundary-curve relocation, inc-1 primitive
//! (spec `specs/yang_s4_boundary_curve_relocation.md` §6).
//!
//! The five oracles the spec names, built on the MEASURED geometry of the two
//! known instances rather than on invented numbers: the n2 I1 fixture's rim
//! (R = 2.1339062731488812e-4, base N = 37 ⇒ a 360/37 = 9.7297° span) and the
//! R0063 sub-class (a vertex perturbed OFF the chord, not a pristine chord
//! point).

use super::*;
use crate::geom::Curve;
use crate::stage4_boundary_curve::{
    boundary_relocation_for_vertex, plan_boundary_relocations, project_onto_curve,
};
use cad_primitives::{Point3, Vector3, TAU_WORK};

/// R0072's cylinder radius — the real scale the defect was measured at.
const R: f64 = 2.1339062731488812e-4;
/// 360/37: the boosted-rim span whose chord carries the defective vertex.
const SPAN_DEG: f64 = 360.0 / 37.0;

fn unit_z_circle() -> Curve {
    Curve::Circle {
        center: Point3::new(0.0, 0.0, 0.0),
        normal: Vector3::new(0.0, 0.0, 1.0),
        radius: R,
    }
}

fn on_circle(deg: f64) -> Point3 {
    let t = deg.to_radians();
    Point3::new(R * t.cos(), R * t.sin(), 0.0)
}

fn dist(a: Point3, b: Point3) -> f64 {
    let (x, y) = (a.as_array(), b.as_array());
    ((x[0] - y[0]).powi(2) + (x[1] - y[1]).powi(2) + (x[2] - y[2]).powi(2)).sqrt()
}

/// The Stage-1 chord bound for this rim: the max sagitta of a `SPAN_DEG` chord.
/// This is the bound Stage 1 already guarantees — NOT a widened tolerance.
fn chord_bound() -> f64 {
    R * (1.0 - (SPAN_DEG / 2.0).to_radians().cos())
}

/// A point at parameter `t` along the chord joining two on-circle rim samples —
/// the pristine artifact (n2 `v6`, measured EXACTLY on its chord, perp 7.14e-22).
fn chord_point(deg_a: f64, deg_b: f64, t: f64) -> Point3 {
    let (a, b) = (on_circle(deg_a).as_array(), on_circle(deg_b).as_array());
    Point3::new(
        a[0] + t * (b[0] - a[0]),
        a[1] + t * (b[1] - a[1]),
        a[2] + t * (b[2] - a[2]),
    )
}

// -------------------------------------------------------------------------
// (i) a pristine chord-position vertex relocates EXACTLY onto the circle
// -------------------------------------------------------------------------

#[test]
fn s4bc_i_chord_vertex_relocates_onto_the_circle() {
    let curve = unit_z_circle();
    let p = chord_point(0.0, SPAN_DEG, 0.5); // the chord midpoint: max sagitta
    let resid = (p.as_array()[0].hypot(p.as_array()[1]) - R).abs();
    assert!(
        resid > 1e-9 * (1.0 + R),
        "fixture must actually be off-band before the fix (resid {resid:.3e})"
    );

    let q = boundary_relocation_for_vertex(7, p, &curve, chord_bound())
        .expect("within the chord bound ⇒ never a STOP")
        .expect("an off-curve chord vertex must be relocated");

    let qa = q.as_array();
    let radial = qa[0].hypot(qa[1]);
    assert!(
        (radial - R).abs() <= TAU_WORK * (1.0 + R),
        "relocated vertex must land ON the analytic circle, got residual {:.3e}",
        (radial - R).abs()
    );
    // §4.4.1 relocation moves ONLY off the chord — it must not slide the vertex
    // around the rim, so its azimuth is preserved.
    assert!(
        (qa[1].atan2(qa[0]) - p.as_array()[1].atan2(p.as_array()[0])).abs() <= 1e-15,
        "projection must preserve azimuth (radial projection, not a re-sample)"
    );
}

// -------------------------------------------------------------------------
// (ii) the R0063 sub-class: perturbed OFF the chord, still relocates
// -------------------------------------------------------------------------

#[test]
fn s4bc_ii_vertex_perturbed_off_the_chord_still_relocates() {
    let curve = unit_z_circle();
    // R0063's vertex sits 0.77 % of the sagitta off its chord — the fix must
    // project from wherever the vertex is, not assume a pristine chord point.
    let base = chord_point(0.0, SPAN_DEG, 0.475239168);
    let nudge = 0.0077 * chord_bound();
    let p = Point3::new(base.as_array()[0], base.as_array()[1] + nudge, 0.0);

    let q = boundary_relocation_for_vertex(9, p, &curve, chord_bound())
        .expect("still within the chord bound")
        .expect("a perturbed chord vertex must also be relocated");
    let qa = q.as_array();
    assert!(
        (qa[0].hypot(qa[1]) - R).abs() <= TAU_WORK * (1.0 + R),
        "perturbed vertex must also land exactly on the circle"
    );
}

// -------------------------------------------------------------------------
// (iii) a junction vertex is NOT moved
// -------------------------------------------------------------------------

#[test]
fn s4bc_iii_cross_curve_junction_vertex_is_never_moved() {
    let curve = unit_z_circle();
    let mut mesh = Mesh::empty();
    // v0/v1 exact rim samples; v2 the chord artifact; v3 an A×B junction that
    // also happens to sit off the circle — it must survive untouched, because
    // it is required to lie on BOTH curves (I1's `v5` is the real instance).
    mesh.verts.push(on_circle(0.0));
    mesh.verts.push(on_circle(SPAN_DEG));
    mesh.verts.push(chord_point(0.0, SPAN_DEG, 0.5));
    mesh.verts.push(chord_point(0.0, SPAN_DEG, 0.25));

    let mut rim: std::collections::BTreeMap<(u32, u32), Curve> = Default::default();
    rim.insert((0, 2), curve.clone());
    rim.insert((2, 1), curve.clone());
    rim.insert((0, 3), curve);

    let mut excluded: std::collections::BTreeSet<u32> = Default::default();
    excluded.insert(3);

    let moves = plan_boundary_relocations(&mesh, &rim, &excluded, chord_bound())
        .expect("all displacements within bound");
    let moved: Vec<u32> = moves.iter().map(|(v, _)| *v).collect();
    assert_eq!(moved, vec![2], "only the unclaimed chord vertex may move");
}

// -------------------------------------------------------------------------
// (iv) beyond the chord bound ⇒ LOUD STOP, never a snap
// -------------------------------------------------------------------------

#[test]
fn s4bc_iv_beyond_chord_bound_is_a_loud_stop() {
    let curve = unit_z_circle();
    // Well inside the rim — a real defect of a different class, which this pass
    // must refuse rather than quietly drag onto the circle.
    let p = Point3::new(R * 0.5, 0.0, 0.0);
    let err = boundary_relocation_for_vertex(11, p, &curve, chord_bound())
        .expect_err("a displacement beyond the chord bound must STOP");
    assert!(
        matches!(
            err,
            YangError::Stage4RegionInvalid {
                vertex: 11,
                reason: Stage4InvalidReason::LocalRefinementRequired
            }
        ),
        "expected the loud Stage-4 STOP, got {err:?}"
    );
}

// -------------------------------------------------------------------------
// (v) an already-exact vertex is a bit-exact no-op (byte-identical corpus)
// -------------------------------------------------------------------------

#[test]
fn s4bc_v_on_curve_vertex_is_a_no_op() {
    let curve = unit_z_circle();
    for deg in [0.0, SPAN_DEG, 90.0, -137.5] {
        let p = on_circle(deg);
        let got = boundary_relocation_for_vertex(3, p, &curve, chord_bound())
            .expect("an on-curve vertex is never a STOP");
        assert!(
            got.is_none(),
            "a vertex already on the curve must not be rewritten (deg {deg})"
        );
    }
}

/// The projection itself is exact and idempotent: projecting a projected point
/// returns it unchanged, so a second Stage-4 pass cannot drift the mesh.
#[test]
fn s4bc_projection_is_idempotent() {
    let curve = unit_z_circle();
    let p = chord_point(0.0, SPAN_DEG, 0.31);
    let q = project_onto_curve(p, &curve).expect("circle projection exists");
    let q2 = project_onto_curve(q, &curve).expect("re-projection exists");
    assert!(
        dist(q, q2) <= TAU_WORK * (1.0 + R),
        "projection must be idempotent, drifted {:.3e}",
        dist(q, q2)
    );
}

/// A point ON the axis has no unique closest point — the pass must skip it,
/// not guess an azimuth.
#[test]
fn s4bc_axis_point_is_skipped_not_guessed() {
    let curve = unit_z_circle();
    assert!(project_onto_curve(Point3::new(0.0, 0.0, 0.0), &curve).is_none());
    assert!(project_onto_curve(Point3::new(0.0, 0.0, 5.0), &curve).is_none());
}

/// Out-of-scope curve kinds are skipped, never snapped (spec §4 step 1).
#[test]
fn s4bc_non_circle_curve_is_skipped() {
    let p = Point3::new(1.0, 1.0, 1.0);
    assert!(project_onto_curve(p, &Curve::LineSegment).is_none());
}
