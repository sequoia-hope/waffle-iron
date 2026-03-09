//! Assay catalog integration test.
//!
//! Runs all 400 assay cases against the real kernel and reports X/400.
//! Does NOT assert all pass — the score is the metric.

use test_harness::assay::catalog::full_catalog;
use test_harness::assay::scoring::{run_assay_kernel, run_assay_mock, AssayStatus};

#[test]
fn assay_catalog_compiles_and_counts() {
    let catalog = full_catalog();
    assert_eq!(catalog.len(), 400, "Catalog should have exactly 400 cases");
}

#[test]
#[ignore] // Takes too long for CI — run manually with --ignored
fn assay_full_kernel() {
    let catalog = full_catalog();
    let report = run_assay_kernel(&catalog);

    println!("\n{}", report.score_line());
    println!("\nFailed/errored cases:");
    for result in &report.results {
        if result.status != AssayStatus::Passed {
            println!(
                "  {} [{}] {:?}: {}",
                result.id, result.description, result.status, result.detail
            );
        }
    }
}

#[test]
fn assay_mock_smoke() {
    // Run a few cases against MockKernel to verify the harness works.
    let catalog = full_catalog();
    let first_5 = &catalog[..5];
    let report = run_assay_mock(first_5);

    println!("\nMock smoke: {}", report.score_line());
    // MockKernel should at least not panic
    assert_eq!(report.total, 5);
}
