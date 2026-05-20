//! Public diagnostic surface for the Yang 2025 hybrid B-Rep/mesh boolean
//! pipeline.
//!
//! This module re-exports the per-stage oracle types and the corpus-runner
//! entry point so external test crates (e.g. `test-harness`) can run the
//! PR9 oracle registry against live boolean operations without needing
//! `pub(crate)` access to `crate::boolean`.
//!
//! PR9 instrumentation; not stable API. Future PRs may revise the snapshot
//! capture mechanism (currently a thread-local in
//! `crate::boolean::pipeline_oracles`).

use crate::boolean::pipeline_oracles::{
    default_oracle_registry, run_pipeline_oracles, with_snapshot_collector, OwnedSnapshotBundle,
};
use crate::types::{KernelError, KernelId};

pub use crate::boolean::pipeline_oracles::{OracleViolation, ViolationKind, YangStage};

// Re-export the conformal-mesh oracle so external integration tests
// (`crates/test-harness/tests/cherchi2022_reference_parity.rs`,
// `crates/test-harness/tests/cherchi_inputcheck_corpus_sweep.rs`) can
// run the same well-formedness check on Cherchi 2022 sidecar output that
// the in-pipeline probes use on Stages A/B/C.
pub use crate::boolean::oracles::conformal_mesh::{
    check_conformal, ConformalReport, MultiPairedEdge, UnpairedEdge,
};

/// Boolean operation selector for [`yang_oracle_run`]. Mirrors the
/// `pub(crate)` `BoolOp` enum in `crate::boolean`; exposed publicly so
/// external test crates can request a diagnostic run without touching
/// kernel internals.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum YangBoolOp {
    Union,
    Subtract,
    Intersect,
}

impl YangBoolOp {
    pub(crate) fn to_internal(self) -> crate::boolean::BoolOp {
        match self {
            YangBoolOp::Union => crate::boolean::BoolOp::Union,
            YangBoolOp::Subtract => crate::boolean::BoolOp::Subtract,
            YangBoolOp::Intersect => crate::boolean::BoolOp::Intersect,
        }
    }
}

/// Per-oracle verdict bundled with stage + name for the public summary.
///
/// `Result<(), OracleViolation>` is collapsed to `Option<OracleViolation>`
/// (None = oracle passed or self-skipped) so callers can iterate without
/// matching on `Result`.
#[derive(Debug)]
pub struct OracleVerdict {
    pub stage: YangStage,
    pub oracle_name: &'static str,
    pub violation: Option<OracleViolation>,
}

/// Public summary of one corpus oracle run.
#[derive(Debug)]
pub struct OracleRunSummary {
    pub case_id: String,
    pub per_oracle: Vec<OracleVerdict>,
    pub first_failing_stage: Option<YangStage>,
    /// `Some(err)` if the Yang pipeline itself errored before producing
    /// any snapshots (e.g. tessellation failure, NotSupported guard).
    /// When set, oracles run on whatever partial state the pipeline
    /// captured, which may be empty.
    pub pipeline_error: Option<String>,
}

/// Install a thread-local snapshot collector, invoke `f`, then take the
/// populated bundle and run the PR9 default oracle registry against it.
/// `case_id` is recorded verbatim in the result for histogram bucketing.
///
/// Use this entry point when you don't have explicit operand handles but
/// do have a callable that triggers a Yang boolean somewhere inside
/// (e.g. `LoadProject` replaying a `.waffle` case). The bundle captures
/// the LAST Yang boolean executed during `f` — earlier booleans are
/// overwritten.
///
/// `f`'s return value is forwarded so callers can inspect it (e.g. check
/// for `LoadProject` errors). PR9 instrumentation; not stable API.
pub fn with_yang_oracle_capture<F, R>(case_id: &str, f: F) -> (OracleRunSummary, R)
where
    F: FnOnce() -> R,
{
    let (bundle, fn_result) = with_snapshot_collector(f);
    let summary = run_oracles_on_bundle(case_id, &bundle, None);
    (summary, fn_result)
}

/// Like [`with_yang_oracle_capture`] but ALSO returns the raw bijectivity
/// reports (per-pair `NonBijectivePair` records) for detailed diagnosis.
/// Used by Stage 1 fix work where the summary message is insufficient and
/// we need to see WHICH face pairs fail and their sample unmatched edges.
///
/// Returns `(summary, Some((report_a, report_b)), fn_result)` when Stage 1
/// was snapshotted; `(summary, None, fn_result)` otherwise. The two
/// `BijectivityReport`s correspond to operand A and operand B of the LAST
/// Yang boolean executed during `f`.
pub fn with_yang_oracle_capture_bijective<F, R>(
    case_id: &str,
    f: F,
) -> (
    OracleRunSummary,
    Option<(
        crate::tessellation::bijective::BijectivityReport,
        crate::tessellation::bijective::BijectivityReport,
    )>,
    R,
)
where
    F: FnOnce() -> R,
{
    let (bundle, fn_result) = with_snapshot_collector(f);
    let bij = crate::boolean::pipeline_oracles::BijectiveFacePairOracle::raw_reports(
        &bundle.as_pipeline_state(),
    );
    let summary = run_oracles_on_bundle(case_id, &bundle, None);
    (summary, bij, fn_result)
}

/// Internal helper: run the default registry against an owned snapshot
/// bundle, return the public summary type.
fn run_oracles_on_bundle(
    case_id: &str,
    bundle: &OwnedSnapshotBundle,
    pipeline_error: Option<String>,
) -> OracleRunSummary {
    let state = bundle.as_pipeline_state();
    let registry = default_oracle_registry();
    let result = run_pipeline_oracles(case_id, &state, &registry);
    let per_oracle = result
        .per_oracle
        .into_iter()
        .map(|(stage, name, verdict)| OracleVerdict {
            stage,
            oracle_name: name,
            violation: verdict.err(),
        })
        .collect();
    OracleRunSummary {
        case_id: result.case_id,
        per_oracle,
        first_failing_stage: result.first_failing_stage,
        pipeline_error,
    }
}

/// Run the PR9 default oracle registry against a Yang boolean operation
/// between two stored solids.
///
/// Internally:
/// 1. Installs a thread-local snapshot collector
///    (`pipeline_oracles::with_snapshot_collector`).
/// 2. Invokes the kernel's Yang boolean inner pipeline on the two solids.
/// 3. Takes the populated `OwnedSnapshotBundle`, converts to a borrowed
///    `PipelineState`, and runs the 6-oracle default registry.
///
/// Production callers do NOT install a collector, so the instrumentation
/// hot-path is a single thread-local null check at each stage boundary.
///
/// `case_id` is included verbatim in the returned summary's `case_id`
/// field for histogram bucketing.
///
/// PR9 instrumentation; not stable API.
pub fn yang_oracle_run(
    kernel: &crate::WaffleKernel,
    handle_a: &crate::KernelSolidHandle,
    handle_b: &crate::KernelSolidHandle,
    op: YangBoolOp,
    case_id: &str,
) -> Result<OracleRunSummary, KernelError> {
    let solid_a = kernel
        .solid_by_handle(handle_a)
        .ok_or(KernelError::EntityNotFound {
            id: KernelId(handle_a.id()),
        })?;
    let solid_b = kernel
        .solid_by_handle(handle_b)
        .ok_or(KernelError::EntityNotFound {
            id: KernelId(handle_b.id()),
        })?;

    let mut next_id = u64::MAX / 2; // Avoid colliding with kernel-allocated IDs.
    let mut id_alloc = || {
        let id = next_id;
        next_id += 1;
        id
    };

    // Capture snapshots while running the inner Yang pipeline. The
    // pipeline's success/failure is captured separately — even on error,
    // any partial snapshots that DID land in the bundle are kept so the
    // oracles can attribute the failure to a specific stage.
    let internal_op = op.to_internal();
    let solid_a_clone = solid_a.clone();
    let solid_b_clone = solid_b.clone();

    let (bundle, pipeline_outcome) = with_snapshot_collector(move || {
        // Catch panics inside the pipeline: a panicking pipeline still
        // leaves the snapshot bundle in place (RAII), so oracles can
        // diagnose the panic-causing stage from upstream snapshots.
        let result = std::panic::catch_unwind(std::panic::AssertUnwindSafe(|| {
            crate::boolean::yang_integration::yang_boolean_inner(
                &solid_a_clone,
                &solid_b_clone,
                internal_op,
                &mut id_alloc,
            )
        }));
        match result {
            Ok(Ok(_)) => Ok(()),
            Ok(Err(e)) => Err(format!("{e}")),
            Err(panic_payload) => {
                let msg = panic_payload
                    .downcast_ref::<&str>()
                    .copied()
                    .or_else(|| panic_payload.downcast_ref::<String>().map(|s| s.as_str()))
                    .unwrap_or("<non-string panic>");
                Err(format!("PANIC: {msg}"))
            }
        }
    });

    Ok(run_oracles_on_bundle(
        case_id,
        &bundle,
        pipeline_outcome.err(),
    ))
}
