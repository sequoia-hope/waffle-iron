//! Exact geometric containment predicates used by the `boolean()` driver's
//! Stage-0 coplanar scan and PR-YR27 face-attribution tie-breaks — finite-
//! extent strict point-in-face tests (planar 2D-frame even-odd, cylinder
//! axial-span) and coincident-cylinder detection. Extracted verbatim from
//! `boolean.rs` (move-only, spec `specs/yang_rs_lib_decomposition.md` F9).

#[allow(clippy::wildcard_imports)]
use crate::*;

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
