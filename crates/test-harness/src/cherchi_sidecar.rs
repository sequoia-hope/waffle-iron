//! Shared helpers for invoking the Cherchi 2022 `mesh_booleans` /
//! `mesh_booleans_inputcheck` sidecar binaries from test-harness tests.
//!
//! Originally introduced inline in
//! `tests/cherchi2022_reference_parity.rs` (PR-S1, commit `17792eb`); extracted
//! here for PR-S2's `cherchi_inputcheck_corpus_sweep.rs` so both tests share
//! one implementation of binary discovery and timed subprocess execution.
//! Parameterizing the timeout (vs. a const) is the only behavioral change vs.
//! PR-S1 — the reference-parity test still uses 30 s, the sweep uses 10 s.
//!
//! Refs: PR-S1 deliverables in `/home/claude/.claude/plans/reactive-juggling-sloth.md`,
//! PR-S2 spec at `specs/cherchi_inputcheck_corpus_sweep.md`.

use std::path::PathBuf;
use std::process::{Command, Stdio};
use std::time::Duration;

/// Default location of the upstream `mesh_booleans` binary. The same
/// directory contains `mesh_booleans_inputcheck`.
pub const CHERCHI2022_BIN_DEFAULT: &str =
    "/home/claude/cherchi2022/InteractiveAndRobustMeshBooleans/build/mesh_booleans";

/// Outcome of a timed subprocess invocation: completed (with the
/// original `Output`), or `TimedOut` after the kill, or a spawn-time error.
pub enum TimedRun {
    Completed(std::process::Output),
    TimedOut,
    SpawnFailed(String),
}

/// Resolve the sidecar binary path. Returns `None` (with an `eprintln`
/// explanation) if neither `CHERCHI2022_BIN` env nor the default exists —
/// callers should `return` cleanly so the test is treated as
/// configuration-skipped rather than a failure.
pub fn cherchi_bin() -> Option<PathBuf> {
    let path =
        std::env::var("CHERCHI2022_BIN").unwrap_or_else(|_| CHERCHI2022_BIN_DEFAULT.to_string());
    let p = PathBuf::from(&path);
    if !p.exists() {
        eprintln!(
            "[cherchi-sidecar] SKIP: Cherchi 2022 binary not found at `{}`. \
             Build it per upstream README and either symlink to the default \
             location or set CHERCHI2022_BIN.",
            path
        );
        return None;
    }
    Some(p)
}

/// Spawn a `Command` and either wait for it to finish within `timeout` or
/// kill it. Pipes stdout+stderr so the child doesn't block on a full pipe
/// buffer; collects them into the returned `Output` on completion.
///
/// Polls in 1-second intervals (or `timeout` itself if smaller than 1 s)
/// using `child.try_wait()`. No external crate (no `wait_timeout` dep).
/// The 30 s reference-parity test uses 5 s polls historically; 1 s here
/// gives finer granularity for the 10 s inputcheck timeout while still
/// being cheap.
pub fn run_with_timeout(mut cmd: Command, timeout: Duration) -> TimedRun {
    cmd.stdout(Stdio::piped()).stderr(Stdio::piped());
    let mut child = match cmd.spawn() {
        Ok(c) => c,
        Err(e) => return TimedRun::SpawnFailed(e.to_string()),
    };
    let poll_interval = std::cmp::min(Duration::from_secs(1), timeout);
    // ceil-div so a 10 s timeout with 1 s polls gives 10 polls (not 9).
    let polls = ((timeout.as_millis() + poll_interval.as_millis() - 1)
        / poll_interval.as_millis().max(1)) as usize;
    for _ in 0..polls {
        match child.try_wait() {
            Ok(Some(_)) => break,
            Ok(None) => std::thread::sleep(poll_interval),
            Err(e) => return TimedRun::SpawnFailed(format!("try_wait failed: {}", e)),
        }
    }
    match child.try_wait() {
        Ok(Some(_)) => match child.wait_with_output() {
            Ok(out) => TimedRun::Completed(out),
            Err(e) => TimedRun::SpawnFailed(format!("wait_with_output failed: {}", e)),
        },
        _ => {
            let _ = child.kill();
            let _ = child.wait();
            TimedRun::TimedOut
        }
    }
}
