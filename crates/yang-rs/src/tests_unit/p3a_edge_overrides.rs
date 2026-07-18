//! P3a #146 increment-1b unit fixtures: `stage1_tessellate_with_edge_overrides`
//! contract (spec `specs/yang_146_conformal_junction_sampling.md` §3.3/§4) —
//! junction pierce points inserted into `LineSegment` edge polylines as shared
//! Steiner vertices, per-loop-copy fan-out honored by identity.
//!
//! Fixtures use the axis-aligned box builder [`rj_box`] (per-loop-copy
//! `LineSegment` edges, 6 planar faces) — the same B-Rep shape the F0082
//! lead customer presents. In `rj_box`, geometric edge {v0=(lo,lo,lo),
//! v1=(hi,lo,lo)} has TWO directed copies: edge 0 (`0→1`, bottom face) and
//! edge 20 (`1→0`, y=lo side face).

use super::n2_junction::rj_box;
use crate::*;
use std::collections::BTreeMap;

fn box_parts() -> (Vec<BRepVertex>, Vec<BRepEdge>, Vec<BRepFace>) {
    let b = rj_box([0.0, 0.0, 0.0], [1.0, 1.0, 1.0]);
    (
        b.vertices().to_vec(),
        b.edges().to_vec(),
        b.faces().to_vec(),
    )
}

fn bits(p: Point3) -> [u64; 3] {
    [p.x().to_bits(), p.y().to_bits(), p.z().to_bits()]
}

/// Every undirected edge shared by exactly two triangles AND every directed
/// edge used exactly once — closed 2-manifold with consistent winding.
fn closed_conformal_2_manifold(tris: &[[u32; 3]]) -> bool {
    let mut undirected: BTreeMap<(u32, u32), u32> = BTreeMap::new();
    let mut directed: BTreeMap<(u32, u32), u32> = BTreeMap::new();
    for tri in tris {
        for k in 0..3 {
            let (a, b) = (tri[k], tri[(k + 1) % 3]);
            *undirected.entry((a.min(b), a.max(b))).or_insert(0) += 1;
            *directed.entry((a, b)).or_insert(0) += 1;
        }
    }
    !undirected.is_empty()
        && undirected.values().all(|&c| c == 2)
        && directed.values().all(|&c| c == 1)
}

/// An EMPTY edge-override map is byte-identical to plain
/// [`stage1_tessellate`] — verts, tris, sources, chains (spec §5 oracle).
#[test]
fn edge_override_empty_is_byte_identical() {
    let (verts, edges, faces) = box_parts();
    let plain = stage1_tessellate(&verts, &edges, &faces).expect("plain");
    let empty: BTreeMap<u32, Vec<Point3>> = BTreeMap::new();
    let t = stage1_tessellate_with_edge_overrides(&verts, &edges, &faces, &empty, None)
        .expect("empty overrides");
    assert_eq!(plain.verts.len(), t.verts.len());
    for (a, b) in plain.verts.iter().zip(&t.verts) {
        assert_eq!(bits(*a), bits(*b), "verts must be byte-identical");
    }
    assert_eq!(plain.tris, t.tris, "tris must be byte-identical");
    assert_eq!(plain.sources, t.sources, "sources must be byte-identical");
    assert_eq!(plain.chains, t.chains, "chains must be byte-identical");
}

/// A mid-edge junction point fanned to BOTH per-loop copies mints exactly ONE
/// Steiner vertex; both copies' chains splice it (opposite orientation), both
/// incident faces reference it, and the box stays a closed, consistently
/// wound 2-manifold — conformality by identity.
#[test]
fn edge_override_mints_once_and_keeps_conformality() {
    let (verts, edges, faces) = box_parts();
    let plain = stage1_tessellate(&verts, &edges, &faces).expect("plain");
    let j = Point3::new(0.4, 0.0, 0.0);
    let mut ov: BTreeMap<u32, Vec<Point3>> = BTreeMap::new();
    ov.insert(0, vec![j]);
    ov.insert(20, vec![j]);
    let t = stage1_tessellate_with_edge_overrides(&verts, &edges, &faces, &ov, None)
        .expect("mid-edge override");

    // Exactly one new vertex, bit-exactly the junction point.
    assert_eq!(t.verts.len(), plain.verts.len() + 1, "one shared mint");
    let minted: Vec<u32> = (0..t.verts.len() as u32)
        .filter(|&i| bits(t.verts[i as usize]) == bits(j))
        .collect();
    assert_eq!(minted.len(), 1, "junction point minted exactly once");
    let jv = minted[0];
    assert_eq!(
        t.sources[jv as usize],
        TessellationSource::BRepEdge { edge: 0, t: 0.4 },
        "Steiner source = canonical copy + chord parameter"
    );

    // Both copies carry the chain, opposite orientation, SAME vertex index.
    assert_eq!(t.chains[&0], vec![0, jv, 1], "copy 0 chain start→J→end");
    assert_eq!(t.chains[&20], vec![1, jv, 0], "copy 20 chain reversed");

    // Both incident faces (0 = bottom, 5 = y-lo side) reference the mint.
    for f_idx in [0usize, 5usize] {
        let uses = t.face_tri_ranges[f_idx]
            .clone()
            .any(|ti| t.tris[ti].contains(&jv));
        assert!(uses, "face {f_idx} must consume the junction vertex");
    }
    assert!(
        closed_conformal_2_manifold(&t.tris),
        "box must stay a closed, consistently wound 2-manifold"
    );
}

/// Two junction points insert in chord-parameter order regardless of the
/// list order handed in; the chain runs start → t=0.3 → t=0.6 → end.
#[test]
fn edge_override_two_points_sorted_by_parameter() {
    let (verts, edges, faces) = box_parts();
    let plain = stage1_tessellate(&verts, &edges, &faces).expect("plain");
    let j_a = Point3::new(0.6, 0.0, 0.0);
    let j_b = Point3::new(0.3, 0.0, 0.0);
    let mut ov: BTreeMap<u32, Vec<Point3>> = BTreeMap::new();
    ov.insert(0, vec![j_a, j_b]);
    ov.insert(20, vec![j_a, j_b]);
    let t = stage1_tessellate_with_edge_overrides(&verts, &edges, &faces, &ov, None)
        .expect("two overrides");
    assert_eq!(t.verts.len(), plain.verts.len() + 2);
    let chain = &t.chains[&0];
    assert_eq!(chain.len(), 4);
    assert_eq!((chain[0], chain[3]), (0, 1));
    assert_eq!(bits(t.verts[chain[1] as usize]), bits(j_b), "t=0.3 first");
    assert_eq!(bits(t.verts[chain[2] as usize]), bits(j_a), "t=0.6 second");
    assert_eq!(
        t.chains[&20],
        vec![1, chain[2], chain[1], 0],
        "reversed copy splices the same Steiner verts"
    );
    assert!(closed_conformal_2_manifold(&t.tris));
}

/// Fan-out contract: targeting ONE copy of a geometric edge while a sibling
/// copy has no list is a loud STOP — a silent single-sided insertion is the
/// exact conformality break this machinery exists to prevent.
#[test]
fn edge_override_missing_copy_is_loud() {
    let (verts, edges, faces) = box_parts();
    let mut ov: BTreeMap<u32, Vec<Point3>> = BTreeMap::new();
    ov.insert(0, vec![Point3::new(0.4, 0.0, 0.0)]);
    let Err(YangError::MalformedTopology(msg)) =
        stage1_tessellate_with_edge_overrides(&verts, &edges, &faces, &ov, None)
    else {
        panic!("missing sibling copy must be loud");
    };
    assert!(msg.contains("fan-out"), "unexpected message: {msg}");
}

/// Fan-out contract: copies carrying DIFFERENT lists is a loud STOP.
#[test]
fn edge_override_mismatched_copies_is_loud() {
    let (verts, edges, faces) = box_parts();
    let mut ov: BTreeMap<u32, Vec<Point3>> = BTreeMap::new();
    ov.insert(0, vec![Point3::new(0.4, 0.0, 0.0)]);
    ov.insert(20, vec![Point3::new(0.5, 0.0, 0.0)]);
    let Err(YangError::MalformedTopology(msg)) =
        stage1_tessellate_with_edge_overrides(&verts, &edges, &faces, &ov, None)
    else {
        panic!("mismatched copies must be loud");
    };
    assert!(
        msg.contains("different override lists"),
        "unexpected message: {msg}"
    );
}

/// An override off the edge's line (beyond the on-curve band) is a loud STOP.
#[test]
fn edge_override_off_line_is_loud() {
    let (verts, edges, faces) = box_parts();
    let bad = Point3::new(0.4, 0.1, 0.0);
    let mut ov: BTreeMap<u32, Vec<Point3>> = BTreeMap::new();
    ov.insert(0, vec![bad]);
    ov.insert(20, vec![bad]);
    let Err(YangError::MalformedTopology(msg)) =
        stage1_tessellate_with_edge_overrides(&verts, &edges, &faces, &ov, None)
    else {
        panic!("off-line override must be loud");
    };
    assert!(msg.contains("off the edge's line"), "unexpected: {msg}");
}

/// An on-line override outside the edge span `t ∈ (0, 1)` is a loud STOP.
#[test]
fn edge_override_outside_span_is_loud() {
    let (verts, edges, faces) = box_parts();
    let bad = Point3::new(1.5, 0.0, 0.0);
    let mut ov: BTreeMap<u32, Vec<Point3>> = BTreeMap::new();
    ov.insert(0, vec![bad]);
    ov.insert(20, vec![bad]);
    let Err(YangError::MalformedTopology(msg)) =
        stage1_tessellate_with_edge_overrides(&verts, &edges, &faces, &ov, None)
    else {
        panic!("outside-span override must be loud");
    };
    assert!(msg.contains("outside the edge span"), "unexpected: {msg}");
}

/// A sub-TAU_MODEL near-endpoint override that differs in bits is a loud
/// STOP (B-Rep vertices are authoritative; a near-corner pierce is P3b
/// territory, never a mid-edge sample).
#[test]
fn edge_override_near_endpoint_differing_bits_is_loud() {
    let (verts, edges, faces) = box_parts();
    let graze = Point3::new(1e-9, 0.0, 0.0);
    let mut ov: BTreeMap<u32, Vec<Point3>> = BTreeMap::new();
    ov.insert(0, vec![graze]);
    ov.insert(20, vec![graze]);
    let Err(YangError::MalformedTopology(msg)) =
        stage1_tessellate_with_edge_overrides(&verts, &edges, &faces, &ov, None)
    else {
        panic!("near-endpoint graze must be loud");
    };
    assert!(
        msg.contains("coincides with an endpoint"),
        "unexpected: {msg}"
    );
}

/// A bit-identical endpoint repeat deduplicates: the result is byte-identical
/// to the plain tessellation (the endpoint is already in the polyline).
#[test]
fn edge_override_endpoint_bit_identical_repeat_dedups() {
    let (verts, edges, faces) = box_parts();
    let plain = stage1_tessellate(&verts, &edges, &faces).expect("plain");
    let endpoint = Point3::new(0.0, 0.0, 0.0);
    let mut ov: BTreeMap<u32, Vec<Point3>> = BTreeMap::new();
    ov.insert(0, vec![endpoint]);
    ov.insert(20, vec![endpoint]);
    let t = stage1_tessellate_with_edge_overrides(&verts, &edges, &faces, &ov, None)
        .expect("endpoint repeat dedups");
    assert_eq!(plain.verts.len(), t.verts.len());
    assert_eq!(plain.tris, t.tris);
}

/// A bit-identical duplicate point in the list deduplicates to one mint.
#[test]
fn edge_override_duplicate_point_dedups() {
    let (verts, edges, faces) = box_parts();
    let plain = stage1_tessellate(&verts, &edges, &faces).expect("plain");
    let j = Point3::new(0.4, 0.0, 0.0);
    let mut ov: BTreeMap<u32, Vec<Point3>> = BTreeMap::new();
    ov.insert(0, vec![j, j]);
    ov.insert(20, vec![j, j]);
    let t = stage1_tessellate_with_edge_overrides(&verts, &edges, &faces, &ov, None)
        .expect("duplicate dedups");
    assert_eq!(t.verts.len(), plain.verts.len() + 1, "one mint, not two");
    assert!(closed_conformal_2_manifold(&t.tris));
}

/// Increment-1b scope guard: an overridden line edge incident to a
/// NON-PLANAR face is a loud STOP (that face's tessellator would not splice
/// the chain — silently dropping one side is the defect class itself).
#[test]
fn edge_override_on_nonplanar_incident_face_is_loud() {
    let (verts, edges, mut faces) = box_parts();
    // Reface the bottom (face 0, edges 0..4) as a fake cylinder lateral: the
    // guard must fire BEFORE any tessellation of it is attempted.
    faces[0].surface = Surface::Cylinder {
        axis_point: Point3::new(0.0, 0.0, 0.0),
        axis_dir: Vector3::new(0.0, 0.0, 1.0),
        radius: 1.0,
    };
    let j = Point3::new(0.4, 0.0, 0.0);
    let mut ov: BTreeMap<u32, Vec<Point3>> = BTreeMap::new();
    ov.insert(0, vec![j]);
    ov.insert(20, vec![j]);
    let Err(YangError::MalformedTopology(msg)) =
        stage1_tessellate_with_edge_overrides(&verts, &edges, &faces, &ov, None)
    else {
        panic!("non-planar incidence must be loud");
    };
    assert!(msg.contains("non-planar"), "unexpected: {msg}");
}

/// Targeting a curved (non-LineSegment) edge through the EDGE override map is
/// a loud STOP — curved-edge junction points go through `rim_overrides`.
#[test]
fn edge_override_on_curved_edge_is_loud() {
    let (verts, edges, faces) = super::boolean_functional::rt_cylinder(0.0, 1.0, 0.5);
    let mut ov: BTreeMap<u32, Vec<Point3>> = BTreeMap::new();
    ov.insert(0, vec![Point3::new(0.5, 0.0, 0.0)]); // edge 0 = bottom rim circle
    let Err(YangError::MalformedTopology(msg)) =
        stage1_tessellate_with_edge_overrides(&verts, &edges, &faces, &ov, None)
    else {
        panic!("curved-edge target must be loud");
    };
    assert!(msg.contains("non-LineSegment"), "unexpected: {msg}");
}
