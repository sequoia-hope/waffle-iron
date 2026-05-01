//! Oracle-validity audit (PR10) — Task B: known-PASS verification probe.
//!
//! AUDIT-ONLY scaffolding. Iterates the 8 audit-class PASS cases, runs all
//! 6 PR9 oracles via `with_yang_oracle_capture`, and emits an 8x6 verdict
//! matrix to stderr. Any `VIOLATION` cell on a known-PASS case is a
//! false-positive finding (or an assay mislabel — the report classifies).
//!
//! The 8 cases come from `docs/audits/yang_audit_b_assay_failures.md` §2:
//!
//! - F0001/F0003/F0007/F0051/F0053 — trivial-merge passes (identical/
//!   concentric extrudes → no real boolean).
//! - F0073/F0074 — `expect_rebuild_error: true` (axis-touching profile);
//!   pipeline doesn't run, oracles must self-skip.
//! - R0018 — nondeterministic flapper per `feedback_no_regression_chasing.md`;
//!   record observed verdict but do not over-interpret.
//!
//! `#[ignore]` (long-running; manual invocation by audit driver).
//!
//! Refs: `specs/pipeline_oracles.md` (PR9 spec under audit);
//! `docs/audits/oracle_validity_task_b_passcheck.md` (deliverable).

use std::path::Path;

use kernel::diagnostics::{
    with_yang_oracle_capture, OracleRunSummary, ViolationKind, YangStage,
};
use test_harness::assay::randomized_runner::discover_cases;
use wasm_bridge::messages::UiToEngine;
use wasm_bridge::{dispatch, EngineState};

const ASSAY_DIR: &str = "../../app/tests/cases/assay";

/// The 8 audit-class PASS cases per `yang_audit_b_assay_failures.md` §2.
const PASS_CASES: &[&str] = &[
    "F0001", "F0003", "F0007", "F0051", "F0053", "F0073", "F0074", "R0018",
];

/// One cell of the 8x6 verdict matrix.
#[derive(Debug, Clone, Copy)]
enum CellVerdict {
    /// Oracle passed (or self-skipped on missing snapshot).
    Ok,
    /// Oracle reported `StateMissing` — snapshot absent and that is itself
    /// a contract violation per the oracle.
    SkipStateMissing,
    /// Oracle reported `OracleStub` — known coverage gap (Stage 0 partial-
    /// overlap unchecked).
    StubOracle,
    /// Oracle reported `ContractViolated` — this is a finding on a
    /// known-PASS case.
    Violation,
}

impl CellVerdict {
    fn label(self) -> &'static str {
        match self {
            CellVerdict::Ok => "Ok",
            CellVerdict::SkipStateMissing => "Skip(StateMissing)",
            CellVerdict::StubOracle => "Stub(OracleStub)",
            CellVerdict::Violation => "VIOLATION",
        }
    }
}

/// All six oracle stages in pipeline order. Used to project the
/// per-oracle verdicts of one case onto a fixed-width row.
const ORACLE_STAGES: &[YangStage] = &[
    YangStage::Stage0Coplanar,
    YangStage::Stage1Bijective,
    YangStage::Stage2Arrangement,
    YangStage::Stage4bClassification,
    YangStage::Stage5PatchSegment,
    YangStage::Stage6Assembly,
];

/// Project the summary's per-oracle verdicts onto the fixed `ORACLE_STAGES`
/// order. Returns `(row, final_verdict)` where `final_verdict` is the
/// rolled-up "AllPass / VIOLATION / Skip" for the row.
fn project_row(summary: &OracleRunSummary) -> (Vec<CellVerdict>, &'static str) {
    let mut row = Vec::with_capacity(ORACLE_STAGES.len());
    let mut any_violation = false;
    let mut any_state_missing = false;
    let mut any_stub = false;

    for stage in ORACLE_STAGES {
        let verdict = summary
            .per_oracle
            .iter()
            .find(|v| v.stage == *stage)
            .map(|v| match &v.violation {
                None => CellVerdict::Ok,
                Some(viol) => match viol.kind {
                    ViolationKind::StateMissing => CellVerdict::SkipStateMissing,
                    ViolationKind::OracleStub => CellVerdict::StubOracle,
                    ViolationKind::ContractViolated => CellVerdict::Violation,
                },
            })
            .unwrap_or(CellVerdict::Ok);
        match verdict {
            CellVerdict::Violation => any_violation = true,
            CellVerdict::SkipStateMissing => any_state_missing = true,
            CellVerdict::StubOracle => any_stub = true,
            CellVerdict::Ok => {}
        }
        row.push(verdict);
    }

    let final_verdict = if any_violation {
        "VIOLATION"
    } else if any_state_missing {
        "Skip"
    } else if any_stub {
        "AllPass*"
    } else {
        "AllPass"
    };
    (row, final_verdict)
}

/// Run one case through the full oracle battery, returning the summary.
fn probe_one_case(case_id: &str, waffle_path: &Path) -> Option<OracleRunSummary> {
    let waffle_json = std::fs::read_to_string(waffle_path).ok()?;
    std::env::set_var("YANG_BOOLEAN", "1");

    let id_for_capture = case_id.to_string();
    let (summary, _load_outcome) = with_yang_oracle_capture(&id_for_capture, move || {
        let mut state = EngineState::new();
        let mut kernel_inst = kernel::WaffleKernel::new();
        let response = dispatch(
            &mut state,
            UiToEngine::LoadProject { data: waffle_json },
            &mut kernel_inst,
        );
        let _ = response;
        state.engine.errors.clone()
    });
    Some(summary)
}

#[test]
#[ignore]
fn oracle_validity_pr10_known_pass_verification() {
    let dir = Path::new(ASSAY_DIR);
    if !dir.exists() {
        eprintln!("Assay corpus not generated; skipping.");
        return;
    }
    let cases = discover_cases(dir);
    if cases.is_empty() {
        eprintln!("Assay corpus is empty; skipping.");
        return;
    }

    eprintln!();
    eprintln!("═══ Oracle-validity audit Task B — known-PASS verification ═══");
    eprintln!();

    // Header row.
    eprintln!(
        "| {:<6} | {:<18} | {:<18} | {:<18} | {:<18} | {:<18} | {:<18} | {:<10} |",
        "Case",
        "Stage 0 Coplanar",
        "Stage 1 Bijective",
        "Stage 2 Arrang.",
        "Stage 4b Classif.",
        "Stage 5 PatchSeg",
        "Stage 6 Assembly",
        "Final"
    );
    eprintln!(
        "|--------|--------------------|--------------------|--------------------|--------------------|--------------------|--------------------|------------|"
    );

    let mut findings: Vec<(String, Vec<(YangStage, String)>)> = Vec::new();

    for &case_id in PASS_CASES {
        let case = match cases.iter().find(|c| c.id == case_id) {
            Some(c) => c,
            None => {
                eprintln!(
                    "| {:<6} | (case not found in corpus)                                                                                                          |",
                    case_id
                );
                continue;
            }
        };

        let summary = match probe_one_case(case_id, &case.waffle_path) {
            Some(s) => s,
            None => {
                eprintln!(
                    "| {:<6} | (probe_one_case returned None — file read failure)                                                                                  |",
                    case_id
                );
                continue;
            }
        };

        let (row, final_verdict) = project_row(&summary);

        eprintln!(
            "| {:<6} | {:<18} | {:<18} | {:<18} | {:<18} | {:<18} | {:<18} | {:<10} |",
            case_id,
            row[0].label(),
            row[1].label(),
            row[2].label(),
            row[3].label(),
            row[4].label(),
            row[5].label(),
            final_verdict,
        );

        // Capture violation messages for the report's per-case analysis.
        let viols: Vec<(YangStage, String)> = summary
            .per_oracle
            .iter()
            .filter_map(|v| {
                v.violation.as_ref().and_then(|viol| match viol.kind {
                    ViolationKind::ContractViolated => {
                        Some((v.stage, viol.message.clone()))
                    }
                    _ => None,
                })
            })
            .collect();
        if !viols.is_empty() {
            findings.push((case_id.to_string(), viols));
        }

        // Also dump pipeline_error + per-oracle violation messages for
        // forensic inspection (especially Stage 0's `OracleStub` reason).
        if let Some(err) = &summary.pipeline_error {
            eprintln!("    {} pipeline_error: {}", case_id, err);
        }
        for v in &summary.per_oracle {
            if let Some(viol) = &v.violation {
                let truncated = if viol.message.len() > 200 {
                    &viol.message[..200]
                } else {
                    &viol.message
                };
                eprintln!(
                    "    {} {:?}/{:?}: {}",
                    case_id, v.stage, viol.kind, truncated
                );
            }
        }
    }

    eprintln!();
    eprintln!("═══ Notes ═══");
    eprintln!("- `Ok` cells: oracle's `check()` returned `Ok(())`. Ambiguous");
    eprintln!("  between (a) snapshot present + contract passed and");
    eprintln!("  (b) snapshot None → oracle self-skipped silently.");
    eprintln!("- `Stub(OracleStub)`: known coverage gap, not a failure.");
    eprintln!("- `VIOLATION`: real `ContractViolated`. False-positive row.");
    eprintln!("- `Skip(StateMissing)`: oracle reported missing snapshot AS");
    eprintln!("  the contract violation.");

    eprintln!();
    eprintln!("═══ Findings (false-positives) ═══");
    if findings.is_empty() {
        eprintln!("No `ContractViolated` cells on known-PASS cases. ✓");
    } else {
        for (case_id, viols) in &findings {
            eprintln!("- {}", case_id);
            for (stage, msg) in viols {
                let truncated = if msg.len() > 200 { &msg[..200] } else { msg };
                eprintln!("    {:?}: {}", stage, truncated);
            }
        }
    }
}
