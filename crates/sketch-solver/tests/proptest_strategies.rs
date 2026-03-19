#![allow(dead_code)]
//! Reusable proptest strategies for generating random sketch geometries.
//!
//! These strategies build valid `Sketch` values that the solver can consume.
//! Each strategy produces geometry with known constraints so tests can verify
//! that the solver recovers the correct solution after perturbation.

use proptest::prelude::*;
use sketch_solver::*;
use std::collections::HashMap;
use uuid::Uuid;

// ── Sketch construction helpers ─────────────────────────────────────────

pub fn dummy_geom_ref() -> GeomRef {
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

pub fn make_sketch(entities: Vec<SketchEntity>, constraints: Vec<SketchConstraint>) -> Sketch {
    Sketch {
        id: Uuid::new_v4(),
        plane: dummy_geom_ref(),
        plane_origin: [0.0, 0.0, 0.0],
        plane_normal: [0.0, 0.0, 1.0],
        entities,
        constraints,
        solve_status: SolveStatus::UnderConstrained { dof: 99 },
        solved_positions: HashMap::new(),
        solved_profiles: Vec::new(),
    }
}

// ── Primitive strategies ────────────────────────────────────────────────

/// Random coordinate in [-100, 100].
pub fn arb_coord() -> impl Strategy<Value = f64> {
    -100.0..100.0f64
}

/// Random point coordinates in [-100, 100]².
pub fn arb_point_coords() -> impl Strategy<Value = (f64, f64)> {
    (arb_coord(), arb_coord())
}

/// Random positive distance in [min_d, max_d].
pub fn arb_distance(min_d: f64, max_d: f64) -> impl Strategy<Value = f64> {
    min_d..max_d
}

// ── Constrained rectangle ───────────────────────────────────────────────

/// Generate a random fully-constrained rectangle.
///
/// Creates 4 points, 4 lines, H/V constraints, 2 distance constraints,
/// and a Dragged constraint to pin the origin. The rectangle has random
/// width in [5, 200] and height in [5, 200].
pub fn arb_constrained_rectangle() -> impl Strategy<Value = (Sketch, f64, f64)> {
    (5.0..200.0f64, 5.0..200.0f64).prop_map(|(w, h)| {
        let entities = vec![
            SketchEntity::Point { id: 1, x: 0.0, y: 0.0, construction: false },
            SketchEntity::Point { id: 2, x: w, y: 0.0, construction: false },
            SketchEntity::Point { id: 3, x: w, y: h, construction: false },
            SketchEntity::Point { id: 4, x: 0.0, y: h, construction: false },
            SketchEntity::Line { id: 10, start_id: 1, end_id: 2, construction: false },
            SketchEntity::Line { id: 11, start_id: 2, end_id: 3, construction: false },
            SketchEntity::Line { id: 12, start_id: 3, end_id: 4, construction: false },
            SketchEntity::Line { id: 13, start_id: 4, end_id: 1, construction: false },
        ];
        let constraints = vec![
            SketchConstraint::Horizontal { entity: 10 },
            SketchConstraint::Horizontal { entity: 12 },
            SketchConstraint::Vertical { entity: 11 },
            SketchConstraint::Vertical { entity: 13 },
            SketchConstraint::Distance { entity_a: 1, entity_b: 2, value: w },
            SketchConstraint::Distance { entity_a: 2, entity_b: 3, value: h },
            SketchConstraint::Dragged { point: 1 },
        ];
        (make_sketch(entities, constraints), w, h)
    })
}

/// Generate a rectangle with perturbed initial positions.
/// Returns (sketch_with_perturbed_positions, expected_width, expected_height).
pub fn arb_perturbed_rectangle() -> impl Strategy<Value = (Sketch, f64, f64)> {
    (
        5.0..200.0f64,
        5.0..200.0f64,
        proptest::collection::vec(-0.1..0.1f64, 8), // 4 points * 2 coords
    )
        .prop_map(|(w, h, perturbations)| {
            // True positions
            let pts = [(0.0, 0.0), (w, 0.0), (w, h), (0.0, h)];
            let entities = vec![
                SketchEntity::Point {
                    id: 1,
                    x: pts[0].0 * (1.0 + perturbations[0]),
                    y: pts[0].1 + perturbations[1] * h.max(1.0),
                    construction: false,
                },
                SketchEntity::Point {
                    id: 2,
                    x: pts[1].0 * (1.0 + perturbations[2]),
                    y: pts[1].1 + perturbations[3] * h.max(1.0),
                    construction: false,
                },
                SketchEntity::Point {
                    id: 3,
                    x: pts[2].0 * (1.0 + perturbations[4]),
                    y: pts[2].1 * (1.0 + perturbations[5]),
                    construction: false,
                },
                SketchEntity::Point {
                    id: 4,
                    x: pts[3].0 + perturbations[6] * w.max(1.0),
                    y: pts[3].1 * (1.0 + perturbations[7]),
                    construction: false,
                },
                SketchEntity::Line { id: 10, start_id: 1, end_id: 2, construction: false },
                SketchEntity::Line { id: 11, start_id: 2, end_id: 3, construction: false },
                SketchEntity::Line { id: 12, start_id: 3, end_id: 4, construction: false },
                SketchEntity::Line { id: 13, start_id: 4, end_id: 1, construction: false },
            ];
            let constraints = vec![
                SketchConstraint::Horizontal { entity: 10 },
                SketchConstraint::Horizontal { entity: 12 },
                SketchConstraint::Vertical { entity: 11 },
                SketchConstraint::Vertical { entity: 13 },
                SketchConstraint::Distance { entity_a: 1, entity_b: 2, value: w },
                SketchConstraint::Distance { entity_a: 2, entity_b: 3, value: h },
                SketchConstraint::Dragged { point: 1 },
            ];
            (make_sketch(entities, constraints), w, h)
        })
}

// ── Constrained triangle ────────────────────────────────────────────────

/// Generate a random fully-constrained triangle.
///
/// Creates 3 points forming a non-degenerate triangle, 3 lines, 3 distance
/// constraints, a horizontal constraint on one side, and a Dragged constraint.
/// Returns (sketch, [d01, d12, d20]).
pub fn arb_constrained_triangle() -> impl Strategy<Value = (Sketch, [f64; 3])> {
    // Generate 3 points that form a non-degenerate triangle
    (
        arb_point_coords(),
        arb_point_coords(),
        arb_point_coords(),
    )
        .prop_filter("triangle must be non-degenerate", |(p0, p1, p2)| {
            // Check area via cross product
            let ax = p1.0 - p0.0;
            let ay = p1.1 - p0.1;
            let bx = p2.0 - p0.0;
            let by = p2.1 - p0.1;
            let area = (ax * by - ay * bx).abs();
            // Minimum side lengths and aspect ratio
            let d01 = ((p1.0 - p0.0).powi(2) + (p1.1 - p0.1).powi(2)).sqrt();
            let d12 = ((p2.0 - p1.0).powi(2) + (p2.1 - p1.1).powi(2)).sqrt();
            let d20 = ((p0.0 - p2.0).powi(2) + (p0.1 - p2.1).powi(2)).sqrt();
            let max_side = d01.max(d12).max(d20);
            let min_side = d01.min(d12).min(d20);
            // Require reasonable area, aspect ratio, and triangle inequality margin
            // to avoid near-degenerate triangles the solver struggles with
            let sum_two_shortest = d01.min(d12).min(d20) + {
                let mut sides = [d01, d12, d20];
                sides.sort_by(|a, b| a.partial_cmp(b).unwrap());
                sides[1]
            };
            area > 50.0 && min_side > 10.0 && max_side / min_side < 5.0
                && sum_two_shortest > max_side * 1.1
        })
        .prop_map(|(p0, p1, p2)| {
            let d01 = ((p1.0 - p0.0).powi(2) + (p1.1 - p0.1).powi(2)).sqrt();
            let d12 = ((p2.0 - p1.0).powi(2) + (p2.1 - p1.1).powi(2)).sqrt();
            let d20 = ((p0.0 - p2.0).powi(2) + (p0.1 - p2.1).powi(2)).sqrt();

            let entities = vec![
                SketchEntity::Point { id: 1, x: p0.0, y: p0.1, construction: false },
                SketchEntity::Point { id: 2, x: p1.0, y: p1.1, construction: false },
                SketchEntity::Point { id: 3, x: p2.0, y: p2.1, construction: false },
                SketchEntity::Line { id: 10, start_id: 1, end_id: 2, construction: false },
                SketchEntity::Line { id: 11, start_id: 2, end_id: 3, construction: false },
                SketchEntity::Line { id: 12, start_id: 3, end_id: 1, construction: false },
            ];
            let constraints = vec![
                SketchConstraint::Distance { entity_a: 1, entity_b: 2, value: d01 },
                SketchConstraint::Distance { entity_a: 2, entity_b: 3, value: d12 },
                SketchConstraint::Distance { entity_a: 3, entity_b: 1, value: d20 },
                SketchConstraint::Horizontal { entity: 10 },
                SketchConstraint::Dragged { point: 1 },
            ];
            (make_sketch(entities, constraints), [d01, d12, d20])
        })
}

/// Generate a perturbed triangle — same constraints but initial positions are offset.
pub fn arb_perturbed_triangle() -> impl Strategy<Value = (Sketch, [f64; 3])> {
    (
        arb_constrained_triangle(),
        proptest::collection::vec(-0.1..0.1f64, 6),
    )
        .prop_map(|((sketch, distances), perturbations)| {
            let mut entities = sketch.entities.clone();
            let mut idx = 0;
            for e in &mut entities {
                if let SketchEntity::Point { x, y, .. } = e {
                    let scale = (x.abs() + y.abs()).max(1.0);
                    *x += perturbations[idx] * scale;
                    *y += perturbations[idx + 1] * scale;
                    idx += 2;
                }
            }
            (
                make_sketch(entities, sketch.constraints.clone()),
                distances,
            )
        })
}

// ── Constrained polygon ─────────────────────────────────────────────────

/// Generate a random convex polygon with N sides (3-8), fully constrained
/// with distance constraints between consecutive vertices, horizontal
/// constraint on first edge, and Dragged on first vertex.
pub fn arb_constrained_polygon(n: usize) -> impl Strategy<Value = (Sketch, Vec<f64>)> {
    assert!((3..=8).contains(&n));
    // Generate N angles for convex polygon vertices on a circle.
    // We start with a regular polygon and add small perturbations to ensure
    // the starting geometry is close to the solution and well-conditioned.
    (
        10.0..100.0f64,
        arb_point_coords(),
        proptest::collection::vec(-0.1..0.1f64, n),
    )
        .prop_map(move |(radius, center, perturbations)| {
            let mut angles: Vec<f64> = (0..n)
                .map(|i| (i as f64 * std::f64::consts::TAU / n as f64) + perturbations[i])
                .collect();
            angles.sort_by(|a, b| a.partial_cmp(b).unwrap());

            // Generate points on circle
            let mut points: Vec<(f64, f64)> = angles
                .iter()
                .map(|a| (center.0 + radius * a.cos(), center.1 + radius * a.sin()))
                .collect();

            // ROTATE points so the first edge (points[0] to points[1]) is horizontal.
            // This ensures the Horizontal { entity: 100 } constraint is satisfied.
            let dx = points[1].0 - points[0].0;
            let dy = points[1].1 - points[0].1;
            let angle = dy.atan2(dx);
            let cos_a = (-angle).cos();
            let sin_a = (-angle).sin();
            let p0 = points[0];
            for p in points.iter_mut() {
                let rx = p.0 - p0.0;
                let ry = p.1 - p0.1;
                p.0 = p0.0 + rx * cos_a - ry * sin_a;
                p.1 = p0.1 + rx * sin_a + ry * cos_a;
            }
            // Ensure exact horizontal for the first edge
            points[1].1 = points[0].1;

            // Compute distances
            let mut distances = Vec::new();
            for i in 0..n {
                let j = (i + 1) % n;
                let dx = points[j].0 - points[i].0;
                let dy = points[j].1 - points[i].1;
                distances.push((dx * dx + dy * dy).sqrt());
            }

            // Build entities
            let mut entities = Vec::new();
            for (i, &(x, y)) in points.iter().enumerate() {
                entities.push(SketchEntity::Point {
                    id: (i + 1) as u32,
                    x,
                    y,
                    construction: false,
                });
            }
            for i in 0..n {
                let j = (i + 1) % n;
                entities.push(SketchEntity::Line {
                    id: (100 + i) as u32,
                    start_id: (i + 1) as u32,
                    end_id: (j + 1) as u32,
                    construction: false,
                });
            }

            // Constraints: all edge distances + angles between consecutive edges
            // + horizontal on first edge + dragged on first vertex
            // DOF: 2N params, need 2N constraints
            //   N distances + (N-2) angles + 1 horizontal + 2 dragged = 2N+1 (over by 1)
            //   Actually: N distances + (N-3) angles + 1 horizontal + 1 dragged = N + (N-3) + 1 + 2 = 2N
            let mut constraints = Vec::new();
            for (i, &d) in distances.iter().enumerate() {
                constraints.push(SketchConstraint::Distance {
                    entity_a: (i + 1) as u32,
                    entity_b: ((i + 1) % n + 1) as u32,
                    value: d,
                });
            }
            // Add angle constraints between consecutive edges to remove internal DOFs
            // We need N-3 angle constraints (N distances + N-3 angles + 1 horizontal + 2 dragged = 2N)
            for i in 0..(n.saturating_sub(3)) {
                let line_a = (100 + i) as u32;
                let line_b = (100 + i + 1) as u32;
                // Compute angle between consecutive edges
                let pi = &points[i];
                let pj = &points[(i + 1) % n];
                let pk = &points[(i + 2) % n];
                let d1 = (pj.0 - pi.0, pj.1 - pi.1);
                let d2 = (pk.0 - pj.0, pk.1 - pj.1);
                let cross = d1.0 * d2.1 - d1.1 * d2.0;
                let dot = d1.0 * d2.0 + d1.1 * d2.1;
                let angle_rad = cross.atan2(dot);
                constraints.push(SketchConstraint::Angle {
                    line_a,
                    line_b,
                    value_degrees: angle_rad.to_degrees(),
                });
            }
            constraints.push(SketchConstraint::Horizontal { entity: 100 });
            constraints.push(SketchConstraint::Dragged { point: 1 });

            (make_sketch(entities, constraints), distances)
        })
}

// ── Simple unconstrained / under-constrained sketches ───────────────────

/// A simple sketch with 2 points and 1 line, only a horizontal constraint.
/// Under-constrained with several DOFs remaining.
pub fn arb_underconstrained_line() -> impl Strategy<Value = Sketch> {
    arb_point_coords().prop_map(|(x, y)| {
        let entities = vec![
            SketchEntity::Point { id: 1, x, y, construction: false },
            SketchEntity::Point { id: 2, x: x + 50.0, y, construction: false },
            SketchEntity::Line { id: 10, start_id: 1, end_id: 2, construction: false },
        ];
        let constraints = vec![SketchConstraint::Horizontal { entity: 10 }];
        make_sketch(entities, constraints)
    })
}

// ── Random constraint spec for Jacobian FD testing ──────────────────────

use sketch_solver::core::constraint::ConstraintImpl;
use sketch_solver::core::types::*;

fn pt(offset: usize) -> PointIdx {
    PointIdx(offset)
}

fn line_idx(s_off: usize, e_off: usize) -> LineIdx {
    LineIdx {
        start: pt(s_off),
        end: pt(e_off),
    }
}

/// A serializable spec for a constraint + params, avoiding the Debug
/// requirement on ConstraintImpl. The actual constraint is built lazily.
#[derive(Debug, Clone)]
pub struct ConstraintSpec {
    pub variant: usize,
    pub raw_params: Vec<f64>,
}

impl ConstraintSpec {
    /// Build the actual ConstraintImpl and matching param vector.
    pub fn build(&self) -> (ConstraintImpl, Vec<f64>) {
        let p = &self.raw_params;
        match self.variant {
            0 => (
                ConstraintImpl::Coincident { p1: pt(0), p2: pt(2) },
                p[0..4].to_vec(),
            ),
            1 => (
                ConstraintImpl::Horizontal { line: line_idx(0, 2) },
                p[0..4].to_vec(),
            ),
            2 => (
                ConstraintImpl::Vertical { line: line_idx(0, 2) },
                p[0..4].to_vec(),
            ),
            3 => (
                ConstraintImpl::SymmetricH { p1: pt(0), p2: pt(2) },
                p[0..4].to_vec(),
            ),
            4 => (
                ConstraintImpl::SymmetricV { p1: pt(0), p2: pt(2) },
                p[0..4].to_vec(),
            ),
            5 => (
                ConstraintImpl::Midpoint { point: pt(0), line: line_idx(2, 4) },
                p[0..6].to_vec(),
            ),
            6 => (
                ConstraintImpl::DistancePP { p1: pt(0), p2: pt(2), d: p[4].abs() + 0.1 },
                p[0..4].to_vec(),
            ),
            7 => (
                ConstraintImpl::EqualLength { l1: line_idx(0, 2), l2: line_idx(4, 6) },
                p[0..8].to_vec(),
            ),
            8 => (
                ConstraintImpl::Parallel { l1: line_idx(0, 2), l2: line_idx(4, 6) },
                p[0..8].to_vec(),
            ),
            9 => (
                ConstraintImpl::Perpendicular { l1: line_idx(0, 2), l2: line_idx(4, 6) },
                p[0..8].to_vec(),
            ),
            10 => {
                let mut params = p[0..8].to_vec();
                if (params[0] - params[2]).abs() < 0.1 && (params[1] - params[3]).abs() < 0.1 {
                    params[2] += 1.0;
                }
                if (params[4] - params[6]).abs() < 0.1 && (params[5] - params[7]).abs() < 0.1 {
                    params[6] += 1.0;
                }
                (
                    ConstraintImpl::Angle {
                        l1: line_idx(0, 2),
                        l2: line_idx(4, 6),
                        value_rad: p[8].rem_euclid(std::f64::consts::PI),
                    },
                    params,
                )
            }
            11 => {
                let mut params = p[0..6].to_vec();
                if (params[2] - params[4]).abs() < 0.1 && (params[3] - params[5]).abs() < 0.1 {
                    params[4] += 1.0;
                }
                (
                    ConstraintImpl::OnLine { point: pt(0), line: line_idx(2, 4) },
                    params,
                )
            }
            12 => {
                let mut params = p[0..5].to_vec();
                params[4] = params[4].abs() + 0.5;
                (
                    ConstraintImpl::OnCircle {
                        point: pt(0),
                        center: pt(2),
                        radius: RadiusDef::Param(RadiusIdx(4)),
                    },
                    params,
                )
            }
            13 => {
                let params = p[0..6].to_vec();
                (
                    ConstraintImpl::OnCircle {
                        point: pt(0),
                        center: pt(2),
                        radius: RadiusDef::Implicit(pt(4)),
                    },
                    params,
                )
            }
            14 => {
                let mut params = p[0..6].to_vec();
                if (params[2] - params[4]).abs() < 0.1 && (params[3] - params[5]).abs() < 0.1 {
                    params[4] += 1.0;
                }
                (
                    ConstraintImpl::DistancePL {
                        point: pt(0),
                        line: line_idx(2, 4),
                        d: p[6].abs() + 0.1,
                    },
                    params,
                )
            }
            15 => {
                let mut params = p[0..7].to_vec();
                if (params[0] - params[2]).abs() < 0.1 && (params[1] - params[3]).abs() < 0.1 {
                    params[2] += 1.0;
                }
                params[6] = params[6].abs() + 0.5;
                (
                    ConstraintImpl::TangentLineCircle {
                        line: line_idx(0, 2),
                        center: pt(4),
                        radius: RadiusDef::Param(RadiusIdx(6)),
                    },
                    params,
                )
            }
            16 => {
                let mut params = p[0..6].to_vec();
                params[4] = params[4].abs() + 0.5;
                params[5] = params[5].abs() + 0.5;
                (
                    ConstraintImpl::TangentArcArc {
                        c1: pt(0),
                        r1: RadiusDef::Param(RadiusIdx(4)),
                        c2: pt(2),
                        r2: RadiusDef::Param(RadiusIdx(5)),
                        internal: false,
                    },
                    params,
                )
            }
            17 => {
                let mut params = p[0..8].to_vec();
                if (params[4] - params[6]).abs() < 0.1 && (params[5] - params[7]).abs() < 0.1 {
                    params[6] += 1.0;
                }
                (
                    ConstraintImpl::SymmetricLine {
                        p1: pt(0),
                        p2: pt(2),
                        line: line_idx(4, 6),
                    },
                    params,
                )
            }
            18 => (
                ConstraintImpl::Ratio {
                    l1: line_idx(0, 2),
                    l2: line_idx(4, 6),
                    k: p[8].abs() + 0.1,
                },
                p[0..8].to_vec(),
            ),
            _ => (
                ConstraintImpl::EqualRadius {
                    r1: RadiusIdx(0),
                    r2: RadiusIdx(1),
                },
                vec![p[0].abs() + 0.5, p[1].abs() + 0.5],
            ),
        }
    }
}

/// Strategy that generates a random constraint spec (variant + params).
pub fn arb_constraint_spec() -> impl Strategy<Value = ConstraintSpec> {
    (0..20usize, proptest::collection::vec(-50.0..50.0f64, 16)).prop_map(
        |(variant, raw_params)| ConstraintSpec { variant, raw_params },
    )
}
