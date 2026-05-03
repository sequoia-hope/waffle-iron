//! Cherchi 2022 `mesh_booleans_inputcheck` corpus-sweep diagnostic test.
//!
//! Runs Cherchi's input-precondition validator against the preprocessed A
//! and B meshes that Waffle hands to `subdivide_mesh_pair_full_cherchi`, for
//! every assay case in `app/tests/cases/assay/`. Classifies each into one of
//! seven mutually exclusive buckets per the spec's classification table,
//! writes a 380-row TSV (one row per case × side), and prints a one-line
//! summary plus a four-line cross-tab.
//!
//! Spec (the contract this test implements):
//!   `specs/pr_s2_corpus_inputcheck_sweep.md`
//!
//! Plan:
//!   `/home/claude/.claude/plans/reactive-juggling-sloth.md` — PR-S2.
//!
//! Sibling helpers: shares `cherchi_bin`, `TimedRun`, `run_with_timeout`
//!   with `tests/cherchi2022_reference_parity.rs` (PR-S1) via the
//!   `test_harness::cherchi_sidecar` module — extracted in PR-S2 so both
//!   tests use one impl, with the timeout parameterized (30 s for
//!   reference parity, 10 s here per spec §4 "Timeout policy").
//!
//! Adversary deliverable (PR-S2 Phase 3, NOT this file):
//!   `docs/audits/pr_s2_inputcheck_corpus_findings.md` will analyze the
//!   TSV produced by this test.

use std::fmt::Write as _;
use std::path::{Path, PathBuf};
use std::process::Command;
use std::time::Duration;

use test_harness::assay::randomized_runner::{discover_cases, run_single_case};
use test_harness::assay::scoring::AssayStatus;
use test_harness::cherchi_sidecar::{cherchi_bin, run_with_timeout, TimedRun};

/// Spec §4 "Timeout policy" — 10 s per inputcheck invocation.
/// `mesh_booleans_inputcheck` is fast on well-formed input (<1 s); cases
/// that exceed this cap are by definition pathological and that signal IS
/// useful for PR-S3 anchor selection.
const INPUTCHECK_TIMEOUT: Duration = Duration::from_secs(10);

const ASSAY_DIR: &str = "../../app/tests/cases/assay";

/// Spec §3 "TSV schema" — committed audit anchor for the PR-S2 findings
/// memo. Dated filename so historical sweeps remain side-by-side at
/// distinct paths.
const TSV_PATH: &str = "../../docs/audits/cherchi_inputcheck_sweep_2026-05-03.tsv";

/// Spec §3 — literal label for the runaway `cherchi_detail` column.
const RUNAWAY_DETAIL: &str = "runaway: subprocess killed at 10s";

/// Per-case Waffle timeout. `run_single_case` does NOT have a built-in
/// timeout (only `run_randomized_assay` does, at 90 s); we run it on a
/// thread and `recv_timeout` so a hung Waffle path can't stall the sweep.
/// 60 s is conservative for normal cases (~3 s) and matches the pattern
/// of "kernel issues we should record, not babysit." Cases that exceed
/// this become MissingDump (`waffle_status=Errored`).
///
/// Backported from `examples/inputcheck_sweep.rs` (PR-S3) — the original
/// adversary's first sweep ran 12+ hours silently because R0071
/// (gear+revolve at scale 1.86e-4) hangs Waffle indefinitely. The example
/// served as a temporary workaround during PR-S2 Phase 3; PR-S3 promotes
/// the pattern into the canonical test and deletes the example.
const WAFFLE_TIMEOUT: Duration = Duration::from_secs(60);

/// Per-case Waffle timeout literal recorded in `cherchi_detail` for
/// MissingDump rows that hit the WAFFLE_TIMEOUT. Per the PR-S2 spec
/// amendment landing in PR-S3.
fn waffle_timeout_detail() -> String {
    format!("waffle-timeout: {}s", WAFFLE_TIMEOUT.as_secs())
}

/// Resolve the `mesh_booleans_inputcheck` binary path. The shared
/// `cherchi_bin()` returns `mesh_booleans` (the union/intersect/subtract
/// driver); the inputcheck validator is its sibling in the same build dir.
/// Per the build guide (`docs/sidecar/cherchi2022_build_guide.md`) the two
/// are always built together by the upstream Makefile.
fn cherchi_inputcheck_bin() -> Option<PathBuf> {
    let base = cherchi_bin()?;
    let parent = base.parent()?;
    let candidate = parent.join("mesh_booleans_inputcheck");
    if !candidate.exists() {
        eprintln!(
            "[inputcheck-sweep] SKIP: `mesh_booleans_inputcheck` not found next \
             to `mesh_booleans` at `{}`. Build it per the upstream README \
             (it's built alongside `mesh_booleans`).",
            candidate.display()
        );
        return None;
    }
    Some(candidate)
}

/// Spec §2 "Classification scheme" — 7 mutually exclusive buckets.
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

/// Spec §3 "TSV schema" — `waffle_status` enum. PascalCase mirrors
/// `AssayStatus` in `assay/scoring.rs`; `MissingDump` is the spec's
/// dedicated value for "Waffle didn't write the OBJ" (not derivable from
/// the underlying `AssayStatus`).
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

/// One TSV row per (case_id × side) per spec §3 — 380 rows total for the
/// 190-case corpus.
#[derive(Debug, Clone)]
struct SweepRow {
    case_id: String,
    /// `"A"` or `"B"`.
    side: &'static str,
    waffle_status: WaffleStatus,
    /// One of `Bucket::as_str()`, OR empty when `waffle_status ==
    /// MissingDump` per spec §3.
    cherchi_class: String,
    /// Spec §3: raw 5-line inputcheck stderr joined with `;`, truncated to
    /// 200 chars. Empty for `MissingDump` or `runaway` (runaway gets the
    /// literal `RUNAWAY_DETAIL` instead, per row 5 of the schema table).
    cherchi_detail: String,
}

/// Map an `AssayStatus` to the spec §3 `waffle_status` vocabulary.
/// `MissingDump` is NOT derived from status — it's set separately when the
/// post-Waffle OBJ for a side is absent.
fn waffle_status_from_assay(status: AssayStatus) -> WaffleStatus {
    match status {
        AssayStatus::Passed => WaffleStatus::Passed,
        AssayStatus::Failed => WaffleStatus::Failed,
        AssayStatus::Errored => WaffleStatus::Errored,
    }
}

/// 5-bit failure mask `(M, W, LO, GO, I)` per spec §2 — each bit is `1`
/// if the corresponding inputcheck line said `failed`/`FAILED`.
type FailureMask = (bool, bool, bool, bool, bool);

/// Parse `mesh_booleans_inputcheck` output per spec §2 — exactly five lines
/// of `<Check>:<padding>{passed|failed|FAILED}`. Returns `(mask, raw5)`
/// where `mask` is the 5-bit failure mask and `raw5` is the matched 5
/// lines re-joined with `;` for the `cherchi_detail` column. Returns
/// `None` if any of the 5 expected lines is missing.
///
/// Spec §2 mandates **case-insensitive** matching of the result word:
/// the build guide shows lower-case `failed`, the F0002 capture shows
/// upper-case `FAILED` — the parser must accept both.
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

/// Map the 5-bit failure mask to a bucket per spec §2 "Classification".
/// Local + Global Orientation collapse into one orientation category for
/// bucket selection (spec table row `bad_orientation`). Total over the
/// 32-element mask domain; panics if the spec ever drifts (spec §2 line 53).
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
            } else if i {
                Bucket::SelfIntersecting
            } else {
                // Unreachable: n_categories == 1 implies one bit set.
                panic!(
                    "classify: 5-bit mask {:?} has n_categories=1 but no bit set",
                    mask
                );
            }
        }
        _ => Bucket::CombinedFailures,
    }
}

/// Truncate `cherchi_detail` to spec §3's 200-char cap. `chars().take()` is
/// safe for the inputcheck output (ASCII), but using `chars()` keeps it
/// UTF-8 safe in case Cinolib ever embeds a non-ASCII path.
fn truncate_detail(s: &str) -> String {
    s.chars().take(200).collect()
}

/// Run inputcheck against one OBJ; return `(class, detail)`.
fn check_one_obj(bin: &Path, obj: &Path, case_id: &str, side: &str) -> (String, String) {
    let mut cmd = Command::new(bin);
    cmd.arg(obj);
    match run_with_timeout(cmd, INPUTCHECK_TIMEOUT) {
        TimedRun::TimedOut => {
            // Spec §4 logging: stderr line per runaway so the operator can
            // see which case stalled without scraping the TSV.
            eprintln!("[inputcheck-sweep] runaway on {} side {}", case_id, side);
            (
                Bucket::Runaway.as_str().to_string(),
                RUNAWAY_DETAIL.to_string(),
            )
        }
        TimedRun::SpawnFailed(e) => {
            // Treat spawn failure as a `combined_failures` row with a
            // diagnostic in the detail column. The spec's classifier is
            // total over inputcheck output space; spawn failure is
            // operational, not a Cherchi opinion.
            eprintln!(
                "[inputcheck-sweep] spawn-failed on {} side {}: {}",
                case_id, side, e
            );
            (
                Bucket::CombinedFailures.as_str().to_string(),
                truncate_detail(&format!("spawn-failed: {}", e)),
            )
        }
        TimedRun::Completed(out) => {
            // Empirically (verified manually 2026-05-03 against
            // `mesh_booleans_inputcheck f0002_a.obj`):
            // the 5 check lines land on STDOUT, not stderr as the spec's
            // §2 narrative inherits from `cherchi2022_sidecar_feasibility.md`.
            // Concatenate so we're robust to either choice (Cinolib error
            // lines like `ERROR: read_OBJ() : couldn't open` DO go to
            // stderr — this lets us include them in cherchi_detail too).
            let stdout = String::from_utf8_lossy(&out.stdout);
            let stderr = String::from_utf8_lossy(&out.stderr);
            let combined = format!("{}\n{}", stdout, stderr);
            match parse_inputcheck_output(&combined) {
                Some((mask, raw5)) => (classify(mask).as_str().to_string(), truncate_detail(&raw5)),
                // Parse error: didn't see all 5 expected lines. Bucket as
                // combined_failures (spec §2 "catch-all") with the raw
                // output prefixed for diagnostic.
                None => (
                    Bucket::CombinedFailures.as_str().to_string(),
                    truncate_detail(&format!("parse-error: {}", combined.replace('\n', " "))),
                ),
            }
        }
    }
}

/// Per-case workflow: dump OBJs via `YANG_DUMP_OBJ_BASE`, run inputcheck on
/// each side, classify, return the two TSV rows (A and B). Cleans up the
/// per-case temp dir before returning. The base sweep temp dir
/// (`waffle_inputcheck_sweep_<pid>`) lives one level up — kept for the
/// life of the test process per spec §7's "leftover files are useful
/// diagnostic artifacts on a failed run" guidance.
fn run_one_case(dir: &Path, bin: &Path, case_id: &str, sweep_root: &Path) -> [SweepRow; 2] {
    // Spec §7 "Per-OBJ temp path" — sweep_root is the PID-stamped base;
    // each case gets its own subdir.
    let workdir = sweep_root.join(case_id);
    let _ = std::fs::create_dir_all(&workdir);
    let base = workdir.join(case_id);
    let base_str = base.to_string_lossy().into_owned();
    let path_a = workdir.join(format!("{}_a.obj", case_id));
    let path_b = workdir.join(format!("{}_b.obj", case_id));

    // Clear stale files so a partial run can't be mistaken for fresh data.
    let _ = std::fs::remove_file(&path_a);
    let _ = std::fs::remove_file(&path_b);

    std::env::set_var("YANG_BOOLEAN", "1");
    std::env::set_var("YANG_DUMP_OBJ_BASE", &base_str);

    // Run Waffle on a thread with recv-timeout so a hung kernel path
    // doesn't stall the sweep. R0071 (gear+revolve at scale 1.86e-4)
    // hangs `run_single_case` indefinitely; without the timeout, the
    // whole sweep stalls. NOTE: `run_single_case` is not panic-safe
    // across threads — a kernel panic on the worker would terminate
    // the whole process — but the alternative (no timeout) loses ALL
    // data on the same panic OR on a hang, so this strictly improves
    // crash-resilience.
    let waffle_timeout_hit;
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
                waffle_timeout_hit = false;
                r
            }
            Err(_) => {
                eprintln!(
                    "[inputcheck-sweep] WAFFLE TIMEOUT on {} after {}s — \
                     leaking thread, marking Errored",
                    case_id,
                    WAFFLE_TIMEOUT.as_secs()
                );
                waffle_timeout_hit = true;
                None
            }
        }
    };
    std::env::remove_var("YANG_DUMP_OBJ_BASE");

    let waffle_status_base = match case_result {
        Some(r) => waffle_status_from_assay(r.status),
        // Case not found in corpus OR WAFFLE_TIMEOUT hit — both are
        // operationally Errored from this sweep's perspective.
        None => WaffleStatus::Errored,
    };

    let mut rows: [SweepRow; 2] = [
        side_row(case_id, "A", waffle_status_base, &path_a, bin),
        side_row(case_id, "B", waffle_status_base, &path_b, bin),
    ];

    // Per the PR-S2 spec amendment landing in PR-S3: MissingDump rows
    // that result from a WAFFLE_TIMEOUT (rather than a kernel error
    // before the dump site) carry literal `waffle-timeout: <SEC>s` in
    // their `cherchi_detail` so post-sweep readers can distinguish hung
    // cases from short-circuited cases.
    if waffle_timeout_hit {
        let detail = waffle_timeout_detail();
        for row in rows.iter_mut() {
            // Only overwrite when the row already classified as MissingDump
            // (i.e., the OBJ never landed). If the dump landed before the
            // timeout fired, the inputcheck classification is the source of
            // truth and we leave it alone.
            if row.waffle_status == WaffleStatus::MissingDump {
                row.cherchi_detail = truncate_detail(&detail);
            }
        }
    }

    // Best-effort cleanup of OBJs (keep the dir; harmless empty subdir).
    let _ = std::fs::remove_file(&path_a);
    let _ = std::fs::remove_file(&path_b);
    let _ = std::fs::remove_dir(&workdir);

    rows
}

/// Build the SweepRow for one side. Splits the MissingDump branch out so
/// the per-case driver stays readable.
fn side_row(
    case_id: &str,
    side: &'static str,
    waffle_status_base: WaffleStatus,
    obj: &Path,
    bin: &Path,
) -> SweepRow {
    if !obj.exists() {
        // Spec §3: when `waffle_status == MissingDump`, `cherchi_class` is
        // empty and `cherchi_detail` is empty.
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

/// TSV writer per spec §3 — header line + one row per (case_id × side).
fn write_tsv(rows: &[SweepRow], path: &str) -> std::io::Result<()> {
    let mut out = String::new();
    writeln!(
        out,
        "case_id\tside\twaffle_status\tcherchi_class\tcherchi_detail"
    )
    .unwrap();
    for r in rows {
        writeln!(
            out,
            "{}\t{}\t{}\t{}\t{}",
            r.case_id,
            r.side,
            r.waffle_status.as_str(),
            r.cherchi_class,
            // Defensive: if a detail string ever contains a tab or
            // newline, replace so the TSV stays parseable. The current
            // truncate_detail input doesn't include tabs (Cherchi uses
            // spaces), but parse-error fallback echoes raw output that
            // could.
            r.cherchi_detail.replace(['\t', '\n'], " "),
        )
        .unwrap();
    }
    if let Some(parent) = PathBuf::from(path).parent() {
        let _ = std::fs::create_dir_all(parent);
    }
    std::fs::write(path, out)
}

/// Tally counts and print the spec §5 "Stdout summary" block.
fn print_summary(rows: &[SweepRow]) {
    let mut valid = 0;
    let mut non_manifold = 0;
    let mut non_watertight = 0;
    let mut self_intersecting = 0;
    let mut bad_orientation = 0;
    let mut combined_failures = 0;
    let mut runaway = 0;
    let mut missing_dump = 0;

    for r in rows {
        if r.waffle_status == WaffleStatus::MissingDump {
            missing_dump += 1;
            continue;
        }
        match r.cherchi_class.as_str() {
            "valid" => valid += 1,
            "non_manifold" => non_manifold += 1,
            "non_watertight" => non_watertight += 1,
            "self_intersecting" => self_intersecting += 1,
            "bad_orientation" => bad_orientation += 1,
            "combined_failures" => combined_failures += 1,
            "runaway" => runaway += 1,
            other => eprintln!("[inputcheck-sweep] WARN: unknown class label `{}`", other),
        }
    }

    let total = rows.len();
    let sum_check = valid
        + non_manifold
        + non_watertight
        + self_intersecting
        + bad_orientation
        + combined_failures
        + runaway
        + missing_dump;
    if sum_check != total {
        eprintln!(
            "[inputcheck-sweep] WARN: bucket sum {} != total rows {}",
            sum_check, total
        );
    }

    // Spec §5 — main summary line. Spec shows the line wrapped across
    // three lines for readability; collapse to one line at print time
    // (the `total=` summary is consumed by humans grepping output).
    println!(
        "[inputcheck-sweep] total={} valid={} non_manifold={} non_watertight={} \
         self_intersecting={} bad_orientation={} combined_failures={} runaway={} \
         missing_dump={}",
        total,
        valid,
        non_manifold,
        non_watertight,
        self_intersecting,
        bad_orientation,
        combined_failures,
        runaway,
        missing_dump,
    );

    // Spec §5 cross-tab — exactly the four cells listed; the
    // `← interesting if >0` annotation is a literal part of the line.
    let mut wp_v = 0;
    let mut wp_cf = 0;
    let mut wf_v = 0;
    let mut wf_cf = 0;
    for r in rows {
        let cls = r.cherchi_class.as_str();
        match (r.waffle_status, cls) {
            (WaffleStatus::Passed, "valid") => wp_v += 1,
            (WaffleStatus::Passed, "combined_failures") => wp_cf += 1,
            (WaffleStatus::Failed, "valid") => wf_v += 1,
            (WaffleStatus::Failed, "combined_failures") => wf_cf += 1,
            _ => {}
        }
    }
    println!("[inputcheck-sweep] cross-tab:");
    println!(
        "[inputcheck-sweep]   waffle=Passed × cherchi=valid: {}",
        wp_v
    );
    println!(
        "[inputcheck-sweep]   waffle=Passed × cherchi=combined_failures: {}",
        wp_cf
    );
    println!(
        "[inputcheck-sweep]   waffle=Failed × cherchi=valid: {}  ← interesting if >0",
        wf_v
    );
    println!(
        "[inputcheck-sweep]   waffle=Failed × cherchi=combined_failures: {}",
        wf_cf
    );
}

/// Full corpus sweep — `#[ignore]` per spec §7's "production zero-impact"
/// model (the test sets `YANG_DUMP_OBJ_BASE` and so MUST be opt-in to keep
/// production / default test runs unaffected).
///
/// Run with:
///   `cargo test -p test-harness --test cherchi_inputcheck_corpus_sweep \
///     -- cherchi_inputcheck_corpus_sweep --ignored --nocapture --test-threads=1`
///
/// `--test-threads=1` is mandatory: this test mutates process-global env
/// (`YANG_BOOLEAN`, `YANG_DUMP_OBJ_BASE`); concurrent execution would race.
#[test]
#[ignore]
fn cherchi_inputcheck_corpus_sweep() {
    let bin = match cherchi_inputcheck_bin() {
        Some(p) => p,
        None => return,
    };
    let dir = Path::new(ASSAY_DIR);
    if !dir.exists() {
        eprintln!(
            "[inputcheck-sweep] SKIP: assay corpus dir `{}` not present",
            dir.display()
        );
        return;
    }

    let cases = discover_cases(dir);
    if cases.is_empty() {
        eprintln!("[inputcheck-sweep] SKIP: no cases discovered (manifest missing/empty)");
        return;
    }

    // Spec §7 "Per-OBJ temp path" — single PID-stamped base for the whole
    // sweep; per-case subdirs hang off it.
    let sweep_root =
        std::env::temp_dir().join(format!("waffle_inputcheck_sweep_{}", std::process::id()));
    let _ = std::fs::create_dir_all(&sweep_root);

    eprintln!(
        "[inputcheck-sweep] starting sweep over {} cases (sweep_root={})",
        cases.len(),
        sweep_root.display()
    );
    let start = std::time::Instant::now();
    let mut rows: Vec<SweepRow> = Vec::with_capacity(cases.len() * 2);
    for (idx, case) in cases.iter().enumerate() {
        eprint!("  [{}/{}] {} ... ", idx + 1, cases.len(), case.id);
        let two = run_one_case(dir, &bin, &case.id, &sweep_root);
        eprintln!(
            "waffle={} A={} B={}",
            two[0].waffle_status.as_str(),
            two[0].cherchi_class,
            two[1].cherchi_class
        );
        rows.push(two[0].clone());
        rows.push(two[1].clone());
    }
    let elapsed = start.elapsed();
    eprintln!(
        "[inputcheck-sweep] sweep complete in {:.1}s",
        elapsed.as_secs_f64()
    );

    match write_tsv(&rows, TSV_PATH) {
        Ok(()) => eprintln!("[inputcheck-sweep] wrote TSV: {}", TSV_PATH),
        Err(e) => eprintln!(
            "[inputcheck-sweep] FAILED to write TSV `{}`: {}",
            TSV_PATH, e
        ),
    }

    print_summary(&rows);
}

/// One-case smoke test using F0002. Validates: dump path works, inputcheck
/// runs, bucket classification reachable. Adversary uses this to sanity-
/// check the harness before running the full sweep. Per the
/// `cherchi2022_sidecar_feasibility.md` §"Build verified 2026-05-03"
/// reference, F0002 is expected to land in `combined_failures` (M+W+I all
/// failed) on both A and B sides.
///
/// Run with:
///   `cargo test -p test-harness --test cherchi_inputcheck_corpus_sweep \
///     -- cherchi_inputcheck_smoke_one_case --ignored --nocapture --test-threads=1`
///
/// Does NOT write the TSV — smoke only.
#[test]
#[ignore]
fn cherchi_inputcheck_smoke_one_case() {
    let bin = match cherchi_inputcheck_bin() {
        Some(p) => p,
        None => return,
    };
    let dir = Path::new(ASSAY_DIR);
    if !dir.exists() {
        eprintln!(
            "[inputcheck-smoke] SKIP: assay corpus dir `{}` not present",
            dir.display()
        );
        return;
    }

    let sweep_root = std::env::temp_dir().join(format!(
        "waffle_inputcheck_sweep_{}_smoke",
        std::process::id()
    ));
    let _ = std::fs::create_dir_all(&sweep_root);
    let two = run_one_case(dir, &bin, "F0002", &sweep_root);
    for row in &two {
        eprintln!(
            "[inputcheck-smoke] F0002 side={} waffle={} cherchi_class={} cherchi_detail={}",
            row.side,
            row.waffle_status.as_str(),
            row.cherchi_class,
            row.cherchi_detail,
        );
    }

    // Sanity: at minimum, both sides must have classified into something.
    // The actual bucket is not asserted (the harness must be willing to
    // accept whatever Cherchi reports — it's the oracle); but if a side
    // came back as `MissingDump` it means the dump path failed, which IS
    // a harness defect worth surfacing loudly.
    for row in &two {
        assert_ne!(
            row.waffle_status,
            WaffleStatus::MissingDump,
            "F0002 side {} OBJ was not dumped — YANG_DUMP_OBJ_BASE path broken or F0002 short-circuited",
            row.side
        );
    }
}

// ── Pure-logic unit tests for the classifier (no Cherchi binary needed) ──

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn classify_all_passed_is_valid() {
        assert_eq!(classify((false, false, false, false, false)), Bucket::Valid);
    }

    #[test]
    fn classify_only_manifold_failed() {
        assert_eq!(
            classify((true, false, false, false, false)),
            Bucket::NonManifold
        );
    }

    #[test]
    fn classify_only_watertight_failed() {
        assert_eq!(
            classify((false, true, false, false, false)),
            Bucket::NonWatertight
        );
    }

    #[test]
    fn classify_only_intersection_failed() {
        assert_eq!(
            classify((false, false, false, false, true)),
            Bucket::SelfIntersecting
        );
    }

    #[test]
    fn classify_only_local_orientation_failed() {
        assert_eq!(
            classify((false, false, true, false, false)),
            Bucket::BadOrientation
        );
    }

    #[test]
    fn classify_only_global_orientation_failed() {
        assert_eq!(
            classify((false, false, false, true, false)),
            Bucket::BadOrientation
        );
    }

    #[test]
    fn classify_both_orientations_failed_one_category() {
        // Spec §2 selector for `bad_orientation`: LO and GO collapse into
        // one category; both bits set is still single-category failure.
        assert_eq!(
            classify((false, false, true, true, false)),
            Bucket::BadOrientation
        );
    }

    #[test]
    fn classify_manifold_plus_orientation_is_combined() {
        // Two distinct categories failed (M + orientation).
        assert_eq!(
            classify((true, false, true, false, false)),
            Bucket::CombinedFailures
        );
    }

    #[test]
    fn classify_f0002_pattern_is_combined() {
        // From `cherchi2022_sidecar_feasibility.md` §"Build verified
        // 2026-05-03": F0002's A and B both report M+W+I failed.
        assert_eq!(
            classify((true, true, false, false, true)),
            Bucket::CombinedFailures
        );
    }

    #[test]
    fn parse_inputcheck_all_passed() {
        let stdout = "Manifold check:                    passed\n\
                      Watertight check:                  passed\n\
                      Local  Orientation check:          passed\n\
                      Global Orientation check:          passed\n\
                      Intersection check:                passed\n";
        let (mask, _raw5) = parse_inputcheck_output(stdout).expect("all-passed input must parse");
        assert_eq!(mask, (false, false, false, false, false));
    }

    #[test]
    fn parse_inputcheck_f0002_uppercase_failed() {
        // Per spec §2: case-insensitive matching is mandatory. F0002's
        // capture in the feasibility memo uses uppercase FAILED.
        let stdout = "Manifold check:                    FAILED\n\
                      Watertight check:                  FAILED\n\
                      Local  Orientation check:          passed\n\
                      Global Orientation check:          passed\n\
                      Intersection check:                FAILED\n";
        let (mask, raw5) = parse_inputcheck_output(stdout).expect("uppercase must parse");
        assert_eq!(mask, (true, true, false, false, true));
        // raw5 carries all 5 lines joined with `;` for the cherchi_detail
        // column; verify the load-bearing structure.
        assert!(raw5.contains("Manifold check"));
        assert!(raw5.contains("Intersection check"));
        assert_eq!(raw5.matches(';').count(), 4); // 5 segments → 4 separators
    }

    #[test]
    fn parse_inputcheck_lowercase_failed() {
        // Per spec §2: lowercase `failed` (the build-guide form) must also
        // parse identically.
        let stdout = "Manifold check: failed\n\
                      Watertight check: failed\n\
                      Local  Orientation check: passed\n\
                      Global Orientation check: passed\n\
                      Intersection check: failed\n";
        let (mask, _) = parse_inputcheck_output(stdout).expect("lowercase must parse");
        assert_eq!(mask, (true, true, false, false, true));
    }

    #[test]
    fn parse_inputcheck_missing_lines_returns_none() {
        // Only 3 of 5 lines present → unparseable → None → caller maps to
        // combined_failures with parse-error detail.
        let stdout = "Manifold check:                    passed\n\
                      Watertight check:                  passed\n\
                      Intersection check:                passed\n";
        assert!(parse_inputcheck_output(stdout).is_none());
    }

    #[test]
    fn waffle_status_from_assay_passed() {
        assert_eq!(
            waffle_status_from_assay(AssayStatus::Passed),
            WaffleStatus::Passed
        );
        assert_eq!(
            waffle_status_from_assay(AssayStatus::Failed),
            WaffleStatus::Failed
        );
        assert_eq!(
            waffle_status_from_assay(AssayStatus::Errored),
            WaffleStatus::Errored
        );
    }

    #[test]
    fn truncate_detail_caps_at_200_chars() {
        let long: String = "x".repeat(300);
        assert_eq!(truncate_detail(&long).len(), 200);
        let short = "abc";
        assert_eq!(truncate_detail(short), "abc");
    }

    #[test]
    fn waffle_status_strings_match_spec() {
        // Spec §3 uses PascalCase enum names for the waffle_status column.
        assert_eq!(WaffleStatus::Passed.as_str(), "Passed");
        assert_eq!(WaffleStatus::Failed.as_str(), "Failed");
        assert_eq!(WaffleStatus::Errored.as_str(), "Errored");
        assert_eq!(WaffleStatus::MissingDump.as_str(), "MissingDump");
    }

    #[test]
    fn runaway_detail_string_matches_spec() {
        // Spec §3 col `cherchi_detail` row 5: literal string for runaway.
        assert_eq!(RUNAWAY_DETAIL, "runaway: subprocess killed at 10s");
    }
}
