//! Generate the sketch corpus (`specs/agent_mechanical_design.md` §10.4, S4).
//!
//! ```text
//! cargo run -p test-harness --example sketch_corpus_gen
//! cargo run -p test-harness --example sketch_corpus_gen -- --check   # write nothing
//! ```
//!
//! An `examples/` target rather than `src/bin/`, deliberately: only examples and
//! tests may use dev-dependencies, and `sketch-solver` is a dev-dependency here
//! precisely so that `src/sketch_rank.rs` — the independent oracle — cannot
//! reach the implementation it is meant to contradict.
//!
//! **Every expectation below is authored, with its arithmetic in a comment, and
//! this generator REFUSES to write a case whose expectations the solver does not
//! already meet.** A corpus whose numbers come from running the solver is a
//! recording: it detects change, but it blesses whatever the answer happens to
//! be. This session found two defects a recording would have enshrined — a
//! fillet silently extruding as a chamfer, and a point-line dimension mirroring
//! its point. So a disagreement here is a finding to adjudicate, never a number
//! to update. Pin a known-wrong answer with `defect: Some(..)` instead, which
//! keeps it loud.
//!
//! Scale: sketch units are METERS (A14), and the cases are authored at the size
//! of real parts — tens of millimetres — so `TAU_MODEL` and the solver's
//! absolute tolerances mean here what they mean in the app.

use std::collections::{BTreeMap, HashMap};
use std::f64::consts::PI;
use std::path::PathBuf;

use feature_engine::types::{Feature, FeatureTree, Operation};
use file_format::{save_project, ProjectMetadata};
use test_harness::sketch_corpus::{
    answer_from_solved, check, regions_of, ExpectedRegion, SketchCaseMeta,
    SketchOracleExpectations, CORPUS_DIR, GENERATOR_VERSION,
};
use uuid::Uuid;
use waffle_types::{
    Anchor, GeomRef, ResolvePolicy, Role, Selector, Sketch, SketchConstraint, SketchEntity,
    SolveStatus, TopoKind,
};

// ── Authoring helpers ───────────────────────────────────────────────────────

fn pt(id: u32, x: f64, y: f64) -> SketchEntity {
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

fn construction_line(id: u32, start_id: u32, end_id: u32) -> SketchEntity {
    SketchEntity::Line {
        id,
        start_id,
        end_id,
        construction: true,
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

fn pin(point: u32, x: f64, y: f64) -> SketchConstraint {
    SketchConstraint::Pinned { point, x, y }
}

fn horizontal(entity: u32) -> SketchConstraint {
    SketchConstraint::Horizontal { entity }
}

fn vertical(entity: u32) -> SketchConstraint {
    SketchConstraint::Vertical { entity }
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

fn radius(entity: u32, value: f64) -> SketchConstraint {
    SketchConstraint::Radius {
        entity,
        value,
        expression: None,
        reference: false,
    }
}

fn distance(entity_a: u32, entity_b: u32, value: f64) -> SketchConstraint {
    SketchConstraint::Distance {
        entity_a,
        entity_b,
        value,
        expression: None,
        reference: false,
    }
}

fn point_line_distance(point: u32, entity: u32, value: f64) -> SketchConstraint {
    SketchConstraint::PointLineDistance {
        point,
        entity,
        value,
        expression: None,
        reference: false,
    }
}

fn reference_distance(entity_a: u32, entity_b: u32, value: f64) -> SketchConstraint {
    SketchConstraint::Distance {
        entity_a,
        entity_b,
        value,
        expression: None,
        reference: true,
    }
}

/// The four rails of an axis-aligned rectangle on lines 5–8.
fn rect_rails() -> Vec<SketchConstraint> {
    vec![horizontal(5), vertical(6), horizontal(7), vertical(8)]
}

/// Points 1–4 counter-clockwise from the origin, lines 5–8.
fn rect(w: f64, h: f64) -> Vec<SketchEntity> {
    vec![
        pt(1, 0.0, 0.0),
        pt(2, w, 0.0),
        pt(3, w, h),
        pt(4, 0.0, h),
        line(5, 1, 2),
        line(6, 2, 3),
        line(7, 3, 4),
        line(8, 4, 1),
    ]
}

fn sketch_on_xy(entities: Vec<SketchEntity>, constraints: Vec<SketchConstraint>) -> Sketch {
    Sketch {
        id: Uuid::nil(),
        // A placeholder datum ref, as `sketch_create` mints for a plane given
        // by origin + normal; the real plane travels in the fields below.
        plane: GeomRef {
            kind: TopoKind::Face,
            anchor: Anchor::Datum {
                datum_id: Uuid::nil(),
            },
            selector: Selector::Role {
                role: Role::ProfileFace,
                index: 0,
            },
            policy: ResolvePolicy::BestEffort,
            scope: None,
        },
        plane_origin: [0.0, 0.0, 0.0],
        plane_normal: [0.0, 0.0, 1.0],
        // Pinned, so a case's sketch coordinates are world coordinates and a
        // reader never has to reproduce the engine's basis derivation.
        plane_x_axis: Some([1.0, 0.0, 0.0]),
        entities,
        constraints,
        solve_status: SolveStatus::Unsolved,
        solved_positions: HashMap::new(),
        solved_profiles: Vec::new(),
        projected: Vec::new(),
        plane_face: None,
    }
}

/// One case as this generator declares it.
struct CaseSpec {
    id: &'static str,
    description: &'static str,
    exercises: &'static [&'static str],
    entities: Vec<SketchEntity>,
    constraints: Vec<SketchConstraint>,
    expect: SketchOracleExpectations,
}

fn expectations(
    status: &str,
    dof: u32,
    params: u32,
    rows: u32,
    rank: u32,
    regions: Vec<ExpectedRegion>,
) -> SketchOracleExpectations {
    SketchOracleExpectations {
        status: status.to_string(),
        dof,
        params,
        rows,
        rank,
        conflicts: Vec::new(),
        redundant: Vec::new(),
        regions,
        positions: BTreeMap::new(),
        positions_tol: 1e-12,
        radii: BTreeMap::new(),
        radii_tol: 1e-12,
        // 1e-7 relative clears both floors under a measured area — the
        // slicer's 2^-29 grid snap and LM's convergence tail — while staying
        // thousands of times tighter than any geometric defect worth catching.
        area_rel_tol: 1e-7,
        defect: None,
    }
}

/// An extrudable region: one whole loop, which a feature can consume.
fn region(entity_ids: &[u32], area: f64) -> ExpectedRegion {
    ExpectedRegion {
        entity_ids: entity_ids.to_vec(),
        extrudable: true,
        area,
    }
}

/// A region that exists but is NOT one whole loop, so it carries no
/// `profile_entity_ids` and no feature can be built on it — the annulus of a
/// plate with a hole.
fn sub_region(entity_ids: &[u32], area: f64) -> ExpectedRegion {
    ExpectedRegion {
        entity_ids: entity_ids.to_vec(),
        extrudable: false,
        area,
    }
}

fn positions(pairs: &[(u32, [f64; 2])]) -> BTreeMap<u32, [f64; 2]> {
    pairs.iter().copied().collect()
}

fn radii(pairs: &[(u32, f64)]) -> BTreeMap<u32, f64> {
    pairs.iter().copied().collect()
}

/// How far BELOW πr² a full circle's region area reads, as a fraction of it.
///
/// `compute_regions` tessellates a circle into `ceil(π / acos(1 − τ))` chords
/// (`regions::circle_segment_count`, τ = `DEFAULT_CHORD_TOLERANCE` = 1e-3), and
/// the inscribed regular n-gon has area `(n/2)r²·sin(2π/n)`. So the deficit is
///
///     1 − n·sin(2π/n) / (2π)
///
/// which is 1.3e-3 at τ = 1e-3 (n = 71) — three orders of magnitude above the
/// grid and LM floors, and the reason a curved loop cannot carry a tight area
/// tolerance while a RADIUS can. Derived here rather than measured, so a case's
/// band is arithmetic too.
fn circle_chord_deficit() -> f64 {
    let tau = waffle_types::regions::DEFAULT_CHORD_TOLERANCE;
    let n = (PI / (1.0 - tau).acos()).ceil().clamp(8.0, 512.0);
    1.0 - n * (2.0 * PI / n).sin() / (2.0 * PI)
}

// ── The corpus ──────────────────────────────────────────────────────────────

/// 60 × 40 mm, the plate every rectangle case is a variation of.
const W: f64 = 0.060;
const H: f64 = 0.040;

fn corpus() -> Vec<CaseSpec> {
    let mut cases = Vec::new();

    // S0001 — nothing constrained at all. Four points, two parameters each,
    // nothing fixing any of them: 8 parameters, 0 rows, rank 0, dof 8. The
    // positions are the authored ones because no driving constraint runs, which
    // makes this the one case whose area is exact apart from the slicer's grid.
    cases.push(CaseSpec {
        id: "S0001",
        description: "A free 60 x 40 mm rectangle: four points, four lines, no constraints.",
        exercises: &["Point", "Line", "under-constrained"],
        entities: rect(W, H),
        constraints: vec![],
        expect: SketchOracleExpectations {
            positions: positions(&[(1, [0.0, 0.0]), (2, [W, 0.0]), (3, [W, H]), (4, [0.0, H])]),
            ..expectations(
                "UnderConstrained",
                8,
                8,
                0,
                0,
                vec![region(&[5, 6, 7, 8], W * H)],
            )
        },
    });

    // S0002 — the four rails and nothing else. Each H/V constraint is one row
    // and each is independent, so rank 4 and dof 8 − 4 = 4: the rectangle can
    // still translate in x and y and change width and height. (A fifth rail
    // would be redundant, which is S0005.)
    cases.push(CaseSpec {
        id: "S0002",
        description: "A railed rectangle: H/V on all four sides, nothing pinned or dimensioned.",
        exercises: &["Horizontal", "Vertical", "under-constrained"],
        entities: rect(W, H),
        constraints: rect_rails(),
        expect: expectations(
            "UnderConstrained",
            4,
            8,
            4,
            4,
            vec![region(&[5, 6, 7, 8], W * H)],
        ),
    });

    // S0003 — fully constrained: rails (4 rows) + a pin on the origin corner
    // (2 rows) + two dimensions (1 row each) = 8 rows, all independent, so
    // rank 8 = params and dof 0. The corners are then arithmetic.
    cases.push(CaseSpec {
        id: "S0003",
        description:
            "A fully constrained 60 x 40 mm plate: rails, a pinned corner, two dimensions.",
        exercises: &[
            "Horizontal",
            "Vertical",
            "Pinned",
            "HDistance",
            "VDistance",
            "fully-constrained",
        ],
        entities: rect(W, H),
        constraints: {
            let mut c = rect_rails();
            c.push(pin(1, 0.0, 0.0));
            c.push(hdist(1, 2, W));
            c.push(vdist(1, 4, H));
            c
        },
        expect: SketchOracleExpectations {
            positions: positions(&[(1, [0.0, 0.0]), (2, [W, 0.0]), (3, [W, H]), (4, [0.0, H])]),
            // LM stops at a step criterion, not the exact root, so a solved
            // coordinate carries a convergence tail; 1e-9 m is a nanometre on a
            // 60 mm plate.
            positions_tol: 1e-9,
            ..expectations(
                "FullyConstrained",
                0,
                8,
                8,
                8,
                vec![region(&[5, 6, 7, 8], W * H)],
            )
        },
    });

    // S0004 — a circle: its centre is 2 parameters and its radius is a third.
    // A pin (2 rows) and a Radius (1 row) leave nothing: dof 0. The area is
    // πr², but measured on the slicer's grid THROUGH a chord polygon, so this
    // case carries the loosest area tolerance in the corpus and says why.
    cases.push(CaseSpec {
        id: "S0004",
        description: "A pinned 10 mm circle with a radius dimension: the radius parameter.",
        exercises: &["Circle", "Radius", "Pinned", "fully-constrained", "curved"],
        entities: vec![pt(1, 0.0, 0.0), circle(2, 1, 0.010)],
        constraints: vec![pin(1, 0.0, 0.0), radius(2, 0.010)],
        expect: SketchOracleExpectations {
            positions: positions(&[(1, [0.0, 0.0])]),
            positions_tol: 1e-12,
            // The radius is the thing this case is really about, and it IS
            // exact: a `Radius` constraint solves the radius parameter
            // directly, so pin it to the picometre and let the area be loose.
            radii: radii(&[(2, 0.010)]),
            radii_tol: 1e-12,
            // The area is the 71-chord polygon's, 1.3e-3 below πr². The band is
            // twice the derived deficit: it admits the chord polygon and
            // rejects anything else, including an exact πr² (which would mean
            // the region stopped being tessellated) by only a factor of two —
            // so the real precision claim lives in `radii` above.
            area_rel_tol: 2.0 * circle_chord_deficit(),
            ..expectations(
                "FullyConstrained",
                0,
                3,
                3,
                3,
                vec![region(&[2], PI * 0.010 * 0.010)],
            )
        },
    });

    // S0005 — redundancy. S0003 plus a SECOND Horizontal on line 5. The system
    // is still satisfiable, so the sketch stays green and dof stays 0, but
    // rank 8 < rows 9 and the later duplicate is the dependent one.
    //
    // Its index is 7. S0003 declares SEVEN constraints — four rails, one pin,
    // two dimensions — occupying 0–6 in the order pushed below, so the
    // duplicate added last is 7. (It is nine ROWS, not nine constraints: the
    // pin contributes two. Counting rows as indices is how the first draft of
    // this case got it wrong, and the generator's gate is what caught it.)
    cases.push(CaseSpec {
        id: "S0005",
        description:
            "S0003 plus a duplicate Horizontal: satisfied, over-determined, not in conflict.",
        exercises: &["Horizontal", "redundant", "fully-constrained"],
        entities: rect(W, H),
        constraints: {
            let mut c = rect_rails();
            c.push(pin(1, 0.0, 0.0));
            c.push(hdist(1, 2, W));
            c.push(vdist(1, 4, H));
            c.push(horizontal(5)); // index 7 — the duplicate
            c
        },
        expect: SketchOracleExpectations {
            redundant: vec![7],
            ..expectations(
                "FullyConstrained",
                0,
                8,
                9,
                8,
                vec![region(&[5, 6, 7, 8], W * H)],
            )
        },
    });

    // S0006 — a contradiction. Both endpoints of line 5 pinned (4 rows) and a
    // Distance of 80 mm over a 60 mm span (1 row): 5 rows, rank 5, but the
    // system has no solution. Both pins and the dimension are offenders, since
    // with every parameter pinned least squares splits the error across all
    // three — which is why `conflicts` is compared as a SET here.
    cases.push(CaseSpec {
        id: "S0006",
        description: "A 60 mm line with both ends pinned and an 80 mm length dimension.",
        exercises: &["Pinned", "Distance", "over-constrained", "conflicts"],
        entities: vec![pt(1, 0.0, 0.0), pt(2, W, 0.0), line(5, 1, 2)],
        constraints: vec![pin(1, 0.0, 0.0), pin(2, W, 0.0), distance(1, 2, 0.080)],
        expect: SketchOracleExpectations {
            conflicts: vec![0, 1, 2],
            ..expectations("OverConstrained", 0, 4, 5, 4, vec![])
        },
    });

    // S0007 — the index space. A REFERENCE dimension is declared FIRST, so
    // every index the report carries about a driving constraint is one higher
    // than it would be in the filtered set. A reference dimension drives
    // nothing: rows counts only the four rails, and dof is 8 − 4 = 4, exactly
    // S0002's. Before S3 `sketch_create` filtered references out before the
    // solve and reported indices in the filtered space, which made a conflict
    // name the wrong constraint.
    cases.push(CaseSpec {
        id: "S0007",
        description: "A railed rectangle behind a leading REFERENCE dimension: the index space.",
        exercises: &["Distance", "reference-dimension", "index-space"],
        entities: rect(W, H),
        constraints: {
            let mut c = vec![reference_distance(1, 2, W)]; // index 0, drives nothing
            c.extend(rect_rails()); // indices 1–4
            c
        },
        expect: expectations(
            "UnderConstrained",
            4,
            8,
            4,
            4,
            vec![region(&[5, 6, 7, 8], W * H)],
        ),
    });

    // S0008 — an ARC in a profile, which is the shape that caught the
    // chamfer defect: a 60 × 40 plate with its top-right corner replaced by a
    // 4 mm quarter round. The arc's centre is 4 mm in from each of the two
    // edges it is tangent to, so the loop is
    //   (0,0) → (60,0) → (60,36) → arc → (56,40) → (0,40) → close
    // and the area is the rectangle less the corner square it gives up plus
    // the quarter disc it gains back: W·H − r² + πr²/4 = W·H − r²(1 − π/4).
    //
    // Unconstrained on purpose: the geometry is authored exactly, so this case
    // is about the PROFILE (its arc_segments, its area) and not about the
    // solver. Points: 1–4 the corners, 9 the arc centre, 10/11 its ends.
    {
        let r = 0.004;
        let cx = W - r;
        let cy = H - r;
        let entities = vec![
            pt(1, 0.0, 0.0),
            pt(2, W, 0.0),
            pt(3, W, cy), // where the right edge meets the arc
            pt(4, cx, H), // where the top edge meets the arc
            pt(5, 0.0, H),
            pt(9, cx, cy), // the arc centre
            line(10, 1, 2),
            line(11, 2, 3),
            arc(12, 9, 3, 4),
            line(13, 4, 5),
            line(14, 5, 1),
        ];
        cases.push(CaseSpec {
            id: "S0008",
            description: "A 60 x 40 mm plate with one 4 mm rounded corner: an arc in the profile.",
            exercises: &["Arc", "curved", "under-constrained", "profile"],
            entities,
            constraints: vec![],
            expect: SketchOracleExpectations {
                // The arc's chords put the measured area BELOW the analytic
                // one by a few parts per million (measured 6.6 ppm through the
                // agent link on this exact shape). The solid extruded from the
                // same profile is EXACT, because the kernel builds a true
                // cylindrical face from the arc record — the two numbers about
                // one corner have different characters, and that is the point
                // of pinning this case.
                area_rel_tol: 1e-5,
                ..expectations(
                    "UnderConstrained",
                    // Six points at two parameters each. The arc contributes
                    // no radius parameter of its own: its radius IS the
                    // centre→start distance.
                    12,
                    12,
                    0,
                    0,
                    vec![region(
                        &[10, 11, 12, 13, 14],
                        W * H - r * r * (1.0 - PI / 4.0),
                    )],
                )
            },
        });
    }

    // S0009 — the point-line dimension, unsigned since 2026-10-08. Point 3 sits
    // 7 mm above a 10 mm line whose ends are pinned, and the dimension asks for
    // the 7 mm it already measures: the residual is |70/10| − 7 = 0, so nothing
    // moves. Under the old SIGNED residual the point was mirrored to y = −7 and
    // the solve called it satisfied. dof: 6 parameters, 4 pinned rows + 1
    // dimension row, all independent ⇒ rank 5, dof 1 (the point can still slide
    // ALONG the line).
    cases.push(CaseSpec {
        id: "S0009",
        description:
            "A point 7 mm above a pinned line, dimensioned to the 7 mm it already measures.",
        exercises: &["PointLineDistance", "Pinned", "under-constrained"],
        entities: vec![
            pt(1, 0.0, 0.0),
            pt(2, 0.010, 0.0),
            pt(3, 0.005, 0.007),
            line(5, 1, 2),
        ],
        constraints: vec![
            pin(1, 0.0, 0.0),
            pin(2, 0.010, 0.0),
            point_line_distance(3, 5, 0.007),
        ],
        expect: SketchOracleExpectations {
            // The whole point: the point does not move, and in particular does
            // not appear at y = −0.007.
            positions: positions(&[(3, [0.005, 0.007]), (1, [0.0, 0.0]), (2, [0.010, 0.0])]),
            positions_tol: 1e-12,
            ..expectations("UnderConstrained", 1, 6, 5, 5, vec![])
        },
    });

    // S0010 — two regions from one sketch: a plate with a hole. The outer loop
    // is the rectangle, the inner one the circle, and `compute_regions` reports
    // the ANNULUS (outer minus hole) for the rectangle's loop plus the disc for
    // the circle's. Authored geometry, no constraints, so the only floors are
    // the grid and the circle's chords.
    {
        let r = 0.008;
        let entities = vec![
            pt(1, 0.0, 0.0),
            pt(2, W, 0.0),
            pt(3, W, H),
            pt(4, 0.0, H),
            line(5, 1, 2),
            line(6, 2, 3),
            line(7, 3, 4),
            line(8, 4, 1),
            pt(9, W / 2.0, H / 2.0),
            circle(10, 9, r),
        ];
        cases.push(CaseSpec {
            id: "S0010",
            description: "A 60 x 40 mm plate with a centred 16 mm hole: two regions.",
            exercises: &["Circle", "Line", "two-regions", "curved", "profile"],
            entities,
            constraints: vec![],
            expect: SketchOracleExpectations {
                radii: radii(&[(10, r)]),
                // Both loops are off by the HOLE's chord deficit, in opposite
                // directions: the disc reads low by 1.3e-3 of itself, and the
                // annulus reads HIGH by the same absolute amount, because a
                // smaller hole is subtracted. As a fraction of the annulus
                // that is deficit · πr²/(W·H − πr²) = 1.3e-3 · 2.01e-4/2.20e-3
                // ≈ 1.2e-4, so one band has to cover the larger of the two
                // relative errors. Twice that is the band.
                area_rel_tol: 2.0 * circle_chord_deficit(),
                ..expectations(
                    "UnderConstrained",
                    // Five points (four corners + the hole centre) at two
                    // parameters each, plus the circle's radius.
                    11,
                    11,
                    0,
                    0,
                    vec![
                        // The annulus: boundary = the outline AND the hole, and
                        // NOT extrudable — it is not one whole loop, so an
                        // Extrude cannot be built on it. The disc can.
                        sub_region(&[5, 6, 7, 8, 10], W * H - PI * r * r),
                        region(&[10], PI * r * r),
                    ],
                )
            },
        });
    }

    // S0011 — symmetry about a construction line. Points 1 and 2 are mirrored
    // about the vertical construction line 10, whose own ends are pinned.
    // `SymmetricV` (the solver's naming is inverted from intuition — see
    // `specs/sketch_solver_rewrite.md`) is 2 rows. 8 parameters (four points),
    // 4 pinned rows + 2 symmetry rows = 6 rows, rank 6, dof 2: the pair can
    // still move together along the axis and apart from it.
    cases.push(CaseSpec {
        id: "S0011",
        description: "Two points held symmetric about a pinned vertical construction line.",
        exercises: &["Symmetric", "construction", "Pinned", "under-constrained"],
        entities: vec![
            pt(1, -0.020, 0.010),
            pt(2, 0.020, 0.010),
            pt(3, 0.0, 0.0),
            pt(4, 0.0, 0.030),
            construction_line(10, 3, 4),
        ],
        constraints: vec![
            pin(3, 0.0, 0.0),
            pin(4, 0.0, 0.030),
            // `Symmetric` is the LINE-based one (two rows: the pair's midpoint
            // lies on the line, and the pair is perpendicular to it).
            // `SymmetricH`/`SymmetricV` take no line — they mirror about a
            // world axis, and their naming is inverted from intuition, which
            // `specs/sketch_solver_rewrite.md` records as one of libslvs's
            // quirks the clean-room solver kept.
            SketchConstraint::Symmetric {
                entity_a: 1,
                entity_b: 2,
                symmetry_line: 10,
            },
        ],
        expect: SketchOracleExpectations {
            positions: positions(&[(3, [0.0, 0.0]), (4, [0.0, 0.030])]),
            positions_tol: 1e-9,
            // A construction line bounds no region, and two loose points make
            // no loop: no regions at all, which is itself worth pinning.
            ..expectations("UnderConstrained", 2, 8, 6, 6, vec![])
        },
    });

    // S0012/S0013 — the same fully constrained plate at 1e-3 and 1e3 times
    // S0003's size. Structure is scale-free: params, rows, rank and dof must be
    // identical three times over. The area scales as s².
    for (id, scale, note) in [
        (
            "S0012",
            1.0e-3_f64,
            "S0003 at 1/1000 scale (60 x 40 um): structure is scale-free.",
        ),
        (
            "S0013",
            1.0e3_f64,
            "S0003 at 1000x scale (60 x 40 m): structure is scale-free.",
        ),
    ] {
        let (w, h) = (W * scale, H * scale);
        cases.push(CaseSpec {
            id,
            description: note,
            exercises: &["scale", "fully-constrained", "HDistance", "VDistance"],
            entities: rect(w, h),
            constraints: {
                let mut c = rect_rails();
                c.push(pin(1, 0.0, 0.0));
                c.push(hdist(1, 2, w));
                c.push(vdist(1, 4, h));
                c
            },
            expect: SketchOracleExpectations {
                positions: positions(&[(1, [0.0, 0.0]), (3, [w, h])]),
                // Scaled with the geometry: an absolute nanometre is
                // meaningless at either end of a six-order range.
                positions_tol: 1e-9 * scale.max(1.0),
                ..expectations(
                    "FullyConstrained",
                    0,
                    8,
                    8,
                    8,
                    vec![region(&[5, 6, 7, 8], w * h)],
                )
            },
        });
    }

    cases
}

// ── Writing ─────────────────────────────────────────────────────────────────

fn main() {
    let check_only = std::env::args().any(|a| a == "--check");
    let root = test_harness::sketch_corpus::repo_root();
    let dir: PathBuf = root.join(CORPUS_DIR);
    if !check_only {
        std::fs::create_dir_all(&dir).expect("create the corpus directory");
    }

    let mut written = 0usize;

    for spec in corpus() {
        let sketch = sketch_on_xy(spec.entities.clone(), spec.constraints.clone());

        // THE GATE. The authored expectations are checked against the solver
        // before anything is written, so a case can only enter the corpus with
        // an answer someone derived. A failure here is a finding — either the
        // arithmetic in the comment above the case is wrong, or the solver is —
        // and the way to record the second is `defect: Some(..)`, not a new
        // number.
        let solved = sketch_solver::solve_sketch(&sketch);
        let regions = regions_of(&sketch, &solved.positions);
        let answer = answer_from_solved(&solved, &regions);
        let mismatches = check(&spec.expect, &answer);
        if !mismatches.is_empty() && spec.expect.defect.is_none() {
            eprintln!(
                "\n{}: the authored answer does NOT match the solver:",
                spec.id
            );
            for m in &mismatches {
                eprintln!("    {m}");
            }
            eprintln!(
                "  {} — adjudicate before writing: fix the arithmetic, or record a solver\n  \
                 defect with `defect: Some(..)`. Do NOT copy the solver's number into the\n  \
                 expectation.",
                spec.description
            );
            std::process::exit(1);
        }

        // `free.len() == dof` is the S2 contract and costs nothing to check
        // while the solve is in hand.
        assert_eq!(
            solved.report.free.len() as u32,
            solved.report.dof,
            "{}: free.len() must equal dof",
            spec.id
        );

        // The case document carries the SOLVED sketch, exactly as the app would
        // save it: positions written back into the entities, derived data from
        // `build_finish_profiles` (never `recompute_derived`, which drops
        // `arc_segments` — the chamfer defect of 2026-10-08).
        let mut stored = sketch.clone();
        let failed = matches!(
            solved.status,
            SolveStatus::OverConstrained { .. } | SolveStatus::SolveFailed { .. }
        );
        if !failed {
            for e in &mut stored.entities {
                match e {
                    SketchEntity::Point { id, x, y, .. } => {
                        if let Some((sx, sy)) = solved.positions.get(id) {
                            *x = *sx;
                            *y = *sy;
                        }
                    }
                    SketchEntity::Circle { id, radius, .. } => {
                        if let Some(r) = solved.radii.get(id) {
                            *radius = *r;
                        }
                    }
                    _ => {}
                }
            }
        }
        stored.solve_status = solved.status.clone();
        let positions = if failed {
            stored
                .entities
                .iter()
                .filter_map(|e| match e {
                    SketchEntity::Point { id, x, y, .. } => Some((*id, (*x, *y))),
                    _ => None,
                })
                .collect()
        } else {
            solved.positions.clone()
        };
        let extracted = waffle_types::extract_profiles(&stored.entities, &positions);
        let finished =
            waffle_types::profiles::build_finish_profiles(&extracted, &stored.entities, &positions);
        stored.solved_positions = finished.solved_positions;
        stored.solved_profiles = finished.profiles;
        // A stable id per case, so regenerating does not churn the files.
        stored.id = stable_uuid(spec.id, "sketch");

        let tree = FeatureTree {
            features: vec![Feature {
                id: stable_uuid(spec.id, "feature"),
                name: "Sketch".to_string(),
                operation: Operation::Sketch { sketch: stored },
                suppressed: false,
                references: Vec::new(),
            }],
            ..Default::default()
        };
        let metadata = ProjectMetadata::new(spec.id);
        let waffle_json = stabilize(&save_project(&tree, &metadata), spec.id);

        let meta = SketchCaseMeta {
            id: spec.id.to_string(),
            description: spec.description.to_string(),
            exercises: spec.exercises.iter().map(|s| s.to_string()).collect(),
            expectations: spec.expect.clone(),
            generator_version: GENERATOR_VERSION,
        };

        if check_only {
            println!("{}: OK (not written) — {}", spec.id, spec.description);
        } else {
            std::fs::write(dir.join(format!("{}.waffle", spec.id)), &waffle_json)
                .expect("write the case document");
            std::fs::write(
                dir.join(format!("{}.meta.json", spec.id)),
                serde_json::to_string_pretty(&meta).expect("meta serializes") + "\n",
            )
            .expect("write the case metadata");
            println!("{}: written — {}", spec.id, spec.description);
        }
        written += 1;
    }

    // No manifest: the dev API's `/api/sketch-cases` derives its listing from
    // what is on disk.
    //
    // §10.4 said to use "the empty second manifest slot"
    // (`app/tests/cases/manifest.json`), and the first version of this
    // generator did. That slot belongs to a CRUD endpoint the Tests browser
    // panel owns, and `app/tests/gui/test-case-browser.spec.js` exercises its
    // DELETE: running the GUI suite unlinked all thirteen committed `.waffle`
    // files and emptied the manifest. A committed fixture cannot live behind a
    // mutable endpoint that a test clears, so the corpus got a read-only
    // endpoint of its own beside `/api/assay-cases`.
    if check_only {
        println!("\n{written} cases check out against the solver");
    } else {
        println!("\nwrote {written} cases to {}", dir.display());
    }
}

/// Make a saved document byte-reproducible.
///
/// `save_project` is deliberately not reproducible — it mints a document id and
/// a tab id and stamps `Utc::now()` — so regenerating the corpus would rewrite
/// every file with three fresh values and nothing else, which is the churn
/// `assay_gen` warns about and the reason its legacy cases are never
/// regenerated. A committed fixture has no use for a wall clock, so this
/// replaces the three with values derived from the case id.
///
/// Re-serializing through `serde_json::Value` also SORTS every object key,
/// which matters for one field in particular: `solved_positions` serializes a
/// `HashMap`, so its key order varies between runs of the same input. (Worth
/// knowing beyond here — it means two saves of one document are not
/// byte-identical in general, only semantically equal.)
fn stabilize(waffle_json: &str, case_id: &str) -> String {
    let mut doc: serde_json::Value =
        serde_json::from_str(waffle_json).expect("the document we just saved parses");
    let stamp = "2026-10-08T00:00:00.000Z";
    let doc_id = stable_uuid(case_id, "document").to_string();
    let tab_id = stable_uuid(case_id, "tab").to_string();
    doc["document"]["id"] = serde_json::json!(doc_id);
    doc["document"]["created"] = serde_json::json!(stamp);
    doc["document"]["modified"] = serde_json::json!(stamp);
    doc["tabs"][0]["id"] = serde_json::json!(tab_id);
    doc["active_tab"] = serde_json::json!(tab_id);
    serde_json::to_string_pretty(&doc).expect("it re-serializes") + "\n"
}

/// A deterministic UUID from the case id and a role, so regenerating the corpus
/// produces byte-identical files instead of churning every one with fresh v4
/// ids (the trap `assay_gen` documents).
fn stable_uuid(case_id: &str, role: &str) -> Uuid {
    // FNV-1a over "<case>/<role>", spread across the 16 bytes. Not a v5 UUID —
    // no namespace semantics are wanted, only determinism.
    let seed = format!("{case_id}/{role}");
    let mut bytes = [0u8; 16];
    let mut hash: u64 = 0xcbf2_9ce4_8422_2325;
    for (i, b) in seed.as_bytes().iter().cycle().take(64).enumerate() {
        hash ^= u64::from(*b) ^ (i as u64);
        hash = hash.wrapping_mul(0x0000_0100_0000_01b3);
        bytes[i % 16] ^= (hash >> ((i % 8) * 8)) as u8;
    }
    Uuid::from_bytes(bytes)
}
