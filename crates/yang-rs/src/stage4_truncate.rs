//! Stage-4 §4.5.1 STEP TRUNCATION — "truncate the step so that the point moves
//! to p on the boundary curve".
//!
//! # The mechanism, and why this class needs it
//!
//! Yang 2025 §4.5.1 "Optimize across boundaries"
//! (`refs/text/yang2025_hybrid_boolean.txt:672-690`):
//!
//! > *Instead of taking a full step length that takes the point to a position
//! > `p1` outside the surface `S2` where the point is initially located, we
//! > truncate the step so that the point moves to `p` on the boundary curve
//! > `C_b` between `S2` and the neighboring surface `S1`. … After obtaining the
//! > correct position of `p`, we first solve the intersection points `q1` and
//! > `q2` on `C_b`.*
//!
//! The 2026-08-06 census (`docs/yang_deviations.md`, update 08-06d) is what
//! points here. Across all 312 corpus cases:
//!
//! * `cross_minted_by_s4 > 0` splits the corpus **perfectly** — 8/47 ERROR,
//!   **0/261** SUPPORTED_CORRECT,
//! * `cross_inherited == 0` **everywhere**: not one self-crossing loop predates
//!   Stage 4.
//!
//! So every self-crossing loop in the corpus is created by a relocation step,
//! and none is inherited. That is why the two §4.4.1 mesh-update trials moved
//! zero cases — there was nothing pre-existing for them to repair — and why the
//! repair belongs here, on the step itself.
//!
//! # HONEST SCOPE: a mechanism borrowed across triggers
//!
//! §4.5.1's *stated* trigger is an **erroneous region** — points that "cannot
//! converge to a distance of 0 within their domains", bounded by two converged
//! points. **Our relocations converge exactly**, which is the same argument that
//! retired the §4.5.2 label for this class on 2026-08-04. So this module
//! implements §4.5.1's MECHANISM under a trigger the paper does not state for
//! it, and that gap is deliberately recorded rather than glossed: calling this
//! class "§4.5.1" would repeat the §4.5.2 mislabel exactly.
//!
//! What justifies the borrow is the census, not the section heading.
//!
//! ## UPDATE 2026-08-20 — the gap above is CLOSED, for a DIFFERENT class
//!
//! The premise "our relocations converge exactly" was true of the class this
//! module was built for, and is measurably false of another one. The §4-I9
//! relocation-domain fire list (`YANG_S4_CARRIER_DOMAIN`) is a population of
//! relocations that converge exactly *as equations* and provably do **not**
//! converge *within their domains*: the traveller rides its carrier model edge,
//! the distance to the far surface falls monotonically to `d_q > 0` at the
//! edge's own endpoint, and reaches 0 only past it. Measured on 24 sites over
//! five cases, with a linear extrapolation predicting the overrun to 0.3–3.6 %.
//!
//! That is §4.5.1's stated trigger, and §4.5.1 describes the defect in the same
//! words: *"a full step length that takes the point to a position `p1` outside
//! the surface `S2` where the point is initially located"*. So the mechanism
//! here and its paper-stated trigger can be joined without borrowing — for that
//! class. The loop-simplicity trigger this module currently answers to remains a
//! borrow, justified by its own census.
//!
//! Before wiring either one, the paper's own selection predicate must be
//! measured (`:740-744` — first strategy only when the failure points are
//! bounded by two successfully optimized points ON THE SAME SURFACE; otherwise
//! §4.5.2). See `specs/yang_441_trim_cdt_construction.md` §4-I10 (d).
//!
//! # What this module computes
//!
//! One question, exactly: **how far along its relocation step can a loop vertex
//! move before its own loop stops being simple?**
//!
//! The candidate steps are found exactly. A segment pair's contact state can
//! only change where one of its orientation determinants vanishes, and with the
//! moved vertex travelling affinely (`v(t) = v0 + t·d`) every one of those
//! determinants is **affine in `t`** — so each pair contributes at most three
//! exact rational roots, and no transition can be missed between them.
//! Classification of the intervals between consecutive roots is delegated to
//! [`crate::stage5_loop_simplicity::scan_cycle`], the same exact classifier the
//! census uses, so the truncation is defined as *the step at which that scan's
//! verdict flips* rather than by a second, possibly-divergent notion of
//! simplicity. Both read the plane through the shared
//! [`crate::stage5_loop_simplicity::projection_axes`].
//!
//! **UNWIRED.** This increment is the primitive plus its measurement; landing
//! the truncated vertex on the analytic boundary curve (via the existing
//! [`crate::stage4_boundary_curve::project_onto_curve`]) and splitting the
//! contacted segment at it — the paper's `q1`/`q2` — is the next increment.

use crate::stage5_loop_simplicity::{projection_axes, scan_cycle};
use dashu::rational::RBig;

/// The outcome of a §4.5.1 step-truncation query.
#[derive(Debug, Clone, PartialEq)]
pub(crate) enum StepTruncation {
    /// The full relocation keeps the loop simple — no truncation needed. This
    /// is the expected answer for the overwhelming majority of relocations.
    FullStepSafe,
    /// The step must stop at `t` (a fraction of the full step). At `t` the loop
    /// has **zero crossings**; beyond it, it crosses.
    ///
    /// At `t` the loop generally **touches** — that is the paper's intent, not a
    /// defect: the point "moves to p ON the boundary curve C_b", and the touch
    /// is then resolved by solving `q1`/`q2`, i.e. splitting the contacted
    /// segment at the landing point. A truncation that stopped strictly short of
    /// contact would be a band; landing exactly on it is the mechanism.
    Truncate { t: f64 },
    /// The loop already CROSSES at `t = 0`, so truncating this step cannot fix
    /// it — the defect predates this relocation. Per the 08-06 census
    /// (`cross_inherited == 0` corpus-wide) this should never fire; if it does,
    /// that is a finding, not a case to swallow.
    AlreadyCrossing,
    /// Fewer than 3 points, a non-finite coordinate, a degenerate normal, or a
    /// zero-length step. Never treated as "safe".
    Unmeasurable,
}

/// Exact 2D point in the dropped-axis projection.
struct P2 {
    x: RBig,
    y: RBig,
}

fn rat(x: f64) -> Option<RBig> {
    crate::coplanar_overlay::rat(x).ok()
}

/// How far `pts[moved]` can travel toward `target` before the loop `pts` stops
/// being simple.
///
/// `pts` is the loop at its PRE-relocation positions (so `pts[moved]` is
/// `t = 0`) and `target` is the fully-relocated position (`t = 1`). `normal` is
/// the loop's plane normal, read through the shared projection.
///
/// Pure and deterministic.
pub(crate) fn max_simple_step(
    pts: &[[f64; 3]],
    moved: usize,
    target: [f64; 3],
    normal: [f64; 3],
) -> StepTruncation {
    let m = pts.len();
    if m < 3 || moved >= m {
        return StepTruncation::Unmeasurable;
    }
    let Some((ax, ay)) = projection_axes(normal) else {
        return StepTruncation::Unmeasurable;
    };
    if pts.iter().any(|p| p.iter().any(|c| !c.is_finite())) || target.iter().any(|c| !c.is_finite())
    {
        return StepTruncation::Unmeasurable;
    }
    // The predicate is CROSSINGS, not full simplicity. A touch is legal here —
    // it is what landing on the boundary curve produces, and what `q1`/`q2`
    // resolve — and it is also the census's own signal (`cross_minted_by_s4`),
    // so the truncation is defined against the same column the class was
    // identified by.
    let crossings_at =
        |p: &[[f64; 3]]| -> Option<usize> { scan_cycle(p, normal).map(|s| s.crossings) };
    match crossings_at(pts) {
        Some(0) => {}
        Some(_) => return StepTruncation::AlreadyCrossing,
        None => return StepTruncation::Unmeasurable,
    }
    // A zero-length step has nothing to truncate.
    if pts[moved] == target {
        return StepTruncation::FullStepSafe;
    }
    // If the full step already crosses nothing there is nothing to do — checked
    // first because it is the common case and skips all root finding.
    let mut full = pts.to_vec();
    full[moved] = target;
    if crossings_at(&full) == Some(0) {
        return StepTruncation::FullStepSafe;
    }

    // ---- Exact candidate roots. -----------------------------------------
    let Some(p2) = pts
        .iter()
        .map(|p| {
            Some(P2 {
                x: rat(p[ax])?,
                y: rat(p[ay])?,
            })
        })
        .collect::<Option<Vec<_>>>()
    else {
        return StepTruncation::Unmeasurable;
    };
    let (Some(tx), Some(ty)) = (rat(target[ax]), rat(target[ay])) else {
        return StepTruncation::Unmeasurable;
    };
    let v0 = &p2[moved];
    let dx = &tx - &v0.x;
    let dy = &ty - &v0.y;

    let prev = (moved + m - 1) % m;
    let next = (moved + 1) % m;
    // The two segments the moved vertex belongs to, as (fixed endpoint, v(t)).
    let incident = [prev, next];

    let mut roots: Vec<RBig> = Vec::new();
    // Root of the affine function `c0 + t*c1` inside (0, 1).
    let push_root = |c0: RBig, c1: RBig, roots: &mut Vec<RBig>| {
        if c1 == RBig::from(0u8) {
            return;
        }
        let t = -c0 / c1;
        if t > RBig::from(0u8) && t < RBig::from(1u8) {
            roots.push(t);
        }
    };

    for &a_idx in &incident {
        let a = &p2[a_idx];
        for s in 0..m {
            let e_idx = (s + 1) % m;
            // Skip segments sharing a vertex with this incident segment: they
            // meet legitimately at that vertex, and `scan_cycle` classifies
            // their backtracking case as a spike, not a crossing.
            if s == a_idx || s == moved || e_idx == a_idx || e_idx == moved {
                continue;
            }
            let (c, e) = (&p2[s], &p2[e_idx]);
            // A(t) = cross(a, v(t), c),  B(t) = cross(a, v(t), e),
            // D(t) = cross(c, e, v(t))  — each affine in t.
            let a0 = (&v0.x - &a.x) * (&c.y - &a.y) - (&v0.y - &a.y) * (&c.x - &a.x);
            let a1 = &dx * (&c.y - &a.y) - &dy * (&c.x - &a.x);
            push_root(a0, a1, &mut roots);

            let b0 = (&v0.x - &a.x) * (&e.y - &a.y) - (&v0.y - &a.y) * (&e.x - &a.x);
            let b1 = &dx * (&e.y - &a.y) - &dy * (&e.x - &a.x);
            push_root(b0, b1, &mut roots);

            let d0 = (&e.x - &c.x) * (&v0.y - &c.y) - (&e.y - &c.y) * (&v0.x - &c.x);
            let d1 = (&e.x - &c.x) * &dy - (&e.y - &c.y) * &dx;
            push_root(d0, d1, &mut roots);
        }
    }
    roots.sort();
    roots.dedup();
    // The full step is known non-simple, so `t = 1` bounds the search.
    roots.push(RBig::from(1u8));

    // ---- Walk the roots ASCENDING; stop at the first crossing. ----------
    // Ascending and conservative on purpose: crossings need not be monotone in
    // `t` (a vertex can cross out and back), and stepping THROUGH a crossing to
    // reach a later crossing-free position would sweep the edge across the
    // outline — exactly the motion §4.5.1 forbids. So the answer is the last
    // crossing-free root before the first crossing, never a later one.
    let mut last_safe = 0.0f64;
    let mut lo = RBig::from(0u8);
    for r in &roots {
        let (Some(mid), Some(r_f)) = (rbig_to_f64(&((&lo + r) / RBig::from(2u8))), rbig_to_f64(r))
        else {
            return StepTruncation::Unmeasurable;
        };
        let mut probe = pts.to_vec();
        probe[moved] = lerp(pts[moved], target, mid);
        if crossings_at(&probe) != Some(0) {
            break; // the interval (lo, r) crosses — `last_safe` stands
        }
        // The interval is clear; can we stand exactly ON this root? At a root
        // the configuration is tangential, so this is normally a touch with
        // zero crossings — verified rather than assumed, because `r` is rounded
        // to f64 here and the verdict must hold at the value we actually return.
        probe[moved] = lerp(pts[moved], target, r_f);
        if crossings_at(&probe) != Some(0) {
            break;
        }
        last_safe = r_f;
        lo = r.clone();
    }
    StepTruncation::Truncate { t: last_safe }
}

fn lerp(a: [f64; 3], b: [f64; 3], t: f64) -> [f64; 3] {
    [
        a[0] + t * (b[0] - a[0]),
        a[1] + t * (b[1] - a[1]),
        a[2] + t * (b[2] - a[2]),
    ]
}

/// Nearest `f64` to an exact rational, via `dashu`'s float conversion.
fn rbig_to_f64(r: &RBig) -> Option<f64> {
    let v = r.to_f64().value();
    v.is_finite().then_some(v)
}

/// The outcome of a §4.5.1 DOMAIN truncation query.
#[derive(Debug, Clone, PartialEq)]
pub(crate) enum DomainTruncation {
    /// The full step stays inside the carrier's domain — nothing to truncate.
    /// This is the expected answer for the overwhelming majority of steps.
    FullStepInDomain,
    /// The step leaves the domain at vertex `at`, which lies on the step
    /// segment; stop there. `t` is the fraction of the full step at which that
    /// happens.
    ///
    /// The caller must land the point on the STORED POSITION of `at`, not on
    /// `lerp(pre, post, t)`: `at` is a model corner whose coordinates are exact
    /// input data, while `t` is a rounded projection. `t` is returned for
    /// ordering and diagnostics, not as the landing point.
    TruncateAtVertex { t: f64, at: u32 },
    /// A zero-length step, or a non-finite coordinate. Never treated as "the
    /// full step is fine".
    Unmeasurable,
}

/// §4.5.1 DOMAIN truncation — *"instead of taking a full step length that takes
/// the point to a position `p1` outside the surface `S2` where the point is
/// initially located, we truncate the step so that the point moves to `p` on the
/// boundary curve `C_b`"* (`refs/text/yang2025_hybrid_boolean.txt:672-690`).
///
/// This answers the domain half of the same question [`max_simple_step`] answers
/// for loop simplicity: **how far along its step can a vertex move before it
/// leaves its carrier's bounded domain?**
///
/// `candidates` are the domain-boundary vertices the caller has certified — in
/// the §4-I9 wiring, the still neighbours carrying a surface the relocated
/// position is OFF, which is what makes them domain ENDPOINTS (a third face
/// joins) rather than plain samples of the traveller's own carrier. Selecting
/// them is the caller's job precisely because that certificate is what
/// distinguishes a domain end from a point Yang's near-curve removal owns; this
/// function only decides WHICH of them the step reaches first, and where.
///
/// A candidate counts when it lies strictly inside the `pre → post` segment at
/// the project's shared relative collinearity identity
/// ([`crate::stage4_construct::point_on_segment_interior`]) — the same gate
/// §4-I9 fires on, so the repair cannot fire on a configuration the STOP would
/// not have named, nor decline one it would.
///
/// Measured shape of the class this serves (§4-I10, 24 sites / 5 cases): the
/// traveller rides its carrier model edge and overruns the edge's own endpoint,
/// so the truncation parameter is the corner's position along the step —
/// R0074's site at `t ≈ 0.296` of a `7.40e-4` travel, R0011's four between
/// `t ≈ 0.21` and `t ≈ 0.67`.
///
/// Pure and deterministic. Ties (two candidates at the same `t`) resolve by
/// lowest vertex index so the answer does not depend on input order.
///
/// **This is the truncation only.** §4.5.1 continues by re-parameterizing the
/// landed point on the neighbouring surface `S1` and solving `q1`/`q2` on `C_b`;
/// that is the next increment, and until it exists a caller must leave the
/// §4-I9 STOP standing rather than accept the landed point as a final answer.
pub(crate) fn max_in_domain_step(
    pre: [f64; 3],
    post: [f64; 3],
    candidates: &[(u32, [f64; 3])],
) -> DomainTruncation {
    let d = [post[0] - pre[0], post[1] - pre[1], post[2] - pre[2]];
    let dd = d[0] * d[0] + d[1] * d[1] + d[2] * d[2];
    if !dd.is_finite() || dd <= 0.0 || !pre.iter().chain(post.iter()).all(|c| c.is_finite()) {
        return DomainTruncation::Unmeasurable;
    }
    let mut best: Option<(f64, u32)> = None;
    for &(vid, q) in candidates {
        if !q.iter().all(|c| c.is_finite()) {
            continue;
        }
        if !crate::stage4_construct::point_on_segment_interior(pre, post, q) {
            continue;
        }
        let t = ((q[0] - pre[0]) * d[0] + (q[1] - pre[1]) * d[1] + (q[2] - pre[2]) * d[2]) / dd;
        if !t.is_finite() || t <= 0.0 || t >= 1.0 {
            // `point_on_segment_interior` already certified strict interiority;
            // a projection that disagrees is a degenerate case, not a boundary.
            continue;
        }
        match best {
            Some((bt, bv)) if (bt, bv) <= (t, vid) => {}
            _ => best = Some((t, vid)),
        }
    }
    match best {
        Some((t, at)) => DomainTruncation::TruncateAtVertex { t, at },
        None => DomainTruncation::FullStepInDomain,
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    const NZ: [f64; 3] = [0.0, 0.0, 1.0];

    fn xy(v: &[(f64, f64)]) -> Vec<[f64; 3]> {
        v.iter().map(|&(x, y)| [x, y, 0.0]).collect()
    }

    #[test]
    fn a_step_that_keeps_the_loop_simple_is_not_truncated() {
        // Unit square; nudge one corner slightly outward — still simple.
        let sq = xy(&[(0.0, 0.0), (1.0, 0.0), (1.0, 1.0), (0.0, 1.0)]);
        assert_eq!(
            max_simple_step(&sq, 2, [1.2, 1.1, 0.0], NZ),
            StepTruncation::FullStepSafe
        );
    }

    #[test]
    fn a_zero_length_step_is_safe() {
        let sq = xy(&[(0.0, 0.0), (1.0, 0.0), (1.0, 1.0), (0.0, 1.0)]);
        assert_eq!(
            max_simple_step(&sq, 2, [1.0, 1.0, 0.0], NZ),
            StepTruncation::FullStepSafe
        );
    }

    /// THE §4.5.1 SHAPE. A vertex of a non-convex loop is driven far enough
    /// across the outline that its incident edges cut a distant segment — the
    /// census's "displacement exceeds the local segment it belongs to".
    #[test]
    fn a_step_that_drives_a_vertex_across_the_outline_is_truncated() {
        // A C-shape (notch open to the right). Vertex 3 sits in the notch
        // mouth; pushing it far left drags its edges through the left wall.
        let c = xy(&[
            (0.0, 0.0),
            (3.0, 0.0),
            (3.0, 1.0),
            (1.0, 1.0),
            (1.0, 2.0),
            (3.0, 2.0),
            (3.0, 3.0),
            (0.0, 3.0),
        ]);
        let target = [-2.0, 1.0, 0.0];
        match max_simple_step(&c, 3, target, NZ) {
            StepTruncation::Truncate { t } => {
                assert!((0.0..1.0).contains(&t), "t must be a partial step, got {t}");
                // Verify the CONTRACT at the value actually returned: zero
                // crossings there, and crossings at the full step. A touch at
                // `t` is expected — that IS landing on the boundary.
                let mut at_t = c.clone();
                at_t[3] = lerp(c[3], target, t);
                assert_eq!(
                    scan_cycle(&at_t, NZ).unwrap().crossings,
                    0,
                    "the returned step must not cross"
                );
                let mut at_1 = c.clone();
                at_1[3] = target;
                assert!(
                    scan_cycle(&at_1, NZ).unwrap().crossings > 0,
                    "the full step must cross, or the fixture proves nothing"
                );
            }
            other => panic!("expected a truncation, got {other:?}"),
        }
    }

    /// The truncation must be the FIRST contact, not merely some crossing-free
    /// step: stepping through a crossing to reach a later clear position sweeps
    /// the edge across the outline, which is the motion §4.5.1 forbids.
    #[test]
    fn truncation_stops_at_the_first_crossing_not_a_later_clear_position() {
        let c = xy(&[
            (0.0, 0.0),
            (3.0, 0.0),
            (3.0, 1.0),
            (1.0, 1.0),
            (1.0, 2.0),
            (3.0, 2.0),
            (3.0, 3.0),
            (0.0, 3.0),
        ]);
        let target = [-2.0, 1.0, 0.0];
        let StepTruncation::Truncate { t } = max_simple_step(&c, 3, target, NZ) else {
            panic!("expected a truncation");
        };
        // Every step strictly below `t` must also be crossing-free.
        for k in 1..20 {
            let s = t * (k as f64) / 20.0;
            let mut probe = c.clone();
            probe[3] = lerp(c[3], target, s);
            assert_eq!(
                scan_cycle(&probe, NZ).unwrap().crossings,
                0,
                "step {s} below the truncation {t} must not cross"
            );
        }
    }

    #[test]
    fn a_loop_that_already_self_crosses_is_reported_not_truncated() {
        // Figure-eight: non-simple at t = 0, so no truncation of this step can
        // rescue it. Per the census this should never occur in the corpus.
        let fig8 = xy(&[(0.0, 0.0), (2.0, 2.0), (2.0, 0.0), (0.0, 2.0)]);
        assert_eq!(
            max_simple_step(&fig8, 0, [-1.0, 0.0, 0.0], NZ),
            StepTruncation::AlreadyCrossing
        );
    }

    #[test]
    fn degenerate_inputs_are_unmeasurable_never_safe() {
        let sq = xy(&[(0.0, 0.0), (1.0, 0.0), (1.0, 1.0), (0.0, 1.0)]);
        // Degenerate normal.
        assert_eq!(
            max_simple_step(&sq, 0, [0.5, 0.5, 0.0], [0.0, 0.0, 0.0]),
            StepTruncation::Unmeasurable
        );
        // Out-of-range index.
        assert_eq!(
            max_simple_step(&sq, 9, [0.5, 0.5, 0.0], NZ),
            StepTruncation::Unmeasurable
        );
        // Non-finite target.
        assert_eq!(
            max_simple_step(&sq, 0, [f64::NAN, 0.0, 0.0], NZ),
            StepTruncation::Unmeasurable
        );
        // Too few points.
        assert_eq!(
            max_simple_step(&xy(&[(0.0, 0.0), (1.0, 0.0)]), 0, [0.5, 0.5, 0.0], NZ),
            StepTruncation::Unmeasurable
        );
    }

    /// The truncation must not depend on which plane axis is dropped — it reads
    /// the same shared projection the census does.
    #[test]
    fn truncation_is_projection_axis_invariant() {
        let c = xy(&[
            (0.0, 0.0),
            (3.0, 0.0),
            (3.0, 1.0),
            (1.0, 1.0),
            (1.0, 2.0),
            (3.0, 2.0),
            (3.0, 3.0),
            (0.0, 3.0),
        ]);
        let in_xy = max_simple_step(&c, 3, [-2.0, 1.0, 0.0], NZ);
        // Same loop re-embedded in the y = 0 plane (x, z), normal +y.
        let c2: Vec<[f64; 3]> = c.iter().map(|p| [p[0], 0.0, p[1]]).collect();
        let in_xz = max_simple_step(&c2, 3, [-2.0, 0.0, 1.0], [0.0, 1.0, 0.0]);
        assert_eq!(in_xy, in_xz);
    }

    // ---- §4.5.1 DOMAIN truncation (`max_in_domain_step`) ----

    /// The overwhelmingly common answer: nothing on the step's path.
    #[test]
    fn domain_full_step_when_no_candidate_lies_on_the_segment() {
        let (pre, post) = ([0.0, 0.0, 0.0], [1.0, 0.0, 0.0]);
        let cands = [(7u32, [0.5, 0.3, 0.0]), (9, [2.0, 0.0, 0.0])];
        assert_eq!(
            max_in_domain_step(pre, post, &cands),
            DomainTruncation::FullStepInDomain
        );
    }

    /// An empty candidate list is "nothing in the way", not "unmeasurable".
    #[test]
    fn domain_no_candidates_is_a_full_step() {
        assert_eq!(
            max_in_domain_step([0.0, 0.0, 0.0], [1.0, 0.0, 0.0], &[]),
            DomainTruncation::FullStepInDomain
        );
    }

    #[test]
    fn domain_truncates_at_the_candidate_on_the_segment() {
        let (pre, post) = ([0.0, 0.0, 0.0], [1.0, 0.0, 0.0]);
        match max_in_domain_step(pre, post, &[(4u32, [0.25, 0.0, 0.0])]) {
            DomainTruncation::TruncateAtVertex { t, at } => {
                assert_eq!(at, 4);
                assert!((t - 0.25).abs() < 1e-12, "t = {t}");
            }
            other => panic!("expected a truncation, got {other:?}"),
        }
    }

    /// The step stops at the FIRST domain boundary it reaches, not the last.
    #[test]
    fn domain_picks_the_first_boundary_along_the_step() {
        let (pre, post) = ([0.0, 0.0, 0.0], [1.0, 0.0, 0.0]);
        let cands = [(11u32, [0.8, 0.0, 0.0]), (3, [0.2, 0.0, 0.0])];
        match max_in_domain_step(pre, post, &cands) {
            DomainTruncation::TruncateAtVertex { t, at } => {
                assert_eq!(at, 3, "the nearer boundary wins regardless of order");
                assert!((t - 0.2).abs() < 1e-12, "t = {t}");
            }
            other => panic!("expected a truncation, got {other:?}"),
        }
    }

    /// Order-independence: a tie resolves by vertex index, both ways round.
    #[test]
    fn domain_ties_resolve_by_lowest_vertex_index() {
        let (pre, post) = ([0.0, 0.0, 0.0], [1.0, 0.0, 0.0]);
        let q = [0.5, 0.0, 0.0];
        for cands in [[(8u32, q), (2u32, q)], [(2u32, q), (8u32, q)]] {
            match max_in_domain_step(pre, post, &cands) {
                DomainTruncation::TruncateAtVertex { at, .. } => assert_eq!(at, 2),
                other => panic!("expected a truncation, got {other:?}"),
            }
        }
    }

    /// The endpoints are not INTERIOR, so neither is a domain crossing: a step
    /// that merely ends on a boundary vertex has not left the domain.
    #[test]
    fn domain_endpoints_are_not_a_crossing() {
        let (pre, post) = ([0.0, 0.0, 0.0], [1.0, 0.0, 0.0]);
        for q in [pre, post] {
            assert_eq!(
                max_in_domain_step(pre, post, &[(1u32, q)]),
                DomainTruncation::FullStepInDomain,
                "q = {q:?}"
            );
        }
    }

    #[test]
    fn domain_zero_length_step_is_unmeasurable() {
        let p = [1.0, 2.0, 3.0];
        assert_eq!(
            max_in_domain_step(p, p, &[(1u32, p)]),
            DomainTruncation::Unmeasurable
        );
    }

    #[test]
    fn domain_non_finite_is_unmeasurable() {
        assert_eq!(
            max_in_domain_step([0.0, 0.0, 0.0], [f64::NAN, 0.0, 0.0], &[]),
            DomainTruncation::Unmeasurable
        );
        assert_eq!(
            max_in_domain_step([f64::INFINITY, 0.0, 0.0], [1.0, 0.0, 0.0], &[]),
            DomainTruncation::Unmeasurable
        );
    }

    /// A non-finite CANDIDATE is skipped, not fatal — the rest of the list still
    /// decides.
    #[test]
    fn domain_non_finite_candidate_is_skipped() {
        let (pre, post) = ([0.0, 0.0, 0.0], [1.0, 0.0, 0.0]);
        let cands = [(1u32, [f64::NAN, 0.0, 0.0]), (2, [0.4, 0.0, 0.0])];
        match max_in_domain_step(pre, post, &cands) {
            DomainTruncation::TruncateAtVertex { at, .. } => assert_eq!(at, 2),
            other => panic!("expected a truncation, got {other:?}"),
        }
    }

    /// The measured §4-I10 shape, at its measured SCALE: R0074's site travels
    /// 7.40e-4 at coordinates of order 1e-1 and overruns its corner by 5.21e-4,
    /// i.e. the corner sits at t ≈ 0.296. A gate with an absolute floor anywhere
    /// in it would call this step degenerate; the shared relative identity does
    /// not. (The absolute-floor class is the 2026-08-19b / 08-19c anchor.)
    #[test]
    fn domain_truncation_holds_at_the_measured_r0074_scale() {
        let pre: [f64; 3] = [-0.019966852, 0.101451355, 0.131249979];
        let post: [f64; 3] = [-0.020060107, 0.101700514, 0.130558979];
        let travel =
            ((post[0] - pre[0]).powi(2) + (post[1] - pre[1]).powi(2) + (post[2] - pre[2]).powi(2))
                .sqrt();
        assert!((travel - 7.4044e-4).abs() < 1e-8, "travel = {travel:e}");
        let t_expected = 1.0 - 5.2128e-4 / travel;
        let q = lerp(pre, post, t_expected);
        match max_in_domain_step(pre, post, &[(127u32, q)]) {
            DomainTruncation::TruncateAtVertex { t, at } => {
                assert_eq!(at, 127);
                assert!((t - t_expected).abs() < 1e-9, "t = {t}, want {t_expected}");
                assert!((t - 0.296).abs() < 1e-3, "t = {t}");
            }
            other => panic!("expected a truncation, got {other:?}"),
        }
    }
}
