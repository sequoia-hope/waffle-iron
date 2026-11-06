//! Subprocess management: binary discovery + timed subprocess.
//!
//! Private to the crate; public API is in `lib.rs`.

use std::path::PathBuf;
use std::process::{Command, Output};
use std::time::Duration;

use crate::SidecarError;

/// The upstream `mesh_booleans` binary, relative to the sidecar root
/// ([`sidecar_root`]). Override the whole path via `CHERCHI2022_BIN`.
pub const DEFAULT_BIN_PATH: &str = "InteractiveAndRobustMeshBooleans/build/mesh_booleans";

const ENV_VAR: &str = "CHERCHI2022_BIN";

/// The upstream `mesh_booleans_inputcheck` binary (the Cherchi 2022 §3
/// input-axiom reference oracle), relative to the sidecar root
/// ([`sidecar_root`]). Override the whole path via
/// `CHERCHI2022_INPUTCHECK_BIN`.
pub const INPUTCHECK_DEFAULT_BIN_PATH: &str =
    "InteractiveAndRobustMeshBooleans/build/mesh_booleans_inputcheck";

const INPUTCHECK_ENV_VAR: &str = "CHERCHI2022_INPUTCHECK_BIN";

/// Where `scripts/build_sidecars.sh` builds the reference:
/// `$CHERCHI2022_ROOT`, else `$HOME/cherchi2022`. With neither set the
/// root is empty, so the default path is relative and simply not found.
pub fn sidecar_root() -> PathBuf {
    std::env::var_os("CHERCHI2022_ROOT")
        .map(PathBuf::from)
        .or_else(|| std::env::var_os("HOME").map(|h| PathBuf::from(h).join("cherchi2022")))
        .unwrap_or_default()
}

fn env_or_default(var: &str, rel: &str) -> PathBuf {
    std::env::var_os(var)
        .map(PathBuf::from)
        .unwrap_or_else(|| sidecar_root().join(rel))
}

/// Resolve the inputcheck binary path via env var or default. Returns
/// `Err(SidecarError::BinaryNotFound)` if neither resolves to an
/// existing file.
pub(crate) fn resolve_inputcheck_bin_from_env() -> Result<PathBuf, SidecarError> {
    let path = env_or_default(INPUTCHECK_ENV_VAR, INPUTCHECK_DEFAULT_BIN_PATH);
    if !path.exists() {
        return Err(SidecarError::BinaryNotFound { path });
    }
    Ok(path)
}

/// Resolve the binary path via env var or default. Returns
/// `Err(SidecarError::BinaryNotFound)` if neither resolves to an
/// existing file.
pub(crate) fn resolve_bin_from_env() -> Result<PathBuf, SidecarError> {
    let path = env_or_default(ENV_VAR, DEFAULT_BIN_PATH);
    if !path.exists() {
        return Err(SidecarError::BinaryNotFound { path });
    }
    Ok(path)
}

/// Spawn `cmd` and wait up to `timeout`. Kills the child on timeout
/// expiry. Polls at 1-second intervals (or `timeout` itself if
/// smaller). No external crate dependency.
pub(crate) fn run_with_timeout(
    mut cmd: Command,
    timeout: Duration,
) -> Result<Output, SidecarError> {
    use std::process::Stdio;
    cmd.stdout(Stdio::piped()).stderr(Stdio::piped());
    let mut child = cmd
        .spawn()
        .map_err(|source| SidecarError::SpawnFailed { source })?;
    let poll = std::cmp::min(Duration::from_secs(1), timeout);
    let polls = ((timeout.as_millis() + poll.as_millis() - 1) / poll.as_millis().max(1)) as usize;
    for _ in 0..polls {
        match child.try_wait() {
            Ok(Some(_)) => break,
            Ok(None) => std::thread::sleep(poll),
            Err(source) => return Err(SidecarError::SpawnFailed { source }),
        }
    }
    match child.try_wait() {
        Ok(Some(_)) => child
            .wait_with_output()
            .map_err(|source| SidecarError::SpawnFailed { source }),
        _ => {
            let _ = child.kill();
            let _ = child.wait();
            Err(SidecarError::TimedOut { after: timeout })
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn run_with_timeout_completes_fast_command() {
        // Use a command that exits immediately.
        let cmd = Command::new("true");
        let result = run_with_timeout(cmd, Duration::from_secs(5));
        // On any platform with /bin/true (Unix), this should succeed.
        // On platforms without it, the SpawnFailed result is acceptable.
        match result {
            Ok(out) => assert!(out.status.success()),
            Err(SidecarError::SpawnFailed { .. }) => {} // accept on portability
            Err(e) => panic!("unexpected error: {e:?}"),
        }
    }

    #[test]
    fn run_with_timeout_kills_slow_command() {
        // Use `sleep 10` with a 1-second timeout.
        let mut cmd = Command::new("sleep");
        cmd.arg("10");
        let result = run_with_timeout(cmd, Duration::from_secs(1));
        match result {
            Err(SidecarError::TimedOut { .. }) => {}    // expected
            Err(SidecarError::SpawnFailed { .. }) => {} // accept on portability
            Ok(out) => panic!("expected timeout, got success: {out:?}"),
            Err(e) => panic!("unexpected error: {e:?}"),
        }
    }
}
