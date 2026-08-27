//! Rim-junction override machinery for the `boolean()` driver: the Case-IV
//! phantom rim-segment guard (`cyl_pair_phantom_n`, `phantom_min_rim_segments`),
//! the exact cross-solid rim-junction insertion scan (`rim_junctions_against`),
//! and the rim/planar-face containment predicates it consults (`RimDesc`,
//! `point_in_rim_sweep`, `planar_face_segments`, `point_in_planar_face`,
//! `rim_junction_overrides`). Extracted verbatim from `boolean.rs` (move-only,
//! spec `specs/yang_rs_lib_decomposition.md` F9).

#[allow(clippy::wildcard_imports)]
use crate::*;

/// Case-IV phantom guard analysis (spec `yang_case_iv_phantom_guard`,
/// M8 increment 15): the forced minimum rim segment count over all
/// ANALYTICALLY DISJOINT cylinder-face pairs (A×B) whose Stage-1 chord
/// bands could otherwise overlap the gap between the surfaces (Yang Fig. 8
/// Case IV — the meshes would intersect where the surfaces do not,
/// manufacturing a phantom intersection curve; measured F0088 op 4).
///
/// For each pair: the axis-line distance gives the analytic gap (external
/// `d − r_a − r_b` for any axis pose; nested `r_large − d − r_small` for
/// parallel axes). A positive gap demands the smallest `N` with
/// `sag(r_a, N) + sag(r_b, N) ≤ gap/2` (`sag(r, N) = r(1 − cos(π/N))` —
/// the Stage-1 sagitta, A14.3 single source; the factor-2 margin keeps the
/// combined band strictly clear, and a finer N is always chord-valid). Far
/// pairs derive a tiny N that the natural Stage-1 `max()` absorbs — the
/// guard is self-limiting, no mode branch. True near-tangency (N would
/// exceed 4096) yields no requirement: the loud Stage-3 `AmbiguousCurve`
/// remains the tripwire (P9 — never silently proceed with phantom
/// topology).
/// The Case-IV pairwise requirement of two cylinder surfaces (spec
/// `yang_case_iv_phantom_guard`): `None` unless the pair is analytically
/// disjoint with a practical derived N — the smallest `N` with
/// `sag(r_a, N) + sag(r_b, N) ≤ gap/2` (`sag(r, N) = r(1 − cos(π/N))`, the
/// Stage-1 sagitta; the factor-2 margin keeps the combined chord band
/// strictly clear of the gap, and a finer N is always chord-valid). Shared
/// by the `boolean()` cross-pair guard AND Stage 1's intra-solid fold.
pub(crate) fn cyl_pair_phantom_n(
    (pa, da, ra): (Point3, Vector3, f64),
    (pb, db, rb): (Point3, Vector3, f64),
) -> Option<usize> {
    let ua = normalize3(da.as_array());
    let ub = normalize3(db.as_array());
    let w = [pb.x() - pa.x(), pb.y() - pa.y(), pb.z() - pa.z()];
    let cx = [
        ua[1] * ub[2] - ua[2] * ub[1],
        ua[2] * ub[0] - ua[0] * ub[2],
        ua[0] * ub[1] - ua[1] * ub[0],
    ];
    let cross_norm = (cx[0] * cx[0] + cx[1] * cx[1] + cx[2] * cx[2]).sqrt();
    // Axis-line distance: skew/crossing axes project the offset onto the
    // common normal; parallel axes take the perpendicular point-line
    // distance.
    let (parallel, d_axes) = if cross_norm > 1e-12 {
        let d = (w[0] * cx[0] + w[1] * cx[1] + w[2] * cx[2]).abs() / cross_norm;
        (false, d)
    } else {
        let t = w[0] * ua[0] + w[1] * ua[1] + w[2] * ua[2];
        let perp = [w[0] - t * ua[0], w[1] - t * ua[1], w[2] - t * ua[2]];
        let d = (perp[0] * perp[0] + perp[1] * perp[1] + perp[2] * perp[2]).sqrt();
        (true, d)
    };
    let external = d_axes - (ra + rb);
    let nested = if parallel {
        ra.max(rb) - d_axes - ra.min(rb)
    } else {
        f64::NEG_INFINITY
    };
    let gap = external.max(nested);
    if gap.partial_cmp(&0.0) != Some(std::cmp::Ordering::Greater) {
        return None; // surfaces intersect / NaN (degenerate input): real curve or no-op
    }
    let sag = |r: f64, n: usize| r * (1.0 - (std::f64::consts::PI / n as f64).cos());
    let mut n = 3usize;
    while sag(ra, n) + sag(rb, n) > gap / 2.0 {
        n += 1;
        if n > 4096 {
            // True near-tangency: no finite practical N — leave the loud
            // Stage-3 stop as the tripwire.
            return None;
        }
    }
    Some(n)
}

pub(crate) fn phantom_min_rim_segments(a: &BRep, b: &BRep) -> Option<usize> {
    let cyls = |brep: &BRep| -> Vec<(Point3, Vector3, f64)> {
        brep.faces()
            .iter()
            .filter_map(|f| match f.surface {
                Surface::Cylinder {
                    axis_point,
                    axis_dir,
                    radius,
                } => Some((axis_point, axis_dir, radius)),
                _ => None,
            })
            .collect()
    };
    let (ca, cb) = (cyls(a), cyls(b));
    let mut req: Option<usize> = None;
    // CROSS pairs only (A×B): the two operands' meshes must not intersect
    // where their surfaces do not (the measured F0088 cut-4 class).
    // INTRA-solid pairs are folded into Stage 1's own N selection
    // (`stage1_tessellate_inner` — M8 increment 16), where EVERY
    // tessellation of the solid picks them up (conversion, Stage-0 rebuilds,
    // this guard's rebuilds), so they need no handling here.
    for &sa in &ca {
        for &sb in &cb {
            if let Some(n) = cyl_pair_phantom_n(sa, sb) {
                req = Some(req.map_or(n, |r: usize| r.max(n)));
            }
        }
    }
    // Self-limiting gate: a requirement BOTH solids' natural Stage-1 N
    // already satisfies is dropped, keeping the common path byte-identical
    // (and rebuild-free). `natural_rim_n` mirrors the Stage-1 N derivation
    // (chord bound over all rim circles, N from the max radius).
    let gated = match req {
        Some(n) if n > natural_rim_n(a) || n > natural_rim_n(b) => Some(n),
        _ => None,
    };
    if std::env::var_os("YANG_SPLIT_PROBE").is_some() {
        eprintln!(
            "[phantom-guard] req={req:?} natural=({},{}) gated={gated:?} \
             cyl_faces=({},{})",
            natural_rim_n(a),
            natural_rim_n(b),
            a.faces()
                .iter()
                .filter(|f| matches!(f.surface, Surface::Cylinder { .. }))
                .count(),
            b.faces()
                .iter()
                .filter(|f| matches!(f.surface, Surface::Cylinder { .. }))
                .count(),
        );
    }
    gated
}

/// The solid's natural Stage-1 rim segment count: mirrors the Stage-1 N
/// derivation exactly (chord bound over all rim circles, N from the max
/// radius). `usize::MAX` for a solid with no circles (nothing to boost).
/// Shared by the Case-IV phantom and Case-III graze guards' self-limiting
/// gates.
fn natural_rim_n(brep: &BRep) -> usize {
    let Some(d_eps) = curved_chord_bound(brep.edges()) else {
        return usize::MAX; // no circles: nothing to boost
    };
    let max_r = brep
        .edges()
        .iter()
        .filter_map(|e| match e.curve {
            Curve::Circle { radius, .. } => Some(radius),
            _ => None,
        })
        .fold(0.0f64, f64::max);
    let mut n = 3usize;
    if d_eps > 0.0 {
        while max_r * (1.0 - (std::f64::consts::PI / n as f64).cos()) > d_eps {
            n += 1;
        }
    }
    n
}

/// One cylinder pair's Case-III verdict (spec
/// `yang_172_case_iii_graze_guard` §3).
#[derive(Debug, Clone, Copy, PartialEq)]
pub(crate) enum GrazeDemand {
    /// Not a scoped shallow graze: disjoint / exact tangency (the Case-IV
    /// side), deep intersection handled by the natural N, depth inside the
    /// authored-tangency noise class, or degenerate input.
    None,
    /// Boost both operands' rim N to at least this value so the chord
    /// meshes must sample the intersection.
    Boost(usize),
    /// Genuine intersection below the observability floor of the rim-N
    /// cap: no practical tessellation sees it — the typed-STOP arm.
    SubSagitta { depth: f64, floor: f64 },
}

/// The Case-III pairwise demand of two cylinder lateral surfaces (spec
/// `yang_172_case_iii_graze_guard`): the mirror of [`cyl_pair_phantom_n`]
/// for pairs that ANALYTICALLY INTERSECT at a shallow penetration `depth`
/// (non-parallel axes: `r_a + r_b − d_lines`; parallel axes crossing
/// properly: `min(r_a + r_b − d, d − |r_a − r_b|)` — the second term is
/// the internal graze). A positive depth demands the smallest `N` with
/// `sag(r_a, N) + sag(r_b, N) ≤ depth/2`, guaranteeing mesh-level
/// penetration ≥ depth/2 regardless of chord phase (inscribed chords
/// recede at most `sag` inward; the factor-2 margin is safety, not a
/// tolerance — a finer N is always chord-valid, A14.3). Deep
/// intersections derive a tiny N absorbed by the caller's natural-N gate.
/// A depth at or below the #178-calibrated coincidence-noise line
/// (`max(TAU_MODEL, scale·TAU_WORK)/100`) is authored tangency residue —
/// no demand. A depth above the noise line whose N would exceed the 4096
/// cap is a genuine sub-resolution intersection: [`GrazeDemand::SubSagitta`]
/// (the caller STOPs loudly after the face-extent witness check).
pub(crate) fn cyl_pair_graze_demand(
    (pa, da, ra): (Point3, Vector3, f64),
    (pb, db, rb): (Point3, Vector3, f64),
) -> GrazeDemand {
    let ua = normalize3(da.as_array());
    let ub = normalize3(db.as_array());
    let w = [pb.x() - pa.x(), pb.y() - pa.y(), pb.z() - pa.z()];
    let cx = [
        ua[1] * ub[2] - ua[2] * ub[1],
        ua[2] * ub[0] - ua[0] * ub[2],
        ua[0] * ub[1] - ua[1] * ub[0],
    ];
    let cross_norm = (cx[0] * cx[0] + cx[1] * cx[1] + cx[2] * cx[2]).sqrt();
    let (parallel, d_axes) = if cross_norm > 1e-12 {
        let d = (w[0] * cx[0] + w[1] * cx[1] + w[2] * cx[2]).abs() / cross_norm;
        (false, d)
    } else {
        let t = w[0] * ua[0] + w[1] * ua[1] + w[2] * ua[2];
        let perp = [w[0] - t * ua[0], w[1] - t * ua[1], w[2] - t * ua[2]];
        let d = (perp[0] * perp[0] + perp[1] * perp[1] + perp[2] * perp[2]).sqrt();
        (true, d)
    };
    let depth = if parallel {
        // Proper crossing of parallel laterals: |r_a − r_b| < d < r_a + r_b.
        // Both margins are graze hazards (external lens / internal crescent).
        let external = (ra + rb) - d_axes;
        let internal = d_axes - (ra - rb).abs();
        external.min(internal)
    } else {
        (ra + rb) - d_axes
    };
    if depth.partial_cmp(&0.0) != Some(std::cmp::Ordering::Greater) {
        return GrazeDemand::None; // disjoint / exact tangency / NaN
    }
    // #178-calibrated coincidence-authoring noise line: an authored-tangent
    // pair arrives with sub-noise residue (measured population ≤ 2.235e-10);
    // designed sub-resolution features sit ≥ 1e-8, 10–100× above the line.
    let mut scale = ra.max(rb);
    for &c in pa.as_array().iter().chain(pb.as_array().iter()) {
        scale = scale.max(c.abs());
    }
    let noise = cad_primitives::TAU_MODEL.max(scale * cad_primitives::TAU_WORK) / 100.0;
    if depth <= noise {
        return GrazeDemand::None;
    }
    let sag = |r: f64, n: usize| r * (1.0 - (std::f64::consts::PI / n as f64).cos());
    let mut n = 3usize;
    while sag(ra, n) + sag(rb, n) > depth / 2.0 {
        n += 1;
        if n > 4096 {
            return GrazeDemand::SubSagitta {
                depth,
                floor: 2.0 * (sag(ra, 4096) + sag(rb, 4096)),
            };
        }
    }
    // Render-observability scope line (spec §3): the boost exists so the
    // OBSERVABLE output is right. A lens shallower than the render mesh's
    // own combined sagitta (kernel-v2 render chord ratio 1e-3·r per face,
    // #173 calibration) cannot be represented at ANY output resolution —
    // the render selfx gate provably cannot see it, and the measured
    // corpus-green status quo (C0057: unfused 1e-6 lens, shell-credited
    // oracles) stays byte-identical. That sub-render band is §4.5.2 LOCAL
    // refinement territory (roadmap P3d), not a global-rim-N job — the
    // derived N would be unbounded (C0057: 3142, measured CORRECT→TIMEOUT
    // corpus regression). Above the line the derived N is bounded ≈ 71
    // regardless of radii (scale-free ratio), so the boost is always
    // affordable. Depths below the 4096-cap floor still STOP above.
    if depth <= 2.0e-3 * (ra + rb) {
        return GrazeDemand::None;
    }
    GrazeDemand::Boost(n)
}

/// Case-III graze guard scan (spec `yang_172_case_iii_graze_guard`): the
/// forced minimum rim segment count over all cross A×B cylinder-face
/// pairs that intersect at a shallow analytic penetration (Yang Fig. 8
/// Case III — the meshes would MISS an intersection the surfaces have,
/// emitting unfused topology whose true trimmed surfaces interpenetrate;
/// measured C0116). `Err(SubSagittaGrazeIntersection)` when a pair's
/// depth is above authoring noise yet below the rim-N cap's
/// observability floor AND the graze region reaches both faces' axial
/// extents (the witness check — an off-face infinite-surface graze must
/// not false-STOP the adjacent-boss class). Self-limiting like the
/// Case-IV guard: a demand both solids' natural Stage-1 N satisfies is
/// dropped, keeping the common path byte-identical.
pub(crate) fn graze_min_rim_segments(a: &BRep, b: &BRep) -> Result<Option<usize>, YangError> {
    let cyls = |brep: &BRep| -> Vec<(usize, Point3, Vector3, f64)> {
        brep.faces()
            .iter()
            .enumerate()
            .filter_map(|(i, f)| match f.surface {
                Surface::Cylinder {
                    axis_point,
                    axis_dir,
                    radius,
                } => Some((i, axis_point, axis_dir, radius)),
                _ => None,
            })
            .collect()
    };
    // The face's span of rim-circle centers along `û` measured from `p0`.
    // `None` when the face carries no Circle edges (no derivable span —
    // the caller treats it as spanning, conservative-loud).
    let axial_span =
        |brep: &BRep, face_idx: usize, p0: Point3, u: [f64; 3]| -> Option<(f64, f64)> {
            let face = &brep.faces()[face_idx];
            let mut span: Option<(f64, f64)> = None;
            for &e_idx in face
                .outer_loop
                .iter()
                .chain(face.inner_loops.iter().flatten())
            {
                if let Curve::Circle { center, .. } = brep.edges()[e_idx as usize].curve {
                    let t = (center.x() - p0.x()) * u[0]
                        + (center.y() - p0.y()) * u[1]
                        + (center.z() - p0.z()) * u[2];
                    span = Some(span.map_or((t, t), |(lo, hi)| (lo.min(t), hi.max(t))));
                }
            }
            span
        };
    let (ca, cb) = (cyls(a), cyls(b));
    let mut boosts: Vec<(usize, usize, usize)> = Vec::new(); // (fa, fb, n)
    for &(fa, pa, da, ra) in &ca {
        for &(fb, pb, db, rb) in &cb {
            match cyl_pair_graze_demand((pa, da, ra), (pb, db, rb)) {
                GrazeDemand::None => {}
                GrazeDemand::Boost(n) => {
                    boosts.push((fa, fb, n));
                }
                GrazeDemand::SubSagitta { depth, floor } => {
                    // Witness check: the graze region (the common
                    // perpendicular of the two axes, widened by the wedge
                    // half-length √(2·r_max·depth)) must reach BOTH faces'
                    // axial spans; the infinite surfaces grazing off-face
                    // is not this pair's defect.
                    let ua = normalize3(da.as_array());
                    let ub = normalize3(db.as_array());
                    let w = [pb.x() - pa.x(), pb.y() - pa.y(), pb.z() - pa.z()];
                    let bdot = ua[0] * ub[0] + ua[1] * ub[1] + ua[2] * ub[2];
                    let d1 = w[0] * ua[0] + w[1] * ua[1] + w[2] * ua[2];
                    let d2 = w[0] * ub[0] + w[1] * ub[1] + w[2] * ub[2];
                    let denom = 1.0 - bdot * bdot;
                    let half_len = (2.0 * ra.max(rb) * depth).sqrt();
                    let hit = |span: Option<(f64, f64)>, t: f64| -> bool {
                        span.is_none_or(|(lo, hi)| t >= lo - half_len && t <= hi + half_len)
                    };
                    let in_extent = if denom > 1e-24 {
                        // Skew/crossing axes: perpendicular feet params.
                        let s_a = (d1 - bdot * d2) / denom;
                        let t_b = (bdot * d1 - d2) / denom;
                        hit(axial_span(a, fa, pa, ua), s_a) && hit(axial_span(b, fb, pb, ub), t_b)
                    } else {
                        // Parallel axes: the graze runs along the common
                        // axial overlap — the two spans (both measured
                        // along û_a from p_a) must overlap.
                        match (axial_span(a, fa, pa, ua), axial_span(b, fb, pb, ua)) {
                            (Some((alo, ahi)), Some((blo, bhi))) => {
                                let boff = d1; // p_b's offset along û_a
                                alo - half_len <= boff + bhi + half_len
                                    && blo + boff - half_len <= ahi + half_len
                            }
                            _ => true, // no derivable span: conservative-loud
                        }
                    };
                    if in_extent {
                        return Err(YangError::SubSagittaGrazeIntersection {
                            face_a: fa,
                            face_b: fb,
                            depth,
                            floor,
                        });
                    }
                }
            }
        }
    }
    // Phase-aware Case-III filter (spec §3): the paper defines Case III as
    // "the meshes MISS intersections" — a pair whose NATURAL meshes already
    // intersect (the seam-anchored chord phase happens to catch the lens,
    // e.g. the C0057 vertex-aligned 1e-6 sliver) is NOT a Case III miss and
    // keeps today's byte-identical path (its output is guarded by the same
    // metric/render oracles as before). Only pairs whose natural face
    // meshes are exactly disjoint (Cherchi tri-tri classifier, exact
    // predicates) demand the boost. Both failure directions are safe: a
    // spurious "disjoint" only costs a finer mesh; a spurious "intersects"
    // is the measured pre-guard baseline.
    let mut req: Option<usize> = None;
    if !boosts.is_empty() {
        let verts_a: Vec<BRepVertex> = a.vertices().to_vec();
        let verts_b: Vec<BRepVertex> = b.vertices().to_vec();
        let ta = stage1_tessellate(&verts_a, a.edges(), a.faces())?;
        let tb = stage1_tessellate(&verts_b, b.edges(), b.faces())?;
        let tri_aabb = |t: &crate::stage1_tessellate::Stage1Tess, tri: [u32; 3]| {
            let mut lo = [f64::INFINITY; 3];
            let mut hi = [f64::NEG_INFINITY; 3];
            for &vi in &tri {
                let p = t.verts[vi as usize].as_array();
                for k in 0..3 {
                    lo[k] = lo[k].min(p[k]);
                    hi[k] = hi[k].max(p[k]);
                }
            }
            (lo, hi)
        };
        // Any-contact test between one operand's tri range and the WHOLE
        // partner mesh. The flagged pair's mesh-level contact need not be
        // lateral×lateral — with parallel axes the lateral tris are all
        // axis-parallel and can never cross each other; the C0057 sliver
        // enters through the partner's CAP disc — so a flagged face is
        // tested against every partner triangle.
        let touches = |xa: &crate::stage1_tessellate::Stage1Tess,
                       range: std::ops::Range<usize>,
                       xb: &crate::stage1_tessellate::Stage1Tess|
         -> bool {
            use cherchi_rs::predicates::TriangleIntersection as TI;
            for ia in range {
                let tri_a = xa.tris[ia];
                let (alo, ahi) = tri_aabb(xa, tri_a);
                for &tri_b in &xb.tris {
                    let (blo, bhi) = tri_aabb(xb, tri_b);
                    if (0..3).any(|k| alo[k] > bhi[k] || blo[k] > ahi[k]) {
                        continue;
                    }
                    match cherchi_rs::predicates::triangle_intersects_triangle_3d(
                        xa.verts[tri_a[0] as usize],
                        xa.verts[tri_a[1] as usize],
                        xa.verts[tri_a[2] as usize],
                        xb.verts[tri_b[0] as usize],
                        xb.verts[tri_b[1] as usize],
                        xb.verts[tri_b[2] as usize],
                    ) {
                        TI::Intersects | TI::Coplanar => return true,
                        TI::Disjoint => {}
                    }
                }
            }
            false
        };
        for (fa, fb, n) in boosts {
            let meshes_touch = touches(&ta, ta.face_tri_ranges[fa].clone(), &tb)
                || touches(&tb, tb.face_tri_ranges[fb].clone(), &ta);
            if std::env::var_os("YANG_SPLIT_PROBE").is_some() {
                eprintln!(
                    "[graze-guard] pair=({fa},{fb}) n={n} meshes_touch={meshes_touch} \
                     tris=({},{})",
                    ta.face_tri_ranges[fa].len(),
                    tb.face_tri_ranges[fb].len(),
                );
            }
            if !meshes_touch {
                req = Some(req.map_or(n, |r: usize| r.max(n)));
            }
        }
    }
    Ok(match req {
        Some(n) if n > natural_rim_n(a) || n > natural_rim_n(b) => Some(n),
        _ => None,
    })
}

/// One rim×plane pair's shallow-crossing demand (#195 inc-2, spec
/// `yang_195_seal_neighborhood_self_overlap` §5): a Circle rim edge
/// (center `c`, unit normal `n`, radius `r`) of one operand crossing a
/// partner PLANE face (`m̂·p + d̂ = 0`) at a shallow extent. The circle's
/// signed distances to the plane span `s ± r·k` (`s = m̂·c + d̂`,
/// `k = √(1−(n·m̂)²)`); it crosses iff `|s| < r·k`, and the shallow-side
/// extent is `depth = r·k − |s|`. The rim's chords recede radially inward
/// by at most `sag(r,N)`, so a crossing with `depth ≤ sag` can be MISSED
/// by the Stage-1 mesh — the arrangement then mints no curve, labeling
/// keeps the whole submerged region, and Stage-4 relocation mints the true
/// junction BEYOND the plane, emitting a self-intersecting B-Rep (the
/// measured F0082 producing-union defect; Yang §4.5.4 class, remedied by
/// refinement exactly as the paper prescribes). Demand the smallest `N`
/// with `sag(r, N) ≤ depth/2` — single-sided recession (the plane's mesh
/// is exact), same factor-2 margin as Case-IV/III (measured: F0082 floor
/// 32 = sag<depth but no margin → silent WRONG; derived 41 → CORRECT).
///
/// Scope lines (spec §5c): depth at or below the #178-calibrated noise
/// line (authored flush-assembly rims) or below the render-observability
/// line `2·10⁻³·r` (sub-render lens, §4.5.2 local-refinement territory)
/// demands nothing; N > 4096 demands nothing for inc-2 (NO SubSagitta
/// STOP arm — the emitted class detonates loudly at the next boolean's
/// (4b) gate; a producer-side STOP is a named follow-up needing the
/// plane-face extent witness). No phase-aware mesh-touch filter: the
/// partner plane face may legitimately intersect the rim's operand
/// ELSEWHERE (F0082's wall is crossed by the tube lateral) while the rim
/// crossing is still missed — a face-global touch test would veto the
/// needed boost, and the render line already bounds N ≈ 71.
pub(crate) fn rim_plane_graze_n(
    (c, n, r): ([f64; 3], [f64; 3], f64),
    (m, d): ([f64; 3], f64),
) -> Option<usize> {
    let dot = |a: [f64; 3], b: [f64; 3]| a[0] * b[0] + a[1] * b[1] + a[2] * b[2];
    let mlen = dot(m, m).sqrt();
    if mlen.partial_cmp(&0.0) != Some(std::cmp::Ordering::Greater) {
        return None; // degenerate plane / NaN
    }
    let mh = [m[0] / mlen, m[1] / mlen, m[2] / mlen];
    let dh = d / mlen;
    let ndm = dot(n, mh);
    let k = (1.0 - ndm * ndm).max(0.0).sqrt();
    let s = dot(mh, c) + dh;
    let depth = r * k - s.abs();
    if depth.partial_cmp(&0.0) != Some(std::cmp::Ordering::Greater) {
        return None; // no crossing / rim in-plane / NaN
    }
    // #178-calibrated coincidence-authoring noise line (same form as
    // `cyl_pair_graze_demand`): flush-assembly rims arrive with sub-noise
    // crossing residue and must not boost.
    let mut scale = r;
    for &v in c.iter() {
        scale = scale.max(v.abs());
    }
    scale = scale.max(dh.abs());
    let noise = cad_primitives::TAU_MODEL.max(scale * cad_primitives::TAU_WORK) / 100.0;
    if depth <= noise {
        return None;
    }
    // Render-observability line, single-radius form of #172 §3: a lens
    // shallower than the render mesh's own rim sagitta cannot be
    // represented at any output resolution; bounds the derived N ≈ 71.
    if depth <= 2.0e-3 * r {
        return None;
    }
    let sag = |n_seg: usize| r * (1.0 - (std::f64::consts::PI / n_seg as f64).cos());
    let mut n_seg = 3usize;
    while sag(n_seg) > depth / 2.0 {
        n_seg += 1;
        if n_seg > 4096 {
            return None; // inc-2: no STOP arm (spec §5c)
        }
    }
    Some(n_seg)
}

/// #195 inc-2 scan: the forced minimum rim segment count over all
/// cross-operand rim-circle × plane-face pairs that shallowly cross
/// (spec `yang_195_seal_neighborhood_self_overlap` §5). Self-limiting
/// like the Case-IV/III guards: a demand both solids' natural Stage-1 N
/// satisfies is dropped by the caller-shared gate here, keeping the
/// common path byte-identical.
pub(crate) fn rim_plane_graze_min_segments(a: &BRep, b: &BRep) -> Option<usize> {
    let rims = |brep: &BRep| -> Vec<([f64; 3], [f64; 3], f64)> {
        brep.edges()
            .iter()
            .filter_map(|e| match e.curve {
                Curve::Circle {
                    center,
                    normal,
                    radius,
                } => Some((center.as_array(), normalize3(normal.as_array()), radius)),
                _ => None,
            })
            .collect()
    };
    let planes = |brep: &BRep| -> Vec<([f64; 3], f64)> {
        brep.faces()
            .iter()
            .filter_map(|f| match f.surface {
                Surface::Plane { normal, d } => Some((normal.as_array(), d)),
                _ => None,
            })
            .collect()
    };
    let mut req: Option<usize> = None;
    for (x, y) in [(a, b), (b, a)] {
        for &rim in &rims(x) {
            for &pl in &planes(y) {
                if let Some(n) = rim_plane_graze_n(rim, pl) {
                    req = Some(req.map_or(n, |r: usize| r.max(n)));
                }
            }
        }
    }
    let gated = match req {
        Some(n) if n > natural_rim_n(a) || n > natural_rim_n(b) => Some(n),
        _ => None,
    };
    if std::env::var_os("YANG_SPLIT_PROBE").is_some() {
        eprintln!(
            "[rim-plane-graze] req={req:?} natural=({},{}) gated={gated:?}",
            natural_rim_n(a),
            natural_rim_n(b),
        );
    }
    gated
}

/// SIGNED perpendicular distance from a point to the guard's curved target
/// surfaces (exact closed forms; `None` = out-of-scope surface kind).
/// Negative = radially INSIDE the flank (toward the axis) — the side the
/// inscribed Stage-1 chords dip toward, the only side a mesh sag can
/// phantom-cross from.
fn point_surface_signed(p: [f64; 3], s: Surface) -> Option<f64> {
    let sub = |a: [f64; 3], b: [f64; 3]| [a[0] - b[0], a[1] - b[1], a[2] - b[2]];
    let dot = |a: [f64; 3], b: [f64; 3]| a[0] * b[0] + a[1] * b[1] + a[2] * b[2];
    match s {
        Surface::Sphere { center, radius } => {
            let w = sub(p, center.as_array());
            Some(dot(w, w).sqrt() - radius)
        }
        Surface::Cylinder {
            axis_point,
            axis_dir,
            radius,
        } => {
            let u = normalize3(axis_dir.as_array());
            let w = sub(p, axis_point.as_array());
            let h = dot(w, u);
            let radial = (dot(w, w) - h * h).max(0.0).sqrt();
            Some(radial - radius)
        }
        Surface::Cone {
            apex,
            axis_dir,
            half_angle,
        } => {
            let u = normalize3(axis_dir.as_array());
            let w = sub(p, apex.as_array());
            let h = dot(w, u);
            let radial = (dot(w, w) - h * h).max(0.0).sqrt();
            // Perpendicular distance to the infinite (canonical-nappe) cone
            // flank — exact for a right cone.
            Some((radial - h * half_angle.tan()) * half_angle.cos())
        }
        _ => None,
    }
}

/// §4.3.3 Case-IV CORNER-phantom guard (spec
/// `specs/yang_433_case_iv_corner_phantom.md`, inc-1 — GATED
/// `YANG_433_GUARD=1|on`): the derived rim-N requirement over every
/// (LineSegment B-Rep edge of one operand) × (curved face of the other)
/// pair whose exact line×surface roots BOTH fall outside the edge's own
/// segment — the edge passes the surface without piercing it, so any
/// mesh-level crossing there is a phantom (Yang Fig. 8 Case IV; the paper's
/// §4.3.3 "no solution in the parametric domains" rule-out, realized at
/// Stage 1 like the sibling guards).
///
/// Measured anchor (R0100 face 15): a prism cap-corner wedge clears the
/// 11.77° cone by 1.33 while the natural N=24 mesh sags 2.29 — the mesh
/// clips the wedge and mints a 3-vertex loop that everts under relocation.
/// The nearest wedge ELEMENT to the surface is derived per edge: the
/// segment↔surface clearance `g` (a certified lower bound: sampled min
/// minus the Lipschitz slack `len/(2(S−1))`, the distance being
/// 1-Lipschitz along the segment). The demand is the smallest `N` with
/// `sag(r_max, N) ≤ g/2` — the factor-2 phase margin, same argument as the
/// sibling guards (A14.3: a finer N is always chord-valid). Measured green
/// floor on the vehicle is N=30; the derivation yields N≈39 from the
/// corner-edge clearance 2.12.
///
/// Fail-closed edges of scope: a segment whose lower-bound clearance is not
/// strictly positive (touching / authoring noise) derives nothing — the
/// loud downstream STOP remains its tripwire; a demand beyond 4096 is
/// dropped the same way (true near-tangency); Torus targets and curved
/// B-Rep edges are out of scope this increment.
pub(crate) fn edge_graze_min_rim_segments(a: &BRep, b: &BRep) -> Option<usize> {
    if !matches!(
        std::env::var("YANG_433_GUARD").as_deref(),
        Ok("1") | Ok("on")
    ) {
        return None;
    }
    let req = edge_graze_requirement(a, b);
    let gated = match req {
        Some(n) if n > natural_rim_n(a) || n > natural_rim_n(b) => Some(n),
        _ => None,
    };
    if std::env::var_os("YANG_SPLIT_PROBE").is_some() {
        eprintln!(
            "[edge-graze-guard] req={req:?} natural=({},{}) gated={gated:?}",
            natural_rim_n(a),
            natural_rim_n(b),
        );
    }
    gated
}

/// The face's axial station band, derived from its own rim circles (their
/// centers projected on the surface axis) — the face's mesh lives strictly
/// within this sweep (rim vertices lie ON the rims), so clearance against
/// the surface's infinite extension beyond it can never mint a phantom on
/// THIS face. `None` for a face with fewer than one rim circle or a
/// non-axial surface — the guard then skips the face (fail closed: no
/// demand, the downstream STOP remains).
fn face_station_band(f: &BRepFace, brep: &BRep) -> Option<([f64; 3], [f64; 3], f64, f64)> {
    let (origin, axis) = match f.surface {
        Surface::Cone { apex, axis_dir, .. } => (apex.as_array(), axis_dir.as_array()),
        Surface::Cylinder {
            axis_point,
            axis_dir,
            ..
        } => (axis_point.as_array(), axis_dir.as_array()),
        _ => return None,
    };
    let u = normalize3(axis);
    let mut lo = f64::INFINITY;
    let mut hi = f64::NEG_INFINITY;
    let mut any = false;
    for &ei in f.outer_loop.iter().chain(f.inner_loops.iter().flatten()) {
        if let Curve::Circle { center, .. } = brep.edges()[ei as usize].curve {
            let c = center.as_array();
            let h =
                (c[0] - origin[0]) * u[0] + (c[1] - origin[1]) * u[1] + (c[2] - origin[2]) * u[2];
            lo = lo.min(h);
            hi = hi.max(h);
            any = true;
        }
    }
    if !any {
        return None;
    }
    Some((origin, u, lo, hi))
}

/// The un-gated requirement (unit-testable): max derived N over both
/// directions' qualifying (CORNER cluster, curved face) pairs, before the
/// natural-N self-limiting gate.
///
/// **Corner-cluster scope (measured 2026-08-27, spec §7):** the broad
/// per-segment form was corpus-measured and REJECTED — it boosted 52
/// cases on real single-edge near-grazes (demands to N=1449), regressing
/// eight CORRECT cases (R0017 broke under a mere N=33 boost) and turning
/// R0011's loud ERROR silently WRONG. The minting configuration is a
/// WEDGE: a B-Rep vertex of one operand BURIED under the other's curved
/// face (radially inside the flank, within the face's own station band)
/// with at least TWO incident LineSegment edges that each inside-graze
/// that face without piercing it in-band. Single-edge grazes derive
/// nothing and keep their loud downstream tripwires.
pub(crate) fn edge_graze_requirement(a: &BRep, b: &BRep) -> Option<usize> {
    let mut req: Option<usize> = None;
    for (x, y) in [(a, b), (b, a)] {
        // Per-vertex incident LineSegment edges of the edge-side operand.
        let mut incident: std::collections::BTreeMap<u32, Vec<usize>> = Default::default();
        for (ei, e) in x.edges().iter().enumerate() {
            if e.curve == Curve::LineSegment && e.start != e.end {
                incident.entry(e.start).or_default().push(ei);
                incident.entry(e.end).or_default().push(ei);
            }
        }
        for f in y.faces() {
            if !matches!(f.surface, Surface::Cylinder { .. } | Surface::Cone { .. }) {
                continue; // Sphere/Torus: no census customers, out of scope
            }
            // Banded by the face's own rim stations; a face the band cannot
            // be derived for is skipped (fail closed).
            let Some(band) = face_station_band(f, y) else {
                continue;
            };
            // The face's own max rim radius drives its sagitta; a face
            // bounded by no Circle edge falls back to the operand's global
            // max (the same radius Stage 1's N derivation uses).
            let face_r = f
                .outer_loop
                .iter()
                .chain(f.inner_loops.iter().flatten())
                .filter_map(|&ei| match y.edges()[ei as usize].curve {
                    Curve::Circle { radius, .. } => Some(radius),
                    _ => None,
                })
                .fold(0.0f64, f64::max);
            let r_max = if face_r > 0.0 {
                face_r
            } else {
                y.edges()
                    .iter()
                    .filter_map(|e| match e.curve {
                        Curve::Circle { radius, .. } => Some(radius),
                        _ => None,
                    })
                    .fold(0.0f64, f64::max)
            };
            if r_max <= 0.0 || r_max.is_nan() {
                continue; // nothing the rim-N vocabulary can boost
            }
            for (&v, edges) in &incident {
                if edges.len() < 2 {
                    continue;
                }
                let p = x.vertices()[v as usize].point.as_array();
                // The wedge signature: the corner vertex is buried under
                // the flank (radially inside) within the face's band.
                let (origin, u, v_lo, v_hi) = band;
                let d_v = match point_surface_signed(p, f.surface) {
                    Some(d) if d < 0.0 => -d,
                    _ => continue,
                };
                let h = (p[0] - origin[0]) * u[0]
                    + (p[1] - origin[1]) * u[1]
                    + (p[2] - origin[2]) * u[2];
                if h < v_lo - d_v || h > v_hi + d_v {
                    continue;
                }
                // Every incident edge's verdict; the cluster fires only
                // with ≥2 grazing (non-piercing, inside, in-band) edges.
                let mut demands: Vec<usize> = Vec::new();
                for &ei in edges {
                    let e = &x.edges()[ei];
                    let p0 = x.vertices()[e.start as usize].point.as_array();
                    let p1 = x.vertices()[e.end as usize].point.as_array();
                    if let Some(n) = segment_face_graze_n(p0, p1, f.surface, r_max, band) {
                        demands.push(n);
                    }
                }
                if demands.len() < 2 {
                    continue;
                }
                let n = demands.into_iter().max().unwrap();
                req = Some(req.map_or(n, |r: usize| r.max(n)));
            }
        }
    }
    req
}

/// One (segment, curved surface) pair's demand: `None` when the segment
/// genuinely pierces the FACE (a root inside `[0,1]` whose station lies in
/// the face's band — the Case-III direction, out of scope), when no sample
/// approaches the face's own station band (the graze is against the
/// surface's infinite extension where this face has no mesh), when the
/// clearance lower bound is not strictly positive (touching — the
/// downstream STOP owns tangency), or when the demand exceeds 4096 (true
/// near-tangency, same cap as the cyl guard).
///
/// `band` is `(axis_origin, axis_unit, v_lo, v_hi)` from
/// [`face_station_band`]. A sample point at distance `d` from the surface
/// counts only when its own axial station is within `[v_lo − d, v_hi + d]`
/// — the perpendicular foot lies within distance `d` of the point, so this
/// superset test is conservative in the fail-closed direction and needs no
/// tuned margin.
pub(crate) fn segment_face_graze_n(
    p0: [f64; 3],
    p1: [f64; 3],
    surface: Surface,
    r_max: f64,
    band: ([f64; 3], [f64; 3], f64, f64),
) -> Option<usize> {
    let (origin, u, v_lo, v_hi) = band;
    let station = |p: [f64; 3]| {
        (p[0] - origin[0]) * u[0] + (p[1] - origin[1]) * u[1] + (p[2] - origin[2]) * u[2]
    };
    let seg = [p1[0] - p0[0], p1[1] - p0[1], p1[2] - p0[2]];
    let at = |t: f64| [p0[0] + t * seg[0], p0[1] + t * seg[1], p0[2] + t * seg[2]];
    let roots = crate::stage4_phantom::segment_surface_roots(p0, p1, surface)?;
    if roots.iter().any(|&t| {
        (0.0..=1.0).contains(&t) && {
            let h = station(at(t));
            (v_lo..=v_hi).contains(&h)
        }
    }) {
        return None; // real pierce of THIS face
    }
    let len = (seg[0] * seg[0] + seg[1] * seg[1] + seg[2] * seg[2]).sqrt();
    if len <= 0.0 || len.is_nan() {
        return None;
    }
    const S: usize = 65;
    let mut min_d = f64::INFINITY;
    for i in 0..S {
        let t = i as f64 / (S - 1) as f64;
        let p = at(t);
        let signed = point_surface_signed(p, surface)?;
        if signed >= 0.0 {
            // Radially OUTSIDE the flank: inscribed chords recede from
            // here — a mesh sag cannot phantom-cross from this side.
            continue;
        }
        let d = -signed;
        let h = station(p);
        if h < v_lo - d || h > v_hi + d {
            continue; // approaches only the face's infinite extension
        }
        min_d = min_d.min(d);
    }
    if !min_d.is_finite() {
        return None; // never inside-near the face's own band
    }
    // Certified lower bound: the distance is 1-Lipschitz in space, so along
    // the segment |d'| ≤ len; the sampled min overestimates the true min by
    // at most len/(2(S−1)).
    let g = min_d - len / (2.0 * (S - 1) as f64);
    if g <= 0.0 || g.is_nan() {
        return None; // touching / authoring noise — fail closed, stay loud
    }
    let bound = g / 2.0;
    let mut n = 3usize;
    while r_max * (1.0 - (std::f64::consts::PI / n as f64).cos()) > bound {
        n += 1;
        if n > 4096 {
            return None; // near-tangency: no practical demand, stay loud
        }
    }
    Some(n)
}

/// N2/F0059 epic increment 2, BANKED-UNWIRED (spec
/// `yang_rim_junction_insertion`): per full-circle rim edge of `x`, the
/// exact points where that rim circle transversally CROSSES one of `y`'s
/// cylinder laterals — the §4.3.3 Case-IV junction points that Stage-1
/// must carry as rim samples so the mesh-level seam chains can terminate
/// exactly at the junctions (the truncated-Steinmetz cap-lobe corners).
///
/// v1 closed-form scope (A13.3/P8 — no ad-hoc root finding): only
/// laterals whose axis is PARALLEL to the rim plane contribute (their
/// section in the rim plane is two lines ⇒ circle∩line quadratics; the
/// F0059 class). Transversal-axis laterals (ellipse section, quartic) and
/// non-cylinder surfaces are out of scope and keep today's loud walls.
/// Tangent grazes are excluded by a DERIVED resolution gate: a root pair
/// closer than `TAU_MODEL` along the section line is one model point
/// (A14.2), i.e. the §4.3.3 tangency class — not a transversal crossing.
///
/// Returned points satisfy `|‖p−c‖−r| ≤ TAU_WORK` and lie on the
/// contributing lateral to fp accuracy (unit-asserted). Deterministic:
/// faces in index order, both section lines, roots in ascending-t order.
pub(crate) fn rim_junctions_against(
    x: &BRep,
    y: &BRep,
) -> std::collections::BTreeMap<u32, Vec<Point3>> {
    let dot = |a: [f64; 3], b: [f64; 3]| a[0] * b[0] + a[1] * b[1] + a[2] * b[2];
    let sub = |a: [f64; 3], b: [f64; 3]| [a[0] - b[0], a[1] - b[1], a[2] - b[2]];
    let add = |a: [f64; 3], b: [f64; 3]| [a[0] + b[0], a[1] + b[1], a[2] + b[2]];
    let scl = |a: [f64; 3], s: f64| [a[0] * s, a[1] * s, a[2] * s];
    let crs = |a: [f64; 3], b: [f64; 3]| {
        [
            a[1] * b[2] - a[2] * b[1],
            a[2] * b[0] - a[0] * b[2],
            a[0] * b[1] - a[1] * b[0],
        ]
    };
    // The lateral's axial extent, from the Circle edges its loops carry
    // (both rims project onto the axis; a lateral without circle loop
    // edges yields None → skipped, loud walls preserved).
    let lateral_extent = |brep: &BRep, f: &BRepFace, ap: [f64; 3], d: [f64; 3]| {
        let mut lo = f64::INFINITY;
        let mut hi = f64::NEG_INFINITY;
        for &ei in f.outer_loop.iter().chain(f.inner_loops.iter().flatten()) {
            if let Curve::Circle { center, .. } = brep.edges()[ei as usize].curve {
                let z = dot(sub(center.as_array(), ap), d);
                lo = lo.min(z);
                hi = hi.max(z);
            }
        }
        (lo < hi).then_some((lo, hi))
    };

    let probe = std::env::var_os("YANG_RIM_JUNCTION_PROBE").is_some();
    if probe {
        let full_rims = x
            .edges()
            .iter()
            .filter(|e| e.start == e.end && matches!(e.curve, Curve::Circle { .. }))
            .count();
        let mut kinds: std::collections::BTreeMap<&str, usize> = std::collections::BTreeMap::new();
        for f in y.faces() {
            let k = match f.surface {
                Surface::Plane { .. } => "plane",
                Surface::Cylinder { .. } => "cyl",
                Surface::Cone { .. } => "cone",
                Surface::Sphere { .. } => "sphere",
                Surface::Torus { .. } => "torus",
            };
            *kinds.entry(k).or_default() += 1;
        }
        eprintln!(
            "[rim-junction] x: edges={} full_circle_rims={full_rims}; y faces: {kinds:?}",
            x.edges().len()
        );
        let mut ekinds: std::collections::BTreeMap<String, usize> =
            std::collections::BTreeMap::new();
        for e in x.edges() {
            let k = match e.curve {
                Curve::Circle { .. } => {
                    if e.start == e.end {
                        "circle-closed".to_string()
                    } else {
                        "circle-arc".to_string()
                    }
                }
                Curve::LineSegment => "line".to_string(),
                ref other => format!("{other:?}")
                    .split([' ', '{'])
                    .next()
                    .unwrap_or("?")
                    .to_string(),
            };
            *ekinds.entry(k).or_default() += 1;
        }
        eprintln!("[rim-junction] x edge kinds: {ekinds:?}");
    }
    let mut out: std::collections::BTreeMap<u32, Vec<Point3>> = std::collections::BTreeMap::new();
    // Rim geometry retained for the §4b coaxial propagation post-pass.
    let mut rims: Vec<RimDesc> = Vec::new();
    for (ei, e) in x.edges().iter().enumerate() {
        let Curve::Circle {
            center,
            normal,
            radius: r,
        } = e.curve
        else {
            continue;
        };
        let n = normalize3(normal.as_array());
        let c = center.as_array();
        // Increment 4 (measured scope correction): partial-revolve rims are
        // ARC edges — candidates are filtered to the CCW sweep window
        // (stage-1 arc-chain convention) by `point_in_rim_sweep`, which
        // also rejects candidates coinciding with the rim's own B-Rep
        // vertices (arc endpoints / the closed rim's seam): such a
        // junction already IS a mesh vertex, and inserting its twin would
        // trip the uniform-coincidence stop (the seam sits at ring slot 0).
        let arc = if e.start != e.end {
            Some((
                x.vertices()[e.start as usize].point.as_array(),
                x.vertices()[e.end as usize].point.as_array(),
            ))
        } else {
            None
        };
        let rim = RimDesc {
            edge: ei as u32,
            c,
            n,
            r,
            seam: x.vertices()[e.start as usize].point.as_array(),
            arc,
        };
        // Increment 4 v1 scope (demonstrated need — the whole measured
        // class is CONE-band lathes): the PLANE arm fires only on rims
        // flanked by ≥1 cone face. Cylinder-rim × plane-face junctions
        // have no demanding case, and the corpus proves that population
        // healthy without insertion (F0047/R0006/R0075/F0081 were CORRECT
        // pre-arm and regressed under it; R0091's cut-tool rim insertions
        // unmask the banked-§3b unverifiable-χ path). The LATERAL arm
        // (the F0059 cylinder class) is independent and unchanged.
        let cone_flanked = x.faces().iter().any(|f| {
            matches!(f.surface, Surface::Cone { .. })
                && f.outer_loop
                    .iter()
                    .chain(f.inner_loops.iter().flatten())
                    .any(|&le| le == ei as u32)
        });
        let mut pts: Vec<Point3> = Vec::new();
        // Shared circle∩line quadratic for a line (q0 + t·u) in the rim
        // plane: t² + 2t·(q0−c)·u + |q0−c|² − r² = 0. `None` = miss or
        // graze (derived tangency gate, A14.2: roots closer than model
        // resolution are ONE point, not two transversal crossings).
        let circle_line_roots = |q0: [f64; 3], u: [f64; 3]| -> Option<[f64; 2]> {
            let m = sub(q0, c);
            let bq = dot(m, u);
            let cq = dot(m, m) - r * r;
            let disc = bq * bq - cq;
            if disc <= 0.0 {
                return None; // no crossing / exact tangent
            }
            let sq = disc.sqrt();
            if 2.0 * sq < cad_primitives::TAU_MODEL {
                return None;
            }
            Some([-bq - sq, -bq + sq])
        };
        for f in y.faces() {
            match f.surface {
                Surface::Cylinder {
                    axis_point,
                    axis_dir,
                    radius: rb,
                } => {
                    let d = normalize3(axis_dir.as_array());
                    // v1: axis parallel to the rim plane (same floor as the
                    // phantom guard's axis-parallel test).
                    if dot(n, d).abs() > 1e-12 {
                        continue;
                    }
                    let ap = axis_point.as_array();
                    let Some((z_lo, z_hi)) = lateral_extent(y, f, ap, d) else {
                        continue;
                    };
                    // Signed axis-to-rim-plane distance; |δ| ≥ r_b ⇒ empty
                    // or a plane-tangent lateral (the tangency class —
                    // skipped).
                    let delta = dot(n, sub(ap, c));
                    if delta.abs() >= rb {
                        continue;
                    }
                    // Section of the lateral in the rim plane: two lines
                    // parallel to the axis at in-plane offset ±√(r_b²−δ²)
                    // from the axis foot.
                    let w_half = (rb * rb - delta * delta).sqrt();
                    let foot = sub(ap, scl(n, delta));
                    let eo = normalize3(crs(d, n));
                    for sgn in [-1.0f64, 1.0] {
                        let q0 = add(foot, scl(eo, sgn * w_half));
                        let Some(roots) = circle_line_roots(q0, d) else {
                            continue;
                        };
                        for t in roots {
                            let pj = add(q0, scl(d, t));
                            // Inside the lateral's axial extent; the
                            // ±TAU_WORK slack keeps boundary-of-extent
                            // triple junctions (rim ∩ lateral ∩ far cap —
                            // the F0059 corners).
                            let z = dot(sub(pj, ap), d);
                            if z < z_lo - cad_primitives::TAU_WORK
                                || z > z_hi + cad_primitives::TAU_WORK
                            {
                                continue;
                            }
                            if !point_in_rim_sweep(&rim, pj) {
                                continue;
                            }
                            let pjp = Point3::new(pj[0], pj[1], pj[2]);
                            // Cross-arm dedup at model resolution (two
                            // laterals / both lines can meet the rim at one
                            // triple point).
                            let dup = pts.iter().any(|q| {
                                let qa = q.as_array();
                                let dd = sub(qa, pj);
                                dot(dd, dd) < cad_primitives::TAU_MODEL * cad_primitives::TAU_MODEL
                            });
                            if !dup {
                                pts.push(pjp);
                            }
                        }
                    }
                }
                // Increment 4 §4a (spec v1 table row 2, promoted): a PLANE
                // face sections the rim plane in a single line — the
                // coaxial cone-band junction class (R0017 et al.).
                Surface::Plane { normal: m, d } => {
                    if !cone_flanked {
                        continue; // v1 scope: cone-band rims only (see above)
                    }
                    let ma = m.as_array();
                    let mlen = dot(ma, ma).sqrt();
                    if mlen <= 0.0 {
                        continue;
                    }
                    let mh = scl(ma, 1.0 / mlen);
                    let dh = d / mlen;
                    let ndm = dot(n, mh);
                    let denom = 1.0 - ndm * ndm;
                    // Parallel/coincident planes have no transversal
                    // section line (same 1e-12 floor class as the lateral
                    // arm's axis test).
                    if denom <= 1e-12 {
                        continue;
                    }
                    // v1: polygonal faces only — every loop edge a
                    // LineSegment (arc-bounded caps keep today's walls).
                    let Some(face2d) = planar_face_segments(y, f, mh) else {
                        continue;
                    };
                    // Line P∩F: q0 lies in BOTH planes, direction u = n×m̂.
                    let alpha = -(dot(mh, c) + dh) / denom;
                    let mperp = sub(mh, scl(n, ndm));
                    let q0 = add(c, scl(mperp, alpha));
                    let u = normalize3(crs(n, mh));
                    let Some(roots) = circle_line_roots(q0, u) else {
                        continue;
                    };
                    for t in roots {
                        let pj = add(q0, scl(u, t));
                        // Within the face extents: boundary-inclusive
                        // (±TAU_WORK) 2D containment — the plane analog of
                        // the lateral arm's z-extent slack.
                        if !point_in_planar_face(&face2d, pj) {
                            continue;
                        }
                        if !point_in_rim_sweep(&rim, pj) {
                            continue;
                        }
                        let pjp = Point3::new(pj[0], pj[1], pj[2]);
                        let dup = pts.iter().any(|q| {
                            let qa = q.as_array();
                            let dd = sub(qa, pj);
                            dot(dd, dd) < cad_primitives::TAU_MODEL * cad_primitives::TAU_MODEL
                        });
                        if !dup {
                            pts.push(pjp);
                        }
                    }
                }
                _ => continue,
            }
        }
        if !pts.is_empty() {
            out.insert(ei as u32, pts);
        }
        rims.push(rim);
    }

    // §4b coaxial azimuth propagation: Stage-1 band strips
    // (`tessellate_cone_frustum_band`, the cylinder tube, the partial-arc
    // strips) pair rims ring-for-ring, so a junction azimuth inserted on
    // ONE rim of a coaxial stack must exist on ALL of them (where their
    // sweep covers it) — otherwise the stack's sample counts diverge and
    // the strip stops loudly.
    if !out.is_empty() {
        // Group rims by axis line: parallel normals (1e-12 floor) with
        // centers on one line (TAU_MODEL off-axis budget).
        let mut groups: Vec<Vec<usize>> = Vec::new();
        for i in 0..rims.len() {
            let (ci, ni) = (rims[i].c, rims[i].n);
            let mut placed = false;
            for g in groups.iter_mut() {
                let (cj, nj) = (rims[g[0]].c, rims[g[0]].n);
                let cx = crs(ni, nj);
                if dot(cx, cx).sqrt() > 1e-12 {
                    continue;
                }
                let w = sub(ci, cj);
                let along = dot(w, nj);
                let off = sub(w, scl(nj, along));
                if dot(off, off).sqrt() > cad_primitives::TAU_MODEL {
                    continue;
                }
                g.push(i);
                placed = true;
                break;
            }
            if !placed {
                groups.push(vec![i]);
            }
        }
        for g in &groups {
            if !g.iter().any(|&i| out.contains_key(&rims[i].edge)) {
                continue;
            }
            // Vocabulary gate: every operand face touching a group rim
            // must be a Cone/Cylinder/Plane — the surfaces whose Stage-1
            // tessellation consumes shared rim rings. A torus/sphere band
            // stack keeps today's loud walls (never a half-inserted
            // stack).
            let rim_set: std::collections::BTreeSet<u32> =
                g.iter().map(|&i| rims[i].edge).collect();
            let vocab_ok = x.faces().iter().all(|f| {
                let touches = f
                    .outer_loop
                    .iter()
                    .chain(f.inner_loops.iter().flatten())
                    .any(|e| rim_set.contains(e));
                !touches
                    || matches!(
                        f.surface,
                        Surface::Cone { .. } | Surface::Cylinder { .. } | Surface::Plane { .. }
                    )
            });
            if !vocab_ok {
                for &i in g {
                    out.remove(&rims[i].edge);
                }
                continue;
            }
            // One shared frame about the group axis (g[0] is the
            // lowest-index rim — deterministic, I4). ALL window / dedup
            // decisions below are made in ANGLE space with ONE shared
            // tolerance `th_eps = TAU_MODEL / r_min` — per-radius chord
            // tolerances would let band-partner arcs (which share their
            // sweep window) disagree by a point and stop the Stage-1
            // strip loudly on a count mismatch (the R0019 161-vs-162
            // wall). Angle-space decisions are conformal by construction.
            let (c0, axis) = (rims[g[0]].c, rims[g[0]].n);
            let (b1v, b2v) = ortho_basis(Vector3::new(axis[0], axis[1], axis[2]));
            let (b1, b2) = (b1v.as_array(), b2v.as_array());
            let two_pi = 2.0 * std::f64::consts::PI;
            let group_az = |p: [f64; 3]| -> f64 {
                let w = sub(p, c0);
                dot(w, b2).atan2(dot(w, b1)).rem_euclid(two_pi)
            };
            let r_min = g
                .iter()
                .map(|&i| rims[i].r)
                .fold(f64::INFINITY, f64::min)
                .max(cad_primitives::MIN_FEATURE_SIZE);
            let th_eps = cad_primitives::TAU_MODEL / r_min;
            // A rim's admissible azimuth window, with the ±th_eps margin
            // excluding its own B-Rep vertices (arc endpoints / seam).
            let in_window = |rim: &RimDesc, th: f64| -> bool {
                match rim.arc {
                    Some((sp, ep)) => {
                        // Own-orientation sweep mapped through the GROUP
                        // frame: the CCW window about rim.n runs start->end;
                        // in the group frame it runs the same way when
                        // rim.n aligns with the group axis, reversed when
                        // anti-aligned.
                        let a0 = group_az(sp);
                        let a1 = group_az(ep);
                        let aligned = dot(rim.n, axis) >= 0.0;
                        let (w0, w1) = if aligned { (a0, a1) } else { (a1, a0) };
                        let sweep = (w1 - w0).rem_euclid(two_pi);
                        let off = (th - w0).rem_euclid(two_pi);
                        off > th_eps && off < sweep - th_eps
                    }
                    None => {
                        let off = (th - group_az(rim.seam)).rem_euclid(two_pi);
                        off > th_eps && off < two_pi - th_eps
                    }
                }
            };
            // Cluster ALL direct-junction azimuths at th_eps. Each cluster
            // is one physical junction column; its representative azimuth
            // is the smallest member (deterministic).
            let mut annotated: Vec<(f64, usize, Point3)> = Vec::new();
            for &i in g {
                if let Some(pts) = out.get(&rims[i].edge) {
                    for pt in pts {
                        annotated.push((group_az(pt.as_array()), i, *pt));
                    }
                }
            }
            annotated.sort_by(|x, y| x.0.total_cmp(&y.0));
            let mut clusters: Vec<Vec<(f64, usize, Point3)>> = Vec::new();
            for a in annotated {
                match clusters.last_mut() {
                    Some(cl) if (a.0 - cl.last().unwrap().0).abs() <= th_eps => cl.push(a),
                    _ => clusters.push(vec![a]),
                }
            }
            // Wrap-around: the first and last clusters may be one junction
            // column split at the 0/2pi cut.
            if clusters.len() > 1 {
                let lo = clusters.first().unwrap().first().unwrap().0;
                let hi = clusters.last().unwrap().last().unwrap().0;
                if (lo + two_pi - hi).abs() <= th_eps {
                    let merged = clusters.pop().unwrap();
                    clusters[0].extend(merged);
                }
            }
            // Rebuild every rim's list from the clusters: the rim's own
            // direct point where it has one (the exact junction position),
            // else the on-circle point at the cluster representative.
            for &i in g {
                let rim = &rims[i];
                let mut pts: Vec<Point3> = Vec::new();
                for cl in &clusters {
                    let th = cl.first().unwrap().0;
                    if !in_window(rim, th) {
                        continue;
                    }
                    if let Some(own) = cl.iter().find(|(_, ri, _)| *ri == i) {
                        pts.push(own.2);
                    } else {
                        let (st, ct) = th.sin_cos();
                        let pj = add(rim.c, add(scl(b1, rim.r * ct), scl(b2, rim.r * st)));
                        pts.push(Point3::new(pj[0], pj[1], pj[2]));
                    }
                }
                if pts.is_empty() {
                    out.remove(&rim.edge);
                } else {
                    out.insert(rim.edge, pts);
                }
            }
        }
    }
    out
}

/// Increment 4: rim descriptor for `rim_junctions_against` — a full-circle
/// rim or a partial ARC (the corpus partial-revolve shape). For an arc,
/// the sweep runs CCW about `n` from `arc.0` to `arc.1` (the stage-1
/// arc-chain convention).
pub(crate) struct RimDesc {
    pub(crate) edge: u32,
    pub(crate) c: [f64; 3],
    pub(crate) n: [f64; 3],
    pub(crate) r: f64,
    /// The edge's start vertex — the seam of a closed rim (ring slot 0).
    pub(crate) seam: [f64; 3],
    pub(crate) arc: Option<([f64; 3], [f64; 3])>,
}

/// Increment 4: candidate filter — never within TAU_MODEL of the rim's
/// own B-Rep vertices (arc endpoints / the closed rim's seam: a boundary
/// junction IS the existing vertex; inserting its ULP twin would trip the
/// uniform-coincidence stop or desynchronize the chain), and for an ARC,
/// inside the CCW sweep window. Full-circle rims accept everything else.
pub(crate) fn point_in_rim_sweep(rim: &RimDesc, pj: [f64; 3]) -> bool {
    let dot = |a: [f64; 3], b: [f64; 3]| a[0] * b[0] + a[1] * b[1] + a[2] * b[2];
    let sub = |a: [f64; 3], b: [f64; 3]| [a[0] - b[0], a[1] - b[1], a[2] - b[2]];
    {
        let dd = sub(pj, rim.seam);
        if dot(dd, dd) < cad_primitives::TAU_MODEL * cad_primitives::TAU_MODEL {
            return false;
        }
    }
    let Some((sp, ep)) = rim.arc else {
        return true;
    };
    for q in [sp, ep] {
        let dd = sub(pj, q);
        if dot(dd, dd) < cad_primitives::TAU_MODEL * cad_primitives::TAU_MODEL {
            return false;
        }
    }
    let (e1v, e2v) = ortho_basis(Vector3::new(rim.n[0], rim.n[1], rim.n[2]));
    let (e1, e2) = (e1v.as_array(), e2v.as_array());
    let ang = |q: [f64; 3]| -> f64 {
        let w = sub(q, rim.c);
        dot(w, e2).atan2(dot(w, e1))
    };
    let two_pi = 2.0 * std::f64::consts::PI;
    let phi0 = ang(sp);
    let sweep = (ang(ep) - phi0).rem_euclid(two_pi);
    let off = (ang(pj) - phi0).rem_euclid(two_pi);
    off < sweep
}

/// Increment 4 §4a: a planar face's loops as 2D segments + full circles in
/// the plane's own frame (frame returned alongside so containment projects
/// identically) — `None` when any loop edge is neither a `LineSegment` nor
/// a closed `Circle` (arc-bounded faces keep today's loud walls). Inner
/// loops (holes) are included: even-odd containment handles both segment
/// and circle boundaries by parity, so discs, annuli, polygons, and mixed
/// forms all work.
pub(crate) type PlanarFace2d = (
    [[f64; 3]; 2],
    Vec<([f64; 2], [f64; 2])>,
    Vec<([f64; 2], f64)>,
);

pub(crate) fn planar_face_segments(
    brep: &BRep,
    f: &BRepFace,
    plane_unit_normal: [f64; 3],
) -> Option<PlanarFace2d> {
    let dot = |a: [f64; 3], b: [f64; 3]| a[0] * b[0] + a[1] * b[1] + a[2] * b[2];
    let nh = plane_unit_normal;
    let (e1v, e2v) = ortho_basis(Vector3::new(nh[0], nh[1], nh[2]));
    let (e1, e2) = (e1v.as_array(), e2v.as_array());
    let mut segs: Vec<([f64; 2], [f64; 2])> = Vec::new();
    let mut circles: Vec<([f64; 2], f64)> = Vec::new();
    for &ei in f.outer_loop.iter().chain(f.inner_loops.iter().flatten()) {
        let e = &brep.edges()[ei as usize];
        match e.curve {
            Curve::LineSegment => {
                let a3 = brep.vertices()[e.start as usize].point.as_array();
                let b3 = brep.vertices()[e.end as usize].point.as_array();
                segs.push(([dot(a3, e1), dot(a3, e2)], [dot(b3, e1), dot(b3, e2)]));
            }
            Curve::Circle { center, radius, .. } if e.start == e.end => {
                let c3 = center.as_array();
                circles.push(([dot(c3, e1), dot(c3, e2)], radius));
            }
            _ => return None,
        }
    }
    Some(([e1, e2], segs, circles))
}

/// Increment 4 §4a: boundary-inclusive (±TAU_WORK) even-odd containment of
/// a 3D point (assumed ON the face plane) in the planar face's boundary
/// set. The TAU_WORK boundary band keeps triple junctions at face edges —
/// the plane analog of the lateral arm's z-extent slack. Holes are
/// excluded by parity (segment ray crossings + circle inside-count).
pub(crate) fn point_in_planar_face(face2d: &PlanarFace2d, p3: [f64; 3]) -> bool {
    let dot3 = |a: [f64; 3], b: [f64; 3]| a[0] * b[0] + a[1] * b[1] + a[2] * b[2];
    let ([e1, e2], segs, circles) = face2d;
    let p = [dot3(p3, *e1), dot3(p3, *e2)];
    // Boundary band first (a point within TAU_WORK of any loop boundary is
    // IN — never lose a face-edge triple junction to parity jitter).
    for &(a, b) in segs {
        let ab = [b[0] - a[0], b[1] - a[1]];
        let ap = [p[0] - a[0], p[1] - a[1]];
        let len2 = ab[0] * ab[0] + ab[1] * ab[1];
        let t = if len2 > 0.0 {
            ((ap[0] * ab[0] + ap[1] * ab[1]) / len2).clamp(0.0, 1.0)
        } else {
            0.0
        };
        let dx = ap[0] - t * ab[0];
        let dy = ap[1] - t * ab[1];
        if (dx * dx + dy * dy).sqrt() <= cad_primitives::TAU_WORK {
            return true;
        }
    }
    for &(cc, r) in circles {
        let d = ((p[0] - cc[0]).powi(2) + (p[1] - cc[1]).powi(2)).sqrt();
        if (d - r).abs() <= cad_primitives::TAU_WORK {
            return true;
        }
    }
    // Even-odd parity: +x-ray crossings over segments (half-open on each
    // segment's y-range so shared loop vertices count once) + one toggle
    // per enclosing circle.
    let mut inside = false;
    for &(a, b) in segs {
        if (a[1] > p[1]) != (b[1] > p[1]) {
            let xi = a[0] + (p[1] - a[1]) / (b[1] - a[1]) * (b[0] - a[0]);
            if xi > p[0] {
                inside = !inside;
            }
        }
    }
    for &(cc, r) in circles {
        let d = ((p[0] - cc[0]).powi(2) + (p[1] - cc[1]).powi(2)).sqrt();
        if d < r {
            inside = !inside;
        }
    }
    inside
}

/// Increment-2 entry point: both operands' rim junction maps against each
/// other (wired in `boolean()` behind the no-Stage-0-interaction scope
/// gate; spec branch table row 3 records the pass-through trap that gate
/// avoids).
pub(crate) fn rim_junction_overrides(
    a: &BRep,
    b: &BRep,
) -> (
    std::collections::BTreeMap<u32, Vec<Point3>>,
    std::collections::BTreeMap<u32, Vec<Point3>>,
) {
    (rim_junctions_against(a, b), rim_junctions_against(b, a))
}

#[cfg(test)]
mod edge_graze_tests {
    use super::*;

    /// R0100's anchor numbers verbatim (spec
    /// `yang_433_case_iv_corner_phantom.md` §1): face-15's cone and the
    /// prism cap-corner edge S1∩S2 from the corner vertex to the wedge-face
    /// exit. Both exact roots fall outside the segment (t=+9.30 behind the
    /// corner, t=−225.9 beyond the far end), the corner sits 2.118 under
    /// the surface, and the derived demand must clear the measured green
    /// floor (N=30) with the factor-2 phase margin.
    fn r0100_cone() -> Surface {
        Surface::Cone {
            apex: Point3::new(-158.66237771434626, 712.2418755345027, 1139.4608460321217),
            axis_dir: Vector3::new(0.0, -0.628340168224518, -0.7779387077370457),
            half_angle: 0.2054337405657868,
        }
    }
    const R0100_CORNER: [f64; 3] = [110.48001521746164, -91.02777345602973, -51.176854359112916];
    const R0100_V0: [f64; 3] = [125.549753154, -151.880624691, -51.176854359];
    const R0100_RMAX: f64 = 322.1053789887729;
    /// Face 15's own station band (bottom rim to top rim on the cone axis).
    fn r0100_band() -> ([f64; 3], [f64; 3], f64, f64) {
        (
            [-158.66237771434626, 712.2418755345027, 1139.4608460321217],
            normalize3([0.0, -0.628340168224518, -0.7779387077370457]),
            1429.233100113699,
            1545.8089032486437,
        )
    }

    #[test]
    fn r0100_corner_edge_derives_above_green_floor() {
        let n = segment_face_graze_n(
            R0100_CORNER,
            R0100_V0,
            r0100_cone(),
            R0100_RMAX,
            r0100_band(),
        )
        .expect("non-piercing near edge must derive a demand");
        assert!(
            (30..=64).contains(&n),
            "derived N={n} must clear the measured green floor 30 and stay practical"
        );
    }

    #[test]
    fn piercing_segment_derives_nothing() {
        // A segment straddling the cone surface radially at the corner's
        // station: from the (buried) corner straight out past the flank.
        let out = [
            R0100_CORNER[0] + 20.0 * 0.9464,
            R0100_CORNER[1] + 20.0 * (-0.2434),
            R0100_CORNER[2] + 20.0 * 0.2113,
        ];
        // Direction chosen radially-ish; verify the premise (a root inside)
        // via the shared solver, then the guard must decline.
        let roots =
            crate::stage4_phantom::segment_surface_roots(R0100_CORNER, out, r0100_cone()).unwrap();
        assert!(
            roots.iter().any(|t| (0.0..=1.0).contains(t)),
            "test premise: the segment must pierce (roots {roots:?})"
        );
        assert_eq!(
            segment_face_graze_n(R0100_CORNER, out, r0100_cone(), R0100_RMAX, r0100_band()),
            None
        );
    }

    #[test]
    fn far_segment_demand_is_absorbed_by_any_natural_n() {
        // A segment ~300 under the surface derives a tiny N (huge clearance
        // halves into a huge sagitta budget) — the natural-N gate absorbs it.
        let p0 = [-158.0, 500.0, 900.0];
        let p1 = [-150.0, 480.0, 880.0];
        let n = segment_face_graze_n(p0, p1, r0100_cone(), R0100_RMAX, r0100_band());
        if let Some(n) = n {
            assert!(
                n <= 24,
                "far segment must not out-demand natural N (got {n})"
            );
        }
    }

    #[test]
    fn touching_segment_stays_loud_not_derived() {
        // A segment with an endpoint exactly ON the flank (clearance lower
        // bound not strictly positive) derives nothing — tangency stays
        // with the loud downstream STOP.
        let s = r0100_cone();
        // Point on the surface: walk from the corner radially to the flank.
        let (apex, u, ha) = match s {
            Surface::Cone {
                apex,
                axis_dir,
                half_angle,
            } => (apex.as_array(), normalize3(axis_dir.as_array()), half_angle),
            _ => unreachable!(),
        };
        let w = [
            R0100_CORNER[0] - apex[0],
            R0100_CORNER[1] - apex[1],
            R0100_CORNER[2] - apex[2],
        ];
        let h = w[0] * u[0] + w[1] * u[1] + w[2] * u[2];
        let rad = [w[0] - h * u[0], w[1] - h * u[1], w[2] - h * u[2]];
        let rl = (rad[0] * rad[0] + rad[1] * rad[1] + rad[2] * rad[2]).sqrt();
        let target = h * ha.tan();
        let on = [
            apex[0] + h * u[0] + rad[0] / rl * target,
            apex[1] + h * u[1] + rad[1] / rl * target,
            apex[2] + h * u[2] + rad[2] / rl * target,
        ];
        let p1 = [on[0] + 1.0, on[1], on[2]];
        assert_eq!(
            segment_face_graze_n(on, p1, s, R0100_RMAX, r0100_band()),
            None
        );
    }
}
