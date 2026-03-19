use std::collections::HashMap;

use crate::types::{ClosedProfile, SketchEntity};

/// Extract closed profiles from solved sketch geometry.
///
/// Delegates to the canonical implementation in `waffle_types::profiles`.
pub fn extract_profiles(
    entities: &[SketchEntity],
    positions: &HashMap<u32, (f64, f64)>,
) -> Vec<ClosedProfile> {
    waffle_types::profiles::extract_profiles(entities, positions)
}
