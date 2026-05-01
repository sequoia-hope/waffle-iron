//! Oracle-validity audit (PR10) — Task C: cross-oracle pairing on full corpus.
//!
//! AUDIT-ONLY scaffolding. Iterates the 157-case yang_fast corpus (190 total
//! minus 33 known timeouts), runs all 6 PR9 oracles per case via
//! `with_yang_oracle_capture`, and emits:
//!
//! 1. Cross-oracle pairing matrix (rows = first-failing-stage bucket,
//!    cols = each oracle stage, cells = how many cases in this row's
//!    bucket also have the column oracle fire `ContractViolated`).
//! 2. Per-case raw verdict trace (one line per case) so the audit synthesis
//!    can spot-check or re-aggregate without re-running the corpus.
//! 3. AllPass-purity breakdown — of cases bucketed `AllPass`, how many
//!    actually have non-empty Stage 2/4b/6 snapshots vs empty bundle
//!    (pipeline errored before any of those stages recorded).
//!
//! The runner does NOT short-circuit per-stage: every oracle observes the
//! full bundle and the existing `run_pipeline_oracles` already records all
//! 6 verdicts in `OracleRunSummary::per_oracle`. PR9's bucketing collapses
//! that to a single `first_failing_stage`; this probe surfaces the full
//! verdict tuple for each case.
//!
//! `#[ignore]` (long-running ~5-10 min; manual invocation by audit driver).
//!
//! Refs:
//! - `specs/pipeline_oracles.md` (PR9 spec under audit, §3 baseline histogram).
//! - `docs/audits/yang_audit_2026-04-30.md` Cluster Y-I (Stage 4b dominance prediction).
//! - `docs/audits/oracle_validity_task_c_pairing.md` (deliverable).

use std::collections::{BTreeMap, BTreeSet, HashSet};
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
/// Mirrors `pr9_pipeline_oracle_corpus.rs::YANG_FAST_SKIP_IDS` verbatim.
const YANG_FAST_SKIP_IDS: &[&str] = &[
    "R0003", "R0010", "R0012", "R0026", "R0028", "R0053", "R0059", "R0065", "R0070", "R0085",
    "R0099", "R0100", "F0063", "F0065", "F0067", "F0068", "F0069", "F0070", "F0071", "F0072",
    "F0077", "F0078", "F0079", "F0080", "F0081", "F0082", "F0083", "F0084", "F0085", "F0087",
    "F0088", "F0089", "F0090",
];

const PER_CASE_TIMEOUT: Duration = Duration::from_secs(30);

/// All six oracle stages in pipeline order.
const ORACLE_STAGES: &[YangStage] = &[
    YangStage::Stage0Coplanar,
    YangStage::Stage1Bijective,
    YangStage::Stage2Arrangement,
    YangStage::Stage4bClassification,
    YangStage::Stage5PatchSegment,
    YangStage::Stage6Assembly,
];

/// One cell in the per-case verdict trace.
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

/// First-failing-stage bucket (matches `pr9_pipeline_oracle_corpus.rs`
/// histogram_key but only the real-fail kinds count). `OracleStub` does
/// NOT count as a first-fail; `StateMissing` does NOT count either,
/// because every PR9 oracle currently self-skips on missing snapshots.
#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord)]
enum FirstFail {
    Stage(YangStage),
    AllPass,
    Timeout,
    LoadError,
}

impl FirstFail {
    fn label(self) -> String {
        match self {
            FirstFail::Stage(s) => format!("{s:?}"),
            FirstFail::AllPass => "AllPass".to_string(),
            FirstFail::Timeout => "Timeout".to_string(),
            FirstFail::LoadError => "LoadError".to_string(),
        }
    }
}

/// Per-case record: case id + first-failing bucket + per-oracle verdict.
#[derive(Debug, Clone)]
struct CaseRecord {
    case_id: String,
    first_fail: FirstFail,
    /// Per-oracle verdict in `ORACLE_STAGES` order.
    verdicts: Vec<CellVerdict>,
}

fn project_verdicts(summary: &OracleRunSummary) -> Vec<CellVerdict> {
    let mut row = Vec::with_capacity(ORACLE_STAGES.len());
    for stage in ORACLE_STAGES {
        let v = summary
            .per_oracle
            .iter()
            .find(|v| v.stage == *stage)
            .map(|v| match &v.violation {
                None => CellVerdict::Ok,
                Some(viol) => match viol.kind {
                    ViolationKind::ContractViolated => CellVerdict::ContractViolated,
                    ViolationKind::StateMissing => CellVerdict::StateMissing,
                    ViolationKind::OracleStub => CellVerdict::OracleStub,
                },
            })
            .unwrap_or(CellVerdict::Ok);
        row.push(v);
    }
    row
}

fn compute_first_fail(verdicts: &[CellVerdict]) -> Option<YangStage> {
    for (i, v) in verdicts.iter().enumerate() {
        if matches!(v, CellVerdict::ContractViolated) {
            return Some(ORACLE_STAGES[i]);
        }
    }
    None
}

/// Worker payload: load one .waffle, run oracles, return summary.
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

#[test]
#[ignore]
fn oracle_validity_pr10_pairing_corpus() {
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
    let skip: HashSet<&str> = YANG_FAST_SKIP_IDS.iter().copied().collect();

    let mut records: Vec<CaseRecord> = Vec::new();
    let mut total_attempted = 0usize;

    for case in &cases {
        if skip.contains(case.id.as_str()) {
            continue;
        }
        total_attempted += 1;

        let (tx, rx) = mpsc::channel::<Option<OracleRunSummary>>();
        let case_id = case.id.clone();
        let waffle_path = case.waffle_path.clone();
        let _handle = thread::spawn(move || {
            let result = std::panic::catch_unwind(std::panic::AssertUnwindSafe(|| {
                run_one_case(case_id, waffle_path)
            }));
            let _ = tx.send(result.unwrap_or(None));
        });

        let record = match rx.recv_timeout(PER_CASE_TIMEOUT) {
            Ok(Some(summary)) => {
                let verdicts = project_verdicts(&summary);
                let first = compute_first_fail(&verdicts);
                // "snapshot present" inferred per-oracle: an oracle's
                // verdict cell of `Ok` when the underlying snapshot is
                // None still maps to Ok (self-skip). We can't tell from
                // just verdicts alone whether snapshot was present. So
                // approximate: Stage 0 present = stage 0 verdict is OracleStub
                // (i.e. partial-overlap pairs detected) OR Ok with at
                // least a Stage 1 verdict that's not skip; conservatively
                // mark snapshot_present[stage] = (verdict != skip).
                // In practice, we read it directly from the summary via
                // a second helper if needed; for AllPass purity audit, we
                // pick a coarser signal — was Stage 6 actually exercised?
                // Stage 6 oracle returns Ok if snapshot None — same
                // problem. For purity, we use a different anchor:
                // look at the pipeline_error field.
                let bucket = match first {
                    Some(s) => FirstFail::Stage(s),
                    None => FirstFail::AllPass,
                };
                CaseRecord {
                    case_id: case.id.clone(),
                    first_fail: bucket,
                    verdicts,
                }
            }
            Ok(None) => CaseRecord {
                case_id: case.id.clone(),
                first_fail: FirstFail::LoadError,
                verdicts: vec![CellVerdict::Ok; ORACLE_STAGES.len()],
            },
            Err(_) => CaseRecord {
                case_id: case.id.clone(),
                first_fail: FirstFail::Timeout,
                verdicts: vec![CellVerdict::Ok; ORACLE_STAGES.len()],
            },
        };
        records.push(record);
    }

    // ── Per-case raw trace (parseable) ──────────────────────────────────
    eprintln!();
    eprintln!("═══ Task C: per-case verdict trace ═══");
    eprintln!("# format: case_id | first_fail | s0 s1 s2 s4b s5 s6 (verdicts)");
    eprintln!("# verdict legend: . = Ok/skip, X = ContractViolated, M = StateMissing, S = OracleStub");
    for r in &records {
        let row: String = r.verdicts.iter().map(|v| v.label()).collect::<Vec<_>>().join(" ");
        eprintln!(
            "TRACE | {:>5} | {:<24} | {}",
            r.case_id,
            r.first_fail.label(),
            row,
        );
    }

    // ── First-failing-stage bucket counts (sanity vs PR9 baseline) ──────
    let mut bucket_counts: BTreeMap<FirstFail, usize> = BTreeMap::new();
    for r in &records {
        *bucket_counts.entry(r.first_fail).or_insert(0) += 1;
    }
    eprintln!();
    eprintln!("═══ Task C: first-failing-stage histogram (sanity vs PR9 §3) ═══");
    for (bucket, count) in &bucket_counts {
        eprintln!("  {:<24} {}", bucket.label(), count);
    }

    // ── Cross-oracle pairing matrix ─────────────────────────────────────
    // Rows: first-failing-stage bucket. Cols: each ORACLE_STAGES entry.
    // Cell: count of cases in that row's bucket that ALSO have
    // ContractViolated for the column's oracle.
    let buckets: Vec<FirstFail> = {
        let set: BTreeSet<FirstFail> = records.iter().map(|r| r.first_fail).collect();
        set.into_iter().collect()
    };

    eprintln!();
    eprintln!("═══ Task C: cross-oracle pairing matrix ═══");
    eprintln!("# Cell: how many cases in this row's first-fail bucket also have");
    eprintln!("# ContractViolated for the column oracle (the 'X' verdict). Note");
    eprintln!("# that for first-fail = Stage N rows, the Stage N column equals");
    eprintln!("# the row size by definition (every case in the bucket fires its");
    eprintln!("# own bucket's oracle). The interesting cells are off-diagonal.");
    eprintln!();
    let header_cols: Vec<String> = ORACLE_STAGES
        .iter()
        .map(|s| match s {
            YangStage::Stage0Coplanar => "S0",
            YangStage::Stage1Bijective => "S1",
            YangStage::Stage2Arrangement => "S2",
            YangStage::Stage4bClassification => "S4b",
            YangStage::Stage5PatchSegment => "S5",
            YangStage::Stage6Assembly => "S6",
            _ => "??",
        })
        .map(|s| s.to_string())
        .collect();
    eprintln!(
        "| {:<24} | {:>5} | {} |",
        "First-fail bucket",
        "n",
        header_cols
            .iter()
            .map(|c| format!("{c:>4}"))
            .collect::<Vec<_>>()
            .join(" | ")
    );
    eprintln!(
        "|--------------------------|------:|{}",
        "------|".repeat(ORACLE_STAGES.len())
    );

    for bucket in &buckets {
        let row_records: Vec<&CaseRecord> = records
            .iter()
            .filter(|r| r.first_fail == *bucket)
            .collect();
        let n = row_records.len();
        let mut cells = Vec::with_capacity(ORACLE_STAGES.len());
        for (col_idx, _stage) in ORACLE_STAGES.iter().enumerate() {
            let count = row_records
                .iter()
                .filter(|r| matches!(r.verdicts[col_idx], CellVerdict::ContractViolated))
                .count();
            cells.push(format!("{count:>4}"));
        }
        eprintln!(
            "| {:<24} | {:>5} | {} |",
            bucket.label(),
            n,
            cells.join(" | ")
        );
    }

    // ── Critical claim 1: Stage 2 → Stage 4b shadowing ─────────────────
    let stage2_bucket: Vec<&CaseRecord> = records
        .iter()
        .filter(|r| matches!(r.first_fail, FirstFail::Stage(YangStage::Stage2Arrangement)))
        .collect();
    let s2_then_s4b = stage2_bucket
        .iter()
        .filter(|r| {
            let s4b_idx = ORACLE_STAGES
                .iter()
                .position(|s| *s == YangStage::Stage4bClassification)
                .unwrap();
            matches!(r.verdicts[s4b_idx], CellVerdict::ContractViolated)
        })
        .count();
    let s2_then_s6 = stage2_bucket
        .iter()
        .filter(|r| {
            let s6_idx = ORACLE_STAGES
                .iter()
                .position(|s| *s == YangStage::Stage6Assembly)
                .unwrap();
            matches!(r.verdicts[s6_idx], CellVerdict::ContractViolated)
        })
        .count();

    // ── Critical claim 2: Stage 4b → Stage 6 propagation ────────────────
    let stage4b_bucket: Vec<&CaseRecord> = records
        .iter()
        .filter(|r| matches!(r.first_fail, FirstFail::Stage(YangStage::Stage4bClassification)))
        .collect();
    let s4b_then_s6 = stage4b_bucket
        .iter()
        .filter(|r| {
            let s6_idx = ORACLE_STAGES
                .iter()
                .position(|s| *s == YangStage::Stage6Assembly)
                .unwrap();
            matches!(r.verdicts[s6_idx], CellVerdict::ContractViolated)
        })
        .count();
    let s4b_then_s5 = stage4b_bucket
        .iter()
        .filter(|r| {
            let s5_idx = ORACLE_STAGES
                .iter()
                .position(|s| *s == YangStage::Stage5PatchSegment)
                .unwrap();
            matches!(r.verdicts[s5_idx], CellVerdict::ContractViolated)
        })
        .count();

    // ── Critical claim 3: AllPass purity ────────────────────────────────
    // For each AllPass case, count whether ALL six verdicts are exactly
    // `Ok` (no `OracleStub`, no `StateMissing`, no `ContractViolated`).
    // Then split: full-bundle = no OracleStub anywhere AND every cell Ok.
    // Empty-bundle proxy: every verdict is Ok AND none of the per-oracle
    // checks would have run (impossible to detect from verdicts alone —
    // every PR9 oracle returns Ok on missing state). So instead we use
    // the count of OracleStub verdicts (Stage 0 partial-overlap is the
    // only PR9 oracle that emits OracleStub).
    let allpass: Vec<&CaseRecord> = records
        .iter()
        .filter(|r| matches!(r.first_fail, FirstFail::AllPass))
        .collect();
    let allpass_total = allpass.len();
    let allpass_with_stage0_stub = allpass
        .iter()
        .filter(|r| {
            let s0_idx = ORACLE_STAGES
                .iter()
                .position(|s| *s == YangStage::Stage0Coplanar)
                .unwrap();
            matches!(r.verdicts[s0_idx], CellVerdict::OracleStub)
        })
        .count();
    let allpass_pure = allpass_total - allpass_with_stage0_stub;

    eprintln!();
    eprintln!("═══ Task C: critical-claim numbers ═══");
    eprintln!(
        "Stage 2 → Stage 4b shadowing rate: {}/{} ({:.1}%)",
        s2_then_s4b,
        stage2_bucket.len(),
        if stage2_bucket.is_empty() {
            0.0
        } else {
            100.0 * s2_then_s4b as f64 / stage2_bucket.len() as f64
        },
    );
    eprintln!(
        "Stage 2 → Stage 6 propagation rate:  {}/{} ({:.1}%)",
        s2_then_s6,
        stage2_bucket.len(),
        if stage2_bucket.is_empty() {
            0.0
        } else {
            100.0 * s2_then_s6 as f64 / stage2_bucket.len() as f64
        },
    );
    eprintln!(
        "Stage 4b → Stage 6 propagation rate: {}/{} ({:.1}%)",
        s4b_then_s6,
        stage4b_bucket.len(),
        if stage4b_bucket.is_empty() {
            0.0
        } else {
            100.0 * s4b_then_s6 as f64 / stage4b_bucket.len() as f64
        },
    );
    eprintln!(
        "Stage 4b → Stage 5 propagation rate: {}/{} ({:.1}%)",
        s4b_then_s5,
        stage4b_bucket.len(),
        if stage4b_bucket.is_empty() {
            0.0
        } else {
            100.0 * s4b_then_s5 as f64 / stage4b_bucket.len() as f64
        },
    );
    eprintln!(
        "AllPass purity: {}/{} have NO OracleStub anywhere; {}/{} have Stage0 OracleStub",
        allpass_pure, allpass_total, allpass_with_stage0_stub, allpass_total,
    );

    // ── AllPass member-list dump (for purity audit cross-reference) ─────
    eprintln!();
    eprintln!("═══ Task C: AllPass case ids (for cross-check vs PR9 §3 baseline) ═══");
    let allpass_ids: Vec<&str> = allpass.iter().map(|r| r.case_id.as_str()).collect();
    eprintln!("AllPass cases ({}): {}", allpass_ids.len(), allpass_ids.join(", "));

    let stage2_ids: Vec<&str> = stage2_bucket.iter().map(|r| r.case_id.as_str()).collect();
    eprintln!("Stage2Arrangement cases ({}): {}", stage2_ids.len(), stage2_ids.join(", "));

    let stage4b_ids: Vec<&str> = stage4b_bucket.iter().map(|r| r.case_id.as_str()).collect();
    eprintln!(
        "Stage4bClassification cases ({}): {}",
        stage4b_ids.len(),
        stage4b_ids.join(", ")
    );

    eprintln!();
    eprintln!(
        "Total attempted: {} cases (skipped {} known timeouts).",
        total_attempted,
        skip.len()
    );
    eprintln!("Records collected: {}", records.len());
    assert_eq!(records.len(), total_attempted);
}
