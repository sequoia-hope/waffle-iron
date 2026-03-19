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

/// Compute SSI curves for two non-parallel equal-radius cylinders with intersecting axes.
///
/// Returns two `SSICurve::Ellipse` for the intersection curves of equal-radius
/// cylinders at angle α > 60°. The curves have semi-axes R/sin(α/2) and R/cos(α/2).
///
/// Guard conditions:
/// - Parallel axes (|cos| > 1-1e-6) → Ok(vec![]) (handled by existing parallel path)
/// - Near-parallel (|cos| >= 0.5, angle < 60°) → NotSupported
/// - Unequal radii (>1% relative) → NotSupported
/// - Skew axes (closest distance >= 0.05×R) → NotSupported
///
/// Ref: Patrikalakis Ch.5 — SSI algorithms for analytic surfaces.
/// Ref: Yang et al. (2023) — topology-guaranteed SSI.
pub(crate) fn cylinder_cylinder_ssi_non_parallel(
    cyl_a_origin: [f64; 3],
    cyl_a_axis: [f64; 3],
    cyl_a_radius: f64,
    cyl_b_origin: [f64; 3],
    cyl_b_axis: [f64; 3],
    cyl_b_radius: f64,
) -> Result<Vec<SSICurve>, KernelError> {
    let cos_angle = v3_dot(cyl_a_axis, cyl_b_axis).abs();

    // Parallel → handled by existing parallel SSI path
    if cos_angle > 1.0 - 1e-6 {
        return Ok(vec![]);
    }

    // Near-parallel (angle < 60°) → not supported
    // Use > 0.5 + epsilon so exactly 60° is supported
    if cos_angle > 0.5 + 1e-9 {
        return Err(KernelError::NotSupported {
            operation: "cylinder-cylinder SSI: near-parallel axes (angle < 60°)".to_string(),
        });
    }

    // Unequal radii check (>1% relative difference)
    let r_max = cyl_a_radius.max(cyl_b_radius);
    let r_min = cyl_a_radius.min(cyl_b_radius);
    if r_max < 1e-15 {
        return Err(KernelError::NotSupported {
            operation: "cylinder-cylinder SSI: zero radius".to_string(),
        });
    }
    if (r_max - r_min) / r_max >= 0.01 {
        return Err(KernelError::NotSupported {
            operation: "cylinder-cylinder SSI: unequal radii".to_string(),
        });
    }

    // Use average radius for nearly-equal radii
    let r = (cyl_a_radius + cyl_b_radius) / 2.0;

    // Compute closest points between the two axis lines to find intersection.
    // Line 1: P1 + t*d1, Line 2: P2 + s*d2
    let d1 = cyl_a_axis;
    let d2 = cyl_b_axis;
    let w = v3_sub(cyl_a_origin, cyl_b_origin);
    let a = v3_dot(d1, d1); // = 1 for unit vectors
    let b = v3_dot(d1, d2);
    let c = v3_dot(d2, d2); // = 1 for unit vectors
    let d = v3_dot(d1, w);
    let e = v3_dot(d2, w);
    let denom = a * c - b * b;

    if denom.abs() < 1e-12 {
        // Degenerate (parallel) — should have been caught above
        return Ok(vec![]);
    }

    let t_closest = (b * e - c * d) / denom;
    let s_closest = (a * e - b * d) / denom;

    let p1_closest = v3_add(cyl_a_origin, v3_scale(d1, t_closest));
    let p2_closest = v3_add(cyl_b_origin, v3_scale(d2, s_closest));
    let closest_dist = v3_length(v3_sub(p1_closest, p2_closest));

    // Skew axes check
    if closest_dist >= 0.05 * r {
        return Err(KernelError::NotSupported {
            operation: "cylinder-cylinder SSI: skew (non-intersecting) axes".to_string(),
        });
    }

    // Center = midpoint of closest approach
    let center = v3_scale(v3_add(p1_closest, p2_closest), 0.5);

    // Compute angle between axes
    // cos_angle already computed above (absolute value)
    // We need the actual angle α between the axes (0..π)
    let raw_cos = v3_dot(cyl_a_axis, cyl_b_axis);
    let alpha = raw_cos.abs().acos(); // angle in [0, π/2]

    let half_alpha = alpha / 2.0;
    let sin_half = half_alpha.sin();
    let cos_half = half_alpha.cos();

    // Local frame: e1 = a1, e2 = component of a2 perpendicular to a1, e3 = e1 × e2
    let e1 = cyl_a_axis;
    // Make sure a2 points "same direction" as the positive dot product
    let a2 = if raw_cos >= 0.0 {
        cyl_b_axis
    } else {
        [-cyl_b_axis[0], -cyl_b_axis[1], -cyl_b_axis[2]]
    };
    let a2_par = v3_scale(e1, v3_dot(a2, e1));
    let a2_perp = v3_sub(a2, a2_par);
    let a2_perp_len = v3_length(a2_perp);
    if a2_perp_len < 1e-12 {
        return Ok(vec![]); // Degenerate
    }
    let e2 = v3_scale(a2_perp, 1.0 / a2_perp_len);
    let e3 = v3_cross(e1, e2);

    // Curve 1: major direction = cot(α/2)*e1 + e2, semi_u = R/sin(α/2)
    let cot_half = cos_half / sin_half;
    let major_dir_1 = v3_add(v3_scale(e1, cot_half), e2);
    let major_dir_1_len = v3_length(major_dir_1);
    let major_axis_1 = v3_scale(major_dir_1, 1.0 / major_dir_1_len);
    let semi_major_1 = r / sin_half;
    let semi_minor_1 = r;
    // Normal = major_axis × e3 (normalized)
    let normal_1 = v3_cross(major_axis_1, v3_scale(e3, 1.0 / v3_length(e3)));
    let normal_1_len = v3_length(normal_1);
    let normal_1 = if normal_1_len > 1e-12 {
        v3_scale(normal_1, 1.0 / normal_1_len)
    } else {
        e1
    };

    // Curve 2: major direction = -tan(α/2)*e1 + e2, semi_u = R/cos(α/2)
    let tan_half = sin_half / cos_half;
    let major_dir_2 = v3_add(v3_scale(e1, -tan_half), e2);
    let major_dir_2_len = v3_length(major_dir_2);
    let major_axis_2 = v3_scale(major_dir_2, 1.0 / major_dir_2_len);
    let semi_major_2 = r / cos_half;
    let semi_minor_2 = r;
    let normal_2 = v3_cross(major_axis_2, v3_scale(e3, 1.0 / v3_length(e3)));
    let normal_2_len = v3_length(normal_2);
    let normal_2 = if normal_2_len > 1e-12 {
        v3_scale(normal_2, 1.0 / normal_2_len)
    } else {
        e1
    };

    Ok(vec![
        SSICurve::Ellipse {
            center,
            normal: normal_1,
            major_axis: major_axis_1,
            semi_major: semi_major_1,
            semi_minor: semi_minor_1,
        },
        SSICurve::Ellipse {
            center,
            normal: normal_2,
            major_axis: major_axis_2,
            semi_major: semi_major_2,
            semi_minor: semi_minor_2,
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

/// Check if a box AABB is fully enclosed within a cylinder (in XY plane).
/// All 4 corners of the AABB must be inside the cylinder circle.
pub(crate) fn box_enclosed_in_cyl(aabb: &Aabb, cyl: &CylinderParams) -> bool {
    let cx = cyl.center_bottom[0];
    let cy = cyl.center_bottom[1];
    let r = cyl.radius;
    for &x in &[aabb.min[0], aabb.max[0]] {
        for &y in &[aabb.min[1], aabb.max[1]] {
            let dx = x - cx;
            let dy = y - cy;
            if dx * dx + dy * dy > r * r + TOL {
                return false;
            }
        }
    }
    true
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

// ── Cylinder-Sphere SSI ──────────────────────────────────────────────────

/// Compute the intersection curves between an infinite cylinder (clipped to a Z range)
/// and a sphere. Returns circles for the coaxial/perpendicular case; degree-4 curve
/// approximations for the general case.
///
/// The cylinder is defined by an axis line (origin + direction), radius, and
/// min/max extent along the axis. The sphere is defined by center and radius.
///
/// Reference: Patrikalakis Ch.5.5 — Cylinder-sphere SSI.
pub(crate) fn cylinder_sphere_ssi(
    cyl_origin: [f64; 3],
    cyl_axis: [f64; 3],
    cyl_radius: f64,
    cyl_z_min: f64,
    cyl_z_max: f64,
    sphere_center: [f64; 3],
    sphere_radius: f64,
) -> Result<Vec<SSICurve>, KernelError> {
    // 1. Project sphere center onto the infinite cylinder axis line.
    //    axis point: cyl_origin, direction: cyl_axis (unit)
    //    P_proj = cyl_origin + t * cyl_axis, where t = (sphere_center - cyl_origin) · cyl_axis
    let diff = v3_sub(sphere_center, cyl_origin);
    let t_proj = v3_dot(diff, cyl_axis);
    let proj = v3_add(cyl_origin, v3_scale(cyl_axis, t_proj));

    // Perpendicular distance from sphere center to the axis
    let perp_vec = v3_sub(sphere_center, proj);
    let d = v3_length(perp_vec);

    // 2. Disjoint check: sphere too far from cylinder axis
    if d >= cyl_radius + sphere_radius - TOL {
        return Ok(vec![]);
    }

    // 3. Coaxial / near-coaxial case (d ≈ 0): exact circle intersection
    if d < TOL {
        // Cylinder x²+y² = R_cyl² and sphere x²+y²+z² = R_sph² (in axis-aligned frame)
        // Substituting: z² = R_sph² - R_cyl²
        let z_sq = sphere_radius * sphere_radius - cyl_radius * cyl_radius;
        if z_sq < 0.0 {
            // Cylinder radius > sphere radius -> sphere is inside cylinder, no contact
            return Ok(vec![]);
        }
        if z_sq < TOL * TOL {
            // Single tangent circle at z=0 (relative to sphere center projection)
            // Treat as tangent -> return empty per branch table
            return Ok(vec![]);
        }
        let dz = z_sq.sqrt();
        let mut curves = Vec::new();

        // Circle at proj + dz * axis
        let z_plus = t_proj + dz;
        if z_plus >= cyl_z_min - TOL && z_plus <= cyl_z_max + TOL {
            let center = v3_add(cyl_origin, v3_scale(cyl_axis, z_plus));
            curves.push(SSICurve::Circle {
                center,
                normal: cyl_axis,
                radius: cyl_radius,
            });
        }

        // Circle at proj - dz * axis
        let z_minus = t_proj - dz;
        if z_minus >= cyl_z_min - TOL && z_minus <= cyl_z_max + TOL {
            let center = v3_add(cyl_origin, v3_scale(cyl_axis, z_minus));
            curves.push(SSICurve::Circle {
                center,
                normal: cyl_axis,
                radius: cyl_radius,
            });
        }

        return Ok(curves);
    }

    // 4. Offset case: sphere center is at distance d > 0 from axis.
    //    Check if sphere is fully inside cylinder (no surface contact).
    if d + sphere_radius <= cyl_radius + TOL {
        // Sphere is entirely inside the cylinder barrel -> no intersection
        return Ok(vec![]);
    }

    // 5. General offset overlap: the intersection is a degree-4 space curve.
    //    We approximate by finding the z-range of the intersection and returning
    //    a representative Line segment (the true curve is not circular).
    //
    //    The intersection exists at height h (relative to sphere center projection)
    //    where the sphere cross-section circle (radius r_s = sqrt(R_sph^2 - h^2),
    //    center at distance d from axis) intersects the cylinder circle (radius R_cyl,
    //    on axis). This requires: |R_cyl - r_s(h)| <= d <= R_cyl + r_s(h).
    //    The second condition gives r_s(h) >= d - R_cyl, i.e.,
    //    R_sph^2 - h^2 >= (d - R_cyl)^2 -> h^2 <= R_sph^2 - (d - R_cyl)^2.

    let h_sq_max = sphere_radius * sphere_radius - (d - cyl_radius) * (d - cyl_radius);
    if h_sq_max < 0.0 {
        return Ok(vec![]);
    }
    let h_max = h_sq_max.sqrt();

    // The intersection curve spans from (t_proj - h_max) to (t_proj + h_max) along axis.
    // Clip to z-range.
    let z_lo = (t_proj - h_max).max(cyl_z_min);
    let z_hi = (t_proj + h_max).min(cyl_z_max);

    if z_lo > z_hi + TOL {
        return Ok(vec![]);
    }

    // Return a representative Line segment along the intersection extent on the
    // cylinder surface, in the direction of the sphere center offset from the axis.
    let perp_unit = v3_scale(perp_vec, 1.0 / d);
    let surf_pt_base = v3_add(cyl_origin, v3_scale(perp_unit, cyl_radius));

    let start = v3_add(surf_pt_base, v3_scale(cyl_axis, z_lo));
    let end = v3_add(surf_pt_base, v3_scale(cyl_axis, z_hi));

    Ok(vec![SSICurve::Line { start, end }])
}

// ── Cone-Sphere SSI ──────────────────────────────────────────────────────

/// Analytical SSI solver for cone-sphere pairs (A15 pair #11).
///
/// Returns intersection curves between a finite cone and a sphere.
/// The cone is defined by its apex, axis direction, half-angle, and axial extent
/// [z_min, z_max] measured from the apex along the axis.
pub(crate) fn cone_sphere_ssi(
    cone_apex: [f64; 3],
    cone_axis: [f64; 3],
    cone_half_angle: f64,
    cone_z_min: f64,
    cone_z_max: f64,
    sphere_center: [f64; 3],
    sphere_radius: f64,
) -> Result<Vec<SSICurve>, KernelError> {
    // Clamp z_min to 0 (cone only valid for h >= 0).
    let z_min = cone_z_min.max(0.0);
    let z_max = cone_z_max;
    if z_max <= z_min {
        return Ok(vec![]);
    }

    let tan_a = cone_half_angle.tan();
    if tan_a.abs() < TOL {
        // Degenerate cone (zero half-angle) → line, no surface.
        return Ok(vec![]);
    }

    // 1. Project sphere center onto the cone axis line.
    let diff = v3_sub(sphere_center, cone_apex);
    let t_proj = v3_dot(diff, cone_axis);
    let proj = v3_add(cone_apex, v3_scale(cone_axis, t_proj));
    let perp_vec = v3_sub(sphere_center, proj);
    let d = v3_length(perp_vec);

    // 2. Coaxial case (sphere center on cone axis): quadratic in h.
    if d < TOL {
        // Solve: h²·tan²(α) + (h - t_proj)² = R²
        //   (1 + tan²α)·h² − 2·t_proj·h + (t_proj² − R²) = 0
        let a_coeff = 1.0 + tan_a * tan_a;
        let b_coeff = -2.0 * t_proj;
        let c_coeff = t_proj * t_proj - sphere_radius * sphere_radius;
        let disc = b_coeff * b_coeff - 4.0 * a_coeff * c_coeff;

        if disc < 0.0 {
            return Ok(vec![]);
        }
        if disc < TOL * TOL {
            // Tangent → return empty per spec.
            return Ok(vec![]);
        }

        let sqrt_disc = disc.sqrt();
        let h1 = (-b_coeff + sqrt_disc) / (2.0 * a_coeff);
        let h2 = (-b_coeff - sqrt_disc) / (2.0 * a_coeff);

        let mut curves = Vec::new();
        for h in [h1, h2] {
            if h >= z_min - TOL && h <= z_max + TOL && h > TOL {
                let r = h * tan_a;
                if r > TOL {
                    let center = v3_add(cone_apex, v3_scale(cone_axis, h));
                    curves.push(SSICurve::Circle {
                        center,
                        normal: cone_axis,
                        radius: r,
                    });
                }
            }
        }
        return Ok(curves);
    }

    // 3. General offset case: degree-4 intersection.
    //    At height h along the axis, the cone radius is r_c = h·tan(α).
    //    The closest point on the cone circle (at height h) to the sphere center
    //    is at distance sqrt((d − r_c)² + (h − t_proj)²) from the sphere center.
    //    Intersection exists where this distance < sphere_radius.

    // Determine h-range where sphere could reach the axis neighbourhood.
    let h_lo = (t_proj - sphere_radius).max(z_min);
    let h_hi = (t_proj + sphere_radius).min(z_max);

    if h_lo >= h_hi {
        return Ok(vec![]);
    }

    // Scan for the h-range where the minimum distance from cone to sphere < R.
    let n_samples: usize = 200;
    let mut h_start = f64::MAX;
    let mut h_end = f64::MIN;
    let mut found = false;

    for i in 0..=n_samples {
        let h = h_lo + (h_hi - h_lo) * (i as f64) / (n_samples as f64);
        if h <= TOL {
            continue;
        }
        let cone_r = h * tan_a;
        let dh = h - t_proj;
        let min_dist_sq = (d - cone_r) * (d - cone_r) + dh * dh;
        if min_dist_sq < sphere_radius * sphere_radius {
            h_start = h_start.min(h);
            h_end = h_end.max(h);
            found = true;
        }
    }

    if !found {
        return Ok(vec![]);
    }

    // Check for tangent (very thin intersection band).
    if h_end - h_start < TOL {
        return Ok(vec![]);
    }

    // Clip to z-range.
    let h_start = h_start.max(z_min);
    let h_end = h_end.min(z_max);
    if h_end - h_start < TOL {
        return Ok(vec![]);
    }

    // Return a Line segment on the cone surface in the direction of the offset.
    let perp_unit = v3_scale(perp_vec, 1.0 / d);
    let start_r = h_start * tan_a;
    let end_r = h_end * tan_a;
    let start = v3_add(
        v3_add(cone_apex, v3_scale(cone_axis, h_start)),
        v3_scale(perp_unit, start_r),
    );
    let end = v3_add(
        v3_add(cone_apex, v3_scale(cone_axis, h_end)),
        v3_scale(perp_unit, end_r),
    );

    Ok(vec![SSICurve::Line { start, end }])
}

// ── Plane-Torus SSI ───────────────────────────────────────────────────────

/// Compute SSI between a plane and a torus.
///
/// Supports perpendicular planes (normal ∥ torus axis) with exact circle solutions.
/// Non-perpendicular orientations return NotSupported (degree-4 curves deferred).
///
/// Reference: Patrikalakis Ch.5 — Torus-plane SSI.
pub(crate) fn plane_torus_ssi(
    plane_origin: [f64; 3],
    plane_normal: [f64; 3],
    torus_center: [f64; 3],
    torus_axis: [f64; 3],
    torus_major_radius: f64,
    torus_minor_radius: f64,
) -> Result<Vec<SSICurve>, KernelError> {
    // Step 1: Check if plane normal is parallel to torus axis
    let dot_na = v3_dot(plane_normal, torus_axis).abs();
    if dot_na < 1.0 - TOL {
        return Err(KernelError::NotSupported {
            operation: "plane_torus_ssi: non-perpendicular plane".into(),
        });
    }

    // Step 2: Perpendicular plane — compute signed distance along axis
    let diff = v3_sub(plane_origin, torus_center);
    let d = v3_dot(diff, torus_axis);
    let r = torus_minor_radius;
    let big_r = torus_major_radius;

    // Disjoint: plane misses the torus tube entirely
    if d.abs() > r + TOL {
        return Ok(vec![]);
    }

    let mut curves = Vec::new();

    // Circle center: project torus center onto plane along axis at height d
    let circle_center = v3_add(torus_center, v3_scale(torus_axis, d));

    if (d.abs() - r).abs() < TOL {
        // Tangent case: |d| ≈ r → single circle at radius R
        curves.push(SSICurve::Circle {
            center: circle_center,
            normal: torus_axis,
            radius: big_r,
        });
    } else {
        // General case: 2 circles at radii R ± sqrt(r² - d²)
        let s = (r * r - d * d).sqrt();
        let r_outer = big_r + s;
        let r_inner = big_r - s;

        // Only emit inner circle if its radius is positive (handles spindle torus)
        if r_inner > TOL {
            curves.push(SSICurve::Circle {
                center: circle_center,
                normal: torus_axis,
                radius: r_inner,
            });
        }

        curves.push(SSICurve::Circle {
            center: circle_center,
            normal: torus_axis,
            radius: r_outer,
        });
    }

    Ok(curves)
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

    // ── Cylinder-Cylinder Non-Parallel SSI (CC1-CC12) ────────────────

    /// Helper: compute distance from a point to a line (origin + t*direction).
    fn dist_to_line(point: [f64; 3], line_origin: [f64; 3], line_dir: [f64; 3]) -> f64 {
        let dp = v3_sub(point, line_origin);
        let along = v3_dot(dp, line_dir);
        let proj = v3_scale(line_dir, along);
        v3_length(v3_sub(dp, proj))
    }

    /// Helper: evaluate an SSICurve::Ellipse at parameter t.
    fn eval_ellipse(curve: &SSICurve, t: f64) -> [f64; 3] {
        if let SSICurve::Ellipse {
            center,
            normal,
            major_axis,
            semi_major,
            semi_minor,
        } = curve
        {
            let minor_axis = v3_cross(*normal, *major_axis);
            v3_add(
                *center,
                v3_add(
                    v3_scale(*major_axis, *semi_major * t.cos()),
                    v3_scale(minor_axis, *semi_minor * t.sin()),
                ),
            )
        } else {
            panic!("Expected Ellipse");
        }
    }

    #[test]
    fn cc1_perpendicular_90deg() {
        let curves = cylinder_cylinder_ssi_non_parallel(
            [0.0, 0.0, 0.0],
            [0.0, 0.0, 1.0],
            1.0,
            [0.0, 0.0, 0.0],
            [1.0, 0.0, 0.0],
            1.0,
        )
        .unwrap();
        assert_eq!(curves.len(), 2);
        let sqrt2 = std::f64::consts::SQRT_2;
        for curve in &curves {
            if let SSICurve::Ellipse {
                semi_major,
                semi_minor,
                ..
            } = curve
            {
                // For 90°, both curves have semi_major = R√2, semi_minor = R
                assert!(
                    (*semi_major - sqrt2).abs() < 0.01,
                    "semi_major={}, expected {}",
                    semi_major,
                    sqrt2
                );
                assert!((*semi_minor - 1.0).abs() < EPS, "semi_minor={}", semi_minor);
            } else {
                panic!("Expected Ellipse");
            }
        }
    }

    #[test]
    fn cc2_60deg_angle() {
        // 60° angle between axes
        let cos60 = 0.5_f64;
        let sin60 = (1.0 - cos60 * cos60).sqrt();
        let axis_b = [sin60, 0.0, cos60]; // 60° from Z
        let r = 2.0;
        let curves = cylinder_cylinder_ssi_non_parallel(
            [0.0, 0.0, 0.0],
            [0.0, 0.0, 1.0],
            r,
            [0.0, 0.0, 0.0],
            axis_b,
            r,
        )
        .unwrap();
        assert_eq!(curves.len(), 2);
        // alpha = 60°, half = 30°
        let expected_1 = r / (30.0_f64.to_radians().sin()); // R/sin(30°) = 2R = 4
        let expected_2 = r / (30.0_f64.to_radians().cos()); // R/cos(30°) ≈ 2.309
        let mut majors: Vec<f64> = curves
            .iter()
            .map(|c| {
                if let SSICurve::Ellipse { semi_major, .. } = c {
                    *semi_major
                } else {
                    panic!()
                }
            })
            .collect();
        majors.sort_by(|a, b| a.partial_cmp(b).unwrap());
        assert!(
            (majors[0] - expected_2).abs() < 0.01,
            "smaller={}, expected {}",
            majors[0],
            expected_2
        );
        assert!(
            (majors[1] - expected_1).abs() < 0.01,
            "larger={}, expected {}",
            majors[1],
            expected_1
        );
    }

    #[test]
    fn cc3_unequal_radii() {
        let result = cylinder_cylinder_ssi_non_parallel(
            [0.0, 0.0, 0.0],
            [0.0, 0.0, 1.0],
            1.0,
            [0.0, 0.0, 0.0],
            [1.0, 0.0, 0.0],
            2.0,
        );
        assert!(matches!(result, Err(KernelError::NotSupported { .. })));
    }

    #[test]
    fn cc4_parallel_axes() {
        let curves = cylinder_cylinder_ssi_non_parallel(
            [0.0, 0.0, 0.0],
            [0.0, 0.0, 1.0],
            1.0,
            [2.0, 0.0, 0.0],
            [0.0, 0.0, 1.0],
            1.0,
        )
        .unwrap();
        assert!(curves.is_empty());
    }

    #[test]
    fn cc5_skew_axes() {
        // Axes don't intersect (offset by 10 in Y)
        let result = cylinder_cylinder_ssi_non_parallel(
            [0.0, 0.0, 0.0],
            [0.0, 0.0, 1.0],
            1.0,
            [0.0, 10.0, 0.0],
            [1.0, 0.0, 0.0],
            1.0,
        );
        assert!(matches!(result, Err(KernelError::NotSupported { .. })));
    }

    #[test]
    fn cc6_near_parallel_30deg() {
        let cos30 = (std::f64::consts::FRAC_PI_6).cos();
        let sin30 = (std::f64::consts::FRAC_PI_6).sin();
        let result = cylinder_cylinder_ssi_non_parallel(
            [0.0, 0.0, 0.0],
            [0.0, 0.0, 1.0],
            1.0,
            [0.0, 0.0, 0.0],
            [sin30, 0.0, cos30],
            1.0,
        );
        assert!(matches!(result, Err(KernelError::NotSupported { .. })));
    }

    #[test]
    fn cc7_shared_center() {
        let curves = cylinder_cylinder_ssi_non_parallel(
            [1.0, 2.0, 3.0],
            [0.0, 0.0, 1.0],
            1.0,
            [1.0, 2.0, 3.0],
            [1.0, 0.0, 0.0],
            1.0,
        )
        .unwrap();
        assert_eq!(curves.len(), 2);
        // Both should share center at (1,2,3)
        for curve in &curves {
            if let SSICurve::Ellipse { center, .. } = curve {
                assert!((center[0] - 1.0).abs() < EPS);
                assert!((center[1] - 2.0).abs() < EPS);
                assert!((center[2] - 3.0).abs() < EPS);
            }
        }
    }

    #[test]
    fn cc8_oracle_points_on_both_cylinders() {
        let axis_a = [0.0, 0.0, 1.0];
        let axis_b = [1.0, 0.0, 0.0];
        let origin = [0.0, 0.0, 0.0];
        let r = 1.0;
        let curves =
            cylinder_cylinder_ssi_non_parallel(origin, axis_a, r, origin, axis_b, r).unwrap();
        assert_eq!(curves.len(), 2);

        for curve in &curves {
            for i in 0..100 {
                let t = std::f64::consts::TAU * (i as f64) / 100.0;
                let pt = eval_ellipse(curve, t);
                let da = dist_to_line(pt, origin, axis_a);
                let db = dist_to_line(pt, origin, axis_b);
                assert!(
                    (da - r).abs() < 1e-5,
                    "point {:?} dist to axis_a = {}, expected {}",
                    pt,
                    da,
                    r
                );
                assert!(
                    (db - r).abs() < 1e-5,
                    "point {:?} dist to axis_b = {}, expected {}",
                    pt,
                    db,
                    r
                );
            }
        }
    }

    #[test]
    fn cc9_offset_origins_intersecting_axes() {
        // Axes intersect at (5, 0, 5)
        let curves = cylinder_cylinder_ssi_non_parallel(
            [5.0, 0.0, 0.0],
            [0.0, 0.0, 1.0],
            1.0,
            [0.0, 0.0, 5.0],
            [1.0, 0.0, 0.0],
            1.0,
        )
        .unwrap();
        assert_eq!(curves.len(), 2);
        for curve in &curves {
            if let SSICurve::Ellipse { center, .. } = curve {
                assert!(
                    (center[0] - 5.0).abs() < 0.01,
                    "cx={}, expected 5.0",
                    center[0]
                );
                assert!(
                    (center[2] - 5.0).abs() < 0.01,
                    "cz={}, expected 5.0",
                    center[2]
                );
            }
        }
    }

    #[test]
    fn cc10_75deg_angle() {
        let alpha = 75.0_f64.to_radians();
        let axis_b = [alpha.sin(), 0.0, alpha.cos()];
        let r = 1.5;
        let curves = cylinder_cylinder_ssi_non_parallel(
            [0.0, 0.0, 0.0],
            [0.0, 0.0, 1.0],
            r,
            [0.0, 0.0, 0.0],
            axis_b,
            r,
        )
        .unwrap();
        assert_eq!(curves.len(), 2);
        let expected_1 = r / (alpha / 2.0).sin();
        let expected_2 = r / (alpha / 2.0).cos();
        let mut majors: Vec<f64> = curves
            .iter()
            .map(|c| {
                if let SSICurve::Ellipse { semi_major, .. } = c {
                    *semi_major
                } else {
                    panic!()
                }
            })
            .collect();
        majors.sort_by(|a, b| a.partial_cmp(b).unwrap());
        let mut expecteds = [expected_1, expected_2];
        expecteds.sort_by(|a, b| a.partial_cmp(b).unwrap());
        assert!(
            (majors[0] - expecteds[0]).abs() < 0.01,
            "got {}, expected {}",
            majors[0],
            expecteds[0]
        );
        assert!(
            (majors[1] - expecteds[1]).abs() < 0.01,
            "got {}, expected {}",
            majors[1],
            expecteds[1]
        );
    }

    #[test]
    fn cc11_nearly_equal_radii() {
        // R1=1.0, R2=1.005 — within 1%, should use average
        let curves = cylinder_cylinder_ssi_non_parallel(
            [0.0, 0.0, 0.0],
            [0.0, 0.0, 1.0],
            1.0,
            [0.0, 0.0, 0.0],
            [1.0, 0.0, 0.0],
            1.005,
        )
        .unwrap();
        assert_eq!(curves.len(), 2);
        let avg_r = (1.0 + 1.005) / 2.0;
        for curve in &curves {
            if let SSICurve::Ellipse { semi_minor, .. } = curve {
                assert!(
                    (*semi_minor - avg_r).abs() < 0.01,
                    "semi_minor={}, expected {}",
                    semi_minor,
                    avg_r
                );
            }
        }
    }

    #[test]
    fn cc12_general_position_oracle() {
        // Arbitrary position and orientation
        let origin_a = [3.0, -2.0, 1.0];
        let axis_a = v3_scale([1.0, 1.0, 0.0], FRAC_1_SQRT_2);
        let origin_b = [3.0, -2.0, 1.0]; // Same origin so axes intersect
        let axis_b = [0.0, 0.0, 1.0];
        let r = 2.0;

        let curves =
            cylinder_cylinder_ssi_non_parallel(origin_a, axis_a, r, origin_b, axis_b, r).unwrap();
        assert_eq!(curves.len(), 2);

        // Oracle: every point on both ellipses lies on both cylinders
        for curve in &curves {
            for i in 0..100 {
                let t = std::f64::consts::TAU * (i as f64) / 100.0;
                let pt = eval_ellipse(curve, t);
                let da = dist_to_line(pt, origin_a, axis_a);
                let db = dist_to_line(pt, origin_b, axis_b);
                assert!(
                    (da - r).abs() < 1e-4,
                    "point {:?} dist to axis_a = {}, expected {}",
                    pt,
                    da,
                    r
                );
                assert!(
                    (db - r).abs() < 1e-4,
                    "point {:?} dist to axis_b = {}, expected {}",
                    pt,
                    db,
                    r
                );
            }
        }
    }

    // ── Cylinder-Sphere SSI helpers ──────────────────────────────────────

    /// Helper: evaluate a point on a Circle SSI curve at parameter t.
    fn eval_circle(curve: &SSICurve, t: f64) -> [f64; 3] {
        if let SSICurve::Circle {
            center,
            normal,
            radius,
        } = curve
        {
            // Build a local frame: u, v perpendicular to normal
            let arbitrary = if normal[0].abs() < 0.9 {
                [1.0, 0.0, 0.0]
            } else {
                [0.0, 1.0, 0.0]
            };
            let u = {
                let raw = v3_cross(*normal, arbitrary);
                let len = v3_length(raw);
                v3_scale(raw, 1.0 / len)
            };
            let v = v3_cross(*normal, u);
            v3_add(
                *center,
                v3_add(
                    v3_scale(u, *radius * t.cos()),
                    v3_scale(v, *radius * t.sin()),
                ),
            )
        } else {
            panic!("Expected Circle, got {:?}", curve);
        }
    }

    /// Helper: perpendicular distance from a point to an infinite line.
    fn dist_point_to_axis(pt: [f64; 3], axis_origin: [f64; 3], axis_dir: [f64; 3]) -> f64 {
        let dp = v3_sub(pt, axis_origin);
        let along = v3_dot(dp, axis_dir);
        let proj = v3_scale(axis_dir, along);
        v3_length(v3_sub(dp, proj))
    }

    /// Helper: signed distance along axis from origin.
    fn z_along_axis(pt: [f64; 3], axis_origin: [f64; 3], axis_dir: [f64; 3]) -> f64 {
        v3_dot(v3_sub(pt, axis_origin), axis_dir)
    }

    #[test]
    fn cs01_coaxial_two_circles() {
        // Cylinder axis along Z through origin, R_cyl=1.
        // Sphere at origin, R_sphere=2.
        // The cylinder axis passes through the sphere center (coaxial).
        // Infinite cylinder intersects sphere where x²+y²=1 and x²+y²+z²=4,
        // so z²=3, z=±√3. Two intersection circles.
        let curves = cylinder_sphere_ssi(
            [0.0, 0.0, 0.0], // cyl_origin
            [0.0, 0.0, 1.0], // cyl_axis
            1.0,             // cyl_radius
            -10.0,           // cyl_z_min (large enough to include both circles)
            10.0,            // cyl_z_max
            [0.0, 0.0, 0.0], // sphere_center
            2.0,             // sphere_radius
        )
        .unwrap();

        assert_eq!(
            curves.len(),
            2,
            "Coaxial cylinder-sphere should produce 2 circles, got {}",
            curves.len()
        );

        // Both should be circles
        for curve in &curves {
            assert!(
                matches!(curve, SSICurve::Circle { .. }),
                "Expected Circle, got {:?}",
                curve
            );
        }

        // The circles should be at z = ±√3, radius = 1 (the cylinder radius)
        let mut z_values: Vec<f64> = curves
            .iter()
            .map(|c| {
                if let SSICurve::Circle { center, .. } = c {
                    center[2]
                } else {
                    panic!()
                }
            })
            .collect();
        z_values.sort_by(|a, b| a.partial_cmp(b).unwrap());

        let sqrt3 = 3.0_f64.sqrt();
        assert!(
            (z_values[0] - (-sqrt3)).abs() < EPS,
            "Expected z≈-{}, got {}",
            sqrt3,
            z_values[0]
        );
        assert!(
            (z_values[1] - sqrt3).abs() < EPS,
            "Expected z≈{}, got {}",
            sqrt3,
            z_values[1]
        );

        // Each circle should have radius = cyl_radius = 1
        for curve in &curves {
            if let SSICurve::Circle { radius, .. } = curve {
                assert!(
                    (*radius - 1.0).abs() < EPS,
                    "Expected circle radius 1.0, got {}",
                    radius
                );
            }
        }
    }

    #[test]
    fn cs02_coaxial_circles_on_both_surfaces() {
        // Same setup as cs01. Verify oracle: every point on each circle lies on
        // both the cylinder surface AND the sphere surface.
        let cyl_origin = [0.0, 0.0, 0.0];
        let cyl_axis = [0.0, 0.0, 1.0];
        let cyl_radius = 1.0;
        let sphere_center = [0.0, 0.0, 0.0];
        let sphere_radius = 2.0;

        let curves = cylinder_sphere_ssi(
            cyl_origin,
            cyl_axis,
            cyl_radius,
            -10.0,
            10.0,
            sphere_center,
            sphere_radius,
        )
        .unwrap();

        assert!(
            curves.len() >= 1,
            "Expected at least 1 intersection curve, got 0"
        );

        let tau = crate::units::TAU_MODEL;
        for curve in &curves {
            for i in 0..64 {
                let t = std::f64::consts::TAU * (i as f64) / 64.0;
                let pt = eval_circle(curve, t);

                // Point should be at distance cyl_radius from the cylinder axis
                let d_cyl = dist_point_to_axis(pt, cyl_origin, cyl_axis);
                assert!(
                    (d_cyl - cyl_radius).abs() < tau,
                    "Point {:?} dist to cyl axis = {}, expected {} (err={})",
                    pt,
                    d_cyl,
                    cyl_radius,
                    (d_cyl - cyl_radius).abs()
                );

                // Point should be at distance sphere_radius from the sphere center
                let d_sph = v3_length(v3_sub(pt, sphere_center));
                assert!(
                    (d_sph - sphere_radius).abs() < tau,
                    "Point {:?} dist to sphere center = {}, expected {} (err={})",
                    pt,
                    d_sph,
                    sphere_radius,
                    (d_sph - sphere_radius).abs()
                );
            }
        }
    }

    #[test]
    fn cs03_disjoint() {
        // Sphere center far from cylinder axis: dist > R_cyl + R_sphere
        // Cylinder along Z at origin, R=1. Sphere at (10, 0, 0), R=1.
        // Distance from sphere center to axis = 10, which > 1+1=2.
        let curves = cylinder_sphere_ssi(
            [0.0, 0.0, 0.0],
            [0.0, 0.0, 1.0],
            1.0,
            -100.0,
            100.0,
            [10.0, 0.0, 0.0],
            1.0,
        )
        .unwrap();

        assert!(
            curves.is_empty(),
            "Disjoint cylinder-sphere should return empty, got {} curves",
            curves.len()
        );
    }

    #[test]
    fn cs04_tangent_external() {
        // Sphere center at exactly R_cyl + R_sphere from axis.
        // Cylinder along Z, R=1. Sphere at (3, 0, 0), R=2.
        // dist = 3 = 1 + 2 = tangent. Should return empty (within tolerance).
        let curves = cylinder_sphere_ssi(
            [0.0, 0.0, 0.0],
            [0.0, 0.0, 1.0],
            1.0,
            -100.0,
            100.0,
            [3.0, 0.0, 0.0],
            2.0,
        )
        .unwrap();

        assert!(
            curves.is_empty(),
            "Tangent (external) cylinder-sphere should return empty, got {} curves",
            curves.len()
        );
    }

    #[test]
    fn cs05_sphere_encloses_cylinder() {
        // Large sphere fully contains the cylinder cross-section.
        // Cylinder along Z, R=1, origin at (0,0,0).
        // Sphere at origin, R=5. dist=0, and 0 < 5 - 1 = 4 → sphere encloses cross-section.
        // Intersection: x²+y²=1 and x²+y²+z²=25 → z²=24, z=±√24.
        let curves = cylinder_sphere_ssi(
            [0.0, 0.0, 0.0],
            [0.0, 0.0, 1.0],
            1.0,
            -10.0,
            10.0,
            [0.0, 0.0, 0.0],
            5.0,
        )
        .unwrap();

        assert_eq!(
            curves.len(),
            2,
            "Sphere enclosing cylinder should produce 2 circles, got {}",
            curves.len()
        );

        let sqrt24 = 24.0_f64.sqrt();
        let mut z_values: Vec<f64> = curves
            .iter()
            .map(|c| {
                if let SSICurve::Circle { center, .. } = c {
                    center[2]
                } else {
                    panic!("Expected Circle")
                }
            })
            .collect();
        z_values.sort_by(|a, b| a.partial_cmp(b).unwrap());

        assert!(
            (z_values[0] - (-sqrt24)).abs() < EPS,
            "Expected z≈-{}, got {}",
            sqrt24,
            z_values[0]
        );
        assert!(
            (z_values[1] - sqrt24).abs() < EPS,
            "Expected z≈{}, got {}",
            sqrt24,
            z_values[1]
        );
    }

    #[test]
    fn cs06_cylinder_encloses_sphere() {
        // Large cylinder fully contains the sphere.
        // Cylinder along Z, R=5, origin at (0,0,0).
        // Sphere at origin, R=2. dist=0, and 0 < 5 - 2 = 3 → cylinder encloses sphere.
        // Intersection: x²+y²=25 and x²+y²+z²=4.
        // x²+y² = 25 > 4 = sphere_radius², so the sphere surface never reaches
        // the cylinder surface. No intersection.
        //
        // Actually: the sphere is fully inside the cylinder (no part of the sphere
        // touches the cylinder surface), so there should be 0 intersection curves.
        let curves = cylinder_sphere_ssi(
            [0.0, 0.0, 0.0],
            [0.0, 0.0, 1.0],
            5.0,
            -10.0,
            10.0,
            [0.0, 0.0, 0.0],
            2.0,
        )
        .unwrap();

        assert!(
            curves.is_empty(),
            "Cylinder enclosing sphere (no contact) should return empty, got {} curves",
            curves.len()
        );
    }

    #[test]
    fn cs07_offset_overlap() {
        // Sphere center offset from axis but still overlapping.
        // Cylinder along Z, R=2. Sphere at (1.5, 0, 0), R=2.
        // dist from sphere center to axis = 1.5.
        // |dist - R_cyl| = |1.5 - 2| = 0.5 < R_sphere=2 → overlapping.
        // Should produce intersection curves (1 or 2 depending on geometry).
        let curves = cylinder_sphere_ssi(
            [0.0, 0.0, 0.0],
            [0.0, 0.0, 1.0],
            2.0,
            -10.0,
            10.0,
            [1.5, 0.0, 0.0],
            2.0,
        )
        .unwrap();

        assert!(
            !curves.is_empty(),
            "Offset overlapping cylinder-sphere should produce curves, got 0"
        );

        // Oracle: every point on every returned curve should lie on both surfaces
        let tau = crate::units::TAU_MODEL;
        let cyl_axis = [0.0, 0.0, 1.0];
        let cyl_origin = [0.0, 0.0, 0.0];
        let sphere_center = [1.5, 0.0, 0.0];
        let sphere_radius = 2.0;
        let cyl_radius = 2.0;

        for curve in &curves {
            match curve {
                SSICurve::Circle { .. } => {
                    for i in 0..64 {
                        let t = std::f64::consts::TAU * (i as f64) / 64.0;
                        let pt = eval_circle(curve, t);
                        let d_cyl = dist_point_to_axis(pt, cyl_origin, cyl_axis);
                        let d_sph = v3_length(v3_sub(pt, sphere_center));
                        assert!(
                            (d_cyl - cyl_radius).abs() < tau,
                            "Point {:?} not on cylinder: dist={}, expected {}",
                            pt,
                            d_cyl,
                            cyl_radius
                        );
                        assert!(
                            (d_sph - sphere_radius).abs() < tau,
                            "Point {:?} not on sphere: dist={}, expected {}",
                            pt,
                            d_sph,
                            sphere_radius
                        );
                    }
                }
                _ => {
                    // Accept other curve types for the general offset case
                }
            }
        }
    }

    #[test]
    fn cs08_z_range_clip() {
        // Sphere intersects infinite cylinder but is outside the z-range.
        // Cylinder along Z, R=1, z_min=5.0, z_max=10.0.
        // Sphere at origin, R=2. Coaxial intersections at z=±√3 ≈ ±1.73.
        // Both circles are below z_min=5, so should be clipped away.
        let curves = cylinder_sphere_ssi(
            [0.0, 0.0, 0.0],
            [0.0, 0.0, 1.0],
            1.0,
            5.0,  // z_min — above the intersection circles
            10.0, // z_max
            [0.0, 0.0, 0.0],
            2.0,
        )
        .unwrap();

        assert!(
            curves.is_empty(),
            "Sphere outside cylinder z-range should return empty, got {} curves",
            curves.len()
        );
    }

    #[test]
    fn cs09_symmetry() {
        // Reversing cylinder axis direction should produce the same number of results.
        let cyl_origin = [0.0, 0.0, 0.0];
        let cyl_radius = 1.0;
        let sphere_center = [0.0, 0.0, 0.0];
        let sphere_radius = 2.0;

        let curves_fwd = cylinder_sphere_ssi(
            cyl_origin,
            [0.0, 0.0, 1.0], // axis +Z
            cyl_radius,
            -10.0,
            10.0,
            sphere_center,
            sphere_radius,
        )
        .unwrap();

        let curves_rev = cylinder_sphere_ssi(
            cyl_origin,
            [0.0, 0.0, -1.0], // axis -Z (reversed)
            cyl_radius,
            -10.0,
            10.0,
            sphere_center,
            sphere_radius,
        )
        .unwrap();

        assert_eq!(
            curves_fwd.len(),
            curves_rev.len(),
            "Reversing axis should give same count: fwd={}, rev={}",
            curves_fwd.len(),
            curves_rev.len()
        );
    }

    #[test]
    fn cs10_near_tangent() {
        // Sphere barely overlaps cylinder: distance = R_cyl + R_sphere - epsilon.
        // Cylinder along Z, R=1. Sphere at (2.999, 0, 0), R=2.
        // dist = 2.999, R_cyl + R_sphere = 3.0. Overlap by 0.001.
        // Should produce intersection (not empty).
        let curves = cylinder_sphere_ssi(
            [0.0, 0.0, 0.0],
            [0.0, 0.0, 1.0],
            1.0,
            -100.0,
            100.0,
            [2.999, 0.0, 0.0],
            2.0,
        )
        .unwrap();

        assert!(
            !curves.is_empty(),
            "Near-tangent (barely overlapping) should produce curves, got 0"
        );
    }

    #[test]
    fn cs11_identical_radii_coaxial() {
        // R_cyl = R_sphere, sphere center on axis.
        // Cylinder along Z, R=3. Sphere at origin, R=3.
        // Coaxial: z² = R_sph² - R_cyl² = 0 → single tangent circle at z=0.
        // Per the implementation, z_sq < TOL*TOL returns empty (tangent → empty).
        // So we test the non-degenerate case: sphere offset along axis.
        // Sphere at (0,0,1), R=3, coaxial with cylinder R=3.
        // z² = 9 - 9 = 0 → still tangent at z=1.
        //
        // Instead, test R_cyl = R_sphere = 3, sphere at origin, R_sphere = 5.
        // Actually, the spec says "identical radii, coaxial". Let's test R=R.
        // With R_cyl = R_sphere and sphere on axis: z_sq = 0, tangent → empty.
        // This verifies the tangent-circle-returns-empty behavior.
        let curves = cylinder_sphere_ssi(
            [0.0, 0.0, 0.0],
            [0.0, 0.0, 1.0],
            3.0,
            -100.0,
            100.0,
            [0.0, 0.0, 0.0],
            3.0,
        )
        .unwrap();

        // R_cyl == R_sphere coaxial → z_sq = 0 → single tangent circle → empty
        assert!(
            curves.is_empty(),
            "Identical radii coaxial (tangent) should return empty, got {} curves",
            curves.len()
        );

        // Now test with sphere slightly larger so we get real circles.
        // R_sphere = 3.001, R_cyl = 3. z² = 3.001² - 3² = 9.006001 - 9 = 0.006001
        let curves2 = cylinder_sphere_ssi(
            [0.0, 0.0, 0.0],
            [0.0, 0.0, 1.0],
            3.0,
            -100.0,
            100.0,
            [0.0, 0.0, 0.0],
            3.001,
        )
        .unwrap();

        assert_eq!(
            curves2.len(),
            2,
            "Nearly-identical radii (R_sph slightly larger) should produce 2 circles, got {}",
            curves2.len()
        );

        // Circles should have radius = R_cyl = 3
        for curve in &curves2 {
            if let SSICurve::Circle { radius, .. } = curve {
                assert!(
                    (*radius - 3.0).abs() < EPS,
                    "Expected circle radius 3.0, got {}",
                    radius
                );
            }
        }
    }

    #[test]
    fn cs12_large_sphere_small_cylinder() {
        // R_sphere = 100, R_cyl = 0.1, coaxial.
        // z² = 100² - 0.1² = 10000 - 0.01 = 9999.99
        // z = ±99.99995. Two circles with radius ≈ 0.1 (= R_cyl).
        let curves = cylinder_sphere_ssi(
            [0.0, 0.0, 0.0],
            [0.0, 0.0, 1.0],
            0.1,
            -200.0,
            200.0,
            [0.0, 0.0, 0.0],
            100.0,
        )
        .unwrap();

        assert_eq!(
            curves.len(),
            2,
            "Large sphere / small cylinder should produce 2 circles, got {}",
            curves.len()
        );

        let expected_z = (100.0_f64 * 100.0 - 0.1 * 0.1).sqrt();
        let mut z_values: Vec<f64> = curves
            .iter()
            .map(|c| {
                if let SSICurve::Circle { center, .. } = c {
                    center[2]
                } else {
                    panic!("Expected Circle")
                }
            })
            .collect();
        z_values.sort_by(|a, b| a.partial_cmp(b).unwrap());

        assert!(
            (z_values[0] - (-expected_z)).abs() < EPS,
            "Expected z ≈ -{}, got {}",
            expected_z,
            z_values[0]
        );
        assert!(
            (z_values[1] - expected_z).abs() < EPS,
            "Expected z ≈ {}, got {}",
            expected_z,
            z_values[1]
        );

        // Each circle should have radius = R_cyl = 0.1
        for curve in &curves {
            if let SSICurve::Circle { radius, .. } = curve {
                assert!(
                    (*radius - 0.1).abs() < EPS,
                    "Expected circle radius 0.1, got {}",
                    radius
                );
            }
        }
    }

    #[test]
    fn cs13_tilted_axis() {
        // Cylinder axis = [1,1,1] normalized, sphere at arbitrary position.
        // This tests that the implementation works with non-axis-aligned cylinders.
        let inv_sqrt3 = 1.0 / 3.0_f64.sqrt();
        let axis = [inv_sqrt3, inv_sqrt3, inv_sqrt3];

        // Cylinder through origin, R=1, tilted axis.
        // Sphere at (2, 0, 0), R=1.5.
        // The perpendicular distance from (2,0,0) to the axis line through origin
        // with direction [1,1,1]/sqrt(3) is:
        //   proj = (2,0,0)·(1,1,1)/sqrt(3) * (1,1,1)/sqrt(3) = (2/sqrt(3)) * (1,1,1)/sqrt(3)
        //        = (2/3)(1,1,1) = (2/3, 2/3, 2/3)
        //   perp = (2,0,0) - (2/3,2/3,2/3) = (4/3, -2/3, -2/3)
        //   |perp| = sqrt(16/9 + 4/9 + 4/9) = sqrt(24/9) = sqrt(8/3) ≈ 1.633
        // dist ≈ 1.633 < R_cyl + R_sphere = 2.5 → overlapping.
        // dist + R_sphere = 3.133 > R_cyl = 1 → sphere not inside cylinder.
        // Should produce intersection curve(s).
        let curves = cylinder_sphere_ssi(
            [0.0, 0.0, 0.0],
            axis,
            1.0,
            -100.0,
            100.0,
            [2.0, 0.0, 0.0],
            1.5,
        )
        .unwrap();

        // Should produce 0, 1, or 2 curves — for this geometry, expect non-empty
        assert!(
            !curves.is_empty(),
            "Tilted axis with overlapping geometry should produce curves, got 0"
        );
        assert!(
            curves.len() <= 2,
            "Should produce at most 2 curves, got {}",
            curves.len()
        );

        // Also test a disjoint case with tilted axis:
        // Sphere at (10, 10, 0), R=0.5. Distance to axis through origin [1,1,1]/sqrt(3):
        //   proj_t = (10,10,0)·(1,1,1)/sqrt(3) = 20/sqrt(3)
        //   proj = 20/3 * (1,1,1) = (20/3, 20/3, 20/3)
        //   perp = (10,10,0) - (20/3,20/3,20/3) = (10/3, 10/3, -20/3)
        //   |perp| = sqrt(100/9 + 100/9 + 400/9) = sqrt(600/9) ≈ 8.165
        // dist ≈ 8.165 > R_cyl + R_sphere = 1.5 → disjoint.
        let curves_disjoint = cylinder_sphere_ssi(
            [0.0, 0.0, 0.0],
            axis,
            1.0,
            -100.0,
            100.0,
            [10.0, 10.0, 0.0],
            0.5,
        )
        .unwrap();

        assert!(
            curves_disjoint.is_empty(),
            "Tilted axis disjoint case should return empty, got {} curves",
            curves_disjoint.len()
        );
    }

    // ── Cone-Sphere SSI ──────────────────────────────────────────────────

    /// Helper: distance from a point to the cone surface.
    /// The cone has apex at `apex`, axis `axis` (unit), half-angle `alpha`.
    /// At height h from the apex, the cone radius is h * tan(alpha).
    fn dist_point_to_cone_surface(pt: [f64; 3], apex: [f64; 3], axis: [f64; 3], alpha: f64) -> f64 {
        let diff = v3_sub(pt, apex);
        let h = v3_dot(diff, axis);
        let cone_radius = h * alpha.tan();
        let d_axis = dist_point_to_axis(pt, apex, axis);
        (d_axis - cone_radius).abs()
    }

    #[test]
    fn cs_cone_01_coaxial_sphere_on_cone() {
        // Cone: apex at origin, axis +Z, half-angle 45°, z in [0, 10].
        // At height h, cone radius = h * tan(45°) = h.
        // Sphere: center at (0, 0, 3), radius 2.
        // Coaxial case: sphere center is on cone axis.
        // Solve: (h * tan(45°))² + (h - 3)² = 4
        //   h² + h² - 6h + 9 = 4  →  2h² - 6h + 5 = 0
        //   h = (6 ± √(36-40))/4 — discriminant = -4 < 0
        // So with 45° half-angle the sphere doesn't intersect.
        //
        // Use half-angle = 30° instead: cone radius = h * tan(30°) = h/√3.
        // Solve: (h/√3)² + (h - 3)² = 4
        //   h²/3 + h² - 6h + 9 = 4  →  (4/3)h² - 6h + 5 = 0
        //   h = (6 ± √(36-80/3)) / (8/3) = (6 ± √(28/3)) / (8/3)
        //   discriminant = 36 - 80/3 = 28/3 ≈ 9.333 > 0 → two roots
        let half_angle = std::f64::consts::FRAC_PI_6; // 30°
        let curves = cone_sphere_ssi(
            [0.0, 0.0, 0.0], // apex
            [0.0, 0.0, 1.0], // axis
            half_angle,
            0.0,             // z_min
            10.0,            // z_max
            [0.0, 0.0, 3.0], // sphere center on axis
            2.0,             // sphere radius
        )
        .unwrap();

        // Coaxial case should produce 1 or 2 circles
        assert!(
            curves.len() >= 1 && curves.len() <= 2,
            "Coaxial cone-sphere should produce 1-2 circles, got {}",
            curves.len()
        );

        let tau = crate::units::TAU_MODEL;
        for curve in &curves {
            match curve {
                SSICurve::Circle {
                    center,
                    normal,
                    radius,
                } => {
                    // Circle normal should be parallel to cone axis (coaxial case)
                    let dot = v3_dot(*normal, [0.0, 0.0, 1.0]).abs();
                    assert!(
                        (dot - 1.0).abs() < tau,
                        "Circle normal should be parallel to axis, dot={}",
                        dot
                    );
                    // Circle center should be on the axis
                    assert!(center[0].abs() < tau, "Circle center x should be 0");
                    assert!(center[1].abs() < tau, "Circle center y should be 0");
                    // Height h = center[2], cone radius at h = h * tan(half_angle)
                    let h = center[2];
                    let expected_r = h * half_angle.tan();
                    assert!(
                        (*radius - expected_r).abs() < tau,
                        "Circle radius {} should equal h*tan(alpha)={}",
                        radius,
                        expected_r
                    );
                }
                _ => panic!("Expected Circle for coaxial case, got {:?}", curve),
            }
        }
    }

    #[test]
    fn cs_cone_02_disjoint_far() {
        // Cone: apex at origin, axis +Z, half-angle 30°, z in [0, 5].
        // Sphere far away at (100, 0, 0), radius 1.
        let curves = cone_sphere_ssi(
            [0.0, 0.0, 0.0],
            [0.0, 0.0, 1.0],
            std::f64::consts::FRAC_PI_6,
            0.0,
            5.0,
            [100.0, 0.0, 0.0],
            1.0,
        )
        .unwrap();

        assert!(
            curves.is_empty(),
            "Disjoint cone-sphere should return empty, got {} curves",
            curves.len()
        );
    }

    #[test]
    fn cs_cone_03_tangent_external() {
        // Cone: apex at origin, axis +Z, half-angle 45°, z in [0, 10].
        // Cone surface: x² + y² = z² (radius = z at height z).
        // Sphere center at (10, 0, 0). Min distance from (10,0,0) to cone surface
        // is at z=5: dist = sqrt((10-5)² + 5²) = sqrt(50) = 10/√2.
        // Set sphere radius = 10/√2 for exact tangency.
        // Tangent case should return empty (within tolerance).
        let r_tangent = 10.0 / std::f64::consts::SQRT_2;
        let curves = cone_sphere_ssi(
            [0.0, 0.0, 0.0],
            [0.0, 0.0, 1.0],
            std::f64::consts::FRAC_PI_4, // 45°
            0.0,
            10.0,
            [10.0, 0.0, 0.0],
            r_tangent,
        )
        .unwrap();

        // Tangent → empty (single point contact within tolerance)
        assert!(
            curves.is_empty(),
            "Tangent external cone-sphere should return empty, got {} curves",
            curves.len()
        );
    }

    #[test]
    fn cs_cone_04_sphere_enclosing_apex() {
        // Cone: apex at origin, axis +Z, half-angle 30°, z in [0, 10].
        // Sphere: center at (0, 0, 0.5), radius 3.0 — encloses the apex.
        // The sphere is large enough to cut the cone, producing intersection.
        let half_angle = std::f64::consts::FRAC_PI_6; // 30°
        let curves = cone_sphere_ssi(
            [0.0, 0.0, 0.0],
            [0.0, 0.0, 1.0],
            half_angle,
            0.0,
            10.0,
            [0.0, 0.0, 0.5],
            3.0,
        )
        .unwrap();

        // Should produce at least 1 circle (coaxial, sphere enclosing apex)
        assert!(
            !curves.is_empty(),
            "Sphere enclosing apex should produce intersection curves"
        );

        let tau = crate::units::TAU_MODEL;
        // Verify all intersection points lie on both surfaces
        for curve in &curves {
            if let SSICurve::Circle {
                center,
                normal: _,
                radius,
            } = curve
            {
                // Sample points on the circle and verify they are on both surfaces
                let h = v3_dot(v3_sub(*center, [0.0, 0.0, 0.0]), [0.0, 0.0, 1.0]);
                let cone_r = h * half_angle.tan();
                assert!(
                    (*radius - cone_r).abs() < tau,
                    "Circle radius {} should match cone radius at h={}: {}",
                    radius,
                    h,
                    cone_r
                );

                // Verify points on the circle are on the sphere
                for i in 0..16 {
                    let t = std::f64::consts::TAU * (i as f64) / 16.0;
                    let pt = eval_circle(curve, t);
                    let d_sphere = v3_length(v3_sub(pt, [0.0, 0.0, 0.5]));
                    assert!(
                        (d_sphere - 3.0).abs() < tau,
                        "Point {:?} dist to sphere center = {}, expected 3.0",
                        pt,
                        d_sphere
                    );
                }
            }
        }
    }

    #[test]
    fn cs_cone_05_general_offset() {
        // Cone: apex at origin, axis +Z, half-angle 30°, z in [0, 10].
        // Sphere: center off-axis at (2, 0, 4), radius 3.
        // General offset case: intersection is a degree-4 curve,
        // approximated as a Line segment or circle.
        let curves = cone_sphere_ssi(
            [0.0, 0.0, 0.0],
            [0.0, 0.0, 1.0],
            std::f64::consts::FRAC_PI_6,
            0.0,
            10.0,
            [2.0, 0.0, 4.0],
            3.0,
        )
        .unwrap();

        // Offset overlap should produce at least 1 curve
        assert!(
            !curves.is_empty(),
            "General offset cone-sphere should produce intersection curves"
        );

        // Verify each curve is a recognized SSICurve variant
        for curve in &curves {
            match curve {
                SSICurve::Circle { radius, .. } => {
                    assert!(*radius > 0.0, "Circle radius must be positive");
                }
                SSICurve::Line { start, end } => {
                    let len = v3_length(v3_sub(*end, *start));
                    assert!(len > 0.0, "Line segment must have nonzero length");
                }
                SSICurve::Ellipse {
                    semi_major,
                    semi_minor,
                    ..
                } => {
                    assert!(*semi_major > 0.0 && *semi_minor > 0.0);
                }
            }
        }
    }

    #[test]
    fn cs_cone_06_outside_z_range() {
        // Cone: apex at origin, axis +Z, half-angle 30°, z in [0, 2].
        // Sphere: center at (0, 0, 8), radius 2.
        // At h=8, cone radius = 8*tan(30°) ≈ 4.62. Sphere would intersect
        // at h ≈ 6-10, but cone z_max = 2 → entirely outside.
        let curves = cone_sphere_ssi(
            [0.0, 0.0, 0.0],
            [0.0, 0.0, 1.0],
            std::f64::consts::FRAC_PI_6,
            0.0,
            2.0,
            [0.0, 0.0, 8.0],
            2.0,
        )
        .unwrap();

        assert!(
            curves.is_empty(),
            "Intersection outside z-range should return empty, got {} curves",
            curves.len()
        );
    }

    #[test]
    fn cs_cone_07_coaxial_two_circles() {
        // Cone: apex at origin, axis +Z, half-angle 30°, z in [0, 20].
        // Sphere: center at (0, 0, 6), radius 4 — on axis, large overlap.
        // Coaxial: solve (h/√3)² + (h-6)² = 16
        //   h²/3 + h² - 12h + 36 = 16  →  (4/3)h² - 12h + 20 = 0
        //   h = (12 ± √(144 - 320/3)) / (8/3)
        //   discriminant = 144 - 320/3 = 112/3 ≈ 37.33 > 0 → two roots
        let half_angle = std::f64::consts::FRAC_PI_6; // 30°
        let curves = cone_sphere_ssi(
            [0.0, 0.0, 0.0],
            [0.0, 0.0, 1.0],
            half_angle,
            0.0,
            20.0,
            [0.0, 0.0, 6.0],
            4.0,
        )
        .unwrap();

        assert_eq!(
            curves.len(),
            2,
            "Coaxial cone-sphere with large overlap should produce 2 circles, got {}",
            curves.len()
        );

        let tau = crate::units::TAU_MODEL;
        let mut h_values: Vec<f64> = Vec::new();

        for curve in &curves {
            match curve {
                SSICurve::Circle {
                    center,
                    normal,
                    radius,
                } => {
                    // Normal parallel to axis
                    let dot = v3_dot(*normal, [0.0, 0.0, 1.0]).abs();
                    assert!(
                        (dot - 1.0).abs() < tau,
                        "Circle normal should be ∥ axis, dot={}",
                        dot
                    );
                    // Center on axis
                    assert!(center[0].abs() < tau);
                    assert!(center[1].abs() < tau);
                    let h = center[2];
                    h_values.push(h);
                    // Radius consistency: r = h * tan(alpha)
                    let expected_r = h * half_angle.tan();
                    assert!(
                        (*radius - expected_r).abs() < tau,
                        "radius {} != h*tan(a)={}",
                        radius,
                        expected_r
                    );
                    // Height must be positive and within z-range
                    assert!(h > 0.0, "h must be > 0, got {}", h);
                    assert!(h <= 20.0 + tau, "h must be <= z_max, got {}", h);

                    // Oracle: points on circle must be on sphere
                    for i in 0..16 {
                        let t = std::f64::consts::TAU * (i as f64) / 16.0;
                        let pt = eval_circle(curve, t);
                        let d_sphere = v3_length(v3_sub(pt, [0.0, 0.0, 6.0]));
                        assert!(
                            (d_sphere - 4.0).abs() < tau,
                            "Point dist to sphere = {}, expected 4.0",
                            d_sphere
                        );
                    }
                }
                _ => panic!("Expected Circle for coaxial case, got {:?}", curve),
            }
        }

        // Two distinct heights
        h_values.sort_by(|a, b| a.partial_cmp(b).unwrap());
        assert!(
            (h_values[1] - h_values[0]).abs() > 0.1,
            "Two circles should be at distinct heights: {:?}",
            h_values
        );
    }

    // ── Adversarial cone-sphere tests ─────────────────────────────────

    /// Helper: verify that a point lies on a cone surface (apex, axis +Z, half_angle).
    /// Returns the absolute distance error from the cone surface.
    fn cone_surface_error(pt: [f64; 3], apex: [f64; 3], axis: [f64; 3], half_angle: f64) -> f64 {
        let diff = v3_sub(pt, apex);
        let h = v3_dot(diff, axis);
        let proj = v3_scale(axis, h);
        let perp = v3_sub(diff, proj);
        let perp_dist = v3_length(perp);
        let expected_r = h * half_angle.tan();
        (perp_dist - expected_r).abs()
    }

    /// Helper: verify that a point lies on a sphere surface.
    fn sphere_surface_error(pt: [f64; 3], center: [f64; 3], radius: f64) -> f64 {
        (v3_length(v3_sub(pt, center)) - radius).abs()
    }

    /// Validate all returned curves: no NaN, positive radii, circle centers
    /// within z_range, and points on circles lie on both cone and sphere.
    fn validate_cone_sphere_results(
        curves: &[SSICurve],
        apex: [f64; 3],
        axis: [f64; 3],
        half_angle: f64,
        z_min: f64,
        z_max: f64,
        sphere_center: [f64; 3],
        sphere_radius: f64,
    ) {
        let tau = crate::units::TAU_MODEL;
        for curve in curves {
            match curve {
                SSICurve::Circle {
                    center,
                    normal,
                    radius,
                } => {
                    // No NaN
                    assert!(
                        !center[0].is_nan() && !center[1].is_nan() && !center[2].is_nan(),
                        "Circle center contains NaN: {:?}",
                        center
                    );
                    assert!(!radius.is_nan(), "Circle radius is NaN");
                    assert!(
                        !normal[0].is_nan() && !normal[1].is_nan() && !normal[2].is_nan(),
                        "Circle normal contains NaN: {:?}",
                        normal
                    );
                    // Positive radius
                    assert!(
                        *radius > 0.0,
                        "Circle radius must be positive, got {}",
                        radius
                    );
                    // Circle center height within z_range (with tolerance)
                    let h = v3_dot(v3_sub(*center, apex), axis);
                    assert!(
                        h >= z_min - tau && h <= z_max + tau,
                        "Circle center height {} outside z_range [{}, {}]",
                        h,
                        z_min,
                        z_max
                    );
                    // Sample 16 points and verify they lie on both surfaces
                    for i in 0..16 {
                        let t = std::f64::consts::TAU * (i as f64) / 16.0;
                        let pt = eval_circle(curve, t);
                        let cone_err = cone_surface_error(pt, apex, axis, half_angle);
                        assert!(
                            cone_err < tau,
                            "Point {:?} not on cone surface, error={}",
                            pt,
                            cone_err
                        );
                        let sphere_err = sphere_surface_error(pt, sphere_center, sphere_radius);
                        assert!(
                            sphere_err < tau,
                            "Point {:?} not on sphere surface, error={}",
                            pt,
                            sphere_err
                        );
                    }
                }
                SSICurve::Line { start, end } => {
                    // No NaN
                    for v in [start, end] {
                        assert!(
                            !v[0].is_nan() && !v[1].is_nan() && !v[2].is_nan(),
                            "Line endpoint contains NaN: {:?}",
                            v
                        );
                    }
                    let len = v3_length(v3_sub(*end, *start));
                    assert!(len > 0.0, "Line segment must have nonzero length");
                }
                SSICurve::Ellipse {
                    semi_major,
                    semi_minor,
                    ..
                } => {
                    assert!(!semi_major.is_nan() && !semi_minor.is_nan());
                    assert!(*semi_major > 0.0 && *semi_minor > 0.0);
                }
            }
        }
    }

    #[test]
    fn cs_cone_08_micro_scale() {
        // Cone and sphere at 1e-4 scale (near MIN_FEATURE_SIZE).
        // Cone: apex at origin, axis +Z, half-angle 30°, z in [0, 1e-4].
        // Sphere: center at (0, 0, 5e-5), radius 4e-5 — coaxial, within cone.
        let half_angle = std::f64::consts::FRAC_PI_6;
        let apex = [0.0, 0.0, 0.0];
        let axis = [0.0, 0.0, 1.0];
        let z_min = 0.0;
        let z_max = 1e-4;
        let sphere_center = [0.0, 0.0, 5e-5];
        let sphere_radius = 4e-5;

        let curves = cone_sphere_ssi(
            apex,
            axis,
            half_angle,
            z_min,
            z_max,
            sphere_center,
            sphere_radius,
        )
        .unwrap();

        // Should not panic. At this scale TOL=1e-9 still leaves room for
        // features. We may or may not get results depending on whether
        // h values pass the h > TOL filter, but must not panic/NaN.
        validate_cone_sphere_results(
            &curves,
            apex,
            axis,
            half_angle,
            z_min,
            z_max,
            sphere_center,
            sphere_radius,
        );
    }

    #[test]
    fn cs_cone_09_large_half_angle() {
        // Very wide cone: half_angle = 80° (near 90° limit).
        // tan(80°) ≈ 5.67. Cone opens very wide.
        // Sphere on axis at z=2, radius 3.
        let half_angle = 80.0_f64.to_radians();
        let apex = [0.0, 0.0, 0.0];
        let axis = [0.0, 0.0, 1.0];
        let z_min = 0.0;
        let z_max = 10.0;
        let sphere_center = [0.0, 0.0, 2.0];
        let sphere_radius = 3.0;

        let curves = cone_sphere_ssi(
            apex,
            axis,
            half_angle,
            z_min,
            z_max,
            sphere_center,
            sphere_radius,
        )
        .unwrap();

        // Coaxial case: solve (1 + tan²(80°))h² - 4h + (4 - 9) = 0
        // tan²(80°) ≈ 32.16, so a_coeff ≈ 33.16
        // disc = 16 - 4*33.16*(-5) = 16 + 663.2 = 679.2 > 0 → two roots
        // But h must be > 0 and within [0, 10].
        assert!(
            !curves.is_empty(),
            "Wide cone (80°) with on-axis sphere should produce intersection"
        );

        validate_cone_sphere_results(
            &curves,
            apex,
            axis,
            half_angle,
            z_min,
            z_max,
            sphere_center,
            sphere_radius,
        );
    }

    #[test]
    fn cs_cone_10_small_half_angle() {
        // Very narrow cone: half_angle = 5°.
        // tan(5°) ≈ 0.0875. Cone is almost a line.
        // Sphere on axis at z=5, radius 2.
        let half_angle = 5.0_f64.to_radians();
        let apex = [0.0, 0.0, 0.0];
        let axis = [0.0, 0.0, 1.0];
        let z_min = 0.0;
        let z_max = 20.0;
        let sphere_center = [0.0, 0.0, 5.0];
        let sphere_radius = 2.0;

        let curves = cone_sphere_ssi(
            apex,
            axis,
            half_angle,
            z_min,
            z_max,
            sphere_center,
            sphere_radius,
        )
        .unwrap();

        // Coaxial: (1 + tan²(5°))h² - 10h + (25-4) = 0
        // a ≈ 1.0077, disc = 100 - 4*1.0077*21 = 100 - 84.6 = 15.4 > 0 → two roots
        assert!(
            !curves.is_empty(),
            "Narrow cone (5°) with on-axis sphere should produce intersection"
        );

        validate_cone_sphere_results(
            &curves,
            apex,
            axis,
            half_angle,
            z_min,
            z_max,
            sphere_center,
            sphere_radius,
        );

        // Verify the circles have small radii (since the cone is narrow)
        for curve in &curves {
            if let SSICurve::Circle { radius, .. } = curve {
                // At h~5, r = 5*tan(5°) ≈ 0.437. Radii should be < 1.
                assert!(
                    *radius < 2.0,
                    "Narrow cone circle radius {} should be small",
                    radius
                );
            }
        }
    }

    #[test]
    fn cs_cone_11_sphere_at_apex() {
        // Sphere centered exactly at the cone apex.
        // Coaxial case with t_proj = 0.
        // Solve: (1 + tan²α)h² + 0 + (0 - R²) = 0
        //   h² = R² / (1 + tan²α)  →  h = R / sec(α) = R·cos(α)
        // Only one positive root (h2 = -h1 < 0, filtered out).
        let half_angle = std::f64::consts::FRAC_PI_6; // 30°
        let apex = [0.0, 0.0, 0.0];
        let axis = [0.0, 0.0, 1.0];
        let z_min = 0.0;
        let z_max = 10.0;
        let sphere_center = [0.0, 0.0, 0.0]; // exactly at apex
        let sphere_radius = 3.0;

        let curves = cone_sphere_ssi(
            apex,
            axis,
            half_angle,
            z_min,
            z_max,
            sphere_center,
            sphere_radius,
        )
        .unwrap();

        // Should produce exactly 1 circle (h = R·cos(α), the negative root is filtered)
        assert_eq!(
            curves.len(),
            1,
            "Sphere at apex should produce 1 circle, got {}",
            curves.len()
        );

        let expected_h = sphere_radius * half_angle.cos();
        let tau = crate::units::TAU_MODEL;

        if let SSICurve::Circle { center, radius, .. } = &curves[0] {
            let h = center[2];
            assert!(
                (h - expected_h).abs() < tau,
                "Circle height {} should be R·cos(α) = {}",
                h,
                expected_h
            );
            let expected_r = expected_h * half_angle.tan();
            assert!(
                (*radius - expected_r).abs() < tau,
                "Circle radius {} should be {}",
                radius,
                expected_r
            );
        } else {
            panic!("Expected Circle, got {:?}", curves[0]);
        }

        validate_cone_sphere_results(
            &curves,
            apex,
            axis,
            half_angle,
            z_min,
            z_max,
            sphere_center,
            sphere_radius,
        );
    }

    #[test]
    fn cs_cone_12_negative_z_range() {
        // Cone: apex at origin, axis +Z, half-angle 30°, z in [0, 10].
        // Sphere centered below apex at (0, 0, -5), radius 8.
        // t_proj = -5 (negative). The sphere is large enough to reach the cone
        // at positive h values.
        // Coaxial: (1+tan²30°)h² + 10h + (25 - 64) = 0
        //   (4/3)h² + 10h - 39 = 0
        //   h = (-10 ± √(100 + 208)) / (8/3) = (-10 ± √308) / (8/3)
        //   √308 ≈ 17.55
        //   h1 = (-10 + 17.55)*3/8 ≈ 2.83  (positive, valid)
        //   h2 = (-10 - 17.55)*3/8 ≈ -10.33 (negative, filtered)
        let half_angle = std::f64::consts::FRAC_PI_6;
        let apex = [0.0, 0.0, 0.0];
        let axis = [0.0, 0.0, 1.0];
        let z_min = 0.0;
        let z_max = 10.0;
        let sphere_center = [0.0, 0.0, -5.0];
        let sphere_radius = 8.0;

        let curves = cone_sphere_ssi(
            apex,
            axis,
            half_angle,
            z_min,
            z_max,
            sphere_center,
            sphere_radius,
        )
        .unwrap();

        // Should produce 1 circle at h ≈ 2.83
        assert_eq!(
            curves.len(),
            1,
            "Sphere below apex should produce 1 circle, got {}",
            curves.len()
        );

        let tau = crate::units::TAU_MODEL;
        if let SSICurve::Circle { center, radius, .. } = &curves[0] {
            let h = center[2];
            // h must be positive and within z_range
            assert!(h > 0.0, "Circle height must be positive, got {}", h);
            assert!(
                h <= z_max + tau,
                "Circle height {} exceeds z_max {}",
                h,
                z_max
            );
            // Radius must be positive
            assert!(
                *radius > 0.0,
                "Circle radius must be positive, got {}",
                radius
            );
        } else {
            panic!("Expected Circle, got {:?}", curves[0]);
        }

        validate_cone_sphere_results(
            &curves,
            apex,
            axis,
            half_angle,
            z_min,
            z_max,
            sphere_center,
            sphere_radius,
        );
    }

    // ── Plane-Torus SSI tests ─────────────────────────────────────────────

    #[test]
    fn pt_01_equatorial_plane() {
        // Plane through torus center, normal = torus axis.
        // Torus: center (0,0,0), axis +Z, R=5, r=2.
        // Expected: 2 circles at radii R+r=7 and R-r=3, centered at origin, normal +Z.
        let tau = crate::units::TAU_MODEL;
        let curves = plane_torus_ssi(
            [0.0, 0.0, 0.0], // plane origin
            [0.0, 0.0, 1.0], // plane normal
            [0.0, 0.0, 0.0], // torus center
            [0.0, 0.0, 1.0], // torus axis
            5.0,             // major radius R
            2.0,             // minor radius r
        )
        .unwrap();

        assert_eq!(
            curves.len(),
            2,
            "Equatorial plane should produce 2 circles, got {}",
            curves.len()
        );

        let mut radii: Vec<f64> = curves
            .iter()
            .map(|c| match c {
                SSICurve::Circle { radius, .. } => *radius,
                other => panic!("Expected Circle, got {:?}", other),
            })
            .collect();
        radii.sort_by(|a, b| a.partial_cmp(b).unwrap());

        assert!(
            (radii[0] - 3.0).abs() < tau,
            "Inner radius should be 3.0, got {}",
            radii[0]
        );
        assert!(
            (radii[1] - 7.0).abs() < tau,
            "Outer radius should be 7.0, got {}",
            radii[1]
        );

        for curve in &curves {
            if let SSICurve::Circle { center, normal, .. } = curve {
                assert!(center[0].abs() < tau, "Circle center x should be 0");
                assert!(center[1].abs() < tau, "Circle center y should be 0");
                assert!(center[2].abs() < tau, "Circle center z should be 0");
                let dot = v3_dot(*normal, [0.0, 0.0, 1.0]).abs();
                assert!(
                    (dot - 1.0).abs() < tau,
                    "Normal should be parallel to +Z, dot={}",
                    dot
                );
            }
        }
    }

    #[test]
    fn pt_02_disjoint() {
        // Plane at z=10, torus at origin with R=5, r=2.
        // Distance |10| > r=2, so disjoint → empty.
        let curves = plane_torus_ssi(
            [0.0, 0.0, 10.0], // plane origin
            [0.0, 0.0, 1.0],  // plane normal
            [0.0, 0.0, 0.0],  // torus center
            [0.0, 0.0, 1.0],  // torus axis
            5.0,              // R
            2.0,              // r
        )
        .unwrap();

        assert!(
            curves.is_empty(),
            "Disjoint plane-torus should return empty, got {} curves",
            curves.len()
        );
    }

    #[test]
    fn pt_03_tangent_top() {
        // Plane at z=r (exactly at top of torus tube).
        // Torus: center (0,0,0), axis +Z, R=5, r=2. Plane at z=2.
        // Tangent → 1 circle at radius R=5.
        let tau = crate::units::TAU_MODEL;
        let curves = plane_torus_ssi(
            [0.0, 0.0, 2.0], // plane origin at z=r
            [0.0, 0.0, 1.0], // plane normal
            [0.0, 0.0, 0.0], // torus center
            [0.0, 0.0, 1.0], // torus axis
            5.0,             // R
            2.0,             // r
        )
        .unwrap();

        assert_eq!(
            curves.len(),
            1,
            "Tangent plane should produce 1 circle, got {}",
            curves.len()
        );

        if let SSICurve::Circle {
            center,
            normal,
            radius,
        } = &curves[0]
        {
            assert!(
                (radius - 5.0).abs() < tau,
                "Tangent circle radius should be R=5, got {}",
                radius
            );
            assert!(center[0].abs() < tau, "Center x should be 0");
            assert!(center[1].abs() < tau, "Center y should be 0");
            assert!(
                (center[2] - 2.0).abs() < tau,
                "Center z should be 2.0, got {}",
                center[2]
            );
            let dot = v3_dot(*normal, [0.0, 0.0, 1.0]).abs();
            assert!((dot - 1.0).abs() < tau, "Normal should be parallel to +Z");
        } else {
            panic!("Expected Circle, got {:?}", curves[0]);
        }
    }

    #[test]
    fn pt_04_perpendicular_offset() {
        // Plane at z=1 (between 0 and r=2).
        // Should produce 2 circles at radii R ± sqrt(r² - d²) = 5 ± sqrt(3).
        let tau = crate::units::TAU_MODEL;
        let d = 1.0_f64;
        let r = 2.0_f64;
        let big_r = 5.0_f64;
        let s = (r * r - d * d).sqrt(); // sqrt(3)
        let expected_outer = big_r + s;
        let expected_inner = big_r - s;

        let curves = plane_torus_ssi(
            [0.0, 0.0, 1.0], // plane origin at z=1
            [0.0, 0.0, 1.0], // plane normal
            [0.0, 0.0, 0.0], // torus center
            [0.0, 0.0, 1.0], // torus axis
            big_r,           // R
            r,               // r
        )
        .unwrap();

        assert_eq!(
            curves.len(),
            2,
            "Offset perpendicular plane should produce 2 circles, got {}",
            curves.len()
        );

        let mut radii: Vec<f64> = curves
            .iter()
            .map(|c| match c {
                SSICurve::Circle { radius, .. } => *radius,
                other => panic!("Expected Circle, got {:?}", other),
            })
            .collect();
        radii.sort_by(|a, b| a.partial_cmp(b).unwrap());

        assert!(
            (radii[0] - expected_inner).abs() < tau,
            "Inner radius should be {}, got {}",
            expected_inner,
            radii[0]
        );
        assert!(
            (radii[1] - expected_outer).abs() < tau,
            "Outer radius should be {}, got {}",
            expected_outer,
            radii[1]
        );

        // Verify centers are at z=1 on the axis
        for curve in &curves {
            if let SSICurve::Circle { center, .. } = curve {
                assert!(center[0].abs() < tau, "Center x should be 0");
                assert!(center[1].abs() < tau, "Center y should be 0");
                assert!(
                    (center[2] - 1.0).abs() < tau,
                    "Center z should be 1.0, got {}",
                    center[2]
                );
            }
        }
    }

    #[test]
    fn pt_05_oblique_not_supported() {
        // Plane with normal at 45° to torus axis → NotSupported.
        let result = plane_torus_ssi(
            [0.0, 0.0, 0.0],
            [FRAC_1_SQRT_2, 0.0, FRAC_1_SQRT_2], // 45° to Z
            [0.0, 0.0, 0.0],
            [0.0, 0.0, 1.0],
            5.0,
            2.0,
        );

        assert!(
            matches!(result, Err(KernelError::NotSupported { .. })),
            "Oblique plane should return NotSupported, got {:?}",
            result
        );
    }

    #[test]
    fn pt_06_parallel_to_axis() {
        // Plane normal ⊥ torus axis (plane parallel to axis) → NotSupported.
        let result = plane_torus_ssi(
            [0.0, 0.0, 0.0],
            [1.0, 0.0, 0.0], // normal ⊥ Z axis
            [0.0, 0.0, 0.0],
            [0.0, 0.0, 1.0],
            5.0,
            2.0,
        );

        assert!(
            matches!(result, Err(KernelError::NotSupported { .. })),
            "Plane parallel to torus axis should return NotSupported, got {:?}",
            result
        );
    }

    #[test]
    fn pt_07_offset_torus_center() {
        // Torus not at origin: center at (10, 20, 30), axis +Z.
        // Perpendicular plane through torus center → 2 circles (equatorial but offset).
        let tau = crate::units::TAU_MODEL;
        let tc = [10.0, 20.0, 30.0];
        let curves = plane_torus_ssi(
            tc,              // plane origin = torus center
            [0.0, 0.0, 1.0], // plane normal
            tc,              // torus center
            [0.0, 0.0, 1.0], // torus axis
            5.0,             // R
            2.0,             // r
        )
        .unwrap();

        assert_eq!(
            curves.len(),
            2,
            "Equatorial offset plane should produce 2 circles, got {}",
            curves.len()
        );

        let mut radii: Vec<f64> = curves
            .iter()
            .map(|c| match c {
                SSICurve::Circle { radius, .. } => *radius,
                other => panic!("Expected Circle, got {:?}", other),
            })
            .collect();
        radii.sort_by(|a, b| a.partial_cmp(b).unwrap());

        assert!(
            (radii[0] - 3.0).abs() < tau,
            "Inner radius should be 3.0, got {}",
            radii[0]
        );
        assert!(
            (radii[1] - 7.0).abs() < tau,
            "Outer radius should be 7.0, got {}",
            radii[1]
        );

        // Verify centers are at the torus center position
        for curve in &curves {
            if let SSICurve::Circle { center, normal, .. } = curve {
                assert!(
                    (center[0] - 10.0).abs() < tau,
                    "Center x should be 10.0, got {}",
                    center[0]
                );
                assert!(
                    (center[1] - 20.0).abs() < tau,
                    "Center y should be 20.0, got {}",
                    center[1]
                );
                assert!(
                    (center[2] - 30.0).abs() < tau,
                    "Center z should be 30.0, got {}",
                    center[2]
                );
                let dot = v3_dot(*normal, [0.0, 0.0, 1.0]).abs();
                assert!((dot - 1.0).abs() < tau, "Normal should be parallel to +Z");
            }
        }
    }

    // ── Adversarial plane-torus SSI tests ─────────────────────────────

    #[test]
    fn pt_08_micro_scale() {
        // Torus near MIN_FEATURE_SIZE: R=1e-4, r=5e-5.
        // Perpendicular plane through center → 2 circles at R±r.
        let tau = crate::units::TAU_MODEL;
        let big_r = 1e-4;
        let r = 5e-5;
        let curves = plane_torus_ssi(
            [0.0, 0.0, 0.0], // plane origin
            [0.0, 0.0, 1.0], // plane normal
            [0.0, 0.0, 0.0], // torus center
            [0.0, 0.0, 1.0], // torus axis
            big_r,
            r,
        )
        .unwrap();

        assert_eq!(
            curves.len(),
            2,
            "Micro-scale equatorial plane should produce 2 circles, got {}",
            curves.len()
        );

        let mut radii: Vec<f64> = curves
            .iter()
            .map(|c| match c {
                SSICurve::Circle { radius, .. } => *radius,
                other => panic!("Expected Circle, got {:?}", other),
            })
            .collect();
        radii.sort_by(|a, b| a.partial_cmp(b).unwrap());

        let expected_inner = big_r - r; // 5e-5
        let expected_outer = big_r + r; // 1.5e-4

        assert!(
            (radii[0] - expected_inner).abs() < tau,
            "Inner radius should be {}, got {}",
            expected_inner,
            radii[0]
        );
        assert!(
            (radii[1] - expected_outer).abs() < tau,
            "Outer radius should be {}, got {}",
            expected_outer,
            radii[1]
        );

        // No NaN in any output
        for curve in &curves {
            if let SSICurve::Circle {
                center,
                normal,
                radius,
            } = curve
            {
                assert!(!radius.is_nan(), "Radius must not be NaN");
                for i in 0..3 {
                    assert!(!center[i].is_nan(), "Center[{}] must not be NaN", i);
                    assert!(!normal[i].is_nan(), "Normal[{}] must not be NaN", i);
                }
            }
        }
    }

    #[test]
    fn pt_09_large_torus() {
        // Large torus: R=1000, r=100. Plane at d=50.
        let tau = crate::units::TAU_MODEL;
        let big_r = 1000.0_f64;
        let r = 100.0_f64;
        let d = 50.0_f64;
        let s = (r * r - d * d).sqrt();
        let expected_outer = big_r + s;
        let expected_inner = big_r - s;

        let curves = plane_torus_ssi(
            [0.0, 0.0, d],   // plane origin at z=50
            [0.0, 0.0, 1.0], // plane normal
            [0.0, 0.0, 0.0], // torus center
            [0.0, 0.0, 1.0], // torus axis
            big_r,
            r,
        )
        .unwrap();

        assert_eq!(
            curves.len(),
            2,
            "Large torus offset plane should produce 2 circles, got {}",
            curves.len()
        );

        let mut radii: Vec<f64> = curves
            .iter()
            .map(|c| match c {
                SSICurve::Circle { radius, .. } => *radius,
                other => panic!("Expected Circle, got {:?}", other),
            })
            .collect();
        radii.sort_by(|a, b| a.partial_cmp(b).unwrap());

        assert!(
            (radii[0] - expected_inner).abs() < 1e-6,
            "Inner radius should be {}, got {}",
            expected_inner,
            radii[0]
        );
        assert!(
            (radii[1] - expected_outer).abs() < 1e-6,
            "Outer radius should be {}, got {}",
            expected_outer,
            radii[1]
        );

        // Verify centers at z=50
        for curve in &curves {
            if let SSICurve::Circle { center, .. } = curve {
                assert!(
                    (center[2] - d).abs() < tau,
                    "Center z should be {}, got {}",
                    d,
                    center[2]
                );
            }
        }
    }

    #[test]
    fn pt_10_near_tangent() {
        // Plane at d = r - 1e-8, just barely inside the torus tube.
        // Should produce 2 circles (not tangent).
        let big_r = 5.0_f64;
        let r = 2.0_f64;
        let d = r - 1e-8; // just inside tangent

        let curves = plane_torus_ssi(
            [0.0, 0.0, d],   // plane origin
            [0.0, 0.0, 1.0], // plane normal
            [0.0, 0.0, 0.0], // torus center
            [0.0, 0.0, 1.0], // torus axis
            big_r,
            r,
        )
        .unwrap();

        assert_eq!(
            curves.len(),
            2,
            "Near-tangent plane (d = r - 1e-8) should produce 2 circles, got {}",
            curves.len()
        );

        let mut radii: Vec<f64> = curves
            .iter()
            .map(|c| match c {
                SSICurve::Circle { radius, .. } => *radius,
                other => panic!("Expected Circle, got {:?}", other),
            })
            .collect();
        radii.sort_by(|a, b| a.partial_cmp(b).unwrap());

        // s = sqrt(r² - d²) ≈ sqrt(r² - (r-1e-8)²) ≈ sqrt(2r * 1e-8) ≈ tiny
        let s = (r * r - d * d).sqrt();
        let expected_inner = big_r - s;
        let expected_outer = big_r + s;

        assert!(
            (radii[0] - expected_inner).abs() < 1e-6,
            "Inner radius should be ~{}, got {}",
            expected_inner,
            radii[0]
        );
        assert!(
            (radii[1] - expected_outer).abs() < 1e-6,
            "Outer radius should be ~{}, got {}",
            expected_outer,
            radii[1]
        );

        // Inner circle should be very close to R (very small s)
        assert!(
            s < 1e-3,
            "s should be very small for near-tangent, got {}",
            s
        );
        assert!(
            radii[0] > 0.0,
            "Inner radius must be positive, got {}",
            radii[0]
        );
    }

    #[test]
    fn pt_11_points_on_torus() {
        // Equatorial cut (d=0): sample 16 points on each returned circle
        // and verify they lie on the torus surface within TAU_MODEL.
        let tau = crate::units::TAU_MODEL;
        let big_r = 5.0_f64;
        let r = 2.0_f64;
        let torus_center = [0.0, 0.0, 0.0];
        let torus_axis = [0.0, 0.0, 1.0];

        let curves = plane_torus_ssi(
            [0.0, 0.0, 0.0], // plane origin
            [0.0, 0.0, 1.0], // plane normal
            torus_center,
            torus_axis,
            big_r,
            r,
        )
        .unwrap();

        assert_eq!(curves.len(), 2);

        for curve in &curves {
            let (center, _normal, radius) = match curve {
                SSICurve::Circle {
                    center,
                    normal,
                    radius,
                } => (*center, *normal, *radius),
                other => panic!("Expected Circle, got {:?}", other),
            };

            // Sample 16 points on the circle
            for i in 0..16 {
                let theta = 2.0 * std::f64::consts::PI * (i as f64) / 16.0;
                let px = center[0] + radius * theta.cos();
                let py = center[1] + radius * theta.sin();
                let pz = center[2];
                let p = [px, py, pz];

                // Check point lies on torus surface:
                // distance from point to nearest point on major circle == r
                let v = v3_sub(p, torus_center);
                let axial = v3_dot(v, torus_axis);
                let radial_vec = v3_sub(v, v3_scale(torus_axis, axial));
                let radial_dist = v3_length(radial_vec);
                let tube_dist = ((radial_dist - big_r).powi(2) + axial.powi(2)).sqrt();

                assert!(
                    (tube_dist - r).abs() < tau,
                    "Point {} on circle r={} is not on torus surface: \
                     tube_dist={}, expected r={}, diff={}",
                    i,
                    radius,
                    tube_dist,
                    r,
                    (tube_dist - r).abs()
                );
            }
        }
    }

    #[test]
    fn pt_12_spindle_torus() {
        // Spindle torus: r >= R. R=2, r=3.
        // Equatorial plane at d=0. Inner radius = R - r = -1 → negative,
        // so only the outer circle (radius R + r = 5) should be returned.
        let tau = crate::units::TAU_MODEL;
        let big_r = 2.0_f64;
        let r = 3.0_f64;

        let curves = plane_torus_ssi(
            [0.0, 0.0, 0.0], // plane origin
            [0.0, 0.0, 1.0], // plane normal
            [0.0, 0.0, 0.0], // torus center
            [0.0, 0.0, 1.0], // torus axis
            big_r,
            r,
        )
        .unwrap();

        assert_eq!(
            curves.len(),
            1,
            "Spindle torus equatorial plane should produce 1 circle (inner radius negative), got {}",
            curves.len()
        );

        if let SSICurve::Circle {
            center,
            normal,
            radius,
        } = &curves[0]
        {
            let expected_outer = big_r + r; // 5.0
            assert!(
                (radius - expected_outer).abs() < tau,
                "Outer circle radius should be {}, got {}",
                expected_outer,
                radius
            );
            assert!(center[0].abs() < tau, "Center x should be 0");
            assert!(center[1].abs() < tau, "Center y should be 0");
            assert!(center[2].abs() < tau, "Center z should be 0");
            let dot = v3_dot(*normal, [0.0, 0.0, 1.0]).abs();
            assert!((dot - 1.0).abs() < tau, "Normal should be parallel to +Z");
        } else {
            panic!("Expected Circle, got {:?}", curves[0]);
        }
    }
}
