//! D1d section tests (`specs/drawings_and_mbd.md` §5.2 increment 4).
//!
//! Every number here is MEASURED from the run and pinned to the closed form it
//! must equal — a box's cross-section, `π·a·b` for an ellipse, `π·r²` for a
//! bore — rather than to whatever the implementation happened to print. A cap
//! bounded by lines, circular arcs and elliptical arcs is integrated in closed
//! form, so these are equalities at float precision and not bands.

use std::f64::consts::{FRAC_1_SQRT_2, PI};

use waffle_types::kernel::projection::{Curve2, KernelProjection, ProjectOpts, ViewFrame};
use waffle_types::kernel::{Kernel, KernelIntrospect, KernelSolidHandle};

use super::{loop_defects, LoopDefects};
use crate::projection::tests::{make_box, make_cylinder, rect_profile};
use crate::KernelV2Adapter;

/// A `w` × `d` × `h` box with its minimum corner at `origin`.
fn make_box_at(
    a: &mut KernelV2Adapter,
    origin: [f64; 3],
    w: f64,
    d: f64,
    h: f64,
) -> KernelSolidHandle {
    let (profile, positions) = rect_profile(w, d);
    let faces = a
        .make_faces_from_profiles(
            &[profile],
            origin,
            [0.0, 0.0, 1.0],
            [1.0, 0.0, 0.0],
            &positions,
        )
        .expect("rectangle stages");
    a.extrude_face(faces[0], [0.0, 0.0, 1.0], h).expect("box")
}

/// Every curve's endpoints, as a sorted, rounded point list — the loop's point
/// SET, which is what survives a change of traversal direction.
fn endpoint_set(curves: &[Curve2]) -> Vec<(i64, i64)> {
    let mut out: Vec<(i64, i64)> = curves
        .iter()
        .filter_map(|c| c.endpoints())
        .flat_map(|(a, b)| {
            [a, b].map(|p| ((p.x() * 1e9).round() as i64, (p.y() * 1e9).round() as i64))
        })
        .collect();
    out.sort_unstable();
    out
}

// ---------------------------------------------------------------------------
// §5.2 increment 4's own cases
// ---------------------------------------------------------------------------

/// A 10 × 6 × 4 box cut at mid height: ONE loop, four exact lines, and the
/// area is the box's cross-section exactly.
#[test]
fn a_box_cut_at_mid_height_caps_with_its_exact_cross_section() {
    let mut a = KernelV2Adapter::new();
    let h = make_box(&mut a, 10.0, 6.0, 4.0);
    let r = a
        .section_with_plane(&h, [0.0, 0.0, 2.0], [0.0, 0.0, 1.0])
        .expect("box section");

    assert_eq!(r.cap_loops.len(), 1, "a convex cap is one loop");
    let cap = &r.cap_loops[0];
    assert_eq!(cap.curves.len(), 4, "four walls, four cap edges");
    assert!(
        cap.curves.iter().all(|c| matches!(c, Curve2::Line { .. })),
        "a prismatic cap is exact lines: {:?}",
        cap.curves
    );
    assert!(cap.exact, "no sampled curve in a prismatic cap");
    assert!(r.exact());
    // 10 × 6, exactly — the cap is integrated in closed form from exact
    // lines, so this is not a tolerance band.
    assert_eq!(cap.signed_area, 60.0);
    assert_eq!(r.cap_area(), 60.0);
    assert!(
        cap.signed_area > 0.0,
        "an OUTER loop is positive in the cap frame"
    );
    assert_eq!(loop_defects(&cap.curves, 1e-9), LoopDefects::default());
    // The kept half-space is a real body: 10 × 6 × 2.
    let cut = r.cut_solid.expect("the cut keeps material");
    let vol = a.solid_volume(&cut).expect("volume");
    assert!(
        (vol - 120.0).abs() < 1e-9,
        "the kept half is 10 × 6 × 2 = 120, got {vol}"
    );
}

/// A cylinder of radius 2 cut at 45°: ONE loop of ellipse arcs whose
/// semi-axes are `r` and `r / cos θ`, with the area `π·a·b`.
#[test]
fn a_cylinder_cut_obliquely_caps_with_an_exact_ellipse() {
    let mut a = KernelV2Adapter::new();
    const R: f64 = 2.0;
    let h = make_cylinder(&mut a, (0.0, 0.0), R, 0.0, 10.0);
    // 45° about the x axis, through the mid height: cos θ = 1/√2.
    let r = a
        .section_with_plane(&h, [0.0, 0.0, 5.0], [0.0, FRAC_1_SQRT_2, FRAC_1_SQRT_2])
        .expect("oblique cylinder section");

    assert_eq!(r.cap_loops.len(), 1);
    let cap = &r.cap_loops[0];
    assert!(cap.exact, "an oblique plane × cylinder cap stays analytic");
    // The semi-axes: minor = the cylinder's radius, major = r / cos θ.
    let want_major = R / FRAC_1_SQRT_2;
    let mut sweep = 0.0;
    for c in &cap.curves {
        match *c {
            Curve2::Ellipse {
                major_radius,
                minor_radius,
                start_param,
                end_param,
                ..
            } => {
                assert!(
                    (minor_radius - R).abs() < 1e-12,
                    "the minor semi-axis IS the cylinder radius {R}, got {minor_radius}"
                );
                assert!(
                    (major_radius - want_major).abs() < 1e-12,
                    "the major semi-axis is r/cos θ = {want_major}, got {major_radius}"
                );
                sweep += end_param - start_param;
            }
            ref other => panic!("an oblique cylinder cap edge must be an ellipse arc: {other:?}"),
        }
    }
    // The arcs tile the whole ellipse — one full turn, no gap and no overlap.
    assert!(
        (sweep - std::f64::consts::TAU).abs() < 1e-9,
        "the cap's arcs must tile one full turn, got {sweep}"
    );
    // π·a·b, in closed form.
    let want_area = PI * want_major * R;
    assert!(
        (cap.signed_area - want_area).abs() < 1e-12,
        "an elliptical cap's area is π·a·b = {want_area}, got {}",
        cap.signed_area
    );
    assert_eq!(loop_defects(&cap.curves, 1e-9), LoopDefects::default());
}

/// A box with a through-hole, cut through the hole: TWO loops, the outer
/// positive and the bore negative, and the net area is `w·d − π·r²`.
#[test]
fn a_through_hole_cut_through_the_bore_caps_with_two_loops() {
    let mut a = KernelV2Adapter::new();
    const R: f64 = 2.0;
    let body = make_box(&mut a, 10.0, 10.0, 6.0);
    let bore = make_cylinder(&mut a, (5.0, 5.0), R, -1.0, 8.0);
    let holed = a.boolean_subtract(&body, &bore).expect("through hole");
    let r = a
        .section_with_plane(&holed, [0.0, 0.0, 3.0], [0.0, 0.0, 1.0])
        .expect("holed section");

    assert_eq!(r.cap_loops.len(), 2, "an outer loop and the bore");
    let outer: Vec<_> = r.cap_loops.iter().filter(|l| l.signed_area > 0.0).collect();
    let inner: Vec<_> = r.cap_loops.iter().filter(|l| l.signed_area < 0.0).collect();
    assert_eq!(outer.len(), 1, "one outer loop");
    assert_eq!(inner.len(), 1, "one hole, and its area is NEGATIVE");
    assert_eq!(outer[0].signed_area, 100.0, "10 × 10");
    assert!(
        (inner[0].signed_area + PI * R * R).abs() < 1e-12,
        "the bore's signed area is −π·r² = {}, got {}",
        -PI * R * R,
        inner[0].signed_area
    );
    assert!(
        (r.cap_area() - (100.0 - PI * R * R)).abs() < 1e-12,
        "the net cap area is w·d − π·r², got {}",
        r.cap_area()
    );
    assert!(r.exact());
    // The bore's cap edge is one full circle of the bore's own radius.
    assert_eq!(inner[0].curves.len(), 1);
    match inner[0].curves[0] {
        Curve2::Circle { radius, .. } => assert!((radius - R).abs() < 1e-12),
        ref other => panic!("a bore's cap edge is a circle, got {other:?}"),
    }
    for l in &r.cap_loops {
        assert_eq!(loop_defects(&l.curves, 1e-9), LoopDefects::default());
    }
}

/// A cut plane COPLANAR with a model face goes through the §4.5.5 Stage-0
/// overlay, and the cap it produces is attributed to the model rather than to
/// the cutting box — surfaced typed on the result, never a silently empty cap.
#[test]
fn a_cut_through_a_model_face_is_the_stage_0_path_and_says_so() {
    let mut a = KernelV2Adapter::new();
    // A 10 cube with its `x > 4, z > 5` corner removed, so a horizontal model
    // face sits at `z = 5` over `x ∈ [4, 10]`. The cutter overhangs on every
    // other side, so the SUBTRACT itself has no coplanar input pair — only the
    // section below does.
    let body = make_box(&mut a, 10.0, 10.0, 10.0);
    let cutter = make_box_at(&mut a, [4.0, -1.0, 5.0], 7.0, 12.0, 7.0);
    let step = a.boolean_subtract(&body, &cutter).expect("step");

    let r = a
        .section_with_plane(&step, [0.0, 0.0, 5.0], [0.0, 0.0, 1.0])
        .expect("a coplanar cut is a legitimate section");
    assert!(
        r.cap_shared_with_model,
        "the cap of a coplanar cut is the Stage-0 shared trimmed surface, and \
         the result must say so"
    );
    assert_eq!(r.cap_loops.len(), 1);
    // The kept half is the full 10 × 10 × 5 block, so the cap is the whole
    // 10 × 10 square — including the part that WAS a model face.
    assert_eq!(r.cap_loops[0].signed_area, 100.0);
    assert!(r.exact());
    let cut = r.cut_solid.expect("material survives");
    let vol = a.solid_volume(&cut).expect("volume");
    assert!((vol - 500.0).abs() < 1e-9, "10 × 10 × 5 = 500, got {vol}");
}

/// A plane that misses the solid is an EMPTY section, typed — not an error —
/// and which side it misses on decides whether a solid survives.
#[test]
fn a_plane_that_misses_the_solid_is_a_typed_empty_section() {
    let mut a = KernelV2Adapter::new();
    let h = make_box(&mut a, 10.0, 6.0, 4.0);

    // Entirely on the kept side: everything survives, and the handle that
    // comes back is the INPUT's (nothing was cut, so nothing was copied).
    let keep_all = a
        .section_with_plane(&h, [0.0, 0.0, 9.0], [0.0, 0.0, 1.0])
        .expect("a miss is not an error");
    assert!(keep_all.cap_loops.is_empty());
    assert_eq!(keep_all.cap_area(), 0.0);
    assert_eq!(
        keep_all.cut_solid.as_ref().map(|s| s.raw()),
        Some(h.raw()),
        "nothing was cut, so the cut solid IS the input body"
    );

    // Entirely on the discarded side: no material, and no handle to name it
    // (kernel-v2 has no empty solid).
    let keep_none = a
        .section_with_plane(&h, [0.0, 0.0, -9.0], [0.0, 0.0, 1.0])
        .expect("a miss is not an error");
    assert!(keep_none.cap_loops.is_empty());
    assert!(
        keep_none.cut_solid.is_none(),
        "a cut that removes everything has no solid to name"
    );

    // A plane exactly ON the top face is a tangency, not a section: the
    // conservative box's `dmax == 0` settles it without handing the boolean a
    // grazing operand (deviation N69's class).
    let tangent = a
        .section_with_plane(&h, [0.0, 0.0, 4.0], [0.0, 0.0, 1.0])
        .expect("a tangent plane is not an error");
    assert!(tangent.cap_loops.is_empty());
    assert_eq!(
        tangent.cut_solid.as_ref().map(|s| s.raw()),
        Some(h.raw()),
        "a tangent plane cuts nothing"
    );
}

/// Flipping the normal keeps the same cap — the same loops, traversed the
/// other way, in the mirrored frame the flip implies.
#[test]
fn the_cap_is_symmetric_in_the_sign_of_the_normal() {
    let mut a = KernelV2Adapter::new();
    let h = make_box(&mut a, 10.0, 6.0, 4.0);
    let up = a
        .section_with_plane(&h, [0.0, 0.0, 2.0], [0.0, 0.0, 1.0])
        .expect("keep z ≤ 2");
    let down = a
        .section_with_plane(&h, [0.0, 0.0, 2.0], [0.0, 0.0, -1.0])
        .expect("keep z ≥ 2");

    // The two frames share `v` and mirror `u`: the line of sight is negated,
    // and the up vector `looking_along` picks for a `z`-axis view is the same.
    assert_eq!(up.plane_basis.u, [1.0, 0.0, 0.0]);
    assert_eq!(down.plane_basis.u, [-1.0, 0.0, 0.0]);
    assert_eq!(up.plane_basis.v, down.plane_basis.v);

    assert_eq!(up.cap_loops.len(), down.cap_loops.len());
    // Same area, same sign: the outward normal flips WITH the frame, so the
    // outer loop stays counter-clockwise in its own `(u, v)`.
    assert_eq!(up.cap_area(), down.cap_area());
    assert_eq!(up.cap_area(), 60.0);
    // And the same point set, through the `u → −u` mirror the frames differ
    // by. (Only the point SET: each `Curve2` conic is normalized
    // counter-clockwise, so the traversal direction is not comparable.)
    let mirrored: Vec<(i64, i64)> = {
        let mut m: Vec<(i64, i64)> = endpoint_set(&down.cap_loops[0].curves)
            .into_iter()
            .map(|(x, y)| (-x, y))
            .collect();
        m.sort_unstable();
        m
    };
    assert_eq!(endpoint_set(&up.cap_loops[0].curves), mirrored);
    // Both halves are real bodies and together they are the whole box.
    let a_vol = a
        .solid_volume(up.cut_solid.as_ref().expect("upper"))
        .expect("volume");
    let b_vol = a
        .solid_volume(down.cut_solid.as_ref().expect("lower"))
        .expect("volume");
    assert!(
        (a_vol + b_vol - 240.0).abs() < 1e-9,
        "the two halves must sum to the box's 10 × 6 × 4 = 240, got {a_vol} + {b_vol}"
    );
}

// ---------------------------------------------------------------------------
// the contract's edges
// ---------------------------------------------------------------------------

/// A degenerate plane is refused by name, before any arena work.
#[test]
fn a_degenerate_cut_plane_is_refused() {
    let mut a = KernelV2Adapter::new();
    let h = make_box(&mut a, 10.0, 6.0, 4.0);
    for (o, n, what) in [
        ([0.0, 0.0, 2.0], [0.0, 0.0, 0.0], "zero normal"),
        ([0.0, 0.0, 2.0], [f64::NAN, 0.0, 1.0], "non-finite normal"),
        (
            [0.0, f64::INFINITY, 2.0],
            [0.0, 0.0, 1.0],
            "non-finite origin",
        ),
    ] {
        let err = a
            .section_with_plane(&h, o, n)
            .expect_err("a degenerate plane has no section");
        assert!(
            format!("{err}").contains("SectionDegeneratePlane"),
            "{what} must be refused by name, got {err}"
        );
    }
}

/// The section leaves the LIVE arena's view of the original body untouched:
/// the box is still there, still projects to the same wireframe, and a second
/// section works.
///
/// This is the scratch arena's whole point. A boolean run in the live arena
/// would append its own entities and journal entries, and the pid hygiene the
/// module docs describe is what keeps the cap's lineage attribution sound
/// across the two arenas.
#[test]
fn a_section_does_not_disturb_the_body_it_sectioned() {
    let mut a = KernelV2Adapter::new();
    let h = make_box(&mut a, 10.0, 6.0, 4.0);
    let before = a
        .project(&h, &ViewFrame::FRONT, &ProjectOpts::default())
        .expect("before");
    let first = a
        .section_with_plane(&h, [0.0, 0.0, 2.0], [0.0, 0.0, 1.0])
        .expect("first section");
    let second = a
        .section_with_plane(&h, [5.0, 0.0, 0.0], [1.0, 0.0, 0.0])
        .expect("a second section of the same body");
    let after = a
        .project(&h, &ViewFrame::FRONT, &ProjectOpts::default())
        .expect("after");

    assert_eq!(
        before.curves, after.curves,
        "the sectioned body's own projection must be unchanged"
    );
    assert_eq!(first.cap_area(), 60.0, "10 × 6");
    assert_eq!(second.cap_area(), 24.0, "6 × 4");
    assert_eq!(
        a.solid_volume(&h).expect("volume"),
        240.0,
        "and the body itself is untouched"
    );
    // The cut solid is projectable — which is the whole point of handing it
    // back (§5.2: "the caller projects the cut solid with D1a–c").
    let view = a
        .project(
            first.cut_solid.as_ref().expect("cut body"),
            &ViewFrame::FRONT,
            &ProjectOpts::default(),
        )
        .expect("the cut solid projects");
    assert!(!view.curves.is_empty());
}

/// `loop_defects` finds what it claims to: an open chain and a crossing.
#[test]
fn loop_defects_reports_an_open_chain_and_a_crossing() {
    use cad_primitives::Point2;
    let p = |x: f64, y: f64| Point2::new(x, y);

    // A closed triangle: no defects.
    let tri = vec![
        Curve2::Line {
            start: p(0.0, 0.0),
            end: p(4.0, 0.0),
        },
        Curve2::Line {
            start: p(4.0, 0.0),
            end: p(0.0, 3.0),
        },
        Curve2::Line {
            start: p(0.0, 3.0),
            end: p(0.0, 0.0),
        },
    ];
    assert_eq!(loop_defects(&tri, 1e-9), LoopDefects::default());

    // Drop the closing edge: two ends with no partner.
    assert_eq!(loop_defects(&tri[..2], 1e-9).unmatched_ends, 2);

    // A bow tie: the two diagonals cross away from any shared endpoint.
    let bow = vec![
        Curve2::Line {
            start: p(0.0, 0.0),
            end: p(4.0, 4.0),
        },
        Curve2::Line {
            start: p(4.0, 4.0),
            end: p(4.0, 0.0),
        },
        Curve2::Line {
            start: p(4.0, 0.0),
            end: p(0.0, 4.0),
        },
        Curve2::Line {
            start: p(0.0, 4.0),
            end: p(0.0, 0.0),
        },
    ];
    let d = loop_defects(&bow, 1e-9);
    assert_eq!(d.unmatched_ends, 0, "the bow tie is closed");
    assert_eq!(d.self_crossings, 1, "and it crosses itself exactly once");

    // A full circle is one closed curve, not an open chain.
    assert_eq!(
        loop_defects(
            &[Curve2::Circle {
                center: p(0.0, 0.0),
                radius: 1.0,
                start_angle: 0.0,
                end_angle: std::f64::consts::TAU,
            }],
            1e-9
        ),
        LoopDefects::default()
    );
}
