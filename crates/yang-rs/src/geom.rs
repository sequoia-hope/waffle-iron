//! Analytic surface / curve vocabulary and exact conic evaluators
//! (extracted verbatim from lib.rs — spec `specs/yang_rs_lib_decomposition.md`,
//! increment 2).

use crate::{normalize3, Point3, Vector3, YangError};

// =========================================================================
// Surface / Curve enums
// =========================================================================

/// Analytical surface for a B-Rep face.
///
/// PR-YR2 supports `Plane` end to end. PR-YR6 adds the curved variants
/// `Sphere`, `Cylinder`, and `Cone` as TYPES so a B-Rep can carry curved
/// faces, but the pipeline does **not** yet process curved geometry: every
/// stage that consumes a `Surface` rejects the curved variants LOUDLY with
/// `YangError::CurvedSurfaceNotYetSupported` (governance A15.2, P9/P10 — never
/// a panic, silent skip, or planar approximation). Field shapes mirror
/// `ssi-rs`'s `QuadricSurface` field-for-field so a future Stage-3 yang→ssi
/// mapping is a trivial copy.
///
/// Future PRs add `Torus`, `NurbsSurface`.
///
/// **Cavity-sense (implemented PR-YR13):** the curved cavity-sense for the
/// `box − cylinder` blind pocket is now implemented via the [`BRepFace`]`.reversed`
/// flag (the curved analog of the plane's outward-normal flip at
/// reconstruction). The surface enum still carries **no** `sense` field — sense
/// lives on `BRepFace`, mirroring `ssi-rs` (which has none). PR-YR15 extends the
/// curved-cavity path to a spherical (hemispherical-dimple) cavity; PR-YR17 extends
/// it to a CONICAL POCKET (`box − cone`, apex inside / base above the top,
/// perpendicular top-plane exit → exact `Circle` rim). Still-deferred curved
/// cavities: through-cone / cone-base-subtracted, OBLIQUE cone cuts
/// (ellipse/parabola/hyperbola rims), and fully-internal cone/sphere voids
/// (multi-shell). The `Curve::Parabola`/`Hyperbola` variants are now wired
/// end-to-end (PR-YR22 parabola, PR-YR23 hyperbola).
#[derive(Copy, Clone, Debug, PartialEq)]
pub enum Surface {
    /// Plane: `n·x + d = 0`. Normal `n` points OUTWARD from the solid.
    Plane { normal: Vector3, d: f64 },
    /// Sphere `|x − center| = radius`. Outward side = radially **away from
    /// `center`** (a positive-radius solid ball). No `sense` field (mirrors
    /// `ssi-rs`).
    Sphere { center: Point3, radius: f64 },
    /// Infinite right-circular cylinder, axis through `axis_point` along
    /// `axis_dir`, of `radius`. Outward side = radially **away from the axis**
    /// (a solid cylinder). No `sense` field (mirrors `ssi-rs`).
    Cylinder {
        axis_point: Point3,
        axis_dir: Vector3,
        radius: f64,
    },
    /// Infinite right-circular cone with `apex`, axis `axis_dir`, and
    /// `half_angle`. Outward side = radially **away from the axis** (a solid
    /// cone). No `sense` field (mirrors `ssi-rs`).
    Cone {
        apex: Point3,
        axis_dir: Vector3,
        half_angle: f64,
    },
    /// Ring torus (KV6d): revolving a circle of radius `minor_radius` (the
    /// profile, the tube) about the axis through `center` along `axis_dir`, the
    /// profile center tracing a circle of radius `major_radius` (the tube
    /// center circle) in the plane through `center` ⊥ the axis. `major_radius >
    /// minor_radius`. Outward side = radially **away from the tube center
    /// circle** (a solid ring). No `sense` field (mirrors the other surfaces).
    Torus {
        center: Point3,
        axis_dir: Vector3,
        major_radius: f64,
        minor_radius: f64,
    },
}

/// Analytical curve for a B-Rep edge.
///
/// PR-YR2 supports `LineSegment` (endpoints implicit from the edge's
/// start/end vertices). PR-YR6 adds `Circle` and `Ellipse` as TYPES (field
/// shapes mirror `ssi-rs`'s `SsiCurve` field-for-field). No production code
/// consumes the curved variants yet — they exist so a future Stage-3 SSI
/// wiring can store analytical intersection curves on output edges.
///
/// `Parabola` (PR-YR22) and `Hyperbola` (PR-YR23) are now wired end-to-end for
/// the cone∩plane sections. Future PRs also add `NurbsCurve`.
#[derive(Copy, Clone, Debug, PartialEq)]
pub enum Curve {
    /// Straight segment; endpoints implicit from the edge's start/end vertices.
    LineSegment,
    /// Circle of `radius` centered at `center`, in the plane with unit
    /// `normal`.
    Circle {
        center: Point3,
        normal: Vector3,
        radius: f64,
    },
    /// Ellipse centered at `center` in the plane with unit `normal`. The
    /// semi-major axis lies along unit `major_axis` with length `major_radius`;
    /// the semi-minor axis (`normal × major_axis`) has length `minor_radius`.
    Ellipse {
        center: Point3,
        normal: Vector3,
        major_axis: Vector3,
        major_radius: f64,
        minor_radius: f64,
    },
    /// Parabola with `vertex` on the curve, in the plane with unit `normal`.
    /// The axis of symmetry lies along unit `axis_dir`; the conjugate in-plane
    /// direction is `normal × axis_dir` (unit, since both are unit and
    /// orthogonal). `focal_length` is the focal distance `f > 0`. In the
    /// in-plane frame `(x along axis_dir, y along normal × axis_dir)` the curve
    /// satisfies `y² = 4f·x`, parameterized (matching `ssi_rs::SsiCurve`) as
    /// `vertex + (t²/(4f))·axis_dir + t·(normal × axis_dir)`.
    Parabola {
        vertex: Point3,
        normal: Vector3,
        axis_dir: Vector3,
        focal_length: f64,
    },
    /// Hyperbola centered at `center` in the plane with unit `normal`. The
    /// transverse axis lies along unit `major_axis`; the conjugate in-plane
    /// direction is `normal × major_axis` (unit, since both are unit and
    /// orthogonal). `semi_transverse` is the transverse semi-axis `a > 0`;
    /// `semi_conjugate` is the conjugate semi-axis `b > 0`. In the in-plane
    /// frame `(u along major_axis, v along normal × major_axis)` the
    /// `+major_axis` branch satisfies `(u/a)² − (v/b)² = 1` with `u > 0`,
    /// parameterized (matching `ssi_rs::SsiCurve`) as
    /// `center + (a·cosh t)·major_axis + (b·sinh t)·(normal × major_axis)`.
    Hyperbola {
        center: Point3,
        normal: Vector3,
        major_axis: Vector3,
        semi_transverse: f64,
        semi_conjugate: f64,
    },
    /// Procedural surface-pair curve (M5, `specs/m5_surface_pair_curve.md`):
    /// the general-position quadric-pair intersection (first producer:
    /// unequal-radius / skew cylinder×cylinder) — a degree-4 space curve with
    /// no conic closed form. Defined IMPLICITLY and exactly by its two analytic
    /// surfaces `a`, `b` ([#24] Yang et al. 2025 §4.1.2/§4.3; Constitution P8
    /// degree-4 clarification). Concrete points are certified by Newton
    /// projection onto BOTH surfaces (`relocate_onto_implicit_pair`); the
    /// operands are carried in ssi-call argument order. There is no closed-form
    /// parameterization — endpoints come from the mesh edge, interior samples
    /// from downstream (kernel-v2) projection. As a Stage-1 INPUT edge (M5
    /// K11 re-entry, a quartic-bounded body re-entering a chained boolean)
    /// the same projection builds its shared boundary sample chain.
    SurfacePair { a: Surface, b: Surface },
}

/// Plane carrier of a planar conic: `n·x + d = 0` through the conic's
/// center / vertex with the conic's plane normal (unit as stored).
/// `SurfacePair` (non-planar) and `LineSegment` (no carrier) yield `None`.
pub(crate) fn curve_plane(c: &Curve) -> Option<([f64; 3], f64)> {
    let (p, n) = match c {
        Curve::Circle { center, normal, .. } => (center, normal),
        Curve::Ellipse { center, normal, .. } => (center, normal),
        Curve::Parabola { vertex, normal, .. } => (vertex, normal),
        Curve::Hyperbola { center, normal, .. } => (center, normal),
        Curve::LineSegment | Curve::SurfacePair { .. } => return None,
    };
    let n = n.as_array();
    let q = p.as_array();
    Some((n, -(n[0] * q[0] + n[1] * q[1] + n[2] * q[2])))
}

// =========================================================================
// PR-YR11 — ONE shared ellipse frame (analogous to `ortho_basis` for circles).
//
// The ellipse parameterization
//   point(t) = C + major_radius·cos t·major + minor_radius·sin t·minor_dir
// with  minor_dir = normalize(normal) × normalize(major_axis)
// MUST be byte-identical in all THREE consumers (spec §3): Stage-4 relocation's
// `t`, `eval_source`'s `Curve::Ellipse` arm, and `is_reversed`'s ellipse tangent.
// These three helpers are the single source of truth; matching the
// `curve_contains_point` Ellipse convention (lib.rs §PR-YR9) exactly.
// =========================================================================

/// PR-YR11 (spec §3): the ellipse's in-plane minor direction
/// `minor_dir = normalize(normal) × normalize(major_axis)`. Returned as a unit
/// `[f64; 3]`; the inputs are the stored `Curve::Ellipse` `normal` / `major_axis`.
pub(crate) fn ellipse_frame(normal: Vector3, major_axis: Vector3) -> [f64; 3] {
    let n = normalize3(normal.as_array());
    let maj = normalize3(major_axis.as_array());
    [
        n[1] * maj[2] - n[2] * maj[1],
        n[2] * maj[0] - n[0] * maj[2],
        n[0] * maj[1] - n[1] * maj[0],
    ]
}

/// PR-YR22: evaluate the exact parabola point at parameter `t`, matching the
/// `ssi_rs::SsiCurve::Parabola` convention field-for-field:
/// `vertex + (t²/(4·focal_length))·axis_dir + t·(normal × axis_dir)`. The
/// conjugate in-plane direction `normal × axis_dir` is unit when `normal` and
/// `axis_dir` are unit and orthogonal (as ssi-rs guarantees). Used by
/// `eval_source` and the relocation round-trip oracle.
pub fn parabola_point(
    vertex: Point3,
    normal: Vector3,
    axis_dir: Vector3,
    focal_length: f64,
    t: f64,
) -> Point3 {
    let ax = axis_dir.as_array();
    let conj = [
        normal.as_array()[1] * ax[2] - normal.as_array()[2] * ax[1],
        normal.as_array()[2] * ax[0] - normal.as_array()[0] * ax[2],
        normal.as_array()[0] * ax[1] - normal.as_array()[1] * ax[0],
    ];
    let v = vertex.as_array();
    Point3::new(
        v[0] + ax[0] * t * t / (4.0 * focal_length) + conj[0] * t,
        v[1] + ax[1] * t * t / (4.0 * focal_length) + conj[1] * t,
        v[2] + ax[2] * t * t / (4.0 * focal_length) + conj[2] * t,
    )
}

/// PR-YR23: evaluate the exact hyperbola point at parameter `t`, matching the
/// `ssi_rs::SsiCurve::Hyperbola` convention field-for-field:
/// `center + (a·cosh t)·major_axis + (b·sinh t)·(normal × major_axis)` with
/// `a = semi_transverse`, `b = semi_conjugate`. The conjugate in-plane direction
/// `normal × major_axis` is unit when `normal` and `major_axis` are unit and
/// orthogonal (as ssi-rs guarantees). This traces the single `+major_axis`
/// branch (`u > 0`). Used by `eval_source` and the relocation round-trip oracle.
pub fn hyperbola_point(
    center: Point3,
    normal: Vector3,
    major_axis: Vector3,
    semi_transverse: f64,
    semi_conjugate: f64,
    t: f64,
) -> Point3 {
    let maj = major_axis.as_array();
    let conj = [
        normal.as_array()[1] * maj[2] - normal.as_array()[2] * maj[1],
        normal.as_array()[2] * maj[0] - normal.as_array()[0] * maj[2],
        normal.as_array()[0] * maj[1] - normal.as_array()[1] * maj[0],
    ];
    let c = center.as_array();
    let ch = semi_transverse * t.cosh();
    let sh = semi_conjugate * t.sinh();
    Point3::new(
        c[0] + maj[0] * ch + conj[0] * sh,
        c[1] + maj[1] * ch + conj[1] * sh,
        c[2] + maj[2] * ch + conj[2] * sh,
    )
}

pub(crate) fn ellipse_point(
    center: Point3,
    normal: Vector3,
    major_axis: Vector3,
    major_radius: f64,
    minor_radius: f64,
    t: f64,
) -> Point3 {
    let c = center.as_array();
    let maj = normalize3(major_axis.as_array());
    let mindir = ellipse_frame(normal, major_axis);
    let (ct, st) = (t.cos(), t.sin());
    Point3::new(
        c[0] + major_radius * ct * maj[0] + minor_radius * st * mindir[0],
        c[1] + major_radius * ct * maj[1] + minor_radius * st * mindir[1],
        c[2] + major_radius * ct * maj[2] + minor_radius * st * mindir[2],
    )
}

/// PR-YR11 (spec §3): the ellipse parameter `t` of a point `x` (assumed on / near
/// the ellipse), in the SAME frame as [`ellipse_point`]:
/// `u = (x−C)·major`, `v = (x−C)·minor_dir`,
/// `t = atan2(v / minor_radius, u / major_radius)`.
/// KV16: parameter of an (on-branch) point of the hyperbola frame:
/// `t = asinh(v / semi_conjugate)` with `v = (x − center)·(normal ×
/// major_axis)` — mirrors [`hyperbola_point`]'s raw (unit-by-contract) frame
/// exactly, so `hyperbola_point(…, hyperbola_param(x, …))` reproduces an
/// on-branch `x`. `sinh` is injective along the branch: no quadrant or
/// branch reconciliation (unlike [`ellipse_param`]'s `atan2`).
pub(crate) fn hyperbola_param(
    x: Point3,
    center: Point3,
    normal: Vector3,
    major_axis: Vector3,
    semi_conjugate: f64,
) -> f64 {
    let maj = major_axis.as_array();
    let n = normal.as_array();
    let conj = [
        n[1] * maj[2] - n[2] * maj[1],
        n[2] * maj[0] - n[0] * maj[2],
        n[0] * maj[1] - n[1] * maj[0],
    ];
    let c = center.as_array();
    let xa = x.as_array();
    let w = [xa[0] - c[0], xa[1] - c[1], xa[2] - c[2]];
    let v = (w[0] * conj[0] + w[1] * conj[1] + w[2] * conj[2]) / semi_conjugate;
    v.asinh()
}

/// PR-YR22: parameter of a point in the parabola frame:
/// `t = (x − vertex)·(normalize(normal) × normalize(axis_dir))` — the
/// conjugate-axis coordinate, injective and strictly monotone along the
/// parabola, matching [`parabola_point`]'s frame so `parabola_point(…,
/// parabola_param(x, …))` reproduces an on-curve `x`.
pub(crate) fn parabola_param(x: Point3, vertex: Point3, normal: Vector3, axis_dir: Vector3) -> f64 {
    let n = normalize3(normal.as_array());
    let ax = normalize3(axis_dir.as_array());
    let conj = [
        n[1] * ax[2] - n[2] * ax[1],
        n[2] * ax[0] - n[0] * ax[2],
        n[0] * ax[1] - n[1] * ax[0],
    ];
    let vx = vertex.as_array();
    let xa = x.as_array();
    (xa[0] - vx[0]) * conj[0] + (xa[1] - vx[1]) * conj[1] + (xa[2] - vx[2]) * conj[2]
}

pub(crate) fn ellipse_param(
    x: Point3,
    center: Point3,
    normal: Vector3,
    major_axis: Vector3,
    major_radius: f64,
    minor_radius: f64,
) -> f64 {
    let c = center.as_array();
    let xa = x.as_array();
    let maj = normalize3(major_axis.as_array());
    let mindir = ellipse_frame(normal, major_axis);
    let w = [xa[0] - c[0], xa[1] - c[1], xa[2] - c[2]];
    let u = w[0] * maj[0] + w[1] * maj[1] + w[2] * maj[2];
    let v = w[0] * mindir[0] + w[1] * mindir[1] + w[2] * mindir[2];
    (v / minor_radius).atan2(u / major_radius)
}

/// I5-0 (§4.3.4 seam-density census; the future I5-1 insert primitive):
/// closed-form evaluation of a conic at parameter `t`, frame-consistent with
/// [`crate::stage4_correct::conic_param`] — `conic_eval(c, conic_param(c, x))`
/// reproduces an on-curve `x` to evaluation precision, and
/// `conic_param(c, conic_eval(c, t))` returns `t` (wrapped to atan2's branch).
/// Circle: the `ortho_basis(normal)` frame `project_onto_circle` uses;
/// Ellipse: the normalized-major / [`ellipse_frame`] frame [`ellipse_param`]
/// uses. `None` for non-conic payloads — the caller keeps its loud skip.
pub(crate) fn conic_eval(curve: &Curve, t: f64) -> Option<Point3> {
    match curve {
        Curve::Circle {
            center,
            normal,
            radius,
        } => {
            let (e1, e2) = crate::ortho_basis(*normal);
            let (e1a, e2a) = (e1.as_array(), e2.as_array());
            let c = center.as_array();
            let (ct, st) = (t.cos(), t.sin());
            Some(Point3::new(
                c[0] + radius * (ct * e1a[0] + st * e2a[0]),
                c[1] + radius * (ct * e1a[1] + st * e2a[1]),
                c[2] + radius * (ct * e1a[2] + st * e2a[2]),
            ))
        }
        Curve::Ellipse {
            center,
            normal,
            major_axis,
            major_radius,
            minor_radius,
        } => {
            let maj = normalize3(major_axis.as_array());
            let mindir = ellipse_frame(*normal, *major_axis);
            let c = center.as_array();
            let (ct, st) = (t.cos(), t.sin());
            Some(Point3::new(
                c[0] + major_radius * ct * maj[0] + minor_radius * st * mindir[0],
                c[1] + major_radius * ct * maj[1] + minor_radius * st * mindir[1],
                c[2] + major_radius * ct * maj[2] + minor_radius * st * mindir[2],
            ))
        }
        _ => None,
    }
}

/// PR-YR11 (spec §3): the (unnormalized) ellipse tangent at parameter `t`:
/// `−major_radius·sin t·major + minor_radius·cos t·minor_dir`. Used by
/// `is_reversed` for the exact ellipse tangent at a relocated point.
pub(crate) fn ellipse_tangent(
    normal: Vector3,
    major_axis: Vector3,
    major_radius: f64,
    minor_radius: f64,
    t: f64,
) -> [f64; 3] {
    let maj = normalize3(major_axis.as_array());
    let mindir = ellipse_frame(normal, major_axis);
    let (ct, st) = (t.cos(), t.sin());
    [
        -major_radius * st * maj[0] + minor_radius * ct * mindir[0],
        -major_radius * st * maj[1] + minor_radius * ct * mindir[1],
        -major_radius * st * maj[2] + minor_radius * ct * mindir[2],
    ]
}

/// PR-YR7: signed distance from `point` to an analytic `surface` (spec §5).
///
/// - `Plane { normal, d }` → `normal·point + d` (the stored normal, as the
///   planar fixtures use unit normals — same convention as the existing
///   `plane_dist`).
/// - `Cylinder { axis_point, axis_dir, radius }` → `dist(point, axis) − radius`.
/// - `Sphere { center, radius }` → `|point − center| − radius` (PR-YR12).
/// - `Cone { apex, axis_dir, half_angle }` → signed radial residual
///   `radial − |h_axial|·tanα` (PR-YR16, spec §5.3): positive outside the
///   lateral, negative inside, ≈ 0 on the surface. LOUD `Ok` — never a panic
///   or planar approximation.
pub fn signed_distance_to_surface(surface: Surface, point: Point3) -> Result<f64, YangError> {
    let x = point.as_array();
    match surface {
        Surface::Plane { normal, d } => {
            let n = normal.as_array();
            Ok(n[0] * x[0] + n[1] * x[1] + n[2] * x[2] + d)
        }
        Surface::Cylinder {
            axis_point,
            axis_dir,
            radius,
        } => {
            let au = normalize3(axis_dir.as_array());
            let ap = axis_point.as_array();
            let w = [x[0] - ap[0], x[1] - ap[1], x[2] - ap[2]];
            let along = w[0] * au[0] + w[1] * au[1] + w[2] * au[2];
            let radial = [
                w[0] - along * au[0],
                w[1] - along * au[1],
                w[2] - along * au[2],
            ];
            let dist =
                (radial[0] * radial[0] + radial[1] * radial[1] + radial[2] * radial[2]).sqrt();
            Ok(dist - radius)
        }
        Surface::Sphere { center, radius } => {
            let c = center.as_array();
            let w = [x[0] - c[0], x[1] - c[1], x[2] - c[2]];
            Ok((w[0] * w[0] + w[1] * w[1] + w[2] * w[2]).sqrt() - radius)
        }
        // PR-YR16 (spec §5.3): SIGNED radial residual of the cone lateral.
        // Positive outside the lateral, negative inside, ≈ 0 on the surface —
        // the honest analog of the Cylinder/Sphere signed arms. LOUD `Ok`
        // (never a panic or planar approximation).
        Surface::Cone {
            apex,
            axis_dir,
            half_angle,
        } => {
            let au = normalize3(axis_dir.as_array());
            let a = apex.as_array();
            let w = [x[0] - a[0], x[1] - a[1], x[2] - a[2]];
            let h_axial = w[0] * au[0] + w[1] * au[1] + w[2] * au[2];
            let radial_vec = [
                w[0] - h_axial * au[0],
                w[1] - h_axial * au[1],
                w[2] - h_axial * au[2],
            ];
            let radial = (radial_vec[0] * radial_vec[0]
                + radial_vec[1] * radial_vec[1]
                + radial_vec[2] * radial_vec[2])
                .sqrt();
            Ok(radial - h_axial.abs() * half_angle.tan())
        }
        // KV6d: signed distance to the torus tube surface,
        // `√((ρ − R)² + τ²) − r` (ρ = radial dist from axis, τ = axial). <0
        // inside the tube, >0 outside, ≈0 on the surface.
        Surface::Torus {
            center,
            axis_dir,
            major_radius,
            minor_radius,
        } => {
            let au = normalize3(axis_dir.as_array());
            let c = center.as_array();
            let w = [x[0] - c[0], x[1] - c[1], x[2] - c[2]];
            let tau = w[0] * au[0] + w[1] * au[1] + w[2] * au[2];
            let radial_vec = [w[0] - tau * au[0], w[1] - tau * au[1], w[2] - tau * au[2]];
            let rho = (radial_vec[0] * radial_vec[0]
                + radial_vec[1] * radial_vec[1]
                + radial_vec[2] * radial_vec[2])
                .sqrt();
            let d = rho - major_radius;
            Ok((d * d + tau * tau).sqrt() - minor_radius)
        }
    }
}

/// Unit outward normal of `surface` at (or near) `point` — the gradient of the
/// signed-distance forms in [`signed_distance_to_surface`]. Used by the §4.5.3
/// straight-run reversal test (spec `yang_453_junction_protected_collapse`
/// §3c): the exact intersection-curve tangent at p_r is `n_A × n_B` (Yang
/// Fig. 15). `None` at gradient singularities (cylinder/cone axis, sphere
/// center, torus spine, cone apex) where the direction is undefined.
pub(crate) fn surface_normal_at(surface: Surface, point: Point3) -> Option<[f64; 3]> {
    let x = point.as_array();
    let unit = |v: [f64; 3]| -> Option<[f64; 3]> {
        let n = (v[0] * v[0] + v[1] * v[1] + v[2] * v[2]).sqrt();
        if n > 0.0 {
            Some([v[0] / n, v[1] / n, v[2] / n])
        } else {
            None
        }
    };
    match surface {
        Surface::Plane { normal, .. } => unit(normal.as_array()),
        Surface::Cylinder {
            axis_point,
            axis_dir,
            ..
        } => {
            let au = normalize3(axis_dir.as_array());
            let ap = axis_point.as_array();
            let w = [x[0] - ap[0], x[1] - ap[1], x[2] - ap[2]];
            let along = w[0] * au[0] + w[1] * au[1] + w[2] * au[2];
            unit([
                w[0] - along * au[0],
                w[1] - along * au[1],
                w[2] - along * au[2],
            ])
        }
        Surface::Sphere { center, .. } => {
            let c = center.as_array();
            unit([x[0] - c[0], x[1] - c[1], x[2] - c[2]])
        }
        Surface::Cone {
            apex,
            axis_dir,
            half_angle,
        } => {
            // Gradient of `|radial| − |h|·tan(α)`: radial_unit − sign(h)·tanα·axis.
            let au = normalize3(axis_dir.as_array());
            let a = apex.as_array();
            let w = [x[0] - a[0], x[1] - a[1], x[2] - a[2]];
            let h = w[0] * au[0] + w[1] * au[1] + w[2] * au[2];
            let radial = [w[0] - h * au[0], w[1] - h * au[1], w[2] - h * au[2]];
            let ru = unit(radial)?;
            let s = h.signum() * half_angle.tan();
            unit([ru[0] - s * au[0], ru[1] - s * au[1], ru[2] - s * au[2]])
        }
        Surface::Torus {
            center,
            axis_dir,
            major_radius,
            ..
        } => {
            // Gradient of `√((ρ−R)² + τ²) − r`: ((ρ−R)·radial_unit + τ·axis).
            let au = normalize3(axis_dir.as_array());
            let c = center.as_array();
            let w = [x[0] - c[0], x[1] - c[1], x[2] - c[2]];
            let tau = w[0] * au[0] + w[1] * au[1] + w[2] * au[2];
            let radial = [w[0] - tau * au[0], w[1] - tau * au[1], w[2] - tau * au[2]];
            let rho =
                (radial[0] * radial[0] + radial[1] * radial[1] + radial[2] * radial[2]).sqrt();
            let ru = unit(radial)?;
            let d = rho - major_radius;
            unit([
                d * ru[0] + tau * au[0],
                d * ru[1] + tau * au[1],
                d * ru[2] + tau * au[2],
            ])
        }
    }
}

/// Are `a` and `b` the SAME plane — parallel normals (either orientation)
/// and one offset — within the model band? The two faces of a §4.5.5
/// coplanar pair are one plane carried twice (a union's touching caps face
/// each other: opposite normals), and a junction that lists both as
/// separate constraints is rank-deficient. `false` unless both are planes.
pub(crate) fn planes_coincide(a: Surface, b: Surface) -> bool {
    let (Surface::Plane { normal: na, d: da }, Surface::Plane { normal: nb, d: db }) = (a, b)
    else {
        return false;
    };
    let (na, nb) = (na.as_array(), nb.as_array());
    let la = (na[0] * na[0] + na[1] * na[1] + na[2] * na[2]).sqrt();
    let lb = (nb[0] * nb[0] + nb[1] * nb[1] + nb[2] * nb[2]).sqrt();
    if la == 0.0 || lb == 0.0 {
        return false;
    }
    let ua = [na[0] / la, na[1] / la, na[2] / la];
    let ub = [nb[0] / lb, nb[1] / lb, nb[2] / lb];
    let cross = [
        ua[1] * ub[2] - ua[2] * ub[1],
        ua[2] * ub[0] - ua[0] * ub[2],
        ua[0] * ub[1] - ua[1] * ub[0],
    ];
    if (cross[0] * cross[0] + cross[1] * cross[1] + cross[2] * cross[2]).sqrt() > 1e-9 {
        return false;
    }
    let dot = ua[0] * ub[0] + ua[1] * ub[1] + ua[2] * ub[2];
    // Signed offsets along ONE orientation: `n·x + d = 0` ⇒ the plane sits
    // at `−d/|n|` along `n̂`; flipping `n̂` flips the sign.
    let oa = -da / la;
    let ob = if dot > 0.0 { -db / lb } else { db / lb };
    (oa - ob).abs() <= cad_primitives::TAU_MODEL * (1.0 + oa.abs().max(ob.abs()))
}

/// Exact per-axis coordinate bounds `(lo, hi)` of a single [`Surface`] where
/// finite (compact `Sphere` / `Torus`, or `Cylinder` axes perpendicular to its
/// ruling direction). Unbounded axes carry `(-INFINITY, INFINITY)`.
fn surface_axis_bounds(surface: Surface) -> ([f64; 3], [f64; 3]) {
    let mut slo = [f64::NEG_INFINITY; 3];
    let mut shi = [f64::INFINITY; 3];
    match surface {
        Surface::Sphere { center, radius } => {
            let c = center.as_array();
            let r = radius.abs();
            for k in 0..3 {
                slo[k] = c[k] - r;
                shi[k] = c[k] + r;
            }
        }
        Surface::Torus {
            center,
            axis_dir,
            major_radius,
            minor_radius,
        } => {
            let c = center.as_array();
            let u = normalize3(axis_dir.as_array());
            let (ra, rb) = (major_radius.abs(), minor_radius.abs());
            for k in 0..3 {
                let ext = ra * (1.0 - u[k] * u[k]).max(0.0).sqrt() + rb;
                slo[k] = c[k] - ext;
                shi[k] = c[k] + ext;
            }
        }
        Surface::Cylinder {
            axis_point,
            axis_dir,
            radius,
        } => {
            let ap = axis_point.as_array();
            let u = normalize3(axis_dir.as_array());
            let r = radius.abs();
            for k in 0..3 {
                if u[k] == 0.0 {
                    slo[k] = ap[k] - r;
                    shi[k] = ap[k] + r;
                }
            }
        }
        Surface::Plane { normal, d } => {
            let n = normal.as_array();
            let len = (n[0] * n[0] + n[1] * n[1] + n[2] * n[2]).sqrt();
            if len > 0.0 && len.is_finite() {
                for k in 0..3 {
                    let other_zero = (0..3).all(|j| j == k || n[j] == 0.0);
                    if other_zero && n[k] != 0.0 {
                        let coord = -d / n[k];
                        slo[k] = coord;
                        shi[k] = coord;
                    }
                }
            }
        }
        Surface::Cone { .. } => {}
    }
    (slo, shi)
}

#[allow(clippy::too_many_arguments)]
fn grow_periodic_conic_aabb(
    c: [f64; 3],
    u: [f64; 3],
    v: [f64; 3],
    t0: f64,
    t1: f64,
    closed: bool,
    lo: &mut [f64; 3],
    hi: &mut [f64; 3],
) {
    let two_pi = std::f64::consts::TAU;
    let sweep = (t1 - t0).rem_euclid(two_pi);
    let full = closed || !(sweep > 0.0 && sweep.is_finite());
    for k in 0..3 {
        let amp = u[k].hypot(v[k]);
        if !amp.is_finite() || amp == 0.0 {
            continue;
        }
        if full {
            lo[k] = lo[k].min(c[k] - amp);
            hi[k] = hi[k].max(c[k] + amp);
        } else {
            let t_max = v[k].atan2(u[k]);
            if (t_max - t0).rem_euclid(two_pi) <= sweep {
                hi[k] = hi[k].max(c[k] + amp);
            }
            let t_min = (-v[k]).atan2(-u[k]);
            if (t_min - t0).rem_euclid(two_pi) <= sweep {
                lo[k] = lo[k].min(c[k] - amp);
            }
        }
    }
}

/// Axis-aligned bounding box `(lo, hi)` of a [`Curve`] edge between endpoints
/// `p0` and `p1` (`closed == true` when `start == end`, denoting the full `2π`
/// sweep on periodic conics).
///
/// Exhaustive over all [`Curve`] variants (no catch-all arm) so callers such
/// as Stage-0 `scan_near_coplanar` (Yang et al. 2025 §4.5.5,
/// `refs/text/yang2025_hybrid_boolean.txt:718-732`; ledger N98) bound every
/// loop edge by its swept extent without enumerating curve types.
pub(crate) fn curve_aabb(
    curve: &Curve,
    p0: Point3,
    p1: Point3,
    closed: bool,
) -> ([f64; 3], [f64; 3]) {
    let (a, b) = (p0.as_array(), p1.as_array());
    let mut lo = [a[0].min(b[0]), a[1].min(b[1]), a[2].min(b[2])];
    let mut hi = [a[0].max(b[0]), a[1].max(b[1]), a[2].max(b[2])];

    match *curve {
        Curve::LineSegment => {}
        Curve::Circle {
            center,
            normal,
            radius,
        } => {
            let c = center.as_array();
            let (e1, e2) = crate::ortho_basis(normal);
            let (e1a, e2a) = (e1.as_array(), e2.as_array());
            let r = radius.abs();
            let u = [r * e1a[0], r * e1a[1], r * e1a[2]];
            let v = [r * e2a[0], r * e2a[1], r * e2a[2]];
            let angle = |p: [f64; 3]| -> f64 {
                let w = [p[0] - c[0], p[1] - c[1], p[2] - c[2]];
                let wx = w[0] * e1a[0] + w[1] * e1a[1] + w[2] * e1a[2];
                let wy = w[0] * e2a[0] + w[1] * e2a[1] + w[2] * e2a[2];
                wy.atan2(wx)
            };
            grow_periodic_conic_aabb(c, u, v, angle(a), angle(b), closed, &mut lo, &mut hi);
        }
        Curve::Ellipse {
            center,
            normal,
            major_axis,
            major_radius,
            minor_radius,
        } => {
            let c = center.as_array();
            let maj = normalize3(major_axis.as_array());
            let mindir = ellipse_frame(normal, major_axis);
            let (ra, rb) = (major_radius.abs(), minor_radius.abs());
            let u = [ra * maj[0], ra * maj[1], ra * maj[2]];
            let v = [rb * mindir[0], rb * mindir[1], rb * mindir[2]];
            let t0 = ellipse_param(p0, center, normal, major_axis, ra, rb);
            let t1 = ellipse_param(p1, center, normal, major_axis, ra, rb);
            grow_periodic_conic_aabb(c, u, v, t0, t1, closed, &mut lo, &mut hi);
        }
        Curve::Parabola {
            vertex,
            normal,
            axis_dir,
            focal_length,
        } => {
            if !closed && focal_length.is_finite() && focal_length > 0.0 {
                let n = normalize3(normal.as_array());
                let ax = normalize3(axis_dir.as_array());
                let conj = [
                    n[1] * ax[2] - n[2] * ax[1],
                    n[2] * ax[0] - n[0] * ax[2],
                    n[0] * ax[1] - n[1] * ax[0],
                ];
                let v = vertex.as_array();
                let t0 = parabola_param(p0, vertex, normal, axis_dir);
                let t1 = parabola_param(p1, vertex, normal, axis_dir);
                let (t_lo, t_hi) = (t0.min(t1), t0.max(t1));
                for k in 0..3 {
                    if ax[k] != 0.0 {
                        let t_star = -2.0 * focal_length * conj[k] / ax[k];
                        if t_star.is_finite() && t_star > t_lo && t_star < t_hi {
                            let val = v[k]
                                + ax[k] * t_star * t_star / (4.0 * focal_length)
                                + conj[k] * t_star;
                            lo[k] = lo[k].min(val);
                            hi[k] = hi[k].max(val);
                        }
                    }
                }
            }
        }
        Curve::Hyperbola {
            center,
            normal,
            major_axis,
            semi_transverse,
            semi_conjugate,
        } => {
            if !closed
                && semi_transverse.is_finite()
                && semi_transverse > 0.0
                && semi_conjugate.is_finite()
                && semi_conjugate > 0.0
            {
                let n = normalize3(normal.as_array());
                let maj = normalize3(major_axis.as_array());
                let conj = [
                    n[1] * maj[2] - n[2] * maj[1],
                    n[2] * maj[0] - n[0] * maj[2],
                    n[0] * maj[1] - n[1] * maj[0],
                ];
                let c = center.as_array();
                let param = |p: [f64; 3]| -> f64 {
                    let w = [p[0] - c[0], p[1] - c[1], p[2] - c[2]];
                    ((w[0] * conj[0] + w[1] * conj[1] + w[2] * conj[2]) / semi_conjugate).asinh()
                };
                let (t0, t1) = (param(a), param(b));
                let (t_lo, t_hi) = (t0.min(t1), t0.max(t1));
                for k in 0..3 {
                    let ca = semi_transverse * maj[k];
                    let cb = semi_conjugate * conj[k];
                    if ca.abs() > cb.abs() {
                        let t_star = (-cb / ca).atanh();
                        if t_star.is_finite() && t_star > t_lo && t_star < t_hi {
                            let val = c[k] + ca * t_star.cosh() + cb * t_star.sinh();
                            lo[k] = lo[k].min(val);
                            hi[k] = hi[k].max(val);
                        }
                    }
                }
            }
        }
        Curve::SurfacePair { a: sa, b: sb } => {
            let (slo_a, shi_a) = surface_axis_bounds(sa);
            let (slo_b, shi_b) = surface_axis_bounds(sb);
            let mid = [
                0.5 * (a[0] + b[0]),
                0.5 * (a[1] + b[1]),
                0.5 * (a[2] + b[2]),
            ];
            let chord =
                ((b[0] - a[0]).powi(2) + (b[1] - a[1]).powi(2) + (b[2] - a[2]).powi(2)).sqrt();
            for k in 0..3 {
                let slo = slo_a[k].max(slo_b[k]);
                let shi = shi_a[k].min(shi_b[k]);
                // Whenever either defining surface bounds axis `k` (all axes on
                // compact `Sphere`/`Torus`, perpendicular axes on `Cylinder`,
                // normal axis on axis-aligned `Plane`), `[slo, shi]` is an exact
                // outer bound for any arc sweep (including major > 180° arcs).
                // On mutually unbounded axes, fall back to the Stage-1 chord-ball
                // bound `mid ± chord` when open.
                let blo = if slo.is_finite() {
                    slo
                } else if !closed {
                    mid[k] - chord
                } else {
                    a[k].min(b[k])
                };
                let bhi = if shi.is_finite() {
                    shi
                } else if !closed {
                    mid[k] + chord
                } else {
                    a[k].max(b[k])
                };
                if blo.is_finite() && bhi.is_finite() && blo <= bhi {
                    lo[k] = lo[k].min(blo);
                    hi[k] = hi[k].max(bhi);
                }
            }
        }
    }

    (lo, hi)
}

#[cfg(test)]
mod curve_aabb_tests {
    use super::*;

    #[test]
    fn curve_aabb_line_segment_matches_endpoints() {
        let p0 = Point3::new(1.5, -2.0, 4.0);
        let p1 = Point3::new(-3.0, 5.5, 0.5);
        let (lo, hi) = curve_aabb(&Curve::LineSegment, p0, p1, false);
        assert_eq!(lo, [-3.0, -2.0, 0.5]);
        assert_eq!(hi, [1.5, 5.5, 4.0]);
    }

    #[test]
    fn curve_aabb_circle_closed_and_minor_arc_bulge() {
        let circle = Curve::Circle {
            center: Point3::new(1.0, 2.0, 3.0),
            normal: Vector3::new(0.0, 0.0, 1.0),
            radius: 2.0,
        };
        let seam = Point3::new(3.0, 2.0, 3.0);
        let (lo_full, hi_full) = curve_aabb(&circle, seam, seam, true);
        for k in 0..3 {
            let want_lo = [-1.0, 0.0, 3.0][k];
            let want_hi = [3.0, 4.0, 3.0][k];
            assert!((lo_full[k] - want_lo).abs() < 1e-12);
            assert!((hi_full[k] - want_hi).abs() < 1e-12);
        }

        // CCW 90° arc from +45° to +135° around +Z bulges to y = center.y + r = 4.0
        // while its endpoints sit at y = 2 + √2 ≈ 3.4142 and x ∈ [1 - √2, 1 + √2].
        let s = 2.0_f64.sqrt();
        let p0 = Point3::new(1.0 + s, 2.0 + s, 3.0);
        let p1 = Point3::new(1.0 - s, 2.0 + s, 3.0);
        let (lo_arc, hi_arc) = curve_aabb(&circle, p0, p1, false);
        assert!((lo_arc[0] - (1.0 - s)).abs() < 1e-12);
        assert!((hi_arc[0] - (1.0 + s)).abs() < 1e-12);
        assert!((lo_arc[1] - (2.0 + s)).abs() < 1e-12);
        assert!((hi_arc[1] - 4.0).abs() < 1e-12);
    }

    #[test]
    fn curve_aabb_ellipse_closed_and_minor_arc_bulge() {
        // Oblique ellipse in plane 0.6·x + 0.8·z = 4 centered at (0, 0, 5),
        // major_axis = [0.8, 0, -0.6] (a = 2.5), minor_dir = [0, 1, 0] (b = 2.0).
        let ell = Curve::Ellipse {
            center: Point3::new(0.0, 0.0, 5.0),
            normal: Vector3::new(0.6, 0.0, 0.8),
            major_axis: Vector3::new(0.8, 0.0, -0.6),
            major_radius: 2.5,
            minor_radius: 2.0,
        };
        let seam = ellipse_point(
            Point3::new(0.0, 0.0, 5.0),
            Vector3::new(0.6, 0.0, 0.8),
            Vector3::new(0.8, 0.0, -0.6),
            2.5,
            2.0,
            0.0,
        );
        let (lo_full, hi_full) = curve_aabb(&ell, seam, seam, true);
        // x amplitude = 2.5 * 0.8 = 2.0; y amplitude = 2.0; z amplitude = 2.5 * 0.6 = 1.5.
        assert!((lo_full[0] - -2.0).abs() < 1e-12 && (hi_full[0] - 2.0).abs() < 1e-12);
        assert!((lo_full[1] - -2.0).abs() < 1e-12 && (hi_full[1] - 2.0).abs() < 1e-12);
        assert!((lo_full[2] - 3.5).abs() < 1e-12 && (hi_full[2] - 6.5).abs() < 1e-12);

        // Minor arc from t = π/4 to t = 3π/4 bulges to y = +2.0 while endpoints have y = √2.
        let p0 = ellipse_point(
            Point3::new(0.0, 0.0, 5.0),
            Vector3::new(0.6, 0.0, 0.8),
            Vector3::new(0.8, 0.0, -0.6),
            2.5,
            2.0,
            std::f64::consts::FRAC_PI_4,
        );
        let p1 = ellipse_point(
            Point3::new(0.0, 0.0, 5.0),
            Vector3::new(0.6, 0.0, 0.8),
            Vector3::new(0.8, 0.0, -0.6),
            2.5,
            2.0,
            3.0 * std::f64::consts::FRAC_PI_4,
        );
        let (lo_arc, hi_arc) = curve_aabb(&ell, p0, p1, false);
        assert!((lo_arc[1] - 2.0_f64.sqrt()).abs() < 1e-12);
        assert!((hi_arc[1] - 2.0).abs() < 1e-12);
    }

    #[test]
    fn curve_aabb_parabola_captures_interior_vertex_bulge() {
        let vertex = Point3::new(-1.0, 0.0, 2.0);
        let normal = Vector3::new(0.0, 0.0, 1.0);
        let axis_dir = Vector3::new(1.0, 0.0, 0.0);
        let f = 0.5;
        let para = Curve::Parabola {
            vertex,
            normal,
            axis_dir,
            focal_length: f,
        };
        let p0 = parabola_point(vertex, normal, axis_dir, f, -2.0); // x = -1 + 4/2 = 1.0
        let p1 = parabola_point(vertex, normal, axis_dir, f, 2.0); // x = 1.0
        let (lo, hi) = curve_aabb(&para, p0, p1, false);
        assert!(
            (lo[0] - -1.0).abs() < 1e-12,
            "parabola vertex at x = -1.0 must be included in lo[0], got {}",
            lo[0]
        );
        assert!((hi[0] - 1.0).abs() < 1e-12);
        assert!((lo[1] - -2.0).abs() < 1e-12 && (hi[1] - 2.0).abs() < 1e-12);
    }

    #[test]
    fn curve_aabb_hyperbola_captures_interior_vertex_bulge() {
        let center = Point3::new(0.0, 0.0, 1.0);
        let normal = Vector3::new(0.0, 0.0, 1.0);
        let major_axis = Vector3::new(1.0, 0.0, 0.0);
        let (a, b) = (2.0, 1.5);
        let hyp = Curve::Hyperbola {
            center,
            normal,
            major_axis,
            semi_transverse: a,
            semi_conjugate: b,
        };
        let p0 = hyperbola_point(center, normal, major_axis, a, b, -1.2);
        let p1 = hyperbola_point(center, normal, major_axis, a, b, 1.2);
        let (lo, hi) = curve_aabb(&hyp, p0, p1, false);
        // Endpoints sit at x = 2·cosh(1.2) ≈ 3.621; branch apex at t = 0 reaches x = 2.0.
        assert!(
            (lo[0] - 2.0).abs() < 1e-12,
            "hyperbola vertex at x = 2.0 must be included in lo[0], got {}",
            lo[0]
        );
        assert!((hi[0] - p1.x()).abs() < 1e-12);
    }

    #[test]
    fn curve_aabb_surface_pair_bounds_bulge_and_clamps_to_compact_operand() {
        let pair = Curve::SurfacePair {
            a: Surface::Plane {
                normal: Vector3::new(0.0, 0.0, 1.0),
                d: -0.5,
            },
            b: Surface::Torus {
                center: Point3::new(0.0, 0.0, 0.0),
                axis_dir: Vector3::new(0.0, 0.0, 1.0),
                major_radius: 3.0,
                minor_radius: 1.0,
            },
        };
        let p0 = Point3::new(2.0, 3.0, 0.5);
        let p1 = Point3::new(-2.0, 3.0, 0.5);
        let (lo, hi) = curve_aabb(&pair, p0, p1, false);
        // Chord length is 4.0 around mid (0, 3, 0.5), which would reach y = 7.0
        // unclamped, but the Torus bounds y ≤ R + r = 4.0 and |z| ≤ r = 1.0,
        // while strictly covering the arc bulge above y = 3.0.
        assert!(hi[1] > 3.5 && (hi[1] - 4.0).abs() < 1e-12);
        assert!(lo[2] >= -1.0 - 1e-12 && hi[2] <= 1.0 + 1e-12);
    }
}
