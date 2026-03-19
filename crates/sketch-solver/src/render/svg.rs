//! SVG generation for solved sketches.
//!
//! Renders sketch entities, constraint annotations, DOF status coloring,
//! and profile highlighting into a self-contained SVG string.

use std::fmt::Write;

use crate::types::{ClosedProfile, Sketch, SketchConstraint, SketchEntity, SolveStatus, SolvedSketch};

// ── Colors ──────────────────────────────────────────────────────────────────

const COLOR_ENTITY: &str = "#2196F3"; // blue
const COLOR_FULLY: &str = "#4CAF50"; // green
const COLOR_UNDER: &str = "#FF9800"; // amber
const COLOR_OVER: &str = "#F44336"; // red
const COLOR_PROFILE_FILL: &str = "#E3F2FD"; // light blue
const COLOR_GRID_MINOR: &str = "#e0e0e0";
const COLOR_GRID_MAJOR: &str = "#c0c0c0";
const COLOR_BADGE_BG: &str = "#FFFFFF";
const COLOR_BADGE_TEXT: &str = "#333333";

const GRID_MINOR: f64 = 10.0;
const GRID_MAJOR: f64 = 100.0;
const PADDING_FRAC: f64 = 0.2;
const MIN_EXTENT: f64 = 50.0;

/// Render a solved sketch to an SVG string.
///
/// The SVG uses a coordinate system where sketch Y-up is transformed to
/// SVG Y-down. The viewBox is auto-calculated from entity bounding box
/// with padding.
pub fn render_sketch_svg(sketch: &Sketch, solved: &SolvedSketch) -> String {
    let positions = &solved.positions;
    let status = &solved.status;

    // Compute bounding box from solved positions
    let (min_x, min_y, max_x, max_y) = bounding_box(positions);
    let width = (max_x - min_x).max(MIN_EXTENT);
    let height = (max_y - min_y).max(MIN_EXTENT);
    let pad_x = width * PADDING_FRAC;
    let pad_y = height * PADDING_FRAC;

    // SVG viewBox in sketch coordinates (Y-up), we'll flip in the transform
    let vb_x = min_x - pad_x;
    let vb_y = min_y - pad_y;
    let vb_w = width + 2.0 * pad_x;
    let vb_h = height + 2.0 * pad_y;

    let point_color = status_color(status);

    let mut svg = String::with_capacity(4096);

    // SVG header — flip Y axis via transform on the root group
    writeln!(
        svg,
        r#"<svg xmlns="http://www.w3.org/2000/svg" viewBox="{vb_x:.2} {y_min:.2} {vb_w:.2} {vb_h:.2}" width="{svg_w}" height="{svg_h}">"#,
        vb_x = vb_x,
        y_min = -(vb_y + vb_h), // flip Y for SVG
        vb_w = vb_w,
        vb_h = vb_h,
        svg_w = (vb_w * 2.0).round() as i64,
        svg_h = (vb_h * 2.0).round() as i64,
    )
    .unwrap();

    // Background
    writeln!(
        svg,
        r#"<rect x="{}" y="{}" width="{}" height="{}" fill="white"/>"#,
        vb_x,
        -(vb_y + vb_h),
        vb_w,
        vb_h
    )
    .unwrap();

    // All content in a group that flips Y: scale(1, -1)
    // In SVG, Y goes down. In sketch, Y goes up. We negate Y coords inline.
    // Grid
    write_grid(&mut svg, vb_x, vb_y, vb_w, vb_h);

    // Profiles (filled regions, drawn first so entities overlay)
    write_profiles(&mut svg, &solved.profiles, positions);

    // Entities
    write_entities(&mut svg, &sketch.entities, positions, point_color);

    // Constraint badges
    write_constraint_badges(&mut svg, &sketch.constraints, &sketch.entities, positions);

    // Status badge in top-left (offset enough so badge rect + text are fully visible)
    write_status_badge(&mut svg, status, vb_x + pad_x, -(vb_y + vb_h) + 10.0);

    writeln!(svg, "</svg>").unwrap();
    svg
}

fn status_color(status: &SolveStatus) -> &'static str {
    match status {
        SolveStatus::FullyConstrained => COLOR_FULLY,
        SolveStatus::UnderConstrained { .. } => COLOR_UNDER,
        SolveStatus::OverConstrained { .. } | SolveStatus::SolveFailed { .. } => COLOR_OVER,
    }
}

fn bounding_box(
    positions: &std::collections::HashMap<u32, (f64, f64)>,
) -> (f64, f64, f64, f64) {
    let mut min_x = f64::INFINITY;
    let mut min_y = f64::INFINITY;
    let mut max_x = f64::NEG_INFINITY;
    let mut max_y = f64::NEG_INFINITY;

    for (x, y) in positions.values() {
        min_x = min_x.min(*x);
        min_y = min_y.min(*y);
        max_x = max_x.max(*x);
        max_y = max_y.max(*y);
    }

    if min_x > max_x {
        // No points at all
        (0.0, 0.0, MIN_EXTENT, MIN_EXTENT)
    } else {
        (min_x, min_y, max_x, max_y)
    }
}

/// Convert sketch Y (up) to SVG Y (down).
fn svg_y(y: f64) -> f64 {
    -y
}

fn write_grid(svg: &mut String, vb_x: f64, vb_y: f64, vb_w: f64, vb_h: f64) {
    // Grid in SVG coordinates (Y is already flipped in viewBox)
    let svg_top = -(vb_y + vb_h);
    let svg_left = vb_x;

    // Minor grid (10mm)
    let x_start = (svg_left / GRID_MINOR).floor() * GRID_MINOR;
    let y_start = (svg_top / GRID_MINOR).floor() * GRID_MINOR;

    writeln!(
        svg,
        r#"<g class="grid-minor" stroke="{}" stroke-width="0.3">"#,
        COLOR_GRID_MINOR
    )
    .unwrap();
    let mut x = x_start;
    while x <= svg_left + vb_w {
        writeln!(
            svg,
            r#"<line x1="{x:.2}" y1="{y1:.2}" x2="{x:.2}" y2="{y2:.2}"/>"#,
            x = x,
            y1 = svg_top,
            y2 = svg_top + vb_h,
        )
        .unwrap();
        x += GRID_MINOR;
    }
    let mut y = y_start;
    while y <= svg_top + vb_h {
        writeln!(
            svg,
            r#"<line x1="{x1:.2}" y1="{y:.2}" x2="{x2:.2}" y2="{y:.2}"/>"#,
            x1 = svg_left,
            y = y,
            x2 = svg_left + vb_w,
        )
        .unwrap();
        y += GRID_MINOR;
    }
    writeln!(svg, "</g>").unwrap();

    // Major grid (100mm)
    let x_start = (svg_left / GRID_MAJOR).floor() * GRID_MAJOR;
    let y_start = (svg_top / GRID_MAJOR).floor() * GRID_MAJOR;

    writeln!(
        svg,
        r#"<g class="grid-major" stroke="{}" stroke-width="0.6">"#,
        COLOR_GRID_MAJOR
    )
    .unwrap();
    let mut x = x_start;
    while x <= svg_left + vb_w {
        writeln!(
            svg,
            r#"<line x1="{x:.2}" y1="{y1:.2}" x2="{x:.2}" y2="{y2:.2}"/>"#,
            x = x,
            y1 = svg_top,
            y2 = svg_top + vb_h,
        )
        .unwrap();
        x += GRID_MAJOR;
    }
    let mut y = y_start;
    while y <= svg_top + vb_h {
        writeln!(
            svg,
            r#"<line x1="{x1:.2}" y1="{y:.2}" x2="{x2:.2}" y2="{y:.2}"/>"#,
            x1 = svg_left,
            y = y,
            x2 = svg_left + vb_w,
        )
        .unwrap();
        y += GRID_MAJOR;
    }
    writeln!(svg, "</g>").unwrap();
}

fn write_profiles(
    svg: &mut String,
    profiles: &[ClosedProfile],
    positions: &std::collections::HashMap<u32, (f64, f64)>,
) {
    for profile in profiles {
        if profile.vertex_ids.len() < 3 {
            continue;
        }
        let mut path = String::new();
        for (i, pid) in profile.vertex_ids.iter().enumerate() {
            if let Some((x, y)) = positions.get(pid) {
                let cmd = if i == 0 { "M" } else { "L" };
                write!(path, "{}{:.4},{:.4} ", cmd, x, svg_y(*y)).unwrap();
            }
        }
        path.push('Z');
        writeln!(
            svg,
            r#"<path d="{}" fill="{}" fill-opacity="0.5" stroke="none"/>"#,
            path, COLOR_PROFILE_FILL
        )
        .unwrap();
    }
}

fn write_entities(
    svg: &mut String,
    entities: &[SketchEntity],
    positions: &std::collections::HashMap<u32, (f64, f64)>,
    point_color: &str,
) {
    writeln!(svg, r#"<g class="entities">"#).unwrap();

    for entity in entities {
        match entity {
            SketchEntity::Line {
                start_id, end_id, ..
            } => {
                if let (Some((x1, y1)), Some((x2, y2))) =
                    (positions.get(start_id), positions.get(end_id))
                {
                    writeln!(
                        svg,
                        r#"<line x1="{:.4}" y1="{:.4}" x2="{:.4}" y2="{:.4}" stroke="{}" stroke-width="1.5" stroke-linecap="round"/>"#,
                        x1,
                        svg_y(*y1),
                        x2,
                        svg_y(*y2),
                        COLOR_ENTITY
                    )
                    .unwrap();
                }
            }
            SketchEntity::Circle {
                center_id, radius, ..
            } => {
                if let Some((cx, cy)) = positions.get(center_id) {
                    writeln!(
                        svg,
                        r#"<circle cx="{:.4}" cy="{:.4}" r="{:.4}" stroke="{}" stroke-width="1.5" fill="none"/>"#,
                        cx,
                        svg_y(*cy),
                        radius,
                        COLOR_ENTITY
                    )
                    .unwrap();
                }
            }
            SketchEntity::Arc {
                center_id,
                start_id,
                end_id,
                ..
            } => {
                if let (Some((cx, cy)), Some((sx, sy)), Some((ex, ey))) = (
                    positions.get(center_id),
                    positions.get(start_id),
                    positions.get(end_id),
                ) {
                    let r = ((sx - cx).powi(2) + (sy - cy).powi(2)).sqrt();
                    // Approximate arc direction: use cross product to determine sweep
                    let v1 = (sx - cx, sy - cy);
                    let v2 = (ex - cx, ey - cy);
                    let cross = v1.0 * v2.1 - v1.1 * v2.0;
                    // SVG arc: flip sweep because Y is negated
                    let sweep = if cross > 0.0 { 0 } else { 1 };
                    writeln!(
                        svg,
                        r#"<path d="M{:.4},{:.4} A{:.4},{:.4} 0 0,{} {:.4},{:.4}" stroke="{}" stroke-width="1.5" fill="none"/>"#,
                        sx,
                        svg_y(*sy),
                        r,
                        r,
                        sweep,
                        ex,
                        svg_y(*ey),
                        COLOR_ENTITY
                    )
                    .unwrap();
                }
            }
            SketchEntity::Point { id, .. } => {
                if let Some((x, y)) = positions.get(id) {
                    writeln!(
                        svg,
                        r#"<circle cx="{:.4}" cy="{:.4}" r="2" fill="{}"/>"#,
                        x,
                        svg_y(*y),
                        point_color
                    )
                    .unwrap();
                }
            }
            _ => {} // Spline, Gear — skip for now
        }
    }

    writeln!(svg, "</g>").unwrap();
}

fn write_constraint_badges(
    svg: &mut String,
    constraints: &[SketchConstraint],
    entities: &[SketchEntity],
    positions: &std::collections::HashMap<u32, (f64, f64)>,
) {
    writeln!(svg, r#"<g class="badges" font-family="sans-serif" font-size="4">"#).unwrap();

    for constraint in constraints {
        match constraint {
            SketchConstraint::Horizontal { entity } => {
                if let Some((mx, my)) = line_midpoint(*entity, entities, positions) {
                    write_badge(svg, mx, svg_y(my), "H");
                }
            }
            SketchConstraint::Vertical { entity } => {
                if let Some((mx, my)) = line_midpoint(*entity, entities, positions) {
                    write_badge(svg, mx, svg_y(my), "V");
                }
            }
            SketchConstraint::Parallel { line_a, .. } => {
                if let Some((mx, my)) = line_midpoint(*line_a, entities, positions) {
                    write_badge(svg, mx, svg_y(my), "\u{2225}"); // ∥
                }
            }
            SketchConstraint::Perpendicular { line_a, .. } => {
                if let Some((mx, my)) = line_midpoint(*line_a, entities, positions) {
                    write_badge(svg, mx, svg_y(my), "\u{22A5}"); // ⊥
                }
            }
            SketchConstraint::Distance {
                entity_a,
                entity_b,
                value,
            } => {
                if let (Some((x1, y1)), Some((x2, y2))) =
                    (positions.get(entity_a), positions.get(entity_b))
                {
                    let mx = (x1 + x2) / 2.0;
                    let my = (y1 + y2) / 2.0;
                    write_badge(svg, mx, svg_y(my), &format!("{:.1}", value));
                }
            }
            SketchConstraint::Radius { entity, value } => {
                // Place badge at the circle/arc center
                if let Some((cx, cy)) = entity_center(*entity, entities, positions) {
                    write_badge(svg, cx + 3.0, svg_y(cy), &format!("R{:.1}", value));
                }
            }
            SketchConstraint::Angle {
                line_a,
                value_degrees,
                ..
            } => {
                if let Some((mx, my)) = line_midpoint(*line_a, entities, positions) {
                    write_badge(svg, mx, svg_y(my), &format!("{:.0}\u{00B0}", value_degrees));
                }
            }
            SketchConstraint::Equal { entity_a, .. } => {
                if let Some((mx, my)) = line_midpoint(*entity_a, entities, positions) {
                    write_badge(svg, mx, svg_y(my), "=");
                }
            }
            SketchConstraint::Coincident { point_a, .. } => {
                if let Some((x, y)) = positions.get(point_a) {
                    write_badge(svg, *x, svg_y(*y), "\u{25C9}"); // ◉
                }
            }
            SketchConstraint::Tangent { .. } => {
                // Tangent badge — skip placement for now (needs curve midpoint)
            }
            SketchConstraint::Symmetric { .. } => {
                // Could place badge at midpoint of symmetry line
            }
            _ => {} // Dragged, SymmetricH/V, Midpoint, OnEntity, etc — skip badges
        }
    }

    writeln!(svg, "</g>").unwrap();
}

fn write_badge(svg: &mut String, x: f64, y: f64, label: &str) {
    let w = label.len() as f64 * 2.8 + 3.0;
    let h = 5.0;
    writeln!(
        svg,
        r#"<rect x="{:.2}" y="{:.2}" width="{:.1}" height="{:.1}" rx="1" fill="{}" stroke="{}" stroke-width="0.3" opacity="0.9"/>"#,
        x - w / 2.0,
        y - h - 1.0,
        w,
        h,
        COLOR_BADGE_BG,
        COLOR_BADGE_TEXT,
    )
    .unwrap();
    writeln!(
        svg,
        r#"<text x="{:.2}" y="{:.2}" text-anchor="middle" fill="{}">{}</text>"#,
        x,
        y - 2.0,
        COLOR_BADGE_TEXT,
        label,
    )
    .unwrap();
}

fn write_status_badge(svg: &mut String, status: &SolveStatus, x: f64, y: f64) {
    let label = match status {
        SolveStatus::FullyConstrained => "Fully Constrained",
        SolveStatus::UnderConstrained { dof } => {
            return write_badge(svg, x + 20.0, y + 3.0, &format!("Under ({dof} DOF)"));
        }
        SolveStatus::OverConstrained { .. } => "Over Constrained",
        SolveStatus::SolveFailed { .. } => "Solve Failed",
    };
    write_badge(svg, x + 20.0, y + 3.0, label);
}

/// Find the midpoint of a line entity by looking up its start/end positions.
fn line_midpoint(
    line_id: u32,
    entities: &[SketchEntity],
    positions: &std::collections::HashMap<u32, (f64, f64)>,
) -> Option<(f64, f64)> {
    for entity in entities {
        if let SketchEntity::Line {
            id,
            start_id,
            end_id,
            ..
        } = entity
        {
            if *id == line_id {
                if let (Some((x1, y1)), Some((x2, y2))) =
                    (positions.get(start_id), positions.get(end_id))
                {
                    return Some(((x1 + x2) / 2.0, (y1 + y2) / 2.0));
                }
            }
        }
    }
    None
}

/// Find the center position of a circle or arc entity.
fn entity_center(
    entity_id: u32,
    entities: &[SketchEntity],
    positions: &std::collections::HashMap<u32, (f64, f64)>,
) -> Option<(f64, f64)> {
    for entity in entities {
        match entity {
            SketchEntity::Circle { id, center_id, .. } | SketchEntity::Arc { id, center_id, .. } => {
                if *id == entity_id {
                    return positions.get(center_id).copied();
                }
            }
            _ => {}
        }
    }
    None
}
