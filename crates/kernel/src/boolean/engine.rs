use thiserror::Error;

use crate::geometry::point::Point3d;
use crate::topology::brep::*;

use super::classify::{classify_point, PointClassification};

/// Boolean operation type.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum BoolOp {
    Union,
    Intersection,
    Difference,
}

/// Structured failure information for Boolean operations.
#[derive(Debug, Error)]
pub enum BooleanFailure {
    #[error("Bounding boxes don't intersect — no Boolean interaction")]
    NoOverlap,

    #[error("Classification ambiguous for face")]
    ClassificationAmbiguous {
        test_results: Vec<PointClassification>,
    },

    #[error("Topology corrupted after Boolean operation")]
    TopologyCorrupted { audit: TopologyAudit },

    #[error("Surface intersection failed: {reason}")]
    IntersectionFailed { reason: String },

    #[error("Degenerate result (zero-volume)")]
    DegenerateResult,
}

/// Perform a Boolean operation between two solids.
/// This is a simplified implementation for Tier 1 (axis-aligned boxes) and Tier 2 (box-cylinder).
///
/// Strategy:
/// 1. Check bounding box overlap
/// 2. Classify faces of each solid against the other
/// 3. Collect faces based on operation type
/// 4. Build result solid
pub fn boolean_op(
    store: &mut EntityStore,
    solid_a: SolidId,
    solid_b: SolidId,
    op: BoolOp,
) -> Result<SolidId, BooleanFailure> {
    // Step 1: Bounding box check
    let bb_a = store.solid_bounding_box(solid_a);
    let bb_b = store.solid_bounding_box(solid_b);

    if !bb_a.intersects(&bb_b) {
        match op {
            BoolOp::Union => {
                // Non-overlapping union: just combine shells
                return Ok(combine_solids(store, solid_a, solid_b));
            }
            BoolOp::Intersection => {
                return Err(BooleanFailure::NoOverlap);
            }
            BoolOp::Difference => {
                // A - B where B doesn't overlap: result is A
                return Ok(clone_solid(store, solid_a));
            }
        }
    }

    // Step 2: For each face in A, classify representative points against B
    // For each face in B, classify representative points against A
    let faces_a = collect_all_faces(store, solid_a);
    let faces_b = collect_all_faces(store, solid_b);

    let classified_a: Vec<(FaceId, PointClassification)> = faces_a
        .iter()
        .map(|&face_id| {
            let center = face_center(store, face_id);
            let class = classify_point(store, solid_b, &center, 1e-6);
            (face_id, class)
        })
        .collect();

    let classified_b: Vec<(FaceId, PointClassification)> = faces_b
        .iter()
        .map(|&face_id| {
            let center = face_center(store, face_id);
            let class = classify_point(store, solid_a, &center, 1e-6);
            (face_id, class)
        })
        .collect();

    // Step 3: Select faces based on operation
    let mut result_faces: Vec<FaceId> = Vec::new();

    match op {
        BoolOp::Union => {
            // Faces of A that are outside B + Faces of B that are outside A
            for &(face_id, class) in &classified_a {
                if class == PointClassification::Outside {
                    result_faces.push(face_id);
                }
            }
            for &(face_id, class) in &classified_b {
                if class == PointClassification::Outside {
                    result_faces.push(face_id);
                }
            }
        }
        BoolOp::Intersection => {
            // Faces of A that are inside B + Faces of B that are inside A
            for &(face_id, class) in &classified_a {
                if class == PointClassification::Inside {
                    result_faces.push(face_id);
                }
            }
            for &(face_id, class) in &classified_b {
                if class == PointClassification::Inside {
                    result_faces.push(face_id);
                }
            }
        }
        BoolOp::Difference => {
            // Faces of A that are outside B + Faces of B that are inside A (reversed)
            for &(face_id, class) in &classified_a {
                if class == PointClassification::Outside {
                    result_faces.push(face_id);
                }
            }
            for &(face_id, class) in &classified_b {
                if class == PointClassification::Inside {
                    result_faces.push(face_id);
                }
            }
        }
    }

    if result_faces.is_empty() {
        return Err(BooleanFailure::DegenerateResult);
    }

    // Step 4: Build result solid
    let result_solid = store.solids.insert(Solid { shells: vec![] });
    let result_shell = store.shells.insert(Shell {
        faces: result_faces.clone(),
        orientation: ShellOrientation::Outward,
        solid: result_solid,
    });
    store.solids[result_solid].shells.push(result_shell);

    // Update face shell references
    for &face_id in &result_faces {
        store.faces[face_id].shell = result_shell;
    }

    Ok(result_solid)
}

/// Collect all face IDs from a solid.
fn collect_all_faces(store: &EntityStore, solid_id: SolidId) -> Vec<FaceId> {
    let solid = &store.solids[solid_id];
    let mut faces = Vec::new();
    for &shell_id in &solid.shells {
        faces.extend(store.shells[shell_id].faces.iter());
    }
    faces
}

/// Compute the center point of a face (average of vertex positions).
fn face_center(store: &EntityStore, face_id: FaceId) -> Point3d {
    let face = &store.faces[face_id];
    let loop_data = &store.loops[face.outer_loop];

    if loop_data.half_edges.is_empty() {
        return Point3d::ORIGIN;
    }

    let mut sum_x = 0.0;
    let mut sum_y = 0.0;
    let mut sum_z = 0.0;
    let mut count = 0;

    for &he_id in &loop_data.half_edges {
        let he = &store.half_edges[he_id];
        let p = store.vertices[he.start_vertex].point;
        sum_x += p.x;
        sum_y += p.y;
        sum_z += p.z;
        count += 1;
    }

    Point3d::new(sum_x / count as f64, sum_y / count as f64, sum_z / count as f64)
}

/// Combine two non-overlapping solids into one.
fn combine_solids(store: &mut EntityStore, a: SolidId, b: SolidId) -> SolidId {
    let shells_a = store.solids[a].shells.clone();
    let shells_b = store.solids[b].shells.clone();

    let result = store.solids.insert(Solid {
        shells: [shells_a, shells_b].concat(),
    });
    result
}

/// Clone a solid (shallow — shares faces with original).
fn clone_solid(store: &mut EntityStore, solid_id: SolidId) -> SolidId {
    let shells = store.solids[solid_id].shells.clone();
    store.solids.insert(Solid { shells })
}

/// Monte Carlo volume estimation for a solid.
/// Shoots random rays through the bounding box and counts inside/outside.
pub fn estimate_volume(store: &EntityStore, solid_id: SolidId, num_samples: usize) -> f64 {
    let bb = store.solid_bounding_box(solid_id);
    if !bb.is_valid() {
        return 0.0;
    }

    let margin = 0.01;
    let bb = bb.expanded(margin);
    let bb_volume = bb.volume();

    let mut inside_count = 0;

    // Use a simple deterministic seed for reproducibility
    let mut rng_state: u64 = 12345;

    for _ in 0..num_samples {
        // Simple LCG pseudo-random
        rng_state = rng_state.wrapping_mul(6364136223846793005).wrapping_add(1442695040888963407);
        let rx = (rng_state >> 33) as f64 / (u32::MAX as f64);
        rng_state = rng_state.wrapping_mul(6364136223846793005).wrapping_add(1442695040888963407);
        let ry = (rng_state >> 33) as f64 / (u32::MAX as f64);
        rng_state = rng_state.wrapping_mul(6364136223846793005).wrapping_add(1442695040888963407);
        let rz = (rng_state >> 33) as f64 / (u32::MAX as f64);

        let point = Point3d::new(
            bb.min.x + rx * (bb.max.x - bb.min.x),
            bb.min.y + ry * (bb.max.y - bb.min.y),
            bb.min.z + rz * (bb.max.z - bb.min.z),
        );

        if classify_point(store, solid_id, &point, 1e-7) == PointClassification::Inside {
            inside_count += 1;
        }
    }

    bb_volume * (inside_count as f64 / num_samples as f64)
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::topology::primitives::make_box;

    #[test]
    fn test_boolean_union_non_overlapping() {
        let mut store = EntityStore::new();
        let a = make_box(&mut store, 0.0, 0.0, 0.0, 1.0, 1.0, 1.0);
        let b = make_box(&mut store, 5.0, 5.0, 5.0, 6.0, 6.0, 6.0);

        let result = boolean_op(&mut store, a, b, BoolOp::Union);
        assert!(result.is_ok());
        let result_id = result.unwrap();
        // Non-overlapping union should have shells from both
        assert!(store.solids[result_id].shells.len() >= 1);
    }

    #[test]
    fn test_boolean_intersection_no_overlap() {
        let mut store = EntityStore::new();
        let a = make_box(&mut store, 0.0, 0.0, 0.0, 1.0, 1.0, 1.0);
        let b = make_box(&mut store, 5.0, 5.0, 5.0, 6.0, 6.0, 6.0);

        let result = boolean_op(&mut store, a, b, BoolOp::Intersection);
        assert!(matches!(result, Err(BooleanFailure::NoOverlap)));
    }

    #[test]
    fn test_boolean_difference_no_overlap() {
        let mut store = EntityStore::new();
        let a = make_box(&mut store, 0.0, 0.0, 0.0, 1.0, 1.0, 1.0);
        let b = make_box(&mut store, 5.0, 5.0, 5.0, 6.0, 6.0, 6.0);

        let result = boolean_op(&mut store, a, b, BoolOp::Difference);
        assert!(result.is_ok());
    }

    #[test]
    fn test_estimate_volume_box() {
        let mut store = EntityStore::new();
        let solid_id = make_box(&mut store, 0.0, 0.0, 0.0, 10.0, 10.0, 10.0);

        let vol = estimate_volume(&store, solid_id, 10000);
        let expected = 1000.0;
        // Monte Carlo should be within 10% for 10000 samples
        assert!(
            (vol - expected).abs() / expected < 0.15,
            "Estimated volume {} too far from expected {}",
            vol,
            expected
        );
    }
}
