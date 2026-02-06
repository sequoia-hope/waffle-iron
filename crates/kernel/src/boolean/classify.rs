use crate::geometry::curves::Ray;
use crate::geometry::intersection;
use crate::geometry::point::Point3d;
use crate::geometry::vector::Vec3;
use crate::topology::brep::*;

/// Classification of a point relative to a solid.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum PointClassification {
    Inside,
    Outside,
    OnBoundary,
}

/// Classify a point relative to a solid using ray casting.
/// Shoots multiple rays and uses majority vote for robustness.
pub fn classify_point(
    store: &EntityStore,
    solid_id: SolidId,
    point: &Point3d,
    tolerance: f64,
) -> PointClassification {
    // First check: is the point on a face boundary?
    let solid = &store.solids[solid_id];
    for &shell_id in &solid.shells {
        let shell = &store.shells[shell_id];
        for &face_id in &shell.faces {
            let face = &store.faces[face_id];
            if let crate::geometry::surfaces::Surface::Plane(plane) = &face.surface {
                let dist = plane.distance_to_point(point).abs();
                if dist < tolerance {
                    // Point is near this plane — check if it's within the face boundary
                    // (simplified: check if it's within the bounding box of the face)
                    if is_point_near_face(store, face_id, point, tolerance) {
                        return PointClassification::OnBoundary;
                    }
                }
            }
        }
    }

    // Ray casting with multiple test rays for robustness
    let test_directions = [
        Vec3::new(1.0, 0.0, 0.0),
        Vec3::new(0.0, 1.0, 0.0),
        Vec3::new(0.0, 0.0, 1.0),
        Vec3::new(1.0, 1.0, 1.0).normalize(),
        Vec3::new(-1.0, 0.5, 0.3).normalize(),
    ];

    let mut inside_votes = 0;
    let mut outside_votes = 0;

    for dir in &test_directions {
        let ray = Ray::new(*point, *dir);
        let crossings = count_ray_crossings(store, solid_id, &ray, tolerance);

        if crossings % 2 == 1 {
            inside_votes += 1;
        } else {
            outside_votes += 1;
        }
    }

    if inside_votes > outside_votes {
        PointClassification::Inside
    } else {
        PointClassification::Outside
    }
}

/// Count the number of times a ray crosses the boundary of a solid.
fn count_ray_crossings(
    store: &EntityStore,
    solid_id: SolidId,
    ray: &Ray,
    tolerance: f64,
) -> usize {
    let mut crossings = 0;
    let solid = &store.solids[solid_id];

    for &shell_id in &solid.shells {
        let shell = &store.shells[shell_id];
        for &face_id in &shell.faces {
            let face = &store.faces[face_id];

            match &face.surface {
                crate::geometry::surfaces::Surface::Plane(plane) => {
                    if let Some(hit) = intersection::ray_plane(ray, plane) {
                        if hit.t > tolerance {
                            // Check if hit point is within the face boundary
                            if is_point_in_face_2d(store, face_id, &hit.point, tolerance) {
                                crossings += 1;
                            }
                        }
                    }
                }
                crate::geometry::surfaces::Surface::Sphere(sphere) => {
                    let hits = intersection::ray_sphere(ray, sphere);
                    for hit in hits {
                        if hit.t > tolerance {
                            crossings += 1;
                        }
                    }
                }
                crate::geometry::surfaces::Surface::Cylinder(cyl) => {
                    let hits = intersection::ray_cylinder(ray, cyl);
                    for hit in hits {
                        if hit.t > tolerance {
                            // Simplified: accept all hits (proper implementation would check face bounds)
                            crossings += 1;
                        }
                    }
                }
                _ => {
                    // For other surface types, skip (will be implemented later)
                }
            }
        }
    }

    crossings
}

/// Check if a point is approximately within a face (simplified 2D point-in-polygon).
fn is_point_in_face_2d(
    store: &EntityStore,
    face_id: FaceId,
    point: &Point3d,
    tolerance: f64,
) -> bool {
    let face = &store.faces[face_id];

    // For planar faces, project to 2D and do point-in-polygon
    if let crate::geometry::surfaces::Surface::Plane(plane) = &face.surface {
        let (pu, pv) = plane.parameters_of(point);
        let dist = plane.distance_to_point(point).abs();
        if dist > tolerance * 10.0 {
            return false;
        }

        // Get 2D polygon from the outer loop
        let loop_data = &store.loops[face.outer_loop];
        let polygon: Vec<(f64, f64)> = loop_data
            .half_edges
            .iter()
            .map(|&he_id| {
                let he = &store.half_edges[he_id];
                let p = store.vertices[he.start_vertex].point;
                plane.parameters_of(&p)
            })
            .collect();

        if polygon.len() < 3 {
            return false;
        }

        point_in_polygon_2d(pu, pv, &polygon)
    } else {
        // Fallback: check bounding box
        is_point_near_face(store, face_id, point, tolerance * 100.0)
    }
}

/// 2D point-in-polygon test using ray casting.
fn point_in_polygon_2d(px: f64, py: f64, polygon: &[(f64, f64)]) -> bool {
    let n = polygon.len();
    let mut inside = false;

    let mut j = n - 1;
    for i in 0..n {
        let (xi, yi) = polygon[i];
        let (xj, yj) = polygon[j];

        if ((yi > py) != (yj > py)) && (px < (xj - xi) * (py - yi) / (yj - yi) + xi) {
            inside = !inside;
        }
        j = i;
    }

    inside
}

/// Simple check: is the point close to any vertex of the face?
fn is_point_near_face(
    store: &EntityStore,
    face_id: FaceId,
    point: &Point3d,
    tolerance: f64,
) -> bool {
    let face = &store.faces[face_id];
    let loop_data = &store.loops[face.outer_loop];

    for &he_id in &loop_data.half_edges {
        let he = &store.half_edges[he_id];
        let v = &store.vertices[he.start_vertex];
        if v.point.distance_to(point) < tolerance {
            return true;
        }
    }

    false
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::topology::primitives::make_box;

    #[test]
    fn test_classify_point_inside_box() {
        let mut store = EntityStore::new();
        let solid_id = make_box(&mut store, 0.0, 0.0, 0.0, 10.0, 10.0, 10.0);

        let center = Point3d::new(5.0, 5.0, 5.0);
        let result = classify_point(&store, solid_id, &center, 1e-7);
        assert_eq!(result, PointClassification::Inside);
    }

    #[test]
    fn test_classify_point_outside_box() {
        let mut store = EntityStore::new();
        let solid_id = make_box(&mut store, 0.0, 0.0, 0.0, 10.0, 10.0, 10.0);

        let outside = Point3d::new(20.0, 20.0, 20.0);
        let result = classify_point(&store, solid_id, &outside, 1e-7);
        assert_eq!(result, PointClassification::Outside);
    }
}
