use std::path::Path;

use test_harness::assay::randomized_runner::{discover_cases, run_randomized_assay};
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
    println!("\n{}", report.score_line());
    for r in &report.results {
        if r.status != AssayStatus::Passed {
            println!("  {} {:?}: {}", r.id, r.status, r.detail);
        }
    }
}
