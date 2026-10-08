//! S4 — the sketch solver's structural report, checked against an INDEPENDENT
//! computation (`specs/agent_mechanical_design.md` §10.4).
//!
//! `sketch_solver` publishes `params`, `rows`, `rank`, `dof = params - rank`, a
//! redundancy list and a null-space basis, all out of one column-pivoted QR of
//! one hand-written analytic Jacobian. A wrong gradient there produces a
//! self-consistent wrong rank that nothing in the crate contradicts.
//!
//! `test_harness::sketch_rank` is the contradicting party: the same residuals
//! written from their published equations, differentiated by CENTRAL FINITE
//! DIFFERENCES, ranked by SVD of the row-normalized Jacobian. Different
//! algebra, different implementation — the reference-parity posture the kernel
//! work uses.
//!
//! Determinism: literal geometry, fixed ids, `Uuid::nil()`. No random values,
//! no time, no filesystem.

use std::collections::HashMap;

use sketch_solver::solve_sketch;
use test_harness::sketch_rank::{analyze_at, summarize, OracleReport};
use uuid::Uuid;
use waffle_types::{
    Anchor, GeomRef, ResolvePolicy, Role, Selector, Sketch, SketchConstraint, SketchEntity,
    SolveStatus, SolvedSketch, TopoKind,
};

// ── Fixture helpers ─────────────────────────────────────────────────────────

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
    }
}

fn pin(point: u32, x: f64, y: f64) -> SketchConstraint {
    SketchConstraint::Pinned { point, x, y }
}

fn hdist(point_a: u32, point_b: u32, value: f64) -> SketchConstraint {
    SketchConstraint::HDistance {
        point_a,
        point_b,
        value,
        expression: None,
        reference: false,
    }
}

fn vdist(point_a: u32, point_b: u32, value: f64) -> SketchConstraint {
    SketchConstraint::VDistance {
        point_a,
        point_b,
        value,
        expression: None,
        reference: false,
    }
}

fn pp_distance(entity_a: u32, entity_b: u32, value: f64) -> SketchConstraint {
    SketchConstraint::Distance {
        entity_a,
        entity_b,
        value,
        expression: None,
        reference: false,
    }
}

/// A `w × h` rectangle with its lower-left corner at the origin: points 1..4
/// counter-clockwise, lines 10..13.
fn rect_entities(w: f64, h: f64) -> Vec<SketchEntity> {
    vec![
        point(1, 0.0, 0.0),
        point(2, w, 0.0),
        point(3, w, h),
        point(4, 0.0, h),
        line(10, 1, 2),
        line(11, 2, 3),
        line(12, 3, 4),
        line(13, 4, 1),
    ]
}

/// Horizontal/vertical rails on all four sides plus a pin at the origin: 6
/// rows, leaving width and height free.
fn rect_rails() -> Vec<SketchConstraint> {
    vec![
        SketchConstraint::Horizontal { entity: 10 },
        SketchConstraint::Vertical { entity: 11 },
        SketchConstraint::Horizontal { entity: 12 },
        SketchConstraint::Vertical { entity: 13 },
        pin(1, 0.0, 0.0),
    ]
}

/// The railed rectangle plus both dimensions: 8 rows over 8 params, zero dof.
fn rect_fully_constrained(w: f64, h: f64) -> Sketch {
    let mut cs = rect_rails();
    cs.push(hdist(1, 2, w));
    cs.push(vdist(2, 3, h));
    make_sketch(rect_entities(w, h), cs)
}

// ── The differential check itself ───────────────────────────────────────────

/// Everything the oracle and the report can be compared on, for one sketch.
///
/// Returns the pair so a caller can go on to assert case-specific facts.
/// Panics with both reports in the message on any structural disagreement —
/// EXCEPT where the oracle itself declines to decide, which is reported as
/// such and never as a solver defect.
fn differential(name: &str, sketch: &Sketch) -> (SolvedSketch, OracleReport) {
    let solved = solve_sketch(sketch);
    let oracle = analyze_at(sketch, &solved.positions, &solved.radii);
    let r = &solved.report;

    assert!(
        oracle.refused.is_empty(),
        "[{name}] the oracle refused to compile {} constraint(s), so its numbers \
         describe a different system from the solver's — investigate before comparing: {:?}\n  {}",
        oracle.refused.len(),
        oracle.refused,
        summarize(&oracle),
    );

    assert_eq!(
        r.params,
        oracle.params,
        "[{name}] parameter count: solver {} vs oracle {} \
         (the published layout is 2 per Point, 1 per Circle radius)\n  {}",
        r.params,
        oracle.params,
        summarize(&oracle),
    );

    assert_eq!(
        r.rows,
        oracle.rows,
        "[{name}] residual row count: solver {} vs oracle {} — the two are \
         solving systems of different SIZE, which makes every later number \
         incomparable\n  {}\n  solver rows per constraint: {:?}",
        r.rows,
        oracle.rows,
        summarize(&oracle),
        oracle
            .blocks
            .iter()
            .map(|b| (b.index, b.kind, b.rows))
            .collect::<Vec<_>>(),
    );

    match oracle.rank.decided() {
        Some(rank) => {
            assert_eq!(
                r.rank,
                rank,
                "[{name}] RANK: solver {} vs oracle {}\n  {}",
                r.rank,
                rank,
                summarize(&oracle),
            );
            let dof = oracle.dof.expect("a decided rank has a dof");
            assert_eq!(
                r.dof,
                dof,
                "[{name}] dof: solver {} vs oracle {}\n  {}",
                r.dof,
                dof,
                summarize(&oracle),
            );
            assert_eq!(
                oracle.null_dim,
                Some(dof),
                "[{name}] the oracle's own two routes to dof disagree: \
                 params-rank = {dof} but the null space counted \
                 {:?} directions\n  {}",
                oracle.null_dim,
                summarize(&oracle),
            );
            assert_eq!(
                oracle.free.len(),
                dof as usize,
                "[{name}] the oracle produced {} null-space directions for dof {dof}",
                oracle.free.len(),
            );
        }
        None => {
            // Not a failure: the oracle is saying the rank sits on its own
            // threshold. Recorded so a run that silently stopped checking
            // ranks is visible in the log.
            println!(
                "[{name}] oracle DECLINED a rank verdict: {}",
                summarize(&oracle)
            );
        }
    }

    // One direction only. When the oracle can see every row satisfied at the
    // configuration the solver returned, there is nothing for `conflicts` to
    // name, and a non-empty list is the solver blaming an innocent
    // constraint. The converse is NOT asserted: `conflicts` is computed on
    // WEIGHTED rows while the oracle weights nothing, which the S2 notes
    // record as an open defect, so an oracle-unsatisfiable row the solver
    // stays quiet about needs adjudicating case by case rather than a blanket
    // assertion.
    if oracle.unsatisfiable.is_empty() {
        assert!(
            r.conflicts.is_empty(),
            "[{name}] every constraint is satisfied at the solved configuration, \
             but report.conflicts names {:?}\n  {}\n  solver residuals: {:?}",
            r.conflicts,
            summarize(&oracle),
            r.residuals
                .iter()
                .map(|c| (c.index, c.kind.as_str(), c.residual, c.satisfied))
                .collect::<Vec<_>>(),
        );
    }

    (solved, oracle)
}

// ── Coverage: every constraint variant appears in the table ─────────────────

/// Every `SketchConstraint` serde tag, as `SketchConstraint::kind` spells them.
/// A new variant added to the vocabulary without a case here fails
/// `every_constraint_variant_is_exercised`.
const ALL_VARIANTS: &[&str] = &[
    "Coincident",
    "Horizontal",
    "Vertical",
    "HorizontalPoints",
    "VerticalPoints",
    "Parallel",
    "Perpendicular",
    "Tangent",
    "Equal",
    "Symmetric",
    "SymmetricH",
    "SymmetricV",
    "Midpoint",
    "Distance",
    "PointLineDistance",
    "HDistance",
    "VDistance",
    "Angle",
    "Radius",
    "Diameter",
    "OnEntity",
    "Dragged",
    "Pinned",
    "EqualAngle",
    "Ratio",
    "EqualPointToLine",
    "SameOrientation",
];

struct Case {
    name: &'static str,
    sketch: Sketch,
}

/// One satisfied, minimal sketch per constraint variant. Each is authored AT a
/// solution so the comparison is about structure, not about where LM landed.
fn vocabulary_cases() -> Vec<Case> {
    let mut cases = Vec::new();
    let mut push = |name: &'static str, sketch: Sketch| cases.push(Case { name, sketch });

    // Coincident: two points already on top of each other, one of them pinned.
    push(
        "coincident",
        make_sketch(
            vec![point(1, 2.0, 3.0), point(2, 2.0, 3.0)],
            vec![
                pin(1, 2.0, 3.0),
                SketchConstraint::Coincident {
                    point_a: 1,
                    point_b: 2,
                },
            ],
        ),
    );

    // Horizontal / Vertical / Pinned / HDistance / VDistance: the railed,
    // dimensioned rectangle.
    push(
        "rect_rails_and_dimensions",
        rect_fully_constrained(4.0, 3.0),
    );

    // The point-pair analogues.
    push(
        "horizontal_and_vertical_points",
        make_sketch(
            vec![point(1, 0.0, 0.0), point(2, 5.0, 0.0), point(3, 0.0, 7.0)],
            vec![
                pin(1, 0.0, 0.0),
                SketchConstraint::HorizontalPoints {
                    point_a: 1,
                    point_b: 2,
                },
                SketchConstraint::VerticalPoints {
                    point_a: 1,
                    point_b: 3,
                },
            ],
        ),
    );

    // Parallel + Perpendicular on three lines.
    push(
        "parallel_and_perpendicular",
        make_sketch(
            vec![
                point(1, 0.0, 0.0),
                point(2, 4.0, 0.0),
                point(3, 0.0, 2.0),
                point(4, 4.0, 2.0),
                point(5, 0.0, 0.0),
                point(6, 0.0, 3.0),
                line(10, 1, 2),
                line(11, 3, 4),
                line(12, 5, 6),
            ],
            vec![
                SketchConstraint::Parallel {
                    line_a: 10,
                    line_b: 11,
                },
                SketchConstraint::Perpendicular {
                    line_a: 10,
                    line_b: 12,
                },
            ],
        ),
    );

    // Tangent: the line y = 5 touches the radius-5 circle centred at the
    // origin. Authored AT tangency — away from it the oracle's linear form and
    // the solver's squared form have non-proportional gradients (see the arm's
    // CONSULTED comment).
    push(
        "tangent_line_circle",
        make_sketch(
            vec![
                point(1, 0.0, 0.0),
                point(2, -10.0, 5.0),
                point(3, 10.0, 5.0),
                line(10, 2, 3),
                circle(20, 1, 5.0),
            ],
            vec![SketchConstraint::Tangent {
                line: 10,
                curve: 20,
            }],
        ),
    );

    // Equal, over all three size-comparable operand shapes the vocabulary
    // admits: line/line, circle/circle, circle/arc.
    push(
        "equal_lines",
        make_sketch(
            vec![
                point(1, 0.0, 0.0),
                point(2, 3.0, 0.0),
                point(3, 0.0, 1.0),
                point(4, 3.0, 1.0),
                line(10, 1, 2),
                line(11, 3, 4),
            ],
            vec![SketchConstraint::Equal {
                entity_a: 10,
                entity_b: 11,
            }],
        ),
    );
    push(
        "equal_circle_arc",
        make_sketch(
            vec![
                point(1, 0.0, 0.0),
                point(2, 20.0, 0.0),
                point(3, 25.0, 0.0),
                point(4, 20.0, 5.0),
                circle(20, 1, 5.0),
                arc(30, 2, 3, 4),
            ],
            vec![SketchConstraint::Equal {
                entity_a: 20,
                entity_b: 30,
            }],
        ),
    );

    // Symmetric about a line: points (−2, 1) and (2, 1) across the y axis.
    push(
        "symmetric_about_line",
        make_sketch(
            vec![
                point(1, -2.0, 1.0),
                point(2, 2.0, 1.0),
                point(3, 0.0, 0.0),
                point(4, 0.0, 6.0),
                line(10, 3, 4),
            ],
            vec![SketchConstraint::Symmetric {
                entity_a: 1,
                entity_b: 2,
                symmetry_line: 10,
            }],
        ),
    );

    // SymmetricH / SymmetricV: no line entity, mirrored about the sketch axes.
    push(
        "symmetric_h",
        make_sketch(
            vec![point(1, -3.0, 2.0), point(2, 3.0, 2.0)],
            vec![SketchConstraint::SymmetricH {
                point_a: 1,
                point_b: 2,
            }],
        ),
    );
    push(
        "symmetric_v",
        make_sketch(
            vec![point(1, 3.0, -2.0), point(2, 3.0, 2.0)],
            vec![SketchConstraint::SymmetricV {
                point_a: 1,
                point_b: 2,
            }],
        ),
    );

    // Midpoint.
    push(
        "midpoint",
        make_sketch(
            vec![
                point(1, 0.0, 0.0),
                point(2, 6.0, 2.0),
                point(3, 3.0, 1.0),
                line(10, 1, 2),
            ],
            vec![SketchConstraint::Midpoint { point: 3, line: 10 }],
        ),
    );

    // Distance over both admitted operand pairs, and PointLineDistance, which
    // the solver's own tests assert is the same quantity as the (point, line)
    // arm. The off-line point sits LEFT of 1→2, where the oracle's
    // left-positive convention and an absolute one cannot be told apart.
    push(
        "distance_point_point_and_point_line",
        make_sketch(
            vec![
                point(1, 0.0, 0.0),
                point(2, 10.0, 0.0),
                point(3, 5.0, 7.0),
                line(10, 1, 2),
            ],
            vec![
                pp_distance(1, 2, 10.0),
                pp_distance(3, 10, 7.0),
                SketchConstraint::PointLineDistance {
                    point: 3,
                    entity: 10,
                    value: 7.0,
                    expression: None,
                    reference: false,
                },
            ],
        ),
    );

    // Angle: 1→2 along +x, 3→4 at 30°.
    let c30 = 30f64.to_radians().cos();
    let s30 = 30f64.to_radians().sin();
    push(
        "angle_30_degrees",
        make_sketch(
            vec![
                point(1, 0.0, 0.0),
                point(2, 1.0, 0.0),
                point(3, 0.0, 0.0),
                point(4, c30, s30),
                line(10, 1, 2),
                line(11, 3, 4),
            ],
            vec![SketchConstraint::Angle {
                line_a: 10,
                line_b: 11,
                value_degrees: 30.0,
                expression: None,
                reference: false,
            }],
        ),
    );

    // Radius and Diameter, on a circle and on an arc.
    push(
        "radius_and_diameter",
        make_sketch(
            vec![
                point(1, 0.0, 0.0),
                point(2, 20.0, 0.0),
                point(3, 23.0, 0.0),
                point(4, 20.0, 3.0),
                circle(20, 1, 5.0),
                arc(30, 2, 3, 4),
            ],
            vec![
                SketchConstraint::Radius {
                    entity: 20,
                    value: 5.0,
                    expression: None,
                    reference: false,
                },
                SketchConstraint::Diameter {
                    entity: 30,
                    value: 6.0,
                    expression: None,
                    reference: false,
                },
            ],
        ),
    );

    // OnEntity, on a line and on a circle.
    push(
        "on_entity_line_and_circle",
        make_sketch(
            vec![
                point(1, 0.0, 0.0),
                point(2, 10.0, 0.0),
                point(3, 4.0, 0.0),
                point(4, 20.0, 0.0),
                point(5, 25.0, 0.0),
                line(10, 1, 2),
                circle(20, 4, 5.0),
            ],
            vec![
                SketchConstraint::OnEntity {
                    point: 3,
                    entity: 10,
                },
                SketchConstraint::OnEntity {
                    point: 5,
                    entity: 20,
                },
            ],
        ),
    );

    // Dragged: the interaction hint, holding the point where it was sent.
    push(
        "dragged",
        make_sketch(
            vec![point(1, 0.0, 0.0), point(2, 4.0, 1.0), line(10, 1, 2)],
            vec![pin(1, 0.0, 0.0), SketchConstraint::Dragged { point: 2 }],
        ),
    );

    // EqualAngle: two pairs of lines, both at 0°.
    push(
        "equal_angle",
        make_sketch(
            vec![
                point(1, 0.0, 0.0),
                point(2, 1.0, 0.0),
                point(3, 0.0, 1.0),
                point(4, 1.0, 1.0),
                point(5, 0.0, 2.0),
                point(6, 1.0, 2.0),
                point(7, 0.0, 3.0),
                point(8, 1.0, 3.0),
                line(10, 1, 2),
                line(11, 3, 4),
                line(12, 5, 6),
                line(13, 7, 8),
            ],
            vec![SketchConstraint::EqualAngle {
                line_a: 10,
                line_b: 11,
                line_c: 12,
                line_d: 13,
            }],
        ),
    );

    // Ratio: a 6-long line is twice a 3-long one.
    push(
        "ratio",
        make_sketch(
            vec![
                point(1, 0.0, 0.0),
                point(2, 6.0, 0.0),
                point(3, 0.0, 1.0),
                point(4, 3.0, 1.0),
                line(10, 1, 2),
                line(11, 3, 4),
            ],
            vec![SketchConstraint::Ratio {
                entity_a: 10,
                entity_b: 11,
                value: 2.0,
            }],
        ),
    );

    // EqualPointToLine: two points the same (signed) distance from one line.
    push(
        "equal_point_to_line",
        make_sketch(
            vec![
                point(1, 0.0, 0.0),
                point(2, 10.0, 0.0),
                point(3, 2.0, 4.0),
                point(4, 8.0, 4.0),
                line(10, 1, 2),
            ],
            vec![SketchConstraint::EqualPointToLine {
                point_a: 3,
                point_b: 4,
                line: 10,
            }],
        ),
    );

    // SameOrientation: a documented 2D no-op, owning zero rows on both sides.
    push(
        "same_orientation",
        make_sketch(
            vec![
                point(1, 0.0, 0.0),
                point(2, 10.0, 0.0),
                point(3, 0.0, 5.0),
                point(4, 10.0, 5.0),
                line(10, 1, 2),
                line(11, 3, 4),
            ],
            vec![
                pin(1, 0.0, 0.0),
                SketchConstraint::Horizontal { entity: 10 },
                SketchConstraint::SameOrientation {
                    entity_a: 10,
                    entity_b: 11,
                },
            ],
        ),
    );

    cases
}

#[test]
fn every_constraint_variant_is_exercised() {
    let cases = vocabulary_cases();
    let mut seen: Vec<&str> = Vec::new();
    for case in &cases {
        for c in &case.sketch.constraints {
            if !seen.contains(&c.kind()) {
                seen.push(c.kind());
            }
        }
    }
    let missing: Vec<&&str> = ALL_VARIANTS.iter().filter(|v| !seen.contains(v)).collect();
    assert!(
        missing.is_empty(),
        "the vocabulary table misses {} constraint variant(s): {missing:?}. \
         A variant with no case has no independent check of its row count or \
         its gradient's rank contribution.",
        missing.len(),
    );
    // And nothing in the table is outside the vocabulary the list names.
    for s in &seen {
        assert!(
            ALL_VARIANTS.contains(s),
            "case table uses constraint kind {s:?}, which ALL_VARIANTS does not list"
        );
    }
}

#[test]
fn the_oracle_agrees_with_the_report_on_every_vocabulary_case() {
    for case in vocabulary_cases() {
        let (solved, oracle) = differential(case.name, &case.sketch);
        println!(
            "[{}] solver: params={} rows={} rank={} dof={} redundant={:?} conflicts={:?} \
             status={:?}\n    {}",
            case.name,
            solved.report.params,
            solved.report.rows,
            solved.report.rank,
            solved.report.dof,
            solved.report.redundant,
            solved.report.conflicts,
            solved.status,
            summarize(&oracle),
        );
    }
}

// ── Under-constrained shapes ────────────────────────────────────────────────

#[test]
fn a_lone_line_has_four_free_parameters() {
    // Two endpoint points, no constraints: 4 params, 0 rows, rank 0, dof 4.
    // Done by hand: the Jacobian is the 0×4 matrix, whose rank is 0.
    let sketch = make_sketch(
        vec![point(1, 0.0, 0.0), point(2, 3.0, 4.0), line(10, 1, 2)],
        vec![],
    );
    let (solved, oracle) = differential("lone_line", &sketch);
    assert_eq!(oracle.dof, Some(4));
    assert_eq!(solved.report.dof, 4);
    assert_eq!(
        solved.report.free.len(),
        solved.report.dof as usize,
        "free.len() must equal dof — the report's own documented invariant"
    );
}

#[test]
fn an_undimensioned_rectangle_has_eight_free_parameters() {
    let sketch = make_sketch(rect_entities(4.0, 3.0), vec![]);
    let (solved, oracle) = differential("rect_no_constraints", &sketch);
    assert_eq!(oracle.dof, Some(8));
    assert_eq!(solved.report.dof, 8);
    assert_eq!(solved.report.free.len(), solved.report.dof as usize);
}

#[test]
fn a_railed_rectangle_without_dimensions_keeps_width_and_height_free() {
    // Four H/V rails plus a 2-row pin = 6 rows over 8 params. The rails are
    // independent (each touches a different coordinate pair) and so is the
    // pin, so rank 6 and dof 2: width and height.
    let sketch = make_sketch(rect_entities(4.0, 3.0), rect_rails());
    let (solved, oracle) = differential("rect_rails_only", &sketch);
    assert_eq!(oracle.rows, 6);
    assert_eq!(oracle.dof, Some(2));
    assert_eq!(solved.report.dof, 2);
    assert_eq!(solved.report.free.len(), 2);
}

// ── Fully constrained shapes ────────────────────────────────────────────────

#[test]
fn a_dimensioned_railed_rectangle_is_fully_constrained() {
    let sketch = rect_fully_constrained(4.0, 3.0);
    let (solved, oracle) = differential("rect_full", &sketch);
    assert_eq!(oracle.params, 8);
    assert_eq!(oracle.rows, 8);
    assert_eq!(oracle.dof, Some(0));
    assert_eq!(solved.report.dof, 0);
    assert!(
        matches!(solved.status, SolveStatus::FullyConstrained),
        "expected FullyConstrained, got {:?}",
        solved.status
    );
    assert!(
        oracle.dependent.is_empty(),
        "no constraint here is dependent, oracle says {:?}",
        oracle.dependent
    );
    assert!(solved.report.redundant.is_empty());
}

#[test]
fn a_circle_with_a_radius_and_a_pinned_centre_is_fully_constrained() {
    // 3 params (centre x, centre y, radius); 3 rows (the 2-row pin and the
    // radius). Each row touches a different parameter, so rank 3, dof 0.
    let sketch = make_sketch(
        vec![point(1, 1.0, 2.0), circle(20, 1, 5.0)],
        vec![
            pin(1, 1.0, 2.0),
            SketchConstraint::Radius {
                entity: 20,
                value: 5.0,
                expression: None,
                reference: false,
            },
        ],
    );
    let (solved, oracle) = differential("circle_pinned_radius", &sketch);
    assert_eq!(oracle.params, 3, "2 for the centre point, 1 for the radius");
    assert_eq!(oracle.rows, 3);
    assert_eq!(oracle.dof, Some(0));
    assert_eq!(solved.report.dof, 0);
    assert!(matches!(solved.status, SolveStatus::FullyConstrained));
}

// ── Redundant but satisfied ─────────────────────────────────────────────────

#[test]
fn a_duplicate_horizontal_is_dependent_in_both_computations() {
    // The fully constrained rectangle plus a SECOND Horizontal on line 10.
    // 9 rows over 8 params with every row satisfied: rank 8, so exactly one
    // row adds nothing, and the greedy declaration-order walk names the later
    // duplicate (index 7).
    let mut cs = rect_rails();
    cs.push(hdist(1, 2, 4.0));
    cs.push(vdist(2, 3, 3.0));
    cs.push(SketchConstraint::Horizontal { entity: 10 });
    let sketch = make_sketch(rect_entities(4.0, 3.0), cs);

    let (solved, oracle) = differential("rect_duplicate_horizontal", &sketch);
    assert_eq!(oracle.rows, 9);
    assert_eq!(oracle.dof, Some(0));
    assert_eq!(
        oracle.dependent,
        vec![7],
        "the duplicate Horizontal is constraint 7, and a declaration-order \
         walk blames the LATER duplicate"
    );
    assert_eq!(
        solved.report.redundant, oracle.dependent,
        "report.redundant must name the same dependent rows the oracle found"
    );
    assert!(
        solved.report.conflicts.is_empty(),
        "a redundant-but-satisfied system has no conflict: {:?}",
        solved.report.conflicts
    );
}

#[test]
fn horizontal_on_both_rails_plus_parallel_is_dependent() {
    // Horizontal on lines 10 and 12 already forces them parallel, so a
    // Parallel(10, 12) on top adds no rank. 9 rows, rank 8, dof 0, and the
    // Parallel (declared last, index 7) is the dependent one.
    let mut cs = rect_rails();
    cs.push(hdist(1, 2, 4.0));
    cs.push(vdist(2, 3, 3.0));
    cs.push(SketchConstraint::Parallel {
        line_a: 10,
        line_b: 12,
    });
    let sketch = make_sketch(rect_entities(4.0, 3.0), cs);

    let (solved, oracle) = differential("rect_parallel_redundant", &sketch);
    assert_eq!(oracle.rows, 9);
    assert_eq!(oracle.dof, Some(0));
    assert_eq!(oracle.dependent, vec![7]);
    assert_eq!(solved.report.redundant, oracle.dependent);
}

// ── Over-constrained / contradictory ────────────────────────────────────────

#[test]
fn two_different_distances_on_one_line_are_rank_deficient_and_in_conflict() {
    // Points 1 and 2, pinned at 1, with Distance(1,2) asked to be both 4 and
    // 5. By hand: params 4; rows 2 (pin) + 1 + 1 = 4; the two distance rows
    // have the SAME gradient direction (the unit vector along 1→2), so rank is
    // 2 + 1 = 3 and dof is 1. One of the two distances cannot be satisfied.
    let sketch = make_sketch(
        vec![point(1, 0.0, 0.0), point(2, 4.0, 0.0)],
        vec![
            pin(1, 0.0, 0.0),
            pp_distance(1, 2, 4.0),
            pp_distance(1, 2, 5.0),
        ],
    );
    let (solved, oracle) = differential("two_distances_conflict", &sketch);

    assert_eq!(oracle.rows, 4);
    assert_eq!(
        oracle.rank.decided(),
        Some(3),
        "rank is deficient by one row"
    );
    assert_eq!(oracle.dof, Some(1));
    assert!(
        !oracle.unsatisfiable.is_empty(),
        "the oracle must see an unsatisfiable row here"
    );
    assert!(
        !solved.report.conflicts.is_empty(),
        "report.conflicts must name an offender under a contradictory system"
    );
    // The offender must be one of the two Distances (indices 1 and 2) — never
    // the pin, which is satisfiable and satisfied.
    for idx in &solved.report.conflicts {
        assert!(
            *idx == 1 || *idx == 2,
            "conflicts names constraint {idx}, which is not one of the two \
             contradictory Distances (1 and 2): {:?}",
            solved.report.conflicts
        );
    }
    for idx in &oracle.unsatisfiable {
        assert!(
            *idx == 1 || *idx == 2,
            "the oracle blames constraint {idx}, not one of the Distances"
        );
    }
}

#[test]
fn an_overdimensioned_pinned_line_names_the_dimension_and_measures_its_violation() {
    // Both endpoints hard-pinned 4 apart, with a Distance asking for 5.
    //
    // By hand: params 4 (two points); rows 2 + 2 + 1 = 5; the two pins already
    // fix all four parameters, so the Distance row lies in their span — rank 4
    // < rows 5, dof 0, and the system is contradictory rather than redundant.
    //
    // With no free parameter to absorb the error, least squares SPLITS it.
    // Write the symmetric solution as p₁ = (-t, 0), p₂ = (4 + t, 0), where the
    // length is 4 + 2t:
    //
    //     minimize  t² + t² + (4 + 2t - 5)²  =  2t² + (2t - 1)²
    //     d/dt      4t + 4(2t - 1) = 12t - 4 = 0   ⇒   t = 1/3
    //
    // so p₁ = (-1/3, 0), p₂ = (13/3, 0), the length is 14/3, and ALL THREE
    // constraints end up violated by exactly 1/3: each pin by t = 1/3, the
    // dimension by 14/3 - 5 = -1/3. Measured: p₁ = (-0.3333333333222222, 5.6e-17),
    // p₂ = (4.333333333322222, 0), residuals 0.3333333333222222,
    // 0.3333333333222219, 0.33333333335555615 — 1/3 to eleven digits, the
    // remainder being the solver's 1e-5 proximal pull toward the input.
    //
    // The oracle and the report agree on all of it, including the ORDER:
    // `conflicts` is documented worst-first, the dimension's residual is
    // marginally the largest of the three, and both lists come out [2, 0, 1].
    // This is also the one place the oracle's residual VALUE is compared with
    // the report's. Point-to-point distance is the one dimensional form whose
    // published equation (`‖P₁ - P₂‖ - d`) is unambiguous — no sign choice
    // (`HDistance`), no factor of two (`Diameter`), no squaring (`Tangent`) —
    // so a disagreement here would be a real one rather than a convention.
    let sketch = make_sketch(
        vec![point(1, 0.0, 0.0), point(2, 4.0, 0.0)],
        vec![pin(1, 0.0, 0.0), pin(2, 4.0, 0.0), pp_distance(1, 2, 5.0)],
    );
    let (solved, oracle) = differential("overdimensioned_pinned_line", &sketch);

    assert_eq!(oracle.rows, 5);
    assert_eq!(oracle.rank.decided(), Some(4));
    assert_eq!(oracle.dof, Some(0));
    assert_eq!(
        oracle.unsatisfiable,
        vec![2, 0, 1],
        "least squares splits an unsatisfiable system across every row that \
         touches it, worst first"
    );
    assert_eq!(
        solved.report.conflicts, oracle.unsatisfiable,
        "report.conflicts must name the same offenders in the same order"
    );
    assert!(
        matches!(solved.status, SolveStatus::OverConstrained { .. }),
        "expected OverConstrained, got {:?}",
        solved.status
    );
    assert!(
        solved.report.redundant.is_empty(),
        "redundancy is a verdict about a SATISFIED system; this one is not"
    );

    let third = 1.0 / 3.0;
    for i in 0..3 {
        let reported = solved.report.residuals[i]
            .residual
            .expect("a driving constraint always has a residual");
        let from_oracle = oracle.blocks[i].residual;
        assert!(
            (reported - third).abs() < 1e-9,
            "constraint {i}: the report's residual should be the 1/3 split, \
             got {reported}"
        );
        assert!(
            (from_oracle - reported).abs() < 1e-9,
            "constraint {i}: oracle residual {from_oracle} vs report residual \
             {reported}"
        );
    }
}

// ── The report's index space is the caller's FULL constraint array ──────────

#[test]
fn report_indices_are_in_the_callers_full_array_including_reference_dimensions() {
    // A reference dimension sits at index 0, BEFORE everything else. It is
    // filtered out of the driving set, so if the report indexed the FILTERED
    // array every later index would be one too small. The redundant duplicate
    // Horizontal is at caller index 8.
    let mut cs: Vec<SketchConstraint> = vec![SketchConstraint::Distance {
        entity_a: 1,
        entity_b: 3,
        value: 5.0,
        expression: None,
        reference: true, // the diagonal, measured, never driving
    }];
    cs.extend(rect_rails()); // indices 1..5
    cs.push(hdist(1, 2, 4.0)); // 6
    cs.push(vdist(2, 3, 3.0)); // 7
    cs.push(SketchConstraint::Horizontal { entity: 10 }); // 8 — the duplicate
    let sketch = make_sketch(rect_entities(4.0, 3.0), cs);

    let (solved, oracle) = differential("reference_then_duplicate", &sketch);

    assert_eq!(
        oracle.rows, 9,
        "the reference dimension owns no row in a driving system"
    );
    assert_eq!(oracle.dof, Some(0));
    assert_eq!(
        oracle.dependent,
        vec![8],
        "the duplicate Horizontal is at CALLER index 8 (7 in the filtered array)"
    );
    assert_eq!(
        solved.report.redundant, oracle.dependent,
        "report.redundant must be in the caller's full index space — a filtered \
         index space would say 7 here"
    );
    assert_eq!(
        solved.report.residuals.len(),
        9,
        "residuals carry one entry per constraint the caller handed in, \
         reference dimensions included"
    );
    assert!(
        solved.report.residuals[0].reference,
        "the entry at caller index 0 must be the reference dimension"
    );
    assert_eq!(solved.report.residuals[0].index, 0);
}

// ── A signed residual behind an unsigned dimension ──────────────────────────

#[test]
fn a_point_line_distance_dimension_mirrors_the_point_across_the_line() {
    // DEFECT, pinned at the current measured behaviour and NOT fixed here.
    //
    // `PointLineDistance`'s doc says "Perpendicular distance between a point
    // and a line", and both places the app mints one emit an UNSIGNED value:
    // `dimensionHeuristic.js`'s `pointLineDistance` and `constraintLogic.js`'s
    // `result.pointLineDistance` are each `Math.abs(cross)/len`. The solver's
    // residual subtracts that value from a SIGNED perpendicular distance, so
    // on whichever side of the line the sign comes out negative the dimension
    // is unsatisfiable WHERE THE GEOMETRY ALREADY IS, and the solve satisfies
    // it by mirroring the point across the line instead.
    //
    // By hand. Line 10 runs 1→2 from (0, 0) to (10, 0), so d = (10, 0) and
    // |d| = 10. Point 3 is authored at (5, 7), seven above the line. Both
    // endpoints are pinned, so the line cannot move. The dimension asks for 7
    // — the distance the point is ALREADY at.
    //
    //   measured: the solve puts point 3 at (5, -6.999999997900005) and calls
    //   the constraint satisfied (residual 1.4e-9).
    //
    // That is the mirror image, 7 BELOW the line. Flipping the sign of the
    // request (value = -7.0) leaves the point exactly where it was authored,
    // with residual 0.0 — which identifies the solver's convention as
    // cross(p - s, d)/|d|, positive to the RIGHT of start→end, the opposite
    // handedness from the app's magnitude.
    //
    // The oracle's arm is the unsigned form (the stored value's meaning), so
    // it disagrees — and the disagreement is the solver's: a dimension that
    // reports the distance the geometry already has must not move the
    // geometry. Note the solver's own
    // `distance_point_line_and_line_point_compile_identically` cannot see
    // this: it asserts `|forward| == 7.0`, discarding the sign.
    let authored = |value: f64| {
        make_sketch(
            vec![
                point(1, 0.0, 0.0),
                point(2, 10.0, 0.0),
                point(3, 5.0, 7.0),
                line(10, 1, 2),
            ],
            vec![
                pin(1, 0.0, 0.0),
                pin(2, 10.0, 0.0),
                SketchConstraint::PointLineDistance {
                    point: 3,
                    entity: 10,
                    value,
                    expression: None,
                    reference: false,
                },
            ],
        )
    };

    let positive = solve_sketch(&authored(7.0));
    let p3 = positive.positions[&3];
    assert!(
        (p3.0 - 5.0).abs() < 1e-9 && (p3.1 + 7.0).abs() < 1e-8,
        "DEFECT PIN: expected the measured mirror image (5, -7), got {p3:?}. \
         If this now reports (5, +7) the signed residual has been fixed — \
         delete this pin and assert the fix instead."
    );
    assert!(
        positive.report.residuals[2].satisfied,
        "the solver considers the mirrored configuration satisfied"
    );

    // The negative request is the one that leaves the authored geometry alone,
    // which is what identifies the handedness.
    let negative = solve_sketch(&authored(-7.0));
    let q3 = negative.positions[&3];
    assert!(
        (q3.0 - 5.0).abs() < 1e-12 && (q3.1 - 7.0).abs() < 1e-12,
        "a value of -7 should leave the authored point untouched, got {q3:?}"
    );
    assert_eq!(negative.report.residuals[2].residual, Some(0.0));
}

// ── Scale ───────────────────────────────────────────────────────────────────

/// The 4:3 rectangle, railed, pinned and dimensioned, authored `rel` off its
/// solution at scale `s` so LM actually has to converge. Point 1 is authored
/// EXACTLY on its pin, so any displacement reported for it is the solver's own
/// numerical settle and not motion the caller asked for.
fn rect_off_solution(s: f64, rel: f64) -> Sketch {
    let (w, h) = (4.0 * s, 3.0 * s);
    let entities = vec![
        point(1, 0.0, 0.0),
        point(2, w * (1.0 + rel), rel / 2.0 * s),
        point(3, w * (1.0 - rel), h * (1.0 + rel * 0.6)),
        point(4, -rel * s, h * (1.0 - rel * 0.6)),
        line(10, 1, 2),
        line(11, 2, 3),
        line(12, 3, 4),
        line(13, 4, 1),
    ];
    let mut cs = rect_rails();
    cs.push(hdist(1, 2, w));
    cs.push(vdist(2, 3, h));
    make_sketch(entities, cs)
}

#[test]
fn the_structural_numbers_are_the_same_at_every_scale() {
    // The required S4 row: the same satisfied rectangle at 1e-3 m, 1 m and
    // 1e3 m. Structure is scale-free — params, rows, rank and dof must be the
    // same three times, authored exactly at the solution and authored 3 % off
    // it — and it is, in both computations.
    for (label, s) in [("1e-3", 1e-3), ("1", 1.0), ("1e3", 1e3)] {
        for (how, sketch) in [
            ("exact", rect_fully_constrained(4.0 * s, 3.0 * s)),
            ("3%off", rect_off_solution(s, 0.03)),
        ] {
            let (solved, oracle) = differential(&format!("rect_scale_{label}_{how}"), &sketch);
            assert_eq!(oracle.params, 8);
            assert_eq!(oracle.rows, 8);
            assert_eq!(
                oracle.dof,
                Some(0),
                "[{label}/{how}] the rectangle is fully constrained at every scale"
            );
            assert_eq!(solved.report.dof, 0, "[{label}/{how}] solver dof");
            assert!(
                matches!(solved.status, SolveStatus::FullyConstrained),
                "[{label}/{how}] expected FullyConstrained, got {:?}",
                solved.status
            );
            println!(
                "[{label}/{how}] residual_inf={:.4e} moved={:?} oracle worst \
                 variety distance={:.4e}",
                solved.report.convergence.residual_inf,
                solved
                    .report
                    .moved
                    .iter()
                    .map(|m| (m.id, m.distance))
                    .collect::<Vec<_>>(),
                oracle
                    .blocks
                    .iter()
                    .filter_map(|b| b.variety_distance)
                    .fold(0.0f64, f64::max),
            );
        }
    }
}

#[test]
fn an_absolute_solve_tol_refuses_the_same_rectangle_once_it_is_authored_large() {
    // KNOWN OPEN DEFECT, pinned at the current measured behaviour and NOT
    // fixed here. `specs/agent_mechanical_design.md` §"S2 — solver state":
    // "`SOLVE_TOL` is absolute for the same reason and already flips the same
    // rectangle to `SolveFailed`".
    //
    // The same 4:3 rectangle, authored 3 % off its solution, measured at four
    // scales. The relative conditioning is IDENTICAL at all four — the
    // oracle's worst first-order distance to the constraint varieties is
    // 2.12e-12 of the rectangle's own width every time — and so is the
    // relative residual, which tracks the scale exactly:
    //
    //   scale 1    → residual_inf 1.2000e-11, FullyConstrained
    //   scale 1e3  → residual_inf 1.2000e-8,  FullyConstrained
    //   scale 1e4  → residual_inf 1.2000e-7,  FullyConstrained
    //   scale 1e5  → residual_inf 1.2000e-6,  SolveFailed
    //                ("Orthogonal (2 evaluations)")
    //
    // 1.2e-6 is the first value above the absolute `SOLVE_TOL` of 1e-6, and
    // the verdict flips there and nowhere else. Nothing about the geometry
    // changed: the report's own `rank` is 8 and `dof` 0 at every scale,
    // including the two that fail, and the oracle agrees. The number that
    // changed is the threshold's units.
    let mut prev_inf: Option<(f64, f64)> = None; // (scale, residual_inf)
    for (label, s) in [("1", 1.0), ("1e3", 1e3), ("1e4", 1e4), ("1e5", 1e5)] {
        let sketch = rect_off_solution(s, 0.03);
        let solved = solve_sketch(&sketch);
        let inf = solved.report.convergence.residual_inf;
        assert_eq!(
            solved.report.rank, 8,
            "[{label}] the structure is right at every scale"
        );
        assert_eq!(solved.report.dof, 0, "[{label}] dof");
        assert_eq!(
            solved.report.convergence.tolerance, 1e-6,
            "[{label}] the verdict tolerance is the absolute SOLVE_TOL"
        );

        // The residual tracks the scale: ten times the model, ten times the
        // absolute residual, at the same relative accuracy.
        if let Some((ps, pi)) = prev_inf {
            let ratio = (inf / pi) / (s / ps);
            assert!(
                (ratio - 1.0).abs() < 1e-3,
                "[{label}] residual_inf should scale with the model: \
                 {pi:.6e} at {ps} → {inf:.6e} at {s} (ratio/scale = {ratio})"
            );
        }
        prev_inf = Some((s, inf));

        if s < 1e5 {
            assert!(
                matches!(solved.status, SolveStatus::FullyConstrained),
                "[{label}] expected FullyConstrained, got {:?}",
                solved.status
            );
        } else {
            // DEFECT. Were the tolerance relative, this would be green too.
            assert!(
                matches!(solved.status, SolveStatus::SolveFailed { .. }),
                "[{label}] DEFECT PIN: expected the measured SolveFailed, got \
                 {:?}. If this is now FullyConstrained, SOLVE_TOL has been \
                 made relative — delete this pin and assert the fix.",
                solved.status
            );
            assert!(
                inf > 1e-6 && inf < 2e-6,
                "[{label}] the residual that trips the absolute tolerance was \
                 measured at 1.2000e-6, got {inf:.6e}"
            );
        }
        println!(
            "[solve_tol scale {label}] status={:?} residual_inf={inf:.6e}",
            solved.status
        );
    }
}

#[test]
fn an_absolute_moved_eps_lists_a_pinned_point_as_moved_once_the_sketch_is_large() {
    // KNOWN OPEN DEFECT, pinned at the current measured behaviour and NOT
    // fixed here. `specs/agent_mechanical_design.md` §"S2 — solver state":
    // "`MOVED_EPS` is absolute (1e-9). A satisfied rectangle authored at
    // 1000 m settles ~4e-7 and lists its own PINNED origin as `moved`, which
    // the constant's comment says cannot happen."
    //
    // Point 1 is authored EXACTLY at (0, 0) and `Pinned` there, so it has no
    // motion to report; points 2, 3 and 4 are 3 % off and really do move.
    // Measured:
    //
    //   scale 1    → moved = {3, 2, 4}        — point 1 correctly absent
    //   scale 1e3  → moved = {3, 2, 4, 1}     — point 1 at 3.3541e-9 m
    //   scale 1e4  → moved = {3, 2, 4, 1}     — point 1 at 3.3541e-8 m
    //
    // The pinned point's "displacement" is LM's last few digits: 3.3541e-9 on
    // a 4000 m rectangle is 8.4e-13 RELATIVE, and it scales exactly with the
    // model (×10 for ×10). Membership of `moved` therefore depends on the
    // units the author chose and not on anything that moved.
    let moved_ids = |s: f64| -> Vec<u32> {
        let solved = solve_sketch(&rect_off_solution(s, 0.03));
        assert!(
            matches!(solved.status, SolveStatus::FullyConstrained),
            "[scale {s}] expected a converged solve, got {:?}",
            solved.status
        );
        let mut ids: Vec<u32> = solved.report.moved.iter().map(|m| m.id).collect();
        ids.sort_unstable();
        println!(
            "[moved_eps scale {s}] {:?}",
            solved
                .report
                .moved
                .iter()
                .map(|m| (m.id, m.distance))
                .collect::<Vec<_>>()
        );
        ids
    };

    assert_eq!(
        moved_ids(1.0),
        vec![2, 3, 4],
        "at metre scale the pinned origin is correctly NOT listed as moved"
    );

    let big = solve_sketch(&rect_off_solution(1e3, 0.03));
    let pinned = big
        .report
        .moved
        .iter()
        .find(|m| m.id == 1)
        .unwrap_or_else(|| {
            panic!(
                "DEFECT PIN: the pinned origin was measured as `moved` at km \
                 scale and is no longer listed. If MOVED_EPS has been made \
                 relative, delete this pin and assert the fix. moved = {:?}",
                big.report
                    .moved
                    .iter()
                    .map(|m| (m.id, m.distance))
                    .collect::<Vec<_>>()
            )
        });
    assert!(
        pinned.distance > 1e-9 && pinned.distance < 1e-8,
        "the pinned origin's spurious displacement was measured at 3.3541e-9, \
         got {:.6e}",
        pinned.distance
    );
    assert!(
        pinned.distance / 4e3 < 1e-12,
        "and it is {:.3e} RELATIVE to the rectangle's width — nothing moved",
        pinned.distance / 4e3
    );
    // The constraint it belongs to is a hard pin, and the solver itself still
    // reports it satisfied: the geometry is right, the threshold is wrong.
    assert!(
        big.report.residuals[4].satisfied,
        "the Pinned constraint (index 4) is satisfied: {:?}",
        big.report.residuals[4]
    );
}
