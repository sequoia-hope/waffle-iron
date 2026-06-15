use chrono::{DateTime, Utc};
use feature_engine::types::FeatureTree;
use serde::{Deserialize, Serialize};

/// Project metadata stored alongside the feature tree.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ProjectMetadata {
    /// Human-readable project name.
    pub name: String,
    /// When the project was first created.
    pub created: DateTime<Utc>,
    /// When the project was last modified.
    pub modified: DateTime<Utc>,
    /// Display unit preference (mm, cm, m, in, ft). None for legacy v1 files.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub display_unit: Option<String>,
}

impl ProjectMetadata {
    /// Create metadata with the given name and current timestamp.
    pub fn new(name: impl Into<String>) -> Self {
        let now = Utc::now();
        Self {
            name: name.into(),
            created: now,
            modified: now,
            display_unit: None,
        }
    }

    /// Create metadata with a display unit preference.
    pub fn with_display_unit(mut self, unit: impl Into<String>) -> Self {
        self.display_unit = Some(unit.into());
        self
    }
}

/// Document-level metadata (v3+).
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct DocumentMetadata {
    pub name: String,
    pub created: DateTime<Utc>,
    pub modified: DateTime<Utc>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub display_unit: Option<String>,
}

/// A single tab in a document.
///
/// `id` is an opaque document-level key used only for matching `active_tab`
/// to its tab. It is *not* required to be a UUID — the UI has historically
/// emitted non-UUID ids (e.g. the literal `"default"` for the implicit first
/// tab), so it is stored as a free-form string to keep those documents
/// loadable.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Tab {
    pub id: String,
    pub name: String,
    pub kind: TabKind,
}

/// The kind/content of a tab.
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(tag = "type")]
pub enum TabKind {
    Part {
        features: FeatureTree,
        #[serde(default, skip_serializing_if = "Option::is_none")]
        preview_mesh: Option<PreviewMesh>,
    },
}

/// A lightweight mesh for 3D thumbnail previews.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct PreviewMesh {
    pub vertices: Vec<f32>,
    pub normals: Vec<f32>,
    pub indices: Vec<u32>,
}
