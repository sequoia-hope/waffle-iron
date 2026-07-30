//! Stage-0 minted-vertex relocation (M8 increments 8–11): exact
//! ear-clip cavity repair, region relocation, ring-crossing seed, and
//! reloc_tests (extracted verbatim from stage0/mod.rs — spec
//! `specs/stage0_decomposition.md`, increment 6).

#[allow(clippy::wildcard_imports)]
use super::*;

/// Outcome of a per-vertex cavity relocation attempt (amendments 5/6).
pub(crate) enum RelocOutcome {
    /// The cavity was re-triangulated and committed.
    Committed,
    /// Rejected for a reason joint relocation cannot help with; no mutation.
    Rejected,
    /// Amendment 6: the cavity polygon was exactly NON-SIMPLE — the classic
    /// interacting-mints signature (another minted vertex's collapsed spokes
    /// cross the ring). `ring_mints` are the OTHER minted vertices on the
    /// CROSSING edges (amendment 10: the interacting set per Fig-11
    /// locality — NOT every mint on the ring; a hole-encircling ring lists
    /// dozens of mints and seeding them all inflates the joint region into
    /// an annulus). `merge_candidate` is the amendment-13 BACKTRACK pair
    /// (unminted p, minted q) when the polygon's first crossing carries the
    /// Fig-11(b→c) signature — the SINGLETON NonSimple class (empty
    /// `ring_mints`) never reaches the joint form, so the ladder merges
    /// directly from here (measured: R0099 verts 4/9). `split_chord` is
    /// the Fig-11(a) signature: the crossing pairs v's OWN boundary edge
    /// with a link CHORD — the mint pokes past a constrained edge with no
    /// endpoint in merging reach (measured: R0099 vert 9, overshoot
    /// 1.7e-4, endpoints 0.17/0.24 away); the ladder reroutes the chord
    /// through the mint (split-at-existing-vertex = the Lawson flip of
    /// the chord, both products taking the external class). No mutation.
    NonSimple {
        ring_mints: Vec<u32>,
        merge_candidate: Option<(u32, u32, f64, f64)>,
        split_chord: Option<(u32, u32)>,
    },
}

/// Outcome of the joint region relocation (amendments 6–9 + 13).
pub(crate) enum RegionOutcome {
    /// At least one class sub-region was re-triangulated and committed.
    Committed,
    /// Nothing committed; no Fig-11 merge candidate was identified.
    Rejected,
    /// Nothing committed, but a rejecting sub-region's boundary carried
    /// the amendment-13 BACKTRACK signature (spec
    /// `m8_stage0_multiclass_cavity_arm` §10): its two crossing ring
    /// edges sandwich one edge joining an UNMINTED vertex `p` to a
    /// MINTED vertex `q` — the [#24 Yang §4.4.1 Fig 11(b→c)] "endpoint p
    /// of the split edge is too close to q" configuration. The LADDER
    /// decides whether p is mergeable (provenance mask) and performs the
    /// position merge; this form only reports. No mutation.
    MergeCandidate {
        p: u32,
        q: u32,
        overshoot: f64,
        chord_len: f64,
    },
}

/// Reject reason of [`earclip_cavity_polygon`]: exact non-simplicity is
/// distinguished because it is the amendment-6 joint-relocation trigger.
/// `crossing` carries the first crossing pair's endpoint POSITIONS (in the
/// caller's frame projection — bit-identical to `frame.project` of the
/// poly vertices), so the caller can identify the interacting mints.
pub(crate) enum EarclipErr {
    NotSimple { crossing: [(f64, f64); 4] },
    Other(&'static str),
}

/// Shared amendment-5/6 re-triangulation core: exact simplicity + CCW
/// verification on the DEDUPLICATED position ring of `poly`, then
/// constrained exact ear-clipping of the polygon (ears exact-CCW,
/// gate-valid, empty, and a NEW diagonal — one not already carried by a
/// triangle outside `cavity`; ears whose 3D image is bit-degenerate clip
/// freely, the M-B emission-drop class). Pure — mutates nothing; the
/// caller commits. `poly` is the cavity boundary cycle (per-vertex form:
/// `[v, w₀, …, w_k]`; joint form: the region boundary), all positions at
/// their CURRENT resolved coordinates.
#[allow(clippy::too_many_arguments)]
pub(crate) fn earclip_cavity_polygon(
    poly: &[u32],
    cavity: &std::collections::BTreeSet<usize>,
    cls0: RegionClass,
    coords: &[Point3],
    frame: &Frame,
    edge_map: &BTreeMap<[u32; 2], Vec<usize>>,
    probe: bool,
    probe_who: &str,
) -> Result<Vec<([u32; 3], RegionClass)>, EarclipErr> {
    let edge_key = |a: u32, b: u32| if a < b { [a, b] } else { [b, a] };
    let pos = |i: u32| frame.project(coords[i as usize]);

    // Exact simplicity + CCW on the DEDUPLICATED position ring (collapsed
    // sub-floor twins share one resolved position; their zero-length edges
    // cannot cross anything).
    let ring: Vec<(f64, f64)> = {
        let mut r: Vec<(f64, f64)> = Vec::with_capacity(poly.len());
        for &pi in poly {
            let q = pos(pi);
            if r.last() != Some(&q) {
                r.push(q);
            }
        }
        while r.len() > 1 && r.first() == r.last() {
            r.pop();
        }
        r
    };
    if ring.len() < 3 {
        return Err(EarclipErr::Other("degenerate cavity polygon"));
    }
    // Simplicity BEFORE orientation (amendment 11, M8 increment 14): a
    // bow-tie's signed area is lobe-balance noise — a net-CW non-simple
    // ring (measured F0088 vert 674: hair-thin full-height strip whose
    // return edge crosses the up-chain, net 2A = −4.2e-3) must surface as
    // `NotSimple` (the joint-relocation trigger), not die at the
    // orientation guard. Only a SIMPLE ring has a meaningful winding.
    let n = ring.len();
    for a in 0..n {
        for b in (a + 1)..n {
            // Adjacent ring edges share exactly one endpoint — allowed.
            if b == a + 1 || (a == 0 && b == n - 1) {
                continue;
            }
            let (p1, p2) = (ring[a], ring[(a + 1) % n]);
            let (q1, q2) = (ring[b], ring[(b + 1) % n]);
            // Non-adjacent shared position = a pinch.
            if p1 == q1 || p1 == q2 || p2 == q1 || p2 == q2 {
                return Err(EarclipErr::Other(
                    "cavity polygon pinched (repeated position)",
                ));
            }
            let (Some(o1), Some(o2), Some(o3), Some(o4)) = (
                orient_sign_exact(p1, p2, q1),
                orient_sign_exact(p1, p2, q2),
                orient_sign_exact(q1, q2, p1),
                orient_sign_exact(q1, q2, p2),
            ) else {
                return Err(EarclipErr::Other("non-finite cavity polygon"));
            };
            // Proper crossing, or an endpoint on the other segment's
            // INTERIOR. Bare collinearity (o == 0 with the point outside
            // the segment) is NOT an intersection — sweep-event columns
            // legitimately put many ring vertices on one exact vertical
            // line, so rejecting all collinear pairs falsely rejects
            // repairable cavities (measured: F0087 cut 10, vert 186).
            // Endpoint coincidence was excluded above, so on-segment
            // here means strictly interior; collinear-overlapping
            // segments have an endpoint interior to the other and are
            // caught by the same test.
            let within = |o: i8, e1: (f64, f64), e2: (f64, f64), q: (f64, f64)| {
                o == 0
                    && q.0 >= e1.0.min(e2.0)
                    && q.0 <= e1.0.max(e2.0)
                    && q.1 >= e1.1.min(e2.1)
                    && q.1 <= e1.1.max(e2.1)
            };
            if (o1 * o2 < 0 && o3 * o4 < 0)
                || within(o1, p1, p2, q1)
                || within(o2, p1, p2, q2)
                || within(o3, q1, q2, p1)
                || within(o4, q1, q2, p2)
            {
                if probe {
                    eprintln!(
                        "  [reloc-ring] edges {a}:({p1:?}->{p2:?}) x {b}:({q1:?}->{q2:?}) \
                         o=({o1},{o2},{o3},{o4}) ring={ring:?}"
                    );
                }
                return Err(EarclipErr::NotSimple {
                    crossing: [p1, p2, q1, q2],
                });
            }
        }
    }
    {
        use crate::coplanar_overlay::rat;
        let mut two_area = RBig::ZERO;
        for k in 0..ring.len() {
            let (ax, ay) = ring[k];
            let (bx, by) = ring[(k + 1) % ring.len()];
            let Ok(t) = rat(ax).and_then(|axr| Ok(axr * rat(by)? - rat(bx)? * rat(ay)?)) else {
                return Err(EarclipErr::Other("non-finite cavity polygon"));
            };
            two_area += t;
        }
        if two_area <= RBig::ZERO {
            // The ring is SIMPLE (checked above) yet winds CW or is
            // degenerate: a genuinely inside-out cavity — terminal.
            if probe {
                eprintln!(
                    "  [reloc-ccw] {probe_who} two_area {} ring={ring:?}",
                    if two_area == RBig::ZERO {
                        "ZERO"
                    } else {
                        "NEG"
                    }
                );
            }
            return Err(EarclipErr::Other("cavity polygon not CCW"));
        }
    }

    // Constrained ear-clip: deterministic first-clippable-ear order.
    let mut work: Vec<u32> = poly.to_vec();
    let mut ears: Vec<([u32; 3], RegionClass)> = Vec::with_capacity(poly.len());
    while work.len() > 3 {
        let m = work.len();
        let mut clipped = false;
        'ear: for k in 0..m {
            let (ia, ib, ic) = (work[(k + m - 1) % m], work[k], work[(k + 1) % m]);
            let ear = [ia, ib, ic];
            if !gate_tri_degenerate(&ear, coords) {
                // Convex, gate-valid, empty, and a NEW diagonal.
                if !gate_tri_valid(&ear, coords, frame) {
                    continue;
                }
                let (pa, pb, pc) = (pos(ia), pos(ib), pos(ic));
                if orient_sign_exact(pa, pb, pc) != Some(1) {
                    continue;
                }
                for &other in work.iter() {
                    if other == ia || other == ib || other == ic {
                        continue;
                    }
                    let q = pos(other);
                    // Coincident with a corner (a collapsed twin) never
                    // blocks; its own zero-area ear clips it.
                    if q == pa || q == pb || q == pc {
                        continue;
                    }
                    let (Some(s1), Some(s2), Some(s3)) = (
                        orient_sign_exact(pa, pb, q),
                        orient_sign_exact(pb, pc, q),
                        orient_sign_exact(pc, pa, q),
                    ) else {
                        return Err(EarclipErr::Other("non-finite cavity polygon"));
                    };
                    if s1 >= 0 && s2 >= 0 && s3 >= 0 {
                        continue 'ear; // inside or on the ear
                    }
                }
                if let Some(inc) = edge_map.get(&edge_key(ia, ic)) {
                    if inc.iter().any(|t| !cavity.contains(t)) {
                        continue; // diagonal exists outside the cavity
                    }
                }
            }
            ears.push((ear, cls0));
            work.remove(k);
            clipped = true;
            break;
        }
        if !clipped {
            return Err(EarclipErr::Other("no clippable ear"));
        }
    }
    let last = [work[0], work[1], work[2]];
    if !gate_tri_valid(&last, coords, frame) {
        return Err(EarclipErr::Other("final ear invalid"));
    }
    ears.push((last, cls0));
    if probe {
        eprintln!("  [reloc-earclip] {probe_who} cavity={} tris", cavity.len());
    }
    Ok(ears)
}

/// Carved star cavity: the output of amendment-5's steps 1–2, shared by
/// `relocate_minted_vertex` and the amendment-14 split (§11).
pub(crate) struct Carved {
    /// Cavity triangle indices (the star, plus any growth absorptions).
    pub(crate) cavity: std::collections::BTreeSet<usize>,
    /// Oriented link chain around `v`: (tail, head, class) per cavity
    /// triangle, head-to-tail; cyclic iff `starts` is empty.
    pub(crate) link: Vec<(u32, u32, RegionClass)>,
    /// Open-chain start tails (empty ⇔ closed link / interior vertex).
    pub(crate) starts: Vec<u32>,
    /// Some link edge deferred at a constraint/pinch (fan invalid there).
    pub(crate) deferred: bool,
}

/// Steps 1–2 of the amendment-5 relocation, extracted verbatim: star +
/// oriented link chain + visibility-growth cavity carve (deferring at
/// constraints). Read-only; `Err` strings are the caller's probe-visible
/// reject reasons (byte-identical to the historical inline path).
pub(crate) fn carve_star_cavity(
    tris: &[[u32; 3]],
    class: &[RegionClass],
    edge_map: &BTreeMap<[u32; 2], Vec<usize>>,
    v: u32,
    coords: &[Point3],
    frame: &Frame,
) -> Result<Carved, &'static str> {
    let edge_key = |a: u32, b: u32| if a < b { [a, b] } else { [b, a] };

    // ── 1. Star + oriented link chain ────────────────────────────────────
    let star: Vec<usize> = tris
        .iter()
        .enumerate()
        .filter(|(_, t)| t.contains(&v))
        .map(|(i, _)| i)
        .collect();
    if star.is_empty() {
        return Err("empty star");
    }
    // Oriented opposite edge of each star triangle (consistent-CCW mesh ⇒
    // the link edges chain head-to-tail around v).
    let mut out: BTreeMap<u32, (u32, RegionClass)> = BTreeMap::new();
    let mut heads: std::collections::BTreeSet<u32> = std::collections::BTreeSet::new();
    for &ti in &star {
        let t = tris[ti];
        let k = t.iter().position(|&x| x == v).unwrap();
        let (a, b) = (t[(k + 1) % 3], t[(k + 2) % 3]);
        if out.insert(a, (b, class[ti])).is_some() {
            return Err("non-manifold star (duplicate link tail)");
        }
        heads.insert(b);
    }
    // Open chain (boundary vertex): exactly one tail that is never a head.
    // Closed chain (interior vertex): none — start at the smallest tail.
    let starts: Vec<u32> = out.keys().copied().filter(|a| !heads.contains(a)).collect();
    let start = match starts.len() {
        0 => *out.keys().next().unwrap(),
        1 => starts[0],
        _ => return Err("disconnected star (multiple open chains)"),
    };
    let mut link: Vec<(u32, u32, RegionClass)> = Vec::with_capacity(star.len());
    let mut visited: std::collections::BTreeSet<u32> = std::collections::BTreeSet::new();
    let mut cur = start;
    for _ in 0..star.len() {
        if !visited.insert(cur) {
            // A revisit before covering every star triangle = a shorter
            // subloop (non-manifold star).
            return Err("link chain revisits a vertex (subloop)");
        }
        let Some(&(next, cls)) = out.get(&cur) else {
            return Err("broken link chain");
        };
        link.push((cur, next, cls));
        cur = next;
    }
    if link.len() != star.len() || (starts.is_empty() && cur != start) {
        return Err("link chain does not cover the star");
    }

    // ── 2. Cavity carve by visibility growth (deferring at constraints) ──
    let mut cavity: std::collections::BTreeSet<usize> = star.iter().copied().collect();
    let mut deferred = false;
    let mut i = 0;
    while i < link.len() {
        let (a, b, cls) = link[i];
        if gate_tri_valid(&[v, a, b], coords, frame) {
            i += 1;
            continue;
        }
        let Some(inc) = edge_map.get(&edge_key(a, b)) else {
            return Err("link edge missing from edge map");
        };
        let ext: Vec<usize> = inc
            .iter()
            .copied()
            .filter(|t| !cavity.contains(t))
            .collect();
        if ext.len() != 1 {
            // Domain boundary (or a pinched edge both of whose sides were
            // absorbed): uncrossable — defer to the ear-clip.
            deferred = true;
            i += 1;
            continue;
        }
        let tj = ext[0];
        if class[tj] != cls {
            // Class boundary IS the intersection curve: uncrossable — defer.
            deferred = true;
            i += 1;
            continue;
        }
        // The external neighbor traverses (b, a); its apex joins the link.
        let tn = tris[tj];
        let Some(k) = (0..3).find(|&k| tn[k] == b && tn[(k + 1) % 3] == a) else {
            return Err("inconsistent neighbor orientation");
        };
        let x = tn[(k + 2) % 3];
        if x == v
            || link.iter().any(|&(la, lb, _)| la == x || lb == x)
            || edge_map.contains_key(&edge_key(v, x))
        {
            // Absorbing would pinch the cavity (apex already on the link /
            // spoke already exists): defer to the ear-clip.
            deferred = true;
            i += 1;
            continue;
        }
        let ncls = class[tj];
        cavity.insert(tj);
        link.splice(i..=i, [(a, x, ncls), (x, b, ncls)]);
        // Re-check from the first replacement edge. Edges before i cannot
        // regress (fan validity is coordinate-determined) and blocked edges
        // cannot become growable (externals only shrink) — one scan is a
        // fixpoint.
    }
    if cavity.len() != link.len() {
        return Err("cavity/link size mismatch");
    }
    Ok(Carved {
        cavity,
        link,
        starts,
        deferred,
    })
}

/// Amendment 5 (spec `n2_stage4_junction_cluster_merge` §3, M8 increment 8):
/// delete-and-reinsert cavity relocation of one minted vertex — the full
/// [#24 Yang §4.4.1 Fig 11] mesh-updating form, for folds a single Lawson
/// flip cannot repair (the rim-mint COLUMN HOP: a mint's in-plane
/// displacement crosses a populated sweep-event column, folding the whole
/// inter-column strip of triangles together; the folded set's boundary is
/// non-simple under the moved vertex, so neither flips nor a fan of the
/// folded set alone can fix it).
///
/// The star of `v` is carved out and re-triangulated around `v`'s CURRENT
/// resolved (minted) position in two stages:
///
/// 1. **Visibility growth (Bowyer–Watson):** a link edge whose fan triangle
///    `(v, wᵢ, wᵢ₊₁)` is invalid is crossed into its single external
///    same-class neighbor. Constraint edges are never crossed — a
///    class-boundary edge IS the intersection curve and a single-incidence
///    edge is the domain boundary — nor is a neighbor absorbed whose apex
///    already lies on the link (a pinch); such edges are DEFERRED, not
///    fatal. Growth is monotone (the cavity only gains triangles), so it
///    terminates; blocked edges can never become growable (fan validity is
///    coordinate-determined and externals only shrink), so one forward scan
///    with in-place re-checks suffices.
/// 2. If every fan triangle is valid, the fan IS the re-triangulation.
///    Otherwise (some deferred edge remains — the mint crossed the LINE of
///    a constraint chord whose segment lies elsewhere, so the cavity is not
///    star-shaped from `v`) the cavity polygon `[v, w₀, …, w_k]` is
///    re-triangulated by **constrained exact ear-clipping**: the constraint
///    edge stays a cavity boundary and is connected to other link vertices
///    instead of `v`. Guards (each rejects, loud): single-class cavity and
///    an open chain only (no constraint spokes to preserve, `v` on the
///    domain boundary); the polygon must be exactly simple and CCW on the
///    deduplicated position ring; an ear needs exact-CCW orientation, gate
///    validity, no other polygon vertex strictly inside or on it, and a
///    diagonal that does not already exist outside the cavity. Ears whose
///    3D image is bit-degenerate (collapsed sub-floor twins) clip freely —
///    they are dropped at emission (M-B).
///
/// Any reject leaves NO mutation (build-then-commit); the amendment-2
/// revert stays the caller's fallback, observable via kernel-v2's tripwire.
/// Purely combinatorial (`coords` fixed, same contract as the amendment-4
/// flips): every committed relocation replaces its cavity with all-valid
/// triangles and no other triangle changes shape, so the gate's folded
/// count strictly decreases — termination. Deterministic: BTree orders,
/// first-invalid-link-edge growth, first-clippable-ear order (I6). Cavity
/// size equals link-edge count throughout (a boundary star of k triangles
/// has a k-edge open chain, an interior star a k-edge cycle; each growth
/// step adds one of each; a (k+2)-gon ear-clips to k triangles; a wedge
/// decomposition sums per-wedge counts to the same total), so the
/// replacement overwrites the cavity slots in place and `edge_map` is
/// maintained incrementally.
///
/// Amendment 12 (spec `m8_stage0_multiclass_cavity_arm` §3, ALWAYS-ON
/// since the inc-2 flip — corpus OFF/ON measured zero category changes):
/// a MULTI-CLASS deferred cavity — the on-curve mint class the
/// single-class ear-clip guard used to reject, R0099's leak — is cut at
/// its class-transition spokes (the intersection polyline through the
/// mint, which moves WITH it) and each per-class WEDGE is
/// re-fanned/ear-clipped independently with the shared spokes as
/// preserved polygon edges.
#[allow(clippy::too_many_arguments)]
pub(crate) fn relocate_minted_vertex(
    tris: &mut [[u32; 3]],
    class: &mut [RegionClass],
    edge_map: &mut BTreeMap<[u32; 2], Vec<usize>>,
    v: u32,
    coords: &[Point3],
    frame: &Frame,
    minted_mark: &[bool],
    probe: bool,
) -> RelocOutcome {
    let edge_key = |a: u32, b: u32| if a < b { [a, b] } else { [b, a] };
    let reject = |why: &str| {
        if probe {
            eprintln!("  [reloc-reject] vert {v} {why}");
        }
        RelocOutcome::Rejected
    };

    // ── 1+2. Star + link + visibility-growth carve (shared with the
    // amendment-14 split, §11 — extracted verbatim; Err strings are the
    // historical probe reasons, byte for byte). ─────────────────────────
    let Carved {
        cavity,
        link,
        starts,
        deferred,
    } = match carve_star_cavity(tris, class, edge_map, v, coords, frame) {
        Ok(c) => c,
        Err(why) => return reject(why),
    };

    // ── 3. Re-triangulation: fan, or constrained exact ear-clip ──────────
    let new_tris: Vec<([u32; 3], RegionClass)> = if !deferred {
        if probe {
            eprintln!("  [reloc-fan] vert {v} cavity={} tris", cavity.len());
        }
        link.iter().map(|&(a, b, cls)| ([v, a, b], cls)).collect()
    } else if !link.iter().any(|&(_, _, c)| c != link[0].2) {
        // SINGLE-CLASS deferred cavity: the mint crossed the LINE of a
        // constraint chord, so the cavity is not star-shaped from v's
        // minted position. Ear-clip the cavity polygon [v, w0..wk] —
        // the constraint edge stays a cavity BOUNDARY, connected to
        // other link vertices. (Multi-class cavities take the
        // amendment-12 wedge decomposition below.)
        if starts.is_empty() {
            // A single-class closed link has zero class transitions by
            // definition: not on the intersection curve, and ear-clipping
            // the ring [w0..wk] would orphan v. The count stays in the
            // message for §7.2 census-probe continuity.
            return reject("interior vertex with constraint-blocked fan (class transitions: 0)");
        }
        let cls0 = link[0].2;
        let mut poly: Vec<u32> = Vec::with_capacity(link.len() + 2);
        poly.push(v);
        poly.push(link[0].0);
        for &(_, b, _) in &link {
            poly.push(b);
        }
        if probe {
            let w0 = link[0].0;
            let wk = link[link.len() - 1].1;
            eprintln!(
                "  [reloc-spokes] v={v} w0={w0} inc={:?} wk={wk} inc={:?}",
                edge_map.get(&edge_key(v, w0)).map(|x| x.len()),
                edge_map.get(&edge_key(v, wk)).map(|x| x.len()),
            );
        }
        match earclip_cavity_polygon(
            &poly,
            &cavity,
            cls0,
            coords,
            frame,
            edge_map,
            probe,
            &format!("vert {v}"),
        ) {
            Ok(ears) => ears,
            Err(EarclipErr::NotSimple { crossing }) => {
                if probe {
                    eprintln!("  [reloc-reject] vert {v} cavity polygon not simple");
                }
                // Amendment 6 trigger, amendment-10 narrowed: the joint
                // seeds are the minted vertices ON the crossing edges (the
                // interacting set — Fig-11 locality), identified by exact
                // position match against the same frame projection the
                // ear-clip used. Amendment 13: the first raw-poly crossing
                // may carry the backtrack merge pair or the split chord.
                let cross_idx = first_ring_crossing(&poly, coords, frame);
                return RelocOutcome::NonSimple {
                    ring_mints: poly
                        .iter()
                        .copied()
                        .filter(|&pi| {
                            pi != v
                                && minted_mark[pi as usize]
                                && crossing.contains(&frame.project(coords[pi as usize]))
                        })
                        .collect(),
                    merge_candidate: cross_idx.and_then(|(ci, cj)| {
                        fig11_backtrack_pair(&poly, ci, cj, minted_mark, coords, frame)
                    }),
                    split_chord: cross_idx.and_then(|(ci, cj)| fig11_split_chord(&poly, ci, cj)),
                };
            }
            Err(EarclipErr::Other(why)) => return reject(why),
        }
    } else {
        // ── Amendment 12: per-class WEDGE decomposition ──────────────────
        // A multi-class deferred cavity: the mint sits ON the intersection
        // polyline (that is what a rim crossing is — amendment 7's founding
        // observation), so its grown link straddles ≥ 2 region classes. Cut
        // the link at its class transitions — each happens exactly at the
        // shared spoke v→bᵢ, a class-boundary edge through v: the
        // intersection polyline through the mint, moved WITH it ([#24 Yang
        // §4.4.1 Fig 11] composed with §4.5.5's "overlap boundaries are
        // intersection curves", at the overlay level). Each maximal
        // same-class run (WEDGE) is re-fanned or ear-clipped independently
        // over the polygon [v, aᵢ, bᵢ, …, b_j] — v INCLUDED, its two
        // bounding spokes as preserved polygon edges (each a constraint
        // spoke at the mint's CURRENT position, or a domain-boundary end).
        // The two wedges flanking a spoke re-triangulate against the SAME
        // edge — same vertex ids, same coordinates — so the union covers
        // the grown cavity with no gap and no overlap across the moved
        // polyline: conformality by shared identity, the #169 two-sided
        // principle one stage earlier. Growth never crosses a class
        // boundary, so class runs (hence transition spokes) are exactly the
        // original constraint spokes. Build-then-commit: any wedge reject
        // leaves NO mutation and falls to the amendment-2 revert.
        let n = link.len();
        let order: Vec<usize> = if starts.is_empty() {
            // Closed link (interior on-curve mint): rotate so every wedge
            // is contiguous — start at the first entry whose CYCLIC
            // predecessor differs in class (exists: the link is
            // multi-class, and a non-constant cyclic sequence has ≥ 2
            // transitions). The rotation also guarantees the first and
            // last runs differ in class, so no cyclic merge is needed.
            let t0 = (0..n)
                .find(|&i| link[(i + n - 1) % n].2 != link[i].2)
                .unwrap();
            (0..n).map(|k| (t0 + k) % n).collect()
        } else {
            (0..n).collect()
        };
        let mut wedges: Vec<Vec<usize>> = Vec::new();
        for &i in &order {
            match wedges.last_mut() {
                Some(w) if link[w[w.len() - 1]].2 == link[i].2 => w.push(i),
                _ => wedges.push(vec![i]),
            }
        }
        if probe {
            eprintln!(
                "  [reloc-wedges] vert {v} closed={} wedges {:?}",
                starts.is_empty(),
                wedges
                    .iter()
                    .map(|w| (link[w[0]].2, w.len()))
                    .collect::<Vec<_>>(),
            );
        }
        let mut ears: Vec<([u32; 3], RegionClass)> = Vec::with_capacity(n);
        for (wi, w) in wedges.iter().enumerate() {
            let cls = link[w[0]].2;
            // A wedge whose fan triangles are all valid saw no growth
            // deferral (growth defers only on an invalid fan triangle):
            // the fan IS its re-triangulation and v keeps every spoke.
            if w.iter()
                .all(|&i| gate_tri_valid(&[v, link[i].0, link[i].1], coords, frame))
            {
                if probe {
                    eprintln!(
                        "  [reloc-wedge-fan] vert {v} wedge {wi} ({cls:?}) {} tris",
                        w.len()
                    );
                }
                ears.extend(w.iter().map(|&i| ([v, link[i].0, link[i].1], cls)));
                continue;
            }
            let mut poly: Vec<u32> = Vec::with_capacity(w.len() + 2);
            poly.push(v);
            poly.push(link[w[0]].0);
            for &i in w {
                poly.push(link[i].1);
            }
            match earclip_cavity_polygon(
                &poly,
                &cavity,
                cls,
                coords,
                frame,
                edge_map,
                probe,
                &format!("vert {v} wedge {wi}"),
            ) {
                Ok(we) => ears.extend(we),
                Err(EarclipErr::NotSimple { crossing }) => {
                    if probe {
                        eprintln!(
                            "  [reloc-reject] vert {v} wedge {wi} ({cls:?}) \
                             cavity polygon not simple"
                        );
                    }
                    // Amendment-10 narrowing, per wedge: the joint seeds are
                    // the mints on THIS wedge polygon's crossing edges (the
                    // interacting set — Fig-11 locality). Amendment 13: the
                    // first raw-poly crossing may carry the backtrack merge
                    // pair or the split chord (the singleton-NonSimple
                    // customers, R0099 verts 4/9).
                    let cross_idx = first_ring_crossing(&poly, coords, frame);
                    return RelocOutcome::NonSimple {
                        ring_mints: poly
                            .iter()
                            .copied()
                            .filter(|&pi| {
                                pi != v
                                    && minted_mark[pi as usize]
                                    && crossing.contains(&frame.project(coords[pi as usize]))
                            })
                            .collect(),
                        merge_candidate: cross_idx.and_then(|(ci, cj)| {
                            fig11_backtrack_pair(&poly, ci, cj, minted_mark, coords, frame)
                        }),
                        split_chord: cross_idx
                            .and_then(|(ci, cj)| fig11_split_chord(&poly, ci, cj)),
                    };
                }
                Err(EarclipErr::Other(why)) => {
                    return reject(&format!("wedge {wi} ({cls:?}): {why}"));
                }
            }
        }
        ears
    };
    if new_tris.len() != cavity.len() {
        return reject("replacement/cavity size mismatch");
    }

    // ── 4. Commit: overwrite the cavity slots in place ────────────────────
    let cavity: Vec<usize> = cavity.into_iter().collect();
    for &ti in &cavity {
        let t = tris[ti];
        for k in 0..3 {
            let kk = edge_key(t[k], t[(k + 1) % 3]);
            if let Some(e) = edge_map.get_mut(&kk) {
                e.retain(|&x| x != ti);
                if e.is_empty() {
                    edge_map.remove(&kk);
                }
            }
        }
    }
    for (&ti, &(t, cls)) in cavity.iter().zip(&new_tris) {
        tris[ti] = t;
        class[ti] = cls;
        for k in 0..3 {
            edge_map
                .entry(edge_key(t[k], t[(k + 1) % 3]))
                .or_default()
                .push(ti);
        }
    }
    RelocOutcome::Committed
}

/// Amendment 6 (spec `n2_stage4_junction_cluster_merge` §3, M8 increment 9,
/// task #64): JOINT delete-and-reinsert relocation of an interacting set of
/// minted vertices — the [#24 Yang §4.4.1 Fig 11] mesh-updating form
/// generalized from one vertex's star to the UNION of the seeds' stars, for
/// the multi-column strip class where each per-vertex cavity polygon is
/// exactly NON-SIMPLE because it contains the OTHER minted vertex's
/// collapsed spokes (measured F0087 cut 9: the plate-rim mint and a
/// hole-rim mint at the two ends of one strip of long CDT triangles).
///
/// The region = the union of the seeds' vertex stars. Its oriented
/// boundary (edges whose reverse no region triangle carries — domain
/// boundaries qualify by construction) must chain into exactly ONE closed
/// cycle passing through every region-triangle vertex; the cycle is then
/// re-triangulated by the shared constrained exact ear-clip
/// ([`earclip_cavity_polygon`]) with all seeds at their minted positions.
/// Guards, each a reject (the caller's amendment-2 revert stays the loud
/// fallback): single class across the region (class-boundary edges — the
/// intersection curve — are then automatically ON the cycle, never
/// re-triangulated across); no interior vertex (seed or not — a polygon
/// triangulation would orphan it); one cycle; exact simplicity + CCW.
///
/// Build-then-commit: a reject leaves NO mutation. Purely combinatorial
/// (`coords` fixed, the amendment-4/5 termination contract): a committed
/// joint relocation replaces ≥1 folded triangle with all-valid triangles
/// and no fold can be created, so the gate's folded count strictly
/// decreases. Deterministic: ascending seeds, smallest-tail cycle start,
/// first-clippable-ear order (I6). A triangulated simple polygon with no
/// interior vertices has exactly (cycle length − 2) triangles, so the
/// replacement count equals the region size and the region's slots are
/// overwritten in place.
#[allow(clippy::too_many_arguments)]
pub(crate) fn relocate_minted_region(
    tris: &mut [[u32; 3]],
    class: &mut [RegionClass],
    edge_map: &mut BTreeMap<[u32; 2], Vec<usize>>,
    seeds: &[u32],
    coords: &[Point3],
    frame: &Frame,
    minted_mark: &[bool],
    probe: bool,
) -> RegionOutcome {
    // ── 1. Region = union of the seeds' stars, PARTITIONED by class ──────
    // Amendment 7 (M8 increment 10): rim mints are minted exactly ON the
    // intersection curve — that is what a rim crossing is — so the star
    // union routinely straddles the class boundary. Each class sub-region
    // is relocated independently: a class-boundary edge's reverse lives in
    // the OTHER class's triangle (outside the sub-region), so the
    // intersection curve becomes sub-region boundary by construction and
    // is never re-triangulated across. A single-class region makes the
    // partition the identity (amendment-6 behavior, unchanged).
    let mut by_class: BTreeMap<RegionClass, std::collections::BTreeSet<usize>> = BTreeMap::new();
    for (ti, t) in tris.iter().enumerate() {
        if t.iter().any(|v| seeds.contains(v)) {
            by_class.entry(class[ti]).or_default().insert(ti);
        }
    }
    let mut committed_any = false;
    // Amendment 13: the first Fig-11 backtrack pair reported by a
    // rejecting sub-region (deterministic: class order, component order).
    let mut merge_candidate: Option<(u32, u32, f64, f64)> = None;
    for (cls0, region) in by_class {
        // Amendment 9 (M8 increment 12): a class sub-region may be
        // DISCONNECTED — the joint trigger accumulates seeds from several
        // separate strips, and one boundary walk cannot cover two
        // components. Split into edge-connected components (deterministic
        // ascending-index BFS through shared edges); each is its own
        // Fig-11 instance, attempted independently.
        let edge_key = |a: u32, b: u32| if a < b { [a, b] } else { [b, a] };
        let mut unvisited = region.clone();
        while let Some(&comp_seed) = unvisited.iter().next() {
            let mut component: std::collections::BTreeSet<usize> =
                std::collections::BTreeSet::new();
            let mut queue = vec![comp_seed];
            unvisited.remove(&comp_seed);
            while let Some(ti) = queue.pop() {
                component.insert(ti);
                let t = tris[ti];
                for k in 0..3 {
                    if let Some(inc) = edge_map.get(&edge_key(t[k], t[(k + 1) % 3])) {
                        for &tj in inc {
                            if unvisited.remove(&tj) {
                                queue.push(tj);
                            }
                        }
                    }
                }
            }
            // Termination contract: only a component carrying at least one
            // FOLDED triangle is attempted — its commit strictly decreases
            // the gate's folded count (replacement ears are gate-valid by
            // construction). A valid-only component is SKIPPED:
            // re-triangulating it could churn the mesh without progress.
            let folded = component.iter().any(|&ti| {
                !gate_tri_degenerate(&tris[ti], coords)
                    && gate_tri_area(&tris[ti], coords, frame) <= 0.0
            });
            if !folded {
                continue;
            }
            if relocate_region_single_class(
                tris,
                class,
                edge_map,
                seeds,
                &component,
                cls0,
                coords,
                frame,
                minted_mark,
                probe,
                &mut merge_candidate,
            ) {
                committed_any = true;
            }
        }
    }
    if committed_any {
        return RegionOutcome::Committed;
    }
    if probe {
        eprintln!("  [reloc-region-reject] seeds {seeds:?} every folded class sub-region rejected");
    }
    // Amendment 13: surface the first (deterministic — class order, then
    // component BFS order) Fig-11 backtrack pair found among the rejecting
    // sub-regions, so the ladder can merge instead of reverting.
    match merge_candidate {
        Some((p, q, overshoot, chord_len)) => RegionOutcome::MergeCandidate {
            p,
            q,
            overshoot,
            chord_len,
        },
        None => RegionOutcome::Rejected,
    }
}

/// One class sub-region of the amendment-6/7 joint relocation: oriented
/// boundary cycle, no interior vertex, exact simplicity + CCW, shared
/// constrained exact ear-clip, in-place commit. Build-then-commit: a
/// reject leaves NO mutation of this sub-region (other sub-regions'
/// commits are independent — each is separately valid and fold-reducing).
#[allow(clippy::too_many_arguments)]
pub(crate) fn relocate_region_single_class(
    tris: &mut [[u32; 3]],
    class: &mut [RegionClass],
    edge_map: &mut BTreeMap<[u32; 2], Vec<usize>>,
    seeds: &[u32],
    region: &std::collections::BTreeSet<usize>,
    cls0: RegionClass,
    coords: &[Point3],
    frame: &Frame,
    minted_mark: &[bool],
    probe: bool,
    merge_candidate: &mut Option<(u32, u32, f64, f64)>,
) -> bool {
    let edge_key = |a: u32, b: u32| if a < b { [a, b] } else { [b, a] };
    let reject = |why: &str| {
        if probe {
            eprintln!("  [reloc-region-reject] seeds {seeds:?} class {cls0:?} {why}");
        }
        false
    };
    if region.len() < 2 {
        // inc-3.0 measurement probe: the 1-triangle class sub-region at an
        // on-curve seed — which triangle, which vertices are seeds, where.
        if probe {
            for &ti in region {
                let t = tris[ti];
                let at: Vec<_> = t
                    .iter()
                    .map(|&vv| {
                        (
                            vv,
                            seeds.contains(&vv),
                            minted_mark[vv as usize],
                            frame.project(coords[vv as usize]),
                        )
                    })
                    .collect();
                eprintln!(
                    "  [reloc-region-toosmall] seeds {seeds:?} class {cls0:?} \
                     tri {ti} (id, is_seed, minted, uv): {at:?}"
                );
            }
        }
        return reject("region too small");
    }

    // Amendment 8 (M8 increment 11): the sub-region may GROW across a
    // crossing boundary edge (below), so the boundary cycle and its guards
    // are recomputed per growth step.
    let mut region: std::collections::BTreeSet<usize> = region.clone();
    let poly = loop {
        // ── 2. Oriented boundary cycle ────────────────────────────────────
        // A consistent-CCW mesh: an oriented edge (a,b) of a region triangle
        // is boundary iff no region triangle carries (b,a).
        let mut oriented: std::collections::BTreeSet<(u32, u32)> =
            std::collections::BTreeSet::new();
        for &ti in &region {
            let t = tris[ti];
            for k in 0..3 {
                if !oriented.insert((t[k], t[(k + 1) % 3])) {
                    return reject("duplicate oriented edge (non-manifold region)");
                }
            }
        }
        let mut nxt: BTreeMap<u32, u32> = BTreeMap::new();
        let mut boundary_edges = 0usize;
        for &(a, b) in &oriented {
            if !oriented.contains(&(b, a)) {
                if nxt.insert(a, b).is_some() {
                    return reject("non-manifold region boundary (duplicate tail)");
                }
                boundary_edges += 1;
            }
        }
        if boundary_edges < 3 {
            return reject("degenerate region boundary");
        }
        let start = *nxt.keys().next().unwrap();
        let mut poly: Vec<u32> = Vec::with_capacity(boundary_edges);
        let mut cur = start;
        for _ in 0..boundary_edges {
            poly.push(cur);
            let Some(&next) = nxt.get(&cur) else {
                return reject("broken region boundary chain");
            };
            cur = next;
        }
        if cur != start || poly.len() != boundary_edges {
            // Increment-13 measurement probe: enumerate ALL boundary
            // cycles (count + lengths) so the annular class's structure is
            // observable at the reject site (one component with several
            // cycles = a region encircling a hole).
            if probe {
                let mut rest = nxt.clone();
                let mut cycles: Vec<usize> = Vec::new();
                while let Some((&s0, _)) = rest.iter().next() {
                    let mut c = s0;
                    let mut len = 0usize;
                    while let Some(n) = rest.remove(&c) {
                        len += 1;
                        c = n;
                        if c == s0 {
                            break;
                        }
                    }
                    cycles.push(len);
                }
                eprintln!(
                    "  [reloc-region-cycles] seeds {seeds:?} class {cls0:?} \
                     {} boundary cycles, lengths {cycles:?}",
                    cycles.len()
                );
            }
            return reject("region boundary is not a single closed cycle");
        }

        // ── 3. No interior vertex (a triangulation would orphan it) ───────
        let on_cycle: std::collections::BTreeSet<u32> = poly.iter().copied().collect();
        for &ti in &region {
            for &vv in &tris[ti] {
                if !on_cycle.contains(&vv) {
                    return reject("region has an interior vertex");
                }
            }
        }

        // ── 3b. Amendment 8: growth to simplicity ─────────────────────────
        // A femto-strip sub-region's boundary can be a BOW-TIE under the
        // minted positions (the strip's two long sides cross exactly — the
        // F0090 class). The region form of amendment 5's constrained
        // visibility growth: absorb the single external same-class neighbor
        // of a crossing edge and rebuild the boundary, until the ring is
        // exactly simple. Constraint edges (domain boundary, intersection
        // curve) are never crossed; an apex already on the cycle would
        // pinch the ring (both defer to the partner edge, else reject).
        let Some((ei, ej)) = first_ring_crossing(&poly, coords, frame) else {
            break poly;
        };
        let mut grew = false;
        // inc-3.0 measurement probe: per crossing edge, WHY it could not
        // be crossed (the reject string alone cannot separate the
        // domain-boundary, intersection-curve, and pinch sub-cases).
        let mut why: Vec<String> = Vec::new();
        for e in [ei, ej] {
            let (a, b) = (poly[e], poly[(e + 1) % poly.len()]);
            let Some(inc) = edge_map.get(&edge_key(a, b)) else {
                if probe {
                    why.push(format!("({a},{b}): missing from edge map"));
                }
                continue;
            };
            let ext: Vec<usize> = inc
                .iter()
                .copied()
                .filter(|t| !region.contains(t))
                .collect();
            if ext.len() != 1 {
                if probe {
                    why.push(format!(
                        "({a},{b}): {} externals (domain boundary/pinched)",
                        ext.len()
                    ));
                }
                continue; // domain boundary (or pinched): uncrossable
            }
            let tj = ext[0];
            if class[tj] != cls0 {
                if probe {
                    why.push(format!(
                        "({a},{b}): external tri {tj} is {:?} (intersection curve)",
                        class[tj]
                    ));
                }
                continue; // class boundary IS the intersection curve
            }
            let Some(x) = tris[tj].iter().copied().find(|&v| v != a && v != b) else {
                if probe {
                    why.push(format!("({a},{b}): degenerate external {tj}"));
                }
                continue;
            };
            if on_cycle.contains(&x) {
                if probe {
                    why.push(format!("({a},{b}): apex {x} already on cycle (pinch)"));
                }
                continue; // absorbing would pinch the ring
            }
            if probe {
                eprintln!(
                    "  [reloc-region-grow] seeds {seeds:?} class {cls0:?} \
                     edge ({a},{b}) absorbs tri {tj} (apex {x})"
                );
            }
            region.insert(tj);
            grew = true;
            break;
        }
        if !grew {
            // Amendment 13: an ungrowable crossing in the Fig-11 BACKTRACK
            // configuration is a merge candidate — report it to the caller
            // (first one wins; no mutation here).
            if let Some((pp, qq, ov, cl)) =
                fig11_backtrack_pair(&poly, ei, ej, minted_mark, coords, frame)
            {
                if probe {
                    eprintln!(
                        "  [reloc-region-fig11] seeds {seeds:?} class {cls0:?} \
                         backtrack p={pp} (unminted, {:?}) q={qq} (minted, {:?}) \
                         overshoot={ov:e} chord={cl:e}",
                        coords[pp as usize], coords[qq as usize]
                    );
                }
                if merge_candidate.is_none() {
                    *merge_candidate = Some((pp, qq, ov, cl));
                }
            }
            if probe {
                let pos = |k: usize| frame.project(coords[poly[k] as usize]);
                let ring_at: Vec<(u32, bool, (f64, f64))> = (0..poly.len())
                    .map(|k| (poly[k], minted_mark[poly[k] as usize], pos(k)))
                    .collect();
                eprintln!(
                    "  [reloc-region-ungrowable] seeds {seeds:?} class {cls0:?} \
                     ring {} (id, minted, uv) {ring_at:?}; crossing e{ei}({:?}->{:?}) x \
                     e{ej}({:?}->{:?}); {}",
                    poly.len(),
                    pos(ei),
                    pos((ei + 1) % poly.len()),
                    pos(ej),
                    pos((ej + 1) % poly.len()),
                    why.join("; ")
                );
            }
            return reject("crossing edges ungrowable (region polygon not simple)");
        }
    };

    // ── 4. Shared constrained exact ear-clip ──────────────────────────────
    let ears = match earclip_cavity_polygon(
        &poly,
        &region,
        cls0,
        coords,
        frame,
        edge_map,
        probe,
        &format!("region {seeds:?}"),
    ) {
        Ok(ears) => ears,
        Err(EarclipErr::NotSimple { .. }) => return reject("region polygon not simple"),
        Err(EarclipErr::Other(why)) => return reject(why),
    };
    if ears.len() != region.len() {
        return reject("replacement/region size mismatch");
    }

    // ── 5. Commit: overwrite the region slots in place ────────────────────
    let region: Vec<usize> = region.iter().copied().collect();
    for &ti in &region {
        let t = tris[ti];
        for k in 0..3 {
            let kk = edge_key(t[k], t[(k + 1) % 3]);
            if let Some(e) = edge_map.get_mut(&kk) {
                e.retain(|&x| x != ti);
                if e.is_empty() {
                    edge_map.remove(&kk);
                }
            }
        }
    }
    for (&ti, &(t, cls)) in region.iter().zip(&ears) {
        tris[ti] = t;
        class[ti] = cls;
        for k in 0..3 {
            edge_map
                .entry(edge_key(t[k], t[(k + 1) % 3]))
                .or_default()
                .push(ti);
        }
    }
    true
}

/// Amendment-13 measurement (spec `m8_stage0_multiclass_cavity_arm`): the
/// Fig-11(b→c) BACKTRACK detector. The two crossing ring edges sit exactly
/// two apart, sandwiching one short edge whose endpoints are an UNMINTED
/// vertex p and a MINTED vertex q — the "endpoint p of the split edge is
/// too close to q" configuration of [#24 Yang §4.4.1 Fig 11] (the boundary
/// walks out past the mint and backtracks, so the overshooting constrained
/// chord crosses the mint's exit edge by a hair). Purely combinatorial +
/// mintedness — no distance band. Returns (p, q).
/// Amendment-13 Fig-11(a) SPLIT detector (spec
/// `m8_stage0_multiclass_cavity_arm` §10d inc-3.2): the ring crossing
/// pairs v's OWN boundary edge (poly position 0 is v, so its edges are
/// e0 and e_{n−1}) with a link CHORD — the mint pokes past a constrained
/// edge whose endpoints are beyond merging reach. Returns the chord's
/// (a, b) in poly order; the ladder verifies incidence/validity and
/// reroutes the chord through the mint. Purely combinatorial.
pub(crate) fn fig11_split_chord(poly: &[u32], ei: usize, ej: usize) -> Option<(u32, u32)> {
    let n = poly.len();
    let touches_v = |e: usize| e == 0 || e == n - 1;
    let chord = match (touches_v(ei), touches_v(ej)) {
        (true, false) => ej,
        (false, true) => ei,
        _ => return None,
    };
    Some((poly[chord], poly[(chord + 1) % n]))
}

/// Returns `(p, q, overshoot, split_chord_len)` — additionally measuring
/// the inc-3.4 CONTAINMENT quantities: `overshoot` is q's distance to the
/// LINE of the crossing edge INCIDENT TO p (Fig 11(b)'s split edge — the
/// "constrained edge containing q"), `split_chord_len` that edge's length.
/// The ladder accepts the merge only when the overshoot is within the
/// chord's own circle-approximation error (sagitta) — the R0059
/// counterexample (overshoot/chord ≈ 0.5, a unit-scale interpenetration)
/// fails it while the R0099 true cases (≈ 1e-4, hair grazes) pass.
pub(crate) fn fig11_backtrack_pair(
    poly: &[u32],
    ei: usize,
    ej: usize,
    minted_mark: &[bool],
    coords: &[Point3],
    frame: &Frame,
) -> Option<(u32, u32, f64, f64)> {
    let n = poly.len();
    // On a 4-gon the two crossing edges sandwich BOTH remaining edges —
    // try each sandwiched mid (deterministic order) and take the first
    // that carries exactly one mint.
    let mut mids = [None, None];
    if (ei + 2) % n == ej {
        mids[0] = Some((ei + 1) % n);
    }
    if (ej + 2) % n == ei {
        mids[1] = Some((ej + 1) % n);
    }
    for mid in mids.into_iter().flatten() {
        let (a, b) = (poly[mid], poly[(mid + 1) % n]);
        let (p, q) = match (minted_mark[a as usize], minted_mark[b as usize]) {
            (false, true) => (a, b),
            (true, false) => (b, a),
            _ => continue,
        };
        // The split edge = the crossing edge whose endpoint is p: the mid
        // edge runs positions mid → mid+1; p's flanking crossing edge is
        // the ring edge on p's side of the sandwich.
        let p_edge = if poly[mid] == p {
            (mid + n - 1) % n
        } else {
            (mid + 1) % n
        };
        let ea = frame.project(coords[poly[p_edge] as usize]);
        let eb = frame.project(coords[poly[(p_edge + 1) % n] as usize]);
        let qq = frame.project(coords[q as usize]);
        let (dx, dy) = (eb.0 - ea.0, eb.1 - ea.1);
        let len = (dx * dx + dy * dy).sqrt();
        if !(len > 0.0) {
            continue;
        }
        let overshoot = ((qq.0 - ea.0) * dy - (qq.1 - ea.1) * dx).abs() / len;
        return Some((p, q, overshoot, len));
    }
    None
}

/// Amendment 8 (spec `n2_stage4_junction_cluster_merge` §3, M8 increment
/// 11): first exact crossing of a boundary polygon's edges under the
/// CURRENT resolved coordinates — a proper crossing or an endpoint strictly
/// interior to the other segment (the same predicate as
/// [`earclip_cavity_polygon`]'s simplicity guard, `EarclipErr::NotSimple`
/// class). Returns the two poly edge indices, first pair in boundary-order
/// scan (deterministic — I6); `None` = simple. Zero-length edges (collapsed
/// sub-floor twins) and edge pairs sharing a POSITION are skipped — shared
/// positions are the pinch class, terminal in the ear-clip, not grown.
pub(crate) fn first_ring_crossing(
    poly: &[u32],
    coords: &[Point3],
    frame: &Frame,
) -> Option<(usize, usize)> {
    let n = poly.len();
    let pos = |i: usize| frame.project(coords[poly[i] as usize]);
    for a in 0..n {
        let (p1, p2) = (pos(a), pos((a + 1) % n));
        if p1 == p2 {
            continue; // zero-length (collapsed twins) cannot cross
        }
        for b in (a + 1)..n {
            let (q1, q2) = (pos(b), pos((b + 1) % n));
            if q1 == q2 {
                continue;
            }
            // Edges sharing a position are adjacent (or a pinch — the
            // ear-clip's terminal class): never a growth trigger.
            if p1 == q1 || p1 == q2 || p2 == q1 || p2 == q2 {
                continue;
            }
            let (Some(o1), Some(o2), Some(o3), Some(o4)) = (
                orient_sign_exact(p1, p2, q1),
                orient_sign_exact(p1, p2, q2),
                orient_sign_exact(q1, q2, p1),
                orient_sign_exact(q1, q2, p2),
            ) else {
                return None; // non-finite: leave for the ear-clip to reject
            };
            let within = |o: i8, e1: (f64, f64), e2: (f64, f64), q: (f64, f64)| {
                o == 0
                    && q.0 >= e1.0.min(e2.0)
                    && q.0 <= e1.0.max(e2.0)
                    && q.1 >= e1.1.min(e2.1)
                    && q.1 <= e1.1.max(e2.1)
            };
            if (o1 * o2 < 0 && o3 * o4 < 0)
                || within(o1, p1, p2, q1)
                || within(o2, p1, p2, q2)
                || within(o3, q1, q2, p1)
                || within(o4, q1, q2, p2)
            {
                return Some((a, b));
            }
        }
    }
    None
}

/// Amendment 14 (spec `m8_stage0_multiclass_cavity_arm` §11, ALWAYS-ON
/// since the inc-3.2d flip): the Fig-11(a) vertex-inserting SPLIT — the
/// overlay's first vertex-inserting operation.
///
/// The armed customer (R0099 vert 9, §11a): an interior mint `v` whose
/// on-circle position pokes a hair PAST a constrained ring chord `C` that
/// is the OTHER input's real model edge (near-tangency the chord-geometry
/// arrangement never saw). Every fixed-vertex-set arm correctly refuses;
/// the paper's operation is to SPLIT the constrained edge where the moved
/// boundary polyline crosses it (Yang §4.4.1 Fig 11(a)) and CDT the
/// trimmed sides so the polyline is their boundary.
///
/// Two new vertices q_a/q_b are minted with exact rational UVs ON C (so
/// `collect_edge_splits` propagates the other-input leg with zero new
/// machinery), the carved cavity re-cuts into two side-class remnants +
/// the material polygon + the past-C bulge, and the chain through `v`
/// (its two class-transition spokes) becomes the union boundary around
/// the bulge. Build-then-commit: any guard reject leaves NO mutation
/// (pushed vertices are unwound) and the amendment-2 revert stays the
/// caller's fallback. Guards per §11d — each loud, census-visible.
#[allow(clippy::too_many_arguments)]
pub(crate) fn fig11_split_cavity(
    overlay: &mut ClassifiedOverlay,
    edge_map: &mut BTreeMap<[u32; 2], Vec<usize>>,
    v: u32,
    chord: (u32, u32),
    coords: &mut Vec<Point3>,
    minted_mark: &mut Vec<bool>,
    mergeable_mark: &mut Vec<bool>,
    frame: &Frame,
    sagitta: Option<f64>,
    own_chords: &[(ExactPoint2, ExactPoint2)],
    other_segs: &[(ExactPoint2, ExactPoint2)],
    other_is_b: bool,
    out_extras: &mut Vec<ExtraRimPoint>,
    probe: bool,
) -> bool {
    let reject = |why: &str| {
        if probe {
            eprintln!("  [split-reject] vert {v} {why}");
        }
        false
    };
    let edge_key = |a: u32, b: u32| if a < b { [a, b] } else { [b, a] };

    // ── 1. Shared carve (star + link + growth). ──────────────────────────
    let Carved {
        cavity,
        link,
        starts,
        deferred: _,
    } = match carve_star_cavity(&overlay.tris, &overlay.class, edge_map, v, coords, frame) {
        Ok(c) => c,
        Err(why) => return reject(why),
    };
    if !starts.is_empty() {
        return reject("split-open-link (boundary vertex — not the armed class)");
    }
    let n = link.len();

    // ── 2. Exactly two class transitions ⇒ the chain spokes. ─────────────
    let trans: Vec<usize> = (0..n)
        .filter(|&i| link[(i + n - 1) % n].2 != link[i].2)
        .collect();
    if trans.len() != 2 {
        return reject(&format!("split-chain-transitions ({})", trans.len()));
    }
    // C must be a ring edge of the carved link.
    let Some(e_c) = (0..n).find(|&i| {
        let (a, b, _) = link[i];
        (a, b) == chord || (b, a) == chord
    }) else {
        return reject("split-chord-not-on-ring");
    };
    // The two single-class runs; the C-containing run is the SIDE, the
    // other the MATERIAL (§11c). Cyclic run [s, e).
    let in_run = |i: usize, s: usize, e: usize| {
        if s <= e {
            i >= s && i < e
        } else {
            i >= s || i < e
        }
    };
    let (t0, t1) = (trans[0], trans[1]);
    let (side_start, mat_start) = if in_run(e_c, t0, t1) {
        (t0, t1)
    } else {
        (t1, t0)
    };
    let side_cls = link[side_start].2;
    let mat_cls = link[mat_start].2;
    // Armed class pair (§11c): material Overlap, side = the own-input-only
    // class; the bulge takes the other input's membership flipped.
    let (want_side, bulge_cls) = if other_is_b {
        (RegionClass::BOnly, RegionClass::AOnly)
    } else {
        (RegionClass::AOnly, RegionClass::BOnly)
    };
    if mat_cls != RegionClass::Overlap || side_cls != want_side {
        return reject(&format!(
            "split-class-pair (mat {mat_cls:?} side {side_cls:?})"
        ));
    }
    // C must be a domain boundary (1-incident): the bulge grows into
    // territory no triangle covers. A 2-incident C means real triangles
    // beyond — a larger op, not the armed form.
    let (r_k, r_k1) = (link[e_c].0, link[e_c].1);
    if edge_map.get(&edge_key(r_k, r_k1)).map(|e| e.len()) != Some(1) {
        return reject("split-chord-not-boundary");
    }
    // C on the OTHER input's real edge: both endpoints exactly collinear
    // with one common boundary sub-segment (exact rationals).
    let on_seg = |p: &ExactPoint2, s: &ExactPoint2, e: &ExactPoint2| {
        let dx = &e.x - &s.x;
        let dy = &e.y - &s.y;
        let wx = &p.x - &s.x;
        let wy = &p.y - &s.y;
        &dx * &wy - &dy * &wx == RBig::ZERO
    };
    let (ka, kb) = (
        &overlay.exact_verts[r_k as usize],
        &overlay.exact_verts[r_k1 as usize],
    );
    if !other_segs
        .iter()
        .any(|(s, e)| on_seg(ka, s, e) && on_seg(kb, s, e))
    {
        return reject("split-chord-not-other-input-edge");
    }
    // The A-leg (§11c step 3): v's OWN rim sub-chord — the chain through v
    // is that rim's boundary, and q_a/q_b must join its override chain in
    // boundary order. Identified by exact collinearity of v's pre-mint UV
    // with a sub-chord, interior parameter; absent ⇒ the propagation leg
    // has no vehicle ⇒ reject (a silent T-junction is the disease).
    let v_ex = &overlay.exact_verts[v as usize];
    let own = own_chords.iter().find(|(s, e)| {
        if !on_seg(v_ex, s, e) {
            return false;
        }
        let dx = &e.x - &s.x;
        let dy = &e.y - &s.y;
        let len2 = &dx * &dx + &dy * &dy;
        if len2 == RBig::ZERO {
            return false;
        }
        let t = (&(&v_ex.x - &s.x) * &dx + &(&v_ex.y - &s.y) * &dy) / &len2;
        t > RBig::ZERO && t < RBig::ONE
    });
    let Some((own_s, own_e)) = own else {
        return reject("split-no-own-chord");
    };

    // ── 3. Crossings of the chain spokes with C (frame f64). ─────────────
    let c_first = link[mat_start].0; // material arc start (chain vertex)
    let c_last = link[side_start].0; // material arc end = side arc start
    let p2 = |i: u32| frame.project(coords[i as usize]);
    let (pk, pk1, pv) = (p2(r_k), p2(r_k1), p2(v));
    let dc = (pk1.0 - pk.0, pk1.1 - pk.1);
    let cross2 = |a: (f64, f64), b: (f64, f64)| a.0 * b.1 - a.1 * b.0;
    let seg_cross = |a: (f64, f64), b: (f64, f64)| -> Option<(f64, f64)> {
        let ds = (b.0 - a.0, b.1 - a.1);
        let den = cross2(ds, dc);
        if den == 0.0 || !den.is_finite() {
            return None;
        }
        let w = (pk.0 - a.0, pk.1 - a.1);
        let t_s = cross2(w, dc) / den; // along the spoke a→b
        let t_c = cross2(w, ds) / den; // along C (r_k → r_k1)
        (t_s > 0.0 && t_s < 1.0 && t_c > 0.0 && t_c < 1.0).then_some((t_s, t_c))
    };
    let Some((_, tc_a)) = seg_cross(p2(c_first), pv) else {
        return reject("split-crossing-count (first spoke)");
    };
    let Some((_, tc_b)) = seg_cross(pv, p2(c_last)) else {
        return reject("split-crossing-count (second spoke)");
    };
    // Along the ring direction (r_k → r_k1), the side arc reaches C from
    // the r_k end through part1 ending at r_k; its q (on c_last's spoke)
    // must come FIRST: t_c(q_b) < t_c(q_a).
    if tc_b >= tc_a {
        return reject("split-crossing-order");
    }
    // Bulge-depth premise: v's overshoot past C within the mint's own
    // rim-slot sagitta (a near-tangency artifact, never a real crossing
    // the arrangement should have seen).
    let clen = (dc.0 * dc.0 + dc.1 * dc.1).sqrt();
    let overshoot = (cross2(dc, (pv.0 - pk.0, pv.1 - pk.1)) / clen).abs();
    match sagitta {
        Some(s) if overshoot <= s => {}
        _ => {
            return reject(&format!(
                "split-bulge-depth (overshoot {overshoot:e} vs sagitta {sagitta:?})"
            ))
        }
    }

    // ── 4. Mint q_a / q_b: exact rational UVs ON C. ──────────────────────
    let (Ok(ra), Ok(rb)) = (
        crate::coplanar_overlay::rat(tc_a),
        crate::coplanar_overlay::rat(tc_b),
    ) else {
        return reject("split-param-nonfinite");
    };
    let q_uv = |t: &RBig| -> ExactPoint2 {
        ExactPoint2 {
            x: &ka.x + &(t * &(&kb.x - &ka.x)),
            y: &ka.y + &(t * &(&kb.y - &ka.y)),
        }
    };
    let (qa_ex, qb_ex) = (q_uv(&ra), q_uv(&rb));
    let base = overlay.verts.len();
    let (qa_id, qb_id) = (base as u32, base as u32 + 1);
    for ex in [&qa_ex, &qb_ex] {
        let (ux, uy) = (ex.x.to_f64().value(), ex.y.to_f64().value());
        overlay.verts.push(Point2::new(ux, uy));
        overlay.exact_verts.push(ex.clone());
        coords.push(frame.lift(ux, uy));
        minted_mark.push(false);
        mergeable_mark.push(false);
    }
    let unwind = |overlay: &mut ClassifiedOverlay,
                  coords: &mut Vec<Point3>,
                  minted_mark: &mut Vec<bool>,
                  mergeable_mark: &mut Vec<bool>| {
        overlay.verts.truncate(base);
        overlay.exact_verts.truncate(base);
        coords.truncate(base);
        minted_mark.truncate(base);
        mergeable_mark.truncate(base);
    };
    if coords[qa_id as usize] == coords[qb_id as usize] {
        unwind(overlay, coords, minted_mark, mergeable_mark);
        return reject("split-degenerate-crossings");
    }

    // ── 5. The four sub-polygons (§11c) and their triangulations. ────────
    // Material: ring tails of the material run, closed along C's
    // (q_b → q_a) sub-segment — v belongs to the BULGE only; the C piece
    // between the crossings is the AOnly|Overlap class boundary (2-incident
    // post-commit), which is exactly B's face boundary there.
    let mut p_mat: Vec<u32> = Vec::with_capacity(n + 3);
    let mut i = mat_start;
    while i != side_start {
        p_mat.push(link[i].0);
        i = (i + 1) % n;
    }
    p_mat.push(c_last);
    p_mat.push(qb_id);
    p_mat.push(qa_id);
    // Side arc split at C: part1 = c_last..r_k, part2 = r_k1..c_first.
    let mut p_rem_out: Vec<u32> = Vec::new();
    let mut i = side_start;
    loop {
        p_rem_out.push(link[i].0);
        if i == e_c {
            break;
        }
        i = (i + 1) % n;
    }
    p_rem_out.push(qb_id);
    let mut p_rem_in: Vec<u32> = vec![qa_id];
    let mut i = (e_c + 1) % n;
    while i != mat_start {
        p_rem_in.push(link[i].0);
        i = (i + 1) % n;
    }
    p_rem_in.push(c_first);
    let p_bulge = [qa_id, qb_id, v];

    let build = || -> Result<Vec<([u32; 3], RegionClass)>, String> {
        let mut ears: Vec<([u32; 3], RegionClass)> = Vec::with_capacity(cavity.len() + 2);
        if !gate_tri_valid(&p_bulge, coords, frame) || gate_tri_degenerate(&p_bulge, coords) {
            return Err("split-bulge-invalid".into());
        }
        ears.push((p_bulge, bulge_cls));
        for (poly, cls, who) in [
            (&p_mat, mat_cls, "split-mat"),
            (&p_rem_out, side_cls, "split-rem-out"),
            (&p_rem_in, side_cls, "split-rem-in"),
        ] {
            match earclip_cavity_polygon(
                poly,
                &cavity,
                cls,
                coords,
                frame,
                edge_map,
                probe,
                &format!("vert {v} {who}"),
            ) {
                Ok(we) => ears.extend(we),
                Err(EarclipErr::NotSimple { .. }) => {
                    return Err(format!("{who} polygon not simple"));
                }
                Err(EarclipErr::Other(why)) => return Err(format!("{who}: {why}")),
            }
        }
        // Count invariant: remnants (k-gons summing to side-run size + 2
        // ears) + material ((mat+3)-gon → mat+1 ears) + the bulge = the
        // old cavity + 1 — the bulge is NEW territory (previously
        // exterior), the only cover the op adds.
        if ears.len() != cavity.len() + 1 {
            return Err(format!(
                "split ear count {} != cavity {} + 1",
                ears.len(),
                cavity.len()
            ));
        }
        Ok(ears)
    };
    let new_tris = match build() {
        Ok(e) => e,
        Err(why) => {
            unwind(overlay, coords, minted_mark, mergeable_mark);
            return reject(&why);
        }
    };

    // ── 6. Commit: overwrite the cavity slots + push the two extras. ─────
    let cavity: Vec<usize> = cavity.into_iter().collect();
    for &ti in &cavity {
        let t = overlay.tris[ti];
        for k in 0..3 {
            let kk = edge_key(t[k], t[(k + 1) % 3]);
            if let Some(e) = edge_map.get_mut(&kk) {
                e.retain(|&x| x != ti);
                if e.is_empty() {
                    edge_map.remove(&kk);
                }
            }
        }
    }
    // 1×1-overlay attribution conventions for the two pushed slots.
    let attr = |cls: RegionClass| -> (u32, u32) {
        (
            if matches!(cls, RegionClass::AOnly | RegionClass::Overlap) {
                0
            } else {
                u32::MAX
            },
            if matches!(cls, RegionClass::BOnly | RegionClass::Overlap) {
                0
            } else {
                u32::MAX
            },
        )
    };
    let mut slots: Vec<usize> = cavity.clone();
    for &(t, cls) in new_tris.iter().skip(cavity.len()) {
        let ti = overlay.tris.len();
        overlay.tris.push(t);
        overlay.class.push(cls);
        let (pa, pb) = attr(cls);
        overlay.poly_a.push(pa);
        overlay.poly_b.push(pb);
        slots.push(ti);
    }
    for (&ti, &(t, cls)) in slots.iter().zip(&new_tris) {
        overlay.tris[ti] = t;
        overlay.class[ti] = cls;
        for k in 0..3 {
            edge_map
                .entry(edge_key(t[k], t[(k + 1) % 3]))
                .or_default()
                .push(ti);
        }
    }
    // §11c step 3 (the A-leg): q_a/q_b join v's own rim override chain,
    // ordered by their exact projection parameter along v's sub-chord.
    // Consumed by `collect_ring_crossings` after the gate loop; the ladder
    // fails loudly if any extra finds no owning sub-chord there.
    for &qi in &[qa_id, qb_id] {
        let q_ex = &overlay.exact_verts[qi as usize];
        let dx = &own_e.x - &own_s.x;
        let dy = &own_e.y - &own_s.y;
        let len2 = &dx * &dx + &dy * &dy;
        let t = (&(&q_ex.x - &own_s.x) * &dx + &(&q_ex.y - &own_s.y) * &dy) / &len2;
        out_extras.push(ExtraRimPoint {
            s: own_s.clone(),
            e: own_e.clone(),
            t,
            pt: coords[qi as usize],
            // v's rim belongs to input A exactly when the OTHER input is B.
            side_a: other_is_b,
        });
    }
    if probe {
        eprintln!(
            "[fold-split] vert {v} chord ({r_k},{r_k1}) q=({qa_id},{qb_id}) \
             t_c=({tc_a},{tc_b}) overshoot={overshoot:e} cavity={} -> {} tris",
            cavity.len(),
            new_tris.len()
        );
    }
    true
}

#[cfg(test)]
mod reloc_tests {
    //! Amendment-5 cavity relocation unit oracles (spec
    //! `n2_stage4_junction_cluster_merge` §3, M8 increment 8). The F0087
    //! engine-frame chain exercises the ear-clip branch end-to-end
    //! (`kernel-v2/tests/m8_swiss_cheese_chain.rs`); these cover the
    //! remaining branch rows in isolation on synthetic triangulations
    //! (P4): fan-with-growth, pinch-defer → ear-clip, and reject with NO
    //! mutation. All fixtures live on the z=0 plane with the identity
    //! frame, so the resolved 3D coords ARE the 2D positions.

    use super::RelocOutcome;
    use super::{
        gate_tri_valid, relocate_minted_region, relocate_minted_vertex, Frame, RegionOutcome,
    };
    use crate::coplanar_overlay::RegionClass;
    use cad_primitives::Point3;
    use std::collections::BTreeMap;

    fn frame_z0() -> Frame {
        Frame {
            n: [0.0, 0.0, 1.0],
            d: 0.0,
            o: [0.0, 0.0, 0.0],
            e1: [1.0, 0.0, 0.0],
            e2: [0.0, 1.0, 0.0],
        }
    }

    fn p(u: f64, v: f64) -> Point3 {
        Point3::new(u, v, 0.0)
    }

    fn edge_map_of(tris: &[[u32; 3]]) -> BTreeMap<[u32; 2], Vec<usize>> {
        let key = |a: u32, b: u32| if a < b { [a, b] } else { [b, a] };
        let mut m: BTreeMap<[u32; 2], Vec<usize>> = BTreeMap::new();
        for (ti, t) in tris.iter().enumerate() {
            for k in 0..3 {
                m.entry(key(t[k], t[(k + 1) % 3])).or_default().push(ti);
            }
        }
        m
    }

    /// Incident-list ORDER is insertion-dependent and no consumer reads it;
    /// compare edge maps as sets.
    fn canon(m: &BTreeMap<[u32; 2], Vec<usize>>) -> BTreeMap<[u32; 2], Vec<usize>> {
        m.iter()
            .map(|(k, v)| {
                let mut v = v.clone();
                v.sort_unstable();
                (*k, v)
            })
            .collect()
    }

    /// Shared base fixture: v (=0) is a bottom-boundary vertex of the
    /// triangle-ish domain {w0(0,0), w2(4,0), far(2,2)} with one interior
    /// vertex w1(2,0.6). Star of v: (v,w2,w1), (v,w1,w0); non-star:
    /// (w0,w1,far), (w1,w2,far). v's resolved coordinate has been minted
    /// to (0.8, 0.4) — ACROSS the line through link edge (w1,w0), so the
    /// fan triangle (v,w1,w0) folds and the gate's flip repair cannot fix
    /// it (the fold is the only same-class neighbor configuration the
    /// fixture cares about; relocation is called directly here).
    fn base() -> (Vec<[u32; 3]>, Vec<RegionClass>, Vec<Point3>) {
        let tris = vec![[0, 2, 3], [0, 3, 1], [1, 3, 4], [3, 2, 4]];
        let class = vec![RegionClass::AOnly; 4];
        let coords = vec![
            p(0.8, 0.4),
            p(0.0, 0.0),
            p(4.0, 0.0),
            p(2.0, 0.6),
            p(2.0, 2.0),
        ];
        (tris, class, coords)
    }

    /// Branch row 1: all fan triangles valid after ONE visibility-growth
    /// step (the folded link edge (w1,w0) is crossed into its same-class
    /// neighbor, whose apex `far` joins the link) — the fan IS the
    /// re-triangulation and v keeps every spoke.
    #[test]
    fn fan_after_growth_retriangulates_star() {
        let (mut tris, mut class, coords) = base();
        let mut em = edge_map_of(&tris);
        let frame = frame_z0();
        assert!(
            !gate_tri_valid(&[0, 3, 1], &coords, &frame),
            "fixture must start folded"
        );
        let minted = vec![true, false, false, false, false];
        assert!(matches!(
            relocate_minted_vertex(
                &mut tris, &mut class, &mut em, 0, &coords, &frame, &minted, false
            ),
            RelocOutcome::Committed
        ));
        // Cavity slots (0,1,2 — star + grown neighbor) fan from v over the
        // final link w2→w1→far→w0; slot 3 untouched.
        assert_eq!(tris, vec![[0, 2, 3], [0, 3, 4], [0, 4, 1], [3, 2, 4]]);
        assert!(class.iter().all(|&c| c == RegionClass::AOnly));
        for t in &tris {
            assert!(
                gate_tri_valid(t, &coords, &frame),
                "{t:?} invalid after fan"
            );
        }
        assert_eq!(
            canon(&em),
            canon(&edge_map_of(&tris)),
            "edge map must be maintained incrementally"
        );
    }

    /// Branch row 3 (reject, no mutation): the same fold, but the growable
    /// neighbor is across a CLASS boundary (the intersection curve). Growth
    /// defers, and the cavity polygon [v,w2,w1,w0] is non-simple under the
    /// minted position (edge v→w2 crosses edge w1→w0), so the ear-clip
    /// rejects — the caller falls back to the amendment-2 revert.
    #[test]
    fn class_blocked_nonsimple_polygon_rejects_without_mutation() {
        let (mut tris, mut class, coords) = base();
        class[2] = RegionClass::Overlap; // (w0,w1,far) across the curve
        let tris0 = tris.clone();
        let class0 = class.clone();
        let mut em = edge_map_of(&tris);
        let em0 = em.clone();
        let frame = frame_z0();
        let minted = vec![true, false, false, false, false];
        let out = relocate_minted_vertex(
            &mut tris, &mut class, &mut em, 0, &coords, &frame, &minted, false,
        );
        let RelocOutcome::NonSimple {
            merge_candidate,
            split_chord,
            ..
        } = out
        else {
            panic!("fixture must reach the non-simple cavity polygon");
        };
        // Amendment-13 detectors on the [v,w2,w1,w0] ring (crossing
        // e0(v→w2) × e2(w1→w0)): the second sandwich mid (w0,v) carries
        // the one-mint pair — a candidate the LADDER's displacement guard
        // then refuses (gap 0.89 ≫ any rim-snap displacement) — and the
        // chord not touching v is (w1,w0), the Fig-11(a) split chord.
        assert!(
            matches!(merge_candidate, Some((1, 0, _, _))),
            "backtrack pair (w0, v): {merge_candidate:?}"
        );
        assert_eq!(split_chord, Some((3, 1)), "split chord (w1, w0)");
        assert_eq!(tris, tris0, "reject must not mutate triangles");
        assert_eq!(class, class0, "reject must not mutate classes");
        assert_eq!(em, em0, "reject must not mutate the edge map");
    }

    /// Branch row 2 (constrained ear-clip): a column-hop strip where growth
    /// pinches (the absorbable neighbor's apex is already on the link), so
    /// the fan is impossible, and the cavity polygon is ear-clipped instead
    /// — v loses its spokes to the hopped column but keeps its two domain-
    /// boundary edges, and the triangulation covers the cavity exactly.
    #[test]
    fn pinch_deferred_cavity_ear_clips() {
        // v(=0) minted to (2.2,-0.3), past the column {b(2,0.5), a(2,1.5)}.
        // Star: (w0,v,tl),(v,a,tl),(v,b,a),(v,w3,b); non-star: (b,w3,tr),
        // (a,b,tr),(tl,a,tr). Growth absorbs (a,b,tr), then the next fold's
        // neighbor (b,w3,tr) has apex w3 already on the link → defer.
        let mut tris = vec![
            [1, 0, 2], // (w0, v, tl)
            [0, 3, 2], // (v, a, tl)
            [0, 4, 3], // (v, b, a)
            [0, 5, 4], // (v, w3, b)
            [4, 5, 6], // (b, w3, tr)
            [3, 4, 6], // (a, b, tr)
            [2, 3, 6], // (tl, a, tr)
        ];
        let mut class = vec![RegionClass::AOnly; 7];
        let coords = vec![
            p(2.2, -0.3), // v (minted)
            p(0.0, 0.0),  // w0
            p(0.0, 2.0),  // tl
            p(2.0, 1.5),  // a
            p(2.0, 0.5),  // b
            p(4.0, 0.0),  // w3
            p(4.0, 2.0),  // tr
        ];
        let mut em = edge_map_of(&tris);
        let frame = frame_z0();
        assert!(
            !gate_tri_valid(&[0, 4, 3], &coords, &frame),
            "fixture must start folded"
        );
        let minted = vec![true, false, false, false, false, false, false];
        assert!(matches!(
            relocate_minted_vertex(
                &mut tris, &mut class, &mut em, 0, &coords, &frame, &minted, false
            ),
            RelocOutcome::Committed
        ));
        // Cavity = star (4) + absorbed (a,b,tr) = 5 tris, ear-clipped over
        // the polygon [v, w3, b, tr, a, tl, w0]. Slot 6 untouched.
        assert_eq!(tris[6], [2, 3, 6]);
        for t in &tris {
            assert!(
                gate_tri_valid(t, &coords, &frame),
                "{t:?} invalid after ear-clip"
            );
        }
        assert!(class.iter().all(|&c| c == RegionClass::AOnly));
        assert_eq!(
            canon(&em),
            canon(&edge_map_of(&tris)),
            "edge map must be maintained incrementally"
        );
        // v keeps its domain-boundary edges but no longer spokes to the
        // hopped column.
        let key = |a: u32, b: u32| if a < b { [a, b] } else { [b, a] };
        assert!(em.contains_key(&key(0, 1)) && em.contains_key(&key(0, 5)));
        assert!(
            !em.contains_key(&key(0, 4)),
            "spoke to hopped column must be gone"
        );
        // Exact cover: total unsigned area of the 7 triangles equals the
        // domain area (rect 4×2 minus the two boundary notches at v).
        let area = |t: &[u32; 3]| {
            let q = |i: u32| frame.project(coords[i as usize]);
            let (p0, p1, p2) = (q(t[0]), q(t[1]), q(t[2]));
            ((p1.0 - p0.0) * (p2.1 - p0.1) - (p1.1 - p0.1) * (p2.0 - p0.0)) / 2.0
        };
        let total: f64 = tris.iter().map(|t| area(t)).sum();
        // Rect 8.0 plus the dip of the boundary V at v below y=0:
        // triangle (w0, v, w3) area = 0.5·base(4)·depth(0.3) = 0.6.
        assert!((total - 8.6).abs() < 1e-12, "cover area {total} != 8.6");
    }

    // ── Amendment 6: joint region relocation (M8 increment 9) ────────────

    /// Region success: the pinch fixture's star-union region (single seed —
    /// the region form must subsume the per-vertex scope) has the closed
    /// boundary cycle [v, w3, b, a, tl, w0], simple and CCW at the minted
    /// position, and ear-clips into exactly region-size triangles with the
    /// edge map maintained. Non-region slots untouched.
    #[test]
    fn region_relocation_retriangulates_star_union() {
        let mut tris = vec![
            [1, 0, 2], // (w0, v, tl)
            [0, 3, 2], // (v, a, tl)
            [0, 4, 3], // (v, b, a)
            [0, 5, 4], // (v, w3, b)
            [4, 5, 6], // (b, w3, tr)
            [3, 4, 6], // (a, b, tr)
            [2, 3, 6], // (tl, a, tr)
        ];
        let mut class = vec![RegionClass::AOnly; 7];
        let coords = vec![
            p(2.2, -0.3), // v (minted)
            p(0.0, 0.0),  // w0
            p(0.0, 2.0),  // tl
            p(2.0, 1.5),  // a
            p(2.0, 0.5),  // b
            p(4.0, 0.0),  // w3
            p(4.0, 2.0),  // tr
        ];
        let mut em = edge_map_of(&tris);
        let frame = frame_z0();
        assert!(
            !gate_tri_valid(&[0, 4, 3], &coords, &frame),
            "fixture must start folded"
        );
        let mut minted = vec![false; coords.len()];
        minted[0] = true;
        assert!(matches!(
            relocate_minted_region(
                &mut tris,
                &mut class,
                &mut em,
                &[0],
                &coords,
                &frame,
                &minted,
                false
            ),
            RegionOutcome::Committed
        ));
        // Non-region slots untouched.
        assert_eq!(tris[4], [4, 5, 6]);
        assert_eq!(tris[5], [3, 4, 6]);
        assert_eq!(tris[6], [2, 3, 6]);
        for t in &tris {
            assert!(
                gate_tri_valid(t, &coords, &frame),
                "{t:?} invalid after region relocation"
            );
        }
        assert!(class.iter().all(|&c| c == RegionClass::AOnly));
        assert_eq!(
            canon(&em),
            canon(&edge_map_of(&tris)),
            "edge map must be maintained incrementally"
        );
        // Exact cover unchanged (same domain as the per-vertex fixture).
        let area = |t: &[u32; 3]| {
            let q = |i: u32| frame.project(coords[i as usize]);
            let (p0, p1, p2) = (q(t[0]), q(t[1]), q(t[2]));
            ((p1.0 - p0.0) * (p2.1 - p0.1) - (p1.1 - p0.1) * (p2.0 - p0.0)) / 2.0
        };
        let total: f64 = tris.iter().map(|t| area(t)).sum();
        assert!((total - 8.6).abs() < 1e-12, "cover area {total} != 8.6");
    }

    /// Amendment 8 (was: `region_nonsimple_cycle_rejects_without_mutation`,
    /// which pinned the amendment-6 limitation this amendment removes): the
    /// base fixture's single-seed region boundary [v, w2, w1, w0] is
    /// exactly NON-SIMPLE at the minted position (edge v→w2 crosses edge
    /// w1→w0) — the region now GROWS across the crossing edge into its
    /// same-class neighbor and commits, all replacement triangles
    /// gate-valid with the exact cover preserved.
    #[test]
    fn region_nonsimple_cycle_grows_to_simplicity() {
        let (mut tris, mut class, coords) = base();
        let mut em = edge_map_of(&tris);
        let frame = frame_z0();
        let mut minted = vec![false; coords.len()];
        minted[0] = true;
        assert!(matches!(
            relocate_minted_region(
                &mut tris,
                &mut class,
                &mut em,
                &[0],
                &coords,
                &frame,
                &minted,
                false
            ),
            RegionOutcome::Committed
        ));
        for t in &tris {
            assert!(
                gate_tri_valid(t, &coords, &frame),
                "{t:?} invalid after grown region relocation"
            );
        }
        assert!(class.iter().all(|&c| c == RegionClass::AOnly));
        assert_eq!(
            canon(&em),
            canon(&edge_map_of(&tris)),
            "edge map must be maintained incrementally"
        );
        // Exact cover: the domain polygon w0(0,0) → v(0.8,0.4) → w2(4,0) →
        // far(2,2) has area 3.2, and every replacement triangle is
        // positive, so the sum doubles as a no-overlap certificate.
        let area = |t: &[u32; 3]| {
            let q = |i: u32| frame.project(coords[i as usize]);
            let (p0, p1, p2) = (q(t[0]), q(t[1]), q(t[2]));
            ((p1.0 - p0.0) * (p2.1 - p0.1) - (p1.1 - p0.1) * (p2.0 - p0.0)) / 2.0
        };
        let total: f64 = tris.iter().map(|t| area(t)).sum();
        assert!((total - 3.2).abs() < 1e-12, "cover area {total} != 3.2");
    }

    /// Amendment 8 reject, no mutation: the same non-simple boundary, but
    /// every neighbor beyond the crossing edges is across the intersection
    /// curve (class boundary) — growth is blocked on both sides, the
    /// sub-region rejects, and nothing is mutated. Amendment 13: the
    /// combinatorial backtrack detector legitimately SURFACES the (w0, v)
    /// sandwich here as a candidate — but this shape is a large fold, not
    /// a hair backtrack, and the LADDER's displacement guard
    /// (‖p−q‖ = 0.89 vs a rim-snap-scale displacement) plus the
    /// provenance mask refuse the merge there; the amendment-2 revert
    /// stays the fallback.
    #[test]
    fn region_nonsimple_ungrowable_rejects_without_mutation() {
        let (mut tris, mut class, coords) = base();
        // Both non-star triangles across the curve: the folded AOnly
        // sub-region is the star {0,1}; its crossing edges' external
        // neighbors (tris 2 and 3) are Overlap — ungrowable.
        class[2] = RegionClass::Overlap;
        class[3] = RegionClass::Overlap;
        let tris0 = tris.clone();
        let class0 = class.clone();
        let mut em = edge_map_of(&tris);
        let em0 = em.clone();
        let frame = frame_z0();
        let mut minted = vec![false; coords.len()];
        minted[0] = true;
        assert!(matches!(
            relocate_minted_region(
                &mut tris,
                &mut class,
                &mut em,
                &[0],
                &coords,
                &frame,
                &minted,
                false
            ),
            RegionOutcome::MergeCandidate { p: 1, q: 0, .. }
        ));
        assert_eq!(tris, tris0, "reject must not mutate triangles");
        assert_eq!(class, class0, "reject must not mutate classes");
        assert_eq!(em, em0, "reject must not mutate the edge map");
    }

    /// Amendment 7 boundary: a multi-class region whose folded class
    /// sub-region is a SINGLE triangle still rejects without mutation —
    /// the partition never re-triangulates across the class boundary, and
    /// a one-triangle sub-region has no alternative triangulation
    /// (`region too small`). The caller's amendment-2 revert stays the
    /// loud fallback.
    #[test]
    fn region_multiclass_rejects_without_mutation() {
        let (mut tris, mut class, coords) = base();
        class[1] = RegionClass::Overlap; // second star triangle across the curve
        let tris0 = tris.clone();
        let class0 = class.clone();
        let mut em = edge_map_of(&tris);
        let em0 = em.clone();
        let frame = frame_z0();
        let mut minted = vec![false; coords.len()];
        minted[0] = true;
        assert!(matches!(
            relocate_minted_region(
                &mut tris,
                &mut class,
                &mut em,
                &[0],
                &coords,
                &frame,
                &minted,
                false
            ),
            RegionOutcome::Rejected
        ));
        assert_eq!(tris, tris0, "reject must not mutate triangles");
        assert_eq!(class, class0, "reject must not mutate classes");
        assert_eq!(em, em0, "reject must not mutate the edge map");
    }

    // ── Amendment 7: class-partitioned joint region (M8 increment 10) ────

    /// A multi-class star union (the F0089/F0090 signature: the mint sits
    /// ON the intersection curve, so its star straddles the class
    /// boundary): the FOLDED class sub-region is re-triangulated
    /// independently while the valid sub-region across the curve is left
    /// untouched, and the class-boundary edge survives as sub-region
    /// boundary.
    #[test]
    fn region_multiclass_folded_subregion_relocates_partitioned() {
        // The star-union fixture with the (w0, v, tl) star triangle moved
        // across the intersection curve (Overlap). At v's minted position
        // that triangle is VALID; the fold lives in the AOnly sub-region
        // {(v,a,tl), (v,b,a), (v,w3,b)}.
        let mut tris = vec![
            [1, 0, 2], // (w0, v, tl) — Overlap, valid, must stay untouched
            [0, 3, 2], // (v, a, tl)
            [0, 4, 3], // (v, b, a) — folded at the minted position
            [0, 5, 4], // (v, w3, b)
            [4, 5, 6], // (b, w3, tr) — non-region
            [3, 4, 6], // (a, b, tr) — non-region
            [2, 3, 6], // (tl, a, tr) — non-region
        ];
        let mut class = vec![RegionClass::AOnly; 7];
        class[0] = RegionClass::Overlap;
        let coords = vec![
            p(2.2, -0.3), // v (minted)
            p(0.0, 0.0),  // w0
            p(0.0, 2.0),  // tl
            p(2.0, 1.5),  // a
            p(2.0, 0.5),  // b
            p(4.0, 0.0),  // w3
            p(4.0, 2.0),  // tr
        ];
        let mut em = edge_map_of(&tris);
        let frame = frame_z0();
        assert!(
            !gate_tri_valid(&[0, 4, 3], &coords, &frame),
            "fixture must start folded in the AOnly sub-region"
        );
        assert!(
            gate_tri_valid(&[1, 0, 2], &coords, &frame),
            "the Overlap sub-region must start valid (it is skipped)"
        );
        let mut minted = vec![false; coords.len()];
        minted[0] = true;
        assert!(matches!(
            relocate_minted_region(
                &mut tris,
                &mut class,
                &mut em,
                &[0],
                &coords,
                &frame,
                &minted,
                false
            ),
            RegionOutcome::Committed
        ));
        // The Overlap sub-region and the non-region slots are untouched.
        assert_eq!(tris[0], [1, 0, 2]);
        assert_eq!(class[0], RegionClass::Overlap);
        assert_eq!(tris[4], [4, 5, 6]);
        assert_eq!(tris[5], [3, 4, 6]);
        assert_eq!(tris[6], [2, 3, 6]);
        // The AOnly sub-region slots (1..=3) are re-triangulated valid.
        for ti in 1..=3 {
            assert!(
                gate_tri_valid(&tris[ti], &coords, &frame),
                "{:?} invalid after partitioned relocation",
                tris[ti]
            );
            assert_eq!(class[ti], RegionClass::AOnly);
        }
        // The class-boundary edge (v, tl) — the intersection curve — is
        // preserved with both sides intact.
        assert_eq!(
            em.get(&[0, 2]).map(|e| e.len()),
            Some(2),
            "class-boundary edge (v,tl) must survive the partition"
        );
        assert_eq!(
            canon(&em),
            canon(&edge_map_of(&tris)),
            "edge map must be maintained incrementally"
        );
        // Exact cover unchanged (same total domain as the star-union
        // fixture: rect 8.0 + the boundary-V dip 0.6).
        let area = |t: &[u32; 3]| {
            let q = |i: u32| frame.project(coords[i as usize]);
            let (p0, p1, p2) = (q(t[0]), q(t[1]), q(t[2]));
            ((p1.0 - p0.0) * (p2.1 - p0.1) - (p1.1 - p0.1) * (p2.0 - p0.0)) / 2.0
        };
        let total: f64 = tris.iter().map(|t| area(t)).sum();
        assert!((total - 8.6).abs() < 1e-12, "cover area {total} != 8.6");
    }

    /// Amendment 7 termination gate: a VALID-ONLY class sub-region is
    /// skipped even when another sub-region commits — re-triangulating a
    /// fold-free sub-region could churn the mesh without reducing the
    /// gate's folded count.
    #[test]
    fn region_validonly_subregion_is_skipped() {
        let mut tris = vec![
            [1, 0, 2], // (w0, v, tl) — Overlap, valid
            [0, 3, 2], // (v, a, tl)
            [0, 4, 3], // (v, b, a) — folded
            [0, 5, 4], // (v, w3, b)
        ];
        let mut class = vec![
            RegionClass::Overlap,
            RegionClass::AOnly,
            RegionClass::AOnly,
            RegionClass::AOnly,
        ];
        let coords = vec![
            p(2.2, -0.3),
            p(0.0, 0.0),
            p(0.0, 2.0),
            p(2.0, 1.5),
            p(2.0, 0.5),
            p(4.0, 0.0),
        ];
        let mut em = edge_map_of(&tris);
        let frame = frame_z0();
        let overlap_before = (tris[0], class[0]);
        let mut minted = vec![false; coords.len()];
        minted[0] = true;
        assert!(matches!(
            relocate_minted_region(
                &mut tris,
                &mut class,
                &mut em,
                &[0],
                &coords,
                &frame,
                &minted,
                false
            ),
            RegionOutcome::Committed
        ));
        assert_eq!(
            (tris[0], class[0]),
            overlap_before,
            "valid-only Overlap sub-region must be skipped, not re-triangulated"
        );
    }

    /// Amendment 10 (M8 increment 13): the joint seeds surfaced by a
    /// NON-SIMPLE cavity polygon are the mints on the CROSSING edges only —
    /// the interacting set per Fig-11 locality — not every mint on the
    /// ring. A 40+-edge ring around a hole lists ~30 mints; seeding them
    /// all inflates the star union into an ANNULUS (measured F0090 ~cut
    /// 22: 2 boundary cycles [32, 20]) that no single boundary walk can
    /// triangulate. Here: the ring [v, w2, w1, w1b, w0] has its (first)
    /// exact crossing at v→w2 × w1b→w0; the minted vertex w1 sits on the
    /// ring but NOT on the crossing — it must not become a joint seed.
    #[test]
    fn nonsimple_ring_mints_narrow_to_crossing_endpoints() {
        // v(=0) minted; star of three triangles, no external neighbors
        // (every link edge is domain boundary ⇒ growth defers), single
        // class, open chain. Fan tri (v,w1b,w0) is invalid at the minted
        // position and the polygon crosses exactly at v→w2 × w1b→w0.
        let mut tris = vec![[0, 2, 3], [0, 3, 4], [0, 4, 1]];
        let mut class = vec![RegionClass::AOnly; 3];
        let coords = vec![
            p(0.8, 0.4), // v (minted)
            p(0.0, 0.0), // w0
            p(4.0, 0.0), // w2
            p(2.0, 0.6), // w1 (minted, NOT on the crossing)
            p(1.2, 0.5), // w1b (minted, crossing endpoint)
        ];
        let mut em = edge_map_of(&tris);
        let frame = frame_z0();
        assert!(
            !gate_tri_valid(&[0, 4, 1], &coords, &frame),
            "fixture must start folded at (v,w1b,w0)"
        );
        let minted = vec![true, false, false, true, true];
        let out = relocate_minted_vertex(
            &mut tris, &mut class, &mut em, 0, &coords, &frame, &minted, false,
        );
        let RelocOutcome::NonSimple { ring_mints, .. } = out else {
            panic!("fixture must reach the non-simple cavity polygon");
        };
        assert_eq!(
            ring_mints,
            vec![4],
            "joint seeds must be the crossing-edge mints only (w1b), not \
             every ring mint (w1 excluded)"
        );
    }

    /// Amendment 11 (M8 increment 14): a NET-CW BOW-TIE cavity polygon —
    /// non-simple with the inverted lobe dominating the signed area
    /// (measured F0088 vert 674: a hair-thin full-height strip whose long
    /// return edge crosses the up-chain; net 2A = −4.2e-3) — must surface
    /// as `NonSimple` (the joint trigger), not die at the orientation
    /// guard. Simplicity is checked BEFORE orientation: a crossing makes
    /// the signed area lobe-balance noise.
    #[test]
    fn net_cw_bowtie_cavity_triggers_joint_path() {
        // Star of v: link chain [a, b, c]; polygon [v, a, b, c] has edge
        // v→a crossing edge b→c at (0.75, 0.375) and net 2A = −1.5 (CW).
        let mut tris = vec![[0, 1, 2], [0, 2, 3]];
        let mut class = vec![RegionClass::AOnly; 2];
        let coords = vec![
            p(0.0, 0.0), // v (minted)
            p(2.0, 1.0), // a (minted, crossing endpoint)
            p(3.0, 0.0), // b
            p(0.0, 0.5), // c
        ];
        let mut em = edge_map_of(&tris);
        let frame = frame_z0();
        assert!(
            !gate_tri_valid(&[0, 1, 2], &coords, &frame),
            "fixture must start folded at (v,a,b)"
        );
        let minted = vec![true, true, false, false];
        let out = relocate_minted_vertex(
            &mut tris, &mut class, &mut em, 0, &coords, &frame, &minted, false,
        );
        let RelocOutcome::NonSimple { ring_mints, .. } = out else {
            panic!(
                "net-CW bow-tie must surface NonSimple (joint trigger), \
                 not a terminal orientation reject"
            );
        };
        assert_eq!(
            ring_mints,
            vec![1],
            "the crossing-endpoint mint must be surfaced as a joint seed"
        );
    }

    // ── Amendment 9: connected-component split (M8 increment 12) ─────────

    /// A DISCONNECTED class sub-region (the F0090 33-seed signature: the
    /// joint trigger accumulates seeds from several separate strips): each
    /// edge-connected component is relocated independently — one boundary
    /// walk per component, not one for the union.
    #[test]
    fn region_disconnected_components_relocate_independently() {
        // Two disjoint copies of the base fixture (the second offset by
        // +10 in u), both folded, seeds one vertex from each.
        let (t1, c1, p1) = base();
        let mut tris = t1.clone();
        let mut class = c1.clone();
        let mut coords = p1.clone();
        let off = p1.len() as u32;
        for t in &t1 {
            tris.push([t[0] + off, t[1] + off, t[2] + off]);
        }
        class.extend(c1.iter().copied());
        for q in &p1 {
            coords.push(p(frame_z0().project(*q).0 + 10.0, frame_z0().project(*q).1));
        }
        let mut em = edge_map_of(&tris);
        let frame = frame_z0();
        assert!(
            !gate_tri_valid(&tris[1], &coords, &frame)
                && !gate_tri_valid(&tris[4 + 1], &coords, &frame),
            "both copies must start folded"
        );
        let mut minted = vec![false; coords.len()];
        minted[0] = true;
        minted[off as usize] = true;
        assert!(matches!(
            relocate_minted_region(
                &mut tris,
                &mut class,
                &mut em,
                &[0, off],
                &coords,
                &frame,
                &minted,
                false
            ),
            RegionOutcome::Committed
        ));
        for t in &tris {
            assert!(
                gate_tri_valid(t, &coords, &frame),
                "{t:?} invalid after component-split relocation"
            );
        }
        assert_eq!(
            canon(&em),
            canon(&edge_map_of(&tris)),
            "edge map must be maintained incrementally"
        );
        // Exact cover per copy (boundary-determined 3.2 each, all ears
        // positive ⇒ no overlap).
        let area = |t: &[u32; 3]| {
            let q = |i: u32| frame.project(coords[i as usize]);
            let (q0, q1, q2) = (q(t[0]), q(t[1]), q(t[2]));
            ((q1.0 - q0.0) * (q2.1 - q0.1) - (q1.1 - q0.1) * (q2.0 - q0.0)) / 2.0
        };
        let total: f64 = tris.iter().map(|t| area(t)).sum();
        assert!((total - 6.4).abs() < 1e-12, "cover area {total} != 6.4");
    }

    // ── Amendment 12: per-class wedge decomposition ──────────────────────
    //
    // Spec `m8_stage0_multiclass_cavity_arm` §3/§4 (ALWAYS-ON since the
    // inc-2 flip; the inc-1 env gate measured zero corpus category changes
    // and was removed with it). NOTE on the spec's fixture (a) "2 wedges,
    // both fan": that shape is UNREACHABLE — the deferred path only runs
    // when some link edge kept an invalid fan triangle (growth defers on
    // nothing else), and that edge's wedge can never fan, so every
    // reachable wedge decomposition ear-clips at least one wedge. Fixtures
    // (a)/(b) therefore share the minimal reachable form: the folded wedge
    // ear-clips while the other wedge fans.

    /// Shared amendment-12 fixture: the pinch fixture's geometry with the
    /// star SPLIT at spoke (v, tl) — star tris (v,a,tl), (v,b,a), (v,w3,b)
    /// and the growth/pinch blockers are BOnly; (w0,v,tl) and the far
    /// non-star (tl,a,tr) are AOnly. Spoke (v,tl) is the intersection
    /// polyline through the mint. At v's minted position (2.2,−0.3) the
    /// fold sits in wedge B ((v,b,a) — the column hop), growth absorbs
    /// (a,b,tr) then pinch-defers at (b,tr), and the wedge decomposition
    /// must ear-clip wedge B over [v,w3,b,tr,a,tl] while wedge A fans its
    /// single valid triangle [v,tl,w0].
    fn wedge_base() -> (Vec<[u32; 3]>, Vec<RegionClass>, Vec<Point3>) {
        let tris = vec![
            [1, 0, 2], // (w0, v, tl) — wedge A
            [0, 3, 2], // (v, a, tl)
            [0, 4, 3], // (v, b, a) — folded at the minted position
            [0, 5, 4], // (v, w3, b)
            [4, 5, 6], // (b, w3, tr) — pinch blocker (non-star)
            [3, 4, 6], // (a, b, tr) — absorbed by growth (non-star)
            [2, 3, 6], // (tl, a, tr) — untouched non-star
        ];
        let mut class = vec![RegionClass::BOnly; 7];
        class[0] = RegionClass::AOnly;
        class[6] = RegionClass::AOnly;
        let coords = vec![
            p(2.2, -0.3), // v (minted)
            p(0.0, 0.0),  // w0
            p(0.0, 2.0),  // tl
            p(2.0, 1.5),  // a
            p(2.0, 0.5),  // b
            p(4.0, 0.0),  // w3
            p(4.0, 2.0),  // tr
        ];
        (tris, class, coords)
    }

    /// Signed area helper shared by the wedge exact-cover oracles.
    fn tri_area2d(t: &[u32; 3], coords: &[Point3], frame: &Frame) -> f64 {
        let q = |i: u32| frame.project(coords[i as usize]);
        let (p0, p1, p2) = (q(t[0]), q(t[1]), q(t[2]));
        ((p1.0 - p0.0) * (p2.1 - p0.1) - (p1.1 - p0.1) * (p2.0 - p0.0)) / 2.0
    }

    /// Fixtures (a)+(b): boundary mint, 2 wedges — the folded wedge
    /// ear-clips while the other wedge fans; commit with the exact-cover
    /// oracle and the conformality invariant (§3b): the transition spoke
    /// (v,tl) — the intersection polyline through the mint, at its CURRENT
    /// position — survives into the result with exactly one triangle per
    /// side, and each side carries its own class.
    #[test]
    fn wedge_boundary_mint_earclips_folded_wedge_fans_other() {
        let (mut tris, mut class, coords) = wedge_base();
        let mut em = edge_map_of(&tris);
        let frame = frame_z0();
        assert!(
            !gate_tri_valid(&[0, 4, 3], &coords, &frame),
            "fixture must start folded in wedge B"
        );
        let minted = vec![true, false, false, false, false, false, false];
        assert!(matches!(
            relocate_minted_vertex(
                &mut tris, &mut class, &mut em, 0, &coords, &frame, &minted, false
            ),
            RelocOutcome::Committed
        ));
        // Non-cavity slots untouched (cavity = star {0,1,2,3} + absorbed 5).
        assert_eq!(tris[4], [4, 5, 6]);
        assert_eq!(class[4], RegionClass::BOnly);
        assert_eq!(tris[6], [2, 3, 6]);
        assert_eq!(class[6], RegionClass::AOnly);
        for t in &tris {
            assert!(
                gate_tri_valid(t, &coords, &frame),
                "{t:?} invalid after wedge relocation"
            );
        }
        assert_eq!(
            canon(&em),
            canon(&edge_map_of(&tris)),
            "edge map must be maintained incrementally"
        );
        // Conformality by shared spoke identity: the class-transition spoke
        // (v,tl) has exactly one incident triangle per side, each in its
        // wedge's class.
        let key = |a: u32, b: u32| if a < b { [a, b] } else { [b, a] };
        let inc = em.get(&key(0, 2)).expect("spoke (v,tl) must survive");
        assert_eq!(inc.len(), 2, "spoke (v,tl) must have one side per wedge");
        let side_classes: Vec<RegionClass> = inc.iter().map(|&ti| class[ti]).collect();
        assert!(
            side_classes.contains(&RegionClass::AOnly)
                && side_classes.contains(&RegionClass::BOnly),
            "the intersection polyline must separate the two classes: {side_classes:?}"
        );
        // The wedge-A fan keeps v's spoke to w0 (domain end); the B-side
        // ear-clip re-hangs a's triangles off other link vertices.
        assert!(em.contains_key(&key(0, 1)), "domain spoke (v,w0) kept");
        assert!(
            !em.contains_key(&key(0, 3)),
            "spoke (v,a) must be re-hung by the wedge-B ear-clip"
        );
        // Class accounting: the replacement carries 4 BOnly ears + 1 AOnly
        // fan triangle (Σ per-wedge ears = grown-cavity size).
        let b_count = class.iter().filter(|&&c| c == RegionClass::BOnly).count();
        assert_eq!(b_count, 5, "4 B ears + untouched blocker: {class:?}");
        // Exact cover: same domain as the pinch fixture (rect 8.0 + the
        // boundary-V dip 0.6); every triangle positive ⇒ no overlap.
        for t in &tris {
            assert!(tri_area2d(t, &coords, &frame) > 0.0);
        }
        let total: f64 = tris.iter().map(|t| tri_area2d(t, &coords, &frame)).sum();
        assert!((total - 8.6).abs() < 1e-12, "cover area {total} != 8.6");
    }

    /// Fixture (c): INTERIOR on-curve mint — closed link, exactly 2 class
    /// transitions (the intersection polyline runs through spokes (v,w0)
    /// and (v,w3)). The B wedge ear-clips the proven 7-gon while the A
    /// wedge fans; both curve spokes survive with one triangle per side.
    /// Pre-amendment this was the `interior vertex with constraint-blocked
    /// fan` reject — the census's dominant (100% 2-transition) class.
    #[test]
    fn wedge_interior_oncurve_closed_link_commits() {
        let (mut tris, mut class, mut coords) = wedge_base();
        // Close the link below the curve: dn(2,−1.5), lower star AOnly.
        coords.push(p(2.0, -1.5)); // dn = 7
        tris.push([0, 1, 7]); // (v, w0, dn)
        tris.push([0, 7, 5]); // (v, dn, w3)
        class.push(RegionClass::AOnly);
        class.push(RegionClass::AOnly);
        // One class per curve side: (w0,v,tl) joins wedge B above the
        // curve; the non-star (tl,a,tr) is irrelevant to the star walk.
        class[0] = RegionClass::BOnly;
        let mut em = edge_map_of(&tris);
        let frame = frame_z0();
        assert!(
            !gate_tri_valid(&[0, 4, 3], &coords, &frame),
            "fixture must start folded in wedge B"
        );
        let minted = vec![true, false, false, false, false, false, false, false];
        assert!(matches!(
            relocate_minted_vertex(
                &mut tris, &mut class, &mut em, 0, &coords, &frame, &minted, false
            ),
            RelocOutcome::Committed
        ));
        assert_eq!(tris[4], [4, 5, 6], "non-cavity slots untouched");
        assert_eq!(tris[6], [2, 3, 6]);
        for t in &tris {
            assert!(
                gate_tri_valid(t, &coords, &frame),
                "{t:?} invalid after closed-link wedge relocation"
            );
        }
        assert_eq!(
            canon(&em),
            canon(&edge_map_of(&tris)),
            "edge map must be maintained incrementally"
        );
        // Both curve spokes survive, one side per class — the two-sided
        // form through an interior mint.
        let key = |a: u32, b: u32| if a < b { [a, b] } else { [b, a] };
        for spoke in [key(0, 1), key(0, 5)] {
            let inc = em.get(&spoke).expect("curve spoke must survive");
            assert_eq!(inc.len(), 2, "curve spoke {spoke:?} needs both sides");
            let cs: Vec<RegionClass> = inc.iter().map(|&ti| class[ti]).collect();
            assert!(
                cs.contains(&RegionClass::AOnly) && cs.contains(&RegionClass::BOnly),
                "spoke {spoke:?} must separate the classes: {cs:?}"
            );
        }
        // Exact cover: the boundary-mint domain (8.6) plus the lower quad
        // (w0, dn, w3, v) = 2.4.
        for t in &tris {
            assert!(tri_area2d(t, &coords, &frame) > 0.0);
        }
        let total: f64 = tris.iter().map(|t| tri_area2d(t, &coords, &frame)).sum();
        assert!((total - 11.0).abs() < 1e-12, "cover area {total} != 11.0");
    }

    /// Fixture (d): JUNCTION mint — >2 constraint spokes at v (two class
    /// transitions plus two domain ends), 3 wedges, same walk with no
    /// special case. Wedge B ear-clips; the A and Overlap wedges fan one
    /// triangle each; both transition spokes keep one side per wedge.
    #[test]
    fn wedge_junction_three_wedges_commit() {
        let (mut tris, mut class, mut coords) = wedge_base();
        // Extend the chain past w0: bl(−1,0), star tri (v,w0,bl) Overlap.
        coords.push(p(-1.0, 0.0)); // bl = 7
        tris.push([0, 1, 7]); // (v, w0, bl)
        class.push(RegionClass::Overlap);
        let mut em = edge_map_of(&tris);
        let frame = frame_z0();
        let minted = vec![true, false, false, false, false, false, false, false];
        assert!(matches!(
            relocate_minted_vertex(
                &mut tris, &mut class, &mut em, 0, &coords, &frame, &minted, false
            ),
            RelocOutcome::Committed
        ));
        for t in &tris {
            assert!(
                gate_tri_valid(t, &coords, &frame),
                "{t:?} invalid after junction wedge relocation"
            );
        }
        assert_eq!(
            canon(&em),
            canon(&edge_map_of(&tris)),
            "edge map must be maintained incrementally"
        );
        // Both transition spokes survive with exactly two sides.
        let key = |a: u32, b: u32| if a < b { [a, b] } else { [b, a] };
        assert_eq!(em.get(&key(0, 2)).map(|e| e.len()), Some(2), "(v,tl)");
        assert_eq!(em.get(&key(0, 1)).map(|e| e.len()), Some(2), "(v,w0)");
        // One triangle per 1-entry wedge, in its own class.
        assert_eq!(
            class.iter().filter(|&&c| c == RegionClass::AOnly).count(),
            2,
            "wedge-A fan + untouched (tl,a,tr): {class:?}"
        );
        assert_eq!(
            class.iter().filter(|&&c| c == RegionClass::Overlap).count(),
            1,
            "the Overlap wedge fans exactly its own triangle: {class:?}"
        );
    }

    /// Fixture (e): a 1-triangle wedge that is VALID at the minted
    /// coordinates fans trivially — its triangle appears verbatim in the
    /// committed result (wedge A of the shared fixture).
    #[test]
    fn wedge_single_triangle_valid_wedge_fans_trivially() {
        let (mut tris, mut class, coords) = wedge_base();
        let mut em = edge_map_of(&tris);
        let frame = frame_z0();
        let minted = vec![true, false, false, false, false, false, false];
        assert!(matches!(
            relocate_minted_vertex(
                &mut tris, &mut class, &mut em, 0, &coords, &frame, &minted, false
            ),
            RelocOutcome::Committed
        ));
        let fan_slot = tris
            .iter()
            .position(|t| *t == [0, 2, 1])
            .expect("wedge A's 1-triangle fan [v,tl,w0] must appear verbatim");
        assert_eq!(class[fan_slot], RegionClass::AOnly);
    }

    /// Fixture (f): a 1-triangle FOLDED wedge is ungrowable (its only
    /// link edge is blocked by the intersection curve) and its 3-gon
    /// ear-clip cannot invert the fold — the wedge rejects LOUDLY with no
    /// mutation and the caller's amendment-2 revert stays the fallback.
    #[test]
    fn wedge_single_triangle_folded_wedge_rejects_without_mutation() {
        let (mut tris, mut class, coords) = base();
        // Split the base fixture at spoke (v,w1): (v,w2,w1) stays AOnly,
        // the folded (v,w1,w0) becomes BOnly, and the external neighbor
        // (w0,w1,far) stays AOnly — a class boundary, so growth defers.
        class[1] = RegionClass::BOnly;
        let tris0 = tris.clone();
        let class0 = class.clone();
        let mut em = edge_map_of(&tris);
        let em0 = em.clone();
        let frame = frame_z0();
        assert!(
            !gate_tri_valid(&[0, 3, 1], &coords, &frame),
            "fixture must start folded"
        );
        let minted = vec![true, false, false, false, false];
        assert!(matches!(
            relocate_minted_vertex(
                &mut tris, &mut class, &mut em, 0, &coords, &frame, &minted, false
            ),
            RelocOutcome::Rejected
        ));
        assert_eq!(tris, tris0, "reject must not mutate triangles");
        assert_eq!(class, class0, "reject must not mutate classes");
        assert_eq!(em, em0, "reject must not mutate the edge map");
    }

    /// Fixture (g): a NON-SIMPLE wedge polygon propagates
    /// `RelocOutcome::NonSimple` with the amendment-10 crossing-narrowed
    /// seeds — the mints on THIS wedge's crossing edges only. Here the
    /// wedge-B polygon [v,w3,b,tr,a] crosses at (w3→b) × (a→v): the minted
    /// `a` is a seed; the minted `tr` sits on the ring but NOT on the
    /// crossing and must be excluded.
    #[test]
    fn wedge_nonsimple_propagates_crossing_narrowed_ring_mints() {
        let (mut tris, mut class, coords) = wedge_base();
        // Third class beyond spoke (v,a): wedge B loses tl, so its polygon
        // closes a→v and crosses its own chords under the minted position.
        class[1] = RegionClass::Overlap; // (v,a,tl)
        let tris0 = tris.clone();
        let mut em = edge_map_of(&tris);
        let frame = frame_z0();
        let minted = vec![true, false, false, true, false, false, true];
        let out = relocate_minted_vertex(
            &mut tris, &mut class, &mut em, 0, &coords, &frame, &minted, false,
        );
        let RelocOutcome::NonSimple { ring_mints, .. } = out else {
            panic!("the non-simple wedge polygon must surface the joint trigger");
        };
        assert_eq!(
            ring_mints,
            vec![3],
            "seeds must be the crossing-edge mints of the wedge only \
             (a=3 in; ring mint tr=6 out)"
        );
        assert_eq!(tris, tris0, "NonSimple must not mutate");
    }

    // ── Amendment 13: Fig-11(b→c) merge candidate surfacing ──────────────

    /// The Z-fold BACKTRACK pentagon (the R0099 [178,182] ring, §10a,
    /// scaled): the boundary walks W→M→F out to p, BACKTRACKS to the mint
    /// q (‖p−q‖ = 0.031), and closes q→W; the overshooting chord F→p
    /// crosses q's exit edge q→W a hair past q. Both crossing edges are
    /// domain boundary (star-only mesh), so amendment-8 growth is
    /// impossible — and the ungrowable reject must surface the
    /// amendment-13 merge candidate (p unminted, q minted) with NO
    /// mutation. The LADDER performs the actual merge (mod.rs, gated).
    fn fig11_pentagon() -> (Vec<[u32; 3]>, Vec<RegionClass>, Vec<Point3>) {
        let tris = vec![
            [0, 1, 2], // (W, M, F)
            [0, 2, 3], // (W, F, p)
            [0, 3, 4], // (W, p, q) — folded at q's minted position
        ];
        let class = vec![RegionClass::AOnly; 3];
        let coords = vec![
            p(0.0, -1.0),   // W
            p(0.35, -0.55), // M
            p(0.33, -0.07), // F
            p(0.11, 0.01),  // p — unminted discretization vertex
            p(0.14, 0.004), // q — the mint (backtrack target)
        ];
        (tris, class, coords)
    }

    #[test]
    fn region_fig11_backtrack_surfaces_merge_candidate() {
        let (mut tris, mut class, coords) = fig11_pentagon();
        let tris0 = tris.clone();
        let class0 = class.clone();
        let mut em = edge_map_of(&tris);
        let em0 = em.clone();
        let frame = frame_z0();
        assert!(
            !gate_tri_valid(&[0, 3, 4], &coords, &frame),
            "fixture must start folded at (W,p,q)"
        );
        let mut minted = vec![false; coords.len()];
        minted[4] = true; // q
        let out = relocate_minted_region(
            &mut tris,
            &mut class,
            &mut em,
            &[0, 4],
            &coords,
            &frame,
            &minted,
            false,
        );
        let RegionOutcome::MergeCandidate {
            p,
            q,
            overshoot,
            chord_len,
        } = out
        else {
            panic!("ungrowable backtrack ring must surface the Fig-11 merge candidate");
        };
        assert_eq!(
            (p, q),
            (3, 4),
            "p = the unminted backtrack vertex, q = the mint"
        );
        // inc-3.4 containment quantities: q sits ~4.6e-3 off the split
        // chord F→p (length ~0.234) — a graze, overshoot ≪ chord.
        assert!(
            overshoot > 0.0 && overshoot < 0.01,
            "overshoot {overshoot} out of the graze range"
        );
        assert!(
            (chord_len - 0.234).abs() < 0.01,
            "split chord length {chord_len} != ~0.234"
        );
        assert_eq!(tris, tris0, "candidate surfacing must not mutate triangles");
        assert_eq!(class, class0, "candidate surfacing must not mutate classes");
        assert_eq!(em, em0, "candidate surfacing must not mutate the edge map");
    }

    /// The backtrack detector requires EXACTLY one mint on the sandwiched
    /// edge: with p minted too (an interacting-mints pair, not a Fig-11
    /// too-close endpoint), no candidate surfaces and the reject stays the
    /// plain loud one.
    #[test]
    fn region_fig11_pair_needs_exactly_one_mint() {
        let (mut tris, mut class, coords) = fig11_pentagon();
        let mut em = edge_map_of(&tris);
        let frame = frame_z0();
        let mut minted = vec![false; coords.len()];
        minted[3] = true; // p minted as well
        minted[4] = true;
        assert!(matches!(
            relocate_minted_region(
                &mut tris,
                &mut class,
                &mut em,
                &[0, 4],
                &coords,
                &frame,
                &minted,
                false,
            ),
            RegionOutcome::Rejected
        ));
    }

    /// Fixture (h) (was: `wedge_arm_single_class_path_byte_identical`,
    /// which pinned OFF/ON gate parity while the arm was env-gated; the
    /// inc-2 flip removed the gate): a SINGLE-CLASS deferred cavity still
    /// takes the pre-amendment single-polygon ear-clip path — pinned by
    /// EXACT output. Any future change that routes single-class cavities
    /// through the wedge decomposition (or otherwise perturbs the
    /// single-class ear order) breaks this literal.
    #[test]
    fn wedge_arm_single_class_path_output_pinned() {
        // The pinch fixture, single class throughout.
        let mut tris = vec![
            [1, 0, 2],
            [0, 3, 2],
            [0, 4, 3],
            [0, 5, 4],
            [4, 5, 6],
            [3, 4, 6],
            [2, 3, 6],
        ];
        let mut class = vec![RegionClass::AOnly; 7];
        let coords = vec![
            p(2.2, -0.3),
            p(0.0, 0.0),
            p(0.0, 2.0),
            p(2.0, 1.5),
            p(2.0, 0.5),
            p(4.0, 0.0),
            p(4.0, 2.0),
        ];
        let mut em = edge_map_of(&tris);
        let frame = frame_z0();
        let minted = vec![true, false, false, false, false, false, false];
        assert!(matches!(
            relocate_minted_vertex(
                &mut tris, &mut class, &mut em, 0, &coords, &frame, &minted, false
            ),
            RelocOutcome::Committed
        ));
        // Cavity slots {0,1,2,3,5} carry the 7-gon's ears in
        // first-clippable order (w0 is the fan hub the ear-clip settles
        // on); slots 4 and 6 are untouched.
        assert_eq!(
            tris,
            vec![
                [1, 0, 5],
                [1, 5, 4],
                [1, 4, 6],
                [1, 6, 3],
                [4, 5, 6],
                [3, 2, 1],
                [2, 3, 6],
            ],
            "single-class ear-clip output must stay pinned"
        );
        assert!(class.iter().all(|&c| c == RegionClass::AOnly));
        assert_eq!(
            canon(&em),
            canon(&edge_map_of(&tris)),
            "edge map must be maintained incrementally"
        );
    }
}

#[cfg(test)]
mod split_tests {
    //! Amendment-14 split unit oracles (spec `m8_stage0_multiclass_cavity_arm`
    //! §11): the vert-9 anatomy in miniature on the z=0 plane — an interior
    //! mint v (closed 6-ring, exactly two class transitions) whose resolved
    //! position pokes 0.05 past the constrained ring chord C (the other
    //! input's edge, 1-incident), folding C's star triangle. The split must
    //! mint q_a/q_b ON C with exact rational UVs, re-cut into
    //! side-remnants + material + bulge (cavity + 1 triangles), emit the
    //! rim extras in boundary order, and reject loudly on each §11d guard.

    use super::{fig11_split_cavity, Frame};
    use crate::coplanar_overlay::{ClassifiedOverlay, ExactPoint2, RegionClass};
    use crate::stage0::rim_chords::ExtraRimPoint;
    use cad_primitives::{Point2, Point3};
    use std::collections::BTreeMap;

    fn frame_z0() -> Frame {
        Frame {
            n: [0.0, 0.0, 1.0],
            d: 0.0,
            o: [0.0, 0.0, 0.0],
            e1: [1.0, 0.0, 0.0],
            e2: [0.0, 1.0, 0.0],
        }
    }

    fn p(u: f64, v: f64) -> Point3 {
        Point3::new(u, v, 0.0)
    }

    fn edge_map_of(tris: &[[u32; 3]]) -> BTreeMap<[u32; 2], Vec<usize>> {
        let key = |a: u32, b: u32| if a < b { [a, b] } else { [b, a] };
        let mut m: BTreeMap<[u32; 2], Vec<usize>> = BTreeMap::new();
        for (ti, t) in tris.iter().enumerate() {
            for k in 0..3 {
                m.entry(key(t[k], t[(k + 1) % 3])).or_default().push(ti);
            }
        }
        m
    }

    /// v=0; ring CCW: rk1=1 (0,0), c_first=2 (-0.3,-0.5), deep1=3 (0.2,-1),
    /// deep2=4 (1.8,-1), c_last=5 (2.3,-0.5), rk=6 (2,0). C = (rk,rk1) along
    /// y=0. v's UV sits on its own rim chord y=-0.4; its RESOLVED position
    /// (1.0, +0.05) pokes past C, folding star triangle (v,rk,rk1).
    #[allow(clippy::type_complexity)]
    fn fixture() -> (
        ClassifiedOverlay,
        BTreeMap<[u32; 2], Vec<usize>>,
        Vec<Point3>,
        Vec<bool>,
        Vec<bool>,
    ) {
        let uv = [
            (1.0, -0.4),  // v — pre-mint UV on its own chord
            (0.0, 0.0),   // rk1
            (-0.3, -0.5), // c_first
            (0.2, -1.0),  // deep1
            (1.8, -1.0),  // deep2
            (2.3, -0.5),  // c_last
            (2.0, 0.0),   // rk
        ];
        let tris = vec![
            [0u32, 1, 2], // (v, rk1, c_first)   BOnly
            [0, 2, 3],    // (v, c_first, deep1) Overlap
            [0, 3, 4],    // (v, deep1, deep2)   Overlap
            [0, 4, 5],    // (v, deep2, c_last)  Overlap
            [0, 5, 6],    // (v, c_last, rk)     BOnly
            [0, 6, 1],    // (v, rk, rk1)        BOnly — folds under the mint
        ];
        let class = vec![
            RegionClass::BOnly,
            RegionClass::Overlap,
            RegionClass::Overlap,
            RegionClass::Overlap,
            RegionClass::BOnly,
            RegionClass::BOnly,
        ];
        let overlay = ClassifiedOverlay {
            verts: uv.iter().map(|&(u, v)| Point2::new(u, v)).collect(),
            exact_verts: uv
                .iter()
                .map(|&(u, v)| ExactPoint2::from_f64(u, v).unwrap())
                .collect(),
            poly_a: vec![0; tris.len()],
            poly_b: vec![0; tris.len()],
            class,
            tris,
            fused: BTreeMap::new(),
        };
        let edge_map = edge_map_of(&overlay.tris);
        let mut coords: Vec<Point3> = uv.iter().map(|&(u, v)| p(u, v)).collect();
        coords[0] = p(1.0, 0.05); // the mint, past C
        let minted = vec![true, false, false, false, false, false, false];
        let mergeable = vec![false; 7];
        (overlay, edge_map, coords, minted, mergeable)
    }

    fn own_chords() -> Vec<(ExactPoint2, ExactPoint2)> {
        vec![(
            ExactPoint2::from_f64(-0.5, -0.4).unwrap(),
            ExactPoint2::from_f64(2.5, -0.4).unwrap(),
        )]
    }

    fn other_segs() -> Vec<(ExactPoint2, ExactPoint2)> {
        vec![(
            ExactPoint2::from_f64(-1.0, 0.0).unwrap(),
            ExactPoint2::from_f64(3.0, 0.0).unwrap(),
        )]
    }

    #[test]
    fn split_commits_recut_and_extras() {
        let (mut overlay, mut edge_map, mut coords, mut minted, mut mergeable) = fixture();
        let mut extras: Vec<ExtraRimPoint> = Vec::new();
        let frame = frame_z0();
        let ok = fig11_split_cavity(
            &mut overlay,
            &mut edge_map,
            0,
            (6, 1),
            &mut coords,
            &mut minted,
            &mut mergeable,
            &frame,
            Some(0.1),
            &own_chords(),
            &other_segs(),
            true,
            &mut extras,
            false,
        );
        assert!(ok, "the armed form must commit");
        // Two q vertices minted, exactly collinear with C (y == 0 exactly).
        assert_eq!(overlay.verts.len(), 9);
        assert_eq!(coords.len(), 9);
        for qi in [7usize, 8] {
            assert_eq!(
                overlay.exact_verts[qi].y,
                ExactPoint2::from_f64(0.0, 0.0).unwrap().y
            );
            assert!(!minted[qi] && !mergeable[qi]);
        }
        // Cavity 6 → 7 triangles: one AOnly bulge, side remnants BOnly,
        // material Overlap; every triangle gate-valid (the fold is gone).
        assert_eq!(overlay.tris.len(), 7);
        let bulges: Vec<usize> = (0..7)
            .filter(|&i| overlay.class[i] == RegionClass::AOnly)
            .collect();
        assert_eq!(bulges.len(), 1, "exactly one bulge triangle");
        assert!(overlay.tris[bulges[0]].contains(&0), "bulge fans the mint");
        for (i, t) in overlay.tris.iter().enumerate() {
            assert!(
                super::gate_tri_valid(t, &coords, &frame),
                "tri {i} {t:?} must be gate-valid post-split"
            );
        }
        // C's old edge is replaced by the three sub-segments; the middle
        // one (between the q's) is the 2-incident AOnly|Overlap boundary.
        let key = |a: u32, b: u32| if a < b { [a, b] } else { [b, a] };
        assert!(!edge_map.contains_key(&key(6, 1)), "old C edge gone");
        let mid = edge_map.get(&key(7, 8)).expect("mid C sub-segment");
        assert_eq!(mid.len(), 2, "bulge|material boundary is 2-incident");
        // Rim extras: both q's, owned by v's own chord, in boundary order.
        assert_eq!(extras.len(), 2);
        for x in &extras {
            assert!(x.side_a);
            assert_eq!(x.s, own_chords()[0].0);
            assert_eq!(x.e, own_chords()[0].1);
        }
        assert!(extras[0].t != extras[1].t);
    }

    #[test]
    fn bulge_depth_guard_rejects_without_mutation() {
        let (mut overlay, mut edge_map, mut coords, mut minted, mut mergeable) = fixture();
        let snapshot = (overlay.tris.clone(), overlay.verts.len(), coords.clone());
        let mut extras: Vec<ExtraRimPoint> = Vec::new();
        let ok = fig11_split_cavity(
            &mut overlay,
            &mut edge_map,
            0,
            (6, 1),
            &mut coords,
            &mut minted,
            &mut mergeable,
            &frame_z0(),
            Some(0.01), // overshoot 0.05 exceeds the sagitta premise
            &own_chords(),
            &other_segs(),
            true,
            &mut extras,
            false,
        );
        assert!(!ok);
        assert_eq!(overlay.tris, snapshot.0);
        assert_eq!(overlay.verts.len(), snapshot.1);
        assert_eq!(coords, snapshot.2);
        assert!(extras.is_empty());
    }

    #[test]
    fn other_edge_and_own_chord_guards_reject() {
        let (mut overlay, mut edge_map, mut coords, mut minted, mut mergeable) = fixture();
        let mut extras: Vec<ExtraRimPoint> = Vec::new();
        // C not on the other input's boundary → reject.
        let ok = fig11_split_cavity(
            &mut overlay,
            &mut edge_map,
            0,
            (6, 1),
            &mut coords,
            &mut minted,
            &mut mergeable,
            &frame_z0(),
            Some(0.1),
            &own_chords(),
            &[],
            true,
            &mut extras,
            false,
        );
        assert!(!ok);
        // No own chord carries v → reject (the A-leg has no vehicle).
        let ok = fig11_split_cavity(
            &mut overlay,
            &mut edge_map,
            0,
            (6, 1),
            &mut coords,
            &mut minted,
            &mut mergeable,
            &frame_z0(),
            Some(0.1),
            &[],
            &other_segs(),
            true,
            &mut extras,
            false,
        );
        assert!(!ok);
        assert_eq!(overlay.tris.len(), 6, "no mutation on either reject");
        assert!(extras.is_empty());
    }
}
