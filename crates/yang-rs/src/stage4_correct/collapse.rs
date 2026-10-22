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

/// A mutual degenerate pair (spec `yang_n2_stage4_cdt_mesh_updating.md` §5c.11):
/// both incident triangles of one long edge are degenerate and report that SAME
/// edge as their long edge — a zero-area quad astride `a–c` with the two
/// off-vertices `bl`/`bh` interleaved strictly inside the segment
/// (`0 < t(bl) < t(bh) < 1` along `a→c`). `nl`/`nh` are the OUTER neighbours
/// across the two insertion edges `(bl,c)` / `(a,bh)`, both non-degenerate.
pub(crate) struct MutualPair {
    pub(crate) t1: usize,
    pub(crate) t2: usize,
    pub(crate) nl: usize,
    pub(crate) nh: usize,
    pub(crate) a: u32,
    pub(crate) c: u32,
    pub(crate) bl: u32,
    pub(crate) bh: u32,
}

/// Validate the mutual-pair configuration for degenerate triangle `ti` whose
/// long-edge neighbour `n` is also degenerate. `None` unless: `n`'s long edge is
/// the SAME edge `(a,c)`; the off vertices are distinct and strictly interleaved
/// inside the open segment; and both insertion edges have exactly two incident
/// triangles whose outer member (not `ti`/`n`) is non-degenerate. Anything else
/// keeps the loud STOP (honest deferral, no partial action).
#[allow(clippy::too_many_arguments, clippy::type_complexity)]
pub(crate) fn mutual_pair_candidate(
    mesh: &Mesh,
    edge_tris: &std::collections::HashMap<(u32, u32), Vec<u32>>,
    is_degen: &dyn Fn(usize, &Mesh) -> bool,
    long_edge_off: &dyn Fn(&[u32; 3], &Mesh) -> (u32, u32, u32),
    ti: usize,
    n: usize,
    a: u32,
    c: u32,
    b: u32,
) -> Option<MutualPair> {
    let (na, nc, nb) = long_edge_off(&mesh.tris[n], mesh);
    let key = if a < c { (a, c) } else { (c, a) };
    let nkey = if na < nc { (na, nc) } else { (nc, na) };
    if nkey != key || nb == b {
        return None;
    }
    let pa = mesh.verts[a as usize].as_array();
    let pc = mesh.verts[c as usize].as_array();
    let e = [pc[0] - pa[0], pc[1] - pa[1], pc[2] - pa[2]];
    let l2 = e[0] * e[0] + e[1] * e[1] + e[2] * e[2];
    if l2 == 0.0 {
        return None;
    }
    let tof = |v: u32| {
        let p = mesh.verts[v as usize].as_array();
        ((p[0] - pa[0]) * e[0] + (p[1] - pa[1]) * e[1] + (p[2] - pa[2]) * e[2]) / l2
    };
    let (tb, tnb) = (tof(b), tof(nb));
    let (bl, bh, tl, th) = if tb <= tnb {
        (b, nb, tb, tnb)
    } else {
        (nb, b, tnb, tb)
    };
    // Equal parameters give no deterministic chain order — keep the STOP.
    if !(0.0 < tl && tl < th && th < 1.0) {
        return None;
    }
    let outer = |u: u32, v: u32| -> Option<usize> {
        let k = if u < v { (u, v) } else { (v, u) };
        let list = edge_tris.get(&k)?;
        if list.len() != 2 {
            return None;
        }
        let o = list
            .iter()
            .map(|&x| x as usize)
            .find(|&x| x != ti && x != n)?;
        if is_degen(o, mesh) {
            return None;
        }
        Some(o)
    };
    let nl = outer(bl, c)?;
    let nh = outer(a, bh)?;
    // Two-sidedness. The §5c.11 watertightness argument assumes `nl` and `nh`
    // lie on OPPOSITE sides of the degenerate quad, so the four split pieces are
    // distinct and every chain edge pairs one piece from each side. When both
    // outer neighbours share their third vertex they are instead the SAME fan
    // over the chain: `nl`'s first piece `[bl,bh,dd]` and `nh`'s second piece
    // `[bl,bh,dd]` are then the identical triangle, so the update emits it TWICE
    // — a double cover that does not bound a 2-manifold. Measured on R0038
    // (dd = 17 on both sides): the doubled piece propagated a foreign vertex
    // into an unrelated planar face's loop and surfaced as `NonManifoldOutput`
    // two stages downstream, 7.5e0 off that face's plane. Exact index equality,
    // no tolerance — it is precisely the condition under which the four pieces
    // fail to be distinct. Keep the loud STOP (honest deferral, no partial
    // action): the same-apex fan is a DIFFERENT configuration needing its own
    // repair (a 3-triangle refan of the chain), not this arm.
    let third = |t: usize, u: u32, v: u32| -> Option<u32> {
        mesh.tris[t].iter().copied().find(|&x| x != u && x != v)
    };
    if third(nl, bl, c)? == third(nh, a, bh)? {
        return None;
    }
    Some(MutualPair {
        t1: ti,
        t2: n,
        nl,
        nh,
        a,
        c,
        bl,
        bh,
    })
}

/// Execute the mutual-pair update (spec §5c.11): drop both zero-area members and
/// Fig-11(a)-split the two outer neighbours — `nl` (across `(bl,c)`) at `bh`,
/// `nh` (across `(a,bh)`) at `bl` — so both sides of the former quad carry the
/// identical vertex chain `a–bl–bh–c`. Pure connectivity: no vertex moves, none
/// is added or dropped; each split piece inherits its parent's winding (restored
/// against the parent's area normal) and attribution. Watertight by
/// construction: the long edge `(a,c)` and both insertion edges vanish together
/// with their two incident triangles each, and every chain edge pairs one piece
/// from each side.
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

pub(crate) fn resolve_mutual_degenerate_pair(
    mesh: &mut Mesh,
    attr_vec: &mut Vec<Option<TriangleAttribution>>,
    m: &MutualPair,
) {
    let split = |parent: [u32; 3], u: u32, v: u32, ins: u32| -> ([u32; 3], [u32; 3]) {
        // Combinatorial Fig-11(a) split (P0005): the halves inherit the
        // parent's winding; no `orient_tri` proxy (see `split_tri_at_edge`).
        split_tri_at_edge(parent, u, v, ins)
            .expect("split parent shares the insertion edge, has a third vertex")
    };
    let (l1, l2) = split(mesh.tris[m.nl], m.bl, m.c, m.bh);
    let (h1, h2) = split(mesh.tris[m.nh], m.a, m.bh, m.bl);
    let nl_attr = attr_vec.get(m.nl).copied().flatten();
    let nh_attr = attr_vec.get(m.nh).copied().flatten();
    let drop = [m.t1, m.t2, m.nl, m.nh];
    let mut new_tris: Vec<[u32; 3]> = Vec::with_capacity(mesh.tris.len());
    let mut new_attr: Vec<Option<TriangleAttribution>> = Vec::with_capacity(attr_vec.len());
    for (i, t) in mesh.tris.iter().enumerate() {
        if drop.contains(&i) {
            continue;
        }
        new_tris.push(*t);
        new_attr.push(attr_vec.get(i).copied().flatten());
    }
    for (t, at) in [(l1, nl_attr), (l2, nl_attr), (h1, nh_attr), (h2, nh_attr)] {
        new_tris.push(t);
        new_attr.push(at);
    }
    *mesh = Mesh::new(std::mem::take(&mut mesh.verts), new_tris);
    *attr_vec = new_attr;
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
pub(crate) fn collapse_vertex(
    mesh: &mut Mesh,
    attribution: &mut Vec<Option<TriangleAttribution>>,
    victim: u32,
    survivor: u32,
) -> usize {
    let mut new_tris: Vec<[u32; 3]> = Vec::with_capacity(mesh.tris.len());
    let mut new_attr: Vec<Option<TriangleAttribution>> = Vec::with_capacity(attribution.len());
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
    }
    // Membrane cancellation (spec `yang_collapse_membrane_cancellation`):
    // identifying `victim` with `survivor` can turn the two-triangle pleat
    // that spanned the twin gap into an EXACT duplicate pair with OPPOSITE
    // windings — a zero-volume doubled flap whose 6 directed edges are 3
    // mutual-reverse pairs (they pair with EACH OTHER). Dropping BOTH
    // preserves the watertight half-edge pairing and restores manifold
    // count-2 on the shared fan edges (the measured F0059 mint: the PR-KV9
    // junction-twin collapse at the Steinmetz seam apex derailed the Stage-6
    // wedge walk). Same-winding duplicates and ≥3-copy groups are genuine
    // non-manifold configurations — left untouched for the downstream loud
    // STOPs (P9: never silently pick).
    {
        let mut by_triple: std::collections::HashMap<[u32; 3], Vec<usize>> =
            std::collections::HashMap::new();
        for (t, tri) in new_tris.iter().enumerate() {
            let mut s = *tri;
            s.sort_unstable();
            by_triple.entry(s).or_default().push(t);
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
            dropped += cancel.len();
        }
    }
    *mesh = Mesh::new(std::mem::take(&mut mesh.verts), new_tris);
    *attribution = new_attr;
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
pub(crate) fn weld_coincident_relocated(
    mesh: &mut Mesh,
    attribution: &mut Vec<Option<TriangleAttribution>>,
    a: &BRep,
    b: &BRep,
    moved: &std::collections::HashSet<u32>,
    minted: &std::collections::HashSet<u32>,
) -> bool {
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
                collapse_vertex(mesh, attribution, victim, survivor);
                welded = true;
            }
            None => break,
        }
    }
    welded
}

/// P3b inc-4b (spec `yang_169_p3b_curved_partner_pierce.md` §5 inc-4b):
/// beyond-corner conformal TRIM. A Stage-4 relocation can land a
/// section-curve sample OUTSIDE the bounded owner face, past a Stage-1
/// minted corner junction on the same curve (F0082's phantom: the chord-ring
/// crossing vertex relocated to the ellipse's canonical t≈π/2, 1.29e-3
/// beyond the wall the minted corner terminates at). Such a sample has ZERO
/// kept content — the curve stops being an output boundary at the corner —
/// so it is removed TOPOLOGICALLY: edge-collapse phantom→mint (survivor =
/// the mint, `collapse_vertex` watertight-preserving), justified by the
/// out-of-face + beyond-corner predicate, never by distance.
///
/// Predicate, per mesh edge (m, v) with `m` minted and `v` moved (and not
/// itself a mint) — all bands derived, no new tolerance:
/// - beyond-corner: signed distance to an owner plane i with a CONVEX
///   pierce-time verdict (`trim_beyond`) exceeds `TAU_MODEL·(1+scale)`;
/// - on-the-other-plane: |signed distance to plane j| ≤ `TAU_EVAL·(1+scale)`
///   (v is a section-curve sample of partner×plane j ⇒ the segment m→v
///   leaves the bounded face AT the corner);
/// - corridor cap: |v−m| ≤ `tangent_plane_corridor(d_ε, sinθ)`,
///   sinθ = dᵢ(v)/|v−m| — the chord-crossing displacement bound. Beyond it
///   the vert may be LEGITIMATE far-side geometry (the owner plane is
///   infinite; a non-convex face can re-enter its positive half-space away
///   from this corner): NO fire, status quo — the #173/ring gates downstream
///   stay loud. A missed trim is never worse; a false trim would be
///   silent-wrong, so every leg fails closed.
pub(crate) fn trim_beyond_corner_phantoms(
    mesh: &mut Mesh,
    attribution: &mut Vec<Option<TriangleAttribution>>,
    moved: &std::collections::HashSet<u32>,
    minted: &std::collections::BTreeMap<u32, crate::boolean::MintProvenance>,
    d_eps: f64,
) -> bool {
    let probe = std::env::var_os("YANG_P3B_TRIM_PROBE").is_some();
    let mut trimmed = false;
    'fixed_point: loop {
        // Patch-subset guard (the F0082 cap-ring lesson, measured 2026-07-19):
        // collapsing v→m reroutes EVERY patch incident to v onto m, so the
        // zero-content justification must hold for all of them — if v carries
        // a patch m does not touch (F0082: the phantom is also a boundary
        // vertex of B's near-coplanar CAP face, which the mint is 1e-4 off),
        // the collapse would drag that face's ring onto a foreign point
        // (s6-planar-loop-nonplanar, silent-wrong were the band looser).
        // Eligibility therefore requires attributed-patch(v) ⊆
        // attributed-patch(m); unattributed (`None`) intersection-strip
        // triangles are neutral (they belong to the junction itself).
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
                        collapse_vertex(mesh, attribution, v, m);
                        trimmed = true;
                        continue 'fixed_point;
                    }
                }
            }
        }
        break;
    }
    trimmed
}

/// P3b inc-4c (spec `yang_169_p3b_inc4c_fan_retriangulation.md`): the Yang
/// §4.4.1 "update the triangulation accordingly" half of the Stage-4 merge
/// ops. The weld/trim passes above collapse clusters of victims onto Stage-1
/// minted junction vertices; a victim cluster spanning ADJACENT mints maps
/// every pre-mesh edge crossing the victim partition onto the single
/// mint-pair edge, stacking surviving triangles there (R0061 measured:
/// edge (186,211) 1A+1B pre → 4A+2B post; six edges total-use ≠ 2, all
/// mint-anchored). The survivors have DISTINCT near-dup tips, so no
/// exact-duplicate rule (membrane cancellation, i6 wedge dedup) can fire —
/// and no deletion-only rule is correct (dropping a copy leaves its tip
/// unpaired). The repair is connectivity-only local RE-TRIANGULATION of the
/// merged fan regions, per attribution key, keeping every healthy edge
/// verbatim:
///
/// - detect: undirected edges with total incident-triangle count ≠ 2 and ≥1
///   minted endpoint (the mint anchor keeps this away from legitimate
///   mint-free 4-sheet structure, e.g. Steinmetz tangency generators);
/// - cluster defective edges by shared vertices; per cluster, per
///   attribution key: region = that key's triangles incident to a cluster
///   vertex;
/// - per region, classify edges: pinned (use 2) with exactly 1 region tri →
///   BOUNDARY (kept verbatim; the outside/other-side triangle keeps it
///   paired); pinned with 2 region tris → interior (CDT may rewire);
///   defective with all tris inside the cluster's regions → interior
///   (the fold being dissolved); anything else → bail the cluster;
/// - keep-boundary re-CDT in the region's `SurfaceChart` (Plane/Cylinder;
///   cylinder θ re-centred, quarter-turn straddle guard). NO vertex is
///   created, moved, or removed — both operands land on the identical 3D
///   seam polyline by construction (the degenerate-but-sufficient case of
///   the Phase-A two-sided update);
/// - postcondition, all-loud: after splicing the cluster, every edge of the
///   new triangles has total-use exactly 2 and every formerly-defective
///   edge has total-use 0 or 2 — else the cluster's ORIGINAL triangles are
///   restored (a bail may never trade one non-manifold shape for another).
///
/// Bails are per-cluster and leave the mesh untouched for the downstream
/// loud gates (P10: this pass can only convert a loud STOP into a correct
/// result or leave it standing). Probe: `YANG_P3B_FANFIX_PROBE`.
pub(crate) fn retriangulate_collapsed_fan_regions(
    mesh: &mut Mesh,
    attr_vec: &mut Vec<Option<TriangleAttribution>>,
    brep_a: &BRep,
    brep_b: &BRep,
    moved: &std::collections::HashSet<u32>,
    minted: &std::collections::HashSet<u32>,
) -> bool {
    use std::collections::{BTreeMap, BTreeSet};
    let probe = std::env::var_os("YANG_P3B_FANFIX_PROBE").is_some();
    if minted.is_empty() {
        return false;
    }
    let mut changed = false;
    // Clusters already attempted (by sorted vertex set) — successful repairs
    // recompute from the mutated mesh; bailed clusters are not retried.
    let mut attempted: BTreeSet<Vec<u32>> = BTreeSet::new();
    'passes: loop {
        // Undirected edge → incident triangle indices, whole mesh.
        let mut edge_use: BTreeMap<(u32, u32), Vec<usize>> = BTreeMap::new();
        for (ti, tri) in mesh.tris.iter().enumerate() {
            for (i, j) in [(0usize, 1usize), (1, 2), (2, 0)] {
                let (u, w) = (tri[i], tri[j]);
                let e = if u < w { (u, w) } else { (w, u) };
                edge_use.entry(e).or_default().push(ti);
            }
        }
        // Defective edges: total use ≠ 2, ≥1 minted endpoint.
        let defective: Vec<(u32, u32)> = edge_use
            .iter()
            .filter(|(&(u, w), ts)| ts.len() != 2 && (minted.contains(&u) || minted.contains(&w)))
            .map(|(&e, _)| e)
            .collect();
        if defective.is_empty() {
            break;
        }
        // Cluster by shared vertices (deterministic union-find over BTree order).
        let mut root_of: BTreeMap<u32, u32> = BTreeMap::new();
        fn find(m: &mut BTreeMap<u32, u32>, v: u32) -> u32 {
            let mut r = v;
            while m.get(&r).copied().unwrap_or(r) != r {
                r = m[&r];
            }
            let mut c = v;
            while m.get(&c).copied().unwrap_or(c) != r {
                let n = m[&c];
                m.insert(c, r);
                c = n;
            }
            r
        }
        for &(u, w) in &defective {
            let (ru, rw) = (find(&mut root_of, u), find(&mut root_of, w));
            if ru != rw {
                root_of.insert(ru.max(rw), ru.min(rw));
            }
        }
        let mut clusters: BTreeMap<u32, BTreeSet<u32>> = BTreeMap::new();
        for &(u, w) in &defective {
            let r = find(&mut root_of, u);
            clusters.entry(r).or_default().extend([u, w]);
        }
        for cluster in clusters.values() {
            let sig: Vec<u32> = cluster.iter().copied().collect();
            if attempted.contains(&sig) {
                continue;
            }
            attempted.insert(sig.clone());
            match repair_fan_cluster(
                mesh, attr_vec, brep_a, brep_b, cluster, &edge_use, moved, minted, probe,
            ) {
                Some(()) => {
                    if probe {
                        eprintln!("[p3b-fanfix] cluster {sig:?} REPAIRED");
                    }
                    changed = true;
                    continue 'passes; // mesh mutated — recompute maps
                }
                None => {
                    if probe {
                        eprintln!("[p3b-fanfix] cluster {sig:?} bailed (loud gates stand)");
                    }
                }
            }
        }
        break; // all remaining clusters attempted (bailed) — done
    }
    changed
}

/// One cluster's repair attempt for [`retriangulate_collapsed_fan_regions`].
/// `Some(())` = the mesh was mutated and the postcondition verified;
/// `None` = bail, mesh guaranteed untouched.
#[allow(clippy::too_many_lines)]
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
/// Used by the inc-4c-2 chain decimation: a sample the paper's own
/// refinement loop would never have inserted may be removed (deviation N58,
/// paper-criterion form). The measurements themselves are factored into
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

/// inc-4c-2: analytic curve parameter for a seam run between the two faces'
/// surfaces, or `None` for an unsupported pair (the run is then left as-is
/// and the CDT stays the loud verifier).
///
/// * Plane × Plane — the section is a line: t = p · d̂ with d = n̂₁×n̂₂
///   (near-parallel pair → `None`).
/// * Plane × Cylinder — the section is an ellipse (θ injective along it):
///   t = θ re-centred on the run's circular mean; a plane ∥ to the axis cuts
///   generator lines instead, where the axial coordinate orders the run. A
///   quarter-turn straddle after re-centring → `None`.
pub(crate) fn seam_run_params(
    s1: Surface,
    s2: Surface,
    path: &[u32],
    mesh: &Mesh,
) -> Option<Vec<f64>> {
    let dot3 = |a: [f64; 3], b: [f64; 3]| a[0] * b[0] + a[1] * b[1] + a[2] * b[2];
    match (s1, s2) {
        (Surface::Plane { normal: n1, .. }, Surface::Plane { normal: n2, .. }) => {
            let (a, b) = (normalize3(n1.as_array()), normalize3(n2.as_array()));
            let d = [
                a[1] * b[2] - a[2] * b[1],
                a[2] * b[0] - a[0] * b[2],
                a[0] * b[1] - a[1] * b[0],
            ];
            let len = dot3(d, d).sqrt();
            if len < 1e-9 {
                return None; // near-parallel planes: no line direction
            }
            let d = [d[0] / len, d[1] / len, d[2] / len];
            Some(
                path.iter()
                    .map(|&v| dot3(mesh.verts[v as usize].as_array(), d))
                    .collect(),
            )
        }
        (
            Surface::Plane { normal, .. },
            Surface::Cylinder {
                axis_point,
                axis_dir,
                ..
            },
        )
        | (
            Surface::Cylinder {
                axis_point,
                axis_dir,
                ..
            },
            Surface::Plane { normal, .. },
        ) => {
            let ax = normalize3(axis_dir.as_array());
            let nn = normalize3(normal.as_array());
            let ap = axis_point.as_array();
            if dot3(ax, nn).abs() < 1e-6 {
                // Generator-line section: order axially.
                return Some(
                    path.iter()
                        .map(|&v| {
                            let p = mesh.verts[v as usize].as_array();
                            dot3([p[0] - ap[0], p[1] - ap[1], p[2] - ap[2]], ax)
                        })
                        .collect(),
                );
            }
            let (e1v, e2v) = ortho_basis(cad_primitives::Vector3::new(ax[0], ax[1], ax[2]));
            let (e1, e2) = (e1v.as_array(), e2v.as_array());
            let thetas: Vec<f64> = path
                .iter()
                .map(|&v| {
                    let p = mesh.verts[v as usize].as_array();
                    let w = [p[0] - ap[0], p[1] - ap[1], p[2] - ap[2]];
                    let z = dot3(w, ax);
                    let r = [w[0] - z * ax[0], w[1] - z * ax[1], w[2] - z * ax[2]];
                    dot3(r, e2).atan2(dot3(r, e1))
                })
                .collect();
            let (mut sx, mut sy) = (0.0f64, 0.0f64);
            for &t in &thetas {
                sx += t.cos();
                sy += t.sin();
            }
            let t0 = sy.atan2(sx);
            let mut out = Vec::with_capacity(thetas.len());
            for &t in &thetas {
                let mut dt = t - t0;
                while dt > std::f64::consts::PI {
                    dt -= 2.0 * std::f64::consts::PI;
                }
                while dt < -std::f64::consts::PI {
                    dt += 2.0 * std::f64::consts::PI;
                }
                if dt.abs() > std::f64::consts::FRAC_PI_2 {
                    return None; // quarter-turn straddle
                }
                out.push(dt);
            }
            Some(out)
        }
        _ => None,
    }
}

/// An unordered attribution-key pair naming one seam chain's two faces.
pub(crate) type SeamKeyPair = ((InputId, u32), (InputId, u32));

#[allow(clippy::too_many_arguments)]
pub(crate) fn repair_fan_cluster(
    mesh: &mut Mesh,
    attr_vec: &mut Vec<Option<TriangleAttribution>>,
    brep_a: &BRep,
    brep_b: &BRep,
    cluster: &std::collections::BTreeSet<u32>,
    edge_use: &std::collections::BTreeMap<(u32, u32), Vec<usize>>,
    moved: &std::collections::HashSet<u32>,
    minted: &std::collections::HashSet<u32>,
    probe: bool,
) -> Option<()> {
    use std::collections::{BTreeMap, BTreeSet, HashMap};
    let bail = |reason: &str| -> Option<()> {
        if probe {
            eprintln!("[p3b-fanfix] bail: {reason}");
        }
        None
    };
    let surf_of = |k: (InputId, u32)| -> Surface {
        let br = if matches!(k.0, InputId::A) {
            brep_a
        } else {
            brep_b
        };
        br.faces()[k.1 as usize].surface
    };

    // Seam-pinned defective edges: a defective edge where some attribution
    // key contributes EXACTLY ONE triangle is a live seam edge — that side is
    // unfolded (a fold contributes >=2 same-side triangles), its single
    // triangle is legitimate kept surface, and the closed output must pair it
    // across the edge from the other side. Such an edge is constrained as a
    // BOUNDARY edge in every region touching it (each side supplies exactly
    // one triangle -> total 2). Balanced fold chords (every key count != 1,
    // e.g. R0061's (186,211) 4A+2B and (193,211) 2A+2B) stay free: the CDTs
    // may dissolve or re-mint them; the postcondition verifies the total.
    let seam_pinned = |e: &(u32, u32)| -> bool {
        let ts = &edge_use[e];
        let mut per_key: BTreeMap<(InputId, u32), usize> = BTreeMap::new();
        for &ti in ts {
            if let Some(at) = attr_vec.get(ti).copied().flatten() {
                *per_key.entry((at.input, at.face)).or_default() += 1;
            }
        }
        per_key.values().any(|&n| n == 1)
    };
    // Regions: attribution key → triangles incident to an ANCHOR vertex
    // (initially the cluster; inc-4c-2 grows the anchor set when a seam
    // disorder reaches the region rim). An UNATTRIBUTED triangle touching
    // the anchors leaves the repair without a surface to re-CDT in — bail.
    //
    // inc-4c-2 seam-run canonicalization (spec §5): Stage-4 relocation can
    // land near-dup seam samples OUT OF ORDER along their analytic section
    // curve (the chain reflects stale chordal positions), so the region
    // boundaries self-cross in-chart and no keep-boundary CDT can run. The
    // chain is connectivity: for every seam run (path of pinned/seam-pinned
    // edges between exactly two attribution keys, fully inside the cluster
    // regions — both sides re-CDT, so the chain is rewireable), sort the
    // run's vertices by the pair's analytic curve parameter and constrain
    // BOTH sides to the sorted chain. No vertex moves; ties bail; a
    // disorder whose parameter extremes are not the run's path ends reaches
    // beyond the current regions → grow the anchors by the run ends and
    // rebuild (bounded).
    struct RegionPlan {
        key: (InputId, u32),
        boundary: std::collections::BTreeSet<(u32, u32)>,
        new_tris: Vec<[u32; 3]>,
    }
    let mut anchors: BTreeSet<u32> = cluster.clone();
    let mut grow_rounds = 0usize;
    let (plans, in_regions, removed_chain, added_chain) = 'grow: loop {
        let mut regions: BTreeMap<(InputId, u32), BTreeSet<usize>> = BTreeMap::new();
        for (ti, tri) in mesh.tris.iter().enumerate() {
            if !tri.iter().any(|v| anchors.contains(v)) {
                continue;
            }
            match attr_vec.get(ti).copied().flatten() {
                Some(at) => {
                    regions.entry((at.input, at.face)).or_default().insert(ti);
                }
                None => return bail("unattributed triangle in cluster"),
            }
        }
        if regions.is_empty() {
            return bail("empty cluster");
        }
        let in_regions: BTreeSet<usize> = regions.values().flatten().copied().collect();
        // Every defective cluster edge must be fully inside the cluster's regions.
        for (&(u, w), ts) in edge_use {
            if ts.len() != 2
                && (cluster.contains(&u) || cluster.contains(&w))
                && !ts.iter().all(|t| in_regions.contains(t))
            {
                return bail("defective edge reaches outside the cluster regions");
            }
        }
        // Rewireable seam edges by key pair.
        let mut by_pair: BTreeMap<SeamKeyPair, BTreeSet<(u32, u32)>> = BTreeMap::new();
        // ALL same-pair seam edges mesh-wide (anchor detection: a component
        // vertex touching a same-pair seam edge OUTSIDE the rewireable set
        // continues the chain beyond the regions).
        let mut full_pair: BTreeMap<SeamKeyPair, BTreeSet<(u32, u32)>> = BTreeMap::new();
        for (&e, ts) in edge_use {
            if ts.is_empty() {
                continue;
            }
            let mut keys: BTreeSet<(InputId, u32)> = BTreeSet::new();
            let mut attributed = true;
            for &ti in ts {
                match attr_vec.get(ti).copied().flatten() {
                    Some(at) => {
                        keys.insert((at.input, at.face));
                    }
                    None => attributed = false,
                }
            }
            if !attributed || keys.len() != 2 {
                continue;
            }
            if !(ts.len() == 2 || seam_pinned(&e)) {
                continue; // balanced fold chords are not chain members
            }
            let mut it = keys.into_iter();
            let pair = (it.next().expect("2 keys"), it.next().expect("2 keys"));
            full_pair.entry(pair).or_default().insert(e);
            if ts.iter().all(|t| in_regions.contains(t)) {
                by_pair.entry(pair).or_default().insert(e);
            }
        }
        let mut removed_chain: BTreeSet<(u32, u32)> = BTreeSet::new();
        let mut added_chain: Vec<(SeamKeyPair, (u32, u32))> = Vec::new();
        // The FULL rewritten chains (before the unchanged-segment cancel) —
        // used for component merging: an unchanged chain segment still ties
        // the components it touches together.
        let mut chain_merge_edges: Vec<(SeamKeyPair, (u32, u32))> = Vec::new();
        let mut dropped_verts: BTreeSet<u32> = BTreeSet::new();
        for (pair, edges) in &by_pair {
            let mut adj: BTreeMap<u32, Vec<u32>> = BTreeMap::new();
            for &(u, w) in edges {
                adj.entry(u).or_default().push(w);
                adj.entry(w).or_default().push(u);
            }
            let mut seen: BTreeSet<u32> = BTreeSet::new();
            for &start in adj.keys() {
                if seen.contains(&start) {
                    continue;
                }
                let mut stack = vec![start];
                let mut comp: BTreeSet<u32> = BTreeSet::new();
                while let Some(v) = stack.pop() {
                    if !comp.insert(v) {
                        continue;
                    }
                    for &n in &adj[&v] {
                        if !comp.contains(&n) {
                            stack.push(n);
                        }
                    }
                }
                seen.extend(comp.iter().copied());
                if comp.len() < 3 {
                    continue; // a 2-vert run cannot be disordered
                }
                let verts: Vec<u32> = comp.iter().copied().collect();
                let Some(params) = seam_run_params(surf_of(pair.0), surf_of(pair.1), &verts, mesh)
                else {
                    if probe {
                        eprintln!("[p3b-fanfix] seam run {pair:?} {verts:?}: no parameter");
                    }
                    continue; // unsupported pair: left as-is (the CDT verifies)
                };
                let mut order: Vec<usize> = (0..verts.len()).collect();
                order.sort_by(|&a, &b| {
                    params[a]
                        .partial_cmp(&params[b])
                        .unwrap_or(std::cmp::Ordering::Equal)
                });
                if order.windows(2).any(|w| params[w[1]] == params[w[0]]) {
                    return bail("seam run parameter tie");
                }
                // The sorted chain's edge set; if it matches the existing run
                // (already a parameter-ordered path), nothing to do.
                let sorted_edges_raw: BTreeSet<(u32, u32)> = order
                    .windows(2)
                    .map(|w| {
                        let (a, b) = (verts[w[0]], verts[w[1]]);
                        if a < b {
                            (a, b)
                        } else {
                            (b, a)
                        }
                    })
                    .collect();
                if &sorted_edges_raw == edges {
                    continue; // already in curve order
                }
                // External connections (same-pair seam edges leaving the
                // rewireable set) may only attach at the parameter extremes;
                // otherwise the disorder reaches past the regions — grow.
                let outside_pair: BTreeSet<(u32, u32)> =
                    full_pair[pair].difference(edges).copied().collect();
                let (lo, hi) = (verts[order[0]], verts[*order.last().expect("nonempty")]);
                let mut grow_verts: BTreeSet<u32> = BTreeSet::new();
                for &(u, w) in &outside_pair {
                    for (a, b) in [(u, w), (w, u)] {
                        if comp.contains(&a) && a != lo && a != hi {
                            grow_verts.insert(b); // pull the chain outward
                        }
                    }
                }
                if !grow_verts.is_empty() {
                    grow_rounds += 1;
                    if grow_rounds > 16 {
                        return bail("seam disorder growth bound exceeded");
                    }
                    if probe {
                        eprintln!(
                            "[p3b-fanfix] seam run {pair:?} disorder reaches its rim — \
                             growing anchors by {grow_verts:?} (round {grow_rounds})"
                        );
                    }
                    anchors.extend(grow_verts);
                    continue 'grow;
                }
                if probe {
                    eprintln!(
                        "[p3b-fanfix] seam run reorder {pair:?}: {verts:?} -> {:?}",
                        order.iter().map(|&i| verts[i]).collect::<Vec<_>>()
                    );
                }
                // §4.3/§4.3.4 loop cleanup on the sorted chain: a RELOCATED
                // sample (moved, non-mint, every triangle inside the regions,
                // on no other seam pair) is dropped iff the PAPER's own
                // curve-refinement acceptance test says the chord between its
                // kept neighbours suffices without it (h/l/α against
                // d_p = TAU_MODEL·(1+scale), `paper_chain_sample_redundant`) —
                // the resulting polyline is one the paper's refinement loop
                // would itself terminate at. Without this cleanup the output
                // ring carries needle samples and the render tessellation
                // mints a degenerate sliver downstream (measured: R0061
                // SUPPORTED_WRONG). Deviation N58, paper-criterion form.
                let on_other_pair = |v: u32| {
                    full_pair
                        .iter()
                        .any(|(op, oes)| op != pair && oes.iter().any(|&(a, b)| a == v || b == v))
                };
                let tris_all_in = |v: u32| {
                    edge_use
                        .iter()
                        .filter(|(&(a, b), _)| a == v || b == v)
                        .all(|(_, ts)| ts.iter().all(|t| in_regions.contains(t)))
                };
                let sorted_verts: Vec<u32> = order.iter().map(|&i| verts[i]).collect();
                let mut kept: Vec<u32> = vec![sorted_verts[0]];
                for i in 1..sorted_verts.len() - 1 {
                    let v = sorted_verts[i];
                    let droppable = moved.contains(&v)
                        && !minted.contains(&v)
                        && !on_other_pair(v)
                        && tris_all_in(v);
                    let drop = droppable
                        && paper_chain_sample_redundant(
                            mesh.verts[*kept.last().expect("nonempty") as usize].as_array(),
                            mesh.verts[v as usize].as_array(),
                            mesh.verts[sorted_verts[i + 1] as usize].as_array(),
                        );
                    if drop {
                        if probe {
                            eprintln!(
                                "[p3b-fanfix] chain sample v{v} dropped (Yang §4.3.4 \
                                 h/l/α redundant, pair {pair:?})"
                            );
                        }
                        dropped_verts.insert(v);
                    } else {
                        kept.push(v);
                    }
                }
                kept.push(*sorted_verts.last().expect("nonempty"));
                let sorted_edges: BTreeSet<(u32, u32)> = kept
                    .windows(2)
                    .map(|w| {
                        if w[0] < w[1] {
                            (w[0], w[1])
                        } else {
                            (w[1], w[0])
                        }
                    })
                    .collect();
                removed_chain.extend(edges.iter().copied());
                for e in &sorted_edges {
                    added_chain.push((*pair, *e));
                    chain_merge_edges.push((*pair, *e));
                }
            }
        }
        // Unchanged segments (present in both sets) stay classified normally.
        let common: Vec<(u32, u32)> = added_chain
            .iter()
            .map(|&(_, e)| e)
            .filter(|e| removed_chain.contains(e))
            .collect();
        for e in &common {
            removed_chain.remove(e);
        }
        added_chain.retain(|(_, e)| !common.contains(e));
        let mut plans: Vec<RegionPlan> = Vec::new();
        // Regions split into edge-connected COMPONENTS: at a 4-strand crossing
        // mint the kept surface of one face meets the cluster in two sectors
        // pinched at the mint; each sector re-triangulates as its own disc.
        let mut components: Vec<((InputId, u32), BTreeSet<usize>)> = Vec::new();
        for (&key, rtris) in &regions {
            let tlist: Vec<usize> = rtris.iter().copied().collect();
            let mut parent: Vec<usize> = (0..tlist.len()).collect();
            fn cfind(p: &mut [usize], x: usize) -> usize {
                let mut r = x;
                while p[r] != r {
                    r = p[r];
                }
                let mut c = x;
                while p[c] != r {
                    let n = p[c];
                    p[c] = r;
                    c = n;
                }
                r
            }
            let mut edge_first: BTreeMap<(u32, u32), usize> = BTreeMap::new();
            for (li, &ti) in tlist.iter().enumerate() {
                let tri = mesh.tris[ti];
                for (i, j) in [(0usize, 1usize), (1, 2), (2, 0)] {
                    let (u, w) = (tri[i], tri[j]);
                    let e = if u < w { (u, w) } else { (w, u) };
                    if let Some(&lj) = edge_first.get(&e) {
                        let (ra, rb) = (cfind(&mut parent, li), cfind(&mut parent, lj));
                        if ra != rb {
                            parent[ra] = rb;
                        }
                    } else {
                        edge_first.insert(e, li);
                    }
                }
            }
            let mut by_root: BTreeMap<usize, BTreeSet<usize>> = BTreeMap::new();
            for (li, &ti) in tlist.iter().enumerate() {
                by_root
                    .entry(cfind(&mut parent, li))
                    .or_default()
                    .insert(ti);
            }
            for comp in by_root.into_values() {
                components.push((key, comp));
            }
        }
        // inc-4c-2: a rewired chain can connect two components of the same key
        // (the old chords defined the old sector split; the sorted chain defines
        // the new one). Merge components joined by an added chain edge so the
        // edge lands inside one region's vertex pool; the pinch fan-chain
        // pairing still resolves the (possibly pinched) merged region.
        if !chain_merge_edges.is_empty() {
            let mut cparent: Vec<usize> = (0..components.len()).collect();
            fn cfind2(p: &mut [usize], x: usize) -> usize {
                let mut r = x;
                while p[r] != r {
                    r = p[r];
                }
                let mut c = x;
                while p[c] != r {
                    let n = p[c];
                    p[c] = r;
                    c = n;
                }
                r
            }
            let comp_verts: Vec<BTreeSet<u32>> = components
                .iter()
                .map(|(_, tris)| {
                    let mut vs = BTreeSet::new();
                    for &ti in tris {
                        vs.extend(mesh.tris[ti]);
                    }
                    vs
                })
                .collect();
            for &(pair, e) in &chain_merge_edges {
                for want_key in [pair.0, pair.1] {
                    let holders: Vec<usize> = (0..components.len())
                        .filter(|&ci| {
                            components[ci].0 == want_key
                                && (comp_verts[ci].contains(&e.0) || comp_verts[ci].contains(&e.1))
                        })
                        .collect();
                    for w in holders.windows(2) {
                        let (ra, rb) = (cfind2(&mut cparent, w[0]), cfind2(&mut cparent, w[1]));
                        if ra != rb {
                            cparent[ra] = rb;
                        }
                    }
                }
            }
            let mut merged: BTreeMap<usize, ((InputId, u32), BTreeSet<usize>)> = BTreeMap::new();
            for (ci, (ckey, ctris)) in components.iter().enumerate() {
                let root = cfind2(&mut cparent, ci);
                let entry = merged
                    .entry(root)
                    .or_insert_with(|| (*ckey, BTreeSet::new()));
                entry.1.extend(ctris.iter().copied());
            }
            components = merged.into_values().collect();
            if probe {
                for (k, tris) in &components {
                    let mut vs: BTreeSet<u32> = BTreeSet::new();
                    for &ti in tris {
                        vs.extend(mesh.tris[ti]);
                    }
                    eprintln!(
                        "[p3b-fanfix] post-merge comp {k:?}: {} tris, verts {vs:?}",
                        tris.len()
                    );
                }
            }
        }
        for (key, rtris) in &components {
            let key = *key;
            // Classify this component's edges.
            let mut boundary: BTreeSet<(u32, u32)> = BTreeSet::new();
            for &ti in rtris {
                let tri = mesh.tris[ti];
                for (i, j) in [(0usize, 1usize), (1, 2), (2, 0)] {
                    let (u, w) = (tri[i], tri[j]);
                    let e = if u < w { (u, w) } else { (w, u) };
                    let ts = &edge_use[&e];
                    let n_in = ts.iter().filter(|t| rtris.contains(t)).count();
                    match (ts.len(), n_in) {
                        (2, 1) => {
                            if !removed_chain.contains(&e) {
                                boundary.insert(e);
                            }
                        }
                        (2, 2) => {} // healthy interior — CDT may rewire
                        (n, _) if n != 2 && ts.iter().all(|t| in_regions.contains(t)) => {
                            if seam_pinned(&e) && !removed_chain.contains(&e) {
                                boundary.insert(e);
                            }
                        }
                        _ => return bail("unclassifiable region edge"),
                    }
                }
            }
            // Region vertex pool (minus §4.3-dropped chain samples — they are
            // sub-render-redundant and must not re-enter via the CDT).
            let mut vset: BTreeSet<u32> = BTreeSet::new();
            for &ti in rtris {
                vset.extend(mesh.tris[ti]);
            }
            for v in &dropped_verts {
                vset.remove(v);
            }
            // inc-4c-2: the rewritten (parameter-sorted) chain edges of any run
            // touching this region's key are constrained boundary edges here.
            for &(pair, e) in &added_chain {
                if pair.0 != key && pair.1 != key {
                    continue;
                }
                if vset.contains(&e.0) && vset.contains(&e.1) {
                    boundary.insert(e);
                } else if probe {
                    eprintln!(
                        "[p3b-fanfix] region {key:?} added edge {e:?} NOT in vset \
                     ({} {})",
                        vset.contains(&e.0),
                        vset.contains(&e.1)
                    );
                }
            }
            if boundary.is_empty() {
                return bail("region has no boundary");
            }
            // Chart (Plane / Cylinder only).
            let surf = surf_of(key);
            let Some(chart) = crate::stage4_project::SurfaceChart::new(surf) else {
                return bail("chartless surface");
            };
            // Project. Cylinder: re-centre θ on the region's circular mean and use
            // the isometric u = r·Δθ; bail if the region straddles a quarter turn
            // (the inc-2 branch-cut guard shape).
            let is_cyl = matches!(surf, Surface::Cylinder { .. });
            let radius = match surf {
                Surface::Cylinder { radius, .. } => radius,
                _ => 1.0,
            };
            let raw: Vec<(u32, cad_primitives::Point2)> = vset
                .iter()
                .map(|&v| (v, chart.project(mesh.verts[v as usize])))
                .collect();
            let theta0 = if is_cyl {
                let (mut sx, mut sy) = (0.0f64, 0.0f64);
                for (_, p) in &raw {
                    sx += p.x().cos();
                    sy += p.x().sin();
                }
                sy.atan2(sx)
            } else {
                0.0
            };
            let mut verts2d: Vec<cad_primitives::Point2> = Vec::with_capacity(raw.len());
            let mut global_of_local: Vec<u32> = Vec::with_capacity(raw.len());
            let mut local_of_global: HashMap<u32, u32> = HashMap::new();
            for (v, p) in &raw {
                let uv = if is_cyl {
                    let mut dt = p.x() - theta0;
                    while dt > std::f64::consts::PI {
                        dt -= 2.0 * std::f64::consts::PI;
                    }
                    while dt < -std::f64::consts::PI {
                        dt += 2.0 * std::f64::consts::PI;
                    }
                    if dt.abs() > std::f64::consts::FRAC_PI_2 {
                        return bail("cylinder region straddles a quarter turn");
                    }
                    cad_primitives::Point2::new(radius * dt, p.y())
                } else {
                    *p
                };
                local_of_global.insert(*v, verts2d.len() as u32);
                global_of_local.push(*v);
                verts2d.push(uv);
            }
            // Boundary loops. Every boundary vertex needs EVEN degree; a
            // degree-2 vertex continues to its other neighbour; a higher even
            // degree is a PINCH (a 4-strand crossing mint: two kept sectors of
            // one face meeting at the vertex). At a pinch the walk pairs each
            // incoming boundary edge with the other bounding edge of the
            // INSIDE angular sector between them — the sector holding region
            // triangles in the chart (folds only ever double-cover inside
            // sectors, so the containment test is fold-tolerant).
            let mut bnd_adj: BTreeMap<u32, Vec<u32>> = BTreeMap::new();
            for &(u, w) in &boundary {
                let (lu, lw) = (local_of_global[&u], local_of_global[&w]);
                bnd_adj.entry(lu).or_default().push(lw);
                bnd_adj.entry(lw).or_default().push(lu);
            }
            // inc-4c-2 guard: a rewritten chain edge is synthetic (it has no mesh
            // triangles yet), so the pinch fan-chain pairing cannot rotate
            // through it — every vertex of an added edge must sit on a plain
            // degree-2 boundary. Extra edges at such a vertex belong to OTHER
            // chains that still carry stale chords through it (measured: R0061's
            // arc chain detouring through the wall-chain vert v195) — grow the
            // anchors by those edges' far endpoints so the offending chains
            // become rewireable next round; bail only when growth is exhausted.
            {
                let mut grow_verts: BTreeSet<u32> = BTreeSet::new();
                for &(pair, e) in &added_chain {
                    if pair.0 != key && pair.1 != key {
                        continue;
                    }
                    for v in [e.0, e.1] {
                        if let Some(&lv) = local_of_global.get(&v) {
                            let nbrs: Vec<u32> = bnd_adj
                                .get(&lv)
                                .map(|n| n.iter().map(|&l| global_of_local[l as usize]).collect())
                                .unwrap_or_default();
                            if nbrs.len() != 2 {
                                if probe {
                                    eprintln!(
                                        "[p3b-fanfix] rewritten seam vert v{v} degree {} \
                                     nbrs {nbrs:?}",
                                        nbrs.len()
                                    );
                                }
                                grow_verts.insert(v);
                                grow_verts.extend(nbrs);
                            }
                        }
                    }
                }
                if !grow_verts.is_empty() {
                    grow_rounds += 1;
                    if grow_rounds > 16 {
                        return bail("seam disorder growth bound exceeded");
                    }
                    if probe {
                        eprintln!(
                            "[p3b-fanfix] chain-junction disorder — growing anchors by \
                         {grow_verts:?} (round {grow_rounds})"
                        );
                    }
                    anchors.extend(grow_verts);
                    continue 'grow;
                }
            }
            if bnd_adj.values().any(|n| n.len() % 2 != 0) {
                // An odd-degree boundary vertex is a chain truncated by the
                // region rim (the disorder continues into un-rewired seam
                // segments) — grow the anchors by the odd vertices and their
                // boundary neighbours; bail only when growth is exhausted.
                let mut grow_verts: BTreeSet<u32> = BTreeSet::new();
                for (&lv, nbrs) in &bnd_adj {
                    if nbrs.len() % 2 != 0 {
                        grow_verts.insert(global_of_local[lv as usize]);
                        for &ln in nbrs {
                            grow_verts.insert(global_of_local[ln as usize]);
                        }
                    }
                }
                grow_rounds += 1;
                let stagnant = grow_verts.iter().all(|v| anchors.contains(v));
                if grow_rounds > 16 || stagnant {
                    if probe {
                        eprintln!(
                            "[p3b-fanfix] region {key:?} odd boundary degrees persist \
                         ({}) at {grow_verts:?}",
                            if stagnant { "stagnant" } else { "growth bound" }
                        );
                        for (&lv, nbrs) in &bnd_adj {
                            if nbrs.len() % 2 != 0 {
                                let gv = global_of_local[lv as usize];
                                let es: Vec<u32> =
                                    nbrs.iter().map(|&l| global_of_local[l as usize]).collect();
                                eprintln!("[p3b-fanfix]   odd v{gv} bnd nbrs {es:?}");
                            }
                        }
                    }
                    return bail("region boundary has an odd-degree vertex");
                }
                if probe {
                    eprintln!(
                        "[p3b-fanfix] region {key:?} odd boundary degree — growing anchors \
                     by {grow_verts:?} (round {grow_rounds})"
                    );
                }
                anchors.extend(grow_verts);
                continue 'grow;
            }
            // Pinch pairing: local vert -> (incoming nbr -> outgoing nbr), by
            // COMBINATORIAL fan chains — at the pinch vertex, rotate through the
            // component's triangles via shared at-vertex interior edges; each
            // chain runs boundary-edge -> ... -> boundary-edge and pairs its two
            // ends. Pure connectivity (fold geometry cannot confuse it); an
            // at-vertex edge whose comp-triangle count is not 1 (boundary) or 2
            // (interior) makes rotation ill-defined -> bail.
            let mut pinch_pair: HashMap<(u32, u32), u32> = HashMap::new();
            for (&v, nbrs) in &bnd_adj {
                if nbrs.len() == 2 {
                    continue;
                }
                let gv = global_of_local[v as usize];
                // Edges at v (by OTHER endpoint, global) -> comp tris incident.
                let mut at_v: BTreeMap<u32, Vec<usize>> = BTreeMap::new();
                for &ti in rtris.iter() {
                    let tri = mesh.tris[ti];
                    let Some(pos) = tri.iter().position(|&g| g == gv) else {
                        continue;
                    };
                    for other in [tri[(pos + 1) % 3], tri[(pos + 2) % 3]] {
                        at_v.entry(other).or_default().push(ti);
                    }
                }
                let is_bnd = |other: u32| -> bool {
                    let e = if gv < other { (gv, other) } else { (other, gv) };
                    boundary.contains(&e)
                };
                for (&other, ts) in &at_v {
                    let want = if is_bnd(other) { 1 } else { 2 };
                    if ts.len() != want {
                        return bail("pinch fan rotation ill-defined");
                    }
                }
                let bnd_others: Vec<u32> = at_v.keys().copied().filter(|&o| is_bnd(o)).collect();
                let mut paired: BTreeSet<u32> = BTreeSet::new();
                for &start_o in &bnd_others {
                    if paired.contains(&start_o) {
                        continue;
                    }
                    // Rotate: current (edge-other, tri) -> tri's third at-v edge.
                    let mut cur_o = start_o;
                    let mut cur_t = at_v[&start_o][0];
                    let mut hops = 0usize;
                    let end_o = loop {
                        hops += 1;
                        if hops > 2 * at_v.len() + 4 {
                            return bail("pinch fan chain does not terminate");
                        }
                        let tri = mesh.tris[cur_t];
                        let pos = tri.iter().position(|&g| g == gv).expect("has v");
                        let (o1, o2) = (tri[(pos + 1) % 3], tri[(pos + 2) % 3]);
                        let next_o = if o1 == cur_o { o2 } else { o1 };
                        if is_bnd(next_o) {
                            break next_o;
                        }
                        let ts = &at_v[&next_o];
                        let next_t = if ts[0] == cur_t { ts[1] } else { ts[0] };
                        cur_o = next_o;
                        cur_t = next_t;
                    };
                    if end_o == start_o || paired.contains(&end_o) {
                        return bail("pinch fan chain closed on itself");
                    }
                    paired.insert(start_o);
                    paired.insert(end_o);
                    let (ls, le) = (local_of_global[&start_o], local_of_global[&end_o]);
                    pinch_pair.insert((v, ls), le);
                    pinch_pair.insert((v, le), ls);
                }
                if paired.len() != bnd_others.len() {
                    return bail("pinch pairing incomplete");
                }
            }
            let mut loops_local: Vec<Vec<u32>> = Vec::new();
            let mut used: BTreeSet<(u32, u32)> = BTreeSet::new(); // undirected, (min,max)
            let bnd_local: Vec<(u32, u32)> = boundary
                .iter()
                .map(|&(u, w)| (local_of_global[&u], local_of_global[&w]))
                .collect();
            let continuation = |prev: u32, cur: u32| -> Option<u32> {
                let nb = &bnd_adj[&cur];
                if nb.len() == 2 {
                    Some(if nb[0] == prev { nb[1] } else { nb[0] })
                } else {
                    pinch_pair.get(&(cur, prev)).copied()
                }
            };
            for &(su, sw) in &bnd_local {
                if used.contains(&(su.min(sw), su.max(sw))) {
                    continue;
                }
                let mut lp = vec![su];
                let (mut prev, mut cur) = (su, sw);
                used.insert((su.min(sw), su.max(sw)));
                let mut steps = 0usize;
                loop {
                    steps += 1;
                    if steps > 2 * bnd_local.len() + 4 {
                        return bail("boundary walk does not close");
                    }
                    let Some(next) = continuation(prev, cur) else {
                        return bail("pinch pairing missing");
                    };
                    if (cur, next) == (su, sw) {
                        break; // closed: back at the starting directed edge
                    }
                    lp.push(cur);
                    if !used.insert((cur.min(next), cur.max(next))) {
                        return bail("boundary walk re-traverses an edge");
                    }
                    prev = cur;
                    cur = next;
                }
                loops_local.push(lp);
            }
            // inc-4c-2: 2D self-crossing scan over this component's boundary
            // loops. A crossing means a seam chain bounding the region is
            // disordered but not (yet) rewireable — typically its other side's
            // face has no triangles in the regions. Grow the anchors by the
            // crossing vertices so that face joins the regions next round and
            // the chain canonicalization can reorder it.
            {
                let mut loop_edges: Vec<(u32, u32)> = Vec::new();
                for lp in &loops_local {
                    let m = lp.len();
                    for i in 0..m {
                        loop_edges.push((lp[i], lp[(i + 1) % m]));
                    }
                }
                let mut grow_verts: BTreeSet<u32> = BTreeSet::new();
                for i in 0..loop_edges.len() {
                    for j in (i + 1)..loop_edges.len() {
                        let (a, b) = loop_edges[i];
                        let (c, d) = loop_edges[j];
                        if a == c || a == d || b == c || b == d {
                            continue;
                        }
                        let (pa, pb, pc, pd) = (
                            verts2d[a as usize],
                            verts2d[b as usize],
                            verts2d[c as usize],
                            verts2d[d as usize],
                        );
                        let cr = |o: cad_primitives::Point2,
                                  p: cad_primitives::Point2,
                                  q: cad_primitives::Point2| {
                            (p.x() - o.x()) * (q.y() - o.y()) - (p.y() - o.y()) * (q.x() - o.x())
                        };
                        let (d1, d2) = (cr(pc, pd, pa), cr(pc, pd, pb));
                        let (d3, d4) = (cr(pa, pb, pc), cr(pa, pb, pd));
                        if (d1 > 0.0) != (d2 > 0.0) && (d3 > 0.0) != (d4 > 0.0) {
                            for &l in &[a, b, c, d] {
                                grow_verts.insert(global_of_local[l as usize]);
                            }
                        }
                    }
                }
                if !grow_verts.is_empty() {
                    grow_rounds += 1;
                    if grow_rounds > 16 {
                        return bail("seam disorder growth bound exceeded");
                    }
                    if probe {
                        eprintln!(
                            "[p3b-fanfix] region {key:?} boundary self-crosses — growing \
                         anchors by {grow_verts:?} (round {grow_rounds})"
                        );
                    }
                    anchors.extend(grow_verts);
                    continue 'grow;
                }
            }
            let signed_area = |lp: &[u32]| -> f64 {
                let mut a2 = 0.0;
                for i in 0..lp.len() {
                    let p = verts2d[lp[i] as usize];
                    let q = verts2d[lp[(i + 1) % lp.len()] as usize];
                    a2 += p.x() * q.y() - q.x() * p.y();
                }
                a2 * 0.5
            };
            let outer_i = (0..loops_local.len()).max_by(|&x, &y| {
                signed_area(&loops_local[x])
                    .abs()
                    .partial_cmp(&signed_area(&loops_local[y]).abs())
                    .unwrap_or(std::cmp::Ordering::Equal)
            })?;
            let outer = loops_local[outer_i].clone();
            let holes: Vec<Vec<u32>> = loops_local
                .iter()
                .enumerate()
                .filter(|(i, _)| *i != outer_i)
                .map(|(_, l)| l.clone())
                .collect();
            let on_loop: BTreeSet<u32> = loops_local.iter().flatten().copied().collect();
            let interior: Vec<u32> = (0..verts2d.len() as u32)
                .filter(|l| !on_loop.contains(l))
                .collect();
            let tris_local = match cherchi_rs::cdt_polygon_with_holes_keep_interior(
                &verts2d, &outer, &holes, &interior,
            ) {
                Ok(t) => t,
                Err(e) => {
                    if probe {
                        eprintln!(
                        "[p3b-fanfix] region {key:?} CDT error {e:?}: outer {} holes {} interior {} \
                         loop_verts {:?}",
                        outer.len(),
                        holes.len(),
                        interior.len(),
                        loops_local
                            .iter()
                            .map(|lp| lp.iter().map(|&l| global_of_local[l as usize]).collect::<Vec<_>>())
                            .collect::<Vec<_>>()
                    );
                        for lp in &loops_local {
                            for &l in lp {
                                let p = verts2d[l as usize];
                                eprintln!(
                                    "[p3b-fanfix]   v{} 2d=({:.9},{:.9})",
                                    global_of_local[l as usize],
                                    p.x(),
                                    p.y()
                                );
                            }
                        }
                    }
                    return bail("keep-interior CDT failed");
                }
            };
            // Winding: align to the region's pre-repair net orientation,
            // measured against the SURFACE's own normal field at each
            // triangle's centroid (`surface_normal_at` — the gradient, every
            // surface type the chart admits). P0059 (2026-10-13): the previous
            // per-type reference direction had Plane and Cylinder arms only
            // and read ZERO on a cone, so both rebuilt cone regions kept the
            // 2D CDT's chart winding — against the cone — and Stage 6's
            // boundary walk met two edges head-on at the next ridge vertex.
            // A reference the surface cannot supply (an undefined gradient, or
            // a region whose net orientation is exactly zero) is a bail, never
            // an unoriented emission.
            let centroid = |g: &[u32; 3]| -> cad_primitives::Point3 {
                cad_primitives::Point3::new(
                    (mesh.verts[g[0] as usize].x()
                        + mesh.verts[g[1] as usize].x()
                        + mesh.verts[g[2] as usize].x())
                        / 3.0,
                    (mesh.verts[g[0] as usize].y()
                        + mesh.verts[g[1] as usize].y()
                        + mesh.verts[g[2] as usize].y())
                        / 3.0,
                    (mesh.verts[g[0] as usize].z()
                        + mesh.verts[g[1] as usize].z()
                        + mesh.verts[g[2] as usize].z())
                        / 3.0,
                )
            };
            let oriented_area = |g: &[u32; 3]| -> Option<f64> {
                let av = tri_area_vector(
                    mesh.verts[g[0] as usize].as_array(),
                    mesh.verts[g[1] as usize].as_array(),
                    mesh.verts[g[2] as usize].as_array(),
                );
                let d = crate::geom::surface_normal_at(surf, centroid(g))?;
                Some(av[0] * d[0] + av[1] * d[1] + av[2] * d[2])
            };
            let mut ref_sign = 0.0f64;
            let (mut pre_pos, mut pre_neg) = (0usize, 0usize);
            for &ti in rtris {
                let Some(s) = oriented_area(&mesh.tris[ti]) else {
                    return bail("region orientation reference undefined (surface normal)");
                };
                ref_sign += s;
                if s >= 0.0 {
                    pre_pos += 1;
                } else {
                    pre_neg += 1;
                }
            }
            if ref_sign == 0.0 {
                return bail("region has no net orientation");
            }
            let ref_sign = if ref_sign > 0.0 { 1.0 } else { -1.0 };
            let mut new_tris: Vec<[u32; 3]> = Vec::with_capacity(tris_local.len());
            let mut flipped = 0usize;
            for tl in &tris_local {
                let mut g = [
                    global_of_local[tl[0] as usize],
                    global_of_local[tl[1] as usize],
                    global_of_local[tl[2] as usize],
                ];
                let Some(s) = oriented_area(&g) else {
                    return bail("new triangle orientation undefined (surface normal)");
                };
                if s == 0.0 {
                    return bail("new triangle is tangent-degenerate against its surface");
                }
                if s * ref_sign < 0.0 {
                    g.swap(1, 2);
                    flipped += 1;
                }
                new_tris.push(g);
            }
            if probe {
                let kind = match surf {
                    Surface::Plane { .. } => "plane",
                    Surface::Cylinder { .. } => "cylinder",
                    Surface::Cone { .. } => "cone",
                    Surface::Sphere { .. } => "sphere",
                    Surface::Torus { .. } => "torus",
                };
                eprintln!(
                    "[p3b-fanfix] region {key:?} {kind}: pre-repair tris vs surface normal \
                     +{pre_pos}/-{pre_neg} (net sign {ref_sign:+.0}); {} CDT tris emitted, \
                     {flipped} flipped to match",
                    new_tris.len()
                );
            }
            plans.push(RegionPlan {
                key,
                boundary,
                new_tris,
            });
        }

        break (plans, in_regions, removed_chain, added_chain);
    };
    // Splice all regions at once; the postcondition below runs on the
    // CANDIDATE triangle list before any mutation, so a violation simply
    // bails with the mesh untouched.
    let mut next_tris: Vec<[u32; 3]> = Vec::with_capacity(mesh.tris.len());
    let mut next_attr: Vec<Option<TriangleAttribution>> = Vec::with_capacity(attr_vec.len());
    for ti in 0..mesh.tris.len() {
        if !in_regions.contains(&ti) {
            next_tris.push(mesh.tris[ti]);
            next_attr.push(attr_vec.get(ti).copied().flatten());
        }
    }
    let mut new_edge_set: BTreeSet<(u32, u32)> = BTreeSet::new();
    for plan in &plans {
        let at = TriangleAttribution {
            input: plan.key.0,
            face: plan.key.1,
        };
        for &g in &plan.new_tris {
            for (i, j) in [(0usize, 1usize), (1, 2), (2, 0)] {
                let (u, w) = (g[i], g[j]);
                new_edge_set.insert(if u < w { (u, w) } else { (w, u) });
            }
            next_tris.push(g);
            next_attr.push(Some(at));
        }
    }
    // Postcondition. Expected total multiplicity per edge:
    // - an edge on >=1 region boundary: (# untouched outside triangles) + 1
    //   per bounding region (each keep-boundary CDT emits its loop edges
    //   exactly once) — in a closed production mesh that is 2; the general
    //   form also holds at pre-existing mesh boundaries;
    // - any other new-triangle edge (an interior CDT chord): exactly 2.
    // Every formerly-defective cluster edge must end at its expected count or
    // vanish entirely. Any violation bails with the mesh untouched.
    let mut post_use: BTreeMap<(u32, u32), usize> = BTreeMap::new();
    for tri in &next_tris {
        for (i, j) in [(0usize, 1usize), (1, 2), (2, 0)] {
            let (u, w) = (tri[i], tri[j]);
            let e = if u < w { (u, w) } else { (w, u) };
            *post_use.entry(e).or_default() += 1;
        }
    }
    let expected = |e: &(u32, u32)| -> usize {
        let bcount = plans.iter().filter(|p| p.boundary.contains(e)).count();
        if bcount > 0 {
            let outside = edge_use
                .get(e)
                .map(|ts| ts.iter().filter(|t| !in_regions.contains(t)).count())
                .unwrap_or(0);
            outside + bcount
        } else {
            2
        }
    };
    for &e in &new_edge_set {
        if post_use.get(&e).copied().unwrap_or(0) != expected(&e) {
            return bail("postcondition: new edge off its expected multiplicity");
        }
    }
    // Orientation (P0059): an undirected count cannot see a region emitted
    // wound against its neighbours — the stacked edge still totals 2. Every
    // edge of a new triangle must be traversed at most once in each
    // direction across the candidate mesh (the manifold-orientation
    // condition; at a pre-existing mesh boundary the single traversal is
    // whichever sense the one triangle has).
    {
        let mut directed: BTreeMap<(u32, u32), usize> = BTreeMap::new();
        for tri in &next_tris {
            for (i, j) in [(0usize, 1usize), (1, 2), (2, 0)] {
                let (u, w) = (tri[i], tri[j]);
                let e = if u < w { (u, w) } else { (w, u) };
                if new_edge_set.contains(&e) {
                    *directed.entry((u, w)).or_default() += 1;
                }
            }
        }
        if directed.values().any(|&n| n > 1) {
            return bail("postcondition: new triangle edge traversed twice in one direction");
        }
    }
    for (&(u, w), ts) in edge_use {
        if ts.len() != 2 && (cluster.contains(&u) || cluster.contains(&w)) {
            let n = post_use.get(&(u, w)).copied().unwrap_or(0);
            if n != 0 && n != expected(&(u, w)) {
                return bail("postcondition: defective edge not resolved");
            }
        }
    }
    // inc-4c-2: every removed chain edge either vanished or persists as an
    // ordinary manifold chord; every rewritten chain edge is claimed by
    // exactly TWO plans (one triangle per side — a chain constrained on one
    // side only would leave an unpaired seam).
    for &e in &removed_chain {
        let n = post_use.get(&e).copied().unwrap_or(0);
        if n != 0 && n != 2 {
            return bail("postcondition: removed chain edge unresolved");
        }
    }
    for &(_, e) in &added_chain {
        let claims = plans.iter().filter(|p| p.boundary.contains(&e)).count();
        if claims != 2 {
            return bail("postcondition: rewritten chain edge not two-sided");
        }
        if post_use.get(&e).copied().unwrap_or(0) != 2 {
            return bail("postcondition: rewritten chain edge not use-2");
        }
    }
    // The repair may never mint a RENDER-DEGENERATE triangle (height below
    // the render channel's resolution — the assay `no_degenerate_triangles`
    // criterion): the re-CDT of a chain still carrying sub-render needle
    // verts can be forced into such a sliver, which would ship as a
    // silent-wrong. Fail closed instead (the loud STOP stands); the §4.3
    // sub-render sample cleanup is its own increment.
    {
        let max_abs = mesh
            .verts
            .iter()
            .flat_map(|p| p.as_array())
            .fold(0.0f64, |m, c| m.max(c.abs()));
        let height_floor = 4.0 * max_abs * (f32::EPSILON as f64);
        for plan in &plans {
            for g in &plan.new_tris {
                let (a, b, c) = (
                    mesh.verts[g[0] as usize].as_array(),
                    mesh.verts[g[1] as usize].as_array(),
                    mesh.verts[g[2] as usize].as_array(),
                );
                let av = tri_area_vector(a, b, c);
                let area = (av[0] * av[0] + av[1] * av[1] + av[2] * av[2]).sqrt() / 2.0;
                let d2 = |p: [f64; 3], q: [f64; 3]| {
                    (p[0] - q[0]).powi(2) + (p[1] - q[1]).powi(2) + (p[2] - q[2]).powi(2)
                };
                let longest = d2(a, b).max(d2(b, c)).max(d2(c, a)).sqrt();
                if longest <= 0.0 || 2.0 * area / longest < height_floor {
                    if probe {
                        eprintln!(
                            "[p3b-fanfix] render-degenerate new tri {g:?} \
                             height {:.3e} < floor {height_floor:.3e}",
                            if longest > 0.0 {
                                2.0 * area / longest
                            } else {
                                0.0
                            }
                        );
                    }
                    return bail("postcondition: repair minted a render-degenerate triangle");
                }
            }
        }
    }
    *mesh = Mesh::new(std::mem::take(&mut mesh.verts), next_tris);
    *attr_vec = next_attr;
    Some(())
}

/// How many DISTINCT analytic surfaces `pos` lies on, counted over the faces
/// carried by the triangles incident to `vi` — the KV15b I1b "richness" measure
/// (spec `kv15b_mint_site_subresolution_collapse` §I1b-curved).
///
/// Incidence is certified at [`junction_certificate_band`], the same band that
/// certifies a Stage-4 exact junction, so a chord-level (un-relocated) curved
/// sample contributes nothing: the count is strictly richer only for positions
/// Stage 4 actually placed ON the surface. Counting EVERY surface, not planes
/// alone, is the 2026-08-19 R0047 amendment.
///
/// Richness ranks two candidate positions for one model point: the richer one
/// carries more analytic authority, so it is the one a merge must keep — merging
/// into the poorer one evicts a face-loop vertex off a surface it lies on.
pub(crate) fn surface_incidence_count(
    mesh: &Mesh,
    attribution: &[Option<TriangleAttribution>],
    a: &BRep,
    b: &BRep,
    vi: u32,
    pos: [f64; 3],
) -> usize {
    carried_surfaces(mesh, attribution, a, b, vi, pos).len()
}

/// The DISTINCT analytic surfaces `pos` lies on, over the faces carried by the
/// triangles incident to `vi` — [`surface_incidence_count`]'s underlying set.
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
pub(crate) fn collapse_subresolution_intersection_segments(
    mesh: &mut Mesh,
    attribution: &mut Vec<Option<TriangleAttribution>>,
    intersection_curves: &std::collections::BTreeMap<(u32, u32), Curve>,
    a: &BRep,
    b: &BRep,
) -> bool {
    let mut redirect: std::collections::BTreeMap<u32, u32> = std::collections::BTreeMap::new();
    fn resolve(redirect: &std::collections::BTreeMap<u32, u32>, mut v: u32) -> u32 {
        while let Some(&n) = redirect.get(&v) {
            v = n;
        }
        v
    }
    // C0036 amendment (spec `kv15b_mint_site_subresolution_collapse` I1b):
    // the surviving POSITION is the pair's SURFACE-incidence-richer endpoint.
    // A sub-floor pair often joins the TRUE junction of k carried surfaces
    // with a near-degenerate crossing OFF one of them by the sub-floor gap
    // (the C0036 near-coplanar seam corner: the exact 3-plane corner vs a
    // crossing 1.75e-8 off the tilted wall). Keeping the min-index position
    // blindly evicts a face-loop vertex off its carried analytic surface,
    // twisting the loop (the fitted Newell then misses the exact input
    // corners — the debug-tier NonPlanarFace red). The topological survivor
    // stays min-index (I1 determinism); only its COORDINATES may adopt the
    // strictly richer endpoint. Ties keep the survivor's own coordinates
    // (byte-identical to the shipped behavior).
    //
    // 2026-08-19 (R0047 anchor, spec §I1b-curved): the measure counts EVERY
    // distinct analytic surface the endpoint lies on (within the same
    // `junction_certificate_band` that certifies Stage-4 exact junctions),
    // not planes alone. The planar-only count read a certified
    // plane∩cone∩cone crease junction (3 surfaces) and its cone∩plane
    // interior neighbour (2 surfaces) as a 1–1 TIE, kept the neighbour's
    // coordinates, and emitted the merged vertex ON cone-1's ellipse but
    // 1.4e-9 OFF cone-2's — kernel-v2's "output ellipse-arc endpoint does
    // not lie on its ellipse". Chord-level (un-relocated) curved samples are
    // NOT on their surface at certificate precision, so they contribute
    // nothing — the count is strictly richer only for positions Stage 4
    // actually placed on the surface.
    let mut any = false;
    for &(u, v) in intersection_curves.keys() {
        let (ru, rv) = (resolve(&redirect, u), resolve(&redirect, v));
        if ru == rv {
            continue;
        }
        let p = mesh.verts[ru as usize].as_array();
        let q = mesh.verts[rv as usize].as_array();
        let d2 = (p[0] - q[0]).powi(2) + (p[1] - q[1]).powi(2) + (p[2] - q[2]).powi(2);
        // #169 N56: scale-relative sub-resolution band `TAU_MODEL·(1+scale)`
        // (was the absolute `TAU_MODEL²` floor N53 flagged). Both endpoints lie
        // on the intersection curve, so an edge below the model-coincidence
        // resolution is a redundant curve sample — Yang §4.3 "remove a point too
        // close to another on the same loop." Scale-relative because a fixed
        // gap is numerical noise at large coordinates; the SAME band coincident
        // and the stage-5 planarity wall use. Measured collapses are ~1e-8…1e-7
        // (genuinely sub-resolution); recovers R0076/R0088/F0078/F0079/F0084.
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
        // I1b, read as §4-I8 CONTAINMENT (P0060, 2026-10-13): a collapse may
        // IDENTIFY two positions only when one endpoint's carried-surface set
        // contains the other's — the poorer endpoint is then a sample of the
        // richer one's junction, and the survivor adopts the richer position.
        // Two sets neither of which contains the other are two DISTINCT exact
        // junctions (P0060: a prism corner {f1, f5, f6} poking 1.47e-8 through
        // the partner's cap plane mints three triple points {f1, f6, cap},
        // {f5, f6, cap}, {f1, f5, cap} 1.9e-8 … 3.4e-8 apart); no single
        // position lies on their union of four planes, and the count-only
        // "tie" rule kept one of them, evicting the face loop of the fourth
        // plane off its surface by 2.4e-8 — kernel-v2's NonPlanarFace. Such a
        // pair is NOT a redundant curve sample (Yang §4.3's rule is for points
        // on ONE loop): the segment stays, exact, and the output carries the
        // sub-resolution feature the model actually has.
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
                mesh.verts[survivor as usize] = mesh.verts[victim as usize];
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
        collapse_vertex(mesh, attribution, victim, survivor);
        redirect.insert(victim, survivor);
        any = true;
    }
    any
}

/// #194 (spec `yang_194_subtauwork_edge_collapse`): collapse mesh EDGES
/// shorter than working precision before Phase-B emission.
///
/// The exact arrangement can mint the SAME junction twice with swapped LPI
/// roles when an operand's own tessellation self-grazes (the F0082
/// Extrude-12 seal corner: two verts 5.5e-14 apart joined by a mesh edge,
/// spawning a zero-area flap whose third edge use is the χ=3 book edge).
/// Nothing existing owns the pair: the I6/KV15 near-weld excludes
/// curved-incident verts (the KV9 lens-tip record), KV15b is
/// provenance-restricted to `intersection_curves` keys (A×B junctions), and
/// Stage-4's KV9 collapse reconciles only this op's curve junctions.
///
/// Domain = ALL undirected mesh edges (deterministic `BTreeSet` order); the
/// band does the scoping: resolved length in the OPEN interval
/// `(0, TAU_WORK·(1+scale))` — five orders TIGHTER than KV15b. An edge
/// below working precision is not a representable segment; collapsing it is
/// not proximity welding. Min-resolved-index survivor keeps its own bits
/// (I1); resolved re-measure prevents chain drift (I2/B5); exact-zero edges
/// are the M-B identification class and stay (B3). KV9's UNCONNECTED ring
/// duplicates are untouched by construction — no edge joins them.
pub(crate) fn collapse_subtauwork_mesh_edges(
    mesh: &mut Mesh,
    attribution: &mut Vec<Option<TriangleAttribution>>,
) -> bool {
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
        collapse_vertex(mesh, attribution, victim, survivor);
        redirect.insert(victim, survivor);
        any = true;
    }
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
pub(crate) fn compact_unreferenced_verts(
    mesh: &mut Mesh,
    relocations: &mut Vec<(u32, f64)>,
) -> Option<Vec<Option<u32>>> {
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
    // Remap (and drop) relocation keys: a relocation referencing a collapsed-away
    // (now-unreferenced) vertex is no longer in the mesh, so it is dropped.
    let remapped: Vec<(u32, f64)> = relocations
        .iter()
        .filter_map(|&(v, t)| remap[v as usize].map(|nv| (nv, t)))
        .collect();
    *relocations = remapped;
    Some(remap)
}

#[cfg(test)]
mod mutual_pair_tests {
    use super::*;

    /// The §5c.11 "pillow" fixture: a closed, coherently wound 8-triangle
    /// surface whose equator carries a mutual degenerate pair — T1=[c,bl,a] and
    /// T2=[bh,c,a] astride the shared long edge (a,c), off-vertices bl/bh
    /// interleaved strictly inside the segment (t = 0.25 / 0.6, off-line height
    /// 1e-13 → areas 5e-14 < MIN_FEATURE_SIZE²).
    fn pillow() -> (Mesh, Vec<Option<TriangleAttribution>>, MutualPair) {
        let d = 1.0e-13;
        let verts = vec![
            Point3::new(0.0, 0.0, 0.0),  // 0 = a
            Point3::new(1.0, 0.0, 0.0),  // 1 = c
            Point3::new(0.25, d, 0.0),   // 2 = bl
            Point3::new(0.6, -d, 0.0),   // 3 = bh
            Point3::new(0.5, 1.0, 0.0),  // 4 = x (NL apex)
            Point3::new(0.5, -1.0, 0.0), // 5 = y (NH apex)
        ];
        let tris = vec![
            [0, 2, 4], // 0 M1
            [1, 4, 2], // 1 NL (outer across (bl,c))
            [1, 2, 0], // 2 T1 (degenerate, off = bl)
            [3, 1, 0], // 3 T2 (degenerate, off = bh)
            [3, 0, 5], // 4 NH (outer across (a,bh))
            [1, 3, 5], // 5 M2
            [5, 0, 4], // 6 E1
            [4, 1, 5], // 7 E2
        ];
        let mut attrs: Vec<Option<TriangleAttribution>> = vec![None; 8];
        attrs[1] = Some(TriangleAttribution {
            input: InputId::A,
            face: 7,
        });
        attrs[4] = Some(TriangleAttribution {
            input: InputId::B,
            face: 3,
        });
        let m = MutualPair {
            t1: 2,
            t2: 3,
            nl: 1,
            nh: 4,
            a: 0,
            c: 1,
            bl: 2,
            bh: 3,
        };
        (Mesh::new(verts, tris), attrs, m)
    }

    fn directed_edge_census(tris: &[[u32; 3]]) -> std::collections::HashMap<(u32, u32), usize> {
        let mut m = std::collections::HashMap::new();
        for t in tris {
            for (i, j) in [(0, 1), (1, 2), (2, 0)] {
                *m.entry((t[i], t[j])).or_insert(0) += 1;
            }
        }
        m
    }

    fn assert_closed_coherent(tris: &[[u32; 3]]) {
        let m = directed_edge_census(tris);
        for (&(u, v), &n) in &m {
            assert_eq!(n, 1, "directed edge ({u},{v}) used {n} times");
            assert_eq!(m.get(&(v, u)), Some(&1), "edge ({u},{v}) has no reverse");
        }
    }

    fn is_degen(mesh: &Mesh, t: [u32; 3]) -> bool {
        tri_is_degenerate(
            mesh.verts[t[0] as usize].as_array(),
            mesh.verts[t[1] as usize].as_array(),
            mesh.verts[t[2] as usize].as_array(),
        )
    }

    fn edge_incidence(tris: &[[u32; 3]]) -> std::collections::HashMap<(u32, u32), Vec<u32>> {
        let mut m: std::collections::HashMap<(u32, u32), Vec<u32>> =
            std::collections::HashMap::new();
        for (ti, tri) in tris.iter().enumerate() {
            for (i, j) in [(0, 1), (1, 2), (2, 0)] {
                let (u, v) = (tri[i], tri[j]);
                let key = if u < v { (u, v) } else { (v, u) };
                m.entry(key).or_default().push(ti as u32);
            }
        }
        m
    }

    #[test]
    fn pillow_fixture_is_closed_and_carries_the_pair() {
        let (mesh, _, m) = pillow();
        assert_closed_coherent(&mesh.tris);
        assert!(is_degen(&mesh, mesh.tris[m.t1]));
        assert!(is_degen(&mesh, mesh.tris[m.t2]));
    }

    #[test]
    fn resolve_drops_the_quad_and_stays_watertight_with_the_fine_chain() {
        let (mut mesh, mut attrs, m) = pillow();
        resolve_mutual_degenerate_pair(&mut mesh, &mut attrs, &m);
        assert_eq!(mesh.tris.len(), 8);
        assert_eq!(attrs.len(), 8);
        assert_closed_coherent(&mesh.tris);
        for &t in &mesh.tris {
            assert!(!is_degen(&mesh, t), "degenerate tri {t:?} survived");
        }
        // The long edge is gone; both sides carry the fine chain a–bl–bh–c.
        let und = edge_incidence(&mesh.tris);
        assert!(!und.contains_key(&(0, 1)), "long edge (a,c) survived");
        for k in [(0, 2), (2, 3), (1, 3)] {
            assert_eq!(
                und.get(&k).map(Vec::len),
                Some(2),
                "chain edge {k:?} not paired"
            );
        }
        // Split pieces inherit their parents' attributions.
        let n_a7 = attrs
            .iter()
            .flatten()
            .filter(|at| at.input == InputId::A && at.face == 7)
            .count();
        let n_b3 = attrs
            .iter()
            .flatten()
            .filter(|at| at.input == InputId::B && at.face == 3)
            .count();
        assert_eq!((n_a7, n_b3), (2, 2));
    }

    #[test]
    fn candidate_accepts_the_pair_and_rejects_equal_parameters() {
        let (mesh, _, want) = pillow();
        let edge_tris = edge_incidence(&mesh.tris);
        let is_degen = |ti: usize, mesh: &Mesh| is_degen(mesh, mesh.tris[ti]);
        let long_edge_off = |t: &[u32; 3], mesh: &Mesh| -> (u32, u32, u32) {
            let d = |i: usize, j: usize| {
                let p = mesh.verts[t[i] as usize].as_array();
                let q = mesh.verts[t[j] as usize].as_array();
                (p[0] - q[0]).powi(2) + (p[1] - q[1]).powi(2) + (p[2] - q[2]).powi(2)
            };
            let (e01, e12, e20) = (d(0, 1), d(1, 2), d(2, 0));
            if e01 >= e12 && e01 >= e20 {
                (t[0], t[1], t[2])
            } else if e12 >= e20 {
                (t[1], t[2], t[0])
            } else {
                (t[2], t[0], t[1])
            }
        };
        let (a, c, b) = long_edge_off(&mesh.tris[2], &mesh);
        let got =
            mutual_pair_candidate(&mesh, &edge_tris, &is_degen, &long_edge_off, 2, 3, a, c, b)
                .expect("mutual pair accepted");
        assert_eq!((got.t1, got.t2, got.nl, got.nh), (2, 3, want.nl, want.nh));
        assert_eq!((got.bl, got.bh), (want.bl, want.bh));
        // Same parameter along a→c (no deterministic chain order) → rejected.
        let mut mesh2 = pillow().0;
        mesh2.verts[2] = Point3::new(0.6, 1.0e-13, 0.0);
        let (a2, c2, b2) = long_edge_off(&mesh2.tris[2], &mesh2);
        assert!(mutual_pair_candidate(
            &mesh2,
            &edge_tris,
            &is_degen,
            &long_edge_off,
            2,
            3,
            a2,
            c2,
            b2
        )
        .is_none());
    }

    /// The SAME-APEX fan (measured on R0038): both outer neighbours across the
    /// two insertion edges share their third vertex, so they are one fan over
    /// the chain rather than two opposite sides. `nl`'s piece `[bl,bh,dd]` and
    /// `nh`'s piece `[bl,bh,dd]` are then the identical triangle and the update
    /// would emit it TWICE — a double cover. The candidate must reject, keeping
    /// the loud STOP. The second half proves the rejection is caused by the
    /// shared apex specifically: give `nh` its own apex and the SAME
    /// configuration is accepted.
    #[test]
    fn candidate_rejects_the_same_apex_fan() {
        let is_degen = |ti: usize, mesh: &Mesh| is_degen(mesh, mesh.tris[ti]);
        let long_edge_off = |t: &[u32; 3], mesh: &Mesh| -> (u32, u32, u32) {
            let d = |i: usize, j: usize| {
                let p = mesh.verts[t[i] as usize].as_array();
                let q = mesh.verts[t[j] as usize].as_array();
                (p[0] - q[0]).powi(2) + (p[1] - q[1]).powi(2) + (p[2] - q[2]).powi(2)
            };
            let (e01, e12, e20) = (d(0, 1), d(1, 2), d(2, 0));
            if e01 >= e12 && e01 >= e20 {
                (t[0], t[1], t[2])
            } else if e12 >= e20 {
                (t[1], t[2], t[0])
            } else {
                (t[2], t[0], t[1])
            }
        };
        // Chain a–bl–bh–c collinear (t = 0.41 / 0.68, off-line height 1e-13 so
        // both quad members are degenerate), apex 4 shared by BOTH neighbours.
        let dy = 1.0e-13;
        let verts = vec![
            Point3::new(0.0, 0.0, 0.0),  // 0 = a
            Point3::new(1.0, 0.0, 0.0),  // 1 = c
            Point3::new(0.41, dy, 0.0),  // 2 = bl
            Point3::new(0.68, -dy, 0.0), // 3 = bh
            Point3::new(0.5, 1.0, 0.0),  // 4 = shared apex
            Point3::new(0.5, -1.0, 0.0), // 5 = distinct apex (two-sided case)
        ];
        let same_apex = vec![
            [1, 2, 0], // 0 T1 degenerate, off = bl
            [3, 1, 0], // 1 T2 degenerate, off = bh
            [1, 4, 2], // 2 NL across (bl,c), third = 4
            [3, 0, 4], // 3 NH across (a,bh), third = 4  ← same apex
        ];
        let mesh = Mesh::new(verts.clone(), same_apex);
        let edge_tris = edge_incidence(&mesh.tris);
        let (a, c, b) = long_edge_off(&mesh.tris[0], &mesh);
        assert!(
            is_degen(0, &mesh) && is_degen(1, &mesh),
            "quad is degenerate"
        );
        assert!(
            !is_degen(2, &mesh) && !is_degen(3, &mesh),
            "both outer neighbours are non-degenerate"
        );
        assert!(
            mutual_pair_candidate(&mesh, &edge_tris, &is_degen, &long_edge_off, 0, 1, a, c, b)
                .is_none(),
            "same-apex fan must be rejected — the arm would double-cover [bl,bh,dd]"
        );

        // Identical configuration, `nh` re-apexed to its own vertex: accepted,
        // and the four split pieces are then distinct.
        let two_sided = vec![
            [1, 2, 0], // T1
            [3, 1, 0], // T2
            [1, 4, 2], // NL, third = 4
            [3, 0, 5], // NH, third = 5  ← distinct
        ];
        let mesh2 = Mesh::new(verts, two_sided);
        let edge_tris2 = edge_incidence(&mesh2.tris);
        let (a2, c2, b2) = long_edge_off(&mesh2.tris[0], &mesh2);
        let got = mutual_pair_candidate(
            &mesh2,
            &edge_tris2,
            &is_degen,
            &long_edge_off,
            0,
            1,
            a2,
            c2,
            b2,
        )
        .expect("two-sided pair still accepted");
        assert_eq!((got.nl, got.nh), (2, 3));
        assert_eq!((got.bl, got.bh), (2, 3));
    }
}
