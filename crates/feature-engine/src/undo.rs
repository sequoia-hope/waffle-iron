use uuid::Uuid;

use crate::types::{Feature, Operation};

/// A reversible command recorded by the engine.
#[derive(Debug, Clone)]
pub enum Command {
    AddFeature {
        feature: Box<Feature>,
        position: usize,
    },
    RemoveFeature {
        feature: Box<Feature>,
        position: usize,
    },
    EditFeature {
        feature_id: Uuid,
        old_operation: Box<Operation>,
        new_operation: Box<Operation>,
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
