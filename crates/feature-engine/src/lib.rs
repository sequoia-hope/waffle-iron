pub mod preview_mesh;
pub mod rebuild;
pub mod resolve;
pub mod tree;
pub mod types;
pub mod undo;

use std::collections::HashMap;
use uuid::Uuid;

use modeling_ops::{KernelBundle, OpResult};

use crate::types::{EngineError, Feature, FeatureTree, Operation};
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
    /// Feature IDs consumed by a later boolean (should not be rendered).
    pub consumed_features: std::collections::HashSet<Uuid>,
    /// KV13 F6: persistent-id → the feature that INTRODUCED it (recomputed each
    /// rebuild). The basis for resolving a face's *creating* feature through
    /// chained booleans — see [`Engine::created_by_feature`].
    pub pid_to_feature: HashMap<u64, Uuid>,
    /// Transient (NOT persisted) inherited body names, keyed by body id. When a
    /// boolean/merge consumes a target body that has a custom name, the result
    /// body inherits it. Recomputed on every rebuild and rename.
    inherited_body_names: HashMap<String, String>,
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
            consumed_features: std::collections::HashSet::new(),
            pid_to_feature: HashMap::new(),
            inherited_body_names: HashMap::new(),
            undo_stack: UndoStack::new(),
        }
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
        let id = self.tree.add_feature(name, operation);
        let position = self.tree.feature_index(id).unwrap_or(0);
        let feature = Box::new(self.tree.find_feature(id).unwrap().clone());
        self.undo_stack
            .push(Command::AddFeature { feature, position });
        self.rebuild(kb, position);
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
        self.undo_stack.push(Command::RemoveFeature {
            feature,
            position: pos,
            removed_body_names,
        });
        self.rebuild(kb, pos.min(self.tree.features.len().saturating_sub(1)));
        Ok(())
    }

    /// Edit a feature's operation and rebuild from that point.
    pub fn edit_feature(
        &mut self,
        id: Uuid,
        operation: Operation,
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

        self.undo_stack.push(Command::EditFeature {
            feature_id: id,
            old_operation: Box::new(old_operation),
            new_operation: Box::new(operation),
        });

        self.rebuild(kb, pos);
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
        self.rebuild(kb, pos);
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
        self.rebuild(kb, old_position.min(actual_new_position));
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
                )
                .first()
                .map(|fid| (*fid, FeatureTree::body_id(*fid, &OutputKey::Main)))
            }
            Operation::Revolve { params } if params.merge || params.cut => {
                rebuild::find_consumed_feature_ids(
                    feature,
                    &self.feature_results,
                    &self.tree,
                    &self.consumed_features,
                )
                .first()
                .map(|fid| (*fid, FeatureTree::body_id(*fid, &OutputKey::Main)))
            }
            _ => None,
        }
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
                let inherited_name = if *key == OutputKey::Main {
                    self.consume_target_body_id(feature)
                        // Only inherit when the target was actually consumed —
                        // a failed union leaves both bodies separate (no theft).
                        .filter(|(tfid, _)| self.consumed_features.contains(tfid))
                        .and_then(|(_, tid)| custom.get(&tid).cloned())
                } else {
                    None
                };

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

    /// Set rollback index and rebuild. Not undoable.
    pub fn set_rollback(&mut self, index: Option<usize>, kb: &mut dyn KernelBundle) {
        self.tree.set_rollback(index);
        self.rebuild(kb, 0);
    }

    /// Undo the last command.
    pub fn undo(&mut self, kb: &mut dyn KernelBundle) -> Result<(), EngineError> {
        let cmd = self
            .undo_stack
            .pop_undo()
            .ok_or(EngineError::NothingToUndo)?;
        let rebuild_from = self.apply_inverse(&cmd);
        self.undo_stack.push_redo(cmd);
        self.rebuild(kb, rebuild_from);
        Ok(())
    }

    /// Redo the last undone command.
    pub fn redo(&mut self, kb: &mut dyn KernelBundle) -> Result<(), EngineError> {
        let cmd = self
            .undo_stack
            .pop_redo()
            .ok_or(EngineError::NothingToRedo)?;
        let rebuild_from = self.apply_forward(&cmd);
        self.undo_stack.push_undo_only(cmd);
        self.rebuild(kb, rebuild_from);
        Ok(())
    }

    /// Apply the inverse of a command (for undo). Returns the rebuild-from index.
    fn apply_inverse(&mut self, cmd: &Command) -> usize {
        match cmd {
            Command::AddFeature { feature, .. } => {
                let pos = self.tree.feature_index(feature.id).unwrap_or(0);
                let _ = self.tree.remove_feature(feature.id);
                self.feature_results.remove(&feature.id);
                pos.min(self.tree.features.len().saturating_sub(1))
            }
            Command::RemoveFeature {
                feature,
                position,
                removed_body_names,
            } => {
                self.tree.features.insert(*position, (**feature).clone());
                // Restore the deleted feature's body-name overrides.
                self.tree.restore_body_names(removed_body_names.clone());
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
                ..
            } => {
                let pos = self.tree.feature_index(*feature_id).unwrap_or(0);
                if let Some(f) = self.tree.find_feature_mut(*feature_id) {
                    f.operation = (**old_operation).clone();
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
        }
    }

    /// Apply a command forward (for redo). Returns the rebuild-from index.
    fn apply_forward(&mut self, cmd: &Command) -> usize {
        match cmd {
            Command::AddFeature { feature, position } => {
                self.tree.features.insert(*position, (**feature).clone());
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
                // Re-GC the feature's body names (already captured in the command).
                let _ = self.tree.take_body_names(feature.id);
                pos.min(self.tree.features.len().saturating_sub(1))
            }
            Command::EditFeature {
                feature_id,
                new_operation,
                ..
            } => {
                let pos = self.tree.feature_index(*feature_id).unwrap_or(0);
                if let Some(f) = self.tree.find_feature_mut(*feature_id) {
                    f.operation = (**new_operation).clone();
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
        }
    }

    /// Rebuild the feature tree from the given index.
    fn rebuild(&mut self, kb: &mut dyn KernelBundle, from_index: usize) {
        // Clear results from the rebuild point onward (active features)
        let active = self.tree.active_features();
        for feature in active.iter().skip(from_index) {
            self.feature_results.remove(&feature.id);
        }

        // Clear results for inactive features (beyond rollback)
        let active_len = active.len();
        for feature in self.tree.features.iter().skip(active_len) {
            self.feature_results.remove(&feature.id);
        }

        let state = rebuild::rebuild(&self.tree, kb, from_index, &self.feature_results);
        self.feature_results.extend(state.feature_results);
        self.warnings = state.warnings;
        self.errors = state.errors;
        self.consumed_features = state.consumed_features;
        // KV13 F6: accumulate the pid→feature map. A full rebuild (from 0)
        // re-executes and re-captures every feature, so clear first; an
        // incremental rebuild (from_index > 0) carries earlier features forward
        // WITHOUT re-executing them, so their captures (from a prior rebuild)
        // must be retained — their kernel geometry, and thus pids, persist
        // unchanged in the same arena. Sound because arena pids are never
        // reused: a pid always maps to its creating feature.
        if from_index == 0 {
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
        self.recompute_body_name_inheritance();
    }

    /// Full rebuild from scratch (clears all results first).
    pub fn rebuild_from_scratch(&mut self, kb: &mut dyn KernelBundle) {
        self.feature_results.clear();
        self.rebuild(kb, 0);
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
}

impl Default for Engine {
    fn default() -> Self {
        Self::new()
    }
}
