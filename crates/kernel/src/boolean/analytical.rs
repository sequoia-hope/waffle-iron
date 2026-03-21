//! SSI-based boolean operations (box-cylinder, cylinder-cylinder) and
//! analytical B-Rep construction helpers.
//!
//! Contains frame rotation utilities, analytical SSI dispatch, and all
//! build_* functions for constructing B-Rep results from cylinder/box
//! primitives.

use crate::geometry::curve::{Arc3D, Circle3D, CurveGeom, Ellipse3D, Line3D};
use crate::geometry::point::{Point3, Vector3};
use crate::geometry::surface::{Cylinder, Plane, SurfaceGeom};
use crate::ssi::{self, Aabb};
use crate::topology::arena::TopoArena;
use crate::topology::half_edge::*;
use crate::types::*;
use crate::units::{TAU_COINCIDENT, TAU_MODEL, TAU_WORK};
use crate::vecmath::*;
use crate::waffle_kernel::{CylinderParams, WaffleSolid};
use std::collections::HashMap;

use super::{
    boolean_op_from_polys, build_brep_from_polygons, build_brep_from_polygons_inner,
    extract_face_polys, extract_face_polys_general, point_in_solid, BoolOp, BooleanResult,
    FacePoly,
};

// ── Frame rotation utilities ────────────────────────────────────────────

/// Compute a rotation matrix that maps unit vector `dir` to [0, 0, 1].
///
/// Uses Rodrigues' rotation formula around the axis `cross(dir, Z)`.
pub(super) fn rotation_to_z(dir: [f64; 3]) -> Mat3 {
    let z = [0.0, 0.0, 1.0];
    let cos_theta = v3_dot(dir, z); // dir · Z = dz

    // Already Z-aligned (within tolerance)
    if cos_theta > 1.0 - TAU_WORK {
        return MAT3_IDENTITY;
    }

    // Anti-parallel to Z: rotate 180° around X
    if cos_theta < -1.0 + TAU_WORK {
        return [[1.0, 0.0, 0.0], [0.0, -1.0, 0.0], [0.0, 0.0, -1.0]];
    }

    // General case: rotation axis = cross(dir, Z), normalized
    let axis = v3_normalize(v3_cross(dir, z));
    let s = (1.0 - cos_theta * cos_theta).max(0.0).sqrt(); // sin(theta)
    let c = cos_theta;
    let t = 1.0 - c;
    let (x, y, zz) = (axis[0], axis[1], axis[2]);

    // Rodrigues' rotation matrix
    [
        [t * x * x + c, t * x * y - s * zz, t * x * zz + s * y],
        [t * x * y + s * zz, t * y * y + c, t * y * zz - s * x],
        [t * x * zz - s * y, t * y * zz + s * x, t * zz * zz + c],
    ]
}

/// Like rotation_to_z, but also aligns a box solid's edges with X/Y.
///
/// Finds a side face normal (perpendicular to cyl_dir), uses it to
/// determine the additional Z-rotation needed for full alignment.
/// Ref #24 Barton: complete frame normalization.
pub(super) fn rotation_to_z_aligned(cyl_dir: [f64; 3], box_solid: &WaffleSolid) -> Mat3 {
    let m1 = rotation_to_z(cyl_dir);

    // Find a side face normal (perpendicular to cyl_dir)
    let mut side_normal = None;
    for geom in box_solid.face_geometry.values() {
        if let SurfaceGeom::Planar(plane) = geom {
            let n = plane.normal.to_array();
            let dot = v3_dot(n, cyl_dir).abs();
            if dot < crate::units::COS_NEAR_PERPENDICULAR {
                // Nearly perpendicular to cyl direction
                side_normal = Some(n);
                break;
            }
        }
    }

    let Some(sn) = side_normal else { return m1 };

    // Rotate side normal into Z-frame
    let sn_rot = mat3_mul_vec(&m1, sn);
    // sn_rot should be mostly in XY plane; find its angle
    let angle = sn_rot[1].atan2(sn_rot[0]);

    // Z-rotation to align side normal with +X
    let (cos_a, sin_a) = (angle.cos(), angle.sin());
    let z_rot: Mat3 = [[cos_a, sin_a, 0.0], [-sin_a, cos_a, 0.0], [0.0, 0.0, 1.0]];
    mat3_mul(&z_rot, &m1)
}

/// Transform CylinderParams into a rotated coordinate frame.
pub(super) fn rotate_cyl_params(cyl: &CylinderParams, m: &Mat3) -> CylinderParams {
    CylinderParams {
        center_bottom: mat3_mul_vec(m, cyl.center_bottom),
        radius: cyl.radius,
        x_axis: mat3_mul_vec(m, cyl.x_axis),
        y_axis: mat3_mul_vec(m, cyl.y_axis),
        direction: mat3_mul_vec(m, cyl.direction),
        depth: cyl.depth,
    }
}

/// Transform a BooleanResult back from a rotated frame using the inverse rotation.
pub(super) fn rotate_boolean_result(result: &mut BooleanResult, m_inv: &Mat3) {
    // Rotate all vertex positions
    for vertex in &mut result.arena.vertices {
        vertex.position = mat3_mul_vec(m_inv, vertex.position);
    }

    // Rotate face geometry (plane normals/origins, cylinder axes/origins)
    for geom in result.face_geometry.values_mut() {
        match geom {
            SurfaceGeom::Planar(plane) => {
                plane.origin = Point3::from_array(mat3_mul_vec(m_inv, plane.origin.to_array()));
                plane.normal = Vector3::from_array(mat3_mul_vec(m_inv, plane.normal.to_array()));
            }
            SurfaceGeom::Cylindrical(cyl) => {
                cyl.origin = Point3::from_array(mat3_mul_vec(m_inv, cyl.origin.to_array()));
                cyl.axis = Vector3::from_array(mat3_mul_vec(m_inv, cyl.axis.to_array()));
            }
            SurfaceGeom::Conical(cone) => {
                cone.apex = Point3::from_array(mat3_mul_vec(m_inv, cone.apex.to_array()));
                cone.axis = Vector3::from_array(mat3_mul_vec(m_inv, cone.axis.to_array()));
            }
            SurfaceGeom::Spherical(sphere) => {
                sphere.center = Point3::from_array(mat3_mul_vec(m_inv, sphere.center.to_array()));
            }
            SurfaceGeom::Toroidal(torus) => {
                torus.center = Point3::from_array(mat3_mul_vec(m_inv, torus.center.to_array()));
                torus.axis = Vector3::from_array(mat3_mul_vec(m_inv, torus.axis.to_array()));
            }
        }
    }

    // Rotate edge geometry (line endpoints/directions, circles, arcs)
    for geom in result.edge_geometry.values_mut() {
        match geom {
            CurveGeom::Linear(line) => {
                line.origin = Point3::from_array(mat3_mul_vec(m_inv, line.origin.to_array()));
                line.direction =
                    Vector3::from_array(mat3_mul_vec(m_inv, line.direction.to_array()));
            }
            CurveGeom::Circular(circle) => {
                circle.center = Point3::from_array(mat3_mul_vec(m_inv, circle.center.to_array()));
                circle.normal = Vector3::from_array(mat3_mul_vec(m_inv, circle.normal.to_array()));
            }
            CurveGeom::Arc(arc) => {
                arc.center = Point3::from_array(mat3_mul_vec(m_inv, arc.center.to_array()));
                arc.normal = Vector3::from_array(mat3_mul_vec(m_inv, arc.normal.to_array()));
                arc.start_point =
                    Point3::from_array(mat3_mul_vec(m_inv, arc.start_point.to_array()));
            }
            CurveGeom::Elliptical(ell) => {
                ell.center = Point3::from_array(mat3_mul_vec(m_inv, ell.center.to_array()));
                ell.normal = Vector3::from_array(mat3_mul_vec(m_inv, ell.normal.to_array()));
                ell.major_axis =
                    Vector3::from_array(mat3_mul_vec(m_inv, ell.major_axis.to_array()));
            }
        }
    }
}

// ── Ellipse discretization ──────────────────────────────────────────────

/// Discretize an ellipse into polygon points for the polygon boolean path.
///
/// Uses adaptive segment count: max(32, ceil(2π * semi_major / tolerance)).
#[allow(dead_code)] // Will be used when SSI ellipses feed into polygon booleans
pub(crate) fn ellipse_to_polygon(
    center: [f64; 3],
    normal: [f64; 3],
    major_axis: [f64; 3],
    semi_major: f64,
    semi_minor: f64,
    n_segments: usize,
) -> Vec<[f64; 3]> {
    // minor_axis = normal × major_axis
    let minor_axis = v3_cross(normal, major_axis);

    let mut pts = Vec::with_capacity(n_segments);
    for i in 0..n_segments {
        let t = 2.0 * std::f64::consts::PI * (i as f64) / (n_segments as f64);
        let cos_t = t.cos();
        let sin_t = t.sin();
        pts.push([
            center[0] + semi_major * cos_t * major_axis[0] + semi_minor * sin_t * minor_axis[0],
            center[1] + semi_major * cos_t * major_axis[1] + semi_minor * sin_t * minor_axis[1],
            center[2] + semi_major * cos_t * major_axis[2] + semi_minor * sin_t * minor_axis[2],
        ]);
    }
    pts
}

// ── SSI dispatch ────────────────────────────────────────────────────────

/// Perform an SSI-based boolean operation on solids involving cylinders.
pub(crate) fn ssi_boolean_op(
    solid_a: &WaffleSolid,
    solid_b: &WaffleSolid,
    op: BoolOp,
    id_alloc: &mut dyn FnMut() -> u64,
) -> Result<BooleanResult, KernelError> {
    let a_is_cyl = solid_a.cylinder_params.is_some();
    let b_is_cyl = solid_b.cylinder_params.is_some();

    // Try analytical SSI pipeline first; fall back to polygon approximation
    // for unsupported cases (partial overlaps, cylinder-minus-box, etc.)
    let analytical_result = if a_is_cyl && b_is_cyl {
        let cyl_a = solid_a.cylinder_params.as_ref().unwrap();
        let cyl_b = solid_b.cylinder_params.as_ref().unwrap();
        cyl_cyl_boolean(cyl_a, cyl_b, op, id_alloc)
    } else if !a_is_cyl && b_is_cyl {
        let box_aabb = ssi::compute_box_aabb(solid_a);
        let cyl = solid_b.cylinder_params.as_ref().unwrap();
        box_cyl_boolean(&box_aabb, solid_a, cyl, op, id_alloc)
    } else if a_is_cyl && !b_is_cyl {
        let box_aabb = ssi::compute_box_aabb(solid_b);
        let cyl = solid_a.cylinder_params.as_ref().unwrap();
        match op {
            BoolOp::Union => box_cyl_boolean(&box_aabb, solid_b, cyl, BoolOp::Union, id_alloc),
            BoolOp::Intersect => {
                box_cyl_boolean(&box_aabb, solid_b, cyl, BoolOp::Intersect, id_alloc)
            }
            BoolOp::Subtract => cyl_minus_box_boolean(&box_aabb, solid_b, cyl, id_alloc),
        }
    } else {
        Err(KernelError::NotSupported {
            operation: "unsupported boolean operand combination".to_string(),
        })
    };

    // A15.2: Do NOT fall back to polygon approximation for quadric pairs.
    // If the SSI solver returns NotSupported, propagate it so callers know
    // the analytical path is incomplete. The polygon fallback is only for
    // freeform (NURBS/BSpline) surfaces via the general-solid dispatch path.
    analytical_result
}

/// Polygon-approximation boolean: convert any cylinder solids to polygon face
/// approximations, then use the standard polygon-clipping boolean pipeline.
///
/// This is a fallback for cylinder-involving booleans that the analytical SSI
/// pipeline doesn't handle (e.g., cylinder-minus-box, partial overlaps).
pub(crate) fn polygon_approx_boolean(
    solid_a: &WaffleSolid,
    solid_b: &WaffleSolid,
    op: BoolOp,
    id_alloc: &mut dyn FnMut() -> u64,
) -> Result<BooleanResult, KernelError> {
    let a_faces = extract_face_polys_general(solid_a);
    let b_faces = extract_face_polys_general(solid_b);

    if a_faces.is_empty() && b_faces.is_empty() {
        return Err(KernelError::BooleanFailed {
            reason: "both solids have no face polygons".to_string(),
        });
    }
    // Handle empty solids: pass through to the non-empty one
    if a_faces.is_empty() {
        return match op {
            BoolOp::Union => build_brep_from_polygons_inner(&b_faces, TAU_MODEL, true, id_alloc),
            _ => {
                let mut arena = TopoArena::new();
                let solid_idx = arena.add_solid();
                let shell_idx = arena.add_shell(solid_idx);
                arena.solids[solid_idx.0].outer_shell = shell_idx;
                Ok(BooleanResult {
                    arena,
                    face_map: HashMap::new(),
                    edge_map: HashMap::new(),
                    vertex_map: HashMap::new(),
                    face_geometry: HashMap::new(),
                    edge_geometry: HashMap::new(),
                    cached_face_polys: None,
                })
            }
        };
    }
    if b_faces.is_empty() {
        return match op {
            BoolOp::Subtract | BoolOp::Union => {
                build_brep_from_polygons_inner(&a_faces, TAU_MODEL, true, id_alloc)
            }
            BoolOp::Intersect => {
                let mut arena = TopoArena::new();
                let solid_idx = arena.add_solid();
                let shell_idx = arena.add_shell(solid_idx);
                arena.solids[solid_idx.0].outer_shell = shell_idx;
                Ok(BooleanResult {
                    arena,
                    face_map: HashMap::new(),
                    edge_map: HashMap::new(),
                    vertex_map: HashMap::new(),
                    face_geometry: HashMap::new(),
                    edge_geometry: HashMap::new(),
                    cached_face_polys: None,
                })
            }
        };
    }

    // Limit face count to prevent O(n*m) explosion in classification.
    // Cylinder (34 faces) + box (6 faces) = 40 → OK.
    // Cylinder (34) + gear (many faces) = 60+ → can be very slow.
    let total_faces = a_faces.len() + b_faces.len();
    if total_faces > 8000 {
        return Err(KernelError::NotSupported {
            operation: format!(
                "polygon approx boolean: {} total faces exceeds limit",
                total_faces
            ),
        });
    }

    // Product-based guard: O(A*B) face classification is too expensive
    // when both solids are non-convex (e.g., two gears with ~200 faces each).
    // Use AABB-filtered effective product: most face pairs are spatially disjoint
    // in multi-step operations, so the raw product vastly overestimates cost.
    let a_convex = a_faces.len() <= 12;
    let b_convex = b_faces.len() <= 12;
    let product = a_faces.len() * b_faces.len();
    if product > 5000 && !a_convex && !b_convex {
        let effective = super::count_aabb_overlapping_pairs(&a_faces, &b_faces, TAU_MODEL);
        if effective > 5000 {
            return Err(KernelError::NotSupported {
                operation: format!(
                    "polygon approx boolean: {}x{} effective face product ({}) too large for non-convex solids",
                    a_faces.len(),
                    b_faces.len(),
                    effective
                ),
            });
        }
    }

    boolean_op_from_polys(a_faces, b_faces, op, id_alloc)
}

// ── Box-cylinder boolean ────────────────────────────────────────────────

/// Box-cylinder boolean dispatch with frame rotation for axis-generic support.
///
/// Rotates the box AABB and cylinder into a Z-aligned frame (using the
/// cylinder's direction), performs the boolean using Z-assumption logic,
/// then rotates the result back. For Z-aligned inputs, `rotation_to_z`
/// returns the identity matrix — zero overhead.
///
/// Ref #24 Barton: frame normalization before boolean.
fn box_cyl_boolean(
    _box_aabb: &Aabb,
    box_solid: &WaffleSolid,
    cyl: &CylinderParams,
    op: BoolOp,
    id_alloc: &mut dyn FnMut() -> u64,
) -> Result<BooleanResult, KernelError> {
    // Cross-plane guard: the analytical box-cyl boolean assumes both bodies
    // share the same extrude axis. When the box was extruded in a different
    // direction than the cylinder, the AABB computed after rotation to the
    // cylinder's Z-aligned frame inflates and produces incorrect enclosure
    // classifications. Detect this by checking whether any box cap-face
    // normal is parallel to the cylinder direction; if not, bail to the
    // polygon-clipping fallback which handles arbitrary orientations.
    let has_parallel_cap = box_solid.face_geometry.values().any(|geom| {
        if let SurfaceGeom::Planar(plane) = geom {
            let dot = v3_dot(plane.normal.to_array(), cyl.direction).abs();
            dot > 0.95 // nearly parallel (within ~18°)
        } else {
            false
        }
    });
    if !has_parallel_cap {
        return Err(KernelError::NotSupported {
            operation: "cross-plane box-cylinder boolean (different extrude axes)".to_string(),
        });
    }

    // Rotate into cylinder's Z-aligned frame.
    // For non-rectangular solids (>6 faces), use rotation_to_z_aligned to align a
    // side face normal with X, improving normal consistency (Barton [#24]).
    // For rectangular solids, plain rotation_to_z suffices since the AABB
    // reconstruction in make_box_face_polys handles the geometry.
    let m = if box_solid.face_map.len() > 6 {
        rotation_to_z_aligned(cyl.direction, box_solid)
    } else {
        rotation_to_z(cyl.direction)
    };
    let m_inv = mat3_transpose(&m);
    let cyl_z = rotate_cyl_params(cyl, &m);
    let box_aabb = ssi::compute_rotated_box_aabb(box_solid, &m);

    let xy_enclosed_aabb = ssi::cyl_enclosed_in_box(&cyl_z, &box_aabb);
    // AABB enclosure is necessary but not sufficient for non-convex polygon extrudes.
    // A rectangular prism has exactly 6 faces; more faces indicate a non-rectangular
    // (possibly concave) polygon extrude. Refine with point-in-solid test.
    // Cross-plane cases (different extrude axes) are already rejected above,
    // so AABB inflation from rotation is no longer a concern here.
    let xy_enclosed = if xy_enclosed_aabb && box_solid.face_map.len() > 6 {
        let face_polys = extract_face_polys(box_solid);
        if face_polys.len() < 4 {
            xy_enclosed_aabb // Not enough faces for reliable point_in_solid
        } else {
            // Test cylinder axis midpoint against the solid's actual face polygons
            // in the ORIGINAL (unrotated) frame.
            let cyl_mid = [
                cyl.center_bottom[0] + cyl.direction[0] * cyl.depth * 0.5,
                cyl.center_bottom[1] + cyl.direction[1] * cyl.depth * 0.5,
                cyl.center_bottom[2] + cyl.direction[2] * cyl.depth * 0.5,
            ];
            point_in_solid(cyl_mid, &face_polys)
        }
    } else {
        xy_enclosed_aabb
    };
    let disjoint = ssi::box_cyl_disjoint(&box_aabb, &cyl_z);

    // Check full 3D enclosure: XY-enclosed AND Z range within box
    let (cyl_z_min, cyl_z_max) = ssi::cyl_z_range(&cyl_z);
    let z_enclosed = cyl_z_min >= box_aabb.min[2] - TAU_COINCIDENT
        && cyl_z_max <= box_aabb.max[2] + TAU_COINCIDENT;
    let fully_enclosed = xy_enclosed && z_enclosed;

    // Detect boss: cylinder XY-enclosed and extends beyond box on top and/or bottom.
    // Covers both the "sits on top/bottom" case (z_touches) and the "passes through"
    // case (cylinder starts inside box but extends beyond a face).
    let extends_above = cyl_z_max > box_aabb.max[2] + TAU_COINCIDENT;
    let extends_below = cyl_z_min < box_aabb.min[2] - TAU_COINCIDENT;
    let is_boss_top = xy_enclosed && !fully_enclosed && extends_above && !extends_below;
    let is_boss_bot = xy_enclosed && !fully_enclosed && extends_below && !extends_above;

    match op {
        BoolOp::Subtract => {
            if fully_enclosed {
                let mut result = build_box_minus_enclosed_cyl(&box_aabb, &cyl_z, id_alloc)?;
                rotate_boolean_result(&mut result, &m_inv);
                Ok(result)
            } else if disjoint {
                clone_solid_as_result(box_solid, id_alloc)
            } else {
                Err(KernelError::NotSupported {
                    operation: "partial box-cylinder subtract".to_string(),
                })
            }
        }
        BoolOp::Union => {
            if fully_enclosed {
                // Cylinder fully inside box → union = box (original frame)
                clone_solid_as_result(box_solid, id_alloc)
            } else if is_boss_top || is_boss_bot {
                let mut result = build_box_with_cyl_boss(&box_aabb, &cyl_z, is_boss_top, id_alloc)?;
                rotate_boolean_result(&mut result, &m_inv);
                Ok(result)
            } else if disjoint {
                let mut result = build_disjoint_box_cyl_union(&box_aabb, &cyl_z, id_alloc)?;
                rotate_boolean_result(&mut result, &m_inv);
                Ok(result)
            } else {
                Err(KernelError::NotSupported {
                    operation: "partial box-cylinder union".to_string(),
                })
            }
        }
        BoolOp::Intersect => {
            if fully_enclosed {
                // Cylinder fully inside box → intersect = cylinder
                let mut result = build_cyl_result(&cyl_z, id_alloc)?;
                rotate_boolean_result(&mut result, &m_inv);
                Ok(result)
            } else if disjoint {
                Err(KernelError::BooleanFailed {
                    reason: "no intersection (disjoint)".to_string(),
                })
            } else {
                Err(KernelError::NotSupported {
                    operation: "partial box-cylinder intersect".to_string(),
                })
            }
        }
    }
}

// ── Cylinder-cylinder boolean ───────────────────────────────────────────

/// Cylinder-cylinder boolean with frame rotation for axis-generic support.
///
/// Rotates both cylinders into a Z-aligned frame, performs the boolean using
/// the Z-assumption logic, then rotates the result back. For Z-aligned inputs,
/// `rotation_to_z` returns the identity matrix — zero overhead.
///
/// Non-parallel cylinders are rejected (elliptical SSI curves are unsupported).
///
/// Ref #24 Barton: frame normalization before boolean.
/// Ref #6 Sugihara-Iri: isometric transform preserves manifoldness.
fn cyl_cyl_boolean(
    cyl_a: &CylinderParams,
    cyl_b: &CylinderParams,
    op: BoolOp,
    id_alloc: &mut dyn FnMut() -> u64,
) -> Result<BooleanResult, KernelError> {
    if !ssi::cyls_parallel(cyl_a, cyl_b) {
        // Try non-parallel analytical SSI path
        return non_parallel_cyl_cyl_boolean(cyl_a, cyl_b, op, id_alloc);
    }

    // Rotate both cylinders to Z-aligned frame using cyl_a's direction
    let m = rotation_to_z(cyl_a.direction);
    let m_inv = mat3_transpose(&m);
    let cyl_a_z = rotate_cyl_params(cyl_a, &m);
    let cyl_b_z = rotate_cyl_params(cyl_b, &m);

    let mut result = cyl_cyl_boolean_z_aligned(&cyl_a_z, &cyl_b_z, op, id_alloc)?;

    // Rotate result back to original frame
    rotate_boolean_result(&mut result, &m_inv);
    Ok(result)
}

/// Non-parallel cylinder-cylinder boolean via analytical SSI.
///
/// Computes the two elliptic intersection curves, then builds a B-Rep with
/// 2 cylindrical patch faces, each bounded by both elliptic curves as
/// outer/inner loops. Supports Union, Subtract, and Intersect for
/// cylinders at angle ≥60°.
///
/// Topology: V=2, E=2, F=2 with inner loops → V-E+F = 2.
/// Face A: outer = ellipse 1, inner = ellipse 2 (hole)
/// Face B: outer = ellipse 2, inner = ellipse 1 (hole)
///
/// Ref: Patrikalakis Ch.5 — cylinder-cylinder SSI.
/// Ref: A15 — analytical primacy, no mesh fallback.
fn non_parallel_cyl_cyl_boolean(
    cyl_a: &CylinderParams,
    cyl_b: &CylinderParams,
    op: BoolOp,
    id_alloc: &mut dyn FnMut() -> u64,
) -> Result<BooleanResult, KernelError> {
    let curves = ssi::cylinder_cylinder_ssi_non_parallel(
        cyl_a.center_bottom,
        cyl_a.direction,
        cyl_a.radius,
        cyl_b.center_bottom,
        cyl_b.direction,
        cyl_b.radius,
    )?;

    if curves.len() != 2 {
        return Err(KernelError::NotSupported {
            operation: "non-parallel cylinder-cylinder boolean: SSI did not produce 2 curves"
                .to_string(),
        });
    }

    // Extract the two ellipses
    let (center, normal_1, major_axis_1, semi_major_1, semi_minor_1) = match &curves[0] {
        ssi::SSICurve::Ellipse {
            center,
            normal,
            major_axis,
            semi_major,
            semi_minor,
        } => (*center, *normal, *major_axis, *semi_major, *semi_minor),
        _ => {
            return Err(KernelError::NotSupported {
                operation: "non-parallel cyl-cyl: expected Ellipse SSI curve".to_string(),
            })
        }
    };
    let (_, normal_2, major_axis_2, semi_major_2, semi_minor_2) = match &curves[1] {
        ssi::SSICurve::Ellipse {
            center: c2,
            normal,
            major_axis,
            semi_major,
            semi_minor,
        } => (*c2, *normal, *major_axis, *semi_major, *semi_minor),
        _ => {
            return Err(KernelError::NotSupported {
                operation: "non-parallel cyl-cyl: expected Ellipse SSI curve".to_string(),
            })
        }
    };

    // Seam vertices at t=0 on each ellipse
    let v0_pos = v3_add(center, v3_scale(major_axis_1, semi_major_1));
    let v1_pos = v3_add(center, v3_scale(major_axis_2, semi_major_2));

    // Build B-Rep: 2 cylindrical faces, each bounded by both elliptic curves.
    // The two SSI ellipses lie in different planes and don't share points,
    // so each face uses one ellipse as outer loop and the other as inner loop (hole).
    let mut arena = TopoArena::new();
    let solid_idx = arena.add_solid();
    let shell_idx = arena.add_shell(solid_idx);
    arena.solids[solid_idx.0].outer_shell = shell_idx;

    let v0 = arena.add_vertex(v0_pos);
    let v1 = arena.add_vertex(v1_pos);

    let face_a = arena.add_face(shell_idx);
    let face_b = arena.add_face(shell_idx);
    arena.shells[shell_idx.0].face = face_a;

    // Face A: outer loop = ellipse 1, inner loop = ellipse 2
    let loop_a_outer = arena.add_loop(face_a);
    let loop_a_inner = arena.add_loop(face_a);
    arena.faces[face_a.0].outer_loop = loop_a_outer;
    arena.faces[face_a.0].inner_loops.push(loop_a_inner);

    // Face B: outer loop = ellipse 2, inner loop = ellipse 1
    let loop_b_outer = arena.add_loop(face_b);
    let loop_b_inner = arena.add_loop(face_b);
    arena.faces[face_b.0].outer_loop = loop_b_outer;
    arena.faces[face_b.0].inner_loops.push(loop_b_inner);

    let (e_ell1, he_ell1_a, he_ell1_b) = arena.add_edge();
    let (e_ell2, he_ell2_a, he_ell2_b) = arena.add_edge();

    // Ellipse 1: self-loop at v0
    arena.half_edges[he_ell1_a.0].origin = v0;
    arena.half_edges[he_ell1_a.0].next = he_ell1_a;
    arena.half_edges[he_ell1_a.0].prev = he_ell1_a;
    arena.half_edges[he_ell1_a.0].loop_ = loop_a_outer;
    arena.loops[loop_a_outer.0].half_edge = he_ell1_a;

    arena.half_edges[he_ell1_b.0].origin = v0;
    arena.half_edges[he_ell1_b.0].next = he_ell1_b;
    arena.half_edges[he_ell1_b.0].prev = he_ell1_b;
    arena.half_edges[he_ell1_b.0].loop_ = loop_b_inner;
    arena.loops[loop_b_inner.0].half_edge = he_ell1_b;

    // Ellipse 2: self-loop at v1
    arena.half_edges[he_ell2_a.0].origin = v1;
    arena.half_edges[he_ell2_a.0].next = he_ell2_a;
    arena.half_edges[he_ell2_a.0].prev = he_ell2_a;
    arena.half_edges[he_ell2_a.0].loop_ = loop_b_outer;
    arena.loops[loop_b_outer.0].half_edge = he_ell2_a;

    arena.half_edges[he_ell2_b.0].origin = v1;
    arena.half_edges[he_ell2_b.0].next = he_ell2_b;
    arena.half_edges[he_ell2_b.0].prev = he_ell2_b;
    arena.half_edges[he_ell2_b.0].loop_ = loop_a_inner;
    arena.loops[loop_a_inner.0].half_edge = he_ell2_b;

    arena.vertices[v0.0].half_edge = Some(he_ell1_a);
    arena.vertices[v1.0].half_edge = Some(he_ell2_a);

    // ── Face geometry ──────────────────────────────────────────
    let mut face_geometry = HashMap::new();

    let flip_b = matches!(op, BoolOp::Subtract);

    face_geometry.insert(
        face_a,
        SurfaceGeom::Cylindrical(Cylinder {
            origin: Point3::from_array(cyl_a.center_bottom),
            axis: Vector3::from_array(cyl_a.direction),
            radius: cyl_a.radius,
        }),
    );
    face_geometry.insert(
        face_b,
        SurfaceGeom::Cylindrical(Cylinder {
            origin: Point3::from_array(cyl_b.center_bottom),
            axis: Vector3::from_array(cyl_b.direction),
            radius: if flip_b { -cyl_b.radius } else { cyl_b.radius },
        }),
    );

    // ── Edge geometry ──────────────────────────────────────────
    let mut edge_geometry: HashMap<EdgeIdx, CurveGeom> = HashMap::new();

    edge_geometry.insert(
        e_ell1,
        CurveGeom::Elliptical(Ellipse3D {
            center: Point3::from_array(center),
            normal: Vector3::from_array(normal_1),
            major_axis: Vector3::from_array(major_axis_1),
            semi_major: semi_major_1,
            semi_minor: semi_minor_1,
        }),
    );
    edge_geometry.insert(
        e_ell2,
        CurveGeom::Elliptical(Ellipse3D {
            center: Point3::from_array(center),
            normal: Vector3::from_array(normal_2),
            major_axis: Vector3::from_array(major_axis_2),
            semi_major: semi_major_2,
            semi_minor: semi_minor_2,
        }),
    );

    // ── Build maps ──────────────────────────────────────────────
    let mut face_map = HashMap::new();
    let mut edge_map = HashMap::new();
    let mut vertex_map = HashMap::new();

    face_map.insert(id_alloc(), face_a);
    face_map.insert(id_alloc(), face_b);

    edge_map.insert(id_alloc(), e_ell1);
    edge_map.insert(id_alloc(), e_ell2);

    vertex_map.insert(id_alloc(), v0);
    vertex_map.insert(id_alloc(), v1);

    Ok(BooleanResult {
        arena,
        face_map,
        edge_map,
        vertex_map,
        face_geometry,
        edge_geometry,
        cached_face_polys: None,
    })
}

/// Z-aligned cylinder-cylinder boolean dispatch (internal).
///
/// Assumes both cylinders have direction ≈ [0,0,±1]. All SSI and build
/// functions use Z-axis assumptions that are valid in this rotated frame.
fn cyl_cyl_boolean_z_aligned(
    cyl_a: &CylinderParams,
    cyl_b: &CylinderParams,
    op: BoolOp,
    id_alloc: &mut dyn FnMut() -> u64,
) -> Result<BooleanResult, KernelError> {
    let disjoint = ssi::cyls_disjoint(cyl_a, cyl_b);

    if disjoint {
        match op {
            BoolOp::Union => build_disjoint_cyl_cyl_union(cyl_a, cyl_b, id_alloc),
            BoolOp::Subtract => build_cyl_result(cyl_a, id_alloc),
            BoolOp::Intersect => Err(KernelError::BooleanFailed {
                reason: "no intersection (disjoint cylinders)".to_string(),
            }),
        }
    } else {
        // Compute z range overlap (direction-aware)
        let (az_min, az_max) = ssi::cyl_z_range(cyl_a);
        let (bz_min, bz_max) = ssi::cyl_z_range(cyl_b);
        let z_min = az_min.max(bz_min);
        let z_max = az_max.min(bz_max);
        if z_max <= z_min + TAU_COINCIDENT {
            return Err(KernelError::BooleanFailed {
                reason: "no Z overlap".to_string(),
            });
        }

        // Compute 2D distance between centers
        let c1 = [cyl_a.center_bottom[0], cyl_a.center_bottom[1]];
        let c2 = [cyl_b.center_bottom[0], cyl_b.center_bottom[1]];
        let r1 = cyl_a.radius;
        let r2 = cyl_b.radius;
        let dx = c2[0] - c1[0];
        let dy = c2[1] - c1[1];
        let d = (dx * dx + dy * dy).sqrt();

        // Concentric cylinders: d ≈ 0, avoid division by zero
        if d < TAU_COINCIDENT {
            return match op {
                BoolOp::Subtract => {
                    if r2 >= r1 - TAU_COINCIDENT {
                        // Tool completely encloses blank: result is empty solid
                        let mut arena = TopoArena::new();
                        let solid_idx = arena.add_solid();
                        let shell_idx = arena.add_shell(solid_idx);
                        arena.solids[solid_idx.0].outer_shell = shell_idx;
                        Ok(BooleanResult {
                            arena,
                            face_map: HashMap::new(),
                            edge_map: HashMap::new(),
                            vertex_map: HashMap::new(),
                            face_geometry: HashMap::new(),
                            edge_geometry: HashMap::new(),
                            cached_face_polys: None,
                        })
                    } else {
                        build_cyl_tube(cyl_a, cyl_b, z_min, z_max, id_alloc)
                    }
                }
                BoolOp::Union => {
                    // Concentric union: keep larger cylinder
                    if r1 >= r2 {
                        build_cyl_result(cyl_a, id_alloc)
                    } else {
                        build_cyl_result(cyl_b, id_alloc)
                    }
                }
                BoolOp::Intersect => {
                    // Concentric intersect: keep smaller cylinder
                    if r1 <= r2 {
                        build_cyl_result(cyl_a, id_alloc)
                    } else {
                        build_cyl_result(cyl_b, id_alloc)
                    }
                }
            };
        }

        // Non-concentric: compute 2D intersection points
        let a = (r1 * r1 - r2 * r2 + d * d) / (2.0 * d);
        let h = (r1 * r1 - a * a).max(0.0).sqrt();
        let ux = dx / d;
        let uy = dy / d;
        let mid_x = c1[0] + a * ux;
        let mid_y = c1[1] + a * uy;
        let p1 = [mid_x - h * uy, mid_y + h * ux];
        let p2 = [mid_x + h * uy, mid_y - h * ux];

        build_partial_cyl_cyl(cyl_a, cyl_b, op, &p1, &p2, z_min, z_max, id_alloc)
    }
}

// ── Clone solid as BooleanResult ───────────────────────────────────────

/// Clone a WaffleSolid into a new BooleanResult with fresh IDs.
pub(super) fn clone_solid_as_result(
    solid: &WaffleSolid,
    id_alloc: &mut dyn FnMut() -> u64,
) -> Result<BooleanResult, KernelError> {
    let mut face_map = HashMap::new();
    let mut edge_map = HashMap::new();
    let mut vertex_map = HashMap::new();

    for &idx in solid.face_map.values() {
        face_map.insert(id_alloc(), idx);
    }
    for &idx in solid.edge_map.values() {
        edge_map.insert(id_alloc(), idx);
    }
    for &idx in solid.vertex_map.values() {
        vertex_map.insert(id_alloc(), idx);
    }

    Ok(BooleanResult {
        arena: solid.arena.clone(),
        face_map,
        edge_map,
        vertex_map,
        face_geometry: solid.face_geometry.clone(),
        edge_geometry: solid.edge_geometry.clone(),
        cached_face_polys: None,
    })
}

// ── Build cylinder B-Rep from CylinderParams ───────────────────────────

/// Build a standalone cylinder B-Rep result (for intersect = cylinder case).
pub(crate) fn build_cyl_result(
    cyl: &CylinderParams,
    id_alloc: &mut dyn FnMut() -> u64,
) -> Result<BooleanResult, KernelError> {
    let center = cyl.center_bottom;
    let r = cyl.radius;
    let depth = cyl.depth;
    let dir = cyl.direction;
    let x_axis = cyl.x_axis;

    let bottom_seam = v3_add(center, v3_scale(x_axis, r));
    let top_seam = v3_add(bottom_seam, v3_scale(dir, depth));
    let top_center = v3_add(center, v3_scale(dir, depth));

    let mut arena = TopoArena::new();
    let solid_idx = arena.add_solid();
    let shell_idx = arena.add_shell(solid_idx);
    arena.solids[solid_idx.0].outer_shell = shell_idx;

    let bottom_face = arena.add_face(shell_idx);
    let top_face = arena.add_face(shell_idx);
    let side_face = arena.add_face(shell_idx);
    arena.shells[shell_idx.0].face = bottom_face;

    let bottom_loop = arena.add_loop(bottom_face);
    let top_loop = arena.add_loop(top_face);
    let side_loop = arena.add_loop(side_face);
    arena.faces[bottom_face.0].outer_loop = bottom_loop;
    arena.faces[top_face.0].outer_loop = top_loop;
    arena.faces[side_face.0].outer_loop = side_loop;

    let v_bottom = arena.add_vertex(bottom_seam);
    let v_top = arena.add_vertex(top_seam);

    let (e_bottom, he_bot_a, he_bot_b) = arena.add_edge();
    let (e_top, he_top_a, he_top_b) = arena.add_edge();
    let (e_seam, he_seam_a, he_seam_b) = arena.add_edge();

    // Bottom cap: self-loop
    arena.half_edges[he_bot_a.0].origin = v_bottom;
    arena.half_edges[he_bot_a.0].next = he_bot_a;
    arena.half_edges[he_bot_a.0].prev = he_bot_a;
    arena.half_edges[he_bot_a.0].loop_ = bottom_loop;
    arena.loops[bottom_loop.0].half_edge = he_bot_a;

    // Top cap: self-loop
    arena.half_edges[he_top_a.0].origin = v_top;
    arena.half_edges[he_top_a.0].next = he_top_a;
    arena.half_edges[he_top_a.0].prev = he_top_a;
    arena.half_edges[he_top_a.0].loop_ = top_loop;
    arena.loops[top_loop.0].half_edge = he_top_a;

    // Side: 4 half-edges: seam_a → top_b → seam_b → bot_b
    arena.half_edges[he_seam_a.0].origin = v_bottom;
    arena.half_edges[he_seam_a.0].next = he_top_b;
    arena.half_edges[he_seam_a.0].prev = he_bot_b;
    arena.half_edges[he_seam_a.0].loop_ = side_loop;

    arena.half_edges[he_top_b.0].origin = v_top;
    arena.half_edges[he_top_b.0].next = he_seam_b;
    arena.half_edges[he_top_b.0].prev = he_seam_a;
    arena.half_edges[he_top_b.0].loop_ = side_loop;

    arena.half_edges[he_seam_b.0].origin = v_top;
    arena.half_edges[he_seam_b.0].next = he_bot_b;
    arena.half_edges[he_seam_b.0].prev = he_top_b;
    arena.half_edges[he_seam_b.0].loop_ = side_loop;

    arena.half_edges[he_bot_b.0].origin = v_bottom;
    arena.half_edges[he_bot_b.0].next = he_seam_a;
    arena.half_edges[he_bot_b.0].prev = he_seam_b;
    arena.half_edges[he_bot_b.0].loop_ = side_loop;

    arena.loops[side_loop.0].half_edge = he_seam_a;

    arena.vertices[v_bottom.0].half_edge = Some(he_bot_a);
    arena.vertices[v_top.0].half_edge = Some(he_top_a);

    // Face geometry
    let mut face_geometry = HashMap::new();
    face_geometry.insert(
        bottom_face,
        SurfaceGeom::Planar(Plane {
            origin: Point3::from_array(center),
            normal: Vector3::from_array(v3_negate(dir)),
        }),
    );
    face_geometry.insert(
        top_face,
        SurfaceGeom::Planar(Plane {
            origin: Point3::from_array(top_center),
            normal: Vector3::from_array(dir),
        }),
    );
    face_geometry.insert(
        side_face,
        SurfaceGeom::Cylindrical(Cylinder {
            origin: Point3::from_array(center),
            axis: Vector3::from_array(dir),
            radius: r,
        }),
    );

    // Edge geometry
    let mut edge_geometry = HashMap::new();
    edge_geometry.insert(
        e_bottom,
        CurveGeom::Circular(Circle3D {
            center: Point3::from_array(center),
            normal: Vector3::from_array(v3_negate(dir)),
            radius: r,
        }),
    );
    edge_geometry.insert(
        e_top,
        CurveGeom::Circular(Circle3D {
            center: Point3::from_array(top_center),
            normal: Vector3::from_array(dir),
            radius: r,
        }),
    );
    edge_geometry.insert(
        e_seam,
        CurveGeom::Linear(Line3D {
            origin: Point3::from_array(bottom_seam),
            direction: Vector3::from_array(v3_scale(dir, depth)),
        }),
    );

    // Build maps
    let mut face_map = HashMap::new();
    let mut edge_map = HashMap::new();
    let mut vertex_map = HashMap::new();
    face_map.insert(id_alloc(), bottom_face);
    face_map.insert(id_alloc(), top_face);
    face_map.insert(id_alloc(), side_face);
    edge_map.insert(id_alloc(), e_bottom);
    edge_map.insert(id_alloc(), e_top);
    edge_map.insert(id_alloc(), e_seam);
    vertex_map.insert(id_alloc(), v_bottom);
    vertex_map.insert(id_alloc(), v_top);

    Ok(BooleanResult {
        arena,
        face_map,
        edge_map,
        vertex_map,
        face_geometry,
        edge_geometry,
        cached_face_polys: None,
    })
}

// ── Build concentric cylinder tube ────────────────────────────────────

/// Build a tube (hollow cylinder) from concentric cylinder subtraction.
///
/// Topology: 4 faces (outer wall, inner wall, top annulus, bottom annulus),
/// 4 edges (outer top circle, outer bottom circle, inner top circle, inner bottom circle),
/// 2 vertices (top seam, bottom seam). Inner loops on cap faces via kemr pattern.
/// V-E+F = 2-4+4 = 2.
fn build_cyl_tube(
    outer_cyl: &CylinderParams,
    inner_cyl: &CylinderParams,
    z_min: f64,
    z_max: f64,
    id_alloc: &mut dyn FnMut() -> u64,
) -> Result<BooleanResult, KernelError> {
    let cx = outer_cyl.center_bottom[0];
    let cy = outer_cyl.center_bottom[1];
    let r_outer = outer_cyl.radius;
    let r_inner = inner_cyl.radius;
    let dir = outer_cyl.direction;

    // Seam points (at +X from center)
    let bot_outer_seam = [cx + r_outer, cy, z_min];
    let top_outer_seam = [cx + r_outer, cy, z_max];
    let bot_inner_seam = [cx + r_inner, cy, z_min];
    let top_inner_seam = [cx + r_inner, cy, z_max];

    let mut arena = TopoArena::new();
    let solid_idx = arena.add_solid();
    let shell_idx = arena.add_shell(solid_idx);
    arena.solids[solid_idx.0].outer_shell = shell_idx;

    // 4 faces
    let face_outer = arena.add_face(shell_idx);
    let face_inner = arena.add_face(shell_idx);
    let face_top = arena.add_face(shell_idx);
    let face_bot = arena.add_face(shell_idx);
    arena.shells[shell_idx.0].face = face_outer;

    // Outer loops for each face
    let loop_outer = arena.add_loop(face_outer);
    let loop_inner = arena.add_loop(face_inner);
    let loop_top_outer = arena.add_loop(face_top);
    let loop_bot_outer = arena.add_loop(face_bot);
    arena.faces[face_outer.0].outer_loop = loop_outer;
    arena.faces[face_inner.0].outer_loop = loop_inner;
    arena.faces[face_top.0].outer_loop = loop_top_outer;
    arena.faces[face_bot.0].outer_loop = loop_bot_outer;

    // Inner loops for annular caps
    let loop_top_inner = arena.add_loop(face_top);
    let loop_bot_inner = arena.add_loop(face_bot);
    arena.faces[face_top.0].inner_loops.push(loop_top_inner);
    arena.faces[face_bot.0].inner_loops.push(loop_bot_inner);

    let v_bot_outer = arena.add_vertex(bot_outer_seam);
    let v_top_outer = arena.add_vertex(top_outer_seam);
    let v_bot_inner = arena.add_vertex(bot_inner_seam);
    let v_top_inner = arena.add_vertex(top_inner_seam);

    let (e_outer_bot, he_obot_a, he_obot_b) = arena.add_edge();
    let (e_outer_top, he_otop_a, he_otop_b) = arena.add_edge();
    let (e_outer_seam, he_oseam_a, he_oseam_b) = arena.add_edge();
    let (e_inner_bot, he_ibot_a, he_ibot_b) = arena.add_edge();
    let (e_inner_top, he_itop_a, he_itop_b) = arena.add_edge();
    let (e_inner_seam, he_iseam_a, he_iseam_b) = arena.add_edge();

    // ── Outer wall loop: oseam_a(bot→top) → otop_b(top→top) → oseam_b(top→bot) → obot_b(bot→bot)
    arena.half_edges[he_oseam_a.0].origin = v_bot_outer;
    arena.half_edges[he_oseam_a.0].next = he_otop_b;
    arena.half_edges[he_oseam_a.0].prev = he_obot_b;
    arena.half_edges[he_oseam_a.0].loop_ = loop_outer;

    arena.half_edges[he_otop_b.0].origin = v_top_outer;
    arena.half_edges[he_otop_b.0].next = he_oseam_b;
    arena.half_edges[he_otop_b.0].prev = he_oseam_a;
    arena.half_edges[he_otop_b.0].loop_ = loop_outer;

    arena.half_edges[he_oseam_b.0].origin = v_top_outer;
    arena.half_edges[he_oseam_b.0].next = he_obot_b;
    arena.half_edges[he_oseam_b.0].prev = he_otop_b;
    arena.half_edges[he_oseam_b.0].loop_ = loop_outer;

    arena.half_edges[he_obot_b.0].origin = v_bot_outer;
    arena.half_edges[he_obot_b.0].next = he_oseam_a;
    arena.half_edges[he_obot_b.0].prev = he_oseam_b;
    arena.half_edges[he_obot_b.0].loop_ = loop_outer;

    arena.loops[loop_outer.0].half_edge = he_oseam_a;

    // ── Inner wall loop: iseam_a(bot→top) → itop_b(top→top) → iseam_b(top→bot) → ibot_b(bot→bot)
    arena.half_edges[he_iseam_a.0].origin = v_bot_inner;
    arena.half_edges[he_iseam_a.0].next = he_itop_b;
    arena.half_edges[he_iseam_a.0].prev = he_ibot_b;
    arena.half_edges[he_iseam_a.0].loop_ = loop_inner;

    arena.half_edges[he_itop_b.0].origin = v_top_inner;
    arena.half_edges[he_itop_b.0].next = he_iseam_b;
    arena.half_edges[he_itop_b.0].prev = he_iseam_a;
    arena.half_edges[he_itop_b.0].loop_ = loop_inner;

    arena.half_edges[he_iseam_b.0].origin = v_top_inner;
    arena.half_edges[he_iseam_b.0].next = he_ibot_b;
    arena.half_edges[he_iseam_b.0].prev = he_itop_b;
    arena.half_edges[he_iseam_b.0].loop_ = loop_inner;

    arena.half_edges[he_ibot_b.0].origin = v_bot_inner;
    arena.half_edges[he_ibot_b.0].next = he_iseam_a;
    arena.half_edges[he_ibot_b.0].prev = he_iseam_b;
    arena.half_edges[he_ibot_b.0].loop_ = loop_inner;

    arena.loops[loop_inner.0].half_edge = he_iseam_a;

    // ── Top cap outer loop: self-loop on outer top circle
    arena.half_edges[he_otop_a.0].origin = v_top_outer;
    arena.half_edges[he_otop_a.0].next = he_otop_a;
    arena.half_edges[he_otop_a.0].prev = he_otop_a;
    arena.half_edges[he_otop_a.0].loop_ = loop_top_outer;
    arena.loops[loop_top_outer.0].half_edge = he_otop_a;

    // ── Top cap inner loop: self-loop on inner top circle
    arena.half_edges[he_itop_a.0].origin = v_top_inner;
    arena.half_edges[he_itop_a.0].next = he_itop_a;
    arena.half_edges[he_itop_a.0].prev = he_itop_a;
    arena.half_edges[he_itop_a.0].loop_ = loop_top_inner;
    arena.loops[loop_top_inner.0].half_edge = he_itop_a;

    // ── Bottom cap outer loop: self-loop on outer bottom circle
    arena.half_edges[he_obot_a.0].origin = v_bot_outer;
    arena.half_edges[he_obot_a.0].next = he_obot_a;
    arena.half_edges[he_obot_a.0].prev = he_obot_a;
    arena.half_edges[he_obot_a.0].loop_ = loop_bot_outer;
    arena.loops[loop_bot_outer.0].half_edge = he_obot_a;

    // ── Bottom cap inner loop: self-loop on inner bottom circle
    arena.half_edges[he_ibot_a.0].origin = v_bot_inner;
    arena.half_edges[he_ibot_a.0].next = he_ibot_a;
    arena.half_edges[he_ibot_a.0].prev = he_ibot_a;
    arena.half_edges[he_ibot_a.0].loop_ = loop_bot_inner;
    arena.loops[loop_bot_inner.0].half_edge = he_ibot_a;

    // ── Vertex half-edge refs
    arena.vertices[v_bot_outer.0].half_edge = Some(he_obot_a);
    arena.vertices[v_top_outer.0].half_edge = Some(he_otop_a);
    arena.vertices[v_bot_inner.0].half_edge = Some(he_ibot_a);
    arena.vertices[v_top_inner.0].half_edge = Some(he_itop_a);

    // ── Face geometry
    let top_center = [cx, cy, z_max];
    let bot_center = [cx, cy, z_min];

    let mut face_geometry = HashMap::new();
    // Z-aligned function: always use [0,0,1] axis and z_min origin for consistent tessellation
    face_geometry.insert(
        face_outer,
        SurfaceGeom::Cylindrical(Cylinder {
            origin: Point3::from_array([cx, cy, z_min]),
            axis: Vector3::new(0.0, 0.0, 1.0),
            radius: r_outer,
        }),
    );
    face_geometry.insert(
        face_inner,
        SurfaceGeom::Cylindrical(Cylinder {
            origin: Point3::from_array([cx, cy, z_min]),
            axis: Vector3::new(0.0, 0.0, 1.0),
            radius: -r_inner, // negative = inward-facing normal
        }),
    );
    face_geometry.insert(
        face_top,
        SurfaceGeom::Planar(Plane {
            origin: Point3::from_array(top_center),
            normal: Vector3::new(0.0, 0.0, 1.0),
        }),
    );
    face_geometry.insert(
        face_bot,
        SurfaceGeom::Planar(Plane {
            origin: Point3::from_array(bot_center),
            normal: Vector3::new(0.0, 0.0, -1.0),
        }),
    );

    // ── Edge geometry
    let mut edge_geometry = HashMap::new();
    edge_geometry.insert(
        e_outer_bot,
        CurveGeom::Circular(Circle3D {
            center: Point3::from_array(bot_center),
            normal: Vector3::from_array(v3_negate(dir)),
            radius: r_outer,
        }),
    );
    edge_geometry.insert(
        e_outer_top,
        CurveGeom::Circular(Circle3D {
            center: Point3::from_array(top_center),
            normal: Vector3::from_array(dir),
            radius: r_outer,
        }),
    );
    edge_geometry.insert(
        e_inner_bot,
        CurveGeom::Circular(Circle3D {
            center: Point3::from_array(bot_center),
            normal: Vector3::new(0.0, 0.0, 1.0),
            radius: r_inner,
        }),
    );
    edge_geometry.insert(
        e_inner_top,
        CurveGeom::Circular(Circle3D {
            center: Point3::from_array(top_center),
            normal: Vector3::new(0.0, 0.0, 1.0),
            radius: r_inner,
        }),
    );
    edge_geometry.insert(
        e_outer_seam,
        CurveGeom::Linear(Line3D {
            origin: Point3::from_array(bot_outer_seam),
            direction: Vector3::from_array([0.0, 0.0, z_max - z_min]),
        }),
    );
    edge_geometry.insert(
        e_inner_seam,
        CurveGeom::Linear(Line3D {
            origin: Point3::from_array(bot_inner_seam),
            direction: Vector3::from_array([0.0, 0.0, z_max - z_min]),
        }),
    );

    // ── Build maps
    let mut face_map = HashMap::new();
    let mut edge_map = HashMap::new();
    let mut vertex_map = HashMap::new();
    face_map.insert(id_alloc(), face_outer);
    face_map.insert(id_alloc(), face_inner);
    face_map.insert(id_alloc(), face_top);
    face_map.insert(id_alloc(), face_bot);
    edge_map.insert(id_alloc(), e_outer_bot);
    edge_map.insert(id_alloc(), e_outer_top);
    edge_map.insert(id_alloc(), e_inner_bot);
    edge_map.insert(id_alloc(), e_inner_top);
    edge_map.insert(id_alloc(), e_outer_seam);
    edge_map.insert(id_alloc(), e_inner_seam);
    vertex_map.insert(id_alloc(), v_bot_outer);
    vertex_map.insert(id_alloc(), v_top_outer);
    vertex_map.insert(id_alloc(), v_bot_inner);
    vertex_map.insert(id_alloc(), v_top_inner);

    Ok(BooleanResult {
        arena,
        face_map,
        edge_map,
        vertex_map,
        face_geometry,
        edge_geometry,
        cached_face_polys: None,
    })
}

// ── Build box-minus-enclosed-cylinder ──────────────────────────────────

/// Build a box with a cylindrical through-hole (enclosed cylinder subtract).
///
/// Uses build_brep_from_polygons for the box (correct edge sharing),
/// then adds inner circle loops and the cylinder side face.
/// Result topology: 4 side faces + 2 holed caps + 1 cylinder inner face = 7 faces.
/// V=10, E=15, F=7 → V-E+F = 2.
fn build_box_minus_enclosed_cyl(
    aabb: &Aabb,
    cyl: &CylinderParams,
    id_alloc: &mut dyn FnMut() -> u64,
) -> Result<BooleanResult, KernelError> {
    let box_z_min = aabb.min[2];
    let box_z_max = aabb.max[2];
    let cx = cyl.center_bottom[0];
    let cy = cyl.center_bottom[1];
    let r = cyl.radius;
    let dir = cyl.direction;

    // Cylinder's actual z-range
    let cyl_z_min = cyl.center_bottom[2];
    let cyl_z_max = cyl_z_min + cyl.depth;

    // Determine if cylinder touches box caps (through-hole vs blind pocket)
    let touches_bot = (cyl_z_min - box_z_min).abs() < TAU_COINCIDENT;
    let touches_top = (cyl_z_max - box_z_max).abs() < TAU_COINCIDENT;

    // Step 1: Build box using build_brep_from_polygons (correct shared edges)
    let box_faces = make_box_face_polys(aabb);
    let tau_weld = TAU_MODEL;
    let mut result = build_brep_from_polygons(&box_faces, tau_weld, id_alloc)?;

    // Step 2: Find bottom and top face indices by normal direction
    let mut face_bot = None;
    let mut face_top = None;
    for (&fi, geom) in &result.face_geometry {
        if let SurfaceGeom::Planar(plane) = geom {
            if plane.normal.z < -0.5 {
                face_bot = Some(fi);
            } else if plane.normal.z > 0.5 {
                face_top = Some(fi);
            }
        }
    }
    let face_bot = face_bot.ok_or(KernelError::BooleanFailed {
        reason: "cannot find bottom face".to_string(),
    })?;
    let face_top = face_top.ok_or(KernelError::BooleanFailed {
        reason: "cannot find top face".to_string(),
    })?;

    // Step 3: Add cylinder seam vertices at the CYLINDER's z-range
    let bot_seam = [cx + r, cy, cyl_z_min];
    let top_seam = [cx + r, cy, cyl_z_max];
    let v_bot_seam = result.arena.add_vertex(bot_seam);
    let v_top_seam = result.arena.add_vertex(top_seam);

    let shell_idx = ShellIdx(0);

    // Step 4: Add inner circle loops for box caps that the cylinder touches,
    // and add cap faces for the blind pocket ends where it doesn't touch.

    // Bottom circle edge
    let (e_bot_circle, he_ibot_a, he_ibot_b) = result.arena.add_edge();

    if touches_bot {
        // Cylinder touches bottom box face → add circular hole to box cap
        let inner_loop_bot = result.arena.add_loop(face_bot);
        result.arena.faces[face_bot.0]
            .inner_loops
            .push(inner_loop_bot);
        result.arena.half_edges[he_ibot_a.0].origin = v_bot_seam;
        result.arena.half_edges[he_ibot_a.0].next = he_ibot_a;
        result.arena.half_edges[he_ibot_a.0].prev = he_ibot_a;
        result.arena.half_edges[he_ibot_a.0].loop_ = inner_loop_bot;
        result.arena.loops[inner_loop_bot.0].half_edge = he_ibot_a;
    } else {
        // Cylinder doesn't touch bottom → add circular cap face (pocket floor)
        let face_cap_bot = result.arena.add_face(shell_idx);
        let loop_cap_bot = result.arena.add_loop(face_cap_bot);
        result.arena.faces[face_cap_bot.0].outer_loop = loop_cap_bot;
        // Self-loop: circle edge bounds the cap face
        result.arena.half_edges[he_ibot_a.0].origin = v_bot_seam;
        result.arena.half_edges[he_ibot_a.0].next = he_ibot_a;
        result.arena.half_edges[he_ibot_a.0].prev = he_ibot_a;
        result.arena.half_edges[he_ibot_a.0].loop_ = loop_cap_bot;
        result.arena.loops[loop_cap_bot.0].half_edge = he_ibot_a;
        // Cap geometry: downward-facing normal (closing the pocket from below)
        result.face_geometry.insert(
            face_cap_bot,
            SurfaceGeom::Planar(Plane {
                origin: Point3::new(cx, cy, cyl_z_min),
                normal: Vector3::new(0.0, 0.0, -1.0),
            }),
        );
        result.face_map.insert(id_alloc(), face_cap_bot);
    }

    // Top circle edge
    let (e_top_circle, he_itop_a, he_itop_b) = result.arena.add_edge();

    if touches_top {
        // Cylinder touches top box face → add circular hole to box cap
        let inner_loop_top = result.arena.add_loop(face_top);
        result.arena.faces[face_top.0]
            .inner_loops
            .push(inner_loop_top);
        result.arena.half_edges[he_itop_a.0].origin = v_top_seam;
        result.arena.half_edges[he_itop_a.0].next = he_itop_a;
        result.arena.half_edges[he_itop_a.0].prev = he_itop_a;
        result.arena.half_edges[he_itop_a.0].loop_ = inner_loop_top;
        result.arena.loops[inner_loop_top.0].half_edge = he_itop_a;
    } else {
        // Cylinder doesn't touch top → add circular cap face (pocket ceiling)
        let face_cap_top = result.arena.add_face(shell_idx);
        let loop_cap_top = result.arena.add_loop(face_cap_top);
        result.arena.faces[face_cap_top.0].outer_loop = loop_cap_top;
        result.arena.half_edges[he_itop_a.0].origin = v_top_seam;
        result.arena.half_edges[he_itop_a.0].next = he_itop_a;
        result.arena.half_edges[he_itop_a.0].prev = he_itop_a;
        result.arena.half_edges[he_itop_a.0].loop_ = loop_cap_top;
        result.arena.loops[loop_cap_top.0].half_edge = he_itop_a;
        // Cap geometry: upward-facing normal (closing the pocket from above)
        result.face_geometry.insert(
            face_cap_top,
            SurfaceGeom::Planar(Plane {
                origin: Point3::new(cx, cy, cyl_z_max),
                normal: Vector3::new(0.0, 0.0, 1.0),
            }),
        );
        result.face_map.insert(id_alloc(), face_cap_top);
    }

    // Step 5: Add cylinder side face
    let face_cyl = result.arena.add_face(shell_idx);
    let loop_cyl = result.arena.add_loop(face_cyl);
    result.arena.faces[face_cyl.0].outer_loop = loop_cyl;

    // Seam edge (vertical)
    let (e_seam, he_seam_a, he_seam_b) = result.arena.add_edge();

    // Cylinder side loop: seam_a → itop_b → seam_b → ibot_b
    result.arena.half_edges[he_seam_a.0].origin = v_bot_seam;
    result.arena.half_edges[he_seam_a.0].next = he_itop_b;
    result.arena.half_edges[he_seam_a.0].prev = he_ibot_b;
    result.arena.half_edges[he_seam_a.0].loop_ = loop_cyl;

    result.arena.half_edges[he_itop_b.0].origin = v_top_seam;
    result.arena.half_edges[he_itop_b.0].next = he_seam_b;
    result.arena.half_edges[he_itop_b.0].prev = he_seam_a;
    result.arena.half_edges[he_itop_b.0].loop_ = loop_cyl;

    result.arena.half_edges[he_seam_b.0].origin = v_top_seam;
    result.arena.half_edges[he_seam_b.0].next = he_ibot_b;
    result.arena.half_edges[he_seam_b.0].prev = he_itop_b;
    result.arena.half_edges[he_seam_b.0].loop_ = loop_cyl;

    result.arena.half_edges[he_ibot_b.0].origin = v_bot_seam;
    result.arena.half_edges[he_ibot_b.0].next = he_seam_a;
    result.arena.half_edges[he_ibot_b.0].prev = he_seam_b;
    result.arena.half_edges[he_ibot_b.0].loop_ = loop_cyl;

    result.arena.loops[loop_cyl.0].half_edge = he_seam_a;

    // Set vertex half-edge refs
    result.arena.vertices[v_bot_seam.0].half_edge = Some(he_ibot_a);
    result.arena.vertices[v_top_seam.0].half_edge = Some(he_itop_a);

    // Step 6: Set face geometry for cylinder face
    // Use negative radius to signal inward-facing normals (hole surface)
    result.face_geometry.insert(
        face_cyl,
        SurfaceGeom::Cylindrical(Cylinder {
            origin: Point3::from_array(cyl.center_bottom),
            axis: Vector3::from_array(dir),
            radius: -r,
        }),
    );

    // Step 7: Set edge geometry for cylinder edges
    result.edge_geometry.insert(
        e_bot_circle,
        CurveGeom::Circular(Circle3D {
            center: Point3::from_array([cx, cy, cyl_z_min]),
            normal: Vector3::new(0.0, 0.0, 1.0),
            radius: r,
        }),
    );
    result.edge_geometry.insert(
        e_top_circle,
        CurveGeom::Circular(Circle3D {
            center: Point3::from_array([cx, cy, cyl_z_max]),
            normal: Vector3::new(0.0, 0.0, 1.0),
            radius: r,
        }),
    );
    result.edge_geometry.insert(
        e_seam,
        CurveGeom::Linear(Line3D {
            origin: Point3::from_array(bot_seam),
            direction: Vector3::from_array([0.0, 0.0, cyl_z_max - cyl_z_min]),
        }),
    );

    // Step 8: Add IDs for new entities
    result.face_map.insert(id_alloc(), face_cyl);
    result.edge_map.insert(id_alloc(), e_bot_circle);
    result.edge_map.insert(id_alloc(), e_top_circle);
    result.edge_map.insert(id_alloc(), e_seam);
    result.vertex_map.insert(id_alloc(), v_bot_seam);
    result.vertex_map.insert(id_alloc(), v_top_seam);

    Ok(result)
}

// ── Cylinder-minus-box boolean ──────────────────────────────────────

/// Cylinder-minus-box boolean dispatch with frame rotation.
///
/// Handles the case where cylinder is operand A and box is operand B in a
/// subtract operation. Mirrors `box_cyl_boolean` but for the inverted operand order.
fn cyl_minus_box_boolean(
    _box_aabb: &Aabb,
    box_solid: &WaffleSolid,
    cyl: &CylinderParams,
    id_alloc: &mut dyn FnMut() -> u64,
) -> Result<BooleanResult, KernelError> {
    let m = rotation_to_z(cyl.direction);
    let m_inv = mat3_transpose(&m);
    let cyl_z = rotate_cyl_params(cyl, &m);
    let box_aabb_z = ssi::compute_rotated_box_aabb(box_solid, &m);

    let xy_enclosed = ssi::box_enclosed_in_cyl(&box_aabb_z, &cyl_z);
    let disjoint = ssi::box_cyl_disjoint(&box_aabb_z, &cyl_z);

    let (cyl_z_min, cyl_z_max) = ssi::cyl_z_range(&cyl_z);
    let z_enclosed = box_aabb_z.min[2] >= cyl_z_min - TAU_COINCIDENT
        && box_aabb_z.max[2] <= cyl_z_max + TAU_COINCIDENT;
    let fully_enclosed = xy_enclosed && z_enclosed;

    if fully_enclosed {
        let mut result = build_cyl_minus_enclosed_box(&box_aabb_z, &cyl_z, id_alloc)?;
        rotate_boolean_result(&mut result, &m_inv);
        Ok(result)
    } else if disjoint {
        // cyl - disjoint box = cyl unchanged
        let mut result = build_cyl_result(&cyl_z, id_alloc)?;
        rotate_boolean_result(&mut result, &m_inv);
        Ok(result)
    } else {
        Err(KernelError::NotSupported {
            operation: "partial cylinder-minus-box subtract".to_string(),
        })
    }
}

/// Build B-Rep for cylinder with a rectangular through-hole (cylinder minus enclosed box).
///
/// Topology: 7 faces:
/// - 1 outer cylinder wall (with seam edge)
/// - 2 annular end caps (outer circle + inner rectangle hole)
/// - 4 inner rectangular wall faces (planar)
///
/// V=10, E=15, F=7 → V-E+F = 10-15+7 = 2 ✓ (Euler)
fn build_cyl_minus_enclosed_box(
    aabb: &Aabb,
    cyl: &CylinderParams,
    id_alloc: &mut dyn FnMut() -> u64,
) -> Result<BooleanResult, KernelError> {
    let (cyl_z_min, cyl_z_max) = ssi::cyl_z_range(cyl);

    // Step 1: Build standalone cylinder as base
    let mut result = build_cyl_result(cyl, id_alloc)?;

    // Step 2: Find bottom and top cap faces by normal direction
    let mut face_bot = None;
    let mut face_top = None;
    for (&fi, geom) in &result.face_geometry {
        if let SurfaceGeom::Planar(plane) = geom {
            if plane.normal.z < -0.5 {
                face_bot = Some(fi);
            } else if plane.normal.z > 0.5 {
                face_top = Some(fi);
            }
        }
    }
    let face_bot = face_bot.ok_or(KernelError::BooleanFailed {
        reason: "cannot find bottom face of cylinder".to_string(),
    })?;
    let face_top = face_top.ok_or(KernelError::BooleanFailed {
        reason: "cannot find top face of cylinder".to_string(),
    })?;

    // Step 3: Box corner positions (4 corners × 2 Z-levels = 8 vertices)
    let bx0 = aabb.min[0];
    let bx1 = aabb.max[0];
    let by0 = aabb.min[1];
    let by1 = aabb.max[1];

    let v_b0 = result.arena.add_vertex([bx0, by0, cyl_z_min]); // bottom-left, bottom
    let v_b1 = result.arena.add_vertex([bx1, by0, cyl_z_min]); // bottom-right, bottom
    let v_b2 = result.arena.add_vertex([bx1, by1, cyl_z_min]); // top-right, bottom
    let v_b3 = result.arena.add_vertex([bx0, by1, cyl_z_min]); // top-left, bottom
    let v_t0 = result.arena.add_vertex([bx0, by0, cyl_z_max]); // bottom-left, top
    let v_t1 = result.arena.add_vertex([bx1, by0, cyl_z_max]); // bottom-right, top
    let v_t2 = result.arena.add_vertex([bx1, by1, cyl_z_max]); // top-right, top
    let v_t3 = result.arena.add_vertex([bx0, by1, cyl_z_max]); // top-left, top

    // Step 4: Inner rectangular loops on bottom and top cap faces
    // Bottom cap inner loop: rectangle winding CW from outside (= CCW from -Z = CW from +Z)
    // For a hole in a face with outward normal -Z, the inner loop winds CW when viewed from -Z,
    // which is CCW when viewed from +Z. The ordering is: b0 → b3 → b2 → b1
    let inner_loop_bot = result.arena.add_loop(face_bot);
    result.arena.faces[face_bot.0]
        .inner_loops
        .push(inner_loop_bot);

    let (e_br0, he_br0_a, he_br0_b) = result.arena.add_edge(); // b0→b3
    let (e_br1, he_br1_a, he_br1_b) = result.arena.add_edge(); // b3→b2
    let (e_br2, he_br2_a, he_br2_b) = result.arena.add_edge(); // b2→b1
    let (e_br3, he_br3_a, he_br3_b) = result.arena.add_edge(); // b1→b0

    // Bottom inner loop: b0 → b3 → b2 → b1 → b0
    result.arena.half_edges[he_br0_a.0].origin = v_b0;
    result.arena.half_edges[he_br0_a.0].next = he_br1_a;
    result.arena.half_edges[he_br0_a.0].prev = he_br3_a;
    result.arena.half_edges[he_br0_a.0].loop_ = inner_loop_bot;

    result.arena.half_edges[he_br1_a.0].origin = v_b3;
    result.arena.half_edges[he_br1_a.0].next = he_br2_a;
    result.arena.half_edges[he_br1_a.0].prev = he_br0_a;
    result.arena.half_edges[he_br1_a.0].loop_ = inner_loop_bot;

    result.arena.half_edges[he_br2_a.0].origin = v_b2;
    result.arena.half_edges[he_br2_a.0].next = he_br3_a;
    result.arena.half_edges[he_br2_a.0].prev = he_br1_a;
    result.arena.half_edges[he_br2_a.0].loop_ = inner_loop_bot;

    result.arena.half_edges[he_br3_a.0].origin = v_b1;
    result.arena.half_edges[he_br3_a.0].next = he_br0_a;
    result.arena.half_edges[he_br3_a.0].prev = he_br2_a;
    result.arena.half_edges[he_br3_a.0].loop_ = inner_loop_bot;

    result.arena.loops[inner_loop_bot.0].half_edge = he_br0_a;

    // Top cap inner loop: rectangle winding CW from outside (= CW from +Z)
    // For a hole in a face with outward normal +Z, the inner loop winds CW from +Z.
    // Ordering: t0 → t1 → t2 → t3
    let inner_loop_top = result.arena.add_loop(face_top);
    result.arena.faces[face_top.0]
        .inner_loops
        .push(inner_loop_top);

    let (e_tr0, he_tr0_a, he_tr0_b) = result.arena.add_edge(); // t0→t1
    let (e_tr1, he_tr1_a, he_tr1_b) = result.arena.add_edge(); // t1→t2
    let (e_tr2, he_tr2_a, he_tr2_b) = result.arena.add_edge(); // t2→t3
    let (e_tr3, he_tr3_a, he_tr3_b) = result.arena.add_edge(); // t3→t0

    // Top inner loop: t0 → t1 → t2 → t3 → t0
    result.arena.half_edges[he_tr0_a.0].origin = v_t0;
    result.arena.half_edges[he_tr0_a.0].next = he_tr1_a;
    result.arena.half_edges[he_tr0_a.0].prev = he_tr3_a;
    result.arena.half_edges[he_tr0_a.0].loop_ = inner_loop_top;

    result.arena.half_edges[he_tr1_a.0].origin = v_t1;
    result.arena.half_edges[he_tr1_a.0].next = he_tr2_a;
    result.arena.half_edges[he_tr1_a.0].prev = he_tr0_a;
    result.arena.half_edges[he_tr1_a.0].loop_ = inner_loop_top;

    result.arena.half_edges[he_tr2_a.0].origin = v_t2;
    result.arena.half_edges[he_tr2_a.0].next = he_tr3_a;
    result.arena.half_edges[he_tr2_a.0].prev = he_tr1_a;
    result.arena.half_edges[he_tr2_a.0].loop_ = inner_loop_top;

    result.arena.half_edges[he_tr3_a.0].origin = v_t3;
    result.arena.half_edges[he_tr3_a.0].next = he_tr0_a;
    result.arena.half_edges[he_tr3_a.0].prev = he_tr2_a;
    result.arena.half_edges[he_tr3_a.0].loop_ = inner_loop_top;

    result.arena.loops[inner_loop_top.0].half_edge = he_tr0_a;

    // Step 5: 4 inner rectangular wall faces (inward-facing normals)
    // Each wall connects a bottom edge to a top edge via 2 vertical edges.
    // Wall 0: front (y=by0, normal +Y inward) — b0→b1 bottom, t1→t0 top
    // Wall 1: right (x=bx1, normal -X inward) — b1→b2 bottom, t2→t1 top
    // Wall 2: back  (y=by1, normal -Y inward) — b2→b3 bottom, t3→t2 top
    // Wall 3: left  (x=bx0, normal +X inward) — b3→b0 bottom, t0→t3 top
    let shell_idx = ShellIdx(0);

    // 4 vertical edges connecting bottom to top corners
    let (e_v0, he_v0_a, he_v0_b) = result.arena.add_edge(); // b0↔t0
    let (e_v1, he_v1_a, he_v1_b) = result.arena.add_edge(); // b1↔t1
    let (e_v2, he_v2_a, he_v2_b) = result.arena.add_edge(); // b2↔t2
    let (e_v3, he_v3_a, he_v3_b) = result.arena.add_edge(); // b3↔t3

    // Wall 0: front face (y=by0), normal pointing inward (+Y)
    // Loop: b1→t1 (v1_a) → t1→t0 (tr0_b) → t0→b0 (v0_b) → b0→b1 (br3_b)
    let face_w0 = result.arena.add_face(shell_idx);
    let loop_w0 = result.arena.add_loop(face_w0);
    result.arena.faces[face_w0.0].outer_loop = loop_w0;

    result.arena.half_edges[he_v1_a.0].origin = v_b1;
    result.arena.half_edges[he_v1_a.0].next = he_tr0_b;
    result.arena.half_edges[he_v1_a.0].prev = he_br3_b;
    result.arena.half_edges[he_v1_a.0].loop_ = loop_w0;

    result.arena.half_edges[he_tr0_b.0].origin = v_t1;
    result.arena.half_edges[he_tr0_b.0].next = he_v0_b;
    result.arena.half_edges[he_tr0_b.0].prev = he_v1_a;
    result.arena.half_edges[he_tr0_b.0].loop_ = loop_w0;

    result.arena.half_edges[he_v0_b.0].origin = v_t0;
    result.arena.half_edges[he_v0_b.0].next = he_br3_b;
    result.arena.half_edges[he_v0_b.0].prev = he_tr0_b;
    result.arena.half_edges[he_v0_b.0].loop_ = loop_w0;

    result.arena.half_edges[he_br3_b.0].origin = v_b0;
    result.arena.half_edges[he_br3_b.0].next = he_v1_a;
    result.arena.half_edges[he_br3_b.0].prev = he_v0_b;
    result.arena.half_edges[he_br3_b.0].loop_ = loop_w0;

    result.arena.loops[loop_w0.0].half_edge = he_v1_a;

    // Wall 1: right face (x=bx1), normal pointing inward (-X)
    // Loop: b2→t2 (v2_a) → t2→t1 (tr1_b) → t1→b1 (v1_b) → b1→b2 (br2_b)
    let face_w1 = result.arena.add_face(shell_idx);
    let loop_w1 = result.arena.add_loop(face_w1);
    result.arena.faces[face_w1.0].outer_loop = loop_w1;

    result.arena.half_edges[he_v2_a.0].origin = v_b2;
    result.arena.half_edges[he_v2_a.0].next = he_tr1_b;
    result.arena.half_edges[he_v2_a.0].prev = he_br2_b;
    result.arena.half_edges[he_v2_a.0].loop_ = loop_w1;

    result.arena.half_edges[he_tr1_b.0].origin = v_t2;
    result.arena.half_edges[he_tr1_b.0].next = he_v1_b;
    result.arena.half_edges[he_tr1_b.0].prev = he_v2_a;
    result.arena.half_edges[he_tr1_b.0].loop_ = loop_w1;

    result.arena.half_edges[he_v1_b.0].origin = v_t1;
    result.arena.half_edges[he_v1_b.0].next = he_br2_b;
    result.arena.half_edges[he_v1_b.0].prev = he_tr1_b;
    result.arena.half_edges[he_v1_b.0].loop_ = loop_w1;

    result.arena.half_edges[he_br2_b.0].origin = v_b1;
    result.arena.half_edges[he_br2_b.0].next = he_v2_a;
    result.arena.half_edges[he_br2_b.0].prev = he_v1_b;
    result.arena.half_edges[he_br2_b.0].loop_ = loop_w1;

    result.arena.loops[loop_w1.0].half_edge = he_v2_a;

    // Wall 2: back face (y=by1), normal pointing inward (-Y)
    // Loop: b3→t3 (v3_a) → t3→t2 (tr2_b) → t2→b2 (v2_b) → b2→b3 (br1_b)
    let face_w2 = result.arena.add_face(shell_idx);
    let loop_w2 = result.arena.add_loop(face_w2);
    result.arena.faces[face_w2.0].outer_loop = loop_w2;

    result.arena.half_edges[he_v3_a.0].origin = v_b3;
    result.arena.half_edges[he_v3_a.0].next = he_tr2_b;
    result.arena.half_edges[he_v3_a.0].prev = he_br1_b;
    result.arena.half_edges[he_v3_a.0].loop_ = loop_w2;

    result.arena.half_edges[he_tr2_b.0].origin = v_t3;
    result.arena.half_edges[he_tr2_b.0].next = he_v2_b;
    result.arena.half_edges[he_tr2_b.0].prev = he_v3_a;
    result.arena.half_edges[he_tr2_b.0].loop_ = loop_w2;

    result.arena.half_edges[he_v2_b.0].origin = v_t2;
    result.arena.half_edges[he_v2_b.0].next = he_br1_b;
    result.arena.half_edges[he_v2_b.0].prev = he_tr2_b;
    result.arena.half_edges[he_v2_b.0].loop_ = loop_w2;

    result.arena.half_edges[he_br1_b.0].origin = v_b2;
    result.arena.half_edges[he_br1_b.0].next = he_v3_a;
    result.arena.half_edges[he_br1_b.0].prev = he_v2_b;
    result.arena.half_edges[he_br1_b.0].loop_ = loop_w2;

    result.arena.loops[loop_w2.0].half_edge = he_v3_a;

    // Wall 3: left face (x=bx0), normal pointing inward (+X)
    // Loop: b0→t0 (v0_a) → t0→t3 (tr3_b) → t3→b3 (v3_b) → b3→b0 (br0_b)
    let face_w3 = result.arena.add_face(shell_idx);
    let loop_w3 = result.arena.add_loop(face_w3);
    result.arena.faces[face_w3.0].outer_loop = loop_w3;

    result.arena.half_edges[he_v0_a.0].origin = v_b0;
    result.arena.half_edges[he_v0_a.0].next = he_tr3_b;
    result.arena.half_edges[he_v0_a.0].prev = he_br0_b;
    result.arena.half_edges[he_v0_a.0].loop_ = loop_w3;

    result.arena.half_edges[he_tr3_b.0].origin = v_t0;
    result.arena.half_edges[he_tr3_b.0].next = he_v3_b;
    result.arena.half_edges[he_tr3_b.0].prev = he_v0_a;
    result.arena.half_edges[he_tr3_b.0].loop_ = loop_w3;

    result.arena.half_edges[he_v3_b.0].origin = v_t3;
    result.arena.half_edges[he_v3_b.0].next = he_br0_b;
    result.arena.half_edges[he_v3_b.0].prev = he_tr3_b;
    result.arena.half_edges[he_v3_b.0].loop_ = loop_w3;

    result.arena.half_edges[he_br0_b.0].origin = v_b3;
    result.arena.half_edges[he_br0_b.0].next = he_v0_a;
    result.arena.half_edges[he_br0_b.0].prev = he_v3_b;
    result.arena.half_edges[he_br0_b.0].loop_ = loop_w3;

    result.arena.loops[loop_w3.0].half_edge = he_v0_a;

    // Step 6: Vertex half-edge references
    result.arena.vertices[v_b0.0].half_edge = Some(he_br0_a);
    result.arena.vertices[v_b1.0].half_edge = Some(he_br3_a);
    result.arena.vertices[v_b2.0].half_edge = Some(he_br2_a);
    result.arena.vertices[v_b3.0].half_edge = Some(he_br1_a);
    result.arena.vertices[v_t0.0].half_edge = Some(he_tr0_a);
    result.arena.vertices[v_t1.0].half_edge = Some(he_tr1_a);
    result.arena.vertices[v_t2.0].half_edge = Some(he_tr2_a);
    result.arena.vertices[v_t3.0].half_edge = Some(he_tr3_a);

    // Step 7: Face geometry for inner walls (inward-facing normals)
    result.face_geometry.insert(
        face_w0,
        SurfaceGeom::Planar(Plane {
            origin: Point3::from_array([bx0, by0, cyl_z_min]),
            normal: Vector3::new(0.0, 1.0, 0.0),
        }),
    );
    result.face_geometry.insert(
        face_w1,
        SurfaceGeom::Planar(Plane {
            origin: Point3::from_array([bx1, by0, cyl_z_min]),
            normal: Vector3::new(-1.0, 0.0, 0.0),
        }),
    );
    result.face_geometry.insert(
        face_w2,
        SurfaceGeom::Planar(Plane {
            origin: Point3::from_array([bx1, by1, cyl_z_min]),
            normal: Vector3::new(0.0, -1.0, 0.0),
        }),
    );
    result.face_geometry.insert(
        face_w3,
        SurfaceGeom::Planar(Plane {
            origin: Point3::from_array([bx0, by0, cyl_z_min]),
            normal: Vector3::new(1.0, 0.0, 0.0),
        }),
    );

    // Step 8: Edge geometry
    // Bottom rect edges (linear)
    let h = cyl_z_max - cyl_z_min;
    result.edge_geometry.insert(
        e_br0,
        CurveGeom::Linear(Line3D {
            origin: Point3::from_array([bx0, by0, cyl_z_min]),
            direction: Vector3::new(0.0, by1 - by0, 0.0),
        }),
    );
    result.edge_geometry.insert(
        e_br1,
        CurveGeom::Linear(Line3D {
            origin: Point3::from_array([bx0, by1, cyl_z_min]),
            direction: Vector3::new(bx1 - bx0, 0.0, 0.0),
        }),
    );
    result.edge_geometry.insert(
        e_br2,
        CurveGeom::Linear(Line3D {
            origin: Point3::from_array([bx1, by1, cyl_z_min]),
            direction: Vector3::new(0.0, by0 - by1, 0.0),
        }),
    );
    result.edge_geometry.insert(
        e_br3,
        CurveGeom::Linear(Line3D {
            origin: Point3::from_array([bx1, by0, cyl_z_min]),
            direction: Vector3::new(bx0 - bx1, 0.0, 0.0),
        }),
    );

    // Top rect edges (linear)
    result.edge_geometry.insert(
        e_tr0,
        CurveGeom::Linear(Line3D {
            origin: Point3::from_array([bx0, by0, cyl_z_max]),
            direction: Vector3::new(bx1 - bx0, 0.0, 0.0),
        }),
    );
    result.edge_geometry.insert(
        e_tr1,
        CurveGeom::Linear(Line3D {
            origin: Point3::from_array([bx1, by0, cyl_z_max]),
            direction: Vector3::new(0.0, by1 - by0, 0.0),
        }),
    );
    result.edge_geometry.insert(
        e_tr2,
        CurveGeom::Linear(Line3D {
            origin: Point3::from_array([bx1, by1, cyl_z_max]),
            direction: Vector3::new(bx0 - bx1, 0.0, 0.0),
        }),
    );
    result.edge_geometry.insert(
        e_tr3,
        CurveGeom::Linear(Line3D {
            origin: Point3::from_array([bx0, by1, cyl_z_max]),
            direction: Vector3::new(0.0, by0 - by1, 0.0),
        }),
    );

    // Vertical edges (linear)
    result.edge_geometry.insert(
        e_v0,
        CurveGeom::Linear(Line3D {
            origin: Point3::from_array([bx0, by0, cyl_z_min]),
            direction: Vector3::new(0.0, 0.0, h),
        }),
    );
    result.edge_geometry.insert(
        e_v1,
        CurveGeom::Linear(Line3D {
            origin: Point3::from_array([bx1, by0, cyl_z_min]),
            direction: Vector3::new(0.0, 0.0, h),
        }),
    );
    result.edge_geometry.insert(
        e_v2,
        CurveGeom::Linear(Line3D {
            origin: Point3::from_array([bx1, by1, cyl_z_min]),
            direction: Vector3::new(0.0, 0.0, h),
        }),
    );
    result.edge_geometry.insert(
        e_v3,
        CurveGeom::Linear(Line3D {
            origin: Point3::from_array([bx0, by1, cyl_z_min]),
            direction: Vector3::new(0.0, 0.0, h),
        }),
    );

    // Step 9: ID maps for new entities
    result.face_map.insert(id_alloc(), face_w0);
    result.face_map.insert(id_alloc(), face_w1);
    result.face_map.insert(id_alloc(), face_w2);
    result.face_map.insert(id_alloc(), face_w3);

    result.edge_map.insert(id_alloc(), e_br0);
    result.edge_map.insert(id_alloc(), e_br1);
    result.edge_map.insert(id_alloc(), e_br2);
    result.edge_map.insert(id_alloc(), e_br3);
    result.edge_map.insert(id_alloc(), e_tr0);
    result.edge_map.insert(id_alloc(), e_tr1);
    result.edge_map.insert(id_alloc(), e_tr2);
    result.edge_map.insert(id_alloc(), e_tr3);
    result.edge_map.insert(id_alloc(), e_v0);
    result.edge_map.insert(id_alloc(), e_v1);
    result.edge_map.insert(id_alloc(), e_v2);
    result.edge_map.insert(id_alloc(), e_v3);

    result.vertex_map.insert(id_alloc(), v_b0);
    result.vertex_map.insert(id_alloc(), v_b1);
    result.vertex_map.insert(id_alloc(), v_b2);
    result.vertex_map.insert(id_alloc(), v_b3);
    result.vertex_map.insert(id_alloc(), v_t0);
    result.vertex_map.insert(id_alloc(), v_t1);
    result.vertex_map.insert(id_alloc(), v_t2);
    result.vertex_map.insert(id_alloc(), v_t3);

    Ok(result)
}

// ── Disjoint unions ────────────────────────────────────────────────────

/// Build a disjoint union of a box and a cylinder.
fn build_disjoint_box_cyl_union(
    aabb: &Aabb,
    cyl: &CylinderParams,
    id_alloc: &mut dyn FnMut() -> u64,
) -> Result<BooleanResult, KernelError> {
    // Build box as polygon faces
    let box_faces = make_box_face_polys(aabb);
    let tau_weld = TAU_MODEL;
    let mut result = build_brep_from_polygons(&box_faces, tau_weld, id_alloc)?;

    // Build cylinder and merge into the same arena
    let cyl_result = build_cyl_result(cyl, id_alloc)?;

    // Merge the cylinder arena into the box result
    merge_brep_into(&mut result, &cyl_result, id_alloc);

    Ok(result)
}

/// Build a box with a cylindrical boss on top (or bottom).
///
/// The cylinder is XY-enclosed in the box and sits on the box top (or bottom) face.
/// Result: box with annular cap face + cylinder wall + cylinder end cap.
///
/// Topology: 4 box side faces + 1 box opposite cap + 1 annular cap + 1 cyl wall + 1 cyl cap = 8 faces.
fn build_box_with_cyl_boss(
    aabb: &Aabb,
    cyl: &CylinderParams,
    on_top: bool,
    id_alloc: &mut dyn FnMut() -> u64,
) -> Result<BooleanResult, KernelError> {
    let cx = cyl.center_bottom[0];
    let cy = cyl.center_bottom[1];
    let r = cyl.radius;
    let (cyl_z_min, cyl_z_max) = ssi::cyl_z_range(cyl);

    // Build box as polygon faces
    let box_faces = make_box_face_polys(aabb);
    let tau_weld = TAU_MODEL;
    let mut result = build_brep_from_polygons(&box_faces, tau_weld, id_alloc)?;

    // Find the face to punch the hole in (top or bottom)
    let mut face_punch = None;
    let punch_z = if on_top { aabb.max[2] } else { aabb.min[2] };
    for (&fi, geom) in &result.face_geometry {
        if let SurfaceGeom::Planar(plane) = geom {
            let matches = if on_top {
                plane.normal.z > 0.5
            } else {
                plane.normal.z < -0.5
            };
            if matches {
                face_punch = Some(fi);
            }
        }
    }
    let face_punch = face_punch.ok_or(KernelError::BooleanFailed {
        reason: "cannot find face to punch for boss".to_string(),
    })?;

    // Cylinder end Z (the end away from the box)
    let cyl_end_z = if on_top { cyl_z_max } else { cyl_z_min };
    let cyl_dir = if on_top {
        [0.0, 0.0, 1.0]
    } else {
        [0.0, 0.0, -1.0]
    };

    // Add cylinder seam vertices at the punched face and at the cyl end
    let punch_seam = [cx + r, cy, punch_z];
    let end_seam = [cx + r, cy, cyl_end_z];
    let v_punch_seam = result.arena.add_vertex(punch_seam);
    let v_end_seam = result.arena.add_vertex(end_seam);

    // Add inner loop to the punched face (annular hole)
    let inner_loop = result.arena.add_loop(face_punch);
    result.arena.faces[face_punch.0]
        .inner_loops
        .push(inner_loop);

    // Inner circle self-loop at punch face
    let (e_punch_circle, he_punch_a, he_punch_b) = result.arena.add_edge();
    result.arena.half_edges[he_punch_a.0].origin = v_punch_seam;
    result.arena.half_edges[he_punch_a.0].next = he_punch_a;
    result.arena.half_edges[he_punch_a.0].prev = he_punch_a;
    result.arena.half_edges[he_punch_a.0].loop_ = inner_loop;
    result.arena.loops[inner_loop.0].half_edge = he_punch_a;

    // End cap circle
    let (e_end_circle, he_end_a, he_end_b) = result.arena.add_edge();

    // End cap face
    let shell_idx = ShellIdx(0);
    let face_end_cap = result.arena.add_face(shell_idx);
    let loop_end_cap = result.arena.add_loop(face_end_cap);
    result.arena.faces[face_end_cap.0].outer_loop = loop_end_cap;

    // End cap: self-loop
    result.arena.half_edges[he_end_a.0].origin = v_end_seam;
    result.arena.half_edges[he_end_a.0].next = he_end_a;
    result.arena.half_edges[he_end_a.0].prev = he_end_a;
    result.arena.half_edges[he_end_a.0].loop_ = loop_end_cap;
    result.arena.loops[loop_end_cap.0].half_edge = he_end_a;

    // Cylinder side face
    let face_cyl = result.arena.add_face(shell_idx);
    let loop_cyl = result.arena.add_loop(face_cyl);
    result.arena.faces[face_cyl.0].outer_loop = loop_cyl;

    // Seam edge (vertical)
    let (e_seam, he_seam_a, he_seam_b) = result.arena.add_edge();

    // Cylinder side loop: seam_a(punch→end) → end_b(end→end) → seam_b(end→punch) → punch_b(punch→punch)
    result.arena.half_edges[he_seam_a.0].origin = v_punch_seam;
    result.arena.half_edges[he_seam_a.0].next = he_end_b;
    result.arena.half_edges[he_seam_a.0].prev = he_punch_b;
    result.arena.half_edges[he_seam_a.0].loop_ = loop_cyl;

    result.arena.half_edges[he_end_b.0].origin = v_end_seam;
    result.arena.half_edges[he_end_b.0].next = he_seam_b;
    result.arena.half_edges[he_end_b.0].prev = he_seam_a;
    result.arena.half_edges[he_end_b.0].loop_ = loop_cyl;

    result.arena.half_edges[he_seam_b.0].origin = v_end_seam;
    result.arena.half_edges[he_seam_b.0].next = he_punch_b;
    result.arena.half_edges[he_seam_b.0].prev = he_end_b;
    result.arena.half_edges[he_seam_b.0].loop_ = loop_cyl;

    result.arena.half_edges[he_punch_b.0].origin = v_punch_seam;
    result.arena.half_edges[he_punch_b.0].next = he_seam_a;
    result.arena.half_edges[he_punch_b.0].prev = he_seam_b;
    result.arena.half_edges[he_punch_b.0].loop_ = loop_cyl;

    result.arena.loops[loop_cyl.0].half_edge = he_seam_a;

    // Vertex half-edge refs
    result.arena.vertices[v_punch_seam.0].half_edge = Some(he_punch_a);
    result.arena.vertices[v_end_seam.0].half_edge = Some(he_end_a);

    // Face geometry
    result.face_geometry.insert(
        face_cyl,
        SurfaceGeom::Cylindrical(Cylinder {
            origin: Point3::from_array([cx, cy, punch_z]),
            axis: Vector3::from_array(cyl_dir),
            radius: r,
        }),
    );
    result.face_geometry.insert(
        face_end_cap,
        SurfaceGeom::Planar(Plane {
            origin: Point3::from_array([cx, cy, cyl_end_z]),
            normal: Vector3::from_array(cyl_dir),
        }),
    );

    // Edge geometry
    result.edge_geometry.insert(
        e_punch_circle,
        CurveGeom::Circular(Circle3D {
            center: Point3::from_array([cx, cy, punch_z]),
            normal: Vector3::new(0.0, 0.0, 1.0),
            radius: r,
        }),
    );
    result.edge_geometry.insert(
        e_end_circle,
        CurveGeom::Circular(Circle3D {
            center: Point3::from_array([cx, cy, cyl_end_z]),
            normal: Vector3::from_array(cyl_dir),
            radius: r,
        }),
    );
    let seam_height = (cyl_end_z - punch_z).abs();
    result.edge_geometry.insert(
        e_seam,
        CurveGeom::Linear(Line3D {
            origin: Point3::from_array(punch_seam),
            direction: Vector3::from_array(v3_scale(cyl_dir, seam_height)),
        }),
    );

    // Add IDs for new entities
    result.face_map.insert(id_alloc(), face_cyl);
    result.face_map.insert(id_alloc(), face_end_cap);
    result.edge_map.insert(id_alloc(), e_punch_circle);
    result.edge_map.insert(id_alloc(), e_end_circle);
    result.edge_map.insert(id_alloc(), e_seam);
    result.vertex_map.insert(id_alloc(), v_punch_seam);
    result.vertex_map.insert(id_alloc(), v_end_seam);

    Ok(result)
}

/// Build a disjoint union of two cylinders.
fn build_disjoint_cyl_cyl_union(
    cyl_a: &CylinderParams,
    cyl_b: &CylinderParams,
    id_alloc: &mut dyn FnMut() -> u64,
) -> Result<BooleanResult, KernelError> {
    let mut result = build_cyl_result(cyl_a, id_alloc)?;
    let cyl_b_result = build_cyl_result(cyl_b, id_alloc)?;
    merge_brep_into(&mut result, &cyl_b_result, id_alloc);
    Ok(result)
}

/// Create FacePoly list for an axis-aligned box.
/// Vertex winding is CCW when viewed from the outward normal direction.
pub(super) fn make_box_face_polys(aabb: &Aabb) -> Vec<FacePoly> {
    let mn = aabb.min;
    let mx = aabb.max;
    vec![
        // Bottom (z=min, normal -Z): CCW from -Z
        FacePoly {
            verts: vec![
                [mn[0], mx[1], mn[2]],
                [mx[0], mx[1], mn[2]],
                [mx[0], mn[1], mn[2]],
                [mn[0], mn[1], mn[2]],
            ],
            normal: [0.0, 0.0, -1.0],
            origin: mn,
            surface_geom: None,
        },
        // Top (z=max, normal +Z): CCW from +Z
        FacePoly {
            verts: vec![
                [mn[0], mn[1], mx[2]],
                [mx[0], mn[1], mx[2]],
                [mx[0], mx[1], mx[2]],
                [mn[0], mx[1], mx[2]],
            ],
            normal: [0.0, 0.0, 1.0],
            origin: [mn[0], mn[1], mx[2]],
            surface_geom: None,
        },
        // Front (y=min, normal -Y): CCW from -Y
        FacePoly {
            verts: vec![
                [mx[0], mn[1], mn[2]],
                [mx[0], mn[1], mx[2]],
                [mn[0], mn[1], mx[2]],
                [mn[0], mn[1], mn[2]],
            ],
            normal: [0.0, -1.0, 0.0],
            origin: [mn[0], mn[1], mn[2]],
            surface_geom: None,
        },
        // Back (y=max, normal +Y): CCW from +Y
        FacePoly {
            verts: vec![
                [mn[0], mx[1], mn[2]],
                [mn[0], mx[1], mx[2]],
                [mx[0], mx[1], mx[2]],
                [mx[0], mx[1], mn[2]],
            ],
            normal: [0.0, 1.0, 0.0],
            origin: [mn[0], mx[1], mn[2]],
            surface_geom: None,
        },
        // Right (x=max, normal +X): CCW from +X
        FacePoly {
            verts: vec![
                [mx[0], mx[1], mn[2]],
                [mx[0], mx[1], mx[2]],
                [mx[0], mn[1], mx[2]],
                [mx[0], mn[1], mn[2]],
            ],
            normal: [1.0, 0.0, 0.0],
            origin: [mx[0], mn[1], mn[2]],
            surface_geom: None,
        },
        // Left (x=min, normal -X): CCW from -X
        FacePoly {
            verts: vec![
                [mn[0], mn[1], mn[2]],
                [mn[0], mn[1], mx[2]],
                [mn[0], mx[1], mx[2]],
                [mn[0], mx[1], mn[2]],
            ],
            normal: [-1.0, 0.0, 0.0],
            origin: [mn[0], mn[1], mn[2]],
            surface_geom: None,
        },
    ]
}

/// Merge a second BooleanResult into the first (for disjoint unions).
pub(super) fn merge_brep_into(
    target: &mut BooleanResult,
    source: &BooleanResult,
    id_alloc: &mut dyn FnMut() -> u64,
) {
    let v_offset = target.arena.vertices.len();
    let he_offset = target.arena.half_edges.len();
    let e_offset = target.arena.edges.len();
    let l_offset = target.arena.loops.len();
    let f_offset = target.arena.faces.len();
    let sh_offset = target.arena.shells.len();
    let so_offset = target.arena.solids.len();

    // Copy vertices with offset
    for v in &source.arena.vertices {
        let mut vc = v.clone();
        if let Some(ref mut he) = vc.half_edge {
            he.0 += he_offset;
        }
        target.arena.vertices.push(vc);
    }

    // Copy half-edges with offset
    for he in &source.arena.half_edges {
        let mut hec = he.clone();
        hec.origin.0 += v_offset;
        hec.edge.0 += e_offset;
        hec.twin.0 += he_offset;
        hec.next.0 += he_offset;
        hec.prev.0 += he_offset;
        hec.loop_.0 += l_offset;
        target.arena.half_edges.push(hec);
    }

    // Copy edges with offset
    for e in &source.arena.edges {
        let mut ec = e.clone();
        ec.half_edge.0 += he_offset;
        target.arena.edges.push(ec);
    }

    // Copy loops with offset
    for l in &source.arena.loops {
        let mut lc = l.clone();
        lc.half_edge.0 += he_offset;
        lc.face.0 += f_offset;
        target.arena.loops.push(lc);
    }

    // Copy faces with offset
    for f in &source.arena.faces {
        let mut fc = f.clone();
        fc.outer_loop.0 += l_offset;
        fc.inner_loops.iter_mut().for_each(|l| l.0 += l_offset);
        fc.shell.0 += sh_offset;
        target.arena.faces.push(fc);
    }

    // Copy shells with offset
    for s in &source.arena.shells {
        let mut sc = s.clone();
        sc.face.0 += f_offset;
        sc.solid.0 += so_offset;
        target.arena.shells.push(sc);
    }

    // Copy solids with offset
    for s in &source.arena.solids {
        let mut sc = s.clone();
        sc.outer_shell.0 += sh_offset;
        sc.inner_shells.iter_mut().for_each(|s| s.0 += sh_offset);
        target.arena.solids.push(sc);
    }

    // Copy face geometry with offset
    for (&fi, geom) in &source.face_geometry {
        target
            .face_geometry
            .insert(FaceIdx(fi.0 + f_offset), geom.clone());
    }

    // Copy edge geometry with offset
    for (&ei, geom) in &source.edge_geometry {
        target
            .edge_geometry
            .insert(EdgeIdx(ei.0 + e_offset), geom.clone());
    }

    // Add new face/edge/vertex maps with fresh IDs
    for &fi in source.face_map.values() {
        target.face_map.insert(id_alloc(), FaceIdx(fi.0 + f_offset));
    }
    for &ei in source.edge_map.values() {
        target.edge_map.insert(id_alloc(), EdgeIdx(ei.0 + e_offset));
    }
    for &vi in source.vertex_map.values() {
        target
            .vertex_map
            .insert(id_alloc(), VertexIdx(vi.0 + v_offset));
    }
}

// ── Partial cylinder-cylinder boolean ──────────────────────────────────

/// Build the result of a partial overlap cylinder-cylinder boolean.
///
/// The two cylinders share the same Z range and have 2 intersection points
/// in the XY plane. The result has 4 vertices (2 at z_min, 2 at z_max),
/// 6 edges (2 vertical lines + 4 arcs), and 4 faces (2 cylindrical + 2 planar caps).
#[allow(clippy::too_many_arguments)]
fn build_partial_cyl_cyl(
    cyl_a: &CylinderParams,
    cyl_b: &CylinderParams,
    op: BoolOp,
    p1: &[f64; 2],
    p2: &[f64; 2],
    z_min: f64,
    z_max: f64,
    id_alloc: &mut dyn FnMut() -> u64,
) -> Result<BooleanResult, KernelError> {
    // 4 vertices: 2 intersection points at z_min and z_max
    let v0_pos = [p1[0], p1[1], z_min]; // intersection point 1, bottom
    let v1_pos = [p2[0], p2[1], z_min]; // intersection point 2, bottom
    let v2_pos = [p1[0], p1[1], z_max]; // intersection point 1, top
    let v3_pos = [p2[0], p2[1], z_max]; // intersection point 2, top

    let ca = [cyl_a.center_bottom[0], cyl_a.center_bottom[1]];
    let cb = [cyl_b.center_bottom[0], cyl_b.center_bottom[1]];
    let ra = cyl_a.radius;
    let rb = cyl_b.radius;

    // Compute arc angles for each cylinder
    let angle_a1 = (p1[1] - ca[1]).atan2(p1[0] - ca[0]);
    let angle_a2 = (p2[1] - ca[1]).atan2(p2[0] - ca[0]);
    let angle_b1 = (p1[1] - cb[1]).atan2(p1[0] - cb[0]);
    let angle_b2 = (p2[1] - cb[1]).atan2(p2[0] - cb[0]);

    // "Outside" arcs: the part of each cylinder NOT inside the other
    let sweep_a_short = normalize_angle(angle_a2 - angle_a1);
    let sweep_a_long = std::f64::consts::TAU - sweep_a_short;

    // Midpoint of short arc from p1 to p2 on cyl_a
    let mid_a_short_angle = angle_a1 + sweep_a_short / 2.0;
    let mid_a_short = [
        ca[0] + ra * mid_a_short_angle.cos(),
        ca[1] + ra * mid_a_short_angle.sin(),
    ];
    let mid_a_short_in_b =
        (mid_a_short[0] - cb[0]).powi(2) + (mid_a_short[1] - cb[1]).powi(2) < rb * rb;

    // The outside arc of A is the one NOT inside B
    let (a_outside_start, a_outside_sweep, a_inside_start, a_inside_sweep) = if mid_a_short_in_b {
        (angle_a2, sweep_a_long, angle_a1, sweep_a_short)
    } else {
        (angle_a1, sweep_a_short, angle_a2, sweep_a_long)
    };

    // Same for cyl_b
    let sweep_b_short = normalize_angle(angle_b2 - angle_b1);
    let sweep_b_long = std::f64::consts::TAU - sweep_b_short;

    let mid_b_short_angle = angle_b1 + sweep_b_short / 2.0;
    let mid_b_short = [
        cb[0] + rb * mid_b_short_angle.cos(),
        cb[1] + rb * mid_b_short_angle.sin(),
    ];
    let mid_b_short_in_a =
        (mid_b_short[0] - ca[0]).powi(2) + (mid_b_short[1] - ca[1]).powi(2) < ra * ra;

    let (b_outside_start, b_outside_sweep, b_inside_start, b_inside_sweep) = if mid_b_short_in_a {
        (angle_b2, sweep_b_long, angle_b1, sweep_b_short)
    } else {
        (angle_b1, sweep_b_short, angle_b2, sweep_b_long)
    };

    // Select which arcs to use based on operation
    struct ArcSpec {
        start_angle: f64,
        sweep: f64,
    }

    let make_arc = |_c: [f64; 2], _r: f64, start: f64, sweep: f64, _origin: [f64; 3]| -> ArcSpec {
        ArcSpec {
            start_angle: start,
            sweep,
        }
    };

    // For union: A_outside + B_outside
    // For subtract: A_outside + B_inside (flipped)
    // For intersect: A_inside + B_inside
    let (arc1, arc2, flip_arc2) = match op {
        BoolOp::Union => (
            make_arc(
                ca,
                ra,
                a_outside_start,
                a_outside_sweep,
                cyl_a.center_bottom,
            ),
            make_arc(
                cb,
                rb,
                b_outside_start,
                b_outside_sweep,
                cyl_b.center_bottom,
            ),
            false,
        ),
        BoolOp::Subtract => (
            make_arc(
                ca,
                ra,
                a_outside_start,
                a_outside_sweep,
                cyl_a.center_bottom,
            ),
            make_arc(cb, rb, b_inside_start, b_inside_sweep, cyl_b.center_bottom),
            true,
        ),
        BoolOp::Intersect => (
            make_arc(ca, ra, a_inside_start, a_inside_sweep, cyl_a.center_bottom),
            make_arc(cb, rb, b_inside_start, b_inside_sweep, cyl_b.center_bottom),
            false,
        ),
    };

    // Build B-Rep: 4 vertices, 6 edges, 4 faces
    let mut arena = TopoArena::new();
    let solid_idx = arena.add_solid();
    let shell_idx = arena.add_shell(solid_idx);
    arena.solids[solid_idx.0].outer_shell = shell_idx;

    let v0 = arena.add_vertex(v0_pos);
    let v1 = arena.add_vertex(v1_pos);
    let v2 = arena.add_vertex(v2_pos);
    let v3 = arena.add_vertex(v3_pos);

    // 4 faces: cyl_a patch, cyl_b patch, top cap, bottom cap
    let face_cyl_a = arena.add_face(shell_idx);
    let face_cyl_b = arena.add_face(shell_idx);
    let face_top = arena.add_face(shell_idx);
    let face_bot = arena.add_face(shell_idx);
    arena.shells[shell_idx.0].face = face_cyl_a;

    let loop_cyl_a = arena.add_loop(face_cyl_a);
    let loop_cyl_b = arena.add_loop(face_cyl_b);
    let loop_top = arena.add_loop(face_top);
    let loop_bot = arena.add_loop(face_bot);
    arena.faces[face_cyl_a.0].outer_loop = loop_cyl_a;
    arena.faces[face_cyl_b.0].outer_loop = loop_cyl_b;
    arena.faces[face_top.0].outer_loop = loop_top;
    arena.faces[face_bot.0].outer_loop = loop_bot;

    // 6 edges
    let (e_line_p1, he_lp1_a, he_lp1_b) = arena.add_edge();
    let (e_line_p2, he_lp2_a, he_lp2_b) = arena.add_edge();
    let (e_arc_a_bot, he_aab_a, he_aab_b) = arena.add_edge();
    let (e_arc_a_top, he_aat_a, he_aat_b) = arena.add_edge();
    let (e_arc_b_bot, he_abb_a, he_abb_b) = arena.add_edge();
    let (e_arc_b_top, he_abt_a, he_abt_b) = arena.add_edge();

    // Cyl_a patch loop: arc_a_bot(v0→v1) → line_p2(v1→v3) → arc_a_top_rev(v3→v2) → line_p1_rev(v2→v0)
    arena.half_edges[he_aab_a.0].origin = v0;
    arena.half_edges[he_aab_a.0].next = he_lp2_a;
    arena.half_edges[he_aab_a.0].prev = he_lp1_b;
    arena.half_edges[he_aab_a.0].loop_ = loop_cyl_a;

    arena.half_edges[he_lp2_a.0].origin = v1;
    arena.half_edges[he_lp2_a.0].next = he_aat_b;
    arena.half_edges[he_lp2_a.0].prev = he_aab_a;
    arena.half_edges[he_lp2_a.0].loop_ = loop_cyl_a;

    arena.half_edges[he_aat_b.0].origin = v3;
    arena.half_edges[he_aat_b.0].next = he_lp1_b;
    arena.half_edges[he_aat_b.0].prev = he_lp2_a;
    arena.half_edges[he_aat_b.0].loop_ = loop_cyl_a;

    arena.half_edges[he_lp1_b.0].origin = v2;
    arena.half_edges[he_lp1_b.0].next = he_aab_a;
    arena.half_edges[he_lp1_b.0].prev = he_aat_b;
    arena.half_edges[he_lp1_b.0].loop_ = loop_cyl_a;

    arena.loops[loop_cyl_a.0].half_edge = he_aab_a;

    // Cyl_b patch loop: arc_b_bot(v1→v0) → line_p1(v0→v2) → arc_b_top_rev(v2→v3) → line_p2_rev(v3→v1)
    arena.half_edges[he_abb_a.0].origin = v1;
    arena.half_edges[he_abb_a.0].next = he_lp1_a;
    arena.half_edges[he_abb_a.0].prev = he_lp2_b;
    arena.half_edges[he_abb_a.0].loop_ = loop_cyl_b;

    arena.half_edges[he_lp1_a.0].origin = v0;
    arena.half_edges[he_lp1_a.0].next = he_abt_b;
    arena.half_edges[he_lp1_a.0].prev = he_abb_a;
    arena.half_edges[he_lp1_a.0].loop_ = loop_cyl_b;

    arena.half_edges[he_abt_b.0].origin = v2;
    arena.half_edges[he_abt_b.0].next = he_lp2_b;
    arena.half_edges[he_abt_b.0].prev = he_lp1_a;
    arena.half_edges[he_abt_b.0].loop_ = loop_cyl_b;

    arena.half_edges[he_lp2_b.0].origin = v3;
    arena.half_edges[he_lp2_b.0].next = he_abb_a;
    arena.half_edges[he_lp2_b.0].prev = he_abt_b;
    arena.half_edges[he_lp2_b.0].loop_ = loop_cyl_b;

    arena.loops[loop_cyl_b.0].half_edge = he_abb_a;

    // Bottom cap loop: arc_a_bot_rev(v1→v0) → arc_b_bot_rev(v0→v1)
    arena.half_edges[he_aab_b.0].origin = v1;
    arena.half_edges[he_aab_b.0].next = he_abb_b;
    arena.half_edges[he_aab_b.0].prev = he_abb_b;
    arena.half_edges[he_aab_b.0].loop_ = loop_bot;

    arena.half_edges[he_abb_b.0].origin = v0;
    arena.half_edges[he_abb_b.0].next = he_aab_b;
    arena.half_edges[he_abb_b.0].prev = he_aab_b;
    arena.half_edges[he_abb_b.0].loop_ = loop_bot;

    arena.loops[loop_bot.0].half_edge = he_aab_b;

    // Top cap loop: arc_a_top(v2→v3) → arc_b_top(v3→v2)
    arena.half_edges[he_aat_a.0].origin = v2;
    arena.half_edges[he_aat_a.0].next = he_abt_a;
    arena.half_edges[he_aat_a.0].prev = he_abt_a;
    arena.half_edges[he_aat_a.0].loop_ = loop_top;

    arena.half_edges[he_abt_a.0].origin = v3;
    arena.half_edges[he_abt_a.0].next = he_aat_a;
    arena.half_edges[he_abt_a.0].prev = he_aat_a;
    arena.half_edges[he_abt_a.0].loop_ = loop_top;

    arena.loops[loop_top.0].half_edge = he_aat_a;

    // Vertex half-edge refs
    arena.vertices[v0.0].half_edge = Some(he_aab_a);
    arena.vertices[v1.0].half_edge = Some(he_abb_a);
    arena.vertices[v2.0].half_edge = Some(he_lp1_b);
    arena.vertices[v3.0].half_edge = Some(he_aat_b);

    // ── Face geometry ───────────────────────────────────────────────
    let mut face_geometry = HashMap::new();
    let origin_a_z = [cyl_a.center_bottom[0], cyl_a.center_bottom[1], z_min];
    face_geometry.insert(
        face_cyl_a,
        SurfaceGeom::Cylindrical(Cylinder {
            origin: Point3::from_array(origin_a_z),
            axis: Vector3::new(0.0, 0.0, 1.0),
            radius: ra,
        }),
    );
    let origin_b_z = [cyl_b.center_bottom[0], cyl_b.center_bottom[1], z_min];
    let cyl_b_geom = SurfaceGeom::Cylindrical(Cylinder {
        origin: Point3::from_array(origin_b_z),
        axis: Vector3::new(0.0, 0.0, 1.0),
        radius: if flip_arc2 { -rb } else { rb },
    });
    face_geometry.insert(face_cyl_b, cyl_b_geom);
    face_geometry.insert(
        face_bot,
        SurfaceGeom::Planar(Plane {
            origin: Point3::new(0.0, 0.0, z_min),
            normal: Vector3::new(0.0, 0.0, -1.0),
        }),
    );
    face_geometry.insert(
        face_top,
        SurfaceGeom::Planar(Plane {
            origin: Point3::new(0.0, 0.0, z_max),
            normal: Vector3::new(0.0, 0.0, 1.0),
        }),
    );

    // ── Edge geometry ───────────────────────────────────────────────

    let mut edge_geometry: HashMap<EdgeIdx, CurveGeom> = HashMap::new();

    // Vertical lines
    edge_geometry.insert(
        e_line_p1,
        CurveGeom::Linear(Line3D {
            origin: Point3::from_array(v0_pos),
            direction: Vector3::new(0.0, 0.0, z_max - z_min),
        }),
    );
    edge_geometry.insert(
        e_line_p2,
        CurveGeom::Linear(Line3D {
            origin: Point3::from_array(v1_pos),
            direction: Vector3::new(0.0, 0.0, z_max - z_min),
        }),
    );

    // Arc edges
    let make_arc_geom =
        |center_2d: [f64; 2], radius: f64, start_angle: f64, sweep: f64, z: f64| -> Arc3D {
            let sp = [
                center_2d[0] + radius * start_angle.cos(),
                center_2d[1] + radius * start_angle.sin(),
                z,
            ];
            Arc3D {
                center: Point3::new(center_2d[0], center_2d[1], z),
                normal: Vector3::new(0.0, 0.0, 1.0),
                radius,
                start_point: Point3::from_array(sp),
                sweep_angle: sweep,
            }
        };

    edge_geometry.insert(
        e_arc_a_bot,
        CurveGeom::Arc(make_arc_geom(ca, ra, arc1.start_angle, arc1.sweep, z_min)),
    );
    edge_geometry.insert(
        e_arc_a_top,
        CurveGeom::Arc(make_arc_geom(ca, ra, arc1.start_angle, arc1.sweep, z_max)),
    );
    edge_geometry.insert(
        e_arc_b_bot,
        CurveGeom::Arc(make_arc_geom(cb, rb, arc2.start_angle, arc2.sweep, z_min)),
    );
    edge_geometry.insert(
        e_arc_b_top,
        CurveGeom::Arc(make_arc_geom(cb, rb, arc2.start_angle, arc2.sweep, z_max)),
    );

    // ── Build maps ──────────────────────────────────────────────────

    let mut face_map = HashMap::new();
    let mut edge_map = HashMap::new();
    let mut vertex_map = HashMap::new();

    face_map.insert(id_alloc(), face_cyl_a);
    face_map.insert(id_alloc(), face_cyl_b);
    face_map.insert(id_alloc(), face_top);
    face_map.insert(id_alloc(), face_bot);

    edge_map.insert(id_alloc(), e_line_p1);
    edge_map.insert(id_alloc(), e_line_p2);
    edge_map.insert(id_alloc(), e_arc_a_bot);
    edge_map.insert(id_alloc(), e_arc_a_top);
    edge_map.insert(id_alloc(), e_arc_b_bot);
    edge_map.insert(id_alloc(), e_arc_b_top);

    vertex_map.insert(id_alloc(), v0);
    vertex_map.insert(id_alloc(), v1);
    vertex_map.insert(id_alloc(), v2);
    vertex_map.insert(id_alloc(), v3);

    Ok(BooleanResult {
        arena,
        face_map,
        edge_map,
        vertex_map,
        face_geometry,
        edge_geometry,
        cached_face_polys: None,
    })
}

/// Normalize an angle difference to [0, 2π).
fn normalize_angle(mut angle: f64) -> f64 {
    while angle < 0.0 {
        angle += std::f64::consts::TAU;
    }
    while angle >= std::f64::consts::TAU {
        angle -= std::f64::consts::TAU;
    }
    angle
}

// ── Tests ───────────────────────────────────────────────────────────────

#[cfg(test)]
mod tests {
    use super::*;
    use crate::waffle_kernel::CylinderParams;

    #[test]
    fn rotation_to_z_identity_for_z_aligned() {
        let m = rotation_to_z([0.0, 0.0, 1.0]);
        for i in 0..3 {
            for j in 0..3 {
                let expected = if i == j { 1.0 } else { 0.0 };
                assert!(
                    (m[i][j] - expected).abs() < 1e-15,
                    "rotation_to_z([0,0,1]) should be identity, m[{}][{}] = {}",
                    i,
                    j,
                    m[i][j]
                );
            }
        }
    }

    #[test]
    fn rotation_to_z_maps_x_to_z() {
        let m = rotation_to_z([1.0, 0.0, 0.0]);
        let result = mat3_mul_vec(&m, [1.0, 0.0, 0.0]);
        assert!((result[0]).abs() < 1e-12, "x component should be ~0");
        assert!((result[1]).abs() < 1e-12, "y component should be ~0");
        assert!((result[2] - 1.0).abs() < 1e-12, "z component should be ~1");
    }

    #[test]
    fn rotation_to_z_maps_y_to_z() {
        let m = rotation_to_z([0.0, 1.0, 0.0]);
        let result = mat3_mul_vec(&m, [0.0, 1.0, 0.0]);
        assert!((result[0]).abs() < 1e-12);
        assert!((result[1]).abs() < 1e-12);
        assert!((result[2] - 1.0).abs() < 1e-12);
    }

    #[test]
    fn rotation_to_z_maps_45deg_to_z() {
        let c = std::f64::consts::FRAC_1_SQRT_2;
        let dir = [c, 0.0, c];
        let m = rotation_to_z(dir);
        let result = mat3_mul_vec(&m, dir);
        assert!((result[0]).abs() < 1e-12);
        assert!((result[1]).abs() < 1e-12);
        assert!((result[2] - 1.0).abs() < 1e-12);
    }

    #[test]
    fn rotation_to_z_anti_z() {
        let m = rotation_to_z([0.0, 0.0, -1.0]);
        let result = mat3_mul_vec(&m, [0.0, 0.0, -1.0]);
        assert!((result[0]).abs() < 1e-12);
        assert!((result[1]).abs() < 1e-12);
        assert!((result[2] - 1.0).abs() < 1e-12);
    }

    #[test]
    fn rotate_cyl_params_roundtrip() {
        let cyl = CylinderParams {
            center_bottom: [1.0, 2.0, 3.0],
            radius: 5.0,
            x_axis: [0.0, 0.0, -1.0],
            y_axis: [0.0, 1.0, 0.0],
            direction: [1.0, 0.0, 0.0],
            depth: 10.0,
        };
        let m = rotation_to_z(cyl.direction);
        let m_inv = mat3_transpose(&m);
        let rotated = rotate_cyl_params(&cyl, &m);
        let back = rotate_cyl_params(&rotated, &m_inv);

        for i in 0..3 {
            assert!(
                (back.center_bottom[i] - cyl.center_bottom[i]).abs() < 1e-12,
                "center_bottom[{}] roundtrip: {} vs {}",
                i,
                back.center_bottom[i],
                cyl.center_bottom[i]
            );
            assert!(
                (back.direction[i] - cyl.direction[i]).abs() < 1e-12,
                "direction[{}] roundtrip: {} vs {}",
                i,
                back.direction[i],
                cyl.direction[i]
            );
        }
        assert!((back.radius - cyl.radius).abs() < 1e-15);
        assert!((back.depth - cyl.depth).abs() < 1e-15);
    }

    // ── Non-parallel cyl-cyl boolean integration tests ──────────────

    fn make_perp_cyls() -> (CylinderParams, CylinderParams) {
        let cyl_a = CylinderParams {
            center_bottom: [0.0, 0.0, -5.0],
            radius: 1.0,
            x_axis: [1.0, 0.0, 0.0],
            y_axis: [0.0, 1.0, 0.0],
            direction: [0.0, 0.0, 1.0],
            depth: 10.0,
        };
        let cyl_b = CylinderParams {
            center_bottom: [-5.0, 0.0, 0.0],
            radius: 1.0,
            x_axis: [0.0, 0.0, 1.0],
            y_axis: [0.0, 1.0, 0.0],
            direction: [1.0, 0.0, 0.0],
            depth: 10.0,
        };
        (cyl_a, cyl_b)
    }

    #[test]
    fn test_non_parallel_cyl_cyl_union_topology() {
        let (cyl_a, cyl_b) = make_perp_cyls();
        let mut id = 100u64;
        let result = non_parallel_cyl_cyl_boolean(&cyl_a, &cyl_b, BoolOp::Union, &mut || {
            id += 1;
            id
        })
        .unwrap();

        // V=2, E=2, F=2 → V-E+F = 2
        let v = result.vertex_map.len();
        let e = result.edge_map.len();
        let f = result.face_map.len();
        assert_eq!(
            v - e + f,
            2,
            "Euler V-E+F: {}-{}+{} = {}",
            v,
            e,
            f,
            v as i64 - e as i64 + f as i64
        );
        assert_eq!(f, 2, "expected 2 faces");
    }

    #[test]
    fn test_non_parallel_cyl_cyl_subtract_topology() {
        let (cyl_a, cyl_b) = make_perp_cyls();
        let mut id = 200u64;
        let result = non_parallel_cyl_cyl_boolean(&cyl_a, &cyl_b, BoolOp::Subtract, &mut || {
            id += 1;
            id
        })
        .unwrap();

        let v = result.vertex_map.len();
        let e = result.edge_map.len();
        let f = result.face_map.len();
        assert_eq!(v - e + f, 2, "Euler V-E+F");
    }

    #[test]
    fn test_non_parallel_cyl_cyl_surface_preservation() {
        let (cyl_a, cyl_b) = make_perp_cyls();
        let mut id = 300u64;
        let result = non_parallel_cyl_cyl_boolean(&cyl_a, &cyl_b, BoolOp::Union, &mut || {
            id += 1;
            id
        })
        .unwrap();

        // A15.5: All faces must be Cylindrical or Planar
        for (_fid, geom) in &result.face_geometry {
            assert!(
                matches!(geom, SurfaceGeom::Cylindrical(_) | SurfaceGeom::Planar(_)),
                "face geometry must be Cylindrical or Planar, got {:?}",
                geom
            );
        }

        // All edge geometry must be Elliptical
        for (_eid, geom) in &result.edge_geometry {
            assert!(
                matches!(geom, CurveGeom::Elliptical(_)),
                "edge geometry must be Elliptical for non-parallel cyl-cyl, got {:?}",
                geom
            );
        }
    }

    #[test]
    fn test_non_parallel_cyl_cyl_tessellation_valid() {
        use crate::tessellation;

        let (cyl_a, cyl_b) = make_perp_cyls();
        let mut id = 400u64;
        let result = non_parallel_cyl_cyl_boolean(&cyl_a, &cyl_b, BoolOp::Union, &mut || {
            id += 1;
            id
        })
        .unwrap();

        let mesh = tessellation::tessellate_solid(
            &result.arena,
            &result.face_map,
            &result.face_geometry,
            &result.edge_geometry,
            None,
            None,
            false,
        )
        .unwrap();

        // Non-degenerate output
        assert!(mesh.vertices.len() > 0, "mesh should have vertices");
        assert!(mesh.indices.len() > 0, "mesh should have triangles");
        assert!(
            mesh.indices.len() >= 6,
            "mesh should have at least 2 triangles"
        );

        // No NaN/Inf in vertices
        for v in &mesh.vertices {
            assert!(v.is_finite(), "vertex contains NaN or Inf: {}", v);
        }

        // Verify non-degenerate AABB (not collapsed to a flat plane)
        let vert_count = mesh.vertices.len() / 3;
        assert!(
            vert_count > 100,
            "expected >100 vertices for cyl-cyl union, got {}",
            vert_count
        );
        let (mut min_x, mut min_y, mut min_z) = (f32::MAX, f32::MAX, f32::MAX);
        let (mut max_x, mut max_y, mut max_z) = (f32::MIN, f32::MIN, f32::MIN);
        for i in 0..vert_count {
            let x = mesh.vertices[i * 3];
            let y = mesh.vertices[i * 3 + 1];
            let z = mesh.vertices[i * 3 + 2];
            min_x = min_x.min(x);
            min_y = min_y.min(y);
            min_z = min_z.min(z);
            max_x = max_x.max(x);
            max_y = max_y.max(y);
            max_z = max_z.max(z);
        }
        let dx = max_x - min_x;
        let dy = max_y - min_y;
        let dz = max_z - min_z;
        let volume = dx as f64 * dy as f64 * dz as f64;
        assert!(
            volume > 1e-6,
            "AABB volume too small (collapsed): {:.2e} (dims {:.4} x {:.4} x {:.4})",
            volume,
            dx,
            dy,
            dz
        );
    }

    #[test]
    fn test_cyl_cyl_inner_loop_produces_annular_mesh() {
        // Verify that the inner loop (hole) on each cylindrical face is tessellated,
        // producing an annular mesh between two elliptic rings rather than a simple tube.
        use crate::tessellation;

        let (cyl_a, cyl_b) = make_perp_cyls();
        let mut id = 500u64;
        let result = non_parallel_cyl_cyl_boolean(&cyl_a, &cyl_b, BoolOp::Union, &mut || {
            id += 1;
            id
        })
        .unwrap();

        // Verify B-Rep has inner loops
        for (_kid, &face_idx) in &result.face_map {
            let face = &result.arena.faces[face_idx.0];
            assert!(
                !face.inner_loops.is_empty(),
                "cyl-cyl face should have inner loops"
            );
        }

        let mesh = tessellation::tessellate_solid(
            &result.arena,
            &result.face_map,
            &result.face_geometry,
            &result.edge_geometry,
            None,
            None,
            false,
        )
        .unwrap();

        let tri_count = mesh.indices.len() / 3;
        // 2 faces with earcut annulus: expect at least 50 triangles total
        assert!(
            tri_count >= 50,
            "annular mesh should have >=50 triangles, got {}",
            tri_count
        );

        // Verify non-degenerate AABB (mesh spans 3D, not collapsed to a plane)
        let vert_count = mesh.vertices.len() / 3;
        let (mut min_x, mut min_y, mut min_z) = (f32::MAX, f32::MAX, f32::MAX);
        let (mut max_x, mut max_y, mut max_z) = (f32::MIN, f32::MIN, f32::MIN);
        for i in 0..vert_count {
            let (x, y, z) = (
                mesh.vertices[i * 3],
                mesh.vertices[i * 3 + 1],
                mesh.vertices[i * 3 + 2],
            );
            min_x = min_x.min(x);
            min_y = min_y.min(y);
            min_z = min_z.min(z);
            max_x = max_x.max(x);
            max_y = max_y.max(y);
            max_z = max_z.max(z);
        }
        let dx = max_x - min_x;
        let dy = max_y - min_y;
        let dz = max_z - min_z;
        let aabb_vol = dx as f64 * dy as f64 * dz as f64;
        assert!(
            aabb_vol > 1e-6,
            "annular mesh AABB collapsed: vol={:.2e} (dims {:.4} x {:.4} x {:.4})",
            aabb_vol,
            dx,
            dy,
            dz
        );
    }

    #[test]
    fn test_box_minus_enclosed_cyl_uses_cylinder_z_range() {
        // Box: [−0.5, −0.5, 0] × [0.5, 0.5, 1.0]
        // Cylinder: centered at origin, r=0.2, z from 0.2 to 0.7 (shorter than box)
        use crate::tessellation;

        let box_aabb = Aabb {
            min: [-0.5, -0.5, 0.0],
            max: [0.5, 0.5, 1.0],
        };
        let cyl = CylinderParams {
            center_bottom: [0.0, 0.0, 0.2],
            radius: 0.2,
            x_axis: [1.0, 0.0, 0.0],
            y_axis: [0.0, 1.0, 0.0],
            direction: [0.0, 0.0, 1.0],
            depth: 0.5,
        };
        let mut id = 1000u64;
        let result = build_box_minus_enclosed_cyl(&box_aabb, &cyl, &mut || {
            id += 1;
            id
        })
        .unwrap();

        let mesh = tessellation::tessellate_solid(
            &result.arena,
            &result.face_map,
            &result.face_geometry,
            &result.edge_geometry,
            None,
            None,
            false,
        )
        .unwrap();

        let vert_count = mesh.vertices.len() / 3;
        assert!(vert_count > 24, "expected >24 vertices, got {}", vert_count);

        // AABB collapse check: not all vertices should be on AABB faces
        let (mut bmin, mut bmax) = ([f32::MAX; 3], [f32::MIN; 3]);
        for i in 0..vert_count {
            for d in 0..3 {
                bmin[d] = bmin[d].min(mesh.vertices[i * 3 + d]);
                bmax[d] = bmax[d].max(mesh.vertices[i * 3 + d]);
            }
        }
        let tol = 1e-4_f32;
        let mut non_aabb = 0;
        for i in 0..vert_count {
            let x = mesh.vertices[i * 3];
            let y = mesh.vertices[i * 3 + 1];
            let z = mesh.vertices[i * 3 + 2];
            let on_face = (x - bmin[0]).abs() < tol
                || (x - bmax[0]).abs() < tol
                || (y - bmin[1]).abs() < tol
                || (y - bmax[1]).abs() < tol
                || (z - bmin[2]).abs() < tol
                || (z - bmax[2]).abs() < tol;
            if !on_face {
                non_aabb += 1;
            }
        }
        assert!(
            non_aabb > 0,
            "all {} vertices on AABB faces — cylinder z-range not used",
            vert_count
        );
    }
}
