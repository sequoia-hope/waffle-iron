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

/// Gate for the inc-3 triple-point re-seat. Separate from the inc-2 gate so the
/// two classes can be measured independently; default OFF.
pub(crate) fn triple_point_enabled() -> bool {
    matches!(
        std::env::var("YANG_S4_TRIPLE_POINT_ENABLE").as_deref(),
        Ok("1") | Ok("on")
    )
}

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

/// The inc-3 CERTIFICATE (spec §11): `q` is accepted only if it satisfies EVERY
/// surface it is supposed to lie on, to f64 noise scaled by the point's own
/// magnitude. Deliberately NOT a displacement band — the measured displacement
/// of this class is not chord-bounded (F0083: 5.4× its chord's own sagitta), so
/// a band would either refuse the real fix or admit anything.
pub(crate) fn satisfies_all_surfaces(q: Point3, surfaces: &[Surface]) -> bool {
    let qa = q.as_array();
    let scale = qa[0].abs().max(qa[1].abs()).max(qa[2].abs()).max(1.0);
    surfaces.iter().all(|&s| {
        match crate::stage4_relocate::surface_value_and_normal(s, qa) {
            // `surface_value_and_normal` returns the implicit value; for the
            // quadrics in play it is a (squared-)distance-like residual, so the
            // scaled f64-noise floor is the right acceptance.
            Some((f, _)) => f.abs() <= TAU_WORK * scale,
            None => false,
        }
    })
}

/// Plan the inc-3 triple-point re-seats (spec §11).
///
/// Selection — a vertex that is ALL of: an endpoint of a CLAIMED own-rim edge
/// (so its rim circle is known); EXCLUDED from inc-2 as a cross-input endpoint
/// (i.e. it is a Fig-11 q, not the `v6` class); and incident to exactly ONE
/// distinct other-operand surface, that surface being a `Plane`. Anything else
/// is skipped, never approximated.
///
/// Acceptance is the §11 certificate — q must satisfy all three surfaces — not
/// a displacement band, because this class's displacement is provably not
/// chord-bounded.
pub(crate) fn plan_triple_point_reseats(
    mesh: &Mesh,
    incidence: &std::collections::BTreeMap<(u32, u32), Vec<(InputId, Surface)>>,
    rim_curves: &std::collections::BTreeMap<(u32, u32), Curve>,
    cross_curve_endpoints: &std::collections::BTreeSet<u32>,
) -> Vec<(u32, Point3)> {
    let mut moves: std::collections::BTreeMap<u32, Point3> = std::collections::BTreeMap::new();
    for (&(s, e), circle) in rim_curves {
        let Some(rim_entries) = incidence.get(&(s, e)) else {
            continue;
        };
        if rim_entries.len() != 2 {
            continue;
        }
        let owner = rim_entries[0].0;
        let rim_surfs = [rim_entries[0].1, rim_entries[1].1];
        for v in [s, e] {
            // Only the Fig-11 q class: inc-2 already owns the rest.
            if !cross_curve_endpoints.contains(&v) || moves.contains_key(&v) {
                continue;
            }
            let Some(&p) = mesh.verts.get(v as usize) else {
                continue;
            };
            // The OTHER operand's distinct surfaces at this vertex.
            let mut others: Vec<Surface> = Vec::new();
            for (&(s2, e2), entries) in incidence {
                if s2 != v && e2 != v {
                    continue;
                }
                for &(i, sf) in entries {
                    if i != owner && !others.contains(&sf) {
                        others.push(sf);
                    }
                }
            }
            let [other] = others[..] else { continue };
            let Surface::Plane { normal, d } = other else {
                continue;
            };
            let Some(q) = circle_plane_nearest_root(circle, normal, d, p) else {
                continue;
            };
            if !satisfies_all_surfaces(q, &[rim_surfs[0], rim_surfs[1], other]) {
                continue;
            }
            let (pa, qa) = (p.as_array(), q.as_array());
            let disp =
                ((qa[0] - pa[0]).powi(2) + (qa[1] - pa[1]).powi(2) + (qa[2] - pa[2]).powi(2))
                    .sqrt();
            if disp > TAU_WORK {
                moves.insert(v, q);
            }
        }
    }
    moves.into_iter().collect()
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

/// The plane of a crease `Curve::Circle`, as a `Surface::Plane` — the object
/// the domain certificate takes signed distances against. `None` for a
/// non-circle or a degenerate normal.
pub(crate) fn crease_plane(curve: &Curve) -> Option<Surface> {
    let Curve::Circle { center, normal, .. } = *curve else {
        return None;
    };
    let n = normal.as_array();
    let nl = (n[0] * n[0] + n[1] * n[1] + n[2] * n[2]).sqrt();
    if !nl.is_finite() || nl <= 0.0 {
        return None;
    }
    let nu = [n[0] / nl, n[1] / nl, n[2] / nl];
    let c = center.as_array();
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
        let Some(plane) = crease_plane(&c) else {
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

/// Crease circles indexed BY SURFACE: for each surface, every crease its own
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
        if let Some(c) = crease_circle_from_pair(s0, s1) {
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
// §4.5.1 inc-2c-3b-12b — the REPAIR: truncate → transit → q-points
// ---------------------------------------------------------------------------

/// Why a crossed-crease site has no DETERMINED repair. Every variant leaves the
/// standing behaviour untouched (P9/P10: no partial move, no guess); the site
/// keeps whatever loud stop it has today.
#[derive(Clone, Copy, Debug, PartialEq)]
pub(crate) enum CreaseTransitFailure {
    /// The vertex's incidence is not the measured anatomy: `s_own` is not one
    /// of its three surfaces, or the neighbour is already among them (that is
    /// the `on_crease` population, exempted upstream).
    AnatomyMismatch,
    /// The crease is not a circle, or the step does not actually cross its
    /// plane transversally (a caller that did not come from
    /// [`crease_crossed_by_step`]).
    NoTruncation,
    /// The truncation point falls on the crease's axis, where the nearest
    /// circle point is not unique.
    TruncationDegenerate,
    /// The transited Newton did not converge, or converged to a point that is
    /// not on all three of its surfaces.
    TransitDiverged,
    /// The transited junction leaves the NEIGHBOUR's domain in turn — the
    /// defect would only move one face over. A multi-crease transit is a
    /// different (unmeasured) class, declined rather than iterated. Carries
    /// the second crossing's own residuals so a census reports the overrun
    /// it declined on rather than merely naming it.
    TransitLeavesNeighbour { d_pre: f64, d_post: f64 },
    /// One of the two other surfaces does not meet `C_b` (no real root), or is
    /// a surface `circle_surface_roots` has no quadric form for (torus).
    NoQPoint { which: usize },
}

/// The determined §4.5.1 repair of one out-of-domain triple relocation.
///
/// Field names follow the paper (§4.5.1,
/// `refs/text/yang2025_hybrid_boolean.txt:672-690`): the step is truncated to
/// `p` on the boundary curve `C_b`, re-optimized using the parameterization of
/// the neighbouring surface `S1`, and the intersection points `q1`/`q2` are
/// then solved ON `C_b`.
#[derive(Clone, Copy, Debug)]
pub(crate) struct CreaseTransit {
    /// The paper's `p`: the step truncated to `C_b`.
    pub(crate) p_trunc: Point3,
    /// The paper's `S1`: the neighbouring surface across `C_b`.
    pub(crate) s_nbr: Surface,
    /// The corrected junction, re-solved on `S1` — the position the relocation
    /// should have produced.
    pub(crate) j: Point3,
    /// `q1` on `C_b`: where the first other surface crosses the crease.
    pub(crate) q1: Point3,
    /// `q2` on `C_b`: where the second other surface crosses the crease.
    pub(crate) q2: Point3,
    /// Distance from the discarded out-of-domain solution to the corrected
    /// junction — the size of the correction.
    pub(crate) correction: f64,
    /// For each q-point, the gap between the chosen root and the next-nearest
    /// root on `C_b`. A small margin means the root selection was not
    /// unambiguous, and the census must show it rather than hide it.
    pub(crate) q_margin: [f64; 2],
    /// The site's two OTHER surfaces, in the same order as `q1`/`q2`: `q[i]`
    /// is where `others[i]` meets the crease. Carried rather than re-derived,
    /// so a consumer can match a chain to its q-point by surface IDENTITY
    /// instead of by proximity.
    pub(crate) others: [Surface; 2],
}

/// Solve the §4.5.1 repair for a step `p → q_full` that
/// [`crease_crossed_by_step`] has already certified as leaving its domain.
///
/// `surfs` are the vertex's own three surfaces (the triple the relocation
/// solved); `crease` is the crossed entry `(C_b, s_own, s_nbr)` from that
/// certificate; `nbr_creases` are the creases bounding `s_nbr`, used for the
/// postcondition.
///
/// In the paper's order:
///
/// 1. **Truncate** the step to `C_b` — the segment meets the crease plane at
///    an affine parameter, and the nearest point of the crease circle to that
///    crossing is the paper's `p`.
/// 2. **Transit**: re-solve the triple with `s_own` replaced by the
///    neighbouring surface `S1`, seeded at `p`. This is "the optimization step
///    of `p` is computed using the parameterization of `S1`".
/// 3. **Certify**: the result must satisfy all three of its own surfaces, and
///    must not itself leave `S1`'s domain (else the defect merely moved).
/// 4. **q-points**: solve each other surface against `C_b` exactly
///    ([`crate::stage4_transit::circle_surface_roots`], all roots), taking the
///    root nearest the corrected junction and recording the selection margin.
///
/// Pure and deterministic; no mesh mutation, no topology side effects. Every
/// non-answer is typed.
pub(crate) fn solve_crease_transit(
    p: Point3,
    q_full: Point3,
    surfs: &[Surface],
    crease: &(Curve, Surface, Surface),
    nbr_creases: &[(Curve, Surface, Surface)],
) -> Result<CreaseTransit, CreaseTransitFailure> {
    let (c_b, s_own, s_nbr) = *crease;
    // --- anatomy -----------------------------------------------------------
    if surfs.len() != 3 || !surfs.contains(&s_own) || surfs.contains(&s_nbr) {
        return Err(CreaseTransitFailure::AnatomyMismatch);
    }
    let others: Vec<Surface> = surfs.iter().copied().filter(|&s| s != s_own).collect();
    if others.len() != 2 {
        return Err(CreaseTransitFailure::AnatomyMismatch);
    }

    // --- 1. truncate to C_b ------------------------------------------------
    // The signed distance to a PLANE is affine along a segment, so the
    // crossing parameter is exact in one division — no search.
    let plane = crease_plane(&c_b).ok_or(CreaseTransitFailure::NoTruncation)?;
    let (fp, fq) = (
        surface_distance(plane, p).ok_or(CreaseTransitFailure::NoTruncation)?,
        surface_distance(plane, q_full).ok_or(CreaseTransitFailure::NoTruncation)?,
    );
    let denom = fp - fq;
    if !denom.is_finite() || denom == 0.0 || (fp < 0.0) == (fq < 0.0) {
        return Err(CreaseTransitFailure::NoTruncation);
    }
    let t = fp / denom;
    if !t.is_finite() || !(0.0..=1.0).contains(&t) {
        return Err(CreaseTransitFailure::NoTruncation);
    }
    let (pa, qa) = (p.as_array(), q_full.as_array());
    let x_cross = Point3::new(
        pa[0] + t * (qa[0] - pa[0]),
        pa[1] + t * (qa[1] - pa[1]),
        pa[2] + t * (qa[2] - pa[2]),
    );
    // `project_onto_curve` is the exact closest point on the circle, and
    // returns `None` exactly on the axis — the one place it is not unique.
    let p_trunc =
        project_onto_curve(x_cross, &c_b).ok_or(CreaseTransitFailure::TruncationDegenerate)?;

    // --- 2. transit onto S1 ------------------------------------------------
    let j =
        crate::stage4_relocate::relocate_onto_implicit_triple(p_trunc, others[0], others[1], s_nbr)
            .ok_or(CreaseTransitFailure::TransitDiverged)?;
    // --- 3. certify --------------------------------------------------------
    if !satisfies_all_surfaces(j, &[others[0], others[1], s_nbr]) {
        return Err(CreaseTransitFailure::TransitDiverged);
    }
    // The postcondition that keeps the repair honest: the corrected junction
    // must lie inside the NEIGHBOUR's own domain. Without it a transit could
    // simply carry the overrun one face further along.
    if let Some((_, d_pre, d_post)) = crease_crossed_by_step(p_trunc, j, nbr_creases) {
        return Err(CreaseTransitFailure::TransitLeavesNeighbour { d_pre, d_post });
    }

    // --- 4. the q-points on C_b -------------------------------------------
    let Curve::Circle {
        center,
        normal,
        radius,
    } = c_b
    else {
        return Err(CreaseTransitFailure::NoTruncation);
    };
    let (ub, vb) = crate::stage1_tessellate::ortho_basis(normal);
    let (e1, e2) = (ub.as_array(), vb.as_array());
    let c0 = center.as_array();
    let at = |theta: f64| -> Point3 {
        let (ct, st) = (theta.cos(), theta.sin());
        Point3::new(
            c0[0] + radius * (ct * e1[0] + st * e2[0]),
            c0[1] + radius * (ct * e1[1] + st * e2[1]),
            c0[2] + radius * (ct * e1[2] + st * e2[2]),
        )
    };
    let ja = j.as_array();
    let d2j = |x: Point3| {
        let a = x.as_array();
        (a[0] - ja[0]).powi(2) + (a[1] - ja[1]).powi(2) + (a[2] - ja[2]).powi(2)
    };
    let mut qs: [Point3; 2] = [j, j];
    let mut margins = [f64::INFINITY; 2];
    for (i, &s) in others.iter().enumerate() {
        let roots = crate::stage4_transit::circle_surface_roots(c0, e1, e2, radius, s)
            .ok_or(CreaseTransitFailure::NoQPoint { which: i })?;
        let mut pts: Vec<(f64, Point3)> = roots
            .into_iter()
            .map(|th| {
                let x = at(th);
                (d2j(x), x)
            })
            .collect();
        if pts.is_empty() {
            return Err(CreaseTransitFailure::NoQPoint { which: i });
        }
        // Deterministic: sort by distance to the corrected junction, then by
        // coordinates so equal distances cannot depend on root order.
        pts.sort_by(|a, b| {
            a.0.partial_cmp(&b.0)
                .unwrap_or(std::cmp::Ordering::Equal)
                .then_with(|| {
                    a.1.as_array()
                        .partial_cmp(&b.1.as_array())
                        .unwrap_or(std::cmp::Ordering::Equal)
                })
        });
        qs[i] = pts[0].1;
        margins[i] = match pts.get(1) {
            Some(second) => second.0.sqrt() - pts[0].0.sqrt(),
            None => f64::INFINITY,
        };
    }

    Ok(CreaseTransit {
        p_trunc,
        s_nbr,
        j,
        q1: qs[0],
        q2: qs[1],
        correction: {
            let a = j.as_array();
            ((a[0] - qa[0]).powi(2) + (a[1] - qa[1]).powi(2) + (a[2] - qa[2]).powi(2)).sqrt()
        },
        q_margin: margins,
        others: [others[0], others[1]],
    })
}

// ---------------------------------------------------------------------------
// §4.5.1 inc-2c-3b-12b-1 — the EMISSION-half site anatomy (pure; census-only)
// ---------------------------------------------------------------------------

/// One incident triangle of an out-of-domain site, as the emission half must
/// see it: which input face it descends from, and where its two other corners
/// sit relative to the crossed crease.
#[derive(Clone, Copy, Debug, PartialEq)]
pub(crate) struct FanTri {
    pub(crate) tri: u32,
    /// The triangle's input-face attribution, or `None` when no 2-of-3
    /// majority claimed it (a triangle built entirely from new intersection
    /// vertices).
    pub(crate) face: Option<(InputId, u32)>,
    /// The two corners other than the site vertex.
    pub(crate) other: [u32; 2],
    /// Their signed crease-plane distances, same order.
    pub(crate) d_other: [f64; 2],
}

/// Which side of the crease a one-ring neighbour sits on, judged the same way
/// the trigger judges the step: a MEMBERSHIP test first (a vertex on both
/// forming surfaces within their own bands is ON the crease and belongs to
/// both faces), then the sign.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(crate) enum CreaseSide {
    /// Same side as the site's PRE position — inside the face that owns it.
    Home,
    /// On the crease itself; belongs to both faces.
    On,
    /// Same side as the out-of-domain solution — already across.
    Past,
}

/// The local mesh anatomy of one out-of-domain relocation site.
///
/// The analytic half ([`solve_crease_transit`]) answers WHERE the corner
/// belongs; this answers WHAT the mesh currently has there, which is what the
/// emission half must edit. It is a measurement, not a plan: every field is a
/// count or a distance the census can print, and nothing here mutates.
#[derive(Clone, Debug, PartialEq)]
pub(crate) struct TransitSiteAnatomy {
    /// The site's incident triangles.
    pub(crate) fan: Vec<FanTri>,
    /// One-ring neighbours with their crease-plane distance and side.
    pub(crate) ring: Vec<(u32, f64, CreaseSide)>,
    /// Neighbour counts by side, in `Home`/`On`/`Past` order.
    pub(crate) sides: [usize; 3],
    /// Distinct input faces in the fan, with how many triangles each claims,
    /// in descending count order (ties by face id) — the census reading of
    /// "which face owns this corner today".
    pub(crate) fan_faces: Vec<(Option<(InputId, u32)>, usize)>,
    /// For `q1`/`q2`: the mesh edge lying ON the crease that hosts it.
    /// `None` when the local mesh carries no crease edge at all.
    pub(crate) q_hosts: [Option<QHost>; 2],
}

/// The mesh edge ON the crease that is nearest one q-point — the segment the
/// emission half would have to split, measured rather than assumed.
#[derive(Clone, Copy, Debug, PartialEq)]
pub(crate) struct QHost {
    pub(crate) a: u32,
    pub(crate) b: u32,
    /// Parameter of the closest point along `a → b`, clamped to `[0, 1]`.
    pub(crate) t: f64,
    /// Distance from the q-point to that segment. The q-point is EXACT on the
    /// crease circle, so this is the chord's own sag, not an error in `q`.
    pub(crate) dist: f64,
    /// The segment's length — the local resolution of the crease chain.
    pub(crate) len: f64,
    /// Whether the segment is incident to the site's own fan, i.e. whether the
    /// split the repair needs is inside the region it is editing.
    pub(crate) in_fan: bool,
}

/// Measure the local mesh anatomy of a site `crease_crossed_by_step` fired on.
///
/// `v` is the site vertex, still at its PRE position in `mesh` (the caller
/// probes before committing the relocation); `crease` is the crossed entry;
/// `d_post` is the out-of-domain solution's signed crease-plane distance,
/// whose SIGN defines the `Past` side. `qs` are the repair's two q-points.
///
/// O(triangles) — one scan for the fan and one for the crease edges. Pure.
pub(crate) fn transit_site_anatomy(
    mesh: &Mesh,
    attribution: &crate::brep::TriangleAttributionMap,
    v: u32,
    crease: &(Curve, Surface, Surface),
    d_post: f64,
    qs: [Point3; 2],
) -> Option<TransitSiteAnatomy> {
    let (c_b, s_own, s_nbr) = *crease;
    let plane = crease_plane(&c_b)?;
    let past_is_neg = d_post < 0.0;

    let dist = |u: u32| surface_distance(plane, mesh.verts[u as usize]).unwrap_or(f64::NAN);
    let side = |u: u32, d: f64| -> CreaseSide {
        if on_crease(mesh.verts[u as usize], s_own, s_nbr) {
            CreaseSide::On
        } else if (d < 0.0) == past_is_neg {
            CreaseSide::Past
        } else {
            CreaseSide::Home
        }
    };

    // --- the fan and its one ring ------------------------------------------
    let mut fan: Vec<FanTri> = Vec::new();
    let mut ring_ids: Vec<u32> = Vec::new();
    for (ti, t) in mesh.tris.iter().enumerate() {
        let Some(k) = t.iter().position(|&x| x == v) else {
            continue;
        };
        let other = [t[(k + 1) % 3], t[(k + 2) % 3]];
        fan.push(FanTri {
            tri: ti as u32,
            face: attribution.lookup(ti as u32).map(|a| (a.input, a.face)),
            other,
            d_other: [dist(other[0]), dist(other[1])],
        });
        ring_ids.extend_from_slice(&other);
    }
    if fan.is_empty() {
        return None;
    }
    ring_ids.sort_unstable();
    ring_ids.dedup();

    let mut sides = [0usize; 3];
    let ring: Vec<(u32, f64, CreaseSide)> = ring_ids
        .iter()
        .map(|&u| {
            let d = dist(u);
            let s = side(u, d);
            sides[match s {
                CreaseSide::Home => 0,
                CreaseSide::On => 1,
                CreaseSide::Past => 2,
            }] += 1;
            (u, d, s)
        })
        .collect();

    // --- which face owns the fan today -------------------------------------
    let mut counts: Vec<(Option<(InputId, u32)>, usize)> = Vec::new();
    for f in fan.iter().map(|t| t.face) {
        match counts.iter_mut().find(|(k, _)| *k == f) {
            Some(slot) => slot.1 += 1,
            None => counts.push((f, 1)),
        }
    }
    counts.sort_by(|a, b| b.1.cmp(&a.1).then_with(|| a.0.cmp(&b.0)));

    // --- the crease's own mesh edges, and which hosts each q-point ---------
    // An edge lies ON the crease when BOTH endpoints do (the same membership
    // test the trigger exempts on) — that is the rim chain the repair must
    // split at `q1`/`q2`.
    let fan_verts: std::collections::BTreeSet<u32> = fan
        .iter()
        .flat_map(|t| [v, t.other[0], t.other[1]])
        .collect();
    let mut q_hosts: [Option<QHost>; 2] = [None, None];
    let mut on_c: std::collections::BTreeMap<u32, bool> = std::collections::BTreeMap::new();
    let mut is_on = |u: u32| -> bool {
        *on_c
            .entry(u)
            .or_insert_with(|| on_crease(mesh.verts[u as usize], s_own, s_nbr))
    };
    let mut edges: Vec<(u32, u32)> = Vec::new();
    for t in &mesh.tris {
        for (i, &x) in t.iter().enumerate() {
            let y = t[(i + 1) % 3];
            let (lo, hi) = if x < y { (x, y) } else { (y, x) };
            if is_on(lo) && is_on(hi) {
                edges.push((lo, hi));
            }
        }
    }
    edges.sort_unstable();
    edges.dedup();
    for (qi, q) in qs.iter().enumerate() {
        let mut best: Option<QHost> = None;
        for &(x, y) in &edges {
            let (a, b) = (
                mesh.verts[x as usize].as_array(),
                mesh.verts[y as usize].as_array(),
            );
            let qa = q.as_array();
            let ab = [b[0] - a[0], b[1] - a[1], b[2] - a[2]];
            let len2 = ab[0] * ab[0] + ab[1] * ab[1] + ab[2] * ab[2];
            if len2 <= 0.0 {
                continue;
            }
            let aq = [qa[0] - a[0], qa[1] - a[1], qa[2] - a[2]];
            let t = ((aq[0] * ab[0] + aq[1] * ab[1] + aq[2] * ab[2]) / len2).clamp(0.0, 1.0);
            let d = ((aq[0] - t * ab[0]).powi(2)
                + (aq[1] - t * ab[1]).powi(2)
                + (aq[2] - t * ab[2]).powi(2))
            .sqrt();
            if best.is_none_or(|h| d < h.dist) {
                best = Some(QHost {
                    a: x,
                    b: y,
                    t,
                    dist: d,
                    len: len2.sqrt(),
                    in_fan: fan_verts.contains(&x) && fan_verts.contains(&y),
                });
            }
        }
        q_hosts[qi] = best;
    }

    Some(TransitSiteAnatomy {
        fan,
        ring,
        sides,
        fan_faces: counts,
        q_hosts,
    })
}

// ---------------------------------------------------------------------------
// §4.5.1 inc-2c-3b-12b-2 — the CUT PATH through the fan (pure; census-only)
// ---------------------------------------------------------------------------

/// Why a site's cut through its own fan is not determined. Every variant is a
/// STRUCTURAL statement about the neighbourhood, never a threshold.
#[derive(Clone, Copy, Debug, PartialEq)]
pub(crate) enum CutPathFailure {
    /// A one-ring neighbour is itself already across the crease, so the cut
    /// leaves the fan through an edge NOT incident to the site. Measured on
    /// R0044's v38/v39/v59 cluster: the `Past` neighbours are siblings the
    /// loop has already relocated, so their repair unit is the cluster.
    PastNeighbour { u: u32 },
    /// The incident triangles do not form one closed cycle around the site
    /// (a boundary fan, or a non-manifold vertex).
    FanNotClosed,
    /// No fan triangle carries the surface the crease bounds, or more than one
    /// input face does — the own patch is not identified, so there is no
    /// "across" to cut toward.
    OwnFaceAmbiguous { found: usize },
    /// The own patch's triangles are not contiguous around the site: the fan
    /// enters and leaves it more than once, which is not the measured corner.
    OwnRunSplit,
    /// Other than exactly two chain edges bound the own run, or their two
    /// q-points are not distinct. Two chains terminate at two q-points; one
    /// point for both would be a pinch, not a corner.
    QTermination { found: usize },
    /// A chain edge's other face is not one of the site's two OTHER surfaces,
    /// so which q-point it terminates at is not IDENTIFIED. Declined rather
    /// than resolved by proximity: the mesh edge is a chord, and its
    /// crease-plane crossing is only an approximation of the chain curve's.
    QSurfaceUnmatched { u: u32 },
    /// Other than exactly one CARRIER edge inside the non-own run — the edge
    /// between the two OTHER surfaces, the curve the site glides along.
    CarrierCount { found: usize },
}

/// One crossing of the crease along the cut the repair makes across the site's
/// OWN patch, in the fan's own order.
#[derive(Clone, Copy, Debug, PartialEq)]
pub(crate) enum CutCrossing {
    /// The chain terminates AT a ring vertex the mesh already has on the
    /// crease — this q-point needs no insertion. `dist` is from that vertex to
    /// the repair's own q-point (measured, not assumed to be zero).
    QVertex { u: u32, q: usize, dist: f64 },
    /// The chain edge `site → u` is crossed, and the crossing is the repair's
    /// q-point `q`. `dist` is from the edge's own crease-plane crossing to
    /// that q-point; `margin` how much nearer it is than the other q.
    QPoint {
        u: u32,
        q: usize,
        dist: f64,
        margin: f64,
    },
    /// An INTERIOR ring vertex already on the crease — the cut passes through
    /// it, nothing is split.
    Vertex(u32),
    /// The cut crosses an edge interior to the own patch. Such a crossing has
    /// no analytic name: it is the segment's crease-plane crossing PROJECTED
    /// onto the crease circle, and `lift` is how far that projection moved it
    /// — the refinement of the crease's own mesh chain this crossing implies.
    Refined { u: u32, point: Point3, lift: f64 },
}

/// The cut the crease makes across one site's own patch.
#[derive(Clone, Debug, PartialEq)]
pub(crate) struct TransitCutPath {
    /// The crossings in order, from one q-termination to the other.
    pub(crate) nodes: Vec<CutCrossing>,
    /// Own-patch triangles lying wholly across the crease — they change face
    /// wholesale, no split.
    pub(crate) past_tris: Vec<u32>,
    /// Own-patch triangles the cut splits.
    pub(crate) split_tris: Vec<u32>,
    /// The ring vertex of the CARRIER edge: the chain between the site's two
    /// OTHER surfaces. The correction `X → J` is a step ALONG that curve, so
    /// this edge is neither split nor re-terminated — it is what the site
    /// glides on.
    pub(crate) carrier: u32,
    /// The cut polyline's own length, q-termination to q-termination.
    pub(crate) span: f64,
    /// Each node's ANGLE along the crease circle, in degrees, in cut order.
    ///
    /// This is the decisive reading of whether the mesh can express the corner
    /// at all. The TRUE cut is the crease arc from `q1` to `q2`, so a cut that
    /// can bound a region must sweep MONOTONICALLY between them. A sequence
    /// that runs out past one q-point and back is a polyline that overlaps its
    /// own arc: the mesh's crease chain near the site is coarser than the
    /// corner, and no re-attribution of existing triangles can express it.
    pub(crate) thetas: Vec<f64>,
}

/// Walk the incident triangles of a site into one cyclic order.
///
/// [`FanTri::other`] is stored in the triangle's own winding (`k+1`, `k+2`
/// from the site), so the fan closes by chaining `other[1] → other[0]`.
fn fan_cycle(fan: &[FanTri]) -> Option<Vec<usize>> {
    // In a manifold fan each ring vertex starts exactly one triangle.
    let mut by_start: std::collections::BTreeMap<u32, usize> = std::collections::BTreeMap::new();
    for (i, t) in fan.iter().enumerate() {
        if by_start.insert(t.other[0], i).is_some() {
            return None;
        }
    }
    let mut order = Vec::with_capacity(fan.len());
    let mut seen = vec![false; fan.len()];
    let mut cur = 0usize;
    for _ in 0..fan.len() {
        if seen[cur] {
            return None;
        }
        seen[cur] = true;
        order.push(cur);
        cur = *by_start.get(&fan[cur].other[1])?;
    }
    // Closed exactly when the walk returns to where it started.
    (cur == 0).then_some(order)
}

/// Where the crease cuts across the site's OWN patch — the edit the emission
/// half would make, expressed as crossings rather than as mesh mutations.
///
/// The measured corner anatomy is one shape (§3v): the fan straddles exactly
/// three input faces, so the site carries THREE chains, and they do not play
/// the same role.
///
/// * Two chains involve the own surface (own × other). They cross the crease,
///   and their crossings are precisely the repair's q-points — which is what
///   makes those points the re-termination targets.
/// * The third joins the two OTHER surfaces and does NOT involve the own face
///   at all, so it never meets the crease as a termination. It is the CARRIER:
///   the correction `X → J` is a step along that very curve. (Measured on
///   R0044 v47: the carrier is the cylinder's own end circle, and `X` and `J`
///   are its intersections with cone 627 and cone 626 respectively.)
///
/// So the cut is not a loop around the site but an ARC across the own patch,
/// from one q-termination to the other, and everything between it and the site
/// belongs to the neighbouring face.
///
/// `site_pos` is the site's OUT-OF-DOMAIN position — the exact triple solution
/// the relocation would land on. It is passed rather than read from the mesh
/// on purpose: the caller detects the defect BEFORE committing the relocation,
/// so `mesh` still holds the seed, on the home side of the crease, and every
/// crossing computed from there would be an extrapolation behind the site.
///
/// `face_surface` resolves an attribution `(input, face)` to that input face's
/// surface — the exact identification of which patch is the own one. Pure; no
/// mutation. Every non-answer is a typed structural decline.
pub(crate) fn transit_cut_path(
    mesh: &Mesh,
    an: &TransitSiteAnatomy,
    site_pos: Point3,
    crease: &(Curve, Surface, Surface),
    t: &CreaseTransit,
    face_surface: &dyn Fn(InputId, u32) -> Option<Surface>,
) -> Result<TransitCutPath, CutPathFailure> {
    let qs = [t.q1, t.q2];
    if let Some((u, _, _)) = an.ring.iter().find(|(_, _, s)| *s == CreaseSide::Past) {
        return Err(CutPathFailure::PastNeighbour { u: *u });
    }
    let order = fan_cycle(&an.fan).ok_or(CutPathFailure::FanNotClosed)?;
    let (c_b, s_own, _) = *crease;
    let plane = crease_plane(&c_b).ok_or(CutPathFailure::FanNotClosed)?;

    // --- which fan face IS the own patch ----------------------------------
    let is_own = |f: Option<(InputId, u32)>| -> bool {
        f.and_then(|(i, x)| face_surface(i, x)) == Some(s_own)
    };
    let own_faces: Vec<Option<(InputId, u32)>> = an
        .fan_faces
        .iter()
        .map(|(f, _)| *f)
        .filter(|f| is_own(*f))
        .collect();
    if own_faces.len() != 1 {
        return Err(CutPathFailure::OwnFaceAmbiguous {
            found: own_faces.len(),
        });
    }
    let own = own_faces[0];

    // --- the own patch's run must be ONE contiguous arc of the fan ---------
    let owned: Vec<bool> = order.iter().map(|&i| an.fan[i].face == own).collect();
    let n = order.len();
    let starts: Vec<usize> = (0..n)
        .filter(|&i| owned[i] && !owned[(i + n - 1) % n])
        .collect();
    if starts.len() != 1 || owned.iter().all(|b| *b) {
        return Err(CutPathFailure::OwnRunSplit);
    }
    let run: Vec<usize> = {
        let mut r = Vec::new();
        let mut i = starts[0];
        while owned[i] {
            r.push(order[i]);
            i = (i + 1) % n;
        }
        r
    };

    // --- exactly one CARRIER edge inside the non-own run -------------------
    let carriers: Vec<u32> = (0..n)
        .filter(|&i| !owned[i] && !owned[(i + n - 1) % n])
        .filter(|&i| an.fan[order[i]].face != an.fan[order[(i + n - 1) % n]].face)
        .map(|i| an.fan[order[i]].other[0])
        .collect();
    if carriers.len() != 1 {
        return Err(CutPathFailure::CarrierCount {
            found: carriers.len(),
        });
    }

    // --- walk the run, emitting one crossing per ring vertex ---------------
    let side = |u: u32| an.ring.iter().find(|(x, _, _)| *x == u).map(|(_, _, s)| *s);
    let d_of = |u: u32| surface_distance(plane, mesh.verts[u as usize]);
    // The site's OUT-OF-DOMAIN position, which is what the cut is around. It
    // is NOT `mesh.verts[v]`: the caller detects the defect before committing
    // the relocation, so the mesh still holds the seed, on the HOME side —
    // and a crossing computed from there is an extrapolation BEHIND the site,
    // not a crease crossing at all.
    let d_site = surface_distance(plane, site_pos).ok_or(CutPathFailure::FanNotClosed)?;
    // The run's ring vertices in order: the first triangle's `other[0]`, then
    // every triangle's `other[1]`. The two ends are the chain edges.
    let mut ring_seq: Vec<u32> = vec![an.fan[run[0]].other[0]];
    ring_seq.extend(run.iter().map(|&i| an.fan[i].other[1]));

    // The two chain edges bounding the run, named by the face ACROSS each:
    // the run's first ring edge is shared with the triangle before the run,
    // its last with the triangle after.
    let pos_first = starts[0];
    let pos_last = (pos_first + run.len() - 1) % n;
    let end_other = [
        an.fan[order[(pos_first + n - 1) % n]].face,
        an.fan[order[(pos_last + 1) % n]].face,
    ];
    // Which q-point each chain terminates at, by SURFACE identity: `q[i]` is
    // where `others[i]` meets the crease, so the chain whose other face IS
    // `others[i]` is the one that terminates there. Never by proximity — the
    // mesh edge is a chord and its crease crossing only approximates the
    // chain curve's.
    let q_of_face = |f: Option<(InputId, u32)>, u: u32| -> Result<usize, CutPathFailure> {
        let sf = f
            .and_then(|(i, x)| face_surface(i, x))
            .ok_or(CutPathFailure::QSurfaceUnmatched { u })?;
        match (sf == t.others[0], sf == t.others[1]) {
            (true, false) => Ok(0),
            (false, true) => Ok(1),
            _ => Err(CutPathFailure::QSurfaceUnmatched { u }),
        }
    };

    let mut nodes: Vec<CutCrossing> = Vec::new();
    let mut q_taken: Vec<usize> = Vec::new();
    let last = ring_seq.len() - 1;
    for (k, &u) in ring_seq.iter().enumerate() {
        let is_chain_end = k == 0 || k == last;
        let on = side(u).ok_or(CutPathFailure::FanNotClosed)? == CreaseSide::On;
        match (is_chain_end, on) {
            (true, true) => {
                // The mesh already carries this q-point as a vertex.
                let q = q_of_face(end_other[usize::from(k != 0)], u)?;
                q_taken.push(q);
                nodes.push(CutCrossing::QVertex {
                    u,
                    q,
                    dist: dist3(mesh.verts[u as usize], qs[q]),
                });
            }
            (true, false) => {
                let cross = crease_crossing_on_edge(mesh, site_pos, u, d_site, d_of(u))?;
                let q = q_of_face(end_other[usize::from(k != 0)], u)?;
                q_taken.push(q);
                nodes.push(CutCrossing::QPoint {
                    u,
                    q,
                    dist: dist3(cross, qs[q]),
                    margin: dist3(cross, qs[1 - q]) - dist3(cross, qs[q]),
                });
            }
            (false, true) => nodes.push(CutCrossing::Vertex(u)),
            (false, false) => {
                let cross = crease_crossing_on_edge(mesh, site_pos, u, d_site, d_of(u))?;
                let p = project_onto_curve(cross, &c_b).ok_or(CutPathFailure::FanNotClosed)?;
                nodes.push(CutCrossing::Refined {
                    u,
                    point: p,
                    lift: dist3(cross, p),
                });
            }
        }
    }
    if q_taken.len() != 2 || q_taken[0] == q_taken[1] {
        return Err(CutPathFailure::QTermination {
            found: q_taken.len(),
        });
    }

    // --- which own triangles are split, and which cross wholesale ----------
    let (mut past_tris, mut split_tris) = (Vec::new(), Vec::new());
    for &i in &run {
        let t = an.fan[i];
        if [t.other[0], t.other[1]]
            .iter()
            .all(|&x| side(x) == Some(CreaseSide::On))
        {
            past_tris.push(t.tri);
        } else {
            split_tris.push(t.tri);
        }
    }
    past_tris.sort_unstable();
    split_tris.sort_unstable();
    // The cut's own length, walked through its nodes in order.
    let at = |c: &CutCrossing| -> Point3 {
        match *c {
            CutCrossing::QVertex { u, .. } | CutCrossing::Vertex(u) => mesh.verts[u as usize],
            CutCrossing::QPoint { q, .. } => qs[q],
            CutCrossing::Refined { point, .. } => point,
        }
    };
    let span = nodes
        .windows(2)
        .map(|w| dist3(at(&w[0]), at(&w[1])))
        .sum::<f64>();
    // Each node's angle along the crease circle. Every node lies on it by
    // construction (`On` vertices by membership, q-points and refinements by
    // solve/projection), so the angle is well defined without a further test.
    let theta = crease_theta_deg(&c_b).ok_or(CutPathFailure::FanNotClosed)?;
    let thetas = nodes.iter().map(|nd| theta(at(nd))).collect();
    Ok(TransitCutPath {
        nodes,
        past_tris,
        split_tris,
        carrier: carriers[0],
        span,
        thetas,
    })
}

/// The crease-plane crossing of the segment `site → u`. Signed distance to a
/// plane is affine along a segment, so the parameter is exact in one division.
///
/// The segment starts at the site's OUT-OF-DOMAIN position, not at whatever
/// the mesh currently holds for it — those differ by the whole relocation.
fn crease_crossing_on_edge(
    mesh: &Mesh,
    site_pos: Point3,
    u: u32,
    d_site: f64,
    d_u: Option<f64>,
) -> Result<Point3, CutPathFailure> {
    let du = d_u.ok_or(CutPathFailure::FanNotClosed)?;
    let denom = d_site - du;
    if denom == 0.0 || !denom.is_finite() {
        return Err(CutPathFailure::FanNotClosed);
    }
    let s = d_site / denom;
    let (a, b) = (site_pos.as_array(), mesh.verts[u as usize].as_array());
    Ok(Point3::new(
        a[0] + s * (b[0] - a[0]),
        a[1] + s * (b[1] - a[1]),
        a[2] + s * (b[2] - a[2]),
    ))
}

fn dist3(p: Point3, q: Point3) -> f64 {
    let (a, b) = (p.as_array(), q.as_array());
    ((a[0] - b[0]).powi(2) + (a[1] - b[1]).powi(2) + (a[2] - b[2]).powi(2)).sqrt()
}

/// The angle-along-the-crease reading, as one closure so every consumer shares
/// a single orthonormal basis.
///
/// Two measurements of the same corner are only comparable if they are taken
/// in the same frame; the cut path's node angles and the emission plan's
/// interval arithmetic are compared directly, so they must not each pick their
/// own basis. `None` for a crease that is not a circle.
fn crease_theta_deg(c_b: &Curve) -> Option<impl Fn(Point3) -> f64> {
    let Curve::Circle { center, normal, .. } = *c_b else {
        return None;
    };
    let (ub, vb) = crate::stage1_tessellate::ortho_basis(normal);
    let (e1, e2, c0) = (ub.as_array(), vb.as_array(), center.as_array());
    Some(move |p: Point3| {
        let x = p.as_array();
        let r = [x[0] - c0[0], x[1] - c0[1], x[2] - c0[2]];
        let dot = |a: [f64; 3]| r[0] * a[0] + r[1] * a[1] + r[2] * a[2];
        dot(e2).atan2(dot(e1)).to_degrees()
    })
}

/// Fold an angular difference in degrees into `(-180, 180]`.
fn wrap_deg(d: f64) -> f64 {
    let mut x = d % 360.0;
    if x > 180.0 {
        x -= 360.0;
    } else if x <= -180.0 {
        x += 360.0;
    }
    x
}

// ---------------------------------------------------------------------------
// §4.5.2 inc-2c-3b-12b-3 — the EMISSION PLAN: what the mesh must ACQUIRE
// ---------------------------------------------------------------------------

/// Why a determined cut has no determined emission plan.
#[derive(Clone, Copy, Debug, PartialEq)]
pub(crate) enum EmissionPlanFailure {
    /// A cut node names a ring vertex the anatomy does not carry — the two
    /// measurements were taken of different sites.
    RingMismatch { u: u32 },
    /// The crease is not a circle, so an arc of it has no angle.
    NoCreaseCircle,
    /// The cut does not carry exactly one termination for each q-point.
    QTerminations { found: usize },
    /// The two q-points coincide to within the crease's own evaluation band:
    /// a pinch, not a corner, and nothing to insert between.
    CornerDegenerate { gap: f64 },
}

/// How the mesh must acquire one q-point on the CHAIN that terminates there.
#[derive(Clone, Copy, Debug, PartialEq)]
pub(crate) enum QAcquire {
    /// A ring vertex already IS this q-point: the chain needs no insert.
    AtVertex { u: u32, dist: f64 },
    /// The chain edge `site → u` must be SPLIT and the new vertex placed at
    /// the exact q. `lift` is how far the edge's own crease crossing lies from
    /// that q — the chord deviation the insert removes, not an error in `q`.
    SplitChain { u: u32, lift: f64 },
}

/// How the CREASE's own mesh chain must acquire the same point.
///
/// The two are independent: a q-point is where the chain meets the crease, so
/// it has to become a vertex of BOTH. A site can already carry it on one and
/// not the other.
#[derive(Clone, Copy, Debug, PartialEq)]
pub(crate) enum CreaseAcquire {
    /// A crease edge of the fan already ENDS at this q-point.
    AtEnd { u: u32, dist: f64 },
    /// The q lies inside the fan's crease edge `a → b`, at parameter `t`, and
    /// `off_chord` away from it. That edge is shared with the neighbouring
    /// face, so the split has to be conforming on BOTH sides — the 3b-11
    /// one-sided-insert lesson, one layer down in the working mesh.
    Interior {
        a: u32,
        b: u32,
        t: f64,
        off_chord: f64,
        len: f64,
    },
    /// The fan carries no crease edge at all, so there is no chain here to
    /// split: it has to be CREATED. Measured shape, not a decline.
    NoChain,
}

/// Two crease edges of one fan covering the same arc of it.
///
/// Not a resolution shortfall: two edges of a single chain over the same sweep
/// is an inconsistency the site's out-of-domain position has already put in the
/// mesh, and `deg` is exactly the corner (up to each q-vertex's own offset from
/// the analytic q-point) because each edge runs from one q-point past the
/// other.
#[derive(Clone, Copy, Debug, PartialEq)]
pub(crate) struct ChainOverlap {
    pub(crate) a: (u32, u32),
    pub(crate) b: (u32, u32),
    /// The doubly covered sweep, in degrees.
    pub(crate) deg: f64,
}

/// What one site's mesh must ACQUIRE for the corner to be representable.
///
/// [`transit_cut_path`] measured that the cut the crease makes across the
/// existing fan is NON-MONOTONE at every determined site: the site's one ring
/// reaches crease-chain vertices two to three orders of magnitude further
/// along the crease than the corner itself is wide. So the emission half is
/// not a re-attribution of existing triangles, and the precondition is Yang
/// §4.5.2's local refinement — applied at an ANALYTICALLY determined place
/// (the q-points), not as a density ladder.
///
/// This says what that refinement is, per site, as measurements only: which
/// edges must be split, where along them, how far off-chord, and whether the
/// corner arc is clear of the chain vertices that would otherwise fall inside
/// it. Nothing here mutates.
#[derive(Clone, Debug, PartialEq)]
pub(crate) struct TransitEmissionPlan {
    /// Per q-point, in `q1`/`q2` order: the chain-side insert.
    pub(crate) q_acquire: [QAcquire; 2],
    /// Per q-point: the crease-side insert.
    pub(crate) crease_acquire: [CreaseAcquire; 2],
    /// The fan's own crease edges — consecutive run neighbours BOTH on the
    /// crease — with each one's angular interval in degrees, relative to
    /// `q1`. The chain the refinement acts on, named rather than assumed.
    pub(crate) fan_crease_edges: Vec<(u32, u32, f64, f64)>,
    /// Two fan crease edges covering the same arc, if any.
    pub(crate) chain_overlap: Option<ChainOverlap>,
    /// The corner's own sweep along the crease, `q1` to `q2`, in degrees.
    pub(crate) corner_deg: f64,
    /// The angular footprint of the whole fan on the crease, in degrees — the
    /// span the re-attribution would have claimed. Its ratio to `corner_deg`
    /// is how far the existing mesh over-reaches.
    pub(crate) fan_span_deg: f64,
    /// Whether the corner arc is CLEAR: no other crease vertex of the fan lies
    /// strictly inside it. False would mean the notch swallows an existing
    /// chain vertex, so the corner is not a simple two-point cut.
    pub(crate) corner_clear: bool,
    /// The corner arc's sagitta off its own chord — how much geometry a single
    /// straight edge between the two q-points would lose.
    pub(crate) arc_sag: f64,
}

/// Turn a determined cut into the determined ACQUISITION: the inserts the mesh
/// needs before the corner can be emitted.
///
/// Composed from the two measurements that precede it rather than re-deriving
/// either: the q-terminations come from `cut`'s own nodes (which already
/// resolved chain-to-q by surface IDENTITY, never by proximity), and the
/// on-crease membership from `an`'s ring. Pure; every non-answer is typed.
pub(crate) fn transit_emission_plan(
    mesh: &Mesh,
    an: &TransitSiteAnatomy,
    cut: &TransitCutPath,
    t: &CreaseTransit,
    crease: &(Curve, Surface, Surface),
) -> Result<TransitEmissionPlan, EmissionPlanFailure> {
    let (c_b, s_own, _) = *crease;
    let theta = crease_theta_deg(&c_b).ok_or(EmissionPlanFailure::NoCreaseCircle)?;
    let Curve::Circle { radius, .. } = c_b else {
        return Err(EmissionPlanFailure::NoCreaseCircle);
    };

    // Angles are taken relative to `q1` so the interval arithmetic below never
    // has to straddle the branch cut. Every span at these sites is degrees, so
    // one wrap is enough.
    let th0 = theta(t.q1);
    let rel = |p: Point3| wrap_deg(theta(p) - th0);
    let qs = [t.q1, t.q2];
    let q_theta = [0.0, rel(t.q2)];

    let gap = dist3(t.q1, t.q2);
    let band = crate::stage4_relocate::junction_certificate_band(t.q1.as_array(), s_own);
    if gap <= band {
        return Err(EmissionPlanFailure::CornerDegenerate { gap });
    }

    // --- the run's ring vertices, in cut order, with their on-crease flag ---
    let side = |u: u32| {
        an.ring
            .iter()
            .find(|(x, _, _)| *x == u)
            .map(|(_, _, s)| *s)
            .ok_or(EmissionPlanFailure::RingMismatch { u })
    };
    let node_vert = |c: &CutCrossing| match *c {
        CutCrossing::QVertex { u, .. }
        | CutCrossing::QPoint { u, .. }
        | CutCrossing::Vertex(u)
        | CutCrossing::Refined { u, .. } => u,
    };
    let mut seq: Vec<(u32, bool)> = Vec::with_capacity(cut.nodes.len());
    for nd in &cut.nodes {
        let u = node_vert(nd);
        seq.push((u, side(u)? == CreaseSide::On));
    }

    // --- the chain-side insert, read off the cut's own terminations --------
    let mut q_acquire: [Option<QAcquire>; 2] = [None, None];
    for nd in &cut.nodes {
        let (q, acq) = match *nd {
            CutCrossing::QVertex { u, q, dist } => (q, QAcquire::AtVertex { u, dist }),
            CutCrossing::QPoint { u, q, dist, .. } => (q, QAcquire::SplitChain { u, lift: dist }),
            _ => continue,
        };
        if let Some(slot) = q_acquire.get_mut(q) {
            slot.get_or_insert(acq);
        }
    }
    let found = q_acquire.iter().filter(|x| x.is_some()).count();
    if found != 2 {
        return Err(EmissionPlanFailure::QTerminations { found });
    }
    let q_acquire = [q_acquire[0].unwrap(), q_acquire[1].unwrap()];
    let is_q_vertex = |u: u32| {
        q_acquire
            .iter()
            .any(|a| matches!(*a, QAcquire::AtVertex { u: x, .. } if x == u))
    };

    // --- the fan's own crease edges ---------------------------------------
    // Two consecutive run neighbours BOTH on the crease share an edge that
    // lies on it. That is the chain the refinement has to act on — and where
    // there is none, the site has no local chain at all.
    let mut fan_crease_edges: Vec<(u32, u32, f64, f64)> = Vec::new();
    for w in seq.windows(2) {
        let ((a, on_a), (b, on_b)) = (w[0], w[1]);
        if on_a && on_b {
            let (ta, tb) = (rel(mesh.verts[a as usize]), rel(mesh.verts[b as usize]));
            fan_crease_edges.push((a, b, ta.min(tb), ta.max(tb)));
        }
    }

    // --- do two of them cover the same arc? --------------------------------
    let mut chain_overlap = None;
    'outer: for (i, e) in fan_crease_edges.iter().enumerate() {
        for f in &fan_crease_edges[i + 1..] {
            let ov = e.3.min(f.3) - e.2.max(f.2);
            if ov > 0.0 {
                chain_overlap = Some(ChainOverlap {
                    a: (e.0, e.1),
                    b: (f.0, f.1),
                    deg: ov,
                });
                break 'outer;
            }
        }
    }

    // --- the crease-side insert, per q ------------------------------------
    let crease_acquire = std::array::from_fn(|i| {
        let q = qs[i];
        // Whether a crease edge already ENDS at this q-point is answered by
        // IDENTITY, from the vertex the cut already resolved as the
        // termination — never by re-measuring a distance against a band. The
        // two readings of one point differ in their last digits (R0003 v1983:
        // 1.7e-12 against a ~1.1e-12 contract band), and a band test there
        // disowns a q-point the mesh demonstrably carries. Same rule as
        // 3b-12b-2's surface-identity q matching and §3t's `on_crease`
        // exemption.
        if let QAcquire::AtVertex { u, dist } = q_acquire[i] {
            if fan_crease_edges
                .iter()
                .any(|&(a, b, _, _)| a == u || b == u)
            {
                return CreaseAcquire::AtEnd { u, dist };
            }
        }
        // Otherwise the edge whose angular interval contains it.
        let inside = fan_crease_edges
            .iter()
            .find(|&&(_, _, lo, hi)| q_theta[i] > lo && q_theta[i] < hi);
        match inside {
            Some(&(a, b, _, _)) => {
                let (pa, pb) = (mesh.verts[a as usize], mesh.verts[b as usize]);
                let (u, v, w) = (pa.as_array(), pb.as_array(), q.as_array());
                let d = [v[0] - u[0], v[1] - u[1], v[2] - u[2]];
                let len2 = d[0] * d[0] + d[1] * d[1] + d[2] * d[2];
                let tt = if len2 > 0.0 {
                    ((w[0] - u[0]) * d[0] + (w[1] - u[1]) * d[1] + (w[2] - u[2]) * d[2]) / len2
                } else {
                    0.0
                };
                let foot = Point3::new(u[0] + tt * d[0], u[1] + tt * d[1], u[2] + tt * d[2]);
                CreaseAcquire::Interior {
                    a,
                    b,
                    t: tt,
                    off_chord: dist3(q, foot),
                    len: len2.sqrt(),
                }
            }
            None => CreaseAcquire::NoChain,
        }
    });

    // --- the corner against the fan's footprint ---------------------------
    let (lo_q, hi_q) = (q_theta[0].min(q_theta[1]), q_theta[0].max(q_theta[1]));
    let mut lo = lo_q;
    let mut hi = hi_q;
    let mut corner_clear = true;
    for &(u, on) in &seq {
        if !on {
            continue;
        }
        let a = rel(mesh.verts[u as usize]);
        lo = lo.min(a);
        hi = hi.max(a);
        // A crease vertex strictly inside the corner would be swallowed by
        // the notch. The q-points themselves are excluded by IDENTITY — the
        // vertices the cut already named as terminations — because at these
        // spans a q-vertex's own angle lands marginally inside its own
        // interval, and a band test on the distance disowns it.
        if a > lo_q && a < hi_q && !is_q_vertex(u) {
            corner_clear = false;
        }
    }

    let corner_deg = hi_q - lo_q;
    Ok(TransitEmissionPlan {
        q_acquire,
        crease_acquire,
        fan_crease_edges,
        chain_overlap,
        corner_deg,
        fan_span_deg: hi - lo,
        corner_clear,
        arc_sag: radius * (1.0 - (corner_deg.to_radians() / 2.0).cos()),
    })
}

// ---------------------------------------------------------------------------
// §4.5.2 inc-2c-3b-12b-4 — the EMISSION EDIT LIST: what the mesh must DO
// ---------------------------------------------------------------------------

/// Why a determined emission PLAN yields no determined EDIT LIST.
///
/// Every variant is a STRUCTURAL statement about the site, never a threshold.
/// The 3/1/3 partition §3x measured is exactly what the first three variants
/// make operational: only the `Interior` shape is an insertion, and the other
/// two are different operations that this function refuses to guess at.
#[derive(Clone, Copy, Debug, PartialEq)]
pub(crate) enum EmissionEditFailure {
    /// The crease side is `AtEnd`: the mesh already carries both q-points on
    /// its own crease chain and — measured at all three such sites — covers
    /// the corner TWICE with them. The repair there is a RE-ORDERING of edges
    /// that already exist, not an insertion. A different operation, and not
    /// determined here.
    AlreadyCarried { overlap_deg: Option<f64> },
    /// The fan carries no crease edge at all, so there is no local chain to
    /// refine: it has to be CREATED, which needs the neighbour patch's mesh
    /// as well as this fan's.
    ChainAbsent,
    /// The two q-points refine DIFFERENT crease chords, so the corner is not
    /// one refinement of one segment and the along-chord order below does not
    /// define the refined chain.
    CreaseHostsDiffer { a: (u32, u32), b: (u32, u32) },
    /// A q-point the crease side wants to insert is already a mesh vertex on
    /// the chain side. Then the edit is a re-connection of an existing vertex,
    /// not a mint, and the two sides disagree about what the mesh has.
    ChainAlreadyCarried { q: usize, u: u32 },
    /// A host edge is carried by no triangle, or by more than two: the
    /// conforming re-triangulation is not defined on it.
    HostNotManifold { edge: (u32, u32), incident: usize },
    /// The corner is not clear — an existing crease vertex lies strictly
    /// inside it — so the notch would swallow a vertex and the edit is not a
    /// two-point refinement of one chord.
    CornerNotClear,
    /// The fan's triangles do not share exactly one vertex, so the site the
    /// edits are relative to is not identified.
    SiteAmbiguous { found: usize },
}

/// One vertex the repair must MINT, with every mesh edge it becomes incident
/// to.
///
/// The two edges are the point's two roles: it TERMINATES the chain that runs
/// out of the site, and it REFINES the crease's own chord. Both must gain it,
/// or the mesh T-junctions.
#[derive(Clone, Copy, Debug, PartialEq)]
pub(crate) struct QInsert {
    /// Which q-point this is (`0`/`1`), as the solver numbered them.
    pub(crate) q: usize,
    /// The exact analytic position — on the crease circle and on its own
    /// surface, from [`solve_crease_transit`]. Never a chord crossing.
    pub(crate) at: Point3,
    /// The chain edge it terminates, `(site, u)`.
    pub(crate) chain: (u32, u32),
    /// How far that chord's OWN crease crossing sits from `at` — the deviation
    /// the insert removes, not an error in `at`.
    pub(crate) chain_lift: f64,
    /// The crease chord it refines.
    pub(crate) crease: (u32, u32),
    /// Its parameter along `crease.0 → crease.1`.
    pub(crate) crease_t: f64,
    /// How far `at` sits off that chord — the chain sag the refinement removes.
    pub(crate) crease_off: f64,
}

/// The mesh EDITS one determined site needs.
///
/// §3x said what the mesh must ACQUIRE; this says what has to be touched to
/// give it that, and — the reason the increment exists — how far outside the
/// fan the touching reaches. A crease chord is shared with the patch on the
/// far side of the crease, so refining it is not a fan-local act: the
/// neighbour's triangle must receive the same two vertices or the mesh
/// T-junctions along the very curve the repair is trying to make conformal.
/// That is the 3b-11 one-sided-insert lesson, one layer down in the working
/// mesh rather than in the output tessellation.
///
/// Pure. Nothing here mutates; every field is a triangle id, a vertex id, or a
/// measured distance.
#[derive(Clone, Debug, PartialEq)]
pub(crate) struct TransitEmissionEdits {
    /// The site the edits are relative to, derived from the fan rather than
    /// passed — the fan's triangles share exactly one vertex.
    pub(crate) site: u32,
    /// The two mints, in the order they occur ALONG the crease chord from its
    /// first endpoint. That is the order the refined chain must connect them
    /// in; taking it from the q numbering instead would invert the notch
    /// wherever the solver happened to number them the other way.
    pub(crate) inserts: [QInsert; 2],
    /// The crease chord both q-points refine.
    pub(crate) crease_host: (u32, u32),
    /// Every triangle carrying that chord.
    pub(crate) crease_tris: Vec<u32>,
    /// Every triangle carrying each chain edge, in `inserts` order.
    pub(crate) chain_tris: [Vec<u32>; 2],
    /// Every triangle the repair re-triangulates: the host edges' triangles
    /// together with the own-patch triangles the cut already crosses.
    /// Deduplicated, sorted.
    pub(crate) touched: Vec<u32>,
    /// Of those, the ones NOT incident to the site — the repair's reach
    /// OUTSIDE the fan it was planned in.
    pub(crate) outside_fan: Vec<u32>,
    /// Triangles that change attribution wholesale to the neighbour face,
    /// carried through from the cut.
    pub(crate) relabel: Vec<u32>,
}

/// Turn a determined emission PLAN into the determined mesh EDITS.
///
/// Composed from the measurements that precede it rather than re-deriving any
/// of them: the positions come from `t` (exact, on the crease circle), the
/// per-side acquisition from `plan`, the re-triangulated own-patch triangles
/// and the wholesale relabels from `cut`, and the fan from `an`. The only
/// thing computed here is edge incidence — which triangles carry each host
/// edge — because that is what says how far the edit reaches.
///
/// Pure; every non-answer is a typed structural decline.
pub(crate) fn transit_emission_edits(
    mesh: &Mesh,
    an: &TransitSiteAnatomy,
    cut: &TransitCutPath,
    plan: &TransitEmissionPlan,
    t: &CreaseTransit,
) -> Result<TransitEmissionEdits, EmissionEditFailure> {
    // The corner must be a two-point refinement of one chord; a swallowed
    // vertex is a different edit.
    if !plan.corner_clear {
        return Err(EmissionEditFailure::CornerNotClear);
    }

    // --- the site, derived from the fan ------------------------------------
    // Every fan triangle contains it, so it is the intersection of their
    // vertex sets. Derived rather than passed: the caller's `v` and the
    // anatomy could disagree, and §3w is what that costs.
    let site = {
        let mut common: Vec<u32> = mesh.tris[an.fan[0].tri as usize].to_vec();
        for ft in &an.fan[1..] {
            let tri = mesh.tris[ft.tri as usize];
            common.retain(|x| tri.contains(x));
        }
        match common[..] {
            [s] => s,
            _ => {
                return Err(EmissionEditFailure::SiteAmbiguous {
                    found: common.len(),
                })
            }
        }
    };

    // --- both sides must be the INSERTION shape ----------------------------
    let mut crease: [(u32, u32, f64, f64); 2] = [(0, 0, 0.0, 0.0); 2];
    for (slot, acq) in crease.iter_mut().zip(plan.crease_acquire.iter()) {
        match *acq {
            CreaseAcquire::AtEnd { .. } => {
                return Err(EmissionEditFailure::AlreadyCarried {
                    overlap_deg: plan.chain_overlap.map(|o| o.deg),
                })
            }
            CreaseAcquire::NoChain => return Err(EmissionEditFailure::ChainAbsent),
            CreaseAcquire::Interior {
                a,
                b,
                t: tt,
                off_chord,
                ..
            } => *slot = (a, b, tt, off_chord),
        }
    }
    // One chord, or the along-chord order below is meaningless.
    let (ha, hb) = (crease[0].0, crease[0].1);
    if (crease[1].0, crease[1].1) != (ha, hb) && (crease[1].1, crease[1].0) != (ha, hb) {
        return Err(EmissionEditFailure::CreaseHostsDiffer {
            a: (crease[0].0, crease[0].1),
            b: (crease[1].0, crease[1].1),
        });
    }
    // `crease[1]`'s parameter is along its OWN orientation; re-read it against
    // the host's if the plan named the chord the other way round.
    let t1 = if (crease[1].0, crease[1].1) == (ha, hb) {
        crease[1].2
    } else {
        1.0 - crease[1].2
    };

    let mut chain: [(u32, f64); 2] = [(0, 0.0); 2];
    for (q, (slot, acq)) in chain.iter_mut().zip(plan.q_acquire.iter()).enumerate() {
        match *acq {
            QAcquire::SplitChain { u, lift } => *slot = (u, lift),
            QAcquire::AtVertex { u, .. } => {
                return Err(EmissionEditFailure::ChainAlreadyCarried { q, u })
            }
        }
    }

    // --- edge incidence, one scan for all three host edges ------------------
    let carries = |tri: [u32; 3], a: u32, b: u32| tri.contains(&a) && tri.contains(&b);
    let mut crease_tris: Vec<u32> = Vec::new();
    let mut chain_tris: [Vec<u32>; 2] = [Vec::new(), Vec::new()];
    for (ti, tri) in mesh.tris.iter().enumerate() {
        if carries(*tri, ha, hb) {
            crease_tris.push(ti as u32);
        }
        for (slot, c) in chain_tris.iter_mut().zip(chain.iter()) {
            if carries(*tri, site, c.0) {
                slot.push(ti as u32);
            }
        }
    }
    for (edge, tris) in [
        ((ha, hb), &crease_tris),
        ((site, chain[0].0), &chain_tris[0]),
        ((site, chain[1].0), &chain_tris[1]),
    ] {
        if tris.is_empty() || tris.len() > 2 {
            return Err(EmissionEditFailure::HostNotManifold {
                edge,
                incident: tris.len(),
            });
        }
    }

    // --- the mints, ordered ALONG the chord --------------------------------
    let qs = [t.q1, t.q2];
    let mk = |i: usize, tt: f64| QInsert {
        q: i,
        at: qs[i],
        chain: (site, chain[i].0),
        chain_lift: chain[i].1,
        crease: (ha, hb),
        crease_t: tt,
        crease_off: crease[i].3,
    };
    let (i0, i1) = (mk(0, crease[0].2), mk(1, t1));
    let inserts = if i0.crease_t <= i1.crease_t {
        [i0, i1]
    } else {
        [i1, i0]
    };
    let chain_tris = if i0.crease_t <= i1.crease_t {
        chain_tris
    } else {
        let [a, b] = chain_tris;
        [b, a]
    };

    // --- what gets re-triangulated, and how far out it reaches --------------
    let mut touched: Vec<u32> = crease_tris.clone();
    touched.extend_from_slice(&chain_tris[0]);
    touched.extend_from_slice(&chain_tris[1]);
    touched.extend_from_slice(&cut.split_tris);
    touched.sort_unstable();
    touched.dedup();
    let in_fan: std::collections::BTreeSet<u32> = an.fan.iter().map(|f| f.tri).collect();
    let outside_fan: Vec<u32> = touched
        .iter()
        .copied()
        .filter(|x| !in_fan.contains(x))
        .collect();

    Ok(TransitEmissionEdits {
        site,
        inserts,
        crease_host: (ha, hb),
        crease_tris,
        chain_tris,
        touched,
        outside_fan,
        relabel: cut.past_tris.clone(),
    })
}

// ---------------------------------------------------------------------------
// §4.5.2 inc-2c-3b-12b-5 — the EMISSION REGION: where the edits can be APPLIED
// ---------------------------------------------------------------------------

/// Why a determined edit list yields no determined REGION.
///
/// Structural, never a threshold: each variant says the neighbourhood the
/// edits name is not a patch a re-triangulation is defined on.
#[derive(Clone, Debug, PartialEq)]
pub(crate) enum EmissionRegionFailure {
    /// The region's boundary edges do not chain into exactly one cycle, so it
    /// is not a topological disk and there is no polygon to re-triangulate.
    NotADisk {
        boundary_edges: usize,
        cycles: usize,
    },
    /// A boundary vertex has more than one outgoing boundary edge: the region
    /// pinches there, and the "polygon" would visit the vertex twice.
    BoundaryPinched { v: u32 },
    /// A triangle carries a host edge in two different roles at once. Then its
    /// children are not defined by a single split and the corpus does not
    /// currently exhibit it, so it is refused rather than guessed at.
    TriangleInBothRoles { tri: u32 },
}

/// Two children of the INDEPENDENT per-edge split that share a vertex set.
///
/// A mint has two host edges — the chain it terminates and the crease chord it
/// refines — and when those hosts are carried by triangles that already share
/// an edge, splitting each host on its own emits the same triangle twice, in
/// opposite windings. That is a zero-area fin, and the edge between the mint
/// and the shared vertex ends up carried by three faces.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(crate) struct SplitCoincidence {
    /// The vertex set both parents produce.
    pub(crate) verts: [u32; 3],
    /// The two parents that produce it.
    pub(crate) parents: (u32, u32),
}

/// An edge that the independent per-edge split would leave carried by more
/// than two triangles — the non-manifold residue of the same interference.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(crate) struct OverfullEdge {
    pub(crate) edge: (u32, u32),
    pub(crate) incident: usize,
}

/// The neighbourhood the emission edits must be applied INSIDE, and the
/// measured verdict on applying them edge by edge.
///
/// §3y closed the edit list: two mints, one crease chord, four chain-carrier
/// triangles. This asks the question that has to be answered before any of it
/// is written to a mesh — whether those edits compose. They are stated
/// per-edge, and a per-edge split is the natural implementation; this measures
/// what that implementation would actually produce.
///
/// Pure. Nothing here mutates; the mint ids are the ones the mints WOULD get
/// (`mesh.verts.len()` onward, in `inserts` order), used only to name the
/// prospective children.
#[derive(Clone, Debug, PartialEq)]
pub(crate) struct TransitEmissionRegion {
    /// Every triangle carrying a host edge, sorted — the region the mints land
    /// in.
    pub(crate) tris: Vec<u32>,
    /// Its boundary as one ordered vertex cycle, in the region's own winding.
    pub(crate) boundary: Vec<u32>,
    /// Whether the site is ON that boundary. It is a vertex of every fan
    /// triangle, so a region that leaves it interior would have to re-connect
    /// the whole fan rather than a polygon.
    pub(crate) site_on_boundary: bool,
    /// The prospective mint ids, in `inserts` (along-chord) order.
    pub(crate) mints: [u32; 2],
    /// What an independent per-edge split of every host would emit.
    pub(crate) naive_children: Vec<[u32; 3]>,
    /// Children of that split sharing a vertex set — each one a zero-area fin.
    pub(crate) coincident: Vec<SplitCoincidence>,
    /// Edges the same split would leave carried by more than two triangles,
    /// counted against the WHOLE mesh (the region's children plus every
    /// triangle outside it), so the verdict is about the mesh and not about
    /// the region in isolation.
    pub(crate) overfull: Vec<OverfullEdge>,
}

/// Chain a set of directed boundary edges into one cycle.
///
/// Returns the vertex cycle, or the structural reason there is not exactly
/// one. `succ` is built from the directed edges the region's own winding
/// leaves unpaired, so the cycle comes out consistently oriented.
fn boundary_cycle(dirs: &[(u32, u32)]) -> Result<Vec<u32>, EmissionRegionFailure> {
    let mut succ: std::collections::BTreeMap<u32, u32> = std::collections::BTreeMap::new();
    for &(a, b) in dirs {
        if succ.insert(a, b).is_some() {
            return Err(EmissionRegionFailure::BoundaryPinched { v: a });
        }
    }
    let Some((&start, _)) = succ.iter().next() else {
        return Err(EmissionRegionFailure::NotADisk {
            boundary_edges: 0,
            cycles: 0,
        });
    };
    let mut cycle = Vec::with_capacity(succ.len());
    let mut cur = start;
    for _ in 0..succ.len() {
        cycle.push(cur);
        cur = succ[&cur];
        if cur == start {
            break;
        }
    }
    if cycle.len() != succ.len() {
        // More than one cycle: the walk closed early, so the remainder is a
        // separate component.
        return Err(EmissionRegionFailure::NotADisk {
            boundary_edges: dirs.len(),
            cycles: 2,
        });
    }
    Ok(cycle)
}

/// Derive the region the emission edits act in, and measure whether applying
/// them edge by edge is sound.
///
/// Composed from [`TransitEmissionEdits`] rather than re-deriving any of it:
/// the hosts, their carriers and the along-chord order all come from there.
/// What is computed here is the region's boundary — which says whether there
/// is a polygon to re-triangulate at all — and the child set an independent
/// split would emit, which says whether it may be done that way.
///
/// Pure; every non-answer is a typed structural decline.
pub(crate) fn transit_emission_region(
    mesh: &Mesh,
    edits: &TransitEmissionEdits,
) -> Result<TransitEmissionRegion, EmissionRegionFailure> {
    // --- the region: every triangle carrying a host edge -------------------
    let mut tris: Vec<u32> = edits.crease_tris.clone();
    tris.extend_from_slice(&edits.chain_tris[0]);
    tris.extend_from_slice(&edits.chain_tris[1]);
    tris.sort_unstable();
    let dup = tris.windows(2).any(|w| w[0] == w[1]);
    tris.dedup();
    if dup {
        // A triangle in two roles: which host does its split follow?
        let mut seen = std::collections::BTreeSet::new();
        for t in edits
            .crease_tris
            .iter()
            .chain(edits.chain_tris[0].iter())
            .chain(edits.chain_tris[1].iter())
        {
            if !seen.insert(*t) {
                return Err(EmissionRegionFailure::TriangleInBothRoles { tri: *t });
            }
        }
    }

    // --- its boundary, from the directed edges its winding leaves unpaired --
    let member: std::collections::BTreeSet<u32> = tris.iter().copied().collect();
    let mut dirs: std::collections::BTreeSet<(u32, u32)> = std::collections::BTreeSet::new();
    for &t in &tris {
        let tri = mesh.tris[t as usize];
        for k in 0..3 {
            dirs.insert((tri[k], tri[(k + 1) % 3]));
        }
    }
    let unpaired: Vec<(u32, u32)> = dirs
        .iter()
        .copied()
        .filter(|&(a, b)| !dirs.contains(&(b, a)))
        .collect();
    let boundary = boundary_cycle(&unpaired)?;
    let site_on_boundary = boundary.contains(&edits.site);

    // --- what an independent per-edge split would emit ----------------------
    // Mint ids are the ones the mints would actually receive, so the children
    // below are the triangles that would really be written.
    let n = mesh.verts.len() as u32;
    let mints = [n, n + 1];

    // Split one triangle's edge `(a, b)` at `ms`, in the triangle's own
    // winding, so the children inherit its orientation.
    let split = |tri: [u32; 3], a: u32, b: u32, ms: &[u32]| -> Vec<[u32; 3]> {
        let Some(k) = (0..3).find(|&k| {
            (tri[k] == a && tri[(k + 1) % 3] == b) || (tri[k] == b && tri[(k + 1) % 3] == a)
        }) else {
            return vec![tri];
        };
        let (p, q, apex) = (tri[k], tri[(k + 1) % 3], tri[(k + 2) % 3]);
        // Walk the inserts from `p` to `q`: reverse them when the triangle
        // names the edge against the order they were given in.
        let mut chain: Vec<u32> = vec![p];
        if p == a {
            chain.extend_from_slice(ms);
        } else {
            chain.extend(ms.iter().rev().copied());
        }
        chain.push(q);
        chain
            .windows(2)
            .map(|w| [w[0], w[1], apex])
            .collect::<Vec<_>>()
    };

    let (ha, hb) = edits.crease_host;
    let mut naive_children: Vec<[u32; 3]> = Vec::new();
    for &t in &edits.crease_tris {
        naive_children.extend(split(mesh.tris[t as usize], ha, hb, &mints));
    }
    // `chain_tris` is permuted into `inserts` order by §3y, so slot `i`
    // carries slot `i`'s edge and therefore slot `i`'s mint.
    for (i, (slot, ins)) in edits
        .chain_tris
        .iter()
        .zip(edits.inserts.iter())
        .enumerate()
    {
        for &t in slot {
            naive_children.extend(split(
                mesh.tris[t as usize],
                ins.chain.0,
                ins.chain.1,
                &[mints[i]],
            ));
        }
    }

    // --- the two ways that split can be unsound ----------------------------
    let key = |t: [u32; 3]| {
        let mut k = t;
        k.sort_unstable();
        k
    };
    let mut by_key: std::collections::BTreeMap<[u32; 3], Vec<usize>> =
        std::collections::BTreeMap::new();
    for (i, c) in naive_children.iter().enumerate() {
        by_key.entry(key(*c)).or_default().push(i);
    }
    let mut coincident: Vec<SplitCoincidence> = Vec::new();
    for (k, idx) in &by_key {
        if idx.len() > 1 {
            coincident.push(SplitCoincidence {
                verts: *k,
                parents: (idx[0] as u32, idx[1] as u32),
            });
        }
    }

    // Edge incidence over the WHOLE mesh after the split: the children, plus
    // every triangle the region does not contain.
    let mut inc: std::collections::BTreeMap<(u32, u32), usize> = std::collections::BTreeMap::new();
    let mut bump = |t: [u32; 3]| {
        for k in 0..3 {
            let (a, b) = (t[k], t[(k + 1) % 3]);
            *inc.entry((a.min(b), a.max(b))).or_default() += 1;
        }
    };
    for c in &naive_children {
        bump(*c);
    }
    for (i, t) in mesh.tris.iter().enumerate() {
        if !member.contains(&(i as u32)) {
            bump(*t);
        }
    }
    let overfull: Vec<OverfullEdge> = inc
        .into_iter()
        .filter(|&(_, n)| n > 2)
        .map(|(edge, incident)| OverfullEdge { edge, incident })
        .collect();

    Ok(TransitEmissionRegion {
        tris,
        boundary,
        site_on_boundary,
        mints,
        naive_children,
        coincident,
        overfull,
    })
}

// ---------------------------------------------------------------------------
// §4.5.2 inc-2c-3b-12b-6 — the region's FACE PARTITION: the fill's own unit
// ---------------------------------------------------------------------------

/// One face-homogeneous part of the emission region.
///
/// The region spans both operands and several input faces, so it has no single
/// chart and cannot be filled as one polygon. The fill's unit is therefore the
/// part — and a part is only fillable if it is an edge-connected disk in its
/// own face's chart, which is what this measures rather than assumes.
#[derive(Clone, Debug, PartialEq)]
pub(crate) struct RegionPart {
    /// The input face every triangle of the part carries, `None` for triangles
    /// the attribution does not name.
    pub(crate) face: Option<(crate::brep::InputId, u32)>,
    /// The part's triangles, from the region alone.
    pub(crate) tris: Vec<u32>,
    /// Its edge-connected component count. More than one means the region cut
    /// the face into pieces that touch only at the site.
    pub(crate) components: usize,
    /// Same-face triangles incident to the SITE that the region left out —
    /// what re-connecting the part would have to take in.
    pub(crate) closure: Vec<u32>,
    /// The component count after that closure.
    pub(crate) components_closed: usize,
    /// The closed part's boundary as one ordered cycle, or `None` if its
    /// boundary is not one simple cycle. This is the polygon a chart-local
    /// fill would stitch, so the mints that lie on it have to be found here.
    pub(crate) boundary_closed: Option<Vec<u32>>,
}

/// Partition the emission region by input face, and measure whether each part
/// is a unit a chart-local fill is defined on.
///
/// Pure. Composed from the region and the attribution; nothing is re-derived
/// and nothing mutates.
pub(crate) fn transit_emission_parts(
    mesh: &Mesh,
    attribution: &crate::brep::TriangleAttributionMap,
    region: &TransitEmissionRegion,
    site: u32,
) -> Vec<RegionPart> {
    let face_of = |t: u32| attribution.lookup(t).map(|a| (a.input, a.face));

    // Edge-adjacency inside a triangle set: two triangles are adjacent when
    // they share two vertices. Vertex-only contact is NOT adjacency — that is
    // exactly the distinction the site makes here.
    let components = |set: &[u32]| -> usize {
        let mut seen = vec![false; set.len()];
        let mut n = 0;
        for i in 0..set.len() {
            if seen[i] {
                continue;
            }
            n += 1;
            let mut stack = vec![i];
            seen[i] = true;
            while let Some(k) = stack.pop() {
                let a = mesh.tris[set[k] as usize];
                for (j, &t) in set.iter().enumerate() {
                    if seen[j] {
                        continue;
                    }
                    let b = mesh.tris[t as usize];
                    if a.iter().filter(|x| b.contains(x)).count() == 2 {
                        seen[j] = true;
                        stack.push(j);
                    }
                }
            }
        }
        n
    };

    let mut faces: Vec<Option<(crate::brep::InputId, u32)>> =
        region.tris.iter().map(|&t| face_of(t)).collect();
    faces.sort_unstable();
    faces.dedup();

    faces
        .into_iter()
        .map(|face| {
            let tris: Vec<u32> = region
                .tris
                .iter()
                .copied()
                .filter(|&t| face_of(t) == face)
                .collect();
            // Everything of this face at the site that the region left out.
            let mut closure: Vec<u32> = (0..mesh.tris.len() as u32)
                .filter(|&t| {
                    mesh.tris[t as usize].contains(&site)
                        && face_of(t) == face
                        && !tris.contains(&t)
                })
                .collect();
            closure.sort_unstable();
            let mut closed = tris.clone();
            closed.extend_from_slice(&closure);
            closed.sort_unstable();
            let components_closed = components(&closed);
            // The closed part's boundary, by the same rule the region uses.
            let mut dirs: std::collections::BTreeSet<(u32, u32)> =
                std::collections::BTreeSet::new();
            for &t in &closed {
                let tri = mesh.tris[t as usize];
                for k in 0..3 {
                    dirs.insert((tri[k], tri[(k + 1) % 3]));
                }
            }
            let unpaired: Vec<(u32, u32)> = dirs
                .iter()
                .copied()
                .filter(|&(a, b)| !dirs.contains(&(b, a)))
                .collect();
            let boundary_closed = boundary_cycle(&unpaired).ok();
            RegionPart {
                face,
                components: components(&tris),
                tris,
                closure,
                components_closed,
                boundary_closed,
            }
        })
        .collect()
}

/// One part's boundary once the mints that lie on it are inserted, split at
/// the vertices it repeats.
///
/// A mint sits on a HOST EDGE, and the own patch carries all three hosts — the
/// crease chord and both chain edges — so each mint lands on that part's
/// boundary TWICE. The doubled visit is not a pathology: it is the corner. A
/// cycle that repeats a vertex pinches there, and the pinch cuts the own patch
/// into the notch (the piece holding the site) and the pieces that keep their
/// face.
#[derive(Clone, Debug, PartialEq)]
pub(crate) struct BoundaryPinch {
    /// The part's boundary with every mint on it inserted, in the order it
    /// occurs along each edge.
    pub(crate) inserted: Vec<u32>,
    /// The loops it separates into at its repeated vertices. One loop means
    /// the part does not pinch and is a simple polygon fill.
    pub(crate) loops: Vec<Vec<u32>>,
    /// Whether the mints' two repeat intervals CROSS on the cycle.
    ///
    /// Two pinch points decompose a cycle cleanly only when the spans between
    /// their repeats nest or stay disjoint. Interleaved, the loops that come
    /// out are not a corner and its remainders — the site's loop swells to
    /// most of the patch — so the fill has no notch to hand over. R0044 v47 is
    /// NOT interleaved; the fixture reaches the interleaved shape when the
    /// same chord is named from its other end, which is what pins the
    /// distinction as measured rather than assumed.
    pub(crate) interleaved: bool,
    /// Which loop is THE NOTCH: the piece holding the site, on a part that
    /// pinched cleanly. A part yielding one loop has no notch — its site, if
    /// it has one, is just a corner of an ordinary polygon — and neither does
    /// an interleaved one, so this is `None` in both cases rather than
    /// trivially `0`.
    pub(crate) notch: Option<usize>,
}

/// Insert the mints a boundary carries and split it where it repeats.
///
/// Pure. The along-edge order comes from `edits` (§3y sorted `inserts` along
/// the chord), never from the mint numbering.
/// Every mint a boundary step can carry, keyed by the UNDIRECTED mesh edge
/// that hosts it, each with its parameter along the edge measured from the
/// lower-id endpoint.
///
/// Built once per fill from the two q-mints (§3y's hosts: the crease chord
/// at each one's measured parameter, and its chain edge) and from any
/// refinement mints (§3ae), and consulted by every part's boundary
/// insertion — so a mint is inserted once per host edge each part carries,
/// in along-edge order, whichever way the part traverses the edge.
#[derive(Clone, Debug, Default, PartialEq)]
pub(crate) struct HostMints {
    pub(crate) by_edge: std::collections::BTreeMap<(u32, u32), Vec<(f64, u32)>>,
}

impl HostMints {
    fn key(a: u32, b: u32) -> (u32, u32) {
        (a.min(b), a.max(b))
    }

    /// Register mint `id` on edge `(a, b)` at parameter `t` measured from `a`.
    pub(crate) fn add(&mut self, a: u32, b: u32, t: f64, id: u32) {
        let t = if a < b { t } else { 1.0 - t };
        let v = self.by_edge.entry(Self::key(a, b)).or_default();
        v.push((t, id));
        v.sort_by(|x, y| x.0.partial_cmp(&y.0).unwrap_or(std::cmp::Ordering::Equal));
    }

    /// The mints on directed step `a → b`, in the order they occur along it.
    pub(crate) fn on_step(&self, a: u32, b: u32) -> Vec<u32> {
        match self.by_edge.get(&Self::key(a, b)) {
            None => Vec::new(),
            Some(v) if a < b => v.iter().map(|x| x.1).collect(),
            Some(v) => v.iter().rev().map(|x| x.1).collect(),
        }
    }

    /// The q-mints' hosts from an edit list.
    pub(crate) fn from_edits(edits: &TransitEmissionEdits, mints: [u32; 2]) -> Self {
        let mut h = Self::default();
        for (i, ins) in edits.inserts.iter().enumerate() {
            h.add(ins.crease.0, ins.crease.1, ins.crease_t, mints[i]);
            h.add(ins.chain.0, ins.chain.1, 0.5, mints[i]);
        }
        h
    }
}

/// Insert every mint the boundary's steps carry, in step order.
pub(crate) fn insert_mints(boundary: &[u32], hosts: &HostMints) -> Vec<u32> {
    let mut inserted = Vec::with_capacity(boundary.len() + 4);
    for k in 0..boundary.len() {
        let (a, b) = (boundary[k], boundary[(k + 1) % boundary.len()]);
        inserted.push(a);
        inserted.extend(hosts.on_step(a, b));
    }
    inserted
}

/// Split an inserted cycle at its repeated vertices (§3aa Reading 2).
///
/// `mints` are the two q-mints, whose repeat spans decide interleaving;
/// `site` names the notch. Returns `None` when no vertex is visited exactly
/// once (nowhere to start) or a q-mint is visited more than twice.
pub(crate) fn pinch_cycle(
    mut inserted: Vec<u32>,
    mints: [u32; 2],
    site: u32,
) -> Option<BoundaryPinch> {
    // Start the walk at a vertex the cycle visits ONCE, so the first and last
    // loops are not an artifact of where it began.
    let start = (0..inserted.len())
        .find(|&i| inserted.iter().filter(|x| **x == inserted[i]).count() == 1)?;
    inserted.rotate_left(start);

    let mut path: Vec<u32> = Vec::new();
    let mut loops: Vec<Vec<u32>> = Vec::new();
    for &v in &inserted {
        match path.iter().position(|x| *x == v) {
            Some(k) => {
                loops.push(path[k..].to_vec());
                path.truncate(k + 1);
            }
            None => path.push(v),
        }
    }
    if path.len() >= 3 {
        loops.push(path);
    }

    // Do the two mints' repeat spans cross? Nested or disjoint spans cut the
    // cycle into a corner and its remainders; crossed ones do not.
    // A mint has exactly two host edges, so a simple boundary can visit it at
    // most twice; a third visit means the cycle is not the polygon this
    // assumes, and the pinch is not defined on it.
    let span = |m: u32| -> Result<Option<(usize, usize)>, ()> {
        let mut it = inserted.iter().enumerate().filter(|(_, x)| **x == m);
        match (it.next(), it.next(), it.next()) {
            (Some((i, _)), Some((j, _)), None) => Ok(Some((i, j))),
            (_, None, _) => Ok(None),
            _ => Err(()),
        }
    };
    let interleaved = match (span(mints[0]).ok()?, span(mints[1]).ok()?) {
        (Some((i1, j1)), Some((i2, j2))) => {
            (i1 < i2 && i2 < j1 && j1 < j2) || (i2 < i1 && i1 < j2 && j2 < j1)
        }
        _ => false,
    };

    let notch = if loops.len() > 1 && !interleaved {
        loops.iter().position(|l| l.contains(&site))
    } else {
        None
    };
    Some(BoundaryPinch {
        inserted,
        loops,
        interleaved,
        notch,
    })
}

/// Insert the q-mints a boundary carries and split it where it repeats.
///
/// Pure. The along-edge order comes from `edits` (§3y sorted `inserts` along
/// the chord), never from the mint numbering.
pub(crate) fn transit_boundary_pinch(
    edits: &TransitEmissionEdits,
    mints: [u32; 2],
    boundary: &[u32],
    site: u32,
) -> Option<BoundaryPinch> {
    let hosts = HostMints::from_edits(edits, mints);
    pinch_cycle(insert_mints(boundary, &hosts), mints, site)
}

// ---------------------------------------------------------------------------
// §4.5.2 inc-2c-3b-12b-7 — the EMISSION FILL: the mutation as a pure plan
// ---------------------------------------------------------------------------

/// Why the fill could not be planned. Every variant is structural; none is a
/// magnitude band.
#[derive(Clone, Debug, PartialEq)]
pub(crate) enum EmissionFillFailure {
    /// No part carries the crease chord AT the site, so the own patch — the
    /// one part the pinch is defined on — is not identified.
    OwnPartMissing,
    /// A part's closed boundary is not one simple cycle (§3aa's
    /// `boundary_closed` is `None`), so there is no polygon to fill.
    PartNotADisk { face: Option<(InputId, u32)> },
    /// The pinch is not defined on the part's boundary: no vertex it visits
    /// once, a mint visited more than twice, or a loop shorter than a
    /// triangle.
    PinchUndefined { face: Option<(InputId, u32)> },
    /// The own patch's two pinch spans cross, so its loops are not a corner
    /// and its remainders (§3aa Reading 3). No notch to hand over.
    Interleaved,
    /// The own patch did not pinch into a notch at all.
    NoNotch,
    /// A part other than the own patch pinched. Every other part carries a
    /// host edge in one role only, so this is not the measured anatomy.
    UnexpectedPinch { face: Option<(InputId, u32)> },
    /// No crease-chord carrier lies outside the own face, so the corner has
    /// no face to bite into — §3y's empty reach, a chord no neighbour
    /// carries.
    NotchDestinationUnknown,
    /// The crease-chord carriers outside the own face name more than one
    /// face between them.
    NotchDestinationAmbiguous,
    /// The neighbour face has no part in the region, though its carrier is
    /// a region triangle — the partition and the carriers disagree.
    NeighbourPartMissing,
    /// The neighbour part, enlarged by the triangles the corner lands in,
    /// does not bound one simple cycle.
    BiteNotADisk,
    /// The neighbour boundary, mints inserted, does not carry the corner
    /// segment as one step — so there is no step to detour through the
    /// corner.
    CornerStepMissing,
    /// The corner lies within the feature floor of an existing vertex of the
    /// bite polygon: inserting it would mint a duplicate, not a corner.
    CornerCoincident { v: u32, dist: f64 },
    /// A polygon's face is unattributed, has no surface, or its surface has
    /// no local chart.
    NoChart { face: Option<(InputId, u32)> },
    /// The cone apex lies in a polygon's chart footprint.
    ApexInPolygon { face: (InputId, u32) },
    /// The polygon wraps a full period of a periodic chart.
    ThetaUnwrap { face: (InputId, u32) },
    /// The chart CDT refused the polygon.
    Cdt {
        face: (InputId, u32),
        error: cherchi_rs::CdtError,
    },
    /// The fill's triangles do not agree with the loop about the winding:
    /// none of them carries a loop edge, or they carry loop edges both ways.
    OrientationUndefined { face: (InputId, u32) },
}

/// One polygon of the fill, in its own face's chart.
#[derive(Clone, Debug, PartialEq)]
pub(crate) struct FillPolygon {
    /// The face the fill's triangles are attributed to.
    pub(crate) face: (InputId, u32),
    /// The loop, in the part's winding, as mesh ids (mints and, for the
    /// bite, the corner included).
    pub(crate) polygon: Vec<u32>,
    /// Its triangulation, wound with the loop.
    pub(crate) tris: Vec<[u32; 3]>,
    /// Whether this is the neighbour's BITE polygon: the one whose boundary
    /// detours through the corner instead of running along the crease.
    pub(crate) bite: bool,
    /// Per triangle, whether its 3D normal lies ALONG its face surface's
    /// gradient at the centroid (`None` = not evaluable). Filled in by the
    /// lift certificate; the per-face [`LiftSense`] counts are its sums.
    pub(crate) lift: Vec<Option<bool>>,
}

/// One edge whose incidence after the edit is not what a manifold requires.
///
/// `expected` is the count the edit must leave: the edge's own count before
/// the edit when it re-creates an existing edge (2 in a closed mesh, 1 on a
/// fixture's open rim), 2 for an edge it creates, 0 for an edge it consumes
/// without re-creating. Anything else is a hole, a fin or a dangling
/// survivor.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(crate) struct EdgeIncidence {
    pub(crate) edge: (u32, u32),
    pub(crate) before: usize,
    pub(crate) after: usize,
    pub(crate) expected: usize,
}

/// The like-for-like chord bound of one face's fill (Yang §4.1.2 `d(T)`):
/// the removed triangles' certified maximum against the added ones'. Planes
/// certify at exactly 0 both ways; `None` means a triangle could not be
/// certified in the chart.
#[derive(Clone, Copy, Debug, PartialEq)]
pub(crate) struct ChordBudget {
    pub(crate) face: (InputId, u32),
    pub(crate) old_max: Option<f64>,
    pub(crate) new_max: Option<f64>,
}

/// One face's chart→3D lift orientation: how many of its removed (old
/// positions), added (new positions) and SURVIVING triangles have their 3D
/// normal along, and against, the face surface's gradient at their centroid.
///
/// A directed-edge check certifies WINDING; it cannot see a triangle that
/// pairs consistently along every edge yet lifts folded onto the surface
/// (the KV9-F2b lesson: the chart→3D lift must be orientation-faithful). The
/// face's survivors supply the sense the fill must keep — the whole face's
/// testimony, not the fossil's, because the fossil is the very thing the
/// defect may have folded (§3ac: the fixture's own fossil is 2 / 2).
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(crate) struct LiftSense {
    pub(crate) face: (InputId, u32),
    pub(crate) old_along: usize,
    pub(crate) old_against: usize,
    pub(crate) new_along: usize,
    pub(crate) new_against: usize,
    pub(crate) survivors_along: usize,
    pub(crate) survivors_against: usize,
    /// Triangles whose surface normal could not be evaluated at the
    /// centroid (a surface kind without a gradient here, or a degenerate
    /// triangle). Uncertified, not certified.
    pub(crate) uncertified: usize,
}

/// Where the corner LANDS on the neighbour face: the surviving triangles of
/// that face whose chart footprint contains the corrected junction, and the
/// ones the two chain stubs cross on their way from the mints to it.
///
/// §3ac measured the reason this exists: the neighbour's host carrier is a
/// full-width sliver of a 1.194-wide band and the corner sits 0.2457 BEYOND
/// its far edge, inside the next triangle. The region the edits land in is
/// therefore not the host carriers alone on the neighbour side; it is the
/// host carriers plus these.
#[derive(Clone, Debug, PartialEq)]
pub(crate) struct BiteRegion {
    pub(crate) face: (InputId, u32),
    /// Survivors of `face` whose chart footprint contains the corner. Sorted.
    pub(crate) contains_corner: Vec<u32>,
    /// Survivors of `face` a stub crosses, not already above. Sorted.
    pub(crate) crossed: Vec<u32>,
    /// The corner's chart position on the face.
    pub(crate) corner_uv: (f64, f64),
}

/// A refinement mint (Yang §4.5.2): an exact crease-circle vertex to insert
/// on an existing mesh edge `host`, at parameter `t` along it from
/// `host.0`. Every triangle carrying `host` — on both faces across it — is
/// re-triangulated with the mint on its boundary: the 3b-11 one-sided-insert
/// lesson, built in rather than learned again.
#[derive(Clone, Copy, Debug, PartialEq)]
pub(crate) struct ExtraMint {
    pub(crate) at: Point3,
    pub(crate) host: (u32, u32),
    pub(crate) t: f64,
}

/// The emission mutation, planned but not written.
///
/// §3aa closed the plan; §3ac corrected what its notch loop IS. Composed
/// here: the own patch is pinched and its notch loop — the flap of its fan
/// that overran the crease, A's material wedge on the own surface's
/// extension — is DROPPED; the neighbour part is enlarged by the triangles
/// the corner lands in ([`BiteRegion`]) and its boundary, mints inserted,
/// DETOURS through the corner instead of running along the corner segment;
/// every other part is filled as an ordinary polygon. Then the RESULT is
/// certified against the whole mesh before anything is written: every edge
/// the edit touches has the incidence a manifold requires, every fill edge
/// shared with a survivor is traversed the opposite way, no face's fill is
/// coarser than what it replaces, and every fill triangle lifts onto its
/// surface the way the face's survivors do.
///
/// Pure. `mints` carry the ids the mints WOULD get (`region.mints`) and their
/// exact positions; `site_at` is where the site will stand (the transit's
/// corrected junction), which is the position every chart projection here
/// uses for it.
#[derive(Clone, Debug, PartialEq)]
pub(crate) struct TransitEmissionFill {
    pub(crate) site: u32,
    pub(crate) site_at: Point3,
    /// Every vertex the edit mints, with its exact position: the two
    /// q-mints first (the ids `region.mints` named), then the refinement
    /// mints in order. Ids are consecutive from the first.
    pub(crate) mints: Vec<(u32, Point3)>,
    /// Every triangle the edit removes: the region, the parts' closures, the
    /// bite and the carriers of every refinement host. Sorted.
    pub(crate) removed: Vec<u32>,
    /// Symmetric difference between `removed` and §3y's `touched` — the two
    /// were derived by different routes; the bite is what the corner adds
    /// beyond the host carriers.
    pub(crate) touched_delta: Vec<u32>,
    pub(crate) own_face: (InputId, u32),
    /// Who owns the far side of the crease chord — the face the corner bites
    /// into.
    pub(crate) notch_face: (InputId, u32),
    /// Whether that face's surface IS the transit's `s_nbr`: the mesh-derived
    /// destination against the analytic one. `None` if the face has no
    /// surface.
    pub(crate) notch_surface_agrees: Option<bool>,
    /// Where the corner lands on the neighbour face.
    pub(crate) bite: BiteRegion,
    /// The own patch's notch loop, dropped: the site and the two mints.
    pub(crate) dropped: Vec<u32>,
    pub(crate) polygons: Vec<FillPolygon>,
    /// Edges whose incidence after the edit is wrong. Empty is the manifold
    /// certificate.
    pub(crate) edge_defects: Vec<EdgeIncidence>,
    /// Fill edges shared with a SURVIVING triangle, traversed the opposite
    /// way (healthy) and the same way (a fold).
    pub(crate) opposed: usize,
    pub(crate) folded: usize,
    /// Directed edges the fill's own triangles carry more than once — a fold
    /// inside the fill.
    pub(crate) added_folds: usize,
    pub(crate) chord: Vec<ChordBudget>,
    /// Per face, the lift sense of what goes, what comes and what stays.
    pub(crate) lift: Vec<LiftSense>,
    /// Added triangles whose lift sense is against their face's reference —
    /// the survivors' majority, or the fossil's where the face has no
    /// survivor, or the added minority where neither decides. Zero is the
    /// lift certificate.
    pub(crate) lift_flips: usize,
    /// Added or removed triangles the lift could not be evaluated on.
    pub(crate) lift_uncertified: usize,
}

/// A part's boundary as one ordered cycle, by the region's own rule: the
/// directed edges its triangles leave unpaired, chained.
fn part_boundary(mesh: &Mesh, tris: &[u32]) -> Result<Vec<u32>, EmissionRegionFailure> {
    let mut dirs: std::collections::BTreeSet<(u32, u32)> = std::collections::BTreeSet::new();
    for &t in tris {
        let tri = mesh.tris[t as usize];
        for k in 0..3 {
            dirs.insert((tri[k], tri[(k + 1) % 3]));
        }
    }
    let unpaired: Vec<(u32, u32)> = dirs
        .iter()
        .copied()
        .filter(|&(a, b)| !dirs.contains(&(b, a)))
        .collect();
    boundary_cycle(&unpaired)
}

/// Where the corner lands on `face`: the survivors of that face (triangles
/// attributed to it and not in `excluded`) whose chart footprint contains
/// `corner`, and the ones either stub `mint → corner` crosses.
///
/// Periodic charts are handled per triangle: each triangle's corners are
/// chain-unwrapped about its first corner, then the whole triangle is
/// shifted by whole periods to sit nearest the corner's azimuth, so a
/// triangle straddling the seam far from the corner cannot be unwrapped
/// into a false hit. Pure.
pub(crate) fn transit_bite_region(
    mesh: &Mesh,
    attribution: &crate::brep::TriangleAttributionMap,
    excluded: &[u32],
    face: (InputId, u32),
    corner: Point3,
    mints: [Point3; 2],
    face_surface: &dyn Fn(InputId, u32) -> Option<Surface>,
) -> Result<BiteRegion, EmissionFillFailure> {
    use crate::stage4_project::SurfaceChart;
    use EmissionFillFailure as F;
    let surface = face_surface(face.0, face.1).ok_or(F::NoChart { face: Some(face) })?;
    let chart = SurfaceChart::new_local(surface).ok_or(F::NoChart { face: Some(face) })?;
    let planar = matches!(chart, SurfaceChart::Plane { .. });
    let biperiodic = matches!(chart, SurfaceChart::Torus { .. });
    let near = |prev: f64, val: f64| -> f64 {
        let mut d = val - prev;
        while d > std::f64::consts::PI {
            d -= std::f64::consts::TAU;
        }
        while d <= -std::f64::consts::PI {
            d += std::f64::consts::TAU;
        }
        prev + d
    };
    let cj = chart.project(corner);
    let cj = (cj.x(), cj.y());
    let stubs: [(f64, f64); 2] = [0, 1].map(|i| {
        let m = chart.project(mints[i]);
        let x = if planar { m.x() } else { near(cj.0, m.x()) };
        let y = if biperiodic { near(cj.1, m.y()) } else { m.y() };
        (x, y)
    });
    let orient = |a: (f64, f64), b: (f64, f64), p: (f64, f64)| -> f64 {
        (b.0 - a.0) * (p.1 - a.1) - (b.1 - a.1) * (p.0 - a.0)
    };
    let inside = |c: &[(f64, f64); 3], p: (f64, f64)| -> bool {
        let total = orient(c[0], c[1], c[2]);
        if total == 0.0 {
            return false;
        }
        let s = total.signum();
        orient(c[0], c[1], p) * s >= 0.0
            && orient(c[1], c[2], p) * s >= 0.0
            && orient(c[2], c[0], p) * s >= 0.0
    };
    let segments_cross = |a: (f64, f64), b: (f64, f64), c: (f64, f64), d: (f64, f64)| -> bool {
        let (o1, o2) = (orient(a, b, c), orient(a, b, d));
        let (o3, o4) = (orient(c, d, a), orient(c, d, b));
        (o1 * o2 < 0.0 && o3 * o4 < 0.0)
            || (o1 == 0.0 && inside_box(a, b, c))
            || (o2 == 0.0 && inside_box(a, b, d))
            || (o3 == 0.0 && inside_box(c, d, a))
            || (o4 == 0.0 && inside_box(c, d, b))
    };
    let mut contains_corner = Vec::new();
    let mut crossed = Vec::new();
    for (ti, tri) in mesh.tris.iter().enumerate() {
        let ti = ti as u32;
        if excluded.binary_search(&ti).is_ok()
            || attribution.lookup(ti).map(|a| (a.input, a.face)) != Some(face)
        {
            continue;
        }
        let raw: Vec<(f64, f64)> = tri
            .iter()
            .map(|&v| {
                let w = chart.project(mesh.verts[v as usize]);
                (w.x(), w.y())
            })
            .collect();
        // Chain-unwrap about the first corner, then shift the whole triangle
        // to the period nearest the corner's azimuth.
        let mut c = [raw[0], raw[1], raw[2]];
        if !planar {
            for k in 1..3 {
                c[k].0 = near(c[0].0, c[k].0);
            }
            let mid = (c[0].0 + c[1].0 + c[2].0) / 3.0;
            let shift = ((cj.0 - mid) / std::f64::consts::TAU).round() * std::f64::consts::TAU;
            for corner in c.iter_mut() {
                corner.0 += shift;
            }
        }
        if biperiodic {
            for k in 1..3 {
                c[k].1 = near(c[0].1, c[k].1);
            }
            let mid = (c[0].1 + c[1].1 + c[2].1) / 3.0;
            let shift = ((cj.1 - mid) / std::f64::consts::TAU).round() * std::f64::consts::TAU;
            for corner in c.iter_mut() {
                corner.1 += shift;
            }
        }
        if inside(&c, cj) {
            contains_corner.push(ti);
            continue;
        }
        let hit = stubs
            .iter()
            .any(|&m| inside(&c, m) || (0..3).any(|k| segments_cross(m, cj, c[k], c[(k + 1) % 3])));
        if hit {
            crossed.push(ti);
        }
    }
    Ok(BiteRegion {
        face,
        contains_corner,
        crossed,
        corner_uv: cj,
    })
}

/// Whether `p` lies within the bounding box of segment `a b` (used only when
/// `p` is collinear with it).
fn inside_box(a: (f64, f64), b: (f64, f64), p: (f64, f64)) -> bool {
    p.0 >= a.0.min(b.0) && p.0 <= a.0.max(b.0) && p.1 >= a.1.min(b.1) && p.1 <= a.1.max(b.1)
}

/// Plan the emission fill from the pinched parts.
///
/// Composed from every measurement that precedes it: the mints and hosts
/// from `edits`, the mint ids from `region`, the parts and their closures from
/// `parts`, the loops from [`transit_boundary_pinch`], the corner's landing
/// from [`transit_bite_region`], the site's corrected position and the
/// neighbour's surface from `t`. Nothing is re-derived; the only things
/// computed here are the chart CDT of each polygon and the certificates on
/// the result. Every non-answer is typed.
#[allow(clippy::too_many_arguments)]
pub(crate) fn transit_emission_fill(
    mesh: &Mesh,
    attribution: &crate::brep::TriangleAttributionMap,
    edits: &TransitEmissionEdits,
    region: &TransitEmissionRegion,
    parts: &[RegionPart],
    t: &CreaseTransit,
    face_surface: &dyn Fn(InputId, u32) -> Option<Surface>,
    extra: &[ExtraMint],
) -> Result<TransitEmissionFill, EmissionFillFailure> {
    use crate::stage4_project::SurfaceChart;
    use cad_primitives::Point2;
    use std::collections::BTreeMap;
    use EmissionFillFailure as F;

    let site = edits.site;
    let mints = region.mints;
    // Every minted vertex with its position: the q-mints, then the extras at
    // the ids that follow.
    let all_mints: Vec<(u32, Point3)> = [
        (mints[0], edits.inserts[0].at),
        (mints[1], edits.inserts[1].at),
    ]
    .into_iter()
    .chain(
        extra
            .iter()
            .enumerate()
            .map(|(i, e)| (mints[1] + 1 + i as u32, e.at)),
    )
    .collect();
    let mut hosts = HostMints::from_edits(edits, mints);
    for (i, e) in extra.iter().enumerate() {
        hosts.add(e.host.0, e.host.1, e.t, mints[1] + 1 + i as u32);
    }
    let face_of = |tr: u32| attribution.lookup(tr).map(|a| (a.input, a.face));
    let pos = |v: u32| -> Point3 {
        if let Some((_, p)) = all_mints.iter().find(|(m, _)| *m == v) {
            *p
        } else if v == site {
            t.j
        } else {
            mesh.verts[v as usize]
        }
    };

    // The own patch: the part carrying the crease chord AT the site.
    let (ha, hb) = edits.crease_host;
    let own_idx = parts
        .iter()
        .position(|p| {
            p.tris.iter().any(|&tr| {
                let x = mesh.tris[tr as usize];
                x.contains(&ha) && x.contains(&hb) && x.contains(&site)
            })
        })
        .ok_or(F::OwnPartMissing)?;
    let own_face = parts[own_idx].face.ok_or(F::NoChart { face: None })?;

    // The corner's destination: who owns the far side of the chord.
    let mut dests: Vec<(InputId, u32)> = edits
        .crease_tris
        .iter()
        .filter_map(|&tr| face_of(tr))
        .filter(|f| *f != own_face)
        .collect();
    dests.sort_unstable();
    dests.dedup();
    let notch_face = match dests[..] {
        [] => return Err(F::NotchDestinationUnknown),
        [one] => one,
        _ => return Err(F::NotchDestinationAmbiguous),
    };
    let notch_surface_agrees = face_surface(notch_face.0, notch_face.1).map(|s| s == t.s_nbr);
    let nbr_idx = parts
        .iter()
        .position(|p| p.face == Some(notch_face))
        .ok_or(F::NeighbourPartMissing)?;

    // What the parts alone remove; then where the corner lands beyond them.
    let mut removed: Vec<u32> = parts
        .iter()
        .flat_map(|p| p.tris.iter().chain(p.closure.iter()).copied())
        .collect();
    removed.sort_unstable();
    removed.dedup();
    let bite = transit_bite_region(
        mesh,
        attribution,
        &removed,
        notch_face,
        t.j,
        [edits.inserts[0].at, edits.inserts[1].at],
        face_surface,
    )?;
    removed.extend(bite.contains_corner.iter().chain(bite.crossed.iter()));
    removed.sort_unstable();
    removed.dedup();
    // The work list, by face: the parts (closed), the neighbour enlarged by
    // the bite, and every carrier of a refinement host on whichever face it
    // lies — a face the parts did not name gets a part of its own.
    type WorkPart = (Option<(InputId, u32)>, Vec<u32>);
    let mut work: Vec<WorkPart> = parts
        .iter()
        .map(|p| {
            (
                p.face,
                p.tris.iter().chain(p.closure.iter()).copied().collect(),
            )
        })
        .collect();
    work[nbr_idx]
        .1
        .extend(bite.contains_corner.iter().chain(bite.crossed.iter()));
    for e in extra {
        for (ti, tri) in mesh.tris.iter().enumerate() {
            let ti = ti as u32;
            if !(tri.contains(&e.host.0) && tri.contains(&e.host.1)) {
                continue;
            }
            if removed.binary_search(&ti).is_ok() {
                continue;
            }
            let face = face_of(ti);
            match work.iter_mut().find(|(f, _)| *f == face) {
                Some((_, tris)) => tris.push(ti),
                None => work.push((face, vec![ti])),
            }
            removed.push(ti);
            removed.sort_unstable();
        }
    }
    for (_, tris) in work.iter_mut() {
        tris.sort_unstable();
        tris.dedup();
    }
    let touched_delta: Vec<u32> = removed
        .iter()
        .filter(|x| edits.touched.binary_search(x).is_err())
        .chain(
            edits
                .touched
                .iter()
                .filter(|x| removed.binary_search(x).is_err()),
        )
        .copied()
        .collect();

    // One loop → one chart fill, wound by the loop.
    let fill_loop = |l: &[u32], face: (InputId, u32), bite: bool| -> Result<FillPolygon, F> {
        if l.len() < 3 {
            return Err(F::PinchUndefined { face: Some(face) });
        }
        if l.len() == 3 {
            return Ok(FillPolygon {
                face,
                polygon: l.to_vec(),
                tris: vec![[l[0], l[1], l[2]]],
                bite,
                lift: Vec::new(),
            });
        }
        let surface = face_surface(face.0, face.1).ok_or(F::NoChart { face: Some(face) })?;
        let chart = SurfaceChart::new_local(surface).ok_or(F::NoChart { face: Some(face) })?;
        // Chain unwrap of every periodic coordinate, as the fan refill does:
        // the polygon is corner-local, so a wrap means the premise failed.
        let planar = matches!(chart, SurfaceChart::Plane { .. });
        let biperiodic = matches!(chart, SurfaceChart::Torus { .. });
        let wrap_near = |prev: f64, val: f64| -> f64 {
            let mut d = val - prev;
            while d > std::f64::consts::PI {
                d -= std::f64::consts::TAU;
            }
            while d <= -std::f64::consts::PI {
                d += std::f64::consts::TAU;
            }
            prev + d
        };
        let mut pool: Vec<Point2> = Vec::with_capacity(l.len());
        for (i, &v) in l.iter().enumerate() {
            let uv = chart.project(pos(v));
            let x = if i == 0 || planar {
                uv.x()
            } else {
                wrap_near(pool[i - 1].x(), uv.x())
            };
            let y = if i == 0 || !biperiodic {
                uv.y()
            } else {
                wrap_near(pool[i - 1].y(), uv.y())
            };
            pool.push(Point2::new(x, y));
        }
        if matches!(chart, SurfaceChart::Cone { .. }) && pool.iter().any(|p| p.y() <= 0.0) {
            return Err(F::ApexInPolygon { face });
        }
        let span = |sel: &dyn Fn(&Point2) -> f64| -> f64 {
            let (lo, hi) = pool
                .iter()
                .fold((f64::INFINITY, f64::NEG_INFINITY), |(lo, hi), p| {
                    (lo.min(sel(p)), hi.max(sel(p)))
                });
            hi - lo
        };
        if !planar {
            let s = span(&|p| p.x());
            if !s.is_finite() || s >= std::f64::consts::TAU {
                return Err(F::ThetaUnwrap { face });
            }
        }
        if biperiodic {
            let s = span(&|p| p.y());
            if !s.is_finite() || s >= std::f64::consts::TAU {
                return Err(F::ThetaUnwrap { face });
            }
        }
        let boundary: Vec<u32> = (0..l.len() as u32).collect();
        let local = cherchi_rs::cdt_with_interior_constraints(&pool, &boundary, &[], &[], &[])
            .map_err(|error| F::Cdt { face, error })?;
        // Wind by the loop, not by an area heuristic: a fill triangle that
        // carries a loop step must traverse it the way the loop does — that
        // is what pairs it with the neighbour across the step.
        let n = l.len();
        let step = |i: u32, j: u32| -> Option<bool> {
            let (i, j) = (i as usize, j as usize);
            if j == (i + 1) % n {
                Some(true)
            } else if i == (j + 1) % n {
                Some(false)
            } else {
                None
            }
        };
        let (mut fwd, mut rev) = (0usize, 0usize);
        for tri in &local {
            for k in 0..3 {
                match step(tri[k], tri[(k + 1) % 3]) {
                    Some(true) => fwd += 1,
                    Some(false) => rev += 1,
                    None => {}
                }
            }
        }
        if (fwd == 0) == (rev == 0) {
            return Err(F::OrientationUndefined { face });
        }
        let mut tris: Vec<[u32; 3]> = local
            .iter()
            .map(|tri| [l[tri[0] as usize], l[tri[1] as usize], l[tri[2] as usize]])
            .collect();
        if rev > 0 {
            for tri in &mut tris {
                tri.swap(1, 2);
            }
        }
        Ok(FillPolygon {
            face,
            polygon: l.to_vec(),
            tris,
            bite,
            lift: Vec::new(),
        })
    };

    // Structure first, charts second: every part is pinched and checked
    // before any polygon is projected, so a structural decline is reported as
    // such and never masked by a chart failure on an earlier part.
    let mut loops: Vec<(Vec<u32>, (InputId, u32), bool)> = Vec::new();
    let mut dropped: Vec<u32> = Vec::new();
    for (pi, (pface, tris)) in work.iter().enumerate() {
        let b = part_boundary(mesh, tris).map_err(|_| {
            if pi == nbr_idx {
                F::BiteNotADisk
            } else {
                F::PartNotADisk { face: *pface }
            }
        })?;
        let pin = pinch_cycle(insert_mints(&b, &hosts), mints, site)
            .ok_or(F::PinchUndefined { face: *pface })?;
        if pi == own_idx {
            if pin.interleaved {
                return Err(F::Interleaved);
            }
            let notch = pin.notch.ok_or(F::NoNotch)?;
            for (li, l) in pin.loops.iter().enumerate() {
                if li == notch {
                    dropped = l.clone();
                } else {
                    loops.push((l.clone(), own_face, false));
                }
            }
            continue;
        }
        if pin.loops.len() != 1 {
            return Err(F::UnexpectedPinch { face: *pface });
        }
        let face = pface.ok_or(F::NoChart { face: None })?;
        if pi == nbr_idx {
            // The neighbour: the corner step between the mints is detoured
            // through the site.
            let l = &pin.loops[0];
            let n = l.len();
            let k = (0..n)
                .find(|&k| {
                    let (a, b) = (l[k], l[(k + 1) % n]);
                    (a == mints[0] && b == mints[1]) || (a == mints[1] && b == mints[0])
                })
                .ok_or(F::CornerStepMissing)?;
            let floor = cad_primitives::MIN_FEATURE_SIZE;
            for &v in l {
                let d = dist3(pos(v), t.j);
                if d < floor {
                    return Err(F::CornerCoincident { v, dist: d });
                }
            }
            let mut detoured = l.clone();
            detoured.insert(k + 1, site);
            loops.push((detoured, notch_face, true));
        } else {
            loops.push((pin.loops[0].clone(), face, false));
        }
    }
    let mut polygons: Vec<FillPolygon> = Vec::with_capacity(loops.len());
    for (l, face, is_bite) in &loops {
        polygons.push(fill_loop(l, *face, *is_bite)?);
    }

    // --- Certificates on the result, against the WHOLE mesh. ---
    let added: Vec<([u32; 3], (InputId, u32))> = polygons
        .iter()
        .flat_map(|p| p.tris.iter().map(move |tri| (*tri, p.face)))
        .collect();
    let key = |a: u32, b: u32| (a.min(b), a.max(b));

    // Undirected incidence: before, and the edit's removed/added deltas.
    let mut before: BTreeMap<(u32, u32), usize> = BTreeMap::new();
    for tri in &mesh.tris {
        for k in 0..3 {
            *before.entry(key(tri[k], tri[(k + 1) % 3])).or_default() += 1;
        }
    }
    let mut delta: BTreeMap<(u32, u32), (usize, usize)> = BTreeMap::new();
    for &tr in &removed {
        let tri = mesh.tris[tr as usize];
        for k in 0..3 {
            delta.entry(key(tri[k], tri[(k + 1) % 3])).or_default().0 += 1;
        }
    }
    for (tri, _) in &added {
        for k in 0..3 {
            delta.entry(key(tri[k], tri[(k + 1) % 3])).or_default().1 += 1;
        }
    }
    let mut edge_defects = Vec::new();
    for (&e, &(rem, add)) in &delta {
        let b = before.get(&e).copied().unwrap_or(0);
        let after = b - rem + add;
        let expected = match (add > 0, b > 0) {
            (true, true) => b,
            (true, false) => 2,
            (false, _) => 0,
        };
        if after != expected {
            edge_defects.push(EdgeIncidence {
                edge: e,
                before: b,
                after,
                expected,
            });
        }
    }

    // Directed incidence among SURVIVORS: a fill edge they share must be
    // traversed the opposite way.
    let mut directed: BTreeMap<(u32, u32), usize> = BTreeMap::new();
    for tri in &mesh.tris {
        for k in 0..3 {
            *directed.entry((tri[k], tri[(k + 1) % 3])).or_default() += 1;
        }
    }
    for &tr in &removed {
        let tri = mesh.tris[tr as usize];
        for k in 0..3 {
            *directed.entry((tri[k], tri[(k + 1) % 3])).or_default() -= 1;
        }
    }
    let (mut opposed, mut folded) = (0usize, 0usize);
    let mut added_dirs: BTreeMap<(u32, u32), usize> = BTreeMap::new();
    for (tri, _) in &added {
        for k in 0..3 {
            let (x, y) = (tri[k], tri[(k + 1) % 3]);
            if directed.get(&(y, x)).copied().unwrap_or(0) > 0 {
                opposed += 1;
            }
            if directed.get(&(x, y)).copied().unwrap_or(0) > 0 {
                folded += 1;
            }
            *added_dirs.entry((x, y)).or_default() += 1;
        }
    }
    let added_folds = added_dirs.values().filter(|c| **c > 1).count();

    // Like-for-like d(T) per face: what each face gives up against what it
    // receives, each triangle unwrapped about its own first corner.
    let mut faces: Vec<(InputId, u32)> = added.iter().map(|(_, f)| *f).collect();
    faces.extend(removed.iter().filter_map(|&tr| face_of(tr)));
    faces.sort_unstable();
    faces.dedup();
    let certify = |chart: &SurfaceChart,
                   surface: &Surface,
                   tris: &[[u32; 3]],
                   at: &dyn Fn(u32) -> Point3|
     -> Option<f64> {
        let biperiodic = matches!(chart, SurfaceChart::Torus { .. });
        let planar = matches!(chart, SurfaceChart::Plane { .. });
        let mut worst = 0.0f64;
        for tri in tris {
            let raw = [
                chart.project(at(tri[0])),
                chart.project(at(tri[1])),
                chart.project(at(tri[2])),
            ];
            let near = |prev: f64, val: f64| -> f64 {
                let mut d = val - prev;
                while d > std::f64::consts::PI {
                    d -= std::f64::consts::TAU;
                }
                while d <= -std::f64::consts::PI {
                    d += std::f64::consts::TAU;
                }
                prev + d
            };
            let mut uv = raw;
            for c in uv.iter_mut().skip(1) {
                let x = if planar {
                    c.x()
                } else {
                    near(raw[0].x(), c.x())
                };
                let y = if biperiodic {
                    near(raw[0].y(), c.y())
                } else {
                    c.y()
                };
                *c = Point2::new(x, y);
            }
            worst = worst.max(crate::stage4_dt::d_of_t(surface, uv).ok()?);
        }
        Some(worst)
    };
    let chord: Vec<ChordBudget> = faces
        .iter()
        .map(|&face| {
            let chart = face_surface(face.0, face.1)
                .map(|s| (s, SurfaceChart::new_local(s)))
                .and_then(|(s, c)| c.map(|c| (s, c)));
            let Some((surface, chart)) = chart else {
                return ChordBudget {
                    face,
                    old_max: None,
                    new_max: None,
                };
            };
            let old: Vec<[u32; 3]> = removed
                .iter()
                .filter(|&&tr| face_of(tr) == Some(face))
                .map(|&tr| mesh.tris[tr as usize])
                .collect();
            let new: Vec<[u32; 3]> = added
                .iter()
                .filter(|(_, f)| *f == face)
                .map(|(tri, _)| *tri)
                .collect();
            ChordBudget {
                face,
                old_max: certify(&chart, &surface, &old, &|v| mesh.verts[v as usize]),
                new_max: certify(&chart, &surface, &new, &pos),
            }
        })
        .collect();

    // The lift sense per face: the 3D normal of each triangle against the
    // face surface's gradient at its centroid — removed triangles at their
    // old positions, added ones at the planned positions, survivors as they
    // stand. The survivors are the reference.
    let sense = |surface: Surface, tri: [u32; 3], at: &dyn Fn(u32) -> Point3| -> Option<bool> {
        let (a, b, c) = (
            at(tri[0]).as_array(),
            at(tri[1]).as_array(),
            at(tri[2]).as_array(),
        );
        let u = [b[0] - a[0], b[1] - a[1], b[2] - a[2]];
        let w = [c[0] - a[0], c[1] - a[1], c[2] - a[2]];
        let n = [
            u[1] * w[2] - u[2] * w[1],
            u[2] * w[0] - u[0] * w[2],
            u[0] * w[1] - u[1] * w[0],
        ];
        let centroid = [
            (a[0] + b[0] + c[0]) / 3.0,
            (a[1] + b[1] + c[1]) / 3.0,
            (a[2] + b[2] + c[2]) / 3.0,
        ];
        let (_, g) = crate::stage4_relocate::surface_value_and_normal(surface, centroid)?;
        let d = n[0] * g[0] + n[1] * g[1] + n[2] * g[2];
        if d == 0.0 || !d.is_finite() {
            None
        } else {
            Some(d > 0.0)
        }
    };
    let mut lift: Vec<LiftSense> = Vec::with_capacity(faces.len());
    let (mut lift_flips, mut lift_uncertified) = (0usize, 0usize);
    for &face in &faces {
        let mut ls = LiftSense {
            face,
            old_along: 0,
            old_against: 0,
            new_along: 0,
            new_against: 0,
            survivors_along: 0,
            survivors_against: 0,
            uncertified: 0,
        };
        if let Some(surface) = face_surface(face.0, face.1) {
            for &tr in removed.iter().filter(|&&tr| face_of(tr) == Some(face)) {
                match sense(surface, mesh.tris[tr as usize], &|v| mesh.verts[v as usize]) {
                    Some(true) => ls.old_along += 1,
                    Some(false) => ls.old_against += 1,
                    None => ls.uncertified += 1,
                }
            }
            for (ti, tri) in mesh.tris.iter().enumerate() {
                let ti = ti as u32;
                if removed.binary_search(&ti).is_ok() || face_of(ti) != Some(face) {
                    continue;
                }
                match sense(surface, *tri, &|v| mesh.verts[v as usize]) {
                    Some(true) => ls.survivors_along += 1,
                    Some(false) => ls.survivors_against += 1,
                    None => {}
                }
            }
            for poly in polygons.iter_mut().filter(|p| p.face == face) {
                poly.lift = poly
                    .tris
                    .iter()
                    .map(|tri| sense(surface, *tri, &pos))
                    .collect();
                for l in &poly.lift {
                    match l {
                        Some(true) => ls.new_along += 1,
                        Some(false) => ls.new_against += 1,
                        None => ls.uncertified += 1,
                    }
                }
            }
        } else {
            ls.uncertified = removed
                .iter()
                .filter(|&&tr| face_of(tr) == Some(face))
                .count()
                + added.iter().filter(|(_, f)| *f == face).count();
        }
        let reference = if ls.survivors_along + ls.survivors_against > 0 {
            ls.survivors_along.cmp(&ls.survivors_against)
        } else {
            ls.old_along.cmp(&ls.old_against)
        };
        lift_flips += match reference {
            std::cmp::Ordering::Greater => ls.new_against,
            std::cmp::Ordering::Less => ls.new_along,
            std::cmp::Ordering::Equal => ls.new_along.min(ls.new_against),
        };
        lift_uncertified += ls.uncertified;
        lift.push(ls);
    }

    Ok(TransitEmissionFill {
        site,
        site_at: t.j,
        mints: all_mints,
        removed,
        touched_delta,
        own_face,
        notch_face,
        notch_surface_agrees,
        bite,
        dropped,
        polygons,
        edge_defects,
        opposed,
        folded,
        added_folds,
        chord,
        lift,
        lift_flips,
        lift_uncertified,
    })
}

// ---------------------------------------------------------------------------
// §4.5.2 inc-2c-3b-12b-10 — REFINEMENT until the lift is faithful
// ---------------------------------------------------------------------------

/// How one refinement mint was chosen.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(crate) enum RefineKind {
    /// The flipped triangle's crease chord, halved at its exact arc
    /// midpoint: its apex lies in the face's interior, so the chord's own
    /// dip is what folds it.
    Halve,
    /// The flipped triangle's apex lies on the face's OTHER crease: the
    /// face is a band, its fossils lift faithfully only because the strip
    /// triangulation keeps every apex above its base's end, and no halving
    /// of the base can restore that. The other crease is split at the
    /// azimuth of the base's midpoint instead — the matched vertex the
    /// strip needs — which pulls the face beyond it into the fill.
    Matched,
}

/// One round of Yang §4.5.2 refinement at one flipped fill triangle.
#[derive(Clone, Debug, PartialEq)]
pub(crate) struct RefineRound {
    pub(crate) face: (InputId, u32),
    pub(crate) tri: [u32; 3],
    pub(crate) kind: RefineKind,
    /// The crease-chord step of the triangle the round acted on (its ends
    /// may be mints): halved, or matched on the other crease.
    pub(crate) chord: (u32, u32),
    /// The original mesh edge the mint is inserted on — the refinement host.
    pub(crate) host: (u32, u32),
    pub(crate) mint: Point3,
    pub(crate) chord_len: f64,
    /// The chord midpoint's distance to its arc — what a halving removes.
    pub(crate) sagitta: f64,
}

/// Why refinement could not reach a faithful lift.
#[derive(Clone, Debug, PartialEq)]
pub(crate) enum EmissionRefineFailure {
    Fill(EmissionFillFailure),
    /// A face's reference sense is undecided (no survivor and a fossil that
    /// disagrees with itself), so its flips cannot be named.
    ReferenceUndecided {
        face: (InputId, u32),
    },
    /// A flipped triangle has no polygon step lying on a crease of its
    /// face, so there is no chord to split.
    FlipWithoutCreaseChord {
        face: (InputId, u32),
        tri: [u32; 3],
    },
    /// The step's ends do not share one original host edge.
    HostAmbiguous {
        chord: (u32, u32),
    },
    /// A matched split's point on the other crease lies beyond every crease
    /// edge of the apex, so no host edge carries it.
    MatchBeyondNeighbours {
        apex: u32,
        chord: (u32, u32),
    },
    /// The chord midpoint has no unique projection onto the crease circle.
    MidpointDegenerate {
        chord: (u32, u32),
    },
    /// The iteration cap was reached with flips remaining — loud, not a
    /// fallback. Carries the halvings made and the last fill, so a census
    /// can see WHERE refinement stalled.
    CapReached {
        iterations: usize,
        lift_flips: usize,
        rounds: Vec<RefineRound>,
        last: Box<TransitEmissionFill>,
    },
}

/// A fill refined until every triangle lifts the way its face's survivors
/// do.
#[derive(Clone, Debug, PartialEq)]
pub(crate) struct TransitEmissionRefined {
    pub(crate) fill: TransitEmissionFill,
    pub(crate) extra: Vec<ExtraMint>,
    pub(crate) rounds: Vec<RefineRound>,
}

/// Yang §4.5.2, certificate-driven and constant-free: plan the fill; while
/// any triangle lifts against its face's reference, split the longest
/// crease chord of each such triangle at the exact arc midpoint (a mint
/// every carrier of that chord receives, on both faces), and plan again.
/// Halving is the only step; `cap` bounds the rounds and is a loud STOP.
///
/// `face_creases` names a face's own creases as `(C_b, s_face, s_other)`
/// triples; a polygon step is a crease chord when both its ends lie on one
/// of them ([`on_crease`], the membership test the trigger uses).
#[allow(clippy::too_many_arguments)]
pub(crate) fn transit_emission_refine(
    mesh: &Mesh,
    attribution: &crate::brep::TriangleAttributionMap,
    edits: &TransitEmissionEdits,
    region: &TransitEmissionRegion,
    parts: &[RegionPart],
    t: &CreaseTransit,
    face_surface: &dyn Fn(InputId, u32) -> Option<Surface>,
    face_creases: &dyn Fn((InputId, u32)) -> Vec<(Curve, Surface, Surface)>,
    cap: usize,
) -> Result<TransitEmissionRefined, EmissionRefineFailure> {
    use EmissionRefineFailure as R;
    let mut extra: Vec<ExtraMint> = Vec::new();
    let mut rounds: Vec<RefineRound> = Vec::new();
    let mut iterations = 0usize;
    loop {
        let fill = transit_emission_fill(
            mesh,
            attribution,
            edits,
            region,
            parts,
            t,
            face_surface,
            &extra,
        )
        .map_err(R::Fill)?;
        if fill.lift_flips == 0 {
            return Ok(TransitEmissionRefined {
                fill,
                extra,
                rounds,
            });
        }
        if iterations >= cap {
            return Err(R::CapReached {
                iterations,
                lift_flips: fill.lift_flips,
                rounds,
                last: Box::new(fill),
            });
        }
        iterations += 1;
        let pos = |v: u32| -> Point3 {
            if let Some((_, p)) = fill.mints.iter().find(|(m, _)| *m == v) {
                *p
            } else if v == fill.site {
                fill.site_at
            } else {
                mesh.verts[v as usize]
            }
        };
        // The original mesh edge under a step: the step itself when both
        // ends are mesh vertices; a mint's host when one end is a mint; the
        // shared host when both are.
        let host_of = |v: u32| -> Option<(u32, u32)> {
            if v == region.mints[0] || v == region.mints[1] {
                return Some(edits.crease_host);
            }
            let k = fill.mints.iter().position(|(m, _)| *m == v)?;
            extra.get(k.checked_sub(2)?).map(|e| e.host)
        };
        let is_mint = |v: u32| fill.mints.iter().any(|(m, _)| *m == v);
        let mut new_extra: Vec<ExtraMint> = Vec::new();
        let mut new_rounds: Vec<RefineRound> = Vec::new();
        for poly in &fill.polygons {
            let ls = fill
                .lift
                .iter()
                .find(|l| l.face == poly.face)
                .expect("every polygon face is certified");
            let reference = if ls.survivors_along + ls.survivors_against > 0 {
                ls.survivors_along.cmp(&ls.survivors_against)
            } else {
                ls.old_along.cmp(&ls.old_against)
            };
            let reference = match reference {
                std::cmp::Ordering::Greater => true,
                std::cmp::Ordering::Less => false,
                std::cmp::Ordering::Equal => {
                    if poly.lift.iter().any(|l| l.is_some()) {
                        return Err(R::ReferenceUndecided { face: poly.face });
                    }
                    continue;
                }
            };
            let creases = face_creases(poly.face);
            let n = poly.polygon.len();
            let is_step = |x: u32, y: u32| {
                (0..n).any(|i| {
                    let s = (poly.polygon[i], poly.polygon[(i + 1) % n]);
                    s == (x, y) || s == (y, x)
                })
            };
            for (k, tri) in poly.tris.iter().enumerate() {
                if poly.lift.get(k).copied().flatten() != Some(!reference) {
                    continue;
                }
                let mut best: Option<(f64, (u32, u32), Curve)> = None;
                for e in 0..3 {
                    let (x, y) = (tri[e], tri[(e + 1) % 3]);
                    if !is_step(x, y) {
                        continue;
                    }
                    for (c, s0, s1) in &creases {
                        if on_crease(pos(x), *s0, *s1) && on_crease(pos(y), *s0, *s1) {
                            let len = dist3(pos(x), pos(y));
                            if best.as_ref().is_none_or(|b| len > b.0) {
                                best = Some((len, (x, y), *c));
                            }
                        }
                    }
                }
                let Some((chord_len, (x, y), curve)) = best else {
                    return Err(R::FlipWithoutCreaseChord {
                        face: poly.face,
                        tri: *tri,
                    });
                };
                if new_rounds
                    .iter()
                    .any(|r| r.chord == (x, y) || r.chord == (y, x))
                {
                    continue;
                }
                let (px, py) = (pos(x).as_array(), pos(y).as_array());
                let mid = Point3::new(
                    0.5 * (px[0] + py[0]),
                    0.5 * (px[1] + py[1]),
                    0.5 * (px[2] + py[2]),
                );
                let apex = *tri
                    .iter()
                    .find(|v| **v != x && **v != y)
                    .expect("a triangle");
                // Is the apex on ANOTHER crease of this face? Then the
                // triangle is a band's, and the other crease is what to
                // split — at the matched azimuth, not at its own midpoint.
                let other = creases
                    .iter()
                    .find(|(c, s0, s1)| *c != curve && on_crease(pos(apex), *s0, *s1));
                let (kind, host, mint) = match other {
                    Some((c2, s0, s1)) => {
                        let m = project_onto_curve(mid, c2)
                            .ok_or(R::MidpointDegenerate { chord: (x, y) })?;
                        // The host on the other crease: the apex's own crease
                        // edge (or, for a minted apex, its host) whose chord
                        // brackets the matched point.
                        let along = |a: Point3, b: Point3, q: Point3| -> f64 {
                            let (a, b, q) = (a.as_array(), b.as_array(), q.as_array());
                            let d = [b[0] - a[0], b[1] - a[1], b[2] - a[2]];
                            ((q[0] - a[0]) * d[0] + (q[1] - a[1]) * d[1] + (q[2] - a[2]) * d[2])
                                / (d[0] * d[0] + d[1] * d[1] + d[2] * d[2])
                        };
                        let host = if is_mint(apex) {
                            host_of(apex).ok_or(R::HostAmbiguous { chord: (x, y) })?
                        } else {
                            let mut found: Option<(u32, u32)> = None;
                            for tri2 in mesh.tris.iter().filter(|t2| t2.contains(&apex)) {
                                for &u in tri2.iter().filter(|u| **u != apex) {
                                    if !on_crease(mesh.verts[u as usize], *s0, *s1) {
                                        continue;
                                    }
                                    let tt = along(pos(apex), mesh.verts[u as usize], m);
                                    if tt > 0.0 && tt < 1.0 {
                                        found = Some((apex, u));
                                    }
                                }
                            }
                            found.ok_or(R::MatchBeyondNeighbours {
                                apex,
                                chord: (x, y),
                            })?
                        };
                        (RefineKind::Matched, host, m)
                    }
                    None => {
                        let host = match (is_mint(x), is_mint(y)) {
                            (false, false) => (x, y),
                            (true, false) => {
                                host_of(x).ok_or(R::HostAmbiguous { chord: (x, y) })?
                            }
                            (false, true) => {
                                host_of(y).ok_or(R::HostAmbiguous { chord: (x, y) })?
                            }
                            (true, true) => match (host_of(x), host_of(y)) {
                                (Some(hx), Some(hy)) if hx == hy => hx,
                                _ => return Err(R::HostAmbiguous { chord: (x, y) }),
                            },
                        };
                        let m = project_onto_curve(mid, &curve)
                            .ok_or(R::MidpointDegenerate { chord: (x, y) })?;
                        (RefineKind::Halve, host, m)
                    }
                };
                // A mint already at this point (the other face across the
                // same chord asks for the same split) is not minted twice.
                let floor = cad_primitives::MIN_FEATURE_SIZE;
                if fill.mints.iter().any(|(_, q)| dist3(*q, mint) < floor)
                    || new_extra.iter().any(|e| dist3(e.at, mint) < floor)
                {
                    continue;
                }
                let (h0, h1) = (
                    mesh.verts[host.0 as usize].as_array(),
                    mesh.verts[host.1 as usize].as_array(),
                );
                let d = [h1[0] - h0[0], h1[1] - h0[1], h1[2] - h0[2]];
                let mm = mint.as_array();
                let tt = ((mm[0] - h0[0]) * d[0] + (mm[1] - h0[1]) * d[1] + (mm[2] - h0[2]) * d[2])
                    / (d[0] * d[0] + d[1] * d[1] + d[2] * d[2]);
                new_extra.push(ExtraMint {
                    at: mint,
                    host,
                    t: tt,
                });
                new_rounds.push(RefineRound {
                    face: poly.face,
                    tri: *tri,
                    kind,
                    chord: (x, y),
                    host,
                    mint,
                    chord_len,
                    sagitta: dist3(mid, mint),
                });
            }
        }
        if new_extra.is_empty() {
            return Err(R::CapReached {
                iterations,
                lift_flips: fill.lift_flips,
                rounds,
                last: Box::new(fill),
            });
        }
        extra.extend(new_extra);
        rounds.extend(new_rounds);
    }
}

// ---------------------------------------------------------------------------
// §4.5.2 inc-2c-3b-12b-8 — WRITING the certified fill (the gated apply arm)
// ---------------------------------------------------------------------------

/// Why a certified-looking fill was not written. Every variant is a refusal
/// the caller must treat as "the standing STOP applies"; none is recoverable
/// by retrying.
#[derive(Clone, Debug, PartialEq)]
pub(crate) enum EmissionWriteFailure {
    /// A certificate on the plan is not clean. The write is only defined on a
    /// fill whose result is manifold, consistently wound, attributed to the
    /// surface the analysis named, and no coarser than what it replaces.
    CertificateFailed { what: &'static str },
    /// The mint ids the plan was built with are not the next two vertex ids:
    /// the mesh changed between planning and writing.
    MintIdsStale { planned: [u32; 2], next: u32 },
    /// The attribution map is not parallel to the triangle list.
    AttributionLength { tris: usize, attributions: usize },
    /// The fill has fewer triangles than it removes, so some removed slot
    /// would have to be deleted and every later triangle index shifted. Not
    /// built (no corpus site needs it); refused rather than shuffled.
    FewerAddedThanRemoved { added: usize, removed: usize },
}

/// What the write did, as counts the census can print.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(crate) struct EmissionWriteReport {
    pub(crate) site: u32,
    /// The two q-mints' ids.
    pub(crate) mints: [u32; 2],
    /// Every vertex minted, q-mints included.
    pub(crate) minted: usize,
    pub(crate) removed: usize,
    pub(crate) added: usize,
    /// Removed slots overwritten in place (so no triangle index shifts).
    pub(crate) overwritten: usize,
    /// Fill triangles appended past the old end.
    pub(crate) appended: usize,
}

/// Write a certified [`TransitEmissionFill`] to the mesh.
///
/// Slot-stable: the fill's triangles overwrite the removed slots in place and
/// the surplus is appended, so every triangle index outside the region keeps
/// its meaning; the attribution map is updated in the same slots. The two
/// mints are appended at exactly the ids the plan named, and the site moves
/// to the corrected junction the plan projected it at. Refuses — leaving the
/// mesh untouched — on any unclean certificate.
pub(crate) fn transit_emission_write(
    mesh: &mut Mesh,
    attribution: &mut crate::brep::TriangleAttributionMap,
    fill: &TransitEmissionFill,
) -> Result<EmissionWriteReport, EmissionWriteFailure> {
    use EmissionWriteFailure as F;
    if !fill.edge_defects.is_empty() {
        return Err(F::CertificateFailed {
            what: "edge_defects",
        });
    }
    if fill.folded > 0 {
        return Err(F::CertificateFailed { what: "folded" });
    }
    if fill.added_folds > 0 {
        return Err(F::CertificateFailed {
            what: "added_folds",
        });
    }
    if fill.notch_surface_agrees != Some(true) {
        return Err(F::CertificateFailed {
            what: "notch_surface_agrees",
        });
    }
    for c in &fill.chord {
        match (c.old_max, c.new_max) {
            (Some(old), Some(new)) if new <= old => {}
            _ => return Err(F::CertificateFailed { what: "chord" }),
        }
    }
    if fill.lift_flips > 0 {
        return Err(F::CertificateFailed { what: "lift_flips" });
    }
    if fill.lift_uncertified > 0 {
        return Err(F::CertificateFailed {
            what: "lift_uncertified",
        });
    }
    if attribution.attributions.len() != mesh.tris.len() {
        return Err(F::AttributionLength {
            tris: mesh.tris.len(),
            attributions: attribution.attributions.len(),
        });
    }
    let next = mesh.verts.len() as u32;
    let planned = [fill.mints[0].0, fill.mints[1].0];
    if fill
        .mints
        .iter()
        .enumerate()
        .any(|(i, (m, _))| *m != next + i as u32)
    {
        return Err(F::MintIdsStale { planned, next });
    }
    let added: Vec<([u32; 3], (InputId, u32))> = fill
        .polygons
        .iter()
        .flat_map(|p| p.tris.iter().map(move |tri| (*tri, p.face)))
        .collect();
    if added.len() < fill.removed.len() {
        return Err(F::FewerAddedThanRemoved {
            added: added.len(),
            removed: fill.removed.len(),
        });
    }

    for (_, at) in &fill.mints {
        mesh.verts.push(*at);
    }
    mesh.verts[fill.site as usize] = fill.site_at;
    let mut overwritten = 0usize;
    let mut appended = 0usize;
    for (i, (tri, face)) in added.iter().enumerate() {
        let att = Some(crate::brep::TriangleAttribution {
            input: face.0,
            face: face.1,
        });
        if i < fill.removed.len() {
            let slot = fill.removed[i] as usize;
            mesh.tris[slot] = *tri;
            attribution.attributions[slot] = att;
            overwritten += 1;
        } else {
            mesh.tris.push(*tri);
            attribution.attributions.push(att);
            appended += 1;
        }
    }
    Ok(EmissionWriteReport {
        site: fill.site,
        mints: planned,
        minted: fill.mints.len(),
        removed: fill.removed.len(),
        added: added.len(),
        overwritten,
        appended,
    })
}
