//! Yang §4.4.1 Fig. 11(b) (`refs/text/yang2025_hybrid_boolean.txt:562-563`):
//! the merge classes of the update's CDT input
//! (`stage4_fig11b::fig11b_vertex_classes`, spec
//! `specs/yang_f2_fig11b_cdt_input_merge.md` §3).
//!
//! The twin fixture is the F0082 Extrude-12 magnitude: one junction minted
//! twice, 5.5e-14 apart at coordinate scale ~2, against a band of
//! `TAU_WORK·(1 + 2.09…)` ≈ 3.1e-12.

use super::m4_substitute::p;
use crate::stage4_fig11b::{fig11b_vertex_classes, Fig11bClasses};
use crate::*;
use cad_primitives::TAU_WORK;

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
