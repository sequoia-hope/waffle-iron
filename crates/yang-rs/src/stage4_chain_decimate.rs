//! §4.4.1 mesh-level curve-chain decimation (F2 / deviation N81, matrix row
//! `4.4-28`; Yang et al. 2025 §4.3.4 / §4.4.1,
//! `refs/text/yang2025_hybrid_boolean.txt:546-572, 575-593, 605-607`, spec
//! `specs/yang_434_output_chord_refinement.md` and
//! `specs/yang_442_classify_after_update.md` §3(d) #3).
//!
//! Formerly `stage5_chain_decimate` (which dropped redundant interior vertices
//! from Stage-5 B-Rep loops while leaving them in `mesh.tris` and ordered
//! chains by `u32` vertex index). Re-sited into §4.4.1 so the polyline
//! decimation updates the adjacent mesh patches conformally before topology
//! emission and canonicalizes every open and closed chain purely by 3D
//! coordinates (`point_cmp`), independent of vertex numbering and compatible
//! with both the post-keep submesh and the full welded arrangement.

use std::collections::{BTreeMap, BTreeSet};

use cad_primitives::Point3;
use cherchi_rs::Mesh;

use crate::brep::{BRep, InputId, TriangleAttributionMap, TriangleSideLabelMap};
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
use crate::stage5_seam_merge::point_cmp;
use crate::stage5_topology::{
    flood_fill_patches_with_side_labels, patch_fold_slivers, triangle_adjacency,
};

/// Pass statistics (logged at the Stage-4 call site when `c441_verbose()`).
#[derive(Debug, Default, Clone, Copy)]
pub(crate) struct DecimateStats {
    pub edges_before: usize,
    pub edges_after: usize,
    /// Canonical chains that dropped at least one interior vertex.
    pub runs_decimated: usize,
    /// Interior vertices dropped (counted once per canonical chain).
    pub verts_dropped: usize,
    /// Chains declined because an owner cycle would fall below 3 edges or
    /// because an owner patch declined its mesh rebuild.
    pub declined_floor: usize,
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
/// `u32` vertex index).
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
/// lexicographically.
fn canonical_closed_chain(chain: &[u32], verts: &[Point3]) -> (Vec<u32>, bool) {
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
    (rotated, reversed)
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

/// STRICT redundancy for a subdivision vertex of a straight operand edge:
/// `m` lies on the segment `a -> b` to working precision
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

fn kept_positions(
    chain: &[u32],
    verts: &[Point3],
    anchors: &[usize],
    strict: &BTreeSet<u32>,
) -> Vec<usize> {
    let k = chain.len();
    let mut kept: Vec<usize> = vec![0];
    for i in 1..k.saturating_sub(1) {
        let last = *kept.last().expect("nonempty");
        let (a, m, b) = (
            verts[chain[last] as usize].as_array(),
            verts[chain[i] as usize].as_array(),
            verts[chain[i + 1] as usize].as_array(),
        );
        let redundant = if strict.contains(&chain[i]) {
            strict_subdivision_redundant(a, m, b)
        } else {
            paper_chain_sample_redundant(a, m, b)
        };
        let drop = !anchors.contains(&i) && redundant;
        if !drop {
            kept.push(i);
        }
    }
    if k >= 2 {
        kept.push(k - 1);
    }
    kept
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
/// straight operand edges) inside Stage 4 (§4.4.1), updating every adjacent
/// patch's triangulation conformally before topology emission.
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
) -> DecimateStats {
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
        return stats;
    }

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

    // Classify every undirected cycle edge `(u, v)`:
    // - `inter`: key in `intersection_curves`
    // - `line`: `LineSegment` (excluding same-input chords that restore onto a
    //   carried curved edge of `a` or `b`)
    // - `run_key`: `Some(Line | Pair)` for coalescible run edges, `None` for conics/carried curves.
    let mut edge_owners: BTreeMap<(u32, u32), Vec<usize>> = BTreeMap::new();
    for (pi, info) in infos.iter().enumerate() {
        for cyc in &info.cycles {
            for &(s, e) in cyc {
                edge_owners
                    .entry((s.min(e), s.max(e)))
                    .or_default()
                    .push(pi);
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
        // Non-intersection edge: check whether `restore_carried_edge_curves`
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
    }
    let mut uses: BTreeMap<u32, VertUse> = BTreeMap::new();
    for (pi, info) in infos.iter().enumerate() {
        for cyc in &info.cycles {
            for &(s, e) in cyc {
                let key = (s.min(e), s.max(e));
                let (inter, line, _) =
                    edge_class
                        .get(&key)
                        .copied()
                        .unwrap_or((false, true, Some(RunKey::Line)));
                for v in [s, e] {
                    let u = uses.entry(v).or_insert_with(|| VertUse {
                        count: 0,
                        patches: BTreeSet::new(),
                        n_intersection: 0,
                        all_line: true,
                    });
                    u.count += 1;
                    u.patches.insert(pi);
                    u.n_intersection += u32::from(inter);
                    u.all_line &= line;
                }
            }
        }
    }

    let max_isect_owners = if full_arrangement { 4 } else { 2 };
    let droppable: BTreeSet<u32> = uses
        .iter()
        .filter(|(&v, u)| {
            if mesh_patches_of.get(&v) != Some(&u.patches) {
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
                 count={} patches={:?} n_intersection={} all_line={} droppable={}",
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
                droppable.contains(&v)
            );
        }
    }

    if droppable.is_empty() {
        return stats;
    }

    // Decisions per canonical chain (`Some(kept)` or `None` = declined).
    let mut decisions: BTreeMap<Vec<u32>, Option<Vec<usize>>> = BTreeMap::new();
    struct CyclePlan {
        orig_cycle: Vec<u32>,
        part: LoopPartition,
        chains: Vec<Option<(Vec<u32>, bool)>>,
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
                    let (rotated, reversed) = canonical_closed_chain(&chain, &mesh.verts);
                    let k_c = rotated.len();
                    decisions.entry(rotated.clone()).or_insert_with(|| {
                        if k_c < 4 {
                            return None;
                        }
                        let mut closed = rotated.clone();
                        closed.push(rotated[0]);
                        let anchors = [k_c / 3, (2 * k_c) / 3];
                        let kept: Vec<usize> =
                            kept_positions(&closed, &mesh.verts, &anchors, &strict)
                                .into_iter()
                                .filter(|&i| i < k_c)
                                .collect();
                        (kept.len() >= 3).then_some(kept)
                    });
                    chains.push(Some((rotated, reversed)));
                } else {
                    chain.push(cycle[orig_pos(start_r + len) % m]);
                    let (canon, reversed) = canonical_open_chain(&chain, &mesh.verts);
                    decisions
                        .entry(canon.clone())
                        .or_insert_with(|| Some(kept_positions(&canon, &mesh.verts, &[], &strict)));
                    chains.push(Some((canon, reversed)));
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
    // whose owner patch declines both fan-local CDT and whole-patch CDT is
    // declined across ALL its owners, preserving conformal boundaries.
    let enforce_loop_floor = |decisions: &mut BTreeMap<Vec<u32>, Option<Vec<usize>>>,
                              declined_floor: &mut usize| loop {
        let mut changed = false;
        for patch_plans in &plans {
            for plan in patch_plans.iter().flatten() {
                let mut after = 0usize;
                for (si, &(_, len, _)) in plan.part.stretches.iter().enumerate() {
                    match plan.chains[si].as_ref().and_then(|(c, _)| decisions.get(c)) {
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
                    for (c, _) in plan.chains.iter().flatten() {
                        if let Some(d) = decisions.get_mut(c) {
                            if d.is_some() {
                                *d = None;
                                *declined_floor += 1;
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

    enforce_loop_floor(&mut decisions, &mut stats.declined_floor);

    let d_eps = stage4_chord_band(a, b);
    let rebuilds: Vec<PatchRebuild> = 'plan_batch: loop {
        // Any discontinuous cycle on a patch forces all chains on that patch to
        // decline so we never rebuild against incomplete cycle data.
        let mut disc_declined = false;
        for (pi, patch_plans) in plans.iter().enumerate() {
            if patch_plans.iter().any(Option::is_none) {
                for plan in patch_plans.iter().flatten() {
                    for (c, _) in plan.chains.iter().flatten() {
                        if let Some(d) = decisions.get_mut(c) {
                            if d.is_some() && d.as_ref().is_some_and(|k| k.len() < c.len()) {
                                *d = None;
                                stats.declined_floor += 1;
                                disc_declined = true;
                            }
                        }
                    }
                }
                let _ = pi;
            }
        }
        if disc_declined {
            enforce_loop_floor(&mut decisions, &mut stats.declined_floor);
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
                        .and_then(|(c, rev)| decisions.get(c).map(|d| (c, *rev, d)));
                    let Some((canon, reversed, Some(kept))) = dec else {
                        for r in start_r..start_r + len {
                            new_cyc.push(plan.orig_cycle[orig_pos(r)]);
                            covers.push(vec![orig_pos(r)]);
                        }
                        continue;
                    };
                    if kept.len() == canon.len() {
                        for r in start_r..start_r + len {
                            new_cyc.push(plan.orig_cycle[orig_pos(r)]);
                            covers.push(vec![orig_pos(r)]);
                        }
                        continue;
                    }
                    patch_changed = true;
                    patch_active_chains.push(canon.clone());
                    let k_c = canon.len();
                    let spans: Vec<(usize, usize)> = if plan.part.whole_closed {
                        (0..kept.len())
                            .map(|i| (kept[i], kept[(i + 1) % kept.len()]))
                            .collect()
                    } else {
                        (0..kept.len() - 1)
                            .map(|i| (kept[i], kept[i + 1]))
                            .collect()
                    };
                    let ordered: Vec<(usize, usize)> = if reversed {
                        spans.iter().rev().map(|&(sa, sb)| (sb, sa)).collect()
                    } else {
                        spans
                    };
                    let trav_index = |ci: usize| -> usize {
                        if reversed {
                            (k_c - 1 - ci) % k_c
                        } else {
                            ci
                        }
                    };
                    for &(sa, sb) in &ordered {
                        new_cyc.push(canon[sa]);
                        let (ta, tb) = (trav_index(sa), trav_index(sb));
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
                continue;
            }

            let orig_splice = SplicePatch {
                cycles: orig_cycles.clone(),
                tris: raw_patch.tri_indices.clone(),
                surface: info.inherited,
            };
            let mod_splice = SplicePatch {
                cycles: mod_cycles.clone(),
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
                                stats.declined_floor += 1;
                                any = true;
                            }
                        }
                    }
                    if !any {
                        break 'plan_batch Vec::new();
                    }
                    enforce_loop_floor(&mut decisions, &mut stats.declined_floor);
                    continue 'plan_batch;
                }
            }
        }
        break batch;
    };

    if rebuilds.is_empty() {
        return stats;
    }

    // Batch conformal boundary postcondition (post-keep overall, or per-operand
    // M_A / M_B on the full arrangement — matching `run_construct_passes`).
    let n_verts = mesh.verts.len() as u32;
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
                let base = n_verts + (ri as u32) * 1_000_000;
                for t in &r.new_tris {
                    let mut tri = t.map(|v| {
                        if v >= r.plan_verts {
                            base + (v - r.plan_verts)
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
                let base = n_verts + (ri as u32) * 1_000_000;
                r.new_tris.iter().map(move |t| {
                    t.map(|v| {
                        if v >= r.plan_verts {
                            base + (v - r.plan_verts)
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
        return stats;
    }

    if apply_rebuild_batch(mesh, attribution, side_labels, &rebuilds).is_err() {
        return stats;
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
    stats
}
