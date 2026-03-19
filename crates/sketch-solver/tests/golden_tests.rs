//! Golden SVG comparison tests.
//!
//! Each test solves a fixture sketch, renders SVG, and compares against
//! the checked-in reference file. If a reference doesn't exist yet,
//! the test prints instructions to regenerate.

#![cfg(feature = "render")]
#![allow(dead_code)]

mod fixtures;

use sketch_solver::{render_sketch_svg, solve_sketch};

fn golden_reference_path(name: &str) -> std::path::PathBuf {
    let manifest = std::path::Path::new(env!("CARGO_MANIFEST_DIR"));
    manifest
        .join("tests")
        .join("golden")
        .join("reference")
        .join(format!("{name}.svg"))
}

fn assert_golden(name: &str, sketch: &sketch_solver::Sketch) {
    let solved = solve_sketch(sketch);
    let svg = render_sketch_svg(sketch, &solved);

    let ref_path = golden_reference_path(name);
    if !ref_path.exists() {
        // No reference yet — write it and pass (first run)
        std::fs::write(&ref_path, &svg).unwrap_or_else(|e| {
            panic!(
                "Golden reference missing and could not write: {}\n\
                 Run: cargo run --example regenerate_golden --features render\n\
                 Error: {}",
                ref_path.display(),
                e
            );
        });
        return;
    }

    let reference = std::fs::read_to_string(&ref_path).unwrap_or_else(|e| {
        panic!(
            "Failed to read golden reference {}: {}",
            ref_path.display(),
            e
        )
    });

    if svg != reference {
        // Write actual output for diffing
        let actual_path = ref_path.with_extension("actual.svg");
        let _ = std::fs::write(&actual_path, &svg);
        panic!(
            "Golden SVG mismatch for '{name}'.\n\
             Reference: {}\n\
             Actual:    {}\n\
             Regenerate with: cargo run --example regenerate_golden --features render",
            ref_path.display(),
            actual_path.display(),
        );
    }
}

#[test]
fn golden_rectangle() {
    assert_golden("rectangle", &fixtures::rectangle_sketch());
}

#[test]
fn golden_circle() {
    assert_golden("circle", &fixtures::circle_sketch());
}

#[test]
fn golden_triangle() {
    assert_golden("triangle", &fixtures::triangle_sketch());
}

#[test]
fn golden_bracket() {
    assert_golden("bracket", &fixtures::bracket_sketch());
}

#[test]
fn golden_underconstrained() {
    assert_golden("underconstrained", &fixtures::underconstrained_sketch());
}

#[test]
fn golden_overconstrained() {
    assert_golden("overconstrained", &fixtures::overconstrained_sketch());
}
