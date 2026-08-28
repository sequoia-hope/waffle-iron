use feature_engine::types::FeatureTree;
use serde::Serialize;
use uuid::Uuid;

use crate::metadata::{DocumentMetadata, ProjectMetadata, Tab, TabKind};

/// Current file format version.
/// v1: original format (coordinates in mm-scale scene units)
/// v2: true-meters (all length coordinates in meters, angles unchanged)
/// v3: multi-tab document model
pub const FORMAT_VERSION: u32 = 3;

/// Oldest reader (by its `FORMAT_VERSION`) that can parse files we write.
///
/// Written into every file as `min_reader_version`; readers refuse files whose
/// `min_reader_version` exceeds their own `FORMAT_VERSION` with a clean
/// `LoadError::FutureVersion` instead of a raw serde parse error. Bump this
/// (together with `FORMAT_VERSION`) whenever a change lands that older readers
/// cannot parse — in this format that includes NEW ENUM VARIANTS (a new
/// `Operation`/`TabKind`/constraint/selector tag is wire-breaking for old
/// readers even though it looks additive). Purely additive defaulted fields do
/// not require a bump. Files without the field (all pre-2026-08-28 files,
/// including the assay corpus) default to 0 and always pass.
/// See `docs/FILE_FORMAT.md` §13.
pub const MIN_READER_VERSION: u32 = 3;

// Keep the constants coherent: we can never require a reader newer than the
// version we claim to write.
const _: () = assert!(MIN_READER_VERSION <= FORMAT_VERSION);

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
    /// See [`MIN_READER_VERSION`]. Old readers ignore this unknown field.
    pub min_reader_version: u32,
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
        min_reader_version: MIN_READER_VERSION,
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

/// [`save_project`] plus a self-check: never hand out a file the loader would
/// refuse. Production save paths (the wasm-bridge `SaveProject` handler) use
/// this so corruption is a loud save-time error instead of a file that saves
/// silently and can never be opened again.
///
/// The known corruption class is non-finite floats: serde_json (and JS
/// `JSON.stringify`) serialize NaN/∞ as `null`, which every reader then
/// rejects — so a NaN anywhere in the live tree used to poison the only saved
/// copy. The round-trip check catches that and any future class of
/// save-side corruption without enumerating float fields.
pub fn save_project_verified(
    tree: &FeatureTree,
    metadata: &ProjectMetadata,
) -> Result<String, crate::errors::LoadError> {
    let json = save_project(tree, metadata);
    crate::load::load_project(&json)?;
    Ok(json)
}
