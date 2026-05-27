//! Cherchi 2022 mesh boolean engine via subprocess sidecar.
//!
//! Wraps the upstream `mesh_booleans` C++ binary as a Rust
//! [`MeshBoolean`] implementation. The binary lives outside the
//! repo (build it per `docs/sidecar/cherchi2022_build_guide.md`);
//! this crate locates it via `CHERCHI2022_BIN` env var or a
//! default path under `/home/claude/cherchi2022/...`.
//!
//! ## NOT WASM-compatible
//!
//! This crate uses `std::process::Command` and filesystem I/O,
//! neither of which is available in browser WASM. WASM targets
//! must use the (future) native cherchi-rs implementation of
//! `MeshBoolean` instead.
//!
//! ## License posture
//!
//! This Rust crate is MIT, matching the rest of the workspace. The
//! wrapped C++ binary (`mesh_booleans`) is itself MIT (Cherchi 2022).
//! The binary internally links the LGPL-2.1 `Indirect_Predicates`
//! library, but since we invoke it as a subprocess we don't link
//! into our process — the LGPL boundary is the binary, not our crate.
//!
//! ## Quick start
//!
//! ```no_run
//! use cherchi_sidecar_rs::SidecarBoolean;
//! use cherchi_rs::{Mesh, MeshBoolean};
//! use cad_primitives::BoolOp;
//!
//! let sb = SidecarBoolean::from_env().expect("binary not found");
//! let a = Mesh::empty(); // your input mesh
//! let b = Mesh::empty(); // your other input mesh
//! let result = sb.boolean(&a, &b, BoolOp::Union).expect("boolean failed");
//! ```

pub mod obj;
mod process;

use std::error::Error;
use std::fmt;
use std::io;
use std::path::PathBuf;
use std::process::ExitStatus;
use std::time::Duration;

use cad_primitives::BoolOp;
use cherchi_rs::{Mesh, MeshBoolean};

pub use process::DEFAULT_BIN_PATH;

/// Default subprocess timeout: 30 seconds. Overridable via
/// [`SidecarBoolean::new`].
pub const DEFAULT_TIMEOUT: Duration = Duration::from_secs(30);

/// Subprocess-based [`MeshBoolean`] implementation.
///
/// Writes inputs as OBJ to a tempdir, invokes the C++ binary with
/// `<op> a.obj b.obj out.obj`, parses the output OBJ.
pub struct SidecarBoolean {
    bin_path: PathBuf,
    timeout: Duration,
}

impl SidecarBoolean {
    /// Resolve the binary via `CHERCHI2022_BIN` env var or the
    /// default path. Returns [`SidecarError::BinaryNotFound`] if
    /// neither resolves to an existing file.
    pub fn from_env() -> Result<Self, SidecarError> {
        let bin_path = process::resolve_bin_from_env()?;
        Ok(Self {
            bin_path,
            timeout: DEFAULT_TIMEOUT,
        })
    }

    /// Construct with an explicit binary path + timeout. Does not
    /// validate that the binary exists at construction time;
    /// validation occurs on first `boolean()` call.
    pub fn new(bin_path: PathBuf, timeout: Duration) -> Self {
        Self { bin_path, timeout }
    }
}

/// Map a `BoolOp` to the upstream binary's CLI string.
fn cli_arg(op: BoolOp) -> &'static str {
    match op {
        BoolOp::Union => "union",
        BoolOp::Intersect => "intersection",
        BoolOp::Subtract => "subtraction",
        BoolOp::Xor => "xor",
    }
}

/// Build a fresh per-call tempdir under `std::env::temp_dir()`.
/// Uses process ID + a counter to avoid collisions when the same
/// process makes concurrent calls.
fn fresh_tempdir() -> Result<PathBuf, SidecarError> {
    use std::sync::atomic::{AtomicU64, Ordering};
    static COUNTER: AtomicU64 = AtomicU64::new(0);
    let n = COUNTER.fetch_add(1, Ordering::Relaxed);
    let pid = std::process::id();
    let dir = std::env::temp_dir().join(format!("cherchi-sidecar-rs-{pid}-{n}"));
    let _ = std::fs::remove_dir_all(&dir);
    std::fs::create_dir_all(&dir).map_err(|source| SidecarError::ObjIo { source })?;
    Ok(dir)
}

impl MeshBoolean for SidecarBoolean {
    fn boolean(
        &self,
        a: &Mesh,
        b: &Mesh,
        op: BoolOp,
    ) -> Result<Mesh, Box<dyn Error + Send + Sync>> {
        let tmp = fresh_tempdir()?;
        let a_path = tmp.join("a.obj");
        let b_path = tmp.join("b.obj");
        let out_path = tmp.join("out.obj");
        obj::write_obj(&a_path, a).map_err(|source| SidecarError::ObjIo { source })?;
        obj::write_obj(&b_path, b).map_err(|source| SidecarError::ObjIo { source })?;
        let mut cmd = std::process::Command::new(&self.bin_path);
        cmd.arg(cli_arg(op))
            .arg(&a_path)
            .arg(&b_path)
            .arg(&out_path);
        let output = process::run_with_timeout(cmd, self.timeout)?;
        if !output.status.success() {
            return Err(Box::new(SidecarError::NonZeroExit {
                status: output.status,
                stderr: String::from_utf8_lossy(&output.stderr).to_string(),
            }));
        }
        let mesh = obj::read_obj(&out_path).map_err(|source| SidecarError::ObjParse { source })?;
        Ok(mesh)
    }
}

/// Concrete error type for [`SidecarBoolean`] operations.
///
/// The [`MeshBoolean`] trait returns `Box<dyn Error + Send + Sync>`;
/// callers can downcast for specific handling:
///
/// ```ignore
/// match sb.boolean(&a, &b, BoolOp::Union) {
///     Err(e) if let Some(se) = e.downcast_ref::<SidecarError>() => { ... }
///     ...
/// }
/// ```
#[derive(Debug)]
pub enum SidecarError {
    /// The binary does not exist at the resolved path.
    BinaryNotFound { path: PathBuf },
    /// `Command::spawn` failed (e.g., binary not executable).
    SpawnFailed { source: io::Error },
    /// Subprocess exceeded the configured timeout. Killed before return.
    TimedOut { after: Duration },
    /// Subprocess exited with a non-zero status.
    NonZeroExit {
        status: ExitStatus,
        stderr: String,
    },
    /// OBJ file write / spawn-prep I/O error.
    ObjIo { source: io::Error },
    /// OBJ file parse error (malformed output from binary, etc.).
    ObjParse { source: io::Error },
}

impl fmt::Display for SidecarError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::BinaryNotFound { path } => {
                write!(f, "Cherchi 2022 binary not found at {}", path.display())
            }
            Self::SpawnFailed { source } => write!(f, "failed to spawn subprocess: {source}"),
            Self::TimedOut { after } => write!(f, "subprocess timed out after {after:?}"),
            Self::NonZeroExit { status, stderr } => {
                write!(f, "subprocess exited {status:?}; stderr: {stderr}")
            }
            Self::ObjIo { source } => write!(f, "OBJ I/O failed: {source}"),
            Self::ObjParse { source } => write!(f, "OBJ parse failed: {source}"),
        }
    }
}

impl Error for SidecarError {
    fn source(&self) -> Option<&(dyn Error + 'static)> {
        match self {
            Self::SpawnFailed { source } | Self::ObjIo { source } | Self::ObjParse { source } => {
                Some(source)
            }
            _ => None,
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn error_binary_not_found_carries_path() {
        let p = PathBuf::from("/nonexistent/path");
        let e = SidecarError::BinaryNotFound { path: p.clone() };
        match e {
            SidecarError::BinaryNotFound { path } => assert_eq!(path, p),
            _ => panic!("wrong variant"),
        }
    }

    #[test]
    fn error_display_and_std_error_trait() {
        let e = SidecarError::BinaryNotFound {
            path: PathBuf::from("/x"),
        };
        let msg = format!("{}", e);
        assert!(msg.contains("/x"));
        // Confirm it implements std::error::Error
        let _: &dyn Error = &e;
    }
}
