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
}
