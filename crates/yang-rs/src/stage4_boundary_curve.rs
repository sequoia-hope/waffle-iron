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
//! **inc-2 status:** WIRED into `stage4_relocate_and_correct`, gated OFF by
//! default behind `YANG_S4_RIM_SNAP_ENABLE` — every addition sits inside that
//! `if`, so the production path is unchanged by construction. Gate-ON it fixes
//! the n2 I1 reproduction (moves exactly `v6`) with the full yang-rs suite green
//! and ZERO corpus category deltas. It does NOT yet reach the corpus tail
//! (F0083/R0099 still `VertexOffSurface`) — see the spec's inc-3 question.

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
        return Err(YangError::Stage4RegionInvalid {
            vertex,
            reason: Stage4InvalidReason::LocalRefinementRequired,
        });
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
        for (v, q) in pending {
            if cross_curve_endpoints.contains(&v) {
                continue;
            }
            moves.entry(v).or_insert(q);
        }
    }
    moves.into_iter().collect()
}

/// Gate for the §4.4.1 boundary-curve relocation pass (inc-2). Default OFF;
/// `YANG_S4_RIM_SNAP_ENABLE=1|on` enables it.
pub(crate) fn rim_snap_enabled() -> bool {
    matches!(
        std::env::var("YANG_S4_RIM_SNAP_ENABLE").as_deref(),
        Ok("1") | Ok("on")
    )
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
