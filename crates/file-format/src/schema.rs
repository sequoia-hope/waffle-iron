//! JSON Schema of the `.waffle` file at the current `FORMAT_VERSION`
//! (`save.rs`), generated from the Rust types
//! (`specs/waffle_v4_document_model.md` §5, `docs/FILE_FORMAT.md`). Behind
//! the `json-schema` feature; the committed golden is
//! `docs/schema/waffle-v5.schema.json`, pinned by `tests/schema_golden.rs`
//! (regenerate with `UPDATE_SCHEMA=1`).

use schemars::JsonSchema;
use serde::Serialize;
use serde_json::{Map, Value};

use crate::metadata::{DocumentMetadata, Tab};
use crate::sources::SourceEntry;

/// Owned mirror of the file envelope for schema derivation. (The golden
/// keeps its historical name `waffle-v5.schema.json`, the version it was
/// first generated at; its content tracks the current format.)
#[derive(Debug, Serialize, JsonSchema)]
#[schemars(
    rename = "WaffleFile",
    title = "Waffle Iron .waffle document"
)]
pub struct WaffleFileSchema {
    /// Must be exactly `"waffle-iron"`.
    #[schemars(regex(pattern = "^waffle-iron$"))]
    pub format: String,
    /// Format version: the writer's `FORMAT_VERSION` (`save.rs`).
    pub version: u32,
    /// Oldest reader (by its FORMAT_VERSION) that can parse this file.
    pub min_reader_version: u32,
    pub document: DocumentMetadata,
    /// External content the document depends on.
    #[serde(default)]
    pub sources: Vec<SourceEntry>,
    pub tabs: Vec<Tab>,
    /// Id of the tab open when saved; must name a tab.
    pub active_tab: String,
    /// Unknown envelope keys are preserved.
    #[serde(flatten)]
    pub extra: Map<String, Value>,
}

/// The schema as a JSON value (draft 2020-12, schemars 1.x default).
pub fn waffle_file_schema() -> Value {
    serde_json::to_value(schemars::schema_for!(WaffleFileSchema)).expect("schema serializes")
}
