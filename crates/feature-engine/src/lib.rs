pub mod rebuild;
pub mod resolve;
pub mod tree;
pub mod types;

use std::collections::HashMap;
use uuid::Uuid;

use modeling_ops::{KernelBundle, OpResult};

use crate::types::{EngineError, FeatureTree, Operation};

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
}

impl Engine {
    /// Create a new engine.
    pub fn new() -> Self {
        Self {
            tree: FeatureTree::new(),
            feature_results: HashMap::new(),
            warnings: Vec::new(),
            errors: Vec::new(),
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
        let rebuild_from = self.tree.feature_index(id).unwrap_or(0);
        self.rebuild(kb, rebuild_from);
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
        self.tree.remove_feature(id)?;
        self.feature_results.remove(&id);
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
        feature.operation = operation;

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
        self.tree.set_suppressed(id, suppressed)?;
        self.rebuild(kb, pos);
        Ok(())
    }

    /// Set rollback index and rebuild.
    pub fn set_rollback(&mut self, index: Option<usize>, kb: &mut dyn KernelBundle) {
        self.tree.set_rollback(index);
        self.rebuild(kb, 0);
    }

    /// Rebuild the feature tree from the given index.
    fn rebuild(&mut self, kb: &mut dyn KernelBundle, from_index: usize) {
        // Clear results from the rebuild point onward
        let active = self.tree.active_features();
        for feature in active.iter().skip(from_index) {
            self.feature_results.remove(&feature.id);
        }

        let state = rebuild::rebuild(&self.tree, kb, from_index, &self.feature_results);
        self.feature_results.extend(state.feature_results);
        self.warnings = state.warnings;
        self.errors = state.errors;
    }

    /// Get the OpResult for a feature.
    pub fn get_result(&self, feature_id: Uuid) -> Option<&OpResult> {
        self.feature_results.get(&feature_id)
    }
}

impl Default for Engine {
    fn default() -> Self {
        Self::new()
    }
}
