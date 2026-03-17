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

use crate::types::KernelError;
use crate::units::TAU_COINCIDENT;
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
    /// A full ellipse in 3D (oblique plane-cylinder intersection).
    Ellipse {
        center: [f64; 3],
        normal: [f64; 3],
        major_axis: [f64; 3],
        semi_major: f64,
        semi_minor: f64,
    },
}

/// Axis-aligned bounding box.
#[derive(Debug, Clone)]
pub(crate) struct Aabb {
    pub min: [f64; 3],
    pub max: [f64; 3],
}

// ── Tolerance ─────────────────────────────────────────────────────────────
// All SSI tolerances derive from units.rs (A14.3).

const TOL: f64 = TAU_COINCIDENT;

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
) -> Result<Vec<SSICurve>, KernelError> {
    let cos_angle = v3_dot(plane_normal, cyl_axis).abs();

    if cos_angle > 1.0 - TOL {
        // Perpendicular to axis: plane cuts a circle
        Ok(plane_cylinder_perp(
            plane_origin,
            plane_normal,
            cyl_origin,
            cyl_axis,
            cyl_radius,
            cyl_height_range,
        ))
    } else if cos_angle < TOL {
        // Parallel to axis: plane cuts 0 or 2 line segments
        Ok(plane_cylinder_parallel(
            plane_origin,
            plane_normal,
            cyl_origin,
            cyl_axis,
            cyl_radius,
            cyl_height_range,
        ))
    } else {
        // Oblique: produces an ellipse (Patrikalakis Ch.5)
        Ok(plane_cylinder_oblique(
            plane_origin,
            plane_normal,
            cyl_origin,
            cyl_axis,
            cyl_radius,
            cyl_height_range,
        ))
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

/// Plane oblique to cylinder axis → ellipse intersection.
///
/// The intersection of a plane that is neither perpendicular nor parallel to a
/// cylinder axis is an ellipse with:
/// - semi_minor = cylinder radius
/// - semi_major = radius / sin(gamma), where gamma is the angle between the
///   plane and the axis
/// - major axis direction = projection of cylinder axis onto the cutting plane
///
/// Ref: Patrikalakis Ch.5 — oblique plane-cylinder intersection.
fn plane_cylinder_oblique(
    plane_origin: [f64; 3],
    plane_normal: [f64; 3],
    cyl_origin: [f64; 3],
    cyl_axis: [f64; 3],
    cyl_radius: f64,
    cyl_height_range: (f64, f64),
) -> Vec<SSICurve> {
    let dot_wn = v3_dot(cyl_axis, plane_normal);

    // sin_gamma = sqrt(1 - cos²(angle between axis and normal))
    let cos_angle = dot_wn.abs();
    let sin_gamma = (1.0 - cos_angle * cos_angle).max(0.0).sqrt();

    if sin_gamma < TOL {
        // Near-parallel: degenerate ellipse (infinite semi_major) → empty
        return vec![];
    }

    // Find where the axis pierces the plane:
    // t = ((plane_origin - cyl_origin) · plane_normal) / (cyl_axis · plane_normal)
    if dot_wn.abs() < TOL {
        return vec![];
    }
    let t = v3_dot(v3_sub(plane_origin, cyl_origin), plane_normal) / dot_wn;

    // Check height range
    if t < cyl_height_range.0 - TOL || t > cyl_height_range.1 + TOL {
        return vec![];
    }

    let center = v3_add(cyl_origin, v3_scale(cyl_axis, t));

    let semi_minor = cyl_radius;
    let semi_major = cyl_radius / sin_gamma;

    // Major axis direction: projection of cylinder axis onto cutting plane
    // major = normalize(W - (W·N)*N)
    let proj = v3_sub(cyl_axis, v3_scale(plane_normal, dot_wn));
    let proj_len = v3_length(proj);
    if proj_len < TOL {
        return vec![];
    }
    let major_axis = v3_scale(proj, 1.0 / proj_len);

    vec![SSICurve::Ellipse {
        center,
        normal: plane_normal,
        major_axis,
        semi_major,
        semi_minor,
    }]
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
) -> Result<Vec<SSICurve>, KernelError> {
    // Check axes are parallel
    let dot = v3_dot(cyl_a_axis, cyl_b_axis).abs();
    if dot < 1.0 - TOL {
        return Err(KernelError::NotSupported {
            operation: "cylinder-cylinder SSI: non-parallel axes produce degree-4 curve"
                .to_string(),
        });
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
        return Ok(vec![]);
    }

    if d < TOL {
        return Ok(vec![]); // Coaxial
    }

    // 2D circle-circle intersection in the perpendicular plane
    let a = (r1 * r1 - r2 * r2 + d * d) / (2.0 * d);
    let h_sq = r1 * r1 - a * a;
    if h_sq < 0.0 {
        return Ok(vec![]);
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

    Ok(vec![
        SSICurve::Line {
            start: v3_add(p1, v3_scale(cyl_a_axis, h_min)),
            end: v3_add(p1, v3_scale(cyl_a_axis, h_max)),
        },
        SSICurve::Line {
            start: v3_add(p2, v3_scale(cyl_a_axis, h_min)),
            end: v3_add(p2, v3_scale(cyl_a_axis, h_max)),
        },
    ])
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
) -> Result<Vec<SSICurve>, KernelError> {
    // Signed distance from sphere center to plane
    let d = v3_dot(v3_sub(sphere_center, plane_origin), plane_normal);

    if d.abs() >= sphere_radius - TOL {
        return Ok(vec![]); // Disjoint or tangent (within tolerance)
    }

    let circle_radius = (sphere_radius * sphere_radius - d * d).sqrt();
    let circle_center = v3_sub(sphere_center, v3_scale(plane_normal, d));

    Ok(vec![SSICurve::Circle {
        center: circle_center,
        normal: plane_normal,
        radius: circle_radius,
    }])
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
) -> Result<Vec<SSICurve>, KernelError> {
    let cos_angle = v3_dot(plane_normal, cone_axis).abs();

    if cos_angle > 1.0 - TOL {
        // Plane perpendicular to cone axis
        let denom = v3_dot(cone_axis, plane_normal);
        if denom.abs() < TOL {
            return Ok(vec![]);
        }
        // Height along axis where the plane intersects
        let h = v3_dot(v3_sub(plane_origin, cone_apex), plane_normal) / denom;

        if h < TOL || h > max_height + TOL {
            return Ok(vec![]);
        }

        let circle_radius = h * half_angle.tan();
        let circle_center = v3_add(cone_apex, v3_scale(cone_axis, h));

        Ok(vec![SSICurve::Circle {
            center: circle_center,
            normal: cone_axis,
            radius: circle_radius,
        }])
    } else {
        // Oblique: produces conic sections (ellipse/parabola/hyperbola)
        // Requires Ellipse/Conic SSICurve variant (A15.4)
        Err(KernelError::NotSupported {
            operation: "plane-cone SSI: oblique cut produces conic section".to_string(),
        })
    }
}

/// Compute SSI between two spheres.
///
/// The intersection of two spheres is a circle (if they overlap) or empty.
/// The circle lies in a plane perpendicular to the line connecting the centers.
///
/// Ref: Patrikalakis Ch.5 — sphere-sphere intersection.
pub(crate) fn sphere_sphere_ssi(
    center_a: [f64; 3],
    radius_a: f64,
    center_b: [f64; 3],
    radius_b: f64,
) -> Result<Vec<SSICurve>, KernelError> {
    let diff = v3_sub(center_b, center_a);
    let d = v3_length(diff);

    // Disjoint: centers too far apart
    if d >= radius_a + radius_b - TOL {
        return Ok(vec![]);
    }

    // One sphere enclosed in the other
    if d <= (radius_a - radius_b).abs() + TOL {
        return Ok(vec![]);
    }

    // Degenerate: coincident centers with same radius (infinite intersection)
    if d < TOL {
        return Ok(vec![]);
    }

    // Distance from center_a to the intersection plane along the axis
    let a = (radius_a * radius_a - radius_b * radius_b + d * d) / (2.0 * d);

    // Radius of the intersection circle
    let h_sq = radius_a * radius_a - a * a;
    if h_sq < 0.0 {
        return Ok(vec![]);
    }
    let circle_radius = h_sq.sqrt();

    // Unit vector from A to B
    let axis = v3_scale(diff, 1.0 / d);

    // Center of the intersection circle
    let circle_center = v3_add(center_a, v3_scale(axis, a));

    Ok(vec![SSICurve::Circle {
        center: circle_center,
        normal: axis,
        radius: circle_radius,
    }])
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
        )
        .unwrap();
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
        )
        .unwrap();
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
        )
        .unwrap();
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
        )
        .unwrap();
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
        )
        .unwrap();
        assert!(curves.is_empty());
    }

    #[test]
    fn test_plane_cylinder_oblique_45deg() {
        // 45° plane → ellipse with semi_major = r*sqrt(2), semi_minor = r
        let normal = [FRAC_1_SQRT_2, 0.0, FRAC_1_SQRT_2];
        let curves = plane_cylinder_ssi(
            [0.0, 0.0, 5.0], // plane origin at z=5
            normal,
            [0.0, 0.0, 0.0], // cyl origin
            [0.0, 0.0, 1.0], // cyl axis
            3.0,             // radius
            (0.0, 10.0),     // height range
        )
        .unwrap();
        assert_eq!(curves.len(), 1);
        if let SSICurve::Ellipse {
            center,
            semi_major,
            semi_minor,
            major_axis,
            ..
        } = &curves[0]
        {
            // sin(45°) = 1/√2, so semi_major = 3 / (1/√2) = 3√2
            let expected_major = 3.0 * std::f64::consts::SQRT_2;
            assert!(
                (semi_major - expected_major).abs() < EPS,
                "a={}",
                semi_major
            );
            assert!((semi_minor - 3.0).abs() < EPS, "b={}", semi_minor);
            // Center should be on the axis at the plane intersection
            assert!(center[0].abs() < EPS);
            assert!(center[1].abs() < EPS);
            assert!((center[2] - 5.0).abs() < EPS, "cz={}", center[2]);
            // Major axis should be projection of Z onto plane → along Z component in plane
            // W=[0,0,1], N=[1/√2,0,1/√2], proj = [0,0,1] - (1/√2)*[1/√2,0,1/√2]
            //   = [0,0,1] - [0.5, 0, 0.5] = [-0.5, 0, 0.5], normalized: [-1/√2, 0, 1/√2]
            assert!(
                (major_axis[0] - (-FRAC_1_SQRT_2)).abs() < EPS,
                "mx={}",
                major_axis[0]
            );
            assert!(major_axis[1].abs() < EPS, "my={}", major_axis[1]);
            assert!(
                (major_axis[2] - FRAC_1_SQRT_2).abs() < EPS,
                "mz={}",
                major_axis[2]
            );
        } else {
            panic!("Expected Ellipse, got {:?}", curves[0]);
        }
    }

    #[test]
    fn test_plane_cylinder_oblique_30deg() {
        // Plane normal at 30° from Z: cos_angle = cos(30°) = √3/2
        // sin_gamma = sin(30°) = 0.5 → semi_major = r / 0.5 = 2r
        let cos30 = (3.0_f64).sqrt() / 2.0;
        let sin30 = 0.5_f64;
        let normal = [sin30, 0.0, cos30]; // 30° tilt from Z in XZ plane
        let curves = plane_cylinder_ssi(
            [0.0, 0.0, 5.0],
            normal,
            [0.0, 0.0, 0.0],
            [0.0, 0.0, 1.0],
            3.0,
            (0.0, 10.0),
        )
        .unwrap();
        assert_eq!(curves.len(), 1);
        if let SSICurve::Ellipse {
            semi_major,
            semi_minor,
            ..
        } = &curves[0]
        {
            // sin_gamma = sin(30°) = 0.5, semi_major = 3 / 0.5 = 6
            assert!((semi_major - 6.0).abs() < EPS, "a={}", semi_major);
            assert!((semi_minor - 3.0).abs() < EPS, "b={}", semi_minor);
        } else {
            panic!("Expected Ellipse");
        }
    }

    #[test]
    fn test_plane_cylinder_oblique_near_perp() {
        // Nearly perpendicular (89°) → cos_angle ≈ cos(1°) ≈ 0.9998
        // sin_gamma ≈ sin(1°) ≈ 0.01745 — nearly circular ellipse
        // This should still be handled as oblique (not perp, which requires cos > 1 - TOL)
        let angle = 89.0_f64.to_radians(); // angle between plane normal and axis
        let normal = [angle.sin(), 0.0, angle.cos()];
        let curves = plane_cylinder_ssi(
            [0.0, 0.0, 5.0],
            normal,
            [0.0, 0.0, 0.0],
            [0.0, 0.0, 1.0],
            3.0,
            (0.0, 10.0),
        )
        .unwrap();
        assert_eq!(curves.len(), 1);
        if let SSICurve::Ellipse {
            semi_major,
            semi_minor,
            ..
        } = &curves[0]
        {
            // Nearly circular: semi_major ≈ semi_minor * (1/sin(1°))
            assert!((semi_minor - 3.0).abs() < EPS);
            // semi_major should be slightly larger than semi_minor
            assert!(*semi_major > *semi_minor);
            // sin(1°) ≈ 0.01745 → semi_major ≈ 3/0.01745 ≈ 171.9
            let sin_gamma = angle.sin();
            let expected = 3.0 / sin_gamma;
            assert!(
                (semi_major - expected).abs() < 0.1,
                "a={} expected={}",
                semi_major,
                expected
            );
        } else {
            panic!("Expected Ellipse");
        }
    }

    #[test]
    fn test_plane_cylinder_oblique_tilted_axis() {
        // Non-Z-aligned cylinder: axis = [1,0,0] (along X), radius 2
        // Plane normal = [0,0,1] (XY plane at z=0)
        // cos_angle = |[1,0,0]·[0,0,1]| = 0 → parallel case (sin_gamma = 1)
        // Actually need oblique: use normal = [FRAC_1_SQRT_2, 0, FRAC_1_SQRT_2]
        let cyl_axis = [1.0, 0.0, 0.0];
        let normal = [FRAC_1_SQRT_2, 0.0, FRAC_1_SQRT_2];
        let curves = plane_cylinder_ssi(
            [5.0, 0.0, 0.0], // plane at x=5
            normal,
            [0.0, 0.0, 0.0], // cyl origin
            cyl_axis,
            2.0,         // radius
            (0.0, 10.0), // height range
        )
        .unwrap();
        assert_eq!(curves.len(), 1);
        if let SSICurve::Ellipse {
            center,
            semi_major,
            semi_minor,
            ..
        } = &curves[0]
        {
            // cos_angle = |[1,0,0]·[1/√2,0,1/√2]| = 1/√2
            // sin_gamma = 1/√2 → semi_major = 2/sin(45°) = 2√2
            let expected_major = 2.0 * std::f64::consts::SQRT_2;
            assert!(
                (semi_major - expected_major).abs() < EPS,
                "a={}",
                semi_major
            );
            assert!((semi_minor - 2.0).abs() < EPS, "b={}", semi_minor);
            // Center: axis line intersects plane
            // t = ((5,0,0)-(0,0,0))·[1/√2,0,1/√2] / ([1,0,0]·[1/√2,0,1/√2])
            //   = (5/√2) / (1/√2) = 5
            // center = (0,0,0) + 5*(1,0,0) = (5,0,0)
            assert!((center[0] - 5.0).abs() < EPS, "cx={}", center[0]);
            assert!(center[1].abs() < EPS, "cy={}", center[1]);
            assert!(center[2].abs() < EPS, "cz={}", center[2]);
        } else {
            panic!("Expected Ellipse");
        }
    }

    #[test]
    fn test_plane_cylinder_oblique_out_of_range() {
        // Plane at z=15, cylinder height 0..10 → center at t=15, outside range → empty
        let normal = [FRAC_1_SQRT_2, 0.0, FRAC_1_SQRT_2];
        let curves = plane_cylinder_ssi(
            [0.0, 0.0, 15.0],
            normal,
            [0.0, 0.0, 0.0],
            [0.0, 0.0, 1.0],
            3.0,
            (0.0, 10.0),
        )
        .unwrap();
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
        )
        .unwrap();
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
        )
        .unwrap();
        assert!(curves.is_empty());
    }

    #[test]
    fn test_cylinder_cylinder_non_parallel() {
        // Skew axes → not supported → Err(NotSupported)
        let result = cylinder_cylinder_ssi(
            [0.0, 0.0, 0.0],
            [0.0, 0.0, 1.0],
            3.0,
            [3.0, 0.0, 0.0],
            [1.0, 0.0, 0.0],
            3.0,
            (0.0, 10.0),
        );
        assert!(matches!(result, Err(KernelError::NotSupported { .. })));
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
        )
        .unwrap();
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
        let curves =
            plane_sphere_ssi([0.0, 0.0, 3.0], [0.0, 0.0, 1.0], [0.0, 0.0, 0.0], 5.0).unwrap();
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
        let curves =
            plane_sphere_ssi([0.0, 0.0, 5.0], [0.0, 0.0, 1.0], [0.0, 0.0, 0.0], 5.0).unwrap();
        assert!(curves.is_empty());
    }

    #[test]
    fn test_plane_sphere_disjoint() {
        // Plane at z=10, sphere r=5 → d=10 > 5 → empty
        let curves =
            plane_sphere_ssi([0.0, 0.0, 10.0], [0.0, 0.0, 1.0], [0.0, 0.0, 0.0], 5.0).unwrap();
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
        )
        .unwrap();
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
        )
        .unwrap();
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
        )
        .unwrap();
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
        )
        .unwrap();
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
        )
        .unwrap();
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
        )
        .unwrap();
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
        // Oblique plane → not supported → Err(NotSupported)
        let normal = [FRAC_1_SQRT_2, 0.0, FRAC_1_SQRT_2];
        let result = plane_cone_ssi(
            [0.0, 0.0, 5.0],
            normal,
            [0.0, 0.0, 0.0],
            [0.0, 0.0, 1.0],
            FRAC_PI_4,
            10.0,
        );
        assert!(matches!(result, Err(KernelError::NotSupported { .. })));
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

    // ── Sphere-Sphere SSI ─────────────────────────────────────────────

    #[test]
    fn test_sphere_sphere_overlapping() {
        // Two spheres, r=5 each, centers 6 apart along X
        let curves = sphere_sphere_ssi([0.0, 0.0, 0.0], 5.0, [6.0, 0.0, 0.0], 5.0).unwrap();
        assert_eq!(curves.len(), 1);
        if let SSICurve::Circle {
            center,
            normal,
            radius,
        } = &curves[0]
        {
            // a = (25 - 25 + 36) / 12 = 3, so center at (3, 0, 0)
            assert!((center[0] - 3.0).abs() < EPS, "cx={}", center[0]);
            assert!(center[1].abs() < EPS);
            assert!(center[2].abs() < EPS);
            // h = sqrt(25 - 9) = 4
            assert!((radius - 4.0).abs() < EPS, "r={}", radius);
            // Normal should be along X (connecting centers)
            assert!((normal[0] - 1.0).abs() < EPS, "nx={}", normal[0]);
        } else {
            panic!("Expected Circle");
        }
    }

    #[test]
    fn test_sphere_sphere_equal_radii_touching() {
        // Two spheres, r=3 each, centers 6 apart → tangent → empty
        let curves = sphere_sphere_ssi([0.0, 0.0, 0.0], 3.0, [6.0, 0.0, 0.0], 3.0).unwrap();
        assert!(curves.is_empty());
    }

    #[test]
    fn test_sphere_sphere_disjoint() {
        // Two spheres, r=1 each, centers 10 apart → disjoint
        let curves = sphere_sphere_ssi([0.0, 0.0, 0.0], 1.0, [10.0, 0.0, 0.0], 1.0).unwrap();
        assert!(curves.is_empty());
    }

    #[test]
    fn test_sphere_sphere_enclosed() {
        // Small sphere inside a large one → no intersection circle
        let curves = sphere_sphere_ssi([0.0, 0.0, 0.0], 10.0, [1.0, 0.0, 0.0], 2.0).unwrap();
        assert!(curves.is_empty());
    }

    #[test]
    fn test_sphere_sphere_concentric() {
        // Same center, different radii → enclosed → empty
        let curves = sphere_sphere_ssi([0.0, 0.0, 0.0], 5.0, [0.0, 0.0, 0.0], 3.0).unwrap();
        assert!(curves.is_empty());
    }

    #[test]
    fn test_sphere_sphere_same_radius() {
        // Equal radii, centers 4 apart → symmetric intersection
        let curves = sphere_sphere_ssi([0.0, 0.0, 0.0], 5.0, [4.0, 0.0, 0.0], 5.0).unwrap();
        assert_eq!(curves.len(), 1);
        if let SSICurve::Circle { center, radius, .. } = &curves[0] {
            // a = (25 - 25 + 16) / 8 = 2, center at (2, 0, 0)
            assert!((center[0] - 2.0).abs() < EPS);
            // h = sqrt(25 - 4) = sqrt(21) ≈ 4.583
            let expected_r = 21.0_f64.sqrt();
            assert!((radius - expected_r).abs() < EPS);
        } else {
            panic!("Expected Circle");
        }
    }

    #[test]
    fn test_sphere_sphere_different_radii() {
        // r1=3, r2=5, centers 4 apart along Y
        let curves = sphere_sphere_ssi([0.0, 0.0, 0.0], 3.0, [0.0, 4.0, 0.0], 5.0).unwrap();
        assert_eq!(curves.len(), 1);
        if let SSICurve::Circle {
            center,
            normal,
            radius,
        } = &curves[0]
        {
            // a = (9 - 25 + 16) / 8 = 0 → center at origin!
            assert!(center[0].abs() < EPS);
            assert!(center[1].abs() < EPS);
            assert!(center[2].abs() < EPS);
            // h = sqrt(9 - 0) = 3
            assert!((radius - 3.0).abs() < EPS);
            // Normal along Y
            assert!((normal[1] - 1.0).abs() < EPS);
        } else {
            panic!("Expected Circle");
        }
    }

    #[test]
    fn test_sphere_sphere_tilted() {
        // Two spheres with centers along a tilted direction
        let sqrt3_inv = 1.0 / 3.0_f64.sqrt();
        let d = 6.0; // distance between centers
        let center_b = [d * sqrt3_inv, d * sqrt3_inv, d * sqrt3_inv];
        let curves = sphere_sphere_ssi([0.0, 0.0, 0.0], 5.0, center_b, 5.0).unwrap();
        assert_eq!(curves.len(), 1);
        if let SSICurve::Circle { center, radius, .. } = &curves[0] {
            // a = (25 - 25 + 36) / 12 = 3
            // center = [0,0,0] + 3 * [1/√3, 1/√3, 1/√3] = [3/√3, 3/√3, 3/√3] = [√3, √3, √3]
            let expected = 3.0 * sqrt3_inv;
            assert!((center[0] - expected).abs() < EPS, "cx={}", center[0]);
            assert!((center[1] - expected).abs() < EPS, "cy={}", center[1]);
            assert!((center[2] - expected).abs() < EPS, "cz={}", center[2]);
            // h = sqrt(25 - 9) = 4
            assert!((radius - 4.0).abs() < EPS, "r={}", radius);
        } else {
            panic!("Expected Circle");
        }
    }
}
