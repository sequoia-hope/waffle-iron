//! §4.4.1 AS WRITTEN — increment I1: the curve-authoritative seam
//! construction (spec `specs/yang_441_trim_cdt_construction.md` §3–§4).
//!
//! The production Stage 4 relocates existing mesh crossing vertices onto the
//! exact curve while KEEPING the Stage-2 connectivity; the 2026-08-06 census
//! measured that every self-crossing loop in the corpus is MINTED by exactly
//! that step (`cross_inherited = 0`). The paper's own construction
//! (`refs/text/yang2025_hybrid_boolean.txt:546-563`) does not have the
//! failure mode: the seam is the CURVE's own sample chain, the patch is
//! re-triangulated by CDT around it, and displaced chain vertices simply
//! cease to exist.
//!
//! This module holds the PURE mechanism for the I1 slice (planar patch
//! pairs, `Curve::LineSegment` seams — the plane×plane class of the CDT
//! ring-reject census cases):
//!
//! - [`seam_groups`] enumerates every intersection-curve seam UNCONDITIONALLY
//!   (the paper does not condition §4.4.1 on a defect detector — the measured
//!   negative of the `detect_nonmanifold_seams` selector, 2026-08-06).
//! - [`replace_seam_run`] rewrites a patch cycle so the seam run between its
//!   two junction endpoints becomes the direct chain — for a LineSegment the
//!   resample IS the two endpoints, and the relocated fold-back interior
//!   vertices (collinear but order-scrambled — the census's crossing mint)
//!   drop out of the boundary entirely. The pair-wide CDT rebuild
//!   ([`crate::stage4_splice::splice_seam_pair`]) then re-triangulates both
//!   sides against the clean seam with shared vertex identities.
//!
//! The pass driver lives beside the other Stage-4/5 passes in
//! `stage5_topology::run_construct_passes`, gated on `YANG_441_CONSTRUCT`
//! (gate-OFF byte-identical). Curved patches and non-line curves are LOUD
//! skips — increment I2's scope, never a silent partial repair.

use std::collections::{BTreeMap, BTreeSet};

use crate::geom::Curve;
use crate::stage4_splice::SplicePatch;

/// One seam: the intersection-curve edges shared by exactly one patch pair,
/// all naming the same exact curve.
#[derive(Debug, Clone)]
pub(crate) struct SeamGroup {
    /// Patch indices, `pair.0 < pair.1`.
    pub pair: (usize, usize),
    /// The seam's exact analytic curve (every member edge names it).
    pub curve: Curve,
    /// Canonical `(min, max)` mesh edges of the seam.
    pub edges: BTreeSet<(u32, u32)>,
}

/// Enumerate every intersection-curve seam over the current patch set.
///
/// An intersection edge belongs to a seam iff it appears in the CYCLES of
/// exactly two patches (the 2-manifold interior seam case — border edges of
/// unpaired provenance are not this increment's business). Edges of one pair
/// are partitioned by exact-curve equality: a pair meeting along two distinct
/// curves (two junction-separated seams) yields two groups. Deterministic:
/// iteration is over `BTreeMap`/`BTreeSet` orders only.
pub(crate) fn seam_groups(
    patches: &[SplicePatch],
    curves: &BTreeMap<(u32, u32), Curve>,
) -> Vec<SeamGroup> {
    // Canonical edge -> owning patch indices (via each patch's cycles).
    let mut owners: BTreeMap<(u32, u32), Vec<usize>> = BTreeMap::new();
    for (pi, p) in patches.iter().enumerate() {
        for cyc in &p.cycles {
            let n = cyc.len();
            for i in 0..n {
                let (s, e) = (cyc[i], cyc[(i + 1) % n]);
                let key = (s.min(e), s.max(e));
                let v = owners.entry(key).or_default();
                // A cycle may repeat an edge (a slit); count each owner once.
                if v.last() != Some(&pi) {
                    v.push(pi);
                }
            }
        }
    }

    let mut groups: Vec<SeamGroup> = Vec::new();
    for (&edge, curve) in curves {
        let Some(own) = owners.get(&edge) else {
            continue; // curve edge not on any current patch boundary
        };
        if own.len() != 2 {
            continue; // border / non-manifold multiplicity — not this seam class
        }
        let pair = (own[0].min(own[1]), own[0].max(own[1]));
        match groups
            .iter_mut()
            .find(|g| g.pair == pair && g.curve == *curve)
        {
            Some(g) => {
                g.edges.insert(edge);
            }
            None => groups.push(SeamGroup {
                pair,
                curve: *curve,
                edges: [edge].into(),
            }),
        }
    }
    groups
}

/// Rewrite `cycles` so the contiguous run spelled by `chain` (forward or
/// reversed, wraparound included) collapses to its two endpoints.
///
/// Returns `None` when no cycle contains the run — the caller's loud skip.
/// The interior chain vertices leave the boundary; the CDT rebuild decides
/// their fate (planar: geometrically redundant, dropped — exactly the
/// paper's "remove a mesh vertex" quality step for the collinear case).
pub(crate) fn replace_seam_run(cycles: &[Vec<u32>], chain: &[u32]) -> Option<Vec<Vec<u32>>> {
    if chain.len() < 3 {
        return None; // nothing to collapse — callers filter this earlier
    }
    let reversed: Vec<u32> = chain.iter().rev().copied().collect();
    for (ci, cyc) in cycles.iter().enumerate() {
        let n = cyc.len();
        if n < chain.len() {
            continue;
        }
        for cand in [chain, reversed.as_slice()] {
            for start in 0..n {
                if (0..cand.len()).all(|k| cyc[(start + k) % n] == cand[k]) {
                    // Rotate the cycle so the run begins at index 0, then keep
                    // its endpoints and everything after it.
                    let rot: Vec<u32> = (0..n).map(|k| cyc[(start + k) % n]).collect();
                    let mut new_cyc = Vec::with_capacity(n - cand.len() + 2);
                    new_cyc.push(cand[0]);
                    new_cyc.push(cand[cand.len() - 1]);
                    new_cyc.extend_from_slice(&rot[cand.len()..]);
                    let mut out = cycles.to_vec();
                    out[ci] = new_cyc;
                    return Some(out);
                }
            }
        }
    }
    None
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::geom::Surface;
    use cad_primitives::{Point3, Vector3};

    fn plane_patch(cycles: Vec<Vec<u32>>, tris: Vec<u32>) -> SplicePatch {
        SplicePatch {
            cycles,
            tris,
            surface: Surface::Plane {
                normal: Vector3::new(0.0, 0.0, 1.0),
                d: 0.0,
            },
        }
    }

    #[test]
    fn seam_groups_pairs_edges_by_curve() {
        let a = plane_patch(vec![vec![0, 1, 2, 3]], vec![0]);
        let b = plane_patch(vec![vec![1, 0, 4, 5]], vec![1]);
        let mut curves = BTreeMap::new();
        curves.insert((0u32, 1u32), Curve::LineSegment);
        let gs = seam_groups(&[a, b], &curves);
        assert_eq!(gs.len(), 1);
        assert_eq!(gs[0].pair, (0, 1));
        assert_eq!(gs[0].edges.len(), 1);
        assert!(matches!(gs[0].curve, Curve::LineSegment));
    }

    #[test]
    fn seam_groups_skips_non_pair_multiplicity() {
        // Edge (0,1) on ONE patch only — a border, not a seam.
        let a = plane_patch(vec![vec![0, 1, 2]], vec![0]);
        let mut curves = BTreeMap::new();
        curves.insert((0u32, 1u32), Curve::LineSegment);
        assert!(seam_groups(&[a], &curves).is_empty());
    }

    #[test]
    fn seam_groups_splits_distinct_curves_of_one_pair() {
        let a = plane_patch(vec![vec![0, 1, 2, 5, 3]], vec![0]);
        let b = plane_patch(vec![vec![1, 0, 4, 3, 5]], vec![1]);
        let circle = Curve::Circle {
            center: Point3::new(0.0, 0.0, 0.0),
            normal: Vector3::new(0.0, 0.0, 1.0),
            radius: 1.0,
        };
        let mut curves = BTreeMap::new();
        curves.insert((0u32, 1u32), Curve::LineSegment);
        curves.insert((3u32, 5u32), circle);
        let gs = seam_groups(&[a, b], &curves);
        assert_eq!(gs.len(), 2, "two curves ⇒ two seams for the same pair");
        assert!(gs.iter().all(|g| g.pair == (0, 1)));
    }

    #[test]
    fn replace_seam_run_forward_reversed_and_wraparound() {
        // Forward, mid-cycle: run 0-7-8-1 collapses to 0-1.
        let cycles = vec![vec![9, 0, 7, 8, 1, 5]];
        let out = replace_seam_run(&cycles, &[0, 7, 8, 1]).expect("forward");
        assert_eq!(out[0], vec![0, 1, 5, 9]);
        // The partner side walks the same run REVERSED (opposite winding).
        let cycles_rev = vec![vec![9, 1, 8, 7, 0, 5]];
        let out = replace_seam_run(&cycles_rev, &[0, 7, 8, 1]).expect("reversed");
        assert_eq!(out[0], vec![1, 0, 5, 9]);
        // Wraparound: the run crosses the cycle's representation seam.
        let cycles_wrap = vec![vec![8, 1, 5, 9, 0, 7]];
        let out = replace_seam_run(&cycles_wrap, &[0, 7, 8, 1]).expect("wraparound");
        assert_eq!(out[0], vec![0, 1, 5, 9]);
        // Not found.
        assert!(replace_seam_run(&cycles_wrap, &[2, 3, 4]).is_none());
    }
}
