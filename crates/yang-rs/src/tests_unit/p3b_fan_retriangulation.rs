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
    let changed = retriangulate_collapsed_fan_regions(
        &mut mesh,
        &mut attr,
        &brep_a,
        &brep_b,
        &HashSet::new(),
        &minted,
    );
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
    let changed = retriangulate_collapsed_fan_regions(
        &mut mesh,
        &mut attr,
        &brep_a,
        &brep_b,
        &HashSet::new(),
        &minted,
    );
    assert!(!changed, "an unattributed cluster triangle must bail");
    assert_eq!(mesh.tris, before, "bail must leave the mesh untouched");
}

/// inc-4c-2 fixtures: the strip fixture extended with a MISORDERED seam
/// chain between G and s3 — two extra samples x1 < x2 (by curve parameter)
/// wired stale as G→x2→x1→s3. Operand A lies on the z=0 plane, operand B on
/// the y=0 plane, so the seam is the x-axis and the pair has a genuine line
/// parameter. `off` is the samples' transverse offset: above the render
/// floor they must be KEPT and reordered; below it they are §4.3-dropped.
fn misordered_chain_fixture(off: f64) -> (Mesh, Vec<Option<TriangleAttribution>>, Ids, u32, u32) {
    let p3 = |x: f64, y: f64, z: f64| Point3::new(x, y, z);
    let verts = vec![
        p3(0.0, 0.0, 0.0),   //  0 s0
        p3(1.0, 0.0, 0.0),   //  1 P   (mint)
        p3(1.4, 0.0, 0.0),   //  2 V1  (victim -> P)
        p3(1.6, 0.0, 0.0),   //  3 V2  (victim -> G)
        p3(2.0, 0.0, 0.0),   //  4 G   (mint)
        p3(3.0, 0.0, 0.0),   //  5 s3
        p3(1.0, 0.5, 0.0),   //  6 q1
        p3(1.4, 0.5, 0.0),   //  7 m1  (victim -> P)
        p3(1.6, 0.5, 0.0),   //  8 m2  (victim -> G)
        p3(2.0, 0.5, 0.0),   //  9 q2
        p3(1.45, 0.3, 0.0),  // 10 cu
        p3(1.55, 0.35, 0.0), // 11 cv
        p3(1.5, 1.0, 0.0),   // 12 r1
        p3(2.5, 1.0, 0.0),   // 13 r2
        p3(0.5, 0.0, -1.0),  // 14 b0 (on B's y=0 plane)
        p3(1.5, 0.0, -1.0),  // 15 b1
        p3(2.5, 0.0, -1.0),  // 16 b2
        p3(2.4, off, 0.0),   // 17 x1 (chain sample, true order G < x1 < x2)
        p3(2.6, -off, 0.0),  // 18 x2
    ];
    let (s0, pp, v1, v2, gg, s3) = (0u32, 1u32, 2u32, 3u32, 4u32, 5u32);
    let (q1, m1, m2, q2, cu, cv, r1, r2) = (6u32, 7u32, 8u32, 9u32, 10u32, 11u32, 12u32, 13u32);
    let (b0, b1, b2) = (14u32, 15u32, 16u32);
    let (x1, x2) = (17u32, 18u32);
    let a_tris = vec![
        [s0, pp, q1],
        [pp, v1, m1],
        [pp, m1, q1],
        [v1, v2, cu],
        [v2, m2, cu],
        [m2, cv, cu],
        [m2, m1, cv],
        [m1, cu, cv],
        [v1, cu, m1],
        [v2, gg, q2],
        [v2, q2, m2],
        [q1, m1, r1],
        [m1, m2, r1],
        [m2, q2, r1],
        [q2, r2, r1],
        // The misordered right span: seam runs G -> x2 -> x1 -> s3.
        [gg, x2, q2],
        [x2, x1, q2],
        [x1, s3, q2],
        [s3, r2, q2],
    ];
    let b_tris = vec![
        [pp, s0, b0],
        [pp, b0, b1],
        [v1, pp, b1],
        [v2, v1, b1],
        [v2, b1, b2],
        [gg, v2, b2],
        // Mirror of the misordered span.
        [x2, gg, b2],
        [x1, x2, b2],
        [s3, x1, b2],
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
        x1,
        x2,
    )
}

fn y_plane() -> Surface {
    Surface::Plane {
        normal: Vector3::new(0.0, 1.0, 0.0),
        d: 0.0,
    }
}

fn run_misordered(off: f64) -> (Mesh, Ids, u32, u32) {
    let (mut mesh, mut attr, ids, x1, x2) = misordered_chain_fixture(off);
    // Pre-collapse manifold sanity.
    for (e, &(n, fwd, rev)) in &edge_profile(&mesh) {
        assert!(
            n == 1 || (n == 2 && fwd == 1 && rev == 1),
            "pre edge {e:?} profile n={n} fwd={fwd} rev={rev}"
        );
    }
    let (v1, v2, m1, m2) = (2u32, 3u32, 7u32, 8u32);
    collapse_vertex(&mut mesh, &mut attr, v1, ids.pp);
    collapse_vertex(&mut mesh, &mut attr, v2, ids.gg);
    collapse_vertex(&mut mesh, &mut attr, m1, ids.pp);
    collapse_vertex(&mut mesh, &mut attr, m2, ids.gg);
    let brep_a = one_face_brep(z_plane());
    let brep_b = one_face_brep(y_plane());
    let minted: HashSet<u32> = [ids.pp, ids.gg].into_iter().collect();
    let moved: HashSet<u32> = [x1, x2].into_iter().collect();
    let changed = retriangulate_collapsed_fan_regions(
        &mut mesh, &mut attr, &brep_a, &brep_b, &moved, &minted,
    );
    assert!(changed, "the repair must fire on the folded cluster");
    for (e, &(n, fwd, rev)) in &edge_profile(&mesh) {
        assert!(
            n == 1 || (n == 2 && fwd == 1 && rev == 1),
            "post edge {e:?} profile n={n} fwd={fwd} rev={rev}"
        );
    }
    (mesh, ids, x1, x2)
}

/// A geometric zigzag OUTSIDE the cluster regions is not this pass's
/// business: the fold still repairs, the mesh stays manifold, and the
/// out-of-region stale chord survives untouched (scope discipline — the
/// pass may only rewire chains whose triangles it is already re-CDTing).
#[test]
fn out_of_region_zigzag_is_left_alone() {
    let (mesh, ids, _x1, x2) = run_misordered(0.01);
    let prof = edge_profile(&mesh);
    assert!(
        prof.contains_key(&(ids.gg.min(x2), ids.gg.max(x2))),
        "the out-of-region chain edge (G,x2) must survive untouched"
    );
}

// ---- seam_run_params: the inc-4c-2 curve parameter. ----

use super::super::stage4_correct::seam_run_params;

#[test]
fn seam_params_plane_plane_orders_along_the_intersection_line() {
    // z=0 x y=0 intersect in the x-axis: the parameter must be monotone in x.
    let verts = vec![
        Point3::new(2.0, 0.0, 0.0),
        Point3::new(2.6, -1.0e-8, 0.0),
        Point3::new(2.4, 1.0e-8, 0.0),
        Point3::new(3.0, 0.0, 0.0),
    ];
    let mesh = Mesh::new(verts, vec![[0, 1, 2]]);
    let params = seam_run_params(z_plane(), y_plane(), &[0, 1, 2, 3], &mesh)
        .expect("plane x plane has a line parameter");
    // Sorting by the parameter recovers the true x-order 0 < 2 < 1 < 3.
    let mut order: Vec<usize> = (0..4).collect();
    order.sort_by(|&a, &b| params[a].partial_cmp(&params[b]).unwrap());
    assert!(
        order == [0, 2, 1, 3] || order == [3, 1, 2, 0],
        "params must order along the seam line, got {order:?}"
    );
}

#[test]
fn seam_params_plane_cylinder_orders_along_theta() {
    let cyl = Surface::Cylinder {
        axis_point: Point3::new(0.0, 0.0, 0.0),
        axis_dir: Vector3::new(0.0, 0.0, 1.0),
        radius: 1.0,
    };
    // A tilted plane cuts an ellipse; three samples at increasing theta,
    // listed out of order.
    let p_at = |theta: f64| Point3::new(theta.cos(), theta.sin(), 0.3 * theta.sin());
    let verts = vec![p_at(0.30), p_at(0.10), p_at(0.20)];
    let mesh = Mesh::new(verts, vec![[0, 1, 2]]);
    let plane = Surface::Plane {
        normal: Vector3::new(0.0, -0.287_347_885_566_345, 0.957_826_285_221_1),
        d: 0.0,
    };
    let params =
        seam_run_params(plane, cyl, &[0, 1, 2], &mesh).expect("ellipse has a theta parameter");
    let mut order: Vec<usize> = (0..3).collect();
    order.sort_by(|&a, &b| params[a].partial_cmp(&params[b]).unwrap());
    assert!(
        order == [1, 2, 0] || order == [0, 2, 1],
        "params must order along theta, got {order:?}"
    );
}

#[test]
fn seam_params_unsupported_pairs_are_none() {
    let verts = vec![Point3::new(0.0, 0.0, 0.0), Point3::new(1.0, 0.0, 0.0)];
    let mesh = Mesh::new(verts, vec![[0, 1, 0]]);
    let sphere = Surface::Sphere {
        center: Point3::new(0.0, 0.0, 0.0),
        radius: 1.0,
    };
    assert!(
        seam_run_params(z_plane(), sphere, &[0, 1], &mesh).is_none(),
        "plane x sphere has no supported parameter"
    );
    assert!(
        seam_run_params(z_plane(), z_plane(), &[0, 1], &mesh).is_none(),
        "parallel planes have no line direction"
    );
}

// ---- paper_chain_sample_redundant: the Yang §4.3.4 h/l/α acceptance test
// (d_p = 1e-7 = TAU_MODEL, `refs/text/yang2025_hybrid_boolean.txt:586-592,744`).

use super::super::stage4_correct::paper_chain_sample_redundant;

#[test]
fn paper_test_drops_a_needle_inside_all_three_bounds() {
    // Neighbours 1e-5 apart, sample 1e-8 off-chord: h ≪ d_p·10²,
    // l ≪ d_p·10³, α ≈ 0 — the paper's refinement would never insert it.
    let a = [0.0, 0.0, 0.0];
    let m = [5.0e-6, 1.0e-8, 0.0];
    let b = [1.0e-5, 0.0, 0.0];
    assert!(paper_chain_sample_redundant(a, m, b));
}

#[test]
fn paper_test_keeps_a_sample_beyond_the_chord_length_bound() {
    // Spacing 1.0 ≫ d_p·10³ (~1e-4): a genuinely load-bearing sample.
    let a = [0.0, 0.0, 0.0];
    let m = [0.5, 1.0e-8, 0.0];
    let b = [1.0, 0.0, 0.0];
    assert!(!paper_chain_sample_redundant(a, m, b));
}

#[test]
fn paper_test_keeps_a_sample_beyond_the_arc_height_bound() {
    // l ≈ 7.8e-5 < d_p·10³ but h = 6e-5 > d_p·10² (~1e-5): real curvature.
    let a = [0.0, 0.0, 0.0];
    let m = [5.0e-5, 6.0e-5, 0.0];
    let b = [1.0e-4, 0.0, 0.0];
    assert!(!paper_chain_sample_redundant(a, m, b));
}

#[test]
fn paper_test_keeps_a_sample_beyond_the_turning_angle_bound() {
    // Tiny h and l but a 20° turn at m (> π/18): a genuine corner sample.
    let a = [0.0, 0.0, 0.0];
    let m = [2.0e-6, 0.0, 0.0];
    let turn = 20.0_f64.to_radians();
    let b = [2.0e-6 + 2.0e-6 * turn.cos(), 2.0e-6 * turn.sin(), 0.0];
    assert!(!paper_chain_sample_redundant(a, m, b));
}
