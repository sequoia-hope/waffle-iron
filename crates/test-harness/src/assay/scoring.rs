//! Scoring types — result and report structures for assay runs.
//!
//! Used by both the legacy recipe-based runner and the randomized assay system.

use std::time::Duration;

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
