use feature_engine::types::FeatureTree;
use kernel_fork::TruckKernel;
use waffle_types::OutputKey;

use crate::errors::ExportError;

/// Export a feature tree to STEP AP203 format.
///
/// Rebuilds the model from scratch using TruckKernel, then exports
/// the final solid to a STEP string. Returns an error if the rebuild
/// fails or produces no solid.
pub fn export_step(tree: &FeatureTree, kb: &mut TruckKernel) -> Result<String, ExportError> {
    // Build an engine and rebuild
    let mut engine = feature_engine::Engine::new();
    engine.tree = tree.clone();
    engine.rebuild_from_scratch(kb);

    // Find the last non-suppressed feature with a Main output
    let last_handle = tree
        .features
        .iter()
        .rev()
        .filter(|f| !f.suppressed)
        .find_map(|f| {
            engine.get_result(f.id).and_then(|result| {
                result
                    .outputs
                    .iter()
                    .find(|(key, _)| *key == OutputKey::Main)
                    .map(|(_, body)| body.handle.clone())
            })
        })
        .ok_or(ExportError::NoSolid)?;

    // Export via TruckKernel
    let step_string = kb
        .export_step(&last_handle, "export.step")
        .map_err(|e| ExportError::StepExportFailed(format!("{}", e)))?;

    Ok(step_string)
}
