//! Randomized test case runner for Assay v3.
//!
//! Discovers `.waffle` + `.meta.json` pairs on disk, loads each through the
//! full `LoadProject` dispatch path (same as File > Open in the GUI), then
//! runs oracle checks on the resulting geometry.

use std::fs;
use std::path::{Path, PathBuf};
use std::time::Instant;

use crate::assay::gen::{AssayMeta, CorpusManifest};
use crate::assay::scoring::{AssayReport, AssayResult, AssayStatus};
use crate::helpers::{mesh_bounding_box, mesh_volume};
use crate::oracle::run_all_mesh_checks;
use crate::workflow::ModelBuilder;

/// A discovered test case on disk.
#[derive(Debug)]
pub struct DiscoveredCase {
    pub id: String,
    pub waffle_path: PathBuf,
    pub meta_path: PathBuf,
}

/// Discover test cases by reading the manifest.json in `dir`.
pub fn discover_cases(dir: &Path) -> Vec<DiscoveredCase> {
    let manifest_path = dir.join("manifest.json");
    let manifest_json = match fs::read_to_string(&manifest_path) {
        Ok(s) => s,
        Err(e) => {
            eprintln!("Cannot read manifest at {}: {}", manifest_path.display(), e);
            return Vec::new();
        }
    };

    let manifest: CorpusManifest = match serde_json::from_str(&manifest_json) {
        Ok(m) => m,
        Err(e) => {
            eprintln!("Cannot parse manifest: {}", e);
            return Vec::new();
        }
    };

    manifest
        .cases
        .iter()
        .map(|entry| DiscoveredCase {
            id: entry.id.clone(),
            waffle_path: dir.join(&entry.filename),
            meta_path: dir.join(&entry.meta_filename),
        })
        .collect()
}

/// Run the randomized assay on all discovered cases in `dir`.
///
/// When `use_kernel` is true, uses `WaffleKernel` (real geometry);
/// when false, uses `MockKernel` (fast, deterministic).
pub fn run_randomized_assay(dir: &Path, use_kernel: bool) -> AssayReport {
    let cases = discover_cases(dir);
    let start = Instant::now();
    let mut results = Vec::new();
    let mut passed = 0usize;
    let mut failed = 0usize;
    let mut errored = 0usize;

    for case in &cases {
        let result = replay_and_validate(case, use_kernel);
        match result.status {
            AssayStatus::Passed => passed += 1,
            AssayStatus::Failed => failed += 1,
            AssayStatus::Errored => errored += 1,
        }
        results.push(result);
    }

    AssayReport {
        total: results.len(),
        passed,
        failed,
        errored,
        results,
        total_duration: start.elapsed(),
    }
}

/// Replay a single test case and validate against oracle expectations.
fn replay_and_validate(case: &DiscoveredCase, use_kernel: bool) -> AssayResult {
    let case_start = Instant::now();

    // 1. Read .waffle JSON
    let waffle_json = match fs::read_to_string(&case.waffle_path) {
        Ok(s) => s,
        Err(e) => {
            return AssayResult {
                id: case.id.clone(),
                description: String::new(),
                status: AssayStatus::Errored,
                duration: case_start.elapsed(),
                detail: format!("cannot read .waffle: {}", e),
            };
        }
    };

    // 2. Read .meta.json
    let meta: AssayMeta = match fs::read_to_string(&case.meta_path)
        .map_err(|e| e.to_string())
        .and_then(|s| serde_json::from_str(&s).map_err(|e| e.to_string()))
    {
        Ok(m) => m,
        Err(e) => {
            return AssayResult {
                id: case.id.clone(),
                description: String::new(),
                status: AssayStatus::Errored,
                duration: case_start.elapsed(),
                detail: format!("cannot read .meta.json: {}", e),
            };
        }
    };

    // 3. Create builder (mock or kernel)
    let mut builder = if use_kernel {
        ModelBuilder::kernel()
    } else {
        ModelBuilder::mock()
    };

    // 4. Load through full LoadProject path
    if let Err(e) = builder.load(&waffle_json) {
        return AssayResult {
            id: case.id.clone(),
            description: meta.description.clone(),
            status: AssayStatus::Errored,
            duration: case_start.elapsed(),
            detail: format!("LoadProject failed: {}", e),
        };
    }

    // 5. Tessellate last feature
    let mesh = match builder.tessellate_last() {
        Ok(m) => m,
        Err(e) => {
            return AssayResult {
                id: case.id.clone(),
                description: meta.description.clone(),
                status: AssayStatus::Errored,
                duration: case_start.elapsed(),
                detail: format!("tessellation failed: {}", e),
            };
        }
    };

    // 6. Run mesh oracle checks
    let verdicts = run_all_mesh_checks(&mesh);

    // 7. Additional checks from meta expectations
    let mut failures: Vec<String> = Vec::new();

    // Collect mesh oracle failures
    for v in &verdicts {
        if !v.passed {
            failures.push(format!("{}: {}", v.oracle_name, v.detail));
        }
    }

    // Non-empty mesh check
    if mesh.indices.is_empty() {
        failures.push("empty mesh: no triangles".to_string());
    }

    // Volume positivity check
    if meta.oracles.expect_positive_volume {
        let vol = mesh_volume(&mesh);
        if vol <= 0.0 {
            failures.push(format!("expected positive volume, got {:.6e}", vol));
        }
    }

    // Bounding box extent check
    if !mesh.vertices.is_empty() {
        let (bb_min, bb_max) = mesh_bounding_box(&mesh);
        let dx = (bb_max[0] - bb_min[0]) as f64;
        let dy = (bb_max[1] - bb_min[1]) as f64;
        let dz = (bb_max[2] - bb_min[2]) as f64;
        let diagonal = (dx * dx + dy * dy + dz * dz).sqrt();
        let max_extent = meta.oracles.max_bbox_extent;
        if diagonal > max_extent {
            failures.push(format!(
                "bbox diagonal {:.3e} exceeds max {:.3e}",
                diagonal, max_extent
            ));
        }
    }

    // 8. Aggregate
    let duration = case_start.elapsed();
    if failures.is_empty() {
        AssayResult {
            id: case.id.clone(),
            description: meta.description.clone(),
            status: AssayStatus::Passed,
            duration,
            detail: format!("{} oracles passed", verdicts.len()),
        }
    } else {
        AssayResult {
            id: case.id.clone(),
            description: meta.description.clone(),
            status: AssayStatus::Failed,
            duration,
            detail: failures.join("; "),
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::assay::gen::{generate_corpus, CorpusConfig};
    use tempfile::TempDir;

    fn generate_test_corpus(count: usize) -> (TempDir, PathBuf) {
        let dir = TempDir::new().expect("create temp dir");
        let out = dir.path().join("assay");
        let config = CorpusConfig {
            master_seed: 12345,
            case_count: count,
            output_dir: out.clone(),
        };
        generate_corpus(&config);
        (dir, out)
    }

    #[test]
    fn discover_cases_from_corpus() {
        let (_dir, corpus_path) = generate_test_corpus(5);
        let cases = discover_cases(&corpus_path);
        assert_eq!(cases.len(), 5);
        assert_eq!(cases[0].id, "R0001");
        assert!(cases[0].waffle_path.exists());
        assert!(cases[0].meta_path.exists());
    }

    #[test]
    fn discover_cases_missing_dir() {
        let cases = discover_cases(Path::new("/nonexistent/path"));
        assert!(cases.is_empty());
    }

    #[test]
    fn run_mock_smoke() {
        let (_dir, corpus_path) = generate_test_corpus(3);
        let report = run_randomized_assay(&corpus_path, false);
        assert_eq!(report.total, 3);
        // Mock kernel produces deterministic results — all should complete (pass or fail, not error)
        assert_eq!(
            report.errored,
            0,
            "mock kernel should not produce errors: {:?}",
            report
                .results
                .iter()
                .filter(|r| r.status == AssayStatus::Errored)
                .map(|r| &r.detail)
                .collect::<Vec<_>>()
        );
    }
}
