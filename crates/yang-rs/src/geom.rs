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
