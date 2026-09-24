use feature_engine::types::FeatureTree;
use serde::Serialize;
use serde_json::{Map, Value};

use crate::document::WaffleDocument;
use crate::metadata::{DocumentMetadata, ProjectMetadata, Tab};
use crate::sources::SourceEntry;

/// Current file format version.
/// v1: original format (coordinates in mm-scale scene units)
/// v2: true-meters (all length coordinates in meters, angles unchanged)
/// v3: multi-tab document model
/// v4: document identity, `sources` table, opaque unknown tab/source kinds,
///     unknown-key preservation (`specs/waffle_v4_document_model.md`)
/// v5: `GeomRef.scope` — references into another tab's instance (in-context
///     editing, spec §2.8). A v4 reader would drop the field and resolve the
///     anchor locally, so the reader floor moved with it.
pub const FORMAT_VERSION: u32 = 5;

/// Oldest reader (by its `FORMAT_VERSION`) that can parse files we write.
///
/// Written into every file as `min_reader_version`; readers refuse files whose
/// `min_reader_version` exceeds their own `FORMAT_VERSION` with a clean
/// `LoadError::FutureVersion` instead of a raw serde parse error. Bump this
/// (together with `FORMAT_VERSION`) whenever a change lands that older readers
/// cannot parse — in this format that includes NEW `Operation`, constraint,
/// selector and `PlaneDefinition` variants (wire-breaking for old readers
/// even though they look additive). Since v4 a new **tab kind, source kind or
/// locator kind** does NOT require a bump: v4 readers preserve unknown ones
/// opaquely. Purely additive defaulted fields never require a bump. Files
/// without the field (all pre-2026-08-28 files, including the assay corpus)
/// default to 0 and always pass. See `docs/FILE_FORMAT.md` §13.
pub const MIN_READER_VERSION: u32 = 5;

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

/// V4 top-level file structure.
#[derive(Debug, Clone, Serialize)]
pub struct WaffleFileV4<'a> {
    pub format: &'static str,
    pub version: u32,
    /// See [`MIN_READER_VERSION`]. Old readers ignore this unknown field.
    pub min_reader_version: u32,
    pub document: &'a DocumentMetadata,
    pub sources: &'a [SourceEntry],
    pub tabs: &'a [Tab],
    pub active_tab: &'a str,
    /// Unknown envelope keys, re-emitted (v4 §2.6).
    #[serde(flatten)]
    pub extra: &'a Map<String, Value>,
}

/// Serialize a v4 document to pretty-printed JSON. **The** writer: every
/// production save path composes its bytes here (v4 §4 invariant 7).
pub fn save_document(doc: &WaffleDocument) -> String {
    let file = WaffleFileV4 {
        format: "waffle-iron",
        version: FORMAT_VERSION,
        min_reader_version: MIN_READER_VERSION,
        document: &doc.document,
        sources: &doc.sources,
        tabs: &doc.tabs,
        active_tab: &doc.active_tab,
        extra: &doc.extra,
    };
    serde_json::to_string_pretty(&file).expect("Document serialization should never fail")
}

/// [`save_document`] plus a self-check: never hand out a file the loader
/// would refuse (v4 §4 invariant 8). The known corruption class is non-finite
/// floats — serde_json serializes NaN/∞ as `null`, which every reader then
/// rejects; the round-trip check catches that and any future class of
/// save-side corruption without enumerating float fields.
pub fn save_document_verified(doc: &WaffleDocument) -> Result<String, crate::errors::LoadError> {
    let json = save_document(doc);
    crate::load::load_document(&json)?;
    Ok(json)
}

/// [`save_document_verified`] that does not re-check what it has already
/// checked.
///
/// The self-check above re-parses the WHOLE document on every save, and a
/// host autosaves after every committed tool: on the Eiffel Tower that was
/// 69.6 ms of re-parsing against 5.7 ms to produce the bytes — a 12x tax,
/// paid mostly on tabs that had not changed since the last save verified them
/// (docs/notes/eiffel/FEATURE_NOTES.md §0a).
///
/// A document is its tabs plus a small envelope. This verifier checks each in
/// the same way the loader would, and remembers the tab payloads it has
/// already accepted — a tab whose bytes are byte-for-byte what was verified
/// before cannot have become unparseable. So a save costs a parse of the tabs
/// that changed, not of the document.
#[derive(Debug, Default)]
pub struct SaveVerifier {
    /// Hashes of tab payloads that parsed back cleanly.
    verified: std::collections::HashSet<u64>,
}

/// Bound on remembered payloads: enough for every tab of a document across
/// many edits, small enough that a long session cannot grow it without limit.
const VERIFIED_CAP: usize = 512;

impl SaveVerifier {
    /// Serialize `doc` and verify it, skipping the tabs already known good.
    pub fn save(&mut self, doc: &WaffleDocument) -> Result<String, crate::errors::LoadError> {
        use std::hash::{Hash, Hasher};

        for tab in &doc.tabs {
            // Canonical bytes, via `serde_json::Value` — whose maps are
            // BTreeMaps, so keys come out sorted. Serializing the tab directly
            // would NOT do: the tree holds HashMaps (`body_names`,
            // `provenance`, a sketch's `solved_positions`), so the same tab
            // serializes differently run to run and nothing would ever match
            // the cache.
            let value = serde_json::to_value(tab)
                .map_err(|e| crate::errors::LoadError::ParseError(e.to_string()))?;
            let payload = serde_json::to_string(&value)
                .map_err(|e| crate::errors::LoadError::ParseError(e.to_string()))?;
            let mut hasher = std::collections::hash_map::DefaultHasher::new();
            payload.hash(&mut hasher);
            let key = hasher.finish();
            if self.verified.contains(&key) {
                continue;
            }
            // The loader's own typed parse, on this tab alone.
            serde_json::from_str::<Tab>(&payload)
                .map_err(|e| crate::errors::LoadError::ParseError(e.to_string()))?;
            if self.verified.len() >= VERIFIED_CAP {
                self.verified.clear();
            }
            self.verified.insert(key);
        }

        let json = save_document(doc);
        // Everything that is not a tab, parsed as the loader parses it: the
        // envelope, the document metadata and the sources. `IgnoredAny` skips
        // the tabs, which the loop above accounted for.
        crate::load::check_document_shell(&json)?;
        // Cross-tab rules (active_tab resolves, source ids unique) — the same
        // call `load_document` ends with, and it costs nothing.
        doc.validate()?;
        Ok(json)
    }
}

/// Serialize a single feature tree as a v4 document with one Part tab.
/// Legacy in-feature STEP payloads are lifted into the `sources` table.
pub fn save_project(tree: &FeatureTree, metadata: &ProjectMetadata) -> String {
    save_document(&WaffleDocument::single_part(metadata, tree.clone()))
}

/// [`save_project`] plus the loader self-check (see
/// [`save_document_verified`]). Production single-tree save paths use this.
pub fn save_project_verified(
    tree: &FeatureTree,
    metadata: &ProjectMetadata,
) -> Result<String, crate::errors::LoadError> {
    let json = save_project(tree, metadata);
    crate::load::load_project(&json)?;
    Ok(json)
}
