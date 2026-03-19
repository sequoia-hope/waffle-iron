//! Structural SVG assertions for the render module.
//!
//! These tests verify SVG structure (element counts, attributes, colors)
//! without comparing against golden references.

#![cfg(feature = "render")]

mod fixtures;

use sketch_solver::{render_sketch_svg, solve_sketch, SolveStatus};

fn count_occurrences(haystack: &str, needle: &str) -> usize {
    haystack.matches(needle).count()
}

#[test]
fn rectangle_has_four_lines() {
    let sketch = fixtures::rectangle_sketch();
    let solved = solve_sketch(&sketch);
    let svg = render_sketch_svg(&sketch, &solved);

    // 4 entity lines (not counting grid lines)
    let entity_lines = count_occurrences(&svg, r##"stroke="#2196F3""##);
    // 4 lines in the rectangle
    assert!(
        entity_lines >= 4,
        "Expected at least 4 blue entity elements, got {}",
        entity_lines
    );
}

#[test]
fn rectangle_has_point_dots() {
    let sketch = fixtures::rectangle_sketch();
    let solved = solve_sketch(&sketch);
    let svg = render_sketch_svg(&sketch, &solved);

    // Points are rendered as filled circles with status color
    // Rectangle is fully constrained → green dots
    let green_dots = count_occurrences(&svg, r##"fill="#4CAF50""##);
    assert!(
        green_dots >= 4,
        "Expected at least 4 green point dots, got {}",
        green_dots
    );
}

#[test]
fn rectangle_has_constraint_badges() {
    let sketch = fixtures::rectangle_sketch();
    let solved = solve_sketch(&sketch);
    let svg = render_sketch_svg(&sketch, &solved);

    // Should have H and V badges
    assert!(svg.contains(">H<"), "Missing horizontal badge");
    assert!(svg.contains(">V<"), "Missing vertical badge");
}

#[test]
fn rectangle_has_grid() {
    let sketch = fixtures::rectangle_sketch();
    let solved = solve_sketch(&sketch);
    let svg = render_sketch_svg(&sketch, &solved);

    assert!(svg.contains("grid-minor"), "Missing minor grid");
    assert!(svg.contains("grid-major"), "Missing major grid");
}

#[test]
fn circle_has_circle_element() {
    let sketch = fixtures::circle_sketch();
    let solved = solve_sketch(&sketch);
    let svg = render_sketch_svg(&sketch, &solved);

    // Should contain a circle element with blue stroke
    assert!(
        svg.contains("<circle") && svg.contains(r##"stroke="#2196F3""##),
        "Missing blue circle element"
    );
}

#[test]
fn underconstrained_has_amber_dots() {
    let sketch = fixtures::underconstrained_sketch();
    let solved = solve_sketch(&sketch);
    let svg = render_sketch_svg(&sketch, &solved);

    assert!(
        matches!(solved.status, SolveStatus::UnderConstrained { .. }),
        "Expected under-constrained status"
    );

    let amber_dots = count_occurrences(&svg, r##"fill="#FF9800""##);
    assert!(
        amber_dots >= 3,
        "Expected at least 3 amber dots for under-constrained, got {}",
        amber_dots
    );
}

#[test]
fn overconstrained_has_red_dots() {
    let sketch = fixtures::overconstrained_sketch();
    let solved = solve_sketch(&sketch);
    let svg = render_sketch_svg(&sketch, &solved);

    // Over-constrained or solve-failed → red dots
    assert!(
        matches!(
            solved.status,
            SolveStatus::OverConstrained { .. } | SolveStatus::SolveFailed { .. }
        ),
        "Expected over-constrained or solve-failed status, got {:?}",
        solved.status
    );

    let red_dots = count_occurrences(&svg, r##"fill="#F44336""##);
    assert!(
        red_dots >= 2,
        "Expected at least 2 red dots for over-constrained, got {}",
        red_dots
    );
}

#[test]
fn svg_is_valid_xml() {
    for (name, sketch) in fixtures::all_fixtures() {
        let solved = solve_sketch(&sketch);
        let svg = render_sketch_svg(&sketch, &solved);
        assert!(
            svg.starts_with("<svg"),
            "Fixture '{name}' SVG doesn't start with <svg"
        );
        assert!(
            svg.trim_end().ends_with("</svg>"),
            "Fixture '{name}' SVG doesn't end with </svg>"
        );
    }
}

#[test]
fn svg_has_viewbox() {
    for (name, sketch) in fixtures::all_fixtures() {
        let solved = solve_sketch(&sketch);
        let svg = render_sketch_svg(&sketch, &solved);
        assert!(
            svg.contains("viewBox"),
            "Fixture '{name}' SVG missing viewBox"
        );
    }
}

#[test]
fn all_fixtures_render_without_panic() {
    for (name, sketch) in fixtures::all_fixtures() {
        let solved = solve_sketch(&sketch);
        let svg = render_sketch_svg(&sketch, &solved);
        assert!(
            !svg.is_empty(),
            "Fixture '{name}' produced empty SVG"
        );
    }
}
