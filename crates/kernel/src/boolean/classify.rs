//! Face classification for boolean operations.
//!
//! Contains Generalized Winding Number (GWN) computation for inside/outside
//! segmentation, and face-level classification against opposing solids using
//! Sutherland-Hodgman clipping + ray casting.
//!
//! Ref #7: Jacobson et al. (2013) — Robust inside/outside via GWN.
//! Ref #4: Shewchuk (1997) — Adaptive precision floating-point arithmetic.

use crate::units::{MIN_FEATURE_SIZE, TAU_NORMALIZE, TAU_PARALLEL};
use crate::vecmath::*;

use super::{
    classify_coplanarity, clip_polygon_by_plane_cached, clip_polygon_by_solid, is_coplanar,
    polygon_area_3d, polygon_centroid, CoplanarClass, FacePoly, IntersectionCache,
};

// ── Generalized Winding Number (GWN) ────────────────────────────────────
//
// Ref #7: Jacobson et al. (2013) — Robust inside/outside segmentation using
//         generalized winding numbers.
// Ref #4: Shewchuk (1997) — Adaptive precision floating-point arithmetic.
//
// Unlike ray-casting, GWN is smooth, continuous, and has no grazing failures.
// w(P) = (1/4π) Σ solid_angle(P, triangle_i)
// w > 0.5 → inside, w < 0.5 → outside.

/// Lazy exact escalation for scalar triple product sign.
///
/// If the floating-point triple product `fp_value` is within the error bound,
/// escalates to Shewchuk's exact `orient3d` predicate. Returns (sign, corrected_value).
/// Ref #4: Shewchuk (1997).
fn lazy_exact_triple_sign(
    p: [f64; 3],
    a: [f64; 3],
    b: [f64; 3],
    c: [f64; 3],
    fp_value: f64,
) -> (i32, f64) {
    let pa = [a[0] - p[0], a[1] - p[1], a[2] - p[2]];
    let pb = [b[0] - p[0], b[1] - p[1], b[2] - p[2]];
    let pc = [c[0] - p[0], c[1] - p[1], c[2] - p[2]];

    let mut max_abs = 0.0f64;
    for v in &[pa, pb, pc] {
        for &coord in v {
            let a = coord.abs();
            if a > max_abs {
                max_abs = a;
            }
        }
    }

    // Error bound: O(eps * max_coord^3) with conservative factor 24
    let eps = f64::EPSILON;
    let error_bound = 24.0 * eps * max_abs * max_abs * max_abs;

    if fp_value.abs() > error_bound {
        let sign = if fp_value > 0.0 { 1 } else { -1 };
        (sign, fp_value)
    } else {
        // Ambiguous — escalate to exact predicate
        let orient = robust::orient3d(
            robust::Coord3D {
                x: a[0],
                y: a[1],
                z: a[2],
            },
            robust::Coord3D {
                x: b[0],
                y: b[1],
                z: b[2],
            },
            robust::Coord3D {
                x: c[0],
                y: c[1],
                z: c[2],
            },
            robust::Coord3D {
                x: p[0],
                y: p[1],
                z: p[2],
            },
        );
        if orient == 0.0 {
            (0, 0.0)
        } else {
            let exact_sign = if orient > 0.0 { 1 } else { -1 };
            let corrected = if fp_value == 0.0 {
                exact_sign as f64 * f64::MIN_POSITIVE
            } else if (fp_value > 0.0) != (exact_sign > 0) {
                -fp_value
            } else {
                fp_value
            };
            (exact_sign, corrected)
        }
    }
}

/// Solid angle subtended by triangle (a, b, c) at point p.
///
/// Van Oosterom & Strackee (1983) formula with lazy exact sign escalation.
/// Ref #7 Jacobson, #4 Shewchuk.
#[inline]
pub(super) fn solid_angle(p: [f64; 3], a: [f64; 3], b: [f64; 3], c: [f64; 3]) -> f64 {
    let pa = v3_sub(a, p);
    let pb = v3_sub(b, p);
    let pc = v3_sub(c, p);

    let la = v3_length(pa);
    let lb = v3_length(pb);
    let lc = v3_length(pc);

    // Degenerate: query point at a vertex
    if la < TAU_NORMALIZE || lb < TAU_NORMALIZE || lc < TAU_NORMALIZE {
        return 0.0;
    }

    // Degenerate triangle: near-zero area
    let cross = v3_cross(v3_sub(b, a), v3_sub(c, a));
    if v3_dot(cross, cross) < TAU_NORMALIZE * TAU_NORMALIZE {
        return 0.0;
    }

    // Scalar triple product: det([pa, pb, pc])
    let fp_numerator = pa[0] * (pb[1] * pc[2] - pb[2] * pc[1])
        + pa[1] * (pb[2] * pc[0] - pb[0] * pc[2])
        + pa[2] * (pb[0] * pc[1] - pb[1] * pc[0]);

    let (exact_sign, numerator) = lazy_exact_triple_sign(p, a, b, c, fp_numerator);

    if exact_sign == 0 {
        return 0.0;
    }

    let ab = v3_dot(pa, pb);
    let bc = v3_dot(pb, pc);
    let ca = v3_dot(pc, pa);
    let denominator = la * lb * lc + ab * lc + bc * la + ca * lb;

    let result = 2.0 * numerator.atan2(denominator);
    if result.is_nan() {
        0.0
    } else {
        result
    }
}

/// Generalized winding number of point `p` w.r.t. a polygon soup.
///
/// Triangulates each FacePoly via fan from vertex 0.
/// Returns ~1.0 for inside, ~0.0 for outside, ~0.5 for boundary.
/// Ref #7 Jacobson et al. (2013).
pub(super) fn winding_number(p: [f64; 3], faces: &[FacePoly]) -> f64 {
    let mut total = 0.0;
    for face in faces {
        let n = face.verts.len();
        if n < 3 {
            continue;
        }
        // Fan triangulation from vertex 0
        for i in 1..n - 1 {
            let sa = solid_angle(p, face.verts[0], face.verts[i], face.verts[i + 1]);
            if !sa.is_nan() {
                total += sa;
            }
        }
    }
    total / (4.0 * std::f64::consts::PI)
}

/// Classify point as inside/outside using GWN.
///
/// Returns Some(true) for inside (w > WINDING_INSIDE_THRESHOLD),
/// Some(false) for outside (w < WINDING_OUTSIDE_THRESHOLD),
/// None for ambiguous boundary.
pub(super) fn winding_number_classify(p: [f64; 3], faces: &[FacePoly]) -> Option<bool> {
    use crate::units::{WINDING_INSIDE_THRESHOLD, WINDING_OUTSIDE_THRESHOLD};
    let w = winding_number(p, faces);
    if w.is_nan() {
        return None;
    }
    if w > WINDING_INSIDE_THRESHOLD {
        Some(true)
    } else if w < WINDING_OUTSIDE_THRESHOLD {
        Some(false)
    } else {
        None // ambiguity band
    }
}

/// Test if a point is inside a closed polyhedral solid using GWN.
///
/// Replaces the former 8-direction ray-cast majority voting approach which
/// failed for grazing angles near edges/vertices of non-convex solids.
/// GWN is smooth and has no such failure modes.
/// Ref #7 Jacobson et al. (2013).
pub(crate) fn point_in_solid(point: [f64; 3], faces: &[FacePoly]) -> bool {
    winding_number_classify(point, faces).unwrap_or(false)
}

// ── Face classification ─────────────────────────────────────────────────

/// Classification of a face fragment with respect to the opposing solid.
#[derive(Debug)]
pub(crate) enum FaceClass {
    /// Entirely outside the opposing solid.
    Outside,
    /// Entirely inside the opposing solid.
    Inside,
    /// Non-coplanar partial: inside fragments are truly inside the opposing
    /// solid's volume. For union, only the outside fragments are emitted.
    /// All inside fragments are retained (no largest-fragment heuristic).
    Partial {
        inside_frags: Vec<Vec<[f64; 3]>>,
        outside_frags: Vec<Vec<[f64; 3]>>,
    },
    /// Same-direction coplanar partial: face has a coplanar partner on the
    /// opposing solid (same normal). The "inside" is the surface overlap,
    /// not inside the volume. For union: primary emits all sub-regions
    /// (inside + outside frags), secondary emits only outside frags.
    /// All inside fragments are retained.
    CoplanarPartial {
        inside_frags: Vec<Vec<[f64; 3]>>,
        outside_frags: Vec<Vec<[f64; 3]>>,
    },
    /// Anti-parallel coplanar: face lies on shared boundary between touching
    /// solids. For union: remove from both. For subtract: keep for A, discard for B.
    CoplanarTouching,
    /// Anti-parallel coplanar face that has been split by the opposing solid's
    /// non-coplanar faces. For subtract: emit outside_frags from A (annular region).
    /// For union: discard both sides (shared internal boundary).
    AntiParallelCoplanarPartial {
        inside_frags: Vec<Vec<[f64; 3]>>,
        outside_frags: Vec<Vec<[f64; 3]>>,
    },
}

/// Classify a face polygon against the opposing solid's faces.
///
/// Uses Sutherland-Hodgman clipping as the primary classifier, with
/// point-in-solid ray casting as a secondary check when S-H reports the
/// face is "fully inside" (inside_area ≈ original_area). For convex solids,
/// S-H is authoritative; for non-convex solids, S-H can falsely report
/// full containment when the half-space intersection is degenerate.
/// Ray casting correctly handles both convex and non-convex solids.
///
/// Ref #7 Jacobson: winding number approach (simplified to ray casting).
pub(crate) fn classify_face(
    face: &FacePoly,
    opposing: &[FacePoly],
    tau: f64,
    cache: &mut Option<IntersectionCache>,
) -> FaceClass {
    let original_area = polygon_area_3d(&face.verts);
    if original_area < TAU_NORMALIZE {
        return FaceClass::Outside;
    }

    // Classify coplanarity with each opposing face
    let mut has_coplanar = false;
    let mut has_antiparallel_coplanar = false;
    for opp in opposing {
        match classify_coplanarity(face.normal, face.verts[0], opp, tau) {
            CoplanarClass::SameDirection => has_coplanar = true,
            CoplanarClass::AntiParallel => {
                has_coplanar = true;
                has_antiparallel_coplanar = true;
            }
            CoplanarClass::NotCoplanar => {}
        }
    }

    // This function is called only when the opposing solid has been verified
    // geometrically convex by is_face_set_convex in boolean/mod.rs.
    //
    // For small opposing sets (≤12 faces), S-H clipping is numerically stable
    // and used as the authoritative classifier. For larger convex solids
    // (e.g. 34-face polygon-approximated cylinders), S-H against many planes
    // can accumulate precision errors causing watertight failures. Use
    // centroid ray-casting for the common fully-inside/fully-outside cases,
    // but fall through to S-H when the face is partially inside (centroid
    // inside but vertices extend beyond — the "centroid-only bug").
    let opposing_likely_convex = opposing.len() <= 12;

    if opposing_likely_convex {
        // Small convex opposing solid: S-H clipping is authoritative
        let inside = clip_polygon_by_solid(&face.verts, opposing, tau, Some(face.normal), cache);
        let inside_area = polygon_area_3d(&inside);

        if inside_area < TAU_NORMALIZE {
            return FaceClass::Outside;
        }

        let rel_diff = (inside_area - original_area).abs() / original_area;
        if rel_diff < TAU_PARALLEL {
            if has_antiparallel_coplanar {
                return FaceClass::CoplanarTouching;
            }
            if has_coplanar {
                return FaceClass::CoplanarPartial {
                    inside_frags: vec![face.verts.clone()],
                    outside_frags: vec![],
                };
            }
            return FaceClass::Inside;
        }

        // Partial: split face using S-H (correct for convex opposing solid)
        let outside_frags =
            split_outside_fragments(&face.verts, opposing, tau, Some(face.normal), cache);
        let has_same_dir_coplanar = has_coplanar && !has_antiparallel_coplanar;
        if has_same_dir_coplanar {
            return FaceClass::CoplanarPartial {
                inside_frags: vec![inside],
                outside_frags,
            };
        }
        return FaceClass::Partial {
            inside_frags: vec![inside],
            outside_frags,
        };
    }

    // Large convex opposing solid (>12 faces): S-H fragment geometry against
    // many planes can accumulate precision errors causing watertight failures.
    // Use S-H to compute the inside AREA RATIO (which is robust), then:
    // - If area ratio ≈ 0: Outside (centroid confirms)
    // - If area ratio ≈ 1: Inside/CoplanarTouching (centroid confirms)
    // - If partial (0 < ratio < 1): use S-H fragments (the centroid-only bug fix)
    let inward_offset = v3_scale(face.normal, -tau * 100.0);

    // Handle coplanar cases first
    if has_antiparallel_coplanar {
        return classify_coplanar_nonconvex_antiparallel(face, opposing, tau, cache);
    }
    if has_coplanar {
        let inside = clip_polygon_by_solid(&face.verts, opposing, tau, Some(face.normal), cache);
        let inside_area = polygon_area_3d(&inside);
        if inside_area > TAU_NORMALIZE {
            let outside_frags =
                split_outside_fragments(&face.verts, opposing, tau, Some(face.normal), cache);
            return FaceClass::CoplanarPartial {
                inside_frags: vec![inside],
                outside_frags,
            };
        }
        return FaceClass::Outside;
    }

    // Non-coplanar face: centroid ray-casting for binary classification,
    // with S-H fragment production only when the face SIGNIFICANTLY extends
    // beyond the opposing solid's AABB (the "centroid-only bug" scenario).
    //
    // The centroid-only bug: a large face whose centroid falls inside a small
    // opposing solid gets classified as fully Inside and discarded, even though
    // most of the face area is outside. Fix: when centroid says Inside, check
    // if the face extends beyond the opposing AABB — if so, use S-H fragments.
    // For small faces near the boundary, binary classification avoids the
    // watertight issues caused by S-H precision errors against many planes.
    let centroid = polygon_centroid(&face.verts);
    let sample = v3_add(centroid, inward_offset);
    let centroid_inside = point_in_solid(sample, opposing);

    if !centroid_inside {
        return FaceClass::Outside;
    }

    // Centroid is inside — check if face extends beyond opposing solid's AABB.
    // If so, the face is larger than the opposing solid and must be partially
    // classified (the centroid-only bug scenario).
    let (opp_min, opp_max) = {
        let mut mn = [f64::INFINITY; 3];
        let mut mx = [f64::NEG_INFINITY; 3];
        for opp in opposing {
            for v in &opp.verts {
                for j in 0..3 {
                    mn[j] = mn[j].min(v[j]);
                    mx[j] = mx[j].max(v[j]);
                }
            }
        }
        (mn, mx)
    };
    let face_extends_beyond = face.verts.iter().any(|v| {
        v[0] < opp_min[0] - tau
            || v[0] > opp_max[0] + tau
            || v[1] < opp_min[1] - tau
            || v[1] > opp_max[1] + tau
            || v[2] < opp_min[2] - tau
            || v[2] > opp_max[2] + tau
    });

    if !face_extends_beyond {
        // Face fits within opposing AABB — centroid Inside is reliable
        return FaceClass::Inside;
    }

    // Face extends beyond opposing AABB. Use S-H to check the area ratio.
    // Only produce fragments when most of the face is OUTSIDE the opposing
    // solid (inside_ratio < 0.5). This catches the centroid-only bug (large
    // face with centroid inside a small solid) while avoiding S-H precision
    // issues for faces that are mostly inside.
    let inside = clip_polygon_by_solid(&face.verts, opposing, tau, Some(face.normal), cache);
    let inside_area = polygon_area_3d(&inside);
    if inside_area < TAU_NORMALIZE {
        return FaceClass::Outside;
    }
    let inside_ratio = inside_area / original_area;
    if inside_ratio >= 0.5 {
        // Face is mostly inside — centroid classification is reliable
        return FaceClass::Inside;
    }
    // Face is mostly outside but centroid is inside — the centroid-only bug.
    // Produce S-H fragments for correct partial classification.
    let outside_frags =
        split_outside_fragments(&face.verts, opposing, tau, Some(face.normal), cache);
    FaceClass::Partial {
        inside_frags: vec![inside],
        outside_frags,
    }
}

/// Classify a face against a non-convex opposing solid using edge-piercing
/// analysis combined with local S-H clipping.
///
/// For non-convex opposing solids, S-H clipping against ALL opposing faces
/// is incorrect (half-space intersection ≠ solid interior). Instead, this
/// function identifies which opposing faces have edges that ACTUALLY pierce
/// this face, and clips only against those relevant planes. Locally, the
/// relevant planes form a convex boundary around the intersection, making
/// S-H valid. For faces with no piercings, uses centroid ray-casting.
///
/// Ref #7 Jacobson: winding numbers for inside/outside classification.
/// Classify a face against a non-convex opposing solid using progressive
/// splitting by face-face intersection planes.
///
/// For non-convex opposing solids, S-H clipping against ALL opposing faces
/// is incorrect (half-space intersection ≠ solid interior).  Instead:
///
/// 1. Find opposing faces whose plane actually intersects this face
///    (verified via `face_face_intersection_segment`).
/// 2. Progressively split this face by those planes (keeping BOTH halves
///    at each step — unlike S-H which keeps only the inside).
/// 3. Classify each resulting fragment with `point_in_solid`.
///
/// This produces matching boundary edges with the opposing solid's S-H
/// splits because both sides clip against the same face planes.
///
/// Ref #7 Jacobson: winding numbers for inside/outside classification.
pub(crate) fn classify_face_nonconvex(
    face: &FacePoly,
    opposing: &[FacePoly],
    tau: f64,
    cache: &mut Option<IntersectionCache>,
) -> FaceClass {
    let original_area = polygon_area_3d(&face.verts);
    if original_area < TAU_NORMALIZE {
        return FaceClass::Outside;
    }

    // Check coplanar partnerships
    let has_antiparallel = opposing.iter().any(|opp| {
        classify_coplanarity(face.normal, face.verts[0], opp, tau) == CoplanarClass::AntiParallel
    });
    if has_antiparallel {
        return classify_coplanar_nonconvex_antiparallel(face, opposing, tau, cache);
    }

    let has_coplanar = opposing
        .iter()
        .any(|opp| is_coplanar(face.normal, face.verts[0], opp, tau));

    if has_coplanar {
        return classify_coplanar_nonconvex(face, opposing, tau, cache);
    }

    // ── Non-coplanar path: progressive splitting ─────────────────────────
    //
    // Split the face by each opposing face plane that straddles it.
    // Uses `clip_polygon_by_plane` (same as S-H and coplanar path) to
    // ensure exact vertex positions match adjacent face fragments.
    // Fragment classification uses `point_in_solid`.

    let mut cutting_planes: Vec<([f64; 3], [f64; 3])> = Vec::new();

    for opp in opposing {
        if is_coplanar(face.normal, face.verts[0], opp, tau) {
            continue;
        }

        // Straddle check: face must have vertices on both sides of the plane
        let mut has_pos = false;
        let mut has_neg = false;
        for v in &face.verts {
            let d = v3_dot(v3_sub(*v, opp.origin), opp.normal);
            if d > tau {
                has_pos = true;
            }
            if d < -tau {
                has_neg = true;
            }
        }
        if has_pos && has_neg {
            cutting_planes.push((opp.origin, v3_negate(opp.normal)));
        }
    }

    let inward_offset = v3_scale(face.normal, -tau * 100.0);

    if cutting_planes.is_empty() {
        // No planes straddle — classify centroid
        let centroid = polygon_centroid(&face.verts);
        let sample = v3_add(centroid, inward_offset);
        if point_in_solid(sample, opposing) {
            return FaceClass::Inside;
        }
        return FaceClass::Outside;
    }

    // Progressive splitting: split face by each cutting plane,
    // keeping BOTH halves at each step.
    // Cap fragment count to prevent exponential blowup (2^N planes).
    // 2048 handles gear profiles (~560 faces) where cylinder caps need
    // splitting by many tangent planes from the gear's tooth edges.
    const MAX_FRAGMENTS: usize = 2048;
    let mut fragments: Vec<Vec<[f64; 3]>> = vec![face.verts.clone()];

    for (plane_pt, inward_n) in &cutting_planes {
        if fragments.len() >= MAX_FRAGMENTS {
            break; // Stop splitting — classify remaining fragments by centroid
        }
        let outward_n = v3_negate(*inward_n);
        let mut new_fragments = Vec::new();
        for frag in &fragments {
            let half_in =
                clip_polygon_by_plane_cached(frag, *plane_pt, *inward_n, tau, cache.as_mut());
            let half_out =
                clip_polygon_by_plane_cached(frag, *plane_pt, outward_n, tau, cache.as_mut());
            if half_in.len() >= 3 && polygon_area_3d(&half_in) > TAU_NORMALIZE {
                new_fragments.push(half_in);
            }
            if half_out.len() >= 3 && polygon_area_3d(&half_out) > TAU_NORMALIZE {
                new_fragments.push(half_out);
            }
        }
        fragments = new_fragments;
    }

    // Classify each fragment with point_in_solid
    let mut inside_frags: Vec<Vec<[f64; 3]>> = Vec::new();
    let mut outside_frags: Vec<Vec<[f64; 3]>> = Vec::new();

    for frag in fragments {
        let centroid = polygon_centroid(&frag);
        let sample = v3_add(centroid, inward_offset);
        if point_in_solid(sample, opposing) {
            inside_frags.push(frag);
        } else {
            outside_frags.push(frag);
        }
    }

    if inside_frags.is_empty() {
        return FaceClass::Outside;
    }
    if outside_frags.is_empty() {
        return FaceClass::Inside;
    }

    // Retain ALL inside fragments — no largest-fragment heuristic.
    // GWN-based point_in_solid is robust enough that small fragments
    // are correctly classified; discarding them creates missing face regions.
    FaceClass::Partial {
        inside_frags,
        outside_frags,
    }
}

/// Classify a coplanar face against a non-convex opposing solid.
///
/// Uses vertex-based classification (same as the non-coplanar path).
/// Each vertex is classified via `point_in_solid`, and edge crossings
/// are found analytically or via binary search.
fn classify_coplanar_nonconvex(
    face: &FacePoly,
    opposing: &[FacePoly],
    tau: f64,
    cache: &mut Option<IntersectionCache>,
) -> FaceClass {
    // For coplanar faces, use progressive splitting by opposing side face
    // planes (straddle-only check — no face-face intersection needed because
    // all perpendicular planes that straddle the coplanar face are relevant).
    // This produces EXACT vertex positions matching the S-H splits on the
    // opposing coplanar face, preventing boundary vertex mismatches.
    let mut cutting_planes: Vec<([f64; 3], [f64; 3])> = Vec::new();

    for opp in opposing {
        if is_coplanar(face.normal, face.verts[0], opp, tau) {
            continue;
        }

        // Straddle check: face must have vertices on both sides of the plane
        let mut has_pos = false;
        let mut has_neg = false;
        for v in &face.verts {
            let d = v3_dot(v3_sub(*v, opp.origin), opp.normal);
            if d > tau {
                has_pos = true;
            }
            if d < -tau {
                has_neg = true;
            }
        }
        if has_pos && has_neg {
            cutting_planes.push((opp.origin, v3_negate(opp.normal)));
        }
    }

    let inward_offset = v3_scale(face.normal, -tau * 100.0);

    if cutting_planes.is_empty() {
        // No non-coplanar faces cut us — fully inside or outside
        let centroid = polygon_centroid(&face.verts);
        let sample = v3_add(centroid, inward_offset);
        if point_in_solid(sample, opposing) {
            return FaceClass::CoplanarPartial {
                inside_frags: vec![face.verts.clone()],
                outside_frags: vec![],
            };
        }
        return FaceClass::Outside;
    }

    // Progressive splitting by non-coplanar opposing face planes
    const MAX_FRAGMENTS: usize = 512;
    let mut fragments: Vec<Vec<[f64; 3]>> = vec![face.verts.clone()];

    for (plane_pt, inward_n) in &cutting_planes {
        if fragments.len() >= MAX_FRAGMENTS {
            break;
        }
        let outward_n = v3_negate(*inward_n);
        let mut new_fragments = Vec::new();
        for frag in &fragments {
            let half_in =
                clip_polygon_by_plane_cached(frag, *plane_pt, *inward_n, tau, cache.as_mut());
            let half_out =
                clip_polygon_by_plane_cached(frag, *plane_pt, outward_n, tau, cache.as_mut());
            if half_in.len() >= 3 && polygon_area_3d(&half_in) > TAU_NORMALIZE {
                new_fragments.push(half_in);
            }
            if half_out.len() >= 3 && polygon_area_3d(&half_out) > TAU_NORMALIZE {
                new_fragments.push(half_out);
            }
        }
        fragments = new_fragments;
    }

    // Classify each fragment using point_in_solid (offset inward from the face)
    let mut inside_frags: Vec<Vec<[f64; 3]>> = Vec::new();
    let mut outside_frags: Vec<Vec<[f64; 3]>> = Vec::new();

    for frag in fragments {
        let centroid = polygon_centroid(&frag);
        let sample = v3_add(centroid, inward_offset);
        if point_in_solid(sample, opposing) {
            inside_frags.push(frag);
        } else {
            outside_frags.push(frag);
        }
    }

    if inside_frags.is_empty() {
        return FaceClass::Outside;
    }

    let original_area = polygon_area_3d(&face.verts);
    let inside_total_area: f64 = inside_frags.iter().map(|f| polygon_area_3d(f)).sum();
    if (inside_total_area - original_area).abs() / original_area < MIN_FEATURE_SIZE {
        return FaceClass::CoplanarPartial {
            inside_frags: vec![face.verts.clone()],
            outside_frags: vec![],
        };
    }

    if outside_frags.is_empty() {
        return FaceClass::CoplanarPartial {
            inside_frags: vec![face.verts.clone()],
            outside_frags: vec![],
        };
    }

    // Retain ALL inside fragments — no largest-fragment heuristic.
    FaceClass::CoplanarPartial {
        inside_frags,
        outside_frags,
    }
}

/// Classify an anti-parallel coplanar face against a non-convex opposing solid.
///
/// Identical logic to `classify_coplanar_nonconvex` but returns
/// `AntiParallelCoplanarPartial` instead of `CoplanarPartial`, and
/// `CoplanarTouching` for degenerate cases (no cutting planes, fully inside).
fn classify_coplanar_nonconvex_antiparallel(
    face: &FacePoly,
    opposing: &[FacePoly],
    tau: f64,
    cache: &mut Option<IntersectionCache>,
) -> FaceClass {
    let mut cutting_planes: Vec<([f64; 3], [f64; 3])> = Vec::new();

    for opp in opposing {
        if is_coplanar(face.normal, face.verts[0], opp, tau) {
            continue;
        }

        // Straddle check: face must have vertices on both sides of the plane
        let mut has_pos = false;
        let mut has_neg = false;
        for v in &face.verts {
            let d = v3_dot(v3_sub(*v, opp.origin), opp.normal);
            if d > tau {
                has_pos = true;
            }
            if d < -tau {
                has_neg = true;
            }
        }
        if has_pos && has_neg {
            cutting_planes.push((opp.origin, v3_negate(opp.normal)));
        }
    }

    let inward_offset = v3_scale(face.normal, -tau * 100.0);

    if cutting_planes.is_empty() {
        // No non-coplanar faces straddle us — no splitting possible.
        // The face is anti-parallel coplanar with at least one opposing face,
        // so treat it as a shared boundary (CoplanarTouching).
        return FaceClass::CoplanarTouching;
    }

    // Progressive splitting by non-coplanar opposing face planes
    const MAX_FRAGMENTS: usize = 2048;
    let mut fragments: Vec<Vec<[f64; 3]>> = vec![face.verts.clone()];

    for (plane_pt, inward_n) in &cutting_planes {
        if fragments.len() >= MAX_FRAGMENTS {
            break;
        }
        let outward_n = v3_negate(*inward_n);
        let mut new_fragments = Vec::new();
        for frag in &fragments {
            let half_in =
                clip_polygon_by_plane_cached(frag, *plane_pt, *inward_n, tau, cache.as_mut());
            let half_out =
                clip_polygon_by_plane_cached(frag, *plane_pt, outward_n, tau, cache.as_mut());
            if half_in.len() >= 3 && polygon_area_3d(&half_in) > TAU_NORMALIZE {
                new_fragments.push(half_in);
            }
            if half_out.len() >= 3 && polygon_area_3d(&half_out) > TAU_NORMALIZE {
                new_fragments.push(half_out);
            }
        }
        fragments = new_fragments;
    }

    // Classify each fragment using point_in_solid
    let mut inside_frags: Vec<Vec<[f64; 3]>> = Vec::new();
    let mut outside_frags: Vec<Vec<[f64; 3]>> = Vec::new();

    for frag in fragments {
        let centroid = polygon_centroid(&frag);
        let sample = v3_add(centroid, inward_offset);
        if point_in_solid(sample, opposing) {
            inside_frags.push(frag);
        } else {
            outside_frags.push(frag);
        }
    }

    if inside_frags.is_empty() || outside_frags.is_empty() {
        // No meaningful split: either fully outside or fully inside the
        // opposing solid. For anti-parallel coplanar faces, this means the
        // face is a shared boundary → CoplanarTouching.
        return FaceClass::CoplanarTouching;
    }

    let original_area = polygon_area_3d(&face.verts);
    let inside_total_area: f64 = inside_frags.iter().map(|f| polygon_area_3d(f)).sum();
    if (inside_total_area - original_area).abs() / original_area < MIN_FEATURE_SIZE {
        // Fully inside: degenerate, same as CoplanarTouching
        return FaceClass::CoplanarTouching;
    }

    FaceClass::AntiParallelCoplanarPartial {
        inside_frags,
        outside_frags,
    }
}

/// Split a face polygon by the opposing solid's planes, collecting all
/// convex outside fragments. As we clip progressively against each plane,
/// the piece that falls outside that plane is a valid outside fragment
/// (it's beyond at least one of the opposing solid's half-spaces).
pub(super) fn split_outside_fragments(
    face_verts: &[[f64; 3]],
    opposing: &[FacePoly],
    tau: f64,
    face_normal: Option<[f64; 3]>,
    cache: &mut Option<IntersectionCache>,
) -> Vec<Vec<[f64; 3]>> {
    let mut current = face_verts.to_vec();
    let mut outside_frags = Vec::new();

    for opp_face in opposing {
        if current.is_empty() {
            break;
        }

        // Skip coplanar opposing faces
        if let Some(fn_normal) = face_normal {
            if is_coplanar(fn_normal, current[0], opp_face, tau) {
                continue;
            }
        }

        // Inward normal = negation of the face's outward normal
        let inward = v3_negate(opp_face.normal);

        // Clip to keep inside portion (for continuing the iteration)
        let inside_part =
            clip_polygon_by_plane_cached(&current, opp_face.origin, inward, tau, cache.as_mut());

        // Clip to keep outside portion (on the outward side of this plane)
        let outside_part = clip_polygon_by_plane_cached(
            &current,
            opp_face.origin,
            opp_face.normal,
            tau,
            cache.as_mut(),
        );

        if outside_part.len() >= 3 && polygon_area_3d(&outside_part) > TAU_NORMALIZE {
            outside_frags.push(outside_part);
        }

        current = inside_part;
    }

    outside_frags
}
