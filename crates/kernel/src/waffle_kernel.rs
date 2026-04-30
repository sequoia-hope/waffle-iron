//! WaffleKernel — clean-sheet B-Rep kernel.
//!
//! Supports: make_faces_from_profiles → extrude_face/revolve_face → tessellate/extract_edges → introspect.
//! Boolean ops (box-box). Fillet, chamfer, shell remain NotSupported.

use crate::geometry::curve::{Arc3D, Circle3D, CurveGeom, Line3D};
use crate::geometry::point::{Point3, Vector3};
use crate::geometry::surface::{Cone, Cylinder, Plane, Sphere, SurfaceGeom, Torus};
use crate::tessellation;
use crate::topology::arena::TopoArena;
use crate::topology::euler_ops::{mef, mev, mvfs};
use crate::topology::half_edge::*;
use crate::traits::{Kernel, KernelIntrospect};
use crate::types::*;
use crate::units::{MIN_FEATURE_SIZE, TAU_COINCIDENT, TAU_MODEL, TAU_NORMALIZE};
use crate::vecmath::*;
use std::collections::{BTreeMap, HashMap};

/// Clean-sheet geometry kernel with half-edge B-Rep topology.
pub struct WaffleKernel {
    next_id: u64,
    next_handle: u64,
    solids: BTreeMap<u64, WaffleSolid>,
    standalone_faces: BTreeMap<u64, StandaloneFace>,
}

/// A full B-Rep solid with topology arena and geometry maps.
#[derive(Clone)]
pub(crate) struct WaffleSolid {
    pub(crate) arena: TopoArena,
    pub(crate) face_map: BTreeMap<u64, FaceIdx>,
    pub(crate) edge_map: BTreeMap<u64, EdgeIdx>,
    pub(crate) vertex_map: BTreeMap<u64, VertexIdx>,
    pub(crate) face_geometry: BTreeMap<FaceIdx, SurfaceGeom>,
    pub(crate) edge_geometry: BTreeMap<EdgeIdx, CurveGeom>,
    pub(crate) cylinder_params: Option<CylinderParams>,
    pub(crate) revolve_params: Option<RevolveParams>,
    pub(crate) sphere_params: Option<SphereParams>,
    pub(crate) cone_params: Option<ConeParams>,
    pub(crate) torus_params: Option<TorusParams>,
    /// Cached face polygons from boolean results for reuse in subsequent booleans.
    pub(crate) cached_face_polys: Option<Vec<crate::boolean::FacePoly>>,
    /// True when this solid's B-Rep was built from polygon-soup classification
    /// (S-H clipping), meaning bounded tessellation should be skipped in favor
    /// of per-face tessellation to allow internal fragment removal.
    pub(crate) is_polygon_soup: bool,
    /// Cached render mesh from the Yang boolean pipeline's retessellation
    /// (yang_boolean_inner Step 9). When present, `tessellate()` returns
    /// this directly to avoid redundant retessellation. Ref [#24] Yang 2025.
    pub(crate) cached_render_mesh: Option<RenderMesh>,
}

/// Parameters for cylinder tessellation (stored after extrude_circle).
#[derive(Clone)]
pub(crate) struct CylinderParams {
    pub center_bottom: [f64; 3],
    pub radius: f64,
    pub x_axis: [f64; 3],
    pub y_axis: [f64; 3],
    pub direction: [f64; 3],
    pub depth: f64,
}

/// Parameters for revolve tessellation (stored after revolve_polygon).
#[derive(Clone)]
pub(crate) struct RevolveParams {
    pub axis_origin: [f64; 3],
    pub axis_dir: [f64; 3],
    pub angle_rad: f64,
    /// Per lateral face: (FaceIdx, start_vertex_3d, end_vertex_3d)
    pub lateral_faces: Vec<(FaceIdx, [f64; 3], [f64; 3])>,
    /// True for 360° full revolution — tessellation skips caps and wraps last ring to first.
    pub full_revolution: bool,
}

/// Parameters for sphere tessellation (stored after make_sphere).
#[derive(Clone)]
pub(crate) struct SphereParams {
    pub center: [f64; 3],
    pub radius: f64,
}

/// Parameters for cone tessellation (stored after make_cone).
#[derive(Clone)]
pub(crate) struct ConeParams {
    pub base_center: [f64; 3],
    pub apex: [f64; 3],
    pub axis: [f64; 3],
    pub radius: f64,
    pub height: f64,
}

/// Parameters for torus tessellation (stored after make_torus).
#[derive(Clone)]
pub(crate) struct TorusParams {
    pub center: [f64; 3],
    pub axis: [f64; 3],
    pub major_radius: f64,
    pub minor_radius: f64,
}

/// Circle geometry stored in a standalone face (pre-extrude).
struct CircleInfo {
    center_3d: [f64; 3],
    radius: f64,
    x_axis: [f64; 3],
    y_axis: [f64; 3],
}

/// Arc segment metadata for cylindrical face assignment during extrude.
struct ArcInfo {
    start_idx: usize,
    end_idx: usize,
    center_3d: [f64; 3],
    radius: f64,
}

/// A standalone face (pre-extrude), stored as either polygon vertices or circle info.
struct StandaloneFace {
    vertices: Vec<[f64; 3]>,
    plane_origin: [f64; 3],
    plane_normal: [f64; 3],
    circle_info: Option<CircleInfo>,
    arc_segments: Vec<ArcInfo>,
}

impl WaffleKernel {
    pub fn new() -> Self {
        Self {
            next_id: 1,
            next_handle: 1,
            solids: BTreeMap::new(),
            standalone_faces: BTreeMap::new(),
        }
    }

    /// Access a stored solid by handle ID (for internal crate tests).
    #[cfg(test)]
    pub(crate) fn get_solid(&self, handle: &KernelSolidHandle) -> Option<&WaffleSolid> {
        self.solids.get(&handle.id())
    }

    /// Diagnostic view of a stored solid's B-Rep arena and face_map.
    ///
    /// Read-only diagnostic instrumentation only — not part of the stable
    /// API. Added in PR4 of the multi-PR tessellation-bijectivity work so
    /// external test crates can run the bijective oracle
    /// (`tessellation::bijective::check_face_pair_bijective`) on a loaded
    /// solid and walk half-edge topology to diagnose non-bijective face
    /// pairs. Pure `&`-reference access; no mutation, no behavior change.
    pub fn brep_diagnostic_view(
        &self,
        handle: &KernelSolidHandle,
    ) -> Option<(&TopoArena, &BTreeMap<u64, FaceIdx>)> {
        self.solids
            .get(&handle.id())
            .map(|ws| (&ws.arena, &ws.face_map))
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

    /// Create a sphere primitive with octahedral B-Rep decomposition.
    ///
    /// Topology: 6 vertices, 12 edges, 8 triangular faces.
    /// All faces tagged `SurfaceGeom::Spherical`.
    /// All edges are great-circle arcs (pi/2 sweep).
    pub fn make_sphere(
        &mut self,
        center: [f64; 3],
        radius: f64,
    ) -> Result<KernelSolidHandle, KernelError> {
        // Validate inputs
        if !radius.is_finite() || radius <= 0.0 {
            return Err(KernelError::Other {
                message: format!("Sphere radius must be finite and positive, got {}", radius),
            });
        }
        if radius < MIN_FEATURE_SIZE {
            return Err(KernelError::Other {
                message: format!(
                    "Sphere radius {} is below MIN_FEATURE_SIZE ({})",
                    radius, MIN_FEATURE_SIZE
                ),
            });
        }
        for (i, &c) in center.iter().enumerate() {
            if !c.is_finite() {
                return Err(KernelError::Other {
                    message: format!("Sphere center component [{}] must be finite, got {}", i, c),
                });
            }
        }

        let r = radius;
        let cx = center[0];
        let cy = center[1];
        let cz = center[2];

        // Octahedral vertices:
        // v0: +X, v1: +Y, v2: +Z, v3: -X, v4: -Y, v5: -Z
        let positions: [[f64; 3]; 6] = [
            [cx + r, cy, cz], // v0: +X
            [cx, cy + r, cz], // v1: +Y
            [cx, cy, cz + r], // v2: +Z
            [cx - r, cy, cz], // v3: -X
            [cx, cy - r, cz], // v4: -Y
            [cx, cy, cz - r], // v5: -Z
        ];

        let mut arena = TopoArena::new();

        // Build octahedron using Euler operators.
        //
        // Strategy (same as extrude_polygon):
        // Phase 1: Build equatorial quad v0-v1-v3-v4 with mev chain + mef close
        // Phase 2: Add top pole v2 via mev, triangulate upper 4 faces with mef
        // Phase 3: Add bottom pole v5 via mev, triangulate lower 4 faces with mef
        //
        // After mev chain v0->v1->v3->v4, the loop has spur structure.
        // After mef(v4, v0, loop0), we get 2 quad faces (E=4, F=2, V=4).
        // Adding v2 via mev creates a spur in one quad, then mef calls triangulate.
        // Same for v5 in the other quad face.

        // Phase 1: Equatorial quad
        let (_solid, _shell, face0, v0) = mvfs(&mut arena, positions[0]);
        let loop0 = arena.faces[face0.0].outer_loop;

        let (_, v1) = mev(&mut arena, v0, loop0, positions[1]);
        let (_, v3) = mev(&mut arena, v1, loop0, positions[3]);
        let (_, v4) = mev(&mut arena, v3, loop0, positions[4]);

        // Close the equatorial quad: v4 back to v0
        let (_, eq_face) = mef(&mut arena, v4, v0, loop0);
        // V=4, E=4, F=2. face0 keeps one quad, eq_face gets the other.

        // Fix vertex half-edge pointers after mef (same pattern as extrude_polygon)
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

        // Phase 2: Add top pole v2 (+Z) to face0's quad, then triangulate.
        // mev from v0 into loop0 creates a spur. Then 3 mef calls split
        // the 6-he loop into 4 triangular faces.
        let (_, v2) = mev(&mut arena, v0, loop0, positions[2]);

        // Triangulate upper hemisphere: 3 mef calls create 3 new faces
        let (_, _f_upper1) = mef(&mut arena, v2, v1, loop0);
        let (_, _f_upper2) = mef(&mut arena, v2, v3, loop0);
        let (_, _f_upper3) = mef(&mut arena, v2, v4, loop0);
        // face0 (loop0) is now the 4th upper triangle.

        // Phase 3: Add bottom pole v5 (-Z) to eq_face's quad, then triangulate.
        let eq_loop = arena.faces[eq_face.0].outer_loop;

        // Fix vertex half-edge pointers for eq_loop
        {
            let start_he = arena.loops[eq_loop.0].half_edge;
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

        let (_, v5) = mev(&mut arena, v0, eq_loop, positions[5]);

        // Triangulate lower hemisphere: 3 mef calls
        let (_, _f_lower1) = mef(&mut arena, v5, v4, eq_loop);
        let (_, _f_lower2) = mef(&mut arena, v5, v3, eq_loop);
        let (_, _f_lower3) = mef(&mut arena, v5, v1, eq_loop);
        // eq_face (eq_loop) is now the 4th lower triangle.

        // Verify Euler formula: V=6, E=12, F=8
        debug_assert_eq!(
            arena.vertex_count(),
            6,
            "Sphere: expected 6 vertices, got {}",
            arena.vertex_count()
        );
        debug_assert_eq!(
            arena.edge_count(),
            12,
            "Sphere: expected 12 edges, got {}",
            arena.edge_count()
        );
        debug_assert_eq!(
            arena.face_count(),
            8,
            "Sphere: expected 8 faces, got {}",
            arena.face_count()
        );

        // Build geometry and ID maps
        let handle_id = self.alloc_handle();
        let mut face_map = BTreeMap::new();
        let mut edge_map = BTreeMap::new();
        let mut vertex_map = BTreeMap::new();
        let mut face_geometry = BTreeMap::new();
        let mut edge_geometry = BTreeMap::new();

        let sphere_geom = SurfaceGeom::Spherical(Sphere {
            center: Point3::from_array(center),
            radius,
        });

        // Map all faces with Spherical geometry
        for (idx, _face) in arena.faces.iter().enumerate() {
            let fid = self.alloc_id();
            face_map.insert(fid, FaceIdx(idx));
            face_geometry.insert(FaceIdx(idx), sphere_geom.clone());
        }

        // Map all edges with Arc geometry (great circle arcs, pi/2 sweep)
        for (idx, _edge) in arena.edges.iter().enumerate() {
            let eid = self.alloc_id();
            edge_map.insert(eid, EdgeIdx(idx));

            let he_a = arena.edges[idx].half_edge;
            let v_start = arena.half_edges[he_a.0].origin;
            let v_end = arena.half_edges[arena.half_edges[he_a.0].twin.0].origin;
            let p0 = arena.vertices[v_start.0].position;
            let p1 = arena.vertices[v_end.0].position;

            let d0 = v3_sub(p0, center);
            let d1 = v3_sub(p1, center);
            let arc_normal = v3_normalize(v3_cross(d0, d1));

            edge_geometry.insert(
                EdgeIdx(idx),
                CurveGeom::Arc(Arc3D {
                    center: Point3::from_array(center),
                    normal: Vector3::from_array(arc_normal),
                    radius,
                    start_point: Point3::from_array(p0),
                    sweep_angle: std::f64::consts::FRAC_PI_2,
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
                sphere_params: Some(SphereParams { center, radius }),
                cone_params: None,
                torus_params: None,
                cached_face_polys: None,
                is_polygon_soup: false,
                cached_render_mesh: None,
            },
        );

        Ok(KernelSolidHandle(handle_id))
    }

    /// Create a right circular cone primitive.
    ///
    /// Topology: 5 vertices (1 apex + 4 base), 8 edges (4 lateral + 4 base), 5 faces (4 lateral + 1 base).
    /// Lateral faces tagged `SurfaceGeom::Conical`, base face tagged `SurfaceGeom::Planar`.
    pub fn make_cone(
        &mut self,
        center: [f64; 3],
        axis: [f64; 3],
        radius: f64,
        height: f64,
    ) -> Result<KernelSolidHandle, KernelError> {
        // Validate inputs
        if !radius.is_finite() || radius <= 0.0 {
            return Err(KernelError::Other {
                message: format!("Cone radius must be finite and positive, got {}", radius),
            });
        }
        if !height.is_finite() || height <= 0.0 {
            return Err(KernelError::Other {
                message: format!("Cone height must be finite and positive, got {}", height),
            });
        }
        if radius < MIN_FEATURE_SIZE {
            return Err(KernelError::Other {
                message: format!(
                    "Cone radius {} is below MIN_FEATURE_SIZE ({})",
                    radius, MIN_FEATURE_SIZE
                ),
            });
        }
        if height < MIN_FEATURE_SIZE {
            return Err(KernelError::Other {
                message: format!(
                    "Cone height {} is below MIN_FEATURE_SIZE ({})",
                    height, MIN_FEATURE_SIZE
                ),
            });
        }
        for (i, &c) in center.iter().enumerate() {
            if !c.is_finite() {
                return Err(KernelError::Other {
                    message: format!("Cone center component [{}] must be finite, got {}", i, c),
                });
            }
        }

        // Normalize axis
        let axis_len = (axis[0] * axis[0] + axis[1] * axis[1] + axis[2] * axis[2]).sqrt();
        if axis_len < TAU_NORMALIZE {
            return Err(KernelError::Other {
                message: "Cone axis must be non-zero".to_string(),
            });
        }
        let ax = [axis[0] / axis_len, axis[1] / axis_len, axis[2] / axis_len];

        // Build orthonormal basis: ax is the cone axis (base → apex)
        // u_axis and v_axis are perpendicular to ax, used for base circle
        let u_axis = {
            let trial = if ax[0].abs() < crate::units::BASIS_AXIS_ALIGNMENT {
                [1.0, 0.0, 0.0]
            } else {
                [0.0, 1.0, 0.0]
            };
            let cross = v3_cross(ax, trial);
            v3_normalize(cross)
        };
        let v_axis = v3_cross(ax, u_axis);

        // Apex = center + axis * height
        let apex = [
            center[0] + ax[0] * height,
            center[1] + ax[1] * height,
            center[2] + ax[2] * height,
        ];

        // Base vertices at 90° intervals: +U, +V, -U, -V
        let base_positions: [[f64; 3]; 4] = [
            [
                center[0] + u_axis[0] * radius,
                center[1] + u_axis[1] * radius,
                center[2] + u_axis[2] * radius,
            ],
            [
                center[0] + v_axis[0] * radius,
                center[1] + v_axis[1] * radius,
                center[2] + v_axis[2] * radius,
            ],
            [
                center[0] - u_axis[0] * radius,
                center[1] - u_axis[1] * radius,
                center[2] - u_axis[2] * radius,
            ],
            [
                center[0] - v_axis[0] * radius,
                center[1] - v_axis[1] * radius,
                center[2] - v_axis[2] * radius,
            ],
        ];

        let mut arena = TopoArena::new();

        // Phase 1: Build base quad from 4 base vertices
        // mvfs creates the initial solid with v_b0
        let (_solid, _shell, face0, v_b0) = mvfs(&mut arena, base_positions[0]);
        let loop0 = arena.faces[face0.0].outer_loop;

        let (_, v_b1) = mev(&mut arena, v_b0, loop0, base_positions[1]);
        let (_, v_b2) = mev(&mut arena, v_b1, loop0, base_positions[2]);
        let (_, v_b3) = mev(&mut arena, v_b2, loop0, base_positions[3]);

        // Close the base quad: v_b3 back to v_b0
        let (_, base_face) = mef(&mut arena, v_b3, v_b0, loop0);
        // V=4, E=4, F=2

        // Fix vertex half-edge pointers after mef
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

        // Phase 2: Add apex vertex and triangulate face0 (which has the base quad in reverse)
        // mev from v_b0 into loop0 creates a spur to apex
        let (_, v_apex) = mev(&mut arena, v_b0, loop0, apex);

        // Triangulate: 3 mef calls split the quad+spur into 4 triangular faces
        let (_, _f_lat1) = mef(&mut arena, v_apex, v_b1, loop0);
        let (_, _f_lat2) = mef(&mut arena, v_apex, v_b2, loop0);
        let (_, _f_lat3) = mef(&mut arena, v_apex, v_b3, loop0);
        // face0 (loop0) is the 4th lateral triangle (apex-b3-b0)

        // Verify topology: V=5, E=8, F=5
        debug_assert_eq!(arena.vertex_count(), 5, "Cone: expected 5 vertices");
        debug_assert_eq!(arena.edge_count(), 8, "Cone: expected 8 edges");
        debug_assert_eq!(arena.face_count(), 5, "Cone: expected 5 faces");

        // Build geometry and ID maps
        let handle_id = self.alloc_handle();
        let mut face_map = BTreeMap::new();
        let mut edge_map = BTreeMap::new();
        let mut vertex_map = BTreeMap::new();
        let mut face_geometry = BTreeMap::new();
        let mut edge_geometry = BTreeMap::new();

        let half_angle = radius.atan2(height);
        let conical_geom = SurfaceGeom::Conical(Cone {
            apex: Point3::from_array(apex),
            axis: Vector3::from_array([-ax[0], -ax[1], -ax[2]]), // axis points from apex toward base
            half_angle,
        });

        let base_normal = [-ax[0], -ax[1], -ax[2]]; // outward normal of base face
        let planar_geom = SurfaceGeom::Planar(Plane {
            origin: Point3::from_array(center),
            normal: Vector3::from_array(base_normal),
        });

        // Assign geometry to faces
        // base_face gets planar; all others get conical
        for (idx, _face) in arena.faces.iter().enumerate() {
            let fid = self.alloc_id();
            let fi = FaceIdx(idx);
            face_map.insert(fid, fi);
            if fi == base_face {
                face_geometry.insert(fi, planar_geom.clone());
            } else {
                face_geometry.insert(fi, conical_geom.clone());
            }
        }

        // Assign edge geometry
        for (idx, _edge) in arena.edges.iter().enumerate() {
            let eid = self.alloc_id();
            edge_map.insert(eid, EdgeIdx(idx));

            let he_a = arena.edges[idx].half_edge;
            let v_start = arena.half_edges[he_a.0].origin;
            let v_end = arena.half_edges[arena.half_edges[he_a.0].twin.0].origin;
            let p0 = arena.vertices[v_start.0].position;
            let p1 = arena.vertices[v_end.0].position;

            // Determine if this is a base edge (both endpoints on base circle)
            // or a lateral edge (one endpoint is the apex)
            let is_apex_0 = v3_length(v3_sub(p0, apex)) < TAU_MODEL;
            let is_apex_1 = v3_length(v3_sub(p1, apex)) < TAU_MODEL;

            if is_apex_0 || is_apex_1 {
                // Lateral edge: straight line from apex to base vertex
                edge_geometry.insert(
                    EdgeIdx(idx),
                    CurveGeom::Linear(Line3D {
                        origin: Point3::from_array(p0),
                        direction: Vector3::from_array(v3_normalize(v3_sub(p1, p0))),
                    }),
                );
            } else {
                // Base edge: quarter-circle arc on the base plane
                edge_geometry.insert(
                    EdgeIdx(idx),
                    CurveGeom::Arc(Arc3D {
                        center: Point3::from_array(center),
                        normal: Vector3::from_array(base_normal),
                        radius,
                        start_point: Point3::from_array(p0),
                        sweep_angle: std::f64::consts::FRAC_PI_2,
                    }),
                );
            }
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
                sphere_params: None,
                cone_params: Some(ConeParams {
                    base_center: center,
                    apex,
                    axis: ax,
                    radius,
                    height,
                }),
                torus_params: None,
                cached_face_polys: None,
                is_polygon_soup: false,
                cached_render_mesh: None,
            },
        );

        Ok(KernelSolidHandle(handle_id))
    }

    /// Create a torus primitive with quad-grid B-Rep decomposition.
    ///
    /// Parameters:
    /// - center: center point of the torus
    /// - axis: orientation axis (will be normalized; must be non-zero)
    /// - major_radius: distance from center to tube center (R)
    /// - minor_radius: tube radius (r); must be less than major_radius
    ///
    /// Topology: N_major × N_minor quad grid (16 faces, 16 vertices, 32 edges for 4×4).
    /// Euler characteristic: V - E + F = 0 (genus-1 surface).
    /// All faces tagged `SurfaceGeom::Toroidal`.
    pub fn make_torus(
        &mut self,
        center: [f64; 3],
        axis: [f64; 3],
        major_radius: f64,
        minor_radius: f64,
    ) -> Result<KernelSolidHandle, KernelError> {
        // Validate inputs
        for (i, &c) in center.iter().enumerate() {
            if !c.is_finite() {
                return Err(KernelError::Other {
                    message: format!("Torus center component [{}] must be finite, got {}", i, c),
                });
            }
        }

        // Normalize axis
        let axis_len = (axis[0] * axis[0] + axis[1] * axis[1] + axis[2] * axis[2]).sqrt();
        if axis_len < TAU_NORMALIZE {
            return Err(KernelError::Other {
                message: "Torus axis must be non-zero".to_string(),
            });
        }
        let ax = [axis[0] / axis_len, axis[1] / axis_len, axis[2] / axis_len];

        if !major_radius.is_finite() || major_radius <= 0.0 {
            return Err(KernelError::Other {
                message: format!(
                    "Torus major_radius must be finite and positive, got {}",
                    major_radius
                ),
            });
        }
        if !minor_radius.is_finite() || minor_radius <= 0.0 {
            return Err(KernelError::Other {
                message: format!(
                    "Torus minor_radius must be finite and positive, got {}",
                    minor_radius
                ),
            });
        }
        if minor_radius >= major_radius {
            return Err(KernelError::Other {
                message: format!(
                    "Torus minor_radius ({}) must be less than major_radius ({})",
                    minor_radius, major_radius
                ),
            });
        }

        // Build orthonormal basis: ax is the torus axis
        let e1 = {
            let trial = if ax[0].abs() < crate::units::BASIS_AXIS_ALIGNMENT {
                [1.0, 0.0, 0.0]
            } else {
                [0.0, 1.0, 0.0]
            };
            let cross = v3_cross(ax, trial);
            v3_normalize(cross)
        };
        let e2 = v3_cross(ax, e1);

        // Grid dimensions for the quad mesh
        let n_major = 4_usize; // subdivisions around the major circle
        let n_minor = 4_usize; // subdivisions around the minor circle

        // Compute vertex positions on the torus surface
        // p(u, v) = center + (R + r*cos(v))*(cos(u)*e1 + sin(u)*e2) + r*sin(v)*ax
        let torus_point = |i: usize, j: usize| -> [f64; 3] {
            let u = 2.0 * std::f64::consts::PI * i as f64 / n_major as f64;
            let v = 2.0 * std::f64::consts::PI * j as f64 / n_minor as f64;
            let cos_u = u.cos();
            let sin_u = u.sin();
            let cos_v = v.cos();
            let sin_v = v.sin();
            let r = major_radius + minor_radius * cos_v;
            [
                center[0] + r * (cos_u * e1[0] + sin_u * e2[0]) + minor_radius * sin_v * ax[0],
                center[1] + r * (cos_u * e1[1] + sin_u * e2[1]) + minor_radius * sin_v * ax[1],
                center[2] + r * (cos_u * e1[2] + sin_u * e2[2]) + minor_radius * sin_v * ax[2],
            ]
        };

        // Build torus B-Rep directly (genus-1 topology cannot be built with
        // standard Euler operators mvfs/mev/mef alone since V-E+F=0, not 2).
        //
        // For a 4x4 quad grid on a torus: V=16, E=32, F=16.
        // Each quad face has 4 half-edges forming a loop.
        // Total half-edges: 4*16 = 64 = 2*32 = 2*E. Correct.

        let mut arena = TopoArena::new();

        // Add solid and shell (required for face construction)
        let solid_idx = arena.add_solid();
        let shell_idx = arena.add_shell(solid_idx);
        arena.solids[solid_idx.0].outer_shell = shell_idx;

        // Add vertices
        let mut vert_grid: Vec<Vec<VertexIdx>> = Vec::with_capacity(n_major);
        for i in 0..n_major {
            let mut row = Vec::with_capacity(n_minor);
            for j in 0..n_minor {
                row.push(arena.add_vertex(torus_point(i, j)));
            }
            vert_grid.push(row);
        }

        // For each quad (i,j), we create 4 half-edges in a loop.
        // he0: v(i,j)→v(i,j+1)       (along minor direction)
        // he1: v(i,j+1)→v(i+1,j+1)   (along major direction)
        // he2: v(i+1,j+1)→v(i+1,j)   (along minor direction, reverse)
        // he3: v(i+1,j)→v(i,j)       (along major direction, reverse)
        //
        // Shared edges:
        // - "minor" edge v(i,j)→v(i,j+1): he0[i][j] is twin of he2[i_prev][j]
        // - "major" edge v(i,j+1)→v(i+1,j+1): he1[i][j] is twin of he3[i][j_next]

        // Phase 1: Create all faces, loops, and half-edges
        let mut he_grid: Vec<Vec<[HalfEdgeIdx; 4]>> = Vec::with_capacity(n_major);
        for i in 0..n_major {
            let i_next = (i + 1) % n_major;
            let mut he_row = Vec::with_capacity(n_minor);
            for j in 0..n_minor {
                let j_next = (j + 1) % n_minor;

                let face = arena.add_face(shell_idx);
                let lp = arena.add_loop(face);
                arena.faces[face.0].outer_loop = lp;

                // Allocate 4 half-edges by pushing directly to arena
                let base_he = arena.half_edges.len();
                for _ in 0..4 {
                    arena.half_edges.push(HalfEdge {
                        origin: VertexIdx(0), // placeholder
                        edge: EdgeIdx(0),     // placeholder
                        twin: HalfEdgeIdx(0), // placeholder
                        next: HalfEdgeIdx(0),
                        prev: HalfEdgeIdx(0),
                        loop_: lp,
                    });
                }
                let he0 = HalfEdgeIdx(base_he);
                let he1 = HalfEdgeIdx(base_he + 1);
                let he2 = HalfEdgeIdx(base_he + 2);
                let he3 = HalfEdgeIdx(base_he + 3);

                // Set next/prev chain
                arena.half_edges[he0.0].next = he1;
                arena.half_edges[he1.0].next = he2;
                arena.half_edges[he2.0].next = he3;
                arena.half_edges[he3.0].next = he0;
                arena.half_edges[he0.0].prev = he3;
                arena.half_edges[he1.0].prev = he0;
                arena.half_edges[he2.0].prev = he1;
                arena.half_edges[he3.0].prev = he2;

                // Set origins
                arena.half_edges[he0.0].origin = vert_grid[i][j];
                arena.half_edges[he1.0].origin = vert_grid[i][j_next];
                arena.half_edges[he2.0].origin = vert_grid[i_next][j_next];
                arena.half_edges[he3.0].origin = vert_grid[i_next][j];

                // Set vertex half-edge pointers
                arena.vertices[vert_grid[i][j].0].half_edge = Some(he0);

                // Set loop half_edge
                arena.loops[lp.0].half_edge = he0;
                // Set shell face
                arena.shells[shell_idx.0].face = face;

                he_row.push([he0, he1, he2, he3]);
            }
            he_grid.push(he_row);
        }

        // Phase 2: Create edges and link twins
        // Minor-direction edges: he0[i][j] (v(i,j)→v(i,j+1)) twins with he2[i_prev][j] (v(i,j+1)→v(i,j))
        #[allow(clippy::needless_range_loop)]
        for i in 0..n_major {
            let i_prev = (i + n_major - 1) % n_major;
            for j in 0..n_minor {
                let he_a = he_grid[i][j][0];
                let he_b = he_grid[i_prev][j][2];

                arena.half_edges[he_a.0].twin = he_b;
                arena.half_edges[he_b.0].twin = he_a;

                let edge_idx = EdgeIdx(arena.edges.len());
                arena.edges.push(Edge { half_edge: he_a });
                arena.half_edges[he_a.0].edge = edge_idx;
                arena.half_edges[he_b.0].edge = edge_idx;
            }
        }

        // Major-direction edges: he1[i][j] (v(i,j+1)→v(i+1,j+1)) twins with he3[i][j_next] (v(i+1,j+1)→v(i,j+1))
        #[allow(clippy::needless_range_loop)]
        for i in 0..n_major {
            for j in 0..n_minor {
                let j_next = (j + 1) % n_minor;
                let he_a = he_grid[i][j][1];
                let he_b = he_grid[i][j_next][3];

                arena.half_edges[he_a.0].twin = he_b;
                arena.half_edges[he_b.0].twin = he_a;

                let edge_idx = EdgeIdx(arena.edges.len());
                arena.edges.push(Edge { half_edge: he_a });
                arena.half_edges[he_a.0].edge = edge_idx;
                arena.half_edges[he_b.0].edge = edge_idx;
            }
        }

        // Verify topology
        let nv = arena.vertex_count();
        let ne = arena.edge_count();
        let nf = arena.face_count();
        debug_assert_eq!(nv, n_major * n_minor, "Torus: V count");
        debug_assert_eq!(ne, 2 * n_major * n_minor, "Torus: E count");
        debug_assert_eq!(nf, n_major * n_minor, "Torus: F count");
        debug_assert_eq!(
            nv as i64 - ne as i64 + nf as i64,
            0,
            "Torus: V-E+F must be 0"
        );

        // Build geometry and ID maps
        let handle_id = self.alloc_handle();
        let mut face_map = BTreeMap::new();
        let mut edge_map = BTreeMap::new();
        let mut vertex_map = BTreeMap::new();
        let mut face_geometry = BTreeMap::new();
        let edge_geometry = BTreeMap::new();

        let torus_geom = SurfaceGeom::Toroidal(Torus {
            center: Point3::from_array(center),
            axis: Vector3::from_array(ax),
            major_radius,
            minor_radius,
        });

        for (idx, _face) in arena.faces.iter().enumerate() {
            let fid = self.alloc_id();
            face_map.insert(fid, FaceIdx(idx));
            face_geometry.insert(FaceIdx(idx), torus_geom.clone());
        }

        for (idx, _edge) in arena.edges.iter().enumerate() {
            let eid = self.alloc_id();
            edge_map.insert(eid, EdgeIdx(idx));
        }

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
                sphere_params: None,
                cone_params: None,
                torus_params: Some(TorusParams {
                    center,
                    axis: ax,
                    major_radius,
                    minor_radius,
                }),
                cached_face_polys: None,
                is_polygon_soup: false,
                cached_render_mesh: None,
            },
        );

        Ok(KernelSolidHandle(handle_id))
    }

    /// Check if all faces in a solid have planar geometry.
    /// Used to distinguish genuine box solids (all-planar) from SSI results
    /// that happen to have ≤6 faces but include cylindrical/conical surfaces.
    fn all_faces_planar(solid: &WaffleSolid) -> bool {
        if solid.face_map.is_empty() {
            return false;
        }
        for face_idx in solid.face_map.values() {
            match solid.face_geometry.get(face_idx) {
                Some(SurfaceGeom::Planar(_)) => {}
                _ => return false,
            }
        }
        true
    }

    /// Check if all faces in a solid have quadric surface geometry (A15 dispatch).
    ///
    /// Returns true if every face has a SurfaceGeom that is Planar, Cylindrical,
    /// Conical, Spherical, or Toroidal. Returns false for empty solids or solids
    /// with faces that lack geometry (e.g., post-boolean results that lost type info).
    fn all_faces_quadric(solid: &WaffleSolid) -> bool {
        if solid.face_map.is_empty() {
            return false;
        }
        // Check that every face in the map has geometry assigned
        for face_idx in solid.face_map.values() {
            if !solid.face_geometry.contains_key(face_idx) {
                return false;
            }
            // All current SurfaceGeom variants are quadric, so if geometry exists it's quadric.
            // When BSpline/NURBS is added, this will need to check is_quadric().
        }
        true
    }

    /// Build a compound solid from two disjoint operands by concatenating
    /// their face polygons. Used when boolean union detects non-overlapping
    /// operands: A ∪ B where A ∩ B = ∅ is a valid compound with two shells.
    fn build_disjoint_union(
        solid_a: &WaffleSolid,
        solid_b: &WaffleSolid,
        id_alloc: &mut dyn FnMut() -> u64,
    ) -> Result<crate::boolean::BooleanResult, KernelError> {
        let mut a_faces = crate::boolean::extract_face_polys_general(solid_a);
        let b_faces = crate::boolean::extract_face_polys_general(solid_b);
        a_faces.extend(b_faces);
        crate::boolean::build_brep_from_polygons_inner(
            &a_faces,
            crate::units::TAU_MODEL,
            true,
            id_alloc,
        )
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

        let mut next_id = self.next_id;
        let mut id_alloc = || {
            let id = next_id;
            next_id += 1;
            id
        };

        // Yang hybrid pipeline (A15.6): try first when both solids have face geometry.
        // Returns NotSupported to fall through when the pipeline isn't ready yet.
        // Wrap in catch_unwind so panics in the Yang pipeline cannot propagate —
        // they fall through to the legacy dispatch instead.
        let yang_result = std::panic::catch_unwind(std::panic::AssertUnwindSafe(|| {
            crate::boolean::yang_integration::yang_boolean_from_solids(
                solid_a,
                solid_b,
                op,
                &mut id_alloc,
            )
        }));
        match yang_result {
            Ok(Ok(result)) => {
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
                        sphere_params: None,
                        cone_params: None,
                        torus_params: None,
                        cached_face_polys: result.cached_face_polys,
                        is_polygon_soup: false,
                        cached_render_mesh: result.cached_render_mesh,
                    },
                );
                return Ok(KernelSolidHandle(handle_id));
            }
            Ok(Err(ref e)) => {
                // A15.6: Yang errors must not silently degrade to the legacy S-H path.
                // The only legitimate fall-through is the env-var gate ("not enabled").
                // Any other error means Yang was activated and failed — propagate it.
                let is_gate = matches!(e, KernelError::NotSupported { operation } if operation.contains("not enabled"));
                if !is_gate {
                    eprintln!("[A15.6] Yang boolean pipeline failed (not falling through): {e}");
                    return Err(e.clone());
                }
                // Env-var gate: Yang not enabled, fall through to legacy dispatch.
            }
            Err(panic_payload) => {
                // A15.6: Yang panics must not silently degrade to legacy dispatch.
                // If YANG_BOOLEAN=1 is set and the pipeline panics, that's a hard error.
                let yang_enabled = std::env::var("YANG_BOOLEAN").unwrap_or_default() == "1";
                let msg = panic_payload
                    .downcast_ref::<&str>()
                    .copied()
                    .or_else(|| panic_payload.downcast_ref::<String>().map(|s| s.as_str()))
                    .unwrap_or("<non-string panic>");
                if yang_enabled {
                    eprintln!(
                        "[A15.6] Yang boolean pipeline panicked (not falling through): {msg}"
                    );
                    return Err(KernelError::NotSupported {
                        operation: format!("yang_boolean: pipeline panicked: {msg}"),
                    });
                }
                // Yang not enabled — panic during env-var check is unexpected but
                // safe to fall through since Yang wasn't requested.
                eprintln!("[A15.6] Yang boolean pipeline panicked unexpectedly: {msg}");
            }
        }

        // Dispatch: classify operands by surface types (A15 compliance).
        //
        // ssi_boolean_op handles primitive pairs: box+cylinder, cyl+cyl,
        // box+sphere, sphere+sphere. For chained booleans (where one operand
        // is a post-boolean complex solid), use polygon clipping which handles
        // arbitrary face counts and geometries.
        let a_is_prim_cyl = solid_a.cylinder_params.is_some();
        let b_is_prim_cyl = solid_b.cylinder_params.is_some();
        let a_is_prim_sphere = solid_a.sphere_params.is_some();
        let b_is_prim_sphere = solid_b.sphere_params.is_some();
        let a_all_quadric = Self::all_faces_quadric(solid_a);
        let b_all_quadric = Self::all_faces_quadric(solid_b);
        // A "simple box" is an extruded rectangle: ≤6 faces, ALL planar, no
        // cylinder/sphere params. SSI-produced disjoint cylinder unions may have
        // ≤6 faces but include cylindrical faces — they must NOT be sent to the
        // box-cylinder SSI path which assumes axis-aligned box geometry.
        let a_is_simple_box = !a_is_prim_cyl
            && !a_is_prim_sphere
            && solid_a.face_map.len() <= 6
            && Self::all_faces_planar(solid_a);
        let b_is_simple_box = !b_is_prim_cyl
            && !b_is_prim_sphere
            && solid_b.face_map.len() <= 6
            && Self::all_faces_planar(solid_b);

        // SSI pipeline: when BOTH operands are primitives (cylinder, sphere, or
        // simple box). A complex post-boolean solid should NOT be sent to
        // ssi_boolean_op, which assumes simple primitive geometry.
        // Extended: all-planar solids (e.g., gear extrudes with >6 faces) can
        // also use SSI when paired with a primitive cylinder (A15 compliance).
        let a_is_prim = a_is_prim_cyl || a_is_prim_sphere || a_is_simple_box;
        let b_is_prim = b_is_prim_cyl || b_is_prim_sphere || b_is_simple_box;
        let a_is_all_planar =
            !a_is_prim_cyl && !a_is_prim_sphere && Self::all_faces_planar(solid_a);
        let b_is_all_planar =
            !b_is_prim_cyl && !b_is_prim_sphere && Self::all_faces_planar(solid_b);
        let use_ssi = (a_is_prim
            && b_is_prim
            && (a_is_prim_cyl || b_is_prim_cyl || a_is_prim_sphere || b_is_prim_sphere))
            || (a_is_all_planar && b_is_prim_cyl)   // planar solid - cylinder
            || (b_is_all_planar && a_is_prim_cyl); // cylinder - planar solid

        // Both operands all-planar — use dedicated exact boolean (A15 compliance).
        let both_all_planar = a_is_all_planar && b_is_all_planar;

        // Track whether the result came from polygon-soup classification
        // (S-H clipping) vs. analytical SSI construction, to control tessellation.
        let mut polygon_soup = false;

        let result = if use_ssi {
            // Both operands are primitives — use SSI pipeline.
            // If the analytical path returns NotSupported (partial overlap, etc.),
            // fall through to polygon clipping with geometry preservation.
            match crate::boolean::ssi_boolean_op(solid_a, solid_b, op, &mut id_alloc) {
                Ok(r) => r,
                Err(KernelError::NotSupported { ref operation }) => {
                    // A15.2: quadric primitive pairs must use exact SSI or return
                    // NotSupported — silent fallback to polygon approximation is
                    // prohibited. See governance/ARCHITECTURAL_INVARIANTS.md A15.2.
                    // Fix missing SSI sub-cases to make this path succeed.
                    return Err(KernelError::NotSupported {
                        operation: format!(
                            "SSI primitive boolean not yet implemented: {}",
                            operation
                        ),
                    });
                }
                Err(KernelError::BooleanFailed { ref reason })
                    if reason.contains("disjoint") && op == crate::boolean::BoolOp::Union =>
                {
                    polygon_soup = true;
                    Self::build_disjoint_union(solid_a, solid_b, &mut id_alloc)?
                }
                Err(e) => return Err(e),
            }
        } else if both_all_planar {
            // Both operands are all-planar polyhedra — use exact planar boolean
            // (A15 compliance). Falls back to polygon clipping if it fails.
            // Invariant: for Union, when planar_planar_boolean succeeds the
            // result is a clean B-Rep that should use bounded tessellation
            // (shared vertices, watertight by construction). Setting
            // polygon_soup unconditionally caused F0001 (stacked box union)
            // to route through fan tessellation which produces non-manifold
            // meshes with unpaired edges. For Subtract/Intersect, the result
            // may have internal face fragments requiring fan-path removal.
            match crate::boolean::planar_planar_boolean(solid_a, solid_b, op, &mut id_alloc) {
                Ok(r) => {
                    // Union results from planar_planar_boolean are clean B-Reps
                    // with internal faces properly removed. Subtract/intersect
                    // results may retain internal fragments that bounded
                    // tessellation cannot distinguish from external faces.
                    if op != crate::boolean::BoolOp::Union {
                        polygon_soup = true;
                    }
                    r
                }
                Err(KernelError::BooleanFailed { ref reason })
                    if reason.contains("disjoint") && op == crate::boolean::BoolOp::Union =>
                {
                    polygon_soup = true;
                    Self::build_disjoint_union(solid_a, solid_b, &mut id_alloc)?
                }
                Err(KernelError::BooleanFailed { ref reason }) if reason.contains("disjoint") => {
                    return Err(KernelError::BooleanFailed {
                        reason: reason.clone(),
                    });
                }
                Err(KernelError::NotSupported { .. }) | Err(KernelError::BooleanFailed { .. }) => {
                    // A15 NOTE: planar-planar pairs have an exact solver, but it
                    // does not yet handle all edge cases (coplanar faces, complex
                    // polygonal geometry). This fallback to polygon-clipping is
                    // temporary — remove when planar_planar_boolean covers all cases.
                    // Tracked in: specs/yang_hybrid_migration.md Phase 5c.
                    eprintln!("[A15 WARN] planar boolean fell back to polygon-clipping pipeline");
                    polygon_soup = true;
                    let strict = crate::boolean::boolean_op(
                        solid_a,
                        solid_b,
                        op,
                        &BooleanOptions::default(),
                        &mut id_alloc,
                    );
                    match strict {
                        ok @ Ok(_) => ok?,
                        Err(KernelError::BooleanFailed { .. }) => {
                            eprintln!(
                                "[A15 WARN] polygon-clipping failed, escalating to tolerant pipeline"
                            );
                            crate::boolean::boolean_op_tolerant(
                                solid_a,
                                solid_b,
                                op,
                                &mut id_alloc,
                            )?
                        }
                        Err(e) => return Err(e),
                    }
                }
                Err(e) => return Err(e),
            }
        } else if b_is_prim_cyl && op == crate::boolean::BoolOp::Subtract {
            // Complex solid minus enclosed cylinder primitive: use direct face
            // construction (no clipping needed). Check if the cylinder axis center
            // is inside solid_a at both ends via ray casting.
            let cyl_b = solid_b.cylinder_params.as_ref().unwrap();
            let a_faces = crate::boolean::extract_face_polys_general(solid_a);
            let cyl_center_bot = cyl_b.center_bottom;
            let cyl_center_top = [
                cyl_b.center_bottom[0] + cyl_b.direction[0] * cyl_b.depth,
                cyl_b.center_bottom[1] + cyl_b.direction[1] * cyl_b.depth,
                cyl_b.center_bottom[2] + cyl_b.direction[2] * cyl_b.depth,
            ];
            let bot_inside = crate::boolean::classify::point_in_solid(cyl_center_bot, &a_faces);
            let top_inside = crate::boolean::classify::point_in_solid(cyl_center_top, &a_faces);
            if bot_inside || top_inside {
                // At least one end is inside — try enclosed hole subtract.
                // This avoids coplanar cap clipping errors by constructing
                // the hole faces directly and splicing them into solid_a.
                polygon_soup = true;
                match crate::boolean::enclosed_hole_in_solid(solid_a, cyl_b, &mut id_alloc) {
                    Ok(r) => r,
                    Err(KernelError::NotSupported { .. }) => {
                        // Fallback to polygon clipping
                        eprintln!(
                            "[A15 WARN] enclosed-hole subtract fell back to polygon-clipping"
                        );
                        let strict = crate::boolean::boolean_op(
                            solid_a,
                            solid_b,
                            op,
                            &BooleanOptions::default(),
                            &mut id_alloc,
                        );
                        match strict {
                            ok @ Ok(_) => ok?,
                            Err(KernelError::BooleanFailed { .. }) => {
                                eprintln!(
                                    "[A15 WARN] enclosed-hole polygon-clipping failed, escalating to tolerant pipeline"
                                );
                                crate::boolean::boolean_op_tolerant(
                                    solid_a,
                                    solid_b,
                                    op,
                                    &mut id_alloc,
                                )?
                            }
                            Err(e) => return Err(e),
                        }
                    }
                    Err(e) => return Err(e),
                }
            } else {
                // Cylinder not inside solid — use standard polygon boolean
                polygon_soup = true;
                let strict = crate::boolean::boolean_op(
                    solid_a,
                    solid_b,
                    op,
                    &BooleanOptions::default(),
                    &mut id_alloc,
                );
                match strict {
                    ok @ Ok(_) => ok?,
                    Err(KernelError::BooleanFailed { .. }) => {
                        eprintln!(
                            "[A15 WARN] cylinder polygon-clipping failed, escalating to tolerant pipeline"
                        );
                        crate::boolean::boolean_op_tolerant(solid_a, solid_b, op, &mut id_alloc)?
                    }
                    Err(e) => return Err(e),
                }
            }
        } else if a_all_quadric && b_all_quadric {
            // A15 VIOLATION: both operands have only quadric faces (mixed
            // planar+cylindrical post-boolean results, chained booleans).
            // Per A15, these should use SSI, not polygon clipping. Currently
            // routes through polygon path because ssi_boolean_op only handles
            // primitive-primitive pairs, not post-boolean complex solids.
            // Fix: extend SSI dispatch to handle complex quadric solids.
            eprintln!(
                "[A15 WARN] all-quadric chained boolean routed through polygon-clipping (A15 violation)"
            );
            polygon_soup = true;
            let strict = crate::boolean::boolean_op(
                solid_a,
                solid_b,
                op,
                &BooleanOptions::default(),
                &mut id_alloc,
            );
            match strict {
                ok @ Ok(_) => ok?,
                Err(KernelError::BooleanFailed { .. }) => {
                    eprintln!(
                        "[A15 WARN] all-quadric polygon-clipping failed, escalating to tolerant pipeline"
                    );
                    crate::boolean::boolean_op_tolerant(solid_a, solid_b, op, &mut id_alloc)?
                }
                Err(e) => return Err(e),
            }
        } else {
            // General solid with non-quadric faces → polygon approximation
            polygon_soup = true;
            crate::boolean::polygon_approx_boolean(solid_a, solid_b, op, &mut id_alloc)?
        };
        self.next_id = next_id;

        // Detect when ALL faces are spherical with the SAME center and radius —
        // this means the result is a single sphere, so set sphere_params for the
        // tessellate_sphere_solid fast path (shared-vertex mesh). For multi-sphere
        // results (disjoint union, shell) or mixed results (box+sphere cavity),
        // leave sphere_params as None; the per-face tessellator derives sphere
        // params from each face's SurfaceGeom::Spherical.
        let result_sphere_params = if !result.face_geometry.is_empty() {
            let mut all_same_sphere = true;
            let mut first_center = None;
            let mut first_radius = None;
            for g in result.face_geometry.values() {
                if let SurfaceGeom::Spherical(s) = g {
                    let c = s.center.to_array();
                    let r = s.radius;
                    if let (Some(fc), Some(fr)) = (first_center, first_radius) {
                        let fc: [f64; 3] = fc;
                        let fr: f64 = fr;
                        let dist = v3_length(v3_sub(c, fc));
                        if dist > TAU_COINCIDENT || (r - fr).abs() > TAU_COINCIDENT {
                            all_same_sphere = false;
                            break;
                        }
                    } else {
                        first_center = Some(c);
                        first_radius = Some(r);
                    }
                } else {
                    all_same_sphere = false;
                    break;
                }
            }
            if all_same_sphere {
                first_center.map(|c| SphereParams {
                    center: c,
                    radius: first_radius.unwrap(),
                })
            } else {
                None
            }
        } else {
            None
        };

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
                sphere_params: result_sphere_params,
                cone_params: None,
                torus_params: None,
                cached_face_polys: result.cached_face_polys,
                is_polygon_soup: polygon_soup,
                cached_render_mesh: result.cached_render_mesh,
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
        let tau_model = TAU_MODEL;

        // Check 1: Reject degenerate (zero-length) axis direction
        if v3_length(axis_direction) < tau_model {
            return Err(KernelError::Other {
                message: "revolve axis direction is degenerate (zero-length)".to_string(),
            });
        }

        let axis_dir = v3_normalize(axis_direction);
        let n = standalone.vertices.len();

        // Yang 2025 §4.1: tessellation requires bijective mapping between B-Rep face
        // and triangle mesh. A profile straddling the revolve axis violates this — the
        // sweep maps the negative-side region 180° onto the positive-side region,
        // producing volumetric overlap (5280 inter-face penetrations in R0002 per
        // PR14 Phase A investigation). Rejecting before tessellation per the
        // `feedback_yang_only.md` constraint that meshes are exact computational
        // tools, not approximations of broken geometry.
        //
        // Reference direction: `axis_dir × plane_normal` is the in-plane direction
        // perpendicular to the axis (when the axis lies in the profile plane this is
        // the natural splitting direction; when it is off-plane the cross product
        // still yields a well-defined direction orthogonal to the axis). If that
        // cross product is degenerate (axis parallel to plane_normal — revolve about
        // an axis perpendicular to the profile, which is itself geometrically
        // pathological), fall back to the first vertex's perpendicular component,
        // which is guaranteed non-zero by the on-axis check below.
        let plane_normal = standalone.plane_normal;
        let mut ref_dir = v3_cross(axis_dir, plane_normal);
        let mut ref_len = v3_length(ref_dir);

        let mut perps: Vec<[f64; 3]> = Vec::with_capacity(n);

        // Check 2: Reject profiles with vertices on (or too close to) the axis
        for v in &standalone.vertices {
            let to_v = v3_sub(*v, axis_origin);
            let along = v3_dot(to_v, axis_dir);
            let perp = v3_sub(to_v, v3_scale(axis_dir, along));
            let dist = v3_length(perp);
            if dist < tau_model {
                return Err(KernelError::Other {
                    message: format!(
                        "revolve self-intersection: profile vertex at distance {:.2e} from axis (min {:.2e})",
                        dist, tau_model
                    ),
                });
            }
            perps.push(perp);
        }

        // Fallback for the degenerate case where axis_dir ∥ plane_normal: use the
        // first vertex's perp as the reference direction. By the check above, its
        // length is ≥ tau_model.
        if ref_len < TAU_NORMALIZE {
            ref_dir = perps[0];
            ref_len = v3_length(ref_dir);
        }
        let ref_unit = v3_scale(ref_dir, 1.0 / ref_len);

        // Check 3: Reject profiles whose vertices straddle the revolve axis.
        // Compute the signed projection of each vertex's perp component onto the
        // reference direction. If signs disagree, the profile spans both sides of
        // the axis — sweeping it produces a self-intersecting solid (R0002
        // pathology: corners at signed distances ≈ [-0.31, -0.58, +0.31, +0.58]).
        // The TAU_MODEL band around zero is treated as "on the axis" and counts as
        // either sign, matching the tolerance of Check 2.
        let mut saw_pos = false;
        let mut saw_neg = false;
        for perp in &perps {
            let signed = v3_dot(*perp, ref_unit);
            if signed > tau_model {
                saw_pos = true;
            } else if signed < -tau_model {
                saw_neg = true;
            }
        }
        if saw_pos && saw_neg {
            return Err(KernelError::Other {
                message: "revolve self-intersection: profile straddles the revolve axis \
                          (vertices lie on both sides); sweeping a straddling profile \
                          produces a self-intersecting solid"
                    .to_string(),
            });
        }

        // Profile edge validation removed: all edge orientations are now valid.
        // Axis-aligned edges produce cylindrical/planar faces; tilted edges produce
        // conical faces; degenerate (near-axis) edges fall back to planar.

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
        let mut face_map = BTreeMap::new();
        let mut edge_map = BTreeMap::new();
        let mut vertex_map = BTreeMap::new();
        let mut face_geometry = BTreeMap::new();
        let mut edge_geometry = BTreeMap::new();
        let mut lateral_face_data = Vec::new();

        // Compute solid centroid for outward-pointing normal determination
        let solid_center = {
            let mut all_verts: Vec<[f64; 3]> = start_verts.clone();
            all_verts.extend_from_slice(&end_verts);
            compute_centroid(&all_verts)
        };

        // Derive cap normals from actual loop winding (Newell) + outward direction
        let start_cap_normal = {
            let loop_verts = get_face_loop_verts(&arena, start_cap_face);
            let newell = compute_newell_normal(&loop_verts);
            let face_center = compute_centroid(&loop_verts);
            let outward = v3_sub(face_center, solid_center);
            if v3_dot(newell, outward) >= 0.0 {
                newell
            } else {
                v3_negate(newell)
            }
        };

        let end_cap_normal = {
            let loop_verts = get_face_loop_verts(&arena, face0);
            let newell = compute_newell_normal(&loop_verts);
            let face_center = compute_centroid(&loop_verts);
            let outward = v3_sub(face_center, solid_center);
            if v3_dot(newell, outward) >= 0.0 {
                newell
            } else {
                v3_negate(newell)
            }
        };

        // Start cap face (start_cap_face)
        let start_cap_kid = self.alloc_id();
        face_map.insert(start_cap_kid, start_cap_face);
        face_geometry.insert(
            start_cap_face,
            SurfaceGeom::Planar(Plane {
                origin: Point3::from_array(standalone.plane_origin),
                normal: Vector3::from_array(start_cap_normal),
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

            let same_radius = (r_a - r_b).abs() < tau_model;
            let same_height = (h_a - h_b).abs() < tau_model;

            if same_radius {
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
            } else if same_height {
                // Planar face at constant height
                let height = (h_a + h_b) / 2.0;
                let plane_origin = v3_add(axis_origin, v3_scale(axis_dir, height));
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
            } else if r_a < tau_model && r_b < tau_model {
                // Both vertices near the axis — degenerate face, use Newell-normal planar fallback
                let loop_verts = get_face_loop_verts(&arena, sf);
                let newell = compute_newell_normal(&loop_verts);
                let face_center = compute_centroid(&loop_verts);
                let outward = v3_sub(face_center, solid_center);
                let normal = if v3_dot(newell, outward) >= 0.0 {
                    newell
                } else {
                    v3_negate(newell)
                };
                face_geometry.insert(
                    sf,
                    SurfaceGeom::Planar(Plane {
                        origin: Point3::from_array(face_center),
                        normal: Vector3::from_array(normal),
                    }),
                );
            } else {
                // Different radius AND different height → conical lateral face.
                // Compute cone apex: where the generatrix line intersects the revolution axis (r=0).
                // Generatrix: r(t) = r_a + t*(r_b - r_a), h(t) = h_a + t*(h_b - h_a).
                // Setting r(t)=0: t = -r_a/(r_b - r_a) => h_apex = h_a - r_a*(h_b - h_a)/(r_b - r_a)
                let dr = r_b - r_a;
                let dh = h_b - h_a;
                let h_apex = h_a - r_a * dh / dr;
                let apex = v3_add(axis_origin, v3_scale(axis_dir, h_apex));
                let half_angle = (dr.abs() / dh.abs()).atan();
                // Axis direction: from apex toward the wider end
                let cone_axis = if r_b > r_a {
                    axis_dir
                } else {
                    v3_negate(axis_dir)
                };
                face_geometry.insert(
                    sf,
                    SurfaceGeom::Conical(Cone {
                        apex: Point3::from_array(apex),
                        axis: Vector3::from_array(cone_axis),
                        half_angle,
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

        let full_revolution = angle_deg >= 360.0;
        let revolve_params = RevolveParams {
            axis_origin,
            axis_dir,
            angle_rad,
            lateral_faces: lateral_face_data,
            full_revolution,
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
                sphere_params: None,
                cone_params: None,
                torus_params: None,
                cached_face_polys: None,
                is_polygon_soup: false,
                cached_render_mesh: None,
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
        let mut face_map = BTreeMap::new();
        let mut edge_map = BTreeMap::new();
        let mut vertex_map = BTreeMap::new();
        let mut face_geometry = BTreeMap::new();
        let mut edge_geometry = BTreeMap::new();

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
                sphere_params: None,
                cone_params: None,
                torus_params: None,
                cached_face_polys: None,
                is_polygon_soup: false,
                cached_render_mesh: None,
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
/// Get all vertex positions from a face's outer loop.
fn get_face_loop_verts(arena: &TopoArena, face: FaceIdx) -> Vec<[f64; 3]> {
    let loop_idx = arena.faces[face.0].outer_loop;
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

pub(crate) fn rotate_point_around_axis(
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
                        arc_segments: vec![],
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

            // Convert arc segments from sketch UV to 3D
            let arc_segments: Vec<ArcInfo> = profile
                .arc_segments
                .iter()
                .map(|a| {
                    let center_3d = v3_add(
                        plane_origin,
                        v3_add(
                            v3_scale(plane_x_axis, a.center_u),
                            v3_scale(plane_y_axis, a.center_v),
                        ),
                    );
                    ArcInfo {
                        start_idx: a.start_vertex_index,
                        end_idx: a.end_vertex_index,
                        center_3d,
                        radius: a.radius,
                    }
                })
                .collect();

            let face_id = self.alloc_id();
            self.standalone_faces.insert(
                face_id,
                StandaloneFace {
                    vertices: vertices_3d,
                    plane_origin,
                    plane_normal,
                    circle_info: None,
                    arc_segments,
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
        let mut face_map = BTreeMap::new();
        let mut edge_map = BTreeMap::new();
        let mut vertex_map = BTreeMap::new();
        let mut face_geometry = BTreeMap::new();
        let mut edge_geometry = BTreeMap::new();

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

        // Build arc-edge lookup: polygon edge index → arc info
        // Edge i connects vertex[i] to vertex[(i+1) % n].
        // Arc samples span vertex indices [start_idx..=end_idx] (16 points for 16 samples).
        // The arc also includes the edge from end_idx to end_idx+1 (the arc's geometric
        // endpoint, added as the next entity's start). So arc edges are start_idx..=end_idx.
        let mut arc_edge_map: BTreeMap<usize, usize> = BTreeMap::new();
        for (ai, arc) in standalone.arc_segments.iter().enumerate() {
            for edge_idx in arc.start_idx..=arc.end_idx {
                arc_edge_map.insert(edge_idx, ai);
            }
        }

        // Side faces with outward normals using Newell winding (correct for non-convex profiles)
        let newell = compute_newell_normal(&standalone.vertices);
        let winding_sign = v3_dot(newell, dir_norm).signum(); // +1 if CCW from extrude dir view
        #[allow(clippy::needless_range_loop)]
        for i in 0..n {
            let sf = side_faces[i];
            let sf_kid = self.alloc_id();
            face_map.insert(sf_kid, sf);

            if let Some(&arc_idx) = arc_edge_map.get(&i) {
                // Arc edge → cylindrical face geometry for smooth shading
                let arc = &standalone.arc_segments[arc_idx];
                face_geometry.insert(
                    sf,
                    SurfaceGeom::Cylindrical(Cylinder {
                        origin: Point3::from_array(arc.center_3d),
                        axis: Vector3::from_array(dir_norm),
                        radius: arc.radius,
                    }),
                );
            } else {
                // Straight edge → planar face geometry
                let v_a = standalone.vertices[i];
                let v_b = standalone.vertices[(i + 1) % n];
                let edge_dir = v3_sub(v_b, v_a);
                let mid = v3_scale(v3_add(v_a, v_b), 0.5);
                let side_normal = if winding_sign >= 0.0 {
                    v3_normalize(v3_cross(edge_dir, direction))
                } else {
                    v3_normalize(v3_cross(direction, edge_dir))
                };

                face_geometry.insert(
                    sf,
                    SurfaceGeom::Planar(Plane {
                        origin: Point3::from_array(mid),
                        normal: Vector3::from_array(side_normal),
                    }),
                );
            }
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
                sphere_params: None,
                cone_params: None,
                torus_params: None,
                cached_face_polys: None,
                is_polygon_soup: false,
                cached_render_mesh: None,
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

        // Return pre-computed render mesh from Yang pipeline if available.
        // yang_boolean_inner retessellates the result B-Rep at Render LOD
        // (Step 9) and caches it here. This avoids redundant retessellation
        // on subsequent tessellate() calls. Ref [#24] Yang 2025.
        if let Some(ref mesh) = ws.cached_render_mesh {
            return Ok(mesh.clone());
        }

        tessellation::tessellate_solid_ext(
            &ws.arena,
            &ws.face_map,
            &ws.face_geometry,
            &ws.edge_geometry,
            ws.cylinder_params.as_ref(),
            ws.revolve_params.as_ref(),
            ws.sphere_params.as_ref(),
            ws.cone_params.as_ref(),
            ws.torus_params.as_ref(),
            ws.is_polygon_soup,
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

        tessellation::extract_edges(
            &ws.arena,
            &ws.edge_map,
            &ws.edge_geometry,
            &ws.face_geometry,
        )
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
        if angle > 360.0 {
            return Err(KernelError::Other {
                message: format!("revolve angle must be <= 360°, got {}", angle),
            });
        }

        let mut standalone = self
            .standalone_faces
            .remove(&face.0)
            .ok_or(KernelError::EntityNotFound { id: face })?;

        // For circle profiles, generate N-gon approximation vertices
        if let Some(ref ci) = standalone.circle_info {
            let n_segs = 64;
            let mut verts = Vec::with_capacity(n_segs);
            for i in 0..n_segs {
                let theta = 2.0 * std::f64::consts::PI * (i as f64) / (n_segs as f64);
                let p = v3_add(
                    ci.center_3d,
                    v3_add(
                        v3_scale(ci.x_axis, ci.radius * theta.cos()),
                        v3_scale(ci.y_axis, ci.radius * theta.sin()),
                    ),
                );
                verts.push(p);
            }
            standalone.vertices = verts;
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
        SurfaceGeom::Conical(_) => [0.0, 0.0, 0.0],
        SurfaceGeom::Spherical(_) => [0.0, 0.0, 0.0],
        SurfaceGeom::Toroidal(_) => [0.0, 0.0, 0.0],
    });

    let surface_type = ws.face_geometry.get(&face_idx).map(|g| match g {
        SurfaceGeom::Planar(_) => "planar".to_string(),
        SurfaceGeom::Cylindrical(_) => "cylindrical".to_string(),
        SurfaceGeom::Conical(_) => "conical".to_string(),
        SurfaceGeom::Spherical(_) => "spherical".to_string(),
        SurfaceGeom::Toroidal(_) => "toroidal".to_string(),
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

// PR14 instrumentation reproducer: builds R0002's first revolve in isolation
// (rectangle profile, 347.34° around an off-plane axis) using the kernel API
// directly, tessellates it, and dumps face_ranges plus a pairwise inter-face
// triangle penetration count so we can diagnose the
// `no_self_intersection: 10 inter-face triangle penetrations, face pairs:
// (0,1)x3, (0,2)x2, ...` failure surfaced by the assay oracle.
//
// Run with:
//   REVOLVE_DEBUG=1 cargo test -p kernel --lib pr14_r0002_first_revolve_repro \
//     --ignored --nocapture
#[cfg(test)]
mod pr14_r0002_repro {
    use super::*;
    use crate::traits::Kernel;
    use crate::types::ClosedProfile;
    use std::collections::HashMap;

    /// Möller-style triangle-triangle intersection used only by the in-test
    /// penetration count (independent of the test-harness oracle which lives
    /// in another crate). Returns true for any geometric overlap; coplanar
    /// triangles are treated as non-intersecting (matches oracle behavior).
    fn tri_tri_penetrate(a: &[[f64; 3]; 3], b: &[[f64; 3]; 3]) -> bool {
        // Plane of A
        let e1 = sub(a[1], a[0]);
        let e2 = sub(a[2], a[0]);
        let na = cross(e1, e2);
        let dna = dot(na, a[0]);
        let db = [
            dot(na, b[0]) - dna,
            dot(na, b[1]) - dna,
            dot(na, b[2]) - dna,
        ];
        if (db[0] > 0.0 && db[1] > 0.0 && db[2] > 0.0)
            || (db[0] < 0.0 && db[1] < 0.0 && db[2] < 0.0)
        {
            return false;
        }
        // Plane of B
        let f1 = sub(b[1], b[0]);
        let f2 = sub(b[2], b[0]);
        let nb = cross(f1, f2);
        let dnb = dot(nb, b[0]);
        let da = [
            dot(nb, a[0]) - dnb,
            dot(nb, a[1]) - dnb,
            dot(nb, a[2]) - dnb,
        ];
        if (da[0] > 0.0 && da[1] > 0.0 && da[2] > 0.0)
            || (da[0] < 0.0 && da[1] < 0.0 && da[2] < 0.0)
        {
            return false;
        }
        // Coplanar fast-rejection (matches oracle convention)
        if na.iter().all(|x| x.abs() < 1e-30) {
            return false;
        }
        // Direction of intersection line
        let dir = cross(na, nb);
        let dlen2 = dot(dir, dir);
        if dlen2 < 1e-30 {
            return false; // Parallel planes: ignore (coplanar/parallel)
        }
        // Project each triangle's vertices onto `dir` and compute interval
        let proj_a: [f64; 3] = [dot(dir, a[0]), dot(dir, a[1]), dot(dir, a[2])];
        let proj_b: [f64; 3] = [dot(dir, b[0]), dot(dir, b[1]), dot(dir, b[2])];

        // For each tri, find the two edges that straddle the other plane
        let interval = |proj: &[f64; 3], d: &[f64; 3]| -> Option<(f64, f64)> {
            let mut t_lo = f64::INFINITY;
            let mut t_hi = f64::NEG_INFINITY;
            for i in 0..3 {
                let j = (i + 1) % 3;
                if (d[i] > 0.0) != (d[j] > 0.0) || (d[i] == 0.0 && d[j] != 0.0) {
                    if d[i] != d[j] {
                        let alpha = d[i] / (d[i] - d[j]);
                        let t = proj[i] + alpha * (proj[j] - proj[i]);
                        t_lo = t_lo.min(t);
                        t_hi = t_hi.max(t);
                    }
                }
                if d[i] == 0.0 {
                    t_lo = t_lo.min(proj[i]);
                    t_hi = t_hi.max(proj[i]);
                }
            }
            if t_lo.is_finite() && t_hi.is_finite() && t_lo <= t_hi {
                Some((t_lo, t_hi))
            } else {
                None
            }
        };
        let ia = interval(&proj_a, &db);
        let ib = interval(&proj_b, &da);
        match (ia, ib) {
            (Some((al, ah)), Some((bl, bh))) => {
                let lo = al.max(bl);
                let hi = ah.min(bh);
                hi > lo + 1e-9
            }
            _ => false,
        }
    }

    fn sub(a: [f64; 3], b: [f64; 3]) -> [f64; 3] {
        [a[0] - b[0], a[1] - b[1], a[2] - b[2]]
    }
    fn dot(a: [f64; 3], b: [f64; 3]) -> f64 {
        a[0] * b[0] + a[1] * b[1] + a[2] * b[2]
    }
    fn cross(a: [f64; 3], b: [f64; 3]) -> [f64; 3] {
        [
            a[1] * b[2] - a[2] * b[1],
            a[2] * b[0] - a[0] * b[2],
            a[0] * b[1] - a[1] * b[0],
        ]
    }

    fn shared_edge_quantized(a: &[[f64; 3]; 3], b: &[[f64; 3]; 3], max_abs: f64) -> bool {
        let grid = (max_abs * 1e-5).max(1e-9);
        let inv = 1.0 / grid;
        let q = |p: [f64; 3]| -> (i64, i64, i64) {
            (
                (p[0] * inv).round() as i64,
                (p[1] * inv).round() as i64,
                (p[2] * inv).round() as i64,
            )
        };
        let qa = [q(a[0]), q(a[1]), q(a[2])];
        let qb = [q(b[0]), q(b[1]), q(b[2])];
        qa.iter().filter(|v| qb.contains(v)).count() >= 2
    }

    /// Control: same axis + plane as R0002 but profile shifted to one side
    /// of the axis. If the axis-straddling hypothesis is correct, this should
    /// produce ZERO penetrations.
    #[test]
    #[ignore]
    fn pr14_r0002_control_shifted_profile() {
        let plane_origin: [f64; 3] = [-1.6180633893959449, 2.0489258805830994, 1.9349493605708323];
        let plane_normal: [f64; 3] = [
            -0.5196280932912005,
            -0.7470903432281938,
            -0.4145390979361667,
        ];
        let pn = plane_normal;
        let mut x_seed: [f64; 3] = [1.0, 0.0, 0.0];
        if pn[0].abs() > 0.9 {
            x_seed = [0.0, 1.0, 0.0];
        }
        let pn_dot_x = dot(x_seed, pn);
        let plane_x_axis = {
            let mut v = [
                x_seed[0] - pn[0] * pn_dot_x,
                x_seed[1] - pn[1] * pn_dot_x,
                x_seed[2] - pn[2] * pn_dot_x,
            ];
            let l = (v[0] * v[0] + v[1] * v[1] + v[2] * v[2]).sqrt();
            v[0] /= l;
            v[1] /= l;
            v[2] /= l;
            v
        };

        // Shift the rectangle in v so all corners have v > 0.7 → entirely on
        // the positive side of the axis (signed distance > 0).
        let mut positions = HashMap::new();
        positions.insert(1u32, (-0.48649840839876, 0.7));
        positions.insert(2u32, (0.48649840839876, 0.7));
        positions.insert(3u32, (0.48649840839876, 1.7));
        positions.insert(4u32, (-0.48649840839876, 1.7));

        let profile = ClosedProfile {
            entity_ids: vec![10, 11, 12, 13],
            is_outer: true,
            vertex_ids: vec![],
            circle: None,
            spline_segments: vec![],
            arc_segments: vec![],
        };
        let profiles = vec![profile];

        let mut k = WaffleKernel::new();
        let faces = k
            .make_faces_from_profiles(
                &profiles,
                plane_origin,
                plane_normal,
                plane_x_axis,
                &positions,
            )
            .expect("make faces");

        let axis_origin = [-2.816235963915955, 2.8822978227786784, 1.9349493605708323];
        let axis_direction = [-0.820949979030506, 0.5710001155252173, 0.0];
        let angle = 347.3374027211539;

        let solid = k
            .revolve_face(faces[0], axis_origin, axis_direction, angle)
            .expect("revolve");

        let mesh = k.tessellate(&solid, 0.023).expect("tessellate");

        let vert_pos = |idx: u32| -> [f64; 3] {
            let i = idx as usize * 3;
            [
                mesh.vertices[i] as f64,
                mesh.vertices[i + 1] as f64,
                mesh.vertices[i + 2] as f64,
            ]
        };
        let mut max_abs = 0.0f32;
        for v in &mesh.vertices {
            if v.abs() > max_abs {
                max_abs = v.abs();
            }
        }
        let max_abs_f64 = (max_abs as f64).max(1.0);
        struct FaceTris {
            tris: Vec<[u32; 3]>,
            aabb_min: [f64; 3],
            aabb_max: [f64; 3],
        }
        let mut faces_tris: Vec<FaceTris> = Vec::new();
        for fr in &mesh.face_ranges {
            let mut tris = Vec::new();
            let mut amin = [f64::MAX; 3];
            let mut amax = [f64::MIN; 3];
            for tri in mesh.indices[fr.start_index as usize..fr.end_index as usize].chunks(3) {
                if tri.len() < 3 {
                    continue;
                }
                tris.push([tri[0], tri[1], tri[2]]);
                for &idx in tri {
                    let p = vert_pos(idx);
                    for d in 0..3 {
                        if p[d] < amin[d] {
                            amin[d] = p[d];
                        }
                        if p[d] > amax[d] {
                            amax[d] = p[d];
                        }
                    }
                }
            }
            faces_tris.push(FaceTris {
                tris,
                aabb_min: amin,
                aabb_max: amax,
            });
        }
        let aabb_overlap = |a: &FaceTris, b: &FaceTris| -> bool {
            for d in 0..3 {
                if a.aabb_max[d] < b.aabb_min[d] || b.aabb_max[d] < a.aabb_min[d] {
                    return false;
                }
            }
            true
        };
        let mut total_pen = 0usize;
        for i in 0..faces_tris.len() {
            for j in (i + 1)..faces_tris.len() {
                if !aabb_overlap(&faces_tris[i], &faces_tris[j]) {
                    continue;
                }
                for ta in &faces_tris[i].tris {
                    for tb in &faces_tris[j].tris {
                        let pa = [vert_pos(ta[0]), vert_pos(ta[1]), vert_pos(ta[2])];
                        let pb = [vert_pos(tb[0]), vert_pos(tb[1]), vert_pos(tb[2])];
                        if shared_edge_quantized(&pa, &pb, max_abs_f64) {
                            continue;
                        }
                        if tri_tri_penetrate(&pa, &pb) {
                            total_pen += 1;
                        }
                    }
                }
            }
        }
        eprintln!(
            "[pr14-control] shifted profile (entirely positive side): TOTAL PENETRATIONS: {}",
            total_pen
        );
    }

    #[test]
    #[ignore]
    fn pr14_r0002_first_revolve_repro() {
        // R0002 sketch plane (from app/tests/cases/assay/R0002.meta.json)
        let plane_origin: [f64; 3] = [-1.6180633893959449, 2.0489258805830994, 1.9349493605708323];
        let plane_normal: [f64; 3] = [
            -0.5196280932912005,
            -0.7470903432281938,
            -0.4145390979361667,
        ];

        // Build orthonormal sketch frame: pick any X-axis perpendicular to normal
        // (project world +X onto the plane). Matches how engine builds frames.
        let pn = plane_normal;
        let mut x_seed: [f64; 3] = [1.0, 0.0, 0.0];
        if pn[0].abs() > 0.9 {
            x_seed = [0.0, 1.0, 0.0];
        }
        let pn_dot_x = dot(x_seed, pn);
        let plane_x_axis = {
            let mut v = [
                x_seed[0] - pn[0] * pn_dot_x,
                x_seed[1] - pn[1] * pn_dot_x,
                x_seed[2] - pn[2] * pn_dot_x,
            ];
            let l = (v[0] * v[0] + v[1] * v[1] + v[2] * v[2]).sqrt();
            v[0] /= l;
            v[1] /= l;
            v[2] /= l;
            v
        };

        // Sketch entities (rectangle):
        //   1: (-0.4865, -0.4683)
        //   2: ( 0.4865, -0.4683)
        //   3: ( 0.4865,  0.4683)
        //   4: (-0.4865,  0.4683)
        let mut positions = HashMap::new();
        positions.insert(1u32, (-0.48649840839876, -0.46833190755614607));
        positions.insert(2u32, (0.48649840839876, -0.46833190755614607));
        positions.insert(3u32, (0.48649840839876, 0.46833190755614607));
        positions.insert(4u32, (-0.48649840839876, 0.46833190755614607));

        let profile = ClosedProfile {
            entity_ids: vec![10, 11, 12, 13],
            is_outer: true,
            vertex_ids: vec![],
            circle: None,
            spline_segments: vec![],
            arc_segments: vec![],
        };
        let profiles = vec![profile];

        let mut k = WaffleKernel::new();
        let faces = k
            .make_faces_from_profiles(
                &profiles,
                plane_origin,
                plane_normal,
                plane_x_axis,
                &positions,
            )
            .expect("make faces");

        let axis_origin = [-2.816235963915955, 2.8822978227786784, 1.9349493605708323];
        let axis_direction = [-0.820949979030506, 0.5710001155252173, 0.0];
        let angle = 347.3374027211539;

        let solid = k
            .revolve_face(faces[0], axis_origin, axis_direction, angle)
            .expect("revolve");

        let tess_tol = 0.023; // matches assay tess_tol = scale * 0.01 ≈ 0.023
        let mesh = k.tessellate(&solid, tess_tol).expect("tessellate");

        // Compute mesh AABB max-abs
        let mut max_abs = 0.0f32;
        for v in &mesh.vertices {
            if v.abs() > max_abs {
                max_abs = v.abs();
            }
        }
        eprintln!(
            "[pr14-repro] R0002 first revolve: face_ranges.len={} verts={} indices={} max_abs={:.4}",
            mesh.face_ranges.len(),
            mesh.vertices.len() / 3,
            mesh.indices.len(),
            max_abs,
        );
        for (i, fr) in mesh.face_ranges.iter().enumerate() {
            let n_tris = (fr.end_index - fr.start_index) / 3;
            eprintln!(
                "[pr14-repro] face_range[{}] face_id={} tri_count={}",
                i, fr.face_id.0, n_tris,
            );
        }

        // Pairwise face-pair penetration count (oracle-style, simplified)
        let vert_pos = |idx: u32| -> [f64; 3] {
            let i = idx as usize * 3;
            [
                mesh.vertices[i] as f64,
                mesh.vertices[i + 1] as f64,
                mesh.vertices[i + 2] as f64,
            ]
        };
        struct FaceTris {
            tris: Vec<[u32; 3]>,
            aabb_min: [f64; 3],
            aabb_max: [f64; 3],
        }
        let mut faces_tris: Vec<FaceTris> = Vec::new();
        for fr in &mesh.face_ranges {
            let mut tris = Vec::new();
            let mut amin = [f64::MAX; 3];
            let mut amax = [f64::MIN; 3];
            for tri in mesh.indices[fr.start_index as usize..fr.end_index as usize].chunks(3) {
                if tri.len() < 3 {
                    continue;
                }
                tris.push([tri[0], tri[1], tri[2]]);
                for &idx in tri {
                    let p = vert_pos(idx);
                    for d in 0..3 {
                        if p[d] < amin[d] {
                            amin[d] = p[d];
                        }
                        if p[d] > amax[d] {
                            amax[d] = p[d];
                        }
                    }
                }
            }
            faces_tris.push(FaceTris {
                tris,
                aabb_min: amin,
                aabb_max: amax,
            });
        }

        let aabb_overlap = |a: &FaceTris, b: &FaceTris| -> bool {
            for d in 0..3 {
                if a.aabb_max[d] < b.aabb_min[d] || b.aabb_max[d] < a.aabb_min[d] {
                    return false;
                }
            }
            true
        };

        let mut total_pen = 0usize;
        let mut pair_counts: Vec<((usize, usize), usize)> = Vec::new();
        let mut max_abs_f64 = max_abs as f64;
        if max_abs_f64 < 1.0 {
            max_abs_f64 = 1.0;
        }
        for i in 0..faces_tris.len() {
            for j in (i + 1)..faces_tris.len() {
                if !aabb_overlap(&faces_tris[i], &faces_tris[j]) {
                    continue;
                }
                let mut pair_pen = 0usize;
                for ta in &faces_tris[i].tris {
                    for tb in &faces_tris[j].tris {
                        let pa = [vert_pos(ta[0]), vert_pos(ta[1]), vert_pos(ta[2])];
                        let pb = [vert_pos(tb[0]), vert_pos(tb[1]), vert_pos(tb[2])];
                        if shared_edge_quantized(&pa, &pb, max_abs_f64) {
                            continue;
                        }
                        if tri_tri_penetrate(&pa, &pb) {
                            pair_pen += 1;
                            if pair_pen <= 5 {
                                eprintln!(
                                    "[pr14-repro] PEN pair=({},{}) tri_a=[{},{},{}] tri_b=[{},{},{}] \
                                     pa=[({:.4},{:.4},{:.4}),({:.4},{:.4},{:.4}),({:.4},{:.4},{:.4})] \
                                     pb=[({:.4},{:.4},{:.4}),({:.4},{:.4},{:.4}),({:.4},{:.4},{:.4})]",
                                    i, j,
                                    ta[0], ta[1], ta[2],
                                    tb[0], tb[1], tb[2],
                                    pa[0][0], pa[0][1], pa[0][2],
                                    pa[1][0], pa[1][1], pa[1][2],
                                    pa[2][0], pa[2][1], pa[2][2],
                                    pb[0][0], pb[0][1], pb[0][2],
                                    pb[1][0], pb[1][1], pb[1][2],
                                    pb[2][0], pb[2][1], pb[2][2],
                                );
                            }
                        }
                    }
                }
                if pair_pen > 0 {
                    total_pen += pair_pen;
                    pair_counts.push(((i, j), pair_pen));
                }
            }
        }

        eprintln!(
            "[pr14-repro] TOTAL PENETRATIONS: {}  pairs: {:?}",
            total_pen, pair_counts
        );

        // Diagnostic: signed distances of profile vertices from the axis,
        // along the perpendicular-in-plane direction.
        // axis_dir × plane_normal → in-plane perpendicular to axis.
        let perp = cross(axis_direction, plane_normal);
        let perp_len = (perp[0] * perp[0] + perp[1] * perp[1] + perp[2] * perp[2]).sqrt();
        let perp = [perp[0] / perp_len, perp[1] / perp_len, perp[2] / perp_len];
        eprintln!(
            "[pr14-repro] perp_in_plane=({:.4},{:.4},{:.4})",
            perp[0], perp[1], perp[2]
        );
        // World-space profile vertices (3D positions of the start ring).
        // Build them from sketch (u,v) positions and the orthonormal frame.
        let plane_y_axis = cross(plane_normal, plane_x_axis);
        for i in 1..=4u32 {
            let (u, v) = positions[&i];
            let w = [
                plane_origin[0] + plane_x_axis[0] * u + plane_y_axis[0] * v,
                plane_origin[1] + plane_x_axis[1] * u + plane_y_axis[1] * v,
                plane_origin[2] + plane_x_axis[2] * u + plane_y_axis[2] * v,
            ];
            let rel = sub(w, axis_origin);
            let signed = dot(rel, perp);
            eprintln!(
                "[pr14-repro] profile_vert[{}]=(uv={:.4},{:.4}) world=({:.4},{:.4},{:.4}) \
                 signed_axis_dist={:.4}",
                i, u, v, w[0], w[1], w[2], signed,
            );
        }
    }
}

// PR14 Phase C — red-before-green tests for the kernel revolve_polygon validator.
//
// Defect: revolve_polygon currently checks only unsigned distance from the axis
// (`dist > TAU_MODEL` at L1410-1424) and accepts profiles whose vertices straddle
// the revolve axis. Sweeping such a profile produces a self-intersecting solid
// (the profile sweeps through itself on the opposite side of the axis), which
// surfaces in the assay as the R0002 `no_self_intersection` failure.
//
// These tests are the official Phase C red signal. Engineer-a's
// `pr14_r0002_repro` module above is documentary evidence (prints penetration
// counts; no assertions on revolve_face's return value).
//
// Refs:
//   - Yang 2025 §4.1 (Tessellation, surface discretization with bijective
//     mapping): a self-intersecting input mesh violates the bijective-mapping
//     precondition before the Cherchi mesh-intersection step (§4.2).
//   - ENGINEERING_CONSTITUTION P3 (red before green), P5 (test author distinct
//     from implementer), P9 (root-cause fix in the validator layer where the
//     defect lives).
//   - FIP §8 (bug-fix variant): reproduce-bug-with-failing-test-first.
#[cfg(test)]
mod pr14_validator_tests {
    use super::*;
    use crate::traits::Kernel;
    use crate::types::ClosedProfile;
    use std::collections::HashMap;

    /// Build R0002's sketch frame (plane_origin, plane_normal, plane_x_axis).
    /// Values copied verbatim from `app/tests/cases/assay/R0002.meta.json`
    /// via engineer-a's `pr14_r0002_first_revolve_repro` (documentary).
    fn r0002_sketch_frame() -> ([f64; 3], [f64; 3], [f64; 3]) {
        let plane_origin = [-1.6180633893959449, 2.0489258805830994, 1.9349493605708323];
        let plane_normal = [
            -0.5196280932912005,
            -0.7470903432281938,
            -0.4145390979361667,
        ];
        // Project world +X onto the plane to build an orthonormal frame
        // (matches how the engine constructs sketch frames).
        let pn: [f64; 3] = plane_normal;
        let mut x_seed: [f64; 3] = [1.0, 0.0, 0.0];
        if pn[0].abs() > 0.9 {
            x_seed = [0.0, 1.0, 0.0];
        }
        let pn_dot_x = x_seed[0] * pn[0] + x_seed[1] * pn[1] + x_seed[2] * pn[2];
        let mut v = [
            x_seed[0] - pn[0] * pn_dot_x,
            x_seed[1] - pn[1] * pn_dot_x,
            x_seed[2] - pn[2] * pn_dot_x,
        ];
        let l = (v[0] * v[0] + v[1] * v[1] + v[2] * v[2]).sqrt();
        v[0] /= l;
        v[1] /= l;
        v[2] /= l;
        (plane_origin, plane_normal, v)
    }

    /// R0002's revolve axis (off-plane, tilted) and angle. Verbatim from the
    /// assay meta JSON via engineer-a's reproducer.
    fn r0002_axis_and_angle() -> ([f64; 3], [f64; 3], f64) {
        let axis_origin = [-2.816235963915955, 2.8822978227786784, 1.9349493605708323];
        let axis_direction = [-0.820949979030506, 0.5710001155252173, 0.0];
        let angle_deg = 347.3374027211539;
        (axis_origin, axis_direction, angle_deg)
    }

    fn rect_profile_at(
        positions: HashMap<u32, (f64, f64)>,
    ) -> (Vec<ClosedProfile>, HashMap<u32, (f64, f64)>) {
        let profile = ClosedProfile {
            entity_ids: vec![10, 11, 12, 13],
            is_outer: true,
            vertex_ids: vec![],
            circle: None,
            spline_segments: vec![],
            arc_segments: vec![],
        };
        (vec![profile], positions)
    }

    /// Watertight check using oracle-compatible scale-adaptive quantization.
    /// (Mirrors `check_watertight` in `waffle_kernel_tests.rs`; cannot be
    /// reused from there because tests in `mod tests` and tests in this
    /// module are separate compilation units after `include!`.)
    fn mesh_is_watertight(mesh: &crate::types::RenderMesh) -> bool {
        use std::collections::HashMap as Map;
        let max_abs = mesh
            .vertices
            .iter()
            .map(|v| v.abs())
            .fold(0.0_f32, f32::max);
        let grid = (max_abs as f64 * crate::units::TAU_TESS_GRID_FACTOR)
            .max(crate::units::TAU_TESS_GRID_MIN);
        let inv_grid = 1.0 / grid;
        let quantize = |idx: u32| -> (i64, i64, i64) {
            let base = idx as usize * 3;
            (
                (mesh.vertices[base] as f64 * inv_grid).round() as i64,
                (mesh.vertices[base + 1] as f64 * inv_grid).round() as i64,
                (mesh.vertices[base + 2] as f64 * inv_grid).round() as i64,
            )
        };
        let mut edge_count: Map<((i64, i64, i64), (i64, i64, i64)), u32> = Map::new();
        let n_tris = mesh.indices.len() / 3;
        for i in 0..n_tris {
            let tri = [
                mesh.indices[i * 3],
                mesh.indices[i * 3 + 1],
                mesh.indices[i * 3 + 2],
            ];
            for j in 0..3 {
                let pa = quantize(tri[j]);
                let pb = quantize(tri[(j + 1) % 3]);
                let key = if pa <= pb { (pa, pb) } else { (pb, pa) };
                *edge_count.entry(key).or_insert(0) += 1;
            }
        }
        edge_count.values().all(|&c| c == 2)
    }

    /// Phase C RED test for the validator gap.
    ///
    /// R0002's rectangle profile straddles the revolve axis: signed
    /// perpendicular distances of the four corners are approximately
    /// `[-0.31, -0.58, +0.31, +0.58]` along the axis-perpendicular in-plane
    /// direction (engineer-a's Phase A finding). This produces a sweep that
    /// passes through itself on the far side of the axis, generating
    /// inter-face triangle penetrations after tessellation
    /// (assay R0002: 10 inter-face penetrations).
    ///
    /// The validator should reject this input. Today it does not — the
    /// unsigned-distance check at L1410-1424 only catches vertices ON the
    /// axis (within TAU_MODEL), not vertices straddling it.
    ///
    /// Expected (after the Phase D fix): `revolve_face(...)` returns
    /// `Err(KernelError::Other { message })` whose message names the
    /// straddling pathology (e.g., contains "self-intersection" or
    /// "straddle"/"straddles").
    #[test]
    fn test_revolve_axis_straddling_profile_rejected() {
        let (plane_origin, plane_normal, plane_x_axis) = r0002_sketch_frame();

        // R0002's exact rectangle corners (sketch-plane uv): centered on the
        // axis projection so signed distances are ±0.31 / ±0.58.
        let mut positions = HashMap::new();
        positions.insert(1u32, (-0.48649840839876, -0.46833190755614607));
        positions.insert(2u32, (0.48649840839876, -0.46833190755614607));
        positions.insert(3u32, (0.48649840839876, 0.46833190755614607));
        positions.insert(4u32, (-0.48649840839876, 0.46833190755614607));
        let (profiles, positions) = rect_profile_at(positions);

        let mut k = WaffleKernel::new();
        let faces = k
            .make_faces_from_profiles(
                &profiles,
                plane_origin,
                plane_normal,
                plane_x_axis,
                &positions,
            )
            .expect("make_faces_from_profiles should succeed for a valid rectangle profile");

        let (axis_origin, axis_direction, angle_deg) = r0002_axis_and_angle();

        let result = k.revolve_face(faces[0], axis_origin, axis_direction, angle_deg);

        assert!(
            result.is_err(),
            "revolve_face must reject a profile whose vertices straddle the revolve axis \
             (R0002 rectangle: signed perpendicular distances span both sides of the axis). \
             Sweeping a straddling profile produces a self-intersecting solid; the validator \
             must catch this before tessellation. Got Ok(_)."
        );

        // Be lenient about the exact message wording so the implementer has
        // room to phrase it. We only require that the error names the
        // pathology so it is actionable in assay logs.
        if let Err(e) = result {
            let msg = format!("{}", e).to_lowercase();
            assert!(
                msg.contains("self-intersection")
                    || msg.contains("self intersection")
                    || msg.contains("straddle")
                    || msg.contains("axis"),
                "Validator error message should describe the axis-straddling pathology; got: {}",
                e
            );
        }
    }

    /// Phase C regression-guard test (currently GREEN; must remain GREEN
    /// after the Phase D fix).
    ///
    /// Same axis, plane, and angle as R0002, but the rectangle is shifted in
    /// `v` so all four corners have signed perpendicular distance > 0
    /// (entirely on the positive side of the axis). Engineer-a's control
    /// reproducer (`pr14_r0002_control_shifted_profile`) confirmed this
    /// produces zero inter-face penetrations.
    ///
    /// This guards against the Phase D fix being too aggressive (e.g., a
    /// fix that rejects ALL revolves with off-plane axes, or one that has
    /// off-by-one signed-distance comparisons rejecting one-sided profiles
    /// near the axis).
    #[test]
    fn test_revolve_one_sided_profile_succeeds() {
        let (plane_origin, plane_normal, plane_x_axis) = r0002_sketch_frame();

        // Rectangle shifted in v so all four corners have v >= 0.7 → entirely
        // on the positive side of the axis (signed perpendicular distance > 0
        // for every corner). This matches engineer-a's
        // `pr14_r0002_control_shifted_profile` setup.
        let mut positions = HashMap::new();
        positions.insert(1u32, (-0.48649840839876, 0.7));
        positions.insert(2u32, (0.48649840839876, 0.7));
        positions.insert(3u32, (0.48649840839876, 1.7));
        positions.insert(4u32, (-0.48649840839876, 1.7));
        let (profiles, positions) = rect_profile_at(positions);

        let mut k = WaffleKernel::new();
        let faces = k
            .make_faces_from_profiles(
                &profiles,
                plane_origin,
                plane_normal,
                plane_x_axis,
                &positions,
            )
            .expect("make_faces_from_profiles should succeed for a valid rectangle profile");

        let (axis_origin, axis_direction, angle_deg) = r0002_axis_and_angle();

        let solid = k
            .revolve_face(faces[0], axis_origin, axis_direction, angle_deg)
            .expect(
                "revolve_face must succeed for a one-sided profile (rectangle entirely on the \
                 positive side of the revolve axis); the Phase D validator fix must not reject \
                 valid one-sided revolves",
            );

        // Tessellation tolerance matches the assay's `tess_tol = scale * 0.01`
        // for R0002's scale ≈ 2.3. A one-sided revolve must produce a
        // watertight closed mesh.
        let mesh = k
            .tessellate(&solid, 0.023)
            .expect("tessellate one-sided revolve");

        assert!(
            !mesh.indices.is_empty(),
            "one-sided revolve must produce a non-empty mesh"
        );
        assert!(
            mesh_is_watertight(&mesh),
            "one-sided revolve mesh must be watertight (every quantized edge shared by \
             exactly 2 triangles)"
        );
    }
}
