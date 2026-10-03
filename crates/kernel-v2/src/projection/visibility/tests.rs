//! D1c unit pins — `specs/drawings_and_mbd.md` §5.2 increment 3 and the §5.3
//! visibility oracle's per-primitive half.
//!
//! The corpus-wide half (a software orthographic depth buffer over the assay
//! documents) is `test-harness/tests/projection_visibility_oracle.rs`. What
//! lives here is the set of configurations whose answer is known in CLOSED
//! FORM, so the test states a number rather than comparing two
//! implementations:
//!
//! - a box from a generic direction: twelve edges, nine visible, three hidden,
//!   and the three are exactly the ones at the far vertex;
//! - a cylinder seen obliquely: the near rim whole, the far rim split in two
//!   equal halves by its own silhouette rulings;
//! - a plate with a through hole: the two rims coincide in `(u, v)` and must
//!   NOT merge, because one is visible and the other hidden;
//! - a frustum seen edge-on: the rims' two halves project onto one segment and
//!   the NEAR one is what the drawing shows, and the seam ruling and the
//!   silhouette ruling coincide and DO merge.

use std::f64::consts::{PI, TAU};

use cad_primitives::Point3;
use waffle_types::kernel::projection::{
    Curve2, CurveKind, KernelProjection, ProjectOpts, ProjectedCurve, ProjectionDeclines,
    ViewFrame, ViewGeometry, Visibility,
};
use waffle_types::kernel::{Kernel, KernelId, KernelSolidHandle};

use crate::arena::UnitVector3;
use crate::cone_fixtures::build_frustum;
use crate::projection::tests::{make_box, make_cylinder};
use crate::KernelV2Adapter;

// ---------------------------------------------------------------------------
// helpers
// ---------------------------------------------------------------------------

fn view(a: &KernelV2Adapter, solid: &KernelSolidHandle, dir: [f64; 3]) -> ViewGeometry {
    a.project(
        solid,
        &ViewFrame::looking_along(dir),
        &ProjectOpts::default(),
    )
    .expect("project")
}

fn of_kind(v: &ViewGeometry, kind: CurveKind) -> Vec<&ProjectedCurve> {
    v.curves.iter().filter(|c| c.kind == kind).collect()
}

fn count(v: &ViewGeometry, kind: CurveKind, vis: Visibility) -> usize {
    v.curves
        .iter()
        .filter(|c| c.kind == kind && c.visibility == vis)
        .count()
}

/// Total length of the curves of one source with one visibility.
fn length_of(v: &ViewGeometry, source: KernelId, vis: Visibility) -> f64 {
    v.curves
        .iter()
        .filter(|c| c.source == Some(source) && c.visibility == vis)
        .map(|c| c.geometry.length())
        .sum()
}

/// Every distinct `source` among the curves of one kind, in first-seen order.
fn sources(v: &ViewGeometry, kind: CurveKind) -> Vec<KernelId> {
    let mut out: Vec<KernelId> = Vec::new();
    for c in v.curves.iter().filter(|c| c.kind == kind) {
        if let Some(s) = c.source {
            if !out.contains(&s) {
                out.push(s);
            }
        }
    }
    out
}

fn close(a: f64, b: f64, rel: f64) -> bool {
    (a - b).abs() <= rel * (1.0 + a.abs().max(b.abs()))
}

// ---------------------------------------------------------------------------
// the box: the pin §5.2 increment 3 is written against
// ---------------------------------------------------------------------------

/// A convex box seen from a direction that aligns with nothing: each of its
/// twelve edges is wholly visible or wholly hidden, and exactly the three at
/// the FAR vertex are hidden.
///
/// This is three properties in one number. The crossing search cuts the three
/// near-vertex edges against the three far ones (they do cross in `(u, v)`),
/// so getting twelve curves back requires the MERGE to rejoin every piece;
/// getting nine and three requires the ray cast to be right about a convex
/// solid; and the far vertex's identity requires the depth to be signed the
/// way [`waffle_types::kernel::projection::ViewBasis::project`] says.
#[test]
fn a_box_from_a_generic_direction_has_nine_visible_and_three_hidden_edges() {
    let mut a = KernelV2Adapter::new();
    let (w, d, h) = (0.040, 0.030, 0.010);
    let solid = make_box(&mut a, w, d, h);
    // The viewer sits beyond `(w, d, h)`, so the near vertex is that corner
    // and the far one is the origin. Deliberately incommensurate so no two
    // edges project onto each other.
    let dir = [-2.0, -3.0, -5.0];
    let v = view(&a, &solid, dir);

    assert_eq!(
        v.curves.len(),
        12,
        "a box has twelve edges and a generic view merges each back to one \
         curve, got {} ({:?})",
        v.curves.len(),
        v.curves
            .iter()
            .map(|c| (c.kind, c.visibility))
            .collect::<Vec<_>>()
    );
    assert_eq!(count(&v, CurveKind::Edge, Visibility::Visible), 9);
    assert_eq!(count(&v, CurveKind::Edge, Visibility::Hidden), 3);
    assert_eq!(
        of_kind(&v, CurveKind::Silhouette).len(),
        0,
        "a box has no curved face"
    );
    assert_eq!(
        v.declines.total(),
        0,
        "nothing to decline: {:?}",
        v.declines
    );

    // The three hidden edges are the three at the far vertex, which is the
    // origin: each hidden curve must have an endpoint there.
    let basis = ViewFrame::looking_along(dir).basis().expect("basis");
    let (far_uv, far_depth) = basis.project([0.0, 0.0, 0.0]);
    for c in v
        .curves
        .iter()
        .filter(|c| c.visibility == Visibility::Hidden)
    {
        let (p, q) = c.geometry.endpoints().expect("a box edge is a segment");
        let touches = [p, q]
            .iter()
            .any(|e| (e.x() - far_uv.x()).hypot(e.y() - far_uv.y()) < 1e-12);
        assert!(touches, "hidden edge {c:?} does not reach the far vertex");
        // And a hidden curve knows what hides it: an occluder strictly nearer
        // than itself.
        let depth = c.depth.expect("a classified curve carries its depth");
        let occluder = depth.occluder.expect("a hidden curve names its occluder");
        assert!(
            occluder < depth.at_midpoint,
            "the occluder at {occluder} is not in front of {} ",
            depth.at_midpoint
        );
    }
    // And every visible curve has a depth with no occluder.
    for c in v
        .curves
        .iter()
        .filter(|c| c.visibility == Visibility::Visible)
    {
        let depth = c.depth.expect("a classified curve carries its depth");
        assert!(
            depth.occluder.is_none(),
            "a visible curve must name no occluder, got {depth:?}"
        );
        assert!(depth.at_midpoint >= far_depth - 1e-12 || depth.at_midpoint.is_finite());
    }
}

/// The same box with the viewer on the other side: the SAME twelve edges, the
/// same nine/three split, and a disjoint set of hidden edges — the three at
/// what is now the far vertex.
#[test]
fn reversing_the_view_direction_hides_the_other_three_edges() {
    let mut a = KernelV2Adapter::new();
    let solid = make_box(&mut a, 0.040, 0.030, 0.010);
    let front = view(&a, &solid, [-2.0, -3.0, -5.0]);
    let back = view(&a, &solid, [2.0, 3.0, 5.0]);
    for v in [&front, &back] {
        assert_eq!(v.curves.len(), 12);
        assert_eq!(count(v, CurveKind::Edge, Visibility::Hidden), 3);
    }
    let hidden = |v: &ViewGeometry| -> Vec<KernelId> {
        let mut s: Vec<KernelId> = v
            .curves
            .iter()
            .filter(|c| c.visibility == Visibility::Hidden)
            .filter_map(|c| c.source)
            .collect();
        s.sort_by_key(|k| k.0);
        s
    };
    let (f, b) = (hidden(&front), hidden(&back));
    assert_eq!(f.len(), 3);
    assert_eq!(b.len(), 3);
    assert!(
        f.iter().all(|e| !b.contains(e)),
        "the two views hide the same edge: {f:?} vs {b:?}"
    );
}

// ---------------------------------------------------------------------------
// the cylinder: a split that is exactly half
// ---------------------------------------------------------------------------

/// A cylinder along `z` seen obliquely from above: the NEAR rim is whole and
/// visible, the FAR rim is cut by its own two silhouette rulings into two
/// equal halves — one visible, one hidden — and the rulings themselves are
/// visible.
///
/// Equal halves is the sharp part. The silhouette rulings touch a rim's
/// projected ellipse at its MAJOR-axis ends, so the split parameters are
/// exactly the two ends of the major axis and the two arcs are congruent. A
/// classification that split anywhere else would not give the same two
/// lengths.
#[test]
fn a_cylinder_seen_obliquely_splits_its_far_rim_into_two_equal_halves() {
    let mut a = KernelV2Adapter::new();
    let (radius, height) = (0.008, 0.020);
    let solid = make_cylinder(&mut a, (0.0, 0.0), radius, 0.0, height);
    // Looking down and along −y: the viewer is above and at +y, so the TOP rim
    // (z = height) is the near one.
    let v = view(&a, &solid, [0.0, -1.0, -0.4]);

    // The rims are the sources whose curves are CONICS; the third edge is the
    // seam, a straight segment joining them, and length alone does not tell
    // the two apart (a 20 mm seam is longer than an 8 mm radius).
    let rims: Vec<KernelId> = sources(&v, CurveKind::Edge)
        .into_iter()
        .filter(|s| {
            v.curves.iter().any(|c| {
                c.source == Some(*s)
                    && matches!(c.geometry, Curve2::Circle { .. } | Curve2::Ellipse { .. })
            })
        })
        .collect();
    assert_eq!(rims.len(), 2, "two rims among the edges, got {rims:?}");
    let mut whole_rims = 0;
    let mut split_rims = 0;
    for &s in &rims {
        let vis = length_of(&v, s, Visibility::Visible);
        let hid = length_of(&v, s, Visibility::Hidden);
        if hid == 0.0 {
            whole_rims += 1;
        } else {
            split_rims += 1;
            assert!(
                close(vis, hid, 1e-6),
                "the far rim's visible {vis} and hidden {hid} halves are not equal"
            );
        }
    }
    assert_eq!(
        (whole_rims, split_rims),
        (1, 1),
        "one rim whole and one split in half"
    );

    // The cylinder's outline carries two verticals at `u = ±R`, both visible
    // and neither split. Only ONE of them is reported as a silhouette: this
    // fixture's seam edge runs along the other, so the two coincide with the
    // same visibility and the merge keeps the EDGE. A drawing wants one line
    // there either way, and both land on the same DXF layer.
    let basis = ViewFrame::looking_along([0.0, -1.0, -0.4])
        .basis()
        .expect("basis");
    // A ruling is parallel to the axis, so its projected length is the height
    // times the axis's own foreshortening.
    let fore = {
        let [du, dv] = basis.project_dir([0.0, 0.0, 1.0]);
        du.hypot(dv)
    };
    let want = height * fore;
    let verticals: Vec<&ProjectedCurve> = v
        .curves
        .iter()
        .filter(|c| matches!(c.geometry, Curve2::Line { .. }))
        .filter(|c| close(c.geometry.length(), want, 1e-9))
        .collect();
    assert_eq!(
        verticals.len(),
        2,
        "the outline has two full-height verticals of {want}, got {:?}",
        v.curves
            .iter()
            .map(|c| (c.kind, c.visibility, c.geometry.length()))
            .collect::<Vec<_>>()
    );
    for c in &verticals {
        assert_eq!(c.visibility, Visibility::Visible);
    }
    assert_eq!(
        of_kind(&v, CurveKind::Silhouette).len(),
        1,
        "the ruling along the seam edge merges away"
    );
}

/// Seen along its own axis a cylinder has no silhouette (D1b) and its two rims
/// project onto the SAME circle — the coincidence the merge exists for.
///
/// Both rims come out VISIBLE, and that is the right answer rather than a
/// grazing accident: the far rim lies exactly on the bore's own outline, so
/// nothing stands BETWEEN it and the viewer — the ray from it runs along the
/// cylinder's wall, crossing no face's plane — and a draughtsman draws one
/// circle here, not a circle with a dashed twin under it. So the two agree on
/// visibility and the merge collapses them, and the view carries ONE circle of
/// the cylinder's radius. The `ray_coplanar` count is what says the
/// configuration was the grazing one.
#[test]
fn a_cylinder_seen_along_its_axis_merges_its_two_coincident_rims() {
    let mut a = KernelV2Adapter::new();
    let (radius, height) = (0.008, 0.020);
    let solid = make_cylinder(&mut a, (0.0, 0.0), radius, 0.0, height);
    let v = view(&a, &solid, [0.0, 0.0, -1.0]);

    let circles: Vec<&ProjectedCurve> = v
        .curves
        .iter()
        .filter(|c| {
            matches!(c.geometry, Curve2::Circle { radius: r, .. } if close(r, radius, 1e-9))
                && c.geometry.is_closed()
        })
        .collect();
    assert_eq!(
        circles.len(),
        1,
        "the two coincident rims merge into one circle, got {circles:?}"
    );
    assert_eq!(circles[0].visibility, Visibility::Visible);
    // The survivor is the NEAR rim: `w = -z`, so depth is `-z` and the top rim
    // at `z = height` is at `-height`.
    assert!(
        close(circles[0].depth.expect("depth").at_midpoint, -height, 1e-9),
        "the kept rim must be the near one, got {:?}",
        circles[0].depth
    );
    assert_eq!(of_kind(&v, CurveKind::Silhouette).len(), 0);
    assert!(
        v.declines.ray_grazes_face > 0,
        "the grazing configuration must be counted: {:?}",
        v.declines
    );
}

// ---------------------------------------------------------------------------
// a through hole: the far rim is hidden
// ---------------------------------------------------------------------------

/// A block with a drilled through hole, seen OBLIQUELY: the hole's near rim is
/// whole and visible, its FAR rim is wholly hidden behind the block's
/// material.
///
/// Oblique on purpose, and the geometry chosen so the answer is not a
/// judgement call. Seen along the hole's own axis the far rim sits exactly on
/// the bore's outline with nothing between it and the viewer, so it is grazing
/// rather than hidden (that is the
/// `a_cylinder_seen_along_its_axis_merges_its_two_coincident_rims` case). Tilt
/// the view past `atan(2R/T)` and the tunnel is blocked along every line of
/// sight: with `R = 3 mm` through `T = 30 mm` that is 11.3 degrees, and this
/// view is at 26.6, so the far rim is hidden along its whole length and no
/// part of it is a near miss.
#[test]
fn a_through_holes_far_rim_is_hidden_behind_the_block() {
    let mut a = KernelV2Adapter::new();
    let (thickness, hole_r) = (0.030, 0.003);
    let block = make_box(&mut a, 0.040, 0.030, thickness);
    let drill = make_cylinder(&mut a, (0.020, 0.015), hole_r, -0.005, thickness + 0.010);
    let holed = a
        .boolean_subtract(&block, &drill)
        .expect("block minus a through hole");
    let v = view(&a, &holed, [0.0, -0.5, -1.0]);

    // A circle's projection keeps the circle's radius as its MAJOR semi-axis,
    // so the rims are the sources carrying a conic of the hole's radius.
    let is_rim = |c: &ProjectedCurve| match c.geometry {
        Curve2::Circle { radius, .. } => close(radius, hole_r, 1e-6),
        Curve2::Ellipse { major_radius, .. } => close(major_radius, hole_r, 1e-6),
        _ => false,
    };
    let mut rims: Vec<KernelId> = Vec::new();
    for c in v.curves.iter().filter(|c| is_rim(c)) {
        if let Some(s) = c.source {
            if !rims.contains(&s) {
                rims.push(s);
            }
        }
    }
    assert_eq!(rims.len(), 2, "the hole has two rims, got {rims:?}");

    let mut measured: Vec<(f64, bool)> = Vec::new();
    for &s in &rims {
        let vis = length_of(&v, s, Visibility::Visible);
        let hid = length_of(&v, s, Visibility::Hidden);
        assert!(vis + hid > 4.0 * hole_r, "a rim is a closed curve");
        assert!(
            vis == 0.0 || hid == 0.0,
            "rim {s:?} came back part visible ({vis}) and part hidden ({hid})"
        );
        let depth = v
            .curves
            .iter()
            .filter(|c| c.source == Some(s))
            .filter_map(|c| c.depth.map(|d| d.at_midpoint))
            .fold(f64::NEG_INFINITY, f64::max);
        measured.push((depth, vis == 0.0));
    }
    measured.sort_by(|a, b| a.0.total_cmp(&b.0));
    assert!(
        !measured[0].1 && measured[1].1,
        "the nearer rim must be the visible one: {measured:?}"
    );
    for c in v
        .curves
        .iter()
        .filter(|c| is_rim(c) && c.visibility == Visibility::Hidden)
    {
        let d = c.depth.expect("depth");
        let occ = d.occluder.expect("a hidden rim names its occluder");
        assert!(occ < d.at_midpoint, "occluder {occ} vs {}", d.at_midpoint);
    }
}

// ---------------------------------------------------------------------------
// the cone: the edge-on fold, and a coincidence that DOES merge
// ---------------------------------------------------------------------------

/// A frustum seen perpendicular to its axis.
///
/// Two things are pinned, and both are about a projection that is not
/// injective. Each rim circle is seen EDGE ON, so its near and far halves
/// project onto the same segment — and the drawing shows the near one, which
/// is what lifting a 2-D point to its NEAREST pre-image produces, so the rim
/// comes out whole and visible rather than half-dashed. And the lateral face's
/// SEAM edge sits at `+x`, exactly where one of the two silhouette rulings
/// runs, so the two are coincident with the same visibility and the merge
/// drops one — keeping the EDGE, since the edge pass runs first.
#[test]
fn a_frustum_seen_edge_on_shows_four_curves_and_merges_the_seam_ruling() {
    let apex = Point3::new(0.0, 0.0, 0.0);
    let axis = UnitVector3 {
        x: 0.0,
        y: 0.0,
        z: 1.0,
    };
    let (tau0, tau1, half) = (0.010, 0.030, 0.4);
    let (arena, solid, _) = build_frustum(apex, axis, tau0, tau1, half, half);
    let basis = ViewFrame::looking_along([0.0, 1.0, 0.0])
        .basis()
        .expect("basis");
    let v = crate::projection::project_solid(
        &arena,
        solid,
        &basis,
        crate::tessellate::RENDER_CHORD_TOLERANCE_REL,
    )
    .expect("project");

    assert_eq!(
        count(&v, CurveKind::Edge, Visibility::Hidden)
            + count(&v, CurveKind::Silhouette, Visibility::Hidden),
        0,
        "everything a frustum shows edge-on is on its outline: {:?}",
        v.curves
            .iter()
            .map(|c| (c.kind, c.visibility, c.geometry.length()))
            .collect::<Vec<_>>()
    );
    assert_eq!(
        of_kind(&v, CurveKind::Silhouette).len(),
        1,
        "one of the two rulings coincides with the seam edge and merges away"
    );
    // The two rims, edge-on, as segments of 2R each.
    let tan = half.tan();
    for (tau, want) in [(tau0, 2.0 * tau0 * tan), (tau1, 2.0 * tau1 * tan)] {
        let found = v
            .curves
            .iter()
            .filter(|c| c.kind == CurveKind::Edge)
            .filter(|c| close(c.geometry.length(), want, 1e-9))
            .count();
        assert_eq!(
            found, 1,
            "the rim at tau {tau} projects to one segment of {want}"
        );
    }
    // Every curve here runs along a face parallel to the line of sight -- the
    // two discs contain the rims, the lateral face contains the rulings -- so
    // each verdict rests on the separation argument rather than on a hit, and
    // the count says so. Every other decline must stay zero.
    assert!(v.declines.ray_grazes_face > 0, "{:?}", v.declines);
    assert_eq!(
        v.declines.total() - v.declines.ray_grazes_face,
        0,
        "{:?}",
        v.declines
    );
}

/// A box with a `w` x `d` x `h` corner at `(x0, y0, z0)`.
fn make_box_at(
    a: &mut KernelV2Adapter,
    (x0, y0, z0): (f64, f64, f64),
    (w, d, h): (f64, f64, f64),
) -> KernelSolidHandle {
    let mut positions = std::collections::HashMap::new();
    positions.insert(1, (x0, y0));
    positions.insert(2, (x0 + w, y0));
    positions.insert(3, (x0 + w, y0 + d));
    positions.insert(4, (x0, y0 + d));
    let profile = waffle_types::kernel::ClosedProfile {
        entity_ids: vec![1, 2, 3, 4],
        is_outer: true,
        vertex_ids: vec![],
        circle: None,
        spline_segments: vec![],
        arc_segments: vec![],
    };
    let faces = a
        .make_faces_from_profiles(
            &[profile],
            [0.0, 0.0, z0],
            [0.0, 0.0, 1.0],
            [1.0, 0.0, 0.0],
            &positions,
        )
        .expect("rectangle stages");
    a.extrude_face(faces[0], [0.0, 0.0, 1.0], h).expect("box")
}

/// A blind slot cut into the side of a box, seen from ABOVE: everything the
/// slot exposes is buried under the box's own top face and must come back
/// hidden.
///
/// This is corpus case C0009's shape, and it is the pin for the RE-CAST rule.
/// There the slot's blind-end edge runs along the occluding face's own symmetry
/// line, which is where its CDT put a triangulation seam — so the ray from the
/// piece's midpoint passed exactly through that seam, every incident triangle
/// reported a boundary touch, and a face standing a fifth of the solid in front
/// of the edge was taken for a graze. Nineteen of the forty-two cases the §5.3
/// visibility sweep covers failed on it, and the stride-48 sample of seven had
/// shown none of it.
///
/// What this fixture pins is the OUTCOME, which is right whatever the
/// tessellator's seams do: the slot's own edges are hidden, their occluder is
/// the top face, and the view declines nothing. The seam COINCIDENCE is pinned
/// by the corpus oracle instead — producing one on purpose means pinning the
/// triangulator's internal choices, which is not this test's business.
#[test]
fn a_blind_slots_edges_are_hidden_under_the_box_it_is_cut_into() {
    let mut a = KernelV2Adapter::new();
    let side = 0.020;
    let box_ = make_box_at(&mut a, (0.0, 0.0, 0.0), (side, side, side));
    // Entering the `+x` face and stopping inside, centred on the box's own
    // mid-plane in `y`, which is where a CDT seam is likeliest.
    let cutter = make_box_at(&mut a, (0.0025, 0.005, 0.003), (0.0275, 0.010, 0.005));
    let slotted = a
        .boolean_subtract(&box_, &cutter)
        .expect("box minus a blind slot");
    let v = view(&a, &slotted, [0.0, 0.0, -1.0]);

    // The slot's BLIND END face stands at `x = 0.0025`, so its two horizontal
    // edges project to segments at `u = 0.0025`, at depths `−0.008` (the roof)
    // and `−0.003` (the floor). Both are deep inside the box's footprint with
    // the top face over them, which is the configuration under test — the
    // slot's MOUTH edges on the `+x` face are deliberately not included, since
    // that face is parallel to the line of sight and grazing is the right
    // answer there.
    // The slot's floor edges are coincident in (u, v) with its roof edges and
    // agree on visibility, so the merge collapses the pairs: what the view
    // carries at the roof's depth is the slot's three interior edges plus its
    // MOUTH edge on the `+x` face.
    let at_roof: Vec<&ProjectedCurve> = v
        .curves
        .iter()
        .filter(|c| {
            c.depth
                .is_some_and(|dd| close(dd.at_midpoint, -0.008, 1e-9))
        })
        .collect();
    let on_outline = |c: &ProjectedCurve| match c.geometry {
        Curve2::Line { start, end } => close(start.x(), side, 1e-9) && close(end.x(), side, 1e-9),
        _ => false,
    };
    let interior: Vec<&&ProjectedCurve> = at_roof.iter().filter(|c| !on_outline(c)).collect();
    assert_eq!(
        interior.len(),
        3,
        "the slot's three interior edges must be in the view: {:?}",
        v.curves
            .iter()
            .map(|c| (c.visibility, c.depth, c.geometry.clone()))
            .collect::<Vec<_>>()
    );
    for c in &interior {
        assert_eq!(
            c.visibility,
            Visibility::Hidden,
            "an edge {:?} under the box's own top face is hidden",
            c.geometry
        );
        let dd = c.depth.expect("depth");
        let occ = dd.occluder.expect("a hidden edge names its occluder");
        assert!(
            close(occ, -side, 1e-6),
            "the occluder must be the top face at {}, got {occ}",
            -side
        );
    }
    // And the mouth edge, which lies IN the `+x` face the line of sight runs
    // along, is the grazing case: nothing stands between it and the viewer, so
    // it stays visible and the view says out loud that it grazed.
    let mouth: Vec<&&ProjectedCurve> = at_roof.iter().filter(|c| on_outline(c)).collect();
    assert_eq!(mouth.len(), 1);
    assert_eq!(mouth[0].visibility, Visibility::Visible);
    assert!(
        v.declines.ray_grazes_face > 0,
        "the mouth edge grazes at every point of itself: {:?}",
        v.declines
    );
    assert_eq!(
        v.declines.total() - v.declines.ray_grazes_face,
        0,
        "{:?}",
        v.declines
    );
}

// ---------------------------------------------------------------------------
// the splitting primitive's own properties
// ---------------------------------------------------------------------------

/// A split piece keeps its parent's analytic KIND — the whole reason
/// [`Curve2::subcurve`] exists rather than a resampling. A projected rim cut
/// in half by a silhouette must still be a circular or elliptic ARC, because
/// the DXF writer turns that into an `ARC` entity and a polyline into
/// hundreds of vertices.
#[test]
fn a_split_rim_is_still_an_arc_and_not_a_polyline() {
    let mut a = KernelV2Adapter::new();
    let solid = make_cylinder(&mut a, (0.0, 0.0), 0.008, 0.0, 0.020);
    let v = view(&a, &solid, [0.0, -1.0, -0.4]);
    let split: Vec<&ProjectedCurve> = v
        .curves
        .iter()
        .filter(|c| c.visibility == Visibility::Hidden)
        .collect();
    assert!(!split.is_empty(), "the far rim's hidden half must exist");
    for c in &split {
        assert!(
            matches!(c.geometry, Curve2::Ellipse { .. } | Curve2::Circle { .. }),
            "a cut rim must stay analytic, got {:?}",
            c.geometry
        );
        let (t0, t1) = c.geometry.param_range().expect("a range");
        assert!(
            t1 - t0 < TAU - 1e-9 && t1 - t0 > 1e-9,
            "a half rim spans {} radians",
            t1 - t0
        );
        assert!(!c.geometry.is_closed(), "a cut rim is not closed");
    }
}

/// `classify` over nothing is nothing, and the declines of an empty view are
/// empty — the vacuous case, so a caller cannot read a clean report out of a
/// pass that did no work.
#[test]
fn classifying_no_curves_declines_nothing() {
    let apex = Point3::new(0.0, 0.0, 0.0);
    let axis = UnitVector3 {
        x: 0.0,
        y: 0.0,
        z: 1.0,
    };
    let (arena, solid, _) = build_frustum(apex, axis, 0.010, 0.030, 0.4, 0.4);
    let basis = ViewFrame::TOP.basis().expect("basis");
    let mut declines = ProjectionDeclines::default();
    let out =
        super::classify(&arena, solid, &basis, Vec::new(), 64, &mut declines).expect("classify");
    assert!(out.is_empty());
    assert_eq!(declines.total(), 0);
}

// ---------------------------------------------------------------------------
// the ray's own pieces
// ---------------------------------------------------------------------------

/// The float ray/triangle solve must report the grazing configurations as
/// grazing rather than guessing them, since that is what hands them to the
/// exact predicate.
#[test]
fn the_float_ray_test_declares_its_own_grazing_cases() {
    let (a, b, c) = ([0.0, 0.0, 0.0], [1.0, 0.0, 0.0], [0.0, 1.0, 0.0]);
    let margin = 1e-9;
    // Straight through the middle, well in front.
    assert!(matches!(
        super::ray_triangle([0.25, 0.25, -1.0], [0.0, 0.0, 1.0], a, b, c, margin),
        super::RayHit::At(t) if (t - 1.0).abs() < 1e-12
    ));
    // Behind the origin: a decisive miss.
    assert!(matches!(
        super::ray_triangle([0.25, 0.25, 1.0], [0.0, 0.0, 1.0], a, b, c, margin),
        super::RayHit::Miss
    ));
    // Beside the triangle: also decisive.
    assert!(matches!(
        super::ray_triangle([2.0, 2.0, -1.0], [0.0, 0.0, 1.0], a, b, c, margin),
        super::RayHit::Miss
    ));
    // Exactly through a vertex, and exactly along an edge: not decisive.
    assert!(matches!(
        super::ray_triangle([0.0, 0.0, -1.0], [0.0, 0.0, 1.0], a, b, c, margin),
        super::RayHit::Grazing
    ));
    assert!(matches!(
        super::ray_triangle([0.5, 0.0, -1.0], [0.0, 0.0, 1.0], a, b, c, margin),
        super::RayHit::Grazing
    ));
    // Parallel to the triangle's plane: not decisive either.
    assert!(matches!(
        super::ray_triangle([0.25, 0.25, 0.0], [1.0, 0.0, 0.0], a, b, c, margin),
        super::RayHit::Grazing
    ));
    // A degenerate triangle occludes nothing, loudly rather than by NaN.
    assert!(matches!(
        super::ray_triangle([0.25, 0.25, -1.0], [0.0, 0.0, 1.0], a, a, a, margin),
        super::RayHit::Miss
    ));
}

/// Lifting a 2-D point to the nearest pre-image, on the configuration that
/// makes "nearest" load-bearing: a circle seen edge-on, whose two halves land
/// on one segment.
#[test]
fn an_edge_on_circle_lifts_to_its_near_half() {
    let basis = ViewFrame::looking_along([0.0, 1.0, 0.0])
        .basis()
        .expect("basis");
    // A unit circle in the xy plane, sampled; `w = +y`, so depth IS y and the
    // near half is `y < 0`.
    let n = 64;
    let lift: Vec<Point3> = (0..=n)
        .map(|i| {
            let t = TAU * (i as f64) / (n as f64);
            Point3::new(t.cos(), t.sin(), 0.0)
        })
        .collect();
    // `u = +x`: the point `u = 0` has pre-images at `y = ±1`.
    let (depth, p3) = super::lift_point(&basis, &lift, cad_primitives::Point2::new(0.0, 0.0))
        .expect("a pre-image");
    assert!(
        depth < 0.0 && close(depth, -1.0, 1e-3),
        "the nearest pre-image of u = 0 is at y = −1, got {depth} at {p3:?}"
    );
    // And an ordinary injective point still lifts to itself.
    let (depth, _) = super::lift_point(
        &basis,
        &lift,
        cad_primitives::Point2::new((PI / 4.0).cos(), 0.0),
    )
    .expect("a pre-image");
    assert!(depth < 0.0, "u = cos(π/4) must lift to the near half");
}

/// A lift with nothing in it cannot answer, and says so rather than inventing
/// a depth.
#[test]
fn an_empty_lift_answers_none() {
    let basis = ViewFrame::TOP.basis().expect("basis");
    assert!(super::lift_point(&basis, &[], cad_primitives::Point2::new(0.0, 0.0)).is_none());
    let one = [Point3::new(1.0, 2.0, 3.0)];
    let (d, p) = super::lift_point(&basis, &one, cad_primitives::Point2::new(9.0, 9.0))
        .expect("a single point lifts to itself");
    assert_eq!(p, [1.0, 2.0, 3.0]);
    assert_eq!(d, -3.0, "the top view's depth is −z");
}
