//! SSI-based boolean operations (box-cylinder, cylinder-cylinder,
//! box-sphere, sphere-sphere) and analytical B-Rep construction helpers.
//!
//! Contains frame rotation utilities, analytical SSI dispatch, and all
//! build_* functions for constructing B-Rep results from cylinder/box/sphere
//! primitives.

use crate::geometry::curve::{Arc3D, Circle3D, CurveGeom, Ellipse3D, Line3D};
use crate::geometry::point::{Point3, Vector3};
use crate::geometry::surface::{Cylinder, Plane, SurfaceGeom};
use crate::ssi::{self, Aabb};
use crate::topology::arena::TopoArena;
use crate::topology::half_edge::*;
use crate::types::*;
use crate::units::{CAP_FACE_NORMAL_Z, TAU_COINCIDENT, TAU_MODEL, TAU_WORK};
use crate::vecmath::*;
use crate::waffle_kernel::{CylinderParams, SphereParams, WaffleSolid};
use std::collections::BTreeMap;

use super::classify::{classify_face, classify_face_nonconvex, point_in_solid, FaceClass};
use super::clip::{
    classify_coplanarity, clip_polygon_by_plane_cached, dedup_face_polys, is_coplanar,
    is_face_set_convex, merge_nearby_vertices, resolve_t_junctions, CoplanarClass,
    IntersectionCache,
};
use super::{
    boolean_op_from_polys, build_brep_from_polygons, build_brep_from_polygons_inner,
    collect_fragments, collect_union_fragments, compute_adaptive_tau_weld, extract_face_polys,
    extract_face_polys_general, polygon_area_3d, polygon_centroid, BoolOp, BooleanResult, FacePoly,
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

/// Perform an SSI-based boolean operation on solids involving cylinders or spheres.
///
/// Dispatches to the appropriate analytical boolean handler based on operand types:
/// - cylinder + cylinder → `cyl_cyl_boolean`
/// - box + cylinder → `box_cyl_boolean`
/// - box + sphere → `box_sphere_boolean`
/// - sphere + sphere → `sphere_sphere_boolean`
///
/// Ref: A15 (Analytical Primacy) — quadric pairs must use exact SSI, not mesh fallback.
pub(crate) fn ssi_boolean_op(
    solid_a: &WaffleSolid,
    solid_b: &WaffleSolid,
    op: BoolOp,
    id_alloc: &mut dyn FnMut() -> u64,
) -> Result<BooleanResult, KernelError> {
    let a_is_cyl = solid_a.cylinder_params.is_some();
    let b_is_cyl = solid_b.cylinder_params.is_some();
    let a_is_sphere = solid_a.sphere_params.is_some();
    let b_is_sphere = solid_b.sphere_params.is_some();

    // Try analytical SSI pipeline first; fall back to polygon approximation
    // for unsupported cases (partial overlaps, cylinder-minus-box, etc.)
    let analytical_result = if a_is_cyl && b_is_cyl {
        let cyl_a = solid_a.cylinder_params.as_ref().unwrap();
        let cyl_b = solid_b.cylinder_params.as_ref().unwrap();
        cyl_cyl_boolean(cyl_a, cyl_b, op, id_alloc)
    } else if !a_is_cyl && !a_is_sphere && b_is_cyl {
        let box_aabb = ssi::compute_box_aabb(solid_a);
        let cyl = solid_b.cylinder_params.as_ref().unwrap();
        box_cyl_boolean(&box_aabb, solid_a, cyl, op, id_alloc)
    } else if a_is_cyl && !b_is_cyl && !b_is_sphere {
        let box_aabb = ssi::compute_box_aabb(solid_b);
        let cyl = solid_a.cylinder_params.as_ref().unwrap();
        match op {
            BoolOp::Union => box_cyl_boolean(&box_aabb, solid_b, cyl, BoolOp::Union, id_alloc),
            BoolOp::Intersect => {
                box_cyl_boolean(&box_aabb, solid_b, cyl, BoolOp::Intersect, id_alloc)
            }
            BoolOp::Subtract => cyl_minus_box_boolean(&box_aabb, solid_b, cyl, id_alloc),
        }
    } else if a_is_sphere && b_is_sphere {
        // Sphere + Sphere
        let sp_a = solid_a.sphere_params.as_ref().unwrap();
        let sp_b = solid_b.sphere_params.as_ref().unwrap();
        sphere_sphere_boolean(sp_a, sp_b, solid_a, solid_b, op, id_alloc)
    } else if !a_is_sphere && b_is_sphere {
        // Box + Sphere
        let sp = solid_b.sphere_params.as_ref().unwrap();
        box_sphere_boolean(solid_a, sp, solid_b, op, id_alloc)
    } else if a_is_sphere && !b_is_sphere {
        // Sphere + Box: commute operands for subtract, symmetric for union/intersect
        let sp = solid_a.sphere_params.as_ref().unwrap();
        match op {
            BoolOp::Union => box_sphere_boolean(solid_b, sp, solid_a, BoolOp::Union, id_alloc),
            BoolOp::Intersect => {
                box_sphere_boolean(solid_b, sp, solid_a, BoolOp::Intersect, id_alloc)
            }
            BoolOp::Subtract => {
                // sphere - box: not yet supported analytically
                Err(KernelError::NotSupported {
                    operation: "sphere-minus-box boolean".to_string(),
                })
            }
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
                    face_map: BTreeMap::new(),
                    edge_map: BTreeMap::new(),
                    vertex_map: BTreeMap::new(),
                    face_geometry: BTreeMap::new(),
                    edge_geometry: BTreeMap::new(),
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
                    face_map: BTreeMap::new(),
                    edge_map: BTreeMap::new(),
                    vertex_map: BTreeMap::new(),
                    face_geometry: BTreeMap::new(),
                    edge_geometry: BTreeMap::new(),
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
    if product > 50000 && !a_convex && !b_convex {
        let effective = super::count_aabb_overlapping_pairs(&a_faces, &b_faces, TAU_MODEL);
        if effective > 50000 {
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

// ── Enclosed hole subtract (complex solid minus enclosed cylinder) ─────

/// Subtract an enclosed cylinder from a complex solid by direct face construction.
///
/// Instead of clipping (which fails on coplanar caps), we:
/// 1. Keep all non-cap faces from solid_a
/// 2. For cap faces coplanar with cylinder ends, cut annular holes via triangulation
/// 3. Add inner cylinder lateral faces (reversed normals)
/// 4. Add inner bottom cap for blind holes
///
/// This is an A15-compliant analytical path that avoids polygon clipping entirely.
pub(crate) fn enclosed_hole_in_solid(
    solid_a: &WaffleSolid,
    cyl_b: &CylinderParams,
    id_alloc: &mut dyn FnMut() -> u64,
) -> Result<BooleanResult, KernelError> {
    let n = 32usize;
    let dir = cyl_b.direction;
    let neg_dir = [-dir[0], -dir[1], -dir[2]];

    // Get solid_a's face polygons
    let a_faces = extract_face_polys_general(solid_a);
    if a_faces.is_empty() {
        return Err(KernelError::NotSupported {
            operation: "enclosed_hole_in_solid: solid_a has no faces".to_string(),
        });
    }

    // Cylinder Z range
    let cyl_z_bot = v3_dot(cyl_b.center_bottom, dir);
    let _cyl_z_top = cyl_z_bot + cyl_b.depth;

    // Compute inner circle points at top and bottom of the SOLID (not cylinder)
    // We need to find the Z range where the hole actually intersects the solid
    // by looking at the cap faces of solid_a.

    // Find the Z extents of solid_a along the cylinder direction
    let mut a_z_min = f64::INFINITY;
    let mut a_z_max = f64::NEG_INFINITY;
    for face in &a_faces {
        for v in &face.verts {
            let z = v3_dot(*v, dir);
            a_z_min = a_z_min.min(z);
            a_z_max = a_z_max.max(z);
        }
    }

    let hole_z_top = a_z_max; // hole always open at the top cap
    let through_hole = cyl_z_bot <= a_z_min + TAU_COINCIDENT;
    let hole_z_bot = if through_hole { a_z_min } else { cyl_z_bot };
    let hole_height = hole_z_top - hole_z_bot;

    if hole_height < TAU_MODEL {
        return Err(KernelError::NotSupported {
            operation: "enclosed_hole_in_solid: hole height too small".to_string(),
        });
    }

    // Generate inner circle points
    let center_bot_3d = v3_add(cyl_b.center_bottom, v3_scale(dir, hole_z_bot - cyl_z_bot));

    let inner_bottom: Vec<[f64; 3]> = (0..n)
        .map(|i| {
            let theta = std::f64::consts::TAU * (i as f64) / (n as f64);
            let cos_t = theta.cos();
            let sin_t = theta.sin();
            [
                center_bot_3d[0]
                    + cyl_b.radius * (cos_t * cyl_b.x_axis[0] + sin_t * cyl_b.y_axis[0]),
                center_bot_3d[1]
                    + cyl_b.radius * (cos_t * cyl_b.x_axis[1] + sin_t * cyl_b.y_axis[1]),
                center_bot_3d[2]
                    + cyl_b.radius * (cos_t * cyl_b.x_axis[2] + sin_t * cyl_b.y_axis[2]),
            ]
        })
        .collect();

    let inner_top: Vec<[f64; 3]> = inner_bottom
        .iter()
        .map(|p| {
            [
                p[0] + dir[0] * hole_height,
                p[1] + dir[1] * hole_height,
                p[2] + dir[2] * hole_height,
            ]
        })
        .collect();

    let inner_cyl_surface = SurfaceGeom::Cylindrical(Cylinder {
        origin: Point3::from_array(cyl_b.center_bottom),
        axis: Vector3::from_array(cyl_b.direction),
        radius: cyl_b.radius,
    });

    let mut result_faces: Vec<FacePoly> = Vec::new();

    // 1. Process existing faces: keep non-cap faces, replace caps with annular versions
    for face in &a_faces {
        let face_z = v3_dot(face.origin, dir);
        let normal_dot = v3_dot(face.normal, dir);

        // Check if this is a top cap (normal ≈ +dir, z ≈ a_z_max)
        let is_top_cap = normal_dot > CAP_FACE_NORMAL_Z && (face_z - a_z_max).abs() < TAU_MODEL;

        // Check if this is a bottom cap (normal ≈ -dir, z ≈ a_z_min)
        let is_bottom_cap = normal_dot < -CAP_FACE_NORMAL_Z && (face_z - a_z_min).abs() < TAU_MODEL;

        if is_top_cap {
            // Replace with annular cap: triangulate outer polygon + inner circle hole
            let outer_verts = &face.verts;
            annular_cap_triangles(outer_verts, &inner_top, dir, face.origin, &mut result_faces);
        } else if is_bottom_cap && through_hole {
            // Through-hole: replace bottom cap with annular cap
            let outer_verts = &face.verts;
            annular_cap_triangles(
                outer_verts,
                &inner_bottom,
                neg_dir,
                face.origin,
                &mut result_faces,
            );
        } else {
            // Keep face as-is
            result_faces.push(face.clone());
        }
    }

    // 2. Inner lateral quads (reversed winding — normals point into hole)
    for i in 0..n {
        let j = (i + 1) % n;
        let edge_bot = v3_sub(inner_bottom[i], inner_bottom[j]);
        let edge_up = v3_sub(inner_top[j], inner_bottom[j]);
        let normal = v3_normalize(v3_cross(edge_bot, edge_up));
        result_faces.push(FacePoly {
            verts: vec![inner_bottom[j], inner_bottom[i], inner_top[i], inner_top[j]],
            normal,
            origin: inner_bottom[j],
            surface_geom: Some(inner_cyl_surface.clone()),
        });
    }

    // 3. Inner bottom cap for blind holes
    if !through_hole {
        result_faces.push(FacePoly {
            verts: inner_bottom.clone(), // CCW from +dir → faces up
            normal: dir,
            origin: center_bot_3d,
            surface_geom: None,
        });
    }

    build_brep_from_polygons_inner(&result_faces, TAU_MODEL, true, id_alloc)
}

/// Triangulate an annular cap (outer polygon with inner circular hole).
///
/// For each segment of the inner circle, finds the nearest outer polygon vertex
/// and creates triangles connecting them. This produces a watertight annular surface
/// without requiring polygon-with-holes support.
fn annular_cap_triangles(
    outer: &[[f64; 3]],
    inner: &[[f64; 3]],
    normal: [f64; 3],
    origin: [f64; 3],
    result: &mut Vec<FacePoly>,
) {
    let n_inner = inner.len();
    let n_outer = outer.len();
    if n_inner == 0 || n_outer == 0 {
        return;
    }

    // For each inner edge (inner[i] → inner[j]), find the closest outer vertex
    // and create a triangle fan connecting them. We walk both circles simultaneously.
    let up = normal; // face normal

    // Find the starting outer index closest to inner[0]
    let mut best_outer = 0;
    let mut best_dist = f64::INFINITY;
    for (k, ov) in outer.iter().enumerate() {
        let d = v3_length(v3_sub(*ov, inner[0]));
        if d < best_dist {
            best_dist = d;
            best_outer = k;
        }
    }

    // Walk both circles, creating triangles
    let mut o = best_outer;
    for i in 0..n_inner {
        let j = (i + 1) % n_inner;

        // Check if the winding direction matches the face normal
        // The inner circle winding should be OPPOSITE to the outer (it's a hole)
        // For normal=+Z: outer is CCW, inner should be CW (reversed)

        // Triangle connecting outer[o] to inner edge
        result.push(FacePoly {
            verts: vec![outer[o], inner[i], inner[j]],
            normal: up,
            origin,
            surface_geom: None,
        });

        // Advance outer vertices to cover the gap
        let next_o = (o + 1) % n_outer;
        let d_current = v3_length(v3_sub(outer[o], inner[j]));
        let d_next = v3_length(v3_sub(outer[next_o], inner[j]));

        if d_next < d_current {
            // Add triangle to fill the gap between outer[o] and outer[next_o]
            result.push(FacePoly {
                verts: vec![outer[o], inner[j], outer[next_o]],
                normal: up,
                origin,
                surface_geom: None,
            });
            o = next_o;

            // May need to advance more outer vertices
            loop {
                let next2 = (o + 1) % n_outer;
                let d_curr = v3_length(v3_sub(outer[o], inner[j]));
                let d_nxt = v3_length(v3_sub(outer[next2], inner[j]));
                if d_nxt < d_curr && next2 != best_outer {
                    result.push(FacePoly {
                        verts: vec![outer[o], inner[j], outer[next2]],
                        normal: up,
                        origin,
                        surface_geom: None,
                    });
                    o = next2;
                } else {
                    break;
                }
            }
        }
    }

    // Close the gap between the last outer vertex and the starting one
    let mut remaining = (o + 1) % n_outer;
    while remaining != best_outer {
        let next = (remaining + 1) % n_outer;
        if next == best_outer {
            // Last triangle
            result.push(FacePoly {
                verts: vec![outer[remaining], inner[0], outer[best_outer]],
                normal: up,
                origin,
                surface_geom: None,
            });
        } else {
            result.push(FacePoly {
                verts: vec![outer[remaining], inner[0], outer[next]],
                normal: up,
                origin,
                surface_geom: None,
            });
        }
        remaining = next;
    }
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
            dot > crate::units::COS_NEAR_PARALLEL_CAP // nearly parallel (within ~18°)
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
            } else if xy_enclosed {
                // Through-hole or partial-depth hole — cylinder XY-enclosed but
                // extends beyond one or both caps. Clip cylinder Z to box Z so
                // seam vertices land on the box caps. build_box_minus_enclosed_cyl's
                // touches_bot/touches_top logic handles all sub-cases correctly.
                let clipped_cyl = CylinderParams {
                    center_bottom: [
                        cyl_z.center_bottom[0],
                        cyl_z.center_bottom[1],
                        box_aabb.min[2],
                    ],
                    depth: box_aabb.max[2] - box_aabb.min[2],
                    radius: cyl_z.radius,
                    direction: cyl_z.direction,
                    x_axis: cyl_z.x_axis,
                    y_axis: cyl_z.y_axis,
                };
                let mut result = if box_solid.face_map.len() <= 6 {
                    build_box_minus_enclosed_cyl(&box_aabb, &clipped_cyl, id_alloc)?
                } else {
                    build_planar_solid_minus_enclosed_cyl(box_solid, &clipped_cyl, &m, id_alloc)?
                };
                rotate_boolean_result(&mut result, &m_inv);
                Ok(result)
            } else {
                Err(KernelError::NotSupported {
                    operation: "partial box-cylinder subtract".to_string(),
                })
            }
        }
        BoolOp::Union => {
            // Box fully enclosed in cylinder → union = cylinder
            let box_in_cyl = ssi::box_enclosed_in_cyl(&box_aabb, &cyl_z);
            let box_z_enclosed = box_aabb.min[2] >= cyl_z_min - TAU_COINCIDENT
                && box_aabb.max[2] <= cyl_z_max + TAU_COINCIDENT;
            if box_in_cyl && box_z_enclosed {
                let mut result = build_cyl_result(&cyl_z, id_alloc)?;
                rotate_boolean_result(&mut result, &m_inv);
                return Ok(result);
            }

            // Box XY-enclosed in cylinder but extends beyond one cap → cyl with box boss
            if box_in_cyl {
                let box_extends_above = box_aabb.max[2] > cyl_z_max + TAU_COINCIDENT;
                let box_extends_below = box_aabb.min[2] < cyl_z_min - TAU_COINCIDENT;
                if box_extends_above != box_extends_below {
                    let mut result =
                        build_cyl_with_box_boss(&box_aabb, &cyl_z, box_extends_above, id_alloc)?;
                    rotate_boolean_result(&mut result, &m_inv);
                    return Ok(result);
                }
                // Both sides extend: fall through (future work)
            }

            if fully_enclosed {
                // Cylinder fully inside box → union = box (original frame)
                clone_solid_as_result(box_solid, id_alloc)
            } else if is_boss_top || is_boss_bot {
                let mut result = build_box_with_cyl_boss(&box_aabb, &cyl_z, is_boss_top, id_alloc)?;
                rotate_boolean_result(&mut result, &m_inv);
                Ok(result)
            } else if disjoint {
                Err(KernelError::BooleanFailed {
                    reason: "operands are disjoint (bounding boxes do not overlap)".into(),
                })
            } else {
                // Partial overlap: cylinder center inside box, protruding through sides
                let center_inside = cyl_z.center_bottom[0] >= box_aabb.min[0] - TAU_COINCIDENT
                    && cyl_z.center_bottom[0] <= box_aabb.max[0] + TAU_COINCIDENT
                    && cyl_z.center_bottom[1] >= box_aabb.min[1] - TAU_COINCIDENT
                    && cyl_z.center_bottom[1] <= box_aabb.max[1] + TAU_COINCIDENT;
                if center_inside {
                    let mut result = build_box_cyl_partial_union(&box_aabb, &cyl_z, id_alloc)?;
                    rotate_boolean_result(&mut result, &m_inv);
                    Ok(result)
                } else {
                    Err(KernelError::NotSupported {
                        operation: "box-cylinder union with center outside box".to_string(),
                    })
                }
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

// ── Box-sphere boolean ──────────────────────────────────────────────────

/// Box-sphere boolean with analytical enclosure classification.
///
/// Uses plane-sphere SSI (A15) to determine the geometric relationship between
/// a box (all-planar) solid and a sphere primitive. Handles fully-enclosed,
/// disjoint, and box-inside-sphere configurations analytically. Partial overlaps
/// return NotSupported and fall through to the polygon clipping path.
///
/// Ref: [#1] Patrikalakis Ch.5 — plane-sphere SSI produces circles.
/// Ref: [#33] Stroud — multi-shell Euler formula for cavity operations.
fn box_sphere_boolean(
    box_solid: &WaffleSolid,
    sphere: &SphereParams,
    sphere_solid: &WaffleSolid,
    op: BoolOp,
    id_alloc: &mut dyn FnMut() -> u64,
) -> Result<BooleanResult, KernelError> {
    let box_aabb = ssi::compute_box_aabb(box_solid);
    let c = sphere.center;
    let r = sphere.radius;

    // Classify: is the sphere fully inside the box?
    let sphere_in_box = c[0] - r >= box_aabb.min[0] - TAU_COINCIDENT
        && c[0] + r <= box_aabb.max[0] + TAU_COINCIDENT
        && c[1] - r >= box_aabb.min[1] - TAU_COINCIDENT
        && c[1] + r <= box_aabb.max[1] + TAU_COINCIDENT
        && c[2] - r >= box_aabb.min[2] - TAU_COINCIDENT
        && c[2] + r <= box_aabb.max[2] + TAU_COINCIDENT;

    // Classify: is the box fully inside the sphere?
    // All 8 box corners must be inside the sphere.
    let box_in_sphere = [
        [box_aabb.min[0], box_aabb.min[1], box_aabb.min[2]],
        [box_aabb.max[0], box_aabb.min[1], box_aabb.min[2]],
        [box_aabb.min[0], box_aabb.max[1], box_aabb.min[2]],
        [box_aabb.max[0], box_aabb.max[1], box_aabb.min[2]],
        [box_aabb.min[0], box_aabb.min[1], box_aabb.max[2]],
        [box_aabb.max[0], box_aabb.min[1], box_aabb.max[2]],
        [box_aabb.min[0], box_aabb.max[1], box_aabb.max[2]],
        [box_aabb.max[0], box_aabb.max[1], box_aabb.max[2]],
    ]
    .iter()
    .all(|corner| ssi::point_in_sphere(*corner, c, r));

    // Classify: are they disjoint?
    // Sphere is disjoint from AABB if its center is farther than r from the nearest
    // point on the AABB.
    let nearest = [
        c[0].clamp(box_aabb.min[0], box_aabb.max[0]),
        c[1].clamp(box_aabb.min[1], box_aabb.max[1]),
        c[2].clamp(box_aabb.min[2], box_aabb.max[2]),
    ];
    let dist_sq = v3_dot(v3_sub(c, nearest), v3_sub(c, nearest));
    let disjoint = dist_sq > (r + TAU_COINCIDENT) * (r + TAU_COINCIDENT);

    match op {
        BoolOp::Subtract => {
            if disjoint {
                // Nothing to subtract → result = box
                clone_solid_as_result(box_solid, id_alloc)
            } else {
                // Sphere-in-box (cavity) and partial overlap cases require
                // multi-shell tessellation with inverted normals, which the
                // current tessellation pipeline doesn't support. Defer to
                // polygon clipping path which produces a single-shell result.
                Err(KernelError::NotSupported {
                    operation: "box-sphere subtract (multi-shell)".to_string(),
                })
            }
        }
        BoolOp::Union => {
            if sphere_in_box {
                // Sphere fully inside box → union = box
                clone_solid_as_result(box_solid, id_alloc)
            } else if disjoint {
                Err(KernelError::BooleanFailed {
                    reason: "operands are disjoint (bounding boxes do not overlap)".into(),
                })
            } else if box_in_sphere {
                // Box inside sphere → union = sphere
                clone_solid_as_result(sphere_solid, id_alloc)
            } else {
                Err(KernelError::NotSupported {
                    operation: "partial box-sphere union".to_string(),
                })
            }
        }
        BoolOp::Intersect => {
            if sphere_in_box {
                // Sphere fully inside box → intersect = sphere
                clone_solid_as_result(sphere_solid, id_alloc)
            } else if disjoint {
                // No intersection
                build_empty_result(id_alloc)
            } else if box_in_sphere {
                // Box inside sphere → intersect = box
                clone_solid_as_result(box_solid, id_alloc)
            } else {
                Err(KernelError::NotSupported {
                    operation: "partial box-sphere intersect".to_string(),
                })
            }
        }
    }
}

/// Build an empty solid result (no faces, edges, vertices).
fn build_empty_result(id_alloc: &mut dyn FnMut() -> u64) -> Result<BooleanResult, KernelError> {
    let _ = id_alloc(); // consume one ID for consistency
    let arena = TopoArena::new();
    Ok(BooleanResult {
        arena,
        face_map: BTreeMap::new(),
        edge_map: BTreeMap::new(),
        vertex_map: BTreeMap::new(),
        face_geometry: BTreeMap::new(),
        edge_geometry: BTreeMap::new(),
        cached_face_polys: None,
    })
}

/// Build a box with a spherical cavity (2 shells) for box-minus-enclosed-sphere.
///
/// The result is the original box B-Rep plus the sphere's B-Rep with reversed
/// winding (inner shell). Uses the same merge+winding-reversal approach as
/// build_sphere_shell for consistency.
///
/// Ref: [#33] Stroud — multi-shell Euler formula: V-E+F = 2S.
#[allow(dead_code)] // Will be used when multi-shell tessellation is implemented
fn build_box_with_sphere_cavity(
    box_solid: &WaffleSolid,
    _sphere: &SphereParams,
    sphere_solid: &WaffleSolid,
    id_alloc: &mut dyn FnMut() -> u64,
) -> Result<BooleanResult, KernelError> {
    // Clone the box topology as the base result
    let mut result = clone_solid_as_result(box_solid, id_alloc)?;

    // Add sphere as inner shell with reversed winding (same as build_sphere_shell)
    let v_offset = result.arena.vertices.len();
    let he_offset = result.arena.half_edges.len();
    let e_offset = result.arena.edges.len();
    let f_offset = result.arena.faces.len();
    let l_offset = result.arena.loops.len();

    for v in &sphere_solid.arena.vertices {
        result.arena.vertices.push(Vertex {
            position: v.position,
            half_edge: v.half_edge.map(|he| HalfEdgeIdx(he.0 + he_offset)),
        });
    }

    let solid_idx = SolidIdx(0);
    let inner_shell = result.arena.add_shell(solid_idx);

    for l in &sphere_solid.arena.loops {
        result.arena.loops.push(Loop {
            half_edge: HalfEdgeIdx(l.half_edge.0 + he_offset),
            face: FaceIdx(l.face.0 + f_offset),
        });
    }

    for f in &sphere_solid.arena.faces {
        result.arena.faces.push(Face {
            outer_loop: LoopIdx(f.outer_loop.0 + l_offset),
            inner_loops: f
                .inner_loops
                .iter()
                .map(|l| LoopIdx(l.0 + l_offset))
                .collect(),
            shell: inner_shell,
        });
    }

    // Clone half-edges with SWAPPED next/prev for reversed winding
    for he in &sphere_solid.arena.half_edges {
        result.arena.half_edges.push(HalfEdge {
            origin: VertexIdx(he.origin.0 + v_offset),
            edge: EdgeIdx(he.edge.0 + e_offset),
            twin: HalfEdgeIdx(he.twin.0 + he_offset),
            next: HalfEdgeIdx(he.prev.0 + he_offset), // SWAPPED
            prev: HalfEdgeIdx(he.next.0 + he_offset), // SWAPPED
            loop_: LoopIdx(he.loop_.0 + l_offset),
        });
    }

    // Fix origins after winding reversal
    for he_idx in he_offset..result.arena.half_edges.len() {
        let twin_idx = result.arena.half_edges[he_idx].twin;
        let twin_origin = result.arena.half_edges[twin_idx.0].origin;
        result.arena.half_edges[he_idx].origin = twin_origin;
    }

    for e in &sphere_solid.arena.edges {
        result.arena.edges.push(Edge {
            half_edge: HalfEdgeIdx(e.half_edge.0 + he_offset),
        });
    }

    for (&_kid, &idx) in &sphere_solid.face_map {
        let new_idx = FaceIdx(idx.0 + f_offset);
        result.face_map.insert(id_alloc(), new_idx);
        if let Some(geom) = sphere_solid.face_geometry.get(&idx) {
            result.face_geometry.insert(new_idx, geom.clone());
        }
    }
    for (&_kid, &idx) in &sphere_solid.edge_map {
        let new_idx = EdgeIdx(idx.0 + e_offset);
        result.edge_map.insert(id_alloc(), new_idx);
        if let Some(geom) = sphere_solid.edge_geometry.get(&idx) {
            result.edge_geometry.insert(new_idx, geom.clone());
        }
    }
    for (&_kid, &idx) in &sphere_solid.vertex_map {
        let new_idx = VertexIdx(idx.0 + v_offset);
        result.vertex_map.insert(id_alloc(), new_idx);
    }

    Ok(result)
}

// ── Sphere-sphere boolean ───────────────────────────────────────────────

/// Sphere-sphere boolean with analytical enclosure classification.
///
/// Handles concentric, fully-enclosed, and disjoint configurations analytically.
/// Partial overlaps (where SSI produces a circle intersection curve) return
/// NotSupported and fall through to the polygon clipping path.
///
/// Ref: [#1] Patrikalakis Ch.5 — sphere-sphere SSI produces circles.
/// Ref: [#33] Stroud — multi-shell Euler formula.
fn sphere_sphere_boolean(
    sp_a: &SphereParams,
    sp_b: &SphereParams,
    solid_a: &WaffleSolid,
    solid_b: &WaffleSolid,
    op: BoolOp,
    id_alloc: &mut dyn FnMut() -> u64,
) -> Result<BooleanResult, KernelError> {
    let dist = v3_length(v3_sub(sp_a.center, sp_b.center));

    // Classify geometric relationship
    let disjoint = dist > sp_a.radius + sp_b.radius + TAU_COINCIDENT;
    let concentric = dist < TAU_COINCIDENT;
    let b_in_a = dist + sp_b.radius <= sp_a.radius + TAU_COINCIDENT;
    let a_in_b = dist + sp_a.radius <= sp_b.radius + TAU_COINCIDENT;

    match op {
        BoolOp::Subtract => {
            if disjoint {
                // Nothing to subtract → result = A
                clone_solid_as_result(solid_a, id_alloc)
            } else {
                // Concentric/enclosed/partial subtract requires multi-shell
                // tessellation with inverted normals. Defer to polygon path.
                Err(KernelError::NotSupported {
                    operation: "sphere-sphere subtract (multi-shell)".to_string(),
                })
            }
        }
        BoolOp::Union => {
            if b_in_a || (concentric && sp_a.radius >= sp_b.radius) {
                // B inside A → union = A
                clone_solid_as_result(solid_a, id_alloc)
            } else if a_in_b {
                // A inside B → union = B
                clone_solid_as_result(solid_b, id_alloc)
            } else if disjoint {
                Err(KernelError::BooleanFailed {
                    reason: "operands are disjoint (bounding boxes do not overlap)".into(),
                })
            } else {
                Err(KernelError::NotSupported {
                    operation: "partial sphere-sphere union".to_string(),
                })
            }
        }
        BoolOp::Intersect => {
            if b_in_a {
                // B inside A → intersect = B
                clone_solid_as_result(solid_b, id_alloc)
            } else if a_in_b {
                // A inside B → intersect = A
                clone_solid_as_result(solid_a, id_alloc)
            } else if disjoint {
                // No intersection
                build_empty_result(id_alloc)
            } else {
                Err(KernelError::NotSupported {
                    operation: "partial sphere-sphere intersect".to_string(),
                })
            }
        }
    }
}

/// Build a spherical shell (outer sphere A minus inner sphere B) → 2-shell result.
///
/// Clones sphere A's B-Rep as the outer shell, then adds sphere B's B-Rep as an
/// inner void shell (same approach as build_box_with_sphere_cavity).
#[allow(dead_code)] // Will be used when multi-shell tessellation is implemented
fn build_sphere_shell(
    solid_a: &WaffleSolid,
    solid_b: &WaffleSolid,
    _sp_a: &SphereParams,
    _sp_b: &SphereParams,
    id_alloc: &mut dyn FnMut() -> u64,
) -> Result<BooleanResult, KernelError> {
    // Clone outer sphere A
    let mut result = clone_solid_as_result(solid_a, id_alloc)?;

    // Add inner sphere B as a second shell with REVERSED winding.
    // The inner shell represents a cavity/void, so face normals must point
    // inward (toward sphere center). Swapping next↔prev reverses the
    // half-edge loop traversal, flipping face orientation.
    // Ref: [#33] Stroud — inner shell face orientation is inverted.
    let v_offset = result.arena.vertices.len();
    let he_offset = result.arena.half_edges.len();
    let e_offset = result.arena.edges.len();
    let f_offset = result.arena.faces.len();
    let l_offset = result.arena.loops.len();

    for v in &solid_b.arena.vertices {
        result.arena.vertices.push(Vertex {
            position: v.position,
            half_edge: v.half_edge.map(|he| HalfEdgeIdx(he.0 + he_offset)),
        });
    }

    let solid_idx = SolidIdx(0);
    let inner_shell = result.arena.add_shell(solid_idx);

    for l in &solid_b.arena.loops {
        result.arena.loops.push(Loop {
            half_edge: HalfEdgeIdx(l.half_edge.0 + he_offset),
            face: FaceIdx(l.face.0 + f_offset),
        });
    }

    for f in &solid_b.arena.faces {
        result.arena.faces.push(Face {
            outer_loop: LoopIdx(f.outer_loop.0 + l_offset),
            inner_loops: f
                .inner_loops
                .iter()
                .map(|l| LoopIdx(l.0 + l_offset))
                .collect(),
            shell: inner_shell,
        });
    }

    // Clone half-edges with SWAPPED next/prev for reversed winding
    for he in &solid_b.arena.half_edges {
        result.arena.half_edges.push(HalfEdge {
            origin: VertexIdx(he.origin.0 + v_offset),
            edge: EdgeIdx(he.edge.0 + e_offset),
            twin: HalfEdgeIdx(he.twin.0 + he_offset),
            next: HalfEdgeIdx(he.prev.0 + he_offset), // SWAPPED
            prev: HalfEdgeIdx(he.next.0 + he_offset), // SWAPPED
            loop_: LoopIdx(he.loop_.0 + l_offset),
        });
    }

    // Fix origins after winding reversal: each half-edge in the reversed
    // loop should originate from the destination of the original half-edge
    // (which is the origin of its twin in the original winding).
    for he_idx in he_offset..result.arena.half_edges.len() {
        let twin_idx = result.arena.half_edges[he_idx].twin;
        let twin_origin = result.arena.half_edges[twin_idx.0].origin;
        result.arena.half_edges[he_idx].origin = twin_origin;
    }

    for e in &solid_b.arena.edges {
        result.arena.edges.push(Edge {
            half_edge: HalfEdgeIdx(e.half_edge.0 + he_offset),
        });
    }

    for (&_kid, &idx) in &solid_b.face_map {
        let new_idx = FaceIdx(idx.0 + f_offset);
        result.face_map.insert(id_alloc(), new_idx);
        if let Some(geom) = solid_b.face_geometry.get(&idx) {
            result.face_geometry.insert(new_idx, geom.clone());
        }
    }
    for (&_kid, &idx) in &solid_b.edge_map {
        let new_idx = EdgeIdx(idx.0 + e_offset);
        result.edge_map.insert(id_alloc(), new_idx);
        if let Some(geom) = solid_b.edge_geometry.get(&idx) {
            result.edge_geometry.insert(new_idx, geom.clone());
        }
    }
    for (&_kid, &idx) in &solid_b.vertex_map {
        let new_idx = VertexIdx(idx.0 + v_offset);
        result.vertex_map.insert(id_alloc(), new_idx);
    }

    Ok(result)
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
    let mut face_geometry = BTreeMap::new();

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
    let mut edge_geometry: BTreeMap<EdgeIdx, CurveGeom> = BTreeMap::new();

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
    let mut face_map = BTreeMap::new();
    let mut edge_map = BTreeMap::new();
    let mut vertex_map = BTreeMap::new();

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
            BoolOp::Union => Err(KernelError::BooleanFailed {
                reason: "operands are disjoint (bounding boxes do not overlap)".into(),
            }),
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
                        // Tool laterally encloses blank — check Z coverage
                        let tool_covers_bottom = bz_min <= az_min + TAU_COINCIDENT;
                        let tool_covers_top = bz_max >= az_max - TAU_COINCIDENT;

                        if tool_covers_bottom && tool_covers_top {
                            // Case 1: Tool fully covers blank Z range → empty solid
                            let mut arena = TopoArena::new();
                            let solid_idx = arena.add_solid();
                            let shell_idx = arena.add_shell(solid_idx);
                            arena.solids[solid_idx.0].outer_shell = shell_idx;
                            Ok(BooleanResult {
                                arena,
                                face_map: BTreeMap::new(),
                                edge_map: BTreeMap::new(),
                                vertex_map: BTreeMap::new(),
                                face_geometry: BTreeMap::new(),
                                edge_geometry: BTreeMap::new(),
                                cached_face_polys: None,
                            })
                        } else if tool_covers_bottom && !tool_covers_top {
                            // Case 2: Tool covers bottom, top survives [bz_max, az_max]
                            let surviving = CylinderParams {
                                center_bottom: [
                                    cyl_a.center_bottom[0],
                                    cyl_a.center_bottom[1],
                                    bz_max,
                                ],
                                radius: r1,
                                x_axis: cyl_a.x_axis,
                                y_axis: cyl_a.y_axis,
                                direction: cyl_a.direction,
                                depth: az_max - bz_max,
                            };
                            build_cyl_result(&surviving, id_alloc)
                        } else if !tool_covers_bottom && tool_covers_top {
                            // Case 3: Tool covers top, bottom survives [az_min, bz_min]
                            let surviving = CylinderParams {
                                center_bottom: [
                                    cyl_a.center_bottom[0],
                                    cyl_a.center_bottom[1],
                                    az_min,
                                ],
                                radius: r1,
                                x_axis: cyl_a.x_axis,
                                y_axis: cyl_a.y_axis,
                                direction: cyl_a.direction,
                                depth: bz_min - az_min,
                            };
                            build_cyl_result(&surviving, id_alloc)
                        } else {
                            // Case 4: Tool in middle — would produce two disjoint solids
                            return Err(KernelError::NotSupported {
                                operation: "concentric subtract producing disjoint solids"
                                    .to_string(),
                            });
                        }
                    } else {
                        // r2 < r1 (inner hole). Check if inner cylinder's Z range
                        // fully covers outer. If not, the result is a tube + cap(s),
                        // which is too complex for a single analytical build.
                        let inner_covers_z =
                            bz_min <= az_min + TAU_COINCIDENT && bz_max >= az_max - TAU_COINCIDENT;
                        if inner_covers_z {
                            build_cyl_tube(cyl_a, cyl_b, z_min, z_max, id_alloc)
                        } else {
                            // Inner hole doesn't span full outer height → fall back
                            // to polygon boolean for correct tube+cap geometry.
                            return Err(KernelError::NotSupported {
                                operation:
                                    "concentric cyl subtract: inner shorter than outer (tube+cap)"
                                        .to_string(),
                            });
                        }
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

        // Enclosed case: when h ≈ 0, one cylinder is fully inside the other
        // (no real 2D intersection). Build result directly — no clipping needed
        // since the inner boundary doesn't intersect the outer boundary.
        if h < TAU_COINCIDENT {
            return build_enclosed_cyl_subtract(cyl_a, cyl_b, op, z_min, z_max, id_alloc);
        }

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
    let mut face_map = BTreeMap::new();
    let mut edge_map = BTreeMap::new();
    let mut vertex_map = BTreeMap::new();

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
    let mut face_geometry = BTreeMap::new();
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
    let mut edge_geometry = BTreeMap::new();
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
    let mut face_map = BTreeMap::new();
    let mut edge_map = BTreeMap::new();
    let mut vertex_map = BTreeMap::new();
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

// ── Build enclosed non-concentric cylinder subtract ──────────────────

/// Build the result of subtracting a small enclosed cylinder from a larger one.
///
/// Since the inner cylinder is fully enclosed (no 2D boundary intersection),
/// no polygon clipping is needed. We construct the result face polygons directly:
///
/// **Through-hole** (inner Z covers outer Z):
///   - Outer lateral quads (kept)
///   - Inner lateral quads (reversed winding — inward-facing normals)
///   - Top annular cap (triangulated fan connecting outer + inner circles)
///   - Bottom annular cap (triangulated fan)
///
/// **Blind hole** (inner Z shorter than outer):
///   - Outer lateral quads (kept)
///   - Inner lateral quads (reversed, partial height)
///   - Top annular cap (triangulated fan)
///   - Bottom cap (full circle, no hole)
///   - Inner bottom cap (circular face at bottom of blind hole)
///
/// Ref: A15 — analytical primacy, no mesh fallback for quadric SSI.
fn build_enclosed_cyl_subtract(
    cyl_a: &CylinderParams,
    cyl_b: &CylinderParams,
    op: BoolOp,
    z_min: f64,
    z_max: f64,
    id_alloc: &mut dyn FnMut() -> u64,
) -> Result<BooleanResult, KernelError> {
    // Determine which is outer vs inner
    let (outer, inner) = if cyl_a.radius >= cyl_b.radius {
        (cyl_a, cyl_b)
    } else {
        (cyl_b, cyl_a)
    };

    match op {
        BoolOp::Subtract => {
            if cyl_a.radius >= cyl_b.radius {
                // A - B: big minus small = hole in big (the common case)
                build_enclosed_hole(outer, inner, z_min, z_max, id_alloc)
            } else {
                // A - B where A is smaller: small minus big = empty
                // (small is entirely inside big, subtracting big removes everything)
                let mut arena = TopoArena::new();
                let solid_idx = arena.add_solid();
                let shell_idx = arena.add_shell(solid_idx);
                arena.solids[solid_idx.0].outer_shell = shell_idx;
                Ok(BooleanResult {
                    arena,
                    face_map: BTreeMap::new(),
                    edge_map: BTreeMap::new(),
                    vertex_map: BTreeMap::new(),
                    face_geometry: BTreeMap::new(),
                    edge_geometry: BTreeMap::new(),
                    cached_face_polys: None,
                })
            }
        }
        BoolOp::Union => {
            // Union of enclosed cylinders = the outer cylinder
            build_cyl_result(outer, id_alloc)
        }
        BoolOp::Intersect => {
            // Intersection of enclosed cylinders = the inner cylinder
            build_cyl_result(inner, id_alloc)
        }
    }
}

/// Construct face polygons for a cylinder with a non-concentric hole drilled through it.
fn build_enclosed_hole(
    outer: &CylinderParams,
    inner: &CylinderParams,
    _z_min: f64,
    _z_max: f64,
    id_alloc: &mut dyn FnMut() -> u64,
) -> Result<BooleanResult, KernelError> {
    let n = 32usize; // polygon subdivision count
    let dir = outer.direction;
    let neg_dir = [-dir[0], -dir[1], -dir[2]];

    // Z ranges
    let (az_min, az_max) = ssi::cyl_z_range(outer);
    let (bz_min, bz_max) = ssi::cyl_z_range(inner);
    let through_hole = bz_min <= az_min + TAU_COINCIDENT && bz_max >= az_max - TAU_COINCIDENT;
    // For blind hole, inner starts at outer's top and goes down
    // The inner cylinder bottom is at bz_min (or bz_max if direction is flipped)
    let inner_z_bottom = bz_min.max(az_min); // clamp to outer range
    let inner_z_top = bz_max.min(az_max);

    // ── Generate circle points ──
    let outer_bottom: Vec<[f64; 3]> = (0..n)
        .map(|i| {
            let theta = std::f64::consts::TAU * (i as f64) / (n as f64);
            let cos_t = theta.cos();
            let sin_t = theta.sin();
            [
                outer.center_bottom[0]
                    + outer.radius * (cos_t * outer.x_axis[0] + sin_t * outer.y_axis[0]),
                outer.center_bottom[1]
                    + outer.radius * (cos_t * outer.x_axis[1] + sin_t * outer.y_axis[1]),
                outer.center_bottom[2]
                    + outer.radius * (cos_t * outer.x_axis[2] + sin_t * outer.y_axis[2]),
            ]
        })
        .collect();

    let outer_top: Vec<[f64; 3]> = outer_bottom
        .iter()
        .map(|p| {
            [
                p[0] + dir[0] * outer.depth,
                p[1] + dir[1] * outer.depth,
                p[2] + dir[2] * outer.depth,
            ]
        })
        .collect();

    // Inner circle points (at the inner cylinder's center, which is offset from outer)
    let inner_bottom_z = if through_hole { az_min } else { inner_z_bottom };
    let inner_top_z = if through_hole { az_max } else { inner_z_top };
    let inner_height = inner_top_z - inner_bottom_z;

    let inner_bottom_center = [
        inner.center_bottom[0] + dir[0] * (inner_bottom_z - ssi::cyl_z_range(inner).0),
        inner.center_bottom[1] + dir[1] * (inner_bottom_z - ssi::cyl_z_range(inner).0),
        inner.center_bottom[2] + dir[2] * (inner_bottom_z - ssi::cyl_z_range(inner).0),
    ];

    let inner_bottom: Vec<[f64; 3]> = (0..n)
        .map(|i| {
            let theta = std::f64::consts::TAU * (i as f64) / (n as f64);
            let cos_t = theta.cos();
            let sin_t = theta.sin();
            [
                inner_bottom_center[0]
                    + inner.radius * (cos_t * inner.x_axis[0] + sin_t * inner.y_axis[0]),
                inner_bottom_center[1]
                    + inner.radius * (cos_t * inner.x_axis[1] + sin_t * inner.y_axis[1]),
                inner_bottom_center[2]
                    + inner.radius * (cos_t * inner.x_axis[2] + sin_t * inner.y_axis[2]),
            ]
        })
        .collect();

    let inner_top: Vec<[f64; 3]> = inner_bottom
        .iter()
        .map(|p| {
            [
                p[0] + dir[0] * inner_height,
                p[1] + dir[1] * inner_height,
                p[2] + dir[2] * inner_height,
            ]
        })
        .collect();

    // ── Build face polygons ──
    let mut faces: Vec<FacePoly> = Vec::new();

    // Surface geometry tags
    let outer_cyl_surface = SurfaceGeom::Cylindrical(Cylinder {
        origin: Point3::from_array(outer.center_bottom),
        axis: Vector3::from_array(outer.direction),
        radius: outer.radius,
    });
    let inner_cyl_surface = SurfaceGeom::Cylindrical(Cylinder {
        origin: Point3::from_array(inner.center_bottom),
        axis: Vector3::from_array(inner.direction),
        radius: inner.radius,
    });

    // 1. Outer lateral quads (outward normals)
    for i in 0..n {
        let j = (i + 1) % n;
        let edge_bot = v3_sub(outer_bottom[j], outer_bottom[i]);
        let edge_up = v3_sub(outer_top[i], outer_bottom[i]);
        let normal = v3_normalize(v3_cross(edge_bot, edge_up));
        faces.push(FacePoly {
            verts: vec![outer_bottom[i], outer_bottom[j], outer_top[j], outer_top[i]],
            normal,
            origin: outer_bottom[i],
            surface_geom: Some(outer_cyl_surface.clone()),
        });
    }

    // 2. Inner lateral quads (reversed winding — normals point INTO the hole)
    for i in 0..n {
        let j = (i + 1) % n;
        // Reverse winding: i→j becomes j→i for inward normal
        let edge_bot = v3_sub(inner_bottom[i], inner_bottom[j]);
        let edge_up = v3_sub(inner_top[j], inner_bottom[j]);
        let normal = v3_normalize(v3_cross(edge_bot, edge_up));
        faces.push(FacePoly {
            verts: vec![inner_bottom[j], inner_bottom[i], inner_top[i], inner_top[j]],
            normal,
            origin: inner_bottom[j],
            surface_geom: Some(inner_cyl_surface.clone()),
        });
    }

    // 3. Top annular cap — triangulated fan connecting outer and inner circles
    //    Normal = +direction (outward = up)
    let outer_top_center = [
        outer.center_bottom[0] + dir[0] * outer.depth,
        outer.center_bottom[1] + dir[1] * outer.depth,
        outer.center_bottom[2] + dir[2] * outer.depth,
    ];
    for i in 0..n {
        let j = (i + 1) % n;
        // Triangle: outer_top[i] → outer_top[j] → inner_top[j]
        faces.push(FacePoly {
            verts: vec![outer_top[i], outer_top[j], inner_top[j]],
            normal: dir,
            origin: outer_top_center,
            surface_geom: None,
        });
        // Triangle: outer_top[i] → inner_top[j] → inner_top[i]
        faces.push(FacePoly {
            verts: vec![outer_top[i], inner_top[j], inner_top[i]],
            normal: dir,
            origin: outer_top_center,
            surface_geom: None,
        });
    }

    // 4. Bottom cap
    if through_hole {
        // Bottom annular cap — same triangulated fan, normal = -direction
        for i in 0..n {
            let j = (i + 1) % n;
            // Reversed winding for -direction normal
            // Triangle: outer_bottom[j] → outer_bottom[i] → inner_bottom[i]
            faces.push(FacePoly {
                verts: vec![outer_bottom[j], outer_bottom[i], inner_bottom[i]],
                normal: neg_dir,
                origin: outer.center_bottom,
                surface_geom: None,
            });
            // Triangle: outer_bottom[j] → inner_bottom[i] → inner_bottom[j]
            faces.push(FacePoly {
                verts: vec![outer_bottom[j], inner_bottom[i], inner_bottom[j]],
                normal: neg_dir,
                origin: outer.center_bottom,
                surface_geom: None,
            });
        }
    } else {
        // Blind hole: full bottom cap (no hole at bottom)
        let mut bottom_verts = outer_bottom.clone();
        bottom_verts.reverse();
        faces.push(FacePoly {
            verts: bottom_verts,
            normal: neg_dir,
            origin: outer.center_bottom,
            surface_geom: None,
        });

        // Inner bottom cap (circular face at bottom of blind hole)
        // Normal = +direction (facing up, into the hole)
        faces.push(FacePoly {
            verts: inner_bottom.clone(), // CCW when viewed from +dir
            normal: dir,
            origin: inner_bottom_center,
            surface_geom: None,
        });
    }

    // ── Stitch into B-Rep ──
    build_brep_from_polygons_inner(&faces, TAU_MODEL, true, id_alloc)
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

    let mut face_geometry = BTreeMap::new();
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
    let mut edge_geometry = BTreeMap::new();
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
    let mut face_map = BTreeMap::new();
    let mut edge_map = BTreeMap::new();
    let mut vertex_map = BTreeMap::new();
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
            if plane.normal.z < -CAP_FACE_NORMAL_Z {
                face_bot = Some(fi);
            } else if plane.normal.z > CAP_FACE_NORMAL_Z {
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

/// Build a planar-solid-minus-enclosed-cylinder result for non-rectangular
/// all-planar solids (e.g., gear extrudes). Same topology as
/// `build_box_minus_enclosed_cyl` but uses the solid's actual face polygons
/// instead of an AABB box.
///
/// `cyl` must be in the Z-aligned frame. `m` is the rotation matrix from
/// world to Z-aligned frame (used to rotate face polygons).
fn build_planar_solid_minus_enclosed_cyl(
    solid: &WaffleSolid,
    cyl: &CylinderParams,
    m: &Mat3,
    id_alloc: &mut dyn FnMut() -> u64,
) -> Result<BooleanResult, KernelError> {
    let cx = cyl.center_bottom[0];
    let cy = cyl.center_bottom[1];
    let r = cyl.radius;
    let dir = cyl.direction;

    // Cylinder's actual z-range (already clipped to solid z-range by caller)
    let cyl_z_min = cyl.center_bottom[2];
    let cyl_z_max = cyl_z_min + cyl.depth;

    // Determine if cylinder touches caps
    let touches_bot = true; // Caller clips cyl to solid Z, so always touches
    let touches_top = true;

    // Extract face polygons from the solid and rotate into Z-aligned frame
    let orig_polys = extract_face_polys(solid);
    if orig_polys.len() < 4 {
        return Err(KernelError::BooleanFailed {
            reason: "too few face polygons for planar solid".to_string(),
        });
    }
    let rotated_polys: Vec<FacePoly> = orig_polys
        .iter()
        .map(|fp| FacePoly {
            verts: fp.verts.iter().map(|v| mat3_mul_vec(m, *v)).collect(),
            normal: mat3_mul_vec(m, fp.normal),
            origin: mat3_mul_vec(m, fp.origin),
            surface_geom: None, // Will be reconstructed from rotated geometry
        })
        .collect();

    // Build B-Rep from the rotated face polygons
    let tau_weld = TAU_MODEL;
    let mut result = build_brep_from_polygons(&rotated_polys, tau_weld, id_alloc)?;

    // Find bottom and top face indices by normal direction
    let mut face_bot = None;
    let mut face_top = None;
    for (&fi, geom) in &result.face_geometry {
        if let SurfaceGeom::Planar(plane) = geom {
            if plane.normal.z < -CAP_FACE_NORMAL_Z {
                face_bot = Some(fi);
            } else if plane.normal.z > CAP_FACE_NORMAL_Z {
                face_top = Some(fi);
            }
        }
    }
    let face_bot = face_bot.ok_or(KernelError::BooleanFailed {
        reason: "cannot find bottom face in planar solid".to_string(),
    })?;
    let face_top = face_top.ok_or(KernelError::BooleanFailed {
        reason: "cannot find top face in planar solid".to_string(),
    })?;

    // Add cylinder seam vertices
    let bot_seam = [cx + r, cy, cyl_z_min];
    let top_seam = [cx + r, cy, cyl_z_max];
    let v_bot_seam = result.arena.add_vertex(bot_seam);
    let v_top_seam = result.arena.add_vertex(top_seam);

    let shell_idx = ShellIdx(0);

    // Bottom circle edge
    let (e_bot_circle, he_ibot_a, he_ibot_b) = result.arena.add_edge();

    if touches_bot {
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
        let face_cap_bot = result.arena.add_face(shell_idx);
        let loop_cap_bot = result.arena.add_loop(face_cap_bot);
        result.arena.faces[face_cap_bot.0].outer_loop = loop_cap_bot;
        result.arena.half_edges[he_ibot_a.0].origin = v_bot_seam;
        result.arena.half_edges[he_ibot_a.0].next = he_ibot_a;
        result.arena.half_edges[he_ibot_a.0].prev = he_ibot_a;
        result.arena.half_edges[he_ibot_a.0].loop_ = loop_cap_bot;
        result.arena.loops[loop_cap_bot.0].half_edge = he_ibot_a;
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
        let face_cap_top = result.arena.add_face(shell_idx);
        let loop_cap_top = result.arena.add_loop(face_cap_top);
        result.arena.faces[face_cap_top.0].outer_loop = loop_cap_top;
        result.arena.half_edges[he_itop_a.0].origin = v_top_seam;
        result.arena.half_edges[he_itop_a.0].next = he_itop_a;
        result.arena.half_edges[he_itop_a.0].prev = he_itop_a;
        result.arena.half_edges[he_itop_a.0].loop_ = loop_cap_top;
        result.arena.loops[loop_cap_top.0].half_edge = he_itop_a;
        result.face_geometry.insert(
            face_cap_top,
            SurfaceGeom::Planar(Plane {
                origin: Point3::new(cx, cy, cyl_z_max),
                normal: Vector3::new(0.0, 0.0, 1.0),
            }),
        );
        result.face_map.insert(id_alloc(), face_cap_top);
    }

    // Cylinder side face
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

    // Face geometry for cylinder face (negative radius = inward-facing normals)
    result.face_geometry.insert(
        face_cyl,
        SurfaceGeom::Cylindrical(Cylinder {
            origin: Point3::from_array(cyl.center_bottom),
            axis: Vector3::from_array(dir),
            radius: -r,
        }),
    );

    // Edge geometry
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

    // Add IDs for new entities
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

/// Build B-Rep for cylinder with an enclosed rectangular subtract.
///
/// Handles 4 cap-touching cases (mirrors `build_box_minus_enclosed_cyl`):
/// - touches_bot && touches_top: through-hole, inner loops on both caps (7 faces, chi=2)
/// - !touches_bot && !touches_top: blind pocket, standalone floor+ceiling faces (9 faces, chi=4)
/// - mixed: inner loop on touched cap, standalone face on untouched cap (8 faces, chi=3)
fn build_cyl_minus_enclosed_box(
    aabb: &Aabb,
    cyl: &CylinderParams,
    id_alloc: &mut dyn FnMut() -> u64,
) -> Result<BooleanResult, KernelError> {
    let (cyl_z_min, cyl_z_max) = ssi::cyl_z_range(cyl);
    let box_z_min = aabb.min[2];
    let box_z_max = aabb.max[2];
    let touches_bot = (box_z_min - cyl_z_min).abs() < TAU_COINCIDENT;
    let touches_top = (box_z_max - cyl_z_max).abs() < TAU_COINCIDENT;

    // Step 1: Build standalone cylinder as base
    let mut result = build_cyl_result(cyl, id_alloc)?;

    // Step 2: Find bottom and top cap faces by normal direction
    let mut face_bot = None;
    let mut face_top = None;
    for (&fi, geom) in &result.face_geometry {
        if let SurfaceGeom::Planar(plane) = geom {
            if plane.normal.z < -CAP_FACE_NORMAL_Z {
                face_bot = Some(fi);
            } else if plane.normal.z > CAP_FACE_NORMAL_Z {
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

    // Step 3: Box corner positions at actual box Z positions
    let bx0 = aabb.min[0];
    let bx1 = aabb.max[0];
    let by0 = aabb.min[1];
    let by1 = aabb.max[1];
    let z_bot = if touches_bot { cyl_z_min } else { box_z_min };
    let z_top = if touches_top { cyl_z_max } else { box_z_max };

    let v_b0 = result.arena.add_vertex([bx0, by0, z_bot]); // bottom-left, bottom
    let v_b1 = result.arena.add_vertex([bx1, by0, z_bot]); // bottom-right, bottom
    let v_b2 = result.arena.add_vertex([bx1, by1, z_bot]); // top-right, bottom
    let v_b3 = result.arena.add_vertex([bx0, by1, z_bot]); // top-left, bottom
    let v_t0 = result.arena.add_vertex([bx0, by0, z_top]); // bottom-left, top
    let v_t1 = result.arena.add_vertex([bx1, by0, z_top]); // bottom-right, top
    let v_t2 = result.arena.add_vertex([bx1, by1, z_top]); // top-right, top
    let v_t3 = result.arena.add_vertex([bx0, by1, z_top]); // top-left, top

    let shell_idx = ShellIdx(0);

    // Step 4: Bottom rectangle edges
    let (e_br0, he_br0_a, he_br0_b) = result.arena.add_edge(); // b0→b3
    let (e_br1, he_br1_a, he_br1_b) = result.arena.add_edge(); // b3→b2
    let (e_br2, he_br2_a, he_br2_b) = result.arena.add_edge(); // b2→b1
    let (e_br3, he_br3_a, he_br3_b) = result.arena.add_edge(); // b1→b0

    if touches_bot {
        // Inner loop on bottom cap (hole in annular face)
        let inner_loop_bot = result.arena.add_loop(face_bot);
        result.arena.faces[face_bot.0]
            .inner_loops
            .push(inner_loop_bot);

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
    } else {
        // Standalone floor face (pocket bottom) — no hole in cylinder cap
        let face_floor = result.arena.add_face(shell_idx);
        let loop_floor = result.arena.add_loop(face_floor);
        result.arena.faces[face_floor.0].outer_loop = loop_floor;

        // Floor loop: b0 → b3 → b2 → b1 → b0 (same winding as inner loop would be)
        result.arena.half_edges[he_br0_a.0].origin = v_b0;
        result.arena.half_edges[he_br0_a.0].next = he_br1_a;
        result.arena.half_edges[he_br0_a.0].prev = he_br3_a;
        result.arena.half_edges[he_br0_a.0].loop_ = loop_floor;

        result.arena.half_edges[he_br1_a.0].origin = v_b3;
        result.arena.half_edges[he_br1_a.0].next = he_br2_a;
        result.arena.half_edges[he_br1_a.0].prev = he_br0_a;
        result.arena.half_edges[he_br1_a.0].loop_ = loop_floor;

        result.arena.half_edges[he_br2_a.0].origin = v_b2;
        result.arena.half_edges[he_br2_a.0].next = he_br3_a;
        result.arena.half_edges[he_br2_a.0].prev = he_br1_a;
        result.arena.half_edges[he_br2_a.0].loop_ = loop_floor;

        result.arena.half_edges[he_br3_a.0].origin = v_b1;
        result.arena.half_edges[he_br3_a.0].next = he_br0_a;
        result.arena.half_edges[he_br3_a.0].prev = he_br2_a;
        result.arena.half_edges[he_br3_a.0].loop_ = loop_floor;

        result.arena.loops[loop_floor.0].half_edge = he_br0_a;

        // Floor geometry: upward-facing normal (closing pocket from below, facing into void)
        result.face_geometry.insert(
            face_floor,
            SurfaceGeom::Planar(Plane {
                origin: Point3::new(bx0, by0, z_bot),
                normal: Vector3::new(0.0, 0.0, 1.0),
            }),
        );
        result.face_map.insert(id_alloc(), face_floor);
    }

    // Step 5: Top rectangle edges
    let (e_tr0, he_tr0_a, he_tr0_b) = result.arena.add_edge(); // t0→t1
    let (e_tr1, he_tr1_a, he_tr1_b) = result.arena.add_edge(); // t1→t2
    let (e_tr2, he_tr2_a, he_tr2_b) = result.arena.add_edge(); // t2→t3
    let (e_tr3, he_tr3_a, he_tr3_b) = result.arena.add_edge(); // t3→t0

    if touches_top {
        // Inner loop on top cap (hole in annular face)
        let inner_loop_top = result.arena.add_loop(face_top);
        result.arena.faces[face_top.0]
            .inner_loops
            .push(inner_loop_top);

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
    } else {
        // Standalone ceiling face (pocket top) — no hole in cylinder cap
        let face_ceil = result.arena.add_face(shell_idx);
        let loop_ceil = result.arena.add_loop(face_ceil);
        result.arena.faces[face_ceil.0].outer_loop = loop_ceil;

        // Ceiling loop: t0 → t1 → t2 → t3 → t0 (same winding as inner loop would be)
        result.arena.half_edges[he_tr0_a.0].origin = v_t0;
        result.arena.half_edges[he_tr0_a.0].next = he_tr1_a;
        result.arena.half_edges[he_tr0_a.0].prev = he_tr3_a;
        result.arena.half_edges[he_tr0_a.0].loop_ = loop_ceil;

        result.arena.half_edges[he_tr1_a.0].origin = v_t1;
        result.arena.half_edges[he_tr1_a.0].next = he_tr2_a;
        result.arena.half_edges[he_tr1_a.0].prev = he_tr0_a;
        result.arena.half_edges[he_tr1_a.0].loop_ = loop_ceil;

        result.arena.half_edges[he_tr2_a.0].origin = v_t2;
        result.arena.half_edges[he_tr2_a.0].next = he_tr3_a;
        result.arena.half_edges[he_tr2_a.0].prev = he_tr1_a;
        result.arena.half_edges[he_tr2_a.0].loop_ = loop_ceil;

        result.arena.half_edges[he_tr3_a.0].origin = v_t3;
        result.arena.half_edges[he_tr3_a.0].next = he_tr0_a;
        result.arena.half_edges[he_tr3_a.0].prev = he_tr2_a;
        result.arena.half_edges[he_tr3_a.0].loop_ = loop_ceil;

        result.arena.loops[loop_ceil.0].half_edge = he_tr0_a;

        // Ceiling geometry: downward-facing normal (closing pocket from above, facing into void)
        result.face_geometry.insert(
            face_ceil,
            SurfaceGeom::Planar(Plane {
                origin: Point3::new(bx0, by0, z_top),
                normal: Vector3::new(0.0, 0.0, -1.0),
            }),
        );
        result.face_map.insert(id_alloc(), face_ceil);
    }

    // Step 6: 4 inner rectangular wall faces (inward-facing normals)
    // Each wall connects a bottom edge to a top edge via 2 vertical edges.
    // Wall 0: front (y=by0, normal +Y inward) — b0→b1 bottom, t1→t0 top
    // Wall 1: right (x=bx1, normal -X inward) — b1→b2 bottom, t2→t1 top
    // Wall 2: back  (y=by1, normal -Y inward) — b2→b3 bottom, t3→t2 top
    // Wall 3: left  (x=bx0, normal +X inward) — b3→b0 bottom, t0→t3 top

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
            origin: Point3::from_array([bx0, by0, z_bot]),
            normal: Vector3::new(0.0, 1.0, 0.0),
        }),
    );
    result.face_geometry.insert(
        face_w1,
        SurfaceGeom::Planar(Plane {
            origin: Point3::from_array([bx1, by0, z_bot]),
            normal: Vector3::new(-1.0, 0.0, 0.0),
        }),
    );
    result.face_geometry.insert(
        face_w2,
        SurfaceGeom::Planar(Plane {
            origin: Point3::from_array([bx1, by1, z_bot]),
            normal: Vector3::new(0.0, -1.0, 0.0),
        }),
    );
    result.face_geometry.insert(
        face_w3,
        SurfaceGeom::Planar(Plane {
            origin: Point3::from_array([bx0, by0, z_bot]),
            normal: Vector3::new(1.0, 0.0, 0.0),
        }),
    );

    // Step 8: Edge geometry
    // Bottom rect edges (linear)
    let h = z_top - z_bot;
    result.edge_geometry.insert(
        e_br0,
        CurveGeom::Linear(Line3D {
            origin: Point3::from_array([bx0, by0, z_bot]),
            direction: Vector3::new(0.0, by1 - by0, 0.0),
        }),
    );
    result.edge_geometry.insert(
        e_br1,
        CurveGeom::Linear(Line3D {
            origin: Point3::from_array([bx0, by1, z_bot]),
            direction: Vector3::new(bx1 - bx0, 0.0, 0.0),
        }),
    );
    result.edge_geometry.insert(
        e_br2,
        CurveGeom::Linear(Line3D {
            origin: Point3::from_array([bx1, by1, z_bot]),
            direction: Vector3::new(0.0, by0 - by1, 0.0),
        }),
    );
    result.edge_geometry.insert(
        e_br3,
        CurveGeom::Linear(Line3D {
            origin: Point3::from_array([bx1, by0, z_bot]),
            direction: Vector3::new(bx0 - bx1, 0.0, 0.0),
        }),
    );

    // Top rect edges (linear)
    result.edge_geometry.insert(
        e_tr0,
        CurveGeom::Linear(Line3D {
            origin: Point3::from_array([bx0, by0, z_top]),
            direction: Vector3::new(bx1 - bx0, 0.0, 0.0),
        }),
    );
    result.edge_geometry.insert(
        e_tr1,
        CurveGeom::Linear(Line3D {
            origin: Point3::from_array([bx1, by0, z_top]),
            direction: Vector3::new(0.0, by1 - by0, 0.0),
        }),
    );
    result.edge_geometry.insert(
        e_tr2,
        CurveGeom::Linear(Line3D {
            origin: Point3::from_array([bx1, by1, z_top]),
            direction: Vector3::new(bx0 - bx1, 0.0, 0.0),
        }),
    );
    result.edge_geometry.insert(
        e_tr3,
        CurveGeom::Linear(Line3D {
            origin: Point3::from_array([bx0, by1, z_top]),
            direction: Vector3::new(0.0, by0 - by1, 0.0),
        }),
    );

    // Vertical edges (linear)
    result.edge_geometry.insert(
        e_v0,
        CurveGeom::Linear(Line3D {
            origin: Point3::from_array([bx0, by0, z_bot]),
            direction: Vector3::new(0.0, 0.0, h),
        }),
    );
    result.edge_geometry.insert(
        e_v1,
        CurveGeom::Linear(Line3D {
            origin: Point3::from_array([bx1, by0, z_bot]),
            direction: Vector3::new(0.0, 0.0, h),
        }),
    );
    result.edge_geometry.insert(
        e_v2,
        CurveGeom::Linear(Line3D {
            origin: Point3::from_array([bx1, by1, z_bot]),
            direction: Vector3::new(0.0, 0.0, h),
        }),
    );
    result.edge_geometry.insert(
        e_v3,
        CurveGeom::Linear(Line3D {
            origin: Point3::from_array([bx0, by1, z_bot]),
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
                plane.normal.z > CAP_FACE_NORMAL_Z
            } else {
                plane.normal.z < -CAP_FACE_NORMAL_Z
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

/// Build a cylinder with a rectangular box boss on one cap.
///
/// The box is XY-enclosed in the cylinder but extends beyond one cap in Z.
/// Result: cylinder side + unpunched cap + annular cap (with rect hole) + 4 box side quads + box end cap = 8 faces.
fn build_cyl_with_box_boss(
    aabb: &Aabb,
    cyl: &CylinderParams,
    on_top: bool,
    id_alloc: &mut dyn FnMut() -> u64,
) -> Result<BooleanResult, KernelError> {
    let (cyl_z_min, cyl_z_max) = ssi::cyl_z_range(cyl);

    // Start with a clean cylinder B-Rep
    let mut result = build_cyl_result(cyl, id_alloc)?;

    // Find the cap face to punch (top or bottom of cylinder)
    let punch_z = if on_top { cyl_z_max } else { cyl_z_min };
    let mut face_punch = None;
    for (&fi, geom) in &result.face_geometry {
        if let SurfaceGeom::Planar(plane) = geom {
            let matches = if on_top {
                plane.normal.z > CAP_FACE_NORMAL_Z
            } else {
                plane.normal.z < -CAP_FACE_NORMAL_Z
            };
            if matches {
                face_punch = Some(fi);
            }
        }
    }
    let face_punch = face_punch.ok_or(KernelError::BooleanFailed {
        reason: "cannot find cylinder cap to punch for box boss".to_string(),
    })?;

    // Box end Z (the end away from the cylinder)
    let box_end_z = if on_top { aabb.max[2] } else { aabb.min[2] };
    let boss_dir: [f64; 3] = if on_top {
        [0.0, 0.0, 1.0]
    } else {
        [0.0, 0.0, -1.0]
    };

    // Box corner coordinates at punch Z and at boss end Z
    let bx0 = aabb.min[0];
    let bx1 = aabb.max[0];
    let by0 = aabb.min[1];
    let by1 = aabb.max[1];

    // 4 punch-level vertices (on the cylinder cap plane)
    let v_p0 = result.arena.add_vertex([bx0, by0, punch_z]);
    let v_p1 = result.arena.add_vertex([bx1, by0, punch_z]);
    let v_p2 = result.arena.add_vertex([bx1, by1, punch_z]);
    let v_p3 = result.arena.add_vertex([bx0, by1, punch_z]);

    // 4 boss-end vertices
    let v_e0 = result.arena.add_vertex([bx0, by0, box_end_z]);
    let v_e1 = result.arena.add_vertex([bx1, by0, box_end_z]);
    let v_e2 = result.arena.add_vertex([bx1, by1, box_end_z]);
    let v_e3 = result.arena.add_vertex([bx0, by1, box_end_z]);

    // Add rectangular inner loop to the punched cap
    let inner_loop = result.arena.add_loop(face_punch);
    result.arena.faces[face_punch.0]
        .inner_loops
        .push(inner_loop);

    // 4 edges for the inner rectangle on the punched cap
    let (e_ip0, he_ip0_a, he_ip0_b) = result.arena.add_edge(); // p0→p1
    let (e_ip1, he_ip1_a, he_ip1_b) = result.arena.add_edge(); // p1→p2
    let (e_ip2, he_ip2_a, he_ip2_b) = result.arena.add_edge(); // p2→p3
    let (e_ip3, he_ip3_a, he_ip3_b) = result.arena.add_edge(); // p3→p0

    // Inner loop winding: for an inner loop (hole), winding is CW from outside.
    // When on_top (normal +Z), inner loop goes p0→p3→p2→p1 (CW from +Z).
    // When on_bottom (normal -Z), inner loop goes p0→p1→p2→p3 (CW from -Z = CCW from +Z).
    if on_top {
        // CW from +Z: p0→p3→p2→p1
        result.arena.half_edges[he_ip3_b.0].origin = v_p0;
        result.arena.half_edges[he_ip3_b.0].next = he_ip2_b;
        result.arena.half_edges[he_ip3_b.0].prev = he_ip0_b;
        result.arena.half_edges[he_ip3_b.0].loop_ = inner_loop;

        result.arena.half_edges[he_ip2_b.0].origin = v_p3;
        result.arena.half_edges[he_ip2_b.0].next = he_ip1_b;
        result.arena.half_edges[he_ip2_b.0].prev = he_ip3_b;
        result.arena.half_edges[he_ip2_b.0].loop_ = inner_loop;

        result.arena.half_edges[he_ip1_b.0].origin = v_p2;
        result.arena.half_edges[he_ip1_b.0].next = he_ip0_b;
        result.arena.half_edges[he_ip1_b.0].prev = he_ip2_b;
        result.arena.half_edges[he_ip1_b.0].loop_ = inner_loop;

        result.arena.half_edges[he_ip0_b.0].origin = v_p1;
        result.arena.half_edges[he_ip0_b.0].next = he_ip3_b;
        result.arena.half_edges[he_ip0_b.0].prev = he_ip1_b;
        result.arena.half_edges[he_ip0_b.0].loop_ = inner_loop;

        result.arena.loops[inner_loop.0].half_edge = he_ip3_b;
    } else {
        // CW from -Z: p0→p1→p2→p3
        result.arena.half_edges[he_ip0_a.0].origin = v_p0;
        result.arena.half_edges[he_ip0_a.0].next = he_ip1_a;
        result.arena.half_edges[he_ip0_a.0].prev = he_ip3_a;
        result.arena.half_edges[he_ip0_a.0].loop_ = inner_loop;

        result.arena.half_edges[he_ip1_a.0].origin = v_p1;
        result.arena.half_edges[he_ip1_a.0].next = he_ip2_a;
        result.arena.half_edges[he_ip1_a.0].prev = he_ip0_a;
        result.arena.half_edges[he_ip1_a.0].loop_ = inner_loop;

        result.arena.half_edges[he_ip2_a.0].origin = v_p2;
        result.arena.half_edges[he_ip2_a.0].next = he_ip3_a;
        result.arena.half_edges[he_ip2_a.0].prev = he_ip1_a;
        result.arena.half_edges[he_ip2_a.0].loop_ = inner_loop;

        result.arena.half_edges[he_ip3_a.0].origin = v_p3;
        result.arena.half_edges[he_ip3_a.0].next = he_ip0_a;
        result.arena.half_edges[he_ip3_a.0].prev = he_ip2_a;
        result.arena.half_edges[he_ip3_a.0].loop_ = inner_loop;

        result.arena.loops[inner_loop.0].half_edge = he_ip0_a;
    }

    // 4 vertical edges connecting punch to end
    let (e_v0, he_v0_a, he_v0_b) = result.arena.add_edge(); // p0→e0
    let (e_v1, he_v1_a, he_v1_b) = result.arena.add_edge(); // p1→e1
    let (e_v2, he_v2_a, he_v2_b) = result.arena.add_edge(); // p2→e2
    let (e_v3, he_v3_a, he_v3_b) = result.arena.add_edge(); // p3→e3

    // 4 edges for the box end cap
    let (e_ep0, he_ep0_a, he_ep0_b) = result.arena.add_edge(); // e0→e1
    let (e_ep1, he_ep1_a, he_ep1_b) = result.arena.add_edge(); // e1→e2
    let (e_ep2, he_ep2_a, he_ep2_b) = result.arena.add_edge(); // e2→e3
    let (e_ep3, he_ep3_a, he_ep3_b) = result.arena.add_edge(); // e3→e0

    let shell_idx = ShellIdx(0);

    // 4 box side quad faces
    // Each quad: punch_i → punch_{i+1} → end_{i+1} → end_i (outward normal)
    // Side face winding depends on boss direction.
    // Vertices are ordered so the outward normal points away from the box center.

    // Helper: corner pairs for the 4 sides (CCW order when viewed from +Z)
    let punch_verts = [v_p0, v_p1, v_p2, v_p3];
    let end_verts = [v_e0, v_e1, v_e2, v_e3];
    let inner_he_a = [he_ip0_a, he_ip1_a, he_ip2_a, he_ip3_a];
    let inner_he_b = [he_ip0_b, he_ip1_b, he_ip2_b, he_ip3_b];
    let vert_he_a = [he_v0_a, he_v1_a, he_v2_a, he_v3_a];
    let vert_he_b = [he_v0_b, he_v1_b, he_v2_b, he_v3_b];
    let end_he_a = [he_ep0_a, he_ep1_a, he_ep2_a, he_ep3_a];
    let end_he_b = [he_ep0_b, he_ep1_b, he_ep2_b, he_ep3_b];

    // Side face normals (outward)
    let side_normals: [[f64; 3]; 4] = [
        [0.0, -1.0, 0.0], // p0→p1 (y=by0, -Y)
        [1.0, 0.0, 0.0],  // p1→p2 (x=bx1, +X)
        [0.0, 1.0, 0.0],  // p2→p3 (y=by1, +Y)
        [-1.0, 0.0, 0.0], // p3→p0 (x=bx0, -X)
    ];
    let side_origins: [[f64; 3]; 4] = [
        [bx0, by0, punch_z],
        [bx1, by0, punch_z],
        [bx0, by1, punch_z],
        [bx0, by0, punch_z],
    ];

    let mut side_faces = Vec::new();
    for i in 0..4 {
        let ni = (i + 1) % 4;
        let face_side = result.arena.add_face(shell_idx);
        let loop_side = result.arena.add_loop(face_side);
        result.arena.faces[face_side.0].outer_loop = loop_side;
        side_faces.push(face_side);

        // Each side quad loop: 4 half-edges
        // For on_top: punch_i → punch_ni (up inner edge _a) → end_ni (vert _a) → end_i (end edge _b reverse) → punch_i (vert _b)
        // Actually, the loop goes: bottom_edge → right_vert → top_edge → left_vert
        // With outward normal pointing away from box center.
        //
        // When on_top (boss extends upward):
        //   Loop (CCW from outward normal): punch_i → punch_ni → end_ni → end_i
        //   half-edges: ip_i_a (punch_i→punch_ni) → v_ni_a (punch_ni→end_ni) → ep_i_b (end_ni→end_i) → v_i_b (end_i→punch_i)
        //
        // When on_bottom (boss extends downward):
        //   Loop (CCW from outward normal): punch_ni → punch_i → end_i → end_ni
        //   half-edges: ip_i_b (punch_ni→punch_i) → v_i_a (punch_i→end_i) → ep_i_a (end_i→end_ni) → v_ni_b (end_ni→punch_ni)

        if on_top {
            // CCW from outward: punch_i → punch_ni → end_ni → end_i
            let h_bottom = inner_he_a[i];
            let h_right = vert_he_a[ni];
            let h_top = end_he_b[i];
            let h_left = vert_he_b[i];

            result.arena.half_edges[h_bottom.0].origin = punch_verts[i];
            result.arena.half_edges[h_bottom.0].next = h_right;
            result.arena.half_edges[h_bottom.0].prev = h_left;
            result.arena.half_edges[h_bottom.0].loop_ = loop_side;

            result.arena.half_edges[h_right.0].origin = punch_verts[ni];
            result.arena.half_edges[h_right.0].next = h_top;
            result.arena.half_edges[h_right.0].prev = h_bottom;
            result.arena.half_edges[h_right.0].loop_ = loop_side;

            result.arena.half_edges[h_top.0].origin = end_verts[ni];
            result.arena.half_edges[h_top.0].next = h_left;
            result.arena.half_edges[h_top.0].prev = h_right;
            result.arena.half_edges[h_top.0].loop_ = loop_side;

            result.arena.half_edges[h_left.0].origin = end_verts[i];
            result.arena.half_edges[h_left.0].next = h_bottom;
            result.arena.half_edges[h_left.0].prev = h_top;
            result.arena.half_edges[h_left.0].loop_ = loop_side;

            result.arena.loops[loop_side.0].half_edge = h_bottom;
        } else {
            // CCW from outward: punch_ni → punch_i → end_i → end_ni
            let h_bottom = inner_he_b[i];
            let h_right = vert_he_a[i];
            let h_top = end_he_a[i];
            let h_left = vert_he_b[ni];

            result.arena.half_edges[h_bottom.0].origin = punch_verts[ni];
            result.arena.half_edges[h_bottom.0].next = h_right;
            result.arena.half_edges[h_bottom.0].prev = h_left;
            result.arena.half_edges[h_bottom.0].loop_ = loop_side;

            result.arena.half_edges[h_right.0].origin = punch_verts[i];
            result.arena.half_edges[h_right.0].next = h_top;
            result.arena.half_edges[h_right.0].prev = h_bottom;
            result.arena.half_edges[h_right.0].loop_ = loop_side;

            result.arena.half_edges[h_top.0].origin = end_verts[i];
            result.arena.half_edges[h_top.0].next = h_left;
            result.arena.half_edges[h_top.0].prev = h_right;
            result.arena.half_edges[h_top.0].loop_ = loop_side;

            result.arena.half_edges[h_left.0].origin = end_verts[ni];
            result.arena.half_edges[h_left.0].next = h_bottom;
            result.arena.half_edges[h_left.0].prev = h_top;
            result.arena.half_edges[h_left.0].loop_ = loop_side;

            result.arena.loops[loop_side.0].half_edge = h_bottom;
        }

        // Face geometry for side
        result.face_geometry.insert(
            face_side,
            SurfaceGeom::Planar(Plane {
                origin: Point3::from_array(side_origins[i]),
                normal: Vector3::from_array(side_normals[i]),
            }),
        );
    }

    // Box end cap face
    let face_end = result.arena.add_face(shell_idx);
    let loop_end = result.arena.add_loop(face_end);
    result.arena.faces[face_end.0].outer_loop = loop_end;

    // End cap winding: CCW from boss_dir
    if on_top {
        // CCW from +Z: e0→e1→e2→e3
        result.arena.half_edges[he_ep0_a.0].origin = v_e0;
        result.arena.half_edges[he_ep0_a.0].next = he_ep1_a;
        result.arena.half_edges[he_ep0_a.0].prev = he_ep3_a;
        result.arena.half_edges[he_ep0_a.0].loop_ = loop_end;

        result.arena.half_edges[he_ep1_a.0].origin = v_e1;
        result.arena.half_edges[he_ep1_a.0].next = he_ep2_a;
        result.arena.half_edges[he_ep1_a.0].prev = he_ep0_a;
        result.arena.half_edges[he_ep1_a.0].loop_ = loop_end;

        result.arena.half_edges[he_ep2_a.0].origin = v_e2;
        result.arena.half_edges[he_ep2_a.0].next = he_ep3_a;
        result.arena.half_edges[he_ep2_a.0].prev = he_ep1_a;
        result.arena.half_edges[he_ep2_a.0].loop_ = loop_end;

        result.arena.half_edges[he_ep3_a.0].origin = v_e3;
        result.arena.half_edges[he_ep3_a.0].next = he_ep0_a;
        result.arena.half_edges[he_ep3_a.0].prev = he_ep2_a;
        result.arena.half_edges[he_ep3_a.0].loop_ = loop_end;

        result.arena.loops[loop_end.0].half_edge = he_ep0_a;
    } else {
        // CCW from -Z: e0→e3→e2→e1
        result.arena.half_edges[he_ep3_b.0].origin = v_e0;
        result.arena.half_edges[he_ep3_b.0].next = he_ep2_b;
        result.arena.half_edges[he_ep3_b.0].prev = he_ep0_b;
        result.arena.half_edges[he_ep3_b.0].loop_ = loop_end;

        result.arena.half_edges[he_ep2_b.0].origin = v_e3;
        result.arena.half_edges[he_ep2_b.0].next = he_ep1_b;
        result.arena.half_edges[he_ep2_b.0].prev = he_ep3_b;
        result.arena.half_edges[he_ep2_b.0].loop_ = loop_end;

        result.arena.half_edges[he_ep1_b.0].origin = v_e2;
        result.arena.half_edges[he_ep1_b.0].next = he_ep0_b;
        result.arena.half_edges[he_ep1_b.0].prev = he_ep2_b;
        result.arena.half_edges[he_ep1_b.0].loop_ = loop_end;

        result.arena.half_edges[he_ep0_b.0].origin = v_e1;
        result.arena.half_edges[he_ep0_b.0].next = he_ep3_b;
        result.arena.half_edges[he_ep0_b.0].prev = he_ep1_b;
        result.arena.half_edges[he_ep0_b.0].loop_ = loop_end;

        result.arena.loops[loop_end.0].half_edge = he_ep3_b;
    }

    result.face_geometry.insert(
        face_end,
        SurfaceGeom::Planar(Plane {
            origin: Point3::from_array([bx0, by0, box_end_z]),
            normal: Vector3::from_array(boss_dir),
        }),
    );

    // Vertex half-edge refs
    result.arena.vertices[v_p0.0].half_edge = Some(vert_he_a[0]);
    result.arena.vertices[v_p1.0].half_edge = Some(vert_he_a[1]);
    result.arena.vertices[v_p2.0].half_edge = Some(vert_he_a[2]);
    result.arena.vertices[v_p3.0].half_edge = Some(vert_he_a[3]);
    result.arena.vertices[v_e0.0].half_edge = Some(end_he_a[0]);
    result.arena.vertices[v_e1.0].half_edge = Some(end_he_a[1]);
    result.arena.vertices[v_e2.0].half_edge = Some(end_he_a[2]);
    result.arena.vertices[v_e3.0].half_edge = Some(end_he_a[3]);

    // Edge geometry: all new edges are linear
    let boss_height = (box_end_z - punch_z).abs();
    for i in 0..4 {
        let ni = (i + 1) % 4;
        // Inner rectangle edges at punch_z
        let p_i = result.arena.vertices[punch_verts[i].0].position;
        let p_ni = result.arena.vertices[punch_verts[ni].0].position;
        result.edge_geometry.insert(
            [e_ip0, e_ip1, e_ip2, e_ip3][i],
            CurveGeom::Linear(Line3D {
                origin: Point3::from_array(p_i),
                direction: Vector3::from_array(v3_sub(p_ni, p_i)),
            }),
        );

        // Vertical edges
        result.edge_geometry.insert(
            [e_v0, e_v1, e_v2, e_v3][i],
            CurveGeom::Linear(Line3D {
                origin: Point3::from_array(p_i),
                direction: Vector3::from_array(v3_scale(boss_dir, boss_height)),
            }),
        );

        // End cap edges
        let e_i = result.arena.vertices[end_verts[i].0].position;
        let e_ni = result.arena.vertices[end_verts[ni].0].position;
        result.edge_geometry.insert(
            [e_ep0, e_ep1, e_ep2, e_ep3][i],
            CurveGeom::Linear(Line3D {
                origin: Point3::from_array(e_i),
                direction: Vector3::from_array(v3_sub(e_ni, e_i)),
            }),
        );
    }

    // Add IDs for new entities
    for &f in &side_faces {
        result.face_map.insert(id_alloc(), f);
    }
    result.face_map.insert(id_alloc(), face_end);
    for &e in &[
        e_ip0, e_ip1, e_ip2, e_ip3, e_v0, e_v1, e_v2, e_v3, e_ep0, e_ep1, e_ep2, e_ep3,
    ] {
        result.edge_map.insert(id_alloc(), e);
    }
    for &v in &[v_p0, v_p1, v_p2, v_p3, v_e0, v_e1, v_e2, v_e3] {
        result.vertex_map.insert(id_alloc(), v);
    }

    Ok(result)
}

/// Build a disjoint union of two cylinders.
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
    let mut face_geometry = BTreeMap::new();
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

    let mut edge_geometry: BTreeMap<EdgeIdx, CurveGeom> = BTreeMap::new();

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

    let mut face_map = BTreeMap::new();
    let mut edge_map = BTreeMap::new();
    let mut vertex_map = BTreeMap::new();

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

// ── Box-cylinder partial union (center inside, protrudes through sides) ──

/// Build the union of a box and a partially-protruding cylinder.
///
/// The cylinder center must be inside the box XY footprint. The cylinder
/// protrudes through 1-4 side faces. The result is constructed analytically
/// using plane-cylinder SSI intersection points (Patrikalakis Ch.5 [#1]).
///
/// Uses `build_brep_from_polygons` for topology construction, with chord
/// approximation for arc boundaries. The cylinder patch face(s) get
/// `SurfaceGeom::Cylindrical` for proper curved tessellation.
fn build_box_cyl_partial_union(
    aabb: &Aabb,
    cyl: &CylinderParams,
    id_alloc: &mut dyn FnMut() -> u64,
) -> Result<BooleanResult, KernelError> {
    let cx = cyl.center_bottom[0];
    let cy = cyl.center_bottom[1];
    let r = cyl.radius;
    let (cyl_z_min, cyl_z_max) = ssi::cyl_z_range(cyl);
    let mn = aabb.min;
    let mx = aabb.max;

    // Union Z extent
    let z_bot = mn[2].min(cyl_z_min);
    let z_top = mx[2].max(cyl_z_max);

    let tau = TAU_COINCIDENT;

    // ── Find all circle-AABB intersection points ──
    let mut ipts: Vec<[f64; 2]> = Vec::new();
    // Left (x=min)
    for &iy in &ssi::circle_vline_intersections(cx, cy, r, mn[0], mn[1], mx[1]) {
        ipts.push([mn[0], iy]);
    }
    // Right (x=max)
    for &iy in &ssi::circle_vline_intersections(cx, cy, r, mx[0], mn[1], mx[1]) {
        ipts.push([mx[0], iy]);
    }
    // Front (y=min)
    for &ix in &ssi::circle_hline_intersections(cx, cy, r, mn[1], mn[0], mx[0]) {
        ipts.push([ix, mn[1]]);
    }
    // Back (y=max)
    for &ix in &ssi::circle_hline_intersections(cx, cy, r, mx[1], mn[0], mx[0]) {
        ipts.push([ix, mx[1]]);
    }

    // Sort CCW by angle from cylinder center
    ipts.sort_by(|a, b| {
        let aa = (a[1] - cy).atan2(a[0] - cx);
        let ab = (b[1] - cy).atan2(b[0] - cx);
        aa.partial_cmp(&ab).unwrap()
    });
    // Deduplicate nearby points
    ipts.dedup_by(|a, b| {
        let d = ((a[0] - b[0]).powi(2) + (a[1] - b[1]).powi(2)).sqrt();
        d < tau
    });

    if ipts.len() < 2 {
        return Err(KernelError::NotSupported {
            operation: "box-cylinder partial union: <2 intersection points".into(),
        });
    }

    let n = ipts.len();

    // ── Classify arcs: exposed (outside box) vs interior ──
    let is_exposed: Vec<bool> = (0..n)
        .map(|i| {
            let j = (i + 1) % n;
            let ai = (ipts[i][1] - cy).atan2(ipts[i][0] - cx);
            let aj = (ipts[j][1] - cy).atan2(ipts[j][0] - cx);
            let sweep = normalize_angle(aj - ai);
            let mid_a = ai + sweep / 2.0;
            let mid_x = cx + r * mid_a.cos();
            let mid_y = cy + r * mid_a.sin();
            mid_x < mn[0] - tau || mid_x > mx[0] + tau || mid_y < mn[1] - tau || mid_y > mx[1] + tau
        })
        .collect();

    // An intersection point is an "exit" if the CCW arc from it goes outside the box.
    let is_exit_pt: Vec<bool> = is_exposed.clone();
    // An intersection point is an "entry" if the CCW arc arriving at it comes from outside.
    let is_entry_pt: Vec<bool> = (0..n).map(|i| is_exposed[(i + n - 1) % n]).collect();

    // ── Build 2D union boundary (CCW) ──
    // Box corners CCW: front-left(0), front-right(1), back-right(2), back-left(3)
    let corners = [
        [mn[0], mn[1]], // 0: front-left
        [mx[0], mn[1]], // 1: front-right
        [mx[0], mx[1]], // 2: back-right
        [mn[0], mx[1]], // 3: back-left
    ];
    let corner_inside_cyl: [bool; 4] = std::array::from_fn(|i| {
        let dx = corners[i][0] - cx;
        let dy = corners[i][1] - cy;
        dx * dx + dy * dy < r * r - tau
    });

    // Identify which intersection points lie on each box side and sort by parameter
    // Sides: 0=front(y=min, x+), 1=right(x=max, y+), 2=back(y=max, x-), 3=left(x=min, y-)
    let on_side = |pt: [f64; 2], side: usize| -> bool {
        match side {
            0 => (pt[1] - mn[1]).abs() < tau && pt[0] >= mn[0] - tau && pt[0] <= mx[0] + tau,
            1 => (pt[0] - mx[0]).abs() < tau && pt[1] >= mn[1] - tau && pt[1] <= mx[1] + tau,
            2 => (pt[1] - mx[1]).abs() < tau && pt[0] >= mn[0] - tau && pt[0] <= mx[0] + tau,
            3 => (pt[0] - mn[0]).abs() < tau && pt[1] >= mn[1] - tau && pt[1] <= mx[1] + tau,
            _ => false,
        }
    };

    let side_param = |pt: [f64; 2], side: usize| -> f64 {
        match side {
            0 => pt[0] - mn[0], // x increases
            1 => pt[1] - mn[1], // y increases
            2 => mx[0] - pt[0], // x decreases
            3 => mx[1] - pt[1], // y decreases
            _ => 0.0,
        }
    };

    let mut side_ipts: [Vec<usize>; 4] = [vec![], vec![], vec![], vec![]];
    for (idx, pt) in ipts.iter().enumerate() {
        for (side, pts) in side_ipts.iter_mut().enumerate() {
            if on_side(*pt, side) {
                pts.push(idx);
            }
        }
    }
    for (side, pts) in side_ipts.iter_mut().enumerate() {
        pts.sort_by(|&a, &b| {
            side_param(ipts[a], side)
                .partial_cmp(&side_param(ipts[b], side))
                .unwrap()
        });
    }

    // Number of chord segments for arc approximation
    let n_arc_chords: usize = 16;

    // Generate chord vertices for an arc from ipts[start] to ipts[end] (CCW).
    // Returns intermediate vertices only (not endpoints).
    let arc_chords_2d = |start_idx: usize, end_idx: usize| -> Vec<[f64; 2]> {
        let a_start = (ipts[start_idx][1] - cy).atan2(ipts[start_idx][0] - cx);
        let a_end = (ipts[end_idx][1] - cy).atan2(ipts[end_idx][0] - cx);
        let sweep = normalize_angle(a_end - a_start);
        let mut verts = Vec::new();
        for k in 1..n_arc_chords {
            let t = k as f64 / n_arc_chords as f64;
            let a = a_start + sweep * t;
            verts.push([cx + r * a.cos(), cy + r * a.sin()]);
        }
        verts
    };

    // Find starting corner outside the cylinder
    let start_corner = (0..4).find(|&i| !corner_inside_cyl[i]);
    let Some(start_c) = start_corner else {
        // All corners inside cylinder — box fully enclosed in cylinder XY,
        // union = cylinder.
        return build_cyl_result(cyl, id_alloc);
    };

    // Walk box boundary CCW, substituting exposed arcs where the cylinder protrudes.
    let mut boundary_2d: Vec<[f64; 2]> = Vec::new();
    // Track which boundary segments are arc chords (indices into boundary_2d)
    let mut arc_segments: Vec<(usize, usize)> = Vec::new(); // (start_boundary_idx, end_boundary_idx)

    // Set of ipt indices already consumed by an arc walk
    let mut consumed: Vec<bool> = vec![false; n];

    for side_offset in 0..4 {
        let side = (start_c + side_offset) % 4;

        // Add start corner if outside cylinder
        if !corner_inside_cyl[side] {
            boundary_2d.push(corners[side]);
        }

        // Process intersection points on this side in order
        for &ipt_idx in &side_ipts[side] {
            if consumed[ipt_idx] {
                continue;
            }

            if is_exit_pt[ipt_idx] {
                // Add exit point, then follow exposed arc(s) to entry point
                boundary_2d.push(ipts[ipt_idx]);
                let arc_start_bi = boundary_2d.len();

                let mut cur = ipt_idx;
                consumed[cur] = true;
                loop {
                    let next = (cur + 1) % n;
                    if !is_exposed[cur] {
                        break;
                    }
                    // Add chord vertices for arc from cur to next
                    let chords = arc_chords_2d(cur, next);
                    boundary_2d.extend_from_slice(&chords);
                    consumed[next] = true;
                    cur = next;
                }
                // `cur` is now the entry point; add it
                boundary_2d.push(ipts[cur]);

                let arc_end_bi = boundary_2d.len() - 1;
                arc_segments.push((arc_start_bi, arc_end_bi));
            } else if !is_entry_pt[ipt_idx] {
                // Neither exit nor entry — add as boundary point
                boundary_2d.push(ipts[ipt_idx]);
                consumed[ipt_idx] = true;
            }
            // Entry points are added by the arc walk from the exit side
        }
    }

    // Deduplicate consecutive near-identical points
    boundary_2d.dedup_by(|a, b| ((a[0] - b[0]).powi(2) + (a[1] - b[1]).powi(2)).sqrt() < tau);
    // Check wrap-around
    if boundary_2d.len() > 2 {
        let first = boundary_2d[0];
        let last = boundary_2d[boundary_2d.len() - 1];
        if ((first[0] - last[0]).powi(2) + (first[1] - last[1]).powi(2)).sqrt() < tau {
            boundary_2d.pop();
        }
    }

    if boundary_2d.len() < 3 {
        return Err(KernelError::BooleanFailed {
            reason: "box-cylinder partial union: degenerate boundary".into(),
        });
    }

    // ── Build face polygons ──
    let mut face_polys: Vec<FacePoly> = Vec::new();
    let nb = boundary_2d.len();

    // Top cap (z=z_top)
    let top_cap_verts: Vec<[f64; 3]> = boundary_2d.iter().map(|p| [p[0], p[1], z_top]).collect();
    face_polys.push(FacePoly {
        verts: top_cap_verts,
        normal: [0.0, 0.0, 1.0],
        origin: [mn[0], mn[1], z_top],
        surface_geom: None,
    });

    // Bottom cap (z=z_bot) — reversed winding
    let bot_cap_verts: Vec<[f64; 3]> = boundary_2d
        .iter()
        .rev()
        .map(|p| [p[0], p[1], z_bot])
        .collect();
    face_polys.push(FacePoly {
        verts: bot_cap_verts,
        normal: [0.0, 0.0, -1.0],
        origin: [mn[0], mn[1], z_bot],
        surface_geom: None,
    });

    // Side faces: each consecutive pair of boundary_2d points forms a vertical quad
    for i in 0..nb {
        let j = (i + 1) % nb;
        let p0 = boundary_2d[i];
        let p1 = boundary_2d[j];

        // Edge direction in XY
        let dx = p1[0] - p0[0];
        let dy = p1[1] - p0[1];
        let len = (dx * dx + dy * dy).sqrt();
        if len < tau {
            continue;
        }

        // Outward normal: rotate edge direction 90deg CW (for CCW boundary).
        // The boundary goes p0→p1 CCW, so outward is to the right: (dy, -dx).
        let nx = dy / len;
        let ny = -dx / len;
        // Note: the quad winding is reversed (p1→p0 on top) to properly twin with
        // cap faces, but the normal still points outward.

        // Quad: CCW from outside. Top edge must go p1→p0 (opposite to cap direction
        // which goes p0→p1) so the shared half-edges are proper twins.
        let verts = vec![
            [p1[0], p1[1], z_bot],
            [p1[0], p1[1], z_top],
            [p0[0], p0[1], z_top],
            [p0[0], p0[1], z_bot],
        ];

        face_polys.push(FacePoly {
            verts,
            normal: [nx, ny, 0.0],
            origin: [p0[0], p0[1], z_bot],
            surface_geom: None,
        });
    }

    // Build B-Rep from face polygons
    let tau_weld = TAU_MODEL;
    let mut result = build_brep_from_polygons(&face_polys, tau_weld, id_alloc)?;

    // Post-process: tag cylinder chord-quad faces with Cylindrical surface geometry.
    // This causes the tessellator to compute smooth cylindrical normals (radial outward)
    // instead of flat polygon normals, fixing the segmented/transparent appearance.
    // The `build_brep_from_polygons` stitch path does NOT call `reconstruct_edge_geometry`,
    // so edges stay Linear and the tessellator uses its `!has_curved_edges` fan path.
    {
        let cyl_geom = SurfaceGeom::Cylindrical(Cylinder {
            origin: Point3::from_array([cx, cy, z_bot]),
            axis: Vector3::new(0.0, 0.0, 1.0),
            radius: r,
        });
        // Find faces whose vertices all lie on the cylinder (within tolerance).
        // These are the chord-quad faces that should have cylindrical normals.
        let face_indices: Vec<FaceIdx> = result.face_geometry.keys().copied().collect::<Vec<_>>();
        for face_idx in face_indices {
            // Only re-tag faces currently marked as Planar with horizontal (non-cap) normals
            if let Some(SurfaceGeom::Planar(plane)) = result.face_geometry.get(&face_idx) {
                if plane.normal.z.abs() > 0.1 {
                    continue; // cap face — keep planar
                }
            } else {
                continue;
            }
            // Check: are ALL vertices of this face on the cylinder circle?
            let outer_loop = result.arena.faces[face_idx.0].outer_loop;
            let start_he = result.arena.loops[outer_loop.0].half_edge;
            let mut he = start_he;
            let mut all_on_cyl = true;
            let mut count = 0;
            loop {
                let vi = result.arena.half_edges[he.0].origin;
                let pos = result.arena.vertices[vi.0].position;
                let dx = pos[0] - cx;
                let dy = pos[1] - cy;
                let dist = (dx * dx + dy * dy).sqrt();
                if (dist - r).abs() > r * 0.02 {
                    all_on_cyl = false;
                    break;
                }
                count += 1;
                he = result.arena.half_edges[he.0].next;
                if he == start_he || count > 100 {
                    break;
                }
            }
            if all_on_cyl && count >= 3 {
                result.face_geometry.insert(face_idx, cyl_geom.clone());
            }
        }
    }

    Ok(result)
}

// ── Planar-planar boolean (A15 compliance) ──────────────────────────────

/// Exact boolean for all-planar solid pairs.
///
/// Both operands must have ONLY `SurfaceGeom::Planar` faces. This function
/// replaces the polygon-clipping fallback for these cases, fixing:
/// - The inward-offset sampling bug in `classify_face_nonconvex` (line 510)
///   that corrupts topology for chained booleans on oblique solids
/// - The self-twin boundary construction in the AABB-disjoint fast path
///
/// For non-convex opposing solids, fragment classification uses
/// `point_in_solid(centroid)` WITHOUT the inward offset. For all-planar
/// solids, fragment centroids from plane-plane splitting lie in the face
/// interior (not on a curved surface), so no offset is needed.
///
/// Ref: A15.1 (quadric → exact SSI), A15.5 (surface type preservation).
pub(crate) fn planar_planar_boolean(
    solid_a: &WaffleSolid,
    solid_b: &WaffleSolid,
    op: BoolOp,
    id_alloc: &mut dyn FnMut() -> u64,
) -> Result<BooleanResult, KernelError> {
    // Step 1: Extract face polygons
    let a_faces = extract_face_polys_general(solid_a);
    let b_faces = extract_face_polys_general(solid_b);

    if a_faces.is_empty() || b_faces.is_empty() {
        return Err(KernelError::BooleanFailed {
            reason: "planar_planar_boolean: empty face set".into(),
        });
    }

    // Step 2: Compute adaptive tolerances
    let (tau, tau_weld) = compute_adaptive_tau_weld(&a_faces, &b_faces);

    // Step 3: AABB disjoint fast-path
    let compute_aabb = |faces: &[FacePoly]| -> ([f64; 3], [f64; 3]) {
        let mut mn = [f64::INFINITY; 3];
        let mut mx = [f64::NEG_INFINITY; 3];
        for f in faces {
            for v in &f.verts {
                for j in 0..3 {
                    mn[j] = mn[j].min(v[j]);
                    mx[j] = mx[j].max(v[j]);
                }
            }
        }
        (mn, mx)
    };
    let (a_min, a_max) = compute_aabb(&a_faces);
    let (b_min, b_max) = compute_aabb(&b_faces);

    let aabb_disjoint = (0..3).any(|i| a_max[i] + tau < b_min[i] || b_max[i] + tau < a_min[i]);

    if aabb_disjoint {
        if matches!(op, BoolOp::Union) {
            return Err(KernelError::BooleanFailed {
                reason: "operands are disjoint (bounding boxes do not overlap)".into(),
            });
        }
        let result_faces: Vec<FacePoly> = match op {
            BoolOp::Union => unreachable!(),
            BoolOp::Subtract => a_faces,
            BoolOp::Intersect => vec![],
        };

        if result_faces.is_empty() {
            let mut arena = TopoArena::new();
            let solid_idx = arena.add_solid();
            let shell_idx = arena.add_shell(solid_idx);
            arena.solids[solid_idx.0].outer_shell = shell_idx;
            return Ok(BooleanResult {
                arena,
                face_map: BTreeMap::new(),
                edge_map: BTreeMap::new(),
                vertex_map: BTreeMap::new(),
                face_geometry: BTreeMap::new(),
                edge_geometry: BTreeMap::new(),
                cached_face_polys: None,
            });
        }

        // Build proper B-Rep (not self-twin boundary hack)
        let cached = result_faces.clone();
        let mut result = build_brep_from_polygons_inner(&result_faces, tau_weld, false, id_alloc)?;
        result.cached_face_polys = Some(cached);
        return Ok(result);
    }

    // Step 4: Guard against pathological face counts
    let total_faces = a_faces.len() + b_faces.len();
    if total_faces > 8000 {
        return Err(KernelError::NotSupported {
            operation: format!(
                "planar_planar_boolean: {} total faces exceeds limit (8000)",
                total_faces
            ),
        });
    }

    // Step 5: Convexity check
    let a_convex = is_face_set_convex(&a_faces, tau);
    let b_convex = is_face_set_convex(&b_faces, tau);

    // Step 6: Per-face AABB early-out helper
    let face_outside_aabb = |face: &FacePoly, aabb_min: &[f64; 3], aabb_max: &[f64; 3]| -> bool {
        for axis in 0..3 {
            if face.verts.iter().all(|v| v[axis] < aabb_min[axis] - tau) {
                return true;
            }
            if face.verts.iter().all(|v| v[axis] > aabb_max[axis] + tau) {
                return true;
            }
        }
        false
    };

    // Step 7: Classify faces
    // Intersection cache for consistent edge-plane intersection points.
    let mut cache: Option<IntersectionCache> = Some(IntersectionCache::new(tau));

    let mut a_classified: Vec<(FacePoly, FaceClass)> = Vec::with_capacity(a_faces.len());
    for f in &a_faces {
        if face_outside_aabb(f, &b_min, &b_max) {
            a_classified.push((f.clone(), FaceClass::Outside));
        } else {
            let class = if b_convex {
                classify_face(f, &b_faces, tau, &mut cache)
            } else {
                classify_face_nonconvex_planar(f, &b_faces, tau, &mut cache)
            };
            a_classified.push((f.clone(), class));
        }
    }

    let mut b_classified: Vec<(FacePoly, FaceClass)> = Vec::with_capacity(b_faces.len());
    for f in &b_faces {
        if face_outside_aabb(f, &a_min, &a_max) {
            b_classified.push((f.clone(), FaceClass::Outside));
        } else {
            let class = if a_convex {
                classify_face(f, &a_faces, tau, &mut cache)
            } else {
                classify_face_nonconvex_planar(f, &a_faces, tau, &mut cache)
            };
            b_classified.push((f.clone(), class));
        }
    }

    // Step 8: Collect result fragments
    let mut result_polys = Vec::new();
    match op {
        BoolOp::Union => {
            collect_union_fragments(&a_classified, &mut result_polys, true);
            collect_union_fragments(&b_classified, &mut result_polys, false);
        }
        BoolOp::Subtract => {
            collect_fragments(&a_classified, &mut result_polys, false, true, false, false);
            collect_fragments(&b_classified, &mut result_polys, true, false, true, false);
        }
        BoolOp::Intersect => {
            collect_fragments(&a_classified, &mut result_polys, false, false, true, true);
            collect_fragments(&b_classified, &mut result_polys, false, false, true, false);
        }
    }

    if result_polys.is_empty() {
        let mut arena = TopoArena::new();
        let solid_idx = arena.add_solid();
        let shell_idx = arena.add_shell(solid_idx);
        arena.solids[solid_idx.0].outer_shell = shell_idx;
        return Ok(BooleanResult {
            arena,
            face_map: BTreeMap::new(),
            edge_map: BTreeMap::new(),
            vertex_map: BTreeMap::new(),
            face_geometry: BTreeMap::new(),
            edge_geometry: BTreeMap::new(),
            cached_face_polys: None,
        });
    }

    // Step 9: Post-processing — ensure all faces are planar (A15.5)
    let result_polys: Vec<FacePoly> = result_polys
        .into_iter()
        .map(|mut fp| {
            if fp.surface_geom.is_none() {
                fp.surface_geom = Some(SurfaceGeom::Planar(Plane {
                    origin: Point3::from_array(fp.origin),
                    normal: Vector3::from_array(fp.normal),
                }));
            }
            fp
        })
        .collect();

    let result_polys = dedup_face_polys(&result_polys, tau_weld);
    let result_polys = merge_nearby_vertices(&result_polys, tau_weld);
    let result_polys = resolve_t_junctions(&result_polys, tau_weld);

    // Step 10: Build B-Rep with strict manifold requirement
    let cached = result_polys.clone();
    let mut result = build_brep_from_polygons_inner(&result_polys, tau_weld, false, id_alloc)?;
    result.cached_face_polys = Some(cached);
    Ok(result)
}

/// Classify a face against a non-convex opposing solid using progressive
/// splitting — planar-specific variant WITHOUT the inward offset.
///
/// For all-planar solids, fragment centroids from plane-plane splitting lie
/// exactly in the face interior, not on a curved surface. The inward offset
/// (`face.normal * -tau * 100`) used in the general `classify_face_nonconvex`
/// can push the sample point through a thin slab, causing mis-classification.
/// This variant uses the raw centroid for `point_in_solid`.
fn classify_face_nonconvex_planar(
    face: &FacePoly,
    opposing: &[FacePoly],
    tau: f64,
    cache: &mut Option<IntersectionCache>,
) -> FaceClass {
    use crate::units::TAU_NORMALIZE;

    let original_area = polygon_area_3d(&face.verts);
    if original_area < TAU_NORMALIZE {
        return FaceClass::Outside;
    }

    // Check coplanar partnerships
    let has_antiparallel = opposing.iter().any(|opp| {
        classify_coplanarity(face.normal, face.verts[0], opp, tau) == CoplanarClass::AntiParallel
    });
    if has_antiparallel {
        // Delegate to the general antiparallel handler — it's correct for
        // planar solids (uses progressive splitting + point_in_solid).
        // The inward offset in the antiparallel path is acceptable because
        // the face IS on the boundary (shared surface), so the offset
        // correctly pushes into the solid volume.
        return classify_face_nonconvex(face, opposing, tau, cache);
    }

    let has_coplanar = opposing
        .iter()
        .any(|opp| is_coplanar(face.normal, face.verts[0], opp, tau));

    if has_coplanar {
        // Coplanar same-direction: delegate to general handler (offset is fine
        // for coplanar surface classification).
        return classify_face_nonconvex(face, opposing, tau, cache);
    }

    // ── Non-coplanar path: progressive splitting WITHOUT inward offset ───

    let mut cutting_planes: Vec<([f64; 3], [f64; 3])> = Vec::new();

    for opp in opposing {
        if is_coplanar(face.normal, face.verts[0], opp, tau) {
            continue;
        }

        // Straddle check: face must have vertices on both sides of the plane
        let mut has_pos = false;
        let mut has_neg = false;
        for v in &face.verts {
            let d = v3_dot(v3_sub(*v, opp.origin), opp.normal);
            if d > tau {
                has_pos = true;
            }
            if d < -tau {
                has_neg = true;
            }
        }
        if has_pos && has_neg {
            cutting_planes.push((opp.origin, v3_negate(opp.normal)));
        }
    }

    if cutting_planes.is_empty() {
        // No planes straddle — classify centroid directly (no inward offset)
        let centroid = polygon_centroid(&face.verts);
        if point_in_solid(centroid, opposing) {
            return FaceClass::Inside;
        }
        return FaceClass::Outside;
    }

    // Progressive splitting
    const MAX_FRAGMENTS: usize = 2048;
    let mut fragments: Vec<Vec<[f64; 3]>> = vec![face.verts.clone()];

    for (plane_pt, inward_n) in &cutting_planes {
        if fragments.len() >= MAX_FRAGMENTS {
            break;
        }
        let outward_n = v3_negate(*inward_n);
        let mut new_fragments = Vec::new();
        for frag in &fragments {
            let half_in =
                clip_polygon_by_plane_cached(frag, *plane_pt, *inward_n, tau, cache.as_mut());
            let half_out =
                clip_polygon_by_plane_cached(frag, *plane_pt, outward_n, tau, cache.as_mut());
            if half_in.len() >= 3 && polygon_area_3d(&half_in) > TAU_NORMALIZE {
                new_fragments.push(half_in);
            }
            if half_out.len() >= 3 && polygon_area_3d(&half_out) > TAU_NORMALIZE {
                new_fragments.push(half_out);
            }
        }
        fragments = new_fragments;
    }

    // Classify each fragment with point_in_solid using raw centroid (NO offset)
    let mut inside_frags: Vec<Vec<[f64; 3]>> = Vec::new();
    let mut outside_frags: Vec<Vec<[f64; 3]>> = Vec::new();

    for frag in fragments {
        let centroid = polygon_centroid(&frag);
        if point_in_solid(centroid, opposing) {
            inside_frags.push(frag);
        } else {
            outside_frags.push(frag);
        }
    }

    if inside_frags.is_empty() {
        return FaceClass::Outside;
    }
    if outside_frags.is_empty() {
        return FaceClass::Inside;
    }

    FaceClass::Partial {
        inside_frags,
        outside_frags,
    }
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

    // ── Cylinder-minus-enclosed-box cap-touching tests ──────────────────

    fn count_inner_loops(result: &BooleanResult) -> usize {
        result.arena.faces.iter().map(|f| f.inner_loops.len()).sum()
    }

    fn count_faces(result: &BooleanResult) -> usize {
        result.arena.faces.len()
    }

    fn count_edges(result: &BooleanResult) -> usize {
        result.arena.edges.len()
    }

    fn count_vertices(result: &BooleanResult) -> usize {
        result.arena.vertices.len()
    }

    #[test]
    fn test_cyl_minus_enclosed_box_through_hole() {
        // Box Z matches cylinder Z exactly → through-hole (2 inner loops on caps)
        let cyl = CylinderParams {
            center_bottom: [0.0, 0.0, 0.0],
            radius: 0.5,
            x_axis: [1.0, 0.0, 0.0],
            y_axis: [0.0, 1.0, 0.0],
            direction: [0.0, 0.0, 1.0],
            depth: 1.0,
        };
        let aabb = Aabb {
            min: [-0.2, -0.2, 0.0],
            max: [0.2, 0.2, 1.0],
        };
        let mut id = 1000u64;
        let result = build_cyl_minus_enclosed_box(&aabb, &cyl, &mut || {
            id += 1;
            id
        })
        .unwrap();

        let v = count_vertices(&result);
        let e = count_edges(&result);
        let f = count_faces(&result);
        let chi = v as i64 - e as i64 + f as i64;
        assert_eq!(chi, 2, "through-hole: V={v}, E={e}, F={f}, chi={chi}");
        assert_eq!(
            count_inner_loops(&result),
            2,
            "through-hole should have 2 inner loops (one per cap)"
        );
        assert_eq!(f, 7, "through-hole: expected 7 faces, got {f}");
    }

    #[test]
    fn test_cyl_minus_enclosed_box_blind_pocket() {
        // Box Z strictly inside cylinder Z → blind pocket (no inner loops, floor+ceiling)
        let cyl = CylinderParams {
            center_bottom: [0.0, 0.0, 0.0],
            radius: 0.5,
            x_axis: [1.0, 0.0, 0.0],
            y_axis: [0.0, 1.0, 0.0],
            direction: [0.0, 0.0, 1.0],
            depth: 1.0,
        };
        let aabb = Aabb {
            min: [-0.2, -0.2, 0.2],
            max: [0.2, 0.2, 0.8],
        };
        let mut id = 1000u64;
        let result = build_cyl_minus_enclosed_box(&aabb, &cyl, &mut || {
            id += 1;
            id
        })
        .unwrap();

        let v = count_vertices(&result);
        let e = count_edges(&result);
        let f = count_faces(&result);
        let chi = v as i64 - e as i64 + f as i64;
        assert_eq!(chi, 4, "blind pocket: V={v}, E={e}, F={f}, chi={chi}");
        assert_eq!(
            count_inner_loops(&result),
            0,
            "blind pocket should have 0 inner loops"
        );
        assert_eq!(f, 9, "blind pocket: expected 9 faces, got {f}");

        // Verify box vertices use actual box Z, not cylinder Z
        let has_z_02 = result
            .arena
            .vertices
            .iter()
            .any(|v| (v.position[2] - 0.2).abs() < 1e-10);
        let has_z_08 = result
            .arena
            .vertices
            .iter()
            .any(|v| (v.position[2] - 0.8).abs() < 1e-10);
        assert!(has_z_02, "should have vertices at z=0.2 (box bottom)");
        assert!(has_z_08, "should have vertices at z=0.8 (box top)");
    }

    #[test]
    fn test_cyl_minus_enclosed_box_top_only() {
        // Box touches top cap only → 1 inner loop on top, floor face at bottom
        let cyl = CylinderParams {
            center_bottom: [0.0, 0.0, 0.0],
            radius: 0.5,
            x_axis: [1.0, 0.0, 0.0],
            y_axis: [0.0, 1.0, 0.0],
            direction: [0.0, 0.0, 1.0],
            depth: 1.0,
        };
        let aabb = Aabb {
            min: [-0.2, -0.2, 0.3],
            max: [0.2, 0.2, 1.0],
        };
        let mut id = 1000u64;
        let result = build_cyl_minus_enclosed_box(&aabb, &cyl, &mut || {
            id += 1;
            id
        })
        .unwrap();

        let v = count_vertices(&result);
        let e = count_edges(&result);
        let f = count_faces(&result);
        let chi = v as i64 - e as i64 + f as i64;
        assert_eq!(chi, 3, "top-only: V={v}, E={e}, F={f}, chi={chi}");
        assert_eq!(
            count_inner_loops(&result),
            1,
            "top-only should have 1 inner loop (on top cap)"
        );
        assert_eq!(f, 8, "top-only: expected 8 faces, got {f}");
    }

    #[test]
    fn test_cyl_minus_enclosed_box_bottom_only() {
        // Box touches bottom cap only → 1 inner loop on bottom, ceiling face at top
        let cyl = CylinderParams {
            center_bottom: [0.0, 0.0, 0.0],
            radius: 0.5,
            x_axis: [1.0, 0.0, 0.0],
            y_axis: [0.0, 1.0, 0.0],
            direction: [0.0, 0.0, 1.0],
            depth: 1.0,
        };
        let aabb = Aabb {
            min: [-0.2, -0.2, 0.0],
            max: [0.2, 0.2, 0.7],
        };
        let mut id = 1000u64;
        let result = build_cyl_minus_enclosed_box(&aabb, &cyl, &mut || {
            id += 1;
            id
        })
        .unwrap();

        let v = count_vertices(&result);
        let e = count_edges(&result);
        let f = count_faces(&result);
        let chi = v as i64 - e as i64 + f as i64;
        assert_eq!(chi, 3, "bottom-only: V={v}, E={e}, F={f}, chi={chi}");
        assert_eq!(
            count_inner_loops(&result),
            1,
            "bottom-only should have 1 inner loop (on bottom cap)"
        );
        assert_eq!(f, 8, "bottom-only: expected 8 faces, got {f}");
    }

    /// Test non-concentric enclosed cylinder subtract (through-hole).
    ///
    /// Big cylinder R=1.0 centered at origin, small cylinder r=0.2 centered at (0.3, 0.0).
    /// Inner fully penetrates outer. Result should be a disc with an off-center hole.
    #[test]
    fn enclosed_cyl_subtract_through_hole() {
        let outer = CylinderParams {
            center_bottom: [0.0, 0.0, 0.0],
            radius: 1.0,
            depth: 0.5,
            direction: [0.0, 0.0, 1.0],
            x_axis: [1.0, 0.0, 0.0],
            y_axis: [0.0, 1.0, 0.0],
        };
        let inner = CylinderParams {
            center_bottom: [0.3, 0.0, -0.1], // offset, extends below
            radius: 0.2,
            depth: 0.8, // extends above: -0.1 to 0.7 covers [0, 0.5]
            direction: [0.0, 0.0, 1.0],
            x_axis: [1.0, 0.0, 0.0],
            y_axis: [0.0, 1.0, 0.0],
        };
        let mut next_id = 1u64;
        let mut id_alloc = || {
            let id = next_id;
            next_id += 1;
            id
        };

        let result = cyl_cyl_boolean(&outer, &inner, BoolOp::Subtract, &mut id_alloc);
        assert!(
            result.is_ok(),
            "enclosed through-hole should succeed: {:?}",
            result.err()
        );
        let result = result.unwrap();

        let f = count_faces(&result);
        // Through-hole: 32 outer quads + 32 inner quads + 64 top tris + 64 bottom tris = 192
        assert!(f > 100, "through-hole should produce many faces, got {f}");
    }

    /// Test non-concentric enclosed cylinder subtract (blind hole).
    ///
    /// Big cylinder R=1.0, small cylinder r=0.15 at (0.4, 0.2).
    /// Inner only goes halfway through — produces blind pocket.
    #[test]
    fn enclosed_cyl_subtract_blind_hole() {
        let outer = CylinderParams {
            center_bottom: [0.0, 0.0, 0.0],
            radius: 1.0,
            depth: 1.0,
            direction: [0.0, 0.0, 1.0],
            x_axis: [1.0, 0.0, 0.0],
            y_axis: [0.0, 1.0, 0.0],
        };
        let inner = CylinderParams {
            center_bottom: [0.4, 0.2, 0.0],
            radius: 0.15,
            depth: 0.5, // only goes halfway
            direction: [0.0, 0.0, 1.0],
            x_axis: [1.0, 0.0, 0.0],
            y_axis: [0.0, 1.0, 0.0],
        };
        let mut next_id = 1u64;
        let mut id_alloc = || {
            let id = next_id;
            next_id += 1;
            id
        };

        let result = cyl_cyl_boolean(&outer, &inner, BoolOp::Subtract, &mut id_alloc);
        assert!(
            result.is_ok(),
            "enclosed blind-hole should succeed: {:?}",
            result.err()
        );
        let result = result.unwrap();

        let f = count_faces(&result);
        // Blind hole: 32 outer quads + 32 inner quads + 64 top tris + 1 bottom cap + 1 inner cap = 130
        assert!(f > 60, "blind-hole should produce many faces, got {f}");
    }
}
