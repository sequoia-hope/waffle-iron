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
}
