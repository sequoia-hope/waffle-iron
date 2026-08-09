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

use cad_primitives::Point3;
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
    let (a, b, x) = (p(e0), p(e1), p(v));
    let d = [b[0] - a[0], b[1] - a[1], b[2] - a[2]];
    let len2 = d[0] * d[0] + d[1] * d[1] + d[2] * d[2];
    if len2 == 0.0 || !len2.is_finite() {
        return false;
    }
    let r = [x[0] - a[0], x[1] - a[1], x[2] - a[2]];
    let t = (r[0] * d[0] + r[1] * d[1] + r[2] * d[2]) / len2;
    if t <= 0.0 || t >= 1.0 {
        return false; // beyond a junction — not this seam's interior
    }
    let perp = [r[0] - t * d[0], r[1] - t * d[1], r[2] - t * d[2]];
    let perp2 = perp[0] * perp[0] + perp[1] * perp[1] + perp[2] * perp[2];
    perp2 <= 1e-18 * len2
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
    /// [`apply_rebuild_batch`] was handed a plan built against a different
    /// mesh.
    StalePlan {
        expected_tris: u32,
        actual_tris: u32,
    },
    /// Two rebuilds in one batch claim the same old triangle — a driver bug,
    /// not input (flood-fill patches are disjoint).
    OverlappingBatch { tri: u32 },
}

/// One patch's single-sided rebuild, entirely in MESH index space and ready
/// for [`apply_rebuild_batch`]. Zero new vertices by construction: a plain
/// CDT of the boundary polygon adds no Steiner points, so every triangle
/// references existing mesh vertices.
#[derive(Debug, Clone, PartialEq)]
pub(crate) struct PatchRebuild {
    /// Index of the patch in the pass's patch list (diagnostic only).
    pub patch: usize,
    /// The old triangles this rebuild replaces (indices into `mesh.tris`).
    pub old_tris: Vec<u32>,
    /// Replacement triangles in mesh vertex indices, orientation matched to
    /// the old patch's outward sense.
    pub new_tris: Vec<[u32; 3]>,
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

/// Re-triangulate one PLANAR patch single-sided against its modified cycles.
///
/// `patch.cycles` are the post-[`collapse_patch_runs`] cycles; `patch.tris`
/// the ORIGINAL triangle set being replaced. The collapsed seams are ordinary
/// boundary edges here — the CDT reproduces every boundary edge exactly, so
/// each collapsed seam's `(e0, e1)` pair and every untouched neighbour chain
/// come out shared-by-index. Interior vertices (chain interiors and flood
/// interiors) simply do not appear in the CDT input: for a planar patch they
/// are geometrically redundant — the paper's collinear "remove a mesh vertex"
/// quality step. The caller's foreign-reference scan is what makes dropping
/// them safe.
pub(crate) fn rebuild_patch_planar(
    mesh: &Mesh,
    patch_index: usize,
    patch: &SplicePatch,
) -> Result<PatchRebuild, ConstructError> {
    if !matches!(patch.surface, Surface::Plane { .. }) {
        return Err(ConstructError::NonPlanarPatch { patch: patch_index });
    }
    let chart = SurfaceChart::new(patch.surface)
        .ok_or(ConstructError::NonPlanarPatch { patch: patch_index })?;
    let (p2, back) =
        patch_from_cycles_shifted(&chart, &mesh.verts, &patch.cycles, &BTreeMap::new())
            .ok_or(ConstructError::MalformedPatch { patch: patch_index })?;

    let tris2 = cdt_with_interior_constraints(&p2.verts, &p2.boundary, &p2.holes, &[], &[])
        .map_err(|error| ConstructError::Cdt {
            patch: patch_index,
            error,
        })?;

    // Back into mesh index space. The CDT adds no Steiner points, so every
    // index is inside the input pool; a miss is a broken invariant, not input.
    let mut new_tris: Vec<[u32; 3]> = tris2
        .iter()
        .map(|t| {
            [
                back[t[0] as usize],
                back[t[1] as usize],
                back[t[2] as usize],
            ]
        })
        .collect();

    // Match the ORIGINAL patch's outward sense by measurement — the chart
    // basis has arbitrary handedness relative to the surface normal.
    let old: Vec<[u32; 3]> = patch.tris.iter().map(|&t| mesh.tris[t as usize]).collect();
    let pos = |v: u32| -> Point3 { mesh.verts[v as usize] };
    let want = area_vector(&old, &pos);
    let got = area_vector(&new_tris, &pos);
    let d = dot3(want, got);
    if d == 0.0 || !d.is_finite() {
        return Err(ConstructError::DegenerateOrientation { patch: patch_index });
    }
    if d < 0.0 {
        for t in &mut new_tris {
            t.swap(1, 2);
        }
    }

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
        dropped,
        plan_verts: mesh.verts.len() as u32,
        plan_tris: mesh.tris.len() as u32,
    })
}

/// Write a batch of [`PatchRebuild`]s into the mesh in ONE pass: drop every
/// rebuilt patch's old triangles, append each patch's replacements carrying
/// that patch's (uniform) attribution. `attribution.attributions` stays in
/// lockstep with `mesh.tris` — the invariant every downstream consumer
/// depends on. No vertices are added; orphaned ones are left for the caller's
/// usual `compact_unreferenced_verts` pass.
pub(crate) fn apply_rebuild_batch(
    mesh: &mut Mesh,
    attribution: &mut TriangleAttributionMap,
    rebuilds: &[PatchRebuild],
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
        tris.push(*tri);
        attrs.push(attribution.attributions[t]);
    }
    for (r, attr) in rebuilds.iter().zip(attrs_of) {
        for tri in &r.new_tris {
            tris.push(*tri);
            attrs.push(attr);
        }
    }
    mesh.tris = tris;
    attribution.attributions = attrs;
    Ok(())
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
    fn rebuild_patch_planar_refuses_a_curved_patch() {
        let mesh = square_fan_mesh();
        let patch = SplicePatch {
            cycles: vec![vec![0, 1, 2, 3]],
            tris: vec![0, 1, 2, 3],
            surface: Surface::Cylinder {
                axis_point: Point3::new(0.0, 0.0, 0.0),
                axis_dir: Vector3::new(0.0, 0.0, 1.0),
                radius: 1.0,
            },
        };
        assert_eq!(
            rebuild_patch_planar(&mesh, 3, &patch),
            Err(ConstructError::NonPlanarPatch { patch: 3 })
        );
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
        apply_rebuild_batch(&mut mesh, &mut attribution, &[r]).expect("write-back");

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
            apply_rebuild_batch(&mut grown, &mut grown_attr, &[r.clone()]),
            Err(ConstructError::StalePlan { .. })
        ));

        // Overlap: the same patch twice in one batch.
        assert!(matches!(
            apply_rebuild_batch(&mut mesh, &mut attribution, &[r.clone(), r]),
            Err(ConstructError::OverlappingBatch { .. })
        ));
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
