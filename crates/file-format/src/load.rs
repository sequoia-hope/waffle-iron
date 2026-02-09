use feature_engine::types::FeatureTree;
use serde::Deserialize;

use crate::errors::LoadError;
use crate::metadata::ProjectMetadata;
use crate::save::FORMAT_VERSION;

/// The top-level file structure for deserialization.
#[derive(Debug, Clone, Deserialize)]
pub struct WaffleFileRaw {
    pub format: String,
    pub version: u32,
    pub project: ProjectMetadata,
    pub features: FeatureTree,
}

/// Deserialize a project from a JSON string.
///
/// Validates the format identifier and version.
/// Returns the feature tree and project metadata.
pub fn load_project(json: &str) -> Result<(FeatureTree, ProjectMetadata), LoadError> {
    let raw: WaffleFileRaw =
        serde_json::from_str(json).map_err(|e| LoadError::ParseError(e.to_string()))?;

    // Validate format identifier
    if raw.format != "waffle-iron" {
        return Err(LoadError::UnknownFormat(raw.format));
    }

    // Validate version
    if raw.version > FORMAT_VERSION {
        return Err(LoadError::FutureVersion {
            file_version: raw.version,
            supported_version: FORMAT_VERSION,
        });
    }

    // Apply migrations if needed (version < current)
    let tree = if raw.version < FORMAT_VERSION {
        crate::migrate::migrate(raw.features, raw.version, FORMAT_VERSION)?
    } else {
        raw.features
    };

    Ok((tree, raw.project))
}
