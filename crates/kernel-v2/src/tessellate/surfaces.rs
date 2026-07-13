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

mod sphere;
mod torus;
pub(crate) use sphere::*;
pub(crate) use torus::*;

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
