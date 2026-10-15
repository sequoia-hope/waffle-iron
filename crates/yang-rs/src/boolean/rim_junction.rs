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

/// Thin-band chart guard (2026-09-05, R0044 face 166; spec
/// `yang_stage1_curved_holed_patch` "The thin-band chart guard"): the
/// Case-IV statement for two rim CIRCLES that bound the SAME face. In that
/// face's chart the two rims are concentric arcs — the isometric development
/// of a cone, or the plane itself — and their chords must not interleave, or
/// the chart polygon crosses itself and the CDT either fails loud or (the
/// R0040 pin) paves the crossing silently.
///
/// Only the OUTER rim's chords can reach the inner rim: every chord of an arc
/// lies inside that arc's own disk, so the inner rim's chords dip AWAY from
/// the outer rim and the outer rim's chords dip TOWARD it. The outer chord
/// stays outside the inner arc iff `sag_outer < gap`; the requirement is the
/// smallest `N` with `sag(r_outer, N) ≤ gap / 2` (`sag(r, N) = r(1 −
/// cos(π/N))`, the Stage-1 sagitta; the factor-2 margin keeps the outer
/// chord band inside its own half of the band, and a finer N is always
/// chord-valid). `gap` is the IN-CHART distance between the circles:
///
/// - `Surface::Cone`: both rims are coaxial; `gap = |h_a − h_b| / cos α`,
///   the slant separation (a rim at axial height `h` from the apex is the
///   development arc of radius `h / cos α`); the outer rim is the one
///   farther from the apex. R0044: rims 1.07 apart along the axis at
///   α = 1.011 ⇒ gap 2.02 at r ≈ 3682, sampled at N = 41 (sag 10.8) ⇒ 17
///   rim × rim crossings; the rule derives N = 135.
/// - `Surface::Plane`, NESTED circles (a thin annulus, a hole close to a
///   circular rim): `gap = r_large − d − r_small`, the outer rim is the
///   larger circle.
/// - `Surface::Plane`, EXTERNAL circles (two holes side by side): each
///   circle's chords stay inside its own disjoint disk — no interleave is
///   possible at any N: `None`.
/// - a cylinder's rims are parallel lines in its strip, a torus / sphere
///   has its own chart: `None`.
///
/// `None` also for the same circle twice, a non-positive gap (intersecting
/// circles — a real curve, not a band), or a true near-tangency whose N
/// would exceed 4096 (the loud CDT stop remains the tripwire, P9). Far
/// pairs derive a tiny N the natural Stage-1 `max()` absorbs — self-limiting,
/// no mode branch (the `cyl_pair_phantom_n` pattern).
pub(crate) fn face_rim_pair_phantom_n(
    surface: Surface,
    (ca, ra): (Point3, f64),
    (cb, rb): (Point3, f64),
) -> Option<usize> {
    let (gap, r_outer) = match surface {
        Surface::Cone {
            apex,
            axis_dir,
            half_angle,
        } => {
            let au = normalize3(axis_dir.as_array());
            let ap = apex.as_array();
            let h = |c: Point3| -> f64 {
                let q = c.as_array();
                (q[0] - ap[0]) * au[0] + (q[1] - ap[1]) * au[1] + (q[2] - ap[2]) * au[2]
            };
            let cos_a = half_angle.cos();
            if cos_a.partial_cmp(&0.0) != Some(std::cmp::Ordering::Greater) {
                return None; // a degenerate (≥ 90°) or NaN half-angle
            }
            let (ha, hb) = (h(ca), h(cb));
            let outer = if ha.abs() >= hb.abs() { ra } else { rb };
            ((ha - hb).abs() / cos_a, outer)
        }
        Surface::Plane { .. } => {
            let (a, b) = (ca.as_array(), cb.as_array());
            let d = ((a[0] - b[0]).powi(2) + (a[1] - b[1]).powi(2) + (a[2] - b[2]).powi(2)).sqrt();
            let nested = ra.max(rb) - d - ra.min(rb);
            if nested.partial_cmp(&0.0) != Some(std::cmp::Ordering::Greater) {
                // External (disjoint disks — no interleave possible) or
                // intersecting (a real curve) or NaN: nothing to demand.
                return None;
            }
            (nested, ra.max(rb))
        }
        _ => return None,
    };
    if gap.partial_cmp(&0.0) != Some(std::cmp::Ordering::Greater) {
        return None; // the same circle / NaN
    }
    let sag = |r: f64, n: usize| r * (1.0 - (std::f64::consts::PI / n as f64).cos());
    let mut n = 3usize;
    while sag(r_outer, n) > gap / 2.0 {
        n += 1;
        if n > 4096 {
            return None; // true near-tangency: the loud CDT stop stands
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
/// line (authored flush-assembly rims) demands nothing here or anywhere;
/// depth at or below the render-observability line
/// [`RIM_PLANE_RENDER_LINE`]`·r` demands nothing BODY-WIDE and is the
/// LOCAL arm's population instead (spec §5k,
/// `rim_plane_graze_local_rim_overrides` — it was "§4.5.2 territory" until
/// P0029 landed there); N > 4096 demands nothing for inc-2 (NO SubSagitta
/// STOP arm — the emitted class detonates loudly at the next boolean's
/// (4b) gate; a producer-side STOP is a named follow-up needing the
/// plane-face extent witness). No phase-aware mesh-touch filter: the
/// partner plane face may legitimately intersect the rim's operand
/// ELSEWHERE (F0082's wall is crossed by the tube lateral) while the rim
/// crossing is still missed — a face-global touch test would veto the
/// needed boost, and the render line already bounds N ≈ 71.
pub(crate) fn rim_plane_graze_n(
    rim: ([f64; 3], [f64; 3], f64),
    plane: ([f64; 3], f64),
) -> Option<usize> {
    let r = rim.2;
    let lens = rim_plane_lens(rim, plane)?;
    // Render-observability line, single-radius form of #172 §3: a lens
    // shallower than the render mesh's own rim sagitta cannot be
    // represented at any output resolution; bounds the derived N ≈ 71.
    // It bounds only the BODY-WIDE demand this function feeds; the local
    // arm (`rim_plane_graze_local_rim_overrides`) takes the sites below it,
    // where an extra sample costs a handful of triangles rather than a
    // rim-N floor on both operands.
    if lens.depth <= RIM_PLANE_RENDER_LINE * r {
        return None;
    }
    rim_sag_demand(r, lens.depth)
}

/// The render-observability line of the #195 rim×plane arm (spec
/// `yang_195_seal_neighborhood_self_overlap` §5c, single-radius form of
/// #172 §3): the lens depth below which a BODY-WIDE rim-N boost is refused.
/// Also the dividing line between the two arms — above it the demand is paid
/// as a rim-N floor, below it as local samples on the grazed arc alone.
pub(crate) const RIM_PLANE_RENDER_LINE: f64 = 2.0e-3;

/// The smallest rim segment count whose chord sagitta clears half a lens of
/// `depth` on a radius-`r` circle — `sag(r, N) = r(1 − cos(π/N)) ≤ depth/2`,
/// the A14.3 single source with the factor-2 phase margin every sibling
/// guard uses. `None` past 4096: a true near-tangency with no practical
/// refinement, where the loud downstream STOP stays the tripwire.
pub(crate) fn rim_sag_demand(r: f64, depth: f64) -> Option<usize> {
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

/// The lens a rim circle and a partner plane form where they shallowly
/// cross — the shared geometry of the #195 rim×plane arm's two remedies, so
/// the body-wide and the local form can never disagree about whether (or
/// where) a pair grazes.
pub(crate) struct RimPlaneLens {
    /// Extent of the crossing on its shallow side, `r·k − |s|`.
    pub depth: f64,
    /// Azimuth of the submerged arc's DEEPEST point, where the rim stands
    /// `depth` clear of the plane. In the frame
    /// `ortho_basis(canonical_axis(rim normal))` — the one frame every rim of
    /// a coaxial closure agrees on.
    pub apex: f64,
    /// Half the angular width of the submerged arc: the crossings are at
    /// `apex ± half_span`.
    pub half_span: f64,
}

/// The rim×plane lens of one pair (#195 inc-2 geometry, spec
/// `yang_195_seal_neighborhood_self_overlap` §5): a Circle rim edge
/// (center `c`, unit normal `n`, radius `r`) against a partner PLANE
/// (`m̂·p + d̂ = 0`). The circle's signed distances to the plane span
/// `s ± r·k` (`s = m̂·c + d̂`, `k = √(1−(n·m̂)²)`), so it crosses iff
/// `|s| < r·k` and the shallow-side extent is `depth = r·k − |s|`.
///
/// `None` for a degenerate plane, for no crossing (or a rim lying IN the
/// plane), and for a crossing at or below the #178-calibrated
/// coincidence-authoring noise line — flush-assembly rims arrive with
/// sub-noise crossing residue and must demand nothing.
pub(crate) fn rim_plane_lens(
    (c, n, r): ([f64; 3], [f64; 3], f64),
    (m, d): ([f64; 3], f64),
) -> Option<RimPlaneLens> {
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
    // #178-calibrated coincidence-authoring noise line (the form the retired
    // cylinder-pair graze guard also used): flush-assembly rims arrive with
    // sub-noise crossing residue and must not boost.
    let mut scale = r;
    for &v in c.iter() {
        scale = scale.max(v.abs());
    }
    scale = scale.max(dh.abs());
    let noise = cad_primitives::TAU_MODEL.max(scale * cad_primitives::TAU_WORK) / 100.0;
    if depth <= noise {
        return None;
    }
    // Azimuth of the submerged arc. In the canonical frame the plane's
    // signed distance along the rim is `f(θ) = s + R·cos(θ − φ)` with
    // `R = |m̂ projected into the rim plane| · r` (= r·k for a unit `n`),
    // so the crossings are `θ = φ ± ψ`, `cos ψ = −s/R`, and the shallow
    // side is the arc on which `f` carries the sign of `−s`.
    let axis_c = canonical_axis(n);
    let (e1, e2) = ortho_basis(Vector3::new(axis_c[0], axis_c[1], axis_c[2]));
    let (ca, cb) = (r * dot(mh, e1.as_array()), r * dot(mh, e2.as_array()));
    let radius_m = ca.hypot(cb);
    if radius_m.partial_cmp(&0.0) != Some(std::cmp::Ordering::Greater) {
        return None; // rim plane parallel to the partner plane: no arc
    }
    let phi = cb.atan2(ca);
    let psi = (-s / radius_m).clamp(-1.0, 1.0).acos();
    let (apex, half_span) = if s > 0.0 {
        // Shallow side is `f < 0`: deepest OPPOSITE φ, crossings at ±ψ.
        (phi + std::f64::consts::PI, std::f64::consts::PI - psi)
    } else {
        // Shallow side is `f > 0`: deepest AT φ.
        (phi, psi)
    };
    Some(RimPlaneLens {
        depth,
        apex,
        half_span,
    })
}

/// The azimuths the #195 LOCAL arm inserts over a submerged arc: the APEX,
/// then outward in both directions at `step` while still strictly inside the
/// arc. `None` past [`LOCAL_REFINE_MAX_SAMPLES`].
///
/// Apex-centred, and the crossings themselves are deliberately NOT emitted.
/// Two reasons, both measured:
///
///  * The crossings are the §4.3.3 generator's and the rim-junction scan's
///    to mint, and they mint them on the grazing rim ALONE. Re-minting them
///    here made one rim of a closure dedup a mint bit-for-bit where its
///    partner did not, and the band merge — which pairs the two rings
///    POSITIONALLY — refused the operand (`face 2: azimuth-merge rims have
///    mismatched / too-few samples (21 vs 22)`, P0029's first wiring).
///  * They are not needed. The arm's whole job is to make the chord
///    polyline DIP PAST the plane as the exact rim does; Stage 2 then
///    computes the crossing exactly and Stage 4 relocates it onto the exact
///    curve. A vertex at the crossing buys nothing the arrangement does not
///    already derive.
///
/// The apex alone already carries the guarantee: it stands the full `depth`
/// clear of the plane while its neighbours — uniform samples outside the
/// arc — stand on the other side, so the polyline crosses twice, once each
/// side of the apex, exactly as the exact rim does. The outward samples
/// bound the chord error over the rest of the arc at `step`'s own sagitta,
/// which is what keeps the mesh inside the band Stage 4 relocates within.
pub(crate) fn submerged_arc_samples(apex: f64, half_span: f64, step: f64) -> Option<Vec<f64>> {
    let positive = |v: f64| v.partial_cmp(&0.0) == Some(std::cmp::Ordering::Greater);
    if !positive(step) || !positive(half_span) {
        return None;
    }
    let reach = ((half_span / step).ceil() as usize).max(1);
    if 2 * reach + 1 > LOCAL_REFINE_MAX_SAMPLES {
        return None;
    }
    let mut angles = vec![apex];
    for j in 1..=reach {
        let off = j as f64 * step;
        if off >= half_span {
            break;
        }
        angles.push(apex - off);
        angles.push(apex + off);
    }
    Some(angles)
}

/// ONE canonical sense for a rim axis: coaxial rims may store OPPOSITE axis
/// senses and `ortho_basis(u) != ortho_basis(-u)`, so every azimuth in this
/// module is taken in the frame of the axis with its first non-zero
/// component positive.
pub(crate) fn canonical_axis(u: [f64; 3]) -> [f64; 3] {
    for c in u {
        if c > 0.0 {
            return u;
        }
        if c < 0.0 {
            return [-u[0], -u[1], -u[2]];
        }
    }
    u
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
pub(crate) fn point_surface_signed(p: [f64; 3], s: Surface) -> Option<f64> {
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

/// The ceiling on one site's inserted sample count (4096, the demand
/// ceiling the retired §4.3.3 edge-graze arm also used): past it the
/// configuration is a true near-tangency with no practical local
/// refinement, so the site derives NOTHING and the loud downstream STOP
/// stays its tripwire.
pub(crate) const LOCAL_REFINE_MAX_SAMPLES: usize = 4096;

/// The face's axial station band, derived from its own rim circles (their
/// centers projected on the surface axis) — the face's mesh lives strictly
/// within this sweep (rim vertices lie ON the rims), so clearance against
/// the surface's infinite extension beyond it can never mint a phantom on
/// THIS face. `None` for a face with fewer than one rim circle or a
/// non-axial surface — the guard then skips the face (fail closed: no
/// demand, the downstream STOP remains).
pub(crate) fn face_station_band(
    f: &BRepFace,
    brep: &BRep,
) -> Option<([f64; 3], [f64; 3], f64, f64)> {
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

/// Extra rim azimuths accumulated per (operand, coaxial rim closure) — the
/// shared tail of every LOCAL rim-refinement arm (§4.3.3 Case-IV corner
/// phantoms, #195 rim×plane grazes).
///
/// The closure, not the face, is the unit: two sites on one closure end up
/// with the IDENTICAL azimuth set, so every rim in it carries the same
/// samples and the §4.5.2 domain lens (P0031) can rely on each rim's own
/// inside-sweep subset of one shared set. (Since 2026-10-13 the band strip
/// is an azimuth ZIPPER that accepts unequal rings — spec
/// `stage1_azimuth_merge_unequal_rings.md` — so a leftover per-rim
/// difference is no longer a STOP; the shared set per closure is kept as
/// the sampling-consistency choice, not as a tessellator requirement.)
#[derive(Default)]
pub(crate) struct RimAngleOverrides {
    per_closure: std::collections::BTreeMap<(bool, Vec<u32>), RimAngleAcc>,
}

struct RimAngleAcc {
    angles: Vec<f64>,
    /// The finest step any site on this closure asked for — the dedup
    /// scale, so a coarse site never swallows a finer site's sample.
    min_step: f64,
    /// ONE canonical azimuth frame for the whole closure (see
    /// [`canonical_axis`]); set by the first site and shared, the rims of a
    /// closure being coaxial by construction.
    axis: [f64; 3],
}

impl RimAngleOverrides {
    /// Record one site's azimuths against its closure. Called only once the
    /// site is fully derived, so a site that fails mid-derivation
    /// contributes nothing.
    pub(crate) fn commit(
        &mut self,
        side_is_a: bool,
        rims: Vec<u32>,
        axis_c: [f64; 3],
        angles: Vec<f64>,
        step: f64,
    ) {
        let two_pi = 2.0 * std::f64::consts::PI;
        let entry = self
            .per_closure
            .entry((side_is_a, rims))
            .or_insert_with(|| RimAngleAcc {
                angles: Vec::new(),
                min_step: f64::INFINITY,
                axis: axis_c,
            });
        for t in angles {
            entry.angles.push(t.rem_euclid(two_pi));
        }
        entry.min_step = entry.min_step.min(step);
    }

    pub(crate) fn is_empty(&self) -> bool {
        self.per_closure.is_empty()
    }

    /// Resolve the accumulated azimuths to per-rim-edge sample POINTS, the
    /// shape `rebuilt_with_rim_overrides` consumes.
    pub(crate) fn finish(
        self,
        a: &BRep,
        b: &BRep,
    ) -> (
        std::collections::BTreeMap<u32, Vec<Point3>>,
        std::collections::BTreeMap<u32, Vec<Point3>>,
    ) {
        let mut out_a: std::collections::BTreeMap<u32, Vec<Point3>> = Default::default();
        let mut out_b: std::collections::BTreeMap<u32, Vec<Point3>> = Default::default();
        for ((side_is_a, rims), mut acc) in self.per_closure {
            acc.angles.sort_by(f64::total_cmp);
            let scale = acc.min_step * 1.0e-9;
            acc.angles.dedup_by(|x, y| (*x - *y).abs() <= scale);
            if acc.angles.len() > LOCAL_REFINE_MAX_SAMPLES {
                continue; // near-tangency at scale: no practical local refinement
            }
            let y = if side_is_a { a } else { b };
            let out = if side_is_a { &mut out_a } else { &mut out_b };
            let (e1, e2) = ortho_basis(Vector3::new(acc.axis[0], acc.axis[1], acc.axis[2]));
            let (e1, e2) = (e1.as_array(), e2.as_array());
            for ei in rims {
                let edge = &y.edges()[ei as usize];
                let Curve::Circle {
                    center,
                    normal,
                    radius,
                } = edge.curve
                else {
                    continue;
                };
                let c = center.as_array();
                // An ARC rim (the #195 arms never commit one; the §4.5.2
                // domain lens does, P0031) takes only the azimuths strictly
                // inside its sweep: Stage 1 refuses an arc-chord override at
                // or beyond either endpoint. The arc runs CCW about its OWN
                // stored normal from `start` to `end`; in the closure's
                // canonical frame that is CW when the normal is reversed.
                let inside_arc: Option<Box<dyn Fn(f64) -> bool>> = if edge.start != edge.end {
                    let az = |vi: u32| -> f64 {
                        let q = y.vertices()[vi as usize].point.as_array();
                        let w = [q[0] - c[0], q[1] - c[1], q[2] - c[2]];
                        (w[0] * e2[0] + w[1] * e2[1] + w[2] * e2[2])
                            .atan2(w[0] * e1[0] + w[1] * e1[1] + w[2] * e1[2])
                    };
                    let (phi_s, phi_e) = (az(edge.start), az(edge.end));
                    let nn = normalize3(normal.as_array());
                    let sense =
                        if nn[0] * acc.axis[0] + nn[1] * acc.axis[1] + nn[2] * acc.axis[2] >= 0.0 {
                            1.0
                        } else {
                            -1.0
                        };
                    let two_pi = 2.0 * std::f64::consts::PI;
                    let sweep = ((phi_e - phi_s) * sense).rem_euclid(two_pi);
                    let margin = (acc.min_step * 1.0e-3).max(1.0e-9);
                    Some(Box::new(move |t: f64| {
                        let off = ((t - phi_s) * sense).rem_euclid(two_pi);
                        off > margin && off < sweep - margin
                    }))
                } else {
                    None
                };
                // Two sample REQUESTS closer than the model's own coincidence
                // band `TAU_MODEL·(1 + scale)` (A14.2: nothing below it is a
                // representable feature) are one request (P0047, 2026-10-13,
                // spec `stage1_azimuth_merge_unequal_rings.md` §3b): two
                // sites' span endpoints 7.5e-4 rad apart on a 9e-5 radius
                // (6.8e-8 of arc) had minted two rulings a junction then sat
                // within the weld band of both. Likewise a free sample within
                // the band of a point this rim ALREADY carries — an earlier
                // arm's sample, a Stage-1 junction mint, a Stage-0 crossing
                // standing on the operand — is that point and is dropped (the
                // standing point keeps its bits; a junction's are shared with
                // the partner's edge chain). The ring builder holds the
                // ULP-twin rule for the opposite arrival order.
                let standing = y.standing_rim().get(&ei);
                let scale = c.iter().fold(0.0f64, |m, &v| m.max(v.abs())) + radius.abs();
                let band = cad_primitives::TAU_MODEL * (1.0 + scale);
                let band2 = band * band;
                let near = |p: &Point3, q: &Point3| {
                    let (pa, qa) = (p.as_array(), q.as_array());
                    let d = [pa[0] - qa[0], pa[1] - qa[1], pa[2] - qa[2]];
                    d[0] * d[0] + d[1] * d[1] + d[2] * d[2] <= band2
                };
                let mut kept: Vec<Point3> = Vec::new();
                let pts: Vec<Point3> = acc
                    .angles
                    .iter()
                    .filter(|&&t| inside_arc.as_ref().is_none_or(|f| f(t)))
                    .map(|&t| {
                        let (s, co) = t.sin_cos();
                        Point3::new(
                            c[0] + radius * (co * e1[0] + s * e2[0]),
                            c[1] + radius * (co * e1[1] + s * e2[1]),
                            c[2] + radius * (co * e1[2] + s * e2[2]),
                        )
                    })
                    .filter(|p| {
                        let dup = standing.is_some_and(|sv| sv.iter().any(|q| near(p, q)))
                            || kept.iter().any(|q| near(p, q));
                        if !dup {
                            kept.push(*p);
                        }
                        !dup
                    })
                    .collect();
                out.entry(ei).or_default().extend(pts);
            }
        }
        (out_a, out_b)
    }
}

/// The #195 rim×plane arm's LOCAL remedy (spec
/// `yang_195_seal_neighborhood_self_overlap` §5k): the extra rim samples
/// every SUB-RENDER-LINE rim×plane graze demands, spent on the grazed arc
/// of the rim's own coaxial closure.
///
/// A rim circle that crosses a partner plane shallower than its own chord
/// sagitta reaches the arrangement as a TANGENTIAL line contact instead of
/// a crossing: the chord between the two exact crossings lies, by
/// construction, in BOTH planes (its endpoints do), so the rim's two
/// incident faces and the partner plane all meet along one segment and the
/// exact arrangement faithfully reports a doubled edge — the measured
/// P0029 `s4-dc-attr` STOP (fwd=2 rev=2, a 0.0146-deep lens on a 24.13
/// radius, the two exact {plane, cap-plane, cylinder} corners already
/// seated as adjacent rim vertices). The sliver the exact solids trade
/// there is REAL (2.37 × 0.0337, 3.4e4 × `MIN_FEATURE_SIZE`), so the
/// configuration is a capability gap, not a loud-by-design wall.
///
/// [`rim_plane_graze_n`] refuses these: its body-wide rim-N floor would be
/// 128 where the natural density is 9, which the corpus has twice refused
/// (R0021's degenerated render rings, the 10.8× triangle explosion of the
/// §4.3.3 global form). The demand is paid LOCALLY instead — at
/// `sag(r, N) ≤ depth/2` the submerged arc takes an apex sample and its
/// outward neighbours at `2π/N` — THREE points on one rim closure for
/// P0029 — so the polyline dips past the plane as the exact rim does and
/// the contact becomes the transversal crossing it is. See
/// [`submerged_arc_samples`] for why the crossings themselves are not
/// re-minted here.
///
/// Scope is the complement of the body-wide arm's, so every case that
/// converts on the rim-N floor today is byte-identical: only lenses at or
/// below [`RIM_PLANE_RENDER_LINE`] are taken here. Self-limiting the same
/// way (a demand the owner's natural N already meets derives nothing), fail
/// closed on a non-coaxial or arc-bounded closure, and capped by
/// [`LOCAL_REFINE_MAX_SAMPLES`]. Kill switch `YANG_195_LOCAL=0|off`.
pub(crate) fn rim_plane_graze_local_rim_overrides(
    a: &BRep,
    b: &BRep,
) -> (
    std::collections::BTreeMap<u32, Vec<Point3>>,
    std::collections::BTreeMap<u32, Vec<Point3>>,
) {
    if matches!(
        std::env::var("YANG_195_LOCAL").as_deref(),
        Ok("0") | Ok("off")
    ) {
        return Default::default();
    }
    let two_pi = 2.0 * std::f64::consts::PI;
    let mut per_closure = RimAngleOverrides::default();
    let (nat_a, nat_b) = (natural_rim_n(a), natural_rim_n(b));
    for (x, y, side_is_a) in [(a, b, true), (b, a, false)] {
        let nat = if side_is_a { nat_a } else { nat_b };
        let planes: Vec<([f64; 3], f64)> = y
            .faces()
            .iter()
            .filter_map(|f| match f.surface {
                Surface::Plane { normal, d } => Some((normal.as_array(), d)),
                _ => None,
            })
            .collect();
        if planes.is_empty() {
            continue;
        }
        let mut closure_memo: std::collections::BTreeMap<u32, Option<Vec<u32>>> =
            Default::default();
        for (ei, e) in x.edges().iter().enumerate() {
            // Full-circle rims only — the same vocabulary the rim-N floor
            // and the §4.3.3 local form speak.
            let Curve::Circle {
                center,
                normal,
                radius,
            } = e.curve
            else {
                continue;
            };
            if e.start != e.end {
                continue;
            }
            let (c, n) = (center.as_array(), normalize3(normal.as_array()));
            for &pl in &planes {
                let Some(lens) = rim_plane_lens((c, n, radius), pl) else {
                    continue;
                };
                // The body-wide arm owns everything above the render line.
                if lens.depth > RIM_PLANE_RENDER_LINE * radius {
                    continue;
                }
                let Some(n_seg) = rim_sag_demand(radius, lens.depth) else {
                    continue;
                };
                if n_seg <= nat {
                    continue; // the natural density already samples the lens
                }
                let ei = ei as u32;
                let rims = closure_memo.entry(ei).or_insert_with(|| {
                    // Seed from a face that actually carries this rim, so the
                    // closure is the band the samples must move together.
                    let seed = x.faces().iter().position(|f| {
                        f.outer_loop
                            .iter()
                            .chain(f.inner_loops.iter().flatten())
                            .any(|&l| l == ei)
                    })?;
                    coaxial_rim_closure(x, seed, c, n)
                });
                let Some(rims) = rims.clone().filter(|r| !r.is_empty()) else {
                    continue;
                };
                let step = two_pi / n_seg as f64;
                let Some(angles) = submerged_arc_samples(lens.apex, lens.half_span, step) else {
                    continue;
                };
                per_closure.commit(side_is_a, rims, canonical_axis(n), angles, step);
            }
        }
    }
    let probe = std::env::var_os("YANG_SPLIT_PROBE").is_some();
    if probe && per_closure.is_empty() {
        eprintln!("[rim-plane-graze] LOCAL no site");
    }
    let (out_a, out_b) = per_closure.finish(a, b);
    if probe {
        eprintln!(
            "[rim-plane-graze] LOCAL rims_a={} pts_a={} rims_b={} pts_b={}",
            out_a.len(),
            out_a.values().map(Vec::len).sum::<usize>(),
            out_b.len(),
            out_b.values().map(Vec::len).sum::<usize>(),
        );
    }
    (out_a, out_b)
}

/// §4.5.2 LOCALIZATION, part 3 — the rims a local refinement of one face
/// must move TOGETHER.
///
/// Inserting a rim sample changes that rim's sample set, and every band
/// face incident to the rim shares it with its other rim (the uniform paths
/// pair rings by index; since 2026-10-13 unequal rings take the azimuth
/// zipper, spec `stage1_azimuth_merge_unequal_rings.md`, but one shared set
/// per closure keeps the §4.5.2 lens's per-rim subsets coherent). So the
/// unit of refinement is not one face: it is the transitive closure of
/// "faces sharing a full-circle rim", starting from the demanding face.
/// Returns that closure's rim edge ids, ascending.
///
/// `None` — fail closed, derive nothing — when any rim in the closure is not
/// COAXIAL with the demanding face's axis (same axis line, parallel
/// direction), because then one shared azimuth set cannot serve them all and
/// the positional pairing would twist.
pub(crate) fn coaxial_rim_closure(
    brep: &BRep,
    face: usize,
    axis_point: [f64; 3],
    axis_dir: [f64; 3],
) -> Option<Vec<u32>> {
    coaxial_rim_closure_impl(brep, face, axis_point, axis_dir, false)
}

fn coaxial_rim_closure_impl(
    brep: &BRep,
    face: usize,
    axis_point: [f64; 3],
    axis_dir: [f64; 3],
    admit_arcs: bool,
) -> Option<Vec<u32>> {
    let u = normalize3(axis_dir);
    let coaxial = |center: [f64; 3], normal: [f64; 3]| -> bool {
        let n = normalize3(normal);
        let dot = |a: [f64; 3], b: [f64; 3]| a[0] * b[0] + a[1] * b[1] + a[2] * b[2];
        // Parallel axes …
        let cr = [
            n[1] * u[2] - n[2] * u[1],
            n[2] * u[0] - n[0] * u[2],
            n[0] * u[1] - n[1] * u[0],
        ];
        if dot(cr, cr).sqrt() > 1e-12 {
            return false;
        }
        // … and the centre ON the axis line.
        let w = [
            center[0] - axis_point[0],
            center[1] - axis_point[1],
            center[2] - axis_point[2],
        ];
        let h = dot(w, u);
        let perp = [w[0] - h * u[0], w[1] - h * u[1], w[2] - h * u[2]];
        dot(perp, perp).sqrt() <= 1e-9 * (1.0 + dot(w, w).sqrt())
    };
    let full_rims_of = |f: &BRepFace| -> Vec<u32> {
        f.outer_loop
            .iter()
            .chain(f.inner_loops.iter().flatten())
            .copied()
            .filter(|&ei| {
                let e = &brep.edges()[ei as usize];
                (admit_arcs || e.start == e.end) && matches!(e.curve, Curve::Circle { .. })
            })
            .collect()
    };
    let mut seen_faces: std::collections::BTreeSet<usize> = Default::default();
    let mut rims: std::collections::BTreeSet<u32> = Default::default();
    let mut queue = vec![face];
    while let Some(fi) = queue.pop() {
        if !seen_faces.insert(fi) {
            continue;
        }
        let f = brep.faces().get(fi)?;
        for ei in full_rims_of(f) {
            let Curve::Circle { center, normal, .. } = brep.edges()[ei as usize].curve else {
                return None;
            };
            if !coaxial(center.as_array(), normal.as_array()) {
                return None; // fail closed: one azimuth set cannot serve it
            }
            if rims.insert(ei) {
                for (gi, g) in brep.faces().iter().enumerate() {
                    if gi != fi && full_rims_of(g).contains(&ei) {
                        queue.push(gi);
                    }
                }
            }
        }
    }
    Some(rims.into_iter().collect())
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

    // ---- P0013 §4 P5: the LOCALIZATION primitives ------------------------

    /// The refinement unit: both rims of a band move together, and a rim the
    /// shared azimuth set cannot serve refuses the whole site.
    #[test]
    fn coaxial_rim_closure_is_the_band_and_refuses_a_skew_rim() {
        let r = 2.0;
        let rim = |z: f64, normal: Vector3| BRepEdge {
            start: if z == 0.0 { 0 } else { 1 },
            end: if z == 0.0 { 0 } else { 1 },
            curve: Curve::Circle {
                center: Point3::new(0.0, 0.0, z),
                normal,
                radius: r,
            },
        };
        let z_up = Vector3::new(0.0, 0.0, 1.0);
        let lateral = |loops: Vec<u32>| BRepFace {
            surface: Surface::Cylinder {
                axis_point: Point3::new(0.0, 0.0, 0.0),
                axis_dir: z_up,
                radius: r,
            },
            outer_loop: loops,
            inner_loops: vec![],
            reversed: false,
        };
        let verts: Vec<BRepVertex> = vec![
            BRepVertex {
                point: Point3::new(r, 0.0, 0.0),
            },
            BRepVertex {
                point: Point3::new(r, 0.0, 5.0),
            },
        ];
        let brep = BRep::new(
            verts.clone(),
            vec![rim(0.0, z_up), rim(5.0, z_up)],
            vec![lateral(vec![0, 1])],
        )
        .expect("band brep");
        assert_eq!(
            coaxial_rim_closure(&brep, 0, [0.0, 0.0, 0.0], [0.0, 0.0, 1.0]),
            Some(vec![0, 1]),
            "a tube's refinement unit is BOTH its rims — one alone would \
             leave the positional ring pairing unequal"
        );
        // A rim the queried azimuth frame cannot serve (here: asking about
        // the +X axis, to which both rims are SKEW) refuses the whole site —
        // one azimuth set cannot keep such rims' ring lengths in step.
        assert_eq!(
            coaxial_rim_closure(&brep, 0, [0.0, 0.0, 0.0], [1.0, 0.0, 0.0]),
            None,
            "a non-coaxial rim must fail the site CLOSED"
        );
        // So must a rim that is parallel but OFF the axis line.
        assert_eq!(
            coaxial_rim_closure(&brep, 0, [1.0, 0.0, 0.0], [0.0, 0.0, 1.0]),
            None,
            "a parallel rim off the axis line must fail the site CLOSED"
        );
    }
}
