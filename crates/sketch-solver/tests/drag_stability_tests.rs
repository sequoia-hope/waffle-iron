//! Regression tests for specs/sketch_drag_stability.md.
//!
//! Reproduction (2026-07-04): two origin-centered centerpoint rectangles,
//! Equal on two adjacent inner edges, inner corner dragged → LM walks the
//! free outer rectangle out along the flat null-space valley (accepted,
//! cost-decreasing steps) and the UI feedback loop amplifies it to 1e8.
//!
//! The fixture mirrors tools.js emitCenterRectangle exactly; the drive loop
//! mirrors store.svelte.js dragSketchPoint → triggerSolve → sketchSolved
//! (solve output becomes the next solve's input).

use sketch_solver::*;
use std::collections::HashMap;
use uuid::Uuid;

fn dummy_geom_ref() -> GeomRef {
    GeomRef {
        kind: TopoKind::Face,
        anchor: Anchor::Datum {
            datum_id: Uuid::nil(),
        },
        selector: Selector::Role {
            role: Role::ProfileFace,
            index: 0,
        },
        policy: ResolvePolicy::Strict,
    }
}

fn pt(id: u32, x: f64, y: f64, construction: bool) -> SketchEntity {
    SketchEntity::Point {
        id,
        x,
        y,
        construction,
    }
}

fn line(id: u32, start: u32, end: u32) -> SketchEntity {
    SketchEntity::Line {
        id,
        start_id: start,
        end_id: end,
        construction: false,
    }
}

fn make_sketch(entities: Vec<SketchEntity>, constraints: Vec<SketchConstraint>) -> Sketch {
    Sketch {
        id: Uuid::nil(),
        plane: dummy_geom_ref(),
        plane_origin: [0.0, 0.0, 0.0],
        plane_normal: [0.0, 0.0, 1.0],
        entities,
        constraints,
        solve_status: SolveStatus::UnderConstrained { dof: 99 },
        solved_positions: HashMap::new(),
        solved_profiles: Vec::new(),
        projected: Vec::new(),
    }
}

struct Fixture {
    entities: Vec<SketchEntity>,
    constraints: Vec<SketchConstraint>,
    center: u32,
    /// dragged corner of the inner (Equal-constrained) rectangle
    drag_pt: u32,
    /// a corner of the outer free rectangle (I2 oracle)
    outer_corner: u32,
}

/// Two centerpoint rectangles sharing the center point (id 1), built exactly
/// as tools.js createRectangleEdges + emitCenterRectangle emit them:
/// 4 corners, 4 lines, H/V on the edge lines, two construction midpoints tied
/// with Midpoint + VerticalPoints/HorizontalPoints to the center. Inner
/// rectangle gets Equal on two adjacent edges (the user repro).
fn two_center_rects(s: f64) -> Fixture {
    let mut entities = vec![pt(1, 0.0, 0.0, false)];
    let mut constraints = Vec::new();
    let mut id = 2u32;
    let mut alloc = || {
        let v = id;
        id += 1;
        v
    };
    let mut drag_pt = 0;
    let mut outer_corner = 0;

    for (hx, hy, inner) in [(7.5 * s, 7.5 * s, true), (10.0 * s, 10.0 * s, false)] {
        let (p1, p2, p3, p4) = (alloc(), alloc(), alloc(), alloc());
        entities.push(pt(p1, -hx, -hy, false));
        entities.push(pt(p2, hx, -hy, false));
        entities.push(pt(p3, hx, hy, false));
        entities.push(pt(p4, -hx, hy, false));
        let (l1, l2, l3, l4) = (alloc(), alloc(), alloc(), alloc());
        entities.push(line(l1, p1, p2));
        entities.push(line(l2, p2, p3));
        entities.push(line(l3, p3, p4));
        entities.push(line(l4, p4, p1));
        constraints.push(SketchConstraint::Horizontal { entity: l1 });
        constraints.push(SketchConstraint::Horizontal { entity: l3 });
        constraints.push(SketchConstraint::Vertical { entity: l2 });
        constraints.push(SketchConstraint::Vertical { entity: l4 });
        let (mt, ml) = (alloc(), alloc());
        entities.push(pt(mt, 0.0, hy, true));
        entities.push(pt(ml, -hx, 0.0, true));
        constraints.push(SketchConstraint::Midpoint {
            point: mt,
            line: l3,
        });
        constraints.push(SketchConstraint::Midpoint {
            point: ml,
            line: l4,
        });
        constraints.push(SketchConstraint::VerticalPoints {
            point_a: 1,
            point_b: mt,
        });
        constraints.push(SketchConstraint::HorizontalPoints {
            point_a: 1,
            point_b: ml,
        });
        if inner {
            constraints.push(SketchConstraint::Equal {
                entity_a: l1,
                entity_b: l2,
            });
            drag_pt = p3;
        } else {
            outer_corner = p3;
        }
    }
    Fixture {
        entities,
        constraints,
        center: 1,
        drag_pt,
        outer_corner,
    }
}

/// One UI-loop drag step: write `mouse` into the dragged point's entity
/// coords, append Dragged pins (drag point + legacy origin pin on the shared
/// center, as mapConstraintForBridge produces today), solve, return positions.
fn ui_solve_step(fixture: &Fixture, positions: &HashMap<u32, (f64, f64)>) -> SolvedSketch {
    let entities: Vec<SketchEntity> = fixture
        .entities
        .iter()
        .map(|e| match e {
            SketchEntity::Point {
                id, construction, ..
            } => {
                let (x, y) = positions[id];
                pt(*id, x, y, *construction)
            }
            other => other.clone(),
        })
        .collect();
    let mut constraints = fixture.constraints.clone();
    constraints.push(SketchConstraint::Dragged {
        point: fixture.center,
    });
    constraints.push(SketchConstraint::Dragged {
        point: fixture.drag_pt,
    });
    solve_sketch(&make_sketch(entities, constraints))
}

/// Drive the 120-step drag loop at scale `s`; return (max |coordinate| seen,
/// worst center drift). Mirrors the production feedback loop: solved
/// positions are applied unconditionally as the next step's input.
fn run_drag_loop(s: f64) -> (f64, f64) {
    let fixture = two_center_rects(s);
    let mut positions: HashMap<u32, (f64, f64)> = fixture
        .entities
        .iter()
        .filter_map(|e| match e {
            SketchEntity::Point { id, x, y, .. } => Some((*id, (*x, *y))),
            _ => None,
        })
        .collect();
    let start = positions[&fixture.drag_pt];
    let mut max_coord = 0.0f64;
    let mut worst_center = 0.0f64;

    for step in 0..120usize {
        let t = step as f64;
        // ~0.4mm/frame pull along +x with ±1mm vertical wiggle, like a human drag
        let mouse = (start.0 + 0.4 * s * t, start.1 + 1.0 * s * (t * 0.7).sin());
        positions.insert(fixture.drag_pt, mouse);
        let solved = ui_solve_step(&fixture, &positions);
        for (id, p) in &solved.positions {
            assert!(
                p.0.is_finite() && p.1.is_finite(),
                "non-finite coordinate for point {id} at step {step}: {p:?}"
            );
            positions.insert(*id, *p);
        }
        let step_max = solved
            .positions
            .values()
            .flat_map(|(x, y)| [x.abs(), y.abs()])
            .fold(0.0f64, f64::max);
        max_coord = max_coord.max(step_max);
        let c = positions[&fixture.center];
        worst_center = worst_center.max((c.0 * c.0 + c.1 * c.1).sqrt());
    }
    (max_coord, worst_center)
}

/// Invariant I1 (spec §4): the drag path reaches ~55·s from origin; solved
/// geometry must stay within 10× that envelope, at both unit scales.
#[test]
fn drag_loop_two_center_rects_stays_bounded_mm_scale() {
    let (max_coord, _) = run_drag_loop(1.0);
    assert!(
        max_coord < 550.0,
        "sketch geometry exploded during drag: max |coordinate| = {max_coord:.3} \
         (fixture half-extent 10, drag path max ~55)"
    );
}

#[test]
fn drag_loop_two_center_rects_stays_bounded_meter_scale() {
    // A14.1: production sketch units are meters — the user's 15mm square is
    // 0.015 in solver space.
    let (max_coord, _) = run_drag_loop(0.001);
    assert!(
        max_coord < 0.55,
        "sketch geometry exploded during drag at meter scale: max |coordinate| = {max_coord:.6}"
    );
}

/// Invariant I2 (spec §4): a drag target CONSISTENT with the constraint set
/// (corner moved along the square's diagonal DOF) must not move the free
/// outer rectangle at all — the nearest solution leaves null-space bodies
/// where they are.
#[test]
fn consistent_drag_leaves_free_body_untouched() {
    let fixture = two_center_rects(1.0);
    let mut positions: HashMap<u32, (f64, f64)> = fixture
        .entities
        .iter()
        .filter_map(|e| match e {
            SketchEntity::Point { id, x, y, .. } => Some((*id, (*x, *y))),
            _ => None,
        })
        .collect();
    // Diagonal growth 7.5 → 8.0 keeps center@origin + Equal exactly satisfiable.
    positions.insert(fixture.drag_pt, (8.0, 8.0));
    let solved = ui_solve_step(&fixture, &positions);

    let before = (10.0, 10.0);
    let after = solved.positions[&fixture.outer_corner];
    let moved = ((after.0 - before.0).powi(2) + (after.1 - before.1).powi(2)).sqrt();
    assert!(
        moved < 1e-3,
        "free outer rectangle moved {moved:.6} during a consistent inner-square drag \
         (outer corner {before:?} → {after:?})"
    );
    // And the drag actually happened: inner corner reached the target.
    let corner = solved.positions[&fixture.drag_pt];
    assert!(
        (corner.0 - 8.0).abs() < 1e-2 && (corner.1 - 8.0).abs() < 1e-2,
        "dragged corner should reach the consistent target (8,8), got {corner:?}"
    );
}

/// Invariant I4 (spec §4): when classification is SolveFailed, the returned
/// positions must be the INPUT positions — a failed solve is inert, never a
/// source of new geometry. (Triangle-inequality-violating distances: 3
/// independent rows, unsatisfiable.)
#[test]
fn failed_solve_returns_input_positions() {
    let entities = vec![
        pt(1, 0.0, 0.0, false),
        pt(2, 5.0, 0.0, false),
        pt(3, 2.5, 2.0, false),
    ];
    let constraints = vec![
        SketchConstraint::Distance {
            entity_a: 1,
            entity_b: 2,
            value: 10.0,
        },
        SketchConstraint::Distance {
            entity_a: 2,
            entity_b: 3,
            value: 2.0,
        },
        SketchConstraint::Distance {
            entity_a: 1,
            entity_b: 3,
            value: 2.0,
        },
    ];
    let solved = solve_sketch(&make_sketch(entities, constraints));
    if let SolveStatus::SolveFailed { .. } = solved.status {
        assert_eq!(
            solved.positions[&1],
            (0.0, 0.0),
            "SolveFailed must echo input positions"
        );
        assert_eq!(solved.positions[&2], (5.0, 0.0));
        assert_eq!(solved.positions[&3], (2.5, 2.0));
    } else {
        // If the classifier calls this OverConstrained instead, positions are
        // a compromise — that path is exercised elsewhere; nothing to assert.
        println!("fixture classified as {:?}, not SolveFailed", solved.status);
    }
}

// ── Adversarial (FIP §6): pathological drag targets ─────────────────────────

/// Dragging the corner ONTO the adjacent corner collapses an edge to zero
/// length (EqualLines/Distance Jacobians hit their 1e-15 guards). The solve
/// may not satisfy everything, but it must stay finite and bounded.
#[test]
fn drag_corner_onto_adjacent_corner_stays_finite() {
    let fixture = two_center_rects(1.0);
    let mut positions: HashMap<u32, (f64, f64)> = fixture
        .entities
        .iter()
        .filter_map(|e| match e {
            SketchEntity::Point { id, x, y, .. } => Some((*id, (*x, *y))),
            _ => None,
        })
        .collect();
    // Inner p3 (7.5,7.5) dragged exactly onto inner p2 (7.5,-7.5): the right
    // edge becomes zero-length while Equal demands it match the bottom edge.
    positions.insert(fixture.drag_pt, (7.5, -7.5));
    let solved = ui_solve_step(&fixture, &positions);
    for (id, p) in &solved.positions {
        assert!(
            p.0.is_finite() && p.1.is_finite(),
            "non-finite coordinate for point {id}: {p:?}"
        );
        assert!(
            p.0.abs() < 1e3 && p.1.abs() < 1e3,
            "runaway coordinate for point {id}: {p:?}"
        );
    }
}

/// Dragging the corner onto the shared CENTER point (origin) — every inner
/// dimension collapses toward zero simultaneously. Must stay finite/bounded.
#[test]
fn drag_corner_onto_center_stays_finite() {
    let fixture = two_center_rects(1.0);
    let mut positions: HashMap<u32, (f64, f64)> = fixture
        .entities
        .iter()
        .filter_map(|e| match e {
            SketchEntity::Point { id, x, y, .. } => Some((*id, (*x, *y))),
            _ => None,
        })
        .collect();
    positions.insert(fixture.drag_pt, (0.0, 0.0));
    let solved = ui_solve_step(&fixture, &positions);
    for (id, p) in &solved.positions {
        assert!(
            p.0.is_finite() && p.1.is_finite(),
            "non-finite coordinate for point {id}: {p:?}"
        );
        assert!(
            p.0.abs() < 1e3 && p.1.abs() < 1e3,
            "runaway coordinate for point {id}: {p:?}"
        );
    }
}

/// Near-tolerance drag: a target only 1e-7 off the current position (below
/// SOLVE_TOL) must not perturb anything measurably.
#[test]
fn subtolerance_drag_is_a_noop() {
    let fixture = two_center_rects(1.0);
    let mut positions: HashMap<u32, (f64, f64)> = fixture
        .entities
        .iter()
        .filter_map(|e| match e {
            SketchEntity::Point { id, x, y, .. } => Some((*id, (*x, *y))),
            _ => None,
        })
        .collect();
    positions.insert(fixture.drag_pt, (7.5 + 1e-7, 7.5));
    let solved = ui_solve_step(&fixture, &positions);
    let outer = solved.positions[&fixture.outer_corner];
    assert!(
        (outer.0 - 10.0).abs() < 1e-4 && (outer.1 - 10.0).abs() < 1e-4,
        "sub-tolerance drag moved the free rectangle: {outer:?}"
    );
}

/// OverConstrained.conflicts must be CONSTRAINT indices, not residual ROW
/// indices. A 2-row constraint (Midpoint) precedes two conflicting Distance
/// constraints: their rows are 2 and 3, but their constraint indices are 1
/// and 2 — the row-index bug reported index 3 (out of range) and highlighted
/// the wrong badge for index 2.
#[test]
fn conflicts_are_constraint_indices_not_row_indices() {
    let entities = vec![
        pt(1, 0.0, 0.0, false),
        pt(2, 5.0, 0.0, false),
        pt(3, 2.0, 1.0, false),
        line(10, 1, 2),
    ];
    let constraints = vec![
        SketchConstraint::Midpoint { point: 3, line: 10 }, // [0], 2 rows
        SketchConstraint::Distance {
            entity_a: 1,
            entity_b: 2,
            value: 10.0,
        }, // [1], row 2
        SketchConstraint::Distance {
            entity_a: 1,
            entity_b: 2,
            value: 20.0,
        }, // [2], row 3
    ];
    let n_constraints = constraints.len() as u32;
    let solved = solve_sketch(&make_sketch(entities, constraints));
    let SolveStatus::OverConstrained { conflicts } = &solved.status else {
        panic!("expected OverConstrained, got {:?}", solved.status);
    };
    assert!(
        !conflicts.is_empty(),
        "conflicting distances must be reported"
    );
    for &idx in conflicts {
        assert!(
            idx < n_constraints,
            "conflict index {idx} out of range — row index leaked (constraints: {n_constraints})"
        );
    }
    // Both irreconcilable Distance constraints — and only they — conflict.
    let mut sorted = conflicts.clone();
    sorted.sort_unstable();
    sorted.dedup();
    assert_eq!(
        sorted,
        vec![1, 2],
        "conflicts must identify the two Distance constraints (indices 1,2), got {conflicts:?}"
    );
}
