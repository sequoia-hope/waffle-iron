//! Stage-4 per-triangle `d(T)` recompute (Yang 2025 §4.1.2 / Fig 6) — the
//! certified discretization-error bound for boundary triangles.
//!
//! Closes deviation **N2** (`docs/yang_deviations.md`) increment 2. Yang 2025
//! §4.4.1 ends: "For the newly generated boundary triangles around the
//! intersection curve, we recalculate `d(T)` to maintain controllable error"
//! (`refs/text/yang2025_hybrid_boolean.txt:568-571`). `d(T)` itself is defined
//! in §4.1.2 / Fig 6 (`refs/text/yang2025_hybrid_boolean.txt:340-378`): take
//! the minimal parametric rectangle covering the triangle's three `uv`
//! corners, build the surface sub-patch on that rectangle, and compute the
//! maximal distance between the sub-patch's **control points** and the 3D
//! triangle. The convex-hull property of the positive-weight rational Bézier
//! net certifies the result as an upper bound on the true patch-to-triangle
//! max distance — never an estimate (spec §7).
//!
//! Our surfaces are analytic quadrics; all four curved [`Surface`] variants
//! are surfaces of revolution with EXACT closed-form rational Bézier nets
//! [#32 Piegl & Tiller ch. 8], so one constructor covers them all (spec §3).
//!
//! Spec: `specs/n2_stage4_dt_recompute.md`. Scope of increment 2: the pure
//! `d(T)` primitive plus its pinned parametric embedding [`eval_uv`],
//! unit-tested in isolation — **not** wired into
//! `stage4_relocate_and_correct` (that is N2-3).

use crate::Surface;
use cad_primitives::{Point2, Point3};

/// Why `d(T)` / [`eval_uv`] failed (spec §6). Every variant is a P9/P10 LOUD
/// stop — no clamping, no silent legalization.
#[derive(Debug, Clone, PartialEq)]
pub enum DtError {
    /// Any NaN/∞ in `uv` (or in `eval_uv`'s point argument) or in the
    /// surface's fields.
    NonFiniteInput,
    /// `radius <= 0`; cone `half_angle ∉ (0, π/2)`; torus
    /// `major_radius <= minor_radius` or `minor_radius <= 0`; zero
    /// `axis_dir`/`normal`.
    InvalidSurface,
    /// Covering-rectangle u-span `> 2π` for a curved surface — the caller
    /// handed coordinates from more than one period, so the covering
    /// rectangle is ambiguous. Unwrapping is the caller's job.
    AzimuthSpanTooLarge,
    /// Sphere v-range not within `[−π/2, π/2]`.
    PolarRangeOutOfBounds,
    /// Cone with any `v < 0` (behind the apex; the single-nappe solid
    /// convention of [`Surface::Cone`]).
    NegativeConeAxialRange,
}

/// Evaluate the pinned parametric embedding of `surface` at `p = (u, v)`
/// (spec §2). This defines what the `uv` coordinates handed to [`d_of_t`]
/// MEAN — Stage-1 sampling, the N2-1 patches and the Fig-6 bound must share
/// this one convention. All frames use the deterministic
/// [`crate::ortho_basis`] `(e1, e2)` (PR-YR7).
///
/// | Surface | `u` | `v` | `eval_uv(u, v)` |
/// |---|---|---|---|
/// | `Plane { normal, d }` | in-plane `e1` coord | in-plane `e2` coord | `(−d)·n̂ + u·e1 + v·e2`, `(e1, e2) = ortho_basis(normal)` |
/// | `Cylinder` | azimuth θ (rad) | axial offset h (world units) | `axis_point + h·â + r(cos u·e1 + sin u·e2)`, `â = normalize(axis_dir)` |
/// | `Cone` | azimuth θ (rad) | axial distance t ≥ 0 from apex | `apex + v·â + v·tan(half_angle)(cos u·e1 + sin u·e2)` |
/// | `Sphere` | azimuth θ (rad) | latitude φ ∈ [−π/2, π/2] | `center + r(cos v cos u·e1 + cos v sin u·e2 + sin v·ẑ)`, frame `ortho_basis(ẑ)` with the pinned canonical axis `ẑ = (0, 0, 1)` |
/// | `Torus` | azimuth θ (rad) | tube angle φ (rad) | `center + (R + r cos v)(cos u·e1 + sin u·e2) + r sin v·â` |
///
/// # Errors
///
/// [`DtError::NonFiniteInput`], [`DtError::InvalidSurface`],
/// [`DtError::PolarRangeOutOfBounds`], [`DtError::NegativeConeAxialRange`].
pub fn eval_uv(surface: &Surface, p: Point2) -> Result<Point3, DtError> {
    let _ = (surface, p);
    todo!("N2-2 Implementer: pinned parametric embedding (spec §2)")
}

/// The Yang 2025 §4.1.2 / Fig 6 per-triangle discretization-error bound
/// `d(T)` for the triangle with parametric corners `uv` on `surface`
/// (spec §1/§3):
///
/// 1. Covering rectangle `[u0,u1]×[v0,v1]` of the three corners (Fig 6c);
///    degenerate rectangles are legal.
/// 2. Validate ranges (spec §6); subdivide so every sub-rectangle's angular
///    spans are ≤ π/2.
/// 3. Per sub-rectangle, build the exact rational Bézier control net of the
///    surface-of-revolution patch [#32 ch. 8].
/// 4. `d(T)` = max over ALL control points of all sub-rectangles of the
///    point-to-triangle distance to the 3D triangle
///    `[eval_uv(uv[0]), eval_uv(uv[1]), eval_uv(uv[2])]` (degenerate 3D
///    triangles degrade to segment/point distance — legal input).
///
/// The convex-hull property of the positive-weight rational net makes the
/// result a CERTIFIED upper bound on the true max distance from the surface
/// patch over the triangle's parametric footprint to the 3D triangle. A
/// `Plane` returns exactly `0.0`. Result is finite and `>= 0`, in world
/// units. Pure and deterministic; no tolerances — there is nothing to tune.
pub fn d_of_t(surface: &Surface, uv: [Point2; 3]) -> Result<f64, DtError> {
    let _ = (surface, uv);
    todo!("N2-2 Implementer: certified Fig-6 control-net bound (spec §3)")
}

#[cfg(test)]
mod tests {
    use super::*;
    use cad_primitives::Vector3;
    use std::f64::consts::PI;

    // ---- Surfaces under test (spec §5 oracles). --------------------------

    fn cyl_z() -> Surface {
        Surface::Cylinder {
            axis_point: Point3::new(0.0, 0.0, 0.0),
            axis_dir: Vector3::new(0.0, 0.0, 1.0),
            radius: 1.0,
        }
    }

    fn cone_z() -> Surface {
        Surface::Cone {
            apex: Point3::new(0.0, 0.0, 0.0),
            axis_dir: Vector3::new(0.0, 0.0, 1.0),
            half_angle: 0.5,
        }
    }

    fn sphere_off() -> Surface {
        Surface::Sphere {
            center: Point3::new(0.1, 0.2, 0.3),
            radius: 2.0,
        }
    }

    fn torus_z() -> Surface {
        Surface::Torus {
            center: Point3::new(0.0, 0.0, 0.0),
            axis_dir: Vector3::new(0.0, 0.0, 1.0),
            major_radius: 3.0,
            minor_radius: 1.0,
        }
    }

    fn uv(a: (f64, f64), b: (f64, f64), c: (f64, f64)) -> [Point2; 3] {
        [
            Point2::new(a.0, a.1),
            Point2::new(b.0, b.1),
            Point2::new(c.0, c.1),
        ]
    }

    /// The I3 canonical cylinder triangle: one 90° sub-rectangle, hand-derived
    /// `d(T) = √6/3`.
    fn canonical_tri() -> [Point2; 3] {
        uv((0.0, 0.0), (PI / 2.0, 0.0), (0.0, 1.0))
    }

    // ---- Test-local robust point-to-triangle distance oracle. ------------
    // Interior projection when the perpendicular foot lands inside the
    // triangle, else nearest edge/vertex (segment distances are clamped, so
    // vertices are covered). Degenerate 3D triangles fall through to the
    // edge minimum.

    fn sub(a: Point3, b: Point3) -> [f64; 3] {
        [a.x() - b.x(), a.y() - b.y(), a.z() - b.z()]
    }

    fn dot(a: [f64; 3], b: [f64; 3]) -> f64 {
        a[0] * b[0] + a[1] * b[1] + a[2] * b[2]
    }

    fn cross(a: [f64; 3], b: [f64; 3]) -> [f64; 3] {
        [
            a[1] * b[2] - a[2] * b[1],
            a[2] * b[0] - a[0] * b[2],
            a[0] * b[1] - a[1] * b[0],
        ]
    }

    fn point_segment_distance(p: Point3, a: Point3, b: Point3) -> f64 {
        let ab = sub(b, a);
        let ap = sub(p, a);
        let len2 = dot(ab, ab);
        let t = if len2 == 0.0 {
            0.0
        } else {
            (dot(ap, ab) / len2).clamp(0.0, 1.0)
        };
        let d = [ap[0] - t * ab[0], ap[1] - t * ab[1], ap[2] - t * ab[2]];
        dot(d, d).sqrt()
    }

    fn point_triangle_distance(p: Point3, a: Point3, b: Point3, c: Point3) -> f64 {
        let edge_min = point_segment_distance(p, a, b)
            .min(point_segment_distance(p, b, c))
            .min(point_segment_distance(p, c, a));
        let v0 = sub(b, a);
        let v1 = sub(c, a);
        let v2 = sub(p, a);
        let n = cross(v0, v1);
        let n2 = dot(n, n);
        if n2 > 0.0 {
            let d00 = dot(v0, v0);
            let d01 = dot(v0, v1);
            let d11 = dot(v1, v1);
            let d20 = dot(v2, v0);
            let d21 = dot(v2, v1);
            let denom = d00 * d11 - d01 * d01; // == n2 > 0
            let bv = (d11 * d20 - d01 * d21) / denom;
            let bw = (d00 * d21 - d01 * d20) / denom;
            let bu = 1.0 - bv - bw;
            if bu >= 0.0 && bv >= 0.0 && bw >= 0.0 {
                // Perpendicular foot is interior: plane distance.
                return dot(v2, n).abs() / n2.sqrt();
            }
        }
        edge_min
    }

    /// Spec I7: every Ok result is finite and `>= 0`.
    fn assert_i7(d: f64) {
        assert!(d.is_finite() && d >= 0.0, "I7 violated: d(T) = {d}");
    }

    /// Spec I1 (certification, the load-bearing invariant): dense barycentric
    /// grid (all `(i, j)` with `i + j <= 20`) over the uv triangle; every
    /// sample's distance to the 3D triangle must be `<= d_of_t(...) + 1e-12`.
    /// Returns the bound for further assertions.
    fn assert_i1_certified(surface: &Surface, tri: [Point2; 3]) -> f64 {
        const N: usize = 20;
        let d = d_of_t(surface, tri).expect("d_of_t must succeed on a legal triangle");
        assert_i7(d);
        let c: Vec<Point3> = tri
            .iter()
            .map(|&p| eval_uv(surface, p).expect("corner eval_uv must succeed"))
            .collect();
        for i in 0..=N {
            for j in 0..=(N - i) {
                let b0 = i as f64 / N as f64;
                let b1 = j as f64 / N as f64;
                let b2 = (N - i - j) as f64 / N as f64;
                let s = Point2::new(
                    b0 * tri[0].x() + b1 * tri[1].x() + b2 * tri[2].x(),
                    b0 * tri[0].y() + b1 * tri[1].y() + b2 * tri[2].y(),
                );
                let q = eval_uv(surface, s).expect("sample eval_uv must succeed");
                let dist = point_triangle_distance(q, c[0], c[1], c[2]);
                assert!(
                    dist <= d + 1e-12,
                    "I1 violated at grid ({i},{j}) uv=({}, {}): sample dist {dist} > d(T) {d}",
                    s.x(),
                    s.y()
                );
            }
        }
        d
    }

    // ---- Spec I2: any triangle on any Plane returns exactly 0.0. ---------
    #[test]
    fn i2_plane_returns_exact_zero() {
        let axis_aligned = Surface::Plane {
            normal: Vector3::new(0.0, 0.0, 1.0),
            d: -2.0,
        };
        let oblique = Surface::Plane {
            normal: Vector3::new(1.0, 2.0, 3.0),
            d: 0.7,
        };
        for s in [axis_aligned, oblique] {
            assert_eq!(d_of_t(&s, uv((0.0, 0.0), (3.0, -1.0), (0.5, 4.0))), Ok(0.0));
            assert_eq!(
                d_of_t(&s, uv((-5.0, 2.0), (7.0, 2.5), (1.0, -3.0))),
                Ok(0.0)
            );
        }
    }

    // ---- Spec I3: canonical cylinder exactness, d(T) = √6/3. -------------
    // Also pins the eval_uv parameterization + ortho_basis frame: for axis
    // (0,0,1), ortho_basis gives e1 = (1,0,0), e2 = (0,1,0), so the three
    // corners land at (1,0,0), (0,1,0), (1,0,1).
    #[test]
    fn i3_canonical_cylinder_sqrt6_over_3() {
        let s = cyl_z();
        let tri = canonical_tri();
        let expect = [
            Point3::new(1.0, 0.0, 0.0),
            Point3::new(0.0, 1.0, 0.0),
            Point3::new(1.0, 0.0, 1.0),
        ];
        for (k, e) in expect.iter().enumerate() {
            let got = eval_uv(&s, tri[k]).expect("corner eval_uv must succeed");
            assert!(
                (got.x() - e.x()).abs() <= 1e-15
                    && (got.y() - e.y()).abs() <= 1e-15
                    && (got.z() - e.z()).abs() <= 1e-15,
                "eval_uv corner {k}: got {got:?}, want {e:?} (frame/parameterization pin)"
            );
        }
        let d = d_of_t(&s, tri).expect("canonical triangle must succeed");
        assert_i7(d);
        let want = 6.0_f64.sqrt() / 3.0;
        assert!(
            (d - want).abs() <= 1e-12,
            "I3 violated: d(T) = {d}, want √6/3 = {want}"
        );
    }

    // ---- Spec I1 certification sweeps, one per curved surface type. ------
    // Each u-span exceeds π/2 to force azimuth subdivision (spec §5).
    #[test]
    fn i1_certified_cylinder_subdivided() {
        // Spec I1: u-span 2.0 > π/2.
        assert_i1_certified(&cyl_z(), uv((0.0, 0.0), (2.0, 0.0), (1.0, 1.5)));
    }

    #[test]
    fn i1_certified_cone_subdivided() {
        // Spec I1: u-span 1.8 > π/2; v > 0 throughout.
        assert_i1_certified(&cone_z(), uv((0.0, 0.5), (1.8, 0.5), (0.9, 2.0)));
    }

    #[test]
    fn i1_certified_sphere_subdivided() {
        // Spec I1: u-span 1.7 > π/2; v within [−π/2, π/2].
        assert_i1_certified(&sphere_off(), uv((0.0, -0.4), (1.7, -0.4), (0.8, 0.9)));
    }

    #[test]
    fn i1_certified_torus_subdivided() {
        // Spec I1: u-span 1.9 > π/2 AND v-span 1.8 > π/2 (both axes subdivide).
        assert_i1_certified(&torus_z(), uv((0.0, 0.0), (1.9, 0.3), (0.9, 1.8)));
    }

    // ---- Spec §5 "Sphere pole legality": corner at v = π/2 exactly. ------
    #[test]
    fn sphere_pole_corner_is_legal_and_certified() {
        // The azimuth ring degenerates at the pole; still finite + certified.
        assert_i1_certified(&sphere_off(), uv((0.2, PI / 2.0), (0.9, 0.3), (1.6, 0.5)));
    }

    // ---- Spec §5 "Cone near-apex": corner at v = 0 exactly (t >= 0). -----
    #[test]
    fn cone_apex_corner_is_legal_and_certified() {
        assert_i1_certified(&cone_z(), uv((0.0, 0.0), (1.0, 1.0), (0.3, 2.0)));
    }

    // ---- Spec I4: shrink monotonicity about the uv centroid. -------------
    #[test]
    fn i4_shrink_monotonicity() {
        let s = cyl_z();
        let tri = canonical_tri();
        let cx = (tri[0].x() + tri[1].x() + tri[2].x()) / 3.0;
        let cy = (tri[0].y() + tri[1].y() + tri[2].y()) / 3.0;
        let half = [
            Point2::new(cx + 0.5 * (tri[0].x() - cx), cy + 0.5 * (tri[0].y() - cy)),
            Point2::new(cx + 0.5 * (tri[1].x() - cx), cy + 0.5 * (tri[1].y() - cy)),
            Point2::new(cx + 0.5 * (tri[2].x() - cx), cy + 0.5 * (tri[2].y() - cy)),
        ];
        let d_full = d_of_t(&s, tri).expect("full triangle must succeed");
        let d_half = d_of_t(&s, half).expect("halved triangle must succeed");
        assert_i7(d_full);
        assert_i7(d_half);
        assert!(
            d_half < d_full,
            "I4 violated: d(half) = {d_half} not strictly < d(full) = {d_full}"
        );
    }

    // ---- Spec I5: determinism (bit-identical repeat invocation). ----------
    #[test]
    fn i5_deterministic_bitwise() {
        let s = torus_z();
        let tri = uv((0.0, 0.0), (1.9, 0.3), (0.9, 1.8));
        let a = d_of_t(&s, tri).expect("first call must succeed");
        let b = d_of_t(&s, tri).expect("second call must succeed");
        assert_eq!(
            a.to_bits(),
            b.to_bits(),
            "I5 violated: {a} vs {b} differ bitwise"
        );
    }

    // ---- Spec I6: rigid-motion (translation) sanity. ----------------------
    #[test]
    fn i6_translation_invariance() {
        // Cylinder: translate axis_point by (1, 2, 3); uv unchanged.
        let tri = canonical_tri();
        let d0 = d_of_t(&cyl_z(), tri).expect("origin cylinder must succeed");
        let moved = Surface::Cylinder {
            axis_point: Point3::new(1.0, 2.0, 3.0),
            axis_dir: Vector3::new(0.0, 0.0, 1.0),
            radius: 1.0,
        };
        let d1 = d_of_t(&moved, tri).expect("moved cylinder must succeed");
        assert!(
            (d0 - d1).abs() < 1e-9,
            "I6 violated (cylinder): {d0} vs {d1}"
        );
        // Sphere: translate center by (1, 2, 3); uv unchanged.
        let stri = uv((0.0, -0.4), (1.7, -0.4), (0.8, 0.9));
        let d2 = d_of_t(&sphere_off(), stri).expect("sphere must succeed");
        let smoved = Surface::Sphere {
            center: Point3::new(1.1, 2.2, 3.3),
            radius: 2.0,
        };
        let d3 = d_of_t(&smoved, stri).expect("moved sphere must succeed");
        assert!((d2 - d3).abs() < 1e-9, "I6 violated (sphere): {d2} vs {d3}");
    }

    // ---- Failure modes (spec §6), exact variants. --------------------------
    #[test]
    fn rejects_non_finite_input() {
        // NaN in a uv coordinate.
        assert_eq!(
            d_of_t(&cyl_z(), uv((f64::NAN, 0.0), (1.0, 0.0), (0.5, 1.0))),
            Err(DtError::NonFiniteInput)
        );
        // NaN in a surface field.
        let bad = Surface::Cylinder {
            axis_point: Point3::new(0.0, 0.0, 0.0),
            axis_dir: Vector3::new(0.0, 0.0, 1.0),
            radius: f64::NAN,
        };
        assert_eq!(
            d_of_t(&bad, uv((0.0, 0.0), (1.0, 0.0), (0.5, 1.0))),
            Err(DtError::NonFiniteInput)
        );
    }

    #[test]
    fn rejects_invalid_surface() {
        let tri = uv((0.0, 0.0), (1.0, 0.0), (0.5, 1.0));
        // Cylinder radius 0.
        let zero_r = Surface::Cylinder {
            axis_point: Point3::new(0.0, 0.0, 0.0),
            axis_dir: Vector3::new(0.0, 0.0, 1.0),
            radius: 0.0,
        };
        assert_eq!(d_of_t(&zero_r, tri), Err(DtError::InvalidSurface));
        // Cone half_angle at both ends of the open interval (0, π/2).
        let cone0 = Surface::Cone {
            apex: Point3::new(0.0, 0.0, 0.0),
            axis_dir: Vector3::new(0.0, 0.0, 1.0),
            half_angle: 0.0,
        };
        assert_eq!(d_of_t(&cone0, tri), Err(DtError::InvalidSurface));
        let cone90 = Surface::Cone {
            apex: Point3::new(0.0, 0.0, 0.0),
            axis_dir: Vector3::new(0.0, 0.0, 1.0),
            half_angle: PI / 2.0,
        };
        assert_eq!(d_of_t(&cone90, tri), Err(DtError::InvalidSurface));
        // Torus major_radius == minor_radius (horn torus).
        let horn = Surface::Torus {
            center: Point3::new(0.0, 0.0, 0.0),
            axis_dir: Vector3::new(0.0, 0.0, 1.0),
            major_radius: 1.0,
            minor_radius: 1.0,
        };
        assert_eq!(d_of_t(&horn, tri), Err(DtError::InvalidSurface));
        // Zero axis_dir.
        let zero_axis = Surface::Cylinder {
            axis_point: Point3::new(0.0, 0.0, 0.0),
            axis_dir: Vector3::new(0.0, 0.0, 0.0),
            radius: 1.0,
        };
        assert_eq!(d_of_t(&zero_axis, tri), Err(DtError::InvalidSurface));
    }

    #[test]
    fn rejects_azimuth_span_too_large() {
        // u-span 2π + 0.1 > 2π: more than one period, ambiguous covering rect.
        assert_eq!(
            d_of_t(&cyl_z(), uv((0.0, 0.0), (2.0 * PI + 0.1, 0.0), (1.0, 1.0))),
            Err(DtError::AzimuthSpanTooLarge)
        );
    }

    #[test]
    fn rejects_polar_range_out_of_bounds() {
        // Sphere corner at v = π/2 + 0.01 (beyond the pole).
        assert_eq!(
            d_of_t(
                &sphere_off(),
                uv((0.0, PI / 2.0 + 0.01), (0.5, 0.0), (1.0, 0.2))
            ),
            Err(DtError::PolarRangeOutOfBounds)
        );
    }

    #[test]
    fn rejects_negative_cone_axial_range() {
        // Cone corner at v = −0.01 (behind the apex).
        assert_eq!(
            d_of_t(&cone_z(), uv((0.0, -0.01), (0.5, 1.0), (1.0, 0.5))),
            Err(DtError::NegativeConeAxialRange)
        );
    }

    #[test]
    fn eval_uv_rejects_non_finite_point() {
        assert_eq!(
            eval_uv(&cyl_z(), Point2::new(f64::NAN, 0.0)),
            Err(DtError::NonFiniteInput)
        );
    }

    #[test]
    fn eval_uv_rejects_polar_out_of_bounds() {
        assert_eq!(
            eval_uv(&sphere_off(), Point2::new(0.0, PI / 2.0 + 0.01)),
            Err(DtError::PolarRangeOutOfBounds)
        );
    }

    // ---- Boundary-legal: u-span EXACTLY 2π is Ok (only > 2π errors). ------
    #[test]
    fn full_turn_azimuth_span_is_legal() {
        let d = d_of_t(&cyl_z(), uv((0.0, 0.0), (2.0 * PI, 0.0), (PI, 1.0)))
            .expect("span == 2π must be legal");
        assert_i7(d);
    }
}
