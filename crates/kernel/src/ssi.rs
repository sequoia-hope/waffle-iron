//! Surface-Surface Intersection (SSI) module for general-position quadric surfaces.
//!
//! Provides analytical SSI computation for plane-cylinder, plane-sphere, plane-cone,
//! and cylinder-cylinder surface pairs, plus point-in-solid classifiers and Aabb helpers.
//!
//! All solvers accept surfaces in arbitrary position/orientation (general-position).
//! The Z-frame spatial helpers (`cyl_z_range`, `cyl_enclosed_in_box`, etc.) are retained
//! for use by boolean.rs which pre-rotates into a Z-aligned frame.
//!
//! Reference: Patrikalakis Ch.5 — SSI algorithms for analytic surfaces.

use crate::vecmath::{mat3_mul_vec, v3_add, v3_cross, v3_dot, v3_length, v3_scale, v3_sub, Mat3};
use crate::waffle_kernel::{CylinderParams, WaffleSolid};

// ── SSI curve types ────────────────────────────────────────────────────────

/// An intersection curve between two surfaces.
#[derive(Debug, Clone)]
pub(crate) enum SSICurve {
    /// A line segment in 3D.
    Line { start: [f64; 3], end: [f64; 3] },
    /// A full circle in 3D.
    Circle {
        center: [f64; 3],
        normal: [f64; 3],
        radius: f64,
    },
}

/// Axis-aligned bounding box.
#[derive(Debug, Clone)]
pub(crate) struct Aabb {
    pub min: [f64; 3],
    pub max: [f64; 3],
}

// ── Tolerance ─────────────────────────────────────────────────────────────

const TOL: f64 = 1e-9;

// ── Z range helper ─────────────────────────────────────────────────────────

/// Compute the Z extent of a cylinder from its center_bottom and direction.
///
/// This function assumes the cylinder has been rotated into a Z-aligned frame
/// (via `rotation_to_z` in boolean.rs). After rotation, the cylinder's axis
/// is [0,0,±1], so only the Z component matters.
///
/// Returns (z_min, z_max) of the cylinder's Z extent.
pub(crate) fn cyl_z_range(cyl: &CylinderParams) -> (f64, f64) {
    let z0 = cyl.center_bottom[2];
    let z1 = z0 + cyl.depth * cyl.direction[2];
    (z0.min(z1), z0.max(z1))
}

/// Check if two cylinder axes are parallel (within tolerance).
///
/// Two axes are parallel if |dot(a.direction, b.direction)| ≈ 1.
/// Non-parallel cylinder booleans produce elliptical intersection curves
/// and are not supported (Ref #1 Patrikalakis Ch.5).
pub(crate) fn cyls_parallel(a: &CylinderParams, b: &CylinderParams) -> bool {
    let dot = v3_dot(a.direction, b.direction);
    dot.abs() > 1.0 - TOL
}

// ── General-position SSI solvers ──────────────────────────────────────────

/// Compute SSI between an arbitrary plane and an arbitrary cylinder.
///
/// The plane is defined by a point and unit normal. The cylinder is defined by
/// a point on its axis, unit axis direction, radius, and height range along axis.
///
/// Returns:
/// - If plane ⊥ axis (cos_angle ≈ 1): a circle (if within height range)
/// - If plane ∥ axis (cos_angle ≈ 0): 0 or 2 line segments
/// - Oblique case: empty (ellipse curve type not yet supported, per A15.4)
pub(crate) fn plane_cylinder_ssi(
    plane_origin: [f64; 3],
    plane_normal: [f64; 3],
    cyl_origin: [f64; 3],
    cyl_axis: [f64; 3],
    cyl_radius: f64,
    cyl_height_range: (f64, f64),
) -> Vec<SSICurve> {
    let cos_angle = v3_dot(plane_normal, cyl_axis).abs();

    if cos_angle > 1.0 - TOL {
        // Perpendicular to axis: plane cuts a circle
        plane_cylinder_perp(
            plane_origin,
            plane_normal,
            cyl_origin,
            cyl_axis,
            cyl_radius,
            cyl_height_range,
        )
    } else if cos_angle < TOL {
        // Parallel to axis: plane cuts 0 or 2 line segments
        plane_cylinder_parallel(
            plane_origin,
            plane_normal,
            cyl_origin,
            cyl_axis,
            cyl_radius,
            cyl_height_range,
        )
    } else {
        // Oblique: produces an ellipse — not yet supported
        vec![]
    }
}

/// Plane perpendicular to cylinder axis → circle intersection.
fn plane_cylinder_perp(
    plane_origin: [f64; 3],
    plane_normal: [f64; 3],
    cyl_origin: [f64; 3],
    cyl_axis: [f64; 3],
    cyl_radius: f64,
    cyl_height_range: (f64, f64),
) -> Vec<SSICurve> {
    // Find where the axis pierces the plane.
    // Plane equation: (P - plane_origin) · plane_normal = 0
    // Axis: P = cyl_origin + t * cyl_axis
    // t = ((plane_origin - cyl_origin) · plane_normal) / (cyl_axis · plane_normal)
    let denom = v3_dot(cyl_axis, plane_normal);
    if denom.abs() < TOL {
        return vec![];
    }
    let t = v3_dot(v3_sub(plane_origin, cyl_origin), plane_normal) / denom;

    // Check height range
    if t < cyl_height_range.0 - TOL || t > cyl_height_range.1 + TOL {
        return vec![];
    }

    let center = v3_add(cyl_origin, v3_scale(cyl_axis, t));
    vec![SSICurve::Circle {
        center,
        normal: cyl_axis,
        radius: cyl_radius,
    }]
}

/// Plane parallel to cylinder axis → 0 or 2 line segments.
fn plane_cylinder_parallel(
    plane_origin: [f64; 3],
    plane_normal: [f64; 3],
    cyl_origin: [f64; 3],
    cyl_axis: [f64; 3],
    cyl_radius: f64,
    cyl_height_range: (f64, f64),
) -> Vec<SSICurve> {
    // Signed distance from cylinder axis to the plane.
    // Project (cyl_origin - plane_origin) onto plane_normal (component ⊥ to axis).
    let diff = v3_sub(cyl_origin, plane_origin);
    let d = v3_dot(diff, plane_normal);
    let d_abs = d.abs();

    if d_abs >= cyl_radius - TOL {
        return vec![];
    }

    // Direction along the cylinder axis for the line segments
    let line_dir = cyl_axis;

    // In the cross-section plane (perpendicular to cyl_axis), find the
    // perpendicular direction within the cutting plane.
    // This is: cross(cyl_axis, plane_normal), normalized.
    let perp = {
        let c = v3_cross(cyl_axis, plane_normal);
        let len = v3_length(c);
        if len < TOL {
            return vec![];
        }
        v3_scale(c, 1.0 / len)
    };

    let offset = (cyl_radius * cyl_radius - d * d).sqrt();

    // Midpoint: project cyl_origin onto the plane along plane_normal
    let mid = v3_sub(cyl_origin, v3_scale(plane_normal, d));

    let p1 = v3_add(mid, v3_scale(perp, offset));
    let p2 = v3_sub(mid, v3_scale(perp, offset));

    let h_min = cyl_height_range.0;
    let h_max = cyl_height_range.1;

    vec![
        SSICurve::Line {
            start: v3_add(p1, v3_scale(line_dir, h_min)),
            end: v3_add(p1, v3_scale(line_dir, h_max)),
        },
        SSICurve::Line {
            start: v3_add(p2, v3_scale(line_dir, h_min)),
            end: v3_add(p2, v3_scale(line_dir, h_max)),
        },
    ]
}

/// Compute SSI between two cylinders with parallel axes.
///
/// Returns 0 or 2 line segments at the circle-circle intersection points
/// in the cross-section perpendicular to the shared axis direction.
/// Returns empty for non-parallel axes (not yet supported).
pub(crate) fn cylinder_cylinder_ssi(
    cyl_a_origin: [f64; 3],
    cyl_a_axis: [f64; 3],
    cyl_a_radius: f64,
    cyl_b_origin: [f64; 3],
    cyl_b_axis: [f64; 3],
    cyl_b_radius: f64,
    height_range: (f64, f64),
) -> Vec<SSICurve> {
    // Check axes are parallel
    let dot = v3_dot(cyl_a_axis, cyl_b_axis).abs();
    if dot < 1.0 - TOL {
        return vec![]; // Non-parallel: not supported
    }

    // Project both origins into the plane perpendicular to cyl_a_axis
    // to get 2D circle-circle intersection.
    let diff = v3_sub(cyl_b_origin, cyl_a_origin);
    // Remove the component along the axis
    let along = v3_dot(diff, cyl_a_axis);
    let diff_perp = v3_sub(diff, v3_scale(cyl_a_axis, along));
    let d = v3_length(diff_perp);

    let r1 = cyl_a_radius;
    let r2 = cyl_b_radius;

    if d >= r1 + r2 - TOL || d <= (r1 - r2).abs() + TOL {
        return vec![];
    }

    if d < TOL {
        return vec![]; // Coaxial
    }

    // 2D circle-circle intersection in the perpendicular plane
    let a = (r1 * r1 - r2 * r2 + d * d) / (2.0 * d);
    let h_sq = r1 * r1 - a * a;
    if h_sq < 0.0 {
        return vec![];
    }
    let h = h_sq.sqrt();

    // Unit vectors in the perpendicular plane
    let u = v3_scale(diff_perp, 1.0 / d); // toward cyl_b
    let v = v3_cross(cyl_a_axis, u); // perpendicular in the cross-section

    let mid = v3_add(cyl_a_origin, v3_scale(u, a));

    let p1 = v3_add(mid, v3_scale(v, h));
    let p2 = v3_sub(mid, v3_scale(v, h));

    let h_min = height_range.0;
    let h_max = height_range.1;

    vec![
        SSICurve::Line {
            start: v3_add(p1, v3_scale(cyl_a_axis, h_min)),
            end: v3_add(p1, v3_scale(cyl_a_axis, h_max)),
        },
        SSICurve::Line {
            start: v3_add(p2, v3_scale(cyl_a_axis, h_min)),
            end: v3_add(p2, v3_scale(cyl_a_axis, h_max)),
        },
    ]
}

/// Compute SSI between a plane and a sphere.
///
/// The intersection of a plane and a sphere is a circle (or empty/point).
/// Returns a circle if the plane cuts through the sphere.
pub(crate) fn plane_sphere_ssi(
    plane_origin: [f64; 3],
    plane_normal: [f64; 3],
    sphere_center: [f64; 3],
    sphere_radius: f64,
) -> Vec<SSICurve> {
    // Signed distance from sphere center to plane
    let d = v3_dot(v3_sub(sphere_center, plane_origin), plane_normal);

    if d.abs() >= sphere_radius - TOL {
        return vec![]; // Disjoint or tangent (within tolerance)
    }

    let circle_radius = (sphere_radius * sphere_radius - d * d).sqrt();
    let circle_center = v3_sub(sphere_center, v3_scale(plane_normal, d));

    vec![SSICurve::Circle {
        center: circle_center,
        normal: plane_normal,
        radius: circle_radius,
    }]
}

/// Compute SSI between a plane and a cone.
///
/// The cone is defined by its apex, unit axis direction (from apex outward),
/// half-angle, and maximum height from apex along axis.
///
/// Returns:
/// - If plane ⊥ axis: a circle at the cut height (if within (0, max_height])
/// - Oblique case: empty (conic sections not yet supported, per A15.4)
pub(crate) fn plane_cone_ssi(
    plane_origin: [f64; 3],
    plane_normal: [f64; 3],
    cone_apex: [f64; 3],
    cone_axis: [f64; 3],
    half_angle: f64,
    max_height: f64,
) -> Vec<SSICurve> {
    let cos_angle = v3_dot(plane_normal, cone_axis).abs();

    if cos_angle > 1.0 - TOL {
        // Plane perpendicular to cone axis
        let denom = v3_dot(cone_axis, plane_normal);
        if denom.abs() < TOL {
            return vec![];
        }
        // Height along axis where the plane intersects
        let h = v3_dot(v3_sub(plane_origin, cone_apex), plane_normal) / denom;

        if h < TOL || h > max_height + TOL {
            return vec![];
        }

        let circle_radius = h * half_angle.tan();
        let circle_center = v3_add(cone_apex, v3_scale(cone_axis, h));

        vec![SSICurve::Circle {
            center: circle_center,
            normal: cone_axis,
            radius: circle_radius,
        }]
    } else {
        // Oblique: produces conic sections (ellipse/parabola/hyperbola)
        // Not yet supported — requires Ellipse/Conic curve type (A15.4)
        vec![]
    }
}

// ── Point-in-solid classification ──────────────────────────────────────────

/// Test if a point is strictly inside an axis-aligned box.
pub(crate) fn point_in_box(pt: [f64; 3], aabb: &Aabb) -> bool {
    pt[0] > aabb.min[0] + TOL
        && pt[0] < aabb.max[0] - TOL
        && pt[1] > aabb.min[1] + TOL
        && pt[1] < aabb.max[1] - TOL
        && pt[2] > aabb.min[2] + TOL
        && pt[2] < aabb.max[2] - TOL
}

/// Test if a point is strictly inside a cylinder (axis-generic).
///
/// Projects the point onto the cylinder's actual axis direction to check both
/// axial containment (within [0, depth]) and radial distance (within radius).
pub(crate) fn point_in_cylinder(pt: [f64; 3], cyl: &CylinderParams) -> bool {
    let dp = v3_sub(pt, cyl.center_bottom);
    let d = cyl.direction;
    let axial = v3_dot(dp, d);
    if axial < TOL || axial > cyl.depth - TOL {
        return false;
    }
    let proj = v3_sub(dp, v3_scale(d, axial));
    let dist_radial = v3_length(proj);
    dist_radial < cyl.radius - TOL
}

/// Test if a point is strictly inside a sphere.
pub(crate) fn point_in_sphere(pt: [f64; 3], center: [f64; 3], radius: f64) -> bool {
    let d = v3_length(v3_sub(pt, center));
    d < radius - TOL
}

/// Test if a point is strictly inside a cone.
///
/// The cone is defined by its apex, unit axis (from apex outward),
/// half-angle, and maximum height from apex along axis.
pub(crate) fn point_in_cone(
    pt: [f64; 3],
    apex: [f64; 3],
    axis: [f64; 3],
    half_angle: f64,
    max_height: f64,
) -> bool {
    let dp = v3_sub(pt, apex);
    let h = v3_dot(dp, axis);
    if h < TOL || h > max_height - TOL {
        return false;
    }
    let radial = v3_sub(dp, v3_scale(axis, h));
    let r = v3_length(radial);
    r < h * half_angle.tan() - TOL
}

// ── Aabb extraction ────────────────────────────────────────────────────────

/// Compute the AABB of a solid's vertices after rotating into a given frame.
///
/// Used by `box_cyl_boolean` to project a box into the cylinder's Z-aligned
/// frame so that XY/Z enclosure checks are valid for tilted extrude directions.
pub(crate) fn compute_rotated_box_aabb(solid: &WaffleSolid, m: &Mat3) -> Aabb {
    let mut min = [f64::INFINITY; 3];
    let mut max = [f64::NEG_INFINITY; 3];
    for vertex in &solid.arena.vertices {
        let p = mat3_mul_vec(m, vertex.position);
        for i in 0..3 {
            min[i] = min[i].min(p[i]);
            max[i] = max[i].max(p[i]);
        }
    }
    Aabb { min, max }
}

/// Extract the Aabb from a solid's vertex positions.
pub(crate) fn compute_box_aabb(solid: &WaffleSolid) -> Aabb {
    let mut min = [f64::INFINITY; 3];
    let mut max = [f64::NEG_INFINITY; 3];

    for vertex in &solid.arena.vertices {
        for i in 0..3 {
            min[i] = min[i].min(vertex.position[i]);
            max[i] = max[i].max(vertex.position[i]);
        }
    }

    Aabb { min, max }
}

/// Check if a cylinder is fully enclosed within an Aabb (in XY plane).
pub(crate) fn cyl_enclosed_in_box(cyl: &CylinderParams, aabb: &Aabb) -> bool {
    let cx = cyl.center_bottom[0];
    let cy = cyl.center_bottom[1];
    let r = cyl.radius;
    cx - r >= aabb.min[0] - TOL
        && cx + r <= aabb.max[0] + TOL
        && cy - r >= aabb.min[1] - TOL
        && cy + r <= aabb.max[1] + TOL
}

/// Check if box and cylinder are fully disjoint (no overlap in XY or Z).
pub(crate) fn box_cyl_disjoint(aabb: &Aabb, cyl: &CylinderParams) -> bool {
    let cx = cyl.center_bottom[0];
    let cy = cyl.center_bottom[1];
    let r = cyl.radius;

    // Check XY separation (circle vs Aabb)
    let closest_x = cx.max(aabb.min[0]).min(aabb.max[0]);
    let closest_y = cy.max(aabb.min[1]).min(aabb.max[1]);
    let dx = cx - closest_x;
    let dy = cy - closest_y;
    if dx * dx + dy * dy > r * r + TOL {
        return true;
    }

    // Check Z overlap — use tolerance so Z-touching surfaces are NOT disjoint
    let (cyl_z_min, cyl_z_max) = cyl_z_range(cyl);
    if cyl_z_max < aabb.min[2] - TOL || cyl_z_min > aabb.max[2] + TOL {
        return true;
    }

    false
}

/// Check if two Z-axis cylinders are disjoint (no overlap in XY).
pub(crate) fn cyls_disjoint(a: &CylinderParams, b: &CylinderParams) -> bool {
    let dx = a.center_bottom[0] - b.center_bottom[0];
    let dy = a.center_bottom[1] - b.center_bottom[1];
    let d = (dx * dx + dy * dy).sqrt();
    d >= a.radius + b.radius - TOL
}

// ── Tests ──────────────────────────────────────────────────────────────────

#[cfg(test)]
mod tests {
    use super::*;
    use std::f64::consts::FRAC_1_SQRT_2;

    const EPS: f64 = 1e-6;

    // ── Plane-Cylinder SSI ────────────────────────────────────────────

    #[test]
    fn test_plane_cylinder_perpendicular() {
        // Z-aligned cylinder, plane at z=5 perpendicular to Z
        let curves = plane_cylinder_ssi(
            [0.0, 0.0, 5.0], // plane origin
            [0.0, 0.0, 1.0], // plane normal
            [0.0, 0.0, 0.0], // cyl origin
            [0.0, 0.0, 1.0], // cyl axis
            3.0,             // radius
            (0.0, 10.0),     // height range
        );
        assert_eq!(curves.len(), 1);
        if let SSICurve::Circle { center, radius, .. } = &curves[0] {
            assert!(center[0].abs() < EPS);
            assert!(center[1].abs() < EPS);
            assert!((center[2] - 5.0).abs() < EPS);
            assert!((radius - 3.0).abs() < EPS);
        } else {
            panic!("Expected Circle");
        }
    }

    #[test]
    fn test_plane_cylinder_parallel() {
        // Z-aligned cylinder, vertical plane at x=1 with normal [1,0,0]
        let curves = plane_cylinder_ssi(
            [1.0, 0.0, 0.0], // plane origin
            [1.0, 0.0, 0.0], // plane normal
            [0.0, 0.0, 0.0], // cyl origin
            [0.0, 0.0, 1.0], // cyl axis
            3.0,             // radius
            (0.0, 10.0),     // height range
        );
        assert_eq!(curves.len(), 2);
        let sqrt8 = 8.0_f64.sqrt();
        for curve in &curves {
            if let SSICurve::Line { start, end } = curve {
                // x should be 1.0 (on the plane)
                assert!((start[0] - 1.0).abs() < EPS, "x={}", start[0]);
                // y should be ±sqrt(r²-d²) = ±sqrt(9-1) = ±sqrt(8)
                assert!((start[1].abs() - sqrt8).abs() < EPS, "y={}", start[1]);
                assert!(start[2].abs() < EPS, "start z={}", start[2]);
                assert!((end[2] - 10.0).abs() < EPS, "end z={}", end[2]);
            } else {
                panic!("Expected Line");
            }
        }
    }

    #[test]
    fn test_plane_cylinder_disjoint() {
        // Plane at z=15, cylinder goes from z=0 to z=10
        let curves = plane_cylinder_ssi(
            [0.0, 0.0, 15.0],
            [0.0, 0.0, 1.0],
            [0.0, 0.0, 0.0],
            [0.0, 0.0, 1.0],
            3.0,
            (0.0, 10.0),
        );
        assert!(curves.is_empty());
    }

    #[test]
    fn test_plane_cylinder_tilted_axis() {
        // Cylinder along [1,1,0]/sqrt(2), plane perpendicular to that axis
        let axis = [FRAC_1_SQRT_2, FRAC_1_SQRT_2, 0.0];
        let curves = plane_cylinder_ssi(
            [3.0, 3.0, 0.0], // plane origin: on axis at t=3*sqrt(2)
            axis,            // plane normal = axis (perpendicular cut)
            [0.0, 0.0, 0.0], // cyl origin
            axis,            // cyl axis
            2.0,             // radius
            (0.0, 10.0),     // height range (t ∈ [0, 10])
        );
        assert_eq!(curves.len(), 1);
        if let SSICurve::Circle { center, radius, .. } = &curves[0] {
            // t = ((3,3,0) - (0,0,0)) · axis / (axis · axis) = (3/√2 + 3/√2) = 3√2 ≈ 4.24
            // center = origin + t * axis = (3, 3, 0)
            assert!((center[0] - 3.0).abs() < EPS, "cx={}", center[0]);
            assert!((center[1] - 3.0).abs() < EPS, "cy={}", center[1]);
            assert!(center[2].abs() < EPS, "cz={}", center[2]);
            assert!((radius - 2.0).abs() < EPS);
        } else {
            panic!("Expected Circle");
        }
    }

    #[test]
    fn test_plane_cylinder_parallel_disjoint() {
        // Plane at x=5, cylinder at origin with r=3 → distance 5 > 3 → empty
        let curves = plane_cylinder_ssi(
            [5.0, 0.0, 0.0],
            [1.0, 0.0, 0.0],
            [0.0, 0.0, 0.0],
            [0.0, 0.0, 1.0],
            3.0,
            (0.0, 10.0),
        );
        assert!(curves.is_empty());
    }

    #[test]
    fn test_plane_cylinder_oblique_empty() {
        // Oblique plane (45°) → not supported yet → empty
        let normal = [FRAC_1_SQRT_2, 0.0, FRAC_1_SQRT_2];
        let curves = plane_cylinder_ssi(
            [0.0, 0.0, 0.0],
            normal,
            [0.0, 0.0, 0.0],
            [0.0, 0.0, 1.0],
            3.0,
            (0.0, 10.0),
        );
        assert!(curves.is_empty());
    }

    // ── Cylinder-Cylinder SSI ─────────────────────────────────────────

    #[test]
    fn test_cylinder_cylinder_overlapping() {
        // Two Z-aligned cylinders, r=3 each, centers 3 apart
        let curves = cylinder_cylinder_ssi(
            [0.0, 0.0, 0.0],
            [0.0, 0.0, 1.0],
            3.0,
            [3.0, 0.0, 0.0],
            [0.0, 0.0, 1.0],
            3.0,
            (0.0, 10.0),
        );
        assert_eq!(curves.len(), 2);
        for curve in &curves {
            if let SSICurve::Line { start, .. } = curve {
                assert!((start[0] - 1.5).abs() < EPS, "x={}", start[0]);
                let expected_y = (9.0 - 2.25_f64).sqrt();
                assert!((start[1].abs() - expected_y).abs() < EPS, "y={}", start[1]);
            } else {
                panic!("Expected Line");
            }
        }
    }

    #[test]
    fn test_cylinder_cylinder_disjoint() {
        // Two Z-aligned cylinders, r=1 each, centers 5 apart → disjoint
        let curves = cylinder_cylinder_ssi(
            [0.0, 0.0, 0.0],
            [0.0, 0.0, 1.0],
            1.0,
            [5.0, 0.0, 0.0],
            [0.0, 0.0, 1.0],
            1.0,
            (0.0, 10.0),
        );
        assert!(curves.is_empty());
    }

    #[test]
    fn test_cylinder_cylinder_non_parallel() {
        // Skew axes → not supported → empty
        let curves = cylinder_cylinder_ssi(
            [0.0, 0.0, 0.0],
            [0.0, 0.0, 1.0],
            3.0,
            [3.0, 0.0, 0.0],
            [1.0, 0.0, 0.0],
            3.0,
            (0.0, 10.0),
        );
        assert!(curves.is_empty());
    }

    // ── Plane-Sphere SSI ──────────────────────────────────────────────

    #[test]
    fn test_plane_sphere_through_center() {
        // Plane through sphere center → circle with r = sphere_r
        let curves = plane_sphere_ssi(
            [0.0, 0.0, 0.0], // plane origin at sphere center
            [0.0, 0.0, 1.0], // plane normal
            [0.0, 0.0, 0.0], // sphere center
            5.0,             // sphere radius
        );
        assert_eq!(curves.len(), 1);
        if let SSICurve::Circle { center, radius, .. } = &curves[0] {
            assert!(center[0].abs() < EPS);
            assert!(center[1].abs() < EPS);
            assert!(center[2].abs() < EPS);
            assert!((radius - 5.0).abs() < EPS);
        } else {
            panic!("Expected Circle");
        }
    }

    #[test]
    fn test_plane_sphere_offset() {
        // Plane at z=3, sphere at origin r=5 → circle at z=3, r=sqrt(25-9)=4
        let curves = plane_sphere_ssi([0.0, 0.0, 3.0], [0.0, 0.0, 1.0], [0.0, 0.0, 0.0], 5.0);
        assert_eq!(curves.len(), 1);
        if let SSICurve::Circle { center, radius, .. } = &curves[0] {
            assert!(center[0].abs() < EPS);
            assert!(center[1].abs() < EPS);
            assert!((center[2] - 3.0).abs() < EPS);
            assert!((radius - 4.0).abs() < EPS);
        } else {
            panic!("Expected Circle");
        }
    }

    #[test]
    fn test_plane_sphere_tangent() {
        // Plane at z=5 (tangent) → within tolerance → empty
        let curves = plane_sphere_ssi([0.0, 0.0, 5.0], [0.0, 0.0, 1.0], [0.0, 0.0, 0.0], 5.0);
        assert!(curves.is_empty());
    }

    #[test]
    fn test_plane_sphere_disjoint() {
        // Plane at z=10, sphere r=5 → d=10 > 5 → empty
        let curves = plane_sphere_ssi([0.0, 0.0, 10.0], [0.0, 0.0, 1.0], [0.0, 0.0, 0.0], 5.0);
        assert!(curves.is_empty());
    }

    #[test]
    fn test_plane_sphere_tilted_plane() {
        // Sphere at (1,2,3) r=5, plane through sphere center with normal [1,0,0]
        let curves = plane_sphere_ssi(
            [1.0, 0.0, 0.0], // plane at x=1
            [1.0, 0.0, 0.0], // normal
            [1.0, 2.0, 3.0], // sphere center (x=1, on the plane)
            5.0,
        );
        assert_eq!(curves.len(), 1);
        if let SSICurve::Circle { center, radius, .. } = &curves[0] {
            // d = (1-1)*1 = 0 → circle at sphere center with full radius
            assert!((center[0] - 1.0).abs() < EPS);
            assert!((center[1] - 2.0).abs() < EPS);
            assert!((center[2] - 3.0).abs() < EPS);
            assert!((radius - 5.0).abs() < EPS);
        } else {
            panic!("Expected Circle");
        }
    }

    // ── Plane-Cone SSI ────────────────────────────────────────────────

    #[test]
    fn test_plane_cone_perp_at_height() {
        use std::f64::consts::FRAC_PI_4;
        // Cone: apex at origin, axis +Z, half_angle=45°, max_height=10
        // Plane at z=5 → circle at (0,0,5) with r = 5*tan(45°) = 5
        let curves = plane_cone_ssi(
            [0.0, 0.0, 5.0],
            [0.0, 0.0, 1.0],
            [0.0, 0.0, 0.0], // apex
            [0.0, 0.0, 1.0], // axis
            FRAC_PI_4,       // 45°
            10.0,
        );
        assert_eq!(curves.len(), 1);
        if let SSICurve::Circle { center, radius, .. } = &curves[0] {
            assert!(center[0].abs() < EPS);
            assert!(center[1].abs() < EPS);
            assert!((center[2] - 5.0).abs() < EPS);
            assert!((radius - 5.0).abs() < EPS);
        } else {
            panic!("Expected Circle");
        }
    }

    #[test]
    fn test_plane_cone_at_apex() {
        use std::f64::consts::FRAC_PI_4;
        // Plane at z=0 (the apex) → h≈0 → empty
        let curves = plane_cone_ssi(
            [0.0, 0.0, 0.0],
            [0.0, 0.0, 1.0],
            [0.0, 0.0, 0.0],
            [0.0, 0.0, 1.0],
            FRAC_PI_4,
            10.0,
        );
        assert!(curves.is_empty());
    }

    #[test]
    fn test_plane_cone_below_apex() {
        use std::f64::consts::FRAC_PI_4;
        // Plane at z=-5 → h=-5 < 0 → empty
        let curves = plane_cone_ssi(
            [0.0, 0.0, -5.0],
            [0.0, 0.0, 1.0],
            [0.0, 0.0, 0.0],
            [0.0, 0.0, 1.0],
            FRAC_PI_4,
            10.0,
        );
        assert!(curves.is_empty());
    }

    #[test]
    fn test_plane_cone_above_max() {
        use std::f64::consts::FRAC_PI_4;
        // Plane at z=15 → h=15 > max_height=10 → empty
        let curves = plane_cone_ssi(
            [0.0, 0.0, 15.0],
            [0.0, 0.0, 1.0],
            [0.0, 0.0, 0.0],
            [0.0, 0.0, 1.0],
            FRAC_PI_4,
            10.0,
        );
        assert!(curves.is_empty());
    }

    #[test]
    fn test_plane_cone_narrow_angle() {
        // half_angle = 30° (π/6), cut at h=4 → r = 4*tan(30°) ≈ 2.309
        let half = std::f64::consts::FRAC_PI_6;
        let curves = plane_cone_ssi(
            [0.0, 0.0, 4.0],
            [0.0, 0.0, 1.0],
            [0.0, 0.0, 0.0],
            [0.0, 0.0, 1.0],
            half,
            10.0,
        );
        assert_eq!(curves.len(), 1);
        if let SSICurve::Circle { radius, .. } = &curves[0] {
            let expected = 4.0 * half.tan();
            assert!(
                (radius - expected).abs() < EPS,
                "r={} expected={}",
                radius,
                expected
            );
        } else {
            panic!("Expected Circle");
        }
    }

    #[test]
    fn test_plane_cone_oblique_empty() {
        use std::f64::consts::FRAC_PI_4;
        // Oblique plane → not supported → empty
        let normal = [FRAC_1_SQRT_2, 0.0, FRAC_1_SQRT_2];
        let curves = plane_cone_ssi(
            [0.0, 0.0, 5.0],
            normal,
            [0.0, 0.0, 0.0],
            [0.0, 0.0, 1.0],
            FRAC_PI_4,
            10.0,
        );
        assert!(curves.is_empty());
    }

    // ── Point-in-Sphere ───────────────────────────────────────────────

    #[test]
    fn test_point_in_sphere_inside() {
        assert!(point_in_sphere([1.0, 0.0, 0.0], [0.0, 0.0, 0.0], 5.0));
    }

    #[test]
    fn test_point_in_sphere_outside() {
        assert!(!point_in_sphere([6.0, 0.0, 0.0], [0.0, 0.0, 0.0], 5.0));
    }

    #[test]
    fn test_point_in_sphere_boundary() {
        // On the surface → not strictly inside
        assert!(!point_in_sphere([5.0, 0.0, 0.0], [0.0, 0.0, 0.0], 5.0));
    }

    // ── Point-in-Cone ─────────────────────────────────────────────────

    #[test]
    fn test_point_in_cone_inside() {
        use std::f64::consts::FRAC_PI_4;
        // Point at (0, 0, 5) — on axis, clearly inside a 45° cone
        assert!(point_in_cone(
            [0.0, 0.0, 5.0],
            [0.0, 0.0, 0.0],
            [0.0, 0.0, 1.0],
            FRAC_PI_4,
            10.0,
        ));
    }

    #[test]
    fn test_point_in_cone_outside() {
        use std::f64::consts::FRAC_PI_4;
        // Point at (10, 0, 5) — radial distance 10, max_r at h=5 is 5 → outside
        assert!(!point_in_cone(
            [10.0, 0.0, 5.0],
            [0.0, 0.0, 0.0],
            [0.0, 0.0, 1.0],
            FRAC_PI_4,
            10.0,
        ));
    }

    #[test]
    fn test_point_in_cone_at_apex() {
        use std::f64::consts::FRAC_PI_4;
        // Point at apex → h≈0 → not strictly inside
        assert!(!point_in_cone(
            [0.0, 0.0, 0.0],
            [0.0, 0.0, 0.0],
            [0.0, 0.0, 1.0],
            FRAC_PI_4,
            10.0,
        ));
    }

    #[test]
    fn test_point_in_cone_above_max_height() {
        use std::f64::consts::FRAC_PI_4;
        // Point at (0, 0, 11) — above max_height=10
        assert!(!point_in_cone(
            [0.0, 0.0, 11.0],
            [0.0, 0.0, 0.0],
            [0.0, 0.0, 1.0],
            FRAC_PI_4,
            10.0,
        ));
    }

    #[test]
    fn test_point_in_cone_tilted_axis() {
        use std::f64::consts::FRAC_PI_4;
        // Cone with apex at (1,1,1), axis [1,0,0], half_angle=45°, max_h=10
        // Point at (6, 1, 1) — on axis at h=5, r=0 < 5 → inside
        assert!(point_in_cone(
            [6.0, 1.0, 1.0],
            [1.0, 1.0, 1.0],
            [1.0, 0.0, 0.0],
            FRAC_PI_4,
            10.0,
        ));
    }
}
