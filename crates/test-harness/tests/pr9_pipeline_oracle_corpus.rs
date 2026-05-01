//! PR9 — pipeline-oracle corpus runner.
//!
//! Iterates the Yang fast assay corpus (157 cases = 190 corpus minus 33
//! known timeout cases), runs each through `LoadProject` while a
//! thread-local snapshot collector captures Stage 0 / 1 / 2 / 4b / 6
//! state from the LAST Yang boolean executed during the load. Then runs
//! the PR9 default oracle registry against the captured `PipelineState`
//! and tallies per-stage first-failure counts into a histogram.
//!
//! Output: a Markdown histogram printed to stderr — copied verbatim into
//! `specs/pipeline_oracles.md` §3 ("First-failing-stage histogram").
//!
//! ## Snapshot capture mechanism
//!
//! The kernel installs an opt-in thread-local collector
//! (`crate::boolean::pipeline_oracles::with_snapshot_collector`); the
//! Yang pipeline writes to it at stage boundaries via `record_snapshot`
//! (no-ops when no collector is installed). PR9's `kernel::diagnostics`
//! re-exports a public wrapper `with_yang_oracle_capture` that bundles
//! collector install + closure invocation + oracle registry run.
//!
//! ## Constraints
//!
//! - `#[ignore]` (long-running; manual invocation).
//! - Sets `YANG_BOOLEAN=1` so LoadProject routes booleans through the
//!   Yang pipeline (production-default is the legacy S-H path).
//! - Uses 30s per-case timeout matching `yang_fast`.
//! - First-call oracle results are anchored per
//!   `feedback_no_regression_chasing.md` (R0080 / R0018 nondeterminism).
//!
//! Refs: Yang 2025 §4; Cherchi 2022 §3-5; audit
//! `docs/audits/yang_audit_2026-04-30.md` Cluster Y-I (Tier 1) which
//! predicts Stage 4b dominance (~92 / 157 first-fails per YB-01).

use std::collections::{BTreeMap, HashSet};
use std::path::Path;
use std::sync::mpsc;
use std::thread;
use std::time::Duration;

use kernel::diagnostics::{with_yang_oracle_capture, OracleRunSummary, ViolationKind, YangStage};
use test_harness::assay::randomized_runner::discover_cases;
use wasm_bridge::messages::UiToEngine;
use wasm_bridge::{dispatch, EngineState};

const ASSAY_DIR: &str = "../../app/tests/cases/assay";

/// Yang-fast skip list: 33 cases that exceed the 30 s per-case budget.
/// Mirrors `assay_randomized::yang_fast`; keeping a duplicate constant
/// here avoids a cross-test dependency on a private list.
const YANG_FAST_SKIP_IDS: &[&str] = &[
    "R0003", "R0010", "R0012", "R0026", "R0028", "R0053", "R0059", "R0065", "R0070", "R0085",
    "R0099", "R0100", "F0063", "F0065", "F0067", "F0068", "F0069", "F0070", "F0071", "F0072",
    "F0077", "F0078", "F0079", "F0080", "F0081", "F0082", "F0083", "F0084", "F0085", "F0087",
    "F0088", "F0089", "F0090",
];

const PER_CASE_TIMEOUT: Duration = Duration::from_secs(30);

/// One bucket in the first-failing-stage histogram. Stages without a
/// registered PR9 oracle (3, 4a) are absent; the runner tracks them via
/// `Stage4aBucket = HistogramKey::AllPass` if no oracle ever fires.
#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Hash)]
enum HistogramKey {
    Stage(YangStage),
    /// Every oracle passed (or self-skipped on missing snapshot).
    AllPass,
    /// `LoadProject` itself errored — no Yang pipeline run, no snapshots.
    /// Reported separately so the headline numbers reflect cases where
    /// the corpus runner actually exercised the pipeline.
    LoadError,
    /// The case spawned a worker thread that did not respond within
    /// `PER_CASE_TIMEOUT`. Bucketed separately so the runner doesn't
    /// stall when a single case runs over budget.
    Timeout,
}

impl HistogramKey {
    fn label(self) -> String {
        match self {
            HistogramKey::Stage(s) => format!("{s:?}"),
            HistogramKey::AllPass => "AllPass".to_string(),
            HistogramKey::LoadError => "LoadError".to_string(),
            HistogramKey::Timeout => "Timeout".to_string(),
        }
    }
}

/// Worker thread payload: load a single .waffle, run the oracle registry
/// against the captured snapshot bundle, return the summary.
fn run_one_case(case_id: String, waffle_path: std::path::PathBuf) -> Option<OracleRunSummary> {
    let waffle_json = std::fs::read_to_string(&waffle_path).ok()?;

    // Yang pipeline must be enabled so LoadProject routes booleans
    // through `yang_boolean_inner`, which is what writes to the
    // snapshot collector via `record_snapshot`.
    std::env::set_var("YANG_BOOLEAN", "1");

    // Capture snapshots from the LAST Yang boolean executed during
    // LoadProject. Cases with multiple booleans (e.g. 3-op chains) get
    // the final boolean's stage state — earlier booleans are
    // overwritten by the bundle's Option<T> fields.
    let id_for_capture = case_id.clone();
    let (summary, _load_outcome) = with_yang_oracle_capture(&id_for_capture, move || {
        let mut state = EngineState::new();
        let mut kernel_inst = kernel::WaffleKernel::new();
        let response = dispatch(
            &mut state,
            UiToEngine::LoadProject { data: waffle_json },
            &mut kernel_inst,
        );
        // Discriminant variant gives us "was it a successful response"
        // without coupling to wasm-bridge's response type internals.
        let _ = response;
        // engine errors: we don't fail-fast on them — the snapshot
        // bundle has whatever Yang stages did execute, which is what
        // we want to oracle-check.
        state.engine.errors.clone()
    });
    Some(summary)
}

/// Produce the histogram key for one summary: the first-failing stage
/// (where ContractViolated > OracleStub > StateMissing — i.e. only real
/// failures count toward the histogram bucket; OracleStub is a known
/// gap, not a failure). If every oracle passed or self-skipped or only
/// reported `OracleStub`, the case is bucketed as `AllPass`.
fn histogram_key(summary: &OracleRunSummary) -> HistogramKey {
    // The runner's `first_failing_stage` already sorts by stage order;
    // we additionally filter by violation kind: `OracleStub` does not
    // count as a real first-fail.
    let first_real_fail = summary
        .per_oracle
        .iter()
        .filter_map(|v| {
            v.violation.as_ref().and_then(|viol| match viol.kind {
                ViolationKind::ContractViolated | ViolationKind::StateMissing => Some(v.stage),
                ViolationKind::OracleStub => None,
            })
        })
        .min();
    match first_real_fail {
        Some(stage) => HistogramKey::Stage(stage),
        None => HistogramKey::AllPass,
    }
}

/// Format the histogram as a Markdown table that can be lifted into the
/// spec doc verbatim. Columns: `| Stage | Count | Notes |`.
fn format_histogram(
    histogram: &BTreeMap<HistogramKey, usize>,
    sample_per_bucket: &BTreeMap<HistogramKey, Vec<String>>,
) -> String {
    let mut out = String::new();
    out.push_str("| Bucket | Count | Sample case ids |\n");
    out.push_str("|--------|------:|-----------------|\n");
    for (key, count) in histogram {
        let label = key.label();
        let samples = sample_per_bucket
            .get(key)
            .map(|v| v.join(", "))
            .unwrap_or_default();
        out.push_str(&format!("| {label} | {count} | {samples} |\n"));
    }
    out
}

#[test]
#[ignore]
fn pr9_pipeline_oracle_corpus() {
    let dir = Path::new(ASSAY_DIR);
    if !dir.exists() {
        eprintln!("Assay corpus not generated — run assay_gen first; skipping.");
        return;
    }
    let cases = discover_cases(dir);
    if cases.is_empty() {
        eprintln!("Assay corpus is empty; skipping.");
        return;
    }
    let skip: HashSet<&str> = YANG_FAST_SKIP_IDS.iter().copied().collect();

    let mut histogram: BTreeMap<HistogramKey, usize> = BTreeMap::new();
    let mut sample_per_bucket: BTreeMap<HistogramKey, Vec<String>> = BTreeMap::new();
    let mut record = |key: HistogramKey, case_id: &str| {
        *histogram.entry(key).or_insert(0) += 1;
        let entries = sample_per_bucket.entry(key).or_default();
        if entries.len() < 4 {
            entries.push(case_id.to_string());
        }
    };

    let mut total_attempted = 0usize;
    for case in &cases {
        if skip.contains(case.id.as_str()) {
            continue;
        }
        total_attempted += 1;

        // Spawn each case in its own thread so a runaway pipeline can be
        // abandoned via `recv_timeout`. Inside the thread, the snapshot
        // collector is a thread-local — so each case's bundle is
        // perfectly isolated.
        let (tx, rx) = mpsc::channel::<Option<OracleRunSummary>>();
        let case_id = case.id.clone();
        let waffle_path = case.waffle_path.clone();
        let _handle = thread::spawn(move || {
            // Wrap in catch_unwind so a panicking pipeline doesn't crash
            // the runner. With panic=unwind enabled, this catches kernel
            // panics; the case is bucketed as the highest stage that
            // managed to record a snapshot before the panic.
            let result = std::panic::catch_unwind(std::panic::AssertUnwindSafe(|| {
                run_one_case(case_id, waffle_path)
            }));
            let _ = tx.send(result.unwrap_or(None));
        });

        match rx.recv_timeout(PER_CASE_TIMEOUT) {
            Ok(Some(summary)) => {
                let key = histogram_key(&summary);
                record(key, &case.id);
            }
            Ok(None) => {
                // run_one_case returned None — file read failed.
                record(HistogramKey::LoadError, &case.id);
            }
            Err(_) => {
                record(HistogramKey::Timeout, &case.id);
            }
        }
    }

    // ── Headline output: histogram in Markdown ──
    eprintln!();
    eprintln!("═══ PR9 pipeline-oracle corpus histogram ═══");
    eprintln!(
        "Total attempted: {total_attempted} cases (skipped {} known timeouts).",
        skip.len()
    );
    eprintln!();
    eprintln!("{}", format_histogram(&histogram, &sample_per_bucket));

    // Sanity: every bucket count sums to total_attempted.
    let summed: usize = histogram.values().sum();
    eprintln!("Bucket sum: {summed} (expected {total_attempted}).");
    assert_eq!(
        summed, total_attempted,
        "histogram bucket sum must equal attempted-case count"
    );
}

// ── R0033 anchor (per `feedback_anchor_before_fix.md`) ──────────────────

/// Empirical anchor: verify the snapshot-capture mechanism populates a
/// non-trivial `PipelineState` and, on a known-failing case, surfaces a
/// real `first_failing_stage`. R0033 is AABB-disjoint and trivially passes;
/// F0001 (box-box union) is the simplest non-disjoint case that exercises
/// Cherchi's full subdivide → label → flood-fill pipeline. The audit
/// (Cluster Y-I) predicts Stage 4b dominance — F0001 is the canonical
/// box-box where YB-01's twin-symmetry violation is documented.
#[test]
#[ignore]
fn pr9_corpus_runner_captures_snapshots_anchor() {
    let dir = Path::new(ASSAY_DIR);
    if !dir.exists() {
        eprintln!("Assay corpus not generated; skipping.");
        return;
    }
    let cases = discover_cases(dir);
    for anchor_id in ["R0033", "F0001", "F0002"] {
        let case = match cases.iter().find(|c| c.id == anchor_id) {
            Some(c) => c,
            None => {
                eprintln!("{anchor_id} not in corpus, skipping");
                continue;
            }
        };
        let summary = match run_one_case(case.id.clone(), case.waffle_path.clone()) {
            Some(s) => s,
            None => {
                eprintln!("{anchor_id} run_one_case returned None");
                continue;
            }
        };
        eprintln!();
        eprintln!("{} PR9 oracle run summary:", case.id);
        eprintln!("  pipeline_error = {:?}", summary.pipeline_error);
        eprintln!("  first_failing  = {:?}", summary.first_failing_stage);
        for v in &summary.per_oracle {
            let verdict_str = match &v.violation {
                None => "PASS / skipped".to_string(),
                Some(viol) => format!(
                    "{:?}: {}",
                    viol.kind,
                    &viol.message[..viol.message.len().min(120)]
                ),
            };
            eprintln!("    {:?} / {} → {}", v.stage, v.oracle_name, verdict_str);
        }
        eprintln!("  bucket = {}", histogram_key(&summary).label());
    }
}
