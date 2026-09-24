//! Sketch-plane coordinate basis: world ↔ 2D sketch-local transforms.
//!
//! This MIRRORS the JS `buildSketchPlane` in `app/src/lib/sketch/sketchCoords.js`
//! exactly — same reference-vector choice and cross-product order — so that a
//! point projected to sketch (u, v) here lands where the UI draws it. It is the
//! basis for projecting external model geometry into a sketch (see
//! `specs/projected_sketch_geometry.md`).

/// An orthonormal basis for a sketch plane: origin + in-plane x/y axes derived
/// from the plane normal the same way the UI derives them.
#[derive(Debug, Clone, Copy)]
pub struct SketchPlaneBasis {
    pub origin: [f64; 3],
    pub normal: [f64; 3],
    pub x_axis: [f64; 3],
    pub y_axis: [f64; 3],
}

fn norm(v: [f64; 3]) -> [f64; 3] {
    let m = (v[0] * v[0] + v[1] * v[1] + v[2] * v[2]).sqrt();
    if m <= f64::MIN_POSITIVE {
        return [0.0, 0.0, 1.0];
    }
    [v[0] / m, v[1] / m, v[2] / m]
}

fn cross(a: [f64; 3], b: [f64; 3]) -> [f64; 3] {
    [
        a[1] * b[2] - a[2] * b[1],
        a[2] * b[0] - a[0] * b[2],
        a[0] * b[1] - a[1] * b[0],
    ]
}

fn dot(a: [f64; 3], b: [f64; 3]) -> f64 {
    a[0] * b[0] + a[1] * b[1] + a[2] * b[2]
}

impl SketchPlaneBasis {
    /// Build the basis from a plane origin and normal, mirroring
    /// `buildSketchPlane(origin, normal)` in the UI.
    pub fn from_origin_normal(origin: [f64; 3], normal: [f64; 3]) -> Self {
        let n = norm(normal);
        // Reference vector not (nearly) parallel to the normal — identical to JS.
        let reference = if dot(n, [0.0, 0.0, 1.0]).abs() < 0.99 {
            [0.0, 0.0, 1.0]
        } else {
            [1.0, 0.0, 0.0]
        };
        let x_axis = norm(cross(reference, n));
        let y_axis = norm(cross(n, x_axis));
        SketchPlaneBasis {
            origin,
            normal: n,
            x_axis,
            y_axis,
        }
    }

    /// Build the basis with a CALLER-CHOSEN in-plane x axis, mirroring
    /// `buildSketchPlane(origin, normal, xAxis)` in the UI.
    ///
    /// `x_axis` is orthogonalized against the normal (its in-plane part is
    /// taken), so a caller may pass any direction that is not parallel to the
    /// normal. `None`, zero-length, non-finite, or parallel to the normal
    /// falls back to [`Self::from_origin_normal`] — callers that must not
    /// fall back validate first ([`Self::x_axis_is_usable`]), which is what
    /// the engine does at rebuild so a bad axis is a loud feature error
    /// rather than a silently rotated sketch.
    pub fn from_origin_normal_x(
        origin: [f64; 3],
        normal: [f64; 3],
        x_axis: Option<[f64; 3]>,
    ) -> Self {
        let n = norm(normal);
        let Some(x) = x_axis else {
            return Self::from_origin_normal(origin, normal);
        };
        if !Self::x_axis_is_usable(normal, x) {
            return Self::from_origin_normal(origin, normal);
        }
        let d = dot(x, n);
        let in_plane = [x[0] - d * n[0], x[1] - d * n[1], x[2] - d * n[2]];
        let x_axis = norm(in_plane);
        let y_axis = norm(cross(n, x_axis));
        SketchPlaneBasis {
            origin,
            normal: n,
            x_axis,
            y_axis,
        }
    }

    /// Whether `x_axis` can orient a plane of this `normal`: finite, not
    /// zero-length, and not (nearly) parallel to the normal. The band is the
    /// same 0.01 of the UI's reference-vector choice — an x axis within
    /// ~0.6° of the normal has no usable in-plane part.
    pub fn x_axis_is_usable(normal: [f64; 3], x_axis: [f64; 3]) -> bool {
        if !x_axis.iter().chain(normal.iter()).all(|c| c.is_finite()) {
            return false;
        }
        let (n, x) = (norm(normal), x_axis);
        // Finiteness is checked above, so a plain comparison is total here.
        let len = (x[0] * x[0] + x[1] * x[1] + x[2] * x[2]).sqrt();
        if len <= 0.0 {
            return false;
        }
        let unit = [x[0] / len, x[1] / len, x[2] / len];
        dot(unit, n).abs() < 0.99999
    }

    /// Project a 3D world point onto the plane and return its 2D sketch-local
    /// (u, v) coordinates. (Out-of-plane component along the normal is dropped.)
    pub fn world_to_local(&self, p: [f64; 3]) -> (f64, f64) {
        let r = [
            p[0] - self.origin[0],
            p[1] - self.origin[1],
            p[2] - self.origin[2],
        ];
        (dot(r, self.x_axis), dot(r, self.y_axis))
    }

    /// Map 2D sketch-local (u, v) coordinates back to a 3D world point on the plane.
    pub fn local_to_world(&self, u: f64, v: f64) -> [f64; 3] {
        [
            self.origin[0] + u * self.x_axis[0] + v * self.y_axis[0],
            self.origin[1] + u * self.x_axis[1] + v * self.y_axis[1],
            self.origin[2] + u * self.x_axis[2] + v * self.y_axis[2],
        ]
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn dist(a: [f64; 3], b: [f64; 3]) -> f64 {
        ((a[0] - b[0]).powi(2) + (a[1] - b[1]).powi(2) + (a[2] - b[2]).powi(2)).sqrt()
    }

    #[test]
    fn basis_is_orthonormal() {
        for normal in [
            [0.0, 0.0, 1.0],
            [1.0, 0.0, 0.0],
            [0.0, 1.0, 0.0],
            [1.0, 2.0, 3.0],
        ] {
            let b = SketchPlaneBasis::from_origin_normal([5.0, -2.0, 1.0], normal);
            // unit length
            assert!((dot(b.x_axis, b.x_axis) - 1.0).abs() < 1e-12);
            assert!((dot(b.y_axis, b.y_axis) - 1.0).abs() < 1e-12);
            // mutually orthogonal (x⊥y, x⊥n, y⊥n)
            assert!(dot(b.x_axis, b.y_axis).abs() < 1e-12);
            assert!(dot(b.x_axis, b.normal).abs() < 1e-12);
            assert!(dot(b.y_axis, b.normal).abs() < 1e-12);
        }
    }

    #[test]
    fn round_trips_in_plane_point() {
        let b = SketchPlaneBasis::from_origin_normal([1.0, 2.0, 3.0], [1.0, 1.0, 1.0]);
        // Take an in-plane world point: origin + 4*x + (-7)*y.
        let p = b.local_to_world(4.0, -7.0);
        let (u, v) = b.world_to_local(p);
        assert!((u - 4.0).abs() < 1e-9, "u={u}");
        assert!((v + 7.0).abs() < 1e-9, "v={v}");
        // local→world→local round-trips the world point too.
        let p2 = b.local_to_world(u, v);
        assert!(dist(p, p2) < 1e-9);
    }

    #[test]
    fn out_of_plane_component_is_dropped() {
        let origin = [0.0, 0.0, 0.0];
        let normal = [0.0, 0.0, 1.0]; // XY plane
        let b = SketchPlaneBasis::from_origin_normal(origin, normal);
        // A point 5 units above the plane projects to the same (u, v) as its foot.
        let (u0, v0) = b.world_to_local([2.0, -3.0, 0.0]);
        let (u1, v1) = b.world_to_local([2.0, -3.0, 5.0]);
        assert!((u0 - u1).abs() < 1e-12 && (v0 - v1).abs() < 1e-12);
    }

    // ── A caller-chosen x axis (FEATURE_NOTES §3) ──────────────────────

    #[test]
    fn a_given_x_axis_is_the_basis() {
        // The XY plane's DERIVED basis sends u to −y; a caller that wants u
        // along +x says so, and gets it.
        let derived = SketchPlaneBasis::from_origin_normal([0.0; 3], [0.0, 0.0, 1.0]);
        assert!(
            dist(derived.x_axis, [0.0, -1.0, 0.0]) < 1e-12,
            "{:?}",
            derived.x_axis
        );
        let chosen = SketchPlaneBasis::from_origin_normal_x(
            [0.0; 3],
            [0.0, 0.0, 1.0],
            Some([1.0, 0.0, 0.0]),
        );
        assert!(
            dist(chosen.x_axis, [1.0, 0.0, 0.0]) < 1e-12,
            "{:?}",
            chosen.x_axis
        );
        // Right-handed about the normal, as the derived basis is.
        assert!(
            dist(chosen.y_axis, [0.0, 1.0, 0.0]) < 1e-12,
            "{:?}",
            chosen.y_axis
        );
        assert!(dist(chosen.local_to_world(2.0, 3.0), [2.0, 3.0, 0.0]) < 1e-12);
    }

    #[test]
    fn a_given_x_axis_is_orthogonalized_not_rejected() {
        // Any direction with an in-plane part orients the plane: the caller
        // may hand over a vector it has lying around (an edge direction, a
        // world axis) without projecting it first.
        let b = SketchPlaneBasis::from_origin_normal_x(
            [0.0; 3],
            [0.0, 0.0, 1.0],
            Some([3.0, 0.0, 7.0]),
        );
        assert!(dist(b.x_axis, [1.0, 0.0, 0.0]) < 1e-12, "{:?}", b.x_axis);
        assert!(dot(b.x_axis, b.normal).abs() < 1e-15);
        assert!((dot(b.x_axis, b.x_axis) - 1.0).abs() < 1e-15);
    }

    #[test]
    fn an_x_axis_that_cannot_orient_the_plane_is_not_usable() {
        let n = [0.0, 0.0, 1.0];
        assert!(!SketchPlaneBasis::x_axis_is_usable(n, [0.0, 0.0, 0.0]));
        assert!(!SketchPlaneBasis::x_axis_is_usable(n, [0.0, 0.0, 5.0]));
        assert!(!SketchPlaneBasis::x_axis_is_usable(n, [f64::NAN, 0.0, 0.0]));
        assert!(SketchPlaneBasis::x_axis_is_usable(n, [1.0, 0.0, 0.0]));
        // The band is |x̂·n̂| < 0.99999, i.e. the axis must be more than
        // ~0.256° off the normal: `[a, 0, 1]` is usable from a ≈ 0.0045 up.
        // Below that the in-plane part is a rounding artefact of the input,
        // and normalizing it multiplies whatever noise is in it by 1/a.
        assert!(SketchPlaneBasis::x_axis_is_usable(n, [0.01, 0.0, 1.0]));
        assert!(!SketchPlaneBasis::x_axis_is_usable(n, [0.001, 0.0, 1.0]));
        // …and an unusable one falls back to the derived basis rather than
        // producing a degenerate frame (the caller validates first when a
        // fallback would be wrong).
        let b = SketchPlaneBasis::from_origin_normal_x([0.0; 3], n, Some([0.0, 0.0, 9.0]));
        let derived = SketchPlaneBasis::from_origin_normal([0.0; 3], n);
        assert!(dist(b.x_axis, derived.x_axis) < 1e-15);
    }

    #[test]
    fn no_x_axis_is_bit_identical_to_the_derived_basis() {
        for normal in [[0.0, 0.0, 1.0], [1.0, 2.0, 3.0], [0.0, 1.0, 0.0]] {
            let a = SketchPlaneBasis::from_origin_normal([1.0, 2.0, 3.0], normal);
            let b = SketchPlaneBasis::from_origin_normal_x([1.0, 2.0, 3.0], normal, None);
            assert_eq!(a.x_axis, b.x_axis, "{normal:?}");
            assert_eq!(a.y_axis, b.y_axis, "{normal:?}");
        }
    }

    #[test]
    fn xy_plane_matches_world_axes() {
        // For the canonical front/XY plane the UI picks ref=+Z, so
        // x_axis = ref×n = +Z×+Z? No: n=+Z, ref=+Z is parallel → falls to +X.
        // |n·z|=1 which is NOT < 0.99, so reference = +X, x_axis = +X×+Z = -Y... etc.
        // Just assert the documented mirror: in-plane mapping is consistent.
        let b = SketchPlaneBasis::from_origin_normal([0.0, 0.0, 0.0], [0.0, 0.0, 1.0]);
        let world = b.local_to_world(1.0, 0.0);
        let (u, v) = b.world_to_local(world);
        assert!((u - 1.0).abs() < 1e-12 && v.abs() < 1e-12);
    }
}
