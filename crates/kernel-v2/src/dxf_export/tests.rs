//! DXF writer tests: structural (the R12 skeleton, per-entity counts, and
//! the coordinates of a box and a cylinder in millimetres), and one byte-exact
//! golden file.
//!
//! There is deliberately no round-trip reader here: a DXF reader would be a
//! second implementation of the same group-code grammar, and agreeing with
//! itself proves nothing. The oracle that matters is upstream — the projection
//! tests check the CURVES against the solid's own 3-D samples — and what is
//! left for this layer is that the text says what the curves say, which
//! parsing the group codes back out answers directly.
//!
//! Regenerate the golden deliberately with
//! `UPDATE_GOLDEN=1 cargo test -p kernel-v2 --release --lib dxf_export`,
//! then read the diff.

use std::collections::HashMap;
use std::path::PathBuf;

use super::*;
use crate::KernelV2Adapter;
use waffle_types::kernel::projection::{KernelProjection, ProjectOpts, ProjectionBody, ViewFrame};
use waffle_types::kernel::{CircleProfile, ClosedProfile, Kernel, KernelError, KernelSolidHandle};

// --- fixtures (the step_export tests' shapes, in the same two sizes) ---

fn make_box(a: &mut KernelV2Adapter, w: f64, d: f64, h: f64) -> KernelSolidHandle {
    let mut positions = HashMap::new();
    positions.insert(1, (0.0, 0.0));
    positions.insert(2, (w, 0.0));
    positions.insert(3, (w, d));
    positions.insert(4, (0.0, d));
    let profile = ClosedProfile {
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
            [0.0, 0.0, 0.0],
            [0.0, 0.0, 1.0],
            [1.0, 0.0, 0.0],
            &positions,
        )
        .expect("rectangle stages");
    a.extrude_face(faces[0], [0.0, 0.0, 1.0], h).expect("box")
}

fn make_cylinder(
    a: &mut KernelV2Adapter,
    (cx, cy): (f64, f64),
    radius: f64,
    height: f64,
) -> KernelSolidHandle {
    let profile = ClosedProfile {
        entity_ids: vec![7],
        is_outer: true,
        vertex_ids: vec![],
        circle: Some(CircleProfile {
            center_u: cx,
            center_v: cy,
            radius,
        }),
        spline_segments: vec![],
        arc_segments: vec![],
    };
    let faces = a
        .make_faces_from_profiles(
            &[profile],
            [0.0, 0.0, 0.0],
            [0.0, 0.0, 1.0],
            [1.0, 0.0, 0.0],
            &HashMap::new(),
        )
        .expect("circle stages");
    a.extrude_face(faces[0], [0.0, 0.0, 1.0], height)
        .expect("cylinder")
}

// --- a minimal group-code reader, for assertions only ---

/// `(code, value)` pairs, in file order. DXF is a flat stream of them.
/// The D1a EDGE pass alone, as this writer's entity-coverage tests need it.
///
/// Since D1c the adapter's `project` merges curves that are COINCIDENT in
/// `(u, v)` with the same visibility — which an axis-aligned view of a
/// prismatic or axial solid is full of, since its front and back outlines
/// project onto each other — so a classified top view of a box carries four
/// lines, not eight. That is the right DRAWING and it is what the golden
/// records; it is the wrong fixture for checking that the writer emits a
/// `LINE` per `Curve2::Line` and a `CIRCLE` per full turn, which is what these
/// tests are for.
fn edge_view(a: &KernelV2Adapter, solid: &KernelSolidHandle, frame: ViewFrame) -> ViewGeometry {
    let (arena, sid) = a.arena_of(solid).expect("a live solid");
    crate::projection::project_edges(
        arena,
        sid,
        &frame.basis().expect("a well-formed frame"),
        crate::tessellate::RENDER_CHORD_TOLERANCE_REL,
    )
    .expect("the edge pass projects")
}

fn codes(text: &str) -> Vec<(i32, String)> {
    let mut lines = text.lines();
    let mut out = Vec::new();
    while let (Some(code), Some(value)) = (lines.next(), lines.next()) {
        out.push((
            code.trim().parse().unwrap_or_else(|e| {
                panic!("group code {code:?} is not an integer: {e}");
            }),
            value.trim().to_string(),
        ));
    }
    out
}

/// Every entity of the ENTITIES section as `(type, its own group codes)`.
fn entities(text: &str) -> Vec<(String, Vec<(i32, String)>)> {
    let all = codes(text);
    let start = all
        .iter()
        .position(|(c, v)| *c == 2 && v == "ENTITIES")
        .expect("an ENTITIES section");
    let mut out: Vec<(String, Vec<(i32, String)>)> = Vec::new();
    for (c, v) in &all[start + 1..] {
        if *c == 0 {
            if v == "ENDSEC" {
                break;
            }
            out.push((v.clone(), Vec::new()));
        } else if let Some(last) = out.last_mut() {
            last.1.push((*c, v.clone()));
        }
    }
    out
}

fn count_of(text: &str, kind: &str) -> usize {
    entities(text).iter().filter(|(k, _)| k == kind).count()
}

fn group(e: &[(i32, String)], code: i32) -> f64 {
    e.iter()
        .find(|(c, _)| *c == code)
        .map(|(_, v)| v.parse::<f64>().expect("a real"))
        .unwrap_or_else(|| panic!("no group {code} in {e:?}"))
}

fn layer_of(e: &[(i32, String)]) -> String {
    e.iter()
        .find(|(c, _)| *c == 8)
        .map(|(_, v)| v.clone())
        .expect("every entity names a layer")
}

// --- structure ---

#[test]
fn the_r12_skeleton_is_present_and_balanced() {
    let mut a = KernelV2Adapter::new();
    let solid = make_box(&mut a, 0.040, 0.030, 0.010);
    let view = a
        .project(&solid, &ViewFrame::TOP, &ProjectOpts::default())
        .expect("projects");
    let text = write_dxf(&view, DEFAULT_POLYLINE_SAGITTA);

    let all = codes(&text);
    assert_eq!(
        all.iter()
            .filter(|(c, v)| *c == 0 && v == "SECTION")
            .count(),
        3
    );
    assert_eq!(
        all.iter().filter(|(c, v)| *c == 0 && v == "ENDSEC").count(),
        3
    );
    assert!(text.ends_with("  0\nEOF\n"));
    // The version the entity choices are made against.
    let i = all
        .iter()
        .position(|(c, v)| *c == 9 && v == "$ACADVER")
        .expect("$ACADVER");
    assert_eq!(all[i + 1], (1, "AC1009".to_string()));

    // Both of §8's layers exist, whether or not anything is on them yet.
    let names: Vec<&str> = all
        .iter()
        .filter(|(c, _)| *c == 2)
        .map(|(_, v)| v.as_str())
        .collect();
    assert!(names.contains(&LAYER_VISIBLE) && names.contains(&LAYER_HIDDEN));

    // The extents are the view's own bounding box, in millimetres.
    let ext = |name: &str| -> (f64, f64) {
        let i = all
            .iter()
            .position(|(c, v)| *c == 9 && v == name)
            .unwrap_or_else(|| panic!("{name}"));
        (
            all[i + 1].1.parse().expect("real"),
            all[i + 2].1.parse().expect("real"),
        )
    };
    assert_eq!(ext("$EXTMIN"), (0.0, 0.0));
    assert_eq!(ext("$EXTMAX"), (40.0, 30.0));

    // Every line is either a code or a value: no stray text.
    assert_eq!(text.lines().count() % 2, 0);
}

#[test]
fn a_boxs_top_view_writes_eight_lines_and_four_points_in_millimetres() {
    let mut a = KernelV2Adapter::new();
    let solid = make_box(&mut a, 0.040, 0.030, 0.010);
    let view = edge_view(&a, &solid, ViewFrame::TOP);
    let text = write_dxf(&view, DEFAULT_POLYLINE_SAGITTA);

    assert_eq!(entities(&text).len(), 12, "twelve edges, twelve entities");
    assert_eq!(count_of(&text, "LINE"), 8);
    assert_eq!(count_of(&text, "POINT"), 4);
    assert_eq!(count_of(&text, "CIRCLE") + count_of(&text, "POLYLINE"), 0);

    // The two coincident rectangles: every LINE is a side of the 40×30 mm
    // footprint, and every endpoint is a corner of it.
    let corners = [(0.0, 0.0), (40.0, 0.0), (40.0, 30.0), (0.0, 30.0)];
    let is_corner = |x: f64, y: f64| {
        corners
            .iter()
            .any(|(cx, cy)| (x - cx).abs() < 1e-9 && (y - cy).abs() < 1e-9)
    };
    let mut lengths = Vec::new();
    for (kind, e) in entities(&text) {
        assert_eq!(
            layer_of(&e),
            LAYER_VISIBLE,
            "the edge pass tags everything visible"
        );
        match kind.as_str() {
            "LINE" => {
                let (x0, y0) = (group(&e, 10), group(&e, 20));
                let (x1, y1) = (group(&e, 11), group(&e, 21));
                assert_eq!(group(&e, 30), 0.0, "a drawing is flat");
                assert_eq!(group(&e, 31), 0.0);
                assert!(is_corner(x0, y0) && is_corner(x1, y1), "{e:?}");
                lengths.push((x1 - x0).hypot(y1 - y0));
            }
            "POINT" => {
                assert!(is_corner(group(&e, 10), group(&e, 20)), "{e:?}");
            }
            other => panic!("unexpected {other}"),
        }
    }
    lengths.sort_by(f64::total_cmp);
    assert_eq!(lengths.len(), 8);
    // Four 30 mm sides and four 40 mm sides, twice over (top and bottom face).
    for (i, l) in lengths.iter().enumerate() {
        let want = if i < 4 { 30.0 } else { 40.0 };
        assert!((l - want).abs() < 1e-9, "side {i} is {l}, wanted {want}");
    }
}

#[test]
fn a_cylinders_top_view_writes_two_circles_and_the_collapsed_seam() {
    let mut a = KernelV2Adapter::new();
    let solid = make_cylinder(&mut a, (0.005, -0.003), 0.008, 0.020);
    let view = edge_view(&a, &solid, ViewFrame::TOP);
    let text = write_dxf(&view, DEFAULT_POLYLINE_SAGITTA);

    assert_eq!(count_of(&text, "CIRCLE"), 2, "two rims, seen face-on");
    assert_eq!(count_of(&text, "POINT"), 1, "the seam runs into the page");
    assert_eq!(count_of(&text, "POLYLINE"), 0, "a circle is not flattened");
    assert_eq!(count_of(&text, "ARC"), 0, "a rim is a full turn");

    for (kind, e) in entities(&text) {
        if kind == "CIRCLE" {
            assert!((group(&e, 10) - 5.0).abs() < 1e-9, "centre x {e:?}");
            assert!((group(&e, 20) + 3.0).abs() < 1e-9, "centre y {e:?}");
            assert!((group(&e, 40) - 8.0).abs() < 1e-9, "radius {e:?}");
        }
    }
}

#[test]
fn an_oblique_rim_is_flattened_within_its_sagitta() {
    let mut a = KernelV2Adapter::new();
    let r = 0.008;
    let solid = make_cylinder(&mut a, (0.0, 0.0), r, 0.020);
    let theta = std::f64::consts::FRAC_PI_4;
    let view = edge_view(
        &a,
        &solid,
        ViewFrame {
            origin: [0.0; 3],
            dir: [0.0, theta.sin(), -theta.cos()],
            up: [0.0, 0.0, 1.0],
        },
    );
    let text = write_dxf(&view, DEFAULT_POLYLINE_SAGITTA);

    assert_eq!(count_of(&text, "POLYLINE"), 2, "R12 has no ELLIPSE entity");
    assert_eq!(count_of(&text, "CIRCLE") + count_of(&text, "ARC"), 0);
    for (kind, e) in entities(&text) {
        if kind != "POLYLINE" {
            continue;
        }
        assert_eq!(
            e.iter().find(|(c, _)| *c == 66).map(|(_, v)| v.as_str()),
            Some("1"),
            "R12 needs the vertices-follow flag"
        );
        assert_eq!(
            e.iter().find(|(c, _)| *c == 70).map(|(_, v)| v.as_str()),
            Some("1"),
            "a full rim is a CLOSED polyline"
        );
    }
    // One SEQEND per POLYLINE, and the vertices in between.
    assert_eq!(count_of(&text, "SEQEND"), 2);
    let verts = count_of(&text, "VERTEX");
    // The flattening bound: 2π·r / (2·acos(1 − s/r)) segments per rim.
    let step = 2.0 * (1.0 - DEFAULT_POLYLINE_SAGITTA / r).acos();
    let want = 2 * ((std::f64::consts::TAU / step).ceil() as usize + 1);
    assert_eq!(
        verts, want,
        "{verts} vertices, the sagitta bound wants {want}"
    );
}

/// A real part with partial arc edges writes real `ARC` entities, with the
/// angles the DXF convention wants.
///
/// The fixtures above are a box and a cylinder, whose rims are full turns, so
/// without this the writer's `ARC` branch is never reached from geometry.
#[test]
fn a_d_shaped_plate_writes_arcs_from_its_rounded_rims() {
    let mut a = KernelV2Adapter::new();
    let radius = 0.012;
    // A diameter chord plus a semicircle, extruded: each rim's arc is split
    // by the arena into sub-arcs under its minor-arc limit.
    let mut positions = HashMap::new();
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
    let solid = a
        .extrude_face(faces[0], [0.0, 0.0, 1.0], 0.004)
        .expect("D-shape extrudes");

    let text = write_dxf(
        &edge_view(&a, &solid, ViewFrame::TOP),
        DEFAULT_POLYLINE_SAGITTA,
    );
    let arcs: Vec<Vec<(i32, String)>> = entities(&text)
        .into_iter()
        .filter(|(k, _)| k == "ARC")
        .map(|(_, e)| e)
        .collect();
    assert!(!arcs.is_empty(), "a rounded rim writes ARCs");
    assert_eq!(
        count_of(&text, "POLYLINE"),
        0,
        "a rim seen face-on is exact"
    );
    let mut total = 0.0;
    for e in &arcs {
        assert!(
            (group(e, 40) - radius * 1000.0).abs() < 1e-9,
            "radius {e:?}"
        );
        for which in [50, 51] {
            let angle = group(e, which);
            assert!(
                (0.0..360.0).contains(&angle),
                "group {which} is {angle}, outside the DXF range"
            );
        }
        let (a0, a1) = (group(e, 50), group(e, 51));
        total += if a1 >= a0 { a1 - a0 } else { a1 + 360.0 - a0 };
    }
    // The sub-arcs of BOTH rims: two semicircles, 360° of sweep in all.
    assert!(
        (total - 360.0).abs() < 1e-6,
        "the arcs sweep {total}°, two semicircles are 360°"
    );
}

/// An `ARC`'s group 50/51 must be a DXF angle: degrees counter-clockwise from
/// the entity's `+x`, in `[0, 360)`.
///
/// `Curve2::Circle` carries whatever radian interval the projection produced —
/// `[angle0 − sweep, angle0]` for a projection that reverses the circle's
/// sense, so a negative start and, for a near-full arc, a start below −360 —
/// and R12 readers are not required to normalize. Nothing else in the suite
/// writes an `ARC` at all (the corpus's rims are full turns), so this is the
/// writer's only cover for the arc path.
#[test]
fn an_arc_is_written_with_degrees_in_the_dxf_range() {
    let cases = [
        // (start, end) in radians, as the projection normalizes them:
        // increasing, but anchored anywhere on the real line.
        (0.0, std::f64::consts::FRAC_PI_2),
        (-std::f64::consts::FRAC_PI_2, 0.0),
        (-6.0, -0.5),
        (2.9, 3.1),
        (-9.0, -3.0),
        (7.0, 9.0),
    ];
    for (s, e) in cases {
        let view = ViewGeometry::new(vec![
            waffle_types::kernel::projection::ProjectedCurve::visible(
                Curve2::Circle {
                    center: cad_primitives::Point2::new(0.001, -0.002),
                    radius: 0.004,
                    start_angle: s,
                    end_angle: e,
                },
                waffle_types::kernel::projection::CurveKind::Edge,
                None,
            ),
        ]);
        let text = write_dxf(&view, DEFAULT_POLYLINE_SAGITTA);
        assert_eq!(count_of(&text, "ARC"), 1, "({s}, {e}) is a partial arc");
        let arc = entities(&text).remove(0).1;
        let (a0, a1) = (group(&arc, 50), group(&arc, 51));
        for (which, a) in [("50", a0), ("51", a1)] {
            assert!(
                (0.0..360.0).contains(&a),
                "({s}, {e}): group {which} is {a}, outside [0, 360)"
            );
        }
        // And the arc still sweeps the same way: CCW from 50 to 51, wrapping
        // through 0, covers exactly the original interval.
        let want = (e - s).to_degrees();
        let got = if a1 >= a0 { a1 - a0 } else { a1 + 360.0 - a0 };
        assert!(
            (got - want).abs() < 1e-6,
            "({s}, {e}): the written arc sweeps {got}°, the curve sweeps {want}°"
        );
    }
}

#[test]
fn an_empty_view_is_still_a_valid_file() {
    let text = write_dxf(&ViewGeometry::default(), DEFAULT_POLYLINE_SAGITTA);
    assert!(entities(&text).is_empty());
    assert!(text.ends_with("  0\nEOF\n"));
    assert!(text.contains("$EXTMIN"));
}

// --- the trait method ---

#[test]
fn export_dxf_writes_the_same_text_as_projecting_then_writing() {
    let mut a = KernelV2Adapter::new();
    let solid = make_box(&mut a, 0.040, 0.030, 0.010);
    let bodies = [ProjectionBody::solo(solid.clone())];
    let direct = a
        .export_dxf(&bodies, &ViewFrame::TOP, &ProjectOpts::default())
        .expect("export_dxf");
    let view = a
        .project(&solid, &ViewFrame::TOP, &ProjectOpts::default())
        .expect("projects");
    assert_eq!(direct, write_dxf(&view, DEFAULT_POLYLINE_SAGITTA));
}

#[test]
fn export_dxf_puts_several_bodies_in_one_view() {
    let mut a = KernelV2Adapter::new();
    let plate = make_box(&mut a, 0.040, 0.030, 0.010);
    let disc = make_cylinder(&mut a, (0.060, 0.015), 0.008, 0.010);
    let text = a
        .export_dxf(
            &[ProjectionBody::solo(plate), ProjectionBody::solo(disc)],
            &ViewFrame::TOP,
            &ProjectOpts::default(),
        )
        .expect("export_dxf");
    // Four lines and one circle, not eight and two: seen along `z` each
    // body's front and back outlines project onto each other, agree on
    // visibility, and D1c merges them. The drawing is the same drawing; the
    // file no longer carries each line twice.
    assert_eq!(count_of(&text, "LINE"), 4, "the plate");
    assert_eq!(count_of(&text, "CIRCLE"), 1, "the disc's merged rims");
    // One nest, one set of extents spanning both parts.
    let all = codes(&text);
    let i = all
        .iter()
        .position(|(c, v)| *c == 9 && v == "$EXTMAX")
        .expect("$EXTMAX");
    assert!((all[i + 1].1.parse::<f64>().expect("real") - 68.0).abs() < 1e-9);
}

#[test]
fn export_dxf_refuses_a_degenerate_view_rather_than_writing_an_empty_file() {
    let mut a = KernelV2Adapter::new();
    let solid = make_box(&mut a, 0.010, 0.010, 0.010);
    let err = a
        .export_dxf(
            &[ProjectionBody::solo(solid)],
            &ViewFrame {
                origin: [0.0; 3],
                dir: [1.0, 0.0, 0.0],
                up: [-2.0, 0.0, 0.0],
            },
            &ProjectOpts::default(),
        )
        .expect_err("up parallel to the line of sight");
    assert!(format!("{err}").contains("degenerate view frame"), "{err}");
}

#[test]
fn the_section_method_is_a_loud_not_supported_until_d1d() {
    let mut a = KernelV2Adapter::new();
    let solid = make_box(&mut a, 0.010, 0.010, 0.010);
    let err = a
        .section_with_plane(&solid, [0.0, 0.0, 0.005], [0.0, 0.0, 1.0])
        .expect_err("D1d has not landed");
    assert!(
        matches!(err, KernelError::NotSupported { ref operation } if operation == "section_with_plane"),
        "{err}"
    );
}

// --- the golden ---

#[test]
fn the_box_top_view_matches_its_golden_byte_for_byte() {
    let mut a = KernelV2Adapter::new();
    let solid = make_box(&mut a, 0.040, 0.030, 0.010);
    let actual = a
        .export_dxf(
            &[ProjectionBody::solo(solid)],
            &ViewFrame::TOP,
            &ProjectOpts::default(),
        )
        .expect("export_dxf");

    let path = PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("tests/golden/box_top_view.dxf");
    if std::env::var_os("UPDATE_GOLDEN").is_some() {
        std::fs::create_dir_all(path.parent().expect("parent")).expect("mkdir");
        std::fs::write(&path, &actual).expect("write golden");
    }
    let expected = std::fs::read_to_string(&path)
        .unwrap_or_else(|e| panic!("{}: {e} (run with UPDATE_GOLDEN=1)", path.display()));
    if expected != actual {
        let first = expected
            .lines()
            .zip(actual.lines())
            .position(|(a, b)| a != b)
            .map(|i| i + 1);
        panic!(
            "{} differs from the writer's output (first differing line: {first:?}); \
             run with UPDATE_GOLDEN=1 and read the diff",
            path.display()
        );
    }
}
