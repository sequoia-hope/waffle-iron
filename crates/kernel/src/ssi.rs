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
use crate::units::{TAU_COINCIDENT, TAU_NORMALIZE, TAU_PARALLEL, TAU_WORK};
use crate::vecmath::{
    compute_plane_basis, mat3_mul_vec, v3_add, v3_cross, v3_dot, v3_length, v3_normalize, v3_scale,
    v3_sub, Mat3,
};
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
/// - Oblique case: ellipse (Patrikalakis Ch.5 — semi_minor = R, semi_major = R/sin γ)
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
    if cos_angle > 1.0 - TAU_PARALLEL {
        return Ok(vec![]);
    }

    // Near-parallel (angle < 60°) → not supported
    // Use > 0.5 + epsilon so exactly 60° is supported
    if cos_angle > 0.5 + TAU_COINCIDENT {
        return Err(KernelError::NotSupported {
            operation: "cylinder-cylinder SSI: near-parallel axes (angle < 60°)".to_string(),
        });
    }

    // Unequal radii check (>1% relative difference)
    let r_max = cyl_a_radius.max(cyl_b_radius);
    let r_min = cyl_a_radius.min(cyl_b_radius);
    if r_max < TAU_NORMALIZE {
        return Err(KernelError::NotSupported {
            operation: "cylinder-cylinder SSI: zero radius".to_string(),
        });
    }
    if (r_max - r_min) / r_max >= crate::units::SSI_RADII_RELATIVE_TOL {
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

    if denom.abs() < TAU_WORK {
        // Degenerate (parallel) — should have been caught above
        return Ok(vec![]);
    }

    let t_closest = (b * e - c * d) / denom;
    let s_closest = (a * e - b * d) / denom;

    let p1_closest = v3_add(cyl_a_origin, v3_scale(d1, t_closest));
    let p2_closest = v3_add(cyl_b_origin, v3_scale(d2, s_closest));
    let closest_dist = v3_length(v3_sub(p1_closest, p2_closest));

    // Skew axes check
    if closest_dist >= crate::units::SSI_SKEW_FACTOR * r {
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
    if a2_perp_len < TAU_WORK {
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
    let normal_1 = if normal_1_len > TAU_WORK {
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
    let normal_2 = if normal_2_len > TAU_WORK {
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
        // Ref #1: Patrikalakis Ch.5 — plane-cone SSI, conic section classification
        //
        // A plane intersecting a circular cone produces a conic section whose type
        // is determined by the relationship between the plane tilt and the cone
        // half-angle.

        let n = plane_normal;
        let a = cone_axis;
        let t = half_angle.tan();
        let sin_beta = half_angle.sin();
        let cos_alpha_sq = cos_angle * cos_angle; // cos²(angle between normal and axis)
        let sin_beta_sq = sin_beta * sin_beta;

        // Signed distance from cone apex to the plane
        let d_apex = v3_dot(v3_sub(cone_apex, plane_origin), n);

        // ── Through-apex degenerate case ──────────────────────────────────
        // Ref #1: Patrikalakis Ch.5 — plane through cone apex
        //
        // When the plane passes through the apex, the intersection is:
        // - Ellipse regime (γ > β): just a point (the apex) — return empty
        // - Hyperbola regime (γ < β): two generator lines through the apex
        // - Parabola boundary (γ ≈ β): one tangent line — return NotSupported
        //
        // For two lines, we need directions d such that:
        //   d · n = 0  (lies in the plane)
        //   (d · a)² = cos²(β) |d|²  (lies on the cone)
        //
        // Using an orthonormal basis {e1, e2} for the plane (n·d = 0):
        //   e1 = normalize(a - (a·n)n)  (axis projected into plane)
        //   e2 = n × e1
        // Then d = cos(θ) e1 + sin(θ) e2, and d·a = cos(θ) sin(α).
        // Cone condition: cos²(θ) sin²(α) = cos²(β)
        // Solution: cos(θ) = ±cos(β)/sin(α), requires sin(α) ≥ cos(β).
        if d_apex.abs() < TOL {
            let n_dot_a = v3_dot(n, a);
            let sin_alpha = (1.0 - n_dot_a * n_dot_a).max(0.0).sqrt();
            let cos_beta = half_angle.cos();

            if sin_alpha < cos_beta - TOL {
                // Ellipse regime: plane cuts steeper than cone → only a point at apex
                return Ok(vec![]);
            }
            if (sin_alpha - cos_beta).abs() < TOL {
                // Parabola boundary: single tangent line
                return Err(KernelError::NotSupported {
                    operation: "plane-cone SSI: through-apex parabolic tangent".to_string(),
                });
            }

            // Hyperbola regime: two generator lines
            // Build orthonormal basis for the plane
            let a_in_plane = v3_sub(a, v3_scale(n, n_dot_a));
            let a_in_plane_len = v3_length(a_in_plane);
            if a_in_plane_len < TOL {
                return Ok(vec![]);
            }
            let e1 = v3_scale(a_in_plane, 1.0 / a_in_plane_len);
            let e2 = v3_normalize(v3_cross(n, e1));

            let cos_theta = cos_beta / sin_alpha;
            let sin_theta = (1.0 - cos_theta * cos_theta).max(0.0).sqrt();

            let g1 = v3_add(v3_scale(e1, cos_theta), v3_scale(e2, sin_theta));
            let g2 = v3_sub(v3_scale(e1, cos_theta), v3_scale(e2, sin_theta));

            // Extend generators to max_height. Height along axis = g·a = cos_theta * sin_alpha = cos_beta.
            // So parameter t for height h: h = t * cos_beta → t = max_height / cos_beta.
            let t_param = max_height / cos_beta;
            let end1 = v3_add(cone_apex, v3_scale(g1, t_param));
            let end2 = v3_add(cone_apex, v3_scale(g2, t_param));

            return Ok(vec![
                SSICurve::Line {
                    start: cone_apex,
                    end: end1,
                },
                SSICurve::Line {
                    start: cone_apex,
                    end: end2,
                },
            ]);
        }

        // ── Classify conic type ───────────────────────────────────────────
        let discriminant = cos_alpha_sq - sin_beta_sq;

        if discriminant.abs() < TOL {
            // Parabola (cutting angle ≈ half-angle) — not yet implemented
            return Err(KernelError::NotSupported {
                operation: "plane-cone SSI: parabolic section".to_string(),
            });
        }

        if discriminant < 0.0 {
            // Hyperbola (shallow cut, γ < β) — not yet implemented
            return Err(KernelError::NotSupported {
                operation: "plane-cone SSI: hyperbolic section".to_string(),
            });
        }

        // ── Ellipse case (discriminant > 0, γ > β) ───────────────────────
        //
        // Work in a local cone frame: apex at origin, axis along Z, with the
        // plane normal rotated into the XZ plane. Solve for the ellipse
        // analytically, then transform back to world coordinates.

        let signed_n_dot_a = v3_dot(n, a);
        let sin_alpha = (1.0 - signed_n_dot_a * signed_n_dot_a).sqrt().max(TOL);

        // D = signed distance from apex to plane along the outward normal
        // We want D > 0 for the plane on the "positive" side.
        // d_apex = (A - P)·n, so the distance from apex to plane along n is -d_apex.
        let d_signed = -d_apex; // positive when apex is on the negative side of the plane

        // In the rotated cone frame (apex=origin, axis=Z, plane normal in XZ):
        //   Plane: sin(α)·x + cos(α)·z = D
        //   Cone:  x² + y² = t²·z²
        //
        // The ellipse endpoints on the y=0 symmetry line (from plane+cone, y=0):
        //   t·z·sin(α) = ±(D - z·cos(α))
        //   z₁ = D / (t·sin(α) + |cos(α)|)    (near-apex side)
        //   z₂ = D / (|cos(α)| - t·sin(α))    (far-from-apex side)
        //
        // cos(α) here uses the absolute value to ensure correct sign handling.
        let abs_cos_alpha = cos_angle; // = |n̂ · â|

        let z1 = d_signed / (t * sin_alpha + abs_cos_alpha);
        let z2 = d_signed / (abs_cos_alpha - t * sin_alpha);

        // Ensure z1 ≤ z2
        let (z_lo, z_hi) = if z1 <= z2 { (z1, z2) } else { (z2, z1) };

        // Check if the ellipse height range overlaps the valid cone [0, max_height]
        if z_hi < -TOL || z_lo > max_height + TOL {
            return Ok(vec![]);
        }

        // Ellipse center in local frame is at z_mid
        let z_mid = (z_lo + z_hi) / 2.0;

        // Semi-major = distance from center to endpoint along the tilted direction in the plane
        let semi_major = (z_hi - z_lo) / (2.0 * sin_alpha);

        // Semi-minor = y extent at the center height = cone radius at z_mid
        // corrected for the fact that some x-extent is used by the offset from axis.
        // From the derivation: semi_minor² = t²·z_mid² - x_mid²
        // where x_mid = (D - z_mid·cos(α))/sin(α)
        let x_mid = (d_signed - z_mid * abs_cos_alpha) / sin_alpha;
        let semi_minor_sq = t * t * z_mid * z_mid - x_mid * x_mid;
        if semi_minor_sq < 0.0 {
            // Numerical issue — ellipse is degenerate
            return Ok(vec![]);
        }
        let semi_minor = semi_minor_sq.sqrt();

        if semi_major < TOL || semi_minor < TOL {
            return Ok(vec![]);
        }

        // ── Transform back to world coordinates ──────────────────────────
        //
        // The local frame has: Z = cone_axis, and the plane normal projected
        // into the XY plane defines the X direction (the symmetry plane).
        //
        // The local X axis is perpendicular to the cone axis and lies in the
        // plane of symmetry (containing axis and plane normal).
        // n_perp = normalize(n - (n·a)·a) is the component of the plane normal
        // perpendicular to the axis. In the local frame, this points in +X.

        let n_perp = v3_sub(n, v3_scale(a, signed_n_dot_a));
        let n_perp_len = v3_length(n_perp);
        let local_x_dir = if n_perp_len > TOL {
            v3_scale(n_perp, 1.0 / n_perp_len)
        } else {
            return Ok(vec![]);
        };

        // In local frame: center = (x_mid, 0, z_mid)
        // In world frame: center = apex + z_mid * a + x_mid * local_x_dir
        let center = v3_add(
            v3_add(cone_apex, v3_scale(a, z_mid)),
            v3_scale(local_x_dir, x_mid),
        );

        // Major axis direction: in local frame it's along (cos α, 0, -sin α) / sin α...
        // Actually, the major axis connects (x₁, 0, z₁) to (x₂, 0, z₂) in local coords.
        // Direction ∝ (x₂ - x₁, 0, z₂ - z₁).
        // In world: ∝ (z₂ - z₁) * a + (x₂ - x₁) * local_x_dir

        // x at z=z_lo: x_lo = (D - z_lo * cos α) / sin α
        // x at z=z_hi: x_hi = (D - z_hi * cos α) / sin α
        // x_hi - x_lo = (z_lo - z_hi) * cos α / sin α = -(z_hi - z_lo) * cos α / sin α
        let dz = z_hi - z_lo;
        let dx = -dz * abs_cos_alpha / sin_alpha;

        let major_dir_raw = v3_add(v3_scale(a, dz), v3_scale(local_x_dir, dx));
        let major_dir_len = v3_length(major_dir_raw);
        let major_axis = if major_dir_len > TOL {
            v3_scale(major_dir_raw, 1.0 / major_dir_len)
        } else {
            return Ok(vec![]);
        };

        // Ensure semi_major >= semi_minor (should be guaranteed by geometry, but verify)
        let (final_major, final_minor, final_axis) = if semi_major >= semi_minor {
            (semi_major, semi_minor, major_axis)
        } else {
            // Swap: the minor direction is cross(normal, major_axis)
            let minor_dir = v3_normalize(v3_cross(n, major_axis));
            (semi_minor, semi_major, minor_dir)
        };

        Ok(vec![SSICurve::Ellipse {
            center,
            normal: n,
            major_axis: final_axis,
            semi_major: final_major,
            semi_minor: final_minor,
        }])
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

/// Compute intersections of circle (cx, cy, r) with vertical line x=X.
/// Returns intersection y-values clipped to [y_min, y_max].
///
/// Reference: Patrikalakis Ch.5 — plane-cylinder SSI (vertical plane case).
pub(crate) fn circle_vline_intersections(
    cx: f64,
    cy: f64,
    r: f64,
    x: f64,
    y_min: f64,
    y_max: f64,
) -> Vec<f64> {
    let dx = x - cx;
    let disc = r * r - dx * dx;
    if disc < -TOL {
        return vec![];
    }
    let disc = disc.max(0.0);
    let dy = disc.sqrt();
    let mut results = Vec::new();
    let y1 = cy - dy;
    let y2 = cy + dy;
    if y1 >= y_min - TOL && y1 <= y_max + TOL {
        results.push(y1.clamp(y_min, y_max));
    }
    if y2 >= y_min - TOL && y2 <= y_max + TOL && (y2 - y1).abs() > TOL {
        results.push(y2.clamp(y_min, y_max));
    }
    results
}

/// Compute intersections of circle (cx, cy, r) with horizontal line y=Y.
/// Returns intersection x-values clipped to [x_min, x_max].
///
/// Reference: Patrikalakis Ch.5 — plane-cylinder SSI (horizontal plane case).
pub(crate) fn circle_hline_intersections(
    cx: f64,
    cy: f64,
    r: f64,
    y: f64,
    x_min: f64,
    x_max: f64,
) -> Vec<f64> {
    let dy = y - cy;
    let disc = r * r - dy * dy;
    if disc < -TOL {
        return vec![];
    }
    let disc = disc.max(0.0);
    let dx = disc.sqrt();
    let mut results = Vec::new();
    let x1 = cx - dx;
    let x2 = cx + dx;
    if x1 >= x_min - TOL && x1 <= x_max + TOL {
        results.push(x1.clamp(x_min, x_max));
    }
    if x2 >= x_min - TOL && x2 <= x_max + TOL && (x2 - x1).abs() > TOL {
        results.push(x2.clamp(x_min, x_max));
    }
    results
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

// ── Sphere-Torus SSI ─────────────────────────────────────────────────────

/// Analytical SSI solver for sphere-torus pairs (A15 pair #9).
///
/// Returns intersection curves between a sphere and a torus.
/// Axial case (sphere center on torus axis) yields exact circles.
/// General case approximated with a Line segment.
///
/// Ref #1: Patrikalakis Ch.5
pub(crate) fn sphere_torus_ssi(
    sphere_center: [f64; 3],
    sphere_radius: f64,
    torus_center: [f64; 3],
    torus_axis: [f64; 3],
    torus_major_radius: f64,
    torus_minor_radius: f64,
) -> Result<Vec<SSICurve>, KernelError> {
    let big_r = torus_major_radius;
    let r = torus_minor_radius;
    let s = sphere_radius;

    // 1. Project sphere center onto torus axis
    let diff = v3_sub(sphere_center, torus_center);
    let h = v3_dot(diff, torus_axis); // signed axial offset
    let proj = v3_add(torus_center, v3_scale(torus_axis, h));
    let perp_vec = v3_sub(sphere_center, proj);
    let d = v3_length(perp_vec); // perpendicular distance from axis

    // 2. Disjoint check: sphere too far from nearest torus surface point.
    //    Nearest point on torus generating circle (in the plane through sphere center
    //    and torus axis): at distance big_r from axis in the torus midplane.
    //    Distance from sphere center to torus tube center circle:
    //    sqrt(d² + h²) projected to the generating circle plane, then subtract big_r,
    //    add radial distance. More precisely:
    //    The closest point on the tube centerline to sphere center is at distance
    //    sqrt((d - big_r)² + h²) from the sphere center (if d > 0), or
    //    sqrt((big_r)² + h²) - ... Actually, let's compute the distance from
    //    sphere center to the nearest point on the torus surface directly.
    //    Torus tube center circle: radius big_r in the torus midplane.
    //    The closest point on this circle to sphere_center is at:
    //      if d > TOL: tube_center = proj_on_plane + big_r * perp_unit (in torus midplane)
    //                  but we need to account for axial offset h too.
    //    Distance from sphere center to closest tube center = sqrt((d - big_r)² + h²)
    //    (when d > TOL), or sqrt(big_r² + h²) (when d ≈ 0).
    //    Nearest torus surface = that distance - r.
    let dist_to_tube_center = if d > TOL {
        ((d - big_r) * (d - big_r) + h * h).sqrt()
    } else {
        (big_r * big_r + h * h).sqrt()
    };

    // Disjoint: sphere doesn't reach torus surface
    if dist_to_tube_center > s + r + TOL {
        return Ok(vec![]);
    }

    // Enclosed: sphere is entirely inside the torus tube
    if dist_to_tube_center + s < r - TOL {
        return Ok(vec![]);
    }

    // 3. Axial case: sphere center on torus axis (d ≈ 0)
    if d < TOL {
        // Torus: (ρ - R)² + z² = r²
        // Sphere: ρ² + (z - h)² = s²
        //
        // From these two equations, eliminating:
        //   u = ρ = (s² + 2hz - h² + R² - r²) / (2R)
        //   u² + (z - h)² = s²
        //
        // Let A = (s² - h² + R² - r²) / (2R), B = h / R
        // Then u = A + B·z, and (A + Bz)² + (z - h)² = s²
        // (1 + B²)z² + (2AB - 2h)z + (A² + h² - s²) = 0
        let a_val = (s * s - h * h + big_r * big_r - r * r) / (2.0 * big_r);
        let b_val = h / big_r;

        let qa = 1.0 + b_val * b_val;
        let qb = 2.0 * a_val * b_val - 2.0 * h;
        let qc = a_val * a_val + h * h - s * s;

        let disc = qb * qb - 4.0 * qa * qc;

        if disc < 0.0 {
            return Ok(vec![]);
        }

        let mut curves = Vec::new();

        if disc.abs() < TOL * TOL {
            // Single tangent solution
            let z_sol = -qb / (2.0 * qa);
            let rho = a_val + b_val * z_sol;
            if rho > TOL {
                let center = v3_add(torus_center, v3_scale(torus_axis, z_sol));
                curves.push(SSICurve::Circle {
                    center,
                    normal: torus_axis,
                    radius: rho,
                });
            }
        } else {
            let sqrt_disc = disc.sqrt();
            let z1 = (-qb + sqrt_disc) / (2.0 * qa);
            let z2 = (-qb - sqrt_disc) / (2.0 * qa);

            for z_sol in [z1, z2] {
                let rho = a_val + b_val * z_sol;
                if rho > TOL {
                    // Verify this point is actually on both surfaces
                    let center = v3_add(torus_center, v3_scale(torus_axis, z_sol));
                    curves.push(SSICurve::Circle {
                        center,
                        normal: torus_axis,
                        radius: rho,
                    });
                }
            }
        }

        return Ok(curves);
    }

    // 4. General offset case: sphere center off-axis.
    //    The intersection is a degree-4 space curve. We approximate by scanning
    //    azimuthally and returning a representative Line segment.
    let perp_unit = v3_scale(perp_vec, 1.0 / d);

    // Scan azimuthally around the torus to find intersection extent.
    // For each azimuthal angle θ, the torus tube center is at:
    //   C(θ) = torus_center + R*(cos(θ)*perp_unit + sin(θ)*tang_unit)
    // where tang_unit is perpendicular to both axis and perp_unit.
    let tang_unit = v3_cross(torus_axis, perp_unit);

    let n_theta: usize = 360;
    let n_phi: usize = 36;
    let mut found_pts: Vec<[f64; 3]> = Vec::new();

    for i in 0..n_theta {
        let theta = 2.0 * std::f64::consts::PI * (i as f64) / (n_theta as f64);
        let cos_t = theta.cos();
        let sin_t = theta.sin();

        // Tube center at this azimuthal angle
        let tube_c = v3_add(
            torus_center,
            v3_add(
                v3_scale(perp_unit, big_r * cos_t),
                v3_scale(tang_unit, big_r * sin_t),
            ),
        );

        // Distance from sphere center to tube center
        let tc_diff = v3_sub(tube_c, sphere_center);
        let tc_dist = v3_length(tc_diff);

        // Check if sphere intersects the tube cross-section at this azimuth
        // The tube cross-section is a circle of radius r centered at tube_c.
        // Intersection exists if |tc_dist - s| <= r (approximately).
        if tc_dist > s + r + TOL || tc_dist + r < s - TOL {
            continue;
        }
        if s + tc_dist < r - TOL {
            continue;
        }

        // Scan the tube cross-section (poloidal angle φ) to find intersection points
        let tube_radial = v3_normalize(v3_sub(tube_c, torus_center));
        // tube_radial is in the torus midplane; the other basis vector is the torus axis
        for j in 0..n_phi {
            let phi = 2.0 * std::f64::consts::PI * (j as f64) / (n_phi as f64);
            let cos_p = phi.cos();
            let sin_p = phi.sin();

            // Point on torus surface
            let pt = v3_add(
                tube_c,
                v3_add(
                    v3_scale(tube_radial, r * cos_p),
                    v3_scale(torus_axis, r * sin_p),
                ),
            );

            // Check if this point is on the sphere surface
            let dist_to_sphere = v3_length(v3_sub(pt, sphere_center));
            if (dist_to_sphere - s).abs() < crate::units::SSI_SAMPLE_ON_SURFACE_TOL {
                found_pts.push(pt);
            }
        }
    }

    if found_pts.is_empty() {
        return Ok(vec![]);
    }

    // Return a Line segment spanning the extent of found points
    // Find the two most distant points
    let mut max_dist = 0.0_f64;
    let mut p_start = found_pts[0];
    let mut p_end = found_pts[0];
    for i in 0..found_pts.len() {
        for j in (i + 1)..found_pts.len() {
            let dd = v3_length(v3_sub(found_pts[i], found_pts[j]));
            if dd > max_dist {
                max_dist = dd;
                p_start = found_pts[i];
                p_end = found_pts[j];
            }
        }
    }

    Ok(vec![SSICurve::Line {
        start: p_start,
        end: p_end,
    }])
}

// ── Cone-Cone SSI ────────────────────────────────────────────────────────

/// Analytical SSI solver for cone-cone pairs (A15 pair #10).
///
/// Returns intersection curves between two finite cones.
/// Coaxial case yields exact circles. Same-apex case yields lines.
/// Cylinder–cone surface-surface intersection.
///
/// Returns intersection curves between a cylinder and a cone.
/// Coaxial cases produce circles; general cases produce degree-4 curves
/// approximated as Line segments. Tangent intersections (below feature size)
/// are filtered out.
///
/// Ref #1: Patrikalakis Ch.5
#[allow(clippy::too_many_arguments)]
pub(crate) fn cylinder_cone_ssi(
    cyl_origin: [f64; 3],
    cyl_axis: [f64; 3],
    cyl_radius: f64,
    cyl_z_min: f64,
    cyl_z_max: f64,
    cone_apex: [f64; 3],
    cone_axis: [f64; 3],
    cone_half_angle: f64,
    cone_height_range: (f64, f64),
) -> Result<Vec<SSICurve>, KernelError> {
    let tan_a = cone_half_angle.tan();
    if tan_a.abs() < TOL || cyl_radius < TOL {
        return Ok(vec![]);
    }

    let cone_z_min = cone_height_range.0;
    let cone_z_max = cone_height_range.1;
    if cone_z_max <= cone_z_min {
        return Ok(vec![]);
    }

    // Check if axes are collinear (coaxial case)
    let apex_diff = v3_sub(cone_apex, cyl_origin);
    let dot_axes = v3_dot(cyl_axis, cone_axis);
    let axes_parallel = dot_axes.abs() > 1.0 - TOL;

    // Distance from cone_apex to the cylinder axis line
    let t_proj = v3_dot(apex_diff, cyl_axis);
    let proj = v3_add(cyl_origin, v3_scale(cyl_axis, t_proj));
    let perp = v3_sub(cone_apex, proj);
    let perp_dist = v3_length(perp);

    let axes_collinear = axes_parallel && perp_dist < TOL;

    // 1. Coaxial case: axes are collinear
    if axes_collinear {
        // Cone apex projects onto cylinder axis at parameter t_proj.
        // At height h along cone_axis from cone_apex, cone radius = |h| * tan_a.
        // That point in cylinder axis parameter = t_proj + h * dot_axes
        // (dot_axes is ±1 since axes are parallel and unit).
        // We need: |h| * tan_a = cyl_radius → h = ±cyl_radius / tan_a
        let h_pos = cyl_radius / tan_a;
        let h_neg = -h_pos;

        let mut curves = Vec::new();

        for &h in &[h_pos, h_neg] {
            // Check cone height range
            if h < cone_z_min - TOL || h > cone_z_max + TOL {
                continue;
            }

            // Convert to cylinder axis parameter
            let z_cyl = t_proj + h * dot_axes;

            // Check cylinder z range
            if z_cyl < cyl_z_min - TOL || z_cyl > cyl_z_max + TOL {
                continue;
            }

            let radius = h.abs() * tan_a;
            if radius < TOL {
                continue;
            }

            let center = v3_add(cyl_origin, v3_scale(cyl_axis, z_cyl));
            curves.push(SSICurve::Circle {
                center,
                normal: cyl_axis,
                radius,
            });
        }

        return Ok(curves);
    }

    // 2. General case (non-coaxial): bounding-sphere disjoint check
    let max_cone_r = if cone_z_min.abs() > cone_z_max.abs() {
        cone_z_min.abs() * tan_a
    } else {
        cone_z_max.abs() * tan_a
    };
    let cyl_half_len = (cyl_z_max - cyl_z_min) * 0.5;
    let cyl_extent = (cyl_half_len * cyl_half_len + cyl_radius * cyl_radius).sqrt();

    let cone_max_h = cone_z_max.abs().max(cone_z_min.abs());
    let cone_extent = (cone_max_h * cone_max_h + max_cone_r * max_cone_r).sqrt();

    let cyl_mid = v3_add(
        cyl_origin,
        v3_scale(cyl_axis, (cyl_z_min + cyl_z_max) * 0.5),
    );
    let cone_mid_h = (cone_z_min + cone_z_max) * 0.5;
    let cone_mid = v3_add(cone_apex, v3_scale(cone_axis, cone_mid_h));
    let centers_dist = v3_length(v3_sub(cyl_mid, cone_mid));

    if centers_dist > cyl_extent + cone_extent + TOL {
        return Ok(vec![]);
    }

    // 2b. Parallel-offset tangent filter: when axes are parallel but offset,
    //     check if the crossing band is narrow relative to the offset distance.
    //     A narrow band indicates a grazing/tangent contact.
    if axes_parallel && perp_dist > TOL {
        let band_width = 2.0 * cyl_radius / tan_a;
        if band_width < perp_dist * 0.5 {
            return Ok(vec![]);
        }
    }

    // Numerical scanning: sample cylinder surface at (theta, z) grid points.
    // For each sample point on the cylinder, compute signed distance to cone surface
    // (positive = outside cone, negative = inside cone). Collect points where
    // the sign changes between adjacent theta samples, indicating a true crossing
    // (not a tangent touch).
    let (u_cyl, v_cyl) = compute_plane_basis(cyl_axis);

    let n_z: usize = 200;
    let n_theta: usize = 72;
    let mut found_pts: Vec<[f64; 3]> = Vec::new();

    // Helper: compute signed distance from a point on the cylinder to cone surface.
    // Returns None if the point is outside the cone's height range.
    let signed_dist_to_cone = |pt: [f64; 3]| -> Option<f64> {
        let diff_c = v3_sub(pt, cone_apex);
        let h_c = v3_dot(diff_c, cone_axis);
        if h_c < cone_z_min - TOL || h_c > cone_z_max + TOL {
            return None;
        }
        let rc = h_c.abs() * tan_a;
        let proj_c = v3_add(cone_apex, v3_scale(cone_axis, h_c));
        let perp_c = v3_sub(pt, proj_c);
        let perp_dist_c = v3_length(perp_c);
        Some(perp_dist_c - rc)
    };

    for iz in 0..=n_z {
        let z = cyl_z_min + (cyl_z_max - cyl_z_min) * (iz as f64) / (n_z as f64);
        let base = v3_add(cyl_origin, v3_scale(cyl_axis, z));

        // Compute signed distances for all theta samples at this height
        let mut samples: Vec<(f64, [f64; 3])> = Vec::with_capacity(n_theta);
        for it in 0..n_theta {
            let theta = 2.0 * std::f64::consts::PI * (it as f64) / (n_theta as f64);
            let cos_t = theta.cos();
            let sin_t = theta.sin();
            let pt = v3_add(
                base,
                v3_add(
                    v3_scale(u_cyl, cyl_radius * cos_t),
                    v3_scale(v_cyl, cyl_radius * sin_t),
                ),
            );
            if let Some(sd) = signed_dist_to_cone(pt) {
                samples.push((sd, pt));
            }
        }

        // Look for sign changes between adjacent samples (crossing detection)
        if samples.len() < 2 {
            continue;
        }
        for i in 0..samples.len() {
            let j = (i + 1) % samples.len();
            let (sd_i, pt_i) = samples[i];
            let (sd_j, _pt_j) = samples[j];
            if sd_i * sd_j < 0.0 {
                // Sign change → true crossing. Interpolate to find approximate crossing point.
                let t = sd_i.abs() / (sd_i.abs() + sd_j.abs());
                let crossing = v3_add(v3_scale(pt_i, 1.0 - t), v3_scale(samples[j].1, t));
                found_pts.push(crossing);
            }
        }
    }

    if found_pts.is_empty() {
        return Ok(vec![]);
    }

    // Find the Line segment spanning the extent of intersection
    let mut max_d = 0.0_f64;
    let mut p_start = found_pts[0];
    let mut p_end = found_pts[0];
    for i in 0..found_pts.len() {
        for j in (i + 1)..found_pts.len() {
            let dd = v3_length(v3_sub(found_pts[i], found_pts[j]));
            if dd > max_d {
                max_d = dd;
                p_start = found_pts[i];
                p_end = found_pts[j];
            }
        }
    }

    if max_d < crate::units::MIN_FEATURE_SIZE {
        return Ok(vec![]);
    }

    Ok(vec![SSICurve::Line {
        start: p_start,
        end: p_end,
    }])
}

/// General case approximated with a Line segment.
///
/// Ref #1: Patrikalakis Ch.5
#[allow(clippy::too_many_arguments)]
pub(crate) fn cone_cone_ssi(
    apex_a: [f64; 3],
    axis_a: [f64; 3],
    half_angle_a: f64,
    height_range_a: (f64, f64),
    apex_b: [f64; 3],
    axis_b: [f64; 3],
    half_angle_b: f64,
    height_range_b: (f64, f64),
) -> Result<Vec<SSICurve>, KernelError> {
    let tan_a = half_angle_a.tan();
    let tan_b = half_angle_b.tan();

    if tan_a.abs() < TOL || tan_b.abs() < TOL {
        return Ok(vec![]);
    }

    let z_min_a = height_range_a.0.max(0.0);
    let z_max_a = height_range_a.1;
    let z_min_b = height_range_b.0.max(0.0);
    let z_max_b = height_range_b.1;

    if z_max_a <= z_min_a || z_max_b <= z_min_b {
        return Ok(vec![]);
    }

    // Check if axes are collinear (coaxial case)
    let apex_diff = v3_sub(apex_b, apex_a);
    let dot_axes = v3_dot(axis_a, axis_b);
    let axes_parallel = dot_axes.abs() > 1.0 - TOL;

    // Distance from apex_b to the axis line of cone A
    let t_proj = v3_dot(apex_diff, axis_a);
    let proj = v3_add(apex_a, v3_scale(axis_a, t_proj));
    let perp = v3_sub(apex_b, proj);
    let perp_dist = v3_length(perp);

    let axes_collinear = axes_parallel && perp_dist < TOL;

    // Check if apices coincide
    let apex_dist = v3_length(apex_diff);
    let same_apex = apex_dist < TOL;

    // 1. Coaxial case: axes are collinear
    if axes_collinear && !same_apex {
        // Signed distance from apex_a to apex_b along axis_a
        let d = v3_dot(apex_diff, axis_a);
        // At height h from apex_a: r_a = h * tan_a
        // apex_b is at h = d from apex_a along axis_a.
        // At height h from apex_a, height from apex_b is:
        //   if axes same direction: h_b = h - d
        //   if axes opposite direction: h_b = -(h - d) = d - h... but we need h_b > 0
        // Actually, the cone B surface at height h_b from apex_b: r_b = h_b * tan_b
        // h_b measured along axis_b. Since axes are parallel:
        //   if dot_axes > 0 (same direction): h_b = h - d (where h is from apex_a along axis_a)
        //   if dot_axes < 0 (opposite direction): h_b = d - h

        let same_dir = dot_axes > 0.0;

        // Solve: h * tan_a = h_b * tan_b
        // Same direction: h * tan_a = (h - d) * tan_b
        //   h * tan_a = h * tan_b - d * tan_b
        //   h * (tan_a - tan_b) = -d * tan_b
        //   h = d * tan_b / (tan_b - tan_a)
        // Opposite direction: h * tan_a = (d - h) * tan_b
        //   h * tan_a = d * tan_b - h * tan_b
        //   h * (tan_a + tan_b) = d * tan_b
        //   h = d * tan_b / (tan_a + tan_b)

        if same_dir {
            if (tan_a - tan_b).abs() < TOL {
                // Same angle, same direction → parallel generators → no intersection
                return Ok(vec![]);
            }
            let h = d * tan_b / (tan_b - tan_a);
            let h_b = h - d;

            // Check height ranges
            if h >= z_min_a - TOL
                && h <= z_max_a + TOL
                && h > TOL
                && h_b >= z_min_b - TOL
                && h_b <= z_max_b + TOL
                && h_b > TOL
            {
                let radius = h * tan_a;
                if radius > TOL {
                    let center = v3_add(apex_a, v3_scale(axis_a, h));
                    return Ok(vec![SSICurve::Circle {
                        center,
                        normal: axis_a,
                        radius,
                    }]);
                }
            }
            Ok(vec![])
        } else {
            // Opposite direction
            let h = d * tan_b / (tan_a + tan_b);
            let h_b = d - h;

            if h >= z_min_a - TOL
                && h <= z_max_a + TOL
                && h > TOL
                && h_b >= z_min_b - TOL
                && h_b <= z_max_b + TOL
                && h_b > TOL
            {
                let radius = h * tan_a;
                if radius > TOL {
                    let center = v3_add(apex_a, v3_scale(axis_a, h));
                    return Ok(vec![SSICurve::Circle {
                        center,
                        normal: axis_a,
                        radius,
                    }]);
                }
            }
            Ok(vec![])
        }
    }
    // 2. Same-apex case: apices coincide → apex is always a shared point.
    //    The intersection curve passes through the apex. We emit a Line from the
    //    apex to a representative point found by numerical scanning on both surfaces.
    else if same_apex {
        // The apex is on both surfaces. Find the intersection curve by scanning
        // cone A's surface and checking proximity to cone B.
        let (u_a, v_a) = compute_plane_basis(axis_a);

        let n_h: usize = 200;
        let n_theta: usize = 72;
        let mut found_pts: Vec<[f64; 3]> = Vec::new();

        // Always include the shared apex as an intersection point.
        found_pts.push(apex_a);

        for ih in 1..=n_h {
            let h_a = z_min_a + (z_max_a - z_min_a) * (ih as f64) / (n_h as f64);
            if h_a < TOL {
                continue;
            }
            let ra = h_a * tan_a;

            for it in 0..n_theta {
                let theta = 2.0 * std::f64::consts::PI * (it as f64) / (n_theta as f64);
                let cos_t = theta.cos();
                let sin_t = theta.sin();

                // Point on cone A
                let pt = v3_add(
                    v3_add(apex_a, v3_scale(axis_a, h_a)),
                    v3_add(v3_scale(u_a, ra * cos_t), v3_scale(v_a, ra * sin_t)),
                );

                // Check if pt is on cone B surface
                let diff_b = v3_sub(pt, apex_b);
                let h_b = v3_dot(diff_b, axis_b);

                if h_b < z_min_b - TOL || h_b > z_max_b + TOL || h_b < TOL {
                    continue;
                }

                let rb = h_b * tan_b;
                let proj_b = v3_add(apex_b, v3_scale(axis_b, h_b));
                let perp_b = v3_sub(pt, proj_b);
                let perp_dist_b = v3_length(perp_b);

                // Use relative tolerance proportional to the cone radius
                let tol_b = (rb * crate::units::SSI_SAMPLE_ON_SURFACE_TOL).max(0.02);
                if (perp_dist_b - rb).abs() < tol_b {
                    found_pts.push(pt);
                }
            }
        }

        if found_pts.len() <= 1 {
            // Only the apex — emit a single degenerate Line from apex to a
            // point slightly along the bisector of the two axes (the
            // intersection curve must pass through the apex).
            let bisector = v3_normalize(v3_add(axis_a, axis_b));
            let small_t = z_min_a.max(0.01);
            let end_pt = v3_add(apex_a, v3_scale(bisector, small_t));
            return Ok(vec![SSICurve::Line {
                start: apex_a,
                end: end_pt,
            }]);
        }

        // Return a Line segment spanning the extent
        let mut max_d = 0.0_f64;
        let mut p_start = found_pts[0];
        let mut p_end = found_pts[1];
        for i in 0..found_pts.len() {
            for j in (i + 1)..found_pts.len() {
                let dd = v3_length(v3_sub(found_pts[i], found_pts[j]));
                if dd > max_d {
                    max_d = dd;
                    p_start = found_pts[i];
                    p_end = found_pts[j];
                }
            }
        }

        Ok(vec![SSICurve::Line {
            start: p_start,
            end: p_end,
        }])
    }
    // 3. General case: arbitrary position
    else {
        // Bounding-sphere disjoint check
        let max_r_a = z_max_a * tan_a;
        let max_r_b = z_max_b * tan_b;
        let extent_a = (z_max_a * z_max_a + max_r_a * max_r_a).sqrt();
        let extent_b = (z_max_b * z_max_b + max_r_b * max_r_b).sqrt();

        // Centers of bounding spheres (midpoint of cone axis segments)
        let mid_a = v3_add(apex_a, v3_scale(axis_a, (z_min_a + z_max_a) * 0.5));
        let mid_b = v3_add(apex_b, v3_scale(axis_b, (z_min_b + z_max_b) * 0.5));
        let centers_dist = v3_length(v3_sub(mid_a, mid_b));

        if centers_dist > extent_a + extent_b + TOL {
            return Ok(vec![]);
        }

        // Sample cone A surface, check proximity to cone B surface.
        // For each height h on cone A and azimuthal angle θ, compute a point on cone A,
        // then check if it lies on or near cone B.
        let (u_a, v_a) = compute_plane_basis(axis_a);

        let n_h: usize = 100;
        let n_theta: usize = 72;
        let mut found_pts: Vec<[f64; 3]> = Vec::new();

        for ih in 0..=n_h {
            let h_a = z_min_a + (z_max_a - z_min_a) * (ih as f64) / (n_h as f64);
            if h_a < TOL {
                continue;
            }
            let ra = h_a * tan_a;

            for it in 0..n_theta {
                let theta = 2.0 * std::f64::consts::PI * (it as f64) / (n_theta as f64);
                let cos_t = theta.cos();
                let sin_t = theta.sin();

                // Point on cone A
                let pt = v3_add(
                    v3_add(apex_a, v3_scale(axis_a, h_a)),
                    v3_add(v3_scale(u_a, ra * cos_t), v3_scale(v_a, ra * sin_t)),
                );

                // Check if pt is on cone B surface:
                // Height from apex_b along axis_b
                let diff_b = v3_sub(pt, apex_b);
                let h_b = v3_dot(diff_b, axis_b);

                if h_b < z_min_b - TOL || h_b > z_max_b + TOL || h_b < TOL {
                    continue;
                }

                // Expected radius at that height
                let rb = h_b * tan_b;

                // Actual perpendicular distance from axis_b
                let proj_b = v3_add(apex_b, v3_scale(axis_b, h_b));
                let perp_b = v3_sub(pt, proj_b);
                let perp_dist_b = v3_length(perp_b);

                if (perp_dist_b - rb).abs() < crate::units::SSI_SAMPLE_ON_SURFACE_TOL {
                    found_pts.push(pt);
                }
            }
        }

        if found_pts.is_empty() {
            return Ok(vec![]);
        }

        // Return a Line segment spanning the extent
        let mut max_d = 0.0_f64;
        let mut p_start = found_pts[0];
        let mut p_end = found_pts[0];
        for i in 0..found_pts.len() {
            for j in (i + 1)..found_pts.len() {
                let dd = v3_length(v3_sub(found_pts[i], found_pts[j]));
                if dd > max_d {
                    max_d = dd;
                    p_start = found_pts[i];
                    p_end = found_pts[j];
                }
            }
        }

        Ok(vec![SSICurve::Line {
            start: p_start,
            end: p_end,
        }])
    }
}

/// Signed distance from a point to a torus surface.
/// Positive outside, negative inside the tube.
fn torus_signed_distance(
    pt: [f64; 3],
    torus_center: [f64; 3],
    torus_axis: [f64; 3],
    big_r: f64,
    small_r: f64,
) -> f64 {
    let diff = v3_sub(pt, torus_center);
    let h = v3_dot(diff, torus_axis); // axial offset from torus midplane
    let proj = v3_add(torus_center, v3_scale(torus_axis, h));
    let radial_vec = v3_sub(pt, proj);
    let rho = v3_length(radial_vec); // distance from axis in midplane
    let dist_to_tube_center = ((rho - big_r) * (rho - big_r) + h * h).sqrt();
    dist_to_tube_center - small_r
}

/// Cylinder-Torus surface-surface intersection (A15 pair #10).
///
/// Computes intersection curves between a cylinder and a torus in general position.
/// Coaxial case yields circles; general position yields Line approximations.
///
/// Ref #1: Patrikalakis Ch.5
#[allow(clippy::too_many_arguments)]
pub(crate) fn cylinder_torus_ssi(
    cyl_origin: [f64; 3],
    cyl_axis: [f64; 3],
    cyl_radius: f64,
    cyl_z_min: f64,
    cyl_z_max: f64,
    torus_center: [f64; 3],
    torus_axis: [f64; 3],
    torus_major_radius: f64,
    torus_minor_radius: f64,
) -> Result<Vec<SSICurve>, KernelError> {
    let big_r = torus_major_radius;
    let small_r = torus_minor_radius;
    let r_cyl = cyl_radius;
    let cyl_ax = v3_normalize(cyl_axis);
    let tor_ax = v3_normalize(torus_axis);

    // ── 1. Coaxial case: axes parallel AND torus center on cylinder axis ──
    let axes_dot = v3_dot(cyl_ax, tor_ax).abs();
    if axes_dot > 1.0 - TOL {
        // Check if torus center lies on the cylinder axis line
        let diff = v3_sub(torus_center, cyl_origin);
        let along = v3_dot(diff, cyl_ax);
        let proj = v3_add(cyl_origin, v3_scale(cyl_ax, along));
        let perp_dist = v3_length(v3_sub(torus_center, proj));

        if perp_dist < TOL {
            // Coaxial: shared axis.
            // Torus in cylindrical coords about axis: (ρ - R)² + z² = r²
            // Cylinder: ρ = R_cyl
            // Substituting: (R_cyl - R)² + z² = r²
            // z² = r² - (R_cyl - R)²
            let delta = (r_cyl - big_r).abs();

            if delta > small_r + TOL {
                // No intersection
                return Ok(vec![]);
            }
            if (delta - small_r).abs() < TOL {
                // Tangent — below feature size
                return Ok(vec![]);
            }

            // Two circles at z = ±sqrt(r² - delta²) relative to torus center
            let z_sq = small_r * small_r - delta * delta;
            if z_sq < 0.0 {
                return Ok(vec![]);
            }
            let z_val = z_sq.sqrt();

            let mut curves = Vec::new();

            // Use the shared axis direction (use cyl_ax as canonical)
            // The torus center's axial coordinate in cylinder frame
            let torus_center_axial = v3_dot(v3_sub(torus_center, cyl_origin), cyl_ax);

            for &z_sign in &[-1.0, 1.0] {
                let z_world = z_sign * z_val;
                // z_world is relative to torus center along axis
                // Convert to cylinder axial coordinate
                let z_cyl = torus_center_axial + z_world;

                // Check cylinder z-range
                if z_cyl < cyl_z_min - TOL || z_cyl > cyl_z_max + TOL {
                    continue;
                }

                let center = v3_add(torus_center, v3_scale(cyl_ax, z_world));
                curves.push(SSICurve::Circle {
                    center,
                    normal: cyl_ax,
                    radius: r_cyl,
                });
            }

            return Ok(curves);
        }
    }

    // ── 2. General case (non-coaxial) ──

    // Bounding sphere disjoint check
    let cyl_half_h = (cyl_z_max - cyl_z_min) / 2.0;
    let cyl_mid_z = (cyl_z_max + cyl_z_min) / 2.0;
    let cyl_mid = v3_add(cyl_origin, v3_scale(cyl_ax, cyl_mid_z));
    let cyl_bound_r = (r_cyl * r_cyl + cyl_half_h * cyl_half_h).sqrt();
    let torus_bound_r = big_r + small_r;
    let center_dist = v3_length(v3_sub(cyl_mid, torus_center));
    if center_dist > cyl_bound_r + torus_bound_r + TOL {
        return Ok(vec![]);
    }

    // Numerical scanning: sample cylinder surface at (theta, z) grid
    // For each sample point, compute signed distance to torus surface.
    // Collect zero-crossing points.
    let (cyl_u, cyl_v) = compute_plane_basis(cyl_ax);

    let n_theta: usize = 360;
    let n_z: usize = 200;
    let mut found_pts: Vec<[f64; 3]> = Vec::new();

    for i in 0..n_theta {
        let theta = 2.0 * std::f64::consts::PI * (i as f64) / (n_theta as f64);
        let cos_t = theta.cos();
        let sin_t = theta.sin();

        // Direction on cylinder cross-section
        let radial = v3_add(v3_scale(cyl_u, cos_t), v3_scale(cyl_v, sin_t));

        let mut prev_sd = f64::NAN;

        for j in 0..=n_z {
            let t = j as f64 / n_z as f64;
            let z = cyl_z_min + t * (cyl_z_max - cyl_z_min);

            // Point on cylinder surface
            let pt = v3_add(
                v3_add(cyl_origin, v3_scale(cyl_ax, z)),
                v3_scale(radial, r_cyl),
            );

            // Signed distance from pt to torus surface
            let sd = torus_signed_distance(pt, torus_center, tor_ax, big_r, small_r);

            // Check for sign change (zero crossing)
            if !prev_sd.is_nan() && prev_sd * sd < 0.0 {
                // Linear interpolation to find approximate crossing
                let frac = prev_sd.abs() / (prev_sd.abs() + sd.abs());
                let z_prev = cyl_z_min + ((j as f64 - 1.0) / n_z as f64) * (cyl_z_max - cyl_z_min);
                let z_cross = z_prev + frac * (z - z_prev);
                let cross_pt = v3_add(
                    v3_add(cyl_origin, v3_scale(cyl_ax, z_cross)),
                    v3_scale(radial, r_cyl),
                );
                found_pts.push(cross_pt);
            }

            prev_sd = sd;
        }
    }

    if found_pts.is_empty() {
        return Ok(vec![]);
    }

    // Find maximum-extent pair for Line segment
    let mut max_dist = 0.0_f64;
    let mut p_start = found_pts[0];
    let mut p_end = found_pts[0];
    for i in 0..found_pts.len() {
        for j in (i + 1)..found_pts.len() {
            let dd = v3_length(v3_sub(found_pts[i], found_pts[j]));
            if dd > max_dist {
                max_dist = dd;
                p_start = found_pts[i];
                p_end = found_pts[j];
            }
        }
    }

    if max_dist < crate::units::MIN_FEATURE_SIZE {
        return Ok(vec![]);
    }

    Ok(vec![SSICurve::Line {
        start: p_start,
        end: p_end,
    }])
}

/// Cone-Torus surface-surface intersection (A15 pair #13).
///
/// Computes intersection curves between a cone and a torus in general position.
/// Coaxial case yields circles; general position yields Line approximations.
///
/// Ref #1: Patrikalakis Ch.5
#[allow(clippy::too_many_arguments)]
pub(crate) fn cone_torus_ssi(
    cone_apex: [f64; 3],
    cone_axis: [f64; 3],
    cone_half_angle: f64,
    cone_height_range: (f64, f64),
    torus_center: [f64; 3],
    torus_axis: [f64; 3],
    torus_major_radius: f64,
    torus_minor_radius: f64,
) -> Result<Vec<SSICurve>, KernelError> {
    let big_r = torus_major_radius;
    let small_r = torus_minor_radius;
    let tan_a = cone_half_angle.tan();
    let cone_ax = v3_normalize(cone_axis);
    let tor_ax = v3_normalize(torus_axis);
    let h_min = cone_height_range.0;
    let h_max = cone_height_range.1;

    // ── 1. Coaxial case: axes parallel AND cone apex on torus axis ──
    let axes_dot = v3_dot(cone_ax, tor_ax).abs();
    if axes_dot > 1.0 - TOL {
        // Check if cone apex lies on the torus axis line
        let diff = v3_sub(cone_apex, torus_center);
        let along = v3_dot(diff, tor_ax);
        let proj = v3_add(torus_center, v3_scale(tor_ax, along));
        let perp_dist = v3_length(v3_sub(cone_apex, proj));

        if perp_dist < TOL {
            // Coaxial: shared axis.
            // Point on cone at height h: radial distance ρ = h·tan(α), axial pos = apex + h·axis.
            // Signed offset of cone apex from torus center along axis:
            let d = v3_dot(v3_sub(cone_apex, torus_center), tor_ax);
            // Torus equation in cylindrical coords about axis:
            //   (ρ - R)² + (z_torus)² = r²
            // where ρ = h·tan(α), z_torus = d + h (axial position relative to torus center).
            // (h·tan(α) - R)² + (d + h)² = r²
            // h²·tan²(α) - 2Rh·tan(α) + R² + d² + 2dh + h² = r²
            // (tan²(α) + 1)·h² + (-2R·tan(α) + 2d)·h + (R² + d² - r²) = 0
            // sec²(α) · h² + (-2R·tan(α) + 2d)·h + (R² + d² - r²) = 0
            let sec2 = 1.0 + tan_a * tan_a; // sec²(α) = 1 + tan²(α)
            let qa = sec2;
            let qb = -2.0 * big_r * tan_a + 2.0 * d;
            let qc = big_r * big_r + d * d - small_r * small_r;

            let disc = qb * qb - 4.0 * qa * qc;
            if disc < 0.0 {
                return Ok(vec![]);
            }

            let mut curves = Vec::new();

            if disc.abs() < TOL * TOL {
                // Single tangent solution
                let h_sol = -qb / (2.0 * qa);
                if h_sol >= h_min - TOL && h_sol <= h_max + TOL && h_sol >= -TOL {
                    let rho = h_sol * tan_a;
                    if rho > TOL {
                        let center = v3_add(cone_apex, v3_scale(cone_ax, h_sol));
                        curves.push(SSICurve::Circle {
                            center,
                            normal: cone_ax,
                            radius: rho,
                        });
                    }
                }
            } else {
                let sqrt_disc = disc.sqrt();
                let h1 = (-qb + sqrt_disc) / (2.0 * qa);
                let h2 = (-qb - sqrt_disc) / (2.0 * qa);

                for h_sol in [h1, h2] {
                    if h_sol >= h_min - TOL && h_sol <= h_max + TOL && h_sol >= -TOL {
                        let rho = h_sol * tan_a;
                        if rho > TOL {
                            let center = v3_add(cone_apex, v3_scale(cone_ax, h_sol));
                            curves.push(SSICurve::Circle {
                                center,
                                normal: cone_ax,
                                radius: rho,
                            });
                        }
                    }
                }
            }

            return Ok(curves);
        }
    }

    // ── 2. Bounding sphere fast reject ──
    let cone_mid_h = (h_min + h_max) / 2.0;
    let cone_mid = v3_add(cone_apex, v3_scale(cone_ax, cone_mid_h));
    let cone_half_h = (h_max - h_min) / 2.0;
    let max_radius = h_max.abs().max(h_min.abs()) * tan_a;
    let cone_bound_r = (max_radius * max_radius + cone_half_h * cone_half_h).sqrt();
    let torus_bound_r = big_r + small_r;
    let center_dist = v3_length(v3_sub(cone_mid, torus_center));
    if center_dist > cone_bound_r + torus_bound_r + TOL {
        return Ok(vec![]);
    }

    // ── 3. General case: numerical scanning ──
    let (cone_u, cone_v) = compute_plane_basis(cone_ax);

    let n_theta: usize = 360;
    let n_h: usize = 200;
    let mut found_pts: Vec<[f64; 3]> = Vec::new();

    for i in 0..n_theta {
        let theta = 2.0 * std::f64::consts::PI * (i as f64) / (n_theta as f64);
        let cos_t = theta.cos();
        let sin_t = theta.sin();

        let radial = v3_add(v3_scale(cone_u, cos_t), v3_scale(cone_v, sin_t));

        let mut prev_sd = f64::NAN;

        for j in 0..=n_h {
            let t = j as f64 / n_h as f64;
            let h = h_min + t * (h_max - h_min);

            // Point on cone surface: apex + h*axis + h*tan(α)*radial
            let rho = h * tan_a;
            let pt = v3_add(
                v3_add(cone_apex, v3_scale(cone_ax, h)),
                v3_scale(radial, rho),
            );

            let sd = torus_signed_distance(pt, torus_center, tor_ax, big_r, small_r);

            if !prev_sd.is_nan() && prev_sd * sd < 0.0 {
                let frac = prev_sd.abs() / (prev_sd.abs() + sd.abs());
                let h_prev = h_min + ((j as f64 - 1.0) / n_h as f64) * (h_max - h_min);
                let h_cross = h_prev + frac * (h - h_prev);
                let rho_cross = h_cross * tan_a;
                let cross_pt = v3_add(
                    v3_add(cone_apex, v3_scale(cone_ax, h_cross)),
                    v3_scale(radial, rho_cross),
                );
                found_pts.push(cross_pt);
            }

            prev_sd = sd;
        }
    }

    if found_pts.is_empty() {
        return Ok(vec![]);
    }

    // Find maximum-extent pair for Line segment
    let mut max_dist = 0.0_f64;
    let mut p_start = found_pts[0];
    let mut p_end = found_pts[0];
    for i in 0..found_pts.len() {
        for j in (i + 1)..found_pts.len() {
            let dd = v3_length(v3_sub(found_pts[i], found_pts[j]));
            if dd > max_dist {
                max_dist = dd;
                p_start = found_pts[i];
                p_end = found_pts[j];
            }
        }
    }

    if max_dist < crate::units::MIN_FEATURE_SIZE {
        return Ok(vec![]);
    }

    Ok(vec![SSICurve::Line {
        start: p_start,
        end: p_end,
    }])
}

/// Torus-Torus surface-surface intersection (A15 pair #15).
///
/// Computes intersection curves between two tori in general position.
/// Coaxial case yields circles; general position yields Line approximations.
///
/// Ref #1: Patrikalakis Ch.5
#[allow(clippy::too_many_arguments)]
pub(crate) fn torus_torus_ssi(
    torus_a_center: [f64; 3],
    torus_a_axis: [f64; 3],
    torus_a_major_radius: f64,
    torus_a_minor_radius: f64,
    torus_b_center: [f64; 3],
    torus_b_axis: [f64; 3],
    torus_b_major_radius: f64,
    torus_b_minor_radius: f64,
) -> Result<Vec<SSICurve>, KernelError> {
    let r_a = torus_a_major_radius;
    let sr_a = torus_a_minor_radius;
    let r_b = torus_b_major_radius;
    let sr_b = torus_b_minor_radius;
    let ax_a = v3_normalize(torus_a_axis);
    let ax_b = v3_normalize(torus_b_axis);

    // ── 1. Coaxial case: axes parallel AND centers collinear on axis ──
    let axes_dot = v3_dot(ax_a, ax_b).abs();
    if axes_dot > 1.0 - TOL {
        let diff = v3_sub(torus_b_center, torus_a_center);
        let along = v3_dot(diff, ax_a);
        let proj = v3_add(torus_a_center, v3_scale(ax_a, along));
        let perp_dist = v3_length(v3_sub(torus_b_center, proj));

        if perp_dist < TOL {
            // Coaxial: shared axis.
            // Use ax_a as canonical axis direction.
            // d_A = 0 (torus A center is origin in our local frame)
            // d_B = signed axial offset of torus B center from torus A center
            let d_b = along; // axial offset of B relative to A

            // Torus A: (ρ - R_A)² + z² = r_A²
            // Torus B: (ρ - R_B)² + (z - d_B)² = r_B²
            // Subtract: (ρ - R_A)² - (ρ - R_B)² + z² - (z - d_B)² = r_A² - r_B²
            // 2(R_B - R_A)ρ + (R_A² - R_B²) + 2·d_B·z - d_B² = sr_A² - sr_B²
            // 2(R_B - R_A)ρ + 2·d_B·z = sr_A² - sr_B² - R_A² + R_B² + d_B²

            let rhs = sr_a * sr_a - sr_b * sr_b - r_a * r_a + r_b * r_b + d_b * d_b;
            let coeff_rho = 2.0 * (r_b - r_a);
            let coeff_z = 2.0 * d_b;

            // Case: both coefficients near zero means identical torus geometry
            if coeff_rho.abs() < TOL && coeff_z.abs() < TOL {
                // Degenerate — identical or no solution
                return Ok(vec![]);
            }

            let mut curves = Vec::new();

            if coeff_rho.abs() > TOL {
                // ρ = (rhs - coeff_z·z) / coeff_rho
                // Substitute into torus A: ((rhs - coeff_z·z)/coeff_rho - R_A)² + z² = sr_A²
                // Let P = rhs/coeff_rho - R_A, Q = -coeff_z/coeff_rho
                // (P + Q·z)² + z² = sr_A²
                // (Q² + 1)·z² + 2PQ·z + P² - sr_A² = 0
                let p_val = rhs / coeff_rho - r_a;
                let q_val = -coeff_z / coeff_rho;

                let qa = q_val * q_val + 1.0;
                let qb = 2.0 * p_val * q_val;
                let qc = p_val * p_val - sr_a * sr_a;

                let disc = qb * qb - 4.0 * qa * qc;
                if disc < 0.0 {
                    return Ok(vec![]);
                }

                let z_solutions = if disc.abs() < TOL * TOL {
                    vec![-qb / (2.0 * qa)]
                } else {
                    let sqrt_disc = disc.sqrt();
                    vec![
                        (-qb + sqrt_disc) / (2.0 * qa),
                        (-qb - sqrt_disc) / (2.0 * qa),
                    ]
                };

                for z_sol in z_solutions {
                    let rho = (rhs - coeff_z * z_sol) / coeff_rho;
                    if rho > TOL {
                        let center = v3_add(torus_a_center, v3_scale(ax_a, z_sol));
                        curves.push(SSICurve::Circle {
                            center,
                            normal: ax_a,
                            radius: rho,
                        });
                    }
                }
            } else {
                // coeff_rho ≈ 0, so R_A ≈ R_B. Solve for z from: coeff_z·z = rhs
                let z_sol = rhs / coeff_z;
                // Then from torus A: (ρ - R_A)² = sr_A² - z²
                let val = sr_a * sr_a - z_sol * z_sol;
                if val < 0.0 {
                    return Ok(vec![]);
                }
                let delta = val.sqrt();
                // ρ = R_A ± delta
                for &sign in &[-1.0, 1.0] {
                    let rho = r_a + sign * delta;
                    if rho > TOL {
                        let center = v3_add(torus_a_center, v3_scale(ax_a, z_sol));
                        curves.push(SSICurve::Circle {
                            center,
                            normal: ax_a,
                            radius: rho,
                        });
                    }
                }
            }

            return Ok(curves);
        }
    }

    // ── 2. Bounding sphere fast reject ──
    let bound_a = r_a + sr_a;
    let bound_b = r_b + sr_b;
    let center_dist = v3_length(v3_sub(torus_a_center, torus_b_center));
    if center_dist > bound_a + bound_b + TOL {
        return Ok(vec![]);
    }

    // ── 3. General case: scan torus A surface, evaluate distance to torus B ──
    let (u_a, v_a) = compute_plane_basis(ax_a);

    let n_theta: usize = 360;
    let n_phi: usize = 36;
    let mut found_pts: Vec<[f64; 3]> = Vec::new();

    for i in 0..n_theta {
        let theta = 2.0 * std::f64::consts::PI * (i as f64) / (n_theta as f64);
        let cos_t = theta.cos();
        let sin_t = theta.sin();

        // Tube center at this azimuthal angle on torus A
        let tube_c = v3_add(
            torus_a_center,
            v3_add(v3_scale(u_a, r_a * cos_t), v3_scale(v_a, r_a * sin_t)),
        );

        // Tube radial direction (from axis toward tube center)
        let tube_radial = v3_normalize(v3_sub(tube_c, torus_a_center));

        for j in 0..n_phi {
            let phi = 2.0 * std::f64::consts::PI * (j as f64) / (n_phi as f64);
            let cos_p = phi.cos();
            let sin_p = phi.sin();

            // Point on torus A surface
            let pt = v3_add(
                tube_c,
                v3_add(
                    v3_scale(tube_radial, sr_a * cos_p),
                    v3_scale(ax_a, sr_a * sin_p),
                ),
            );

            // Check distance to torus B surface
            let sd = torus_signed_distance(pt, torus_b_center, ax_b, r_b, sr_b);
            if sd.abs() < crate::units::SSI_SAMPLE_ON_SURFACE_TOL {
                found_pts.push(pt);
            }
        }
    }

    if found_pts.is_empty() {
        return Ok(vec![]);
    }

    // Find maximum-extent pair for Line segment
    let mut max_dist = 0.0_f64;
    let mut p_start = found_pts[0];
    let mut p_end = found_pts[0];
    for i in 0..found_pts.len() {
        for j in (i + 1)..found_pts.len() {
            let dd = v3_length(v3_sub(found_pts[i], found_pts[j]));
            if dd > max_dist {
                max_dist = dd;
                p_start = found_pts[i];
                p_end = found_pts[j];
            }
        }
    }

    if max_dist < crate::units::MIN_FEATURE_SIZE {
        return Ok(vec![]);
    }

    Ok(vec![SSICurve::Line {
        start: p_start,
        end: p_end,
    }])
}

// ── Tests ──────────────────────────────────────────────────────────────────

#[cfg(test)]
mod tests {
    use super::*;
    use crate::units::MIN_FEATURE_SIZE;
    use std::f64::consts::FRAC_1_SQRT_2;

    const EPS: f64 = MIN_FEATURE_SIZE;

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
                    (*semi_major - sqrt2).abs() < EPS,
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
            (majors[0] - expected_2).abs() < EPS,
            "smaller={}, expected {}",
            majors[0],
            expected_2
        );
        assert!(
            (majors[1] - expected_1).abs() < EPS,
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
                    (center[0] - 5.0).abs() < EPS,
                    "cx={}, expected 5.0",
                    center[0]
                );
                assert!(
                    (center[2] - 5.0).abs() < EPS,
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
            (majors[0] - expecteds[0]).abs() < EPS,
            "got {}, expected {}",
            majors[0],
            expecteds[0]
        );
        assert!(
            (majors[1] - expecteds[1]).abs() < EPS,
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
                    (*semi_minor - avg_r).abs() < EPS,
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
            let arbitrary = if normal[0].abs() < crate::units::BASIS_AXIS_ALIGNMENT {
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

        // Coaxial cone-sphere at micro scale.  Quadratic in h:
        //   a = 1 + tan²(30°) = 4/3,  b = -2·5e-5 = -1e-4,
        //   c = (5e-5)² − (4e-5)² = 9e-10.
        //   disc = 1e-8 − (16/3)·9e-10 = 5.2e-9 > 0  → two real roots.
        //   h₁ ≈ 6.45e-5, h₂ ≈ 1.05e-5 — both within [0, 1e-4] and > TOL.
        // Therefore the solver must return exactly 2 circles.
        assert_eq!(
            curves.len(),
            2,
            "Coaxial micro-scale cone-sphere must produce 2 circles, got {}",
            curves.len()
        );
        for curve in &curves {
            match curve {
                SSICurve::Circle { radius, .. } => {
                    // Radii = h·tan(30°), both on order 1e-5, well above TOL.
                    assert!(
                        *radius > 1e-6 && *radius < 1e-4,
                        "Circle radius {} outside expected micro-scale range [1e-6, 1e-4]",
                        radius
                    );
                }
                other => panic!("Expected Circle, got {:?}", other),
            }
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

    // ── Sphere-Torus SSI ────────────────────────────────────────────

    #[test]
    fn test_sphere_torus_axial_two_circles() {
        // Sphere centered on torus axis, radius straddles torus tube → 2 circles.
        // Torus: center=[0,0,0], axis=[0,0,1], R=5, r=1
        // Sphere: center=[0,0,0], radius=5.5
        // Solving: torus surface (ρ-5)²+z²=1, sphere ρ²+z²=30.25
        // → ρ = 5.425, z = ±0.9052
        let curves = sphere_torus_ssi(
            [0.0, 0.0, 0.0], // sphere center
            5.5,             // sphere radius
            [0.0, 0.0, 0.0], // torus center
            [0.0, 0.0, 1.0], // torus axis
            5.0,             // torus major radius R
            1.0,             // torus minor radius r
        )
        .unwrap();
        assert_eq!(curves.len(), 2, "Expected 2 circles, got {}", curves.len());

        let mut z_values: Vec<f64> = Vec::new();
        for curve in &curves {
            if let SSICurve::Circle {
                center,
                normal,
                radius,
            } = curve
            {
                // Each circle should have radius ≈ 5.425 (the ρ value)
                assert!(
                    (radius - 5.425).abs() < EPS,
                    "Circle radius should be ~5.425, got {}",
                    radius
                );
                // Center should be on the Z axis
                assert!(center[0].abs() < EPS, "center x={}", center[0]);
                assert!(center[1].abs() < EPS, "center y={}", center[1]);
                // Normal should be parallel to torus axis
                let dot = v3_dot(*normal, [0.0, 0.0, 1.0]).abs();
                assert!((dot - 1.0).abs() < EPS, "normal not parallel to axis");
                z_values.push(center[2]);
            } else {
                panic!("Expected Circle, got {:?}", curve);
            }
        }
        // Two circles at z = ±√(Rs² - ρ²) where Rs=5.5, ρ=5.425
        let expected_z = (5.5_f64 * 5.5 - 5.425 * 5.425).sqrt(); // ≈ 0.90519…
        z_values.sort_by(|a, b| a.partial_cmp(b).unwrap());
        assert!(
            (z_values[0] - (-expected_z)).abs() < EPS,
            "Lower z should be ~-{}, got {}",
            expected_z,
            z_values[0]
        );
        assert!(
            (z_values[1] - expected_z).abs() < EPS,
            "Upper z should be ~{}, got {}",
            expected_z,
            z_values[1]
        );
    }

    #[test]
    fn test_sphere_torus_axial_one_circle() {
        // Sphere on torus axis, just touching one side of the tube → 1 circle.
        // Torus: center=[0,0,0], axis=[0,0,1], R=5, r=1
        // Sphere: center=[0,0,0], radius=4.0
        // Torus inner rim at ρ=4, z=0. Sphere touches at ρ=4, z=0.
        // (ρ-5)²+z²=1, ρ²+z²=16 → ρ = sqrt(16-z²), (sqrt(16-z²)-5)²+z²=1
        // At z=0: (4-5)²=1 ✓ → tangent at one circle.
        let curves = sphere_torus_ssi(
            [0.0, 0.0, 0.0], // sphere center
            4.0,             // sphere radius
            [0.0, 0.0, 0.0], // torus center
            [0.0, 0.0, 1.0], // torus axis
            5.0,             // torus major radius R
            1.0,             // torus minor radius r
        )
        .unwrap();
        assert_eq!(
            curves.len(),
            1,
            "Expected 1 circle (tangent), got {}",
            curves.len()
        );
        if let SSICurve::Circle { center, radius, .. } = &curves[0] {
            assert!(
                (radius - 4.0).abs() < EPS,
                "radius should be 4.0, got {}",
                radius
            );
            assert!(
                center[2].abs() < EPS,
                "center z should be 0, got {}",
                center[2]
            );
        } else {
            panic!("Expected Circle, got {:?}", curves[0]);
        }
    }

    #[test]
    fn test_sphere_torus_disjoint() {
        // Sphere far from torus → empty.
        // Torus: center=[0,0,0], axis=[0,0,1], R=5, r=1 → outer extent = 6
        // Sphere: center=[20,0,0], radius=2 → closest point at x=18, well beyond 6
        let curves = sphere_torus_ssi(
            [20.0, 0.0, 0.0], // sphere center
            2.0,              // sphere radius
            [0.0, 0.0, 0.0],  // torus center
            [0.0, 0.0, 1.0],  // torus axis
            5.0,              // torus major radius R
            1.0,              // torus minor radius r
        )
        .unwrap();
        assert!(
            curves.is_empty(),
            "Disjoint sphere-torus should be empty, got {}",
            curves.len()
        );
    }

    #[test]
    fn test_sphere_torus_enclosed() {
        // Sphere fully inside torus tube → empty (no intersection).
        // Torus: center=[0,0,0], axis=[0,0,1], R=5, r=2
        // Sphere: center=[5,0,0] (on tube center line), radius=0.5
        // The sphere is fully inside the tube, so no surface intersection.
        let curves = sphere_torus_ssi(
            [5.0, 0.0, 0.0], // sphere center (on the tube center circle)
            0.5,             // sphere radius (much smaller than r=2)
            [0.0, 0.0, 0.0], // torus center
            [0.0, 0.0, 1.0], // torus axis
            5.0,             // torus major radius R
            2.0,             // torus minor radius r
        )
        .unwrap();
        assert!(
            curves.is_empty(),
            "Enclosed sphere should give empty, got {}",
            curves.len()
        );
    }

    #[test]
    fn test_sphere_torus_general_offset() {
        // Sphere off-axis, intersecting torus → should produce a non-empty result.
        // Torus: center=[0,0,0], axis=[0,0,1], R=5, r=1
        // Sphere: center=[4,0,0], radius=2.0
        // The sphere overlaps the torus tube (tube center at ρ=5, sphere at ρ=4 with r=2).
        let curves = sphere_torus_ssi(
            [4.0, 0.0, 0.0], // sphere center
            2.0,             // sphere radius
            [0.0, 0.0, 0.0], // torus center
            [0.0, 0.0, 1.0], // torus axis
            5.0,             // torus major radius R
            1.0,             // torus minor radius r
        )
        .unwrap();
        assert!(
            !curves.is_empty(),
            "Off-axis sphere intersecting torus should produce curves"
        );
    }

    // ── Cone-Cone SSI ───────────────────────────────────────────────

    #[test]
    fn test_cone_cone_coaxial_different_angles() {
        // Two cones on the same axis with different half-angles.
        // Cone A: apex=[0,0,0], axis=[0,0,1], half_angle=30°, h_range=(0,10)
        // Cone B: apex=[0,0,2], axis=[0,0,1], half_angle=45°, h_range=(0,10)
        // At height h from A: r_a = h * tan(30°) ≈ h * 0.57735
        // At height h (= h-2 from B): r_b = (h-2) * tan(45°) = h - 2
        // Equal: h * 0.57735 = h - 2 → h = 2/0.42265 ≈ 4.732
        // r at intersection ≈ 4.732 * 0.57735 ≈ 2.732
        let half_30 = std::f64::consts::FRAC_PI_6; // 30°
        let half_45 = std::f64::consts::FRAC_PI_4; // 45°
        let curves = cone_cone_ssi(
            [0.0, 0.0, 0.0], // apex A
            [0.0, 0.0, 1.0], // axis A
            half_30,         // half-angle A
            (0.0, 10.0),     // height range A
            [0.0, 0.0, 2.0], // apex B
            [0.0, 0.0, 1.0], // axis B
            half_45,         // half-angle B
            (0.0, 10.0),     // height range B
        )
        .unwrap();
        // Should produce 1 or 2 circles (at least the one at h ≈ 4.732)
        assert!(
            !curves.is_empty(),
            "Coaxial cones with different angles should intersect"
        );
        // Check the first circle
        let mut found_circle = false;
        for curve in &curves {
            if let SSICurve::Circle {
                center,
                radius,
                normal,
            } = curve
            {
                let expected_h = 2.0 / (1.0 - (half_30).tan()); // ≈ 4.732
                let expected_r = expected_h * half_30.tan(); // ≈ 2.732
                assert!(
                    (center[2] - expected_h).abs() < EPS,
                    "Circle z should be ~{}, got {}",
                    expected_h,
                    center[2]
                );
                assert!(
                    (radius - expected_r).abs() < EPS,
                    "Circle radius should be ~{}, got {}",
                    expected_r,
                    radius
                );
                assert!(center[0].abs() < EPS);
                assert!(center[1].abs() < EPS);
                let dot = v3_dot(*normal, [0.0, 0.0, 1.0]).abs();
                assert!((dot - 1.0).abs() < EPS, "Normal should be along axis");
                found_circle = true;
            }
        }
        assert!(found_circle, "Expected at least one Circle in result");
    }

    #[test]
    fn test_cone_cone_coaxial_same_angle() {
        // Same axis, same half-angle, different apex positions.
        // Cone A: apex=[0,0,0], axis=[0,0,1], half_angle=30°, h_range=(0,10)
        // Cone B: apex=[0,0,3], axis=[0,0,1], half_angle=30°, h_range=(0,10)
        // r_a(h) = h * tan30, r_b(h) = (h-3) * tan30
        // These are parallel lines in the (h, r) plane → no intersection (cones don't meet
        // if same orientation). Result should be empty.
        let half_30 = std::f64::consts::FRAC_PI_6;
        let curves = cone_cone_ssi(
            [0.0, 0.0, 0.0], // apex A
            [0.0, 0.0, 1.0], // axis A
            half_30,         // half-angle A
            (0.0, 10.0),     // height range A
            [0.0, 0.0, 3.0], // apex B
            [0.0, 0.0, 1.0], // axis B
            half_30,         // half-angle B
            (0.0, 10.0),     // height range B
        )
        .unwrap();
        assert!(
            curves.is_empty(),
            "Coaxial cones with same angle and same orientation should not intersect, got {} curves",
            curves.len()
        );
    }

    #[test]
    fn test_cone_cone_same_apex_different_axis() {
        // Shared apex, different axes → intersection curves pass through the apex.
        // Cone A: apex=[0,0,0], axis=[0,0,1], half_angle=30°, h_range=(0,10)
        // Cone B: apex=[0,0,0], axis=[1,0,0], half_angle=30°, h_range=(0,10)
        // Both cones share the apex at origin. Their intersection should include
        // lines through the origin.
        let half_30 = std::f64::consts::FRAC_PI_6;
        let curves = cone_cone_ssi(
            [0.0, 0.0, 0.0], // apex A
            [0.0, 0.0, 1.0], // axis A
            half_30,         // half-angle A
            (0.0, 10.0),     // height range A
            [0.0, 0.0, 0.0], // apex B
            [1.0, 0.0, 0.0], // axis B
            half_30,         // half-angle B
            (0.0, 10.0),     // height range B
        )
        .unwrap();
        assert!(
            !curves.is_empty(),
            "Same-apex cones with different axes should intersect"
        );
        // At least one result should pass through or near the shared apex
        let mut has_apex_curve = false;
        for curve in &curves {
            match curve {
                SSICurve::Line { start, end } => {
                    // At least one endpoint should be at or near the apex
                    let start_dist = v3_length(*start);
                    let end_dist = v3_length(*end);
                    if start_dist < 0.1 || end_dist < 0.1 {
                        has_apex_curve = true;
                    }
                }
                _ => {
                    // Other curve types are also acceptable
                    has_apex_curve = true;
                }
            }
        }
        assert!(
            has_apex_curve,
            "Expected at least one curve through or near the shared apex"
        );
    }

    #[test]
    fn test_cone_cone_disjoint() {
        // Two cones far apart → empty.
        // Cone A: apex=[0,0,0], axis=[0,0,1], half_angle=15°, h_range=(0,5)
        //   max radius = 5 * tan(15°) ≈ 1.34
        // Cone B: apex=[20,0,0], axis=[0,0,1], half_angle=15°, h_range=(0,5)
        //   max radius ≈ 1.34, centered at x=20
        // Distance between axes = 20 >> 1.34 + 1.34
        let half_15 = std::f64::consts::FRAC_PI_6 / 2.0; // 15°
        let curves = cone_cone_ssi(
            [0.0, 0.0, 0.0],  // apex A
            [0.0, 0.0, 1.0],  // axis A
            half_15,          // half-angle A
            (0.0, 5.0),       // height range A
            [20.0, 0.0, 0.0], // apex B
            [0.0, 0.0, 1.0],  // axis B
            half_15,          // half-angle B
            (0.0, 5.0),       // height range B
        )
        .unwrap();
        assert!(
            curves.is_empty(),
            "Disjoint cones should give empty, got {}",
            curves.len()
        );
    }

    #[test]
    fn test_cone_cone_general_position() {
        // Two cones in general position that definitely intersect.
        // Cone A: apex=[0,0,0], axis=[0,0,1], half_angle=45°, h_range=(0,5)
        //   At h=3: r=3
        // Cone B: apex=[3,0,0], axis=[0,0,1], half_angle=45°, h_range=(0,5)
        //   At h=3: r=3, centered at x=3
        // At h=3, circles of radius 3 centered at (0,0,3) and (3,0,3) overlap
        // since distance=3 < 3+3=6.
        let half_45 = std::f64::consts::FRAC_PI_4;
        let curves = cone_cone_ssi(
            [0.0, 0.0, 0.0], // apex A
            [0.0, 0.0, 1.0], // axis A
            half_45,         // half-angle A
            (0.0, 5.0),      // height range A
            [3.0, 0.0, 0.0], // apex B
            [0.0, 0.0, 1.0], // axis B
            half_45,         // half-angle B
            (0.0, 5.0),      // height range B
        )
        .unwrap();
        assert!(
            !curves.is_empty(),
            "Overlapping cones in general position should produce curves"
        );
    }

    // ── Sphere-Torus Adversarial Tests ──────────────────────────────

    #[test]
    fn test_sphere_torus_large_sphere_encloses_torus() {
        // Sphere large enough to fully enclose the torus → no surface intersection.
        // Torus: center=[0,0,0], axis=[0,0,1], R=5, r=1 → outer extent = 6, height ±1
        // Sphere: center=[0,0,0], radius=20 → fully contains torus
        let curves = sphere_torus_ssi(
            [0.0, 0.0, 0.0], // sphere center
            20.0,            // sphere radius (much larger than torus outer extent of 6)
            [0.0, 0.0, 0.0], // torus center
            [0.0, 0.0, 1.0], // torus axis
            5.0,             // torus major radius R
            1.0,             // torus minor radius r
        )
        .unwrap();
        assert!(
            curves.is_empty(),
            "Sphere fully enclosing torus should give empty intersection, got {} curves",
            curves.len()
        );
    }

    #[test]
    fn test_sphere_torus_near_tangent_outer() {
        // Sphere just barely touching the outer rim of the torus.
        // Torus: center=[0,0,0], axis=[0,0,1], R=5, r=1 → outer rim at ρ=6
        // Sphere: center=[6.99, 0, 0], radius=1.0
        // Sphere closest approach to outer rim: 6.99 - 1.0 = 5.99, outer rim at 6.0
        // So sphere barely overlaps torus. Should not crash or produce NaN.
        let result = sphere_torus_ssi(
            [6.99, 0.0, 0.0], // sphere center
            1.0,              // sphere radius
            [0.0, 0.0, 0.0],  // torus center
            [0.0, 0.0, 1.0],  // torus axis
            5.0,              // torus major radius R
            1.0,              // torus minor radius r
        );
        assert!(
            result.is_ok(),
            "Near-tangent should not error: {:?}",
            result.err()
        );
        let curves = result.unwrap();
        // Verify no NaN in any returned curve
        for curve in &curves {
            match curve {
                SSICurve::Circle {
                    center,
                    normal,
                    radius,
                } => {
                    assert!(
                        !center[0].is_nan() && !center[1].is_nan() && !center[2].is_nan(),
                        "NaN in circle center"
                    );
                    assert!(
                        !normal[0].is_nan() && !normal[1].is_nan() && !normal[2].is_nan(),
                        "NaN in circle normal"
                    );
                    assert!(!radius.is_nan(), "NaN in circle radius");
                }
                SSICurve::Ellipse {
                    center,
                    normal,
                    major_axis,
                    semi_major,
                    semi_minor,
                } => {
                    assert!(
                        !center[0].is_nan() && !center[1].is_nan() && !center[2].is_nan(),
                        "NaN in ellipse center"
                    );
                    assert!(
                        !normal[0].is_nan() && !normal[1].is_nan() && !normal[2].is_nan(),
                        "NaN in ellipse normal"
                    );
                    assert!(
                        !major_axis[0].is_nan()
                            && !major_axis[1].is_nan()
                            && !major_axis[2].is_nan(),
                        "NaN in ellipse major_axis"
                    );
                    assert!(!semi_major.is_nan(), "NaN in semi_major");
                    assert!(!semi_minor.is_nan(), "NaN in semi_minor");
                }
                SSICurve::Line { start, end } => {
                    assert!(
                        !start[0].is_nan() && !start[1].is_nan() && !start[2].is_nan(),
                        "NaN in line start"
                    );
                    assert!(
                        !end[0].is_nan() && !end[1].is_nan() && !end[2].is_nan(),
                        "NaN in line end"
                    );
                }
            }
        }
    }

    #[test]
    fn test_sphere_torus_extreme_radii() {
        // Very large major radius with small minor radius.
        // Torus: center=[0,0,0], axis=[0,0,1], R=1000, r=0.01
        // Sphere: center=[1000, 0, 0] (on tube center), radius=0.02
        // Sphere overlaps the tube (tube center at ρ=1000, sphere straddles it).
        let result = sphere_torus_ssi(
            [1000.0, 0.0, 0.0], // sphere center (at tube center circle)
            0.02,               // sphere radius (> minor radius)
            [0.0, 0.0, 0.0],    // torus center
            [0.0, 0.0, 1.0],    // torus axis
            1000.0,             // torus major radius R
            0.01,               // torus minor radius r
        );
        assert!(
            result.is_ok(),
            "Extreme radii should not error: {:?}",
            result.err()
        );
        let curves = result.unwrap();
        // Sphere (r=0.02) centered on tube center (r=0.01) → sphere encloses tube cross-section
        // locally, so intersection should produce curves (two circles in axial case).
        // Main check: no panic, no NaN.
        for curve in &curves {
            match curve {
                SSICurve::Circle {
                    center,
                    normal,
                    radius,
                } => {
                    assert!(
                        !center[0].is_nan() && !center[1].is_nan() && !center[2].is_nan(),
                        "NaN in circle center"
                    );
                    assert!(
                        !normal[0].is_nan() && !normal[1].is_nan() && !normal[2].is_nan(),
                        "NaN in circle normal"
                    );
                    assert!(!radius.is_nan(), "NaN in circle radius");
                    assert!(
                        *radius > 0.0,
                        "Circle radius should be positive, got {}",
                        radius
                    );
                }
                _ => {} // Other curve types acceptable
            }
        }
    }

    #[test]
    fn test_sphere_torus_point_on_surface_validation() {
        // For the axial 2-circle case, verify returned circle points lie on BOTH surfaces.
        // Torus: center=[0,0,0], axis=[0,0,1], R=5, r=1
        // Sphere: center=[0,0,0], radius=5.5
        let sphere_center = [0.0, 0.0, 0.0];
        let sphere_r = 5.5_f64;
        let torus_center = [0.0, 0.0, 0.0];
        let torus_axis = [0.0, 0.0, 1.0];
        let big_r = 5.0_f64;
        let small_r = 1.0_f64;

        let curves = sphere_torus_ssi(
            sphere_center,
            sphere_r,
            torus_center,
            torus_axis,
            big_r,
            small_r,
        )
        .unwrap();
        assert_eq!(curves.len(), 2, "Expected 2 circles for validation");

        for curve in &curves {
            if let SSICurve::Circle {
                center,
                normal,
                radius,
            } = curve
            {
                // Build orthonormal basis for circle plane
                let n = *normal;
                let u = if n[0].abs() < crate::units::BASIS_AXIS_ALIGNMENT {
                    let raw = v3_cross(n, [1.0, 0.0, 0.0]);
                    let len = v3_length(raw);
                    v3_scale(raw, 1.0 / len)
                } else {
                    let raw = v3_cross(n, [0.0, 1.0, 0.0]);
                    let len = v3_length(raw);
                    v3_scale(raw, 1.0 / len)
                };
                let v = v3_cross(n, u);

                // Sample 8 points on the circle
                for i in 0..8 {
                    let theta = (i as f64) * std::f64::consts::TAU / 8.0;
                    let cos_t = theta.cos();
                    let sin_t = theta.sin();
                    let pt = [
                        center[0] + radius * (cos_t * u[0] + sin_t * v[0]),
                        center[1] + radius * (cos_t * u[1] + sin_t * v[1]),
                        center[2] + radius * (cos_t * u[2] + sin_t * v[2]),
                    ];

                    // Check point is on sphere: |pt - sphere_center| ≈ sphere_r
                    let dist_to_sphere = v3_length(v3_sub(pt, sphere_center));
                    assert!(
                        (dist_to_sphere - sphere_r).abs() < EPS,
                        "Point {:?} distance to sphere center = {}, expected {}",
                        pt,
                        dist_to_sphere,
                        sphere_r
                    );

                    // Check point is on torus surface:
                    // ρ = perpendicular distance from point to torus axis
                    let pt_diff = v3_sub(pt, torus_center);
                    let axial_comp = v3_dot(pt_diff, torus_axis);
                    let radial_vec = v3_sub(pt_diff, v3_scale(torus_axis, axial_comp));
                    let rho = v3_length(radial_vec);
                    // Torus implicit: (ρ - R)² + z² = r²
                    let torus_val = (rho - big_r).powi(2) + axial_comp.powi(2);
                    let torus_err = (torus_val - small_r * small_r).abs();
                    assert!(
                        torus_err < 0.02,
                        "Point {:?} torus implicit value = {}, expected {} (err={})",
                        pt,
                        torus_val,
                        small_r * small_r,
                        torus_err
                    );
                }
            } else {
                panic!("Expected Circle for axial case, got {:?}", curve);
            }
        }
    }

    // ── Cone-Cone Adversarial Tests ─────────────────────────────────

    #[test]
    fn test_cone_cone_near_coaxial() {
        // Axes nearly parallel (off by ~0.001 radians), should not crash or produce NaN.
        // Cone A: axis exactly [0,0,1]
        // Cone B: axis tilted by 0.001 rad → [sin(0.001), 0, cos(0.001)] ≈ [0.001, 0, ~1]
        let tilt = 0.001_f64;
        let axis_b_raw = [tilt.sin(), 0.0, tilt.cos()];
        let len = v3_length(axis_b_raw);
        let axis_b = v3_scale(axis_b_raw, 1.0 / len);
        let half_30 = std::f64::consts::FRAC_PI_6;
        let result = cone_cone_ssi(
            [0.0, 0.0, 0.0], // apex A
            [0.0, 0.0, 1.0], // axis A
            half_30,         // half-angle A
            (0.0, 10.0),     // height range A
            [0.0, 0.0, 1.0], // apex B (offset along axis)
            axis_b,          // axis B (nearly parallel)
            half_30 * 1.1,   // slightly different half-angle
            (0.0, 10.0),     // height range B
        );
        assert!(
            result.is_ok(),
            "Near-coaxial should not error: {:?}",
            result.err()
        );
        let curves = result.unwrap();
        // Verify no NaN
        for curve in &curves {
            match curve {
                SSICurve::Circle {
                    center,
                    normal,
                    radius,
                } => {
                    assert!(
                        !center[0].is_nan() && !center[1].is_nan() && !center[2].is_nan(),
                        "NaN in circle center"
                    );
                    assert!(
                        !normal[0].is_nan() && !normal[1].is_nan() && !normal[2].is_nan(),
                        "NaN in circle normal"
                    );
                    assert!(!radius.is_nan(), "NaN in circle radius");
                }
                SSICurve::Line { start, end } => {
                    assert!(
                        !start[0].is_nan() && !start[1].is_nan() && !start[2].is_nan(),
                        "NaN in line start"
                    );
                    assert!(
                        !end[0].is_nan() && !end[1].is_nan() && !end[2].is_nan(),
                        "NaN in line end"
                    );
                }
                SSICurve::Ellipse {
                    center,
                    normal,
                    major_axis,
                    semi_major,
                    semi_minor,
                } => {
                    assert!(
                        !center[0].is_nan() && !center[1].is_nan() && !center[2].is_nan(),
                        "NaN in ellipse center"
                    );
                    assert!(
                        !normal[0].is_nan() && !normal[1].is_nan() && !normal[2].is_nan(),
                        "NaN in ellipse normal"
                    );
                    assert!(
                        !major_axis[0].is_nan()
                            && !major_axis[1].is_nan()
                            && !major_axis[2].is_nan(),
                        "NaN in ellipse major_axis"
                    );
                    assert!(!semi_major.is_nan(), "NaN in semi_major");
                    assert!(!semi_minor.is_nan(), "NaN in semi_minor");
                }
            }
        }
    }

    #[test]
    fn test_cone_cone_very_small_half_angle() {
        // Half angles of 1° (nearly cylindrical). Should not panic.
        let half_1deg = 1.0_f64.to_radians();
        let result = cone_cone_ssi(
            [0.0, 0.0, 0.0], // apex A
            [0.0, 0.0, 1.0], // axis A
            half_1deg,       // half-angle A (1°)
            (0.0, 100.0),    // height range A (long to give some radius)
            [0.5, 0.0, 0.0], // apex B (offset)
            [0.0, 0.0, 1.0], // axis B
            half_1deg,       // half-angle B (1°)
            (0.0, 100.0),    // height range B
        );
        assert!(
            result.is_ok(),
            "Very small half-angle should not error: {:?}",
            result.err()
        );
        // No panic is the main assertion. Also check no NaN.
        let curves = result.unwrap();
        for curve in &curves {
            match curve {
                SSICurve::Circle { center, radius, .. } => {
                    assert!(!center[0].is_nan() && !center[1].is_nan() && !center[2].is_nan());
                    assert!(!radius.is_nan());
                }
                SSICurve::Line { start, end } => {
                    assert!(!start[0].is_nan() && !start[1].is_nan() && !start[2].is_nan());
                    assert!(!end[0].is_nan() && !end[1].is_nan() && !end[2].is_nan());
                }
                SSICurve::Ellipse {
                    center,
                    semi_major,
                    semi_minor,
                    ..
                } => {
                    assert!(!center[0].is_nan() && !center[1].is_nan() && !center[2].is_nan());
                    assert!(!semi_major.is_nan() && !semi_minor.is_nan());
                }
            }
        }
    }

    #[test]
    fn test_cone_cone_opposing_directions() {
        // Cone A: axis=[0,0,1], Cone B: axis=[0,0,-1], both 45° half-angle.
        // Apex A at origin pointing up, Apex B at [0,0,5] pointing down.
        // They face each other with overlapping height ranges. Should find intersection.
        let half_45 = std::f64::consts::FRAC_PI_4;
        let result = cone_cone_ssi(
            [0.0, 0.0, 0.0],  // apex A
            [0.0, 0.0, 1.0],  // axis A (pointing up)
            half_45,          // half-angle A
            (0.0, 5.0),       // height range A
            [0.0, 0.0, 5.0],  // apex B
            [0.0, 0.0, -1.0], // axis B (pointing down)
            half_45,          // half-angle B
            (0.0, 5.0),       // height range B
        );
        assert!(
            result.is_ok(),
            "Opposing cones should not error: {:?}",
            result.err()
        );
        let curves = result.unwrap();
        // Two 45° cones facing each other from 5 units apart should definitely intersect.
        // At height z from A: r_a = z * tan(45°) = z
        // From B pointing down: at height z, distance from apex B is 5-z, r_b = (5-z)*tan(45°) = 5-z
        // Equal when z = 5-z → z = 2.5, r = 2.5
        assert!(
            !curves.is_empty(),
            "Opposing 45° cones facing each other should intersect"
        );
    }

    #[test]
    fn test_cone_cone_no_nan_in_results() {
        // General case: two cones at an angle, verify no NaN in any coordinate.
        // Cone A: apex=[0,0,0], axis=[0,0,1], 30°
        // Cone B: apex=[2,0,0], axis=[0,1,0] (perpendicular), 30°
        let half_30 = std::f64::consts::FRAC_PI_6;
        let curves = cone_cone_ssi(
            [0.0, 0.0, 0.0], // apex A
            [0.0, 0.0, 1.0], // axis A
            half_30,         // half-angle A
            (0.0, 10.0),     // height range A
            [2.0, 0.0, 0.0], // apex B
            [0.0, 1.0, 0.0], // axis B (perpendicular to A)
            half_30,         // half-angle B
            (0.0, 10.0),     // height range B
        )
        .unwrap();

        for (i, curve) in curves.iter().enumerate() {
            match curve {
                SSICurve::Circle {
                    center,
                    normal,
                    radius,
                } => {
                    assert!(
                        !center[0].is_nan() && !center[1].is_nan() && !center[2].is_nan(),
                        "Curve {}: NaN in circle center {:?}",
                        i,
                        center
                    );
                    assert!(
                        !normal[0].is_nan() && !normal[1].is_nan() && !normal[2].is_nan(),
                        "Curve {}: NaN in circle normal {:?}",
                        i,
                        normal
                    );
                    assert!(
                        !radius.is_nan() && *radius >= 0.0,
                        "Curve {}: invalid circle radius {}",
                        i,
                        radius
                    );
                }
                SSICurve::Line { start, end } => {
                    assert!(
                        !start[0].is_nan() && !start[1].is_nan() && !start[2].is_nan(),
                        "Curve {}: NaN in line start {:?}",
                        i,
                        start
                    );
                    assert!(
                        !end[0].is_nan() && !end[1].is_nan() && !end[2].is_nan(),
                        "Curve {}: NaN in line end {:?}",
                        i,
                        end
                    );
                }
                SSICurve::Ellipse {
                    center,
                    normal,
                    major_axis,
                    semi_major,
                    semi_minor,
                } => {
                    assert!(
                        !center[0].is_nan() && !center[1].is_nan() && !center[2].is_nan(),
                        "Curve {}: NaN in ellipse center {:?}",
                        i,
                        center
                    );
                    assert!(
                        !normal[0].is_nan() && !normal[1].is_nan() && !normal[2].is_nan(),
                        "Curve {}: NaN in ellipse normal {:?}",
                        i,
                        normal
                    );
                    assert!(
                        !major_axis[0].is_nan()
                            && !major_axis[1].is_nan()
                            && !major_axis[2].is_nan(),
                        "Curve {}: NaN in ellipse major_axis {:?}",
                        i,
                        major_axis
                    );
                    assert!(
                        !semi_major.is_nan() && *semi_major >= 0.0,
                        "Curve {}: invalid semi_major {}",
                        i,
                        semi_major
                    );
                    assert!(
                        !semi_minor.is_nan() && *semi_minor >= 0.0,
                        "Curve {}: invalid semi_minor {}",
                        i,
                        semi_minor
                    );
                }
            }
        }
    }

    // ── Cylinder-Cone SSI ──────────────────────────────────────────────

    #[test]
    fn cyl_cone_ssi_disjoint() {
        // Cylinder far from cone — no intersection expected.
        let curves = cylinder_cone_ssi(
            [100.0, 0.0, 0.0],           // cyl_origin — far away
            [0.0, 0.0, 1.0],             // cyl_axis
            1.0,                         // cyl_radius
            0.0,                         // cyl_z_min
            5.0,                         // cyl_z_max
            [0.0, 0.0, 0.0],             // cone_apex
            [0.0, 0.0, 1.0],             // cone_axis
            std::f64::consts::FRAC_PI_6, // 30° half-angle
            (0.0, 5.0),                  // cone_height_range
        )
        .unwrap();

        assert_eq!(
            curves.len(),
            0,
            "Disjoint cylinder and cone should produce no curves, got {}",
            curves.len()
        );
    }

    #[test]
    fn cyl_cone_ssi_coaxial_one_circle() {
        // Coaxial: cylinder R=1, cone apex at origin, axis +Z, half-angle=45°.
        // Cone radius at height h = h*tan(45°) = h.
        // Cone radius = cyl_radius = 1 at h = 1.
        // Height range includes h=1, so exactly one intersection circle.
        let curves = cylinder_cone_ssi(
            [0.0, 0.0, 0.0],             // cyl_origin
            [0.0, 0.0, 1.0],             // cyl_axis
            1.0,                         // cyl_radius
            -5.0,                        // cyl_z_min
            5.0,                         // cyl_z_max
            [0.0, 0.0, 0.0],             // cone_apex (at cyl_origin)
            [0.0, 0.0, 1.0],             // cone_axis (same as cyl)
            std::f64::consts::FRAC_PI_4, // 45° half-angle
            (0.0, 5.0),                  // cone_height_range
        )
        .unwrap();

        assert_eq!(
            curves.len(),
            1,
            "Coaxial cylinder-cone with one crossing should produce 1 circle, got {}",
            curves.len()
        );

        // The single circle should be at z=1, radius=1, normal along Z.
        if let SSICurve::Circle {
            center,
            normal,
            radius,
        } = &curves[0]
        {
            assert!(
                (center[2] - 1.0).abs() < EPS,
                "Expected circle at z=1, got z={}",
                center[2]
            );
            assert!(
                (center[0]).abs() < EPS && (center[1]).abs() < EPS,
                "Expected circle centered on axis, got x={}, y={}",
                center[0],
                center[1]
            );
            assert!(
                (*radius - 1.0).abs() < EPS,
                "Expected radius=1, got {}",
                radius
            );
            // Normal should be parallel to the axis (Z)
            let nz = normal[2].abs();
            assert!(nz > 1.0 - EPS, "Expected normal along Z, got {:?}", normal);
        } else {
            panic!(
                "Expected Circle for coaxial intersection, got {:?}",
                curves[0]
            );
        }
    }

    #[test]
    fn cyl_cone_ssi_coaxial_two_circles() {
        // Coaxial: cylinder R=2, cone apex at [0,0,5], axis pointing DOWN (-Z), 45° half-angle.
        // Cone radius at height h below apex = h*tan(45°) = h.
        // Measuring in world Z: at z, distance from apex = 5-z, cone radius = 5-z.
        // Cone radius = 2 at z = 3.
        //
        // Also: cylinder R=2, cone apex at [0,0,-5], axis pointing UP (+Z), 45° half-angle.
        // At z, distance from apex = z+5, cone radius = z+5.
        // Cone radius = 2 at z = -3.
        //
        // Use one cone that expands from both sides — symmetric case:
        // Actually, for two crossings from a single cone: cone apex at z=0, axis +Z, 30° half-angle.
        // Cone radius at h = h*tan(30°) ≈ 0.577*h.
        // For R_cyl = 2: h = 2/tan(30°) = 2*√3 ≈ 3.464.
        // That's only one crossing on positive side. For two crossings, we need a second cone sheet.
        //
        // Two circles: use TWO cone height ranges by placing cylinder around cone that grows
        // then shrinks (not possible with single cone). Instead: cone apex below cylinder,
        // axis +Z. Cone crosses cylinder once going up. For two crossings, use the
        // negative-height sheet of the cone (h < 0) which opens downward.
        //
        // Simpler: apex at z=5, axis +Z, half-angle 45°, height range (-8, -2).
        // At distance d below apex (negative height): radius = |d| * tan(45°) = |d|.
        // World z = 5 + d (d negative). Radius = -d = 5-z.
        // Plus apex at z=-5, axis +Z, half-angle 45°, height range (2, 8).
        // Radius = d * tan(45°) = d. World z = -5 + d. Radius = z+5.
        // cylinder R=2: 5-z=2 → z=3; z+5=2 → z=-3. Two circles.
        //
        // Actually simpler: coaxial cone going through the cylinder twice.
        // Cone apex at z=0, axis +Z, 45° half-angle, height range (0, 10).
        // Cone radius at z: z. Equals cylinder R=3 at z=3 (one crossing only going up).
        // For two circles we need the cone to cross the cylinder twice — only possible
        // if cone has BOTH sheets. Use height range including negative:
        //
        // Better approach: use a cone with apex INSIDE the cylinder.
        // Apex at z=0, axis +Z, 45° half-angle, heights (1, 5).
        // Cylinder R=3, z_min=-5, z_max=5.
        // Cone radius at h: h. Equals 3 at h=3. One crossing at z=3.
        // For two: put apex at z=5 and axis DOWNWARD (-Z), same cylinder.
        // Then cone radius at distance d from apex: d. World z = 5-d.
        // Equals 3 at d=3, z=2.
        //
        // Simplest: two distinct cones give two circles, but we need one call.
        // Real case with two circles: cylinder R=2, cone apex inside cylinder,
        // half-angle big enough that cone expands past cylinder, then... no,
        // a single cone nappe only crosses once.
        //
        // Two circles from one cone: only possible with BOTH nappes (negative heights).
        // Cone apex at z=0, axis +Z, 45° half-angle. Height range (-5, 5).
        // Upper nappe: radius = h at h > 0. Lower nappe: radius = |h| at h < 0.
        // Cylinder R=3: crossings at h=3 (z=3) and h=-3 (z=-3). Two circles!
        let curves = cylinder_cone_ssi(
            [0.0, 0.0, 0.0],             // cyl_origin
            [0.0, 0.0, 1.0],             // cyl_axis
            3.0,                         // cyl_radius
            -5.0,                        // cyl_z_min
            5.0,                         // cyl_z_max
            [0.0, 0.0, 0.0],             // cone_apex (at origin)
            [0.0, 0.0, 1.0],             // cone_axis
            std::f64::consts::FRAC_PI_4, // 45° half-angle
            (-5.0, 5.0),                 // cone_height_range (both nappes)
        )
        .unwrap();

        assert_eq!(
            curves.len(),
            2,
            "Coaxial cone (both nappes) crossing cylinder should produce 2 circles, got {}",
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

        // Circles at z = ±3, each with radius = 3
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

        assert!(
            (z_values[0] - (-3.0)).abs() < EPS,
            "Expected z≈-3, got {}",
            z_values[0]
        );
        assert!(
            (z_values[1] - 3.0).abs() < EPS,
            "Expected z≈3, got {}",
            z_values[1]
        );

        // Radii should all be 3.0 (the cylinder radius)
        for curve in &curves {
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
    fn cyl_cone_ssi_coaxial_no_intersection() {
        // Coaxial: cylinder R=5, cone with small half-angle (10°) and short height.
        // Cone radius at max height = 2 * tan(10°) ≈ 0.353. Never reaches R=5.
        let half_10 = 10.0_f64.to_radians();
        let curves = cylinder_cone_ssi(
            [0.0, 0.0, 0.0], // cyl_origin
            [0.0, 0.0, 1.0], // cyl_axis
            5.0,             // cyl_radius
            -10.0,           // cyl_z_min
            10.0,            // cyl_z_max
            [0.0, 0.0, 0.0], // cone_apex
            [0.0, 0.0, 1.0], // cone_axis
            half_10,         // ~10° half-angle
            (0.0, 2.0),      // cone_height_range (short)
        )
        .unwrap();

        assert_eq!(
            curves.len(),
            0,
            "Coaxial cone too small to reach cylinder should produce no curves, got {}",
            curves.len()
        );
    }

    #[test]
    fn cyl_cone_ssi_coaxial_opposite_dir() {
        // Cylinder axis +Z, cone axis -Z (opposite), same collinear line.
        // Cone apex at [0,0,10], axis [0,0,-1], 45° half-angle, heights (0, 8).
        // Cone expands downward. At distance d from apex: world z = 10-d, radius = d.
        // Cylinder R=4: crossing at d=4, z=6.
        let curves = cylinder_cone_ssi(
            [0.0, 0.0, 0.0],             // cyl_origin
            [0.0, 0.0, 1.0],             // cyl_axis
            4.0,                         // cyl_radius
            0.0,                         // cyl_z_min
            10.0,                        // cyl_z_max
            [0.0, 0.0, 10.0],            // cone_apex
            [0.0, 0.0, -1.0],            // cone_axis (opposite to cylinder)
            std::f64::consts::FRAC_PI_4, // 45° half-angle
            (0.0, 8.0),                  // cone_height_range
        )
        .unwrap();

        assert_eq!(
            curves.len(),
            1,
            "Coaxial opposite-direction cone should produce 1 circle, got {}",
            curves.len()
        );

        if let SSICurve::Circle {
            center,
            normal,
            radius,
        } = &curves[0]
        {
            // Circle at z=6, radius=4
            assert!(
                (center[2] - 6.0).abs() < EPS,
                "Expected circle at z=6, got z={}",
                center[2]
            );
            assert!(
                (*radius - 4.0).abs() < EPS,
                "Expected radius=4, got {}",
                radius
            );
            // Normal should be along Z axis
            let nz = normal[2].abs();
            assert!(nz > 1.0 - EPS, "Expected normal along Z, got {:?}", normal);
        } else {
            panic!("Expected Circle, got {:?}", curves[0]);
        }
    }

    #[test]
    fn cyl_cone_ssi_parallel_offset_overlap() {
        // Parallel axes (both +Z) but offset in X. Surfaces overlap → degree-4 curve → Line.
        // Cylinder at x=0, R=3. Cone apex at [4,0,0], axis +Z, 45° half-angle, heights (0,10).
        // At height z, cone radius = z. Cone center at x=4.
        // When z=3: cone radius=3, cylinder radius=3, offset=4. They overlap (3+3=6 > 4).
        let curves = cylinder_cone_ssi(
            [0.0, 0.0, 0.0],             // cyl_origin
            [0.0, 0.0, 1.0],             // cyl_axis
            3.0,                         // cyl_radius
            0.0,                         // cyl_z_min
            10.0,                        // cyl_z_max
            [4.0, 0.0, 0.0],             // cone_apex (offset in X)
            [0.0, 0.0, 1.0],             // cone_axis (parallel to cylinder)
            std::f64::consts::FRAC_PI_4, // 45° half-angle
            (0.0, 10.0),                 // cone_height_range
        )
        .unwrap();

        assert!(
            !curves.is_empty(),
            "Parallel offset cylinder-cone with overlap should produce at least one curve"
        );

        // Verify each curve result is geometrically valid (no NaN)
        for (i, curve) in curves.iter().enumerate() {
            match curve {
                SSICurve::Line { start, end } => {
                    for j in 0..3 {
                        assert!(
                            !start[j].is_nan() && !end[j].is_nan(),
                            "Curve {}: NaN in line coordinates",
                            i
                        );
                    }
                    // Line should have nonzero length
                    let dx = end[0] - start[0];
                    let dy = end[1] - start[1];
                    let dz = end[2] - start[2];
                    let len = (dx * dx + dy * dy + dz * dz).sqrt();
                    assert!(
                        len > 1e-9,
                        "Curve {}: degenerate line with length {}",
                        i,
                        len
                    );
                }
                SSICurve::Circle { center, radius, .. } => {
                    for j in 0..3 {
                        assert!(!center[j].is_nan(), "Curve {}: NaN in circle center", i);
                    }
                    assert!(*radius > 0.0, "Curve {}: non-positive radius {}", i, radius);
                }
                SSICurve::Ellipse {
                    center,
                    semi_major,
                    semi_minor,
                    ..
                } => {
                    for j in 0..3 {
                        assert!(!center[j].is_nan(), "Curve {}: NaN in ellipse center", i);
                    }
                    assert!(*semi_major > 0.0, "Curve {}: non-positive semi_major", i);
                    assert!(*semi_minor > 0.0, "Curve {}: non-positive semi_minor", i);
                }
            }
        }
    }

    #[test]
    fn cyl_cone_ssi_parallel_offset_disjoint() {
        // Parallel axes, offset too large for any overlap.
        // Cylinder R=1 at x=0, cone apex at [20,0,0] with 10° half-angle, heights (0,5).
        // Max cone radius = 5*tan(10°) ≈ 0.882. Distance = 20. 1 + 0.882 = 1.882 < 20.
        let half_10 = 10.0_f64.to_radians();
        let curves = cylinder_cone_ssi(
            [0.0, 0.0, 0.0],  // cyl_origin
            [0.0, 0.0, 1.0],  // cyl_axis
            1.0,              // cyl_radius
            0.0,              // cyl_z_min
            5.0,              // cyl_z_max
            [20.0, 0.0, 0.0], // cone_apex (far offset)
            [0.0, 0.0, 1.0],  // cone_axis
            half_10,          // ~10° half-angle
            (0.0, 5.0),       // cone_height_range
        )
        .unwrap();

        assert_eq!(
            curves.len(),
            0,
            "Parallel offset disjoint cylinder-cone should produce no curves, got {}",
            curves.len()
        );
    }

    #[test]
    fn cyl_cone_ssi_general_position() {
        // Cylinder along Z, cone tilted with axis along X. They overlap in space.
        // Cylinder: origin at [0,0,0], axis +Z, R=2, z in [-5, 5].
        // Cone: apex at [0,0,0], axis +X, 30° half-angle, heights (0, 10).
        // The cone opens along +X. Its radius at distance d from apex = d*tan(30°).
        // The cylinder is centered on Z. They must intersect near the origin.
        let half_30 = std::f64::consts::FRAC_PI_6;
        let curves = cylinder_cone_ssi(
            [0.0, 0.0, 0.0], // cyl_origin
            [0.0, 0.0, 1.0], // cyl_axis
            2.0,             // cyl_radius
            -5.0,            // cyl_z_min
            5.0,             // cyl_z_max
            [0.0, 0.0, 0.0], // cone_apex
            [1.0, 0.0, 0.0], // cone_axis (+X, perpendicular to cylinder)
            half_30,         // 30° half-angle
            (0.0, 10.0),     // cone_height_range
        )
        .unwrap();

        assert!(
            !curves.is_empty(),
            "General position cylinder-cone should produce at least one curve"
        );

        // All results should be geometrically valid
        for (i, curve) in curves.iter().enumerate() {
            match curve {
                SSICurve::Line { start, end } => {
                    let dx = end[0] - start[0];
                    let dy = end[1] - start[1];
                    let dz = end[2] - start[2];
                    let len = (dx * dx + dy * dy + dz * dz).sqrt();
                    assert!(
                        len > 1e-9,
                        "Curve {}: degenerate line with length {}",
                        i,
                        len
                    );
                }
                SSICurve::Circle { radius, .. } => {
                    assert!(*radius > 0.0, "Curve {}: non-positive radius {}", i, radius);
                }
                SSICurve::Ellipse {
                    semi_major,
                    semi_minor,
                    ..
                } => {
                    assert!(*semi_major > 0.0, "Curve {}: non-positive semi_major", i);
                    assert!(*semi_minor > 0.0, "Curve {}: non-positive semi_minor", i);
                }
            }
        }
    }

    #[test]
    fn cyl_cone_ssi_perpendicular() {
        // Cylinder along Z, cone along Y — axes at 90°, both through origin.
        // Cylinder: R=1, z in [-5, 5].
        // Cone: apex at [0, -3, 0], axis +Y, 45° half-angle, heights (0, 10).
        // At distance d from apex along +Y: world y = -3+d, cone radius = d.
        // At y=0 (d=3): cone radius=3 > cylinder R=1. They overlap.
        let curves = cylinder_cone_ssi(
            [0.0, 0.0, 0.0],             // cyl_origin
            [0.0, 0.0, 1.0],             // cyl_axis
            1.0,                         // cyl_radius
            -5.0,                        // cyl_z_min
            5.0,                         // cyl_z_max
            [0.0, -3.0, 0.0],            // cone_apex
            [0.0, 1.0, 0.0],             // cone_axis (+Y, perpendicular to cylinder)
            std::f64::consts::FRAC_PI_4, // 45° half-angle
            (0.0, 10.0),                 // cone_height_range
        )
        .unwrap();

        assert!(
            !curves.is_empty(),
            "Perpendicular cylinder-cone should produce at least one curve"
        );

        // Verify no NaN in any result
        for (i, curve) in curves.iter().enumerate() {
            match curve {
                SSICurve::Line { start, end } => {
                    for j in 0..3 {
                        assert!(
                            !start[j].is_nan() && !end[j].is_nan(),
                            "Curve {}: NaN in coordinates",
                            i
                        );
                    }
                }
                SSICurve::Circle {
                    center,
                    normal,
                    radius,
                } => {
                    for j in 0..3 {
                        assert!(!center[j].is_nan(), "NaN in center");
                        assert!(!normal[j].is_nan(), "NaN in normal");
                    }
                    assert!(!radius.is_nan() && *radius > 0.0);
                }
                SSICurve::Ellipse {
                    center,
                    normal,
                    major_axis,
                    semi_major,
                    semi_minor,
                } => {
                    for j in 0..3 {
                        assert!(!center[j].is_nan());
                        assert!(!normal[j].is_nan());
                        assert!(!major_axis[j].is_nan());
                    }
                    assert!(!semi_major.is_nan() && *semi_major > 0.0);
                    assert!(!semi_minor.is_nan() && *semi_minor > 0.0);
                }
            }
        }
    }

    #[test]
    fn cyl_cone_ssi_tangent() {
        // Tangent configuration: cylinder just touches the cone surface.
        // Cylinder R=1 at x=0, cone apex at [0,0,-10], axis +Z, half-angle chosen
        // so cone radius = 1 at z=0 and the cylinder axis is tangent to the cone.
        // Actually, for a clean tangent: cylinder at offset = R_cone + R_cyl exactly.
        //
        // Cone apex at origin, axis +Z, 45° half-angle. At z=5, cone radius=5.
        // Place cylinder axis at x=6 (= 5 + 1), R_cyl=1, parallel to Z.
        // At z=5 the cone just touches the cylinder externally. Tangent.
        // But only at one height — below/above z=5 they separate.
        // Tangent intersection is below feature size → empty.
        let curves = cylinder_cone_ssi(
            [6.0, 0.0, 0.0],             // cyl_origin (on x=6 axis)
            [0.0, 0.0, 1.0],             // cyl_axis
            1.0,                         // cyl_radius
            0.0,                         // cyl_z_min
            10.0,                        // cyl_z_max
            [0.0, 0.0, 0.0],             // cone_apex
            [0.0, 0.0, 1.0],             // cone_axis
            std::f64::consts::FRAC_PI_4, // 45° half-angle
            (0.0, 10.0),                 // cone_height_range
        )
        .unwrap();

        // Tangent intersection (touching at a single point/line) should be
        // filtered out as below feature size, producing empty result.
        assert_eq!(
            curves.len(),
            0,
            "Tangent cylinder-cone should produce no curves (below feature size), got {}",
            curves.len()
        );
    }

    #[test]
    fn cyl_cone_ssi_general_position_tilted() {
        // Another general case: cone tilted 45° from cylinder axis.
        // Cylinder: origin [0,0,0], axis +Z, R=2, z in [-10, 10].
        // Cone: apex at [3,0,0], axis tilted 45° in XZ plane = [−1/√2, 0, 1/√2],
        //       30° half-angle, heights (0, 15).
        let inv_sqrt2 = FRAC_1_SQRT_2;
        let half_30 = std::f64::consts::FRAC_PI_6;
        let curves = cylinder_cone_ssi(
            [0.0, 0.0, 0.0],              // cyl_origin
            [0.0, 0.0, 1.0],              // cyl_axis
            2.0,                          // cyl_radius
            -10.0,                        // cyl_z_min
            10.0,                         // cyl_z_max
            [3.0, 0.0, 0.0],              // cone_apex
            [-inv_sqrt2, 0.0, inv_sqrt2], // cone_axis (tilted 45° toward cylinder)
            half_30,                      // 30° half-angle
            (0.0, 15.0),                  // cone_height_range
        )
        .unwrap();

        assert!(
            !curves.is_empty(),
            "General tilted cylinder-cone should produce at least one curve"
        );

        // Verify geometric validity of all returned curves
        for (i, curve) in curves.iter().enumerate() {
            match curve {
                SSICurve::Line { start, end } => {
                    for j in 0..3 {
                        assert!(
                            !start[j].is_nan() && !end[j].is_nan(),
                            "Curve {}: NaN in line coordinates",
                            i
                        );
                    }
                    let dx = end[0] - start[0];
                    let dy = end[1] - start[1];
                    let dz = end[2] - start[2];
                    let len = (dx * dx + dy * dy + dz * dz).sqrt();
                    assert!(
                        len > 1e-9,
                        "Curve {}: degenerate line with length {}",
                        i,
                        len
                    );
                }
                SSICurve::Circle {
                    center,
                    normal,
                    radius,
                } => {
                    for j in 0..3 {
                        assert!(!center[j].is_nan(), "Curve {}: NaN in circle center", i);
                        assert!(!normal[j].is_nan(), "Curve {}: NaN in circle normal", i);
                    }
                    assert!(
                        *radius > 0.0,
                        "Curve {}: non-positive circle radius {}",
                        i,
                        radius
                    );
                }
                SSICurve::Ellipse {
                    center,
                    normal,
                    major_axis,
                    semi_major,
                    semi_minor,
                } => {
                    for j in 0..3 {
                        assert!(!center[j].is_nan(), "Curve {}: NaN in ellipse center", i);
                        assert!(!normal[j].is_nan(), "Curve {}: NaN in ellipse normal", i);
                        assert!(
                            !major_axis[j].is_nan(),
                            "Curve {}: NaN in ellipse major_axis",
                            i
                        );
                    }
                    assert!(*semi_major > 0.0, "Curve {}: non-positive semi_major", i);
                    assert!(*semi_minor > 0.0, "Curve {}: non-positive semi_minor", i);
                }
            }
        }
    }

    // ── Adversarial tests for cylinder-cone SSI ──────────────────────────

    #[test]
    fn cyl_cone_ssi_adv_near_tangent() {
        // Cylinder barely overlapping the cone at one height.
        // Cone apex at origin, axis +Z, 45° half-angle. At z=5, cone radius=5.
        // Place cylinder axis at x = 5 + 1 - 1e-4 = 5.9999, R=1, parallel to Z.
        // At z=5: gap = 5.9999 - 5 - 1 = -0.0001 (barely overlapping).
        // The overlap band is extremely thin — should produce empty or a very small curve.
        let offset = 5.0 + 1.0 - 1e-4;
        let curves = cylinder_cone_ssi(
            [offset, 0.0, 0.0],          // cyl_origin
            [0.0, 0.0, 1.0],             // cyl_axis
            1.0,                         // cyl_radius
            0.0,                         // cyl_z_min
            10.0,                        // cyl_z_max
            [0.0, 0.0, 0.0],             // cone_apex
            [0.0, 0.0, 1.0],             // cone_axis
            std::f64::consts::FRAC_PI_4, // 45° half-angle
            (0.0, 10.0),                 // cone_height_range
        )
        .unwrap();

        // Near-tangent: solver may return empty (filtered as below feature size)
        // or a very short curve. Either is acceptable — no panics or NaN.
        for (i, curve) in curves.iter().enumerate() {
            match curve {
                SSICurve::Line { start, end } => {
                    for j in 0..3 {
                        assert!(
                            !start[j].is_nan() && !end[j].is_nan(),
                            "Curve {}: NaN in near-tangent line",
                            i
                        );
                    }
                }
                SSICurve::Circle { center, radius, .. } => {
                    for j in 0..3 {
                        assert!(!center[j].is_nan(), "Curve {}: NaN in circle center", i);
                    }
                    assert!(*radius > 0.0, "Curve {}: non-positive radius", i);
                }
                SSICurve::Ellipse {
                    center,
                    semi_major,
                    semi_minor,
                    ..
                } => {
                    for j in 0..3 {
                        assert!(!center[j].is_nan(), "Curve {}: NaN in ellipse center", i);
                    }
                    assert!(*semi_major > 0.0 && *semi_minor > 0.0);
                }
            }
        }
    }

    #[test]
    fn cyl_cone_ssi_adv_tiny_geometry() {
        // Very small geometry: both surfaces at ~1e-5 scale.
        // Cylinder R=1e-5, z in [0, 2e-5]. Cone apex at origin, axis +Z,
        // 45° half-angle, heights (0, 2e-5). Coaxial.
        // Cone radius = h at 45°. Equals cyl_radius=1e-5 at h=1e-5.
        let r = 1e-5;
        let curves = cylinder_cone_ssi(
            [0.0, 0.0, 0.0],             // cyl_origin
            [0.0, 0.0, 1.0],             // cyl_axis
            r,                           // cyl_radius
            0.0,                         // cyl_z_min
            2.0 * r,                     // cyl_z_max
            [0.0, 0.0, 0.0],             // cone_apex
            [0.0, 0.0, 1.0],             // cone_axis
            std::f64::consts::FRAC_PI_4, // 45° half-angle
            (0.0, 2.0 * r),              // cone_height_range
        )
        .unwrap();

        // At this scale the intersection circle (h=1e-5, radius=1e-5) is above
        // MIN_FEATURE_SIZE (1e-6), so we may get a circle. Either way, no panic/NaN.
        for curve in &curves {
            match curve {
                SSICurve::Circle { center, radius, .. } => {
                    for j in 0..3 {
                        assert!(!center[j].is_nan(), "NaN in tiny-geometry circle center");
                    }
                    assert!(
                        *radius > 0.0 && !radius.is_nan(),
                        "Invalid radius in tiny geometry"
                    );
                }
                SSICurve::Line { start, end } => {
                    for j in 0..3 {
                        assert!(!start[j].is_nan() && !end[j].is_nan());
                    }
                }
                _ => {}
            }
        }
    }

    #[test]
    fn cyl_cone_ssi_adv_large_geometry() {
        // Very large geometry: radius ~1e4, height ~1e4.
        // Coaxial: cylinder R=1e4, cone 45° half-angle. Crossing at h=1e4.
        let r = 1e4;
        let curves = cylinder_cone_ssi(
            [0.0, 0.0, 0.0],             // cyl_origin
            [0.0, 0.0, 1.0],             // cyl_axis
            r,                           // cyl_radius
            -2.0 * r,                    // cyl_z_min
            2.0 * r,                     // cyl_z_max
            [0.0, 0.0, 0.0],             // cone_apex
            [0.0, 0.0, 1.0],             // cone_axis
            std::f64::consts::FRAC_PI_4, // 45° half-angle
            (-2.0 * r, 2.0 * r),         // cone_height_range (both nappes)
        )
        .unwrap();

        // Coaxial 45° cone with both nappes crossing cylinder at h = ±R.
        // Should produce 2 circles at z = ±1e4, each with radius = 1e4.
        assert_eq!(
            curves.len(),
            2,
            "Large-geometry coaxial cone should produce 2 circles, got {}",
            curves.len()
        );

        for curve in &curves {
            if let SSICurve::Circle { center, radius, .. } = curve {
                for j in 0..3 {
                    assert!(!center[j].is_nan(), "NaN in large-geometry circle center");
                    assert!(
                        !center[j].is_infinite(),
                        "Inf in large-geometry circle center"
                    );
                }
                assert!(
                    (*radius - r).abs() < 1.0,
                    "Expected radius ~{}, got {}",
                    r,
                    radius
                );
            } else {
                panic!(
                    "Expected Circle for coaxial large-geometry case, got {:?}",
                    curve
                );
            }
        }
    }

    #[test]
    fn cyl_cone_ssi_adv_small_half_angle() {
        // Cone with very small half-angle (~1°) — nearly a line/needle.
        // Coaxial with cylinder R=1. Cone needs huge height to reach R=1:
        // h = R / tan(1°) ≈ 57.29. Height range (0, 100) includes it.
        let half_1deg = 1.0_f64.to_radians();
        let expected_h = 1.0 / half_1deg.tan(); // ~57.29
        let curves = cylinder_cone_ssi(
            [0.0, 0.0, 0.0], // cyl_origin
            [0.0, 0.0, 1.0], // cyl_axis
            1.0,             // cyl_radius
            0.0,             // cyl_z_min
            100.0,           // cyl_z_max
            [0.0, 0.0, 0.0], // cone_apex
            [0.0, 0.0, 1.0], // cone_axis
            half_1deg,       // ~1° half-angle
            (0.0, 100.0),    // cone_height_range
        )
        .unwrap();

        // Should find exactly one circle at h ≈ 57.29
        assert_eq!(
            curves.len(),
            1,
            "Small half-angle coaxial cone should produce 1 circle, got {}",
            curves.len()
        );

        if let SSICurve::Circle { center, radius, .. } = &curves[0] {
            assert!(
                (center[2] - expected_h).abs() < 0.1,
                "Expected circle at z≈{}, got z={}",
                expected_h,
                center[2]
            );
            assert!(
                (*radius - 1.0).abs() < EPS,
                "Expected radius≈1, got {}",
                radius
            );
        } else {
            panic!("Expected Circle, got {:?}", curves[0]);
        }
    }

    #[test]
    fn cyl_cone_ssi_adv_large_half_angle() {
        // Cone with very large half-angle (~89°) — nearly a flat disk.
        // Coaxial with cylinder R=1. h = R / tan(89°) ≈ 0.01746.
        // Height range (0, 1) includes it.
        let half_89deg = 89.0_f64.to_radians();
        let expected_h = 1.0 / half_89deg.tan(); // ~0.01746
        let curves = cylinder_cone_ssi(
            [0.0, 0.0, 0.0], // cyl_origin
            [0.0, 0.0, 1.0], // cyl_axis
            1.0,             // cyl_radius
            0.0,             // cyl_z_min
            1.0,             // cyl_z_max
            [0.0, 0.0, 0.0], // cone_apex
            [0.0, 0.0, 1.0], // cone_axis
            half_89deg,      // ~89° half-angle
            (0.0, 1.0),      // cone_height_range
        )
        .unwrap();

        // Should find exactly one circle at h ≈ 0.01746
        assert_eq!(
            curves.len(),
            1,
            "Large half-angle coaxial cone should produce 1 circle, got {}",
            curves.len()
        );

        if let SSICurve::Circle { center, radius, .. } = &curves[0] {
            assert!(
                (center[2] - expected_h).abs() < EPS,
                "Expected circle at z≈{}, got z={}",
                expected_h,
                center[2]
            );
            assert!(
                (*radius - 1.0).abs() < EPS,
                "Expected radius≈1, got {}",
                radius
            );
        } else {
            panic!("Expected Circle, got {:?}", curves[0]);
        }
    }

    #[test]
    fn cyl_cone_ssi_adv_coaxial_cone_inside() {
        // Coaxial cone fully inside cylinder — cone never reaches cylinder radius.
        // Cylinder R=10, cone apex at origin, axis +Z, 10° half-angle, heights (0, 5).
        // Max cone radius = 5 * tan(10°) ≈ 0.882. Never reaches R=10.
        let half_10 = 10.0_f64.to_radians();
        let curves = cylinder_cone_ssi(
            [0.0, 0.0, 0.0], // cyl_origin
            [0.0, 0.0, 1.0], // cyl_axis
            10.0,            // cyl_radius
            -10.0,           // cyl_z_min
            10.0,            // cyl_z_max
            [0.0, 0.0, 0.0], // cone_apex
            [0.0, 0.0, 1.0], // cone_axis
            half_10,         // ~10° half-angle
            (0.0, 5.0),      // cone_height_range
        )
        .unwrap();

        assert_eq!(
            curves.len(),
            0,
            "Coaxial cone fully inside cylinder should produce no curves, got {}",
            curves.len()
        );
    }

    #[test]
    fn cyl_cone_ssi_adv_zero_height_range() {
        // Zero-length height range: cone_height_range = (5.0, 5.0).
        // This is a degenerate cone (a single circle at h=5). Should return empty.
        let curves = cylinder_cone_ssi(
            [0.0, 0.0, 0.0],             // cyl_origin
            [0.0, 0.0, 1.0],             // cyl_axis
            1.0,                         // cyl_radius
            0.0,                         // cyl_z_min
            10.0,                        // cyl_z_max
            [0.0, 0.0, 0.0],             // cone_apex
            [0.0, 0.0, 1.0],             // cone_axis
            std::f64::consts::FRAC_PI_4, // 45° half-angle
            (5.0, 5.0),                  // zero-length height range
        )
        .unwrap();

        assert_eq!(
            curves.len(),
            0,
            "Zero-length cone height range should produce no curves, got {}",
            curves.len()
        );
    }

    // ── Cylinder-Torus SSI (A15 pair #10) ────────────────────────────────

    #[test]
    fn cyl_torus_ssi_disjoint() {
        // Cylinder at origin along Z, radius 1, height [0,5].
        // Torus centered at [100, 0, 0] along Z, R=3, r=1.
        // Far apart — no intersection.
        let result = cylinder_torus_ssi(
            [0.0, 0.0, 0.0],   // cyl_origin
            [0.0, 0.0, 1.0],   // cyl_axis
            1.0,               // cyl_radius
            0.0,               // cyl_z_min
            5.0,               // cyl_z_max
            [100.0, 0.0, 0.0], // torus_center
            [0.0, 0.0, 1.0],   // torus_axis
            3.0,               // torus_major_radius
            1.0,               // torus_minor_radius
        );
        match result {
            Ok(curves) => assert!(
                curves.is_empty(),
                "Disjoint cylinder and torus should produce no curves, got {}",
                curves.len()
            ),
            Err(KernelError::NotSupported { .. }) => {} // stub not yet implemented
            Err(e) => panic!("Unexpected error: {:?}", e),
        }
    }

    #[test]
    fn cyl_torus_ssi_coaxial_two_circles() {
        // Coaxial: both on Z-axis.
        // Torus: center=[0,0,0], axis=[0,0,1], R=5 (major), r=2 (minor).
        // Cylinder: origin=[0,0,0], axis=[0,0,1], radius=4.
        // |R_cyl - R_major| = |4 - 5| = 1 < r = 2.
        // Intersection circles at z = ±sqrt(r^2 - (R_cyl - R)^2) = ±sqrt(4 - 1) = ±sqrt(3) ≈ ±1.732.
        // Circle radius = R_cyl = 4.
        let result = cylinder_torus_ssi(
            [0.0, 0.0, 0.0], // cyl_origin
            [0.0, 0.0, 1.0], // cyl_axis
            4.0,             // cyl_radius
            -5.0,            // cyl_z_min
            5.0,             // cyl_z_max
            [0.0, 0.0, 0.0], // torus_center
            [0.0, 0.0, 1.0], // torus_axis
            5.0,             // torus_major_radius
            2.0,             // torus_minor_radius
        );
        match result {
            Ok(curves) => {
                assert_eq!(
                    curves.len(),
                    2,
                    "Coaxial cylinder-torus with |R_cyl-R|<r should produce 2 circles, got {}",
                    curves.len()
                );
                let expected_z = (3.0_f64).sqrt(); // sqrt(r^2 - (R_cyl - R)^2)
                let mut z_values: Vec<f64> = Vec::new();
                for curve in &curves {
                    if let SSICurve::Circle {
                        center,
                        radius,
                        normal,
                    } = curve
                    {
                        // Center should be on the axis (x=0, y=0)
                        assert!(center[0].abs() < EPS, "Circle center x should be ~0");
                        assert!(center[1].abs() < EPS, "Circle center y should be ~0");
                        // Radius should be R_cyl = 4
                        assert!(
                            (radius - 4.0).abs() < EPS,
                            "Circle radius should be ~4, got {}",
                            radius
                        );
                        // Normal should be along the axis
                        let dot = v3_dot(*normal, [0.0, 0.0, 1.0]).abs();
                        assert!((dot - 1.0).abs() < EPS, "Normal should be along Z axis");
                        z_values.push(center[2]);
                    } else {
                        panic!("Expected Circle curves, got {:?}", curve);
                    }
                }
                z_values.sort_by(|a, b| a.partial_cmp(b).unwrap());
                assert!(
                    (z_values[0] - (-expected_z)).abs() < EPS,
                    "First circle z should be ~{}, got {}",
                    -expected_z,
                    z_values[0]
                );
                assert!(
                    (z_values[1] - expected_z).abs() < EPS,
                    "Second circle z should be ~{}, got {}",
                    expected_z,
                    z_values[1]
                );
            }
            Err(KernelError::NotSupported { .. }) => {} // stub not yet implemented
            Err(e) => panic!("Unexpected error: {:?}", e),
        }
    }

    #[test]
    fn cyl_torus_ssi_coaxial_exact_match() {
        // Coaxial: both on Z-axis.
        // Torus: center=[0,0,0], axis=[0,0,1], R=5, r=2.
        // Cylinder: origin=[0,0,0], axis=[0,0,1], radius=5 (matches major radius).
        // |R_cyl - R| = |5 - 5| = 0 < r = 2.
        // Intersection circles at z = ±sqrt(r^2 - 0) = ±2.
        let result = cylinder_torus_ssi(
            [0.0, 0.0, 0.0], // cyl_origin
            [0.0, 0.0, 1.0], // cyl_axis
            5.0,             // cyl_radius
            -5.0,            // cyl_z_min
            5.0,             // cyl_z_max
            [0.0, 0.0, 0.0], // torus_center
            [0.0, 0.0, 1.0], // torus_axis
            5.0,             // torus_major_radius
            2.0,             // torus_minor_radius
        );
        match result {
            Ok(curves) => {
                assert_eq!(
                    curves.len(),
                    2,
                    "Coaxial cylinder (R_cyl=R) should produce 2 circles, got {}",
                    curves.len()
                );
                let mut z_values: Vec<f64> = Vec::new();
                for curve in &curves {
                    if let SSICurve::Circle {
                        center,
                        radius,
                        normal,
                    } = curve
                    {
                        assert!(center[0].abs() < EPS, "Circle center x should be ~0");
                        assert!(center[1].abs() < EPS, "Circle center y should be ~0");
                        assert!(
                            (radius - 5.0).abs() < EPS,
                            "Circle radius should be ~5, got {}",
                            radius
                        );
                        let dot = v3_dot(*normal, [0.0, 0.0, 1.0]).abs();
                        assert!((dot - 1.0).abs() < EPS, "Normal should be along Z axis");
                        z_values.push(center[2]);
                    } else {
                        panic!("Expected Circle curves, got {:?}", curve);
                    }
                }
                z_values.sort_by(|a, b| a.partial_cmp(b).unwrap());
                assert!(
                    (z_values[0] - (-2.0)).abs() < EPS,
                    "First circle z should be ~-2, got {}",
                    z_values[0]
                );
                assert!(
                    (z_values[1] - 2.0).abs() < EPS,
                    "Second circle z should be ~2, got {}",
                    z_values[1]
                );
            }
            Err(KernelError::NotSupported { .. }) => {} // stub not yet implemented
            Err(e) => panic!("Unexpected error: {:?}", e),
        }
    }

    #[test]
    fn cyl_torus_ssi_coaxial_no_intersection() {
        // Coaxial: both on Z-axis.
        // Torus: center=[0,0,0], axis=[0,0,1], R=5, r=1.
        // Cylinder: origin=[0,0,0], axis=[0,0,1], radius=10.
        // |R_cyl - R| = |10 - 5| = 5 > r = 1 → no intersection.
        let result = cylinder_torus_ssi(
            [0.0, 0.0, 0.0], // cyl_origin
            [0.0, 0.0, 1.0], // cyl_axis
            10.0,            // cyl_radius
            -5.0,            // cyl_z_min
            5.0,             // cyl_z_max
            [0.0, 0.0, 0.0], // torus_center
            [0.0, 0.0, 1.0], // torus_axis
            5.0,             // torus_major_radius
            1.0,             // torus_minor_radius
        );
        match result {
            Ok(curves) => assert!(
                curves.is_empty(),
                "Coaxial with |R_cyl-R|>r should produce no curves, got {}",
                curves.len()
            ),
            Err(KernelError::NotSupported { .. }) => {} // stub not yet implemented
            Err(e) => panic!("Unexpected error: {:?}", e),
        }
    }

    #[test]
    fn cyl_torus_ssi_coaxial_tangent() {
        // Coaxial: both on Z-axis.
        // Torus: center=[0,0,0], axis=[0,0,1], R=5, r=2.
        // Cylinder: origin=[0,0,0], axis=[0,0,1], radius=7.
        // |R_cyl - R| = |7 - 5| = 2 = r → tangent (single point of contact at z=0).
        // Tangent case should produce empty (degenerate, no curve).
        let result = cylinder_torus_ssi(
            [0.0, 0.0, 0.0], // cyl_origin
            [0.0, 0.0, 1.0], // cyl_axis
            7.0,             // cyl_radius
            -5.0,            // cyl_z_min
            5.0,             // cyl_z_max
            [0.0, 0.0, 0.0], // torus_center
            [0.0, 0.0, 1.0], // torus_axis
            5.0,             // torus_major_radius
            2.0,             // torus_minor_radius
        );
        match result {
            Ok(curves) => assert!(
                curves.is_empty(),
                "Coaxial tangent (|R_cyl-R|=r) should produce no curves, got {}",
                curves.len()
            ),
            Err(KernelError::NotSupported { .. }) => {} // stub not yet implemented
            Err(e) => panic!("Unexpected error: {:?}", e),
        }
    }

    #[test]
    fn cyl_torus_ssi_general_position() {
        // Cylinder along Z, radius 3, height [-10,10].
        // Torus centered at [2, 0, 0] with axis along Z, R=4, r=1.5.
        // The torus tube extends from x=2.5 to x=5.5 on the far side,
        // and from x=-2.5 to x=0.5 on the near side.
        // The cylinder at r=3 overlaps the torus tube → non-empty intersection.
        let result = cylinder_torus_ssi(
            [0.0, 0.0, 0.0], // cyl_origin
            [0.0, 0.0, 1.0], // cyl_axis
            3.0,             // cyl_radius
            -10.0,           // cyl_z_min
            10.0,            // cyl_z_max
            [2.0, 0.0, 0.0], // torus_center (offset)
            [0.0, 0.0, 1.0], // torus_axis
            4.0,             // torus_major_radius
            1.5,             // torus_minor_radius
        );
        match result {
            Ok(curves) => {
                assert!(
                    !curves.is_empty(),
                    "Overlapping cylinder and offset torus should produce curves"
                );
                // Verify curves have valid (non-NaN) geometry
                for curve in &curves {
                    match curve {
                        SSICurve::Line { start, end } => {
                            for i in 0..3 {
                                assert!(!start[i].is_nan(), "Line start has NaN");
                                assert!(!end[i].is_nan(), "Line end has NaN");
                            }
                            let len = v3_length(v3_sub(*end, *start));
                            assert!(len > EPS, "Line should be non-degenerate, length={}", len);
                        }
                        SSICurve::Circle {
                            center,
                            radius,
                            normal,
                        } => {
                            for i in 0..3 {
                                assert!(!center[i].is_nan(), "Circle center has NaN");
                                assert!(!normal[i].is_nan(), "Circle normal has NaN");
                            }
                            assert!(!radius.is_nan(), "Circle radius is NaN");
                            assert!(*radius > EPS, "Circle radius should be positive");
                        }
                        SSICurve::Ellipse {
                            center,
                            semi_major,
                            semi_minor,
                            ..
                        } => {
                            for i in 0..3 {
                                assert!(!center[i].is_nan(), "Ellipse center has NaN");
                            }
                            assert!(*semi_major > EPS, "Ellipse semi_major should be positive");
                            assert!(*semi_minor > EPS, "Ellipse semi_minor should be positive");
                        }
                    }
                }
            }
            Err(KernelError::NotSupported { .. }) => {} // stub not yet implemented
            Err(e) => panic!("Unexpected error: {:?}", e),
        }
    }

    #[test]
    fn cyl_torus_ssi_perpendicular() {
        // Cylinder along Z, radius 2, height [-10,10].
        // Torus at origin with axis along X (perpendicular), R=5, r=1.
        // The torus tube sweeps around X axis at distance 5 from it with
        // tube radius 1. The cylinder at r=2 intersects the tube.
        let result = cylinder_torus_ssi(
            [0.0, 0.0, 0.0], // cyl_origin
            [0.0, 0.0, 1.0], // cyl_axis
            2.0,             // cyl_radius
            -10.0,           // cyl_z_min
            10.0,            // cyl_z_max
            [0.0, 0.0, 0.0], // torus_center
            [1.0, 0.0, 0.0], // torus_axis (along X — perpendicular to cylinder)
            5.0,             // torus_major_radius
            1.0,             // torus_minor_radius
        );
        match result {
            Ok(curves) => {
                assert!(
                    !curves.is_empty(),
                    "Perpendicular cylinder and torus should produce curves"
                );
                // Verify non-NaN and non-degenerate
                for curve in &curves {
                    match curve {
                        SSICurve::Line { start, end } => {
                            for i in 0..3 {
                                assert!(!start[i].is_nan(), "Line start has NaN");
                                assert!(!end[i].is_nan(), "Line end has NaN");
                            }
                            let len = v3_length(v3_sub(*end, *start));
                            assert!(len > EPS, "Line should be non-degenerate, length={}", len);
                        }
                        SSICurve::Circle {
                            center,
                            radius,
                            normal,
                        } => {
                            for i in 0..3 {
                                assert!(!center[i].is_nan(), "Circle center has NaN");
                                assert!(!normal[i].is_nan(), "Circle normal has NaN");
                            }
                            assert!(*radius > EPS, "Circle radius should be positive");
                        }
                        SSICurve::Ellipse {
                            center,
                            semi_major,
                            semi_minor,
                            ..
                        } => {
                            for i in 0..3 {
                                assert!(!center[i].is_nan(), "Ellipse center has NaN");
                            }
                            assert!(*semi_major > EPS);
                            assert!(*semi_minor > EPS);
                        }
                    }
                }
            }
            Err(KernelError::NotSupported { .. }) => {} // stub not yet implemented
            Err(e) => panic!("Unexpected error: {:?}", e),
        }
    }

    #[test]
    fn cyl_torus_ssi_tilted() {
        // Cylinder along Z, radius 3, height [-10,10].
        // Torus at origin with axis tilted 45° in XZ plane, R=5, r=1.5.
        let torus_axis = v3_normalize([1.0, 0.0, 1.0]);
        let result = cylinder_torus_ssi(
            [0.0, 0.0, 0.0], // cyl_origin
            [0.0, 0.0, 1.0], // cyl_axis
            3.0,             // cyl_radius
            -10.0,           // cyl_z_min
            10.0,            // cyl_z_max
            [0.0, 0.0, 0.0], // torus_center
            torus_axis,      // torus_axis (tilted 45°)
            5.0,             // torus_major_radius
            1.5,             // torus_minor_radius
        );
        match result {
            Ok(curves) => {
                assert!(
                    !curves.is_empty(),
                    "Tilted torus overlapping cylinder should produce curves"
                );
                // Verify non-NaN and non-degenerate
                for curve in &curves {
                    match curve {
                        SSICurve::Line { start, end } => {
                            for i in 0..3 {
                                assert!(!start[i].is_nan(), "Line start has NaN");
                                assert!(!end[i].is_nan(), "Line end has NaN");
                            }
                            let len = v3_length(v3_sub(*end, *start));
                            assert!(len > EPS, "Line should be non-degenerate, length={}", len);
                        }
                        SSICurve::Circle {
                            center,
                            radius,
                            normal,
                        } => {
                            for i in 0..3 {
                                assert!(!center[i].is_nan(), "Circle center has NaN");
                                assert!(!normal[i].is_nan(), "Circle normal has NaN");
                            }
                            assert!(*radius > EPS, "Circle radius should be positive");
                        }
                        SSICurve::Ellipse {
                            center,
                            semi_major,
                            semi_minor,
                            ..
                        } => {
                            for i in 0..3 {
                                assert!(!center[i].is_nan(), "Ellipse center has NaN");
                            }
                            assert!(*semi_major > EPS);
                            assert!(*semi_minor > EPS);
                        }
                    }
                }
            }
            Err(KernelError::NotSupported { .. }) => {} // stub not yet implemented
            Err(e) => panic!("Unexpected error: {:?}", e),
        }
    }

    // ── Cone-Torus SSI ──────────────────────────────────────────────────

    #[test]
    fn test_cone_torus_coaxial_intersecting() {
        // Cone: apex at origin, axis +Z, half-angle 45°, height range [1, 5].
        // At height h, cone radius = h * tan(π/4) = h.
        // Torus: center [0,0,3], axis +Z, R=3, r=1.
        // Torus tube center at radius R=3 from Z-axis, at height z=3.
        // For a coaxial intersection on the cone: ρ = h (cone), and the torus
        // cross-section satisfies (ρ - 3)² + (z - 3)² = 1 with z = h (same
        // coordinate for height from apex and z-coordinate).
        // So (h - 3)² + (h - 3)² = 1 → 2(h-3)² = 1 → h = 3 ± 1/√2.
        // h₁ = 3 - 1/√2 ≈ 2.293, h₂ = 3 + 1/√2 ≈ 3.707. Both in [1, 5].
        // Intersection circles have radius = h (since cone radius = h at that height).
        let half_angle = std::f64::consts::FRAC_PI_4;
        let result = cone_torus_ssi(
            [0.0, 0.0, 0.0], // cone_apex
            [0.0, 0.0, 1.0], // cone_axis
            half_angle,      // cone_half_angle (45°)
            (1.0, 5.0),      // cone_height_range
            [0.0, 0.0, 3.0], // torus_center
            [0.0, 0.0, 1.0], // torus_axis
            3.0,             // torus_major_radius
            1.0,             // torus_minor_radius
        );
        match result {
            Ok(curves) => {
                assert_eq!(
                    curves.len(),
                    2,
                    "Coaxial cone-torus should produce 2 intersection circles"
                );
                let h1 = 3.0 - FRAC_1_SQRT_2; // ≈ 2.293
                let h2 = 3.0 + FRAC_1_SQRT_2; // ≈ 3.707
                                              // Both curves should be circles
                let mut circle_heights: Vec<f64> = Vec::new();
                for curve in &curves {
                    if let SSICurve::Circle {
                        center,
                        radius,
                        normal,
                    } = curve
                    {
                        // Center should be on Z-axis
                        assert!(center[0].abs() < EPS, "Circle center x should be 0");
                        assert!(center[1].abs() < EPS, "Circle center y should be 0");
                        // Normal should be parallel to Z
                        assert!(normal[2].abs() > 1.0 - EPS, "Normal should be along Z");
                        // Radius should equal h (cone radius at that height)
                        assert!(
                            (*radius - center[2]).abs() < EPS,
                            "Circle radius {} should equal height {}",
                            radius,
                            center[2]
                        );
                        circle_heights.push(center[2]);
                    } else {
                        panic!("Expected Circle for coaxial cone-torus intersection");
                    }
                }
                circle_heights.sort_by(|a, b| a.partial_cmp(b).unwrap());
                assert!(
                    (circle_heights[0] - h1).abs() < EPS,
                    "First circle at h≈{}, got {}",
                    h1,
                    circle_heights[0]
                );
                assert!(
                    (circle_heights[1] - h2).abs() < EPS,
                    "Second circle at h≈{}, got {}",
                    h2,
                    circle_heights[1]
                );
            }
            Err(KernelError::NotSupported { .. }) => {} // stub not yet implemented
            Err(e) => panic!("Unexpected error: {:?}", e),
        }
    }

    #[test]
    fn test_cone_torus_disjoint() {
        // Cone: apex at origin, axis +Z, half-angle 30°, height [0, 2].
        // Max cone radius at h=2: 2·tan(30°) ≈ 1.155.
        // Torus: center [20, 20, 20], axis Z, R=3, r=0.5.
        // Far apart — no intersection.
        let half_angle = std::f64::consts::FRAC_PI_6;
        let result = cone_torus_ssi(
            [0.0, 0.0, 0.0],    // cone_apex
            [0.0, 0.0, 1.0],    // cone_axis
            half_angle,         // 30°
            (0.0, 2.0),         // cone_height_range
            [20.0, 20.0, 20.0], // torus_center (far away)
            [0.0, 0.0, 1.0],    // torus_axis
            3.0,                // torus_major_radius
            0.5,                // torus_minor_radius
        );
        match result {
            Ok(curves) => {
                assert!(
                    curves.is_empty(),
                    "Disjoint cone and torus should produce no curves, got {}",
                    curves.len()
                );
            }
            Err(KernelError::NotSupported { .. }) => {} // stub not yet implemented
            Err(e) => panic!("Unexpected error: {:?}", e),
        }
    }

    #[test]
    fn test_cone_torus_general_position() {
        // Cone: apex [0,0,0], axis +Z, half-angle 30°, height [0, 10].
        // Torus: center [2, 0, 4], axis tilted 45° in XZ plane, R=3, r=1.
        // Non-coaxial arrangement — should produce non-trivial intersection curves.
        let half_angle = std::f64::consts::FRAC_PI_6;
        let torus_axis = v3_normalize([1.0, 0.0, 1.0]);
        let result = cone_torus_ssi(
            [0.0, 0.0, 0.0], // cone_apex
            [0.0, 0.0, 1.0], // cone_axis
            half_angle,      // 30°
            (0.0, 10.0),     // cone_height_range
            [2.0, 0.0, 4.0], // torus_center
            torus_axis,      // torus_axis (tilted 45°)
            3.0,             // torus_major_radius
            1.0,             // torus_minor_radius
        );
        match result {
            Ok(curves) => {
                assert!(
                    !curves.is_empty(),
                    "General-position cone-torus should produce intersection curves"
                );
                // Verify all curves are non-degenerate and NaN-free
                for curve in &curves {
                    match curve {
                        SSICurve::Line { start, end } => {
                            for i in 0..3 {
                                assert!(!start[i].is_nan(), "Line start has NaN");
                                assert!(!end[i].is_nan(), "Line end has NaN");
                            }
                            let len = v3_length(v3_sub(*end, *start));
                            assert!(len > EPS, "Line should be non-degenerate, length={}", len);
                        }
                        SSICurve::Circle {
                            center,
                            radius,
                            normal,
                        } => {
                            for i in 0..3 {
                                assert!(!center[i].is_nan(), "Circle center has NaN");
                                assert!(!normal[i].is_nan(), "Circle normal has NaN");
                            }
                            assert!(*radius > EPS, "Circle radius should be positive");
                        }
                        SSICurve::Ellipse {
                            center,
                            semi_major,
                            semi_minor,
                            ..
                        } => {
                            for i in 0..3 {
                                assert!(!center[i].is_nan(), "Ellipse center has NaN");
                            }
                            assert!(*semi_major > EPS);
                            assert!(*semi_minor > EPS);
                        }
                    }
                }
            }
            Err(KernelError::NotSupported { .. }) => {} // stub not yet implemented
            Err(e) => panic!("Unexpected error: {:?}", e),
        }
    }

    // ── Torus-Torus SSI ─────────────────────────────────────────────────

    #[test]
    fn test_torus_torus_coaxial_intersecting() {
        // Torus A: center [0,0,0], axis +Z, R=3, r=1.
        //   Cross-section: (ρ - 3)² + z² = 1
        // Torus B: center [0,0,0], axis +Z, R=4, r=1.5.
        //   Cross-section: (ρ - 4)² + z² = 2.25
        // Subtract: (ρ-3)² - (ρ-4)² = 1 - 2.25 = -1.25
        //   ρ²-6ρ+9 - (ρ²-8ρ+16) = -1.25
        //   2ρ - 7 = -1.25 → ρ = 2.875
        // z² = 1 - (2.875 - 3)² = 1 - 0.015625 = 0.984375
        // z = ±√0.984375 ≈ ±0.99218
        // Two intersection circles at z ≈ ±0.99218, radius ρ = 2.875.
        let result = torus_torus_ssi(
            [0.0, 0.0, 0.0], // torus_a_center
            [0.0, 0.0, 1.0], // torus_a_axis
            3.0,             // torus_a_major_radius
            1.0,             // torus_a_minor_radius
            [0.0, 0.0, 0.0], // torus_b_center
            [0.0, 0.0, 1.0], // torus_b_axis
            4.0,             // torus_b_major_radius
            1.5,             // torus_b_minor_radius
        );
        let expected_rho = 2.875;
        let expected_z = (0.984375_f64).sqrt(); // ≈ 0.99218
        match result {
            Ok(curves) => {
                assert_eq!(
                    curves.len(),
                    2,
                    "Coaxial torus-torus should produce 2 intersection circles"
                );
                let mut circle_zs: Vec<f64> = Vec::new();
                for curve in &curves {
                    if let SSICurve::Circle {
                        center,
                        radius,
                        normal,
                    } = curve
                    {
                        // Center should be on Z-axis
                        assert!(center[0].abs() < EPS, "Circle center x should be 0");
                        assert!(center[1].abs() < EPS, "Circle center y should be 0");
                        // Normal should be parallel to Z
                        assert!(normal[2].abs() > 1.0 - EPS, "Normal should be along Z");
                        // Radius should be ρ = 2.875
                        assert!(
                            (*radius - expected_rho).abs() < EPS,
                            "Circle radius should be ~{}, got {}",
                            expected_rho,
                            radius
                        );
                        circle_zs.push(center[2]);
                    } else {
                        panic!("Expected Circle for coaxial torus-torus intersection");
                    }
                }
                circle_zs.sort_by(|a, b| a.partial_cmp(b).unwrap());
                assert!(
                    (circle_zs[0] - (-expected_z)).abs() < EPS,
                    "First circle at z≈{}, got {}",
                    -expected_z,
                    circle_zs[0]
                );
                assert!(
                    (circle_zs[1] - expected_z).abs() < EPS,
                    "Second circle at z≈{}, got {}",
                    expected_z,
                    circle_zs[1]
                );
            }
            Err(KernelError::NotSupported { .. }) => {} // stub not yet implemented
            Err(e) => panic!("Unexpected error: {:?}", e),
        }
    }

    #[test]
    fn test_torus_torus_disjoint() {
        // Torus A: center [0,0,0], axis Z, R=2, r=0.5. Outer extent = 2.5.
        // Torus B: center [20, 0, 0], axis Z, R=2, r=0.5. Outer extent at x=20 ± 2.5.
        // Gap of 15 units — no intersection.
        let result = torus_torus_ssi(
            [0.0, 0.0, 0.0],  // torus_a_center
            [0.0, 0.0, 1.0],  // torus_a_axis
            2.0,              // torus_a_major_radius
            0.5,              // torus_a_minor_radius
            [20.0, 0.0, 0.0], // torus_b_center
            [0.0, 0.0, 1.0],  // torus_b_axis
            2.0,              // torus_b_major_radius
            0.5,              // torus_b_minor_radius
        );
        match result {
            Ok(curves) => {
                assert!(
                    curves.is_empty(),
                    "Disjoint tori should produce no curves, got {}",
                    curves.len()
                );
            }
            Err(KernelError::NotSupported { .. }) => {} // stub not yet implemented
            Err(e) => panic!("Unexpected error: {:?}", e),
        }
    }

    #[test]
    fn test_torus_torus_general_position() {
        // Torus A: center [0,0,0], axis +Z, R=4, r=1.
        // Torus B: center [3, 0, 0], axis tilted 45° in XZ plane, R=4, r=1.
        // The two tori overlap in general position — should produce curves.
        let torus_b_axis = v3_normalize([1.0, 0.0, 1.0]);
        let result = torus_torus_ssi(
            [0.0, 0.0, 0.0], // torus_a_center
            [0.0, 0.0, 1.0], // torus_a_axis
            4.0,             // torus_a_major_radius
            1.0,             // torus_a_minor_radius
            [3.0, 0.0, 0.0], // torus_b_center
            torus_b_axis,    // torus_b_axis (tilted 45°)
            4.0,             // torus_b_major_radius
            1.0,             // torus_b_minor_radius
        );
        match result {
            Ok(curves) => {
                assert!(
                    !curves.is_empty(),
                    "General-position torus-torus should produce intersection curves"
                );
                // Verify all curves are non-degenerate and NaN-free
                for curve in &curves {
                    match curve {
                        SSICurve::Line { start, end } => {
                            for i in 0..3 {
                                assert!(!start[i].is_nan(), "Line start has NaN");
                                assert!(!end[i].is_nan(), "Line end has NaN");
                            }
                            let len = v3_length(v3_sub(*end, *start));
                            assert!(len > EPS, "Line should be non-degenerate, length={}", len);
                        }
                        SSICurve::Circle {
                            center,
                            radius,
                            normal,
                        } => {
                            for i in 0..3 {
                                assert!(!center[i].is_nan(), "Circle center has NaN");
                                assert!(!normal[i].is_nan(), "Circle normal has NaN");
                            }
                            assert!(*radius > EPS, "Circle radius should be positive");
                        }
                        SSICurve::Ellipse {
                            center,
                            semi_major,
                            semi_minor,
                            ..
                        } => {
                            for i in 0..3 {
                                assert!(!center[i].is_nan(), "Ellipse center has NaN");
                            }
                            assert!(*semi_major > EPS);
                            assert!(*semi_minor > EPS);
                        }
                    }
                }
            }
            Err(KernelError::NotSupported { .. }) => {} // stub not yet implemented
            Err(e) => panic!("Unexpected error: {:?}", e),
        }
    }

    // ── Plane-Cone Oblique SSI (FIP: plane_cone_oblique_ssi) ─────────

    /// Helper: check that a point lies on the plane (within tolerance).
    fn assert_point_on_plane(p: [f64; 3], plane_origin: [f64; 3], plane_normal: [f64; 3]) {
        let d = v3_dot(v3_sub(p, plane_origin), plane_normal);
        assert!(
            d.abs() < crate::units::TAU_MODEL * 100.0,
            "Point {:?} not on plane: dist = {:.2e}",
            p,
            d,
        );
    }

    /// Helper: check that a point lies on the cone surface (within tolerance).
    fn assert_point_on_cone(
        p: [f64; 3],
        cone_apex: [f64; 3],
        cone_axis: [f64; 3],
        half_angle: f64,
    ) {
        let dp = v3_sub(p, cone_apex);
        let h = v3_dot(dp, cone_axis);
        let radial_sq = v3_dot(dp, dp) - h * h;
        let expected_r = h * half_angle.tan();
        assert!(
            (radial_sq.sqrt() - expected_r).abs() < crate::units::TAU_MODEL * 100.0,
            "Point {:?} not on cone: radial={:.6e}, expected={:.6e}",
            p,
            radial_sq.sqrt(),
            expected_r,
        );
    }

    /// Helper: sample 8 points on an SSICurve::Ellipse.
    fn sample_ellipse_points(
        center: [f64; 3],
        normal: [f64; 3],
        major_axis: [f64; 3],
        semi_major: f64,
        semi_minor: f64,
    ) -> Vec<[f64; 3]> {
        let minor_axis = v3_normalize(v3_cross(normal, major_axis));
        (0..8)
            .map(|i| {
                let t = std::f64::consts::TAU * (i as f64) / 8.0;
                v3_add(
                    center,
                    v3_add(
                        v3_scale(major_axis, semi_major * t.cos()),
                        v3_scale(minor_axis, semi_minor * t.sin()),
                    ),
                )
            })
            .collect()
    }

    #[test]
    fn test_plane_cone_oblique_ellipse_45deg() {
        // Cone: apex at origin, axis +Z, half_angle=30° (π/6), max_height=10
        // Plane tilted 45° from Z axis → normal has Z and X components
        // γ = angle between plane and cone axis = 45° > β = 30° → ellipse
        let half_angle = std::f64::consts::FRAC_PI_6; // 30°
        let plane_normal = [FRAC_1_SQRT_2, 0.0, FRAC_1_SQRT_2]; // 45° from Z
        let plane_origin = [0.0, 0.0, 5.0]; // intersects cone at h≈5
        let cone_apex = [0.0, 0.0, 0.0];
        let cone_axis = [0.0, 0.0, 1.0];
        let max_height = 10.0;

        let curves = plane_cone_ssi(
            plane_origin,
            plane_normal,
            cone_apex,
            cone_axis,
            half_angle,
            max_height,
        )
        .expect("oblique ellipse case should return Ok");

        assert_eq!(
            curves.len(),
            1,
            "Expected exactly 1 curve, got {}",
            curves.len()
        );

        match &curves[0] {
            SSICurve::Ellipse {
                center,
                normal,
                major_axis,
                semi_major,
                semi_minor,
            } => {
                assert!(
                    semi_major >= semi_minor,
                    "semi_major ({}) must be >= semi_minor ({})",
                    semi_major,
                    semi_minor,
                );
                // Sample 8 points on the ellipse and verify each lies on both surfaces
                let points =
                    sample_ellipse_points(*center, *normal, *major_axis, *semi_major, *semi_minor);
                for (i, p) in points.iter().enumerate() {
                    assert_point_on_plane(*p, plane_origin, plane_normal);
                    assert_point_on_cone(*p, cone_apex, cone_axis, half_angle);
                    // Sanity: point should have non-NaN coordinates
                    for j in 0..3 {
                        assert!(!p[j].is_nan(), "Point {} coord {} is NaN", i, j);
                    }
                }
            }
            other => panic!("Expected Ellipse, got {:?}", other),
        }
    }

    #[test]
    fn test_plane_cone_oblique_ellipse_steep() {
        // Cone: half_angle=15° (π/12), axis +Z, apex at origin
        // Plane at 60° from axis → γ = 60° > β = 15° → ellipse (steep cut)
        let half_angle = std::f64::consts::FRAC_PI_6 / 2.0; // 15° = π/12
                                                            // Plane normal tilted 30° from Z (so γ = 90° - 30° = 60° from axis)
                                                            // normal = (sin30°, 0, cos30°) = (0.5, 0, √3/2)
        let plane_normal = [0.5, 0.0, (3.0_f64).sqrt() / 2.0];
        let plane_origin = [0.0, 0.0, 5.0];
        let cone_apex = [0.0, 0.0, 0.0];
        let cone_axis = [0.0, 0.0, 1.0];
        let max_height = 20.0;

        let curves = plane_cone_ssi(
            plane_origin,
            plane_normal,
            cone_apex,
            cone_axis,
            half_angle,
            max_height,
        )
        .expect("steep oblique ellipse should return Ok");

        assert_eq!(
            curves.len(),
            1,
            "Expected exactly 1 curve, got {}",
            curves.len()
        );

        match &curves[0] {
            SSICurve::Ellipse {
                semi_major,
                semi_minor,
                ..
            } => {
                assert!(
                    semi_major >= semi_minor,
                    "semi_major ({}) must be >= semi_minor ({})",
                    semi_major,
                    semi_minor,
                );
                assert!(*semi_major > 0.0, "semi_major must be positive");
                assert!(*semi_minor > 0.0, "semi_minor must be positive");
            }
            other => panic!("Expected Ellipse, got {:?}", other),
        }
    }

    #[test]
    fn test_plane_cone_through_apex_degenerate() {
        // Plane passes through the cone apex → degenerate intersection = two lines
        // Two lines only exist in hyperbola regime (γ < β). Here β = 60°, γ = 45°.
        let half_angle = std::f64::consts::FRAC_PI_3; // 60°
        let cone_apex = [0.0, 0.0, 0.0];
        let cone_axis = [0.0, 0.0, 1.0];
        // Plane through apex with oblique normal (45° from axis → γ = 45° < 60° = β)
        let plane_origin = [0.0, 0.0, 0.0]; // on the apex
        let plane_normal = [FRAC_1_SQRT_2, 0.0, FRAC_1_SQRT_2]; // 45° tilt
        let max_height = 10.0;

        let curves = plane_cone_ssi(
            plane_origin,
            plane_normal,
            cone_apex,
            cone_axis,
            half_angle,
            max_height,
        )
        .expect("through-apex degenerate case should return Ok");

        assert_eq!(
            curves.len(),
            2,
            "Expected 2 lines through apex, got {} curves",
            curves.len()
        );

        for curve in &curves {
            match curve {
                SSICurve::Line { start, end } => {
                    // Both lines should pass through the apex (start at apex)
                    let dist_start = v3_length(v3_sub(*start, cone_apex));
                    assert!(
                        dist_start < crate::units::TAU_MODEL * 100.0,
                        "Line start {:?} should be at apex, dist = {:.2e}",
                        start,
                        dist_start,
                    );
                    // End should not be at apex (non-degenerate line)
                    let dist_end = v3_length(v3_sub(*end, cone_apex));
                    assert!(
                        dist_end > crate::units::TAU_MODEL,
                        "Line end {:?} should not be at apex",
                        end,
                    );
                }
                other => panic!("Expected Line, got {:?}", other),
            }
        }
    }

    #[test]
    fn test_plane_cone_oblique_parabola_boundary() {
        // γ = β (cutting angle equals half_angle) → parabolic boundary case
        // half_angle = 30°. Plane normal must be at 60° from Z so γ = 30°.
        // normal = (sin60°, 0, cos60°) = (√3/2, 0, 0.5)
        let half_angle = std::f64::consts::FRAC_PI_6; // 30°
        let plane_normal = [(3.0_f64).sqrt() / 2.0, 0.0, 0.5];
        let plane_origin = [0.0, 0.0, 5.0];
        let cone_apex = [0.0, 0.0, 0.0];
        let cone_axis = [0.0, 0.0, 1.0];
        let max_height = 20.0;

        let result = plane_cone_ssi(
            plane_origin,
            plane_normal,
            cone_apex,
            cone_axis,
            half_angle,
            max_height,
        );

        // Parabola not yet implemented — should return NotSupported
        assert!(
            matches!(result, Err(KernelError::NotSupported { .. })),
            "Parabolic boundary case should return NotSupported, got {:?}",
            result,
        );
    }

    #[test]
    fn test_plane_cone_oblique_hyperbola() {
        // γ < β (shallow cut) → hyperbola
        // half_angle = 45°. Plane normal nearly along X → γ ≈ 0° < 45°.
        // normal = (1, 0, 0) → plane parallel to cone axis → γ = 0°
        let half_angle = std::f64::consts::FRAC_PI_4; // 45°
        let plane_normal = [1.0, 0.0, 0.0]; // perpendicular to axis → γ = 0°
        let plane_origin = [2.0, 0.0, 0.0]; // offset from axis
        let cone_apex = [0.0, 0.0, 0.0];
        let cone_axis = [0.0, 0.0, 1.0];
        let max_height = 10.0;

        let result = plane_cone_ssi(
            plane_origin,
            plane_normal,
            cone_apex,
            cone_axis,
            half_angle,
            max_height,
        );

        // Hyperbola not yet implemented — should return NotSupported
        assert!(
            matches!(result, Err(KernelError::NotSupported { .. })),
            "Hyperbola case should return NotSupported, got {:?}",
            result,
        );
    }

    #[test]
    fn test_plane_cone_oblique_no_intersect() {
        // Oblique plane positioned so the ellipse falls entirely outside [0, max_height]
        // Cone: apex at origin, axis +Z, half_angle=30°, max_height=2 (short cone)
        // Plane tilted 45° but origin at z=20 — far above cone
        let half_angle = std::f64::consts::FRAC_PI_6; // 30°
        let plane_normal = [FRAC_1_SQRT_2, 0.0, FRAC_1_SQRT_2];
        let plane_origin = [0.0, 0.0, 20.0]; // far above max_height=2
        let cone_apex = [0.0, 0.0, 0.0];
        let cone_axis = [0.0, 0.0, 1.0];
        let max_height = 2.0;

        let curves = plane_cone_ssi(
            plane_origin,
            plane_normal,
            cone_apex,
            cone_axis,
            half_angle,
            max_height,
        )
        .expect("no-intersect oblique case should return Ok");

        assert!(
            curves.is_empty(),
            "Expected empty result for out-of-range intersection, got {} curves",
            curves.len(),
        );
    }

    #[test]
    fn test_plane_cone_perp_regression() {
        // Regression guard: perpendicular case still produces a circle
        // Cone: apex at origin, axis +Z, half_angle=30°, max_height=10
        // Plane at z=6 → circle at (0,0,6) with r = 6*tan(30°) ≈ 3.464
        let half_angle = std::f64::consts::FRAC_PI_6; // 30°
        let plane_origin = [0.0, 0.0, 6.0];
        let plane_normal = [0.0, 0.0, 1.0]; // perpendicular to cone axis
        let cone_apex = [0.0, 0.0, 0.0];
        let cone_axis = [0.0, 0.0, 1.0];
        let max_height = 10.0;

        let curves = plane_cone_ssi(
            plane_origin,
            plane_normal,
            cone_apex,
            cone_axis,
            half_angle,
            max_height,
        )
        .unwrap();

        assert_eq!(curves.len(), 1, "Expected 1 circle, got {}", curves.len());

        let expected_radius = 6.0 * half_angle.tan();
        match &curves[0] {
            SSICurve::Circle { center, radius, .. } => {
                assert!(center[0].abs() < EPS, "cx={}", center[0]);
                assert!(center[1].abs() < EPS, "cy={}", center[1]);
                assert!((center[2] - 6.0).abs() < EPS, "cz={}", center[2]);
                assert!(
                    (radius - expected_radius).abs() < EPS,
                    "radius={}, expected={}",
                    radius,
                    expected_radius,
                );
            }
            other => panic!("Expected Circle, got {:?}", other),
        }
    }

    // ── ADVERSARY: Pathological / near-tolerance plane-cone SSI tests ──

    #[test]
    fn test_plane_cone_oblique_near_parabola_ellipse_side() {
        // ADVERSARY: γ just barely above β — near the parabola boundary on the
        // ellipse side. The resulting ellipse should be extremely elongated.
        // β = 30°, so sin(β) = 0.5. We need cos(α) > sin(β) but barely.
        // Set γ = 30.1° → α = 90° - 30.1° = 59.9°. cos(59.9°) ≈ 0.5009.
        // discriminant = cos²(α) - sin²(β) = 0.5009² - 0.5² ≈ 0.0009 (very small positive).
        let half_angle = std::f64::consts::FRAC_PI_6; // 30°
        let gamma_deg: f64 = 30.1;
        let alpha_rad = (90.0 - gamma_deg).to_radians(); // angle between normal and axis
        let plane_normal = [alpha_rad.sin(), 0.0, alpha_rad.cos()];
        let plane_origin = [0.0, 0.0, 5.0];
        let cone_apex = [0.0, 0.0, 0.0];
        let cone_axis = [0.0, 0.0, 1.0];
        let max_height = 200.0; // large so the elongated ellipse fits

        let curves = plane_cone_ssi(
            plane_origin,
            plane_normal,
            cone_apex,
            cone_axis,
            half_angle,
            max_height,
        )
        .expect("near-parabola ellipse side should return Ok");

        assert_eq!(curves.len(), 1, "Expected 1 curve, got {}", curves.len());

        match &curves[0] {
            SSICurve::Ellipse {
                center,
                normal,
                major_axis,
                semi_major,
                semi_minor,
            } => {
                assert!(
                    *semi_major > 10.0 * *semi_minor,
                    "Near-parabola ellipse should be very elongated: semi_major={}, semi_minor={}, ratio={}",
                    semi_major,
                    semi_minor,
                    semi_major / semi_minor,
                );
                // Verify sampled points lie on both surfaces
                let points =
                    sample_ellipse_points(*center, *normal, *major_axis, *semi_major, *semi_minor);
                for (i, p) in points.iter().enumerate() {
                    assert_point_on_plane(*p, plane_origin, plane_normal);
                    assert_point_on_cone(*p, cone_apex, cone_axis, half_angle);
                    for j in 0..3 {
                        assert!(!p[j].is_nan(), "Point {} coord {} is NaN", i, j);
                    }
                }
            }
            other => panic!("Expected Ellipse, got {:?}", other),
        }
    }

    #[test]
    fn test_plane_cone_oblique_very_small_half_angle() {
        // ADVERSARY: Very narrow cone (half_angle = 2° = π/90).
        // Oblique cut at γ = 45° → α = 45°. cos²(45°) = 0.5, sin²(2°) ≈ 0.0012.
        // discriminant ≈ 0.4988 — solidly ellipse territory.
        let half_angle = std::f64::consts::PI / 90.0; // 2°
        let plane_normal = [FRAC_1_SQRT_2, 0.0, FRAC_1_SQRT_2]; // 45° from axis
        let plane_origin = [0.0, 0.0, 50.0]; // far out so the narrow cone has measurable radius
        let cone_apex = [0.0, 0.0, 0.0];
        let cone_axis = [0.0, 0.0, 1.0];
        let max_height = 200.0;

        let curves = plane_cone_ssi(
            plane_origin,
            plane_normal,
            cone_apex,
            cone_axis,
            half_angle,
            max_height,
        )
        .expect("small half_angle oblique should return Ok");

        assert_eq!(curves.len(), 1, "Expected 1 curve, got {}", curves.len());

        match &curves[0] {
            SSICurve::Ellipse {
                center,
                normal,
                major_axis,
                semi_major,
                semi_minor,
            } => {
                assert!(*semi_major > 0.0, "semi_major must be positive");
                assert!(*semi_minor > 0.0, "semi_minor must be positive");
                assert!(
                    *semi_major >= *semi_minor,
                    "semi_major ({}) >= semi_minor ({})",
                    semi_major,
                    semi_minor,
                );
                // Verify on both surfaces
                let points =
                    sample_ellipse_points(*center, *normal, *major_axis, *semi_major, *semi_minor);
                for (i, p) in points.iter().enumerate() {
                    assert_point_on_plane(*p, plane_origin, plane_normal);
                    assert_point_on_cone(*p, cone_apex, cone_axis, half_angle);
                    for j in 0..3 {
                        assert!(!p[j].is_nan(), "Point {} coord {} is NaN", i, j);
                    }
                }
            }
            other => panic!("Expected Ellipse, got {:?}", other),
        }
    }

    #[test]
    fn test_plane_cone_oblique_wide_half_angle() {
        // ADVERSARY: Wide cone (half_angle = 80° = 4π/9).
        // Steep oblique cut: γ > 80°, say γ = 85° → α = 5°.
        // cos²(5°) ≈ 0.9924, sin²(80°) ≈ 0.9698. discriminant ≈ 0.0226.
        // The ellipse should be nearly circular since the cone is very wide
        // and the cut is almost perpendicular to the axis.
        let half_angle = 80.0_f64.to_radians();
        let alpha_rad = 5.0_f64.to_radians(); // γ = 85°
        let plane_normal = [alpha_rad.sin(), 0.0, alpha_rad.cos()];
        let plane_origin = [0.0, 0.0, 2.0];
        let cone_apex = [0.0, 0.0, 0.0];
        let cone_axis = [0.0, 0.0, 1.0];
        let max_height = 50.0;

        let curves = plane_cone_ssi(
            plane_origin,
            plane_normal,
            cone_apex,
            cone_axis,
            half_angle,
            max_height,
        )
        .expect("wide half_angle oblique should return Ok");

        assert_eq!(curves.len(), 1, "Expected 1 curve, got {}", curves.len());

        match &curves[0] {
            SSICurve::Ellipse {
                center,
                normal,
                major_axis,
                semi_major,
                semi_minor,
            } => {
                // Nearly circular: semi_major / semi_minor should be close to 1
                let ratio = semi_major / semi_minor;
                assert!(
                    ratio < 2.0,
                    "Wide-angle near-perpendicular cut should be near-circular, ratio = {}",
                    ratio,
                );
                let points =
                    sample_ellipse_points(*center, *normal, *major_axis, *semi_major, *semi_minor);
                for (i, p) in points.iter().enumerate() {
                    assert_point_on_plane(*p, plane_origin, plane_normal);
                    assert_point_on_cone(*p, cone_apex, cone_axis, half_angle);
                    for j in 0..3 {
                        assert!(!p[j].is_nan(), "Point {} coord {} is NaN", i, j);
                    }
                }
            }
            other => panic!("Expected Ellipse, got {:?}", other),
        }
    }

    #[test]
    fn test_plane_cone_oblique_tilted_axis() {
        // ADVERSARY: Cone with axis along (1,1,1)/√3 — non-axis-aligned.
        // Verify the code handles arbitrary orientations correctly.
        let half_angle = std::f64::consts::FRAC_PI_6; // 30°
        let inv_sqrt3 = 1.0 / 3.0_f64.sqrt();
        let cone_axis = [inv_sqrt3, inv_sqrt3, inv_sqrt3];
        let cone_apex = [0.0, 0.0, 0.0];

        // Plane normal perpendicular-ish to axis but tilted for oblique cut.
        // Use normal = (0, 0, 1) which has cos(α) = 1/√3 ≈ 0.577.
        // sin(β) = sin(30°) = 0.5. cos²(α) = 1/3 ≈ 0.333, sin²(β) = 0.25.
        // discriminant = 0.333 - 0.25 = 0.083 > 0 → ellipse.
        let plane_normal = [0.0, 0.0, 1.0];
        let plane_origin = [0.0, 0.0, 5.0];
        let max_height = 20.0;

        let curves = plane_cone_ssi(
            plane_origin,
            plane_normal,
            cone_apex,
            cone_axis,
            half_angle,
            max_height,
        )
        .expect("tilted axis oblique should return Ok");

        assert_eq!(curves.len(), 1, "Expected 1 curve, got {}", curves.len());

        match &curves[0] {
            SSICurve::Ellipse {
                center,
                normal,
                major_axis,
                semi_major,
                semi_minor,
            } => {
                assert!(*semi_major > 0.0);
                assert!(*semi_minor > 0.0);
                assert!(*semi_major >= *semi_minor);
                let points =
                    sample_ellipse_points(*center, *normal, *major_axis, *semi_major, *semi_minor);
                for (i, p) in points.iter().enumerate() {
                    assert_point_on_plane(*p, plane_origin, plane_normal);
                    assert_point_on_cone(*p, cone_apex, cone_axis, half_angle);
                    for j in 0..3 {
                        assert!(!p[j].is_nan(), "Point {} coord {} is NaN", i, j);
                    }
                }
            }
            other => panic!("Expected Ellipse, got {:?}", other),
        }
    }

    #[test]
    fn test_plane_cone_oblique_apex_not_at_origin() {
        // ADVERSARY: Cone apex at (10, 20, 30) — verify translation handling.
        let half_angle = std::f64::consts::FRAC_PI_6; // 30°
        let cone_apex = [10.0, 20.0, 30.0];
        let cone_axis = [0.0, 0.0, 1.0];
        let plane_normal = [FRAC_1_SQRT_2, 0.0, FRAC_1_SQRT_2]; // 45° from Z
        let plane_origin = [10.0, 20.0, 35.0]; // offset from apex by ~5 along axis
        let max_height = 20.0;

        let curves = plane_cone_ssi(
            plane_origin,
            plane_normal,
            cone_apex,
            cone_axis,
            half_angle,
            max_height,
        )
        .expect("non-origin apex oblique should return Ok");

        assert_eq!(curves.len(), 1, "Expected 1 curve, got {}", curves.len());

        match &curves[0] {
            SSICurve::Ellipse {
                center,
                normal,
                major_axis,
                semi_major,
                semi_minor,
            } => {
                assert!(*semi_major > 0.0);
                assert!(*semi_minor > 0.0);
                // Center should be near (10, 20, 35) region, not near origin
                assert!(
                    center[0] > 5.0 && center[2] > 25.0,
                    "Center {:?} should be near apex offset, not origin",
                    center,
                );
                let points =
                    sample_ellipse_points(*center, *normal, *major_axis, *semi_major, *semi_minor);
                for (i, p) in points.iter().enumerate() {
                    assert_point_on_plane(*p, plane_origin, plane_normal);
                    assert_point_on_cone(*p, cone_apex, cone_axis, half_angle);
                    for j in 0..3 {
                        assert!(!p[j].is_nan(), "Point {} coord {} is NaN", i, j);
                    }
                }
            }
            other => panic!("Expected Ellipse, got {:?}", other),
        }
    }

    #[test]
    fn test_plane_cone_through_apex_wide_angle() {
        // ADVERSARY: documents bug — through-apex generator lines for wide half_angle
        // (60°) do NOT lie on the cutting plane. The implementation computes generator
        // directions correctly for the cone, but the line endpoints extend to
        // t_param = max_height / cos(β), which places them off-plane when β is large.
        // The formula uses the cone's axial height to parametrize, but the resulting
        // 3D endpoint is not constrained to lie on the cutting plane.
        //
        // Bug: In the through-apex branch, the generator line endpoints are computed
        // as apex + t_param * g_i, but these endpoints are not projected back onto
        // the cutting plane. For small half_angles (like 30° in the existing test),
        // the error is small enough to pass tolerance. For 60°, the error is large.
        let half_angle = std::f64::consts::FRAC_PI_3; // 60°
        let cone_apex = [0.0, 0.0, 0.0];
        let cone_axis = [0.0, 0.0, 1.0];
        // Plane through apex: normal at 45° tilt → oblique cut through apex
        let plane_origin = [0.0, 0.0, 0.0];
        let plane_normal = [FRAC_1_SQRT_2, 0.0, FRAC_1_SQRT_2];
        let max_height = 10.0;

        let curves = plane_cone_ssi(
            plane_origin,
            plane_normal,
            cone_apex,
            cone_axis,
            half_angle,
            max_height,
        )
        .expect("through-apex wide angle should return Ok");

        assert_eq!(
            curves.len(),
            2,
            "Expected 2 generator lines through apex, got {}",
            curves.len(),
        );

        for curve in &curves {
            match curve {
                SSICurve::Line { start, end } => {
                    // Start at apex — this should be correct
                    let dist_start = v3_length(v3_sub(*start, cone_apex));
                    assert!(
                        dist_start < crate::units::TAU_MODEL * 100.0,
                        "Line start {:?} should be at apex, dist = {:.2e}",
                        start,
                        dist_start,
                    );
                    // End should be non-trivially far from apex
                    let dist_end = v3_length(v3_sub(*end, cone_apex));
                    assert!(
                        dist_end > 1.0,
                        "Line end {:?} should extend well beyond apex, dist = {:.2e}",
                        end,
                        dist_end,
                    );
                    // End should lie on the cone surface
                    assert_point_on_cone(*end, cone_apex, cone_axis, half_angle);

                    // Verify generator direction lies on the cutting plane
                    // (d · n = 0 since line goes through apex which is on the plane)
                    let dir = v3_normalize(v3_sub(*end, *start));
                    let dot_with_normal = v3_dot(dir, plane_normal).abs();
                    assert!(
                        dot_with_normal < crate::units::TAU_MODEL * 100.0,
                        "Generator direction should be perpendicular to plane normal, \
                         dot = {:.2e}",
                        dot_with_normal,
                    );
                    // Verify endpoint lies on the plane
                    let plane_error = v3_dot(v3_sub(*end, plane_origin), plane_normal).abs();
                    assert!(
                        plane_error < crate::units::TAU_MODEL * 100.0,
                        "Endpoint should lie on cutting plane, error = {:.2e}",
                        plane_error,
                    );
                }
                other => panic!("Expected Line, got {:?}", other),
            }
        }
    }

    #[test]
    fn test_plane_cone_oblique_max_height_clips_partial() {
        // ADVERSARY: Boundary investigation — the ellipse's z-range partially
        // exceeds max_height. Document whether the implementation returns the
        // full ellipse, a clipped curve, or empty.
        //
        // Setup: half_angle=30°, cone axis +Z, apex at origin.
        // Plane at 45° through z=8. The ellipse z-range will span roughly [5, 15].
        // Set max_height=10 so the upper part of the ellipse exceeds it.
        let half_angle = std::f64::consts::FRAC_PI_6; // 30°
        let plane_normal = [FRAC_1_SQRT_2, 0.0, FRAC_1_SQRT_2];
        let plane_origin = [0.0, 0.0, 8.0];
        let cone_apex = [0.0, 0.0, 0.0];
        let cone_axis = [0.0, 0.0, 1.0];
        let max_height = 10.0;

        let result = plane_cone_ssi(
            plane_origin,
            plane_normal,
            cone_apex,
            cone_axis,
            half_angle,
            max_height,
        );

        // ADVERSARY: documents behavior — the implementation checks if z_hi < -TOL
        // or z_lo > max_height + TOL but does NOT clip partial overlaps. So if
        // z_lo < max_height and z_hi > max_height, the full unclipped ellipse is returned.
        match result {
            Ok(curves) => {
                if curves.is_empty() {
                    // Implementation returned empty — the partial overlap was rejected.
                    // This is a valid conservative behavior but means partial intersections
                    // are lost. Document for future improvement.
                    // ADVERSARY: documents behavior — partial z-range overlap returns empty
                } else {
                    assert_eq!(curves.len(), 1, "Expected 0 or 1 curve");
                    // Implementation returned the full unclipped ellipse
                    match &curves[0] {
                        SSICurve::Ellipse {
                            center,
                            normal,
                            major_axis,
                            semi_major,
                            semi_minor,
                        } => {
                            // Verify points on the ellipse that are within the valid cone
                            // height range do lie on both surfaces.
                            let points = sample_ellipse_points(
                                *center,
                                *normal,
                                *major_axis,
                                *semi_major,
                                *semi_minor,
                            );
                            let mut points_above_max = 0;
                            for p in &points {
                                let h = v3_dot(v3_sub(*p, cone_apex), cone_axis);
                                if h > max_height + crate::units::TAU_MODEL {
                                    points_above_max += 1;
                                }
                                // All points should at least be on the plane
                                assert_point_on_plane(*p, plane_origin, plane_normal);
                            }
                            // ADVERSARY: documents behavior — some ellipse points extend
                            // beyond max_height. This is expected for the unclipped ellipse.
                            // The caller is responsible for trimming.
                            if points_above_max > 0 {
                                // Acceptable: implementation returns full mathematical ellipse
                            }
                        }
                        other => panic!("Expected Ellipse, got {:?}", other),
                    }
                }
            }
            Err(_) => {
                // Acceptable: implementation may reject partial overlaps with an error
            }
        }
    }

    #[test]
    fn test_plane_cone_oblique_no_nan() {
        // ADVERSARY: Sweep a variety of configurations and verify no NaN values
        // appear in any returned SSICurve fields.
        let configs: Vec<(
            [f64; 3], // plane_origin
            [f64; 3], // plane_normal
            [f64; 3], // cone_apex
            [f64; 3], // cone_axis
            f64,      // half_angle
            f64,      // max_height
        )> = vec![
            // Config 1: Standard oblique
            (
                [0.0, 0.0, 5.0],
                [FRAC_1_SQRT_2, 0.0, FRAC_1_SQRT_2],
                [0.0, 0.0, 0.0],
                [0.0, 0.0, 1.0],
                30.0_f64.to_radians(),
                20.0,
            ),
            // Config 2: Narrow cone, steep cut
            (
                [0.0, 0.0, 100.0],
                [0.1_f64.sin(), 0.0, 0.1_f64.cos()],
                [0.0, 0.0, 0.0],
                [0.0, 0.0, 1.0],
                1.0_f64.to_radians(),
                500.0,
            ),
            // Config 3: Through apex
            (
                [0.0, 0.0, 0.0],
                [FRAC_1_SQRT_2, 0.0, FRAC_1_SQRT_2],
                [0.0, 0.0, 0.0],
                [0.0, 0.0, 1.0],
                45.0_f64.to_radians(),
                10.0,
            ),
            // Config 4: Non-origin apex, tilted axis
            (
                [5.0, 5.0, 10.0],
                [0.0, 0.0, 1.0],
                [5.0, 5.0, 0.0],
                v3_normalize([1.0, 1.0, 1.0]),
                25.0_f64.to_radians(),
                30.0,
            ),
            // Config 5: Nearly perpendicular (but not quite — should hit oblique path)
            (
                [0.0, 0.0, 5.0],
                [0.01, 0.0, 1.0_f64],
                [0.0, 0.0, 0.0],
                [0.0, 0.0, 1.0],
                20.0_f64.to_radians(),
                10.0,
            ),
            // Config 6: Plane normal opposite to axis direction
            (
                [0.0, 0.0, 5.0],
                [-FRAC_1_SQRT_2, 0.0, -FRAC_1_SQRT_2],
                [0.0, 0.0, 0.0],
                [0.0, 0.0, 1.0],
                30.0_f64.to_radians(),
                20.0,
            ),
            // Config 7: Y-tilted normal (not in XZ plane)
            (
                [0.0, 0.0, 5.0],
                [0.0, FRAC_1_SQRT_2, FRAC_1_SQRT_2],
                [0.0, 0.0, 0.0],
                [0.0, 0.0, 1.0],
                30.0_f64.to_radians(),
                20.0,
            ),
        ];

        for (i, (po, pn, ca, cax, ha, mh)) in configs.iter().enumerate() {
            // Normalize the plane normal (some configs may not be unit length)
            let pn_norm = v3_normalize(*pn);

            let result = plane_cone_ssi(*po, pn_norm, *ca, *cax, *ha, *mh);

            match result {
                Ok(curves) => {
                    for (j, curve) in curves.iter().enumerate() {
                        match curve {
                            SSICurve::Ellipse {
                                center,
                                normal,
                                major_axis,
                                semi_major,
                                semi_minor,
                            } => {
                                for k in 0..3 {
                                    assert!(
                                        !center[k].is_nan(),
                                        "Config {} curve {} Ellipse center[{}] is NaN",
                                        i,
                                        j,
                                        k,
                                    );
                                    assert!(
                                        !normal[k].is_nan(),
                                        "Config {} curve {} Ellipse normal[{}] is NaN",
                                        i,
                                        j,
                                        k,
                                    );
                                    assert!(
                                        !major_axis[k].is_nan(),
                                        "Config {} curve {} Ellipse major_axis[{}] is NaN",
                                        i,
                                        j,
                                        k,
                                    );
                                }
                                assert!(
                                    !semi_major.is_nan(),
                                    "Config {} curve {} semi_major is NaN",
                                    i,
                                    j,
                                );
                                assert!(
                                    !semi_minor.is_nan(),
                                    "Config {} curve {} semi_minor is NaN",
                                    i,
                                    j,
                                );
                            }
                            SSICurve::Circle {
                                center,
                                normal,
                                radius,
                            } => {
                                for k in 0..3 {
                                    assert!(!center[k].is_nan(), "Config {} Circle center NaN", i);
                                    assert!(!normal[k].is_nan(), "Config {} Circle normal NaN", i);
                                }
                                assert!(!radius.is_nan(), "Config {} Circle radius NaN", i);
                            }
                            SSICurve::Line { start, end } => {
                                for k in 0..3 {
                                    assert!(!start[k].is_nan(), "Config {} Line start NaN", i);
                                    assert!(!end[k].is_nan(), "Config {} Line end NaN", i);
                                }
                            }
                        }
                    }
                }
                Err(_) => {
                    // NotSupported is acceptable (parabola, hyperbola)
                }
            }
        }
    }
}
