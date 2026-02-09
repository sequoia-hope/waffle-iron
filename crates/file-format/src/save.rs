use feature_engine::types::FeatureTree;
use serde::Serialize;

use crate::metadata::ProjectMetadata;

/// Current file format version.
pub const FORMAT_VERSION: u32 = 1;

/// The top-level file structure.
#[derive(Debug, Clone, Serialize)]
pub struct WaffleFile {
    /// Format identifier.
    pub format: String,
    /// Format version number.
    pub version: u32,
    /// Project metadata.
    pub project: ProjectMetadata,
    /// The feature tree (the parametric recipe).
    pub features: FeatureTree,
}

/// Serialize a project to a pretty-printed JSON string.
pub fn save_project(tree: &FeatureTree, metadata: &ProjectMetadata) -> String {
    let file = WaffleFile {
        format: "waffle-iron".to_string(),
        version: FORMAT_VERSION,
        project: metadata.clone(),
        features: tree.clone(),
    };
    serde_json::to_string_pretty(&file).expect("FeatureTree serialization should never fail")
}
