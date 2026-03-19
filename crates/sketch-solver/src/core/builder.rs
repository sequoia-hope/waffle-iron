//! Constraint builder: maps `SketchConstraint` → `ConstraintImpl`.
//!
//! Resolves entity IDs to typed indices using `ParamLayout` at build time.
//! The solver never touches entity IDs after this step.

use std::collections::HashMap;

use crate::types::{EntityKind, SketchConstraint, SketchEntity};

use super::constraint::ConstraintImpl;
use super::params::ParamLayout;

/// Build internal constraint representations from sketch constraints.
///
/// Entity-type dispatch (Equal, Distance, OnEntity, etc.) inspects
/// `entity_types` to choose the correct ConstraintImpl variant.
pub fn build_constraints(
    constraints: &[SketchConstraint],
    entities: &[SketchEntity],
    layout: &ParamLayout,
) -> Vec<ConstraintImpl> {
    let entity_types = classify_entities(entities);
    constraints
        .iter()
        .filter_map(|c| build_one(c, &entity_types, layout))
        .collect()
}

/// Map each equation row back to its parent constraint index.
///
/// Multi-equation constraints (Coincident=2, Midpoint=2, etc.) produce
/// multiple rows pointing to the same constraint index.
pub fn build_eq_to_constraint_map(constraints: &[ConstraintImpl]) -> Vec<usize> {
    use super::constraint::ConstraintEq;
    let mut map = Vec::new();
    for (i, c) in constraints.iter().enumerate() {
        for _ in 0..c.num_equations() {
            map.push(i);
        }
    }
    map
}

// ── Internals ──────────────────────────────────────────────────────────────

fn classify_entities(entities: &[SketchEntity]) -> HashMap<u32, EntityKind> {
    let mut map = HashMap::new();
    for e in entities {
        let kind = match e {
            SketchEntity::Point { .. } => EntityKind::Point,
            SketchEntity::Line { .. } => EntityKind::Line,
            SketchEntity::Circle { .. } => EntityKind::Circle,
            SketchEntity::Arc { .. } => EntityKind::Arc,
            _ => continue,
        };
        map.insert(e.id(), kind);
    }
    map
}

fn build_one(
    _constraint: &SketchConstraint,
    _entity_types: &HashMap<u32, EntityKind>,
    _layout: &ParamLayout,
) -> Option<ConstraintImpl> {
    todo!("Fork A/builder: constraint dispatch")
}
