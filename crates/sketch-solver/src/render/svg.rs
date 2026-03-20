//! SVG generation for solved sketches.
//!
//! Renders sketch entities, constraint annotations, DOF status coloring,
//! and profile highlighting into a self-contained SVG string.

use std::fmt::Write;

use crate::types::{
    ClosedProfile, Sketch, SketchConstraint, SketchEntity, SolveStatus, SolvedSketch,
};

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
const COLOR_CONSTRUCTION: &str = "#9E9E9E"; // grey for construction geometry

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
    let radii = &solved.radii;
    let status = &solved.status;

    // Compute bounding box from solved positions + entity extents (circles, arcs)
    let (min_x, min_y, max_x, max_y) =
        bounding_box_with_entities(positions, radii, &sketch.entities);
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
    write_profiles(
        &mut svg,
        &solved.profiles,
        &sketch.entities,
        positions,
        radii,
    );

    // Entities
    write_entities(&mut svg, &sketch.entities, positions, radii, point_color);

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

fn bounding_box_with_entities(
    positions: &std::collections::HashMap<u32, (f64, f64)>,
    radii: &std::collections::HashMap<u32, f64>,
    entities: &[SketchEntity],
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

    // Expand bbox for circles and arcs (center ± solved radius)
    for entity in entities {
        match entity {
            SketchEntity::Circle {
                id,
                center_id,
                radius,
                ..
            } => {
                if let Some((cx, cy)) = positions.get(center_id) {
                    let r = radii.get(id).copied().unwrap_or(*radius);
                    min_x = min_x.min(cx - r);
                    min_y = min_y.min(cy - r);
                    max_x = max_x.max(cx + r);
                    max_y = max_y.max(cy + r);
                }
            }
            SketchEntity::Arc {
                id,
                center_id,
                start_id,
                ..
            } => {
                if let Some((cx, cy)) = positions.get(center_id) {
                    let r = radii.get(id).copied().unwrap_or_else(|| {
                        if let Some((sx, sy)) = positions.get(start_id) {
                            ((sx - cx).powi(2) + (sy - cy).powi(2)).sqrt()
                        } else {
                            0.0
                        }
                    });
                    min_x = min_x.min(cx - r);
                    min_y = min_y.min(cy - r);
                    max_x = max_x.max(cx + r);
                    max_y = max_y.max(cy + r);
                }
            }
            _ => {}
        }
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

/// Emit an SVG path segment for a profile edge, going from `from_pt` to the other endpoint.
fn emit_edge_to(
    edge: &EdgeInfo,
    from_pt: u32,
    path: &mut String,
    positions: &std::collections::HashMap<u32, (f64, f64)>,
) {
    let to_pt = if from_pt == edge.pt_a {
        edge.pt_b
    } else {
        edge.pt_a
    };
    if let Some((tx, ty)) = positions.get(&to_pt) {
        if edge.is_arc {
            // For arcs, we need to compute the sweep direction.
            // If traversing in the arc's natural direction (start→end), use original sweep.
            // If reversed, flip the sweep.
            let forward = from_pt == edge.arc_start;
            if let Some((fx, fy)) = positions.get(&from_pt) {
                let (cx, cy) = edge.center;
                let v1 = (fx - cx, fy - cy);
                let v2 = (tx - cx, ty - cy);
                let cross = v1.0 * v2.1 - v1.1 * v2.0;
                // In SVG (Y-down after our flip), positive cross = counter-clockwise
                let sweep = if cross > 0.0 { 0 } else { 1 };
                let _ = forward; // sweep computed from actual direction
                write!(
                    path,
                    "A{:.4},{:.4} 0 0,{} {:.4},{:.4} ",
                    edge.radius,
                    edge.radius,
                    sweep,
                    tx,
                    svg_y(*ty)
                )
                .unwrap();
            }
        } else {
            write!(path, "L{:.4},{:.4} ", tx, svg_y(*ty)).unwrap();
        }
    }
}

/// Edge info for profile path construction.
struct EdgeInfo {
    pt_a: u32,
    pt_b: u32,
    is_arc: bool,
    center: (f64, f64),
    radius: f64,
    arc_start: u32,
}

fn write_profiles(
    svg: &mut String,
    profiles: &[ClosedProfile],
    entities: &[SketchEntity],
    positions: &std::collections::HashMap<u32, (f64, f64)>,
    radii: &std::collections::HashMap<u32, f64>,
) {
    // Build entity lookup
    let entity_map: std::collections::HashMap<u32, &SketchEntity> =
        entities.iter().map(|e| (e.id(), e)).collect();

    // Combine all profile paths into one <path> with fill-rule="evenodd"
    // so that inner (hole) profiles subtract from outer profiles.
    let mut combined_path = String::new();

    for profile in profiles {
        // Handle explicit circle profiles (circle field set by profile extractor)
        if let Some(circle) = &profile.circle {
            let cx = circle.center_u;
            let cy = svg_y(circle.center_v);
            let r = circle.radius;
            write_circle_subpath(&mut combined_path, cx, cy, r);
            continue;
        }

        // Check if this profile is a single circle entity (circle field not set,
        // but entity_ids contains one circle). Emit as circle subpath.
        if profile.entity_ids.len() == 1 {
            if let Some(SketchEntity::Circle {
                id,
                center_id,
                radius,
                ..
            }) = entity_map.get(&profile.entity_ids[0])
            {
                if let Some((cx, cy)) = positions.get(center_id) {
                    let r = radii.get(id).copied().unwrap_or(*radius);
                    write_circle_subpath(&mut combined_path, *cx, svg_y(*cy), r);
                    continue;
                }
            }
        }

        if profile.vertex_ids.len() < 3 && profile.entity_ids.is_empty() {
            continue;
        }

        // Check if any entity_ids contain arcs — if so, we need arc-aware path building.
        let has_arcs = profile.entity_ids.iter().any(|eid| {
            entity_map
                .get(eid)
                .is_some_and(|e| matches!(e, SketchEntity::Arc { .. }))
        });

        if has_arcs && !profile.entity_ids.is_empty() {
            // Build arc-aware path by chaining edges with endpoint matching
            let mut edges: Vec<EdgeInfo> = Vec::new();
            for eid in &profile.entity_ids {
                if let Some(entity) = entity_map.get(eid) {
                    match entity {
                        SketchEntity::Line {
                            start_id, end_id, ..
                        } => {
                            edges.push(EdgeInfo {
                                pt_a: *start_id,
                                pt_b: *end_id,
                                is_arc: false,
                                center: (0.0, 0.0),
                                radius: 0.0,
                                arc_start: 0,
                            });
                        }
                        SketchEntity::Arc {
                            id,
                            center_id,
                            start_id,
                            end_id,
                            ..
                        } => {
                            if let Some((cx, cy)) = positions.get(center_id) {
                                let r = radii.get(id).copied().unwrap_or_else(|| {
                                    if let Some((sx, sy)) = positions.get(start_id) {
                                        ((sx - cx).powi(2) + (sy - cy).powi(2)).sqrt()
                                    } else {
                                        0.0
                                    }
                                });
                                edges.push(EdgeInfo {
                                    pt_a: *start_id,
                                    pt_b: *end_id,
                                    is_arc: true,
                                    center: (*cx, *cy),
                                    radius: r,
                                    arc_start: *start_id,
                                });
                            }
                        }
                        _ => {}
                    }
                }
            }

            if !edges.is_empty() {
                // Chain edges: determine correct traversal direction.
                // For the first edge, pick direction by checking which endpoint
                // connects to the second edge (if available).
                let first_dir = if edges.len() > 1 {
                    let e2 = &edges[1];
                    if edges[0].pt_b == e2.pt_a || edges[0].pt_b == e2.pt_b {
                        (edges[0].pt_a, edges[0].pt_b) // natural direction
                    } else {
                        (edges[0].pt_b, edges[0].pt_a) // reversed
                    }
                } else {
                    (edges[0].pt_a, edges[0].pt_b)
                };

                let (first_from, mut current_exit) = first_dir;
                if let Some((x, y)) = positions.get(&first_from) {
                    write!(combined_path, "M{:.4},{:.4} ", x, svg_y(*y)).unwrap();
                }
                emit_edge_to(&edges[0], first_from, &mut combined_path, positions);

                for edge in &edges[1..] {
                    let from = if edge.pt_a == current_exit {
                        edge.pt_a
                    } else {
                        edge.pt_b
                    };
                    let exit = if from == edge.pt_a {
                        edge.pt_b
                    } else {
                        edge.pt_a
                    };
                    emit_edge_to(edge, from, &mut combined_path, positions);
                    current_exit = exit;
                }
                combined_path.push_str("Z ");
            }
        } else if !profile.vertex_ids.is_empty() {
            // No arcs — use vertex_ids for simple polygon fill
            for (i, pid) in profile.vertex_ids.iter().enumerate() {
                if let Some((x, y)) = positions.get(pid) {
                    let cmd = if i == 0 { "M" } else { "L" };
                    write!(combined_path, "{}{:.4},{:.4} ", cmd, x, svg_y(*y)).unwrap();
                }
            }
            combined_path.push_str("Z ");
        } else if !profile.entity_ids.is_empty() {
            // No arcs, no vertex_ids — chain line entities
            let mut pts: Vec<u32> = Vec::new();
            for eid in &profile.entity_ids {
                if let Some(SketchEntity::Line {
                    start_id, end_id, ..
                }) = entity_map.get(eid)
                {
                    if pts.is_empty() {
                        pts.push(*start_id);
                        pts.push(*end_id);
                    } else {
                        let last = *pts.last().unwrap();
                        if *start_id == last {
                            pts.push(*end_id);
                        } else {
                            pts.push(*start_id);
                        }
                    }
                }
            }
            for (i, pid) in pts.iter().enumerate() {
                if let Some((x, y)) = positions.get(pid) {
                    let cmd = if i == 0 { "M" } else { "L" };
                    write!(combined_path, "{}{:.4},{:.4} ", cmd, x, svg_y(*y)).unwrap();
                }
            }
            if !pts.is_empty() {
                combined_path.push_str("Z ");
            }
        }
    }

    if !combined_path.is_empty() {
        writeln!(
            svg,
            r#"<path d="{}" fill="{}" fill-opacity="0.5" fill-rule="evenodd" stroke="none"/>"#,
            combined_path.trim(),
            COLOR_PROFILE_FILL
        )
        .unwrap();
    }
}

/// Write a circle as an SVG subpath (two semicircular arcs).
fn write_circle_subpath(path: &mut String, cx: f64, cy: f64, r: f64) {
    write!(
        path,
        "M{:.4},{:.4} A{:.4},{:.4} 0 1,1 {:.4},{:.4} A{:.4},{:.4} 0 1,1 {:.4},{:.4} Z ",
        cx - r,
        cy,
        r,
        r,
        cx + r,
        cy,
        r,
        r,
        cx - r,
        cy,
    )
    .unwrap();
}

fn write_entities(
    svg: &mut String,
    entities: &[SketchEntity],
    positions: &std::collections::HashMap<u32, (f64, f64)>,
    radii: &std::collections::HashMap<u32, f64>,
    point_color: &str,
) {
    writeln!(svg, r#"<g class="entities">"#).unwrap();

    for entity in entities {
        let is_construction = entity.is_construction();
        let stroke_color = if is_construction {
            COLOR_CONSTRUCTION
        } else {
            COLOR_ENTITY
        };
        let dash_attr = if is_construction {
            r#" stroke-dasharray="3,2""#
        } else {
            ""
        };

        match entity {
            SketchEntity::Line {
                start_id, end_id, ..
            } => {
                if let (Some((x1, y1)), Some((x2, y2))) =
                    (positions.get(start_id), positions.get(end_id))
                {
                    writeln!(
                        svg,
                        r#"<line x1="{:.4}" y1="{:.4}" x2="{:.4}" y2="{:.4}" stroke="{}" stroke-width="1.5" stroke-linecap="round"{}/>"#,
                        x1,
                        svg_y(*y1),
                        x2,
                        svg_y(*y2),
                        stroke_color,
                        dash_attr,
                    )
                    .unwrap();
                }
            }
            SketchEntity::Circle {
                id,
                center_id,
                radius,
                ..
            } => {
                if let Some((cx, cy)) = positions.get(center_id) {
                    let r = radii.get(id).copied().unwrap_or(*radius);
                    writeln!(
                        svg,
                        r#"<circle cx="{:.4}" cy="{:.4}" r="{:.4}" stroke="{}" stroke-width="1.5" fill="none"{}/>"#,
                        cx,
                        svg_y(*cy),
                        r,
                        stroke_color,
                        dash_attr,
                    )
                    .unwrap();
                }
            }
            SketchEntity::Arc {
                id,
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
                    let r = radii
                        .get(id)
                        .copied()
                        .unwrap_or_else(|| ((sx - cx).powi(2) + (sy - cy).powi(2)).sqrt());
                    // Approximate arc direction: use cross product to determine sweep
                    let v1 = (sx - cx, sy - cy);
                    let v2 = (ex - cx, ey - cy);
                    let cross = v1.0 * v2.1 - v1.1 * v2.0;
                    // SVG arc: flip sweep because Y is negated
                    let sweep = if cross > 0.0 { 0 } else { 1 };
                    writeln!(
                        svg,
                        r#"<path d="M{:.4},{:.4} A{:.4},{:.4} 0 0,{} {:.4},{:.4}" stroke="{}" stroke-width="1.5" fill="none"{}/>"#,
                        sx,
                        svg_y(*sy),
                        r,
                        r,
                        sweep,
                        ex,
                        svg_y(*ey),
                        stroke_color,
                        dash_attr,
                    )
                    .unwrap();
                }
            }
            SketchEntity::Point {
                id, construction, ..
            } => {
                if let Some((x, y)) = positions.get(id) {
                    let color = if *construction {
                        COLOR_CONSTRUCTION
                    } else {
                        point_color
                    };
                    writeln!(
                        svg,
                        r#"<circle cx="{:.4}" cy="{:.4}" r="2" fill="{}"/>"#,
                        x,
                        svg_y(*y),
                        color
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
    writeln!(
        svg,
        r#"<g class="badges" font-family="sans-serif" font-size="4">"#
    )
    .unwrap();

    // Track how many badges have been placed at each approximate location,
    // so we can offset stacked badges vertically. Key = (x rounded, y rounded).
    let mut badge_counts: std::collections::HashMap<(i64, i64), usize> =
        std::collections::HashMap::new();

    let place_badge =
        |svg: &mut String,
         x: f64,
         y: f64,
         label: &str,
         counts: &mut std::collections::HashMap<(i64, i64), usize>| {
            let key = ((x * 2.0).round() as i64, (y * 2.0).round() as i64);
            let n = counts.entry(key).or_insert(0);
            let offset_y = *n as f64 * -6.5; // stack upward
            *n += 1;
            write_badge(svg, x, y + offset_y, label);
        };

    for constraint in constraints {
        match constraint {
            SketchConstraint::Horizontal { entity } => {
                if let Some((mx, my)) = line_midpoint(*entity, entities, positions) {
                    place_badge(svg, mx, svg_y(my), "H", &mut badge_counts);
                }
            }
            SketchConstraint::Vertical { entity } => {
                if let Some((mx, my)) = line_midpoint(*entity, entities, positions) {
                    place_badge(svg, mx, svg_y(my), "V", &mut badge_counts);
                }
            }
            SketchConstraint::Parallel { line_a, .. } => {
                if let Some((mx, my)) = line_midpoint(*line_a, entities, positions) {
                    place_badge(svg, mx, svg_y(my), "\u{2225}", &mut badge_counts);
                }
            }
            SketchConstraint::Perpendicular { line_a, .. } => {
                if let Some((mx, my)) = line_midpoint(*line_a, entities, positions) {
                    place_badge(svg, mx, svg_y(my), "\u{22A5}", &mut badge_counts);
                }
            }
            SketchConstraint::Distance {
                entity_a,
                entity_b,
                value,
            } => {
                // Try line midpoint first (for point-to-point on a line),
                // fall back to midpoint between two entity positions
                let pos = point_or_entity_midpoint(*entity_a, *entity_b, entities, positions);
                if let Some((mx, my)) = pos {
                    place_badge(
                        svg,
                        mx,
                        svg_y(my),
                        &format!("{:.1}", value),
                        &mut badge_counts,
                    );
                }
            }
            SketchConstraint::Radius { entity, value } => {
                if let Some((cx, cy)) = entity_center(*entity, entities, positions) {
                    place_badge(
                        svg,
                        cx + 3.0,
                        svg_y(cy),
                        &format!("R{:.1}", value),
                        &mut badge_counts,
                    );
                }
            }
            SketchConstraint::Diameter { entity, value } => {
                if let Some((cx, cy)) = entity_center(*entity, entities, positions) {
                    place_badge(
                        svg,
                        cx + 3.0,
                        svg_y(cy),
                        &format!("\u{2300}{:.1}", value),
                        &mut badge_counts,
                    );
                }
            }
            SketchConstraint::Angle {
                line_a,
                value_degrees,
                ..
            } => {
                if let Some((mx, my)) = line_midpoint(*line_a, entities, positions) {
                    place_badge(
                        svg,
                        mx,
                        svg_y(my),
                        &format!("{:.1}\u{00B0}", value_degrees),
                        &mut badge_counts,
                    );
                }
            }
            SketchConstraint::Equal { entity_a, entity_b } => {
                // Place on entity_a, offset from entity_b
                let pos = line_midpoint(*entity_a, entities, positions)
                    .or_else(|| entity_center(*entity_a, entities, positions));
                if let Some((mx, my)) = pos {
                    place_badge(svg, mx, svg_y(my), "=", &mut badge_counts);
                }
                // Also mark entity_b
                let pos_b = line_midpoint(*entity_b, entities, positions)
                    .or_else(|| entity_center(*entity_b, entities, positions));
                if let Some((mx, my)) = pos_b {
                    place_badge(svg, mx, svg_y(my), "=", &mut badge_counts);
                }
            }
            SketchConstraint::Coincident { point_a, .. } => {
                if let Some((x, y)) = positions.get(point_a) {
                    place_badge(svg, *x, svg_y(*y), "\u{25C9}", &mut badge_counts);
                }
            }
            SketchConstraint::Tangent { line, curve } => {
                // Place badge at the point where line meets curve — approximate with
                // the midpoint between line midpoint and curve center
                let lm = line_midpoint(*line, entities, positions);
                let cc = entity_center(*curve, entities, positions);
                if let (Some((lx, ly)), Some((cx, cy))) = (lm, cc) {
                    let mx = (lx + cx) / 2.0;
                    let my = (ly + cy) / 2.0;
                    place_badge(svg, mx, svg_y(my), "T", &mut badge_counts);
                }
            }
            SketchConstraint::Symmetric {
                entity_a, entity_b, ..
            } => {
                // Badge at midpoint between the two symmetric entities
                if let (Some((x1, y1)), Some((x2, y2))) =
                    (positions.get(entity_a), positions.get(entity_b))
                {
                    let mx = (x1 + x2) / 2.0;
                    let my = (y1 + y2) / 2.0;
                    place_badge(svg, mx, svg_y(my), "Sym", &mut badge_counts);
                }
            }
            SketchConstraint::SymmetricH { point_a, point_b } => {
                if let (Some((x1, y1)), Some((x2, y2))) =
                    (positions.get(point_a), positions.get(point_b))
                {
                    let mx = (x1 + x2) / 2.0;
                    let my = (y1 + y2) / 2.0;
                    place_badge(svg, mx, svg_y(my), "SymH", &mut badge_counts);
                }
            }
            SketchConstraint::SymmetricV { point_a, point_b } => {
                if let (Some((x1, y1)), Some((x2, y2))) =
                    (positions.get(point_a), positions.get(point_b))
                {
                    let mx = (x1 + x2) / 2.0;
                    let my = (y1 + y2) / 2.0;
                    place_badge(svg, mx, svg_y(my), "SymV", &mut badge_counts);
                }
            }
            SketchConstraint::Midpoint { point, .. } => {
                if let Some((x, y)) = positions.get(point) {
                    place_badge(svg, *x, svg_y(*y), "M", &mut badge_counts);
                }
            }
            SketchConstraint::OnEntity { point, .. } => {
                if let Some((x, y)) = positions.get(point) {
                    place_badge(svg, *x, svg_y(*y), "On", &mut badge_counts);
                }
            }
            SketchConstraint::EqualAngle { line_a, .. } => {
                if let Some((mx, my)) = line_midpoint(*line_a, entities, positions) {
                    place_badge(svg, mx, svg_y(my), "=\u{2220}", &mut badge_counts);
                }
            }
            SketchConstraint::Ratio {
                entity_a, value, ..
            } => {
                if let Some((mx, my)) = line_midpoint(*entity_a, entities, positions) {
                    place_badge(
                        svg,
                        mx,
                        svg_y(my),
                        &format!("{}:1", value),
                        &mut badge_counts,
                    );
                }
            }
            SketchConstraint::EqualPointToLine {
                point_a, point_b, ..
            } => {
                if let (Some((x1, y1)), Some((x2, y2))) =
                    (positions.get(point_a), positions.get(point_b))
                {
                    let mx = (x1 + x2) / 2.0;
                    let my = (y1 + y2) / 2.0;
                    place_badge(svg, mx, svg_y(my), "=d", &mut badge_counts);
                }
            }
            SketchConstraint::SameOrientation { entity_a, .. } => {
                if let Some((mx, my)) = line_midpoint(*entity_a, entities, positions) {
                    place_badge(svg, mx, svg_y(my), "\u{21C6}", &mut badge_counts);
                }
            }
            SketchConstraint::Dragged { .. } => {
                // No badge for dragged constraints
            }
        }
    }

    writeln!(svg, "</g>").unwrap();
}

/// Get midpoint between two entities — tries point positions first, falls back to
/// line midpoints or entity centers.
fn point_or_entity_midpoint(
    id_a: u32,
    id_b: u32,
    entities: &[SketchEntity],
    positions: &std::collections::HashMap<u32, (f64, f64)>,
) -> Option<(f64, f64)> {
    let pa = positions
        .get(&id_a)
        .copied()
        .or_else(|| line_midpoint(id_a, entities, positions))
        .or_else(|| entity_center(id_a, entities, positions));
    let pb = positions
        .get(&id_b)
        .copied()
        .or_else(|| line_midpoint(id_b, entities, positions))
        .or_else(|| entity_center(id_b, entities, positions));
    match (pa, pb) {
        (Some((x1, y1)), Some((x2, y2))) => Some(((x1 + x2) / 2.0, (y1 + y2) / 2.0)),
        (Some(p), None) | (None, Some(p)) => Some(p),
        _ => None,
    }
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
        r#"<text x="{:.2}" y="{:.2}" text-anchor="middle" fill="{}" font-family="sans-serif" font-size="4">{}</text>"#,
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
            SketchEntity::Circle { id, center_id, .. }
            | SketchEntity::Arc { id, center_id, .. } => {
                if *id == entity_id {
                    return positions.get(center_id).copied();
                }
            }
            _ => {}
        }
    }
    None
}
