//! Imported-body operation (STEP import SI1, task #138 —
//! `docs/step_import_roadmap.md` §3).
//!
//! Ingests an already-parsed, already-placed [`ImportedBodyData`] into the
//! kernel and wraps it as a standard one-output `OpResult`, so everything
//! downstream (render, picking, sketch-on-face, persistent naming) treats it
//! like any other body. Role assignment is skipped — imported faces have no
//! semantic operation roles; GeomRefs resolve through signature matching.

use crate::diff;
use crate::types::{Diagnostics, OpError, OpResult, Provenance};
use crate::{BodyOutput, KernelBundle};
use waffle_types::kernel::ImportedBodyData;
use waffle_types::OutputKey;

/// Ingest `data` (world-placed, meters) as one composite body.
pub fn execute_import(
    kb: &mut dyn KernelBundle,
    data: &ImportedBodyData,
) -> Result<OpResult, OpError> {
    let handle = kb.import_body(data)?;

    // Snapshot for persistent naming: everything is "created".
    let after = diff::snapshot(kb.as_introspect(), &handle);
    let empty = crate::TopoSnapshot {
        faces: Vec::new(),
        edges: Vec::new(),
        vertices: Vec::new(),
    };
    let diff_result = diff::diff(&empty, &after);

    Ok(OpResult {
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
            warnings: data.warnings.clone(),
            kernel_time_ms: 0.0,
            tessellation_time_ms: 0.0,
        },
    })
}
