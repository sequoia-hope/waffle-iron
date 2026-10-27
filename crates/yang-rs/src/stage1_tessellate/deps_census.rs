//! Stage-1 `d_ε` and `d(T)` census & B-Rep model AABB helpers (Roadmap F5:
//! N85 `d_ε` from model bounding box, N93 Stage-1 `d(T)` / `d_ε` certificate;
//! spec `specs/yang_41_deps_from_bbox.md`).
//!
//! Paper references (`refs/text/yang2025_hybrid_boolean.txt`):
//! - §4.1.1 (`:391-395`): `d_ε = 10⁻² · d`, where `d` is the diagonal length of
//!   the Axis-Aligned Bounding Box (AABB) of the B-Rep model.
//! - §4.1.2 / Fig. 6 (`:354-390`) and Appendix A Theorem A.1 (`:1163-1185`):
//!   `d_ε(T) = d_ε` if `T` is an inner triangle (contains no boundary sampling
//!   points), and `d(T)` otherwise.

use crate::geom::{ellipse_frame, parabola_param};
#[allow(clippy::wildcard_imports)]
use crate::*;
use cad_primitives::{Point2, TAU_MODEL};
use std::f64::consts::{PI, TAU};

#[inline]
fn expand_pt(lo: &mut [f64; 3], hi: &mut [f64; 3], p: [f64; 3]) {
    if p[0].is_finite() && p[1].is_finite() && p[2].is_finite() {
        for k in 0..3 {
            lo[k] = lo[k].min(p[k]);
            hi[k] = hi[k].max(p[k]);
        }
    }
}

/// Unwrap three angles on `S¹` by opening the circle at the largest circular
/// gap between sorted angles in `[0, 2π)`. Pure, symmetric in the input set,
/// preserves every angle modulo `2π`, and guarantees covering span `≤ 4π/3 < 2π`.
pub(crate) fn unwrap_circle_triple(raw: [f64; 3]) -> [f64; 3] {
    let mut indexed = [
        (0usize, raw[0].rem_euclid(TAU)),
        (1usize, raw[1].rem_euclid(TAU)),
        (2usize, raw[2].rem_euclid(TAU)),
    ];
    indexed.sort_by(|a, b| a.1.total_cmp(&b.1));
    let (s0, s1, s2) = (indexed[0].1, indexed[1].1, indexed[2].1);
    let g0 = s1 - s0;
    let g1 = s2 - s1;
    let g2 = TAU + s0 - s2;
    let mut out = [0.0_f64; 3];
    if g2 >= g0 && g2 >= g1 {
        out[indexed[0].0] = s0;
        out[indexed[1].0] = s1;
        out[indexed[2].0] = s2;
    } else if g0 >= g1 {
        out[indexed[0].0] = s0 + TAU;
        out[indexed[1].0] = s1;
        out[indexed[2].0] = s2;
    } else {
        out[indexed[0].0] = s0 + TAU;
        out[indexed[1].0] = s1 + TAU;
        out[indexed[2].0] = s2;
    }
    out
}

/// Exact Axis-Aligned Bounding Box `(lo, hi)` of a B-Rep model `(verts, edges, faces)`
/// (Yang et al. 2025 §4.1.1, `refs/text/yang2025_hybrid_boolean.txt:391-395`).
///
/// Closed-form over every analytic primitive in `yang-rs`:
/// - All B-Rep vertices.
/// - Every curve edge (`LineSegment`, full/arc `Circle`, full/arc `Ellipse`,
///   `Parabola`, `Hyperbola`, and projected `SurfacePair` chord bisection).
/// - Doubly-curved faces that bulge past the convex hull of their boundary
///   loops (`Sphere`, closed/structured/patch `Torus`). Ruled faces (`Plane`,
///   `Cylinder`, `Cone`) lie inside the convex hull of their boundary curves
///   because every generator segment connects two boundary points.
pub(crate) fn brep_model_aabb(
    verts: &[BRepVertex],
    edges: &[BRepEdge],
    faces: &[BRepFace],
) -> Option<([f64; 3], [f64; 3])> {
    let mut lo = [f64::INFINITY; 3];
    let mut hi = [f64::NEG_INFINITY; 3];

    for v in verts {
        expand_pt(&mut lo, &mut hi, v.point.as_array());
    }

    for e in edges {
        let p0 = verts.get(e.start as usize).map(|v| v.point);
        let p1 = verts.get(e.end as usize).map(|v| v.point);
        match e.curve {
            Curve::LineSegment => {}
            Curve::Circle {
                center,
                normal,
                radius,
            } => {
                if !radius.is_finite() || radius <= 0.0 {
                    continue;
                }
                let nu = normalize3(normal.as_array());
                let c = center.as_array();
                if e.start == e.end {
                    for k in 0..3 {
                        let span = radius * (1.0 - nu[k] * nu[k]).max(0.0).sqrt();
                        lo[k] = lo[k].min(c[k] - span);
                        hi[k] = hi[k].max(c[k] + span);
                    }
                } else if let (Some(q0), Some(q1)) = (p0, p1) {
                    let (e1v, e2v) = ortho_basis(normal);
                    let (e1, e2) = (e1v.as_array(), e2v.as_array());
                    let angle_of = |q: Point3| -> f64 {
                        let qa = q.as_array();
                        let w = [qa[0] - c[0], qa[1] - c[1], qa[2] - c[2]];
                        let wx = w[0] * e1[0] + w[1] * e1[1] + w[2] * e1[2];
                        let wy = w[0] * e2[0] + w[1] * e2[1] + w[2] * e2[2];
                        wy.atan2(wx)
                    };
                    let phi0 = angle_of(q0);
                    let phi1 = angle_of(q1);
                    let sweep = (phi1 - phi0).rem_euclid(TAU);
                    for k in 0..3 {
                        let base = e2[k].atan2(e1[k]);
                        for cand in [base, base + PI] {
                            let off = (cand - phi0).rem_euclid(TAU);
                            if off > 0.0 && off < sweep {
                                let (st, ct) = cand.sin_cos();
                                expand_pt(
                                    &mut lo,
                                    &mut hi,
                                    [
                                        c[0] + radius * (ct * e1[0] + st * e2[0]),
                                        c[1] + radius * (ct * e1[1] + st * e2[1]),
                                        c[2] + radius * (ct * e1[2] + st * e2[2]),
                                    ],
                                );
                            }
                        }
                    }
                }
            }
            Curve::Ellipse {
                center,
                normal,
                major_axis,
                major_radius,
                minor_radius,
            } => {
                if !major_radius.is_finite()
                    || !minor_radius.is_finite()
                    || major_radius <= 0.0
                    || minor_radius <= 0.0
                {
                    continue;
                }
                let c = center.as_array();
                let maj = normalize3(major_axis.as_array());
                let mindir = ellipse_frame(normal, major_axis);
                if e.start == e.end {
                    for k in 0..3 {
                        let span = (major_radius * maj[k]).hypot(minor_radius * mindir[k]);
                        lo[k] = lo[k].min(c[k] - span);
                        hi[k] = hi[k].max(c[k] + span);
                    }
                } else if let (Some(q0), Some(q1)) = (p0, p1) {
                    let t0 =
                        ellipse_param(q0, center, normal, major_axis, major_radius, minor_radius);
                    let t1 =
                        ellipse_param(q1, center, normal, major_axis, major_radius, minor_radius);
                    let sweep = (t1 - t0).rem_euclid(TAU);
                    for k in 0..3 {
                        let base = (minor_radius * mindir[k]).atan2(major_radius * maj[k]);
                        for cand in [base, base + PI] {
                            let off = (cand - t0).rem_euclid(TAU);
                            if off > 0.0 && off < sweep {
                                expand_pt(
                                    &mut lo,
                                    &mut hi,
                                    ellipse_point(
                                        center,
                                        normal,
                                        major_axis,
                                        major_radius,
                                        minor_radius,
                                        cand,
                                    )
                                    .as_array(),
                                );
                            }
                        }
                    }
                }
            }
            Curve::Parabola {
                vertex,
                normal,
                axis_dir,
                focal_length,
            } => {
                if !focal_length.is_finite() || focal_length <= 0.0 {
                    continue;
                }
                if let (Some(q0), Some(q1)) = (p0, p1) {
                    let t0 = parabola_param(q0, vertex, normal, axis_dir);
                    let t1 = parabola_param(q1, vertex, normal, axis_dir);
                    let (t_lo, t_hi) = (t0.min(t1), t0.max(t1));
                    let ax = axis_dir.as_array();
                    let n = normal.as_array();
                    let conj = [
                        n[1] * ax[2] - n[2] * ax[1],
                        n[2] * ax[0] - n[0] * ax[2],
                        n[0] * ax[1] - n[1] * ax[0],
                    ];
                    for k in 0..3 {
                        if ax[k].abs() > 0.0 {
                            let t_star = -2.0 * focal_length * conj[k] / ax[k];
                            if t_star.is_finite() && t_star > t_lo && t_star < t_hi {
                                expand_pt(
                                    &mut lo,
                                    &mut hi,
                                    parabola_point(vertex, normal, axis_dir, focal_length, t_star)
                                        .as_array(),
                                );
                            }
                        }
                    }
                }
            }
            Curve::Hyperbola {
                center,
                normal,
                major_axis,
                semi_transverse,
                semi_conjugate,
            } => {
                if !semi_transverse.is_finite()
                    || !semi_conjugate.is_finite()
                    || semi_transverse <= 0.0
                    || semi_conjugate <= 0.0
                {
                    continue;
                }
                if let (Some(q0), Some(q1)) = (p0, p1) {
                    let t0 = hyperbola_param(q0, center, normal, major_axis, semi_conjugate);
                    let t1 = hyperbola_param(q1, center, normal, major_axis, semi_conjugate);
                    let (t_lo, t_hi) = (t0.min(t1), t0.max(t1));
                    let maj = major_axis.as_array();
                    let n = normal.as_array();
                    let conj = [
                        n[1] * maj[2] - n[2] * maj[1],
                        n[2] * maj[0] - n[0] * maj[2],
                        n[0] * maj[1] - n[1] * maj[0],
                    ];
                    for k in 0..3 {
                        let ak = semi_transverse * maj[k];
                        let bk = semi_conjugate * conj[k];
                        if bk.abs() < ak.abs() {
                            let t_star = (-bk / ak).atanh();
                            if t_star.is_finite() && t_star > t_lo && t_star < t_hi {
                                expand_pt(
                                    &mut lo,
                                    &mut hi,
                                    hyperbola_point(
                                        center,
                                        normal,
                                        major_axis,
                                        semi_transverse,
                                        semi_conjugate,
                                        t_star,
                                    )
                                    .as_array(),
                                );
                            }
                        }
                    }
                }
            }
            Curve::SurfacePair { a, b } => {
                if e.start != e.end {
                    if let (Some(q0), Some(q1)) = (p0, p1) {
                        fn sample_pair_aabb(
                            a: Surface,
                            b: Surface,
                            pa: Point3,
                            pb: Point3,
                            depth: u32,
                            lo: &mut [f64; 3],
                            hi: &mut [f64; 3],
                        ) {
                            if depth == 0 {
                                return;
                            }
                            let (paa, pba) = (pa.as_array(), pb.as_array());
                            let mid = Point3::new(
                                0.5 * (paa[0] + pba[0]),
                                0.5 * (paa[1] + pba[1]),
                                0.5 * (paa[2] + pba[2]),
                            );
                            let Some(pm) = relocate_onto_implicit_pair(mid, a, b) else {
                                return;
                            };
                            let (pma, mida) = (pm.as_array(), mid.as_array());
                            let sag = ((pma[0] - mida[0]).powi(2)
                                + (pma[1] - mida[1]).powi(2)
                                + (pma[2] - mida[2]).powi(2))
                            .sqrt();
                            let chord = ((pba[0] - paa[0]).powi(2)
                                + (pba[1] - paa[1]).powi(2)
                                + (pba[2] - paa[2]).powi(2))
                            .sqrt();
                            if !sag.is_finite() || !chord.is_finite() || sag >= chord {
                                return;
                            }
                            expand_pt(lo, hi, pma);
                            if sag <= 1e-6 * chord.max(1.0) {
                                return;
                            }
                            sample_pair_aabb(a, b, pa, pm, depth - 1, lo, hi);
                            sample_pair_aabb(a, b, pm, pb, depth - 1, lo, hi);
                        }
                        sample_pair_aabb(a, b, q0, q1, 8, &mut lo, &mut hi);
                    }
                }
            }
        }
    }

    for f in faces {
        match f.surface {
            // Ruled surfaces: every compact face is contained in the convex
            // hull of its exact boundary curves (already accumulated above).
            Surface::Plane { .. } | Surface::Cylinder { .. } | Surface::Cone { .. } => {}
            // In yang-rs Stage 1, every Surface::Sphere face is a closed solid
            // sphere (`tessellate_sphere_face`).
            Surface::Sphere { center, radius } => {
                if radius.is_finite() && radius > 0.0 {
                    let c = center.as_array();
                    for k in 0..3 {
                        lo[k] = lo[k].min(c[k] - radius);
                        hi[k] = hi[k].max(c[k] + radius);
                    }
                }
            }
            Surface::Torus {
                center,
                axis_dir,
                major_radius,
                minor_radius,
            } => {
                if !major_radius.is_finite()
                    || !minor_radius.is_finite()
                    || minor_radius <= 0.0
                    || major_radius <= minor_radius
                {
                    continue;
                }
                let c = center.as_array();
                let ax = normalize3(axis_dir.as_array());
                let band = 1e-9 * (1.0 + major_radius + minor_radius);
                if !crate::stage1_tessellate::torus_face_takes_patch_path(
                    f,
                    edges,
                    major_radius,
                    minor_radius,
                ) {
                    let mut seam: Option<u32> = None;
                    let mut has_equator = false;
                    for &e_idx in &f.outer_loop {
                        if let Some(ed) = edges.get(e_idx as usize) {
                            if let Curve::Circle { radius, .. } = ed.curve {
                                if ed.start != ed.end {
                                    seam = Some(e_idx);
                                } else if (radius - (major_radius + minor_radius)).abs() <= band {
                                    has_equator = true;
                                }
                            }
                        }
                    }
                    if has_equator && seam.is_none() {
                        // Closed torus: exact half-span R·√(1 − â_k²) + r.
                        for k in 0..3 {
                            let span =
                                major_radius * (1.0 - ax[k] * ax[k]).max(0.0).sqrt() + minor_radius;
                            lo[k] = lo[k].min(c[k] - span);
                            hi[k] = hi[k].max(c[k] + span);
                        }
                    } else if let Some(seam_idx) = seam {
                        let seam_ed = &edges[seam_idx as usize];
                        if let (
                            Curve::Circle {
                                center: sc,
                                normal: sn,
                                ..
                            },
                            Some(q0),
                            Some(q1),
                        ) = (
                            seam_ed.curve,
                            verts.get(seam_ed.start as usize).map(|v| v.point),
                            verts.get(seam_ed.end as usize).map(|v| v.point),
                        ) {
                            let sca = sc.as_array();
                            let (s1v, s2v) = ortho_basis(sn);
                            let (s1, s2) = (s1v.as_array(), s2v.as_array());
                            let angle_of = |q: Point3| -> f64 {
                                let qa = q.as_array();
                                let w = [qa[0] - sca[0], qa[1] - sca[1], qa[2] - sca[2]];
                                let wx = w[0] * s1[0] + w[1] * s1[1] + w[2] * s1[2];
                                let wy = w[0] * s2[0] + w[1] * s2[1] + w[2] * s2[2];
                                wy.atan2(wx)
                            };
                            let phi0 = angle_of(q0);
                            let phi1 = angle_of(q1);
                            let sweep = (phi1 - phi0).rem_euclid(TAU);
                            for k in 0..3 {
                                let span = major_radius * (1.0 - ax[k] * ax[k]).max(0.0).sqrt()
                                    + minor_radius;
                                let theta_max = s2[k].atan2(s1[k]);
                                let theta_min = theta_max + PI;
                                let off_max = (theta_max - phi0).rem_euclid(TAU);
                                if off_max > 0.0 && off_max < sweep {
                                    hi[k] = hi[k].max(c[k] + span);
                                }
                                let off_min = (theta_min - phi0).rem_euclid(TAU);
                                if off_min > 0.0 && off_min < sweep {
                                    lo[k] = lo[k].min(c[k] - span);
                                }
                            }
                        }
                    }
                } else {
                    // Patch-path torus (poloidal/toroidal band with inner loops,
                    // closed torus with window loops, or UV disk patch): mirror
                    // `patch_tessellators::tessellate_torus_patch`'s oriented
                    // universal-cover unwrapping (`patch_tessellators.rs:980-1289`)
                    // so >180° bands and Slice F-4 holed closed tori recover the
                    // true face domain rather than the shorter complement arc.
                    let (e1v, e2v) = ortho_basis(axis_dir);
                    let (e1, e2) = (e1v.as_array(), e2v.as_array());
                    let sample_loop = |loop_edges: &[u32]| -> Option<Vec<[f64; 3]>> {
                        if loop_edges.is_empty() {
                            return None;
                        }
                        if loop_edges.len() == 1 {
                            let ed = edges.get(loop_edges[0] as usize)?;
                            if ed.start == ed.end {
                                match ed.curve {
                                    Curve::Circle {
                                        center: cc,
                                        normal: cn,
                                        radius,
                                    } => {
                                        let p0 = verts.get(ed.start as usize)?.point.as_array();
                                        let cca = cc.as_array();
                                        let (s1v, s2v) = ortho_basis(cn);
                                        let (s1, s2) = (s1v.as_array(), s2v.as_array());
                                        let w = [p0[0] - cca[0], p0[1] - cca[1], p0[2] - cca[2]];
                                        let wx = w[0] * s1[0] + w[1] * s1[1] + w[2] * s1[2];
                                        let wy = w[0] * s2[0] + w[1] * s2[1] + w[2] * s2[2];
                                        let a0 = wy.atan2(wx);
                                        let mut pts = Vec::with_capacity(16);
                                        for step in 0..16 {
                                            let a = a0 + TAU * (step as f64) / 16.0;
                                            let (sa, ca) = a.sin_cos();
                                            pts.push([
                                                cca[0] + radius * (ca * s1[0] + sa * s2[0]),
                                                cca[1] + radius * (ca * s1[1] + sa * s2[1]),
                                                cca[2] + radius * (ca * s1[2] + sa * s2[2]),
                                            ]);
                                        }
                                        return Some(pts);
                                    }
                                    Curve::Ellipse {
                                        center: ec,
                                        normal: en,
                                        major_axis,
                                        major_radius: er_maj,
                                        minor_radius: er_min,
                                    } => {
                                        let p0 = verts.get(ed.start as usize)?.point;
                                        let t0 =
                                            ellipse_param(p0, ec, en, major_axis, er_maj, er_min);
                                        let mut pts = Vec::with_capacity(16);
                                        for step in 0..16 {
                                            let t = t0 + TAU * (step as f64) / 16.0;
                                            pts.push(
                                                ellipse_point(
                                                    ec, en, major_axis, er_maj, er_min, t,
                                                )
                                                .as_array(),
                                            );
                                        }
                                        return Some(pts);
                                    }
                                    _ => return None,
                                }
                            }
                        }
                        for first_forward in [true, false] {
                            let e0 = edges.get(loop_edges[0] as usize)?;
                            let mut cur = if first_forward { e0.start } else { e0.end };
                            let origin = cur;
                            let mut pts: Vec<[f64; 3]> = Vec::new();
                            let mut ok = true;
                            for &e_idx in loop_edges {
                                let Some(ed) = edges.get(e_idx as usize) else {
                                    ok = false;
                                    break;
                                };
                                if ed.start == ed.end {
                                    ok = false;
                                    break;
                                }
                                let forward = if ed.start == cur {
                                    true
                                } else if ed.end == cur {
                                    false
                                } else {
                                    ok = false;
                                    break;
                                };
                                let v_orig = if forward { ed.start } else { ed.end };
                                let Some(p_start) = verts.get(ed.start as usize).map(|v| v.point)
                                else {
                                    ok = false;
                                    break;
                                };
                                let Some(p_end) = verts.get(ed.end as usize).map(|v| v.point)
                                else {
                                    ok = false;
                                    break;
                                };
                                let p_orig = verts[v_orig as usize].point;
                                pts.push(p_orig.as_array());
                                match ed.curve {
                                    Curve::Circle {
                                        center: cc,
                                        normal: cn,
                                        radius,
                                    } => {
                                        let cca = cc.as_array();
                                        let (s1v, s2v) = ortho_basis(cn);
                                        let (s1, s2) = (s1v.as_array(), s2v.as_array());
                                        let ang = |q: Point3| -> f64 {
                                            let qa = q.as_array();
                                            let w =
                                                [qa[0] - cca[0], qa[1] - cca[1], qa[2] - cca[2]];
                                            let wx = w[0] * s1[0] + w[1] * s1[1] + w[2] * s1[2];
                                            let wy = w[0] * s2[0] + w[1] * s2[1] + w[2] * s2[2];
                                            wy.atan2(wx)
                                        };
                                        let a0 = ang(p_start);
                                        let sw = (ang(p_end) - a0).rem_euclid(TAU);
                                        for step in 1..8 {
                                            let k = if forward { step } else { 8 - step };
                                            let a = a0 + sw * (k as f64) / 8.0;
                                            let (sa, ca) = a.sin_cos();
                                            pts.push([
                                                cca[0] + radius * (ca * s1[0] + sa * s2[0]),
                                                cca[1] + radius * (ca * s1[1] + sa * s2[1]),
                                                cca[2] + radius * (ca * s1[2] + sa * s2[2]),
                                            ]);
                                        }
                                    }
                                    Curve::Ellipse {
                                        center: ec,
                                        normal: en,
                                        major_axis,
                                        major_radius: er_maj,
                                        minor_radius: er_min,
                                    } => {
                                        let t0 = ellipse_param(
                                            p_start, ec, en, major_axis, er_maj, er_min,
                                        );
                                        let t1 = ellipse_param(
                                            p_end, ec, en, major_axis, er_maj, er_min,
                                        );
                                        let sw = (t1 - t0).rem_euclid(TAU);
                                        for step in 1..8 {
                                            let k = if forward { step } else { 8 - step };
                                            let t = t0 + sw * (k as f64) / 8.0;
                                            pts.push(
                                                ellipse_point(
                                                    ec, en, major_axis, er_maj, er_min, t,
                                                )
                                                .as_array(),
                                            );
                                        }
                                    }
                                    Curve::SurfacePair { a, b } => {
                                        let (pa, pb) = if forward {
                                            (p_start.as_array(), p_end.as_array())
                                        } else {
                                            (p_end.as_array(), p_start.as_array())
                                        };
                                        for step in 1..4 {
                                            let s = (step as f64) / 4.0;
                                            let guess = Point3::new(
                                                pa[0] + s * (pb[0] - pa[0]),
                                                pa[1] + s * (pb[1] - pa[1]),
                                                pa[2] + s * (pb[2] - pa[2]),
                                            );
                                            if let Some(pm) =
                                                relocate_onto_implicit_pair(guess, a, b)
                                            {
                                                pts.push(pm.as_array());
                                            }
                                        }
                                    }
                                    _ => {}
                                }
                                cur = if forward { ed.end } else { ed.start };
                            }
                            if ok && cur == origin && pts.len() >= 3 {
                                return Some(pts);
                            }
                        }
                        None
                    };

                    #[derive(Clone)]
                    struct TorusLoopUv {
                        u: Vec<f64>,
                        v: Vec<f64>,
                        wu: i64,
                        wv: i64,
                    }
                    let net_wrap = |a: &[f64]| -> i64 {
                        if a.len() < 2 {
                            return 0;
                        }
                        let raw_closure = a[0] - a[a.len() - 1];
                        let mut c_step = raw_closure % TAU;
                        if c_step > PI {
                            c_step -= TAU;
                        } else if c_step <= -PI {
                            c_step += TAU;
                        }
                        let net = (a[a.len() - 1] - a[0]) + c_step;
                        (net / TAU).round() as i64
                    };
                    let snap = |a: f64| (a / 1e-12).round() * 1e-12;
                    let project_loop = |pts: &[[f64; 3]]| -> TorusLoopUv {
                        let mut us = Vec::with_capacity(pts.len());
                        let mut vs = Vec::with_capacity(pts.len());
                        for &p in pts {
                            let w = [p[0] - c[0], p[1] - c[1], p[2] - c[2]];
                            let z = w[0] * ax[0] + w[1] * ax[1] + w[2] * ax[2];
                            let wx = w[0] * e1[0] + w[1] * e1[1] + w[2] * e1[2];
                            let wy = w[0] * e2[0] + w[1] * e2[1] + w[2] * e2[2];
                            let rho = wx.hypot(wy);
                            us.push(z.atan2(rho - major_radius));
                            vs.push(wy.atan2(wx));
                        }
                        patch_tessellators::unwrap_seq(&mut us);
                        patch_tessellators::unwrap_seq(&mut vs);
                        for a in us.iter_mut().chain(vs.iter_mut()) {
                            *a = snap(*a);
                        }
                        let wu = net_wrap(&us);
                        let wv = net_wrap(&vs);
                        TorusLoopUv {
                            u: us,
                            v: vs,
                            wu,
                            wv,
                        }
                    };

                    let mut loops_uv: Vec<TorusLoopUv> = Vec::new();
                    let mut all_loops_ok = false;
                    if let Some(outer_pts) = sample_loop(&f.outer_loop) {
                        for &pt in &outer_pts {
                            expand_pt(&mut lo, &mut hi, pt);
                        }
                        loops_uv.push(project_loop(&outer_pts));
                        all_loops_ok = true;
                        for inner in &f.inner_loops {
                            if let Some(inner_pts) = sample_loop(inner) {
                                for &pt in &inner_pts {
                                    expand_pt(&mut lo, &mut hi, pt);
                                }
                                loops_uv.push(project_loop(&inner_pts));
                            } else {
                                all_loops_ok = false;
                                break;
                            }
                        }
                    }

                    let mut full_toroidal = false;
                    let mut full_poloidal = false;
                    let mut theta_range: Option<(f64, f64)> = None;
                    let mut phi_range: Option<(f64, f64)> = None;

                    if all_loops_ok && !loops_uv.is_empty() {
                        let wrap_u: Vec<usize> = (0..loops_uv.len())
                            .filter(|&i| loops_uv[i].wu != 0)
                            .collect();
                        let wrap_v: Vec<usize> = (0..loops_uv.len())
                            .filter(|&i| loops_uv[i].wv != 0)
                            .collect();
                        let mean = |s: &[f64]| s.iter().sum::<f64>() / s.len() as f64;
                        let extent = |s: &[f64]| -> (f64, f64) {
                            s.iter()
                                .fold((f64::INFINITY, f64::NEG_INFINITY), |(a, b), &x| {
                                    (a.min(x), b.max(x))
                                })
                        };

                        if wrap_v.is_empty() && wrap_u.is_empty() {
                            // 0-wrapping loops: either a closed torus carrying
                            // window holes (`(area2 < 0) == f.reversed`, Slice F-4)
                            // or a bounded UV disk patch (Slice F-3).
                            let m = loops_uv[0].u.len();
                            let area2: f64 = (0..m)
                                .map(|k| {
                                    let j = (k + 1) % m;
                                    (loops_uv[0].u[k] * minor_radius)
                                        * (loops_uv[0].v[j] * major_radius)
                                        - (loops_uv[0].u[j] * minor_radius)
                                            * (loops_uv[0].v[k] * major_radius)
                                })
                                .sum();
                            if area2 != 0.0 && (area2 < 0.0) == f.reversed {
                                full_toroidal = true;
                                full_poloidal = true;
                                theta_range = Some((0.0, TAU));
                                phi_range = Some((0.0, TAU));
                            } else {
                                let (u_ref, v_ref) = (mean(&loops_uv[0].u), mean(&loops_uv[0].v));
                                let (mut u_lo, mut u_hi) = extent(&loops_uv[0].u);
                                let (mut v_lo, mut v_hi) = extent(&loops_uv[0].v);
                                for l in &loops_uv[1..] {
                                    let du = ((mean(&l.u) - u_ref) / TAU).round() * TAU;
                                    let dv = ((mean(&l.v) - v_ref) / TAU).round() * TAU;
                                    for &u in &l.u {
                                        u_lo = u_lo.min(u - du);
                                        u_hi = u_hi.max(u - du);
                                    }
                                    for &v in &l.v {
                                        v_lo = v_lo.min(v - dv);
                                        v_hi = v_hi.max(v - dv);
                                    }
                                }
                                phi_range = Some((u_lo, u_hi));
                                theta_range = Some((v_lo, v_hi));
                            }
                        } else if wrap_v.is_empty() && wrap_u.len() == 2 {
                            // Poloidal-wrapping band (`patch_tessellators.rs:1227-1289`):
                            // shift `mc` onto `pc`'s oriented side so >180° bands
                            // span their true longitude interval rather than the
                            // shorter complement.
                            full_poloidal = true;
                            phi_range = Some((0.0, TAU));
                            let (ia, ib) = (wrap_u[0], wrap_u[1]);
                            let (pc, mc) = if loops_uv[ia].wu > 0 {
                                (&loops_uv[ia], &loops_uv[ib])
                            } else {
                                (&loops_uv[ib], &loops_uv[ia])
                            };
                            let (v_pc, v_mc) = (mean(&pc.v), mean(&mc.v));
                            let t = (v_pc - v_mc) / TAU;
                            let k_shift = if f.reversed {
                                t.floor() + 1.0
                            } else {
                                t.ceil() - 1.0
                            };
                            let (mut v_lo, mut v_hi) = extent(&pc.v);
                            for &v in &mc.v {
                                let vs = v + k_shift * TAU;
                                v_lo = v_lo.min(vs);
                                v_hi = v_hi.max(vs);
                            }
                            let v_mid = 0.5 * (v_pc + v_mc + k_shift * TAU);
                            for (idx, l) in loops_uv.iter().enumerate() {
                                if idx == ia || idx == ib {
                                    continue;
                                }
                                let dv = ((mean(&l.v) - v_mid) / TAU).round() * TAU;
                                for &v in &l.v {
                                    v_lo = v_lo.min(v - dv);
                                    v_hi = v_hi.max(v - dv);
                                }
                            }
                            theta_range = Some((v_lo, v_hi));
                        } else if wrap_u.is_empty() && wrap_v.len() == 2 {
                            // Symmetric toroidal-wrapping band (`wv == ±1`, `wu == 0`).
                            full_toroidal = true;
                            theta_range = Some((0.0, TAU));
                            let (ia, ib) = (wrap_v[0], wrap_v[1]);
                            let (pv, mv) = if loops_uv[ia].wv > 0 {
                                (&loops_uv[ia], &loops_uv[ib])
                            } else {
                                (&loops_uv[ib], &loops_uv[ia])
                            };
                            let (u_pv, u_mv) = (mean(&pv.u), mean(&mv.u));
                            let t = (u_mv - u_pv) / TAU;
                            let k_shift = if f.reversed {
                                t.ceil() - 1.0
                            } else {
                                t.floor() + 1.0
                            };
                            let (mut u_lo, mut u_hi) = extent(&pv.u);
                            for &u in &mv.u {
                                let us = u - k_shift * TAU;
                                u_lo = u_lo.min(us);
                                u_hi = u_hi.max(us);
                            }
                            phi_range = Some((u_lo, u_hi));
                        }
                    }

                    if theta_range.is_none() || phi_range.is_none() {
                        // Fallback if a non-standard loop topology declined the
                        // oriented walk: cover the full torus conservatively.
                        full_toroidal = true;
                        full_poloidal = true;
                        theta_range = Some((0.0, TAU));
                        phi_range = Some((0.0, TAU));
                    }
                    if let (Some((th_lo, th_hi)), Some((ph_lo, ph_hi))) = (theta_range, phi_range) {
                        let in_arc = |ang: f64, lo_a: f64, hi_a: f64| -> Option<f64> {
                            if hi_a - lo_a >= TAU - 1e-12 {
                                return Some(ang);
                            }
                            let off = (ang - lo_a).rem_euclid(TAU);
                            (off <= hi_a - lo_a).then_some(lo_a + off)
                        };
                        let eval_torus = |th: f64, ph: f64| -> [f64; 3] {
                            let (sth, cth) = th.sin_cos();
                            let (sph, cph) = ph.sin_cos();
                            let rad = major_radius + minor_radius * cph;
                            let z = minor_radius * sph;
                            [
                                c[0] + rad * (cth * e1[0] + sth * e2[0]) + z * ax[0],
                                c[1] + rad * (cth * e1[1] + sth * e2[1]) + z * ax[1],
                                c[2] + rad * (cth * e1[2] + sth * e2[2]) + z * ax[2],
                            ]
                        };
                        for k in 0..3 {
                            let th_star = e2[k].atan2(e1[k]);
                            let mut th_cands = if full_toroidal {
                                vec![th_star, th_star + PI]
                            } else {
                                let mut v = vec![th_lo, th_hi];
                                for tc in [th_star, th_star + PI] {
                                    if let Some(t_in) = in_arc(tc, th_lo, th_hi) {
                                        v.push(t_in);
                                    }
                                }
                                v
                            };
                            th_cands.dedup_by(|a, b| (*a - *b).abs() <= 1e-15);
                            for th in th_cands {
                                let (sth, cth) = th.sin_cos();
                                let uk = cth * e1[k] + sth * e2[k];
                                let ph_star = ax[k].atan2(uk);
                                let mut ph_cands = if full_poloidal {
                                    vec![ph_star, ph_star + PI]
                                } else {
                                    let mut v = vec![ph_lo, ph_hi];
                                    for pc in [ph_star, ph_star + PI] {
                                        if let Some(p_in) = in_arc(pc, ph_lo, ph_hi) {
                                            v.push(p_in);
                                        }
                                    }
                                    v
                                };
                                ph_cands.dedup_by(|a, b| (*a - *b).abs() <= 1e-15);
                                for ph in ph_cands {
                                    expand_pt(&mut lo, &mut hi, eval_torus(th, ph));
                                }
                            }
                        }
                    }
                }
            }
        }
    }

    if (0..3).all(|k| lo[k].is_finite() && hi[k].is_finite() && hi[k] >= lo[k]) {
        Some((lo, hi))
    } else {
        None
    }
}

/// Yang et al. 2025 §4.1.1 (`refs/text/yang2025_hybrid_boolean.txt:391-395`):
/// `d_ε = 10⁻² · d`, where `d` is the space diagonal of the B-Rep model's AABB
/// (scaled by [`crate::stage1_tessellate::chord_rel`] so the debug-only
/// `YANG_CHORD_REFINE` measurement ladder scales one single source, A14.3).
pub(crate) fn brep_model_chord_bound(
    verts: &[BRepVertex],
    edges: &[BRepEdge],
    faces: &[BRepFace],
) -> Option<f64> {
    let (lo, hi) = brep_model_aabb(verts, edges, faces)?;
    let dx = hi[0] - lo[0];
    let dy = hi[1] - lo[1];
    let dz = hi[2] - lo[2];
    let diag = (dx * dx + dy * dy + dz * dz).sqrt();
    (diag.is_finite() && diag > 0.0).then(|| crate::stage1_tessellate::chord_rel() * diag)
}

/// Project a Stage-1 mesh triangle's three 3D corners onto `face_surface` in
/// [`crate::stage4_dt::d_of_t`]'s exact `(u, v)` convention (`stage4_dt.rs:58-63`).
///
/// Handles:
/// - Single-nappe orientation on `Surface::Cone`: if the face sits on the
///   `-axis_dir` nappe (`(p − apex)·axis_dir < 0`), returns the geometrically
///   identical `Surface::Cone { apex, axis_dir: -axis_dir, half_angle }` so
///   `v ≥ 0`.
/// - Coordinate singularities at the cone apex (`v = 0`) and sphere poles
///   (`v = ±π/2`), where `eval_uv` is independent of `u`: assigns an off-singular
///   corner's `u` so the covering rectangle `[u0, u1] × [v0, v1]` spans only
///   the triangle's true sector/lune.
/// - Largest-circular-gap unwrapping on `S¹` (`u` for Cylinder/Cone/Sphere/Torus,
///   and `v` for Torus), keeping covering spans `≤ 4π/3 < 2π`.
pub(crate) fn stage1_tri_surface_and_uv(
    face_surface: Surface,
    tri_pts: [Point3; 3],
) -> (Surface, [Point2; 3]) {
    let unwrap_with_singular = |raw_u: [f64; 3], singular: [bool; 3]| -> [f64; 3] {
        let valid: Vec<usize> = (0..3).filter(|&i| !singular[i]).collect();
        match valid.len() {
            3 => unwrap_circle_triple(raw_u),
            2 => {
                let (i0, i1) = (valid[0], valid[1]);
                let a0 = raw_u[i0].rem_euclid(TAU);
                let mut d = (raw_u[i1] - raw_u[i0]).rem_euclid(TAU);
                if d > PI {
                    d -= TAU;
                }
                let a1 = a0 + d;
                let mid = 0.5 * (a0 + a1);
                let mut out = [mid; 3];
                out[i0] = a0;
                out[i1] = a1;
                out
            }
            1 => [raw_u[valid[0]].rem_euclid(TAU); 3],
            _ => [0.0; 3],
        }
    };

    match face_surface {
        Surface::Plane { normal, d } => {
            let nu = normalize3(normal.as_array());
            let (e1v, e2v) = ortho_basis(normal);
            let (e1, e2) = (e1v.as_array(), e2v.as_array());
            let orig = [-d * nu[0], -d * nu[1], -d * nu[2]];
            let mut uv = [Point2::new(0.0, 0.0); 3];
            for k in 0..3 {
                let p = tri_pts[k].as_array();
                let w = [p[0] - orig[0], p[1] - orig[1], p[2] - orig[2]];
                let u = w[0] * e1[0] + w[1] * e1[1] + w[2] * e1[2];
                let v = w[0] * e2[0] + w[1] * e2[1] + w[2] * e2[2];
                uv[k] = Point2::new(u, v);
            }
            (face_surface, uv)
        }
        Surface::Cylinder {
            axis_point,
            axis_dir,
            ..
        } => {
            let au = normalize3(axis_dir.as_array());
            let (e1v, e2v) = ortho_basis(axis_dir);
            let (e1, e2) = (e1v.as_array(), e2v.as_array());
            let ap = axis_point.as_array();
            let mut raw_u = [0.0; 3];
            let mut raw_v = [0.0; 3];
            for k in 0..3 {
                let p = tri_pts[k].as_array();
                let w = [p[0] - ap[0], p[1] - ap[1], p[2] - ap[2]];
                raw_v[k] = w[0] * au[0] + w[1] * au[1] + w[2] * au[2];
                let wx = w[0] * e1[0] + w[1] * e1[1] + w[2] * e1[2];
                let wy = w[0] * e2[0] + w[1] * e2[1] + w[2] * e2[2];
                raw_u[k] = wy.atan2(wx);
            }
            let u = unwrap_circle_triple(raw_u);
            (
                face_surface,
                [
                    Point2::new(u[0], raw_v[0]),
                    Point2::new(u[1], raw_v[1]),
                    Point2::new(u[2], raw_v[2]),
                ],
            )
        }
        Surface::Cone {
            apex,
            axis_dir,
            half_angle,
        } => {
            let ap = apex.as_array();
            let ap_scale = 1.0 + ap[0].abs().max(ap[1].abs()).max(ap[2].abs());
            let au0 = normalize3(axis_dir.as_array());
            let mut max_abs_h = 0.0_f64;
            let mut dominant_h = 0.0_f64;
            for pt in &tri_pts {
                let p = pt.as_array();
                let w = [p[0] - ap[0], p[1] - ap[1], p[2] - ap[2]];
                let h = w[0] * au0[0] + w[1] * au0[1] + w[2] * au0[2];
                if h.abs() > max_abs_h {
                    max_abs_h = h.abs();
                    dominant_h = h;
                }
            }
            let eff_axis = if dominant_h < -TAU_MODEL * ap_scale {
                Vector3::new(-axis_dir.x(), -axis_dir.y(), -axis_dir.z())
            } else {
                axis_dir
            };
            let eff_surface = Surface::Cone {
                apex,
                axis_dir: eff_axis,
                half_angle,
            };
            let au = normalize3(eff_axis.as_array());
            let (e1v, e2v) = ortho_basis(eff_axis);
            let (e1, e2) = (e1v.as_array(), e2v.as_array());
            let mut raw_u = [0.0; 3];
            let mut raw_v = [0.0; 3];
            let mut is_apex = [false; 3];
            let tol = TAU_MODEL * ap_scale;
            for k in 0..3 {
                let p = tri_pts[k].as_array();
                let w = [p[0] - ap[0], p[1] - ap[1], p[2] - ap[2]];
                let h = w[0] * au[0] + w[1] * au[1] + w[2] * au[2];
                let wx = w[0] * e1[0] + w[1] * e1[1] + w[2] * e1[2];
                let wy = w[0] * e2[0] + w[1] * e2[1] + w[2] * e2[2];
                let rho = wx.hypot(wy);
                if rho <= tol && h.abs() <= tol {
                    is_apex[k] = true;
                    raw_v[k] = 0.0;
                } else {
                    raw_u[k] = wy.atan2(wx);
                    raw_v[k] = h;
                }
            }
            let u = unwrap_with_singular(raw_u, is_apex);
            (
                eff_surface,
                [
                    Point2::new(u[0], raw_v[0]),
                    Point2::new(u[1], raw_v[1]),
                    Point2::new(u[2], raw_v[2]),
                ],
            )
        }
        Surface::Sphere { center, radius } => {
            let z_axis = Vector3::new(0.0, 0.0, 1.0);
            let (e1v, e2v) = ortho_basis(z_axis);
            let (e1, e2) = (e1v.as_array(), e2v.as_array());
            let cen = center.as_array();
            let pole_tol = TAU_MODEL * (1.0 + radius.abs());
            let mut raw_u = [0.0; 3];
            let mut raw_v = [0.0; 3];
            let mut is_pole = [false; 3];
            for k in 0..3 {
                let p = tri_pts[k].as_array();
                let w = [p[0] - cen[0], p[1] - cen[1], p[2] - cen[2]];
                let wx = w[0] * e1[0] + w[1] * e1[1] + w[2] * e1[2];
                let wy = w[0] * e2[0] + w[1] * e2[1] + w[2] * e2[2];
                let rho = wx.hypot(wy);
                raw_v[k] = w[2].atan2(rho);
                if rho <= pole_tol {
                    is_pole[k] = true;
                } else {
                    raw_u[k] = wy.atan2(wx);
                }
            }
            let u = unwrap_with_singular(raw_u, is_pole);
            (
                face_surface,
                [
                    Point2::new(u[0], raw_v[0]),
                    Point2::new(u[1], raw_v[1]),
                    Point2::new(u[2], raw_v[2]),
                ],
            )
        }
        Surface::Torus {
            center,
            axis_dir,
            major_radius,
            ..
        } => {
            let au = normalize3(axis_dir.as_array());
            let (e1v, e2v) = ortho_basis(axis_dir);
            let (e1, e2) = (e1v.as_array(), e2v.as_array());
            let cen = center.as_array();
            let mut raw_u = [0.0; 3];
            let mut raw_v = [0.0; 3];
            for k in 0..3 {
                let p = tri_pts[k].as_array();
                let w = [p[0] - cen[0], p[1] - cen[1], p[2] - cen[2]];
                let z = w[0] * au[0] + w[1] * au[1] + w[2] * au[2];
                let wx = w[0] * e1[0] + w[1] * e1[1] + w[2] * e1[2];
                let wy = w[0] * e2[0] + w[1] * e2[1] + w[2] * e2[2];
                let rho = wx.hypot(wy);
                raw_u[k] = wy.atan2(wx);
                raw_v[k] = z.atan2(rho - major_radius);
            }
            let u = unwrap_circle_triple(raw_u);
            let v = unwrap_circle_triple(raw_v);
            (
                face_surface,
                [
                    Point2::new(u[0], v[0]),
                    Point2::new(u[1], v[1]),
                    Point2::new(u[2], v[2]),
                ],
            )
        }
    }
}

fn fmt_opt(v: Option<f64>) -> String {
    match v {
        Some(x) if x.is_finite() => format!("{x:.12e}"),
        _ => "null".to_string(),
    }
}

/// Print-only Stage-1 `d_ε` and `d(T)` census probe (`YANG_S1_DEPS_CENSUS=<file>`,
/// registered in `docs/kernel_probe_registry.md`). Appends one JSONL record per
/// initial operand `BRep::new` construction when the env var is set; immediate
/// no-op otherwise.
pub(crate) fn maybe_log_stage1_deps_census(
    verts: &[BRepVertex],
    edges: &[BRepEdge],
    faces: &[BRepFace],
    tess_res: Result<&crate::stage1_tessellate::Stage1Tess, &YangError>,
) {
    let Some(path) = std::env::var_os("YANG_S1_DEPS_CENSUS") else {
        return;
    };

    let mut surf_counts = [0usize; 5]; // plane, cyl, cone, sphere, torus
    let mut sphere_min: Option<f64> = None;
    let mut sphere_max: Option<f64> = None;
    let mut torus_patch_min: Option<f64> = None;
    let mut torus_patch_max: Option<f64> = None;

    for f in faces {
        match f.surface {
            Surface::Plane { .. } => surf_counts[0] += 1,
            Surface::Cylinder { .. } => surf_counts[1] += 1,
            Surface::Cone { .. } => surf_counts[2] += 1,
            Surface::Sphere { radius, .. } => {
                surf_counts[3] += 1;
                let b = crate::stage1_tessellate::sphere_chord_bound(radius);
                sphere_min = Some(sphere_min.map_or(b, |m| m.min(b)));
                sphere_max = Some(sphere_max.map_or(b, |m| m.max(b)));
            }
            Surface::Torus {
                major_radius,
                minor_radius,
                ..
            } => {
                surf_counts[4] += 1;
                if crate::stage1_tessellate::torus_face_takes_patch_path(
                    f,
                    edges,
                    major_radius,
                    minor_radius,
                ) {
                    let b = crate::stage1_tessellate::torus_chord_bound(major_radius, minor_radius);
                    torus_patch_min = Some(torus_patch_min.map_or(b, |m| m.min(b)));
                    torus_patch_max = Some(torus_patch_max.map_or(b, |m| m.max(b)));
                }
            }
        }
    }

    let mut curve_counts = [0usize; 6]; // line, circle, ellipse, parabola, hyperbola, spair
    let mut spair_max: Option<f64> = None;
    for e in edges {
        match e.curve {
            Curve::LineSegment => curve_counts[0] += 1,
            Curve::Circle { .. } => curve_counts[1] += 1,
            Curve::Ellipse { .. } => curve_counts[2] += 1,
            Curve::Parabola { .. } => curve_counts[3] += 1,
            Curve::Hyperbola { .. } => curve_counts[4] += 1,
            Curve::SurfacePair { a, b } => {
                curve_counts[5] += 1;
                if let (Some(v0), Some(v1)) =
                    (verts.get(e.start as usize), verts.get(e.end as usize))
                {
                    if let Some(d) =
                        crate::stage1_tessellate::surface_pair_chain_bound(a, b, v0.point, v1.point)
                    {
                        spair_max = Some(spair_max.map_or(d, |m| m.max(d)));
                    }
                }
            }
        }
    }

    let sphere_seam_edges: std::collections::BTreeSet<u32> = faces
        .iter()
        .filter(|f| matches!(f.surface, Surface::Sphere { .. }))
        .flat_map(|f| {
            f.outer_loop
                .iter()
                .chain(f.inner_loops.iter().flatten())
                .copied()
        })
        .collect();
    let has_non_sphere_circle = edges.iter().enumerate().any(|(i, e)| {
        matches!(e.curve, Curve::Circle { .. }) && !sphere_seam_edges.contains(&(i as u32))
    });

    let d_eps_circle = crate::stage1_tessellate::curved_chord_bound(edges);
    let d_eps_circle_prepass = if has_non_sphere_circle {
        let mut d = d_eps_circle.unwrap_or(0.0);
        for f in faces {
            if let Surface::Cone {
                apex,
                axis_dir,
                half_angle,
            } = f.surface
            {
                let au = normalize3(axis_dir.as_array());
                let ap = apex.as_array();
                for &e_idx in &f.outer_loop {
                    if let Curve::Circle { center, .. } = edges[e_idx as usize].curve {
                        let c = center.as_array();
                        let h = ((c[0] - ap[0]) * au[0]
                            + (c[1] - ap[1]) * au[1]
                            + (c[2] - ap[2]) * au[2])
                            .abs();
                        d = d.min(crate::stage1_tessellate::cone_chord_bound(h, half_angle));
                    }
                }
            }
        }
        Some(d)
    } else {
        None
    };

    let d_eps_ellipse = crate::stage1_tessellate::ellipse_rim_chord_bound(edges);
    let d_eps_owner_budget = d_eps_circle.or(d_eps_ellipse).or(spair_max);

    // Collect all distinct Stage-1 chord bounds active on this operand today.
    let mut active_bounds: Vec<f64> = Vec::new();
    let push_distinct = |vec: &mut Vec<f64>, val: Option<f64>| {
        if let Some(x) = val {
            if !vec
                .iter()
                .any(|&y| (x - y).abs() <= 1e-12 * x.abs().max(1.0))
            {
                vec.push(x);
            }
        }
    };
    push_distinct(&mut active_bounds, d_eps_circle_prepass);
    push_distinct(&mut active_bounds, sphere_min);
    push_distinct(&mut active_bounds, sphere_max);
    push_distinct(&mut active_bounds, torus_patch_min);
    push_distinct(&mut active_bounds, torus_patch_max);
    push_distinct(&mut active_bounds, d_eps_ellipse);
    push_distinct(&mut active_bounds, spair_max);

    let d_eps_today_primary = d_eps_circle_prepass
        .or(sphere_min)
        .or(torus_patch_min)
        .or(d_eps_ellipse)
        .or(spair_max);
    let d_eps_today_min = active_bounds.iter().copied().reduce(f64::min);
    let d_eps_today_max = active_bounds.iter().copied().reduce(f64::max);

    let aabb_brep = brep_model_aabb(verts, edges, faces);
    let diag_brep = aabb_brep.map(|(lo, hi)| {
        let (dx, dy, dz) = (hi[0] - lo[0], hi[1] - lo[1], hi[2] - lo[2]);
        (dx * dx + dy * dy + dz * dz).sqrt()
    });
    let d_eps_bbox = brep_model_chord_bound(verts, edges, faces);

    let (
        ok,
        n_tris,
        diag_mesh,
        n_inner_curved,
        n_bdy_curved,
        max_dt_inner,
        max_dt_bdy,
        max_dt_inner_over_today_face,
        max_dt_inner_over_bbox,
        dt_errors,
        max_eval_err,
        worst_inner_kind,
    ) = match tess_res {
        Ok(tess) => {
            let mut mlo = [f64::INFINITY; 3];
            let mut mhi = [f64::NEG_INFINITY; 3];
            for p in &tess.verts {
                expand_pt(&mut mlo, &mut mhi, p.as_array());
            }
            let dmesh = if mlo[0].is_finite() {
                Some(
                    ((mhi[0] - mlo[0]).powi(2)
                        + (mhi[1] - mlo[1]).powi(2)
                        + (mhi[2] - mlo[2]).powi(2))
                    .sqrt(),
                )
            } else {
                None
            };

            let mut n_in = 0usize;
            let mut n_bd = 0usize;
            let mut m_in = 0.0_f64;
            let mut m_bd = 0.0_f64;
            let mut m_in_ratio_today = 0.0_f64;
            let mut m_in_ratio_bbox = 0.0_f64;
            let mut n_dt_err = 0usize;
            let mut m_eval = 0.0_f64;
            let mut w_kind = "none";

            for (f_idx, range) in tess.face_tri_ranges.iter().enumerate() {
                let surface = faces[f_idx].surface;
                if matches!(surface, Surface::Plane { .. }) {
                    continue;
                }
                let face_today_deps = match surface {
                    Surface::Sphere { radius, .. } => {
                        Some(crate::stage1_tessellate::sphere_chord_bound(radius))
                    }
                    Surface::Torus {
                        major_radius,
                        minor_radius,
                        ..
                    } if crate::stage1_tessellate::torus_face_takes_patch_path(
                        &faces[f_idx],
                        edges,
                        major_radius,
                        minor_radius,
                    ) =>
                    {
                        Some(crate::stage1_tessellate::torus_chord_bound(
                            major_radius,
                            minor_radius,
                        ))
                    }
                    _ => d_eps_circle_prepass.or(d_eps_ellipse).or(spair_max),
                };
                let surf_label = match surface {
                    Surface::Plane { .. } => "plane",
                    Surface::Cylinder { .. } => "cyl",
                    Surface::Cone { .. } => "cone",
                    Surface::Sphere { .. } => "sphere",
                    Surface::Torus { .. } => "torus",
                };
                for ti in range.clone() {
                    let tri = tess.tris[ti];
                    let is_inner = tri.iter().all(|&vi| {
                        matches!(
                            tess.sources[vi as usize],
                            TessellationSource::BRepFace { .. }
                        )
                    });
                    let pts = [
                        tess.verts[tri[0] as usize],
                        tess.verts[tri[1] as usize],
                        tess.verts[tri[2] as usize],
                    ];
                    let (eff_surf, uv) = stage1_tri_surface_and_uv(surface, pts);
                    for k in 0..3 {
                        if let Ok(q) = crate::stage4_dt::eval_uv(&eff_surf, uv[k]) {
                            let (pa, qa) = (pts[k].as_array(), q.as_array());
                            let err = ((pa[0] - qa[0]).powi(2)
                                + (pa[1] - qa[1]).powi(2)
                                + (pa[2] - qa[2]).powi(2))
                            .sqrt();
                            m_eval = m_eval.max(err);
                        }
                    }
                    match crate::stage4_dt::d_of_t(&eff_surf, uv) {
                        Ok(dt) => {
                            if is_inner {
                                n_in += 1;
                                m_in = m_in.max(dt);
                                if let Some(fd) = face_today_deps.filter(|&x| x > 0.0) {
                                    let r = dt / fd;
                                    if r > m_in_ratio_today {
                                        m_in_ratio_today = r;
                                        w_kind = surf_label;
                                    }
                                }
                                if let Some(bd) = d_eps_bbox.filter(|&x| x > 0.0) {
                                    m_in_ratio_bbox = m_in_ratio_bbox.max(dt / bd);
                                }
                            } else {
                                n_bd += 1;
                                m_bd = m_bd.max(dt);
                            }
                        }
                        Err(_) => {
                            n_dt_err += 1;
                        }
                    }
                }
            }

            (
                true,
                tess.tris.len(),
                dmesh,
                n_in,
                n_bd,
                m_in,
                m_bd,
                m_in_ratio_today,
                m_in_ratio_bbox,
                n_dt_err,
                m_eval,
                w_kind,
            )
        }
        Err(_) => (
            false,
            0,
            None,
            0,
            0,
            0.0,
            0.0,
            0.0,
            0.0,
            0,
            0.0,
            "stage1_err",
        ),
    };

    let case = std::env::var("ASSAY_CASE").unwrap_or_default();
    let has_curved_face = surf_counts[1..].iter().any(|&n| n > 0);
    let has_curved_edge = curve_counts[1..].iter().any(|&n| n > 0);

    let line = format!(
        "{{\"case\":\"{case}\",\"ok\":{ok},\"verts\":{},\"edges\":{},\"faces\":{},\"tris\":{n_tris},\
\"has_curved_face\":{has_curved_face},\"has_curved_edge\":{has_curved_edge},\
\"surf_counts\":{surf_counts:?},\"curve_counts\":{curve_counts:?},\
\"diag_brep\":{},\"diag_mesh\":{},\"d_eps_bbox\":{},\
\"d_eps_circle\":{},\"d_eps_circle_prepass\":{},\
\"d_eps_sphere_min\":{},\"d_eps_sphere_max\":{},\
\"d_eps_torus_patch_min\":{},\"d_eps_torus_patch_max\":{},\
\"d_eps_ellipse\":{},\"d_eps_spair\":{},\
\"d_eps_today_primary\":{},\"d_eps_today_min\":{},\"d_eps_today_max\":{},\
\"d_eps_owner_budget\":{},\"active_bound_count\":{},\
\"n_inner_curved\":{n_inner_curved},\"n_bdy_curved\":{n_bdy_curved},\
\"max_dt_inner\":{max_dt_inner:.12e},\"max_dt_bdy\":{max_dt_bdy:.12e},\
\"max_dt_inner_over_today\":{max_dt_inner_over_today_face:.9},\"max_dt_inner_over_bbox\":{max_dt_inner_over_bbox:.9},\
\"worst_inner_surf\":\"{worst_inner_kind}\",\"dt_errors\":{dt_errors},\"max_eval_err\":{max_eval_err:.6e}}}\n",
        verts.len(),
        edges.len(),
        faces.len(),
        fmt_opt(diag_brep),
        fmt_opt(diag_mesh),
        fmt_opt(d_eps_bbox),
        fmt_opt(d_eps_circle),
        fmt_opt(d_eps_circle_prepass),
        fmt_opt(sphere_min),
        fmt_opt(sphere_max),
        fmt_opt(torus_patch_min),
        fmt_opt(torus_patch_max),
        fmt_opt(d_eps_ellipse),
        fmt_opt(spair_max),
        fmt_opt(d_eps_today_primary),
        fmt_opt(d_eps_today_min),
        fmt_opt(d_eps_today_max),
        fmt_opt(d_eps_owner_budget),
        active_bounds.len(),
    );

    if let Ok(mut f) = std::fs::OpenOptions::new()
        .create(true)
        .append(true)
        .open(path)
    {
        use std::io::Write as _;
        let _ = f.write_all(line.as_bytes());
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn brep_model_aabb_tall_cylinder_includes_axial_height() {
        // Radius 1 cylinder of height 20 along Z: circle-only AABB has
        // dx=2, dy=2, dz=20 => diag = sqrt(4+4+400) = sqrt(408), which
        // happens to match for full rims at both ends; now test an ALL-PLANAR
        // 10×20×30 box where curved_chord_bound is None while brep_model_aabb
        // gives exact diag = sqrt(100+400+900) = sqrt(1400).
        let pts = [
            Point3::new(0.0, 0.0, 0.0),
            Point3::new(10.0, 0.0, 0.0),
            Point3::new(0.0, 20.0, 0.0),
            Point3::new(0.0, 0.0, 30.0),
        ];
        let verts: Vec<BRepVertex> = pts.into_iter().map(|point| BRepVertex { point }).collect();
        let (lo, hi) = brep_model_aabb(&verts, &[], &[]).expect("non-empty box aabb");
        assert_eq!(lo, [0.0, 0.0, 0.0]);
        assert_eq!(hi, [10.0, 20.0, 30.0]);
        let deps = brep_model_chord_bound(&verts, &[], &[]).expect("positive d_eps");
        let expected = 1e-2 * (1400.0_f64).sqrt();
        assert!((deps - expected).abs() <= 1e-15);
    }

    #[test]
    fn brep_model_aabb_closed_sphere_and_tilted_closed_torus() {
        // Closed sphere of radius 5 centered at (1, -2, 3): exact diag = 10√3.
        let sphere_verts = vec![
            BRepVertex {
                point: Point3::new(1.0, -2.0, -2.0),
            },
            BRepVertex {
                point: Point3::new(1.0, -2.0, 8.0),
            },
        ];
        let sphere_edges = vec![BRepEdge {
            start: 0,
            end: 1,
            curve: Curve::Circle {
                center: Point3::new(1.0, -2.0, 3.0),
                normal: Vector3::new(0.0, -1.0, 0.0),
                radius: 5.0,
            },
        }];
        let sphere_faces = vec![BRepFace {
            surface: Surface::Sphere {
                center: Point3::new(1.0, -2.0, 3.0),
                radius: 5.0,
            },
            outer_loop: vec![0],
            inner_loops: Vec::new(),
            reversed: false,
        }];
        let deps_s = brep_model_chord_bound(&sphere_verts, &sphere_edges, &sphere_faces)
            .expect("sphere d_eps");
        let exp_s = 1e-2 * 10.0 * 3.0_f64.sqrt();
        assert!((deps_s - exp_s).abs() <= 1e-14);

        // Z-axis closed torus R=10, r=2: dx=24, dy=24, dz=4 (NOT 2*(R+r)=24 along Z!).
        let torus_verts = vec![BRepVertex {
            point: Point3::new(12.0, 0.0, 0.0),
        }];
        let torus_edges = vec![
            BRepEdge {
                start: 0,
                end: 0,
                curve: Curve::Circle {
                    center: Point3::new(10.0, 0.0, 0.0),
                    normal: Vector3::new(0.0, 1.0, 0.0),
                    radius: 2.0,
                },
            },
            BRepEdge {
                start: 0,
                end: 0,
                curve: Curve::Circle {
                    center: Point3::new(0.0, 0.0, 0.0),
                    normal: Vector3::new(0.0, 0.0, 1.0),
                    radius: 12.0,
                },
            },
        ];
        let torus_faces = vec![BRepFace {
            surface: Surface::Torus {
                center: Point3::new(0.0, 0.0, 0.0),
                axis_dir: Vector3::new(0.0, 0.0, 1.0),
                major_radius: 10.0,
                minor_radius: 2.0,
            },
            outer_loop: vec![0, 1],
            inner_loops: Vec::new(),
            reversed: false,
        }];
        let (tlo, thi) =
            brep_model_aabb(&torus_verts, &torus_edges, &torus_faces).expect("torus aabb");
        assert!((thi[0] - 12.0).abs() <= 1e-12 && (tlo[0] + 12.0).abs() <= 1e-12);
        assert!((thi[1] - 12.0).abs() <= 1e-12 && (tlo[1] + 12.0).abs() <= 1e-12);
        assert!((thi[2] - 2.0).abs() <= 1e-12 && (tlo[2] + 2.0).abs() <= 1e-12);
    }

    #[test]
    fn unwrap_circle_triple_crosses_branch_cut_cleanly() {
        let u = unwrap_circle_triple([0.05, -0.05, 0.02]);
        let lo = u[0].min(u[1]).min(u[2]);
        let hi = u[0].max(u[1]).max(u[2]);
        assert!((hi - lo - 0.10).abs() <= 1e-12);
    }
}
