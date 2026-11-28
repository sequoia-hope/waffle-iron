#[allow(clippy::wildcard_imports)]
use super::*;

/// Probe-only (`YANG_S4_BALANCE_PROBE`): census the mesh's unbalanced undirected
/// edges (`fwd != rev`) at a named checkpoint. Reports the count and the sorted
/// edge list, so the SAME census can be compared across checkpoints — the only
/// way to tell a defect a downstream gate MASKED from one an intervening pass
/// MINTED. Read-only; no effect when the env is unset.
pub(crate) fn balance_census(mesh: &Mesh, checkpoint: &str) {
    if std::env::var_os("YANG_S4_BALANCE_PROBE").is_none() {
        return;
    }
    let mut dir: std::collections::BTreeMap<(u32, u32), i32> = std::collections::BTreeMap::new();
    for tri in &mesh.tris {
        for (i, j) in [(0usize, 1usize), (1, 2), (2, 0)] {
            *dir.entry((tri[i], tri[j])).or_insert(0) += 1;
        }
    }
    let mut bad: Vec<String> = Vec::new();
    for (&(s, e), &fwd) in &dir {
        if s >= e {
            continue;
        }
        let rev = dir.get(&(e, s)).copied().unwrap_or(0);
        if fwd != rev {
            bad.push(format!("({s},{e}):{fwd}/{rev}"));
        }
    }
    eprintln!(
        "YANG_S4_BALANCE at={checkpoint} tris={} verts={} unbalanced={} {}",
        mesh.tris.len(),
        mesh.verts.len(),
        bad.len(),
        bad.join(" ")
    );
}

/// Yang Fig. 11(a) — "locate the constrained edge containing q, split it at
/// q" — as the TOPOLOGICAL operation it is: insert `ins` on the edge `{u, v}`
/// of `parent`, returning the two halves WOUND LIKE THE PARENT, i.e. the
/// parent's own cyclic vertex order with `ins` spliced between `u` and `v`
/// in whichever direction that edge runs in the parent. Every directed edge
/// of the parent other than the split one survives verbatim, the split one
/// becomes the two-segment chain in the same direction, and the halves pair
/// each other across `ins–third`; watertightness is preserved by
/// construction, with no geometry consulted.
///
/// P0005 (2026-09-28, spec `yang_n2_stage4_cdt_mesh_updating.md` §5c.15):
/// both §4.4.1(a) arms used to wind the halves GEOMETRICALLY (`orient_tri`
/// against the parent's area normal). A half that is itself a needle — the
/// inserted point within 2.7e-7 of the parent's edge endpoint, which is what a
/// 3e-3-wide × 424-long cone-chart sliver quad (two rim mints 3e-3 apart, each
/// with its seeded generator) delivers to the chain — has an area normal that
/// is numerical noise against the parent's, so the proxy inverted it and the
/// §4.4.3 gate stopped on the fold (`s4-halfedge-pairing` fwd=2 rev=0). The
/// winding of a split half is not a geometric question.
///
/// `None` iff `parent` does not contain the edge `{u, v}` — the callers choose
/// the parent from the edge's own incidence list, so this is unreachable by
/// construction and they treat it as an invariant violation.
pub(crate) fn split_tri_at_edge(
    parent: [u32; 3],
    u: u32,
    v: u32,
    ins: u32,
) -> Option<([u32; 3], [u32; 3])> {
    (0..3).find_map(|i| {
        let (p, q, r) = (parent[i], parent[(i + 1) % 3], parent[(i + 2) % 3]);
        ((p == u && q == v) || (p == v && q == u)).then_some(([p, ins, r], [ins, q, r]))
    })
}

/// Remap a vertex-index set through a compaction table (`old -> Option<new>`).
pub(crate) fn remap_vert_set(set: &mut std::collections::BTreeSet<u32>, remap: &[Option<u32>]) {
    let next: std::collections::BTreeSet<u32> = set
        .iter()
        .filter_map(|&v| remap.get(v as usize).copied().flatten())
        .collect();
    *set = next;
}

/// PR-YR10 helper: edge-collapse `victim` onto `survivor` in `mesh` + the
/// parallel `attribution`. Replaces every `victim` index with `survivor`, then
/// drops the now-degenerate triangles (two equal indices) from BOTH the mesh
/// and the attribution in lockstep. A proper edge-collapse preserves the
/// watertight half-edge pairing (the two collapsed slivers' surviving directed
/// edges are mutual opposites that cancel — spec §4.5.3 / boolean() sliver rule
/// at the compaction step). The cancellation also covers the COINCIDENT-PAIR
/// form (spec `yang_collapse_membrane_cancellation`): an exact duplicate
/// triangle pair with opposite windings — the pleat spanning the twin gap —
/// is a zero-volume flap whose directed edges pair with each other; both
/// copies are dropped. Returns the number of triangles dropped.
#[allow(dead_code)]
pub(crate) fn collapse_vertex(
    mesh: &mut Mesh,
    attribution: &mut Vec<Option<TriangleAttribution>>,
    side_labels: &mut Vec<TriangleSideLabel>,
    victim: u32,
    survivor: u32,
) -> usize {
    collapse_vertex_scoped(
        mesh,
        attribution,
        side_labels,
        victim,
        survivor,
        false,
        None,
    )
}

/// [`collapse_vertex`] with optional per-operand membrane scoping
/// (`scope_by_side_label`) and `tangential_verts` victim→survivor propagation.
#[cfg_attr(not(test), allow(dead_code))]
pub(crate) fn collapse_vertex_scoped(
    mesh: &mut Mesh,
    attribution: &mut Vec<Option<TriangleAttribution>>,
    side_labels: &mut Vec<TriangleSideLabel>,
    victim: u32,
    survivor: u32,
    scope_by_side_label: bool,
    tangential_verts: Option<&mut std::collections::BTreeSet<u32>>,
) -> usize {
    let mut dummy_bounds = vec![f64::NAN; mesh.tris.len()];
    collapse_vertex_scoped_with_bounds(
        mesh,
        attribution,
        side_labels,
        &mut dummy_bounds,
        victim,
        survivor,
        scope_by_side_label,
        tangential_verts,
    )
}

/// Every `victim -> survivor` of `map` applied at once (spec
/// `yang_f2_fig11b_cdt_input_merge.md` §8.6, increment 2a).
///
/// The points of a class are one point (§4.3.3 `:535-536`, Fig. 11(b)
/// `:562-563`, `:563-564`, `r_A = r_B = r` `:550-554`), so the merge has no
/// order: all indices are rewritten together, the triangles that lose an
/// edge are dropped, and then — once, over the result — every pair of
/// triangles on one vertex triple with opposite windings is cancelled (the
/// zero-volume membrane of [`collapse_vertex_scoped_with_bounds`], scoped the
/// same way). One pair at a time, an earlier merge's cancellation could
/// delete the edge a later pair was read from, and the result depended on
/// the order of the pairs (F0060, ledger N55 amendment 2026-11-25).
///
/// Returns the number of triangles removed.
#[allow(clippy::too_many_arguments)]
pub(crate) fn merge_vertex_classes_with_bounds(
    mesh: &mut Mesh,
    attribution: &mut Vec<Option<TriangleAttribution>>,
    side_labels: &mut Vec<TriangleSideLabel>,
    tri_bounds: &mut Vec<f64>,
    map: &std::collections::BTreeMap<u32, u32>,
    scope_by_side_label: bool,
    tangential_verts: Option<&mut std::collections::BTreeSet<u32>>,
) -> usize {
    assert_eq!(
        tri_bounds.len(),
        mesh.tris.len(),
        "merge_vertex_classes: tri_bounds must be 1:1 with mesh.tris"
    );
    if map.is_empty() {
        return 0;
    }
    if let Some(tv) = tangential_verts {
        for (victim, survivor) in map {
            if tv.remove(victim) {
                tv.insert(*survivor);
            }
        }
    }
    let m = |v: u32| map.get(&v).copied().unwrap_or(v);
    let mut new_tris: Vec<[u32; 3]> = Vec::with_capacity(mesh.tris.len());
    let mut kept: Vec<usize> = Vec::with_capacity(mesh.tris.len());
    for (t, tri) in mesh.tris.iter().enumerate() {
        let mapped = tri.map(m);
        if mapped[0] == mapped[1] || mapped[1] == mapped[2] || mapped[2] == mapped[0] {
            continue;
        }
        new_tris.push(mapped);
        kept.push(t);
    }
    type ScopeKey = Option<TriangleSideLabel>;
    let label_of = |t: usize| side_labels.get(t).copied().unwrap_or_default();
    let mut by_triple: std::collections::BTreeMap<[u32; 3], Vec<usize>> =
        std::collections::BTreeMap::new();
    for (i, tri) in new_tris.iter().enumerate() {
        let mut s = *tri;
        s.sort_unstable();
        by_triple.entry(s).or_default().push(i);
    }
    // Cyclic-winding key: the smallest index first; equal keys ⇔ same winding.
    let winding_key = |tri: [u32; 3]| -> [u32; 3] {
        let k = (0..3).min_by_key(|&i| tri[i]).expect("3 verts");
        [tri[k], tri[(k + 1) % 3], tri[(k + 2) % 3]]
    };
    let mut cancel: std::collections::BTreeSet<usize> = std::collections::BTreeSet::new();
    for group in by_triple.values() {
        // Within one scope, exactly two triangles on the triple with
        // opposite windings are a membrane; any other count is left for the
        // gates downstream to read.
        let mut by_scope: Vec<(ScopeKey, Vec<usize>)> = Vec::new();
        for &i in group {
            let scope: ScopeKey = scope_by_side_label.then(|| label_of(kept[i]));
            match by_scope.iter_mut().find(|(s, _)| *s == scope) {
                Some((_, list)) => list.push(i),
                None => by_scope.push((scope, vec![i])),
            }
        }
        for (_, list) in by_scope {
            if let [x, y] = list.as_slice() {
                if winding_key(new_tris[*x]) != winding_key(new_tris[*y]) {
                    cancel.insert(*x);
                    cancel.insert(*y);
                }
            }
        }
    }
    let before = mesh.tris.len();
    let keep: Vec<usize> = (0..new_tris.len())
        .filter(|i| !cancel.contains(i))
        .collect();
    let tris: Vec<[u32; 3]> = keep.iter().map(|&i| new_tris[i]).collect();
    let attr: Vec<Option<TriangleAttribution>> = keep
        .iter()
        .map(|&i| attribution.get(kept[i]).copied().flatten())
        .collect();
    let lbls: Vec<TriangleSideLabel> = keep.iter().map(|&i| label_of(kept[i])).collect();
    let bounds: Vec<f64> = keep.iter().map(|&i| tri_bounds[kept[i]]).collect();
    *mesh = Mesh::new(std::mem::take(&mut mesh.verts), tris);
    *attribution = attr;
    *side_labels = lbls;
    *tri_bounds = bounds;
    before - mesh.tris.len()
}

#[allow(clippy::too_many_arguments)]
pub(crate) fn collapse_vertex_scoped_with_bounds(
    mesh: &mut Mesh,
    attribution: &mut Vec<Option<TriangleAttribution>>,
    side_labels: &mut Vec<TriangleSideLabel>,
    tri_bounds: &mut Vec<f64>,
    victim: u32,
    survivor: u32,
    scope_by_side_label: bool,
    tangential_verts: Option<&mut std::collections::BTreeSet<u32>>,
) -> usize {
    debug_assert_eq!(
        side_labels.len(),
        mesh.tris.len(),
        "collapse_vertex: side_labels ({}) must be 1:1 with mesh.tris ({})",
        side_labels.len(),
        mesh.tris.len(),
    );
    assert_eq!(
        tri_bounds.len(),
        mesh.tris.len(),
        "collapse_vertex: tri_bounds ({}) must be 1:1 with mesh.tris ({})",
        tri_bounds.len(),
        mesh.tris.len(),
    );
    if let Some(tv) = tangential_verts {
        if tv.remove(&victim) {
            tv.insert(survivor);
        }
    }
    let mut new_tris: Vec<[u32; 3]> = Vec::with_capacity(mesh.tris.len());
    let mut new_attr: Vec<Option<TriangleAttribution>> = Vec::with_capacity(attribution.len());
    let mut new_lbls: Vec<TriangleSideLabel> = Vec::with_capacity(side_labels.len());
    let mut new_bounds: Vec<f64> = Vec::with_capacity(tri_bounds.len());
    let mut dropped = 0usize;
    for (t, tri) in mesh.tris.iter().enumerate() {
        let mapped = [
            if tri[0] == victim { survivor } else { tri[0] },
            if tri[1] == victim { survivor } else { tri[1] },
            if tri[2] == victim { survivor } else { tri[2] },
        ];
        if mapped[0] == mapped[1] || mapped[1] == mapped[2] || mapped[2] == mapped[0] {
            dropped += 1;
            continue;
        }
        new_tris.push(mapped);
        new_attr.push(attribution.get(t).copied().flatten());
        new_lbls.push(side_labels.get(t).copied().unwrap_or_default());
        new_bounds.push(tri_bounds[t]);
    }
    // Membrane cancellation (spec `yang_collapse_membrane_cancellation`):
    // identifying `victim` with `survivor` can turn the two-triangle pleat
    // that spanned the twin gap into an EXACT duplicate pair with OPPOSITE
    // windings — a zero-volume doubled flap whose 6 directed edges are 3
    // mutual-reverse pairs (they pair with EACH OTHER). Dropping BOTH
    // preserves the watertight half-edge pairing and restores manifold
    // count-2 on the shared fan edges. On the full arrangement
    // (`scope_by_side_label = true`, Yang §4.4.2 `yang2025_hybrid_boolean.txt:572-578`),
    // cancellation is scoped to the exact `TriangleSideLabel` (`cherchi_patch`,
    // `inside`, `surface`, `sheet_opposite`) so opposite-winding pleats only
    // cancel within the SAME patch — never a kept patch (`A-out`) against a
    // dropped patch (`A-in`), nor across operands.
    {
        type ScopeKey = Option<TriangleSideLabel>;
        let mut by_triple: std::collections::HashMap<([u32; 3], ScopeKey), Vec<usize>> =
            std::collections::HashMap::new();
        for (t, tri) in new_tris.iter().enumerate() {
            let mut s = *tri;
            s.sort_unstable();
            let scope: ScopeKey = scope_by_side_label.then_some(new_lbls[t]);
            by_triple.entry((s, scope)).or_default().push(t);
        }
        // Cyclic-winding key: rotate the smallest index to the front; equal
        // keys ⇔ same winding.
        let winding_key = |tri: [u32; 3]| -> [u32; 3] {
            let k = (0..3).min_by_key(|&i| tri[i]).expect("3 verts");
            [tri[k], tri[(k + 1) % 3], tri[(k + 2) % 3]]
        };
        let mut cancel: std::collections::BTreeSet<usize> = std::collections::BTreeSet::new();
        for ts in by_triple.values() {
            if ts.len() != 2 {
                continue;
            }
            let (x, y) = (ts[0], ts[1]);
            if winding_key(new_tris[x]) != winding_key(new_tris[y]) {
                cancel.insert(x);
                cancel.insert(y);
                if std::env::var_os("YANG_DOUBLECOVER_PROBE").is_some() {
                    eprintln!(
                        "[membrane-cancel] dropping opposite-winding dup pair tris {x},{y} = \
                         {:?}/{:?} (victim={victim} survivor={survivor})",
                        new_tris[x], new_tris[y]
                    );
                }
            }
        }
        if !cancel.is_empty() {
            let keep: Vec<usize> = (0..new_tris.len())
                .filter(|t| !cancel.contains(t))
                .collect();
            new_tris = keep.iter().map(|&t| new_tris[t]).collect();
            new_attr = keep.iter().map(|&t| new_attr[t]).collect();
            new_lbls = keep.iter().map(|&t| new_lbls[t]).collect();
            new_bounds = keep.iter().map(|&t| new_bounds[t]).collect();
            dropped += cancel.len();
        }
    }
    *mesh = Mesh::new(std::mem::take(&mut mesh.verts), new_tris);
    *attribution = new_attr;
    *side_labels = new_lbls;
    *tri_bounds = new_bounds;
    assert_eq!(tri_bounds.len(), mesh.tris.len());
    // EXPERIMENTAL probe (task #121, read-only, env-gated): did THIS collapse
    // mint a duplicate (double-cover) triangle pair?
    if std::env::var_os("YANG_DOUBLECOVER_PROBE").is_some() {
        use std::collections::HashMap;
        let mut by_triple: HashMap<[u32; 3], Vec<usize>> = HashMap::new();
        for (t, tri) in mesh.tris.iter().enumerate() {
            let mut s = *tri;
            s.sort_unstable();
            by_triple.entry(s).or_default().push(t);
        }
        for (key, ts) in &by_triple {
            if ts.len() > 1 {
                eprintln!(
                    "[doublecover-collapse] victim={victim} survivor={survivor} \
                     dup triple {key:?} tris {ts:?} windings {:?}",
                    ts.iter().map(|&t| mesh.tris[t]).collect::<Vec<_>>()
                );
            }
        }
    }
    dropped
}

// N47 (spec `yang_n47_coincident_moved_weld`): weld coincident RELOCATED
// vertices before topology emission.
//
// Two vertices this pipeline pushed onto an analytic curve (`moved`) can
// Newton-converge to within the MODEL coincidence tolerance
// `TAU_MODEL·(1+scale)` — they are the SAME geometric point emitted twice (a
// near-tangent seam crossing whose two arrangement vertices both project onto
// one intersection point). Emitted distinct, they become a sub-render-precision
// output edge that trips kernel-v2's G1 render-collapse gate far downstream.
//
// The four non-compliant vertex welds are RETIRED (#169 weld-retirement track,
// audit 2026-07-16): they are OFF in production. Each was a tolerance hack
// (violating the Cherchi B6 "never a tolerance weld" invariant) that masked
// upstream near-coincident minting; a case that only stayed CORRECT via such a
// weld was a false green by project intent (Yang-paper compliance is the north
// star), and retiring it can only expose a loud STOP, never a silent-wrong. The
// measured cost of turning all four off was 13 cases (241C → 228C, 0 WRONG).
//
// **Update (N55/N56): the audit was wrong for THREE of the four.** The correct
// test is "is it a Yang paper operation?", not "does it use a tolerance." The
// paper prescribes tolerance-gated merges, and those are desired:
// - `subfeature` = Yang §4.4.1(b) (Fig-11(b) "merge p with q if too close"),
//   retightened to `TAU_WORK·(1+scale)` (`is_relocation_coincidence`) →
//   compliant always-on merge (N55). Recovers R0055/F0056/F0057/F0059.
// - `coincident` = Yang §4.3 ("remove a point too close to another on the same
//   loop"; both verts relocated onto the curve) → reinstated always-on (N56).
//   0-conversion (near-tangency infra for #137) but genuine paper machinery.
// - `subres` = Yang §4.3 (sub-resolution intersection-curve segment collapse),
//   retightened from the absolute floor to `TAU_MODEL·(1+scale)` → reinstated
//   always-on (N56). Recovers R0076/R0088/F0078/F0079/F0084 — and, combined
//   with `coincident`, the render twins R0012/R0098/F0090.
//
// Net: 12 of the 13 retired cases recover COMPLIANTLY (228C → 240C, 0 WRONG);
// only R0072 stays a loud STOP (a real ~1e-7 micro-scale collapse → curved
// re-CDT). The last gated arm — `f32`, the sole confirmed hack (it keyed on
// f32 RENDER precision, not geometry; it is nowhere in the paper; it
// REGRESSES C0036; it was redundant since the §4.3 dedup recovers its
// cases) — was kept callable behind `YANG_WELD_ENABLE` as a historical A/B
// artifact until the §4.4.1 epic's **I4-1 (2026-08-15) removed the arm and
// the `weld_enabled` gate entirely** (production had it off by default
// since the audit, so the removal is byte-identical by construction).

/// Yang §4.4.1(b) same-point test (deviation N55): two relocated endpoints
/// `len` apart at local magnitude `scale` (= max |coord| of the pair) are the
/// SAME intersection point — a numerical coincidence eligible for the Fig-11(b)
/// merge — iff their separation is below the scale-relative WORKING tolerance
/// `TAU_WORK·(1+scale)`.
///
/// This is the COMPLIANT criterion that replaced the retired `subfeature` weld's
/// absolute `MIN_FEATURE_SIZE` floor. The distinction is load-bearing: the
/// absolute floor merged BOTH machine-ε relocation twins (exact duplicates —
/// which the compliance ratchet keeps) AND genuine sub-feature edges at
/// micro-scale (R0072's ~1e-7 collapse = 0.4 % of a ~2e-4 span — the R0091
/// silent-wrong hazard). The `TAU_WORK` band (5 orders tighter than
/// `MIN_FEATURE_SIZE`) admits only the former: a numerically-identical pair
/// merges, a real sub-feature edge stays a loud STOP (→ curved re-CDT).
pub(crate) fn is_relocation_coincidence(len: f64, scale: f64) -> bool {
    len < cad_primitives::TAU_WORK * (1.0 + scale)
}

/// Band: the scale-relative model coincidence tolerance (`scale` = max |coord| of
/// the pair) — the SAME band every other coincidence test uses, 10× tighter than
/// the `MIN_FEATURE_SIZE·(1+scale)` feature floor, so it admits ONLY
/// sub-(feature/10) coincidences. Restricted to `moved`×`moved` and
/// `moved`×`minted` pairs: it never touches un-relocated arrangement geometry
/// `boolean()` kept for watertightness (cf. the §4.4.1(b) micro-scale R0091
/// revert — P9/P10). `collapse_vertex` is the proven watertight-preserving
/// edge-collapse; iterate to a fixed point over live (still-referenced)
/// vertices. Returns whether any pair welded.
///
/// P3b inc-4a (R0061): `minted` = Stage-1 minted junction vertices. A Stage-4
/// relocation arm can converge a chord-crossing vertex onto the SAME geometric
/// junction a Stage-1 mint carries (R0061: the `ell_junction` plane-pair ×
/// cylinder junction IS the minted line×cylinder pierce corner, two exact-intent
/// computations landing ~1e-15 apart), and the mint is unmoved so a moved×moved
/// restriction misses the pair. Eligibility: at least one member `moved` (a
/// minted×minted sub-band pair is a mint-multiplicity contract violation and
/// must stay LOUD). Survivor: the minted vertex ALWAYS — its bits are the
/// shared cross-operand junction identity; the mint never moves (N54).
///
/// Cleanup Phase 2a (§4-I8 CONTAINMENT, the KV15b I1c rule): the band alone
/// never identifies two vertices. Yang §4.3 removes a point "too close to
/// another point on the same loop" (`:535-536`) — a redundant sample of ONE
/// curve — so the weld requires the victim's carried surfaces
/// ([`carried_surfaces`]) to be contained in the survivor's. When only the
/// reverse holds the richer vertex survives (a mint is never the victim, so a
/// mint poorer than its partner declines); when neither contains the other
/// the pair is two DISTINCT exact points closer than the band, and both stay.
#[allow(dead_code)]
pub(crate) fn weld_coincident_relocated(
    mesh: &mut Mesh,
    attribution: &mut Vec<Option<TriangleAttribution>>,
    side_labels: &mut Vec<TriangleSideLabel>,
    a: &BRep,
    b: &BRep,
    moved: &std::collections::HashSet<u32>,
    minted: &std::collections::HashSet<u32>,
) -> bool {
    weld_coincident_relocated_ex(
        mesh,
        attribution,
        side_labels,
        a,
        b,
        moved,
        minted,
        false,
        None,
    )
}

#[allow(clippy::too_many_arguments)]
#[cfg_attr(not(test), allow(dead_code))]
pub(crate) fn weld_coincident_relocated_ex(
    mesh: &mut Mesh,
    attribution: &mut Vec<Option<TriangleAttribution>>,
    side_labels: &mut Vec<TriangleSideLabel>,
    a: &BRep,
    b: &BRep,
    moved: &std::collections::HashSet<u32>,
    minted: &std::collections::HashSet<u32>,
    scope_by_side_label: bool,
    tangential_verts: Option<&mut std::collections::BTreeSet<u32>>,
) -> bool {
    let mut dummy_bounds = vec![f64::NAN; mesh.tris.len()];
    weld_coincident_relocated_ex_with_bounds(
        mesh,
        attribution,
        side_labels,
        &mut dummy_bounds,
        a,
        b,
        moved,
        minted,
        scope_by_side_label,
        tangential_verts,
    )
}

#[allow(clippy::too_many_arguments)]
pub(crate) fn weld_coincident_relocated_ex_with_bounds(
    mesh: &mut Mesh,
    attribution: &mut Vec<Option<TriangleAttribution>>,
    side_labels: &mut Vec<TriangleSideLabel>,
    tri_bounds: &mut Vec<f64>,
    a: &BRep,
    b: &BRep,
    moved: &std::collections::HashSet<u32>,
    minted: &std::collections::HashSet<u32>,
    scope_by_side_label: bool,
    mut tangential_verts: Option<&mut std::collections::BTreeSet<u32>>,
) -> bool {
    assert_eq!(
        tri_bounds.len(),
        mesh.tris.len(),
        "weld_coincident_relocated: tri_bounds ({}) must be 1:1 with mesh.tris ({})",
        tri_bounds.len(),
        mesh.tris.len(),
    );
    let mut welded = false;
    loop {
        // Live moved/minted verts (still referenced by some triangle), ascending.
        let mut live: std::collections::BTreeSet<u32> = std::collections::BTreeSet::new();
        for tri in &mesh.tris {
            for &v in tri {
                if moved.contains(&v) || minted.contains(&v) {
                    live.insert(v);
                }
            }
        }
        let live: Vec<u32> = live.into_iter().collect();
        let mut pair: Option<(u32, u32)> = None;
        'scan: for (i, &u) in live.iter().enumerate() {
            let pu = mesh.verts[u as usize].as_array();
            for &w in &live[i + 1..] {
                // A vert in BOTH sets counts as minted (identity outranks
                // relocation). Pairs need ≥1 moved member; minted×minted is
                // ineligible (multiplicity stays loud).
                let (u_minted, w_minted) = (minted.contains(&u), minted.contains(&w));
                let (u_moved, w_moved) = (
                    !u_minted && moved.contains(&u),
                    !w_minted && moved.contains(&w),
                );
                if !(u_moved || w_moved) {
                    continue;
                }
                let pw = mesh.verts[w as usize].as_array();
                let d =
                    ((pu[0] - pw[0]).powi(2) + (pu[1] - pw[1]).powi(2) + (pu[2] - pw[2]).powi(2))
                        .sqrt();
                let scale = pu[0]
                    .abs()
                    .max(pu[1].abs())
                    .max(pu[2].abs())
                    .max(pw[0].abs())
                    .max(pw[1].abs())
                    .max(pw[2].abs());
                let band = cad_primitives::TAU_MODEL * (1.0 + scale);
                if d < band {
                    // Survivor: the minted member if any (the mint's bits are
                    // the cross-operand junction identity); else lower index
                    // (matches every other collapse's survivor rule — both are
                    // already exact on their curve).
                    let (victim, survivor) = if u_minted {
                        (w, u) // (victim, survivor = mint)
                    } else if w_minted {
                        (u, w)
                    } else {
                        (w, u) // (victim = higher, survivor = lower)
                    };
                    // §4-I8 containment (doc above).
                    let sv = carried_surfaces(
                        mesh,
                        attribution,
                        a,
                        b,
                        victim,
                        mesh.verts[victim as usize].as_array(),
                    );
                    let ss = carried_surfaces(
                        mesh,
                        attribution,
                        a,
                        b,
                        survivor,
                        mesh.verts[survivor as usize].as_array(),
                    );
                    let victim_in_survivor = sv.iter().all(|x| ss.contains(x));
                    let survivor_in_victim = ss.iter().all(|x| sv.contains(x));
                    let chosen = if victim_in_survivor {
                        Some((victim, survivor))
                    } else if survivor_in_victim && !minted.contains(&survivor) {
                        Some((survivor, victim))
                    } else {
                        None
                    };
                    if std::env::var_os("YANG_MOVED_WELD_PROBE").is_some() && chosen.is_none() {
                        eprintln!(
                            "[moved-weld] DECLINE {u}/{w} d={d:.3e}: carried surfaces \
                             {} vs {}, neither contains the other",
                            sv.len(),
                            ss.len()
                        );
                    }
                    if let Some(vs) = chosen {
                        pair = Some(vs);
                        break 'scan;
                    }
                }
            }
        }
        match pair {
            Some((victim, survivor)) => {
                if std::env::var_os("YANG_MOVED_WELD_PROBE").is_some() {
                    eprintln!(
                        "[moved-weld] victim={victim} survivor={survivor} p={:?}",
                        mesh.verts[survivor as usize]
                    );
                }
                // `YANG_FIG11B_CENSUS` (print-only): the pair, its distance, whether
                // it is one point within roundoff and whether a mesh edge joins it.
                if crate::stage4_fig11b::census_on() {
                    let (pv, ps) = (
                        mesh.verts[victim as usize].as_array(),
                        mesh.verts[survivor as usize].as_array(),
                    );
                    let d = ((pv[0] - ps[0]).powi(2)
                        + (pv[1] - ps[1]).powi(2)
                        + (pv[2] - ps[2]).powi(2))
                    .sqrt();
                    let scale = pv
                        .iter()
                        .chain(ps.iter())
                        .fold(0.0f64, |m, &c| m.max(c.abs()));
                    crate::stage4_fig11b::log(&format!(
                        "weld-pair\tvictim={victim}:{}\tsurvivor={survivor}:{}\td={d:.3e}\t\
                         same_point={}\tjoined={}\tmint_survivor={}",
                        crate::stage4_fig11b::pos_key(mesh, victim),
                        crate::stage4_fig11b::pos_key(mesh, survivor),
                        crate::stage4_correct::is_relocation_coincidence(d, scale),
                        mesh.tris
                            .iter()
                            .any(|t| t.contains(&victim) && t.contains(&survivor)),
                        minted.contains(&survivor),
                    ));
                }
                collapse_vertex_scoped_with_bounds(
                    mesh,
                    attribution,
                    side_labels,
                    tri_bounds,
                    victim,
                    survivor,
                    scope_by_side_label,
                    tangential_verts.as_deref_mut(),
                );
                welded = true;
            }
            None => break,
        }
    }
    assert_eq!(tri_bounds.len(), mesh.tris.len());
    welded
}

#[allow(dead_code)]
pub(crate) fn trim_beyond_corner_phantoms(
    mesh: &mut Mesh,
    attribution: &mut Vec<Option<TriangleAttribution>>,
    side_labels: &mut Vec<TriangleSideLabel>,
    moved: &std::collections::HashSet<u32>,
    minted: &std::collections::BTreeMap<u32, crate::boolean::MintProvenance>,
    d_eps: f64,
) -> bool {
    trim_beyond_corner_phantoms_ex(
        mesh,
        attribution,
        side_labels,
        moved,
        minted,
        d_eps,
        false,
        None,
    )
}

#[allow(clippy::too_many_arguments)]
#[cfg_attr(not(test), allow(dead_code))]
pub(crate) fn trim_beyond_corner_phantoms_ex(
    mesh: &mut Mesh,
    attribution: &mut Vec<Option<TriangleAttribution>>,
    side_labels: &mut Vec<TriangleSideLabel>,
    moved: &std::collections::HashSet<u32>,
    minted: &std::collections::BTreeMap<u32, crate::boolean::MintProvenance>,
    d_eps: f64,
    scope_by_side_label: bool,
    tangential_verts: Option<&mut std::collections::BTreeSet<u32>>,
) -> bool {
    let mut dummy_bounds = vec![f64::NAN; mesh.tris.len()];
    trim_beyond_corner_phantoms_ex_with_bounds(
        mesh,
        attribution,
        side_labels,
        &mut dummy_bounds,
        moved,
        minted,
        d_eps,
        scope_by_side_label,
        tangential_verts,
    )
}

#[allow(clippy::too_many_arguments)]
pub(crate) fn trim_beyond_corner_phantoms_ex_with_bounds(
    mesh: &mut Mesh,
    attribution: &mut Vec<Option<TriangleAttribution>>,
    side_labels: &mut Vec<TriangleSideLabel>,
    tri_bounds: &mut Vec<f64>,
    moved: &std::collections::HashSet<u32>,
    minted: &std::collections::BTreeMap<u32, crate::boolean::MintProvenance>,
    d_eps: f64,
    scope_by_side_label: bool,
    mut tangential_verts: Option<&mut std::collections::BTreeSet<u32>>,
) -> bool {
    assert_eq!(
        tri_bounds.len(),
        mesh.tris.len(),
        "trim_beyond_corner_phantoms: tri_bounds ({}) must be 1:1 with mesh.tris ({})",
        tri_bounds.len(),
        mesh.tris.len(),
    );
    let probe = std::env::var_os("YANG_P3B_TRIM_PROBE").is_some();
    let mut trimmed = false;
    'fixed_point: loop {
        let mut patches: std::collections::BTreeMap<
            u32,
            std::collections::BTreeSet<(InputId, u32)>,
        > = std::collections::BTreeMap::new();
        for (ti, tri) in mesh.tris.iter().enumerate() {
            if let Some(Some(att)) = attribution.get(ti) {
                for &tv in tri {
                    if moved.contains(&tv) || minted.contains_key(&tv) {
                        patches.entry(tv).or_default().insert((att.input, att.face));
                    }
                }
            }
        }
        let empty: std::collections::BTreeSet<(InputId, u32)> = std::collections::BTreeSet::new();
        let mut seen: std::collections::BTreeSet<(u32, u32)> = std::collections::BTreeSet::new();
        for tri in &mesh.tris {
            for (i, j) in [(0usize, 1usize), (1, 2), (2, 0)] {
                let (u, w) = (tri[i], tri[j]);
                if u == w || !seen.insert((u.min(w), u.max(w))) {
                    continue;
                }
                for (m, v) in [(u, w), (w, u)] {
                    let Some(prov) = minted.get(&m) else {
                        continue;
                    };
                    if minted.contains_key(&v) || !moved.contains(&v) {
                        continue;
                    }
                    let pv_patches = patches.get(&v).unwrap_or(&empty);
                    let pm_patches = patches.get(&m).unwrap_or(&empty);
                    if !pv_patches.is_subset(pm_patches) {
                        if probe {
                            eprintln!(
                                "[p3b-trim] patch-guard NO-FIRE mint v{m} ~ v{v}: \
                                 v patches {pv_patches:?} ⊄ m patches {pm_patches:?}"
                            );
                        }
                        continue;
                    }
                    let pv = mesh.verts[v as usize].as_array();
                    let pm = mesh.verts[m as usize].as_array();
                    let dist = ((pv[0] - pm[0]).powi(2)
                        + (pv[1] - pm[1]).powi(2)
                        + (pv[2] - pm[2]).powi(2))
                    .sqrt();
                    if dist <= 0.0 {
                        continue; // coincidence is the weld's territory
                    }
                    let scale = pv
                        .iter()
                        .chain(pm.iter())
                        .fold(0.0f64, |acc, &c| acc.max(c.abs()));
                    let beyond_band = cad_primitives::TAU_MODEL * (1.0 + scale);
                    let on_band = cad_primitives::TAU_EVAL * (1.0 + scale);
                    for k in 0..2 {
                        let pi = prov.owner_planes[k];
                        let pj = prov.owner_planes[1 - k];
                        if !pi.trim_beyond {
                            continue; // reflex/ambiguous/default — fail closed
                        }
                        let d_i = pi.n[0] * pv[0] + pi.n[1] * pv[1] + pi.n[2] * pv[2] + pi.d;
                        let d_j = pj.n[0] * pv[0] + pj.n[1] * pv[1] + pj.n[2] * pv[2] + pj.d;
                        if d_i <= beyond_band || d_j.abs() > on_band {
                            continue;
                        }
                        let sin_theta = d_i / dist;
                        if dist > tangent_plane_corridor(d_eps, sin_theta) {
                            if probe {
                                eprintln!(
                                    "[p3b-trim] over-corridor NO-FIRE mint v{m} ~ v{v} \
                                     dist={dist:.3e} d_i={d_i:.3e} d_eps={d_eps:.3e}"
                                );
                            }
                            continue;
                        }
                        if probe {
                            eprintln!(
                                "[p3b-trim] TRIM v{v} -> mint v{m} dist={dist:.3e} \
                                 d_i={d_i:.3e} d_j={:.3e} sin={sin_theta:.3}",
                                d_j.abs()
                            );
                        }
                        collapse_vertex_scoped_with_bounds(
                            mesh,
                            attribution,
                            side_labels,
                            tri_bounds,
                            v,
                            m,
                            scope_by_side_label,
                            tangential_verts.as_deref_mut(),
                        );
                        trimmed = true;
                        continue 'fixed_point;
                    }
                }
            }
        }
        break;
    }
    assert_eq!(tri_bounds.len(), mesh.tris.len());
    trimmed
}

/// Yang §4.3.4 curve-refinement acceptance test
/// (`refs/text/yang2025_hybrid_boolean.txt:586-592`): for consecutive curve
/// points p, q with an intermediate point m, no further subdivision is
/// needed — i.e. m is REDUNDANT and the chord p→q suffices — iff
///
///   h < d_p·10²,  l < d_p·10³,  α < π/18
///
/// with h = the arc height (distance from m to segment pq), l = the chord
/// length max(|pm|, |mq|), and α = the turning angle between p→m and m→q.
/// The paper pins d_p = 1e-7 (`:744-745`), which is exactly this port's
/// `TAU_MODEL`; scale-relative as everywhere else: d_p = TAU_MODEL·(1+scale).
/// Used by the §4.4.1 chain decimation (`stage4_chain_decimate`, deviation
/// N58) and the §4.3.3 small loops. The measurements themselves are factored into
/// [`paper_chain_metrics`], shared with the I5-0 seam-density census
/// (spec `yang_441_trim_cdt_construction.md` §4-I5).
pub(crate) fn paper_chain_sample_redundant(a: [f64; 3], m: [f64; 3], b: [f64; 3]) -> bool {
    let mt = paper_chain_metrics(a, m, b);
    if mt.l >= mt.dp * 1e3 {
        return false;
    }
    if mt.h >= mt.dp * 1e2 {
        return false;
    }
    if mt.degenerate {
        return true; // coincident with a neighbour: trivially redundant
    }
    mt.alpha < std::f64::consts::PI / 18.0
}

/// I5-0: the §4.3.4 measurements of one (a, m, b) sample triple — h (arc
/// height of m over chord ab), l = max(|am|, |mb|), α (turning angle a→m→b,
/// 0 when a leg is degenerate), and the triple's own scale-relative
/// d_p = TAU_MODEL·(1+scale). Pure measurement; the acceptance thresholds
/// (h < d_p·10², l < d_p·10³, α < π/18) live in the consumers so the
/// predicate stays byte-identical to its pre-factoring form.
#[derive(Debug, Clone, Copy)]
pub(crate) struct ChainSampleMetrics {
    pub h: f64,
    pub l: f64,
    pub alpha: f64,
    pub degenerate: bool,
    pub dp: f64,
}

/// See [`ChainSampleMetrics`]. Extracted verbatim from
/// [`paper_chain_sample_redundant`]; same float operations on the same
/// inputs, so the predicate's decisions are unchanged.
pub(crate) fn paper_chain_metrics(a: [f64; 3], m: [f64; 3], b: [f64; 3]) -> ChainSampleMetrics {
    let scale = a
        .iter()
        .chain(m.iter())
        .chain(b.iter())
        .fold(0.0f64, |acc, &c| acc.max(c.abs()));
    let dp = cad_primitives::TAU_MODEL * (1.0 + scale);
    let am = [m[0] - a[0], m[1] - a[1], m[2] - a[2]];
    let mb = [b[0] - m[0], b[1] - m[1], b[2] - m[2]];
    let ab = [b[0] - a[0], b[1] - a[1], b[2] - a[2]];
    let n2 = |v: [f64; 3]| v[0] * v[0] + v[1] * v[1] + v[2] * v[2];
    let dot = |x: [f64; 3], y: [f64; 3]| x[0] * y[0] + x[1] * y[1] + x[2] * y[2];
    // l = max(|pm|, |mq|).
    let (lam, lmb) = (n2(am).sqrt(), n2(mb).sqrt());
    let l = lam.max(lmb);
    // h = distance from m to segment ab.
    let lab2 = n2(ab);
    let h = if lab2 > 0.0 {
        let t = (dot(am, ab) / lab2).clamp(0.0, 1.0);
        n2([am[0] - t * ab[0], am[1] - t * ab[1], am[2] - t * ab[2]]).sqrt()
    } else {
        lam
    };
    // α = turning angle between a→m and m→b.
    let degenerate = lam <= 0.0 || lmb <= 0.0;
    let alpha = if degenerate {
        0.0
    } else {
        let cos_a = (dot(am, mb) / (lam * lmb)).clamp(-1.0, 1.0);
        cos_a.acos()
    };
    ChainSampleMetrics {
        h,
        l,
        alpha,
        degenerate,
        dp,
    }
}

/// The DISTINCT analytic surfaces `pos` lies on, over the faces carried by the
/// triangles incident to `vi`.
///
/// Incidence is certified at [`junction_certificate_band`], the same band that
/// certifies a Stage-4 exact junction, so a chord-level (un-relocated) curved
/// sample contributes nothing. Every surface counts, not planes alone (the
/// 2026-08-19 R0047 amendment).
///
/// The set, not just its size, is what decides whether two positions may be
/// IDENTIFIED: a merge is authority-preserving exactly when the victim's set is
/// contained in the survivor's (see §4-I8 in
/// `specs/yang_441_trim_cdt_construction.md`). Two sets of equal size that
/// differ are two DISTINCT model points, which no merge can join.
pub(crate) fn carried_surfaces(
    mesh: &Mesh,
    attribution: &[Option<TriangleAttribution>],
    a: &BRep,
    b: &BRep,
    vi: u32,
    pos: [f64; 3],
) -> Vec<Surface> {
    let mut seen: Vec<Surface> = Vec::new();
    for (t, tri) in mesh.tris.iter().enumerate() {
        if !tri.contains(&vi) {
            continue;
        }
        let Some(att) = attribution.get(t).copied().flatten() else {
            continue;
        };
        let faces = match att.input {
            InputId::A => a.faces(),
            InputId::B => b.faces(),
        };
        let Some(face) = faces.get(att.face as usize) else {
            continue;
        };
        let surf = face.surface;
        let on = surface_distance_and_normal(surf, pos)
            .is_some_and(|(f, _)| f.abs() <= junction_certificate_band(pos, surf));
        if !on {
            continue;
        }
        if !seen.contains(&surf) {
            seen.push(surf);
        }
    }
    seen
}

/// KV15b (spec `kv15b_mint_site_subresolution_collapse`): collapse
/// sub-resolution intersection segments before Phase-B emission.
///
/// The exact arrangement legitimately mints two crossings of near-parallel
/// geometry closer than the model tolerance (R0076: gear flank grazing a box
/// edge, 3.999e-8 / 6.472e-8 pairs). Emitted as two distinct output vertices,
/// the pair is POISON downstream: the Stage-0 coplanar clustering band floor
/// is exactly `TAU_MODEL`, and Stage-6 patch walks of the next boolean
/// disagree over the twin (the measured F0070/KV15 mechanism at sub-floor
/// scale). Per A8.1/A14 `TAU_MODEL` is the single central vertex-merge
/// resolution — two points closer than it ARE one model point — so emission
/// hygiene collapses the segment at the mint site.
///
/// Eligibility is FULL-PROVENANCE (I3): only consecutive intersection-curve
/// vertices — keys of `intersection_curves` — are candidates; inherited
/// operand geometry (e.g. legitimately sub-floor micro-profile corners) is
/// never touched. This is one order TIGHTER than the reverted-R0091
/// `MIN_FEATURE_SIZE` global widening and scoped to the increment-4
/// provenance pattern. One sweep over the ORIGINAL segment set in
/// deterministic `BTreeMap` order; endpoints resolve through prior collapses
/// (min-index survivor, I1 — the survivor keeps its own exact coordinates,
/// never an average), and a segment whose RESOLVED length is ≥ `TAU_MODEL`
/// stays (I2/B5 — no chain drift). Exact-zero pairs are the M-B
/// emission-identification class and stay untouched here (B3).
#[cfg_attr(not(test), allow(dead_code))]
pub(crate) fn collapse_subresolution_intersection_segments(
    mesh: &mut Mesh,
    attribution: &mut Vec<Option<TriangleAttribution>>,
    side_labels: &mut Vec<TriangleSideLabel>,
    intersection_curves: &std::collections::BTreeMap<(u32, u32), Curve>,
    a: &BRep,
    b: &BRep,
) -> bool {
    let mut dummy_bounds = vec![f64::NAN; mesh.tris.len()];
    collapse_subresolution_intersection_segments_with_bounds(
        mesh,
        attribution,
        side_labels,
        &mut dummy_bounds,
        intersection_curves,
        a,
        b,
        None,
    )
}

#[allow(clippy::too_many_arguments)]
pub(crate) fn collapse_subresolution_intersection_segments_with_bounds(
    mesh: &mut Mesh,
    attribution: &mut Vec<Option<TriangleAttribution>>,
    side_labels: &mut Vec<TriangleSideLabel>,
    tri_bounds: &mut Vec<f64>,
    intersection_curves: &std::collections::BTreeMap<(u32, u32), Curve>,
    a: &BRep,
    b: &BRep,
    vertex_params: Option<&mut crate::param_surface::carried::VertexParamMap>,
) -> bool {
    collapse_subresolution_intersection_segments_ex_with_bounds(
        mesh,
        attribution,
        side_labels,
        tri_bounds,
        intersection_curves,
        a,
        b,
        false,
        None,
        vertex_params,
    )
}

#[allow(clippy::too_many_arguments, dead_code)]
pub(crate) fn collapse_subresolution_intersection_segments_ex(
    mesh: &mut Mesh,
    attribution: &mut Vec<Option<TriangleAttribution>>,
    side_labels: &mut Vec<TriangleSideLabel>,
    intersection_curves: &std::collections::BTreeMap<(u32, u32), Curve>,
    a: &BRep,
    b: &BRep,
    scope_by_side_label: bool,
    tangential_verts: Option<&mut std::collections::BTreeSet<u32>>,
) -> bool {
    let mut dummy_bounds = vec![f64::NAN; mesh.tris.len()];
    collapse_subresolution_intersection_segments_ex_with_bounds(
        mesh,
        attribution,
        side_labels,
        &mut dummy_bounds,
        intersection_curves,
        a,
        b,
        scope_by_side_label,
        tangential_verts,
        None,
    )
}

#[allow(clippy::too_many_arguments)]
pub(crate) fn collapse_subresolution_intersection_segments_ex_with_bounds(
    mesh: &mut Mesh,
    attribution: &mut Vec<Option<TriangleAttribution>>,
    side_labels: &mut Vec<TriangleSideLabel>,
    tri_bounds: &mut Vec<f64>,
    intersection_curves: &std::collections::BTreeMap<(u32, u32), Curve>,
    a: &BRep,
    b: &BRep,
    scope_by_side_label: bool,
    mut tangential_verts: Option<&mut std::collections::BTreeSet<u32>>,
    mut vertex_params: Option<&mut crate::param_surface::carried::VertexParamMap>,
) -> bool {
    assert_eq!(
        tri_bounds.len(),
        mesh.tris.len(),
        "collapse_subresolution_intersection_segments: tri_bounds ({}) must be 1:1 with mesh.tris ({})",
        tri_bounds.len(),
        mesh.tris.len(),
    );
    let mut redirect: std::collections::BTreeMap<u32, u32> = std::collections::BTreeMap::new();
    fn resolve(redirect: &std::collections::BTreeMap<u32, u32>, mut v: u32) -> u32 {
        while let Some(&n) = redirect.get(&v) {
            v = n;
        }
        v
    }
    let mut any = false;
    for &(u, v) in intersection_curves.keys() {
        let (ru, rv) = (resolve(&redirect, u), resolve(&redirect, v));
        if ru == rv {
            continue;
        }
        let p = mesh.verts[ru as usize].as_array();
        let q = mesh.verts[rv as usize].as_array();
        let d2 = (p[0] - q[0]).powi(2) + (p[1] - q[1]).powi(2) + (p[2] - q[2]).powi(2);
        let scale = p
            .iter()
            .chain(q.iter())
            .fold(0.0f64, |m, &c| m.max(c.abs()));
        let band = cad_primitives::TAU_MODEL * (1.0 + scale);
        if std::env::var_os("YANG_KV15B_PROBE").is_some() && d2 < 100.0 * band * band {
            eprintln!(
                "[kv15b] key ({u},{v}) resolved ({ru},{rv}) len={:.3e} band={band:.3e} \
                 verdict={}",
                d2.sqrt(),
                if d2 == 0.0 {
                    "SKIP exact-zero (B3)"
                } else if d2 >= band * band {
                    "SKIP over band"
                } else {
                    "COLLAPSE"
                }
            );
        }
        if d2 == 0.0 || d2 >= band * band {
            continue;
        }
        let survivor = ru.min(rv);
        let victim = ru.max(rv);
        {
            let sp = mesh.verts[survivor as usize].as_array();
            let vp = mesh.verts[victim as usize].as_array();
            let ss = carried_surfaces(mesh, attribution, a, b, survivor, sp);
            let vs = carried_surfaces(mesh, attribution, a, b, victim, vp);
            let (cs, cv) = (ss.len(), vs.len());
            let victim_in_survivor = vs.iter().all(|s| ss.contains(s));
            let survivor_in_victim = ss.iter().all(|s| vs.contains(s));
            if std::env::var_os("YANG_DOUBLECOVER_PROBE").is_some()
                || std::env::var_os("YANG_KV15B_PROBE").is_some()
            {
                eprintln!(
                    "[collapse-site] kv15b carried survivor={survivor} cs={cs} victim={victim} \
                     cv={cv} victim⊆survivor={victim_in_survivor} \
                     survivor⊆victim={survivor_in_victim}"
                );
            }
            if !victim_in_survivor && !survivor_in_victim {
                if std::env::var_os("YANG_KV15B_PROBE").is_some() {
                    eprintln!(
                        "[kv15b] key ({u},{v}) resolved ({ru},{rv}) len={:.3e}: two DISTINCT \
                         junctions (§4-I8) — segment kept",
                        d2.sqrt()
                    );
                }
                continue;
            }
            if survivor_in_victim && !victim_in_survivor {
                let p = mesh.verts[victim as usize];
                mesh.verts[survivor as usize] = p;
                if let Some(vp) = vertex_params.as_deref_mut() {
                    vp.merged_into(survivor, victim, p);
                }
            }
        }
        if std::env::var_os("YANG_DOUBLECOVER_PROBE").is_some() {
            eprintln!(
                "[collapse-site] kv15b victim={victim} p=({:.17e},{:.17e},{:.17e}) \
                 survivor={survivor} q=({:.17e},{:.17e},{:.17e}) dist={:.3e}",
                mesh.verts[victim as usize].as_array()[0],
                mesh.verts[victim as usize].as_array()[1],
                mesh.verts[victim as usize].as_array()[2],
                mesh.verts[survivor as usize].as_array()[0],
                mesh.verts[survivor as usize].as_array()[1],
                mesh.verts[survivor as usize].as_array()[2],
                d2.sqrt(),
            );
        }
        collapse_vertex_scoped_with_bounds(
            mesh,
            attribution,
            side_labels,
            tri_bounds,
            victim,
            survivor,
            scope_by_side_label,
            tangential_verts.as_deref_mut(),
        );
        redirect.insert(victim, survivor);
        any = true;
    }
    assert_eq!(tri_bounds.len(), mesh.tris.len());
    any
}

#[cfg_attr(not(test), allow(dead_code))]
pub(crate) fn collapse_subtauwork_mesh_edges(
    mesh: &mut Mesh,
    attribution: &mut Vec<Option<TriangleAttribution>>,
    side_labels: &mut Vec<TriangleSideLabel>,
) -> bool {
    let mut dummy_bounds = vec![f64::NAN; mesh.tris.len()];
    collapse_subtauwork_mesh_edges_with_bounds(mesh, attribution, side_labels, &mut dummy_bounds)
}

pub(crate) fn collapse_subtauwork_mesh_edges_with_bounds(
    mesh: &mut Mesh,
    attribution: &mut Vec<Option<TriangleAttribution>>,
    side_labels: &mut Vec<TriangleSideLabel>,
    tri_bounds: &mut Vec<f64>,
) -> bool {
    collapse_subtauwork_mesh_edges_ex_with_bounds(
        mesh,
        attribution,
        side_labels,
        tri_bounds,
        false,
        None,
    )
}

#[allow(dead_code)]
pub(crate) fn collapse_subtauwork_mesh_edges_ex(
    mesh: &mut Mesh,
    attribution: &mut Vec<Option<TriangleAttribution>>,
    side_labels: &mut Vec<TriangleSideLabel>,
    scope_by_side_label: bool,
    tangential_verts: Option<&mut std::collections::BTreeSet<u32>>,
) -> bool {
    let mut dummy_bounds = vec![f64::NAN; mesh.tris.len()];
    collapse_subtauwork_mesh_edges_ex_with_bounds(
        mesh,
        attribution,
        side_labels,
        &mut dummy_bounds,
        scope_by_side_label,
        tangential_verts,
    )
}

pub(crate) fn collapse_subtauwork_mesh_edges_ex_with_bounds(
    mesh: &mut Mesh,
    attribution: &mut Vec<Option<TriangleAttribution>>,
    side_labels: &mut Vec<TriangleSideLabel>,
    tri_bounds: &mut Vec<f64>,
    scope_by_side_label: bool,
    mut tangential_verts: Option<&mut std::collections::BTreeSet<u32>>,
) -> bool {
    assert_eq!(
        tri_bounds.len(),
        mesh.tris.len(),
        "collapse_subtauwork_mesh_edges: tri_bounds ({}) must be 1:1 with mesh.tris ({})",
        tri_bounds.len(),
        mesh.tris.len(),
    );
    let mut edges: std::collections::BTreeSet<(u32, u32)> = std::collections::BTreeSet::new();
    for tri in &mesh.tris {
        for k in 0..3 {
            let (u, v) = (tri[k], tri[(k + 1) % 3]);
            edges.insert((u.min(v), u.max(v)));
        }
    }
    let mut redirect: std::collections::BTreeMap<u32, u32> = std::collections::BTreeMap::new();
    fn resolve(redirect: &std::collections::BTreeMap<u32, u32>, mut v: u32) -> u32 {
        while let Some(&n) = redirect.get(&v) {
            v = n;
        }
        v
    }
    let mut any = false;
    for &(u, v) in &edges {
        let (ru, rv) = (resolve(&redirect, u), resolve(&redirect, v));
        if ru == rv {
            continue;
        }
        let p = mesh.verts[ru as usize].as_array();
        let q = mesh.verts[rv as usize].as_array();
        let d2 = (p[0] - q[0]).powi(2) + (p[1] - q[1]).powi(2) + (p[2] - q[2]).powi(2);
        let scale = p
            .iter()
            .chain(q.iter())
            .fold(0.0f64, |m, &c| m.max(c.abs()));
        let band = cad_primitives::TAU_WORK * (1.0 + scale);
        if d2 == 0.0 || d2 >= band * band {
            continue;
        }
        let survivor = ru.min(rv);
        let victim = ru.max(rv);
        if std::env::var_os("YANG_DOUBLECOVER_PROBE").is_some() {
            eprintln!(
                "[collapse-site] s194 victim={victim} survivor={survivor} dist={:.3e}",
                d2.sqrt(),
            );
        }
        if crate::stage4_fig11b::census_on() {
            crate::stage4_fig11b::log(&format!(
                "taken\tpass=s194\tvictim={victim}:{} survivor={survivor}:{} dist={:.3e}",
                crate::stage4_fig11b::pos_key(mesh, victim),
                crate::stage4_fig11b::pos_key(mesh, survivor),
                d2.sqrt(),
            ));
        }
        collapse_vertex_scoped_with_bounds(
            mesh,
            attribution,
            side_labels,
            tri_bounds,
            victim,
            survivor,
            scope_by_side_label,
            tangential_verts.as_deref_mut(),
        );
        redirect.insert(victim, survivor);
        any = true;
    }
    assert_eq!(tri_bounds.len(), mesh.tris.len());
    any
}

/// PR-YR11 helper: drop mesh vertices no surviving triangle references and remap
/// triangle indices + the Stage-4 `relocations` keys to the dense vertex set.
///
/// A §4.5.3 [`collapse_vertex`] keeps the full vertex array (it only drops the
/// now-degenerate triangles), leaving the collapsed-away vertices DANGLING. The
/// internal per-shell `check_watertight_2manifold` gate ignores them (it sums V
/// over triangle-referenced verts only), but they inflate a caller's GLOBAL
/// `V − E + F`. An output mesh must carry no unreferenced vertices, so this
/// compaction runs after Stage 4. It is a strict NO-OP (returns early, mesh and
/// `relocations` untouched) when every vertex is already referenced — so the
/// no-collapse paths (planar / perpendicular-circle / on-curve mock) stay
/// byte-identical.
///
/// Returns the old→new index remap when it compacted (`None` on the no-op path),
/// so callers holding vertex-keyed side tables can re-key them. `None` in a slot
/// means that vertex did not survive.
#[cfg_attr(not(test), allow(dead_code))]
pub(crate) fn compact_unreferenced_verts(
    mesh: &mut Mesh,
    relocations: &mut Vec<(u32, f64)>,
    side_labels: &[TriangleSideLabel],
) -> Option<Vec<Option<u32>>> {
    let dummy_bounds = vec![f64::NAN; mesh.tris.len()];
    compact_unreferenced_verts_with_bounds(mesh, relocations, side_labels, &dummy_bounds, None)
}

pub(crate) fn compact_unreferenced_verts_with_bounds(
    mesh: &mut Mesh,
    relocations: &mut Vec<(u32, f64)>,
    side_labels: &[TriangleSideLabel],
    tri_bounds: &[f64],
    vertex_params: Option<&mut crate::param_surface::carried::VertexParamMap>,
) -> Option<Vec<Option<u32>>> {
    debug_assert_eq!(
        side_labels.len(),
        mesh.tris.len(),
        "compact_unreferenced_verts: side_labels ({}) must be 1:1 with mesh.tris ({})",
        side_labels.len(),
        mesh.tris.len(),
    );
    assert_eq!(
        tri_bounds.len(),
        mesh.tris.len(),
        "compact_unreferenced_verts: tri_bounds ({}) must be 1:1 with mesh.tris ({})",
        tri_bounds.len(),
        mesh.tris.len(),
    );
    let n = mesh.verts.len();
    let mut referenced = vec![false; n];
    for tri in &mesh.tris {
        for &v in tri {
            referenced[v as usize] = true;
        }
    }
    if referenced.iter().all(|&r| r) {
        return None; // no danglers — byte-identical no-op.
    }
    // Dense remap preserving the relative order of surviving vertices.
    let mut remap: Vec<Option<u32>> = vec![None; n];
    let mut new_verts: Vec<Point3> = Vec::with_capacity(n);
    for (i, &r) in referenced.iter().enumerate() {
        if r {
            remap[i] = Some(new_verts.len() as u32);
            new_verts.push(mesh.verts[i]);
        }
    }
    let new_tris: Vec<[u32; 3]> = mesh
        .tris
        .iter()
        .map(|tri| {
            // Invariant: `referenced` was built from this same triangle list
            // above, so every triangle vertex has a `Some` remap entry.
            tri.map(|v| {
                remap[v as usize]
                    .expect("compact_unreferenced_verts: triangle vertex not marked referenced")
            })
        })
        .collect();
    *mesh = Mesh::new(new_verts, new_tris);
    if let Some(vp) = vertex_params {
        vp.remap(&remap);
        vp.assert_lockstep(mesh.verts.len(), "compact_unreferenced_verts");
    }
    debug_assert_eq!(
        side_labels.len(),
        mesh.tris.len(),
        "compact_unreferenced_verts: side_labels ({}) must remain 1:1 with mesh.tris ({})",
        side_labels.len(),
        mesh.tris.len(),
    );
    assert_eq!(
        tri_bounds.len(),
        mesh.tris.len(),
        "compact_unreferenced_verts: tri_bounds ({}) must remain 1:1 with mesh.tris ({})",
        tri_bounds.len(),
        mesh.tris.len(),
    );
    // Remap (and drop) relocation keys: a relocation referencing a collapsed-away
    // (now-unreferenced) vertex is no longer in the mesh, so it is dropped.
    let remapped: Vec<(u32, f64)> = relocations
        .iter()
        .filter_map(|&(v, t)| remap[v as usize].map(|nv| (nv, t)))
        .collect();
    *relocations = remapped;
    Some(remap)
}
