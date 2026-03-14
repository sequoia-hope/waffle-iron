//! Randomized test case runner for Assay v3.
//!
//! Discovers `.waffle` + `.meta.json` pairs on disk, loads each through the
//! full `LoadProject` dispatch path (same as File > Open in the GUI), then
//! runs oracle checks on the resulting geometry.

use std::collections::HashMap;
use std::fmt::Write as FmtWrite;
use std::fs;
use std::path::{Path, PathBuf};
use std::time::Instant;

use crate::assay::gen::{AssayMeta, CorpusManifest};
use crate::assay::scoring::{AssayReport, AssayResult, AssayStatus};
use crate::helpers::{mesh_bounding_box, mesh_signed_volume};
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

    for (idx, case) in cases.iter().enumerate() {
        eprint!("  [{}/{}] {} ... ", idx + 1, cases.len(), case.id);
        let case_start = std::time::Instant::now();

        // Run with a per-case timeout to prevent hangs on slow boolean cases
        let case_id = case.id.clone();
        let result = {
            let case_ref = case;
            let (tx, rx) = std::sync::mpsc::channel();
            let case_path = case_ref.waffle_path.clone();
            let meta_path = case_ref.meta_path.clone();
            let id = case_ref.id.clone();
            let use_k = use_kernel;
            let handle = std::thread::spawn(move || {
                let c = DiscoveredCase {
                    id,
                    waffle_path: case_path,
                    meta_path,
                };
                let r = replay_and_validate(&c, use_k);
                let _ = tx.send(r);
            });
            match rx.recv_timeout(std::time::Duration::from_secs(90)) {
                Ok(r) => {
                    let _ = handle.join();
                    r
                }
                Err(_) => {
                    // Timeout — don't join (let thread die), report as errored
                    AssayResult {
                        id: case_id.clone(),
                        description: String::new(),
                        status: AssayStatus::Errored,
                        duration: case_start.elapsed(),
                        detail: "timeout after 90s".to_string(),
                    }
                }
            }
        };
        let elapsed = case_start.elapsed();
        eprintln!("{:?} ({:.1}s)", result.status, elapsed.as_secs_f64());
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

    // 4b. Check engine errors after load
    let engine_errors = builder.engine_errors().to_vec();
    let error_msgs: Vec<String> = engine_errors
        .iter()
        .map(|(id, msg)| format!("{}: {}", id, msg))
        .collect();

    // 4c. Feature count validation
    let expected_feature_count = meta.operations.len() * 2; // each op = sketch + operation
    let actual_feature_count = builder.feature_count();
    let feature_count_mismatch = if actual_feature_count != expected_feature_count {
        Some(format!(
            "feature count mismatch: expected {} ({}×2), got {}",
            expected_feature_count,
            meta.operations.len(),
            actual_feature_count
        ))
    } else {
        None
    };

    // 5. Tessellate last feature
    let mesh = match builder.tessellate_last() {
        Ok(m) => m,
        Err(_e) => {
            // Enrich "no solid" with actual engine errors
            let detail = if engine_errors.is_empty() {
                "no active features with solids (no engine errors recorded)".to_string()
            } else {
                format!(
                    "no solid — {} engine error(s): {}",
                    engine_errors.len(),
                    error_msgs.join("; ")
                )
            };
            return AssayResult {
                id: case.id.clone(),
                description: meta.description.clone(),
                status: AssayStatus::Errored,
                duration: case_start.elapsed(),
                detail,
            };
        }
    };

    // 6. Run mesh oracle checks
    let verdicts = run_all_mesh_checks(&mesh);

    // 7. Additional checks from meta expectations
    let mut failures: Vec<String> = Vec::new();

    // Check for auto-union warnings
    let auto_union_warnings: Vec<String> = builder
        .engine_warnings()
        .iter()
        .filter(|w| w.contains("Auto-union failed"))
        .cloned()
        .collect();
    if !auto_union_warnings.is_empty() {
        failures.push(format!(
            "auto-union-failed ({} warning(s)): {}",
            auto_union_warnings.len(),
            auto_union_warnings.join("; ")
        ));
    }

    // Report partial engine errors (some ops failed but a solid exists)
    if !engine_errors.is_empty() {
        failures.push(format!(
            "partial rebuild ({} error(s)): {}",
            engine_errors.len(),
            error_msgs.join("; ")
        ));
    }

    // Report feature count mismatch
    if let Some(msg) = feature_count_mismatch {
        failures.push(msg);
    }

    // Check: multi-operation boss cases should produce a single merged solid.
    // If merge=true on N boss operations but the result is N separate solids,
    // the merges failed silently.
    let n_ops = meta.operations.len();
    if n_ops > 1 {
        let solid_count = builder.distinct_solid_count();
        if solid_count > 1 {
            failures.push(format!(
                "merge incomplete: {} operations produced {} separate solids (expected 1 merged)",
                n_ops, solid_count
            ));
        }
    }

    // AABB-collapse check for gear profiles (rects legitimately produce boxes)
    let has_gear_profile = meta.operations.iter().any(|op| op.profile_type == "gear");
    if has_gear_profile && !mesh.vertices.is_empty() {
        let verdict = crate::oracle::check_aabb_collapse(&mesh);
        if !verdict.passed {
            failures.push(format!("aabb_collapse: {}", verdict.detail));
        }
    }

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

    // Volume positivity check (use signed volume to catch inverted winding)
    if meta.oracles.expect_positive_volume {
        let vol = mesh_signed_volume(&mesh);
        if vol <= 0.0 {
            failures.push(format!("expected positive signed volume, got {:.6e}", vol));
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

// ── Failure Categorization ────────────────────────────────────────────────

/// Root cause category for a test case result.
#[derive(Debug, Clone, PartialEq, Eq, Hash)]
pub enum FailureCategory {
    /// Auto-union of multiple boss features failed (warning from rebuild).
    AutoUnionFailed,
    /// All operations failed to produce a solid (typically revolve not supported).
    RevolveNotSupported,
    /// Revolve tessellation produces reversed normals on caps.
    RevolveNormals,
    /// Boolean face stitching leaves unpaired edges.
    BooleanWatertight,
    /// Boolean result has inconsistent normals (inward on one half).
    BooleanNormals,
    /// Degenerate triangles at revolve seams or boolean edges.
    TessellationDegenerate,
    /// Boolean operation not supported for the given geometry combo.
    BooleanNotSupported,
    /// Cascading failure: first op failed, subsequent cuts can't find a body.
    CascadingFailure,
    /// Multiple distinct failure modes.
    MultipleFailures,
    /// Passes with meaningful multi-op coverage.
    PassGenuine,
    /// Passes but only exercises trivial single-boss paths (no cuts/booleans).
    PassBossOnly,
    /// Multi-operation case where merge failed silently — separate solids remain.
    MergeIncomplete,
    /// Mesh collapsed to its AABB — non-rectangular geometry replaced by bounding box.
    AabbCollapse,
}

impl std::fmt::Display for FailureCategory {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            Self::AutoUnionFailed => write!(f, "auto-union-failed"),
            Self::RevolveNotSupported => write!(f, "revolve-not-supported"),
            Self::RevolveNormals => write!(f, "revolve-normals"),
            Self::BooleanWatertight => write!(f, "boolean-watertight"),
            Self::BooleanNormals => write!(f, "boolean-normals"),
            Self::TessellationDegenerate => write!(f, "tessellation-degenerate"),
            Self::BooleanNotSupported => write!(f, "boolean-not-supported"),
            Self::CascadingFailure => write!(f, "cascading-failure"),
            Self::MultipleFailures => write!(f, "multiple-failures"),
            Self::PassGenuine => write!(f, "pass-genuine"),
            Self::PassBossOnly => write!(f, "pass-boss-only"),
            Self::MergeIncomplete => write!(f, "merge-incomplete"),
            Self::AabbCollapse => write!(f, "aabb-collapse"),
        }
    }
}

/// A catalog entry combining result + meta + categorization.
pub struct CatalogEntry {
    pub id: String,
    pub status: AssayStatus,
    pub category: FailureCategory,
    pub meta: AssayMeta,
    pub detail: String,
}

/// Categorize a test result based on its detail string and metadata.
///
/// When multiple failure modes co-occur, classifies by the *primary* (most
/// impactful) root cause. Priority order: engine errors > mesh failures.
pub fn categorize_result(result: &AssayResult, meta: &AssayMeta) -> FailureCategory {
    let detail = &result.detail;

    match result.status {
        AssayStatus::Passed => {
            let has_cut = meta.operations.iter().any(|op| op.is_cut);
            if has_cut {
                FailureCategory::PassGenuine
            } else {
                FailureCategory::PassBossOnly
            }
        }
        AssayStatus::Errored => {
            if detail.contains("revolve: circle profile (torus)")
                || detail.contains("revolve: profile edge neither radial nor axial")
            {
                if detail.contains("Cut extrude requires an existing body") {
                    FailureCategory::CascadingFailure
                } else {
                    FailureCategory::RevolveNotSupported
                }
            } else {
                FailureCategory::CascadingFailure
            }
        }
        AssayStatus::Failed => {
            let has_auto_union_failed = detail.contains("auto-union-failed");
            let has_normals_fail = detail.contains("consistent_normals:");
            let has_outward_fail = detail.contains("outward_normals:");
            let has_watertight_fail = detail.contains("watertight_mesh:");
            let has_degenerate_fail = detail.contains("no_degenerate_triangles:");
            let has_boolean_not_supported = detail.contains("cylinder minus box")
                || detail.contains("partial box-cylinder subtract")
                || detail.contains("boolean on revolve solids")
                || detail.contains("tool encloses or equals blank");
            let has_boolean_manifold = detail.contains("non-manifold result:");
            let has_revolve_not_supported = detail.contains("revolve: circle profile (torus)")
                || detail.contains("revolve: profile edge neither radial nor axial");

            // Classify by primary root cause (most upstream issue).
            //
            // Engine-level errors take priority over mesh-level oracle failures,
            // because the mesh failures are downstream symptoms.

            // 0. Auto-union failed (highest priority — root cause when co-occurring with mesh checks)
            if has_auto_union_failed {
                return FailureCategory::AutoUnionFailed;
            }

            // 0b. Merge incomplete (multi-op case where merge failed silently)
            if detail.contains("merge incomplete:") {
                return FailureCategory::MergeIncomplete;
            }

            // 0c. AABB collapse (geometry degenerated to bounding box)
            if detail.contains("aabb_collapse:") {
                return FailureCategory::AabbCollapse;
            }

            // 1. Boolean not supported (engine-level: geometry combo not implemented)
            if has_boolean_not_supported
                && !has_boolean_manifold
                && !has_revolve_not_supported
                && !has_normals_fail
                && !has_watertight_fail
            {
                return FailureCategory::BooleanNotSupported;
            }

            // 2. Revolve not supported (partial rebuild, only error is revolve)
            if has_revolve_not_supported
                && !has_boolean_manifold
                && !has_normals_fail
                && !has_watertight_fail
            {
                return FailureCategory::RevolveNotSupported;
            }

            // 3. Boolean manifold failure (non-manifold result from boolean engine)
            //    — these often co-occur with watertight failures (downstream effect)
            if has_boolean_manifold {
                return FailureCategory::BooleanWatertight;
            }

            // 4. Watertight mesh failure without boolean manifold error
            //    — likely revolve seam issues or face stitching
            if has_watertight_fail && has_normals_fail {
                // Both watertight + normals → the watertight issue is more fundamental
                return FailureCategory::BooleanWatertight;
            }
            if has_watertight_fail && has_outward_fail {
                return FailureCategory::BooleanNormals;
            }
            if has_watertight_fail {
                return FailureCategory::BooleanWatertight;
            }

            // 5. Normal consistency issues (consistent_normals or outward_normals)
            //    — most commonly from revolve cap winding asymmetry
            if has_normals_fail || has_outward_fail {
                // If this case has a revolve-not-supported partial error too,
                // the normals issue is on the remaining (non-revolve) solid
                if has_revolve_not_supported || has_boolean_not_supported {
                    return FailureCategory::RevolveNormals;
                }
                return FailureCategory::RevolveNormals;
            }

            // 6. Degenerate triangles only
            if has_degenerate_fail {
                return FailureCategory::TessellationDegenerate;
            }

            // Fallback
            FailureCategory::MultipleFailures
        }
    }
}

/// Build a catalog from an assay run, reading metadata from disk.
pub fn build_catalog(dir: &Path, report: &AssayReport) -> Vec<CatalogEntry> {
    let cases = discover_cases(dir);
    let mut entries = Vec::new();

    for result in &report.results {
        let case = cases.iter().find(|c| c.id == result.id);
        let meta: AssayMeta = case
            .and_then(|c| {
                fs::read_to_string(&c.meta_path)
                    .ok()
                    .and_then(|s| serde_json::from_str(&s).ok())
            })
            .unwrap_or_else(|| AssayMeta {
                id: result.id.clone(),
                description: result.description.clone(),
                master_seed: 0,
                test_seed: 0,
                scale: 0.0,
                log_scale: 0.0,
                plane_origin: [0.0; 3],
                plane_normal: [0.0; 3],
                operations: vec![],
                oracles: crate::assay::gen::OracleExpectations {
                    euler_target: 2,
                    expect_watertight: true,
                    max_bbox_extent: 0.0,
                    expect_positive_volume: true,
                    volume_monotonicity: vec![],
                },
                generator_version: 0,
                featured: false,
            });

        let category = categorize_result(result, &meta);

        entries.push(CatalogEntry {
            id: result.id.clone(),
            status: result.status,
            category,
            meta,
            detail: result.detail.clone(),
        });
    }

    entries
}

/// Generate a summary statistics report from a catalog.
pub fn catalog_summary(report: &AssayReport, catalog: &[CatalogEntry]) -> String {
    let mut out = String::new();

    writeln!(out, "ASSAY v3 FAILURE CATALOG — WaffleKernel (2026-03-10)").unwrap();
    writeln!(
        out,
        "Score: {}/{} ({} pass, {} fail, {} error) in {:.1}s\n",
        report.passed,
        report.total,
        report.passed,
        report.failed,
        report.errored,
        report.total_duration.as_secs_f64()
    )
    .unwrap();

    // Count by category
    let mut category_counts: HashMap<String, (usize, &str)> = HashMap::new();
    for entry in catalog {
        let status_label = match entry.status {
            AssayStatus::Passed => "passed",
            AssayStatus::Failed => "failed",
            AssayStatus::Errored => "errored",
        };
        let counter = category_counts
            .entry(entry.category.to_string())
            .or_insert((0, status_label));
        counter.0 += 1;
    }

    writeln!(out, "By Root Cause:").unwrap();
    // Sort categories for deterministic output
    let mut cats: Vec<_> = category_counts.iter().collect();
    cats.sort_by_key(|(name, _)| (*name).clone());
    for (cat, (count, status)) in &cats {
        writeln!(out, "  {:<30} {:>3} {}", cat, count, status).unwrap();
    }

    // Highest-leverage fixes
    writeln!(out, "\nHighest-Leverage Fixes:").unwrap();
    let mut fixes: Vec<_> = cats
        .iter()
        .filter(|(_, (_, status))| *status != "passed")
        .map(|(cat, (count, _))| ((*cat).clone(), *count))
        .collect();
    fixes.sort_by(|a, b| b.1.cmp(&a.1));
    for (i, (cat, count)) in fixes.iter().enumerate() {
        writeln!(
            out,
            "  {}. Fix {} → would address ~{} cases",
            i + 1,
            cat,
            count
        )
        .unwrap();
    }

    out
}

/// Generate a full markdown catalog document.
pub fn generate_catalog_markdown(report: &AssayReport, catalog: &[CatalogEntry]) -> String {
    let mut out = String::new();

    writeln!(out, "# ASSAY v3 Failure Catalog — WaffleKernel").unwrap();
    writeln!(out).unwrap();
    writeln!(out, "Generated: 2026-03-10").unwrap();
    writeln!(
        out,
        "Score: **{}/{}** ({} pass, {} fail, {} error)",
        report.passed, report.total, report.passed, report.failed, report.errored
    )
    .unwrap();
    writeln!(out).unwrap();

    // Summary table
    writeln!(out, "## Summary by Root Cause").unwrap();
    writeln!(out).unwrap();
    writeln!(out, "| Category | Count | Status |").unwrap();
    writeln!(out, "|---|---|---|").unwrap();

    let mut category_counts: HashMap<String, (usize, String)> = HashMap::new();
    for entry in catalog {
        let status_label = match entry.status {
            AssayStatus::Passed => "passed",
            AssayStatus::Failed => "failed",
            AssayStatus::Errored => "errored",
        };
        let counter = category_counts
            .entry(entry.category.to_string())
            .or_insert((0, status_label.to_string()));
        counter.0 += 1;
    }
    let mut cats: Vec<_> = category_counts.iter().collect();
    cats.sort_by(|a, b| b.1 .0.cmp(&a.1 .0));
    for (cat, (count, status)) in &cats {
        writeln!(out, "| {} | {} | {} |", cat, count, status).unwrap();
    }

    // Highest-leverage fixes
    writeln!(out).unwrap();
    writeln!(out, "## Highest-Leverage Fixes").unwrap();
    writeln!(out).unwrap();
    let mut fixes: Vec<_> = cats
        .iter()
        .filter(|(_, (_, status))| status != "passed")
        .map(|(cat, (count, _))| ((*cat).clone(), *count))
        .collect();
    fixes.sort_by(|a, b| b.1.cmp(&a.1));
    for (i, (cat, count)) in fixes.iter().enumerate() {
        writeln!(
            out,
            "{}. **Fix {}** → would address ~{} cases",
            i + 1,
            cat,
            count
        )
        .unwrap();
    }

    // Individual case entries
    writeln!(out).unwrap();
    writeln!(out, "## Individual Case Results").unwrap();

    for entry in catalog {
        writeln!(out).unwrap();
        let status_str = match entry.status {
            AssayStatus::Passed => "PASS",
            AssayStatus::Failed => "FAIL",
            AssayStatus::Errored => "ERROR",
        };
        writeln!(out, "### {} — {}", entry.id, status_str).unwrap();
        writeln!(out).unwrap();

        // Operations
        let ops: Vec<String> = entry
            .meta
            .operations
            .iter()
            .map(|o| {
                format!(
                    "{}({},{})",
                    o.kind,
                    o.profile_type,
                    if o.is_cut { "cut" } else { "boss" }
                )
            })
            .collect();
        writeln!(out, "- **Operations**: {}", ops.join(" + ")).unwrap();
        writeln!(
            out,
            "- **Scale**: {:.2e} (log: {:.2})",
            entry.meta.scale, entry.meta.log_scale
        )
        .unwrap();
        writeln!(out, "- **Category**: {}", entry.category).unwrap();
        writeln!(out, "- **Detail**: {}", entry.detail).unwrap();
    }

    out
}

/// Write a machine-readable `results.json` to the assay corpus directory.
///
/// This JSON is consumed by the Assay Browser GUI to show pass/fail status.
pub fn write_results_json(dir: &Path, catalog: &[CatalogEntry]) {
    let entries: Vec<serde_json::Value> = catalog
        .iter()
        .map(|e| {
            let status = match e.status {
                AssayStatus::Passed => "pass",
                AssayStatus::Failed => "fail",
                AssayStatus::Errored => "error",
            };
            serde_json::json!({
                "id": e.id,
                "status": status,
                "category": e.category.to_string(),
                "detail": e.detail,
            })
        })
        .collect();

    let json = serde_json::json!({
        "generated": "2026-03-10",
        "total": catalog.len(),
        "passed": catalog.iter().filter(|e| e.status == AssayStatus::Passed).count(),
        "failed": catalog.iter().filter(|e| e.status == AssayStatus::Failed).count(),
        "errored": catalog.iter().filter(|e| e.status == AssayStatus::Errored).count(),
        "results": entries,
    });

    let results_path = dir.join("results.json");
    fs::write(
        &results_path,
        serde_json::to_string_pretty(&json).expect("serialize results"),
    )
    .unwrap_or_else(|e| {
        eprintln!(
            "Failed to write results.json to {}: {}",
            results_path.display(),
            e
        );
    });
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
        assert_eq!(cases.len(), 15); // 5 random + 10 featured
        assert_eq!(cases[0].id, "R0001");
        assert!(cases[0].waffle_path.exists());
        assert!(cases[0].meta_path.exists());
        // Featured cases at the end
        assert_eq!(cases[5].id, "F0001");
        assert!(cases[5].waffle_path.exists());
    }

    #[test]
    fn discover_cases_missing_dir() {
        let cases = discover_cases(Path::new("/nonexistent/path"));
        assert!(cases.is_empty());
    }

    #[test]
    fn categorize_detects_auto_union_failed() {
        use crate::assay::gen::{AssayMeta, OpMeta, OracleExpectations};
        use std::time::Duration;

        let result = AssayResult {
            id: "R0001".to_string(),
            description: "test".to_string(),
            status: AssayStatus::Failed,
            duration: Duration::from_millis(10),
            detail: "auto-union-failed (2 warning(s)): Extrude 2: Auto-union failed: no overlapping bodies".to_string(),
        };
        let meta = AssayMeta {
            id: "R0001".to_string(),
            description: "test".to_string(),
            master_seed: 0,
            test_seed: 0,
            scale: 1.0,
            log_scale: 0.0,
            plane_origin: [0.0; 3],
            plane_normal: [0.0, 0.0, 1.0],
            operations: vec![
                OpMeta {
                    kind: "extrude".to_string(),
                    profile_type: "rectangle".to_string(),
                    profile_size: 0.01,
                    depth_or_angle: 0.01,
                    is_cut: false,
                },
                OpMeta {
                    kind: "extrude".to_string(),
                    profile_type: "rectangle".to_string(),
                    profile_size: 0.01,
                    depth_or_angle: 0.01,
                    is_cut: false,
                },
            ],
            oracles: OracleExpectations {
                euler_target: 2,
                expect_watertight: true,
                max_bbox_extent: 1.0,
                expect_positive_volume: true,
                volume_monotonicity: vec![],
            },
            generator_version: 3,
            featured: false,
        };

        let category = categorize_result(&result, &meta);
        assert_eq!(category, FailureCategory::AutoUnionFailed);
        assert_eq!(category.to_string(), "auto-union-failed");
    }

    #[test]
    fn categorize_detects_merge_incomplete() {
        use crate::assay::gen::{AssayMeta, OpMeta, OracleExpectations};
        use std::time::Duration;

        let result = AssayResult {
            id: "R0099".to_string(),
            description: "test merge incomplete".to_string(),
            status: AssayStatus::Failed,
            duration: Duration::from_millis(10),
            detail: "merge incomplete: 2 operations produced 2 separate solids (expected 1 merged)"
                .to_string(),
        };
        let meta = AssayMeta {
            id: "R0099".to_string(),
            description: "test merge incomplete".to_string(),
            master_seed: 0,
            test_seed: 0,
            scale: 1.0,
            log_scale: 0.0,
            plane_origin: [0.0; 3],
            plane_normal: [0.0, 0.0, 1.0],
            operations: vec![
                OpMeta {
                    kind: "extrude".to_string(),
                    profile_type: "rectangle".to_string(),
                    profile_size: 0.01,
                    depth_or_angle: 0.01,
                    is_cut: false,
                },
                OpMeta {
                    kind: "extrude".to_string(),
                    profile_type: "circle".to_string(),
                    profile_size: 0.01,
                    depth_or_angle: 0.01,
                    is_cut: false,
                },
            ],
            oracles: OracleExpectations {
                euler_target: 2,
                expect_watertight: true,
                max_bbox_extent: 1.0,
                expect_positive_volume: true,
                volume_monotonicity: vec![],
            },
            generator_version: 3,
            featured: false,
        };

        let category = categorize_result(&result, &meta);
        assert_eq!(category, FailureCategory::MergeIncomplete);
        assert_eq!(category.to_string(), "merge-incomplete");
    }

    #[test]
    #[ignore] // Runs real kernel on full corpus — use `cargo test -p test-harness --lib -- --ignored`
    fn multi_boss_no_false_pass() {
        // Validate that multi-op cases marked Passed don't have merge-incomplete failures.
        // Uses the real assay corpus if available.
        let assay_dir = PathBuf::from(env!("CARGO_MANIFEST_DIR"))
            .parent()
            .unwrap()
            .parent()
            .unwrap()
            .join("app/tests/cases/assay");
        if !assay_dir.exists() {
            return;
        }

        let cases = discover_cases(&assay_dir);
        for case in &cases {
            let result = replay_and_validate(case, true);
            if result.status == AssayStatus::Passed {
                let meta: AssayMeta =
                    serde_json::from_str(&fs::read_to_string(&case.meta_path).unwrap()).unwrap();
                if meta.operations.len() > 1 {
                    // Multi-op passes must not have merge-incomplete in detail
                    assert!(
                        !result.detail.contains("merge incomplete"),
                        "Case {} passed but has merge-incomplete failure",
                        case.id
                    );
                }
            }
        }
    }

    #[test]
    fn run_mock_smoke() {
        let (_dir, corpus_path) = generate_test_corpus(3);
        let report = run_randomized_assay(&corpus_path, false);
        // 3 random + 10 featured = 13 total
        assert_eq!(report.total, 13);
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
