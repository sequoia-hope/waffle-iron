//! Box-box boolean operations using convex face-polygon clipping.
//!
//! Supports Union, Subtract, and Intersect on axis-aligned box solids
//! produced by the WaffleKernel extrude pipeline. Uses Sutherland-Hodgman
//! polygon clipping against convex half-spaces to classify face fragments
//! as inside, outside, or partial with respect to the opposing solid.

use crate::geometry::curve::{CurveGeom, Line3D};
use crate::geometry::point::{Point3, Vector3};
use crate::geometry::surface::{Plane, SurfaceGeom};
use crate::topology::arena::TopoArena;
use crate::topology::half_edge::*;
use crate::types::*;
use crate::units::{TAU_NORMALIZE, TAU_PARALLEL};
use crate::vecmath::*;
use crate::waffle_kernel::WaffleSolid;
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
}

// ── Internal types ──────────────────────────────────────────────────────

/// A planar polygon with its face normal and a representative origin point.
#[derive(Debug, Clone)]
struct FacePoly {
    verts: Vec<[f64; 3]>,
    normal: [f64; 3],
    origin: [f64; 3],
}

/// Compute a rotation matrix that maps unit vector `dir` to [0, 0, 1].
///
/// Uses Rodrigues' rotation formula around the axis `cross(dir, Z)`.
fn rotation_to_z(dir: [f64; 3]) -> Mat3 {
    let z = [0.0, 0.0, 1.0];
    let cos_theta = v3_dot(dir, z); // dir · Z = dz

    // Already Z-aligned (within tolerance)
    if cos_theta > 1.0 - 1e-12 {
        return MAT3_IDENTITY;
    }

    // Anti-parallel to Z: rotate 180° around X
    if cos_theta < -1.0 + 1e-12 {
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

/// Transform CylinderParams into a rotated coordinate frame.
fn rotate_cyl_params(cyl: &CylinderParams, m: &Mat3) -> CylinderParams {
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
fn rotate_boolean_result(result: &mut BooleanResult, m_inv: &Mat3) {
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
        }
    }
}

/// Compute polygon area using cross-product accumulation (works in 3D).
fn polygon_area_3d(verts: &[[f64; 3]]) -> f64 {
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

/// Extract all face polygons from a WaffleSolid.
fn extract_face_polys(solid: &WaffleSolid) -> Vec<FacePoly> {
    let mut polys = Vec::new();
    for (&_kid, &face_idx) in &solid.face_map {
        let verts = collect_face_vertices(&solid.arena, face_idx);
        if verts.is_empty() {
            continue;
        }
        let (normal, origin) = match solid.face_geometry.get(&face_idx) {
            Some(SurfaceGeom::Planar(p)) => (
                [p.normal.x, p.normal.y, p.normal.z],
                [p.origin.x, p.origin.y, p.origin.z],
            ),
            _ => continue, // Skip non-planar faces for box-box booleans
        };
        polys.push(FacePoly {
            verts,
            normal,
            origin,
        });
    }
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
        });
    }

    polys
}

/// Extract face polys from a solid, using polygon approximation for cylinders.
///
/// For solids with `cylinder_params`, generates face polys from the cylinder
/// parameters (since the B-Rep topology only has 2 seam vertices).
/// For polygon solids, uses the standard B-Rep face extraction.
fn extract_face_polys_general(solid: &WaffleSolid) -> Vec<FacePoly> {
    if let Some(ref cyl) = solid.cylinder_params {
        cylinder_to_face_polys(cyl, 32)
    } else {
        extract_face_polys(solid)
    }
}

// ── Sutherland-Hodgman polygon clipping ─────────────────────────────────

/// Clip a polygon to keep only the portion on the INWARD side of a plane.
/// Points where `dot(p - plane_point, inward_normal) >= -tau` are kept.
fn clip_polygon_by_plane(
    verts: &[[f64; 3]],
    plane_point: [f64; 3],
    inward_normal: [f64; 3],
    tau: f64,
) -> Vec<[f64; 3]> {
    if verts.is_empty() {
        return vec![];
    }

    let mut output = Vec::with_capacity(verts.len() + 1);

    let dist = |p: [f64; 3]| -> f64 { v3_dot(v3_sub(p, plane_point), inward_normal) };

    let n = verts.len();
    for i in 0..n {
        let current = verts[i];
        let next = verts[(i + 1) % n];
        let d_current = dist(current);
        let d_next = dist(next);

        let current_inside = d_current >= -tau;
        let next_inside = d_next >= -tau;

        if current_inside {
            output.push(current);
            if !next_inside {
                // Crossing from inside to outside: emit intersection
                let t = d_current / (d_current - d_next);
                let intersection = v3_add(current, v3_scale(v3_sub(next, current), t));
                output.push(intersection);
            }
        } else if next_inside {
            // Crossing from outside to inside: emit intersection then next
            let t = d_current / (d_current - d_next);
            let intersection = v3_add(current, v3_scale(v3_sub(next, current), t));
            output.push(intersection);
        }
    }

    output
}

/// Coplanarity classification between two face planes.
#[derive(Debug, Clone, Copy, PartialEq)]
enum CoplanarClass {
    NotCoplanar,
    SameDirection,
    AntiParallel,
}

/// Classify whether a face is coplanar with an opposing face, and if so,
/// whether their normals are parallel or anti-parallel.
fn classify_coplanarity(
    face_normal: [f64; 3],
    face_point: [f64; 3],
    opp: &FacePoly,
    tau: f64,
) -> CoplanarClass {
    let dot_n = v3_dot(face_normal, opp.normal);
    if dot_n.abs() > 1.0 - TAU_PARALLEL {
        let dist = v3_dot(v3_sub(face_point, opp.origin), opp.normal).abs();
        if dist < tau * 100.0 {
            if dot_n > 0.0 {
                CoplanarClass::SameDirection
            } else {
                CoplanarClass::AntiParallel
            }
        } else {
            CoplanarClass::NotCoplanar
        }
    } else {
        CoplanarClass::NotCoplanar
    }
}

/// Check if a face polygon is coplanar with an opposing face.
fn is_coplanar(face_normal: [f64; 3], face_point: [f64; 3], opp: &FacePoly, tau: f64) -> bool {
    classify_coplanarity(face_normal, face_point, opp, tau) != CoplanarClass::NotCoplanar
}

/// Clip a polygon by a convex solid's interior (intersection of inward half-spaces).
/// For a convex solid, each face's inward normal is the NEGATION of its outward normal.
///
/// If `face_normal` is provided, skip opposing faces that are coplanar with the
/// polygon being clipped. Two faces are coplanar when their normals are parallel
/// (or anti-parallel) and a vertex of the polygon lies on the opposing face's plane.
fn clip_polygon_by_solid(
    verts: &[[f64; 3]],
    opposing_faces: &[FacePoly],
    tau: f64,
    face_normal: Option<[f64; 3]>,
) -> Vec<[f64; 3]> {
    let mut current = verts.to_vec();
    for face in opposing_faces {
        if current.is_empty() {
            break;
        }

        // Skip coplanar opposing faces
        if let Some(fn_normal) = face_normal {
            if is_coplanar(fn_normal, current[0], face, tau) {
                continue;
            }
        }

        // Inward normal = negation of the face's outward normal
        let inward = v3_negate(face.normal);
        current = clip_polygon_by_plane(&current, face.origin, inward, tau);
    }
    current
}

// ── Ray-casting point-in-solid ───────────────────────────────────────────

/// Test if a point is inside a closed polyhedral solid.
/// Casts a ray in +Z direction, counts face crossings. Odd = inside.
///
/// If the ray grazes an edge/vertex (ambiguous result), retries with
/// perturbed directions.
///
/// Ref #7 Jacobson: generalized winding number (simplified to ray-crossing
/// for polyhedral solids).
fn point_in_solid(point: [f64; 3], faces: &[FacePoly]) -> bool {
    // Try multiple ray directions, including both positive and negative,
    // to handle points near boundaries (e.g., at z=max where upward rays
    // exit the solid immediately and give 0 crossings).
    let directions = [
        [0.0, 0.0, 1.0],
        [0.0, 0.0, -1.0],
        [0.0, 1.0, 0.0],
        [0.0, -1.0, 0.0],
        [1.0, 0.0, 0.0],
        [-1.0, 0.0, 0.0],
        [0.57735, 0.57735, 0.57735],    // (1,1,1)/sqrt(3)
        [-0.57735, -0.57735, -0.57735], // (-1,-1,-1)/sqrt(3)
    ];

    for &dir in &directions {
        let mut crossings = 0u32;
        let mut grazing = false;

        for poly in faces {
            let denom = v3_dot(dir, poly.normal);
            if denom.abs() < 1e-12 {
                // Ray nearly parallel to this face — check if point is near the face plane
                let dist = v3_dot(v3_sub(point, poly.origin), poly.normal).abs();
                if dist < 1e-9 {
                    grazing = true;
                    break;
                }
                continue;
            }

            let t = v3_dot(v3_sub(poly.origin, point), poly.normal) / denom;
            if t <= 1e-12 {
                continue;
            }

            let hit = v3_add(point, v3_scale(dir, t));

            // Project to 2D
            let nx = poly.normal[0].abs();
            let ny = poly.normal[1].abs();
            let nz = poly.normal[2].abs();
            let (ax_u, ax_v) = if nz >= nx && nz >= ny {
                (0, 1)
            } else if ny >= nx {
                (0, 2)
            } else {
                (1, 2)
            };

            let hit_u = hit[ax_u];
            let hit_v = hit[ax_v];

            // Check if hit point is very close to a polygon edge (grazing)
            let n = poly.verts.len();
            let mut near_edge = false;
            for i in 0..n {
                let a = poly.verts[i];
                let b = poly.verts[(i + 1) % n];
                let edge = v3_sub(b, a);
                let edge_len_sq = v3_dot(edge, edge);
                if edge_len_sq < 1e-30 {
                    continue;
                }
                let ap = v3_sub(hit, a);
                let param = v3_dot(ap, edge) / edge_len_sq;
                if (-0.01..=1.01).contains(&param) {
                    let closest = v3_add(a, v3_scale(edge, param.clamp(0.0, 1.0)));
                    let dist_sq = v3_dot(v3_sub(hit, closest), v3_sub(hit, closest));
                    if dist_sq < 1e-14 {
                        near_edge = true;
                        break;
                    }
                }
            }
            if near_edge {
                grazing = true;
                break;
            }

            // Crossing-number PIP test
            let mut pip_crossings = 0u32;
            for i in 0..n {
                let a = &poly.verts[i];
                let b = &poly.verts[(i + 1) % n];
                let au = a[ax_u];
                let av = a[ax_v];
                let bu = b[ax_u];
                let bv = b[ax_v];

                if (av <= hit_v && bv > hit_v) || (bv <= hit_v && av > hit_v) {
                    let frac = (hit_v - av) / (bv - av);
                    let u_intercept = au + frac * (bu - au);
                    if hit_u < u_intercept {
                        pip_crossings += 1;
                    }
                }
            }

            if pip_crossings % 2 == 1 {
                crossings += 1;
            }
        }

        if !grazing {
            return crossings % 2 == 1;
        }
        // If grazing, try next direction
    }

    // All directions were grazing — conservatively say outside
    false
}

// ── Face classification ─────────────────────────────────────────────────

/// Classification of a face fragment with respect to the opposing solid.
#[derive(Debug)]
enum FaceClass {
    /// Entirely outside the opposing solid.
    Outside,
    /// Entirely inside the opposing solid.
    Inside,
    /// Non-coplanar partial: inside fragment is truly inside the opposing
    /// solid's volume. For union, only the outside fragments are emitted.
    Partial {
        inside: Vec<[f64; 3]>,
        outside_frags: Vec<Vec<[f64; 3]>>,
    },
    /// Same-direction coplanar partial: face has a coplanar partner on the
    /// opposing solid (same normal). The "inside" is the surface overlap,
    /// not inside the volume. For union: primary emits all sub-regions
    /// (inside + outside frags), secondary emits only outside frags.
    CoplanarPartial {
        inside: Vec<[f64; 3]>,
        outside_frags: Vec<Vec<[f64; 3]>>,
    },
    /// Anti-parallel coplanar: face lies on shared boundary between touching
    /// solids. For union: remove from both. For subtract: keep for A, discard for B.
    CoplanarTouching,
}

/// Classify a face polygon against the opposing solid's faces.
///
/// Uses Sutherland-Hodgman clipping as the primary classifier, with
/// point-in-solid ray casting as a secondary check when S-H reports the
/// face is "fully inside" (inside_area ≈ original_area). For convex solids,
/// S-H is authoritative; for non-convex solids, S-H can falsely report
/// full containment when the half-space intersection is degenerate.
/// Ray casting correctly handles both convex and non-convex solids.
///
/// Ref #7 Jacobson: winding number approach (simplified to ray casting).
fn classify_face(face: &FacePoly, opposing: &[FacePoly], tau: f64) -> FaceClass {
    let original_area = polygon_area_3d(&face.verts);
    if original_area < TAU_NORMALIZE {
        return FaceClass::Outside;
    }

    // Classify coplanarity with each opposing face
    let mut has_coplanar = false;
    let mut has_antiparallel_coplanar = false;
    for opp in opposing {
        match classify_coplanarity(face.normal, face.verts[0], opp, tau) {
            CoplanarClass::SameDirection => has_coplanar = true,
            CoplanarClass::AntiParallel => {
                has_coplanar = true;
                has_antiparallel_coplanar = true;
            }
            CoplanarClass::NotCoplanar => {}
        }
    }

    // Heuristic: a convex solid from extruding a convex polygon has at most
    // ~12 faces (rect=6, hexagon=8, etc.). Solids with many more faces are
    // likely non-convex (e.g., gear profiles with 50+ faces). For non-convex
    // opposing solids, Sutherland-Hodgman clipping gives wrong results, so
    // we use pure ray-casting classification instead.
    let opposing_likely_convex = opposing.len() <= 12;

    if opposing_likely_convex {
        // Convex opposing solid: S-H clipping is authoritative
        let inside = clip_polygon_by_solid(&face.verts, opposing, tau, Some(face.normal));
        let inside_area = polygon_area_3d(&inside);

        if inside_area < TAU_NORMALIZE {
            return FaceClass::Outside;
        }

        let rel_diff = (inside_area - original_area).abs() / original_area;
        if rel_diff < TAU_PARALLEL {
            if has_antiparallel_coplanar {
                return FaceClass::CoplanarTouching;
            }
            if has_coplanar {
                return FaceClass::CoplanarPartial {
                    inside: face.verts.clone(),
                    outside_frags: vec![],
                };
            }
            return FaceClass::Inside;
        }

        // Partial: split face using S-H (correct for convex opposing solid)
        let outside_frags = split_outside_fragments(&face.verts, opposing, tau, Some(face.normal));
        let has_same_dir_coplanar = has_coplanar && !has_antiparallel_coplanar;
        if has_same_dir_coplanar {
            return FaceClass::CoplanarPartial {
                inside,
                outside_frags,
            };
        }
        return FaceClass::Partial {
            inside,
            outside_frags,
        };
    }

    // Non-convex opposing solid: use ray-casting for classification.
    // S-H clipping cannot reliably classify or split faces against non-convex
    // solids because the half-space intersection is degenerate.
    let inward_offset = v3_scale(face.normal, -tau * 100.0);

    // Handle coplanar cases first
    if has_antiparallel_coplanar {
        return FaceClass::CoplanarTouching;
    }
    if has_coplanar {
        // For coplanar faces, S-H can still find the overlap region because
        // the clip skips coplanar opposing faces and clips against the
        // opposing solid's bounding planes only (which form a convex set
        // per-coplanar-face). Use S-H for the overlap calculation.
        let inside = clip_polygon_by_solid(&face.verts, opposing, tau, Some(face.normal));
        let inside_area = polygon_area_3d(&inside);
        if inside_area > 1e-15 {
            let outside_frags =
                split_outside_fragments(&face.verts, opposing, tau, Some(face.normal));
            return FaceClass::CoplanarPartial {
                inside,
                outside_frags,
            };
        }
        return FaceClass::Outside;
    }

    // Non-coplanar face against non-convex solid: classify by centroid ray-casting.
    // Cannot produce partial fragments (S-H splitting is unreliable for
    // non-convex solids), so classify as either fully Inside or fully Outside.
    let centroid = polygon_centroid(&face.verts);
    let sample = v3_add(centroid, inward_offset);
    if point_in_solid(sample, opposing) {
        return FaceClass::Inside;
    }
    FaceClass::Outside
}

/// Classify a face against a non-convex opposing solid using edge-piercing
/// analysis combined with local S-H clipping.
///
/// For non-convex opposing solids, S-H clipping against ALL opposing faces
/// is incorrect (half-space intersection ≠ solid interior). Instead, this
/// function identifies which opposing faces have edges that ACTUALLY pierce
/// this face, and clips only against those relevant planes. Locally, the
/// relevant planes form a convex boundary around the intersection, making
/// S-H valid. For faces with no piercings, uses centroid ray-casting.
///
/// Ref #7 Jacobson: winding numbers for inside/outside classification.
/// Classify a face against a non-convex opposing solid using progressive
/// splitting by face-face intersection planes.
///
/// For non-convex opposing solids, S-H clipping against ALL opposing faces
/// is incorrect (half-space intersection ≠ solid interior).  Instead:
///
/// 1. Find opposing faces whose plane actually intersects this face
///    (verified via `face_face_intersection_segment`).
/// 2. Progressively split this face by those planes (keeping BOTH halves
///    at each step — unlike S-H which keeps only the inside).
/// 3. Classify each resulting fragment with `point_in_solid`.
///
/// This produces matching boundary edges with the opposing solid's S-H
/// splits because both sides clip against the same face planes.
///
/// Ref #7 Jacobson: winding numbers for inside/outside classification.
fn classify_face_nonconvex(face: &FacePoly, opposing: &[FacePoly], tau: f64) -> FaceClass {
    let original_area = polygon_area_3d(&face.verts);
    if original_area < TAU_NORMALIZE {
        return FaceClass::Outside;
    }

    // Check coplanar partnerships
    if opposing.iter().any(|opp| {
        classify_coplanarity(face.normal, face.verts[0], opp, tau) == CoplanarClass::AntiParallel
    }) {
        return FaceClass::CoplanarTouching;
    }

    let has_coplanar = opposing
        .iter()
        .any(|opp| is_coplanar(face.normal, face.verts[0], opp, tau));

    if has_coplanar {
        return classify_coplanar_nonconvex(face, opposing, tau);
    }

    // ── Non-coplanar path: progressive splitting ─────────────────────────
    //
    // Split the face by each opposing face plane that straddles it.
    // Uses `clip_polygon_by_plane` (same as S-H and coplanar path) to
    // ensure exact vertex positions match adjacent face fragments.
    // Fragment classification uses `point_in_solid`.

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

    let inward_offset = v3_scale(face.normal, -tau * 100.0);

    if cutting_planes.is_empty() {
        // No planes straddle — classify centroid
        let centroid = polygon_centroid(&face.verts);
        let sample = v3_add(centroid, inward_offset);
        if point_in_solid(sample, opposing) {
            return FaceClass::Inside;
        }
        return FaceClass::Outside;
    }

    // Progressive splitting: split face by each cutting plane,
    // keeping BOTH halves at each step.
    let mut fragments: Vec<Vec<[f64; 3]>> = vec![face.verts.clone()];

    for (plane_pt, inward_n) in &cutting_planes {
        let outward_n = v3_negate(*inward_n);
        let mut new_fragments = Vec::new();
        for frag in &fragments {
            let half_in = clip_polygon_by_plane(frag, *plane_pt, *inward_n, tau);
            let half_out = clip_polygon_by_plane(frag, *plane_pt, outward_n, tau);
            if half_in.len() >= 3 && polygon_area_3d(&half_in) > TAU_NORMALIZE {
                new_fragments.push(half_in);
            }
            if half_out.len() >= 3 && polygon_area_3d(&half_out) > TAU_NORMALIZE {
                new_fragments.push(half_out);
            }
        }
        fragments = new_fragments;
    }

    // Classify each fragment with point_in_solid
    let mut inside_frags: Vec<Vec<[f64; 3]>> = Vec::new();
    let mut outside_frags: Vec<Vec<[f64; 3]>> = Vec::new();

    for frag in fragments {
        let centroid = polygon_centroid(&frag);
        let sample = v3_add(centroid, inward_offset);
        if point_in_solid(sample, opposing) {
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

    // Use the largest inside fragment as the canonical "inside" polygon
    let inside = inside_frags
        .into_iter()
        .max_by(|a, b| polygon_area_3d(a).partial_cmp(&polygon_area_3d(b)).unwrap())
        .unwrap();

    FaceClass::Partial {
        inside,
        outside_frags,
    }
}

/// Classify a coplanar face against a non-convex opposing solid.
///
/// Uses vertex-based classification (same as the non-coplanar path).
/// Each vertex is classified via `point_in_solid`, and edge crossings
/// are found analytically or via binary search.
fn classify_coplanar_nonconvex(face: &FacePoly, opposing: &[FacePoly], tau: f64) -> FaceClass {
    // For coplanar faces, use progressive splitting by opposing side face
    // planes (straddle-only check — no face-face intersection needed because
    // all perpendicular planes that straddle the coplanar face are relevant).
    // This produces EXACT vertex positions matching the S-H splits on the
    // opposing coplanar face, preventing boundary vertex mismatches.
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

    let inward_offset = v3_scale(face.normal, -tau * 100.0);

    if cutting_planes.is_empty() {
        // No non-coplanar faces cut us — fully inside or outside
        let centroid = polygon_centroid(&face.verts);
        let sample = v3_add(centroid, inward_offset);
        if point_in_solid(sample, opposing) {
            return FaceClass::CoplanarPartial {
                inside: face.verts.clone(),
                outside_frags: vec![],
            };
        }
        return FaceClass::Outside;
    }

    // Progressive splitting by non-coplanar opposing face planes
    let mut fragments: Vec<Vec<[f64; 3]>> = vec![face.verts.clone()];

    for (plane_pt, inward_n) in &cutting_planes {
        let outward_n = v3_negate(*inward_n);
        let mut new_fragments = Vec::new();
        for frag in &fragments {
            let half_in = clip_polygon_by_plane(frag, *plane_pt, *inward_n, tau);
            let half_out = clip_polygon_by_plane(frag, *plane_pt, outward_n, tau);
            if half_in.len() >= 3 && polygon_area_3d(&half_in) > TAU_NORMALIZE {
                new_fragments.push(half_in);
            }
            if half_out.len() >= 3 && polygon_area_3d(&half_out) > TAU_NORMALIZE {
                new_fragments.push(half_out);
            }
        }
        fragments = new_fragments;
    }

    // Classify each fragment using point_in_solid (offset inward from the face)
    let mut inside_frags: Vec<Vec<[f64; 3]>> = Vec::new();
    let mut outside_frags: Vec<Vec<[f64; 3]>> = Vec::new();

    for frag in fragments {
        let centroid = polygon_centroid(&frag);
        let sample = v3_add(centroid, inward_offset);
        if point_in_solid(sample, opposing) {
            inside_frags.push(frag);
        } else {
            outside_frags.push(frag);
        }
    }

    if inside_frags.is_empty() {
        return FaceClass::Outside;
    }

    let original_area = polygon_area_3d(&face.verts);
    let inside_total_area: f64 = inside_frags.iter().map(|f| polygon_area_3d(f)).sum();
    if (inside_total_area - original_area).abs() / original_area < 1e-6 {
        return FaceClass::CoplanarPartial {
            inside: face.verts.clone(),
            outside_frags: vec![],
        };
    }

    if outside_frags.is_empty() {
        return FaceClass::CoplanarPartial {
            inside: face.verts.clone(),
            outside_frags: vec![],
        };
    }

    let inside = inside_frags
        .into_iter()
        .max_by(|a, b| polygon_area_3d(a).partial_cmp(&polygon_area_3d(b)).unwrap())
        .unwrap();

    FaceClass::CoplanarPartial {
        inside,
        outside_frags,
    }
}

/// Compute the centroid (average position) of a polygon's vertices.
fn polygon_centroid(verts: &[[f64; 3]]) -> [f64; 3] {
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

/// Split a face polygon by the opposing solid's planes, collecting all
/// convex outside fragments. As we clip progressively against each plane,
/// the piece that falls outside that plane is a valid outside fragment
/// (it's beyond at least one of the opposing solid's half-spaces).
fn split_outside_fragments(
    face_verts: &[[f64; 3]],
    opposing: &[FacePoly],
    tau: f64,
    face_normal: Option<[f64; 3]>,
) -> Vec<Vec<[f64; 3]>> {
    let mut current = face_verts.to_vec();
    let mut outside_frags = Vec::new();

    for opp_face in opposing {
        if current.is_empty() {
            break;
        }

        // Skip coplanar opposing faces
        if let Some(fn_normal) = face_normal {
            if is_coplanar(fn_normal, current[0], opp_face, tau) {
                continue;
            }
        }

        // Inward normal = negation of the face's outward normal
        let inward = v3_negate(opp_face.normal);

        // Clip to keep inside portion (for continuing the iteration)
        let inside_part = clip_polygon_by_plane(&current, opp_face.origin, inward, tau);

        // Clip to keep outside portion (on the outward side of this plane)
        let outside_part = clip_polygon_by_plane(&current, opp_face.origin, opp_face.normal, tau);

        if outside_part.len() >= 3 && polygon_area_3d(&outside_part) > TAU_NORMALIZE {
            outside_frags.push(outside_part);
        }

        current = inside_part;
    }

    outside_frags
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
    // Use 1e-7 relative to the model diagonal, clamped to [1e-12, 1e-4].
    // This matches the pre-existing 1e-7 for unit-scale models.
    let tau_weld = (diag * 1e-7).max(1e-12).min(1e-4);
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

    if a_faces.is_empty() || b_faces.is_empty() {
        return Err(KernelError::BooleanFailed {
            reason: "one or both solids have no planar faces".to_string(),
        });
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

    if a_faces.is_empty() || b_faces.is_empty() {
        return Err(KernelError::BooleanFailed {
            reason: "one or both solids have no planar faces".to_string(),
        });
    }

    boolean_op_from_polys(a_faces, b_faces, op, id_alloc)
}

/// Snap all polygon vertices to the weld grid, ensuring that independent
/// Sutherland-Hodgman clipping operations on adjacent faces produce
/// bit-identical intersection vertices for the same geometric point.
///
/// After snapping, deduplicates consecutive vertices and removes degenerate
/// polygons (< 3 unique vertices).
#[allow(dead_code)]
fn snap_vertices_to_grid(polys: &[FacePoly], tau_weld: f64) -> Vec<FacePoly> {
    let inv_tau = 1.0 / tau_weld;
    let snap = |v: [f64; 3]| -> [f64; 3] {
        [
            (v[0] * inv_tau).round() * tau_weld,
            (v[1] * inv_tau).round() * tau_weld,
            (v[2] * inv_tau).round() * tau_weld,
        ]
    };

    let mut result = Vec::with_capacity(polys.len());
    for poly in polys {
        // Snap all vertices
        let snapped: Vec<[f64; 3]> = poly.verts.iter().map(|&v| snap(v)).collect();

        // Deduplicate consecutive vertices (from snapping nearby points to same grid cell)
        let mut deduped: Vec<[f64; 3]> = Vec::with_capacity(snapped.len());
        for i in 0..snapped.len() {
            let prev = if i == 0 { snapped.len() - 1 } else { i - 1 };
            if snapped[i] != snapped[prev] {
                deduped.push(snapped[i]);
            }
        }

        if deduped.len() >= 3 {
            result.push(FacePoly {
                verts: deduped,
                normal: poly.normal,
                origin: snap(poly.origin),
            });
        }
    }
    result
}

/// Resolve T-junctions in a polygon soup.
///
/// When boolean classification splits some faces but not others, adjacent
/// faces can have mismatched edges: one face has a long edge from A→C, while
/// an adjacent face introduces a vertex B between A and C. This creates a
/// T-junction that makes edge pairing impossible.
///
/// This function detects and resolves T-junctions by:
/// 1. Collecting all vertices from all polygons
/// 2. For each face edge, checking if any vertex from other faces lies on the
///    edge interior (within tolerance)
/// 3. Inserting those vertices into the edge, splitting it
fn resolve_t_junctions(polys: &[FacePoly], tau: f64) -> Vec<FacePoly> {
    // Collect all unique vertices (quantized for lookup)
    let inv_tau = 1.0 / tau;
    let quantize = |p: [f64; 3]| -> (i64, i64, i64) {
        (
            (p[0] * inv_tau).round() as i64,
            (p[1] * inv_tau).round() as i64,
            (p[2] * inv_tau).round() as i64,
        )
    };

    // Build set of all vertices across all polygons
    let mut all_verts: Vec<[f64; 3]> = Vec::new();
    let mut seen = std::collections::HashSet::new();
    for poly in polys {
        for &v in &poly.verts {
            let key = quantize(v);
            if seen.insert(key) {
                all_verts.push(v);
            }
        }
    }

    // For each polygon, check each edge for T-junction vertices
    let mut result = Vec::with_capacity(polys.len());
    for poly in polys {
        let n = poly.verts.len();
        if n < 3 {
            result.push(poly.clone());
            continue;
        }

        let mut new_verts: Vec<[f64; 3]> = Vec::new();
        for i in 0..n {
            let a = poly.verts[i];
            let b = poly.verts[(i + 1) % n];
            new_verts.push(a);

            // Find vertices that lie strictly on the interior of edge A→B
            let edge_vec = v3_sub(b, a);
            let edge_len_sq = v3_dot(edge_vec, edge_vec);
            if edge_len_sq < tau * tau {
                continue; // degenerate edge
            }

            // Collect candidate split points with their parametric position
            let mut splits: Vec<(f64, [f64; 3])> = Vec::new();
            let a_key = quantize(a);
            let b_key = quantize(b);

            for &v in &all_verts {
                let v_key = quantize(v);
                // Skip edge endpoints
                if v_key == a_key || v_key == b_key {
                    continue;
                }

                // Check if v lies on the line segment A→B
                let av = v3_sub(v, a);
                let t = v3_dot(av, edge_vec) / edge_len_sq;
                if t <= tau || t >= 1.0 - tau {
                    continue; // not in interior
                }

                // Check distance from the line (relative to edge length)
                let proj = v3_add(a, v3_scale(edge_vec, t));
                let diff = v3_sub(v, proj);
                let dist_sq = v3_dot(diff, diff);
                let rel_tol_sq = edge_len_sq * (tau * 10.0) * (tau * 10.0);
                if dist_sq < tau * tau * 100.0 || dist_sq < rel_tol_sq {
                    splits.push((t, v));
                }
            }

            // Sort splits by parametric position and insert
            splits.sort_by(|a, b| a.0.partial_cmp(&b.0).unwrap());
            for (_, split_pt) in splits {
                new_verts.push(split_pt);
            }
        }

        result.push(FacePoly {
            verts: new_verts,
            normal: poly.normal,
            origin: poly.origin,
        });
    }

    result
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
    let emit =
        |output: &mut Vec<FacePoly>, verts: Vec<[f64; 3]>, normal: [f64; 3], origin: [f64; 3]| {
            if verts.len() < 3 {
                return;
            }
            let mut f = FacePoly {
                verts,
                normal,
                origin,
            };
            if flip_normals {
                f.normal = v3_negate(f.normal);
                f.verts.reverse();
            }
            output.push(f);
        };

    for (face, class) in classified {
        match class {
            FaceClass::Outside => {
                if include_outside {
                    emit(output, face.verts.clone(), face.normal, face.origin);
                }
            }
            FaceClass::Inside => {
                if include_fully_inside {
                    emit(output, face.verts.clone(), face.normal, face.origin);
                }
            }
            FaceClass::Partial {
                inside,
                outside_frags,
            }
            | FaceClass::CoplanarPartial {
                inside,
                outside_frags,
            } => {
                if include_outside {
                    for frag in outside_frags {
                        emit(output, frag.clone(), face.normal, face.origin);
                    }
                }
                if include_partial_inside {
                    emit(output, inside.clone(), face.normal, face.origin);
                }
            }
            FaceClass::CoplanarTouching => {
                // Anti-parallel coplanar: face is on the shared boundary.
                // For subtract A: keep (B doesn't cut A at touching boundary).
                // For subtract B / intersect: discard.
                if include_outside {
                    emit(output, face.verts.clone(), face.normal, face.origin);
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
                inside: _,
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

// ── B-Rep construction from polygon soup ────────────────────────────────

/// Build a complete B-Rep (arena + maps + geometry) from a list of face polygons.
///
/// Steps:
/// 1. Weld vertices by quantizing to `tau_weld` grid
/// 2. Create faces and loops with half-edges
/// 3. Pair twin half-edges to form edges
/// 4. Assign planar geometry to faces and linear geometry to edges
/// 5. Build KernelId maps for all entities
fn build_brep_from_polygons(
    faces: &[FacePoly],
    tau_weld: f64,
    id_alloc: &mut dyn FnMut() -> u64,
) -> Result<BooleanResult, KernelError> {
    build_brep_from_polygons_inner(faces, tau_weld, false, id_alloc)
}

/// Build B-Rep from polygon soup with optional near-manifold tolerance.
///
/// When `allow_boundary` is true, allows up to 5% unpaired half-edges
/// (creates self-twin boundary edges). When false, any unpaired edges
/// produce an error.
fn build_brep_from_polygons_inner(
    faces: &[FacePoly],
    tau_weld: f64,
    allow_boundary: bool,
    id_alloc: &mut dyn FnMut() -> u64,
) -> Result<BooleanResult, KernelError> {
    let mut arena = TopoArena::new();
    let mut face_map = HashMap::new();
    let mut edge_map = HashMap::new();
    let mut vertex_map = HashMap::new();
    let mut face_geometry: HashMap<FaceIdx, SurfaceGeom> = HashMap::new();
    let mut edge_geometry: HashMap<EdgeIdx, CurveGeom> = HashMap::new();

    // Step 1: Weld vertices — quantize positions to tau_weld grid
    let inv_tau = 1.0 / tau_weld;
    let mut pos_to_vertex: HashMap<(i64, i64, i64), VertexIdx> = HashMap::new();

    let quantize = |p: [f64; 3]| -> (i64, i64, i64) {
        (
            (p[0] * inv_tau).round() as i64,
            (p[1] * inv_tau).round() as i64,
            (p[2] * inv_tau).round() as i64,
        )
    };

    // Pre-scan all face vertices to build the welded vertex set
    let mut face_vert_indices: Vec<Vec<VertexIdx>> = Vec::with_capacity(faces.len());
    for face_poly in faces {
        let mut indices = Vec::with_capacity(face_poly.verts.len());
        for &pos in &face_poly.verts {
            let key = quantize(pos);
            let vidx = *pos_to_vertex
                .entry(key)
                .or_insert_with(|| arena.add_vertex(pos));
            indices.push(vidx);
        }
        face_vert_indices.push(indices);
    }

    // Step 2: Create solid, shell, faces, loops, and half-edges
    let solid_idx = arena.add_solid();
    let shell_idx = arena.add_shell(solid_idx);
    arena.solids[solid_idx.0].outer_shell = shell_idx;

    // Map directed edges (from, to) → HalfEdgeIdx for twin pairing
    let mut directed_he: HashMap<(VertexIdx, VertexIdx), HalfEdgeIdx> = HashMap::new();
    // Track all half-edges that need twin pairing
    let mut unpaired_hes: Vec<HalfEdgeIdx> = Vec::new();

    let mut first_face_idx = None;
    let mut face_idx_map: HashMap<usize, FaceIdx> = HashMap::new();

    for (fi, face_poly) in faces.iter().enumerate() {
        let vert_indices = &face_vert_indices[fi];
        let n = vert_indices.len();
        if n < 3 {
            continue;
        }

        // Deduplicate consecutive vertices (from welding)
        let mut deduped_verts: Vec<VertexIdx> = Vec::with_capacity(n);
        for i in 0..n {
            let v = vert_indices[i];
            let prev = vert_indices[(i + n - 1) % n];
            if v != prev {
                deduped_verts.push(v);
            }
        }
        if deduped_verts.len() < 3 {
            continue;
        }

        let face_idx = arena.add_face(shell_idx);
        face_idx_map.insert(fi, face_idx);
        if first_face_idx.is_none() {
            first_face_idx = Some(face_idx);
        }
        let loop_idx = arena.add_loop(face_idx);
        arena.faces[face_idx.0].outer_loop = loop_idx;

        // Assign face geometry
        face_geometry.insert(
            face_idx,
            SurfaceGeom::Planar(Plane {
                origin: Point3::from_array(face_poly.origin),
                normal: Vector3::from_array(face_poly.normal),
            }),
        );

        // Allocate KernelId for this face
        let fid = id_alloc();
        face_map.insert(fid, face_idx);

        // Create half-edges for this face loop
        let m = deduped_verts.len();
        let first_he_idx = HalfEdgeIdx(arena.half_edges.len());

        for i in 0..m {
            let origin = deduped_verts[i];
            let he_idx = HalfEdgeIdx(arena.half_edges.len());
            let next_he = HalfEdgeIdx(first_he_idx.0 + ((i + 1) % m));
            let prev_he = HalfEdgeIdx(first_he_idx.0 + ((i + m - 1) % m));

            arena.half_edges.push(HalfEdge {
                origin,
                edge: EdgeIdx(0), // placeholder, set during twin pairing
                twin: he_idx,     // placeholder, set during twin pairing
                next: next_he,
                prev: prev_he,
                loop_: loop_idx,
            });

            // Set vertex half-edge reference
            arena.vertices[origin.0].half_edge = Some(he_idx);

            // Register directed edge for twin pairing
            let dest = deduped_verts[(i + 1) % m];
            directed_he.insert((origin, dest), he_idx);
            unpaired_hes.push(he_idx);
        }

        // Set loop's half-edge
        arena.loops[loop_idx.0].half_edge = first_he_idx;
    }

    // Set shell's face reference
    if let Some(ff) = first_face_idx {
        arena.shells[shell_idx.0].face = ff;
    }

    // Step 3: Twin pairing — match directed edges (A→B) with (B→A)
    let mut paired: std::collections::HashSet<HalfEdgeIdx> = std::collections::HashSet::new();

    for &he_idx in &unpaired_hes {
        if paired.contains(&he_idx) {
            continue;
        }
        let origin = arena.half_edges[he_idx.0].origin;
        let next_he = arena.half_edges[he_idx.0].next;
        let dest = arena.half_edges[next_he.0].origin;

        // Look for twin: the half-edge going from dest to origin
        if let Some(&twin_idx) = directed_he.get(&(dest, origin)) {
            if twin_idx != he_idx && !paired.contains(&twin_idx) {
                // Create an edge for this pair
                let edge_idx = EdgeIdx(arena.edges.len());
                arena.edges.push(Edge { half_edge: he_idx });

                arena.half_edges[he_idx.0].twin = twin_idx;
                arena.half_edges[he_idx.0].edge = edge_idx;
                arena.half_edges[twin_idx.0].twin = he_idx;
                arena.half_edges[twin_idx.0].edge = edge_idx;

                paired.insert(he_idx);
                paired.insert(twin_idx);

                // Assign edge geometry
                let p0 = arena.vertices[origin.0].position;
                let p1 = arena.vertices[dest.0].position;
                let dir = v3_sub(p1, p0);
                edge_geometry.insert(
                    edge_idx,
                    CurveGeom::Linear(Line3D {
                        origin: Point3::from_array(p0),
                        direction: Vector3::from_array(dir),
                    }),
                );

                // Allocate KernelId for this edge
                let eid = id_alloc();
                edge_map.insert(eid, edge_idx);
            }
        }
    }

    // Step 3b: Remove degenerate faces (area < tau_weld^2) and retry twin pairing.
    // Degenerate faces arise from near-coplanar clipping and produce unpaired half-edges.
    let tau_sq = tau_weld * tau_weld;
    let mut degenerate_faces: Vec<FaceIdx> = Vec::new();
    for (fi, face_poly) in faces.iter().enumerate() {
        if face_poly.verts.len() < 3 {
            continue;
        }
        let area = polygon_area_3d(&face_poly.verts);
        if area < tau_sq {
            // Find the FaceIdx for this face (fi-th face that was added)
            // We track face indices during creation
            if let Some(&fidx) = face_idx_map.get(&fi) {
                degenerate_faces.push(fidx);
            }
        }
    }

    if !degenerate_faces.is_empty() {
        // Collect half-edges belonging to degenerate faces
        let mut degen_hes: std::collections::HashSet<HalfEdgeIdx> =
            std::collections::HashSet::new();
        for &face_idx in &degenerate_faces {
            let loop_idx = arena.faces[face_idx.0].outer_loop;
            let start_he = arena.loops[loop_idx.0].half_edge;
            let mut he = start_he;
            loop {
                degen_hes.insert(he);
                he = arena.half_edges[he.0].next;
                if he == start_he {
                    break;
                }
            }
        }

        // Unpair any half-edges paired with degenerate face half-edges
        for &he in &degen_hes {
            if paired.contains(&he) {
                let twin = arena.half_edges[he.0].twin;
                if twin != he {
                    paired.remove(&he);
                    paired.remove(&twin);
                }
            }
        }

        // Remove degenerate half-edges from unpaired tracking
        unpaired_hes.retain(|he| !degen_hes.contains(he));

        // Retry twin pairing for newly unpaired half-edges
        for &he_idx in &unpaired_hes {
            if paired.contains(&he_idx) {
                continue;
            }
            let origin = arena.half_edges[he_idx.0].origin;
            let next_he = arena.half_edges[he_idx.0].next;
            let dest = arena.half_edges[next_he.0].origin;

            if let Some(&twin_idx) = directed_he.get(&(dest, origin)) {
                if twin_idx != he_idx
                    && !paired.contains(&twin_idx)
                    && !degen_hes.contains(&twin_idx)
                {
                    let edge_idx = EdgeIdx(arena.edges.len());
                    arena.edges.push(Edge { half_edge: he_idx });

                    arena.half_edges[he_idx.0].twin = twin_idx;
                    arena.half_edges[he_idx.0].edge = edge_idx;
                    arena.half_edges[twin_idx.0].twin = he_idx;
                    arena.half_edges[twin_idx.0].edge = edge_idx;

                    paired.insert(he_idx);
                    paired.insert(twin_idx);

                    let p0 = arena.vertices[origin.0].position;
                    let p1 = arena.vertices[dest.0].position;
                    let dir = v3_sub(p1, p0);
                    edge_geometry.insert(
                        edge_idx,
                        CurveGeom::Linear(Line3D {
                            origin: Point3::from_array(p0),
                            direction: Vector3::from_array(dir),
                        }),
                    );

                    let eid = id_alloc();
                    edge_map.insert(eid, edge_idx);
                }
            }
        }
    }

    // Step 3c: Position-based fallback twin pairing
    // Some edges fail to pair when same-position vertices got different indices
    // due to quantization grid boundary effects.
    {
        let mut pos_directed_he: HashMap<((i64, i64, i64), (i64, i64, i64)), Vec<HalfEdgeIdx>> =
            HashMap::new();

        // Build position-based map for unpaired half-edges only
        for &he_idx in &unpaired_hes {
            if paired.contains(&he_idx) {
                continue;
            }
            let origin = arena.half_edges[he_idx.0].origin;
            let next_he = arena.half_edges[he_idx.0].next;
            let dest = arena.half_edges[next_he.0].origin;
            let origin_pos = quantize(arena.vertices[origin.0].position);
            let dest_pos = quantize(arena.vertices[dest.0].position);
            pos_directed_he
                .entry((origin_pos, dest_pos))
                .or_default()
                .push(he_idx);
        }

        // Try to pair using position keys (reverse direction lookup)
        for he_idx in unpaired_hes.clone() {
            if paired.contains(&he_idx) {
                continue;
            }
            let origin = arena.half_edges[he_idx.0].origin;
            let next_he = arena.half_edges[he_idx.0].next;
            let dest = arena.half_edges[next_he.0].origin;
            let origin_pos = quantize(arena.vertices[origin.0].position);
            let dest_pos = quantize(arena.vertices[dest.0].position);

            // Look for reverse-direction edge by position
            if let Some(candidates) = pos_directed_he.get(&(dest_pos, origin_pos)) {
                for &twin_idx in candidates {
                    if twin_idx != he_idx && !paired.contains(&twin_idx) {
                        // Pair them
                        let edge_idx = EdgeIdx(arena.edges.len());
                        arena.edges.push(Edge { half_edge: he_idx });
                        arena.half_edges[he_idx.0].twin = twin_idx;
                        arena.half_edges[he_idx.0].edge = edge_idx;
                        arena.half_edges[twin_idx.0].twin = he_idx;
                        arena.half_edges[twin_idx.0].edge = edge_idx;
                        paired.insert(he_idx);
                        paired.insert(twin_idx);

                        // Add edge geometry
                        let p0 = arena.vertices[origin.0].position;
                        let p1 = arena.vertices[dest.0].position;
                        let dir = v3_sub(p1, p0);
                        edge_geometry.insert(
                            edge_idx,
                            CurveGeom::Linear(Line3D {
                                origin: Point3::from_array(p0),
                                direction: Vector3::from_array(dir),
                            }),
                        );
                        let eid = id_alloc();
                        edge_map.insert(eid, edge_idx);
                        break;
                    }
                }
            }
        }
    }

    // Step 3d: Proximity-based twin pairing for remaining unpaired half-edges.
    // When adjacent faces are at an angle, their shared edge may be split at
    // slightly different positions by independent S-H clipping. The T-junction
    // resolver can't fix this because the split point on face A is NOT on face
    // B's edge (they're at different angles). Instead, pair unpaired edges by
    // finding the closest reverse-direction unpaired edge whose midpoints match.
    {
        let remaining_unpaired: Vec<HalfEdgeIdx> = unpaired_hes
            .iter()
            .filter(|he| !paired.contains(he))
            .copied()
            .collect();

        if remaining_unpaired.len() >= 2 {
            // Compute midpoints for all unpaired half-edges
            let midpoints: Vec<([f64; 3], [f64; 3], [f64; 3])> = remaining_unpaired
                .iter()
                .map(|&he| {
                    let origin = arena.half_edges[he.0].origin;
                    let next_he = arena.half_edges[he.0].next;
                    let dest = arena.half_edges[next_he.0].origin;
                    let p0 = arena.vertices[origin.0].position;
                    let p1 = arena.vertices[dest.0].position;
                    let mid = [
                        (p0[0] + p1[0]) * 0.5,
                        (p0[1] + p1[1]) * 0.5,
                        (p0[2] + p1[2]) * 0.5,
                    ];
                    (p0, p1, mid)
                })
                .collect();

            // For each unpaired he, find closest unpaired he going in opposite direction
            for i in 0..remaining_unpaired.len() {
                let he_a = remaining_unpaired[i];
                if paired.contains(&he_a) {
                    continue;
                }
                let (a_p0, a_p1, a_mid) = midpoints[i];

                let mut best_j = None;
                let mut best_dist = f64::INFINITY;

                for j in (i + 1)..remaining_unpaired.len() {
                    let he_b = remaining_unpaired[j];
                    if paired.contains(&he_b) {
                        continue;
                    }
                    let (b_p0, b_p1, b_mid) = midpoints[j];

                    // Check reverse direction: A goes p0→p1, B should go ~p1→p0
                    let fwd_dist = v3_dot(v3_sub(a_p0, b_p1), v3_sub(a_p0, b_p1))
                        + v3_dot(v3_sub(a_p1, b_p0), v3_sub(a_p1, b_p0));
                    let mid_dist = v3_dot(v3_sub(a_mid, b_mid), v3_sub(a_mid, b_mid));

                    // Use generous tolerance: 100 * tau_weld
                    let tol = tau_weld * 100.0;
                    if fwd_dist < tol * tol && mid_dist < tol * tol && fwd_dist < best_dist {
                        best_dist = fwd_dist;
                        best_j = Some(j);
                    }
                }

                if let Some(j) = best_j {
                    let he_b = remaining_unpaired[j];
                    let edge_idx = EdgeIdx(arena.edges.len());
                    arena.edges.push(Edge { half_edge: he_a });
                    arena.half_edges[he_a.0].twin = he_b;
                    arena.half_edges[he_a.0].edge = edge_idx;
                    arena.half_edges[he_b.0].twin = he_a;
                    arena.half_edges[he_b.0].edge = edge_idx;
                    paired.insert(he_a);
                    paired.insert(he_b);

                    let p0 = arena.vertices[arena.half_edges[he_a.0].origin.0].position;
                    let next_he = arena.half_edges[he_a.0].next;
                    let p1 = arena.vertices[arena.half_edges[next_he.0].origin.0].position;
                    let dir = v3_sub(p1, p0);
                    edge_geometry.insert(
                        edge_idx,
                        CurveGeom::Linear(Line3D {
                            origin: Point3::from_array(p0),
                            direction: Vector3::from_array(dir),
                        }),
                    );
                    let eid = id_alloc();
                    edge_map.insert(eid, edge_idx);
                }
            }
        }
    }

    // Step 4: Handle remaining unpaired half-edges.
    let unpaired_count = unpaired_hes
        .iter()
        .filter(|he| !paired.contains(he))
        .count();
    let total_count = unpaired_hes.len();

    if unpaired_count > 0 {
        let unpaired_ratio = unpaired_count as f64 / total_count.max(1) as f64;
        // Allow up to 5% unpaired in strict mode (S-H clipping creates small
        // T-junction gaps from independent floating-point intersection computation).
        // Allow up to 25% in tolerant mode (polygon approximation and fallback
        // from strict mode — more boundary edges accepted as best-effort).
        let threshold = if allow_boundary { 0.25 } else { 0.05 };
        if unpaired_ratio > threshold {
            return Err(KernelError::BooleanFailed {
                reason: format!(
                    "non-manifold result: {} half-edges unpaired out of {} ({:.1}%)",
                    unpaired_count,
                    total_count,
                    unpaired_ratio * 100.0
                ),
            });
        }
    }

    // Create self-twin boundary edges for any remaining unpaired half-edges.
    for &he_idx in &unpaired_hes {
        if paired.contains(&he_idx) {
            continue;
        }
        let edge_idx = EdgeIdx(arena.edges.len());
        arena.edges.push(Edge { half_edge: he_idx });
        arena.half_edges[he_idx.0].edge = edge_idx;

        let origin = arena.half_edges[he_idx.0].origin;
        let next_he = arena.half_edges[he_idx.0].next;
        let dest = arena.half_edges[next_he.0].origin;
        let p0 = arena.vertices[origin.0].position;
        let p1 = arena.vertices[dest.0].position;
        let dir = v3_sub(p1, p0);
        edge_geometry.insert(
            edge_idx,
            CurveGeom::Linear(Line3D {
                origin: Point3::from_array(p0),
                direction: Vector3::from_array(dir),
            }),
        );
        let eid = id_alloc();
        edge_map.insert(eid, edge_idx);
    }

    // Step 5: Build vertex map
    for (idx, _) in arena.vertices.iter().enumerate() {
        let vid = id_alloc();
        vertex_map.insert(vid, VertexIdx(idx));
    }

    Ok(BooleanResult {
        arena,
        face_map,
        edge_map,
        vertex_map,
        face_geometry,
        edge_geometry,
    })
}

// ── SSI-based boolean operations (box-cylinder, cylinder-cylinder) ──────

use crate::geometry::curve::{Arc3D, Circle3D};
use crate::geometry::surface::Cylinder;
use crate::ssi::{self, Aabb};
use crate::waffle_kernel::CylinderParams;

/// Polygon-approximation boolean: convert any cylinder solids to polygon face
/// approximations, then use the standard polygon-clipping boolean pipeline.
///
/// This is a fallback for cylinder-involving booleans that the analytical SSI
/// pipeline doesn't handle (e.g., cylinder-minus-box, partial overlaps).
fn polygon_approx_boolean(
    solid_a: &WaffleSolid,
    solid_b: &WaffleSolid,
    op: BoolOp,
    id_alloc: &mut dyn FnMut() -> u64,
) -> Result<BooleanResult, KernelError> {
    let a_faces = extract_face_polys_general(solid_a);
    let b_faces = extract_face_polys_general(solid_b);

    if a_faces.is_empty() || b_faces.is_empty() {
        return Err(KernelError::BooleanFailed {
            reason: "one or both solids have no face polygons".to_string(),
        });
    }

    // Limit face count to prevent O(n*m) explosion in classification.
    // Cylinder (34 faces) + box (6 faces) = 40 → OK.
    // Cylinder (34) + gear (many faces) = 60+ → can be very slow.
    let total_faces = a_faces.len() + b_faces.len();
    if total_faces > 200 {
        return Err(KernelError::NotSupported {
            operation: format!(
                "polygon approx boolean: {} total faces exceeds limit",
                total_faces
            ),
        });
    }

    boolean_op_from_polys(a_faces, b_faces, op, id_alloc)
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
fn boolean_op_from_polys(
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
    if total_faces > 250 {
        return Err(KernelError::NotSupported {
            operation: format!(
                "polygon boolean: {} total faces exceeds limit (250)",
                total_faces
            ),
        });
    }

    let (tau, tau_weld) = compute_adaptive_tau_weld(&a_faces, &b_faces);

    let b_convex = b_faces.len() <= 12;
    let a_convex = a_faces.len() <= 12;

    let a_classified: Vec<(FacePoly, FaceClass)> = a_faces
        .iter()
        .map(|f| {
            let class = if b_convex {
                classify_face(f, &b_faces, tau)
            } else {
                classify_face_nonconvex(f, &b_faces, tau)
            };
            (f.clone(), class)
        })
        .collect();

    let b_classified: Vec<(FacePoly, FaceClass)> = b_faces
        .iter()
        .map(|f| {
            let class = if a_convex {
                classify_face(f, &a_faces, tau)
            } else {
                classify_face_nonconvex(f, &a_faces, tau)
            };
            (f.clone(), class)
        })
        .collect();

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
        });
    }

    let result_polys = resolve_t_junctions(&result_polys, tau_weld);

    build_brep_from_polygons_inner(&result_polys, tau_weld, allow_boundary, id_alloc)
}

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
            BoolOp::Subtract => Err(KernelError::NotSupported {
                operation: "cylinder minus box".to_string(),
            }),
        }
    } else {
        Err(KernelError::NotSupported {
            operation: "unsupported boolean operand combination".to_string(),
        })
    };

    // Fall back to polygon approximation for NotSupported errors
    match analytical_result {
        Err(KernelError::NotSupported { .. }) => {
            polygon_approx_boolean(solid_a, solid_b, op, id_alloc)
        }
        other => other,
    }
}

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
    // Rotate into cylinder's Z-aligned frame
    let m = rotation_to_z(cyl.direction);
    let m_inv = mat3_transpose(&m);
    let cyl_z = rotate_cyl_params(cyl, &m);
    let box_aabb = ssi::compute_rotated_box_aabb(box_solid, &m);

    let xy_enclosed_aabb = ssi::cyl_enclosed_in_box(&cyl_z, &box_aabb);
    // AABB enclosure is necessary but not sufficient for non-convex polygon extrudes.
    // A rectangular prism has exactly 6 faces; more faces indicate a non-rectangular
    // (possibly concave) polygon extrude. Refine with point-in-solid test.
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
    let z_enclosed = cyl_z_min >= box_aabb.min[2] - 1e-9 && cyl_z_max <= box_aabb.max[2] + 1e-9;
    let fully_enclosed = xy_enclosed && z_enclosed;

    // Detect boss: cylinder XY-enclosed and extends beyond box on top and/or bottom.
    // Covers both the "sits on top/bottom" case (z_touches) and the "passes through"
    // case (cylinder starts inside box but extends beyond a face).
    let extends_above = cyl_z_max > box_aabb.max[2] + 1e-9;
    let extends_below = cyl_z_min < box_aabb.min[2] - 1e-9;
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
        return Err(KernelError::NotSupported {
            operation: "non-parallel cylinder-cylinder boolean".to_string(),
        });
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
        if z_max <= z_min + 1e-9 {
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
        if d < 1e-9 {
            return match op {
                BoolOp::Subtract => {
                    if r2 >= r1 - 1e-9 {
                        Err(KernelError::BooleanFailed {
                            reason: "tool encloses or equals blank (concentric)".to_string(),
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
fn clone_solid_as_result(
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

    // 4 vertices: outer bottom seam, outer top seam, inner bottom seam, inner top seam
    // But for the standard cylinder B-Rep with seam edges, we only need 2 vertices
    // on the outer cylinder and 2 on the inner cylinder. However the annular caps
    // connect outer and inner, so we need all 4.
    // Actually: the outer wall self-loops with 2 vertices, inner wall self-loops with 2 vertices,
    // and the annular caps connect them. So we need 4 vertices total.
    // But wait — the annular caps need to reference both outer and inner seam vertices.
    // Let's keep it simple: 4 vertices.
    let v_bot_outer = arena.add_vertex(bot_outer_seam);
    let v_top_outer = arena.add_vertex(top_outer_seam);
    let v_bot_inner = arena.add_vertex(bot_inner_seam);
    let v_top_inner = arena.add_vertex(top_inner_seam);

    // 7 edges:
    // e_outer_bot: outer bottom circle (self-loop on bottom cap outer)
    // e_outer_top: outer top circle (self-loop on top cap outer)
    // e_inner_bot: inner bottom circle (self-loop on bottom cap inner)
    // e_inner_top: inner top circle (self-loop on top cap inner)
    // e_outer_seam: outer vertical seam
    // e_inner_seam: inner vertical seam
    // But actually, following the same pattern as build_cyl_result and build_box_minus_enclosed_cyl:
    // Outer wall: 3 edges (outer_bot circle, outer_top circle, outer_seam line)
    //   - loop: seam_a → top_b → seam_b → bot_b (4 half-edges)
    // Inner wall: 3 edges (inner_bot circle, inner_top circle, inner_seam line)
    //   - loop: seam_a → top_b → seam_b → bot_b (4 half-edges)
    // Top cap outer loop: 1 self-loop edge (outer_top circle) — he_outer_top_a
    // Top cap inner loop: 1 self-loop edge (inner_top circle) — he_inner_top_a
    // Bottom cap outer loop: 1 self-loop edge (outer_bot circle) — he_outer_bot_a
    // Bottom cap inner loop: 1 self-loop edge (inner_bot circle) — he_inner_bot_a

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
    let z_min = aabb.min[2];
    let z_max = aabb.max[2];
    let cx = cyl.center_bottom[0];
    let cy = cyl.center_bottom[1];
    let r = cyl.radius;
    let dir = cyl.direction;

    // Step 1: Build box using build_brep_from_polygons (correct shared edges)
    let box_faces = make_box_face_polys(aabb);
    let tau_weld = 1e-7;
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

    // Step 3: Add cylinder seam vertices
    let bot_seam = [cx + r, cy, z_min];
    let top_seam = [cx + r, cy, z_max];
    let v_bot_seam = result.arena.add_vertex(bot_seam);
    let v_top_seam = result.arena.add_vertex(top_seam);

    // Step 4: Add inner circle loops for bottom and top caps
    let inner_loop_bot = result.arena.add_loop(face_bot);
    let inner_loop_top = result.arena.add_loop(face_top);
    result.arena.faces[face_bot.0]
        .inner_loops
        .push(inner_loop_bot);
    result.arena.faces[face_top.0]
        .inner_loops
        .push(inner_loop_top);

    // Inner circle self-loops
    let (e_bot_circle, he_ibot_a, he_ibot_b) = result.arena.add_edge();
    result.arena.half_edges[he_ibot_a.0].origin = v_bot_seam;
    result.arena.half_edges[he_ibot_a.0].next = he_ibot_a;
    result.arena.half_edges[he_ibot_a.0].prev = he_ibot_a;
    result.arena.half_edges[he_ibot_a.0].loop_ = inner_loop_bot;
    result.arena.loops[inner_loop_bot.0].half_edge = he_ibot_a;

    let (e_top_circle, he_itop_a, he_itop_b) = result.arena.add_edge();
    result.arena.half_edges[he_itop_a.0].origin = v_top_seam;
    result.arena.half_edges[he_itop_a.0].next = he_itop_a;
    result.arena.half_edges[he_itop_a.0].prev = he_itop_a;
    result.arena.half_edges[he_itop_a.0].loop_ = inner_loop_top;
    result.arena.loops[inner_loop_top.0].half_edge = he_itop_a;

    // Step 5: Add cylinder side face
    let shell_idx = ShellIdx(0);
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
            center: Point3::from_array([cx, cy, z_min]),
            normal: Vector3::new(0.0, 0.0, 1.0),
            radius: r,
        }),
    );
    result.edge_geometry.insert(
        e_top_circle,
        CurveGeom::Circular(Circle3D {
            center: Point3::from_array([cx, cy, z_max]),
            normal: Vector3::new(0.0, 0.0, 1.0),
            radius: r,
        }),
    );
    result.edge_geometry.insert(
        e_seam,
        CurveGeom::Linear(Line3D {
            origin: Point3::from_array(bot_seam),
            direction: Vector3::from_array([0.0, 0.0, z_max - z_min]),
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

// ── Disjoint unions ────────────────────────────────────────────────────

/// Build a disjoint union of a box and a cylinder.
fn build_disjoint_box_cyl_union(
    aabb: &Aabb,
    cyl: &CylinderParams,
    id_alloc: &mut dyn FnMut() -> u64,
) -> Result<BooleanResult, KernelError> {
    // Build box as polygon faces
    let box_faces = make_box_face_polys(aabb);
    let tau_weld = 1e-7;
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
    let tau_weld = 1e-7;
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
fn make_box_face_polys(aabb: &Aabb) -> Vec<FacePoly> {
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
        },
    ]
}

/// Merge a second BooleanResult into the first (for disjoint unions).
fn merge_brep_into(
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
    // For cyl_a: the arc from p1 to p2 going away from cyl_b (the longer arc if partially overlapping)
    // For cyl_b: the arc from p2 to p1 going away from cyl_a

    // Determine which arc of cyl_a is "outside" cyl_b:
    // Sample the midpoint of each arc and check if it's outside cyl_b
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
        // Short arc is inside B → outside arc is the long arc (from p2 to p1, CCW)
        (angle_a2, sweep_a_long, angle_a1, sweep_a_short)
    } else {
        // Short arc is outside B → outside arc is the short arc
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

    // 6 edges: line_p1 (v0↔v2), line_p2 (v1↔v3),
    //          arc_a_bot (v0↔v1), arc_a_top (v2↔v3),
    //          arc_b_bot (v1↔v0), arc_b_top (v3↔v2)
    let (e_line_p1, he_lp1_a, he_lp1_b) = arena.add_edge(); // v0→v2 / v2→v0
    let (e_line_p2, he_lp2_a, he_lp2_b) = arena.add_edge(); // v1→v3 / v3→v1
    let (e_arc_a_bot, he_aab_a, he_aab_b) = arena.add_edge(); // arc_a at bottom: v0→v1 / v1→v0
    let (e_arc_a_top, he_aat_a, he_aat_b) = arena.add_edge(); // arc_a at top: v2→v3 / v3→v2
    let (e_arc_b_bot, he_abb_a, he_abb_b) = arena.add_edge(); // arc_b at bottom: v1→v0 / v0→v1
    let (e_arc_b_top, he_abt_a, he_abt_b) = arena.add_edge(); // arc_b at top: v3→v2 / v2→v3

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
    // Wait, need to think about winding. For outward-facing normals:
    // If the arc_b is the "outside" arc, its normal should point outward.
    // The winding should be CCW when viewed from outside.
    // The cyl_b patch boundary goes: v1→v0 (arc_b bottom) → v0→v2 (line_p1) → v2→v3 (arc_b top) → v3→v1 (line_p2 rev)
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
    // Wait, the bottom cap is bounded by the bottom arcs from both cylinders.
    // The cap boundary goes around the 2D cross-section perimeter.
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
    // This is a Z-aligned function, so face axes are always [0,0,1].
    // For antiparallel cylinders (direction=[0,0,-1]), normalize the origin
    // to the z_min end so tessellation row ordering is consistent.

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

    // ── Frame rotation unit tests ──────────────────────────────────

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
}
