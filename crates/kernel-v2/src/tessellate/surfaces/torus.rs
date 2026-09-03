//! `Surface::Torus` render tessellators (move-only F9 split from
//! `tessellate/surfaces.rs`; byte-identical): the partial-torus lateral band
//! (θ×φ quad grid) and the trimmed torus patch. See `super`'s module docs.

use super::*;

/// Tessellate a [`Surface::Torus`] lateral (KV6d): a partial torus (bent tube)
/// as a (θ × φ) quad grid. θ runs over the sweep `α`, φ over the profile circle.
/// The θ=0 / θ=α rings reproduce the start/end profile circles bit-for-bit
/// (same φ table at `n_seg`), so the band is watertight against its two disk
/// caps as position sets. The θ=0 reference `w0` and the sweep `α` are recovered
/// from the seam arc (the φ=0 longitude: radius major+minor, normal +axis).
pub(crate) fn tessellate_torus_lateral(
    arena: &BrepArena,
    fid: FaceId,
    n_seg: u32,
    out: &mut RenderMesh,
) -> Result<(), KernelV2Error> {
    use std::f64::consts::PI;
    let face = arena.face(fid)?;
    let Some(Surface::Torus {
        center,
        axis_dir,
        major_radius: r_maj,
        minor_radius: r_min,
        reversed,
    }) = face.surface
    else {
        return Err(KernelV2Error::TessellationFailed {
            face: fid,
            reason: "tessellate_torus_lateral on a non-torus face",
        });
    };
    let fail = |reason: &'static str| KernelV2Error::TessellationFailed { face: fid, reason };
    let ax = [axis_dir.x, axis_dir.y, axis_dir.z];
    let c = [center.x(), center.y(), center.z()];

    // Recover (w0, α) from the +axis seam arc (radius major+minor). The
    // CLOSED torus (KV6d full turn, spec `kv6d_closed_torus_revolve.md`)
    // has no seam ARC — its toroidal seam is the closed outer-equator
    // CIRCLE; anchor θ = 0 at its seam vertex and sweep the full 2π with
    // wrapped θ rows.
    let hes = arena.loop_half_edges(face.outer_loop)?;
    let mut seam = None;
    let mut closed = false;
    for &h in &hes {
        let he = arena.half_edge(h)?;
        if let Curve::Arc { radius, normal, .. } = he.curve {
            if (radius - (r_maj + r_min)).abs() <= 1e-9 * (1.0 + r_maj + r_min)
                && (normal.x * ax[0] + normal.y * ax[1] + normal.z * ax[2]) > 0.0
            {
                let v0 = arena.vertex(he.origin)?.point;
                let dest = arena.half_edge(he.next)?.origin;
                seam = Some((v0, arena.vertex(dest)?.point));
                break;
            }
        }
    }
    if seam.is_none() {
        for &h in &hes {
            let he = arena.half_edge(h)?;
            if let Curve::Circle { radius, normal, .. } = he.curve {
                if (radius - (r_maj + r_min)).abs() <= 1e-9 * (1.0 + r_maj + r_min)
                    && (normal.x * ax[0] + normal.y * ax[1] + normal.z * ax[2]) > 0.0
                {
                    let v0 = arena.vertex(he.origin)?.point;
                    seam = Some((v0, v0));
                    closed = true;
                    break;
                }
            }
        }
    }
    let Some((v0, valpha)) = seam else {
        return Err(fail("torus lateral missing its +axis seam arc"));
    };
    let wv = [v0.x() - c[0], v0.y() - c[1], v0.z() - c[2]];
    let along = wv[0] * ax[0] + wv[1] * ax[1] + wv[2] * ax[2];
    let wr = [
        wv[0] - along * ax[0],
        wv[1] - along * ax[1],
        wv[2] - along * ax[2],
    ];
    let wl = (wr[0] * wr[0] + wr[1] * wr[1] + wr[2] * wr[2]).sqrt();
    if !(wl.is_finite() && wl > 0.0) {
        return Err(fail("degenerate torus θ=0 reference"));
    }
    let w0 = [wr[0] / wl, wr[1] / wl, wr[2] / wl];
    let alpha = if closed {
        2.0 * PI
    } else {
        crate::geom::ccw_sweep(center, ax, v0, valpha).ok_or(fail("degenerate torus sweep"))?
    };
    let m0 = [
        ax[1] * w0[2] - ax[2] * w0[1],
        ax[2] * w0[0] - ax[0] * w0[2],
        ax[0] * w0[1] - ax[1] * w0[0],
    ];

    // φ matches the caps (n_seg); θ steps keep a comparable chord at radius R+r.
    let n_phi = n_seg.max(3) as usize;
    let n_theta = {
        let per = (2.0 * PI / n_seg as f64) * r_min / (r_maj + r_min);
        ((alpha / per).ceil() as usize).max(if closed { 3 } else { 2 })
    };
    // Closed torus: the θ = 2π row IS the θ = 0 row — emit n_theta rows and
    // wrap the row index instead of duplicating the seam ring.
    let n_rows = if closed { n_theta } else { n_theta + 1 };
    let range_start = out.indices.len() as u32;
    let base = out.num_vertices() as u32;
    let point = |theta: f64, phi: f64| -> ([f64; 3], [f64; 3]) {
        let (st, ct) = theta.sin_cos();
        let wth = [
            ct * w0[0] + st * m0[0],
            ct * w0[1] + st * m0[1],
            ct * w0[2] + st * m0[2],
        ];
        let (sp, cp) = phi.sin_cos();
        let rad = r_maj + r_min * cp;
        let p = [
            c[0] + rad * wth[0] + r_min * sp * ax[0],
            c[1] + rad * wth[1] + r_min * sp * ax[1],
            c[2] + rad * wth[2] + r_min * sp * ax[2],
        ];
        let mut nrm = [
            cp * wth[0] + sp * ax[0],
            cp * wth[1] + sp * ax[1],
            cp * wth[2] + sp * ax[2],
        ];
        if reversed {
            nrm = [-nrm[0], -nrm[1], -nrm[2]];
        }
        (p, nrm)
    };
    for i in 0..n_rows {
        let theta = alpha * (i as f64) / (n_theta as f64);
        for j in 0..n_phi {
            let phi = 2.0 * PI * (j as f64) / (n_phi as f64);
            let (p, nrm) = point(theta, phi);
            out.positions.extend_from_slice(&p);
            out.normals.extend_from_slice(&nrm);
        }
    }
    let idx = |i: usize, j: usize| base + ((i % n_rows) * n_phi + (j % n_phi)) as u32;
    let pos = |out: &RenderMesh, vi: u32| {
        let k = vi as usize * 3;
        [out.positions[k], out.positions[k + 1], out.positions[k + 2]]
    };
    // Emit a triangle, winding it so its geometric normal agrees with the
    // analytic torus outward normal at the centroid (reversed-aware).
    let emit = |a: u32, b: u32, cc: u32, out: &mut RenderMesh| {
        let (pa, pb, pc) = (pos(out, a), pos(out, b), pos(out, cc));
        let e1 = [pb[0] - pa[0], pb[1] - pa[1], pb[2] - pa[2]];
        let e2 = [pc[0] - pa[0], pc[1] - pa[1], pc[2] - pa[2]];
        let gn = [
            e1[1] * e2[2] - e1[2] * e2[1],
            e1[2] * e2[0] - e1[0] * e2[2],
            e1[0] * e2[1] - e1[1] * e2[0],
        ];
        let cen = [
            (pa[0] + pb[0] + pc[0]) / 3.0,
            (pa[1] + pb[1] + pc[1]) / 3.0,
            (pa[2] + pb[2] + pc[2]) / 3.0,
        ];
        let d = [cen[0] - c[0], cen[1] - c[1], cen[2] - c[2]];
        let t = d[0] * ax[0] + d[1] * ax[1] + d[2] * ax[2];
        let rv = [d[0] - t * ax[0], d[1] - t * ax[1], d[2] - t * ax[2]];
        let rl = (rv[0] * rv[0] + rv[1] * rv[1] + rv[2] * rv[2])
            .sqrt()
            .max(1e-300);
        let rhat = [rv[0] / rl, rv[1] / rl, rv[2] / rl];
        let mut on = [
            cen[0] - (c[0] + r_maj * rhat[0]),
            cen[1] - (c[1] + r_maj * rhat[1]),
            cen[2] - (c[2] + r_maj * rhat[2]),
        ];
        if reversed {
            on = [-on[0], -on[1], -on[2]];
        }
        if gn[0] * on[0] + gn[1] * on[1] + gn[2] * on[2] >= 0.0 {
            out.indices.extend_from_slice(&[a, b, cc]);
        } else {
            out.indices.extend_from_slice(&[a, cc, b]);
        }
    };
    for i in 0..n_theta {
        for j in 0..n_phi {
            let (a, b, cc, d) = (idx(i, j), idx(i, j + 1), idx(i + 1, j + 1), idx(i + 1, j));
            emit(a, b, cc, out);
            emit(a, cc, d, out);
        }
    }
    out.face_ranges.push(FaceRange {
        face: fid,
        start: range_start,
        count: out.indices.len() as u32 - range_start,
    });
    Ok(())
}

/// KV6d increment 5b2: render-tessellate a boolean-OUTPUT torus PATCH — a
/// `Surface::Torus` face whose boundary is the trimmed intersection loop (a
/// chord polyline, possibly with surviving seam-arc spans), NOT the structured
/// seam-arc loop the modeling tessellator [`tessellate_torus_lateral`] needs.
///
/// The torus is degree-4 and NOT developable, so the cylinder patch's
/// unroll+ear-clip does not transfer; instead we delegate to yang-rs's UV-CDT
/// consumer [`yang_rs::tessellate_torus_patch`], which projects the boundary
/// into the `(meridian, longitude)` plane, constrained-Delaunay-triangulates
/// with interior Steiner points (to bound chord error), and maps back to 3D
/// with the boundary vertices kept EXACT (conformal with the neighbouring
/// faces, which sample the same arc/segment edges twin-canonically). We then
/// emit with the analytic outward torus normal, winding each triangle to agree.
pub(crate) fn tessellate_torus_patch(
    arena: &BrepArena,
    fid: FaceId,
    n_seg: u32,
    out: &mut RenderMesh,
) -> Result<(), KernelV2Error> {
    use std::f64::consts::PI;
    let face = arena.face(fid)?;
    let Some(Surface::Torus {
        center,
        axis_dir,
        major_radius: r_maj,
        minor_radius: r_min,
        reversed,
    }) = face.surface
    else {
        return Err(KernelV2Error::TessellationFailed {
            face: fid,
            reason: "tessellate_torus_patch on a non-torus face",
        });
    };
    let fail = |reason: &'static str| KernelV2Error::TessellationFailed { face: fid, reason };
    let c = [center.x(), center.y(), center.z()];
    let ax = [axis_dir.x, axis_dir.y, axis_dir.z];

    // Gather a loop as an ordered 3D polyline: each half-edge's origin, then its
    // arc interior samples (empty for a line segment), in walk order. Arc
    // samples are twin-canonical, so a surviving seam arc shared with a cap is
    // sampled identically on both faces.
    let gather = |loop_id| -> Result<Vec<Point3>, KernelV2Error> {
        let hes = arena.loop_half_edges(loop_id)?;
        let mut pts: Vec<Point3> = Vec::with_capacity(hes.len());
        for &h in &hes {
            let he = arena.half_edge(h)?;
            pts.push(arena.vertex(he.origin)?.point);
            pts.extend(arc_interior_samples(arena, h, n_seg)?);
        }
        Ok(pts)
    };
    let boundary = gather(face.outer_loop)?;
    if boundary.len() < 3 {
        return Err(fail("torus patch boundary has fewer than 3 vertices"));
    }
    // Interior holes (e.g. a window bitten out of the tube middle) become CDT
    // holes in the (u, v) parameter plane.
    let mut holes: Vec<Vec<Point3>> = Vec::with_capacity(face.inner_loops.len());
    for &lid in &face.inner_loops {
        let h = gather(lid)?;
        if h.len() < 3 {
            return Err(fail("torus patch interior loop has fewer than 3 vertices"));
        }
        holes.push(h);
    }

    // Triangle-area budget in arc-length² (the consumer scales (u,v) to
    // arc-length before refining): match the meridian grid spacing of the
    // structured tessellator (tube circumference 2π·r_min over n_seg).
    let seg = 2.0 * PI * r_min / f64::from(n_seg.max(3));
    let max_area = seg * seg;

    // §4.3.4 inc-0 census (spec `yang_434_output_chord_refinement.md` §3,
    // env-gated `KV2_CHORD_DEPTH_CENSUS`, print-only): the torus-patch
    // analogue of the developable chord-depth row. No split/fold machinery
    // here (UV-CDT mints interior Steiner points on-surface); the signal is
    // the midpoint depth of `LineSegment` boundary chords off the torus.
    if std::env::var_os("KV2_CHORD_DEPTH_CENSUS").is_some() {
        let mut n_chord = 0usize;
        let mut max_sag = 0.0f64;
        let mut all_loops = vec![face.outer_loop];
        all_loops.extend(face.inner_loops.iter().copied());
        for &lid in &all_loops {
            for &h in &arena.loop_half_edges(lid)? {
                let he = arena.half_edge(h)?;
                if !matches!(he.curve, Curve::LineSegment) {
                    continue;
                }
                let p = arena.vertex(he.origin)?.point;
                let q = arena.vertex(arena.half_edge(he.next)?.origin)?.point;
                let m = [
                    (p.x() + q.x()) / 2.0,
                    (p.y() + q.y()) / 2.0,
                    (p.z() + q.z()) / 2.0,
                ];
                let d = [m[0] - c[0], m[1] - c[1], m[2] - c[2]];
                let t = d[0] * ax[0] + d[1] * ax[1] + d[2] * ax[2];
                let rv = [d[0] - t * ax[0], d[1] - t * ax[1], d[2] - t * ax[2]];
                let rl = (rv[0] * rv[0] + rv[1] * rv[1] + rv[2] * rv[2])
                    .sqrt()
                    .max(1e-300);
                let tube = [
                    c[0] + r_maj * rv[0] / rl,
                    c[1] + r_maj * rv[1] / rl,
                    c[2] + r_maj * rv[2] / rl,
                ];
                let dt = ((m[0] - tube[0]).powi(2)
                    + (m[1] - tube[1]).powi(2)
                    + (m[2] - tube[2]).powi(2))
                .sqrt();
                n_chord += 1;
                max_sag = max_sag.max((dt - r_min).abs());
            }
        }
        if n_chord > 0 {
            eprintln!(
                "[chord-census] face={fid:?} kind=torus seg={seg:.6e} r_min={r_min:.6e} \
                 n_chord={n_chord} max_chord_sag={max_sag:.6e}"
            );
        }
    }

    let axis_v = Vector3::new(ax[0], ax[1], ax[2]);
    let Some((verts, tris)) = yang_rs::tessellate_torus_patch(
        center, axis_v, r_maj, r_min, &boundary, &holes, max_area, reversed,
    ) else {
        return Err(fail(
            "torus patch UV-CDT failed (self-intersecting projection / seam-crossing patch)",
        ));
    };

    // Analytic outward torus normal at a point p (reversed-aware): project to
    // the tube centre circle, take p − tubeCentre.
    let normal_at = |p: [f64; 3]| -> [f64; 3] {
        let d = [p[0] - c[0], p[1] - c[1], p[2] - c[2]];
        let t = d[0] * ax[0] + d[1] * ax[1] + d[2] * ax[2];
        let rv = [d[0] - t * ax[0], d[1] - t * ax[1], d[2] - t * ax[2]];
        let rl = (rv[0] * rv[0] + rv[1] * rv[1] + rv[2] * rv[2])
            .sqrt()
            .max(1e-300);
        let rhat = [rv[0] / rl, rv[1] / rl, rv[2] / rl];
        let tube = [
            c[0] + r_maj * rhat[0],
            c[1] + r_maj * rhat[1],
            c[2] + r_maj * rhat[2],
        ];
        let mut n = [p[0] - tube[0], p[1] - tube[1], p[2] - tube[2]];
        let nl = (n[0] * n[0] + n[1] * n[1] + n[2] * n[2]).sqrt().max(1e-300);
        n = [n[0] / nl, n[1] / nl, n[2] / nl];
        if reversed {
            n = [-n[0], -n[1], -n[2]];
        }
        n
    };

    let range_start = out.indices.len() as u32;
    let base = out.num_vertices() as u32;
    for v in &verts {
        let p = [v.x(), v.y(), v.z()];
        out.positions.extend_from_slice(&p);
        out.normals.extend_from_slice(&normal_at(p));
    }
    let pos = |out: &RenderMesh, vi: u32| {
        let k = vi as usize * 3;
        [out.positions[k], out.positions[k + 1], out.positions[k + 2]]
    };
    // Wind each triangle so its geometric normal agrees with the analytic
    // outward normal at the centroid (reversed-aware).
    for t in &tris {
        let (a, b, cc) = (base + t[0], base + t[1], base + t[2]);
        let (pa, pb, pc) = (pos(out, a), pos(out, b), pos(out, cc));
        let e1 = [pb[0] - pa[0], pb[1] - pa[1], pb[2] - pa[2]];
        let e2 = [pc[0] - pa[0], pc[1] - pa[1], pc[2] - pa[2]];
        let gn = [
            e1[1] * e2[2] - e1[2] * e2[1],
            e1[2] * e2[0] - e1[0] * e2[2],
            e1[0] * e2[1] - e1[1] * e2[0],
        ];
        let cen = [
            (pa[0] + pb[0] + pc[0]) / 3.0,
            (pa[1] + pb[1] + pc[1]) / 3.0,
            (pa[2] + pb[2] + pc[2]) / 3.0,
        ];
        let on = normal_at(cen);
        if gn[0] * on[0] + gn[1] * on[1] + gn[2] * on[2] >= 0.0 {
            out.indices.extend_from_slice(&[a, b, cc]);
        } else {
            out.indices.extend_from_slice(&[a, cc, b]);
        }
    }
    out.face_ranges.push(FaceRange {
        face: fid,
        start: range_start,
        count: out.indices.len() as u32 - range_start,
    });
    Ok(())
}
