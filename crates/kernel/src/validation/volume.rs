use crate::boolean::engine::estimate_volume;
use crate::topology::brep::*;

/// Verify that Boolean operations preserve the volume identity:
/// vol(A ∪ B) = vol(A) + vol(B) - vol(A ∩ B)
///
/// Returns the maximum relative error.
pub fn verify_boolean_volume_identity(
    store: &EntityStore,
    solid_a: SolidId,
    solid_b: SolidId,
    union: SolidId,
    intersection: Option<SolidId>,
    num_samples: usize,
) -> VolumeVerification {
    let vol_a = estimate_volume(store, solid_a, num_samples);
    let vol_b = estimate_volume(store, solid_b, num_samples);
    let vol_union = estimate_volume(store, union, num_samples);
    let vol_intersection = intersection.map(|s| estimate_volume(store, s, num_samples));

    let expected_union = if let Some(vol_int) = vol_intersection {
        vol_a + vol_b - vol_int
    } else {
        // If no intersection solid, assume no overlap
        vol_a + vol_b
    };

    let error = if expected_union > 0.0 {
        (vol_union - expected_union).abs() / expected_union
    } else {
        vol_union
    };

    VolumeVerification {
        vol_a,
        vol_b,
        vol_union,
        vol_intersection,
        expected_union,
        relative_error: error,
    }
}

#[derive(Debug)]
pub struct VolumeVerification {
    pub vol_a: f64,
    pub vol_b: f64,
    pub vol_union: f64,
    pub vol_intersection: Option<f64>,
    pub expected_union: f64,
    pub relative_error: f64,
}

impl VolumeVerification {
    pub fn is_valid(&self, max_relative_error: f64) -> bool {
        self.relative_error < max_relative_error
    }
}
