//! Surface-Surface Intersection (SSI) module for cylindrical boolean operations.
//!
//! Provides analytical SSI computation for axis-aligned boxes and Z-axis cylinders,
//! plus point-in-solid classification and Aabb extraction.
//!
//! Reference: Patrikalakis Ch.5 — SSI algorithms for analytic surfaces.

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

// ── Z range helper ─────────────────────────────────────────────────────────

/// Compute the actual Z extent of a cylinder, accounting for direction.
/// Returns (z_min, z_max) regardless of whether direction is +Z or -Z.
pub(crate) fn cyl_z_range(cyl: &CylinderParams) -> (f64, f64) {
    let z0 = cyl.center_bottom[2];
    let z1 = z0 + cyl.depth * cyl.direction[2];
    (z0.min(z1), z0.max(z1))
}

// ── SSI computation ────────────────────────────────────────────────────────

/// Compute SSI between a plane perpendicular to the Z-axis and a Z-axis cylinder.
/// Returns a circle at the plane height if within the cylinder's Z range.
pub(crate) fn plane_perp_cylinder_ssi(plane_z: f64, cyl: &CylinderParams) -> Vec<SSICurve> {
    let (cyl_z_min, cyl_z_max) = cyl_z_range(cyl);

    if plane_z < cyl_z_min - 1e-9 || plane_z > cyl_z_max + 1e-9 {
        return vec![];
    }

    vec![SSICurve::Circle {
        center: [cyl.center_bottom[0], cyl.center_bottom[1], plane_z],
        normal: [0.0, 0.0, 1.0],
        radius: cyl.radius,
    }]
}

/// Compute SSI between a plane parallel to the Z-axis and a Z-axis cylinder.
/// Returns 0 or 2 vertical line segments.
pub(crate) fn plane_parallel_cylinder_ssi(
    plane_origin: [f64; 3],
    plane_normal: [f64; 3],
    cyl: &CylinderParams,
    z_min: f64,
    z_max: f64,
) -> Vec<SSICurve> {
    let cx = cyl.center_bottom[0];
    let cy = cyl.center_bottom[1];
    let r = cyl.radius;

    // Signed distance from cylinder center to the plane (2D, XY projection)
    let d = (cx - plane_origin[0]) * plane_normal[0] + (cy - plane_origin[1]) * plane_normal[1];
    let d_abs = d.abs();

    if d_abs >= r - 1e-9 {
        return vec![];
    }

    // Perpendicular direction along the plane in XY
    let px = -plane_normal[1];
    let py = plane_normal[0];

    let offset = (r * r - d_abs * d_abs).sqrt();

    // Midpoint on the plane closest to cylinder center
    let mid_x = cx - d * plane_normal[0];
    let mid_y = cy - d * plane_normal[1];

    vec![
        SSICurve::Line {
            start: [mid_x + offset * px, mid_y + offset * py, z_min],
            end: [mid_x + offset * px, mid_y + offset * py, z_max],
        },
        SSICurve::Line {
            start: [mid_x - offset * px, mid_y - offset * py, z_min],
            end: [mid_x - offset * px, mid_y - offset * py, z_max],
        },
    ]
}

/// Compute SSI between two parallel Z-axis cylinders.
/// Returns 0 or 2 vertical line segments at circle-circle intersection points.
pub(crate) fn cylinder_cylinder_ssi(
    cyl_a: &CylinderParams,
    cyl_b: &CylinderParams,
    z_min: f64,
    z_max: f64,
) -> Vec<SSICurve> {
    let c1 = [cyl_a.center_bottom[0], cyl_a.center_bottom[1]];
    let c2 = [cyl_b.center_bottom[0], cyl_b.center_bottom[1]];
    let r1 = cyl_a.radius;
    let r2 = cyl_b.radius;

    let dx = c2[0] - c1[0];
    let dy = c2[1] - c1[1];
    let d = (dx * dx + dy * dy).sqrt();

    if d >= r1 + r2 - 1e-9 || d <= (r1 - r2).abs() + 1e-9 {
        return vec![];
    }

    // 2D circle-circle intersection
    let a = (r1 * r1 - r2 * r2 + d * d) / (2.0 * d);
    let h_sq = r1 * r1 - a * a;
    if h_sq < 0.0 {
        return vec![];
    }
    let h = h_sq.sqrt();

    let ux = dx / d;
    let uy = dy / d;

    let mid_x = c1[0] + a * ux;
    let mid_y = c1[1] + a * uy;

    vec![
        SSICurve::Line {
            start: [mid_x - h * uy, mid_y + h * ux, z_min],
            end: [mid_x - h * uy, mid_y + h * ux, z_max],
        },
        SSICurve::Line {
            start: [mid_x + h * uy, mid_y - h * ux, z_min],
            end: [mid_x + h * uy, mid_y - h * ux, z_max],
        },
    ]
}

// ── Point-in-solid classification ──────────────────────────────────────────

/// Test if a point is strictly inside an axis-aligned box.
pub(crate) fn point_in_box(pt: [f64; 3], aabb: &Aabb) -> bool {
    pt[0] > aabb.min[0] + 1e-9
        && pt[0] < aabb.max[0] - 1e-9
        && pt[1] > aabb.min[1] + 1e-9
        && pt[1] < aabb.max[1] - 1e-9
        && pt[2] > aabb.min[2] + 1e-9
        && pt[2] < aabb.max[2] - 1e-9
}

/// Test if a point is strictly inside a Z-axis cylinder.
pub(crate) fn point_in_cylinder(pt: [f64; 3], cyl: &CylinderParams) -> bool {
    let dx = pt[0] - cyl.center_bottom[0];
    let dy = pt[1] - cyl.center_bottom[1];
    let dist_2d = (dx * dx + dy * dy).sqrt();
    let (z_min, z_max) = cyl_z_range(cyl);

    dist_2d < cyl.radius - 1e-9 && pt[2] > z_min + 1e-9 && pt[2] < z_max - 1e-9
}

// ── Aabb extraction ────────────────────────────────────────────────────────

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
    cx - r >= aabb.min[0] - 1e-9
        && cx + r <= aabb.max[0] + 1e-9
        && cy - r >= aabb.min[1] - 1e-9
        && cy + r <= aabb.max[1] + 1e-9
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
    if dx * dx + dy * dy > r * r + 1e-9 {
        return true;
    }

    // Check Z overlap — use tolerance so Z-touching surfaces are NOT disjoint
    let (cyl_z_min, cyl_z_max) = cyl_z_range(cyl);
    if cyl_z_max < aabb.min[2] - 1e-9 || cyl_z_min > aabb.max[2] + 1e-9 {
        return true;
    }

    false
}

/// Check if two Z-axis cylinders are disjoint (no overlap in XY).
pub(crate) fn cyls_disjoint(a: &CylinderParams, b: &CylinderParams) -> bool {
    let dx = a.center_bottom[0] - b.center_bottom[0];
    let dy = a.center_bottom[1] - b.center_bottom[1];
    let d = (dx * dx + dy * dy).sqrt();
    d >= a.radius + b.radius - 1e-9
}
