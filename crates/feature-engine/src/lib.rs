pub mod assembly;
pub mod assembly_solver;
pub mod connector;
pub mod context;
pub mod expr;
pub mod opaque;
pub mod params;
pub mod pattern;
pub mod preview_mesh;
pub mod progress;
pub mod rebuild;
pub mod resolve;
pub mod script;
pub mod share_a_face;
pub mod sources;
pub mod tree;
pub mod types;
pub mod undo;
pub mod union_all;

use std::collections::HashMap;
use uuid::Uuid;

use modeling_ops::{KernelBundle, OpResult};

use crate::sources::SourceStore;
use crate::types::{
    EngineError, ErrorKind, Feature, FeatureError, FeatureTree, Operation, Provenance,
};
use crate::undo::{Command, UndoStack};
use waffle_types::{Anchor, OutputKey};

/// The parametric modeling engine.
///
/// Manages the feature tree, holds the kernel instance, and coordinates
/// rebuilds with GeomRef resolution.
pub struct Engine {
    /// The feature tree.
    pub tree: FeatureTree,
    /// Cached results from the last rebuild.
    pub feature_results: HashMap<Uuid, OpResult>,
    /// Warnings from the last rebuild.
    pub warnings: Vec<String>,
    /// Errors from the last rebuild.
    pub errors: Vec<(Uuid, String)>,
    /// The same errors, typed (`specs/waffle_mcp_server.md` ICR-2):
    /// `feature_errors[i]` describes `errors[i]`.
    pub feature_errors: Vec<FeatureError>,
    /// Feature IDs consumed by a later boolean (should not be rendered).
    pub consumed_features: std::collections::HashSet<Uuid>,
    /// Which feature consumed which (consumer → consumed, in target order),
    /// so name inheritance can find a consumer's FIRST target after the
    /// rebuild (`consumed_features` alone has lost that).
    pub consumed_by: HashMap<Uuid, Vec<Uuid>>,
    /// The last rebuild's feature errors (not expression or context errors): a
    /// failed feature the next rebuild does not re-execute reports these again.
    rebuild_errors: Vec<FeatureError>,
    /// KV13 F6: persistent-id → the feature that INTRODUCED it (recomputed each
    /// rebuild). The basis for resolving a face's *creating* feature through
    /// chained booleans — see [`Engine::created_by_feature`].
    pub pid_to_feature: HashMap<u64, Uuid>,
    /// Transient (NOT persisted) inherited body names, keyed by body id. When a
    /// boolean/merge consumes a target body that has a custom name, the result
    /// body inherits it. Recomputed on every rebuild and rename.
    inherited_body_names: HashMap<String, String>,
    /// Content of the document's external sources (v4 `sources` table),
    /// keyed by source id. Document-scoped: not part of the tree, not
    /// undoable, survives tab switches. See [`crate::sources::SourceStore`].
    pub sources: SourceStore,
    /// The assembly context this part is being edited in (v4 §2.8, in-context
    /// editing), if any. Runtime-only: set by the host when the part is opened
    /// in context, dropped on tab switch. Scoped `GeomRef`s resolve through it;
    /// see [`context::EditContext`].
    pub context: Option<context::EditContext>,
    /// The part's named mate connectors (`MateConnector` features) as the
    /// last rebuild evaluated them, in part coordinates. Recomputed every
    /// rebuild; what an assembly's connectors on this part draw on.
    pub connectors: Vec<connector::PartConnector>,
    /// Undo/redo history.
    undo_stack: UndoStack,
}

impl Engine {
    /// Create a new engine.
    pub fn new() -> Self {
        Self {
            tree: FeatureTree::new(),
            feature_results: HashMap::new(),
            warnings: Vec::new(),
            errors: Vec::new(),
            feature_errors: Vec::new(),
            consumed_features: std::collections::HashSet::new(),
            consumed_by: HashMap::new(),
            rebuild_errors: Vec::new(),
            pid_to_feature: HashMap::new(),
            inherited_body_names: HashMap::new(),
            sources: SourceStore::new(),
            context: None,
            connectors: Vec::new(),
            undo_stack: UndoStack::new(),
        }
    }

    /// Record (or clear) a feature's provenance (v4 §2.7). Not undoable and
    /// no rebuild: provenance is metadata about authorship, not geometry.
    pub fn set_provenance(
        &mut self,
        feature_id: Uuid,
        provenance: Option<Provenance>,
    ) -> Result<Option<Provenance>, EngineError> {
        if self.tree.find_feature(feature_id).is_none() {
            return Err(EngineError::FeatureNotFound { id: feature_id });
        }
        Ok(self.tree.set_provenance(feature_id, provenance))
    }

    /// KV13 F6: the feature that *introduced* a face's geometry — through
    /// chained booleans, not the last boolean. Resolves the face's lineage
    /// root (via `face_provenance`) to the feature that created that root.
    /// `None` if the kernel does not track persistent ids, or the root's
    /// feature is unknown (e.g. produced before an incremental rebuild point).
    pub fn created_by_feature(
        &self,
        introspect: &dyn waffle_types::kernel::KernelIntrospect,
        face: waffle_types::kernel::KernelId,
    ) -> Option<Uuid> {
        let prov = introspect.face_provenance(face)?;
        self.pid_to_feature.get(&prov.root_pid).copied()
    }

    /// Add a feature and rebuild.
    pub fn add_feature(
        &mut self,
        name: String,
        operation: Operation,
        kb: &mut dyn KernelBundle,
    ) -> Result<Uuid, EngineError> {
        self.add_feature_with_provenance(name, operation, None, kb)
    }

    /// Add a feature with its provenance record in the SAME undo step
    /// (`specs/waffle_mcp_server.md` ICR-4): undo removes the record with the
    /// feature and redo restores both, so an agent's call undoes exactly.
    pub fn add_feature_with_provenance(
        &mut self,
        name: String,
        operation: Operation,
        provenance: Option<Provenance>,
        kb: &mut dyn KernelBundle,
    ) -> Result<Uuid, EngineError> {
        let id = self.tree.add_feature(name, operation);
        let position = self.tree.feature_index(id).unwrap_or(0);
        let feature = Box::new(self.tree.find_feature(id).unwrap().clone());
        self.tree.restore_provenance(id, provenance.clone());
        self.undo_stack.push(Command::AddFeature {
            feature,
            position,
            provenance,
        });
        self.rebuild(kb, position, changed_feature(id));
        Ok(id)
    }

    /// Remove a feature and rebuild.
    pub fn remove_feature(
        &mut self,
        id: Uuid,
        kb: &mut dyn KernelBundle,
    ) -> Result<(), EngineError> {
        let pos = self
            .tree
            .feature_index(id)
            .ok_or(EngineError::FeatureNotFound { id })?;
        let feature = Box::new(self.tree.find_feature(id).unwrap().clone());
        self.tree.remove_feature(id)?;
        self.feature_results.remove(&id);
        // GC body-name overrides owned by the deleted feature (feature-delete
        // only — never on a transient empty rebuild), capturing them for undo.
        let removed_body_names = self.tree.take_body_names(id);
        let removed_provenance = self.tree.take_provenance(id);
        self.undo_stack.push(Command::RemoveFeature {
            feature,
            position: pos,
            removed_body_names,
            removed_provenance,
        });
        self.rebuild(
            kb,
            pos.min(self.tree.features.len().saturating_sub(1)),
            changed_feature(id),
        );
        Ok(())
    }

    /// Edit a feature's operation and rebuild from that point.
    pub fn edit_feature(
        &mut self,
        id: Uuid,
        operation: Operation,
        kb: &mut dyn KernelBundle,
    ) -> Result<(), EngineError> {
        self.edit_feature_with_provenance(id, operation, None, kb)
    }

    /// Edit a feature and, when `provenance` is given, replace its provenance
    /// record in the same undo step (ICR-4). `None` leaves the record as is.
    pub fn edit_feature_with_provenance(
        &mut self,
        id: Uuid,
        operation: Operation,
        provenance: Option<Provenance>,
        kb: &mut dyn KernelBundle,
    ) -> Result<(), EngineError> {
        let pos = self
            .tree
            .feature_index(id)
            .ok_or(EngineError::FeatureNotFound { id })?;

        let feature = self
            .tree
            .find_feature_mut(id)
            .ok_or(EngineError::FeatureNotFound { id })?;
        let old_operation = feature.operation.clone();
        feature.operation = operation.clone();
        let provenance = provenance.map(|new| {
            let old = self.tree.set_provenance(id, Some(new.clone()));
            (old, new)
        });

        self.undo_stack.push(Command::EditFeature {
            feature_id: id,
            old_operation: Box::new(old_operation),
            new_operation: Box::new(operation),
            provenance,
        });

        self.rebuild(kb, pos, changed_feature(id));
        Ok(())
    }

    /// Suppress/unsuppress a feature and rebuild.
    pub fn set_suppressed(
        &mut self,
        id: Uuid,
        suppressed: bool,
        kb: &mut dyn KernelBundle,
    ) -> Result<(), EngineError> {
        let pos = self
            .tree
            .feature_index(id)
            .ok_or(EngineError::FeatureNotFound { id })?;
        let old_suppressed = self.tree.find_feature(id).unwrap().suppressed;
        self.tree.set_suppressed(id, suppressed)?;
        self.undo_stack.push(Command::SuppressFeature {
            feature_id: id,
            old_suppressed,
            new_suppressed: suppressed,
        });
        self.rebuild(kb, pos, changed_feature(id));
        Ok(())
    }

    /// Reorder a feature and rebuild.
    pub fn reorder_feature(
        &mut self,
        id: Uuid,
        new_position: usize,
        kb: &mut dyn KernelBundle,
    ) -> Result<(), EngineError> {
        let old_position = self
            .tree
            .feature_index(id)
            .ok_or(EngineError::FeatureNotFound { id })?;
        self.tree.reorder_feature(id, new_position)?;
        let actual_new_position = self.tree.feature_index(id).unwrap();
        self.undo_stack.push(Command::ReorderFeature {
            feature_id: id,
            old_position,
            new_position: actual_new_position,
        });
        self.rebuild(
            kb,
            old_position.min(actual_new_position),
            rebuild::Changed::All,
        );
        Ok(())
    }

    /// Rename a feature. No rebuild needed.
    pub fn rename_feature(&mut self, id: Uuid, new_name: String) -> Result<(), EngineError> {
        let old_name = self.tree.rename_feature(id, new_name.clone())?;
        self.undo_stack.push(Command::RenameFeature {
            feature_id: id,
            old_name,
            new_name,
        });
        Ok(())
    }

    /// Set (or clear, with an empty name) a body's display-name override. The
    /// body is identified by its persistent id (`FeatureTree::body_id`).
    /// Independent of feature names. No rebuild needed.
    pub fn rename_body(&mut self, body_id: String, new_name: String) {
        let trimmed = new_name.trim();
        let new = if trimmed.is_empty() {
            None
        } else {
            Some(trimmed.to_string())
        };
        let old_name = self.tree.set_body_name(&body_id, new.clone());
        self.undo_stack.push(Command::RenameBody {
            body_id,
            old_name,
            new_name: new,
        });
        self.recompute_body_name_inheritance();
    }

    /// Resolved name override for a body: the explicit user override if set,
    /// else a name inherited from a consumed target body. `None` ⇒ the caller
    /// should derive a name from the producing feature. This is the single
    /// resolution point consulted by the render layer.
    pub fn display_body_name_override(&self, body_id: &str) -> Option<&str> {
        self.tree
            .body_name_override(body_id)
            .or_else(|| self.inherited_body_names.get(body_id).map(String::as_str))
    }

    /// The target body a feature consumes (for name inheritance), as
    /// `(target_feature_id, target_body_id)`: `BooleanCombine`'s first operand
    /// (`body_a`), or the most-recent prior solid for a merge/cut
    /// extrude/revolve. `None` if the feature consumes nothing.
    fn consume_target_body_id(&self, feature: &Feature) -> Option<(Uuid, String)> {
        match &feature.operation {
            Operation::BooleanCombine { params } => {
                if let Anchor::FeatureOutput {
                    feature_id,
                    output_key,
                } = &params.body_a.anchor
                {
                    Some((*feature_id, FeatureTree::body_id(*feature_id, output_key)))
                } else {
                    None
                }
            }
            Operation::Extrude { .. } => {
                // find_consumed_feature_ids honors the normalized combine
                // (NewBody ⇒ none; Add/Cut/Intersect ⇒ resolved targets).
                rebuild::find_consumed_feature_ids(
                    feature,
                    &self.feature_results,
                    &self.tree,
                    &self.consumed_features,
                    None,
                )
                .first()
                .map(|fid| (*fid, FeatureTree::body_id(*fid, &OutputKey::Main)))
            }
            Operation::Revolve { .. } | Operation::Pipe { .. } => {
                rebuild::find_consumed_feature_ids(
                    feature,
                    &self.feature_results,
                    &self.tree,
                    &self.consumed_features,
                    None,
                )
                .first()
                .map(|fid| (*fid, FeatureTree::body_id(*fid, &OutputKey::Main)))
            }
            // A pattern's Main is instance 0 of its first seed: it inherits
            // that seed body's name.
            Operation::PatternCircular { params } => params.seeds.first().and_then(|gr| {
                if let Anchor::FeatureOutput {
                    feature_id,
                    output_key,
                } = &gr.anchor
                {
                    Some((*feature_id, FeatureTree::body_id(*feature_id, output_key)))
                } else {
                    None
                }
            }),
            Operation::PatternLinear { params } => params.seeds.first().and_then(|gr| {
                if let Anchor::FeatureOutput {
                    feature_id,
                    output_key,
                } = &gr.anchor
                {
                    Some((*feature_id, FeatureTree::body_id(*feature_id, output_key)))
                } else {
                    None
                }
            }),
            // A union's Main is the first body's lump: it inherits that
            // body's name (`specs/b4_balanced_union.md` §2.2).
            Operation::UnionAll { params } => union_all::name_source(
                params,
                self.consumed_by
                    .get(&feature.id)
                    .and_then(|v| v.first().copied()),
            ),
            _ => None,
        }
    }

    /// The body one output of `feature` inherits its custom name from, as
    /// `(source_feature_id, source_body_id)`.
    ///
    /// For an explicit-target combine: siblings it carries unchanged (its
    /// trailing outputs, `rebuild::untargeted_sibling_sources`) inherit from the
    /// body they carry, and `Main` from the first resolved target's OWN output —
    /// not that feature's `Main`, which gave the result of cutting a down tube
    /// the top tube's name (docs/notes/agent_bicycle_session_failures_2026_09_14.md F9b).
    /// Otherwise only `Main` inherits, from `consume_target_body_id`.
    fn inherit_source_body_id(
        &self,
        feature: &Feature,
        result: &OpResult,
        key: &OutputKey,
    ) -> Option<(Uuid, String)> {
        let explicit = match &feature.operation {
            Operation::Extrude { params } => Some(types::normalize_extrude_combine(params)),
            Operation::Revolve { params } => Some(types::normalize_revolve_combine(params)),
            Operation::Pipe { params } => Some(types::normalize_pipe_combine(params)),
            _ => None,
        }
        .filter(|eff| {
            !matches!(eff.mode, types::CombineMode::NewBody)
                && matches!(eff.targets, types::TargetStrategy::Explicit(_))
        });
        let Some(eff) = explicit else {
            return if *key == OutputKey::Main {
                self.consume_target_body_id(feature)
            } else {
                None
            };
        };

        let siblings = rebuild::untargeted_sibling_sources(&eff, &self.feature_results);
        let n = result.outputs.len();
        if let Some(pos) = result.outputs.iter().position(|(k, _)| k == key) {
            let first_carried = n.saturating_sub(siblings.len());
            if !siblings.is_empty() && pos >= first_carried {
                let (fid, source_key) = &siblings[pos - first_carried];
                return Some((*fid, FeatureTree::body_id(*fid, source_key)));
            }
        }
        if *key == OutputKey::Main {
            return rebuild::first_resolved_explicit_target(&eff, &self.feature_results)
                .map(|(fid, target_key)| (fid, FeatureTree::body_id(fid, &target_key)));
        }
        None
    }

    /// Recompute the transient body-name inheritance map. When a feature's Main
    /// result consumes a target body that carries a CUSTOM name (an explicit
    /// override, or itself inherited from one), the result inherits it — unless
    /// the result has its own explicit override. Derived (uncustomized) target
    /// names do NOT propagate. Built in feature order so inheritance chains
    /// (A→C→E); a feature only ever consumes earlier features.
    fn recompute_body_name_inheritance(&mut self) {
        let mut inherited: HashMap<String, String> = HashMap::new();
        // Resolved custom name per body id, accumulated in feature order.
        let mut custom: HashMap<String, String> = HashMap::new();

        for feature in &self.tree.features {
            let Some(result) = self.feature_results.get(&feature.id) else {
                continue;
            };
            for (key, _body) in &result.outputs {
                let body_id = FeatureTree::body_id(feature.id, key);
                let explicit = self.tree.body_names.get(&body_id).cloned();
                let inherited_name = self
                    .inherit_source_body_id(feature, result, key)
                    // Only inherit when the target was actually consumed —
                    // a failed union leaves both bodies separate (no theft).
                    .filter(|(tfid, _)| self.consumed_features.contains(tfid))
                    .and_then(|(_, tid)| custom.get(&tid).cloned());

                // The body's resolved custom name (if any) propagates downstream.
                if let Some(name) = explicit.clone().or_else(|| inherited_name.clone()) {
                    custom.insert(body_id.clone(), name);
                }
                // Record an inheritance only where the user set no explicit name.
                if explicit.is_none() {
                    if let Some(name) = inherited_name {
                        inherited.insert(body_id, name);
                    }
                }
            }
        }

        self.inherited_body_names = inherited;
    }

    /// Replace the design-parameter table and rebuild everything that
    /// consumes it (undoable). The UI always sends the complete list.
    pub fn set_parameters(
        &mut self,
        parameters: Vec<types::DesignParameter>,
        kb: &mut dyn KernelBundle,
    ) {
        let old = std::mem::replace(&mut self.tree.parameters, parameters);
        self.undo_stack.push(Command::SetParameters {
            old,
            new: self.tree.parameters.clone(),
        });
        // Rebuild from 0: any feature may consume any parameter. The apply
        // pass inside rebuild() refreshes every expression and reports the
        // features whose values changed; only those (and their dependents)
        // re-execute.
        self.rebuild(kb, 0, nothing_changed());
    }

    /// Set rollback index and rebuild. Not undoable.
    pub fn set_rollback(&mut self, index: Option<usize>, kb: &mut dyn KernelBundle) {
        self.tree.set_rollback(index);
        // No definition changed: features that became active have no result
        // and execute; the rest keep theirs.
        self.rebuild(kb, 0, nothing_changed());
    }

    /// Undo the last command.
    pub fn undo(&mut self, kb: &mut dyn KernelBundle) -> Result<(), EngineError> {
        let cmd = self
            .undo_stack
            .pop_undo()
            .ok_or(EngineError::NothingToUndo)?;
        let changed = changed_by(&cmd);
        let rebuild_from = self.apply_inverse(&cmd);
        self.undo_stack.push_redo(cmd);
        self.rebuild(kb, rebuild_from, changed);
        Ok(())
    }

    /// Redo the last undone command.
    pub fn redo(&mut self, kb: &mut dyn KernelBundle) -> Result<(), EngineError> {
        let cmd = self
            .undo_stack
            .pop_redo()
            .ok_or(EngineError::NothingToRedo)?;
        let changed = changed_by(&cmd);
        let rebuild_from = self.apply_forward(&cmd);
        self.undo_stack.push_undo_only(cmd);
        self.rebuild(kb, rebuild_from, changed);
        Ok(())
    }

    /// Apply the inverse of a command (for undo). Returns the rebuild-from index.
    fn apply_inverse(&mut self, cmd: &Command) -> usize {
        match cmd {
            Command::AddFeature { feature, .. } => {
                let pos = self.tree.feature_index(feature.id).unwrap_or(0);
                let _ = self.tree.remove_feature(feature.id);
                self.feature_results.remove(&feature.id);
                // GC the record with the feature — including one set outside
                // the command, which would otherwise be orphaned (ICR-4).
                let _ = self.tree.take_provenance(feature.id);
                pos.min(self.tree.features.len().saturating_sub(1))
            }
            Command::RemoveFeature {
                feature,
                position,
                removed_body_names,
                removed_provenance,
            } => {
                self.tree.features.insert(*position, (**feature).clone());
                // Restore the deleted feature's body-name overrides + provenance.
                self.tree.restore_body_names(removed_body_names.clone());
                self.tree
                    .restore_provenance(feature.id, removed_provenance.clone());
                // Adjust active_index if needed
                if let Some(ref mut idx) = self.tree.active_index {
                    if *position <= *idx {
                        *idx += 1;
                    }
                }
                *position
            }
            Command::EditFeature {
                feature_id,
                old_operation,
                provenance,
                ..
            } => {
                let pos = self.tree.feature_index(*feature_id).unwrap_or(0);
                if let Some(f) = self.tree.find_feature_mut(*feature_id) {
                    f.operation = (**old_operation).clone();
                }
                if let Some((old, _)) = provenance {
                    self.tree.set_provenance(*feature_id, old.clone());
                }
                pos
            }
            Command::ReorderFeature {
                feature_id,
                old_position,
                ..
            } => {
                let current = self.tree.feature_index(*feature_id).unwrap_or(0);
                let _ = self.tree.reorder_feature(*feature_id, *old_position);
                current.min(*old_position)
            }
            Command::SuppressFeature {
                feature_id,
                old_suppressed,
                ..
            } => {
                let pos = self.tree.feature_index(*feature_id).unwrap_or(0);
                let _ = self.tree.set_suppressed(*feature_id, *old_suppressed);
                pos
            }
            Command::RenameFeature {
                feature_id,
                old_name,
                ..
            } => {
                let _ = self.tree.rename_feature(*feature_id, old_name.clone());
                0 // No rebuild needed for rename
            }
            Command::RenameBody {
                body_id, old_name, ..
            } => {
                self.tree.set_body_name(body_id, old_name.clone());
                0 // No rebuild needed for rename
            }
            Command::SetParameters { old, .. } => {
                self.tree.parameters = old.clone();
                0 // Any feature may consume any parameter.
            }
        }
    }

    /// Apply a command forward (for redo). Returns the rebuild-from index.
    fn apply_forward(&mut self, cmd: &Command) -> usize {
        match cmd {
            Command::AddFeature {
                feature,
                position,
                provenance,
            } => {
                self.tree.features.insert(*position, (**feature).clone());
                self.tree.restore_provenance(feature.id, provenance.clone());
                if let Some(ref mut idx) = self.tree.active_index {
                    if *position <= *idx {
                        *idx += 1;
                    }
                }
                *position
            }
            Command::RemoveFeature { feature, .. } => {
                let pos = self.tree.feature_index(feature.id).unwrap_or(0);
                let _ = self.tree.remove_feature(feature.id);
                self.feature_results.remove(&feature.id);
                // Re-GC the feature's body names + provenance (already captured
                // in the command).
                let _ = self.tree.take_body_names(feature.id);
                let _ = self.tree.take_provenance(feature.id);
                pos.min(self.tree.features.len().saturating_sub(1))
            }
            Command::EditFeature {
                feature_id,
                new_operation,
                provenance,
                ..
            } => {
                let pos = self.tree.feature_index(*feature_id).unwrap_or(0);
                if let Some(f) = self.tree.find_feature_mut(*feature_id) {
                    f.operation = (**new_operation).clone();
                }
                if let Some((_, new)) = provenance {
                    self.tree.set_provenance(*feature_id, Some(new.clone()));
                }
                pos
            }
            Command::ReorderFeature {
                feature_id,
                new_position,
                ..
            } => {
                let current = self.tree.feature_index(*feature_id).unwrap_or(0);
                let _ = self.tree.reorder_feature(*feature_id, *new_position);
                current.min(*new_position)
            }
            Command::SuppressFeature {
                feature_id,
                new_suppressed,
                ..
            } => {
                let pos = self.tree.feature_index(*feature_id).unwrap_or(0);
                let _ = self.tree.set_suppressed(*feature_id, *new_suppressed);
                pos
            }
            Command::RenameFeature {
                feature_id,
                new_name,
                ..
            } => {
                let _ = self.tree.rename_feature(*feature_id, new_name.clone());
                0 // No rebuild needed for rename
            }
            Command::RenameBody {
                body_id, new_name, ..
            } => {
                self.tree.set_body_name(body_id, new_name.clone());
                0 // No rebuild needed for rename
            }
            Command::SetParameters { new, .. } => {
                self.tree.parameters = new.clone();
                0 // Any feature may consume any parameter.
            }
        }
    }

    /// Rebuild the feature tree from the given index, re-executing only what
    /// `changed` reaches (see [`rebuild::Changed`]).
    fn rebuild(
        &mut self,
        kb: &mut dyn KernelBundle,
        from_index: usize,
        mut changed: rebuild::Changed,
    ) {
        // Design-parameter pass FIRST: refresh every expression-driven
        // measurement (and re-solve affected sketches) so the rebuild below
        // executes against current values. If an expression changed a feature
        // EARLIER than the requested rebuild point, widen to include it.
        let param_outcome = params::apply_parameters(&mut self.tree);
        let from_index = param_outcome
            .first_changed
            .map_or(from_index, |c| c.min(from_index));
        // Context pass (in-context editing): re-derive every sketch plane that
        // is a scoped reference from the open assembly context; a moved plane
        // widens the rebuild to that sketch.
        let context_outcome =
            context::apply_context(&mut self.tree, self.context.as_ref(), kb.as_introspect());
        let from_index = context_outcome
            .first_changed
            .map_or(from_index, |c| c.min(from_index));
        if let rebuild::Changed::Features(ids) = &mut changed {
            ids.extend(param_outcome.changed.iter().copied());
            ids.extend(context_outcome.changed.iter().copied());
        }

        // Clear results for inactive features (beyond rollback). Active
        // features' results go to the rebuild, which keeps the ones it does
        // not re-execute.
        let active_len = self.tree.active_features().len();
        for feature in self.tree.features.iter().skip(active_len) {
            self.feature_results.remove(&feature.id);
        }

        let state = rebuild::rebuild(
            &self.tree,
            kb,
            from_index,
            &changed,
            &self.feature_results,
            &self.rebuild_errors,
            &self.sources,
            self.context.as_ref(),
        );
        self.feature_results = state.feature_results;
        self.rebuild_errors = state.feature_errors.clone();
        self.warnings = state.warnings;
        self.warnings.extend(context_outcome.warnings);
        // Parameter/expression errors surface ahead of rebuild errors — a bad
        // expression is usually the CAUSE of the downstream failures.
        // The typed list is built from the same sources in the same order
        // (ICR-2), so `feature_errors[i]` describes `errors[i]`.
        let typed = |errors: &[(Uuid, String)], kind: ErrorKind| {
            errors
                .iter()
                .map(|(id, message)| FeatureError {
                    feature_id: *id,
                    kind: kind.clone(),
                    message: message.clone(),
                })
                .collect::<Vec<_>>()
        };
        self.feature_errors = typed(&param_outcome.errors, ErrorKind::Expression);
        self.feature_errors
            .extend(typed(&context_outcome.errors, ErrorKind::Context));
        self.feature_errors.extend(state.feature_errors);
        self.errors = param_outcome.errors;
        self.errors.extend(context_outcome.errors);
        self.errors.extend(state.errors);
        self.consumed_features = state.consumed_features;
        self.consumed_by = state.consumed_by;
        // KV13 F6: accumulate the pid→feature map. A full rebuild (from 0, all
        // changed) re-executes and re-captures every feature, so clear first;
        // any other rebuild carries features forward WITHOUT re-executing
        // them, so their captures (from a prior rebuild) must be retained —
        // their kernel geometry, and thus pids, persist unchanged in the same
        // arena. Sound because arena pids are never reused: a pid always maps
        // to its creating feature.
        if from_index == 0 && matches!(changed, rebuild::Changed::All) {
            self.pid_to_feature.clear();
        }
        // First-claimant-wins (NOT `extend`, which would OVERWRITE): an
        // incremental rebuild's fresh state re-derives a consumed operand's
        // root pids and would otherwise re-attribute them to the consuming
        // feature. The introducing feature claimed them in an earlier rebuild;
        // keep that. (Within a single rebuild's state, `capture_face_pids`
        // already applies first-claimant ordering via feature order.)
        for (pid, fid) in state.pid_to_feature {
            self.pid_to_feature.entry(pid).or_insert(fid);
        }
        self.connectors =
            connector::part_connectors(&self.tree, &self.feature_results, kb.as_introspect());
        self.recompute_body_name_inheritance();
    }

    /// Full rebuild from scratch (clears all results first).
    pub fn rebuild_from_scratch(&mut self, kb: &mut dyn KernelBundle) {
        self.feature_results.clear();
        self.rebuild_errors.clear();
        self.rebuild(kb, 0, rebuild::Changed::All);
    }

    /// Get the OpResult for a feature.
    pub fn get_result(&self, feature_id: Uuid) -> Option<&OpResult> {
        self.feature_results.get(&feature_id)
    }

    /// Whether undo is available.
    pub fn can_undo(&self) -> bool {
        self.undo_stack.can_undo()
    }

    /// Whether redo is available.
    pub fn can_redo(&self) -> bool {
        self.undo_stack.can_redo()
    }

    /// Take the undo/redo history out of the engine, leaving it empty.
    ///
    /// A host that drives several feature trees through one engine (the
    /// document session's tabs, `specs/waffle_server_mode.md` §2.3 S2) must
    /// park the outgoing tree's history and restore the incoming one.
    /// Without it a command recorded against one tree is popped against
    /// another: `rebuild_from_scratch` clears results, never the stack.
    pub fn take_history(&mut self) -> UndoStack {
        std::mem::take(&mut self.undo_stack)
    }

    /// Restore a history taken by [`Engine::take_history`], discarding the
    /// current one.
    pub fn set_history(&mut self, history: UndoStack) {
        self.undo_stack = history;
    }
}

/// One feature changed.
fn changed_feature(id: Uuid) -> rebuild::Changed {
    rebuild::Changed::Features(std::collections::HashSet::from([id]))
}

/// No feature definition changed (the parameter and context passes may still
/// report some).
fn nothing_changed() -> rebuild::Changed {
    rebuild::Changed::Features(std::collections::HashSet::new())
}

/// What undoing or redoing a command changes.
fn changed_by(cmd: &Command) -> rebuild::Changed {
    match cmd {
        Command::AddFeature { feature, .. } | Command::RemoveFeature { feature, .. } => {
            changed_feature(feature.id)
        }
        Command::EditFeature { feature_id, .. } | Command::SuppressFeature { feature_id, .. } => {
            changed_feature(*feature_id)
        }
        Command::ReorderFeature { .. } => rebuild::Changed::All,
        Command::RenameFeature { .. }
        | Command::RenameBody { .. }
        | Command::SetParameters { .. } => nothing_changed(),
    }
}

impl Default for Engine {
    fn default() -> Self {
        Self::new()
    }
}
