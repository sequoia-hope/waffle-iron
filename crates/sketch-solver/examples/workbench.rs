//! Programmatic Sketch Workbench — builds complex engineering sketches step by step.
//!
//! Each scenario progressively adds entities and constraints, snapshotting at each step
//! to show the solver working. Produces a folder of PNGs and a REPORT.md per scenario.
//!
//! Usage:
//!   cargo run -p sketch-solver --example workbench --features render

use sketch_solver::{
    render_sketch_png, render_sketch_svg, solve_sketch, ArcId, CircleId, EntityId, LineId, PointId,
    Sketch, SketchConstraint, SketchEntity, SolveStatus,
};
use std::collections::HashMap;
use std::path::PathBuf;
use uuid::Uuid;

fn dummy_geom_ref() -> sketch_solver::GeomRef {
    sketch_solver::GeomRef {
        kind: sketch_solver::TopoKind::Face,
        anchor: sketch_solver::Anchor::Datum {
            datum_id: Uuid::nil(),
        },
        selector: sketch_solver::Selector::Role {
            role: sketch_solver::Role::ProfileFace,
            index: 0,
        },
        policy: sketch_solver::ResolvePolicy::Strict,
    }
}

// ─── SketchWorkbench ─────────────────────────────────────────────────────────

struct SketchWorkbench {
    #[allow(dead_code)]
    name: String,
    #[allow(dead_code)]
    description: String,
    entities: Vec<SketchEntity>,
    constraints: Vec<SketchConstraint>,
    step: usize,
    output_dir: PathBuf,
    report_lines: Vec<String>,
}

impl SketchWorkbench {
    fn new(name: &str, description: &str) -> Self {
        let output_dir = PathBuf::from(env!("CARGO_MANIFEST_DIR"))
            .join("output")
            .join("workbench")
            .join(name);
        std::fs::create_dir_all(&output_dir).unwrap();

        let mut report_lines = Vec::new();
        report_lines.push(format!("# {}", description));
        report_lines.push(String::new());

        eprintln!("\n=== {} ===", description);

        SketchWorkbench {
            name: name.to_string(),
            description: description.to_string(),
            entities: Vec::new(),
            constraints: Vec::new(),
            step: 0,
            output_dir,
            report_lines,
        }
    }

    fn add_entity(&mut self, entity: SketchEntity) -> &mut Self {
        self.entities.push(entity);
        self
    }

    fn add_entities(&mut self, entities: Vec<SketchEntity>) -> &mut Self {
        self.entities.extend(entities);
        self
    }

    fn add_constraint(&mut self, constraint: SketchConstraint) -> &mut Self {
        self.constraints.push(constraint);
        self
    }

    fn add_constraints(&mut self, constraints: Vec<SketchConstraint>) -> &mut Self {
        self.constraints.extend(constraints);
        self
    }

    fn snapshot(&mut self, label: &str, description: &str) -> &mut Self {
        let sketch = Sketch {
            id: Uuid::nil(),
            plane: dummy_geom_ref(),
            plane_origin: [0.0, 0.0, 0.0],
            plane_normal: [0.0, 0.0, 1.0],
            entities: self.entities.clone(),
            constraints: self.constraints.clone(),
            solve_status: SolveStatus::UnderConstrained { dof: 99 },
            solved_positions: HashMap::new(),
            solved_profiles: Vec::new(),
        };

        let solved = solve_sketch(&sketch).expect("workbench solve");

        let file_stem = format!("step_{:02}_{}", self.step, label);
        let svg = render_sketch_svg(&sketch, &solved);
        let svg_path = self.output_dir.join(format!("{file_stem}.svg"));
        std::fs::write(&svg_path, &svg).unwrap();

        let png = render_sketch_png(&svg, 800, 600);
        let png_path = self.output_dir.join(format!("{file_stem}.png"));
        std::fs::write(&png_path, &png).unwrap();

        // Status summary
        let status_str = format_status(&solved.status);
        let entity_summary = count_entities(&self.entities);
        let constraint_count = self.constraints.len();

        eprintln!(
            "  Step {}: {} — {} | {} | {} constraints",
            self.step, label, status_str, entity_summary, constraint_count
        );

        // Report section
        self.report_lines
            .push(format!("## Step {}: {}", self.step, titlecase(label)));
        self.report_lines.push(format!(
            "**Status**: {} | **Entities**: {} | **Constraints**: {}",
            status_str, entity_summary, constraint_count
        ));
        self.report_lines.push(format!("> {}", description));
        self.report_lines.push(String::new());

        // Constraint list for this step
        if !self.constraints.is_empty() {
            self.report_lines
                .push("**Active constraints**:".to_string());
            for c in &self.constraints {
                self.report_lines
                    .push(format!("- {}", format_constraint(c)));
            }
            self.report_lines.push(String::new());
        }

        // Point positions table
        if !solved.positions.is_empty() {
            self.report_lines.push("| Point | X | Y |".to_string());
            self.report_lines
                .push("|-------|-------|-------|".to_string());
            let mut points: Vec<_> = solved.positions.iter().collect();
            points.sort_by_key(|(id, _)| id.0);
            for (id, (x, y)) in points {
                self.report_lines
                    .push(format!("| P{} | {:.2} | {:.2} |", id.0, x, y));
            }
            self.report_lines.push(String::new());
        }

        // Profile info
        if !solved.profiles.is_empty() {
            self.report_lines.push(format!(
                "**Profiles detected**: {} closed profile(s)",
                solved.profiles.len()
            ));
            self.report_lines.push(String::new());
        }

        self.report_lines
            .push(format!("![Step {}]({file_stem}.png)", self.step));
        self.report_lines.push(String::new());
        self.report_lines.push("---".to_string());
        self.report_lines.push(String::new());

        self.step += 1;
        self
    }

    fn finish(&self) {
        let report_path = self.output_dir.join("REPORT.md");
        std::fs::write(&report_path, self.report_lines.join("\n")).unwrap();
        eprintln!(
            "  Wrote REPORT.md ({} steps, {} lines)",
            self.step,
            self.report_lines.len()
        );
    }
}

// ─── Helpers ─────────────────────────────────────────────────────────────────

fn format_status(status: &SolveStatus) -> String {
    match status {
        SolveStatus::FullyConstrained => "Fully constrained (0 DOF)".to_string(),
        SolveStatus::UnderConstrained { dof } => format!("Under-constrained ({dof} DOF)"),
        SolveStatus::OverConstrained { conflicts } => {
            format!("Over-constrained ({} conflicts)", conflicts.len())
        }
        SolveStatus::SolveFailed { reason } => format!("Solve failed: {reason}"),
    }
}

fn count_entities(entities: &[SketchEntity]) -> String {
    let mut points = 0u32;
    let mut lines = 0u32;
    let mut circles = 0u32;
    let mut arcs = 0u32;
    for e in entities {
        match e {
            SketchEntity::Point { .. } => points += 1,
            SketchEntity::Line { .. } => lines += 1,
            SketchEntity::Circle { .. } => circles += 1,
            SketchEntity::Arc { .. } => arcs += 1,
            _ => {}
        }
    }
    let mut parts = Vec::new();
    if points > 0 {
        parts.push(format!("{points} pt"));
    }
    if lines > 0 {
        parts.push(format!("{lines} ln"));
    }
    if circles > 0 {
        parts.push(format!("{circles} cir"));
    }
    if arcs > 0 {
        parts.push(format!("{arcs} arc"));
    }
    parts.join(", ")
}

fn format_constraint(c: &SketchConstraint) -> String {
    match c {
        SketchConstraint::Horizontal { entity } => format!("Horizontal(E{})", entity.0),
        SketchConstraint::Vertical { entity } => format!("Vertical(E{})", entity.0),
        SketchConstraint::Distance {
            entity_a,
            entity_b,
            value,
        } => format!("Distance(E{}, E{}, {:.1}mm)", entity_a.0, entity_b.0, value),
        SketchConstraint::Coincident { point_a, point_b } => {
            format!("Coincident(P{}, P{})", point_a.0, point_b.0)
        }
        SketchConstraint::Dragged { point } => format!("Dragged(P{})", point.0),
        SketchConstraint::Radius { entity, value } => {
            format!("Radius(E{}, {:.1}mm)", entity.0, value)
        }
        SketchConstraint::Diameter { entity, value } => {
            format!("Diameter(E{}, {:.1}mm)", entity.0, value)
        }
        SketchConstraint::Equal { entity_a, entity_b } => {
            format!("Equal(E{}, E{})", entity_a.0, entity_b.0)
        }
        SketchConstraint::Parallel { line_a, line_b } => {
            format!("Parallel(E{}, E{})", line_a.0, line_b.0)
        }
        SketchConstraint::Perpendicular { line_a, line_b } => {
            format!("Perpendicular(E{}, E{})", line_a.0, line_b.0)
        }
        SketchConstraint::Tangent { line, curve } => {
            format!("Tangent(E{}, E{})", line.0, curve.0)
        }
        SketchConstraint::Angle {
            line_a,
            line_b,
            value_degrees,
        } => format!("Angle(E{}, E{}, {:.1}°)", line_a.0, line_b.0, value_degrees),
        SketchConstraint::Symmetric {
            entity_a,
            entity_b,
            symmetry_line,
        } => format!(
            "Symmetric(P{}, P{}, line=E{})",
            entity_a.0, entity_b.0, symmetry_line.0
        ),
        SketchConstraint::SymmetricH { point_a, point_b } => {
            format!("SymmetricH(P{}, P{})", point_a.0, point_b.0)
        }
        SketchConstraint::SymmetricV { point_a, point_b } => {
            format!("SymmetricV(P{}, P{})", point_a.0, point_b.0)
        }
        SketchConstraint::Midpoint { point, line } => {
            format!("Midpoint(P{}, E{})", point.0, line.0)
        }
        SketchConstraint::OnEntity { point, entity } => {
            format!("OnEntity(P{}, E{})", point.0, entity.0)
        }
        SketchConstraint::EqualAngle {
            line_a,
            line_b,
            line_c,
            line_d,
        } => format!(
            "EqualAngle(E{}, E{}, E{}, E{})",
            line_a.0, line_b.0, line_c.0, line_d.0
        ),
        SketchConstraint::Ratio {
            entity_a,
            entity_b,
            value,
        } => format!("Ratio(E{}, E{}, {:.2})", entity_a.0, entity_b.0, value),
        SketchConstraint::EqualPointToLine {
            point_a,
            point_b,
            line,
        } => format!(
            "EqualPointToLine(P{}, P{}, E{})",
            point_a.0, point_b.0, line.0
        ),
        SketchConstraint::SameOrientation { entity_a, entity_b } => {
            format!("SameOrientation(E{}, E{})", entity_a.0, entity_b.0)
        }
    }
}

fn titlecase(s: &str) -> String {
    s.replace('_', " ")
        .split_whitespace()
        .map(|w| {
            let mut c = w.chars();
            match c.next() {
                None => String::new(),
                Some(f) => f.to_uppercase().to_string() + c.as_str(),
            }
        })
        .collect::<Vec<_>>()
        .join(" ")
}

// ─── Scenario 1: Parametric Rectangle ────────────────────────────────────────

fn scenario_parametric_rectangle() {
    let mut wb = SketchWorkbench::new("01_parametric_rectangle", "Parametric Rectangle");

    // Step 0: Four corner points at rough positions
    wb.add_entities(vec![
        SketchEntity::Point {
            id: PointId(1),
            x: 5.0,
            y: 5.0,
            construction: false,
        },
        SketchEntity::Point {
            id: PointId(2),
            x: 110.0,
            y: 8.0,
            construction: false,
        },
        SketchEntity::Point {
            id: PointId(3),
            x: 105.0,
            y: 75.0,
            construction: false,
        },
        SketchEntity::Point {
            id: PointId(4),
            x: 3.0,
            y: 78.0,
            construction: false,
        },
    ]);
    wb.snapshot(
        "four_points",
        "Four corner points placed at rough positions. No constraints — all points free to move.",
    );

    // Step 1: Connect with lines
    wb.add_entities(vec![
        SketchEntity::Line {
            id: LineId(10),
            start_id: PointId(1),
            end_id: PointId(2),
            construction: false,
        },
        SketchEntity::Line {
            id: LineId(11),
            start_id: PointId(2),
            end_id: PointId(3),
            construction: false,
        },
        SketchEntity::Line {
            id: LineId(12),
            start_id: PointId(3),
            end_id: PointId(4),
            construction: false,
        },
        SketchEntity::Line {
            id: LineId(13),
            start_id: PointId(4),
            end_id: PointId(1),
            construction: false,
        },
    ]);
    wb.snapshot("connected", "Lines connect corners into a closed loop. DOF unchanged — lines reference existing points, adding no new parameters.");

    // Step 2: Horizontal and vertical constraints
    wb.add_constraints(vec![
        SketchConstraint::Horizontal {
            entity: EntityId(10),
        },
        SketchConstraint::Horizontal {
            entity: EntityId(12),
        },
        SketchConstraint::Vertical {
            entity: EntityId(11),
        },
        SketchConstraint::Vertical {
            entity: EntityId(13),
        },
    ]);
    wb.snapshot("hv_constrained", "Top/bottom horizontal, left/right vertical. Rectangle now axis-aligned but size and position still free.");

    // Step 3: Pin origin
    wb.add_constraint(SketchConstraint::Dragged { point: PointId(1) });
    wb.snapshot(
        "pinned",
        "Bottom-left corner pinned at origin. Position fixed, but width and height still free.",
    );

    // Step 4: Width constraint
    wb.add_constraint(SketchConstraint::Distance {
        entity_a: EntityId(1),
        entity_b: EntityId(2),
        value: 120.0,
    });
    wb.snapshot(
        "width_set",
        "Width constrained to 120mm. Only height remains free.",
    );

    // Step 5: Height constraint — fully constrained
    wb.add_constraint(SketchConstraint::Distance {
        entity_a: EntityId(2),
        entity_b: EntityId(3),
        value: 80.0,
    });
    wb.snapshot("fully_constrained", "Height constrained to 80mm. Fully constrained — zero DOF, rectangle is 120×80mm at origin.");

    wb.finish();
}

// ─── Scenario 2: Bolt Circle Pattern ─────────────────────────────────────────

fn scenario_bolt_circle() {
    let mut wb = SketchWorkbench::new("02_bolt_circle", "Bolt Circle Pattern");

    // Step 0: Center point
    wb.add_entity(SketchEntity::Point {
        id: PointId(1),
        x: 50.0,
        y: 50.0,
        construction: false,
    });
    wb.snapshot(
        "center_point",
        "Single center point for the concentric circles.",
    );

    // Step 1: Three concentric circles
    wb.add_entities(vec![
        SketchEntity::Circle {
            id: CircleId(10),
            center_id: PointId(1),
            radius: 12.0,
            construction: false,
        },
        SketchEntity::Circle {
            id: CircleId(11),
            center_id: PointId(1),
            radius: 22.0,
            construction: false,
        },
        SketchEntity::Circle {
            id: CircleId(12),
            center_id: PointId(1),
            radius: 38.0,
            construction: false,
        },
    ]);
    wb.snapshot("three_circles", "Three concentric circles at rough radii. All share the center point. Radii and center position free.");

    // Step 2: Pin center
    wb.add_constraint(SketchConstraint::Dragged { point: PointId(1) });
    wb.snapshot(
        "center_pinned",
        "Center pinned at (50, 50). Only radii remain free.",
    );

    // Step 3: Inner radius
    wb.add_constraint(SketchConstraint::Radius {
        entity: EntityId(10),
        value: 10.0,
    });
    wb.snapshot(
        "inner_radius",
        "Inner circle constrained to R=10mm (bolt hole).",
    );

    // Step 4: Middle radius
    wb.add_constraint(SketchConstraint::Radius {
        entity: EntityId(11),
        value: 25.0,
    });
    wb.snapshot(
        "middle_radius",
        "Middle circle constrained to R=25mm (bolt circle diameter).",
    );

    // Step 5: Outer radius
    wb.add_constraint(SketchConstraint::Radius {
        entity: EntityId(12),
        value: 40.0,
    });
    wb.snapshot(
        "outer_radius",
        "Outer circle constrained to R=40mm (flange edge). All radii defined.",
    );

    // Step 6: Equal constraint forcing inner two to same size
    wb.add_constraint(SketchConstraint::Equal {
        entity_a: EntityId(10),
        entity_b: EntityId(11),
    });
    wb.snapshot("equal_radii", "Equal constraint forces inner and middle circles to same radius. Over-constrains unless we remove one radius — demonstrates the over-constrained state.");

    wb.finish();
}

// ─── Scenario 3: Tangent Arc Transition ──────────────────────────────────────

fn scenario_tangent_arc() {
    let mut wb = SketchWorkbench::new("03_tangent_arc_transition", "Tangent Arc Transition");

    // Step 0: Points for the V-shape
    wb.add_entities(vec![
        SketchEntity::Point {
            id: PointId(1),
            x: 0.0,
            y: 0.0,
            construction: false,
        },
        SketchEntity::Point {
            id: PointId(2),
            x: 50.0,
            y: 60.0,
            construction: false,
        },
        SketchEntity::Point {
            id: PointId(3),
            x: 100.0,
            y: 0.0,
            construction: false,
        },
        // Arc center and endpoints (near the junction)
        SketchEntity::Point {
            id: PointId(4),
            x: 35.0,
            y: 40.0,
            construction: false,
        },
        SketchEntity::Point {
            id: PointId(5),
            x: 65.0,
            y: 40.0,
            construction: false,
        },
        SketchEntity::Point {
            id: PointId(6),
            x: 50.0,
            y: 25.0,
            construction: false,
        }, // arc center
    ]);
    wb.snapshot(
        "v_points",
        "Six points: two line endpoints, V-apex, arc center and connection points.",
    );

    // Step 1: Two line segments forming V
    wb.add_entities(vec![
        SketchEntity::Line {
            id: LineId(10),
            start_id: PointId(1),
            end_id: PointId(4),
            construction: false,
        },
        SketchEntity::Line {
            id: LineId(11),
            start_id: PointId(5),
            end_id: PointId(3),
            construction: false,
        },
    ]);
    wb.snapshot(
        "v_lines",
        "Two line segments forming the arms of a V-shape. Arc will bridge the gap at the top.",
    );

    // Step 2: Arc connecting the line endpoints
    wb.add_entity(SketchEntity::Arc {
        id: ArcId(20),
        center_id: PointId(6),
        start_id: PointId(4),
        end_id: PointId(5),
        construction: false,
    });
    wb.snapshot(
        "arc_added",
        "Arc added connecting the tops of both line segments. Creates a smooth transition path.",
    );

    // Step 3: Tangent constraints at both junctions
    wb.add_constraints(vec![
        SketchConstraint::Tangent {
            line: EntityId(10),
            curve: EntityId(20),
        },
        SketchConstraint::Tangent {
            line: EntityId(11),
            curve: EntityId(20),
        },
    ]);
    wb.snapshot("tangent_constrained", "Tangent constraints at both line-arc junctions. Arc now smoothly transitions between the two lines.");

    // Step 4: Pin base points
    wb.add_constraints(vec![
        SketchConstraint::Dragged { point: PointId(1) },
        SketchConstraint::Dragged { point: PointId(3) },
    ]);
    wb.snapshot("bases_pinned", "Base points of both lines pinned. Shape position fixed, arc geometry adjusting to tangency.");

    // Step 5: Arc radius
    wb.add_constraint(SketchConstraint::Radius {
        entity: EntityId(20),
        value: 20.0,
    });
    wb.snapshot(
        "arc_radius_set",
        "Arc radius constrained to 20mm. Shape position and arc size fixed.",
    );

    // Step 6: Symmetric V-shape (equal line lengths)
    wb.add_constraint(SketchConstraint::Equal {
        entity_a: EntityId(10),
        entity_b: EntityId(11),
    });
    wb.snapshot(
        "symmetric_v",
        "Equal line lengths make V-shape symmetric. Tangent arc transition fully defined.",
    );

    wb.finish();
}

// ─── Scenario 4: Symmetric Mounting Bracket ──────────────────────────────────

fn scenario_symmetric_bracket() {
    let mut wb = SketchWorkbench::new("04_symmetric_bracket", "Symmetric Mounting Bracket");

    // Step 0: Half-bracket points (left side of L-shape)
    wb.add_entities(vec![
        SketchEntity::Point {
            id: PointId(1),
            x: 0.0,
            y: 0.0,
            construction: false,
        },
        SketchEntity::Point {
            id: PointId(2),
            x: -40.0,
            y: 0.0,
            construction: false,
        },
        SketchEntity::Point {
            id: PointId(3),
            x: -40.0,
            y: 20.0,
            construction: false,
        },
        SketchEntity::Point {
            id: PointId(4),
            x: -15.0,
            y: 20.0,
            construction: false,
        },
        SketchEntity::Point {
            id: PointId(5),
            x: -15.0,
            y: 60.0,
            construction: false,
        },
        SketchEntity::Point {
            id: PointId(6),
            x: 0.0,
            y: 60.0,
            construction: false,
        },
        // Mirror points (right side)
        SketchEntity::Point {
            id: PointId(7),
            x: 40.0,
            y: 0.0,
            construction: false,
        },
        SketchEntity::Point {
            id: PointId(8),
            x: 40.0,
            y: 20.0,
            construction: false,
        },
        SketchEntity::Point {
            id: PointId(9),
            x: 15.0,
            y: 20.0,
            construction: false,
        },
        SketchEntity::Point {
            id: PointId(10),
            x: 15.0,
            y: 60.0,
            construction: false,
        },
    ]);
    wb.snapshot("bracket_points", "Ten points: 6 for the left half of an L-bracket, 4 mirrored on the right. Point 1 and 6 lie on the Y-axis.");

    // Step 1: Left half lines
    wb.add_entities(vec![
        SketchEntity::Line {
            id: LineId(20),
            start_id: PointId(1),
            end_id: PointId(2),
            construction: false,
        },
        SketchEntity::Line {
            id: LineId(21),
            start_id: PointId(2),
            end_id: PointId(3),
            construction: false,
        },
        SketchEntity::Line {
            id: LineId(22),
            start_id: PointId(3),
            end_id: PointId(4),
            construction: false,
        },
        SketchEntity::Line {
            id: LineId(23),
            start_id: PointId(4),
            end_id: PointId(5),
            construction: false,
        },
        SketchEntity::Line {
            id: LineId(24),
            start_id: PointId(5),
            end_id: PointId(6),
            construction: false,
        },
        // Right half lines
        SketchEntity::Line {
            id: LineId(25),
            start_id: PointId(1),
            end_id: PointId(7),
            construction: false,
        },
        SketchEntity::Line {
            id: LineId(26),
            start_id: PointId(7),
            end_id: PointId(8),
            construction: false,
        },
        SketchEntity::Line {
            id: LineId(27),
            start_id: PointId(8),
            end_id: PointId(9),
            construction: false,
        },
        SketchEntity::Line {
            id: LineId(28),
            start_id: PointId(9),
            end_id: PointId(10),
            construction: false,
        },
        SketchEntity::Line {
            id: LineId(29),
            start_id: PointId(10),
            end_id: PointId(6),
            construction: false,
        },
    ]);
    wb.snapshot("bracket_lines", "Ten lines forming a closed symmetric L-bracket profile. Left and right halves share points 1 (bottom) and 6 (top) on the Y-axis.");

    // Step 2: Construction line on Y-axis for symmetry reference
    wb.add_entities(vec![
        SketchEntity::Point {
            id: PointId(50),
            x: 0.0,
            y: -10.0,
            construction: true,
        },
        SketchEntity::Point {
            id: PointId(51),
            x: 0.0,
            y: 70.0,
            construction: true,
        },
        SketchEntity::Line {
            id: LineId(60),
            start_id: PointId(50),
            end_id: PointId(51),
            construction: true,
        },
    ]);
    wb.add_constraint(SketchConstraint::Vertical {
        entity: EntityId(60),
    });
    wb.snapshot("construction_line", "Vertical construction line on Y-axis added as symmetry reference. Rendered with grey dashed stroke.");

    // Step 3: Horizontal/vertical constraints on bracket edges
    wb.add_constraints(vec![
        SketchConstraint::Horizontal {
            entity: EntityId(20),
        },
        SketchConstraint::Horizontal {
            entity: EntityId(22),
        },
        SketchConstraint::Horizontal {
            entity: EntityId(24),
        },
        SketchConstraint::Horizontal {
            entity: EntityId(25),
        },
        SketchConstraint::Horizontal {
            entity: EntityId(27),
        },
        SketchConstraint::Horizontal {
            entity: EntityId(29),
        },
        SketchConstraint::Vertical {
            entity: EntityId(21),
        },
        SketchConstraint::Vertical {
            entity: EntityId(23),
        },
        SketchConstraint::Vertical {
            entity: EntityId(26),
        },
        SketchConstraint::Vertical {
            entity: EntityId(28),
        },
    ]);
    wb.snapshot("hv_constrained", "All bracket edges forced horizontal or vertical. Shape is axis-aligned but size/position still free.");

    // Step 4: SymmetricH to mirror point pairs across Y-axis
    wb.add_constraints(vec![
        SketchConstraint::SymmetricH {
            point_a: PointId(2),
            point_b: PointId(7),
        },
        SketchConstraint::SymmetricH {
            point_a: PointId(3),
            point_b: PointId(8),
        },
        SketchConstraint::SymmetricH {
            point_a: PointId(4),
            point_b: PointId(9),
        },
        SketchConstraint::SymmetricH {
            point_a: PointId(5),
            point_b: PointId(10),
        },
    ]);
    wb.snapshot("symmetric", "SymmetricH constraints mirror 4 point pairs across the Y-axis. Right half now exactly mirrors left half.");

    // Step 5: Pin origin, construction line endpoints, and add dimensions
    wb.add_constraints(vec![
        SketchConstraint::Dragged { point: PointId(1) },
        SketchConstraint::Dragged { point: PointId(50) },
        SketchConstraint::Dragged { point: PointId(51) },
        SketchConstraint::Distance {
            entity_a: EntityId(1),
            entity_b: EntityId(2),
            value: 40.0,
        },
        SketchConstraint::Distance {
            entity_a: EntityId(2),
            entity_b: EntityId(3),
            value: 20.0,
        },
        SketchConstraint::Distance {
            entity_a: EntityId(4),
            entity_b: EntityId(5),
            value: 40.0,
        },
        SketchConstraint::Distance {
            entity_a: EntityId(5),
            entity_b: EntityId(6),
            value: 15.0,
        },
        SketchConstraint::Distance {
            entity_a: EntityId(1),
            entity_b: EntityId(6),
            value: 60.0,
        },
    ]);
    wb.snapshot("dimensioned", "Origin pinned, construction line fixed, all dimensions set (width 80mm, base 20mm, arm 40mm, arm width 15mm, total height 60mm). Near-fully constrained — remaining DOF from redundant symmetry interactions.");

    wb.finish();
}

// ─── Scenario 5: Hex Bolt Head ───────────────────────────────────────────────

fn scenario_hex_bolt() {
    let mut wb = SketchWorkbench::new("05_hex_bolt_head", "Hex Bolt Head");

    // Regular hexagon: 6 vertices at radius 30mm, centered at (50,50)
    let cx = 50.0_f64;
    let cy = 50.0_f64;
    let r = 30.0_f64;
    let pts: Vec<(f64, f64)> = (0..6)
        .map(|i| {
            let angle = std::f64::consts::FRAC_PI_3 * i as f64 + std::f64::consts::FRAC_PI_6;
            (cx + r * angle.cos(), cy + r * angle.sin())
        })
        .collect();

    // Step 0: Six points
    wb.add_entities(
        pts.iter()
            .enumerate()
            .map(|(i, &(x, y))| SketchEntity::Point {
                id: PointId(i as u32 + 1),
                x,
                y,
                construction: false,
            })
            .collect(),
    );
    wb.snapshot("hex_points", "Six points arranged roughly as a hexagon. Positions approximate — constraint solving will regularize them.");

    // Step 1: Six lines forming the hexagon
    for i in 0..6u32 {
        let next = (i + 1) % 6;
        wb.add_entity(SketchEntity::Line {
            id: LineId(10 + i),
            start_id: PointId(i + 1),
            end_id: PointId(next + 1),
            construction: false,
        });
    }
    wb.snapshot(
        "hex_lines",
        "Six lines connecting adjacent points into a closed hexagonal loop.",
    );

    // Step 2: Pin one vertex
    wb.add_constraint(SketchConstraint::Dragged { point: PointId(1) });
    wb.snapshot(
        "vertex_pinned",
        "First vertex pinned. Hexagon can still rotate, scale, and deform.",
    );

    // Step 3: Equal length on all sides (chain: each side equals the next)
    wb.add_constraints(vec![
        SketchConstraint::Equal {
            entity_a: EntityId(10),
            entity_b: EntityId(11),
        },
        SketchConstraint::Equal {
            entity_a: EntityId(11),
            entity_b: EntityId(12),
        },
        SketchConstraint::Equal {
            entity_a: EntityId(12),
            entity_b: EntityId(13),
        },
        SketchConstraint::Equal {
            entity_a: EntityId(13),
            entity_b: EntityId(14),
        },
        SketchConstraint::Equal {
            entity_a: EntityId(14),
            entity_b: EntityId(15),
        },
    ]);
    wb.snapshot("equal_sides", "Equal-length chain: all six sides forced to same length. Shape regularizing toward regular hexagon.");

    // Step 4: One side length to set scale
    wb.add_constraint(SketchConstraint::Distance {
        entity_a: EntityId(1),
        entity_b: EntityId(2),
        value: 30.0,
    });
    wb.snapshot(
        "side_length",
        "First side constrained to 30mm. All sides now 30mm via equal chain. Size is fixed.",
    );

    // Step 5: Angle constraint to fix rotation
    wb.add_constraint(SketchConstraint::Horizontal {
        entity: EntityId(10),
    });
    wb.snapshot("rotation_fixed", "Bottom side forced horizontal, fixing the hexagon's rotation. Fully constrained regular hexagon.");

    wb.finish();
}

// ─── Scenario 6: Slotted Plate ──────────────────────────────────────────────

fn scenario_slotted_plate() {
    let mut wb = SketchWorkbench::new("06_slotted_plate", "Slotted Plate");

    // Step 0: Outer rectangle points
    wb.add_entities(vec![
        SketchEntity::Point {
            id: PointId(1),
            x: 0.0,
            y: 0.0,
            construction: false,
        },
        SketchEntity::Point {
            id: PointId(2),
            x: 120.0,
            y: 0.0,
            construction: false,
        },
        SketchEntity::Point {
            id: PointId(3),
            x: 120.0,
            y: 60.0,
            construction: false,
        },
        SketchEntity::Point {
            id: PointId(4),
            x: 0.0,
            y: 60.0,
            construction: false,
        },
    ]);
    wb.snapshot(
        "plate_corners",
        "Four corners of the outer plate rectangle.",
    );

    // Step 1: Rectangle lines
    wb.add_entities(vec![
        SketchEntity::Line {
            id: LineId(10),
            start_id: PointId(1),
            end_id: PointId(2),
            construction: false,
        },
        SketchEntity::Line {
            id: LineId(11),
            start_id: PointId(2),
            end_id: PointId(3),
            construction: false,
        },
        SketchEntity::Line {
            id: LineId(12),
            start_id: PointId(3),
            end_id: PointId(4),
            construction: false,
        },
        SketchEntity::Line {
            id: LineId(13),
            start_id: PointId(4),
            end_id: PointId(1),
            construction: false,
        },
    ]);
    wb.snapshot(
        "plate_outline",
        "Closed rectangular outline for the plate body.",
    );

    // Step 2: Constrain the rectangle
    wb.add_constraints(vec![
        SketchConstraint::Horizontal {
            entity: EntityId(10),
        },
        SketchConstraint::Horizontal {
            entity: EntityId(12),
        },
        SketchConstraint::Vertical {
            entity: EntityId(11),
        },
        SketchConstraint::Vertical {
            entity: EntityId(13),
        },
        SketchConstraint::Dragged { point: PointId(1) },
        SketchConstraint::Distance {
            entity_a: EntityId(1),
            entity_b: EntityId(2),
            value: 120.0,
        },
        SketchConstraint::Distance {
            entity_a: EntityId(2),
            entity_b: EntityId(3),
            value: 60.0,
        },
    ]);
    wb.snapshot(
        "plate_constrained",
        "Outer rectangle fully constrained: 120x60mm at origin. Now adding the stadium slot.",
    );

    // Step 3: Slot geometry — stadium shape (two lines + two semicircular caps)
    // Slot runs horizontally, centered at y=30, from x=30 to x=90, R=6mm caps
    // Points: 4 arc endpoints (shared with lines) + 2 arc centers
    wb.add_entities(vec![
        // Left cap: top, bottom, center
        SketchEntity::Point {
            id: PointId(20),
            x: 30.0,
            y: 36.0,
            construction: false,
        },
        SketchEntity::Point {
            id: PointId(21),
            x: 30.0,
            y: 24.0,
            construction: false,
        },
        SketchEntity::Point {
            id: PointId(22),
            x: 30.0,
            y: 30.0,
            construction: false,
        },
        // Right cap: top, bottom, center
        SketchEntity::Point {
            id: PointId(23),
            x: 90.0,
            y: 36.0,
            construction: false,
        },
        SketchEntity::Point {
            id: PointId(24),
            x: 90.0,
            y: 24.0,
            construction: false,
        },
        SketchEntity::Point {
            id: PointId(25),
            x: 90.0,
            y: 30.0,
            construction: false,
        },
    ]);
    wb.snapshot(
        "slot_points",
        "Six points for the stadium slot: left cap (top P20, bottom P21, center P22) and right cap (top P23, bottom P24, center P25).",
    );

    // Step 4: Slot edges — two horizontal lines + two semicircular arcs
    wb.add_entities(vec![
        // Top straight edge: left-top → right-top
        SketchEntity::Line {
            id: LineId(30),
            start_id: PointId(20),
            end_id: PointId(23),
            construction: false,
        },
        // Bottom straight edge: right-bottom → left-bottom
        SketchEntity::Line {
            id: LineId(31),
            start_id: PointId(24),
            end_id: PointId(21),
            construction: false,
        },
        // Left semicircular cap: bottom → top (counterclockwise, center on left)
        SketchEntity::Arc {
            id: ArcId(40),
            center_id: PointId(22),
            start_id: PointId(21),
            end_id: PointId(20),
            construction: false,
        },
        // Right semicircular cap: top → bottom (counterclockwise, center on right)
        SketchEntity::Arc {
            id: ArcId(41),
            center_id: PointId(25),
            start_id: PointId(23),
            end_id: PointId(24),
            construction: false,
        },
    ]);
    wb.snapshot(
        "slot_shape",
        "Stadium slot: two horizontal lines + two semicircular end caps. Closed loop: P20→P23 (top line) → arc right → P24→P21 (bottom line) → arc left → back to P20.",
    );

    // Step 5: Constrain slot lines horizontal
    wb.add_constraints(vec![
        SketchConstraint::Horizontal {
            entity: EntityId(30),
        },
        SketchConstraint::Horizontal {
            entity: EntityId(31),
        },
    ]);
    wb.snapshot(
        "slot_horizontal",
        "Slot sides constrained horizontal. Slot is level within the plate.",
    );

    // Step 6: Arc radii — both caps R=6mm
    wb.add_constraints(vec![
        SketchConstraint::Radius {
            entity: EntityId(40),
            value: 6.0,
        },
        SketchConstraint::Radius {
            entity: EntityId(41),
            value: 6.0,
        },
    ]);
    wb.snapshot(
        "slot_radii",
        "Both arc caps constrained to R=6mm. Slot width is now 12mm. Position and length still free.",
    );

    // Step 7: Construction line at plate center for Symmetric constraint
    wb.add_entities(vec![
        SketchEntity::Point {
            id: PointId(50),
            x: 60.0,
            y: 0.0,
            construction: true,
        },
        SketchEntity::Point {
            id: PointId(51),
            x: 60.0,
            y: 60.0,
            construction: true,
        },
        SketchEntity::Line {
            id: LineId(60),
            start_id: PointId(50),
            end_id: PointId(51),
            construction: true,
        },
    ]);
    wb.add_constraints(vec![
        SketchConstraint::Vertical {
            entity: EntityId(60),
        },
        // Pin construction line on the plate midpoints
        SketchConstraint::Midpoint {
            point: PointId(50),
            line: EntityId(10),
        },
        SketchConstraint::Midpoint {
            point: PointId(51),
            line: EntityId(12),
        },
    ]);
    wb.snapshot(
        "centerline",
        "Construction line at plate center (x=60) via midpoint constraints on top/bottom plate edges. Will serve as symmetry axis for the slot.",
    );

    // Step 8: Symmetric constraint — slot centers mirrored about plate centerline
    wb.add_constraint(SketchConstraint::Symmetric {
        entity_a: PointId(22),
        entity_b: PointId(25),
        symmetry_line: EntityId(60),
    });
    wb.snapshot(
        "slot_symmetric",
        "Slot centers constrained symmetric about the plate centerline. Slot is horizontally centered in the plate.",
    );

    // Step 9: Fix arc END points to lie on their circles.
    //
    // SOLVER GAP FOUND: Arc radius is RadiusDef::Implicit(start_point), derived
    // from dist(center, start). The END point has no implicit binding — it's a
    // free point that can drift off the arc circle. We must add explicit OnEntity
    // constraints for end points. Start points are already implicitly on the circle.
    wb.add_constraints(vec![
        // Left arc end point P20 onto arc 40
        SketchConstraint::OnEntity {
            point: PointId(20),
            entity: EntityId(40),
        },
        // Right arc end point P24 onto arc 41
        SketchConstraint::OnEntity {
            point: PointId(24),
            entity: EntityId(41),
        },
    ]);
    wb.snapshot(
        "end_points_on_arcs",
        "OnEntity constraints bind arc END points to their circles. (Start points are implicitly on-circle via RadiusDef::Implicit — but end points are free. This is a solver gap.)",
    );

    // Step 10: Pin slot position and fix endpoint angular positions.
    // Centers are fixed (Dragged + Symmetric). Radii are fixed. End points are
    // now on-circle. But endpoints can still rotate around the circle.
    // Constrain the vertical span of each cap: dist(top, bottom) = 2R = 12mm.
    wb.add_constraints(vec![
        SketchConstraint::Dragged { point: PointId(22) },
        SketchConstraint::Distance {
            entity_a: EntityId(20),
            entity_b: EntityId(21),
            value: 12.0,
        },
        SketchConstraint::Distance {
            entity_a: EntityId(23),
            entity_b: EntityId(24),
            value: 12.0,
        },
    ]);
    wb.snapshot(
        "slot_positioned",
        "Left center pinned, endpoint spacing = 2R locks semicircular cap orientation. Compound plate with stadium slot.",
    );

    wb.finish();
}

// ─── Main ────────────────────────────────────────────────────────────────────

fn main() {
    eprintln!("Sketch Workbench — building 6 engineering scenarios\n");

    scenario_parametric_rectangle();
    scenario_bolt_circle();
    scenario_tangent_arc();
    scenario_symmetric_bracket();
    scenario_hex_bolt();
    scenario_slotted_plate();

    eprintln!("\nDone! All scenarios written to crates/sketch-solver/output/workbench/");
}
