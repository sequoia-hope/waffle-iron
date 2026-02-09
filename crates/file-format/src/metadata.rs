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
}

impl ProjectMetadata {
    /// Create metadata with the given name and current timestamp.
    pub fn new(name: impl Into<String>) -> Self {
        let now = Utc::now();
        Self {
            name: name.into(),
            created: now,
            modified: now,
        }
    }
}
