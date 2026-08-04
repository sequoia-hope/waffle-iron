//! Stage-4 fold-risk planner (Yang 2025 §4.4.1) — increment N2-3a.
//!
//! # The paper's own words
//!
//! §4.4.1 opens: *"As the intersections on the surfaces are relocated and
//! refined during the optimization, the bijectivity is essentially broken. Each
//! intersection curve is no longer mapped to the corresponding intersection
//! curve between the two meshes, **thus causing gaps or self-intersections**."*
//! (`refs/text/yang2025_hybrid_boolean.txt:605`.)
//!
//! That is the anchored F0067 / R0074 class verbatim: Stage 4 relocates the
//! intersection vertices onto the exact curve, the mesh around them is NOT
//! updated, and the loop Stage 5/6 later extracts from that stale mesh crosses
//! itself. The repair the paper prescribes is the Fig-11 mesh update
//! (`stage4_update::stage4_mesh_update`, built and unit-tested under N2-1,
//! still unwired) — NOT §4.5.2 local refinement, whose trigger is optimization
//! NON-convergence. F0067's relocations converge exactly; §4.5.2 does not apply
//! to it, and the roadmap's own finding Q3 already measured §4.5.2 as
//! recovering ~zero current cases.
//!
//! # What this module decides
//!
//! WHICH relocations need the Fig-11 treatment. The criterion is not invented
//! here — it is the one the 2026-07-29 R0074 fold census measured and recorded
//! in `docs/yang_deviations.md`: a fold is minted when the relocation
//! **displacement exceeds the pre-relocation spacing of the adjacent chain
//! vertices**. On R0074's 78 folds, `ratio < 1` was violated by 14 of the 16
//! Stage-4-MINTED folds and respected by 56 of the 62 INHERITED ones. The
//! displacement there was ~97% NORMAL to the chain, so what inverts local order
//! is its MAGNITUDE relative to the spacing, not its direction — which is why
//! the statistic is a bare ratio and carries no directional term.
//!
//! The 2026-08-03 loop-simplicity census reached the same statistic from the
//! other end: every self-crossing emitted planar loop reports
//! `disp_over_min_seg` well above 1 (F0067 41x…52,187x; the anchored notch is
//! 5.8x), and no SUPPORTED_CORRECT case has a self-crossing loop at all.
//!
//! # Scope of this increment
//!
//! Pure planner: pre/post positions + chain adjacency in, ranked risk list out.
//! No mesh mutation, no call site in `stage4_relocate_and_correct` — wiring the
//! Fig-11 merge arm onto this plan is N2-3b. Landing the decision function
//! first, unit-tested in isolation, is the same shape N2-1 used for
//! `stage4_mesh_update`.

use std::collections::{BTreeMap, BTreeSet, HashMap};

/// One relocation whose displacement is large enough, relative to its own
/// chain neighbourhood, to invert local order.
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct FoldRisk {
    /// The relocated mesh vertex.
    pub vertex: u32,
    /// How far Stage 4 moved it.
    pub displacement: f64,
    /// Distance to its CLOSEST chain neighbour, measured at PRE-relocation
    /// positions — the spacing the displacement has to fit inside.
    pub min_pre_spacing: f64,
    /// The neighbour realizing `min_pre_spacing`; the Fig-11 `merge`
    /// candidate.
    pub nearest_neighbour: u32,
    /// `displacement / min_pre_spacing`. `>= 1.0` is the minted-fold class.
    pub ratio: f64,
}

/// Rank every relocation by fold risk.
///
/// `pre` maps vertex → PRE-Stage-4 position (the `S4_PRE_POS` oracle's
/// contract: a POSITION, not a displacement, so it survives the four
/// `compact_unreferenced_verts` renumberings that run after Stage 4 — the gap
/// that left R0011/F0045 unmeasured in the 2026-07-29 census and that
/// `probe_remap_pre_pos` has since closed). `post` is the current mesh.
/// `chain_edges` are the `intersection_curves` keys: `(a, b)` means `a` and `b`
/// are consecutive on an analytic intersection chain.
///
/// A vertex is scored only when it MOVED, has a pre position, and has at least
/// one chain neighbour that also has a pre position — the spacing is otherwise
/// not defined, and reporting a risk without one would be a guess. Vertices
/// minted during Stage 4 (`pre` absent) are excluded for the same reason.
/// Returns every scored vertex, worst ratio first, so the caller applies its
/// own threshold rather than inheriting one from here.
pub fn rank_fold_risks(
    pre: &HashMap<u32, [f64; 3]>,
    post: &[[f64; 3]],
    chain_edges: &BTreeSet<(u32, u32)>,
) -> Vec<FoldRisk> {
    // Chain adjacency, both directions.
    let mut adj: BTreeMap<u32, BTreeSet<u32>> = BTreeMap::new();
    for &(a, b) in chain_edges {
        if a == b {
            continue;
        }
        adj.entry(a).or_default().insert(b);
        adj.entry(b).or_default().insert(a);
    }

    let dist = |p: [f64; 3], q: [f64; 3]| {
        ((p[0] - q[0]).powi(2) + (p[1] - q[1]).powi(2) + (p[2] - q[2]).powi(2)).sqrt()
    };

    let mut out = Vec::new();
    for (&v, &p0) in pre {
        let Some(&p1) = post.get(v as usize) else {
            continue;
        };
        let displacement = dist(p0, p1);
        if displacement == 0.0 {
            continue;
        }
        let Some(nbrs) = adj.get(&v) else {
            continue;
        };
        // Spacing is measured PRE-relocation on BOTH endpoints: the neighbour's
        // post position may itself have moved, and comparing a post-move
        // displacement against a post-move spacing would measure the outcome
        // rather than the risk.
        let mut best: Option<(u32, f64)> = None;
        for &w in nbrs {
            let Some(&q0) = pre.get(&w) else {
                continue;
            };
            let d = dist(p0, q0);
            if d == 0.0 {
                continue;
            }
            if best.is_none_or(|(_, bd)| d < bd) {
                best = Some((w, d));
            }
        }
        let Some((nearest_neighbour, min_pre_spacing)) = best else {
            continue;
        };
        out.push(FoldRisk {
            vertex: v,
            displacement,
            min_pre_spacing,
            nearest_neighbour,
            ratio: displacement / min_pre_spacing,
        });
    }
    // Worst first; vertex id breaks ties so the order is deterministic
    // (`pre` is a HashMap, so iteration order is not).
    out.sort_by(|a, b| {
        b.ratio
            .partial_cmp(&a.ratio)
            .unwrap_or(std::cmp::Ordering::Equal)
            .then(a.vertex.cmp(&b.vertex))
    });
    out
}

/// The subset that will mint a fold: `ratio >= 1`, i.e. the vertex is moved
/// further than the gap it has to stay inside.
///
/// Exactly the criterion the R0074 census validated (14/16 minted violate it,
/// 56/62 inherited respect it). `>=` rather than `>`: at ratio exactly 1 the
/// vertex lands ON its neighbour, which is the Fig-11 `merge` case, not a safe
/// relocation.
pub fn minting_risks(risks: &[FoldRisk]) -> Vec<FoldRisk> {
    risks.iter().copied().filter(|r| r.ratio >= 1.0).collect()
}

#[cfg(test)]
mod tests {
    use super::*;

    fn m(pairs: &[(u32, [f64; 3])]) -> HashMap<u32, [f64; 3]> {
        pairs.iter().copied().collect()
    }
    fn e(pairs: &[(u32, u32)]) -> BTreeSet<(u32, u32)> {
        pairs.iter().copied().collect()
    }

    /// A chain of three, middle vertex nudged well inside its spacing.
    #[test]
    fn small_displacement_is_not_a_fold_risk() {
        let pre = m(&[
            (0, [0.0, 0.0, 0.0]),
            (1, [1.0, 0.0, 0.0]),
            (2, [2.0, 0.0, 0.0]),
        ]);
        let post = vec![[0.0, 0.0, 0.0], [1.0, 0.1, 0.0], [2.0, 0.0, 0.0]];
        let r = rank_fold_risks(&pre, &post, &e(&[(0, 1), (1, 2)]));
        assert_eq!(r.len(), 1);
        assert_eq!(r[0].vertex, 1);
        assert!((r[0].ratio - 0.1).abs() < 1e-12, "{r:?}");
        assert!(minting_risks(&r).is_empty());
    }

    /// The F0067 shape: displacement several times the local spacing.
    #[test]
    fn displacement_beyond_the_spacing_is_a_minting_risk() {
        let seg = 6.4e-4;
        let push = 3.7e-3;
        let pre = m(&[
            (0, [0.0, 0.0, 0.0]),
            (1, [seg, 0.0, 0.0]),
            (2, [2.0 * seg, 0.0, 0.0]),
        ]);
        let post = vec![[0.0, 0.0, 0.0], [seg, push, 0.0], [2.0 * seg, 0.0, 0.0]];
        let r = rank_fold_risks(&pre, &post, &e(&[(0, 1), (1, 2)]));
        let mint = minting_risks(&r);
        assert_eq!(mint.len(), 1);
        assert_eq!(mint[0].vertex, 1);
        // The anchored 5.8x.
        assert!(
            (mint[0].ratio - push / seg).abs() < 1e-12,
            "ratio should be displacement/spacing: {mint:?}"
        );
        assert!(mint[0].ratio > 5.7 && mint[0].ratio < 5.9);
        assert_eq!(mint[0].nearest_neighbour, 0);
    }

    /// Spacing must come from the CLOSEST neighbour — a chain with one tight
    /// and one loose side is bounded by the tight one.
    #[test]
    fn spacing_is_the_closest_neighbour_not_the_average() {
        let pre = m(&[
            (0, [0.0, 0.0, 0.0]),
            (1, [1.0, 0.0, 0.0]),
            (2, [1.01, 0.0, 0.0]),
        ]);
        let post = vec![[0.0, 0.0, 0.0], [1.0, 0.05, 0.0], [1.01, 0.0, 0.0]];
        let r = rank_fold_risks(&pre, &post, &e(&[(0, 1), (1, 2)]));
        let v1 = r.iter().find(|x| x.vertex == 1).unwrap();
        assert_eq!(v1.nearest_neighbour, 2);
        assert!((v1.min_pre_spacing - 0.01).abs() < 1e-12);
        assert!(v1.ratio > 1.0, "0.05 into a 0.01 gap must mint: {v1:?}");
    }

    /// Both endpoints are read at PRE positions: a neighbour that also moved
    /// must not change this vertex's measured spacing.
    #[test]
    fn spacing_ignores_where_the_neighbour_moved_to() {
        let pre = m(&[(0, [0.0, 0.0, 0.0]), (1, [1.0, 0.0, 0.0])]);
        let near = vec![[0.0, 0.0, 0.0], [1.0, 0.5, 0.0]];
        let far = vec![[0.0, -9.0, 0.0], [1.0, 0.5, 0.0]];
        let a = rank_fold_risks(&pre, &near, &e(&[(0, 1)]));
        let b = rank_fold_risks(&pre, &far, &e(&[(0, 1)]));
        let ra = a.iter().find(|x| x.vertex == 1).unwrap();
        let rb = b.iter().find(|x| x.vertex == 1).unwrap();
        assert_eq!(ra.min_pre_spacing, rb.min_pre_spacing);
        assert_eq!(ra.ratio, rb.ratio);
    }

    #[test]
    fn unmoved_vertices_and_stage4_mints_are_not_scored() {
        // v0 did not move; v9 has no pre position (minted during Stage 4);
        // v1 moved but has no chain neighbour with a pre position.
        let pre = m(&[(0, [0.0, 0.0, 0.0]), (1, [1.0, 0.0, 0.0])]);
        let post = vec![[0.0, 0.0, 0.0], [1.0, 1.0, 0.0]];
        assert!(rank_fold_risks(&pre, &post, &e(&[(1, 9)])).is_empty());
        // With a scorable neighbour it appears.
        assert_eq!(rank_fold_risks(&pre, &post, &e(&[(0, 1)])).len(), 1);
    }

    #[test]
    fn ranking_is_worst_first_and_deterministic() {
        let pre = m(&[
            (0, [0.0, 0.0, 0.0]),
            (1, [1.0, 0.0, 0.0]),
            (2, [2.0, 0.0, 0.0]),
            (3, [3.0, 0.0, 0.0]),
        ]);
        let post = vec![
            [0.0, 0.0, 0.0],
            [1.0, 0.2, 0.0],
            [2.0, 3.0, 0.0],
            [3.0, 0.5, 0.0],
        ];
        let edges = e(&[(0, 1), (1, 2), (2, 3)]);
        let first = rank_fold_risks(&pre, &post, &edges);
        assert_eq!(
            first.iter().map(|r| r.vertex).collect::<Vec<_>>(),
            vec![2, 3, 1]
        );
        for _ in 0..8 {
            assert_eq!(rank_fold_risks(&pre, &post, &edges), first);
        }
    }

    /// Ratio exactly 1 lands the vertex on its neighbour — the Fig-11 `merge`
    /// case, so it belongs in the minting set.
    #[test]
    fn ratio_exactly_one_counts_as_minting() {
        let pre = m(&[(0, [0.0, 0.0, 0.0]), (1, [1.0, 0.0, 0.0])]);
        let post = vec![[0.0, 0.0, 0.0], [0.0, 0.0, 0.0]];
        let r = rank_fold_risks(&pre, &post, &e(&[(0, 1)]));
        assert_eq!(r[0].ratio, 1.0);
        assert_eq!(minting_risks(&r).len(), 1);
    }

    #[test]
    fn degenerate_inputs_are_skipped_not_scored() {
        // Self-loop edge, and a zero-length pre spacing (coincident pair).
        let pre = m(&[(0, [0.0, 0.0, 0.0]), (1, [0.0, 0.0, 0.0])]);
        let post = vec![[0.0, 0.0, 0.0], [1.0, 0.0, 0.0]];
        assert!(rank_fold_risks(&pre, &post, &e(&[(1, 1), (0, 1)])).is_empty());
    }
}
