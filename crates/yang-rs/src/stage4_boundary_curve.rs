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
    })
}
