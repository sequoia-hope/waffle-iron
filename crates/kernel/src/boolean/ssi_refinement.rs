//! Phase 4, Task 4a — Intersection edge surface classification.
//!
//! For each intersection edge in the Yang boolean pipeline result, determines
//! the surface types of the two adjacent faces. This classification enables
//! Phase 4b to dispatch to the correct SSI solver for geometry refinement.
//!
//! Ref [#24]: Yang, Jia & Yan (2025) — Stage 4 of the hybrid pipeline.
//! Ref [#1]: Patrikalakis et al. — SSI dispatch by surface type pair (Ch. 5).

use std::collections::BTreeMap;

use crate::boolean::exact_mesh::MeshId;
use crate::boolean::topology_extract::ResultTopology;
use crate::geometry::surface::SurfaceGeom;
use crate::geometry::surface::{Cone, Cylinder, Plane, Sphere, Torus};
use crate::ssi;
use crate::ssi::SSICurve;
use crate::topology::half_edge::{EdgeIdx, FaceIdx};
use crate::types::KernelError;

/// Classification of the surface pair at an intersection edge.
#[derive(Debug, Clone)]
#[allow(dead_code)] // Phase 4 building block — task 4a
pub(crate) enum SurfacePairKind {
    /// Both faces are planar — intersection is a line. No refinement needed.
    PlanarPlanar,
    /// At least one face is curved — SSI solver required for refinement.
    NeedsRefinement {
        surface_a: SurfaceGeom,
        surface_b: SurfaceGeom,
    },
}

/// Maps each intersection edge to its surface pair classification.
#[derive(Debug)]
#[allow(dead_code)] // Phase 4 building block — task 4a
pub(crate) struct IntersectionEdgeClassification {
    pub edges: BTreeMap<EdgeIdx, SurfacePairKind>,
}

/// Classify each intersection edge in the boolean result by the surface types
/// of its two adjacent faces.
///
/// For each edge flagged as an intersection edge, traverses the half-edge
/// topology to find the two adjacent faces, looks up their source provenance,
/// and retrieves the analytical surface geometry from the surface map. The
/// surface pair is then classified as `PlanarPlanar` (no refinement needed)
/// or `NeedsRefinement` (SSI solver required).
///
/// # Arguments
/// - `result` — Half-edge B-Rep from Phase 3 with face provenance and edge flags.
/// - `surface_map` — Maps each original B-Rep face `(MeshId, FaceIdx)` to its
///   analytical surface geometry.
///
/// # Returns
/// `IntersectionEdgeClassification` with one entry per intersection edge, or
/// `KernelError` if a referenced surface is missing from `surface_map`.
///
/// Ref [#24]: Yang 2025 — Stage 4 classification
/// Ref [#1]: Patrikalakis et al. — SSI dispatch by surface type pair (Ch. 5)
#[allow(dead_code)] // Phase 4 building block — task 4a
pub(crate) fn classify_intersection_edges(
    result: &ResultTopology,
    surface_map: &BTreeMap<(MeshId, FaceIdx), SurfaceGeom>,
) -> Result<IntersectionEdgeClassification, KernelError> {
    let mut edges = BTreeMap::new();

    // Early return if no intersection edges exist.
    if result.edge_is_intersection.is_empty() || !result.edge_is_intersection.values().any(|&v| v) {
        return Ok(IntersectionEdgeClassification { edges });
    }

    for (&edge_idx, &is_intersection) in &result.edge_is_intersection {
        if !is_intersection {
            continue;
        }

        // (a) Get the edge's half-edge
        let he = result.arena.edges[edge_idx.0].half_edge;

        // (b) Get the twin half-edge
        let twin = result.arena.half_edges[he.0].twin;

        // (c) Get face_a from the half-edge's loop
        let face_a = result.arena.loops[result.arena.half_edges[he.0].loop_.0].face;

        // (d) Get face_b from the twin's loop
        let face_b = result.arena.loops[result.arena.half_edges[twin.0].loop_.0].face;

        // (e) Look up provenance for both faces
        let source_a = result
            .face_provenance
            .get(&face_a)
            .ok_or_else(|| KernelError::Other {
                message: format!(
                    "Missing face provenance for face {:?} on edge {:?}",
                    face_a, edge_idx
                ),
            })?;
        let source_b = result
            .face_provenance
            .get(&face_b)
            .ok_or_else(|| KernelError::Other {
                message: format!(
                    "Missing face provenance for face {:?} on edge {:?}",
                    face_b, edge_idx
                ),
            })?;

        // (f) Look up surfaces from the surface map
        let surf_a = surface_map
            .get(&(source_a.mesh_id, source_a.face_idx))
            .ok_or_else(|| KernelError::Other {
                message: format!(
                    "Missing surface for ({:?}, {:?}) on edge {:?}",
                    source_a.mesh_id, source_a.face_idx, edge_idx
                ),
            })?;
        let surf_b = surface_map
            .get(&(source_b.mesh_id, source_b.face_idx))
            .ok_or_else(|| KernelError::Other {
                message: format!(
                    "Missing surface for ({:?}, {:?}) on edge {:?}",
                    source_b.mesh_id, source_b.face_idx, edge_idx
                ),
            })?;

        // (h) Classify: both planar → PlanarPlanar, otherwise NeedsRefinement
        let kind = if matches!(surf_a, SurfaceGeom::Planar(_))
            && matches!(surf_b, SurfaceGeom::Planar(_))
        {
            SurfacePairKind::PlanarPlanar
        } else {
            SurfacePairKind::NeedsRefinement {
                surface_a: surf_a.clone(),
                surface_b: surf_b.clone(),
            }
        };

        edges.insert(edge_idx, kind);
    }

    Ok(IntersectionEdgeClassification { edges })
}

/// Result of Phase 4b SSI refinement — analytical curves for intersection edges.
#[derive(Debug)]
#[allow(dead_code)] // Phase 4 building block — task 4b
pub(crate) struct EdgeRefinementMap {
    /// Analytical SSI curve for each refined intersection edge.
    pub edges: BTreeMap<EdgeIdx, SSICurve>,
    /// Count of PlanarPlanar edges skipped (already exact).
    pub skipped_planar: usize,
    /// Edges where SSI solver returned NotSupported.
    pub unsupported: Vec<(EdgeIdx, String)>,
}

/// Refine intersection edges by dispatching to SSI solvers.
/// Phase 4b stub — implementation pending.
///
/// For each intersection edge classified by `classify_intersection_edges`:
/// - `PlanarPlanar` edges are skipped (already exact line intersections).
/// - `NeedsRefinement` edges are dispatched to the appropriate SSI solver
///   based on the surface pair type.
///
/// # Arguments
/// - `result` — Half-edge B-Rep from Phase 3 with face provenance and edge flags.
/// - `classification` — Phase 4a output mapping intersection edges to surface pairs.
/// - `surface_map` — Maps each original B-Rep face `(MeshId, FaceIdx)` to its
///   analytical surface geometry.
///
/// # Returns
/// `EdgeRefinementMap` with refined curves, skip counts, and unsupported pairs.
///
/// Ref [#24]: Yang 2025 — Stage 4 SSI refinement
/// Ref [#1]: Patrikalakis et al. — SSI dispatch by surface type pair (Ch. 5)
#[allow(dead_code)] // Phase 4 building block — task 4b
pub(crate) fn refine_intersection_edges(
    result: &ResultTopology,
    classification: &IntersectionEdgeClassification,
    _surface_map: &BTreeMap<(MeshId, FaceIdx), SurfaceGeom>,
) -> Result<EdgeRefinementMap, KernelError> {
    let mut edges = BTreeMap::new();
    let mut skipped_planar: usize = 0;
    let mut unsupported: Vec<(EdgeIdx, String)> = Vec::new();

    for (&edge_idx, kind) in &classification.edges {
        match kind {
            SurfacePairKind::PlanarPlanar => {
                skipped_planar += 1;
            }
            SurfacePairKind::NeedsRefinement {
                surface_a,
                surface_b,
            } => {
                let midpoint = edge_midpoint(result, edge_idx);

                match dispatch_ssi(surface_a, surface_b) {
                    Ok(curves) => {
                        if curves.is_empty() {
                            // Solver found no intersection curves for this edge.
                            // This may indicate a tangent/degenerate case or a solver
                            // that handles a sub-case analytically but finds the surfaces
                            // disjoint. Record as unsupported so the caller knows.
                            unsupported.push((
                                edge_idx,
                                "SSI solver returned no curves for intersection edge".to_string(),
                            ));
                            continue;
                        }
                        let curve = if curves.len() == 1 {
                            curves.into_iter().next().unwrap()
                        } else {
                            select_nearest_curve(curves, midpoint)
                        };
                        edges.insert(edge_idx, curve);
                    }
                    Err(KernelError::NotSupported { operation }) => {
                        unsupported.push((edge_idx, operation));
                    }
                    Err(e) => return Err(e),
                }
            }
        }
    }

    Ok(EdgeRefinementMap {
        edges,
        skipped_planar,
        unsupported,
    })
}

/// Compute the midpoint of a mesh edge for curve selection.
fn edge_midpoint(result: &ResultTopology, edge_idx: EdgeIdx) -> Option<[f64; 3]> {
    let he = result.arena.edges[edge_idx.0].half_edge;
    let twin = result.arena.half_edges[he.0].twin;
    let v0_idx = result.arena.half_edges[he.0].origin;
    let v1_idx = result.arena.half_edges[twin.0].origin;
    let p0 = result.arena.vertices[v0_idx.0].position;
    let p1 = result.arena.vertices[v1_idx.0].position;
    Some([
        (p0[0] + p1[0]) * 0.5,
        (p0[1] + p1[1]) * 0.5,
        (p0[2] + p1[2]) * 0.5,
    ])
}

/// Helper: get a surface type discriminant for ordering.
fn surface_order(s: &SurfaceGeom) -> u8 {
    match s {
        SurfaceGeom::Planar(_) => 0,
        SurfaceGeom::Cylindrical(_) => 1,
        SurfaceGeom::Conical(_) => 2,
        SurfaceGeom::Spherical(_) => 3,
        SurfaceGeom::Toroidal(_) => 4,
    }
}

/// Dispatch to the correct SSI solver based on surface pair types.
/// Normalizes ordering so the "lower" surface type comes first.
fn dispatch_ssi(
    surface_a: &SurfaceGeom,
    surface_b: &SurfaceGeom,
) -> Result<Vec<SSICurve>, KernelError> {
    // Normalize order: lower discriminant first
    let (sa, sb) = if surface_order(surface_a) <= surface_order(surface_b) {
        (surface_a, surface_b)
    } else {
        (surface_b, surface_a)
    };

    const BIG: f64 = 1e6;

    match (sa, sb) {
        // Plane + Plane — should not reach here (handled as PlanarPlanar)
        (SurfaceGeom::Planar(_), SurfaceGeom::Planar(_)) => Ok(vec![]),

        // Plane + Cylinder
        (SurfaceGeom::Planar(pl), SurfaceGeom::Cylindrical(cy)) => dispatch_plane_cylinder(pl, cy),

        // Plane + Cone
        (SurfaceGeom::Planar(pl), SurfaceGeom::Conical(co)) => dispatch_plane_cone(pl, co),

        // Plane + Sphere
        (SurfaceGeom::Planar(pl), SurfaceGeom::Spherical(sp)) => dispatch_plane_sphere(pl, sp),

        // Plane + Torus
        (SurfaceGeom::Planar(pl), SurfaceGeom::Toroidal(to)) => dispatch_plane_torus(pl, to),

        // Cylinder + Cylinder
        (SurfaceGeom::Cylindrical(ca), SurfaceGeom::Cylindrical(cb)) => ssi::cylinder_cylinder_ssi(
            ca.origin.to_array(),
            ca.axis.to_array(),
            ca.radius,
            cb.origin.to_array(),
            cb.axis.to_array(),
            cb.radius,
            (-BIG, BIG),
        ),

        // Cylinder + Cone
        (SurfaceGeom::Cylindrical(cy), SurfaceGeom::Conical(co)) => ssi::cylinder_cone_ssi(
            cy.origin.to_array(),
            cy.axis.to_array(),
            cy.radius,
            -BIG,
            BIG,
            co.apex.to_array(),
            co.axis.to_array(),
            co.half_angle,
            (0.0, BIG),
        ),

        // Cylinder + Sphere
        (SurfaceGeom::Cylindrical(cy), SurfaceGeom::Spherical(sp)) => ssi::cylinder_sphere_ssi(
            cy.origin.to_array(),
            cy.axis.to_array(),
            cy.radius,
            -BIG,
            BIG,
            sp.center.to_array(),
            sp.radius,
        ),

        // Cylinder + Torus
        (SurfaceGeom::Cylindrical(cy), SurfaceGeom::Toroidal(to)) => ssi::cylinder_torus_ssi(
            cy.origin.to_array(),
            cy.axis.to_array(),
            cy.radius,
            -BIG,
            BIG,
            to.center.to_array(),
            to.axis.to_array(),
            to.major_radius,
            to.minor_radius,
        ),

        // Cone + Cone
        (SurfaceGeom::Conical(ca), SurfaceGeom::Conical(cb)) => ssi::cone_cone_ssi(
            ca.apex.to_array(),
            ca.axis.to_array(),
            ca.half_angle,
            (0.0, BIG),
            cb.apex.to_array(),
            cb.axis.to_array(),
            cb.half_angle,
            (0.0, BIG),
        ),

        // Cone + Sphere
        (SurfaceGeom::Conical(co), SurfaceGeom::Spherical(sp)) => ssi::cone_sphere_ssi(
            co.apex.to_array(),
            co.axis.to_array(),
            co.half_angle,
            0.0,
            BIG,
            sp.center.to_array(),
            sp.radius,
        ),

        // Cone + Torus
        (SurfaceGeom::Conical(co), SurfaceGeom::Toroidal(to)) => ssi::cone_torus_ssi(
            co.apex.to_array(),
            co.axis.to_array(),
            co.half_angle,
            (0.0, BIG),
            to.center.to_array(),
            to.axis.to_array(),
            to.major_radius,
            to.minor_radius,
        ),

        // Sphere + Sphere
        (SurfaceGeom::Spherical(sa), SurfaceGeom::Spherical(sb)) => ssi::sphere_sphere_ssi(
            sa.center.to_array(),
            sa.radius,
            sb.center.to_array(),
            sb.radius,
        ),

        // Sphere + Torus
        (SurfaceGeom::Spherical(sp), SurfaceGeom::Toroidal(to)) => ssi::sphere_torus_ssi(
            sp.center.to_array(),
            sp.radius,
            to.center.to_array(),
            to.axis.to_array(),
            to.major_radius,
            to.minor_radius,
        ),

        // Torus + Torus
        (SurfaceGeom::Toroidal(ta), SurfaceGeom::Toroidal(tb)) => ssi::torus_torus_ssi(
            ta.center.to_array(),
            ta.axis.to_array(),
            ta.major_radius,
            ta.minor_radius,
            tb.center.to_array(),
            tb.axis.to_array(),
            tb.major_radius,
            tb.minor_radius,
        ),

        // Catch-all (should not occur with current surface types)
        _ => Err(KernelError::NotSupported {
            operation: format!(
                "SSI for surface pair ({}, {})",
                surface_order(sa),
                surface_order(sb)
            ),
        }),
    }
}

fn dispatch_plane_cylinder(pl: &Plane, cy: &Cylinder) -> Result<Vec<SSICurve>, KernelError> {
    ssi::plane_cylinder_ssi(
        pl.origin.to_array(),
        pl.normal.to_array(),
        cy.origin.to_array(),
        cy.axis.to_array(),
        cy.radius,
        (-1e6, 1e6),
    )
}

fn dispatch_plane_cone(pl: &Plane, co: &Cone) -> Result<Vec<SSICurve>, KernelError> {
    ssi::plane_cone_ssi(
        pl.origin.to_array(),
        pl.normal.to_array(),
        co.apex.to_array(),
        co.axis.to_array(),
        co.half_angle,
        1e6,
    )
}

fn dispatch_plane_sphere(pl: &Plane, sp: &Sphere) -> Result<Vec<SSICurve>, KernelError> {
    ssi::plane_sphere_ssi(
        pl.origin.to_array(),
        pl.normal.to_array(),
        sp.center.to_array(),
        sp.radius,
    )
}

fn dispatch_plane_torus(pl: &Plane, to: &Torus) -> Result<Vec<SSICurve>, KernelError> {
    ssi::plane_torus_ssi(
        pl.origin.to_array(),
        pl.normal.to_array(),
        to.center.to_array(),
        to.axis.to_array(),
        to.major_radius,
        to.minor_radius,
    )
}

/// Given multiple SSI curves, select the one whose representative point is
/// closest to the mesh edge midpoint.
fn select_nearest_curve(curves: Vec<SSICurve>, midpoint: Option<[f64; 3]>) -> SSICurve {
    let mid = match midpoint {
        Some(m) => m,
        None => return curves.into_iter().next().unwrap(),
    };

    curves
        .into_iter()
        .min_by(|a, b| {
            let da = dist_sq_to_curve_rep(a, &mid);
            let db = dist_sq_to_curve_rep(b, &mid);
            da.partial_cmp(&db).unwrap_or(std::cmp::Ordering::Equal)
        })
        .unwrap()
}

/// Squared distance from a point to a curve's representative point.
fn dist_sq_to_curve_rep(curve: &SSICurve, pt: &[f64; 3]) -> f64 {
    let rep = curve_representative_point(curve);
    let dx = rep[0] - pt[0];
    let dy = rep[1] - pt[1];
    let dz = rep[2] - pt[2];
    dx * dx + dy * dy + dz * dz
}

/// Get a representative point for a curve (center, vertex, or midpoint).
fn curve_representative_point(curve: &SSICurve) -> [f64; 3] {
    match curve {
        SSICurve::Circle { center, .. } => *center,
        SSICurve::Ellipse { center, .. } => *center,
        SSICurve::Line { start, end } => [
            (start[0] + end[0]) * 0.5,
            (start[1] + end[1]) * 0.5,
            (start[2] + end[2]) * 0.5,
        ],
        SSICurve::Parabola { vertex, .. } => *vertex,
        SSICurve::Hyperbola { center, .. } => *center,
        SSICurve::Degree4CylCyl { center, .. } => *center,
        SSICurve::Degree4ConeSphere { cone_apex, .. } => *cone_apex,
        SSICurve::Degree4CylSphere { cyl_origin, .. } => *cyl_origin,
        SSICurve::Degree4CylCone { cyl_origin, .. } => *cyl_origin,
        SSICurve::Degree4ConeCone { cone_a_apex, .. } => *cone_a_apex,
        SSICurve::Degree4PlaneTorus { torus_center, .. } => *torus_center,
        SSICurve::Degree4SphereTorus { torus_center, .. } => *torus_center,
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::boolean::exact_mesh::MeshBooleanOp;
    use crate::boolean::topology_extract::{yang_boolean_pipeline, SourceFace};
    use crate::geometry::point::{Point3, Vector3};
    use crate::geometry::surface::{Cone, Cylinder, Plane, Sphere, Torus};
    use crate::tessellation::bijective::BijectiveMap;
    use crate::topology::arena::TopoArena;
    use crate::topology::half_edge::FaceIdx;

    // ── Test helpers (reused from topology_extract tests) ──

    /// Build a box mesh with 8 vertices and 12 triangles (2 per face).
    fn make_box_mesh(min: [f64; 3], max: [f64; 3]) -> (Vec<[f64; 3]>, Vec<[usize; 3]>) {
        let [x0, y0, z0] = min;
        let [x1, y1, z1] = max;
        let verts = vec![
            [x0, y0, z0], // 0
            [x1, y0, z0], // 1
            [x1, y1, z0], // 2
            [x0, y1, z0], // 3
            [x0, y0, z1], // 4
            [x1, y0, z1], // 5
            [x1, y1, z1], // 6
            [x0, y1, z1], // 7
        ];
        let tris = vec![
            // Back face (z=z0) — face 0
            [0, 2, 1],
            [0, 3, 2],
            // Front face (z=z1) — face 1
            [4, 5, 6],
            [4, 6, 7],
            // Bottom face (y=y0) — face 2
            [0, 1, 5],
            [0, 5, 4],
            // Top face (y=y1) — face 3
            [3, 6, 2],
            [3, 7, 6],
            // Left face (x=x0) — face 4
            [0, 4, 7],
            [0, 7, 3],
            // Right face (x=x1) — face 5
            [1, 2, 6],
            [1, 6, 5],
        ];
        (verts, tris)
    }

    /// Run the full yang_boolean_pipeline for two overlapping boxes.
    fn run_full_pipeline(op: MeshBooleanOp) -> ResultTopology {
        let (verts_a, tris_a) = make_box_mesh([0.0, 0.0, 0.0], [2.0, 2.0, 2.0]);
        let (verts_b, tris_b) = make_box_mesh([1.0, 0.0, 0.0], [3.0, 2.0, 2.0]);
        let bijective_a = BijectiveMap {
            tri_face_ids: (0..12).map(|i| FaceIdx(i / 2)).collect(),
        };
        let bijective_b = BijectiveMap {
            tri_face_ids: (0..12).map(|i| FaceIdx(i / 2)).collect(),
        };
        yang_boolean_pipeline(
            &verts_a,
            &tris_a,
            &verts_b,
            &tris_b,
            &bijective_a,
            &bijective_b,
            op,
        )
    }

    /// Build an all-planar surface map for two boxes.
    /// Box A has face indices 0..6, box B has face indices 0..6.
    /// Each face is planar: back(z-), front(z+), bottom(y-), top(y+), left(x-), right(x+).
    fn planar_surface_map_for_boxes() -> BTreeMap<(MeshId, FaceIdx), SurfaceGeom> {
        let normals = [
            Vector3::new(0.0, 0.0, -1.0), // face 0: back
            Vector3::new(0.0, 0.0, 1.0),  // face 1: front
            Vector3::new(0.0, -1.0, 0.0), // face 2: bottom
            Vector3::new(0.0, 1.0, 0.0),  // face 3: top
            Vector3::new(-1.0, 0.0, 0.0), // face 4: left
            Vector3::new(1.0, 0.0, 0.0),  // face 5: right
        ];
        let mut map = BTreeMap::new();
        for mesh_id in [MeshId::A, MeshId::B] {
            for (i, normal) in normals.iter().enumerate() {
                map.insert(
                    (mesh_id, FaceIdx(i)),
                    SurfaceGeom::Planar(Plane {
                        origin: Point3::origin(),
                        normal: *normal,
                    }),
                );
            }
        }
        map
    }

    // ── B1: Empty topology returns empty classification ──

    #[test]
    fn test_empty_topology_returns_empty_classification() {
        let result = ResultTopology {
            arena: TopoArena::new(),
            face_provenance: BTreeMap::new(),
            edge_is_intersection: BTreeMap::new(),
        };
        let surface_map = BTreeMap::new();
        let classification = classify_intersection_edges(&result, &surface_map)
            .expect("Empty topology should return Ok with empty classification");
        assert!(
            classification.edges.is_empty(),
            "Empty topology must produce empty classification, got {} entries",
            classification.edges.len(),
        );
    }

    // ── B2: Box-box subtract — all intersection edges are PlanarPlanar ──

    #[test]
    fn test_box_box_subtract_all_planar() {
        let result = run_full_pipeline(MeshBooleanOp::Subtract);
        let surface_map = planar_surface_map_for_boxes();

        let classification = classify_intersection_edges(&result, &surface_map)
            .expect("Box-box subtract with complete surface map should succeed");

        // Must have at least one intersection edge
        let intersection_count = result.edge_is_intersection.values().filter(|&&v| v).count();
        assert!(
            intersection_count > 0,
            "Box-box subtract must have intersection edges"
        );

        // Every classified edge must be PlanarPlanar
        for (edge_idx, kind) in &classification.edges {
            match kind {
                SurfacePairKind::PlanarPlanar => {} // expected
                SurfacePairKind::NeedsRefinement { .. } => {
                    panic!(
                        "Edge {:?} classified as NeedsRefinement but all surfaces are planar",
                        edge_idx,
                    );
                }
            }
        }
    }

    // ── B3: Planar-curved classification ──

    #[test]
    fn test_planar_curved_classification() {
        // Construct a minimal ResultTopology by hand with 2 faces sharing one edge.
        // Face A is planar, face B is cylindrical.
        let mut arena = TopoArena::new();

        // Create minimal topology scaffolding
        let solid = arena.add_solid();
        let shell = arena.add_shell(solid);

        // Two faces, each with one loop
        let face0 = arena.add_face(shell);
        let face1 = arena.add_face(shell);
        let loop0 = arena.add_loop(face0);
        let loop1 = arena.add_loop(face1);
        arena.faces[face0.0].outer_loop = loop0;
        arena.faces[face1.0].outer_loop = loop1;

        // Vertices
        let _v0 = arena.add_vertex([0.0, 0.0, 0.0]);
        let _v1 = arena.add_vertex([1.0, 0.0, 0.0]);

        // The shared edge: add_edge creates twin half-edges automatically
        let (edge_shared, he_a, he_b) = arena.add_edge();
        arena.half_edges[he_a.0].origin = _v0;
        arena.half_edges[he_b.0].origin = _v1;
        arena.half_edges[he_a.0].loop_ = loop0;
        arena.half_edges[he_b.0].loop_ = loop1;
        arena.half_edges[he_a.0].next = he_a; // self-loop (minimal)
        arena.half_edges[he_a.0].prev = he_a;
        arena.half_edges[he_b.0].next = he_b;
        arena.half_edges[he_b.0].prev = he_b;
        arena.loops[loop0.0].half_edge = he_a;
        arena.loops[loop1.0].half_edge = he_b;

        // Set up provenance and intersection flags
        let mut face_provenance = BTreeMap::new();
        face_provenance.insert(
            face0,
            SourceFace {
                mesh_id: MeshId::A,
                face_idx: FaceIdx(0),
            },
        );
        face_provenance.insert(
            face1,
            SourceFace {
                mesh_id: MeshId::B,
                face_idx: FaceIdx(0),
            },
        );

        let mut edge_is_intersection = BTreeMap::new();
        edge_is_intersection.insert(edge_shared, true);

        let result = ResultTopology {
            arena,
            face_provenance,
            edge_is_intersection,
        };

        // Surface map: face A is planar, face B is cylindrical
        let mut surface_map = BTreeMap::new();
        surface_map.insert(
            (MeshId::A, FaceIdx(0)),
            SurfaceGeom::Planar(Plane {
                origin: Point3::origin(),
                normal: Vector3::new(0.0, 0.0, 1.0),
            }),
        );
        surface_map.insert(
            (MeshId::B, FaceIdx(0)),
            SurfaceGeom::Cylindrical(Cylinder {
                origin: Point3::origin(),
                axis: Vector3::new(0.0, 0.0, 1.0),
                radius: 1.0,
            }),
        );

        let classification = classify_intersection_edges(&result, &surface_map)
            .expect("Planar-curved classification should succeed");

        assert_eq!(
            classification.edges.len(),
            1,
            "Should classify exactly 1 intersection edge"
        );
        match classification.edges.values().next().unwrap() {
            SurfacePairKind::NeedsRefinement {
                surface_a,
                surface_b,
            } => {
                // One should be planar and the other cylindrical
                let has_planar = matches!(surface_a, SurfaceGeom::Planar(_))
                    || matches!(surface_b, SurfaceGeom::Planar(_));
                let has_cyl = matches!(surface_a, SurfaceGeom::Cylindrical(_))
                    || matches!(surface_b, SurfaceGeom::Cylindrical(_));
                assert!(has_planar, "One surface should be planar");
                assert!(has_cyl, "One surface should be cylindrical");
            }
            SurfacePairKind::PlanarPlanar => {
                panic!("Edge between planar and cylindrical faces should be NeedsRefinement, not PlanarPlanar");
            }
        }
    }

    // ── Invariant 3: Count matches intersection edge count ──

    #[test]
    fn test_count_matches_intersection_edge_count() {
        let result = run_full_pipeline(MeshBooleanOp::Subtract);
        let surface_map = planar_surface_map_for_boxes();

        let classification = classify_intersection_edges(&result, &surface_map)
            .expect("Classification should succeed for complete surface map");

        let expected_count = result.edge_is_intersection.values().filter(|&&v| v).count();

        assert_eq!(
            classification.edges.len(),
            expected_count,
            "Classification entries ({}) must equal intersection edge count ({expected_count})",
            classification.edges.len(),
        );
    }

    // ── B6: Missing surface returns error ──

    #[test]
    fn test_missing_surface_returns_error() {
        let result = run_full_pipeline(MeshBooleanOp::Subtract);

        // Provide an incomplete surface map (empty)
        let surface_map = BTreeMap::new();

        // Should succeed only if there are no intersection edges to classify.
        // Since box-box subtract produces intersection edges, this should fail.
        let has_intersection_edges = result.edge_is_intersection.values().any(|&v| v);

        if has_intersection_edges {
            let err = classify_intersection_edges(&result, &surface_map);
            assert!(
                err.is_err(),
                "Missing surface in map should produce an error when intersection edges exist"
            );
        }
    }

    // ── Adversarial: B4 — Curved-curved classification ──

    #[test]
    fn test_curved_curved_classification() {
        // Both faces are curved: Cylindrical + Spherical.
        // Build minimal topology by hand (same pattern as test_planar_curved_classification).
        let mut arena = TopoArena::new();

        let solid = arena.add_solid();
        let shell = arena.add_shell(solid);

        let face0 = arena.add_face(shell);
        let face1 = arena.add_face(shell);
        let loop0 = arena.add_loop(face0);
        let loop1 = arena.add_loop(face1);
        arena.faces[face0.0].outer_loop = loop0;
        arena.faces[face1.0].outer_loop = loop1;

        let v0 = arena.add_vertex([0.0, 0.0, 0.0]);
        let v1 = arena.add_vertex([1.0, 0.0, 0.0]);

        let (edge_shared, he_a, he_b) = arena.add_edge();
        arena.half_edges[he_a.0].origin = v0;
        arena.half_edges[he_b.0].origin = v1;
        arena.half_edges[he_a.0].loop_ = loop0;
        arena.half_edges[he_b.0].loop_ = loop1;
        arena.half_edges[he_a.0].next = he_a;
        arena.half_edges[he_a.0].prev = he_a;
        arena.half_edges[he_b.0].next = he_b;
        arena.half_edges[he_b.0].prev = he_b;
        arena.loops[loop0.0].half_edge = he_a;
        arena.loops[loop1.0].half_edge = he_b;

        let mut face_provenance = BTreeMap::new();
        face_provenance.insert(
            face0,
            SourceFace {
                mesh_id: MeshId::A,
                face_idx: FaceIdx(0),
            },
        );
        face_provenance.insert(
            face1,
            SourceFace {
                mesh_id: MeshId::B,
                face_idx: FaceIdx(0),
            },
        );

        let mut edge_is_intersection = BTreeMap::new();
        edge_is_intersection.insert(edge_shared, true);

        let result = ResultTopology {
            arena,
            face_provenance,
            edge_is_intersection,
        };

        // Surface map: face A is cylindrical, face B is spherical
        let mut surface_map = BTreeMap::new();
        surface_map.insert(
            (MeshId::A, FaceIdx(0)),
            SurfaceGeom::Cylindrical(Cylinder {
                origin: Point3::origin(),
                axis: Vector3::new(0.0, 0.0, 1.0),
                radius: 2.0,
            }),
        );
        surface_map.insert(
            (MeshId::B, FaceIdx(0)),
            SurfaceGeom::Spherical(Sphere {
                center: Point3::origin(),
                radius: 3.0,
            }),
        );

        let classification = classify_intersection_edges(&result, &surface_map)
            .expect("Curved-curved classification should succeed");

        assert_eq!(
            classification.edges.len(),
            1,
            "Should classify exactly 1 intersection edge"
        );
        match classification.edges.values().next().unwrap() {
            SurfacePairKind::NeedsRefinement {
                surface_a,
                surface_b,
            } => {
                let has_cyl = matches!(surface_a, SurfaceGeom::Cylindrical(_))
                    || matches!(surface_b, SurfaceGeom::Cylindrical(_));
                let has_sph = matches!(surface_a, SurfaceGeom::Spherical(_))
                    || matches!(surface_b, SurfaceGeom::Spherical(_));
                assert!(has_cyl, "One surface should be cylindrical");
                assert!(has_sph, "One surface should be spherical");
            }
            SurfacePairKind::PlanarPlanar => {
                panic!("Edge between two curved faces must be NeedsRefinement, not PlanarPlanar");
            }
        }
    }

    // ── Adversarial: Non-intersection edges excluded from output ──

    #[test]
    fn test_non_intersection_edges_excluded() {
        // Build a topology with 3 edges: 2 intersection, 1 non-intersection.
        // Verify the non-intersection edge does NOT appear in the classification.
        let mut arena = TopoArena::new();

        let solid = arena.add_solid();
        let shell = arena.add_shell(solid);

        // Three faces, each with one loop
        let face0 = arena.add_face(shell);
        let face1 = arena.add_face(shell);
        let face2 = arena.add_face(shell);
        let loop0 = arena.add_loop(face0);
        let loop1 = arena.add_loop(face1);
        let loop2 = arena.add_loop(face2);
        arena.faces[face0.0].outer_loop = loop0;
        arena.faces[face1.0].outer_loop = loop1;
        arena.faces[face2.0].outer_loop = loop2;

        let v0 = arena.add_vertex([0.0, 0.0, 0.0]);
        let v1 = arena.add_vertex([1.0, 0.0, 0.0]);
        let v2 = arena.add_vertex([0.0, 1.0, 0.0]);

        // Edge 0: intersection edge between face0 and face1
        let (edge0, he0a, he0b) = arena.add_edge();
        arena.half_edges[he0a.0].origin = v0;
        arena.half_edges[he0b.0].origin = v1;
        arena.half_edges[he0a.0].loop_ = loop0;
        arena.half_edges[he0b.0].loop_ = loop1;
        arena.half_edges[he0a.0].next = he0a;
        arena.half_edges[he0a.0].prev = he0a;
        arena.half_edges[he0b.0].next = he0b;
        arena.half_edges[he0b.0].prev = he0b;
        arena.loops[loop0.0].half_edge = he0a;
        arena.loops[loop1.0].half_edge = he0b;

        // Edge 1: NON-intersection edge between face1 and face2
        let (edge1, he1a, he1b) = arena.add_edge();
        arena.half_edges[he1a.0].origin = v1;
        arena.half_edges[he1b.0].origin = v2;
        arena.half_edges[he1a.0].loop_ = loop1;
        arena.half_edges[he1b.0].loop_ = loop2;
        arena.half_edges[he1a.0].next = he1a;
        arena.half_edges[he1a.0].prev = he1a;
        arena.half_edges[he1b.0].next = he1b;
        arena.half_edges[he1b.0].prev = he1b;
        // Note: loop1 already has half_edge set; in real topology each loop
        // would chain through multiple half-edges, but for this test we only
        // need the edge→half_edge→loop→face traversal to work per-edge.

        // Edge 2: intersection edge between face0 and face2
        let (edge2, he2a, he2b) = arena.add_edge();
        arena.half_edges[he2a.0].origin = v0;
        arena.half_edges[he2b.0].origin = v2;
        arena.half_edges[he2a.0].loop_ = loop0;
        arena.half_edges[he2b.0].loop_ = loop2;
        arena.half_edges[he2a.0].next = he2a;
        arena.half_edges[he2a.0].prev = he2a;
        arena.half_edges[he2b.0].next = he2b;
        arena.half_edges[he2b.0].prev = he2b;

        let mut face_provenance = BTreeMap::new();
        face_provenance.insert(
            face0,
            SourceFace {
                mesh_id: MeshId::A,
                face_idx: FaceIdx(0),
            },
        );
        face_provenance.insert(
            face1,
            SourceFace {
                mesh_id: MeshId::B,
                face_idx: FaceIdx(0),
            },
        );
        face_provenance.insert(
            face2,
            SourceFace {
                mesh_id: MeshId::A,
                face_idx: FaceIdx(1),
            },
        );

        let mut edge_is_intersection = BTreeMap::new();
        edge_is_intersection.insert(edge0, true);
        edge_is_intersection.insert(edge1, false); // NOT an intersection edge
        edge_is_intersection.insert(edge2, true);

        let result = ResultTopology {
            arena,
            face_provenance,
            edge_is_intersection,
        };

        // All-planar surface map
        let mut surface_map = BTreeMap::new();
        for (mesh_id, face_idx) in &[
            (MeshId::A, FaceIdx(0)),
            (MeshId::A, FaceIdx(1)),
            (MeshId::B, FaceIdx(0)),
        ] {
            surface_map.insert(
                (*mesh_id, *face_idx),
                SurfaceGeom::Planar(Plane {
                    origin: Point3::origin(),
                    normal: Vector3::new(0.0, 0.0, 1.0),
                }),
            );
        }

        let classification = classify_intersection_edges(&result, &surface_map)
            .expect("Classification should succeed");

        // Exactly 2 intersection edges should be classified
        assert_eq!(
            classification.edges.len(),
            2,
            "Only intersection edges should appear: expected 2, got {}",
            classification.edges.len(),
        );

        // The non-intersection edge must NOT be present
        assert!(
            !classification.edges.contains_key(&edge1),
            "Non-intersection edge {:?} must not appear in classification",
            edge1,
        );

        // The two intersection edges must be present
        assert!(
            classification.edges.contains_key(&edge0),
            "Intersection edge {:?} must appear in classification",
            edge0,
        );
        assert!(
            classification.edges.contains_key(&edge2),
            "Intersection edge {:?} must appear in classification",
            edge2,
        );
    }

    // ── Adversarial: All curved surface types recognized as NeedsRefinement ──

    #[test]
    fn test_all_surface_types_as_curved() {
        // Verify Conical, Spherical, and Toroidal surfaces are each classified
        // as NeedsRefinement when paired with Planar. Tests each individually.
        let curved_surfaces = vec![
            (
                "Conical",
                SurfaceGeom::Conical(Cone {
                    apex: Point3::origin(),
                    axis: Vector3::new(0.0, 0.0, 1.0),
                    half_angle: std::f64::consts::FRAC_PI_4,
                }),
            ),
            (
                "Spherical",
                SurfaceGeom::Spherical(Sphere {
                    center: Point3::origin(),
                    radius: 5.0,
                }),
            ),
            (
                "Toroidal",
                SurfaceGeom::Toroidal(Torus {
                    center: Point3::origin(),
                    axis: Vector3::new(0.0, 0.0, 1.0),
                    major_radius: 5.0,
                    minor_radius: 1.0,
                }),
            ),
        ];

        let planar = SurfaceGeom::Planar(Plane {
            origin: Point3::origin(),
            normal: Vector3::new(0.0, 1.0, 0.0),
        });

        for (label, curved_geom) in curved_surfaces {
            // Build minimal topology with one intersection edge
            let mut arena = TopoArena::new();
            let solid = arena.add_solid();
            let shell = arena.add_shell(solid);

            let face0 = arena.add_face(shell);
            let face1 = arena.add_face(shell);
            let loop0 = arena.add_loop(face0);
            let loop1 = arena.add_loop(face1);
            arena.faces[face0.0].outer_loop = loop0;
            arena.faces[face1.0].outer_loop = loop1;

            let v0 = arena.add_vertex([0.0, 0.0, 0.0]);
            let v1 = arena.add_vertex([1.0, 0.0, 0.0]);

            let (edge_shared, he_a, he_b) = arena.add_edge();
            arena.half_edges[he_a.0].origin = v0;
            arena.half_edges[he_b.0].origin = v1;
            arena.half_edges[he_a.0].loop_ = loop0;
            arena.half_edges[he_b.0].loop_ = loop1;
            arena.half_edges[he_a.0].next = he_a;
            arena.half_edges[he_a.0].prev = he_a;
            arena.half_edges[he_b.0].next = he_b;
            arena.half_edges[he_b.0].prev = he_b;
            arena.loops[loop0.0].half_edge = he_a;
            arena.loops[loop1.0].half_edge = he_b;

            let mut face_provenance = BTreeMap::new();
            face_provenance.insert(
                face0,
                SourceFace {
                    mesh_id: MeshId::A,
                    face_idx: FaceIdx(0),
                },
            );
            face_provenance.insert(
                face1,
                SourceFace {
                    mesh_id: MeshId::B,
                    face_idx: FaceIdx(0),
                },
            );

            let mut edge_is_intersection = BTreeMap::new();
            edge_is_intersection.insert(edge_shared, true);

            let result = ResultTopology {
                arena,
                face_provenance,
                edge_is_intersection,
            };

            let mut surface_map = BTreeMap::new();
            surface_map.insert((MeshId::A, FaceIdx(0)), planar.clone());
            surface_map.insert((MeshId::B, FaceIdx(0)), curved_geom.clone());

            let classification = classify_intersection_edges(&result, &surface_map)
                .unwrap_or_else(|e| panic!("{label}: classification failed: {e:?}"));

            assert_eq!(
                classification.edges.len(),
                1,
                "{label}: should classify exactly 1 intersection edge",
            );

            match classification.edges.values().next().unwrap() {
                SurfacePairKind::NeedsRefinement { .. } => {
                    // Correct — curved surface paired with planar needs refinement
                }
                SurfacePairKind::PlanarPlanar => {
                    panic!("{label}: Planar + {label} must be NeedsRefinement, not PlanarPlanar",);
                }
            }
        }
    }

    // ══════════════════════════════════════════════════════════════════════
    // Phase 4b — SSI Curve Refinement tests (R-series)
    // These tests target `refine_intersection_edges` which is currently a
    // `todo!()` stub. All are marked `#[should_panic]` because the stub
    // panics at runtime.
    // ══════════════════════════════════════════════════════════════════════

    // ── R1: Empty classification returns empty refinement ──

    #[test]
    fn test_r1_empty_classification_returns_empty_refinement() {
        let result = ResultTopology {
            arena: TopoArena::new(),
            face_provenance: BTreeMap::new(),
            edge_is_intersection: BTreeMap::new(),
        };
        let classification = IntersectionEdgeClassification {
            edges: BTreeMap::new(),
        };
        let surface_map = BTreeMap::new();

        let refinement = refine_intersection_edges(&result, &classification, &surface_map)
            .expect("Empty classification should return Ok with empty refinement");

        assert!(
            refinement.edges.is_empty(),
            "Empty classification must produce empty refined edges, got {}",
            refinement.edges.len(),
        );
        assert_eq!(
            refinement.skipped_planar, 0,
            "Empty classification must skip 0 planar edges",
        );
        assert!(
            refinement.unsupported.is_empty(),
            "Empty classification must have no unsupported edges",
        );
    }

    // ── R2: Box-box subtract — all PlanarPlanar skipped ──

    #[test]
    fn test_r2_box_box_subtract_all_planar_skipped() {
        let result = run_full_pipeline(MeshBooleanOp::Subtract);
        let surface_map = planar_surface_map_for_boxes();

        let classification = classify_intersection_edges(&result, &surface_map)
            .expect("Classification should succeed for box-box subtract");

        assert!(
            !classification.edges.is_empty(),
            "Box-box subtract must produce intersection edges to classify"
        );

        let refinement = refine_intersection_edges(&result, &classification, &surface_map)
            .expect("Refinement should succeed for all-planar box-box subtract");

        assert!(
            refinement.edges.is_empty(),
            "All-planar box-box subtract should produce no refined curves, got {}",
            refinement.edges.len(),
        );
        assert!(
            refinement.skipped_planar > 0,
            "All-planar box-box subtract must skip at least one planar edge",
        );
        assert_eq!(
            refinement.skipped_planar,
            classification.edges.len(),
            "skipped_planar ({}) must equal classification count ({})",
            refinement.skipped_planar,
            classification.edges.len(),
        );
    }

    // ── R3: Plane-cylinder intersection → Circle SSICurve ──

    #[test]
    fn test_r3_plane_cylinder_produces_circle() {
        // Build a minimal 2-face topology with one intersection edge.
        // Face A: plane at z=5 with normal [0,0,1]
        // Face B: cylinder at origin with axis [0,0,1], radius 2.0
        // Expected SSI: circle at center [0,0,5], radius 2.0, normal [0,0,1]
        let mut arena = TopoArena::new();

        let solid = arena.add_solid();
        let shell = arena.add_shell(solid);

        let face0 = arena.add_face(shell);
        let face1 = arena.add_face(shell);
        let loop0 = arena.add_loop(face0);
        let loop1 = arena.add_loop(face1);
        arena.faces[face0.0].outer_loop = loop0;
        arena.faces[face1.0].outer_loop = loop1;

        // Vertices on the expected circle
        let _v0 = arena.add_vertex([2.0, 0.0, 5.0]);
        let _v1 = arena.add_vertex([-2.0, 0.0, 5.0]);

        let (edge_shared, he_a, he_b) = arena.add_edge();
        arena.half_edges[he_a.0].origin = _v0;
        arena.half_edges[he_b.0].origin = _v1;
        arena.half_edges[he_a.0].loop_ = loop0;
        arena.half_edges[he_b.0].loop_ = loop1;
        arena.half_edges[he_a.0].next = he_a;
        arena.half_edges[he_a.0].prev = he_a;
        arena.half_edges[he_b.0].next = he_b;
        arena.half_edges[he_b.0].prev = he_b;
        arena.loops[loop0.0].half_edge = he_a;
        arena.loops[loop1.0].half_edge = he_b;

        let mut face_provenance = BTreeMap::new();
        face_provenance.insert(
            face0,
            SourceFace {
                mesh_id: MeshId::A,
                face_idx: FaceIdx(0),
            },
        );
        face_provenance.insert(
            face1,
            SourceFace {
                mesh_id: MeshId::B,
                face_idx: FaceIdx(0),
            },
        );

        let mut edge_is_intersection = BTreeMap::new();
        edge_is_intersection.insert(edge_shared, true);

        let result = ResultTopology {
            arena,
            face_provenance,
            edge_is_intersection,
        };

        let mut surface_map = BTreeMap::new();
        surface_map.insert(
            (MeshId::A, FaceIdx(0)),
            SurfaceGeom::Planar(Plane {
                origin: Point3::new(0.0, 0.0, 5.0),
                normal: Vector3::new(0.0, 0.0, 1.0),
            }),
        );
        surface_map.insert(
            (MeshId::B, FaceIdx(0)),
            SurfaceGeom::Cylindrical(Cylinder {
                origin: Point3::origin(),
                axis: Vector3::new(0.0, 0.0, 1.0),
                radius: 2.0,
            }),
        );

        let classification = classify_intersection_edges(&result, &surface_map)
            .expect("Classification should succeed");

        let refinement = refine_intersection_edges(&result, &classification, &surface_map)
            .expect("Plane-cylinder refinement should succeed");

        assert_eq!(
            refinement.edges.len(),
            1,
            "Plane-cylinder intersection should produce exactly 1 refined curve",
        );

        let curve = refinement.edges.values().next().unwrap();
        match curve {
            SSICurve::Circle {
                center,
                normal,
                radius,
            } => {
                let tol = 1e-7;
                assert!(
                    (center[0]).abs() < tol
                        && (center[1]).abs() < tol
                        && (center[2] - 5.0).abs() < tol,
                    "Circle center should be near [0,0,5], got {:?}",
                    center,
                );
                assert!(
                    (normal[0]).abs() < tol
                        && (normal[1]).abs() < tol
                        && (normal[2] - 1.0).abs() < tol,
                    "Circle normal should be near [0,0,1], got {:?}",
                    normal,
                );
                assert!(
                    (radius - 2.0).abs() < tol,
                    "Circle radius should be 2.0, got {}",
                    radius,
                );
            }
            other => panic!(
                "Expected SSICurve::Circle for plane-cylinder intersection, got {:?}",
                other,
            ),
        }
    }

    // ── R4: Plane-sphere intersection → Circle SSICurve ──

    #[test]
    fn test_r4_plane_sphere_produces_circle() {
        // Face A: plane at z=3, normal [0,0,1]
        // Face B: sphere at origin, radius 5.0
        // Expected: circle at [0,0,3], normal [0,0,1], radius = sqrt(25 - 9) = 4.0
        let mut arena = TopoArena::new();

        let solid = arena.add_solid();
        let shell = arena.add_shell(solid);

        let face0 = arena.add_face(shell);
        let face1 = arena.add_face(shell);
        let loop0 = arena.add_loop(face0);
        let loop1 = arena.add_loop(face1);
        arena.faces[face0.0].outer_loop = loop0;
        arena.faces[face1.0].outer_loop = loop1;

        // Vertices on the expected circle (radius 4 at z=3)
        let v0 = arena.add_vertex([4.0, 0.0, 3.0]);
        let v1 = arena.add_vertex([-4.0, 0.0, 3.0]);

        let (edge_shared, he_a, he_b) = arena.add_edge();
        arena.half_edges[he_a.0].origin = v0;
        arena.half_edges[he_b.0].origin = v1;
        arena.half_edges[he_a.0].loop_ = loop0;
        arena.half_edges[he_b.0].loop_ = loop1;
        arena.half_edges[he_a.0].next = he_a;
        arena.half_edges[he_a.0].prev = he_a;
        arena.half_edges[he_b.0].next = he_b;
        arena.half_edges[he_b.0].prev = he_b;
        arena.loops[loop0.0].half_edge = he_a;
        arena.loops[loop1.0].half_edge = he_b;

        let mut face_provenance = BTreeMap::new();
        face_provenance.insert(
            face0,
            SourceFace {
                mesh_id: MeshId::A,
                face_idx: FaceIdx(0),
            },
        );
        face_provenance.insert(
            face1,
            SourceFace {
                mesh_id: MeshId::B,
                face_idx: FaceIdx(0),
            },
        );

        let mut edge_is_intersection = BTreeMap::new();
        edge_is_intersection.insert(edge_shared, true);

        let result = ResultTopology {
            arena,
            face_provenance,
            edge_is_intersection,
        };

        let mut surface_map = BTreeMap::new();
        surface_map.insert(
            (MeshId::A, FaceIdx(0)),
            SurfaceGeom::Planar(Plane {
                origin: Point3::new(0.0, 0.0, 3.0),
                normal: Vector3::new(0.0, 0.0, 1.0),
            }),
        );
        surface_map.insert(
            (MeshId::B, FaceIdx(0)),
            SurfaceGeom::Spherical(Sphere {
                center: Point3::origin(),
                radius: 5.0,
            }),
        );

        let classification = classify_intersection_edges(&result, &surface_map)
            .expect("Classification should succeed");

        let refinement = refine_intersection_edges(&result, &classification, &surface_map)
            .expect("Plane-sphere refinement should succeed");

        assert_eq!(
            refinement.edges.len(),
            1,
            "Plane-sphere intersection should produce exactly 1 refined curve",
        );

        let curve = refinement.edges.values().next().unwrap();
        match curve {
            SSICurve::Circle {
                center,
                normal,
                radius,
            } => {
                let tol = 1e-7;
                assert!(
                    (center[0]).abs() < tol
                        && (center[1]).abs() < tol
                        && (center[2] - 3.0).abs() < tol,
                    "Circle center should be near [0,0,3], got {:?}",
                    center,
                );
                assert!(
                    (normal[0]).abs() < tol
                        && (normal[1]).abs() < tol
                        && (normal[2] - 1.0).abs() < tol,
                    "Circle normal should be near [0,0,1], got {:?}",
                    normal,
                );
                assert!(
                    (radius - 4.0).abs() < tol,
                    "Circle radius should be 4.0 (sqrt(25-9)), got {}",
                    radius,
                );
            }
            other => panic!(
                "Expected SSICurve::Circle for plane-sphere intersection, got {:?}",
                other,
            ),
        }
    }

    // ── R5: NotSupported solver pair recorded ──

    #[test]
    fn test_r5_unsupported_solver_pair_recorded() {
        // Face A: cylindrical, Face B: toroidal — currently unsupported SSI pair
        let mut arena = TopoArena::new();

        let solid = arena.add_solid();
        let shell = arena.add_shell(solid);

        let face0 = arena.add_face(shell);
        let face1 = arena.add_face(shell);
        let loop0 = arena.add_loop(face0);
        let loop1 = arena.add_loop(face1);
        arena.faces[face0.0].outer_loop = loop0;
        arena.faces[face1.0].outer_loop = loop1;

        let v0 = arena.add_vertex([1.0, 0.0, 0.0]);
        let v1 = arena.add_vertex([0.0, 1.0, 0.0]);

        let (edge_shared, he_a, he_b) = arena.add_edge();
        arena.half_edges[he_a.0].origin = v0;
        arena.half_edges[he_b.0].origin = v1;
        arena.half_edges[he_a.0].loop_ = loop0;
        arena.half_edges[he_b.0].loop_ = loop1;
        arena.half_edges[he_a.0].next = he_a;
        arena.half_edges[he_a.0].prev = he_a;
        arena.half_edges[he_b.0].next = he_b;
        arena.half_edges[he_b.0].prev = he_b;
        arena.loops[loop0.0].half_edge = he_a;
        arena.loops[loop1.0].half_edge = he_b;

        let mut face_provenance = BTreeMap::new();
        face_provenance.insert(
            face0,
            SourceFace {
                mesh_id: MeshId::A,
                face_idx: FaceIdx(0),
            },
        );
        face_provenance.insert(
            face1,
            SourceFace {
                mesh_id: MeshId::B,
                face_idx: FaceIdx(0),
            },
        );

        let mut edge_is_intersection = BTreeMap::new();
        edge_is_intersection.insert(edge_shared, true);

        let result = ResultTopology {
            arena,
            face_provenance,
            edge_is_intersection,
        };

        let mut surface_map = BTreeMap::new();
        surface_map.insert(
            (MeshId::A, FaceIdx(0)),
            SurfaceGeom::Cylindrical(Cylinder {
                origin: Point3::origin(),
                axis: Vector3::new(0.0, 0.0, 1.0),
                radius: 3.0,
            }),
        );
        surface_map.insert(
            (MeshId::B, FaceIdx(0)),
            SurfaceGeom::Toroidal(Torus {
                center: Point3::origin(),
                axis: Vector3::new(0.0, 0.0, 1.0),
                major_radius: 5.0,
                minor_radius: 1.0,
            }),
        );

        let classification = classify_intersection_edges(&result, &surface_map)
            .expect("Classification should succeed for cyl-torus pair");

        let refinement = refine_intersection_edges(&result, &classification, &surface_map)
            .expect("Refinement should return Ok even for unsupported pairs");

        assert!(
            refinement.edges.is_empty(),
            "Unsupported solver pair should produce no refined curves, got {}",
            refinement.edges.len(),
        );
        assert_eq!(
            refinement.unsupported.len(),
            1,
            "Unsupported solver pair should record exactly 1 unsupported entry, got {}",
            refinement.unsupported.len(),
        );
    }

    // ── R6: Count conservation ──

    #[test]
    fn test_r6_count_conservation() {
        // For box-box subtract (all planar), verify that:
        // skipped_planar + edges.len() + unsupported.len() == classification.edges.len()
        let result = run_full_pipeline(MeshBooleanOp::Subtract);
        let surface_map = planar_surface_map_for_boxes();

        let classification = classify_intersection_edges(&result, &surface_map)
            .expect("Classification should succeed for box-box subtract");

        let total_classified = classification.edges.len();
        assert!(
            total_classified > 0,
            "Must have intersection edges to test conservation"
        );

        let refinement = refine_intersection_edges(&result, &classification, &surface_map)
            .expect("Refinement should succeed for all-planar box-box subtract");

        let total_accounted =
            refinement.skipped_planar + refinement.edges.len() + refinement.unsupported.len();

        assert_eq!(
            total_accounted, total_classified,
            "Count conservation violated: skipped({}) + refined({}) + unsupported({}) = {} != classified({})",
            refinement.skipped_planar,
            refinement.edges.len(),
            refinement.unsupported.len(),
            total_accounted,
            total_classified,
        );
    }
}
