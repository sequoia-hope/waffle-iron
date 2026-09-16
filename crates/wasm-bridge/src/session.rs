//! The document session: the tab list, every tab's content, the document's
//! metadata, and a monotonic revision.
//!
//! Target-independent, and authoritative. Today the JS store owns all of
//! this (`specs/waffle_server_mode.md` §1.3) and hands it back to Rust on
//! every save; a second host would have to reimplement it. This type is the
//! one place it lives, so the browser worker and a native host drive the
//! same session (§2.3 S2).
//!
//! # The live tree
//!
//! One [`feature_engine::Engine`] holds one feature tree, so only the active
//! tab's tree is *live*. This type keeps every other tab's tree in its
//! [`Tab`], and treats the live tree as authoritative for the active tab:
//! [`DocumentSession::stash_active`] writes it back before the active tab
//! changes or the document is serialized. That is exactly what the JS
//! `switchTab` does today by copying the tree out — moved here and made the
//! rule rather than a call-site convention.
//!
//! # Per-tab history
//!
//! Undo is per tab. The engine's stack follows the live tree: switching
//! parks the outgoing tab's history and restores the incoming tab's. This
//! is a behavior fix — today `SwitchTab` replaces the tree and leaves the
//! stack alone, so an `Undo` after a switch pops a command recorded against
//! the tab you just left.

use std::collections::HashMap;

use serde::{Deserialize, Serialize};

use feature_engine::preview_mesh::PreviewMesh;
use feature_engine::types::FeatureTree;
use feature_engine::undo::UndoStack;
use feature_engine::Engine;
use file_format::{DocumentMetadata, Tab, TabKind, WaffleDocument};

/// Why a session operation was refused. Each maps to an agent error code of
/// the closed set (`specs/waffle_mcp_server.md` §6.1).
#[derive(Debug, Clone, PartialEq, Eq, thiserror::Error)]
pub enum SessionError {
    /// `TabNotFound`: an id the document does not have.
    #[error("the document has no tab with id `{id}`")]
    TabNotFound { id: String },

    /// `TabKindNotSupported`: the operation needs a tab kind this build can
    /// open (a `Part`'s tree, an `Assembly`'s tree).
    #[error("tab `{name}` has kind `{kind}`, which does not {expected}")]
    TabKindNotSupported {
        name: String,
        kind: String,
        expected: String,
    },

    /// A document always has at least one tab.
    #[error("the document's last tab cannot be closed")]
    LastTab,
}

/// A tab's content as the session hands it out, without its tree.
///
/// Serializable because it rides on `ModelUpdated.document` (C2): the tab bar
/// is display data for the UI, never the tab's tree.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct TabInfo {
    pub id: String,
    pub name: String,
    /// `Part`, `Assembly`, or an unknown kind's tag (v4 §2.5).
    pub kind: String,
}

/// The open document: metadata, tabs, which tab is active, and a revision
/// that increments on every committed mutation.
#[derive(Debug)]
pub struct DocumentSession {
    document: DocumentMetadata,
    tabs: Vec<Tab>,
    active_tab: String,
    revision: u64,
    /// Parked undo history of every tab except the active one, whose history
    /// is live in the engine.
    histories: HashMap<String, UndoStack>,
}

impl DocumentSession {
    /// A fresh document with one empty Part tab, as the tab bar starts.
    pub fn new(name: impl Into<String>) -> Self {
        Self::from_document(WaffleDocument::new(name))
    }

    /// Adopt a loaded document. `active_tab` is trusted: the loader has
    /// already refused a dangling one (`WaffleDocument::validate`).
    pub fn from_document(doc: WaffleDocument) -> Self {
        DocumentSession {
            document: doc.document,
            tabs: doc.tabs,
            active_tab: doc.active_tab,
            revision: 0,
            histories: HashMap::new(),
        }
    }

    pub fn document(&self) -> &DocumentMetadata {
        &self.document
    }

    pub fn active_tab_id(&self) -> &str {
        &self.active_tab
    }

    pub fn revision(&self) -> u64 {
        self.revision
    }

    /// Every tab in bar order.
    pub fn tabs(&self) -> Vec<TabInfo> {
        self.tabs
            .iter()
            .map(|t| TabInfo {
                id: t.id.clone(),
                name: t.name.clone(),
                kind: t.kind.type_tag().to_string(),
            })
            .collect()
    }

    pub fn tab(&self, id: &str) -> Option<&Tab> {
        self.tabs.iter().find(|t| t.id == id)
    }

    /// Bump the revision. Every committed mutation names a new document
    /// state, including ones that change no geometry (a rename, a reorder).
    pub fn commit(&mut self) -> u64 {
        self.revision += 1;
        self.revision
    }

    // ── Tabs ────────────────────────────────────────────────────────────

    /// Add a tab of `kind` after the last one and return its id. `name`
    /// defaults to `"Part N"` / `"Assembly N"`, counting existing tabs of
    /// that kind exactly as the tab bar's + buttons do.
    pub fn add_tab(&mut self, kind: &str, name: Option<String>) -> Result<String, SessionError> {
        let tab = match kind {
            "Part" => Tab::part(
                name.unwrap_or_else(|| self.next_name("Part")),
                FeatureTree::new(),
            ),
            "Assembly" => Tab::assembly(
                name.unwrap_or_else(|| self.next_name("Assembly")),
                Default::default(),
            ),
            other => {
                return Err(SessionError::TabKindNotSupported {
                    name: name.unwrap_or_default(),
                    kind: other.to_string(),
                    expected: "name a tab kind this build can create".to_string(),
                })
            }
        };
        let id = tab.id.clone();
        self.tabs.push(tab);
        self.commit();
        Ok(id)
    }

    /// `"{kind} N"`, where N counts the tabs already of that kind.
    fn next_name(&self, kind: &str) -> String {
        let n = self
            .tabs
            .iter()
            .filter(|t| t.kind.type_tag() == kind)
            .count();
        format!("{kind} {}", n + 1)
    }

    /// Remove a tab and its parked history. Returns the tab that should
    /// become active when the closed tab was the active one (the next tab,
    /// or the last if it was last), so the caller can switch to it.
    pub fn close_tab(&mut self, id: &str) -> Result<Option<String>, SessionError> {
        let index = self.index_of(id)?;
        if self.tabs.len() == 1 {
            return Err(SessionError::LastTab);
        }
        self.tabs.remove(index);
        self.histories.remove(id);
        let successor = if self.active_tab == id {
            let next = index.min(self.tabs.len() - 1);
            Some(self.tabs[next].id.clone())
        } else {
            None
        };
        self.commit();
        Ok(successor)
    }

    pub fn rename_tab(&mut self, id: &str, name: impl Into<String>) -> Result<(), SessionError> {
        let index = self.index_of(id)?;
        self.tabs[index].name = name.into();
        self.commit();
        Ok(())
    }

    /// Move a tab to `index` in the bar (0 = first). An index past the end
    /// moves it last, as `tab_move` documents.
    pub fn move_tab(&mut self, id: &str, index: usize) -> Result<(), SessionError> {
        let from = self.index_of(id)?;
        let to = index.min(self.tabs.len() - 1);
        if to != from {
            let tab = self.tabs.remove(from);
            self.tabs.insert(to, tab);
        }
        self.commit();
        Ok(())
    }

    fn index_of(&self, id: &str) -> Result<usize, SessionError> {
        self.tabs
            .iter()
            .position(|t| t.id == id)
            .ok_or_else(|| SessionError::TabNotFound { id: id.to_string() })
    }

    // ── The live tree ───────────────────────────────────────────────────

    /// Write the engine's live tree and history back into the active tab, so
    /// the session's copy is current. Call before the active tab changes and
    /// before serializing.
    ///
    /// An active tab that holds no tree (an `Assembly`, an unknown kind) keeps
    /// its content: the live tree is empty while such a tab is open, and
    /// stamping it in would invent a `features` key the tab never had.
    pub fn stash_active(&mut self, engine: &mut Engine) {
        // Always taken: the outgoing tab's history must not follow the engine
        // to the next tab, even when there is nowhere left to park it (the
        // active tab was just closed — C3's `CloseTab` switches to a
        // successor). Parking it under a dead id would leak a history that
        // outlives its tab.
        let history = engine.take_history();
        let Ok(index) = self.index_of(&self.active_tab.clone()) else {
            return;
        };
        self.histories.insert(self.active_tab.clone(), history);
        if let Some(features) = self.tabs[index].features_mut() {
            *features = engine.tree.clone();
        }
    }

    /// Make `id` active and load its tree and history into the engine.
    /// Stashes the outgoing tab first. The caller rebuilds.
    ///
    /// A tab with no tree of its own (an `Assembly`) leaves the engine's tree
    /// empty — the assembly is evaluated separately, and its instances are
    /// what the renderer shows.
    pub fn switch_tab(&mut self, id: &str, engine: &mut Engine) -> Result<(), SessionError> {
        let index = self.index_of(id)?;
        self.stash_active(engine);
        self.active_tab = self.tabs[index].id.clone();
        engine.tree = self.tabs[index]
            .features()
            .cloned()
            .unwrap_or_else(FeatureTree::new);
        let history = self.histories.remove(id).unwrap_or_default();
        engine.set_history(history);
        self.commit();
        Ok(())
    }

    /// Record a tab's thumbnail. `preview_mesh` is engine-owned but lives in
    /// the tab (it is saved with it), so every tab keeps the last one its
    /// tree produced.
    ///
    /// Takes the engine's preview type and converts: the engine and the file
    /// format declare the same three buffers as two distinct types, and
    /// neither is ours to write a `From` for. JS never had to notice — both
    /// cross the wire as JSON.
    pub fn set_preview_mesh(&mut self, id: &str, mesh: Option<PreviewMesh>) {
        let Ok(index) = self.index_of(id) else {
            return;
        };
        let stored = mesh.map(|m| file_format::PreviewMesh {
            vertices: m.vertices,
            normals: m.normals,
            indices: m.indices,
        });
        match &mut self.tabs[index].kind {
            TabKind::Part { preview_mesh, .. } => *preview_mesh = stored,
            TabKind::Assembly { preview_mesh, .. } => *preview_mesh = stored,
            TabKind::Unknown(_) => {}
        }
    }

    // ── Assemblies ──────────────────────────────────────────────────────

    /// Replace a `Part` tab's stored tree.
    ///
    /// The ACTIVE tab's tree is the live one in the engine — this writes the
    /// copy the session keeps, so setting it on the active tab is overwritten
    /// by the next stash. Use it for the tabs that are not open.
    pub fn set_features(&mut self, id: &str, features: FeatureTree) -> Result<(), SessionError> {
        let index = self.index_of(id)?;
        let tab = &mut self.tabs[index];
        match tab.features_mut() {
            Some(slot) => *slot = features,
            None => {
                return Err(SessionError::TabKindNotSupported {
                    name: tab.name.clone(),
                    kind: tab.kind.type_tag().to_string(),
                    expected: "hold a feature tree".to_string(),
                })
            }
        }
        self.commit();
        Ok(())
    }

    /// Record the solved placements on an `Assembly` tab (S2 C3c).
    ///
    /// Placements are DERIVED — the engine solves them on every evaluation —
    /// but they are saved with the tab (v4 §2.5), and the session is what
    /// composes the file now. Without this the store's copy would be the only
    /// one that ever had them, and a saved assembly would reopen unplaced.
    /// Silent for a tab that holds no assembly: this rides on an evaluation,
    /// not on a user action.
    pub fn set_assembly_placements(
        &mut self,
        id: &str,
        placements: std::collections::BTreeMap<uuid::Uuid, feature_engine::assembly::Transform>,
    ) {
        let Ok(index) = self.index_of(id) else {
            return;
        };
        if let TabKind::Assembly { assembly, .. } = &mut self.tabs[index].kind {
            assembly.placements = placements;
        }
    }

    /// Replace an `Assembly` tab's tree, as the assembly panel's edits do.
    pub fn set_assembly(
        &mut self,
        id: &str,
        assembly: feature_engine::assembly::AssemblyTree,
    ) -> Result<(), SessionError> {
        let index = self.index_of(id)?;
        let tab = &mut self.tabs[index];
        match &mut tab.kind {
            TabKind::Assembly { assembly: slot, .. } => *slot = assembly,
            other => {
                return Err(SessionError::TabKindNotSupported {
                    name: tab.name.clone(),
                    kind: other.type_tag().to_string(),
                    expected: "hold an assembly".to_string(),
                })
            }
        }
        self.commit();
        Ok(())
    }

    /// An `Assembly` tab's tree, refused loudly for a tab of any other kind.
    ///
    /// This is what `OpenAssembly` evaluates (S2 C3b): the tab's content lives
    /// here, so the message names the tab instead of carrying its assembly.
    pub fn assembly(
        &self,
        id: &str,
    ) -> Result<&feature_engine::assembly::AssemblyTree, SessionError> {
        let tab = self
            .tab(id)
            .ok_or_else(|| SessionError::TabNotFound { id: id.to_string() })?;
        tab.assembly_tree()
            .ok_or_else(|| SessionError::TabKindNotSupported {
                name: tab.name.clone(),
                kind: tab.kind.type_tag().to_string(),
                expected: "hold an assembly".to_string(),
            })
    }

    /// Every `Part` tab's tree, keyed by tab id — what evaluating an assembly
    /// needs. The active tab contributes the LIVE tree, so an assembly always
    /// builds its parts from what is on screen.
    ///
    /// This is the payload the JS store re-sends on every assembly
    /// evaluation today (`refreshAssembly`); the session already has it.
    pub fn part_trees(&self, engine: &Engine) -> HashMap<String, FeatureTree> {
        self.tabs
            .iter()
            .filter_map(|t| {
                let tree = if t.id == self.active_tab {
                    // Only if this tab is one that holds a tree at all.
                    t.features().map(|_| engine.tree.clone())
                } else {
                    t.features().cloned()
                };
                tree.map(|tree| (t.id.clone(), tree))
            })
            .collect()
    }

    /// Every `Assembly` tab's tree, keyed by tab id, so an instance may be of
    /// a sub-assembly.
    pub fn assembly_trees(&self) -> HashMap<String, feature_engine::assembly::AssemblyTree> {
        self.tabs
            .iter()
            .filter_map(|t| t.assembly_tree().map(|a| (t.id.clone(), a.clone())))
            .collect()
    }

    // ── Metadata and serialization ──────────────────────────────────────

    /// Set the document's name, display unit, identity, creation time, or any
    /// subset. `modified` is stamped by [`DocumentSession::to_document`] at
    /// save time, not here.
    ///
    /// `id` and `created` arrive from the UI rather than being minted here: the
    /// storage record is keyed by the document's own identity (v4 §4 inv. 1,
    /// P2-5), so the id has to be the one the host already filed the document
    /// under, and `created` is preserved from the file it was opened from.
    pub fn set_meta(
        &mut self,
        name: Option<String>,
        display_unit: Option<String>,
        id: Option<uuid::Uuid>,
        created: Option<chrono::DateTime<chrono::Utc>>,
    ) {
        if let Some(name) = name {
            self.document.name = name;
        }
        if let Some(unit) = display_unit {
            self.document.display_unit = Some(unit);
        }
        if let Some(id) = id {
            self.document.id = id;
        }
        if let Some(created) = created {
            self.document.created = created;
        }
        self.commit();
    }

    /// The document as it would be saved: the live tree stashed into the
    /// active tab, `modified` stamped, `sources` supplied by the caller (the
    /// engine's source store owns their content, not the session).
    ///
    /// The one writer stays `file_format::save_document_verified` (v4 §4
    /// inv. 7); this only composes the value handed to it.
    pub fn to_document(
        &mut self,
        engine: &mut Engine,
        sources: Vec<file_format::SourceEntry>,
        modified: chrono::DateTime<chrono::Utc>,
        envelope_extra: serde_json::Map<String, serde_json::Value>,
    ) -> WaffleDocument {
        self.stash_active(engine);
        // stash_active parked the history; the tab is still the active one,
        // so give it straight back rather than leaving the engine empty.
        if let Some(history) = self.histories.remove(&self.active_tab) {
            engine.set_history(history);
        }
        let mut document = self.document.clone();
        document.modified = modified;
        WaffleDocument {
            document,
            sources,
            tabs: self.tabs.clone(),
            active_tab: self.active_tab.clone(),
            extra: envelope_extra,
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn session() -> DocumentSession {
        DocumentSession::new("Doc")
    }

    fn engine() -> Engine {
        Engine::new()
    }

    /// A tree distinguishable from every other by its rollback index — the
    /// one field a bare `FeatureTree` lets us set without building geometry.
    fn marked(n: usize) -> FeatureTree {
        let mut tree = FeatureTree::new();
        tree.active_index = Some(n);
        tree
    }

    #[test]
    fn a_new_session_has_one_active_part_tab() {
        let s = session();
        let tabs = s.tabs();
        assert_eq!(tabs.len(), 1);
        assert_eq!(tabs[0].kind, "Part");
        assert_eq!(tabs[0].name, "Part 1");
        assert_eq!(s.active_tab_id(), tabs[0].id);
        assert_eq!(s.revision(), 0);
    }

    #[test]
    fn tabs_are_named_by_kind_and_count_like_the_tab_bar() {
        let mut s = session();
        s.add_tab("Part", None).unwrap();
        s.add_tab("Assembly", None).unwrap();
        s.add_tab("Assembly", None).unwrap();
        s.add_tab("Part", Some("Bracket".into())).unwrap();
        let names: Vec<_> = s.tabs().into_iter().map(|t| t.name).collect();
        assert_eq!(
            names,
            ["Part 1", "Part 2", "Assembly 1", "Assembly 2", "Bracket"]
        );
    }

    #[test]
    fn every_mutation_bumps_the_revision() {
        let mut s = session();
        let mut e = engine();
        let before = s.revision();
        let id = s.add_tab("Part", None).unwrap();
        s.rename_tab(&id, "Renamed").unwrap();
        s.move_tab(&id, 0).unwrap();
        s.switch_tab(&id, &mut e).unwrap();
        s.set_meta(Some("New name".into()), None, None, None);
        // add, rename, move, switch, set_meta.
        assert_eq!(s.revision(), before + 5);
    }

    #[test]
    fn an_unknown_tab_id_is_refused_by_every_tab_operation() {
        let mut s = session();
        let mut e = engine();
        let missing = "no-such-tab";
        let expected = SessionError::TabNotFound { id: missing.into() };
        assert_eq!(s.rename_tab(missing, "x").unwrap_err(), expected);
        assert_eq!(s.move_tab(missing, 0).unwrap_err(), expected);
        assert_eq!(s.close_tab(missing).unwrap_err(), expected);
        assert_eq!(s.switch_tab(missing, &mut e).unwrap_err(), expected);
    }

    #[test]
    fn the_last_tab_cannot_be_closed() {
        let mut s = session();
        let only = s.tabs()[0].id.clone();
        assert_eq!(s.close_tab(&only).unwrap_err(), SessionError::LastTab);
    }

    #[test]
    fn moving_past_the_end_moves_last() {
        let mut s = session();
        let first = s.tabs()[0].id.clone();
        s.add_tab("Part", None).unwrap();
        s.add_tab("Part", None).unwrap();
        s.move_tab(&first, 99).unwrap();
        assert_eq!(s.tabs().last().unwrap().id, first);
    }

    #[test]
    fn closing_the_active_tab_names_its_successor() {
        let mut s = session();
        let first = s.tabs()[0].id.clone();
        let second = s.add_tab("Part", None).unwrap();
        let third = s.add_tab("Part", None).unwrap();

        // Closing an inactive tab names no successor.
        assert_eq!(s.close_tab(&third).unwrap(), None);
        // Closing the active one names the tab that takes its place.
        assert_eq!(s.active_tab_id(), first);
        assert_eq!(s.close_tab(&first).unwrap(), Some(second));
    }

    #[test]
    fn switching_carries_each_tabs_tree_with_it() {
        let mut s = session();
        let mut e = engine();
        let first = s.tabs()[0].id.clone();
        let second = s.add_tab("Part", None).unwrap();

        e.tree = marked(1);
        s.switch_tab(&second, &mut e).unwrap();
        // The second tab is empty, and the first tab kept its tree.
        assert_eq!(e.tree.active_index, None);
        e.tree = marked(2);

        s.switch_tab(&first, &mut e).unwrap();
        assert_eq!(e.tree.active_index, Some(1));
        s.switch_tab(&second, &mut e).unwrap();
        assert_eq!(e.tree.active_index, Some(2));
    }

    #[test]
    fn an_assembly_tab_leaves_the_live_tree_empty_and_gains_no_features_key() {
        let mut s = session();
        let mut e = engine();
        let asm = s.add_tab("Assembly", None).unwrap();

        e.tree = marked(3);
        s.switch_tab(&asm, &mut e).unwrap();
        assert_eq!(e.tree.active_index, None);

        // Leaving the assembly must not stamp the empty live tree onto it —
        // the JS `switchTab` does exactly that today, inventing a `features`
        // key on an Assembly tab.
        let first = s.tabs()[0].id.clone();
        s.switch_tab(&first, &mut e).unwrap();
        assert!(s.tab(&asm).unwrap().features().is_none());
        assert!(matches!(
            s.tab(&asm).unwrap().kind,
            TabKind::Assembly { .. }
        ));
        // …and the tab we returned to still has its own tree.
        assert_eq!(e.tree.active_index, Some(3));
    }

    #[test]
    fn history_follows_the_tab_not_the_engine() {
        let mut s = session();
        let mut e = engine();
        let first = s.tabs()[0].id.clone();
        let second = s.add_tab("Part", None).unwrap();

        // A command recorded against the first tab.
        e.set_history({
            let mut h = UndoStack::new();
            h.push(feature_engine::undo::Command::RenameBody {
                body_id: "b".into(),
                old_name: None,
                new_name: Some("x".into()),
            });
            h
        });
        assert!(e.can_undo());

        // The second tab has its own, empty history.
        s.switch_tab(&second, &mut e).unwrap();
        assert!(
            !e.can_undo(),
            "undo after a switch must not reach the tab we left"
        );

        // Coming back restores it.
        s.switch_tab(&first, &mut e).unwrap();
        assert!(e.can_undo());
    }

    #[test]
    fn part_trees_take_the_active_tab_from_the_live_engine() {
        let mut s = session();
        let mut e = engine();
        let first = s.tabs()[0].id.clone();
        let second = s.add_tab("Part", None).unwrap();
        s.add_tab("Assembly", None).unwrap();

        s.switch_tab(&second, &mut e).unwrap();
        e.tree = marked(7);

        let trees = s.part_trees(&e);
        // Both Part tabs, and only they.
        assert_eq!(trees.len(), 2);
        // The active tab's entry is the live tree, not the stale stored one.
        assert_eq!(trees[&second].active_index, Some(7));
        assert_eq!(trees[&first].active_index, None);
        assert_eq!(s.assembly_trees().len(), 1);
    }

    #[test]
    fn to_document_stashes_the_live_tree_and_keeps_the_history() {
        let mut s = session();
        let mut e = engine();
        e.tree = marked(4);
        e.set_history({
            let mut h = UndoStack::new();
            h.push(feature_engine::undo::Command::RenameBody {
                body_id: "b".into(),
                old_name: None,
                new_name: Some("x".into()),
            });
            h
        });

        let doc = s.to_document(&mut e, Vec::new(), chrono::Utc::now(), Default::default());
        assert_eq!(doc.tabs.len(), 1);
        assert_eq!(doc.tabs[0].features().unwrap().active_index, Some(4));
        assert_eq!(doc.active_tab, s.active_tab_id());
        // Saving is not a tab switch: the active tab keeps its undo history.
        assert!(e.can_undo(), "saving must not discard the undo history");
    }

    #[test]
    fn a_loaded_document_round_trips_through_the_session() {
        let mut s = session();
        let mut e = engine();
        s.add_tab("Part", Some("Second".into())).unwrap();
        // The identity and creation time are the host's (S2 C3c); they must
        // survive the round trip untouched, because the storage record is
        // keyed by the identity.
        let id = uuid::Uuid::new_v4();
        let created = chrono::DateTime::parse_from_rfc3339("2021-02-03T04:05:06Z")
            .expect("a fixed timestamp")
            .with_timezone(&chrono::Utc);
        s.set_meta(
            Some("Named".into()),
            Some("in".into()),
            Some(id),
            Some(created),
        );
        let doc = s.to_document(&mut e, Vec::new(), chrono::Utc::now(), Default::default());

        let mut reopened = DocumentSession::from_document(doc);
        assert_eq!(reopened.document().name, "Named");
        assert_eq!(reopened.document().display_unit.as_deref(), Some("in"));
        assert_eq!(reopened.document().id, id);
        assert_eq!(reopened.document().created, created);
        assert_eq!(
            reopened
                .tabs()
                .into_iter()
                .map(|t| t.name)
                .collect::<Vec<_>>(),
            ["Part 1", "Second"]
        );
        assert_eq!(reopened.active_tab_id(), s.active_tab_id());
        assert_eq!(reopened.revision(), 0, "a reopened document starts at 0");
        // And it is usable: the reopened session still switches tabs.
        let second = reopened.tabs()[1].id.clone();
        reopened.switch_tab(&second, &mut e).unwrap();
        assert_eq!(reopened.active_tab_id(), second);
    }

    #[test]
    fn set_assembly_refuses_a_part_tab() {
        let mut s = session();
        let part = s.tabs()[0].id.clone();
        let err = s.set_assembly(&part, Default::default()).unwrap_err();
        assert!(matches!(err, SessionError::TabKindNotSupported { .. }));
    }

    #[test]
    fn a_preview_mesh_is_kept_per_tab() {
        let mut s = session();
        let mesh = PreviewMesh {
            vertices: vec![0.0, 0.0, 0.0],
            normals: vec![0.0, 0.0, 1.0],
            indices: vec![0],
        };
        let part = s.tabs()[0].id.clone();
        s.set_preview_mesh(&part, Some(mesh));
        match &s.tab(&part).unwrap().kind {
            TabKind::Part { preview_mesh, .. } => assert!(preview_mesh.is_some()),
            other => panic!("{other:?}"),
        }
        // An id the document does not have is ignored, not a panic.
        s.set_preview_mesh("no-such-tab", None);
    }
}
