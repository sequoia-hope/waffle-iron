use uuid::Uuid;

use crate::types::{BodyNames, DesignParameter, Feature, Operation, Provenance};

/// A reversible command recorded by the engine.
#[derive(Debug, Clone)]
pub enum Command {
    AddFeature {
        feature: Box<Feature>,
        position: usize,
        /// The provenance recorded with the add (`specs/waffle_mcp_server.md`
        /// ICR-4): undo removes it with the feature, redo restores it, so one
        /// undo leaves no orphan record in the file.
        provenance: Option<Provenance>,
    },
    RemoveFeature {
        feature: Box<Feature>,
        position: usize,
        /// Body-name overrides owned by the removed feature, captured so undo
        /// can restore them.
        removed_body_names: BodyNames,
        /// The removed feature's provenance record, likewise captured.
        removed_provenance: Option<Provenance>,
    },
    EditFeature {
        feature_id: Uuid,
        old_operation: Box<Operation>,
        new_operation: Box<Operation>,
        /// `(previous record, new record)` when the edit also set provenance
        /// (ICR-4); `None` leaves the record untouched in both directions.
        provenance: Option<(Option<Provenance>, Provenance)>,
    },
    ReorderFeature {
        feature_id: Uuid,
        old_position: usize,
        new_position: usize,
    },
    SuppressFeature {
        feature_id: Uuid,
        old_suppressed: bool,
        new_suppressed: bool,
    },
    RenameFeature {
        feature_id: Uuid,
        old_name: String,
        new_name: String,
    },
    RenameBody {
        body_id: String,
        /// Previous override (`None` ⇒ the body had no override / used a derived name).
        old_name: Option<String>,
        new_name: Option<String>,
    },
    /// Whole-table design-parameter replacement (the UI always sends the full
    /// list). Undo restores `old`, redo re-applies `new`; both rebuild from 0
    /// since any feature may consume any parameter.
    SetParameters {
        old: Vec<DesignParameter>,
        new: Vec<DesignParameter>,
    },
}

/// Two-stack undo/redo history.
#[derive(Debug)]
pub struct UndoStack {
    undo: Vec<Command>,
    redo: Vec<Command>,
}

impl UndoStack {
    pub fn new() -> Self {
        Self {
            undo: Vec::new(),
            redo: Vec::new(),
        }
    }

    /// Push a command onto the undo stack, clearing the redo stack.
    pub fn push(&mut self, cmd: Command) {
        self.undo.push(cmd);
        self.redo.clear();
    }

    /// Push a command onto the undo stack without clearing redo.
    /// Used by `redo()` to re-populate the undo stack.
    pub fn push_undo_only(&mut self, cmd: Command) {
        self.undo.push(cmd);
    }

    /// Pop the most recent command from the undo stack.
    pub fn pop_undo(&mut self) -> Option<Command> {
        self.undo.pop()
    }

    /// Push a command onto the redo stack.
    pub fn push_redo(&mut self, cmd: Command) {
        self.redo.push(cmd);
    }

    /// Pop the most recent command from the redo stack.
    pub fn pop_redo(&mut self) -> Option<Command> {
        self.redo.pop()
    }

    pub fn can_undo(&self) -> bool {
        !self.undo.is_empty()
    }

    pub fn can_redo(&self) -> bool {
        !self.redo.is_empty()
    }
}

impl Default for UndoStack {
    fn default() -> Self {
        Self::new()
    }
}
