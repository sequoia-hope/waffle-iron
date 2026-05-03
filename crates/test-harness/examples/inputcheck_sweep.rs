//! Crash-resilient corpus sweep for PR-S2 Phase 3 (adversary deliverable).
//!
//! Mirrors the logic in `tests/cherchi_inputcheck_corpus_sweep.rs`, but
//! appends one TSV row per case (per side, so 2 rows) IMMEDIATELY after each
//! case completes. This means a SIGKILL or harness timeout cannot lose
//! partial progress — the TSV always reflects the cases that did land.
//!
//! Why an example, not a test: the spec forbids the adversary from
//! modifying `cherchi_inputcheck_corpus_sweep.rs` (test-author-c's work)
//! and the original runner writes the TSV only at the end of the sweep.
//! The prior adversary went silent for 12+ hours mid-sweep with no TSV
//! produced. This example exists solely to be crash-resilient infrastructure
//! for the adversary to use — it does NOT replace the test, which remains
//! the load-bearing artifact for the codebase.
//!
//! Run:
//!   YANG_BOOLEAN=1 cargo run -p test-harness --example inputcheck_sweep \
//!     --release -- [start_idx] [end_idx]
//!
//! With no args, sweeps the whole corpus. With one arg, starts at that idx.
//! With two args, sweeps `start_idx..end_idx` (1-based, exclusive end).

use std::fmt::Write as _;
use std::fs::OpenOptions;
use std::io::Write;
use std::path::{Path, PathBuf};
use std::process::Command;
use std::time::Duration;

use test_harness::assay::randomized_runner::{discover_cases, run_single_case};
use test_harness::assay::scoring::AssayStatus;
use test_harness::cherchi_sidecar::{cherchi_bin, run_with_timeout, TimedRun};

const INPUTCHECK_TIMEOUT: Duration = Duration::from_secs(10);
const ASSAY_DIR: &str = "app/tests/cases/assay";
const TSV_PATH: &str = "docs/audits/cherchi_inputcheck_sweep_2026-05-03.tsv";
const RUNAWAY_DETAIL: &str = "runaway: subprocess killed at 10s";
/// Per-case Waffle timeout. `run_single_case` does NOT have a built-in
/// timeout (only `run_randomized_assay` does, at 90 s); run it on a
/// thread and recv-timeout so a hung Waffle path can't stall the sweep.
/// 60 s is conservative for normal cases (~3 s) and matches the pattern
/// of "kernel issues we should record, not babysit." Cases that exceed
/// this become MissingDump (`waffle_status=Errored`).
const WAFFLE_TIMEOUT: Duration = Duration::from_secs(60);

fn cherchi_inputcheck_bin() -> Option<PathBuf> {
    let base = cherchi_bin()?;
    let parent = base.parent()?;
    let candidate = parent.join("mesh_booleans_inputcheck");
    if !candidate.exists() {
        eprintln!(
            "[inputcheck-sweep] SKIP: mesh_booleans_inputcheck missing at {}",
            candidate.display()
        );
        return None;
    }
    Some(candidate)
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum Bucket {
    Valid,
    NonManifold,
    NonWatertight,
    SelfIntersecting,
    BadOrientation,
    CombinedFailures,
    Runaway,
}

impl Bucket {
    fn as_str(self) -> &'static str {
        match self {
            Bucket::Valid => "valid",
            Bucket::NonManifold => "non_manifold",
            Bucket::NonWatertight => "non_watertight",
            Bucket::SelfIntersecting => "self_intersecting",
            Bucket::BadOrientation => "bad_orientation",
            Bucket::CombinedFailures => "combined_failures",
            Bucket::Runaway => "runaway",
        }
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum WaffleStatus {
    Passed,
    Failed,
    Errored,
    MissingDump,
}

impl WaffleStatus {
    fn as_str(self) -> &'static str {
        match self {
            WaffleStatus::Passed => "Passed",
            WaffleStatus::Failed => "Failed",
            WaffleStatus::Errored => "Errored",
            WaffleStatus::MissingDump => "MissingDump",
        }
    }
}

fn waffle_status_from_assay(status: AssayStatus) -> WaffleStatus {
    match status {
        AssayStatus::Passed => WaffleStatus::Passed,
        AssayStatus::Failed => WaffleStatus::Failed,
        AssayStatus::Errored => WaffleStatus::Errored,
    }
}

type FailureMask = (bool, bool, bool, bool, bool);

fn parse_inputcheck_output(text: &str) -> Option<(FailureMask, String)> {
    let prefixes = [
        "Manifold check",
        "Watertight check",
        "Local  Orientation check",
        "Global Orientation check",
        "Intersection check",
    ];
    let mut bits: [Option<bool>; 5] = [None; 5];
    let mut matched_lines: [Option<String>; 5] = [None, None, None, None, None];
    for line in text.lines() {
        for (i, prefix) in prefixes.iter().enumerate() {
            if line.contains(prefix) {
                let ll = line.to_ascii_lowercase();
                if ll.contains("failed") {
                    bits[i] = Some(true);
                    matched_lines[i] = Some(line.trim().to_string());
                } else if ll.contains("passed") {
                    bits[i] = Some(false);
                    matched_lines[i] = Some(line.trim().to_string());
                }
                break;
            }
        }
    }
    let mask = (bits[0]?, bits[1]?, bits[2]?, bits[3]?, bits[4]?);
    let raw5 = matched_lines
        .iter()
        .map(|o| o.as_deref().unwrap_or(""))
        .collect::<Vec<_>>()
        .join(";");
    Some((mask, raw5))
}

fn classify(mask: FailureMask) -> Bucket {
    let (m, w, lo, go, i) = mask;
    let orientation_failed = lo || go;
    let n_categories = (m as u8) + (w as u8) + (orientation_failed as u8) + (i as u8);
    match n_categories {
        0 => Bucket::Valid,
        1 => {
            if m {
                Bucket::NonManifold
            } else if w {
                Bucket::NonWatertight
            } else if orientation_failed {
                Bucket::BadOrientation
            } else {
                Bucket::SelfIntersecting
            }
        }
        _ => Bucket::CombinedFailures,
    }
}

fn truncate_detail(s: &str) -> String {
    s.chars().take(200).collect()
}

fn check_one_obj(bin: &Path, obj: &Path, case_id: &str, side: &str) -> (String, String) {
    let mut cmd = Command::new(bin);
    cmd.arg(obj);
    match run_with_timeout(cmd, INPUTCHECK_TIMEOUT) {
        TimedRun::TimedOut => {
            eprintln!("[inputcheck-sweep] runaway on {} side {}", case_id, side);
            (
                Bucket::Runaway.as_str().to_string(),
                RUNAWAY_DETAIL.to_string(),
            )
        }
        TimedRun::SpawnFailed(e) => (
            Bucket::CombinedFailures.as_str().to_string(),
            truncate_detail(&format!("spawn-failed: {}", e)),
        ),
        TimedRun::Completed(out) => {
            let stdout = String::from_utf8_lossy(&out.stdout);
            let stderr = String::from_utf8_lossy(&out.stderr);
            let combined = format!("{}\n{}", stdout, stderr);
            match parse_inputcheck_output(&combined) {
                Some((mask, raw5)) => (classify(mask).as_str().to_string(), truncate_detail(&raw5)),
                None => (
                    Bucket::CombinedFailures.as_str().to_string(),
                    truncate_detail(&format!("parse-error: {}", combined.replace('\n', " "))),
                ),
            }
        }
    }
}

#[derive(Debug, Clone)]
struct SweepRow {
    case_id: String,
    side: &'static str,
    waffle_status: WaffleStatus,
    cherchi_class: String,
    cherchi_detail: String,
}

fn append_row(path: &str, row: &SweepRow) -> std::io::Result<()> {
    if let Some(parent) = PathBuf::from(path).parent() {
        let _ = std::fs::create_dir_all(parent);
    }
    let mut f = OpenOptions::new().append(true).create(true).open(path)?;
    let mut line = String::new();
    writeln!(
        line,
        "{}\t{}\t{}\t{}\t{}",
        row.case_id,
        row.side,
        row.waffle_status.as_str(),
        row.cherchi_class,
        row.cherchi_detail.replace(['\t', '\n'], " "),
    )
    .unwrap();
    f.write_all(line.as_bytes())?;
    f.flush()
}

fn write_header(path: &str) -> std::io::Result<()> {
    if let Some(parent) = PathBuf::from(path).parent() {
        let _ = std::fs::create_dir_all(parent);
    }
    std::fs::write(
        path,
        "case_id\tside\twaffle_status\tcherchi_class\tcherchi_detail\n",
    )
}

fn run_one_case(dir: &Path, bin: &Path, case_id: &str, sweep_root: &Path) -> [SweepRow; 2] {
    let workdir = sweep_root.join(case_id);
    let _ = std::fs::create_dir_all(&workdir);
    let base = workdir.join(case_id);
    let base_str = base.to_string_lossy().into_owned();
    let path_a = workdir.join(format!("{}_a.obj", case_id));
    let path_b = workdir.join(format!("{}_b.obj", case_id));

    let _ = std::fs::remove_file(&path_a);
    let _ = std::fs::remove_file(&path_b);

    std::env::set_var("YANG_BOOLEAN", "1");
    std::env::set_var("YANG_DUMP_OBJ_BASE", &base_str);
    // Run Waffle on a thread with recv-timeout so a hung kernel path
    // doesn't stall the sweep (e.g. R0071 gear+revolve at micro-scale
    // ran >2 min in the first attempt before being killed).
    // NOTE: `run_single_case` is not panic-safe across threads —
    // a kernel panic on the worker would terminate the whole process —
    // but the alternative (no timeout) loses ALL data on the same panic
    // OR on a hang, so this strictly improves crash-resilience.
    let case_result = {
        let dir_owned = dir.to_path_buf();
        let id_owned = case_id.to_string();
        let (tx, rx) = std::sync::mpsc::channel();
        let handle = std::thread::spawn(move || {
            let r = run_single_case(&dir_owned, &id_owned, true);
            let _ = tx.send(r);
        });
        match rx.recv_timeout(WAFFLE_TIMEOUT) {
            Ok(r) => {
                let _ = handle.join();
                r
            }
            Err(_) => {
                eprintln!(
                    "[inputcheck-sweep] WAFFLE TIMEOUT on {} after {}s — \
                     leaking thread, marking Errored",
                    case_id,
                    WAFFLE_TIMEOUT.as_secs()
                );
                None
            }
        }
    };
    std::env::remove_var("YANG_DUMP_OBJ_BASE");

    let waffle_status_base = match case_result {
        Some(r) => waffle_status_from_assay(r.status),
        None => WaffleStatus::Errored,
    };

    let rows: [SweepRow; 2] = [
        side_row(case_id, "A", waffle_status_base, &path_a, bin),
        side_row(case_id, "B", waffle_status_base, &path_b, bin),
    ];

    let _ = std::fs::remove_file(&path_a);
    let _ = std::fs::remove_file(&path_b);
    let _ = std::fs::remove_dir(&workdir);

    rows
}

fn side_row(
    case_id: &str,
    side: &'static str,
    waffle_status_base: WaffleStatus,
    obj: &Path,
    bin: &Path,
) -> SweepRow {
    if !obj.exists() {
        return SweepRow {
            case_id: case_id.to_string(),
            side,
            waffle_status: WaffleStatus::MissingDump,
            cherchi_class: String::new(),
            cherchi_detail: String::new(),
        };
    }
    let (cls, detail) = check_one_obj(bin, obj, case_id, side);
    SweepRow {
        case_id: case_id.to_string(),
        side,
        waffle_status: waffle_status_base,
        cherchi_class: cls,
        cherchi_detail: detail,
    }
}

fn main() {
    let args: Vec<String> = std::env::args().collect();
    let start_idx: usize = args.get(1).and_then(|s| s.parse().ok()).unwrap_or(1);
    let end_idx: Option<usize> = args.get(2).and_then(|s| s.parse().ok());

    let bin = match cherchi_inputcheck_bin() {
        Some(p) => p,
        None => {
            eprintln!("[inputcheck-sweep] FATAL: missing binary, exiting");
            std::process::exit(1);
        }
    };
    let dir = Path::new(ASSAY_DIR);
    if !dir.exists() {
        eprintln!(
            "[inputcheck-sweep] SKIP: corpus dir missing: {}",
            dir.display()
        );
        std::process::exit(1);
    }
    let cases = discover_cases(dir);
    if cases.is_empty() {
        eprintln!("[inputcheck-sweep] SKIP: no cases discovered");
        std::process::exit(1);
    }
    let total_cases = cases.len();
    let end = end_idx.unwrap_or(total_cases + 1).min(total_cases + 1);

    let sweep_root = std::env::temp_dir().join(format!(
        "waffle_inputcheck_sweep_example_{}",
        std::process::id()
    ));
    let _ = std::fs::create_dir_all(&sweep_root);

    // Header only on a fresh run (start_idx == 1).
    if start_idx == 1 {
        if let Err(e) = write_header(TSV_PATH) {
            eprintln!(
                "[inputcheck-sweep] FATAL: cannot write header to {}: {}",
                TSV_PATH, e
            );
            std::process::exit(1);
        }
        eprintln!("[inputcheck-sweep] wrote fresh TSV header to {}", TSV_PATH);
    } else {
        eprintln!(
            "[inputcheck-sweep] resuming at idx {} (TSV append mode)",
            start_idx
        );
    }

    eprintln!(
        "[inputcheck-sweep] sweeping cases {}..{} of {} (sweep_root={})",
        start_idx,
        end,
        total_cases,
        sweep_root.display()
    );
    let start = std::time::Instant::now();

    for (idx0, case) in cases.iter().enumerate() {
        let idx = idx0 + 1;
        if idx < start_idx || idx >= end {
            continue;
        }
        let case_start = std::time::Instant::now();
        eprint!("  [{}/{}] {} ... ", idx, total_cases, case.id);
        let two = run_one_case(dir, &bin, &case.id, &sweep_root);
        eprintln!(
            "waffle={} A={} B={} ({:.1}s elapsed_total={:.0}s)",
            two[0].waffle_status.as_str(),
            two[0].cherchi_class,
            two[1].cherchi_class,
            case_start.elapsed().as_secs_f64(),
            start.elapsed().as_secs_f64(),
        );
        // Append immediately so a crash mid-sweep doesn't lose data.
        for r in &two {
            if let Err(e) = append_row(TSV_PATH, r) {
                eprintln!("[inputcheck-sweep] FATAL: cannot append row: {}", e);
                std::process::exit(1);
            }
        }
    }

    let elapsed = start.elapsed();
    eprintln!(
        "[inputcheck-sweep] complete in {:.1}s — TSV at {}",
        elapsed.as_secs_f64(),
        TSV_PATH
    );
}
