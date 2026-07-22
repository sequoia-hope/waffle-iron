//! #194 (spec `yang_194_subtauwork_edge_collapse`): sub-TAU_WORK mesh-edge
//! collapse at emission — the F0082 Extrude-12 operand-self-graze twin
//! class (same junction minted twice with swapped LPI roles, 5.5e-14
//! apart, edge-connected, zero-area flap → χ=3 book edge).

use super::m4_substitute::p;
use crate::stage4_correct::collapse_subtauwork_mesh_edges;
use crate::*;

/// The F0082 shape: an edge-connected twin pair below TAU_WORK·(1+scale)
/// with a zero-area flap over the twin edge. The pair collapses to the
/// min-index survivor (own bits), the flap drops, and the healthy
/// neighbors survive with the twin remapped.
#[test]
fn subtauwork_twin_edge_collapses_and_flap_drops() {
    // Twin at the F0082 magnitude: 5.5e-14 at coordinate scale ~2. Band =
    // TAU_WORK·(1+2.09…) ≈ 3.1e-12 ≫ 5.5e-14.
    let a = p(0.309456248416426, 0.092020830714880, 2.094303729583326);
    let twin = p(
        0.309456248416426 + 5.5e-14,
        0.092020830714880,
        2.094303729583326,
    );
    let far1 = p(1.0, 0.0, 2.0);
    let far2 = p(0.0, 1.0, 2.0);
    let far3 = p(-1.0, 0.0, 2.0);
    let mut mesh = Mesh::new(
        vec![a, twin, far1, far2, far3],
        vec![
            [0, 1, 2], // zero-area flap over the twin edge
            [0, 2, 3], // healthy neighbor using the survivor
            [1, 3, 4], // healthy neighbor using the twin (remaps)
        ],
    );
    let mut attr = vec![None; 3];
    assert!(collapse_subtauwork_mesh_edges(&mut mesh, &mut attr));
    assert_eq!(
        mesh.tris,
        vec![[0, 2, 3], [0, 3, 4]],
        "flap dropped; twin remapped onto the min-index survivor"
    );
    assert_eq!(
        mesh.verts[0], a,
        "I1: the survivor keeps its own exact coordinates"
    );
    assert_eq!(attr.len(), 2, "attribution stays in lockstep with tris");
}

/// Above-band edges are untouched. 1e-9 at unit scale is ≥ 500× the band
/// (TAU_WORK·(1+scale) ≈ 3e-12): a mutation widening the band toward
/// TAU_MODEL must fail here (1e-9 < 1e-7).
#[test]
fn supratauwork_edge_untouched() {
    let mut mesh = Mesh::new(
        vec![
            p(0.0, 0.0, 0.0),
            p(1.0e-9, 0.0, 1.0),
            p(1.0, 0.0, 0.5),
            p(0.0, 1.0, 0.5),
        ],
        vec![[0, 1, 3], [1, 2, 3]],
    );
    // Make (0,1) an actual mesh edge pair 1e-9 apart in x only.
    mesh.verts[1] = p(1.0e-9, 0.0, 0.0);
    let mut attr = vec![None; 2];
    assert!(!collapse_subtauwork_mesh_edges(&mut mesh, &mut attr));
    assert_eq!(mesh.tris, vec![[0, 1, 3], [1, 2, 3]], "mesh untouched");
}

/// Exact-zero edges are the M-B emission-identification class — untouched
/// (open interval at 0, the KV15b B3 rule).
#[test]
fn exact_zero_edge_untouched() {
    let q = p(0.25, 0.25, 0.25);
    let mut mesh = Mesh::new(
        vec![q, q, p(1.0, 0.0, 0.0), p(0.0, 1.0, 0.0)],
        vec![[0, 1, 3], [1, 2, 3]],
    );
    let mut attr = vec![None; 2];
    assert!(!collapse_subtauwork_mesh_edges(&mut mesh, &mut attr));
    assert_eq!(
        mesh.tris,
        vec![[0, 1, 3], [1, 2, 3]],
        "exact-zero pair stays"
    );
}

/// KV9 guard: UNCONNECTED coincident-scale verts (no mesh edge joins them)
/// are structurally out of the pass's domain — the ring-duplicate record.
#[test]
fn unconnected_subtauwork_pair_untouched() {
    let a = p(0.5, 0.5, 0.5);
    let b = p(0.5 + 5.5e-14, 0.5, 0.5);
    // Two SEPARATE triangles sharing no edge between verts 0 and 1.
    let mut mesh = Mesh::new(
        vec![
            a,
            b,
            p(1.0, 0.0, 0.0),
            p(1.0, 1.0, 0.0),
            p(0.0, 1.0, 1.0),
            p(0.0, 0.0, 1.0),
        ],
        vec![[0, 2, 3], [1, 4, 5]],
    );
    let mut attr = vec![None; 2];
    assert!(!collapse_subtauwork_mesh_edges(&mut mesh, &mut attr));
    assert_eq!(
        mesh.verts.len(),
        6,
        "unconnected near-coincident pair untouched (KV9 record)"
    );
    assert_eq!(mesh.tris, vec![[0, 2, 3], [1, 4, 5]]);
}
