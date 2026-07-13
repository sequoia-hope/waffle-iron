//! Per-surface render tessellators for the curved surface families: the
//! circular/annular planar caps, the cylinder/cone/torus laterals, the
//! torus/sphere patches, and the closed-sphere case. Extracted verbatim from
//! `tessellate/mod.rs` (move-only, F9). The nine entry points reached from the
//! `tessellate_with_chord_tolerance` dispatcher (and the two exercised by the
//! sibling `*_tess_tests` modules) are `pub(crate)`; the shared CDT/ring
//! machinery, `circle_frame`, and the planar-face path stay in the parent and
//! are reached here via `use super::*`. `tessellate_annular_cap` stays private
//! (called only from within this module).

#[allow(clippy::wildcard_imports)]
use super::*;

/// The single canonical planar-disk-cap routine (PR-KV5a): a planar face
/// whose outer loop is ONE full-circle half-edge, no rings. Rim sampled at
/// the chord-bound `N` (uniform angles from the seam anchor, CCW around the
/// circle's directional normal == the face normal), fanned from sample 0 —
/// the fan of a convex polygon needs no ear search, and its winding follows
/// the boundary walk (hard rule 5: no post-hoc flips). Flat-shaded with the
/// face normal.
pub(crate) fn tessellate_circular_cap(
    arena: &BrepArena,
    fid: FaceId,
    n_seg: u32,
    out: &mut RenderMesh,
) -> Result<(), KernelV2Error> {
    let face = arena.face(fid)?;
    let Some(Surface::Plane(plane)) = face.surface else {
        return Err(KernelV2Error::FaceWithoutSurface { face: fid });
    };
    if !face.inner_loops.is_empty() {
        return tessellate_annular_cap(arena, fid, n_seg, out);
    }
    let hes = arena.loop_half_edges(face.outer_loop)?;
    let [h] = hes[..] else {
        return Err(KernelV2Error::TessellationFailed {
            face: fid,
            reason: "circle-bounded planar loop must be a single circle half-edge (KV5a)",
        });
    };
    let he = arena.half_edge(h)?;
    let Curve::Circle {
        center,
        normal,
        radius,
    } = he.curve
    else {
        return Err(KernelV2Error::TessellationFailed {
            face: fid,
            reason: "circle-bounded planar loop must be a single circle half-edge (KV5a)",
        });
    };
    let anchor = arena.vertex(he.origin)?.point;
    let Some((e1, e2)) = circle_frame(center, normal, anchor) else {
        return Err(KernelV2Error::TessellationFailed {
            face: fid,
            reason: "degenerate circle frame (anchor does not span a radial direction)",
        });
    };

    let range_start = out.indices.len() as u32;
    let base = out.num_vertices() as u32;
    let n = n_seg as usize;
    for k in 0..n {
        let theta = 2.0 * std::f64::consts::PI * (k as f64) / (n as f64);
        let (s, c) = theta.sin_cos();
        out.positions.extend_from_slice(&[
            center.x() + radius * (c * e1[0] + s * e2[0]),
            center.y() + radius * (c * e1[1] + s * e2[1]),
            center.z() + radius * (c * e1[2] + s * e2[2]),
        ]);
        out.normals
            .extend_from_slice(&[plane.normal.x, plane.normal.y, plane.normal.z]);
    }
    for k in 1..(n as u32) - 1 {
        out.indices
            .extend_from_slice(&[base, base + k, base + k + 1]);
    }
    out.face_ranges.push(FaceRange {
        face: fid,
        start: range_start,
        count: out.indices.len() as u32 - range_start,
    });
    Ok(())
}

/// Annular planar cap (PR-KV6a, the full-turn revolve washer): outer loop
/// ONE full-circle half-edge, exactly one ring that is also one full-circle
/// half-edge, both concentric in the face plane. Sampled at the shared
/// chord-bound `N` on a single angle table anchored at the OUTER circle's
/// seam vertex (the ring is sampled at the same table re-anchored at its
/// own seam), then stitched as one quad strip — the planar analog of the
/// cylinder lateral, flat-shaded with the face normal.
fn tessellate_annular_cap(
    arena: &BrepArena,
    fid: FaceId,
    n_seg: u32,
    out: &mut RenderMesh,
) -> Result<(), KernelV2Error> {
    let face = arena.face(fid)?;
    let Some(Surface::Plane(plane)) = face.surface else {
        return Err(KernelV2Error::FaceWithoutSurface { face: fid });
    };
    let [ring_lid] = face.inner_loops[..] else {
        return Err(KernelV2Error::TessellationFailed {
            face: fid,
            reason: "annular cap with more than one ring is outside the KV6a vocabulary",
        });
    };
    let circle_of = |lid| -> Result<(Point3, UnitVector3, f64, Point3), KernelV2Error> {
        let hes = arena.loop_half_edges(lid)?;
        let [h] = hes[..] else {
            return Err(KernelV2Error::TessellationFailed {
                face: fid,
                reason: "annular cap loop must be a single circle half-edge",
            });
        };
        let he = arena.half_edge(h)?;
        let Curve::Circle {
            center,
            normal,
            radius,
        } = he.curve
        else {
            return Err(KernelV2Error::TessellationFailed {
                face: fid,
                reason: "annular cap loop must be a single circle half-edge",
            });
        };
        Ok((center, normal, radius, arena.vertex(he.origin)?.point))
    };
    let (c_o, nu_o, r_o, anchor_o) = circle_of(face.outer_loop)?;
    let (c_r, _nu_r, r_r, anchor_r) = circle_of(ring_lid)?;

    // Each ring is sampled in its OWN anchor frame (CCW around `nu_o`), so its
    // boundary samples coincide with the adjacent lateral's rim samples (both
    // anchored at the same seam vertex) — load-bearing for cross-face
    // watertightness. The two seams need NOT be at the same azimuth (the
    // gear's counterbore floor has independent outer/inner seams); the strip
    // below sweeps both rings by azimuth rather than stitching column-to-
    // column, so a phase offset between the seams no longer twists the quads.
    let Some((e1_o, e2_o)) = circle_frame(c_o, nu_o, anchor_o) else {
        return Err(KernelV2Error::TessellationFailed {
            face: fid,
            reason: "degenerate circle frame (anchor does not span a radial direction)",
        });
    };
    let Some((e1_r, e2_r)) = circle_frame(c_r, nu_o, anchor_r) else {
        return Err(KernelV2Error::TessellationFailed {
            face: fid,
            reason: "degenerate ring circle frame",
        });
    };

    let range_start = out.indices.len() as u32;
    let base = out.num_vertices() as u32;
    let n = n_seg;

    // Emit both rings' samples (each anchored at its OWN seam — load-bearing
    // for cross-face watertightness with the adjacent lateral, which samples
    // from the same seam vertex). The outer ring occupies render indices
    // base..base+n, the inner ring base+n..base+2n.
    for (center, radius, e1, e2) in [(c_o, r_o, e1_o, e2_o), (c_r, r_r, e1_r, e2_r)] {
        for k in 0..n {
            let theta = 2.0 * std::f64::consts::PI * (k as f64) / (n as f64);
            let (sn, cs) = theta.sin_cos();
            out.positions.extend_from_slice(&[
                center.x() + radius * (cs * e1[0] + sn * e2[0]),
                center.y() + radius * (cs * e1[1] + sn * e2[1]),
                center.z() + radius * (cs * e1[2] + sn * e2[2]),
            ]);
            out.normals
                .extend_from_slice(&[plane.normal.x, plane.normal.y, plane.normal.z]);
        }
    }

    // The two rings are anchored at INDEPENDENT seam azimuths (the gear's
    // counterbore floor: outer rim seam ≠ inner bore seam — they descend from
    // different boolean-output vertices). A column-`k`-to-column-`k` strip
    // would stitch outer[k] to inner[k] across that phase offset, producing
    // twisted, self-overlapping quads whose two triangles wind OPPOSITELY —
    // half facing +normal, half −normal (PR-M8-cyl-Inc2). Instead, sweep both
    // rings by their azimuth around the shared axis (measured in the OUTER
    // frame `(e1_o, e2_o)`, CCW around `nu_o`) and advance whichever ring is
    // angularly behind — the standard two-ring annulus triangulation. Each
    // emitted triangle then winds CCW around `nu_o`, i.e. faces `+nu_o`.
    let tau = 2.0 * std::f64::consts::PI;
    // Azimuth (in the OUTER frame, CCW around nu_o, in [0, 2π)) of ring sample
    // `k`. A ring is sampled in its own anchor frame, so sample 0 sits at the
    // anchor's azimuth (measured in the outer frame) and each subsequent sample
    // advances by 2π/n.
    let azimuth = |anchor_dir: [f64; 3], k: u32| -> f64 {
        let ax = anchor_dir[0] * e1_o[0] + anchor_dir[1] * e1_o[1] + anchor_dir[2] * e1_o[2];
        let ay = anchor_dir[0] * e2_o[0] + anchor_dir[1] * e2_o[1] + anchor_dir[2] * e2_o[2];
        let base_phi = ay.atan2(ax);
        (base_phi + tau * (k as f64) / (n as f64)).rem_euclid(tau)
    };
    let dir_of = |anchor: Point3, center: Point3| -> [f64; 3] {
        [
            anchor.x() - center.x(),
            anchor.y() - center.y(),
            anchor.z() - center.z(),
        ]
    };
    let outer_dir = dir_of(anchor_o, c_o);
    let inner_dir = dir_of(anchor_r, c_r);
    let outer_az: Vec<f64> = (0..n).map(|k| azimuth(outer_dir, k)).collect();
    let inner_az: Vec<f64> = (0..n).map(|k| azimuth(inner_dir, k)).collect();

    // Two-pointer sweep. We walk both rings once around the full turn. At each
    // step the quad face (outer[oi], outer[oi+1] | inner[ii], inner[ii+1]) is
    // split by advancing the ring whose NEXT sample has the smaller forward
    // azimuth gap, emitting a triangle that always uses the current edge of one
    // ring and the leading vertex of the other. Winding `[a, b, c]` is chosen
    // so the triangle normal points along `+nu_o` (== the outer frame's CCW
    // sense); the per-vertex render normal is `plane.normal`.
    for tri in annulus_sweep_triangles(&outer_az, &inner_az, base, base + n) {
        out.indices.extend_from_slice(&tri);
    }

    out.face_ranges.push(FaceRange {
        face: fid,
        start: range_start,
        count: out.indices.len() as u32 - range_start,
    });
    Ok(())
}

/// Triangulate the annulus between two concentric rings sampled CCW around a
/// common axis at INDEPENDENT seam azimuths (PR-M8-cyl-Inc2). `outer_az` /
/// `inner_az` are the per-sample azimuths (radians, in the same CCW frame);
/// `outer_base` / `inner_base` are the render-vertex indices of each ring's
/// sample 0. Sweeps both rings by azimuth — at each step advancing whichever
/// ring's current vertex is angularly behind — so every emitted triangle winds
/// CCW around the axis (faces `+axis`), regardless of the phase offset between
/// the two seams. A naive column-`k`-to-column-`k` strip would twist each quad
/// when the seams differ, flipping half its triangles.
pub(crate) fn annulus_sweep_triangles(
    outer_az: &[f64],
    inner_az: &[f64],
    outer_base: u32,
    inner_base: u32,
) -> Vec<[u32; 3]> {
    let tau = 2.0 * std::f64::consts::PI;
    let fwd_gap = |from: f64, to: f64| -> f64 { (to - from).rem_euclid(tau) };
    let no = outer_az.len();
    let ni = inner_az.len();
    let mut tris = Vec::with_capacity(no + ni);
    if no == 0 || ni == 0 {
        return tris;
    }
    // Align the inner walk's start to the sample nearest-ahead of outer[0] so
    // the two pointers march in lockstep around the turn (deterministic).
    let ii0 = inner_az
        .iter()
        .enumerate()
        .min_by(|(_, a), (_, b)| {
            fwd_gap(outer_az[0], **a)
                .partial_cmp(&fwd_gap(outer_az[0], **b))
                .unwrap_or(Ordering::Equal)
        })
        .map(|(i, _)| i)
        .unwrap_or(0);
    let outer_idx = |k: usize| outer_base + (k % no) as u32;
    let inner_idx = |k: usize| inner_base + ((ii0 + k) % ni) as u32;
    let mut oi = 0usize;
    let mut ii = 0usize;
    while oi < no || ii < ni {
        let advance_outer = if oi >= no {
            false
        } else if ii >= ni {
            true
        } else {
            let o_cur = outer_az[oi % no];
            let i_cur = inner_az[(ii0 + ii) % ni];
            // Advance whichever ring's current vertex is angularly behind the
            // other's (smaller forward gap).
            fwd_gap(o_cur, i_cur) <= fwd_gap(i_cur, o_cur)
        };
        if advance_outer {
            // outer[oi], outer[oi+1], inner[ii] — CCW around +axis.
            tris.push([outer_idx(oi), outer_idx(oi + 1), inner_idx(ii)]);
            oi += 1;
        } else {
            // outer[oi], inner[ii+1], inner[ii] — CCW around +axis.
            tris.push([outer_idx(oi), inner_idx(ii + 1), inner_idx(ii)]);
            ii += 1;
        }
    }
    tris
}

/// The single canonical cylinder-lateral routine (PR-KV5a): the full tube
/// between two full-circle rims, as `N` quad-pairs. The angular frame comes
/// from the BOTTOM rim (the one whose directional normal points toward the
/// other — the validated outward-orientation rule), so the quad winding
/// follows the boundary walk; per-vertex normals are the exact analytic
/// outward radial directions at the sampled corners (smooth shading — the
/// surface, not the facets, defines the normal field).
pub(crate) fn tessellate_cylinder_lateral(
    arena: &BrepArena,
    fid: FaceId,
    n_seg: u32,
    out: &mut RenderMesh,
) -> Result<(), KernelV2Error> {
    let face = arena.face(fid)?;
    let hes = arena.loop_half_edges(face.outer_loop)?;
    let mut rims = Vec::new();
    for &h in &hes {
        let he = arena.half_edge(h)?;
        if let Curve::Circle {
            center,
            normal,
            radius,
        } = he.curve
        {
            rims.push((center, normal, radius, arena.vertex(he.origin)?.point));
        }
    }
    let [rim_a, rim_b] = rims[..] else {
        return Err(KernelV2Error::TessellationFailed {
            face: fid,
            reason: "cylinder lateral must be bounded by exactly two full-circle rims (KV5a)",
        });
    };
    // Bottom rim: traversal axis points toward the opposite rim.
    let toward = |from: &(Point3, UnitVector3, f64, Point3),
                  to: &(Point3, UnitVector3, f64, Point3)| {
        (to.0.x() - from.0.x()) * from.1.x
            + (to.0.y() - from.0.y()) * from.1.y
            + (to.0.z() - from.0.z()) * from.1.z
    };
    // Material sense: outward laterals have rims pointing TOWARD each
    // other's centers; cavity walls (reversed, PR-KV6a washers) point AWAY.
    let reversed = matches!(face.surface, Some(Surface::Cylinder { reversed: true, .. }));
    let (bot, top) = match (
        reversed,
        toward(&rim_a, &rim_b) > 0.0,
        toward(&rim_b, &rim_a) > 0.0,
    ) {
        // Outward: BOTH rims traverse toward each other (the KV5a shape);
        // the walk-order first is the frame rim.
        (false, true, _) => (rim_a, rim_b),
        (false, false, true) => (rim_b, rim_a),
        // Reversed: both rims point away; the frame rim is the walk-order
        // first (deterministic), and the SAME quad index pattern then winds
        // inward (tangent CCW around an away-pointing axis × toward-top).
        (true, false, false) => (rim_a, rim_b),
        _ => {
            return Err(KernelV2Error::TessellationFailed {
                face: fid,
                reason: "cylinder rim orientations disagree with the material sense",
            });
        }
    };
    let (cb, _nub, _radius, _anchor) = bot;
    let ct = top.0;

    // PR-KV7: each row is sampled with BITWISE the frame its adjacent cap
    // uses — `circle_frame(center, NEG(rim half-edge normal), rim anchor)`.
    // The cap's full-circle half-edge carries the exact negation of the
    // lateral's (a validated twin invariant) and the same anchor vertex, so
    // the cap row and the lateral row are bit-identical position sequences:
    // cross-face watertightness by construction, independent of whether the
    // two rims' anchors sit on exactly the same ruling (recovered boolean
    // outputs guarantee anchor alignment only within the recovery band;
    // the pre-KV7 single-frame scheme cracked there at f32 granularity).
    // The two rows' cap frames always advance OPPOSITELY around the
    // bottom→top axis, so one row is index-reversed to align the strip.
    let sample_row = |row: &(Point3, UnitVector3, f64, Point3),
                      out: &mut RenderMesh|
     -> Result<[f64; 3], KernelV2Error> {
        let (c0, nu, r, anc) = *row;
        let cap_nu = UnitVector3 {
            x: -nu.x,
            y: -nu.y,
            z: -nu.z,
        };
        let Some((e1, e2)) = circle_frame(c0, cap_nu, anc) else {
            return Err(KernelV2Error::TessellationFailed {
                face: fid,
                reason: "degenerate circle frame (anchor does not span a radial direction)",
            });
        };
        for k in 0..n_seg {
            let theta = 2.0 * std::f64::consts::PI * (k as f64) / (n_seg as f64);
            let (s, c) = theta.sin_cos();
            let radial = [
                c * e1[0] + s * e2[0],
                c * e1[1] + s * e2[1],
                c * e1[2] + s * e2[2],
            ];
            out.positions.extend_from_slice(&[
                c0.x() + r * radial[0],
                c0.y() + r * radial[1],
                c0.z() + r * radial[2],
            ]);
            if reversed {
                out.normals
                    .extend_from_slice(&[-radial[0], -radial[1], -radial[2]]);
            } else {
                out.normals.extend_from_slice(&radial);
            }
        }
        // The row's advance direction: CCW around cap_nu.
        Ok([cap_nu.x, cap_nu.y, cap_nu.z])
    };

    let range_start = out.indices.len() as u32;
    let base = out.num_vertices() as u32;
    let n = n_seg;
    let d_bot = sample_row(&bot, out)?; // rows: bottom [base..base+n)
    let d_top = sample_row(&top, out)?; // top [base+n..base+2n)

    // Align both rows to advance CCW around the bottom→top axis: re-index
    // the row whose cap frame advances the other way (k → (n−k) mod n; the
    // positions are untouched, only the strip indexing).
    let axis_up = [ct.x() - cb.x(), ct.y() - cb.y(), ct.z() - cb.z()];
    let along = |d: &[f64; 3]| d[0] * axis_up[0] + d[1] * axis_up[1] + d[2] * axis_up[2];
    let idx_b = |k: u32| -> u32 {
        if along(&d_bot) >= 0.0 {
            base + (k % n)
        } else {
            base + ((n - (k % n)) % n)
        }
    };
    let idx_t = |k: u32| -> u32 {
        if along(&d_top) >= 0.0 {
            base + n + (k % n)
        } else {
            base + n + ((n - (k % n)) % n)
        }
    };
    for k in 0..n {
        let (bk, bk1, tk, tk1) = (idx_b(k), idx_b(k + 1), idx_t(k), idx_t(k + 1));
        if reversed {
            // Cavity sense: wind inward.
            out.indices.extend_from_slice(&[bk, tk1, bk1]);
            out.indices.extend_from_slice(&[bk, tk, tk1]);
        } else {
            // CCW-around-axis bottom row + axis toward the top row ⇒ these
            // wind with outward normals (∝ tangent × axis = radial).
            out.indices.extend_from_slice(&[bk, bk1, tk1]);
            out.indices.extend_from_slice(&[bk, tk1, tk]);
        }
    }
    out.face_ranges.push(FaceRange {
        face: fid,
        start: range_start,
        count: out.indices.len() as u32 - range_start,
    });
    Ok(())
}

/// Tessellate a canonical [`Surface::Cone`] frustum band (KV6c increment 3).
///
/// The two full-circle rims sit at DIFFERENT radii, so sampling each row at
/// its own rim radius/center — exactly as [`tessellate_cylinder_lateral`] —
/// yields the frustum strip directly; only the surface NORMAL differs. The
/// outward cone normal is `cos(α)·r̂ − sin(α)·axis` (the radial tilted back
/// toward the apex by the half-angle α; → r̂ as α→0, the cylinder limit),
/// negated for the cavity (`reversed`) sense. Rows are sampled with the
/// adjacent cap's BITWISE circle frame, so the band is watertight against its
/// caps by construction — the same PR-KV7 scheme the cylinder lateral uses.
pub(crate) fn tessellate_cone_lateral(
    arena: &BrepArena,
    fid: FaceId,
    n_seg: u32,
    out: &mut RenderMesh,
) -> Result<(), KernelV2Error> {
    let face = arena.face(fid)?;
    let (apex, half_angle, axis_dir, reversed) = match face.surface {
        Some(Surface::Cone {
            apex,
            half_angle,
            axis_dir,
            reversed,
        }) => (apex, half_angle, axis_dir, reversed),
        _ => {
            return Err(KernelV2Error::TessellationFailed {
                face: fid,
                reason: "tessellate_cone_lateral called on a non-cone face",
            })
        }
    };
    let (sa, ca) = half_angle.sin_cos();
    let hes = arena.loop_half_edges(face.outer_loop)?;
    let mut rims = Vec::new();
    for &h in &hes {
        let he = arena.half_edge(h)?;
        if let Curve::Circle {
            center,
            normal,
            radius,
        } = he.curve
        {
            rims.push((center, normal, radius, arena.vertex(he.origin)?.point));
        }
    }

    // KV6 slice 2B: the APEX form — a single base rim, the apex an interior
    // singular point. The base ring is sampled with the cap's bitwise frame
    // (watertight against the disc cap, the PR-KV7 scheme); the "top row" is
    // n_seg copies of the apex point carrying per-azimuth cone normals, and
    // the same bottom-row index transform as the 2-rim strip is applied so
    // each fan triangle winds outward exactly as the strip's first triangle
    // does. Only the outward solid sense has a producer; a reversed apex
    // cavity is rejected typed (matching `validate_cone_face`).
    if let [rim] = rims[..] {
        if reversed {
            return Err(KernelV2Error::TessellationFailed {
                face: fid,
                reason: "apex-cone cavity (reversed) is outside the KV6c vocabulary",
            });
        }
        let (c0, nu, r, anc) = rim;
        let range_start = out.indices.len() as u32;
        let base = out.num_vertices() as u32;
        let n = n_seg;
        // Base ring: identical sampling to `sample_row` (cap frame).
        let cap_nu = UnitVector3 {
            x: -nu.x,
            y: -nu.y,
            z: -nu.z,
        };
        let Some((e1, e2)) = circle_frame(c0, cap_nu, anc) else {
            return Err(KernelV2Error::TessellationFailed {
                face: fid,
                reason: "degenerate circle frame (anchor does not span a radial direction)",
            });
        };
        let radial_at = |k: u32| {
            let theta = 2.0 * std::f64::consts::PI * (k as f64) / (n as f64);
            let (s, c) = theta.sin_cos();
            [
                c * e1[0] + s * e2[0],
                c * e1[1] + s * e2[1],
                c * e1[2] + s * e2[2],
            ]
        };
        for k in 0..n {
            let radial = radial_at(k);
            out.positions.extend_from_slice(&[
                c0.x() + r * radial[0],
                c0.y() + r * radial[1],
                c0.z() + r * radial[2],
            ]);
            out.normals.extend_from_slice(&[
                ca * radial[0] - sa * axis_dir.x,
                ca * radial[1] - sa * axis_dir.y,
                ca * radial[2] - sa * axis_dir.z,
            ]);
        }
        // Apex row: bit-identical apex positions, per-azimuth normals.
        for k in 0..n {
            let radial = radial_at(k);
            out.positions
                .extend_from_slice(&[apex.x(), apex.y(), apex.z()]);
            out.normals.extend_from_slice(&[
                ca * radial[0] - sa * axis_dir.x,
                ca * radial[1] - sa * axis_dir.y,
                ca * radial[2] - sa * axis_dir.z,
            ]);
        }
        // Same orientation logic as the 2-rim strip with axis_up = apex − c0:
        // the fan triangle [bk, bk1, apex(k)] is the strip's [bk, bk1, tk1]
        // with the degenerate second triangle dropped.
        let axis_up = [apex.x() - c0.x(), apex.y() - c0.y(), apex.z() - c0.z()];
        let along = cap_nu.x * axis_up[0] + cap_nu.y * axis_up[1] + cap_nu.z * axis_up[2];
        let idx = |k: u32| -> u32 {
            if along >= 0.0 {
                k % n
            } else {
                (n - (k % n)) % n
            }
        };
        for k in 0..n {
            let (bk, bk1) = (base + idx(k), base + idx(k + 1));
            let ak = base + n + idx(k);
            out.indices.extend_from_slice(&[bk, bk1, ak]);
        }
        out.face_ranges.push(FaceRange {
            face: fid,
            start: range_start,
            count: out.indices.len() as u32 - range_start,
        });
        return Ok(());
    }

    let [rim_a, rim_b] = rims[..] else {
        return Err(KernelV2Error::TessellationFailed {
            face: fid,
            reason: "cone lateral must be bounded by exactly two full-circle rims (KV6c)",
        });
    };
    let toward = |from: &(Point3, UnitVector3, f64, Point3),
                  to: &(Point3, UnitVector3, f64, Point3)| {
        (to.0.x() - from.0.x()) * from.1.x
            + (to.0.y() - from.0.y()) * from.1.y
            + (to.0.z() - from.0.z()) * from.1.z
    };
    // Material sense: identical to the cylinder lateral (rim traversal axes
    // point toward each other for an outward band, away for a cavity bore).
    let (bot, top) = match (
        reversed,
        toward(&rim_a, &rim_b) > 0.0,
        toward(&rim_b, &rim_a) > 0.0,
    ) {
        (false, true, _) => (rim_a, rim_b),
        (false, false, true) => (rim_b, rim_a),
        (true, false, false) => (rim_a, rim_b),
        _ => {
            return Err(KernelV2Error::TessellationFailed {
                face: fid,
                reason: "cone rim orientations disagree with the material sense",
            });
        }
    };
    let (cb, _nub, _radius, _anchor) = bot;
    let ct = top.0;

    let sample_row = |row: &(Point3, UnitVector3, f64, Point3),
                      out: &mut RenderMesh|
     -> Result<[f64; 3], KernelV2Error> {
        let (c0, nu, r, anc) = *row;
        let cap_nu = UnitVector3 {
            x: -nu.x,
            y: -nu.y,
            z: -nu.z,
        };
        let Some((e1, e2)) = circle_frame(c0, cap_nu, anc) else {
            return Err(KernelV2Error::TessellationFailed {
                face: fid,
                reason: "degenerate circle frame (anchor does not span a radial direction)",
            });
        };
        for k in 0..n_seg {
            let theta = 2.0 * std::f64::consts::PI * (k as f64) / (n_seg as f64);
            let (s, c) = theta.sin_cos();
            let radial = [
                c * e1[0] + s * e2[0],
                c * e1[1] + s * e2[1],
                c * e1[2] + s * e2[2],
            ];
            out.positions.extend_from_slice(&[
                c0.x() + r * radial[0],
                c0.y() + r * radial[1],
                c0.z() + r * radial[2],
            ]);
            // Cone normal: cos(α)·r̂ − sin(α)·axis (negated for the cavity).
            let mut nrm = [
                ca * radial[0] - sa * axis_dir.x,
                ca * radial[1] - sa * axis_dir.y,
                ca * radial[2] - sa * axis_dir.z,
            ];
            if reversed {
                nrm = [-nrm[0], -nrm[1], -nrm[2]];
            }
            out.normals.extend_from_slice(&nrm);
        }
        Ok([cap_nu.x, cap_nu.y, cap_nu.z])
    };

    let range_start = out.indices.len() as u32;
    let base = out.num_vertices() as u32;
    let n = n_seg;
    let d_bot = sample_row(&bot, out)?;
    let d_top = sample_row(&top, out)?;

    let axis_up = [ct.x() - cb.x(), ct.y() - cb.y(), ct.z() - cb.z()];
    let along = |d: &[f64; 3]| d[0] * axis_up[0] + d[1] * axis_up[1] + d[2] * axis_up[2];
    let idx_b = |k: u32| -> u32 {
        if along(&d_bot) >= 0.0 {
            base + (k % n)
        } else {
            base + ((n - (k % n)) % n)
        }
    };
    let idx_t = |k: u32| -> u32 {
        if along(&d_top) >= 0.0 {
            base + n + (k % n)
        } else {
            base + n + ((n - (k % n)) % n)
        }
    };
    for k in 0..n {
        let (bk, bk1, tk, tk1) = (idx_b(k), idx_b(k + 1), idx_t(k), idx_t(k + 1));
        if reversed {
            out.indices.extend_from_slice(&[bk, tk1, bk1]);
            out.indices.extend_from_slice(&[bk, tk, tk1]);
        } else {
            out.indices.extend_from_slice(&[bk, bk1, tk1]);
            out.indices.extend_from_slice(&[bk, tk1, tk]);
        }
    }
    out.face_ranges.push(FaceRange {
        face: fid,
        start: range_start,
        count: out.indices.len() as u32 - range_start,
    });
    Ok(())
}

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

    let axis_v = Vector3::new(ax[0], ax[1], ax[2]);
    let Some((verts, tris)) =
        yang_rs::tessellate_torus_patch(center, axis_v, r_maj, r_min, &boundary, &holes, max_area)
    else {
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
