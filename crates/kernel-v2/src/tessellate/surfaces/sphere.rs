//! `Surface::Sphere` render tessellators (move-only F9 split from
//! `tessellate/surfaces.rs`; byte-identical): the closed-sphere detector, the
//! closed lat/long grid, and the trimmed sphere patch. See `super`'s module docs.

use super::*;

/// Is this [`Surface::Sphere`] face the CLOSED modeling sphere — the outer
/// loop exactly the meridian seam-Arc twin pair, no inner loops (KV6d
/// increment 2, spec `kv6d_sphere_revolve.md`)? Anything else is a
/// boolean-output trimmed patch.
pub(crate) fn sphere_face_is_closed(arena: &BrepArena, fid: FaceId) -> Result<bool, KernelV2Error> {
    let face = arena.face(fid)?;
    if !face.inner_loops.is_empty() {
        return Ok(false);
    }
    let hes = arena.loop_half_edges(face.outer_loop)?;
    if hes.len() != 2 {
        return Ok(false);
    }
    let both_arcs = matches!(arena.half_edge(hes[0])?.curve, Curve::Arc { .. })
        && matches!(arena.half_edge(hes[1])?.curve, Curve::Arc { .. });
    Ok(both_arcs && arena.half_edge(hes[0])?.twin == hes[1])
}

/// Tessellate the CLOSED [`Surface::Sphere`] face (KV6d increment 2): a z-up
/// latitude/longitude grid. Poles are emitted ONCE (single vertex each, fan
/// closure); the longitude wrap reuses column 0 via modular indexing (no
/// duplicated seam column) — watertight by construction, mirroring the
/// closed-torus θ-row wrap.
pub(crate) fn tessellate_sphere_closed(
    arena: &BrepArena,
    fid: FaceId,
    n_seg: u32,
    out: &mut RenderMesh,
) -> Result<(), KernelV2Error> {
    use std::f64::consts::PI;
    let face = arena.face(fid)?;
    let Some(Surface::Sphere {
        center,
        radius: r,
        reversed,
    }) = face.surface
    else {
        return Err(KernelV2Error::TessellationFailed {
            face: fid,
            reason: "tessellate_sphere_closed on a non-sphere face",
        });
    };
    let c = [center.x(), center.y(), center.z()];
    let n_lon = n_seg.max(3) as usize;
    let n_lat = ((n_seg / 2).max(2)) as usize;

    let range_start = out.indices.len() as u32;
    let base = out.num_vertices() as u32;
    let sign = if reversed { -1.0 } else { 1.0 };
    let push = |p: [f64; 3], out: &mut RenderMesh| {
        out.positions.extend_from_slice(&p);
        let n = [
            sign * (p[0] - c[0]) / r,
            sign * (p[1] - c[1]) / r,
            sign * (p[2] - c[2]) / r,
        ];
        out.normals.extend_from_slice(&n);
    };
    // Vertex layout: south pole, north pole, then interior rings
    // j = 1..n_lat (bottom to top), each n_lon columns.
    push([c[0], c[1], c[2] - r], out);
    push([c[0], c[1], c[2] + r], out);
    for j in 1..n_lat {
        let v = -PI / 2.0 + PI * (j as f64) / (n_lat as f64);
        let (sv, cv) = v.sin_cos();
        for i in 0..n_lon {
            let u = 2.0 * PI * (i as f64) / (n_lon as f64);
            let (su, cu) = u.sin_cos();
            push([c[0] + r * cv * cu, c[1] + r * cv * su, c[2] + r * sv], out);
        }
    }
    let (south, north) = (base, base + 1);
    let ring = |j: usize, i: usize| base + 2 + ((j - 1) * n_lon + (i % n_lon)) as u32;
    // Winding: emitted CCW-outward by construction (u eastward, v northward);
    // a reversed (cavity) face flips.
    let emit = |a: u32, b: u32, cc: u32, out: &mut RenderMesh| {
        if reversed {
            out.indices.extend_from_slice(&[a, cc, b]);
        } else {
            out.indices.extend_from_slice(&[a, b, cc]);
        }
    };
    for i in 0..n_lon {
        emit(south, ring(1, i + 1), ring(1, i), out);
        emit(north, ring(n_lat - 1, i), ring(n_lat - 1, i + 1), out);
    }
    for j in 1..n_lat - 1 {
        for i in 0..n_lon {
            let (a, b) = (ring(j, i), ring(j, i + 1));
            let (d, cc) = (ring(j + 1, i), ring(j + 1, i + 1));
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

/// Render-tessellate a boolean-OUTPUT sphere PATCH (KV6d increment 2) — a
/// [`Surface::Sphere`] face whose boundary is the trimmed intersection loop
/// (plane∩sphere circle arcs + chord polylines) instead of the seam-arc pair
/// the modeling tessellator needs.
///
/// The sphere is not developable, so like the torus this delegates to a
/// yang-rs UV consumer ([`yang_rs::tessellate_sphere_patch`]): project the
/// boundary into the (longitude, latitude) plane, CDT with interior Steiner
/// refinement, and (for a pole-containing patch) bridge the wrapping loop to
/// the pole. Boundary polylines pass through EXACTLY, so the patch stays
/// watertight against its planar neighbors; each triangle is wound to agree
/// with the analytic outward sphere normal.
pub(crate) fn tessellate_sphere_patch(
    arena: &BrepArena,
    fid: FaceId,
    n_seg: u32,
    out: &mut RenderMesh,
) -> Result<(), KernelV2Error> {
    use std::f64::consts::PI;
    let face = arena.face(fid)?;
    let Some(Surface::Sphere {
        center,
        radius: r,
        reversed,
    }) = face.surface
    else {
        return Err(KernelV2Error::TessellationFailed {
            face: fid,
            reason: "tessellate_sphere_patch on a non-sphere face",
        });
    };
    let fail = |reason: &'static str| KernelV2Error::TessellationFailed { face: fid, reason };
    let c = [center.x(), center.y(), center.z()];

    // Gather each loop as an ordered 3D polyline (walk order; arc interior
    // samples are twin-canonical — shared with the adjacent planar face).
    // A FULL-circle boundary edge (a hemisphere's rim, shared with a disk
    // cap) is densified with BITWISE the frame the cap tessellator uses —
    // `circle_frame(center, NEG(this normal), anchor)`, reversed into walk
    // order (the cylinder-lateral recipe) — so the shared rim positions are
    // bit-identical across the two faces.
    let gather = |loop_id| -> Result<Vec<Point3>, KernelV2Error> {
        let hes = arena.loop_half_edges(loop_id)?;
        let mut pts: Vec<Point3> = Vec::with_capacity(hes.len());
        for &h in &hes {
            let he = arena.half_edge(h)?;
            pts.push(arena.vertex(he.origin)?.point);
            if let Curve::Circle {
                center: cc,
                normal,
                radius: cr,
            } = he.curve
            {
                let anchor = arena.vertex(he.origin)?.point;
                let cap_n = crate::arena::UnitVector3 {
                    x: -normal.x,
                    y: -normal.y,
                    z: -normal.z,
                };
                let Some((e1, e2)) = circle_frame(cc, cap_n, anchor) else {
                    return Err(fail("degenerate circle frame on a sphere patch rim"));
                };
                let n = n_seg.max(3) as usize;
                for k in (1..n).rev() {
                    let theta = 2.0 * PI * (k as f64) / (n as f64);
                    let (s, co) = theta.sin_cos();
                    pts.push(Point3::new(
                        cc.x() + cr * (co * e1[0] + s * e2[0]),
                        cc.y() + cr * (co * e1[1] + s * e2[1]),
                        cc.z() + cr * (co * e1[2] + s * e2[2]),
                    ));
                }
            } else {
                pts.extend(arc_interior_samples(arena, h, n_seg)?);
            }
        }
        Ok(pts)
    };
    let boundary = gather(face.outer_loop)?;
    if boundary.len() < 3 {
        return Err(fail("sphere patch boundary has fewer than 3 vertices"));
    }
    let mut holes: Vec<Vec<Point3>> = Vec::with_capacity(face.inner_loops.len());
    for &lid in &face.inner_loops {
        let h = gather(lid)?;
        if h.len() < 3 {
            return Err(fail("sphere patch interior loop has fewer than 3 vertices"));
        }
        holes.push(h);
    }

    // Triangle-area budget in arc-length²: match the equator chord spacing of
    // the structured tessellator (the torus-patch recipe).
    let seg = 2.0 * PI * r / f64::from(n_seg.max(3));
    let max_area = seg * seg;

    let Some((verts, tris)) =
        yang_rs::tessellate_sphere_patch(center, r, reversed, &boundary, &holes, max_area)
    else {
        return Err(fail(
            "sphere patch UV-CDT failed (multi-wrap / pole-crossing boundary — later slice)",
        ));
    };

    // Analytic outward sphere normal (reversed-aware).
    let normal_at = |p: [f64; 3]| -> [f64; 3] {
        let mut n = [p[0] - c[0], p[1] - c[1], p[2] - c[2]];
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

// ---------------------------------------------------------------------------
// PR-KV5b: partial cylinder patches (boolean outputs)
// ---------------------------------------------------------------------------
