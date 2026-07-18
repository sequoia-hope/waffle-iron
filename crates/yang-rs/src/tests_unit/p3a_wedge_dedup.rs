//! #146 increment-3a unit fixtures: the collapsed-wedge classifier
//! (`wedge_reject_reason`, spec `specs/yang_146_collapsed_wedge_dedup.md`
//! §2/§5) driven directly on the F0016-shaped configuration and each named
//! reject arm.

use crate::boolean::wedge_reject_reason;
use crate::*;

/// A minimal per-parent-tri B-Rep face map: parents 0 and 1 tessellate the
/// SAME face (7); parent 2 belongs to a DIFFERENT face (8) — the genuine
/// independent-coincident-sheet configuration.
const TRI_FACE: [u32; 3] = [7, 7, 8];

/// The F0016-shaped wedge, index-shrunk: raw tris [9,4,1] / [3,1,9] share
/// raw edge {1,9}; tips 4 and 3 weld to the same root 3; welded post-flip
/// triples (9,3,1) and (3,1,9) are cyclically equal (same winding). Parents
/// (B,0)/(B,1) are distinct tris of the same B-Rep face.
struct Wedge {
    raw_first: [u32; 3],
    raw_cur: [u32; 3],
    welded_first: [u32; 3],
    welded_cur: [u32; 3],
    weld: Vec<u32>,
    src_first: Vec<(LaInputId, u32)>,
    src_cur: Vec<(LaInputId, u32)>,
    surface: Vec<LaInputId>,
}

fn f0016_shape() -> Wedge {
    // weld: identity on 0..10 except 4 → 3 (the fused tip cluster).
    let mut weld: Vec<u32> = (0..10).collect();
    weld[4] = 3;
    Wedge {
        raw_first: [9, 4, 1],
        raw_cur: [3, 1, 9],
        welded_first: [9, 3, 1],
        welded_cur: [3, 1, 9],
        weld,
        src_first: vec![(LaInputId(1), 0)],
        src_cur: vec![(LaInputId(1), 1)],
        surface: vec![LaInputId(1)],
    }
}

fn classify(w: &Wedge) -> Option<&'static str> {
    wedge_reject_reason(
        w.raw_first,
        w.raw_cur,
        w.welded_first,
        w.welded_cur,
        &w.weld,
        &w.src_first,
        &w.src_cur,
        &w.surface,
        &w.surface,
        &[],       // input A face map (unused by the B-attributed fixtures)
        &TRI_FACE, // input B face map
    )
}

#[test]
fn f0016_shape_is_a_wedge() {
    assert_eq!(classify(&f0016_shape()), None);
}

#[test]
fn opposite_winding_rejects() {
    let mut w = f0016_shape();
    // Reverse the candidate's cyclic order: (3,1,9) → (3,9,1).
    w.welded_cur = [3, 9, 1];
    assert_eq!(classify(&w), Some("winding"));
}

#[test]
fn surface_label_mismatch_rejects() {
    let w = f0016_shape();
    let surface_cur = vec![LaInputId(0)];
    let got = wedge_reject_reason(
        w.raw_first,
        w.raw_cur,
        w.welded_first,
        w.welded_cur,
        &w.weld,
        &w.src_first,
        &w.src_cur,
        &w.surface,
        &surface_cur,
        &[],
        &TRI_FACE,
    );
    assert_eq!(got, Some("surface"));
}

#[test]
fn disjoint_raw_triples_reject() {
    // The a4 combinatorial variant where the two tris share NO raw
    // vertices (each corner independently welded): 3 shared-after-weld but
    // 0 shared raw indices.
    let mut w = f0016_shape();
    w.raw_cur = [5, 6, 7];
    w.weld = (0..10).collect();
    w.weld[5] = 9;
    w.weld[6] = 4;
    w.weld[7] = 1;
    assert_eq!(classify(&w), Some("raw-shared"));
}

#[test]
fn unfused_tips_reject() {
    let mut w = f0016_shape();
    w.weld = (0..10).collect(); // identity — tips 4 and 3 stay distinct
    assert_eq!(classify(&w), Some("tips-not-welded"));
}

#[test]
fn missing_lineage_rejects() {
    // The a4 adversary mock (`m3_adversary.rs`) carries `source:
    // Vec::new()`; the call site passes empty slices.
    let mut w = f0016_shape();
    w.src_first = Vec::new();
    w.src_cur = Vec::new();
    assert_eq!(classify(&w), Some("lineage"));
}

#[test]
fn cross_input_parents_reject() {
    let mut w = f0016_shape();
    w.src_cur = vec![(LaInputId(0), 1)];
    assert_eq!(classify(&w), Some("cross-input"));
}

#[test]
fn same_parent_rejects() {
    let mut w = f0016_shape();
    w.src_cur = vec![(LaInputId(1), 0)];
    assert_eq!(classify(&w), Some("same-parent"));
}

#[test]
fn different_face_parents_reject() {
    // Parents 0 and 2 of `TRI_FACE` tessellate DIFFERENT B-Rep faces — the
    // genuine independent-coincident-sheet configuration.
    let mut w = f0016_shape();
    w.src_cur = vec![(LaInputId(1), 2)];
    assert_eq!(classify(&w), Some("parents-not-same-face"));
}

#[test]
fn cross_input_empty_face_map_rejects() {
    // The fixture routes input A to an EMPTY face map — an A-attributed
    // pair must reject loudly rather than index it.
    let mut w = f0016_shape();
    w.src_first = vec![(LaInputId(0), 0)];
    w.src_cur = vec![(LaInputId(0), 1)];
    assert_eq!(classify(&w), Some("no-face-map"));
}

#[test]
fn out_of_range_parent_rejects() {
    let mut w = f0016_shape();
    w.src_cur = vec![(LaInputId(1), 99)];
    assert_eq!(classify(&w), Some("parent-range"));
}
