//! Yang §4.5.3 (`refs/text/yang2025_hybrid_boolean.txt:679-689, 718-751`):
//! the intersection curve C between two surfaces, read from the mesh as the
//! chain of intersection edges two faces share
//! (`stage4_chains::IntersectionEdges`, spec
//! `specs/yang_f2_fig11b_cdt_input_merge.md` §8.8).
//!
//! The fixtures are combinatorial: the reading uses the triangles' face
//! attribution only, so every vertex may sit anywhere.

use super::m4_substitute::p;
use crate::stage4_chains::{
    edge_agreement, faces_on_edge, variant_name, Chain, IntersectionEdges, SiteKind,
};
use crate::*;
use std::collections::BTreeSet;

fn on(input: InputId, face: u32) -> Option<TriangleAttribution> {
    Some(TriangleAttribution { input, face })
}

fn mesh_of(n_verts: usize, tris: Vec<[u32; 3]>) -> Mesh {
    Mesh::new((0..n_verts).map(|i| p(i as f64, 0.0, 0.0)).collect(), tris)
}

/// A curve 0–1–2–3 between face 0 of A (triangles above it, apex 4/5/6) and
/// face 0 of B (triangles below it, apex 7/8/9).
fn open_curve() -> (Mesh, Vec<Option<TriangleAttribution>>) {
    let mesh = mesh_of(
        10,
        vec![
            [0, 1, 4],
            [1, 2, 5],
            [2, 3, 6],
            [1, 0, 7],
            [2, 1, 8],
            [3, 2, 9],
        ],
    );
    let attr = vec![
        on(InputId::A, 0),
        on(InputId::A, 0),
        on(InputId::A, 0),
        on(InputId::B, 0),
        on(InputId::B, 0),
        on(InputId::B, 0),
    ];
    (mesh, attr)
}

#[test]
fn an_edge_with_a_triangle_of_each_operand_is_an_intersection_edge() {
    let (mesh, attr) = open_curve();
    let edges = IntersectionEdges::read(&mesh, &attr);
    let keys: Vec<(u32, u32)> = edges.pairs.keys().copied().collect();
    assert_eq!(keys, vec![(0, 1), (1, 2), (2, 3)]);
    assert!(edges.is_intersection_edge(2, 1));
    // An edge between two triangles of one operand is not one.
    assert!(!edges.is_intersection_edge(1, 4));
    assert!(!edges.is_intersection_edge(0, 7));
}

#[test]
fn an_open_curve_is_one_chain_between_its_two_ends() {
    let (mesh, attr) = open_curve();
    let chains = IntersectionEdges::read(&mesh, &attr).chains();
    assert_eq!(
        chains,
        vec![Chain {
            faces: (0, 0),
            verts: vec![0, 1, 2, 3],
            closed: false,
        }]
    );
}

#[test]
fn a_closed_curve_is_one_ring_listing_each_vertex_once() {
    // The triangle 0–1–2 as a curve: A's triangles outside it, B's inside.
    let mesh = mesh_of(
        7,
        vec![
            [0, 1, 3],
            [1, 2, 4],
            [2, 0, 5],
            [1, 0, 6],
            [2, 1, 6],
            [0, 2, 6],
        ],
    );
    let attr = vec![
        on(InputId::A, 2),
        on(InputId::A, 2),
        on(InputId::A, 2),
        on(InputId::B, 5),
        on(InputId::B, 5),
        on(InputId::B, 5),
    ];
    let edges = IntersectionEdges::read(&mesh, &attr);
    assert_eq!(
        edges.chains(),
        vec![Chain {
            faces: (2, 5),
            verts: vec![0, 1, 2],
            closed: true,
        }]
    );
    // Every vertex of a ring is a point between endpoints, across the wrap too.
    assert_eq!(edges.site(2, 0, 1), SiteKind::Interior);
    assert_eq!(edges.site(1, 0, 2), SiteKind::Interior);
}

#[test]
fn a_curve_ends_where_the_face_of_either_operand_changes() {
    // 0–1–2 on (A0, B0); at vertex 2 A's face changes to A1: 2–3 on (A1, B0).
    let (mesh, mut attr) = open_curve();
    attr[2] = on(InputId::A, 1);
    let edges = IntersectionEdges::read(&mesh, &attr);
    assert_eq!(
        edges.chains(),
        vec![
            Chain {
                faces: (0, 0),
                verts: vec![0, 1, 2],
                closed: false,
            },
            Chain {
                faces: (1, 0),
                verts: vec![2, 3],
                closed: false,
            },
        ]
    );
    assert_eq!(edges.site(0, 1, 2), SiteKind::Interior);
    assert_eq!(edges.site(2, 1, 0), SiteKind::Interior);
    assert_eq!(edges.site(1, 2, 3), SiteKind::Straddle);
    // Off the curve: one edge of the walk is an intersection edge, or none.
    assert_eq!(edges.site(0, 1, 4), SiteKind::OneEdge);
    assert_eq!(edges.site(4, 1, 5), SiteKind::Off);
}

#[test]
fn an_edge_of_one_operand_lying_in_a_face_of_the_other_is_on_two_curves() {
    // Edge 1–2 has triangles of A0 AND A1 on it (an edge of A) and of B0.
    let (mut mesh, mut attr) = open_curve();
    mesh.tris.push([2, 1, 4]);
    attr.push(on(InputId::A, 1));
    let edges = IntersectionEdges::read(&mesh, &attr);
    let on_12: Vec<(u32, u32)> = edges.pairs[&(1, 2)].iter().copied().collect();
    assert_eq!(on_12, vec![(0, 0), (1, 0)]);
    let chains = edges.chains();
    assert!(chains.contains(&Chain {
        faces: (0, 0),
        verts: vec![0, 1, 2, 3],
        closed: false,
    }));
    assert!(chains.contains(&Chain {
        faces: (1, 0),
        verts: vec![1, 2],
        closed: false,
    }));
    // The curve (A0, B0) runs straight through vertex 1.
    assert_eq!(edges.site(0, 1, 2), SiteKind::Interior);
}

#[test]
fn three_edges_of_one_curve_at_a_vertex_end_its_chains_there() {
    // A second branch 1–4 of the same face pair leaves vertex 1.
    let mesh = mesh_of(
        9,
        vec![
            [0, 1, 5],
            [1, 2, 6],
            [1, 4, 7],
            [1, 0, 8],
            [2, 1, 8],
            [4, 1, 8],
        ],
    );
    let attr = vec![
        on(InputId::A, 0),
        on(InputId::A, 0),
        on(InputId::A, 0),
        on(InputId::B, 0),
        on(InputId::B, 0),
        on(InputId::B, 0),
    ];
    let edges = IntersectionEdges::read(&mesh, &attr);
    let chains = edges.chains();
    assert_eq!(chains.len(), 3);
    assert!(chains.iter().all(|c| !c.closed && c.verts.len() == 2));
    assert_eq!(edges.site(0, 1, 2), SiteKind::Branch);
}

#[test]
fn a_triangle_with_no_face_takes_no_part() {
    let (mesh, mut attr) = open_curve();
    attr[4] = None; // B's triangle on edge 1–2
    let edges = IntersectionEdges::read(&mesh, &attr);
    let keys: Vec<(u32, u32)> = edges.pairs.keys().copied().collect();
    assert_eq!(keys, vec![(0, 1), (2, 3)]);
    assert_eq!(edges.chains().len(), 2);
}

#[test]
fn the_census_counts_the_edges_only_one_reading_holds() {
    let (mesh, attr) = open_curve();
    let edges = IntersectionEdges::read(&mesh, &attr);
    let loops: BTreeSet<(u32, u32)> = [(0, 1), (1, 2), (4, 5)].into_iter().collect();
    let agree = edge_agreement(&edges, &loops);
    assert_eq!(
        (
            agree.loop_cross,
            agree.chain_edges,
            agree.only_loop,
            agree.only_chain
        ),
        (3, 3, 1, 1)
    );
}

#[test]
fn the_census_detail_names_each_triangle_on_an_edge_and_a_variant() {
    let (mesh, attr) = open_curve();
    assert_eq!(faces_on_edge(&mesh, &attr, (1, 2)), vec!["A0", "B0"]);
    assert_eq!(faces_on_edge(&mesh, &attr, (1, 4)), vec!["A0"]);
    assert_eq!(variant_name(&SiteKind::Straddle), "Straddle");
    assert_eq!(variant_name(&on(InputId::B, 3)), "Some");
}
