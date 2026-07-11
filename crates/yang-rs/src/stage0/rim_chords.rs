//! Stage-0 rim-chord machinery: rim/mixed chord contexts, chord-vertex
//! resolution, lateral-for-cap lookup, rim/ring/mixed crossing
//! collectors (extracted verbatim from stage0/mod.rs — spec
//! `specs/stage0_decomposition.md`, increment 5).

#[allow(clippy::wildcard_imports)]
use super::*;

/// N2-3a (spec `n2_stage4_junction_cluster_merge` §3): exact resolution
/// context for one disc face's rim chords, built once per handled pair.
/// Carries the disc polygon's rim sub-chords and the OTHER input polygon's
/// boundary sub-segments as exact rationals (classification is exact — no
/// tolerance), plus the disc's exact rim `Curve::Circle` geometry
/// (`disc_circle_edge`) snapped into the pair's canonical cap plane.
pub(crate) struct RimChordCtx {
    /// The disc's rim sub-chords (consecutive rim-ring samples), exact 2D.
    pub(crate) chords: Vec<(ExactPoint2, ExactPoint2)>,
    /// The OTHER input's boundary sub-segments (outer ring + holes), exact 2D.
    pub(crate) other_segs: Vec<(ExactPoint2, ExactPoint2)>,
    /// The exact rim circle's center, snapped onto the pair plane (identity
    /// for bit-exact coplanar input) so both minting branches stay in the
    /// cap plane.
    pub(crate) center: Point3,
    /// The exact rim circle's radius.
    pub(crate) radius: f64,
}

/// Build the N2-3a mint contexts for face `fi` of `brep` — ONE
/// [`RimChordCtx`] per rim circle (`poly` is the face's in-frame polygon,
/// `others` the partner faces' — ONE for a 1×1 pair, every other-side
/// polygon of the plane group for the n-ary path, spec
/// `m8_nary_tessellated_faces`). A plain disc yields exactly one (the outer
/// rim — byte-identical to the historical single-ctx path); an annular face
/// (M8 holed-disc increment 6, task #62) yields outer + one PER HOLE rim,
/// each with its own chord ring and exact circle, sharing the partner
/// polygons' boundary sub-segments. Empty for a non-disc/non-annular face
/// or a non-finite coordinate (→ the caller falls through to the raw lift,
/// byte-identical to the pre-N2-3a path). Without the annular arm, crossing
/// vertices on an annular face's rim chords resolved to raw CHORD lifts —
/// off-circle by the Stage-1 sagitta — populating its rim overrides with
/// on-chord points that reach chained outputs as mixed on-circle/on-chord
/// rims (the cut-3 re-entry wall + F0087/88/90 VertexOffSurface class).
pub(crate) fn rim_chord_ctxs(
    brep: &BRep,
    fi: usize,
    poly: &PolygonWithHoles,
    others: &[PolygonWithHoles],
    frame: &Frame,
) -> Vec<RimChordCtx> {
    let ring_exact = |ring: &[Point2]| -> Option<Vec<(ExactPoint2, ExactPoint2)>> {
        let n = ring.len();
        let mut out = Vec::with_capacity(n);
        for i in 0..n {
            let s = &ring[i];
            let e = &ring[(i + 1) % n];
            out.push((
                ExactPoint2::from_f64(s.x(), s.y())?,
                ExactPoint2::from_f64(e.x(), e.y())?,
            ));
        }
        Some(out)
    };
    // (ring, circle edge) per rim of the face, in loop order (outer first).
    let mut rims: Vec<(&[Point2], u32)> = Vec::new();
    if let Some(e) = disc_circle_edge(brep, fi) {
        rims.push((poly.outer.as_slice(), e));
    } else if let Some((outer_e, hole_es)) = annular_disc_face(brep, fi) {
        rims.push((poly.outer.as_slice(), outer_e));
        for (k, &he) in hole_es.iter().enumerate() {
            let Some(h) = poly.holes.get(k) else {
                return Vec::new();
            };
            rims.push((h.as_slice(), he));
        }
    } else {
        return Vec::new();
    }
    let other_segs = {
        let mut segs = Vec::new();
        for other in others {
            let Some(os) = ring_exact(&other.outer) else {
                return Vec::new();
            };
            segs.extend(os);
            for h in &other.holes {
                let Some(hs) = ring_exact(h) else {
                    return Vec::new();
                };
                segs.extend(hs);
            }
        }
        segs
    };
    let mut out = Vec::with_capacity(rims.len());
    for (ring, e) in rims {
        let Curve::Circle { center, radius, .. } = brep.edges()[e as usize].curve else {
            return Vec::new();
        };
        let Some(chords) = ring_exact(ring) else {
            return Vec::new();
        };
        out.push(RimChordCtx {
            chords,
            other_segs: other_segs.clone(),
            center: frame.snap(center),
            radius,
        });
    }
    out
}

/// M8-mixed analog of [`rim_chord_ctxs`] (spec
/// `m8_mixed_loop_coplanar_overlay` amendment 1): ONE [`RimChordCtx`] per
/// curved EDGE of a mixed face, its chord set = the ring sub-chords `masks`
/// attributes to that edge (an arc contributes its chain's chords; a
/// full-circle loop its whole ring). The minting/resolution machinery is
/// shared unchanged — each ctx carries its own exact circle.
pub(crate) fn mixed_chord_ctxs(
    brep: &BRep,
    poly: &PolygonWithHoles,
    masks: &[Vec<Option<u32>>],
    others: &[PolygonWithHoles],
    frame: &Frame,
) -> Vec<RimChordCtx> {
    let exact_seg = |s: &Point2, e: &Point2| -> Option<(ExactPoint2, ExactPoint2)> {
        Some((
            ExactPoint2::from_f64(s.x(), s.y())?,
            ExactPoint2::from_f64(e.x(), e.y())?,
        ))
    };
    let other_segs = {
        let mut segs = Vec::new();
        for ring in others
            .iter()
            .flat_map(|other| std::iter::once(&other.outer).chain(other.holes.iter()))
        {
            let n = ring.len();
            for i in 0..n {
                let Some(seg) = exact_seg(&ring[i], &ring[(i + 1) % n]) else {
                    return Vec::new();
                };
                segs.push(seg);
            }
        }
        segs
    };
    // Chords grouped per curved edge, in first-appearance order.
    let mut edge_order: Vec<u32> = Vec::new();
    let mut chords_of: BTreeMap<u32, Vec<(ExactPoint2, ExactPoint2)>> = BTreeMap::new();
    for (ring, mask) in std::iter::once(&poly.outer)
        .chain(poly.holes.iter())
        .zip(masks)
    {
        let n = ring.len();
        if n < 2 || mask.len() != n {
            continue;
        }
        for i in 0..n {
            let Some(e) = mask[i] else { continue };
            let Some(seg) = exact_seg(&ring[i], &ring[(i + 1) % n]) else {
                return Vec::new();
            };
            if !edge_order.contains(&e) {
                edge_order.push(e);
            }
            chords_of.entry(e).or_default().push(seg);
        }
    }
    let mut out = Vec::with_capacity(edge_order.len());
    for e in edge_order {
        let Curve::Circle { center, radius, .. } = brep.edges()[e as usize].curve else {
            return Vec::new();
        };
        out.push(RimChordCtx {
            chords: chords_of.remove(&e).unwrap_or_default(),
            other_segs: other_segs.clone(),
            center: frame.snap(center),
            radius,
        });
    }
    out
}

/// Outcome of [`resolve_rim_chord_vertex`].
pub(crate) enum RimResolve {
    /// Not strictly interior to any rim sub-chord — resolve as before.
    NotOnChord,
    /// Minted on the exact rim circle (I1), in the cap plane. `crossing` is
    /// true for the circle∩line branch (the point is a transversal junction
    /// with another input's edge — I2 pins it to that edge, so a sub-floor
    /// shared-mint group prefers it as the collapse target) and false for a
    /// pure x-event radial projection.
    OnCircle { point: Point3, crossing: bool },
    /// The exact discriminant of the circle∩line quadratic is negative for a
    /// claimed rim×other-edge crossing — a loud Stage-0 stop (spec §6).
    NoIntersection,
}

/// N2-3a: resolve one overlay vertex that may lie on a disc-rim chord (spec
/// §3 branch table, [#24 §4.5.5] — overlap boundaries carry exact curve
/// geometry). Uses the SAME exact on-chord predicate as
/// `collect_rim_crossings` (exact rational collinearity + strictly-interior
/// parameter with the 1e-6 endpoint margin — a vertex inside the margin is a
/// reconstructed rim sample, reconciled by the rim ULP-snap upstream).
pub(crate) fn resolve_rim_chord_vertex(
    ctx: &RimChordCtx,
    q: &ExactPoint2,
    qx: f64,
    qy: f64,
    frame: &Frame,
) -> RimResolve {
    // ── On a rim sub-chord? (the `collect_rim_crossings` predicate) ─────
    let mut on_chord: Option<usize> = None;
    for (ci, (s2, e2)) in ctx.chords.iter().enumerate() {
        let dx = &e2.x - &s2.x;
        let dy = &e2.y - &s2.y;
        let len2 = &dx * &dx + &dy * &dy;
        if len2 == RBig::ZERO {
            continue;
        }
        let wx = &q.x - &s2.x;
        let wy = &q.y - &s2.y;
        if &dx * &wy - &dy * &wx != RBig::ZERO {
            continue;
        }
        let t = (&dx * &wx + &dy * &wy) / &len2;
        let tf = t.to_f64().value();
        if tf > 1.0e-6 && tf < 1.0 - 1.0e-6 {
            on_chord = Some(ci);
            break;
        }
    }
    let Some(ci) = on_chord else {
        return RimResolve::NotOnChord;
    };
    let (cs, ce) = &ctx.chords[ci];
    let cdx = &ce.x - &cs.x;
    let cdy = &ce.y - &cs.y;

    // ── Also on another input's edge sub-segment (exact, transversal)? ──
    // A crossing must be minted at the exact circle∩line intersection (I2):
    // radial projection would slide it off the other input's edge, breaking
    // that solid's edge-split propagation. An other-edge COLLINEAR with the
    // chord defines no transversal junction and is skipped (the vertex then
    // radially projects like a pure subdivision point).
    let mut crossing: Option<(&ExactPoint2, RBig, RBig)> = None;
    for (s2, e2) in &ctx.other_segs {
        let dx = &e2.x - &s2.x;
        let dy = &e2.y - &s2.y;
        let len2 = &dx * &dx + &dy * &dy;
        if len2 == RBig::ZERO {
            continue;
        }
        let wx = &q.x - &s2.x;
        let wy = &q.y - &s2.y;
        if &dx * &wy - &dy * &wx != RBig::ZERO {
            continue;
        }
        let t = (&dx * &wx + &dy * &wy) / &len2;
        if t < RBig::ZERO || t > RBig::ONE {
            continue;
        }
        if &cdx * &dy - &cdy * &dx == RBig::ZERO {
            continue;
        }
        crossing = Some((s2, dx, dy));
        break;
    }

    if let Some((s2, dx, dy)) = crossing {
        // ── Exact 2D circle∩line intersection (spec §3 row 4, I2) ───────
        // Line p(t) = s + t·d against circle |p − c|² = r²: the quadratic
        // a·t² + b·t + c₀ = 0 with exact rational coefficients; the
        // discriminant sign is decided EXACTLY (spec §6), the root itself
        // via one f64 square root (closed-form, ~ULP accuracy — the same
        // class as the opposite-rim exact-radius projection).
        let (cu, cv) = frame.project(ctx.center);
        let (Some(cc), Ok(rr)) = (ExactPoint2::from_f64(cu, cv), rat(ctx.radius)) else {
            return RimResolve::NotOnChord;
        };
        let fx = &s2.x - &cc.x;
        let fy = &s2.y - &cc.y;
        let a_q = &dx * &dx + &dy * &dy;
        let b_q = (&dx * &fx + &dy * &fy) * RBig::from(2);
        let c_q = &fx * &fx + &fy * &fy - &rr * &rr;
        let disc = &b_q * &b_q - RBig::from(4) * (&a_q * &c_q);
        if disc < RBig::ZERO {
            return RimResolve::NoIntersection;
        }
        let a_f = a_q.to_f64().value();
        let b_f = b_q.to_f64().value();
        let c_f = c_q.to_f64().value();
        let sq = disc.to_f64().value().sqrt();
        // Numerically stable root pair (no catastrophic −b ± √D cancellation).
        let qq = if b_f >= 0.0 {
            -(b_f + sq) / 2.0
        } else {
            -(b_f - sq) / 2.0
        };
        let (t1, t2) = if qq != 0.0 {
            (qq / a_f, c_f / qq)
        } else {
            (0.0, 0.0)
        };
        let (sxf, syf) = (s2.x.to_f64().value(), s2.y.to_f64().value());
        let (dxf, dyf) = (dx.to_f64().value(), dy.to_f64().value());
        let p_at = |t: f64| [sxf + t * dxf, syf + t * dyf];
        // Choose the root on THIS chord's parameter interval (spec §3); if
        // that is ambiguous (a near-tangent line can put both roots over one
        // chord), the root nearest the overlay's exact chord crossing — the
        // two are within a sagitta of each other — disambiguates.
        let (csx, csy) = (cs.x.to_f64().value(), cs.y.to_f64().value());
        let (cdxf, cdyf) = (cdx.to_f64().value(), cdy.to_f64().value());
        let clen2 = cdxf * cdxf + cdyf * cdyf;
        let t_chord = |pp: [f64; 2]| ((pp[0] - csx) * cdxf + (pp[1] - csy) * cdyf) / clen2;
        let d2q = |pp: [f64; 2]| (pp[0] - qx) * (pp[0] - qx) + (pp[1] - qy) * (pp[1] - qy);
        let (p1, p2) = (p_at(t1), p_at(t2));
        let in1 = (0.0..=1.0).contains(&t_chord(p1));
        let in2 = (0.0..=1.0).contains(&t_chord(p2));
        let chosen = match (in1, in2) {
            (true, false) => p1,
            (false, true) => p2,
            _ => {
                if d2q(p1) <= d2q(p2) {
                    p1
                } else {
                    p2
                }
            }
        };
        return RimResolve::OnCircle {
            point: frame.lift(chosen[0], chosen[1]),
            crossing: true,
        };
    }

    // ── Pure x-event subdivision (spec §3 row 5, I1): radial projection ──
    // onto the exact circle in the cap plane — the own-cap analog of the
    // opposite-rim exact-radius projection (`opp_radius` below):
    // center + radius·normalize(lift(q) − center).
    let c3 = ctx.center.as_array();
    let l3 = frame.lift(qx, qy).as_array();
    let w = [l3[0] - c3[0], l3[1] - c3[1], l3[2] - c3[2]];
    let n = (w[0] * w[0] + w[1] * w[1] + w[2] * w[2]).sqrt();
    if n == 0.0 {
        // Degenerate (chord through the exact center — impossible for a
        // sampled rim): fall through unchanged rather than divide by zero.
        return RimResolve::NotOnChord;
    }
    let s = ctx.radius / n;
    RimResolve::OnCircle {
        point: Point3::new(c3[0] + w[0] * s, c3[1] + w[1] * s, c3[2] + w[2] * s),
        crossing: false,
    }
}

/// The cylinder lateral incident to a cap's circle edge, the OPPOSITE rim
/// edge, and the cylinder's axis params. The cap's circle edge appears in
/// exactly one `Surface::Cylinder` face's loops; that lateral's OTHER full-
/// circle rim is the opposite cap's edge.
///
/// Returns `Err(tag)` (→ the caller raises the loud residue) if the cap is not
/// a clean 2-rim cylinder cap (no incident cylinder lateral, or the lateral
/// does not have exactly two full-circle rims).
pub(crate) type LateralForCap = (usize, u32, [f64; 3], [f64; 3], f64);

pub(crate) fn lateral_for_cap(brep: &BRep, cap_edge: u32) -> Result<LateralForCap, &'static str> {
    for (fi, f) in brep.faces().iter().enumerate() {
        let Surface::Cylinder {
            axis_point,
            axis_dir,
            radius,
        } = f.surface
        else {
            continue;
        };
        if !f.outer_loop.contains(&cap_edge) {
            continue;
        }
        // Full-circle rims of this lateral.
        let rims: Vec<u32> = f
            .outer_loop
            .iter()
            .copied()
            .filter(|&e| {
                let ed = &brep.edges()[e as usize];
                matches!(ed.curve, Curve::Circle { .. }) && ed.start == ed.end
            })
            .collect();
        // Dedup (the lateral loop lists the seam twice but each rim once).
        let mut uniq = rims.clone();
        uniq.sort_unstable();
        uniq.dedup();
        if uniq.len() != 2 {
            return Err("rim-lateral-not-2rim");
        }
        let Some(&opposite) = uniq.iter().find(|&&e| e != cap_edge) else {
            return Err("rim-lateral-no-opposite");
        };
        return Ok((
            fi,
            opposite,
            axis_point.as_array(),
            normalize3(axis_dir.as_array()),
            radius,
        ));
    }
    if std::env::var_os("YANG_RIMLAT_PROBE").is_some() {
        for (fi, f) in brep.faces().iter().enumerate() {
            let in_outer = f.outer_loop.contains(&cap_edge);
            let in_inner = f.inner_loops.iter().any(|l| l.contains(&cap_edge));
            if in_outer || in_inner {
                eprintln!(
                    "[rimlat-probe] cap_edge={cap_edge} face={fi} outer={in_outer} \
                     inner={in_inner} surface={:?}",
                    f.surface
                );
            }
        }
    }
    Err("rim-lateral-none")
}

/// PR-M8 disc-rim crossing (§4.5.5 shared sampling for a CROSSING disc rim):
/// for each overlay vertex strictly interior to one of the disc rim polygon's
/// sub-chords, resolve it to its BIT-EXACT shared 3D point (`coords[vi]` — the
/// SAME point the cap override uses, so no T-junction) and record it on the
/// cap rim edge; also project that crossing's azimuth (in the cylinder axis
/// frame) onto the OPPOSITE rim circle and record the exact-radius point there
/// (so the opposite cap + the lateral stay conformal).
pub(crate) fn collect_rim_crossings(
    brep: &BRep,
    fi: usize,
    poly: &PolygonWithHoles,
    overlay: &ClassifiedOverlay,
    coords: &[Point3],
    rim_overrides: &mut RimSplitMap,
) -> Result<(), &'static str> {
    // Disc: one rim (the outer circle). Annular (M8 holed-disc): the outer rim
    // PLUS each hole rim — each propagated into ITS OWN cylinder lateral +
    // opposite rim via `lateral_for_cap(rim_edge)`. `poly.holes[k]` corresponds
    // to `annular_disc_face`'s hole-edge `k` (both follow `f.inner_loops` order;
    // `face_polygon_2d_tessellated` builds the hole rings in that order).
    if let Some(cap_edge) = disc_circle_edge(brep, fi) {
        return collect_ring_crossings(brep, cap_edge, &poly.outer, overlay, coords, rim_overrides);
    }
    if let Some((outer_edge, hole_edges)) = annular_disc_face(brep, fi) {
        collect_ring_crossings(
            brep,
            outer_edge,
            &poly.outer,
            overlay,
            coords,
            rim_overrides,
        )?;
        for (k, &he) in hole_edges.iter().enumerate() {
            let ring = poly.holes.get(k).ok_or("rim-hole-count-mismatch")?;
            collect_ring_crossings(brep, he, ring, overlay, coords, rim_overrides)?;
        }
        return Ok(());
    }
    Err("rim-not-disc")
}

/// Propagate the overlay's rim-chord split points for ONE circular rim
/// (`cap_edge`) into that rim's override AND its cylinder's opposite rim (so the
/// shared lateral stays conformal). Called once per rim by
/// [`collect_rim_crossings`] (outer + each hole for an annular cap).
pub(crate) fn collect_ring_crossings(
    brep: &BRep,
    cap_edge: u32,
    ring: &[Point2],
    overlay: &ClassifiedOverlay,
    coords: &[Point3],
    rim_overrides: &mut RimSplitMap,
) -> Result<(), &'static str> {
    // The cap circle's own geometry is not needed (crossing points come from
    // the resolved `coords`); only the OPPOSITE rim + the cylinder axis are.
    let (_lat_fi, opp_edge, axis_point, axis_dir, _r) = lateral_for_cap(brep, cap_edge)?;
    let Curve::Circle {
        center: opp_center,
        normal: opp_normal,
        radius: opp_radius,
    } = brep.edges()[opp_edge as usize].curve
    else {
        return Err("rim-opp-not-circle");
    };

    let n = ring.len();
    if n < 2 {
        return Err("rim-poly-degenerate");
    }
    let cap_entry = rim_overrides.entry(cap_edge).or_default();
    // Collected as (chord index, exact chord parameter, point) and sorted
    // before pushing (spec `m8_holed_disc_coplanar_overlay` §8 F1): the
    // override insertion order is then the EXACT boundary order along the rim
    // polygon, not the overlay-vertex enumeration order. Ring correctness no
    // longer depends on it (the ring sort has an exact tie-break), but the
    // deterministic order keeps probes readable and future consumers safe.
    let mut found: Vec<(usize, RBig, Point3)> = Vec::new();
    for i in 0..n {
        let s = &ring[i];
        let e = &ring[(i + 1) % n];
        let (Some(s2), Some(e2)) = (
            ExactPoint2::from_f64(s.x(), s.y()),
            ExactPoint2::from_f64(e.x(), e.y()),
        ) else {
            continue;
        };
        let dx = &e2.x - &s2.x;
        let dy = &e2.y - &s2.y;
        let len2 = &dx * &dx + &dy * &dy;
        if len2 == RBig::ZERO {
            continue;
        }
        let rim_probe = std::env::var_os("YANG_SPLIT_PROBE").is_some();
        for (vi, q) in overlay.exact_verts.iter().enumerate() {
            let wx = &q.x - &s2.x;
            let wy = &q.y - &s2.y;
            // Exact collinearity with the sub-chord's supporting line.
            if &dx * &wy - &dy * &wx != RBig::ZERO {
                continue;
            }
            // Strictly interior parameter, away from BOTH endpoints.
            let t = (&dx * &wx + &dy * &wy) / &len2;
            let tf = t.to_f64().value();
            if !(tf > 1.0e-6 && tf < 1.0 - 1.0e-6) {
                // M-C diagnosis probe (read-only, env-gated): report the
                // exactly-collinear chord vertices the endpoint window skips.
                if rim_probe && tf > 0.0 && tf < 1.0 {
                    eprintln!(
                        "[rim-cross-probe] edge={cap_edge} chord {i} vert {vi} t={tf} \
                         SKIPPED (endpoint window)"
                    );
                }
                continue;
            }
            // The BIT-EXACT shared point (the cap override uses the same one).
            let pt = coords[vi];
            if found.iter().any(|(_, _, p)| *p == pt) {
                if rim_probe {
                    eprintln!(
                        "[rim-cross-probe] edge={cap_edge} chord {i} vert {vi} t={tf} \
                         SKIPPED (duplicate pt)"
                    );
                }
                continue;
            }
            if rim_probe {
                eprintln!("[rim-cross-probe] edge={cap_edge} chord {i} vert {vi} t={tf} KEPT");
            }
            found.push((i, t, pt));
        }
    }
    found.sort_by(|a, b| a.0.cmp(&b.0).then_with(|| a.1.cmp(&b.1)));
    let cap_pts: Vec<Point3> = found.into_iter().map(|(_, _, p)| p).collect();
    for &pt in &cap_pts {
        if !cap_entry.contains(&pt) {
            cap_entry.push(pt);
        }
    }

    // Place each cap crossing onto the OPPOSITE rim by EXACT AXIAL PROJECTION:
    // strip the point's axial component and re-attach the radial offset at the
    // opposite rim's plane/radius. This is a direct 1:1 map (NO azimuth grid
    // search) — so it preserves the cap set's cardinality EXACTLY, including
    // femto-close split pairs, giving the two rims of the shared lateral matched
    // sample counts (the azimuth-merge conformality requirement). The old
    // 720-step f64 grid search collapsed femto-close azimuths to a single theta,
    // desynchronising the rims (18 cap → 12 opp — the M8 holed-disc `24 vs 30`
    // azimuth-merge wall). Radial magnitude is renormalised to `opp_radius`, so
    // this is exact for equal AND unequal cap/opposite radii.
    let oc = opp_center.as_array();
    let _ = opp_normal; // opposite plane is fixed by `oc`; normal no longer used
    let opp_entry = rim_overrides.entry(opp_edge).or_default();
    for &pt in &cap_pts {
        let p = pt.as_array();
        let w = [
            p[0] - axis_point[0],
            p[1] - axis_point[1],
            p[2] - axis_point[2],
        ];
        let axial = w[0] * axis_dir[0] + w[1] * axis_dir[1] + w[2] * axis_dir[2];
        let radial = [
            w[0] - axial * axis_dir[0],
            w[1] - axial * axis_dir[1],
            w[2] - axial * axis_dir[2],
        ];
        let rlen = (radial[0] * radial[0] + radial[1] * radial[1] + radial[2] * radial[2]).sqrt();
        if rlen < cad_primitives::TAU_WORK {
            // A rim point should never sit on the axis; if it does the geometry
            // is degenerate — skip rather than mint a NaN (P9: no silent bad pt).
            continue;
        }
        let scale = opp_radius / rlen;
        let opp_pt = Point3::new(
            oc[0] + radial[0] * scale,
            oc[1] + radial[1] * scale,
            oc[2] + radial[2] * scale,
        );
        if !opp_entry.contains(&opp_pt) {
            opp_entry.push(opp_pt);
        }
    }
    if std::env::var_os("YANG_SPLIT_PROBE").is_some() {
        eprintln!(
            "[rim-count] cap_edge={cap_edge} cap_pts={} cap_entry={} opp_edge={opp_edge} opp_entry={}",
            cap_pts.len(),
            rim_overrides.get(&cap_edge).map(|v| v.len()).unwrap_or(0),
            rim_overrides.get(&opp_edge).map(|v| v.len()).unwrap_or(0),
        );
    }
    Ok(())
}

/// M8-mixed (spec `m8_mixed_loop_coplanar_overlay` amendment 1): propagate
/// the overlay's curved-chord split points of a MIXED face into the curved
/// edges' chains. Per curved edge:
/// - a FULL-CIRCLE loop delegates to [`collect_ring_crossings`] (the disc
///   machinery: own rim + opposite rim of the shared cylinder);
/// - an ARC inserts each split point into its own chain AND, by the same
///   exact axial projection the ring path uses, into the OPPOSITE arc of the
///   shared partial-strip lateral — the strip pairs its two chains
///   index-for-index, so both must gain the point.
pub(crate) fn collect_mixed_crossings(
    brep: &BRep,
    fi: usize,
    poly: &PolygonWithHoles,
    seg_edges: &[Vec<Option<u32>>],
    overlay: &ClassifiedOverlay,
    coords: &[Point3],
    rim_overrides: &mut RimSplitMap,
) -> Result<(), &'static str> {
    for (ring, mask) in std::iter::once(&poly.outer)
        .chain(poly.holes.iter())
        .zip(seg_edges)
    {
        let n = ring.len();
        if n < 2 || mask.len() != n {
            continue;
        }
        // Curved edges of this ring, first-appearance order (deterministic).
        let mut curved: Vec<u32> = Vec::new();
        for e in mask.iter().flatten() {
            if !curved.contains(e) {
                curved.push(*e);
            }
        }
        for &e in &curved {
            let be = &brep.edges()[e as usize];
            if be.start == be.end {
                // Full-circle loop: the ring IS this edge's polyline — the
                // disc-rim propagation applies wholesale (own + opposite rim
                // of the cylinder found via `lateral_for_cap`).
                collect_ring_crossings(brep, e, ring, overlay, coords, rim_overrides)?;
                continue;
            }
            // ARC: gather split points strictly interior to THIS edge's
            // chords ((chord index, exact parameter) sorted — boundary order).
            let mut found: Vec<(usize, RBig, Point3)> = Vec::new();
            for i in 0..n {
                if mask[i] != Some(e) {
                    continue;
                }
                let s = &ring[i];
                let ee = &ring[(i + 1) % n];
                let (Some(s2), Some(e2)) = (
                    ExactPoint2::from_f64(s.x(), s.y()),
                    ExactPoint2::from_f64(ee.x(), ee.y()),
                ) else {
                    continue;
                };
                let dx = &e2.x - &s2.x;
                let dy = &e2.y - &s2.y;
                let len2 = &dx * &dx + &dy * &dy;
                if len2 == RBig::ZERO {
                    continue;
                }
                for (vi, q) in overlay.exact_verts.iter().enumerate() {
                    let wx = &q.x - &s2.x;
                    let wy = &q.y - &s2.y;
                    if &dx * &wy - &dy * &wx != RBig::ZERO {
                        continue;
                    }
                    let t = (&dx * &wx + &dy * &wy) / &len2;
                    let tf = t.to_f64().value();
                    if !(tf > 1.0e-6 && tf < 1.0 - 1.0e-6) {
                        continue;
                    }
                    let pt = coords[vi];
                    if found.iter().any(|(_, _, p)| *p == pt) {
                        continue;
                    }
                    found.push((i, t, pt));
                }
            }
            if found.is_empty() {
                continue;
            }
            found.sort_by(|a, b| a.0.cmp(&b.0).then_with(|| a.1.cmp(&b.1)));
            let pts: Vec<Point3> = found.into_iter().map(|(_, _, p)| p).collect();

            // Classify the shared lateral (spec `m8_mixed_arc_lateral_holed`
            // branch table): a structured 2-arc strip needs PAIRED insertion;
            // a chain-consuming (holed CDT) lateral takes the point from the
            // arc's own chain — one-sided.
            let lateral = arc_lateral_opposite(brep, fi, e)?;

            let cap_entry = rim_overrides.entry(e).or_default();
            for &pt in &pts {
                if !cap_entry.contains(&pt) {
                    cap_entry.push(pt);
                }
            }
            let ArcLateral::Strip {
                opp_edge,
                axis_point,
                axis_dir,
                opp_center,
                opp_radius,
            } = lateral
            else {
                // Chain-consuming lateral (`tessellate_lateral_holed_cdt`
                // splices every boundary loop from the shared per-edge
                // chains via `loop_polyline`): the inserted point is
                // consumed automatically and conformally — no strip
                // index-pairing constraint, so no opposite-arc projection.
                continue;
            };
            // Exact axial projection onto the opposite arc (the
            // `collect_ring_crossings` map: strip the axial component,
            // renormalise the radial offset to the opposite radius).
            let oc = opp_center;
            let opp_entry = rim_overrides.entry(opp_edge).or_default();
            for &pt in &pts {
                let p = pt.as_array();
                let w = [
                    p[0] - axis_point[0],
                    p[1] - axis_point[1],
                    p[2] - axis_point[2],
                ];
                let axial = w[0] * axis_dir[0] + w[1] * axis_dir[1] + w[2] * axis_dir[2];
                let radial = [
                    w[0] - axial * axis_dir[0],
                    w[1] - axial * axis_dir[1],
                    w[2] - axial * axis_dir[2],
                ];
                let rlen =
                    (radial[0] * radial[0] + radial[1] * radial[1] + radial[2] * radial[2]).sqrt();
                if rlen < cad_primitives::TAU_WORK {
                    continue;
                }
                let scale = opp_radius / rlen;
                let opp_pt = Point3::new(
                    oc[0] + radial[0] * scale,
                    oc[1] + radial[1] * scale,
                    oc[2] + radial[2] * scale,
                );
                if !opp_entry.contains(&opp_pt) {
                    opp_entry.push(opp_pt);
                }
            }
        }
    }
    Ok(())
}

/// Classification of the lateral adjacent to a mixed face's arc edge — how a
/// crossing split point must be propagated into its tessellation (spec
/// `m8_mixed_arc_lateral_holed` §2).
pub(crate) enum ArcLateral {
    /// Structured 2-arc partial strip: its tessellation pairs the two arc
    /// chains index-for-index, so insertion must be PAIRED (own chain + exact
    /// axial projection onto the opposite arc).
    Strip {
        opp_edge: u32,
        axis_point: [f64; 3],
        axis_dir: [f64; 3],
        opp_center: [f64; 3],
        opp_radius: f64,
    },
    /// Holed cylinder lateral routed through the KV14 unroll+CDT path
    /// (`tessellate_lateral_holed_cdt`), which splices every boundary loop
    /// from the shared per-edge chains via `loop_polyline`: an inserted chain
    /// point is consumed automatically — one-sided insertion suffices.
    ChainConsuming,
}

/// Find and classify the CYLINDER lateral adjacent to arc edge `e` of mixed
/// face `fi` (see [`ArcLateral`]). Loud typed tags for the unsupported
/// shapes: non-cylinder lateral; a holed lateral with a loop the CDT path
/// cannot splice (multi-edge loop containing a full-circle rim, or a
/// degree-4 `SurfacePair` edge); a hole-free lateral that is not the
/// structured 2-arc strip.
pub(crate) fn arc_lateral_opposite(
    brep: &BRep,
    fi: usize,
    e: u32,
) -> Result<ArcLateral, &'static str> {
    for (gi, g) in brep.faces().iter().enumerate() {
        if gi == fi {
            continue;
        }
        let in_loops = std::iter::once(&g.outer_loop)
            .chain(g.inner_loops.iter())
            .flatten()
            .any(|&ge| ge == e);
        if !in_loops {
            continue;
        }
        let Surface::Cylinder {
            axis_point,
            axis_dir,
            ..
        } = g.surface
        else {
            return Err("mixed-arc-lateral-not-cylinder");
        };
        if !g.inner_loops.is_empty() {
            // Holed lateral → Stage 1 routes it to the unroll+CDT path,
            // which consumes the arc's own chain — IF every loop is
            // `loop_polyline`-spliceable (spec branch 2 vs 3). A loop it
            // cannot splice would turn the typed capability wall into a
            // Stage-1 `MalformedTopology` ERROR, so verify here.
            let spliceable = std::iter::once(&g.outer_loop)
                .chain(g.inner_loops.iter())
                .all(|lp| {
                    let closed_single = lp.len() == 1 && {
                        let ed = &brep.edges()[lp[0] as usize];
                        matches!(ed.curve, Curve::Circle { .. } | Curve::Ellipse { .. })
                            && ed.start == ed.end
                    };
                    closed_single
                        || lp.iter().all(|&ge| {
                            let ed = &brep.edges()[ge as usize];
                            match ed.curve {
                                Curve::LineSegment => true,
                                Curve::Circle { .. } | Curve::Ellipse { .. } => ed.start != ed.end,
                                _ => false,
                            }
                        })
                });
            if !spliceable {
                return Err("mixed-arc-lateral-holed");
            }
            return Ok(ArcLateral::ChainConsuming);
        }
        let arcs: Vec<u32> = g
            .outer_loop
            .iter()
            .copied()
            .filter(|&ge| {
                let edge = &brep.edges()[ge as usize];
                matches!(edge.curve, Curve::Circle { .. }) && edge.start != edge.end
            })
            .collect();
        if arcs.len() != 2 || !arcs.contains(&e) {
            return Err("mixed-arc-lateral-unpaired");
        }
        let opp = if arcs[0] == e { arcs[1] } else { arcs[0] };
        let Curve::Circle { center, radius, .. } = brep.edges()[opp as usize].curve else {
            return Err("mixed-arc-lateral-unpaired");
        };
        let ap = axis_point.as_array();
        let ad = normalize3(axis_dir.as_array());
        return Ok(ArcLateral::Strip {
            opp_edge: opp,
            axis_point: ap,
            axis_dir: ad,
            opp_center: center.as_array(),
            opp_radius: radius,
        });
    }
    Err("mixed-arc-no-lateral")
}
