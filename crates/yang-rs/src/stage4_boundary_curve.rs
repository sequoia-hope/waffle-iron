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
