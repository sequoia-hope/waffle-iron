//! The in-memory v4 document (`specs/waffle_v4_document_model.md` §2):
//! metadata + sources + tabs, plus the two shims that keep the single-tree
//! API honest — lifting legacy in-feature STEP payloads into `sources`, and
//! inlining them back for consumers that have no source store.

use feature_engine::types::{FeatureTree, Operation};
use serde_json::{Map, Value};
use uuid::Uuid;

use crate::errors::LoadError;
use crate::hash::{check_content_hash, git_blob_sha1, HashCheck};
use crate::metadata::{DocumentMetadata, ProjectMetadata, Tab, TabKind};
use crate::sources::{Embed, SourceEntry, SourceKind};

/// A loaded/loadable v4 document.
#[derive(Debug, Clone)]
pub struct WaffleDocument {
    pub document: DocumentMetadata,
    pub sources: Vec<SourceEntry>,
    pub tabs: Vec<Tab>,
    pub active_tab: String,
    /// Unknown envelope keys preserved across load → save (§2.6).
    pub extra: Map<String, Value>,
}

/// `load_document`'s result: the document plus non-fatal findings (missing
/// `document.id`, unresolvable locators, embed hash mismatches, …).
#[derive(Debug, Clone)]
pub struct LoadedDocument {
    pub document: WaffleDocument,
    pub warnings: Vec<String>,
}

impl WaffleDocument {
    /// A fresh document with one empty Part tab.
    pub fn new(name: impl Into<String>) -> Self {
        let tab = Tab::part("Part 1", FeatureTree::new());
        let active_tab = tab.id.clone();
        WaffleDocument {
            document: DocumentMetadata::new(name),
            sources: Vec::new(),
            tabs: vec![tab],
            active_tab,
            extra: Map::new(),
        }
    }

    /// The single-tree shape: one Part tab holding `tree`, legacy in-feature
    /// payloads lifted into `sources`.
    pub fn single_part(metadata: &ProjectMetadata, tree: FeatureTree) -> Self {
        let tab = Tab::part("Part 1", tree);
        let active_tab = tab.id.clone();
        let mut doc = WaffleDocument {
            document: DocumentMetadata::from(metadata),
            sources: Vec::new(),
            tabs: vec![tab],
            active_tab,
            extra: Map::new(),
        };
        let _ = doc.lift_inline_payloads();
        doc
    }

    pub fn active_tab(&self) -> Option<&Tab> {
        self.tabs.iter().find(|t| t.id == self.active_tab)
    }

    pub fn tab(&self, id: &str) -> Option<&Tab> {
        self.tabs.iter().find(|t| t.id == id)
    }

    pub fn source(&self, id: Uuid) -> Option<&SourceEntry> {
        self.sources.iter().find(|s| s.id == id)
    }

    pub fn source_mut(&mut self, id: Uuid) -> Option<&mut SourceEntry> {
        self.sources.iter_mut().find(|s| s.id == id)
    }

    /// Sources that must not leave this browser as they are (§3): a `Local`
    /// locator without `pack`.
    pub fn unshareable_sources(&self) -> Vec<&SourceEntry> {
        self.sources
            .iter()
            .filter(|s| !s.locator.is_shareable() && !s.effective_pack())
            .collect()
    }

    /// Structural validation (§3/§6): duplicate source ids and a dangling
    /// `active_tab` are errors; everything else is a warning.
    pub fn validate(&self) -> Result<Vec<String>, LoadError> {
        let mut seen = std::collections::HashSet::new();
        for s in &self.sources {
            if !seen.insert(s.id) {
                return Err(LoadError::ParseError(format!(
                    "duplicate source id {}",
                    s.id
                )));
            }
        }
        if !self.tabs.iter().any(|t| t.id == self.active_tab) {
            return Err(LoadError::ParseError(
                "active_tab references non-existent tab".to_string(),
            ));
        }
        let mut warnings: Vec<String> = self.sources.iter().flat_map(|s| s.validate()).collect();
        for t in &self.tabs {
            if let TabKind::Unknown(_) = &t.kind {
                warnings.push(format!(
                    "tab `{}` ({}): unknown tab kind `{}` — preserved, not editable in this version",
                    t.name,
                    t.id,
                    t.kind.type_tag()
                ));
            }
            if let Some(drawing) = t.drawing_tree() {
                warnings.extend(
                    drawing
                        .validate()
                        .into_iter()
                        .map(|w| format!("tab `{}` ({}): {w}", t.name, t.id)),
                );
                // A view's source tab must exist in this document. Same
                // check, and the same "warning, never a load failure", as an
                // assembly instance's part below: a drawing of a tab someone
                // deleted is a document worth opening and telling about.
                for sheet in &drawing.sheets {
                    for view in &sheet.views {
                        if !self.tabs.iter().any(|x| x.id == view.source.tab_id) {
                            warnings.push(format!(
                                "tab `{}` ({}): view `{}` draws tab `{}`, which this document \
                                 does not have",
                                t.name, t.id, view.name, view.source.tab_id
                            ));
                        }
                    }
                }
            }
            if let Some(asm) = t.assembly_tree() {
                warnings.extend(
                    asm.validate()
                        .into_iter()
                        .map(|w| format!("tab `{}` ({}): {w}", t.name, t.id)),
                );
                for i in &asm.instances {
                    let ok = match i.source.source_id {
                        None => self.tabs.iter().any(|x| x.id == i.source.tab_id),
                        Some(sid) => self.source(sid).is_some(),
                    };
                    if !ok {
                        warnings.push(format!(
                            "tab `{}` ({}): instance `{}` ({}) references {} `{}`, which this document does not have",
                            t.name,
                            t.id,
                            i.name,
                            i.id,
                            if i.source.source_id.is_some() { "source" } else { "tab" },
                            i.source.source_id.map(|s| s.to_string()).unwrap_or_else(|| i.source.tab_id.clone())
                        ));
                    }
                }
            }
            for f in t
                .features()
                .map(|tree| tree.features.as_slice())
                .unwrap_or(&[])
            {
                if let Operation::Unknown(_) = &f.operation {
                    warnings.push(format!(
                        "feature `{}` ({}) in tab `{}`: unknown operation kind `{}` — preserved, not rebuildable in this version",
                        f.name,
                        f.id,
                        t.name,
                        f.operation.type_tag()
                    ));
                }
            }
        }
        for s in &self.sources {
            if let Some(embed) = &s.embed {
                if let Ok(text) = embed.decode() {
                    if let HashCheck::Mismatch { recorded, actual } =
                        check_content_hash(s.content_hash.as_deref(), text.as_bytes())
                    {
                        warnings.push(format!(
                            "source `{}` ({}): EmbedHashMismatch — embed hashes to {actual}, entry records {recorded}; the embed is ignored",
                            s.name, s.id
                        ));
                    }
                }
            }
        }
        Ok(warnings)
    }

    /// Move every legacy in-feature STEP payload (v3 `ImportedBody.blob`) in
    /// every Part tab into `sources` (`Embedded`, packed, hashed), setting
    /// `source_id` and clearing the blob. Byte-identical payloads share one
    /// source. A payload that fails to decode stays inline (it will fail
    /// loudly at rebuild, as before) and is reported.
    pub fn lift_inline_payloads(&mut self) -> Vec<String> {
        let mut warnings = Vec::new();
        for tab in &mut self.tabs {
            if let Some(tree) = tab.features_mut() {
                warnings.extend(lift_inline_payloads(tree, &mut self.sources));
            }
        }
        warnings
    }

    /// Inverse shim for the single-tree API: give each `ImportedBody` that
    /// names a source with a hash-verified embed its legacy inline blob, so
    /// a consumer with no [`feature_engine::sources::SourceStore`] still
    /// rebuilds. Unverifiable (no hash) embeds are inlined; mismatching ones
    /// are not (the feature then fails `SourceUnavailable`, loudly).
    pub fn inline_payloads_into(&self, tree: &mut FeatureTree) -> Vec<String> {
        inline_payloads(tree, &self.sources)
    }

    /// Decoded text of every source that has a usable embed (hash-verified or
    /// unverifiable), for registering into a source store.
    pub fn embedded_contents(&self) -> Vec<(Uuid, String)> {
        self.sources
            .iter()
            .filter_map(|s| {
                let text = s.embed.as_ref()?.decode().ok()?;
                match check_content_hash(s.content_hash.as_deref(), text.as_bytes()) {
                    HashCheck::Mismatch { .. } => None,
                    _ => Some((s.id, text)),
                }
            })
            .collect()
    }
}

/// See [`WaffleDocument::lift_inline_payloads`].
pub fn lift_inline_payloads(tree: &mut FeatureTree, sources: &mut Vec<SourceEntry>) -> Vec<String> {
    let mut warnings = Vec::new();
    for feature in &mut tree.features {
        let Operation::ImportedBody { params } = &mut feature.operation else {
            continue;
        };
        let Some(blob) = params.blob.clone() else {
            continue;
        };
        let encoding = params
            .blob_encoding
            .clone()
            .unwrap_or_else(|| step_import::STEP_BLOB_ENCODING.to_string());
        let text = match step_import::decode_step_blob(&encoding, &blob) {
            Ok(t) => t,
            Err(e) => {
                warnings.push(format!(
                    "feature `{}`: inline STEP payload left in place, it does not decode: {e}",
                    feature.name
                ));
                continue;
            }
        };
        let hash = git_blob_sha1(text.as_bytes());
        let id = match sources.iter().find(|s| {
            s.content_hash.as_deref() == Some(hash.as_str()) && s.kind == SourceKind::Step
        }) {
            Some(existing) => existing.id,
            None => {
                let mut entry =
                    SourceEntry::embedded(params.file_name.clone(), SourceKind::Step, "");
                entry.content_hash = Some(hash);
                // Keep the bytes exactly as the feature carried them.
                entry.embed = Some(Embed::from_encoded(&encoding, &blob));
                sources.push(entry);
                sources.last().map(|s| s.id).expect("just pushed")
            }
        };
        params.source_id = Some(id);
        params.blob = None;
        params.blob_encoding = None;
    }
    warnings
}

/// See [`WaffleDocument::inline_payloads_into`].
pub fn inline_payloads(tree: &mut FeatureTree, sources: &[SourceEntry]) -> Vec<String> {
    let mut warnings = Vec::new();
    for feature in &mut tree.features {
        let Operation::ImportedBody { params } = &mut feature.operation else {
            continue;
        };
        if params.blob.is_some() {
            continue;
        }
        let Some(id) = params.source_id else {
            continue;
        };
        let Some(source) = sources.iter().find(|s| s.id == id) else {
            warnings.push(format!(
                "feature `{}`: source {id} is not in the document's sources table",
                feature.name
            ));
            continue;
        };
        let Some(embed) = &source.embed else {
            continue; // linked, not packed: the host must provide it
        };
        let text = match embed.decode() {
            Ok(t) => t,
            Err(e) => {
                warnings.push(format!(
                    "source `{}` ({id}): embed does not decode: {e}",
                    source.name
                ));
                continue;
            }
        };
        if let HashCheck::Mismatch { recorded, actual } =
            check_content_hash(source.content_hash.as_deref(), text.as_bytes())
        {
            warnings.push(format!(
                "source `{}` ({id}): EmbedHashMismatch ({actual} vs recorded {recorded}); not inlined",
                source.name
            ));
            continue;
        }
        params.blob_encoding = Some(embed.encoding.clone());
        params.blob = Some(embed.blob.clone());
    }
    warnings
}
