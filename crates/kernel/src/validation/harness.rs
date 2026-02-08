//! Operation test harness for validating modeling operations.
//!
//! Captures pre-operation state, runs the operation, validates the result,
//! and computes deltas (volume change, face count change, etc.).

use crate::boolean::engine::estimate_volume;
use crate::topology::brep::*;
use super::config::ValidationConfig;
use super::types::*;
use super::BRepValidator;

/// Snapshot of a solid's state before an operation.
#[derive(Debug, Clone)]
pub struct PreOperationSnapshot {
    pub volume: f64,
    pub entity_counts: EntityCounts,
    pub validation_report: ValidationReport,
}

/// Result of running an operation through the validation harness.
#[derive(Debug)]
pub struct HarnessResult<T> {
    pub pre_snapshot: PreOperationSnapshot,
    pub operation_result: T,
    pub post_report: ValidationReport,
    pub volume_change: f64,
    pub volume_ratio: f64,
    pub face_count_change: i64,
}

/// Run a modeling operation through the validation harness.
///
/// 1. Captures pre-operation snapshot (validation + volume estimate).
/// 2. Runs the operation.
/// 3. Validates the result.
/// 4. Computes deltas.
///
/// The operation closure receives the store and original solid_id,
/// and returns a Result with the new SolidId.
pub fn validate_operation_ok<E: std::fmt::Debug>(
    store: &mut EntityStore,
    solid_id: SolidId,
    config: &ValidationConfig,
    op: impl FnOnce(&mut EntityStore, SolidId) -> Result<SolidId, E>,
) -> Result<HarnessResult<SolidId>, E> {
    let validator = BRepValidator::new(config.clone());

    // Pre-operation snapshot.
    let pre_report = validator.validate(store, solid_id);
    let pre_volume = estimate_volume(store, solid_id, 5000);
    let pre_counts = pre_report.metrics.entity_counts;

    let pre_snapshot = PreOperationSnapshot {
        volume: pre_volume,
        entity_counts: pre_counts,
        validation_report: pre_report,
    };

    // Run the operation.
    let result_id = op(store, solid_id)?;

    // Post-operation validation.
    let post_report = validator.validate(store, result_id);
    let post_volume = estimate_volume(store, result_id, 5000);
    let post_counts = post_report.metrics.entity_counts;

    let volume_change = post_volume - pre_volume;
    let volume_ratio = if pre_volume > 1e-15 {
        post_volume / pre_volume
    } else {
        f64::INFINITY
    };
    let face_count_change = post_counts.faces as i64 - pre_counts.faces as i64;

    Ok(HarnessResult {
        pre_snapshot,
        operation_result: result_id,
        post_report,
        volume_change,
        volume_ratio,
        face_count_change,
    })
}

/// Convenience macro for asserting that an operation through the harness
/// produces a valid result.
#[macro_export]
macro_rules! assert_operation_valid {
    ($harness_result:expr) => {
        assert!(
            $harness_result.post_report.valid,
            "Operation produced invalid solid:\n{}",
            $harness_result.post_report
        );
    };
    ($harness_result:expr, $msg:expr) => {
        assert!(
            $harness_result.post_report.valid,
            "{}: Operation produced invalid solid:\n{}",
            $msg,
            $harness_result.post_report
        );
    };
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::topology::primitives::make_box;
    use crate::geometry::point::Point3d;
    use crate::geometry::vector::Vec3;
    use crate::operations::extrude::{Profile, extrude_profile};

    #[test]
    fn test_extrude_through_harness() {
        let mut store = EntityStore::new();

        // Create a simple profile (square) and extrude it.
        let profile = Profile::rectangle(1.0, 1.0);
        let solid_id = extrude_profile(&mut store, &profile, Vec3::Z, 2.0).unwrap();
        let config = ValidationConfig::geometry();

        let result = validate_operation_ok(
            &mut store,
            solid_id,
            &config,
            |_store, sid| Ok::<SolidId, String>(sid), // Identity operation for testing
        );

        let result = result.unwrap();
        assert!(result.post_report.valid, "Extruded box should be valid: {}", result.post_report);
        assert!(result.pre_snapshot.volume > 0.0, "Volume should be positive");
    }

    #[test]
    fn test_box_identity_through_harness() {
        let mut store = EntityStore::new();
        let solid_id = make_box(&mut store, 0.0, 0.0, 0.0, 1.0, 1.0, 1.0);
        let config = ValidationConfig::topology();

        let result = validate_operation_ok(
            &mut store,
            solid_id,
            &config,
            |_store, sid| Ok::<SolidId, String>(sid),
        );

        let result = result.unwrap();
        assert!(result.post_report.valid);
        assert!((result.volume_change).abs() < 0.01, "Identity op should not change volume");
        assert_eq!(result.face_count_change, 0, "Identity op should not change face count");
        assert_operation_valid!(result);
    }

    #[test]
    fn test_harness_volume_positive() {
        let mut store = EntityStore::new();
        let solid_id = make_box(&mut store, 0.0, 0.0, 0.0, 2.0, 3.0, 4.0);
        let config = ValidationConfig::topology();

        let result = validate_operation_ok(
            &mut store,
            solid_id,
            &config,
            |_store, sid| Ok::<SolidId, String>(sid),
        );

        let result = result.unwrap();
        // Volume of 2x3x4 box = 24
        assert!(result.pre_snapshot.volume > 10.0 && result.pre_snapshot.volume < 40.0,
            "Volume should be ~24, got {}", result.pre_snapshot.volume);
    }

    #[test]
    fn test_harness_error_propagation() {
        let mut store = EntityStore::new();
        let solid_id = make_box(&mut store, 0.0, 0.0, 0.0, 1.0, 1.0, 1.0);
        let config = ValidationConfig::topology();

        let result = validate_operation_ok(
            &mut store,
            solid_id,
            &config,
            |_store, _sid| -> Result<SolidId, String> {
                Err("Intentional failure".into())
            },
        );

        assert!(result.is_err(), "Error should propagate");
        assert_eq!(result.unwrap_err(), "Intentional failure");
    }
}
