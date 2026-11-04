//! CPU-budgeted case runner shared by `tests/assay_kv2.rs` and
//! `tests/assay_pair_matrix.rs`.
//!
//! Runs individual categorize cases either in-process with a per-thread CPU
//! watcher (`ASSAY_JOBS=1` / `single_case`) or in a parallel pool of killable
//! child processes (`ASSAY_JOBS > 1`, re-invoking the current test binary's
//! `single_case` entry point with `ASSAY_CASE=<id>`). On Linux, timeouts are
//! billed against `/proc` CPU time rather than wall clock so sibling contention
//! cannot manufacture false `TIMEOUT` verdicts; a generous wall-clock safety
//! cap still terminates genuinely blocked children.

use std::io::Read as _;
use std::process::{Command, Stdio};
use std::sync::atomic::{AtomicU64, AtomicUsize, Ordering};
use std::sync::{Arc, Mutex};
use std::time::{Duration, Instant, SystemTime, UNIX_EPOCH};

use crate::assay::categorize::{CaseOutcome, Category};

/// Prefix emitted on stdout by a `single_case` child so [`run_case_subprocess`]
/// can parse its structured [`CaseOutcome`].
pub const ASSAY_OUTCOME_PREFIX: &str = "ASSAY_OUTCOME ";

/// A [`CaseOutcome`] paired with an optional structured `"extra"` JSON payload
/// transported across [`emit_assay_outcome_with_extra`] / [`run_batch_with_extra`].
pub type OutcomeWithExtra = (CaseOutcome, Option<serde_json::Value>);

/// Run provenance recorded into JSON reports so timeout counts are
/// interpretable after the fact (build mode, job count, CPU vs wall budget).
#[derive(Debug, Clone)]
pub struct RunProvenance {
    pub build_mode: &'static str,
    pub jobs: usize,
    pub budget_kind: &'static str,
    pub per_case_budget_secs: u64,
    pub wall_secs: f64,
    pub host_parallelism: usize,
    pub generated_at_unix: u64,
}

impl RunProvenance {
    /// Capture provenance for a completed batch run.
    pub fn capture(jobs: usize, per_case_budget_secs: u64, wall_secs: f64) -> Self {
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
        let generated_at_unix = SystemTime::now()
            .duration_since(UNIX_EPOCH)
            .map(|d| d.as_secs())
            .unwrap_or(0);
        Self {
            build_mode,
            jobs,
            budget_kind,
            per_case_budget_secs,
            wall_secs,
            host_parallelism,
            generated_at_unix,
        }
    }

    /// One-line summary printed to stderr by batch runners.
    pub fn summary_line(&self) -> String {
        format!(
            "run: {} build, {} job(s), {}-budget {}s/case, {}-way host, wall {:.1}s",
            self.build_mode,
            self.jobs,
            self.budget_kind,
            self.per_case_budget_secs,
            self.host_parallelism,
            self.wall_secs
        )
    }

    /// JSON block matching `assay_kv2_report.json`'s `"run"` schema.
    pub fn to_json(&self, fast: Option<bool>) -> serde_json::Value {
        let mut obj = serde_json::json!({
            "build_mode": self.build_mode,
            "jobs": self.jobs,
            "budget_kind": self.budget_kind,
            "per_case_budget_secs": self.per_case_budget_secs,
            "wall_secs": (self.wall_secs * 10.0).round() / 10.0,
            "host_parallelism": self.host_parallelism,
            "generated_at_unix": self.generated_at_unix,
        });
        if let Some(f) = fast {
            obj["fast"] = serde_json::Value::Bool(f);
        }
        obj
    }
}

/// Default worker count from `ASSAY_JOBS`: conservative default of 4 (capped at
/// host parallelism, minimum 1).
pub fn default_assay_jobs() -> usize {
    std::env::var("ASSAY_JOBS")
        .ok()
        .and_then(|s| s.parse().ok())
        .unwrap_or_else(|| {
            4.min(
                std::thread::available_parallelism()
                    .map(|n| n.get())
                    .unwrap_or(1),
            )
        })
        .max(1)
}

/// Print the machine-readable `ASSAY_OUTCOME {json}` line expected by
/// [`run_case_subprocess`].
pub fn emit_assay_outcome(outcome: &CaseOutcome) {
    emit_assay_outcome_with_extra(outcome, None);
}

/// Print the machine-readable `ASSAY_OUTCOME {json}` line with an optional
/// structured `"extra"` JSON field carried alongside [`CaseOutcome`].
pub fn emit_assay_outcome_with_extra(outcome: &CaseOutcome, extra: Option<&serde_json::Value>) {
    let mut obj = serde_json::json!({
        "id": outcome.id,
        "category": outcome.category.label(),
        "detail": outcome.detail,
    });
    if let Some(extra) = extra {
        obj["extra"] = extra.clone();
    }
    println!("{ASSAY_OUTCOME_PREFIX}{obj}");
}

/// Run `work` on a dedicated worker thread with a CPU-time hang guard on Linux
/// (falling back to wall clock elsewhere), plus a generous wall-clock safety cap
/// for blocked threads that accrue no CPU.
pub fn run_with_cpu_timeout<F>(id: &str, budget: Duration, work: F) -> CaseOutcome
where
    F: FnOnce() -> CaseOutcome + Send + 'static,
{
    run_with_cpu_timeout_map(id, budget, work, |o| o)
}

/// Generic companion to [`run_with_cpu_timeout`] for runners that pair
/// [`CaseOutcome`] with extra structured per-case telemetry.
pub fn run_with_cpu_timeout_map<T, F, M>(id: &str, budget: Duration, work: F, on_timeout: M) -> T
where
    T: Send + 'static,
    F: FnOnce() -> T + Send + 'static,
    M: FnOnce(CaseOutcome) -> T,
{
    let (tx, rx) = std::sync::mpsc::channel();
    let case_id = id.to_string();
    // Worker publishes its own kernel tid so the watcher can bill its CPU time.
    // 0 = not yet published; never treated as a timeout.
    let tid = Arc::new(AtomicU64::new(0));
    let worker_tid = Arc::clone(&tid);
    let handle = std::thread::spawn(move || {
        if let Some(t) = current_tid() {
            worker_tid.store(t, Ordering::Relaxed);
        }
        let _ = tx.send(work());
    });

    let cpu_budget = budget.as_secs_f64();
    let wall_cap = Duration::from_secs_f64((cpu_budget * 4.0).max(cpu_budget + 120.0));
    let started = Instant::now();
    let poll = Duration::from_millis(250);

    let timed_out = |detail: String| CaseOutcome {
        id: case_id.clone(),
        category: Category::Timeout,
        detail,
    };

    loop {
        match rx.recv_timeout(poll) {
            Ok(r) => {
                let _ = handle.join();
                return r;
            }
            Err(std::sync::mpsc::RecvTimeoutError::Disconnected) => {
                let _ = handle.join();
                return on_timeout(timed_out(
                    "worker thread ended without a result".to_string(),
                ));
            }
            Err(std::sync::mpsc::RecvTimeoutError::Timeout) => {
                let t = tid.load(Ordering::Relaxed);
                let cpu = if t == 0 { None } else { thread_cpu_secs(t) };
                match cpu {
                    Some(spent) if spent >= cpu_budget => {
                        return on_timeout(timed_out(format!("timeout after {spent:.0}s CPU")));
                    }
                    None if t != 0 && started.elapsed() >= budget => {
                        return on_timeout(timed_out(format!(
                            "timeout after {}s",
                            budget.as_secs()
                        )));
                    }
                    _ => {}
                }
                if started.elapsed() >= wall_cap {
                    return on_timeout(timed_out(format!(
                        "timeout after {}s wall (CPU budget {}s never reached — blocked, not slow)",
                        wall_cap.as_secs(),
                        cpu_budget as u64
                    )));
                }
            }
        }
    }
}

/// Replay one case in a killable subprocess — re-invoking `current_exe()` with
/// `--exact single_case --ignored --nocapture` and `ASSAY_CASE=<id>` — and parse
/// its [`ASSAY_OUTCOME_PREFIX`] line.
///
/// `budget` is a **CPU-time** budget on Linux (summed process CPU from
/// `/proc/<pid>/stat`) with a generous wall-clock safety cap; on non-Linux it
/// degrades to a plain wall deadline. On timeout the child is killed so
/// parallel runs leave no orphan CPU load.
pub fn run_case_subprocess(id: &str, budget: Duration) -> CaseOutcome {
    run_case_subprocess_with_extra(id, budget).0
}

/// Same as [`run_case_subprocess`], additionally returning the optional
/// `"extra"` JSON payload emitted by [`emit_assay_outcome_with_extra`].
pub fn run_case_subprocess_with_extra(id: &str, budget: Duration) -> OutcomeWithExtra {
    let err_outcome = |detail: String| {
        (
            CaseOutcome {
                id: id.to_string(),
                category: Category::Error,
                detail,
            },
            None,
        )
    };
    let cpu_budget = budget.as_secs_f64();
    let wall_cap = Duration::from_secs_f64((cpu_budget * 4.0).max(cpu_budget + 120.0));
    let exe = match std::env::current_exe() {
        Ok(p) => p,
        Err(e) => return err_outcome(format!("driver: cannot resolve current_exe: {e}")),
    };
    let mut child = match Command::new(&exe)
        .args(["--exact", "single_case", "--ignored", "--nocapture"])
        .env("ASSAY_CASE", id)
        // Keep the child's own in-process guard strictly behind the parent's
        // wall cap so the parent always decides and kills first.
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
    let Some(mut stdout) = child.stdout.take() else {
        let _ = child.kill();
        let _ = child.wait();
        return err_outcome("driver: missing piped child stdout".to_string());
    };
    let reader = std::thread::spawn(move || {
        let mut buf = String::new();
        let _ = stdout.read_to_string(&mut buf);
        buf
    });
    let pid = child.id();
    let cpu_probe = child_cpu_secs(pid).is_some();
    let wall_deadline = Instant::now() + if cpu_probe { wall_cap } else { budget };
    let status = loop {
        match child.try_wait() {
            Ok(Some(status)) => break Ok(status),
            Ok(None) => {
                let over_cpu = cpu_probe && child_cpu_secs(pid).is_some_and(|c| c >= cpu_budget);
                let over_wall = Instant::now() >= wall_deadline;
                if over_cpu || over_wall {
                    let _ = child.kill();
                    let _ = child.wait();
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
            return (
                CaseOutcome {
                    id: id.to_string(),
                    category: Category::Timeout,
                    detail: timeout_detail,
                },
                None,
            );
        }
    };
    if let Some(parsed) = parse_assay_outcome_line(&out, id) {
        return parsed;
    }
    err_outcome(format!(
        "driver: subprocess exited ({status}) without a parseable ASSAY_OUTCOME line"
    ))
}

/// Run a list of case IDs in deterministic input order: serial in-process when
/// `jobs <= 1`, or across `jobs` scoped worker threads calling
/// [`run_case_subprocess`] when `jobs > 1`.
pub fn run_batch<S>(
    case_ids: &[String],
    jobs: usize,
    budget: Duration,
    serial_runner: S,
) -> (Vec<CaseOutcome>, f64)
where
    S: Fn(&str, Duration) -> CaseOutcome,
{
    let (pairs, wall) =
        run_batch_with_extra(case_ids, jobs, budget, |id, b| (serial_runner(id, b), None));
    (pairs.into_iter().map(|(o, _)| o).collect(), wall)
}

/// Same as [`run_batch`], preserving optional `"extra"` JSON payloads from
/// `serial_runner` or [`run_case_subprocess_with_extra`].
pub fn run_batch_with_extra<S>(
    case_ids: &[String],
    jobs: usize,
    budget: Duration,
    serial_runner: S,
) -> (Vec<OutcomeWithExtra>, f64)
where
    S: Fn(&str, Duration) -> OutcomeWithExtra,
{
    let started = Instant::now();
    let total = case_ids.len();
    if total == 0 {
        return (Vec::new(), 0.0);
    }
    let outcomes = if jobs <= 1 {
        let mut out = Vec::with_capacity(total);
        for (i, id) in case_ids.iter().enumerate() {
            eprint!("  [{}/{}] {} ... ", i + 1, total, id);
            let t0 = Instant::now();
            let (o, extra) = serial_runner(id, budget);
            eprintln!(
                "{} ({:.1}s)",
                o.category.label(),
                t0.elapsed().as_secs_f64()
            );
            out.push((o, extra));
        }
        out
    } else {
        let next = AtomicUsize::new(0);
        let done = AtomicUsize::new(0);
        let collected: Mutex<Vec<(usize, OutcomeWithExtra)>> =
            Mutex::new(Vec::with_capacity(total));
        std::thread::scope(|s| {
            for _ in 0..jobs.min(total) {
                s.spawn(|| loop {
                    let i = next.fetch_add(1, Ordering::Relaxed);
                    if i >= total {
                        break;
                    }
                    let id = &case_ids[i];
                    let t0 = Instant::now();
                    let (o, extra) = run_case_subprocess_with_extra(id, budget);
                    let k = done.fetch_add(1, Ordering::Relaxed) + 1;
                    eprintln!(
                        "  [{k}/{total} done] {id} ... {} ({:.1}s)",
                        o.category.label(),
                        t0.elapsed().as_secs_f64()
                    );
                    if let Ok(mut guard) = collected.lock() {
                        guard.push((i, (o, extra)));
                    }
                });
            }
        });
        let mut v = collected.into_inner().unwrap_or_default();
        v.sort_by_key(|&(i, _)| i);
        v.into_iter().map(|(_, pair)| pair).collect()
    };
    (outcomes, started.elapsed().as_secs_f64())
}

fn parse_assay_outcome_line(stdout: &str, expected_id: &str) -> Option<OutcomeWithExtra> {
    let line = stdout
        .lines()
        .rev()
        .find(|l| l.starts_with(ASSAY_OUTCOME_PREFIX))?;
    let mut v =
        serde_json::from_str::<serde_json::Value>(&line[ASSAY_OUTCOME_PREFIX.len()..]).ok()?;
    let cid = v["id"].as_str()?.to_string();
    let category = v["category"].as_str().and_then(Category::from_label)?;
    let detail = v["detail"].as_str()?.to_string();
    if cid != expected_id {
        return None;
    }
    let extra = v
        .get_mut("extra")
        .map(serde_json::Value::take)
        .filter(|e| !e.is_null());
    Some((
        CaseOutcome {
            id: cid,
            category,
            detail,
        },
        extra,
    ))
}

/// Calling thread's kernel task id from `/proc/thread-self` (`<pid>/task/<tid>`).
#[cfg(target_os = "linux")]
fn current_tid() -> Option<u64> {
    std::fs::read_link("/proc/thread-self")
        .ok()?
        .file_name()?
        .to_str()?
        .parse()
        .ok()
}

#[cfg(not(target_os = "linux"))]
fn current_tid() -> Option<u64> {
    None
}

/// CPU seconds consumed so far by one thread (`/proc/self/task/<tid>/stat`).
#[cfg(target_os = "linux")]
fn thread_cpu_secs(tid: u64) -> Option<f64> {
    parse_stat_cpu_secs(&format!("/proc/self/task/{tid}/stat"))
}

#[cfg(not(target_os = "linux"))]
fn thread_cpu_secs(_tid: u64) -> Option<f64> {
    None
}

/// CPU seconds (user + system across all threads) consumed so far by `pid`
/// (`/proc/<pid>/stat` fields 14 `utime` + 15 `stime`).
#[cfg(target_os = "linux")]
fn child_cpu_secs(pid: u32) -> Option<f64> {
    parse_stat_cpu_secs(&format!("/proc/{pid}/stat"))
}

#[cfg(not(target_os = "linux"))]
fn child_cpu_secs(_pid: u32) -> Option<f64> {
    None
}

/// Shared parser for `/proc/<pid>/stat` and `/proc/self/task/<tid>/stat`.
/// Assumes `USER_HZ == 100` (`man 5 proc`, "utime").
#[cfg(target_os = "linux")]
fn parse_stat_cpu_secs(path: &str) -> Option<f64> {
    let stat = std::fs::read_to_string(path).ok()?;
    let rest = stat.rsplit_once(')')?.1;
    let f: Vec<&str> = rest.split_whitespace().collect();
    let utime: u64 = f.get(11)?.parse().ok()?;
    let stime: u64 = f.get(12)?.parse().ok()?;
    Some((utime + stime) as f64 / 100.0)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn outcome_line_round_trips() {
        let line = format!(
            "{ASSAY_OUTCOME_PREFIX}{}",
            serde_json::json!({
                "id": "pm_00_plane_plane_union_s0",
                "category": Category::SupportedCorrect.label(),
                "detail": "ok\nline2",
            })
        );
        let (parsed, extra) =
            parse_assay_outcome_line(&line, "pm_00_plane_plane_union_s0").unwrap();
        assert_eq!(parsed.id, "pm_00_plane_plane_union_s0");
        assert_eq!(parsed.category, Category::SupportedCorrect);
        assert_eq!(parsed.detail, "ok\nline2");
        assert!(extra.is_none());
        assert!(parse_assay_outcome_line(&line, "other_id").is_none());

        let with_extra = format!(
            "{ASSAY_OUTCOME_PREFIX}{}",
            serde_json::json!({
                "id": "pm_00_plane_plane_union_s0",
                "category": Category::SupportedCorrect.label(),
                "detail": "ok",
                "extra": { "verdict": "NotCovered", "reason": "thin" },
            })
        );
        let (_, extra2) =
            parse_assay_outcome_line(&with_extra, "pm_00_plane_plane_union_s0").unwrap();
        assert_eq!(extra2.unwrap()["verdict"], "NotCovered");
    }
}
