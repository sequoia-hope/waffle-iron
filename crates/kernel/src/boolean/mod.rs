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
    is_coplanar, is_face_set_convex, merge_nearby_vertices, resolve_t_junctions, CoplanarClass,
    IntersectionCache,
};
use stitch::{build_brep_from_polygons, build_brep_from_polygons_inner};

use crate::geometry::curve::{CurveGeom, Line3D};
use crate::geometry::point::{Point3, Vector3};
use crate::geometry::surface::{Cylinder, SurfaceGeom};
use crate::topology::arena::TopoArena;
use crate::topology::half_edge::*;
use crate::types::*;
use crate::units::{
    CURVATURE_SUBDIV_THRESHOLD, TAU_MODEL, TAU_NORMALIZE, TAU_WELD_FACTOR, TAU_WELD_MAX,
    TAU_WELD_MIN,
};
use crate::vecmath::*;
use crate::waffle_kernel::{rotate_point_around_axis, CylinderParams, RevolveParams, WaffleSolid};
use std::collections::BTreeMap;

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
    pub face_map: BTreeMap<u64, FaceIdx>,
    pub edge_map: BTreeMap<u64, EdgeIdx>,
    pub vertex_map: BTreeMap<u64, VertexIdx>,
    pub face_geometry: BTreeMap<FaceIdx, SurfaceGeom>,
    pub edge_geometry: BTreeMap<EdgeIdx, CurveGeom>,
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
                let up = if axis[0].abs() < crate::units::BASIS_AXIS_ALIGNMENT {
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
                let up = if normal[0].abs() < crate::units::BASIS_AXIS_ALIGNMENT {
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
        SurfaceGeom::Spherical(sphere) => {
            // Sphere face: triangular patch on the octahedral decomposition.
            // The face's loop vertices define a flat triangle; subdivide and project
            // onto the sphere surface to create a polygon grid, then emit quads/tris.
            let center_arr = [sphere.center.x, sphere.center.y, sphere.center.z];
            let r = sphere.radius;
            if loop_verts.len() < 3 {
                return;
            }
            let p0 = loop_verts[0];
            let p1 = loop_verts[1];
            let p2 = loop_verts[2];

            // Subdivide and project onto sphere
            let n_sub = 8; // subdivision level for boolean polygons
            for i in 0..n_sub {
                for j in 0..(n_sub - i) {
                    // Barycentric coords for the 4 corners of this sub-quad/tri
                    let bary = |ii: usize, jj: usize| -> [f64; 3] {
                        let u = (n_sub - ii - jj) as f64 / n_sub as f64;
                        let v = jj as f64 / n_sub as f64;
                        let w = ii as f64 / n_sub as f64;
                        let px = u * p0[0] + v * p1[0] + w * p2[0];
                        let py = u * p0[1] + v * p1[1] + w * p2[1];
                        let pz = u * p0[2] + v * p1[2] + w * p2[2];
                        // Project onto sphere
                        let dx = px - center_arr[0];
                        let dy = py - center_arr[1];
                        let dz = pz - center_arr[2];
                        let len = (dx * dx + dy * dy + dz * dz).sqrt();
                        let s = r / len;
                        [
                            center_arr[0] + dx * s,
                            center_arr[1] + dy * s,
                            center_arr[2] + dz * s,
                        ]
                    };

                    // Upper triangle: (i,j), (i,j+1), (i+1,j)
                    let v0 = bary(i, j);
                    let v1 = bary(i, j + 1);
                    let v2 = bary(i + 1, j);

                    let e1 = v3_sub(v1, v0);
                    let e2 = v3_sub(v2, v0);
                    let n_vec = v3_normalize(v3_cross(e1, e2));

                    polys.push(FacePoly {
                        verts: vec![v0, v1, v2],
                        normal: n_vec,
                        origin: v0,
                        surface_geom: Some(geom.clone()),
                    });

                    // Lower triangle: (i,j+1), (i+1,j+1), (i+1,j)
                    if j + 1 < n_sub - i {
                        let v3 = bary(i + 1, j + 1);
                        let e1b = v3_sub(v3, v1);
                        let e2b = v3_sub(v2, v1);
                        let n_vec_b = v3_normalize(v3_cross(e1b, e2b));

                        polys.push(FacePoly {
                            verts: vec![v1, v3, v2],
                            normal: n_vec_b,
                            origin: v1,
                            surface_geom: Some(geom.clone()),
                        });
                    }
                }
            }
        }
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
                if face_size > TAU_NORMALIZE && max_dist > face_size * CURVATURE_SUBDIV_THRESHOLD {
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
    // Sort for deterministic order (BTreeMap iteration is nondeterministic).
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

/// Generate planar face polygons approximating a revolve solid.
///
/// Converts a revolve (lateral swept faces + optional caps) into N planar quads
/// per lateral face plus cap N-gons. Mirrors `cylinder_to_face_polys` but uses
/// the revolve axis + angle instead of a fixed extrusion direction.
fn revolve_to_face_polys(
    rp: &RevolveParams,
    face_geometry: &BTreeMap<FaceIdx, SurfaceGeom>,
    n: usize,
) -> Vec<FacePoly> {
    let mut polys = Vec::new();

    // Collect all start profile vertices (v_a) for cap construction
    let mut start_profile_pts: Vec<[f64; 3]> = Vec::new();

    for &(face_idx, v_a, v_b) in &rp.lateral_faces {
        start_profile_pts.push(v_a);

        // Get surface geometry tag for this face
        let face_sg = face_geometry.get(&face_idx).cloned();

        // Generate N+1 rings of rotated vertex pairs
        let ring_count = if rp.full_revolution { n } else { n + 1 };
        let mut ring_a = Vec::with_capacity(ring_count);
        let mut ring_b = Vec::with_capacity(ring_count);
        for i in 0..ring_count {
            let theta = rp.angle_rad * (i as f64) / (n as f64);
            ring_a.push(rotate_point_around_axis(
                v_a,
                rp.axis_origin,
                rp.axis_dir,
                theta,
            ));
            ring_b.push(rotate_point_around_axis(
                v_b,
                rp.axis_origin,
                rp.axis_dir,
                theta,
            ));
        }

        // Emit quads connecting consecutive rings
        for i in 0..n {
            let j = if rp.full_revolution {
                (i + 1) % n
            } else {
                i + 1
            };
            // Quad: ring_a[i] → ring_b[i] → ring_b[j] → ring_a[j]
            let quad = vec![ring_a[i], ring_b[i], ring_b[j], ring_a[j]];
            let edge1 = v3_sub(ring_b[i], ring_a[i]);
            let edge2 = v3_sub(ring_a[j], ring_a[i]);
            let normal = v3_normalize(v3_cross(edge1, edge2));
            polys.push(FacePoly {
                verts: quad,
                normal,
                origin: ring_a[i],
                surface_geom: face_sg.clone(),
            });
        }
    }

    // Cap faces (partial revolve only)
    if !rp.full_revolution && !start_profile_pts.is_empty() {
        // Also collect v_b of last lateral face to close the profile polygon
        let mut profile_polygon: Vec<[f64; 3]> = start_profile_pts.clone();
        // Add the v_b vertices in reverse order to form a closed polygon
        for &(_fi, _va, vb) in rp.lateral_faces.iter().rev() {
            profile_polygon.push(vb);
        }

        // Compute solid centroid for outward normal orientation
        let centroid = rotate_point_around_axis(
            polygon_centroid(&profile_polygon),
            rp.axis_origin,
            rp.axis_dir,
            rp.angle_rad * 0.5,
        );

        // Start cap (at angle = 0): profile polygon as-is
        {
            let mut cap_verts = profile_polygon.clone();
            let newell = newell_normal_3d(&cap_verts);
            // Orient outward: if newell points toward centroid, flip
            let cap_center = polygon_centroid(&cap_verts);
            let to_centroid = v3_sub(centroid, cap_center);
            let normal = if v3_dot(newell, to_centroid) > 0.0 {
                cap_verts.reverse();
                [-newell[0], -newell[1], -newell[2]]
            } else {
                newell
            };
            polys.push(FacePoly {
                verts: cap_verts,
                normal,
                origin: cap_center,
                surface_geom: None,
            });
        }

        // End cap (at angle = angle_rad): rotate all profile vertices
        {
            let mut cap_verts: Vec<[f64; 3]> = profile_polygon
                .iter()
                .map(|&p| rotate_point_around_axis(p, rp.axis_origin, rp.axis_dir, rp.angle_rad))
                .collect();
            let newell = newell_normal_3d(&cap_verts);
            let cap_center = polygon_centroid(&cap_verts);
            let to_centroid = v3_sub(centroid, cap_center);
            let normal = if v3_dot(newell, to_centroid) > 0.0 {
                cap_verts.reverse();
                [-newell[0], -newell[1], -newell[2]]
            } else {
                newell
            };
            polys.push(FacePoly {
                verts: cap_verts,
                normal,
                origin: cap_center,
                surface_geom: None,
            });
        }
    }

    polys
}

/// Compute Newell normal for a polygon (unnormalized → normalized).
fn newell_normal_3d(verts: &[[f64; 3]]) -> [f64; 3] {
    let mut n = [0.0, 0.0, 0.0];
    let len = verts.len();
    for i in 0..len {
        let cur = verts[i];
        let nxt = verts[(i + 1) % len];
        n[0] += (cur[1] - nxt[1]) * (cur[2] + nxt[2]);
        n[1] += (cur[2] - nxt[2]) * (cur[0] + nxt[0]);
        n[2] += (cur[0] - nxt[0]) * (cur[1] + nxt[1]);
    }
    v3_normalize(n)
}

/// Extract face polys from a solid, using polygon approximation for cylinders.
///
/// For solids with `cylinder_params`, generates face polys from the cylinder
/// parameters (since the B-Rep topology only has 2 seam vertices).
/// For polygon solids, uses the standard B-Rep face extraction.
pub(super) fn extract_face_polys_general(solid: &WaffleSolid) -> Vec<FacePoly> {
    if let Some(ref cyl) = solid.cylinder_params {
        cylinder_to_face_polys(cyl, 32)
    } else if let Some(ref rp) = solid.revolve_params {
        revolve_to_face_polys(rp, &solid.face_geometry, 32)
    } else if solid.sphere_params.is_some() || solid.cone_params.is_some() {
        // Sphere faces need analytic subdivision (the B-Rep has only flat
        // octahedral triangles; generate_analytic_face_polys produces curved
        // polygon approximations projected onto the sphere surface).
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
        analytic_polys
    } else {
        // For polygon-soup solids (boolean results built from S-H clipping),
        // prefer cached face polys over B-Rep walk. The B-Rep walk via
        // collect_face_vertices may return partial results (faces with < 3
        // loop vertices are skipped), silently losing geometry from earlier
        // boolean operations. Cached polys are the exact face geometry that
        // produced the solid — more reliable for chained booleans.
        // Ref #24 Barton: bijective mesh extraction preserves original quality.
        if solid.is_polygon_soup {
            if let Some(ref cached) = solid.cached_face_polys {
                return cached.clone();
            }
        }
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
    // Compute tau early so AABB overlap and convexity check can use it.
    let (tau, tau_weld) = compute_adaptive_tau_weld(&a_faces, &b_faces);

    // Compute AABBs early for disjoint fast-path and per-face early-out.
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

    // Spec: aabb_disjoint_boolean_fastpath.md
    // Ref #24 Barton: spatial rejection for non-interfering geometry
    // AABB disjointness fast-path: if bounding boxes don't overlap (with tau
    // margin), skip S-H clipping entirely. Disjoint solids have no intersection.
    // Placed BEFORE face-product guard so disjoint high-face-count solids
    // (e.g., two large gears) don't hit the product limit and timeout.
    let aabb_disjoint = (0..3).any(|i| a_max[i] + tau < b_min[i] || b_max[i] + tau < a_min[i]);

    if aabb_disjoint {
        let result_faces: Vec<FacePoly> = match op {
            BoolOp::Union => {
                // Both face sets combined
                let mut combined = a_faces.to_vec();
                combined.extend(b_faces.iter().cloned());
                combined
            }
            BoolOp::Subtract => {
                // A minus nothing = A
                a_faces.to_vec()
            }
            BoolOp::Intersect => {
                // No shared volume = empty
                vec![]
            }
        };

        // Build B-Rep directly from face polys without S-H clipping.
        // Each face becomes a loop of half-edges with self-twin boundary edges.
        let mut arena = TopoArena::new();
        let solid_idx = arena.add_solid();
        let shell_idx = arena.add_shell(solid_idx);
        arena.solids[solid_idx.0].outer_shell = shell_idx;

        let mut face_map = BTreeMap::new();
        let mut edge_map = BTreeMap::new();
        let vertex_map = BTreeMap::new();
        let mut face_geometry = BTreeMap::new();
        let mut edge_geometry = BTreeMap::new();

        let mut first_face_set = false;
        for fp in &result_faces {
            if fp.verts.len() < 3 {
                continue;
            }
            let face_idx = arena.add_face(shell_idx);
            if !first_face_set {
                arena.shells[shell_idx.0].face = face_idx;
                first_face_set = true;
            }
            let loop_idx = arena.add_loop(face_idx);
            arena.faces[face_idx.0].outer_loop = loop_idx;

            let geom = fp.surface_geom.clone().unwrap_or(SurfaceGeom::Planar(
                crate::geometry::surface::Plane {
                    origin: Point3::from_array(fp.origin),
                    normal: Vector3::from_array(fp.normal),
                },
            ));
            face_geometry.insert(face_idx, geom);
            face_map.insert(id_alloc(), face_idx);

            let n = fp.verts.len();
            let first_he = HalfEdgeIdx(arena.half_edges.len());
            for i in 0..n {
                let v_idx = arena.add_vertex(fp.verts[i]);
                let he_idx = HalfEdgeIdx(first_he.0 + i);
                let next_he = HalfEdgeIdx(first_he.0 + ((i + 1) % n));
                let prev_he = HalfEdgeIdx(first_he.0 + ((i + n - 1) % n));
                let edge_idx = EdgeIdx(arena.edges.len());
                arena.edges.push(Edge { half_edge: he_idx });
                arena.half_edges.push(HalfEdge {
                    origin: v_idx,
                    edge: edge_idx,
                    twin: he_idx, // self-twin (boundary)
                    next: next_he,
                    prev: prev_he,
                    loop_: loop_idx,
                });
                arena.vertices[v_idx.0].half_edge = Some(he_idx);

                let p0 = fp.verts[i];
                let p1 = fp.verts[(i + 1) % n];
                let dir = v3_sub(p1, p0);
                edge_geometry.insert(
                    edge_idx,
                    CurveGeom::Linear(Line3D {
                        origin: Point3::from_array(p0),
                        direction: Vector3::from_array(dir),
                    }),
                );
                edge_map.insert(id_alloc(), edge_idx);
            }
            arena.loops[loop_idx.0].half_edge = first_he;
        }

        let cached = if result_faces.is_empty() {
            None
        } else {
            Some(result_faces)
        };

        return Ok(BooleanResult {
            arena,
            face_map,
            edge_map,
            vertex_map,
            face_geometry,
            edge_geometry,
            cached_face_polys: cached,
        });
    }

    // Guard against pathological face counts: O(n*m) classification becomes
    // too expensive when both solids have many faces (e.g., revolve(gear) × gear).
    // Placed AFTER disjoint fast-path so disjoint high-face-count solids still succeed.
    let total_faces = a_faces.len() + b_faces.len();
    if total_faces > 8000 {
        return Err(KernelError::NotSupported {
            operation: format!(
                "polygon boolean: {} total faces exceeds limit (8000)",
                total_faces
            ),
        });
    }

    // Convexity check: polygon-approximated cylinders have 34 faces (32 side
    // quads + 2 caps) but are geometrically convex. The old heuristic
    // (len <= 12) misclassified them as non-convex, causing
    // classify_face_nonconvex to produce catastrophically broken results.
    //
    // Use geometric convexity test only when at least one solid is simple
    // (<=12 faces, e.g. a box). When BOTH solids are large (e.g. two
    // cylinders), S-H clipping against many planes accumulates numerical
    // error causing empty-mesh results. Fall back to non-convex path for
    // the both-large case to preserve progressive-splitting robustness.
    let one_solid_simple = a_faces.len() <= 12 || b_faces.len() <= 12;
    let a_convex = if one_solid_simple {
        is_face_set_convex(&a_faces, tau)
    } else {
        a_faces.len() <= 12
    };
    let b_convex = if one_solid_simple {
        is_face_set_convex(&b_faces, tau)
    } else {
        b_faces.len() <= 12
    };

    // Product-based guard: O(A*B) face classification is too expensive
    // when both solids are non-convex (e.g., two gears with ~200 faces each).
    // Use AABB-filtered effective product: most face pairs are spatially disjoint
    // in multi-step operations, so the raw product vastly overestimates cost.
    let product = a_faces.len() * b_faces.len();
    if product > 50000 && !a_convex && !b_convex {
        let effective = count_aabb_overlapping_pairs(&a_faces, &b_faces, tau);
        if effective > 50000 {
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
            face_map: BTreeMap::new(),
            edge_map: BTreeMap::new(),
            vertex_map: BTreeMap::new(),
            face_geometry: BTreeMap::new(),
            edge_geometry: BTreeMap::new(),
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
    use std::collections::HashMap;

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
            (area - 0.5).abs() < 1e-9,
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
        let up = if dir[1].abs() < crate::units::BASIS_AXIS_ALIGNMENT {
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
        let handle = result.expect("step shape union should succeed");
        let mesh = kernel.tessellate(&handle, 0.01).unwrap();
        // Step shape volume: 10×10×5 + 5×10×5 = 500 + 250 = 750
        // Compute volume via divergence theorem (signed tetrahedra)
        let mut vol = 0.0_f64;
        let n_tris = mesh.indices.len() / 3;
        for i in 0..n_tris {
            let idx = |j: usize| {
                let k = mesh.indices[i * 3 + j] as usize;
                [
                    mesh.vertices[k * 3] as f64,
                    mesh.vertices[k * 3 + 1] as f64,
                    mesh.vertices[k * 3 + 2] as f64,
                ]
            };
            let (v0, v1, v2) = (idx(0), idx(1), idx(2));
            vol += v0[0] * (v1[1] * v2[2] - v1[2] * v2[1])
                - v0[1] * (v1[0] * v2[2] - v1[2] * v2[0])
                + v0[2] * (v1[0] * v2[1] - v1[1] * v2[0]);
        }
        vol = vol.abs() / 6.0;
        let expected = 750.0;
        assert!(
            (vol - expected).abs() / expected < 0.05,
            "Step-shape union volume should be ~{}, got {} ({}% error)",
            expected,
            vol,
            ((vol - expected).abs() / expected * 100.0)
        );
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

    // NOTE: AABB collapse in partial cyl-box boolean tessellation is a known issue.
    // See specs/boolean_tessellation_routing.md for root cause analysis.
    // The bounded tessellation path generates cylindrical face vertices only at
    // z_min/z_max (2 rows), so all vertices lie on AABB end-cap planes.
    // Fixing this requires either:
    // 1. Axial subdivision in tessellate_cylindrical_face_bounded (complex vertex sharing)
    // 2. Setting polygon_soup = true for SSI fallback (causes non-manifold regression)
    // Deferred to a future session.

    /// Verify that the face-product guard allows effective products up to 50000.
    ///
    /// Two non-convex solids with many faces that fully overlap spatially produce
    /// an effective AABB product exceeding 5000 but under 50000. The current limit
    /// of 5000 rejects this; after raising to 50000 it should succeed.
    #[test]
    fn face_product_limit_raised_to_50000() {
        // Strategy: create faces whose AABBs all cover the same large region, so
        // every pair overlaps. We achieve this by making each face a triangle that
        // spans from one corner to the opposite corner of the [0,1]³ cube, giving
        // each face an AABB of [0,1]³. This guarantees EVERY face pair has
        // overlapping AABBs, making effective product = raw product.
        //
        // Each solid is a collection of triangular faces radiating from a central
        // spine, like a folded fan. The vertices alternate between (0,0,0) and
        // (1,1,1), with the third vertex walking along the cube's surface. This
        // makes every face non-convex when considered as a set (the fold angles
        // break convexity). Each face's AABB is exactly [0,1]³ since it contains
        // both corners.

        fn make_nonconvex_fan_solid(n_faces: usize) -> Vec<FacePoly> {
            let mut faces = Vec::new();
            let corner_a = [0.0, 0.0, 0.0];
            let corner_b = [1.0, 1.0, 1.0];

            for i in 0..n_faces {
                let t = i as f64 / n_faces as f64;
                // Walk the third vertex around the cube surface
                // This creates fan-like faces all spanning [0,1]³
                let (vx, vy, vz) = if t < 0.25 {
                    let s = t * 4.0;
                    (s, 0.0, 0.5) // bottom edge, x varies
                } else if t < 0.5 {
                    let s = (t - 0.25) * 4.0;
                    (1.0, s, 0.5) // right edge, y varies
                } else if t < 0.75 {
                    let s = (t - 0.5) * 4.0;
                    (1.0 - s, 1.0, 0.5) // top edge, x varies
                } else {
                    let s = (t - 0.75) * 4.0;
                    (0.0, 1.0 - s, 0.5) // left edge, y varies
                };

                let v3 = [vx, vy, vz];
                // Compute approximate outward normal
                let ab = [
                    corner_b[0] - corner_a[0],
                    corner_b[1] - corner_a[1],
                    corner_b[2] - corner_a[2],
                ];
                let ac = [
                    v3[0] - corner_a[0],
                    v3[1] - corner_a[1],
                    v3[2] - corner_a[2],
                ];
                let nx = ab[1] * ac[2] - ab[2] * ac[1];
                let ny = ab[2] * ac[0] - ab[0] * ac[2];
                let nz = ab[0] * ac[1] - ab[1] * ac[0];
                let len = (nx * nx + ny * ny + nz * nz).sqrt().max(1e-12);

                faces.push(FacePoly {
                    verts: vec![corner_a, corner_b, v3],
                    normal: [nx / len, ny / len, nz / len],
                    origin: corner_a,
                    surface_geom: None,
                });
            }
            faces
        }

        // 80 faces × 80 faces = 6400 effective pairs (all overlap since every
        // face AABB is [0,1]³). Well above 5000, well below 50000.
        let a_faces = make_nonconvex_fan_solid(80);
        let b_faces = make_nonconvex_fan_solid(80);

        // Both solids have >12 faces → classified as non-convex
        assert!(a_faces.len() > 12);
        assert!(b_faces.len() > 12);

        let tau = 1e-7;
        let effective = count_aabb_overlapping_pairs(&a_faces, &b_faces, tau);
        let raw_product = a_faces.len() * b_faces.len();
        eprintln!(
            "face_product_limit test: A={} faces, B={} faces, raw={}, effective={}",
            a_faces.len(),
            b_faces.len(),
            raw_product,
            effective
        );
        assert!(
            effective > 5000,
            "Effective product {} should exceed 5000 (current limit). A={}, B={}, raw={}",
            effective,
            a_faces.len(),
            b_faces.len(),
            raw_product
        );
        assert!(
            effective < 50000,
            "Effective product {} should be under 50000 (proposed limit)",
            effective
        );

        // The boolean should succeed with the raised limit (50000).
        // Currently fails because the limit is 5000.
        let mut id_counter = 0u64;
        let mut id_alloc = || {
            id_counter += 1;
            id_counter
        };

        let result = boolean_op_from_polys(a_faces, b_faces, BoolOp::Union, &mut id_alloc);
        assert!(
            result.is_ok(),
            "Boolean with effective product ~{} should succeed (limit should be 50000, not 5000): {:?}",
            effective,
            result.err()
        );
    }

    // ── AABB Disjoint Fast-Path Tests ──────────────────────────────────
    mod disjoint_fastpath_tests {
        use super::*;

        /// Build 6 face polygons for an axis-aligned box from min to max.
        /// Each face has 4 vertices in CCW order viewed from outside,
        /// outward-pointing normal, and origin at face center.
        fn make_box_face_polys(min: [f64; 3], max: [f64; 3]) -> Vec<FacePoly> {
            let [x0, y0, z0] = min;
            let [x1, y1, z1] = max;

            vec![
                // +X face
                FacePoly {
                    verts: vec![[x1, y0, z0], [x1, y1, z0], [x1, y1, z1], [x1, y0, z1]],
                    normal: [1.0, 0.0, 0.0],
                    origin: [x1, (y0 + y1) / 2.0, (z0 + z1) / 2.0],
                    surface_geom: None,
                },
                // -X face
                FacePoly {
                    verts: vec![[x0, y0, z1], [x0, y1, z1], [x0, y1, z0], [x0, y0, z0]],
                    normal: [-1.0, 0.0, 0.0],
                    origin: [x0, (y0 + y1) / 2.0, (z0 + z1) / 2.0],
                    surface_geom: None,
                },
                // +Y face
                FacePoly {
                    verts: vec![[x0, y1, z0], [x1, y1, z0], [x1, y1, z1], [x0, y1, z1]],
                    normal: [0.0, 1.0, 0.0],
                    origin: [(x0 + x1) / 2.0, y1, (z0 + z1) / 2.0],
                    surface_geom: None,
                },
                // -Y face
                FacePoly {
                    verts: vec![[x0, y0, z1], [x1, y0, z1], [x1, y0, z0], [x0, y0, z0]],
                    normal: [0.0, -1.0, 0.0],
                    origin: [(x0 + x1) / 2.0, y0, (z0 + z1) / 2.0],
                    surface_geom: None,
                },
                // +Z face
                FacePoly {
                    verts: vec![[x0, y0, z1], [x1, y0, z1], [x1, y1, z1], [x0, y1, z1]],
                    normal: [0.0, 0.0, 1.0],
                    origin: [(x0 + x1) / 2.0, (y0 + y1) / 2.0, z1],
                    surface_geom: None,
                },
                // -Z face
                FacePoly {
                    verts: vec![[x0, y1, z0], [x1, y1, z0], [x1, y0, z0], [x0, y0, z0]],
                    normal: [0.0, 0.0, -1.0],
                    origin: [(x0 + x1) / 2.0, (y0 + y1) / 2.0, z0],
                    surface_geom: None,
                },
            ]
        }

        /// Build a polygon-approximated cylinder (N-sided prism) centered at
        /// (cx, cy) with given radius, extending from z=z0 to z=z1.
        fn make_prism_face_polys(
            cx: f64,
            cy: f64,
            radius: f64,
            z0: f64,
            z1: f64,
            n_sides: usize,
        ) -> Vec<FacePoly> {
            let mut faces = Vec::new();
            let tau = std::f64::consts::TAU;

            // Bottom cap (normal -Z)
            let mut bottom_verts = Vec::new();
            for i in (0..n_sides).rev() {
                let angle = tau * i as f64 / n_sides as f64;
                bottom_verts.push([cx + radius * angle.cos(), cy + radius * angle.sin(), z0]);
            }
            faces.push(FacePoly {
                verts: bottom_verts,
                normal: [0.0, 0.0, -1.0],
                origin: [cx, cy, z0],
                surface_geom: None,
            });

            // Top cap (normal +Z)
            let mut top_verts = Vec::new();
            for i in 0..n_sides {
                let angle = tau * i as f64 / n_sides as f64;
                top_verts.push([cx + radius * angle.cos(), cy + radius * angle.sin(), z1]);
            }
            faces.push(FacePoly {
                verts: top_verts,
                normal: [0.0, 0.0, 1.0],
                origin: [cx, cy, z1],
                surface_geom: None,
            });

            // Side quads
            for i in 0..n_sides {
                let a0 = tau * i as f64 / n_sides as f64;
                let a1 = tau * ((i + 1) % n_sides) as f64 / n_sides as f64;
                let cos0 = a0.cos();
                let sin0 = a0.sin();
                let cos1 = a1.cos();
                let sin1 = a1.sin();

                let p0 = [cx + radius * cos0, cy + radius * sin0, z0];
                let p1 = [cx + radius * cos1, cy + radius * sin1, z0];
                let p2 = [cx + radius * cos1, cy + radius * sin1, z1];
                let p3 = [cx + radius * cos0, cy + radius * sin0, z1];

                let mid_angle = (a0 + a1) / 2.0;
                let nx = mid_angle.cos();
                let ny = mid_angle.sin();

                faces.push(FacePoly {
                    verts: vec![p0, p1, p2, p3],
                    normal: [nx, ny, 0.0],
                    origin: [cx + radius * nx, cy + radius * ny, (z0 + z1) / 2.0],
                    surface_geom: None,
                });
            }

            faces
        }

        fn new_id_alloc() -> impl FnMut() -> u64 {
            let mut counter = 0u64;
            move || {
                counter += 1;
                counter
            }
        }

        #[test]
        fn disjoint_union_preserves_all_faces() {
            // Two unit boxes separated by a gap: box A at [0,1]^3, box B at [5,6]^3
            let a_faces = make_box_face_polys([0.0, 0.0, 0.0], [1.0, 1.0, 1.0]);
            let b_faces = make_box_face_polys([5.0, 5.0, 5.0], [6.0, 6.0, 6.0]);

            assert_eq!(a_faces.len(), 6);
            assert_eq!(b_faces.len(), 6);

            let mut id_alloc = new_id_alloc();
            let result = boolean_op_from_polys(a_faces, b_faces, BoolOp::Union, &mut id_alloc)
                .expect("disjoint union should succeed");

            // Disjoint union must preserve ALL faces from both operands: 6 + 6 = 12
            let face_count = result.arena.faces.len();
            assert_eq!(
                face_count, 12,
                "Disjoint union should produce exactly 12 faces (6+6), got {}",
                face_count
            );
        }

        #[test]
        fn disjoint_subtract_preserves_operand_a() {
            // A at [0,1]^3, B at [5,6]^3 — disjoint, so A - B = A
            let a_faces = make_box_face_polys([0.0, 0.0, 0.0], [1.0, 1.0, 1.0]);
            let b_faces = make_box_face_polys([5.0, 5.0, 5.0], [6.0, 6.0, 6.0]);

            let mut id_alloc = new_id_alloc();
            let result = boolean_op_from_polys(a_faces, b_faces, BoolOp::Subtract, &mut id_alloc)
                .expect("disjoint subtract should succeed");

            // A - B where B is disjoint should produce exactly A's faces
            let face_count = result.arena.faces.len();
            assert_eq!(
                face_count, 6,
                "Disjoint subtract should produce exactly 6 faces (operand A only), got {}",
                face_count
            );
        }

        #[test]
        fn disjoint_intersect_produces_empty() {
            // A at [0,1]^3, B at [5,6]^3 — disjoint, so A ∩ B = empty
            let a_faces = make_box_face_polys([0.0, 0.0, 0.0], [1.0, 1.0, 1.0]);
            let b_faces = make_box_face_polys([5.0, 5.0, 5.0], [6.0, 6.0, 6.0]);

            let mut id_alloc = new_id_alloc();
            let result = boolean_op_from_polys(a_faces, b_faces, BoolOp::Intersect, &mut id_alloc);

            // Disjoint intersection should produce an empty result (0 faces).
            // This is acceptable as either Ok with 0 faces or an Err indicating empty.
            match result {
                Ok(res) => {
                    let face_count = res.arena.faces.len();
                    assert_eq!(
                        face_count, 0,
                        "Disjoint intersect should produce 0 faces, got {}",
                        face_count
                    );
                }
                Err(_) => {
                    // An error (e.g., empty result) is also acceptable for disjoint intersect
                }
            }
        }

        #[test]
        fn disjoint_union_volume_equals_sum() {
            // Two disjoint unit boxes: each has volume 1.0, union volume should be ~2.0
            let a_faces = make_box_face_polys([0.0, 0.0, 0.0], [1.0, 1.0, 1.0]);
            let b_faces = make_box_face_polys([5.0, 5.0, 5.0], [6.0, 6.0, 6.0]);

            let mut id_alloc = new_id_alloc();
            let result = boolean_op_from_polys(a_faces, b_faces, BoolOp::Union, &mut id_alloc)
                .expect("disjoint union should succeed");

            // Verify face count as a proxy for volume preservation
            // Each unit box has 6 faces; union of disjoint should have 12
            let face_count = result.arena.faces.len();
            assert_eq!(
                face_count, 12,
                "Disjoint union should have 12 faces for volume preservation (6+6), got {}",
                face_count
            );

            // Also verify cached_face_polys if available
            if let Some(ref polys) = result.cached_face_polys {
                assert_eq!(
                    polys.len(),
                    12,
                    "Cached face polys should have 12 entries for disjoint union, got {}",
                    polys.len()
                );

                // Check total area as volume proxy: each unit box has surface area 6.0
                let total_area: f64 = polys.iter().map(|fp| polygon_area_3d(&fp.verts)).sum();
                assert!(
                    (total_area - 12.0).abs() < 1.2, // 10% tolerance
                    "Total surface area of disjoint union should be ~12.0 (2 unit boxes), got {}",
                    total_area
                );
            }
        }

        #[test]
        fn touching_boxes_use_full_pipeline() {
            // Two boxes sharing a face: A=[0,1]^3, B=[1,2] x [0,1] x [0,1]
            // These are NOT disjoint — the shared face should be eliminated in union.
            let a_faces = make_box_face_polys([0.0, 0.0, 0.0], [1.0, 1.0, 1.0]);
            let b_faces = make_box_face_polys([1.0, 0.0, 0.0], [2.0, 1.0, 1.0]);

            let mut id_alloc = new_id_alloc();
            let result = boolean_op_from_polys(a_faces, b_faces, BoolOp::Union, &mut id_alloc)
                .expect("touching union should succeed");

            // The shared face (A's +X and B's -X) should be eliminated,
            // so union should produce fewer than 12 faces.
            let face_count = result.arena.faces.len();
            assert!(
                face_count < 12,
                "Touching boxes union should eliminate shared face: expected < 12 faces, got {}",
                face_count
            );
        }

        // ── Adversarial tests ──────────────────────────────────────────

        #[test]
        fn adv_near_touching_gap_at_tau_boundary() {
            // Two boxes with a gap of TAU_MODEL (1e-7).
            // At unit scale the adaptive tau used for AABB disjointness is
            // much smaller than 1e-7, so the fast-path will fire. Either
            // way the union must succeed and produce a valid result.
            let gap = 1e-7; // TAU_MODEL
            let a = make_box_face_polys([0.0, 0.0, 0.0], [1.0, 1.0, 1.0]);
            let b = make_box_face_polys([1.0 + gap, 0.0, 0.0], [2.0 + gap, 1.0, 1.0]);
            let mut id = new_id_alloc();
            let result = boolean_op_from_polys(a, b, BoolOp::Union, &mut id);
            assert!(
                result.is_ok(),
                "Near-touching union must succeed: {:?}",
                result.err()
            );
            let res = result.unwrap();
            // Must have at least 10 faces (12 if fast-path, 10 if full pipeline merges shared face)
            assert!(
                res.arena.faces.len() >= 10,
                "Near-touching union should preserve most faces, got {}",
                res.arena.faces.len()
            );
        }

        #[test]
        fn adv_disjoint_along_each_axis() {
            // Disjoint along X only
            let a_x = make_box_face_polys([0.0, 0.0, 0.0], [1.0, 1.0, 1.0]);
            let b_x = make_box_face_polys([5.0, 0.0, 0.0], [6.0, 1.0, 1.0]);
            let mut id = new_id_alloc();
            let r = boolean_op_from_polys(a_x, b_x, BoolOp::Union, &mut id).unwrap();
            assert_eq!(
                r.arena.faces.len(),
                12,
                "Disjoint along X: expected 12 faces"
            );

            // Disjoint along Y only
            let a_y = make_box_face_polys([0.0, 0.0, 0.0], [1.0, 1.0, 1.0]);
            let b_y = make_box_face_polys([0.0, 5.0, 0.0], [1.0, 6.0, 1.0]);
            let mut id = new_id_alloc();
            let r = boolean_op_from_polys(a_y, b_y, BoolOp::Union, &mut id).unwrap();
            assert_eq!(
                r.arena.faces.len(),
                12,
                "Disjoint along Y: expected 12 faces"
            );

            // Disjoint along Z only
            let a_z = make_box_face_polys([0.0, 0.0, 0.0], [1.0, 1.0, 1.0]);
            let b_z = make_box_face_polys([0.0, 0.0, 5.0], [1.0, 1.0, 6.0]);
            let mut id = new_id_alloc();
            let r = boolean_op_from_polys(a_z, b_z, BoolOp::Union, &mut id).unwrap();
            assert_eq!(
                r.arena.faces.len(),
                12,
                "Disjoint along Z: expected 12 faces"
            );
        }

        #[test]
        fn adv_disjoint_micro_scale() {
            // Two tiny boxes at 1e-4 scale, separated by 1e-3
            let a = make_box_face_polys([0.0, 0.0, 0.0], [1e-4, 1e-4, 1e-4]);
            let b = make_box_face_polys([1e-3, 0.0, 0.0], [1e-3 + 1e-4, 1e-4, 1e-4]);
            let mut id = new_id_alloc();
            let r = boolean_op_from_polys(a, b, BoolOp::Union, &mut id).unwrap();
            assert_eq!(
                r.arena.faces.len(),
                12,
                "Micro-scale disjoint union should have 12 faces, got {}",
                r.arena.faces.len()
            );
        }

        #[test]
        fn adv_disjoint_macro_scale() {
            // Two large boxes at 1e3 scale, separated by 1e4
            let a = make_box_face_polys([0.0, 0.0, 0.0], [1e3, 1e3, 1e3]);
            let b = make_box_face_polys([1e4, 0.0, 0.0], [1e4 + 1e3, 1e3, 1e3]);
            let mut id = new_id_alloc();
            let r = boolean_op_from_polys(a, b, BoolOp::Union, &mut id).unwrap();
            assert_eq!(
                r.arena.faces.len(),
                12,
                "Macro-scale disjoint union should have 12 faces, got {}",
                r.arena.faces.len()
            );
        }

        #[test]
        fn adv_disjoint_subtract_identity() {
            // A - B where B is disjoint should equal A (same face count)
            let a = make_box_face_polys([0.0, 0.0, 0.0], [1.0, 1.0, 1.0]);
            let a_alone_count = a.len(); // 6
            let b = make_box_face_polys([10.0, 10.0, 10.0], [11.0, 11.0, 11.0]);
            let mut id = new_id_alloc();
            let r = boolean_op_from_polys(a, b, BoolOp::Subtract, &mut id).unwrap();
            assert_eq!(
                r.arena.faces.len(),
                a_alone_count,
                "Disjoint subtract should preserve A's {} faces, got {}",
                a_alone_count,
                r.arena.faces.len()
            );
        }

        #[test]
        fn adv_mutation_sanity_check() {
            // Overlapping boxes: the full pipeline should merge shared volume,
            // producing fewer than 12 faces. This confirms the fast-path
            // (12 faces for disjoint) is actually different from the overlap case.
            let a = make_box_face_polys([0.0, 0.0, 0.0], [1.0, 1.0, 1.0]);
            let b = make_box_face_polys([0.5, 0.0, 0.0], [1.5, 1.0, 1.0]);
            let mut id = new_id_alloc();
            let r = boolean_op_from_polys(a, b, BoolOp::Union, &mut id).unwrap();
            assert!(
                r.arena.faces.len() < 12,
                "Overlapping union should produce fewer than 12 faces (shared volume merged), got {}",
                r.arena.faces.len()
            );
        }

        #[test]
        fn disjoint_union_no_timeout_large_solids() {
            // Two 64-sided prisms far apart. With AABB fast-path, union should be
            // nearly instant. Without it, the O(n*m) classification is slow.
            // 66 faces each => 132 total. 66*66 = 4356 face pairs in S-H pipeline.
            let a_faces = make_prism_face_polys(0.0, 0.0, 1.0, 0.0, 1.0, 64);
            let b_faces = make_prism_face_polys(100.0, 100.0, 1.0, 0.0, 1.0, 64);

            // Each prism has 66 faces (2 caps + 64 side quads)
            assert_eq!(a_faces.len(), 66, "Prism A should have 66 faces");
            assert_eq!(b_faces.len(), 66, "Prism B should have 66 faces");

            let start = std::time::Instant::now();
            let mut id_alloc = new_id_alloc();
            let result = boolean_op_from_polys(a_faces, b_faces, BoolOp::Union, &mut id_alloc);
            let elapsed = start.elapsed();

            // With the AABB fast-path, disjoint solids should complete in well
            // under 1 second. Without it, 66*66 = 4356 face pairs go through
            // S-H clipping which can be slow and produce incorrect results.
            assert!(
                elapsed.as_secs_f64() < 1.0,
                "Disjoint union of 66-face prisms should complete in < 1s (AABB fast-path), took {:.2}s",
                elapsed.as_secs_f64()
            );

            let result = result.expect("disjoint union of prisms should succeed");

            // Should preserve all faces: 66 + 66 = 132
            let face_count = result.arena.faces.len();
            assert_eq!(
                face_count, 132,
                "Disjoint union of two 66-face prisms should produce 132 faces, got {}",
                face_count
            );
        }
    }
}
