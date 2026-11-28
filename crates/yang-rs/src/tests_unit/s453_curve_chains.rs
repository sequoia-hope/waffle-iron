//! Yang §4.5.3 (`refs/text/yang2025_hybrid_boolean.txt:679-689, 718-751`,
//! Fig. 15): the sweep reads each curve C as a chain of patch-boundary edges
//! (`stage4_correct::curve_chains`) and walks no patch boundary as a loop, so
//! a patch whose boundary is not a loop does not stop the correction.

use super::i13_junction_overrun::planes_carrier_brep;
use super::m4_substitute::p;
use crate::stage4_chains::{chains_by, mesh_identity};
use crate::*;
use std::collections::{BTreeMap, HashSet};

fn plane(normal: [f64; 3]) -> Surface {
    Surface::Plane {
        normal: Vector3::new(normal[0], normal[1], normal[2]),
        d: 0.0,
    }
}

fn on(input: InputId, face: u32) -> Option<TriangleAttribution> {
    Some(TriangleAttribution { input, face })
}

/// A's face `z = 0` and B's face `x = 0` cross along the y axis at vertices
/// 0–3 (at the heights `ys`). A's patch is a fan from vertex 4, B's a fan
/// from vertex 5. With `fold`, A holds a second triangle on the same side of
/// edge 1–2 (apex 6), so its boundary is not a loop.
fn crossing(ys: [f64; 4], fold: bool) -> (Mesh, Vec<Option<TriangleAttribution>>, BRep, BRep) {
    let mut verts: Vec<Point3> = ys.iter().map(|&y| p(0.0, y, 0.0)).collect();
    verts.extend([p(1.0, 1.5, 0.0), p(0.0, 1.5, 1.0), p(-1.0, 1.5, 0.0)]);
    let mut tris = vec![[0, 1, 4], [1, 2, 4], [2, 3, 4]];
    let mut attr = vec![on(InputId::A, 0); 3];
    if fold {
        tris.push([1, 2, 6]);
        attr.push(on(InputId::A, 0));
    }
    tris.extend([[1, 0, 5], [2, 1, 5], [3, 2, 5]]);
    attr.extend([on(InputId::B, 0); 3]);
    (
        Mesh::new(verts, tris),
        attr,
        planes_carrier_brep(&[plane([0.0, 0.0, 1.0])]),
        planes_carrier_brep(&[plane([1.0, 0.0, 0.0])]),
    )
}

fn read(mesh: &Mesh, attr: &[Option<TriangleAttribution>], a: &BRep, b: &BRep) -> CurveChains {
    curve_chains(
        mesh,
        &TriangleAttributionMap {
            attributions: attr.to_vec(),
        },
        None,
        a,
        b,
    )
    .expect("curve_chains")
}

#[test]
fn a_curve_is_one_chain_from_end_to_end() {
    let (mesh, attr, a, b) = crossing([0.0, 1.0, 2.0, 3.0], false);
    let got = read(&mesh, &attr, &a, &b);
    assert!(
        got.chains.contains(&(vec![0, 1, 2, 3], false)),
        "the edges both patches are bounded by are one open chain: {:?}",
        got.chains
    );
    // Each operand's own boundary beside the curve is read too (the overtake
    // arms test it), as its own chain between the curve's two ends.
    assert!(got.chains.contains(&(vec![0, 4, 3], false)));
    assert!(got.chains.contains(&(vec![0, 5, 3], false)));
    assert_eq!(got.chains.len(), 3);
    assert_eq!(
        got.incidence[&(1, 2)],
        vec![
            (InputId::A, plane([0.0, 0.0, 1.0])),
            (InputId::B, plane([1.0, 0.0, 0.0]))
        ]
    );
    assert_eq!(got.incidence[&(0, 4)].len(), 1);
    // An edge between two triangles of one patch bounds nothing.
    assert!(!got.incidence.contains_key(&(1, 4)));
    let typed: Vec<(u32, u32)> = got.curves.keys().copied().collect();
    assert_eq!(typed, vec![(0, 1), (1, 2), (2, 3)]);
}

#[test]
fn a_patch_whose_boundary_is_not_a_loop_is_read_like_any_other() {
    let (mesh, attr, a, b) = crossing([0.0, 1.0, 2.0, 3.0], true);
    // The boundary walk of A's patch stops: edge 1–2 holds two of its
    // triangles on one side.
    let map = TriangleAttributionMap {
        attributions: attr.clone(),
    };
    assert!(
        compute_phase_a(&mesh, &map, &a, &b, &crate::stage3_ssi::NO_EDGE_PROVENANCE).is_err(),
        "the fixture must be one the patch boundary walk refuses"
    );
    let got = read(&mesh, &attr, &a, &b);
    assert!(
        got.chains.contains(&(vec![0, 1, 2, 3], false)),
        "the curve is still one chain: {:?}",
        got.chains
    );
    // The fold's two triangles do not cancel along 1–2: it stays a boundary
    // edge of A's patch, and of B's.
    assert_eq!(got.incidence[&(1, 2)].len(), 2);
    assert!(got.curves.contains_key(&(1, 2)));
}

#[test]
fn the_sweep_corrects_a_reversal_on_a_patch_whose_boundary_is_not_a_loop() {
    // Points 1 and 2 are out of order along the curve (heights 2 and 1).
    let (mut mesh, mut attr, a, b) = crossing([0.0, 2.0, 1.0, 3.0], true);
    let entry_pos: Vec<[f64; 3]> = mesh.verts.iter().map(|v| v.as_array()).collect();
    let mut side_labels = vec![TriangleSideLabel::default(); mesh.tris.len()];
    let mut tri_bounds = vec![1.0f64; mesh.tris.len()];
    let swept = sweep_reversed_intersections(
        &mut mesh,
        &mut attr,
        &mut side_labels,
        &mut tri_bounds,
        &a,
        &b,
        1.0,
        &HashSet::new(),
        &entry_pos,
        &BTreeMap::new(),
        false,
        None,
    )
    .expect("the sweep must not stop on the folded patch");
    assert!(swept, "the reversal at point 1 is corrected");
    assert_eq!(tri_bounds.len(), mesh.tris.len());
    // The paper's order: the NEXT point (2) is removed, point 3 resolves it.
    assert!(mesh.tris.iter().all(|t| !t.contains(&2)));
    assert!(mesh.tris.iter().any(|t| t.contains(&1)));
    let after = read(&mesh, &attr, &a, &b);
    assert!(after.chains.contains(&(vec![0, 1, 3], false)));
}

#[test]
fn a_run_that_returns_to_its_start_is_a_closed_ring() {
    // Two rings 0–1–2 and 0–3–4 of one curve meet at vertex 0; A's patch is a
    // fan from 5 over both, B's a fan from 6.
    let verts = vec![
        p(0.0, 0.0, 0.0),
        p(0.0, 1.0, 0.0),
        p(0.0, 2.0, 0.0),
        p(0.0, -1.0, 0.0),
        p(0.0, -2.0, 0.0),
        p(1.0, 0.0, 0.0),
        p(0.0, 0.0, 1.0),
    ];
    let ring = [(0u32, 1u32), (1, 2), (2, 0), (0, 3), (3, 4), (4, 0)];
    let mut tris = Vec::new();
    let mut attr = Vec::new();
    for &(u, v) in &ring {
        tris.push([u, v, 5]);
        attr.push(on(InputId::A, 0));
    }
    for &(u, v) in &ring {
        tris.push([v, u, 6]);
        attr.push(on(InputId::B, 0));
    }
    let mesh = Mesh::new(verts, tris);
    let got = read(
        &mesh,
        &attr,
        &planes_carrier_brep(&[plane([0.0, 0.0, 1.0])]),
        &planes_carrier_brep(&[plane([1.0, 0.0, 0.0])]),
    );
    assert_eq!(
        got.chains,
        vec![(vec![0, 1, 2], true), (vec![0, 3, 4], true)],
        "each lobe leaves vertex 0 and comes back to it"
    );
}

#[test]
fn chains_run_between_the_vertices_where_their_key_branches_or_ends() {
    // Key 'a': a path 0–1–2–3. Key 'b': a triangle 4–5–6 (a ring).
    let got = chains_by([
        ((0, 1), 'a'),
        ((1, 2), 'a'),
        ((2, 3), 'a'),
        ((4, 5), 'b'),
        ((5, 6), 'b'),
        ((4, 6), 'b'),
    ]);
    assert_eq!(
        got,
        vec![('a', vec![0, 1, 2, 3], false), ('b', vec![4, 5, 6], true)]
    );
    // A vertex with three edges of one key ends every chain through it.
    let star = chains_by([((0, 9), 'k'), ((1, 9), 'k'), ((2, 9), 'k')]);
    assert_eq!(star.len(), 3);
    assert!(star
        .iter()
        .all(|(_, verts, closed)| !closed && verts.len() == 2));
}

#[test]
fn a_mesh_identity_follows_its_positions_and_its_triangles() {
    let (mesh, _, _, _) = crossing([0.0, 1.0, 2.0, 3.0], false);
    let (same, _, _, _) = crossing([0.0, 1.0, 2.0, 3.0], false);
    let (moved, _, _, _) = crossing([0.0, 1.0, 2.5, 3.0], false);
    let (folded, _, _, _) = crossing([0.0, 1.0, 2.0, 3.0], true);
    assert_eq!(mesh_identity(&mesh), mesh_identity(&same));
    assert_ne!(mesh_identity(&mesh), mesh_identity(&moved));
    assert_ne!(mesh_identity(&mesh), mesh_identity(&folded));
}

#[test]
fn an_edge_of_the_full_arrangement_names_its_surface_pair_once() {
    let (pz, px, py) = (
        plane([0.0, 0.0, 1.0]),
        plane([1.0, 0.0, 0.0]),
        plane([0.0, 1.0, 0.0]),
    );
    // Four sub-patches along a transverse curve: two of each surface.
    assert_eq!(
        reduce_overlap_incidence(vec![
            (InputId::A, pz, false),
            (InputId::A, pz, false),
            (InputId::B, px, false),
            (InputId::B, px, false),
        ]),
        vec![(InputId::A, pz), (InputId::B, px)]
    );
    // A common-surface sheet already carried by a pure sheet of the same
    // surface adds nothing.
    assert_eq!(
        reduce_overlap_incidence(vec![
            (InputId::A, pz, false),
            (InputId::B, px, false),
            (InputId::B, pz, true),
        ]),
        vec![(InputId::A, pz), (InputId::B, px)]
    );
    // Both operands leave the common surface onto coincident walls: the
    // sheet's operand names the common surface instead of its wall.
    assert_eq!(
        reduce_overlap_incidence(vec![
            (InputId::A, px, false),
            (InputId::B, px, false),
            (InputId::B, pz, true),
        ]),
        vec![(InputId::A, px), (InputId::B, pz)]
    );
    // Two walls that cross: the sheet is not a third surface of the edge.
    assert_eq!(
        reduce_overlap_incidence(vec![
            (InputId::A, px, false),
            (InputId::B, py, false),
            (InputId::B, pz, true),
        ]),
        vec![(InputId::A, px), (InputId::B, py)]
    );
    // A sheet beside one pure sheet is the edge's second surface.
    assert_eq!(
        reduce_overlap_incidence(vec![(InputId::A, px, false), (InputId::B, pz, true)]),
        vec![(InputId::A, px), (InputId::B, pz)]
    );
}
