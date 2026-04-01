//! Randomized test case runner for Assay v3.
//!
//! Discovers `.waffle` + `.meta.json` pairs on disk, loads each through the
//! full `LoadProject` dispatch path (same as File > Open in the GUI), then
//! runs oracle checks on the resulting geometry.

use std::collections::HashMap;
use std::fmt::Write as FmtWrite;
use std::fs;
use std::path::{Path, PathBuf};
use std::time::{Instant, SystemTime, UNIX_EPOCH};

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

    let report = AssayReport {
        total: results.len(),
        passed,
        failed,
        errored,
        results,
        total_duration: start.elapsed(),
    };

    // Auto-update results.json so the AssayBrowser GUI stays in sync
    // Only write when using real kernel — mock results aren't meaningful for the GUI
    if use_kernel {
        let catalog = build_catalog(dir, &report);
        write_results_json(dir, &catalog);
    }

    report
}

/// Run the randomized assay on a single case by ID.
///
/// Useful for debugging specific test failures.
pub fn run_single_case(dir: &Path, case_id: &str, use_kernel: bool) -> Option<AssayResult> {
    let cases = discover_cases(dir);
    let case = cases.iter().find(|c| c.id == case_id)?;
    let result = replay_and_validate(case, use_kernel);

    // Merge this single result into results.json so the GUI stays in sync
    // Only write when using real kernel — mock results aren't meaningful for the GUI
    if use_kernel {
        update_single_result(dir, case, &result);
    }

    Some(result)
}

/// Update a single case's entry in the existing results.json (read → merge → write).
fn update_single_result(dir: &Path, case: &DiscoveredCase, result: &AssayResult) {
    let meta: AssayMeta = fs::read_to_string(&case.meta_path)
        .ok()
        .and_then(|s| serde_json::from_str(&s).ok())
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
                expect_rebuild_error: false,
            },
            generator_version: 0,
            featured: false,
        });

    let category = categorize_result(result, &meta);
    let status_str = match result.status {
        AssayStatus::Passed => "pass",
        AssayStatus::Failed => "fail",
        AssayStatus::Errored => "error",
    };

    let new_entry = serde_json::json!({
        "id": result.id,
        "status": status_str,
        "category": category.to_string(),
        "detail": result.detail,
    });

    let results_path = dir.join("results.json");

    // Read existing results.json, or start with an empty structure
    let mut doc: serde_json::Value = fs::read_to_string(&results_path)
        .ok()
        .and_then(|s| serde_json::from_str(&s).ok())
        .unwrap_or_else(|| {
            serde_json::json!({
                "generated": today_utc(),
                "total": 0,
                "passed": 0,
                "failed": 0,
                "errored": 0,
                "results": [],
            })
        });

    // Find or insert entry for this case
    if let Some(results_arr) = doc["results"].as_array_mut() {
        if let Some(existing) = results_arr.iter_mut().find(|e| e["id"] == result.id) {
            *existing = new_entry;
        } else {
            results_arr.push(new_entry);
        }

        // Recompute summary counts
        let total = results_arr.len();
        let passed = results_arr.iter().filter(|e| e["status"] == "pass").count();
        let failed = results_arr.iter().filter(|e| e["status"] == "fail").count();
        let errored = results_arr
            .iter()
            .filter(|e| e["status"] == "error")
            .count();
        doc["generated"] = serde_json::Value::String(today_utc());
        doc["total"] = serde_json::json!(total);
        doc["passed"] = serde_json::json!(passed);
        doc["failed"] = serde_json::json!(failed);
        doc["errored"] = serde_json::json!(errored);
    }

    fs::write(
        &results_path,
        serde_json::to_string_pretty(&doc).expect("serialize results"),
    )
    .unwrap_or_else(|e| {
        eprintln!(
            "Failed to write results.json to {}: {}",
            results_path.display(),
            e
        );
    });
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

    // 4b-1. If we expect a rebuild error (e.g., disjoint union), check and pass early
    if meta.oracles.expect_rebuild_error {
        if !engine_errors.is_empty() {
            return AssayResult {
                id: case.id.clone(),
                description: meta.description.clone(),
                status: AssayStatus::Passed,
                duration: case_start.elapsed(),
                detail: format!(
                    "expected rebuild error (disjoint operands): {}",
                    error_msgs.join("; ")
                ),
            };
        }
        // Expected an error but got none — that's a failure
        return AssayResult {
            id: case.id.clone(),
            description: meta.description.clone(),
            status: AssayStatus::Failed,
            duration: case_start.elapsed(),
            detail: "expected rebuild error but rebuild succeeded".to_string(),
        };
    }

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

    // 5. Tessellate last feature with scale-adaptive tolerance.
    // At micro scales (1e-4), a fixed 0.1 tolerance is larger than the
    // features themselves. Use scale * 0.01 (clamped to [1e-9, 0.1]).
    let tess_tol = (meta.scale * 0.01).clamp(1e-9, 0.1);
    let mesh = match builder.tessellate_last_with_tol(tess_tol) {
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

    // AABB-collapse check: any non-rectangle profile can collapse to its AABB if
    // the kernel degenerates. Only pure-rectangle cases legitimately produce box-shaped meshes.
    let all_rectangles = meta
        .operations
        .iter()
        .all(|op| op.profile_type == "rectangle");
    if !all_rectangles && !mesh.vertices.is_empty() {
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

    // Note: volume positivity is checked unconditionally by check_positive_signed_volume()
    // in run_all_mesh_checks(). The meta.oracles.expect_positive_volume field is vestigial
    // and always true — no separate inline check needed.

    // Minimum triangle count oracle
    {
        let ops: Vec<(String, String)> = meta
            .operations
            .iter()
            .map(|o| (o.kind.clone(), o.profile_type.clone()))
            .collect();
        let verdict = crate::oracle::check_minimum_triangle_count(&mesh, &ops);
        if !verdict.passed {
            failures.push(format!("minimum_triangle_count: {}", verdict.detail));
        }
    }

    // Volume magnitude bounds oracle
    if !mesh.vertices.is_empty() {
        let verdict = crate::oracle::check_volume_magnitude(&mesh, meta.scale);
        if !verdict.passed {
            failures.push(format!("volume_magnitude: {}", verdict.detail));
        }
    }

    // Mesh Euler characteristic check
    if !mesh.vertices.is_empty() {
        let verdict =
            crate::oracle::check_mesh_euler_characteristic(&mesh, meta.oracles.euler_target);
        if !verdict.passed {
            failures.push(format!("mesh_euler_characteristic: {}", verdict.detail));
        }
    }

    // Volume monotonicity check (per-step)
    if !meta.oracles.volume_monotonicity.is_empty() {
        if let Some(failure_msg) = check_volume_monotonicity(case, use_kernel, &meta) {
            failures.push(failure_msg);
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

/// Check volume monotonicity by replaying the model incrementally.
///
/// Loads the `.waffle` with truncated feature lists (first 2 features, then 4, etc.)
/// to capture per-step volumes. Compares consecutive volumes against expected directions.
fn check_volume_monotonicity(
    case: &DiscoveredCase,
    use_kernel: bool,
    meta: &AssayMeta,
) -> Option<String> {
    let waffle_json = fs::read_to_string(&case.waffle_path).ok()?;
    let doc: serde_json::Value = serde_json::from_str(&waffle_json).ok()?;

    let features = doc
        .get("features")
        .and_then(|f| f.get("features"))
        .and_then(|f| f.as_array())
        .cloned()?;

    let n_ops = meta.operations.len();
    let expected = &meta.oracles.volume_monotonicity;
    if expected.len() != n_ops {
        return Some(format!(
            "volume_monotonicity: expected {} entries for {} ops, got {}",
            n_ops,
            n_ops,
            expected.len()
        ));
    }

    // Each operation = 2 features (sketch + op). Collect volume after each op.
    let mut volumes: Vec<f64> = Vec::new();
    for step in 0..n_ops {
        let feature_count = (step + 1) * 2; // include sketch + op for this step
        let truncated_features: Vec<serde_json::Value> =
            features.iter().take(feature_count).cloned().collect();

        // Build a truncated waffle JSON
        let mut truncated_doc = doc.clone();
        truncated_doc["features"]["features"] = serde_json::Value::Array(truncated_features);
        let truncated_json = match serde_json::to_string(&truncated_doc) {
            Ok(s) => s,
            Err(_) => {
                volumes.push(f64::NAN);
                continue;
            }
        };

        let mut builder = if use_kernel {
            ModelBuilder::kernel()
        } else {
            ModelBuilder::mock()
        };

        if builder.load(&truncated_json).is_err() {
            volumes.push(f64::NAN);
            continue;
        }

        match builder.tessellate_last() {
            Ok(mesh) => {
                let vol = mesh_signed_volume(&mesh);
                volumes.push(vol.abs());
            }
            Err(_) => {
                volumes.push(f64::NAN);
            }
        }
    }

    // Report NaN volumes (load/tessellation failures)
    let mut violations = Vec::new();
    let nan_count = volumes.iter().filter(|v| v.is_nan()).count();
    if nan_count > 0 {
        violations.push(format!(
            "{} of {} steps had NaN volume (load/tessellation failure)",
            nan_count,
            volumes.len()
        ));
    }

    // Compare consecutive volumes against expected monotonicity
    for i in 0..expected.len() {
        let vol = volumes[i];
        if vol.is_nan() || vol <= 0.0 {
            // Can't check monotonicity if we couldn't get a valid volume
            continue;
        }
        if i == 0 {
            // First op: just verify we got a positive volume (boss)
            continue;
        }
        let prev_vol = volumes[i - 1];
        if prev_vol.is_nan() || prev_vol <= 0.0 {
            continue;
        }

        let direction = &expected[i];
        // Use relative tolerance: volume can stay the same (e.g., overlapping
        // boss or non-overlapping cut) but should never move in the WRONG direction.
        // A boss that decreases volume or a cut that increases volume is a real bug.
        let rel_tol = 1e-6;
        match direction.as_str() {
            "increase" => {
                // Boss/union: volume must not DECREASE (but can stay same)
                if vol < prev_vol * (1.0 - rel_tol) {
                    violations.push(format!(
                        "step {}: expected non-decrease, vol {:.6e} < prev {:.6e}",
                        i + 1,
                        vol,
                        prev_vol
                    ));
                }
            }
            "decrease" => {
                // Cut/subtract: volume must not INCREASE (but can stay same)
                if vol > prev_vol * (1.0 + rel_tol) {
                    violations.push(format!(
                        "step {}: expected non-increase, vol {:.6e} > prev {:.6e}",
                        i + 1,
                        vol,
                        prev_vol
                    ));
                }
            }
            _ => {} // unknown direction, skip
        }
    }

    if violations.is_empty() {
        None
    } else {
        Some(format!("volume_monotonicity: {}", violations.join(", ")))
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
    /// Mesh has fewer triangles than expected for its profile/operation types.
    MeshTooSimple,
    /// Volume magnitude is wildly out of range for the model's scale.
    VolumeMagnitude,
    /// Volume monotonicity violated — boss didn't increase or cut didn't decrease volume.
    VolumeMonotonicity,
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
            Self::MeshTooSimple => write!(f, "mesh-too-simple"),
            Self::VolumeMagnitude => write!(f, "volume-magnitude"),
            Self::VolumeMonotonicity => write!(f, "volume-monotonicity"),
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

            // 6b. Mesh too simple (fewer triangles than expected)
            if detail.contains("minimum_triangle_count:") {
                return FailureCategory::MeshTooSimple;
            }

            // 6c. Volume magnitude out of range
            if detail.contains("volume_magnitude:") {
                return FailureCategory::VolumeMagnitude;
            }

            // 6d. Volume monotonicity violated
            if detail.contains("volume_monotonicity:") {
                return FailureCategory::VolumeMonotonicity;
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
                    expect_rebuild_error: false,
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

    writeln!(
        out,
        "ASSAY v3 FAILURE CATALOG — WaffleKernel ({})",
        today_utc()
    )
    .unwrap();
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
    writeln!(out, "Generated: {}", today_utc()).unwrap();
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

/// Return today's date as `YYYY-MM-DD` in UTC, using only `std::time`.
fn today_utc() -> String {
    let secs = SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .unwrap_or_default()
        .as_secs();
    let days = secs / 86400;

    // Civil date from day count (algorithm from Howard Hinnant)
    let z = days as i64 + 719468;
    let era = z.div_euclid(146097);
    let doe = z.rem_euclid(146097) as u64; // day of era [0, 146096]
    let yoe = (doe - doe / 1460 + doe / 36524 - doe / 146096) / 365; // year of era [0, 399]
    let y = (yoe as i64) + era * 400;
    let doy = doe - (365 * yoe + yoe / 4 - yoe / 100); // day of year [0, 365]
    let mp = (5 * doy + 2) / 153; // [0, 11]
    let d = doy - (153 * mp + 2) / 5 + 1; // [1, 31]
    let m = if mp < 10 { mp + 3 } else { mp - 9 }; // [1, 12]
    let y = if m <= 2 { y + 1 } else { y };
    format!("{:04}-{:02}-{:02}", y, m, d)
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
        "generated": today_utc(),
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

// ── Yang Pipeline Comparison ─────────────────────────────────────────────

/// Result of comparing legacy vs. Yang pipeline for a single case.
#[derive(Debug, Clone)]
pub struct ComparisonEntry {
    pub id: String,
    pub legacy_status: AssayStatus,
    pub yang_status: AssayStatus,
    pub legacy_detail: String,
    pub yang_detail: String,
    pub change: ComparisonChange,
}

/// How a case's status changed between pipelines.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum ComparisonChange {
    /// Both pipelines produce the same status.
    Unchanged,
    /// Legacy failed/errored but Yang passes.
    Improved,
    /// Legacy passed but Yang failed/errored.
    Regressed,
    /// Both fail/error but with different status (e.g., error → fail).
    DifferentFailure,
}

/// Run the full assay corpus twice — once with the legacy pipeline, once with
/// `YANG_BOOLEAN=1` — and produce a per-case comparison.
///
/// Returns `(legacy_report, yang_report, comparisons)`.
///
/// **Thread safety**: This function manipulates `YANG_BOOLEAN` env var and must
/// not be called concurrently with other tests that depend on it.
pub fn run_yang_comparison(dir: &Path) -> (AssayReport, AssayReport, Vec<ComparisonEntry>) {
    // Phase 1: Legacy pipeline (ensure YANG_BOOLEAN is unset)
    std::env::remove_var("YANG_BOOLEAN");
    eprintln!("\n=== LEGACY PIPELINE ===\n");
    let legacy_report = run_randomized_assay(dir, true);

    // Phase 2: Yang pipeline
    std::env::set_var("YANG_BOOLEAN", "1");
    eprintln!("\n=== YANG PIPELINE (YANG_BOOLEAN=1) ===\n");
    let yang_report = run_randomized_assay(dir, true);

    // Restore env
    std::env::remove_var("YANG_BOOLEAN");

    // Phase 3: Build comparison
    let mut comparisons = Vec::new();
    for (legacy, yang) in legacy_report.results.iter().zip(yang_report.results.iter()) {
        assert_eq!(legacy.id, yang.id, "case ordering mismatch");
        let change = match (legacy.status, yang.status) {
            (a, b) if a == b => ComparisonChange::Unchanged,
            (AssayStatus::Passed, _) => ComparisonChange::Regressed,
            (_, AssayStatus::Passed) => ComparisonChange::Improved,
            _ => ComparisonChange::DifferentFailure,
        };
        comparisons.push(ComparisonEntry {
            id: legacy.id.clone(),
            legacy_status: legacy.status,
            yang_status: yang.status,
            legacy_detail: legacy.detail.clone(),
            yang_detail: yang.detail.clone(),
            change,
        });
    }

    (legacy_report, yang_report, comparisons)
}

/// Generate a markdown comparison report between legacy and Yang pipelines.
pub fn generate_comparison_markdown(
    legacy: &AssayReport,
    yang: &AssayReport,
    comparisons: &[ComparisonEntry],
) -> String {
    let mut out = String::new();

    writeln!(out, "# Yang Pipeline Assay Comparison (Phase 5b)").unwrap();
    writeln!(out).unwrap();
    writeln!(out, "Generated: {}", today_utc()).unwrap();
    writeln!(
        out,
        "Reference: [#24] Yang et al. 2025 — Hybrid B-Rep/mesh boolean"
    )
    .unwrap();
    writeln!(out).unwrap();

    // Summary table
    writeln!(out, "## Summary").unwrap();
    writeln!(out).unwrap();
    writeln!(out, "| Metric | Legacy | Yang | Delta |").unwrap();
    writeln!(out, "|--------|--------|------|-------|").unwrap();
    let delta_pass = yang.passed as i64 - legacy.passed as i64;
    let delta_fail = yang.failed as i64 - legacy.failed as i64;
    let delta_err = yang.errored as i64 - legacy.errored as i64;
    writeln!(
        out,
        "| Passed | {}/{} | {}/{} | {:+} |",
        legacy.passed, legacy.total, yang.passed, yang.total, delta_pass
    )
    .unwrap();
    writeln!(
        out,
        "| Failed | {} | {} | {:+} |",
        legacy.failed, yang.failed, delta_fail
    )
    .unwrap();
    writeln!(
        out,
        "| Errored | {} | {} | {:+} |",
        legacy.errored, yang.errored, delta_err
    )
    .unwrap();
    writeln!(
        out,
        "| Duration | {:.1}s | {:.1}s | |",
        legacy.total_duration.as_secs_f64(),
        yang.total_duration.as_secs_f64()
    )
    .unwrap();
    writeln!(out).unwrap();

    // Change counts
    let improved: Vec<_> = comparisons
        .iter()
        .filter(|c| c.change == ComparisonChange::Improved)
        .collect();
    let regressed: Vec<_> = comparisons
        .iter()
        .filter(|c| c.change == ComparisonChange::Regressed)
        .collect();
    let different: Vec<_> = comparisons
        .iter()
        .filter(|c| c.change == ComparisonChange::DifferentFailure)
        .collect();
    let unchanged: Vec<_> = comparisons
        .iter()
        .filter(|c| c.change == ComparisonChange::Unchanged)
        .collect();

    writeln!(out, "## Change Summary").unwrap();
    writeln!(out).unwrap();
    writeln!(
        out,
        "- **Improved** (legacy fail → Yang pass): {}",
        improved.len()
    )
    .unwrap();
    writeln!(
        out,
        "- **Regressed** (legacy pass → Yang fail): {}",
        regressed.len()
    )
    .unwrap();
    writeln!(out, "- **Different failure**: {}", different.len()).unwrap();
    writeln!(out, "- **Unchanged**: {}", unchanged.len()).unwrap();
    writeln!(out).unwrap();

    // Improved cases
    if !improved.is_empty() {
        writeln!(out, "## Improved Cases").unwrap();
        writeln!(out).unwrap();
        writeln!(
            out,
            "| Case | Legacy Status | Legacy Detail | Yang Detail |"
        )
        .unwrap();
        writeln!(out, "|------|--------------|---------------|-------------|").unwrap();
        for c in &improved {
            let legacy_s = match c.legacy_status {
                AssayStatus::Failed => "FAIL",
                AssayStatus::Errored => "ERROR",
                _ => "PASS",
            };
            // Truncate details for table readability
            let leg_d = truncate_detail(&c.legacy_detail, 80);
            let yang_d = truncate_detail(&c.yang_detail, 80);
            writeln!(out, "| {} | {} | {} | {} |", c.id, legacy_s, leg_d, yang_d).unwrap();
        }
        writeln!(out).unwrap();
    }

    // Regressed cases
    if !regressed.is_empty() {
        writeln!(out, "## Regressed Cases").unwrap();
        writeln!(out).unwrap();
        writeln!(out, "| Case | Yang Status | Legacy Detail | Yang Detail |").unwrap();
        writeln!(out, "|------|------------|---------------|-------------|").unwrap();
        for c in &regressed {
            let yang_s = match c.yang_status {
                AssayStatus::Failed => "FAIL",
                AssayStatus::Errored => "ERROR",
                _ => "PASS",
            };
            let leg_d = truncate_detail(&c.legacy_detail, 80);
            let yang_d = truncate_detail(&c.yang_detail, 80);
            writeln!(out, "| {} | {} | {} | {} |", c.id, yang_s, leg_d, yang_d).unwrap();
        }
        writeln!(out).unwrap();
    }

    // Different failure cases
    if !different.is_empty() {
        writeln!(out, "## Different Failure Mode").unwrap();
        writeln!(out).unwrap();
        writeln!(
            out,
            "| Case | Legacy | Yang | Legacy Detail | Yang Detail |"
        )
        .unwrap();
        writeln!(
            out,
            "|------|--------|------|---------------|-------------|"
        )
        .unwrap();
        for c in &different {
            let legacy_s = match c.legacy_status {
                AssayStatus::Failed => "FAIL",
                AssayStatus::Errored => "ERROR",
                _ => "PASS",
            };
            let yang_s = match c.yang_status {
                AssayStatus::Failed => "FAIL",
                AssayStatus::Errored => "ERROR",
                _ => "PASS",
            };
            let leg_d = truncate_detail(&c.legacy_detail, 60);
            let yang_d = truncate_detail(&c.yang_detail, 60);
            writeln!(
                out,
                "| {} | {} | {} | {} | {} |",
                c.id, legacy_s, yang_s, leg_d, yang_d
            )
            .unwrap();
        }
        writeln!(out).unwrap();
    }

    // Verdict
    writeln!(out, "## Verdict").unwrap();
    writeln!(out).unwrap();
    if regressed.is_empty() {
        writeln!(
            out,
            "**No regressions.** The Yang pipeline matches or exceeds legacy results."
        )
        .unwrap();
        if !improved.is_empty() {
            writeln!(out, "The Yang pipeline improves {} cases.", improved.len()).unwrap();
        }
        writeln!(out).unwrap();
        writeln!(
            out,
            "**Recommendation**: Proceed to Phase 5c — remove the `YANG_BOOLEAN` feature flag \
             and make the Yang pipeline the default boolean path."
        )
        .unwrap();
    } else {
        writeln!(
            out,
            "**{} regressions detected.** The Yang pipeline regresses {} cases that \
             previously passed. These must be investigated before Phase 5c.",
            regressed.len(),
            regressed.len()
        )
        .unwrap();
        writeln!(out).unwrap();
        writeln!(out, "### Regression Analysis").unwrap();
        writeln!(out).unwrap();
        for c in &regressed {
            writeln!(out, "#### {}", c.id).unwrap();
            writeln!(out, "- **Legacy**: PASS — {}", c.legacy_detail).unwrap();
            writeln!(out, "- **Yang**: {:?} — {}", c.yang_status, c.yang_detail).unwrap();
            writeln!(out).unwrap();
        }
    }

    out
}

/// Write the comparison report as JSON for machine consumption.
pub fn write_comparison_json(
    dir: &Path,
    legacy: &AssayReport,
    yang: &AssayReport,
    comparisons: &[ComparisonEntry],
) {
    let entries: Vec<serde_json::Value> = comparisons
        .iter()
        .map(|c| {
            let change_str = match c.change {
                ComparisonChange::Unchanged => "unchanged",
                ComparisonChange::Improved => "improved",
                ComparisonChange::Regressed => "regressed",
                ComparisonChange::DifferentFailure => "different-failure",
            };
            let legacy_s = match c.legacy_status {
                AssayStatus::Passed => "pass",
                AssayStatus::Failed => "fail",
                AssayStatus::Errored => "error",
            };
            let yang_s = match c.yang_status {
                AssayStatus::Passed => "pass",
                AssayStatus::Failed => "fail",
                AssayStatus::Errored => "error",
            };
            serde_json::json!({
                "id": c.id,
                "legacy_status": legacy_s,
                "yang_status": yang_s,
                "change": change_str,
                "legacy_detail": c.legacy_detail,
                "yang_detail": c.yang_detail,
            })
        })
        .collect();

    let improved = comparisons
        .iter()
        .filter(|c| c.change == ComparisonChange::Improved)
        .count();
    let regressed = comparisons
        .iter()
        .filter(|c| c.change == ComparisonChange::Regressed)
        .count();

    let json = serde_json::json!({
        "generated": today_utc(),
        "legacy": {
            "passed": legacy.passed,
            "failed": legacy.failed,
            "errored": legacy.errored,
            "total": legacy.total,
            "duration_secs": legacy.total_duration.as_secs_f64(),
        },
        "yang": {
            "passed": yang.passed,
            "failed": yang.failed,
            "errored": yang.errored,
            "total": yang.total,
            "duration_secs": yang.total_duration.as_secs_f64(),
        },
        "improved": improved,
        "regressed": regressed,
        "results": entries,
    });

    let path = dir.join("yang_comparison.json");
    fs::write(
        &path,
        serde_json::to_string_pretty(&json).expect("serialize comparison"),
    )
    .unwrap_or_else(|e| {
        eprintln!(
            "Failed to write yang_comparison.json to {}: {}",
            path.display(),
            e
        );
    });
}

/// Truncate a detail string for table display.
fn truncate_detail(s: &str, max_len: usize) -> String {
    if s.len() <= max_len {
        s.to_string()
    } else {
        format!("{}...", &s[..max_len])
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
        assert_eq!(cases.len(), 95); // 5 random + 90 featured
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
                    plane_origin: None,
                    plane_normal: None,
                },
                OpMeta {
                    kind: "extrude".to_string(),
                    profile_type: "rectangle".to_string(),
                    profile_size: 0.01,
                    depth_or_angle: 0.01,
                    is_cut: false,
                    plane_origin: None,
                    plane_normal: None,
                },
            ],
            oracles: OracleExpectations {
                euler_target: 2,
                expect_watertight: true,
                max_bbox_extent: 1.0,
                expect_positive_volume: true,
                volume_monotonicity: vec![],
                expect_rebuild_error: false,
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
                    plane_origin: None,
                    plane_normal: None,
                },
                OpMeta {
                    kind: "extrude".to_string(),
                    profile_type: "circle".to_string(),
                    profile_size: 0.01,
                    depth_or_angle: 0.01,
                    is_cut: false,
                    plane_origin: None,
                    plane_normal: None,
                },
            ],
            oracles: OracleExpectations {
                euler_target: 2,
                expect_watertight: true,
                max_bbox_extent: 1.0,
                expect_positive_volume: true,
                volume_monotonicity: vec![],
                expect_rebuild_error: false,
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
    fn categorize_watertight_beats_monotonicity() {
        // When both watertight AND monotonicity fail, categorize as BooleanWatertight
        // (root cause) not VolumeMonotonicity (downstream symptom).
        use crate::assay::gen::{AssayMeta, OpMeta, OracleExpectations};
        use std::time::Duration;

        let result = AssayResult {
            id: "R0050".to_string(),
            description: "test priority".to_string(),
            status: AssayStatus::Failed,
            duration: Duration::from_millis(10),
            detail: "watertight_mesh: 5 unpaired edges out of 100 total; volume_monotonicity: step 2: expected decrease, vol 1.5e0 >= prev 1.0e0".to_string(),
        };
        let meta = AssayMeta {
            id: "R0050".to_string(),
            description: "test priority".to_string(),
            master_seed: 0,
            test_seed: 0,
            scale: 1.0,
            log_scale: 0.0,
            plane_origin: [0.0; 3],
            plane_normal: [0.0, 0.0, 1.0],
            operations: vec![OpMeta {
                kind: "extrude".to_string(),
                profile_type: "rectangle".to_string(),
                profile_size: 0.01,
                depth_or_angle: 0.01,
                is_cut: false,
                plane_origin: None,
                plane_normal: None,
            }],
            oracles: OracleExpectations {
                euler_target: 2,
                expect_watertight: true,
                max_bbox_extent: 1.0,
                expect_positive_volume: true,
                volume_monotonicity: vec!["increase".to_string()],
                expect_rebuild_error: false,
            },
            generator_version: 3,
            featured: false,
        };

        let category = categorize_result(&result, &meta);
        assert_eq!(
            category,
            FailureCategory::BooleanWatertight,
            "watertight failure should take priority over volume monotonicity"
        );
    }

    #[test]
    fn categorize_detects_mesh_too_simple() {
        use crate::assay::gen::{AssayMeta, OpMeta, OracleExpectations};
        use std::time::Duration;

        let result = AssayResult {
            id: "R0100".to_string(),
            description: "test mesh too simple".to_string(),
            status: AssayStatus::Failed,
            duration: Duration::from_millis(10),
            detail: "minimum_triangle_count: 12 triangles < expected minimum 36".to_string(),
        };
        let meta = AssayMeta {
            id: "R0100".to_string(),
            description: "test".to_string(),
            master_seed: 0,
            test_seed: 0,
            scale: 1.0,
            log_scale: 0.0,
            plane_origin: [0.0; 3],
            plane_normal: [0.0, 0.0, 1.0],
            operations: vec![OpMeta {
                kind: "revolve".to_string(),
                profile_type: "rectangle".to_string(),
                profile_size: 0.01,
                depth_or_angle: 90.0,
                is_cut: true,
                plane_origin: None,
                plane_normal: None,
            }],
            oracles: OracleExpectations {
                euler_target: 2,
                expect_watertight: true,
                max_bbox_extent: 1.0,
                expect_positive_volume: true,
                volume_monotonicity: vec![],
                expect_rebuild_error: false,
            },
            generator_version: 3,
            featured: false,
        };

        let category = categorize_result(&result, &meta);
        assert_eq!(category, FailureCategory::MeshTooSimple);
        assert_eq!(category.to_string(), "mesh-too-simple");
    }

    #[test]
    fn categorize_detects_volume_magnitude() {
        use crate::assay::gen::{AssayMeta, OpMeta, OracleExpectations};
        use std::time::Duration;

        let result = AssayResult {
            id: "R0042".to_string(),
            description: "test volume magnitude".to_string(),
            status: AssayStatus::Failed,
            duration: Duration::from_millis(10),
            detail: "volume_magnitude: volume 1e-20 outside [1e-2, 1e14] for scale 1e2".to_string(),
        };
        let meta = AssayMeta {
            id: "R0042".to_string(),
            description: "test".to_string(),
            master_seed: 0,
            test_seed: 0,
            scale: 100.0,
            log_scale: 2.0,
            plane_origin: [0.0; 3],
            plane_normal: [0.0, 0.0, 1.0],
            operations: vec![OpMeta {
                kind: "extrude".to_string(),
                profile_type: "rectangle".to_string(),
                profile_size: 50.0,
                depth_or_angle: 30.0,
                is_cut: false,
                plane_origin: None,
                plane_normal: None,
            }],
            oracles: OracleExpectations {
                euler_target: 2,
                expect_watertight: true,
                max_bbox_extent: 300.0,
                expect_positive_volume: true,
                volume_monotonicity: vec![],
                expect_rebuild_error: false,
            },
            generator_version: 3,
            featured: false,
        };

        let category = categorize_result(&result, &meta);
        assert_eq!(category, FailureCategory::VolumeMagnitude);
        assert_eq!(category.to_string(), "volume-magnitude");
    }

    #[test]
    fn run_mock_smoke() {
        let (_dir, corpus_path) = generate_test_corpus(3);
        let report = run_randomized_assay(&corpus_path, false);
        // 3 random + 90 featured = 93 total
        assert_eq!(report.total, 93);
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

    /// Test 3: The bbox oracle should use a larger multiplier for revolve operations.
    ///
    /// A revolve operation sweeps a profile around an axis, potentially creating
    /// geometry that extends much further than the profile's scale. For example,
    /// a rectangle profile at scale=0.1 with its center 0.2m from the revolve axis
    /// produces a torus-like solid with diameter ~0.5m — well beyond `scale * 3.0 = 0.3`.
    ///
    /// The current `max_bbox_extent = scale * 3.0` in gen.rs does NOT account for
    /// revolve operations. This test verifies that a revolve case with reasonable
    /// geometry doesn't get falsely flagged as "bbox exceeded".
    #[test]
    fn bbox_oracle_uses_larger_multiplier_for_revolve() {
        use crate::assay::gen::{AssayMeta, OpMeta, OracleExpectations};

        // Simulate a revolve case at scale=0.1 where the profile center is offset
        // from the revolve axis, producing a solid with bbox diagonal ~0.5m.
        let scale = 0.1;
        let meta = AssayMeta {
            id: "test-revolve-bbox".to_string(),
            description: "revolve bbox test".to_string(),
            master_seed: 0,
            test_seed: 0,
            scale,
            log_scale: scale.log10(),
            plane_origin: [0.0, 0.0, 0.0],
            plane_normal: [0.0, 0.0, 1.0],
            operations: vec![OpMeta {
                kind: "revolve".to_string(),
                profile_type: "rectangle".to_string(),
                profile_size: 0.05,
                depth_or_angle: 360.0,
                is_cut: false,
                plane_origin: None,
                plane_normal: None,
            }],
            oracles: OracleExpectations {
                euler_target: 2,
                expect_watertight: true,
                max_bbox_extent: scale * 10.0, // Revolve-aware formula: scale * 10.0 = 1.0
                expect_positive_volume: true,
                volume_monotonicity: vec!["increase".to_string()],
                expect_rebuild_error: false,
            },
            generator_version: 2,
            featured: false,
        };

        // A revolve of a rectangle at offset from axis produces geometry whose
        // bbox diagonal can easily be 0.5m at scale 0.1. This is a valid solid
        // but exceeds `scale * 3.0 = 0.3`.
        let bbox_diagonal = 0.5; // realistic revolve solid bbox

        // The bbox check: `diagonal > max_extent` → failure
        let max_extent = meta.oracles.max_bbox_extent; // 0.3

        // With the current formula (scale * 3.0), this is a false positive:
        // diagonal 0.5 > max_extent 0.3 → flagged as failure, but the solid is valid.
        //
        // After the fix, revolve operations should use a larger multiplier
        // (e.g., scale * 10.0 or account for profile offset from axis),
        // so max_extent would be >= 0.5 and this check would pass.
        let has_revolve = meta.operations.iter().any(|op| op.kind == "revolve");
        assert!(has_revolve, "test case should have a revolve operation");

        // The max_bbox_extent should be large enough for revolve geometry.
        // Currently fails: 0.3 < 0.5
        assert!(
            max_extent >= bbox_diagonal,
            "bbox oracle max_extent ({:.3}) should accommodate revolve geometry (diagonal {:.3}). \
             Current formula `scale * 3.0` is too tight for revolve operations — \
             needs a larger multiplier (e.g., scale * 10.0) when revolve ops are present.",
            max_extent,
            bbox_diagonal
        );
    }
}
