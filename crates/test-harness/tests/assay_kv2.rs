//! PR-KV4 — categorized assay replay through `KernelV2Adapter` (kernel-v2).
//!
//! Replays the 193-case assay corpus (`app/tests/cases/assay`) through the
//! real feature-engine dispatch path with kernel-v2 behind the legacy
//! `Kernel` trait, and categorizes EVERY case:
//!
//! - `SUPPORTED_CORRECT` — replay succeeded and validation passed (kernel-v2
//!   `validate_solid` runs inside every constructor/boolean/tessellate call;
//!   here we additionally require a non-empty mesh and the legacy replay's
//!   mesh oracles: watertight, consistent/outward normals, no degenerate
//!   triangles, valid indices/face ranges, positive signed volume, no
//!   self-intersection, Euler characteristic, volume magnitude, minimum
//!   triangle count, bbox extent).
//! - `SUPPORTED_WRONG` — the case replayed (no NotSupported boundary hit)
//!   but validation failed. These are REAL kernel-v2/yang-rs/adapter bugs.
//! - `UNSUPPORTED(reason)` — the replay hit a loud `KernelError::NotSupported`
//!   boundary: revolve / curved profile (circle) / coplanar boolean (Yang
//!   Stage 0, roadmap M8) / fillet-chamfer-shell / other.
//! - `ERROR` — an unexpected failure (anything that is neither clean success
//!   nor a declared NotSupported boundary).
//!
//! This is the NEW kernel's categorized score — there is deliberately no
//! yang_comparison-style legacy scoring here.
//!
//! Tests:
//! - `smoke_*` (always on) — the regression gate: synthetic planar scenarios
//!   through the full dispatch path that must be SUPPORTED_CORRECT (the
//!   corpus itself contains ZERO Phase-4a-boundary cases — see the smoke
//!   section comment), plus representative corpus cases pinned to their
//!   expected categories (UNSUPPORTED boundaries, the PR-TH1 oracle-fix
//!   movers, and the one known-WRONG case).
//! - `full_corpus_categorized` (`#[ignore]`) — the full corpus run; prints
//!   the category table and writes `target/assay_kv2_report.json`. Run with:
//!   `cargo test -p test-harness --test assay_kv2 -- --ignored --nocapture`
//!   By default cases run as parallel killable subprocesses (`ASSAY_JOBS`,
//!   default 4; each re-invokes this binary's `single_case`);
//!   `ASSAY_JOBS=1` selects the historical in-process serial path. Verdicts
//!   are per-case deterministic. The parallel path budgets each case on
//!   **CPU time** (Linux `/proc/<pid>/stat`), so TIMEOUT no longer depends on
//!   sibling load — a case starved by neighbours accrues wall time but not CPU
//!   time and keeps its real verdict; only the serial (in-process) path and
//!   non-Linux still budget on wall clock. Env-gated stage probes print to the
//!   child's nulled stderr under parallel runs — use `ASSAY_JOBS=1` or a manual
//!   `ASSAY_CASE=<id> ... single_case` run when probing. Every full run stamps
//!   build mode / jobs / budget kind / wall time into the JSON report's `run`
//!   block so a TIMEOUT count is interpretable after the fact.
//!
//!   A/B measured 2026-07-07 (24-core box, 120s cap): serial 76.5 min /
//!   parallel(22) 5.8 min, categories identical except 8 borderline-slow
//!   cases flipping X→TIMEOUT under sibling contention. For runs that
//!   COMMIT the baseline results.json, raise the cap to absorb contention
//!   (`ASSAY_CASE_TIMEOUT_SECS=240` parallel ≈ 10 min ≪ any serial run) so
//!   near-cap cases keep real verdicts; quick P9 gates can use the default.

use std::collections::BTreeSet;
use std::fmt::Write as _;
use std::fs;
use std::path::{Path, PathBuf};
use std::time::Duration;

use test_harness::assay::gen::AssayMeta;
use test_harness::assay::randomized_runner::{discover_cases, DiscoveredCase};
use test_harness::helpers::mesh_bounding_box;
use test_harness::oracle;
use test_harness::ModelBuilder;

// ── Categories ─────────────────────────────────────────────────────────────

#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord)]
enum UnsupportedReason {
    Revolve,
    CurvedProfile,
    CoplanarBoolean,
    FilletChamferShell,
    MultiShell,
    Other,
}

impl UnsupportedReason {
    fn label(self) -> &'static str {
        match self {
            Self::Revolve => "revolve",
            Self::CurvedProfile => "curved-profile",
            Self::CoplanarBoolean => "coplanar-boolean",
            Self::FilletChamferShell => "fillet-chamfer-shell",
            Self::MultiShell => "multi-shell",
            Self::Other => "other",
        }
    }

    /// Inverse of [`label`] — the subprocess outcome-line codec (ASSAY_JOBS).
    fn from_label(s: &str) -> Option<Self> {
        Some(match s {
            "revolve" => Self::Revolve,
            "curved-profile" => Self::CurvedProfile,
            "coplanar-boolean" => Self::CoplanarBoolean,
            "fillet-chamfer-shell" => Self::FilletChamferShell,
            "multi-shell" => Self::MultiShell,
            "other" => Self::Other,
            _ => return None,
        })
    }
}

#[derive(Debug, Clone, PartialEq, Eq)]
enum Category {
    SupportedCorrect,
    SupportedWrong,
    Unsupported(UnsupportedReason),
    /// The meta EXPECTS a rebuild error and the engine raised one (the
    /// self-intersection canaries F0073/F0074). The canary fired correctly
    /// — but PASS is reserved for fully-supported WORKING geometry, so this
    /// reports as an error status with the expectation as context.
    ExpectedError,
    Error,
    /// The case exceeded the per-case timeout. A distinct category — NOT
    /// `Error` (it's "too slow to judge here," usually the heavy exact-
    /// arithmetic gear arrangements), and NOT silently dropped. Feeds the
    /// auto slow-list that `ASSAY_FAST=1` skips for a quick baseline.
    Timeout,
    /// Skipped because it is on the slow-list and `ASSAY_FAST=1` was set.
    SkippedSlow,
}

impl Category {
    fn label(&self) -> String {
        match self {
            Self::SupportedCorrect => "SUPPORTED_CORRECT".to_string(),
            Self::SupportedWrong => "SUPPORTED_WRONG".to_string(),
            Self::Unsupported(r) => format!("UNSUPPORTED({})", r.label()),
            Self::ExpectedError => "EXPECTED_ERROR".to_string(),
            Self::Error => "ERROR".to_string(),
            Self::Timeout => "TIMEOUT".to_string(),
            Self::SkippedSlow => "SKIPPED_SLOW".to_string(),
        }
    }

    /// Inverse of [`label`] — the subprocess outcome-line codec (ASSAY_JOBS).
    fn from_label(s: &str) -> Option<Self> {
        Some(match s {
            "SUPPORTED_CORRECT" => Self::SupportedCorrect,
            "SUPPORTED_WRONG" => Self::SupportedWrong,
            "EXPECTED_ERROR" => Self::ExpectedError,
            "ERROR" => Self::Error,
            "TIMEOUT" => Self::Timeout,
            "SKIPPED_SLOW" => Self::SkippedSlow,
            other => {
                let inner = other.strip_prefix("UNSUPPORTED(")?.strip_suffix(')')?;
                Self::Unsupported(UnsupportedReason::from_label(inner)?)
            }
        })
    }
}

struct CaseOutcome {
    id: String,
    category: Category,
    detail: String,
}

// ── Replay ─────────────────────────────────────────────────────────────────

fn assay_dir() -> PathBuf {
    PathBuf::from(env!("CARGO_MANIFEST_DIR"))
        .parent()
        .unwrap()
        .parent()
        .unwrap()
        .join("app/tests/cases/assay")
}

/// Classify a `NotSupported` message (engine error or auto-union warning
/// text) into the adapter's declared unsupported boundaries.
///
/// Classification runs on the text AFTER the `operation not supported:`
/// marker — the adapter's typed reason — never on the failing feature's
/// name. An auto-union warning reads "Revolve 3: Auto-union failed: …
/// operation not supported: boolean_union: coplanar input face pair …";
/// matching the whole message keyed on "Revolve" and mislabeled coplanar
/// walls as UNSUPPORTED(revolve) (R0015/R0053; the same mislabel hid
/// R0085's coplanar wall until task #131).
fn unsupported_reason(msg: &str) -> UnsupportedReason {
    let reason = msg
        .split_once(NOT_SUPPORTED_MARKER)
        .map_or(msg, |(_, after)| after);
    let m = reason.to_lowercase();
    if m.contains("revolve") {
        UnsupportedReason::Revolve
    } else if m.contains("circle") || m.contains("curved") || m.contains("arc") {
        UnsupportedReason::CurvedProfile
    } else if m.contains("coplanar") {
        UnsupportedReason::CoplanarBoolean
    } else if m.contains("multi-shell") {
        // PR-KV7: the multi-shell operand wall (internal voids / disjoint
        // bodies cannot re-enter yang). Checked BEFORE the fillet/chamfer/
        // shell bucket so "multi-shell" does not pattern-match "shell".
        UnsupportedReason::MultiShell
    } else if m.contains("fillet") || m.contains("chamfer") || m.contains("shell") {
        UnsupportedReason::FilletChamferShell
    } else {
        UnsupportedReason::Other
    }
}

const NOT_SUPPORTED_MARKER: &str = "operation not supported:";

/// The auto-union message shape embeds the FAILING FEATURE's name before
/// the marker; the reason bucket must come from the adapter's typed text
/// after it (R0015/R0053 were mislabeled UNSUPPORTED(revolve) for their
/// coplanar wall).
#[test]
fn unsupported_reason_ignores_feature_name_prefix() {
    assert_eq!(
        unsupported_reason(
            "Revolve 3: Auto-union failed: kernel error: operation not supported: \
             boolean_union: coplanar input face pair (Yang Stage 0 coplanar \
             preprocessing — roadmap M8 — not yet implemented)"
        ),
        UnsupportedReason::CoplanarBoolean
    );
    // A genuine revolve wall still classifies as revolve (marker present).
    assert_eq!(
        unsupported_reason(
            "abc-123: operation error: kernel error: operation not supported: \
             revolve_face: full-turn circle profile sweeps a CLOSED torus \
             (kernel-v2 roadmap KV6d; PARTIAL-turn circle revolve → torus is supported)"
        ),
        UnsupportedReason::Revolve
    );
    // Marker-less text (defensive): falls back to whole-message matching.
    assert_eq!(
        unsupported_reason("coplanar input face pair"),
        UnsupportedReason::CoplanarBoolean
    );
}

/// Replay one corpus case through feature-engine + `KernelV2Adapter` and
/// categorize the outcome. Mirrors the legacy randomized runner's replay
/// shape (load → engine errors → tessellate last → mesh oracles) but with
/// the NotSupported-boundary categorization in front.
fn replay_case(case: &DiscoveredCase) -> CaseOutcome {
    let err_outcome = |detail: String| CaseOutcome {
        id: case.id.clone(),
        category: Category::Error,
        detail,
    };

    let waffle_json = match fs::read_to_string(&case.waffle_path) {
        Ok(s) => s,
        Err(e) => return err_outcome(format!("cannot read .waffle: {e}")),
    };
    let meta: AssayMeta = match fs::read_to_string(&case.meta_path)
        .map_err(|e| e.to_string())
        .and_then(|s| serde_json::from_str(&s).map_err(|e| e.to_string()))
    {
        Ok(m) => m,
        Err(e) => return err_outcome(format!("cannot read .meta.json: {e}")),
    };

    let mut builder = ModelBuilder::kernel_v2();
    if let Err(e) = builder.load(&waffle_json) {
        return err_outcome(format!("LoadProject failed: {e}"));
    }

    let engine_errors: Vec<String> = builder
        .engine_errors()
        .iter()
        .map(|(id, msg)| format!("{id}: {msg}"))
        .collect();
    let warnings: Vec<String> = builder.engine_warnings().to_vec();

    // 1. NotSupported boundary? Check engine errors first (rebuild failures),
    //    then warnings (the merge=true auto-union path downgrades a boolean
    //    error to an "Auto-union failed: …" warning).
    let not_supported_msgs: Vec<&String> = engine_errors
        .iter()
        .chain(warnings.iter())
        .filter(|m| m.contains(NOT_SUPPORTED_MARKER))
        .collect();
    if let Some(first) = not_supported_msgs.first() {
        return CaseOutcome {
            id: case.id.clone(),
            category: Category::Unsupported(unsupported_reason(first)),
            detail: format!(
                "{} NotSupported boundary(ies); first: {}",
                not_supported_msgs.len(),
                first
            ),
        };
    }

    // 2. Cases whose meta EXPECTS a rebuild error (legacy: disjoint-operand
    //    unions). If kernel-v2 also errors (for a non-NotSupported reason),
    //    that is the expected behavior; if it succeeds, fall through to
    //    normal mesh validation — succeeding with a valid (multi-shell)
    //    result is not wrong for the new kernel.
    if meta.oracles.expect_rebuild_error && !engine_errors.is_empty() {
        return CaseOutcome {
            id: case.id.clone(),
            category: Category::ExpectedError,
            detail: format!("expected rebuild error: {}", engine_errors.join("; ")),
        };
    }

    // 3. Any other engine error is an unexpected failure.
    if !engine_errors.is_empty() {
        return err_outcome(format!(
            "{} engine error(s): {}",
            engine_errors.len(),
            engine_errors.join("; ")
        ));
    }

    // 3b. An auto-union failure that is NOT a declared NotSupported boundary
    //     is an unexpected boolean failure (the merge=true path downgrades
    //     it to a warning and leaves separate bodies, so without this check
    //     it would masquerade as a merge-incomplete SUPPORTED_WRONG).
    let union_failures: Vec<&String> = warnings
        .iter()
        .filter(|w| w.contains("Auto-union failed"))
        .collect();
    if !union_failures.is_empty() {
        return err_outcome(format!(
            "{} auto-union failure(s): {}",
            union_failures.len(),
            union_failures
                .iter()
                .map(|s| s.as_str())
                .collect::<Vec<_>>()
                .join("; ")
        ));
    }

    // 4. Tessellate the last solid (scale-adaptive tolerance like the legacy
    //    runner; the adapter's planar tessellation is exact and ignores it).
    let tess_tol = (meta.scale * 0.01).clamp(1e-9, 0.1);
    let mesh = match builder.tessellate_last_with_tol(tess_tol) {
        Ok(m) => m,
        Err(e) => return err_outcome(format!("no solid / tessellation failed: {e}")),
    };

    // 5. Validation: the legacy replay's mesh oracles + meta expectations.
    let mut failures: Vec<String> = Vec::new();
    for v in oracle::run_all_mesh_checks(&mesh) {
        if !v.passed {
            failures.push(format!("{}: {}", v.oracle_name, v.detail));
        }
    }
    if mesh.indices.is_empty() {
        failures.push("empty mesh: no triangles".to_string());
    }
    // Spec `cut_consumes_body`: when the engine explicitly reports a boolean
    // that CONSUMED a body (an engulfing cut / empty intersect), the ops that
    // built that body contribute no final triangles, so the op-derived
    // minimum is not a valid expectation — the volume/euler/watertight
    // oracles still validate the survivors. Only the engine's typed
    // consumption warning skips the check; never relaxed on error paths.
    let body_consumed = builder
        .engine_warnings()
        .iter()
        .any(|w| w.contains("consumed the entire target body") || w.contains("no material"));
    if !body_consumed {
        let ops: Vec<(String, String)> = meta
            .operations
            .iter()
            .map(|o| (o.kind.clone(), o.profile_type.clone()))
            .collect();
        let v = oracle::check_minimum_triangle_count(&mesh, &ops);
        if !v.passed {
            failures.push(format!("minimum_triangle_count: {}", v.detail));
        }
    }
    if !mesh.vertices.is_empty() {
        let v = oracle::check_volume_magnitude(&mesh, meta.scale);
        if !v.passed {
            failures.push(format!("volume_magnitude: {}", v.detail));
        }
        // C-series exact-volume oracle: the meta carries an analytic volume
        // computed at generation time from kernel-independent arithmetic.
        // Multi-body cases must sum ALL bodies, so tessellate the whole model.
        if let Some(expected) = meta.oracles.expected_volume {
            let tol_rel = meta.oracles.expected_volume_tol_rel.unwrap_or(1e-3);
            let vol = builder
                .tessellate_live_with_tol(tess_tol)
                .map(|meshes| {
                    meshes
                        .iter()
                        .map(test_harness::helpers::mesh_signed_volume)
                        .sum::<f64>()
                })
                .unwrap_or_else(|_| test_harness::helpers::mesh_signed_volume(&mesh));
            if (vol - expected).abs() > tol_rel * expected.abs() {
                failures.push(format!(
                    "expected_volume: {vol:.9e} vs expected {expected:.9e} (rel tol {tol_rel:.1e})"
                ));
            }
        }
        let v = oracle::check_mesh_euler_characteristic(&mesh, meta.oracles.euler_target);
        if !v.passed {
            failures.push(format!("mesh_euler_characteristic: {}", v.detail));
        }
        let (bb_min, bb_max) = mesh_bounding_box(&mesh);
        let dx = (bb_max[0] - bb_min[0]) as f64;
        let dy = (bb_max[1] - bb_min[1]) as f64;
        let dz = (bb_max[2] - bb_min[2]) as f64;
        let diagonal = (dx * dx + dy * dy + dz * dz).sqrt();
        if diagonal > meta.oracles.max_bbox_extent {
            failures.push(format!(
                "bbox diagonal {:.3e} exceeds max {:.3e}",
                diagonal, meta.oracles.max_bbox_extent
            ));
        }
    }
    // Multi-op cases must end as a single merged body (legacy runner check) —
    // unless the meta declares a deliberate multi-body count (C-series 3a).
    if let Some(expected_solids) = meta.oracles.expected_solid_count {
        let solid_count = builder.distinct_solid_count();
        if solid_count != expected_solids {
            failures.push(format!(
                "solid count: {solid_count} bodies (meta expects {expected_solids})"
            ));
        }
    } else if meta.operations.len() > 1 {
        let solid_count = builder.distinct_solid_count();
        if solid_count > 1 {
            failures.push(format!(
                "merge incomplete: {} operations produced {} separate solids",
                meta.operations.len(),
                solid_count
            ));
        }
    }

    if failures.is_empty() {
        CaseOutcome {
            id: case.id.clone(),
            category: Category::SupportedCorrect,
            detail: "all checks passed".to_string(),
        }
    } else {
        CaseOutcome {
            id: case.id.clone(),
            category: Category::SupportedWrong,
            detail: failures.join("; "),
        }
    }
}

/// Replay with a hang guard (booleans go through the yang-rs/cherchi-rs
/// pipeline; a hung case must not wedge the whole run).
fn replay_case_with_timeout(case: &DiscoveredCase, timeout: Duration) -> CaseOutcome {
    let (tx, rx) = std::sync::mpsc::channel();
    let id = case.id.clone();
    let c = DiscoveredCase {
        id: case.id.clone(),
        waffle_path: case.waffle_path.clone(),
        meta_path: case.meta_path.clone(),
    };
    let handle = std::thread::spawn(move || {
        let _ = tx.send(replay_case(&c));
    });
    match rx.recv_timeout(timeout) {
        Ok(r) => {
            let _ = handle.join();
            r
        }
        Err(_) => CaseOutcome {
            id,
            // Distinct from Error: the orphaned worker thread keeps running
            // (heavy exact arithmetic can't be safely killed in-process), but
            // the run moves on and the case is flagged TIMEOUT, not failed.
            category: Category::Timeout,
            detail: format!("timeout after {}s", timeout.as_secs()),
        },
    }
}

/// CPU seconds (user+system, summed across all threads) consumed so far by
/// `pid`, read from `/proc/<pid>/stat` fields 14 (`utime`) + 15 (`stime`).
/// Linux only; `None` when the file is gone (process exited) or unparseable.
///
/// This is what makes the parallel per-case budget load-INSENSITIVE: a case
/// starved by siblings accrues wall time but not CPU time, so budgeting on CPU
/// (below) gives it the same verdict whether it ran alone or under contention
/// — killing the timeout-cascade false-positives (see the campaign that traced
/// 20 "timeouts" to a debug/loaded-box run of correct-but-slow cases).
///
/// USER_HZ (`sysconf(_SC_CLK_TCK)`) is 100 on effectively every Linux config;
/// std exposes no sysconf, so we assume 100 (Ref: `man 5 proc`, "utime").
#[cfg(target_os = "linux")]
fn child_cpu_secs(pid: u32) -> Option<f64> {
    let stat = std::fs::read_to_string(format!("/proc/{pid}/stat")).ok()?;
    // The `comm` field (2nd) may itself contain spaces and ')' — everything up
    // to and including the LAST ')' is `pid (comm)`, so split there first.
    let rest = stat.rsplit_once(')')?.1;
    // `rest` now begins with " <state> <ppid> …"; after whitespace-splitting,
    // index i holds stat field (i + 3). utime = field 14 → idx 11; stime =
    // field 15 → idx 12.
    let f: Vec<&str> = rest.split_whitespace().collect();
    let utime: u64 = f.get(11)?.parse().ok()?;
    let stime: u64 = f.get(12)?.parse().ok()?;
    Some((utime + stime) as f64 / 100.0)
}

#[cfg(not(target_os = "linux"))]
fn child_cpu_secs(_pid: u32) -> Option<f64> {
    None // no /proc → the caller falls back to a wall-clock deadline.
}

/// ASSAY_JOBS driver: replay one case in a KILLABLE subprocess — this test
/// binary re-invoked with `--exact single_case` + `ASSAY_CASE=<id>` — and
/// parse its `ASSAY_OUTCOME {json}` line. On deadline the child is KILLED:
/// a real kill, unlike the in-process path's abandoned worker thread, so a
/// parallel run cannot accumulate orphan CPU load (the timeout-cascade trap
/// documented in `full_corpus_categorized`). Child stderr goes to null —
/// env-gated probes need the serial path or a manual `single_case` run.
///
/// `budget` is a **CPU-time** budget on Linux (the process's summed thread CPU
/// from [`child_cpu_secs`]), with a generous wall-clock cap as a safety net for
/// a genuinely blocked child that burns no CPU. On non-Linux the CPU probe is
/// unavailable and `budget` degrades to a plain wall deadline (legacy). CPU
/// budgeting is why parallel verdicts stop depending on box load.
fn replay_case_subprocess(id: &str, budget: Duration) -> CaseOutcome {
    use std::io::Read as _;
    use std::process::{Command, Stdio};
    let err_outcome = |detail: String| CaseOutcome {
        id: id.to_string(),
        category: Category::Error,
        detail,
    };
    let cpu_budget = budget.as_secs_f64();
    // Wall safety net: a BLOCKED child (deadlock / I/O wait) accrues no CPU and
    // would never trip the CPU budget, so cap wall at a generous multiple to
    // still terminate it. CPU-bound geometry — the norm — trips the CPU budget
    // first, which is what decouples the verdict from box load.
    let wall_cap = Duration::from_secs_f64((cpu_budget * 4.0).max(cpu_budget + 120.0));
    let exe = std::env::current_exe().expect("current_exe of the test binary");
    let mut child = match Command::new(&exe)
        .args(["--exact", "single_case", "--ignored", "--nocapture"])
        .env("ASSAY_CASE", id)
        // The parent CPU/wall kill below is the real deadline; keep the child's
        // own in-process (wall) guard strictly behind the parent's wall cap so
        // the parent always decides first (fallback only).
        .env(
            "ASSAY_CASE_TIMEOUT_SECS",
            (wall_cap.as_secs() + 60).to_string(),
        )
        .stdin(Stdio::null())
        .stdout(Stdio::piped())
        .stderr(Stdio::null())
        .spawn()
    {
        Ok(c) => c,
        Err(e) => return err_outcome(format!("driver: cannot spawn subprocess: {e}")),
    };
    // Drain stdout concurrently (an undrained full pipe would wedge the child).
    let mut stdout = child.stdout.take().expect("piped child stdout");
    let reader = std::thread::spawn(move || {
        let mut buf = String::new();
        let _ = stdout.read_to_string(&mut buf);
        buf
    });
    let pid = child.id();
    // Whether the CPU probe works for this child (Linux + readable /proc). If
    // not, `over_cpu` stays false and only the wall cap fires — legacy behaviour
    // with `budget` acting as the wall deadline (wall_cap == budget on that
    // path would be too tight, so non-Linux keeps the generous cap too; a
    // non-CPU-budgeted parallel run there is simply the old load-sensitive
    // behaviour, documented in the report's `budget_kind`).
    let cpu_probe = child_cpu_secs(pid).is_some();
    let wall_deadline = std::time::Instant::now() + if cpu_probe { wall_cap } else { budget };
    let status = loop {
        match child.try_wait() {
            Ok(Some(status)) => break Ok(status),
            Ok(None) => {
                let over_cpu = cpu_probe && child_cpu_secs(pid).is_some_and(|c| c >= cpu_budget);
                let over_wall = std::time::Instant::now() >= wall_deadline;
                if over_cpu || over_wall {
                    let _ = child.kill();
                    let _ = child.wait();
                    // Distinct detail strings so a wall-cap kill (possible
                    // deadlock / non-CPU stall) is never confused with a genuine
                    // CPU-budget overrun.
                    break Err(if over_cpu {
                        format!("timeout after {cpu_budget:.0}s CPU (subprocess killed)")
                    } else {
                        let cap = if cpu_probe { wall_cap } else { budget };
                        format!(
                            "wall cap {}s exceeded — child burned <{cpu_budget:.0}s CPU, likely stalled (subprocess killed)",
                            cap.as_secs()
                        )
                    });
                }
                std::thread::sleep(Duration::from_millis(50));
            }
            Err(e) => {
                let _ = child.kill();
                let _ = child.wait();
                break Err(format!("driver: wait failed: {e} (subprocess killed)"));
            }
        }
    };
    let out = reader.join().unwrap_or_default();
    let status = match status {
        Ok(s) => s,
        Err(timeout_detail) => {
            return CaseOutcome {
                id: id.to_string(),
                category: Category::Timeout,
                detail: timeout_detail,
            }
        }
    };
    if let Some(line) = out.lines().rev().find(|l| l.starts_with("ASSAY_OUTCOME ")) {
        if let Ok(v) = serde_json::from_str::<serde_json::Value>(&line["ASSAY_OUTCOME ".len()..]) {
            if let (Some(cid), Some(category), Some(detail)) = (
                v["id"].as_str(),
                v["category"].as_str().and_then(Category::from_label),
                v["detail"].as_str(),
            ) {
                if cid == id {
                    return CaseOutcome {
                        id: cid.to_string(),
                        category,
                        detail: detail.to_string(),
                    };
                }
            }
        }
    }
    // No parseable outcome line: the child crashed (panic/abort/OOM) — a
    // loud ERROR with the exit status, never a silent drop.
    err_outcome(format!(
        "driver: subprocess exited ({status}) without a parseable ASSAY_OUTCOME line"
    ))
}

/// Auto slow-list: the set of case ids that timed out or ran slow in a prior
/// full run. `ASSAY_FAST=1` skips these for a sub-minute baseline. Lives under
/// `target/` (gitignored, ephemeral) — absent ⇒ nothing is skipped.
fn slow_cases_path() -> PathBuf {
    PathBuf::from(env!("CARGO_MANIFEST_DIR"))
        .parent()
        .unwrap()
        .parent()
        .unwrap()
        .join("target/assay_slow_cases.txt")
}

fn read_slow_cases(path: &Path) -> BTreeSet<String> {
    fs::read_to_string(path)
        .map(|s| {
            s.lines()
                .map(|l| l.trim().to_string())
                .filter(|l| !l.is_empty() && !l.starts_with('#'))
                .collect()
        })
        .unwrap_or_default()
}

fn write_slow_cases(path: &Path, ids: &BTreeSet<String>) {
    let mut body = String::from(
        "# auto-generated by assay_kv2 full_corpus_categorized\n\
         # case ids that timed out or ran slow; ASSAY_FAST=1 skips these\n",
    );
    for id in ids {
        let _ = writeln!(body, "{id}");
    }
    let _ = fs::write(path, body);
}

// ── Smoke subset (always-on regression gate) ───────────────────────────────
//
// HONEST FINDING (PR-KV4 full-corpus run): the assay corpus contains ZERO
// cases inside kernel-v2's Phase-4a boundary — every one of the 190 cases
// has ≥ 2 operations, and every multi-op planar case either auto-unions
// coplanar-coincident solids (the declared Yang Stage 0 / M8 boundary) or
// hits a real yang-rs boolean defect (see SUPPORTED_WRONG / ERROR in the
// full run). So the always-on SUPPORTED_CORRECT gate is built from
// synthetic scenarios driven through the SAME full dispatch path
// (wasm-bridge → feature-engine → KernelV2Adapter), hand-placed to avoid
// coplanar face pairs: single boxes, an oblique-plane box, a non-convex
// polygon, an auto-union boss, and explicit subtract / intersect / cut
// operations. A separate corpus test pins representative corpus cases to
// their expected UNSUPPORTED categories so the corpus boundary itself is
// also regression-gated.

/// FINDING KV4-F3 (PR-KV4, reported — NOT patched around, per P9),
/// NARROWED at PR-TH1: the original finding allowed `watertight_mesh`,
/// `no_self_intersection`, and `no_degenerate_triangles` on ALL boolean
/// smoke scenarios because `kernel_v2::tessellate` drops exactly-collinear
/// chain vertices per face independently (one long boundary edge vs two
/// short ones on the neighbor). PR-TH1 made the watertight/χ oracles
/// T-junction-aware (that conforming-under-subdivision shape is now scored
/// clean) and normalized the penetration-depth guard, after which
/// `union_offset_boss`, `blind_pocket_cut`, and `through_hole_cut` pass the
/// FULL oracle set with no allowances. What remains — on the subtract and
/// intersect scenarios only — is a REAL tessellation defect: one degenerate
/// (zero-area) sliver triangle whose collapsed edges also break pairing
/// (1 boundary + 1 non-manifold edge that do NOT close under subdivision).
/// Allow exactly those two oracles there; remove when the boolean
/// tessellation stops emitting the sliver.
const KV4_F3_ALLOWED: &[&str] = &["watertight_mesh", "no_degenerate_triangles"];

/// Assert a dispatch-path scenario is SUPPORTED_CORRECT: no engine errors,
/// no NotSupported / auto-union-failure warnings, and the final mesh passes
/// the full legacy oracle set (plus an exact-volume check where given).
fn assert_scenario_supported_correct(
    name: &str,
    builder: &mut ModelBuilder,
    expect_volume: Option<f64>,
) {
    assert_scenario_with_allowances(name, builder, expect_volume, &[]);
}

/// Like [`assert_scenario_supported_correct`] but with a named list of
/// oracle failures tied to a documented finding (see [`KV4_F3_ALLOWED`]).
fn assert_scenario_with_allowances(
    name: &str,
    builder: &mut ModelBuilder,
    expect_volume: Option<f64>,
    allowed_failures: &[&str],
) {
    let errors = builder.engine_errors().to_vec();
    assert!(errors.is_empty(), "{name}: engine errors: {errors:?}");
    let bad_warnings: Vec<String> = builder
        .engine_warnings()
        .iter()
        .filter(|w| w.contains("Auto-union failed") || w.contains(NOT_SUPPORTED_MARKER))
        .cloned()
        .collect();
    assert!(
        bad_warnings.is_empty(),
        "{name}: NotSupported / auto-union warnings: {bad_warnings:?}"
    );

    let mesh = builder
        .tessellate_last_with_tol(0.001)
        .unwrap_or_else(|e| panic!("{name}: tessellation failed: {e}"));
    assert!(!mesh.indices.is_empty(), "{name}: empty mesh");
    let failures: Vec<String> = oracle::run_all_mesh_checks(&mesh)
        .into_iter()
        .filter(|v| !v.passed && !allowed_failures.contains(&v.oracle_name.as_str()))
        .map(|v| format!("{}: {}", v.oracle_name, v.detail))
        .collect();
    assert!(
        failures.is_empty(),
        "{name}: mesh oracles failed: {failures:?}"
    );

    if let Some(expected) = expect_volume {
        let vol = test_harness::helpers::mesh_signed_volume(&mesh);
        assert!(
            (vol - expected).abs() < 1e-3 * expected.max(1.0),
            "{name}: signed volume {vol} (expected {expected})"
        );
    }
}

#[test]
fn smoke_single_box() {
    let mut b = ModelBuilder::kernel_v2();
    b.rect_sketch("s", [0.0; 3], [0.0, 0.0, 1.0], 0.0, 0.0, 1.0, 1.0)
        .unwrap();
    b.extrude("e", "s", 2.0).unwrap();
    assert_scenario_supported_correct("single_box", &mut b, Some(2.0));
}

#[test]
fn smoke_thin_slab() {
    let mut b = ModelBuilder::kernel_v2();
    b.rect_sketch("s", [0.0; 3], [0.0, 0.0, 1.0], -2.0, -1.5, 4.0, 3.0)
        .unwrap();
    b.extrude("e", "s", 0.2).unwrap();
    assert_scenario_supported_correct("thin_slab", &mut b, Some(4.0 * 3.0 * 0.2));
}

#[test]
fn smoke_oblique_plane_box() {
    // Sketch plane with a non-axis-aligned unit normal (1, 2, 2)/3.
    let n = [1.0 / 3.0, 2.0 / 3.0, 2.0 / 3.0];
    let mut b = ModelBuilder::kernel_v2();
    b.rect_sketch("s", [0.5, -0.25, 0.75], n, 0.0, 0.0, 1.0, 0.8)
        .unwrap();
    b.extrude("e", "s", 0.6).unwrap();
    assert_scenario_supported_correct("oblique_plane_box", &mut b, Some(1.0 * 0.8 * 0.6));
}

#[test]
fn smoke_l_shaped_extrude() {
    // Non-convex profile: L-shape of area 3.
    let mut b = ModelBuilder::kernel_v2();
    b.polygon_sketch(
        "s",
        [0.0; 3],
        [0.0, 0.0, 1.0],
        &[
            (0.0, 0.0),
            (2.0, 0.0),
            (2.0, 1.0),
            (1.0, 1.0),
            (1.0, 2.0),
            (0.0, 2.0),
        ],
    )
    .unwrap();
    b.extrude("e", "s", 0.5).unwrap();
    assert_scenario_supported_correct("l_shaped_extrude", &mut b, Some(3.0 * 0.5));
}

#[test]
fn smoke_union_offset_boss() {
    // Box A: (0..1)² × z∈[0,1]. Boss B: (0.3..0.7)² sketched at z=0.25,
    // extruded 1.5 → z∈[0.25,1.75]. Overlapping, NO coplanar face pairs.
    // merge=true auto-unions through the adapter's boolean_union.
    let mut b = ModelBuilder::kernel_v2();
    b.rect_sketch("sa", [0.0; 3], [0.0, 0.0, 1.0], 0.0, 0.0, 1.0, 1.0)
        .unwrap();
    b.extrude("a", "sa", 1.0).unwrap();
    b.rect_sketch("sb", [0.0, 0.0, 0.25], [0.0, 0.0, 1.0], 0.3, 0.3, 0.4, 0.4)
        .unwrap();
    b.extrude("u", "sb", 1.5).unwrap();
    // Union volume: 1 + 0.4·0.4·1.5 − 0.4·0.4·0.75 (overlap z∈[0.25,1]).
    // Full oracle set since PR-TH1 (T-junction-aware pairing) — no allowances.
    assert_scenario_supported_correct("union_offset_boss", &mut b, Some(1.0 + 0.24 - 0.12));
    assert_eq!(
        b.distinct_solid_count(),
        1,
        "union must merge into one body"
    );
}

#[test]
fn smoke_union_face_to_face_stack() {
    // TRUE face-to-face union: 2-unit cube (0..2)² × z∈[0,2], 1-unit cube
    // (0.5..1.5)² sketched ON its top plane z=2, extruded 1 → z∈[2,3].
    // The small cube's bottom face lies strictly INSIDE the big cube's top
    // face — a coplanar face pair where neither operand swallows the other.
    // The union must be ONE 3-unit-tall body of volume 2³ + 1³ = 9.
    //
    // FINDING (this test's first run): kernel-v2 + yang-rs handle this
    // face-INSIDE-face coplanar contact correctly TODAY — the near-coplanar
    // NotSupported gate does not fire, and the result passes the FULL
    // oracle set with exact volume. The M8 coplanar wall (F0002/F0016/…)
    // is about coincident/overlapping face pairs, not strict containment.
    // This test pins that capability so a regression (or a gate widening
    // that swallows it) is loud.
    let mut b = ModelBuilder::kernel_v2();
    b.rect_sketch("sa", [0.0; 3], [0.0, 0.0, 1.0], 0.0, 0.0, 2.0, 2.0)
        .unwrap();
    b.extrude("a", "sa", 2.0).unwrap();
    b.rect_sketch("sb", [0.0, 0.0, 2.0], [0.0, 0.0, 1.0], 0.5, 0.5, 1.0, 1.0)
        .unwrap();
    b.extrude("boss", "sb", 1.0).unwrap();
    assert_scenario_supported_correct("union_face_to_face_stack", &mut b, Some(9.0));
    assert_eq!(
        b.distinct_solid_count(),
        1,
        "face-to-face union must merge into one body"
    );
    let mesh = b.tessellate_last_with_tol(0.001).unwrap();
    let (bb_min, bb_max) = mesh_bounding_box(&mesh);
    assert!(
        (f64::from(bb_max[2] - bb_min[2]) - 3.0).abs() < 1e-6,
        "body must be 3 units tall, got {}",
        bb_max[2] - bb_min[2]
    );
}

#[test]
fn smoke_subtract_offset_boxes() {
    // Blank (0..1)³ minus tool (0.4..1.4)² × z∈[-0.3,0.6] — offset on all
    // axes, no coplanar pairs. Volume 1 − 0.6³ = 0.784.
    let mut b = ModelBuilder::kernel_v2();
    b.rect_sketch("sa", [0.0; 3], [0.0, 0.0, 1.0], 0.0, 0.0, 1.0, 1.0)
        .unwrap();
    b.extrude_no_merge("a", "sa", 1.0).unwrap();
    b.rect_sketch("sb", [0.0, 0.0, -0.3], [0.0, 0.0, 1.0], 0.4, 0.4, 1.0, 1.0)
        .unwrap();
    b.extrude_no_merge("t", "sb", 0.9).unwrap();
    b.boolean_subtract("cut", "a", "t").unwrap();
    assert_scenario_with_allowances(
        "subtract_offset_boxes",
        &mut b,
        Some(1.0 - 0.216),
        KV4_F3_ALLOWED,
    );
}

#[test]
fn smoke_intersect_offset_boxes() {
    // Same operands as the subtract; intersection volume 0.6³ = 0.216.
    let mut b = ModelBuilder::kernel_v2();
    b.rect_sketch("sa", [0.0; 3], [0.0, 0.0, 1.0], 0.0, 0.0, 1.0, 1.0)
        .unwrap();
    b.extrude_no_merge("a", "sa", 1.0).unwrap();
    b.rect_sketch("sb", [0.0, 0.0, -0.3], [0.0, 0.0, 1.0], 0.4, 0.4, 1.0, 1.0)
        .unwrap();
    b.extrude_no_merge("t", "sb", 0.9).unwrap();
    b.boolean_intersect("common", "a", "t").unwrap();
    assert_scenario_with_allowances(
        "intersect_offset_boxes",
        &mut b,
        Some(0.216),
        KV4_F3_ALLOWED,
    );
}

#[test]
fn smoke_blind_pocket_cut() {
    // Box (0..1)³; cut tool (0.3..0.6)² sketched at z=1.5, cut depth 1.2 →
    // tool z∈[0.3,1.5] (the cut path auto-reverses toward the body). Blind
    // pocket, no coplanar pairs. Volume 1 − 0.3·0.3·0.7 = 0.937.
    let mut b = ModelBuilder::kernel_v2();
    b.rect_sketch("sa", [0.0; 3], [0.0, 0.0, 1.0], 0.0, 0.0, 1.0, 1.0)
        .unwrap();
    b.extrude("a", "sa", 1.0).unwrap();
    b.rect_sketch("sb", [0.0, 0.0, 1.5], [0.0, 0.0, 1.0], 0.3, 0.3, 0.3, 0.3)
        .unwrap();
    b.extrude_cut("pocket", "sb", 1.2).unwrap();
    // Full oracle set since PR-TH1 — no allowances.
    assert_scenario_supported_correct("blind_pocket_cut", &mut b, Some(1.0 - 0.09 * 0.7));
}

#[test]
fn smoke_through_hole_cut() {
    // Box (0..1)³; cut tool (0.3..0.6)² × z∈[-0.25,1.5] pierces both caps →
    // genus-1 through-hole. Volume 1 − 0.09.
    let mut b = ModelBuilder::kernel_v2();
    b.rect_sketch("sa", [0.0; 3], [0.0, 0.0, 1.0], 0.0, 0.0, 1.0, 1.0)
        .unwrap();
    b.extrude("a", "sa", 1.0).unwrap();
    b.rect_sketch("sb", [0.0, 0.0, 1.5], [0.0, 0.0, 1.0], 0.3, 0.3, 0.3, 0.3)
        .unwrap();
    b.extrude_cut("hole", "sb", 1.75).unwrap();
    // Full oracle set since PR-TH1 — no allowances.
    assert_scenario_supported_correct("through_hole_cut", &mut b, Some(1.0 - 0.09));
}

#[test]
fn smoke_two_standalone_bodies() {
    // Two disjoint no-merge boxes; both tessellate independently.
    let mut b = ModelBuilder::kernel_v2();
    b.rect_sketch("sa", [0.0; 3], [0.0, 0.0, 1.0], 0.0, 0.0, 1.0, 1.0)
        .unwrap();
    b.extrude_no_merge("a", "sa", 1.0).unwrap();
    b.rect_sketch("sb", [3.0, 0.0, 0.0], [0.0, 0.0, 1.0], 0.0, 0.0, 0.5, 0.5)
        .unwrap();
    b.extrude_no_merge("b", "sb", 0.5).unwrap();
    assert_scenario_supported_correct("two_standalone_bodies", &mut b, Some(0.5 * 0.5 * 0.5));
    assert_eq!(b.distinct_solid_count(), 2);
    let mesh_a = b.tessellate("a").expect("body a tessellates");
    assert_eq!(mesh_a.indices.len() / 3, 12);
}

/// Representative corpus cases pinned to their expected category — the
/// corpus-side regression gate (a silent change in where the boundary falls
/// is a finding, even when the score doesn't move).
///
/// PR-TH1 pin refresh: the mesh oracles were fixed to measure REAL defects
/// (T-junction-aware watertight/χ pairing, per-shell χ expectation,
/// normalized penetration depth), which moved F0003/F0009/F0010 (T-junction
/// false positives) and F0011–F0015 (2-shell disjoint unions, χ=4 is
/// correct) to SUPPORTED_CORRECT. Those are pinned below so an oracle or
/// kernel regression is loud.
#[test]
fn smoke_corpus_boundary_categories() {
    let dir = assay_dir();
    let cases = discover_cases(&dir);
    assert!(!cases.is_empty(), "assay corpus not found at {dir:?}");

    let expected: &[(&str, Category)] = &[
        // F0002: cross-shaped prisms with PARTIALLY-overlapping coplanar caps.
        // The M8 planar partial-overlap fix (interior-centroid fan fallback in
        // `triangulate_ring`, commit 8c64c236) takes F0002/F0004/F0006 end-to-end
        // — all mesh oracles pass. Stale-pin reconciliation: the pin lagged the
        // fix (the full tier isn't in the rewrite inner loop, so it stayed latent).
        ("F0002", Category::SupportedCorrect),
        // PR-TH1: previously pinned UNSUPPORTED(coplanar-boolean), but the
        // case replays cleanly; its only failures were oracle false
        // positives (one-sided collinear boundary subdivision from
        // kernel-v2's render tessellation). With T-junction-aware pairing
        // the mesh measures clean.
        ("F0003", Category::SupportedCorrect),
        ("F0008", Category::SupportedCorrect),
        ("F0009", Category::SupportedCorrect),
        ("F0010", Category::SupportedCorrect),
        // PR-TH1 (KV4-F4 triage): disjoint-union outputs are single solids
        // with TWO closed shells — χ_total = 4 = 2 per shell, and the old
        // "penetrations" were unnormalized grazing-guard false positives.
        // The outputs are correct; the oracle now scores them honestly.
        ("F0011", Category::SupportedCorrect),
        ("F0012", Category::SupportedCorrect),
        ("F0013", Category::SupportedCorrect),
        ("F0014", Category::SupportedCorrect),
        ("F0015", Category::SupportedCorrect),
        // PR-KV7 flip (was SupportedWrong since PR-TH1): the defect was a
        // T-junction seam — an original box edge [A,B] crossed at m, with
        // the chain [A,m]+[m,B] carried by coincident sheets (4 sheets
        // along the split, χ = 3). Output curve recovery's collinear-chain
        // fusion removes exactly that T-vertex class, and the case now
        // passes all mesh checks end-to-end (the KV6b-F1 class fix).
        ("R0029", Category::SupportedCorrect),
        // PR-KV10: the F0016-family (3 same-plane oblique bosses) used to
        // stop at the intra-coplanar wall because chained outputs carried
        // femto-distinct same-plane sibling plane bits (canonicalized in
        // to_yang) over near-duplicate junction vertices (planar I6
        // near-weld). PR-KV4-F1 then implemented the cherchi rational-ray
        // fallback (the C++ "requires rationals" exit) for the
        // sub-f64-resolution needle patches these chains produce — the
        // family is now correct end-to-end. (F0022 progresses to a
        // non-manifold reassembly wall — a separate finding.)
        ("F0017", Category::SupportedCorrect),
        ("F0016", Category::SupportedCorrect),
        ("F0018", Category::SupportedCorrect),
        ("F0019", Category::SupportedCorrect),
        ("F0021", Category::SupportedCorrect),
        ("F0025", Category::SupportedCorrect),
        // PR-KV5b: circle profiles now extrude to cylinder solids, so these
        // cases march PAST the old curved-profile wall to their next
        // boundary — the auto-union of coaxial cylinders is a coplanar pair
        // (cap-on-cap). PR-M8-disc-disc (Increment 1) handles disc∩disc
        // CONTAINMENT, so F0030 (coaxial cap-on-cap, one rim inside the other)
        // now succeeds end-to-end. F0086 stays the M8 residue (a coplanar
        // sub-case Increment 1 does not yet cover — crossing / multi-pair).
        ("F0030", Category::SupportedCorrect),
        // Task #62 increment 7 (2026-07-07): the corpus-path residual was
        // the PRODUCTION SKETCH FRAME (x=(0,−1,0)) reaching a Stage-0
        // configuration the canonical frame never hits — a femto-strip
        // sliver inverted by its rim vertex's on-circle mint, which the
        // fold gate then REVERTED to a chord position that escaped into
        // the output rims (`VertexOffSurface { FaceId(15) }`). Amendment 4
        // (spec `n2_stage4_junction_cluster_merge` §3) repairs repairable
        // folds with constrained Lawson edge flips instead of reverting;
        // the engine-frame chain (`m8_swiss_cheese_chain.rs::engine_frame_*`)
        // pins the mechanism at the kernel level. F0086 replays
        // end-to-end. Increment 8 (amendment 5, Fig-11 cavity relocation)
        // repaired the second fold class — the rim-mint COLUMN HOP — so
        // F0087/F0089/F0090 moved ERROR → the typed curved partial-patch
        // re-entry boundary they share with F0088 (the family's next
        // lever).
        ("F0086", Category::SupportedCorrect),
        // PR-TH2 (KV5b-F2 resolved): the enclosed-cavity families
        // F0031–F0035 (box-minus-cyl) and F0036–F0040 (cyl-minus-box)
        // succeed end-to-end: 2 closed genus-0 shells (outer + cavity),
        // χ = 4 — exactly what their metas' euler_target = 4 encodes.
        // The PR-TH1 per-shell adjustment used to add the second shell
        // AGAIN (expected 6); the oracle now decodes the meta's shell
        // count from euler_target and only credits shells BEYOND it.
        ("F0031", Category::SupportedCorrect),
        ("F0032", Category::SupportedCorrect),
        ("F0033", Category::SupportedCorrect),
        ("F0034", Category::SupportedCorrect),
        ("F0035", Category::SupportedCorrect),
        ("F0036", Category::SupportedCorrect),
        ("F0037", Category::SupportedCorrect),
        ("F0038", Category::SupportedCorrect),
        ("F0039", Category::SupportedCorrect),
        ("F0040", Category::SupportedCorrect),
        // F0044: cylinder-boolean case passing the FULL oracle set
        // end-to-end. (R0006 was its companion until PR-ASSAY-NOOP.)
        // PR-KV11: R0006's oblique section resolved (ellipse junction
        // relocation). KV14 ellipse-arc re-entry (spec
        // `kv14_ellipse_arc_reentry`) lifted the degree-4 boundary wall its
        // NEXT op used to hit: EllipseArc boundaries now convert to yang
        // `Curve::Ellipse` chains, so the full 3-op chain replays. Its meta
        // euler_target was ALSO corrected 2 → 0 (the oblique cut is a true
        // through-tunnel: the void breaches two disjoint openings on
        // opposite box faces → genus-1; the disjoint third boss adds the
        // second shell — the R0099 authoring-error pattern, verified by
        // slab/fiber analysis of the cut cylinder vs the box).
        ("R0006", Category::SupportedCorrect),
        ("F0044", Category::SupportedCorrect),
        // F0046: oblique box plane × cylinder sections (ELLIPSE arcs meeting
        // at junctions). PR-KV11: junction relocation + the hybrid
        // exact/quantized mesh oracles take it end-to-end.
        ("F0046", Category::SupportedCorrect),
        // F0041: cylinder×cylinder lateral∩lateral (degree-4). Stale-pin
        // reconciliation (2026-07-05): the case passes end-to-end on main and
        // the committed results.json agrees — the KV9 tangency/junction work
        // took this class past the old Stage-3 AmbiguousCurve wall. Pinned
        // green so a regression is loud.
        ("F0041", Category::SupportedCorrect),
        // R0067: was a yang Stage-5 NoExplicitRayOrigin wall on a curved patch;
        // that path has since been resolved and the case now replays correctly
        // (all mesh oracles pass). Stale-pin reconciliation — pre-existing
        // drift, independent of the M8 coplanar work.
        ("R0067", Category::SupportedCorrect),
        // F0091: TRUE face-to-face union — 1u cube extruded ON the 2u cube's
        // top face (bottom face strictly inside the top face). The coplanar
        // NotSupported gate does not fire for strict containment and the
        // union is correct end-to-end (see smoke_union_face_to_face_stack
        // for the exact-volume version of this scenario).
        ("F0091", Category::SupportedCorrect),
        // PR-KV6a: revolve is implemented for axis-aligned polygon profiles
        // (partial + full 360°). The self-intersection canaries exercise the
        // REAL validation: F0074 places the axis THROUGH the profile
        // interior, and the typed RevolveAxisIntersectsProfile maps to the
        // plain rebuild error its meta expects.
        //
        // F0073 PIN MOVED at KV6 slice 3 (spec
        // `kv6_on_axis_revolve_partial_wedge.md`): its axis TOUCHES the
        // profile boundary — the on-axis lathe wedge class, now a SUPPORTED
        // construction (the meta's expected-error is stale the same way
        // C0035-F1's was: authored against a since-closed capability gap).
        // PIN MOVED AGAIN at the M8 plane-group n-ary overlay (task #129,
        // spec `m8_plane_group_nary_overlay`): the wedge's auto-union used
        // to stop at the multi-pair coplanar wall; the plane-grouped
        // overlay takes the case end-to-end — all mesh oracles pass.
        ("F0073", Category::SupportedCorrect),
        ("F0074", Category::ExpectedError),
        // F0075 PIN MOVED at the M8 mixed Line+Arc coplanar overlay
        // (2026-07-09, spec `m8_mixed_loop_coplanar_overlay`): the
        // arc-bearing auto-union re-enters Stage 1 through the mixed-loop
        // overlay and the case completes CORRECT (the committed release
        // baseline agrees). The stale UNSUPPORTED(revolve) pin sat unseen
        // behind the debug tier's fail-fast (the C0036 gate red, task #128)
        // until 2026-07-11.
        ("F0075", Category::SupportedCorrect),
        // R0008 marched past the partial-cone revolve wall (KV6c increment 5,
        // spec kv6c_partial_revolve_cone_patch.md) to its next honest
        // boundary: an auto-union whose Stage-3 SSI refinement stops loud at
        // AmbiguousCurve { candidates: 2, matched: 2 } (a cone-pair curve
        // matching ambiguity — M5-family). ERROR is the typed downstream
        // state, not a regression; re-pin when the SSI class lands.
        ("R0008", Category::Error),
        // ── C-series complexity corpus (2026-07-05 baseline) ─────────────
        // Representative pins per family; see specs/assay_complexity_corpus.md.
        // Group 1/3 (in-boundary bug hunters) — green, with two NAMED
        // findings pinned RED honestly:
        ("C0001", Category::SupportedCorrect), // 1a genus-2 plate
        ("C0021", Category::SupportedCorrect), // 1c star + through-cut
        // C0035-F1 RECLASSIFIED (2026-07-06): an authoring error, not a
        // kernel defect. The original cut depth 3.0−1e-4 from the z=2 sketch
        // reached z=−0.9999 — a geometric through-cut (the meta was
        // self-contradictory: expected_volume 0.36 encoded the through-cut
        // while euler_target 2 encoded the floor). The kernel was correct on
        // BOTH geometries: through-hole for the authored coords, and the
        // 100 µm floor preserved (chi 2, volume 0.360064 exact) for the
        // intended depth 2.0−1e-4, which the case now carries. A14.2 holds.
        ("C0035", Category::SupportedCorrect),
        ("C0038", Category::SupportedCorrect), // 1d 10 µm hole in 1 m cube
        // C0079-F1 FIXED (2026-07-06): the Add fold in dispatch_combine took
        // `.outputs.first()` of a disjoint target union (which kernel-v2
        // legitimately splits into two lumps), silently dropping body B. The
        // fold is now a connected-component sweep (spec §4.2 set-union
        // semantics); see tests/combine_add_disjoint_targets.rs.
        ("C0079", Category::SupportedCorrect),
        ("C0083", Category::SupportedCorrect), // 3a NewBody overlap, 2 bodies
        ("C0091", Category::SupportedCorrect), // 3c one-op holed profile
        ("C0100", Category::SupportedCorrect), // 3d plural-regions extrude
        // Group 2 trackers — the 2026-07-05 boundary. Several designed
        // trackers turned out SUPPORTED (capability better than documented);
        // they are pinned green so regressions are loud:
        ("C0041", Category::SupportedCorrect), // same-section crossing tunnels (M8 class!)
        ("C0042", Category::SupportedCorrect), // external rim tangency
        ("C0047", Category::SupportedCorrect), // holed-disc partner (task #54 class)
        ("C0057", Category::SupportedCorrect), // near-tangent 1e-6 lens union
        ("C0066", Category::SupportedCorrect), // partial torus + bore
        ("C0077", Category::SupportedCorrect), // 40-tooth gear CDT
        // KV6 on-axis slice 2 increment A (task #66,
        // specs/kv6_on_axis_revolve_oblique.md): on-axis oblique-quad
        // revolve builds SOLID FRUSTA; the three coaxial interpenetrating
        // unions (cone×cone coaxial-circle SSI) pass the exact-volume
        // oracle end-to-end.
        ("C0064", Category::SupportedCorrect), // [KV6c] stacked solid frusta chain
        // Still-walled trackers (flip these when the milestone lands):
        // C0048 PIN MOVED (2026-07-17, task #176 session): the M8 campaign
        // (#142/#143) had already lifted the UNSUPPORTED(coplanar) wall —
        // the case now progresses to the deeper typed azimuth-merge ERROR
        // ("rims have mismatched / too-few samples", the committed baseline's
        // verdict at 6d6141ef too). The stale pin sat unseen behind the debug
        // tier's fail-fast, exactly like C0065's and C0071's below.
        ("C0048", Category::Error), // [M8] chained swiss-cheese plates → azimuth-merge ERROR
        // M5 LANDED (specs/m5_surface_pair_curve.md): the general
        // unequal-radius perpendicular cyl×cyl intersection is now carried by
        // the procedural surface-pair curve — union then cut passes the
        // exact-volume oracle end-to-end.
        ("C0052", Category::SupportedCorrect), // [M5] unequal-R perpendicular CUT
        ("C0058", Category::Error),            // [M5] equal-R 30° oblique union (tangency neck)
        // KV6 on-axis slice 2 increment B (task #66): the apex triangle now
        // BUILDS the solid cone; the case's real boundary is the OBLIQUE
        // slab cut (conic-bounded cone patch), which lands on the typed
        // curved re-entry wall instead of a revolve ERROR.
        (
            "C0063",
            Category::Unsupported(UnsupportedReason::CurvedProfile),
        ), // [KV6c] oblique cone cut
        // C0065 PIN MOVED at KV6d (2026-07-11, ada0dc42, task #136): the
        // full-turn circle-revolve wall was RETIRED (the closed torus now
        // BUILDS); the case's boundary moved downstream to the boolean's
        // typed Stage-4 relocation error (near-tangent shaft containment
        // guard — see the KV6d roadmap ledger entry and task #137). The
        // stale UNSUPPORTED(revolve) pin sat unseen behind the debug tier's
        // fail-fast, exactly like C0071's below.
        ("C0065", Category::Error), // [KV6d] torus boolean → Stage-4 typed ERROR
        // C0071 PIN MOVED at KV7-F2 (2026-07-10): the multi-shell operand
        // wall was REMOVED (lumps and voids re-enter booleans) and the case
        // completes CORRECT; the stale pin sat unseen behind the debug
        // tier's fail-fast (the C0036 gate red, task #128).
        ("C0071", Category::SupportedCorrect), // [KV7] void breach
        // Group 6: user-reported drivers. C0101 = flush bridge across two
        // tower tops (user `error_coplanar.waffle`, task #129) — the bridge
        // bottom face lands in TWO Stage-0 coplanar pairs; handled by the
        // plane-grouped n-ary overlay (spec `m8_plane_group_nary_overlay`).
        ("C0101", Category::SupportedCorrect), // [M8 n-ary] flush bridge frame
        // ── Group 7: junction-scenario coverage, task #176 (2026-07-17
        // baseline; spec `assay_junction_scenario_corpus.md`). Boundary
        // corrections at first run: the cyl GRAZING corner (C0103), sphere
        // point-graze (C0104), cyl×cyl×plane blind-bore corner (C0106),
        // line-tangent union (C0110) and both zero-thickness results
        // (C0114/C0115) all pass — capability better than suspected.
        ("C0102", Category::SupportedCorrect), // 7a transversal cyl notch corner
        ("C0103", Category::SupportedCorrect), // 7a GRAZING cyl corner (#137-cyl class)
        ("C0106", Category::SupportedCorrect), // 7a cap through bicylinder curve
        // FINDING C0105-F1 (2026-07-17): the frustum notch (cone∩plane∩plane
        // corners) silently emitted a non-watertight, SELF-INTERSECTING shell
        // (51 unpaired edges, 10 penetrations, χ=−1). CONVERTED to a loud
        // typed STOP by the #173/N6 render-level gate the same day
        // (`SelfIntersectingBooleanOutput`, kernel-v2 `validate::selfx`) —
        // the boolean_subtract now rejects at the boundary. The #177
        // residual is the WATERTIGHT half (how 51 unpaired edges evaded a
        // gate); the case itself is loud now.
        ("C0105", Category::Error),
        ("C0107", Category::Error), // 7b point-tangent sphere⊕cyl: loud non-2-manifold
        ("C0110", Category::SupportedCorrect), // 7b line-tangent box⊕cyl union
        // FINDING C0111/C0113-F1 (2026-07-17): sliver walls at 1e-8 m (below
        // the 1e-6 m feature floor) and at exactly TAU_MODEL = 1e-7 m were
        // SILENTLY dissolved (χ 0→2, wall gone) — while C0031 (2e-6 m) and
        // C0112 (1e-2 m @ km scale) survive correctly. CONVERTED to a loud
        // typed STOP the same day by #178/N57 (spec
        // `yang_178_subres_coplanar_gap_stop.md`): Stage-0's near-coplanar
        // scan rejects a cross pair of DISTINCT parallel planes (offset gap
        // above rounding noise `TAU_WORK·(1+scale)`) with
        // `SubResolutionCoplanarGap` before any overlay work.
        ("C0111", Category::Error),
        ("C0112", Category::SupportedCorrect), // 7d km-scale sliver: scale-relative bands hold
        ("C0113", Category::Error),
        ("C0114", Category::SupportedCorrect), // 7e coincident pocket walls merge exactly
        ("C0115", Category::SupportedCorrect), // 7e coplanar-floor membrane opens exactly
        // FINDING C0116-F1 (2026-07-17): the 0.01-deep perpendicular cyl×cyl
        // graze passed watertight/χ/volume while the shell SELF-INTERSECTED
        // (10 penetrations) with no kernel STOP — the N6 red-phase fixture.
        // CONVERTED the same day by the #173/N6 detector: the render-level
        // gate (`SelfIntersectingBooleanOutput`, kernel-v2 `validate::selfx`)
        // rejects the auto-union loudly. (Measurement note, spec §6: the
        // defect is SUB-SAGITTA in the coarse Stage-4 mesh — the exact
        // mesh-level detector provably cannot see it; render resolution is
        // the observable layer.)
        ("C0116", Category::Error),
        ("C0117", Category::SupportedCorrect), // 7f 1e-4 curved tube wall survives
    ];
    for (id, expect) in expected {
        let case = cases
            .iter()
            .find(|c| c.id == *id)
            .unwrap_or_else(|| panic!("case {id} not in corpus"));
        let outcome = replay_case_with_timeout(case, Duration::from_secs(120));
        assert_eq!(
            &outcome.category,
            expect,
            "{id}: expected {}, got {} — {}",
            expect.label(),
            outcome.category.label(),
            outcome.detail
        );
    }
}

// ── Single-case run (manual / diagnosis) ───────────────────────────────────

/// Replay ONE corpus case by id (env `ASSAY_CASE`), generous budget (300s
/// default, `ASSAY_CASE_TIMEOUT_SECS` override), printing the outcome + wall
/// time. Two consumers:
/// - manual diagnosis for timing/timeout investigations (isolated runner —
///   no sibling contention);
/// - the `ASSAY_JOBS` parallel driver in `full_corpus_categorized`, which
///   re-invokes this test as a KILLABLE subprocess per case and parses the
///   `ASSAY_OUTCOME {json}` line below.
///
/// Without `ASSAY_CASE` set this SKIPS (prints a note and returns) rather
/// than panicking — a bare `--ignored` sweep otherwise reports exit 101 even
/// when the full corpus run passed.
#[test]
#[ignore] // manual: ASSAY_CASE=R0001 cargo test ... single_case -- --ignored --nocapture
fn single_case() {
    let Ok(id) = std::env::var("ASSAY_CASE") else {
        println!("single_case: ASSAY_CASE not set — nothing to do (manual/driver runner)");
        return;
    };
    let timeout_secs: u64 = std::env::var("ASSAY_CASE_TIMEOUT_SECS")
        .ok()
        .and_then(|s| s.parse().ok())
        .unwrap_or(300);
    let dir = assay_dir();
    let cases = discover_cases(&dir);
    let case = cases
        .iter()
        .find(|c| c.id == id)
        .unwrap_or_else(|| panic!("case {id} not in corpus"));
    let t0 = std::time::Instant::now();
    let outcome = replay_case_with_timeout(case, Duration::from_secs(timeout_secs));
    println!(
        "{id}: {} ({:.1}s) — {}",
        outcome.category.label(),
        t0.elapsed().as_secs_f64(),
        outcome.detail
    );
    // Machine-readable line for the ASSAY_JOBS driver (must stay one line;
    // serde_json escapes any newlines inside `detail`).
    println!(
        "ASSAY_OUTCOME {}",
        serde_json::json!({
            "id": outcome.id,
            "category": outcome.category.label(),
            "detail": outcome.detail,
        })
    );
}

// ── Full corpus run (manual / driver) ──────────────────────────────────────

#[test]
#[ignore] // full 193-case corpus; run with --ignored --nocapture
fn full_corpus_categorized() {
    let dir = assay_dir();
    let cases = discover_cases(&dir);
    assert_eq!(
        cases.len(),
        311,
        "expected the 311-case assay corpus (194 legacy + 117 C-series)"
    );

    // Per-case timeout (default 30s, env-overridable) so no single case can
    // wedge the run. ASSAY_FAST=1 skips cases on the auto slow-list for a quick
    // baseline. Cases that complete but exceed SLOW_THRESHOLD also join the list.
    let timeout_secs: u64 = std::env::var("ASSAY_CASE_TIMEOUT_SECS")
        .ok()
        .and_then(|s| s.parse().ok())
        .unwrap_or(30);
    let timeout = Duration::from_secs(timeout_secs);
    let fast = std::env::var("ASSAY_FAST").is_ok();

    // Slow-list semantics: ONLY cases that time out. A case that completes (even
    // slowly) is judged and counted, so FAST must not skip it — skipping would
    // lose a verdict. FAST skips only the un-judgeable (timed-out) cases.
    //
    // Dispatch model (ASSAY_JOBS): cases are independent and the pipeline is
    // deliberately single-threaded PER CASE, so the corpus parallelizes
    // cleanly across cases. ASSAY_JOBS > 1 (default 4, capped at the box's
    // parallelism — a deliberately conservative default: measured at 22
    // jobs, sibling contention flips ~8 borderline-slow cases X→TIMEOUT;
    // 4 keeps near-cap verdicts stable while still cutting the wall clock
    // several-fold; raise explicitly for quick gates) runs each case as a
    // KILLABLE subprocess (`replay_case_subprocess`) budgeted on CPU TIME
    // (Linux) — kill-on-timeout leaves NO orphans, and CPU budgeting makes the
    // TIMEOUT verdict load-INSENSITIVE: a case starved by siblings accrues wall
    // time but not CPU time, so it is judged the same alone or under contention
    // (this is the fix for the debug/loaded-box "20 false timeouts" episode).
    // ASSAY_JOBS=1 keeps the historical in-process serial path (byte-identical
    // verdicts), which budgets on WALL and whose caveat stands: a timed-out
    // case's worker thread is abandoned, not killed (heavy exact arithmetic
    // can't be cancelled in-process), so it keeps burning a core and contending
    // with later cases — but with no siblings that is benign. Verdicts are
    // per-case deterministic either way.
    let jobs: usize = std::env::var("ASSAY_JOBS")
        .ok()
        .and_then(|s| s.parse().ok())
        .unwrap_or_else(|| {
            4.min(
                std::thread::available_parallelism()
                    .map(|n| n.get())
                    .unwrap_or(1),
            )
        })
        .max(1);
    let slow_path = slow_cases_path();
    let prior_slow = read_slow_cases(&slow_path);
    let mut slow_now: BTreeSet<String> = prior_slow.clone();

    eprintln!(
        "per-case timeout {}s, {}{}",
        timeout_secs,
        if jobs <= 1 {
            "serial (in-process)".to_string()
        } else {
            format!("{jobs} parallel subprocess jobs")
        },
        if fast {
            format!(", FAST (skipping {} slow-listed)", prior_slow.len())
        } else {
            String::new()
        }
    );

    let run_started = std::time::Instant::now();
    let mut outcomes: Vec<CaseOutcome>;
    if jobs <= 1 {
        outcomes = Vec::with_capacity(cases.len());
        for (i, case) in cases.iter().enumerate() {
            eprint!("  [{}/{}] {} ... ", i + 1, cases.len(), case.id);
            if fast && prior_slow.contains(&case.id) {
                eprintln!("SKIPPED_SLOW");
                outcomes.push(CaseOutcome {
                    id: case.id.clone(),
                    category: Category::SkippedSlow,
                    detail: "on slow-list; skipped (ASSAY_FAST)".to_string(),
                });
                continue;
            }
            let case_start = std::time::Instant::now();
            let o = replay_case_with_timeout(case, timeout);
            let elapsed = case_start.elapsed();
            eprintln!("{} ({:.1}s)", o.category.label(), elapsed.as_secs_f64());
            if o.category == Category::Timeout {
                slow_now.insert(case.id.clone());
            }
            outcomes.push(o);
        }
    } else {
        use std::sync::atomic::{AtomicUsize, Ordering};
        use std::sync::Mutex;
        let total = cases.len();
        let next = AtomicUsize::new(0);
        let done = AtomicUsize::new(0);
        let collected: Mutex<Vec<(usize, CaseOutcome)>> = Mutex::new(Vec::with_capacity(total));
        std::thread::scope(|s| {
            for _ in 0..jobs.min(total) {
                s.spawn(|| loop {
                    let i = next.fetch_add(1, Ordering::Relaxed);
                    if i >= total {
                        break;
                    }
                    let case = &cases[i];
                    let o = if fast && prior_slow.contains(&case.id) {
                        CaseOutcome {
                            id: case.id.clone(),
                            category: Category::SkippedSlow,
                            detail: "on slow-list; skipped (ASSAY_FAST)".to_string(),
                        }
                    } else {
                        let case_start = std::time::Instant::now();
                        let o = replay_case_subprocess(&case.id, timeout);
                        let k = done.fetch_add(1, Ordering::Relaxed) + 1;
                        eprintln!(
                            "  [{k}/{total} done] {} ... {} ({:.1}s)",
                            case.id,
                            o.category.label(),
                            case_start.elapsed().as_secs_f64()
                        );
                        o
                    };
                    collected.lock().unwrap().push((i, o));
                });
            }
        });
        let mut v = collected.into_inner().unwrap();
        // Completion order is scheduling-dependent; corpus order is not —
        // restore it so the summary/report/results.json stay deterministic.
        v.sort_by_key(|&(i, _)| i);
        outcomes = v.into_iter().map(|(_, o)| o).collect();
        for o in &outcomes {
            if o.category == Category::Timeout {
                slow_now.insert(o.id.clone());
            }
        }
    }
    assert_eq!(outcomes.len(), cases.len(), "driver lost a case outcome");
    let wall_secs = run_started.elapsed().as_secs_f64();

    // Run provenance — without this a bare "20 timeouts" number is
    // uninterpretable after the fact (was it debug? a loaded box? a tight
    // budget?). The serial path (jobs<=1, in-process) budgets on WALL; the
    // parallel path budgets on CPU on Linux (load-insensitive) and degrades to
    // wall elsewhere. Goes into the gitignored target report only — the
    // committed UI results.json stays verdict-only to remain byte-stable.
    let build_mode = if cfg!(debug_assertions) {
        "debug"
    } else {
        "release"
    };
    let budget_kind = if jobs > 1 && cfg!(target_os = "linux") {
        "cpu"
    } else {
        "wall"
    };
    let host_parallelism = std::thread::available_parallelism()
        .map(|n| n.get())
        .unwrap_or(0);
    let generated_at_unix = std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .map(|d| d.as_secs())
        .unwrap_or(0);
    eprintln!(
        "run: {build_mode} build, {jobs} job(s), {budget_kind}-budget {timeout_secs}s/case, \
         {host_parallelism}-way host, wall {wall_secs:.1}s"
    );

    // Persist the slow-list (union with prior — a FAST run leaves skipped
    // entries in place) so subsequent ASSAY_FAST runs stay quick.
    write_slow_cases(&slow_path, &slow_now);
    if !fast {
        eprintln!(
            "slow-list: {} timed-out case(s) → {} (re-run with ASSAY_FAST=1 to skip)",
            slow_now.len(),
            slow_path.display()
        );
    }

    // ---- summary table ----------------------------------------------------
    let count = |pred: &dyn Fn(&Category) -> bool| -> usize {
        outcomes.iter().filter(|o| pred(&o.category)).count()
    };
    let mut table = String::new();
    writeln!(table, "\nASSAY KV2 — kernel-v2 categorized corpus score").unwrap();
    writeln!(table, "  total                {:>4}", outcomes.len()).unwrap();
    writeln!(
        table,
        "  SUPPORTED_CORRECT    {:>4}",
        count(&|c| *c == Category::SupportedCorrect)
    )
    .unwrap();
    writeln!(
        table,
        "  SUPPORTED_WRONG      {:>4}",
        count(&|c| *c == Category::SupportedWrong)
    )
    .unwrap();
    for reason in [
        UnsupportedReason::Revolve,
        UnsupportedReason::CurvedProfile,
        UnsupportedReason::CoplanarBoolean,
        UnsupportedReason::FilletChamferShell,
        UnsupportedReason::MultiShell,
        UnsupportedReason::Other,
    ] {
        writeln!(
            table,
            "  UNSUPPORTED({:<20}) {:>4}",
            reason.label(),
            count(&|c| *c == Category::Unsupported(reason))
        )
        .unwrap();
    }
    writeln!(
        table,
        "  EXPECTED_ERROR       {:>4}",
        count(&|c| *c == Category::ExpectedError)
    )
    .unwrap();
    writeln!(
        table,
        "  ERROR                {:>4}",
        count(&|c| *c == Category::Error)
    )
    .unwrap();
    writeln!(
        table,
        "  TIMEOUT              {:>4}",
        count(&|c| *c == Category::Timeout)
    )
    .unwrap();
    writeln!(
        table,
        "  SKIPPED_SLOW         {:>4}",
        count(&|c| *c == Category::SkippedSlow)
    )
    .unwrap();

    for (label, cat) in [
        ("SUPPORTED_WRONG", Category::SupportedWrong),
        ("ERROR", Category::Error),
        ("TIMEOUT", Category::Timeout),
    ] {
        let ids: Vec<&str> = outcomes
            .iter()
            .filter(|o| o.category == cat)
            .map(|o| o.id.as_str())
            .collect();
        if !ids.is_empty() {
            writeln!(table, "\n{label} cases ({}):", ids.len()).unwrap();
            for o in outcomes.iter().filter(|o| o.category == cat) {
                writeln!(table, "  {} — {}", o.id, o.detail).unwrap();
            }
        }
    }
    eprintln!("{table}");

    // ---- JSON report --------------------------------------------------------
    let report = serde_json::json!({
        "corpus": "app/tests/cases/assay",
        "kernel": "kernel-v2 (KernelV2Adapter)",
        // Provenance for the timeout/perf verdicts — see the block above.
        "run": {
            "build_mode": build_mode,
            "jobs": jobs,
            "budget_kind": budget_kind,
            "per_case_budget_secs": timeout_secs,
            "wall_secs": (wall_secs * 10.0).round() / 10.0,
            "host_parallelism": host_parallelism,
            "fast": fast,
            "generated_at_unix": generated_at_unix,
        },
        "total": outcomes.len(),
        "supported_correct": count(&|c| *c == Category::SupportedCorrect),
        "supported_wrong": count(&|c| *c == Category::SupportedWrong),
        "unsupported": {
            "revolve": count(&|c| *c == Category::Unsupported(UnsupportedReason::Revolve)),
            "curved_profile": count(&|c| *c == Category::Unsupported(UnsupportedReason::CurvedProfile)),
            "coplanar_boolean": count(&|c| *c == Category::Unsupported(UnsupportedReason::CoplanarBoolean)),
            "fillet_chamfer_shell": count(&|c| *c == Category::Unsupported(UnsupportedReason::FilletChamferShell)),
            "other": count(&|c| *c == Category::Unsupported(UnsupportedReason::Other)),
        },
        "expected_error": count(&|c| *c == Category::ExpectedError),
        "error": count(&|c| *c == Category::Error),
        "timeout": count(&|c| *c == Category::Timeout),
        "skipped_slow": count(&|c| *c == Category::SkippedSlow),
        "cases": outcomes.iter().map(|o| serde_json::json!({
            "id": o.id,
            "category": o.category.label(),
            "detail": o.detail,
        })).collect::<Vec<_>>(),
    });
    let report_path = PathBuf::from(env!("CARGO_MANIFEST_DIR"))
        .parent()
        .unwrap()
        .parent()
        .unwrap()
        .join("target/assay_kv2_report.json");
    fs::write(&report_path, serde_json::to_string_pretty(&report).unwrap())
        .unwrap_or_else(|e| panic!("cannot write {report_path:?}: {e}"));
    eprintln!("report written to {report_path:?}");

    // Also emit the UI-schema results.json consumed by the app's
    // AssayBrowser (app/src/lib/engine/assayCaseApi.js → /assay/results.json
    // or the vite dev plugin), so the in-app assay browser reflects the NEW
    // kernel's categorized score rather than stale legacy-WaffleKernel runs.
    // Status mapping: SUPPORTED_CORRECT→pass, SUPPORTED_WRONG→fail,
    // ERROR→error, UNSUPPORTED(*)→"unsupported" (sorts after error in the
    // browser; the reason rides in `category`).
    let ui_status = |c: &Category| -> &'static str {
        match c {
            Category::SupportedCorrect => "pass",
            Category::SupportedWrong => "fail",
            // An EXPECTED error is still an error in the browser — PASS is
            // reserved for fully-supported working geometry; the canary
            // context rides in `category` + `detail`.
            Category::ExpectedError | Category::Error => "error",
            // Too-slow-to-judge here surfaces as an error in the browser (the
            // precise TIMEOUT rides in `category`); skipped-slow sorts into the
            // benign unsupported bucket.
            Category::Timeout => "error",
            Category::SkippedSlow => "unsupported",
            Category::Unsupported(_) => "unsupported",
        }
    };
    let ui_results = serde_json::json!({
        "generated": format!("kernel-v2 (assay_kv2 categorized run)"),
        "total": outcomes.len(),
        "passed": count(&|c| *c == Category::SupportedCorrect),
        "failed": count(&|c| *c == Category::SupportedWrong),
        "errored": count(&|c| *c == Category::Error),
        "results": outcomes.iter().map(|o| serde_json::json!({
            "id": o.id,
            "status": ui_status(&o.category),
            "category": o.category.label(),
            "detail": o.detail,
        })).collect::<Vec<_>>(),
    });
    // Only a FULL run (not FAST) writes the committed UI results.json — a FAST
    // run intentionally skips the slow-list, so its score is partial and must
    // not overwrite the canonical baseline the assay browser reads.
    if fast {
        eprintln!(
            "FAST run — UI results.json NOT overwritten (partial: {} skipped)",
            count(&|c| *c == Category::SkippedSlow)
        );
    } else {
        let ui_path = PathBuf::from(env!("CARGO_MANIFEST_DIR"))
            .parent()
            .unwrap()
            .parent()
            .unwrap()
            .join("app/tests/cases/assay/results.json");
        fs::write(&ui_path, serde_json::to_string_pretty(&ui_results).unwrap())
            .unwrap_or_else(|e| panic!("cannot write {ui_path:?}: {e}"));
        eprintln!("UI results.json written to {ui_path:?} (new-kernel categorized score)");
    }
}
