//! The `boolean()` driver — PR-YR3 vertex provenance, PR-YR4 triangle
//! attribution, Stage-0 coplanar scan glue, KV15 near-weld, phantom
//! rim N, rim-junction overrides (extracted verbatim from lib.rs —
//! spec `specs/yang_rs_lib_decomposition.md`, increment 9).

#[allow(clippy::wildcard_imports)]
use crate::*;

// =========================================================================
// boolean() — PR-YR3 vertex provenance + PR-YR4 triangle attribution
// =========================================================================

/// Per-op orientation fix for a kept arrangement triangle, mirroring
/// Cherchi's `booleans.cpp` post-keep flip loops:
/// - Union (`boolUnion`) / Intersection (`boolIntersection`): no flip.
/// - Subtraction (`boolSubtraction`:1480-1483): flip kept tris NOT on
///   solid A's surface (`surface[t][0] != 1`) — the B-surface tris that
///   bound the carved cavity, whose outward normal must point into A.
/// - Xor (`boolXOR`:1506-1509): flip kept tris with any inside bit set
///   (`inside.count() > 0`).
pub(crate) fn flip_for_op(op: BoolOp, la: &LabeledArrangement, t: usize) -> bool {
    match op {
        BoolOp::Union | BoolOp::Intersect => false,
        BoolOp::Subtract => {
            // surface[t][0] set ⟺ solid 0 (A) is in the surface label list.
            let on_a = la.surface[t].iter().any(|&LaInputId(id)| id == 0);
            !on_a
        }
        BoolOp::Xor => la.inside[t].iter().any(|&b| b),
    }
}

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
    /// pair; returns the pair's `band` when both hold. Condition 3 (which
    /// AABBs must overlap) differs between cross and intra pairs — see the
    /// scan loops below.
    fn near_coplanar_band(pa: &FacePlane, pb: &FacePlane) -> Option<f64> {
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
        if (pa.d - s * pb.d).abs() > band {
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
        Some(band)
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
            if let Some(band) = near_coplanar_band(pa, pb) {
                if aabbs_overlap(&pa.lo, &pa.hi, &pb.lo, &pb.hi, band) {
                    cross.push(CrossCoplanarPair {
                        face_a: ia,
                        face_b: ib,
                        band,
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
                if let Some(band) = near_coplanar_band(pi, pj) {
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

/// PR-YR27 (Finding 3): finite-extent STRICT containment — is `p` strictly
/// inside planar face `fi`'s trimmed region (outer loop minus holes) of
/// `brep`, tested EXACTLY in the face's 2D plane frame?
///
/// Verdicts:
/// - `Some(true)`  — strictly interior: inside the loop arrangement
///   (even-odd over outer + holes) and ON no loop edge,
/// - `Some(false)` — ON a loop edge, or outside,
/// - `None`        — undecidable by this test (curved surface, a curved
///   loop edge — whose chord segment would misrepresent the boundary —
///   or non-finite coordinates). The caller must NOT exclude the face.
///
/// Exactness: the 2D projection `(u, v) = (q·e1, q·e2)` is one LINEAR map
/// applied in f64 and lifted exactly to rationals, so points that are
/// 3D-collinear along a straight loop edge project to EXACTLY 2D-collinear
/// points — the on-boundary rejection cannot be defeated by femto rounding.
/// Loop-vertex off-plane residuals (e.g. a Stage-0 snapped pair face) lie
/// along the face normal, which both frame axes annihilate, so they do not
/// perturb the in-plane region shape.
/// PR-KV7: finite-extent strict containment for a CYLINDER face, along the
/// AXIS only. A chainable boolean output can carry several faces of the SAME
/// infinite cylinder (the two stubs of a drill-through), so the YR27
/// infinite-surface membership ties between them; the axial span breaks the
/// tie exactly like the planar 2D test: the TRUE owning face's loop vertices
/// (rims / arc endpoints / ruling ends — all exactly on the surface) bound an
/// axial interval that strictly contains the centroid of every positive-area
/// triangle attributed to it, while a different same-cylinder face at best
/// touches the boundary. Azimuthal extent is NOT tested: a false candidate
/// that ties axially merely keeps the tie loud (P9-safe), never mis-excludes
/// the owner. `None` for non-cylinder faces / degenerate axes.
pub(crate) fn point_strictly_in_cylinder_face_axially(
    brep: &BRep,
    fi: usize,
    p: [f64; 3],
) -> Option<bool> {
    let f = brep.faces().get(fi)?;
    let Surface::Cylinder {
        axis_point,
        axis_dir,
        ..
    } = f.surface
    else {
        return None;
    };
    let a = normalize3(axis_dir.as_array());
    let ap = axis_point.as_array();
    let t_of = |q: [f64; 3]| (q[0] - ap[0]) * a[0] + (q[1] - ap[1]) * a[1] + (q[2] - ap[2]) * a[2];
    let mut t_min = f64::INFINITY;
    let mut t_max = f64::NEG_INFINITY;
    for e_idx in f.outer_loop.iter().chain(f.inner_loops.iter().flatten()) {
        let e = brep.edges().get(*e_idx as usize)?;
        for v in [e.start, e.end] {
            let t = t_of(brep.vertices().get(v as usize)?.point.as_array());
            t_min = t_min.min(t);
            t_max = t_max.max(t);
        }
    }
    if !(t_min.is_finite() && t_max.is_finite() && t_min < t_max) {
        return None;
    }
    let t = t_of(p);
    Some(t_min < t && t < t_max)
}

pub(crate) fn point_strictly_in_planar_face(brep: &BRep, fi: usize, p: [f64; 3]) -> Option<bool> {
    use crate::coplanar_overlay::{cross_r, point_in_even_odd, ExactPoint2};
    use dashu::rational::RBig;

    let f = brep.faces().get(fi)?;
    let Surface::Plane { normal, .. } = f.surface else {
        return None;
    };
    let n = normal.as_array();
    if (n[0] * n[0] + n[1] * n[1] + n[2] * n[2]).sqrt() < cad_primitives::MIN_FEATURE_SIZE {
        return None;
    }
    let (e1, e2) = ortho_basis(normal);
    let (e1, e2) = (e1.as_array(), e2.as_array());
    let proj = |q: [f64; 3]| -> Option<ExactPoint2> {
        ExactPoint2::from_f64(
            q[0] * e1[0] + q[1] * e1[1] + q[2] * e1[2],
            q[0] * e2[0] + q[1] * e2[1] + q[2] * e2[2],
        )
    };
    let q = proj(p)?;

    let mut edges2: Vec<(ExactPoint2, ExactPoint2)> = Vec::new();
    for lp in std::iter::once(&f.outer_loop).chain(f.inner_loops.iter()) {
        for &ei in lp {
            let edge = brep.edges().get(ei as usize)?;
            // A curved loop edge's chord would misrepresent the trimmed
            // boundary — undecidable, never a silent approximation.
            if !matches!(edge.curve, Curve::LineSegment) {
                return None;
            }
            let s = brep.vertices().get(edge.start as usize)?.point.as_array();
            let e = brep.vertices().get(edge.end as usize)?.point.as_array();
            edges2.push((proj(s)?, proj(e)?));
        }
    }

    // Exact ON-closed-segment rejection against every loop edge (strictness:
    // a boundary point is NOT contained).
    for (a, b) in &edges2 {
        if cross_r(a, b, &q) != RBig::ZERO {
            continue;
        }
        let dx = &b.x - &a.x;
        let dy = &b.y - &a.y;
        let t_num = (&q.x - &a.x) * &dx + (&q.y - &a.y) * &dy;
        let len2 = &dx * &dx + &dy * &dy;
        if t_num >= RBig::ZERO && t_num <= len2 {
            return Some(false);
        }
    }

    // Strictly off the boundary: exact even-odd over outer + hole loops
    // (the no-boundary precondition of `point_in_even_odd` now holds).
    Some(point_in_even_odd(&q, &edges2))
}

/// Surface distance of a point `c` to a coincident-cylinder pair, namely the
/// value `abs(dist_to_axis_line minus radius)`, which is zero on the shared
/// cylindrical surface. Used by the membrane resolution to match an
/// overlap-sheet triangle to a [`stage0::PairCylinder`] (the cylinder analog of
/// the planar plane-distance match).
pub(crate) fn centroid_on_cylinder(c: [f64; 3], p: &stage0::PairCylinder) -> f64 {
    let w = [
        c[0] - p.axis_point[0],
        c[1] - p.axis_point[1],
        c[2] - p.axis_point[2],
    ];
    let t = w[0] * p.axis_dir[0] + w[1] * p.axis_dir[1] + w[2] * p.axis_dir[2];
    let perp = [
        w[0] - t * p.axis_dir[0],
        w[1] - t * p.axis_dir[1],
        w[2] - t * p.axis_dir[2],
    ];
    let dist = (perp[0] * perp[0] + perp[1] * perp[1] + perp[2] * perp[2]).sqrt();
    (dist - p.radius).abs()
}

/// PR-5: are `surf0` and `surf1` COINCIDENT cylinders — same axis line
/// (parallel axes, collinear) and equal radius, all within `tol`? Two such
/// cylinders share their entire lateral surface and `ssi_rs::intersect` refuses
/// them (`DegenerateInput`), so the caller must NOT route their edges to SSI.
pub(crate) fn cylinders_are_coincident(surf0: Surface, surf1: Surface, tol: f64) -> bool {
    let (
        Surface::Cylinder {
            axis_point: ap0,
            axis_dir: ad0,
            radius: r0,
        },
        Surface::Cylinder {
            axis_point: ap1,
            axis_dir: ad1,
            radius: r1,
        },
    ) = (surf0, surf1)
    else {
        return false;
    };
    let ad0 = normalize3(ad0.as_array());
    let ad1 = normalize3(ad1.as_array());
    // Parallel axes (|cross| ≈ 0).
    let cross = [
        ad0[1] * ad1[2] - ad0[2] * ad1[1],
        ad0[2] * ad1[0] - ad0[0] * ad1[2],
        ad0[0] * ad1[1] - ad0[1] * ad1[0],
    ];
    let sin = (cross[0] * cross[0] + cross[1] * cross[1] + cross[2] * cross[2]).sqrt();
    if sin > tol.max(cad_primitives::TAU_MODEL) {
        return false;
    }
    // Equal radius.
    if (r0 - r1).abs() > tol {
        return false;
    }
    // Collinear axes: ap1 lies on ap0's axis line (perpendicular distance ≈ 0).
    let ap0a = ap0.as_array();
    let ap1a = ap1.as_array();
    let w = [ap1a[0] - ap0a[0], ap1a[1] - ap0a[1], ap1a[2] - ap0a[2]];
    let tw = w[0] * ad0[0] + w[1] * ad0[1] + w[2] * ad0[2];
    let perp = [w[0] - tw * ad0[0], w[1] - tw * ad0[1], w[2] - tw * ad0[2]];
    (perp[0] * perp[0] + perp[1] * perp[1] + perp[2] * perp[2]).sqrt() <= tol
}

/// Boolean operation on two B-Rep solids via a `MeshBoolean` backend.
///
/// **M3 functional pipeline** (replaces the PR-YR3/YR4 spatial-match +
/// majority-vote substitute, now a `#[cfg(test)]` differential oracle):
///
/// 0. **XOR is deferred (spec §Scope)** — its symmetric-difference result
///    is multi-shell / has a void that `reconstruct_topology` cannot
///    reassemble yet. `boolean()` errors loudly with `UnsupportedOp` once it
///    sees a non-empty XOR kept-set (a degenerate XOR with nothing to
///    reassemble still trivially yields an empty result).
/// 1. Obtain the real Stage-2 [`LabeledArrangement`] from
///    `backend.labeled_arrangement(..)` (full arrangement mesh +
///    per-triangle `surface`/`inside`/`patch` labels).
/// 2. **I6 weld** — the C++ producer does NOT always weld coincident
///    vertices (e.g. A@[0,0,0]/B@[0.7,0.3,0.4] emits a bit-exact duplicate
///    vertex used by shared triangles), so yang welds: map each vertex to
///    the *original index* of its first bit-identical occurrence. yang's
///    index-based adjacency then sees coincident points as one index. A
///    kept triangle that welds to a repeated index is a zero-area sliver at
///    that coincident point — dropped (no surface/volume; its edges pair up
///    so the output stays watertight). Two *distinct* surviving triangles
///    that weld to the same 3 indices are genuinely coincident faces →
///    `NonManifoldInput` (the a4 bit-exact-coincident-vertex case).
/// 3. `keep = la.keep_set(op)` — Stage 4 face survival.
/// 4. Compact the welded kept tris into a fresh sub-mesh (the output mesh).
/// 5. **Geometric face resolution** (Stage 6) per kept tri → a FULL
///    `TriangleAttributionMap` (every entry `Some`). A SURVIVING
///    multi-solid `surface[t]` (a §4.5.5 overlap-sheet triangle the (3b)
///    side rule kept) attributes to input A — the dedup survivor's side,
///    whose winding it carries (PR-YR26; B's coincident face has the same
///    plane, so the inherited output surface is identical). For a
///    *non-degenerate* (positive-area) triangle: pick the unique labeled-solid
///    face plane within `TAU_WORK` of the centroid; no match / a genuine tie →
///    `FaceResolutionFailed` (F3). For a *degenerate* (zero-area sliver, kept
///    because its edges pair into the watertight result) triangle: attribute
///    to the LOWEST labeled-solid face index within `TAU_WORK` (its centroid
///    sits on a solid edge, so the two adjacent planes tie — harmless for a
///    zero-area tri; never F3). Never a silent `None` (P9).
/// 6. `reconstruct_topology(..)` — flood-fill patches, walk boundary
///    cycles, inherit input-face `Surface`; full attribution ⇒ closed
///    boundary cycles ⇒ watertight 2-manifold output.
///
/// **N4 (provenance):** before the geometric resolution in step 5, a kept
/// triangle is attributed DIRECTLY from cherchi's per-triangle provenance
/// (`LabeledArrangement.source` → the parent input triangle → its B-Rep face via
/// the Stage-1 `tri_face` map) whenever that is unambiguous. The geometric path
/// remains the fallback. See [`provenance_face_reason`].
///
/// N4 helper: resolve a kept arrangement triangle's B-Rep face from cherchi's
/// per-triangle provenance (`§4.2.3`), not geometric centroid-proximity.
///
/// The triangle is attributed to `surface_input` (A or B — the side the keep-rule
/// kept it on; for a coplanar overlap sheet the §4.5.5 survivor convention picks
/// A). We select that side's parent from `source` and resolve it through that
/// input mesh's per-triangle face map (`tri_face_a` for A, `tri_face_b` for B).
/// This handles BOTH a non-coplanar triangle (its only parent) AND a coplanar
/// overlap sheet (the parent on the kept side). Returns `None` (→ geometric
/// fallback) when that side has no parent in `source`, the parent is beyond
/// the face map (a Stage-0 path that did not emit provenance, or a lineage-less
/// `from_mesh` / boolean-output input), or the parent maps to the `u32::MAX`
/// sentinel (a producer that emitted a map but could not attribute THAT
/// triangle — e.g. a coincident-cylinder band-strip column with no covering
/// arc-patch face). Never a wrong face.
/// Why N4 provenance attribution could not name a face for a kept triangle —
/// the exact reason the Stage-6 geometric fallback is still reached. Used by the
/// `YANG_N4_FALLBACK_PROBE` measurement (N4 retirement: prove the geometric path
/// is dead in production, or name the producers that still leave a triangle
/// un-provenanced).
#[derive(Debug, Clone, Copy)]
pub(crate) enum ProvMiss {
    /// The kept triangle's `source` has no parent triangle from this input
    /// (e.g. a cut/arrangement triangle with only the OTHER input's lineage).
    /// On a lineage-carrying input this is a producer FAULT (loud).
    NoSourceEntry,
    /// This input emitted NO provenance map at all (empty `tri_face`) — a
    /// LINEAGE-LESS input: a yang boolean OUTPUT chained directly back in,
    /// or a `from_mesh` B-Rep. This is the documented geometric-resolution
    /// path (task #53), NOT a fault.
    NoLineage,
    /// The map is present but the parent-triangle index lies beyond it —
    /// the producer emitted a TOO-SHORT provenance map (fault, loud).
    NoMap,
    /// The producer minted this triangle but could not attribute it to a face
    /// (`u32::MAX` sentinel — e.g. the coincident-cylinder band strip with no
    /// covering arc column). Fault, loud.
    Sentinel,
}

/// N4 (§4.2.3): map a kept triangle to its owning B-Rep face via the
/// arrangement's per-triangle provenance. `Ok(face)` on a hit; `Err(reason)`
/// records WHY it missed — `NoLineage` is the one non-fault reason (the
/// input never had a provenance map), everything else is loud at the caller
/// (task #53, spec `specs/n4_retire_stage6_fallback.md`).
pub(crate) fn provenance_face_reason(
    source: &[(LaInputId, u32)],
    surface_input: InputId,
    tri_face_a: &[u32],
    tri_face_b: &[u32],
) -> Result<u32, ProvMiss> {
    let (want_k, tf): (u32, &[u32]) = match surface_input {
        InputId::A => (0, tri_face_a),
        InputId::B => (1, tri_face_b),
    };
    if tf.is_empty() {
        return Err(ProvMiss::NoLineage);
    }
    let &(_, local) = source
        .iter()
        .find(|&&(LaInputId(k), _)| k == want_k)
        .ok_or(ProvMiss::NoSourceEntry)?;
    match tf.get(local as usize).copied() {
        None => Err(ProvMiss::NoMap),
        Some(f) if f == u32::MAX => Err(ProvMiss::Sentinel),
        Some(f) => Ok(f),
    }
}

/// KV15 (spec `kv15_mixed_operand_planar_near_weld` §3): per-vertex weld
/// eligibility for MIXED operands. A vertex is CURVED-ADJACENT (ineligible
/// for the near-weld, `true` in the returned vec) when ANY incident
/// arrangement triangle fails to prove planar descent: empty provenance
/// (`source[t]` empty — e.g. the sidecar parity producer, spec W4),
/// out-of-range / `u32::MAX`-sentinel `tri_face` entries, an out-of-range
/// face index, or a face whose surface is not `Surface::Plane`
/// (`face_planar` returns `Some(false)` — or `None` for a bad index).
/// Conservative by construction: only positively-proven all-planar descent
/// yields eligibility.
pub(crate) fn kv15_curved_touch(
    n_verts: usize,
    tris: &[[u32; 3]],
    source: &[Vec<(LaInputId, u32)>],
    tri_face_a: &[u32],
    tri_face_b: &[u32],
    face_planar: impl Fn(u32, u32) -> Option<bool>,
) -> Vec<bool> {
    let mut curved = vec![false; n_verts];
    for (t, tri) in tris.iter().enumerate() {
        let src = source.get(t).map(Vec::as_slice).unwrap_or(&[]);
        let tri_curved = src.is_empty()
            || src.iter().any(|&(LaInputId(k), local)| {
                let tf: &[u32] = if k == 0 { tri_face_a } else { tri_face_b };
                match tf.get(local as usize).copied() {
                    Some(fi) if fi != u32::MAX => !matches!(face_planar(k, fi), Some(true)),
                    _ => true,
                }
            });
        if tri_curved {
            for &v in tri {
                if let Some(slot) = curved.get_mut(v as usize) {
                    *slot = true;
                }
            }
        }
    }
    curved
}

/// KV15 (spec §3): near-union among planar-only weld roots — the identical
/// grid, per-pair band `TAU_WORK·(1+max|coord|)`, and min-index-survivor
/// rule as the all-planar KV10 weld (spec I2/I4). `weld` enters as the
/// bit-exact weld map (each entry pointing at its cluster's original
/// representative) and leaves fully resolved. Roots flagged in
/// `root_curved` never participate (kv9 junction-duplicate protection).
pub(crate) fn kv15_near_weld_pass(verts: &[Point3], weld: &mut [u32], root_curved: &[bool]) {
    use std::collections::HashMap;
    let mut parent: Vec<u32> = weld.to_vec();
    fn find(parent: &mut [u32], mut x: u32) -> u32 {
        while parent[x as usize] != x {
            parent[x as usize] = parent[parent[x as usize] as usize];
            x = parent[x as usize];
        }
        x
    }
    let scale = verts
        .iter()
        .flat_map(|v| v.as_array())
        .fold(0.0f64, |m, c| m.max(c.abs()));
    let band = cad_primitives::TAU_WORK * (1.0 + scale);
    let cell = |c: f64| -> i64 { (c / band).floor() as i64 };
    let mut grid: HashMap<[i64; 3], Vec<u32>> = HashMap::new();
    for i in 0..verts.len() as u32 {
        if weld[i as usize] != i || root_curved[i as usize] {
            continue;
        }
        let p = verts[i as usize].as_array();
        let key = [cell(p[0]), cell(p[1]), cell(p[2])];
        for dx in -1..=1i64 {
            for dy in -1..=1i64 {
                for dz in -1..=1i64 {
                    let Some(occ) = grid.get(&[key[0] + dx, key[1] + dy, key[2] + dz]) else {
                        continue;
                    };
                    for &j in occ {
                        let q = verts[j as usize].as_array();
                        let pair_band = cad_primitives::TAU_WORK
                            * (1.0 + p.iter().chain(q.iter()).fold(0.0f64, |m, c| m.max(c.abs())));
                        if (0..3).all(|k| (p[k] - q[k]).abs() <= pair_band) {
                            let (ri, rj) = (find(&mut parent, i), find(&mut parent, j));
                            if ri != rj {
                                parent[ri.max(rj) as usize] = ri.min(rj);
                            }
                        }
                    }
                }
            }
        }
        grid.entry(key).or_default().push(i);
    }
    for w in weld.iter_mut() {
        *w = find(&mut parent, *w);
    }
}

/// M8 Stage-0 operand dump — diagnostic-only observer (spec
/// `specs/m8_stage0_inputcheck_clean_emission.md` §6). Env-gated on
/// `YANG_STAGE0_DUMP_DIR`; zero-cost when unset (never set in production or
/// WASM). Writes, per boolean call, the EXACT operand meshes handed to the
/// backend — plus, when Stage 0 rewrote them, each solid's pre-Stage-0
/// Stage-1 mesh (`_pre`) and the `tri_face` provenance maps — so the
/// five-axiom census can split defects introduced-vs-inherited and join
/// offenders back to B-Rep faces. Vertex coordinates use f64 `Display`
/// (shortest round-trip), so the dump is bit-faithful. Write failures are
/// reported on stderr and never affect the boolean (read-only, spec I6).
pub(crate) fn stage0_dump(
    op: BoolOp,
    stage0: Option<&stage0::Stage0>,
    cyl_pair_count: usize,
    mesh_a: &Mesh,
    mesh_b: &Mesh,
    pre_a: &Mesh,
    pre_b: &Mesh,
) {
    let Some(dir) = std::env::var_os("YANG_STAGE0_DUMP_DIR") else {
        return;
    };
    use std::sync::atomic::{AtomicU64, Ordering};
    // Process-global op counter: yang-rs has no case identity; harnesses
    // namespace by pointing the env var at a per-case directory.
    static OP_COUNTER: AtomicU64 = AtomicU64::new(0);
    let n = OP_COUNTER.fetch_add(1, Ordering::Relaxed);
    let dir = std::path::PathBuf::from(dir);
    if let Err(e) = std::fs::create_dir_all(&dir) {
        eprintln!(
            "[stage0-dump] create_dir_all({}) failed: {e}",
            dir.display()
        );
        return;
    }
    let op_name = match op {
        BoolOp::Union => "union",
        BoolOp::Intersect => "intersect",
        BoolOp::Subtract => "subtract",
        BoolOp::Xor => "xor",
    };
    let stem = format!("{n:03}_{op_name}");
    let write_obj = |suffix: &str, m: &Mesh| {
        let path = dir.join(format!("{stem}_{suffix}.obj"));
        let mut out = String::new();
        for v in &m.verts {
            out.push_str(&format!("v {} {} {}\n", v.x(), v.y(), v.z()));
        }
        for t in &m.tris {
            out.push_str(&format!("f {} {} {}\n", t[0] + 1, t[1] + 1, t[2] + 1));
        }
        if let Err(e) = std::fs::write(&path, out) {
            eprintln!("[stage0-dump] write {} failed: {e}", path.display());
        }
    };
    write_obj("a", mesh_a);
    write_obj("b", mesh_b);
    let mut meta = format!(
        "op: {op_name}\nstage0: {}\ncyl_pairs: {cyl_pair_count}\n\
         mesh_a: {} verts / {} tris\nmesh_b: {} verts / {} tris\n",
        stage0.is_some(),
        mesh_a.verts.len(),
        mesh_a.tris.len(),
        mesh_b.verts.len(),
        mesh_b.tris.len(),
    );
    if let Some(s0) = stage0 {
        write_obj("a_pre", pre_a);
        write_obj("b_pre", pre_b);
        let write_csv = |suffix: &str, tf: &[u32]| {
            let path = dir.join(format!("{stem}_{suffix}.tri_face.csv"));
            let mut out = String::new();
            for f in tf {
                out.push_str(&format!("{f}\n"));
            }
            if let Err(e) = std::fs::write(&path, out) {
                eprintln!("[stage0-dump] write {} failed: {e}", path.display());
            }
        };
        write_csv("a", &s0.tri_face_a);
        write_csv("b", &s0.tri_face_b);
        for p in &s0.pairs {
            meta.push_str(&format!(
                "pair_plane: face_a={} face_b={} opposite={} n=({},{},{}) d={} band={}\n",
                p.face_a, p.face_b, p.opposite, p.n[0], p.n[1], p.n[2], p.d, p.band,
            ));
        }
    }
    let meta_path = dir.join(format!("{stem}_meta.txt"));
    if let Err(e) = std::fs::write(&meta_path, meta.as_bytes()) {
        eprintln!("[stage0-dump] write {} failed: {e}", meta_path.display());
    }
}

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
    let natural_rim_n = |brep: &BRep| -> usize {
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
    };
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

pub fn boolean(
    a: &BRep,
    b: &BRep,
    op: BoolOp,
    backend: &dyn MeshBoolean,
) -> Result<BRep, YangError> {
    // Run separator for env-gated probe streams: which boolean call a probe
    // line belongs to (multi-op corpus cases interleave several runs).
    if std::env::var_os("YANG_RUN_PROBE").is_some() {
        eprintln!(
            "[yang-run] op={op:?} a: {}v/{}f b: {}v/{}f",
            a.vertices().len(),
            a.faces().len(),
            b.vertices().len(),
            b.faces().len()
        );
    }
    // Case-IV phantom guard (spec `yang_case_iv_phantom_guard`): rebuild
    // both operands at the pair-derived rim density BEFORE any Stage-0/1
    // machinery samples their meshes, so analytically-disjoint cylinder
    // pairs cannot mesh-intersect. `None` (no cylinder faces, e.g. the
    // `from_mesh` chained-output operand, or no disjoint pair demanding
    // more than each solid's own N) leaves both operands byte-identical.
    let boosted: Option<(BRep, BRep)> = match phantom_min_rim_segments(a, b) {
        Some(n) => Some((
            a.rebuilt_with_min_rim_segments(n)?,
            b.rebuilt_with_min_rim_segments(n)?,
        )),
        None => None,
    };
    let (a, b): (&BRep, &BRep) = match &boosted {
        Some((ba, bb)) => (ba, bb),
        None => (a, b),
    };

    // (0) Stage 0 — §4.5.5 coplanar preprocessing (PR-YR26, M8 slice b).
    // Near-coplanar planar A×B face pairs are HANDLED: both faces snapped
    // onto one canonical shared plane, segmented by the exact 2D overlay,
    // and re-tessellated so the overlap region carries IDENTICAL meshes on
    // both solids (see `stage0::stage0_preprocess`). Unsupported residue
    // (intra-solid near pairs — the chained-output class — plus curved
    // faces in a multi-pair group and overlay failures) keeps the loud
    // typed PR-YR24 wall (`CoplanarFacesUnsupported`); multi-pair PLANAR
    // groups route through the n-ary overlay (`stage0::nary`, spec
    // `m8_plane_group_nary_overlay`).
    let stage0 = stage0::stage0_preprocess(a, b)?;
    // M8-cyl Increment 1 (§4.5.5 curved analog): when the planar scan found NO
    // cross pairs, a COINCIDENT-CYLINDER pair (the gear's bore wall ∩ a coaxial
    // flange/plug wall, opposite normal, full θ, one z-extent contained in the
    // other) gets a conformal re-tessellation so its overlap band is
    // bit-identical on BOTH solids' meshes. cherchi then pocket-dedups the band
    // into one multi-label sheet and the membrane resolution below drops it.
    // `task28_plug_in_bore` proved both native cherchi AND the C++ sidecar leave
    // this non-watertight WITHOUT this upstream conformal step. Only consulted
    // when the planar Stage-0 produced nothing (the two paths never overlap on a
    // single pair in Increment 1's scope).
    let stage0 = match stage0 {
        Some(s0) => Some(s0),
        None => stage0::coincident_cylinder_stage0(a, b)?,
    };
    // PR-5: coincident-CYLINDER A×B pairs (the membrane analog of the planar
    // `PairPlane`s in `stage0`). cherchi (coplanar PRs 1-4) constructs the
    // coincident-cylinder overlap with a MULTI-SOLID label exactly as it does a
    // coplanar planar overlap, but the Stage-0 planar scan records only
    // `Surface::Plane` pairs — so a coaxial-cylinder sheet (a flange outer wall
    // coincident with a gear bore, `err.waffle`) had no matching pair and was
    // dropped with `FaceResolutionFailed`. This parallel detector supplies the
    // keep/drop decision for those sheets. It does NOT touch the planar overlay
    // / mesh re-tessellation path (the coincident-cylinder meshes are already
    // bit-identical: both faces are the identical analytic cylinder).
    let cyl_pairs = stage0::detect_coincident_cylinder_pairs(a, b);

    // Increment 2 (spec `yang_rim_junction_insertion`): insert the exact
    // §4.3.3 Case-IV rim junction points as Stage-1 rim samples, so the
    // mesh-level seam chains can terminate exactly at the junctions (the
    // truncated-Steinmetz cap-lobe corners). SCOPE GATE (spec branch row
    // 3): only for a pair with NO Stage-0 interaction — the Stage-0
    // re-tessellation paths do not thread rim overrides yet (the M8
    // incr-15 pass-through trap), and skipping keeps them byte-identical.
    // Rim re-tessellation changes neither surfaces nor topology, so the
    // Stage-0 detectors' verdicts (computed above) remain valid for the
    // rebuilt operands.
    if std::env::var_os("YANG_RIM_JUNCTION_PROBE").is_some() {
        eprintln!(
            "[rim-junction] gate: stage0_none={} cyl_pairs_empty={}",
            stage0.is_none(),
            cyl_pairs.is_empty()
        );
    }
    let junction_boosted: Option<(BRep, BRep)> = if stage0.is_none()
        && cyl_pairs.is_empty()
        // Diagnostic kill-switch (read-only, env-gated): bisect whether a
        // downstream behavior change is enabled by the insertion.
        && std::env::var_os("YANG_RIM_JUNCTION_DISABLE").is_none()
    {
        let (map_a, map_b) = rim_junction_overrides(a, b);
        if map_a.is_empty() && map_b.is_empty() {
            None
        } else {
            if std::env::var_os("YANG_RIM_JUNCTION_PROBE").is_some() {
                eprintln!("[rim-junction] overrides a={map_a:?} b={map_b:?}");
            }
            Some((
                a.rebuilt_with_rim_overrides(&map_a)?,
                b.rebuilt_with_rim_overrides(&map_b)?,
            ))
        }
    } else {
        None
    };
    let (a, b): (&BRep, &BRep) = match &junction_boosted {
        Some((ba, bb)) => (ba, bb),
        None => (a, b),
    };

    // Twin-origin probe (read-only, env-gated): `YANG_INPUT_VERT_PROBE=x,y,z,r`
    // dumps every INPUT B-Rep vertex and every Stage-0/1 mesh vertex within
    // radius r of the target point, per operand — to establish whether a
    // downstream femto-twin pair arrives as two distinct input points
    // (chained-output drift) or is minted inside this boolean.
    if let Some(spec) = std::env::var_os("YANG_INPUT_VERT_PROBE") {
        let nums: Vec<f64> = spec
            .to_string_lossy()
            .split(',')
            .filter_map(|s| s.trim().parse().ok())
            .collect();
        if let [x, y, z, r] = nums[..] {
            let near = |p: &Point3| {
                let q = p.as_array();
                let d = [q[0] - x, q[1] - y, q[2] - z];
                (d[0] * d[0] + d[1] * d[1] + d[2] * d[2]).sqrt() <= r
            };
            for (tag, brep) in [("A", a), ("B", b)] {
                for (i, v) in brep.vertices().iter().enumerate() {
                    if near(&v.point) {
                        let q = v.point.as_array();
                        eprintln!(
                            "[input-vert-probe] input {tag} brep vert {i}: ({},{},{})",
                            q[0], q[1], q[2]
                        );
                    }
                }
            }
            if let Some(s0) = &stage0 {
                for (tag, m) in [("A", &s0.mesh_a), ("B", &s0.mesh_b)] {
                    for (i, v) in m.verts.iter().enumerate() {
                        if near(v) {
                            let q = v.as_array();
                            eprintln!(
                                "[input-vert-probe] stage0 mesh {tag} vert {i}: ({},{},{})",
                                q[0], q[1], q[2]
                            );
                        }
                    }
                }
            }
        }
    }
    let (mesh_a, mesh_b): (&Mesh, &Mesh) = match &stage0 {
        Some(s0) => (&s0.mesh_a, &s0.mesh_b),
        // No coplanar pairs: the B-Reps' own Stage-1 meshes — byte-for-byte
        // the pre-YR26 path.
        None => (a.as_mesh(), b.as_mesh()),
    };
    // M8 diagnostic operand dump (env-gated, read-only; spec
    // `m8_stage0_inputcheck_clean_emission` §6).
    stage0_dump(
        op,
        stage0.as_ref(),
        cyl_pairs.len(),
        mesh_a,
        mesh_b,
        a.as_mesh(),
        b.as_mesh(),
    );

    // (1) Stage 2: full labeled arrangement.
    let la = backend
        .labeled_arrangement(mesh_a, mesh_b)
        .map_err(YangError::MeshBooleanFailed)?;

    // (2) I6 weld: the C++ producer does NOT always weld coincident vertices
    // (it can emit two distinct indices at bit-identical coordinates — a
    // non-manifold touching point — used by shared triangles). yang's
    // index-based adjacency requires coincident points to share one index, so
    // weld each vertex to the ORIGINAL index of its first coincident
    // occurrence. (Mapping to the original index — not a renumbered counter —
    // keeps `la.mesh.verts[welded]` valid: coordinates are unchanged.)
    //
    // PR-KV10 (M8 residue): for ALL-PLANAR input pairs the weld is
    // NEAR-aware, not just bit-exact. The old "the producer never emits
    // TAU_WORK-near-but-bit-distinct coincident verts" assumption is FALSE
    // for chained planar inputs: an oblique solid's f64 vertices make
    // adjacent same-face tessellation triangles span femto-different EXACT
    // planes, so the exact arrangement legitimately mints distinct
    // intersection points ~1e-16·scale apart where several intersection
    // segments junction (one geometric point, several generating tri
    // pairs). Left distinct, the copies chain into sliver fans in the
    // output B-Rep and poison the NEXT boolean's attribution (the
    // F0016-class corpus residue's second layer — found behind the
    // intra-coplanar wall). Welding them within the scale-relative rounding
    // band `TAU_WORK·(1+|coord|)` is the same reconciliation principle as
    // the §4.5.5 Stage-0 snap; genuinely distinct model features are
    // ≥ MIN_FEATURE_SIZE apart — six orders beyond the band. Clusters weld
    // to their LOWEST member index (deterministic; survivor keeps its own
    // coordinates). Bucketed by a quantized grid with 27-neighborhood
    // probing + an EXACT per-pair band check — quantization alone aliases
    // (the KV8c lesson), so it only ever NOMINATES candidates, never
    // decides.
    //
    // CURVED inputs keep the bit-exact weld: the cyl×cyl pipeline expects
    // near-coincident-but-structurally-distinct vertices at ruling-line /
    // tangency junctions (one copy per incident surface's chord ring) and
    // reconciles them ITSELF in Stage-4 relocation with curve knowledge
    // (the KV9 junction duplicate collapse); welding them at step (2)
    // collapses lens-tip seam edges into degenerate (<3-edge) output loops
    // — found by kv9_cyl_cyl_special RED on the first attempt.
    // Per-triangle B-Rep face maps for the operand meshes — the inputs' OWN
    // Stage-1 `tri_face` when Stage 0 did not re-tessellate, else the Stage-0
    // re-tessellated meshes' maps. Consumed by the KV15 weld eligibility
    // below and by the Stage-6 N4 provenance attribution.
    let (tri_face_a, tri_face_b): (&[u32], &[u32]) = match &stage0 {
        Some(s0) => (&s0.tri_face_a, &s0.tri_face_b),
        None => (a.tri_face(), b.tri_face()),
    };
    let all_planar = a
        .faces()
        .iter()
        .chain(b.faces().iter())
        .all(|f| matches!(f.surface, Surface::Plane { .. }));
    let weld: Vec<u32> = if all_planar {
        use std::collections::HashMap;
        let verts = &la.mesh.verts;
        // Union-find over vertex indices (path-halving; union by min index
        // happens at the final resolution pass).
        let mut parent: Vec<u32> = (0..verts.len() as u32).collect();
        fn find(parent: &mut [u32], mut x: u32) -> u32 {
            while parent[x as usize] != x {
                parent[x as usize] = parent[parent[x as usize] as usize];
                x = parent[x as usize];
            }
            x
        }
        // Grid cell size: one band at the mesh's coordinate scale.
        let scale = verts
            .iter()
            .flat_map(|v| v.as_array())
            .fold(0.0f64, |m, c| m.max(c.abs()));
        let band = cad_primitives::TAU_WORK * (1.0 + scale);
        let cell = |c: f64| -> i64 { (c / band).floor() as i64 };
        let mut grid: HashMap<[i64; 3], Vec<u32>> = HashMap::with_capacity(verts.len());
        for (i, v) in verts.iter().enumerate() {
            let p = v.as_array();
            let key = [cell(p[0]), cell(p[1]), cell(p[2])];
            // Probe the 27-neighborhood for near-coincident occupants; the
            // EXACT pairwise band test decides. Union with EVERY in-band
            // occupant (a vertex can bridge two so-far-separate clusters).
            for dx in -1..=1i64 {
                for dy in -1..=1i64 {
                    for dz in -1..=1i64 {
                        let Some(occ) = grid.get(&[key[0] + dx, key[1] + dy, key[2] + dz]) else {
                            continue;
                        };
                        for &j in occ {
                            let q = verts[j as usize].as_array();
                            let pair_band = cad_primitives::TAU_WORK
                                * (1.0
                                    + p.iter().chain(q.iter()).fold(0.0f64, |m, c| m.max(c.abs())));
                            if (0..3).all(|k| (p[k] - q[k]).abs() <= pair_band) {
                                let (ri, rj) = (find(&mut parent, i as u32), find(&mut parent, j));
                                if ri != rj {
                                    // Root at the smaller index so the final
                                    // representative is the cluster minimum.
                                    parent[ri.max(rj) as usize] = ri.min(rj);
                                }
                            }
                        }
                    }
                }
            }
            grid.entry(key).or_default().push(i as u32);
        }
        (0..verts.len() as u32)
            .map(|i| find(&mut parent, i))
            .collect()
    } else {
        // Bit-exact weld (the pre-KV10 path, byte-identical for curved
        // pipelines): weld each vertex to the ORIGINAL index of its first
        // bit-identical occurrence.
        use std::collections::HashMap;
        let mut first: HashMap<[u64; 3], u32> = HashMap::with_capacity(la.mesh.verts.len());
        let mut weld: Vec<u32> = la
            .mesh
            .verts
            .iter()
            .enumerate()
            .map(|(i, v)| {
                let key = [v.x().to_bits(), v.y().to_bits(), v.z().to_bits()];
                *first.entry(key).or_insert(i as u32)
            })
            .collect();

        // KV15 (spec `kv15_mixed_operand_planar_near_weld` §3): per-vertex
        // planar near-weld for MIXED operands. The chained-extrude corpus
        // mints planar femto twins whose reconciliation is exactly the KV10
        // near-weld above — but one curved face ANYWHERE in either operand
        // used to drop the whole model to bit-exact, leaving the twins'
        // femto membrane to poison Stage-6 patch boundaries (the
        // edge-not-2-directed InvalidBooleanOutput class). Eligibility is
        // PER VERTEX: a vertex near-welds only when EVERY incident
        // arrangement triangle descends, via `la.source` + the operand
        // `tri_face` map, from a `Surface::Plane` face. Curved-adjacent
        // vertices keep bit-exact (kv9: cyl×cyl junction duplicates are
        // structurally distinct — one copy per incident surface's chord
        // ring — and Stage-4 owns their collapse). Empty / out-of-range /
        // sentinel provenance marks its vertices ineligible (conservative:
        // the sidecar parity producer keeps today's behavior, spec W4).
        {
            let face_planar = |k: u32, fi: u32| -> Option<bool> {
                let brep: &BRep = if k == 0 { a } else { b };
                brep.faces()
                    .get(fi as usize)
                    .map(|f| matches!(f.surface, Surface::Plane { .. }))
            };
            let curved = kv15_curved_touch(
                la.mesh.verts.len(),
                &la.mesh.tris,
                &la.source,
                tri_face_a,
                tri_face_b,
                face_planar,
            );
            // Propagate ineligibility through bit-exact clusters: a root is
            // curved if ANY member is (a bit-duplicate of a protected
            // junction vertex must not drag it into a near-weld).
            let mut root_curved = vec![false; la.mesh.verts.len()];
            for (i, &c) in curved.iter().enumerate() {
                if c {
                    root_curved[weld[i] as usize] = true;
                }
            }
            kv15_near_weld_pass(&la.mesh.verts, &mut weld, &root_curved);
        }

        // PR-6 (coincident-cylinder rim conformal weld). The §4.5.5 planar
        // Stage-0 overlay makes two coincident PLANAR faces' shared loop
        // vertices bit-identical (the cross-weld at `stage0.rs:261`). Its
        // curved analog: where a coincident-CYLINDER pair's lateral meets a
        // CAP PLANE, cherchi's exact arrangement mints the SAME rim-circle
        // point redundantly (once per generating tri-pair / incident surface),
        // landing a cluster of copies a FEW ULPs apart (verified on
        // `err.waffle`: 31 such near-twins, all at machine-zero distance from
        // a `cyl_pairs` lateral AND on the cap plane, max separation ~9e-19 at
        // a coordinate scale of 5e-3 — i.e. ~1 ULP). The bit-exact weld leaves
        // them distinct, so a kept triangle can carry two copies of one
        // geometric rim point: a zero-area sliver that fails Stage-4
        // (`DegenerateTriangle` at v4497/v4495) and pinches the post-membrane
        // seam.
        //
        // The conformal reconciliation: union ONLY vertices that lie EXACTLY
        // (within the pair's analytic band) on a coincident-cylinder pair's
        // shared lateral AND are within the scale-relative `TAU_WORK·(1+scale)`
        // band of each other. This is an EXACT-IDENTITY weld of redundant
        // reconstructions of one analytic point — NOT a tolerance bucket:
        //   • Membership is gated on the analytic coincident-cylinder surface
        //     (machine-zero radial distance), not a proximity guess.
        //   • The union band (~1e-12) is six orders below MIN_FEATURE_SIZE
        //     (1e-6); genuinely distinct rim points (≥ chord-spacing ~1e-4)
        //     never fuse — only sub-ULP duplicates do.
        //   • It touches NO planar case (gated on `cyl_pairs`), so it cannot
        //     reintroduce the reverted F0057 planar-weld masking (that weld
        //     fused planar vertices and hid 74 unpaired edges).
        // Survivor = the cluster's minimum welded index (deterministic).
        if !cyl_pairs.is_empty() {
            let verts = &la.mesh.verts;
            // On-cylinder predicate: radial distance within the pair band. The
            // observed rim duplicates sit at ~1e-19 (machine zero); the band
            // (1e-7) is a safe analytic membership gate that admits no
            // off-surface vertex of this model (off-rim arrangement points are
            // ≥ chord-scale ~1e-4 off any OTHER cylinder, and on-lateral
            // tessellation chords sit up to the sagitta INSIDE the radius —
            // far beyond 1e-7 — so only true on-surface rim points qualify).
            let on_rim = |i: u32| -> bool {
                let c = verts[i as usize].as_array();
                cyl_pairs
                    .iter()
                    .any(|p| centroid_on_cylinder(c, p) <= p.band)
            };
            let scale = verts
                .iter()
                .flat_map(|v| v.as_array())
                .fold(0.0f64, |m, c| m.max(c.abs()));
            let cluster_band = cad_primitives::TAU_WORK * (1.0 + scale);
            // Candidate rim vertices (post bit-exact weld representatives only).
            let rim: Vec<u32> = (0..verts.len() as u32)
                .filter(|&i| weld[i as usize] == i && on_rim(i))
                .collect();
            // Bucketed union-find (27-neighborhood probe + exact pairwise band).
            let mut parent: HashMap<u32, u32> = rim.iter().map(|&i| (i, i)).collect();
            fn find(parent: &mut HashMap<u32, u32>, mut x: u32) -> u32 {
                while parent[&x] != x {
                    let g = parent[&parent[&x]];
                    parent.insert(x, g);
                    x = g;
                }
                x
            }
            let cell = |c: f64| -> i64 { (c / cluster_band).floor() as i64 };
            let mut grid: HashMap<[i64; 3], Vec<u32>> = HashMap::new();
            for &i in &rim {
                let p = verts[i as usize].as_array();
                let key = [cell(p[0]), cell(p[1]), cell(p[2])];
                for dx in -1..=1i64 {
                    for dy in -1..=1i64 {
                        for dz in -1..=1i64 {
                            let Some(occ) = grid.get(&[key[0] + dx, key[1] + dy, key[2] + dz])
                            else {
                                continue;
                            };
                            for &j in occ {
                                let q = verts[j as usize].as_array();
                                let pair_band = cad_primitives::TAU_WORK
                                    * (1.0
                                        + p.iter()
                                            .chain(q.iter())
                                            .fold(0.0f64, |m, c| m.max(c.abs())));
                                if (0..3).all(|k| (p[k] - q[k]).abs() <= pair_band) {
                                    let (ri, rj) = (find(&mut parent, i), find(&mut parent, j));
                                    if ri != rj {
                                        parent.insert(ri.max(rj), ri.min(rj));
                                    }
                                }
                            }
                        }
                    }
                }
                grid.entry(key).or_default().push(i);
            }
            // Re-point every vertex whose bit-exact representative is a rim
            // candidate to its cluster minimum.
            for w in weld.iter_mut() {
                if parent.contains_key(w) {
                    *w = find(&mut parent, *w);
                }
            }
        }

        weld
    };

    // (3) Stage 4: which arrangement tris survive `op`.
    let kept = la.keep_set(op);

    // KV9-F1 diagnosis probe (read-only, env-gated): per-input label + keep
    // census over the labeled arrangement.
    if std::env::var_os("YANG_KEEP_PROBE").is_some() {
        let kept_set: std::collections::BTreeSet<usize> = kept.iter().copied().collect();
        let mut rows: std::collections::BTreeMap<(String, Vec<bool>, bool), usize> =
            std::collections::BTreeMap::new();
        for t in 0..la.mesh.tris.len() {
            let surf = format!("{:?}", la.surface[t]);
            *rows
                .entry((surf, la.inside[t].clone(), kept_set.contains(&t)))
                .or_insert(0) += 1;
        }
        eprintln!(
            "[keep-probe] la tris {} kept {} (op {op:?})",
            la.mesh.tris.len(),
            kept.len()
        );
        for ((surf, inside, k), n) in rows {
            eprintln!("[keep-probe]   surface {surf} inside {inside:?} kept={k}: {n}");
        }
        let mut patches: std::collections::BTreeMap<u32, (String, usize)> =
            std::collections::BTreeMap::new();
        for t in 0..la.mesh.tris.len() {
            let e = patches
                .entry(la.patch[t])
                .or_insert_with(|| (format!("{:?}", la.surface[t]), 0));
            e.1 += 1;
        }
        for (pid, (surf, n)) in patches {
            eprintln!("[keep-probe]   patch {pid}: surface {surf} tris {n}");
        }
    }

    // (3a) XOR deferred (spec §Scope): its symmetric-difference result is
    // multi-shell / has a void that `reconstruct_topology` cannot reassemble
    // yet. Error LOUDLY (`UnsupportedOp`) rather than emitting a generic
    // `NonManifoldOutput` or a silently-wrong result (P9). Gated on a
    // non-empty XOR kept-set: a degenerate XOR with nothing to reassemble
    // (empty arrangement) still trivially succeeds with an empty result, so
    // op-dispatch over an empty arrangement is well-defined for all four ops.
    if op == BoolOp::Xor && !kept.is_empty() {
        return Err(YangError::UnsupportedOp(op));
    }

    // (4) Compact kept sub-mesh: weld + per-op winding fix, then remap the
    // referenced (welded) verts to dense indices.
    let mut remap: Vec<Option<u32>> = vec![None; la.mesh.verts.len()];
    let mut compact_verts: Vec<Point3> = Vec::new();
    let mut compact_tris: Vec<[u32; 3]> = Vec::with_capacity(kept.len());
    // compact-tri index -> original `la` tri index (for surface lookup).
    let mut orig_tri: Vec<usize> = Vec::with_capacity(kept.len());
    for &orig_t in &kept {
        let raw = la.mesh.tris[orig_t];

        // (3b) §4.5.5 overlap-sheet ("membrane") resolution. A triangle with
        // a multi-solid surface label lies on the trimmed common planar
        // surface of a Stage-0 pair. Cherchi's keep-rules alone keep it for
        // EVERY op (surface = {A,B}, inside = ∅ satisfies the union /
        // intersection / subtraction-branch-1 rules, booleans.cpp:1397/
        // 1422/1467 — the C++ emits the zero-volume sheet); solid semantics
        // instead keep it iff exactly ONE side of its plane is inside the
        // result. With the pair's normal-agreement flag (`opposite`: solids
        // on opposite sides, stacked; else both interiors on the same
        // side, flush/pocket) that side rule reduces to:
        //
        //   Union:     keep iff !opposite (boundary of both ⇒ of the union)
        //   Intersect: keep iff !opposite (boundary of A∩B; opposite ⇒ the
        //              intersection is the zero-volume sheet itself: drop)
        //   Subtract:  keep iff opposite (B is beyond the plane: the sheet
        //              stays A's boundary; equal ⇒ B consumes it: the
        //              pocket OPENING is removed)
        //
        // The kept copy is the dedup survivor — input A's, with A's winding
        // — which is the correct result orientation in every kept case
        // (subtract-opposite / union-equal / intersect-equal all bound the
        // result with A's outward direction).
        if la.surface[orig_t].len() > 1 {
            let p0 = la.mesh.verts[raw[0] as usize].as_array();
            let p1 = la.mesh.verts[raw[1] as usize].as_array();
            let p2 = la.mesh.verts[raw[2] as usize].as_array();
            let c = [
                (p0[0] + p1[0] + p2[0]) / 3.0,
                (p0[1] + p1[1] + p2[1]) / 3.0,
                (p0[2] + p1[2] + p2[2]) / 3.0,
            ];
            // The sheet's `opposite` flag — found by matching its centroid to a
            // Stage-0 PLANAR pair plane (the §4.5.5 membrane) OR, failing that,
            // to a coincident-CYLINDER pair (PR-5: a sheet triangle lies on a
            // cylinder pair iff `|dist(c, axis_line) − radius| <= band`). Only
            // if NEITHER matches is it an unhandled config — still loud (P9).
            let planar = stage0.as_ref().and_then(|s0| {
                s0.pairs
                    .iter()
                    .find(|p| (p.n[0] * c[0] + p.n[1] * c[1] + p.n[2] * c[2] + p.d).abs() <= p.band)
                    .map(|p| p.opposite)
            });
            let opposite = match planar {
                Some(o) => o,
                // A sheet triangle on the TESSELLATED cylinder sits up to the
                // Stage-1 chord sagitta inside the analytic radius — far beyond
                // the detection `band`. Match against the curved chord bound
                // `d_ε` (the SAME bound Stage 1 sizes the tessellation to and
                // Stage-6 attribution uses for cylinder faces — A14.3, not a
                // widening). Both solids' overlap meshes are bit-identical, so
                // either chord bound applies; use the larger to be safe.
                None => match cyl_pairs.iter().find(|p| {
                    let de = curved_chord_bound(a.edges())
                        .unwrap_or(0.0)
                        .max(curved_chord_bound(b.edges()).unwrap_or(0.0))
                        .max(p.band);
                    centroid_on_cylinder(c, p) <= de
                }) {
                    Some(p) => p.opposite,
                    // On no known pair (planar or cylinder) — loud, never a
                    // guessed config.
                    None => return Err(YangError::FaceResolutionFailed { tri: orig_t }),
                },
            };
            let keep_sheet = match op {
                BoolOp::Union | BoolOp::Intersect => !opposite,
                BoolOp::Subtract => opposite,
                // XOR never reaches here (rejected at (3a) on a non-empty
                // kept set), but the side rule drops the sheet in both
                // configs anyway.
                BoolOp::Xor => false,
            };
            if !keep_sheet {
                continue;
            }
        }

        // Apply the weld (coincident points → shared original index).
        let mut tri = [
            weld[raw[0] as usize],
            weld[raw[1] as usize],
            weld[raw[2] as usize],
        ];
        // A welded triangle with a repeated index is a zero-area sliver at a
        // coincident (welded) point — it carries no surface and no volume, and
        // its two non-degenerate directed edges are mutual opposites that
        // cancel, so dropping it preserves the watertight half-edge pairing.
        // (Real, in-scope arrangement artifact — NOT non-manifold input.)
        if tri[0] == tri[1] || tri[1] == tri[2] || tri[2] == tri[0] {
            continue;
        }
        // Per-op winding fix (Cherchi booleans.cpp boolSubtraction:1480-1483):
        // the keep-rule selects triangles but some kept triangles bound the
        // result with reversed orientation and must be flipped so the output
        // is consistently outward-oriented (I9 signed volume). Union /
        // Intersection keep winding as-is.
        if flip_for_op(op, &la, orig_t) {
            tri.swap(1, 2);
        }
        let mut new_tri = [0u32; 3];
        for (k, &wi) in tri.iter().enumerate() {
            let slot = &mut remap[wi as usize];
            let new_vi = match slot {
                Some(idx) => *idx,
                None => {
                    let idx = compact_verts.len() as u32;
                    compact_verts.push(la.mesh.verts[wi as usize]);
                    *slot = Some(idx);
                    idx
                }
            };
            new_tri[k] = new_vi;
        }
        compact_tris.push(new_tri);
        orig_tri.push(orig_t);
    }
    // (I6 guard) Two distinct surviving triangles that welded to the same 3
    // vertices are genuinely coincident faces (non-manifold input) — e.g. the
    // a4 fixture's two tris over bit-exact-coincident vertices. A valid
    // arrangement has no such pair; reject it. (Compact indices are 1:1 with
    // welded indices, so a sorted-index key suffices.)
    {
        use std::collections::HashSet;
        let mut seen: HashSet<[u32; 3]> = HashSet::with_capacity(compact_tris.len());
        for t in &compact_tris {
            let mut sorted = *t;
            sorted.sort_unstable();
            if !seen.insert(sorted) {
                return Err(YangError::NonManifoldInput);
            }
        }
    }
    let kept_submesh = Mesh::new(compact_verts, compact_tris);

    // (5) Stage 6: face resolution → FULL attribution. PRIMARY path is N4
    // provenance (cherchi `source` → B-Rep face via the per-triangle face map,
    // `tri_face_a`/`tri_face_b` bound above the weld); the geometric
    // resolution below is the fallback. Either map may be empty (a Stage-0
    // path that does not emit provenance yet, or a lineage-less input) → that
    // triangle falls back to geometric.
    let mut attributions: Vec<Option<TriangleAttribution>> = Vec::with_capacity(orig_tri.len());
    for (compact_t, &orig_t) in orig_tri.iter().enumerate() {
        let surf = &la.surface[orig_t];
        let (input_brep, input) = if surf.len() > 1 {
            // §4.5.5 trimmed common surface (PR-YR26): a SURVIVING
            // multi-label triangle is a kept overlap-sheet triangle (the
            // (3b) side rule already decided it bounds the result). It
            // descends from coincident faces of BOTH inputs; the kept copy
            // is the dedup survivor — input A's, with A's winding — so it
            // attributes to input A (its plane equals B's, so the
            // inherited output surface is identical either way; A is the
            // deterministic choice consistent with the kept orientation).
            (a, InputId::A)
        } else {
            let LaInputId(k) = surf[0];
            // cherchi InputId(u32): 0 → A, 1 → B.
            match k {
                0 => (a, InputId::A),
                _ => (b, InputId::B),
            }
        };

        // N4 (provenance, §4.2.3): attribute this kept triangle to its B-Rep face
        // DIRECTLY from its parent input triangle (cherchi `source` → `tri_face`)
        // — exact, no geometry, no tolerance. Works for non-coplanar AND coplanar
        // overlaps (the latter via the Stage-0 re-tessellated meshes' face maps).
        //
        // N4 RETIREMENT (task #53, spec `specs/n4_retire_stage6_fallback.md`):
        // on a lineage-CARRYING input, a provenance MISS is a producer fault
        // and fails LOUDLY — the `YANG_N4_FALLBACK_PROBE` measurement proved
        // zero misses across the full corpus, and a silent geometric guess can
        // misattribute (the failure class N4 eliminated) while masking
        // provenance regressions. The geometric resolution below remains ONLY
        // for LINEAGE-LESS attribution: an arrangement without `source` (the
        // dev-only C++ sidecar oracle and the in-crate mock-label fixtures;
        // reference parity depends on it) or an input without a provenance
        // map (`ProvMiss::NoLineage` — a yang boolean OUTPUT chained directly
        // back in, or a `from_mesh` B-Rep).
        if !la.source.is_empty() {
            match provenance_face_reason(&la.source[orig_t], input, tri_face_a, tri_face_b) {
                Ok(face) => {
                    attributions.push(Some(TriangleAttribution { input, face }));
                    continue;
                }
                // Lineage-less input: the documented geometric path below.
                Err(ProvMiss::NoLineage) => {}
                Err(reason) => {
                    // Env-gated diagnostic naming the miss reason; the error
                    // itself is unconditional.
                    if std::env::var_os("YANG_N4_FALLBACK_PROBE").is_some() {
                        eprintln!(
                            "[n4-fallback] input={input:?} orig_t={orig_t} reason={reason:?} \
                             stage0={} tf_a_len={} tf_b_len={}",
                            stage0.is_some(),
                            tri_face_a.len(),
                            tri_face_b.len(),
                        );
                    }
                    return Err(YangError::FaceResolutionFailed { tri: compact_t });
                }
            }
        }

        // Centroid of the (compact) triangle — same coords as `la.mesh`.
        let tri = kept_submesh.tris[compact_t];
        let p0 = kept_submesh.verts[tri[0] as usize].as_array();
        let p1 = kept_submesh.verts[tri[1] as usize].as_array();
        let p2 = kept_submesh.verts[tri[2] as usize].as_array();
        let c = [
            (p0[0] + p1[0] + p2[0]) / 3.0,
            (p0[1] + p1[1] + p2[1]) / 3.0,
            (p0[2] + p1[2] + p2[2]) / 3.0,
        ];

        // Is this kept triangle DEGENERATE (zero-area / collinear)? The exact
        // arrangement emits sliver triangles along shared solid edges (3
        // distinct welded verts, all collinear). They carry no surface and no
        // volume but pair their edges into the watertight result, so they are
        // kept (not dropped — dropping breaks edge-pairing). Their centroid
        // lands on a solid edge, equidistant from the two adjacent face planes,
        // so the unique-face rule would (wrongly) F3-tie them. Threshold is the
        // M1 area threshold (2·area = ‖cross(e1,e2)‖; compare to MIN_FEATURE_SIZE²;
        // governance A14.3 — shared constant, no ad-hoc epsilon).
        let e1 = [p1[0] - p0[0], p1[1] - p0[1], p1[2] - p0[2]];
        let e2 = [p2[0] - p0[0], p2[1] - p0[1], p2[2] - p0[2]];
        let cross = [
            e1[1] * e2[2] - e1[2] * e2[1],
            e1[2] * e2[0] - e1[0] * e2[2],
            e1[0] * e2[1] - e1[1] * e2[0],
        ];
        let twice_area = (cross[0] * cross[0] + cross[1] * cross[1] + cross[2] * cross[2]).sqrt();
        let degenerate =
            twice_area < cad_primitives::MIN_FEATURE_SIZE * cad_primitives::MIN_FEATURE_SIZE;

        // Distance of the centroid to each labeled-solid face plane. Curved
        // faces are already rejected at `BRep::new`, so this is defensive — but
        // it must compile and be LOUD (P9): a curved arm returns the carrying
        // `Err`, never `unreachable!`/panic. `fi` is the input B-Rep face index.
        // PR-YR27 (Finding 2): a face that went through a Stage-0 pair had
        // its loop vertices SNAPPED onto the pair's CANONICAL plane, so its
        // kept triangles lie on the canonical plane — up to the pair's
        // detection `band` (≫ TAU_WORK) away from the face's STORED plane.
        // Membership for exactly those faces is therefore measured against
        // the canonical pair plane (KEYED to the pair: every non-pair face
        // keeps its stored surface + TAU_WORK byte-for-byte — this is the
        // Stage-1 geometry the snap actually produced, NOT a tolerance
        // widening).
        let stage0_pair_plane = |fi: usize| -> Option<&stage0::PairPlane> {
            stage0.as_ref().and_then(|s0| {
                s0.pairs.iter().find(|p| match input {
                    InputId::A => p.face_a == fi,
                    InputId::B => p.face_b == fi,
                })
            })
        };
        let plane_dist = |fi: usize, face: &BRepFace| -> Result<f64, YangError> {
            if let Some(pp) = stage0_pair_plane(fi) {
                return Ok((pp.n[0] * c[0] + pp.n[1] * c[1] + pp.n[2] * c[2] + pp.d).abs());
            }
            // PR-YR7: delegate to the shared `signed_distance_to_surface`
            // (Plane + Cylinder + Sphere); take `.abs()` (distance to the
            // surface). Cone still rejects loudly — the free function returns a
            // sentinel face index, which we replace with the real input `fi`.
            match signed_distance_to_surface(face.surface, Point3::new(c[0], c[1], c[2])) {
                Ok(d) => Ok(d.abs()),
                Err(YangError::CurvedSurfaceNotYetSupported { .. }) => {
                    Err(YangError::CurvedSurfaceNotYetSupported { face: fi })
                }
                Err(other) => Err(other),
            }
        };

        // PER-FACE membership tolerance (PR-YR8 Blocker 1, spec §4). The
        // membership tolerance is the surface's OWN Stage-1 tessellation chord
        // bound (governance A15 / A14.3 — not tolerance widening): a `Plane`
        // face has zero chord error → `TAU_WORK`; a `Cylinder` face is a
        // `d_ε`-chord approximation BY CONSTRUCTION → its labeled solid's curved
        // chord band `d_ε`, the SAME bound Stage 1 guarantees. Computed once per
        // labeled solid from the SINGLE shared source.
        //
        // A `Cylinder` face implies the solid HAS circle rims, so `band` is
        // `Some`; if it is somehow `None` for a cylinder face that is a genuine
        // producer fault → `FaceResolutionFailed` (do NOT silently default a
        // cylinder face to `TAU_WORK`).
        //
        // For ALL-PLANAR inputs every face uses `TAU_WORK` (planar faces always
        // do; an all-planar solid has `band == None` so no face consults it),
        // making BOTH branches below byte-for-byte the OLD rules — the 900-case
        // box fuzz and the m3/yr5c planar-sliver tests are unaffected.
        let band = curved_chord_bound(input_brep.edges());
        let tol_for = |fi: usize, surface: Surface| -> Result<f64, YangError> {
            match surface {
                // PR-YR27 Finding 2 (completion): a planar face welded onto a
                // Stage-0 canonical pair plane legitimately lies up to the
                // pair's detection `band` from it — the SAME band `plane_dist`
                // above already measures the centroid against. The membership
                // THRESHOLD must match that distance basis, so a pair-plane face
                // uses its pair band; every NON-pair planar face keeps TAU_WORK
                // byte-for-byte (the exact/band tier split below still keys on
                // TAU_WORK, so on-plane triangles stay EXACT hits and the
                // all-planar fuzz corpus is unaffected — this only admits the
                // band-level offset the Stage-0 weld itself introduced, NOT a
                // widening). Without it a coplanar boolean at non-unit model
                // scale (e.g. a 10 mm bearing recess, coords ~1e-2, weld
                // residual ~1e-10 ≫ TAU_WORK) loses its annulus-cap triangles to
                // a spurious FaceResolutionFailed.
                Surface::Plane { .. } => Ok(match stage0_pair_plane(fi) {
                    Some(pp) => pp.band.max(cad_primitives::TAU_WORK),
                    None => cad_primitives::TAU_WORK,
                }),
                Surface::Cylinder { .. } => match band {
                    Some(de) => Ok(de),
                    None => Err(YangError::FaceResolutionFailed { tri: compact_t }),
                },
                // PR-YR15: a Sphere face uses its OWN Stage-1 chord bound
                // `sphere_chord_bound(radius) = 1e-2·2r√3` — the SAME bound
                // Stage 1 guarantees (A15/A14.3, NOT tolerance widening). It is
                // deliberately NOT the Circle-rim `band` (2r√2), which would
                // underestimate the sphere's chord error.
                Surface::Sphere { radius, .. } => Ok(sphere_chord_bound(radius)),
                // PR-YR17: a Cone face uses its OWN Stage-1 chord bound
                // `cone_chord_bound(height, half_angle)` — the SAME bound Stage 1
                // guarantees (A15/A14.3, NOT tolerance widening). The cone height
                // is not in `Surface::Cone` (only apex/axis_dir/half_angle), so it
                // is derived from the cone face's rim `Curve::Circle` edge in its
                // outer loop exactly as the Stage-1 pre-pass does (src/lib.rs
                // ~503-525): `height = |(rim_center − apex)·â|`. This is the live
                // reject site for a Cone (PR-YR16 made
                // `signed_distance_to_surface(Cone)` return `Ok`, so `plane_dist`
                // no longer rejects the cone upstream). If the cone face's outer
                // loop has NO rim Circle, no sound height can be derived → loud
                // `FaceResolutionFailed` (a genuine producer fault; P9 — NEVER a
                // defaulted or widened tolerance).
                Surface::Cone {
                    apex,
                    axis_dir,
                    half_angle,
                } => {
                    let au = normalize3(axis_dir.as_array());
                    let ap = apex.as_array();
                    let mut height: Option<f64> = None;
                    for &e_idx in &input_brep.faces()[fi].outer_loop {
                        if let Curve::Circle { center, .. } =
                            input_brep.edges()[e_idx as usize].curve
                        {
                            let c = center.as_array();
                            height = Some(
                                ((c[0] - ap[0]) * au[0]
                                    + (c[1] - ap[1]) * au[1]
                                    + (c[2] - ap[2]) * au[2])
                                    .abs(),
                            );
                            break;
                        }
                    }
                    match height {
                        Some(h) => Ok(cone_chord_bound(h, half_angle)),
                        None => Err(YangError::FaceResolutionFailed { tri: compact_t }),
                    }
                }
                // KV6d: a torus face uses the rim chord `band` (the rim AABB
                // bound covers the outermost latitude radius major+minor).
                Surface::Torus { .. } => match band {
                    Some(de) => Ok(de),
                    None => Err(YangError::FaceResolutionFailed { tri: compact_t }),
                },
            }
        };

        let face = if degenerate {
            // Degenerate sliver: attribute to the LOWEST face index within ITS
            // per-face tolerance (a zero-area triangle has no area, so which
            // adjacent face it joins is geometrically harmless). Never an F3
            // tie — the tie contract is for *real* (positive-area) triangles.
            //
            // PR-YR8: this branch uses the PER-FACE tolerance, not absolute
            // TAU_WORK. The spec §4 "degenerate branch keeps TAU_WORK" line was
            // written for the planar-only world (slivers only on shared
            // planar-planar solid edges, centroid on both planes within
            // TAU_WORK). It did not foresee a sliver lying ON a tessellated
            // CYLINDER face: the sidecar arrangement emits a near-zero-area
            // sliver on the cylinder lateral surface whose centroid is ~d_ε
            // inside the analytic cylinder (within the Stage-1 bound, but ≫
            // TAU_WORK). The governing PRINCIPLE (§4 Blocker 1: test membership
            // at the surface's own Stage-1 chord bound) applies to ANY triangle
            // on the cylinder face, degenerate or not. For all-planar inputs
            // this stays byte-identical (every tol = TAU_WORK). If no face is
            // within tolerance, that is a genuine producer fault → loud (P9).
            let mut hit: Option<u32> = None;
            for (fi, f) in input_brep.faces().iter().enumerate() {
                if plane_dist(fi, f)? < tol_for(fi, f.surface)? {
                    hit = Some(fi as u32);
                    break;
                }
            }
            match hit {
                Some(fi) => fi,
                None => return Err(YangError::FaceResolutionFailed { tri: compact_t }),
            }
        } else {
            // PR-YR20 tiered tie-break: an EXACT membership (centroid within
            // TAU_WORK of the surface — it lies ON it) dominates a
            // within-chord-band membership. Each face still uses its own A14.3
            // band via tol_for; we only rank the tie by tier. For all-planar
            // inputs every hit is EXACT (planar tol == TAU_WORK), so a unique
            // hit is byte-for-byte the old "exactly one face within TAU_WORK"
            // rule.
            let mut exact_hits: Vec<u32> = Vec::new();
            let mut band_hits: Vec<u32> = Vec::new();
            for (fi, f) in input_brep.faces().iter().enumerate() {
                let d = plane_dist(fi, f)?;
                if d < tol_for(fi, f.surface)? {
                    if d < cad_primitives::TAU_WORK {
                        exact_hits.push(fi as u32);
                    } else {
                        band_hits.push(fi as u32);
                    }
                }
            }
            // PR-YR27 (Finding 3): a multi-hit tier is narrowed by FINITE-
            // EXTENT strict containment before it is declared a tie. The
            // infinite-plane rule alone false-positives whenever a kept
            // triangle's centroid happens to lie bit-exactly ON another
            // face's plane (the L-profile CDT class: cap triangle
            // (0,0),(2,0),(1,1) → centroid x = 1 = the x=1 side plane;
            // likewise a chained input carrying two same-plane faces). The
            // TRUE owning face strictly contains the centroid of every
            // positive-area kept triangle attributed to it; the false
            // positive at best touches its trimmed region's boundary —
            // strictness is therefore sound and load-bearing. Faces the
            // exact 2D test cannot decide (curved surfaces / curved loop
            // edges → `None`) are NEVER excluded, so an undecidable tie
            // stays the loud error (P9 — containment breaks ties, it never
            // widens membership; a unique hit is accepted without it,
            // byte-identical to the old rule).
            let narrow = |hits: Vec<u32>| -> Result<Option<u32>, YangError> {
                match hits.len() {
                    0 => Ok(None),
                    1 => Ok(Some(hits[0])),
                    _ => {
                        let kept: Vec<u32> = hits
                            .into_iter()
                            .filter(|&fi| {
                                point_strictly_in_planar_face(input_brep, fi as usize, c)
                                    != Some(false)
                                    && point_strictly_in_cylinder_face_axially(
                                        input_brep,
                                        fi as usize,
                                        c,
                                    ) != Some(false)
                            })
                            .collect();
                        match kept.len() {
                            1 => Ok(Some(kept[0])),
                            // 0 (centroid on every tied face's boundary) — loud.
                            0 => Err(YangError::FaceResolutionFailed { tri: compact_t }),
                            // ≥2 survivors. SAME-SURFACE TIE: faces sharing
                            // IDENTICAL surface geometry are INTERCHANGEABLE for
                            // attribution — a triangle on that surface belongs to
                            // it no matter which fragment owns it, and topology
                            // reconstruction regroups them by adjacency into one
                            // output face. This arises when one analytic surface
                            // is SPLIT into several faces — e.g. a cylindrical
                            // bore fragmented into arc-faces by the
                            // tessellated-polygon profile fallback (gear bores).
                            // Pick the lowest index: NOT silent-wrong (same
                            // surface), unlike a tolerance widening. A tie among
                            // GEOMETRICALLY DISTINCT surfaces stays the loud error
                            // (P9 — genuinely ambiguous).
                            _ => {
                                let s0 = input_brep.faces()[kept[0] as usize].surface;
                                if kept
                                    .iter()
                                    .all(|&fi| input_brep.faces()[fi as usize].surface == s0)
                                {
                                    Ok(kept.iter().copied().min())
                                } else {
                                    Err(YangError::FaceResolutionFailed { tri: compact_t })
                                }
                            }
                        }
                    }
                }
            };
            match narrow(exact_hits)? {
                Some(fi) => fi, // exact tier dominates
                None => match narrow(band_hits)? {
                    Some(fi) => fi,
                    None => return Err(YangError::FaceResolutionFailed { tri: compact_t }),
                },
            }
        };
        attributions.push(Some(TriangleAttribution { input, face }));
    }
    let mut triangle_attribution = TriangleAttributionMap { attributions };

    // (6) Topology reconstruction + Stage-4 relocation (PR-YR10). Stage 4 may
    // relocate intersection vertices in-place (onto the exact curves) and, on a
    // §4.5.3 reversal, edge-collapse a mesh vertex — mutating BOTH the mesh and
    // the attribution in lockstep — so both are passed by `&mut` and the
    // tessellation sources come back from `reconstruct_topology`.
    let mut kept_submesh = kept_submesh;
    let (vertices, edges, faces, sources, face_attribution) =
        reconstruct_topology_stage4(&mut kept_submesh, &mut triangle_attribution, a, b, op)?;

    let tessellation = TessellationMap { sources };

    Ok(BRep {
        vertices,
        edges,
        faces,
        mesh: kept_submesh,
        tessellation,
        triangle_attribution,
        face_attribution,
        // A boolean-output BRep has no Stage-1 face_tri_ranges lineage; leave the
        // provenance map empty so a CHAINED boolean falls back to geometric
        // attribution (until the output reconstruction also emits a tri→face map).
        tri_face: Vec::new(),
        forced_rim_n: None,
    })
}
