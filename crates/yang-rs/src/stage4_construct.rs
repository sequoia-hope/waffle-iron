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
//!
//! # I1b — the PATCH with ALL its curves (2026-08-09)
//!
//! I1's one-seam-per-pass application measured SOUND, ZERO CONVERSIONS on
//! F0067: 39 seams collapsed, and the fixpoint decline census
//! (`SelfIntersectingPolyline` ×500) named the reason — a collapsed straight
//! seam still crosses the OTHER not-yet-collapsed relocated chains of the
//! same cycle, so mutually-blocked seams can never collapse pairwise. The
//! paper's own unit of work is plural ("we trim and update the meshes using
//! the intersection curveS"): the PATCH with all its curves.
//!
//! The I1b mechanism ([`collapse_patch_runs`] + [`rebuild_patch_planar`] +
//! [`apply_rebuild_batch`]) therefore collapses ALL of a patch's eligible
//! seam runs simultaneously and re-triangulates the patch SINGLE-SIDED: after
//! collapse each seam is an ordinary boundary edge of the modified cycles, so
//! a plain CDT of the cycle polygon suffices — no two-sided driver, no
//! constraint insertion, no tolerance. Conformality is by construction:
//! a collapsed seam is the SAME `(e0, e1)` mesh-vertex pair on both owner
//! patches (both rebuilt in the same batch), and every untouched boundary
//! chain is reproduced edge-for-edge by the CDT, so interfaces to
//! non-rebuilt neighbours are byte-identical.

use std::collections::{BTreeMap, BTreeSet};

use cad_primitives::{Point2, Point3};
use cherchi_rs::{cdt_with_interior_constraints, CdtError, Mesh};

use crate::brep::{TriangleAttribution, TriangleAttributionMap};
use crate::geom::{Curve, Surface};
use crate::stage4_project::{patch_from_cycles_shifted, SurfaceChart};
use crate::stage4_splice::{area_vector, dot3, SplicePatch};

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

/// Worst RELATIVE perpendicular offset of the chain's interior vertices from
/// the line through its endpoints (relative to the chain's own extent), or
/// `None` for a degenerate chord (coincident endpoints).
///
/// This is the STRAIGHTNESS IDENTITY the collapse requires and
/// `Curve::LineSegment`'s unit-variant equality fails to encode:
/// `seam_groups` merges ALL straight seams of one patch pair into one group,
/// so a chain `e0–x–e1` can be TWO different lines meeting at a real corner
/// `x` (coplanar-contact region boundaries — the R0095 regression), and
/// collapsing it to the chord `(e0,e1)` would CUT THE CORNER. A genuine
/// §4.4.1 seam's interior vertices were relocated ONTO the one exact line
/// (off-line by f64 rounding only, ~1e-15 relative, even when their PARAMETER
/// order is scrambled — the F0067 fold-backs), while a real corner is off by
/// a macroscopic fraction. The caller's band (1e-9 relative) sits six orders
/// from both; outside it the seam is LOUDLY skipped, never collapsed — the
/// P10-sanctioned use of a band: turning a silent wrong into a loud stop.
pub(crate) fn chain_straightness(verts: &[cad_primitives::Point3], chain: &[u32]) -> Option<f64> {
    let p = |v: u32| -> [f64; 3] {
        let w = verts[v as usize];
        [w.x(), w.y(), w.z()]
    };
    let (p0, p1) = (p(chain[0]), p(*chain.last().expect("chain non-empty")));
    let d = [p1[0] - p0[0], p1[1] - p0[1], p1[2] - p0[2]];
    let len2 = d[0] * d[0] + d[1] * d[1] + d[2] * d[2];
    if len2 == 0.0 || !len2.is_finite() {
        return None;
    }
    let mut extent2 = len2;
    let mut worst2 = 0.0f64;
    for &v in &chain[1..chain.len() - 1] {
        let x = p(v);
        let r = [x[0] - p0[0], x[1] - p0[1], x[2] - p0[2]];
        let r2 = r[0] * r[0] + r[1] * r[1] + r[2] * r[2];
        extent2 = extent2.max(r2);
        let t = (r[0] * d[0] + r[1] * d[1] + r[2] * d[2]) / len2;
        let perp = [r[0] - t * d[0], r[1] - t * d[1], r[2] - t * d[2]];
        worst2 = worst2.max(perp[0] * perp[0] + perp[1] * perp[1] + perp[2] * perp[2]);
    }
    if extent2 == 0.0 || !worst2.is_finite() {
        return None;
    }
    Some((worst2 / extent2).sqrt())
}

/// §4.4.1's NEAR-CURVE VERTEX REMOVAL predicate (spec §3 step 2, the I1f
/// increment): is `v` a boundary vertex lying ON the collapsed seam's
/// segment `e0→e1`, STRICTLY between the junction endpoints?
///
/// These are the F0067 walk-back vertices: exact plane×plane geometry puts
/// them EXACTLY on the seam line (perp ~1e-16 relative), parametrically
/// inside the run, but their edges are plain mesh boundary edges — so the
/// run collapse never swallows them and the boundary walks out to the
/// junction and back over them. The paper removes them ("we remove a mesh
/// vertex if it is too close to the intersection curve") and re-CDTs. The
/// band is the same 1e-9 relative measure as [`chain_straightness`] — an
/// identity test, six orders from both f64 noise and real geometry.
pub(crate) fn on_segment_interior(verts: &[Point3], e0: u32, e1: u32, v: u32) -> bool {
    let p = |i: u32| -> [f64; 3] {
        let w = verts[i as usize];
        [w.x(), w.y(), w.z()]
    };
    point_on_segment_interior(p(e0), p(e1), p(v))
}

/// [`on_segment_interior`] on bare POSITIONS — the same identity, for callers
/// whose segment endpoints are not two mesh vertices.
///
/// The §4-I9 relocation-domain postcondition needs exactly this: its segment is
/// ONE vertex's travel, `pre → post`, and the question is whether a still
/// neighbour lies on it. Sharing the predicate keeps the two gates on one
/// metric (the 1e-9 relative identity, i.e. `perp² ≤ 1e-18·len²`) rather than
/// letting a second collinearity band drift away from this one.
pub(crate) fn point_on_segment_interior(a: [f64; 3], b: [f64; 3], x: [f64; 3]) -> bool {
    let d = [b[0] - a[0], b[1] - a[1], b[2] - a[2]];
    let len2 = d[0] * d[0] + d[1] * d[1] + d[2] * d[2];
    if len2 == 0.0 || !len2.is_finite() {
        return false;
    }
    let r = [x[0] - a[0], x[1] - a[1], x[2] - a[2]];
    let t = (r[0] * d[0] + r[1] * d[1] + r[2] * d[2]) / len2;
    if t <= 0.0 || t >= 1.0 {
        return false; // beyond an endpoint — not this segment's interior
    }
    let perp = [r[0] - t * d[0], r[1] - t * d[1], r[2] - t * d[2]];
    let perp2 = perp[0] * perp[0] + perp[1] * perp[1] + perp[2] * perp[2];
    perp2 <= 1e-18 * len2
}

// =========================================================================
// I2c — input-edge chain refinement at seam-adjacent corners (spec §4-I2c)
// =========================================================================

/// One plain (input-edge) boundary run of a seam-owning patch, shared with
/// exactly one same-input neighbour patch and adjacent to an eligible seam
/// junction.
///
/// This is the I2c object (spec `yang_441_trim_cdt_construction.md` §4-I2c):
/// the Stage-1 discretization of a B-Rep edge of ONE input solid (the F0067
/// rib's wall∩cap edge), chord-anchored by tessellation and therefore off
/// the exact surface∩surface curve by up to the curved owner's sagitta.
/// Where such a chain meets an intersection-seam junction, two authorities
/// disagree by the chord gap — the chain's corner endpoint (975-class) vs
/// the exact junction (999-class) — and the boundary folds back between
/// them. Refining the chain onto the exact input edge (Fig-13's "boundary
/// points glide along boundary curves" discipline) lands the corner at the
/// junction, and the existing Fig-11(b) merge + near-curve removal close
/// the corner.
#[derive(Debug, Clone, PartialEq)]
pub(crate) struct InputEdgeChain {
    /// The seam-owning patch whose cycle carries the run.
    pub patch: usize,
    /// The single other owner of every run edge (same input, different face).
    pub neighbor: usize,
    /// Run vertices in cycle order (open, `len() >= 2`).
    pub verts: Vec<u32>,
    /// `(p, q)` corner adjacencies that scoped this run: chain endpoint `p`
    /// within `band` of eligible-seam junction `q` (`p != q`, nearest `q`,
    /// deterministic tie-break). After refinement `p` lies on the exact
    /// input edge and the driver feeds `p -> q` to the Fig-11(b) merge.
    pub corner_pairs: Vec<(u32, u32)>,
}

/// Why a qualified plain run was not returned as an [`InputEdgeChain`] —
/// the identification's own coverage ledger, printed by the driver under
/// `YANG_441_VERBOSE`.
#[derive(Debug, Clone, PartialEq)]
pub(crate) struct RunSkip {
    pub patch: usize,
    pub neighbor: usize,
    pub verts: Vec<u32>,
    pub reason: RunSkipReason,
}

#[derive(Debug, Clone, Copy, PartialEq)]
pub(crate) enum RunSkipReason {
    /// No endpoint pairs with (or is) a junction — outside I2c's scope.
    NotSeamAdjacent,
    /// The neighbour's copy of a run another scoped patch already returned.
    Deduped,
}

/// Enumerate every seam-adjacent input-edge chain over the scoped patches.
///
/// A cycle edge belongs to a run iff it is PLAIN (not an intersection-curve
/// edge, not one of the batch's seam-chain or collapsed-direct edges), is
/// owned by exactly two patches `{patch, neighbor}`, and both owners carry
/// the SAME `InputId` with DIFFERENT attributions — the discretization of a
/// B-Rep edge of one input solid. Maximal same-neighbour runs are returned
/// when at least one endpoint lies within `band` of a junction vertex; a
/// cycle fully shared with one neighbour (a closed ring, no corner) is
/// skipped. Runs shared by two scoped patches are returned once (canonical
/// orientation). Deterministic: `BTreeMap`/`BTreeSet` iteration only.
#[allow(clippy::too_many_arguments)]
pub(crate) fn input_edge_chains(
    patches: &[SplicePatch],
    patch_attr: &[Option<TriangleAttribution>],
    verts: &[Point3],
    curves: &BTreeMap<(u32, u32), Curve>,
    seam_edges: &BTreeSet<(u32, u32)>,
    junctions: &BTreeSet<u32>,
    scope: &BTreeSet<usize>,
    band: f64,
) -> (Vec<InputEdgeChain>, Vec<RunSkip>) {
    let mut owners: BTreeMap<(u32, u32), Vec<usize>> = BTreeMap::new();
    for (pi, p) in patches.iter().enumerate() {
        for cyc in &p.cycles {
            let n = cyc.len();
            for i in 0..n {
                let (s, e) = (cyc[i], cyc[(i + 1) % n]);
                let key = (s.min(e), s.max(e));
                let v = owners.entry(key).or_default();
                if v.last() != Some(&pi) {
                    v.push(pi);
                }
            }
        }
    }
    let dist = |x: u32, y: u32| -> f64 {
        let (a, b) = (verts[x as usize], verts[y as usize]);
        ((a.x() - b.x()).powi(2) + (a.y() - b.y()).powi(2) + (a.z() - b.z()).powi(2)).sqrt()
    };
    // Nearest junction within `band` of `p` (excluding `p` itself); ties
    // break to the lower vertex index. Near-coincident junction copies (the
    // femto family) are equally valid merge targets — the pair itself stays
    // a separately-recorded defect. An endpoint that IS a junction never
    // pairs (it is already under junction authority — the caller PINS it);
    // it still qualifies the run as seam-adjacent.
    let corner_of = |p: u32| -> Option<(u32, u32)> {
        if junctions.contains(&p) {
            return None;
        }
        junctions
            .iter()
            .filter(|&&q| q != p)
            .map(|&q| (q, dist(p, q)))
            .filter(|&(_, d)| d <= band)
            .min_by(|a, b| a.1.total_cmp(&b.1).then(a.0.cmp(&b.0)))
            .map(|(q, _)| (p, q))
    };

    let mut seen: BTreeSet<Vec<u32>> = BTreeSet::new();
    let mut skips: Vec<RunSkip> = Vec::new();
    let mut out: Vec<InputEdgeChain> = Vec::new();
    for &pi in scope {
        let Some(my) = patch_attr[pi] else {
            continue; // mixed/absent attribution — not attributable to a face
        };
        for cyc in &patches[pi].cycles {
            let n = cyc.len();
            if n < 2 {
                continue;
            }
            // Per-edge qualified neighbour.
            let label: Vec<Option<usize>> = (0..n)
                .map(|i| {
                    let (s, e) = (cyc[i], cyc[(i + 1) % n]);
                    let key = (s.min(e), s.max(e));
                    if curves.contains_key(&key) || seam_edges.contains(&key) {
                        return None;
                    }
                    let own = owners.get(&key)?;
                    if own.len() != 2 {
                        return None;
                    }
                    let qi = match (own[0] == pi, own[1] == pi) {
                        (true, false) => own[1],
                        (false, true) => own[0],
                        _ => return None,
                    };
                    let qa = patch_attr[qi]?;
                    (qa.input == my.input && qa != my).then_some(qi)
                })
                .collect();
            // A start index at a label boundary; none ⇒ uniform labels —
            // either nothing qualifies or a closed one-neighbour ring (no
            // corner endpoint either way).
            let Some(start) = (0..n).find(|&i| label[i] != label[(i + n - 1) % n]) else {
                continue;
            };
            let mut i = 0;
            while i < n {
                let at = (start + i) % n;
                let Some(qi) = label[at] else {
                    i += 1;
                    continue;
                };
                let mut len = 1;
                while i + len < n && label[(start + i + len) % n] == Some(qi) {
                    len += 1;
                }
                let run: Vec<u32> = (0..=len).map(|k| cyc[(start + i + k) % n]).collect();
                i += len;
                let ends = [run[0], *run.last().expect("run non-empty")];
                let corner_pairs: Vec<(u32, u32)> =
                    ends.iter().filter_map(|&p| corner_of(p)).collect();
                if corner_pairs.is_empty() && !ends.iter().any(|e| junctions.contains(e)) {
                    skips.push(RunSkip {
                        patch: pi,
                        neighbor: qi,
                        verts: run,
                        reason: RunSkipReason::NotSeamAdjacent,
                    });
                    continue;
                }
                let canonical = if run.first() <= run.last() {
                    run.clone()
                } else {
                    run.iter().rev().copied().collect()
                };
                if !seen.insert(canonical) {
                    // The neighbour's copy of an already-found run.
                    skips.push(RunSkip {
                        patch: pi,
                        neighbor: qi,
                        verts: run,
                        reason: RunSkipReason::Deduped,
                    });
                    continue;
                }
                out.push(InputEdgeChain {
                    patch: pi,
                    neighbor: qi,
                    verts: run,
                    corner_pairs,
                });
            }
        }
    }
    (out, skips)
}

/// Why an input-edge chain refinement was refused — every variant a LOUD,
/// censused stop (P9/P10), never a silent partial move.
#[derive(Debug, Clone, PartialEq)]
pub(crate) enum RulingError {
    /// The plane's normal is not perpendicular to the cylinder axis: the
    /// exact intersection is a conic, not a ruling — the I2c tail.
    NonParallelAxis { dot: f64 },
    /// The plane misses the cylinder entirely (`|h| > R`): the claimed
    /// adjacency has no exact edge — an upstream defect, refuse loud.
    NoRuling { excess: f64 },
    /// A chain vertex sits closer to the OTHER of the two candidate rulings
    /// than to the chosen one — the chain straddles the plane's two rulings.
    AmbiguousRuling { vert: u32 },
    /// A vertex's projection displacement exceeds the derived chord band —
    /// this chain is not a chord-anchored discretization of the claimed
    /// edge (moving it would be a repair, not a refinement).
    Displacement { vert: u32, dist: f64 },
    /// Zero-length normal or axis.
    Degenerate,
}

/// Project a chain of mesh vertices onto the exact plane∩cylinder RULING —
/// the I2c refinement for the `Plane × Cylinder` input-edge class.
///
/// The two input faces meet along a ruling only when the plane is parallel
/// to the axis (`n̂·d̂ = 0`, an identity of the input B-Rep, checked at the
/// same 1e-9 identity scale as the module's other predicates). The ruling
/// candidates are the 0/1/2 lines `x = c + h·n̂ ± γ·(d̂×n̂) + t·d̂` with
/// `h` the axis→plane signed distance and `γ = √(R²−h²)`; the chain picks
/// the candidate EVERY vertex is nearest (else [`RulingError::AmbiguousRuling`]).
/// Each vertex projects axially onto the chosen line; a displacement above
/// `band` refuses the whole chain. Idempotent: a second application moves
/// nothing.
pub(crate) fn refine_chain_to_ruling(
    verts: &[Point3],
    chain: &[u32],
    plane: &Surface,
    cyl: &Surface,
    band: f64,
) -> Result<Vec<(u32, Point3)>, RulingError> {
    let (
        Surface::Plane { normal, d },
        Surface::Cylinder {
            axis_point,
            axis_dir,
            radius,
        },
    ) = (plane, cyl)
    else {
        return Err(RulingError::Degenerate); // caller matches the pair
    };
    let arr3 = |p: &Point3| [p.x(), p.y(), p.z()];
    let n = [normal.x(), normal.y(), normal.z()];
    let nn = (n[0] * n[0] + n[1] * n[1] + n[2] * n[2]).sqrt();
    let a = [axis_dir.x(), axis_dir.y(), axis_dir.z()];
    let an = (a[0] * a[0] + a[1] * a[1] + a[2] * a[2]).sqrt();
    if nn == 0.0 || an == 0.0 || !nn.is_finite() || !an.is_finite() {
        return Err(RulingError::Degenerate);
    }
    let nh = [n[0] / nn, n[1] / nn, n[2] / nn];
    let ah = [a[0] / an, a[1] / an, a[2] / an];
    let dot = nh[0] * ah[0] + nh[1] * ah[1] + nh[2] * ah[2];
    if dot.abs() > 1e-9 {
        return Err(RulingError::NonParallelAxis { dot });
    }
    // Plane `n·x + d = 0` ⇒ n̂·x = −d/‖n‖. With n̂ ⊥ d̂ the radial offset of
    // the plane from the axis along n̂ is h; the in-plane radial direction is
    // e2 = d̂ × n̂ (unit, since n̂ ⊥ d̂).
    let p0 = -d / nn;
    let c = arr3(axis_point);
    let h = p0 - (nh[0] * c[0] + nh[1] * c[1] + nh[2] * c[2]);
    let r = *radius;
    if h.abs() > r {
        return Err(RulingError::NoRuling {
            excess: h.abs() - r,
        });
    }
    let gamma = (r * r - h * h).max(0.0).sqrt();
    let e2 = [
        ah[1] * nh[2] - ah[2] * nh[1],
        ah[2] * nh[0] - ah[0] * nh[2],
        ah[0] * nh[1] - ah[1] * nh[0],
    ];
    let q0 = |sign: f64| -> [f64; 3] {
        [
            c[0] + h * nh[0] + sign * gamma * e2[0],
            c[1] + h * nh[1] + sign * gamma * e2[1],
            c[2] + h * nh[2] + sign * gamma * e2[2],
        ]
    };
    let line_dist = |q: [f64; 3], x: [f64; 3]| -> f64 {
        let rel = [x[0] - q[0], x[1] - q[1], x[2] - q[2]];
        let t = rel[0] * ah[0] + rel[1] * ah[1] + rel[2] * ah[2];
        let perp = [rel[0] - t * ah[0], rel[1] - t * ah[1], rel[2] - t * ah[2]];
        (perp[0] * perp[0] + perp[1] * perp[1] + perp[2] * perp[2]).sqrt()
    };
    let (qp, qm) = (q0(1.0), q0(-1.0));
    let (mut sum_p, mut sum_m) = (0.0f64, 0.0f64);
    for &v in chain {
        let x = arr3(&verts[v as usize]);
        sum_p += line_dist(qp, x);
        sum_m += line_dist(qm, x);
    }
    let chosen = if sum_p <= sum_m { qp } else { qm };
    let other = if sum_p <= sum_m { qm } else { qp };
    let mut moves: Vec<(u32, Point3)> = Vec::with_capacity(chain.len());
    for &v in chain {
        let x = arr3(&verts[v as usize]);
        if gamma > 0.0 && line_dist(other, x) < line_dist(chosen, x) {
            return Err(RulingError::AmbiguousRuling { vert: v });
        }
        let rel = [x[0] - chosen[0], x[1] - chosen[1], x[2] - chosen[2]];
        let t = rel[0] * ah[0] + rel[1] * ah[1] + rel[2] * ah[2];
        let proj = [
            chosen[0] + t * ah[0],
            chosen[1] + t * ah[1],
            chosen[2] + t * ah[2],
        ];
        let disp =
            ((proj[0] - x[0]).powi(2) + (proj[1] - x[1]).powi(2) + (proj[2] - x[2]).powi(2)).sqrt();
        if disp > band {
            return Err(RulingError::Displacement {
                vert: v,
                dist: disp,
            });
        }
        moves.push((v, Point3::new(proj[0], proj[1], proj[2])));
    }
    Ok(moves)
}

/// Why an I1b patch rebuild or batch write-back was refused. Every variant is
/// a P9/P10 LOUD stop, censused by the driver — never a silent partial repair.
#[derive(Debug, Clone, PartialEq)]
pub(crate) enum ConstructError {
    /// The patch is not planar. Single-sided rebuild of a curved patch would
    /// drop its interior chord-fidelity vertices — increment I2's scope.
    NonPlanarPatch { patch: usize },
    /// `patch_from_cycles_shifted` rejected the modified cycles (short cycle,
    /// bad index, zero-area outer boundary).
    MalformedPatch { patch: usize },
    /// A cylinder patch's θ-branches could not be resolved: the boundary
    /// encircles the axis, a vertex is reached on two branches, or an
    /// interior vertex has no branch inside the unwrapped boundary span.
    ThetaUnwrap { patch: usize },
    /// I13a: a cone patch's boundary touches or crosses the apex station
    /// (`z ≤ 0` in the chart) — the single-nappe `(θ, z)` chart is not
    /// injective there, so the rebuild refuses rather than re-triangulate
    /// through the apex.
    ApexInPatch { patch: usize },
    /// The plain CDT of the modified cycle polygon refused. The two live
    /// classes: `TriangulationFailed` — the modified boundary still
    /// self-crosses (a residual crossing NOT minted by a seam chain) — and
    /// `DuplicateVertex` — two kept boundary vertices coincide in the chart
    /// (the femto-pair junction family).
    Cdt { patch: usize, error: CdtError },
    /// The rebuilt patch has no well-defined orientation to match against the
    /// original (degenerate area vector).
    DegenerateOrientation { patch: usize },
    /// The patch's old triangles do not all share one attribution, so the
    /// replacement triangles have no unambiguous attribution to inherit.
    MixedAttribution { patch: usize },
    /// I2d (Yang §4.4.1's closing sentence, "we recalculate d(T) to
    /// maintain controllable error"): the rebuilt CURVED patch certifies a
    /// LARGER discretization error than the triangles it replaces. A chart
    /// CDT is geometry-blind — on a curved patch, chart-valid
    /// triangulations are NOT geometry-equivalent, and a boundary whose
    /// θ-sampling is sparse away from the rims forces secant triangles
    /// that shave the cylindrical bulge (the kv6b revolve∪box measurement:
    /// watertight, topology-clean, −10 % pipeline-mesh volume). The budget
    /// is the patch's own PRE-rebuild certified max `d(T)` — like for like
    /// (certified bound vs certified bound), tolerance-free.
    ChordDegradation {
        patch: usize,
        old_max: f64,
        new_max: f64,
    },
    /// I2d: a pre- or post-rebuild triangle could not be `d(T)`-certified
    /// (a θ-branch lands outside the patch's unwrapped span, or
    /// [`crate::stage4_dt::d_of_t`] refused). Without a certified budget
    /// the curved rebuild refuses — loud, never a silent acceptance.
    ChordCertify { patch: usize },
    /// [`apply_rebuild_batch`] was handed a plan built against a different
    /// mesh.
    StalePlan {
        expected_tris: u32,
        actual_tris: u32,
    },
    /// Two rebuilds in one batch claim the same old triangle — a driver bug,
    /// not input (flood-fill patches are disjoint).
    OverlappingBatch { tri: u32 },
    /// [`rebuild_merge_fan`]: the triangles incident to the victim do not form
    /// ONE simple fan — their opposite edges chain into more than one run, or a
    /// vertex is reached twice. A pinched / non-manifold vertex; re-triangulating
    /// it locally would guess which sheet the merge belongs to. `reason` names
    /// WHICH condition rejected, so the decline is attributable without a
    /// re-run (the §4-I6 census lesson: a refusal that does not name its
    /// condition cannot be scoped).
    FanNotSimple {
        patch: usize,
        victim: u32,
        reason: FanReason,
    },
    /// [`rebuild_merge_fan`]: the survivor is not on the victim's link, so the
    /// two are not joined by a triangle edge and the merge is not the local
    /// Fig-11 operation.
    FanSurvivorNotAdjacent { patch: usize, victim: u32 },
    /// f2c [`split_boundary_edge`]: the named edge has `incident` patch
    /// triangles instead of exactly one — it is absent, interior, or the
    /// patch is non-manifold there. A boundary seam-insert only splits a
    /// true boundary edge.
    EdgeNotBoundary {
        patch: usize,
        edge: (u32, u32),
        incident: usize,
    },
    /// f2c [`split_boundary_edge`]: a split child would flip or degenerate
    /// the parent triangle's orientation with the inserted vertex at its
    /// mint position.
    SplitFlip { patch: usize },
}

/// Which condition made [`rebuild_merge_fan`] declare the victim's fan
/// unsimple. Diagnostic only — every variant is the same loud refusal — but it
/// separates populations that need DIFFERENT repairs, so the census can scope
/// them apart instead of reporting one opaque `FanNotSimple` count.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(crate) enum FanReason {
    /// A fan triangle repeats the victim, or its two link corners coincide.
    Degenerate,
    /// f2c `delete_boundary_fan` only: the victim's link is not a single
    /// simple OPEN chain — the victim is interior to the patch (closed
    /// link: deleting the fan would punch a hole) or the region walk did
    /// not cover every link edge in one run.
    Closed { fan: usize },
    /// Two fan triangles leave the victim toward the SAME link vertex, or two
    /// arrive at the same one: the patch meets the victim on more than one
    /// sheet and the link is not a path.
    Pinch { fan: usize },
    /// The link edges chain into `runs` disjoint runs — the patch holds the
    /// victim in several separate fans. `with_survivor` counts the runs whose
    /// link contains the survivor.
    Split {
        fan: usize,
        runs: usize,
        with_survivor: usize,
    },
    /// One run, but shorter than a triangle: the link has 2 vertices, so the
    /// victim carries a SINGLE fan triangle in this patch (`fan == 1`) or a
    /// doubled pair over the same link edge (`fan == 2`). Those are different
    /// configurations, so the count is reported.
    Short { fan: usize, link: usize },
    /// I13d run regions only: a region vertex that is neither a victim nor on
    /// the link — the rebuild would silently disconnect it (a boundary vertex
    /// sandwiched between non-consecutive victims, or an interior vertex whose
    /// whole fan lies inside the region). Refused, never guessed.
    Orphaned { fan: usize, vertex: u32 },
    /// I13e group regions only: the region-boundary cycle's maximal victim
    /// arcs do not correspond one-to-one with the group's sites present on
    /// this patch — two sites' arcs fused (victims of different sites
    /// adjacent on the boundary), one site's victims split into several
    /// arcs or partially region-interior, or an arc mixes two sites.
    ArcMismatch {
        fan: usize,
        arcs: usize,
        sites: usize,
    },
}

/// One patch's single-sided rebuild, entirely in MESH index space and ready
/// for [`apply_rebuild_batch`]. The CDT adds no Steiner points of its own; a
/// seedless rebuild references only existing mesh vertices. An I2e-seeded
/// curved rebuild additionally carries `new_verts` — chart-lifted interior
/// seed positions, referenced by `new_tris` as `plan_verts + k` and remapped
/// onto the appended block by the write-back.
#[derive(Debug, Clone, PartialEq)]
pub(crate) struct PatchRebuild {
    /// Index of the patch in the pass's patch list (diagnostic only).
    pub patch: usize,
    /// The old triangles this rebuild replaces (indices into `mesh.tris`).
    pub old_tris: Vec<u32>,
    /// Replacement triangles in mesh vertex indices, orientation matched to
    /// the old patch's outward sense.
    pub new_tris: Vec<[u32; 3]>,
    /// I2e interior seed vertices (chart-lifted, exactly on-surface) to
    /// append to the mesh; `new_tris` references the k-th one as
    /// `plan_verts + k`. Empty for every seedless rebuild.
    pub new_verts: Vec<Point3>,
    /// Mesh vertices the old triangles referenced that the rebuild does not:
    /// collapsed seam-chain interiors plus planar flood interiors. The driver
    /// scans these for foreign references before committing the batch.
    pub dropped: BTreeSet<u32>,
    /// Mesh extents the plan was built against; the write-back refuses a
    /// changed mesh.
    pub plan_verts: u32,
    pub plan_tris: u32,
}

/// Collapse ALL of `chains` in `cycles` — the I1b per-patch simultaneous
/// collapse. Chains of distinct seams are disjoint runs sharing at most
/// junction ENDPOINTS (which every collapse keeps), so sequential application
/// is order-independent; each chain must still spell a contiguous run when
/// its turn comes.
///
/// `Err(i)` names the chain that failed — either its run was not contiguous
/// in the (progressively modified) cycles, or collapsing it would leave a
/// cycle with fewer than 3 vertices (the whole cycle was the open chain).
/// The caller drops that SEAM from the batch (both owners) and retries.
// Superseded in the driver by the mixed-action loop (I2b adds conic
// reorders), which applies the same sequential semantics inline. Kept as the
// pure line-only form its own tests exercise.
#[cfg_attr(not(test), allow(dead_code))]
pub(crate) fn collapse_patch_runs(
    cycles: &[Vec<u32>],
    chains: &[Vec<u32>],
) -> Result<Vec<Vec<u32>>, usize> {
    let mut cur: Vec<Vec<u32>> = cycles.to_vec();
    for (i, chain) in chains.iter().enumerate() {
        cur = replace_seam_run(&cur, chain).ok_or(i)?;
        if cur.iter().any(|c| c.len() < 3) {
            return Err(i);
        }
    }
    Ok(cur)
}

/// Re-triangulate one patch single-sided against its modified cycles —
/// Plane and Cylinder charts (I2a; Sphere/Cone/Torus stay a loud refusal).
///
/// `patch.cycles` are the post-[`collapse_patch_runs`] cycles; `patch.tris`
/// the ORIGINAL triangle set being replaced. The collapsed seams are ordinary
/// boundary edges here — the CDT reproduces every boundary edge exactly, so
/// each collapsed seam's `(e0, e1)` pair and every untouched neighbour chain
/// come out shared-by-index.
///
/// Interior vertices: for a PLANAR patch they are geometrically redundant and
/// are dropped (the paper's collinear "remove a mesh vertex" quality step; the
/// caller's foreign-reference scan makes that safe). For a CYLINDER patch they
/// carry chord fidelity, so they are CARRIED into the CDT (`interior` keep
/// list) after θ-branch assignment: a cylinder patch never encircles the axis
/// (`unwrap_theta` refuses), so its unwrapped boundary spans < 2π and each
/// interior vertex has exactly one branch landing inside that span — no
/// tolerance, a branch containment check that fails loud.
pub(crate) fn rebuild_patch_planar(
    mesh: &Mesh,
    patch_index: usize,
    patch: &SplicePatch,
) -> Result<PatchRebuild, ConstructError> {
    if !SurfaceChart::supports(&patch.surface) {
        return Err(ConstructError::NonPlanarPatch { patch: patch_index });
    }
    let chart = SurfaceChart::new(patch.surface)
        .ok_or(ConstructError::NonPlanarPatch { patch: patch_index })?;
    let shift = crate::stage4_splice::unwrap_theta(
        &chart,
        &mesh.verts,
        &patch.cycles,
        crate::stage4_splice::Side::A,
    )
    .map_err(|_| ConstructError::ThetaUnwrap { patch: patch_index })?;
    let (p2, back) = patch_from_cycles_shifted(&chart, &mesh.verts, &patch.cycles, &shift)
        .ok_or(ConstructError::MalformedPatch { patch: patch_index })?;
    // I13a apex guard: the cone chart's `(θ, z)` is injective only on the
    // single nappe strictly beyond the apex (`z > 0` — the same `v ≥ 0`
    // convention `d_of_t` certifies; the apex itself has no azimuth). A patch
    // touching or crossing the apex station refuses loudly — a capability
    // boundary, never a fallback.
    if matches!(chart, SurfaceChart::Cone { .. }) && p2.verts.iter().any(|p| p.y() <= 0.0) {
        return Err(ConstructError::ApexInPatch { patch: patch_index });
    }

    // Interior carry (cylinder only): every old-triangle vertex not on the
    // cycles, branch-assigned into the unwrapped boundary span.
    let cycle_verts: BTreeSet<u32> = patch.cycles.iter().flatten().copied().collect();
    let mut interior_back: Vec<u32> = Vec::new();
    let mut pool = p2.verts.clone();
    let mut interior_idx: Vec<u32> = Vec::new();
    let theta_span: Option<(f64, f64)> = if !matches!(patch.surface, Surface::Plane { .. }) {
        Some(
            pool.iter()
                .take(back.len())
                .fold((f64::INFINITY, f64::NEG_INFINITY), |(lo, hi), p| {
                    (lo.min(p.x()), hi.max(p.x()))
                }),
        )
    } else {
        None
    };
    if let Some((theta_min, theta_max)) = theta_span {
        let interiors: BTreeSet<u32> = patch
            .tris
            .iter()
            .flat_map(|&t| mesh.tris[t as usize])
            .filter(|v| !cycle_verts.contains(v))
            .collect();
        // §4.4.1 near-curve removal applies to cylinder patches too: a
        // candidate lying ON a boundary edge (a dropped seam-chain interior —
        // collinear ruling vertices) is a REMOVED vertex, not a fidelity
        // point. Carrying it would make spade split the boundary constraint
        // one-sidedly — the F0059 edge-use imbalance.
        let on_boundary_edge = |v: u32| -> bool {
            patch.cycles.iter().any(|cyc| {
                let n = cyc.len();
                (0..n).any(|i| on_segment_interior(&mesh.verts, cyc[i], cyc[(i + 1) % n], v))
            })
        };
        for &v in &interiors {
            if on_boundary_edge(v) {
                continue;
            }
            let uv = chart.project(mesh.verts[v as usize]);
            let mid = 0.5 * (theta_min + theta_max);
            let k = ((mid - uv.x()) / std::f64::consts::TAU).round();
            let theta = uv.x() + k * std::f64::consts::TAU;
            if theta < theta_min || theta > theta_max {
                return Err(ConstructError::ThetaUnwrap { patch: patch_index });
            }
            interior_idx.push(pool.len() as u32);
            pool.push(cad_primitives::Point2::new(theta, uv.y()));
            interior_back.push(v);
        }
    }

    let old: Vec<[u32; 3]> = patch.tris.iter().map(|&t| mesh.tris[t as usize]).collect();

    // I2d budget — certify the OLD triangles once (cylinder only): the max
    // certified d(T) over the triangles this rebuild replaces, plus the
    // patch's own θ-arc sampling scale (the I2e seed-spacing basis).
    // Projection-based uv with span containment — old-triangle vertices
    // include the boundary-edge-filtered ones that are not in `pool`.
    let mut old_max: Option<f64> = None;
    let mut old_arc_span = 0.0f64;
    let radius = match chart {
        SurfaceChart::Cylinder { radius, .. } => radius,
        // I13a: the θ↔arc-length reference for seed spacing / arc spans. The
        // patch's LARGEST station gives the largest local radius — seeds are
        // then never sparser than `spacing` anywhere on the patch.
        SurfaceChart::Cone { tan_half, .. } => {
            p2.verts.iter().fold(0.0f64, |m, p| m.max(p.y())) * tan_half
        }
        SurfaceChart::Plane { .. } => 0.0,
        // inc-2c-3b-3: unreachable today — the wholesale path charts via
        // `SurfaceChart::new`, which never builds a Torus (a whole patch may
        // wrap a period; only fan-local holders use `new_local`). The value
        // is the honest θ↔arc reference all the same: the outer equator.
        SurfaceChart::Torus { major, minor, .. } => major + minor,
    };
    if let Some((theta_min, theta_max)) = theta_span {
        let uv_of = |v: u32| -> Result<Point2, ConstructError> {
            let uv = chart.project(mesh.verts[v as usize]);
            let mid = 0.5 * (theta_min + theta_max);
            let k = ((mid - uv.x()) / std::f64::consts::TAU).round();
            let theta = uv.x() + k * std::f64::consts::TAU;
            if theta < theta_min || theta > theta_max {
                return Err(ConstructError::ChordCertify { patch: patch_index });
            }
            Ok(Point2::new(theta, uv.y()))
        };
        let mut m = 0.0f64;
        for t in &old {
            let uv = [uv_of(t[0])?, uv_of(t[1])?, uv_of(t[2])?];
            let dt = crate::stage4_dt::d_of_t(&patch.surface, uv)
                .map_err(|_| ConstructError::ChordCertify { patch: patch_index })?;
            m = m.max(dt);
            let (lo, hi) = uv
                .iter()
                .fold((f64::INFINITY, f64::NEG_INFINITY), |(lo, hi), p| {
                    (lo.min(p.x()), hi.max(p.x()))
                });
            old_arc_span = old_arc_span.max((hi - lo) * radius);
        }
        old_max = Some(m);
    }

    // One CDT attempt over `pool` extended with `seeds` (chart points, I2e).
    // Attempt 0 runs seedless — the pre-I2e path, byte-identical when it
    // passes the gate.
    let n_pool = pool.len();
    let run_attempt = |seeds: &[Point2]| -> Result<(Vec<[u32; 3]>, Vec<Point3>), ConstructError> {
        let mut pool_ext = pool.clone();
        let mut interior_ext = interior_idx.clone();
        for s in seeds {
            interior_ext.push(pool_ext.len() as u32);
            pool_ext.push(*s);
        }
        let tris2 =
            cdt_with_interior_constraints(&pool_ext, &p2.boundary, &p2.holes, &interior_ext, &[])
                .map_err(|error| ConstructError::Cdt {
                patch: patch_index,
                error,
            })?;

        // 3D positions for pool indices: boundary and carried interiors are
        // existing mesh vertices; seeds lift through the chart — exactly
        // on-surface, and `lift` is 2π-periodic so the unwrap shift is a
        // world-space no-op.
        let pos3 = |i: u32| -> Point3 {
            let iu = i as usize;
            if iu < back.len() {
                mesh.verts[back[iu] as usize]
            } else if iu < n_pool {
                mesh.verts[interior_back[iu - back.len()] as usize]
            } else {
                chart.lift(pool_ext[iu])
            }
        };
        // Match the ORIGINAL patch's outward sense by measurement — the
        // chart basis has arbitrary handedness relative to the surface
        // normal.
        let mesh_pos = |v: u32| -> Point3 { mesh.verts[v as usize] };
        let want = area_vector(&old, &mesh_pos);
        let got = area_vector(&tris2, &pos3);
        let d = dot3(want, got);
        if d == 0.0 || !d.is_finite() {
            return Err(ConstructError::DegenerateOrientation { patch: patch_index });
        }
        let mut tris2 = tris2;
        if d < 0.0 {
            for t in &mut tris2 {
                t.swap(1, 2);
            }
        }

        // I2d gate — Yang §4.4.1's closing sentence ("we recalculate d(T)
        // to maintain controllable error",
        // refs/text/yang2025_hybrid_boolean.txt:568-571). Certified like for
        // like, in the CDT's own frame: the pool coordinates ARE the
        // unwrapped parametrization the new triangles were built in. The
        // rebuild is accepted only if its certified max d(T) does not
        // exceed the certified max of the triangles it replaces — the
        // patch's own pre-rebuild bound as the budget, no external
        // constant. Planar patches are exempt by identity (d(T) ≡ 0 for
        // any triangulation of a plane polygon).
        if let Some(budget) = old_max {
            let mut new_max = 0.0f64;
            for t in &tris2 {
                let uv = [
                    pool_ext[t[0] as usize],
                    pool_ext[t[1] as usize],
                    pool_ext[t[2] as usize],
                ];
                let dt = crate::stage4_dt::d_of_t(&patch.surface, uv)
                    .map_err(|_| ConstructError::ChordCertify { patch: patch_index })?;
                new_max = new_max.max(dt);
            }
            if new_max > budget {
                return Err(ConstructError::ChordDegradation {
                    patch: patch_index,
                    old_max: budget,
                    new_max,
                });
            }
        }

        // Back into mesh index space; a seed index lands at plan_verts + k
        // and `apply_rebuild_batch` remaps it onto the appended block.
        let plan_verts = mesh.verts.len() as u32;
        let at = |i: u32| -> u32 {
            let iu = i as usize;
            if iu < back.len() {
                back[iu]
            } else if iu < n_pool {
                interior_back[iu - back.len()]
            } else {
                plan_verts + (i - n_pool as u32)
            }
        };
        let new_tris: Vec<[u32; 3]> = tris2
            .iter()
            .map(|t| [at(t[0]), at(t[1]), at(t[2])])
            .collect();
        let new_verts: Vec<Point3> = seeds.iter().map(|&s| chart.lift(s)).collect();
        Ok((new_tris, new_verts))
    };

    let (new_tris, new_verts) = match run_attempt(&[]) {
        Ok(out) => out,
        // I2e — a seedless curved rebuild that certifies coarser than what
        // it replaces is retried with a deterministic interior seed grid at
        // the patch's own pre-rebuild θ-arc sampling scale (halved once on
        // a second failure). The banding a chart CDT cannot reproduce from
        // cycle constraints alone is re-established as interior sampling —
        // the §4.1.2/§4.3.4 tessellation discipline — and the I2d gate
        // re-verifies every attempt; a rescue is never taken on faith.
        Err(ConstructError::ChordDegradation {
            patch: ep,
            old_max: eo,
            new_max: en,
        }) if old_arc_span > 0.0 => {
            let mut rescued = None;
            let mut spacing = old_arc_span;
            for _ in 0..2 {
                let seeds = i2e_seed_grid(&pool, &p2.boundary, &p2.holes, radius, spacing);
                if !seeds.is_empty() {
                    match run_attempt(&seeds) {
                        Ok(out) => {
                            if crate::stage5_topology::c441_verbose() {
                                eprintln!(
                                    "[s4-construct] I2E SEEDED patch {patch_index}: {} seeds \
                                     at arc spacing {spacing:.3e}",
                                    out.1.len()
                                );
                            }
                            rescued = Some(out);
                            break;
                        }
                        Err(ConstructError::ChordDegradation { .. }) => {}
                        Err(e) => return Err(e),
                    }
                } else if crate::stage5_topology::c441_verbose() {
                    eprintln!(
                        "[s4-construct] I2E patch {patch_index}: empty seed grid at arc \
                         spacing {spacing:.3e} (degenerate span or seed cap)"
                    );
                }
                spacing *= 0.5;
            }
            match rescued {
                Some(out) => out,
                None => {
                    return Err(ConstructError::ChordDegradation {
                        patch: ep,
                        old_max: eo,
                        new_max: en,
                    })
                }
            }
        }
        Err(e) => return Err(e),
    };

    let kept: BTreeSet<u32> = new_tris.iter().flatten().copied().collect();
    let dropped: BTreeSet<u32> = old
        .iter()
        .flatten()
        .copied()
        .filter(|v| !kept.contains(v))
        .collect();

    Ok(PatchRebuild {
        patch: patch_index,
        old_tris: patch.tris.clone(),
        new_tris,
        new_verts,
        dropped,
        plan_verts: mesh.verts.len() as u32,
        plan_tris: mesh.tris.len() as u32,
    })
}

/// I2e seed grid: deterministic interior points at `spacing` (arc-length
/// units) over the unwrapped chart polygon — strictly inside the outer
/// boundary, outside every hole, and at least `0.25·spacing` (arc metric)
/// clear of every boundary edge. A seed ON a constraint would make spade
/// split it one-sidedly (the F0059 hazard); the clearance choice is
/// quality-only — it decides which OPTIONAL seeds to insert and cannot make
/// an accepted rebuild wrong, because the d(T) gate re-verifies every
/// attempt. Returns empty (→ the caller keeps the loud decline) for
/// degenerate spans or when the grid would exceed the runaway backstop.
fn i2e_seed_grid(
    pool: &[Point2],
    outer: &[u32],
    holes: &[Vec<u32>],
    radius: f64,
    spacing: f64,
) -> Vec<Point2> {
    const MAX_SEEDS: usize = 4096;
    if !spacing.is_finite() || spacing <= 0.0 || !radius.is_finite() || radius <= 0.0 {
        return Vec::new();
    }
    let pt = |i: u32| pool[i as usize];
    let (mut tlo, mut thi, mut zlo, mut zhi) = (
        f64::INFINITY,
        f64::NEG_INFINITY,
        f64::INFINITY,
        f64::NEG_INFINITY,
    );
    for &i in outer {
        let p = pt(i);
        tlo = tlo.min(p.x());
        thi = thi.max(p.x());
        zlo = zlo.min(p.y());
        zhi = zhi.max(p.y());
    }
    let s_theta = spacing / radius;
    let nt = ((thi - tlo) / s_theta).floor();
    let nz = ((zhi - zlo) / spacing).floor();
    if !nt.is_finite() || !nz.is_finite() || nt * nz > 4.0 * MAX_SEEDS as f64 {
        return Vec::new();
    }
    let inside = |p: Point2, ring: &[u32]| -> bool {
        let n = ring.len();
        let mut inside = false;
        for i in 0..n {
            let a = pt(ring[i]);
            let b = pt(ring[(i + 1) % n]);
            if (a.y() > p.y()) != (b.y() > p.y()) {
                let x = a.x() + (p.y() - a.y()) / (b.y() - a.y()) * (b.x() - a.x());
                if p.x() < x {
                    inside = !inside;
                }
            }
        }
        inside
    };
    let clear_of = |p: Point2, ring: &[u32]| -> bool {
        let n = ring.len();
        let (px, py) = (p.x() * radius, p.y());
        let min_d2 = (0.25 * spacing) * (0.25 * spacing);
        for i in 0..n {
            let a = pt(ring[i]);
            let b = pt(ring[(i + 1) % n]);
            let (ax, ay) = (a.x() * radius, a.y());
            let (bx, by) = (b.x() * radius, b.y());
            let (dx, dy) = (bx - ax, by - ay);
            let l2 = dx * dx + dy * dy;
            let t = if l2 > 0.0 {
                (((px - ax) * dx + (py - ay) * dy) / l2).clamp(0.0, 1.0)
            } else {
                0.0
            };
            let (qx, qy) = (ax + t * dx, ay + t * dy);
            let d2 = (px - qx) * (px - qx) + (py - qy) * (py - qy);
            if d2 < min_d2 {
                return false;
            }
        }
        true
    };
    let mut seeds = Vec::new();
    let mut theta = tlo + s_theta;
    while theta < thi {
        let mut z = zlo + spacing;
        while z < zhi {
            let p = Point2::new(theta, z);
            if inside(p, outer)
                && holes.iter().all(|h| !inside(p, h))
                && clear_of(p, outer)
                && holes.iter().all(|h| clear_of(p, h))
            {
                if seeds.len() >= MAX_SEEDS {
                    return Vec::new();
                }
                seeds.push(p);
            }
            z += spacing;
        }
        theta += s_theta;
    }
    seeds
}

/// Chain a victim's link edges into every maximal run — the general form of the
/// single-run walk [`rebuild_merge_fan`] performs. With in- and out-degree both
/// capped at 1 the link graph is a disjoint union of paths and cycles, so the
/// paths (each from an in-degree-0 source) and then the leftover cycles cover it
/// exactly. Deterministic: `BTreeMap` order throughout.
fn chain_link_runs(next: &BTreeMap<u32, u32>, indeg: &BTreeMap<u32, usize>) -> Vec<Vec<u32>> {
    fn walk(next: &BTreeMap<u32, u32>, seen: &mut BTreeSet<u32>, start: u32) -> Vec<u32> {
        let mut run = vec![start];
        seen.insert(start);
        let mut cur = start;
        while let Some(&nx) = next.get(&cur) {
            if !seen.insert(nx) {
                break;
            }
            run.push(nx);
            cur = nx;
        }
        run
    }
    let mut seen: BTreeSet<u32> = BTreeSet::new();
    let mut runs: Vec<Vec<u32>> = Vec::new();
    for (&v, &d) in indeg {
        if d == 0 && !seen.contains(&v) {
            runs.push(walk(next, &mut seen, v));
        }
    }
    for &v in indeg.keys() {
        if !seen.contains(&v) {
            runs.push(walk(next, &mut seen, v));
        }
    }
    runs
}

/// Yang §4.4.1 **Fig-11(b)→(c)**, LOCALLY: merge `victim` into `survivor` by
/// re-triangulating exactly the triangles of `patch` incident to `victim`.
///
/// # Why local rather than whole-patch
///
/// [`rebuild_patch_planar`] re-CDTs a patch's ENTIRE boundary, which imposes two
/// requirements the merge does not need and often cannot meet (measured
/// 2026-08-19 over the ring-reject family):
/// * the patch must not encircle the cylinder axis (`unwrap_theta` refuses) —
///   F0045 and R0090 both merge on the rim of a lateral that wraps all the way
///   around, so the whole-patch rebuild declines `ThetaUnwrap`;
/// * the patch's whole boundary must already be simple — but a patch generally
///   carries SEVERAL folds, so the CDT of the full cycle refuses
///   (`TriangulationFailed`) until every one of them is repaired, which no
///   single merge can achieve (R0074, R0085).
///
/// The fan has neither problem: it spans a small θ window (unwrapped against the
/// victim's own branch, so no global span exists to fall outside of) and its
/// polygon is local, so one fold's repair never waits on another's.
///
/// It is emphatically NOT the bare `collapse_vertex` the 2026-08-05 trial
/// measured negative: the triangles around the victim are DISCARDED and rebuilt
/// by CDT, so no fan is left inconsistent by an index rewrite.
///
/// # Construction
///
/// The fan triangles are `victim → l_i → l_{i+1}`, so their opposite directed
/// edges chain into the victim's LINK. Removing the victim leaves exactly the
/// link polygon `l_0 … l_k` — closed by `l_k → l_0` when the victim is on the
/// patch boundary (the new boundary edge the merge creates), or already closed
/// when the victim is interior. The survivor must be a link vertex; the merge is
/// then precisely "the boundary now runs to the survivor instead of the victim".
/// The polygon is triangulated in the patch's chart and orientation-matched to
/// the triangles it replaces, exactly as the whole-patch rebuild does.
///
/// `fan_of_one` enables the §4-I8 degenerate case (the victim has a SINGLE
/// triangle here, so the merge deletes it rather than re-triangulating a
/// 2-vertex link); it is the caller's gate (always-on in the driver since
/// 2026-08-20), passed explicitly rather than read from the environment so the
/// tests can exercise both settings.
pub(crate) fn rebuild_merge_fan(
    mesh: &Mesh,
    patch_index: usize,
    patch: &SplicePatch,
    victim: u32,
    survivor: u32,
    fan_of_one: bool,
) -> Result<PatchRebuild, ConstructError> {
    let chart = SurfaceChart::new(patch.surface)
        .ok_or(ConstructError::NonPlanarPatch { patch: patch_index })?;

    // Fan triangles, and each one's opposite directed edge (the link edge).
    let mut old_tris: Vec<u32> = Vec::new();
    let mut link_edges: Vec<(u32, u32)> = Vec::new();
    for &t in &patch.tris {
        let tri = mesh.tris[t as usize];
        let Some(k) = tri.iter().position(|&v| v == victim) else {
            continue;
        };
        old_tris.push(t);
        let (x, y) = (tri[(k + 1) % 3], tri[(k + 2) % 3]);
        if x == victim || y == victim || x == y {
            return Err(ConstructError::FanNotSimple {
                patch: patch_index,
                victim,
                reason: FanReason::Degenerate,
            });
        }
        link_edges.push((x, y));
    }
    if old_tris.is_empty() {
        return Err(ConstructError::MalformedPatch { patch: patch_index });
    }

    // Chain the directed link edges into ONE run. A vertex appearing as the
    // source (or target) of two edges is a pinch — loud, never guessed.
    let fan = old_tris.len();
    let mut next: BTreeMap<u32, u32> = BTreeMap::new();
    let mut indeg: BTreeMap<u32, usize> = BTreeMap::new();
    for &(x, y) in &link_edges {
        if next.insert(x, y).is_some() {
            return Err(ConstructError::FanNotSimple {
                patch: patch_index,
                victim,
                reason: FanReason::Pinch { fan },
            });
        }
        *indeg.entry(y).or_default() += 1;
        indeg.entry(x).or_default();
    }
    if indeg.values().any(|&d| d > 1) {
        return Err(ConstructError::FanNotSimple {
            patch: patch_index,
            victim,
            reason: FanReason::Pinch { fan },
        });
    }
    // Start at the run's source (boundary victim) or anywhere (interior victim,
    // where the link is a closed cycle).
    let start = indeg
        .iter()
        .find(|&(_, &d)| d == 0)
        .map(|(&v, _)| v)
        .unwrap_or_else(|| link_edges[0].0);
    let mut link: Vec<u32> = vec![start];
    let mut cur = start;
    while let Some(&nx) = next.get(&cur) {
        if nx == start {
            break; // closed link (interior victim)
        }
        if link.len() > link_edges.len() {
            return Err(ConstructError::FanNotSimple {
                patch: patch_index,
                victim,
                reason: FanReason::Pinch { fan },
            });
        }
        link.push(nx);
        cur = nx;
    }
    if link.len() != indeg.len() {
        // Not every link vertex was reached: the fan is split into several runs.
        // Enumerate them all so the decline reports HOW MANY sheets meet at the
        // victim and which of them the survivor is on — the two numbers that
        // decide whether a multi-run repair is even well-posed.
        let runs = chain_link_runs(&next, &indeg);
        let with_survivor = runs.iter().filter(|r| r.contains(&survivor)).count();
        return Err(ConstructError::FanNotSimple {
            patch: patch_index,
            victim,
            reason: FanReason::Split {
                fan,
                runs: runs.len(),
                with_survivor,
            },
        });
    }
    if !link.contains(&survivor) {
        return Err(ConstructError::FanSurvivorNotAdjacent {
            patch: patch_index,
            victim,
        });
    }
    if link.len() == 2 && fan == 1 && fan_of_one {
        // FAN OF ONE. The victim is a corner of exactly ONE triangle here, so
        // its link is the single edge `(x, y)` — no polygon to re-triangulate.
        // That is not a refusal, it is the ANSWER: the merge rewrites the
        // triangle `(victim, x, y)` as `(survivor, x, y)` with `survivor` one of
        // `x`/`y` (checked above), which is degenerate, so the triangle is
        // simply removed. The patch's boundary walk `… y → victim → x …`
        // becomes `… y → x …`, exactly one edge, which is what the merge means
        // — and the same edge every OTHER holder's re-CDT produces, so the
        // batch stays conformal.
        //
        // Measured 2026-08-20 over the ring-reject family: `Short { fan: 1,
        // link: 2 }` was the decline on EVERY Fig-11 site that reached its
        // repair (R0011 v828, R0074 v127, R0085 v316) — the fan-of-one is the
        // family's dominant configuration, not an exotic one. (The spec's
        // earlier reading of these declines as a "pinched victim" was an
        // inference; the reason discriminator measured them.)
        if crate::stage5_topology::c441_verbose() {
            eprintln!(
                "[s4-construct] FAN-OF-ONE patch {patch_index}: victim v{victim} -> v{survivor}, \
                 dropping tri {} of the patch's {} ({} link)",
                old_tris[0],
                patch.tris.len(),
                link.len(),
            );
        }
        return Ok(PatchRebuild {
            patch: patch_index,
            old_tris,
            new_tris: Vec::new(),
            new_verts: Vec::new(),
            dropped: [victim].into_iter().collect(),
            plan_verts: mesh.verts.len() as u32,
            plan_tris: mesh.tris.len() as u32,
        });
    }
    if link.len() < 3 {
        // A 2-vertex link with TWO triangles is a doubled pair over the same
        // link edge — a distinct configuration this has never measured. Loud,
        // never guessed.
        return Err(ConstructError::FanNotSimple {
            patch: patch_index,
            victim,
            reason: FanReason::Short {
                fan,
                link: link.len(),
            },
        });
    }

    // Chart coordinates, θ-unwrapped against the VICTIM's own branch: the fan is
    // local, so every link vertex is within π of it and the branch is unique.
    let base = chart.project(mesh.verts[victim as usize]);
    let periodic = matches!(
        patch.surface,
        Surface::Cylinder { .. } | Surface::Cone { .. }
    );
    // I13a apex guard, fan form: the cone chart is injective only strictly
    // beyond the apex — refuse a fan any of whose vertices sits at or behind
    // the apex station (same contract as the whole-patch rebuild).
    if matches!(chart, SurfaceChart::Cone { .. })
        && std::iter::once(victim)
            .chain(link.iter().copied())
            .any(|v| chart.project(mesh.verts[v as usize]).y() <= 0.0)
    {
        return Err(ConstructError::ApexInPatch { patch: patch_index });
    }
    let uv_of = |v: u32| -> Point2 {
        let uv = chart.project(mesh.verts[v as usize]);
        if periodic {
            let k = ((base.x() - uv.x()) / std::f64::consts::TAU).round();
            Point2::new(uv.x() + k * std::f64::consts::TAU, uv.y())
        } else {
            uv
        }
    };
    let pool: Vec<Point2> = link.iter().map(|&v| uv_of(v)).collect();
    let boundary: Vec<u32> = (0..link.len() as u32).collect();
    let tris2 =
        cdt_with_interior_constraints(&pool, &boundary, &[], &[], &[]).map_err(|error| {
            if std::env::var_os("YANG_441_FAN_PROBE").is_some() {
                let poly: Vec<(u32, f64, f64)> = link
                    .iter()
                    .zip(pool.iter())
                    .map(|(&v, p)| (v, p.x(), p.y()))
                    .collect();
                eprintln!(
                    "[i13d-fan] CDT-DECLINE patch={patch_index} victim=v{victim} \
                     survivor=v{survivor} err={error:?} link_poly={poly:?} \
                     victim_uv={:?}",
                    (base.x(), base.y()),
                );
            }
            ConstructError::Cdt {
                patch: patch_index,
                error,
            }
        })?;

    // Orientation: match the fan this replaces, measured (the chart basis has
    // arbitrary handedness relative to the surface normal).
    let old: Vec<[u32; 3]> = old_tris.iter().map(|&t| mesh.tris[t as usize]).collect();
    let mesh_pos = |v: u32| -> Point3 { mesh.verts[v as usize] };
    let pos3 = |i: u32| -> Point3 { mesh.verts[link[i as usize] as usize] };
    let want = crate::stage4_splice::area_vector(&old, &mesh_pos);
    let got = crate::stage4_splice::area_vector(&tris2, &pos3);
    let d = crate::stage4_splice::dot3(want, got);
    if d == 0.0 || !d.is_finite() {
        return Err(ConstructError::DegenerateOrientation { patch: patch_index });
    }
    let mut tris2 = tris2;
    if d < 0.0 {
        for t in &mut tris2 {
            t.swap(1, 2);
        }
    }
    let new_tris: Vec<[u32; 3]> = tris2
        .iter()
        .map(|t| {
            [
                link[t[0] as usize],
                link[t[1] as usize],
                link[t[2] as usize],
            ]
        })
        .collect();

    Ok(PatchRebuild {
        patch: patch_index,
        old_tris,
        new_tris,
        new_verts: Vec::new(),
        dropped: [victim].into_iter().collect(),
        plan_verts: mesh.verts.len() as u32,
        plan_tris: mesh.tris.len() as u32,
    })
}

/// I13d — the run-level analog of [`rebuild_merge_fan`]: re-triangulate the
/// union of SEVERAL victims' fans in one rebuild, absorbing them all into
/// the junction survivor.
///
/// The single-victim fan is structurally unable to repair a multi-vertex
/// overrun run: each victim's link polygon still contains its still-folded
/// run sibling, so every per-victim CDT is refused (measured 2026-08-25,
/// R0003 — the wall patch declines each of the six single sites with
/// `TriangulationFailed`). The REGION's outer link contains no victim at
/// all, so that refusal cannot recur by construction.
///
/// Construction: region = every patch triangle touching any victim; its
/// boundary = directed edges whose reverse is absent from the region; the
/// LINK = boundary edges with no victim endpoint, chained into ONE open run.
/// The victims sit on the patch boundary, so the link is open and its two
/// endpoints are where the region meets the boundary chain. The polygon is
/// the link closed by (end → start), whose closure edge becomes the NEW
/// boundary segment — which equals the absorbed chain `survivor → far
/// neighbour` exactly when the SURVIVOR is a link endpoint. A survivor
/// interior to the link (a boundary ear extending the region past the
/// junction) would be stranded off the new boundary by the closure, so that
/// configuration is refused loudly, never guessed.
pub(crate) fn rebuild_run_fan(
    mesh: &Mesh,
    patch_index: usize,
    patch: &SplicePatch,
    victims: &BTreeSet<u32>,
    survivor: u32,
) -> Result<PatchRebuild, ConstructError> {
    fan_rebuild_core(mesh, patch_index, patch, victims, survivor, None)
}

/// §I13(f) f2 — the re-homing variant of [`rebuild_run_fan`]: the survivor
/// is evaluated at `survivor_pos` (its post-surgery mint, applied by the
/// caller only if the whole batch plans), and it may JOIN a patch it has no
/// triangle on. On the re-homed corner's neighbor-band patch the old corner
/// vertex is absent from the victim's link entirely; the true topology puts
/// the minted corner exactly where the victim arc was, so the closure
/// appends the survivor to the link polygon between the chain's two ends.
/// A survivor present but MID-link keeps the [`rebuild_run_fan`] refusal —
/// that shape is a genuine pinch, not a join.
pub(crate) fn rebuild_rehome_fan(
    mesh: &Mesh,
    patch_index: usize,
    patch: &SplicePatch,
    victims: &BTreeSet<u32>,
    survivor: u32,
    survivor_pos: Point3,
) -> Result<PatchRebuild, ConstructError> {
    fan_rebuild_core(
        mesh,
        patch_index,
        patch,
        victims,
        survivor,
        Some(survivor_pos),
    )
}

/// §I13(f) f2c — delete a BOUNDARY vertex's whole fan from one patch: the
/// generalized empty rebuild. The victim's fan region (every patch
/// triangle touching it) is removed with NO replacement; its link — which
/// must be a single simple OPEN chain between the victim's two patch-cycle
/// neighbors — becomes the patch boundary. This is the S_i-side repair of
/// the inverted-junction-pair corner: the phantom's fan is the fossil
/// sliver overhanging the band rim, and the link IS the already-existing
/// rim chain (measured, f2c precondition census 2026-08-28). Returns the
/// rebuild and the ordered link chain for the caller's certification
/// (its ends must be the two true corners).
///
/// A closed link (interior vertex — deleting would punch a hole), a
/// pinch, a multi-run link, or any degeneracy declines typed. The victim
/// vertex itself is NOT dropped — it lives on in other patches (the f2c
/// relocation moves it).
pub(crate) fn delete_boundary_fan(
    mesh: &Mesh,
    patch_index: usize,
    patch: &SplicePatch,
    victim: u32,
) -> Result<(PatchRebuild, Vec<u32>), ConstructError> {
    delete_boundary_fan_set(mesh, patch_index, patch, &BTreeSet::from([victim]))
}

/// §4.5.1 inc-2c-3b-2 (spec `specs/yang_451_corner_transit.md` §3j) — the
/// set generalization of [`delete_boundary_fan`]: delete the joint fan
/// REGION of several boundary vertices (a corridor's phantom plus its
/// absorbed chain-end anchors, which are mesh-adjacent by construction).
/// The link — the region's boundary edges touching no victim — must chain
/// as one simple OPEN run; every degeneracy declines typed exactly as the
/// single-victim form does.
pub(crate) fn delete_boundary_fan_set(
    mesh: &Mesh,
    patch_index: usize,
    patch: &SplicePatch,
    victims: &BTreeSet<u32>,
) -> Result<(PatchRebuild, Vec<u32>), ConstructError> {
    let victim = victims.iter().next().copied().unwrap_or(u32::MAX);
    let (rebuild, mut runs, fan) = delete_boundary_fan_runs(mesh, patch_index, patch, victims)?;
    if runs.len() != 1 {
        // The historical single-run contract: a multi-run link keeps the
        // Closed refusal shape (the walk cannot cover the rim in one run).
        return Err(ConstructError::FanNotSimple {
            patch: patch_index,
            victim,
            reason: FanReason::Closed { fan },
        });
    }
    Ok((rebuild, runs.remove(0)))
}

/// §4.5.1 inc-2c-3b-6 — the MULTI-RUN generalization: a JOINT region
/// (adjacent corridors' overshoot fans meeting on one patch, the measured
/// R0044 v76+v75 anatomy) touches the patch boundary in several stretches,
/// so its link decomposes into several open chains. Returns every run (the
/// caller stitches them with corrected-cycle arcs). A closed link loop
/// (interior region) or any pinch still refuses typed.
pub(crate) fn delete_boundary_fan_runs(
    mesh: &Mesh,
    patch_index: usize,
    patch: &SplicePatch,
    victims: &BTreeSet<u32>,
) -> Result<(PatchRebuild, Vec<Vec<u32>>, usize), ConstructError> {
    let victim = victims.iter().next().copied().unwrap_or(u32::MAX);
    let mut old_tris: Vec<u32> = Vec::new();
    let mut directed: BTreeSet<(u32, u32)> = BTreeSet::new();
    for &t in &patch.tris {
        let tri = mesh.tris[t as usize];
        if !tri.iter().any(|v| victims.contains(v)) {
            continue;
        }
        old_tris.push(t);
        for k in 0..3 {
            let (x, y) = (tri[k], tri[(k + 1) % 3]);
            if x == y || !directed.insert((x, y)) {
                return Err(ConstructError::FanNotSimple {
                    patch: patch_index,
                    victim,
                    reason: FanReason::Degenerate,
                });
            }
        }
    }
    if old_tris.is_empty() {
        return Err(ConstructError::MalformedPatch { patch: patch_index });
    }
    let fan = old_tris.len();
    // The link: region-boundary edges not touching a victim, chained.
    let mut next: BTreeMap<u32, u32> = BTreeMap::new();
    let mut has_pred: BTreeSet<u32> = BTreeSet::new();
    for &(x, y) in &directed {
        if directed.contains(&(y, x)) || victims.contains(&x) || victims.contains(&y) {
            continue;
        }
        if next.insert(x, y).is_some() {
            return Err(ConstructError::FanNotSimple {
                patch: patch_index,
                victim,
                reason: FanReason::Pinch { fan },
            });
        }
        has_pred.insert(y);
    }
    let starts: Vec<u32> = next
        .keys()
        .filter(|x| !has_pred.contains(x))
        .copied()
        .collect();
    if starts.is_empty() {
        // No chain start anywhere: the link is closed — an interior region.
        return Err(ConstructError::FanNotSimple {
            patch: patch_index,
            victim,
            reason: FanReason::Closed { fan },
        });
    }
    let mut runs: Vec<Vec<u32>> = Vec::new();
    let mut covered = 0usize;
    for start in starts {
        let mut link: Vec<u32> = vec![start];
        let mut cur = start;
        while let Some(&n) = next.get(&cur) {
            if link.contains(&n) {
                return Err(ConstructError::FanNotSimple {
                    patch: patch_index,
                    victim,
                    reason: FanReason::Pinch { fan },
                });
            }
            link.push(n);
            cur = n;
        }
        covered += link.len() - 1;
        runs.push(link);
    }
    if covered != next.len() {
        // Leftover edges form a closed loop alongside the open runs.
        return Err(ConstructError::FanNotSimple {
            patch: patch_index,
            victim,
            reason: FanReason::Closed { fan },
        });
    }
    Ok((
        PatchRebuild {
            patch: patch_index,
            old_tris,
            new_tris: Vec::new(),
            new_verts: Vec::new(),
            dropped: BTreeSet::new(),
            plan_verts: mesh.verts.len() as u32,
            plan_tris: mesh.tris.len() as u32,
        },
        runs,
        fan,
    ))
}

/// §I13(f) f2c — seam-insert an EXISTING mesh vertex into a patch's
/// boundary edge: the unique patch triangle carrying edge {x, y} splits
/// into two children sharing the inserted vertex, winding preserved. This
/// is the S_j-side repair (both fragments): the moved phantom lands ON
/// the fragment's boundary conic between two of its chain vertices, and
/// without the split the neighbor patch's new corner is a T-junction.
///
/// `insert_pos` is the vertex's POST-relocation position (the mint) — the
/// orientation guard evaluates the children against it, because the batch
/// write applies rebuilds first and the relocation with them. The vertex
/// must not already be on the triangle; the edge must have exactly one
/// incident patch triangle (a true boundary edge).
pub(crate) fn split_boundary_edge(
    mesh: &Mesh,
    patch_index: usize,
    patch: &SplicePatch,
    x: u32,
    y: u32,
    insert_v: u32,
    insert_pos: Point3,
) -> Result<PatchRebuild, ConstructError> {
    let hits: Vec<u32> = patch
        .tris
        .iter()
        .copied()
        .filter(|&t| {
            let tri = mesh.tris[t as usize];
            tri.contains(&x) && tri.contains(&y)
        })
        .collect();
    let (t, tri) = match hits[..] {
        [t] => (t, mesh.tris[t as usize]),
        _ => {
            return Err(ConstructError::EdgeNotBoundary {
                patch: patch_index,
                edge: (x.min(y), x.max(y)),
                incident: hits.len(),
            })
        }
    };
    if tri.contains(&insert_v) || x == y || insert_v == x || insert_v == y {
        return Err(ConstructError::EdgeNotBoundary {
            patch: patch_index,
            edge: (x.min(y), x.max(y)),
            incident: hits.len(),
        });
    }
    let k = (0..3)
        .find(|&k| {
            let (p, q) = (tri[k], tri[(k + 1) % 3]);
            (p == x && q == y) || (p == y && q == x)
        })
        .expect("both endpoints on the triangle imply an edge");
    let (p, q, a) = (tri[k], tri[(k + 1) % 3], tri[(k + 2) % 3]);
    let children = [[p, insert_v, a], [insert_v, q, a]];
    // Orientation guard at the post-relocation position: both children
    // must keep the parent's sense (sign-only, no band).
    let at = |v: u32| -> Point3 {
        if v == insert_v {
            insert_pos
        } else {
            mesh.verts[v as usize]
        }
    };
    let parent_av = crate::stage4_splice::area_vector(&[tri], &|v: u32| at(v));
    for child in &children {
        let av = crate::stage4_splice::area_vector(&[*child], &|v: u32| at(v));
        let d = crate::stage4_splice::dot3(parent_av, av);
        if !(d.is_finite() && d > 0.0) {
            return Err(ConstructError::SplitFlip { patch: patch_index });
        }
    }
    Ok(PatchRebuild {
        patch: patch_index,
        old_tris: vec![t],
        new_tris: children.to_vec(),
        new_verts: Vec::new(),
        dropped: BTreeSet::new(),
        plan_verts: mesh.verts.len() as u32,
        plan_tris: mesh.tris.len() as u32,
    })
}

/// §I13(f) f2c-2 — the S_i-side HOLE RE-FILL: seedless CDT of the fossil
/// fan's link polygon MINUS its dropped-end corner, in the patch's chart.
/// The phantom is NOT on the polygon — the re-homed corner leaves the band
/// patch entirely (its true carriers are {S_j, W, K}), and the dropped-end
/// corner tucks below the rim onto the neighbor-band side (the measured
/// chord-overshoot anatomy, census-15 2026-08-28: keeping BOTH corners on
/// the band boundary needs the window edge twice — the fossil's own
/// stacked-chord slit — so exactly one corner stays, deterministically the
/// link-walk's END).
///
/// `polygon` is the cyclic boundary in mesh ids (the link minus its first
/// vertex); `reference` the old fan triangles being replaced (orientation
/// and — on curved charts — the like-for-like d(T) budget, Yang §4.4.1's
/// closing sentence). Returns oriented replacement triangles in mesh ids.
/// Every failure is a typed refusal; nothing is legalized.
pub(crate) fn refill_fan_hole(
    mesh: &Mesh,
    patch_index: usize,
    patch: &SplicePatch,
    polygon: &[u32],
    reference: &[u32],
) -> Result<Vec<[u32; 3]>, ConstructError> {
    let (tris, seeds) =
        refill_fan_hole_seeded(mesh, patch_index, patch, polygon, reference, 0, u32::MAX)?;
    debug_assert!(seeds.is_empty(), "seed budget 0 mints nothing");
    Ok(tris)
}

/// inc-2c-3b-6 — the SEEDED refill: when the boundary-only fill certifies
/// coarser than the fossil (the like-for-like d(T) budget), insert interior
/// Steiner vertices — the worst triangle's chart centroid, LIFTED exactly
/// onto the analytic surface — and re-CDT, up to `seed_budget` points. The
/// budget is the fossil's own interior vertex spend (the caller passes its
/// victim count): like-for-like in density as well as in d(T) — §4.4.1's
/// mesh update inserts vertices; it never legalizes a coarser fill. Seed
/// ids in the returned triangles start at `polygon.len()` and map to the
/// returned positions in order; exhausting the budget refuses
/// `ChordDegradation` with the last certified value (loud, as before).
pub(crate) fn refill_fan_hole_seeded(
    mesh: &Mesh,
    patch_index: usize,
    patch: &SplicePatch,
    polygon: &[u32],
    reference: &[u32],
    seed_budget: usize,
    seed_id_base: u32,
) -> Result<(Vec<[u32; 3]>, Vec<Point3>), ConstructError> {
    let chart = SurfaceChart::new_local(patch.surface)
        .ok_or(ConstructError::NonPlanarPatch { patch: patch_index })?;
    if polygon.len() < 3 || reference.is_empty() {
        return Err(ConstructError::MalformedPatch { patch: patch_index });
    }
    // Chain θ-unwrap: each vertex within a half turn of its predecessor —
    // the polygon is corner-local by construction, so a wrap means the
    // premise failed and the CDT would be fed a self-crossing boundary.
    // A torus chart (inc-2c-3b-3) is DOUBLY periodic: φ gets the same
    // predecessor-relative unwrap and the same span guard as θ.
    let planar = matches!(chart, SurfaceChart::Plane { .. });
    let biperiodic = matches!(chart, SurfaceChart::Torus { .. });
    let wrap_near = |prev: f64, val: f64| -> f64 {
        let mut d = val - prev;
        while d > std::f64::consts::PI {
            d -= std::f64::consts::TAU;
        }
        while d <= -std::f64::consts::PI {
            d += std::f64::consts::TAU;
        }
        prev + d
    };
    let mut pool: Vec<Point2> = Vec::with_capacity(polygon.len());
    for (i, &v) in polygon.iter().enumerate() {
        let uv = chart.project(mesh.verts[v as usize]);
        let theta = if i == 0 || planar {
            uv.x()
        } else {
            wrap_near(pool[i - 1].x(), uv.x())
        };
        let vee = if i == 0 || !biperiodic {
            uv.y()
        } else {
            wrap_near(pool[i - 1].y(), uv.y())
        };
        pool.push(Point2::new(theta, vee));
    }
    if matches!(chart, SurfaceChart::Cone { .. }) && pool.iter().any(|p| p.y() <= 0.0) {
        return Err(ConstructError::ApexInPatch { patch: patch_index });
    }
    if !planar {
        let (lo, hi) = pool
            .iter()
            .fold((f64::INFINITY, f64::NEG_INFINITY), |(lo, hi), p| {
                (lo.min(p.x()), hi.max(p.x()))
            });
        if !(hi - lo).is_finite() || hi - lo >= std::f64::consts::TAU {
            return Err(ConstructError::ThetaUnwrap { patch: patch_index });
        }
    }
    if biperiodic {
        let (lo, hi) = pool
            .iter()
            .fold((f64::INFINITY, f64::NEG_INFINITY), |(lo, hi), p| {
                (lo.min(p.y()), hi.max(p.y()))
            });
        if !(hi - lo).is_finite() || hi - lo >= std::f64::consts::TAU {
            return Err(ConstructError::ThetaUnwrap { patch: patch_index });
        }
    }
    let boundary: Vec<u32> = (0..polygon.len() as u32).collect();
    let old: Vec<[u32; 3]> = reference.iter().map(|&t| mesh.tris[t as usize]).collect();
    let mesh_pos = |v: u32| -> Point3 { mesh.verts[v as usize] };
    let want = crate::stage4_splice::area_vector(&old, &mesh_pos);
    // I2d like-for-like d(T) on curved charts: the fossil fan's own
    // certified bound is the budget — no external constant. Old-triangle
    // vertices (the phantom included) unwrap toward the pool's span mid.
    let budget: Option<f64> = if planar {
        None
    } else {
        let (lo, hi) = pool
            .iter()
            .fold((f64::INFINITY, f64::NEG_INFINITY), |(lo, hi), p| {
                (lo.min(p.x()), hi.max(p.x()))
            });
        let mid = 0.5 * (lo + hi);
        let ymid = if biperiodic {
            let (ylo, yhi) = pool
                .iter()
                .fold((f64::INFINITY, f64::NEG_INFINITY), |(lo, hi), p| {
                    (lo.min(p.y()), hi.max(p.y()))
                });
            0.5 * (ylo + yhi)
        } else {
            0.0
        };
        let uv_of = |v: u32| -> Result<Point2, ConstructError> {
            let uv = chart.project(mesh.verts[v as usize]);
            let k = ((mid - uv.x()) / std::f64::consts::TAU).round();
            let y = if biperiodic {
                let l = ((ymid - uv.y()) / std::f64::consts::TAU).round();
                uv.y() + l * std::f64::consts::TAU
            } else {
                uv.y()
            };
            Ok(Point2::new(uv.x() + k * std::f64::consts::TAU, y))
        };
        let mut b = 0.0f64;
        for t in &old {
            let uv = [uv_of(t[0])?, uv_of(t[1])?, uv_of(t[2])?];
            b = b.max(
                crate::stage4_dt::d_of_t(&patch.surface, uv)
                    .map_err(|_| ConstructError::ChordCertify { patch: patch_index })?,
            );
        }
        Some(b)
    };
    // The seeded fill loop: boundary-only first; on a budget miss, insert
    // the worst triangle's chart centroid (lifted exactly onto the analytic
    // surface) and re-CDT, up to `seed_budget` interior points.
    let mut all: Vec<Point2> = pool.clone();
    loop {
        let interior: Vec<u32> = (polygon.len() as u32..all.len() as u32).collect();
        let tris2 = cdt_with_interior_constraints(&all, &boundary, &[], &interior, &[]).map_err(
            |error| ConstructError::Cdt {
                patch: patch_index,
                error,
            },
        )?;
        let pos3 = |i: u32| -> Point3 {
            if (i as usize) < polygon.len() {
                mesh.verts[polygon[i as usize] as usize]
            } else {
                chart.lift(all[i as usize])
            }
        };
        let got = crate::stage4_splice::area_vector(&tris2, &pos3);
        let d = crate::stage4_splice::dot3(want, got);
        if d == 0.0 || !d.is_finite() {
            return Err(ConstructError::DegenerateOrientation { patch: patch_index });
        }
        let mut tris2 = tris2;
        if d < 0.0 {
            for t in &mut tris2 {
                t.swap(1, 2);
            }
        }
        if let Some(budget) = budget {
            let mut new_max = 0.0f64;
            let mut worst: Option<[u32; 3]> = None;
            for t in &tris2 {
                let uv = [all[t[0] as usize], all[t[1] as usize], all[t[2] as usize]];
                let dt = crate::stage4_dt::d_of_t(&patch.surface, uv)
                    .map_err(|_| ConstructError::ChordCertify { patch: patch_index })?;
                if dt > new_max {
                    new_max = dt;
                    worst = Some(*t);
                }
            }
            if new_max > budget {
                let n_seeds = all.len() - polygon.len();
                if n_seeds >= seed_budget {
                    return Err(ConstructError::ChordDegradation {
                        patch: patch_index,
                        old_max: budget,
                        new_max,
                    });
                }
                let w = worst.expect("new_max > 0 implies a worst triangle");
                let c = [
                    (all[w[0] as usize].x() + all[w[1] as usize].x() + all[w[2] as usize].x())
                        / 3.0,
                    (all[w[0] as usize].y() + all[w[1] as usize].y() + all[w[2] as usize].y())
                        / 3.0,
                ];
                all.push(Point2::new(c[0], c[1]));
                continue;
            }
        }
        let seeds: Vec<Point3> = all[polygon.len()..]
            .iter()
            .map(|&uv| chart.lift(uv))
            .collect();
        return Ok((
            tris2
                .iter()
                .map(|t| {
                    let id = |i: u32| -> u32 {
                        if (i as usize) < polygon.len() {
                            polygon[i as usize]
                        } else {
                            seed_id_base + (i - polygon.len() as u32)
                        }
                    };
                    [id(t[0]), id(t[1]), id(t[2])]
                })
                .collect(),
            seeds,
        ));
    }
}

fn fan_rebuild_core(
    mesh: &Mesh,
    patch_index: usize,
    patch: &SplicePatch,
    victims: &BTreeSet<u32>,
    survivor: u32,
    survivor_pos: Option<Point3>,
) -> Result<PatchRebuild, ConstructError> {
    let chart = SurfaceChart::new(patch.surface)
        .ok_or(ConstructError::NonPlanarPatch { patch: patch_index })?;
    let vert_pos = |v: u32| -> Point3 {
        match survivor_pos {
            Some(p) if v == survivor => p,
            _ => mesh.verts[v as usize],
        }
    };
    let mut old_tris: Vec<u32> = Vec::new();
    let mut directed: BTreeSet<(u32, u32)> = BTreeSet::new();
    for &t in &patch.tris {
        let tri = mesh.tris[t as usize];
        if !tri.iter().any(|v| victims.contains(v)) {
            continue;
        }
        old_tris.push(t);
        for k in 0..3 {
            let (x, y) = (tri[k], tri[(k + 1) % 3]);
            if x == y || !directed.insert((x, y)) {
                // A degenerate triangle, or two region triangles sharing a
                // DIRECTED edge — the region is not an oriented 2-manifold
                // piece here.
                return Err(ConstructError::FanNotSimple {
                    patch: patch_index,
                    victim: survivor,
                    reason: FanReason::Degenerate,
                });
            }
        }
    }
    if old_tris.is_empty() {
        return Err(ConstructError::MalformedPatch { patch: patch_index });
    }
    let fan = old_tris.len();
    let mut next: BTreeMap<u32, u32> = BTreeMap::new();
    let mut indeg: BTreeMap<u32, usize> = BTreeMap::new();
    for &(x, y) in &directed {
        if directed.contains(&(y, x)) {
            continue; // interior to the region
        }
        if victims.contains(&x) || victims.contains(&y) {
            continue; // the open end along the patch boundary chain
        }
        if next.insert(x, y).is_some() {
            return Err(ConstructError::FanNotSimple {
                patch: patch_index,
                victim: survivor,
                reason: FanReason::Pinch { fan },
            });
        }
        *indeg.entry(y).or_default() += 1;
        indeg.entry(x).or_default();
    }
    if indeg.values().any(|&d| d > 1) {
        return Err(ConstructError::FanNotSimple {
            patch: patch_index,
            victim: survivor,
            reason: FanReason::Pinch { fan },
        });
    }
    // The victims are boundary-cycle vertices, so the link must be ONE open
    // run. A closed link (no source) means an interior victim — malformed
    // input for this repair; several runs mean the region meets the boundary
    // in separate sheets.
    let Some(start) = indeg.iter().find(|&(_, &d)| d == 0).map(|(&v, _)| v) else {
        return Err(ConstructError::FanNotSimple {
            patch: patch_index,
            victim: survivor,
            reason: FanReason::Pinch { fan },
        });
    };
    let mut link: Vec<u32> = vec![start];
    let mut cur = start;
    while let Some(&nx) = next.get(&cur) {
        if link.len() > next.len() {
            return Err(ConstructError::FanNotSimple {
                patch: patch_index,
                victim: survivor,
                reason: FanReason::Pinch { fan },
            });
        }
        link.push(nx);
        cur = nx;
    }
    if link.len() != indeg.len() {
        let runs = chain_link_runs(&next, &indeg);
        let with_survivor = runs.iter().filter(|r| r.contains(&survivor)).count();
        return Err(ConstructError::FanNotSimple {
            patch: patch_index,
            victim: survivor,
            reason: FanReason::Split {
                fan,
                runs: runs.len(),
                with_survivor,
            },
        });
    }
    let mut joins = false;
    if link.first() != Some(&survivor) && link.last() != Some(&survivor) {
        // Present mid-link (a boundary ear past the junction): the closure
        // edge would bypass the survivor — refused in every mode. Absent
        // entirely: refused for a run absorption; a re-homing survivor JOINS
        // the patch instead (appended to the polygon where the victim arc
        // was — see `rebuild_rehome_fan`).
        if survivor_pos.is_none() || link.contains(&survivor) {
            return Err(ConstructError::FanSurvivorNotAdjacent {
                patch: patch_index,
                victim: survivor,
            });
        }
        joins = true;
    }
    // Every region vertex must be a victim (dropped) or on the link (kept in
    // the polygon): anything else would be silently disconnected by the
    // rebuild — a boundary vertex sandwiched between non-consecutive victims,
    // or an interior vertex fully enclosed by the region.
    if let Some(&orphan) = old_tris
        .iter()
        .flat_map(|&t| mesh.tris[t as usize].iter())
        .find(|v| !victims.contains(v) && !link.contains(v))
    {
        return Err(ConstructError::FanNotSimple {
            patch: patch_index,
            victim: survivor,
            reason: FanReason::Orphaned {
                fan,
                vertex: orphan,
            },
        });
    }
    if joins {
        // The victim arc ran v_last → victims → v_first along the old
        // boundary; the survivor replaces that arc, closing the polygon.
        link.push(survivor);
    }
    if link.len() < 3 {
        // Re-homing, survivor flanking a 2-vertex link: the region is ONE
        // sliver triangle (victim, survivor, other) — under the merge it
        // degenerates to (survivor, survivor, other), so the correct
        // rebuild is EMPTY: the sliver is deleted and the survivor→other
        // edge becomes the boundary (measured shape: every R0003 §I13(f)
        // pair's sliver-band fan, census 2026-08-28). Run absorptions keep
        // the refusal — a shrinking region there is a selector defect.
        if survivor_pos.is_some() && !joins && link.len() == 2 {
            return Ok(PatchRebuild {
                patch: patch_index,
                old_tris,
                new_tris: Vec::new(),
                new_verts: Vec::new(),
                dropped: victims.iter().copied().collect(),
                plan_verts: mesh.verts.len() as u32,
                plan_tris: mesh.tris.len() as u32,
            });
        }
        return Err(ConstructError::FanNotSimple {
            patch: patch_index,
            victim: survivor,
            reason: FanReason::Short {
                fan,
                link: link.len(),
            },
        });
    }

    // Chart coordinates, θ-unwrapped against the SURVIVOR's branch (the
    // region is local: every link vertex is within π of the junction).
    let base = chart.project(vert_pos(survivor));
    let periodic = matches!(
        patch.surface,
        Surface::Cylinder { .. } | Surface::Cone { .. }
    );
    if matches!(chart, SurfaceChart::Cone { .. })
        && victims
            .iter()
            .copied()
            .chain(link.iter().copied())
            .any(|v| chart.project(vert_pos(v)).y() <= 0.0)
    {
        return Err(ConstructError::ApexInPatch { patch: patch_index });
    }
    let uv_of = |v: u32| -> Point2 {
        let uv = chart.project(vert_pos(v));
        if periodic {
            let k = ((base.x() - uv.x()) / std::f64::consts::TAU).round();
            Point2::new(uv.x() + k * std::f64::consts::TAU, uv.y())
        } else {
            uv
        }
    };
    let pool: Vec<Point2> = link.iter().map(|&v| uv_of(v)).collect();
    let boundary: Vec<u32> = (0..link.len() as u32).collect();
    let tris2 =
        cdt_with_interior_constraints(&pool, &boundary, &[], &[], &[]).map_err(|error| {
            ConstructError::Cdt {
                patch: patch_index,
                error,
            }
        })?;
    let old: Vec<[u32; 3]> = old_tris.iter().map(|&t| mesh.tris[t as usize]).collect();
    // `want` is the OLD region's outward sense — old positions on purpose;
    // `got` evaluates the replacement at the survivor's (possibly re-homed)
    // position.
    let mesh_pos = |v: u32| -> Point3 { mesh.verts[v as usize] };
    let pos3 = |i: u32| -> Point3 { vert_pos(link[i as usize]) };
    let want = crate::stage4_splice::area_vector(&old, &mesh_pos);
    let got = crate::stage4_splice::area_vector(&tris2, &pos3);
    let d = crate::stage4_splice::dot3(want, got);
    if d == 0.0 || !d.is_finite() {
        return Err(ConstructError::DegenerateOrientation { patch: patch_index });
    }
    let mut tris2 = tris2;
    if d < 0.0 {
        for t in &mut tris2 {
            t.swap(1, 2);
        }
    }
    let new_tris: Vec<[u32; 3]> = tris2
        .iter()
        .map(|t| {
            [
                link[t[0] as usize],
                link[t[1] as usize],
                link[t[2] as usize],
            ]
        })
        .collect();
    Ok(PatchRebuild {
        patch: patch_index,
        old_tris,
        new_tris,
        new_verts: Vec::new(),
        dropped: victims.iter().copied().collect(),
        plan_verts: mesh.verts.len() as u32,
        plan_tris: mesh.tris.len() as u32,
    })
}

/// I13e — the cross-site analog of [`rebuild_run_fan`]: re-triangulate the
/// union of an interlocked GROUP's regions in one rebuild, absorbing each
/// site's victims into that site's own junction survivor.
///
/// The per-site rebuild is structurally unable to repair an interlocked
/// group: adjacent strips' deep overruns cross each other's territory, so
/// each site's link polygon contains the partner's still-folded victim and
/// its CDT is RIGHT to refuse (measured 2026-08-25, R0003 wall patch 475 —
/// all six single fans decline with `TriangulationFailed`,
/// `YANG_441_FAN_PROBE` crossings=1, in both repair orders). Once every
/// arc of the group is absorbed together, the region's boundary contains
/// no group victim at all, so that refusal cannot recur by construction.
///
/// Construction: region = every patch triangle touching any victim of any
/// site; its boundary edges chain into ONE closed cycle (the region must
/// be a disk — several loops are refused loudly). Each site's victims
/// appear on that cycle as one maximal ARC; deleting the arc joins its two
/// flanking vertices with a closure edge, which becomes the new boundary
/// span `survivor → far neighbour` — hence each site's survivor must FLANK
/// its own arc. A site with no victim on this patch contributes no arc (a
/// strip holder sees only its own site; the wall holder sees them all), so
/// the k = 1 restriction of this construction is exactly
/// [`rebuild_run_fan`]'s.
pub(crate) fn rebuild_group_fan(
    mesh: &Mesh,
    patch_index: usize,
    patch: &SplicePatch,
    sites: &[(BTreeSet<u32>, u32)],
) -> Result<PatchRebuild, ConstructError> {
    let chart = SurfaceChart::new(patch.surface)
        .ok_or(ConstructError::NonPlanarPatch { patch: patch_index })?;
    let mut site_of: BTreeMap<u32, usize> = BTreeMap::new();
    for (i, (victims, _)) in sites.iter().enumerate() {
        for &v in victims {
            if site_of.insert(v, i).is_some() || sites.iter().any(|&(_, s)| s == v) {
                // A victim claimed by two sites, or doubling as a survivor —
                // the selector's ambiguity rule should have dropped this
                // group; refused, never guessed.
                return Err(ConstructError::MalformedPatch { patch: patch_index });
            }
        }
    }
    let report = sites.first().map(|&(_, s)| s).unwrap_or(0);
    let is_victim = |v: &u32| site_of.contains_key(v);
    let mut old_tris: Vec<u32> = Vec::new();
    let mut directed: BTreeSet<(u32, u32)> = BTreeSet::new();
    for &t in &patch.tris {
        let tri = mesh.tris[t as usize];
        if !tri.iter().any(is_victim) {
            continue;
        }
        old_tris.push(t);
        for k in 0..3 {
            let (x, y) = (tri[k], tri[(k + 1) % 3]);
            if x == y || !directed.insert((x, y)) {
                // A degenerate triangle, or two region triangles sharing a
                // DIRECTED edge — not an oriented 2-manifold piece.
                return Err(ConstructError::FanNotSimple {
                    patch: patch_index,
                    victim: report,
                    reason: FanReason::Degenerate,
                });
            }
        }
    }
    if old_tris.is_empty() {
        return Err(ConstructError::MalformedPatch { patch: patch_index });
    }
    let fan = old_tris.len();
    // The FULL region boundary — victim-incident edges included, unlike the
    // run fan's open link: the cycle ORDER is what assigns each arc its
    // flanks.
    let mut next: BTreeMap<u32, u32> = BTreeMap::new();
    let mut indeg: BTreeMap<u32, usize> = BTreeMap::new();
    for &(x, y) in &directed {
        if directed.contains(&(y, x)) {
            continue; // interior to the region
        }
        if next.insert(x, y).is_some() {
            return Err(ConstructError::FanNotSimple {
                patch: patch_index,
                victim: report,
                reason: FanReason::Pinch { fan },
            });
        }
        *indeg.entry(y).or_default() += 1;
        indeg.entry(x).or_default();
    }
    if indeg.values().any(|&d| d != 1) || next.len() != indeg.len() {
        // A manifold region's boundary is closed cycles — every boundary
        // vertex has exactly one in- and one out-edge; any imbalance means
        // the boundary passes a vertex twice.
        return Err(ConstructError::FanNotSimple {
            patch: patch_index,
            victim: report,
            reason: FanReason::Pinch { fan },
        });
    }
    let Some(&start) = next.keys().next() else {
        // A bounded region with no boundary edge at all — malformed input.
        return Err(ConstructError::MalformedPatch { patch: patch_index });
    };
    let mut cycle: Vec<u32> = vec![start];
    let mut cur = start;
    while let Some(&nx) = next.get(&cur) {
        if nx == start || cycle.len() > next.len() {
            break;
        }
        cycle.push(nx);
        cur = nx;
    }
    if cycle.len() != next.len() {
        // Several boundary loops: the region is disconnected on this patch,
        // or encloses a hole. Not a disk — refused; the census names the
        // configuration if it ever occurs in the wild.
        let mut seen: BTreeSet<u32> = cycle.iter().copied().collect();
        let mut runs = 1usize;
        let mut with_survivor = usize::from(sites.iter().any(|&(_, s)| seen.contains(&s)));
        while let Some(&s2) = next.keys().find(|v| !seen.contains(v)) {
            runs += 1;
            let mut c = s2;
            let mut lp: BTreeSet<u32> = BTreeSet::new();
            loop {
                lp.insert(c);
                seen.insert(c);
                c = next[&c];
                if c == s2 {
                    break;
                }
            }
            with_survivor += usize::from(sites.iter().any(|&(_, s)| lp.contains(&s)));
        }
        return Err(ConstructError::FanNotSimple {
            patch: patch_index,
            victim: report,
            reason: FanReason::Split {
                fan,
                runs,
                with_survivor,
            },
        });
    }
    // Rotate so position 0 is a non-victim: arcs then never straddle the
    // wrap. All victims on the boundary and no non-victim at all would be a
    // region with no link — malformed.
    let Some(off) = cycle.iter().position(|v| !is_victim(v)) else {
        return Err(ConstructError::MalformedPatch { patch: patch_index });
    };
    cycle.rotate_left(off);
    let mut arcs: Vec<(usize, usize)> = Vec::new();
    let mut i = 0;
    while i < cycle.len() {
        if is_victim(&cycle[i]) {
            let s = i;
            while i < cycle.len() && is_victim(&cycle[i]) {
                i += 1;
            }
            arcs.push((s, i - 1));
        } else {
            i += 1;
        }
    }
    // Sites present on this patch (≥ 1 victim in a region triangle — which
    // is every patch triangle containing that victim, by construction).
    let present: BTreeSet<usize> = old_tris
        .iter()
        .flat_map(|&t| mesh.tris[t as usize])
        .filter_map(|v| site_of.get(&v).copied())
        .collect();
    // One arc per present site, each arc exactly that site's COMPLETE victim
    // set: a fused/split/partial arc has no single closure edge to absorb it.
    let mismatch = || {
        Err(ConstructError::FanNotSimple {
            patch: patch_index,
            victim: report,
            reason: FanReason::ArcMismatch {
                fan,
                arcs: arcs.len(),
                sites: present.len(),
            },
        })
    };
    if arcs.len() != present.len() {
        return mismatch();
    }
    let mut owner: Vec<bool> = vec![false; sites.len()];
    for &(s, e) in &arcs {
        let arc_set: BTreeSet<u32> = cycle[s..=e].iter().copied().collect();
        let si = site_of[&cycle[s]];
        if sites[si].0 != arc_set || std::mem::replace(&mut owner[si], true) {
            return mismatch();
        }
    }
    // Each site's survivor must flank its own arc: the closure edge for the
    // arc runs `flank → flank`, and a survivor elsewhere on (or off) the
    // cycle would be stranded off its absorbed boundary span.
    let n = cycle.len();
    for &(s, e) in &arcs {
        let survivor = sites[site_of[&cycle[s]]].1;
        if survivor != cycle[(s + n - 1) % n] && survivor != cycle[(e + 1) % n] {
            return Err(ConstructError::FanSurvivorNotAdjacent {
                patch: patch_index,
                victim: survivor,
            });
        }
    }
    let polygon: Vec<u32> = cycle.iter().copied().filter(|v| !is_victim(v)).collect();
    // Every region vertex is a victim (dropped) or on the polygon (kept):
    // anything else would be silently disconnected by the rebuild.
    let on_polygon: BTreeSet<u32> = polygon.iter().copied().collect();
    if let Some(&orphan) = old_tris
        .iter()
        .flat_map(|&t| mesh.tris[t as usize].iter())
        .find(|v| !is_victim(v) && !on_polygon.contains(v))
    {
        return Err(ConstructError::FanNotSimple {
            patch: patch_index,
            victim: report,
            reason: FanReason::Orphaned {
                fan,
                vertex: orphan,
            },
        });
    }
    if polygon.len() < 3 {
        return Err(ConstructError::FanNotSimple {
            patch: patch_index,
            victim: report,
            reason: FanReason::Short {
                fan,
                link: polygon.len(),
            },
        });
    }
    // Chart coordinates, θ-unwrapped against the first present site's
    // survivor (a cycle vertex by the flank check; the region is local —
    // interlocked sites overlap by definition).
    let Some(&base_si) = present.iter().next() else {
        return Err(ConstructError::MalformedPatch { patch: patch_index });
    };
    let base_v = sites[base_si].1;
    let base = chart.project(mesh.verts[base_v as usize]);
    let periodic = matches!(
        patch.surface,
        Surface::Cylinder { .. } | Surface::Cone { .. }
    );
    if matches!(chart, SurfaceChart::Cone { .. })
        && site_of
            .keys()
            .copied()
            .chain(polygon.iter().copied())
            .any(|v| chart.project(mesh.verts[v as usize]).y() <= 0.0)
    {
        return Err(ConstructError::ApexInPatch { patch: patch_index });
    }
    let uv_of = |v: u32| -> Point2 {
        let uv = chart.project(mesh.verts[v as usize]);
        if periodic {
            let k = ((base.x() - uv.x()) / std::f64::consts::TAU).round();
            Point2::new(uv.x() + k * std::f64::consts::TAU, uv.y())
        } else {
            uv
        }
    };
    let pool: Vec<Point2> = polygon.iter().map(|&v| uv_of(v)).collect();
    let boundary: Vec<u32> = (0..polygon.len() as u32).collect();
    let tris2 =
        cdt_with_interior_constraints(&pool, &boundary, &[], &[], &[]).map_err(|error| {
            ConstructError::Cdt {
                patch: patch_index,
                error,
            }
        })?;
    let old: Vec<[u32; 3]> = old_tris.iter().map(|&t| mesh.tris[t as usize]).collect();
    let mesh_pos = |v: u32| -> Point3 { mesh.verts[v as usize] };
    let pos3 = |i: u32| -> Point3 { mesh.verts[polygon[i as usize] as usize] };
    let want = crate::stage4_splice::area_vector(&old, &mesh_pos);
    let got = crate::stage4_splice::area_vector(&tris2, &pos3);
    let d = crate::stage4_splice::dot3(want, got);
    if d == 0.0 || !d.is_finite() {
        return Err(ConstructError::DegenerateOrientation { patch: patch_index });
    }
    let mut tris2 = tris2;
    if d < 0.0 {
        for t in &mut tris2 {
            t.swap(1, 2);
        }
    }
    let new_tris: Vec<[u32; 3]> = tris2
        .iter()
        .map(|t| {
            [
                polygon[t[0] as usize],
                polygon[t[1] as usize],
                polygon[t[2] as usize],
            ]
        })
        .collect();
    // Per the `dropped` contract: vertices THIS patch's old triangles
    // referenced that its rebuild does not — the present sites' victims
    // only, not the whole group's (a strip holder never referenced the
    // partner strip's victim).
    let dropped: BTreeSet<u32> = present
        .iter()
        .flat_map(|&si| sites[si].0.iter().copied())
        .collect();
    Ok(PatchRebuild {
        patch: patch_index,
        old_tris,
        new_tris,
        new_verts: Vec::new(),
        dropped,
        plan_verts: mesh.verts.len() as u32,
        plan_tris: mesh.tris.len() as u32,
    })
}

/// Write a batch of [`PatchRebuild`]s into the mesh in ONE pass: drop every
/// rebuilt patch's old triangles, append each patch's replacements carrying
/// that patch's (uniform) attribution. `attribution.attributions` stays in
/// lockstep with `mesh.tris` — the invariant every downstream consumer
/// depends on. I2e seed vertices (`new_verts`) are appended per rebuild and
/// that rebuild's `plan_verts + k` triangle indices are remapped onto the
/// appended block; orphaned old vertices are left for the caller's usual
/// `compact_unreferenced_verts` pass.
///
/// `subs` is the I1g Fig-11(b) corner-merge map (`p -> q`): every SURVIVING
/// (non-rebuilt) triangle that references a merged corner `p` is re-pointed
/// at `q` — the shared-index identification that lets a curved neighbour
/// (out of the planar rebuild's scope) adopt the merge without a re-CDT.
/// Rebuilt triangles already reference `q` via the substituted cycles.
pub(crate) fn apply_rebuild_batch(
    mesh: &mut Mesh,
    attribution: &mut TriangleAttributionMap,
    rebuilds: &[PatchRebuild],
    subs: &BTreeMap<u32, u32>,
) -> Result<(), ConstructError> {
    let (n_verts, n_tris) = (mesh.verts.len() as u32, mesh.tris.len() as u32);
    let mut removed: BTreeSet<u32> = BTreeSet::new();
    let mut attrs_of: Vec<Option<TriangleAttribution>> = Vec::with_capacity(rebuilds.len());
    for r in rebuilds {
        if r.plan_verts != n_verts || r.plan_tris != n_tris {
            return Err(ConstructError::StalePlan {
                expected_tris: r.plan_tris,
                actual_tris: n_tris,
            });
        }
        let mut it = r
            .old_tris
            .iter()
            .map(|&t| attribution.attributions[t as usize]);
        let first = it.next().flatten();
        if it.any(|a| a != first) {
            return Err(ConstructError::MixedAttribution { patch: r.patch });
        }
        attrs_of.push(first);
        for &t in &r.old_tris {
            if !removed.insert(t) {
                return Err(ConstructError::OverlappingBatch { tri: t });
            }
        }
    }

    let added: usize = rebuilds.iter().map(|r| r.new_tris.len()).sum();
    let mut tris = Vec::with_capacity(mesh.tris.len() - removed.len() + added);
    let mut attrs = Vec::with_capacity(tris.capacity());
    for (t, tri) in mesh.tris.iter().enumerate() {
        if removed.contains(&(t as u32)) {
            continue;
        }
        let mut out = *tri;
        if !subs.is_empty() {
            for v in &mut out {
                if let Some(&q) = subs.get(v) {
                    *v = q;
                }
            }
        }
        tris.push(out);
        attrs.push(attribution.attributions[t]);
    }
    for (r, attr) in rebuilds.iter().zip(attrs_of) {
        // I2e: append this rebuild's seed vertices and point its
        // `plan_verts + k` indices at the appended block. Every rebuild in
        // the batch was planned against the same pre-batch mesh (the
        // StalePlan check above), so `v >= r.plan_verts` identifies exactly
        // the seed references.
        let seed_base = mesh.verts.len() as u32;
        mesh.verts.extend_from_slice(&r.new_verts);
        for tri in &r.new_tris {
            let mut out = *tri;
            if !r.new_verts.is_empty() {
                for v in &mut out {
                    if *v >= r.plan_verts {
                        *v = seed_base + (*v - r.plan_verts);
                    }
                }
            }
            tris.push(out);
            attrs.push(attr);
        }
    }
    mesh.tris = tris;
    attribution.attributions = attrs;
    Ok(())
}

/// I5-0 — the §4.3.4 seam-density census of one ordered conic seam chain
/// (spec `yang_441_trim_cdt_construction.md` §4-I5; read-only). Measures how
/// far the mesh-inherited chain is from the paper's h/l/α acceptance
/// (`refs/text/yang2025_hybrid_boolean.txt:575-593`), and how many samples
/// the paper's own subdivision loop would insert to reach it.
#[derive(Debug, Clone, Copy, Default)]
pub(crate) struct ChainDensityCensus {
    /// Consecutive sample pairs measured (n−1 open, n closed).
    pub pairs: usize,
    /// Pairs failing each criterion individually (h ≥ d_p·10², l ≥ d_p·10³,
    /// α ≥ π/18) and the paper's conjunction (`fail_any` = the pairs §4.3.4
    /// would subdivide).
    pub fail_h: usize,
    pub fail_l: usize,
    pub fail_alpha: usize,
    pub fail_any: usize,
    /// Worst measurements over the chain (radians for `max_alpha`).
    pub max_h: f64,
    pub max_l: f64,
    pub max_alpha: f64,
    /// Largest scale-relative d_p seen (context for the maxima).
    pub dp_max: f64,
    /// Samples the paper's recursion would insert over the whole chain,
    /// simulated on the exact curve; `capped` when the simulation hit its
    /// depth or budget guard (the count is then a lower bound).
    pub implied_inserts: u64,
    pub capped: bool,
}

/// Budget/depth guards for the [`census_conic_seam_density`] subdivision
/// simulation — bounds work on pathological chains; `capped` reports a hit.
const I5_SIM_DEPTH_MAX: u32 = 32;
const I5_SIM_INSERT_BUDGET: u64 = 1_000_000;

/// See [`ChainDensityCensus`]. `None` when any chain vertex has no conic
/// parameter or the curve has no closed form — the caller reports the skip
/// loudly; a partial census would misreport the seam as sparse-but-healthy.
pub(crate) fn census_conic_seam_density(
    verts: &[Point3],
    chain: &[u32],
    curve: &Curve,
    closed: bool,
) -> Option<ChainDensityCensus> {
    use crate::stage4_correct::{conic_param, paper_chain_metrics, paper_chain_sample_redundant};
    let pi = std::f64::consts::PI;
    let n = chain.len();
    if n < 2 {
        return None;
    }
    // I13b: the (−π, π] delta wrap below is angle-domain — open-conic params
    // must not pass through it. Same loud `None` a missing parameter produced.
    if !crate::stage4_correct::conic_param_periodic(curve) {
        return None;
    }
    let mut out = ChainDensityCensus::default();
    let pair_count = if closed { n } else { n - 1 };
    for i in 0..pair_count {
        let (va, vb) = (chain[i], chain[(i + 1) % n]);
        let (pa, pb) = (verts[va as usize], verts[vb as usize]);
        let ta = conic_param(curve, pa)?;
        let tb = conic_param(curve, pb)?;
        if !ta.is_finite() || !tb.is_finite() {
            return None;
        }
        // Wrap the delta to (−π, π] — a seam chord subtends less than a half
        // turn (the port's standing convention, cf. `conic_param_deltas`).
        // The unwrapped interval [ta, ta+d] then contains every midpoint, so
        // the recursion below needs no further wrapping.
        let mut d = tb - ta;
        while d > pi {
            d -= 2.0 * pi;
        }
        while d <= -pi {
            d += 2.0 * pi;
        }
        let tb_unwrapped = ta + d;
        let tm = 0.5 * (ta + tb_unwrapped);
        let m = crate::geom::conic_eval(curve, tm)?;
        let mt = paper_chain_metrics(pa.as_array(), m.as_array(), pb.as_array());
        out.pairs += 1;
        if mt.h >= mt.dp * 1e2 {
            out.fail_h += 1;
        }
        if mt.l >= mt.dp * 1e3 {
            out.fail_l += 1;
        }
        if !mt.degenerate && mt.alpha >= pi / 18.0 {
            out.fail_alpha += 1;
        }
        if !paper_chain_sample_redundant(pa.as_array(), m.as_array(), pb.as_array()) {
            out.fail_any += 1;
        }
        out.max_h = out.max_h.max(mt.h);
        out.max_l = out.max_l.max(mt.l);
        out.max_alpha = out.max_alpha.max(mt.alpha);
        out.dp_max = out.dp_max.max(mt.dp);
        let mut budget = I5_SIM_INSERT_BUDGET.saturating_sub(out.implied_inserts);
        let mut capped = false;
        let ins = implied_inserts_rec(
            curve,
            ta,
            pa,
            tb_unwrapped,
            pb,
            I5_SIM_DEPTH_MAX,
            &mut budget,
            &mut capped,
        );
        out.implied_inserts = out.implied_inserts.saturating_add(ins);
        out.capped |= capped;
    }
    Some(out)
}

/// I5-1 — the §4.3.4 refinement itself (spec §4-I5; gated by the caller on
/// `YANG_434_INSERT`): run the paper's midpoint recursion over an ordered
/// open conic chain, producing the on-curve points to insert and the
/// refined chain.
///
/// Returns `(new_points, refined)` where `refined` interleaves the existing
/// chain indices with `base + k` references into `new_points` (parameter
/// order; endpoints preserved — junctions are never inserted past).
/// `new_points` may be empty (the chain already meets the paper's
/// acceptance — the caller keeps its shipped behavior). `None` declines
/// LOUDLY-at-the-caller: a missing conic parameter, or the recursion
/// exceeding `budget` inserts (the I2e-precedent runaway backstop) or the
/// depth guard — the caller falls back to the reorder-only action and logs;
/// the shipped coarse chain is the pre-I5 status quo, not a silent wrong.
pub(crate) fn refine_conic_chain(
    verts: &[Point3],
    chain: &[u32],
    curve: &Curve,
    base: u32,
    budget: u64,
) -> Option<(Vec<Point3>, Vec<u32>)> {
    use crate::stage4_correct::conic_param;
    let pi = std::f64::consts::PI;
    if chain.len() < 2 {
        return None;
    }
    // I13b: same angle-domain guard as `census_conic_seam_density` — the
    // wrap below and the midpoint recursion are for periodic params only.
    if !crate::stage4_correct::conic_param_periodic(curve) {
        return None;
    }
    let mut new_points: Vec<Point3> = Vec::new();
    let mut refined: Vec<u32> = Vec::with_capacity(chain.len());
    for i in 0..chain.len() - 1 {
        let (va, vb) = (chain[i], chain[i + 1]);
        let (pa, pb) = (verts[va as usize], verts[vb as usize]);
        let ta = conic_param(curve, pa)?;
        let tb = conic_param(curve, pb)?;
        if !ta.is_finite() || !tb.is_finite() {
            return None;
        }
        let mut d = tb - ta;
        while d > pi {
            d -= 2.0 * pi;
        }
        while d <= -pi {
            d += 2.0 * pi;
        }
        refined.push(va);
        let before = new_points.len() as u64;
        let mut remaining = budget.saturating_sub(before);
        let mut capped = false;
        // In-order traversal: left half, midpoint, right half — so the
        // points land in `new_points` already in parameter order and the
        // chain slice for this pair is a contiguous run of ordinals.
        collect_inserts_rec(
            curve,
            ta,
            pa,
            ta + d,
            pb,
            I5_SIM_DEPTH_MAX,
            &mut remaining,
            &mut capped,
            &mut new_points,
        )?;
        if capped {
            return None;
        }
        for k in before..new_points.len() as u64 {
            refined.push(base + u32::try_from(k).ok()?);
        }
    }
    refined.push(*chain.last().expect("chain len >= 2"));
    Some((new_points, refined))
}

/// In-order §4.3.4 recursion collecting the inserted points (the executable
/// twin of [`implied_inserts_rec`] — same acceptance, same guards). `None`
/// only on a failed curve evaluation.
#[allow(clippy::too_many_arguments)]
fn collect_inserts_rec(
    curve: &Curve,
    ta: f64,
    pa: Point3,
    tb: f64,
    pb: Point3,
    depth: u32,
    budget: &mut u64,
    capped: &mut bool,
    out: &mut Vec<Point3>,
) -> Option<()> {
    let tm = 0.5 * (ta + tb);
    let m = crate::geom::conic_eval(curve, tm)?;
    if crate::stage4_correct::paper_chain_sample_redundant(
        pa.as_array(),
        m.as_array(),
        pb.as_array(),
    ) {
        return Some(());
    }
    if depth == 0 || *budget == 0 {
        *capped = true;
        return Some(());
    }
    *budget -= 1;
    collect_inserts_rec(curve, ta, pa, tm, m, depth - 1, budget, capped, out)?;
    out.push(m);
    collect_inserts_rec(curve, tm, m, tb, pb, depth - 1, budget, capped, out)?;
    Some(())
}

/// The paper's §4.3.4 recursion, simulated on the exact curve: test the
/// parameter-midpoint sample; if the pair fails h/l/α, count the insert and
/// recurse on both halves. Returns the insert count for this interval;
/// `budget`/`depth` exhaustion sets `capped` and under-counts (a lower
/// bound, reported as such).
#[allow(clippy::too_many_arguments)]
fn implied_inserts_rec(
    curve: &Curve,
    ta: f64,
    pa: Point3,
    tb: f64,
    pb: Point3,
    depth: u32,
    budget: &mut u64,
    capped: &mut bool,
) -> u64 {
    let tm = 0.5 * (ta + tb);
    let Some(m) = crate::geom::conic_eval(curve, tm) else {
        *capped = true;
        return 0;
    };
    if crate::stage4_correct::paper_chain_sample_redundant(
        pa.as_array(),
        m.as_array(),
        pb.as_array(),
    ) {
        return 0;
    }
    if depth == 0 || *budget == 0 {
        *capped = true;
        return 0;
    }
    *budget -= 1;
    let left = implied_inserts_rec(curve, ta, pa, tm, m, depth - 1, budget, capped);
    let right = implied_inserts_rec(curve, tm, m, tb, pb, depth - 1, budget, capped);
    1 + left + right
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
    fn chain_straightness_separates_online_scramble_from_a_real_corner() {
        // An on-line chain with a SCRAMBLED parameter order (the F0067
        // fold-back mint): interior at x=2.0 lies beyond the chord but ON the
        // line — straightness ~0, collapse stays allowed.
        let verts = vec![
            Point3::new(0.0, 0.0, 0.0),
            Point3::new(2.0, 1e-16, 0.0),
            Point3::new(1.0, 0.0, 0.0),
        ];
        let s = chain_straightness(&verts, &[0, 1, 2]).expect("non-degenerate");
        assert!(s < 1e-9, "on-line overshoot must pass, got {s:.3e}");
        // A REAL corner (two different lines meeting at x): macroscopic.
        let verts = vec![
            Point3::new(0.0, 0.0, 0.0),
            Point3::new(1.0, 1.0, 0.0),
            Point3::new(2.0, 0.0, 0.0),
        ];
        let s = chain_straightness(&verts, &[0, 1, 2]).expect("non-degenerate");
        assert!(
            s > 1e-3,
            "a corner must be loudly non-straight, got {s:.3e}"
        );
        // Coincident endpoints: degenerate, refused.
        let verts = vec![Point3::new(0.0, 0.0, 0.0), Point3::new(1.0, 0.0, 0.0)];
        assert_eq!(chain_straightness(&verts, &[0, 1, 0]), None);
    }

    #[test]
    fn on_segment_interior_takes_inside_and_refuses_beyond_and_off() {
        let verts = vec![
            Point3::new(0.0, 0.0, 0.0),   // e0
            Point3::new(1.0, 0.0, 0.0),   // e1
            Point3::new(0.4, 0.0, 0.0),   // inside, exact
            Point3::new(1.2, 0.0, 0.0),   // beyond e1 — a different feature's vertex
            Point3::new(0.4, 1e-8, 0.0),  // off the line just above the band
            Point3::new(0.4, 1e-12, 0.0), // exactness-scale noise — inside
        ];
        assert!(on_segment_interior(&verts, 0, 1, 2), "exact interior");
        assert!(!on_segment_interior(&verts, 0, 1, 3), "beyond junction");
        assert!(
            !on_segment_interior(&verts, 0, 1, 4),
            "1e-8 relative is above the 1e-9 identity band"
        );
        assert!(
            on_segment_interior(&verts, 0, 1, 5),
            "1e-12 relative is exactness noise — inside the band"
        );
        // Degenerate chord refuses.
        assert!(!on_segment_interior(&verts, 0, 0, 2));
    }

    #[test]
    fn collapse_patch_runs_collapses_all_runs_of_one_cycle() {
        // Two disjoint seam runs on one cycle: 0-7-8-1 and 2-9-3. Collapsing
        // BOTH is exactly the I1b move I1 could not make one-at-a-time.
        let cycles = vec![vec![0, 7, 8, 1, 5, 2, 9, 3]];
        let out = collapse_patch_runs(&cycles, &[vec![0, 7, 8, 1], vec![2, 9, 3]])
            .expect("both runs collapse");
        assert_eq!(out[0], vec![2, 3, 0, 1, 5]);
    }

    #[test]
    fn collapse_patch_runs_keeps_shared_junction_endpoints() {
        // Adjacent seams share junction vertex 1; both collapses keep it.
        let cycles = vec![vec![0, 7, 1, 8, 2, 5]];
        let out = collapse_patch_runs(&cycles, &[vec![0, 7, 1], vec![1, 8, 2]])
            .expect("adjacent runs collapse");
        assert_eq!(out[0], vec![1, 2, 5, 0]);
    }

    #[test]
    fn collapse_patch_runs_names_the_degenerating_chain() {
        // Collapsing the second chain would leave a 2-gon: Err names it.
        let cycles = vec![vec![0, 7, 8, 1, 9]];
        assert_eq!(
            collapse_patch_runs(&cycles, &[vec![0, 7, 8, 1], vec![1, 9, 0]]),
            Err(1)
        );
        // A chain absent from the cycles is named too.
        assert_eq!(collapse_patch_runs(&cycles, &[vec![2, 3, 4]]), Err(0));
    }

    fn square_fan_mesh() -> Mesh {
        // Unit square in z=0, CCW from +z, with a kept-out-of-boundary center
        // vertex 4 fanned by four triangles.
        Mesh {
            verts: vec![
                Point3::new(0.0, 0.0, 0.0),
                Point3::new(1.0, 0.0, 0.0),
                Point3::new(1.0, 1.0, 0.0),
                Point3::new(0.0, 1.0, 0.0),
                Point3::new(0.5, 0.5, 0.0),
            ],
            tris: vec![[0, 1, 4], [1, 2, 4], [2, 3, 4], [3, 0, 4]],
        }
    }

    // ---- I6: Fig-11(b)->(c) local fan merge ----------------------------

    /// A boundary victim: the fan around vertex 4 on a half-square, merged into
    /// its boundary neighbour. The rebuild must replace exactly the fan, drop
    /// the victim, and keep the same outward sense.
    #[test]
    fn rebuild_merge_fan_replaces_only_the_victim_fan() {
        // Pentagon 0,1,2,3 with a boundary vertex 4 between 0 and 1, fanned to
        // an interior vertex 5.
        let mesh = Mesh {
            verts: vec![
                Point3::new(0.0, 0.0, 0.0),
                Point3::new(1.0, 0.0, 0.0),
                Point3::new(1.0, 1.0, 0.0),
                Point3::new(0.0, 1.0, 0.0),
                Point3::new(0.5, 0.0, 0.0),
                Point3::new(0.5, 0.5, 0.0),
            ],
            tris: vec![[0, 4, 5], [4, 1, 5], [1, 2, 5], [2, 3, 5], [3, 0, 5]],
        };
        let patch = plane_patch(vec![vec![0, 4, 1, 2, 3]], vec![0, 1, 2, 3, 4]);
        let r = rebuild_merge_fan(&mesh, 9, &patch, 4, 0, false).expect("fan rebuild");
        assert_eq!(r.patch, 9);
        assert_eq!(r.old_tris, vec![0, 1], "exactly the two triangles at v4");
        assert_eq!(r.dropped, [4u32].into());
        assert!(
            r.new_tris.iter().flatten().all(|&v| v != 4),
            "the victim is gone: {:?}",
            r.new_tris
        );
        // Link 0 -> 5 -> 1 closed by 1 -> 0: one triangle.
        assert_eq!(r.new_tris.len(), 1);
        let t = r.new_tris[0];
        assert_eq!(
            t.iter().copied().collect::<BTreeSet<u32>>(),
            [0u32, 1, 5].into()
        );
        // Orientation matches the fan it replaces (both +z here).
        let pos = |v: u32| -> Point3 { mesh.verts[v as usize] };
        let want = crate::stage4_splice::area_vector(&[[0, 4, 5], [4, 1, 5]], &pos);
        let got = crate::stage4_splice::area_vector(&r.new_tris, &pos);
        assert!(crate::stage4_splice::dot3(want, got) > 0.0);
    }

    /// An INTERIOR victim: the link is a closed cycle, and the hole it leaves
    /// is re-triangulated whole.
    #[test]
    fn rebuild_merge_fan_handles_an_interior_victim() {
        let mesh = square_fan_mesh();
        let patch = plane_patch(vec![vec![0, 1, 2, 3]], vec![0, 1, 2, 3]);
        let r = rebuild_merge_fan(&mesh, 0, &patch, 4, 0, false).expect("interior fan rebuild");
        assert_eq!(r.old_tris, vec![0, 1, 2, 3], "the whole fan");
        assert_eq!(r.dropped, [4u32].into());
        assert_eq!(r.new_tris.len(), 2, "the square link is two triangles");
        assert!(r.new_tris.iter().flatten().all(|&v| v != 4));
    }

    /// The survivor must be joined to the victim by a triangle edge — otherwise
    /// this is not the local Fig-11 operation and the pass refuses it loudly.
    #[test]
    fn rebuild_merge_fan_refuses_a_survivor_off_the_link() {
        let mesh = Mesh {
            verts: vec![
                Point3::new(0.0, 0.0, 0.0),
                Point3::new(1.0, 0.0, 0.0),
                Point3::new(1.0, 1.0, 0.0),
                Point3::new(0.0, 1.0, 0.0),
                Point3::new(0.5, 0.5, 0.0),
                Point3::new(9.0, 9.0, 0.0), // not on any fan triangle
            ],
            tris: vec![[0, 1, 4], [1, 2, 4], [2, 3, 4], [3, 0, 4]],
        };
        let patch = plane_patch(vec![vec![0, 1, 2, 3]], vec![0, 1, 2, 3]);
        assert_eq!(
            rebuild_merge_fan(&mesh, 2, &patch, 4, 5, false),
            Err(ConstructError::FanSurvivorNotAdjacent {
                patch: 2,
                victim: 4
            })
        );
    }

    /// A PINCHED victim — two triangle fans meeting only at the vertex — chains
    /// into two link runs. Re-triangulating it locally would have to guess which
    /// sheet the merge belongs to, so it is a loud refusal (this is the measured
    /// R0011 / R0074 / R0085 decline).
    #[test]
    fn rebuild_merge_fan_refuses_a_pinched_vertex() {
        let mesh = Mesh {
            verts: vec![
                Point3::new(0.0, 0.0, 0.0),
                Point3::new(1.0, 0.0, 0.0),
                Point3::new(1.0, 1.0, 0.0),
                Point3::new(-1.0, 0.0, 0.0),
                Point3::new(-1.0, -1.0, 0.0),
                Point3::new(0.5, 0.5, 0.0),
            ],
            // Two disjoint fans around vertex 0.
            tris: vec![[0, 1, 2], [0, 3, 4]],
        };
        let patch = plane_patch(vec![vec![1, 2, 0, 3, 4]], vec![0, 1]);
        assert_eq!(
            rebuild_merge_fan(&mesh, 4, &patch, 0, 1, false),
            Err(ConstructError::FanNotSimple {
                patch: 4,
                victim: 0,
                reason: FanReason::Split {
                    fan: 2,
                    runs: 2,
                    with_survivor: 1
                }
            })
        );
    }

    /// The §4-I9 certificate separates the two measured populations by seven
    /// orders: a crossed carrier vertex sits ON the travel segment (measured
    /// 6.4e-13 down to exactly 0 relative), a legitimate Fig-11 victim sits
    /// 5–6.6 % of travel OFF it.
    #[test]
    fn point_on_segment_interior_separates_a_crossing_from_a_fig11_victim() {
        let pre = [0.0, 0.0, 0.0];
        let post = [22.213, 0.0, 0.0];
        // R0011's shape: the corner at t = 0.668, on the line to 6.4e-13.
        let crossed = [0.668 * 22.213, 6.4e-13, 0.0];
        assert!(point_on_segment_interior(pre, post, crossed));
        // F0045/R0090's shape: the victim is 5–6.6 % of travel off the line.
        let off = [0.535 * 22.213, 0.066 * 22.213, 0.0];
        assert!(!point_on_segment_interior(pre, post, off));
        // Beyond either endpoint is not a crossing.
        assert!(!point_on_segment_interior(pre, post, [-1.0, 0.0, 0.0]));
        assert!(!point_on_segment_interior(pre, post, [23.0, 0.0, 0.0]));
        // A zero-length travel has no interior.
        assert!(!point_on_segment_interior(pre, pre, pre));
    }

    /// A FAN OF ONE — the victim is a corner of exactly one triangle in this
    /// patch. The merge makes that triangle degenerate, so the rebuild is the
    /// EMPTY one: drop the triangle, add nothing. Measured 2026-08-20 to be the
    /// configuration of every Fig-11 site in the ring-reject family that
    /// reached its repair (R0011 / R0074 / R0085).
    #[test]
    fn rebuild_merge_fan_drops_a_fan_of_one() {
        let mesh = Mesh {
            verts: vec![
                Point3::new(0.0, 0.0, 0.0),
                Point3::new(1.0, 0.0, 0.0),
                Point3::new(1.0, 1.0, 0.0),
                Point3::new(0.0, 1.0, 0.0),
            ],
            // Vertex 0 is a corner of ONE triangle; vertex 2 of both.
            tris: vec![[0, 1, 2], [2, 3, 0]],
        };
        // The patch holds only the first triangle, so v0's fan there is a
        // single triangle with link [1, 2].
        let patch = plane_patch(vec![vec![0, 1, 2]], vec![0]);
        let r = rebuild_merge_fan(&mesh, 3, &patch, 0, 1, true).expect("fan-of-one merge");
        // Gate off, the same fan is the pre-2026-08-20 loud refusal.
        assert_eq!(
            rebuild_merge_fan(&mesh, 3, &patch, 0, 1, false),
            Err(ConstructError::FanNotSimple {
                patch: 3,
                victim: 0,
                reason: FanReason::Short { fan: 1, link: 2 }
            })
        );
        assert_eq!(r.old_tris, vec![0]);
        assert!(r.new_tris.is_empty(), "the merged triangle is degenerate");
        assert_eq!(r.dropped, [0].into_iter().collect());
    }

    /// A fan of one whose survivor is NOT on the link is still refused: that
    /// merge would RE-ANCHOR the triangle rather than degenerate it, which is
    /// not the local Fig-11 operation.
    #[test]
    fn rebuild_merge_fan_of_one_refuses_a_survivor_off_the_link() {
        let mesh = Mesh {
            verts: vec![
                Point3::new(0.0, 0.0, 0.0),
                Point3::new(1.0, 0.0, 0.0),
                Point3::new(1.0, 1.0, 0.0),
                Point3::new(0.0, 1.0, 0.0),
            ],
            tris: vec![[0, 1, 2], [2, 3, 0]],
        };
        let patch = plane_patch(vec![vec![0, 1, 2]], vec![0]);
        assert_eq!(
            rebuild_merge_fan(&mesh, 3, &patch, 0, 3, true),
            Err(ConstructError::FanSurvivorNotAdjacent {
                patch: 3,
                victim: 0
            })
        );
    }

    // ---- I13d rebuild_run_fan -----------------------------------------

    /// Hexagon boundary 0..5 fanned to interior 6; boundary run victims
    /// {1, 2} absorb into their junction 0: the region is the three
    /// triangles touching them, the link chains 3 → 6 → 0 with the survivor
    /// at an END, and the closure edge (0, 3) is exactly the absorbed
    /// boundary chain.
    #[test]
    fn rebuild_run_fan_absorbs_a_two_victim_boundary_run() {
        let mesh = Mesh {
            verts: vec![
                Point3::new(0.0, 0.0, 0.0),   // 0 = junction survivor
                Point3::new(0.4, -0.3, 0.0),  // 1 = victim (out-of-band)
                Point3::new(0.8, -0.15, 0.0), // 2 = victim (out-of-band)
                Point3::new(1.2, 0.0, 0.0),   // 3 = far boundary neighbour
                Point3::new(1.2, 1.0, 0.0),   // 4
                Point3::new(0.0, 1.0, 0.0),   // 5
                Point3::new(0.6, 0.5, 0.0),   // 6 = interior
            ],
            tris: vec![
                [0, 1, 6],
                [1, 2, 6],
                [2, 3, 6],
                [3, 4, 6],
                [4, 5, 6],
                [5, 0, 6],
            ],
        };
        let patch = plane_patch(vec![vec![0, 1, 2, 3, 4, 5]], vec![0, 1, 2, 3, 4, 5]);
        let victims: BTreeSet<u32> = [1, 2].into_iter().collect();
        let r = rebuild_run_fan(&mesh, 7, &patch, &victims, 0).expect("run rebuild");
        assert_eq!(r.patch, 7);
        assert_eq!(r.old_tris, vec![0, 1, 2], "the three triangles at the run");
        assert_eq!(r.dropped, victims);
        assert!(r.new_tris.iter().flatten().all(|v| !victims.contains(v)));
        assert_eq!(r.new_tris.len(), 1, "link 3→6→0 closes into one triangle");
        assert_eq!(
            r.new_tris[0].iter().copied().collect::<BTreeSet<u32>>(),
            [0u32, 3, 6].into()
        );
        let pos = |v: u32| -> Point3 { mesh.verts[v as usize] };
        let want = crate::stage4_splice::area_vector(&[[0, 1, 6], [1, 2, 6], [2, 3, 6]], &pos);
        let got = crate::stage4_splice::area_vector(&r.new_tris, &pos);
        assert!(crate::stage4_splice::dot3(want, got) > 0.0);
    }

    /// A survivor that is not a link ENDPOINT is refused: the closure edge
    /// would bypass it, stranding the junction off the new boundary.
    #[test]
    fn rebuild_run_fan_refuses_a_mid_link_survivor() {
        let mesh = Mesh {
            verts: vec![
                Point3::new(0.0, 0.0, 0.0),
                Point3::new(0.5, -0.3, 0.0), // 1 = victim
                Point3::new(1.0, 0.0, 0.0),
                Point3::new(1.5, 0.5, 0.0),
                Point3::new(0.0, 1.0, 0.0),
                Point3::new(0.5, 0.5, 0.0), // 5 = interior (mid-link)
            ],
            tris: vec![[0, 1, 5], [1, 2, 5], [2, 3, 5], [3, 4, 5], [4, 0, 5]],
        };
        let patch = plane_patch(vec![vec![0, 1, 2, 3, 4]], vec![0, 1, 2, 3, 4]);
        let victims: BTreeSet<u32> = [1].into_iter().collect();
        // Link chains 2 → 5 → 0; vertex 5 is interior to it.
        assert_eq!(
            rebuild_run_fan(&mesh, 3, &patch, &victims, 5),
            Err(ConstructError::FanSurvivorNotAdjacent {
                patch: 3,
                victim: 5
            })
        );
        // The endpoints ARE accepted.
        assert!(rebuild_run_fan(&mesh, 3, &patch, &victims, 0).is_ok());
        assert!(rebuild_run_fan(&mesh, 3, &patch, &victims, 2).is_ok());
    }

    /// Non-consecutive victims: the region here happens to be CONNECTED (the
    /// sandwiched vertex's triangles each touch a victim), so the link chains
    /// cleanly — and the sandwiched boundary vertex would be silently
    /// disconnected. The orphan guard is what refuses the shape.
    #[test]
    fn rebuild_run_fan_refuses_non_consecutive_victims() {
        let mesh = Mesh {
            verts: vec![
                Point3::new(0.0, 0.0, 0.0),
                Point3::new(0.5, -0.3, 0.0), // 1 = victim
                Point3::new(1.0, 0.0, 0.0),
                Point3::new(1.3, 0.7, 0.0), // 3 = victim, not adjacent to 1
                Point3::new(0.6, 1.2, 0.0),
                Point3::new(0.0, 1.0, 0.0),
                Point3::new(0.6, 0.4, 0.0), // 6 = interior
            ],
            tris: vec![
                [0, 1, 6],
                [1, 2, 6],
                [2, 3, 6],
                [3, 4, 6],
                [4, 5, 6],
                [5, 0, 6],
            ],
        };
        let patch = plane_patch(vec![vec![0, 1, 2, 3, 4, 5]], vec![0, 1, 2, 3, 4, 5]);
        let victims: BTreeSet<u32> = [1, 3].into_iter().collect();
        // The region is CONNECTED (vertex 2's triangles each touch a victim),
        // so the link chains cleanly — and boundary vertex 2, sandwiched
        // between the non-consecutive victims, would be silently
        // disconnected. The orphan guard is what refuses this shape.
        match rebuild_run_fan(&mesh, 0, &patch, &victims, 0) {
            Err(ConstructError::FanNotSimple {
                reason: FanReason::Orphaned { vertex, .. },
                ..
            }) => assert_eq!(vertex, 2),
            other => panic!("expected an Orphaned refusal, got {other:?}"),
        }
    }

    /// The doubled-back spur shape from R0003 face 467: the boundary walks
    /// out past the junction and back over the same territory. The region
    /// polygon (junction → far side, through the interior link) is clean
    /// even though the removed boundary chain is a hairpin.
    #[test]
    fn rebuild_run_fan_repairs_a_hairpin_spur() {
        let mesh = Mesh {
            verts: vec![
                Point3::new(0.0, 0.0, 0.0),    // 0 = junction
                Point3::new(-0.4, -0.55, 0.0), // 1 = deep victim
                Point3::new(-0.1, -0.3, 0.0),  // 2 = shallow victim
                Point3::new(1.0, 0.0, 0.0),    // 3 = far neighbour
                Point3::new(0.5, 0.9, 0.0),    // 4
                Point3::new(-0.5, 0.6, 0.0),   // 5
                Point3::new(0.2, 0.35, 0.0),   // 6 = interior
            ],
            tris: vec![
                [0, 1, 6],
                [1, 2, 6],
                [2, 3, 6],
                [3, 4, 6],
                [4, 5, 6],
                [5, 0, 6],
            ],
        };
        let patch = plane_patch(vec![vec![0, 1, 2, 3, 4, 5]], vec![0, 1, 2, 3, 4, 5]);
        let victims: BTreeSet<u32> = [1, 2].into_iter().collect();
        let r = rebuild_run_fan(&mesh, 0, &patch, &victims, 0).expect("hairpin absorbed");
        assert_eq!(r.old_tris, vec![0, 1, 2]);
        assert!(r.new_tris.iter().flatten().all(|v| !victims.contains(v)));
    }

    // ---- §I13(f) rebuild_rehome_fan ------------------------------------

    /// The neighbor-band shape: the survivor has NO triangle on this patch
    /// (mesh vertex 7 rides on other patches), so it JOINS — appended to
    /// the link polygon where the victim's arc was, evaluated at its mint.
    #[test]
    fn rebuild_rehome_fan_survivor_joins_the_patch() {
        let mesh = Mesh {
            verts: vec![
                Point3::new(0.0, 0.0, 0.0),   // 0
                Point3::new(0.4, -0.3, 0.0),  // 1 = victim (j_rim)
                Point3::new(0.8, -0.15, 0.0), // 2
                Point3::new(1.2, 0.0, 0.0),   // 3
                Point3::new(1.2, 1.0, 0.0),   // 4
                Point3::new(0.0, 1.0, 0.0),   // 5
                Point3::new(0.6, 0.5, 0.0),   // 6 = interior
                Point3::new(9.0, 9.0, 9.0),   // 7 = j_cut, on OTHER patches
            ],
            tris: vec![
                [0, 1, 6],
                [1, 2, 6],
                [2, 3, 6],
                [3, 4, 6],
                [4, 5, 6],
                [5, 0, 6],
            ],
        };
        let patch = plane_patch(vec![vec![0, 1, 2, 3, 4, 5]], vec![0, 1, 2, 3, 4, 5]);
        let victims: BTreeSet<u32> = [1].into_iter().collect();
        let mint = Point3::new(0.5, -0.25, 0.0);
        let r = rebuild_rehome_fan(&mesh, 7, &patch, &victims, 7, mint).expect("joins");
        assert_eq!(r.old_tris, vec![0, 1], "the two triangles at the victim");
        assert_eq!(r.dropped, victims);
        assert!(r.new_tris.iter().flatten().all(|v| !victims.contains(v)));
        assert_eq!(r.new_tris.len(), 2, "quad 2→6→0→7 splits into two");
        let used: BTreeSet<u32> = r.new_tris.iter().flatten().copied().collect();
        assert_eq!(used, [0u32, 2, 6, 7].into(), "the survivor joined");
        // Orientation matches the old region at the survivor's MINT.
        let pos = |v: u32| -> Point3 {
            if v == 7 {
                mint
            } else {
                mesh.verts[v as usize]
            }
        };
        let want = crate::stage4_splice::area_vector(&[[0, 1, 6], [1, 2, 6]], &|v: u32| {
            mesh.verts[v as usize]
        });
        let got = crate::stage4_splice::area_vector(&r.new_tris, &pos);
        assert!(crate::stage4_splice::dot3(want, got) > 0.0);
    }

    /// With the survivor already FLANKING the link and its override equal to
    /// its mesh position, the rehome fan reduces exactly to the run fan.
    #[test]
    fn rebuild_rehome_fan_flanking_reduces_to_the_run_fan() {
        let mesh = Mesh {
            verts: vec![
                Point3::new(0.0, 0.0, 0.0),
                Point3::new(0.4, -0.3, 0.0),
                Point3::new(0.8, -0.15, 0.0),
                Point3::new(1.2, 0.0, 0.0),
                Point3::new(1.2, 1.0, 0.0),
                Point3::new(0.0, 1.0, 0.0),
                Point3::new(0.6, 0.5, 0.0),
            ],
            tris: vec![
                [0, 1, 6],
                [1, 2, 6],
                [2, 3, 6],
                [3, 4, 6],
                [4, 5, 6],
                [5, 0, 6],
            ],
        };
        let patch = plane_patch(vec![vec![0, 1, 2, 3, 4, 5]], vec![0, 1, 2, 3, 4, 5]);
        let victims: BTreeSet<u32> = [1, 2].into_iter().collect();
        let run = rebuild_run_fan(&mesh, 7, &patch, &victims, 0).expect("run");
        let reh = rebuild_rehome_fan(&mesh, 7, &patch, &victims, 0, mesh.verts[0]).expect("rehome");
        assert_eq!(run.old_tris, reh.old_tris);
        assert_eq!(run.new_tris, reh.new_tris);
        assert_eq!(run.dropped, reh.dropped);
    }

    /// A survivor present MID-link keeps the refusal in rehome mode — that
    /// shape is a genuine pinch, not a join.
    #[test]
    fn rebuild_rehome_fan_still_refuses_a_mid_link_survivor() {
        let mesh = Mesh {
            verts: vec![
                Point3::new(0.0, 0.0, 0.0),
                Point3::new(0.5, -0.3, 0.0), // 1 = victim
                Point3::new(1.0, 0.0, 0.0),
                Point3::new(1.5, 0.5, 0.0),
                Point3::new(0.0, 1.0, 0.0),
                Point3::new(0.5, 0.5, 0.0), // 5 = interior (mid-link)
            ],
            tris: vec![[0, 1, 5], [1, 2, 5], [2, 3, 5], [3, 4, 5], [4, 0, 5]],
        };
        let patch = plane_patch(vec![vec![0, 1, 2, 3, 4]], vec![0, 1, 2, 3, 4]);
        let victims: BTreeSet<u32> = [1].into_iter().collect();
        let err = rebuild_rehome_fan(&mesh, 0, &patch, &victims, 5, mesh.verts[5]).unwrap_err();
        assert!(
            matches!(err, ConstructError::FanSurvivorNotAdjacent { .. }),
            "{err:?}"
        );
    }

    /// A single sliver triangle (victim, survivor, other): under the merge
    /// it degenerates, so the rehome rebuild is EMPTY — sliver deleted,
    /// survivor→other edge becomes the boundary. The run fan keeps its
    /// Short refusal on the same shape.
    #[test]
    fn rebuild_rehome_fan_deletes_a_single_sliver_triangle() {
        let mesh = Mesh {
            verts: vec![
                Point3::new(0.0, 0.0, 0.0),   // 0 = survivor
                Point3::new(0.5, -0.05, 0.0), // 1 = victim (sliver apex)
                Point3::new(1.0, 0.0, 0.0),   // 2
                Point3::new(0.5, 1.0, 0.0),   // 3
            ],
            tris: vec![[0, 1, 2], [0, 2, 3]],
        };
        let patch = plane_patch(vec![vec![0, 1, 2, 3]], vec![0, 1]);
        let victims: BTreeSet<u32> = [1].into_iter().collect();
        let r = rebuild_rehome_fan(&mesh, 4, &patch, &victims, 0, mesh.verts[0])
            .expect("sliver deletion");
        assert_eq!(r.old_tris, vec![0]);
        assert!(r.new_tris.is_empty(), "{:?}", r.new_tris);
        assert_eq!(r.dropped, victims);
        let err = rebuild_run_fan(&mesh, 4, &patch, &victims, 0).unwrap_err();
        assert!(
            matches!(
                err,
                ConstructError::FanNotSimple {
                    reason: FanReason::Short { fan: 1, link: 2 },
                    ..
                }
            ),
            "{err:?}"
        );
    }

    // ---- I13e rebuild_group_fan ---------------------------------------

    /// The R0003 wall-patch-475 shape, scaled down: two strips' overruns
    /// (victims 2 and 5) each fold across the OTHER strip's territory, so
    /// each single fan's link polygon contains the partner's victim and
    /// self-intersects — both per-site CDTs refuse, in either order. The
    /// GROUP region's boundary cycle carries both victims as arcs flanked
    /// by their own survivors; deleting both arcs leaves a simple quad.
    fn interlocked_pair() -> (Mesh, SplicePatch) {
        let mesh = Mesh {
            verts: vec![
                Point3::new(0.0, 0.0, 0.0),  // 0 = far neighbour of victim 2
                Point3::new(0.0, 2.0, 0.0),  // 1 = survivor of victim 2
                Point3::new(0.0, 4.0, 0.0),  // 2 = victim (strip A)
                Point3::new(10.0, 3.0, 0.0), // 3 = far neighbour of victim 5
                Point3::new(10.0, 1.0, 0.0), // 4 = survivor of victim 5
                Point3::new(10.0, 0.5, 0.0), // 5 = victim (strip B)
                Point3::new(5.0, -2.0, 0.0), // 6 = spectator (outside region)
            ],
            tris: vec![
                [2, 0, 4], // strip A's fold reaching into B's territory
                [2, 4, 5], // the interlock: both victims in one triangle
                [2, 5, 1], // ditto
                [5, 3, 1], // strip B's fold reaching into A's territory
                [4, 0, 6], // spectator: keeps (0, 4) a patch-interior edge
            ],
        };
        let patch = plane_patch(vec![vec![2, 0, 6, 4, 5, 3, 1]], vec![0, 1, 2, 3, 4]);
        (mesh, patch)
    }

    #[test]
    fn rebuild_group_fan_absorbs_the_interlocked_pair() {
        let (mesh, patch) = interlocked_pair();
        // The interlock property, measured: BOTH single fans decline (each
        // link polygon contains the partner's still-folded victim and
        // self-intersects), in either repair order.
        for (victim, survivor) in [(2, 1), (5, 4)] {
            assert!(
                matches!(
                    rebuild_merge_fan(&mesh, 9, &patch, victim, survivor, false),
                    Err(ConstructError::Cdt { .. })
                ),
                "single fan of v{victim} must self-intersect"
            );
        }
        let sites: Vec<(BTreeSet<u32>, u32)> = vec![
            ([2].into_iter().collect(), 1),
            ([5].into_iter().collect(), 4),
        ];
        let r = rebuild_group_fan(&mesh, 9, &patch, &sites).expect("group rebuild");
        assert_eq!(r.patch, 9);
        assert_eq!(r.old_tris, vec![0, 1, 2, 3], "the four folded triangles");
        assert_eq!(r.dropped, [2u32, 5].into_iter().collect::<BTreeSet<u32>>());
        assert!(r.new_tris.iter().flatten().all(|v| *v != 2 && *v != 5));
        assert_eq!(r.new_tris.len(), 2, "the quad 0-4-3-1 as two triangles");
        let pos = |v: u32| -> Point3 { mesh.verts[v as usize] };
        let old: Vec<[u32; 3]> = r.old_tris.iter().map(|&t| mesh.tris[t as usize]).collect();
        let want = crate::stage4_splice::area_vector(&old, &pos);
        let got = crate::stage4_splice::area_vector(&r.new_tris, &pos);
        assert!(crate::stage4_splice::dot3(want, got) > 0.0);
    }

    /// A holder where only ONE of the group's sites is present (the strip
    /// cone in the real case): the group rebuild degenerates to exactly the
    /// run-level rebuild, and `dropped` lists only the present victims —
    /// the absent site's victim was never referenced by this patch.
    #[test]
    fn rebuild_group_fan_reduces_to_the_run_fan_when_one_site_is_present() {
        let mesh = Mesh {
            verts: vec![
                Point3::new(0.0, 0.0, 0.0),   // 0 = junction survivor
                Point3::new(0.4, -0.3, 0.0),  // 1 = victim
                Point3::new(0.8, -0.15, 0.0), // 2 = victim
                Point3::new(1.2, 0.0, 0.0),
                Point3::new(1.2, 1.0, 0.0),
                Point3::new(0.0, 1.0, 0.0),
                Point3::new(0.6, 0.5, 0.0), // 6 = interior
            ],
            tris: vec![
                [0, 1, 6],
                [1, 2, 6],
                [2, 3, 6],
                [3, 4, 6],
                [4, 5, 6],
                [5, 0, 6],
            ],
        };
        let patch = plane_patch(vec![vec![0, 1, 2, 3, 4, 5]], vec![0, 1, 2, 3, 4, 5]);
        let run_victims: BTreeSet<u32> = [1, 2].into_iter().collect();
        let run = rebuild_run_fan(&mesh, 7, &patch, &run_victims, 0).expect("run rebuild");
        // Site B's victim (99) has no triangle on this patch at all.
        let sites: Vec<(BTreeSet<u32>, u32)> = vec![
            ([1, 2].into_iter().collect(), 0),
            ([99].into_iter().collect(), 98),
        ];
        let group = rebuild_group_fan(&mesh, 7, &patch, &sites).expect("group rebuild");
        assert_eq!(group.old_tris, run.old_tris);
        assert_eq!(group.dropped, run.dropped, "present victims only");
        let norm = |tris: &[[u32; 3]]| -> BTreeSet<BTreeSet<u32>> {
            tris.iter().map(|t| t.iter().copied().collect()).collect()
        };
        assert_eq!(norm(&group.new_tris), norm(&run.new_tris));
    }

    /// Victims of DIFFERENT sites adjacent on the boundary fuse into one
    /// arc: there is no per-site closure edge that absorbs a fused arc, so
    /// the shape is refused loudly, never guessed.
    #[test]
    fn rebuild_group_fan_refuses_fused_arcs() {
        let mesh = Mesh {
            verts: vec![
                Point3::new(0.0, 0.0, 0.0),
                Point3::new(0.4, -0.3, 0.0),  // 1 = site A's victim
                Point3::new(0.8, -0.15, 0.0), // 2 = site B's victim, adjacent
                Point3::new(1.2, 0.0, 0.0),
                Point3::new(1.2, 1.0, 0.0),
                Point3::new(0.0, 1.0, 0.0),
                Point3::new(0.6, 0.5, 0.0),
            ],
            tris: vec![
                [0, 1, 6],
                [1, 2, 6],
                [2, 3, 6],
                [3, 4, 6],
                [4, 5, 6],
                [5, 0, 6],
            ],
        };
        let patch = plane_patch(vec![vec![0, 1, 2, 3, 4, 5]], vec![0, 1, 2, 3, 4, 5]);
        let sites: Vec<(BTreeSet<u32>, u32)> = vec![
            ([1].into_iter().collect(), 0),
            ([2].into_iter().collect(), 3),
        ];
        match rebuild_group_fan(&mesh, 0, &patch, &sites) {
            Err(ConstructError::FanNotSimple {
                reason: FanReason::ArcMismatch { arcs, sites, .. },
                ..
            }) => {
                assert_eq!((arcs, sites), (1, 2), "one fused arc, two sites");
            }
            other => panic!("expected an ArcMismatch refusal, got {other:?}"),
        }
    }

    /// Each site's survivor must flank its OWN arc: a survivor that flanks
    /// the partner's arc instead would be stranded off its absorbed span.
    #[test]
    fn rebuild_group_fan_refuses_a_survivor_off_its_own_arc() {
        let (mesh, patch) = interlocked_pair();
        // Victim 5's survivor wrongly set to 1 (which flanks victim 2's arc).
        let sites: Vec<(BTreeSet<u32>, u32)> = vec![
            ([2].into_iter().collect(), 1),
            ([5].into_iter().collect(), 1),
        ];
        assert_eq!(
            rebuild_group_fan(&mesh, 9, &patch, &sites),
            Err(ConstructError::FanSurvivorNotAdjacent {
                patch: 9,
                victim: 1
            })
        );
    }

    /// A region vertex fully enclosed by the group's triangles is neither a
    /// victim nor on the boundary cycle — the rebuild would silently
    /// disconnect it, so the orphan guard refuses.
    #[test]
    fn rebuild_group_fan_refuses_an_enclosed_interior_vertex() {
        let mesh = Mesh {
            verts: vec![
                Point3::new(0.0, 0.0, 0.0),  // 0
                Point3::new(0.0, 2.0, 0.0),  // 1
                Point3::new(0.0, 4.0, 0.0),  // 2 = victim A
                Point3::new(10.0, 3.0, 0.0), // 3
                Point3::new(10.0, 1.0, 0.0), // 4
                Point3::new(10.0, 0.5, 0.0), // 5 = victim B
                Point3::new(5.0, 1.5, 0.0),  // 6 = enclosed interior vertex
            ],
            tris: vec![
                [2, 0, 4],
                [2, 4, 6],
                [4, 5, 6],
                [2, 6, 5],
                [2, 5, 1],
                [5, 3, 1],
            ],
        };
        let patch = plane_patch(vec![vec![2, 0, 4, 5, 3, 1]], vec![0, 1, 2, 3, 4, 5]);
        let sites: Vec<(BTreeSet<u32>, u32)> = vec![
            ([2].into_iter().collect(), 1),
            ([5].into_iter().collect(), 4),
        ];
        match rebuild_group_fan(&mesh, 0, &patch, &sites) {
            Err(ConstructError::FanNotSimple {
                reason: FanReason::Orphaned { vertex, .. },
                ..
            }) => assert_eq!(vertex, 6),
            other => panic!("expected an Orphaned refusal, got {other:?}"),
        }
    }

    /// Three sites chained by shared triangles (the ladder): all three arcs
    /// absorb in ONE rebuild, each closure edge flanked by its own survivor.
    #[test]
    fn rebuild_group_fan_absorbs_a_three_site_ladder() {
        // Convex octagon fanned from boundary vertex 0; victims 0, 2, 5.
        // Triangle (0,1,2) links sites B and A; (0,4,5) links B and C.
        let octagon = |k: usize| {
            let a = (22.5 + 45.0 * k as f64).to_radians();
            Point3::new(a.cos(), a.sin(), 0.0)
        };
        let mesh = Mesh {
            verts: (0..8).map(octagon).collect(),
            tris: vec![
                [0, 1, 2],
                [0, 2, 3],
                [0, 3, 4],
                [0, 4, 5],
                [0, 5, 6],
                [0, 6, 7],
            ],
        };
        let patch = plane_patch(vec![vec![0, 1, 2, 3, 4, 5, 6, 7]], (0..6).collect());
        let sites: Vec<(BTreeSet<u32>, u32)> = vec![
            ([2].into_iter().collect(), 3),
            ([0].into_iter().collect(), 7),
            ([5].into_iter().collect(), 4),
        ];
        let r = rebuild_group_fan(&mesh, 4, &patch, &sites).expect("ladder rebuild");
        assert_eq!(r.old_tris, vec![0, 1, 2, 3, 4, 5], "every triangle folded");
        assert_eq!(
            r.dropped,
            [0u32, 2, 5].into_iter().collect::<BTreeSet<u32>>()
        );
        assert!(r.new_tris.iter().flatten().all(|v| ![0, 2, 5].contains(v)));
        assert_eq!(r.new_tris.len(), 3, "pentagon 1-3-4-6-7 as three triangles");
    }

    /// The decline NAMES its condition: a victim whose fan triangles leave it
    /// toward the same link vertex twice is a `Pinch`, not a `Split` — two
    /// populations that would need different repairs, so the census must not
    /// see one opaque count.
    #[test]
    fn rebuild_merge_fan_names_a_pinch_apart_from_a_split() {
        let mesh = Mesh {
            verts: vec![
                Point3::new(0.0, 0.0, 0.0),
                Point3::new(1.0, 0.0, 0.0),
                Point3::new(1.0, 1.0, 0.0),
                Point3::new(0.0, 1.0, 0.0),
            ],
            // Both triangles leave the victim toward vertex 1: the link is not
            // a path (duplicate source), which is the pinch condition.
            tris: vec![[0, 1, 2], [0, 1, 3]],
        };
        let patch = plane_patch(vec![vec![1, 2, 0, 3]], vec![0, 1]);
        assert_eq!(
            rebuild_merge_fan(&mesh, 7, &patch, 0, 1, false),
            Err(ConstructError::FanNotSimple {
                patch: 7,
                victim: 0,
                reason: FanReason::Pinch { fan: 2 }
            })
        );
    }

    /// A cylinder fan straddling the θ = ±π branch cut: the unwrap is against
    /// the VICTIM's own branch, so the fan is contiguous and the rebuild
    /// succeeds — this is why the merge does not need `unwrap_theta`'s
    /// non-encircling precondition.
    #[test]
    fn rebuild_merge_fan_unwraps_across_the_branch_cut() {
        // Unit cylinder about +z. Victim at theta = pi (the cut), neighbours
        // just either side of it.
        let at = |th: f64, z: f64| Point3::new(th.cos(), th.sin(), z);
        use std::f64::consts::PI;
        let mesh = Mesh {
            verts: vec![
                at(PI, 0.0),         // 0 — victim, exactly on the cut
                at(PI - 0.3, 0.0),   // 1
                at(PI - 0.15, 0.6),  // 2
                at(-PI + 0.15, 0.6), // 3  (== PI + 0.15, other branch)
                at(-PI + 0.3, 0.0),  // 4
            ],
            tris: vec![[0, 1, 2], [0, 2, 3], [0, 3, 4]],
        };
        let patch = SplicePatch {
            cycles: vec![vec![1, 0, 4, 3, 2]],
            tris: vec![0, 1, 2],
            surface: Surface::Cylinder {
                axis_point: Point3::new(0.0, 0.0, 0.0),
                axis_dir: Vector3::new(0.0, 0.0, 1.0),
                radius: 1.0,
            },
        };
        let r = rebuild_merge_fan(&mesh, 1, &patch, 0, 1, false).expect("branch-cut fan rebuild");
        assert_eq!(r.dropped, [0u32].into());
        assert!(r.new_tris.iter().flatten().all(|&v| v != 0));
        assert_eq!(r.new_tris.len(), 2, "link 1,2,3,4 is two triangles");
    }

    #[test]
    fn rebuild_patch_planar_keeps_boundary_edges_and_drops_interiors() {
        let mesh = square_fan_mesh();
        let patch = plane_patch(vec![vec![0, 1, 2, 3]], vec![0, 1, 2, 3]);
        let r = rebuild_patch_planar(&mesh, 7, &patch).expect("planar rebuild");
        assert_eq!(r.patch, 7);
        assert_eq!(r.dropped, [4u32].into());
        assert_eq!(r.new_tris.len(), 2, "square CDT is two triangles");
        // Every boundary edge of the cycle survives edge-for-edge.
        let mut edges: BTreeSet<(u32, u32)> = BTreeSet::new();
        for t in &r.new_tris {
            for k in 0..3 {
                let (s, e) = (t[k], t[(k + 1) % 3]);
                edges.insert((s.min(e), s.max(e)));
            }
        }
        for e in [(0u32, 1u32), (1, 2), (2, 3), (0, 3)] {
            assert!(edges.contains(&e), "boundary edge {e:?} must survive");
        }
        // Orientation matches the original fan's outward (+z) sense.
        let pos = |v: u32| mesh.verts[v as usize];
        let got = area_vector(&r.new_tris, &pos);
        assert!(got[2] > 0.0, "rebuilt patch keeps the +z outward sense");
        assert_eq!(r.plan_verts, 5);
        assert_eq!(r.plan_tris, 4);
    }

    #[test]
    fn rebuild_patch_planar_refuses_an_unchartable_patch() {
        let mesh = square_fan_mesh();
        let patch = SplicePatch {
            cycles: vec![vec![0, 1, 2, 3]],
            tris: vec![0, 1, 2, 3],
            surface: Surface::Sphere {
                center: Point3::new(0.0, 0.0, 0.0),
                radius: 1.0,
            },
        };
        assert_eq!(
            rebuild_patch_planar(&mesh, 3, &patch),
            Err(ConstructError::NonPlanarPatch { patch: 3 })
        );
    }

    fn cylinder_patch_mesh(theta0: f64) -> (Mesh, SplicePatch) {
        // Unit cylinder about z; patch spans θ ∈ [theta0, theta0+0.8],
        // z ∈ [0, 1], with one INTERIOR vertex mid-patch fanned by the
        // triangulation. I2a must carry that vertex through the rebuild.
        let cyl = |theta: f64, z: f64| Point3::new(theta.cos(), theta.sin(), z);
        let (t0, t1) = (theta0, theta0 + 0.8);
        let tm = theta0 + 0.4;
        let mesh = Mesh {
            verts: vec![
                cyl(t0, 0.0), // 0
                cyl(t1, 0.0), // 1
                cyl(t1, 1.0), // 2
                cyl(t0, 1.0), // 3
                cyl(tm, 0.5), // 4 interior
            ],
            tris: vec![[0, 1, 4], [1, 2, 4], [2, 3, 4], [3, 0, 4]],
        };
        let patch = SplicePatch {
            cycles: vec![vec![0, 1, 2, 3]],
            tris: vec![0, 1, 2, 3],
            surface: Surface::Cylinder {
                axis_point: Point3::new(0.0, 0.0, 0.0),
                axis_dir: Vector3::new(0.0, 0.0, 1.0),
                radius: 1.0,
            },
        };
        (mesh, patch)
    }

    #[test]
    fn rebuild_cylinder_patch_carries_interior_vertices() {
        let (mesh, patch) = cylinder_patch_mesh(0.2);
        let r = rebuild_patch_planar(&mesh, 5, &patch).expect("cylinder rebuild");
        assert!(r.dropped.is_empty(), "interior vertex must be CARRIED");
        assert!(
            r.new_tris.iter().flatten().any(|&v| v == 4),
            "interior vertex 4 appears in the rebuilt triangulation"
        );
        // Orientation matches the original outward sense.
        let pos = |v: u32| mesh.verts[v as usize];
        let want = area_vector(
            &patch
                .tris
                .iter()
                .map(|&t| mesh.tris[t as usize])
                .collect::<Vec<_>>(),
            &pos,
        );
        let got = area_vector(&r.new_tris, &pos);
        assert!(dot3(want, got) > 0.0, "outward sense preserved");
    }

    /// The kv6b revolve∪box wall in miniature: cylinder r=2 about z, wall
    /// θ ∈ [0, π/2] × z ∈ [0, height] with a notch [0, π/16] cut into the
    /// θ=0 edge across the middle third of the height (the box bite). Both
    /// rims are densely sampled (π/8 columns) but the notch verts are the
    /// ONLY mid-height vertices, and there are NO interior vertices: the
    /// old banding's fidelity lives purely in rim-to-rim CONNECTIVITY,
    /// which a chart CDT discards — chart Delaunay (θ-radians ×
    /// world-units, aspect-distorted) fans the mid-height notch verts
    /// across wide θ, minting 3D secants that shave the cylindrical bulge.
    fn notched_drum(height: f64) -> (Mesh, SplicePatch) {
        let cyl = |theta: f64, z: f64| Point3::new(2.0 * theta.cos(), 2.0 * theta.sin(), z);
        let s = std::f64::consts::PI / 8.0;
        let n = std::f64::consts::PI / 16.0;
        let (z1, z2) = (height / 3.0, 2.0 * height / 3.0);
        let mesh = Mesh {
            verts: vec![
                cyl(0.0, 0.0),        // 0  rim z=0
                cyl(s, 0.0),          // 1
                cyl(2.0 * s, 0.0),    // 2
                cyl(3.0 * s, 0.0),    // 3
                cyl(4.0 * s, 0.0),    // 4  far ruling bottom
                cyl(4.0 * s, height), // 5  far ruling top
                cyl(3.0 * s, height), // 6  rim z=height
                cyl(2.0 * s, height), // 7
                cyl(s, height),       // 8
                cyl(0.0, height),     // 9  near ruling top
                cyl(0.0, z2),         // 10 notch NW
                cyl(n, z2),           // 11 notch NE
                cyl(n, z1),           // 12 notch SE
                cyl(0.0, z1),         // 13 notch SW
            ],
            tris: vec![
                // band θ ∈ [0, π/8] around the notch
                [0, 1, 13],
                [13, 1, 12],
                [12, 1, 11],
                [11, 1, 8],
                [11, 8, 10],
                [10, 8, 9],
                // full-height bands, θ-span π/8 each
                [1, 2, 8],
                [2, 7, 8],
                [2, 3, 7],
                [3, 6, 7],
                [3, 4, 6],
                [4, 5, 6],
            ],
        };
        let patch = SplicePatch {
            cycles: vec![vec![0, 1, 2, 3, 4, 5, 6, 7, 8, 9, 10, 11, 12, 13]],
            tris: (0..12).collect(),
            surface: Surface::Cylinder {
                axis_point: Point3::new(0.0, 0.0, 0.0),
                axis_dir: Vector3::new(0.0, 0.0, 1.0),
                radius: 2.0,
            },
        };
        (mesh, patch)
    }

    #[test]
    fn rebuild_cylinder_chord_gate_seeds_wide_theta_rebuild() {
        // I2d + I2e: the seedless rebuild of the notched drum certifies
        // coarser than the banding it replaces (the kv6b secant class), so
        // the escalation retries with an interior seed grid at the patch's
        // own θ-arc sampling scale — and the seeded rebuild must PASS the
        // d(T) gate, carrying chart-lifted (exactly on-surface) new
        // vertices referenced as plan_verts + k.
        let (mesh, patch) = notched_drum(3.0);
        let r = rebuild_patch_planar(&mesh, 4, &patch).expect("seeded rebuild passes the gate");
        assert!(
            !r.new_verts.is_empty(),
            "the wide-θ rebuild is only certifiable WITH seeds"
        );
        for (k, p) in r.new_verts.iter().enumerate() {
            let rad = (p.x() * p.x() + p.y() * p.y()).sqrt();
            assert!(
                (rad - 2.0).abs() <= 1e-12,
                "seed {k} off the cylinder: r = {rad}"
            );
        }
        let plan_verts = mesh.verts.len() as u32;
        let seeds_referenced: BTreeSet<u32> = r
            .new_tris
            .iter()
            .flatten()
            .copied()
            .filter(|&v| v >= plan_verts)
            .collect();
        assert_eq!(
            seeds_referenced.len(),
            r.new_verts.len(),
            "every appended seed vertex is used by the new triangulation"
        );
        assert!(
            seeds_referenced
                .iter()
                .all(|&v| ((v - plan_verts) as usize) < r.new_verts.len()),
            "seed references stay inside the appended block"
        );
        assert!(r.dropped.is_empty(), "no boundary vertex may be lost");
    }

    #[test]
    fn rebuild_cylinder_squashed_drum_passes_seedless() {
        // The drum squashed to z ∈ [0, 0.01]: the chart aspect inverts —
        // θ columns are now FAT relative to the height, so chart Delaunay
        // ladders them naturally and the seedless rebuild certifies within
        // the old budget. Attempt 0 must pass WITHOUT seeds (the pre-I2e
        // path is byte-identical wherever it already passes the gate).
        let (mesh, patch) = notched_drum(0.01);
        let r = rebuild_patch_planar(&mesh, 4, &patch).expect("squashed drum passes");
        assert!(
            r.new_verts.is_empty(),
            "no seeds when the gate passes seedless"
        );
    }

    #[test]
    fn i2e_seed_grid_populates_interior_with_clearance() {
        // Chart rectangle θ ∈ [0, 1] × z ∈ [0, 2] on a radius-2 cylinder
        // (arc width 2.0), spacing 0.5: seeds must be strictly interior
        // with ≥ 0.125 arc-metric clearance from every edge.
        let pool = vec![
            Point2::new(0.0, 0.0),
            Point2::new(1.0, 0.0),
            Point2::new(1.0, 2.0),
            Point2::new(0.0, 2.0),
        ];
        let outer = vec![0u32, 1, 2, 3];
        let seeds = i2e_seed_grid(&pool, &outer, &[], 2.0, 0.5);
        assert!(!seeds.is_empty(), "an open rectangle must seed");
        for s in &seeds {
            let arc = s.x() * 2.0;
            assert!(arc >= 0.125 - 1e-12 && 2.0 - arc >= 0.125 - 1e-12);
            assert!(s.y() >= 0.125 - 1e-12 && 2.0 - s.y() >= 0.125 - 1e-12);
        }
        // A hole swallowing the middle excludes its seeds.
        let pool_h = [
            pool.clone(),
            vec![
                Point2::new(0.2, 0.4),
                Point2::new(0.8, 0.4),
                Point2::new(0.8, 1.6),
                Point2::new(0.2, 1.6),
            ],
        ]
        .concat();
        let hole = vec![4u32, 5, 6, 7];
        let seeded = i2e_seed_grid(&pool_h, &outer, std::slice::from_ref(&hole), 2.0, 0.5);
        for s in &seeded {
            let in_hole = s.x() > 0.2 && s.x() < 0.8 && s.y() > 0.4 && s.y() < 1.6;
            assert!(!in_hole, "seed {s:?} landed inside the hole");
        }
    }

    #[test]
    fn i2e_seed_grid_returns_empty_on_degenerate_input() {
        let pool = vec![
            Point2::new(0.0, 0.0),
            Point2::new(1.0, 0.0),
            Point2::new(1.0, 2.0),
            Point2::new(0.0, 2.0),
        ];
        let outer = vec![0u32, 1, 2, 3];
        assert!(i2e_seed_grid(&pool, &outer, &[], 2.0, 0.0).is_empty());
        assert!(i2e_seed_grid(&pool, &outer, &[], 2.0, f64::NAN).is_empty());
        assert!(i2e_seed_grid(&pool, &outer, &[], 0.0, 0.5).is_empty());
        // A spacing far below the runaway backstop's grid bound refuses
        // loudly-by-decline instead of exploding.
        assert!(i2e_seed_grid(&pool, &outer, &[], 2.0, 1e-6).is_empty());
    }

    #[test]
    fn apply_rebuild_batch_appends_seed_vertices_and_remaps() {
        // The write-back appends new_verts and points plan_verts + k at the
        // appended block; attribution stays in lockstep with mesh.tris.
        let (mut mesh, patch) = notched_drum(3.0);
        let r = rebuild_patch_planar(&mesh, 4, &patch).expect("seeded rebuild");
        let n_verts_before = mesh.verts.len();
        let n_seeds = r.new_verts.len();
        assert!(n_seeds > 0);
        let mut attribution = TriangleAttributionMap {
            attributions: vec![attr(crate::brep::InputId::A, 7); mesh.tris.len()],
        };
        apply_rebuild_batch(
            &mut mesh,
            &mut attribution,
            std::slice::from_ref(&r),
            &BTreeMap::new(),
        )
        .expect("write-back");
        assert_eq!(mesh.verts.len(), n_verts_before + n_seeds);
        assert_eq!(mesh.tris.len(), attribution.attributions.len());
        for (k, p) in r.new_verts.iter().enumerate() {
            assert_eq!(
                mesh.verts[n_verts_before + k],
                *p,
                "seed {k} appended in order"
            );
        }
        for tri in &mesh.tris {
            for &v in tri {
                assert!(
                    (v as usize) < mesh.verts.len(),
                    "no dangling vertex reference after remap"
                );
            }
        }
    }

    #[test]
    fn rebuild_cylinder_patch_unwraps_the_theta_seam() {
        // Patch straddling θ = ±π: the unwrap must place all vertices on one
        // branch and the rebuild must succeed with the interior carried.
        let (mesh, patch) = cylinder_patch_mesh(std::f64::consts::PI - 0.4);
        let r = rebuild_patch_planar(&mesh, 6, &patch).expect("straddling rebuild");
        assert!(r.dropped.is_empty());
        assert!(r.new_tris.iter().flatten().any(|&v| v == 4));
    }

    #[test]
    fn rebuild_patch_planar_reports_a_femto_pair_as_duplicate_vertex() {
        // Two distinct boundary vertices whose chart projections coincide:
        // the CDT sees one point twice — the femto-pair junction family stays
        // a LOUD decline, not a weld.
        let mesh = Mesh {
            verts: vec![
                Point3::new(0.0, 0.0, 0.0),
                Point3::new(1.0, 0.0, 0.0),
                Point3::new(1.0, 0.0, 0.0),
                Point3::new(0.5, 1.0, 0.0),
            ],
            tris: vec![[0, 1, 2], [0, 2, 3]],
        };
        let patch = plane_patch(vec![vec![0, 1, 2, 3]], vec![0, 1]);
        assert_eq!(
            rebuild_patch_planar(&mesh, 0, &patch),
            Err(ConstructError::Cdt {
                patch: 0,
                error: CdtError::DuplicateVertex
            })
        );
    }

    #[test]
    fn apply_rebuild_batch_swaps_tris_and_keeps_attribution_lockstep() {
        use crate::brep::InputId;
        let mut mesh = square_fan_mesh();
        // Two foreign triangles after the fan, with a different attribution.
        mesh.verts.push(Point3::new(2.0, 0.0, 0.0));
        mesh.tris.push([1, 5, 2]);
        mesh.tris.push([2, 5, 3]);
        let fan_attr = Some(TriangleAttribution {
            input: InputId::A,
            face: 0,
        });
        let foreign_attr = Some(TriangleAttribution {
            input: InputId::B,
            face: 9,
        });
        let mut attribution = TriangleAttributionMap::empty();
        attribution.attributions = vec![
            fan_attr,
            fan_attr,
            fan_attr,
            fan_attr,
            foreign_attr,
            foreign_attr,
        ];

        let patch = plane_patch(vec![vec![0, 1, 2, 3]], vec![0, 1, 2, 3]);
        let r = rebuild_patch_planar(&mesh, 0, &patch).expect("planar rebuild");
        apply_rebuild_batch(&mut mesh, &mut attribution, &[r], &BTreeMap::new())
            .expect("write-back");

        assert_eq!(mesh.tris.len(), 4, "2 foreign survivors + 2 replacements");
        assert_eq!(attribution.attributions.len(), 4, "lockstep");
        assert_eq!(
            &attribution.attributions[..2],
            &[foreign_attr, foreign_attr]
        );
        assert_eq!(&attribution.attributions[2..], &[fan_attr, fan_attr]);
        assert!(
            !mesh.tris.iter().flatten().any(|&v| v == 4),
            "interior dropped"
        );
    }

    #[test]
    fn apply_rebuild_batch_refuses_stale_and_overlapping_plans() {
        let mut mesh = square_fan_mesh();
        let mut attribution = TriangleAttributionMap::empty();
        attribution.attributions = vec![None; 4];
        let patch = plane_patch(vec![vec![0, 1, 2, 3]], vec![0, 1, 2, 3]);
        let r = rebuild_patch_planar(&mesh, 0, &patch).expect("planar rebuild");

        // Stale: the mesh grew after the plan was built.
        let mut grown = mesh.clone();
        grown.tris.push([0, 1, 2]);
        let mut grown_attr = TriangleAttributionMap::empty();
        grown_attr.attributions = vec![None; 5];
        assert!(matches!(
            apply_rebuild_batch(
                &mut grown,
                &mut grown_attr,
                std::slice::from_ref(&r),
                &BTreeMap::new()
            ),
            Err(ConstructError::StalePlan { .. })
        ));

        // Overlap: the same patch twice in one batch.
        assert!(matches!(
            apply_rebuild_batch(
                &mut mesh,
                &mut attribution,
                &[r.clone(), r],
                &BTreeMap::new()
            ),
            Err(ConstructError::OverlappingBatch { .. })
        ));
    }

    #[test]
    fn refine_chain_to_ruling_projects_a_chord_anchored_chain() {
        // The wheel-corner fixture: unit cylinder about z, radial plane x=0
        // (through the axis, h=0) — rulings at (0, ±1, z). A chord-anchored
        // chain at y ≈ 0.99866 (the F0067 1.34e-3 gap) projects onto the
        // exact ruling with z preserved.
        let plane = Surface::Plane {
            normal: Vector3::new(1.0, 0.0, 0.0),
            d: 0.0,
        };
        let cyl = Surface::Cylinder {
            axis_point: Point3::new(0.0, 0.0, 0.0),
            axis_dir: Vector3::new(0.0, 0.0, 1.0),
            radius: 1.0,
        };
        let verts = vec![
            Point3::new(0.0, 0.99866, 0.3),
            Point3::new(0.0, 0.99866, 0.7),
        ];
        let moves =
            refine_chain_to_ruling(&verts, &[0, 1], &plane, &cyl, 2e-3).expect("chain refines");
        assert_eq!(moves.len(), 2);
        for (i, (v, p)) in moves.iter().enumerate() {
            assert_eq!(*v, i as u32);
            assert!((p.x()).abs() < 1e-15, "stays in the plane");
            assert!((p.y() - 1.0).abs() < 1e-12, "lands at the exact radius");
            assert!(
                (p.z() - verts[i].z()).abs() < 1e-15,
                "axial coordinate preserved"
            );
        }
        // A tighter band refuses the whole chain loudly.
        assert!(matches!(
            refine_chain_to_ruling(&verts, &[0, 1], &plane, &cyl, 1e-3),
            Err(RulingError::Displacement { vert: 0, .. })
        ));
        // Idempotent: re-refining the refined chain moves nothing.
        let refined: Vec<Point3> = moves.iter().map(|&(_, p)| p).collect();
        let again =
            refine_chain_to_ruling(&refined, &[0, 1], &plane, &cyl, 2e-3).expect("still refines");
        for (i, (_, p)) in again.iter().enumerate() {
            let q = refined[i];
            let d = ((p.x() - q.x()).powi(2) + (p.y() - q.y()).powi(2) + (p.z() - q.z()).powi(2))
                .sqrt();
            assert!(d < 1e-15, "second application is a no-op, moved {d:.3e}");
        }
    }

    #[test]
    fn refine_chain_to_ruling_offset_plane_and_refusals() {
        let cyl = Surface::Cylinder {
            axis_point: Point3::new(0.0, 0.0, 0.0),
            axis_dir: Vector3::new(0.0, 0.0, 1.0),
            radius: 1.0,
        };
        // Offset parallel plane x = 0.5 (n·x + d = 0 with d = −0.5):
        // rulings at (0.5, ±√0.75, z).
        let plane = Surface::Plane {
            normal: Vector3::new(1.0, 0.0, 0.0),
            d: -0.5,
        };
        let g = 0.75f64.sqrt();
        let verts = vec![Point3::new(0.5, g - 8e-4, 0.2)];
        let moves =
            refine_chain_to_ruling(&verts, &[0], &plane, &cyl, 2e-3).expect("offset refines");
        assert!((moves[0].1.y() - g).abs() < 1e-12);
        // A chain straddling the two rulings is ambiguous, named by vertex
        // (v1 sits exactly ON the −ruling, so that side is chosen and v0 is
        // the straddler).
        let straddle = vec![Point3::new(0.5, g - 1e-4, 0.0), Point3::new(0.5, -g, 0.5)];
        assert!(matches!(
            refine_chain_to_ruling(&straddle, &[0, 1], &plane, &cyl, 2e-3),
            Err(RulingError::AmbiguousRuling { vert: 0 })
        ));
        // Non-parallel plane: the exact edge is a conic — I2c tail.
        let tilted = Surface::Plane {
            normal: Vector3::new(
                0.0,
                std::f64::consts::FRAC_1_SQRT_2,
                std::f64::consts::FRAC_1_SQRT_2,
            ),
            d: 0.0,
        };
        assert!(matches!(
            refine_chain_to_ruling(&verts, &[0], &tilted, &cyl, 2e-3),
            Err(RulingError::NonParallelAxis { .. })
        ));
        // Plane missing the cylinder: no exact edge exists.
        let missing = Surface::Plane {
            normal: Vector3::new(1.0, 0.0, 0.0),
            d: -2.0,
        };
        assert!(matches!(
            refine_chain_to_ruling(&verts, &[0], &missing, &cyl, 2e-3),
            Err(RulingError::NoRuling { .. })
        ));
    }

    fn attr(input: crate::brep::InputId, face: u32) -> Option<TriangleAttribution> {
        Some(TriangleAttribution { input, face })
    }

    fn corner_fixture() -> (Vec<SplicePatch>, Vec<Point3>) {
        // Wall cycle [0,1,2,3,4]: (0,1) is the seam; (1,2) shared with `top`
        // (patch 1); (2,3),(3,4) shared with `cap` (patch 2); (4,0) shared
        // with `base` (patch 3). Junction 1 sits 5e-3 from corner endpoint 2.
        let wall = plane_patch(vec![vec![0, 1, 2, 3, 4]], vec![0]);
        let top = plane_patch(vec![vec![2, 1, 8, 9]], vec![1]);
        let cap = SplicePatch {
            cycles: vec![vec![4, 3, 2, 10]],
            tris: vec![2],
            surface: Surface::Cylinder {
                axis_point: Point3::new(0.0, 0.0, 0.0),
                axis_dir: Vector3::new(0.0, 0.0, 1.0),
                radius: 1.0,
            },
        };
        let base = plane_patch(vec![vec![0, 4, 6, 7]], vec![3]);
        let far = |k: usize| Point3::new(10.0 + k as f64, 0.0, 0.0);
        let verts = vec![
            Point3::new(-1.0, 0.0, 0.0),  // 0 — other junction
            Point3::new(1.0, 0.0, 0.0),   // 1 — junction q
            Point3::new(0.995, 0.0, 0.0), // 2 — corner endpoint p (5e-3 gap)
            Point3::new(0.9, 0.0, -1.0),  // 3
            Point3::new(0.5, 0.0, -1.0),  // 4
            far(5),
            far(6),
            far(7),
            far(8),
            far(9),
            far(10),
        ];
        (vec![wall, top, cap, base], verts)
    }

    #[test]
    fn input_edge_chains_finds_seam_adjacent_runs_only() {
        use crate::brep::InputId;
        let (patches, verts) = corner_fixture();
        let patch_attr = vec![
            attr(InputId::B, 1),
            attr(InputId::B, 4),
            attr(InputId::B, 2),
            attr(InputId::B, 3),
        ];
        let mut curves = BTreeMap::new();
        curves.insert((0u32, 1u32), Curve::LineSegment);
        let seam_edges: BTreeSet<(u32, u32)> = [(0u32, 1u32)].into();
        let junctions: BTreeSet<u32> = [0u32, 1].into();
        let scope: BTreeSet<usize> = [0usize].into();
        let (chains, _) = input_edge_chains(
            &patches,
            &patch_attr,
            &verts,
            &curves,
            &seam_edges,
            &junctions,
            &scope,
            0.01,
        );
        assert_eq!(chains.len(), 3, "top, cap, and base runs: {chains:?}");
        let top_run = chains.iter().find(|c| c.neighbor == 1).expect("top run");
        assert_eq!(top_run.verts, vec![1, 2]);
        assert_eq!(
            top_run.corner_pairs,
            vec![(2, 1)],
            "endpoint 1 IS a junction — pinned, never a pair's p"
        );
        let cap_run = chains.iter().find(|c| c.neighbor == 2).expect("cap run");
        assert_eq!(cap_run.verts, vec![2, 3, 4]);
        assert_eq!(cap_run.corner_pairs, vec![(2, 1)]);
        // The base run [4,0] ends AT junction 0: seam-adjacent via the
        // pinned endpoint, but it pairs nothing.
        let base_run = chains.iter().find(|c| c.neighbor == 3).expect("base run");
        assert!(base_run.corner_pairs.is_empty());
    }

    #[test]
    fn input_edge_chains_dedups_shared_runs_and_excludes_cross_input() {
        use crate::brep::InputId;
        let (patches, verts) = corner_fixture();
        // `top` belongs to the OTHER input: its run is not an input edge.
        let patch_attr = vec![
            attr(InputId::B, 1),
            attr(InputId::A, 4),
            attr(InputId::B, 2),
            attr(InputId::B, 3),
        ];
        let mut curves = BTreeMap::new();
        curves.insert((0u32, 1u32), Curve::LineSegment);
        let seam_edges: BTreeSet<(u32, u32)> = [(0u32, 1u32)].into();
        let junctions: BTreeSet<u32> = [0u32, 1].into();
        // BOTH owners scoped: the wall/cap run must come back ONCE.
        let scope: BTreeSet<usize> = [0usize, 2].into();
        let (chains, _) = input_edge_chains(
            &patches,
            &patch_attr,
            &verts,
            &curves,
            &seam_edges,
            &junctions,
            &scope,
            0.01,
        );
        // The cap run comes back ONCE (deduped); the cross-input top run is
        // excluded; the junction-ended base run rides along.
        let cap_runs: Vec<_> = chains.iter().filter(|c| c.verts.contains(&3)).collect();
        assert_eq!(cap_runs.len(), 1, "one deduped cap run: {chains:?}");
        assert_eq!(cap_runs[0].verts, vec![2, 3, 4]);
        assert!(!chains
            .iter()
            .any(|c| c.verts == vec![1, 2] || c.verts == vec![2, 1]));
    }

    #[test]
    fn input_edge_chains_skips_a_closed_one_neighbour_ring() {
        use crate::brep::InputId;
        // A square fully shared with ONE neighbour — no run boundary, no
        // corner endpoint, skipped even with a junction-adjacent vertex.
        let a = plane_patch(vec![vec![0, 1, 2, 3]], vec![0]);
        let b = plane_patch(vec![vec![3, 2, 1, 0]], vec![1]);
        let verts = vec![
            Point3::new(0.0, 0.0, 0.0),
            Point3::new(1.0, 0.0, 0.0),
            Point3::new(1.0, 1.0, 0.0),
            Point3::new(0.0, 1.0, 0.0),
            Point3::new(1.0005, 0.0, 0.0), // junction near vertex 1
        ];
        let patch_attr = vec![attr(InputId::B, 1), attr(InputId::B, 2)];
        let curves = BTreeMap::new();
        let seam_edges = BTreeSet::new();
        let junctions: BTreeSet<u32> = [4u32].into();
        let scope: BTreeSet<usize> = [0usize].into();
        let (chains, _) = input_edge_chains(
            &[a, b],
            &patch_attr,
            &verts,
            &curves,
            &seam_edges,
            &junctions,
            &scope,
            0.01,
        );
        assert!(chains.is_empty(), "closed ring has no corner: {chains:?}");
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
    // ---- §I13(f) f2c delete_boundary_fan / split_boundary_edge ---------

    /// The measured corner shape: a boundary vertex 6 whose fan overhangs
    /// a chain [1, 2, 3] (the rim); deleting the fan leaves the chain as
    /// the boundary with no replacement triangles.
    #[test]
    fn delete_boundary_fan_leaves_the_link_chain() {
        let mesh = Mesh {
            verts: vec![
                Point3::new(0.0, 0.0, 0.0),  // 0
                Point3::new(1.0, 0.0, 0.0),  // 1
                Point3::new(2.0, 0.05, 0.0), // 2 (rim chain)
                Point3::new(3.0, 0.0, 0.0),  // 3
                Point3::new(4.0, 0.0, 0.0),  // 4
                Point3::new(2.0, 1.0, 0.0),  // 5 interior
                Point3::new(2.0, -0.6, 0.0), // 6 = victim (overhang)
            ],
            tris: vec![
                [0, 1, 5],
                [1, 2, 5],
                [2, 3, 5],
                [3, 4, 5],
                [1, 6, 2], // fan of 6 over the chain
                [2, 6, 3],
            ],
        };
        let patch = plane_patch(vec![vec![0, 1, 6, 3, 4, 5]], vec![0, 1, 2, 3, 4, 5]);
        let (r, link) = delete_boundary_fan(&mesh, 3, &patch, 6).expect("boundary fan");
        assert_eq!(r.old_tris, vec![4, 5]);
        assert!(r.new_tris.is_empty() && r.new_verts.is_empty() && r.dropped.is_empty());
        assert_eq!(link, vec![3, 2, 1], "the rim chain, region-oriented");
        // An interior vertex's closed link declines: deleting would hole.
        let square = square_fan_mesh();
        let sq_patch = plane_patch(vec![vec![0, 1, 2, 3]], vec![0, 1, 2, 3]);
        match delete_boundary_fan(&square, 0, &sq_patch, 4) {
            Err(ConstructError::FanNotSimple {
                reason: FanReason::Closed { fan: 4 },
                ..
            }) => {}
            other => panic!("interior vertex must decline Closed, got {other:?}"),
        }
    }

    /// §3j — the set form deletes the JOINT region of two adjacent
    /// boundary victims (a phantom plus its absorbed chain-end anchor)
    /// and chains one open link across both fans.
    #[test]
    fn delete_boundary_fan_set_joins_adjacent_fans() {
        let mesh = Mesh {
            verts: vec![
                Point3::new(0.0, 0.0, 0.0),  // 0
                Point3::new(1.0, 0.0, 0.0),  // 1
                Point3::new(2.0, 0.05, 0.0), // 2
                Point3::new(3.0, 0.0, 0.0),  // 3
                Point3::new(4.0, 0.0, 0.0),  // 4
                Point3::new(2.0, 1.0, 0.0),  // 5 interior
                Point3::new(2.0, -0.6, 0.0), // 6 = phantom
                Point3::new(1.0, -0.5, 0.0), // 7 = absorbed anchor
            ],
            tris: vec![
                [0, 1, 5],
                [1, 2, 5],
                [2, 3, 5],
                [3, 4, 5],
                [1, 6, 2],
                [2, 6, 3],
                [0, 7, 1],
                [1, 7, 6],
            ],
        };
        let patch = plane_patch(vec![vec![0, 7, 6, 3, 4, 5]], vec![0, 1, 2, 3, 4, 5, 6, 7]);
        let (r, link) =
            delete_boundary_fan_set(&mesh, 3, &patch, &BTreeSet::from([6, 7])).expect("set fan");
        assert_eq!(r.old_tris, vec![4, 5, 6, 7]);
        assert!(r.new_tris.is_empty() && r.dropped.is_empty());
        assert_eq!(link, vec![3, 2, 1, 0], "one open chain across both fans");
    }

    /// Splitting a true boundary edge yields two winding-preserving
    /// children; interior and absent edges decline; a flipping insert
    /// position declines.
    #[test]
    fn split_boundary_edge_splits_exactly_one_triangle() {
        let mesh = Mesh {
            verts: vec![
                Point3::new(0.0, 0.0, 0.0), // 0
                Point3::new(2.0, 0.0, 0.0), // 1
                Point3::new(2.0, 1.0, 0.0), // 2
                Point3::new(0.0, 1.0, 0.0), // 3
                Point3::new(9.0, 9.0, 9.0), // 4 = vertex to insert (moved)
            ],
            tris: vec![[0, 1, 2], [0, 2, 3]],
        };
        let patch = plane_patch(vec![vec![0, 1, 2, 3]], vec![0, 1]);
        let mint = Point3::new(1.0, -0.01, 0.0);
        let r = split_boundary_edge(&mesh, 2, &patch, 0, 1, 4, mint).expect("boundary split");
        assert_eq!(r.old_tris, vec![0]);
        assert_eq!(r.new_tris, vec![[0, 4, 2], [4, 1, 2]]);
        assert!(r.dropped.is_empty());
        // The reversed call splits the same directed occurrence.
        let r2 = split_boundary_edge(&mesh, 2, &patch, 1, 0, 4, mint).expect("order-free");
        assert_eq!(r2.new_tris, r.new_tris);
        // The diagonal {0, 2} is interior (two incident patch triangles).
        match split_boundary_edge(&mesh, 2, &patch, 0, 2, 4, mint) {
            Err(ConstructError::EdgeNotBoundary { incident: 2, .. }) => {}
            other => panic!("interior edge must decline, got {other:?}"),
        }
        // An absent edge declines with zero incidents.
        match split_boundary_edge(&mesh, 2, &patch, 1, 3, 4, mint) {
            Err(ConstructError::EdgeNotBoundary { incident: 0, .. }) => {}
            other => panic!("absent edge must decline, got {other:?}"),
        }
        // A mint on the far side of the apex flips a child: declined.
        match split_boundary_edge(&mesh, 2, &patch, 0, 1, 4, Point3::new(1.0, 2.0, 0.0)) {
            Err(ConstructError::SplitFlip { .. }) => {}
            other => panic!("flipping insert must decline, got {other:?}"),
        }
    }

    // ---- §I13(f) f2c-2 refill_fan_hole ---------------------------------

    /// The measured hole shape on a planar stand-in: a boundary victim 5
    /// whose fossil fan spans the strip; the fill re-covers the link
    /// polygon MINUS the dropped-end corner, orientation matched.
    #[test]
    fn refill_fan_hole_covers_the_trimmed_polygon() {
        let mesh = Mesh {
            verts: vec![
                Point3::new(0.0, 0.0, 0.0),  // 0 = dropped corner
                Point3::new(1.0, 1.0, 0.0),  // 1 upper link
                Point3::new(2.0, 1.1, 0.0),  // 2 upper link
                Point3::new(3.0, 1.0, 0.0),  // 3 upper link
                Point3::new(3.5, 0.0, 0.0),  // 4 = kept corner
                Point3::new(1.7, -0.4, 0.0), // 5 = victim (overhang)
            ],
            tris: vec![[5, 0, 1], [5, 1, 2], [5, 2, 3], [5, 3, 4]],
        };
        let patch = plane_patch(vec![vec![0, 1, 2, 3, 4, 5]], vec![0, 1, 2, 3]);
        let (r, link) = delete_boundary_fan(&mesh, 0, &patch, 5).expect("boundary fan");
        assert_eq!(link, vec![0, 1, 2, 3, 4]);
        let fill = refill_fan_hole(&mesh, 0, &patch, &link[1..], &r.old_tris).expect("planar fill");
        assert_eq!(fill.len(), 2, "quad polygon fills with two triangles");
        // Only polygon vertices referenced; the victim and the dropped
        // corner appear nowhere.
        for t in &fill {
            for v in t {
                assert!(link[1..].contains(v), "foreign vertex {v} in {fill:?}");
            }
        }
        // Orientation matches the fossil fan's sense per triangle.
        let pos = |v: u32| mesh.verts[v as usize];
        let want = area_vector(&[[5, 0, 1], [5, 1, 2], [5, 2, 3], [5, 3, 4]], &pos);
        for t in &fill {
            let av = area_vector(&[*t], &pos);
            assert!(dot3(want, av) > 0.0, "triangle {t:?} flipped");
        }
        // A polygon shorter than a triangle refuses loudly.
        match refill_fan_hole(&mesh, 0, &patch, &link[3..], &r.old_tris) {
            Err(ConstructError::MalformedPatch { .. }) => {}
            other => panic!("short polygon must refuse, got {other:?}"),
        }
    }

    /// The cone-chart path: chain θ-unwrap, the like-for-like d(T) budget
    /// (the fossil slivers' own certified bound), and the apex guard.
    #[test]
    fn refill_fan_hole_cone_chart_unwraps_and_respects_the_budget() {
        let half_angle = 0.4f64;
        let tan = half_angle.tan();
        let on_cone = |theta: f64, z: f64| {
            // ortho_basis(+z) is deterministic; evaluate through the chart
            // itself so the fixture matches project() exactly.
            let chart = crate::stage4_project::SurfaceChart::Cone {
                apex: [0.0, 0.0, 0.0],
                axis: [0.0, 0.0, 1.0],
                e1: [1.0, 0.0, 0.0],
                e2: [0.0, 1.0, 0.0],
                tan_half: tan,
            };
            chart.lift(cad_primitives::Point2::new(theta, z))
        };
        let surface = Surface::Cone {
            apex: Point3::new(0.0, 0.0, 0.0),
            axis_dir: Vector3::new(0.0, 0.0, 1.0),
            half_angle,
        };
        let mesh = Mesh {
            verts: vec![
                on_cone(0.00, 2.0),  // 0 = dropped corner (on the lower rim)
                on_cone(0.30, 2.6),  // 1 upper link
                on_cone(-0.10, 2.6), // 2 upper link (θ zigzag)
                on_cone(0.12, 2.0),  // 3 = kept corner
                on_cone(0.06, 1.9),  // 4 = victim below the rim
            ],
            tris: vec![[4, 0, 1], [4, 1, 2], [4, 2, 3]],
        };
        let patch = SplicePatch {
            cycles: vec![vec![0, 1, 2, 3, 4]],
            tris: vec![0, 1, 2],
            surface,
        };
        let (r, link) = delete_boundary_fan(&mesh, 0, &patch, 4).expect("boundary fan");
        assert_eq!(link, vec![0, 1, 2, 3]);
        let fill = refill_fan_hole(&mesh, 0, &patch, &link[1..], &r.old_tris).expect("cone fill");
        assert_eq!(fill.len(), 1, "triangle polygon fills with one triangle");
        let pos = |v: u32| mesh.verts[v as usize];
        let want = area_vector(&[[4, 0, 1], [4, 1, 2], [4, 2, 3]], &pos);
        assert!(dot3(want, area_vector(&fill, &pos)) > 0.0);
        // A polygon touching the apex station refuses loudly.
        let mut apex_mesh = mesh.clone();
        apex_mesh.verts[1] = Point3::new(0.0, 0.0, 0.0);
        match refill_fan_hole(&apex_mesh, 0, &patch, &link[1..], &r.old_tris) {
            Err(ConstructError::ApexInPatch { .. }) => {}
            other => panic!("apex polygon must refuse, got {other:?}"),
        }
    }

    /// §4.5.1 inc-2c-3b-3 — the fan-local Torus chart (`YANG_441_TORUS_CHART`,
    /// default OFF). The window straddles BOTH seams (θ = π azimuth AND φ = π
    /// tube angle), so raw `atan2` coordinates jump a full turn between chain
    /// neighbours in each coordinate — the double chain-unwrap is what makes
    /// the CDT boundary simple. Knob off, the refusal is today's typed
    /// `NonPlanarPatch` (byte-identical default, the R0074 wall).
    #[test]
    fn refill_fan_hole_torus_chart_double_unwraps_across_both_seams() {
        let (major, minor) = (10.0, 2.0);
        let chart = crate::stage4_project::SurfaceChart::Torus {
            center: [0.0, 0.0, 0.0],
            axis: [0.0, 0.0, 1.0],
            e1: [1.0, 0.0, 0.0],
            e2: [0.0, 1.0, 0.0],
            major,
            minor,
        };
        let on_torus = |theta: f64, phi: f64| chart.lift(cad_primitives::Point2::new(theta, phi));
        let surface = Surface::Torus {
            center: Point3::new(0.0, 0.0, 0.0),
            axis_dir: Vector3::new(0.0, 0.0, 1.0),
            major_radius: major,
            minor_radius: minor,
        };
        let pi = std::f64::consts::PI;
        let mesh = Mesh {
            verts: vec![
                on_torus(pi - 0.30, pi - 0.25), // 0 link end (θ, φ both below seam)
                on_torus(pi + 0.25, pi - 0.30), // 1 link (θ crosses the seam)
                on_torus(pi + 0.30, pi + 0.25), // 2 link (φ crosses the seam too)
                on_torus(pi - 0.25, pi + 0.30), // 3 link end (θ back across)
                on_torus(pi - 0.02, pi + 0.02), // 4 = the phantom (victim mid-window)
                // 5 = the corridor mint replacing it — at the SAME station, so
                // the fill reproduces the fossil's own granularity exactly and
                // the like-for-like budget is met with equality (the test pins
                // the chart/unwrap/budget plumbing; mint placement is the
                // planner's business).
                on_torus(pi - 0.02, pi + 0.02),
            ],
            tris: vec![[4, 0, 1], [4, 1, 2], [4, 2, 3]],
        };
        let patch = SplicePatch {
            cycles: vec![vec![0, 1, 2, 3, 4]],
            tris: vec![0, 1, 2],
            surface,
        };
        let (r, link) = delete_boundary_fan(&mesh, 0, &patch, 4).expect("boundary fan");
        assert_eq!(link, vec![0, 1, 2, 3]);
        // The far-arm polygon shape (§3j): the link plus the corridor path
        // closing the hole where the phantom sat — same footprint, same
        // density, so the like-for-like budget is satisfiable.
        let polygon = [0u32, 1, 2, 3, 5];
        // Knob off (the default): the chart does not exist — the holder's
        // refusal is exactly today's, the measured R0074 wall.
        match refill_fan_hole(&mesh, 0, &patch, &polygon, &r.old_tris) {
            Err(ConstructError::NonPlanarPatch { .. }) => {}
            other => panic!("knob-off torus refill must refuse NonPlanarPatch, got {other:?}"),
        }
        std::env::set_var("YANG_441_TORUS_CHART", "1");
        let fill = refill_fan_hole(&mesh, 0, &patch, &polygon, &r.old_tris).expect("torus fill");
        assert_eq!(fill.len(), 3, "pentagon fills with three triangles");
        assert!(
            fill.iter().any(|t| t.contains(&5)),
            "the mint must appear in the fill"
        );
        let pos = |v: u32| mesh.verts[v as usize];
        let want = area_vector(&[[4, 0, 1], [4, 1, 2], [4, 2, 3]], &pos);
        assert!(dot3(want, area_vector(&fill, &pos)) > 0.0);
    }
}
