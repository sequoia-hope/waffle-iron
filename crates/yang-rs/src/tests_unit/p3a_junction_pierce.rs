//! P3a #146 increment-1a unit fixtures: `junction_pierce_points` contract
//! (spec `specs/yang_146_conformal_junction_sampling.md` §4 increment 1).
//!
//! Fixtures use the axis-aligned box builder [`rj_box`] (per-loop-copy
//! `LineSegment` edges, 6 planar faces) — the same B-Rep shape the F0082
//! lead customer presents.

use super::n2_junction::rj_box;
use crate::boolean::{junction_pierce_points, PiercePoint};
use crate::*;

/// Interpenetrating boxes: B's four vertical edges pierce A's top face
/// (z=1) strictly inside its polygon, transversally (edge ⊥ plane). Each
/// pierce must mint ONCE per geometric edge and fan out to BOTH per-loop
/// copies with identical values (conformality by identity).
#[test]
fn transversal_pierce_mints_and_fans_out_to_all_copies() {
    let a = rj_box([0.0, 0.0, 0.0], [1.0, 1.0, 1.0]);
    let b = rj_box([0.3, 0.3, 0.5], [0.7, 0.7, 1.5]);
    let out = junction_pierce_points(&a, &b);

    // Every minted point lies on A's top plane z=1, strictly inside (0,1)
    // chord parameter, fully transversal.
    let b_side: Vec<(&(InputId, u32), &Vec<PiercePoint>)> = out
        .iter()
        .filter(|((input, _), _)| *input == InputId::B)
        .collect();
    assert!(
        !b_side.is_empty(),
        "B's vertical edges pierce A's top face — must mint"
    );
    for (_, pierces) in &b_side {
        for pp in pierces.iter() {
            assert!(
                (pp.point.z() - 1.0).abs() < 1e-12,
                "pierce must sit on A's top plane, got {:?}",
                pp.point
            );
            assert!(pp.t > 0.0 && pp.t < 1.0);
            assert!(pp.transversality > 0.9);
        }
    }
    // Fan-out: group by geometric edge (endpoint pair) — every copy of one
    // geometric edge carries the IDENTICAL pierce list.
    let mut by_geom: std::collections::BTreeMap<([u64; 3], [u64; 3]), Vec<&Vec<PiercePoint>>> =
        std::collections::BTreeMap::new();
    let kb = |p: Point3| [p.x().to_bits(), p.y().to_bits(), p.z().to_bits()];
    for ((_, ei), pierces) in &b_side {
        let e = &b.edges()[*ei as usize];
        let k0 = kb(b.vertices()[e.start as usize].point);
        let k1 = kb(b.vertices()[e.end as usize].point);
        let key = if k0 <= k1 { (k0, k1) } else { (k1, k0) };
        by_geom.entry(key).or_default().push(pierces);
    }
    // B has 4 piercing vertical geometric edges; each has 2 per-loop copies.
    assert_eq!(
        by_geom.len(),
        4,
        "4 vertical geometric edges pierce A's top"
    );
    for copies in by_geom.values() {
        assert_eq!(copies.len(), 2, "both per-loop copies carry the mint");
        assert_eq!(copies[0], copies[1], "copies carry IDENTICAL pierce lists");
        assert_eq!(copies[0].len(), 1, "one transversal pierce per edge");
    }
}

/// Flush stacking: B sits exactly ON TOP of A. B's bottom edges lie IN A's
/// top plane (tangential) and its vertical edges START on it (endpoint
/// margin) — nothing may mint on either side. Coplanar contact is the
/// Stage-0/M8 seam family (the C0044 lesson), never a pierce.
#[test]
fn tangential_and_endpoint_contact_must_not_mint() {
    let a = rj_box([0.0, 0.0, 0.0], [1.0, 1.0, 1.0]);
    let b = rj_box([0.3, 0.3, 1.0], [0.7, 0.7, 1.5]);
    let out = junction_pierce_points(&a, &b);
    assert!(
        out.is_empty(),
        "flush/tangential contact must not mint: {out:?}"
    );
}

/// A pierce landing ON the partner face's own boundary edge is a CORNER
/// (P3b stitch territory): B's vertical edge at (x=1.0, y=0.5) meets A's
/// top plane exactly on A's boundary x=1 — must NOT mint through A's top,
/// and symmetric corner-adjacent geometry must stay quiet.
#[test]
fn pierce_on_partner_boundary_must_not_mint() {
    let a = rj_box([0.0, 0.0, 0.0], [1.0, 1.0, 1.0]);
    let b = rj_box([1.0, 0.25, 0.5], [2.0, 0.75, 1.5]);
    let out = junction_pierce_points(&a, &b);
    // B's only candidate vertical edges sit at x=1.0 (ON A's boundary) and
    // x=2.0 (outside) — no mint through A's top face. A's edges at x∈{0,1}
    // likewise land on B's boundary plane x=1 or outside B. Nothing mints.
    assert!(
        out.is_empty(),
        "boundary-corner pierce must not mint: {out:?}"
    );
}

/// Disjoint solids: no pierce candidates at all.
#[test]
fn disjoint_solids_mint_nothing() {
    let a = rj_box([0.0, 0.0, 0.0], [1.0, 1.0, 1.0]);
    let b = rj_box([3.0, 3.0, 3.0], [4.0, 4.0, 4.0]);
    assert!(junction_pierce_points(&a, &b).is_empty());
}

/// Symmetric corner overlap (each box's corner inside the other): per side,
/// exactly THREE geometric edges pierce the partner — the vertical edge
/// through the buried corner plus the two horizontal edges leaving it —
/// each mid-edge (t=0.5), each fanned to its 2 per-loop copies.
#[test]
fn corner_overlap_mints_on_both_sides() {
    let a = rj_box([0.0, 0.0, 0.0], [1.0, 1.0, 1.0]);
    let b = rj_box([0.5, 0.5, 0.5], [1.5, 1.5, 1.5]);
    let out = junction_pierce_points(&a, &b);
    let b_mints = out.keys().filter(|(i, _)| *i == InputId::B).count();
    let a_mints = out.keys().filter(|(i, _)| *i == InputId::A).count();
    // B side: vertical at (0.5,0.5) → A top z=1 at (0.5,0.5,1); bottom
    // horizontals (0.5,0.5,0.5)→(1.5,0.5,0.5) → A face x=1 at (1,0.5,0.5)
    // and (0.5,0.5,0.5)→(0.5,1.5,0.5) → A face y=1 at (0.5,1,0.5).
    assert_eq!(b_mints, 6, "3 geometric B edges × 2 per-loop copies");
    // A side, mirrored: vertical at (1,1) → B bottom z=0.5 at (1,1,0.5);
    // top horizontals (1,0,1)→(1,1,1) → B face y=0.5 at (1,0.5,1) and
    // (1,1,1)→(0,1,1) → B face x=0.5 at (0.5,1,1).
    assert_eq!(a_mints, 6, "3 geometric A edges × 2 per-loop copies");
    for pierces in out.values() {
        assert_eq!(pierces.len(), 1);
        let pp = &pierces[0];
        assert!((pp.t - 0.5).abs() < 1e-12, "mid-edge pierce, t=0.5");
    }
}
