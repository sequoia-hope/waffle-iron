//! Tests for specs/pinned_constraint.md — explicit-target point lock.
//!
//! The defect being fixed: origin snaps were lowered to `Dragged {point}`,
//! which snapshots the point's CURRENT position at weight 1/20 — a drifting
//! soft anchor, not a lock. `Pinned {point, x, y}` stores the target and
//! holds at weight 1.0.

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

// ── Canonical (spec §5) ─────────────────────────────────────────────────────

/// Invariant I1: the TARGET is authoritative — a point whose current
/// coordinates differ from the pin target must solve TO the target.
/// (This is the exact behavior the `Dragged` lowering cannot express: it
/// snapshots current coordinates, so this test fails under it.)
#[test]
fn pinned_point_solves_to_explicit_target() {
    let sketch = make_sketch(
        vec![pt(1, 2.0, 3.0, false)],
        vec![SketchConstraint::Pinned {
            point: 1,
            x: 5.0,
            y: 7.0,
        }],
    );
    let solved = solve_sketch(&sketch);
    let p = solved.positions[&1];
    assert!(
        (p.0 - 5.0).abs() < 1e-6 && (p.1 - 7.0).abs() < 1e-6,
        "pinned point must solve to the explicit target (5,7), got {p:?}"
    );
    assert!(
        matches!(solved.status, SolveStatus::FullyConstrained),
        "a lone pinned point has 0 dof, got {:?}",
        solved.status
    );
}

/// Invariant I4: Pinned removes exactly 2 dof.
#[test]
fn pinned_dof_accounting() {
    // Two free points + line: 4 dof. Pin one point: 2 dof left.
    let sketch = make_sketch(
        vec![
            pt(1, 0.0, 0.0, false),
            pt(2, 4.0, 0.0, false),
            line(10, 1, 2),
        ],
        vec![SketchConstraint::Pinned {
            point: 1,
            x: 0.0,
            y: 0.0,
        }],
    );
    let solved = solve_sketch(&sketch);
    assert!(
        matches!(solved.status, SolveStatus::UnderConstrained { dof: 2 }),
        "expected UnderConstrained(dof=2), got {:?}",
        solved.status
    );
}

// ── Branch B2/B3 (spec §3) ──────────────────────────────────────────────────

/// B2: pin wins against a live drag on connected geometry. Distance(10)
/// between pinned A and dragged B; drag target pulls B to 20 units out.
/// The pin (w=1.0) must hold A within (w_drag/w_pin)² x offset of target.
#[test]
fn pin_wins_tug_of_war_with_drag() {
    let entities = vec![pt(1, 0.0, 0.0, false), pt(2, 20.0, 0.0, false)];
    let constraints = vec![
        SketchConstraint::Pinned {
            point: 1,
            x: 0.0,
            y: 0.0,
        },
        SketchConstraint::Distance {
            entity_a: 1,
            entity_b: 2,
            value: 10.0,
            expression: None,
            reference: false,
        },
        SketchConstraint::Dragged { point: 2 }, // pinned at its current (20,0)
    ];
    let solved = solve_sketch(&make_sketch(entities, constraints));
    let a = solved.positions[&1];
    // Drag offset is 10 (B wants 20, distance allows 10). Sag bound:
    // (1/20)^2 * 10 = 0.025, with slack for LM tolerance.
    let sag = (a.0 * a.0 + a.1 * a.1).sqrt();
    assert!(
        sag < 0.05,
        "pinned point sagged {sag:.4} against a live drag — pin must dominate"
    );
}

/// B3: two pins on the same point at different targets → OverConstrained,
/// not a silent override.
#[test]
fn conflicting_pins_report_overconstrained() {
    let sketch = make_sketch(
        vec![pt(1, 0.0, 0.0, false)],
        vec![
            SketchConstraint::Pinned {
                point: 1,
                x: 0.0,
                y: 0.0,
            },
            SketchConstraint::Pinned {
                point: 1,
                x: 5.0,
                y: 0.0,
            },
        ],
    );
    let solved = solve_sketch(&sketch);
    assert!(
        matches!(solved.status, SolveStatus::OverConstrained { .. }),
        "conflicting pins must classify OverConstrained, got {:?}",
        solved.status
    );
}

// ── Regression: the user's origin-lock scenario (spec §5) ──────────────────

struct Fixture {
    entities: Vec<SketchEntity>,
    constraints: Vec<SketchConstraint>,
    center: u32,
    drag_pt: u32,
}

/// Origin-centered centerpoint square (tools.js emission), Equal on two
/// adjacent edges, center pinned at the ORIGIN via `Pinned` (the fixed
/// bridge lowering of the origin snap's WhereDragged{x:0,y:0}).
fn origin_pinned_square() -> Fixture {
    let mut entities = vec![pt(1, 0.0, 0.0, false)];
    let mut constraints = Vec::new();
    let mut id = 2u32;
    let mut alloc = || {
        let v = id;
        id += 1;
        v
    };
    let hx = 7.5;
    let hy = 7.5;
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
    constraints.push(SketchConstraint::Equal {
        entity_a: l1,
        entity_b: l2,
    });
    constraints.push(SketchConstraint::Pinned {
        point: 1,
        x: 0.0,
        y: 0.0,
    });
    Fixture {
        entities,
        constraints,
        center: 1,
        drag_pt: p3,
    }
}

fn solve_with_positions(
    fixture: &Fixture,
    positions: &HashMap<u32, (f64, f64)>,
    with_drag: bool,
) -> SolvedSketch {
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
    if with_drag {
        constraints.push(SketchConstraint::Dragged {
            point: fixture.drag_pt,
        });
    }
    solve_sketch(&make_sketch(entities, constraints))
}

/// Invariants I2 + I3: 120-step corner drag with a wildly off-manifold mouse
/// path. Center excursion stays bounded (< 1) throughout — no ratchet — and
/// the release solve returns it to the origin within SOLVE_TOL. Under the
/// old Dragged lowering the center walks 33 units away.
#[test]
fn origin_pin_holds_through_drag_and_release() {
    let fixture = origin_pinned_square();
    let mut positions: HashMap<u32, (f64, f64)> = fixture
        .entities
        .iter()
        .filter_map(|e| match e {
            SketchEntity::Point { id, x, y, .. } => Some((*id, (*x, *y))),
            _ => None,
        })
        .collect();
    let start = positions[&fixture.drag_pt];
    let mut worst_center = 0.0f64;
    for step in 0..120usize {
        let t = step as f64;
        let mouse = (start.0 + 0.4 * t, start.1 + 1.0 * (t * 0.7).sin());
        positions.insert(fixture.drag_pt, mouse);
        let solved = solve_with_positions(&fixture, &positions, true);
        for (id, p) in &solved.positions {
            positions.insert(*id, *p);
        }
        let c = positions[&fixture.center];
        worst_center = worst_center.max((c.0 * c.0 + c.1 * c.1).sqrt());
    }
    assert!(
        worst_center < 1.0,
        "origin-pinned center drifted {worst_center:.3} during drag — must stay bounded"
    );

    // Release: drop the drag constraint, solve once more.
    let released = solve_with_positions(&fixture, &positions, false);
    let c = released.positions[&fixture.center];
    let dist = (c.0 * c.0 + c.1 * c.1).sqrt();
    assert!(
        dist < 1e-6,
        "after release the pinned center must return to the origin, got ({}, {})",
        c.0,
        c.1
    );
}

// ── Failure modes (spec §6) ─────────────────────────────────────────────────

#[test]
fn pinned_unknown_point_fails_loud() {
    let sketch = make_sketch(
        vec![pt(1, 0.0, 0.0, false)],
        vec![SketchConstraint::Pinned {
            point: 99,
            x: 0.0,
            y: 0.0,
        }],
    );
    let solved = solve_sketch(&sketch);
    assert!(
        matches!(solved.status, SolveStatus::SolveFailed { .. }),
        "unknown point id must be SolveFailed, got {:?}",
        solved.status
    );
}

#[test]
fn pinned_nonfinite_target_fails_loud() {
    let sketch = make_sketch(
        vec![pt(1, 0.0, 0.0, false)],
        vec![SketchConstraint::Pinned {
            point: 1,
            x: f64::NAN,
            y: 0.0,
        }],
    );
    let solved = solve_sketch(&sketch);
    assert!(
        matches!(solved.status, SolveStatus::SolveFailed { .. }),
        "non-finite pin target must be SolveFailed, got {:?}",
        solved.status
    );
}
