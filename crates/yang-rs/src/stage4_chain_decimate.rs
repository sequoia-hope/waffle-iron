//! §4.4.1 mesh-level curve-chain decimation (F2 / deviation N81, matrix row
//! `4.4-28`; Yang et al. 2025 §4.3.4 / §4.4.1,
//! `refs/text/yang2025_hybrid_boolean.txt:575-593, 605-609, 546-572`, spec
//! `specs/yang_434_output_chord_refinement.md` and
//! `specs/yang_442_classify_after_update.md` §3(d) #3).
//!
//! Formerly `stage5_chain_decimate` (which dropped interior vertices from
//! Stage-5 B-Rep loops after emission while leaving them in `mesh.tris`,
//! applied the §4.3.4 test in reverse on consecutive triples without
//! re-checking the kept chord against all replaced samples, and ordered chains
//! by `u32` vertex index).
//!
//! Design and remaining open debt:
//! - **Forward §4.3.4 admissibility (`kept_positions`):** starts from the chain
//!   endpoints and selects a minimum-chord-count path of §4.3.4-admissible
//!   chords where every kept chord `(p_i, p_j)` (`j - i >= 2`) satisfies the
//!   symmetric §4.3.4 `h/l/α` predicate (or strict `TAU_WORK` collinearity for
//!   class-(b) straight operand-edge subdivisions, ledger `N105`) against
//!   **every** sample `m in (i + 1)..j` it replaces. Equal-chord-count ties
//!   minimize the descending-sorted multiset of squared chord lengths
//!   `||p_j - p_i||^2` (strictly monotone under path extension and bit-identical
//!   on `chain` vs `chain.rev()`), so open-chain decimation is independent of
//!   walk direction.
//! - **Closed chains:** closed loops carrying `>= 1` surviving authored operand
//!   `BRepVertex`es keep all of them as mandatory anchors (and open the loop at
//!   one), making the kept 3D vertex set independent of vertex numbering and
//!   world frame. Closed `LineSegment`/`SurfacePair` loops carrying `0`
//!   surviving operand `BRepVertex`es open at the `point_cmp` minimum 3D sample
//!   (vertex-index-free, retaining world-frame start-sample dependence on
//!   unanchored closed loops; ledger `N81` / `N58`).
//! - **`Surface::Sphere` patches:** `SurfaceChart` has no sphere chart
//!   (`stage4_project.rs`), so chains owned by a `Surface::Sphere` patch
//!   decline decimation (`DecimateStats::declined_sphere_unsupported` — a
//!   documented regression vs the deleted loop-only `stage5_chain_decimate`,
//!   ledger `N81`).
//! - **Class-(b) straight operand-edge subdivision elision:** valence-2
//!   non-intersection split vertices on straight operand edges are elided only
//!   when collinear to working precision (`h <= TAU_WORK * (1 + scale)`,
//!   `l < d_p * 10^3`); ledgered as open debt `N105`.
//! - **Loud invariants & named census:** broken mesh/partition invariants
//!   return `Err(YangError::Stage4ChainDecimateFailed { .. })`; legitimate
//!   per-chain declines are counted by reason in [`DecimateStats`].

use std::collections::{BTreeMap, BTreeSet};

use cad_primitives::Point3;
use cherchi_rs::Mesh;

use crate::brep::{BRep, InputId, TriangleAttributionMap, TriangleSideLabelMap};
use crate::errors::{ChainDecimateError, YangError};
use crate::geom::{Curve, Surface};
use crate::stage4_construct::{
    apply_rebuild_batch, directed_boundary, rebuild_patch_planar_bounded_ex,
    refill_fan_hole_seeded, PatchRebuild,
};
use crate::stage4_correct::{
    merge_same_plane_patches_with_side_labels, paper_chain_metrics, paper_chain_sample_redundant,
    stage4_chord_band, PatchInfo,
};
use crate::stage4_project::SurfaceChart;
use crate::stage4_splice::SplicePatch;
use crate::stage5_output_refine::{certified_output_edge_curve, RestoreStats};
use crate::stage5_seam_merge::{point_cmp, point_key};
use crate::stage5_topology::{
    flood_fill_patches_with_side_labels, patch_fold_slivers, triangle_adjacency,
};

/// Pass census statistics (logged at the Stage-4 call site when `c441_verbose()`).
#[derive(Debug, Default, Clone, Copy, PartialEq, Eq)]
pub(crate) struct DecimateStats {
    pub edges_before: usize,
    pub edges_after: usize,
    /// Canonical chains that dropped at least one interior vertex.
    pub runs_decimated: usize,
    /// Interior vertices dropped (counted once per canonical chain).
    pub verts_dropped: usize,
    /// Chains declined because an owner cycle would fall below 3 edges.
    pub declined_loop_floor: usize,
    /// Chains declined because an owner patch had a discontinuous boundary cycle.
    pub declined_discontinuous_patch: usize,
    /// Chains declined because an owner patch sits on `Surface::Sphere`
    /// (`SurfaceChart` has no sphere chart).
    pub declined_sphere_unsupported: usize,
    /// Chains declined because an owner patch on a chartable surface (`Plane`,
    /// `Cylinder`, `Cone`, `Torus`) declined both fan-local CDT and whole-patch CDT.
    pub declined_patch_rebuild: usize,
    /// Cycles skipped because their directed half-edges do not chain.
    pub skipped_discontinuous_loops: usize,
}

#[derive(Clone, Copy, Debug, PartialEq)]
enum RunKey {
    Line,
    Pair(Surface, Surface),
}

impl RunKey {
    fn same(&self, other: &RunKey) -> bool {
        match (self, other) {
            (RunKey::Line, RunKey::Line) => true,
            (RunKey::Pair(a, b), RunKey::Pair(c, d)) => (a == c && b == d) || (a == d && b == c),
            _ => false,
        }
    }
}

fn run_key_of_curve(c: &Curve) -> Option<RunKey> {
    match *c {
        Curve::LineSegment => Some(RunKey::Line),
        Curve::SurfacePair { a, b } => Some(RunKey::Pair(a, b)),
        _ => None,
    }
}

/// Directed vertex cycle of a patch boundary loop (`cycle[i] = cyc[i].0`), or
/// `None` if consecutive half-edges do not chain `end == next.start`.
fn patch_cycle_verts(cyc: &[(u32, u32)]) -> Option<Vec<u32>> {
    if cyc.is_empty() {
        return None;
    }
    let m = cyc.len();
    let mut out = Vec::with_capacity(m);
    for i in 0..m {
        if cyc[i].1 != cyc[(i + 1) % m].0 {
            return None;
        }
        out.push(cyc[i].0);
    }
    Some(out)
}

/// Canonical form of an OPEN chain by 3D point sequence (`point_cmp`, never
/// `u32` vertex index). With forward §4.3.4 in [`kept_positions`], the kept 3D
/// sample set is walk-direction-independent; this canonicalization gives a
/// shared map key across twin/4-owner patches and breaks exact uniform-grid ties
/// deterministically.
fn canonical_open_chain(chain: &[u32], verts: &[Point3]) -> (Vec<u32>, bool) {
    let k = chain.len();
    let mut ord = std::cmp::Ordering::Equal;
    for i in 0..k {
        ord = point_cmp(verts[chain[i] as usize], verts[chain[k - 1 - i] as usize]);
        if ord != std::cmp::Ordering::Equal {
            break;
        }
    }
    if ord != std::cmp::Ordering::Greater {
        (chain.to_vec(), false)
    } else {
        (chain.iter().rev().copied().collect(), true)
    }
}

/// Canonical form of a CLOSED chain by 3D point sequence (`point_cmp`, never
/// `u32` vertex index): rotate and orient to minimize the 3D point tuple
/// lexicographically. Returns `(rotated, min_pos_in_chain, reversed)`.
fn canonical_closed_chain(chain: &[u32], verts: &[Point3]) -> (Vec<u32>, usize, bool) {
    let k = chain.len();
    assert!(k > 0, "closed chain is non-empty");
    let idx_at = |pos: usize, rev: bool, step: usize| -> usize {
        if rev {
            (pos + k - step) % k
        } else {
            (pos + step) % k
        }
    };
    let mut best: Option<(usize, bool)> = None;
    for pos in 0..k {
        for rev in [false, true] {
            let take = match best {
                None => true,
                Some((bpos, brev)) => {
                    let mut ord = std::cmp::Ordering::Equal;
                    for step in 0..k {
                        let pa = verts[chain[idx_at(pos, rev, step)] as usize];
                        let pb = verts[chain[idx_at(bpos, brev, step)] as usize];
                        ord = point_cmp(pa, pb);
                        if ord != std::cmp::Ordering::Equal {
                            break;
                        }
                    }
                    ord == std::cmp::Ordering::Less
                }
            };
            if take {
                best = Some((pos, rev));
            }
        }
    }
    let (min_pos, reversed) = best.expect("non-empty closed chain");
    let rotated: Vec<u32> = (0..k)
        .map(|step| chain[idx_at(min_pos, reversed, step)])
        .collect();
    (rotated, min_pos, reversed)
}

struct LoopPartition {
    stretches: Vec<(usize, usize, Option<RunKey>)>,
    offset: usize,
    whole_closed: bool,
}

fn partition_cycle(
    cyc: &[(u32, u32)],
    cycle: &[u32],
    edge_keys: &BTreeMap<(u32, u32), Option<RunKey>>,
    droppable: &BTreeSet<u32>,
) -> LoopPartition {
    let m = cyc.len();
    let keys: Vec<Option<RunKey>> = cyc
        .iter()
        .map(|&(s, e)| edge_keys.get(&(s.min(e), s.max(e))).copied().flatten())
        .collect();
    let joinable_after = |i: usize| -> bool {
        let j = (i + 1) % m;
        match (&keys[i], &keys[j]) {
            (Some(a), Some(b)) => a.same(b) && droppable.contains(&cycle[j]),
            _ => false,
        }
    };
    let Some(first_break) = (0..m).find(|&i| !joinable_after(i)) else {
        return LoopPartition {
            stretches: vec![(0, m, keys[0])],
            offset: 0,
            whole_closed: true,
        };
    };
    let offset = (first_break + 1) % m;
    let key_at = |r: usize| keys[(offset + r) % m];
    let mut stretches = Vec::new();
    let mut r = 0usize;
    while r < m {
        let k = key_at(r);
        let mut len = 1usize;
        while r + len < m
            && matches!((key_at(r + len), k), (Some(a), Some(b)) if a.same(&b))
            && joinable_after((offset + r + len - 1) % m)
        {
            len += 1;
        }
        stretches.push((r, len, k));
        r += len;
    }
    LoopPartition {
        stretches,
        offset,
        whole_closed: false,
    }
}

/// STRICT redundancy for a subdivision vertex of a straight operand edge
/// (ledger `N105`): `m` lies on the segment `a -> b` to working precision
/// (`h <= TAU_WORK * (1 + scale)`) and within the paper's chord bound
/// (`l < d_p * 10^3`, Yang §4.3.4 `refs/text/yang2025_hybrid_boolean.txt:575-593`).
fn strict_subdivision_redundant(a: [f64; 3], m: [f64; 3], b: [f64; 3]) -> bool {
    let mt = paper_chain_metrics(a, m, b);
    let scale = a
        .iter()
        .chain(m.iter())
        .chain(b.iter())
        .fold(0.0f64, |acc, &c| acc.max(c.abs()));
    mt.l < mt.dp * 1e3 && mt.h <= cad_primitives::TAU_WORK * (1.0 + scale)
}

/// Direction-symmetric §4.3.4 single-sample redundancy predicate on chord `a -- b`:
/// evaluates both `(a, m, b)` and `(b, m, a)` so 1-ULP projection rounding in
/// `paper_chain_metrics` cannot make chord admissibility depend on walk direction.
fn sample_redundant_on_chord(a: [f64; 3], m: [f64; 3], b: [f64; 3], is_strict: bool) -> bool {
    if is_strict {
        strict_subdivision_redundant(a, m, b) && strict_subdivision_redundant(b, m, a)
    } else {
        paper_chain_sample_redundant(a, m, b) && paper_chain_sample_redundant(b, m, a)
    }
}

fn sq_chord_len(a: [f64; 3], b: [f64; 3]) -> f64 {
    let (dx, dy, dz) = (b[0] - a[0], b[1] - a[1], b[2] - a[2]);
    dx * dx + dy * dy + dz * dz
}

fn insert_desc_f64(sorted_desc: &[f64], val: f64) -> Vec<f64> {
    let pos = sorted_desc
        .binary_search_by(|probe| val.total_cmp(probe))
        .unwrap_or_else(|e| e);
    let mut out = Vec::with_capacity(sorted_desc.len() + 1);
    out.extend_from_slice(&sorted_desc[..pos]);
    out.push(val);
    out.extend_from_slice(&sorted_desc[pos..]);
    out
}

fn cmp_desc_vecs(a: &[f64], b: &[f64]) -> std::cmp::Ordering {
    for (&x, &y) in a.iter().zip(b.iter()) {
        let ord = x.total_cmp(&y);
        if ord != std::cmp::Ordering::Equal {
            return ord;
        }
    }
    a.len().cmp(&b.len())
}

/// Forward §4.3.4 (`refs/text/yang2025_hybrid_boolean.txt:575-593`) on a
/// discrete sample chain `chain[0..k]`:
/// - Starts from the endpoints `(0, k - 1)`; a chord `(i, j)` (`j >= i + 2`) is
///   admissible iff it skips no mandatory index in `anchors` and **every**
///   replaced interior sample `m in (i + 1)..j` passes [`sample_redundant_on_chord`]
///   against `(p_i, p_j)` (atomic mesh edges `j == i + 1` replace zero samples
///   and are always admissible).
/// - Selects a minimum-chord-count admissible path `0 = i_0 < i_1 < ... < i_r = k - 1`,
///   breaking equal-chord-count ties by minimizing the descending-sorted multiset
///   of squared chord lengths `||p_{i_{s+1}} - p_{i_s}||^2`. Because multiset
///   insertion strictly preserves lexicographic order and `sq_chord_len(a, b) ==
///   sq_chord_len(b, a)` bit-for-bit in IEEE-754, the kept sample set does not
///   depend on walk direction.
fn kept_positions(
    chain: &[u32],
    verts: &[Point3],
    anchors: &[usize],
    strict: &BTreeSet<u32>,
) -> Vec<usize> {
    let k = chain.len();
    if k == 0 {
        return Vec::new();
    }
    if k == 1 {
        return vec![0];
    }
    let pts: Vec<[f64; 3]> = chain
        .iter()
        .map(|&v| verts[v as usize].as_array())
        .collect();
    let is_strict: Vec<bool> = chain.iter().map(|v| strict.contains(v)).collect();

    let mut sorted_anchors: Vec<usize> = anchors
        .iter()
        .copied()
        .filter(|&a| a > 0 && a + 1 < k)
        .collect();
    sorted_anchors.sort_unstable();
    sorted_anchors.dedup();

    let chord_admissible = |i: usize, j: usize| -> bool {
        if j == i + 1 {
            return true;
        }
        let (pa, pb) = (pts[i], pts[j]);
        // Fast boundary-neighbour + midpoint rejection (`l` and `h` peak at the
        // two end-adjacent samples and the middle sample respectively) before
        // verifying every remaining replaced sample in `(i + 1)..j`.
        let first_m = i + 1;
        if !sample_redundant_on_chord(pa, pts[first_m], pb, is_strict[first_m]) {
            return false;
        }
        let last_m = j - 1;
        if last_m != first_m && !sample_redundant_on_chord(pa, pts[last_m], pb, is_strict[last_m]) {
            return false;
        }
        let mid_m = (i + j) / 2;
        if mid_m != first_m
            && mid_m != last_m
            && !sample_redundant_on_chord(pa, pts[mid_m], pb, is_strict[mid_m])
        {
            return false;
        }
        for m in (i + 2)..last_m {
            if m != mid_m && !sample_redundant_on_chord(pa, pts[m], pb, is_strict[m]) {
                return false;
            }
        }
        true
    };

    let mut best_steps = vec![usize::MAX; k];
    let mut best_sq: Vec<Vec<f64>> = vec![Vec::new(); k];
    let mut prev = vec![0usize; k];
    best_steps[0] = 0;

    let mut anchor_cursor = 0usize;
    for i in 0..(k - 1) {
        if best_steps[i] == usize::MAX {
            continue;
        }
        while anchor_cursor < sorted_anchors.len() && sorted_anchors[anchor_cursor] <= i {
            anchor_cursor += 1;
        }
        let j_limit = sorted_anchors.get(anchor_cursor).copied().unwrap_or(k - 1);
        let cand_steps = best_steps[i] + 1;
        for j in (i + 1)..=j_limit {
            if cand_steps > best_steps[j] {
                continue;
            }
            if !chord_admissible(i, j) {
                continue;
            }
            let cand_sq = insert_desc_f64(&best_sq[i], sq_chord_len(pts[i], pts[j]));
            let take = if cand_steps < best_steps[j] {
                true
            } else {
                cmp_desc_vecs(&cand_sq, &best_sq[j]) == std::cmp::Ordering::Less
            };
            if take {
                best_steps[j] = cand_steps;
                best_sq[j] = cand_sq;
                prev[j] = i;
            }
        }
    }

    let mut out = Vec::with_capacity(best_steps[k - 1].saturating_add(1));
    let mut cur = k - 1;
    out.push(cur);
    while cur > 0 {
        cur = prev[cur];
        out.push(cur);
    }
    out.reverse();
    out
}

/// Signed directed-edge delta `edges(new) - edges(old)`.
fn signed_edge_delta(
    old_edges: &[(u32, u32)],
    new_edges: &[(u32, u32)],
) -> BTreeMap<(u32, u32), i64> {
    let mut delta: BTreeMap<(u32, u32), i64> = BTreeMap::new();
    let mut add = |u: u32, v: u32, sgn: i64| {
        if u < v {
            *delta.entry((u, v)).or_default() += sgn;
        } else if v < u {
            *delta.entry((v, u)).or_default() -= sgn;
        }
    };
    for &(u, v) in old_edges {
        add(u, v, -1);
    }
    for &(u, v) in new_edges {
        add(u, v, 1);
    }
    delta.retain(|_, &mut v| v != 0);
    delta
}

/// Certify that a planned [`PatchRebuild`] on `patch_tris` produces a manifold,
/// non-degenerate patch triangulation whose boundary change matches
/// `orig_cycles -> mod_cycles` and drops every vertex in `victims`.
#[allow(clippy::too_many_arguments)]
fn certify_patch_rebuild(
    mesh: &Mesh,
    attribution: &TriangleAttributionMap,
    side_labels: &TriangleSideLabelMap,
    raw_patch: &crate::stage5_topology::Patch,
    orig_cycles: &[Vec<u32>],
    mod_cycles: &[Vec<u32>],
    victims: &BTreeSet<u32>,
    rebuild: &PatchRebuild,
) -> bool {
    if rebuild.old_tris.is_empty() {
        return false;
    }
    // Uniform attribution & side-label on replaced triangles (apply_rebuild_batch contract).
    let first_attr = attribution.attributions[rebuild.old_tris[0] as usize];
    if first_attr.is_none()
        || rebuild
            .old_tris
            .iter()
            .any(|&t| attribution.attributions[t as usize] != first_attr)
    {
        return false;
    }
    let first_lbl = side_labels.labels[rebuild.old_tris[0] as usize];
    if rebuild
        .old_tris
        .iter()
        .any(|&t| side_labels.labels[t as usize] != first_lbl)
    {
        return false;
    }
    let old_set: BTreeSet<u32> = rebuild.old_tris.iter().copied().collect();
    if old_set.len() != rebuild.old_tris.len() {
        return false;
    }
    // No surviving triangle in this patch may still reference a dropped victim.
    for &t in &raw_patch.tri_indices {
        if old_set.contains(&t) {
            continue;
        }
        if mesh.tris[t as usize].iter().any(|v| victims.contains(v)) {
            return false;
        }
    }
    // No replacement triangle may repeat a vertex or reference a dropped victim.
    for tri in &rebuild.new_tris {
        if tri[0] == tri[1] || tri[1] == tri[2] || tri[2] == tri[0] {
            return false;
        }
        if tri.iter().any(|v| victims.contains(v)) {
            return false;
        }
    }
    // Manifold directed-edge uniqueness across (surviving non-fold-sliver tris) ∪ new_tris.
    let fold_slivers = patch_fold_slivers(raw_patch, mesh);
    let mut seen_dir: BTreeSet<(u32, u32)> = BTreeSet::new();
    for &t in &raw_patch.tri_indices {
        if old_set.contains(&t) || fold_slivers.contains(&t) {
            continue;
        }
        let tri = mesh.tris[t as usize];
        for k in 0..3 {
            seen_dir.insert((tri[k], tri[(k + 1) % 3]));
        }
    }
    for tri in &rebuild.new_tris {
        for k in 0..3 {
            if !seen_dir.insert((tri[k], tri[(k + 1) % 3])) {
                return false;
            }
        }
    }
    // Exact boundary-delta certificate: net_edges(new_tris) - net_edges(old_tris)
    // must equal net_edges(mod_cycles) - net_edges(orig_cycles).
    let old_tri_edges: Vec<(u32, u32)> = rebuild
        .old_tris
        .iter()
        .flat_map(|&t| {
            let tr = mesh.tris[t as usize];
            [(tr[0], tr[1]), (tr[1], tr[2]), (tr[2], tr[0])]
        })
        .collect();
    let new_tri_edges: Vec<(u32, u32)> = rebuild
        .new_tris
        .iter()
        .flat_map(|tr| [(tr[0], tr[1]), (tr[1], tr[2]), (tr[2], tr[0])])
        .collect();
    let orig_cyc_edges: Vec<(u32, u32)> = orig_cycles
        .iter()
        .flat_map(|c| (0..c.len()).map(move |i| (c[i], c[(i + 1) % c.len()])))
        .collect();
    let mod_cyc_edges: Vec<(u32, u32)> = mod_cycles
        .iter()
        .flat_map(|c| (0..c.len()).map(move |i| (c[i], c[(i + 1) % c.len()])))
        .collect();
    signed_edge_delta(&old_tri_edges, &new_tri_edges)
        == signed_edge_delta(&orig_cyc_edges, &mod_cyc_edges)
}

/// Fan-local §4.4.1 mesh update on one patch (`Plane`, `Cylinder`, `Cone`, or
/// `Torus` via `SurfaceChart::new_local` + `refill_fan_hole_seeded`): partition
/// all patch triangles incident to `victims` (plus any triangle holding the
/// reversed edge of a newly formed boundary chord) into edge-connected disk
/// components, delete 2-vertex ear caps, and CDT-refill `>= 3`-vertex holes.
fn rebuild_decimated_patch_fans(
    mesh: &Mesh,
    patch_index: usize,
    patch: &SplicePatch,
    mod_cycles: &[Vec<u32>],
    victims: &BTreeSet<u32>,
) -> Option<PatchRebuild> {
    SurfaceChart::new_local(patch.surface)?;
    let orig_dir_edges: BTreeSet<(u32, u32)> = patch
        .cycles
        .iter()
        .flat_map(|c| (0..c.len()).map(move |i| (c[i], c[(i + 1) % c.len()])))
        .collect();
    let new_minted_chords: BTreeSet<(u32, u32)> = mod_cycles
        .iter()
        .flat_map(|c| (0..c.len()).map(move |i| (c[i], c[(i + 1) % c.len()])))
        .filter(|e| !orig_dir_edges.contains(e))
        .collect();

    let mut target_tris: Vec<u32> = Vec::new();
    for &t in &patch.tris {
        let tri = mesh.tris[t as usize];
        let touches_victim = tri.iter().any(|v| victims.contains(v));
        let holds_rev_new_chord = (0..3).any(|k| {
            let (x, y) = (tri[k], tri[(k + 1) % 3]);
            new_minted_chords.contains(&(y, x))
        });
        if touches_victim || holds_rev_new_chord {
            target_tris.push(t);
        }
    }
    if target_tris.is_empty() {
        return None;
    }

    // Group target_tris into edge-connected components so kissing bowties at a
    // kept boundary vertex stay independent simple disks while adjacent fans
    // sharing an interior edge merge into one disk.
    let n = target_tris.len();
    let mut parent: Vec<usize> = (0..n).collect();
    fn find(p: &mut [usize], mut x: usize) -> usize {
        while p[x] != x {
            p[x] = p[p[x]];
            x = p[x];
        }
        x
    }
    let mut edge_owner: BTreeMap<(u32, u32), usize> = BTreeMap::new();
    for (idx, &t) in target_tris.iter().enumerate() {
        let tri = mesh.tris[t as usize];
        for k in 0..3 {
            let (x, y) = (tri[k], tri[(k + 1) % 3]);
            let key = (x.min(y), x.max(y));
            if let Some(&prev) = edge_owner.get(&key) {
                let (ra, rb) = (find(&mut parent, idx), find(&mut parent, prev));
                if ra != rb {
                    parent[ra.max(rb)] = ra.min(rb);
                }
            } else {
                edge_owner.insert(key, idx);
            }
        }
    }
    let mut groups: BTreeMap<usize, Vec<u32>> = BTreeMap::new();
    for (idx, &t) in target_tris.iter().enumerate() {
        let r = find(&mut parent, idx);
        groups.entry(r).or_default().push(t);
    }

    let plan_verts = mesh.verts.len() as u32;
    let plan_tris = mesh.tris.len() as u32;
    let mut all_old_tris: Vec<u32> = Vec::with_capacity(target_tris.len());
    let mut all_new_tris: Vec<[u32; 3]> = Vec::new();
    let mut all_new_verts: Vec<Point3> = Vec::new();

    let probe = std::env::var_os("YANG_434_DECIMATE_DECLINE_PROBE").is_some();
    for comp_tris in groups.into_values() {
        let comp_victims: BTreeSet<u32> = comp_tris
            .iter()
            .flat_map(|&t| mesh.tris[t as usize])
            .filter(|v| victims.contains(v))
            .collect();
        if comp_victims.is_empty() {
            if probe {
                eprintln!("[s434-fan-decline] patch={patch_index} reason=isolated_rev_chord");
            }
            return None;
        }
        let mut directed: BTreeSet<(u32, u32)> = BTreeSet::new();
        for &t in &comp_tris {
            let tri = mesh.tris[t as usize];
            for k in 0..3 {
                let (x, y) = (tri[k], tri[(k + 1) % 3]);
                if x == y || !directed.insert((x, y)) {
                    if probe {
                        eprintln!("[s434-fan-decline] patch={patch_index} reason=dup_dir_edge");
                    }
                    return None;
                }
            }
        }
        let mut next: BTreeMap<u32, u32> = BTreeMap::new();
        let mut indeg: BTreeMap<u32, usize> = BTreeMap::new();
        for &(x, y) in &directed {
            if directed.contains(&(y, x)) {
                continue;
            }
            if next.insert(x, y).is_some() {
                if probe {
                    eprintln!("[s434-fan-decline] patch={patch_index} reason=pinched_outdeg v={x}");
                }
                return None;
            }
            *indeg.entry(y).or_default() += 1;
            indeg.entry(x).or_default();
        }
        if indeg.is_empty() || indeg.values().any(|&d| d != 1) || next.len() != indeg.len() {
            if probe {
                eprintln!("[s434-fan-decline] patch={patch_index} reason=non_simple_boundary");
            }
            return None;
        }
        let start = *next.keys().next()?;
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
            if probe {
                eprintln!("[s434-fan-decline] patch={patch_index} reason=multi_loop_hole");
            }
            return None;
        }
        let polygon: Vec<u32> = cycle
            .into_iter()
            .filter(|v| !comp_victims.contains(v))
            .collect();
        let on_polygon: BTreeSet<u32> = polygon.iter().copied().collect();
        if comp_tris
            .iter()
            .flat_map(|&t| mesh.tris[t as usize])
            .any(|v| !comp_victims.contains(&v) && !on_polygon.contains(&v))
        {
            if probe {
                eprintln!("[s434-fan-decline] patch={patch_index} reason=enclosed_interior_vert");
            }
            return None;
        }
        all_old_tris.extend_from_slice(&comp_tris);
        if polygon.len() < 2 {
            if probe {
                eprintln!("[s434-fan-decline] patch={patch_index} reason=poly_lt_2");
            }
            return None;
        }
        if polygon.len() == 2 {
            // Ear-cap deletion: the surviving interior neighbour across
            // `(polygon[0], polygon[1])` already exposes the new boundary chord.
            continue;
        }
        let seed_id_base = plan_verts + all_new_verts.len() as u32;
        let (comp_new_tris, comp_seeds) = match refill_fan_hole_seeded(
            mesh,
            patch_index,
            patch,
            &polygon,
            &comp_tris,
            comp_victims.len(),
            seed_id_base,
            victims,
        ) {
            Ok(res) => res,
            Err(err) => {
                if probe {
                    eprintln!(
                        "[s434-fan-decline] patch={patch_index} poly={} victims={} reason=refill_err({err:?})",
                        polygon.len(),
                        comp_victims.len()
                    );
                }
                return None;
            }
        };
        all_new_tris.extend(comp_new_tris);
        all_new_verts.extend(comp_seeds);
    }

    Some(PatchRebuild {
        patch: patch_index,
        old_tris: all_old_tris,
        new_tris: all_new_tris,
        new_verts: all_new_verts,
        dropped: victims.clone(),
        plan_verts,
        plan_tris,
    })
}

/// Decimate redundant interior samples along `LineSegment` and `SurfacePair`
/// intersection polylines (and sub-resolution collinear subdivisions of
/// straight operand edges, ledger `N105`) inside Stage 4 (§4.4.1), updating
/// every adjacent patch's triangulation conformally before topology emission.
#[allow(clippy::too_many_arguments)]
pub(crate) fn decimate_patch_chains_441(
    mesh: &mut Mesh,
    attribution: &mut TriangleAttributionMap,
    side_labels: &mut TriangleSideLabelMap,
    infos: &[PatchInfo],
    intersection_curves: &BTreeMap<(u32, u32), Curve>,
    a: &BRep,
    b: &BRep,
    full_arrangement: bool,
) -> Result<DecimateStats, YangError> {
    let edges_before: usize = infos
        .iter()
        .map(|info| info.cycles.iter().map(Vec::len).sum::<usize>())
        .sum();
    let mut stats = DecimateStats {
        edges_before,
        edges_after: edges_before,
        ..DecimateStats::default()
    };

    let side_labels_opt = full_arrangement.then_some(&*side_labels);
    let adjacency = triangle_adjacency(mesh);
    let raw = merge_same_plane_patches_with_side_labels(
        flood_fill_patches_with_side_labels(mesh, attribution, side_labels_opt, &adjacency),
        &adjacency,
        side_labels_opt,
        a,
        b,
    );
    if raw.len() != infos.len() {
        return Err(YangError::Stage4ChainDecimateFailed {
            reason: ChainDecimateError::PatchPartitionMismatch {
                raw_patches: raw.len(),
                info_patches: infos.len(),
            },
        });
    }

    let operand_vertex_keys: BTreeSet<[u64; 3]> = a
        .vertices
        .iter()
        .chain(b.vertices.iter())
        .map(|v| point_key(v.point))
        .collect();

    // Per-vertex set of patches holding any mesh triangle on that vertex, so a
    // vertex referenced by a patch outside its boundary-cycle owners is never
    // treated as droppable.
    let mut mesh_patches_of: BTreeMap<u32, BTreeSet<usize>> = BTreeMap::new();
    for (pi, p) in raw.iter().enumerate() {
        for &t in &p.tri_indices {
            for v in mesh.tris[t as usize] {
                mesh_patches_of.entry(v).or_default().insert(pi);
            }
        }
    }

    // Classify every undirected cycle edge `(u, v)` and track its net signed
    // cycle balance (`+1` for `u < v`, `-1` for `u > v`):
    // - `inter`: key in `intersection_curves`
    // - `line`: `LineSegment` (excluding chords certified onto a curved edge by
    //   `certified_output_edge_curve`)
    // - `run_key`: `Some(Line | Pair)` for coalescible run edges, `None` for conics/carried curves.
    let mut edge_owners: BTreeMap<(u32, u32), Vec<usize>> = BTreeMap::new();
    let mut edge_signed_balance: BTreeMap<(u32, u32), i64> = BTreeMap::new();
    for (pi, info) in infos.iter().enumerate() {
        for cyc in &info.cycles {
            for &(s, e) in cyc {
                let key = (s.min(e), s.max(e));
                edge_owners.entry(key).or_default().push(pi);
                if s < e {
                    *edge_signed_balance.entry(key).or_default() += 1;
                } else if s > e {
                    *edge_signed_balance.entry(key).or_default() -= 1;
                }
            }
        }
    }
    let mut edge_class: BTreeMap<(u32, u32), (bool, bool, Option<RunKey>)> = BTreeMap::new();
    let mut restore_scratch = RestoreStats::default();
    for (&key, owners) in &edge_owners {
        if let Some(c) = intersection_curves.get(&key) {
            let line = matches!(c, Curve::LineSegment);
            let rk = run_key_of_curve(c);
            edge_class.insert(key, (true, line, rk));
            continue;
        }
        // Non-intersection edge: check whether `certified_output_edge_curve`
        // certifies it onto a typed curve (`Circle`, `Ellipse`, `Hyperbola`,
        // `SurfacePair`) — either from a carried operand edge or from the
        // owner surfaces' intersection. If so, it is NOT a straight `LineSegment`.
        let mut carried_curved = false;
        let (p, q) = (mesh.verts[key.0 as usize], mesh.verts[key.1 as usize]);
        'owners_pair: for oi in 0..owners.len() {
            for oj in (oi + 1)..owners.len() {
                if owners[oi] == owners[oj] {
                    continue;
                }
                let (i0, i1) = (&infos[owners[oi]], &infos[owners[oj]]);
                let carried_input = if i0.input == i1.input && i0.face_idx != i1.face_idx {
                    let input = match i0.input {
                        InputId::A => (a.faces.as_slice(), a.edges.as_slice()),
                        InputId::B => (b.faces.as_slice(), b.edges.as_slice()),
                    };
                    Some((input, i0.face_idx as u32, i1.face_idx as u32))
                } else {
                    None
                };
                if certified_output_edge_curve(
                    p,
                    q,
                    i0.inherited,
                    i1.inherited,
                    carried_input,
                    &mut restore_scratch,
                )
                .is_some()
                {
                    carried_curved = true;
                    break 'owners_pair;
                }
            }
        }
        if carried_curved {
            edge_class.insert(key, (false, false, None));
        } else {
            edge_class.insert(key, (false, true, Some(RunKey::Line)));
        }
    }
    let edge_run_keys: BTreeMap<(u32, u32), Option<RunKey>> =
        edge_class.iter().map(|(&k, &(_, _, rk))| (k, rk)).collect();

    // Global cycle-use census per vertex.
    struct VertUse {
        count: u32,
        patches: BTreeSet<usize>,
        n_intersection: u32,
        all_line: bool,
        all_balanced: bool,
    }
    let mut uses: BTreeMap<u32, VertUse> = BTreeMap::new();
    for (pi, info) in infos.iter().enumerate() {
        for cyc in &info.cycles {
            for &(s, e) in cyc {
                let key = (s.min(e), s.max(e));
                let &(inter, line, _) = edge_class.get(&key).ok_or({
                    YangError::Stage4ChainDecimateFailed {
                        reason: ChainDecimateError::UnclassifiedCycleEdge { edge: key },
                    }
                })?;
                let balanced = edge_signed_balance.get(&key).copied().unwrap_or(1) == 0;
                for v in [s, e] {
                    let u = uses.entry(v).or_insert_with(|| VertUse {
                        count: 0,
                        patches: BTreeSet::new(),
                        n_intersection: 0,
                        all_line: true,
                        all_balanced: true,
                    });
                    u.count += 1;
                    u.patches.insert(pi);
                    u.n_intersection += u32::from(inter);
                    u.all_line &= line;
                    u.all_balanced &= balanced;
                }
            }
        }
    }

    let max_isect_owners = if full_arrangement { 4 } else { 2 };
    let droppable: BTreeSet<u32> = uses
        .iter()
        .filter(|(&v, u)| {
            if !u.all_balanced || mesh_patches_of.get(&v) != Some(&u.patches) {
                return false;
            }
            let n_p = u.patches.len();
            if u.count != (2 * n_p) as u32 {
                return false;
            }
            let is_isect_interior =
                (2..=max_isect_owners).contains(&n_p) && u.n_intersection == u.count;
            let is_straight_subdiv = n_p == 2 && u.n_intersection == 0 && u.all_line;
            is_isect_interior || is_straight_subdiv
        })
        .map(|(&v, _)| v)
        .collect();
    let strict: BTreeSet<u32> = uses
        .iter()
        .filter(|(&v, u)| droppable.contains(&v) && u.n_intersection == 0)
        .map(|(&v, _)| v)
        .collect();

    // Read-only diagnostic probe (`YANG_434_DECIMATE_PROBE=x,y,z[;x,y,z...]`).
    if let Ok(spec) = std::env::var("YANG_434_DECIMATE_PROBE") {
        for site in spec.split(';') {
            let c: Vec<f64> = site
                .split(',')
                .filter_map(|s| s.trim().parse().ok())
                .collect();
            if c.len() != 3 {
                continue;
            }
            let Some((&v, u)) = uses.iter().min_by(|(va, _), (vb, _)| {
                let d = |i: u32| {
                    let p = mesh.verts[i as usize];
                    (p.x() - c[0]).powi(2) + (p.y() - c[1]).powi(2) + (p.z() - c[2]).powi(2)
                };
                d(**va)
                    .partial_cmp(&d(**vb))
                    .unwrap_or(std::cmp::Ordering::Equal)
            }) else {
                continue;
            };
            let p = mesh.verts[v as usize];
            eprintln!(
                "[s434-decimate-probe] site=({},{},{}) nearest v{v}=({:.9e},{:.9e},{:.9e}) \
                 count={} patches={:?} n_intersection={} all_line={} balanced={} droppable={}",
                c[0],
                c[1],
                c[2],
                p.x(),
                p.y(),
                p.z(),
                u.count,
                u.patches,
                u.n_intersection,
                u.all_line,
                u.all_balanced,
                droppable.contains(&v)
            );
        }
    }

    if droppable.is_empty() {
        return Ok(stats);
    }

    // Decisions per canonical chain (`Some(kept)` or `None` = declined).
    let mut decisions: BTreeMap<Vec<u32>, Option<Vec<usize>>> = BTreeMap::new();
    struct ChainRef {
        canon: Vec<u32>,
        min_pos: usize,
        reversed: bool,
    }
    struct CyclePlan {
        orig_cycle: Vec<u32>,
        part: LoopPartition,
        chains: Vec<Option<ChainRef>>,
    }
    let mut plans: Vec<Vec<Option<CyclePlan>>> = Vec::with_capacity(infos.len());
    for info in infos {
        let mut patch_plans = Vec::with_capacity(info.cycles.len());
        for cyc in &info.cycles {
            let Some(cycle) = patch_cycle_verts(cyc) else {
                stats.skipped_discontinuous_loops += 1;
                patch_plans.push(None);
                continue;
            };
            let part = partition_cycle(cyc, &cycle, &edge_run_keys, &droppable);
            let m = cyc.len();
            let mut chains = Vec::with_capacity(part.stretches.len());
            for &(start_r, len, key) in &part.stretches {
                if key.is_none() || (!part.whole_closed && len < 2) {
                    chains.push(None);
                    continue;
                }
                let orig_pos = |r: usize| (part.offset + r) % m;
                let mut chain: Vec<u32> = Vec::with_capacity(len + 1);
                for r in start_r..start_r + len {
                    chain.push(cycle[orig_pos(r)]);
                }
                if part.whole_closed {
                    let (rotated, min_pos, reversed) = canonical_closed_chain(&chain, &mesh.verts);
                    let k_c = rotated.len();
                    if let std::collections::btree_map::Entry::Vacant(slot) =
                        decisions.entry(rotated.clone())
                    {
                        if k_c < 4 {
                            slot.insert(None);
                        } else {
                            let op_pos: Vec<usize> = rotated
                                .iter()
                                .enumerate()
                                .filter(|&(_, &v)| {
                                    operand_vertex_keys.contains(&point_key(mesh.verts[v as usize]))
                                })
                                .map(|(idx, _)| idx)
                                .collect();
                            let kept: Vec<usize> = if let Some(&p0) = op_pos.first() {
                                // Authored operand BRepVertex(es) on this closed loop:
                                // open at `p0` and hold all remaining operand vertices
                                // as mandatory anchors so the DP splits independently
                                // across each inter-anchor arc (world-frame-invariant).
                                let mut closed_anchored: Vec<u32> =
                                    (0..k_c).map(|step| rotated[(p0 + step) % k_c]).collect();
                                closed_anchored.push(rotated[p0]);
                                let anchors: Vec<usize> =
                                    op_pos[1..].iter().map(|&p| (p + k_c - p0) % k_c).collect();
                                let mut mapped: Vec<usize> = kept_positions(
                                    &closed_anchored,
                                    &mesh.verts,
                                    &anchors,
                                    &strict,
                                )
                                .into_iter()
                                .filter(|&i| i < k_c)
                                .map(|i| (i + p0) % k_c)
                                .collect();
                                mapped.sort_unstable();
                                mapped
                            } else {
                                // Unanchored closed loop (`0` surviving operand BRepVertexes):
                                // open at `rotated[0]` (`point_cmp` minimum 3D sample).
                                let mut closed = rotated.clone();
                                closed.push(rotated[0]);
                                kept_positions(&closed, &mesh.verts, &[], &strict)
                                    .into_iter()
                                    .filter(|&i| i < k_c)
                                    .collect()
                            };
                            if kept.len() >= 3 {
                                slot.insert(Some(kept));
                            } else {
                                stats.declined_loop_floor += 1;
                                slot.insert(None);
                            }
                        }
                    }
                    chains.push(Some(ChainRef {
                        canon: rotated,
                        min_pos,
                        reversed,
                    }));
                } else {
                    chain.push(cycle[orig_pos(start_r + len) % m]);
                    let (canon, reversed) = canonical_open_chain(&chain, &mesh.verts);
                    decisions
                        .entry(canon.clone())
                        .or_insert_with(|| Some(kept_positions(&canon, &mesh.verts, &[], &strict)));
                    chains.push(Some(ChainRef {
                        canon,
                        min_pos: 0,
                        reversed,
                    }));
                }
            }
            patch_plans.push(Some(CyclePlan {
                orig_cycle: cycle,
                part,
                chains,
            }));
        }
        plans.push(patch_plans);
    }

    // Enforce the 3-edge cycle floor and per-patch mesh rebuildability to a
    // joint fixpoint: any chain whose owner cycle would fall below 3 edges OR
    // whose owner patch declines its mesh rebuild is declined across ALL its
    // owners, preserving conformal boundaries.
    let enforce_loop_floor = |decisions: &mut BTreeMap<Vec<u32>, Option<Vec<usize>>>,
                              declined_loop_floor: &mut usize| loop {
        let mut changed = false;
        for patch_plans in &plans {
            for plan in patch_plans.iter().flatten() {
                let mut after = 0usize;
                for (si, &(_, len, _)) in plan.part.stretches.iter().enumerate() {
                    match plan.chains[si]
                        .as_ref()
                        .and_then(|cr| decisions.get(&cr.canon))
                    {
                        Some(Some(kept)) => {
                            after += if plan.part.whole_closed {
                                kept.len()
                            } else {
                                kept.len().saturating_sub(1)
                            };
                        }
                        _ => after += len,
                    }
                }
                if after < 3 {
                    for cr in plan.chains.iter().flatten() {
                        if let Some(d) = decisions.get_mut(&cr.canon) {
                            if d.is_some() {
                                *d = None;
                                *declined_loop_floor += 1;
                                changed = true;
                            }
                        }
                    }
                }
            }
        }
        if !changed {
            break;
        }
    };

    enforce_loop_floor(&mut decisions, &mut stats.declined_loop_floor);

    let d_eps = stage4_chord_band(a, b);
    let rebuilds: Vec<PatchRebuild> = 'plan_batch: loop {
        // Any discontinuous cycle on a patch forces all chains on that patch to
        // decline so we never rebuild against incomplete cycle data.
        let mut disc_declined = false;
        for patch_plans in &plans {
            if patch_plans.iter().any(Option::is_none) {
                for plan in patch_plans.iter().flatten() {
                    for cr in plan.chains.iter().flatten() {
                        if let Some(d) = decisions.get_mut(&cr.canon) {
                            if d.as_ref().is_some_and(|k| k.len() < cr.canon.len()) {
                                *d = None;
                                stats.declined_discontinuous_patch += 1;
                                disc_declined = true;
                            }
                        }
                    }
                }
            }
        }
        if disc_declined {
            enforce_loop_floor(&mut decisions, &mut stats.declined_loop_floor);
        }

        let mut batch: Vec<PatchRebuild> = Vec::new();
        for (pi, (info, raw_patch)) in infos.iter().zip(raw.iter()).enumerate() {
            let mut orig_cycles: Vec<Vec<u32>> = Vec::with_capacity(info.cycles.len());
            let mut mod_cycles: Vec<Vec<u32>> = Vec::with_capacity(info.cycles.len());
            let mut patch_changed = false;
            let mut patch_active_chains: Vec<Vec<u32>> = Vec::new();

            for plan_slot in plans[pi].iter().take(info.cycles.len()) {
                let Some(plan) = plan_slot.as_ref() else {
                    break;
                };
                orig_cycles.push(plan.orig_cycle.clone());
                let m = plan.orig_cycle.len();
                let orig_pos = |r: usize| (plan.part.offset + r) % m;
                let mut new_cyc: Vec<u32> = Vec::with_capacity(m);
                let mut covers: Vec<Vec<usize>> = Vec::with_capacity(m);

                for (si, &(start_r, len, _)) in plan.part.stretches.iter().enumerate() {
                    let dec = plan.chains[si]
                        .as_ref()
                        .and_then(|cr| decisions.get(&cr.canon).map(|d| (cr, d)));
                    let Some((cr, Some(kept))) = dec else {
                        for r in start_r..start_r + len {
                            new_cyc.push(plan.orig_cycle[orig_pos(r)]);
                            covers.push(vec![orig_pos(r)]);
                        }
                        continue;
                    };
                    if kept.len() == cr.canon.len() {
                        for r in start_r..start_r + len {
                            new_cyc.push(plan.orig_cycle[orig_pos(r)]);
                            covers.push(vec![orig_pos(r)]);
                        }
                        continue;
                    }
                    patch_changed = true;
                    patch_active_chains.push(cr.canon.clone());
                    let k_c = cr.canon.len();
                    let spans: Vec<(usize, usize)> = if plan.part.whole_closed {
                        (0..kept.len())
                            .map(|i| (kept[i], kept[(i + 1) % kept.len()]))
                            .collect()
                    } else {
                        (0..kept.len() - 1)
                            .map(|i| (kept[i], kept[i + 1]))
                            .collect()
                    };
                    let ordered: Vec<(usize, usize)> = if cr.reversed {
                        spans.iter().rev().map(|&(sa, sb)| (sb, sa)).collect()
                    } else {
                        spans
                    };
                    let chain_pos = |ci: usize| -> usize {
                        if plan.part.whole_closed {
                            if cr.reversed {
                                (cr.min_pos + k_c - ci) % k_c
                            } else {
                                (cr.min_pos + ci) % k_c
                            }
                        } else if cr.reversed {
                            k_c - 1 - ci
                        } else {
                            ci
                        }
                    };
                    for &(sa, sb) in &ordered {
                        new_cyc.push(cr.canon[sa]);
                        let (ta, tb) = (chain_pos(sa), chain_pos(sb));
                        let mut cov = Vec::new();
                        let mut idx = ta;
                        loop {
                            cov.push(orig_pos(start_r + idx));
                            idx = (idx + 1) % k_c;
                            if idx == tb || (!plan.part.whole_closed && idx == 0) {
                                break;
                            }
                        }
                        covers.push(cov);
                    }
                }
                if let Some(k) = covers.iter().position(|c| c.contains(&0)) {
                    new_cyc.rotate_left(k);
                }
                mod_cycles.push(new_cyc);
            }

            if !patch_changed {
                continue;
            }

            let kept_set: BTreeSet<u32> = mod_cycles.iter().flatten().copied().collect();
            let victims: BTreeSet<u32> = orig_cycles
                .iter()
                .flatten()
                .copied()
                .filter(|v| !kept_set.contains(v))
                .collect();
            if victims.is_empty() {
                return Err(YangError::Stage4ChainDecimateFailed {
                    reason: ChainDecimateError::FixpointStall { patch: pi },
                });
            }

            // `SurfaceChart` has no sphere chart (`stage4_project.rs`): count
            // and decline explicitly rather than conflating with CDT declines.
            if matches!(info.inherited, Surface::Sphere { .. }) {
                let mut any = false;
                for c in patch_active_chains {
                    if let Some(d) = decisions.get_mut(&c) {
                        if d.is_some() {
                            *d = None;
                            stats.declined_sphere_unsupported += 1;
                            any = true;
                        }
                    }
                }
                if !any {
                    return Err(YangError::Stage4ChainDecimateFailed {
                        reason: ChainDecimateError::FixpointStall { patch: pi },
                    });
                }
                enforce_loop_floor(&mut decisions, &mut stats.declined_loop_floor);
                continue 'plan_batch;
            }

            let orig_splice = SplicePatch {
                cycles: orig_cycles.clone(),
                tris: raw_patch.tri_indices.clone(),
                surface: info.inherited,
            };
            // `patch_from_cycles_shifted` (`stage4_project.rs:512`) reads `cycles[0]`
            // as the outer boundary and `cycles[1..]` as holes, whereas
            // `patch_boundary_cycle` orders orbits by lowest `u32` vertex index.
            // Place the largest-|Newell area| cycle at index 0 so holed-patch CDT
            // never depends on vertex numbering.
            let ordered_mod_cycles = if mod_cycles.len() > 1 {
                let newell_mag = |c: &[u32]| -> f64 {
                    if c.len() < 3 {
                        return 0.0;
                    }
                    let o = mesh.verts[c[0] as usize].as_array();
                    let (mut nx, mut ny, mut nz) = (0.0f64, 0.0f64, 0.0f64);
                    for i in 0..c.len() {
                        let a = mesh.verts[c[i] as usize].as_array();
                        let b = mesh.verts[c[(i + 1) % c.len()] as usize].as_array();
                        let (ax, ay, az) = (a[0] - o[0], a[1] - o[1], a[2] - o[2]);
                        let (bx, by, bz) = (b[0] - o[0], b[1] - o[1], b[2] - o[2]);
                        nx += ay * bz - az * by;
                        ny += az * bx - ax * bz;
                        nz += ax * by - ay * bx;
                    }
                    (nx * nx + ny * ny + nz * nz).sqrt()
                };
                let outer_idx = (0..mod_cycles.len())
                    .max_by(|&ia, &ib| {
                        newell_mag(&mod_cycles[ia]).total_cmp(&newell_mag(&mod_cycles[ib]))
                    })
                    .unwrap_or(0);
                let mut v = Vec::with_capacity(mod_cycles.len());
                v.push(mod_cycles[outer_idx].clone());
                for (ci, cyc) in mod_cycles.iter().enumerate() {
                    if ci != outer_idx {
                        v.push(cyc.clone());
                    }
                }
                v
            } else {
                mod_cycles.clone()
            };
            let mod_splice = SplicePatch {
                cycles: ordered_mod_cycles,
                tris: raw_patch.tri_indices.clone(),
                surface: info.inherited,
            };

            // Primary: fan-local CDT refill (`Plane`, `Cylinder`, `Cone`, `Torus`).
            // Fallback: whole-patch CDT (`Plane`, `Cylinder`, `Cone`) excluding `victims`.
            let fan_raw =
                rebuild_decimated_patch_fans(mesh, pi, &orig_splice, &mod_cycles, &victims);
            let fan_ok = fan_raw.as_ref().is_some_and(|r| {
                certify_patch_rebuild(
                    mesh,
                    attribution,
                    side_labels,
                    raw_patch,
                    &orig_cycles,
                    &mod_cycles,
                    &victims,
                    r,
                )
            });
            let candidate = if fan_ok {
                fan_raw
            } else {
                let whole = if info.had_fold_sliver || !SurfaceChart::supports(&info.inherited) {
                    None
                } else {
                    rebuild_patch_planar_bounded_ex(mesh, pi, &mod_splice, d_eps, &victims)
                        .ok()
                        .filter(|r| {
                            certify_patch_rebuild(
                                mesh,
                                attribution,
                                side_labels,
                                raw_patch,
                                &orig_cycles,
                                &mod_cycles,
                                &victims,
                                r,
                            )
                        })
                };
                if whole.is_none() && std::env::var_os("YANG_434_DECIMATE_DECLINE_PROBE").is_some()
                {
                    eprintln!(
                        "[s434-decimate-decline] patch={pi} surf={:?} victims={} tris={} fan_some={} had_sliver={} active_chains={}",
                        info.inherited,
                        victims.len(),
                        raw_patch.tri_indices.len(),
                        fan_raw.is_some(),
                        info.had_fold_sliver,
                        patch_active_chains.len(),
                    );
                }
                whole
            };

            match candidate {
                Some(r) => batch.push(r),
                None => {
                    // Decline this patch's active chains across all owners and re-plan.
                    let mut any = false;
                    for c in patch_active_chains {
                        if let Some(d) = decisions.get_mut(&c) {
                            if d.is_some() {
                                *d = None;
                                stats.declined_patch_rebuild += 1;
                                any = true;
                            }
                        }
                    }
                    if !any {
                        return Err(YangError::Stage4ChainDecimateFailed {
                            reason: ChainDecimateError::FixpointStall { patch: pi },
                        });
                    }
                    enforce_loop_floor(&mut decisions, &mut stats.declined_loop_floor);
                    continue 'plan_batch;
                }
            }
        }
        break batch;
    };

    if rebuilds.is_empty() {
        return Ok(stats);
    }

    // Batch conformal boundary postcondition (post-keep overall, or per-operand
    // M_A / M_B on the full arrangement — matching `run_construct_passes`).
    // Each rebuild `ri`'s appended seed vertices (`r.new_verts`) are assigned
    // the exact prefix-sum base index `apply_rebuild_batch` will allocate
    // (`seed_bases[ri] = n_verts + sum_{j < ri} rebuilds[j].new_verts.len()`).
    let n_verts = mesh.verts.len() as u32;
    let mut seed_bases: Vec<u32> = Vec::with_capacity(rebuilds.len());
    let mut next_seed_base = n_verts;
    for r in &rebuilds {
        seed_bases.push(next_seed_base);
        next_seed_base += r.new_verts.len() as u32;
    }

    let boundary_leaks = if full_arrangement {
        [InputId::A, InputId::B].into_iter().any(|op_id| {
            let mut old_op: Vec<[u32; 3]> = Vec::new();
            let mut new_op: Vec<[u32; 3]> = Vec::new();
            for (ri, r) in rebuilds.iter().enumerate() {
                let first_t = match r.old_tris.first() {
                    Some(&t) => t as usize,
                    None => continue,
                };
                let lbl = side_labels.labels[first_t];
                let slot = match op_id {
                    InputId::A => 0,
                    InputId::B => 1,
                };
                if !lbl.surface[slot] {
                    continue;
                }
                let flip = attribution.attributions[first_t].is_some_and(|at| at.input != op_id)
                    && lbl.sheet_opposite == Some(true);
                for &t in &r.old_tris {
                    let mut tri = mesh.tris[t as usize];
                    if flip {
                        tri.swap(1, 2);
                    }
                    old_op.push(tri);
                }
                let seed_base = seed_bases[ri];
                for t in &r.new_tris {
                    let mut tri = t.map(|v| {
                        if v >= r.plan_verts {
                            seed_base + (v - r.plan_verts)
                        } else {
                            v
                        }
                    });
                    if flip {
                        tri.swap(1, 2);
                    }
                    new_op.push(tri);
                }
            }
            directed_boundary(&old_op) != directed_boundary(&new_op)
        })
    } else {
        let old_all: Vec<[u32; 3]> = rebuilds
            .iter()
            .flat_map(|r| r.old_tris.iter().map(|&t| mesh.tris[t as usize]))
            .collect();
        let new_all: Vec<[u32; 3]> = rebuilds
            .iter()
            .enumerate()
            .flat_map(|(ri, r)| {
                let seed_base = seed_bases[ri];
                r.new_tris.iter().map(move |t| {
                    t.map(|v| {
                        if v >= r.plan_verts {
                            seed_base + (v - r.plan_verts)
                        } else {
                            v
                        }
                    })
                })
            })
            .collect();
        directed_boundary(&old_all) != directed_boundary(&new_all)
    };
    if boundary_leaks {
        return Err(YangError::Stage4ChainDecimateFailed {
            reason: ChainDecimateError::BoundaryLeak {
                rebuilds: rebuilds.len(),
                full_arrangement,
            },
        });
    }

    if let Err(err) = apply_rebuild_batch(mesh, attribution, side_labels, &rebuilds) {
        return Err(YangError::Stage4ChainDecimateFailed {
            reason: ChainDecimateError::BatchWritebackFailed {
                detail: format!("{err:?}"),
            },
        });
    }

    // Tally canonical chains that actually dropped interior vertices.
    for (canon, dec) in &decisions {
        if let Some(kept) = dec {
            if kept.len() < canon.len() {
                stats.runs_decimated += 1;
                stats.verts_dropped += canon.len() - kept.len();
            }
        }
    }
    let owner_mult: usize = decisions
        .iter()
        .filter_map(|(canon, dec)| {
            let kept = dec.as_ref()?;
            if kept.len() >= canon.len() {
                return None;
            }
            let n_owners = canon
                .get(1)
                .and_then(|v| uses.get(v))
                .map_or(2, |u| u.patches.len());
            Some((canon.len() - kept.len()) * n_owners)
        })
        .sum();
    stats.edges_after = stats.edges_before.saturating_sub(owner_mult);
    Ok(stats)
}
