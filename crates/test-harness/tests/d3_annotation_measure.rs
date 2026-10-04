//! D3's real-geometry pins (`specs/drawings_and_mbd.md` §7): a dimension
//! whose anchors are **persistent ids** on a solid the kernel actually built,
//! resolved through the projection D1 produces, measures the part.
//!
//! The pure-function tests for [`waffle_types::annotation::measure`] live
//! beside it in `waffle-types`. What they cannot reach is the whole path:
//!
//! ```text
//! build a solid → project it (D1) → map each projected curve back to its
//! entity's EntityPid (D0) → resolve the annotation's Selector::Pid anchors
//! to that geometry → measure
//! ```
//!
//! That path is where a wrong number would actually come from — a pid that
//! names a different edge after projection, a projected circle whose radius
//! is the chord polygon's rather than the surface's, a view basis that scales.
//! Each test below asserts the measured value against the dimension the
//! fixture was AUTHORED with, not against a recorded number.
//!
//! ```text
//! cargo test -p test-harness --test d3_annotation_measure
//! ```

use std::collections::BTreeMap;

use test_harness::workflow::ModelBuilder;
use waffle_types::annotation::layout::{AnchorGeometry, LayoutCurve};
use waffle_types::annotation::measure::measure;
use waffle_types::annotation::{Annotation, DimensionKind, Measured, Placement2};
use waffle_types::geom_ref::{Anchor, GeomRef, OutputKey, ResolvePolicy, Selector};
use waffle_types::kernel::projection::{CurveKind, ProjectOpts, ViewFrame};
use waffle_types::kernel::{ProjectionBody, TopoKind};

/// Looking straight down −Z: the drafter's TOP view, where a box's width and
/// depth are true length and a vertical cylinder's rim is a true circle.
fn top_view() -> ViewFrame {
    ViewFrame::looking_along([0.0, 0.0, -1.0])
}

/// Every projected EDGE curve of the model's live bodies, keyed by the
/// `EntityPid` of the edge it came from — the map a drawing rebuild needs to
/// turn an annotation's `Selector::Pid` anchor into geometry.
///
/// A pid that names more than one projected curve is dropped rather than
/// disambiguated: a dimension must not silently pick one of two.
fn pid_to_curve(builder: &mut ModelBuilder) -> BTreeMap<u64, LayoutCurve> {
    pid_to_curve_in(builder, &top_view())
}

/// [`pid_to_curve`] in an arbitrary view — an oblique one foreshortens, which
/// is where a radial dimension's "the major radius IS the true radius" claim
/// has to hold.
fn pid_to_curve_in(builder: &mut ModelBuilder, frame: &ViewFrame) -> BTreeMap<u64, LayoutCurve> {
    let handles = builder.live_solid_handles();
    assert!(!handles.is_empty(), "the fixture built no live body");
    let bodies: Vec<ProjectionBody> = handles
        .iter()
        .map(|h| ProjectionBody::solo(h.clone()))
        .collect();
    let view = builder
        .kernel_mut()
        .project_bodies(&bodies, frame, &ProjectOpts::default())
        .expect("the top view projects");

    let mut by_pid: BTreeMap<u64, Vec<LayoutCurve>> = BTreeMap::new();
    for curve in &view.curves {
        if curve.kind != CurveKind::Edge {
            continue;
        }
        let Some(source) = curve.source else { continue };
        let Some(pid) = builder
            .kernel_mut()
            .as_introspect()
            .entity_pid(source, TopoKind::Edge)
        else {
            continue;
        };
        by_pid
            .entry(pid.pid)
            .or_default()
            .push(LayoutCurve::from_curve2(&curve.geometry));
    }
    by_pid
        .into_iter()
        .filter(|(_, curves)| curves.len() == 1)
        .map(|(pid, mut curves)| (pid, curves.remove(0)))
        .collect()
}

/// An annotation anchor on `pid`, the way the UI will author one (D0 item 4).
fn pid_anchor(kind: TopoKind, pid: u64) -> GeomRef {
    GeomRef {
        kind,
        anchor: Anchor::FeatureOutput {
            feature_id: uuid::Uuid::nil(),
            output_key: OutputKey::Main,
        },
        selector: Selector::Pid { pid, root_pid: pid },
        policy: ResolvePolicy::Strict,
        scope: None,
    }
}

/// Resolve the annotation's anchors through `map` and measure it — the
/// rebuild's own two steps, so a test cannot accidentally measure geometry
/// the annotation does not name.
///
/// `Err` names the step that refused: an anchor whose pid resolved to nothing
/// (D0's never-rebinding `Selector::Pid`) or geometry the dimension cannot
/// measure.
fn try_resolve_and_measure(
    annotation: &Annotation,
    map: &BTreeMap<u64, LayoutCurve>,
) -> Result<f64, String> {
    let Annotation::Dimension { kind, anchors, .. } = annotation else {
        panic!("not a dimension");
    };
    let mut resolved: Vec<AnchorGeometry> = Vec::with_capacity(anchors.len());
    for a in anchors {
        let Selector::Pid { pid, .. } = a.selector else {
            panic!("an annotation anchor must be a Pid selector");
        };
        let curve = map
            .get(&pid)
            .ok_or_else(|| format!("pid {pid} resolved to no projected curve"))?;
        resolved.push(AnchorGeometry::curve(curve.clone()));
    }
    measure(*kind, &resolved).map_err(|e| e.to_string())
}

fn resolve_and_measure(annotation: &Annotation, map: &BTreeMap<u64, LayoutCurve>) -> f64 {
    try_resolve_and_measure(annotation, map).expect("the dimension measures")
}

fn dimension(kind: DimensionKind, pids: &[u64]) -> Annotation {
    Annotation::Dimension {
        kind,
        anchors: pids
            .iter()
            .map(|p| pid_anchor(TopoKind::Edge, *p))
            .collect(),
        value: Measured::FromGeometry,
        tolerance: None,
        precision: Some(2),
        dual_unit: None,
        dual_precision: None,
        placement: Placement2::default(),
    }
}

/// A 40 × 25 mm plate, 10 mm thick, on the XY plane.
const PLATE_W: f64 = 0.040;
const PLATE_D: f64 = 0.025;
const PLATE_T: f64 = 0.010;

/// The sketch's own +x is pinned to world +x (`rect_sketch_oriented`), not
/// left to the derived basis — otherwise which world axis carries `PLATE_W`
/// is the sketch plane's choice, and [`expected_extent`] has nothing to
/// predict from.
fn plate() -> ModelBuilder {
    let mut builder = ModelBuilder::kernel_v2();
    builder
        .rect_sketch_oriented(
            "s",
            [0.0, 0.0, 0.0],
            [0.0, 0.0, 1.0],
            Some([1.0, 0.0, 0.0]),
            0.0,
            0.0,
            PLATE_W,
            PLATE_D,
        )
        .expect("sketch");
    builder.extrude("plate", "s", PLATE_T).expect("extrude");
    builder
}

/// The plate's extent that lands on view axis `axis` (0 = `u`, 1 = `v`).
///
/// Which world axis a named view puts on `u` is the view basis's business —
/// `ViewFrame::looking_along` derives `up`, and for the top view it lands the
/// sketch's +y on `u`, not +x. So the expected number is **derived from the
/// basis** rather than assumed: whichever world direction projects onto
/// `axis` brings its own authored extent with it.
fn expected_extent(axis: usize) -> f64 {
    let basis = top_view().basis().expect("an axis view has a basis");
    let x = basis.project_dir([1.0, 0.0, 0.0]);
    let y = basis.project_dir([0.0, 1.0, 0.0]);
    // Exactly one of world x / world y lands on this view axis for an
    // axis-aligned view; refuse rather than guess if the basis is oblique.
    let on_x = x[axis].abs() > 1.0 - 1e-12;
    let on_y = y[axis].abs() > 1.0 - 1e-12;
    assert!(
        on_x ^ on_y,
        "the top view should be axis-aligned; world x → {x:?}, world y → {y:?}"
    );
    if on_x {
        PLATE_W
    } else {
        PLATE_D
    }
}

/// The two projected lines perpendicular to view axis `axis`, at that axis's
/// extremes — the box's two opposite walls. Returned as
/// `(low_pid, high_pid)`, with the span cross-checked against
/// [`expected_extent`].
fn wall_pair(map: &BTreeMap<u64, LayoutCurve>, axis: usize) -> (u64, u64) {
    let other = 1 - axis;
    let mut walls: Vec<(u64, f64)> = map
        .iter()
        .filter_map(|(pid, curve)| {
            let LayoutCurve::Line { start, end } = curve else {
                return None;
            };
            // Constant along `axis`, extended along the other — and not a
            // degenerate point.
            let flat = (end[axis] - start[axis]).abs() < 1e-12;
            let long = (end[other] - start[other]).abs() > 1e-9;
            (flat && long).then_some((*pid, start[axis]))
        })
        .collect();
    walls.sort_by(|a, b| a.1.total_cmp(&b.1));
    assert!(
        walls.len() >= 2,
        "a top view of a box has at least two walls perpendicular to axis {axis}, found {}",
        walls.len()
    );
    let (lo_pid, lo) = walls[0];
    let (hi_pid, hi) = *walls.last().unwrap();
    let expected = expected_extent(axis);
    assert!(
        (hi - lo - expected).abs() < 1e-9,
        "the extreme walls should span {expected} on axis {axis}: {lo} .. {hi}"
    );
    (lo_pid, hi_pid)
}

#[test]
fn a_linear_dimension_between_two_box_edges_in_a_top_view_measures_the_box_side() {
    let mut builder = plate();
    let map = pid_to_curve(&mut builder);

    // Both of the plate's sides, and for each, both readings of it: the
    // single-component linear dimension (`HDistance` on the u pair,
    // `VDistance` on the v pair) and the aligned distance between the two
    // parallel walls. All four must be the authored extent for that axis.
    for (axis, across) in [(0, DimensionKind::HDistance), (1, DimensionKind::VDistance)] {
        let (lo, hi) = wall_pair(&map, axis);
        let expected = expected_extent(axis);
        for kind in [across, DimensionKind::Distance] {
            let v = resolve_and_measure(&dimension(kind, &[lo, hi]), &map);
            assert!(
                (v - expected).abs() < 1e-12,
                "{kind:?} on axis {axis} measured {v}, the plate is {expected}"
            );
        }
    }

    // And the two sides really are the two the plate was authored with.
    let mut sides = [expected_extent(0), expected_extent(1)];
    sides.sort_by(f64::total_cmp);
    assert_eq!(sides, [PLATE_D, PLATE_W]);
}

#[test]
fn the_component_across_a_pair_of_walls_is_zero_in_the_direction_they_run() {
    // The reason a drawing needs the single-component kinds at all: on the
    // very same anchors, the OTHER component is zero. A `VDistance` that
    // quietly returned the aligned distance here would print the plate's
    // width under a vertical dimension's arrows.
    let mut builder = plate();
    let map = pid_to_curve(&mut builder);
    for (axis, along) in [(0, DimensionKind::VDistance), (1, DimensionKind::HDistance)] {
        let (lo, hi) = wall_pair(&map, axis);
        let v = resolve_and_measure(&dimension(along, &[lo, hi]), &map);
        assert!(v.abs() < 1e-12, "{along:?} on axis {axis} measured {v}");
    }
}

#[test]
fn an_angular_dimension_between_two_adjacent_box_walls_reads_ninety_degrees() {
    let mut builder = plate();
    let map = pid_to_curve(&mut builder);
    let (u_wall, _) = wall_pair(&map, 0);
    let (v_wall, _) = wall_pair(&map, 1);
    let v = resolve_and_measure(&dimension(DimensionKind::Angle, &[u_wall, v_wall]), &map);
    assert!(
        (v.to_degrees() - 90.0).abs() < 1e-9,
        "measured {} degrees",
        v.to_degrees()
    );
}

/// A Ø16 mm cylinder, 12 mm tall, axis along +Z.
const CYL_R: f64 = 0.008;
const CYL_H: f64 = 0.012;

#[test]
fn a_radial_dimension_on_a_cylinder_rim_in_a_top_view_measures_the_radius() {
    let mut builder = cylinder();
    let map = pid_to_curve(&mut builder);

    // A rim seen down its own axis stays an analytic circle — the projection
    // must not have handed back a chord polyline, because a polyline has no
    // radius at all and the dimension would refuse.
    let rims: Vec<(u64, f64)> = map
        .iter()
        .filter_map(|(pid, curve)| match curve {
            LayoutCurve::Circle { radius, .. } => Some((*pid, *radius)),
            _ => None,
        })
        .collect();
    assert!(
        !rims.is_empty(),
        "a top view of a cylinder projects its rims as circles; got {:?}",
        map.values().collect::<Vec<_>>()
    );

    for (pid, _) in &rims {
        let r = resolve_and_measure(&dimension(DimensionKind::Radius, &[*pid]), &map);
        assert!((r - CYL_R).abs() < 1e-12, "radius measured {r}, is {CYL_R}");
        let d = resolve_and_measure(&dimension(DimensionKind::Diameter, &[*pid]), &map);
        assert!(
            (d - 2.0 * CYL_R).abs() < 1e-12,
            "diameter measured {d}, is {}",
            2.0 * CYL_R
        );
    }
}

/// A cylinder, for the two radial tests.
fn cylinder() -> ModelBuilder {
    let mut builder = ModelBuilder::kernel_v2();
    builder
        .true_circle_sketch("s", [0.0, 0.0, 0.0], [0.0, 0.0, 1.0], 0.0, 0.0, CYL_R)
        .expect("sketch");
    builder.extrude("cyl", "s", CYL_H).expect("extrude");
    builder
}

#[test]
fn a_radial_dimension_on_an_obliquely_seen_rim_reads_the_true_radius_not_the_foreshortened_one() {
    // `LayoutCurve::radius` returns an ELLIPSE's major radius and claims that
    // is the hole's true radius. That is a property of orthographic
    // projection — the circle's diameter along the line of nodes is
    // unforeshortened, so it survives as the major axis — and until now it was
    // asserted on a hand-built ellipse in waffle-types. This is the real
    // thing: a tilted view of a real cylinder, where the minor radius is
    // visibly wrong and the dimension must not read it.
    //
    // Looking along (0, −1, −1): 45° off the rim's own +Z normal, so the
    // foreshortening factor is cos 45° = 1/√2 and the minor radius is
    // CYL_R/√2 ≈ 5.657 mm against a true 8 mm.
    let mut builder = cylinder();
    let view = ViewFrame::looking_along([0.0, -1.0, -1.0]);
    let map = pid_to_curve_in(&mut builder, &view);

    let ellipses: Vec<(u64, f64, f64)> = map
        .iter()
        .filter_map(|(pid, curve)| match curve {
            LayoutCurve::Ellipse {
                major_radius,
                minor_radius,
                ..
            } => Some((*pid, *major_radius, *minor_radius)),
            _ => None,
        })
        .collect();
    assert!(
        !ellipses.is_empty(),
        "a tilted view of a cylinder projects its rims as analytic ellipses; got {:?}",
        map.values().collect::<Vec<_>>()
    );

    let foreshortened = CYL_R / 2.0_f64.sqrt();
    for (pid, major, minor) in &ellipses {
        // The fixture is only honest if the rim really is foreshortened: a
        // circular projection would make the assertion below vacuous.
        assert!(
            (minor - foreshortened).abs() < 1e-9,
            "the 45° view should foreshorten the rim to {foreshortened}, got {minor}"
        );
        assert!(
            (major - CYL_R).abs() < 1e-9,
            "the major radius should survive at {CYL_R}, got {major}"
        );

        let r = resolve_and_measure(&dimension(DimensionKind::Radius, &[*pid]), &map);
        assert!(
            (r - CYL_R).abs() < 1e-12,
            "radius measured {r}, is {CYL_R} (the foreshortened {foreshortened} is the wrong answer)"
        );
        let d = resolve_and_measure(&dimension(DimensionKind::Diameter, &[*pid]), &map);
        assert!(
            (d - 2.0 * CYL_R).abs() < 1e-12,
            "diameter measured {d}, is {}",
            2.0 * CYL_R
        );
    }
}

#[test]
fn an_anchor_whose_pid_is_gone_does_not_resolve_to_a_neighbour() {
    // D0's `Selector::Pid` never rebinds, and this is the consequence D3
    // depends on: a dimension whose entity is absent has no geometry at all,
    // so it cannot measure a different edge and print a plausible number.
    let mut builder = plate();
    let map = pid_to_curve(&mut builder);
    assert!(!map.is_empty());
    let absent = map.keys().max().unwrap() + 1_000_000;
    assert!(
        !map.contains_key(&absent),
        "the fixture must not happen to own the pid under test"
    );

    // One good anchor, one absent one: the dimension refuses, naming the pid.
    // It does NOT fall back to the nearest edge and return a number — which
    // on this plate would have been a perfectly plausible 25 or 40 mm.
    let (good, _) = wall_pair(&map, 0);
    let err = try_resolve_and_measure(&dimension(DimensionKind::HDistance, &[good, absent]), &map)
        .unwrap_err();
    assert!(
        err.contains(&absent.to_string()) && err.contains("no projected curve"),
        "{err}"
    );
}
