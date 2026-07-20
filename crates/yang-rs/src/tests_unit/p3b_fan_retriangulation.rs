//! P3b inc-4c (spec `yang_169_p3b_inc4c_fan_retriangulation.md`): the §4.4.1
//! post-merge fan re-triangulation. The fixture reproduces the measured R0061
//! mechanism ORGANICALLY: a manifold two-operand strip whose seam carries two
//! Stage-1 mints `P`, `G` with a victim cluster between them; running the
//! production `collapse_vertex` on the victims (the weld/trim shape) folds the
//! strip — the mint-pair edge (P,G) picks up >2 triangles with DISTINCT kept
//! tips (the class no exact-duplicate rule can touch). The repair must
//! re-triangulate the merged fans back to a closed complex without moving any
//! vertex or touching any healthy boundary edge.

use super::super::stage4_correct::{collapse_vertex, retriangulate_collapsed_fan_regions};
use crate::{
    BRep, BRepEdge, BRepFace, BRepVertex, Curve, InputId, Mesh, Surface, TriangleAttribution,
    Vector3,
};
use cad_primitives::Point3;
use std::collections::{BTreeMap, HashSet};

fn p(x: f64, y: f64) -> Point3 {
    Point3::new(x, y, 0.0)
}

/// One-face BRep whose face 0 has the given surface (the repair only reads
/// `faces()[key.face].surface` for the chart).
fn one_face_brep(surface: Surface) -> BRep {
    let vertices = vec![
        BRepVertex {
            point: Point3::new(0.0, 0.0, 0.0),
        },
        BRepVertex {
            point: Point3::new(1.0, 0.0, 0.0),
        },
        BRepVertex {
            point: Point3::new(0.0, 1.0, 0.0),
        },
    ];
    let edges = vec![
        BRepEdge {
            start: 0,
            end: 1,
            curve: Curve::LineSegment,
        },
        BRepEdge {
            start: 1,
            end: 2,
            curve: Curve::LineSegment,
        },
        BRepEdge {
            start: 2,
            end: 0,
            curve: Curve::LineSegment,
        },
    ];
    let faces = vec![BRepFace {
        surface,
        outer_loop: vec![0, 1, 2],
        inner_loops: Vec::new(),
        reversed: false,
    }];
    BRep::new(vertices, edges, faces).unwrap()
}

fn z_plane() -> Surface {
    Surface::Plane {
        normal: Vector3::new(0.0, 0.0, 1.0),
        d: 0.0,
    }
}

/// Vertex ids for the strip fixture.
#[allow(clippy::upper_case_acronyms)]
struct Ids {
    s0: u32,
    pp: u32,
    v1: u32,
    v2: u32,
    gg: u32,
    s3: u32,
}

/// The pre-collapse MANIFOLD strip: operand-A triangles above the seam
/// polyline s0—P—V1—V2—G—s3, operand-B below. The A side has a fan cell
/// between the victims with TWO kept interior verts (cu, cv) so that the
/// collapses leave two distinct-tip cross triangles (the fold), plus an upper
/// row through a third kept vert r1 (a third cross). Returns
/// (mesh, attributions, ids).
fn folded_strip_fixture() -> (Mesh, Vec<Option<TriangleAttribution>>, Ids) {
    let verts = vec![
        p(0.0, 0.0),   //  0 s0
        p(1.0, 0.0),   //  1 P   (mint)
        p(1.4, 0.0),   //  2 V1  (victim -> P)
        p(1.6, 0.0),   //  3 V2  (victim -> G)
        p(2.0, 0.0),   //  4 G   (mint)
        p(3.0, 0.0),   //  5 s3
        p(1.0, 0.5),   //  6 q1
        p(1.4, 0.5),   //  7 m1  (victim -> P)
        p(1.6, 0.5),   //  8 m2  (victim -> G)
        p(2.0, 0.5),   //  9 q2
        p(1.45, 0.3),  // 10 cu  (kept cell vert)
        p(1.55, 0.35), // 11 cv  (kept cell vert)
        p(1.5, 1.0),   // 12 r1  (kept rim vert)
        p(2.5, 1.0),   // 13 r2
        p(0.5, -1.0),  // 14 b0
        p(1.5, -1.0),  // 15 b1
        p(2.5, -1.0),  // 16 b2
    ];
    let (s0, pp, v1, v2, gg, s3) = (0u32, 1u32, 2u32, 3u32, 4u32, 5u32);
    let (q1, m1, m2, q2, cu, cv, r1, r2) = (6u32, 7u32, 8u32, 9u32, 10u32, 11u32, 12u32, 13u32);
    let (b0, b1, b2) = (14u32, 15u32, 16u32);
    let a_tris = vec![
        [s0, pp, q1],
        [pp, v1, m1],
        [pp, m1, q1],
        // The victim cell (V1,V2,m2,m1) fanned around the two kept verts.
        [v1, v2, cu],
        [v2, m2, cu],
        [m2, cv, cu],
        [m2, m1, cv],
        [m1, cu, cv],
        [v1, cu, m1],
        [v2, gg, q2],
        [v2, q2, m2],
        // Upper row.
        [q1, m1, r1],
        [m1, m2, r1],
        [m2, q2, r1],
        [q2, r2, r1],
        [gg, s3, q2],
        [s3, r2, q2],
    ];
    let b_tris = vec![
        [pp, s0, b0],
        [pp, b0, b1],
        [v1, pp, b1],
        [v2, v1, b1],
        [v2, b1, b2],
        [gg, v2, b2],
        [s3, gg, b2],
    ];
    let mut tris = Vec::new();
    let mut attr = Vec::new();
    for t in &a_tris {
        tris.push(*t);
        attr.push(Some(TriangleAttribution {
            input: InputId::A,
            face: 0,
        }));
    }
    for t in &b_tris {
        tris.push(*t);
        attr.push(Some(TriangleAttribution {
            input: InputId::B,
            face: 0,
        }));
    }
    (
        Mesh::new(verts, tris),
        attr,
        Ids {
            s0,
            pp,
            v1,
            v2,
            gg,
            s3,
        },
    )
}

/// Undirected edge → (total uses, forward uses, reverse uses).
fn edge_profile(mesh: &Mesh) -> BTreeMap<(u32, u32), (usize, usize, usize)> {
    let mut m: BTreeMap<(u32, u32), (usize, usize, usize)> = BTreeMap::new();
    for tri in &mesh.tris {
        for (i, j) in [(0usize, 1usize), (1, 2), (2, 0)] {
            let (u, w) = (tri[i], tri[j]);
            let e = if u < w { (u, w) } else { (w, u) };
            let ent = m.entry(e).or_default();
            ent.0 += 1;
            if (u, w) == e {
                ent.1 += 1;
            } else {
                ent.2 += 1;
            }
        }
    }
    m
}

/// The pre-collapse fixture must be a valid manifold-with-boundary: every
/// edge has 1 use (fixture rim) or 2 uses in opposite directions.
#[test]
fn fixture_strip_is_manifold_with_boundary() {
    let (mesh, _, _) = folded_strip_fixture();
    for (e, &(n, fwd, rev)) in &edge_profile(&mesh) {
        assert!(
            n == 1 || (n == 2 && fwd == 1 && rev == 1),
            "edge {e:?} has profile n={n} fwd={fwd} rev={rev}"
        );
    }
}

/// Applying the production collapses (weld/trim shape) folds the strip: the
/// mint-pair edge (P,G) ends with >2 incident triangles whose tips are
/// DISTINCT kept vertices — no exact-duplicate rule can fire.
fn collapsed_fixture() -> (Mesh, Vec<Option<TriangleAttribution>>, Ids) {
    let (mut mesh, mut attr, ids) = folded_strip_fixture();
    let (v1, v2, m1, m2) = (2u32, 3u32, 7u32, 8u32);
    collapse_vertex(&mut mesh, &mut attr, v1, ids.pp);
    collapse_vertex(&mut mesh, &mut attr, v2, ids.gg);
    collapse_vertex(&mut mesh, &mut attr, m1, ids.pp);
    collapse_vertex(&mut mesh, &mut attr, m2, ids.gg);
    (mesh, attr, ids)
}

#[test]
fn stacked_collapses_manufacture_the_fold() {
    let (mesh, _, ids) = collapsed_fixture();
    let prof = edge_profile(&mesh);
    let pg = prof
        .get(&(ids.pp.min(ids.gg), ids.pp.max(ids.gg)))
        .copied()
        .unwrap_or((0, 0, 0));
    assert!(
        pg.0 > 2,
        "the mint-pair edge must be over-used post-collapse, got {pg:?}"
    );
}

#[test]
fn fan_fold_is_retriangulated_to_a_closed_complex() {
    let (mut mesh, mut attr, ids) = collapsed_fixture();
    let brep_a = one_face_brep(z_plane());
    let brep_b = one_face_brep(z_plane());
    let minted: HashSet<u32> = [ids.pp, ids.gg].into_iter().collect();
    let changed =
        retriangulate_collapsed_fan_regions(&mut mesh, &mut attr, &brep_a, &brep_b, &minted);
    assert!(changed, "the repair must fire on the folded cluster");
    for (e, &(n, fwd, rev)) in &edge_profile(&mesh) {
        assert!(
            n == 1 || (n == 2 && fwd == 1 && rev == 1),
            "post-repair edge {e:?} has profile n={n} fwd={fwd} rev={rev}"
        );
    }
    // The seam anchors stayed referenced (no geometry was touched at all —
    // the repair is connectivity-only).
    let live: HashSet<u32> = mesh.tris.iter().flatten().copied().collect();
    for v in [ids.s0, ids.pp, ids.gg, ids.s3] {
        assert!(live.contains(&v), "seam vertex {v} lost by the repair");
    }
}

#[test]
fn mint_free_fold_is_left_alone() {
    let (mut mesh, mut attr, _) = collapsed_fixture();
    let brep_a = one_face_brep(z_plane());
    let brep_b = one_face_brep(z_plane());
    let before = mesh.tris.clone();
    let changed = retriangulate_collapsed_fan_regions(
        &mut mesh,
        &mut attr,
        &brep_a,
        &brep_b,
        &HashSet::new(),
    );
    assert!(!changed, "no mints -> detector must not fire");
    assert_eq!(mesh.tris, before, "mesh must be untouched");
}

#[test]
fn unattributed_cluster_triangle_bails_without_mutation() {
    let (mut mesh, mut attr, ids) = collapsed_fixture();
    // Strip the attribution of one triangle incident to the mint pair: the
    // repair has no surface to re-CDT in and must bail, mesh untouched.
    let ti = mesh
        .tris
        .iter()
        .position(|t| t.contains(&ids.pp) && t.contains(&ids.gg))
        .expect("a fold triangle exists");
    attr[ti] = None;
    let brep_a = one_face_brep(z_plane());
    let brep_b = one_face_brep(z_plane());
    let minted: HashSet<u32> = [ids.pp, ids.gg].into_iter().collect();
    let before = mesh.tris.clone();
    let changed =
        retriangulate_collapsed_fan_regions(&mut mesh, &mut attr, &brep_a, &brep_b, &minted);
    assert!(!changed, "an unattributed cluster triangle must bail");
    assert_eq!(mesh.tris, before, "bail must leave the mesh untouched");
}
