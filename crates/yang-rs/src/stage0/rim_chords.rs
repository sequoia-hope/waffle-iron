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

/// Classified lateral incident to a cap's full-circle rim edge — how a rim
/// crossing propagates to the OPPOSITE rim (task #131, spec
/// `m8_torus_profile_rim_crossing` §2).
pub(crate) enum CapLateral {
    /// Cylinder lateral (2 full-circle rims): exact AXIAL projection.
    Cylinder(LateralForCap),
    /// Torus lateral (2 minor-radius full-circle PROFILE rims — a
    /// revolved-circle body's seam discs): exact POLOIDAL-angle projection
    /// onto the opposite profile circle.
    Torus {
        /// The opposite profile rim edge.
        opp_edge: u32,
        /// Torus center (on the axis) and unit axis direction.
        center: [f64; 3],
        axis_dir: [f64; 3],
        /// Major radius (the poloidal angle is atan2(τ, ρ − major)).
        major: f64,
    },
}

pub(crate) fn lateral_for_cap(brep: &BRep, cap_edge: u32) -> Result<CapLateral, &'static str> {
    // Full-circle rims of a lateral's outer loop, deduped (the loop lists
    // the seam twice but each rim once). For a torus lateral, restrict to
    // PROFILE circles (radius ≈ minor) — the loop also carries seam arcs on
    // the outer-equator circle (radius ≈ major + minor), which are open
    // arcs (start != end) and thus already excluded.
    let full_circle_rims = |f: &crate::BRepFace| -> Vec<u32> {
        let mut rims: Vec<u32> = f
            .outer_loop
            .iter()
            .copied()
            .filter(|&e| {
                let ed = &brep.edges()[e as usize];
                matches!(ed.curve, Curve::Circle { .. }) && ed.start == ed.end
            })
            .collect();
        rims.sort_unstable();
        rims.dedup();
        rims
    };
    for (fi, f) in brep.faces().iter().enumerate() {
        match f.surface {
            Surface::Cylinder {
                axis_point,
                axis_dir,
                radius,
            } => {
                if !f.outer_loop.contains(&cap_edge) {
                    continue;
                }
                let uniq = full_circle_rims(f);
                if uniq.len() != 2 {
                    return Err("rim-lateral-not-2rim");
                }
                let Some(&opposite) = uniq.iter().find(|&&e| e != cap_edge) else {
                    return Err("rim-lateral-no-opposite");
                };
                return Ok(CapLateral::Cylinder((
                    fi,
                    opposite,
                    axis_point.as_array(),
                    normalize3(axis_dir.as_array()),
                    radius,
                )));
            }
            Surface::Torus {
                center,
                axis_dir,
                major_radius,
                minor_radius,
            } => {
                if !f.outer_loop.contains(&cap_edge) {
                    continue;
                }
                // Profile rims: closed circles of radius ≈ minor.
                let band = 1e-9 * (1.0 + major_radius + minor_radius);
                let profiles: Vec<u32> = full_circle_rims(f)
                    .into_iter()
                    .filter(|&e| {
                        matches!(brep.edges()[e as usize].curve,
                            Curve::Circle { radius, .. } if (radius - minor_radius).abs() <= band)
                    })
                    .collect();
                if profiles.len() != 2 || !profiles.contains(&cap_edge) {
                    return Err("rim-lateral-torus-not-2profile");
                }
                let Some(&opposite) = profiles.iter().find(|&&e| e != cap_edge) else {
                    return Err("rim-lateral-no-opposite");
                };
                if !matches!(brep.edges()[opposite as usize].curve, Curve::Circle { .. }) {
                    return Err("rim-lateral-no-opposite");
                }
                return Ok(CapLateral::Torus {
                    opp_edge: opposite,
                    center: center.as_array(),
                    axis_dir: normalize3(axis_dir.as_array()),
                    major: major_radius,
                });
            }
            _ => continue,
        }
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

/// Amendment 14 inc-3.2c (spec `m8_stage0_multiclass_cavity_arm` §11c
/// step 3): a rim point the SPLIT minted that is NOT UV-collinear with any
/// rim sub-chord (q_a/q_b live exactly on the OTHER input's edge), carried
/// into the rim-override chain through a side-channel. `s`/`e` identify
/// the owning rim sub-chord by exact endpoint equality; `t` is the exact
/// projection parameter along it (boundary ORDER only — the inserted
/// position is `pt`, bit-identical with the overlay emission).
#[derive(Clone)]
pub(crate) struct ExtraRimPoint {
    pub(crate) s: ExactPoint2,
    pub(crate) e: ExactPoint2,
    pub(crate) t: RBig,
    pub(crate) pt: Point3,
    /// True when the owning rim belongs to input A's face of the pair.
    pub(crate) side_a: bool,
}

/// PR-M8 disc-rim crossing (§4.5.5 shared sampling for a CROSSING disc rim):
/// for each overlay vertex strictly interior to one of the disc rim polygon's
/// sub-chords, resolve it to its BIT-EXACT shared 3D point (`coords[vi]` — the
/// SAME point the cap override uses, so no T-junction) and record it on the
/// cap rim edge; also project that crossing's azimuth (in the cylinder axis
/// frame) onto the OPPOSITE rim circle and record the exact-radius point there
/// (so the opposite cap + the lateral stay conformal). Returns the number of
/// `extra` split points consumed (the §11c A-leg accounting — the caller
/// fails LOUDLY if any extra found no owning sub-chord).
pub(crate) fn collect_rim_crossings(
    brep: &BRep,
    fi: usize,
    poly: &PolygonWithHoles,
    overlay: &ClassifiedOverlay,
    coords: &[Point3],
    extra: &[ExtraRimPoint],
    rim_overrides: &mut RimSplitMap,
) -> Result<usize, &'static str> {
    // Disc: one rim (the outer circle). Annular (M8 holed-disc): the outer rim
    // PLUS each hole rim — each propagated into ITS OWN cylinder lateral +
    // opposite rim via `lateral_for_cap(rim_edge)`. `poly.holes[k]` corresponds
    // to `annular_disc_face`'s hole-edge `k` (both follow `f.inner_loops` order;
    // `face_polygon_2d_tessellated` builds the hole rings in that order).
    if let Some(cap_edge) = disc_circle_edge(brep, fi) {
        return collect_ring_crossings(
            brep,
            cap_edge,
            &poly.outer,
            overlay,
            coords,
            extra,
            rim_overrides,
        );
    }
    if let Some((outer_edge, hole_edges)) = annular_disc_face(brep, fi) {
        let mut consumed = collect_ring_crossings(
            brep,
            outer_edge,
            &poly.outer,
            overlay,
            coords,
            extra,
            rim_overrides,
        )?;
        for (k, &he) in hole_edges.iter().enumerate() {
            let ring = poly.holes.get(k).ok_or("rim-hole-count-mismatch")?;
            consumed +=
                collect_ring_crossings(brep, he, ring, overlay, coords, extra, rim_overrides)?;
        }
        return Ok(consumed);
    }
    Err("rim-not-disc")
}

/// Bit-exact dedup + push of one projected opposite-rim sample, shared by
/// the translation and renormalisation arms (task #144). The env-gated
/// probe reports skips, distinguishing a pairwise collapse (two cap
/// samples → one image, the C0048/F0067 count-deficit mechanism) from a
/// dedup against a pre-existing entry.
#[allow(clippy::too_many_arguments)]
fn push_opp(
    opp_entry: &mut Vec<Point3>,
    opp_pt: Point3,
    pt: Point3,
    opp_preexisting: usize,
    pushed_srcs: &mut Vec<Point3>,
    rim_probe: bool,
    cap_edge: u32,
    opp_edge: u32,
) {
    if let Some(hit) = opp_entry.iter().position(|q| *q == opp_pt) {
        if rim_probe {
            let kind = if hit < opp_preexisting {
                "PREEXISTING"
            } else {
                "PAIRWISE-COLLAPSE"
            };
            // For a pairwise collapse, name the OTHER cap sample whose image
            // this one collided with — the twin-pair identity is the whole
            // diagnosis (task #144).
            let partner = if hit >= opp_preexisting {
                pushed_srcs
                    .get(hit - opp_preexisting)
                    .map(|s| format!(" partner_src={:?}", s.as_array()))
                    .unwrap_or_default()
            } else {
                String::new()
            };
            eprintln!(
                "[opp-proj] cap_edge={cap_edge} opp_edge={opp_edge} pt={:?} \
                 opp_pt={:?} SKIPPED ({kind} idx={hit}){partner}",
                pt.as_array(),
                opp_pt.as_array()
            );
        }
    } else {
        opp_entry.push(opp_pt);
        pushed_srcs.push(pt);
    }
}

/// Propagate the overlay's rim-chord split points for ONE circular rim
/// (`cap_edge`) into that rim's override AND its cylinder's opposite rim (so the
/// shared lateral stays conformal). Called once per rim by
/// [`collect_rim_crossings`] (outer + each hole for an annular cap).
#[allow(clippy::too_many_arguments)]
pub(crate) fn collect_ring_crossings(
    brep: &BRep,
    cap_edge: u32,
    ring: &[Point2],
    overlay: &ClassifiedOverlay,
    coords: &[Point3],
    extra: &[ExtraRimPoint],
    rim_overrides: &mut RimSplitMap,
) -> Result<usize, &'static str> {
    // The cap circle's own geometry is not needed (crossing points come from
    // the resolved `coords`); only the OPPOSITE rim + the lateral's frame are.
    let lateral = lateral_for_cap(brep, cap_edge)?;
    let (opp_edge, axis_point, axis_dir) = match &lateral {
        CapLateral::Cylinder((_, opp_edge, axis_point, axis_dir, _)) => {
            (*opp_edge, *axis_point, *axis_dir)
        }
        CapLateral::Torus {
            opp_edge,
            center,
            axis_dir,
            ..
        } => (*opp_edge, *center, *axis_dir),
    };
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
    let mut consumed = 0usize;
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
        // Amendment 14 inc-3.2c (§11c step 3): split-minted extra rim
        // points owned by THIS sub-chord (exact endpoint identity). Their
        // `t` is the exact projection parameter — boundary ORDER — and the
        // inserted position is the split's own resolved point, bit-shared
        // with the overlay emission.
        for ex in extra {
            if ex.s != s2 || ex.e != e2 {
                continue;
            }
            if found.iter().any(|(_, _, p)| *p == ex.pt) {
                consumed += 1; // bit-identical repeat — already carried
                continue;
            }
            if rim_probe {
                eprintln!(
                    "[rim-cross-probe] edge={cap_edge} chord {i} SPLIT-EXTRA t={} KEPT",
                    ex.t.to_f64().value()
                );
            }
            found.push((i, ex.t.clone(), ex.pt));
            consumed += 1;
        }
    }
    found.sort_by(|a, b| a.0.cmp(&b.0).then_with(|| a.1.cmp(&b.1)));
    let cap_pts: Vec<Point3> = found.into_iter().map(|(_, _, p)| p).collect();
    for &pt in &cap_pts {
        if !cap_entry.contains(&pt) {
            cap_entry.push(pt);
        }
    }

    // Place each cap crossing onto the OPPOSITE rim by an exact 1:1 map (NO
    // azimuth grid search) — preserving the cap set's cardinality EXACTLY,
    // including femto-close split pairs, so the two rims of the shared
    // lateral keep matched sample counts (the azimuth-merge conformality
    // requirement; the old 720-step f64 grid search collapsed femto-close
    // azimuths — the M8 holed-disc `24 vs 30` wall).
    // - CYLINDER: AXIAL projection — strip the axial component, re-attach
    //   the radial offset at the opposite rim's plane/radius (renormalised
    //   to `opp_radius`, exact for equal AND unequal radii).
    // - TORUS (task #131): POLOIDAL projection — the crossing's intrinsic
    //   poloidal angle φ = atan2(τ, ρ − R) (the `tessellate_torus_face`
    //   `phi_slot` convention) evaluated on the OPPOSITE profile circle:
    //   c₁ + r₁(cos φ · û + sin φ · â), û = the outward radial unit at the
    //   opposite meridian.
    let oc = opp_center.as_array();
    let _ = opp_normal; // opposite plane is fixed by `oc`; normal no longer used

    // Task #144 P10 REFUTATION RECORD (spec
    // `m8_exact_opposite_rim_projection`): an exact-translation arm
    // (`opp = p + (oc − cc)` in rational) was implemented here and REVERTED.
    // It fixed the C0048/F0067 azimuth-merge count deficits (same-ray radial
    // twin pairs — a #142 chord-depth fused survivor + its on-circle twin at
    // bit-identical exact azimuth — collapse to ONE on-circle image below;
    // merge counts are SYMMETRIC, measured `[ring-build]` C0048: 3=3), but
    // mirrored chord-DEEP samples onto rims with no own crossings, where
    // nothing relocates them: n2_rim_mint_adversary caught off-surface loop
    // vertices (residual ≈ sagitta) on Ok outputs — SILENT-WRONG. A correct
    // fix must place ON-CIRCLE (within the stage1 rim band), injectively
    // (deterministic tangential separation for exact-azimuth twins),
    // merge-MIRRORING, and exact-order-consistent — snap-rounding grade
    // ([#52] Hobby), a design increment. Until then the collapse stays and
    // the downstream azimuth-merge count wall stays LOUD (never silent).
    let opp_entry = rim_overrides.entry(opp_edge).or_default();
    let opp_preexisting = opp_entry.len();
    let rim_probe = std::env::var_os("YANG_SPLIT_PROBE").is_some();
    let mut pushed_srcs: Vec<Point3> = Vec::new();
    let _ = (axis_point, axis_dir); // axis now read inside `opposite_rim_image`
    for &pt in &cap_pts {
        let Some(opp_pt) = opposite_rim_image(&lateral, oc, opp_radius, pt)? else {
            // On-axis rim point: degenerate geometry — skip rather than mint
            // a NaN (P9: no silent bad point).
            continue;
        };
        push_opp(
            opp_entry,
            opp_pt,
            pt,
            opp_preexisting,
            &mut pushed_srcs,
            rim_probe,
            cap_edge,
            opp_edge,
        );
    }
    if std::env::var_os("YANG_SPLIT_PROBE").is_some() {
        eprintln!(
            "[rim-count] cap_edge={cap_edge} cap_pts={} cap_entry={} opp_edge={opp_edge} opp_entry={}",
            cap_pts.len(),
            rim_overrides.get(&cap_edge).map(|v| v.len()).unwrap_or(0),
            rim_overrides.get(&opp_edge).map(|v| v.len()).unwrap_or(0),
        );
    }
    Ok(consumed)
}

/// Exact 1:1 opposite-rim image of one cap-rim point — cylinder AXIAL /
/// torus POLOIDAL projection (the #143 count-preserving map used by
/// [`collect_ring_crossings`], factored so rim membership refinement mints
/// matched samples on both rims). `Ok(None)` = degenerate on-axis point
/// (P9: the caller skips, never mints a NaN).
pub(crate) fn opposite_rim_image(
    lateral: &CapLateral,
    oc: [f64; 3],
    opp_radius: f64,
    pt: Point3,
) -> Result<Option<Point3>, &'static str> {
    let (axis_point, axis_dir) = match lateral {
        CapLateral::Cylinder((_, _, ap, ad, _)) => (*ap, *ad),
        CapLateral::Torus {
            center, axis_dir, ..
        } => (*center, *axis_dir),
    };
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
        return Ok(None);
    }
    Ok(Some(match lateral {
        CapLateral::Cylinder(_) => {
            let scale = opp_radius / rlen;
            Point3::new(
                oc[0] + radial[0] * scale,
                oc[1] + radial[1] * scale,
                oc[2] + radial[2] * scale,
            )
        }
        CapLateral::Torus { center, major, .. } => {
            // φ from the crossing point (axis frame at the torus center).
            let wc = [p[0] - center[0], p[1] - center[1], p[2] - center[2]];
            let tau = wc[0] * axis_dir[0] + wc[1] * axis_dir[1] + wc[2] * axis_dir[2];
            let rad = [
                wc[0] - tau * axis_dir[0],
                wc[1] - tau * axis_dir[1],
                wc[2] - tau * axis_dir[2],
            ];
            let rho = (rad[0] * rad[0] + rad[1] * rad[1] + rad[2] * rad[2]).sqrt();
            let phi = tau.atan2(rho - major);
            // Outward radial unit at the OPPOSITE meridian.
            let co = [oc[0] - center[0], oc[1] - center[1], oc[2] - center[2]];
            let co_ax = co[0] * axis_dir[0] + co[1] * axis_dir[1] + co[2] * axis_dir[2];
            let u = [
                co[0] - co_ax * axis_dir[0],
                co[1] - co_ax * axis_dir[1],
                co[2] - co_ax * axis_dir[2],
            ];
            let ulen = (u[0] * u[0] + u[1] * u[1] + u[2] * u[2]).sqrt();
            if ulen < cad_primitives::TAU_WORK {
                return Err("rim-lateral-torus-degenerate-meridian");
            }
            let (sp, cp) = phi.sin_cos();
            let s = opp_radius * cp / ulen;
            Point3::new(
                oc[0] + u[0] * s + opp_radius * sp * axis_dir[0],
                oc[1] + u[1] * s + opp_radius * sp * axis_dir[1],
                oc[2] + u[2] * s + opp_radius * sp * axis_dir[2],
            )
        }
    }))
}

/// M8 rim membership refinement (spec `m8_stage0_rim_membership_refine`,
/// ALWAYS-ON since the 2026-08-14 corpus flip): subdivide this face's rim
/// rings with
/// exact on-circle samples until NO partner chain vertex strictly inside
/// the exact rim circle lies strictly outside the chord polygon (in a sag
/// crescent). The §4.5.5 2D Boolean classifies membership against the
/// chord polygon; a partner feature in a crescent is misclassified
/// (measured F0067: 126 gear root-region corners `AOnly` at dr −3.1e-4..
/// −1.34e-3 inside the exact circle → the A-top rim-weave → Stage-6
/// non-2-manifold). Refinement makes polygonal membership agree with the
/// exact 2D Boolean for every partner feature — Yang §4.2.1's
/// conservative-discretization principle applied to the §4.5.5 shared
/// plane.
///
/// `partner_chain` = the partner polygon's chain coordinates with its own
/// rim-sample coordinates EXCLUDED (the §2c chain/rim domain split).
/// Inserted samples propagate to all four consumers bit-shared: the `poly`
/// ring (overlay region boundary), the `rim` resolution map, and the
/// cap + opposite rim overrides (via [`opposite_rim_image`], keeping the
/// azimuth-merge sample counts matched — the C0048 #143/#144 lesson).
/// Every guard is LOUD (`Err` → the caller's pair_err residue).
pub(crate) fn refine_rim_membership(
    brep: &BRep,
    fi: usize,
    poly: &mut PolygonWithHoles,
    rim: &mut BTreeMap<ExactPoint2, Point3>,
    partner_chain: &[Point2],
    frame: &Frame,
    rim_overrides: &mut RimSplitMap,
) -> Result<usize, &'static str> {
    // Rim rings of this face, in loop order (outer first, then holes). A
    // hole ring has the IDENTICAL predicate: a partner vertex inside the
    // hole circle but outside its inscribed chord polygon is exactly the
    // misclassified-crescent case.
    #[derive(Clone, Copy)]
    enum RingRef {
        Outer,
        Hole(usize),
    }
    let mut rims: Vec<(RingRef, u32)> = Vec::new();
    if let Some(e) = disc_circle_edge(brep, fi) {
        rims.push((RingRef::Outer, e));
    } else if let Some((outer_e, hole_es)) = annular_disc_face(brep, fi) {
        rims.push((RingRef::Outer, outer_e));
        for (k, &he) in hole_es.iter().enumerate() {
            if poly.holes.get(k).is_none() {
                return Err("rim-refine-hole-count");
            }
            rims.push((RingRef::Hole(k), he));
        }
    } else {
        return Ok(0);
    }

    let split_probe = std::env::var_os("YANG_SPLIT_PROBE").is_some();
    let mut inserted_total = 0usize;
    for (ring_ref, cap_edge) in rims {
        let Curve::Circle { center, radius, .. } = brep.edges()[cap_edge as usize].curve else {
            return Err("rim-refine-not-circle");
        };
        let (cu, cv) = frame.project(center);
        let (Some(cc), Ok(rr)) = (ExactPoint2::from_f64(cu, cv), rat(radius)) else {
            return Err("rim-refine-center");
        };
        let rr2 = &rr * &rr;
        // Band floor: a partner vertex within the Stage-1 rim band of the
        // circle is on-circle content (junction/tangency machinery owns
        // it); the floor also bounds the refinement depth.
        let band = 1e-9 * (1.0 + radius);
        let mut feats: Vec<ExactPoint2> = Vec::new();
        for q in partner_chain {
            let d = ((q.x() - cu).powi(2) + (q.y() - cv).powi(2)).sqrt();
            if d < radius - band {
                let Some(eq) = ExactPoint2::from_f64(q.x(), q.y()) else {
                    return Err("rim-refine-partner-coord");
                };
                feats.push(eq);
            }
        }
        if feats.is_empty() {
            continue;
        }
        let ring: &mut Vec<Point2> = match ring_ref {
            RingRef::Outer => &mut poly.outer,
            RingRef::Hole(k) => &mut poly.holes[k],
        };
        let mut new_pts: Vec<Point3> = Vec::new();
        let mut rounds = 0usize;
        // Ring orientation in-frame (shoelace): the azimuth-midpoint walk
        // below needs the ring's angular direction; a CW-in-frame ring
        // (an opposite-normal cap) traverses decreasing azimuth.
        let ccw = {
            let mut area2 = 0.0f64;
            let n = ring.len();
            for i in 0..n {
                let s = ring[i];
                let e = ring[(i + 1) % n];
                area2 += s.x() * e.y() - e.x() * s.y();
            }
            if area2 == 0.0 {
                return Err("rim-refine-zero-area-ring");
            }
            area2 > 0.0
        };
        loop {
            let n = ring.len();
            if n < 3 {
                return Err("rim-refine-degenerate-ring");
            }
            let mut split_spans: Vec<usize> = Vec::new();
            for i in 0..n {
                let s = ring[i];
                let e = ring[(i + 1) % n];
                let (Some(s2), Some(e2)) = (
                    ExactPoint2::from_f64(s.x(), s.y()),
                    ExactPoint2::from_f64(e.x(), e.y()),
                ) else {
                    return Err("rim-refine-ring-coord");
                };
                let dx = &e2.x - &s2.x;
                let dy = &e2.y - &s2.y;
                let cxs = &dx * (&cc.y - &s2.y) - &dy * (&cc.x - &s2.x);
                if cxs == RBig::ZERO {
                    // A diameter chord (center exactly on the chord line)
                    // has no well-defined crescent side — loud residue.
                    return Err("rim-refine-chord-through-center");
                }
                let center_pos = cxs > RBig::ZERO;
                for q in &feats {
                    let cq = &dx * (&q.y - &s2.y) - &dy * (&q.x - &s2.x);
                    if cq == RBig::ZERO {
                        // Exactly on the chord line: an overlay on-chord
                        // vertex — the existing mint machinery owns it.
                        continue;
                    }
                    if (cq > RBig::ZERO) != center_pos {
                        let du = &q.x - &cc.x;
                        let dv = &q.y - &cc.y;
                        if &du * &du + &dv * &dv < rr2 {
                            split_spans.push(i);
                            break;
                        }
                    }
                }
            }
            if split_spans.is_empty() {
                break;
            }
            rounds += 1;
            if rounds > 32 {
                // P10: never silently accept a residual crescent feature.
                return Err("rim-refine-depth");
            }
            // Insert back-to-front so earlier span indices stay valid; the
            // wrap span (i = n−1) appends at the ring end (boundary order).
            for &i in split_spans.iter().rev() {
                let (s, e) = if ccw {
                    (ring[i], ring[(i + 1) % ring.len()])
                } else {
                    (ring[(i + 1) % ring.len()], ring[i])
                };
                let ts = (s.y() - cv).atan2(s.x() - cu);
                let te = (e.y() - cv).atan2(e.x() - cu);
                let mut dt = te - ts;
                while dt <= 0.0 {
                    dt += std::f64::consts::TAU;
                }
                if dt >= std::f64::consts::PI {
                    return Err("rim-refine-span-ge-pi");
                }
                let tm = ts + dt / 2.0;
                let (u, v) = (cu + radius * tm.cos(), cv + radius * tm.sin());
                if !u.is_finite() || !v.is_finite() {
                    return Err("rim-refine-nonfinite");
                }
                // 3D: the x-event mint convention — radial projection onto
                // the exact circle in the cap plane through the 3D center.
                let c3 = center.as_array();
                let l3 = frame.lift(u, v).as_array();
                let w = [l3[0] - c3[0], l3[1] - c3[1], l3[2] - c3[2]];
                let nl = (w[0] * w[0] + w[1] * w[1] + w[2] * w[2]).sqrt();
                if nl == 0.0 {
                    return Err("rim-refine-degenerate-lift");
                }
                let sc = radius / nl;
                let p3 = Point3::new(c3[0] + w[0] * sc, c3[1] + w[1] * sc, c3[2] + w[2] * sc);
                ring.insert(i + 1, Point2::new(u, v));
                let Some(key) = ExactPoint2::from_f64(u, v) else {
                    return Err("rim-refine-key");
                };
                rim.insert(key, p3);
                new_pts.push(p3);
                inserted_total += 1;
            }
        }
        if new_pts.is_empty() {
            continue;
        }
        // Overrides: cap ring + the exact 1:1 opposite image (matched
        // counts for the shared lateral's azimuth merge).
        let lateral = lateral_for_cap(brep, cap_edge)?;
        let opp_edge = match &lateral {
            CapLateral::Cylinder((_, e, _, _, _)) => *e,
            CapLateral::Torus { opp_edge, .. } => *opp_edge,
        };
        let Curve::Circle {
            center: opp_center,
            radius: opp_radius,
            ..
        } = brep.edges()[opp_edge as usize].curve
        else {
            return Err("rim-refine-opp-not-circle");
        };
        let oc = opp_center.as_array();
        {
            let cap_entry = rim_overrides.entry(cap_edge).or_default();
            for &pt in &new_pts {
                if !cap_entry.contains(&pt) {
                    cap_entry.push(pt);
                }
            }
        }
        let opp_entry = rim_overrides.entry(opp_edge).or_default();
        for &pt in &new_pts {
            let Some(opp_pt) = opposite_rim_image(&lateral, oc, opp_radius, pt)? else {
                continue;
            };
            if !opp_entry.contains(&opp_pt) {
                opp_entry.push(opp_pt);
            }
        }
        if split_probe {
            eprintln!(
                "[rim-refine] face={fi} edge={cap_edge} inserted={} rounds={rounds} \
                 feats_inside={} ring_len={}",
                new_pts.len(),
                feats.len(),
                match ring_ref {
                    RingRef::Outer => poly.outer.len(),
                    RingRef::Hole(k) => poly.holes[k].len(),
                }
            );
        }
    }
    Ok(inserted_total)
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
                // of the cylinder found via `lateral_for_cap`). No split
                // extras on the mixed path (§11e inc-3.2c scope: a
                // mixed-face split customer is caught by the ladder's
                // unconsumed-extras check, loud).
                collect_ring_crossings(brep, e, ring, overlay, coords, &[], rim_overrides)?;
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

/// Amendment 16 (spec `m8_stage0_multiclass_cavity_arm` §14): the
/// increment-4 sub-floor shared-mint collapse groups, carried into the
/// revert authorities. A qualified (sub-floor-ANCHORED) group reverts
/// WHOLE — every member to the ONE shared chord target (the elected
/// member's lift) — or not at all: a per-member revert tears the A14.2
/// identification into a real-scale phantom pair whose opposite-rim
/// images bit-collide by ulp lottery (the C0048 68v67 azimuth-merge
/// wall). Wide-anchored groups (coincident junction images from far
/// anchors) are NOT qualified and keep per-member semantics. ALWAYS-ON
/// since the inc-2 corpus flip (2026-07-31: zero CORRECT→ERROR; C0048
/// past the count wall; F0067's desync-manufactured N17 deferral gone).
#[derive(Default)]
pub(crate) struct CollapseGroups {
    /// member vi → all members of its qualified group (incl. itself).
    pub(crate) members: std::collections::BTreeMap<usize, Vec<usize>>,
    /// member vi → the group's shared chord target (elected member's
    /// lift, bit-identical across the group).
    pub(crate) shared_lift: std::collections::BTreeMap<usize, Point3>,
}

impl CollapseGroups {
    /// The revert target for `vi`: the group's shared chord target for a
    /// qualified member, else the caller's own chord lift.
    pub(crate) fn effective_lift(&self, vi: usize, own: Point3) -> Point3 {
        self.shared_lift.get(&vi).copied().unwrap_or(own)
    }

    /// The revert unit containing `vi`: the whole qualified group, else
    /// the singleton.
    pub(crate) fn revert_unit(&self, vi: usize) -> Vec<usize> {
        self.members.get(&vi).cloned().unwrap_or_else(|| vec![vi])
    }
}

/// Amendment 13 inc-3.5 (spec `m8_stage0_multiclass_cavity_arm` §10d):
/// rim-chain boundary-order settle check.
///
/// Every rim chord's crossing set is consumed by TWO orderings: the cap
/// overlay emits the chain in chord-parameter order, while the ring builder
/// (`stage1_tessellate` slot sort) orders the same points by azimuth — the
/// revolved lateral's arc-length parameterization, which cannot honor a
/// non-monotone chain (forcing boundary order there would emit bowtie
/// laterals). A JUNCTION mint (exact circle∩line) is azimuthally displaced
/// from its chord anchor by up to the snap displacement, so a kept junction
/// beside a fold-REVERTED neighbor can azimuthally leap past it (measured:
/// R0059 op 001, v25M −104.281° vs v19rev −105.055° at chord order
/// v25 < v19) — the two consumers then disagree and the shared boundary
/// desynchronizes (unpaired mesh edges).
///
/// This check runs when a gate pass ends quiescent: per chord, gather the
/// crossing set with the SAME exact collinearity + parameter-window
/// predicate as `collect_ring_crossings` (the policed set IS the propagated
/// set), and verify the resolved points' angular order about the rim center
/// matches chord-parameter order — decided in EXACT rational arithmetic
/// (orientation signs; no f64 atan2, no band). On an adjacent inversion,
/// revert the DISPLACED member(s) — two undisplaced chord points cannot
/// invert, so a victim always exists — to their chord lift (amendment-2
/// semantics at chord granularity), restore any merge partner of a
/// reverted target, and mark the target merge-ineligible (`settled`).
/// One inversion per firing; the caller re-runs the gate ladder and calls
/// again. Termination: `settled` grows monotonically. Interior-pair scope
/// only: a leap past the chord's END CORNER is a named non-goal (no
/// measured customer; corners are not revertable anchors).
#[allow(clippy::too_many_arguments)]
pub(crate) fn settle_rim_chain_order(
    ctxs: &[RimChordCtx],
    overlay: &ClassifiedOverlay,
    coords: &mut [Point3],
    minted_mark: &[bool],
    frame: &Frame,
    merges: &[(u32, u32, Point3)],
    settled: &mut std::collections::BTreeSet<u32>,
    groups: &CollapseGroups,
    probe_flip: bool,
) -> usize {
    let rat2 = |u: f64, v: f64| ExactPoint2::from_f64(u, v);
    for ctx in ctxs {
        let (cu, cv) = frame.project(ctx.center);
        let Some(c2) = rat2(cu, cv) else { continue };
        for (s2, e2) in &ctx.chords {
            // Chord arc direction about the center, exact.
            let sx = &s2.x - &c2.x;
            let sy = &s2.y - &c2.y;
            let ex = &e2.x - &c2.x;
            let ey = &e2.y - &c2.y;
            let dir = &sx * &ey - &sy * &ex;
            if dir == RBig::ZERO {
                continue; // degenerate (antipodal) — not this check's class
            }
            let ccw = dir > RBig::ZERO;
            // Crossing set: exact collinearity, interior parameter window —
            // mirrors `collect_ring_crossings` verbatim.
            let dx = &e2.x - &s2.x;
            let dy = &e2.y - &s2.y;
            let len2 = &dx * &dx + &dy * &dy;
            if len2 == RBig::ZERO {
                continue;
            }
            let mut found: Vec<(RBig, usize)> = Vec::new();
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
                found.push((t, vi));
            }
            if found.len() < 2 {
                continue;
            }
            found.sort_by(|a, b| a.0.cmp(&b.0));
            // Resolved angular positions (exact rationals of the resolved
            // projections — deterministic, jitter-free order predicate).
            let proj: Vec<Option<ExactPoint2>> = found
                .iter()
                .map(|&(_, vi)| {
                    let (u, v) = frame.project(coords[vi]);
                    rat2(u, v)
                })
                .collect();
            for i in 0..found.len() - 1 {
                let (Some(pi), Some(pj)) = (&proj[i], &proj[i + 1]) else {
                    continue;
                };
                let ax = &pi.x - &c2.x;
                let ay = &pi.y - &c2.y;
                let bx = &pj.x - &c2.x;
                let by = &pj.y - &c2.y;
                let cross = &ax * &by - &ay * &bx;
                let inverted = if ccw {
                    cross < RBig::ZERO
                } else {
                    cross > RBig::ZERO
                };
                if !inverted {
                    continue;
                }
                // Revert the displaced member(s) of the pair.
                let mut reverted = 0usize;
                for &(_, vi) in [&found[i], &found[i + 1]] {
                    let q = overlay.verts[vi];
                    // Amendment 16 (spec §14): a qualified collapse-group
                    // member's chord target is the group's SHARED lift, and
                    // its revert is group-atomic — the identification either
                    // holds or reverts whole, so quiescence can never strand
                    // a half-fused group (the §13i class, preventively).
                    let lift = groups.effective_lift(vi, frame.lift(q.x(), q.y()));
                    if let Some(&(mp, mq, orig)) =
                        merges.iter().find(|&&(mp, _, _)| mp as usize == vi)
                    {
                        // A merged partner out of order: restore it and
                        // block its target from re-absorbing it.
                        if coords[vi] != orig {
                            if probe_flip {
                                eprintln!(
                                    "[rim-order-settle] partner {vi} restored \
                                     (target {mq}) {:?} -> {orig:?}",
                                    coords[vi]
                                );
                            }
                            coords[vi] = orig;
                            settled.insert(mq);
                            let _ = mp;
                            reverted += 1;
                        }
                    } else if minted_mark[vi] && coords[vi] != lift {
                        for m in groups.revert_unit(vi) {
                            let qm = overlay.verts[m];
                            let lm = groups.effective_lift(m, frame.lift(qm.x(), qm.y()));
                            if coords[m] == lm {
                                continue;
                            }
                            if probe_flip {
                                let tag = if m == vi { "" } else { " (group-atomic)" };
                                eprintln!(
                                    "[rim-order-settle] vert {m} angular-order \
                                     inversion -> chord {lm:?} (was {:?}){tag}",
                                    coords[m]
                                );
                            }
                            coords[m] = lm;
                            settled.insert(m as u32);
                            for &(mp, mq, orig) in merges {
                                if mq as usize == m && coords[mp as usize] != orig {
                                    if probe_flip {
                                        eprintln!(
                                            "[rim-order-settle]   partner {mp} of \
                                             reverted target {mq} restored"
                                        );
                                    }
                                    coords[mp as usize] = orig;
                                }
                            }
                            reverted += 1;
                        }
                    }
                }
                if reverted > 0 {
                    return reverted;
                }
                // Defensive: an inversion with no displaced member should be
                // geometrically impossible — probe loudly, keep scanning.
                if probe_flip {
                    eprintln!(
                        "[rim-order-settle] UNREVERTABLE inversion verts \
                         {} / {} (no displaced member)",
                        found[i].1,
                        found[i + 1].1
                    );
                }
            }
        }
    }
    0
}

#[cfg(test)]
mod settle_tests {
    //! Amendment 13 inc-3.5 unit oracles (spec
    //! `m8_stage0_multiclass_cavity_arm` §10d): rim-chain boundary-order
    //! settle check. Fixtures live on the z=0 plane with the identity
    //! frame and the UNIT circle centered at the origin; the policed chord
    //! is the vertical secant x = cos 30° spanning azimuth [−30°, +30°].
    //! R0059's measured shape in miniature: a kept junction mint whose
    //! on-circle azimuth LEAPS PAST a fold-reverted neighbor's chord
    //! azimuth, with a merged partner twinned onto the junction.

    use super::{settle_rim_chain_order, CollapseGroups, Frame, RimChordCtx};
    use crate::coplanar_overlay::{ClassifiedOverlay, ExactPoint2};
    use cad_primitives::{Point2, Point3};
    use std::collections::BTreeMap;
    use std::collections::BTreeSet;

    const CX: f64 = 0.866_025_403_784_438_7; // cos 30°

    fn frame_z0() -> Frame {
        Frame {
            n: [0.0, 0.0, 1.0],
            d: 0.0,
            o: [0.0, 0.0, 0.0],
            e1: [1.0, 0.0, 0.0],
            e2: [0.0, 1.0, 0.0],
        }
    }

    fn overlay_of(uv: &[(f64, f64)]) -> ClassifiedOverlay {
        ClassifiedOverlay {
            verts: uv.iter().map(|&(u, v)| Point2::new(u, v)).collect(),
            exact_verts: uv
                .iter()
                .map(|&(u, v)| ExactPoint2::from_f64(u, v).unwrap())
                .collect(),
            tris: Vec::new(),
            class: Vec::new(),
            poly_a: Vec::new(),
            poly_b: Vec::new(),
            fused: BTreeMap::new(),
        }
    }

    fn ctx_unit_circle() -> RimChordCtx {
        RimChordCtx {
            chords: vec![(
                ExactPoint2::from_f64(CX, -0.5).unwrap(),
                ExactPoint2::from_f64(CX, 0.5).unwrap(),
            )],
            other_segs: Vec::new(),
            center: Point3::new(0.0, 0.0, 0.0),
            radius: 1.0,
        }
    }

    fn on_circle(deg: f64) -> Point3 {
        let (s, c) = deg.to_radians().sin_cos();
        Point3::new(c, s, 0.0)
    }

    /// Radial projection of a chord point onto the circle — azimuth-true.
    fn radial(u: f64, v: f64) -> Point3 {
        let r = (u * u + v * v).sqrt();
        Point3::new(u / r, v / r, 0.0)
    }

    /// The measured R0059 shape: kept junction (chord t=0.2) resolved at
    /// azimuth +10°, PAST the reverted neighbor (t=0.4, at its chord lift,
    /// azimuth ≈ −6.6°). One settle call reverts exactly the junction,
    /// restores its merged partner, and blocks the target; a second call
    /// finds the chord monotone.
    #[test]
    fn junction_leap_reverts_and_restores_partner() {
        let overlay = overlay_of(&[(CX, -0.3), (CX, -0.1), (CX, 0.2), (2.0, 2.0)]);
        let p_orig = Point3::new(2.0, 2.0, 0.0);
        let junction = on_circle(10.0);
        let mut coords = vec![
            junction,                   // v0 kept junction mint — the leaper
            Point3::new(CX, -0.1, 0.0), // v1 reverted mint (at lift)
            radial(CX, 0.2),            // v2 kept radial mint — azimuth-true
            junction,                   // v3 merged partner (bit-twin of v0)
        ];
        let minted = vec![true, true, true, false];
        let merges = vec![(3u32, 0u32, p_orig)];
        let mut settled: BTreeSet<u32> = BTreeSet::new();
        let frame = frame_z0();
        let n = settle_rim_chain_order(
            &[ctx_unit_circle()],
            &overlay,
            &mut coords,
            &minted,
            &frame,
            &merges,
            &mut settled,
            &CollapseGroups::default(),
            false,
        );
        assert_eq!(n, 1, "exactly the junction reverts");
        assert_eq!(
            coords[0],
            Point3::new(CX, -0.3, 0.0),
            "leaper reverted to its chord lift"
        );
        assert_eq!(coords[3], p_orig, "merged partner restored");
        assert_eq!(coords[1], Point3::new(CX, -0.1, 0.0), "neighbor untouched");
        assert_eq!(coords[2], radial(CX, 0.2), "azimuth-true mint untouched");
        assert!(settled.contains(&0), "target blocked from re-merge");
        let snapshot = coords.clone();
        let n2 = settle_rim_chain_order(
            &[ctx_unit_circle()],
            &overlay,
            &mut coords,
            &minted,
            &frame,
            &merges,
            &mut settled,
            &CollapseGroups::default(),
            false,
        );
        assert_eq!(n2, 0, "settled chord is monotone");
        assert_eq!(coords, snapshot, "no further mutation");
    }

    /// The partner itself sits ON the chord and was merged onto an
    /// off-chord target: the settle restores the PARTNER (not a mint
    /// revert) and blocks the target — the re-merge livelock guard.
    #[test]
    fn out_of_order_partner_is_restored_and_target_settled() {
        let overlay = overlay_of(&[(CX, -0.3), (CX, -0.1), (1.2, 0.7)]);
        let p_orig = Point3::new(CX, -0.3, 0.0);
        let target = on_circle(20.0);
        let mut coords = vec![
            target,                     // v0 partner, twinned onto the target
            Point3::new(CX, -0.1, 0.0), // v1 plain chord vertex
            target,                     // v2 the (off-chord) merge target
        ];
        let minted = vec![false, false, true];
        let merges = vec![(0u32, 2u32, p_orig)];
        let mut settled: BTreeSet<u32> = BTreeSet::new();
        let frame = frame_z0();
        let n = settle_rim_chain_order(
            &[ctx_unit_circle()],
            &overlay,
            &mut coords,
            &minted,
            &frame,
            &merges,
            &mut settled,
            &CollapseGroups::default(),
            false,
        );
        assert_eq!(n, 1);
        assert_eq!(coords[0], p_orig, "partner restored to its origin");
        assert_eq!(coords[2], target, "off-chord target itself untouched");
        assert!(settled.contains(&2), "target blocked from re-merge");
    }

    /// A monotone chord — junction kept INSIDE its azimuthal slot beside a
    /// reverted neighbor — is untouched (the discriminating case: R0099's
    /// merges must survive the check).
    #[test]
    fn monotone_chord_is_untouched() {
        let overlay = overlay_of(&[(CX, -0.3), (CX, -0.1), (CX, 0.2)]);
        let mut coords = vec![
            on_circle(-12.0),           // v0 junction kept, azimuth between −30° and v1
            Point3::new(CX, -0.1, 0.0), // v1 reverted (lift)
            radial(CX, 0.2),            // v2 radial mint
        ];
        let minted = vec![true, true, true];
        let mut settled: BTreeSet<u32> = BTreeSet::new();
        let frame = frame_z0();
        let snapshot = coords.clone();
        let n = settle_rim_chain_order(
            &[ctx_unit_circle()],
            &overlay,
            &mut coords,
            &minted,
            &frame,
            &[],
            &mut settled,
            &CollapseGroups::default(),
            false,
        );
        assert_eq!(n, 0);
        assert_eq!(coords, snapshot);
        assert!(settled.is_empty());
    }

    /// Amendment 16 (spec §14): the torn-group configuration. A qualified
    /// sub-floor collapse group (ulp-twin anchors near (CX, −0.1)) sits
    /// minted at a leaped shared position; the settle finds the inversion
    /// against the next chord point and must revert BOTH members to the ONE
    /// shared chord target bit-identically — never one alone.
    #[test]
    fn group_atomic_settle_reverts_whole_group() {
        let anchor_twin = -0.1 + 1.0e-15;
        let overlay = overlay_of(&[(CX, -0.1), (CX, anchor_twin), (CX, 0.2)]);
        let leaped = on_circle(15.0); // past v2's chord azimuth ≈ 13.0°
        let mut coords = vec![leaped, leaped, Point3::new(CX, 0.2, 0.0)];
        let minted = vec![true, true, false];
        let shared = Point3::new(CX, -0.1, 0.0); // elected member v0's lift
        let mut groups = CollapseGroups::default();
        for vi in [0usize, 1] {
            groups.members.insert(vi, vec![0, 1]);
            groups.shared_lift.insert(vi, shared);
        }
        let mut settled: BTreeSet<u32> = BTreeSet::new();
        let frame = frame_z0();
        let n = settle_rim_chain_order(
            &[ctx_unit_circle()],
            &overlay,
            &mut coords,
            &minted,
            &frame,
            &[],
            &mut settled,
            &groups,
            false,
        );
        assert_eq!(n, 2, "both group members revert in one firing");
        assert_eq!(coords[0], shared, "member 0 on the shared chord target");
        assert_eq!(coords[1], shared, "member 1 bit-identical — group intact");
        assert_eq!(coords[2], Point3::new(CX, 0.2, 0.0), "neighbor untouched");
        assert!(settled.contains(&0) && settled.contains(&1));
        let snapshot = coords.clone();
        let n2 = settle_rim_chain_order(
            &[ctx_unit_circle()],
            &overlay,
            &mut coords,
            &minted,
            &frame,
            &[],
            &mut settled,
            &groups,
            false,
        );
        assert_eq!(n2, 0, "fused group is quiescent — no settle×revert fight");
        assert_eq!(coords, snapshot);
    }

    /// Verts with NO registered collapse group (an empty `CollapseGroups`)
    /// keep per-member revert semantics — the displaced member reverts to
    /// its OWN lift while the other mint stays leaped. This is the tear the
    /// group discipline exists to prevent, preserved here as the documented
    /// baseline for unregistered (non-group / wide-anchored) mints.
    #[test]
    fn unregistered_verts_revert_per_member() {
        let anchor_twin = -0.1 + 1.0e-15;
        let overlay = overlay_of(&[(CX, -0.1), (CX, anchor_twin), (CX, 0.2)]);
        let leaped = on_circle(15.0);
        let mut coords = vec![leaped, leaped, Point3::new(CX, 0.2, 0.0)];
        let minted = vec![true, true, false];
        let mut settled: BTreeSet<u32> = BTreeSet::new();
        let frame = frame_z0();
        let n = settle_rim_chain_order(
            &[ctx_unit_circle()],
            &overlay,
            &mut coords,
            &minted,
            &frame,
            &[],
            &mut settled,
            &CollapseGroups::default(),
            false,
        );
        assert_eq!(n, 1, "only the inversion's displaced member reverts");
        assert_eq!(
            coords[1],
            Point3::new(CX, anchor_twin, 0.0),
            "displaced member at its OWN lift"
        );
        assert_eq!(coords[0], leaped, "group partner left leaped — the tear");
    }
}
