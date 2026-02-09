pub mod rebuild;
pub mod resolve;
pub mod tree;
pub mod types;
pub mod undo;

use std::collections::HashMap;
use uuid::Uuid;

use modeling_ops::{KernelBundle, OpResult};

use crate::types::{EngineError, FeatureTree, Operation};
use crate::undo::{Command, UndoStack};

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
            undo_stack: UndoStack::new(),
        }
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
        self.undo_stack.push(Command::RemoveFeature {
            feature,
            position: pos,
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
            Command::RemoveFeature { feature, position } => {
                self.tree.features.insert(*position, (**feature).clone());
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
