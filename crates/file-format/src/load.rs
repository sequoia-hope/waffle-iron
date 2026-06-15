use feature_engine::types::FeatureTree;
use serde::Deserialize;

use crate::errors::LoadError;
use crate::metadata::{DocumentMetadata, ProjectMetadata, Tab, TabKind};
use crate::save::FORMAT_VERSION;

/// The top-level v2 file structure for deserialization.
#[derive(Debug, Clone, Deserialize)]
pub struct WaffleFileRaw {
    pub format: String,
    pub version: u32,
    pub project: ProjectMetadata,
    pub features: FeatureTree,
}

/// V3 file structure for deserialization.
#[derive(Debug, Clone, Deserialize)]
struct WaffleFileV3Raw {
    #[allow(dead_code)]
    pub format: String,
    #[allow(dead_code)]
    pub version: u32,
    pub document: DocumentMetadata,
    pub tabs: Vec<Tab>,
    pub active_tab: String,
}

/// Load a v3 document from JSON.
/// Handles both v2 (flat) and v3 (tabbed) formats.
/// For v2/v1, wraps the single feature tree into a Part tab.
pub fn load_document(json: &str) -> Result<(DocumentMetadata, Vec<Tab>, String), LoadError> {
    let value: serde_json::Value =
        serde_json::from_str(json).map_err(|e| LoadError::ParseError(e.to_string()))?;

    if value.get("format").and_then(|f| f.as_str()) != Some("waffle-iron") {
        let fmt = value
            .get("format")
            .and_then(|f| f.as_str())
            .unwrap_or("unknown")
            .to_string();
        return Err(LoadError::UnknownFormat(fmt));
    }

    let version = value.get("version").and_then(|v| v.as_u64()).unwrap_or(0) as u32;
    if version > FORMAT_VERSION {
        return Err(LoadError::FutureVersion {
            file_version: version,
            supported_version: FORMAT_VERSION,
        });
    }

    // V3+ format: has "document" and "tabs"
    if version >= 3 && value.get("tabs").is_some() {
        let raw: WaffleFileV3Raw =
            serde_json::from_value(value).map_err(|e| LoadError::ParseError(e.to_string()))?;

        // Validate active_tab references a real tab
        if !raw.tabs.iter().any(|t| t.id == raw.active_tab) {
            return Err(LoadError::ParseError(
                "active_tab references non-existent tab".to_string(),
            ));
        }

        return Ok((raw.document, raw.tabs, raw.active_tab));
    }

    // Fall back to v2/v1 loading (flat format)
    let (tree, meta) = load_project_from_value(value)?;
    let tab_id = uuid::Uuid::new_v4().to_string();
    let doc = DocumentMetadata {
        name: meta.name,
        created: meta.created,
        modified: meta.modified,
        display_unit: meta.display_unit,
    };
    let tab = Tab {
        id: tab_id.clone(),
        name: "Part 1".to_string(),
        kind: TabKind::Part {
            features: tree,
            preview_mesh: None,
        },
    };
    Ok((doc, vec![tab], tab_id))
}

/// Deserialize a project from a JSON string.
///
/// Handles both v2 (flat) and v3 (tabbed) formats.
/// For v3, returns the active tab's feature tree.
/// Returns the feature tree and project metadata.
pub fn load_project(json: &str) -> Result<(FeatureTree, ProjectMetadata), LoadError> {
    let value: serde_json::Value =
        serde_json::from_str(json).map_err(|e| LoadError::ParseError(e.to_string()))?;

    if value.get("format").and_then(|f| f.as_str()) != Some("waffle-iron") {
        let fmt = value
            .get("format")
            .and_then(|f| f.as_str())
            .unwrap_or("unknown")
            .to_string();
        return Err(LoadError::UnknownFormat(fmt));
    }

    let version = value.get("version").and_then(|v| v.as_u64()).unwrap_or(0) as u32;
    if version > FORMAT_VERSION {
        return Err(LoadError::FutureVersion {
            file_version: version,
            supported_version: FORMAT_VERSION,
        });
    }

    // V3+ format: has "document" and "tabs"
    if version >= 3 && value.get("tabs").is_some() {
        let raw: WaffleFileV3Raw =
            serde_json::from_value(value).map_err(|e| LoadError::ParseError(e.to_string()))?;

        let active_tab = raw
            .tabs
            .iter()
            .find(|t| t.id == raw.active_tab)
            .or_else(|| raw.tabs.first())
            .ok_or_else(|| LoadError::ParseError("no tabs in document".to_string()))?;

        let tree = match &active_tab.kind {
            TabKind::Part { features, .. } => features.clone(),
        };

        let meta = ProjectMetadata {
            name: raw.document.name,
            created: raw.document.created,
            modified: raw.document.modified,
            display_unit: raw.document.display_unit,
        };

        return Ok((tree, meta));
    }

    // V1/V2 format: has "project" and "features"
    load_project_from_value(value)
}

/// Internal: load a v1/v2 project from a pre-parsed JSON value.
fn load_project_from_value(
    value: serde_json::Value,
) -> Result<(FeatureTree, ProjectMetadata), LoadError> {
    let raw: WaffleFileRaw =
        serde_json::from_value(value).map_err(|e| LoadError::ParseError(e.to_string()))?;

    // Validate format identifier
    if raw.format != "waffle-iron" {
        return Err(LoadError::UnknownFormat(raw.format));
    }

    // Apply migrations if needed (version < current, but only content migrations up to v2)
    let tree = if raw.version < 2 {
        crate::migrate::migrate(raw.features, raw.version, 2)?
    } else {
        raw.features
    };

    Ok((tree, raw.project))
}
