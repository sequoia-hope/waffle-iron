use uuid::Uuid;

use crate::types::{EngineError, Feature, FeatureTree, Operation};

impl FeatureTree {
    /// Add a feature at the end of the tree (or at the active index).
    pub fn add_feature(&mut self, name: String, operation: Operation) -> Uuid {
        let id = Uuid::new_v4();
        let feature = Feature {
            id,
            name,
            operation,
            suppressed: false,
            references: Vec::new(),
        };

        match self.active_index {
            Some(idx) => {
                let insert_pos = (idx + 1).min(self.features.len());
                self.features.insert(insert_pos, feature);
                self.active_index = Some(insert_pos);
            }
            None => {
                self.features.push(feature);
            }
        }

        id
    }

    /// Remove a feature by ID. Returns the removed feature.
    pub fn remove_feature(&mut self, id: Uuid) -> Result<Feature, EngineError> {
        let pos = self
            .features
            .iter()
            .position(|f| f.id == id)
            .ok_or(EngineError::FeatureNotFound { id })?;

        let feature = self.features.remove(pos);

        // Adjust active_index
        if let Some(ref mut idx) = self.active_index {
            if pos <= *idx && *idx > 0 {
                *idx -= 1;
            }
        }

        Ok(feature)
    }

    /// Reorder a feature from its current position to a new position.
    pub fn reorder_feature(&mut self, id: Uuid, new_pos: usize) -> Result<(), EngineError> {
        let old_pos = self
            .features
            .iter()
            .position(|f| f.id == id)
            .ok_or(EngineError::FeatureNotFound { id })?;

        let feature = self.features.remove(old_pos);
        let clamped_pos = new_pos.min(self.features.len());
        self.features.insert(clamped_pos, feature);

        Ok(())
    }

    /// Suppress or unsuppress a feature.
    pub fn set_suppressed(&mut self, id: Uuid, suppressed: bool) -> Result<(), EngineError> {
        let feature = self
            .features
            .iter_mut()
            .find(|f| f.id == id)
            .ok_or(EngineError::FeatureNotFound { id })?;
        feature.suppressed = suppressed;
        Ok(())
    }

    /// Set the rollback index.
    pub fn set_rollback(&mut self, index: Option<usize>) {
        self.active_index = index;
    }

    /// Find a feature by ID.
    pub fn find_feature(&self, id: Uuid) -> Option<&Feature> {
        self.features.iter().find(|f| f.id == id)
    }

    /// Find a feature by ID (mutable).
    pub fn find_feature_mut(&mut self, id: Uuid) -> Option<&mut Feature> {
        self.features.iter_mut().find(|f| f.id == id)
    }

    /// Get index of a feature by ID.
    pub fn feature_index(&self, id: Uuid) -> Option<usize> {
        self.features.iter().position(|f| f.id == id)
    }
}
