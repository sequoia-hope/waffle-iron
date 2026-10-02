//! The two-tier STEP import — SI5 checkpoint **C6**
//! (`specs/step_import_si5_exact_analytic_ingestion.md` §7).
//!
//! An `ImportedBody` feature is served per SHELL by the first tier that can
//! carry it:
//!
//! 1. **Exact.** The shell's analytic extraction
//!    (`step_import::parse_step_tiered`) is scaled and placed on its own
//!    parameters and ingested as a first-class kernel solid
//!    (`Kernel::import_analytic_shell`). No tessellation anywhere in the
//!    path; the body integrates, exports and enters booleans like any
//!    constructed solid.
//! 2. **Mesh.** A shell the extractor refuses (an out-of-vocabulary surface
//!    or curve, a void boundary, a dropped `VERTEX_LOOP`) or the kernel
//!    refuses at ingestion (a vertex off its own surface, a self-contradicting
//!    loop, a C5c form) is served by the mesh-backed tier
//!    (`Kernel::import_body`) — tessellated, rendered, pickable, but not a
//!    boolean operand and not exportable.
//!
//! **The fallback is never silent.** Every shell the mesh tier serves adds a
//! feature warning naming the shell and the reason, so the user sees why a
//! body cannot be cut, and the P-series promotion path has its row.
//!
//! One file, one feature, possibly several bodies: with the exact tier each
//! shell is its own solid, so the outputs are `Main` for the first shell in
//! the file's canonical order and `Body { index }` after it; the mesh tier's
//! shells, if any, are wrapped as ONE composite body at the end — the shape
//! the mesh tier has always produced, so a file with no exact shell is
//! byte-identical to before C6.

use modeling_ops::{KernelBundle, OpResult};
use step_import::{TieredImport, TieredShell};
use waffle_types::kernel::{AnalyticShellData, ImportedBodyData};

use crate::types::ImportedBodyParams;

/// Why a whole file is served from the mesh tier without trying the exact
/// one — reported as a warning, never silently.
fn whole_file_mesh_reason(params: &ImportedBodyParams) -> Option<String> {
    if params.product.is_some() {
        return Some(
            "exact tier: a per-product import is served from the mesh tier (the exact tier \
             covers whole-file imports; per-product extraction is SI5 roadmap)"
                .to_string(),
        );
    }
    if !(params.scale.is_finite() && params.scale > 0.0) {
        return Some(format!(
            "exact tier: scale {} is not a positive finite factor; served from the mesh tier",
            params.scale
        ));
    }
    None
}

/// Execute the import feature over both tiers.
///
/// `mesh_whole_file` resolves the mesh-tier data for the WHOLE file (the
/// cached mesh parse) — called only when a shell the extractor accepted is
/// refused by the kernel, so the tessellation cost is paid for exactly the
/// shells that need it. Its shell order is the tiered parse's (both walk
/// the same canonical shell list), which is what makes `shells[i]` the same
/// shell in both.
pub(crate) fn execute_tiered_import(
    params: &ImportedBodyParams,
    tiered: &TieredImport,
    kb: &mut dyn KernelBundle,
    mesh_whole_file: &mut dyn FnMut() -> Result<ImportedBodyData, String>,
) -> Result<OpResult, String> {
    let mut parts: Vec<OpResult> = Vec::new();
    let mut warnings: Vec<String> = tiered.warnings.clone();
    // Shells the mesh tier serves, with the reason, in file order.
    let mut mesh_shells: Vec<(usize, String, waffle_types::kernel::ImportedShellData)> = Vec::new();
    let mut whole_file: Option<ImportedBodyData> = None;

    for (i, shell) in tiered.shells.iter().enumerate() {
        match shell {
            TieredShell::Mesh { why, data } => {
                mesh_shells.push((i, why.to_string(), data.clone()));
            }
            TieredShell::Exact(analytic) => {
                let mut placed: AnalyticShellData = analytic.clone();
                placed.apply_scale(params.scale);
                placed.apply_placement(params.rotation_deg, params.translation_m);
                match modeling_ops::execute_import_analytic(kb, &placed) {
                    Ok(res) => parts.push(res),
                    Err(e) => {
                        // The kernel refused what the extractor accepted: the
                        // mesh tier serves this shell, from the whole-file
                        // mesh parse (same canonical shell order).
                        let body = match &whole_file {
                            Some(b) => b,
                            None => {
                                whole_file = Some(mesh_whole_file()?);
                                whole_file.as_ref().expect("just set")
                            }
                        };
                        let Some(data) = body.shells.get(i) else {
                            return Err(format!(
                                "mesh tier has {} shells but the exact tier saw shell {i}",
                                body.shells.len()
                            ));
                        };
                        mesh_shells.push((i, e.to_string(), data.clone()));
                    }
                }
            }
        }
    }

    if !mesh_shells.is_empty() {
        for (i, why, _) in &mesh_shells {
            warnings.push(format!(
                "shell {i} served from the mesh tier (not a boolean operand, not exportable): {why}"
            ));
        }
        let mut data = ImportedBodyData {
            source_name: tiered.source_name.clone(),
            shells: mesh_shells.into_iter().map(|(_, _, d)| d).collect(),
            warnings: Vec::new(),
        };
        data.apply_scale(params.scale);
        data.apply_placement(params.rotation_deg, params.translation_m);
        parts.push(modeling_ops::execute_import(kb, &data).map_err(|e| e.to_string())?);
    }

    let mut result = modeling_ops::merge_import_results(parts);
    result.diagnostics.warnings.extend(warnings);
    Ok(result)
}

/// The whole-file mesh path, as before C6: parse (cached), scale, place,
/// ingest as one composite body. Used for the forms the exact tier does not
/// attempt (`whole_file_mesh_reason`) and for the per-product import.
pub(crate) fn execute_mesh_import(
    params: &ImportedBodyParams,
    mut data: ImportedBodyData,
    kb: &mut dyn KernelBundle,
    reason: Option<String>,
) -> Result<OpResult, String> {
    data.apply_scale(params.scale);
    data.apply_placement(params.rotation_deg, params.translation_m);
    let mut result = modeling_ops::execute_import(kb, &data).map_err(|e| e.to_string())?;
    if let Some(reason) = reason {
        result.diagnostics.warnings.push(reason);
    }
    Ok(result)
}

/// Decide the path for a whole-file import: `Some(reason)` ⇒ mesh tier for
/// the whole file, `None` ⇒ tiered.
pub(crate) fn mesh_only_reason(params: &ImportedBodyParams) -> Option<String> {
    whole_file_mesh_reason(params)
}
