//! Exact signed area of a closed PLANAR loop whose edges are line segments,
//! circular arcs, ellipse arcs and hyperbola arcs — the orientation oracle
//! for planar faces (`from_yang` step 1d, `validate_planar_face`).
//!
//! Why not a sampled polygon: the previous oracle augmented the vertex
//! polygon with ONE parametric midpoint per arc (PR-KV9 / KV11 / KV16) and
//! took its Newell normal. That polygon's sagitta under a long arc is
//! `R(1 − cos(sweep/4))` — 0.13·R for a 118° arc — so a planar region THINNER
//! than that between a long convex arc and a nearby concave one (the crescent
//! cap of a grazing parallel-cylinder boolean, thickness 0.10·R at a 10.7°
//! crossing) sampled to a self-crossing polygon whose Newell normal pointed
//! the wrong way, and a CORRECT output was refused (`InvalidBooleanOutput`
//! "plane normal disagrees with its outer-loop Newell normal"). Finer
//! sampling only moves the threshold; the region can be arbitrarily thin.
//!
//! The area is instead the exact line integral `½∮ n̂·(p × dp)`: the vertex
//! chords contribute the shoelace sum, and every curved edge contributes the
//! closed-form area between its chord and the curve — the circular segment
//! `(R²/2)(θ − sin θ)`, its ellipse analog `(ab/2)(θ − sin θ)` in the
//! parametric angle, and the hyperbola's `(ab/2)(θ − sinh θ)` (negative: the
//! branch dips TOWARD its centre relative to the chord) — each signed by the
//! traversal sense about the face normal. No sampling, no threshold.

use super::{ccw_sweep, ellipse_ccw_sweep, hyperbola_param};
use cad_primitives::Point3;

/// One loop edge's curve, in the frame the edge itself carries. `normal` is
/// the edge's own directional axis (CCW sense of ITS parametrization); the
/// face normal decides the sign.
#[derive(Clone, Copy, Debug)]
pub(crate) enum LoopEdgeCurve {
    /// Straight chord — no segment term.
    Line,
    Circle {
        center: Point3,
        normal: [f64; 3],
        radius: f64,
    },
    Ellipse {
        center: Point3,
        normal: [f64; 3],
        major_axis: [f64; 3],
        major_radius: f64,
        minor_radius: f64,
    },
    Hyperbola {
        center: Point3,
        normal: [f64; 3],
        major_axis: [f64; 3],
        semi_transverse: f64,
        semi_conjugate: f64,
    },
}

/// Exact signed area of the closed loop `pts[0] → pts[1] → … → pts[0]`
/// whose edge `k` (from `pts[k]` to `pts[k+1 mod n]`) follows `curves[k]`,
/// measured counter-clockwise about `plane_normal` (positive = the loop
/// winds CCW around the normal, i.e. is an OUTER loop of a face with that
/// normal). `None` when a curved edge is degenerate (an endpoint with no
/// radial component, or a non-positive semi-axis) or the loop has fewer than
/// two points; a zero return is a genuinely degenerate loop.
///
/// Chords are taken relative to `pts[0]` so a loop far from the origin does
/// not lose its (small) area to cancellation.
pub(crate) fn planar_loop_signed_area(
    plane_normal: [f64; 3],
    pts: &[Point3],
    curves: &[LoopEdgeCurve],
) -> Option<f64> {
    let n = pts.len();
    if n < 2 || curves.len() != n {
        return None;
    }
    let nn = plane_normal;
    let o = pts[0];
    let rel = |p: Point3| -> [f64; 3] { [p.x() - o.x(), p.y() - o.y(), p.z() - o.z()] };
    let mut twice_area = 0.0f64;
    for k in 0..n {
        let p0 = pts[k];
        let p1 = pts[(k + 1) % n];
        let (a, b) = (rel(p0), rel(p1));
        // Chord term: n̂·(a × b).
        twice_area += nn[0] * (a[1] * b[2] - a[2] * b[1])
            + nn[1] * (a[2] * b[0] - a[0] * b[2])
            + nn[2] * (a[0] * b[1] - a[1] * b[0]);
        // Segment term: the area between the chord and the curve, signed by
        // the traversal sense about n̂ (`s`): a sweep that is CCW in the
        // edge's own frame is CCW about n̂ iff the two normals agree.
        let sense = |edge_normal: [f64; 3]| -> f64 {
            let d = edge_normal[0] * nn[0] + edge_normal[1] * nn[1] + edge_normal[2] * nn[2];
            if d >= 0.0 {
                1.0
            } else {
                -1.0
            }
        };
        match curves[k] {
            LoopEdgeCurve::Line => {}
            LoopEdgeCurve::Circle {
                center,
                normal,
                radius,
            } => {
                let sweep = ccw_sweep(center, normal, p0, p1)?;
                let theta = sense(normal) * sweep;
                twice_area += radius * radius * (theta - theta.sin());
            }
            LoopEdgeCurve::Ellipse {
                center,
                normal,
                major_axis,
                major_radius,
                minor_radius,
            } => {
                let sweep = ellipse_ccw_sweep(
                    center,
                    normal,
                    major_axis,
                    major_radius,
                    minor_radius,
                    p0,
                    p1,
                )?;
                let theta = sense(normal) * sweep;
                twice_area += major_radius * minor_radius * (theta - theta.sin());
            }
            LoopEdgeCurve::Hyperbola {
                center,
                normal,
                major_axis,
                semi_transverse,
                semi_conjugate,
            } => {
                let t0 = hyperbola_param(center, normal, major_axis, semi_conjugate, p0)?;
                let t1 = hyperbola_param(center, normal, major_axis, semi_conjugate, p1)?;
                let theta = sense(normal) * (t1 - t0);
                twice_area += semi_transverse * semi_conjugate * (theta - theta.sinh());
            }
        }
    }
    Some(0.5 * twice_area)
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::f64::consts::PI;

    const Z: [f64; 3] = [0.0, 0.0, 1.0];
    const NEG_Z: [f64; 3] = [0.0, 0.0, -1.0];

    fn circle(center: Point3, normal: [f64; 3], radius: f64) -> LoopEdgeCurve {
        LoopEdgeCurve::Circle {
            center,
            normal,
            radius,
        }
    }

    #[test]
    fn unit_square_ccw_is_one_and_cw_is_minus_one() {
        let pts = [
            Point3::new(0.0, 0.0, 0.0),
            Point3::new(1.0, 0.0, 0.0),
            Point3::new(1.0, 1.0, 0.0),
            Point3::new(0.0, 1.0, 0.0),
        ];
        let lines = [LoopEdgeCurve::Line; 4];
        let a = planar_loop_signed_area(Z, &pts, &lines).unwrap();
        assert!((a - 1.0).abs() < 1e-15, "{a}");
        let mut rev = pts;
        rev.reverse();
        let b = planar_loop_signed_area(Z, &rev, &lines).unwrap();
        assert!((b + 1.0).abs() < 1e-15, "{b}");
        // The same walk seen from the other side of the plane.
        let c = planar_loop_signed_area(NEG_Z, &pts, &lines).unwrap();
        assert!((c + 1.0).abs() < 1e-15, "{c}");
    }

    /// A disc as two half-circle arcs: exactly π. The arc term is what
    /// separates this from the zero-area two-point chord polygon.
    #[test]
    fn two_half_arcs_enclose_pi() {
        let c = Point3::new(0.0, 0.0, 0.0);
        let pts = [Point3::new(1.0, 0.0, 0.0), Point3::new(-1.0, 0.0, 0.0)];
        let curves = [circle(c, Z, 1.0), circle(c, Z, 1.0)];
        let a = planar_loop_signed_area(Z, &pts, &curves).unwrap();
        assert!((a - PI).abs() < 1e-14, "{a}");
        // Traversed CW about +z (the arcs' own frame says −z).
        let curves_cw = [circle(c, NEG_Z, 1.0), circle(c, NEG_Z, 1.0)];
        let b = planar_loop_signed_area(Z, &pts, &curves_cw).unwrap();
        assert!((b + PI).abs() < 1e-14, "{b}");
    }

    /// A major arc (270°) closed by its chord: the segment is more than half
    /// the disc, `(R²/2)(θ − sin θ)` with `sin θ < 0`.
    #[test]
    fn major_arc_segment_is_exact() {
        let c = Point3::new(0.0, 0.0, 0.0);
        let pts = [Point3::new(1.0, 0.0, 0.0), Point3::new(0.0, -1.0, 0.0)];
        let curves = [circle(c, Z, 1.0), LoopEdgeCurve::Line];
        let a = planar_loop_signed_area(Z, &pts, &curves).unwrap();
        let theta = 1.5 * PI;
        let expect = 0.5 * (theta - theta.sin());
        assert!((a - expect).abs() < 1e-14, "{a} vs {expect}");
    }

    /// The crescent that the midpoint-sampled Newell oracle refused
    /// (`cyl_cyl_grazing_ruling_sweep` at δ = 0.25): A's 118° arc from the
    /// upper ruling through (−1, 0) to the lower one, then back along B's
    /// circle (centre (0.25, 0), r 1.15) CW about +z. The region is 0.10
    /// thick under an arc whose one-midpoint polygon sags 0.13, and its exact
    /// area is the disc minus the lens — POSITIVE about +z.
    #[test]
    fn thin_crescent_under_a_long_arc_is_positive() {
        let (r_a, r_b, d) = (1.0f64, 1.15f64, 0.25f64);
        let x = (r_a * r_a - r_b * r_b + d * d) / (2.0 * d);
        let y = (r_a * r_a - x * x).sqrt();
        let top = Point3::new(x, y, 1.0);
        let bottom = Point3::new(x, -y, 1.0);
        let ca = Point3::new(0.0, 0.0, 1.0);
        let cb = Point3::new(d, 0.0, 1.0);
        // The output B-Rep carried B's arc as two pieces through an on-curve
        // vertex at B-azimuth ≈ 183° (the probe's v24); the area is
        // additive, so keep it that way.
        let b_at = |az: f64| Point3::new(d + r_b * az.cos(), r_b * az.sin(), 1.0);
        let az_mid_b = (-0.061425885460412735f64).atan2(-0.8983583328366647 - d);
        let mid_b = b_at(az_mid_b);
        let pts = [top, bottom, mid_b];
        let curves = [
            circle(ca, Z, r_a),
            circle(cb, NEG_Z, r_b),
            circle(cb, NEG_Z, r_b),
        ];
        let a = planar_loop_signed_area(Z, &pts, &curves).unwrap();
        // Lens area of the two discs.
        let a1 = ((d * d + r_a * r_a - r_b * r_b) / (2.0 * d * r_a)).acos();
        let a2 = ((d * d + r_b * r_b - r_a * r_a) / (2.0 * d * r_b)).acos();
        let k =
            0.5 * ((-d + r_a + r_b) * (d + r_a - r_b) * (d - r_a + r_b) * (d + r_a + r_b)).sqrt();
        let lens = r_a * r_a * a1 + r_b * r_b * a2 - k;
        let expect = PI * r_a * r_a - lens;
        assert!(a > 0.0, "crescent read as CW: {a}");
        assert!((a - expect).abs() < 1e-12, "{a} vs {expect}");
        // And the SAMPLED polygon that used to stand in for it really is CW —
        // the false reject pinned.
        let arc_mid = Point3::new(-1.0, 0.0, 1.0);
        // B's azimuths in (−π, π]: bottom ≈ −132°, mid_b ≈ −177°, top ≈
        // +132°; the CW walk bottom → mid_b → top passes through ±180°
        // between mid_b and top.
        let az_bottom = (-y).atan2(x - d);
        let az_top = y.atan2(x - d);
        let sampled = [
            top,
            arc_mid,
            bottom,
            b_at(0.5 * (az_bottom + az_mid_b)),
            mid_b,
            b_at(0.5 * (az_mid_b + (az_top - 2.0 * PI))),
        ];
        let nw = super::super::newell(&sampled);
        assert!(nw[2] < 0.0, "the midpoint polygon should mis-sign: {nw:?}");
    }

    /// An ellipse traversed as two parametric half-arcs: exactly π·a·b.
    #[test]
    fn two_half_ellipse_arcs_enclose_pi_ab() {
        let (a, b) = (2.0f64, 0.5f64);
        let c = Point3::new(3.0, -1.0, 0.0);
        let curve = LoopEdgeCurve::Ellipse {
            center: c,
            normal: Z,
            major_axis: [1.0, 0.0, 0.0],
            major_radius: a,
            minor_radius: b,
        };
        let pts = [
            Point3::new(3.0 + a, -1.0, 0.0),
            Point3::new(3.0 - a, -1.0, 0.0),
        ];
        let area = planar_loop_signed_area(Z, &pts, &[curve, curve]).unwrap();
        assert!((area - PI * a * b).abs() < 1e-13, "{area}");
    }

    /// A hyperbola arc closed by its chord: the arc dips toward the centre,
    /// so the region between chord and arc has area `(ab/2)(sinh θ − θ)`
    /// and lies on the centre's side of the chord. Walking UP the chord
    /// (x = a·cosh t, the region's right edge) and back DOWN the branch
    /// through (a, 0) on its left is CCW about +z: positive.
    #[test]
    fn hyperbola_segment_sign_and_magnitude() {
        let (a, b) = (1.0f64, 0.5f64);
        let c = Point3::new(0.0, 0.0, 0.0);
        let curve = LoopEdgeCurve::Hyperbola {
            center: c,
            normal: Z,
            major_axis: [1.0, 0.0, 0.0],
            semi_transverse: a,
            semi_conjugate: b,
        };
        let t = 1.0f64;
        let lo = Point3::new(a * t.cosh(), -b * t.sinh(), 0.0);
        let hi = Point3::new(a * t.cosh(), b * t.sinh(), 0.0);
        // lo → hi by the chord (up the line x = a·cosh t), hi → lo along the
        // branch, which passes through (a, 0), LEFT of the chord: the region
        // is to the left of the walk ⇒ CCW ⇒ positive.
        let pts = [lo, hi];
        let curves = [LoopEdgeCurve::Line, curve];
        let area = planar_loop_signed_area(Z, &pts, &curves).unwrap();
        let theta = 2.0 * t;
        let expect = 0.5 * a * b * (theta.sinh() - theta);
        assert!((area - expect).abs() < 1e-13, "{area} vs {expect}");
        // The reverse walk (up the branch, down the chord) is CW.
        let rev = planar_loop_signed_area(Z, &[lo, hi], &[curve, LoopEdgeCurve::Line]).unwrap();
        assert!((rev + expect).abs() < 1e-13, "{rev} vs {}", -expect);
    }
}
