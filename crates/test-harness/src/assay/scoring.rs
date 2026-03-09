//! Scoring harness — runs the full assay catalog and produces a report.
//!
//! Never panics. Catches errors per-test. Reports X/400 summary.

use crate::assay::catalog::{AssayCase, AssayExpected};
use crate::assay::runner::execute_recipe;
use crate::workflow::ModelBuilder;
use std::time::{Duration, Instant};

/// Result of a single assay test case.
#[derive(Debug, Clone)]
pub struct AssayResult {
    pub id: String,
    pub description: String,
    pub status: AssayStatus,
    pub duration: Duration,
    pub detail: String,
}

/// Status of a single assay test.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum AssayStatus {
    Passed,
    Failed,
    Errored,
}

/// Summary report from a full assay run.
#[derive(Debug)]
pub struct AssayReport {
    pub total: usize,
    pub passed: usize,
    pub failed: usize,
    pub errored: usize,
    pub results: Vec<AssayResult>,
    pub total_duration: Duration,
}

impl AssayReport {
    pub fn score_line(&self) -> String {
        format!(
            "ASSAY SCORE: {}/{} ({} failed, {} errored) in {:.1}s",
            self.passed,
            self.total,
            self.failed,
            self.errored,
            self.total_duration.as_secs_f64()
        )
    }
}

/// The result of executing a recipe.
pub struct ExecutionResult {
    pub volume: Option<f64>,
    pub euler: Option<i64>,
    pub face_count: Option<usize>,
    pub watertight: bool,
    pub bbox: Option<([f64; 3], [f64; 3])>,
}

/// Run the full assay catalog using WaffleKernel.
pub fn run_assay_kernel(cases: &[AssayCase]) -> AssayReport {
    run_assay_with(cases, || ModelBuilder::kernel())
}

/// Run the full assay catalog using MockKernel.
pub fn run_assay_mock(cases: &[AssayCase]) -> AssayReport {
    run_assay_with(cases, || ModelBuilder::mock())
}

/// Run the full assay catalog using the provided builder factory.
/// Each case gets a fresh ModelBuilder to avoid state leakage.
pub fn run_assay_with<F: Fn() -> ModelBuilder>(cases: &[AssayCase], make_builder: F) -> AssayReport {
    let start = Instant::now();
    let mut results = Vec::with_capacity(cases.len());
    let mut passed = 0usize;
    let mut failed = 0usize;
    let mut errored = 0usize;

    for case in cases {
        let case_start = Instant::now();
        let result = run_single_case(&make_builder, case);
        let duration = case_start.elapsed();

        match result.status {
            AssayStatus::Passed => passed += 1,
            AssayStatus::Failed => failed += 1,
            AssayStatus::Errored => errored += 1,
        }

        results.push(AssayResult {
            id: case.id.to_string(),
            description: case.description.to_string(),
            status: result.status,
            duration,
            detail: result.detail,
        });
    }

    AssayReport {
        total: cases.len(),
        passed,
        failed,
        errored,
        results,
        total_duration: start.elapsed(),
    }
}

struct SingleResult {
    status: AssayStatus,
    detail: String,
}

fn run_single_case<F: Fn() -> ModelBuilder>(make_builder: &F, case: &AssayCase) -> SingleResult {
    // Each case gets a fresh builder
    let mut builder = make_builder();

    // Execute the recipe
    let exec_result = std::panic::catch_unwind(std::panic::AssertUnwindSafe(|| {
        execute_recipe(&mut builder, &case.recipe)
    }));

    let execution = match exec_result {
        Ok(Ok(exec)) => exec,
        Ok(Err(e)) => {
            return SingleResult {
                status: AssayStatus::Errored,
                detail: format!("Recipe execution failed: {}", e),
            };
        }
        Err(panic) => {
            let msg = if let Some(s) = panic.downcast_ref::<String>() {
                s.clone()
            } else if let Some(s) = panic.downcast_ref::<&str>() {
                s.to_string()
            } else {
                "unknown panic".to_string()
            };
            return SingleResult {
                status: AssayStatus::Errored,
                detail: format!("Panic during execution: {}", msg),
            };
        }
    };

    // Check expected values against actual
    let mut failures = Vec::new();
    check_expected(&case.expected, &execution, &mut failures);

    if failures.is_empty() {
        SingleResult {
            status: AssayStatus::Passed,
            detail: "All checks passed".to_string(),
        }
    } else {
        SingleResult {
            status: AssayStatus::Failed,
            detail: failures.join("; "),
        }
    }
}

fn check_expected(
    expected: &AssayExpected,
    actual: &ExecutionResult,
    failures: &mut Vec<String>,
) {
    // Volume check
    if let Some(exp_vol) = expected.volume {
        match actual.volume {
            Some(act_vol) => {
                if (act_vol - exp_vol).abs() > expected.volume_tol {
                    failures.push(format!(
                        "Volume: expected {:.6} ± {:.6}, got {:.6}",
                        exp_vol, expected.volume_tol, act_vol
                    ));
                }
            }
            None => {
                failures.push("Volume: expected a value but got None".to_string());
            }
        }
    }

    // Euler check
    if let Some(exp_euler) = expected.euler {
        match actual.euler {
            Some(act_euler) => {
                if act_euler != exp_euler {
                    failures.push(format!(
                        "Euler: expected {}, got {}",
                        exp_euler, act_euler
                    ));
                }
            }
            None => {
                failures.push("Euler: expected a value but got None".to_string());
            }
        }
    }

    // Face count check
    if let Some(exp_faces) = expected.face_count {
        match actual.face_count {
            Some(act_faces) => {
                if act_faces != exp_faces {
                    failures.push(format!(
                        "Face count: expected {}, got {}",
                        exp_faces, act_faces
                    ));
                }
            }
            None => {
                failures.push("Face count: expected a value but got None".to_string());
            }
        }
    }

    // Watertight check
    if expected.watertight && !actual.watertight {
        failures.push("Watertight: expected watertight but got open edges".to_string());
    }

    // Bounding box check
    if let Some((exp_min, exp_max)) = expected.bbox {
        if let Some((act_min, act_max)) = actual.bbox {
            let tol = expected.volume_tol.cbrt().max(1e-6);
            for i in 0..3 {
                if (act_min[i] - exp_min[i]).abs() > tol {
                    failures.push(format!(
                        "BBox min[{}]: expected {:.6}, got {:.6}",
                        i, exp_min[i], act_min[i]
                    ));
                }
                if (act_max[i] - exp_max[i]).abs() > tol {
                    failures.push(format!(
                        "BBox max[{}]: expected {:.6}, got {:.6}",
                        i, exp_max[i], act_max[i]
                    ));
                }
            }
        }
    }
}
