//! Cherchi 2022 `mesh_booleans` sidecar harness — binary discovery + timed
//! subprocess invocation.
//!
//! Re-implemented (not copied) per `crates/cherchi-rs/CLAUDE.md` Hard Rule
//! #1 ("Zero workspace deps except cad-primitives"). Design mirrors the
//! legacy helper at `crates/test-harness/src/cherchi_sidecar.rs` —
//! same env-var contract, same poll strategy, no external crate dep.
//!
//! Build recipe for the binary: `docs/sidecar/cherchi2022_build_guide.md`.

use std::path::PathBuf;
use std::process::{Command, Output, Stdio};
use std::time::Duration;

/// The `mesh_booleans` binary, relative to the sidecar root. The same directory
/// contains `mesh_booleans_inputcheck` (not currently wired into this
/// harness — banked for future PR).
pub const CHERCHI2022_BIN_DEFAULT: &str = "InteractiveAndRobustMeshBooleans/build/mesh_booleans";

/// [`CHERCHI2022_BIN_DEFAULT`] under the root `scripts/build_sidecars.sh`
/// builds into: `$CHERCHI2022_ROOT`, else `$HOME/cherchi2022`.
pub fn cherchi_bin_default() -> PathBuf {
    std::env::var_os("CHERCHI2022_ROOT")
        .map(PathBuf::from)
        .or_else(|| std::env::var_os("HOME").map(|h| PathBuf::from(h).join("cherchi2022")))
        .unwrap_or_default()
        .join(CHERCHI2022_BIN_DEFAULT)
}

/// Outcome of a timed subprocess invocation.
pub enum TimedRun {
    Completed(Output),
    TimedOut,
    SpawnFailed(String),
}

/// Resolve the sidecar binary path via the `CHERCHI2022_BIN` env var or
/// the default location. Returns `None` (with an `eprintln` SKIP
/// message) if neither path resolves to an existing file — callers
/// should `return` cleanly so the test counts as a configuration-skip
/// rather than a failure.
pub fn cherchi_bin() -> Option<PathBuf> {
    let p = std::env::var_os("CHERCHI2022_BIN")
        .map(PathBuf::from)
        .unwrap_or_else(cherchi_bin_default);
    let path = p.display();
    if !p.exists() {
        eprintln!(
            "[cherchi-sidecar] SKIP: Cherchi 2022 binary not found at `{}`. \
             Build per docs/sidecar/cherchi2022_build_guide.md or set CHERCHI2022_BIN.",
            path
        );
        return None;
    }
    Some(p)
}

/// Spawn `cmd` and wait up to `timeout`. Kills the child on timeout
/// expiry to avoid the infinite-loop hazard documented in the build
/// guide (malformed input → 6-hour 99% CPU runaway observed on F0002).
/// Polls in 1-second intervals (or `timeout` itself if smaller) via
/// `try_wait()` — no `wait_timeout` crate dependency.
pub fn run_with_timeout(mut cmd: Command, timeout: Duration) -> TimedRun {
    cmd.stdout(Stdio::piped()).stderr(Stdio::piped());
    let mut child = match cmd.spawn() {
        Ok(c) => c,
        Err(e) => return TimedRun::SpawnFailed(e.to_string()),
    };
    let poll = std::cmp::min(Duration::from_secs(1), timeout);
    // ceil-div so a 10 s timeout with 1 s polls gives 10 polls (not 9).
    let polls = ((timeout.as_millis() + poll.as_millis() - 1) / poll.as_millis().max(1)) as usize;
    for _ in 0..polls {
        match child.try_wait() {
            Ok(Some(_)) => break,
            Ok(None) => std::thread::sleep(poll),
            Err(e) => return TimedRun::SpawnFailed(format!("try_wait: {}", e)),
        }
    }
    match child.try_wait() {
        Ok(Some(_)) => match child.wait_with_output() {
            Ok(out) => TimedRun::Completed(out),
            Err(e) => TimedRun::SpawnFailed(format!("wait_with_output: {}", e)),
        },
        _ => {
            let _ = child.kill();
            let _ = child.wait();
            TimedRun::TimedOut
        }
    }
}
