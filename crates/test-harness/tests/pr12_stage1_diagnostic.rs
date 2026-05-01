//! PR12 — Stage 1 oracle diagnostic capture for the 15 first-fail cases.
//!
//! AUDIT-ONLY scaffolding. Iterates the 15 Stage 1 first-fail cases identified
//! by PR11's adversary report (`docs/audits/pr11_adversary_validation.md` §5 F1)
//! and captures per-case `BijectiveFacePairOracle` verdicts plus the cross-stage
//! co-fire pattern (which other oracles fire `ContractViolated` on the same
//! case). Output drives PR12's cluster classification.
//!
//! The original plan §"Phase A T2" defined three clusters keyed on Stage 4b
//! also firing. Empirically, post-PR11 Stage 4b is `Ok` on EVERY one of these
//! 15 cases (the PR11 per-patch labeling fix made S4b structurally correct).
//! The taxonomy is therefore reframed against Stage 2 (the next-most-relevant
//! cascade signal):
//!
//! - **Cluster X (cascade-with-arrangement-collapse)**: S1 + S2 + S6 fire.
//!   Tessellation defect propagates into the Cherchi mesh arrangement
//!   (Stage 2 conservation count fails).
//! - **Cluster Y (decoupled-tessellation-only)**: S1 + S6 fire, S2 = Ok.
//!   Tessellation defect is preserved through the arrangement (S2 conserves
//!   tri counts) but the resulting half-edge twin pairing in the result
//!   topology is broken (S6 fails).
//! - **Cluster Z (other)**: anything else (e.g., S1 doesn't fire — empty in
//!   the corpus PR12 measurement).
//!
//! `#[ignore]` (long-running ~30s on 15 cases; manual invocation by the
//! diagnose driver). Audit artifact only — this file does not modify
//! production code and is not part of the regression suite.
//!
//! Refs:
//! - `/home/claude/.claude/plans/fluttering-rolling-crystal.md` (PR12 plan).
//! - `docs/audits/pr11_adversary_validation.md` §5 F1 (15-case list).
//! - `docs/audits/oracle_validity_task_c_pairing.md` (PR10 Task C TRACE format).
//! - `crates/kernel/src/tessellation/bijective.rs` (`NonBijectivePair` struct).
//! - `crates/kernel/src/boolean/pipeline_oracles.rs::BijectiveFacePairOracle`.
//!
//! Companion report: `docs/audits/pr12_stage1_diagnostic.md`.

use std::path::Path;
use std::sync::mpsc;
use std::thread;
use std::time::Duration;

use kernel::diagnostics::{with_yang_oracle_capture, OracleRunSummary, ViolationKind, YangStage};
use wasm_bridge::messages::UiToEngine;
use wasm_bridge::{dispatch, EngineState};

const ASSAY_DIR: &str = "../../app/tests/cases/assay";
const PER_CASE_TIMEOUT: Duration = Duration::from_secs(60);

/// The 15 Stage 1 first-fail cases per PR11 adversary §5 F1.
/// 2 pre-existing in PR10 (R0031, R0081) + 13 unmasked by PR11.
const STAGE1_CASES: &[&str] = &[
    "R0007", "R0014", "R0020", "R0021", "R0031", "R0034", "R0035", "R0046", "R0063", "R0081",
    "R0095", "F0016", "F0018", "F0019", "F0076",
];

/// All six oracle stages in pipeline order.
const ORACLE_STAGES: &[YangStage] = &[
    YangStage::Stage0Coplanar,
    YangStage::Stage1Bijective,
    YangStage::Stage2Arrangement,
    YangStage::Stage4bClassification,
    YangStage::Stage5PatchSegment,
    YangStage::Stage6Assembly,
];

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum CellVerdict {
    Ok,
    ContractViolated,
    StateMissing,
    OracleStub,
}

impl CellVerdict {
    fn label(self) -> &'static str {
        match self {
            CellVerdict::Ok => ".",
            CellVerdict::ContractViolated => "X",
            CellVerdict::StateMissing => "M",
            CellVerdict::OracleStub => "S",
        }
    }
}

#[derive(Debug, Clone)]
struct CaseRecord {
    case_id: String,
    /// Per-oracle verdict in `ORACLE_STAGES` order (s0, s1, s2, s4b, s5, s6).
    verdicts: Vec<CellVerdict>,
    /// Stage 1 violation message (if any) — captures the human-readable
    /// pair-count summary from `BijectiveFacePairOracle::check`.
    stage1_message: Option<String>,
    /// Stage 0 violation message (if any) — useful for understanding when
    /// the coplanar preprocessing fired.
    stage0_message: Option<String>,
    /// Pipeline error if the boolean op errored before producing snapshots.
    pipeline_error: Option<String>,
    /// Whether this case timed out.
    timed_out: bool,
}

fn project_verdicts(summary: &OracleRunSummary) -> (Vec<CellVerdict>, Option<String>, Option<String>) {
    let mut row = Vec::with_capacity(ORACLE_STAGES.len());
    let mut stage1_msg = None;
    let mut stage0_msg = None;
    for stage in ORACLE_STAGES {
        let v = summary
            .per_oracle
            .iter()
            .find(|v| v.stage == *stage)
            .map(|v| {
                let cell = match &v.violation {
                    None => CellVerdict::Ok,
                    Some(viol) => match viol.kind {
                        ViolationKind::ContractViolated => CellVerdict::ContractViolated,
                        ViolationKind::StateMissing => CellVerdict::StateMissing,
                        ViolationKind::OracleStub => CellVerdict::OracleStub,
                    },
                };
                if matches!(cell, CellVerdict::ContractViolated) {
                    if let Some(viol) = &v.violation {
                        match stage {
                            YangStage::Stage1Bijective => {
                                stage1_msg = Some(viol.message.clone());
                            }
                            YangStage::Stage0Coplanar => {
                                stage0_msg = Some(viol.message.clone());
                            }
                            _ => {}
                        }
                    }
                }
                cell
            })
            .unwrap_or(CellVerdict::Ok);
        row.push(v);
    }
    (row, stage1_msg, stage0_msg)
}

/// Run one case end-to-end with snapshot capture.
fn run_one_case(case_id: String, waffle_path: std::path::PathBuf) -> Option<OracleRunSummary> {
    let waffle_json = std::fs::read_to_string(&waffle_path).ok()?;
    std::env::set_var("YANG_BOOLEAN", "1");

    let id_for_capture = case_id.clone();
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

/// Stage 1 first-fail clustering buckets, reframed for the post-PR11 corpus
/// (Stage 4b is structurally `Ok` per the PR11 per-patch labeling refactor).
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum Cluster {
    /// S1 + S2 + S6 all fire: tessellation defect collapses the Cherchi
    /// arrangement (Stage 2 conservation fails).
    XCascade,
    /// S1 + S6 fire, S2 = Ok: tessellation defect preserved through
    /// arrangement; result-topology half-edge twin pairing fails (S6).
    YDecoupled,
    /// S1 doesn't fire (or any other unmodeled pattern).
    ZOther,
}

impl Cluster {
    fn label(self) -> &'static str {
        match self {
            Cluster::XCascade => "X (1+2+6)",
            Cluster::YDecoupled => "Y (1+6, S2=Ok)",
            Cluster::ZOther => "Z (other)",
        }
    }
}

fn classify_cluster(verdicts: &[CellVerdict]) -> Cluster {
    let s1_idx = ORACLE_STAGES
        .iter()
        .position(|s| *s == YangStage::Stage1Bijective)
        .unwrap();
    let s2_idx = ORACLE_STAGES
        .iter()
        .position(|s| *s == YangStage::Stage2Arrangement)
        .unwrap();
    let s6_idx = ORACLE_STAGES
        .iter()
        .position(|s| *s == YangStage::Stage6Assembly)
        .unwrap();

    let s1 = matches!(verdicts[s1_idx], CellVerdict::ContractViolated);
    let s2 = matches!(verdicts[s2_idx], CellVerdict::ContractViolated);
    let s6 = matches!(verdicts[s6_idx], CellVerdict::ContractViolated);

    if !s1 {
        return Cluster::ZOther;
    }
    if s2 && s6 {
        Cluster::XCascade
    } else if !s2 && s6 {
        Cluster::YDecoupled
    } else {
        Cluster::ZOther
    }
}

/// Collect records for the 15 cases. Returns the per-case records plus a
/// list of cases that timed out or failed to load.
fn collect_records() -> Vec<CaseRecord> {
    let dir = Path::new(ASSAY_DIR);
    if !dir.exists() {
        eprintln!("Assay corpus directory not found at {ASSAY_DIR}; skipping.");
        return Vec::new();
    }

    let mut records = Vec::new();

    for case_id in STAGE1_CASES {
        let waffle_path = dir.join(format!("{case_id}.waffle"));
        if !waffle_path.exists() {
            eprintln!("[ANCHOR] missing waffle for {case_id} at {waffle_path:?}; skipping");
            records.push(CaseRecord {
                case_id: case_id.to_string(),
                verdicts: vec![CellVerdict::Ok; ORACLE_STAGES.len()],
                stage1_message: None,
                stage0_message: None,
                pipeline_error: Some("missing .waffle".to_string()),
                timed_out: false,
            });
            continue;
        }

        eprintln!("[ANCHOR] running case {case_id}");

        let (tx, rx) = mpsc::channel::<Option<OracleRunSummary>>();
        let case_id_for_thread = case_id.to_string();
        let waffle_path_for_thread = waffle_path.clone();
        let _handle = thread::spawn(move || {
            let result = std::panic::catch_unwind(std::panic::AssertUnwindSafe(|| {
                run_one_case(case_id_for_thread, waffle_path_for_thread)
            }));
            let _ = tx.send(result.unwrap_or(None));
        });

        match rx.recv_timeout(PER_CASE_TIMEOUT) {
            Ok(Some(summary)) => {
                let (verdicts, stage1_msg, stage0_msg) = project_verdicts(&summary);
                records.push(CaseRecord {
                    case_id: case_id.to_string(),
                    verdicts,
                    stage1_message: stage1_msg,
                    stage0_message: stage0_msg,
                    pipeline_error: summary.pipeline_error,
                    timed_out: false,
                });
            }
            Ok(None) => {
                eprintln!("[ANCHOR] case {case_id} returned None (load error or panic)");
                records.push(CaseRecord {
                    case_id: case_id.to_string(),
                    verdicts: vec![CellVerdict::Ok; ORACLE_STAGES.len()],
                    stage1_message: None,
                    stage0_message: None,
                    pipeline_error: Some("worker returned None".to_string()),
                    timed_out: false,
                });
            }
            Err(_) => {
                eprintln!("[ANCHOR] case {case_id} TIMED OUT after {PER_CASE_TIMEOUT:?}");
                records.push(CaseRecord {
                    case_id: case_id.to_string(),
                    verdicts: vec![CellVerdict::Ok; ORACLE_STAGES.len()],
                    stage1_message: None,
                    stage0_message: None,
                    pipeline_error: None,
                    timed_out: true,
                });
            }
        }
    }

    records
}

/// Emit the standard Task-C-style TRACE line + Stage 1 message body.
fn dump_records(records: &[CaseRecord], header: &str) {
    eprintln!();
    eprintln!("═══ {header}: per-case verdict trace ═══");
    eprintln!("# format: case_id | s0 s1 s2 s4b s5 s6 (verdicts) | cluster | stage1 message");
    eprintln!("# verdict legend: . = Ok/skip, X = ContractViolated, M = StateMissing, S = OracleStub");
    for r in records {
        let row: String = r
            .verdicts
            .iter()
            .map(|v| v.label())
            .collect::<Vec<_>>()
            .join(" ");
        let cluster = classify_cluster(&r.verdicts).label();
        let s1_msg = r.stage1_message.as_deref().unwrap_or("(no s1 violation)");
        let s0_msg = r
            .stage0_message
            .as_deref()
            .map(|m| format!(" | s0=`{m}`"))
            .unwrap_or_default();
        let timeout_marker = if r.timed_out { " [TIMEOUT]" } else { "" };
        let pipeline_err = r
            .pipeline_error
            .as_deref()
            .map(|e| format!(" pipeline_error=`{e}`"))
            .unwrap_or_default();
        eprintln!(
            "TRACE | {:>5} | {} | {:<28} | {}{}{}{}",
            r.case_id, row, cluster, s1_msg, s0_msg, timeout_marker, pipeline_err,
        );
    }
}

fn dump_cluster_breakdown(records: &[CaseRecord], header: &str) {
    let mut x = 0usize;
    let mut y = 0usize;
    let mut z = 0usize;
    let mut x_ids = Vec::new();
    let mut y_ids = Vec::new();
    let mut z_ids = Vec::new();
    for r in records {
        match classify_cluster(&r.verdicts) {
            Cluster::XCascade => {
                x += 1;
                x_ids.push(r.case_id.clone());
            }
            Cluster::YDecoupled => {
                y += 1;
                y_ids.push(r.case_id.clone());
            }
            Cluster::ZOther => {
                z += 1;
                z_ids.push(r.case_id.clone());
            }
        }
    }
    eprintln!();
    eprintln!("═══ {header}: cluster breakdown ═══");
    eprintln!("Cluster X (S1 + S2 + S6 fire): {x} | {}", x_ids.join(", "));
    eprintln!(
        "Cluster Y (S1 + S6 fire, S2 = Ok): {y} | {}",
        y_ids.join(", ")
    );
    eprintln!("Cluster Z (other / S1 not firing): {z} | {}", z_ids.join(", "));
}

/// Embedded PR10 baseline reference table.
///
/// Captured by re-running this probe at commit `c2e473c` (the pre-PR11 main
/// commit) on 2026-05-01. Every entry is the verbatim verdict tuple
/// (s0, s1, s2, s4b, s5, s6) the PR10 oracle produced for the same case.
/// Includes a second deterministic-check run to confirm the S1 fire/no-fire
/// signal is stable (counts within S1 messages flap due to upstream
/// HashMap RandomState, but the binary fired-or-not verdict matches across
/// both PR10 runs).
///
/// Format: (case_id, S1_fired_in_PR10, full_verdict_tuple_string)
const PR10_BASELINE_VERDICTS: &[(&str, bool, &str)] = &[
    ("R0007", false, ". . . X . X"),
    ("R0014", true, ". X . . . X"),
    ("R0020", false, ". . X X . X"),
    ("R0021", false, ". . . X . X"),
    ("R0031", true, "S X X X . X"),
    ("R0034", true, ". X . X . X"),
    ("R0035", true, ". X . X . X"),
    ("R0046", false, ". . . X . X"),
    ("R0063", false, ". . . X . X"),
    ("R0081", true, "S X . X . X"),
    ("R0095", false, ". . . X . X"),
    ("F0016", true, ". X . . . X"),
    ("F0018", true, ". X . . . X"),
    ("F0019", true, ". X . X . X"),
    ("F0076", false, ". . . X . X"),
];

/// Compare PR12 records against the embedded PR10 baseline to determine
/// per-case provenance: cascade-unmasked (S1 was already firing in PR10)
/// vs PR11-introduced (S1 newly fires in PR12).
fn dump_pr10_comparison(records: &[CaseRecord]) {
    eprintln!();
    eprintln!("═══ PR10-baseline-vs-PR12 Stage 1 first-fail provenance ═══");
    eprintln!(
        "Per case: was S1 already firing in PR10 (cascade unmask) or did \
         PR11 introduce the S1 firing (regression)?"
    );
    eprintln!();
    eprintln!(
        "| {:<5} | {:<14} | {:<14} | {:<28} |",
        "case", "PR10 verdicts", "PR12 verdicts", "provenance"
    );
    eprintln!(
        "|-------|----------------|----------------|------------------------------|"
    );
    let s1_idx = ORACLE_STAGES
        .iter()
        .position(|s| *s == YangStage::Stage1Bijective)
        .unwrap();

    let mut cascade_unmask = Vec::new();
    let mut pr11_regression = Vec::new();
    for r in records {
        let pr12_row: String = r
            .verdicts
            .iter()
            .map(|v| v.label())
            .collect::<Vec<_>>()
            .join(" ");
        let baseline = PR10_BASELINE_VERDICTS
            .iter()
            .find(|(cid, _, _)| *cid == r.case_id);
        let (pr10_s1, pr10_row) = match baseline {
            Some((_, s1, row)) => (*s1, *row),
            None => (false, "(missing)"),
        };
        let pr12_s1 = matches!(r.verdicts[s1_idx], CellVerdict::ContractViolated);
        let prov = match (pr10_s1, pr12_s1) {
            (true, true) => {
                cascade_unmask.push(r.case_id.clone());
                "CASCADE (S1 firing in both)"
            }
            (false, true) => {
                pr11_regression.push(r.case_id.clone());
                "PR11-INTRODUCED (S1 newly fires)"
            }
            (true, false) => "PR12-FIXED (S1 was firing, now Ok)",
            (false, false) => "NEITHER (S1 never fires)",
        };
        eprintln!(
            "| {:<5} | {:<14} | {:<14} | {:<28} |",
            r.case_id, pr10_row, pr12_row, prov
        );
    }
    eprintln!();
    eprintln!(
        "CASCADE unmasking (S1 already firing in PR10): {} cases — {}",
        cascade_unmask.len(),
        cascade_unmask.join(", ")
    );
    eprintln!(
        "PR11-INTRODUCED (S1 newly fires in PR12): {} cases — {}",
        pr11_regression.len(),
        pr11_regression.join(", ")
    );
}

/// Stage 1 verdict summary: how many of the records have S1 fire / Ok / skip.
fn stage1_verdict_summary(records: &[CaseRecord]) -> (usize, usize, usize, Vec<String>) {
    let s1_idx = ORACLE_STAGES
        .iter()
        .position(|s| *s == YangStage::Stage1Bijective)
        .unwrap();
    let mut fired = 0usize;
    let mut ok = 0usize;
    let mut other = 0usize;
    let mut fired_ids = Vec::new();
    for r in records {
        match r.verdicts[s1_idx] {
            CellVerdict::ContractViolated => {
                fired += 1;
                fired_ids.push(r.case_id.clone());
            }
            CellVerdict::Ok => ok += 1,
            _ => other += 1,
        }
    }
    (fired, ok, other, fired_ids)
}

#[test]
#[ignore]
fn pr12_stage1_diagnostic_capture() {
    eprintln!("═══ PR12 Stage 1 oracle diagnostic — 15 first-fail cases ═══");
    eprintln!(
        "Cases ({}): {}",
        STAGE1_CASES.len(),
        STAGE1_CASES.join(", ")
    );

    let records = collect_records();
    if records.is_empty() {
        eprintln!("No records collected; aborting.");
        return;
    }

    dump_records(&records, "PR12 (current branch HEAD)");
    dump_cluster_breakdown(&records, "PR12");
    dump_pr10_comparison(&records);

    let (fired, ok, other, fired_ids) = stage1_verdict_summary(&records);
    eprintln!();
    eprintln!("═══ PR12 Stage 1 oracle verdict summary ═══");
    eprintln!("ContractViolated: {fired} / {} ({})", records.len(), fired_ids.join(", "));
    eprintln!("Ok: {ok} / {}", records.len());
    eprintln!("Other (StateMissing/OracleStub): {other} / {}", records.len());

    eprintln!();
    eprintln!("[ANCHOR] PR12 probe complete. {} records.", records.len());
}
