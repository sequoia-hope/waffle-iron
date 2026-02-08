pub mod engine;
pub mod classify;
pub mod split;

use crate::topology::brep::{EntityStore, SolidId};
use engine::BooleanFailure;

/// Trait for Boolean operations on B-Rep solids.
///
/// Provides `union`, `subtract`, and `intersect` operations. Implement this
/// trait to provide alternative Boolean backends or mock implementations.
pub trait BooleanEngine {
    /// Compute the union of two solids.
    fn union(&self, store: &mut EntityStore, a: SolidId, b: SolidId) -> Result<SolidId, BooleanFailure>;

    /// Subtract solid `b` from solid `a`.
    fn subtract(&self, store: &mut EntityStore, a: SolidId, b: SolidId) -> Result<SolidId, BooleanFailure>;

    /// Compute the intersection of two solids.
    fn intersect(&self, store: &mut EntityStore, a: SolidId, b: SolidId) -> Result<SolidId, BooleanFailure>;
}

/// Default Boolean engine backed by the existing `boolean_op()` function.
pub struct DefaultBooleanEngine;

impl BooleanEngine for DefaultBooleanEngine {
    fn union(&self, store: &mut EntityStore, a: SolidId, b: SolidId) -> Result<SolidId, BooleanFailure> {
        engine::boolean_op(store, a, b, engine::BoolOp::Union)
    }

    fn subtract(&self, store: &mut EntityStore, a: SolidId, b: SolidId) -> Result<SolidId, BooleanFailure> {
        engine::boolean_op(store, a, b, engine::BoolOp::Difference)
    }

    fn intersect(&self, store: &mut EntityStore, a: SolidId, b: SolidId) -> Result<SolidId, BooleanFailure> {
        engine::boolean_op(store, a, b, engine::BoolOp::Intersection)
    }
}

#[cfg(test)]
mod trait_tests {
    use super::*;
    use crate::topology::primitives::make_box;

    #[test]
    fn test_boolean_engine_trait_union() {
        let engine = DefaultBooleanEngine;
        let mut store = EntityStore::new();
        let a = make_box(&mut store, 0.0, 0.0, 0.0, 1.0, 1.0, 1.0);
        let b = make_box(&mut store, 5.0, 5.0, 5.0, 6.0, 6.0, 6.0);
        let result = engine.union(&mut store, a, b);
        assert!(result.is_ok());
    }

    #[test]
    fn test_boolean_engine_trait_subtract() {
        let engine = DefaultBooleanEngine;
        let mut store = EntityStore::new();
        let a = make_box(&mut store, 0.0, 0.0, 0.0, 2.0, 2.0, 2.0);
        let b = make_box(&mut store, 1.0, 1.0, 1.0, 3.0, 3.0, 3.0);
        let result = engine.subtract(&mut store, a, b);
        assert!(result.is_ok());
    }

    #[test]
    fn test_boolean_engine_trait_intersect() {
        let engine = DefaultBooleanEngine;
        let mut store = EntityStore::new();
        let a = make_box(&mut store, 0.0, 0.0, 0.0, 2.0, 2.0, 2.0);
        let b = make_box(&mut store, 1.0, 1.0, 1.0, 3.0, 3.0, 3.0);
        let result = engine.intersect(&mut store, a, b);
        assert!(result.is_ok());
    }
}
