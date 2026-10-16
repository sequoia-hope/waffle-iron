//! Stage-4 Fig-11 merge selectors (Yang 2025 §4.4.1).
//!
//! §4.4.1 opens: *"As the intersections on the surfaces are relocated and
//! refined during the optimization, the bijectivity is essentially broken. Each
//! intersection curve is no longer mapped to the corresponding intersection
//! curve between the two meshes, **thus causing gaps or self-intersections**."*
//! (`refs/text/yang2025_hybrid_boolean.txt:605`.) Stage 4 relocates the
//! intersection vertices onto the exact curve; where a relocation steps PAST a
//! neighbouring boundary vertex, the loop Stage 5/6 later extracts folds back
//! over it. The paper's repair is the Fig-11 merge, applied here as a LOCAL
//! re-triangulation (`stage4_construct::rebuild_merge_fan`) by
//! `stage5_topology::run_fold_merge_passes`. This module is the pure SELECTOR
//! layer that pass consumes:
//!
//! * [`fold_merge_sites_censused`] — the corner arm (§4-I6): a fold apex is the
//!   vertex that did NOT move, overrun by a relocated neighbour; the survivor is
//!   picked by the SIGN of the chord parameter ([`chord_param`]), with a
//!   per-condition rejection census so "found nothing" is attributable.
//! * The I13c on-curve terminal-overrun certificate (`oncurve_terminal_overrun`,
//!   gated by [`oncurve_merge_enabled`]).
//! * [`run_absorption_sites`] — the I13d run-level junction absorption
//!   ([`RunAbsorptionSite`], [`RehomeCandidate`], [`RunAbsorptionCensus`]).
//! * [`interlock_groups`] — the I13e grouping of mutually interlocked sites.
//!
//! Every selector is read-only over pre/post vertex positions (the
//! `S4_PRE_POS` oracle) and boundary cycles; mutation belongs to the driver.

use std::collections::{BTreeMap, BTreeSet, HashMap};

/// Where `b` projects onto the line through `a` and `c`, as the parameter `t`
/// with `a` at 0 and `c` at 1. `None` when `a == c` (no line).
///
/// `t` inside `[0, 1]` means `b` sits BETWEEN its two neighbours along the
/// chord; outside means it has moved PAST one of them — which is what "local
/// order inverted" means, stated without any angle or threshold.
fn chord_param(a: [f64; 3], b: [f64; 3], c: [f64; 3]) -> Option<f64> {
    let d = [c[0] - a[0], c[1] - a[1], c[2] - a[2]];
    let len2 = d[0] * d[0] + d[1] * d[1] + d[2] * d[2];
    if len2 == 0.0 || !len2.is_finite() {
        return None;
    }
    let w = [b[0] - a[0], b[1] - a[1], b[2] - a[2]];
    Some((w[0] * d[0] + w[1] * d[1] + w[2] * d[2]) / len2)
}

/// One Yang §4.4.1 **Fig-11(b)→(c)** merge site on a patch boundary cycle: a
/// plain mesh vertex the Stage-4 relocation OVERRAN.
///
/// Fig 11's `q` is "an intersection point on the boundary curve" and `p` is the
/// endpoint of the constrained edge containing it; when `p` is too close, "we
/// merge `p` with `q`". This is that configuration reached from the other
/// direction: Stage 4 relocates `q` onto its exact analytic position and the
/// displacement carries it PAST `p` along the same input-edge chain, so the
/// kept patch's boundary walks out to `q` and back over `p`.
///
/// Measured signature (2026-08-19 ring-reject census, F0045 face 0): the turn
/// at `p` goes `27.69° → 167.34°` when its neighbour moves `2.382e-2` across a
/// `1.283e-2` pre-spacing, and `p`'s own residual on both its surfaces is
/// exactly 0 — a healthy discretization vertex on the wrong side of a refined
/// curve.
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct FoldMergeSite {
    /// Fig-11's `p`: the overrun mesh vertex. Merged away — it carries no
    /// analytic certificate (it was never relocated).
    pub victim: u32,
    /// Fig-11's `q`: the relocated neighbour that overran it. SURVIVES, because
    /// it is the one holding Stage 4's exact curve position.
    pub survivor: u32,
    /// `chord_param(prev, victim, next)` at the POST positions. `< 0` ⇒ the
    /// victim lies past `prev`; `> 1` ⇒ past `next`. The survivor is whichever
    /// end was overrun, so the sign PICKS it — no distance tie-break.
    pub chord_t: f64,
}

/// I13d — the run-level junction-absorption arm (`YANG_441_RUN_ABSORB`).
/// **FLIPPED ALWAYS-ON 2026-08-25** (same day as landing) with the corpus
/// proofs: gate-off default corpus BIT-IDENTICAL to the committed baseline;
/// gate-on corpus CATEGORY-IDENTICAL — 271C/0W/36E/1EE/0T with exactly ONE
/// explained detail row (R0003 advances face 467 → 517, the I13e
/// interlocked-pair wall). `YANG_441_RUN_ABSORB=census` selects and reports
/// at the fold-merge fixed points without applying (its `0|off` kill switch
/// was deleted 2026-10-16, cleanup spec §4).
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(crate) enum RunAbsorbMode {
    Census,
    On,
}

pub(crate) fn run_absorb_mode() -> RunAbsorbMode {
    match std::env::var("YANG_441_RUN_ABSORB") {
        Ok(v) if v == "census" => RunAbsorbMode::Census,
        _ => RunAbsorbMode::On,
    }
}

/// I13e — the cross-site group-absorption arm (`YANG_441_GROUP_ABSORB`).
/// **FLIPPED ALWAYS-ON 2026-08-26** (same day as landing) with the corpus
/// proofs: gate-off default corpus BIT-IDENTICAL to the committed baseline
/// (tracked `results.json` unchanged after a full 312-case run, clean
/// tree); gate-on corpus CATEGORY-IDENTICAL — 271C/0W/36E/1EE/0T with
/// exactly ONE explained detail row (R0003 advances face 517 → 577, off
/// the ring-CDT wall and into the pre-existing KV9-F2 fold family that
/// R0017 already fails on independently). `YANG_441_GROUP_ABSORB=census`
/// selects and reports groups at the run arm's fixed point without applying
/// (its `0|off` kill switch was deleted 2026-10-16). The arm sits strictly downstream of
/// the I13d per-site loop: it only sees sites whose per-site repair the
/// holders already refused.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(crate) enum GroupAbsorbMode {
    Census,
    On,
}

pub(crate) fn group_absorb_mode() -> GroupAbsorbMode {
    match std::env::var("YANG_441_GROUP_ABSORB") {
        Ok(v) if v == "census" => GroupAbsorbMode::Census,
        _ => GroupAbsorbMode::On,
    }
}

/// I13e grouping: connected components of the interlock relation "some mesh
/// triangle contains a victim of mine and a victim of yours" over the given
/// per-site victim sets. A shared triangle is exactly the spec's "my fan
/// polygon contains your victim" — the other site's victim is then a link
/// vertex of mine — so the components are the sites whose single fans
/// mutually contain each other's still-folded overruns and refuse in every
/// repair order (R0003 wall patch 475, measured 2026-08-25).
///
/// Returns only components of size ≥ 2 (a singleton IS the single-site
/// repair, already tried and refused by the caller), each sorted by site
/// index, ordered by smallest member. Victim sets must be pairwise disjoint
/// (the selector's ambiguity rule drops overlapping claims upstream).
pub(crate) fn interlock_groups(
    victims_per_site: &[std::collections::BTreeSet<u32>],
    tris: &[[u32; 3]],
) -> Vec<Vec<usize>> {
    use std::collections::BTreeMap;
    let mut site_of: BTreeMap<u32, usize> = BTreeMap::new();
    for (i, vs) in victims_per_site.iter().enumerate() {
        for &v in vs {
            let prior = site_of.insert(v, i);
            debug_assert!(prior.is_none(), "victim sets must be disjoint");
        }
    }
    fn find(parent: &mut [usize], i: usize) -> usize {
        let mut r = i;
        while parent[r] != r {
            r = parent[r];
        }
        let mut c = i;
        while parent[c] != c {
            let up = parent[c];
            parent[c] = r;
            c = up;
        }
        r
    }
    let mut parent: Vec<usize> = (0..victims_per_site.len()).collect();
    for tri in tris {
        let mut hit = [usize::MAX; 3];
        let mut k = 0;
        for v in tri {
            if let Some(&si) = site_of.get(v) {
                hit[k] = si;
                k += 1;
            }
        }
        for a in 1..k {
            let (ra, rb) = (find(&mut parent, hit[0]), find(&mut parent, hit[a]));
            parent[ra.max(rb)] = ra.min(rb);
        }
    }
    let mut components: BTreeMap<usize, Vec<usize>> = BTreeMap::new();
    for i in 0..victims_per_site.len() {
        let r = find(&mut parent, i);
        components.entry(r).or_default().push(i);
    }
    components.into_values().filter(|c| c.len() >= 2).collect()
}

/// I13c certificate: is corner `i` of `cyc` (post-inversion `t` already
/// established by the caller) a TERMINAL overrun on its own intersection
/// curve? Yes iff:
///
/// 1. both corner edges carry the SAME curve (exact or up to the stored
///    normal's sign);
/// 2. the end the apex crossed (`t < 0` ⇒ prev, `t > 1` ⇒ next) is the seam
///    run's TERMINAL — its far-side cycle edge does not carry that curve
///    (the run ends there: a junction shared with other boundary chains);
/// 3. the curve parameters of (other end → survivor → apex) at the POST
///    positions are strictly monotone — the survivor lies strictly BETWEEN
///    its run neighbour and the apex on the curve, i.e. the apex's
///    relocation carried it past the terminal. Periodic params compare via
///    (−π, π]-wrapped deltas (a chain chord subtends < π — the standing
///    convention); open-conic params (I13b) compare raw.
///
/// Returns the survivor. No distance band anywhere (P10) — the certificate
/// is the parameter order itself.
fn oncurve_terminal_overrun(
    curves: &BTreeMap<(u32, u32), crate::Curve>,
    cyc: &[u32],
    i: usize,
    t: f64,
    post: &[[f64; 3]],
) -> Option<u32> {
    use crate::stage4_correct::{
        conic_param, conic_param_periodic, conics_equal_up_to_normal_sign,
    };
    let n = cyc.len();
    let (a, b, c) = (cyc[(i + n - 1) % n], cyc[i], cyc[(i + 1) % n]);
    let key = |x: u32, y: u32| (x.min(y), x.max(y));
    let cab = curves.get(&key(a, b))?;
    let cbc = curves.get(&key(b, c))?;
    let same = |x: &crate::Curve, y: &crate::Curve| x == y || conics_equal_up_to_normal_sign(x, y);
    if !same(cab, cbc) {
        return None;
    }
    let (survivor, other, far) = if t < 0.0 {
        (a, c, cyc[(i + n - 2) % n])
    } else {
        (c, a, cyc[(i + 2) % n])
    };
    if survivor == far || survivor == other || far == b {
        return None; // degenerate cycle neighbourhood — not a run shape
    }
    if let Some(cf) = curves.get(&key(survivor, far)) {
        if same(cf, cab) {
            return None; // run continues past the crossed end — not a terminal
        }
    }
    let param = |v: u32| conic_param(cab, cad_primitives::Point3::from(*post.get(v as usize)?));
    let (t_o, t_s, t_b) = (param(other)?, param(survivor)?, param(b)?);
    let (d1, d2) = if conic_param_periodic(cab) {
        let wrap = |mut d: f64| -> f64 {
            while d > std::f64::consts::PI {
                d -= 2.0 * std::f64::consts::PI;
            }
            while d <= -std::f64::consts::PI {
                d += 2.0 * std::f64::consts::PI;
            }
            d
        };
        (wrap(t_s - t_o), wrap(t_b - t_s))
    } else {
        (t_s - t_o, t_b - t_s)
    };
    if !d1.is_finite() || !d2.is_finite() || d1 == 0.0 || d2 == 0.0 {
        return None;
    }
    (d1 * d2 > 0.0).then_some(survivor)
}

/// Per-condition rejection counts for [`fold_merge_sites`] — the selector's own
/// coverage ledger, so "found nothing" is always attributable to a CONDITION
/// rather than inferred. Reported by the driver under `YANG_441_VERBOSE`.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub struct FoldMergeCensus {
    /// Corners examined (degenerate ones, and ones with no pre position for all
    /// three vertices, excluded).
    pub corners: usize,
    /// Corners whose chord order Stage 4 inverted (condition 2).
    pub inversions: usize,
    /// Inversions rejected because the apex itself MOVED (condition 1) — two
    /// on-curve vertices crossed each other, which is chain ORDER (§4.3.4
    /// `ReorderConic`), not Fig-11.
    pub apex_moved: usize,
    /// Inversions rejected because the apex was MINTED during Stage 4 (no `pre`
    /// entry at all — e.g. an appended §4.3.4 on-curve sample). Counted apart
    /// from [`Self::apex_moved`] because the two are different populations with
    /// different owners, and collapsing them into one counter would attribute
    /// the residue to chain order without having measured it. **Measured
    /// 2026-08-19d over the ring-reject family: 0 in every case** — every
    /// rejected inversion has an apex that genuinely moved.
    pub apex_minted: usize,
    /// Of [`Self::apex_moved`], how many sit on an INTERSECTION-CURVE chain
    /// (both incident cycle edges are curve edges). Those are two on-curve
    /// vertices that crossed each other — chain ORDER, owned by §4.3.4's
    /// `ReorderConic`. The remainder are relocated vertices that crossed a
    /// neighbour on a PLAIN boundary, which is neither that nor Fig-11, and is
    /// the class with no owner yet.
    pub apex_moved_on_curve: usize,
    /// Inversions rejected because the overrun end never moved (condition 3) —
    /// including the case where NOTHING at the corner moved, i.e. the crossing
    /// was minted by a relocation elsewhere on the loop.
    pub survivor_still: usize,
    /// Victims dropped as ambiguous (two survivors claim them).
    pub ambiguous: usize,
    /// I13c (always on): of
    /// [`Self::apex_moved_on_curve`], corners certified as a TERMINAL
    /// overrun on their shared curve — the apex's relocation carried it past
    /// the seam run's terminal junction in curve parameter — and proposed as
    /// merge sites (victim = apex, survivor = the junction). Zero when the
    /// gate is off.
    pub oncurve_sites: usize,
}

/// Select every [`FoldMergeSite`] over a patch-cycle set — the threshold-free
/// selector for the §4.4.1 Fig-11 merge.
///
/// A corner `(a, b, c)` qualifies iff ALL of:
/// 1. `b` did NOT move across Stage 4 (`post == pre`) — a relocated vertex sits
///    on an exact analytic curve and is never merged away (that would discard
///    the certificate), and a Stage-4-MINTED vertex (no `pre` entry) has no
///    "was it overrun" question to ask;
/// 2. `b` sat INSIDE the chord of its own neighbours before and lies OUTSIDE it
///    after — the chord-order-inversion certificate, which is exactly the
///    `class=MINTED_BY_S4` verdict the loop-simplicity census reports. A
///    sign/interval test on a ratio: no band, no angle, scale-free;
/// 3. the END it overran (`a` when `t < 0`, `c` when `t > 1`) DID move — that
///    relocation is what carried the boundary across `b`. An inversion between
///    two unmoved vertices is inherited geometry and is left alone.
///
/// `pre` is the `S4_PRE_POS` map. It — not the `relocations` vector — is the
/// correct "was it relocated?" oracle: `relocations` carries conic `(vertex, t)`
/// retags only, so the implicit-pair and junction arms move vertices without
/// appearing in it. Measured 2026-08-19: on R0074/R0085/R0095/R0025 the vector
/// is EMPTY while 59–83 vertices per loop moved, so a `relocations`-keyed
/// condition 3 rejects every inversion in the family.
///
/// Ambiguity is dropped, never guessed: a victim claimed by two different
/// survivors is excluded. Deterministic — `BTreeMap` iteration only.
///
/// Returns the sites with the per-condition rejection census. `curves` is the
/// `intersection_curves` map, keys canonicalized `(min, max)`. The KEYS drive
/// the census's on/off-curve split; the VALUES feed the I13c on-curve
/// TERMINAL-overrun arm (ALWAYS-ON since 2026-08-25, the I13 corpus proofs).
/// May be empty.
pub fn fold_merge_sites_censused<'a>(
    cycles: impl IntoIterator<Item = &'a [u32]>,
    pre: &HashMap<u32, [f64; 3]>,
    post: &[[f64; 3]],
    curves: &BTreeMap<(u32, u32), crate::Curve>,
) -> (Vec<FoldMergeSite>, FoldMergeCensus) {
    let mut claimed: BTreeMap<u32, (u32, f64)> = BTreeMap::new();
    let mut ambiguous: BTreeSet<u32> = BTreeSet::new();
    let mut census = FoldMergeCensus::default();
    let moved = |v: u32| -> Option<bool> { Some(*pre.get(&v)? != *post.get(v as usize)?) };
    for cyc in cycles {
        let n = cyc.len();
        if n < 3 {
            continue;
        }
        for i in 0..n {
            let (a, b, c) = (cyc[(i + n - 1) % n], cyc[i], cyc[(i + 1) % n]);
            if a == b || b == c || a == c {
                continue;
            }
            let (Some(&pa), Some(&pb), Some(&pc)) = (pre.get(&a), pre.get(&b), pre.get(&c)) else {
                continue;
            };
            let (Some(&qa), Some(&qb), Some(&qc)) = (
                post.get(a as usize),
                post.get(b as usize),
                post.get(c as usize),
            ) else {
                continue;
            };
            census.corners += 1;
            let (Some(t_pre), Some(t)) = (chord_param(pa, pb, pc), chord_param(qa, qb, qc)) else {
                continue;
            };
            if !(0.0..=1.0).contains(&t_pre) || (0.0..=1.0).contains(&t) {
                continue;
            }
            census.inversions += 1;
            match moved(b) {
                Some(false) => {}
                Some(true) => {
                    census.apex_moved += 1;
                    let on_curve = |x: u32, y: u32| curves.contains_key(&(x.min(y), x.max(y)));
                    let oc = on_curve(a, b) && on_curve(b, c);
                    if oc {
                        census.apex_moved_on_curve += 1;
                        // I13c (gated): the TERMINAL-overrun arm. The apex
                        // moved — the still-apex Fig-11 conditions cannot
                        // hold — but when its relocation carried it past the
                        // seam run's TERMINAL junction in curve parameter,
                        // the same Fig-11 merge (victim = apex, survivor =
                        // the junction) is the paper's repair; the reorder
                        // authority refuses this shape by its endpoint
                        // guard. Shares `claimed` so cross-arm duplicates
                        // and ambiguity resolve in one place.
                        if let Some(s) = oncurve_terminal_overrun(curves, cyc, i, t, post) {
                            census.oncurve_sites += 1;
                            match claimed.get(&b) {
                                Some(&(s0, _)) if s0 != s => {
                                    ambiguous.insert(b);
                                }
                                Some(_) => {}
                                None => {
                                    claimed.insert(b, (s, t));
                                }
                            }
                        }
                        // Per-corner detail for the ON-CURVE arm (2026-08-25,
                        // R0003 face-437 rim×cut boundary-hook census): two
                        // on-curve vertices whose chain order Stage 4 inverted.
                        // What decides its treatment is WHERE on the shared
                        // curve the apex landed relative to the end it overran
                        // — an apex carried past a run-TERMINAL junction is
                        // Fig-11's p/q (the reorder authority refuses endpoint
                        // re-roots by design); an interior×interior crossing is
                        // §4.3.4 chain order. Ids + pre/post positions let the
                        // offline curve fit assign the parameter certificate.
                        if std::env::var_os("YANG_441_FOLD_CENSUS").is_some() {
                            let over = if t < 0.0 { a } else { c };
                            eprintln!(
                                "YANG_441_FOLD_ONCURVE apex={b} over={over} other={} t={t:.4} \
                                 pre_a={pa:?} pre_b={pb:?} pre_c={pc:?} \
                                 post_a={qa:?} post_b={qb:?} post_c={qc:?}",
                                if t < 0.0 { c } else { a },
                            );
                        }
                    }
                    // Per-corner detail for the OFF-CURVE arm (the class with no
                    // owner as of 2026-08-19d). What decides its treatment is
                    // how far the apex ended up from the neighbour it overran,
                    // measured against the corner's own edge lengths — a
                    // near-duplicate pair is a Fig-11 merge with a
                    // richer-certificate survivor; a far one is not.
                    if !oc && std::env::var_os("YANG_441_FOLD_CENSUS").is_some() {
                        let over = if t < 0.0 { a } else { c };
                        let d = |x: [f64; 3], y: [f64; 3]| {
                            ((x[0] - y[0]).powi(2) + (x[1] - y[1]).powi(2) + (x[2] - y[2]).powi(2))
                                .sqrt()
                        };
                        let qo = if t < 0.0 { qa } else { qc };
                        let disp_b = d(pb, qb);
                        let disp_o = d(if t < 0.0 { pa } else { pc }, qo);
                        eprintln!(
                            "YANG_441_FOLD_OFFCURVE apex={b} over={over} t={t:.4}                              gap={:.4e} seg_ab={:.4e} seg_bc={:.4e} disp_apex={disp_b:.4e}                              disp_over={disp_o:.4e} edge_ab_curve={} edge_bc_curve={}",
                            d(qb, qo),
                            d(qa, qb),
                            d(qb, qc),
                            on_curve(a, b),
                            on_curve(b, c),
                        );
                    }
                    continue;
                }
                None => {
                    census.apex_minted += 1;
                    continue;
                }
            }
            let survivor = if t < 0.0 { a } else { c };
            if moved(survivor) != Some(true) {
                census.survivor_still += 1;
                continue;
            }
            match claimed.get(&b) {
                Some(&(s0, _)) if s0 != survivor => {
                    ambiguous.insert(b);
                }
                Some(_) => {}
                None => {
                    claimed.insert(b, (survivor, t));
                }
            }
        }
    }
    for b in &ambiguous {
        claimed.remove(b);
    }
    census.ambiguous = ambiguous.len();
    // No chained-substitution filter is needed: survivors moved and victims did
    // not, so the two sets are DISJOINT by conditions 1 and 3 — a victim can
    // never also be some other site's survivor.
    let sites = claimed
        .into_iter()
        .map(|(victim, (survivor, chord_t))| FoldMergeSite {
            victim,
            survivor,
            chord_t,
        })
        .collect();
    (sites, census)
}

/// I13d (spec `yang_441_trim_cdt_construction.md` §I13(c)) — one **run-level
/// junction absorption** site: a maximal same-curve boundary run whose
/// Stage-4 relocation carried a junction-adjacent prefix of vertices PAST the
/// run's junction terminal, in curve parameter. Every out-of-band prefix
/// vertex merges into the junction (Fig-11's p→q at run granularity).
///
/// The corner-level I13c arm is structurally blind to this family (measured,
/// R0003 face 467): the only chord-inverted corner's chord-sign survivor is
/// the NEXT run vertex — itself out-of-band — and its far-side edge continues
/// on the same curve, so the terminal condition refuses. The run vantage sees
/// what the corner cannot: the junction bounding the run, and each vertex's
/// pre→post side of it.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct RunAbsorptionSite {
    /// The junction terminal — carries strictly more surfaces than every
    /// victim (a model junction), so the merge discards no authority (I8).
    pub survivor: u32,
    /// The out-of-band run vertices, junction-nearest first. Each one's curve
    /// parameter crossed the junction's between its pre and post positions.
    pub victims: Vec<u32>,
}

/// §I13(f) — a single-victim `not_richer` refusal carried OUT of the
/// selector with its certified inversion evidence: the selector's own
/// t-params along the run curve. The selector is the only production
/// authority for the pair's ORDER INVERSION (which side of a rim belongs
/// to which band is authored profile data); the re-homing planner
/// (`stage4_rehome`) certifies construction, never order, so the apply
/// arm re-verifies these exact params at its site (spec §I13(f) f2).
/// Multi-victim refusals are not the measured family (f0 census: 228/228
/// R0003 pairs are single-victim true-corner pairs) and are not carried.
#[derive(Clone, Copy, Debug)]
pub struct RehomeCandidate {
    /// The run terminal at the refusal.
    pub j: u32,
    /// The single inverted-prefix vertex.
    pub v: u32,
    /// The run's typed curve — the conic both t-params live on.
    pub curve: crate::Curve,
    pub t_pre_j: f64,
    pub t_post_j: f64,
    pub t_pre_v: f64,
    pub t_post_v: f64,
}

/// Per-condition coverage ledger for [`run_absorption_sites`] — every
/// refused candidate is attributable to a CONDITION, mirroring
/// [`FoldMergeCensus`]'s discipline.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub struct RunAbsorptionCensus {
    /// Maximal same-curve runs examined (≥1 typed edge, both ends bounded).
    pub runs: usize,
    /// Terminal ends walked (two per run unless the cycle is fully typed).
    pub terminals: usize,
    /// Terminals refused: the junction's own curve parameter is undefined.
    pub no_param: usize,
    /// Terminals refused: no vertex flipped sides of the junction (the
    /// healthy-chain verdict — pre and post both in-band).
    pub no_flip: usize,
    /// Terminals refused: a flipped prefix exists but no MINTED chord
    /// inversion sits on it (pre inside its neighbours' chord, post outside)
    /// — the doubling-back witness Stage 4's own relocation must supply.
    pub no_inversion: usize,
    /// Terminals refused: the junction is not strictly richer than every
    /// flipped vertex (missing carrier ⇒ not a model junction ⇒ absorbing
    /// would discard authority).
    pub not_richer: usize,
    /// Victims dropped because two sites with different survivors claim them.
    pub ambiguous: usize,
    /// Sites emitted (after cross-cycle dedupe).
    pub sites: usize,
}

/// I13d selection — the run-level junction-absorption sites over a
/// patch-cycle set.
///
/// For each maximal run of consecutive cycle edges typed on the SAME curve
/// (identity up to the stored normal's sign), each bounded end `J` is a
/// candidate junction terminal. Walking outward from `J`, a vertex `w` is
/// out-of-band iff Stage 4's relocation INVERTED the order of the pair
/// `(w, J)` along the curve — strict opposite signs of
/// `t_pre(w) − t_pre(J)` and `t_post(w) − t_post(J)` (wrapped to (−π, π]
/// for periodic conics, raw for open ones; the standing chord-subtends-<π
/// convention). The test is symmetric in WHICH endpoint moved — measured
/// both ways on R0003: a spur vertex carried past a solved junction (the
/// face-437 shape), and a junction solved 0.67 PAST its first two chain
/// samples (face 467: v2332 hops v2331/v2330 on their shared hyperbola,
/// which the samples' own one-sided view calls healthy). The maximal
/// inverted prefix is the victim set. A site is emitted iff:
///
/// 1. the prefix is nonempty (the pair order inverted for someone);
/// 2. at least one prefix vertex is a MINTED chord inversion in its cycle
///    corner (pre inside `[0,1]`, post outside) — the doubling-back
///    witness. Load-bearing, not decorative: the junction's own large
///    relocation can invert its pre/post pair order against the NEIGHBOUR
///    chain's in-domain samples too (projection artifacts of the drifted
///    pre position), but an in-domain chain stays post-monotone and can
///    have no minted fold at its samples, so this witness structurally
///    refuses that side;
/// 3. `strictly_richer(J, w)` holds for every victim `w` — the caller's
///    carrier oracle: `carried(w) ⊂ carried(J)` proper, the I8 containment
///    plus junction-hood.
///
/// No distance band anywhere (P10): every test is an order or containment
/// certificate. Sites found from two cycles (each boundary chain appears in
/// both adjacent patches' cycles) dedupe by value; a victim claimed by two
/// different survivors drops BOTH sites, loudly, never guessed.
pub fn run_absorption_sites<'a>(
    cycles: impl IntoIterator<Item = &'a [u32]>,
    pre: &HashMap<u32, [f64; 3]>,
    post: &[[f64; 3]],
    curves: &BTreeMap<(u32, u32), crate::Curve>,
    strictly_richer: impl Fn(u32, u32) -> bool,
) -> (
    Vec<RunAbsorptionSite>,
    RunAbsorptionCensus,
    Vec<RehomeCandidate>,
) {
    use crate::stage4_correct::{
        conic_param, conic_param_periodic, conics_equal_up_to_normal_sign,
    };
    let mut census = RunAbsorptionCensus::default();
    let mut sites: Vec<RunAbsorptionSite> = Vec::new();
    let mut rehome: Vec<RehomeCandidate> = Vec::new();
    let same = |x: &crate::Curve, y: &crate::Curve| x == y || conics_equal_up_to_normal_sign(x, y);
    let wrap = |mut d: f64| -> f64 {
        while d > std::f64::consts::PI {
            d -= 2.0 * std::f64::consts::PI;
        }
        while d <= -std::f64::consts::PI {
            d += 2.0 * std::f64::consts::PI;
        }
        d
    };
    for cyc in cycles {
        let n = cyc.len();
        if n < 3 {
            continue;
        }
        let key = |x: u32, y: u32| (x.min(y), x.max(y));
        let edge_curve =
            |i: usize| -> Option<&crate::Curve> { curves.get(&key(cyc[i], cyc[(i + 1) % n])) };
        // Maximal same-curve runs of edges. A run STARTS at a typed edge whose
        // predecessor does not continue its curve; a cycle typed end-to-end on
        // ONE curve has no start and is skipped — that is a closed seam, the
        // reorder authority's shape, with no bounded end to absorb into.
        for i in 0..n {
            let Some(c0) = edge_curve(i) else {
                continue;
            };
            if edge_curve((i + n - 1) % n).is_some_and(|c| same(c, c0)) {
                continue; // mid-run: its start is found at its own index
            }
            let mut len = 1usize;
            while len < n {
                match edge_curve((i + len) % n) {
                    Some(c) if same(c, c0) => len += 1,
                    _ => break,
                }
            }
            census.runs += 1;
            // Run vertices w_0..w_len (len edges): cyc[i..=i+len] mod n.
            let w = |k: usize| cyc[(i + k) % n];
            for (jk, dir) in [(0usize, 1i64), (len, -1i64)] {
                census.terminals += 1;
                let j = w(jk);
                let param = |p: [f64; 3]| conic_param(c0, cad_primitives::Point3::from(p));
                let (Some(t_j), Some(t_j_pre)) = (
                    post.get(j as usize).copied().and_then(param),
                    pre.get(&j).copied().and_then(param),
                ) else {
                    census.no_param += 1;
                    continue;
                };
                let sided = |t: f64, tref: f64| -> f64 {
                    if conic_param_periodic(c0) {
                        wrap(t - tref)
                    } else {
                        t - tref
                    }
                };
                // Maximal ORDER-INVERTED prefix walking outward from the
                // junction: the pre→post relocation swapped which side of the
                // junction the vertex lies on, in curve parameter. Symmetric
                // in which endpoint moved — measured both ways on R0003:
                // face 437's spur vertex carried past a solved junction, and
                // face 467's junction solved 0.67 PAST its first two chain
                // samples (v2332 hops v2331/v2330 on their shared hyperbola).
                let mut victims: Vec<u32> = Vec::new();
                let mut k = jk as i64 + dir;
                while (0..=len as i64).contains(&k) {
                    let v = w(k as usize);
                    if v == j {
                        break;
                    }
                    let flipped = (|| -> Option<bool> {
                        let tq = param(*post.get(v as usize)?)?;
                        let tp = param(*pre.get(&v)?)?;
                        let (dq, dp) = (sided(tq, t_j), sided(tp, t_j_pre));
                        Some(dq != 0.0 && dp != 0.0 && (dq > 0.0) != (dp > 0.0))
                    })();
                    if flipped != Some(true) {
                        break;
                    }
                    victims.push(v);
                    k += dir;
                }
                if victims.is_empty() {
                    census.no_flip += 1;
                    continue;
                }
                // Doubling-back witness: a MINTED chord inversion whose apex
                // is a victim, in this cycle's corner context.
                let minted_inversion = |v: u32| -> bool {
                    let Some(iv) = (0..n).find(|&x| cyc[x] == v) else {
                        return false;
                    };
                    let (a, b, c) = (cyc[(iv + n - 1) % n], cyc[iv], cyc[(iv + 1) % n]);
                    if a == b || b == c || a == c {
                        return false;
                    }
                    let (Some(&pa), Some(&pb), Some(&pc)) = (pre.get(&a), pre.get(&b), pre.get(&c))
                    else {
                        return false;
                    };
                    let (Some(&qa), Some(&qb), Some(&qc)) = (
                        post.get(a as usize),
                        post.get(b as usize),
                        post.get(c as usize),
                    ) else {
                        return false;
                    };
                    let (Some(tp), Some(tq)) = (chord_param(pa, pb, pc), chord_param(qa, qb, qc))
                    else {
                        return false;
                    };
                    (0.0..=1.0).contains(&tp) && !(0.0..=1.0).contains(&tq)
                };
                if !victims.iter().any(|&v| minted_inversion(v)) {
                    census.no_inversion += 1;
                    continue;
                }
                if !victims.iter().all(|&v| strictly_richer(j, v)) {
                    census.not_richer += 1;
                    // §I13(f): carry the single-victim refused PAIR out with
                    // this terminal's inversion params. Both terminals of the
                    // pair's run (and both adjacent patches' cycles) refuse
                    // here — dedupe by unordered ids, keeping the first
                    // sighting's params (same mesh state, same evidence).
                    if let [v] = victims[..] {
                        let seen = rehome
                            .iter()
                            .any(|c| (c.j.min(c.v), c.j.max(c.v)) == (j.min(v), j.max(v)));
                        let params = (
                            pre.get(&v).copied().and_then(param),
                            post.get(v as usize).copied().and_then(param),
                        );
                        if let (false, (Some(t_pre_v), Some(t_post_v))) = (seen, params) {
                            rehome.push(RehomeCandidate {
                                j,
                                v,
                                curve: *c0,
                                t_pre_j: t_j_pre,
                                t_post_j: t_j,
                                t_pre_v,
                                t_post_v,
                            });
                        }
                    }
                    // I13f census (spec §I13(f), read-only): the not_richer
                    // bucket holds the inverted-junction-pair family (both
                    // ends true corners — R0003 face 903) among ordinary
                    // refusals. Params + curve kind here; the caller's
                    // richer-closure prints the carrier sets for the same
                    // ids.
                    if std::env::var("YANG_441_I13F").is_ok_and(|v| v == "census") {
                        let vic: Vec<String> = victims
                            .iter()
                            .map(|&v| {
                                let (tq, tp) = (
                                    post.get(v as usize).copied().and_then(param),
                                    pre.get(&v).copied().and_then(param),
                                );
                                format!("v{v}(t={tq:?} pre={tp:?})")
                            })
                            .collect();
                        eprintln!(
                            "[i13f-census] NOT_RICHER j=v{j} t_j={t_j:.9} \
                             t_j_pre={t_j_pre:.9} curve={} victims=[{}]",
                            crate::stage5_output_refine::curve_kind(c0),
                            vic.join(", ")
                        );
                    }
                    continue;
                }
                let site = RunAbsorptionSite {
                    survivor: j,
                    victims,
                };
                if !sites.contains(&site) {
                    sites.push(site);
                }
            }
        }
    }
    // A victim claimed by two different survivors: drop every site touching
    // it — ambiguity is refused, never guessed.
    let mut owner: BTreeMap<u32, u32> = BTreeMap::new();
    let mut poisoned: BTreeSet<u32> = BTreeSet::new();
    for s in &sites {
        for &v in &s.victims {
            match owner.get(&v) {
                Some(&j0) if j0 != s.survivor => {
                    poisoned.insert(v);
                }
                _ => {
                    owner.insert(v, s.survivor);
                }
            }
        }
    }
    census.ambiguous = poisoned.len();
    sites.retain(|s| s.victims.iter().all(|v| !poisoned.contains(v)));
    census.sites = sites.len();
    (sites, census, rehome)
}

#[cfg(test)]
mod tests {
    use super::*;

    /// [`fold_merge_sites_censused`] without the census.
    fn fold_merge_sites<'a>(
        cycles: impl IntoIterator<Item = &'a [u32]>,
        pre: &HashMap<u32, [f64; 3]>,
        post: &[[f64; 3]],
    ) -> Vec<FoldMergeSite> {
        fold_merge_sites_censused(cycles, pre, post, &BTreeMap::new()).0
    }

    fn m(pairs: &[(u32, [f64; 3])]) -> HashMap<u32, [f64; 3]> {
        pairs.iter().copied().collect()
    }
    #[test]
    fn chord_param_is_none_on_a_degenerate_chord() {
        assert!(chord_param([1.0, 0.0, 0.0], [0.5, 1.0, 0.0], [1.0, 0.0, 0.0]).is_none());
        let t = chord_param([0.0, 0.0, 0.0], [0.25, 9.0, 0.0], [1.0, 0.0, 0.0]).unwrap();
        assert!(
            (t - 0.25).abs() < 1e-15,
            "projection is along the chord: {t}"
        );
    }

    // ---- Fig-11 merge-site selector (§4.4.1) ---------------------------

    /// The F0045 witness in miniature: a rim chain `q, p, r` whose junction `q`
    /// was relocated PAST `p`. `p` is still, so a selector ranging over MOVED
    /// vertices cannot see it; this one must.
    #[test]
    fn fold_merge_site_picks_the_overrun_still_vertex() {
        // Cycle: 0 (relocated junction), 1 (still rim vertex), 2, 3.
        let cyc = vec![0u32, 1, 2, 3];
        let pre = m(&[
            (0, [1.4, 0.0, 0.0]), // 0 sat BEYOND 1 — chain order 0,1,2
            (1, [1.2, 0.0, 0.0]),
            (2, [0.0, 0.0, 0.0]),
            (3, [0.5, -1.0, 0.0]),
        ]);
        let post = vec![
            [1.0, 0.0, 0.0], // 0 — relocated, landed on the FAR side of 1
            [1.2, 0.0, 0.0], // 1 — the overrun still vertex
            [0.0, 0.0, 0.0],
            [0.5, -1.0, 0.0],
        ];
        let sites = fold_merge_sites([cyc.as_slice()], &pre, &post);
        assert_eq!(sites.len(), 1, "exactly the overrun vertex: {sites:?}");
        assert_eq!(sites[0].victim, 1);
        assert_eq!(sites[0].survivor, 0, "the RELOCATED end survives");
        assert!(sites[0].chord_t < 0.0, "victim lies past `prev`");
    }

    /// The same overshoot from the other side: the relocated vertex is the NEXT
    /// neighbour, so the sign flips and it is still the survivor.
    #[test]
    fn fold_merge_site_picks_the_relocated_next_neighbour() {
        let cyc = vec![0u32, 1, 2, 3];
        let pre = m(&[
            (0, [0.0, 0.0, 0.0]),
            (1, [1.2, 0.0, 0.0]),
            (2, [1.4, 0.0, 0.0]), // 2 sat beyond 1
            (3, [0.5, -1.0, 0.0]),
        ]);
        let post = vec![
            [0.0, 0.0, 0.0],
            [1.2, 0.0, 0.0], // 1 — overrun
            [1.0, 0.0, 0.0], // 2 — relocated, landed BEFORE 1
            [0.5, -1.0, 0.0],
        ];
        let sites = fold_merge_sites([cyc.as_slice()], &pre, &post);
        assert_eq!(sites.len(), 1, "{sites:?}");
        assert_eq!((sites[0].victim, sites[0].survivor), (1, 2));
        assert!(sites[0].chord_t > 1.0, "victim lies past `next`");
    }

    // ---- I13c on-curve terminal-overrun certificate --------------------

    /// Circle standing in for the seam conic (its `conic_param` is ungated);
    /// positions minted via `conic_eval` at chosen parameters so the ORDER is
    /// exact by construction.
    fn unit_circle() -> crate::Curve {
        crate::Curve::Circle {
            center: cad_primitives::Point3::new(0.0, 0.0, 0.0),
            normal: crate::Vector3::new(0.0, 0.0, 1.0),
            radius: 1.0,
        }
    }

    fn on_circle(t: f64) -> [f64; 3] {
        crate::geom::conic_eval(&unit_circle(), t)
            .expect("circle eval")
            .as_array()
    }

    fn curve_map(edges: &[(u32, u32)]) -> BTreeMap<(u32, u32), crate::Curve> {
        edges
            .iter()
            .map(|&(x, y)| ((x.min(y), x.max(y)), unit_circle()))
            .collect()
    }

    /// The R0003 face-437 shape in miniature: cycle run `1 → 2 → 3` on one
    /// curve, the run TERMINAL at 3 (edge (3,4) is off-curve), and the apex
    /// 2's post position BEYOND 3 in curve parameter. The certificate fires
    /// and names 3 the survivor.
    #[test]
    fn oncurve_terminal_overrun_certifies_the_r0003_shape() {
        let cyc = vec![0u32, 1, 2, 3, 4];
        let post = vec![
            [5.0, 5.0, 0.0], // 0 — far, not involved
            on_circle(0.10), // 1 = other end of the corner
            on_circle(0.28), // 2 = apex, past the terminal
            on_circle(0.20), // 3 = survivor (terminal junction)
            [6.0, 6.0, 0.0], // 4 — off-curve continuation
        ];
        let curves = curve_map(&[(1, 2), (2, 3)]);
        let got = oncurve_terminal_overrun(&curves, &cyc, 2, 5.0, &post);
        assert_eq!(got, Some(3));
    }

    /// If the crossed end's far-side edge carries the SAME curve, the run
    /// continues past it — it is not a terminal, and the corner belongs to
    /// chain-order (ReorderConic) territory, not the merge.
    #[test]
    fn oncurve_terminal_overrun_declines_when_the_run_continues() {
        let cyc = vec![0u32, 1, 2, 3, 4];
        let post = vec![
            [5.0, 5.0, 0.0],
            on_circle(0.10),
            on_circle(0.28),
            on_circle(0.20),
            on_circle(0.35), // 4 — the run continues on the same curve
        ];
        let curves = curve_map(&[(1, 2), (2, 3), (3, 4)]);
        assert_eq!(oncurve_terminal_overrun(&curves, &cyc, 2, 5.0, &post), None);
    }

    /// A curve-parameter-monotone corner is healthy chain order regardless of
    /// what the 3D chord test claimed — the certificate is the parameter
    /// order, and it must decline.
    #[test]
    fn oncurve_terminal_overrun_declines_a_parameter_monotone_corner() {
        let cyc = vec![0u32, 1, 2, 3, 4];
        let post = vec![
            [5.0, 5.0, 0.0],
            on_circle(0.10),
            on_circle(0.15), // apex BETWEEN its ends on the curve
            on_circle(0.20),
            [6.0, 6.0, 0.0],
        ];
        let curves = curve_map(&[(1, 2), (2, 3)]);
        assert_eq!(oncurve_terminal_overrun(&curves, &cyc, 2, 5.0, &post), None);
    }

    /// Two DIFFERENT conics meeting at the apex is a junction, not a run —
    /// no shared parameter, no certificate.
    #[test]
    fn oncurve_terminal_overrun_declines_mismatched_curves() {
        let cyc = vec![0u32, 1, 2, 3, 4];
        let post = vec![
            [5.0, 5.0, 0.0],
            on_circle(0.10),
            on_circle(0.28),
            on_circle(0.20),
            [6.0, 6.0, 0.0],
        ];
        let mut curves = curve_map(&[(1, 2)]);
        curves.insert(
            (2, 3),
            crate::Curve::Circle {
                center: cad_primitives::Point3::new(0.0, 0.0, 0.0),
                normal: crate::Vector3::new(0.0, 0.0, 1.0),
                radius: 2.0,
            },
        );
        assert_eq!(oncurve_terminal_overrun(&curves, &cyc, 2, 5.0, &post), None);
    }

    /// Periodic params certify across the ±π branch cut — the wrapped deltas
    /// keep the order reading, exactly like `conic_param_deltas`' convention.
    #[test]
    fn oncurve_terminal_overrun_wraps_periodic_params_across_the_cut() {
        let pi = std::f64::consts::PI;
        let cyc = vec![0u32, 1, 2, 3, 4];
        let post = vec![
            [5.0, 5.0, 0.0],
            on_circle(pi - 0.05),  // other
            on_circle(-pi + 0.03), // apex — 0.04 past the survivor, wrapped
            on_circle(pi - 0.01),  // survivor at the terminal
            [6.0, 6.0, 0.0],
        ];
        let curves = curve_map(&[(1, 2), (2, 3)]);
        assert_eq!(
            oncurve_terminal_overrun(&curves, &cyc, 2, 5.0, &post),
            Some(3)
        );
    }

    /// Arm pin (always-on since 2026-08-25): the certified corner yields
    /// exactly one site.
    #[test]
    fn oncurve_arm_follows_the_flipped_gate() {
        let cyc = vec![0u32, 1, 2, 3, 4];
        // v4 sits so corner (2,3,4) is NOT chord-inverted — only the on-curve
        // corner (1,2,3) carries the inversion, and its apex MOVED, so the
        // still-apex selector has nothing and only the gated arm could act.
        let pre = m(&[
            (0, [5.0, 5.0, 0.0]),
            (1, on_circle(0.10)),
            (2, on_circle(0.15)), // apex sat between its ends…
            (3, on_circle(0.20)),
            (4, [0.99, -1.0, 0.0]),
        ]);
        let post = vec![
            [5.0, 5.0, 0.0],
            on_circle(0.10),
            on_circle(0.28), // …and was relocated past the terminal
            on_circle(0.20),
            [0.99, -1.0, 0.0],
        ];
        let curves = curve_map(&[(1, 2), (2, 3)]);
        let (sites, census) = fold_merge_sites_censused([cyc.as_slice()], &pre, &post, &curves);
        assert_eq!(census.apex_moved_on_curve, 1, "the corner IS censused");
        assert_eq!(census.oncurve_sites, 1, "the always-on arm proposes it");
        assert_eq!(sites.len(), 1, "{sites:?}");
        assert_eq!((sites[0].victim, sites[0].survivor), (2, 3));
    }

    /// A healthy convex cycle has every corner inside its own chord — the
    /// selector must not touch a patch that is fine (P10: never rewrite a valid
    /// boundary).
    #[test]
    fn fold_merge_site_leaves_a_healthy_cycle_alone() {
        let cyc = vec![0u32, 1, 2, 3];
        let pre = m(&[
            (0, [0.0, 0.0, 0.0]),
            (1, [1.0, 0.0, 0.0]),
            (2, [1.0, 1.0, 0.0]),
            (3, [0.0, 1.0, 0.0]),
        ]);
        let post = vec![
            [0.0, 0.0, 0.0],
            [1.1, 0.0, 0.0],
            [1.0, 1.0, 0.0],
            [0.0, 1.0, 0.0],
        ];
        assert!(fold_merge_sites([cyc.as_slice()], &pre, &post).is_empty());
    }

    /// An inversion that ALREADY existed before Stage 4 is inherited geometry,
    /// not a mint — excluded (the repair must not claim it).
    #[test]
    fn fold_merge_site_refuses_an_inherited_inversion() {
        let cyc = vec![0u32, 1, 2, 3];
        // 1 lies past 0 both BEFORE and after; only 0's z nudges.
        let pre = m(&[
            (0, [1.0, 0.0, 0.0]),
            (1, [1.2, 0.0, 0.0]),
            (2, [0.0, 0.0, 0.0]),
            (3, [0.5, -1.0, 0.0]),
        ]);
        let post = vec![
            [1.0, 0.0, 0.001],
            [1.2, 0.0, 0.0],
            [0.0, 0.0, 0.0],
            [0.5, -1.0, 0.0],
        ];
        assert!(fold_merge_sites([cyc.as_slice()], &pre, &post).is_empty());
    }

    /// An inversion whose overrun end never MOVED is not this defect — the
    /// apex must have been overrun BY a relocation.
    #[test]
    fn fold_merge_site_refuses_an_inversion_with_no_moved_end() {
        let cyc = vec![0u32, 1, 2, 3];
        let pre = m(&[
            (0, [1.4, 0.0, 0.0]),
            (1, [1.2, 0.0, 0.0]),
            (2, [0.0, 0.0, 0.0]),
            (3, [0.5, -1.0, 0.0]),
        ]);
        // Nobody moved: the inversion cannot have been minted here, so the
        // pre-order test already excludes it. Move only a far-away vertex.
        let post = vec![
            [1.4, 0.0, 0.0],
            [1.2, 0.0, 0.0],
            [0.0, 0.0, 0.0],
            [0.5, -1.1, 0.0],
        ];
        assert!(fold_merge_sites([cyc.as_slice()], &pre, &post).is_empty());
    }

    /// A MOVED apex is never merged away: it is the one carrying Stage 4's
    /// exact analytic position.
    #[test]
    fn fold_merge_site_never_merges_a_moved_vertex() {
        let cyc = vec![0u32, 1, 2, 3];
        let pre = m(&[
            (0, [1.4, 0.0, 0.0]),
            (1, [1.2, 0.0, 0.0]),
            (2, [0.0, 0.0, 0.0]),
            (3, [0.5, -1.0, 0.0]),
        ]);
        // BOTH ends of the inversion moved — the apex is no longer a plain
        // discretization vertex, so the site is refused.
        let post = vec![
            [1.0, 0.0, 0.0],
            [1.25, 0.0, 0.0],
            [0.0, 0.0, 0.0],
            [0.5, -1.0, 0.0],
        ];
        let sites = fold_merge_sites([cyc.as_slice()], &pre, &post);
        assert!(sites.iter().all(|s| s.victim != 1), "{sites:?}");
    }

    /// Two different survivors claiming one victim is AMBIGUOUS — dropped,
    /// never resolved by a distance guess.
    #[test]
    fn fold_merge_site_drops_an_ambiguous_victim() {
        let a = vec![0u32, 1, 2, 3];
        let b = vec![4u32, 1, 5, 6];
        let pre = m(&[
            (0, [1.4, 0.0, 0.0]),
            (1, [1.2, 0.0, 0.0]),
            (2, [0.0, 0.0, 0.0]),
            (3, [0.5, -1.0, 0.0]),
            (4, [1.4, 0.1, 0.0]),
            (5, [0.0, 0.1, 0.0]),
            (6, [0.5, -1.0, 0.1]),
        ]);
        let post = vec![
            [1.0, 0.0, 0.0],
            [1.2, 0.0, 0.0],
            [0.0, 0.0, 0.0],
            [0.5, -1.0, 0.0],
            [1.1, 0.1, 0.0],
            [0.0, 0.1, 0.0],
            [0.5, -1.0, 0.1],
        ];
        let sites = fold_merge_sites([a.as_slice(), b.as_slice()], &pre, &post);
        assert!(
            sites.iter().all(|s| s.victim != 1),
            "two survivors claim v1, so it is dropped: {sites:?}"
        );
    }

    /// Victims and survivors are disjoint by construction (a survivor moved, a
    /// victim did not), so no batch can ever chain two substitutions. Pinned so
    /// a later relaxation of either condition confronts chaining explicitly.
    #[test]
    fn fold_merge_victims_and_survivors_are_disjoint() {
        let a = vec![0u32, 1, 2, 3];
        let b = vec![7u32, 0, 8, 9];
        let pre = m(&[
            (0, [1.4, 0.0, 0.0]),
            (1, [1.2, 0.0, 0.0]),
            (2, [0.0, 0.0, 0.0]),
            (3, [0.5, -1.0, 0.0]),
            (7, [1.1, 0.0, 0.0]),
            (8, [0.2, 0.0, 0.0]),
            (9, [0.7, -1.0, 0.0]),
        ]);
        let post = vec![
            [1.0, 0.0, 0.0],
            [1.2, 0.0, 0.0],
            [0.0, 0.0, 0.0],
            [0.5, -1.0, 0.0],
            [0.0, 0.0, 0.0],
            [0.0, 0.0, 0.0],
            [0.0, 0.0, 0.0],
            [0.9, 0.0, 0.0],
            [0.2, 0.0, 0.0],
            [0.7, -1.0, 0.0],
        ];
        let sites = fold_merge_sites([a.as_slice(), b.as_slice()], &pre, &post);
        let victims: BTreeSet<u32> = sites.iter().map(|s| s.victim).collect();
        let survivors: BTreeSet<u32> = sites.iter().map(|s| s.survivor).collect();
        assert!(victims.is_disjoint(&survivors), "{sites:?}");
        assert!(!victims.contains(&0), "a moved vertex is never a victim");
    }

    // ---- I13d run-level junction absorption ----------------------------

    /// Positions off the run curve (the rim chain in the measured shape):
    /// scaled circle points, so every conic_param is still defined but the
    /// vertices sit on no typed edge.
    fn off_circle(t: f64) -> [f64; 3] {
        let p = on_circle(t);
        [2.0 * p[0], 2.0 * p[1], p[2]]
    }

    fn absorb(
        cycles: &[Vec<u32>],
        pre: &HashMap<u32, [f64; 3]>,
        post: &[[f64; 3]],
        curves: &BTreeMap<(u32, u32), crate::Curve>,
        richer: impl Fn(u32, u32) -> bool,
    ) -> (Vec<RunAbsorptionSite>, RunAbsorptionCensus) {
        let (sites, census, _) =
            run_absorption_sites(cycles.iter().map(Vec::as_slice), pre, post, curves, richer);
        (sites, census)
    }

    /// The R0003 face-467 shape in miniature: junction 1 at t=0.50, victims
    /// 2 (t 0.55→0.40, deepest) and 3 (t 0.58→0.45) carried past it, then
    /// the healthy ascending chain 4 (0.60), 5 (0.70) up to the far junction
    /// 6 (0.80). The run selector absorbs BOTH victims into junction 1; the
    /// corner selector is blind here (2's chord-sign survivor is 3, whose
    /// far edge continues the curve).
    #[test]
    fn run_absorption_certifies_the_face_467_two_vertex_run() {
        let cyc = vec![0u32, 1, 2, 3, 4, 5, 6, 7];
        let pre = m(&[
            (0, off_circle(0.45)),
            (1, on_circle(0.50)),
            (2, on_circle(0.55)),
            (3, on_circle(0.58)),
            (4, on_circle(0.60)),
            (5, on_circle(0.70)),
            (6, on_circle(0.80)),
            (7, off_circle(0.85)),
        ]);
        let post = vec![
            off_circle(0.45),
            on_circle(0.50),
            on_circle(0.40),
            on_circle(0.45),
            on_circle(0.60),
            on_circle(0.70),
            on_circle(0.80),
            off_circle(0.85),
        ];
        let curves = curve_map(&[(1, 2), (2, 3), (3, 4), (4, 5), (5, 6)]);
        let (sites, census) = absorb(&[cyc], &pre, &post, &curves, |j, _| j == 1);
        assert_eq!(
            sites,
            vec![RunAbsorptionSite {
                survivor: 1,
                victims: vec![2, 3],
            }]
        );
        assert_eq!(census.runs, 1);
        // The far terminal's walk finds no flipped vertex (healthy chain).
        assert_eq!(census.no_flip, 1);
    }

    /// The REAL face-467 shape (measured 2026-08-25): the chain samples
    /// barely move, and the JUNCTION's own relocation solves it past the
    /// first two of them in curve parameter. The one-sided view from the
    /// samples calls this healthy; the pair-order inversion certifies it,
    /// and the same two-victim absorption falls out.
    #[test]
    fn run_absorption_certifies_a_junction_that_hopped_its_samples() {
        let cyc = vec![0u32, 1, 2, 3, 4, 5];
        let pre = m(&[
            (0, off_circle(0.35)),
            (1, on_circle(0.40)), // junction pre: below the whole chain
            (2, on_circle(0.44)),
            (3, on_circle(0.47)),
            (4, on_circle(0.60)),
            (5, off_circle(0.65)),
        ]);
        let post = vec![
            off_circle(0.35),
            on_circle(0.50), // junction solved PAST samples 2 and 3
            on_circle(0.44),
            on_circle(0.47),
            on_circle(0.60),
            off_circle(0.65),
        ];
        let curves = curve_map(&[(1, 2), (2, 3), (3, 4)]);
        let (sites, census) = absorb(&[cyc], &pre, &post, &curves, |j, _| j == 1);
        assert_eq!(
            sites,
            vec![RunAbsorptionSite {
                survivor: 1,
                victims: vec![2, 3],
            }]
        );
        assert_eq!(census.no_flip, 1); // the far terminal's healthy walk
    }

    /// The v3264 shape: ONE victim between two junctions, carried past the
    /// nearer one. Same site the corner arm certifies — the two arms agree.
    #[test]
    fn run_absorption_matches_the_corner_arm_on_a_single_overrun() {
        let cyc = vec![0u32, 1, 2, 3, 4];
        let pre = m(&[
            (0, off_circle(0.65)),
            (1, on_circle(0.60)),
            (2, on_circle(0.55)),
            (3, on_circle(0.50)),
            (4, off_circle(0.45)),
        ]);
        let post = vec![
            off_circle(0.65),
            on_circle(0.60),
            on_circle(0.45),
            on_circle(0.50),
            off_circle(0.45),
        ];
        let curves = curve_map(&[(1, 2), (2, 3)]);
        let (sites, census) = absorb(&[cyc], &pre, &post, &curves, |_, _| true);
        assert_eq!(
            sites,
            vec![RunAbsorptionSite {
                survivor: 3,
                victims: vec![2],
            }]
        );
        assert_eq!(census.no_flip, 1); // the far junction's end
    }

    /// A healthy chain — every vertex pre AND post in-band — produces no
    /// site: the flip test is what separates a legitimate same-curve run
    /// bounded by a junction from an overrun one.
    #[test]
    fn run_absorption_leaves_a_healthy_chain_alone() {
        let cyc = vec![0u32, 1, 2, 3, 4];
        let pts = [
            off_circle(0.65),
            on_circle(0.60),
            on_circle(0.55),
            on_circle(0.50),
            off_circle(0.45),
        ];
        let pre = m(&[
            (0, pts[0]),
            (1, pts[1]),
            (2, on_circle(0.53)), // moved, but stays in-band
            (3, pts[3]),
            (4, pts[4]),
        ]);
        let post = pts.to_vec();
        let curves = curve_map(&[(1, 2), (2, 3)]);
        let (sites, census) = absorb(&[cyc], &pre, &post, &curves, |_, _| true);
        assert!(sites.is_empty());
        assert_eq!(census.no_flip, 2);
    }

    /// A flipped vertex whose corner was ALREADY outside its neighbours'
    /// chord before Stage 4 is inherited geometry: no MINTED inversion, no
    /// absorption.
    #[test]
    fn run_absorption_refuses_an_inherited_fold() {
        let cyc = vec![0u32, 1, 2, 3, 4];
        let pre = m(&[
            (0, off_circle(0.65)),
            (1, on_circle(0.60)),
            (2, on_circle(0.65)), // outside [1,3]'s chord already
            (3, on_circle(0.50)),
            (4, off_circle(0.45)),
        ]);
        let post = vec![
            off_circle(0.65),
            on_circle(0.60),
            on_circle(0.45),
            on_circle(0.50),
            off_circle(0.45),
        ];
        let curves = curve_map(&[(1, 2), (2, 3)]);
        let (sites, census) = absorb(&[cyc], &pre, &post, &curves, |_, _| true);
        assert!(sites.is_empty());
        // Both terminals walk a flipped prefix (the pre position sits outside
        // the whole band), and both refuse it as inherited.
        assert_eq!(census.no_inversion, 2);
    }

    /// The junction must be strictly richer than every victim — a terminal
    /// that is a plain 2-carrier vertex is not a model junction, and
    /// absorbing into it would discard nothing-provable.
    #[test]
    fn run_absorption_requires_the_richer_junction() {
        let cyc = vec![0u32, 1, 2, 3, 4];
        let pre = m(&[
            (0, off_circle(0.65)),
            (1, on_circle(0.60)),
            (2, on_circle(0.55)),
            (3, on_circle(0.50)),
            (4, off_circle(0.45)),
        ]);
        let post = vec![
            off_circle(0.65),
            on_circle(0.60),
            on_circle(0.45),
            on_circle(0.50),
            off_circle(0.45),
        ];
        let curves = curve_map(&[(1, 2), (2, 3)]);
        let (sites, census, cands) =
            run_absorption_sites([cyc.as_slice()], &pre, &post, &curves, |_, _| false);
        assert!(sites.is_empty());
        assert_eq!(census.not_richer, 1);
        // §I13(f): the single-victim refusal is carried OUT with this
        // terminal's inversion params, and those params re-certify the
        // inversion at the apply site (the f2 plumb).
        let [c] = cands[..] else {
            panic!("one rehome candidate, got {cands:?}");
        };
        assert_eq!(c.v, 2, "the flipped vertex is the pair's victim");
        assert!(c.j == 1 || c.j == 3, "the run terminal, got v{}", c.j);
        let t_of = |p: [f64; 3]| {
            crate::stage4_correct::conic_param(&c.curve, cad_primitives::Point3::from(p)).unwrap()
        };
        assert_eq!(c.t_pre_v, t_of(pre[&2]));
        assert_eq!(c.t_post_v, t_of(post[2]));
        assert_eq!(c.t_pre_j, t_of(pre[&c.j]));
        assert_eq!(c.t_post_j, t_of(post[c.j as usize]));
        assert!(crate::stage4_rehome::inversion_still_holds(&c));
    }

    /// Periodic parameters: the flip test wraps deltas to (−π, π], so an
    /// overrun across the circle's branch cut still certifies.
    #[test]
    fn run_absorption_wraps_the_flip_across_the_branch_cut() {
        let cyc = vec![0u32, 1, 2, 3, 4];
        let pre = m(&[
            (0, off_circle(2.7)),
            (1, on_circle(2.8)),
            (2, on_circle(2.9)),
            (3, on_circle(3.0)),
            (4, off_circle(3.05)),
        ]);
        let post = vec![
            off_circle(2.7),
            on_circle(2.8),
            on_circle(-3.1), // ≈ +3.18 wrapped: past the junction at 3.0
            on_circle(3.0),
            off_circle(3.05),
        ];
        let curves = curve_map(&[(1, 2), (2, 3)]);
        let (sites, _) = absorb(&[cyc], &pre, &post, &curves, |_, _| true);
        assert_eq!(
            sites,
            vec![RunAbsorptionSite {
                survivor: 3,
                victims: vec![2],
            }]
        );
    }

    /// The same boundary chain appears in BOTH adjacent patches' cycles —
    /// the reversed walk derives the identical site, deduped to one.
    #[test]
    fn run_absorption_dedupes_across_the_two_holder_cycles() {
        let fwd = vec![0u32, 1, 2, 3, 4];
        let rev = vec![4u32, 3, 2, 1, 0];
        let pre = m(&[
            (0, off_circle(0.65)),
            (1, on_circle(0.60)),
            (2, on_circle(0.55)),
            (3, on_circle(0.50)),
            (4, off_circle(0.45)),
        ]);
        let post = vec![
            off_circle(0.65),
            on_circle(0.60),
            on_circle(0.45),
            on_circle(0.50),
            off_circle(0.45),
        ];
        let curves = curve_map(&[(1, 2), (2, 3)]);
        let (sites, census) = absorb(&[fwd, rev], &pre, &post, &curves, |_, _| true);
        assert_eq!(sites.len(), 1);
        assert_eq!(census.sites, 1);
    }

    /// A vertex shared by TWO curve chains whose pair order inverted against
    /// a different junction on each: two sites with different survivors
    /// claim it — both drop, loudly, and the census names the ambiguity.
    #[test]
    fn run_absorption_drops_an_ambiguous_victim() {
        // C1 = the unit circle; C2 = a unit circle centred at (0.5, 0, 0).
        let c2 = crate::Curve::Circle {
            center: cad_primitives::Point3::new(0.5, 0.0, 0.0),
            normal: crate::Vector3::new(0.0, 0.0, 1.0),
            radius: 1.0,
        };
        let on_c2 = |t: f64| [0.5 + t.cos(), t.sin(), 0.0];
        // Vertex 2 sits on both chains: its C1 order vs junction 1 inverts
        // (0.55 → 0.45 about 0.50), and its C2 order vs junction 3 inverts
        // (C2 params of those positions are ≈0.977 → ≈0.827 about 0.90).
        let cyc_a = vec![8u32, 1, 2, 9];
        let cyc_b = vec![7u32, 3, 2, 6];
        let pre = m(&[
            (8, off_circle(0.40)),
            (1, on_circle(0.50)),
            (2, on_circle(0.55)),
            (9, on_circle(0.60)),
            (7, off_circle(0.40)),
            (3, on_c2(0.90)),
            (6, on_c2(1.05)),
        ]);
        let mut post = vec![[0.0; 3]; 10];
        post[8] = off_circle(0.40);
        post[1] = on_circle(0.50);
        post[2] = on_circle(0.45);
        post[9] = on_circle(0.60);
        post[7] = off_circle(0.40);
        post[3] = on_c2(0.90);
        post[6] = on_c2(1.05);
        let mut curves = curve_map(&[(1, 2)]);
        curves.insert((2, 3), c2);
        let richer = |j: u32, _v: u32| j == 1 || j == 3;
        let (sites, census) = absorb(&[cyc_a, cyc_b], &pre, &post, &curves, richer);
        assert!(sites.is_empty(), "both claims dropped: {sites:?}");
        assert_eq!(census.ambiguous, 1);
    }

    /// A cycle typed end-to-end on one curve is a CLOSED seam — no bounded
    /// end, the reorder authority's shape, never absorbed.
    #[test]
    fn run_absorption_skips_a_fully_typed_cycle() {
        let cyc = vec![0u32, 1, 2, 3];
        let pre = m(&[
            (0, on_circle(0.0)),
            (1, on_circle(1.5)),
            (2, on_circle(3.0)),
            (3, on_circle(4.5)),
        ]);
        let post = vec![
            on_circle(0.0),
            on_circle(1.5),
            on_circle(3.0),
            on_circle(4.5),
        ];
        let curves = curve_map(&[(0, 1), (1, 2), (2, 3), (3, 0)]);
        let (sites, census) = absorb(&[cyc], &pre, &post, &curves, |_, _| true);
        assert!(sites.is_empty());
        assert_eq!(census.runs, 0);
    }

    // ---- I13e interlock_groups ----------------------------------------

    fn vsets(sets: &[&[u32]]) -> Vec<BTreeSet<u32>> {
        sets.iter().map(|s| s.iter().copied().collect()).collect()
    }

    /// The R0003 wall shape: three disjoint pairs, each pair sharing fold
    /// triangles — three components, no cross-pair merging.
    #[test]
    fn interlock_groups_forms_the_measured_pairs() {
        let sites = vsets(&[&[10], &[20], &[30], &[40], &[50], &[60]]);
        let tris: Vec<[u32; 3]> = vec![
            [10, 11, 20], // pair 10-20
            [10, 20, 12],
            [30, 31, 40], // pair 30-40
            [50, 51, 60], // pair 50-60
            [1, 2, 3],    // no victims at all
        ];
        assert_eq!(
            interlock_groups(&sites, &tris),
            vec![vec![0, 1], vec![2, 3], vec![4, 5]]
        );
    }

    /// Transitivity: A shares a triangle with B, B with C — one component
    /// of three, even though A and C never share a triangle. A site with
    /// no shared triangle stays a singleton and is not reported.
    #[test]
    fn interlock_groups_closes_transitively_and_drops_singletons() {
        let sites = vsets(&[&[10], &[20], &[30], &[99]]);
        let tris: Vec<[u32; 3]> = vec![[10, 20, 5], [20, 6, 30], [99, 7, 8]];
        assert_eq!(interlock_groups(&sites, &tris), vec![vec![0, 1, 2]]);
    }

    /// A multi-victim run groups through ANY of its victims, and two
    /// victims of the SAME site in one triangle do not self-group.
    #[test]
    fn interlock_groups_spans_run_victims() {
        let sites = vsets(&[&[10, 11, 12], &[20], &[30]]);
        let tris: Vec<[u32; 3]> = vec![[10, 11, 5], [12, 20, 6], [30, 7, 8]];
        assert_eq!(interlock_groups(&sites, &tris), vec![vec![0, 1]]);
    }

    #[test]
    fn interlock_groups_reports_nothing_without_shared_triangles() {
        let sites = vsets(&[&[10], &[20]]);
        let tris: Vec<[u32; 3]> = vec![[10, 1, 2], [20, 3, 4]];
        assert!(interlock_groups(&sites, &tris).is_empty());
    }
}
