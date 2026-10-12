//! Stage-1 UNIVERSAL-COVER unroll of a sheared bounded lateral patch (spec
//! `yang_stage1_curved_holed_patch` §"Slice I"; P0038, 2026-10-12).
//!
//! A cylinder lateral bounded by two rim arcs and two oblique side walls
//! that each advance 40° in azimuth over the height: the bottom rim spans
//! 0° … 340°, the top rim 40° … 380°, so the complement wedge rotates from
//! (340°, 360°) at the bottom to (20°, 40°) at the top and NO generator
//! meridian misses the face. The outer loop nets zero winding but its
//! unwrapped azimuth range is 380° ≥ 2π — the single-sheet STOP's class
//! (P0038's face 15 spanned 7.5551 rad: a cylinder boss lateral between two
//! oblique prism bosses). Every circle of latitude meets the face in 340°,
//! so the face is an embedded disc; its lift to the universal cover is a
//! simple chart polygon and the CDT runs there.

use super::*;
use crate::stage1_tessellate::COVER_UNROLL_OVERRIDE;
use std::f64::consts::PI;

const R: f64 = 1.0;
const H: f64 = 2.0;
/// Each side wall advances this much azimuth from bottom to top.
const SHEAR_DEG: f64 = 40.0;
/// The bottom rim spans [0°, BAND_DEG]; the top rim [SHEAR, SHEAR + BAND].
const BAND_DEG: f64 = 340.0;

fn deg(d: f64) -> f64 {
    d.to_radians()
}

/// The oblique plane `z = H/2 + k·(d · (x, y))` through the cylinder, with
/// `d = (cos φ, sin φ)`: its section is the ellipse `(r cos θ, r sin θ,
/// H/2 + k r cos(θ − φ))`. The wall from `(θ_lo, 0)` to `(θ_lo + SHEAR, H)`
/// is the arc of the plane symmetric about `θ_lo + SHEAR/2`.
fn side_wall(theta_lo_deg: f64) -> (Curve, f64) {
    let phi = deg(theta_lo_deg + 0.5 * SHEAR_DEG + 90.0);
    // z(θ_lo) = 0 ⇒ H/2 + k r cos(−SHEAR/2 − 90°) = 0 ⇒ k = H / (2 r sin(SHEAR/2)).
    let k = H / (2.0 * R * deg(0.5 * SHEAR_DEG).sin());
    let s = (1.0 + k * k).sqrt();
    let d = [phi.cos(), phi.sin()];
    let curve = Curve::Ellipse {
        center: Point3::new(0.0, 0.0, 0.5 * H),
        normal: Vector3::new(-k * d[0] / s, -k * d[1] / s, 1.0 / s),
        major_axis: Vector3::new(d[0] / s, d[1] / s, k / s),
        major_radius: s * R,
        minor_radius: R,
    };
    (curve, phi)
}

fn wall_point(theta_deg: f64, phi: f64) -> Point3 {
    let t = deg(theta_deg);
    let k = H / (2.0 * R * deg(0.5 * SHEAR_DEG).sin());
    Point3::new(R * t.cos(), R * t.sin(), 0.5 * H + k * R * (t - phi).cos())
}

fn rim_point(theta_deg: f64, z: f64) -> Point3 {
    let t = deg(theta_deg);
    Point3::new(R * t.cos(), R * t.sin(), z)
}

fn rim_arc(z: f64, start: u32, end: u32) -> BRepEdge {
    BRepEdge {
        start,
        end,
        curve: Curve::Circle {
            center: Point3::new(0.0, 0.0, z),
            normal: Vector3::new(0.0, 0.0, 1.0),
            radius: R,
        },
    }
}

/// The sheared band as a lone face. Vertices: a (0°, 0), b (170°, 0),
/// c (340°, 0), d (20° ≡ 380°, H), e (210°, H), f (40°, H). Edges: bottom
/// arcs a→b, b→c; wall S1 c→d; top arcs f→e, e→d; wall S2 a→f. Loop:
/// a b c d (e) f a.
fn sheared_band() -> (Vec<BRepVertex>, Vec<BRepEdge>, Vec<BRepFace>) {
    let (s1, phi1) = side_wall(BAND_DEG);
    let (s2, phi2) = side_wall(0.0);
    let a = wall_point(0.0, phi2);
    let f = wall_point(SHEAR_DEG, phi2);
    let c = wall_point(BAND_DEG, phi1);
    let d = wall_point(BAND_DEG + SHEAR_DEG, phi1);
    assert!(a.as_array()[2].abs() < 1e-12 && c.as_array()[2].abs() < 1e-12);
    assert!((f.as_array()[2] - H).abs() < 1e-12 && (d.as_array()[2] - H).abs() < 1e-12);
    let verts = vec![
        BRepVertex { point: a },
        BRepVertex {
            point: rim_point(170.0, 0.0),
        },
        BRepVertex { point: c },
        BRepVertex { point: d },
        BRepVertex {
            point: rim_point(210.0, H),
        },
        BRepVertex { point: f },
    ];
    let edges = vec![
        rim_arc(0.0, 0, 1),
        rim_arc(0.0, 1, 2),
        BRepEdge {
            start: 2,
            end: 3,
            curve: s1,
        },
        rim_arc(H, 5, 4),
        rim_arc(H, 4, 3),
        BRepEdge {
            start: 0,
            end: 5,
            curve: s2,
        },
    ];
    let faces = vec![BRepFace {
        surface: Surface::Cylinder {
            axis_point: Point3::new(0.0, 0.0, 0.0),
            axis_dir: Vector3::new(0.0, 0.0, 1.0),
            radius: R,
        },
        outer_loop: vec![0, 1, 2, 4, 3, 5],
        inner_loops: vec![],
        reversed: false,
    }];
    (verts, edges, faces)
}

fn with_cover<T>(on: bool, f: impl FnOnce() -> T) -> T {
    COVER_UNROLL_OVERRIDE.with(|c| c.set(Some(on)));
    let out = f();
    COVER_UNROLL_OVERRIDE.with(|c| c.set(None));
    out
}

fn tri_area(p: Point3, q: Point3, r: Point3) -> f64 {
    let (p, q, r) = (p.as_array(), q.as_array(), r.as_array());
    let u = [q[0] - p[0], q[1] - p[1], q[2] - p[2]];
    let v = [r[0] - p[0], r[1] - p[1], r[2] - p[2]];
    let c = [
        u[1] * v[2] - u[2] * v[1],
        u[2] * v[0] - u[0] * v[2],
        u[0] * v[1] - u[1] * v[0],
    ];
    0.5 * (c[0] * c[0] + c[1] * c[1] + c[2] * c[2]).sqrt()
}

#[test]
fn a_sheared_band_is_the_single_sheet_stop_without_the_cover_unroll() {
    let (verts, edges, faces) = sheared_band();
    let text = match with_cover(false, || stage1_tessellate(&verts, &edges, &faces)) {
        Err(e) => format!("{e}"),
        Ok(_) => panic!("no generator meridian misses a 380°-unwrapped band"),
    };
    assert!(
        text.contains("rad of azimuth unwrapped (≥ 2π): no seam wedge"),
        "{text}"
    );
    assert!(text.contains("6.6323"), "span 380° = 6.6323 rad: {text}");
}

#[test]
fn a_sheared_band_unrolls_in_the_universal_cover() {
    let (verts, edges, faces) = sheared_band();
    let tess = with_cover(true, || stage1_tessellate(&verts, &edges, &faces))
        .expect("the cover unroll tessellates the band");
    let range = tess.face_tri_ranges[0].clone();
    assert!(range.len() >= 4, "{} triangles", range.len());
    let cyl = faces[0].surface;
    let theta = |p: Point3| {
        let a = p.as_array();
        a[1].atan2(a[0])
    };
    let mut area = 0.0;
    for &t in &tess.tris[range.clone()] {
        let pts = t.map(|v| tess.verts[v as usize]);
        for (k, p) in pts.iter().enumerate() {
            let d = signed_distance_to_surface(cyl, *p).unwrap();
            assert!(d.abs() < 1e-9, "vertex {} off the cylinder: {d:e}", t[k]);
            let z = p.as_array()[2];
            assert!(
                (-1e-9..=H + 1e-9).contains(&z),
                "vertex {} outside the band's height",
                t[k]
            );
        }
        // No chord of the lift spans more than a quarter turn: a folded
        // single-sheet unroll would join the two rims' far ends across the
        // seam wedge.
        for k in 0..3 {
            let span = azimuth_span(theta(pts[k]), theta(pts[(k + 1) % 3]));
            assert!(
                span < 0.5 * PI,
                "chord spans {:.1}° of azimuth",
                span.to_degrees()
            );
        }
        area += tri_area(pts[0], pts[1], pts[2]);
    }
    // The band is 340° wide on every circle of latitude: area r·H·(340° in
    // rad), minus the chords' sagitta loss (under 2 % at the rim density).
    let exact = R * H * deg(BAND_DEG);
    assert!(
        area > 0.97 * exact && area <= exact * (1.0 + 1e-9),
        "band area {area} vs exact {exact}"
    );
    // Each triangle lies inside the face: its centroid's azimuth is inside
    // the band at its height (the complement wedge is 20° wide and rotates
    // 40° from bottom to top).
    for &t in &tess.tris[range] {
        let pts = t.map(|v| tess.verts[v as usize]);
        let c = pts.iter().fold([0.0; 3], |acc, p| {
            let a = p.as_array();
            [
                acc[0] + a[0] / 3.0,
                acc[1] + a[1] / 3.0,
                acc[2] + a[2] / 3.0,
            ]
        });
        let th = c[1].atan2(c[0]).rem_euclid(2.0 * PI);
        let frac = (c[2] / H).clamp(0.0, 1.0);
        // The wedge at this height: [BAND + frac·SHEAR, 360 + frac·SHEAR].
        let w_lo = deg(BAND_DEG) + frac * deg(SHEAR_DEG);
        let w_hi = 2.0 * PI + frac * deg(SHEAR_DEG);
        let in_wedge = {
            let x = if th < w_lo - PI { th + 2.0 * PI } else { th };
            x > w_lo + 1e-6 && x < w_hi - 1e-6
        };
        assert!(
            !in_wedge,
            "triangle centroid at {:.1}°, z {:.3} lies in the wedge",
            th.to_degrees(),
            c[2]
        );
    }
}

#[test]
fn unwrap_loop_lifts_steps_wrapped_to_a_half_turn() {
    let two_pi = 2.0 * PI;
    // A loop that advances 100° six times (600°, a helical turn and a bit).
    let raw: Vec<f64> = (0..7)
        .map(|k| (k as f64 * deg(100.0) + PI).rem_euclid(two_pi) - PI)
        .collect();
    let lifted = unwrap_loop(&raw);
    for (k, t) in lifted.iter().enumerate() {
        assert!(
            (t - (raw[0] + k as f64 * deg(100.0))).abs() < 1e-12,
            "{k}: {t}"
        );
    }
    // The empty loop and a one-point loop.
    assert!(unwrap_loop(&[]).is_empty());
    assert_eq!(unwrap_loop(&[0.3]), vec![0.3]);
    // Steps are wrapped to (−π, π]: a −350° raw step is a +10° lift.
    assert!((unwrap_loop(&[deg(175.0), deg(-175.0)])[1] - deg(185.0)).abs() < 1e-12);
}

#[test]
fn chart_point_in_polygon_is_even_odd() {
    let p = |x: f64, y: f64| cad_primitives::Point2::new(x, y);
    // A C-shaped polygon: (0,0)-(3,0)-(3,3)-(0,3)-(0,2)-(2,2)-(2,1)-(0,1).
    let poly = vec![
        p(0.0, 0.0),
        p(3.0, 0.0),
        p(3.0, 3.0),
        p(0.0, 3.0),
        p(0.0, 2.0),
        p(2.0, 2.0),
        p(2.0, 1.0),
        p(0.0, 1.0),
    ];
    assert!(chart_point_in_polygon(p(1.0, 0.5), &poly));
    assert!(chart_point_in_polygon(p(2.5, 1.5), &poly));
    assert!(!chart_point_in_polygon(p(1.0, 1.5), &poly), "the notch");
    assert!(!chart_point_in_polygon(p(4.0, 1.5), &poly));
    assert!(!chart_point_in_polygon(p(1.0, -0.5), &poly));
    assert!(
        !chart_point_in_polygon(p(1.0, 1.5), &poly[..2]),
        "a degenerate polygon holds nothing"
    );
}
