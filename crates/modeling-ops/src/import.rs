//! Imported-body operations (STEP import SI1, task #138 —
//! `docs/step_import_roadmap.md` §3; SI5 C6,
//! `specs/step_import_si5_exact_analytic_ingestion.md`).
//!
//! Two tiers, one result shape. [`execute_import`] ingests an
//! already-parsed, already-placed mesh-backed [`ImportedBodyData`];
//! [`execute_import_analytic`] ingests one EXACT [`AnalyticShellData`] as a
//! first-class kernel solid. Both wrap the body as a standard one-output
//! `OpResult`, so everything downstream (render, picking, sketch-on-face,
//! persistent naming, booleans for the exact tier) treats it like any other
//! body. Role assignment is skipped — imported faces have no semantic
//! operation roles; GeomRefs resolve through signature matching.

use crate::diff;
use crate::types::{Diagnostics, OpError, OpResult, Provenance};
use crate::{BodyOutput, KernelBundle};
use waffle_types::kernel::{AnalyticShellData, ImportedBodyData, KernelSolidHandle};
use waffle_types::OutputKey;

/// Ingest `data` (world-placed, meters) as one composite mesh-backed body.
pub fn execute_import(
    kb: &mut dyn KernelBundle,
    data: &ImportedBodyData,
) -> Result<OpResult, OpError> {
    let handle = kb.import_body(data)?;
    Ok(one_body_result(kb, handle, data.warnings.clone()))
}

/// Ingest one exact analytic shell (world-placed, meters) as a first-class
/// kernel solid. A kernel refusal is returned as-is — the caller decides
/// whether the mesh tier serves the shell instead, and must say so.
pub fn execute_import_analytic(
    kb: &mut dyn KernelBundle,
    shell: &AnalyticShellData,
) -> Result<OpResult, OpError> {
    let handle = kb.import_analytic_shell(shell)?;
    Ok(one_body_result(kb, handle, Vec::new()))
}

/// Snapshot a freshly imported body as a `Main` output whose every entity
/// is "created", for persistent naming.
fn one_body_result(
    kb: &mut dyn KernelBundle,
    handle: KernelSolidHandle,
    warnings: Vec<String>,
) -> OpResult {
    let after = diff::snapshot(kb.as_introspect(), &handle);
    let empty = crate::TopoSnapshot {
        faces: Vec::new(),
        edges: Vec::new(),
        vertices: Vec::new(),
    };
    let diff_result = diff::diff(&empty, &after);

    OpResult {
        outputs: vec![(
            OutputKey::Main,
            BodyOutput {
                handle,
                mesh: None,
                edges: None,
            },
        )],
        provenance: Provenance {
            created: diff_result.created,
            deleted: Vec::new(),
            modified: Vec::new(),
            role_assignments: Vec::new(),
        },
        diagnostics: Diagnostics {
            warnings,
            kernel_time_ms: 0.0,
            tessellation_time_ms: 0.0,
        },
    }
}

/// Fold several one-body import results into one feature result: the first
/// body is `Main`, the rest `Body { index }` in order; provenance and
/// warnings concatenate. The order is the caller's (SI5 C6 uses the file's
/// canonical shell order, so the keys are stable across rebuilds).
pub fn merge_import_results(parts: Vec<OpResult>) -> OpResult {
    let mut merged = OpResult {
        outputs: Vec::new(),
        provenance: Provenance {
            created: Vec::new(),
            deleted: Vec::new(),
            modified: Vec::new(),
            role_assignments: Vec::new(),
        },
        diagnostics: Diagnostics::default(),
    };
    for part in parts {
        for (_, body) in part.outputs {
            let key = if merged.outputs.is_empty() {
                OutputKey::Main
            } else {
                OutputKey::Body {
                    index: merged.outputs.len(),
                }
            };
            merged.outputs.push((key, body));
        }
        merged.provenance.created.extend(part.provenance.created);
        merged
            .diagnostics
            .warnings
            .extend(part.diagnostics.warnings);
        merged.diagnostics.kernel_time_ms += part.diagnostics.kernel_time_ms;
        merged.diagnostics.tessellation_time_ms += part.diagnostics.tessellation_time_ms;
    }
    merged
}
