//! S2 — solver state as data (`specs/agent_mechanical_design.md` §10.2).
//!
//! Every assertion here is about `SolvedSketch.report`, the data the solver
//! used to compute and discard: per-constraint residuals, what moved, the
//! null-space basis of what is still free, which constraints are dependent,
//! and why LM stopped. The four pins the increment was scoped on are
//! `a_fully_constrained_rectangle_has_zero_degrees_of_freedom`,
//! `removing_one_dimension_leaves_exactly_one_free_direction`,
//! `a_redundant_equal_length_is_flagged_as_dependent` and
//! `a_conflicting_dimension_pair_is_named_in_conflicts`.
//!
//! Determinism: literal geometry, fixed ids, `Uuid::nil()`. No random values,
//! no time, no filesystem.

use sketch_solver::*;
use std::collections::HashMap;
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
            scope: None,
        },
        plane_origin: [0.0, 0.0, 0.0],
        plane_normal: [0.0, 0.0, 1.0],
        plane_x_axis: None,
        entities,
        constraints,
        solve_status: SolveStatus::Unsolved,
        solved_positions: HashMap::new(),
        solved_profiles: Vec::new(),
        projected: Vec::new(),
    }
}

/// A unit square: points 1..4 counter-clockwise from the origin, lines 10..13.
fn square_entities() -> Vec<SketchEntity> {
    vec![
        point(1, 0.0, 0.0),
        point(2, 1.0, 0.0),
        point(3, 1.0, 1.0),
        point(4, 0.0, 1.0),
        line(10, 1, 2),
        line(11, 2, 3),
        line(12, 3, 4),
        line(13, 4, 1),
    ]
}

/// Horizontal/vertical rails plus a pin at the origin. 8 params; the four
/// H/V constraints and the 2-row pin leave width and height free.
fn square_rails() -> Vec<SketchConstraint> {
    vec![
        SketchConstraint::Horizontal { entity: 10 },
        SketchConstraint::Vertical { entity: 11 },
        SketchConstraint::Horizontal { entity: 12 },
        SketchConstraint::Vertical { entity: 13 },
        SketchConstraint::Pinned {
            point: 1,
            x: 0.0,
            y: 0.0,
        },
    ]
}

fn width_dimension(value: f64) -> SketchConstraint {
    SketchConstraint::HDistance {
        point_a: 1,
        point_b: 2,
        value,
        expression: None,
        reference: false,
    }
}

fn height_dimension(value: f64) -> SketchConstraint {
    SketchConstraint::VDistance {
        point_a: 2,
        point_b: 3,
        value,
        expression: None,
        reference: false,
    }
}

/// Point ids named by a free direction's components.
fn moving_points(d: &FreeDirection) -> Vec<u32> {
    let mut ids: Vec<u32> = d
        .components
        .iter()
        .filter_map(|c| match c {
            FreeComponent::Point { id, .. } => Some(*id),
            FreeComponent::Radius { .. } => None,
        })
        .collect();
    ids.sort_unstable();
    ids
}

// ── Pin 1: a fully constrained rectangle ────────────────────────────────────

#[test]
fn a_fully_constrained_rectangle_has_zero_degrees_of_freedom() {
    let mut constraints = square_rails();
    constraints.push(width_dimension(2.0));
    constraints.push(height_dimension(3.0));

    let solved = solve_sketch(&make_sketch(square_entities(), constraints));

    assert!(
        matches!(solved.status, SolveStatus::FullyConstrained),
        "status was {:?}",
        solved.status
    );
    let r = &solved.report;
    assert_eq!(r.dof, 0, "a dimensioned, pinned rectangle has no freedom");
    assert_eq!(r.params, 8, "four points, two params each");
    assert!(r.free.is_empty(), "zero dof means zero free directions");
    assert!(
        r.conflicts.is_empty(),
        "nothing conflicts: {:?}",
        r.conflicts
    );
    assert!(
        r.redundant.is_empty(),
        "nothing is dependent: {:?}",
        r.redundant
    );
    assert_eq!(r.rank as usize, r.params as usize, "full column rank");

    // Every constraint reports a satisfied residual — one row per constraint
    // the caller handed in, in declaration order.
    assert_eq!(r.residuals.len(), 7);
    for (i, row) in r.residuals.iter().enumerate() {
        assert_eq!(row.index as usize, i, "residual rows are in caller order");
        assert!(
            row.satisfied,
            "{} at index {i} unsatisfied: {:?}",
            row.kind, row.residual
        );
        assert!(
            row.residual.expect("a driving constraint is evaluable") < 1e-6,
            "{} residual {:?}",
            row.kind,
            row.residual
        );
    }
    assert_eq!(r.residuals[5].kind, "HDistance");
    assert_eq!(r.residuals[6].kind, "VDistance");

    // The solve moved the two points the width/height dimensions pulled, and
    // only those: the pinned origin and its two rails hold the rest.
    let moved: Vec<u32> = r.moved.iter().map(|m| m.id).collect();
    assert_eq!(
        moved,
        vec![3, 4, 2],
        "worst-first by distance — the far corner (2.24), the top-left (2.0), \
         then the bottom-right (1.0): {:?}",
        r.moved
    );
    let p3 = &r.moved[0];
    // (1, 1) → (2, 3). Compared at 1e-8: the solve stops on `ftol`, so a
    // satisfied solution carries ~1e-9 of settling (measured 1.2e-9 here),
    // two orders below the 1e-6 satisfiability tolerance.
    assert!((p3.dx - 1.0).abs() < 1e-8, "x: 1 → 2, dx = {}", p3.dx);
    assert!((p3.dy - 2.0).abs() < 1e-8, "y: 1 → 3, dy = {}", p3.dy);
    assert!(
        (p3.distance - p3.dx.hypot(p3.dy)).abs() < 1e-12,
        "distance is hypot(dx, dy)"
    );

    assert!(
        solved.report.convergence.residual_inf < solved.report.convergence.tolerance,
        "the verdict's own number: {:?}",
        solved.report.convergence
    );
}

// ── Pin 2: one dimension removed ────────────────────────────────────────────

#[test]
fn removing_one_dimension_leaves_exactly_one_free_direction() {
    // The same rectangle with the HEIGHT dimension dropped: height is free,
    // width is not.
    let mut constraints = square_rails();
    constraints.push(width_dimension(2.0));

    let solved = solve_sketch(&make_sketch(square_entities(), constraints));

    assert!(
        matches!(solved.status, SolveStatus::UnderConstrained { dof: 1 }),
        "status was {:?}",
        solved.status
    );
    let r = &solved.report;
    assert_eq!(r.dof, 1, "exactly one freedom: the height");
    assert_eq!(
        r.free.len(),
        1,
        "free.len() == dof is the type's invariant: {:?}",
        r.free
    );
    assert_eq!(r.free[0].basis, 0);

    // The freedom is the TOP edge sliding vertically: points 3 and 4 move,
    // points 1 and 2 (pinned / on the bottom rail) do not.
    assert_eq!(
        moving_points(&r.free[0]),
        vec![3, 4],
        "the top edge is what can still move: {:?}",
        r.free[0]
    );
    for c in &r.free[0].components {
        if let FreeComponent::Point { id, dx, dy } = c {
            assert!(
                dx.abs() < 1e-6,
                "point {id} slides vertically, not horizontally (dx = {dx})"
            );
            assert!(dy.abs() > 0.1, "point {id} has a real dy ({dy})");
        }
    }

    // Removing the dimension removed nothing from the residual report: the
    // six remaining constraints each still report a satisfied residual.
    assert_eq!(r.residuals.len(), 6);
    assert!(r.residuals.iter().all(|row| row.satisfied));
}

#[test]
fn a_sketch_with_no_constraints_reports_every_parameter_as_free() {
    let solved = solve_sketch(&make_sketch(square_entities(), Vec::new()));
    let r = &solved.report;
    assert_eq!(r.dof, 8, "four free points");
    assert_eq!(r.free.len(), 8, "one direction per parameter");
    assert_eq!(r.rows, 0);
    assert_eq!(
        r.convergence.termination,
        Termination::NotRun,
        "nothing to solve means no LM run, said so rather than implied"
    );
    // Each direction names exactly one point, and the eight directions cover
    // the four points twice (x and y).
    let mut named: Vec<u32> = r.free.iter().flat_map(moving_points).collect();
    named.sort_unstable();
    assert_eq!(named, vec![1, 1, 2, 2, 3, 3, 4, 4]);
}

#[test]
fn a_free_circle_radius_is_a_free_direction_of_its_own() {
    // A circle pinned at its center: the radius is the only freedom left.
    let entities = vec![point(1, 0.0, 0.0), circle(20, 1, 0.5)];
    let constraints = vec![SketchConstraint::Pinned {
        point: 1,
        x: 0.0,
        y: 0.0,
    }];
    let solved = solve_sketch(&make_sketch(entities, constraints));
    let r = &solved.report;
    assert_eq!(r.dof, 1, "x, y pinned; r free");
    assert_eq!(r.free.len(), 1);
    assert_eq!(
        r.free[0].components,
        vec![FreeComponent::Radius {
            entity: 20,
            dr: 1.0
        }],
        "the freedom is the radius, named as a radius: {:?}",
        r.free[0]
    );
}

// ── Pin 3: a redundant constraint ───────────────────────────────────────────

#[test]
fn a_redundant_equal_length_is_flagged_as_dependent() {
    // A square whose four sides are already equal by construction: two
    // dimensions plus an Equal between the bottom and top edges, which the
    // two Horizontal rails and the dimensions already imply. The system is
    // SATISFIABLE, so it is not a conflict — it is redundant, and the later
    // constraint is the dependent one.
    let mut constraints = square_rails();
    constraints.push(width_dimension(2.0));
    constraints.push(height_dimension(2.0));
    constraints.push(SketchConstraint::Equal {
        entity_a: 10,
        entity_b: 12,
    });

    let solved = solve_sketch(&make_sketch(square_entities(), constraints));

    assert!(
        matches!(solved.status, SolveStatus::FullyConstrained),
        "a redundant system is satisfiable, not a conflict: {:?}",
        solved.status
    );
    let r = &solved.report;
    assert!(
        r.rank < r.rows,
        "redundancy is rank {} below row count {}",
        r.rank,
        r.rows
    );
    assert_eq!(
        r.redundant,
        vec![7],
        "the Equal at caller index 7 adds no rank: {:?}",
        r.redundant
    );
    assert!(
        r.conflicts.is_empty(),
        "redundant is not conflicting: {:?}",
        r.conflicts
    );
    assert_eq!(r.residuals[7].kind, "Equal");
    assert!(r.residuals[7].satisfied);
}

#[test]
fn a_satisfiable_full_rank_system_flags_nothing_as_redundant() {
    // The same shape WITHOUT the extra Equal: the redundancy report must not
    // fire on a system that is merely fully constrained.
    let mut constraints = square_rails();
    constraints.push(width_dimension(2.0));
    constraints.push(height_dimension(2.0));
    let solved = solve_sketch(&make_sketch(square_entities(), constraints));
    assert!(solved.report.redundant.is_empty());
    assert_eq!(solved.report.rank, solved.report.params);
}

// ── Pin 4: a conflicting pair ───────────────────────────────────────────────

#[test]
fn a_conflicting_dimension_pair_is_named_in_conflicts() {
    // Two width dimensions that disagree: 2.0 and 5.0 on the same point pair.
    // Unsatisfiable, rank-deficient in the rows — an over-constrained system,
    // and the report names the offenders in the CALLER's index space.
    let mut constraints = square_rails();
    constraints.push(width_dimension(2.0));
    constraints.push(SketchConstraint::HDistance {
        point_a: 1,
        point_b: 2,
        value: 5.0,
        expression: None,
        reference: false,
    });

    let solved = solve_sketch(&make_sketch(square_entities(), constraints));

    let SolveStatus::OverConstrained { conflicts } = &solved.status else {
        panic!("expected OverConstrained, got {:?}", solved.status);
    };
    let r = &solved.report;
    assert_eq!(
        &r.conflicts, conflicts,
        "the report's conflicts and the status's are one list"
    );
    assert_eq!(
        r.conflicts.len(),
        2,
        "both dimensions fight: {:?}",
        r.conflicts
    );
    assert!(r.conflicts.contains(&5) && r.conflicts.contains(&6));
    assert!(
        r.redundant.is_empty(),
        "an UNSATISFIED system reports conflicts, not redundancy"
    );

    // The residual rows say by how much, not merely that.
    let worst = r.residuals[*r.conflicts.first().unwrap() as usize]
        .residual
        .expect("a driving dimension is evaluable");
    assert!(
        worst > 1e-3,
        "a 3 m disagreement is not a rounding error: {worst}"
    );
    assert!(!r.residuals[5].satisfied && !r.residuals[6].satisfied);
    // The rails and the pin are innocent and say so.
    for i in 0..5 {
        assert!(
            r.residuals[i].satisfied,
            "{} at {i} should not be blamed",
            r.residuals[i].kind
        );
    }
}

#[test]
fn conflicts_are_populated_even_when_the_verdict_is_not_over_constrained() {
    // An unsatisfiable system whose rows are INDEPENDENT classifies as
    // SolveFailed, not OverConstrained — before S2 the offending constraints
    // were reported nowhere at all on that path. Two Pinned constraints on
    // one point at two places: 2 + 2 independent rows, no solution.
    let entities = vec![point(1, 0.0, 0.0)];
    let constraints = vec![
        SketchConstraint::Pinned {
            point: 1,
            x: 0.0,
            y: 0.0,
        },
        SketchConstraint::Pinned {
            point: 1,
            x: 1.0,
            y: 1.0,
        },
    ];
    let solved = solve_sketch(&make_sketch(entities, constraints));
    assert!(
        !solved.report.conflicts.is_empty(),
        "an unsatisfied solve names its offenders whatever the verdict: {:?} / {:?}",
        solved.status,
        solved.report
    );
    assert!(solved
        .report
        .residuals
        .iter()
        .any(|row| row.kind == "Pinned" && !row.satisfied));
}

// ── The reference-dimension filter and index space ──────────────────────────

#[test]
fn a_reference_dimension_does_not_drive_and_keeps_the_caller_index_space() {
    // A rectangle dimensioned 2 x 3, plus a REFERENCE width dimension that
    // claims 9.0. A driven dimension measures and must not fight: the
    // geometry stays 2 x 3, the reference row reports its measurement error,
    // and the conflict list stays empty.
    let mut constraints = square_rails();
    constraints.push(width_dimension(2.0));
    constraints.push(height_dimension(3.0));
    constraints.push(SketchConstraint::HDistance {
        point_a: 1,
        point_b: 2,
        value: 9.0,
        expression: None,
        reference: true,
    });

    let solved = solve_sketch(&make_sketch(square_entities(), constraints));

    assert!(
        matches!(solved.status, SolveStatus::FullyConstrained),
        "the reference dimension must not change the verdict: {:?}",
        solved.status
    );
    let (x2, _) = solved.positions[&2];
    assert!(
        (x2 - 2.0).abs() < 1e-9,
        "width is the DRIVING 2.0, got {x2}"
    );

    let r = &solved.report;
    assert_eq!(
        r.residuals.len(),
        8,
        "one row per caller constraint, reference dimensions included"
    );
    assert_eq!(
        r.residuals[7].index, 7,
        "indices are the CALLER's, unfiltered"
    );
    assert!(r.residuals[7].reference);
    let measured = r.residuals[7]
        .residual
        .expect("a reference dimension over two points is evaluable");
    assert!(
        (measured - 7.0).abs() < 1e-6,
        "the reference dimension is out by 9 - 2 = 7, reported as {measured}"
    );
    assert!(!r.residuals[7].satisfied, "and says it is not satisfied");
    assert!(
        r.conflicts.is_empty(),
        "a driven dimension is never an offender: {:?}",
        r.conflicts
    );
    assert_eq!(r.dof, 0, "it contributes no rank either");
}

#[test]
fn conflict_indices_skip_over_a_leading_reference_dimension() {
    // The index-space regression the mapping exists to prevent: a reference
    // dimension FIRST, then two conflicting widths at caller indices 6 and 7.
    // Reported in the filtered driving space they would read 5 and 6.
    let mut constraints = vec![SketchConstraint::HDistance {
        point_a: 1,
        point_b: 2,
        value: 9.0,
        expression: None,
        reference: true,
    }];
    constraints.extend(square_rails());
    constraints.push(width_dimension(2.0));
    constraints.push(SketchConstraint::HDistance {
        point_a: 1,
        point_b: 2,
        value: 5.0,
        expression: None,
        reference: false,
    });

    let solved = solve_sketch(&make_sketch(square_entities(), constraints));
    let r = &solved.report;
    let mut conflicts = r.conflicts.clone();
    conflicts.sort_unstable();
    assert_eq!(
        conflicts,
        vec![6, 7],
        "caller indices, not driving indices: {:?}",
        r.conflicts
    );
}

// ── Convergence ─────────────────────────────────────────────────────────────

#[test]
fn convergence_is_typed_and_reports_the_evaluation_count() {
    let mut constraints = square_rails();
    constraints.push(width_dimension(2.0));
    constraints.push(height_dimension(3.0));
    let solved = solve_sketch(&make_sketch(square_entities(), constraints));
    let c = &solved.report.convergence;
    assert!(
        matches!(
            c.termination,
            Termination::Converged { .. } | Termination::ResidualsZero | Termination::Orthogonal
        ),
        "a satisfiable solve stops for a successful reason: {:?}",
        c.termination
    );
    assert!(c.successful, "{:?}", c);
    assert!(c.evaluations > 0, "LM ran: {:?}", c);
    assert_eq!(c.tolerance, 1e-6, "the tolerance the verdict used");
}

#[test]
fn the_report_is_bit_identical_across_two_solves_of_the_same_sketch() {
    let mut constraints = square_rails();
    constraints.push(width_dimension(2.0));
    let sketch = make_sketch(square_entities(), constraints);
    let a = solve_sketch(&sketch);
    let b = solve_sketch(&sketch);
    assert_eq!(
        a.report, b.report,
        "the report is part of the determinism contract"
    );
}

#[test]
fn the_report_round_trips_through_json() {
    // The report travels to the UI and to an MCP tool over serde; a field
    // that cannot serialize would be discovered only in the browser.
    let mut constraints = square_rails();
    constraints.push(width_dimension(2.0));
    let solved = solve_sketch(&make_sketch(square_entities(), constraints));
    let text = serde_json::to_string(&solved.report).expect("report serializes");
    let back: SketchSolveReport = serde_json::from_str(&text).expect("report deserializes");
    // Structure exactly; floats to 1e-18. `serde_json`'s float parse is not
    // bit-exact for every shortest-form f64 (measured: a 1 ULP drift on
    // 1.0000000827403709e-10), so a bit-identity assertion here would pin the
    // JSON library's rounding rather than this report's contract. The
    // determinism contract is pinned in Rust by
    // `the_report_is_bit_identical_across_two_solves_of_the_same_sketch`.
    assert_eq!(back.dof, solved.report.dof);
    assert_eq!(back.params, solved.report.params);
    assert_eq!(back.rank, solved.report.rank);
    assert_eq!(back.rows, solved.report.rows);
    assert_eq!(back.conflicts, solved.report.conflicts);
    assert_eq!(back.redundant, solved.report.redundant);
    assert_eq!(
        back.convergence.termination,
        solved.report.convergence.termination
    );
    assert_eq!(back.residuals.len(), solved.report.residuals.len());
    for (got, want) in back.residuals.iter().zip(solved.report.residuals.iter()) {
        assert_eq!(got.index, want.index);
        assert_eq!(got.kind, want.kind);
        assert_eq!(got.satisfied, want.satisfied);
        assert!((got.residual.unwrap() - want.residual.unwrap()).abs() < 1e-18);
    }
    assert_eq!(back.free.len(), solved.report.free.len());
    assert_eq!(
        moving_points(&back.free[0]),
        moving_points(&solved.report.free[0])
    );
    assert_eq!(back.moved.len(), solved.report.moved.len());
    // And an older payload with no report at all still parses.
    let legacy = serde_json::json!({
        "positions": {},
        "radii": {},
        "profiles": [],
        "status": { "type": "Unsolved" }
    });
    let old: SolvedSketch = serde_json::from_value(legacy).expect("a pre-S2 payload still parses");
    assert_eq!(old.report, SketchSolveReport::default());
}
