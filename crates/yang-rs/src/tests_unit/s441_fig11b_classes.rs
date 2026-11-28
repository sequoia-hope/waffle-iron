//! Yang §4.4.1 Fig. 11(b) (`refs/text/yang2025_hybrid_boolean.txt:562-563`):
//! the merge classes of the update's CDT input
//! (`stage4_fig11b::fig11b_vertex_classes`, spec
//! `specs/yang_f2_fig11b_cdt_input_merge.md` §3).
//!
//! The twin fixture is the F0082 Extrude-12 magnitude: one junction minted
//! twice, 5.5e-14 apart at coordinate scale ~2, against a band of
//! `TAU_WORK·(1 + 2.09…)` ≈ 3.1e-12.

use super::m4_substitute::p;
use crate::param_surface::carried::FaceRef;
use crate::stage4_fig11b::{fig11b_vertex_classes, pair_kind, Fig11bClasses, PairKind};
use crate::*;
use cad_primitives::TAU_WORK;
use std::collections::BTreeSet;

fn of(input: InputId) -> Option<TriangleAttribution> {
    Some(TriangleAttribution { input, face: 0 })
}

const X: f64 = 0.309456248416426;
const Y: f64 = 0.092020830714880;
const Z: f64 = 2.094303729583326;

/// Vertex 0 and 1 are the twins; 2, 3, 4 are far.
fn twin_mesh() -> Mesh {
    Mesh::new(
        vec![
            p(X, Y, Z),
            p(X + 5.5e-14, Y, Z),
            p(1.0, 0.0, 2.0),
            p(0.0, 1.0, 2.0),
            p(-1.0, 0.0, 2.0),
        ],
        vec![[0, 1, 2], [0, 2, 3], [1, 3, 4]],
    )
}

fn classes(mesh: &Mesh, attr: &[Option<TriangleAttribution>]) -> Fig11bClasses {
    fig11b_vertex_classes(mesh, attr, None, &|_| 0)
}

/// Fig. 11(b): vertex 1 is an intersection point (its star holds A and B),
/// vertex 0 a vertex of A alone. One class; the intersection point survives
/// though its index is the higher.
#[test]
fn a_mesh_vertex_joins_the_intersection_point_beside_it() {
    let mesh = twin_mesh();
    let attr = vec![of(InputId::A), of(InputId::A), of(InputId::B)];
    let c = classes(&mesh, &attr);
    assert_eq!(c.classes.len(), 1);
    assert!(c.too_wide.is_empty());
    assert_eq!(c.classes[0].members, vec![0, 1]);
    assert_eq!(c.classes[0].survivor, 1);
    assert_eq!(c.survivor_map(), [(0, 1)].into_iter().collect());
}

/// Two vertices of ONE operand's mesh, however close, form no class: the
/// paper's update never merges two original mesh vertices (population 4 of
/// the N89 item 1 census, a Stage-1 defect, F5b).
#[test]
fn two_vertices_of_one_operand_form_no_class() {
    let mesh = twin_mesh();
    let attr = vec![of(InputId::A); 3];
    assert_eq!(classes(&mesh, &attr), Fig11bClasses::default());
}

/// Two indices at exactly one position are one point: they form a class
/// (the N55 scan merges such pairs today; census 2026-11-25).
#[test]
fn an_exact_duplicate_is_one_class() {
    let mut mesh = twin_mesh();
    mesh.verts[1] = mesh.verts[0];
    let attr = vec![of(InputId::A), of(InputId::A), of(InputId::B)];
    let c = classes(&mesh, &attr);
    assert_eq!(c.classes.len(), 1);
    assert_eq!(c.classes[0].members, vec![0, 1]);
    assert_eq!(c.classes[0].survivor, 1, "the intersection point");
    assert_eq!(c.classes[0].diameter, 0.0);
}

/// Two intersection points: the caller's exactness rank decides, not the
/// index; at equal rank the least position, not the index.
#[test]
fn the_survivor_is_ranked_then_least_position() {
    let mesh = twin_mesh();
    // Both twins on A and B: 0 in triangles 0 (B) and 1 (A), 1 in
    // triangles 0 (B) and 2 (A).
    let attr = vec![of(InputId::B), of(InputId::A), of(InputId::A)];
    let ranked = fig11b_vertex_classes(&mesh, &attr, None, &|v| u8::from(v == 1));
    assert_eq!(ranked.classes[0].survivor, 1, "rank wins over index");
    // Equal rank: vertex 0 has the smaller x.
    assert_eq!(classes(&mesh, &attr).classes[0].survivor, 0);
}

/// The property the index-order scan lacks (N55 amendment 2026-11-25): the
/// partition and the survivor's POSITION are the same under any numbering
/// of the vertices.
#[test]
fn classes_do_not_depend_on_vertex_numbering() {
    // Three intersection points within the band of each other, one A-only
    // vertex beside them, and far vertices; two triangles join them.
    let base = vec![
        p(X, Y, Z),
        p(X + 4e-13, Y, Z),
        p(X, Y + 3e-13, Z),
        p(X + 2e-13, Y + 2e-13, Z),
        p(1.0, 0.0, 2.0),
        p(0.0, 1.0, 2.0),
    ];
    let tris = vec![[0, 1, 4], [1, 2, 5], [2, 3, 4], [0, 5, 4]];
    let attr = vec![
        of(InputId::A),
        of(InputId::B),
        of(InputId::A),
        of(InputId::B),
    ];
    // Vertex 3 sits only in triangle 2 (A): it is a mesh vertex of A.
    let positions = |m: &Mesh, c: &Fig11bClasses| {
        let mut out: Vec<(Vec<[u64; 3]>, [u64; 3])> = c
            .classes
            .iter()
            .map(|cl| {
                let key = |v: u32| {
                    let a = m.verts[v as usize].as_array();
                    [a[0].to_bits(), a[1].to_bits(), a[2].to_bits()]
                };
                let mut ms: Vec<[u64; 3]> = cl.members.iter().map(|&v| key(v)).collect();
                ms.sort_unstable();
                (ms, key(cl.survivor))
            })
            .collect();
        out.sort();
        out
    };
    let mesh = Mesh::new(base.clone(), tris.clone());
    let want = positions(&mesh, &classes(&mesh, &attr));
    assert_eq!(want.len(), 1, "one class");
    assert_eq!(want[0].0.len(), 4, "all four close vertices");

    // Every rotation and the reversal of the numbering.
    let n = base.len();
    let mut perms: Vec<Vec<usize>> = (0..n)
        .map(|r| (0..n).map(|i| (i + r) % n).collect())
        .collect();
    perms.push((0..n).rev().collect());
    for perm in perms {
        // perm[old] = new
        let mut verts = vec![base[0]; n];
        for (old, &new) in perm.iter().enumerate() {
            verts[new] = base[old];
        }
        let t: Vec<[u32; 3]> = tris
            .iter()
            .map(|tri| tri.map(|v| perm[v as usize] as u32))
            .collect();
        let m = Mesh::new(verts, t);
        assert_eq!(positions(&m, &classes(&m, &attr)), want, "perm {perm:?}");
    }
}

/// A chain of close vertices is not one point: each link is inside the
/// band, the chain's ends are not. The component is reported, not merged.
#[test]
fn a_chain_wider_than_the_band_is_left_alone() {
    let band = TAU_WORK * (1.0 + Z);
    let step = 0.7 * band;
    let mesh = Mesh::new(
        vec![
            p(X, Y, Z),
            p(X + step, Y, Z),
            p(X + 2.0 * step, Y, Z),
            p(1.0, 0.0, 2.0),
        ],
        vec![[0, 1, 3], [1, 2, 3]],
    );
    let attr = vec![of(InputId::A), of(InputId::B)];
    let c = classes(&mesh, &attr);
    assert!(c.classes.is_empty());
    assert_eq!(c.too_wide.len(), 1);
    assert_eq!(c.too_wide[0].members, vec![0, 1, 2]);
    assert!(c.survivor_map().is_empty());
}

fn faces(list: &[(InputId, u32)]) -> BTreeSet<FaceRef> {
    list.iter()
        .map(|&(input, face)| FaceRef { input, face })
        .collect()
}

/// Fig. 11(b) (`:562-563`): p is a mesh vertex on the edge between two faces
/// of A, q the point where the intersection curve with a face of B meets
/// that edge. In either order.
#[test]
fn a_boundary_vertex_beside_the_intersection_point_on_its_curve_is_fig11b() {
    use InputId::{A, B};
    let p = faces(&[(A, 0), (A, 1)]);
    let q = faces(&[(A, 0), (A, 1), (B, 4)]);
    assert_eq!(pair_kind(&p, &q), PairKind::BoundaryEndpoint);
    assert_eq!(pair_kind(&q, &p), PairKind::BoundaryEndpoint);
}

/// §4.4.1 (`:563-564`): a mesh vertex inside one face, beside an
/// intersection point on that face, is the near-curve removal, not Fig. 11(b).
#[test]
fn an_interior_vertex_beside_the_curve_is_the_near_curve_removal() {
    use InputId::{A, B};
    let p = faces(&[(A, 0)]);
    let q = faces(&[(A, 0), (B, 4)]);
    assert_eq!(pair_kind(&p, &q), PairKind::NearCurveInterior);
    assert_eq!(pair_kind(&q, &p), PairKind::NearCurveInterior);
}

/// §4.3.3 (`:535-536`): two intersection points on the same two faces are
/// two points of one loop; where one also lies on a further face it is the
/// loop's end there.
#[test]
fn two_intersection_points_are_one_curve_or_its_end() {
    use InputId::{A, B};
    let on_curve = faces(&[(A, 0), (B, 4)]);
    let at_end = faces(&[(A, 0), (A, 1), (B, 4)]);
    assert_eq!(pair_kind(&on_curve, &on_curve), PairKind::SameCurve);
    assert_eq!(pair_kind(&on_curve, &at_end), PairKind::CurveEnd);
    assert_eq!(pair_kind(&at_end, &on_curve), PairKind::CurveEnd);
}

/// Two points each recorded on a face the other is not: the ends of two
/// different curves, or a mesh vertex of a face the intersection point is not
/// on. No single statement of the paper names the pair.
#[test]
fn points_recorded_on_different_faces_are_their_own_kind() {
    use InputId::{A, B};
    let end_on_a = faces(&[(A, 0), (A, 1), (B, 4)]);
    let end_on_b = faces(&[(A, 0), (B, 4), (B, 5)]);
    assert_eq!(pair_kind(&end_on_a, &end_on_b), PairKind::DifferentFaces);
    let other_face = faces(&[(A, 2)]);
    assert_eq!(pair_kind(&other_face, &end_on_a), PairKind::DifferentFaces);
}

/// Two vertices of one operand's mesh, or a vertex with no recorded face,
/// fall under no statement.
#[test]
fn one_operand_pairs_and_bare_vertices_are_unread() {
    use InputId::A;
    let p = faces(&[(A, 0), (A, 1)]);
    assert_eq!(pair_kind(&p, &p), PairKind::Unread);
    assert_eq!(pair_kind(&p, &BTreeSet::new()), PairKind::Unread);
}

fn merge(
    mesh: &mut Mesh,
    labels: &mut Vec<TriangleSideLabel>,
    map: &[(u32, u32)],
    scoped: bool,
) -> usize {
    let mut attr: Vec<Option<TriangleAttribution>> = vec![of(InputId::A); mesh.tris.len()];
    let mut bounds = vec![0.0; mesh.tris.len()];
    let dropped = crate::stage4_correct::merge_vertex_classes_with_bounds(
        mesh,
        &mut attr,
        labels,
        &mut bounds,
        &map.iter().copied().collect(),
        scoped,
        None,
    );
    assert_eq!(attr.len(), mesh.tris.len());
    assert_eq!(bounds.len(), mesh.tris.len());
    assert_eq!(labels.len(), mesh.tris.len());
    dropped
}

fn far_verts(n: usize) -> Vec<Point3> {
    (0..n).map(|i| p(i as f64, (i * i) as f64, 0.0)).collect()
}

/// Every class is applied in one rewrite: a triangle that loses an edge is
/// dropped, the others take the survivors' indices.
#[test]
fn every_class_is_merged_in_one_rewrite() {
    // 1 -> 0 and 3 -> 2: [0,1,4] and [2,3,4] lose an edge, [1,3,4] becomes [0,2,4].
    let mut mesh = Mesh::new(far_verts(5), vec![[0, 1, 4], [2, 3, 4], [1, 3, 4]]);
    let mut labels = vec![TriangleSideLabel::default(); 3];
    let dropped = merge(&mut mesh, &mut labels, &[(1, 0), (3, 2)], false);
    assert_eq!(dropped, 2);
    assert_eq!(mesh.tris, vec![[0, 2, 4]]);
}

/// The membrane (spec `yang_collapse_membrane_cancellation`): two triangles
/// that the merge puts on one vertex triple with opposite windings are a
/// zero-volume flap, and both go. It is read once, over the whole result —
/// here the pair exists only after BOTH classes are merged, so no single
/// pair read in any order finds it.
#[test]
fn a_membrane_made_by_two_classes_together_is_cancelled() {
    // [0,1,2] and [3,5,4] with 3 -> 0 and 4 -> 1, 5 -> 2: the second becomes
    // [0,2,1], the first's mirror.
    let mut mesh = Mesh::new(far_verts(7), vec![[0, 1, 2], [3, 5, 4], [0, 1, 6]]);
    let mut labels = vec![TriangleSideLabel::default(); 3];
    let dropped = merge(&mut mesh, &mut labels, &[(3, 0), (4, 1), (5, 2)], false);
    assert_eq!(dropped, 2);
    assert_eq!(mesh.tris, vec![[0, 1, 6]]);

    // Same winding: a doubled triangle is not a membrane and stays.
    let mut mesh = Mesh::new(far_verts(6), vec![[0, 1, 2], [3, 4, 5]]);
    let mut labels = vec![TriangleSideLabel::default(); 2];
    let dropped = merge(&mut mesh, &mut labels, &[(3, 0), (4, 1), (5, 2)], false);
    assert_eq!(dropped, 0);
    assert_eq!(mesh.tris.len(), 2);
}

/// On the full arrangement a membrane is cancelled only within one side
/// label: a kept patch never cancels against a dropped one, nor across
/// operands (§4.4.2 `:572-578`).
#[test]
fn a_membrane_across_two_side_labels_is_not_cancelled() {
    let tris = vec![[0, 1, 2], [3, 5, 4]];
    let map = [(3, 0), (4, 1), (5, 2)];
    let other = TriangleSideLabel {
        cherchi_patch: 7,
        ..TriangleSideLabel::default()
    };

    let mut mesh = Mesh::new(far_verts(6), tris.clone());
    let mut labels = vec![TriangleSideLabel::default(), other];
    assert_eq!(merge(&mut mesh, &mut labels, &map, true), 0);
    assert_eq!(mesh.tris.len(), 2);
    assert_eq!(labels[1], other, "labels stay 1:1 with the triangles");

    // Unscoped (the kept submesh), the same pair is a membrane.
    let mut mesh = Mesh::new(far_verts(6), tris);
    let mut labels = vec![TriangleSideLabel::default(), other];
    assert_eq!(merge(&mut mesh, &mut labels, &map, false), 2);
    assert!(mesh.tris.is_empty());
}

/// §4.3.3 / Fig. 11(b) as one pass (`stage4_correct::merge_same_point_classes`):
/// the classes are merged onto their ranked survivors and formed again until
/// none is left; a mesh without a class is left alone.
#[test]
fn the_class_merge_runs_to_a_mesh_with_no_class() {
    let mut mesh = twin_mesh();
    // Vertex 1's star holds A and B (an intersection point); vertex 0 is A's.
    let mut attr = vec![of(InputId::A), of(InputId::A), of(InputId::B)];
    let mut labels = vec![TriangleSideLabel::default(); 3];
    let mut bounds = vec![0.0; 3];
    let merged = crate::stage4_correct::merge_same_point_classes(
        "test",
        &mut mesh,
        &mut attr,
        &mut labels,
        &mut bounds,
        None,
        &|_| 0,
        false,
        None,
    )
    .expect("merge");
    assert!(merged);
    // The twins are one vertex: the triangle between them is gone and the
    // intersection point (1) is the survivor.
    assert_eq!(mesh.tris, vec![[1, 2, 3], [1, 3, 4]]);
    assert_eq!((attr.len(), labels.len(), bounds.len()), (2, 2, 2));
    assert!(classes(&mesh, &attr).survivor_map().is_empty());
    let again = crate::stage4_correct::merge_same_point_classes(
        "test",
        &mut mesh,
        &mut attr,
        &mut labels,
        &mut bounds,
        None,
        &|_| 0,
        false,
        None,
    )
    .expect("merge");
    assert!(!again, "no class, no merge");
    assert_eq!(mesh.tris.len(), 2);
}
