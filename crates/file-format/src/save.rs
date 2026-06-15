use feature_engine::types::FeatureTree;
use serde::Serialize;
use uuid::Uuid;

use crate::metadata::{DocumentMetadata, ProjectMetadata, Tab, TabKind};

/// Current file format version.
/// v1: original format (coordinates in mm-scale scene units)
/// v2: true-meters (all length coordinates in meters, angles unchanged)
/// v3: multi-tab document model
pub const FORMAT_VERSION: u32 = 3;

/// The top-level v2 file structure (kept for deserialization compat).
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

/// V3 top-level file structure with tabs.
#[derive(Debug, Clone, Serialize)]
pub struct WaffleFileV3 {
    pub format: String,
    pub version: u32,
    pub document: DocumentMetadata,
    pub tabs: Vec<Tab>,
    pub active_tab: String,
}

/// Serialize a v3 document to pretty-printed JSON.
pub fn save_document(
    document: &DocumentMetadata,
    tabs: &[Tab],
    active_tab: impl Into<String>,
) -> String {
    let file = WaffleFileV3 {
        format: "waffle-iron".to_string(),
        version: FORMAT_VERSION,
        document: document.clone(),
        tabs: tabs.to_vec(),
        active_tab: active_tab.into(),
    };
    serde_json::to_string_pretty(&file).expect("Document serialization should never fail")
}

/// Serialize a project to a pretty-printed JSON string (v3 format).
/// Wraps the single feature tree into a single Part tab for backwards compatibility.
pub fn save_project(tree: &FeatureTree, metadata: &ProjectMetadata) -> String {
    let tab_id = Uuid::new_v4().to_string();
    let doc = DocumentMetadata {
        name: metadata.name.clone(),
        created: metadata.created,
        modified: metadata.modified,
        display_unit: metadata.display_unit.clone(),
    };
    let tab = Tab {
        id: tab_id.clone(),
        name: "Part 1".to_string(),
        kind: TabKind::Part {
            features: tree.clone(),
            preview_mesh: None,
        },
    };
    save_document(&doc, &[tab], tab_id)
}
