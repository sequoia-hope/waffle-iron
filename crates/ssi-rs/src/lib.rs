//! Analytical surface-surface intersection (SSI) solvers.
//!
//! ## Scope
//!
//! Closed-form intersection curves between pairs of analytical surfaces:
//! plane, cylinder, cone, sphere, torus. Used by `yang-rs` Stage 3
//! (refinement of mesh-approximate intersection curves to surface-exact).
//!
//! Each solver answers: given surface A and surface B, what are the
//! analytical intersection curves (lines, circles, ellipses, conics, or
//! general parameterized curves) on both surfaces?
//!
//! ## References
//!
//! - Patrikalakis & Maekawa 2002, "Shape Interrogation for Computer Aided
//!   Design and Manufacturing," Chapter 5 (SSI algorithms)
//! - Yang et al. 2025, §4.3 (SSI in the hybrid boolean pipeline)
//!
//! ## Solver matrix (target)
//!
//! Symmetric: pair (A, B) === pair (B, A).
//!
//! | A \ B    | Plane | Cylinder | Cone | Sphere | Torus |
//! |----------|-------|----------|------|--------|-------|
//! | Plane    | ✓     |          |      |        |       |
//! | Cylinder | ✓     | ✓        |      |        |       |
//! | Cone     | ✓     | ✓        | ✓    |        |       |
//! | Sphere   | ✓     | ✓        | ✓    | ✓      |       |
//! | Torus    | ✓     | ✓        | ✓    | ✓      | ✓     |
//!
//! 15 unique solvers total.

use cad_primitives::{Point3, Vector3, MIN_FEATURE_SIZE, TAU_MODEL};

// ---------------------------------------------------------------------------
// Inline f64 vector helpers. `Vector3`/`Point3` are storage-only (no dot/
// cross/norm), so all algebra is done on `[f64; 3]` here (A14.3: no ad-hoc
// epsilons — tolerances come from `cad-primitives`).
// ---------------------------------------------------------------------------

#[inline]
fn dot(a: [f64; 3], b: [f64; 3]) -> f64 {
    a[0] * b[0] + a[1] * b[1] + a[2] * b[2]
}

#[inline]
fn sub(a: [f64; 3], b: [f64; 3]) -> [f64; 3] {
    [a[0] - b[0], a[1] - b[1], a[2] - b[2]]
}

#[inline]
fn add(a: [f64; 3], b: [f64; 3]) -> [f64; 3] {
    [a[0] + b[0], a[1] + b[1], a[2] + b[2]]
}

#[inline]
fn scale(a: [f64; 3], s: f64) -> [f64; 3] {
    [a[0] * s, a[1] * s, a[2] * s]
}

#[inline]
fn cross(a: [f64; 3], b: [f64; 3]) -> [f64; 3] {
    [
        a[1] * b[2] - a[2] * b[1],
        a[2] * b[0] - a[0] * b[2],
        a[0] * b[1] - a[1] * b[0],
    ]
}

#[inline]
fn norm(a: [f64; 3]) -> f64 {
    dot(a, a).sqrt()
}

/// Defensive normalization: a zero-length or non-finite vector yields
/// `Err(DegenerateInput)` rather than a NaN/Inf-laden direction.
fn normalize(a: [f64; 3]) -> Result<[f64; 3], SsiError> {
    let len = norm(a);
    if !len.is_finite() || len < TAU_MODEL {
        return Err(SsiError::DegenerateInput);
    }
    let inv = 1.0 / len;
    let out = [a[0] * inv, a[1] * inv, a[2] * inv];
    if out.iter().all(|c| c.is_finite()) {
        Ok(out)
    } else {
        Err(SsiError::DegenerateInput)
    }
}

// ---------------------------------------------------------------------------
// Public types (see spec §Types).
// ---------------------------------------------------------------------------

/// A natural-quadric surface in implicit form. Only `Plane` and `Sphere` are
/// present in PR-SSI1; `Cylinder`/`Cone`/`Torus` arrive with their solvers.
#[derive(Debug, Clone, Copy, PartialEq)]
pub enum QuadricSurface {
    /// Plane through `point` with unit `normal`: `n·(x − point) = 0`.
    Plane {
        /// A point on the plane.
        point: Point3,
        /// Unit outward normal.
        normal: Vector3,
    },
    /// Sphere of radius `radius` centered at `center`: `|x − center| = radius`.
    Sphere {
        /// Sphere center.
        center: Point3,
        /// Sphere radius (must be positive for a valid surface).
        radius: f64,
    },
}

/// An exact analytical intersection curve (never a polyline).
#[derive(Debug, Clone, Copy, PartialEq)]
pub enum SsiCurve {
    /// Infinite line `point + t·dir`; `dir` is unit.
    Line {
        /// A point on the line.
        point: Point3,
        /// Unit direction.
        dir: Vector3,
    },
    /// Circle in the plane through `center` with unit `normal`, radius `radius`.
    Circle {
        /// Circle center.
        center: Point3,
        /// Unit normal of the circle's supporting plane.
        normal: Vector3,
        /// Circle radius.
        radius: f64,
    },
}

/// Error categories for SSI. Both variants are part of the public API.
#[derive(Debug, Clone, Copy, PartialEq)]
pub enum SsiError {
    /// The requested surface pair has no analytical solver (A15.2: no mesh
    /// or grid fallback — the caller decides). Reserved; not triggerable in
    /// PR-SSI1 because all `Plane`/`Sphere` pairs are implemented.
    AnalyticalSolutionNotAvailable,
    /// Degenerate input: coincident planes, zero/negative radius, concentric
    /// spheres, or a zero/non-finite normal.
    DegenerateInput,
}

impl SsiCurve {
    /// Evaluate the curve at parameter `t`, returning a point on it.
    ///
    /// - `Line`: `point + t·dir`.
    /// - `Circle`: `center + radius·(cos t · u + sin t · v)`, where `(u, v)`
    ///   is a deterministic orthonormal in-plane basis derived from `normal`
    ///   (see [`in_plane_basis`]). Determinism (I5) requires that the basis is
    ///   a pure function of `normal`.
    pub fn eval(&self, t: f64) -> Point3 {
        match self {
            SsiCurve::Line { point, dir } => {
                Point3::from(add(point.as_array(), scale(dir.as_array(), t)))
            }
            SsiCurve::Circle {
                center,
                normal,
                radius,
            } => {
                // `normal` was produced unit by the solvers; if a caller hands
                // in a degenerate normal, fall back to the center (no NaN).
                let (u, v) = match in_plane_basis(normal.as_array()) {
                    Ok(b) => b,
                    Err(_) => return *center,
                };
                let p = add(
                    center.as_array(),
                    add(scale(u, radius * t.cos()), scale(v, radius * t.sin())),
                );
                Point3::from(p)
            }
        }
    }
}

/// Deterministic orthonormal in-plane basis `(u, v)` for a plane with unit
/// `normal`. Construction (I5):
///
/// 1. Pick the world axis whose component of `normal` has the smallest
///    absolute value (ties broken by axis order x < y < z) — this axis is the
///    "least aligned" with the normal, guaranteeing a well-conditioned cross.
/// 2. `u = normalize(axis × normal)`.
/// 3. `v = normal × u` (already unit since `normal ⟂ u` and both are unit).
///
/// Because every choice is a deterministic function of `normal`, repeated
/// calls produce byte-identical bases.
fn in_plane_basis(normal: [f64; 3]) -> Result<([f64; 3], [f64; 3]), SsiError> {
    let ax = normal[0].abs();
    let ay = normal[1].abs();
    let az = normal[2].abs();
    // Smallest |component|; deterministic tie-break x < y < z.
    let axis = if ax <= ay && ax <= az {
        [1.0, 0.0, 0.0]
    } else if ay <= az {
        [0.0, 1.0, 0.0]
    } else {
        [0.0, 0.0, 1.0]
    };
    let u = normalize(cross(axis, normal))?;
    let v = cross(normal, u);
    Ok((u, v))
}

// ---------------------------------------------------------------------------
// Dispatcher (I4 symmetry: both argument orders handled).
// ---------------------------------------------------------------------------

/// Compute the analytical intersection curve(s) of two quadric surfaces.
///
/// Dispatches to the per-pair solver. Returns an empty `Vec` when the
/// surfaces do not intersect in a curve (parallel/disjoint/tangent), and
/// `Err(SsiError::DegenerateInput)` for degenerate configurations. The
/// `AnalyticalSolutionNotAvailable` variant is reserved for future
/// unimplemented pairs; with only `Plane`/`Sphere` it is unreachable.
pub fn intersect(a: &QuadricSurface, b: &QuadricSurface) -> Result<Vec<SsiCurve>, SsiError> {
    match (a, b) {
        (QuadricSurface::Plane { .. }, QuadricSurface::Plane { .. }) => plane_plane(a, b),
        (QuadricSurface::Plane { .. }, QuadricSurface::Sphere { .. }) => plane_sphere(a, b),
        (QuadricSurface::Sphere { .. }, QuadricSurface::Plane { .. }) => {
            // Symmetry (I4): swap so the plane is first.
            plane_sphere(b, a)
        }
        (QuadricSurface::Sphere { .. }, QuadricSurface::Sphere { .. }) => sphere_sphere(a, b),
    }
}

// ---------------------------------------------------------------------------
// Solvers.
// ---------------------------------------------------------------------------

/// Plane ∩ plane.
///
/// Patrikalakis & Maekawa, *Shape Interrogation for CAD/M*, §5.8
/// (Surface/Surface Intersections — natural quadrics). Two planes
/// `n_a·(x−p_a)=0`, `n_b·(x−p_b)=0` meet in a line with direction
/// `n_a × n_b`. A point on both planes is found by solving the 2×2 system in
/// the subspace spanned by the two normals.
///
/// - Transverse (`|n_a × n_b| > tol`): one `Line`.
/// - Parallel, distinct: `Ok([])`.
/// - Coincident (parallel and the same plane): `Err(DegenerateInput)`.
fn plane_plane(a: &QuadricSurface, b: &QuadricSurface) -> Result<Vec<SsiCurve>, SsiError> {
    let (
        QuadricSurface::Plane {
            point: pa,
            normal: na,
        },
        QuadricSurface::Plane {
            point: pb,
            normal: nb,
        },
    ) = (a, b)
    else {
        return Err(SsiError::AnalyticalSolutionNotAvailable);
    };

    let na = normalize(na.as_array())?;
    let nb = normalize(nb.as_array())?;
    let pa = pa.as_array();
    let pb = pb.as_array();

    let dir = cross(na, nb);
    if norm(dir) < TAU_MODEL {
        // Parallel planes. Distinguish coincident from distinct by the signed
        // gap of pb from plane A along the (unit) normal.
        let gap = dot(na, sub(pb, pa)).abs();
        if gap < TAU_MODEL {
            return Err(SsiError::DegenerateInput); // coincident: overlap is 2D
        }
        return Ok(Vec::new()); // parallel, distinct: no intersection
    }
    let dir = normalize(dir)?;

    // Point on both planes. With d_a = n_a·p_a, d_b = n_b·p_b, solve for the
    // point in span{n_a, n_b}:
    //   x = (c_a · n_a + c_b · n_b)
    // where the c's come from the 2×2 system; standard closed form below.
    let da = dot(na, pa);
    let db = dot(nb, pb);
    let naa = dot(na, na); // = 1, but kept general
    let nbb = dot(nb, nb);
    let nab = dot(na, nb);
    let det = naa * nbb - nab * nab;
    if det.abs() < TAU_MODEL {
        // Should not happen for non-parallel unit normals; defensive.
        return Err(SsiError::DegenerateInput);
    }
    let ca = (da * nbb - db * nab) / det;
    let cb = (db * naa - da * nab) / det;
    let point = add(scale(na, ca), scale(nb, cb));

    Ok(vec![SsiCurve::Line {
        point: Point3::from(point),
        dir: Vector3::from(dir),
    }])
}

/// Plane ∩ sphere.
///
/// Patrikalakis & Maekawa, *Shape Interrogation for CAD/M*, §5.8. With signed
/// distance `d = n·(center − planePoint)` (unit `n`), the intersection is a
/// circle of radius `√(r² − d²)` centered at the foot of the perpendicular
/// `center − d·n`, lying in a plane parallel to the input plane.
///
/// - `radius ≤ 0`: `Err(DegenerateInput)`.
/// - `|d| > radius` (disjoint) or `|d| ≈ radius` (tangent point): `Ok([])`.
/// - `|d| < radius`: one `Circle`.
fn plane_sphere(
    plane: &QuadricSurface,
    sphere: &QuadricSurface,
) -> Result<Vec<SsiCurve>, SsiError> {
    let (
        QuadricSurface::Plane {
            point: pp,
            normal: pn,
        },
        QuadricSurface::Sphere {
            center: sc,
            radius: r,
        },
    ) = (plane, sphere)
    else {
        return Err(SsiError::AnalyticalSolutionNotAvailable);
    };

    let r = *r;
    if r <= 0.0 || !r.is_finite() {
        return Err(SsiError::DegenerateInput);
    }
    let n = normalize(pn.as_array())?;
    let sc = sc.as_array();
    let pp = pp.as_array();

    let d = dot(n, sub(sc, pp)); // signed distance center → plane
    let ad = d.abs();
    // Tangent (point contact) and disjoint both yield no curve.
    if ad > r - TAU_MODEL {
        return Ok(Vec::new());
    }
    let radius = (r * r - d * d).sqrt();
    let center = sub(sc, scale(n, d)); // foot of perpendicular

    Ok(vec![SsiCurve::Circle {
        center: Point3::from(center),
        normal: Vector3::from(n),
        radius,
    }])
}

/// Sphere ∩ sphere.
///
/// Patrikalakis & Maekawa, *Shape Interrogation for CAD/M*, §5.8. With
/// `D = |c_b − c_a|`, the intersection circle lies in the plane perpendicular
/// to the center line at distance `a = (D² + r_a² − r_b²)/(2D)` from `c_a`,
/// with radius `√(r_a² − a²)` and normal `(c_b − c_a)/D`.
///
/// - Concentric (`D < TAU_MODEL`) or either `radius ≤ 0`: `Err(DegenerateInput)`.
/// - Disjoint (`D > r_a + r_b`) or contained (`D < |r_a − r_b|`): `Ok([])`.
/// - Tangent (`D ≈ r_a + r_b` or `D ≈ |r_a − r_b|`): `Ok([])` (point contact).
/// - Otherwise: one `Circle`.
fn sphere_sphere(a: &QuadricSurface, b: &QuadricSurface) -> Result<Vec<SsiCurve>, SsiError> {
    let (
        QuadricSurface::Sphere {
            center: ca,
            radius: ra,
        },
        QuadricSurface::Sphere {
            center: cb,
            radius: rb,
        },
    ) = (a, b)
    else {
        return Err(SsiError::AnalyticalSolutionNotAvailable);
    };

    let ra = *ra;
    let rb = *rb;
    if ra <= 0.0 || rb <= 0.0 || !ra.is_finite() || !rb.is_finite() {
        return Err(SsiError::DegenerateInput);
    }
    let ca = ca.as_array();
    let cb = cb.as_array();

    let axis = sub(cb, ca);
    let d = norm(axis);
    if d < TAU_MODEL {
        return Err(SsiError::DegenerateInput); // concentric
    }

    let sum = ra + rb;
    let diff = (ra - rb).abs();
    // Disjoint or contained → no intersection. Tangent (within TAU_MODEL of
    // either bound) is point contact → also no curve.
    if d > sum - TAU_MODEL || d < diff + TAU_MODEL {
        return Ok(Vec::new());
    }

    let u = normalize(axis)?;
    let dist = (d * d + ra * ra - rb * rb) / (2.0 * d);
    let radius_sq = ra * ra - dist * dist;
    if radius_sq <= 0.0 {
        // Numerically degenerate (effectively tangent); no curve.
        return Ok(Vec::new());
    }
    let radius = radius_sq.sqrt();
    let center = add(ca, scale(u, dist));

    Ok(vec![SsiCurve::Circle {
        center: Point3::from(center),
        normal: Vector3::from(u),
        radius,
    }])
}

// `MIN_FEATURE_SIZE` is part of the cad-primitives tolerance vocabulary
// (A14.3). It is not load-bearing for the three PR-SSI1 branch tables (those
// use TAU_MODEL), but is re-exported intent here for future solvers; silence
// the unused-import lint without an ad-hoc allow by referencing it.
const _: f64 = MIN_FEATURE_SIZE;
