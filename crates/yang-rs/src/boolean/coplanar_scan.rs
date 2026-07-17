//! Stage-0 NEAR-coplanar input scan (PR-YR24/YR26): the detector that finds
//! cross-solid (A-face × B-face) and intra-solid near-coplanar face pairs
//! feeding the coplanar-boolean preprocessing. Extracted verbatim from
//! `boolean.rs` (move-only, spec `specs/yang_rs_lib_decomposition.md` F9).

#[allow(clippy::wildcard_imports)]
use crate::*;

/// PR-YR24: Stage-1 NEAR-coplanar input scan (PR-YR26: now the Stage-0
/// DETECTOR, no longer a hard gate).
///
/// Scans A-face × B-face pairs of the two input B-Reps (planar faces only,
/// while their surfaces are still symbolic `Surface::Plane`s — i.e. BEFORE
/// any mesh-level processing) and returns ALL cross pairs that are coplanar
/// within the sub-model-resolution band AND could actually interact
/// (overlapping AABBs), plus the first INTRA-solid pair (which remains the
/// loud unsupported residue).
///
/// **Why this scan exists.** Yang 2025 §4.5.5 requires coplanar face pairs
/// to be detected and resolved by a 2D Boolean at the B-Rep level BEFORE
/// mesh discretization ("it is necessary to check coplanar planes and
/// perform 2D Boolean operations before mesh discretizations",
/// `refs/text/yang2025_hybrid_boolean.txt:717-731`) — Stage 0, roadmap
/// milestone M8. Bit-EXACT coplanar overlaps that reach the arrangement
/// unhandled hit cherchi-rs's loud deferral (`CoplanarPairDeferred`,
/// deviation N17, `arrangements/soup.rs`). But f64 vertex construction
/// leaves femto-scale residuals on faces built on the SAME oblique sketch
/// plane (the KV4-F1 corpus family: R0029, F0016/18/19/21/25), so the EXACT
/// deferral does not catch them; the exact arrangement then faithfully
/// builds sub-f64-ulp sliver patches (all-LPI, all-border, width < 1 ulp)
/// whose in/out classification has no seedable ray origin
/// (`NoExplicitRayOrigin` — the C++ reference `booleans.cpp:504-575` would
/// exit there too). PR-YR24 converted both classes into the loud typed
/// `CoplanarFacesUnsupported` wall; PR-YR26 (M8 slice b) HANDLES the
/// cross-pair planar class via the §4.5.5 overlay (`stage0_preprocess`) and
/// keeps the wall only for the residue (intra pairs, unsupported face
/// shapes; multi-pair faces route through the plane-grouped n-ary overlay,
/// spec `m8_plane_group_nary_overlay`).
///
/// **The band.** For a candidate pair, with unit normals `n̂a`, `n̂b`
/// (orientation-aligned: `s = sign(n̂a·n̂b)`) and unit-normal plane offsets
/// `d̂ = d/‖n‖` (`n̂·x + d̂ = 0`):
///
/// ```text
/// scale = max |coordinate| over both faces' AABB corners
/// band  = max(TAU_MODEL, scale · TAU_WORK)
/// ```
///
/// and the pair is flagged iff ALL of:
/// 1. offset agreement:  |d̂a − s·d̂b| ≤ band
/// 2. parallel normals:  ‖n̂a × n̂b‖ · extent ≤ band, where `extent` is the
///    diagonal of the union of the two faces' AABBs (an angular tilt θ
///    displaces the planes by at most sin θ · extent over the region where
///    the faces could meet, so this bounds the true plane-to-plane gap by
///    2·band over that region)
/// 3. AABB overlap (each axis, inflated by band) — far-apart faces on the
///    same plane do not interact in the boolean and are NOT flagged
///    (over-deferral avoided).
///
/// Justification: `TAU_MODEL` (1e-7, absolute, governance A14) is the model
/// resolution — two parallel planes closer than `TAU_MODEL` are
/// sub-model-resolution and semantically the same plane (the R0029 family's
/// residuals are ~1e-13..1e-15 absolute at |coord| ~ 6e2, far inside the
/// band, while `MIN_FEATURE_SIZE` = 1e-6 guarantees genuinely distinct
/// model features sit OUTSIDE it). The `scale·TAU_WORK` term (relative
/// 1e-12 ≫ machine ε ≈ 2.2e-16) keeps the band above the f64
/// construction-noise floor for very large models where 1e-7 absolute
/// approaches the coordinate ulp; for |coord| < 1e5 it is inactive.
///
/// Conservative choices: face AABBs are taken over the loop edges'
/// START/END vertices (a curved rim's bulge is not included), which can
/// only UNDER-approximate the AABB — i.e. err toward NOT flagging; a missed
/// pair falls through to the existing loud downstream errors, never to a
/// silent wrong result. Non-planar surfaces are skipped (curved-curved
/// coplanarity is out of this gate's scope; the curved pipeline has its own
/// guards).
///
/// **Intra-solid pairs (the CHAINED KV4-F1 mechanism).** A solid that is
/// itself the output of an exact boolean re-creates near-incidences via
/// exact→f64 output rounding: the surviving A-side and B-side fragments of
/// one near-coplanar plane come back as faces of the SAME solid on planes a
/// few ulps apart (e.g. F0016's second union: operand A carries face pairs
/// with offset residual ~1.6e-16). The next boolean then builds the same
/// sub-ulp sliver patches. So the gate also scans A×A and B×B pairs — with
/// one crucial distinction: BIT-IDENTICAL intra-solid planes are benign (one
/// plane legitimately split into several faces, e.g. an annulus; cherchi's
/// N17 passes exact same-plane adjacency through) and are skipped; only
/// near-but-NOT-bit-identical intra pairs carry the femto signature. Cross
/// (A×B) pairs flag in BOTH cases — bit-exact A×B coplanarity is the
/// original M8 case.
///
/// Intra pairs use a DIFFERENT condition 3: the two fragments of a rounded
/// plane are usually disjoint in-plane regions that never overlap each
/// other, so the cross rule's mutual-overlap test can never fire. The
/// danger is contact by the OTHER solid: crossing both fragments creates
/// two cut lines a few ulps apart (verified on F0018), and even crossing
/// ONE fragment can cut through the rounded seam geometry the split left
/// behind (observed on F0025, where the other solid overlaps only one
/// fragment yet in/out still fails). AABB granularity cannot localize the
/// seam, so the conservative rule is: flag the intra pair iff the other
/// solid's whole-solid AABB overlaps EITHER fragment's AABB
/// (band-inflated). This over-defers a boolean that touches a femto-split
/// plane's region without actually reaching its seam — weighed and
/// accepted: a loud typed M8 deferral is strictly better than
/// `NoExplicitRayOrigin` (P9), and a boolean that stays clear of the
/// region entirely is still NOT flagged.
/// One near-coplanar CROSS (A-face × B-face) pair found by
/// [`scan_near_coplanar`], with the pair's detection `band`.
pub(crate) struct CrossCoplanarPair {
    pub(crate) face_a: usize,
    pub(crate) face_b: usize,
    pub(crate) band: f64,
    /// The pair's orientation-aligned unit-normal offset gap
    /// `|d̂a − s·d̂b|` — the plane-to-plane separation the overlay would
    /// dissolve. Exactly `0.0` for bit-exact coplanar pairs; ~1e-16-relative
    /// for legitimate rounding twins (the chained-output femto class); a
    /// genuinely NONZERO sub-band value means two DISTINCT model planes
    /// closer than the detection band (the C0111/C0113 sub-resolution wall,
    /// task #178).
    pub(crate) gap: f64,
    /// #178 (spec `yang_178_subres_coplanar_gap_stop.md`): `true` iff
    /// `gap > band/100` — the pair's planes are DISTINCT beyond the
    /// coincidence-authoring noise class (measured: corpus femto twins
    /// ≤ 2.7e-12; the real mm-scale bearing-recess producer residual
    /// ≤ 2.235e-10 — both weld; the designed C0111/C0113 rungs 1e-8 and
    /// TAU_MODEL sit 10–100× above the line and STOP), so the interposed
    /// volume is a sub-resolution feature the overlay would silently
    /// dissolve. `stage0_preprocess` STOPs loudly on it.
    pub(crate) sub_resolution: bool,
}

/// Output of [`scan_near_coplanar`]: ALL cross pairs (PR-YR26 Stage-0
/// handles each via the §4.5.5 overlay) plus the FIRST intra-solid pair
/// (still the loud unsupported-residue error — the chained-output class).
pub(crate) struct CoplanarScan {
    pub(crate) cross: Vec<CrossCoplanarPair>,
    pub(crate) intra: Option<(InputId, usize, usize)>,
}

pub(crate) fn scan_near_coplanar(a: &BRep, b: &BRep) -> CoplanarScan {
    /// Per-face plane data: unit normal, unit-normal offset, loop-vertex
    /// AABB, plus the RAW (un-normalized) plane bits for the intra-solid
    /// bit-identical exclusion.
    struct FacePlane {
        n: [f64; 3],
        d: f64,
        lo: [f64; 3],
        hi: [f64; 3],
        raw_bits: [u64; 4],
    }

    fn collect(brep: &BRep) -> Vec<Option<FacePlane>> {
        brep.faces()
            .iter()
            .map(|f| {
                let Surface::Plane { normal, d } = f.surface else {
                    return None;
                };
                let na = normal.as_array();
                let len = (na[0] * na[0] + na[1] * na[1] + na[2] * na[2]).sqrt();
                if len < cad_primitives::MIN_FEATURE_SIZE {
                    // Degenerate normal — rejected loudly elsewhere
                    // (`DegenerateFace`); not this gate's job.
                    return None;
                }
                let n = [na[0] / len, na[1] / len, na[2] / len];
                let mut lo = [f64::INFINITY; 3];
                let mut hi = [f64::NEG_INFINITY; 3];
                for lp in std::iter::once(&f.outer_loop).chain(f.inner_loops.iter()) {
                    for &e in lp {
                        let Some(edge) = brep.edges().get(e as usize) else {
                            continue;
                        };
                        for vi in [edge.start, edge.end] {
                            let Some(v) = brep.vertices().get(vi as usize) else {
                                continue;
                            };
                            let p = v.point.as_array();
                            for k in 0..3 {
                                lo[k] = lo[k].min(p[k]);
                                hi[k] = hi[k].max(p[k]);
                            }
                        }
                        // A `Circle`/`Ellipse` loop edge's endpoints are only
                        // its seam — the swept curve reaches much further. A
                        // disc cap bounded by a single closed circle would
                        // otherwise get a single-POINT AABB (the seam), so a
                        // coplanar disc∩polygon pair is detected only when the
                        // seam happens to overlap the other face. Expand by the
                        // analytic circle box: `center ± r·√(1−n_k²)` per axis.
                        if let Curve::Circle {
                            center,
                            normal,
                            radius,
                        } = edge.curve
                        {
                            let c = center.as_array();
                            let nu = normalize3(normal.as_array());
                            for k in 0..3 {
                                let ext = radius * (1.0 - nu[k] * nu[k]).max(0.0).sqrt();
                                lo[k] = lo[k].min(c[k] - ext);
                                hi[k] = hi[k].max(c[k] + ext);
                            }
                        }
                    }
                }
                if !lo[0].is_finite() {
                    return None;
                }
                Some(FacePlane {
                    n,
                    d: d / len,
                    lo,
                    hi,
                    raw_bits: [
                        na[0].to_bits(),
                        na[1].to_bits(),
                        na[2].to_bits(),
                        d.to_bits(),
                    ],
                })
            })
            .collect()
    }

    /// Conditions 1 (offset agreement) + 2 (parallel normals) for one face
    /// pair; returns the pair's `(band, gap, sub_resolution)` when both
    /// hold, where `gap` is the orientation-aligned offset separation
    /// `|d̂a − s·d̂b|` and `sub_resolution` classifies it against the
    /// coincidence-authoring noise line `band/100` (#178). Condition 3 (which
    /// AABBs must overlap) differs between cross and intra pairs — see the
    /// scan loops below.
    fn near_coplanar_band(pa: &FacePlane, pb: &FacePlane) -> Option<(f64, f64, bool)> {
        // scale = max |coordinate| over both faces' AABB corners.
        let mut scale: f64 = 0.0;
        for p in [&pa.lo, &pa.hi, &pb.lo, &pb.hi] {
            for &c in p.iter() {
                scale = scale.max(c.abs());
            }
        }
        let band = cad_primitives::TAU_MODEL.max(scale * cad_primitives::TAU_WORK);

        // 1. Orientation-aligned offset agreement.
        let dot = pa.n[0] * pb.n[0] + pa.n[1] * pb.n[1] + pa.n[2] * pb.n[2];
        let s = if dot >= 0.0 { 1.0 } else { -1.0 };
        let gap = (pa.d - s * pb.d).abs();
        if gap > band {
            return None;
        }

        // 2. Parallel normals over the pair's geometric extent.
        let cross = [
            pa.n[1] * pb.n[2] - pa.n[2] * pb.n[1],
            pa.n[2] * pb.n[0] - pa.n[0] * pb.n[2],
            pa.n[0] * pb.n[1] - pa.n[1] * pb.n[0],
        ];
        let sin = (cross[0] * cross[0] + cross[1] * cross[1] + cross[2] * cross[2]).sqrt();
        let mut ext2 = 0.0;
        for k in 0..3 {
            let e = pa.hi[k].max(pb.hi[k]) - pa.lo[k].min(pb.lo[k]);
            ext2 += e * e;
        }
        if sin * ext2.sqrt() > band {
            return None;
        }
        // #178: gap above the coincidence-authoring noise class = two
        // DISTINCT model planes (a sub-resolution feature), not one plane
        // authored twice imprecisely. The line is 1% of the pair's own
        // detection band (absolute floor TAU_MODEL/100 = 1e-9), calibrated
        // by the measured populations on both sides: intended-coincident
        // pairs arrive with gaps ≤ 2.7e-12 (corpus chained femto twins,
        // max at scale ≈ 4944) and ≤ 2.235e-10 (the REAL mm-scale
        // bearing-recess producer residual, `bearing_recess_mm_regression`
        // — a `TAU_WORK·(1+scale)` line was refuted by exactly that
        // fixture); designed sub-resolution features sit at 1e-8 (C0111)
        // and TAU_MODEL (C0113), 10–100× above the line.
        let sub_resolution = gap > band / 100.0;
        Some((band, gap, sub_resolution))
    }

    /// Band-inflated AABB overlap on every axis.
    fn aabbs_overlap(
        lo_a: &[f64; 3],
        hi_a: &[f64; 3],
        lo_b: &[f64; 3],
        hi_b: &[f64; 3],
        band: f64,
    ) -> bool {
        (0..3).all(|k| lo_a[k] <= hi_b[k] + band && lo_b[k] <= hi_a[k] + band)
    }

    /// Whole-solid AABB over all B-Rep vertices (None for an empty solid).
    fn solid_aabb(brep: &BRep) -> Option<([f64; 3], [f64; 3])> {
        let mut lo = [f64::INFINITY; 3];
        let mut hi = [f64::NEG_INFINITY; 3];
        for v in brep.vertices() {
            let p = v.point.as_array();
            for k in 0..3 {
                lo[k] = lo[k].min(p[k]);
                hi[k] = hi[k].max(p[k]);
            }
        }
        lo[0].is_finite().then_some((lo, hi))
    }

    let fa = collect(a);
    let fb = collect(b);

    // Cross pairs (A×B): bit-exact AND near-coplanar both flag; condition 3
    // is mutual AABB overlap (the two faces must be able to interact).
    // PR-YR26: collect ALL such pairs (Stage 0 overlays each), not just the
    // first.
    let mut cross: Vec<CrossCoplanarPair> = Vec::new();
    for (ia, pa) in fa.iter().enumerate() {
        let Some(pa) = pa else { continue };
        for (ib, pb) in fb.iter().enumerate() {
            let Some(pb) = pb else { continue };
            if let Some((band, gap, sub_resolution)) = near_coplanar_band(pa, pb) {
                if aabbs_overlap(&pa.lo, &pa.hi, &pb.lo, &pb.hi, band) {
                    cross.push(CrossCoplanarPair {
                        face_a: ia,
                        face_b: ib,
                        band,
                        gap,
                        sub_resolution,
                    });
                }
            }
        }
    }

    // Intra-solid pairs (A×A, B×B): only near-but-NOT-bit-identical planes
    // flag (bit-identical = one plane split into several faces, benign).
    // Condition 3 is different: the fragments are typically DISJOINT
    // in-plane regions, so the danger is contact by the OTHER solid —
    // flagged iff the other solid's whole-solid AABB overlaps EITHER
    // fragment (see the function docs for the F0018/F0025 evidence and the
    // weighed over-deferral).
    let mut intra: Option<(InputId, usize, usize)> = None;
    'intra: for (input, fp, other) in [
        (InputId::A, &fa, solid_aabb(b)),
        (InputId::B, &fb, solid_aabb(a)),
    ] {
        let Some((olo, ohi)) = other else { continue };
        for (i, pi) in fp.iter().enumerate() {
            let Some(pi) = pi else { continue };
            for (j, pj) in fp.iter().enumerate().skip(i + 1) {
                let Some(pj) = pj else { continue };
                if pi.raw_bits == pj.raw_bits {
                    continue;
                }
                // Spec `m8_intra_opposite_plane_canonicalization` B6: raw
                // plane values that are EXACTLY negated (f64 VALUE compare,
                // so `0.0 == -0.0` matches — bit compare would not) are two
                // orientations of ONE geometric plane. A valid 2-manifold
                // solid's faces on one plane are disjoint in-plane (a
                // stepped solid: lower-step top + overhang bottom), so the
                // arrangement needs no Stage-0 resolution — benign, like
                // the bit-identical case above. `to_yang_brep`'s sign-aware
                // sibling canonicalization produces exactly this form for
                // chained outputs; near-but-NOT-exact negation still walls
                // loud below (B7).
                if (0..4).all(|k| f64::from_bits(pi.raw_bits[k]) == -f64::from_bits(pj.raw_bits[k]))
                {
                    continue;
                }
                if let Some((band, _gap, _sub_resolution)) = near_coplanar_band(pi, pj) {
                    // (A PR-KV6b attempt narrowed this to ADJACENT fragments;
                    // it regressed F0017–F0025 from the typed M8 deferral
                    // into NoExplicitRayOrigin failures — the conservative
                    // rule stands. The benign exactly-coplanar class — a
                    // 180° revolve's two caps — is excluded by the
                    // bit-identical rule above instead: producers SNAP their
                    // trig so exact-π caps carry bitwise-equal planes.)
                    if aabbs_overlap(&pi.lo, &pi.hi, &olo, &ohi, band)
                        || aabbs_overlap(&pj.lo, &pj.hi, &olo, &ohi, band)
                    {
                        intra = Some((input, i, j));
                        break 'intra;
                    }
                }
            }
        }
    }
    CoplanarScan { cross, intra }
}
