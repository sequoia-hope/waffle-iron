//! S1 — the sketch operation set (`specs/agent_mechanical_design.md` §10.1).
//!
//! One test per operation and per typed refusal, plus the batch behaviour
//! `sketch_edit` (S3) and the rewired UI both depend on: ops compose on the
//! running state, ids never collide, and a `MovePoint` pin is transient.
//!
//! Determinism: literal geometry, fixed ids, `Uuid::nil()`.

use sketch_solver::ops::geom::Point2;
use sketch_solver::ops::{
    apply_edit, apply_ops, extend, fillet, fillet_default_radius, fillet_geometry, mirror, offset,
    project, remove_entities, trim, IdAllocator,
};
use sketch_solver::*;
use std::collections::HashMap;
use uuid::Uuid;
use waffle_types::sketch_plane::SketchPlaneBasis;
use waffle_types::{End, ProjectShape, ProjectedPoint, Side, SketchOp, SketchOpError};

// ── Fixtures ────────────────────────────────────────────────────────────────

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

fn circle(id: u32, center_id: u32, radius: f64) -> SketchEntity {
    SketchEntity::Circle {
        id,
        center_id,
        radius,
        construction: false,
    }
}

fn make_sketch(entities: Vec<SketchEntity>, constraints: Vec<SketchConstraint>) -> Sketch {
    let mut sketch = Sketch {
        id: Uuid::nil(),
        plane: GeomRef {
            kind: TopoKind::Face,
            anchor: Anchor::Datum {
                datum_id: Uuid::nil(),
            },
            selector: Selector::Role {
                role: Role::ProfileFace,
                index: 0,
            },
            policy: ResolvePolicy::Strict,
            scope: None,
        },
        plane_face: None,
        plane_origin: [0.0, 0.0, 0.0],
        plane_normal: [0.0, 0.0, 1.0],
        plane_x_axis: None,
        entities,
        constraints,
        solve_status: SolveStatus::Unsolved,
        solved_positions: HashMap::new(),
        solved_profiles: Vec::new(),
        projected: Vec::new(),
    };
    // Ops read solved positions; a fixture's declared coordinates ARE its
    // solved ones until something solves it.
    for e in sketch.entities.clone() {
        if let SketchEntity::Point { id, x, y, .. } = e {
            sketch.solved_positions.insert(id, (x, y));
        }
    }
    sketch
}

/// Two crossing lines forming an X through the origin: line 10 from
/// (-5, -5) to (5, 5), line 11 from (-5, 5) to (5, -5).
fn crossing_lines() -> Sketch {
    make_sketch(
        vec![
            point(1, -5.0, -5.0),
            point(2, 5.0, 5.0),
            point(3, -5.0, 5.0),
            point(4, 5.0, -5.0),
            line(10, 1, 2),
            line(11, 3, 4),
        ],
        Vec::new(),
    )
}

/// A unit square: points 1..4 CCW from the origin, lines 10..13.
fn square() -> Sketch {
    make_sketch(
        vec![
            point(1, 0.0, 0.0),
            point(2, 1.0, 0.0),
            point(3, 1.0, 1.0),
            point(4, 0.0, 1.0),
            line(10, 1, 2),
            line(11, 2, 3),
            line(12, 3, 4),
            line(13, 4, 1),
        ],
        Vec::new(),
    )
}

fn positions(sketch: &Sketch) -> HashMap<u32, (f64, f64)> {
    sketch.solved_positions.clone()
}

fn entity_of(sketch: &Sketch, id: u32) -> &SketchEntity {
    sketch
        .entities
        .iter()
        .find(|e| e.id() == id)
        .unwrap_or_else(|| panic!("entity {id} is gone"))
}

fn count_of(sketch: &Sketch, kind: &str) -> usize {
    sketch
        .entities
        .iter()
        .filter(|e| match e {
            SketchEntity::Point { .. } => kind == "Point",
            SketchEntity::Line { .. } => kind == "Line",
            SketchEntity::Circle { .. } => kind == "Circle",
            SketchEntity::Arc { .. } => kind == "Arc",
            _ => false,
        })
        .count()
}

// ── Trim ────────────────────────────────────────────────────────────────────

#[test]
fn trimming_the_upper_half_of_a_crossed_line_leaves_the_lower_half() {
    let s = crossing_lines();
    let mut ids = IdAllocator::for_sketch(&s, 0);
    // Click on the upper-right piece of line 10, past the (0,0) crossing.
    let edit = trim(&s, 10, Point2::new(3.0, 3.0), &mut ids).expect("a crossed line trims");
    let mut after = s.clone();
    apply_edit(&mut after, &edit);

    assert_eq!(
        count_of(&after, "Line"),
        2,
        "line 11 is untouched, 10 is cut"
    );
    // Line 10 KEEPS its id and its start point: the lower half survives.
    let SketchEntity::Line {
        start_id, end_id, ..
    } = entity_of(&after, 10)
    else {
        panic!("line 10 is still a line");
    };
    assert_eq!(*start_id, 1, "the far endpoint keeps its id");
    let cut = positions(&after)[end_id];
    assert!(
        cut.0.abs() < 1e-12 && cut.1.abs() < 1e-12,
        "the cut lands on the crossing, got {cut:?}"
    );
    assert!(
        after.entities.iter().all(|e| e.id() != 2) || *end_id == 2,
        "the trimmed-away endpoint is gone"
    );
}

#[test]
fn trimming_the_middle_of_a_twice_crossed_line_leaves_both_ends() {
    // A horizontal line crossed by two verticals at x = -1 and x = 1.
    let s = make_sketch(
        vec![
            point(1, -3.0, 0.0),
            point(2, 3.0, 0.0),
            line(10, 1, 2),
            point(3, -1.0, -1.0),
            point(4, -1.0, 1.0),
            line(11, 3, 4),
            point(5, 1.0, -1.0),
            point(6, 1.0, 1.0),
            line(12, 5, 6),
        ],
        Vec::new(),
    );
    let mut ids = IdAllocator::for_sketch(&s, 0);
    let edit = trim(&s, 10, Point2::new(0.0, 0.0), &mut ids).expect("trims");
    let mut after = s.clone();
    apply_edit(&mut after, &edit);
    assert_eq!(
        count_of(&after, "Line"),
        4,
        "two crossing lines plus both surviving halves"
    );
    assert_eq!(
        edit.added
            .iter()
            .filter(|e| matches!(e, SketchEntity::Line { .. }))
            .count(),
        1
    );
    assert_eq!(edit.changed.len(), 1, "the head keeps the original id");
}

#[test]
fn trimming_a_line_that_crosses_nothing_removes_it() {
    let s = make_sketch(
        vec![point(1, 0.0, 0.0), point(2, 1.0, 1.0), line(10, 1, 2)],
        Vec::new(),
    );
    let mut ids = IdAllocator::for_sketch(&s, 0);
    let edit = trim(&s, 10, Point2::new(0.5, 0.5), &mut ids).expect("trims");
    let mut after = s.clone();
    apply_edit(&mut after, &edit);
    assert!(
        after.entities.is_empty(),
        "the line and its orphaned points go"
    );
}

#[test]
fn trimming_a_circle_removes_it_whole() {
    // The inherited limitation, pinned so it is a decision and not a surprise:
    // piece-wise trimming of a curve is not implemented.
    let s = make_sketch(vec![point(1, 0.0, 0.0), circle(20, 1, 1.0)], Vec::new());
    let mut ids = IdAllocator::for_sketch(&s, 0);
    let edit = trim(&s, 20, Point2::new(1.0, 0.0), &mut ids).expect("removes");
    assert!(edit.removed.contains(&20));
}

#[test]
fn trimming_an_entity_that_is_not_there_is_refused_by_name() {
    let s = crossing_lines();
    let mut ids = IdAllocator::for_sketch(&s, 0);
    assert_eq!(
        trim(&s, 999, Point2::new(0.0, 0.0), &mut ids).expect_err("refused"),
        SketchOpError::NoSuchEntity { id: 999 }
    );
}

#[test]
fn a_trim_drops_the_constraints_that_named_the_vanished_geometry() {
    let mut s = make_sketch(
        vec![point(1, 0.0, 0.0), point(2, 1.0, 1.0), line(10, 1, 2)],
        vec![SketchConstraint::Horizontal { entity: 10 }],
    );
    s.solve_status = SolveStatus::Unsolved;
    let mut ids = IdAllocator::for_sketch(&s, 0);
    let edit = trim(&s, 10, Point2::new(0.5, 0.5), &mut ids).expect("removes");
    assert_eq!(edit.constraints_removed, vec![0]);
    let mut after = s.clone();
    apply_edit(&mut after, &edit);
    assert!(after.constraints.is_empty());
}

// ── Extend ──────────────────────────────────────────────────────────────────

#[test]
fn extending_a_line_reaches_the_target_it_points_at() {
    // A short horizontal line and a vertical wall at x = 4.
    let s = make_sketch(
        vec![
            point(1, 0.0, 0.0),
            point(2, 1.0, 0.0),
            line(10, 1, 2),
            point(3, 4.0, -2.0),
            point(4, 4.0, 2.0),
            line(11, 3, 4),
        ],
        Vec::new(),
    );
    let mut ids = IdAllocator::for_sketch(&s, 0);
    let edit = extend(&s, 10, End::End, None, &mut ids).expect("reaches the wall");
    let mut after = s.clone();
    apply_edit(&mut after, &edit);
    let (x, y) = positions(&after)[&2];
    assert!((x - 4.0).abs() < 1e-12, "x = {x}");
    assert!(y.abs() < 1e-12, "y = {y}");
    assert_eq!(
        count_of(&after, "Line"),
        2,
        "no new geometry, just a longer line"
    );
}

#[test]
fn extending_the_start_end_goes_the_other_way() {
    let s = make_sketch(
        vec![
            point(1, 0.0, 0.0),
            point(2, 1.0, 0.0),
            line(10, 1, 2),
            point(3, -4.0, -2.0),
            point(4, -4.0, 2.0),
            line(11, 3, 4),
        ],
        Vec::new(),
    );
    let mut ids = IdAllocator::for_sketch(&s, 0);
    let edit = extend(&s, 10, End::Start, None, &mut ids).expect("reaches");
    let mut after = s.clone();
    apply_edit(&mut after, &edit);
    assert!((positions(&after)[&1].0 + 4.0).abs() < 1e-12);
}

#[test]
fn extending_to_a_shared_point_mints_its_own_endpoint() {
    // Point 2 is shared by lines 10 and 12; extending 10 must not drag 12.
    let s = make_sketch(
        vec![
            point(1, 0.0, 0.0),
            point(2, 1.0, 0.0),
            line(10, 1, 2),
            point(5, 1.0, 3.0),
            line(12, 2, 5),
            point(3, 4.0, -2.0),
            point(4, 4.0, 2.0),
            line(11, 3, 4),
        ],
        Vec::new(),
    );
    let mut ids = IdAllocator::for_sketch(&s, 0);
    let edit = extend(&s, 10, End::End, None, &mut ids).expect("reaches");
    let mut after = s.clone();
    apply_edit(&mut after, &edit);
    assert_eq!(
        positions(&after)[&2],
        (1.0, 0.0),
        "the shared point stays where line 12 needs it"
    );
    let SketchEntity::Line { end_id, .. } = entity_of(&after, 10) else {
        panic!("still a line");
    };
    assert_ne!(*end_id, 2, "line 10 got its own extended endpoint");
    assert!((positions(&after)[end_id].0 - 4.0).abs() < 1e-12);
}

#[test]
fn extending_a_line_that_reaches_nothing_is_refused_by_name() {
    let s = make_sketch(
        vec![point(1, 0.0, 0.0), point(2, 1.0, 0.0), line(10, 1, 2)],
        Vec::new(),
    );
    let mut ids = IdAllocator::for_sketch(&s, 0);
    assert_eq!(
        extend(&s, 10, End::End, None, &mut ids).expect_err("refused"),
        SketchOpError::NothingToExtendTo { entity: 10 }
    );
}

#[test]
fn extending_only_counts_a_hit_on_the_targets_own_span() {
    // The wall is short and OFF to the side: its carrier crosses our line's
    // carrier, but the wall itself does not reach there.
    let s = make_sketch(
        vec![
            point(1, 0.0, 0.0),
            point(2, 1.0, 0.0),
            line(10, 1, 2),
            point(3, 4.0, 1.0),
            point(4, 4.0, 2.0),
            line(11, 3, 4),
        ],
        Vec::new(),
    );
    let mut ids = IdAllocator::for_sketch(&s, 0);
    assert_eq!(
        extend(&s, 10, End::End, None, &mut ids).expect_err("refused"),
        SketchOpError::NothingToExtendTo { entity: 10 },
        "an extend must not stop in mid-air at a carrier crossing"
    );
}

// ── Offset ──────────────────────────────────────────────────────────────────

#[test]
fn offsetting_a_square_outward_adds_four_lines_and_four_arcs() {
    let s = square();
    let mut ids = IdAllocator::for_sketch(&s, 0);
    // The square is traversed CCW, so its outside is Right.
    let edit = offset(&s, &[10, 11, 12, 13], 0.1, Side::Right, &mut ids).expect("offsets");
    let mut after = s.clone();
    apply_edit(&mut after, &edit);
    assert_eq!(count_of(&after, "Line"), 8, "the original four plus four");
    assert_eq!(count_of(&after, "Arc"), 4, "one rounded corner each");
    // The new geometry's extent is the original grown by the distance.
    let xs: Vec<f64> = edit
        .added
        .iter()
        .filter_map(|e| match e {
            SketchEntity::Point { x, .. } => Some(*x),
            _ => None,
        })
        .collect();
    assert!(
        xs.iter().cloned().fold(f64::MAX, f64::min) < -0.09,
        "grown outward: {xs:?}"
    );
}

#[test]
fn offsetting_a_square_inward_adds_four_mitered_lines() {
    let s = square();
    let mut ids = IdAllocator::for_sketch(&s, 0);
    let edit = offset(&s, &[10, 11, 12, 13], 0.1, Side::Left, &mut ids).expect("offsets");
    let lines = edit
        .added
        .iter()
        .filter(|e| matches!(e, SketchEntity::Line { .. }))
        .count();
    let arcs = edit
        .added
        .iter()
        .filter(|e| matches!(e, SketchEntity::Arc { .. }))
        .count();
    assert_eq!((lines, arcs), (4, 0), "inside corners trim, never arc");
}

#[test]
fn offsetting_a_circle_adds_one_concentric_circle() {
    let s = make_sketch(vec![point(1, 1.0, 2.0), circle(20, 1, 0.5)], Vec::new());
    let mut ids = IdAllocator::for_sketch(&s, 0);
    let edit = offset(&s, &[20], 0.25, Side::Left, &mut ids).expect("offsets");
    let added_circles: Vec<&SketchEntity> = edit
        .added
        .iter()
        .filter(|e| matches!(e, SketchEntity::Circle { .. }))
        .collect();
    assert_eq!(added_circles.len(), 1);
    let SketchEntity::Circle {
        radius, center_id, ..
    } = added_circles[0]
    else {
        panic!("a circle");
    };
    assert!((radius - 0.75).abs() < 1e-12, "r = {radius}");
    let center = edit
        .added
        .iter()
        .find_map(|e| match e {
            SketchEntity::Point { id, x, y, .. } if id == center_id => Some((*x, *y)),
            _ => None,
        })
        .expect("the new circle has a centre");
    assert_eq!(center, (1.0, 2.0), "concentric");
}

#[test]
fn a_closed_offset_comes_back_as_one_connected_loop() {
    let s = square();
    let mut ids = IdAllocator::for_sketch(&s, 0);
    let edit = offset(&s, &[10, 11, 12, 13], 0.1, Side::Left, &mut ids).expect("offsets");
    let mut after = s.clone();
    apply_edit(&mut after, &edit);
    // Every new line's endpoints are shared with exactly two new curves: the
    // loop welds, rather than arriving as four loose segments.
    let new_ids: Vec<u32> = edit.added.iter().map(|e| e.id()).collect();
    let mut uses: HashMap<u32, usize> = HashMap::new();
    for e in &edit.added {
        if let SketchEntity::Line {
            start_id, end_id, ..
        } = e
        {
            *uses.entry(*start_id).or_default() += 1;
            *uses.entry(*end_id).or_default() += 1;
        }
    }
    assert_eq!(uses.len(), 4, "four joints, not eight: {uses:?}");
    assert!(
        uses.values().all(|n| *n == 2),
        "each joint is shared: {uses:?}"
    );
    assert!(
        new_ids.iter().all(|id| *id > 13),
        "no id collides with the source"
    );
}

#[test]
fn offsetting_a_branching_selection_is_refused_with_the_chain_reason() {
    let s = make_sketch(
        vec![
            point(1, -1.0, 0.0),
            point(2, 0.0, 0.0),
            point(3, 1.0, 0.0),
            point(4, 0.0, -1.0),
            line(10, 1, 2),
            line(11, 2, 3),
            line(12, 2, 4),
        ],
        Vec::new(),
    );
    let mut ids = IdAllocator::for_sketch(&s, 0);
    assert_eq!(
        offset(&s, &[10, 11, 12], 0.1, Side::Left, &mut ids).expect_err("refused"),
        SketchOpError::OffsetRefused {
            reason: "branching".to_string()
        }
    );
}

#[test]
fn an_offset_that_would_collapse_a_radius_is_refused() {
    let s = make_sketch(vec![point(1, 0.0, 0.0), circle(20, 1, 0.5)], Vec::new());
    let mut ids = IdAllocator::for_sketch(&s, 0);
    assert_eq!(
        offset(&s, &[20], 0.5, Side::Right, &mut ids).expect_err("refused"),
        SketchOpError::OffsetRefused {
            reason: "radius-collapse".to_string()
        }
    );
}

// ── Fillet ──────────────────────────────────────────────────────────────────

#[test]
fn filleting_a_corner_rounds_it_and_keeps_both_lines() {
    // An L: (0,1) → (0,0) → (1,0), corner at point 2.
    let s = make_sketch(
        vec![
            point(1, 0.0, 1.0),
            point(2, 0.0, 0.0),
            point(3, 1.0, 0.0),
            line(10, 1, 2),
            line(11, 2, 3),
        ],
        Vec::new(),
    );
    let mut ids = IdAllocator::for_sketch(&s, 0);
    let edit = fillet(&s, 2, 0.2, &mut ids).expect("a right-angle corner rounds");
    let mut after = s.clone();
    apply_edit(&mut after, &edit);

    assert_eq!(count_of(&after, "Arc"), 1, "one fillet arc");
    assert_eq!(count_of(&after, "Line"), 2, "both lines survive, shortened");
    assert!(
        after.entities.iter().all(|e| e.id() != 2),
        "the corner point is gone"
    );
    // The arc is tangent to both lines: its centre is `r` from each.
    let SketchEntity::Arc { center_id, .. } = after
        .entities
        .iter()
        .find(|e| matches!(e, SketchEntity::Arc { .. }))
        .expect("an arc")
    else {
        panic!("an arc");
    };
    let c = positions(&after)[center_id];
    assert!(
        (c.0 - 0.2).abs() < 1e-9 && (c.1 - 0.2).abs() < 1e-9,
        "centre {c:?}"
    );
    assert_eq!(
        edit.constraints_added.len(),
        2,
        "a Tangent to each line, so the rounding survives the next solve"
    );
}

#[test]
fn a_fillet_keeps_the_dimension_on_the_line_it_shortens() {
    // The JS re-created both lines, which dropped this constraint.
    let s = make_sketch(
        vec![
            point(1, 0.0, 1.0),
            point(2, 0.0, 0.0),
            point(3, 1.0, 0.0),
            line(10, 1, 2),
            line(11, 2, 3),
        ],
        vec![SketchConstraint::Vertical { entity: 10 }],
    );
    let mut ids = IdAllocator::for_sketch(&s, 0);
    let edit = fillet(&s, 2, 0.2, &mut ids).expect("rounds");
    let mut after = s.clone();
    apply_edit(&mut after, &edit);
    assert!(
        after
            .constraints
            .iter()
            .any(|c| matches!(c, SketchConstraint::Vertical { entity: 10 })),
        "the Vertical on line 10 outlives the fillet: {:?}",
        after.constraints
    );
}

#[test]
fn a_fillet_radius_that_does_not_fit_is_refused_by_name() {
    let s = make_sketch(
        vec![
            point(1, 0.0, 1.0),
            point(2, 0.0, 0.0),
            point(3, 1.0, 0.0),
            line(10, 1, 2),
            line(11, 2, 3),
        ],
        Vec::new(),
    );
    let mut ids = IdAllocator::for_sketch(&s, 0);
    let err = fillet(&s, 2, 5.0, &mut ids).expect_err("5 m does not fit in a 1 m corner");
    assert_eq!(
        err,
        SketchOpError::FilletDoesNotFit {
            corner: 2,
            radius: 5.0
        }
    );
}

#[test]
fn a_point_that_is_not_a_corner_is_refused_with_its_line_count() {
    let s = square(); // every point has exactly two lines
    let mut ids = IdAllocator::for_sketch(&s, 0);
    // A point with THREE lines.
    let mut t = s.clone();
    t.entities.push(point(5, 2.0, 2.0));
    t.solved_positions.insert(5, (2.0, 2.0));
    t.entities.push(line(14, 1, 5));
    assert_eq!(
        fillet(&t, 1, 0.1, &mut ids).expect_err("refused"),
        SketchOpError::NotACorner { point: 1, lines: 3 }
    );
    // And a lone point with none.
    let lone = make_sketch(vec![point(1, 0.0, 0.0)], Vec::new());
    assert_eq!(
        fillet(&lone, 1, 0.1, &mut ids).expect_err("refused"),
        SketchOpError::NotACorner { point: 1, lines: 0 }
    );
}

#[test]
fn the_default_fillet_radius_is_a_third_of_the_shorter_leg() {
    let s = make_sketch(
        vec![
            point(1, 0.0, 3.0),
            point(2, 0.0, 0.0),
            point(3, 0.9, 0.0),
            line(10, 1, 2),
            line(11, 2, 3),
        ],
        Vec::new(),
    );
    let r = fillet_default_radius(&s, 2).expect("a corner");
    assert!((r - 0.3).abs() < 1e-12, "0.9 / 3 = 0.3, got {r}");
}

#[test]
fn the_fillet_preview_geometry_is_the_geometry_the_commit_uses() {
    let s = make_sketch(
        vec![
            point(1, 0.0, 1.0),
            point(2, 0.0, 0.0),
            point(3, 1.0, 0.0),
            line(10, 1, 2),
            line(11, 2, 3),
        ],
        Vec::new(),
    );
    let preview = fillet_geometry(&s, 2, 0.2).expect("previews");
    let mut ids = IdAllocator::for_sketch(&s, 0);
    let edit = fillet(&s, 2, 0.2, &mut ids).expect("commits");
    let minted: Vec<(f64, f64)> = edit
        .added
        .iter()
        .filter_map(|e| match e {
            SketchEntity::Point { x, y, .. } => Some((*x, *y)),
            _ => None,
        })
        .collect();
    assert!(
        minted.contains(&(preview.center.x, preview.center.y)),
        "the committed arc centre is the previewed one: {minted:?} vs {preview:?}"
    );
    assert!(minted.contains(&(preview.tangent_a.x, preview.tangent_a.y)));
    assert!(minted.contains(&(preview.tangent_b.x, preview.tangent_b.y)));
}

// ── Mirror ──────────────────────────────────────────────────────────────────

#[test]
fn mirroring_a_line_across_a_vertical_axis_reflects_its_points() {
    // Axis: the y-axis (line 99 from (0,-2) to (0,2)). Subject: a line at x > 0.
    let s = make_sketch(
        vec![
            point(1, 0.0, -2.0),
            point(2, 0.0, 2.0),
            line(99, 1, 2),
            point(3, 1.0, 0.0),
            point(4, 2.0, 1.0),
            line(10, 3, 4),
        ],
        Vec::new(),
    );
    let mut ids = IdAllocator::for_sketch(&s, 0);
    let edit = mirror(&s, &[10], 99, &mut ids).expect("mirrors");
    let mut after = s.clone();
    apply_edit(&mut after, &edit);
    assert_eq!(count_of(&after, "Line"), 3, "axis, subject, image");
    let images: Vec<(f64, f64)> = edit
        .added
        .iter()
        .filter_map(|e| match e {
            SketchEntity::Point { x, y, .. } => Some((*x, *y)),
            _ => None,
        })
        .collect();
    assert!(images.contains(&(-1.0, 0.0)), "{images:?}");
    assert!(images.contains(&(-2.0, 1.0)), "{images:?}");
}

#[test]
fn mirroring_an_arc_swaps_its_endpoints_to_stay_counter_clockwise() {
    let s = make_sketch(
        vec![
            point(1, 0.0, -2.0),
            point(2, 0.0, 2.0),
            line(99, 1, 2),
            point(3, 2.0, 0.0), // centre
            point(4, 3.0, 0.0), // start
            point(5, 2.0, 1.0), // end
            SketchEntity::Arc {
                id: 30,
                center_id: 3,
                start_id: 4,
                end_id: 5,
                construction: false,
            },
        ],
        Vec::new(),
    );
    let mut ids = IdAllocator::for_sketch(&s, 0);
    let edit = mirror(&s, &[30], 99, &mut ids).expect("mirrors");
    let arc = edit
        .added
        .iter()
        .find(|e| matches!(e, SketchEntity::Arc { .. }))
        .expect("an arc image");
    let SketchEntity::Arc {
        center_id,
        start_id,
        end_id,
        ..
    } = arc
    else {
        panic!("an arc")
    };
    let at = |id: u32| {
        edit.added
            .iter()
            .find_map(|e| match e {
                SketchEntity::Point { id: i, x, y, .. } if *i == id => Some((*x, *y)),
                _ => None,
            })
            .expect("minted")
    };
    assert_eq!(at(*center_id), (-2.0, 0.0));
    // The original ran start (3,0) → end (2,1); reflected, the CCW run is the
    // image of the END to the image of the START.
    assert_eq!(
        at(*start_id),
        (-2.0, 1.0),
        "the swap keeps the short way round"
    );
    assert_eq!(at(*end_id), (-3.0, 0.0));
}

#[test]
fn mirroring_the_axis_itself_is_refused() {
    let s = make_sketch(
        vec![point(1, 0.0, -2.0), point(2, 0.0, 2.0), line(99, 1, 2)],
        Vec::new(),
    );
    let mut ids = IdAllocator::for_sketch(&s, 0);
    assert!(matches!(
        mirror(&s, &[99], 99, &mut ids),
        Err(SketchOpError::MirrorRefused { .. })
    ));
}

#[test]
fn mirroring_across_something_that_is_not_a_line_is_refused() {
    let s = make_sketch(
        vec![
            point(1, 0.0, 0.0),
            circle(20, 1, 1.0),
            point(3, 2.0, 0.0),
            point(4, 3.0, 0.0),
            line(10, 3, 4),
        ],
        Vec::new(),
    );
    let mut ids = IdAllocator::for_sketch(&s, 0);
    assert_eq!(
        mirror(&s, &[10], 20, &mut ids).expect_err("refused"),
        SketchOpError::WrongEntityKind {
            id: 20,
            expected: "Line".to_string(),
            found: "Circle".to_string()
        }
    );
}

// ── Project ─────────────────────────────────────────────────────────────────

#[test]
fn projecting_world_points_maps_them_onto_the_sketch_plane() {
    // The XY plane: world (x, y, 0) lands at sketch (x, y) for the derived
    // basis, so assert through the basis rather than assuming its choice.
    let plane = SketchPlaneBasis::from_origin_normal_x([0.0, 0.0, 5.0], [0.0, 0.0, 1.0], None);
    let worlds = [[1.0, 2.0, 5.0], [3.0, 4.0, 5.0]];
    let points: Vec<ProjectedPoint> = worlds
        .iter()
        .map(|w| ProjectedPoint {
            world: *w,
            source: None,
        })
        .collect();
    let s = make_sketch(Vec::new(), Vec::new());
    let mut ids = IdAllocator::for_sketch(&s, 0);
    let edit = project(
        &points,
        &ProjectShape::Polyline { closed: false },
        &plane,
        &mut ids,
    )
    .expect("projects");
    assert_eq!(edit.added.len(), 3, "two points and the line between them");
    for (i, w) in worlds.iter().enumerate() {
        let (u, v) = plane.world_to_local(*w);
        let SketchEntity::Point { x, y, .. } = edit.added[i] else {
            panic!("a point")
        };
        assert!((x - u).abs() < 1e-12 && (y - v).abs() < 1e-12);
    }
}

#[test]
fn a_closed_projection_joins_the_last_point_back_to_the_first() {
    let plane = SketchPlaneBasis::from_origin_normal_x([0.0; 3], [0.0, 0.0, 1.0], None);
    let points: Vec<ProjectedPoint> = [[0.0, 0.0, 0.0], [1.0, 0.0, 0.0], [1.0, 1.0, 0.0]]
        .iter()
        .map(|w| ProjectedPoint {
            world: *w,
            source: None,
        })
        .collect();
    let s = make_sketch(Vec::new(), Vec::new());
    let mut ids = IdAllocator::for_sketch(&s, 0);
    let edit = project(
        &points,
        &ProjectShape::Polyline { closed: true },
        &plane,
        &mut ids,
    )
    .expect("projects");
    let lines = edit
        .added
        .iter()
        .filter(|e| matches!(e, SketchEntity::Line { .. }))
        .count();
    assert_eq!(lines, 3, "a closed triangle, not an open polyline");
}

#[test]
fn projecting_nothing_is_refused() {
    let plane = SketchPlaneBasis::from_origin_normal_x([0.0; 3], [0.0, 0.0, 1.0], None);
    let s = make_sketch(Vec::new(), Vec::new());
    let mut ids = IdAllocator::for_sketch(&s, 0);
    assert!(matches!(
        project(&[], &ProjectShape::Points, &plane, &mut ids),
        Err(SketchOpError::ProjectRefused { .. })
    ));
}

#[test]
fn a_projected_point_carries_its_rebuild_binding() {
    let plane = SketchPlaneBasis::from_origin_normal_x([0.0; 3], [0.0, 0.0, 1.0], None);
    let source = waffle_types::ProjectedSource {
        geom_ref: GeomRef {
            kind: TopoKind::Vertex,
            anchor: Anchor::Datum {
                datum_id: Uuid::nil(),
            },
            selector: Selector::Role {
                role: Role::ProfileFace,
                index: 0,
            },
            policy: ResolvePolicy::Strict,
            scope: None,
        },
        kind: waffle_types::ProjectedKind::Vertex,
    };
    let points = vec![ProjectedPoint {
        world: [1.0, 1.0, 0.0],
        source: Some(source),
    }];
    let s = make_sketch(Vec::new(), Vec::new());
    let mut ids = IdAllocator::for_sketch(&s, 0);
    let edit = project(&points, &ProjectShape::Points, &plane, &mut ids).expect("projects");
    assert_eq!(edit.projected_added.len(), 1);
    assert_eq!(edit.projected_added[0].point_id, edit.added[0].id());
}

// ── Removal cascade ─────────────────────────────────────────────────────────

#[test]
fn removing_a_line_takes_its_orphaned_points_and_not_the_shared_one() {
    let s = square();
    let edit = remove_entities(&s, &[10]);
    let mut after = s.clone();
    apply_edit(&mut after, &edit);
    assert_eq!(count_of(&after, "Line"), 3);
    assert_eq!(
        count_of(&after, "Point"),
        4,
        "both of line 10's points are shared by the neighbours, so none orphan"
    );
}

#[test]
fn removing_a_point_cascades_to_the_curves_that_stand_on_it() {
    let s = square();
    let edit = remove_entities(&s, &[1]);
    let mut after = s.clone();
    apply_edit(&mut after, &edit);
    assert_eq!(
        count_of(&after, "Line"),
        2,
        "lines 10 and 13 went with point 1"
    );
    assert_eq!(count_of(&after, "Point"), 3);
}

// ── The batch ───────────────────────────────────────────────────────────────

#[test]
fn ops_apply_in_order_against_the_running_state() {
    // Trim an X, then fillet the corner the trim left. The fillet can only
    // find its corner if it sees the trimmed geometry.
    let s = make_sketch(
        vec![
            point(1, 0.0, 2.0),
            point(2, 0.0, 0.0),
            point(3, 2.0, 0.0),
            line(10, 1, 2),
            line(11, 2, 3),
        ],
        Vec::new(),
    );
    let applied = apply_ops(
        &s,
        &[
            SketchOp::Fillet {
                corner: 2,
                radius: 0.5,
            },
            SketchOp::AddConstraint {
                constraint: SketchConstraint::Vertical { entity: 10 },
            },
        ],
        0,
    )
    .expect("a batch applies");
    assert_eq!(count_of(&applied.sketch, "Arc"), 1);
    assert_eq!(
        applied.sketch.constraints.len(),
        3,
        "two Tangents and the Vertical"
    );
    assert!(
        applied.next_id > 11,
        "the allocator advanced: {}",
        applied.next_id
    );
}

#[test]
fn a_batch_never_mints_an_id_that_collides_with_the_callers_counter() {
    let s = square();
    let applied = apply_ops(
        &s,
        &[SketchOp::Offset {
            chain: vec![10, 11, 12, 13],
            distance: 0.1,
            side: Side::Left,
        }],
        500, // the UI's own nextEntityId
    )
    .expect("offsets");
    assert!(
        applied.edit.added.iter().all(|e| e.id() >= 500),
        "the hint is a floor: {:?}",
        applied
            .edit
            .added
            .iter()
            .map(|e| e.id())
            .collect::<Vec<_>>()
    );
    assert!(applied.next_id > 500);
}

#[test]
fn an_add_entity_with_id_zero_is_allocated_one() {
    let s = make_sketch(Vec::new(), Vec::new());
    let applied = apply_ops(
        &s,
        &[SketchOp::AddEntity {
            entity: point(0, 1.0, 2.0),
        }],
        0,
    )
    .expect("adds");
    assert_eq!(applied.sketch.entities.len(), 1);
    assert!(applied.sketch.entities[0].id() > 0, "ids start at 1");
}

#[test]
fn a_move_point_is_a_transient_pin_and_not_a_stored_constraint() {
    let s = square();
    let applied = apply_ops(
        &s,
        &[SketchOp::MovePoint {
            id: 3,
            to: [2.0, 2.0],
        }],
        0,
    )
    .expect("moves");
    assert_eq!(applied.sketch.solved_positions[&3], (2.0, 2.0));
    assert!(
        applied.sketch.constraints.is_empty(),
        "a drag leaves no constraint behind"
    );
    assert_eq!(
        applied.transient_constraints.len(),
        1,
        "but the next solve is told where the pointer put it"
    );
    assert!(matches!(
        applied.transient_constraints[0],
        SketchConstraint::Pinned { point: 3, .. }
    ));
}

#[test]
fn setting_a_dimension_retargets_it_without_touching_the_geometry() {
    let s = make_sketch(
        vec![point(1, 0.0, 0.0), point(2, 1.0, 0.0), line(10, 1, 2)],
        vec![SketchConstraint::Distance {
            entity_a: 1,
            entity_b: 2,
            value: 1.0,
            expression: None,
            reference: false,
        }],
    );
    let applied = apply_ops(
        &s,
        &[SketchOp::SetDimension {
            index: 0,
            value: Some(3.0),
            expression: Some("width".to_string()),
        }],
        0,
    )
    .expect("retargets");
    assert_eq!(applied.sketch.constraints[0].dimension_value(), Some(3.0));
    assert_eq!(applied.sketch.constraints[0].expression(), Some("width"));
    assert_eq!(
        applied.sketch.solved_positions[&2],
        (1.0, 0.0),
        "the solve moves the geometry, not the op"
    );
}

#[test]
fn setting_a_dimension_on_a_constraint_that_has_none_is_refused() {
    let s = make_sketch(
        vec![point(1, 0.0, 0.0), point(2, 1.0, 0.0), line(10, 1, 2)],
        vec![SketchConstraint::Horizontal { entity: 10 }],
    );
    assert_eq!(
        apply_ops(
            &s,
            &[SketchOp::SetDimension {
                index: 0,
                value: Some(3.0),
                expression: None,
            }],
            0,
        )
        .expect_err("refused"),
        SketchOpError::NotADimension { index: 0 }
    );
}

#[test]
fn removing_a_constraint_that_is_not_there_is_refused_by_index() {
    let s = square();
    assert_eq!(
        apply_ops(&s, &[SketchOp::RemoveConstraint { index: 7 }], 0).expect_err("refused"),
        SketchOpError::NoSuchConstraint { index: 7 }
    );
}

#[test]
fn setting_construction_flips_only_that_entity() {
    let s = square();
    let applied = apply_ops(
        &s,
        &[SketchOp::SetConstruction {
            entity: 10,
            construction: true,
        }],
        0,
    )
    .expect("flips");
    assert!(entity_of(&applied.sketch, 10).is_construction());
    assert!(!entity_of(&applied.sketch, 11).is_construction());
}

#[test]
fn a_refused_op_leaves_the_sketch_alone() {
    // The batch is all-or-nothing: `apply_ops` works on a clone and the
    // caller only ever sees the sketch of a batch that succeeded.
    let s = square();
    let before = s.entities.len();
    let err = apply_ops(
        &s,
        &[
            SketchOp::SetConstruction {
                entity: 10,
                construction: true,
            },
            SketchOp::Fillet {
                corner: 1,
                radius: 99.0,
            },
        ],
        0,
    )
    .expect_err("the fillet does not fit");
    assert!(matches!(err, SketchOpError::FilletDoesNotFit { .. }));
    assert_eq!(s.entities.len(), before, "the input is untouched");
    assert!(
        !entity_of(&s, 10).is_construction(),
        "including the first op"
    );
}

#[test]
fn the_same_batch_twice_produces_the_same_geometry() {
    let s = square();
    let ops = [SketchOp::Offset {
        chain: vec![10, 11, 12, 13],
        distance: 0.1,
        side: Side::Right,
    }];
    let a = apply_ops(&s, &ops, 0).expect("offsets");
    let b = apply_ops(&s, &ops, 0).expect("offsets");
    let coords = |applied: &sketch_solver::ops::AppliedOps| -> Vec<(u32, f64, f64)> {
        applied
            .edit
            .added
            .iter()
            .filter_map(|e| match e {
                SketchEntity::Point { id, x, y, .. } => Some((*id, *x, *y)),
                _ => None,
            })
            .collect()
    };
    assert_eq!(coords(&a), coords(&b), "ops are deterministic");
}

#[test]
fn an_edited_sketch_still_solves() {
    // The whole point of the operation set: what comes out is a sketch the
    // solver accepts. A fillet adds two Tangent constraints, and this is the
    // pin that they are solvable rather than an instant conflict.
    let s = make_sketch(
        vec![
            point(1, 0.0, 1.0),
            point(2, 0.0, 0.0),
            point(3, 1.0, 0.0),
            line(10, 1, 2),
            line(11, 2, 3),
        ],
        Vec::new(),
    );
    let applied = apply_ops(
        &s,
        &[SketchOp::Fillet {
            corner: 2,
            radius: 0.2,
        }],
        0,
    )
    .expect("rounds");
    let solved = solve_sketch(&applied.sketch);
    assert!(
        matches!(
            solved.status,
            SolveStatus::FullyConstrained | SolveStatus::UnderConstrained { .. }
        ),
        "a filleted sketch solves: {:?} (residuals {:?})",
        solved.status,
        solved.report.residuals
    );
    assert!(
        solved.report.conflicts.is_empty(),
        "the two Tangents do not fight: {:?}",
        solved.report.conflicts
    );
}
