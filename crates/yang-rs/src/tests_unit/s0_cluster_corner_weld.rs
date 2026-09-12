//! Stage-0 cluster-band corner weld (R0081, 2026-09-12).
//!
//! A B corner that is NOT bit-equal to an A corner in the group frame but
//! lies within the in-frame clustering band of one is identified with it
//! by `cluster_frame_coords_rim_aware`: the overlay then resolves the
//! shared key to A's 3D bits (`corners_a` first). Before the weld, B's own
//! vertex array kept its pre-cluster bits, so B's faces OUTSIDE the overlay
//! (the laterals around its cap) emitted a vertex 1e-15 … 1e-14 from the
//! cap's, and B's Stage-0 emission was non-conformal (R0081 op 3: 584 of
//! 588 gear-profile corners, 2,001 asymmetric directed edges, Stage 6
//! `reassembled output would be non-2-manifold`). The weld propagates the
//! clustering's decision into the solid; both paths (1×1 and n-ary) are
//! pinned here on the measured gap magnitude.

use std::collections::BTreeMap;

use cad_primitives::{BoolOp, Point3};

use crate::stage0::stage0_preprocess;
use crate::tests_unit::n2_junction::rj_box;
use crate::{BRep, Mesh};

/// Number of directed edges of `mesh` (by vertex INDEX) whose reverse is
/// used a different number of times — zero for a conformal closed mesh.
fn asymmetric_directed_edges(mesh: &Mesh) -> usize {
    let mut count: BTreeMap<(u32, u32), usize> = BTreeMap::new();
    for t in &mesh.tris {
        for k in 0..3 {
            *count.entry((t[k], t[(k + 1) % 3])).or_default() += 1;
        }
    }
    count
        .iter()
        .filter(|(&(u, v), &c)| count.get(&(v, u)).copied().unwrap_or(0) != c)
        .count()
}

/// Bit-identical duplicate vertex positions in `mesh` (a welded emission
/// has none — every shared corner is ONE mesh vertex).
fn duplicate_positions(mesh: &Mesh) -> usize {
    let mut seen: BTreeMap<[u64; 3], usize> = BTreeMap::new();
    for p in &mesh.verts {
        let a = p.as_array();
        *seen
            .entry([a[0].to_bits(), a[1].to_bits(), a[2].to_bits()])
            .or_default() += 1;
    }
    seen.values().filter(|&&c| c > 1).count()
}

/// A truncated square pyramid with the box's topology (rj_box's edge and
/// loop layout): bottom square `lo..hi` at `z0`, top square inset by
/// `inset` at `z1`. Its side planes are SLANTED, so stacking it on a box
/// whose top is its bottom square produces exactly ONE coplanar pair per
/// A top fragment — no in-plane-touching side pairs whose snap would
/// silently remove an in-plane nudge of the shared corners (a box on a box
/// has four such pairs, which is why that fixture cannot pin the weld).
fn frustum(lo: [f64; 2], hi: [f64; 2], z0: f64, z1: f64, inset: f64) -> BRep {
    let v = |x: f64, y: f64, z: f64| crate::BRepVertex {
        point: Point3::new(x, y, z),
    };
    let (li, hi2) = (
        [lo[0] + inset, lo[1] + inset],
        [hi[0] - inset, hi[1] - inset],
    );
    let vertices = vec![
        v(lo[0], lo[1], z0),
        v(hi[0], lo[1], z0),
        v(hi[0], hi[1], z0),
        v(lo[0], hi[1], z0),
        v(hi2[0], hi2[1], z1),
        v(hi2[0], li[1], z1),
        v(li[0], li[1], z1),
        v(li[0], hi2[1], z1),
    ];
    const EDGE_PAIRS: [(u32, u32); 24] = [
        (0, 1),
        (1, 2),
        (2, 3),
        (3, 0),
        (4, 5),
        (5, 6),
        (6, 7),
        (7, 4),
        (2, 1),
        (1, 5),
        (5, 4),
        (4, 2),
        (3, 2),
        (2, 4),
        (4, 7),
        (7, 3),
        (0, 3),
        (3, 7),
        (7, 6),
        (6, 0),
        (1, 0),
        (0, 6),
        (6, 5),
        (5, 1),
    ];
    let edges: Vec<crate::BRepEdge> = EDGE_PAIRS
        .iter()
        .map(|&(start, end)| crate::BRepEdge {
            start,
            end,
            curve: crate::Curve::LineSegment,
        })
        .collect();
    let faces: Vec<crate::BRepFace> = (0..6u32)
        .map(|i| {
            // Newell normal over the loop's start vertices; rj_box's loop
            // layout runs CW seen from outside (its face 0 loop 0→1→2→3 is
            // CCW from +z for the −z bottom), so outward = −Newell. Then
            // d = −n·p₀.
            let pts: Vec<[f64; 3]> = (4 * i..4 * i + 4)
                .map(|e| vertices[EDGE_PAIRS[e as usize].0 as usize].point.as_array())
                .collect();
            let mut n = [0.0f64; 3];
            for k in 0..4 {
                let (p, q) = (pts[k], pts[(k + 1) % 4]);
                n[0] += (p[1] - q[1]) * (p[2] + q[2]);
                n[1] += (p[2] - q[2]) * (p[0] + q[0]);
                n[2] += (p[0] - q[0]) * (p[1] + q[1]);
            }
            let len = (n[0] * n[0] + n[1] * n[1] + n[2] * n[2]).sqrt();
            let n = [-n[0] / len, -n[1] / len, -n[2] / len];
            let d = -(n[0] * pts[0][0] + n[1] * pts[0][1] + n[2] * pts[0][2]);
            crate::BRepFace {
                surface: crate::Surface::Plane {
                    normal: crate::Vector3::new(n[0], n[1], n[2]),
                    d,
                },
                outer_loop: (4 * i..4 * i + 4).collect(),
                inner_loops: Vec::new(),
                reversed: false,
            }
        })
        .collect();
    BRep::new(vertices, edges, faces).expect("frustum fixture builds")
}

/// Perturb every B-Rep vertex on the plane z = `z` by `dx` in x and `dy`
/// in y — an IN-PLANE offset the snap phase cannot remove and the bit-equal
/// cross-weld cannot see.
fn nudge_plane_vertices(b: &mut BRep, z: f64, dx: f64, dy: f64) -> usize {
    let mut n = 0;
    for v in b.vertices.iter_mut() {
        let p = v.point.as_array();
        if p[2] == z {
            v.point = Point3::new(p[0] + dx, p[1] + dy, p[2]);
            n += 1;
        }
    }
    n
}

/// The measured R0081 gap: corners 4e-15 … 4e-14 apart in-plane. On unit
/// boxes 4e-15 is ~18 ulps at 1.0 — not bit-equal, well inside the
/// clustering band (the pair band, TAU_MODEL = 1e-7).
const GAP: f64 = 4.0e-15;

/// 1×1 path: frustum B stacked on box A with B's bottom corners nudged
/// in-plane by the measured gap. The emitted B mesh must be conformal and
/// carry no duplicate positions (its bottom cap and laterals share the
/// welded corners).
#[test]
fn stacked_box_nudged_corners_emit_conformal_b_mesh() {
    let a = rj_box([0.0, 0.0, 0.0], [1.0, 1.0, 1.0]);
    let mut b = frustum([0.0, 0.0], [1.0, 1.0], 1.0, 2.0, 0.25);
    assert_eq!(nudge_plane_vertices(&mut b, 1.0, GAP, -GAP), 4);
    let s0 = stage0_preprocess(&a, &b)
        .expect("stacked boxes are a supported coplanar pair")
        .expect("the flush caps must be detected");
    assert_eq!(
        s0.pairs.len(),
        1,
        "one A-top × B-bottom pair, no side pairs"
    );
    assert!(s0.pairs[0].opposite, "flush caps oppose");
    for (tag, mesh) in [("A", &s0.mesh_a), ("B", &s0.mesh_b)] {
        assert_eq!(
            asymmetric_directed_edges(mesh),
            0,
            "{tag}'s Stage-0 emission must be conformal"
        );
        assert_eq!(
            duplicate_positions(mesh),
            0,
            "{tag}'s Stage-0 emission must not carry duplicate positions"
        );
    }
    // The shared corners are bit-identical across the two solids' meshes
    // (§4.5.5 identical overlap sampling).
    let key = |p: &Point3| {
        let a = p.as_array();
        [a[0].to_bits(), a[1].to_bits(), a[2].to_bits()]
    };
    let cap_a: std::collections::BTreeSet<[u64; 3]> = s0
        .mesh_a
        .verts
        .iter()
        .filter(|p| p.z() == 1.0)
        .map(key)
        .collect();
    let cap_b: std::collections::BTreeSet<[u64; 3]> = s0
        .mesh_b
        .verts
        .iter()
        .filter(|p| p.z() == 1.0)
        .map(key)
        .collect();
    assert_eq!(
        cap_a, cap_b,
        "the flush caps must sample identical vertices"
    );
}

/// n-ary path: A's top cap is split into two coplanar fragments by a slot
/// cut from above, so B's (nudged) bottom pairs with BOTH — a two-pair
/// plane group. Same contract on B's emission.
#[test]
fn nary_group_nudged_corners_emit_conformal_b_mesh() {
    let nb = crate::native_backend().expect("native backend");
    let slab = rj_box([0.0, 0.0, 0.0], [2.0, 1.0, 1.0]);
    let slot = rj_box([0.9, -1.0, 0.5], [1.1, 2.0, 2.0]);
    let a = crate::boolean(&slab, &slot, BoolOp::Subtract, &nb).expect("slab − slot");
    let top_faces = a
        .faces()
        .iter()
        .filter(|f| {
            matches!(f.surface, crate::Surface::Plane { normal, d }
                if normal.as_array()[2] > 0.5 && (d.abs() - 1.0).abs() < 1e-9)
        })
        .count();
    assert_eq!(
        top_faces, 2,
        "the slot must split A's top into two fragments"
    );
    let mut b = frustum([0.0, 0.0], [2.0, 1.0], 1.0, 2.0, 0.25);
    assert_eq!(nudge_plane_vertices(&mut b, 1.0, GAP, -GAP), 4);
    let s0 = stage0_preprocess(&a, &b)
        .expect("two-fragment plane group is supported")
        .expect("the flush caps must be detected");
    assert_eq!(s0.pairs.len(), 2, "B's bottom pairs with both A fragments");
    assert!(s0.pairs.iter().all(|p| p.opposite), "flush caps oppose");
    for (tag, mesh) in [("A", &s0.mesh_a), ("B", &s0.mesh_b)] {
        assert_eq!(
            asymmetric_directed_edges(mesh),
            0,
            "{tag}'s Stage-0 emission must be conformal"
        );
        assert_eq!(
            duplicate_positions(mesh),
            0,
            "{tag}'s Stage-0 emission must not carry duplicate positions"
        );
    }
}

/// Control: with bit-equal corners nothing is welded and the emission is
/// byte-identical to the historical path — the weld is a no-op there.
#[test]
fn bit_equal_corners_are_a_no_op() {
    let a = rj_box([0.0, 0.0, 0.0], [1.0, 1.0, 1.0]);
    let b = frustum([0.0, 0.0], [1.0, 1.0], 1.0, 2.0, 0.25);
    let s0 = stage0_preprocess(&a, &b)
        .expect("stacked boxes are a supported coplanar pair")
        .expect("the flush caps must be detected");
    assert_eq!(asymmetric_directed_edges(&s0.mesh_b), 0);
    assert_eq!(duplicate_positions(&s0.mesh_b), 0);
}
