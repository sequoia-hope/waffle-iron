use chrono::{DateTime, Utc};
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
