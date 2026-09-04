//! Stage-1 patch tessellators for revolve surfaces: the (theta x phi) torus
//! grid (face/closed/band + the shared band_seam_bridge and unwrap_seq helper)
//! and the torus/sphere UV-patch tessellators consumed by KV6 revolve.
//! Extracted move-only from stage1_tessellate.rs (#159 F9 decomposition).

#[allow(clippy::wildcard_imports)]
use crate::*;

/// KV14 Slice F / F-3: does this torus lateral tessellate through the UV-CDT
/// PATCH path (`tessellate_torus_band` → `tessellate_torus_patch`) rather than
/// the structured (θ × φ) grid? Yes when it carries inner loops (a poloidal
/// band, Slice F/F-2) or when its outer loop has none of the structured
/// vocabulary — no closed profile circle (radius ≈ minor) and no closed outer
/// equator (radius ≈ major + minor) — a DISK bounded by chords / open arcs
/// (Slice F-3). SINGLE SOURCE of the dispatch: `tessellate_torus_face` routes
/// on it and `input_curved_chord_bound` folds `torus_chord_bound` in for
/// exactly these faces (a structured lateral samples at its rims' density and
/// is covered by the rim band).
pub(crate) fn torus_face_takes_patch_path(
    f: &BRepFace,
    edges: &[BRepEdge],
    major: f64,
    minor: f64,
) -> bool {
    if !f.inner_loops.is_empty() {
        return true;
    }
    let band = 1e-9 * (1.0 + major + minor);
    !f.outer_loop.iter().any(|&e| {
        let ed = &edges[e as usize];
        matches!(ed.curve, Curve::Circle { radius, .. }
            if ed.start == ed.end
                && ((radius - minor).abs() <= band || (radius - (major + minor)).abs() <= band))
    })
}

/// KV6d 4b: tessellate a partial-torus `Surface::Torus` face — a bent tube —
/// as a watertight (θ × φ) bijective grid. Rows are meridians (constant sweep
/// angle θ), columns are longitudes (constant profile angle φ). The two θ-end
/// meridians REUSE the profile-circle rim rings (so they match the disk caps
/// bit-for-bit → watertight); the φ=0 column REUSES the seam-arc chain; the
/// interior points are fresh `BRepFace { u=φ, v=θ }` (so `eval_source`
/// round-trips via the torus `face_eval` arm). Rings are aligned by intrinsic
/// φ (`atan2(τ, ρ−R)`), so a counter-rotating end meridian still lines up
/// column-for-column.
#[allow(clippy::too_many_arguments)]
pub(crate) fn tessellate_torus_face(
    f_idx: usize,
    f: &BRepFace,
    edges: &[BRepEdge],
    rim_rings: &std::collections::BTreeMap<u32, Vec<u32>>,
    center: Point3,
    axis_dir: Vector3,
    major: f64,
    minor: f64,
    out_verts: &mut Vec<Point3>,
    sources: &mut Vec<TessellationSource>,
    out_tris: &mut Vec<[u32; 3]>,
) -> Result<(), YangError> {
    use std::collections::BTreeSet;
    use std::f64::consts::PI;
    let malformed = |m: String| YangError::MalformedTopology(m);
    // KV14 Slice F: a boolean-result torus lateral carrying inner loops is a
    // POLOIDAL PERIODIC BAND — the boundary wraps fully around the tube (poloidal
    // φ) while the toroidal θ is bounded (probe KV14_TORUS_PROBE:
    // R0028/R0059/R0026/R0051). A torus is NOT ruled in the toroidal direction,
    // so it needs interior toroidal rings sampled onto the surface — a STRUCTURED
    // (θ × φ) grid, not a flat unroll+CDT (which would chord the sweep). The
    // hole-free structured (2 profiles + seam) arm below is left untouched.
    //
    // KV14 Slice F-3 (R0032): a torus lateral with NO inner loop whose outer
    // loop carries none of the structured vocabulary — no full profile circle,
    // no closed equator — is a DISK patch: one non-wrapping loop of
    // `LineSegment` chords / open arcs (R0032: the previous boolean's 57-chord
    // torus∩cone polyline, a degree-8 curve with no analytic curve type). It
    // re-enters through the same UV-CDT consumer, whose 0-wrapping branch fills
    // the loop's interior on the (u, v) chart (the unwrap keeps the branch cuts
    // away from the loop); a wrapping loop, or one bounding the torus's
    // complement, declines there — typed, not guessed.
    if torus_face_takes_patch_path(f, edges, major, minor) {
        return tessellate_torus_band(
            f_idx, f, edges, rim_rings, center, axis_dir, major, minor, out_verts, sources,
            out_tris,
        );
    }
    let cen = center.as_array();
    let ax = normalize3(axis_dir.as_array());
    let (e1v, e2v) = ortho_basis(axis_dir);
    let (e1, e2) = (e1v.as_array(), e2v.as_array());
    let dot = |a: [f64; 3], b: [f64; 3]| a[0] * b[0] + a[1] * b[1] + a[2] * b[2];
    let band = 1e-9 * (1.0 + major + minor);

    // Classify the boundary edges: 2 profile circles (closed, radius ≈ minor)
    // + 1 seam arc (open, radius ≈ major+minor) — the partial tube — OR
    // 1 profile circle + 1 closed outer-equator circle (the CLOSED torus,
    // KV6d full turn, spec `kv6d_closed_torus_revolve.md`).
    let mut profiles: Vec<u32> = Vec::new();
    let mut seam: Option<u32> = None;
    let mut equator: Option<u32> = None;
    let mut seen = BTreeSet::new();
    for &e in f.outer_loop.iter() {
        if !seen.insert(e) {
            continue;
        }
        if let Curve::Circle { radius, .. } = edges[e as usize].curve {
            let ed = &edges[e as usize];
            if ed.start == ed.end && (radius - minor).abs() <= band {
                profiles.push(e);
            } else if ed.start != ed.end && (radius - (major + minor)).abs() <= band {
                seam = Some(e);
            } else if ed.start == ed.end && (radius - (major + minor)).abs() <= band {
                equator = Some(e);
            }
        }
    }
    if let (None, 1, Some(eq_e)) = (seam, profiles.len(), equator) {
        return tessellate_torus_closed(
            f_idx,
            f,
            edges,
            rim_rings,
            center,
            axis_dir,
            major,
            minor,
            profiles[0],
            eq_e,
            out_verts,
            sources,
            out_tris,
        );
    }
    let (Some(seam_e), 2) = (seam, profiles.len()) else {
        return Err(malformed(format!(
            "face {f_idx}: torus face needs 2 profile circles + 1 seam arc (got {} circles)",
            profiles.len()
        )));
    };
    let seam_chain = rim_rings
        .get(&seam_e)
        .ok_or_else(|| malformed(format!("face {f_idx}: seam chain {seam_e} not built")))?;
    let n_theta = seam_chain.len() - 1;
    if n_theta < 1 {
        return Err(malformed(format!(
            "face {f_idx}: torus seam chain too short"
        )));
    }
    let (seam_start, seam_end) = (edges[seam_e as usize].start, edges[seam_e as usize].end);
    let prof0_e = *profiles
        .iter()
        .find(|&&p| edges[p as usize].start == seam_start)
        .ok_or_else(|| malformed(format!("face {f_idx}: no θ=0 profile at the seam start")))?;
    let profa_e = *profiles
        .iter()
        .find(|&&p| edges[p as usize].start == seam_end)
        .ok_or_else(|| malformed(format!("face {f_idx}: no θ=α profile at the seam end")))?;
    let ring0 = rim_rings
        .get(&prof0_e)
        .ok_or_else(|| malformed(format!("face {f_idx}: θ=0 profile ring not built")))?;
    let ringa = rim_rings
        .get(&profa_e)
        .ok_or_else(|| malformed(format!("face {f_idx}: θ=α profile ring not built")))?;
    let n_phi = ring0.len();
    if ringa.len() != n_phi || n_phi < 3 {
        return Err(malformed(format!(
            "face {f_idx}: torus profile rings mismatched / too few ({n_phi} vs {})",
            ringa.len()
        )));
    }

    // Intrinsic profile angle φ of a mesh vertex (the shared poloidal
    // convention — also `collect_ring_crossings`' torus projection).
    let phi_of = |out_verts: &[Point3], vi: u32| -> f64 {
        let p = out_verts[vi as usize].as_array();
        let d = [p[0] - cen[0], p[1] - cen[1], p[2] - cen[2]];
        let tau = dot(d, ax);
        let radial = [d[0] - tau * ax[0], d[1] - tau * ax[1], d[2] - tau * ax[2]];
        let rho = dot(radial, radial).sqrt();
        tau.atan2(rho - major)
    };
    // Historical UNIFORM path first, byte-identical (spec
    // `m8_torus_profile_rim_crossing` B5): both rings slot-align on uniform
    // 2π/n_phi rounding ⇒ the pre-#131 grid, bit-for-bit. Only when a ring
    // carries NON-uniform samples (a Stage-0 rim-crossing override) does
    // the φ-value column path below take over (B6).
    let phi_slot = |out_verts: &[Point3], vi: u32| -> usize {
        let phi = phi_of(out_verts, vi).rem_euclid(2.0 * PI);
        ((phi / (2.0 * PI / n_phi as f64)).round() as usize) % n_phi
    };
    let uniform_rows: Option<(Vec<u32>, Vec<u32>)> = {
        let assign = |ring: &[u32]| -> Option<Vec<u32>> {
            let mut row = vec![u32::MAX; n_phi];
            for &v in ring {
                let s = phi_slot(out_verts, v);
                if row[s] != u32::MAX {
                    return None; // slot collision — non-uniform sampling
                }
                row[s] = v;
            }
            (!row.contains(&u32::MAX)).then_some(row)
        };
        match (assign(ring0), assign(ringa)) {
            (Some(r0), Some(ra)) => Some((r0, ra)),
            _ => None,
        }
    };
    let (row0, rowa, interior_phi): (Vec<u32>, Vec<u32>, Vec<f64>) =
        if let Some((row0, rowa)) = uniform_rows {
            // Historical interior column angles (bit-identical to pre-#131).
            let phis = (0..n_phi)
                .map(|j| 2.0 * PI * (j as f64) / (n_phi as f64))
                .collect();
            (row0, rowa, phis)
        } else {
            // Task #131 (spec `m8_torus_profile_rim_crossing` §1.3): the grid
            // columns are ring0's ACTUAL intrinsic φ values, anchored at the
            // seam (ring0[0] — the seam-arc endpoint on the outer equator,
            // offset 0 by construction) — a Stage-0 rim-crossing override
            // inserts non-uniform profile samples the uniform slots cannot
            // represent.
            let base = phi_of(out_verts, ring0[0]);
            let offset_of =
                |out_verts: &[Point3], vi: u32| (phi_of(out_verts, vi) - base).rem_euclid(2.0 * PI);
            let mut cols: Vec<(f64, u32)> = ring0
                .iter()
                .map(|&v| (offset_of(out_verts, v), v))
                .collect();
            cols.sort_by(|a, b| a.0.total_cmp(&b.0));
            if cols[0].1 != ring0[0] || cols[0].0 != 0.0 {
                return Err(malformed(format!(
                    "face {f_idx}: torus profile ring seam anchor is not at φ-offset 0"
                )));
            }
            let col_angles: Vec<f64> = cols.iter().map(|&(o, _)| o).collect();
            let row0: Vec<u32> = cols.iter().map(|&(_, v)| v).collect();
            // Match the θ=α ring to the columns INDEX-WISE on the sorted
            // offsets: both rings are seam-anchored (`ring[0]` is the seam-arc
            // endpoint — force ITS offset to exactly 0, since its own atan2
            // can compute 2π−ε across the wrap) and carry one sample per
            // column (the paired Stage-0 insertion), so the sorted sequences
            // correspond 1:1. A fixed ULP band guards each pair — NOT a
            // min-gap-derived tolerance, which a femto-close crossing twin
            // pair would collapse below legitimate cross-ring rounding
            // (R0050: Δφ ≈ 9e-16 vs a 4e-16 twin gap).
            let band = 1e-9 * (1.0 + 2.0 * PI);
            let mut a_off: Vec<(f64, u32)> = ringa
                .iter()
                .enumerate()
                .map(|(k, &v)| {
                    let o = if k == 0 { 0.0 } else { offset_of(out_verts, v) };
                    (o, v)
                })
                .collect();
            a_off.sort_by(|x, y| x.0.total_cmp(&y.0));
            let mut rowa: Vec<u32> = Vec::with_capacity(n_phi);
            for (j, &(o, v)) in a_off.iter().enumerate() {
                let d0 = (o - col_angles[j]).abs();
                let d = d0.min(2.0 * PI - d0);
                if d > band {
                    return Err(malformed(format!(
                        "face {f_idx}: torus profile rings are not φ-aligned \
                     (vertex offset {o} vs column {} beyond band {band})",
                        col_angles[j]
                    )));
                }
                rowa.push(v);
            }
            let phis = col_angles.iter().map(|&o| base + o).collect();
            (row0, rowa, phis)
        };

    // Build the full (n_theta+1) × n_phi grid of vertex indices.
    let mut grid: Vec<Vec<u32>> = Vec::with_capacity(n_theta + 1);
    #[allow(clippy::needless_range_loop)]
    for i in 0..=n_theta {
        if i == 0 {
            grid.push(row0.clone());
        } else if i == n_theta {
            grid.push(rowa.clone());
        } else {
            let s = seam_chain[i];
            let p = out_verts[s as usize].as_array();
            let d = [p[0] - cen[0], p[1] - cen[1], p[2] - cen[2]];
            let theta = dot(d, e2).atan2(dot(d, e1));
            let (st, ct) = theta.sin_cos();
            let mut row = vec![0u32; n_phi];
            row[0] = s; // φ=0 column reuses the seam chain
            for (j, slot) in row.iter_mut().enumerate().skip(1) {
                let phi = interior_phi[j];
                let rad = major + minor * phi.cos();
                let sp = minor * phi.sin();
                let pt = [
                    cen[0] + rad * (ct * e1[0] + st * e2[0]) + sp * ax[0],
                    cen[1] + rad * (ct * e1[1] + st * e2[1]) + sp * ax[1],
                    cen[2] + rad * (ct * e1[2] + st * e2[2]) + sp * ax[2],
                ];
                let vi = out_verts.len() as u32;
                out_verts.push(Point3::new(pt[0], pt[1], pt[2]));
                sources.push(TessellationSource::BRepFace {
                    face: f_idx as u32,
                    u: phi,
                    v: theta,
                });
                *slot = vi;
            }
            grid.push(row);
        }
    }

    // Emit quads, each triangle wound to agree with the torus outward normal
    // (direction from the nearest tube-center-circle point).
    let emit = |a: u32, b: u32, c: u32, out_verts: &[Point3], out_tris: &mut Vec<[u32; 3]>| {
        let pa = out_verts[a as usize].as_array();
        let pb = out_verts[b as usize].as_array();
        let pc = out_verts[c as usize].as_array();
        let e_a = [pb[0] - pa[0], pb[1] - pa[1], pb[2] - pa[2]];
        let e_b = [pc[0] - pa[0], pc[1] - pa[1], pc[2] - pa[2]];
        let gn = [
            e_a[1] * e_b[2] - e_a[2] * e_b[1],
            e_a[2] * e_b[0] - e_a[0] * e_b[2],
            e_a[0] * e_b[1] - e_a[1] * e_b[0],
        ];
        let ctr = [
            (pa[0] + pb[0] + pc[0]) / 3.0,
            (pa[1] + pb[1] + pc[1]) / 3.0,
            (pa[2] + pb[2] + pc[2]) / 3.0,
        ];
        let d = [ctr[0] - cen[0], ctr[1] - cen[1], ctr[2] - cen[2]];
        let tau = dot(d, ax);
        let rv = [d[0] - tau * ax[0], d[1] - tau * ax[1], d[2] - tau * ax[2]];
        let rl = dot(rv, rv).sqrt().max(1e-300);
        let rhat = [rv[0] / rl, rv[1] / rl, rv[2] / rl];
        let on = [
            ctr[0] - (cen[0] + major * rhat[0]),
            ctr[1] - (cen[1] + major * rhat[1]),
            ctr[2] - (cen[2] + major * rhat[2]),
        ];
        if dot(gn, on) >= 0.0 {
            out_tris.push([a, b, c]);
        } else {
            out_tris.push([a, c, b]);
        }
    };
    for i in 0..n_theta {
        for j in 0..n_phi {
            let jn = (j + 1) % n_phi;
            let (a, b, c, d) = (grid[i][j], grid[i][jn], grid[i + 1][jn], grid[i + 1][j]);
            emit(a, b, c, out_verts, out_tris);
            emit(a, c, d, out_verts, out_tris);
        }
    }
    Ok(())
}

/// KV6d closed torus (spec `kv6d_closed_torus_revolve.md`): tessellate the
/// CLOSED `Surface::Torus` face — outer loop = 1 closed poloidal PROFILE
/// circle (radius ≈ minor) + 1 closed toroidal OUTER-EQUATOR circle (radius
/// ≈ major+minor), both anchored at the shared seam vertex — as a doubly
/// periodic (θ × φ) bijective grid (both index directions wrap; no ring is
/// duplicated, so the mesh is watertight by construction).
///
/// Rows: the profile ring is the θ = θ₀ meridian; the equator ring supplies
/// one row per sample (its Steiner points are exactly the φ = 0 column).
/// Columns: the profile ring's ACTUAL intrinsic φ values, seam-anchored
/// (the #131 φ-value convention — correct for both uniform and overridden
/// rings; this arm is new, so there is no historical uniform grid to
/// reproduce byte-for-byte). Interior points are fresh
/// `BRepFace { u = φ, v = θ }` sources (the torus `face_eval` arm).
#[allow(clippy::too_many_arguments)]
pub(crate) fn tessellate_torus_closed(
    f_idx: usize,
    _f: &BRepFace,
    _edges: &[BRepEdge],
    rim_rings: &std::collections::BTreeMap<u32, Vec<u32>>,
    center: Point3,
    axis_dir: Vector3,
    major: f64,
    minor: f64,
    prof_e: u32,
    eq_e: u32,
    out_verts: &mut Vec<Point3>,
    sources: &mut Vec<TessellationSource>,
    out_tris: &mut Vec<[u32; 3]>,
) -> Result<(), YangError> {
    use std::f64::consts::PI;
    let malformed = |m: String| YangError::MalformedTopology(m);
    let cen = center.as_array();
    let ax = normalize3(axis_dir.as_array());
    let (e1v, e2v) = ortho_basis(axis_dir);
    let (e1, e2) = (e1v.as_array(), e2v.as_array());
    let dot = |a: [f64; 3], b: [f64; 3]| a[0] * b[0] + a[1] * b[1] + a[2] * b[2];

    let ring0 = rim_rings
        .get(&prof_e)
        .ok_or_else(|| malformed(format!("face {f_idx}: closed-torus profile ring not built")))?;
    let eq_ring = rim_rings
        .get(&eq_e)
        .ok_or_else(|| malformed(format!("face {f_idx}: closed-torus equator ring not built")))?;
    let n_phi = ring0.len();
    let n_theta = eq_ring.len();
    if n_phi < 3 || n_theta < 3 {
        return Err(malformed(format!(
            "face {f_idx}: closed-torus rings too short ({n_phi} × {n_theta})"
        )));
    }
    if ring0[0] != eq_ring[0] {
        return Err(malformed(format!(
            "face {f_idx}: closed-torus rings do not share the seam anchor"
        )));
    }

    // Intrinsic poloidal φ (the shared convention with `tessellate_torus_face`).
    let phi_of = |out_verts: &[Point3], vi: u32| -> f64 {
        let p = out_verts[vi as usize].as_array();
        let d = [p[0] - cen[0], p[1] - cen[1], p[2] - cen[2]];
        let tau = dot(d, ax);
        let radial = [d[0] - tau * ax[0], d[1] - tau * ax[1], d[2] - tau * ax[2]];
        let rho = dot(radial, radial).sqrt();
        tau.atan2(rho - major)
    };
    // Column table: seam-anchored sorted intrinsic offsets (#131 φ-value path).
    let base = phi_of(out_verts, ring0[0]);
    let mut cols: Vec<(f64, u32)> = ring0
        .iter()
        .enumerate()
        .map(|(k, &v)| {
            let o = if k == 0 {
                0.0
            } else {
                (phi_of(out_verts, v) - base).rem_euclid(2.0 * PI)
            };
            (o, v)
        })
        .collect();
    cols.sort_by(|a, b| a.0.total_cmp(&b.0));
    if cols[0].1 != ring0[0] {
        return Err(malformed(format!(
            "face {f_idx}: closed-torus profile ring seam anchor is not at φ-offset 0"
        )));
    }
    let col_angles: Vec<f64> = cols.iter().map(|&(o, _)| o).collect();
    let row0: Vec<u32> = cols.iter().map(|&(_, v)| v).collect();

    // Rows: profile ring at the anchor azimuth, then one row per equator
    // sample (in ring order — cyclically monotone by construction).
    let mut grid: Vec<Vec<u32>> = Vec::with_capacity(n_theta);
    grid.push(row0);
    for &s in &eq_ring[1..] {
        let p = out_verts[s as usize].as_array();
        let d = [p[0] - cen[0], p[1] - cen[1], p[2] - cen[2]];
        let theta = dot(d, e2).atan2(dot(d, e1));
        let (st, ct) = theta.sin_cos();
        let mut row = vec![0u32; n_phi];
        row[0] = s; // φ = 0 column reuses the equator ring
        for (j, slot) in row.iter_mut().enumerate().skip(1) {
            let phi = base + col_angles[j];
            let rad = major + minor * phi.cos();
            let sp = minor * phi.sin();
            let pt = [
                cen[0] + rad * (ct * e1[0] + st * e2[0]) + sp * ax[0],
                cen[1] + rad * (ct * e1[1] + st * e2[1]) + sp * ax[1],
                cen[2] + rad * (ct * e1[2] + st * e2[2]) + sp * ax[2],
            ];
            let vi = out_verts.len() as u32;
            out_verts.push(Point3::new(pt[0], pt[1], pt[2]));
            sources.push(TessellationSource::BRepFace {
                face: f_idx as u32,
                u: phi,
                v: theta,
            });
            *slot = vi;
        }
        grid.push(row);
    }

    // Emit quads with BOTH directions wrapped, each triangle wound to agree
    // with the torus outward normal (same rule as `tessellate_torus_face`).
    let emit = |a: u32, b: u32, c: u32, out_verts: &[Point3], out_tris: &mut Vec<[u32; 3]>| {
        let pa = out_verts[a as usize].as_array();
        let pb = out_verts[b as usize].as_array();
        let pc = out_verts[c as usize].as_array();
        let e_a = [pb[0] - pa[0], pb[1] - pa[1], pb[2] - pa[2]];
        let e_b = [pc[0] - pa[0], pc[1] - pa[1], pc[2] - pa[2]];
        let gn = [
            e_a[1] * e_b[2] - e_a[2] * e_b[1],
            e_a[2] * e_b[0] - e_a[0] * e_b[2],
            e_a[0] * e_b[1] - e_a[1] * e_b[0],
        ];
        let ctr = [
            (pa[0] + pb[0] + pc[0]) / 3.0,
            (pa[1] + pb[1] + pc[1]) / 3.0,
            (pa[2] + pb[2] + pc[2]) / 3.0,
        ];
        let d = [ctr[0] - cen[0], ctr[1] - cen[1], ctr[2] - cen[2]];
        let tau = dot(d, ax);
        let rv = [d[0] - tau * ax[0], d[1] - tau * ax[1], d[2] - tau * ax[2]];
        let rl = dot(rv, rv).sqrt().max(1e-300);
        let rhat = [rv[0] / rl, rv[1] / rl, rv[2] / rl];
        let on = [
            ctr[0] - (cen[0] + major * rhat[0]),
            ctr[1] - (cen[1] + major * rhat[1]),
            ctr[2] - (cen[2] + major * rhat[2]),
        ];
        if dot(gn, on) >= 0.0 {
            out_tris.push([a, b, c]);
        } else {
            out_tris.push([a, c, b]);
        }
    };
    for i in 0..n_theta {
        let inx = (i + 1) % n_theta;
        for j in 0..n_phi {
            let jn = (j + 1) % n_phi;
            let (a, b, c, d) = (grid[i][j], grid[i][jn], grid[inx][jn], grid[inx][j]);
            emit(a, b, c, out_verts, out_tris);
            emit(a, c, d, out_verts, out_tris);
        }
    }
    Ok(())
}

/// Unwrap a periodic angle sequence in place: each value is shifted by a
/// multiple of 2π so successive entries never jump by more than π (standard
/// phase unwrapping). Turns a torus patch's `atan2` parameters into a simple
/// (non-self-crossing) polygon as long as the patch does not wrap the whole way
/// around a seam.
pub(crate) fn unwrap_seq(a: &mut [f64]) {
    use std::f64::consts::{PI, TAU};
    for k in 1..a.len() {
        while a[k] - a[k - 1] > PI {
            a[k] -= TAU;
        }
        while a[k - 1] - a[k] > PI {
            a[k] += TAU;
        }
    }
}

/// KV6d UV-CDT consumer: tessellate an arbitrary torus PATCH from its ordered
/// closed 3D boundary loop `boundary` (the mesh-boolean intersection curve +
/// seam, finely sampled) via the interior-Steiner CDT
/// ([`cherchi_rs::cdt_polygon_with_holes_refined`]).
///
/// Pipeline: invert each boundary point into the `(u = meridian, v = longitude)`
/// parameter plane — `face_eval(u,v) = center + (R + r·cos u)·(cos v·ê1 +
/// sin v·ê2) + r·sin u·â`, so `v = atan2(w·ê2, w·ê1)` and `u = atan2(τ, ρ − R)`
/// (τ axial, ρ radial-from-axis) — angle-UNWRAP `u` and `v` to a simple polygon,
/// SCALE to ~arc-length isotropy (`u·r`, `v·R`) so a Delaunay-quality triangle
/// maps to a well-shaped 3D one, REFINE to `max_3d_area`, then MAP BACK: BOUNDARY
/// verts keep their EXACT input 3D coordinates (conformal with the neighbouring
/// patch — the boolean's shared intersection-curve samples), interior STEINER
/// verts are `face_eval` of their unscaled `(u,v)` (exactly on the torus).
///
/// Returns `(verts, tris)` with `verts[0..boundary.len()] == boundary` bit-for-
/// bit. `None` if the boundary degenerates or the CDT rejects a self-intersecting
/// projection (a seam-CROSSING / self-overlapping patch — out of this v1 scope;
/// the deferral is loud at the caller).
///
/// Consumed by kernel-v2's render-time torus-patch tessellation (the owner of
/// output recovery re-tessellation); exposed `pub` for that cross-crate call.
/// One boundary loop of a torus patch projected into the continuous, scaled
/// meridian/longitude plane (`su = u·minor`, `sv = v·major`), with its net
/// MERIDIAN winding `wu` (in 2π units). `wu != 0` ⇒ the loop wraps the meridian
/// seam (a band edge); the longitude is required non-wrapping (partial torus).
pub(crate) struct PLoop {
    su: Vec<f64>,
    sv: Vec<f64>,
    pts: Vec<Point3>,
    wu: i64,
}

/// KV6d seam-wrapping render — the cylinder patch's pass-2 case-2 ported to the
/// torus meridian. Unrolls a BAND bounded by two oppositely-meridian-wrapping
/// loops `pc` (wrap +1) and `mc` (wrap −1) into a single simple ring in the
/// universal cover: walk `pc` over one full period (`su` increasing by `span`),
/// bridge to `mc`, walk `mc` reversed (`su` decreasing), bridge back. The seam
/// bridges connect an anchor pair chosen so neither bridge crosses a boundary
/// edge; the duplicated anchor vertices coincide in 3D (the meridian is
/// periodic) so the result is watertight. Returns the ring as `(su, sv, 3d)` per
/// vertex, or `None` if no unblocked bridge exists.
pub(crate) fn band_seam_bridge<F: Fn(f64, f64) -> Point3>(
    pc: &PLoop,
    mc: &PLoop,
    span: f64,
    minor: f64,
    major: f64,
    windows: &[(f64, f64)],
    eval: F,
) -> Option<Vec<(f64, f64, Point3)>> {
    let (m, mm) = (pc.su.len(), mc.su.len());
    if m < 3 || mm < 3 {
        return None;
    }
    let principal = |d: f64| d - (d / span).round() * span;
    // Proper (open) segment crossing in the (su, sv) plane.
    let seg_cross = |a: [f64; 2], b: [f64; 2], c: [f64; 2], d: [f64; 2]| -> bool {
        let o = |p: [f64; 2], q: [f64; 2], r: [f64; 2]| {
            (q[0] - p[0]) * (r[1] - p[1]) - (q[1] - p[1]) * (r[0] - p[0])
        };
        o(a, c, d) * o(b, c, d) < 0.0 && o(a, b, c) * o(a, b, d) < 0.0
    };
    // Each seam bridge spans the full longitude gap (v0 → v1) as a SINGLE
    // segment; left unsplit, the CDT cannot place Steiner near it and the edge
    // region stays coarse → large chord error on the curved tube. Subdivide it
    // into `k` segments matching the meridian sampling density, with each
    // intermediate point ON the torus at the seam meridian (so the left- and
    // right-seam copies, one period apart in u, coincide in 3D → watertight).
    let mean_step = span / m as f64;
    // Interpolate a bridge from `(su_a, sv_a)` to `(su_b, sv_b)` at the seam
    // meridian `u_seam`, returning the K−1 interior points (2D + on-torus 3D).
    let bridge_pts =
        |su_a: f64, sv_a: f64, su_b: f64, sv_b: f64, u_seam: f64| -> Vec<(f64, f64, Point3)> {
            let dsv = (sv_b - sv_a).abs();
            let k = (dsv / mean_step).ceil().max(1.0) as usize;
            let mut out = Vec::with_capacity(k.saturating_sub(1));
            for i in 1..k {
                let t = i as f64 / k as f64;
                let su = su_a + (su_b - su_a) * t;
                let sv = sv_a + (sv_b - sv_a) * t;
                out.push((su, sv, eval(u_seam, sv / major)));
            }
            out
        };
    let build_ring = |xi: usize, yi: usize, dpr: f64| -> Vec<(f64, f64, Point3)> {
        let mut ring: Vec<(f64, f64, Point3)> = Vec::with_capacity(m + mm + 2);
        let base_x = pc.su[xi];
        for j in 0..=m {
            let idx = (xi + j) % m;
            let mut u = pc.su[idx];
            if j > 0 && (xi + j) >= m {
                u += span;
            }
            if j == m {
                u = base_x + span;
            }
            ring.push((u, pc.sv[idx], pc.pts[idx]));
        }
        let y_target = base_x + span + dpr;
        let y_base = mc.su[yi];
        let wsign = mc.wu as f64; // −1
                                  // Bridge 1: pc closing point (base_x+span, v0) → mc start (y_target, v1),
                                  // at the right-seam meridian (base_x+span)/minor.
        ring.extend(bridge_pts(
            base_x + span,
            pc.sv[xi],
            y_target,
            mc.sv[yi],
            (base_x + span) / minor,
        ));
        for j in 0..=mm {
            let idx = (yi + j) % mm;
            let mut u = mc.su[idx];
            if j > 0 && (yi + j) >= mm {
                u += wsign * span;
            }
            if j == mm {
                u = y_base + wsign * span;
            }
            ring.push((u - y_base + y_target, mc.sv[idx], mc.pts[idx]));
        }
        // Bridge 2: mc closing point (≈ base_x+dpr, v1) → pc start (base_x, v0),
        // at the left-seam meridian base_x/minor (= right meridian − 2π in u, so
        // `eval` returns the SAME 3D points as bridge 1 → watertight seam).
        ring.extend(bridge_pts(
            base_x + dpr,
            mc.sv[yi],
            base_x,
            pc.sv[xi],
            base_x / minor,
        ));
        ring
    };
    // A non-wrapping window straddles the seam cut (at u ≡ base_x mod span) iff a
    // seam line falls strictly inside its u-interval [wlo, whi] — i.e. Slice F-2's
    // "window on the seam" split (the R0063/cylinder wall on the torus). Prefer an
    // anchor whose seam splits no window; if none exists, fall back to the first
    // non-crossing anchor (a genuinely seam-crossing patch, out of scope).
    let straddles_window = |base_x: f64| -> bool {
        windows.iter().any(|&(wlo, whi)| {
            let d = (base_x - wlo).rem_euclid(span);
            d > 1e-12 && d < (whi - wlo) - 1e-12
        })
    };
    let mut fallback: Option<(usize, usize, f64)> = None;
    // Pick the anchor pair (xi on pc, yi on mc) whose two seam-bridge SPANS
    // (validated as single segments, before subdivision) cross no boundary edge.
    for xi in 0..m {
        let xu = pc.su[xi];
        let mut best: Option<(usize, f64)> = None;
        for (yi, &syu) in mc.su.iter().enumerate() {
            let dpr = principal(syu - xu);
            if best.is_none_or(|(_, b)| dpr.abs() < b.abs()) {
                best = Some((yi, dpr));
            }
        }
        let (yi, dpr) = best?;
        let base_x = pc.su[xi];
        // The two seam spans (right: pc→mc; left: mc→pc), in the unrolled frame.
        let spans = [
            ([base_x + span, pc.sv[xi]], [base_x + span + dpr, mc.sv[yi]]),
            ([base_x + dpr, mc.sv[yi]], [base_x, pc.sv[xi]]),
        ];
        // Boundary edges to test against: the pc bottom chain (at sv=pc.sv) and
        // the mc top chain (at sv=mc.sv), in the bridge's local frame.
        let bottom: Vec<[f64; 2]> = (0..=m)
            .map(|j| {
                let idx = (xi + j) % m;
                let mut u = pc.su[idx];
                if (j > 0 && (xi + j) >= m) || j == m {
                    u = if j == m { base_x + span } else { u + span };
                }
                [u, pc.sv[idx]]
            })
            .collect();
        let y_target = base_x + span + dpr;
        let y_base = mc.su[yi];
        let wsign = mc.wu as f64;
        let top: Vec<[f64; 2]> = (0..=mm)
            .map(|j| {
                let idx = (yi + j) % mm;
                let mut u = mc.su[idx];
                if (j > 0 && (yi + j) >= mm) || j == mm {
                    u = if j == mm {
                        y_base + wsign * span
                    } else {
                        u + wsign * span
                    };
                }
                [u - y_base + y_target, mc.sv[idx]]
            })
            .collect();
        let crosses = |p: [f64; 2], q: [f64; 2]| -> bool {
            if p == q {
                return true;
            }
            let chain_hit =
                |chain: &[[f64; 2]]| chain.windows(2).any(|w| seg_cross(p, q, w[0], w[1]));
            chain_hit(&bottom) || chain_hit(&top)
        };
        if spans.iter().any(|&(p, q)| crosses(p, q)) {
            continue;
        }
        // Non-crossing anchor: take it immediately if it also splits no window;
        // otherwise remember it as the fallback and keep looking for a clean seam.
        if !straddles_window(base_x) {
            return Some(build_ring(xi, yi, dpr));
        }
        if fallback.is_none() {
            fallback = Some((xi, yi, dpr));
        }
    }
    fallback.map(|(xi, yi, dpr)| build_ring(xi, yi, dpr))
}

/// `reversed`: the face's outward normal is the torus's INWARD normal (a
/// bore). It decides which side of a BAND's `+1`-meridian-wrapping rim the
/// band lies on (see the band case below); it has no effect on a bounded
/// (non-wrapping) patch, whose loop fixes its own region.
#[allow(clippy::too_many_arguments)]
pub fn tessellate_torus_patch(
    center: Point3,
    axis_dir: Vector3,
    major: f64,
    minor: f64,
    boundary: &[Point3],
    holes: &[Vec<Point3>],
    max_3d_area: f64,
    reversed: bool,
) -> Option<(Vec<Point3>, Vec<[u32; 3]>)> {
    // Env-gated decline probe (zero-cost off): the torus patch waller names
    // its site — a `None` here surfaces downstream as one shared wall string
    // ("torus patch UV-CDT failed"), which cannot self-localize.
    let probe = std::env::var_os("YANG_TORUS_PATCH_PROBE").is_some();
    if probe {
        eprintln!(
            "[torus-patch] boundary={} holes={} major={major:.6e} minor={minor:.6e}",
            boundary.len(),
            holes.len()
        );
    }
    if boundary.len() < 3 || major <= 0.0 || minor <= 0.0 {
        if probe {
            eprintln!("[torus-patch] DECLINE degenerate inputs");
        }
        return None;
    }
    let ax = normalize3(axis_dir.as_array());
    let (e1, e2) = ortho_basis(axis_dir);
    let (e1a, e2a) = (e1.as_array(), e2.as_array());
    let c = center.as_array();
    let dot = |a: [f64; 3], b: [f64; 3]| a[0] * b[0] + a[1] * b[1] + a[2] * b[2];
    let eval = |u: f64, v: f64| -> Point3 {
        let (cu, su) = (u.cos(), u.sin());
        let (cv, sv) = (v.cos(), v.sin());
        let rad = major + minor * cu;
        Point3::new(
            c[0] + rad * (cv * e1a[0] + sv * e2a[0]) + minor * su * ax[0],
            c[1] + rad * (cv * e1a[1] + sv * e2a[1]) + minor * su * ax[1],
            c[2] + rad * (cv * e1a[2] + sv * e2a[2]) + minor * su * ax[2],
        )
    };
    let snap = |a: f64| (a / 1e-12).round() * 1e-12;
    // Invert ONE loop into (u, v): project, unwrap each angle to a simple
    // polygon, and condition to the TAU_WORK (1e-12 rad) grid. The atan2/cos/sin
    // inversion introduces ~1 ULP noise; on a straight seam run (constant u or
    // v) that faint kink makes spade over-refine into a sliver storm — snapping
    // removes it without perturbing output geometry (boundary verts are emitted
    // from the EXACT input 3D below; Steiner verts are `face_eval`, on-torus).
    let invert = |loop_pts: &[Point3]| -> (Vec<f64>, Vec<f64>) {
        let mut us = Vec::with_capacity(loop_pts.len());
        let mut vs = Vec::with_capacity(loop_pts.len());
        for &p in loop_pts {
            let pa = p.as_array();
            let w = [pa[0] - c[0], pa[1] - c[1], pa[2] - c[2]];
            let tau = dot(w, ax);
            let radial = [w[0] - tau * ax[0], w[1] - tau * ax[1], w[2] - tau * ax[2]];
            let wx = dot(radial, e1a);
            let wy = dot(radial, e2a);
            let rho = (wx * wx + wy * wy).sqrt();
            vs.push(wy.atan2(wx));
            us.push(tau.atan2(rho - major));
        }
        unwrap_seq(&mut us);
        unwrap_seq(&mut vs);
        for a in us.iter_mut().chain(vs.iter_mut()) {
            *a = snap(*a);
        }
        (us, vs)
    };

    // Net winding of a closed loop's already-unwrapped angle sequence: the span
    // plus the closure step, in units of 2π. Nonzero ⇒ the loop wraps that
    // periodic coordinate (a meridian- or longitude-wrapping patch).
    let net_wrap = |a: &[f64]| -> i64 {
        if a.len() < 2 {
            return 0;
        }
        let raw_closure = a[0] - a[a.len() - 1];
        // wrap_to_pi(raw_closure)
        let mut c = raw_closure % std::f64::consts::TAU;
        if c > std::f64::consts::PI {
            c -= std::f64::consts::TAU;
        } else if c <= -std::f64::consts::PI {
            c += std::f64::consts::TAU;
        }
        let net = (a[a.len() - 1] - a[0]) + c;
        (net / std::f64::consts::TAU).round() as i64
    };

    use std::f64::consts::TAU;
    // ---- Pass 1: project every loop into the continuous, scaled (su, sv)
    // meridian/longitude plane with its net meridian winding `wu`. ----------
    let project = |pts: &[Point3]| -> Option<PLoop> {
        if pts.len() < 3 {
            return None;
        }
        let (us, vs) = invert(pts);
        if net_wrap(&vs) != 0 {
            if probe {
                eprintln!(
                    "[torus-patch] DECLINE longitude wrap {} on a {}-pt loop",
                    net_wrap(&vs),
                    pts.len()
                );
            }
            return None; // a LONGITUDE wrap is a full-torus seam — out of scope
        }
        let wu = net_wrap(&us);
        Some(PLoop {
            su: us.iter().map(|u| u * minor).collect(),
            sv: vs.iter().map(|v| v * major).collect(),
            pts: pts.to_vec(),
            wu,
        })
    };
    let mut ploops: Vec<PLoop> = Vec::with_capacity(1 + holes.len());
    ploops.push(project(boundary)?);
    for h in holes {
        ploops.push(project(h)?);
    }
    let span = TAU * minor; // one full meridian wrap in scaled u
    let span_v = TAU * major;

    // ---- Pass 2: assemble one simple (su, sv) outer polygon + holes. -------
    let wrapping: Vec<usize> = (0..ploops.len()).filter(|&i| ploops[i].wu != 0).collect();
    let mut verts2d: Vec<cad_primitives::Point2> = Vec::new();
    let mut vert3d: Vec<Point3> = Vec::new();
    let mut hole_idx: Vec<Vec<u32>> = Vec::new();
    let umean = |l: &PLoop| l.su.iter().sum::<f64>() / l.su.len() as f64;
    let vmean = |l: &PLoop| l.sv.iter().sum::<f64>() / l.sv.len() as f64;
    let outer: Vec<u32> = match wrapping.len() {
        0 => {
            // DISK: boundary is the outer loop; holes are the rest, each shifted
            // by whole periods so it sits in the outer's period (a hole atan2
            // placed on the opposite branch would project outside the outer).
            //
            // WHICH REGION (KV14 Slice F-3 — the band's side rule applied to a
            // bounded loop): the CDT fills the polygon's INTERIOR, which is
            // the face only when the loop bounds it with the material on its
            // left about the face's outward normal. In this chart
            // `∂P/∂u × ∂P/∂v = −(R + r·cos u)·r·n̂_out` (`(e1, e2, axis)` is
            // right-handed), so a material-left loop of an OUTWARD face runs
            // CW in (u, v) and one of a `reversed` face runs CCW. A loop in
            // the other sense bounds the COMPLEMENT (the torus minus this
            // disk); filling the interior would emit the wrong region
            // silently — decline instead (typed at the caller).
            let disk = &ploops[0];
            let m = disk.su.len();
            let area2: f64 = (0..m)
                .map(|k| {
                    let j = (k + 1) % m;
                    disk.su[k] * disk.sv[j] - disk.su[j] * disk.sv[k]
                })
                .sum();
            if area2 == 0.0 || (area2 < 0.0) == reversed {
                if probe {
                    eprintln!(
                        "[torus-patch] DECLINE disk loop bounds the complement \
                         (signed area2={area2:.6e}, reversed={reversed})"
                    );
                }
                return None;
            }
            let (u_ref, v_ref) = (umean(&ploops[0]), vmean(&ploops[0]));
            let mut o = Vec::with_capacity(ploops[0].su.len());
            for k in 0..ploops[0].su.len() {
                o.push(verts2d.len() as u32);
                verts2d.push(cad_primitives::Point2::new(
                    ploops[0].su[k],
                    ploops[0].sv[k],
                ));
                vert3d.push(ploops[0].pts[k]);
            }
            for l in &ploops[1..] {
                let du = ((umean(l) - u_ref) / span).round() * span;
                let dv = ((vmean(l) - v_ref) / span_v).round() * span_v;
                let mut hi = Vec::with_capacity(l.su.len());
                for k in 0..l.su.len() {
                    hi.push(verts2d.len() as u32);
                    verts2d.push(cad_primitives::Point2::new(l.su[k] - du, l.sv[k] - dv));
                    vert3d.push(l.pts[k]);
                }
                hole_idx.push(hi);
            }
            o
        }
        2 => {
            // BAND (KV14 Slice F/F-2): two oppositely-meridian-wrapping loops are
            // the band's meridian edges; any REMAINING non-wrapping loops are
            // window holes in the tube wall. The universal-cover seam bridge lays
            // the outer ring across one meridian period; each window is then
            // shifted by whole periods (in both scaled params) to land inside the
            // band's unrolled (su, sv) region and carved as a CDT hole.
            let (a, b) = (wrapping[0], wrapping[1]);
            let (pc, mc) = if ploops[a].wu > 0 {
                (&ploops[a], &ploops[b])
            } else {
                (&ploops[b], &ploops[a])
            };
            if pc.wu + mc.wu != 0 {
                if probe {
                    eprintln!(
                        "[torus-patch] DECLINE band wu mismatch ({} + {})",
                        pc.wu, mc.wu
                    );
                }
                return None;
            }
            // WHICH SIDE of `pc` the band lies on (2026-09-03, the exact-
            // membership oracle's class-A finding — R0091 / R0045 / R0096:
            // a band spanning more than 180° came back as its COMPLEMENT).
            // Each rim's longitude is a principal `atan2` value in (−π, π];
            // the two rims bound TWO candidate bands, and laying the ribbon
            // between the values as they come picks the shorter arc — right
            // below 180°, wrong above it, ambiguous at it. The side is fixed
            // by ORIENTATION, not span: the loops wind material-CCW about
            // the face's OUTWARD normal, and in this chart
            // `∂P/∂u × ∂P/∂v = −(R + r·cos u)·r·n̂_out` (with `(e1, e2, axis)`
            // right-handed), so a material-CCW loop runs CW in (u, v) and
            // the material sits on the traversal's RIGHT: for `pc` (u
            // increasing) that is DECREASING v. A `reversed` face (outward
            // = −n̂_torus) mirrors it. Shift `mc` by WHOLE periods onto that
            // side — a band already on it is untouched bit-for-bit.
            let mean = |a: &[f64]| a.iter().sum::<f64>() / a.len() as f64;
            let (v_pc, v_mc) = (mean(&pc.sv), mean(&mc.sv));
            let t = (v_pc - v_mc) / span_v;
            if (t - t.round()).abs() < 1e-9 {
                if probe {
                    eprintln!("[torus-patch] DECLINE band rims coincide in longitude");
                }
                return None;
            }
            let k = if reversed {
                t.floor() + 1.0
            } else {
                t.ceil() - 1.0
            };
            let shifted;
            let mc: &PLoop = if k == 0.0 {
                mc
            } else {
                shifted = PLoop {
                    su: mc.su.clone(),
                    sv: mc.sv.iter().map(|v| v + k * span_v).collect(),
                    pts: mc.pts.clone(),
                    wu: mc.wu,
                };
                &shifted
            };
            if probe {
                eprintln!(
                    "[torus-patch] band side: reversed={reversed} v_pc={:.6}° v_mc={:.6}° → shifted by {k} period(s) to {:.6}° (span {:.3}°)",
                    (v_pc / major).to_degrees(),
                    (v_mc / major).to_degrees(),
                    (mean(&mc.sv) / major).to_degrees(),
                    ((mean(&mc.sv) - v_pc) / major).to_degrees()
                );
            }
            // Window u-intervals (the non-wrapping loops) so the seam bridge can
            // place its cut where it splits no window (Slice F-2).
            let windows: Vec<(f64, f64)> = ploops
                .iter()
                .filter(|l| l.wu == 0)
                .map(|l| {
                    (
                        l.su.iter().cloned().fold(f64::INFINITY, f64::min),
                        l.su.iter().cloned().fold(f64::NEG_INFINITY, f64::max),
                    )
                })
                .collect();
            let ring = band_seam_bridge(pc, mc, span, minor, major, &windows, eval)?;
            let mut o = Vec::with_capacity(ring.len());
            let (mut umin, mut umax) = (f64::INFINITY, f64::NEG_INFINITY);
            let (mut vmin, mut vmax) = (f64::INFINITY, f64::NEG_INFINITY);
            for (su, sv, p) in &ring {
                umin = umin.min(*su);
                umax = umax.max(*su);
                vmin = vmin.min(*sv);
                vmax = vmax.max(*sv);
                o.push(verts2d.len() as u32);
                verts2d.push(cad_primitives::Point2::new(*su, *sv));
                vert3d.push(*p);
            }
            // Window holes = the non-wrapping loops (band edges already consumed).
            let (u_center, v_center) = (0.5 * (umin + umax), 0.5 * (vmin + vmax));
            for l in ploops.iter().filter(|l| l.wu == 0) {
                let du = ((umean(l) - u_center) / span).round() * span;
                let dv = ((vmean(l) - v_center) / span_v).round() * span_v;
                let mut hi = Vec::with_capacity(l.su.len());
                for k in 0..l.su.len() {
                    hi.push(verts2d.len() as u32);
                    verts2d.push(cad_primitives::Point2::new(l.su[k] - du, l.sv[k] - dv));
                    vert3d.push(l.pts[k]);
                }
                hole_idx.push(hi);
            }
            o
        }
        // 1 wrapping loop (degenerate) or > 2 wraps: out of scope.
        _ => {
            if probe {
                eprintln!(
                    "[torus-patch] DECLINE {} meridian-wrapping loops (need 0 or 2)",
                    wrapping.len()
                );
            }
            return None;
        }
    };

    // Chord-band seeding (2026-08-08, `docs/audits/volume_oracle_flags_anchored.md`
    // §deficit-class): the AREA budget alone leaves interior chords sagging ~8×
    // the render band. Interior grid at the STRUCTURED tessellator's own
    // spacing (kernel-v2 `surfaces/torus.rs`: θ steps keep the minor chord
    // `s = √max_area` at the worst radius, `per = (2π/n)·minor/(major+minor)`),
    // so the patch path carries the same 3D chord — and therefore the same
    // sagitta band — as the structured rings: `su` is exact meridian arc
    // (budget s); `sv = v·major`, so the θ-step budget is
    // `Δsv = per·major = s·major/(major+minor)`.
    let s = max_3d_area.sqrt();
    let (ref_verts, tris) = match cherchi_rs::cdt_polygon_with_holes_refined_seeded(
        &verts2d,
        &outer,
        &hole_idx,
        max_3d_area,
        [s, s * (major / (major + minor))],
    ) {
        Ok(x) => x,
        Err(e) => {
            if probe {
                eprintln!(
                    "[torus-patch] DECLINE CDT: {e:?} (outer={} pts, {} holes)",
                    outer.len(),
                    hole_idx.len()
                );
                for (k, &i) in outer.iter().enumerate() {
                    let q = verts2d[i as usize];
                    let p3 = vert3d[i as usize].as_array();
                    eprintln!(
                        "[torus-patch]   outer[{k}] su={:.9e} sv={:.9e} p=({:.9e},{:.9e},{:.9e})",
                        q.x(),
                        q.y(),
                        p3[0],
                        p3[1],
                        p3[2]
                    );
                }
                for (h, hi) in hole_idx.iter().enumerate() {
                    for (k, &i) in hi.iter().enumerate() {
                        let q = verts2d[i as usize];
                        let p3 = vert3d[i as usize].as_array();
                        eprintln!(
                            "[torus-patch]   hole{h}[{k}] su={:.9e} sv={:.9e} p=({:.9e},{:.9e},{:.9e})",
                            q.x(), q.y(), p3[0], p3[1], p3[2]
                        );
                    }
                }
            }
            return None;
        }
    };

    // Map back: boundary verts → EXACT input 3D; refined Steiner verts (appended
    // after) → `face_eval`. A band's duplicated seam vertices carry the same 3D
    // point on both sides (the meridian is periodic) ⇒ watertight.
    let n = vert3d.len();
    let mut verts3d: Vec<Point3> = Vec::with_capacity(ref_verts.len());
    for (i, sp) in ref_verts.iter().enumerate() {
        if i < n {
            verts3d.push(vert3d[i]);
        } else {
            verts3d.push(eval(sp.x() / minor, sp.y() / major));
        }
    }
    Some((verts3d, tris))
}

/// KV6d increment 2 (spec `kv6d_sphere_revolve.md`): UV-CDT tessellation of a
/// boolean-output SPHERE patch — the render consumer for a trimmed
/// `Surface::Sphere` face (the sphere analog of [`tessellate_torus_patch`]).
///
/// Projects every loop into the scaled `(u·r, v·r)` longitude/latitude plane
/// (the FIXED z-up sphere frame of `tessellate_sphere_face`: `u = atan2`
/// about `ẑ`, `v = asin((z − c_z)/r)`), then:
///
/// - **all loops non-wrapping** → disk + period-shifted holes → refined CDT
///   (boundary vertices pass through bit-for-bit — conformal with the
///   neighboring planar patches);
/// - **the OUTER loop wraps longitude once (`wu = ±1`)** → the patch
///   contains exactly one pole (`wu = +1` → north for an outward face,
///   flipped when `reversed` — the region lies LEFT of a loop traversed CCW
///   around the face's outward normal): bridge the unwrapped boundary to
///   the pole with a two-sided meridian seam whose two copies carry
///   BIT-IDENTICAL 3D sample points, plus a UV bottom edge that degenerates
///   to the single 3D pole point; after the CDT, bit-identical positions
///   are WELDED and 3D-degenerate triangles dropped, closing the seam and
///   the pole fan watertight;
/// - anything else (multi-wrap, wrapping holes, boundary within band of a
///   pole) → `None` (the caller's typed wall; later slice).
///
/// Steiner refinement is interior-only (`keep_constraint_edges`), budgeted
/// by `max_3d_area` in arc-length² (the caller matches its structured-grid
/// chord spacing).
pub fn tessellate_sphere_patch(
    center: Point3,
    radius: f64,
    reversed: bool,
    boundary: &[Point3],
    holes: &[Vec<Point3>],
    max_3d_area: f64,
) -> Option<(Vec<Point3>, Vec<[u32; 3]>)> {
    use std::f64::consts::{FRAC_PI_2, TAU};
    if boundary.len() < 3 || !(radius.is_finite() && radius > 0.0) {
        return None;
    }
    let c = center.as_array();
    let r = radius;
    let eval = |u: f64, v: f64| -> Point3 {
        let (su, cu) = u.sin_cos();
        let (sv, cv) = v.sin_cos();
        Point3::new(c[0] + r * cv * cu, c[1] + r * cv * su, c[2] + r * sv)
    };
    let snap = |a: f64| (a / 1e-12).round() * 1e-12;
    // A boundary vertex too close to a pole makes its longitude meaningless
    // (atan2 of a sub-tolerance radial) — out of scope, loud upstream.
    let polar_band = 1e-9 * r;

    // Invert ONE loop into unwrapped, snapped, SCALED (su, sv) + net u-wrap.
    let invert = |pts: &[Point3]| -> Option<(Vec<f64>, Vec<f64>, i64)> {
        if pts.len() < 3 {
            return None;
        }
        let mut us = Vec::with_capacity(pts.len());
        let mut vs = Vec::with_capacity(pts.len());
        for p in pts {
            let pa = p.as_array();
            let w = [pa[0] - c[0], pa[1] - c[1], pa[2] - c[2]];
            let rho = (w[0] * w[0] + w[1] * w[1]).sqrt();
            if rho < polar_band {
                return None; // boundary touches a pole — later slice
            }
            us.push(w[1].atan2(w[0]));
            vs.push((w[2] / r).clamp(-1.0, 1.0).asin());
        }
        unwrap_seq(&mut us);
        let raw_closure = {
            // wrap_to_pi of the closing step; nonzero net ⇒ longitude wrap.
            let mut cl = (us[0] - us[us.len() - 1]) % TAU;
            if cl > std::f64::consts::PI {
                cl -= TAU;
            } else if cl <= -std::f64::consts::PI {
                cl += TAU;
            }
            cl
        };
        let wu = (((us[us.len() - 1] - us[0]) + raw_closure) / TAU).round() as i64;
        for a in us.iter_mut().chain(vs.iter_mut()) {
            *a = snap(*a);
        }
        Some((
            us.iter().map(|u| u * r).collect(),
            vs.iter().map(|v| v * r).collect(),
            wu,
        ))
    };

    let (b_su, b_sv, b_wu) = invert(boundary)?;
    let mut h_loops: Vec<(Vec<f64>, Vec<f64>)> = Vec::with_capacity(holes.len());
    for h in holes {
        let (hu, hv, hwu) = invert(h)?;
        if hwu != 0 {
            return None; // a wrapping HOLE — later slice
        }
        h_loops.push((hu, hv));
    }
    let span = TAU * r;

    let mut verts2d: Vec<cad_primitives::Point2> = Vec::new();
    let mut vert3d: Vec<Point3> = Vec::new();
    let mut hole_idx: Vec<Vec<u32>> = Vec::new();
    let umean = |su: &[f64]| su.iter().sum::<f64>() / su.len() as f64;

    let outer: Vec<u32> = match b_wu {
        0 => {
            // DISK: boundary is the outer loop; holes shift by whole u-periods
            // into its period (latitude does not wrap — no v shift).
            //
            // The face region must be the polygon INTERIOR (the CDT
            // triangulates inside the outer ring): a walk with the region on
            // its LEFT is CCW in the right-handed (u, v) frame for an
            // outward face and CW for a cavity (`reversed`) face. A
            // non-wrapping boundary bounding the COMPLEMENT (a sphere minus
            // a small side cap — both poles inside the region) fails this
            // test — out of scope, loud at the caller (later slice).
            let area2: f64 = (0..b_su.len())
                .map(|k| {
                    let j = (k + 1) % b_su.len();
                    b_su[k] * b_sv[j] - b_su[j] * b_sv[k]
                })
                .sum();
            if area2 == 0.0 || (area2 > 0.0) == reversed {
                return None;
            }
            let u_ref = umean(&b_su);
            let mut o = Vec::with_capacity(b_su.len());
            for k in 0..b_su.len() {
                o.push(verts2d.len() as u32);
                verts2d.push(cad_primitives::Point2::new(b_su[k], b_sv[k]));
                vert3d.push(boundary[k]);
            }
            for (li, (hu, hv)) in h_loops.iter().enumerate() {
                let du = ((umean(hu) - u_ref) / span).round() * span;
                let mut hi = Vec::with_capacity(hu.len());
                for k in 0..hu.len() {
                    hi.push(verts2d.len() as u32);
                    verts2d.push(cad_primitives::Point2::new(hu[k] - du, hv[k]));
                    vert3d.push(holes[li][k]);
                }
                hole_idx.push(hi);
            }
            o
        }
        1 | -1 => {
            // POLE CAP (possibly the complement — most of the sphere): the
            // wrapping outer loop encloses exactly one pole. Region-left of a
            // CCW-around-outward-normal loop: wu = +1 → north, flipped for a
            // cavity (reversed) face.
            let north = (b_wu > 0) != reversed;
            let pole_sv = if north { FRAC_PI_2 * r } else { -FRAC_PI_2 * r };
            let pole_3d = eval(0.0, if north { FRAC_PI_2 } else { -FRAC_PI_2 });
            let wsign = b_wu as f64;
            let m = b_su.len();

            // Hole u-extents (shifted near the boundary period below) so the
            // seam can avoid splitting one; and a segment-crossing test
            // against every boundary + hole edge.
            let seg_cross = |p: [f64; 2], q: [f64; 2], a: [f64; 2], b: [f64; 2]| -> bool {
                let d = [q[0] - p[0], q[1] - p[1]];
                let e = [b[0] - a[0], b[1] - a[1]];
                let denom = d[0] * e[1] - d[1] * e[0];
                if denom == 0.0 {
                    return false; // parallel: endpoint contact is handled by candidate choice
                }
                let w = [a[0] - p[0], a[1] - p[1]];
                let t = (w[0] * e[1] - w[1] * e[0]) / denom;
                let s = (w[0] * d[1] - w[1] * d[0]) / denom;
                let eps = 1e-12;
                t > eps && t < 1.0 - eps && s > eps && s < 1.0 - eps
            };

            // Unwrapped boundary chain starting at candidate xi (index copy
            // of xi appended at +wu·span).
            let chain_at = |xi: usize| -> Vec<[f64; 2]> {
                (0..=m)
                    .map(|j| {
                        let idx = (xi + j) % m;
                        let off = if xi + j >= m { wsign * span } else { 0.0 };
                        [b_su[idx] + off, b_sv[idx]]
                    })
                    .collect()
            };

            let mut choice: Option<usize> = None;
            'candidates: for xi in 0..m {
                let chain = chain_at(xi);
                let left = [chain[0][0], pole_sv];
                let right = [chain[m][0], pole_sv];
                // The two seam verticals and the pole bottom edge must cross
                // no boundary edge (interior crossings only; the shared
                // chain endpoints are excluded by the open interval).
                let seam_segs = [(chain[0], left), (chain[m], right), (left, right)];
                for w in chain.windows(2) {
                    for &(p, q) in &seam_segs {
                        if seg_cross(p, q, w[0], w[1]) {
                            continue 'candidates;
                        }
                    }
                }
                // Holes sit inside the region; the seam must not split one.
                // (Hole u-extents are compared in the chain's period.)
                let u_lo = chain[0][0].min(chain[m][0]);
                let u_hi = chain[0][0].max(chain[m][0]);
                let u_center = 0.5 * (u_lo + u_hi);
                let splits_hole = h_loops.iter().any(|(hu, _)| {
                    let du = ((umean(hu) - u_center) / span).round() * span;
                    let (mut lo, mut hi) = (f64::INFINITY, f64::NEG_INFINITY);
                    for &u in hu {
                        lo = lo.min(u - du);
                        hi = hi.max(u - du);
                    }
                    for &x in &[chain[0][0], chain[m][0]] {
                        if x > lo && x < hi {
                            return true;
                        }
                    }
                    false
                });
                if splits_hole {
                    continue 'candidates;
                }
                choice = Some(xi);
                break;
            }
            let xi = choice?;
            let chain = chain_at(xi);

            // Meridian seam subdivision: shared 3D samples for BOTH copies
            // (bit-identical — the weld below closes the seam watertight).
            let seg = max_3d_area.sqrt().max(1e-9 * r);
            let v_top = chain[0][1];
            let k_sub = (((v_top - pole_sv).abs() / seg).ceil() as usize).max(1);
            let u_seam = b_su[xi] / r; // seam meridian angle (period-agnostic)
            let seam_pts: Vec<(f64, Point3)> = (1..k_sub)
                .map(|k| {
                    let sv = v_top + (pole_sv - v_top) * (k as f64) / (k_sub as f64);
                    (sv, eval(u_seam, sv / r))
                })
                .collect();
            // The UV pole line must ALSO be subdivided at `seg`: as a single
            // 2π·r-long constraint edge its diametral (encroachment) circle
            // covers most of the domain, and spade's `keep_constraint_edges`
            // refinement then refuses nearly every Steiner insertion —
            // leaving equator-to-pole chord triangles (measured: 19% area
            // deficit on a hemisphere). Every subdivision point carries the
            // SAME 3D pole point; the weld collapses them into the fan apex.
            let k_bot = (((chain[m][0] - chain[0][0]).abs() / seg).ceil() as usize).max(1);

            // Ring: B_0..B_m (unwrapped, B_m = B_0's period copy), seam down
            // at u = chain[m][0], the subdivided pole line (right → left, one
            // 3D point), seam up at u = chain[0][0].
            let mut o: Vec<u32> = Vec::with_capacity(m + 2 + k_bot + 2 * seam_pts.len());
            for (j, uv) in chain.iter().enumerate() {
                o.push(verts2d.len() as u32);
                verts2d.push(cad_primitives::Point2::new(uv[0], uv[1]));
                vert3d.push(boundary[(xi + j) % m]);
            }
            for &(sv, p3) in &seam_pts {
                o.push(verts2d.len() as u32);
                verts2d.push(cad_primitives::Point2::new(chain[m][0], sv));
                vert3d.push(p3);
            }
            for k in 0..=k_bot {
                let u = chain[m][0] + (chain[0][0] - chain[m][0]) * (k as f64) / (k_bot as f64);
                o.push(verts2d.len() as u32);
                verts2d.push(cad_primitives::Point2::new(u, pole_sv));
                vert3d.push(pole_3d);
            }
            for &(sv, p3) in seam_pts.iter().rev() {
                o.push(verts2d.len() as u32);
                verts2d.push(cad_primitives::Point2::new(chain[0][0], sv));
                vert3d.push(p3);
            }

            // Holes, period-shifted into the unwrapped chain's u-window.
            let u_center = 0.5 * (chain[0][0] + chain[m][0]);
            for (li, (hu, hv)) in h_loops.iter().enumerate() {
                let du = ((umean(hu) - u_center) / span).round() * span;
                let mut hi = Vec::with_capacity(hu.len());
                for k in 0..hu.len() {
                    hi.push(verts2d.len() as u32);
                    verts2d.push(cad_primitives::Point2::new(hu[k] - du, hv[k]));
                    vert3d.push(holes[li][k]);
                }
                hole_idx.push(hi);
            }
            o
        }
        _ => return None, // |wu| ≥ 2: multi-wrap — out of scope
    };

    // Chord-band seeding (see the torus site): sphere scaling is
    // `su = u·r, sv = v·r` — `sv` is exact meridian arc; `su` is arc at the
    // EQUATOR, an over-estimate at latitude (true arc = Δsu·cos v), so the
    // isotropic budget s = √max_area is conservative in both directions.
    let s = max_3d_area.sqrt();
    let (ref_verts, tris) = cherchi_rs::cdt_polygon_with_holes_refined_seeded(
        &verts2d,
        &outer,
        &hole_idx,
        max_3d_area,
        [s, s],
    )
    .ok()?;

    // Map back: ring/hole verts → EXACT input 3D (seam copies and pole
    // corners repeat one bit-identical Point3); Steiner verts → `eval`.
    let n = vert3d.len();
    let mut verts3d: Vec<Point3> = Vec::with_capacity(ref_verts.len());
    for (i, sp) in ref_verts.iter().enumerate() {
        if i < n {
            verts3d.push(vert3d[i]);
        } else {
            verts3d.push(eval(sp.x() / r, sp.y() / r));
        }
    }

    // Weld bit-identical positions (the two seam copies + the two pole
    // corners) and drop triangles that degenerate under the weld — this is
    // what closes the pole fan and the seam watertight.
    let mut first_at: std::collections::BTreeMap<[u64; 3], u32> = std::collections::BTreeMap::new();
    let mut remap: Vec<u32> = Vec::with_capacity(verts3d.len());
    for p in &verts3d {
        let key = [p.x().to_bits(), p.y().to_bits(), p.z().to_bits()];
        let id = *first_at.entry(key).or_insert(remap.len() as u32);
        remap.push(id);
    }
    let welded: Vec<[u32; 3]> = tris
        .iter()
        .map(|t| {
            [
                remap[t[0] as usize],
                remap[t[1] as usize],
                remap[t[2] as usize],
            ]
        })
        .filter(|t| t[0] != t[1] && t[1] != t[2] && t[2] != t[0])
        .collect();
    if welded.is_empty() {
        return None;
    }
    Some((verts3d, welded))
}

/// KV14 Slice F: Stage-1 tessellation of a boolean-result TORUS lateral carrying
/// inner loops — a POLOIDAL PERIODIC BAND (the boundary wraps the tube; the
/// toroidal sweep is bounded). Delegates to [`tessellate_torus_patch`] (the same
/// UV-CDT the render path uses: it projects the boundary into the (meridian,
/// longitude) plane, seam-bridges the two meridian-wrapping profiles with
/// on-surface subdivision, and refines interior Steiner points onto the torus —
/// exactly what a non-developable band needs).
///
/// The patch returns a FRESH vertex pool (boundary verts bit-exact, seam/Steiner
/// verts on-surface). Map back by QUANTIZED position: a profile-boundary vert
/// recovers its EXISTING shared global (watertight with adjacent caps); a seam
/// duplicate or Steiner vert welds by position (the two seam copies are one
/// meridian, ULP-apart, so an exact key would leave a crack — Cherchi needs the
/// band watertight). A holed band (a window in the tube, Slice F-2) carves each
/// non-wrapping window as a CDT hole; only seam-crossing / longitude-wrapping
/// patches return `None` from the patch tessellator → a typed wall.
///
/// KV14 Slice F-3: a hole-free lateral whose loop carries none of the
/// structured vocabulary (R0032's lone 57-chord torus∩cone polyline) is a
/// DISK patch — the same consumer's 0-wrapping branch fills the loop's
/// interior; a loop bounding the complement declines there (typed).
#[allow(clippy::too_many_arguments)]
pub(crate) fn tessellate_torus_band(
    f_idx: usize,
    f: &BRepFace,
    edges: &[BRepEdge],
    chains: &std::collections::BTreeMap<u32, Vec<u32>>,
    center: Point3,
    axis_dir: Vector3,
    major: f64,
    minor: f64,
    out_verts: &mut Vec<Point3>,
    sources: &mut Vec<TessellationSource>,
    out_tris: &mut Vec<[u32; 3]>,
) -> Result<(), YangError> {
    // Gather boundary + hole loops as ordered global-vertex chains, then 3D.
    let outer_g = loop_polyline(f_idx, &f.outer_loop, edges, chains)?;
    let mut holes_g: Vec<Vec<u32>> = Vec::with_capacity(f.inner_loops.len());
    for inner in &f.inner_loops {
        holes_g.push(loop_polyline(f_idx, inner, edges, chains)?);
    }
    let boundary: Vec<Point3> = outer_g.iter().map(|&g| out_verts[g as usize]).collect();
    let holes: Vec<Vec<Point3>> = holes_g
        .iter()
        .map(|h| h.iter().map(|&g| out_verts[g as usize]).collect())
        .collect();

    // Quantized position key (1e-9 m ≪ TAU_MODEL 1e-7 ≪ MIN_FEATURE_SIZE 1e-6):
    // welds ULP-apart seam twins without merging distinct model features.
    let q = 1e-9_f64;
    let key = |p: Point3| -> (i64, i64, i64) {
        let a = p.as_array();
        (
            (a[0] / q).round() as i64,
            (a[1] / q).round() as i64,
            (a[2] / q).round() as i64,
        )
    };
    // Pre-seed with the EXISTING boundary/hole globals so they stay shared.
    let mut pos_to_global: std::collections::HashMap<(i64, i64, i64), u32> =
        std::collections::HashMap::new();
    for &g in outer_g.iter().chain(holes_g.iter().flatten()) {
        pos_to_global.entry(key(out_verts[g as usize])).or_insert(g);
    }

    // Triangle-area budget in arc-length² — the torus patch's own chord
    // budget (`torus_chord_bound`, the single source Stage 4's relocation
    // band reads back) sets the meridian spacing; the patch scales (u,v) to
    // arc-length so this is a true area cap.
    let d_eps = torus_chord_bound(major, minor);
    let dphi = (8.0 * d_eps / minor).sqrt().min(0.5);
    let n_seg = ((2.0 * std::f64::consts::PI / dphi).ceil() as u32).max(12);
    let seg = 2.0 * std::f64::consts::PI * minor / f64::from(n_seg);
    let max_area = seg * seg;

    let Some((verts, tris)) = tessellate_torus_patch(
        center, axis_dir, major, minor, &boundary, &holes, max_area, f.reversed,
    ) else {
        return Err(YangError::MalformedTopology(format!(
            "face {f_idx}: torus patch UV-CDT declined (a seam-crossing / \
             longitude-wrapping loop, or a lone loop bounding the torus's \
             complement — KV14 Slice F later sub-slice)"
        )));
    };

    // Map every patch vertex to a global index (existing boundary shared,
    // seam/Steiner welded + freshly created with an on-surface source).
    let au = normalize3(axis_dir.as_array());
    let cen = center.as_array();
    let (e1v, e2v) = ortho_basis(axis_dir);
    let (e1a, e2a) = (e1v.as_array(), e2v.as_array());
    let mut global_of_patch: Vec<u32> = Vec::with_capacity(verts.len());
    for &p in &verts {
        if let Some(&g) = pos_to_global.get(&key(p)) {
            global_of_patch.push(g);
            continue;
        }
        // Fresh vertex: invert to (φ poloidal, θ toroidal) for the source so
        // `eval_source` round-trips through the torus arm.
        let pa = p.as_array();
        let w = [pa[0] - cen[0], pa[1] - cen[1], pa[2] - cen[2]];
        let tau = w[0] * au[0] + w[1] * au[1] + w[2] * au[2];
        let rv = [w[0] - tau * au[0], w[1] - tau * au[1], w[2] - tau * au[2]];
        let wx = rv[0] * e1a[0] + rv[1] * e1a[1] + rv[2] * e1a[2];
        let wy = rv[0] * e2a[0] + rv[1] * e2a[1] + rv[2] * e2a[2];
        let rho = (wx * wx + wy * wy).sqrt();
        let phi = tau.atan2(rho - major);
        let theta = wy.atan2(wx);
        let g = out_verts.len() as u32;
        out_verts.push(p);
        sources.push(TessellationSource::BRepFace {
            face: f_idx as u32,
            u: phi,
            v: theta,
        });
        pos_to_global.insert(key(p), g);
        global_of_patch.push(g);
    }

    // Emit, orienting each triangle by the analytic torus outward normal (inward
    // if `reversed` — a cavity wall).
    for t in &tris {
        let mut tri = [
            global_of_patch[t[0] as usize],
            global_of_patch[t[1] as usize],
            global_of_patch[t[2] as usize],
        ];
        // Outward = centroid − nearest tube-centre-circle point.
        let cn = {
            let a = out_verts[tri[0] as usize].as_array();
            let b = out_verts[tri[1] as usize].as_array();
            let c = out_verts[tri[2] as usize].as_array();
            [
                (a[0] + b[0] + c[0]) / 3.0,
                (a[1] + b[1] + c[1]) / 3.0,
                (a[2] + b[2] + c[2]) / 3.0,
            ]
        };
        let w = [cn[0] - cen[0], cn[1] - cen[1], cn[2] - cen[2]];
        let tau = w[0] * au[0] + w[1] * au[1] + w[2] * au[2];
        let rv = [w[0] - tau * au[0], w[1] - tau * au[1], w[2] - tau * au[2]];
        let rl = (rv[0] * rv[0] + rv[1] * rv[1] + rv[2] * rv[2])
            .sqrt()
            .max(1e-300);
        let rhat = [rv[0] / rl, rv[1] / rl, rv[2] / rl];
        let mut n = [
            cn[0] - (cen[0] + major * rhat[0]),
            cn[1] - (cen[1] + major * rhat[1]),
            cn[2] - (cen[2] + major * rhat[2]),
        ];
        if f.reversed {
            n = [-n[0], -n[1], -n[2]];
        }
        orient_tri(out_verts, &mut tri, n);
        out_tris.push(tri);
    }
    Ok(())
}
