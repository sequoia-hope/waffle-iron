use std::path::Path;

use test_harness::assay::randomized_runner::{
    build_catalog, catalog_summary, discover_cases, generate_catalog_markdown,
    run_randomized_assay, write_results_json,
};
use test_harness::assay::scoring::AssayStatus;

const ASSAY_DIR: &str = "../../app/tests/cases/assay";

#[test]
fn randomized_corpus_discovery() {
    let dir = Path::new(ASSAY_DIR);
    if !dir.exists() {
        eprintln!("Assay corpus not generated yet — skipping discovery test");
        return;
    }
    let manifest = dir.join("manifest.json");
    assert!(manifest.exists(), "manifest.json should exist in assay dir");
    let cases = discover_cases(dir);
    assert!(!cases.is_empty(), "corpus should contain at least one case");
}

#[test]
fn randomized_assay_mock_smoke() {
    let dir = Path::new(ASSAY_DIR);
    if !dir.exists() {
        eprintln!("Assay corpus not generated — skipping mock smoke test");
        return;
    }
    let report = run_randomized_assay(dir, false);
    println!("\nMock smoke: {}", report.score_line());
    for r in &report.results {
        if r.status != AssayStatus::Passed {
            println!("  {} {:?}: {}", r.id, r.status, r.detail);
        }
    }
    assert!(report.total > 0, "should have at least some test cases");
}

#[test]
#[ignore]
fn randomized_assay_full_kernel() {
    let dir = Path::new(ASSAY_DIR);
    if !dir.exists() {
        eprintln!("Assay corpus not generated yet");
        return;
    }
    let report = run_randomized_assay(dir, true);
    let catalog = build_catalog(dir, &report);

    // Print summary
    println!("\n{}", catalog_summary(&report, &catalog));

    // Print individual failures
    for r in &report.results {
        if r.status != AssayStatus::Passed {
            println!("  {} {:?}: {}", r.id, r.status, r.detail);
        }
    }
}

/// Generate the full catalog markdown and write to ASSAY_CATALOG.md.
///
/// Run with: cargo test -p test-harness --test assay_randomized -- generate_catalog --ignored --nocapture
#[test]
#[ignore]
fn generate_catalog() {
    let dir = Path::new(ASSAY_DIR);
    if !dir.exists() {
        eprintln!("Assay corpus not generated yet");
        return;
    }
    let report = run_randomized_assay(dir, true);
    let catalog = build_catalog(dir, &report);
    let markdown = generate_catalog_markdown(&report, &catalog);

    // Write to crates/test-harness/ASSAY_CATALOG.md
    let catalog_path = Path::new("ASSAY_CATALOG.md");
    std::fs::write(catalog_path, &markdown).expect("failed to write ASSAY_CATALOG.md");

    // Write results.json to assay dir for GUI consumption
    write_results_json(dir, &catalog);

    println!("\nWrote ASSAY_CATALOG.md + results.json");
    println!("{}", catalog_summary(&report, &catalog));
}
