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
    let solved = solve_sketch(sketch).expect("valid test input");
    let svg = render_sketch_svg(sketch, &solved);

    let ref_path = golden_reference_path(name);
    if !ref_path.exists() {
        let should_update = std::env::var("UPDATE_GOLDENS")
            .map(|v| !v.is_empty() && v != "0")
            .unwrap_or(false);

        if should_update {
            std::fs::write(&ref_path, &svg).unwrap_or_else(|e| {
                panic!(
                    "Golden reference missing and could not write: {}\nError: {}",
                    ref_path.display(),
                    e
                );
            });
            return;
        } else {
            panic!(
                "Golden reference missing: {}.\n\
                 Regenerate with:\n\
                   cargo run --example regenerate_golden --features render\n\
                 Or re-run tests with:\n\
                   UPDATE_GOLDENS=1 cargo test --features render --test golden_tests",
                ref_path.display(),
            );
        }
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
