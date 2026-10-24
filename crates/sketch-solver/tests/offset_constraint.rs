//! The parametric chain offset (specs/sketch_chain_offset.md §"Cycle 4").
//!
//! An Offset op now records ONE `SketchConstraint::Offset` tying the new
//! chain to its source at the typed distance. The contract, per chain shape:
//! the freshly made offset already satisfies it (the first solve moves
//! nothing), it fully constrains the offset relative to a fixed source (no
//! free direction, no redundant row), and editing its one value moves every
//! offset curve to the new distance with the joints still joined.
//!
//! Determinism: literal geometry, fixed ids, `Uuid::nil()`.

use sketch_solver::ops::{apply_edit, offset, IdAllocator};
use sketch_solver::*;
use std::collections::HashMap;
use uuid::Uuid;
use waffle_types::Side;

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

fn circle(id: u32, center_id: u32, radius: f64) -> SketchEntity {
    SketchEntity::Circle {
        id,
        center_id,
        radius,
        construction: false,
    }
}

/// A sketch whose every point is pinned where it is: the source is fixed, so
/// whatever freedom the solve reports belongs to the offset.
fn pinned_sketch(entities: Vec<SketchEntity>) -> Sketch {
    let mut constraints = Vec::new();
    let mut solved_positions = HashMap::new();
    for e in &entities {
        if let SketchEntity::Point { id, x, y, .. } = e {
            constraints.push(SketchConstraint::Pinned {
                point: *id,
                x: *x,
                y: *y,
            });
            solved_positions.insert(*id, (*x, *y));
        }
    }
    // A pinned circle radius has no constraint of its own; a Radius does it.
    for e in &entities {
        if let SketchEntity::Circle { id, radius, .. } = e {
            constraints.push(SketchConstraint::Radius {
                entity: *id,
                value: *radius,
                expression: None,
                reference: false,
            });
        }
    }
    Sketch {
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
        solved_positions,
        solved_profiles: Vec::new(),
        projected: Vec::new(),
    }
}

fn square() -> Sketch {
    pinned_sketch(vec![
        point(1, 0.0, 0.0),
        point(2, 1.0, 0.0),
        point(3, 1.0, 1.0),
        point(4, 0.0, 1.0),
        line(10, 1, 2),
        line(11, 2, 3),
        line(12, 3, 4),
        line(13, 4, 1),
    ])
}

/// A slot: two horizontal lines joined by two tangent semicircles (welded
/// joints only).
fn slot() -> Sketch {
    pinned_sketch(vec![
        point(1, 0.0, 0.0),
        point(2, 2.0, 0.0),
        point(3, 2.0, 1.0),
        point(4, 0.0, 1.0),
        point(5, 2.0, 0.5),
        point(6, 0.0, 0.5),
        line(10, 1, 2),
        arc(11, 5, 2, 3),
        line(12, 3, 4),
        arc(13, 6, 4, 1),
    ])
}

/// An open L: (0,0) → (2,0) → (2,1).
fn open_l() -> Sketch {
    pinned_sketch(vec![
        point(1, 0.0, 0.0),
        point(2, 2.0, 0.0),
        point(3, 2.0, 1.0),
        line(10, 1, 2),
        line(11, 2, 3),
    ])
}

/// Offset `chain`, returning the sketch with the result applied and the index
/// of the recorded Offset constraint.
fn offset_into(s: &Sketch, chain: &[u32], d: f64, side: Side) -> (Sketch, usize) {
    let mut ids = IdAllocator::for_sketch(s, 0);
    let edit = offset(s, chain, d, side, &mut ids).expect("offsets");
    let mut after = s.clone();
    apply_edit(&mut after, &edit);
    let idx = after
        .constraints
        .iter()
        .position(|c| matches!(c, SketchConstraint::Offset { .. }))
        .expect("an offset records its Offset constraint");
    (after, idx)
}

fn solved_with(sketch: &Sketch) -> (SolvedSketch, Sketch) {
    let solved = solve_sketch(sketch);
    let mut s = sketch.clone();
    s.solved_positions = solved.positions.clone();
    (solved, s)
}

fn set_offset_value(sketch: &mut Sketch, idx: usize, v: f64) {
    sketch.constraints[idx].set_dimension_value(v);
}

fn max_moved(before: &Sketch, solved: &SolvedSketch) -> f64 {
    before
        .entities
        .iter()
        .filter_map(|e| match e {
            SketchEntity::Point { id, x, y, .. } => {
                let (sx, sy) = solved.positions[id];
                Some((sx - x).hypot(sy - y))
            }
            _ => None,
        })
        .fold(0.0, f64::max)
}

fn pos(sketch: &Sketch, id: u32) -> (f64, f64) {
    sketch.solved_positions[&id]
}

/// Every offset line's endpoints sit at `d` from the source line it links to,
/// every offset arc at `r_src ± d` from its (shared) centre.
fn assert_at_distance(sketch: &Sketch, idx: usize, d: f64) {
    let SketchConstraint::Offset { links, .. } = &sketch.constraints[idx] else {
        panic!("offset constraint");
    };
    let find = |id: u32| {
        sketch
            .entities
            .iter()
            .find(|e| e.id() == id)
            .unwrap()
            .clone()
    };
    for link in links {
        match (find(link.entity), find(link.source)) {
            (
                SketchEntity::Line {
                    start_id, end_id, ..
                },
                SketchEntity::Line {
                    start_id: s0,
                    end_id: s1,
                    ..
                },
            ) => {
                let (a, b) = (pos(sketch, s0), pos(sketch, s1));
                let (dx, dy) = (b.0 - a.0, b.1 - a.1);
                let l = dx.hypot(dy);
                for q in [start_id, end_id] {
                    let p = pos(sketch, q);
                    let dist = ((p.0 - a.0) * dy - (p.1 - a.1) * dx).abs() / l;
                    assert!(
                        (dist - d).abs() < 1e-7,
                        "line {} at {dist}, want {d}",
                        link.entity
                    );
                }
            }
            (
                SketchEntity::Arc {
                    center_id,
                    start_id,
                    end_id,
                    ..
                },
                source,
            ) => {
                let c = pos(sketch, center_id);
                let want = match source {
                    SketchEntity::Arc {
                        center_id: sc,
                        start_id: ss,
                        ..
                    } => {
                        let (c0, s0) = (pos(sketch, sc), pos(sketch, ss));
                        let r_src = (s0.0 - c0.0).hypot(s0.1 - c0.1);
                        assert!((c.0 - c0.0).hypot(c.1 - c0.1) < 1e-9, "concentric");
                        r_src + f64::from(link.sign) * d
                    }
                    SketchEntity::Point { id, .. } => {
                        let j = pos(sketch, id);
                        assert!((c.0 - j.0).hypot(c.1 - j.1) < 1e-9, "corner centred");
                        d
                    }
                    other => panic!("arc linked to {other:?}"),
                };
                for q in [start_id, end_id] {
                    let p = pos(sketch, q);
                    let r = (p.0 - c.0).hypot(p.1 - c.1);
                    assert!(
                        (r - want).abs() < 1e-7,
                        "arc {} radius {r}, want {want}",
                        link.entity
                    );
                }
            }
            (SketchEntity::Circle { .. }, SketchEntity::Circle { .. }) => {}
            (e, s) => panic!("unexpected link {e:?} from {s:?}"),
        }
    }
}

/// The three-part contract for one chain shape.
fn check_contract(s: &Sketch, chain: &[u32], d: f64, side: Side, d2: f64) {
    let (after, idx) = offset_into(s, chain, d, side);

    // 1. The fresh offset already satisfies its constraint.
    let solved = solve_sketch(&after);
    assert!(
        max_moved(&after, &solved) < 1e-9,
        "the first solve moved the offset by {}",
        max_moved(&after, &solved)
    );
    // 2. Fully constrained against the fixed source, nothing redundant.
    assert!(
        matches!(solved.status, SolveStatus::FullyConstrained),
        "status {:?}, report free {:?}",
        solved.status,
        solved.report.free
    );
    assert!(
        solved.report.redundant.is_empty(),
        "redundant rows: {:?}",
        solved.report.redundant
    );

    // 3. One edit moves the whole offset.
    let mut edited = after.clone();
    set_offset_value(&mut edited, idx, d2);
    let (solved2, edited) = solved_with(&edited);
    assert!(
        matches!(solved2.status, SolveStatus::FullyConstrained),
        "after the edit: {:?}",
        solved2.status
    );
    assert_at_distance(&edited, idx, d2);
}

#[test]
fn a_square_offset_outward_is_one_dimension_with_corner_arcs() {
    let s = square();
    let (after, idx) = offset_into(&s, &[10, 11, 12, 13], 0.1, Side::Right);
    let SketchConstraint::Offset {
        value,
        links,
        placements,
        ..
    } = &after.constraints[idx]
    else {
        unreachable!()
    };
    assert_eq!(*value, 0.1);
    assert_eq!(links.len(), 8, "four lines + four corner arcs");
    assert_eq!(placements.len(), 8, "both ends of every corner arc");
    check_contract(&s, &[10, 11, 12, 13], 0.1, Side::Right, 0.25);
}

#[test]
fn a_square_offset_inward_is_held_by_its_crossings_alone() {
    let s = square();
    let (after, idx) = offset_into(&s, &[10, 11, 12, 13], 0.1, Side::Left);
    let SketchConstraint::Offset { placements, .. } = &after.constraints[idx] else {
        unreachable!()
    };
    assert!(placements.is_empty(), "mitered corners are crossings");
    check_contract(&s, &[10, 11, 12, 13], 0.1, Side::Left, 0.3);
}

#[test]
fn a_slot_offset_keeps_its_tangent_welds_joined() {
    // Outward (Right of the CCW traversal) and inward.
    check_contract(&slot(), &[10, 11, 12, 13], 0.1, Side::Right, 0.2);
    check_contract(&slot(), &[10, 11, 12, 13], 0.1, Side::Left, 0.3);
}

#[test]
fn an_open_chain_offset_places_its_two_free_ends() {
    // Left of (0,0)→(2,0)→(2,1) is the inside of the L (a crossing joint);
    // Right is the outside (a corner arc).
    check_contract(&open_l(), &[10, 11], 0.1, Side::Left, 0.2);
    check_contract(&open_l(), &[10, 11], 0.1, Side::Right, 0.2);
}

#[test]
fn a_circle_offset_is_concentric_at_the_radius_plus_the_value() {
    let s = pinned_sketch(vec![point(1, 1.0, 2.0), circle(20, 1, 0.5)]);
    let (after, idx) = offset_into(&s, &[20], 0.25, Side::Left);
    let solved = solve_sketch(&after);
    assert!(matches!(solved.status, SolveStatus::FullyConstrained));
    let mut edited = after.clone();
    set_offset_value(&mut edited, idx, 0.4);
    let solved2 = solve_sketch(&edited);
    let new_circle = after
        .entities
        .iter()
        .filter_map(|e| match e {
            SketchEntity::Circle { id, .. } if *id != 20 => Some(*id),
            _ => None,
        })
        .next()
        .unwrap();
    assert!(
        (solved2.radii[&new_circle] - 0.9).abs() < 1e-9,
        "r = {}",
        solved2.radii[&new_circle]
    );
}

#[test]
fn moving_the_source_carries_the_offset_with_it() {
    // Unpin one source corner and pin it somewhere else: the offset follows.
    let s = square();
    let (mut after, idx) = offset_into(&s, &[10, 11, 12, 13], 0.1, Side::Left);
    for c in after.constraints.iter_mut() {
        if let SketchConstraint::Pinned { point: 3, x, y } = c {
            *x = 1.5;
            *y = 1.2;
        }
    }
    let (solved, moved) = solved_with(&after);
    assert!(
        !matches!(solved.status, SolveStatus::SolveFailed { .. }),
        "{:?}",
        solved.status
    );
    assert_at_distance(&moved, idx, 0.1);
}

/// The same square traversed clockwise, with its lines pointing every which
/// way — what the rectangle tool actually draws. Exercises reversed chain
/// items and the CCW normalization of a closed result.
fn scrambled_square() -> Sketch {
    pinned_sketch(vec![
        point(1, -0.04, 0.03),
        point(2, 0.04, -0.03),
        point(3, 0.04, 0.03),
        point(4, -0.04, -0.03),
        line(5, 1, 3),
        line(6, 3, 2),
        line(7, 4, 2),
        line(8, 1, 4),
    ])
}

#[test]
fn a_scrambled_rectangle_offset_records_its_constraint_both_ways() {
    for side in [Side::Left, Side::Right] {
        check_contract(&scrambled_square(), &[5, 6, 7, 8], 0.005, side, 0.008);
    }
}

#[test]
fn a_slot_with_reversed_lines_keeps_its_welds_both_ways() {
    let s = pinned_sketch(vec![
        point(1, 0.0, 0.0),
        point(2, 2.0, 0.0),
        point(3, 2.0, 1.0),
        point(4, 0.0, 1.0),
        point(5, 2.0, 0.5),
        point(6, 0.0, 0.5),
        line(10, 2, 1),
        arc(11, 5, 2, 3),
        line(12, 4, 3),
        arc(13, 6, 4, 1),
    ]);
    for side in [Side::Left, Side::Right] {
        for chain in [[10, 11, 12, 13], [13, 12, 11, 10]] {
            check_contract(&s, &chain, 0.1, side, 0.2);
        }
    }
}
