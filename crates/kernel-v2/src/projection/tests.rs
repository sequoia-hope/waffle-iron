//! D1a projection tests, including the **projection oracle** of
//! `specs/drawings_and_mbd.md` §5.3.
//!
//! ## What the oracle can and cannot assert at D1a
//!
//! §5.3 states the oracle as "the projected bbox EQUALS the solid AABB's
//! projection". That is a D1b+ statement: at D1a the view holds edges only,
//! and a curved solid's extreme points are generally on a **silhouette**, not
//! on an edge — a revolved sphere's whole boundary is one seam meridian and
//! two poles, whose projection is far smaller than its AABB. So the oracle
//! splits in two here, and both halves are real:
//!
//! - **Containment** — the projected bbox lies inside the AABB's projection —
//!   holds for every solid the kernel can bound, in every direction, and is
//!   what catches a wrong basis, a swapped axis or a mis-signed depth.
//! - **Equality** — asserted for the PRISMATIC cases only. A curved solid is
//!   excluded for a second, independent reason beyond silhouettes: the
//!   kernel's own `solid_aabb` is documented CONSERVATIVE and bounds a circle
//!   EDGE by the box of its whole circle, so a z-axis cylinder's rim at
//!   `z = 0` inflates the reported box to `z ∈ [−r, r]`. An equality there
//!   would measure that slack, not the projection. The curved fixtures get
//!   exact per-case assertions instead (radius, chord length, minor axis).
//!
//! Two of the corpus's cases also show where the kernel declines to bound a
//! solid at all: `conservative_aabb` answers `None` once an edge carries an
//! unbounded-bulge curve, which the crossing cylinders do. The containment
//! test skips those by measurement and fails if the skip ever eats the corpus.
//!
//! The 180°-rotation length invariant holds for every solid at D1a and is
//! asserted for all of them: rotating the view about its own axis is an
//! isometry of the view plane, so every analytic reconstruction — the
//! ellipse's principal axes, its parameter range, the circular special case —
//! must come out the same length or the reconstruction is wrong.

use std::collections::HashMap;

use super::*;
use crate::KernelV2Adapter;
use waffle_types::kernel::projection::{Aabb2, KernelProjection, ProjectOpts, ViewFrame};
use waffle_types::kernel::{
    CircleProfile, ClosedProfile, Kernel, KernelIntrospect, KernelSolidHandle, ProjectionBody,
    RigidPlacement,
};

// ---------------------------------------------------------------------------
// fixtures
// ---------------------------------------------------------------------------

fn rect_profile(w: f64, d: f64) -> (ClosedProfile, HashMap<u32, (f64, f64)>) {
    let mut positions = HashMap::new();
    positions.insert(1, (0.0, 0.0));
    positions.insert(2, (w, 0.0));
    positions.insert(3, (w, d));
    positions.insert(4, (0.0, d));
    (
        ClosedProfile {
            entity_ids: vec![1, 2, 3, 4],
            is_outer: true,
            vertex_ids: vec![],
            circle: None,
            spline_segments: vec![],
            arc_segments: vec![],
        },
        positions,
    )
}

/// A `w` × `d` × `h` box with a corner at the origin.
fn make_box(a: &mut KernelV2Adapter, w: f64, d: f64, h: f64) -> KernelSolidHandle {
    let (profile, positions) = rect_profile(w, d);
    let faces = a
        .make_faces_from_profiles(
            &[profile],
            [0.0, 0.0, 0.0],
            [0.0, 0.0, 1.0],
            [1.0, 0.0, 0.0],
            &positions,
        )
        .expect("rectangle stages");
    a.extrude_face(faces[0], [0.0, 0.0, 1.0], h).expect("box")
}

/// A cylinder of `radius` about the z axis through `(cx, cy)`, from `z0`,
/// `height` tall.
fn make_cylinder(
    a: &mut KernelV2Adapter,
    (cx, cy): (f64, f64),
    radius: f64,
    z0: f64,
    height: f64,
) -> KernelSolidHandle {
    make_cylinder_on(
        a,
        [0.0, 0.0, z0],
        [0.0, 0.0, 1.0],
        [1.0, 0.0, 0.0],
        (cx, cy),
        radius,
        height,
    )
}

/// A cylinder on an arbitrary sketch plane, extruded along the plane normal —
/// how the corpus gets a cylinder whose axis is NOT `z`.
fn make_cylinder_on(
    a: &mut KernelV2Adapter,
    origin: [f64; 3],
    normal: [f64; 3],
    x_axis: [f64; 3],
    (cu, cv): (f64, f64),
    radius: f64,
    height: f64,
) -> KernelSolidHandle {
    let profile = ClosedProfile {
        entity_ids: vec![7],
        is_outer: true,
        vertex_ids: vec![],
        circle: Some(CircleProfile {
            center_u: cu,
            center_v: cv,
            radius,
        }),
        spline_segments: vec![],
        arc_segments: vec![],
    };
    let faces = a
        .make_faces_from_profiles(&[profile], origin, normal, x_axis, &HashMap::new())
        .expect("circle stages");
    a.extrude_face(faces[0], normal, height).expect("cylinder")
}

/// A D-shaped plate: a diameter chord and a semicircular arc, `height` tall.
///
/// The corpus's other curved cases carry FULL circles, so this is the only
/// fixture whose edges are partial `Curve::Arc`s — the arm whose projected
/// parameter range is derived rather than a full turn. (The arena splits the
/// 180° arc into two sub-arcs under its minor-arc limit, so the solid carries
/// four arc edges, two per rim.)
fn make_d_shape(a: &mut KernelV2Adapter, radius: f64, height: f64) -> KernelSolidHandle {
    let mut positions: HashMap<u32, (f64, f64)> = HashMap::new();
    positions.insert(0, (-radius, 0.0));
    positions.insert(1, (radius, 0.0));
    for (k, deg) in [
        (2u32, 30.0f64),
        (3, 60.0),
        (4, 90.0),
        (5, 120.0),
        (6, 150.0),
    ] {
        let t = deg.to_radians();
        positions.insert(k, (radius * t.cos(), radius * t.sin()));
    }
    let profile = ClosedProfile {
        entity_ids: vec![],
        is_outer: true,
        vertex_ids: vec![0, 1, 2, 3, 4, 5, 6],
        circle: None,
        spline_segments: vec![],
        // The arc covers vertices 1 → 0, over the top; end < start wraps.
        arc_segments: vec![waffle_types::ArcSegment {
            start_vertex_index: 1,
            end_vertex_index: 0,
            center_u: 0.0,
            center_v: 0.0,
            radius,
        }],
    };
    let faces = a
        .make_faces_from_profiles(
            &[profile],
            [0.0, 0.0, 0.0],
            [0.0, 0.0, 1.0],
            [1.0, 0.0, 0.0],
            &positions,
        )
        .expect("D-shape stages");
    a.extrude_face(faces[0], [0.0, 0.0, 1.0], height)
        .expect("D-shape extrudes")
}

/// The solids the oracle sweeps: a name, the handle, and whether the solid is
/// PRISMATIC (planar faces and straight edges only).
///
/// Prismatic matters because the equality half of §5.3's oracle is asserted
/// against the kernel's own [`KernelIntrospect::solid_aabb`], and that box is
/// documented as CONSERVATIVE: a circle or arc EDGE is bounded by the box of
/// its whole circle (`introspect::conservative_aabb`), so a z-axis cylinder's
/// rim at `z = 0` inflates the solid's reported box to `z ∈ [−r, r]`. Measured
/// here, 2026-10-03. Asserting equality against that would be measuring the
/// AABB's slack, not the projection; the curved cases get the containment
/// assertion plus the exact per-fixture checks above.
struct Case {
    name: &'static str,
    handle: KernelSolidHandle,
    prismatic: bool,
}

fn corpus(a: &mut KernelV2Adapter) -> Vec<Case> {
    let mut cases = Vec::new();

    cases.push(Case {
        name: "box",
        handle: make_box(a, 0.040, 0.030, 0.010),
        prismatic: true,
    });
    cases.push(Case {
        name: "cylinder",
        handle: make_cylinder(a, (0.0, 0.0), 0.008, 0.0, 0.020),
        prismatic: false,
    });
    // The only fixture with PARTIAL arc edges, so the only one that exercises
    // the arc's derived parameter range through the whole stack.
    cases.push(Case {
        name: "D-shaped plate",
        handle: make_d_shape(a, 0.012, 0.004),
        prismatic: false,
    });

    // Prismatic boolean: an L-shaped union, still planar everywhere, so the
    // equality oracle applies to a boolean output and not only a primitive.
    let p1 = make_box(a, 0.040, 0.010, 0.010);
    let p2 = make_box(a, 0.010, 0.030, 0.010);
    let ell = a
        .boolean_union(&p1, &p2)
        .expect("two boxes union into an L");
    cases.push(Case {
        name: "L (box union box)",
        handle: ell,
        prismatic: true,
    });

    // A through hole: the two rim circles exercise the circle→circle and
    // circle→segment arms on a boolean output.
    let plate = make_box(a, 0.040, 0.030, 0.010);
    let drill = make_cylinder(a, (0.020, 0.015), 0.006, -0.005, 0.020);
    let holed = a
        .boolean_subtract(&plate, &drill)
        .expect("plate minus a through hole");
    cases.push(Case {
        name: "box minus a through cylinder",
        handle: holed,
        prismatic: false,
    });

    // Crossing cylinders: the intersection is a general degree-4
    // cylinder×cylinder surface-pair curve (M5), which projects as a polyline.
    let along_z = make_cylinder(a, (0.0, 0.0), 0.008, -0.020, 0.040);
    let along_x = make_cylinder_on(
        a,
        [-0.020, 0.0, 0.0],
        [1.0, 0.0, 0.0],
        [0.0, 1.0, 0.0],
        (0.0, 0.0),
        0.005,
        0.040,
    );
    let cross = a
        .boolean_union(&along_z, &along_x)
        .expect("crossing cylinders union");
    cases.push(Case {
        name: "crossing cylinders",
        handle: cross,
        prismatic: false,
    });

    cases
}

// ---------------------------------------------------------------------------
// helpers
// ---------------------------------------------------------------------------

fn basis_of(frame: ViewFrame) -> ViewBasis {
    frame.basis().expect("a named view frame has a basis")
}

/// The six axis-aligned views §5.3 asks for.
fn axis_views() -> Vec<(&'static str, ViewFrame)> {
    vec![
        ("+x", ViewFrame::looking_along([1.0, 0.0, 0.0])),
        ("-x", ViewFrame::looking_along([-1.0, 0.0, 0.0])),
        ("+y", ViewFrame::looking_along([0.0, 1.0, 0.0])),
        ("-y", ViewFrame::looking_along([0.0, -1.0, 0.0])),
        ("+z", ViewFrame::looking_along([0.0, 0.0, 1.0])),
        ("-z", ViewFrame::looking_along([0.0, 0.0, -1.0])),
    ]
}

/// The projection of an axis-aligned box's eight corners.
fn aabb_projection(basis: &ViewBasis, lo: [f64; 3], hi: [f64; 3]) -> Aabb2 {
    let mut bb: Option<Aabb2> = None;
    for i in 0..8 {
        let p = [
            if i & 1 == 0 { lo[0] } else { hi[0] },
            if i & 2 == 0 { lo[1] } else { hi[1] },
            if i & 4 == 0 { lo[2] } else { hi[2] },
        ];
        let (uv, _) = basis.project(p);
        bb = Some(match bb {
            None => Aabb2::point(uv),
            Some(b) => b.united_point(uv),
        });
    }
    bb.expect("eight corners")
}

fn dist_point_segment(p: Point2, a: Point2, b: Point2) -> f64 {
    let (vx, vy) = (b.x() - a.x(), b.y() - a.y());
    let len2 = vx * vx + vy * vy;
    let t = if len2 <= 0.0 {
        0.0
    } else {
        (((p.x() - a.x()) * vx + (p.y() - a.y()) * vy) / len2).clamp(0.0, 1.0)
    };
    (p.x() - (a.x() + t * vx)).hypot(p.y() - (a.y() + t * vy))
}

/// Distance from `p` to a curve, measured against a flattening fine enough
/// that the flattening error is well under the tolerances the tests use.
fn dist_to_curve(p: Point2, c: &Curve2) -> f64 {
    let pts = c.flatten(1e-9);
    if pts.len() == 1 {
        return (p.x() - pts[0].x()).hypot(p.y() - pts[0].y());
    }
    let mut best = f64::INFINITY;
    for w in pts.windows(2) {
        best = best.min(dist_point_segment(p, w[0], w[1]));
    }
    if c.is_closed() && pts.len() > 2 {
        best = best.min(dist_point_segment(p, pts[pts.len() - 1], pts[0]));
    }
    best
}

// ---------------------------------------------------------------------------
// the analytic arms, one at a time
// ---------------------------------------------------------------------------

#[test]
fn a_boxs_top_view_is_eight_segments_and_four_points() {
    let mut a = KernelV2Adapter::new();
    let solid = make_box(&mut a, 0.040, 0.030, 0.010);
    let view = a
        .project(&solid, &ViewFrame::TOP, &ProjectOpts::default())
        .expect("box projects");

    let lines = view
        .curves
        .iter()
        .filter(|c| matches!(c.geometry, Curve2::Line { .. }))
        .count();
    let points = view
        .curves
        .iter()
        .filter(|c| matches!(c.geometry, Curve2::Point(_)))
        .count();
    assert_eq!(view.curves.len(), 12, "a box has twelve edges");
    // The four verticals run along the line of sight and collapse.
    assert_eq!((lines, points), (8, 4));

    let bb = view.bbox.expect("a box has a box");
    assert!((bb.min.x() - 0.0).abs() < 1e-12 && (bb.min.y() - 0.0).abs() < 1e-12);
    assert!((bb.max.x() - 0.040).abs() < 1e-12 && (bb.max.y() - 0.030).abs() < 1e-12);

    // Every curve is Visible and names its edge (D1a: no hidden lines yet).
    assert!(view
        .curves
        .iter()
        .all(|c| c.visibility == Visibility::Visible && c.kind == CurveKind::Edge));
    let sources: Vec<u64> = view
        .curves
        .iter()
        .filter_map(|c| c.source.map(|k| k.0))
        .collect();
    let mut distinct = sources.clone();
    distinct.sort_unstable();
    distinct.dedup();
    assert_eq!(distinct.len(), 12, "one distinct edge id per curve");
    assert_eq!(
        sources,
        a.list_edges(&solid).iter().map(|k| k.0).collect::<Vec<_>>(),
        "the sources ARE the solid's edges, in order"
    );
}

#[test]
fn a_rims_circle_survives_as_a_circle_from_the_top() {
    let mut a = KernelV2Adapter::new();
    let solid = make_cylinder(&mut a, (0.003, -0.002), 0.008, 0.0, 0.020);
    let view = a
        .project(&solid, &ViewFrame::TOP, &ProjectOpts::default())
        .expect("cylinder projects");

    let circles: Vec<&Curve2> = view
        .curves
        .iter()
        .map(|c| &c.geometry)
        .filter(|g| matches!(g, Curve2::Circle { .. }))
        .collect();
    assert_eq!(circles.len(), 2, "two rims, seen face-on");
    for g in circles {
        let Curve2::Circle {
            center,
            radius,
            start_angle,
            end_angle,
        } = g
        else {
            unreachable!()
        };
        assert!((radius - 0.008).abs() < 1e-12, "radius {radius}");
        assert!((center.x() - 0.003).abs() < 1e-12 && (center.y() + 0.002).abs() < 1e-12);
        assert!(
            (end_angle - start_angle - std::f64::consts::TAU).abs() < 1e-12,
            "a rim is a FULL circle"
        );
        assert!(g.is_closed());
    }
}

#[test]
fn a_rims_circle_collapses_to_a_segment_seen_edge_on() {
    let mut a = KernelV2Adapter::new();
    let r = 0.008;
    let solid = make_cylinder(&mut a, (0.0, 0.0), r, 0.0, 0.020);
    let view = a
        .project(&solid, &ViewFrame::FRONT, &ProjectOpts::default())
        .expect("cylinder projects");

    // Both rims are edge-on: each is a segment of length 2r at its own height.
    let mut segments: Vec<(f64, f64)> = Vec::new(); // (v, length)
    for c in &view.curves {
        if let Curve2::Line { start, end } = c.geometry {
            if (start.y() - end.y()).abs() < 1e-12 {
                segments.push((start.y(), (end.x() - start.x()).abs()));
            }
        }
    }
    segments.sort_by(|a, b| a.0.total_cmp(&b.0));
    assert_eq!(segments.len(), 2, "two edge-on rims, got {segments:?}");
    for (v, len) in &segments {
        assert!((len - 2.0 * r).abs() < 1e-12, "chord {len} should be 2r");
        assert!(v.abs() < 1e-12 || (v - 0.020).abs() < 1e-12, "height {v}");
    }
}

#[test]
fn a_rims_circle_becomes_an_ellipse_seen_obliquely() {
    let mut a = KernelV2Adapter::new();
    let r = 0.008;
    let solid = make_cylinder(&mut a, (0.0, 0.0), r, 0.0, 0.020);
    // 60° off the cylinder axis: the rim's minor axis is r·cos 60° = r/2.
    let theta = std::f64::consts::FRAC_PI_3;
    let frame = ViewFrame {
        origin: [0.0; 3],
        dir: [0.0, theta.sin(), -theta.cos()],
        up: [0.0, 0.0, 1.0],
    };
    let view = a
        .project(&solid, &frame, &ProjectOpts::default())
        .expect("cylinder projects");

    let ellipses: Vec<&Curve2> = view
        .curves
        .iter()
        .map(|c| &c.geometry)
        .filter(|g| matches!(g, Curve2::Ellipse { .. }))
        .collect();
    assert_eq!(ellipses.len(), 2, "two rims seen obliquely");
    for g in ellipses {
        let Curve2::Ellipse {
            major_radius,
            minor_radius,
            start_param,
            end_param,
            major_axis,
            ..
        } = g
        else {
            unreachable!()
        };
        assert!((major_radius - r).abs() < 1e-12, "major {major_radius}");
        assert!(
            (minor_radius - r * theta.cos()).abs() < 1e-12,
            "minor {minor_radius} should be r·cos θ"
        );
        assert!((major_axis[0].hypot(major_axis[1]) - 1.0).abs() < 1e-12);
        assert!(
            (end_param - start_param - std::f64::consts::TAU).abs() < 1e-12,
            "a full rim stays a full ellipse"
        );
    }
}

/// The decisive D1a oracle: a projected curve must CONTAIN the projection of
/// its own 3-D edge.
///
/// Every edge's render polyline (`KernelIntrospect::edge_polyline`, the same
/// samples the viewport's edge overlay draws) is projected point by point and
/// must land on the analytic 2-D curve the projection reported for that edge.
/// Nothing about the reconstruction survives a mistake here: a wrong ellipse
/// axis, a flipped parameter sense, a swapped degenerate branch all move the
/// curve away from its own points.
#[test]
fn every_projected_curve_contains_its_own_three_dimensional_samples() {
    let mut a = KernelV2Adapter::new();
    for case in corpus(&mut a) {
        let edges = a.list_edges(&case.handle);
        for (name, frame) in axis_views() {
            let basis = basis_of(frame);
            let view = a
                .project(&case.handle, &frame, &ProjectOpts::default())
                .unwrap_or_else(|e| panic!("{} along {name}: {e}", case.name));
            assert_eq!(
                view.curves.len(),
                edges.len(),
                "{} along {name}: one curve per edge",
                case.name
            );
            for (i, (curve, id)) in view.curves.iter().zip(edges.iter()).enumerate() {
                assert_eq!(curve.source, Some(*id), "{} edge {i}", case.name);
                for p3 in a.edge_polyline(*id) {
                    let (uv, _) = basis.project(p3);
                    let d = dist_to_curve(uv, &curve.geometry);
                    assert!(
                        d < 1e-9,
                        "{} along {name}, edge {i}: a 3-D sample projects {d} off its curve {:?}",
                        case.name,
                        curve.geometry
                    );
                }
            }
        }
    }
}

/// A PARTIAL arc, projected, against a brute-force sampling of the same arc —
/// in BOTH directions.
///
/// The corpus's circles are all FULL turns, so nothing else here exercises the
/// arc's parameter range: `ccw_sweep` → [`ccw_range`] → the ellipse's
/// `start_param`/`end_param`. Containment of the 3-D samples cannot see that
/// range being too WIDE (a quarter arc reported as its whole ellipse still
/// contains every sample), so this pins all three of the arc's endpoints, its
/// length and its samples, over a sweep of orientations and angles.
#[test]
fn a_partial_arcs_projection_agrees_with_a_brute_force_sampling_both_ways() {
    use std::f64::consts::FRAC_PI_2;

    let center = Point3::new(0.013, -0.007, 0.004);
    let radius = 0.008;
    // Each a circle plane normal, spanning face-on, oblique and edge-on to
    // every view the sweep below looks from.
    let normals: [[f64; 3]; 6] = [
        [0.0, 0.0, 1.0],
        [1.0, 0.0, 0.0],
        [0.0, 1.0, 0.0],
        [0.6, 0.0, 0.8],
        [0.3, -0.5, 0.8],
        [1.0, 1.0, 1.0],
    ];
    let sweeps = [0.2, FRAC_PI_2, 2.5, PI, 4.0, 6.0];
    let mut checked = 0usize;

    for n in normals {
        let len = (n[0] * n[0] + n[1] * n[1] + n[2] * n[2]).sqrt();
        let nu = UnitVector3 {
            x: n[0] / len,
            y: n[1] / len,
            z: n[2] / len,
        };
        // An in-plane frame for the circle, right-handed with `nu`, so a
        // growing parameter sweeps counter-clockwise about it — the same
        // convention `circle_frame` and `geom::ccw_sweep` use.
        let seed = if nu.x.abs() < 0.9 {
            [1.0, 0.0, 0.0]
        } else {
            [0.0, 1.0, 0.0]
        };
        let t = seed[0] * nu.x + seed[1] * nu.y + seed[2] * nu.z;
        let f1raw = [seed[0] - t * nu.x, seed[1] - t * nu.y, seed[2] - t * nu.z];
        let l1 = (f1raw[0] * f1raw[0] + f1raw[1] * f1raw[1] + f1raw[2] * f1raw[2]).sqrt();
        let f1 = [f1raw[0] / l1, f1raw[1] / l1, f1raw[2] / l1];
        let f2 = [
            nu.y * f1[2] - nu.z * f1[1],
            nu.z * f1[0] - nu.x * f1[2],
            nu.x * f1[1] - nu.y * f1[0],
        ];
        let at = |theta: f64| -> Point3 {
            let (s, c) = theta.sin_cos();
            Point3::new(
                center.x() + radius * (c * f1[0] + s * f2[0]),
                center.y() + radius * (c * f1[1] + s * f2[1]),
                center.z() + radius * (c * f1[2] + s * f2[2]),
            )
        };

        for sweep in sweeps {
            let (start, end) = (at(0.0), at(sweep));
            for (name, frame) in axis_views() {
                let basis = basis_of(frame);
                let got = project_circle(&basis, center, nu, radius, start, Some(end))
                    .unwrap_or_else(|| panic!("{name}, n {n:?}, sweep {sweep}: no analytic arm"));
                checked += 1;

                // Brute force: the arc's own points, projected.
                const N: usize = 2048;
                let samples: Vec<Point2> = (0..=N)
                    .map(|i| project_point(&basis, at(sweep * i as f64 / N as f64)))
                    .collect();

                // 1. Every sampled point lands on the reported curve.
                //    Measured against ONE 1e-9 flattening of it, so 1e-9 is
                //    this check's own floor; the sharp checks are 2 and 3.
                let flat = got.flatten(1e-9);
                for p in &samples {
                    let mut d = f64::INFINITY;
                    for w in flat.windows(2) {
                        d = d.min(dist_point_segment(*p, w[0], w[1]));
                    }
                    assert!(
                        d < 2e-9,
                        "{name}, n {n:?}, sweep {sweep}: a sample is {d} off {got:?}"
                    );
                }

                // 2. The reported curve is no LONGER than the arc. An
                //    edge-on arc is excluded: its segment legitimately runs
                //    to the `cos` extremes of the range, which lie off the
                //    arc's endpoints, and `cos_range` is tested directly.
                let brute: f64 = samples
                    .windows(2)
                    .map(|w| (w[1].x() - w[0].x()).hypot(w[1].y() - w[0].y()))
                    .sum();
                if matches!(got, Curve2::Ellipse { .. } | Curve2::Circle { .. }) {
                    // The chord sum has its own O(Δ²) deficit — 1.6e-7
                    // relative at `N` over a 4-radian sweep — so this is a
                    // 1e-5 check. It is still four orders sharper than any
                    // wrong range, which changes the length by a FACTOR; the
                    // exact check on the range is the endpoints below.
                    assert!(
                        (got.length() - brute).abs() <= 1e-5 * brute.max(1e-9),
                        "{name}, n {n:?}, sweep {sweep}: length {} vs {brute} — the \
                         parameter range is wrong",
                        got.length()
                    );
                    // 3. And it starts and ends where the arc does.
                    let (a, b) = got.endpoints().unwrap_or_else(|| {
                        panic!("{name}, n {n:?}, sweep {sweep}: a partial arc has endpoints")
                    });
                    let (s, e) = (samples[0], samples[N]);
                    let near = |p: Point2, q: Point2| (p.x() - q.x()).hypot(p.y() - q.y()) < 1e-11;
                    assert!(
                        (near(a, s) && near(b, e)) || (near(a, e) && near(b, s)),
                        "{name}, n {n:?}, sweep {sweep}: endpoints {a:?},{b:?} are not \
                         the arc's {s:?},{e:?}"
                    );
                } else {
                    // The edge-on segment must still CONTAIN the arc's span.
                    assert!(
                        got.length() >= brute * 0.5 - 1e-12,
                        "{name}, n {n:?}, sweep {sweep}: the edge-on segment {} is \
                         shorter than half the arc's projected length {brute}",
                        got.length()
                    );
                }
            }
        }
    }
    assert_eq!(checked, normals.len() * sweeps.len() * 6);
}

// ---------------------------------------------------------------------------
// the §5.3 projection oracle
// ---------------------------------------------------------------------------

#[test]
fn the_projected_bbox_lies_inside_the_aabb_projection() {
    let mut a = KernelV2Adapter::new();
    let mut checked = 0;
    for case in corpus(&mut a) {
        // `conservative_aabb` answers `None` for a solid carrying an
        // unbounded-bulge curve — a surface-pair or hyperbola edge — so the
        // crossing cylinders have no box to compare against. Skipped by
        // measurement, not assumed: the counter below fails if the skip ever
        // swallows the whole corpus.
        let Some((lo, hi)) = a.solid_aabb(&case.handle) else {
            continue;
        };
        checked += 1;
        for (name, frame) in axis_views() {
            let basis = basis_of(frame);
            let view = a
                .project(&case.handle, &frame, &ProjectOpts::default())
                .unwrap_or_else(|e| panic!("{} along {name}: {e}", case.name));
            let got = view.bbox.expect("a solid has curves");
            let want = aabb_projection(&basis, lo, hi);
            assert!(
                got.within(&want, 1e-9),
                "{} along {name}: projected {got:?} escapes the AABB projection {want:?}",
                case.name
            );
        }
    }
    assert!(checked >= 4, "only {checked} cases could be bounded");
}

#[test]
fn the_projected_bbox_equals_the_aabb_projection_for_prismatic_solids() {
    let mut a = KernelV2Adapter::new();
    for case in corpus(&mut a).into_iter().filter(|c| c.prismatic) {
        let (lo, hi) = a
            .solid_aabb(&case.handle)
            .expect("kernel-v2 bounds a solid");
        for (name, frame) in axis_views() {
            let basis = basis_of(frame);
            let view = a
                .project(&case.handle, &frame, &ProjectOpts::default())
                .unwrap_or_else(|e| panic!("{} along {name}: {e}", case.name));
            let got = view.bbox.expect("a solid has curves");
            let want = aabb_projection(&basis, lo, hi);
            for (g, w, which) in [
                (got.min.x(), want.min.x(), "min u"),
                (got.min.y(), want.min.y(), "min v"),
                (got.max.x(), want.max.x(), "max u"),
                (got.max.y(), want.max.y(), "max v"),
            ] {
                assert!(
                    (g - w).abs() < 1e-9,
                    "{} along {name}: {which} is {g}, AABB says {w}",
                    case.name
                );
            }
        }
    }
}

#[test]
fn the_visible_length_is_invariant_under_a_180_degree_view_rotation() {
    let mut a = KernelV2Adapter::new();
    for case in corpus(&mut a) {
        for (name, frame) in axis_views() {
            let flipped = ViewFrame {
                origin: frame.origin,
                dir: frame.dir,
                up: [-frame.up[0], -frame.up[1], -frame.up[2]],
            };
            let one = a
                .project(&case.handle, &frame, &ProjectOpts::default())
                .unwrap_or_else(|e| panic!("{} along {name}: {e}", case.name))
                .total_length(Visibility::Visible);
            let two = a
                .project(&case.handle, &flipped, &ProjectOpts::default())
                .unwrap_or_else(|e| panic!("{} along {name} flipped: {e}", case.name))
                .total_length(Visibility::Visible);
            assert!(one > 0.0, "{} along {name}: nothing projected", case.name);
            assert!(
                (one - two).abs() <= 1e-9 * one,
                "{} along {name}: {one} vs {two} after a half turn",
                case.name
            );
        }
    }
}

// ---------------------------------------------------------------------------
// placement, ordering and the loud refusals
// ---------------------------------------------------------------------------

#[test]
fn a_placed_body_projects_where_the_placement_puts_it() {
    let mut a = KernelV2Adapter::new();
    let solid = make_box(&mut a, 0.040, 0.030, 0.010);
    let placement = RigidPlacement {
        translation: [0.100, -0.050, 0.0],
        rotation: RigidPlacement::rotation_matrix([0.0, 0.0, 1.0], std::f64::consts::FRAC_PI_2),
    };
    let placed = a
        .project_bodies(
            &[ProjectionBody {
                handle: solid.clone(),
                name: "Plate".to_string(),
                placement: Some(placement),
            }],
            &ViewFrame::TOP,
            &ProjectOpts::default(),
        )
        .expect("a placed body projects");
    let bb = placed.bbox.expect("curves");
    // A quarter turn about z swaps the footprint's extents, then translates.
    assert!((bb.min.x() - (0.100 - 0.030)).abs() < 1e-12, "{bb:?}");
    assert!((bb.max.x() - 0.100).abs() < 1e-12, "{bb:?}");
    assert!((bb.min.y() + 0.050).abs() < 1e-12, "{bb:?}");
    assert!((bb.max.y() - (-0.050 + 0.040)).abs() < 1e-12, "{bb:?}");

    // Two bodies land in ONE view, with one union bounding box.
    let both = a
        .project_bodies(
            &[
                ProjectionBody::solo(solid.clone()),
                ProjectionBody {
                    handle: solid,
                    name: "Plate copy".to_string(),
                    placement: Some(placement),
                },
            ],
            &ViewFrame::TOP,
            &ProjectOpts::default(),
        )
        .expect("two bodies project");
    assert_eq!(both.curves.len(), 24);
    let bb = both.bbox.expect("curves");
    assert!(
        (bb.min.x() - 0.0).abs() < 1e-12 && (bb.max.x() - 0.100).abs() < 1e-12,
        "{bb:?}"
    );
}

#[test]
fn the_curve_order_is_the_listed_edge_order() {
    let mut a = KernelV2Adapter::new();
    let solid = make_cylinder(&mut a, (0.0, 0.0), 0.008, 0.0, 0.020);
    let edges = a.list_edges(&solid);
    let view = a
        .project(&solid, &ViewFrame::TOP, &ProjectOpts::default())
        .expect("projects");
    assert_eq!(
        view.curves.iter().map(|c| c.source).collect::<Vec<_>>(),
        edges.into_iter().map(Some).collect::<Vec<_>>(),
        "the nth curve is the nth listed edge"
    );
}

#[test]
fn a_degenerate_view_frame_is_refused_loudly() {
    let mut a = KernelV2Adapter::new();
    let solid = make_box(&mut a, 0.010, 0.010, 0.010);
    let err = a
        .project(
            &solid,
            &ViewFrame {
                origin: [0.0; 3],
                dir: [0.0, 0.0, 1.0],
                up: [0.0, 0.0, 2.0],
            },
            &ProjectOpts::default(),
        )
        .expect_err("up parallel to the line of sight has no basis");
    assert!(format!("{err}").contains("degenerate view frame"), "{err}");

    let err = a
        .project(
            &solid,
            &ViewFrame::TOP,
            &ProjectOpts {
                rel_chord_tolerance: Some(0.0),
            },
        )
        .expect_err("a zero chord tolerance is not a density");
    assert!(format!("{err}").contains("chord tolerance"), "{err}");
}

#[test]
fn an_unknown_handle_is_refused() {
    let a = KernelV2Adapter::new();
    let err = a
        .project(
            &KernelSolidHandle::from_raw(9_999),
            &ViewFrame::TOP,
            &ProjectOpts::default(),
        )
        .expect_err("no such solid");
    assert!(format!("{err}").contains("unknown solid handle"), "{err}");
}

#[test]
fn a_finer_chord_tolerance_buys_more_polyline_samples() {
    let mut a = KernelV2Adapter::new();
    // Crossing cylinders: the surface-pair intersection curve is the one that
    // cannot stay analytic at D1a, so it is the one a tolerance moves.
    let cross = corpus(&mut a)
        .into_iter()
        .find(|c| c.name == "crossing cylinders")
        .expect("the corpus has crossing cylinders")
        .handle;
    let count = |rel: f64| -> usize {
        a.project(
            &cross,
            &ViewFrame::TOP,
            &ProjectOpts {
                rel_chord_tolerance: Some(rel),
            },
        )
        .expect("projects")
        .curves
        .iter()
        .map(|c| match &c.geometry {
            Curve2::Polyline { points, .. } => points.len(),
            _ => 0,
        })
        .sum()
    };
    let coarse = count(1e-2);
    let fine = count(1e-4);
    assert!(coarse > 0, "the union has sampled curves");
    assert!(
        fine > coarse,
        "a finer tolerance must sample more: {coarse} then {fine}"
    );
}

// ---------------------------------------------------------------------------
// the small pure helpers
// ---------------------------------------------------------------------------

#[test]
fn cos_range_is_exact_over_a_parameter_interval() {
    let (lo, hi) = cos_range(0.0, TAU);
    assert!((lo + 1.0).abs() < 1e-15 && (hi - 1.0).abs() < 1e-15);
    // A quarter turn from 0: cos falls from 1 to 0, both endpoints extremal.
    let (lo, hi) = cos_range(0.0, PI / 2.0);
    assert!(lo.abs() < 1e-15 && (hi - 1.0).abs() < 1e-15);
    // Straddling π picks up the -1.
    let (lo, hi) = cos_range(PI / 2.0, 3.0 * PI / 2.0);
    assert!((lo + 1.0).abs() < 1e-15 && hi.abs() < 1e-15);
}

#[test]
fn ccw_range_normalizes_both_senses_to_increasing() {
    let (a, b) = ccw_range(0.0, 1.0, 0.25, 1.0);
    assert!(a < b);
    assert!((a + 0.25).abs() < 1e-15 && (b - 0.75).abs() < 1e-15);
    let (a, b) = ccw_range(0.0, 1.0, 0.25, -1.0);
    assert!(a < b, "a reversed sense still reads counter-clockwise");
    assert!((a + 0.75).abs() < 1e-15 && (b - 0.25).abs() < 1e-15);
}

#[test]
fn interval_hits_period_only_strictly_inside() {
    assert!(interval_hits_period(-0.1, 0.1, 0.0));
    assert!(!interval_hits_period(0.0, 1.0, 0.0), "endpoints excluded");
    assert!(interval_hits_period(6.0, 7.0, 0.0), "the next period");
    assert!(!interval_hits_period(0.1, 3.0, PI));
}
