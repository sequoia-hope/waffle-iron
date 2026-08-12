//! Dispatch-arm and compile-error coverage for `CompiledConstraint::compile`.
//!
//! Several `SketchConstraint` variants are polymorphic over entity kind and
//! pick a different compiled form per operand pair — `Equal` alone has five
//! accepting arms plus a rejecting one. The pre-existing suite exercised the
//! circle/line arms; the arc arms and every `Err(...)` arm were unreachable
//! from any test, so a mis-wired arc arm (or a silently-accepted nonsense
//! operand pair) would not have failed anything.
//!
//! Method: compile the constraint against a real `ParamLayout` and evaluate the
//! resulting residual at the layout's own parameters. The residual VALUE
//! identifies which arm was selected — an `Equal` that compiled to
//! `EqualCircles` cannot produce an arc-radius residual — so these assertions
//! pin arm selection and operand binding together, without needing `PartialEq`
//! on the compiled form.
//!
//! Determinism: all geometry is literal, all ids are fixed, `Uuid::nil()` is
//! used for the sketch id. No random values, no time, no filesystem.

use sketch_solver::constraint_mapping::CompiledConstraint;
use sketch_solver::entity_mapping::ParamLayout;
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

/// Compile `c` against `entities` and return its residual at the entities'
/// declared positions.
fn residual_at_declared(entities: &[SketchEntity], c: &SketchConstraint) -> f64 {
    let layout = ParamLayout::build(entities);
    let cc = CompiledConstraint::compile(c, &layout)
        .unwrap_or_else(|e| panic!("expected constraint to compile, got Err({e})"));
    let r = cc.residuals(&layout.params);
    assert_eq!(r.nrows(), 1, "helper is for single-row constraints only");
    r[0]
}

/// Compile `c` against `entities`, expecting rejection, and return the reason.
fn compile_error(entities: &[SketchEntity], c: &SketchConstraint) -> String {
    let layout = ParamLayout::build(entities);
    match CompiledConstraint::compile(c, &layout) {
        Ok(_) => panic!("expected compile to REJECT this constraint, but it compiled"),
        Err(e) => e,
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

/// Arc 40: center (0,0), start (3,4) → radius 5. Arc 41: center (20,20),
/// start (20,22) → radius 2. Circle 30: center (50,50), radius 7.
fn arcs_and_circle() -> Vec<SketchEntity> {
    vec![
        point(1, 0.0, 0.0),   // arc 40 center
        point(2, 3.0, 4.0),   // arc 40 start  → r = 5
        point(3, -4.0, 3.0),  // arc 40 end
        point(4, 20.0, 20.0), // arc 41 center
        point(5, 20.0, 22.0), // arc 41 start  → r = 2
        point(6, 18.0, 20.0), // arc 41 end
        point(7, 50.0, 50.0), // circle 30 center
        circle(30, 7, 7.0),
        arc(40, 1, 2, 3),
        arc(41, 4, 5, 6),
    ]
}

// ── Equal: the arc arms ─────────────────────────────────────────────────────

#[test]
fn equal_arc_arc_selects_the_arc_arc_arm() {
    // EqualArcArc residual = ‖C_a-S_a‖ - ‖C_b-S_b‖ = 5 - 2 = 3.
    let r = residual_at_declared(
        &arcs_and_circle(),
        &SketchConstraint::Equal {
            entity_a: 40,
            entity_b: 41,
        },
    );
    assert!(
        (r - 3.0).abs() < 1e-12,
        "Equal(arc,arc) must compare center→start radii (expected 5-2=3), got {r}"
    );
}

#[test]
fn equal_circle_arc_selects_the_arc_circle_arm() {
    // entity_a = circle (r=7), entity_b = arc (r=5).
    // EqualArcCircle residual = arc_radius - circle_radius = 5 - 7 = -2.
    let r = residual_at_declared(
        &arcs_and_circle(),
        &SketchConstraint::Equal {
            entity_a: 30,
            entity_b: 40,
        },
    );
    assert!(
        (r + 2.0).abs() < 1e-12,
        "Equal(circle,arc) must give arc_r - circle_r = 5-7 = -2, got {r}"
    );
}

#[test]
fn equal_arc_circle_is_symmetric_with_the_reversed_operand_order() {
    // The (Arc, Circle) and (Circle, Arc) arms are separate code paths that
    // must resolve to the SAME compiled constraint. A transposed operand
    // binding in either arm shows up as a sign flip here.
    let entities = arcs_and_circle();
    let forward = residual_at_declared(
        &entities,
        &SketchConstraint::Equal {
            entity_a: 30,
            entity_b: 40,
        },
    );
    let reversed = residual_at_declared(
        &entities,
        &SketchConstraint::Equal {
            entity_a: 40,
            entity_b: 30,
        },
    );
    assert!(
        (forward - reversed).abs() < 1e-12,
        "Equal(circle,arc) and Equal(arc,circle) must compile identically, \
         got {forward} vs {reversed}"
    );
}

#[test]
fn equal_arc_arc_solves_to_matching_radii() {
    // End-to-end: pin arc 40 rigid, leave arc 41 free, and require Equal.
    // Arc 41's radius (2) must move to arc 40's (5).
    let mut entities = arcs_and_circle();
    entities.retain(|e| !matches!(e, SketchEntity::Circle { .. }));
    let sketch = make_sketch(
        entities,
        vec![
            SketchConstraint::Pinned {
                point: 1,
                x: 0.0,
                y: 0.0,
            },
            SketchConstraint::Pinned {
                point: 2,
                x: 3.0,
                y: 4.0,
            },
            SketchConstraint::Pinned {
                point: 4,
                x: 20.0,
                y: 20.0,
            },
            SketchConstraint::Equal {
                entity_a: 40,
                entity_b: 41,
            },
        ],
    );
    let solved = solve_sketch(&sketch);
    assert!(
        matches!(
            solved.status,
            SolveStatus::FullyConstrained | SolveStatus::UnderConstrained { .. }
        ),
        "Equal(arc,arc) must be satisfiable, got {:?}",
        solved.status
    );

    let c = solved.positions[&4];
    let s = solved.positions[&5];
    let r_b = ((s.0 - c.0).powi(2) + (s.1 - c.1).powi(2)).sqrt();
    assert!(
        (r_b - 5.0).abs() < 1e-5,
        "arc 41 radius must solve to arc 40's radius of 5, got {r_b}"
    );
}

// ── Radius / Diameter / OnEntity / Tangent: the arc arms ────────────────────

#[test]
fn radius_on_arc_selects_the_radius_arc_arm() {
    // RadiusArc residual = ‖C-S‖ - value = 5 - 4 = 1. (A Circle arm would have
    // panicked on the missing radius param instead.)
    let r = residual_at_declared(
        &arcs_and_circle(),
        &SketchConstraint::Radius {
            entity: 40,
            value: 4.0,
        },
    );
    assert!(
        (r - 1.0).abs() < 1e-12,
        "Radius on an arc must measure center→start (expected 5-4=1), got {r}"
    );
}

#[test]
fn diameter_on_arc_selects_the_diameter_arc_arm() {
    // DiameterArc residual = 2‖C-S‖ - value = 10 - 4 = 6.
    let r = residual_at_declared(
        &arcs_and_circle(),
        &SketchConstraint::Diameter {
            entity: 40,
            value: 4.0,
        },
    );
    assert!(
        (r - 6.0).abs() < 1e-12,
        "Diameter on an arc must be 2·‖C-S‖ (expected 10-4=6), got {r}"
    );
}

#[test]
fn radius_and_diameter_on_an_arc_differ_by_the_factor_of_two() {
    // Guards the arc arms against the classic copy-paste defect of Diameter
    // reusing Radius's residual: at value 0 the two must differ by exactly 2×.
    let entities = arcs_and_circle();
    let rad = residual_at_declared(
        &entities,
        &SketchConstraint::Radius {
            entity: 40,
            value: 0.0,
        },
    );
    let dia = residual_at_declared(
        &entities,
        &SketchConstraint::Diameter {
            entity: 40,
            value: 0.0,
        },
    );
    assert!(
        (dia - 2.0 * rad).abs() < 1e-12,
        "arc Diameter must be exactly 2× arc Radius, got {dia} vs 2×{rad}"
    );
}

#[test]
fn on_entity_circle_selects_the_circle_arm() {
    // Circle 30: center (50,50), r=7. Point 8 at (57,50) is ON it → residual 0.
    let mut entities = arcs_and_circle();
    entities.push(point(8, 57.0, 50.0));
    let r = residual_at_declared(
        &entities,
        &SketchConstraint::OnEntity {
            point: 8,
            entity: 30,
        },
    );
    assert!(
        r.abs() < 1e-12,
        "point on circle rim must give zero residual, got {r}"
    );
}

#[test]
fn on_entity_arc_selects_the_arc_arm() {
    // Arc 40: center (0,0), radius 5. Point 8 at (0,8) → ‖P-C‖-r = 8-5 = 3.
    let mut entities = arcs_and_circle();
    entities.push(point(8, 0.0, 8.0));
    let r = residual_at_declared(
        &entities,
        &SketchConstraint::OnEntity {
            point: 8,
            entity: 40,
        },
    );
    assert!(
        (r - 3.0).abs() < 1e-12,
        "OnEntity(arc) must be ‖P-C‖ - ‖C-S‖ (expected 8-5=3), got {r}"
    );
}

#[test]
fn tangent_line_arc_selects_the_arc_arm() {
    // Arc 40: center (0,0), radius 5. Line 50 is y = 5 — tangent from above.
    // TangentLineArc uses the squared form, so a tangent line gives 0.
    let mut entities = arcs_and_circle();
    entities.push(point(8, -10.0, 5.0));
    entities.push(point(9, 10.0, 5.0));
    entities.push(line(50, 8, 9));
    let r = residual_at_declared(
        &entities,
        &SketchConstraint::Tangent {
            line: 50,
            curve: 40,
        },
    );
    assert!(
        r.abs() < 1e-9,
        "line y=5 is tangent to the radius-5 arc at the origin, got residual {r}"
    );
}

// ── Distance: operand-order symmetry ────────────────────────────────────────

#[test]
fn distance_point_line_and_line_point_compile_identically() {
    // (Point, Line) and (Line, Point) are separate arms that must bind the
    // point to P and the line to A→B in both orders. The residual is a SIGNED
    // perpendicular distance, so a swapped binding is not merely a sign flip —
    // it is a different quantity entirely.
    let entities = vec![
        point(1, 0.0, 0.0),
        point(2, 10.0, 0.0),
        point(3, 5.0, 7.0),
        line(10, 1, 2),
    ];
    let forward = residual_at_declared(
        &entities,
        &SketchConstraint::Distance {
            entity_a: 3,
            entity_b: 10,
            value: 0.0,
        },
    );
    let reversed = residual_at_declared(
        &entities,
        &SketchConstraint::Distance {
            entity_a: 10,
            entity_b: 3,
            value: 0.0,
        },
    );
    assert!(
        (forward - reversed).abs() < 1e-12,
        "Distance(point,line) and Distance(line,point) must agree, \
         got {forward} vs {reversed}"
    );
    assert!(
        (forward.abs() - 7.0).abs() < 1e-12,
        "perpendicular distance from (5,7) to the x-axis is 7, got |{forward}|"
    );
}

#[test]
fn point_line_distance_matches_the_distance_point_line_arm() {
    // `PointLineDistance` is documented as "same residual as the (Point, Line)
    // arm of Distance". That equivalence is load-bearing for the dimension
    // tool and is asserted here rather than assumed.
    let entities = vec![
        point(1, 0.0, 0.0),
        point(2, 10.0, 0.0),
        point(3, 5.0, 7.0),
        line(10, 1, 2),
    ];
    let via_distance = residual_at_declared(
        &entities,
        &SketchConstraint::Distance {
            entity_a: 3,
            entity_b: 10,
            value: 2.0,
        },
    );
    let via_point_line = residual_at_declared(
        &entities,
        &SketchConstraint::PointLineDistance {
            point: 3,
            entity: 10,
            value: 2.0,
        },
    );
    assert!(
        (via_distance - via_point_line).abs() < 1e-12,
        "PointLineDistance must equal Distance(point,line), got {via_distance} vs {via_point_line}"
    );
}

// ── The rejecting arms ──────────────────────────────────────────────────────

#[test]
fn equal_between_a_line_and_a_circle_is_rejected() {
    let entities = vec![
        point(1, 0.0, 0.0),
        point(2, 10.0, 0.0),
        point(3, 50.0, 50.0),
        line(10, 1, 2),
        circle(30, 3, 7.0),
    ];
    let e = compile_error(
        &entities,
        &SketchConstraint::Equal {
            entity_a: 10,
            entity_b: 30,
        },
    );
    assert!(
        e.contains("Equal constraint not supported"),
        "expected an Equal-unsupported reason, got: {e}"
    );
}

#[test]
fn equal_between_two_points_is_rejected() {
    // Points have no "size", so Equal is meaningless — it must be refused
    // loudly rather than silently compiling to a no-op.
    let entities = vec![point(1, 0.0, 0.0), point(2, 10.0, 0.0)];
    let e = compile_error(
        &entities,
        &SketchConstraint::Equal {
            entity_a: 1,
            entity_b: 2,
        },
    );
    assert!(
        e.contains("Equal constraint not supported"),
        "expected an Equal-unsupported reason, got: {e}"
    );
}

#[test]
fn distance_between_two_circles_is_rejected() {
    let entities = vec![
        point(1, 0.0, 0.0),
        point(2, 50.0, 50.0),
        circle(30, 1, 7.0),
        circle(31, 2, 3.0),
    ];
    let e = compile_error(
        &entities,
        &SketchConstraint::Distance {
            entity_a: 30,
            entity_b: 31,
            value: 5.0,
        },
    );
    assert!(
        e.contains("Distance constraint not supported"),
        "expected a Distance-unsupported reason, got: {e}"
    );
}

#[test]
fn radius_on_a_line_is_rejected() {
    let entities = vec![point(1, 0.0, 0.0), point(2, 10.0, 0.0), line(10, 1, 2)];
    let e = compile_error(
        &entities,
        &SketchConstraint::Radius {
            entity: 10,
            value: 5.0,
        },
    );
    assert!(
        e.contains("Radius requires circle/arc"),
        "expected a Radius-kind reason, got: {e}"
    );
}

#[test]
fn diameter_on_a_line_is_rejected() {
    let entities = vec![point(1, 0.0, 0.0), point(2, 10.0, 0.0), line(10, 1, 2)];
    let e = compile_error(
        &entities,
        &SketchConstraint::Diameter {
            entity: 10,
            value: 5.0,
        },
    );
    assert!(
        e.contains("Diameter requires circle/arc"),
        "expected a Diameter-kind reason, got: {e}"
    );
}

#[test]
fn on_entity_targeting_a_point_is_rejected() {
    let entities = vec![point(1, 0.0, 0.0), point(2, 10.0, 0.0)];
    let e = compile_error(
        &entities,
        &SketchConstraint::OnEntity {
            point: 1,
            entity: 2,
        },
    );
    assert!(
        e.contains("OnEntity target must be line/circle/arc"),
        "expected an OnEntity-kind reason, got: {e}"
    );
}

#[test]
fn tangent_to_a_line_is_rejected() {
    let entities = vec![
        point(1, 0.0, 0.0),
        point(2, 10.0, 0.0),
        point(3, 0.0, 5.0),
        point(4, 10.0, 5.0),
        line(10, 1, 2),
        line(11, 3, 4),
    ];
    let e = compile_error(
        &entities,
        &SketchConstraint::Tangent {
            line: 10,
            curve: 11,
        },
    );
    assert!(
        e.contains("Tangent curve must be arc or circle"),
        "expected a Tangent-kind reason, got: {e}"
    );
}

// ── Unknown-id paths ────────────────────────────────────────────────────────

#[test]
fn constraints_referencing_unknown_ids_are_rejected_not_ignored() {
    // Every resolver closure in `compile` (entity kind, point, line,
    // circle/arc) has its own unknown-id error. A dangling reference — e.g.
    // from a constraint that outlived the entity it names — must surface, not
    // be silently dropped, or the sketch would appear satisfiable while
    // ignoring a constraint the user still sees in the UI.
    let entities = vec![
        point(1, 0.0, 0.0),
        point(2, 10.0, 0.0),
        point(3, 50.0, 50.0),
        line(10, 1, 2),
        circle(30, 3, 7.0),
    ];

    // Unknown POINT (via a point-typed operand).
    let e = compile_error(
        &entities,
        &SketchConstraint::Coincident {
            point_a: 1,
            point_b: 999,
        },
    );
    assert!(
        e.contains("unknown point 999"),
        "expected an unknown-point reason, got: {e}"
    );

    // Unknown LINE (via a line-typed operand).
    let e = compile_error(&entities, &SketchConstraint::Horizontal { entity: 999 });
    assert!(
        e.contains("unknown line 999"),
        "expected an unknown-line reason, got: {e}"
    );

    // Unknown ENTITY (via a kind-dispatched operand).
    let e = compile_error(
        &entities,
        &SketchConstraint::Radius {
            entity: 999,
            value: 5.0,
        },
    );
    assert!(
        e.contains("unknown entity 999"),
        "expected an unknown-entity reason, got: {e}"
    );

    // Unknown MIDPOINT line, with a valid point — the second resolver must
    // still fire.
    let e = compile_error(
        &entities,
        &SketchConstraint::Midpoint {
            point: 1,
            line: 999,
        },
    );
    assert!(
        e.contains("unknown line 999"),
        "expected an unknown-line reason from Midpoint, got: {e}"
    );
}

#[test]
fn a_constraint_on_an_unsupported_entity_kind_fails_the_whole_solve_inertly() {
    // Invariant I4 (specs/sketch_drag_stability.md §4): a failed solve is
    // inert — it echoes the INPUT positions rather than a half-solved iterate,
    // and publishes no profiles. Here the failure comes from the compile
    // stage (an Equal between a line and a circle), which returns before LM
    // ever runs — the path that `failed_result` serves.
    let sketch = make_sketch(
        vec![
            point(1, 0.0, 0.0),
            point(2, 10.0, 0.0),
            point(3, 50.0, 50.0),
            line(10, 1, 2),
            circle(30, 3, 7.0),
        ],
        vec![
            // A satisfiable constraint that WOULD move point 2 if it ran.
            SketchConstraint::Distance {
                entity_a: 1,
                entity_b: 2,
                value: 999.0,
            },
            SketchConstraint::Equal {
                entity_a: 10,
                entity_b: 30,
            },
        ],
    );
    let solved = solve_sketch(&sketch);

    match &solved.status {
        SolveStatus::SolveFailed { reason } => {
            assert!(
                reason.contains("Equal constraint not supported"),
                "SolveFailed must carry the compile reason, got: {reason}"
            );
        }
        other => panic!("expected SolveFailed for an unsupported Equal, got {other:?}"),
    }

    assert_eq!(
        solved.positions[&2],
        (10.0, 0.0),
        "a failed solve must echo the INPUT position, not a partially-solved one"
    );
    assert!(
        solved.profiles.is_empty(),
        "a failed solve must publish no profiles"
    );
}

#[test]
fn same_orientation_never_changes_a_solve() {
    // SameOrientation is a documented 2D no-op. It owns zero residual rows, so
    // adding it must leave status, dof and positions bit-identical — in
    // particular it must never inflate the constraint count in a way that
    // perturbs rank or conflict indices.
    let entities = vec![
        point(1, 0.0, 0.0),
        point(2, 10.0, 0.0),
        point(3, 0.0, 5.0),
        point(4, 10.0, 5.0),
        line(10, 1, 2),
        line(11, 3, 4),
    ];
    let base = vec![
        SketchConstraint::Pinned {
            point: 1,
            x: 0.0,
            y: 0.0,
        },
        SketchConstraint::Horizontal { entity: 10 },
    ];
    let mut with_noop = base.clone();
    with_noop.push(SketchConstraint::SameOrientation {
        entity_a: 10,
        entity_b: 11,
    });

    let a = solve_sketch(&make_sketch(entities.clone(), base));
    let b = solve_sketch(&make_sketch(entities, with_noop));

    assert_eq!(
        format!("{:?}", a.status),
        format!("{:?}", b.status),
        "SameOrientation must not change the solve status"
    );
    for (id, pos) in &a.positions {
        let other = b.positions[id];
        assert_eq!(
            *pos, other,
            "SameOrientation must not move point {id}: {pos:?} vs {other:?}"
        );
    }
}
