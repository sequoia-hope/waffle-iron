//! §4.4.2 carried-edge curve restoration in the boolean OUTPUT — the
//! KV9-F2a deep-chord fold family's owner (spec
//! `specs/yang_434_output_chord_refinement.md`, revised by measurement
//! 2026-08-24).
//!
//! The paper's B-Rep Boolean output restores patches bounded by "either the
//! original boundary curves or the intersection curves"
//! (`refs/text/yang2025_hybrid_boolean.txt:592-605`). The shipped emission
//! types the intersection curves (the `intersection_curves` map) but left
//! ORIGINAL boundary curves between same-input faces as one `LineSegment`
//! edge per mesh seam segment — a polyline at Stage-1 MESH density.
//! kernel-v2's developable tessellator keeps `LineSegment` boundary splits
//! collinear on the original 3D chord (the load-bearing T-junction closure
//! rule), so a facet-deep chord's sagitta became a permanent off-surface
//! deviation and a sliver thinner than that depth folded (KV9-F2a:
//! R0003/R0100/R0020). The inc-1 census measured every deep chord in those
//! cases onto an INPUT `Circle` edge (15 077/15 077 at ≤1.0e-11) — carried
//! revolve rims, not Stage-3 intersection output.
//!
//! [`restore_carried_edge_curves`] (ALWAYS-ON; `YANG_434_OUT=0|off` restores
//! the polyline emission) re-types the
//! certified chords in place; the always-on I5-1b merge then coalesces the
//! runs into arc edges and kernel-v2 samples them at render density.
//!
//! [`census_output_pair_chords`] (`YANG_434_OUT=census`, print-only, apply
//! off): per 2-use `LineSegment` chord between two distinct faces — owner
//! class (cross/same-input/same-surface), surfaces, midpoint depth
//! (pair-Newton or plain surface distance), endpoint residuals (GIGO
//! guard), curve-map status, and the carried input-circle match. The
//! measurement that decided this design.

use std::collections::BTreeMap;

use cad_primitives::Point3;

use crate::brep::{BRepEdge, BRepFace, InputId, TriangleAttribution};
use crate::geom::{Curve, Surface};
use crate::stage4_relocate::{relocate_onto_implicit_pair, surface_distance_and_normal};

/// 3D distance from `p` to the circle (`center`, unit `normal`, `radius`).
fn dist_to_circle(p: Point3, center: Point3, normal: [f64; 3], radius: f64) -> f64 {
    let (p, c) = (p.as_array(), center.as_array());
    let d = [p[0] - c[0], p[1] - c[1], p[2] - c[2]];
    let h = d[0] * normal[0] + d[1] * normal[1] + d[2] * normal[2];
    let r = [
        d[0] - h * normal[0],
        d[1] - h * normal[1],
        d[2] - h * normal[2],
    ];
    let rl = (r[0] * r[0] + r[1] * r[1] + r[2] * r[2]).sqrt();
    ((rl - radius).powi(2) + h * h).sqrt()
}

/// 3D distance from `p` to the ellipse, measured at the foot the ellipse's
/// own angular parameter names (`ellipse_param` → `conic_eval`): exact
/// zero for an on-curve point and an upper bound on the true distance
/// otherwise — sufficient for a certification that only ever ACCEPTS
/// within the classify band (P0007, 2026-09-29: the re-entered oblique
/// plane × cylinder boundaries are `Curve::Ellipse` input edges).
fn dist_to_ellipse(p: Point3, curve: &Curve) -> Option<f64> {
    let Curve::Ellipse {
        center,
        normal,
        major_axis,
        major_radius,
        minor_radius,
    } = *curve
    else {
        return None;
    };
    let t = crate::geom::ellipse_param(p, center, normal, major_axis, major_radius, minor_radius);
    let q = crate::geom::conic_eval(curve, t)?;
    let (pa, qa) = (p.as_array(), q.as_array());
    Some(((pa[0] - qa[0]).powi(2) + (pa[1] - qa[1]).powi(2) + (pa[2] - qa[2]).powi(2)).sqrt())
}

/// The max endpoint residual of a chord `(p, q)` against a carried conic
/// candidate — `Circle` and `Ellipse`; `None` for every other curve kind.
fn carried_residual(curve: &Curve, p: Point3, q: Point3) -> Option<f64> {
    match *curve {
        Curve::Circle {
            center,
            normal,
            radius,
        } => {
            let n = crate::normalize3(normal.as_array());
            Some(dist_to_circle(p, center, n, radius).max(dist_to_circle(q, center, n, radius)))
        }
        Curve::Ellipse { .. } => Some(dist_to_ellipse(p, curve)?.max(dist_to_ellipse(q, curve)?)),
        _ => None,
    }
}

/// The candidate carried-curve match for a same-input seam chord: the
/// non-`LineSegment` curves on the loops of the two INPUT faces the chord's
/// owners descend from, tested by both chord endpoints' distance to the
/// curve. Returns the best `(curve, max endpoint distance)`.
fn carried_curve_match(
    input: (&[BRepFace], &[BRepEdge]),
    face_a: u32,
    face_b: u32,
    p: Point3,
    q: Point3,
) -> Option<(Curve, f64)> {
    let mut best: Option<(Curve, f64)> = None;
    for fi in [face_a, face_b] {
        let Some(f) = input.0.get(fi as usize) else {
            continue;
        };
        for lp in std::iter::once(&f.outer_loop).chain(f.inner_loops.iter()) {
            for &ei in lp {
                let Some(e) = input.1.get(ei as usize) else {
                    continue;
                };
                // Circle (revolve/extrude rims) and Ellipse (a re-entered
                // oblique plane × cylinder boundary, KV14 ellipse re-entry —
                // P0007 named it through this census on 2026-09-29).
                // Parabola / hyperbola / surface-pair inputs are left to the
                // census's unmatched count until a case names them.
                let Some(d) = carried_residual(&e.curve, p, q) else {
                    continue;
                };
                if best.as_ref().is_none_or(|(_, bd)| d < *bd) {
                    best = Some((e.curve, d));
                }
            }
        }
    }
    best
}

/// Short kind tag for census rows.
fn surface_kind(s: &Surface) -> &'static str {
    match s {
        Surface::Plane { .. } => "Plane",
        Surface::Cylinder { .. } => "Cylinder",
        Surface::Sphere { .. } => "Sphere",
        Surface::Cone { .. } => "Cone",
        Surface::Torus { .. } => "Torus",
    }
}

pub(crate) fn curve_kind(c: &Curve) -> &'static str {
    match c {
        Curve::LineSegment => "LineSegment",
        Curve::Circle { .. } => "Circle",
        Curve::Ellipse { .. } => "Ellipse",
        Curve::Parabola { .. } => "Parabola",
        Curve::Hyperbola { .. } => "Hyperbola",
        Curve::SurfacePair { .. } => "SurfacePair",
    }
}

/// `YANG_434_OUT=census`: read-only census of the untyped cross-input seam
/// chords in the emitted topology. One row per chord above the noise floor,
/// plus a per-boolean summary. Never mutates anything.
pub(crate) fn census_output_pair_chords(
    verts: &[Point3],
    edges: &[BRepEdge],
    faces: &[BRepFace],
    face_attribution: &[TriangleAttribution],
    intersection_curves: &BTreeMap<(u32, u32), Curve>,
    input_a: (&[BRepFace], &[BRepEdge]),
    input_b: (&[BRepFace], &[BRepEdge]),
) {
    if !matches!(std::env::var("YANG_434_OUT"), Ok(v) if v == "census") {
        return;
    }
    // Undirected vertex-pair key → the loop-edge occurrences carrying it.
    let mut occ: BTreeMap<(u32, u32), Vec<(usize, usize)>> = BTreeMap::new();
    for (fi, f) in faces.iter().enumerate() {
        for lp in std::iter::once(&f.outer_loop).chain(f.inner_loops.iter()) {
            for &ei in lp {
                let Some(e) = edges.get(ei as usize) else {
                    continue;
                };
                if !matches!(e.curve, Curve::LineSegment) {
                    continue;
                }
                let key = (e.start.min(e.end), e.start.max(e.end));
                occ.entry(key).or_default().push((fi, ei as usize));
            }
        }
    }
    let mut n_seam = 0usize;
    let mut n_nonconv = 0usize;
    let mut n_deep = 0usize;
    let mut n_deep_matched = 0usize;
    let mut worst_match_resid = 0.0f64;
    let mut max_sag = 0.0f64;
    let mut max_end_res = 0.0f64;
    let mut per_pair: BTreeMap<(&'static str, &'static str), (usize, f64)> = BTreeMap::new();
    let mut scale = 0.0f64;
    for p in verts {
        let q = p.as_array();
        scale = scale.max(q[0].abs()).max(q[1].abs()).max(q[2].abs());
    }
    let floor = cad_primitives::TAU_MODEL * (1.0 + scale);
    for (key, uses) in &occ {
        if uses.len() != 2 {
            continue;
        }
        let (fa, fb) = (uses[0].0, uses[1].0);
        if fa == fb {
            continue;
        }
        let (ia, ib) = (face_attribution[fa].input, face_attribution[fb].input);
        n_seam += 1;
        let (s0, s1) = (faces[fa].surface, faces[fb].surface);
        // Same GEOMETRY (bit-identical surface params) ⇒ a mesh-seam chord
        // inside one analytic surface: depth = plain distance to it. Distinct
        // surfaces ⇒ the chord claims the pair curve: depth = pair-Newton
        // projection distance.
        let same_geom = s0 == s1;
        let class = if ia != ib {
            "cross"
        } else if same_geom {
            "same-surface"
        } else {
            "same-input"
        };
        let (p, q) = (verts[key.0 as usize], verts[key.1 as usize]);
        let (pa, qa) = (p.as_array(), q.as_array());
        let mid = Point3::new(
            (pa[0] + qa[0]) / 2.0,
            (pa[1] + qa[1]) / 2.0,
            (pa[2] + qa[2]) / 2.0,
        );
        // Endpoint residuals: the max |signed distance| to the claimed
        // carrier(s) over both endpoints — the chain's own quality (GIGO
        // guard: refinement cannot repair off-curve vertices).
        let end_res = |x: Point3| -> f64 {
            let x = x.as_array();
            let r0 = surface_distance_and_normal(s0, x).map_or(f64::NAN, |(d, _)| d.abs());
            if same_geom {
                return r0;
            }
            let r1 = surface_distance_and_normal(s1, x).map_or(f64::NAN, |(d, _)| d.abs());
            r0.max(r1)
        };
        let e_res = end_res(p).max(end_res(q));
        max_end_res = max_end_res.max(e_res);
        let (kind0, kind1) = (surface_kind(&s0), surface_kind(&s1));
        let pk = if kind0 <= kind1 {
            (kind0, kind1)
        } else {
            (kind1, kind0)
        };
        let sag = if same_geom {
            surface_distance_and_normal(s0, mid.as_array()).map_or_else(
                || {
                    n_nonconv += 1;
                    f64::NAN
                },
                |(d, _)| d.abs(),
            )
        } else {
            match relocate_onto_implicit_pair(mid, s0, s1) {
                Some(m) => {
                    let (m, c) = (m.as_array(), mid.as_array());
                    ((m[0] - c[0]).powi(2) + (m[1] - c[1]).powi(2) + (m[2] - c[2]).powi(2)).sqrt()
                }
                None => {
                    n_nonconv += 1;
                    f64::NAN
                }
            }
        };
        let entry = per_pair.entry(pk).or_insert((0, 0.0));
        entry.0 += 1;
        if sag.is_finite() {
            entry.1 = entry.1.max(sag);
            max_sag = max_sag.max(sag);
        }
        if sag.is_finite() && sag > floor {
            n_deep += 1;
            let map_status = intersection_curves
                .get(key)
                .map_or("none", |c| curve_kind(c));
            // Carried-edge hypothesis: does an INPUT edge of the two
            // attributed input faces carry this chord?
            let (afa, afb) = (&face_attribution[fa], &face_attribution[fb]);
            let matched = if ia == ib && afa.face != afb.face {
                let input = match ia {
                    InputId::A => input_a,
                    InputId::B => input_b,
                };
                carried_curve_match(input, afa.face, afb.face, p, q)
            } else {
                None
            };
            let match_desc = match &matched {
                Some((c, d)) => {
                    if *d <= floor {
                        n_deep_matched += 1;
                        worst_match_resid = worst_match_resid.max(*d);
                    }
                    format!("{}:{d:.3e}", curve_kind(c))
                }
                None => "none".to_string(),
            };
            eprintln!(
                "[s434-out] chord v{}–v{} class={class} faces=({fa},{fb}) \
                 surfs=({kind0},{kind1}) len={:.6e} sag={sag:.6e} end_res={e_res:.3e} \
                 map={map_status} carried={match_desc}",
                key.0,
                key.1,
                ((qa[0] - pa[0]).powi(2) + (qa[1] - pa[1]).powi(2) + (qa[2] - pa[2]).powi(2))
                    .sqrt(),
            );
        }
    }
    let pair_summary: Vec<String> = per_pair
        .iter()
        .map(|((a, b), (n, mx))| format!("{a}x{b}:n={n},max_sag={mx:.3e}"))
        .collect();
    eprintln!(
        "[s434-out] SUMMARY seam_chords={n_seam} nonconv={n_nonconv} max_sag={max_sag:.6e} \
         max_end_res={max_end_res:.3e} floor={floor:.3e} deep={n_deep} \
         deep_carried_matched={n_deep_matched} worst_match_resid={worst_match_resid:.3e} \
         pairs=[{}]",
        pair_summary.join(" ")
    );
}

// =========================================================================
// inc-1 (revised by the inc-0/census measurement, 2026-08-24): carried-edge
// curve RESTORATION.
//
// The census measured every facet-deep output chord in the F2a cases to be a
// carried INPUT edge — the rim circles between adjacent same-input faces
// (worst input-circle match residual 1.0e-11 vs a 4.3e-5 band; 15077/15077
// deep chords matched) — NOT Stage-3 intersection polylines. The paper's
// owner is therefore §4.4.2's output shape ("the original boundary curves or
// the intersection curves", `refs/text/yang2025_hybrid_boolean.txt:592-605`):
// the output B-Rep must restore the input's own typed curve on same-input
// boundaries, exactly as `intersection_curves` types the cross-input ones.
//
// The pass re-TYPES eligible output edges in place (no vertex/loop/index
// mutation): a 2-use `LineSegment` edge whose two owner faces descend from
// the SAME input but DIFFERENT input faces, both endpoints on exactly ONE
// candidate input circle within the classify band, gets that circle oriented
// per copy. The always-on I5-1b merge then coalesces the typed runs into
// single arc edges (its own full certification), and kernel-v2 samples them
// at render density — the F2a depth family dissolves at its root.
// =========================================================================

/// Gate: the restoration is ALWAYS-ON (flipped 2026-08-26 after the two
/// blocker fixes — kernel-v2 conformal grid-aligned arc sampling for R0054
/// and the invariant-4 debug-tiering for F0085; spec
/// `yang_434_output_chord_refinement.md` inc-4/inc-5). `YANG_434_OUT=0`
/// or `off` restores the mesh-polyline emission; `census` stays read-only
/// measurement (no apply).
pub(crate) fn restore_gate_enabled() -> bool {
    match std::env::var("YANG_434_OUT") {
        Ok(v) => v != "0" && v != "off" && v != "census",
        Err(_) => true,
    }
}

#[derive(Default, Debug)]
pub(crate) struct RestoreStats {
    pub eligible: usize,
    pub typed_chords: usize,
    pub no_candidate: usize,
    pub declined_offcurve: usize,
    pub declined_ambiguous: usize,
    pub declined_sweep: usize,
    pub declined_midpoint: usize,
    /// Of `typed_chords`, those re-typed onto an `Ellipse` (P0007).
    pub typed_ellipses: usize,
}

/// Canonical dedup key for a candidate conic (normal sign-canonicalized;
/// the circle's normal is unit-normalized first, as the residual test
/// reads it). Two candidates share a key iff they are the same curve up
/// to the normal's sign.
fn conic_key(curve: &Curve) -> Vec<u64> {
    let canon_n = |n: [f64; 3]| -> [f64; 3] {
        let flip = match n.iter().find(|&&c| c != 0.0) {
            Some(&c) => c < 0.0,
            None => false,
        };
        if flip {
            [-n[0], -n[1], -n[2]]
        } else {
            n
        }
    };
    let mut k: Vec<u64> = Vec::with_capacity(13);
    match *curve {
        Curve::Circle {
            center,
            normal,
            radius,
        } => {
            let n = canon_n(crate::normalize3(normal.as_array()));
            let c = center.as_array();
            k.push(1);
            k.extend(c.iter().map(|x| x.to_bits()));
            k.extend(n.iter().map(|x| x.to_bits()));
            k.push(radius.to_bits());
        }
        Curve::Ellipse {
            center,
            normal,
            major_axis,
            major_radius,
            minor_radius,
        } => {
            let n = canon_n(normal.as_array());
            k.push(2);
            k.extend(center.as_array().iter().map(|x| x.to_bits()));
            k.extend(n.iter().map(|x| x.to_bits()));
            k.extend(major_axis.as_array().iter().map(|x| x.to_bits()));
            k.push(major_radius.to_bits());
            k.push(minor_radius.to_bits());
        }
        _ => k.push(0),
    }
    k
}

/// Re-type carried same-input boundary chords onto their input conics —
/// `Circle` (revolve/extrude rims) and, since 2026-09-29 (P0007), `Ellipse`
/// (the KV14 re-entered oblique plane × cylinder boundary: Stage 1 sampled
/// it as an ellipse chain, but the output emitted its chords straight and
/// the render sampler drew the planar side along the 3D chord while the
/// cylinder side followed its chart — a self-intersection at the chord's
/// sagitta, kernel-v2's `SelfIntersectingBooleanOutput`). Pure in-place
/// curve re-typing: never touches vertices, loop structure, or edge
/// indices — with the gate off (or nothing eligible) the topology is
/// byte-identical. The circle branch is unchanged bit-for-bit; the ellipse
/// branch runs the same certification (both endpoints in band, exactly one
/// candidate, sweep ≤ π/2, minor-arc midpoint on BOTH owner surfaces) in
/// the ellipse's own angular parameter.
pub(crate) fn restore_carried_edge_curves(
    verts: &[Point3],
    edges: &mut [BRepEdge],
    faces: &[BRepFace],
    face_attribution: &[TriangleAttribution],
    input_a: (&[BRepFace], &[BRepEdge]),
    input_b: (&[BRepFace], &[BRepEdge]),
) -> RestoreStats {
    let mut stats = RestoreStats::default();
    // Undirected vertex-pair key → loop-edge occurrences (LineSegment only).
    let mut occ: BTreeMap<(u32, u32), Vec<(usize, usize)>> = BTreeMap::new();
    for (fi, f) in faces.iter().enumerate() {
        for lp in std::iter::once(&f.outer_loop).chain(f.inner_loops.iter()) {
            for &ei in lp {
                let Some(e) = edges.get(ei as usize) else {
                    continue;
                };
                if !matches!(e.curve, Curve::LineSegment) {
                    continue;
                }
                let key = (e.start.min(e.end), e.start.max(e.end));
                occ.entry(key).or_default().push((fi, ei as usize));
            }
        }
    }
    for (key, uses) in &occ {
        if uses.len() != 2 {
            continue;
        }
        let (fa, fb) = (uses[0].0, uses[1].0);
        if fa == fb {
            continue;
        }
        let (attr_a, attr_b) = (&face_attribution[fa], &face_attribution[fb]);
        if attr_a.input != attr_b.input || attr_a.face == attr_b.face {
            continue;
        }
        stats.eligible += 1;
        let (s0, s1) = (faces[fa].surface, faces[fb].surface);
        let input = match attr_a.input {
            InputId::A => input_a,
            InputId::B => input_b,
        };
        let (p, q) = (verts[key.0 as usize], verts[key.1 as usize]);
        let coord = {
            let (pa, qa) = (p.as_array(), q.as_array());
            pa.iter()
                .chain(qa.iter())
                .fold(0.0f64, |m, &c| m.max(c.abs()))
        };
        // Distinct in-band candidate conics from the two input faces' loops
        // (the classify band the merge and from_yang import both use).
        let mut in_band: BTreeMap<Vec<u64>, Curve> = BTreeMap::new();
        let mut any_candidate = false;
        for fi in [attr_a.face, attr_b.face] {
            let Some(f) = input.0.get(fi as usize) else {
                continue;
            };
            for lp in std::iter::once(&f.outer_loop).chain(f.inner_loops.iter()) {
                for &ei in lp {
                    let Some(e) = input.1.get(ei as usize) else {
                        continue;
                    };
                    let Some(d) = carried_residual(&e.curve, p, q) else {
                        continue;
                    };
                    any_candidate = true;
                    let scale = match e.curve {
                        Curve::Circle { radius, .. } => radius,
                        Curve::Ellipse { major_radius, .. } => major_radius,
                        _ => unreachable!("carried_residual admits conics only"),
                    };
                    if d <= cad_primitives::TAU_EVAL * (1.0 + scale.max(coord)) {
                        in_band.insert(conic_key(&e.curve), e.curve);
                    }
                }
            }
        }
        if in_band.is_empty() {
            if any_candidate {
                stats.declined_offcurve += 1;
            } else {
                stats.no_candidate += 1;
            }
            continue;
        }
        if in_band.len() > 1 {
            stats.declined_ambiguous += 1;
            continue;
        }
        let conic = *in_band.values().next().expect("len == 1");
        // Ellipse arm (P0007): sweep guard and minor-arc midpoint DOMAIN
        // certification in the ellipse's own angular parameter, the same
        // two rules the circle branch below applies in its frame.
        if let Curve::Ellipse {
            center,
            normal,
            major_axis,
            major_radius,
            minor_radius,
        } = conic
        {
            let param = |x: Point3| {
                crate::geom::ellipse_param(
                    x,
                    center,
                    normal,
                    major_axis,
                    major_radius,
                    minor_radius,
                )
            };
            let (t0, t1) = (param(p), param(q));
            let mut dt = t1 - t0;
            while dt > std::f64::consts::PI {
                dt -= 2.0 * std::f64::consts::PI;
            }
            while dt <= -std::f64::consts::PI {
                dt += 2.0 * std::f64::consts::PI;
            }
            if dt.abs() > std::f64::consts::FRAC_PI_2 {
                stats.declined_sweep += 1;
                continue;
            }
            let Some(mid) = crate::geom::conic_eval(&conic, t0 + 0.5 * dt) else {
                stats.declined_sweep += 1;
                continue;
            };
            let band = cad_primitives::TAU_EVAL * (1.0 + major_radius.max(coord));
            let on = |surf| {
                surface_distance_and_normal(surf, mid.as_array())
                    .is_some_and(|(d, _)| d.abs() <= band)
            };
            if !(on(s0) && on(s1)) {
                stats.declined_midpoint += 1;
                continue;
            }
            for &(_, ei) in uses {
                let (s, e) = (edges[ei].start, edges[ei].end);
                edges[ei].curve = crate::stage5_topology::orient_directed_curve(conic, s, e, verts);
            }
            stats.typed_chords += 1;
            stats.typed_ellipses += 1;
            continue;
        }
        let circle = conic;
        // Sweep guard: a mesh chord subtends a small arc; anything past π/2
        // is not this family (and approaches the minor-side ambiguity).
        // Then the DOMAIN certification: the restored arc must lie on BOTH
        // owner faces' surfaces, checked at the minor-arc midpoint. The
        // endpoints-on-circle test alone is necessary but NOT sufficient —
        // a STRAIGHT carried edge whose two endpoints happen to lie on one
        // rim circle (a chord line of the rim; measured on R0063's
        // micro-scale gear) would otherwise be re-typed as an arc bulging
        // out of both faces (the circle is edge-on to the planar owner:
        // arc-plane · face-normal ≈ 0.0005 at the anchor).
        {
            let Curve::Circle { center, normal, .. } = circle else {
                unreachable!("in_band holds circles and ellipses only; ellipses returned above");
            };
            let n = crate::normalize3(normal.as_array());
            let c = center.as_array();
            let proj = |x: Point3| -> [f64; 3] {
                let x = x.as_array();
                let d = [x[0] - c[0], x[1] - c[1], x[2] - c[2]];
                let h = d[0] * n[0] + d[1] * n[1] + d[2] * n[2];
                [d[0] - h * n[0], d[1] - h * n[1], d[2] - h * n[2]]
            };
            let (u, v) = (proj(p), proj(q));
            let (lu, lv) = (
                (u[0] * u[0] + u[1] * u[1] + u[2] * u[2]).sqrt(),
                (v[0] * v[0] + v[1] * v[1] + v[2] * v[2]).sqrt(),
            );
            let cosang = if lu > 0.0 && lv > 0.0 {
                ((u[0] * v[0] + u[1] * v[1] + u[2] * v[2]) / (lu * lv)).clamp(-1.0, 1.0)
            } else {
                -1.0
            };
            if cosang.acos() > std::f64::consts::FRAC_PI_2 {
                stats.declined_sweep += 1;
                continue;
            }
            // Minor-arc midpoint: bisect the in-plane directions and put the
            // point back on the circle at the stored radius.
            let Curve::Circle { radius, .. } = circle else {
                unreachable!("the ellipse arm returned above");
            };
            let bis = [
                u[0] / lu + v[0] / lv,
                u[1] / lu + v[1] / lv,
                u[2] / lu + v[2] / lv,
            ];
            let bl = (bis[0] * bis[0] + bis[1] * bis[1] + bis[2] * bis[2]).sqrt();
            if bl <= 0.0 {
                stats.declined_sweep += 1;
                continue;
            }
            let mid_arc = [
                c[0] + radius * bis[0] / bl,
                c[1] + radius * bis[1] / bl,
                c[2] + radius * bis[2] / bl,
            ];
            let band = cad_primitives::TAU_EVAL * (1.0 + radius.max(coord));
            let on = |surf| {
                surface_distance_and_normal(surf, mid_arc).is_some_and(|(d, _)| d.abs() <= band)
            };
            if !(on(s0) && on(s1)) {
                stats.declined_midpoint += 1;
                continue;
            }
        }
        for &(_, ei) in uses {
            let (s, e) = (edges[ei].start, edges[ei].end);
            edges[ei].curve = crate::stage5_topology::orient_directed_curve(circle, s, e, verts);
        }
        stats.typed_chords += 1;
    }
    stats
}
