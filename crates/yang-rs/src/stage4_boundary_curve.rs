//! Stage-4 §4.4.1 boundary-curve relocation — "boundary curves map to boundary
//! curves" (spec `specs/yang_s4_boundary_curve_relocation.md`, inc-1).
//!
//! Yang 2025 §4.4.1 / Fig. 11 (`refs/text/yang2025_hybrid_boolean.txt:545-565`)
//! covers the case where the two mesh patches are "two adjacent surfaces from
//! ONE B-Rep model" and the intersection point q lies "on the boundary curve" —
//! i.e. on that operand's OWN rim. The trimmed triangulation must "map boundary
//! curves to boundary curves": every mesh vertex representing a point of a rim
//! has to lie ON the rim curve.
//!
//! `build_intersection_curves` (`stage3_ssi.rs`) only ever claims CROSS-input
//! edges, so an operand's own rim vertices are never relocated and can survive
//! at their Stage-1 CHORD position — measured as the n2 I1 fixture's `v6`
//! (6.840109e-7, exactly on the chord) and R0063 face 636 (3.126e-5, perturbed
//! 2.409e-7 OFF the chord). This module is the pointwise repair.
//!
//! **Status: ALWAYS-ON since inc-5** (was gated OFF behind
//! `YANG_S4_RIM_SNAP_ENABLE`). Wired into `stage4_relocate_and_correct`. It
//! fixes the n2 I1 reproduction (moves exactly `v6`) and is corpus-neutral on
//! its own; it was flipped together with the §4.5.4 rim×plane graze refinement
//! in `boolean`, which DEPENDS on it — boosting the rim exposes this same
//! latent relocation gap, so `n2_junction_cluster::i1` is RED with the graze
//! arm alone and GREEN with both. Combined flip measured on the full 312-case
//! corpus: 252C→254C (R0072, R0095 ERROR→CORRECT), 0W, zero CORRECT→ERROR.
//!
//! It does NOT reach the corpus tail (F0083/R0099 still `VertexOffSurface`) —
//! the spec's §17 records why: that class is a Stage-3 `on_both` gate defect,
//! not a §4.4.1 one, and cannot be fixed from edge-local data.

use crate::errors::{Stage4InvalidReason, YangError};
use crate::geom::{Curve, Surface};
use crate::InputId;
use crate::Mesh;
use cad_primitives::{Point3, TAU_WORK};

/// Exact closest point on an analytic boundary `Curve`.
///
/// `Circle` is exact and closed-form: project into the circle's plane, then
/// rescale the in-plane radial component to the radius. Returns `None` for a
/// point ON the axis (the closest point is not unique — a degenerate input this
/// pass must not guess at) and for curve kinds outside this increment's scope
/// (every measured instance of the defect is a cylinder-patch rim, i.e. a
/// `Circle`); `None` means "skip this vertex", never "snap it anyway".
pub(crate) fn project_onto_curve(p: Point3, curve: &Curve) -> Option<Point3> {
    match *curve {
        Curve::Circle {
            center,
            normal,
            radius,
        } => {
            let c = center.as_array();
            let n = normal.as_array();
            let nl = (n[0] * n[0] + n[1] * n[1] + n[2] * n[2]).sqrt();
            // NaN-safe: a NaN normal length or radius must SKIP, never snap.
            if nl.is_nan() || nl <= 0.0 || !radius.is_finite() || radius <= 0.0 {
                return None;
            }
            let nu = [n[0] / nl, n[1] / nl, n[2] / nl];
            let pa = p.as_array();
            let d = [pa[0] - c[0], pa[1] - c[1], pa[2] - c[2]];
            let h = d[0] * nu[0] + d[1] * nu[1] + d[2] * nu[2];
            // In-plane (radial) component.
            let r = [d[0] - h * nu[0], d[1] - h * nu[1], d[2] - h * nu[2]];
            let rl = (r[0] * r[0] + r[1] * r[1] + r[2] * r[2]).sqrt();
            if rl.is_nan() || rl <= 0.0 {
                // On the axis: the closest point is the whole circle.
                return None;
            }
            let s = radius / rl;
            Some(Point3::new(
                c[0] + r[0] * s,
                c[1] + r[1] * s,
                c[2] + r[2] * s,
            ))
        }
        _ => None,
    }
}

/// The §4.4.1 decision for ONE vertex against its boundary curve (spec §4
/// steps 3-4).
///
/// - `Ok(None)` — nothing to do: no projection available (out-of-scope curve
///   kind / degenerate), or the vertex is already on the curve within
///   `TAU_WORK` (a bit-exact no-op, so a clean corpus stays byte-identical).
/// - `Ok(Some(q))` — relocate to `q`. Reached only when the displacement is
///   within `bound`, the owner's Stage-1 chord bound: the vertex was never
///   further from its own rim than Stage 1 already guarantees, which is exactly
///   the chord-position artifact class.
/// - `Err(..)` — LOUD STOP (P9/P10). A vertex further from the rim than the
///   chord bound is NOT this class, and snapping it would be tolerance widening
///   producing a right answer for a wrong reason.
///
/// Note the guard only ever REFUSES; it never admits anything. The relocation
/// itself is an exact projection onto an exact curve.
pub(crate) fn boundary_relocation_for_vertex(
    vertex: u32,
    p: Point3,
    curve: &Curve,
    bound: f64,
) -> Result<Option<Point3>, YangError> {
    let Some(q) = project_onto_curve(p, curve) else {
        return Ok(None);
    };
    let pa = p.as_array();
    let qa = q.as_array();
    let d = ((qa[0] - pa[0]).powi(2) + (qa[1] - pa[1]).powi(2) + (qa[2] - pa[2]).powi(2)).sqrt();
    if d <= TAU_WORK {
        return Ok(None);
    }
    if d > bound {
        return Err(YangError::stage4_region_invalid(
            vertex,
            Stage4InvalidReason::LocalRefinementRequired,
        ));
    }
    Ok(Some(q))
}

/// Plan the §4.4.1 relocations for a whole mesh (spec §4).
///
/// `rim_curves` maps an operand's OWN boundary edges (same-input incidence with
/// two DIFFERENT surfaces) to their analytic curve (`collect_rim_curves`).
/// `cross_curve_endpoints` is the set of vertices claimed by CROSS-input
/// intersection curves — A×B junctions that are already relocated and are
/// required to lie on BOTH curves. Moving one would break that, so they are
/// excluded by construction (measured: the I1 fixture's exact junction `v5` at
/// +7.0361° must not move, while `v6` must).
///
/// **Membership is VERIFIED per edge, not assumed** (inc-2, measured). A
/// same-input `Cylinder`+`Plane` patch adjacency does NOT imply the shared edge
/// lies on cylinder∩plane: after a boolean, a cylinder patch can be adjacent to
/// a plane patch along a trimming boundary nowhere near the analytic rim
/// (measured on `m8_nary_tessellated_overlay::flush_pocket_subtract_and_union_partition`,
/// which STOPped when the candidate curve was trusted blindly). So the derived
/// circle is a CANDIDATE, and an edge is accepted as lying on it only when BOTH
/// endpoints are within `bound` of it. An endpoint outside the bound means "this
/// edge is not that rim" — the pass makes no claim and skips the edge; it does
/// NOT snap it and does not treat it as a defect.
///
/// Returns the moves in deterministic vertex order. Pure: mutates nothing.
pub(crate) fn plan_boundary_relocations(
    mesh: &Mesh,
    rim_curves: &std::collections::BTreeMap<(u32, u32), Curve>,
    incidence: &std::collections::BTreeMap<(u32, u32), Vec<(InputId, Surface)>>,
    cross_curve_endpoints: &std::collections::BTreeSet<u32>,
    bound: f64,
) -> Vec<(u32, Point3)> {
    let mut moves: std::collections::BTreeMap<u32, Point3> = std::collections::BTreeMap::new();
    'edges: for (&(s, e), curve) in rim_curves {
        // Verification pass: every endpoint must be ON this candidate curve
        // within `bound`, else the edge is not this rim.
        let mut pending: Vec<(u32, Point3)> = Vec::new();
        for v in [s, e] {
            let Some(&p) = mesh.verts.get(v as usize) else {
                continue 'edges;
            };
            match boundary_relocation_for_vertex(v, p, curve, bound) {
                // Beyond the bound ⇒ not this rim ⇒ abandon the whole edge.
                Err(_) => continue 'edges,
                Ok(Some(q)) => pending.push((v, q)),
                Ok(None) => {}
            }
        }
        // Only now commit, and never for a vertex a cross-input curve owns.
        let own: &[(InputId, Surface)] = incidence
            .get(&(s, e))
            .map(|es| es.as_slice())
            .unwrap_or(&[]);
        for (v, q) in pending {
            if cross_curve_endpoints.contains(&v) {
                continue;
            }
            // inc-6 (spec §21), ALWAYS-ON: the projection consumes the rim pair
            // ONLY, so a further incident surface is a constraint it would
            // silently drop — F0067's vertex was ON its flank plane to 2.8e-17
            // and 4.1e-5 off it after the snap. Seat at the certificate instead.
            let Some(&p) = mesh.verts.get(v as usize) else {
                continue;
            };
            let others = unconsumed_surfaces_for_vertex(v, own, incidence);
            match seat_against_unconsumed(p, q, curve, &others, bound) {
                Some(q2) => {
                    moves.entry(v).or_insert(q2);
                }
                // No derivable seat ⇒ make no claim, exactly as this pass does
                // for an edge it cannot verify. Projecting onto the rim alone is
                // the measured defect, not an acceptable fallback.
                None => continue,
            }
        }
    }
    moves.into_iter().collect()
}

/// The surfaces incident to `v` that the rim projection does NOT consume.
///
/// `own` is the rim edge's own surface pair — the two the projection accounts
/// for. Everything else incident to the vertex is a further constraint, EXCEPT
/// a coplanar duplicate of an own surface: at a flush junction the other operand
/// contributes a cap 5e-16 from the rim's own cap, which constrains nothing and
/// is the reason F0067's quadruple point defeats a uniqueness guard (§19).
///
/// Deduped by VALUE only; labels are never deduped.
pub(crate) fn unconsumed_surfaces_for_vertex(
    v: u32,
    own: &[(InputId, Surface)],
    incidence: &std::collections::BTreeMap<(u32, u32), Vec<(InputId, Surface)>>,
) -> Vec<Surface> {
    let mut out: Vec<Surface> = Vec::new();
    for (&(a, b), entries) in incidence {
        if a != v && b != v {
            continue;
        }
        for &(_, sf) in entries {
            if own.iter().any(|&(_, s2)| s2 == sf) {
                continue;
            }
            if own.iter().any(|&(_, s2)| planes_are_duplicates(sf, s2)) {
                continue;
            }
            if !out.contains(&sf) {
                out.push(sf);
            }
        }
    }
    out
}

/// Where a rim vertex carrying `others` must actually be seated.
///
/// - **no unconsumed surface** — the rim projection `q` is the whole answer
///   (measured: 89 of inc-2's 101 corpus snaps, byte-identical);
/// - **exactly one, a plane** — the seat is the `Circle ∩ Plane` root, a
///   CERTIFICATE satisfying both the rim and the surface the projection would
///   have dropped. Subject to the pass's existing `bound`: a root further from
///   the vertex than the owner's own Stage-1 chord guarantee is not this class.
/// - **anything else** (a non-plane surface, or more than one) — `None`: the
///   seat is not derivable in closed form here, and projecting onto the rim
///   alone is the measured defect, not an acceptable fallback.
pub(crate) fn seat_against_unconsumed(
    p: Point3,
    q: Point3,
    curve: &Curve,
    others: &[Surface],
    bound: f64,
) -> Option<Point3> {
    match others {
        [] => Some(q),
        [Surface::Plane { normal, d }] => {
            let seat = circle_plane_nearest_root(curve, *normal, *d, p)?;
            let (a, b) = (p.as_array(), seat.as_array());
            let dist =
                ((a[0] - b[0]).powi(2) + (a[1] - b[1]).powi(2) + (a[2] - b[2]).powi(2)).sqrt();
            (dist.is_finite() && dist <= bound).then_some(seat)
        }
        _ => None,
    }
}

/// The analytic rim curve of a same-operand surface PAIR, in closed form.
///
/// inc-2 scope is the measured class: a cylinder patch meeting a planar cap on
/// a plane PERPENDICULAR to the cylinder axis, whose intersection is exactly a
/// `Circle`. Anything else — an oblique plane (an ellipse), a non-cylinder pair
/// — returns `None` and is skipped, never approximated. No SSI call is needed
/// and no selection tolerance arises, because the pair itself names the curve.
pub(crate) fn rim_circle_from_pair(surf0: Surface, surf1: Surface) -> Option<Curve> {
    let (cyl, plane) = match (surf0, surf1) {
        (Surface::Cylinder { .. }, Surface::Plane { .. }) => (surf0, surf1),
        (Surface::Plane { .. }, Surface::Cylinder { .. }) => (surf1, surf0),
        _ => return None,
    };
    let (
        Surface::Cylinder {
            axis_point,
            axis_dir,
            radius,
        },
        Surface::Plane { normal, d },
    ) = (cyl, plane)
    else {
        return None;
    };
    let ax = axis_dir.as_array();
    let axl = (ax[0] * ax[0] + ax[1] * ax[1] + ax[2] * ax[2]).sqrt();
    let n = normal.as_array();
    let nl = (n[0] * n[0] + n[1] * n[1] + n[2] * n[2]).sqrt();
    if axl.is_nan()
        || axl <= 0.0
        || nl.is_nan()
        || nl <= 0.0
        || !radius.is_finite()
        || radius <= 0.0
    {
        return None;
    }
    let au = [ax[0] / axl, ax[1] / axl, ax[2] / axl];
    let nu = [n[0] / nl, n[1] / nl, n[2] / nl];
    let cos = au[0] * nu[0] + au[1] * nu[1] + au[2] * nu[2];
    // Perpendicular-plane ONLY: |cos| must be 1 to within TAU_WORK. An oblique
    // plane cuts an ellipse, which this increment does not handle — skip it
    // rather than approximate it with a circle.
    if (cos.abs() - 1.0).abs() > TAU_WORK {
        return None;
    }
    // Slide the axis point along the axis onto the plane: n·(P + h·a) + d/nl = 0.
    let ap = axis_point.as_array();
    let dn = d / nl;
    let h = -((nu[0] * ap[0] + nu[1] * ap[1] + nu[2] * ap[2]) + dn) / cos;
    Some(Curve::Circle {
        center: Point3::new(ap[0] + h * au[0], ap[1] + h * au[1], ap[2] + h * au[2]),
        normal: cad_primitives::Vector3::new(au[0], au[1], au[2]),
        radius,
    })
}

/// Collect the operand-own rim curves from the Stage-4 incidence map: exactly
/// two entries, SAME `InputId` (an operand's own adjacency, not an A×B
/// intersection) and DIFFERENT surfaces (equal surfaces are patch-interior).
pub(crate) fn collect_rim_curves(
    incidence: &std::collections::BTreeMap<(u32, u32), Vec<(InputId, Surface)>>,
) -> std::collections::BTreeMap<(u32, u32), Curve> {
    let mut out = std::collections::BTreeMap::new();
    for (&key, entries) in incidence {
        if entries.len() != 2 {
            continue;
        }
        let (i0, s0) = entries[0];
        let (i1, s1) = entries[1];
        if i0 != i1 || s0 == s1 {
            continue;
        }
        if let Some(c) = rim_circle_from_pair(s0, s1) {
            out.insert(key, c);
        }
    }
    out
}

/// Apply planned relocations in place. Returns the number of vertices moved.
pub(crate) fn apply_boundary_relocations(mesh: &mut Mesh, moves: &[(u32, Point3)]) -> usize {
    let mut n = 0;
    for &(v, q) in moves {
        if let Some(slot) = mesh.verts.get_mut(v as usize) {
            *slot = q;
            n += 1;
        }
    }
    n
}

// =========================================================================
// CENSUS — how often does inc-2 snap a vertex carrying a surface it never
// consumed? (spec §19 "what a fix must do", the census that must precede it)
// =========================================================================

/// Are two surfaces the SAME plane, up to a sub-`TAU_WORK` offset and either
/// orientation?
///
/// This is the identification F0067's quadruple point needs and inc-3's
/// `let [other] = ...` lacks: at a FLUSH junction the other operand contributes
/// a cap plane 5e-16 from the rim's own cap, which adds no constraint but does
/// add an element. Planes only — a duplicate of any other kind has not been
/// measured, and the census must not invent one.
pub(crate) fn planes_are_duplicates(a: Surface, b: Surface) -> bool {
    let (Surface::Plane { normal: na, d: da }, Surface::Plane { normal: nb, d: db }) = (a, b)
    else {
        return false;
    };
    let (va, vb) = (na.as_array(), nb.as_array());
    let la = (va[0] * va[0] + va[1] * va[1] + va[2] * va[2]).sqrt();
    let lb = (vb[0] * vb[0] + vb[1] * vb[1] + vb[2] * vb[2]).sqrt();
    // NaN-safe, the module idiom: a NaN length must report "not duplicates".
    if la.is_nan() || la <= 0.0 || lb.is_nan() || lb <= 0.0 {
        return false;
    }
    let dot = (va[0] * vb[0] + va[1] * vb[1] + va[2] * vb[2]) / (la * lb);
    if (dot.abs() - 1.0).abs() > TAU_WORK {
        return false;
    }
    // Same orientation ⇒ the offsets must match; opposite ⇒ they must negate.
    let (oa, ob) = (da / la, db / lb);
    let delta = if dot > 0.0 { oa - ob } else { oa + ob };
    delta.abs() <= TAU_WORK
}

/// Signed distance of `x` to `s`, normalized so a plane reports metres.
fn surface_distance(s: Surface, x: Point3) -> Option<f64> {
    let (f, _) = crate::stage4_relocate::surface_value_and_normal(s, x.as_array())?;
    match s {
        Surface::Plane { normal, .. } => {
            let n = normal.as_array();
            let l = (n[0] * n[0] + n[1] * n[1] + n[2] * n[2]).sqrt();
            if l > 0.0 {
                Some(f / l)
            } else {
                None
            }
        }
        _ => Some(f),
    }
}

/// Corpus census for `YANG_S4_UNCONSUMED_PROBE` — read-only, no mutation, no
/// effect on the production path.
///
/// For each vertex inc-2 is about to snap, report the surfaces incident to that
/// vertex that the pass did NOT consume: the rim projection constrains the
/// vertex to the rim's own two surfaces only, so any THIRD incident surface is a
/// constraint the snap has no knowledge of. Each is classified:
///
/// - `dup` — a coplanar duplicate of one of the rim's own surfaces (F0067's
///   flush-junction cap). It carries no constraint; it only breaks a
///   uniqueness guard downstream.
/// - `live` — a genuine further constraint. Reported with the vertex's distance
///   to it BEFORE and AFTER the snap, because "carries a live surface" and "the
///   snap moved off it" are different questions and only the second is a defect.
///
/// Called before `apply_boundary_relocations`, so `mesh` holds pre-snap
/// positions and `moves` holds the post-snap ones.
pub(crate) fn census_unconsumed_surfaces(
    mesh: &Mesh,
    moves: &[(u32, Point3)],
    incidence: &std::collections::BTreeMap<(u32, u32), Vec<(InputId, Surface)>>,
    rim_curves: &std::collections::BTreeMap<(u32, u32), Curve>,
) {
    let (mut n_with_other, mut n_dup_only, mut n_live, mut n_dropped) = (0usize, 0usize, 0usize, 0);
    for &(v, q) in moves {
        let Some(&p) = mesh.verts.get(v as usize) else {
            continue;
        };
        // The rim edges that CLAIM this vertex give the pass's own surfaces and
        // the owning operand.
        let mut own: Vec<(InputId, Surface)> = Vec::new();
        for &(s, e) in rim_curves.keys() {
            if s != v && e != v {
                continue;
            }
            if let Some(entries) = incidence.get(&(s, e)) {
                for &(i, sf) in entries {
                    if !own.iter().any(|&(i2, s2)| i2 == i && s2 == sf) {
                        own.push((i, sf));
                    }
                }
            }
        }
        // Every surface incident to the vertex, deduped by VALUE only. Labels
        // are never deduped: two distinct planes of one operand share a label,
        // and at a flush junction that multiplicity is the whole point.
        let mut all: Vec<(InputId, Surface)> = Vec::new();
        for (&(s, e), entries) in incidence {
            if s != v && e != v {
                continue;
            }
            for &(i, sf) in entries {
                if !all.iter().any(|&(i2, s2)| i2 == i && s2 == sf) {
                    all.push((i, sf));
                }
            }
        }
        let mut dups = 0usize;
        let mut live: Vec<(InputId, Surface, f64, f64)> = Vec::new();
        for &(i, sf) in &all {
            if own.iter().any(|&(i2, s2)| i2 == i && s2 == sf) {
                continue;
            }
            if own.iter().any(|&(_, s2)| planes_are_duplicates(sf, s2)) {
                dups += 1;
                continue;
            }
            let (Some(dp), Some(dq)) = (surface_distance(sf, p), surface_distance(sf, q)) else {
                continue;
            };
            live.push((i, sf, dp, dq));
        }
        if dups == 0 && live.is_empty() {
            continue;
        }
        n_with_other += 1;
        if live.is_empty() {
            n_dup_only += 1;
        } else {
            n_live += 1;
        }
        // A DROP is the F0067 defect proper: the vertex was on the unconsumed
        // surface before the snap and is materially off it after. `TAU_MODEL` is
        // the reporting threshold, not an acceptance band — nothing here decides
        // anything.
        let dropped: Vec<_> = live
            .iter()
            .filter(|(_, _, dp, dq)| {
                dp.abs() <= cad_primitives::TAU_MODEL && dq.abs() > cad_primitives::TAU_MODEL
            })
            .collect();
        if !dropped.is_empty() {
            n_dropped += 1;
        }
        let owners: Vec<String> = own
            .iter()
            .map(|(i, s)| format!("{i:?}:{}", crate::stage4_correct::surface_kind_name(*s)))
            .collect();
        eprintln!(
            "[s4-unconsumed] v={v} own={owners:?} dup={dups} live={} dropped={}",
            live.len(),
            dropped.len()
        );
        for (i, sf, dp, dq) in &live {
            eprintln!(
                "[s4-unconsumed]   live {i:?}:{} pre={dp:.6e} post={dq:.6e}",
                crate::stage4_correct::surface_kind_name(*sf)
            );
        }
    }
    eprintln!(
        "[s4-unconsumed] SUMMARY moved={} with_other={n_with_other} dup_only={n_dup_only} \
         live={n_live} dropped={n_dropped}",
        moves.len()
    );
}

// =========================================================================
// inc-3 — the Fig-11 point q as a TRIPLE POINT (spec §11)
// =========================================================================

/// Closed-form `Circle ∩ Plane`, returning the root NEAREST `current`.
///
/// With `C = (c, n̂, r)`, an orthonormal in-plane basis `(û, v̂)` and the plane
/// `m̂·x + d = 0`, a circle point is `c + r(cosθ·û + sinθ·v̂)` and the plane
/// equation becomes `A·cosθ + B·sinθ = −K` for `A = r(m̂·û)`, `B = r(m̂·v̂)`,
/// `K = m̂·c + d`. Two roots at `atan2(B, A) ± acos(−K/‖(A,B)‖)`.
///
/// `None` when the rim does not reach the plane (`|K| > ‖(A,B)‖`) or the two
/// planes are parallel (`‖(A,B)‖ = 0`: either no intersection or the whole
/// circle lies on the plane — ambiguous, so the pass makes no claim).
pub(crate) fn circle_plane_nearest_root(
    circle: &Curve,
    plane_normal: cad_primitives::Vector3,
    plane_d: f64,
    current: Point3,
) -> Option<Point3> {
    let Curve::Circle {
        center,
        normal,
        radius,
    } = *circle
    else {
        return None;
    };
    let m = plane_normal.as_array();
    let ml = (m[0] * m[0] + m[1] * m[1] + m[2] * m[2]).sqrt();
    if ml.is_nan() || ml <= 0.0 || !radius.is_finite() || radius <= 0.0 {
        return None;
    }
    let mu = [m[0] / ml, m[1] / ml, m[2] / ml];
    let dn = plane_d / ml;
    let (ub, vb) = crate::stage1_tessellate::ortho_basis(normal);
    let (u, v) = (ub.as_array(), vb.as_array());
    let c = center.as_array();
    let a = radius * (mu[0] * u[0] + mu[1] * u[1] + mu[2] * u[2]);
    let b = radius * (mu[0] * v[0] + mu[1] * v[1] + mu[2] * v[2]);
    let k = mu[0] * c[0] + mu[1] * c[1] + mu[2] * c[2] + dn;
    let rr = (a * a + b * b).sqrt();
    if rr.is_nan() || rr <= 0.0 {
        return None; // circle plane parallel to the cutting plane
    }
    let ratio = -k / rr;
    if !(-1.0..=1.0).contains(&ratio) {
        return None; // the rim never reaches the plane
    }
    let phi = b.atan2(a);
    let da = ratio.acos();
    let pt = |theta: f64| {
        let (ct, st) = (theta.cos(), theta.sin());
        Point3::new(
            c[0] + radius * (ct * u[0] + st * v[0]),
            c[1] + radius * (ct * u[1] + st * v[1]),
            c[2] + radius * (ct * u[2] + st * v[2]),
        )
    };
    let d2 = |p: Point3| {
        let (x, y) = (p.as_array(), current.as_array());
        (x[0] - y[0]).powi(2) + (x[1] - y[1]).powi(2) + (x[2] - y[2]).powi(2)
    };
    let (q0, q1) = (pt(phi + da), pt(phi - da));
    Some(if d2(q0) <= d2(q1) { q0 } else { q1 })
}

// ---------------------------------------------------------------------------
// §4.5.1 inc-2c-3b-12 — CREASE circles and the relocation domain certificate
// ---------------------------------------------------------------------------

/// The CREASE circle where two surfaces of ONE operand meet — the analytic
/// boundary curve `C_b` of Yang §4.5.1
/// (`refs/text/yang2025_hybrid_boolean.txt:672-690`), generalized past
/// [`rim_circle_from_pair`]'s Cylinder×Plane case.
///
/// Only configurations whose intersection is exactly a CIRCLE are answered;
/// everything else returns `None` ("no certificate available here"), never an
/// approximation. Coaxiality is required for the quadric pairs — two
/// non-coaxial quadrics meet in a quartic, not a circle.
///
/// * `Cylinder × Plane` — delegated to [`rim_circle_from_pair`].
/// * `Cone × Plane` — plane ⊥ the axis: the circle at the plane's own station.
/// * `Cone × Cone` — coaxial, distinct half-angles: `h·tanα₀ = (h+δ)·tanα₁`
///   with `δ` the apex offset along the axis.
/// * `Cylinder × Cone` — coaxial: the station where the cone's radius equals
///   the cylinder's.
pub(crate) fn crease_circle_from_pair(surf0: Surface, surf1: Surface) -> Option<Curve> {
    if let Some(c) = rim_circle_from_pair(surf0, surf1) {
        return Some(c);
    }
    let unit = |v: [f64; 3]| -> Option<[f64; 3]> {
        let l = (v[0] * v[0] + v[1] * v[1] + v[2] * v[2]).sqrt();
        (l.is_finite() && l > 0.0).then(|| [v[0] / l, v[1] / l, v[2] / l])
    };
    let dot = |a: [f64; 3], b: [f64; 3]| a[0] * b[0] + a[1] * b[1] + a[2] * b[2];
    // Perpendicular component of `v` against the unit axis — the coaxiality
    // and perpendicularity witness.
    let perp = |v: [f64; 3], a: [f64; 3]| -> f64 {
        let h = dot(v, a);
        let r = [v[0] - h * a[0], v[1] - h * a[1], v[2] - h * a[2]];
        (r[0] * r[0] + r[1] * r[1] + r[2] * r[2]).sqrt()
    };
    // Scale-relative axis-alignment witness: a direction is "along the axis"
    // when its perpendicular part is below the same evaluation-precision band
    // the junction certificates use (unit vectors ⇒ L = 2).
    let dir_eps = TAU_WORK.max(16.0 * f64::EPSILON);
    let circle = |center: [f64; 3], axis: [f64; 3], radius: f64| -> Option<Curve> {
        (radius.is_finite() && radius > 0.0).then_some(Curve::Circle {
            center: Point3::new(center[0], center[1], center[2]),
            normal: cad_primitives::Vector3::new(axis[0], axis[1], axis[2]),
            radius,
        })
    };
    match (surf0, surf1) {
        (Surface::Cone { .. }, Surface::Plane { .. })
        | (Surface::Plane { .. }, Surface::Cone { .. }) => {
            let (cone, plane) = match surf0 {
                Surface::Cone { .. } => (surf0, surf1),
                _ => (surf1, surf0),
            };
            let (
                Surface::Cone {
                    apex,
                    axis_dir,
                    half_angle,
                },
                Surface::Plane { normal, d },
            ) = (cone, plane)
            else {
                return None;
            };
            let a = unit(axis_dir.as_array())?;
            let n = unit(normal.as_array())?;
            // Plane ⊥ axis ONLY: an oblique plane cuts a conic, not a circle.
            if perp(n, a) > dir_eps {
                return None;
            }
            let tan_a = half_angle.tan();
            if !tan_a.is_finite() || tan_a <= 0.0 {
                return None;
            }
            // Station of the plane along the axis, measured from the apex.
            let nl = (normal.as_array()[0].powi(2)
                + normal.as_array()[1].powi(2)
                + normal.as_array()[2].powi(2))
            .sqrt();
            let ap = apex.as_array();
            let cos = dot(n, a);
            if cos == 0.0 {
                return None;
            }
            let h = -((dot(n, ap)) + d / nl) / cos;
            circle(
                [ap[0] + h * a[0], ap[1] + h * a[1], ap[2] + h * a[2]],
                a,
                h * tan_a,
            )
        }
        (
            Surface::Cone {
                apex: a0,
                axis_dir: d0,
                half_angle: g0,
            },
            Surface::Cone {
                apex: a1,
                axis_dir: d1,
                half_angle: g1,
            },
        ) => {
            let a = unit(d0.as_array())?;
            let b = unit(d1.as_array())?;
            // Coaxial: parallel axes (either orientation) through one line.
            if perp(b, a) > dir_eps {
                return None;
            }
            let (p0, p1) = (a0.as_array(), a1.as_array());
            let off = [p0[0] - p1[0], p0[1] - p1[1], p0[2] - p1[2]];
            if perp(off, a) > dir_eps * (1.0 + off[0].abs().max(off[1].abs()).max(off[2].abs())) {
                return None;
            }
            let (t0, t1) = (g0.tan(), g1.tan());
            if !t0.is_finite() || !t1.is_finite() || t0 <= 0.0 || t1 <= 0.0 {
                return None;
            }
            let denom = t0 - t1;
            if denom == 0.0 {
                return None; // same opening ⇒ nested or identical, no circle
            }
            // h measured from cone 0's apex: h·t0 = (h + δ)·t1, δ = (a0−a1)·â.
            let delta = dot(off, a);
            let h = delta * t1 / denom;
            circle(
                [p0[0] + h * a[0], p0[1] + h * a[1], p0[2] + h * a[2]],
                a,
                h * t0,
            )
        }
        (Surface::Cylinder { .. }, Surface::Cone { .. })
        | (Surface::Cone { .. }, Surface::Cylinder { .. }) => {
            let (cyl, cone) = match surf0 {
                Surface::Cylinder { .. } => (surf0, surf1),
                _ => (surf1, surf0),
            };
            let (
                Surface::Cylinder {
                    axis_point,
                    axis_dir,
                    radius,
                },
                Surface::Cone {
                    apex,
                    axis_dir: cone_dir,
                    half_angle,
                },
            ) = (cyl, cone)
            else {
                return None;
            };
            let a = unit(axis_dir.as_array())?;
            let b = unit(cone_dir.as_array())?;
            if perp(b, a) > dir_eps {
                return None;
            }
            let (cp, ap) = (axis_point.as_array(), apex.as_array());
            let off = [cp[0] - ap[0], cp[1] - ap[1], cp[2] - ap[2]];
            if perp(off, a) > dir_eps * (1.0 + off[0].abs().max(off[1].abs()).max(off[2].abs())) {
                return None;
            }
            let tan_a = half_angle.tan();
            if !tan_a.is_finite() || tan_a <= 0.0 || !radius.is_finite() || radius <= 0.0 {
                return None;
            }
            // Station from the CONE's apex where the cone radius equals r.
            let h = radius / tan_a;
            circle(
                [ap[0] + h * a[0], ap[1] + h * a[1], ap[2] + h * a[2]],
                a,
                radius,
            )
        }
        _ => None,
    }
}

/// Does the planar curve lie IN the plane `s_own` — is `s_own` the face that
/// CONTAINS the crease rather than one its plane cuts transversally? Parallel
/// normals (the crease's plane and the face's), either sense.
fn planar_curve_lies_in(curve: &Curve, s_own: Surface) -> bool {
    let Surface::Plane { normal: n_face, .. } = s_own else {
        return false;
    };
    let Some(n_curve) = planar_curve_normal(curve) else {
        return false;
    };
    let n = n_curve.as_array();
    let nl = (n[0] * n[0] + n[1] * n[1] + n[2] * n[2]).sqrt();
    let m = n_face.as_array();
    let ml = (m[0] * m[0] + m[1] * m[1] + m[2] * m[2]).sqrt();
    if !(nl.is_finite() && nl > 0.0 && ml.is_finite() && ml > 0.0) {
        return false;
    }
    let cx = [
        n[1] * m[2] - n[2] * m[1],
        n[2] * m[0] - n[0] * m[2],
        n[0] * m[1] - n[1] * m[0],
    ];
    (cx[0] * cx[0] + cx[1] * cx[1] + cx[2] * cx[2]).sqrt() / (nl * ml) <= 1e-9
}

/// The normal of a planar crease curve (every `Curve` but a line and a
/// surface pair lies in one plane), and a point of that plane.
fn planar_curve_normal(curve: &Curve) -> Option<cad_primitives::Vector3> {
    match *curve {
        Curve::Circle { normal, .. }
        | Curve::Ellipse { normal, .. }
        | Curve::Hyperbola { normal, .. }
        | Curve::Parabola { normal, .. } => Some(normal),
        Curve::LineSegment | Curve::SurfacePair { .. } => None,
    }
}

fn planar_curve_anchor(curve: &Curve) -> Option<Point3> {
    match *curve {
        Curve::Circle { center, .. }
        | Curve::Ellipse { center, .. }
        | Curve::Hyperbola { center, .. } => Some(center),
        Curve::Parabola { vertex, .. } => Some(vertex),
        Curve::LineSegment | Curve::SurfacePair { .. } => None,
    }
}

/// The divider a crease presents to the face on `s_own` — the surface a step
/// must not cross to leave that face's domain through this crease.
///
/// A crease is the intersection of two surfaces, and within either one the
/// OTHER surface's signed residual changes sign exactly across the crease:
/// that other surface is the divider. For a face whose surface the crease's
/// plane cuts TRANSVERSALLY (a cylinder, cone or torus rim; an oblique
/// section of a cylinder) the crease's plane IS that other surface up to the
/// crease, and [`crease_plane`] gives it. For a PLANAR face that CONTAINS the
/// crease — a cap disc, a plate around a hole, an OBLIQUE cap bounded by an
/// ellipse — that plane is the face's own plane: every point of the face lies
/// in it, its residual is identically zero, and it divides nothing. The
/// divider is then the crease's other surface:
///
/// * a circle's is the cylinder through it about its own axis — the radial
///   divider every surface of revolution carrying that circle shares, so it
///   needs no `s_other` and is exact for a cone or torus rim as well;
/// * any other planar conic's is `s_other` itself — the cylinder or cone whose
///   oblique section it is. A conic alone names TWO such surfaces (mirror
///   images across its plane), so with no `s_other` the divider is declined
///   (`None`): no certificate, rather than a plane that cannot fire.
///
/// Measured on P0040 (2026-10-12, a circle): two triple relocations stepped
/// WITHIN a cylinder's cap plane from inside its rim to 10 units outside it,
/// and both the triple arm's certificate and the §4.5 postcondition read a
/// zero residual at both ends. Generalized to every planar conic 2026-10-13
/// (the audit's "circle-only arm" finding): the rule is the crease's other
/// surface, not the curve type the first customer happened to carry.
pub(crate) fn crease_divider_for(
    curve: &Curve,
    s_own: Surface,
    s_other: Option<Surface>,
) -> Option<Surface> {
    if planar_curve_lies_in(curve, s_own) {
        return match *curve {
            Curve::Circle {
                center,
                normal,
                radius,
            } => {
                let n = normal.as_array();
                let nl = (n[0] * n[0] + n[1] * n[1] + n[2] * n[2]).sqrt();
                (radius.is_finite() && radius > 0.0).then(|| Surface::Cylinder {
                    axis_point: center,
                    axis_dir: cad_primitives::Vector3::new(n[0] / nl, n[1] / nl, n[2] / nl),
                    radius,
                })
            }
            Curve::Ellipse { .. } | Curve::Hyperbola { .. } | Curve::Parabola { .. } => {
                s_other.filter(|s| !matches!(s, Surface::Plane { .. }))
            }
            Curve::LineSegment | Curve::SurfacePair { .. } => None,
        };
    }
    crease_plane(curve)
}

/// The plane of a planar crease curve (circle, ellipse, parabola, hyperbola),
/// as a `Surface::Plane` — the object the domain certificate takes signed
/// distances against on a face that plane cuts transversally. `None` for a
/// line, a surface pair or a degenerate normal.
pub(crate) fn crease_plane(curve: &Curve) -> Option<Surface> {
    let normal = planar_curve_normal(curve)?;
    let anchor = planar_curve_anchor(curve)?;
    let n = normal.as_array();
    let nl = (n[0] * n[0] + n[1] * n[1] + n[2] * n[2]).sqrt();
    if !nl.is_finite() || nl <= 0.0 {
        return None;
    }
    let nu = [n[0] / nl, n[1] / nl, n[2] / nl];
    let c = anchor.as_array();
    Some(Surface::Plane {
        normal: cad_primitives::Vector3::new(nu[0], nu[1], nu[2]),
        d: -(nu[0] * c[0] + nu[1] * c[1] + nu[2] * c[2]),
    })
}

/// §4.5.1's stated trigger, as a CERTIFICATE: did the relocation step
/// `p → q` leave the domain of the face it started on by CROSSING one of the
/// operand's own creases?
///
/// The paper describes the defect in exactly these words — *"a full step
/// length that takes the point to a position `p1` **outside the surface `S2`**
/// where the point is initially located"* — and prescribes truncating the step
/// to `C_b`. This answers only the detection half: **which crease was
/// crossed**, or `None` for a step that stays home.
///
/// The predicate is a SIGN comparison, not a distance band: `p` and `q` must
/// lie on strictly opposite sides of the crease plane, each farther from it
/// than its own [`junction_certificate_band`] — the codebase's existing notion
/// of "exactly on this surface". That band is what separates the two
/// populations this certificate must never confuse:
///
/// * A junction legitimately lying ON a patch boundary (every triple point
///   involving a patch's own rim) evaluates to ~1e-13 on BOTH ends — inside
///   the band, so it is "on the crease", never "across" it. Measured on
///   R0044's own cylinder cap: both ends 0 to f64.
/// * A junction solved on the EXTENDED surface past the rim evaluates to a
///   material overrun of opposite sign. Measured on R0044 v47: the seed sits
///   −0.194 from the cone×cone crease and the exact triple solution +0.827,
///   ~5 orders outside the band.
///
/// Being mesh-independent is the point: the mesh chord scale in that
/// neighbourhood is ~18, so no mesh-derived domain test could separate a
/// 0.827 overrun from a legitimate landing. The crease is analytic, so the
/// certificate is too.
pub(crate) fn crease_crossed_by_step(
    p: Point3,
    q: Point3,
    creases: &[(Curve, Surface, Surface)],
) -> Option<(usize, f64, f64)> {
    for (i, &(c, s_own, s_other)) in creases.iter().enumerate() {
        // A vertex ON the crease belongs to both faces meeting there and may
        // glide along it; the sign of its residual is evaluation noise.
        if on_crease(p, s_own, s_other) {
            continue;
        }
        let Some(plane) = crease_divider_for(&c, s_own, Some(s_other)) else {
            continue;
        };
        let (Some(fp), Some(fq)) = (surface_distance(plane, p), surface_distance(plane, q)) else {
            continue;
        };
        // PROPAGATED evaluation-precision band. The crease plane is not an
        // input: it is DERIVED from two surfaces, so its coefficients already
        // carry both parents' rounding, and a residual against it cannot be
        // certified more tightly than that. Its own band alone understates the
        // construction's scale badly — the plane's reference magnitude is
        // `|n·center|`, which omits the crease RADIUS entirely and, for a
        // near-cylindrical cone, omits an apex magnitude four orders larger
        // than the geometry it describes.
        //
        // Measured on R0044: with the plane's own band, five crease-RIDING
        // relocations whose residuals are pure noise (1.6e-11 … 1.4e-10, both
        // ends, sign meaningless) read as crossings. With the parents' bands
        // added, all five fall inside and every MATERIAL crossing still fires
        // with ten orders to spare (smallest overrun 3.1e-1, against bands of
        // order 1e-11 at that coordinate magnitude).
        // This is error propagation through the construction, not a threshold
        // chosen to separate the populations.
        let band = |x: Point3| -> f64 {
            crate::stage4_relocate::junction_certificate_band(x.as_array(), plane)
                + crate::stage4_relocate::junction_certificate_band(x.as_array(), s_own)
                + crate::stage4_relocate::junction_certificate_band(x.as_array(), s_other)
        };
        if fp.abs() > band(p) && fq.abs() > band(q) && (fp < 0.0) != (fq < 0.0) {
            return Some((i, fp, fq));
        }
    }
    None
}

/// The planar crease two surfaces of ONE operand meet in: a circle when
/// [`crease_circle_from_pair`] knows one (rims, coaxial cone stations), else
/// the single conic an OBLIQUE plane cuts out of a cylinder or cone — the
/// ellipse rim of an oblique cap, a cone's parabolic or hyperbolic section —
/// from the ssi-rs plane-section solver (Yang §4.3, the same solver Stage 3
/// uses). A pair whose section is not one planar conic (two generator lines,
/// a torus spiric, a surface pair) has no planar crease here; the B-Rep edge
/// index (`CreaseIndex`) still carries its line edges.
///
/// 2026-10-13: before this the triple arm's certificate knew only circle
/// creases, so a corner stepping WITHIN an oblique cap across its ellipse rim
/// was never in its index at all — the P0040 class one curve type over.
pub(crate) fn crease_curve_from_pair(s0: Surface, s1: Surface) -> Option<Curve> {
    if let Some(c) = crease_circle_from_pair(s0, s1) {
        return Some(c);
    }
    let conic_pair = matches!(
        (s0, s1),
        (
            Surface::Cylinder { .. } | Surface::Cone { .. },
            Surface::Plane { .. }
        ) | (
            Surface::Plane { .. },
            Surface::Cylinder { .. } | Surface::Cone { .. }
        )
    );
    if !conic_pair {
        return None;
    }
    let q0 = crate::stage3_ssi::surface_to_quadric(s0).ok()?;
    let q1 = crate::stage3_ssi::surface_to_quadric(s1).ok()?;
    let mut curves = ssi_rs::intersect(&q0, &q1).ok()?;
    if curves.len() != 1 {
        return None;
    }
    let c = crate::stage3_ssi::ssi_curve_to_curve(curves.pop()?).ok()?;
    matches!(
        c,
        Curve::Ellipse { .. } | Curve::Parabola { .. } | Curve::Hyperbola { .. }
    )
    .then_some(c)
}

/// Planar creases indexed BY SURFACE: for each surface, every crease its own
/// operand carries on it.
///
/// The domain a relocation must not leave is the FACE's, and a face is bounded
/// by the creases of ITS OWN surface — not by creases the moving vertex happens
/// to sit on. (Measured on R0044 v47: the vertex lies 10.5 from the crease its
/// solution overruns, so a vertex-incident sourcing sees nothing.) An incidence
/// edge whose two entries are the SAME operand on DIFFERENT surfaces is that
/// operand's own rim — the [`collect_rim_curves`] rule — and the crease is the
/// analytic circle those two surfaces share; it is registered under BOTH.
pub(crate) fn creases_by_surface(
    incidence: &std::collections::BTreeMap<(u32, u32), Vec<(InputId, Surface)>>,
) -> Vec<(Surface, Vec<(Curve, Surface)>)> {
    let mut out: Vec<(Surface, Vec<(Curve, Surface)>)> = Vec::new();
    let mut push = |key: Surface, c: Curve, other: Surface| {
        let slot = match out.iter_mut().find(|(k, _)| *k == key) {
            Some(s) => s,
            None => {
                out.push((key, Vec::new()));
                out.last_mut().expect("just pushed")
            }
        };
        // Dedup by value: one crease is carried by many edges.
        if !slot.1.iter().any(|(o, _)| *o == c) {
            slot.1.push((c, other));
        }
    };
    for entries in incidence.values() {
        if entries.len() != 2 {
            continue;
        }
        let (i0, s0) = entries[0];
        let (i1, s1) = entries[1];
        if i0 != i1 || s0 == s1 {
            continue;
        }
        if let Some(c) = crease_curve_from_pair(s0, s1) {
            push(s0, c, s1);
            push(s1, c, s0);
        }
    }
    out
}

/// The creases bounding the faces a vertex's own `surfs` live on.
pub(crate) fn creases_for_surfaces(
    by_surface: &[(Surface, Vec<(Curve, Surface)>)],
    surfs: &[Surface],
) -> Vec<(Curve, Surface, Surface)> {
    let mut out: Vec<(Curve, Surface, Surface)> = Vec::new();
    for &s in surfs {
        let Some((_, list)) = by_surface.iter().find(|(k, _)| *k == s) else {
            continue;
        };
        for &(c, other) in list {
            if !out.iter().any(|(o, _, _)| *o == c) {
                out.push((c, s, other));
            }
        }
    }
    out
}

/// [`surface_distance`] for callers outside this module (census diagnostics).
pub(crate) fn surface_distance_pub(s: Surface, x: Point3) -> Option<f64> {
    surface_distance(s, x)
}

/// Does `p` lie ON the crease itself — i.e. on BOTH surfaces that form it,
/// each within its own [`junction_certificate_band`]?
///
/// Such a vertex belongs to the crease and to both faces meeting there; a
/// relocation may legitimately glide it ALONG the crease, and the sign of its
/// residual against the crease plane is then pure evaluation noise. Exempting
/// it is a MEMBERSHIP statement (the codebase's existing exactness certificate),
/// not a distance threshold.
pub(crate) fn on_crease(p: Point3, s0: Surface, s1: Surface) -> bool {
    [s0, s1].iter().all(|&s| {
        surface_distance(s, p).is_some_and(|f| {
            f.abs() <= crate::stage4_relocate::junction_certificate_band(p.as_array(), s)
        })
    })
}

// ---------------------------------------------------------------------------
// Yang §4.5 — the BOUNDARY-POINT domain certificate
// (spec `specs/yang_45_boundary_point_domain_certificate.md`)
// ---------------------------------------------------------------------------
//
// §4.5 collects "the point pairs that cannot converge to a distance of 0
// WITHIN THEIR DOMAINS" (`refs/text/yang2025_hybrid_boolean.txt:656-659`) and
// §4.5.1 excludes from its own remedy "the boundary points that glide along
// the boundary curves" (`:635-657`, Fig. 13) — a mesh vertex that is one
// operand's MODEL EDGE crossing the other operand's face. The relocation arms
// solve such a vertex exactly (a line × torus root, a circle × plane root) on
// the EXTENDED surfaces; the solution can lie past a crease that bounds the
// face it started on, and every downstream stage then reads a junction the
// B-Rep does not have. Measured on P0003 (2026-09-28): the cutter's end-cap
// rim (a `Circle` edge of B) crossing the boss's lateral face relocated onto
// the exact circle × lateral-plane root 9.9e-4 ABOVE the boss's top cap (past
// the lateral face's top edge), and the boss's top edge crossing B's torus
// relocated onto the exact line × torus root 1.27° INSIDE the revolve's open
// wedge (past the torus face's rim circle). Neither arm has a domain
// postcondition; the two phantom junctions ride into Stage 6 as a 1.8 mm
// stray edge and the render tessellator declines the face's loop
// (`torus patch UV-CDT failed`). At `d_ε/2` the case is SUPPORTED_CORRECT —
// §4.5.2 is the paper's remedy for this class, and this certificate is the
// trigger it lacked.
//
// The certificate generalizes [`crease_crossed_by_step`] (the triple-junction
// arm's, §4.5.1 inc-2c-3b-12) in two ways the measured sites demand:
// * creases come from the operand's B-REP EDGES, not from surface pairs
//   reconstructed out of the mesh incidence — a `LineSegment` edge between two
//   planes (the boss's top edge) has no circle to reconstruct, and a torus ×
//   axial plane pair meets in TWO circles of which only one is the rim;
// * a crease has an EXTENT. The divider plane of a line crease extends past
//   the edge's ends and through a non-convex face's interior (the 9-gon),
//   and a rim circle's plane cuts the tube a second time half a turn away;
//   a crossing counts only where the step meets the crease itself.

/// The extent of a crease along its divider plane.
#[derive(Clone, Copy, Debug, PartialEq)]
pub(crate) enum CreaseExtent {
    /// A `LineSegment` edge: the segment `p0 → p1`.
    Segment { p0: Point3, p1: Point3 },
    /// A `Circle` edge: the circle in the divider plane, bounded to the
    /// edge's own arc when the edge is one (`arc`, `None` for a full circle).
    Circle {
        center: Point3,
        radius: f64,
        arc: Option<CreaseArc>,
    },
    /// A conic edge (ellipse / parabola / hyperbola): its plane meets its
    /// surface in that one curve, so the plane is the extent ALONG the curve
    /// — bounded to the edge's own arc (`arc`, `None` for a closed conic).
    Conic { arc: Option<CreaseArc> },
}

/// The arc of a curved B-Rep edge between its endpoints — what bounds a face,
/// as opposed to the whole curve the arc lies on (P0020, 2026-10-18: a 1.6 mm
/// flank-ellipse arc of a gear-trimmed cylinder face read as the whole
/// ellipse, so steps crossing the flank plane 4 cm away fired as leaving the
/// face). The parameter is [`crate::stage4_correct::conic_param`], the frame
/// Stage 1 builds the arc in: a `Circle` / `Ellipse` arc runs counter-clockwise
/// about the curve's normal from `p0` to `p1`
/// (`stage1_tessellate.rs`, "Minor-arc chain" / "ARC chain"), a `Hyperbola`
/// arc is the parameter interval between them.
#[derive(Clone, Copy, Debug, PartialEq)]
pub(crate) struct CreaseArc {
    pub curve: Curve,
    pub p0: Point3,
    pub p1: Point3,
}

impl CreaseArc {
    /// Does `x` (a point on the arc's curve, to the caller's tolerance) lie on
    /// the arc, or within `tol` of one of its ends? A parameter that cannot be
    /// evaluated answers `true` — the whole curve, the certificate's loud side.
    pub fn contains(&self, x: Point3, tol: f64) -> bool {
        let near = |q: Point3| {
            let (a, b) = (x.as_array(), q.as_array());
            ((a[0] - b[0]).powi(2) + (a[1] - b[1]).powi(2) + (a[2] - b[2]).powi(2)).sqrt() <= tol
        };
        if near(self.p0) || near(self.p1) {
            return true;
        }
        let param = |q: Point3| crate::stage4_correct::conic_param(&self.curve, q);
        let (Some(t0), Some(t1), Some(t)) = (param(self.p0), param(self.p1), param(x)) else {
            return true;
        };
        if crate::stage4_correct::conic_param_periodic(&self.curve) {
            let two_pi = 2.0 * std::f64::consts::PI;
            (t - t0).rem_euclid(two_pi) <= (t1 - t0).rem_euclid(two_pi)
        } else {
            (t0.min(t1)..=t0.max(t1)).contains(&t)
        }
    }
}

/// The arc of a curved edge: `None` for a closed edge (`start == end`) — the
/// whole curve bounds the face — and for an endpoint the vertex table does not
/// hold (the whole curve again: the loud side, never a dropped crease).
fn crease_arc(e: &crate::BRepEdge, verts: &[crate::BRepVertex]) -> Option<CreaseArc> {
    if e.start == e.end {
        return None;
    }
    Some(CreaseArc {
        curve: e.curve,
        p0: verts.get(e.start as usize)?.point,
        p1: verts.get(e.end as usize)?.point,
    })
}

/// One crease of one FACE: the plane a step must not cross, the face's own
/// surface, the neighbour across the crease when the topology names one, and
/// the crease's extent.
#[derive(Clone, Copy, Debug, PartialEq)]
pub(crate) struct FaceCrease {
    /// The transverse plane through the crease — for a line edge, the plane
    /// spanned by the edge and the face normal; for a planar curve, its plane.
    pub divider: Surface,
    pub s_own: Surface,
    pub s_other: Option<Surface>,
    /// The neighbour's face index on the same operand, when found — the
    /// face §4.5.1's continuation lands on (S1).
    pub face_other: Option<u32>,
    pub extent: CreaseExtent,
    /// The B-Rep edge index on its operand (diagnostics).
    pub edge: u32,
}

/// Every face's creases for one operand, indexed by face.
#[derive(Clone, Debug, Default)]
pub(crate) struct CreaseIndex {
    pub faces: Vec<Vec<FaceCrease>>,
}

impl CreaseIndex {
    /// Build from the operand's B-Rep: each loop edge of each face is a crease
    /// of that face unless the surface across it is the same surface (a seam
    /// or a split of one surface is not a boundary of its domain). The
    /// neighbour is found by the edge index when two faces reference the same
    /// edge, else by the edge's unordered vertex pair (the m1 convention emits
    /// one directed copy per half-edge). `SurfacePair` edges have no plane and
    /// are declined — not approximated.
    pub fn build(brep: &crate::BRep) -> Self {
        let edges = brep.edges();
        let verts = brep.vertices();
        let faces = brep.faces();
        // edge index -> faces referencing it; vertex pair -> (face, edge) list.
        let mut by_edge: Vec<Vec<u32>> = vec![Vec::new(); edges.len()];
        let mut by_pair: std::collections::HashMap<(u32, u32), Vec<(u32, u32)>> =
            std::collections::HashMap::new();
        for (fi, f) in faces.iter().enumerate() {
            for &ei in f.outer_loop.iter().chain(f.inner_loops.iter().flatten()) {
                let Some(e) = edges.get(ei as usize) else {
                    continue;
                };
                by_edge[ei as usize].push(fi as u32);
                let key = (e.start.min(e.end), e.start.max(e.end));
                by_pair.entry(key).or_default().push((fi as u32, ei));
            }
        }
        let mut out: Vec<Vec<FaceCrease>> = Vec::with_capacity(faces.len());
        for (fi, f) in faces.iter().enumerate() {
            let s_own = f.surface;
            let mut list: Vec<FaceCrease> = Vec::new();
            for &ei in f.outer_loop.iter().chain(f.inner_loops.iter().flatten()) {
                let Some(e) = edges.get(ei as usize) else {
                    continue;
                };
                // The neighbour across this edge: same edge index on another
                // face, else another face's edge on the same vertex pair with
                // the same curve.
                let key = (e.start.min(e.end), e.start.max(e.end));
                let mut other: Option<u32> = by_edge[ei as usize]
                    .iter()
                    .find(|&&g| g != fi as u32)
                    .copied();
                if other.is_none() {
                    other = by_pair
                        .get(&key)
                        .into_iter()
                        .flatten()
                        .find(|&&(g, ej)| {
                            g != fi as u32
                                && edges
                                    .get(ej as usize)
                                    .is_some_and(|x| curves_coincide(&x.curve, &e.curve))
                        })
                        .map(|&(g, _)| g);
                }
                let face_other = other.filter(|&g| faces.get(g as usize).is_some());
                let s_other: Option<Surface> = face_other
                    .and_then(|g| faces.get(g as usize))
                    .map(|g| g.surface);
                if s_other == Some(s_own) {
                    continue; // a seam / split of ONE surface bounds no domain
                }
                let Some((divider, extent)) = crease_divider(e, verts, s_own, s_other) else {
                    continue;
                };
                list.push(FaceCrease {
                    divider,
                    s_own,
                    s_other,
                    face_other,
                    extent,
                    edge: ei,
                });
            }
            out.push(list);
        }
        Self { faces: out }
    }
}

/// Do two B-Rep edges carry the SAME curve? Exact equality, or — the m1
/// convention emits one directed copy per half-edge, and each copy carries
/// the curve's normal in ITS face's sense — the same planar curve with its
/// directions flipped. Measured 2026-10-13 on a tilted cylinder ∩ box: the
/// cap's ellipse copy reads `normal (0, 0, 1)`, the lateral's `(−0, −0, −1)`,
/// so an exact test paired no neighbour across any planar crease of a real
/// boolean output and every `s_other` was `None`.
fn curves_coincide(a: &Curve, b: &Curve) -> bool {
    if a == b {
        return true;
    }
    let Some(scale) = planar_curve_scale(a) else {
        return false;
    };
    let Some(plane) = crease_plane(a) else {
        return false;
    };
    let Some(anchor) = planar_curve_anchor(a) else {
        return false;
    };
    let tol = crate::stage4_relocate::junction_certificate_band(anchor.as_array(), plane)
        .max(1e-12 * scale);
    same_planar_curve(a, b, tol)
}

/// The divider plane and extent of one B-Rep edge on the face whose surface is
/// `s_own`. `None` for a degenerate edge, a curve without a plane, or a face
/// normal the edge direction does not span a plane with.
pub(crate) fn crease_divider(
    e: &crate::BRepEdge,
    verts: &[crate::BRepVertex],
    s_own: Surface,
    s_other: Option<Surface>,
) -> Option<(Surface, CreaseExtent)> {
    let plane = |n: [f64; 3], through: [f64; 3]| -> Option<Surface> {
        let l = (n[0] * n[0] + n[1] * n[1] + n[2] * n[2]).sqrt();
        if !l.is_finite() || l <= 0.0 {
            return None;
        }
        let nu = [n[0] / l, n[1] / l, n[2] / l];
        Some(Surface::Plane {
            normal: cad_primitives::Vector3::new(nu[0], nu[1], nu[2]),
            d: -(nu[0] * through[0] + nu[1] * through[1] + nu[2] * through[2]),
        })
    };
    match e.curve {
        Curve::LineSegment => {
            let p0 = verts.get(e.start as usize)?.point;
            let p1 = verts.get(e.end as usize)?.point;
            let (a, b) = (p0.as_array(), p1.as_array());
            let dir = [b[0] - a[0], b[1] - a[1], b[2] - a[2]];
            let mid = [
                (a[0] + b[0]) / 2.0,
                (a[1] + b[1]) / 2.0,
                (a[2] + b[2]) / 2.0,
            ];
            let (_, n_own) = crate::stage4_relocate::surface_distance_and_normal(s_own, mid)?;
            // The plane spanned by the edge and the face normal: transverse
            // to the face along the whole edge.
            let n = [
                dir[1] * n_own[2] - dir[2] * n_own[1],
                dir[2] * n_own[0] - dir[0] * n_own[2],
                dir[0] * n_own[1] - dir[1] * n_own[0],
            ];
            let len = (dir[0] * dir[0] + dir[1] * dir[1] + dir[2] * dir[2]).sqrt();
            let nl = (n[0] * n[0] + n[1] * n[1] + n[2] * n[2]).sqrt();
            // Degenerate: a zero-length edge, or an edge along the normal.
            if len.is_nan() || len <= 0.0 || nl.is_nan() || nl <= 1e-9 * len {
                return None;
            }
            Some((plane(n, a)?, CreaseExtent::Segment { p0, p1 }))
        }
        Curve::Circle { center, radius, .. } => {
            if !(radius.is_finite() && radius > 0.0) {
                return None;
            }
            // A planar face containing the circle: the radial divider
            // (`crease_divider_for`); every other face: the circle's plane.
            let divider = crease_divider_for(&e.curve, s_own, s_other)?;
            Some((
                divider,
                CreaseExtent::Circle {
                    center,
                    radius,
                    arc: crease_arc(e, verts),
                },
            ))
        }
        // A planar face containing the conic: the crease's OTHER surface (the
        // cylinder or cone it is a section of — the conic's plane is the
        // face's own and divides nothing); every other face: the conic's
        // plane. One conic per plane section, so the plane is the extent
        // along the curve, bounded to the edge's arc.
        Curve::Ellipse { .. } | Curve::Hyperbola { .. } | Curve::Parabola { .. } => Some((
            crease_divider_for(&e.curve, s_own, s_other)?,
            CreaseExtent::Conic {
                arc: crease_arc(e, verts),
            },
        )),
        Curve::SurfacePair { .. } => None,
    }
}

/// A boundary-point step that left its face across a crease.
#[derive(Clone, Copy, Debug, PartialEq)]
pub(crate) struct CreaseFire {
    /// Index into the face's crease list.
    pub crease: usize,
    /// The B-Rep edge of the crossed crease.
    pub edge: u32,
    /// Signed distances of the step's ends to the divider plane.
    pub f_pre: f64,
    pub f_post: f64,
}

/// Did the step `pre → post` leave the face across one of its creases?
///
/// The predicate is the [`crease_crossed_by_step`] SIGN comparison — both ends
/// farther from the divider than the propagated evaluation-precision band, on
/// strictly opposite sides — plus the EXTENT test: the point where the step
/// meets the divider must lie on the crease itself, within the step's own
/// length plus the band (a mesh end sits within one chord sag of the curve it
/// samples, and the step is that sag's correction; nothing farther along the
/// divider than the step's reach was crossed by it). A vertex riding the
/// crease (either end within the band) is a boundary point gliding along its
/// boundary curve — exempt, as in the triple-junction certificate.
pub(crate) fn boundary_crease_crossed(
    pre: Point3,
    post: Point3,
    creases: &[FaceCrease],
) -> Option<CreaseFire> {
    let (a, b) = (pre.as_array(), post.as_array());
    let travel = ((b[0] - a[0]).powi(2) + (b[1] - a[1]).powi(2) + (b[2] - a[2]).powi(2)).sqrt();
    if travel.is_nan() || travel <= 0.0 {
        return None;
    }
    for (i, c) in creases.iter().enumerate() {
        let band = |x: Point3| -> f64 {
            let xa = x.as_array();
            crate::stage4_relocate::junction_certificate_band(xa, c.divider)
                + crate::stage4_relocate::junction_certificate_band(xa, c.s_own)
                + c.s_other.map_or(0.0, |s| {
                    crate::stage4_relocate::junction_certificate_band(xa, s)
                })
        };
        let (Some(fp), Some(fq)) = (
            surface_distance(c.divider, pre),
            surface_distance(c.divider, post),
        ) else {
            continue;
        };
        if !(fp.abs() > band(pre) && fq.abs() > band(post) && (fp < 0.0) != (fq < 0.0)) {
            continue;
        }
        // Where the step meets the divider.
        let t = fp / (fp - fq);
        let x = [
            a[0] + t * (b[0] - a[0]),
            a[1] + t * (b[1] - a[1]),
            a[2] + t * (b[2] - a[2]),
        ];
        let tol = travel + band(Point3::new(x[0], x[1], x[2]));
        let on_extent = match c.extent {
            CreaseExtent::Segment { p0, p1 } => {
                let (p, q) = (p0.as_array(), p1.as_array());
                let d = [q[0] - p[0], q[1] - p[1], q[2] - p[2]];
                let l2 = d[0] * d[0] + d[1] * d[1] + d[2] * d[2];
                if l2.is_nan() || l2 <= 0.0 {
                    false
                } else {
                    let w = [x[0] - p[0], x[1] - p[1], x[2] - p[2]];
                    let s = (w[0] * d[0] + w[1] * d[1] + w[2] * d[2]) / l2;
                    let sc = s.clamp(0.0, 1.0);
                    let foot = [p[0] + sc * d[0], p[1] + sc * d[1], p[2] + sc * d[2]];
                    let dist = ((x[0] - foot[0]).powi(2)
                        + (x[1] - foot[1]).powi(2)
                        + (x[2] - foot[2]).powi(2))
                    .sqrt();
                    dist <= tol
                }
            }
            CreaseExtent::Circle {
                center,
                radius,
                arc,
            } => {
                let cc = center.as_array();
                let w = [x[0] - cc[0], x[1] - cc[1], x[2] - cc[2]];
                (match c.divider {
                    // The circle's plane divides a curved face: the crossing
                    // must sit at the circle's RADIUS within that plane.
                    Surface::Plane { normal, .. } => {
                        let n = normal.as_array();
                        let h = w[0] * n[0] + w[1] * n[1] + w[2] * n[2];
                        let r = [w[0] - h * n[0], w[1] - h * n[1], w[2] - h * n[2]];
                        let rho = (r[0] * r[0] + r[1] * r[1] + r[2] * r[2]).sqrt();
                        (rho - radius).abs() <= tol
                    }
                    // The circle's cylinder divides a planar face: the
                    // crossing must sit at the circle's PLANE on that cylinder.
                    Surface::Cylinder { axis_dir, .. } => {
                        let n = axis_dir.as_array();
                        let h = w[0] * n[0] + w[1] * n[1] + w[2] * n[2];
                        h.abs() <= tol
                    }
                    _ => continue,
                }) && arc.is_none_or(|arc| arc.contains(Point3::new(x[0], x[1], x[2]), tol))
            }
            CreaseExtent::Conic { arc } => {
                arc.is_none_or(|arc| arc.contains(Point3::new(x[0], x[1], x[2]), tol))
            }
        };
        if !on_extent {
            continue;
        }
        return Some(CreaseFire {
            crease: i,
            edge: c.edge,
            f_pre: fp,
            f_post: fq,
        });
    }
    None
}

/// The size of a planar crease curve — the radius a `1e-12` relative floor
/// is read against.
fn planar_curve_scale(c: &Curve) -> Option<f64> {
    let r = match *c {
        Curve::Circle { radius, .. } => radius,
        Curve::Ellipse { major_radius, .. } => major_radius,
        Curve::Hyperbola {
            semi_transverse, ..
        } => semi_transverse,
        Curve::Parabola { focal_length, .. } => focal_length,
        Curve::LineSegment | Curve::SurfacePair { .. } => return None,
    };
    (r.is_finite() && r > 0.0).then_some(r)
}

/// Are two planar crease curves the SAME curve, to `tol`? Same variant, same
/// anchor and radii within `tol`, every direction parallel either way (a
/// curve's normal and axis carry no sense of their own).
fn same_planar_curve(a: &Curve, b: &Curve, tol: f64) -> bool {
    let parallel = |u: cad_primitives::Vector3, v: cad_primitives::Vector3| -> bool {
        let (u, v) = (u.as_array(), v.as_array());
        let ul = (u[0] * u[0] + u[1] * u[1] + u[2] * u[2]).sqrt();
        let vl = (v[0] * v[0] + v[1] * v[1] + v[2] * v[2]).sqrt();
        if !(ul.is_finite() && ul > 0.0 && vl.is_finite() && vl > 0.0) {
            return false;
        }
        let cx = [
            u[1] * v[2] - u[2] * v[1],
            u[2] * v[0] - u[0] * v[2],
            u[0] * v[1] - u[1] * v[0],
        ];
        (cx[0] * cx[0] + cx[1] * cx[1] + cx[2] * cx[2]).sqrt() / (ul * vl) <= 1e-9
    };
    let close = |p: Point3, q: Point3| -> bool {
        let (p, q) = (p.as_array(), q.as_array());
        ((p[0] - q[0]).powi(2) + (p[1] - q[1]).powi(2) + (p[2] - q[2]).powi(2)).sqrt() <= tol
    };
    match (*a, *b) {
        (
            Curve::Circle {
                center: ca,
                normal: na,
                radius: ra,
            },
            Curve::Circle {
                center: cb,
                normal: nb,
                radius: rb,
            },
        ) => close(ca, cb) && (ra - rb).abs() <= tol && parallel(na, nb),
        (
            Curve::Ellipse {
                center: ca,
                normal: na,
                major_axis: ma,
                major_radius: ra,
                minor_radius: sa,
            },
            Curve::Ellipse {
                center: cb,
                normal: nb,
                major_axis: mb,
                major_radius: rb,
                minor_radius: sb,
            },
        ) => {
            close(ca, cb)
                && (ra - rb).abs() <= tol
                && (sa - sb).abs() <= tol
                && parallel(na, nb)
                && parallel(ma, mb)
        }
        (
            Curve::Hyperbola {
                center: ca,
                normal: na,
                major_axis: ma,
                semi_transverse: ra,
                semi_conjugate: sa,
            },
            Curve::Hyperbola {
                center: cb,
                normal: nb,
                major_axis: mb,
                semi_transverse: rb,
                semi_conjugate: sb,
            },
        ) => {
            close(ca, cb)
                && (ra - rb).abs() <= tol
                && (sa - sb).abs() <= tol
                && parallel(na, nb)
                && parallel(ma, mb)
        }
        (
            Curve::Parabola {
                vertex: va,
                normal: na,
                axis_dir: da,
                focal_length: fa,
            },
            Curve::Parabola {
                vertex: vb,
                normal: nb,
                axis_dir: db,
                focal_length: fb,
            },
        ) => close(va, vb) && (fa - fb).abs() <= tol && parallel(na, nb) && parallel(da, db),
        _ => false,
    }
}
