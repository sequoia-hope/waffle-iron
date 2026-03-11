//! WaffleKernel — clean-sheet B-Rep kernel.
//!
//! Supports: make_faces_from_profiles → extrude_face/revolve_face → tessellate/extract_edges → introspect.
//! Boolean ops (box-box). Fillet, chamfer, shell remain NotSupported.

use crate::geometry::curve::{Arc3D, Circle3D, CurveGeom, Line3D};
use crate::geometry::point::{Point3, Vector3};
use crate::geometry::surface::{Cylinder, Plane, SurfaceGeom};
use crate::tessellation;
use crate::topology::arena::TopoArena;
use crate::topology::euler_ops::{mef, mev, mvfs};
use crate::topology::half_edge::*;
use crate::traits::{Kernel, KernelIntrospect};
use crate::types::*;
use crate::units::TAU_NORMALIZE;
use crate::vecmath::*;
use std::collections::HashMap;

/// Clean-sheet geometry kernel with half-edge B-Rep topology.
pub struct WaffleKernel {
    next_id: u64,
    next_handle: u64,
    solids: HashMap<u64, WaffleSolid>,
    standalone_faces: HashMap<u64, StandaloneFace>,
}

/// A full B-Rep solid with topology arena and geometry maps.
pub(crate) struct WaffleSolid {
    pub(crate) arena: TopoArena,
    pub(crate) face_map: HashMap<u64, FaceIdx>,
    pub(crate) edge_map: HashMap<u64, EdgeIdx>,
    pub(crate) vertex_map: HashMap<u64, VertexIdx>,
    pub(crate) face_geometry: HashMap<FaceIdx, SurfaceGeom>,
    pub(crate) edge_geometry: HashMap<EdgeIdx, CurveGeom>,
    pub(crate) cylinder_params: Option<CylinderParams>,
    pub(crate) revolve_params: Option<RevolveParams>,
}

/// Parameters for cylinder tessellation (stored after extrude_circle).
pub(crate) struct CylinderParams {
    pub center_bottom: [f64; 3],
    pub radius: f64,
    pub x_axis: [f64; 3],
    pub y_axis: [f64; 3],
    pub direction: [f64; 3],
    pub depth: f64,
}

/// Parameters for revolve tessellation (stored after revolve_polygon).
pub(crate) struct RevolveParams {
    pub axis_origin: [f64; 3],
    pub axis_dir: [f64; 3],
    pub angle_rad: f64,
    /// Per lateral face: (FaceIdx, start_vertex_3d, end_vertex_3d)
    pub lateral_faces: Vec<(FaceIdx, [f64; 3], [f64; 3])>,
}

/// Circle geometry stored in a standalone face (pre-extrude).
struct CircleInfo {
    center_3d: [f64; 3],
    radius: f64,
    x_axis: [f64; 3],
    y_axis: [f64; 3],
}

/// A standalone face (pre-extrude), stored as either polygon vertices or circle info.
struct StandaloneFace {
    vertices: Vec<[f64; 3]>,
    plane_origin: [f64; 3],
    plane_normal: [f64; 3],
    circle_info: Option<CircleInfo>,
}

impl WaffleKernel {
    pub fn new() -> Self {
        Self {
            next_id: 1,
            next_handle: 1,
            solids: HashMap::new(),
            standalone_faces: HashMap::new(),
        }
    }

    fn alloc_id(&mut self) -> u64 {
        let id = self.next_id;
        self.next_id += 1;
        id
    }

    fn alloc_handle(&mut self) -> u64 {
        let h = self.next_handle;
        self.next_handle += 1;
        h
    }

    /// Execute a boolean operation on two box solids.
    fn do_boolean(
        &mut self,
        a: &KernelSolidHandle,
        b: &KernelSolidHandle,
        op: crate::boolean::BoolOp,
    ) -> Result<KernelSolidHandle, KernelError> {
        let solid_a = self
            .solids
            .get(&a.id())
            .ok_or(KernelError::EntityNotFound {
                id: KernelId(a.id()),
            })?;
        let solid_b = self
            .solids
            .get(&b.id())
            .ok_or(KernelError::EntityNotFound {
                id: KernelId(b.id()),
            })?;

        // Guard: revolve solids not supported for booleans
        if solid_a.revolve_params.is_some() || solid_b.revolve_params.is_some() {
            return Err(KernelError::NotSupported {
                operation: "boolean on revolve solids".to_string(),
            });
        }

        let mut next_id = self.next_id;
        let mut id_alloc = || {
            let id = next_id;
            next_id += 1;
            id
        };

        // Dispatch: use SSI pipeline for cylinders, polygon clipping for box-box
        let result = if solid_a.cylinder_params.is_some() || solid_b.cylinder_params.is_some() {
            crate::boolean::ssi_boolean_op(solid_a, solid_b, op, &mut id_alloc)?
        } else {
            crate::boolean::boolean_op(
                solid_a,
                solid_b,
                op,
                &BooleanOptions::default(),
                &mut id_alloc,
            )?
        };
        self.next_id = next_id;

        let handle_id = self.alloc_handle();
        self.solids.insert(
            handle_id,
            WaffleSolid {
                arena: result.arena,
                face_map: result.face_map,
                edge_map: result.edge_map,
                vertex_map: result.vertex_map,
                face_geometry: result.face_geometry,
                edge_geometry: result.edge_geometry,
                cylinder_params: None,
                revolve_params: None,
            },
        );

        Ok(KernelSolidHandle(handle_id))
    }

    /// Revolve a polygon profile around an axis to create a solid with analytic surfaces.
    fn revolve_polygon(
        &mut self,
        standalone: &StandaloneFace,
        axis_origin: [f64; 3],
        axis_direction: [f64; 3],
        angle_deg: f64,
    ) -> Result<KernelSolidHandle, KernelError> {
        let angle_rad = angle_deg.to_radians();
        let axis_dir = v3_normalize(axis_direction);
        let n = standalone.vertices.len();
        let tau_model = 1e-7;

        // Validate all profile edges are axis-aligned (constant radius OR constant height)
        for i in 0..n {
            let v_a = standalone.vertices[i];
            let v_b = standalone.vertices[(i + 1) % n];
            let r_a = {
                let v = v3_sub(v_a, axis_origin);
                let proj = v3_scale(axis_dir, v3_dot(v, axis_dir));
                v3_length(v3_sub(v, proj))
            };
            let r_b = {
                let v = v3_sub(v_b, axis_origin);
                let proj = v3_scale(axis_dir, v3_dot(v, axis_dir));
                v3_length(v3_sub(v, proj))
            };
            let h_a = v3_dot(v3_sub(v_a, axis_origin), axis_dir);
            let h_b = v3_dot(v3_sub(v_b, axis_origin), axis_dir);
            let same_radius = (r_a - r_b).abs() < tau_model;
            let same_height = (h_a - h_b).abs() < tau_model;
            if !same_radius && !same_height {
                return Err(KernelError::NotSupported {
                    operation: "revolve: profile edge neither radial nor axial".to_string(),
                });
            }
        }

        // Compute start (angle=0) and end (angle=angle_rad) vertex positions
        let start_verts: Vec<[f64; 3]> = standalone.vertices.clone();
        let end_verts: Vec<[f64; 3]> = start_verts
            .iter()
            .map(|&v| rotate_point_around_axis(v, axis_origin, axis_dir, angle_rad))
            .collect();

        let mut arena = TopoArena::new();

        // Phase 1: Build start cap polygon using Euler ops
        let (_, _, face0, v_start_0) = mvfs(&mut arena, start_verts[0]);
        let loop0 = arena.faces[face0.0].outer_loop;

        let mut bottom_verts = vec![v_start_0];
        for i in 1..n {
            let (_, vi) = mev(&mut arena, bottom_verts[i - 1], loop0, start_verts[i]);
            bottom_verts.push(vi);
        }

        // Close the start cap: connect last vertex back to first
        let (_close_edge, start_cap_face) =
            mef(&mut arena, bottom_verts[n - 1], bottom_verts[0], loop0);

        // Fix stale vertex half_edge pointers (same pattern as extrude)
        {
            let start_he = arena.loops[loop0.0].half_edge;
            let mut he = start_he;
            loop {
                let v = arena.half_edges[he.0].origin;
                arena.vertices[v.0].half_edge = Some(he);
                he = arena.half_edges[he.0].next;
                if he == start_he {
                    break;
                }
            }
        }

        // Phase 2: Create end cap vertices by extending from start vertices
        let mut top_verts = Vec::with_capacity(n);
        #[allow(clippy::needless_range_loop)]
        for i in 0..n {
            let (_, tv) = mev(&mut arena, bottom_verts[i], loop0, end_verts[i]);
            top_verts.push(tv);
        }

        // Phase 3: Create lateral faces by connecting consecutive end vertices
        let mut side_faces = Vec::with_capacity(n);
        for i in 0..n {
            let next = (i + 1) % n;
            let (_, sf) = mef(&mut arena, top_verts[i], top_verts[next], loop0);
            side_faces.push(sf);
        }

        // Build maps: allocate KernelIds for all topology entities
        let handle_id = self.alloc_handle();
        let mut face_map = HashMap::new();
        let mut edge_map = HashMap::new();
        let mut vertex_map = HashMap::new();
        let mut face_geometry = HashMap::new();
        let mut edge_geometry = HashMap::new();
        let mut lateral_face_data = Vec::new();

        // Compute rotated profile normal for end cap
        let rotated_normal = rotate_point_around_axis(
            v3_add(axis_origin, standalone.plane_normal),
            axis_origin,
            axis_dir,
            angle_rad,
        );
        let end_cap_normal = v3_sub(rotated_normal, axis_origin);

        // Start cap face (start_cap_face)
        let start_cap_kid = self.alloc_id();
        face_map.insert(start_cap_kid, start_cap_face);
        face_geometry.insert(
            start_cap_face,
            SurfaceGeom::Planar(Plane {
                origin: Point3::from_array(standalone.plane_origin),
                normal: Vector3::from_array(v3_negate(standalone.plane_normal)),
            }),
        );

        // End cap face (face0, the residual face after all mef splits)
        let end_cap_kid = self.alloc_id();
        face_map.insert(end_cap_kid, face0);
        let end_cap_origin =
            rotate_point_around_axis(standalone.plane_origin, axis_origin, axis_dir, angle_rad);
        face_geometry.insert(
            face0,
            SurfaceGeom::Planar(Plane {
                origin: Point3::from_array(end_cap_origin),
                normal: Vector3::from_array(end_cap_normal),
            }),
        );

        // Lateral faces with geometry assignment
        #[allow(clippy::needless_range_loop)]
        for i in 0..n {
            let sf = side_faces[i];
            let sf_kid = self.alloc_id();
            face_map.insert(sf_kid, sf);

            let v_a = start_verts[i];
            let v_b = start_verts[(i + 1) % n];

            // Compute radius and height for this profile edge
            let r_a = {
                let v = v3_sub(v_a, axis_origin);
                let proj = v3_scale(axis_dir, v3_dot(v, axis_dir));
                v3_length(v3_sub(v, proj))
            };
            let r_b = {
                let v = v3_sub(v_b, axis_origin);
                let proj = v3_scale(axis_dir, v3_dot(v, axis_dir));
                v3_length(v3_sub(v, proj))
            };
            let h_a = v3_dot(v3_sub(v_a, axis_origin), axis_dir);
            let h_b = v3_dot(v3_sub(v_b, axis_origin), axis_dir);

            if (r_a - r_b).abs() < tau_model {
                // Cylindrical face at constant radius
                let radius = (r_a + r_b) / 2.0;
                let avg_height = (h_a + h_b) / 2.0;
                let cyl_origin = v3_add(axis_origin, v3_scale(axis_dir, avg_height));
                face_geometry.insert(
                    sf,
                    SurfaceGeom::Cylindrical(Cylinder {
                        origin: Point3::from_array(cyl_origin),
                        axis: Vector3::from_array(axis_dir),
                        radius,
                    }),
                );
            } else {
                // Planar face at constant height
                let height = (h_a + h_b) / 2.0;
                let plane_origin = v3_add(axis_origin, v3_scale(axis_dir, height));
                // Normal: +axis_dir if face is top, -axis_dir if bottom
                // Check which direction faces outward by comparing to centroid
                let centroid = polygon_centroid(&start_verts);
                let centroid_height = v3_dot(v3_sub(centroid, axis_origin), axis_dir);
                let normal = if height > centroid_height {
                    axis_dir
                } else {
                    v3_negate(axis_dir)
                };
                face_geometry.insert(
                    sf,
                    SurfaceGeom::Planar(Plane {
                        origin: Point3::from_array(plane_origin),
                        normal: Vector3::from_array(normal),
                    }),
                );
            }

            lateral_face_data.push((sf, v_a, v_b));
        }

        // Map all edges and assign geometry
        for (idx, _edge) in arena.edges.iter().enumerate() {
            let eid = self.alloc_id();
            edge_map.insert(eid, EdgeIdx(idx));

            let he_a = arena.edges[idx].half_edge;
            let v_start = arena.half_edges[he_a.0].origin;
            let v_end = arena.half_edges[arena.half_edges[he_a.0].twin.0].origin;
            let p0 = arena.vertices[v_start.0].position;
            let p1 = arena.vertices[v_end.0].position;

            // Determine if this is an arc edge (connects start vertex to corresponding end vertex)
            let is_arc = bottom_verts
                .iter()
                .zip(top_verts.iter())
                .any(|(&bv, &tv)| (v_start == bv && v_end == tv) || (v_start == tv && v_end == bv));

            if is_arc {
                // Find the start position (on the start cap)
                let (arc_start, arc_center_radius) = if bottom_verts.contains(&v_start) {
                    let start_pos = p0;
                    let v = v3_sub(start_pos, axis_origin);
                    let proj = v3_scale(axis_dir, v3_dot(v, axis_dir));
                    let radial = v3_sub(v, proj);
                    let radius = v3_length(radial);
                    let center = v3_add(axis_origin, proj);
                    (start_pos, (center, radius))
                } else {
                    // v_end is the start cap vertex
                    let start_pos = p1;
                    let v = v3_sub(start_pos, axis_origin);
                    let proj = v3_scale(axis_dir, v3_dot(v, axis_dir));
                    let radial = v3_sub(v, proj);
                    let radius = v3_length(radial);
                    let center = v3_add(axis_origin, proj);
                    (start_pos, (center, radius))
                };
                let (center, radius) = arc_center_radius;
                edge_geometry.insert(
                    EdgeIdx(idx),
                    CurveGeom::Arc(Arc3D {
                        center: Point3::from_array(center),
                        normal: Vector3::from_array(axis_dir),
                        radius,
                        start_point: Point3::from_array(arc_start),
                        sweep_angle: angle_rad,
                    }),
                );
            } else {
                // Linear edge (cap edge)
                let dir = v3_sub(p1, p0);
                edge_geometry.insert(
                    EdgeIdx(idx),
                    CurveGeom::Linear(Line3D {
                        origin: Point3::from_array(p0),
                        direction: Vector3::from_array(dir),
                    }),
                );
            }
        }

        // Map all vertices
        for (idx, _) in arena.vertices.iter().enumerate() {
            let vid = self.alloc_id();
            vertex_map.insert(vid, VertexIdx(idx));
        }

        let revolve_params = RevolveParams {
            axis_origin,
            axis_dir,
            angle_rad,
            lateral_faces: lateral_face_data,
        };

        self.solids.insert(
            handle_id,
            WaffleSolid {
                arena,
                face_map,
                edge_map,
                vertex_map,
                face_geometry,
                edge_geometry,
                cylinder_params: None,
                revolve_params: Some(revolve_params),
            },
        );

        Ok(KernelSolidHandle(handle_id))
    }

    /// Build a true cylinder B-Rep: 2 vertices, 3 edges, 3 faces.
    fn extrude_circle(
        &mut self,
        circle_info: &CircleInfo,
        standalone: &StandaloneFace,
        direction: [f64; 3],
        depth: f64,
    ) -> Result<KernelSolidHandle, KernelError> {
        let center = circle_info.center_3d;
        let r = circle_info.radius;
        let x_axis = circle_info.x_axis;
        let y_axis = circle_info.y_axis;
        let dir_norm = v3_normalize(direction);

        // Seam point positions
        let bottom_seam = v3_add(center, v3_scale(x_axis, r));
        let top_center = v3_add(center, v3_scale(dir_norm, depth));
        let top_seam = v3_add(bottom_seam, v3_scale(dir_norm, depth));

        let mut arena = TopoArena::new();

        // Build topology: 1 solid, 1 shell, 3 faces, 3 loops, 2 vertices, 3 edges (6 half-edges)
        let solid_idx = arena.add_solid();
        let shell_idx = arena.add_shell(solid_idx);
        arena.solids[solid_idx.0].outer_shell = shell_idx;

        // 3 faces
        let bottom_face_idx = arena.add_face(shell_idx);
        let top_face_idx = arena.add_face(shell_idx);
        let side_face_idx = arena.add_face(shell_idx);
        arena.shells[shell_idx.0].face = bottom_face_idx;

        // 3 loops (one per face)
        let bottom_loop = arena.add_loop(bottom_face_idx);
        let top_loop = arena.add_loop(top_face_idx);
        let side_loop = arena.add_loop(side_face_idx);
        arena.faces[bottom_face_idx.0].outer_loop = bottom_loop;
        arena.faces[top_face_idx.0].outer_loop = top_loop;
        arena.faces[side_face_idx.0].outer_loop = side_loop;

        // 2 vertices
        let v_bottom = arena.add_vertex(bottom_seam);
        let v_top = arena.add_vertex(top_seam);

        // 3 edges (each creates 2 half-edges)
        let (e_bottom, he_bot_a, he_bot_b) = arena.add_edge();
        let (e_top, he_top_a, he_top_b) = arena.add_edge();
        let (e_seam, he_seam_a, he_seam_b) = arena.add_edge();

        // Wire bottom cap: he_bot_a is a self-loop in bottom_loop
        arena.half_edges[he_bot_a.0].origin = v_bottom;
        arena.half_edges[he_bot_a.0].next = he_bot_a;
        arena.half_edges[he_bot_a.0].prev = he_bot_a;
        arena.half_edges[he_bot_a.0].loop_ = bottom_loop;
        arena.loops[bottom_loop.0].half_edge = he_bot_a;

        // Wire top cap: he_top_a is a self-loop in top_loop
        arena.half_edges[he_top_a.0].origin = v_top;
        arena.half_edges[he_top_a.0].next = he_top_a;
        arena.half_edges[he_top_a.0].prev = he_top_a;
        arena.half_edges[he_top_a.0].loop_ = top_loop;
        arena.loops[top_loop.0].half_edge = he_top_a;

        // Wire side face: he_bot_b → he_seam_a → he_top_b → he_seam_b → (cycle)
        arena.half_edges[he_bot_b.0].origin = v_bottom;
        arena.half_edges[he_bot_b.0].next = he_seam_a;
        arena.half_edges[he_bot_b.0].prev = he_seam_b;
        arena.half_edges[he_bot_b.0].loop_ = side_loop;

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

        arena.loops[side_loop.0].half_edge = he_bot_b;

        // Set vertex half-edge references
        arena.vertices[v_bottom.0].half_edge = Some(he_bot_a);
        arena.vertices[v_top.0].half_edge = Some(he_top_a);

        // Build maps and geometry
        let handle_id = self.alloc_handle();
        let mut face_map = HashMap::new();
        let mut edge_map = HashMap::new();
        let mut vertex_map = HashMap::new();
        let mut face_geometry = HashMap::new();
        let mut edge_geometry = HashMap::new();

        // Face geometry
        let bottom_kid = self.alloc_id();
        face_map.insert(bottom_kid, bottom_face_idx);
        face_geometry.insert(
            bottom_face_idx,
            SurfaceGeom::Planar(Plane {
                origin: Point3::from_array(center),
                normal: Vector3::from_array(v3_negate(dir_norm)),
            }),
        );

        let top_kid = self.alloc_id();
        face_map.insert(top_kid, top_face_idx);
        face_geometry.insert(
            top_face_idx,
            SurfaceGeom::Planar(Plane {
                origin: Point3::from_array(top_center),
                normal: Vector3::from_array(dir_norm),
            }),
        );

        let side_kid = self.alloc_id();
        face_map.insert(side_kid, side_face_idx);
        face_geometry.insert(
            side_face_idx,
            SurfaceGeom::Cylindrical(Cylinder {
                origin: Point3::from_array(center),
                axis: Vector3::from_array(dir_norm),
                radius: r,
            }),
        );

        // Edge geometry
        let bot_edge_kid = self.alloc_id();
        edge_map.insert(bot_edge_kid, e_bottom);
        edge_geometry.insert(
            e_bottom,
            CurveGeom::Circular(Circle3D {
                center: Point3::from_array(center),
                normal: Vector3::from_array(standalone.plane_normal),
                radius: r,
            }),
        );

        let top_edge_kid = self.alloc_id();
        edge_map.insert(top_edge_kid, e_top);
        edge_geometry.insert(
            e_top,
            CurveGeom::Circular(Circle3D {
                center: Point3::from_array(top_center),
                normal: Vector3::from_array(standalone.plane_normal),
                radius: r,
            }),
        );

        let seam_edge_kid = self.alloc_id();
        edge_map.insert(seam_edge_kid, e_seam);
        edge_geometry.insert(
            e_seam,
            CurveGeom::Linear(Line3D {
                origin: Point3::from_array(bottom_seam),
                direction: Vector3::from_array(v3_scale(dir_norm, depth)),
            }),
        );

        // Vertex map
        let vbot_kid = self.alloc_id();
        vertex_map.insert(vbot_kid, v_bottom);
        let vtop_kid = self.alloc_id();
        vertex_map.insert(vtop_kid, v_top);

        let cylinder_params = CylinderParams {
            center_bottom: center,
            radius: r,
            x_axis,
            y_axis,
            direction: dir_norm,
            depth,
        };

        self.solids.insert(
            handle_id,
            WaffleSolid {
                arena,
                face_map,
                edge_map,
                vertex_map,
                face_geometry,
                edge_geometry,
                cylinder_params: Some(cylinder_params),
                revolve_params: None,
            },
        );

        Ok(KernelSolidHandle(handle_id))
    }
}

impl Default for WaffleKernel {
    fn default() -> Self {
        Self::new()
    }
}

// ── Helper geometry functions ────────────────────────────────────────────

/// Rotate a point around an axis (Rodrigues' rotation formula).
fn rotate_point_around_axis(
    point: [f64; 3],
    axis_origin: [f64; 3],
    axis_dir: [f64; 3],
    angle_rad: f64,
) -> [f64; 3] {
    let v = v3_sub(point, axis_origin);
    let k = axis_dir; // must be normalized
    let cos_a = angle_rad.cos();
    let sin_a = angle_rad.sin();
    let k_dot_v = v3_dot(k, v);
    let k_cross_v = v3_cross(k, v);
    // v_rot = v*cos(a) + (k x v)*sin(a) + k*(k.v)*(1 - cos(a))
    let rotated = v3_add(
        v3_add(v3_scale(v, cos_a), v3_scale(k_cross_v, sin_a)),
        v3_scale(k, k_dot_v * (1.0 - cos_a)),
    );
    v3_add(axis_origin, rotated)
}

/// Compute polygon area using cross product magnitudes (works in 3D).
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

/// Compute centroid of a polygon.
fn polygon_centroid(verts: &[[f64; 3]]) -> [f64; 3] {
    let n = verts.len() as f64;
    let mut c = [0.0, 0.0, 0.0];
    for v in verts {
        c[0] += v[0];
        c[1] += v[1];
        c[2] += v[2];
    }
    [c[0] / n, c[1] / n, c[2] / n]
}

/// Compute AABB from a set of vertices.
fn compute_bbox(verts: &[[f64; 3]]) -> [f64; 6] {
    let mut min = [f64::MAX; 3];
    let mut max = [f64::MIN; 3];
    for v in verts {
        for i in 0..3 {
            if v[i] < min[i] {
                min[i] = v[i];
            }
            if v[i] > max[i] {
                max[i] = v[i];
            }
        }
    }
    [min[0], min[1], min[2], max[0], max[1], max[2]]
}

// ── Kernel trait implementation ──────────────────────────────────────────

impl Kernel for WaffleKernel {
    fn make_faces_from_profiles(
        &mut self,
        profiles: &[ClosedProfile],
        plane_origin: [f64; 3],
        plane_normal: [f64; 3],
        plane_x_axis: [f64; 3],
        positions: &HashMap<u32, (f64, f64)>,
    ) -> Result<Vec<KernelId>, KernelError> {
        // Compute plane Y axis from normal x X-axis
        let plane_y_axis = v3_cross(plane_normal, plane_x_axis);

        let mut face_ids = Vec::new();

        for profile in profiles {
            if let Some(ref circle) = profile.circle {
                // Circle profile path
                if circle.radius <= 0.0 {
                    return Err(KernelError::Other {
                        message: format!("Circle radius must be positive, got {}", circle.radius),
                    });
                }
                let center_3d = v3_add(
                    plane_origin,
                    v3_add(
                        v3_scale(plane_x_axis, circle.center_u),
                        v3_scale(plane_y_axis, circle.center_v),
                    ),
                );
                let face_id = self.alloc_id();
                self.standalone_faces.insert(
                    face_id,
                    StandaloneFace {
                        vertices: vec![],
                        plane_origin,
                        plane_normal,
                        circle_info: Some(CircleInfo {
                            center_3d,
                            radius: circle.radius,
                            x_axis: plane_x_axis,
                            y_axis: plane_y_axis,
                        }),
                    },
                );
                face_ids.push(KernelId(face_id));
                continue;
            }

            // Polygon profile path — prefer vertex_ids, fall back to entity_ids, then sorted keys.
            let keys: Vec<u32> = if !profile.vertex_ids.is_empty()
                && profile
                    .vertex_ids
                    .iter()
                    .all(|id| positions.contains_key(id))
            {
                profile.vertex_ids.clone()
            } else if !profile.entity_ids.is_empty()
                && profile
                    .entity_ids
                    .iter()
                    .all(|id| positions.contains_key(id))
            {
                profile.entity_ids.clone()
            } else {
                let mut k: Vec<u32> = positions.keys().copied().collect();
                k.sort();
                k
            };

            if keys.len() < 3 {
                return Err(KernelError::Other {
                    message: format!("Need at least 3 vertices for a polygon, got {}", keys.len()),
                });
            }

            // Convert 2D sketch coords → 3D world coords
            let vertices_3d: Vec<[f64; 3]> = keys
                .iter()
                .map(|k| {
                    let (u, v) = positions[k];
                    v3_add(
                        plane_origin,
                        v3_add(v3_scale(plane_x_axis, u), v3_scale(plane_y_axis, v)),
                    )
                })
                .collect();

            // Validate non-zero area
            let area = polygon_area_3d(&vertices_3d);
            if area < TAU_NORMALIZE {
                return Err(KernelError::Other {
                    message: "Profile has zero area".to_string(),
                });
            }

            let face_id = self.alloc_id();
            self.standalone_faces.insert(
                face_id,
                StandaloneFace {
                    vertices: vertices_3d,
                    plane_origin,
                    plane_normal,
                    circle_info: None,
                },
            );
            face_ids.push(KernelId(face_id));
        }

        Ok(face_ids)
    }

    fn extrude_face(
        &mut self,
        face: KernelId,
        direction: [f64; 3],
        depth: f64,
    ) -> Result<KernelSolidHandle, KernelError> {
        if depth <= 0.0 {
            return Err(KernelError::Other {
                message: "extrude depth must be positive".to_string(),
            });
        }

        let standalone = self
            .standalone_faces
            .remove(&face.0)
            .ok_or(KernelError::EntityNotFound { id: face })?;

        // Dispatch: circle or polygon extrude
        if let Some(ref circle_info) = standalone.circle_info {
            return self.extrude_circle(circle_info, &standalone, direction, depth);
        }

        let n = standalone.vertices.len();
        let offset = v3_scale(direction, depth);

        let mut arena = TopoArena::new();

        // Phase 1: Build bottom face polygon using Euler ops
        let (_, _, face0, v_bottom_0) = mvfs(&mut arena, standalone.vertices[0]);
        let loop0 = arena.faces[face0.0].outer_loop;

        let mut bottom_verts = vec![v_bottom_0];
        for i in 1..n {
            let (_, vi) = mev(
                &mut arena,
                bottom_verts[i - 1],
                loop0,
                standalone.vertices[i],
            );
            bottom_verts.push(vi);
        }

        // Close the bottom face: connect last vertex back to first
        let (_close_edge, bottom_face) =
            mef(&mut arena, bottom_verts[n - 1], bottom_verts[0], loop0);

        // After mef, loop0 is the "outer" face (face0) loop going around the bottom polygon CCW.
        // bottom_face has its own loop going CW (inner/bottom).

        // CRITICAL: After mef splits the loop, some vertex.half_edge pointers are stale
        // (pointing into the bottom_face loop instead of loop0). Fix them by walking loop0
        // and updating each vertex's half_edge to reference its outgoing half-edge in loop0.
        {
            let start_he = arena.loops[loop0.0].half_edge;
            let mut he = start_he;
            loop {
                let v = arena.half_edges[he.0].origin;
                arena.vertices[v.0].half_edge = Some(he);
                he = arena.half_edges[he.0].next;
                if he == start_he {
                    break;
                }
            }
        }

        // Phase 2: Create top vertices by extending from bottom vertices
        let mut top_verts = Vec::with_capacity(n);
        #[allow(clippy::needless_range_loop)]
        for i in 0..n {
            let top_pos = v3_add(standalone.vertices[i], offset);
            let (_, tv) = mev(&mut arena, bottom_verts[i], loop0, top_pos);
            top_verts.push(tv);
        }

        // Phase 3: Create side faces by connecting consecutive top vertices with mef.
        // After the mev calls, loop0 has wire edges sticking up from each bottom vertex.
        // We connect consecutive top vertices to form side quads.
        // For each side i: connect top_verts[i] to top_verts[(i+1) % n]
        // The last mef closes the top face.
        let mut side_faces = Vec::with_capacity(n);
        for i in 0..n {
            let next = (i + 1) % n;
            let (_, sf) = mef(&mut arena, top_verts[i], top_verts[next], loop0);
            side_faces.push(sf);
        }
        // After all n mef calls, the last one closes the top face.
        // The faces created are: side_faces[0..n-1] are side faces, side_faces[n-1] is the top face.
        // Actually, each mef creates a new face from the split. The last mef should close loop0
        // into the top face. Let me think...
        // After n mef calls, we have n new faces (the sides) plus the original face0 becomes the top.
        // Actually face0 is the "infinite" outer face from mvfs. After all mef splits, loop0 shrinks
        // to just the top polygon, making face0 effectively the top face.

        // Assign face roles:
        // bottom_face = the bottom face
        // face0 = becomes the top face after all the side mef splits
        // side_faces[0..n] = n side faces
        // Wait, we have n side_faces from n mef calls. But the last mef should close the
        // top. Let me count: Each mef creates one new face. After n mef calls for sides,
        // we've created n new faces (side faces), and the residual loop0 face (face0) is the top.

        // Build maps: allocate KernelIds for all topology entities
        let handle_id = self.alloc_handle();
        let mut face_map = HashMap::new();
        let mut edge_map = HashMap::new();
        let mut vertex_map = HashMap::new();
        let mut face_geometry = HashMap::new();
        let mut edge_geometry = HashMap::new();

        let dir_norm = v3_normalize(direction);

        // Bottom face
        let bottom_face_kid = self.alloc_id();
        face_map.insert(bottom_face_kid, bottom_face);
        face_geometry.insert(
            bottom_face,
            SurfaceGeom::Planar(Plane {
                origin: Point3::from_array(standalone.plane_origin),
                normal: Vector3::from_array(v3_negate(dir_norm)),
            }),
        );

        // Top face (face0 after all splits)
        let top_face_kid = self.alloc_id();
        face_map.insert(top_face_kid, face0);
        let top_origin = v3_add(standalone.plane_origin, offset);
        face_geometry.insert(
            face0,
            SurfaceGeom::Planar(Plane {
                origin: Point3::from_array(top_origin),
                normal: Vector3::from_array(dir_norm),
            }),
        );

        // Side faces with outward normals
        let center_bottom = polygon_centroid(&standalone.vertices);
        #[allow(clippy::needless_range_loop)]
        for i in 0..n {
            let sf = side_faces[i];
            let sf_kid = self.alloc_id();
            face_map.insert(sf_kid, sf);

            // Compute outward normal for this side face
            let v_a = standalone.vertices[i];
            let v_b = standalone.vertices[(i + 1) % n];
            let edge_dir = v3_sub(v_b, v_a);
            // Outward normal = cross(edge_dir, extrude_dir), possibly negated
            let mut side_normal = v3_normalize(v3_cross(edge_dir, direction));
            // Check it points outward (away from center)
            let mid = v3_scale(v3_add(v_a, v_b), 0.5);
            let to_center = v3_sub(center_bottom, mid);
            if v3_dot(side_normal, to_center) > 0.0 {
                side_normal = v3_negate(side_normal);
            }

            face_geometry.insert(
                sf,
                SurfaceGeom::Planar(Plane {
                    origin: Point3::from_array(mid),
                    normal: Vector3::from_array(side_normal),
                }),
            );
        }

        // Map all edges
        for (idx, _edge) in arena.edges.iter().enumerate() {
            let eid = self.alloc_id();
            edge_map.insert(eid, EdgeIdx(idx));

            // Compute edge geometry (linear)
            let he_a = arena.edges[idx].half_edge;
            let v_start = arena.half_edges[he_a.0].origin;
            let v_end = arena.half_edges[arena.half_edges[he_a.0].twin.0].origin;
            let p0 = arena.vertices[v_start.0].position;
            let p1 = arena.vertices[v_end.0].position;
            let dir = v3_sub(p1, p0);
            edge_geometry.insert(
                EdgeIdx(idx),
                CurveGeom::Linear(Line3D {
                    origin: Point3::from_array(p0),
                    direction: Vector3::from_array(dir),
                }),
            );
        }

        // Map all vertices
        for (idx, _) in arena.vertices.iter().enumerate() {
            let vid = self.alloc_id();
            vertex_map.insert(vid, VertexIdx(idx));
        }

        self.solids.insert(
            handle_id,
            WaffleSolid {
                arena,
                face_map,
                edge_map,
                vertex_map,
                face_geometry,
                edge_geometry,
                cylinder_params: None,
                revolve_params: None,
            },
        );

        Ok(KernelSolidHandle(handle_id))
    }

    fn tessellate(
        &mut self,
        solid: &KernelSolidHandle,
        _tolerance: f64,
    ) -> Result<RenderMesh, KernelError> {
        let ws = self
            .solids
            .get(&solid.id())
            .ok_or(KernelError::EntityNotFound {
                id: KernelId(solid.id()),
            })?;

        tessellation::tessellate_solid(
            &ws.arena,
            &ws.face_map,
            &ws.face_geometry,
            &ws.edge_geometry,
            ws.cylinder_params.as_ref(),
            ws.revolve_params.as_ref(),
        )
    }

    fn extract_edges(
        &mut self,
        solid: &KernelSolidHandle,
        _tolerance: f64,
    ) -> Result<EdgeRenderData, KernelError> {
        let ws = self
            .solids
            .get(&solid.id())
            .ok_or(KernelError::EntityNotFound {
                id: KernelId(solid.id()),
            })?;

        tessellation::extract_edges(&ws.arena, &ws.edge_map, &ws.edge_geometry)
    }

    fn revolve_face(
        &mut self,
        face: KernelId,
        axis_origin: [f64; 3],
        axis_direction: [f64; 3],
        angle: f64,
    ) -> Result<KernelSolidHandle, KernelError> {
        // Validate angle (API receives degrees)
        if angle <= 0.0 {
            return Err(KernelError::Other {
                message: format!("revolve angle must be positive, got {}", angle),
            });
        }
        if angle >= 360.0 {
            return Err(KernelError::NotSupported {
                operation: "revolve: full 360° revolution".to_string(),
            });
        }

        let standalone = self
            .standalone_faces
            .remove(&face.0)
            .ok_or(KernelError::EntityNotFound { id: face })?;

        // Circle profile → NotSupported
        if standalone.circle_info.is_some() {
            return Err(KernelError::NotSupported {
                operation: "revolve: circle profile (torus)".to_string(),
            });
        }

        self.revolve_polygon(&standalone, axis_origin, axis_direction, angle)
    }

    fn boolean_union(
        &mut self,
        a: &KernelSolidHandle,
        b: &KernelSolidHandle,
    ) -> Result<KernelSolidHandle, KernelError> {
        self.do_boolean(a, b, crate::boolean::BoolOp::Union)
    }

    fn boolean_subtract(
        &mut self,
        a: &KernelSolidHandle,
        b: &KernelSolidHandle,
    ) -> Result<KernelSolidHandle, KernelError> {
        self.do_boolean(a, b, crate::boolean::BoolOp::Subtract)
    }

    fn boolean_intersect(
        &mut self,
        a: &KernelSolidHandle,
        b: &KernelSolidHandle,
    ) -> Result<KernelSolidHandle, KernelError> {
        self.do_boolean(a, b, crate::boolean::BoolOp::Intersect)
    }

    fn fillet_edges(
        &mut self,
        _solid: &KernelSolidHandle,
        _edges: &[KernelId],
        _radius: f64,
    ) -> Result<KernelSolidHandle, KernelError> {
        Err(KernelError::NotSupported {
            operation: "fillet_edges".to_string(),
        })
    }

    fn chamfer_edges(
        &mut self,
        _solid: &KernelSolidHandle,
        _edges: &[KernelId],
        _distance: f64,
    ) -> Result<KernelSolidHandle, KernelError> {
        Err(KernelError::NotSupported {
            operation: "chamfer_edges".to_string(),
        })
    }

    fn shell(
        &mut self,
        _solid: &KernelSolidHandle,
        _faces_to_remove: &[KernelId],
        _thickness: f64,
    ) -> Result<KernelSolidHandle, KernelError> {
        Err(KernelError::NotSupported {
            operation: "shell".to_string(),
        })
    }
}

// ── KernelIntrospect ─────────────────────────────────────────────────────

impl KernelIntrospect for WaffleKernel {
    fn list_faces(&self, solid: &KernelSolidHandle) -> Vec<KernelId> {
        self.solids
            .get(&solid.id())
            .map(|ws| ws.face_map.keys().map(|&k| KernelId(k)).collect())
            .unwrap_or_default()
    }

    fn list_edges(&self, solid: &KernelSolidHandle) -> Vec<KernelId> {
        self.solids
            .get(&solid.id())
            .map(|ws| ws.edge_map.keys().map(|&k| KernelId(k)).collect())
            .unwrap_or_default()
    }

    fn list_vertices(&self, solid: &KernelSolidHandle) -> Vec<KernelId> {
        self.solids
            .get(&solid.id())
            .map(|ws| ws.vertex_map.keys().map(|&k| KernelId(k)).collect())
            .unwrap_or_default()
    }

    fn face_edges(&self, face: KernelId) -> Vec<KernelId> {
        for ws in self.solids.values() {
            if let Some(&face_idx) = ws.face_map.get(&face.0) {
                return collect_face_edge_kids(ws, face_idx);
            }
        }
        vec![]
    }

    fn edge_faces(&self, edge: KernelId) -> Vec<KernelId> {
        for ws in self.solids.values() {
            if let Some(&edge_idx) = ws.edge_map.get(&edge.0) {
                return collect_edge_face_kids(ws, edge_idx);
            }
        }
        vec![]
    }

    fn edge_vertices(&self, edge: KernelId) -> (KernelId, KernelId) {
        for ws in self.solids.values() {
            if let Some(&edge_idx) = ws.edge_map.get(&edge.0) {
                let he_a = ws.arena.edges[edge_idx.0].half_edge;
                let he_b = ws.arena.half_edges[he_a.0].twin;
                let v0 = ws.arena.half_edges[he_a.0].origin;
                let v1 = ws.arena.half_edges[he_b.0].origin;
                let kid0 = reverse_lookup_vertex(ws, v0);
                let kid1 = reverse_lookup_vertex(ws, v1);
                return (kid0, kid1);
            }
        }
        (KernelId(0), KernelId(0))
    }

    fn face_neighbors(&self, face: KernelId) -> Vec<KernelId> {
        for ws in self.solids.values() {
            if let Some(&face_idx) = ws.face_map.get(&face.0) {
                let edge_kids = collect_face_edge_kids(ws, face_idx);
                let mut neighbors = Vec::new();
                for ek in &edge_kids {
                    let fk = self.edge_faces(*ek);
                    for f in fk {
                        if f != face && !neighbors.contains(&f) {
                            neighbors.push(f);
                        }
                    }
                }
                return neighbors;
            }
        }
        vec![]
    }

    fn compute_signature(&self, entity: KernelId, kind: TopoKind) -> TopoSignature {
        for ws in self.solids.values() {
            match kind {
                TopoKind::Face => {
                    if let Some(&face_idx) = ws.face_map.get(&entity.0) {
                        return compute_face_signature(ws, face_idx);
                    }
                }
                TopoKind::Edge => {
                    if let Some(&edge_idx) = ws.edge_map.get(&entity.0) {
                        return compute_edge_signature(ws, edge_idx);
                    }
                }
                TopoKind::Vertex => {
                    if let Some(&vert_idx) = ws.vertex_map.get(&entity.0) {
                        return compute_vertex_signature(ws, vert_idx);
                    }
                }
                _ => {}
            }
        }
        TopoSignature::empty()
    }

    fn compute_all_signatures(
        &self,
        solid: &KernelSolidHandle,
        kind: TopoKind,
    ) -> Vec<(KernelId, TopoSignature)> {
        let ws = match self.solids.get(&solid.id()) {
            Some(ws) => ws,
            None => return vec![],
        };

        match kind {
            TopoKind::Face => ws
                .face_map
                .iter()
                .map(|(&kid, &fidx)| (KernelId(kid), compute_face_signature(ws, fidx)))
                .collect(),
            TopoKind::Edge => ws
                .edge_map
                .iter()
                .map(|(&kid, &eidx)| (KernelId(kid), compute_edge_signature(ws, eidx)))
                .collect(),
            TopoKind::Vertex => ws
                .vertex_map
                .iter()
                .map(|(&kid, &vidx)| (KernelId(kid), compute_vertex_signature(ws, vidx)))
                .collect(),
            _ => vec![],
        }
    }
}

// ── Introspect helpers ───────────────────────────────────────────────────

/// Walk the outer loop of a face, collecting vertices.
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

/// Collect KernelId keys for edges around a face's outer loop.
fn collect_face_edge_kids(ws: &WaffleSolid, face_idx: FaceIdx) -> Vec<KernelId> {
    let loop_idx = ws.arena.faces[face_idx.0].outer_loop;
    let start_he = ws.arena.loops[loop_idx.0].half_edge;
    let mut edge_kids = Vec::new();
    let mut he = start_he;
    loop {
        let edge_idx = ws.arena.half_edges[he.0].edge;
        if let Some(kid) = reverse_lookup_edge_id(ws, edge_idx) {
            if !edge_kids.contains(&KernelId(kid)) {
                edge_kids.push(KernelId(kid));
            }
        }
        he = ws.arena.half_edges[he.0].next;
        if he == start_he {
            break;
        }
    }
    edge_kids
}

/// Collect KernelId keys for faces adjacent to an edge.
fn collect_edge_face_kids(ws: &WaffleSolid, edge_idx: EdgeIdx) -> Vec<KernelId> {
    let he_a = ws.arena.edges[edge_idx.0].half_edge;
    let he_b = ws.arena.half_edges[he_a.0].twin;
    let loop_a = ws.arena.half_edges[he_a.0].loop_;
    let loop_b = ws.arena.half_edges[he_b.0].loop_;
    let face_a = ws.arena.loops[loop_a.0].face;
    let face_b = ws.arena.loops[loop_b.0].face;

    let mut result = Vec::new();
    if let Some(kid) = reverse_lookup_face_id(ws, face_a) {
        result.push(KernelId(kid));
    }
    if face_b != face_a {
        if let Some(kid) = reverse_lookup_face_id(ws, face_b) {
            result.push(KernelId(kid));
        }
    }
    result
}

fn reverse_lookup_edge_id(ws: &WaffleSolid, edge_idx: EdgeIdx) -> Option<u64> {
    ws.edge_map
        .iter()
        .find(|(_, &v)| v == edge_idx)
        .map(|(&k, _)| k)
}

fn reverse_lookup_face_id(ws: &WaffleSolid, face_idx: FaceIdx) -> Option<u64> {
    ws.face_map
        .iter()
        .find(|(_, &v)| v == face_idx)
        .map(|(&k, _)| k)
}

fn reverse_lookup_vertex(ws: &WaffleSolid, vert_idx: VertexIdx) -> KernelId {
    ws.vertex_map
        .iter()
        .find(|(_, &v)| v == vert_idx)
        .map(|(&k, _)| KernelId(k))
        .unwrap_or(KernelId(0))
}

fn compute_face_signature(ws: &WaffleSolid, face_idx: FaceIdx) -> TopoSignature {
    let verts = collect_face_vertices(&ws.arena, face_idx);

    // For cylinder caps (self-loop faces with 1 vertex), use circle geometry
    if let Some(ref cyl) = ws.cylinder_params {
        if let Some(SurfaceGeom::Planar(ref plane)) = ws.face_geometry.get(&face_idx) {
            if verts.len() == 1 {
                // This is a circular cap face
                let area = std::f64::consts::PI * cyl.radius * cyl.radius;
                let centroid = [plane.origin.x, plane.origin.y, plane.origin.z];
                let normal = [plane.normal.x, plane.normal.y, plane.normal.z];
                // Bbox: center ± radius in the cap plane
                let cx = plane.origin.x;
                let cy = plane.origin.y;
                let cz = plane.origin.z;
                let bbox = [
                    cx - cyl.radius,
                    cy - cyl.radius,
                    cz - cyl.radius,
                    cx + cyl.radius,
                    cy + cyl.radius,
                    cz + cyl.radius,
                ];
                return TopoSignature {
                    surface_type: Some("planar".to_string()),
                    area: Some(area),
                    centroid: Some(centroid),
                    normal: Some(normal),
                    bbox: Some(bbox),
                    adjacency_hash: None,
                    length: None,
                };
            }
        }
    }

    let area = polygon_area_3d(&verts);
    let centroid = if verts.is_empty() {
        [0.0; 3]
    } else {
        polygon_centroid(&verts)
    };
    let bbox = if verts.is_empty() {
        [0.0; 6]
    } else {
        compute_bbox(&verts)
    };

    let normal = ws.face_geometry.get(&face_idx).map(|g| match g {
        SurfaceGeom::Planar(p) => [p.normal.x, p.normal.y, p.normal.z],
        SurfaceGeom::Cylindrical(_) => [0.0, 0.0, 0.0],
    });

    let surface_type = ws.face_geometry.get(&face_idx).map(|g| match g {
        SurfaceGeom::Planar(_) => "planar".to_string(),
        SurfaceGeom::Cylindrical(_) => "cylindrical".to_string(),
    });

    TopoSignature {
        surface_type,
        area: Some(area),
        centroid: Some(centroid),
        normal,
        bbox: Some(bbox),
        adjacency_hash: None,
        length: None,
    }
}

fn compute_edge_signature(ws: &WaffleSolid, edge_idx: EdgeIdx) -> TopoSignature {
    // Check for circular edge geometry
    if let Some(CurveGeom::Circular(ref circle)) = ws.edge_geometry.get(&edge_idx) {
        let length = 2.0 * std::f64::consts::PI * circle.radius;
        let centroid = [circle.center.x, circle.center.y, circle.center.z];
        let r = circle.radius;
        let bbox = [
            circle.center.x - r,
            circle.center.y - r,
            circle.center.z - r,
            circle.center.x + r,
            circle.center.y + r,
            circle.center.z + r,
        ];
        return TopoSignature {
            surface_type: None,
            area: None,
            centroid: Some(centroid),
            normal: None,
            bbox: Some(bbox),
            adjacency_hash: None,
            length: Some(length),
        };
    }

    // Check for arc edge geometry
    if let Some(CurveGeom::Arc(ref arc)) = ws.edge_geometry.get(&edge_idx) {
        let length = arc.radius * arc.sweep_angle;
        let centroid = [arc.center.x, arc.center.y, arc.center.z];
        let r = arc.radius;
        let bbox = [
            arc.center.x - r,
            arc.center.y - r,
            arc.center.z - r,
            arc.center.x + r,
            arc.center.y + r,
            arc.center.z + r,
        ];
        return TopoSignature {
            surface_type: None,
            area: None,
            centroid: Some(centroid),
            normal: None,
            bbox: Some(bbox),
            adjacency_hash: None,
            length: Some(length),
        };
    }

    let he_a = ws.arena.edges[edge_idx.0].half_edge;
    let he_b = ws.arena.half_edges[he_a.0].twin;
    let p0 = ws.arena.vertices[ws.arena.half_edges[he_a.0].origin.0].position;
    let p1 = ws.arena.vertices[ws.arena.half_edges[he_b.0].origin.0].position;
    let length = v3_length(v3_sub(p1, p0));
    let centroid = v3_scale(v3_add(p0, p1), 0.5);
    let bbox = compute_bbox(&[p0, p1]);

    TopoSignature {
        surface_type: None,
        area: None,
        centroid: Some(centroid),
        normal: None,
        bbox: Some(bbox),
        adjacency_hash: None,
        length: Some(length),
    }
}

fn compute_vertex_signature(ws: &WaffleSolid, vert_idx: VertexIdx) -> TopoSignature {
    let p = ws.arena.vertices[vert_idx.0].position;
    TopoSignature {
        surface_type: None,
        area: None,
        centroid: Some(p),
        normal: None,
        bbox: Some([p[0], p[1], p[2], p[0], p[1], p[2]]),
        adjacency_hash: None,
        length: None,
    }
}

// ── Tests ────────────────────────────────────────────────────────────────

#[cfg(test)]
mod tests {
    include!("waffle_kernel_tests.rs");
}
