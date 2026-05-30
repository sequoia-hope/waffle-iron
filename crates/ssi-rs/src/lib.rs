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

/// A natural-quadric surface in implicit form. `Plane`/`Sphere`/`Cylinder`/
/// `Cone` are present; `Torus` arrives with its solver.
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
    /// Infinite right-circular cylinder.
    Cylinder {
        /// Any point on the axis.
        axis_point: Point3,
        /// Axis direction (normalized defensively; need not be unit on input).
        axis_dir: Vector3,
        /// Cylinder radius (must be positive for a valid surface).
        radius: f64,
    }, // implicit: dist(x, axis line) = radius
    /// Infinite right-circular DOUBLE cone (both nappes).
    Cone {
        /// Cone apex.
        apex: Point3,
        /// Axis direction (normalized defensively; need not be unit on input).
        axis_dir: Vector3,
        /// Half-angle `α ∈ (0, π/2)` between the axis and a generator.
        half_angle: f64,
    }, // implicit: radial distance from axis = |h|·tanα, h=(x−apex)·â; both nappes
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
    /// Exact ellipse centered at `center` in the plane with unit `normal`. The
    /// semi-major axis lies along unit `major_axis` with length `major_radius`;
    /// the semi-minor axis (`normal × major_axis`) has length `minor_radius`.
    Ellipse {
        /// Ellipse center (= axis ∩ plane).
        center: Point3,
        /// Unit normal of the cutting plane.
        normal: Vector3,
        /// Unit in-plane direction of the semi-major axis.
        major_axis: Vector3,
        /// Semi-major length `a` (`a ≥ b`).
        major_radius: f64,
        /// Semi-minor length `b`.
        minor_radius: f64,
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
    /// - `Ellipse`: `center + a·cos t · major_axis + b·sin t · minor_axis`,
    ///   where `minor_axis = normal × major_axis` (unit and in-plane because
    ///   `normal ⟂ major_axis` and both are unit). Self-contained — the frame
    ///   is exactly the one the solver chose (I5), so this does not call
    ///   [`in_plane_basis`].
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
            SsiCurve::Ellipse {
                center,
                normal,
                major_axis,
                major_radius,
                minor_radius,
            } => {
                let major = major_axis.as_array();
                // minor_axis = normal × major_axis (unit and in-plane).
                let minor = cross(normal.as_array(), major);
                let p = add(
                    center.as_array(),
                    add(
                        scale(major, major_radius * t.cos()),
                        scale(minor, minor_radius * t.sin()),
                    ),
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
        (QuadricSurface::Plane { .. }, QuadricSurface::Cylinder { .. }) => plane_cylinder(a, b),
        (QuadricSurface::Cylinder { .. }, QuadricSurface::Plane { .. }) => {
            // Symmetry (I4): swap so the plane is first.
            plane_cylinder(b, a)
        }
        (QuadricSurface::Plane { .. }, QuadricSurface::Cone { .. }) => plane_cone(a, b),
        (QuadricSurface::Cone { .. }, QuadricSurface::Plane { .. }) => {
            // Symmetry (I4): swap so the plane is first.
            plane_cone(b, a)
        }
        // No analytical solver yet (Degree-4 and unimplemented quadric pairs;
        // future increment). A15.2: loud `Err`, never a silent mesh/grid
        // fallback.
        (QuadricSurface::Sphere { .. }, QuadricSurface::Cylinder { .. })
        | (QuadricSurface::Cylinder { .. }, QuadricSurface::Sphere { .. })
        | (QuadricSurface::Cylinder { .. }, QuadricSurface::Cylinder { .. })
        | (QuadricSurface::Sphere { .. }, QuadricSurface::Cone { .. })
        | (QuadricSurface::Cone { .. }, QuadricSurface::Sphere { .. })
        | (QuadricSurface::Cylinder { .. }, QuadricSurface::Cone { .. })
        | (QuadricSurface::Cone { .. }, QuadricSurface::Cylinder { .. })
        | (QuadricSurface::Cone { .. }, QuadricSurface::Cone { .. }) => {
            Err(SsiError::AnalyticalSolutionNotAvailable)
        }
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

/// Plane ∩ cylinder.
///
/// Patrikalakis & Maekawa, *Shape Interrogation for CAD/M*, §5.8
/// (Surface/Surface Intersections — natural quadrics). A plane section of a
/// (right-circular) cylinder is a conic section: a circle when the plane is
/// perpendicular to the axis, an ellipse when oblique, and one or two lines
/// (or nothing) when the plane is parallel to the axis.
///
/// With unit plane normal `n̂`, plane point `p`, axis point `q`, unit axis
/// `â`, radius `r`, and `c = n̂·â` (the cosine of the angle between normal and
/// axis; `|c|=1` ⇒ plane ⟂ axis, `|c|=0` ⇒ plane ∥ axis):
///
/// - **C1** (`|c| > 1 − TAU_MODEL`, perpendicular): one `Circle` of radius `r`
///   centered at the axis∩plane point, normal `â`.
/// - **C2** (`TAU_MODEL ≤ |c| ≤ 1 − TAU_MODEL`, oblique): one `Ellipse` with
///   `minor_radius = r`, `major_radius = r/|c|`, centered at axis∩plane, with
///   `major_axis = normalize(â − c·n̂)` and `normal = n̂`.
/// - **C3a** (`|c| < TAU_MODEL` and `d < r − TAU_MODEL`, parallel secant): two
///   `Line`s parallel to `â`, at `c0 ± off·ŵ` where `off = √(r²−d²)`.
/// - **C3b** (`|c| < TAU_MODEL` and `|d − r| ≤ TAU_MODEL`, parallel tangent):
///   one `Line` at the foot `c0`.
/// - **C3c** (`|c| < TAU_MODEL` and `d > r + TAU_MODEL`, parallel disjoint):
///   `Ok([])`.
/// - `r ≤ 0` / non-finite, or zero/non-finite `axis_dir` or plane `normal`:
///   `Err(DegenerateInput)`.
fn plane_cylinder(
    plane: &QuadricSurface,
    cylinder: &QuadricSurface,
) -> Result<Vec<SsiCurve>, SsiError> {
    let (
        QuadricSurface::Plane {
            point: pp,
            normal: pn,
        },
        QuadricSurface::Cylinder {
            axis_point: ap,
            axis_dir: ad,
            radius: r,
        },
    ) = (plane, cylinder)
    else {
        return Err(SsiError::AnalyticalSolutionNotAvailable);
    };

    let r = *r;
    if r <= 0.0 || !r.is_finite() {
        return Err(SsiError::DegenerateInput);
    }
    // `normalize` rejects zero / non-finite vectors (E1: zero axis or normal).
    let nhat = normalize(pn.as_array())?;
    let ahat = normalize(ad.as_array())?;
    let p = pp.as_array();
    let q = ap.as_array();

    let c = dot(nhat, ahat);
    let abs_c = c.abs();

    // In-plane projection of the axis: proj = â − c·n̂, with
    // |proj| = √(1 − c²) = sin θ (θ = tilt of the plane normal from the axis).
    // The C1/C2 split is gated on |proj| (the sine), NOT on 1 − |c|: the C1
    // circle snaps the supporting plane to ⟂ â, so its points sit off the
    // tilted cutting plane by ≤ r·sin θ = r·√(1−c²). Bounding the sine by
    // TAU_MODEL bounds that off-plane error by r·TAU_MODEL, and the gate then
    // exactly coincides with C2's normalize(proj) guard.
    let proj = sub(ahat, scale(nhat, c));
    let proj_norm = norm(proj);

    if proj_norm < TAU_MODEL {
        // C1 — perpendicular: a circle of radius r in the cutting plane.
        // axis ∩ plane: center = q + s·â, s = (n̂·(p − q)) / c.
        let s = dot(nhat, sub(p, q)) / c;
        let center = add(q, scale(ahat, s));
        return Ok(vec![SsiCurve::Circle {
            center: Point3::from(center),
            normal: Vector3::from(ahat),
            radius: r,
        }]);
    }

    if abs_c >= TAU_MODEL {
        // C2 — oblique: an ellipse.
        // axis ∩ plane: center = q + s·â, s = (n̂·(p − q)) / c.
        let s = dot(nhat, sub(p, q)) / c;
        let center = add(q, scale(ahat, s));
        // major_axis = projection of the axis onto the plane (in-plane);
        // normalize succeeds because the C1 gate guarantees |proj| ≥ TAU_MODEL.
        let major_axis = normalize(proj)?;
        return Ok(vec![SsiCurve::Ellipse {
            center: Point3::from(center),
            normal: Vector3::from(nhat),
            major_axis: Vector3::from(major_axis),
            major_radius: r / abs_c,
            minor_radius: r,
        }]);
    }

    // C3 — plane parallel to the axis. The whole axis is at constant signed
    // distance from the plane.
    let d_signed = dot(nhat, sub(q, p));
    let d = d_signed.abs();
    // Foot of the axis on the plane (signed distance used here, not |d|).
    let c0 = sub(q, scale(nhat, d_signed));
    let what = normalize(cross(nhat, ahat))?; // in-plane, ⟂ the axis

    if d < r - TAU_MODEL {
        // C3a — secant: two lines, +ŵ first (deterministic order, I5).
        let off = (r * r - d * d).sqrt();
        let p_plus = add(c0, scale(what, off));
        let p_minus = sub(c0, scale(what, off));
        return Ok(vec![
            SsiCurve::Line {
                point: Point3::from(p_plus),
                dir: Vector3::from(ahat),
            },
            SsiCurve::Line {
                point: Point3::from(p_minus),
                dir: Vector3::from(ahat),
            },
        ]);
    }

    if (d - r).abs() <= TAU_MODEL {
        // C3b — tangent: a single line at the foot.
        return Ok(vec![SsiCurve::Line {
            point: Point3::from(c0),
            dir: Vector3::from(ahat),
        }]);
    }

    // C3c — disjoint: no intersection.
    Ok(Vec::new())
}

/// Plane ∩ cone (bounded sections only: circle + ellipse).
///
/// Patrikalakis & Maekawa, *Shape Interrogation for CAD/M*, §5.8
/// (Surface/Surface Intersections — natural quadrics; elliptic-cone implicit
/// form). A plane section of a (right-circular, double) cone is a classical
/// conic section, classified by the two symmetry-plane generators
/// `g_± = cosα·â ± sinα·û` (the cone generators lying in `span{â, n̂}`):
/// a circle when the plane is ⟂ the axis, a closed ellipse when both
/// generators pierce the same nappe, and a parabola/hyperbola otherwise.
///
/// With unit plane normal `n̂`, plane point `p`, unit axis `â`, apex, half-angle
/// `α`, and `k = n̂·â`, the in-plane projection `proj = n̂ − k·â`
/// (`proj_norm = |proj| = √(1 − k²) = sin θ`):
///
/// - **E1** — `α` non-finite, `α ≤ TAU_MODEL`, or `α ≥ π/2 − TAU_MODEL`, or a
///   zero/non-finite axis or normal: `Err(DegenerateInput)`.
/// - **AP** (through-apex, `|n̂·(apex − p)| < TAU_MODEL`): the section is a
///   degenerate conic (point/line/two-lines): `Err(DegenerateInput)`.
/// - **C1** (`proj_norm < TAU_MODEL`, plane ⟂ axis): one `Circle` of radius
///   `|h|·tanα` centered at `apex + h·â`, normal `â`, `h = n̂·(p − apex)/k`.
/// - **C2** (both `gd_±` same sign and non-negligible): one `Ellipse`
///   (vertex construction below).
/// - **PH** (one `|gd_±| ≤ TAU_MODEL` ⇒ parabola, or opposite signs ⇒
///   hyperbola): `Err(AnalyticalSolutionNotAvailable)`. **This is a deliberate
///   staged limitation** — the unbounded conics are implemented in PR-SSI4, not
///   a "no solver" verdict, and never a mesh/grid fallback (A15.2); the caller
///   decides.
///
/// **C2 ellipse construction (vertex method).** The two symmetry-plane
/// generators each pierce the cutting plane at `V_± = apex + s_±·g_±`,
/// `s_± = n̂·(p − apex)/gd_±`, `gd_± = n̂·g_±`. Then `center = (V₊ + V₋)/2`,
/// `major_radius a = |V₊ − V₋|/2`, `major_axis = normalize(V₊ − V₋)`, and the
/// semi-minor length `b = √((d·â)²/cos²α − |d|²)` with `d = center − apex`
/// (the cone equation collapsed along the minor direction `ŵ = n̂ × â`).
fn plane_cone(plane: &QuadricSurface, cone: &QuadricSurface) -> Result<Vec<SsiCurve>, SsiError> {
    let (
        QuadricSurface::Plane {
            point: pp,
            normal: pn,
        },
        QuadricSurface::Cone {
            apex,
            axis_dir: ad,
            half_angle: alpha,
        },
    ) = (plane, cone)
    else {
        return Err(SsiError::AnalyticalSolutionNotAvailable);
    };

    let alpha = *alpha;
    // E1: invalid cone half-angle (a line at α→0, a plane at α→π/2).
    if !alpha.is_finite() || alpha <= TAU_MODEL || alpha >= std::f64::consts::FRAC_PI_2 - TAU_MODEL
    {
        return Err(SsiError::DegenerateInput);
    }
    // `normalize` rejects zero / non-finite vectors (E1: zero axis or normal).
    let nhat = normalize(pn.as_array())?;
    let ahat = normalize(ad.as_array())?;
    let p = pp.as_array();
    let apex = apex.as_array();

    let cosa = alpha.cos();
    let sina = alpha.sin();
    let tana = alpha.tan();
    let k = dot(nhat, ahat);

    // In-plane projection of the plane normal: proj = n̂ − k·â, with
    // |proj| = √(1 − k²) = sin θ (θ = tilt of the plane normal from the axis).
    // The C1/C2 split is gated on |proj| (the sine), NOT on √(1 − k²): the C1
    // circle snaps the supporting plane to ⟂ â, so its points sit off the
    // tilted cutting plane by an error scaling with sin θ = |proj|. The
    // vector-norm form has no cancellation near k→1 (perpendicular plane),
    // bounding the off-plane error by R·TAU_MODEL, and reuses proj for û below
    // (matching plane_cylinder).
    let proj = sub(nhat, scale(ahat, k));
    let proj_norm = norm(proj);

    // AP — apex lies on the cutting plane ⇒ degenerate conic.
    if dot(nhat, sub(apex, p)).abs() < TAU_MODEL {
        return Err(SsiError::DegenerateInput);
    }

    // C1 — plane ⟂ axis ⇒ circle. (proj_norm → 0 ⇒ û below is undefined, so
    // this branch must precede the û computation.)
    if proj_norm < TAU_MODEL {
        let h = dot(nhat, sub(p, apex)) / k;
        let center = add(apex, scale(ahat, h));
        return Ok(vec![SsiCurve::Circle {
            center: Point3::from(center),
            normal: Vector3::from(ahat),
            radius: h.abs() * tana,
        }]);
    }

    // Symmetry-plane generators g_± = cosα·â ± sinα·û, where û is the unit
    // component of n̂ ⟂ â (well-defined since C1 consumed proj_norm < TAU_MODEL).
    let uhat = normalize(proj)?;
    let g_plus = add(scale(ahat, cosa), scale(uhat, sina));
    let g_minus = sub(scale(ahat, cosa), scale(uhat, sina));
    let gd_plus = dot(nhat, g_plus);
    let gd_minus = dot(nhat, g_minus);

    // PH — parabola (a generator ∥ the plane) takes precedence; only then test
    // the hyperbola sign condition (both gd_± verified non-negligible here).
    if gd_plus.abs() <= TAU_MODEL || gd_minus.abs() <= TAU_MODEL {
        return Err(SsiError::AnalyticalSolutionNotAvailable);
    }
    if gd_plus.signum() != gd_minus.signum() {
        return Err(SsiError::AnalyticalSolutionNotAvailable);
    }

    // C2 — closed ellipse via the vertex method.
    let rhs = dot(nhat, sub(p, apex));
    let s_plus = rhs / gd_plus;
    let s_minus = rhs / gd_minus;
    let v_plus = add(apex, scale(g_plus, s_plus));
    let v_minus = add(apex, scale(g_minus, s_minus));

    let center = scale(add(v_plus, v_minus), 0.5);
    let span = sub(v_plus, v_minus);
    let a = norm(span) * 0.5;
    let major_axis = normalize(span)?;

    let d = sub(center, apex);
    let da = dot(d, ahat);
    let b = (da * da / (cosa * cosa) - dot(d, d)).sqrt();

    Ok(vec![SsiCurve::Ellipse {
        center: Point3::from(center),
        normal: Vector3::from(nhat),
        major_axis: Vector3::from(major_axis),
        major_radius: a,
        minor_radius: b,
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
