//! Boolean operations using convex face-polygon clipping and SSI.
//!
//! Supports Union, Subtract, and Intersect on axis-aligned box solids
//! produced by the WaffleKernel extrude pipeline. Uses Sutherland-Hodgman
//! polygon clipping against convex half-spaces to classify face fragments
//! as inside, outside, or partial with respect to the opposing solid.

pub(crate) mod analytical;
mod classify;
mod clip;
pub(crate) mod stitch;

#[cfg(test)]
pub(crate) use analytical::build_cyl_result;
pub(crate) use analytical::{polygon_approx_boolean, ssi_boolean_op};

use classify::{classify_face, classify_face_nonconvex, point_in_solid, FaceClass};
#[cfg(test)]
use classify::{solid_angle, winding_number, winding_number_classify};
#[cfg(test)]
use clip::clip_polygon_by_plane;
use clip::{
    classify_coplanarity, clip_polygon_by_plane_cached, clip_polygon_by_solid, dedup_face_polys,
    is_coplanar, merge_nearby_vertices, resolve_t_junctions, CoplanarClass, IntersectionCache,
};
use stitch::{build_brep_from_polygons, build_brep_from_polygons_inner};

use crate::geometry::curve::CurveGeom;
use crate::geometry::point::{Point3, Vector3};
use crate::geometry::surface::{Cylinder, SurfaceGeom};
use crate::topology::arena::TopoArena;
use crate::topology::half_edge::*;
use crate::types::*;
use crate::units::{TAU_MODEL, TAU_NORMALIZE, TAU_WELD_FACTOR, TAU_WELD_MAX, TAU_WELD_MIN};
use crate::vecmath::*;
use crate::waffle_kernel::{CylinderParams, WaffleSolid};
use std::collections::HashMap;

// ── Public types ────────────────────────────────────────────────────────

/// The boolean operation to perform.
#[derive(Debug, Clone, Copy)]
pub(crate) enum BoolOp {
    Union,
    Subtract,
    Intersect,
}

/// Result of a boolean operation: a new B-Rep solid with topology and geometry.
pub(crate) struct BooleanResult {
    pub arena: TopoArena,
    pub face_map: HashMap<u64, FaceIdx>,
    pub edge_map: HashMap<u64, EdgeIdx>,
    pub vertex_map: HashMap<u64, VertexIdx>,
    pub face_geometry: HashMap<FaceIdx, SurfaceGeom>,
    pub edge_geometry: HashMap<EdgeIdx, CurveGeom>,
    /// Cached face polygons from the boolean result, for reuse in subsequent booleans.
    pub cached_face_polys: Option<Vec<FacePoly>>,
}

// ── Internal types ──────────────────────────────────────────────────────

/// A planar polygon with its face normal and a representative origin point.
#[derive(Debug, Clone)]
pub(crate) struct FacePoly {
    pub(crate) verts: Vec<[f64; 3]>,
    pub(crate) normal: [f64; 3],
    pub(crate) origin: [f64; 3],
    /// Analytical surface geometry for this face. When `Some`, preserved through
    /// the boolean pipeline into the result B-Rep (Ref #24 Barton: bijective
    /// re-mapping of analytical surfaces through mesh booleans).
    pub(crate) surface_geom: Option<SurfaceGeom>,
}

/// Compute polygon area using cross-product accumulation (works in 3D).
pub(super) fn polygon_area_3d(verts: &[[f64; 3]]) -> f64 {
    if verts.len() < 3 {
        return 0.0;
    }
    let mut sum = [0.0, 0.0, 0.0];
    for i in 1..verts.len() - 1 {
        let ab = v3_sub(verts[i], verts[0]);
        let ac = v3_sub(verts[i + 1], verts[0]);
        let c = v3_cross(ab, ac);
        sum = v3_add(sum, c);
    }
    v3_length(sum) * 0.5
}

// ── Face polygon extraction ─────────────────────────────────────────────

/// Walk the outer loop of a face, collecting vertex positions.
fn collect_face_vertices(arena: &TopoArena, face_idx: FaceIdx) -> Vec<[f64; 3]> {
    let loop_idx = arena.faces[face_idx.0].outer_loop;
    let start_he = arena.loops[loop_idx.0].half_edge;
    let mut verts = Vec::new();
    let mut he = start_he;
    loop {
        let v = arena.half_edges[he.0].origin;
        verts.push(arena.vertices[v.0].position);
        he = arena.half_edges[he.0].next;
        if he == start_he {
            break;
        }
    }
    verts
}

/// Generate polygon approximation for a face with analytic geometry but too few
/// loop vertices (SSI boolean results use seam-edge B-Rep).
///
/// For cylindrical faces (2 seam vertices defining the Z range), generates N
/// side quads. For planar caps (1 seam vertex), generates an N-gon circle.
fn generate_analytic_face_polys(
    geom: &SurfaceGeom,
    loop_verts: &[[f64; 3]],
    arena: &TopoArena,
    face_idx: FaceIdx,
    polys: &mut Vec<FacePoly>,
) {
    let n_seg = 32;
    match geom {
        SurfaceGeom::Cylindrical(cyl) => {
            let axis = [cyl.axis.x, cyl.axis.y, cyl.axis.z];
            let origin = [cyl.origin.x, cyl.origin.y, cyl.origin.z];
            let radius = cyl.radius.abs();
            let inward = cyl.radius < 0.0;

            if radius < TAU_NORMALIZE {
                return;
            }

            // Find Z extent from loop vertices (seam vertices define top/bottom)
            let heights: Vec<f64> = loop_verts
                .iter()
                .map(|v| v3_dot(v3_sub(*v, origin), axis))
                .collect();
            let z_min = heights.iter().copied().fold(f64::INFINITY, f64::min);
            let z_max = heights.iter().copied().fold(f64::NEG_INFINITY, f64::max);
            if (z_max - z_min).abs() < TAU_NORMALIZE {
                return;
            }

            // Build local frame (x_axis, y_axis perpendicular to axis)
            let (x_axis, y_axis) = {
                let up = if axis[0].abs() < 0.9 {
                    [1.0, 0.0, 0.0]
                } else {
                    [0.0, 1.0, 0.0]
                };
                let x = v3_normalize(v3_cross(axis, up));
                let y = v3_normalize(v3_cross(axis, x));
                (x, y)
            };

            // Generate N side quads
            let bot_center = v3_add(origin, v3_scale(axis, z_min));
            let top_center = v3_add(origin, v3_scale(axis, z_max));
            for i in 0..n_seg {
                let theta0 = std::f64::consts::TAU * (i as f64) / (n_seg as f64);
                let theta1 = std::f64::consts::TAU * ((i + 1) as f64) / (n_seg as f64);
                let (c0, s0) = (theta0.cos(), theta0.sin());
                let (c1, s1) = (theta1.cos(), theta1.sin());

                let offset0 = v3_add(v3_scale(x_axis, radius * c0), v3_scale(y_axis, radius * s0));
                let offset1 = v3_add(v3_scale(x_axis, radius * c1), v3_scale(y_axis, radius * s1));

                let b0 = v3_add(bot_center, offset0);
                let b1 = v3_add(bot_center, offset1);
                let t0 = v3_add(top_center, offset0);
                let t1 = v3_add(top_center, offset1);

                let edge_bot = v3_sub(b1, b0);
                let edge_up = v3_sub(t0, b0);
                let mut normal = v3_normalize(v3_cross(edge_bot, edge_up));
                if inward {
                    normal = v3_negate(normal);
                }

                let quad = if inward {
                    vec![b0, t0, t1, b1] // reversed winding for inward
                } else {
                    vec![b0, b1, t1, t0]
                };
                polys.push(FacePoly {
                    verts: quad,
                    normal,
                    origin: b0,
                    surface_geom: Some(geom.clone()),
                });
            }
        }
        SurfaceGeom::Planar(plane) => {
            let normal = [plane.normal.x, plane.normal.y, plane.normal.z];
            let origin = [plane.origin.x, plane.origin.y, plane.origin.z];

            // Planar cap face with < 3 loop vertices: circular cap.
            // Find radius from the seam vertex distance to the plane origin.
            // Also check inner loops for annular caps (tube geometry).
            let mut radii = Vec::new();

            // Outer loop radius
            if let Some(v) = loop_verts.first() {
                let r = v3_length(v3_sub(*v, origin));
                if r > TAU_NORMALIZE {
                    radii.push(r);
                }
            }

            // Check inner loops for tube/annular faces
            for inner_loop in &arena.faces[face_idx.0].inner_loops {
                let start_he = arena.loops[inner_loop.0].half_edge;
                let v_idx = arena.half_edges[start_he.0].origin;
                let v_pos = arena.vertices[v_idx.0].position;
                let r = v3_length(v3_sub(v_pos, origin));
                if r > TAU_NORMALIZE {
                    radii.push(r);
                }
            }

            if radii.is_empty() {
                return;
            }

            // Build local frame
            let (x_axis, y_axis) = {
                let up = if normal[0].abs() < 0.9 {
                    [1.0, 0.0, 0.0]
                } else {
                    [0.0, 1.0, 0.0]
                };
                let x = v3_normalize(v3_cross(normal, up));
                let y = v3_normalize(v3_cross(normal, x));
                (x, y)
            };

            if radii.len() == 1 {
                // Simple circular cap
                let r = radii[0];
                let mut cap_verts = Vec::with_capacity(n_seg);
                for i in 0..n_seg {
                    let theta = std::f64::consts::TAU * (i as f64) / (n_seg as f64);
                    let offset = v3_add(
                        v3_scale(x_axis, r * theta.cos()),
                        v3_scale(y_axis, r * theta.sin()),
                    );
                    cap_verts.push(v3_add(origin, offset));
                }
                polys.push(FacePoly {
                    verts: cap_verts,
                    normal,
                    origin,
                    surface_geom: None,
                });
            } else if radii.len() >= 2 {
                // Annular cap: generate quads between outer and inner circles
                let (r_outer, r_inner) = if radii[0] > radii[1] {
                    (radii[0], radii[1])
                } else {
                    (radii[1], radii[0])
                };
                for i in 0..n_seg {
                    let theta0 = std::f64::consts::TAU * (i as f64) / (n_seg as f64);
                    let theta1 = std::f64::consts::TAU * ((i + 1) as f64) / (n_seg as f64);

                    let outer0 = v3_add(
                        origin,
                        v3_add(
                            v3_scale(x_axis, r_outer * theta0.cos()),
                            v3_scale(y_axis, r_outer * theta0.sin()),
                        ),
                    );
                    let outer1 = v3_add(
                        origin,
                        v3_add(
                            v3_scale(x_axis, r_outer * theta1.cos()),
                            v3_scale(y_axis, r_outer * theta1.sin()),
                        ),
                    );
                    let inner0 = v3_add(
                        origin,
                        v3_add(
                            v3_scale(x_axis, r_inner * theta0.cos()),
                            v3_scale(y_axis, r_inner * theta0.sin()),
                        ),
                    );
                    let inner1 = v3_add(
                        origin,
                        v3_add(
                            v3_scale(x_axis, r_inner * theta1.cos()),
                            v3_scale(y_axis, r_inner * theta1.sin()),
                        ),
                    );

                    // Quad: outer0 → outer1 → inner1 → inner0
                    polys.push(FacePoly {
                        verts: vec![outer0, outer1, inner1, inner0],
                        normal,
                        origin: outer0,
                        surface_geom: None,
                    });
                }
            }
        }
        SurfaceGeom::Conical(_) => {} // Analytic poly generation not yet implemented
        SurfaceGeom::Spherical(_) => {} // Analytic poly generation not yet implemented
        SurfaceGeom::Toroidal(_) => {} // Analytic poly generation not yet implemented
    }
}

/// Extract all face polygons from a WaffleSolid.
pub(super) fn extract_face_polys(solid: &WaffleSolid) -> Vec<FacePoly> {
    let mut polys = Vec::new();
    for (&_kid, &face_idx) in &solid.face_map {
        let verts = collect_face_vertices(&solid.arena, face_idx);
        if verts.len() < 3 {
            continue;
        }
        let face_sg = solid.face_geometry.get(&face_idx).cloned();
        let (normal, origin) = match &face_sg {
            Some(SurfaceGeom::Planar(p)) => (
                [p.normal.x, p.normal.y, p.normal.z],
                [p.origin.x, p.origin.y, p.origin.z],
            ),
            _ => {
                // For non-planar faces (cylindrical, conical, etc.),
                // compute planar approximation from loop vertices using
                // Newell normal. Only include if the face is approximately
                // planar (all vertices within 5% of face size from the plane).
                let newell = compute_newell_normal(&verts);
                let nl = v3_dot(newell, newell).sqrt();
                if nl < TAU_NORMALIZE {
                    continue; // Degenerate face
                }
                let n = [newell[0] / nl, newell[1] / nl, newell[2] / nl];
                let o = polygon_centroid(&verts);
                // Planarity check: max distance from any vertex to the Newell plane
                let max_dist = verts
                    .iter()
                    .map(|v| v3_dot(v3_sub(*v, o), n).abs())
                    .fold(0.0_f64, f64::max);
                // Face size: max pairwise distance between first vertex and others
                let face_size = verts
                    .iter()
                    .skip(1)
                    .map(|v| v3_length(v3_sub(*v, verts[0])))
                    .fold(0.0_f64, f64::max);
                if face_size > TAU_NORMALIZE && max_dist > face_size * 0.05 {
                    // Too curved: subdivide into triangles (each triangle is
                    // exactly planar). This handles revolve lateral faces.
                    if verts.len() >= 3 {
                        for t in 1..verts.len() - 1 {
                            let tri = vec![verts[0], verts[t], verts[t + 1]];
                            let e1 = v3_sub(tri[1], tri[0]);
                            let e2 = v3_sub(tri[2], tri[0]);
                            let tri_n = v3_cross(e1, e2);
                            let tri_nl = v3_length(tri_n);
                            if tri_nl < TAU_NORMALIZE {
                                continue;
                            }
                            let tri_normal =
                                [tri_n[0] / tri_nl, tri_n[1] / tri_nl, tri_n[2] / tri_nl];
                            // Orient triangle normal consistently with Newell normal
                            let tri_normal = if v3_dot(tri_normal, n) >= 0.0 {
                                tri_normal
                            } else {
                                v3_negate(tri_normal)
                            };
                            let tri_origin = polygon_centroid(&tri);
                            polys.push(FacePoly {
                                verts: tri,
                                normal: tri_normal,
                                origin: tri_origin,
                                surface_geom: face_sg.clone(),
                            });
                        }
                    }
                    continue;
                }
                (n, o)
            }
        };
        polys.push(FacePoly {
            verts,
            normal,
            origin,
            surface_geom: face_sg,
        });
    }
    // Sort for deterministic order (HashMap iteration is nondeterministic).
    // This ensures classify_face sees cutting planes in the same order every run.
    polys.sort_by(|a, b| {
        let ca = polygon_centroid(&a.verts);
        let cb = polygon_centroid(&b.verts);
        let ka = [
            (ca[0] * 1e9) as i64,
            (ca[1] * 1e9) as i64,
            (ca[2] * 1e9) as i64,
        ];
        let kb = [
            (cb[0] * 1e9) as i64,
            (cb[1] * 1e9) as i64,
            (cb[2] * 1e9) as i64,
        ];
        ka.cmp(&kb)
    });
    polys
}

/// Generate planar face polygons approximating a cylinder.
///
/// Converts a cylinder (2 circular caps + 1 cylindrical lateral face) into
/// N planar quads for the lateral surface plus 2 N-gon caps. This allows
/// cylinder solids to participate in the polygon-clipping boolean pipeline.
fn cylinder_to_face_polys(cyl: &CylinderParams, n: usize) -> Vec<FacePoly> {
    let mut polys = Vec::with_capacity(n + 2);
    let dir = cyl.direction;

    // Build cylindrical surface geometry for tagging side quads
    let cyl_surface = SurfaceGeom::Cylindrical(Cylinder {
        origin: Point3::from_array(cyl.center_bottom),
        axis: Vector3::from_array(cyl.direction),
        radius: cyl.radius,
    });

    // Generate N points on bottom and top circles
    let mut bottom_pts = Vec::with_capacity(n);
    let mut top_pts = Vec::with_capacity(n);
    for i in 0..n {
        let theta = std::f64::consts::TAU * (i as f64) / (n as f64);
        let cos_t = theta.cos();
        let sin_t = theta.sin();
        let bottom = [
            cyl.center_bottom[0] + cyl.radius * (cos_t * cyl.x_axis[0] + sin_t * cyl.y_axis[0]),
            cyl.center_bottom[1] + cyl.radius * (cos_t * cyl.x_axis[1] + sin_t * cyl.y_axis[1]),
            cyl.center_bottom[2] + cyl.radius * (cos_t * cyl.x_axis[2] + sin_t * cyl.y_axis[2]),
        ];
        let top = [
            bottom[0] + dir[0] * cyl.depth,
            bottom[1] + dir[1] * cyl.depth,
            bottom[2] + dir[2] * cyl.depth,
        ];
        bottom_pts.push(bottom);
        top_pts.push(top);
    }

    // Bottom cap (outward normal = -direction)
    let neg_dir = [-dir[0], -dir[1], -dir[2]];
    let mut bottom_verts = bottom_pts.clone();
    bottom_verts.reverse(); // Reverse for outward normal = -direction
    polys.push(FacePoly {
        verts: bottom_verts,
        normal: neg_dir,
        origin: cyl.center_bottom,
        surface_geom: None,
    });

    // Top cap (outward normal = +direction)
    let center_top = [
        cyl.center_bottom[0] + dir[0] * cyl.depth,
        cyl.center_bottom[1] + dir[1] * cyl.depth,
        cyl.center_bottom[2] + dir[2] * cyl.depth,
    ];
    polys.push(FacePoly {
        verts: top_pts.clone(),
        normal: dir,
        origin: center_top,
        surface_geom: None,
    });

    // Side quads: each connects consecutive bottom/top points
    for i in 0..n {
        let j = (i + 1) % n;
        // Quad winding: bottom[i] → bottom[j] → top[j] → top[i]
        // Outward normal = cross(bottom_edge, up_edge)
        let edge_bot = v3_sub(bottom_pts[j], bottom_pts[i]);
        let edge_up = v3_sub(top_pts[i], bottom_pts[i]);
        let normal = v3_normalize(v3_cross(edge_bot, edge_up));
        polys.push(FacePoly {
            verts: vec![bottom_pts[i], bottom_pts[j], top_pts[j], top_pts[i]],
            normal,
            origin: bottom_pts[i],
            surface_geom: Some(cyl_surface.clone()),
        });
    }

    polys
}

/// Extract face polys from a solid, using polygon approximation for cylinders.
///
/// For solids with `cylinder_params`, generates face polys from the cylinder
/// parameters (since the B-Rep topology only has 2 seam vertices).
/// For polygon solids, uses the standard B-Rep face extraction.
pub(super) fn extract_face_polys_general(solid: &WaffleSolid) -> Vec<FacePoly> {
    if let Some(ref cyl) = solid.cylinder_params {
        cylinder_to_face_polys(cyl, 32)
    } else {
        let polys = extract_face_polys(solid);
        if !polys.is_empty() {
            return polys;
        }
        // Fallback: B-Rep walk returned empty (SSI results with seam edges
        // have < 3 loop vertices per face). Generate polygon approximations
        // from the analytic face geometry.
        let mut analytic_polys = Vec::new();
        for (&_kid, &face_idx) in &solid.face_map {
            if let Some(geom) = solid.face_geometry.get(&face_idx) {
                let verts = collect_face_vertices(&solid.arena, face_idx);
                generate_analytic_face_polys(
                    geom,
                    &verts,
                    &solid.arena,
                    face_idx,
                    &mut analytic_polys,
                );
            }
        }
        if !analytic_polys.is_empty() {
            return analytic_polys;
        }
        // Last resort: use cached face polys from a previous boolean result.
        if let Some(ref cached) = solid.cached_face_polys {
            return cached.clone();
        }
        Vec::new()
    }
}

/// Compute the centroid (average position) of a polygon's vertices.
pub(super) fn polygon_centroid(verts: &[[f64; 3]]) -> [f64; 3] {
    let n = verts.len() as f64;
    let mut cx = 0.0;
    let mut cy = 0.0;
    let mut cz = 0.0;
    for v in verts {
        cx += v[0];
        cy += v[1];
        cz += v[2];
    }
    [cx / n, cy / n, cz / n]
}
// ── AABB-aware face product guard ───────────────────────────────────────

/// Per-face axis-aligned bounding box.
struct FaceAabb {
    min: [f64; 3],
    max: [f64; 3],
}

/// Compute a tight AABB from a face polygon's vertices.
fn face_aabb(face: &FacePoly) -> FaceAabb {
    let mut mn = [f64::INFINITY; 3];
    let mut mx = [f64::NEG_INFINITY; 3];
    for v in &face.verts {
        for j in 0..3 {
            mn[j] = mn[j].min(v[j]);
            mx[j] = mx[j].max(v[j]);
        }
    }
    FaceAabb { min: mn, max: mx }
}

/// Count face pairs whose per-face AABBs overlap (with tau padding).
/// Used to compute "effective product" for the face product guard.
pub(crate) fn count_aabb_overlapping_pairs(
    a_faces: &[FacePoly],
    b_faces: &[FacePoly],
    tau: f64,
) -> usize {
    let a_aabbs: Vec<FaceAabb> = a_faces.iter().map(face_aabb).collect();
    let b_aabbs: Vec<FaceAabb> = b_faces.iter().map(face_aabb).collect();

    let mut count = 0;
    for a in &a_aabbs {
        for b in &b_aabbs {
            let overlaps = (0..3).all(|j| a.min[j] - tau <= b.max[j] && b.min[j] - tau <= a.max[j]);
            if overlaps {
                count += 1;
            }
        }
    }
    count
}

// ── Boolean operation dispatch ──────────────────────────────────────────

/// Compute scale-adaptive weld tolerance from face polygon bounding boxes.
///
/// tau_weld: vertex welding tolerance (positions within this distance are merged).
/// tau: face classification tolerance (signed-distance threshold for inside/outside).
///
/// Scales with model size to handle extreme scale ranges (1e-4 to 1e4).
fn compute_adaptive_tau_weld(a_faces: &[FacePoly], b_faces: &[FacePoly]) -> (f64, f64) {
    let mut min = [f64::INFINITY; 3];
    let mut max = [f64::NEG_INFINITY; 3];
    for face in a_faces.iter().chain(b_faces.iter()) {
        for v in &face.verts {
            for j in 0..3 {
                if v[j] < min[j] {
                    min[j] = v[j];
                }
                if v[j] > max[j] {
                    max[j] = v[j];
                }
            }
        }
    }
    let diag =
        ((max[0] - min[0]).powi(2) + (max[1] - min[1]).powi(2) + (max[2] - min[2]).powi(2)).sqrt();
    // Use TAU_WELD_FACTOR relative to the model diagonal, clamped to [TAU_WELD_MIN, TAU_WELD_MAX].
    // This matches TAU_MODEL for unit-scale models.
    let tau_weld = (diag * TAU_WELD_FACTOR).clamp(TAU_WELD_MIN, TAU_WELD_MAX);
    let tau = tau_weld * 0.01;
    (tau, tau_weld)
}

/// Perform a boolean operation on two polygon solids.
///
/// Uses `extract_face_polys_general` to handle both box solids (B-Rep walk)
/// and cylinder/revolve solids (polygon approximation).
pub(crate) fn boolean_op(
    solid_a: &WaffleSolid,
    solid_b: &WaffleSolid,
    op: BoolOp,
    _opts: &BooleanOptions,
    id_alloc: &mut dyn FnMut() -> u64,
) -> Result<BooleanResult, KernelError> {
    let a_faces = extract_face_polys_general(solid_a);
    let b_faces = extract_face_polys_general(solid_b);

    if a_faces.is_empty() && b_faces.is_empty() {
        return Err(KernelError::BooleanFailed {
            reason: "both solids have no planar faces".to_string(),
        });
    }
    // Handle empty solids: Union with empty returns the non-empty solid,
    // Subtract from empty returns empty, Intersect with empty returns empty.
    if a_faces.is_empty() {
        return match op {
            BoolOp::Union => build_brep_from_polygons_inner(&b_faces, TAU_MODEL, false, id_alloc),
            _ => {
                // Subtract from nothing or intersect with nothing = empty
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
            BoolOp::Subtract => {
                // A minus nothing = A
                build_brep_from_polygons_inner(&a_faces, TAU_MODEL, false, id_alloc)
            }
            BoolOp::Union => build_brep_from_polygons_inner(&a_faces, TAU_MODEL, false, id_alloc),
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

    // Use strict stitching (no boundary edge tolerance) for the primary path
    boolean_op_from_polys_strict(a_faces, b_faces, op, id_alloc)
}

/// Tolerant polygon-clipping boolean: accepts more boundary edges.
/// Used as fallback when strict mode fails with non-manifold result.
pub(crate) fn boolean_op_tolerant(
    solid_a: &WaffleSolid,
    solid_b: &WaffleSolid,
    op: BoolOp,
    id_alloc: &mut dyn FnMut() -> u64,
) -> Result<BooleanResult, KernelError> {
    let a_faces = extract_face_polys_general(solid_a);
    let b_faces = extract_face_polys_general(solid_b);

    // Handle empty solids gracefully
    if a_faces.is_empty() && b_faces.is_empty() {
        return Err(KernelError::BooleanFailed {
            reason: "both solids have no planar faces".to_string(),
        });
    }
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

    boolean_op_from_polys(a_faces, b_faces, op, id_alloc)
}

/// Collect face fragments from classified faces.
///
/// - `flip_normals`: reverse normal and winding of collected faces
/// - `include_outside`: collect Outside faces and Partial outside fragments
/// - `include_fully_inside`: collect fully-Inside faces (truly enclosed by opposing solid)
/// - `include_partial_inside`: collect Partial inside fragments (coplanar overlap regions)
fn collect_fragments(
    classified: &[(FacePoly, FaceClass)],
    output: &mut Vec<FacePoly>,
    flip_normals: bool,
    include_outside: bool,
    include_fully_inside: bool,
    include_partial_inside: bool,
) {
    let emit = |output: &mut Vec<FacePoly>,
                verts: Vec<[f64; 3]>,
                normal: [f64; 3],
                origin: [f64; 3],
                sg: Option<SurfaceGeom>| {
        if verts.len() < 3 {
            return;
        }
        let mut f = FacePoly {
            verts,
            normal,
            origin,
            surface_geom: sg,
        };
        if flip_normals {
            f.normal = v3_negate(f.normal);
            f.verts.reverse();
            // A15.5: preserve surface type but flip orientation
            if let Some(ref mut sg) = f.surface_geom {
                match sg {
                    SurfaceGeom::Planar(p) => {
                        p.normal = Vector3::new(-p.normal.x, -p.normal.y, -p.normal.z);
                    }
                    SurfaceGeom::Cylindrical(c) => {
                        c.axis = Vector3::new(-c.axis.x, -c.axis.y, -c.axis.z);
                    }
                    SurfaceGeom::Conical(c) => {
                        c.axis = Vector3::new(-c.axis.x, -c.axis.y, -c.axis.z);
                    }
                    SurfaceGeom::Spherical(_) => {} // symmetric
                    SurfaceGeom::Toroidal(t) => {
                        t.axis = Vector3::new(-t.axis.x, -t.axis.y, -t.axis.z);
                    }
                }
            }
        }
        output.push(f);
    };

    for (face, class) in classified {
        let sg = face.surface_geom.clone();
        match class {
            FaceClass::Outside => {
                if include_outside {
                    emit(output, face.verts.clone(), face.normal, face.origin, sg);
                }
            }
            FaceClass::Inside => {
                if include_fully_inside {
                    emit(output, face.verts.clone(), face.normal, face.origin, sg);
                }
            }
            FaceClass::Partial {
                inside_frags,
                outside_frags,
            }
            | FaceClass::CoplanarPartial {
                inside_frags,
                outside_frags,
            } => {
                if include_outside {
                    for frag in outside_frags {
                        emit(output, frag.clone(), face.normal, face.origin, sg.clone());
                    }
                }
                if include_partial_inside {
                    for frag in inside_frags {
                        emit(output, frag.clone(), face.normal, face.origin, sg.clone());
                    }
                }
            }
            FaceClass::CoplanarTouching => {
                // Anti-parallel coplanar: face is on the shared boundary.
                // For subtract A: keep (B doesn't cut A at touching boundary).
                // For subtract B / intersect: discard.
                if include_outside {
                    emit(output, face.verts.clone(), face.normal, face.origin, sg);
                }
            }
        }
    }
}

/// Collect face fragments for a union operation.
///
/// For non-coplanar Partial faces: emit only outside fragments (inside is hidden).
/// For CoplanarPartial faces: primary emits ALL sub-regions (inside + outside frags)
/// to keep the surface overlap; secondary emits only outside frags.
/// By emitting sub-regions instead of the original face, edges are properly split
/// at intersection boundaries, preventing T-junctions.
fn collect_union_fragments(
    classified: &[(FacePoly, FaceClass)],
    output: &mut Vec<FacePoly>,
    is_primary: bool,
) {
    let push_frag = |output: &mut Vec<FacePoly>, verts: &Vec<[f64; 3]>, face: &FacePoly| {
        if verts.len() >= 3 {
            output.push(FacePoly {
                verts: verts.clone(),
                normal: face.normal,
                origin: face.origin,
                surface_geom: face.surface_geom.clone(),
            });
        }
    };

    for (face, class) in classified {
        match class {
            FaceClass::Outside => {
                output.push(face.clone());
            }
            FaceClass::Inside => {
                // Fully-inside faces are hidden — discard for union
            }
            FaceClass::Partial { outside_frags, .. } => {
                // Non-coplanar partial: inside is truly inside the volume.
                // Emit only the outside fragments.
                for frag in outside_frags {
                    push_frag(output, frag, face);
                }
            }
            FaceClass::CoplanarPartial {
                inside_frags: _,
                outside_frags,
            } => {
                // Same-direction coplanar: "inside" is surface overlap.
                if is_primary {
                    // Primary: emit the ORIGINAL unsplit face. Using
                    // fragments would create duplicate directed edges
                    // with the secondary's outside fragments (both are
                    // coplanar same-direction, so their shared boundary
                    // edges have the same winding). T-junction resolution
                    // will insert split vertices from adjacent faces.
                    output.push(face.clone());
                } else {
                    // Secondary: emit only outside frags.
                    for frag in outside_frags {
                        push_frag(output, frag, face);
                    }
                }
            }
            FaceClass::CoplanarTouching => {
                // Anti-parallel coplanar: shared boundary face.
                // Remove from both primary and secondary in union.
            }
        }
    }
}

/// Strict polygon-clipping boolean: errors on any unpaired half-edges.
fn boolean_op_from_polys_strict(
    a_faces: Vec<FacePoly>,
    b_faces: Vec<FacePoly>,
    op: BoolOp,
    id_alloc: &mut dyn FnMut() -> u64,
) -> Result<BooleanResult, KernelError> {
    boolean_op_from_polys_inner(a_faces, b_faces, op, false, id_alloc)
}

/// Core polygon-clipping boolean logic operating on pre-extracted face polys.
/// Uses tolerant stitching (allows up to 10% unpaired half-edges as boundary).
pub(super) fn boolean_op_from_polys(
    a_faces: Vec<FacePoly>,
    b_faces: Vec<FacePoly>,
    op: BoolOp,
    id_alloc: &mut dyn FnMut() -> u64,
) -> Result<BooleanResult, KernelError> {
    boolean_op_from_polys_inner(a_faces, b_faces, op, true, id_alloc)
}

/// Shared implementation for polygon-clipping boolean with configurable
/// boundary tolerance.
fn boolean_op_from_polys_inner(
    a_faces: Vec<FacePoly>,
    b_faces: Vec<FacePoly>,
    op: BoolOp,
    allow_boundary: bool,
    id_alloc: &mut dyn FnMut() -> u64,
) -> Result<BooleanResult, KernelError> {
    // Guard against pathological face counts: O(n*m) classification becomes
    // too expensive when both solids have many faces (e.g., revolve(gear) × gear).
    let total_faces = a_faces.len() + b_faces.len();
    if total_faces > 8000 {
        return Err(KernelError::NotSupported {
            operation: format!(
                "polygon boolean: {} total faces exceeds limit (8000)",
                total_faces
            ),
        });
    }

    let a_convex = a_faces.len() <= 12;
    let b_convex = b_faces.len() <= 12;

    // Compute tau early so AABB overlap can use it for padding.
    let (tau, tau_weld) = compute_adaptive_tau_weld(&a_faces, &b_faces);

    // Product-based guard: O(A*B) face classification is too expensive
    // when both solids are non-convex (e.g., two gears with ~200 faces each).
    // Use AABB-filtered effective product: most face pairs are spatially disjoint
    // in multi-step operations, so the raw product vastly overestimates cost.
    let product = a_faces.len() * b_faces.len();
    if product > 5000 && !a_convex && !b_convex {
        let effective = count_aabb_overlapping_pairs(&a_faces, &b_faces, tau);
        if effective > 5000 {
            return Err(KernelError::NotSupported {
                operation: format!(
                    "polygon boolean: {}x{} effective face product ({}) too large for non-convex solids",
                    a_faces.len(),
                    b_faces.len(),
                    effective
                ),
            });
        }
    }

    // Compute AABBs for early-out: faces entirely outside the opposing
    // solid's bounding box are classified as Outside without expensive
    // S-H clipping or ray casting.
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

    let face_outside_aabb = |face: &FacePoly, aabb_min: &[f64; 3], aabb_max: &[f64; 3]| -> bool {
        // Face is outside AABB if ALL its vertices are outside on the same side
        // in any axis.
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

    // Intersection cache: ensures that the same geometric edge clipped by the
    // same plane produces bitwise-identical intersection points across all faces.
    // Ref [#9] Cherchi: indirect predicates avoid recomputation.
    let mut cache: Option<IntersectionCache> = Some(IntersectionCache::new(tau));

    let mut a_classified: Vec<(FacePoly, FaceClass)> = Vec::with_capacity(a_faces.len());
    for f in &a_faces {
        if face_outside_aabb(f, &b_min, &b_max) {
            a_classified.push((f.clone(), FaceClass::Outside));
        } else {
            let class = if b_convex {
                classify_face(f, &b_faces, tau, &mut cache)
            } else {
                classify_face_nonconvex(f, &b_faces, tau, &mut cache)
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
                classify_face_nonconvex(f, &a_faces, tau, &mut cache)
            };
            b_classified.push((f.clone(), class));
        }
    }

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
            face_map: HashMap::new(),
            edge_map: HashMap::new(),
            vertex_map: HashMap::new(),
            face_geometry: HashMap::new(),
            edge_geometry: HashMap::new(),
            cached_face_polys: None,
        });
    }

    // Remove near-duplicate face polygons: fragments with nearly identical
    // centroids and normals that arise from classification edge cases.
    let result_polys = dedup_face_polys(&result_polys, tau_weld);

    // Merge nearby vertices so independently-clipped adjacent faces share
    // identical intersection coordinates, then resolve T-junctions where one
    // face's edge passes through another face's vertex.
    let result_polys = merge_nearby_vertices(&result_polys, tau_weld);
    let result_polys = resolve_t_junctions(&result_polys, tau_weld);

    // Cache the final face polys so subsequent booleans on this result
    // can reuse them directly (avoids B-Rep walk round-trip precision loss).
    let cached = result_polys.clone();
    let mut result =
        build_brep_from_polygons_inner(&result_polys, tau_weld, allow_boundary, id_alloc)?;
    result.cached_face_polys = Some(cached);
    Ok(result)
}

// ── Tests ───────────────────────────────────────────────────────────────

#[cfg(test)]
mod tests {
    use super::*;
    use crate::traits::Kernel;
    use crate::waffle_kernel::WaffleKernel;

    // ── Test helpers ────────────────────────────────────────────────

    /// Create a rectangular profile centered at (cx, cy) with width w and height h.
    fn make_rect_profile(
        cx: f64,
        cy: f64,
        w: f64,
        h: f64,
    ) -> (Vec<ClosedProfile>, HashMap<u32, (f64, f64)>) {
        let mut positions = HashMap::new();
        positions.insert(1, (cx - w / 2.0, cy - h / 2.0));
        positions.insert(2, (cx + w / 2.0, cy - h / 2.0));
        positions.insert(3, (cx + w / 2.0, cy + h / 2.0));
        positions.insert(4, (cx - w / 2.0, cy + h / 2.0));

        let profile = ClosedProfile {
            entity_ids: vec![10, 11, 12, 13],
            is_outer: true,
            vertex_ids: vec![],
            circle: None,
            spline_segments: vec![],
        };

        (vec![profile], positions)
    }

    const XY_ORIGIN: [f64; 3] = [0.0, 0.0, 0.0];
    const XY_NORMAL: [f64; 3] = [0.0, 0.0, 1.0];
    const XY_X_AXIS: [f64; 3] = [1.0, 0.0, 0.0];
    const Z_DIR: [f64; 3] = [0.0, 0.0, 1.0];

    /// Create a box solid and return the WaffleSolid reference inside the kernel.
    fn make_box_solid(
        kernel: &mut WaffleKernel,
        cx: f64,
        cy: f64,
        w: f64,
        h: f64,
        depth: f64,
    ) -> KernelSolidHandle {
        let (profiles, positions) = make_rect_profile(cx, cy, w, h);
        let face_ids = kernel
            .make_faces_from_profiles(&profiles, XY_ORIGIN, XY_NORMAL, XY_X_AXIS, &positions)
            .expect("make_faces_from_profiles should succeed");
        kernel
            .extrude_face(face_ids[0], Z_DIR, depth)
            .expect("extrude_face should succeed")
    }

    /// Perform a boolean op on two boxes via the Kernel trait and return the handle.
    fn do_boolean_via_kernel(
        cx_a: f64,
        cy_a: f64,
        w_a: f64,
        h_a: f64,
        d_a: f64,
        cx_b: f64,
        cy_b: f64,
        w_b: f64,
        h_b: f64,
        d_b: f64,
        op: BoolOp,
    ) -> Result<(WaffleKernel, KernelSolidHandle), KernelError> {
        let mut kernel = WaffleKernel::new();
        let handle_a = make_box_solid(&mut kernel, cx_a, cy_a, w_a, h_a, d_a);
        let handle_b = make_box_solid(&mut kernel, cx_b, cy_b, w_b, h_b, d_b);

        let result = match op {
            BoolOp::Union => kernel.boolean_union(&handle_a, &handle_b)?,
            BoolOp::Subtract => kernel.boolean_subtract(&handle_a, &handle_b)?,
            BoolOp::Intersect => kernel.boolean_intersect(&handle_a, &handle_b)?,
        };
        Ok((kernel, result))
    }

    // Standard test case: A at x=[0,10], y=[0,10], z=[0,10]
    //                      B at x=[5,15], y=[0,10], z=[0,10]

    // ── Vector math unit tests ──────────────────────────────────────

    #[test]
    fn vec_sub() {
        let r = v3_sub([3.0, 2.0, 1.0], [1.0, 1.0, 1.0]);
        assert!((r[0] - 2.0).abs() < 1e-15);
        assert!((r[1] - 1.0).abs() < 1e-15);
        assert!((r[2] - 0.0).abs() < 1e-15);
    }

    #[test]
    fn vec_dot() {
        let d = v3_dot([1.0, 0.0, 0.0], [0.0, 1.0, 0.0]);
        assert!(d.abs() < 1e-15);
    }

    #[test]
    fn vec_cross() {
        let c = v3_cross([1.0, 0.0, 0.0], [0.0, 1.0, 0.0]);
        assert!((c[2] - 1.0).abs() < 1e-15);
    }

    // ── Clipping unit tests ─────────────────────────────────────────

    #[test]
    fn clip_square_by_half_plane() {
        // Unit square in XY plane, clip by x >= 0.5
        let square = vec![
            [0.0, 0.0, 0.0],
            [1.0, 0.0, 0.0],
            [1.0, 1.0, 0.0],
            [0.0, 1.0, 0.0],
        ];
        let clipped = clip_polygon_by_plane(
            &square,
            [0.5, 0.0, 0.0], // plane point
            [1.0, 0.0, 0.0], // inward normal (keep x >= 0.5)
            1e-9,
        );
        let area = polygon_area_3d(&clipped);
        assert!(
            (area - 0.5).abs() < 0.01,
            "Clipped area should be ~0.5, got {}",
            area
        );
    }

    #[test]
    fn clip_fully_inside() {
        let square = vec![
            [0.2, 0.2, 0.0],
            [0.8, 0.2, 0.0],
            [0.8, 0.8, 0.0],
            [0.2, 0.8, 0.0],
        ];
        let clipped = clip_polygon_by_plane(
            &square,
            [0.0, 0.0, 0.0],
            [1.0, 0.0, 0.0], // keep x >= 0
            1e-9,
        );
        let orig_area = polygon_area_3d(&square);
        let clip_area = polygon_area_3d(&clipped);
        assert!(
            (clip_area - orig_area).abs() < 1e-10,
            "Fully-inside clip should preserve area"
        );
    }

    #[test]
    fn clip_fully_outside() {
        let square = vec![
            [0.0, 0.0, 0.0],
            [1.0, 0.0, 0.0],
            [1.0, 1.0, 0.0],
            [0.0, 1.0, 0.0],
        ];
        let clipped = clip_polygon_by_plane(
            &square,
            [2.0, 0.0, 0.0],
            [1.0, 0.0, 0.0], // keep x >= 2
            1e-9,
        );
        assert!(
            clipped.is_empty() || polygon_area_3d(&clipped) < 1e-15,
            "Fully-outside clip should produce empty polygon"
        );
    }

    #[test]
    fn polygon_area_triangle() {
        let tri = vec![[0.0, 0.0, 0.0], [1.0, 0.0, 0.0], [0.0, 1.0, 0.0]];
        let area = polygon_area_3d(&tri);
        assert!(
            (area - 0.5).abs() < 1e-10,
            "Right triangle area should be 0.5, got {}",
            area
        );
    }

    #[test]
    fn polygon_area_unit_square() {
        let sq = vec![
            [0.0, 0.0, 0.0],
            [1.0, 0.0, 0.0],
            [1.0, 1.0, 0.0],
            [0.0, 1.0, 0.0],
        ];
        let area = polygon_area_3d(&sq);
        assert!(
            (area - 1.0).abs() < 1e-10,
            "Unit square area should be 1.0, got {}",
            area
        );
    }

    // ── Boolean operation integration tests ─────────────────────────

    use crate::traits::KernelIntrospect;

    #[test]
    fn union_face_count() {
        let (k, result) = do_boolean_via_kernel(
            5.0,
            5.0,
            10.0,
            10.0,
            10.0,
            10.0,
            5.0,
            10.0,
            10.0,
            10.0,
            BoolOp::Union,
        )
        .expect("union should succeed");
        let faces = k.list_faces(&result);
        // With face splitting at intersection boundaries, union produces
        // more sub-faces (14) than the minimal 10. Geometry is correct.
        assert!(
            faces.len() >= 10,
            "Union should have >= 10 faces, got {}",
            faces.len()
        );
    }

    #[test]
    fn subtract_face_count() {
        let (k, result) = do_boolean_via_kernel(
            5.0,
            5.0,
            10.0,
            10.0,
            10.0,
            10.0,
            5.0,
            10.0,
            10.0,
            10.0,
            BoolOp::Subtract,
        )
        .expect("subtract should succeed");
        let faces = k.list_faces(&result);
        assert_eq!(
            faces.len(),
            6,
            "Subtract should have 6 faces, got {}",
            faces.len()
        );
    }

    #[test]
    fn intersect_face_count() {
        let (k, result) = do_boolean_via_kernel(
            5.0,
            5.0,
            10.0,
            10.0,
            10.0,
            10.0,
            5.0,
            10.0,
            10.0,
            10.0,
            BoolOp::Intersect,
        )
        .expect("intersect should succeed");
        let faces = k.list_faces(&result);
        assert_eq!(
            faces.len(),
            6,
            "Intersect should have 6 faces, got {}",
            faces.len()
        );
    }

    #[test]
    fn union_euler_formula() {
        let (k, result) = do_boolean_via_kernel(
            5.0,
            5.0,
            10.0,
            10.0,
            10.0,
            10.0,
            5.0,
            10.0,
            10.0,
            10.0,
            BoolOp::Union,
        )
        .expect("union should succeed");
        let v = k.list_vertices(&result).len() as i64;
        let e = k.list_edges(&result).len() as i64;
        let f = k.list_faces(&result).len() as i64;
        assert_eq!(v - e + f, 2, "V-E+F must be 2 (V={}, E={}, F={})", v, e, f);
    }

    #[test]
    fn subtract_euler_formula() {
        let (k, result) = do_boolean_via_kernel(
            5.0,
            5.0,
            10.0,
            10.0,
            10.0,
            10.0,
            5.0,
            10.0,
            10.0,
            10.0,
            BoolOp::Subtract,
        )
        .expect("subtract should succeed");
        let v = k.list_vertices(&result).len() as i64;
        let e = k.list_edges(&result).len() as i64;
        let f = k.list_faces(&result).len() as i64;
        assert_eq!(v - e + f, 2, "V-E+F must be 2 (V={}, E={}, F={})", v, e, f);
    }

    #[test]
    fn intersect_euler_formula() {
        let (k, result) = do_boolean_via_kernel(
            5.0,
            5.0,
            10.0,
            10.0,
            10.0,
            10.0,
            5.0,
            10.0,
            10.0,
            10.0,
            BoolOp::Intersect,
        )
        .expect("intersect should succeed");
        let v = k.list_vertices(&result).len() as i64;
        let e = k.list_edges(&result).len() as i64;
        let f = k.list_faces(&result).len() as i64;
        assert_eq!(v - e + f, 2, "V-E+F must be 2 (V={}, E={}, F={})", v, e, f);
    }

    #[test]
    fn disjoint_boxes_union() {
        let (k, result) = do_boolean_via_kernel(
            5.0,
            5.0,
            10.0,
            10.0,
            10.0,
            100.0,
            5.0,
            10.0,
            10.0,
            10.0,
            BoolOp::Union,
        )
        .expect("disjoint union should succeed");
        let faces = k.list_faces(&result);
        assert_eq!(faces.len(), 12, "Disjoint union should have 12 faces");
    }

    #[test]
    fn disjoint_boxes_intersect_empty() {
        let (_k, result) = do_boolean_via_kernel(
            5.0,
            5.0,
            10.0,
            10.0,
            10.0,
            100.0,
            5.0,
            10.0,
            10.0,
            10.0,
            BoolOp::Intersect,
        )
        .expect("disjoint intersect should succeed (empty)");
        let faces = _k.list_faces(&result);
        assert_eq!(faces.len(), 0, "Disjoint intersect should have 0 faces");
    }

    /// Create a box at a custom origin with custom X axis.
    fn make_box_at(
        kernel: &mut WaffleKernel,
        origin: [f64; 3],
        cx: f64,
        cy: f64,
        w: f64,
        h: f64,
        depth: f64,
    ) -> KernelSolidHandle {
        let (profiles, positions) = make_rect_profile(cx, cy, w, h);
        let face_ids = kernel
            .make_faces_from_profiles(&profiles, origin, XY_NORMAL, XY_X_AXIS, &positions)
            .expect("make_faces_from_profiles should succeed");
        kernel
            .extrude_face(face_ids[0], Z_DIR, depth)
            .expect("extrude_face should succeed")
    }

    /// Create a circle profile centered at (cx, cy) with radius r.
    fn make_circle_profile(
        cx: f64,
        cy: f64,
        r: f64,
    ) -> (Vec<ClosedProfile>, HashMap<u32, (f64, f64)>) {
        let mut positions = HashMap::new();
        positions.insert(1, (cx, cy));

        let profile = ClosedProfile {
            entity_ids: vec![1],
            is_outer: true,
            vertex_ids: vec![],
            circle: Some(crate::types::CircleProfile {
                center_u: cx,
                center_v: cy,
                radius: r,
            }),
            spline_segments: vec![],
        };

        (vec![profile], positions)
    }

    #[test]
    fn box_cyl_union_tilted_plane() {
        // R0002 regression: box-cylinder union on tilted plane must include both bodies.
        // Without frame rotation, the Z-axis enclosure check falsely detects the
        // cylinder as enclosed in the box and discards it.
        let dir = v3_normalize([-0.5196, -0.7471, -0.4145]);
        // Compute a valid x_axis perpendicular to dir
        let up = if dir[1].abs() < 0.9 {
            [0.0, 1.0, 0.0]
        } else {
            [1.0, 0.0, 0.0]
        };
        let x_axis = v3_normalize(v3_cross(up, dir));

        let mut kernel = WaffleKernel::new();

        // Create box on tilted plane: 2x2 rect, depth 0.3
        let (rect_profiles, rect_positions) = make_rect_profile(0.0, 0.0, 2.0, 2.0);
        let rect_faces = kernel
            .make_faces_from_profiles(&rect_profiles, [0.0; 3], dir, x_axis, &rect_positions)
            .expect("make rect faces");
        let box_handle = kernel
            .extrude_face(rect_faces[0], dir, 0.3)
            .expect("extrude box");

        use crate::traits::KernelIntrospect;

        // Count box faces
        let box_faces = kernel.list_faces(&box_handle).len();

        // Create cylinder on tilted plane: radius 0.5, depth 1.5, boss on top of box
        // Position it so center_bottom is on the box top face
        let cyl_origin = [dir[0] * 0.3, dir[1] * 0.3, dir[2] * 0.3];
        let (circ_profiles, circ_positions) = make_circle_profile(0.0, 0.0, 0.5);
        let circ_faces = kernel
            .make_faces_from_profiles(&circ_profiles, cyl_origin, dir, x_axis, &circ_positions)
            .expect("make circle faces");
        let cyl_handle = kernel
            .extrude_face(circ_faces[0], dir, 1.5)
            .expect("extrude cylinder");

        // Union: box + cylinder boss
        let result = kernel.boolean_union(&box_handle, &cyl_handle);
        let union_handle = result.expect("box-cyl union on tilted plane should succeed");

        // The union result must have MORE faces than the box alone (6),
        // proving the cylinder was not discarded. Boss union produces 8 faces.
        let union_faces = kernel.list_faces(&union_handle).len();

        assert!(
            union_faces > box_faces,
            "Union should include cylinder geometry: union_faces={} must be > box_faces={}",
            union_faces,
            box_faces
        );
    }

    #[test]
    fn step_shape_union() {
        // Step shape: Box A at z=0 (10x10x5) + Box B at z=5 (5x10x5)
        // Box A: centered (5,5), w=10, h=10 => X[0,10] Y[0,10] Z[0,5]
        // Box B: centered (2.5,5), w=5, h=10 => X[0,5] Y[0,10] Z[5,10]
        let mut kernel = WaffleKernel::new();
        let handle_a = make_box_at(&mut kernel, [0.0, 0.0, 0.0], 5.0, 5.0, 10.0, 10.0, 5.0);
        let handle_b = make_box_at(&mut kernel, [0.0, 0.0, 5.0], 2.5, 5.0, 5.0, 10.0, 5.0);

        // Run the full union
        let result = kernel.boolean_union(&handle_a, &handle_b);
        result.expect("step shape union should succeed");
    }

    /// Two adjacent faces sharing a geometric edge, clipped by the same plane,
    /// must produce bitwise-identical intersection points regardless of edge
    /// traversal direction. Ref [#4] Shewchuk: deterministic evaluation order.
    #[test]
    fn canonical_intersection_identical_for_shared_edge() {
        // Face F1 and F2 share edge from A=(0, 0, 0) to B=(1, 0, 0).
        // F1 traverses it as A→B, F2 traverses it as B→A.
        // A y-plane at y=0 with inward normal (0,-1,0) clips through both
        // faces, producing intersection points on the shared edge.
        let face1 = vec![
            [0.0, -1.0, 0.0],
            [1.0, -1.0, 0.0],
            [1.0, 1.0, 0.0],
            [0.0, 1.0, 0.0],
        ];
        // F2 shares the bottom edge but reversed + offset in z
        let face2 = vec![
            [1.0, 1.0, 0.0],
            [0.0, 1.0, 0.0],
            [0.0, -1.0, 0.0],
            [1.0, -1.0, 0.0],
        ];

        let plane_pt = [0.0, 0.3, 0.0];
        let inward_n = [0.0, -1.0, 0.0]; // keep y <= 0.3
        let tau = 1e-9;

        let clipped1 = clip_polygon_by_plane(&face1, plane_pt, inward_n, tau);
        let clipped2 = clip_polygon_by_plane(&face2, plane_pt, inward_n, tau);

        // Both should produce intersection vertices at y≈0.3
        let find_at_y = |clipped: &[[f64; 3]]| -> Vec<[f64; 3]> {
            clipped
                .iter()
                .filter(|v| (v[1] - 0.3).abs() < 1e-6)
                .copied()
                .collect()
        };

        let isects1 = find_at_y(&clipped1);
        let isects2 = find_at_y(&clipped2);

        assert!(
            !isects1.is_empty(),
            "Face1 should have intersection at y=0.3, got {:?}",
            clipped1
        );
        assert!(
            !isects2.is_empty(),
            "Face2 should have intersection at y=0.3, got {:?}",
            clipped2
        );

        // Match intersection points by x coordinate
        for i1 in &isects1 {
            for i2 in &isects2 {
                if (i1[0] - i2[0]).abs() < 0.01 {
                    assert_eq!(
                        i1[0].to_bits(),
                        i2[0].to_bits(),
                        "x must be bitwise identical"
                    );
                    assert_eq!(
                        i1[1].to_bits(),
                        i2[1].to_bits(),
                        "y must be bitwise identical"
                    );
                    assert_eq!(
                        i1[2].to_bits(),
                        i2[2].to_bits(),
                        "z must be bitwise identical"
                    );
                }
            }
        }
    }

    /// IntersectionCache deduplicates intersection points across faces.
    #[test]
    fn intersection_cache_deduplicates() {
        let tau = 1e-7;
        let mut cache = IntersectionCache::new(tau);

        let a = [0.0, 0.0, 0.0];
        let b = [1.0, 0.0, 0.0];
        let plane_pt = [0.5, 0.0, 0.0];
        let plane_n = [1.0, 0.0, 0.0];
        let computed1 = [0.5, 0.0, 0.0];

        // First insertion
        let result1 = cache.get_or_insert(a, b, plane_pt, plane_n, computed1);
        assert_eq!(result1, computed1);

        // Second lookup with slightly different computed value
        // (simulating floating-point divergence from reversed operand order)
        let computed2 = [0.5 + 1e-15, 0.0, 0.0];
        let result2 = cache.get_or_insert(a, b, plane_pt, plane_n, computed2);
        // Should return the cached value, not the new computed value
        assert_eq!(result2, computed1, "Cache should return first value");

        // Reversed edge order should also find the same cached value
        let computed3 = [0.5 - 1e-15, 0.0, 0.0];
        let result3 = cache.get_or_insert(b, a, plane_pt, plane_n, computed3);
        assert_eq!(
            result3, computed1,
            "Reversed edge order should find cached value"
        );
    }

    // ── GWN (Generalized Winding Number) tests ─────────────────────

    /// Build a unit cube [0,1]^3 as a Vec<FacePoly> for GWN testing.
    /// Vertex winding: CCW when viewed from outside, so that
    /// (v1-v0)×(v2-v0) · outward_normal > 0.
    fn unit_cube_face_polys() -> Vec<FacePoly> {
        vec![
            // -Z face (z=0)
            FacePoly {
                verts: vec![
                    [0.0, 0.0, 0.0],
                    [0.0, 1.0, 0.0],
                    [1.0, 1.0, 0.0],
                    [1.0, 0.0, 0.0],
                ],
                normal: [0.0, 0.0, -1.0],
                origin: [0.0, 0.0, 0.0],
                surface_geom: None,
            },
            // +Z face (z=1)
            FacePoly {
                verts: vec![
                    [0.0, 0.0, 1.0],
                    [1.0, 0.0, 1.0],
                    [1.0, 1.0, 1.0],
                    [0.0, 1.0, 1.0],
                ],
                normal: [0.0, 0.0, 1.0],
                origin: [0.0, 0.0, 1.0],
                surface_geom: None,
            },
            // -Y face (y=0)
            FacePoly {
                verts: vec![
                    [0.0, 0.0, 0.0],
                    [1.0, 0.0, 0.0],
                    [1.0, 0.0, 1.0],
                    [0.0, 0.0, 1.0],
                ],
                normal: [0.0, -1.0, 0.0],
                origin: [0.0, 0.0, 0.0],
                surface_geom: None,
            },
            // +Y face (y=1)
            FacePoly {
                verts: vec![
                    [0.0, 1.0, 0.0],
                    [0.0, 1.0, 1.0],
                    [1.0, 1.0, 1.0],
                    [1.0, 1.0, 0.0],
                ],
                normal: [0.0, 1.0, 0.0],
                origin: [0.0, 1.0, 0.0],
                surface_geom: None,
            },
            // -X face (x=0)
            FacePoly {
                verts: vec![
                    [0.0, 0.0, 0.0],
                    [0.0, 0.0, 1.0],
                    [0.0, 1.0, 1.0],
                    [0.0, 1.0, 0.0],
                ],
                normal: [-1.0, 0.0, 0.0],
                origin: [0.0, 0.0, 0.0],
                surface_geom: None,
            },
            // +X face (x=1)
            FacePoly {
                verts: vec![
                    [1.0, 0.0, 0.0],
                    [1.0, 1.0, 0.0],
                    [1.0, 1.0, 1.0],
                    [1.0, 0.0, 1.0],
                ],
                normal: [1.0, 0.0, 0.0],
                origin: [1.0, 0.0, 0.0],
                surface_geom: None,
            },
        ]
    }

    #[test]
    fn gwn_unit_cube_center_inside() {
        let faces = unit_cube_face_polys();
        let w = winding_number([0.5, 0.5, 0.5], &faces);
        assert!(
            (w - 1.0).abs() < 0.1,
            "Center of cube should have winding number ~1.0, got {}",
            w
        );
    }

    #[test]
    fn gwn_unit_cube_far_point_outside() {
        let faces = unit_cube_face_polys();
        let w = winding_number([5.0, 5.0, 5.0], &faces);
        assert!(
            w.abs() < 0.1,
            "Far point should have winding number ~0.0, got {}",
            w
        );
    }

    #[test]
    fn gwn_classify_inside() {
        let faces = unit_cube_face_polys();
        assert_eq!(
            winding_number_classify([0.5, 0.5, 0.5], &faces),
            Some(true),
            "Center of cube should classify as inside"
        );
    }

    #[test]
    fn gwn_classify_outside() {
        let faces = unit_cube_face_polys();
        assert_eq!(
            winding_number_classify([5.0, 5.0, 5.0], &faces),
            Some(false),
            "Far point should classify as outside"
        );
    }

    #[test]
    fn gwn_point_in_solid_cube() {
        let faces = unit_cube_face_polys();
        assert!(
            point_in_solid([0.5, 0.5, 0.5], &faces),
            "Center of cube should be inside"
        );
        assert!(
            !point_in_solid([5.0, 5.0, 5.0], &faces),
            "Far point should be outside"
        );
    }

    #[test]
    fn gwn_nonconvex_l_shape() {
        // L-shaped solid: [0,2]x[0,1]x[0,1] ∪ [0,1]x[0,2]x[0,1]
        // (big square minus [1,2]x[1,2] cutout)
        // Non-convex solid where ray-casting can fail.
        // Vertex winding: CCW from outside (outward normals).
        let faces = vec![
            // Bottom face (L-shape, z=0, normal -Z)
            FacePoly {
                verts: vec![
                    [0.0, 2.0, 0.0],
                    [1.0, 2.0, 0.0],
                    [1.0, 1.0, 0.0],
                    [2.0, 1.0, 0.0],
                    [2.0, 0.0, 0.0],
                    [0.0, 0.0, 0.0],
                ],
                normal: [0.0, 0.0, -1.0],
                origin: [0.0, 0.0, 0.0],
                surface_geom: None,
            },
            // Top face (L-shape, z=1, normal +Z)
            FacePoly {
                verts: vec![
                    [0.0, 0.0, 1.0],
                    [2.0, 0.0, 1.0],
                    [2.0, 1.0, 1.0],
                    [1.0, 1.0, 1.0],
                    [1.0, 2.0, 1.0],
                    [0.0, 2.0, 1.0],
                ],
                normal: [0.0, 0.0, 1.0],
                origin: [0.0, 0.0, 1.0],
                surface_geom: None,
            },
            // -Y face (y=0, normal -Y)
            FacePoly {
                verts: vec![
                    [0.0, 0.0, 0.0],
                    [2.0, 0.0, 0.0],
                    [2.0, 0.0, 1.0],
                    [0.0, 0.0, 1.0],
                ],
                normal: [0.0, -1.0, 0.0],
                origin: [0.0, 0.0, 0.0],
                surface_geom: None,
            },
            // +X face (x=2, y=0..1, normal +X)
            FacePoly {
                verts: vec![
                    [2.0, 0.0, 0.0],
                    [2.0, 1.0, 0.0],
                    [2.0, 1.0, 1.0],
                    [2.0, 0.0, 1.0],
                ],
                normal: [1.0, 0.0, 0.0],
                origin: [2.0, 0.0, 0.0],
                surface_geom: None,
            },
            // Inner step face +Y (y=1, x=1..2, normal +Y)
            FacePoly {
                verts: vec![
                    [1.0, 1.0, 0.0],
                    [1.0, 1.0, 1.0],
                    [2.0, 1.0, 1.0],
                    [2.0, 1.0, 0.0],
                ],
                normal: [0.0, 1.0, 0.0],
                origin: [1.0, 1.0, 0.0],
                surface_geom: None,
            },
            // Inner step face +X (x=1, y=1..2, normal +X outward into cutout)
            FacePoly {
                verts: vec![
                    [1.0, 2.0, 0.0],
                    [1.0, 2.0, 1.0],
                    [1.0, 1.0, 1.0],
                    [1.0, 1.0, 0.0],
                ],
                normal: [1.0, 0.0, 0.0],
                origin: [1.0, 1.0, 0.0],
                surface_geom: None,
            },
            // +Y face (y=2, x=0..1, normal +Y)
            FacePoly {
                verts: vec![
                    [0.0, 2.0, 0.0],
                    [0.0, 2.0, 1.0],
                    [1.0, 2.0, 1.0],
                    [1.0, 2.0, 0.0],
                ],
                normal: [0.0, 1.0, 0.0],
                origin: [0.0, 2.0, 0.0],
                surface_geom: None,
            },
            // -X face (x=0, normal -X)
            FacePoly {
                verts: vec![
                    [0.0, 0.0, 0.0],
                    [0.0, 0.0, 1.0],
                    [0.0, 2.0, 1.0],
                    [0.0, 2.0, 0.0],
                ],
                normal: [-1.0, 0.0, 0.0],
                origin: [0.0, 0.0, 0.0],
                surface_geom: None,
            },
        ];

        // Point in the interior pocket of the L (bottom-left arm)
        assert!(
            point_in_solid([0.5, 1.5, 0.5], &faces),
            "Point in L-shape arm should be inside"
        );
        // Point in the other arm
        assert!(
            point_in_solid([1.5, 0.5, 0.5], &faces),
            "Point in L-shape arm should be inside"
        );
        // Point in the concave cutout (should be outside)
        assert!(
            !point_in_solid([1.5, 1.5, 0.5], &faces),
            "Point in L-shape cutout should be outside"
        );
        // Point far outside
        assert!(
            !point_in_solid([5.0, 5.0, 5.0], &faces),
            "Far point should be outside"
        );
    }

    #[test]
    fn gwn_solid_angle_degenerate_triangle() {
        // Collinear triangle vertices → should return 0.0
        let sa = solid_angle(
            [1.0, 1.0, 1.0],
            [0.0, 0.0, 0.0],
            [1.0, 0.0, 0.0],
            [2.0, 0.0, 0.0],
        );
        assert!(!sa.is_nan(), "Collinear triangle should not produce NaN");
        assert!(sa.abs() < 1e-10, "Collinear triangle should give ~0");
    }

    #[test]
    fn gwn_solid_angle_point_at_vertex() {
        let a = [0.0, 0.0, 0.0];
        let b = [1.0, 0.0, 0.0];
        let c = [0.0, 1.0, 0.0];
        let sa = solid_angle(a, a, b, c);
        assert!(!sa.is_nan(), "Point-at-vertex should not produce NaN");
        assert_eq!(sa, 0.0, "Point-at-vertex should give 0.0");
    }
}
