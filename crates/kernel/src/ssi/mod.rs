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
    /// A parabola in 3D (plane-cone intersection at γ ≈ β).
    /// axis_dir is the symmetry/opening direction of the parabola.
    /// Parametric: P(t) = vertex + t·perp_dir + (t²/(4·focal_length))·axis_dir
    /// where perp_dir = normalize(normal × axis_dir) is the transverse direction.
    Parabola {
        vertex: [f64; 3],
        axis_dir: [f64; 3],
        normal: [f64; 3],
        focal_length: f64,
        t_range: (f64, f64),
    },
    /// A hyperbola branch in 3D (plane-cone intersection at γ < β).
    /// Parametric: P(t) = center + a·cosh(t)·major_axis + b·sinh(t)·minor_axis
    /// where minor_axis = normalize(normal × major_axis).
    Hyperbola {
        center: [f64; 3],
        major_axis: [f64; 3],
        normal: [f64; 3],
        semi_transverse: f64,
        semi_conjugate: f64,
        t_range: (f64, f64),
    },
    /// A degree-4 parametric intersection curve between two unequal-radius cylinders.
    /// Parametrized by angle θ on cylinder A:
    ///   x(θ) = r_a cos θ
    ///   y(θ) = r_a sin θ
    ///   z(θ) = (r_a sin θ cos_alpha + sign·√(r_b² − r_a² cos²θ)) / sin_alpha
    /// Points are in a local frame; transform to world via frame matrix + center.
    /// Ref: [#1] Patrikalakis Ch.5 — quadric SSI degree-4 algebraic curves.
    Degree4CylCyl {
        /// Center point (midpoint of closest approach between axes)
        center: [f64; 3],
        /// Orthonormal frame columns [e3, e2, e1] for local-to-world transform.
        /// e1 = cyl_a_axis, e2 = perp component of cyl_b_axis, e3 = e1 × e2.
        frame: [[f64; 3]; 3],
        /// Cylinder A radius
        r_a: f64,
        /// Cylinder B radius
        r_b: f64,
        /// Cosine of inter-axis angle α
        cos_alpha: f64,
        /// Sine of inter-axis angle α (always positive)
        sin_alpha: f64,
        /// Branch sign: +1.0 or -1.0
        sign: f64,
        /// Valid θ range (θ_min, θ_max). Full [0, 2π) when r_b ≥ r_a.
        theta_range: (f64, f64),
    },
}

impl SSICurve {
    /// Evaluate a Degree4CylCyl curve at parameter θ, returning the world-space point.
    /// Returns None for non-Degree4CylCyl variants.
    pub(crate) fn evaluate_degree4(&self, theta: f64) -> Option<[f64; 3]> {
        match self {
            SSICurve::Degree4CylCyl {
                center,
                frame,
                r_a,
                r_b,
                cos_alpha,
                sin_alpha,
                sign,
                ..
            } => {
                let (cos_t, sin_t) = (theta.cos(), theta.sin());
                // Discriminant: R_B² − R_A² cos²θ ≥ 0 for valid θ
                let disc = r_b * r_b - r_a * r_a * cos_t * cos_t;
                if disc < 0.0 {
                    return None;
                }
                // Local coordinates in frame {e3, e2, e1}
                let lx = r_a * cos_t;
                let ly = r_a * sin_t;
                // Ref: [#1] Patrikalakis Ch.5 — derived from cylinder B implicit equation
                // x² + (y cos α − z sin α)² = R_B², solving quadratic in z.
                let lz = (r_a * sin_t * cos_alpha + sign * disc.sqrt()) / sin_alpha;

                // Transform to world: P = center + lx*frame[0] + ly*frame[1] + lz*frame[2]
                let wx = center[0] + lx * frame[0][0] + ly * frame[1][0] + lz * frame[2][0];
                let wy = center[1] + lx * frame[0][1] + ly * frame[1][1] + lz * frame[2][1];
                let wz = center[2] + lx * frame[0][2] + ly * frame[1][2] + lz * frame[2][2];
                Some([wx, wy, wz])
            }
            _ => None,
        }
    }
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

/// Compute SSI curves for two non-parallel cylinders with intersecting axes.
///
/// For equal radii (within SSI_RADII_RELATIVE_TOL), returns two `SSICurve::Ellipse`
/// with semi-axes R/sin(α/2) and R/cos(α/2) (dual-ellipse formula).
///
/// For unequal radii, returns two `SSICurve::Degree4CylCyl` parametric curves.
/// The intersection of unequal-radius cylinders is a degree-4 algebraic curve
/// parametrized by angle θ on cylinder A:
///   x(θ) = R_A cos θ,  y(θ) = R_A sin θ,
///   z(θ) = (R_A cos θ cos α ± √(R_B² − R_A² sin²θ)) / sin α
/// Ref: [#1] Patrikalakis Ch.5 — quadric SSI degree-4 algebraic curves.
///
/// Guard conditions:
/// - Parallel axes (|cos| > 1 - TAU_PARALLEL) → Ok(vec![])
/// - Near-parallel (angle < 15°) → NotSupported
/// - Zero radius → NotSupported
/// - Skew axes (closest distance >= 0.05×max(R_A,R_B)) → NotSupported
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

    // Near-parallel (angle < 15°) → not supported
    // Both dual-ellipse (equal-R) and degree-4 (unequal-R) formulas produce
    // curves that are too eccentric for reliable downstream use below 15°.
    // Ref: Patrikalakis Ch.5.
    if cos_angle > crate::units::SSI_CYL_CYL_MIN_ANGLE_COS {
        return Err(KernelError::NotSupported {
            operation: "cylinder-cylinder SSI: near-parallel axes (angle < 15°)".to_string(),
        });
    }

    // Zero-radius check
    let r_max = cyl_a_radius.max(cyl_b_radius);
    let r_min = cyl_a_radius.min(cyl_b_radius);
    if r_max < TAU_NORMALIZE {
        return Err(KernelError::NotSupported {
            operation: "cylinder-cylinder SSI: zero radius".to_string(),
        });
    }

    // Determine if radii are equal (within tolerance) for solver dispatch
    let radii_equal = (r_max - r_min) / r_max < crate::units::SSI_RADII_RELATIVE_TOL;

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

    // Skew axes check — use r_max for scale-appropriate threshold
    if closest_dist >= crate::units::SSI_SKEW_FACTOR * r_max {
        return Err(KernelError::NotSupported {
            operation: "cylinder-cylinder SSI: skew (non-intersecting) axes".to_string(),
        });
    }

    // Center = midpoint of closest approach
    let center = v3_scale(v3_add(p1_closest, p2_closest), 0.5);

    // Compute angle between axes
    let raw_cos = v3_dot(cyl_a_axis, cyl_b_axis);
    let alpha = raw_cos.abs().acos(); // angle in [0, π/2]

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

    if radii_equal {
        // ── Equal-R path: dual-ellipse formula ──────────────────────────
        let r = (cyl_a_radius + cyl_b_radius) / 2.0;
        let half_alpha = alpha / 2.0;
        let sin_half = half_alpha.sin();
        let cos_half = half_alpha.cos();

        // Curve 1: major direction = cot(α/2)*e1 + e2, semi_u = R/sin(α/2)
        let cot_half = cos_half / sin_half;
        let major_dir_1 = v3_add(v3_scale(e1, cot_half), e2);
        let major_dir_1_len = v3_length(major_dir_1);
        let major_axis_1 = v3_scale(major_dir_1, 1.0 / major_dir_1_len);
        let semi_major_1 = r / sin_half;
        let semi_minor_1 = r;
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
    } else {
        // ── Unequal-R path: degree-4 parametric curves ──────────────────
        // Ref: [#1] Patrikalakis Ch.5 — quadric SSI.
        //
        // In the local frame {e3, e2, e1} centered at `center`:
        //   Cyl A along e1 with radius R_A → x² + y² = R_A²
        //   Cyl B at angle α from e1 in the e1-e2 plane with radius R_B
        //
        // Parametrize on Cyl A: x = R_A cos θ, y = R_A sin θ
        // Substitute into Cyl B implicit equation to get:
        //   z(θ) = (R_A cos θ cos α ± √(R_B² − R_A² sin²θ)) / sin α
        //
        // The ± gives two branches (two intersection curves).
        // When R_B < R_A, the discriminant R_B² − R_A² sin²θ can go negative,
        // restricting θ to arcs where |sin θ| ≤ R_B/R_A.

        let r_a = cyl_a_radius;
        let r_b = cyl_b_radius;
        let cos_alpha = alpha.cos();
        let sin_alpha = alpha.sin();

        // Invariant: sin_alpha > 0 because we rejected parallel (α ≈ 0) above
        debug_assert!(sin_alpha > TAU_WORK);

        // Frame columns for local-to-world transform: [e3, e2, e1]
        // Local x maps to e3, local y maps to e2, local z maps to e1
        let frame = [e3, e2, e1];

        // Compute valid θ range
        // Discriminant: R_B² − R_A² cos²θ ≥ 0  ⟺  |cos θ| ≤ R_B/R_A
        let theta_range = if r_b >= r_a {
            // Full revolution — discriminant always non-negative
            (0.0, std::f64::consts::TAU)
        } else {
            // Restricted domain: |cos θ| ≤ R_B/R_A
            // cos θ ≤ R_B/R_A when θ ≥ arccos(R_B/R_A)
            // The valid arcs are centered at θ = π/2 and θ = 3π/2 (where cos θ = 0).
            // Arc 1: [arccos(R_B/R_A), π - arccos(R_B/R_A)]
            // Arc 2: [π + arccos(R_B/R_A), 2π - arccos(R_B/R_A)]
            // Store the first arc; the second is symmetric.
            let theta_min = (r_b / r_a).acos();
            let theta_max = std::f64::consts::PI - theta_min;
            (theta_min, theta_max)
        };

        // Check if the curves actually exist — verify discriminant at θ=0
        // (cos θ = 1, sin θ = 0 → discriminant = R_B², always ≥ 0)
        // So curves always exist when axes intersect, but may be partial.

        Ok(vec![
            SSICurve::Degree4CylCyl {
                center,
                frame,
                r_a,
                r_b,
                cos_alpha,
                sin_alpha,
                sign: 1.0,
                theta_range,
            },
            SSICurve::Degree4CylCyl {
                center,
                frame,
                r_a,
                r_b,
                cos_alpha,
                sin_alpha,
                sign: -1.0,
                theta_range,
            },
        ])
    }
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

        // ── Shared setup for parabola / hyperbola / ellipse ─────────────
        //
        // All three conic cases share the same local-frame construction:
        //   Local Z = cone axis, local X = projection of plane normal
        //   perpendicular to the axis (the symmetry plane of the conic).

        let signed_n_dot_a = v3_dot(n, a);
        let sin_alpha = (1.0 - signed_n_dot_a * signed_n_dot_a).sqrt().max(TOL);
        let d_signed = -d_apex; // positive when apex is on the negative side of the plane
        let abs_cos_alpha = cos_angle; // = |n̂ · â|

        // Local X direction: plane normal projected perpendicular to cone axis
        let n_perp = v3_sub(n, v3_scale(a, signed_n_dot_a));
        let n_perp_len = v3_length(n_perp);
        let local_x_dir = if n_perp_len > TOL {
            v3_scale(n_perp, 1.0 / n_perp_len)
        } else {
            return Ok(vec![]);
        };

        if discriminant.abs() < TOL {
            // ── Parabola case (γ ≈ β) ──────────────────────────────────
            // Ref #1: Patrikalakis Ch.5 — plane-cone parabolic section
            //
            // When cos²(α) ≈ sin²(β), the cutting plane is tangent to the
            // cone's asymptotic direction. The intersection is a parabola.
            //
            // In local cone frame (apex=origin, axis=Z, plane normal in XZ):
            //   Plane: sin(α)·x + cos(α)·z = D
            //   Cone:  x² + y² = t²·z²
            //
            // Substituting x = (D - z·cos(α))/sin(α) into cone eqn:
            //   (D - z·cos(α))²/sin²(α) + y² = t²·z²
            // Since cos²(α) ≈ sin²(β) = t²·cos²(β) = t²(1-sin²(β)) ≈ t²·cos²(α)/cos²(α)...
            // Actually: cos²α = sin²β ⟹ t·sinα = cosα (the z² terms cancel).
            //
            // The vertex is at the point where the parabola is closest to
            // the apex. On the y=0 symmetry line:
            //   (D - z·cosα)² / sin²α = t²·z²
            // With t·sinα = cosα: (D - z·cosα)² = cos²α · z²
            //   D - z·cosα = ±cosα · z
            // Two solutions: z = D/(2cosα) (double root from + sign) or z→∞ (- sign).
            //
            // Vertex: z_v = D/(2·cosα), x_v = (D - z_v·cosα)/sinα = D/(2·sinα)
            //
            // The parabola opens in the +z direction (away from apex) in local frame.
            // Focal length p: from the standard form y² = 4p·(z-z_v) at x=x_v,
            // expanding the cone-plane intersection near the vertex:
            //   y² = t²·z² - (D-z·cosα)²/sin²α
            // Let z = z_v + δ. After expansion with t·sinα = cosα:
            //   y² ≈ 2·t·D·δ / sinα = 4p·δ  → p = t·D / (2·sinα)

            if abs_cos_alpha < TOL || d_signed.abs() < TOL {
                // Plane through apex or perpendicular: degenerate
                return Ok(vec![]);
            }

            let z_v = d_signed / (2.0 * abs_cos_alpha);
            let x_v = d_signed / (2.0 * sin_alpha);

            // Check vertex is within cone bounds
            if z_v < -TOL || z_v > max_height + TOL {
                return Ok(vec![]);
            }

            // Invariant I4: focal_length = D·cos(α) / (2·sin(α))
            // Derived from u² = 2w·(t²z_v·sinα + x_v·cosα), where the quadratic
            // term in w vanishes (t²sin²α = cos²α at the parabola boundary),
            // leaving u² = 4·p·w with p = (t²z_v·sinα + x_v·cosα)/2 = D·cosα/(2sinα).
            let focal_length = d_signed.abs() * abs_cos_alpha / (2.0 * sin_alpha);
            if focal_length < TOL {
                return Ok(vec![]);
            }

            // Vertex in world coordinates
            let vertex = v3_add(
                v3_add(cone_apex, v3_scale(a, z_v)),
                v3_scale(local_x_dir, x_v),
            );

            // Parabola axis direction in local frame: along +z (cone axis direction)
            // projected into the cutting plane.
            // The parabola opens in the direction of increasing z along the plane.
            // In local frame the axis direction is (−cosα, 0, sinα)/1 (direction
            // along the plane away from the apex in the symmetry plane).
            // In world: sinα·a − cosα·local_x_dir (increasing z, decreasing x in local)
            let axis_dir_raw = v3_sub(v3_scale(a, sin_alpha), v3_scale(local_x_dir, abs_cos_alpha));
            let axis_dir = v3_normalize(axis_dir_raw);

            // Determine t_range: the parabola parameter t where cone height
            // stays within [0, max_height].
            //
            // With corrected parametric form:
            //   P(t) = vertex + t·perp_dir + (t²/(4p))·axis_dir
            //
            // The z-component (height along cone axis):
            //   z(t) = z_v + (t²/(4p))·(axis_dir·a) + t·(perp_dir·a)
            //
            // axis_dir·a = sinα (by construction)
            // perp_dir·a = 0 (perp is y-direction in local frame, perpendicular to axis)
            //
            // So z(t) = z_v + (t²/(4p))·sinα.  The parabola is symmetric in t.
            // z is minimized at t=0 (the vertex) and increases with |t|.
            //
            // For z = max_height: t² = 4p·(max_height - z_v)/sinα
            // Since the curve is symmetric, t_range = [-t_max, t_max].
            let delta_z = max_height - z_v;
            if delta_z < TOL {
                // Vertex at or beyond max_height — only point intersection
                return Ok(vec![]);
            }
            let t_max_sq = 4.0 * focal_length * delta_z / sin_alpha;
            if t_max_sq < TOL {
                return Ok(vec![]);
            }
            let t_max = t_max_sq.sqrt();

            // Symmetric range
            let t_min = -t_max;

            return Ok(vec![SSICurve::Parabola {
                vertex,
                axis_dir,
                normal: n,
                focal_length,
                t_range: (t_min, t_max),
            }]);
        }

        if discriminant < 0.0 {
            // ── Hyperbola case (γ < β, discriminant < 0) ────────────────
            // Ref #1: Patrikalakis Ch.5 — plane-cone hyperbolic section
            //
            // When cos²(α) < sin²(β), the cutting plane is shallower than
            // the cone surface. The intersection is a hyperbola.
            //
            // In local frame: Plane: sin(α)·x + cos(α)·z = D, Cone: x²+y² = t²z²
            //
            // On the y=0 symmetry line: (D - z·cosα)²/sin²α = t²z²
            //   D - z·cosα = ±t·sinα·z
            //   z₁ = D / (t·sinα + cosα)    (always valid, + branch)
            //   z₂ = D / (cosα - t·sinα)    (note: cosα < t·sinα here, so denom < 0)
            //
            // Both z-values are the vertices of the hyperbola (on the y=0 line).
            // z₁ > 0 (on the cone), z₂ < 0 (on the opposite nappe if D > 0).
            // For a single-nappe cone, only the z₁ branch matters.
            //
            // Hyperbola center: midpoint of vertices in local frame.
            // semi_transverse a = |z₂ - z₁| / (2·sinα) (distance from center to vertex along major axis)
            // semi_conjugate b: from y² = t²z² - (D-z·cosα)²/sin²α at z = z_center,
            //   b² = t²·z_c² - x_c² where x_c = (D - z_c·cosα)/sinα

            let z1 = d_signed / (t * sin_alpha + abs_cos_alpha);
            let z2 = d_signed / (abs_cos_alpha - t * sin_alpha);

            // For single-nappe cone, we only want the branch with z > 0
            // z1 is always on the positive nappe (when D > 0)
            // z2 is on the negative nappe (opposite side)

            // Check if the positive-nappe vertex is within cone bounds
            let z_pos = if d_signed > 0.0 { z1 } else { z2 };
            let z_neg = if d_signed > 0.0 { z2 } else { z1 };

            if z_pos < -TOL || z_pos > max_height + TOL {
                return Ok(vec![]);
            }

            // Hyperbola center in local frame
            let z_c = (z_pos + z_neg) / 2.0;
            let x_c = (d_signed - z_c * abs_cos_alpha) / sin_alpha;

            // Invariant I5: Semi-axes from conic section theory
            // In cutting-plane coordinates centered at vertex, the hyperbola satisfies:
            //   u² = 2w·C + w²·E
            // where C = D·t (always), E = t²sin²α - cos²α = |disc|/cos²β.
            // Standard form centered at midpoint: W²/a² - U²/b² = 1
            //   a = C/E = D·sinβ·cosβ / |disc|
            //   b = C/√E = D·sinβ / √|disc|
            let abs_disc = discriminant.abs(); // = sin²β - cos²α > 0
            let cos_beta = half_angle.cos();

            let a_conic = (d_signed.abs() * sin_beta * cos_beta) / abs_disc;
            let b_conic = d_signed.abs() * sin_beta / abs_disc.sqrt();

            if a_conic < TOL || b_conic < TOL {
                return Ok(vec![]);
            }

            // Center in world coordinates
            let center = v3_add(
                v3_add(cone_apex, v3_scale(a, z_c)),
                v3_scale(local_x_dir, x_c),
            );

            // Major axis direction (transverse axis): same as for ellipse,
            // along the symmetry plane of the conic.
            // Direction: sinα·a − cosα·local_x_dir (from low-z to high-z along the plane)
            let major_dir_raw =
                v3_sub(v3_scale(a, sin_alpha), v3_scale(local_x_dir, abs_cos_alpha));
            let major_dir = v3_normalize(major_dir_raw);

            // Determine t_range for the branch within [0, max_height]
            // P(t) = center + a·cosh(t)·major + b·sinh(t)·minor
            // Height: z(t) = z_c + a·cosh(t)·sinα
            // (minor_axis·a = 0 since minor is perpendicular to the symmetry plane)
            //
            // At the vertex (t=0): z = z_c + a·sinα
            // For z = 0: cosh(t) = (0 - z_c)/(a·sinα) → but cosh ≥ 1, so check
            // For z = max_height: cosh(t) = (max_height - z_c)/(a·sinα)

            // The positive branch vertex is at t=0: z_vertex = z_c + a_conic * sin_alpha
            // We want the branch that stays in [0, max_height]
            let z_vertex = z_c + a_conic * sin_alpha;

            // For t_range: height is z(t) = z_c + a·cosh(t)·sinα
            // For positive branch: z increases with |t|
            // t=0 gives minimum z on this branch = z_vertex
            if z_vertex < -TOL || z_vertex > max_height + TOL {
                // Try the other branch: z_vertex_neg = z_c - a·sinα
                let z_vertex_neg = z_c - a_conic * sin_alpha;
                if z_vertex_neg < -TOL || z_vertex_neg > max_height + TOL {
                    return Ok(vec![]);
                }
                // Use the other branch (negate major_axis)
                let neg_major = v3_scale(major_dir, -1.0);

                // t_range: z(t) = z_c - a·cosh(t)·sinα, z decreases with |t|
                // z = 0: cosh(t) = z_c / (a·sinα)
                // z = max_height: cosh(t) = (z_c - max_height) / (a·sinα)
                let cosh_at_zero = z_c / (a_conic * sin_alpha);
                let cosh_at_max = (z_c - max_height) / (a_conic * sin_alpha);

                let t_lo = if cosh_at_max >= 1.0 {
                    -cosh_at_max.acosh()
                } else {
                    0.0
                };
                let t_hi = if cosh_at_zero >= 1.0 {
                    cosh_at_zero.acosh()
                } else {
                    0.0
                };

                if (t_hi - t_lo).abs() < TOL {
                    return Ok(vec![]);
                }

                return Ok(vec![SSICurve::Hyperbola {
                    center,
                    major_axis: neg_major,
                    normal: n,
                    semi_transverse: a_conic,
                    semi_conjugate: b_conic,
                    t_range: (t_lo, t_hi),
                }]);
            }

            // Positive branch: z(t) = z_c + a·cosh(t)·sinα
            // z = max_height → cosh(t) = (max_height - z_c) / (a·sinα)
            // z = 0 → cosh(t) = -z_c / (a·sinα) — only if z_c < 0
            let cosh_at_max = (max_height - z_c) / (a_conic * sin_alpha);
            let t_hi = if cosh_at_max >= 1.0 {
                cosh_at_max.acosh()
            } else {
                0.0 // vertex is already past max_height — shouldn't happen given check above
            };

            // The branch extends symmetrically from t=0 (the vertex).
            // cosh is even, so z(-t) = z(t). The range is symmetric: [-t_hi, t_hi].
            let t_range = (-t_hi, t_hi);

            if (t_range.1 - t_range.0).abs() < TOL {
                return Ok(vec![]);
            }

            return Ok(vec![SSICurve::Hyperbola {
                center,
                major_axis: major_dir,
                normal: n,
                semi_transverse: a_conic,
                semi_conjugate: b_conic,
                t_range,
            }]);
        }

        // ── Ellipse case (discriminant > 0, γ > β) ───────────────────────
        //
        // Uses the shared local cone frame (apex=origin, axis=Z, plane normal in XZ).
        // Variables signed_n_dot_a, sin_alpha, d_signed, abs_cos_alpha, local_x_dir
        // are already computed in the shared setup block above.

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
        // Uses local_x_dir from the shared setup block above.

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

#[cfg(test)]
mod tests;
