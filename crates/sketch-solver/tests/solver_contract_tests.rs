//! Solver output-contract and edge-case coverage.
//!
//! Three gaps this file closes:
//!
//! 1. **`SolvedSketch.radii` was asserted nowhere.** `solve_sketch` deliberately
//!    publishes solved radii and then overrides a standalone circle profile's
//!    radius with the solved value ("so the solver's output is self-consistent
//!    — a Diameter/Radius constraint actually resizes the circle",
//!    `solver.rs`). Both steps could be deleted without failing a single test:
//!    the existing profile tests read the DECLARED radius, which is equal to
//!    the solved one whenever no dimension is applied.
//!
//! 2. **No end-to-end determinism test.** `entity_mapping` proves the
//!    *layout* is deterministic, and both modules document "no `HashMap`
//!    iteration in the solve path", but nothing pinned `solve_sketch` itself to
//!    repeatable output, nor to independence from constraint declaration order.
//!
//! 3. **Degenerate geometry reached only some guards.** The `1e-15` length
//!    guards in the Parallel / Angle / Tangent / Symmetric Jacobians are the
//!    difference between a bounded solve and a NaN that propagates into the
//!    kernel; zero-length lines were only driven through Distance/EqualLines.
//!
//! Determinism: literal geometry, fixed ids, `Uuid::nil()`. No random values,
//! no time, no filesystem.

use sketch_solver::*;
use uuid::Uuid;

// ── Helpers ─────────────────────────────────────────────────────────────────

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

fn arc(id: u32, center_id: u32, start_id: u32, end_id: u32) -> SketchEntity {
    SketchEntity::Arc {
        id,
        center_id,
        start_id,
        end_id,
        construction: false,
    }
}

fn make_sketch(entities: Vec<SketchEntity>, constraints: Vec<SketchConstraint>) -> Sketch {
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
        },
        plane_origin: [0.0, 0.0, 0.0],
        plane_normal: [0.0, 0.0, 1.0],
        entities,
        constraints,
        solve_status: SolveStatus::UnderConstrained { dof: 99 },
        solved_positions: std::collections::HashMap::new(),
        solved_profiles: Vec::new(),
        projected: Vec::new(),
    }
}

fn radius_of(positions: &std::collections::HashMap<u32, (f64, f64)>, c: u32, s: u32) -> f64 {
    let (cx, cy) = positions[&c];
    let (sx, sy) = positions[&s];
    ((sx - cx).powi(2) + (sy - cy).powi(2)).sqrt()
}

// ── The `radii` output channel ──────────────────────────────────────────────

#[test]
fn a_radius_constraint_publishes_the_solved_radius_in_the_radii_map() {
    // Circle declared at r=10, constrained to r=25. The solved radius must
    // reach the caller through `radii` — points alone cannot carry it, since a
    // circle's radius is a parameter in its own right.
    let sketch = make_sketch(
        vec![point(1, 0.0, 0.0), circle(30, 1, 10.0)],
        vec![
            SketchConstraint::Pinned {
                point: 1,
                x: 0.0,
                y: 0.0,
            },
            SketchConstraint::Radius {
                entity: 30,
                value: 25.0,
                expression: None,
                reference: false,
            },
        ],
    );
    let solved = solve_sketch(&sketch);
    assert!(
        matches!(
            solved.status,
            SolveStatus::FullyConstrained | SolveStatus::UnderConstrained { .. }
        ),
        "radius dimension must be satisfiable, got {:?}",
        solved.status
    );

    let r = solved
        .radii
        .get(&30)
        .copied()
        .expect("circle 30 must appear in the solved radii map");
    assert!(
        (r - 25.0).abs() < 1e-6,
        "solved radius must be the constrained 25, got {r}"
    );
}

#[test]
fn a_diameter_constraint_resizes_the_standalone_circle_profile() {
    // `solve_sketch` overrides a standalone circle profile's radius with the
    // SOLVED radius. Without that override the profile still carries the
    // DECLARED radius (10) and the extrude downstream builds the wrong solid
    // while the solver reports success — a silent-wrong, not a loud failure.
    let sketch = make_sketch(
        vec![point(1, 4.0, 5.0), circle(30, 1, 10.0)],
        vec![
            SketchConstraint::Pinned {
                point: 1,
                x: 4.0,
                y: 5.0,
            },
            SketchConstraint::Diameter {
                entity: 30,
                value: 50.0,
                expression: None,
                reference: false,
            },
        ],
    );
    let solved = solve_sketch(&sketch);

    let profile = solved
        .profiles
        .iter()
        .find(|p| p.entity_ids == vec![30])
        .expect("standalone circle must produce a profile");
    let circle_profile = profile
        .circle
        .as_ref()
        .expect("a standalone circle profile must carry circle data");

    assert!(
        (circle_profile.radius - 25.0).abs() < 1e-6,
        "profile radius must be the SOLVED 25 (diameter 50), not the declared 10, got {}",
        circle_profile.radius
    );
    assert!(
        (circle_profile.center_u - 4.0).abs() < 1e-6
            && (circle_profile.center_v - 5.0).abs() < 1e-6,
        "profile centre must track the solved centre, got ({}, {})",
        circle_profile.center_u,
        circle_profile.center_v
    );
}

#[test]
fn equal_circles_publishes_both_solved_radii() {
    // Circle 30 dimensioned to 12; circle 31 declared at 3 and tied Equal.
    // Both entries in `radii` must agree.
    let sketch = make_sketch(
        vec![
            point(1, 0.0, 0.0),
            point(2, 100.0, 0.0),
            circle(30, 1, 5.0),
            circle(31, 2, 3.0),
        ],
        vec![
            SketchConstraint::Radius {
                entity: 30,
                value: 12.0,
                expression: None,
                reference: false,
            },
            SketchConstraint::Equal {
                entity_a: 30,
                entity_b: 31,
            },
        ],
    );
    let solved = solve_sketch(&sketch);

    let a = solved.radii[&30];
    let b = solved.radii[&31];
    assert!(
        (a - 12.0).abs() < 1e-6,
        "dimensioned circle must solve to 12, got {a}"
    );
    assert!(
        (b - 12.0).abs() < 1e-6,
        "Equal circle must follow to 12, got {b}"
    );
}

#[test]
fn arcs_carry_no_radius_param_and_are_absent_from_the_radii_map() {
    // Documented contract on `extract_radii`: an arc's radius is the
    // center→start distance, so it has no radius parameter and must NOT appear
    // in `radii`. A caller that found an arc key there would be reading a
    // fabricated value.
    let sketch = make_sketch(
        vec![
            point(1, 0.0, 0.0),
            point(2, 5.0, 0.0),
            point(3, 0.0, 5.0),
            arc(40, 1, 2, 3),
        ],
        vec![SketchConstraint::Radius {
            entity: 40,
            value: 5.0,
            expression: None,
            reference: false,
        }],
    );
    let solved = solve_sketch(&sketch);
    assert!(
        !solved.radii.contains_key(&40),
        "arc 40 must not appear in radii, got {:?}",
        solved.radii
    );
}

#[test]
fn a_radius_dimension_on_an_arc_solves_through_the_arc_radius_form() {
    // End-to-end for the RadiusArc arm: the arc's center→start distance must
    // move from 5 to 9 while the pinned center stays put.
    let sketch = make_sketch(
        vec![
            point(1, 0.0, 0.0),
            point(2, 5.0, 0.0),
            point(3, 0.0, 5.0),
            arc(40, 1, 2, 3),
        ],
        vec![
            SketchConstraint::Pinned {
                point: 1,
                x: 0.0,
                y: 0.0,
            },
            SketchConstraint::Radius {
                entity: 40,
                value: 9.0,
                expression: None,
                reference: false,
            },
        ],
    );
    let solved = solve_sketch(&sketch);
    assert!(
        matches!(
            solved.status,
            SolveStatus::FullyConstrained | SolveStatus::UnderConstrained { .. }
        ),
        "arc radius dimension must be satisfiable, got {:?}",
        solved.status
    );
    let r = radius_of(&solved.positions, 1, 2);
    assert!(
        (r - 9.0).abs() < 1e-5,
        "arc radius must solve to 9, got {r}"
    );
}

// ── Determinism ─────────────────────────────────────────────────────────────

fn rect_with_dimensions() -> Sketch {
    make_sketch(
        vec![
            point(1, 0.0, 0.0),
            point(2, 100.0, 3.0),
            point(3, 97.0, 50.0),
            point(4, 2.0, 48.0),
            line(10, 1, 2),
            line(11, 2, 3),
            line(12, 3, 4),
            line(13, 4, 1),
        ],
        vec![
            SketchConstraint::Pinned {
                point: 1,
                x: 0.0,
                y: 0.0,
            },
            SketchConstraint::Horizontal { entity: 10 },
            SketchConstraint::Vertical { entity: 11 },
            SketchConstraint::Horizontal { entity: 12 },
            SketchConstraint::Vertical { entity: 13 },
            SketchConstraint::Distance {
                entity_a: 1,
                entity_b: 2,
                value: 100.0,
                expression: None,
                reference: false,
            },
            SketchConstraint::Distance {
                entity_a: 2,
                entity_b: 3,
                value: 50.0,
                expression: None,
                reference: false,
            },
        ],
    )
}

#[test]
fn solving_the_same_sketch_twice_gives_bit_identical_output() {
    // The solve path documents "no `HashMap` iteration" precisely so that
    // repeated solves agree exactly. Bit-identity (not a tolerance) is the
    // claim, so it is what gets asserted.
    let sketch = rect_with_dimensions();
    let a = solve_sketch(&sketch);
    let b = solve_sketch(&sketch);

    assert_eq!(
        format!("{:?}", a.status),
        format!("{:?}", b.status),
        "status must be reproducible"
    );
    assert_eq!(
        a.positions.len(),
        b.positions.len(),
        "position count must be reproducible"
    );
    for (id, pos) in &a.positions {
        let other = b.positions[id];
        assert_eq!(
            pos.0.to_bits(),
            other.0.to_bits(),
            "point {id} x differs between identical solves: {} vs {}",
            pos.0,
            other.0
        );
        assert_eq!(
            pos.1.to_bits(),
            other.1.to_bits(),
            "point {id} y differs between identical solves: {} vs {}",
            pos.1,
            other.1
        );
    }
}

#[test]
fn a_solved_sketch_is_a_fixpoint_when_fed_back_in() {
    // Re-solving from the solved configuration must not drift. The proximal
    // anchor makes the solved point the nearest solution, so a second pass has
    // nothing to do; drift here would compound over the UI's solve-per-drag-step
    // loop.
    let sketch = rect_with_dimensions();
    let first = solve_sketch(&sketch);

    // Rebuild the entity list at the solved positions, constraints unchanged.
    let refed: Vec<SketchEntity> = sketch
        .entities
        .iter()
        .map(|e| match e {
            SketchEntity::Point {
                id, construction, ..
            } => {
                let (x, y) = first.positions[id];
                SketchEntity::Point {
                    id: *id,
                    x,
                    y,
                    construction: *construction,
                }
            }
            other => other.clone(),
        })
        .collect();
    let second = solve_sketch(&make_sketch(refed, sketch.constraints.clone()));

    for (id, pos) in &first.positions {
        let other = second.positions[id];
        assert!(
            (pos.0 - other.0).abs() < 1e-9 && (pos.1 - other.1).abs() < 1e-9,
            "re-solving drifted point {id}: {pos:?} → {other:?}"
        );
    }
}

#[test]
fn constraint_declaration_order_does_not_change_the_solution() {
    // Residual rows are assembled in declaration order, so reordering permutes
    // the Jacobian's ROWS. Row order must not change the least-squares
    // solution — if it does, the solve depends on the order the UI happened to
    // append constraints in, and an undo/redo round-trip could move geometry.
    let sketch = rect_with_dimensions();
    let forward = solve_sketch(&sketch);

    let mut reversed_constraints = sketch.constraints.clone();
    reversed_constraints.reverse();
    let reversed = solve_sketch(&make_sketch(sketch.entities.clone(), reversed_constraints));

    assert_eq!(
        format!("{:?}", forward.status),
        format!("{:?}", reversed.status),
        "status must not depend on constraint order"
    );
    for (id, pos) in &forward.positions {
        let other = reversed.positions[id];
        assert!(
            (pos.0 - other.0).abs() < 1e-6 && (pos.1 - other.1).abs() < 1e-6,
            "point {id} moved when constraints were reordered: {pos:?} vs {other:?}"
        );
    }
}

// ── Degrees of freedom ──────────────────────────────────────────────────────

#[test]
fn a_free_circle_reports_three_degrees_of_freedom() {
    // Centre x, centre y, radius. If the radius parameter were omitted from
    // the dof count the UI would tell the user a circle is fully constrained
    // while its size is still free.
    let solved = solve_sketch(&make_sketch(
        vec![point(1, 0.0, 0.0), circle(30, 1, 5.0)],
        vec![],
    ));
    match solved.status {
        SolveStatus::UnderConstrained { dof } => assert_eq!(
            dof, 3,
            "a free circle has 3 dof (cx, cy, r), reported {dof}"
        ),
        other => panic!("expected UnderConstrained, got {other:?}"),
    }
}

#[test]
fn pinning_a_circle_centre_leaves_only_the_radius_free() {
    let solved = solve_sketch(&make_sketch(
        vec![point(1, 0.0, 0.0), circle(30, 1, 5.0)],
        vec![SketchConstraint::Pinned {
            point: 1,
            x: 0.0,
            y: 0.0,
        }],
    ));
    match solved.status {
        SolveStatus::UnderConstrained { dof } => assert_eq!(
            dof, 1,
            "a pinned circle centre leaves 1 dof (radius), reported {dof}"
        ),
        other => panic!("expected UnderConstrained with dof 1, got {other:?}"),
    }
}

#[test]
fn dimensioning_a_pinned_circle_reaches_zero_degrees_of_freedom() {
    let solved = solve_sketch(&make_sketch(
        vec![point(1, 0.0, 0.0), circle(30, 1, 5.0)],
        vec![
            SketchConstraint::Pinned {
                point: 1,
                x: 0.0,
                y: 0.0,
            },
            SketchConstraint::Radius {
                entity: 30,
                value: 12.0,
                expression: None,
                reference: false,
            },
        ],
    ));
    assert!(
        matches!(solved.status, SolveStatus::FullyConstrained),
        "a pinned, dimensioned circle is fully constrained, got {:?}",
        solved.status
    );
}

// ── Degenerate geometry: the length guards ──────────────────────────────────

/// Every point in the solve stays finite. A NaN here would be published
/// straight into the kernel as sketch geometry.
fn assert_all_finite(solved: &SolvedSketch, what: &str) {
    for (id, (x, y)) in &solved.positions {
        assert!(
            x.is_finite() && y.is_finite(),
            "{what}: point {id} became non-finite: ({x}, {y})"
        );
    }
    for (id, r) in &solved.radii {
        assert!(
            r.is_finite(),
            "{what}: radius of {id} became non-finite: {r}"
        );
    }
}

#[test]
fn parallel_to_a_zero_length_line_stays_finite() {
    // Line 11 has both endpoints on top of each other: its direction is
    // undefined and the Parallel Jacobian's 1e-15 guard is the only thing
    // between this and a division by zero.
    let solved = solve_sketch(&make_sketch(
        vec![
            point(1, 0.0, 0.0),
            point(2, 10.0, 0.0),
            point(3, 5.0, 5.0),
            point(4, 5.0, 5.0), // coincident with 3 → zero-length line
            line(10, 1, 2),
            line(11, 3, 4),
        ],
        vec![
            SketchConstraint::Pinned {
                point: 1,
                x: 0.0,
                y: 0.0,
            },
            SketchConstraint::Parallel {
                line_a: 10,
                line_b: 11,
            },
        ],
    ));
    assert_all_finite(&solved, "parallel to zero-length line");
}

#[test]
fn an_angle_dimension_against_a_zero_length_line_stays_finite() {
    let solved = solve_sketch(&make_sketch(
        vec![
            point(1, 0.0, 0.0),
            point(2, 10.0, 0.0),
            point(3, 5.0, 5.0),
            point(4, 5.0, 5.0),
            line(10, 1, 2),
            line(11, 3, 4),
        ],
        vec![
            SketchConstraint::Pinned {
                point: 1,
                x: 0.0,
                y: 0.0,
            },
            SketchConstraint::Angle {
                line_a: 10,
                line_b: 11,
                value_degrees: 45.0,
                expression: None,
                reference: false,
            },
        ],
    ));
    assert_all_finite(&solved, "angle against zero-length line");
}

#[test]
fn tangent_to_a_zero_radius_circle_stays_finite() {
    let solved = solve_sketch(&make_sketch(
        vec![
            point(1, 0.0, 0.0),
            point(2, 10.0, 0.0),
            point(3, 5.0, 5.0),
            line(10, 1, 2),
            circle(30, 3, 0.0), // degenerate: zero radius
        ],
        vec![
            SketchConstraint::Pinned {
                point: 1,
                x: 0.0,
                y: 0.0,
            },
            SketchConstraint::Tangent {
                line: 10,
                curve: 30,
            },
        ],
    ));
    assert_all_finite(&solved, "tangent to zero-radius circle");
}

#[test]
fn symmetry_about_a_zero_length_line_stays_finite() {
    // The Symmetric residual divides by the symmetry line's length; both its
    // rows are guarded to zero when that length collapses.
    let solved = solve_sketch(&make_sketch(
        vec![
            point(1, -5.0, 0.0),
            point(2, 5.0, 0.0),
            point(3, 0.0, 0.0),
            point(4, 0.0, 0.0), // coincident with 3
            line(10, 3, 4),
        ],
        vec![SketchConstraint::Symmetric {
            entity_a: 1,
            entity_b: 2,
            symmetry_line: 10,
        }],
    ));
    assert_all_finite(&solved, "symmetric about zero-length line");
}

#[test]
fn a_point_constrained_onto_a_zero_radius_arc_stays_finite() {
    // Arc whose centre and start coincide → radius 0, exercising the
    // OnEntityArc guard on BOTH the point term and the radius term.
    let solved = solve_sketch(&make_sketch(
        vec![
            point(1, 0.0, 0.0),
            point(2, 0.0, 0.0), // start == centre → zero radius
            point(3, 0.0, 0.0),
            point(4, 7.0, 7.0),
            arc(40, 1, 2, 3),
        ],
        vec![SketchConstraint::OnEntity {
            point: 4,
            entity: 40,
        }],
    ));
    assert_all_finite(&solved, "point on zero-radius arc");
}

// ── Conflict reporting ──────────────────────────────────────────────────────

#[test]
fn conflicting_radius_dimensions_report_in_range_constraint_indices() {
    // Two contradictory radius dimensions on one circle. Whatever the
    // classifier decides, every reported conflict index must be a valid index
    // into the sketch's constraint list — the UI dereferences them directly to
    // highlight badges, so an out-of-range index is a panic in the app.
    let constraints = vec![
        SketchConstraint::Pinned {
            point: 1,
            x: 0.0,
            y: 0.0,
        },
        SketchConstraint::Radius {
            entity: 30,
            value: 10.0,
            expression: None,
            reference: false,
        },
        SketchConstraint::Radius {
            entity: 30,
            value: 40.0,
            expression: None,
            reference: false,
        },
    ];
    let n = constraints.len();
    let solved = solve_sketch(&make_sketch(
        vec![point(1, 0.0, 0.0), circle(30, 1, 5.0)],
        constraints,
    ));

    match &solved.status {
        SolveStatus::OverConstrained { conflicts } => {
            assert!(
                !conflicts.is_empty(),
                "an over-constrained solve must name its conflicts"
            );
            for &c in conflicts {
                assert!(
                    (c as usize) < n,
                    "conflict index {c} is out of range for {n} constraints"
                );
            }
            let mut sorted = conflicts.clone();
            sorted.sort_unstable();
            sorted.dedup();
            assert_eq!(
                sorted.len(),
                conflicts.len(),
                "conflict indices must be deduplicated, got {conflicts:?}"
            );
        }
        other => panic!("contradictory radius dimensions must be OverConstrained, got {other:?}"),
    }
}
