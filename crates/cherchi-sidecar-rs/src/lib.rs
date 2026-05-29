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

// Re-export the frozen Stage-2 contract (defined in cherchi-rs) so consumers can
// name it through this producer crate.
pub use cherchi_rs::labeled_arrangement::{InputId, LabeledArrangement};
pub use process::{DEFAULT_BIN_PATH, INPUTCHECK_DEFAULT_BIN_PATH};

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

/// Verdict of the `mesh_booleans_inputcheck` reference oracle: the five
/// Cherchi 2022 §3 input axioms a mesh must satisfy for the boolean
/// pipeline to be well-defined (malformed input is undefined behavior).
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct InputCheckReport {
    /// Every edge is shared by exactly two triangles (combinatorial 2-manifold).
    pub manifold: bool,
    /// No boundary edges (the surface is closed).
    pub watertight: bool,
    /// Adjacent triangles wind consistently across each shared edge.
    pub local_orientation: bool,
    /// The mesh as a whole is oriented outward (not inside-out).
    pub global_orientation: bool,
    /// No pair of triangles intersects except along shared edges/vertices.
    pub intersection_free: bool,
}

impl InputCheckReport {
    /// True iff all five axioms pass.
    pub fn all_pass(&self) -> bool {
        self.manifold
            && self.watertight
            && self.local_orientation
            && self.global_orientation
            && self.intersection_free
    }
}

/// Run the upstream `mesh_booleans_inputcheck` binary as a reference oracle
/// for the Cherchi 2022 §3 input axioms over `mesh`.
///
/// Resolves the binary via `CHERCHI2022_INPUTCHECK_BIN` env var or the
/// default path ([`INPUTCHECK_DEFAULT_BIN_PATH`]); returns
/// [`SidecarError::BinaryNotFound`] if neither exists.
///
/// **Verdict parsing:** the binary prints a 5-line verdict to **stdout**
/// (not stderr) and exits **0 regardless of pass/fail**, so the verdict is
/// parsed from stdout, never gated on the exit code. Each line names one
/// axiom and ends in `passed` / `failed` (case-insensitive).
pub fn inputcheck(mesh: &Mesh, timeout: Duration) -> Result<InputCheckReport, SidecarError> {
    let bin_path = process::resolve_inputcheck_bin_from_env()?;
    let tmp = fresh_tempdir()?;
    let mesh_path = tmp.join("mesh.obj");
    obj::write_obj(&mesh_path, mesh).map_err(|source| SidecarError::ObjIo { source })?;
    let mut cmd = std::process::Command::new(&bin_path);
    cmd.arg(&mesh_path);
    let output = process::run_with_timeout(cmd, timeout)?;
    // Exit code is 0 regardless of pass/fail; the verdict is on stdout.
    let stdout = String::from_utf8_lossy(&output.stdout);
    Ok(parse_inputcheck_stdout(&stdout))
}

/// Parse the 5-line inputcheck verdict from stdout. Each line is classified
/// by keyword; a line "passes" iff it contains `passed` (case-insensitive)
/// and not `failed`. Unmatched / absent lines default to `false` (a missing
/// verdict is a failure, never a silent pass — P9).
fn parse_inputcheck_stdout(stdout: &str) -> InputCheckReport {
    let mut report = InputCheckReport {
        manifold: false,
        watertight: false,
        local_orientation: false,
        global_orientation: false,
        intersection_free: false,
    };
    for line in stdout.lines() {
        let lower = line.to_ascii_lowercase();
        let passed = lower.contains("passed") && !lower.contains("failed");
        if lower.contains("manifold") {
            report.manifold = passed;
        } else if lower.contains("watertight") {
            report.watertight = passed;
        } else if lower.contains("local") && lower.contains("orientation") {
            report.local_orientation = passed;
        } else if lower.contains("global") && lower.contains("orientation") {
            report.global_orientation = passed;
        } else if lower.contains("intersection") {
            report.intersection_free = passed;
        }
    }
    report
}

/// Produce the Stage-2 [`LabeledArrangement`] from the patched `mesh_booleans`
/// binary: the full exact arrangement of `a` ∪ `b` plus, per arrangement
/// triangle, its surface solid(s), per-solid inside/outside, and patch id.
///
/// Invokes the binary with `union a.obj b.obj out.obj` and
/// `CHERCHI_DUMP_LABELS=<tmp>/arr`, which causes the (patched) binary to write
/// `<tmp>/arr.obj` (the arrangement mesh) and `<tmp>/arr.labels` (per-triangle
/// labels). The chosen op is irrelevant — the dump happens before the op filter
/// — so `union` is used unconditionally.
///
/// Returns [`SidecarError::BinaryNotFound`] when the binary is absent (callers
/// self-skip), or [`SidecarError::LabelsParse`] when the `.labels` file is
/// missing or malformed (never a silent success — P9).
pub fn labeled_arrangement(
    a: &Mesh,
    b: &Mesh,
    timeout: Duration,
) -> Result<LabeledArrangement, SidecarError> {
    let bin_path = process::resolve_bin_from_env()?;
    let tmp = fresh_tempdir()?;
    let a_path = tmp.join("a.obj");
    let b_path = tmp.join("b.obj");
    let out_path = tmp.join("out.obj");
    let arr_base = tmp.join("arr");
    obj::write_obj(&a_path, a).map_err(|source| SidecarError::ObjIo { source })?;
    obj::write_obj(&b_path, b).map_err(|source| SidecarError::ObjIo { source })?;

    let mut cmd = std::process::Command::new(&bin_path);
    cmd.arg("union")
        .arg(&a_path)
        .arg(&b_path)
        .arg(&out_path)
        .env("CHERCHI_DUMP_LABELS", &arr_base);
    let output = process::run_with_timeout(cmd, timeout)?;
    if !output.status.success() {
        return Err(SidecarError::NonZeroExit {
            status: output.status,
            stderr: String::from_utf8_lossy(&output.stderr).to_string(),
        });
    }

    let arr_obj_path = tmp.join("arr.obj");
    let arr_labels_path = tmp.join("arr.labels");
    let mesh = obj::read_obj(&arr_obj_path).map_err(|source| SidecarError::ObjParse { source })?;
    let labels_text =
        std::fs::read_to_string(&arr_labels_path).map_err(|e| SidecarError::LabelsParse {
            msg: format!("cannot read {}: {e}", arr_labels_path.display()),
        })?;
    parse_labels(mesh, &labels_text)
}

/// Parse the `.labels` sidecar into a [`LabeledArrangement`].
///
/// Format: first line `num_tris num_inputs`; then one line per triangle (in id
/// order) of the form `surf_bits... | inside_bits... | patch`, where the bit
/// lists are space-separated set-bit positions.
fn parse_labels(mesh: Mesh, text: &str) -> Result<LabeledArrangement, SidecarError> {
    let err = |msg: String| SidecarError::LabelsParse { msg };

    let mut lines = text.lines();
    let header = lines
        .next()
        .ok_or_else(|| err("empty labels file (no header)".to_string()))?;
    let mut header_parts = header.split_whitespace();
    let num_tris: usize = header_parts
        .next()
        .ok_or_else(|| err("header missing num_tris".to_string()))?
        .parse()
        .map_err(|e| err(format!("bad num_tris in header: {e}")))?;
    let num_inputs: u32 = header_parts
        .next()
        .ok_or_else(|| err("header missing num_inputs".to_string()))?
        .parse()
        .map_err(|e| err(format!("bad num_inputs in header: {e}")))?;

    let mut surface: Vec<Vec<InputId>> = Vec::with_capacity(num_tris);
    let mut inside: Vec<Vec<bool>> = Vec::with_capacity(num_tris);
    let mut patch: Vec<u32> = Vec::with_capacity(num_tris);

    for (t, line) in lines.enumerate() {
        // Each tri line is `surf... | inside... | patch`.
        let mut sections = line.split('|');
        let surf_sec = sections
            .next()
            .ok_or_else(|| err(format!("tri {t}: missing surface section")))?;
        let inside_sec = sections
            .next()
            .ok_or_else(|| err(format!("tri {t}: missing inside section")))?;
        let patch_sec = sections
            .next()
            .ok_or_else(|| err(format!("tri {t}: missing patch section")))?;
        if sections.next().is_some() {
            return Err(err(format!("tri {t}: too many '|'-separated sections")));
        }

        // surface[t]: set-bit positions -> InputId.
        let mut surf_ids = Vec::new();
        for tok in surf_sec.split_whitespace() {
            let id: u32 = tok
                .parse()
                .map_err(|e| err(format!("tri {t}: bad surface bit {tok:?}: {e}")))?;
            surf_ids.push(InputId(id));
        }

        // inside[t]: dense vec of length num_inputs, listed positions true.
        let mut inside_bits = vec![false; num_inputs as usize];
        for tok in inside_sec.split_whitespace() {
            let k: usize = tok
                .parse()
                .map_err(|e| err(format!("tri {t}: bad inside bit {tok:?}: {e}")))?;
            if k >= inside_bits.len() {
                return Err(err(format!(
                    "tri {t}: inside bit {k} out of range (num_inputs={num_inputs})"
                )));
            }
            inside_bits[k] = true;
        }

        let patch_tok = patch_sec
            .split_whitespace()
            .next()
            .ok_or_else(|| err(format!("tri {t}: missing patch id")))?;
        let patch_id: u32 = patch_tok
            .parse()
            .map_err(|e| err(format!("tri {t}: bad patch id {patch_tok:?}: {e}")))?;

        surface.push(surf_ids);
        inside.push(inside_bits);
        patch.push(patch_id);
    }

    if surface.len() != num_tris {
        return Err(err(format!(
            "label line count {} != header num_tris {num_tris}",
            surface.len()
        )));
    }
    if mesh.tris.len() != num_tris {
        return Err(err(format!(
            "arrangement mesh tri count {} != labels num_tris {num_tris}",
            mesh.tris.len()
        )));
    }

    Ok(LabeledArrangement {
        mesh,
        surface,
        inside,
        patch,
        num_inputs,
    })
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
    NonZeroExit { status: ExitStatus, stderr: String },
    /// OBJ file write / spawn-prep I/O error.
    ObjIo { source: io::Error },
    /// OBJ file parse error (malformed output from binary, etc.).
    ObjParse { source: io::Error },
    /// The `.labels` sidecar file is missing or malformed (e.g. bad header,
    /// wrong line count, unparseable bit list). Never a silent success (P9).
    LabelsParse { msg: String },
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
            Self::LabelsParse { msg } => write!(f, "labels parse failed: {msg}"),
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
