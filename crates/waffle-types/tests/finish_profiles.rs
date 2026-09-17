//! `build_finish_profiles` — the profile payload of a `FinishSketch`
//! (`specs/waffle_server_mode.md` §2.3 S3 C5), ported from
//! `app/src/lib/sketch/finishProfiles.js`.
//!
//! The agreement with the page's implementation is proven end to end by the
//! recorded goldens (`app/tests/gui/fixtures/agent-authoring-goldens.json`,
//! captured from the JS side). These tests pin what a whole-document
//! comparison cannot isolate: the arc sampling, the synthetic id counter, and
//! the arc segment's end index — the off-by-one that once cut every arc to
//! (N-1)/N of its sweep and left a chord sliver.

use std::collections::HashMap;

use waffle_types::profiles::{build_finish_profiles, extract_profiles};
use waffle_types::sketch::{ClosedProfile, SketchEntity};

fn point(id: u32, x: f64, y: f64) -> SketchEntity {
    SketchEntity::Point {
        id,
        x,
        y,
        construction: false,
    }
}

fn line(id: u32, start_id: u32, end_id: u32) -> SketchEntity {
    SketchEntity::Line {
        id,
        start_id,
        end_id,
        construction: false,
    }
}

fn arc(id: u32, center_id: u32, start_id: u32, end_id: u32) -> SketchEntity {
    SketchEntity::Arc {
        id,
        center_id,
        start_id,
        end_id,
        construction: false,
    }
}

fn positions(pairs: &[(u32, f64, f64)]) -> HashMap<u32, (f64, f64)> {
    pairs.iter().map(|&(id, x, y)| (id, (x, y))).collect()
}

/// A profile as `extract_profiles` hands it over: ids and winding only.
fn extracted(entity_ids: &[u32], is_outer: bool) -> ClosedProfile {
    ClosedProfile {
        entity_ids: entity_ids.to_vec(),
        is_outer,
        vertex_ids: Vec::new(),
        circle: None,
        spline_segments: Vec::new(),
        arc_segments: Vec::new(),
    }
}

/// 20 × 10 mm rectangle: points 1–4, lines 5–8.
fn rectangle() -> (Vec<SketchEntity>, HashMap<u32, (f64, f64)>) {
    let entities = vec![
        point(1, 0.0, 0.0),
        point(2, 0.02, 0.0),
        point(3, 0.02, 0.01),
        point(4, 0.0, 0.01),
        line(5, 1, 2),
        line(6, 2, 3),
        line(7, 3, 4),
        line(8, 4, 1),
    ];
    let pos = positions(&[
        (1, 0.0, 0.0),
        (2, 0.02, 0.0),
        (3, 0.02, 0.01),
        (4, 0.0, 0.01),
    ]);
    (entities, pos)
}

/// A D: an arc from (0.01,0) to (-0.01,0) about the origin, closed by a line.
fn d_shape(
    arc_id: u32,
    line_id: u32,
    ids: [u32; 3],
) -> (Vec<SketchEntity>, HashMap<u32, (f64, f64)>) {
    let [c, s, e] = ids;
    let entities = vec![
        point(c, 0.0, 0.0),
        point(s, 0.01, 0.0),
        point(e, -0.01, 0.0),
        arc(arc_id, c, s, e),
        line(line_id, e, s),
    ];
    let pos = positions(&[(c, 0.0, 0.0), (s, 0.01, 0.0), (e, -0.01, 0.0)]);
    (entities, pos)
}

#[test]
fn a_polygon_contributes_one_point_per_entity_in_walk_order() {
    let (entities, pos) = rectangle();
    let out = build_finish_profiles(&[extracted(&[5, 6, 7, 8], true)], &entities, &pos);

    assert_eq!(out.profiles.len(), 1);
    let p = &out.profiles[0];
    // Each line gives its start point; the next line's start is its end.
    assert_eq!(p.vertex_ids, vec![1, 2, 3, 4]);
    assert!(p.arc_segments.is_empty());
    assert!(p.circle.is_none());
    assert!(p.is_outer);
    // Nothing synthetic was minted, so the positions are untouched.
    assert_eq!(out.solved_positions.len(), pos.len());
}

#[test]
fn a_standalone_circle_becomes_a_tagged_circle_profile() {
    let entities = vec![
        point(1, 0.0, 0.0),
        SketchEntity::Circle {
            id: 2,
            center_id: 1,
            radius: 0.005,
            construction: false,
        },
    ];
    let pos = positions(&[(1, 0.0, 0.0)]);
    let out = build_finish_profiles(&[extracted(&[2], true)], &entities, &pos);

    let circle = out.profiles[0].circle.as_ref().expect("a circle profile");
    assert_eq!(circle.center_u, 0.0);
    assert_eq!(circle.center_v, 0.0);
    assert_eq!(circle.radius, 0.005);
    // A circle is a true cylinder, not a polygon: it names no vertices.
    assert!(out.profiles[0].vertex_ids.is_empty());
}

#[test]
fn a_circle_whose_centre_has_no_position_degrades_to_ids_only() {
    let entities = vec![SketchEntity::Circle {
        id: 2,
        center_id: 1,
        radius: 0.005,
        construction: false,
    }];
    let out = build_finish_profiles(&[extracted(&[2], true)], &entities, &HashMap::new());

    assert!(out.profiles[0].circle.is_none());
    assert!(out.profiles[0].vertex_ids.is_empty());
    assert_eq!(out.profiles[0].entity_ids, vec![2]);
}

#[test]
fn an_arc_is_sampled_and_its_segment_ends_on_the_next_entitys_start() {
    let (entities, pos) = d_shape(4, 5, [1, 2, 3]);
    let out = build_finish_profiles(&[extracted(&[4, 5], true)], &entities, &pos);
    let p = &out.profiles[0];

    // The arc's own start, then 15 interior samples, then the line's start.
    assert_eq!(p.vertex_ids.len(), 17);
    assert_eq!(p.vertex_ids[0], 2);
    assert_eq!(p.vertex_ids[16], 3);
    for (i, id) in p.vertex_ids[1..16].iter().enumerate() {
        assert_eq!(*id, 900_000 + i as u32, "synthetic ids run from 900000");
    }

    let seg = &p.arc_segments[0];
    assert_eq!(seg.start_vertex_index, 0);
    // The arc's true end vertex is the NEXT entity's start — index 16 — not
    // the last interior sample, which would cut the sweep to 15/16.
    assert_eq!(seg.end_vertex_index, 16);
    assert_eq!(seg.center_u, 0.0);
    assert_eq!(seg.center_v, 0.0);
    assert!((seg.radius - 0.01).abs() < 1e-12);

    // Every sample lies on the arc, and was recorded for the kernel to read.
    for id in &p.vertex_ids[1..16] {
        let &(x, y) = out.solved_positions.get(id).expect("a synthetic position");
        assert!((x.hypot(y) - 0.01).abs() < 1e-12, "sample off the circle");
        assert!(y >= 0.0, "the CCW sweep stays on the upper half");
    }
}

#[test]
fn synthetic_ids_do_not_collide_across_profiles() {
    // Two D shapes in one sketch: the second profile's samples must not reuse
    // the first's ids, or the two arcs would share points.
    let (mut entities, mut pos) = d_shape(4, 5, [1, 2, 3]);
    let (more, more_pos) = d_shape(14, 15, [11, 12, 13]);
    entities.extend(more);
    pos.extend(more_pos);

    let out = build_finish_profiles(
        &[extracted(&[4, 5], true), extracted(&[14, 15], true)],
        &entities,
        &pos,
    );

    let first: Vec<u32> = out.profiles[0].vertex_ids[1..16].to_vec();
    let second: Vec<u32> = out.profiles[1].vertex_ids[1..16].to_vec();
    assert_eq!(first[0], 900_000);
    assert_eq!(second[0], 900_015, "the counter carries across profiles");
    for id in &second {
        assert!(!first.contains(id), "profiles shared a synthetic id");
    }
}

#[test]
fn the_extractor_and_the_payload_compose() {
    // The pair as `sketch_create` uses them: extract the loops, then build the
    // payload the kernel takes.
    let (entities, pos) = rectangle();
    let out = build_finish_profiles(&extract_profiles(&entities, &pos), &entities, &pos);

    assert_eq!(out.profiles.len(), 1, "one closed loop");
    assert_eq!(out.profiles[0].vertex_ids.len(), 4);
    assert!(out.profiles[0].is_outer, "the rectangle winds outward");
}
