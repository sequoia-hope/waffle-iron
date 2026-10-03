//! Silhouettes of curved faces — **D1b** of `specs/drawings_and_mbd.md`
//! (§5.2 increment 2).
//!
//! A drawing of a curved solid is mostly NOT its edges. A revolved sphere has
//! one seam meridian and two poles; a cylinder has two rims. What a draughtsman
//! draws is the **silhouette**: the locus on each curved face where the surface
//! normal turns away from the viewer, i.e. where `n(p) · w = 0` for the line of
//! sight `w`. D1a projected the edges; this module adds that locus, clipped to
//! the face it belongs to.
//!
//! ## The locus, per surface
//!
//! With `w` the unit line of sight, `a` a surface's unit axis,
//! `w∥ = w·a`, `w⊥ = w − w∥·a`, `m = |w⊥|`, `u₁ = w⊥/m` and `u₂ = a × u₁`:
//!
//! | surface | normal | silhouette |
//! |---|---|---|
//! | cylinder, radius `R` | radial `r̂` | `r̂ · w = 0` ⟹ `r̂ = ±u₂`: **two rulings** parallel to the axis, at `axis ± R·u₂`. `m = 0` (seen along the axis) ⟹ NONE — the rims are the outline. |
//! | cone, half-angle `α` | `cos α·r̂ − sin α·â` | `r̂ · w = tan α·w∥`, i.e. `cos(θ−θ₀) = tan α·w∥/m`: **two rulings through the apex** where that is `≤ 1` in modulus, one where it is `1`, NONE above it (the viewer is inside the cone's own shadow) and NONE when `m = 0`. |
//! | sphere, radius `R` | radial | the **great circle** in the plane through the centre ⊥ `w` — which IS the view plane, so it projects to an exact circle of radius `R`. |
//! | torus, `R > r` | `cos φ·r̂ + sin φ·â` | `m·cos θ·cos φ + w∥·sin φ = 0`. Generic `w∥`: **two closed branches** `φ(θ) = atan2(−m·cos θ, w∥) (+π)`, exact on the `(θ, φ)` chart and sampled at the chord tolerance. `m = 0` (seen along the axis): the two **equator circles** `ρ = R ± r`, exact. `w∥ = 0` (seen edge-on): `{φ = ±π/2} ∪ {θ = ±π/2}` — the two latitude circles of radius `R` at `τ = ±r` AND the two profile circles of radius `r`, all four exact. |
//!
//! The `reversed` (cavity-sense) flag does NOT enter the locus: `n·w = 0` is
//! insensitive to the normal's sign. It enters the CLIP, through the outward
//! normal that decides which side of a boundary edge the face lies on.
//!
//! ## Clipping to the trimming loops
//!
//! A silhouette is only drawn where it is actually ON the face, so each path is
//! clipped against the face's loops. The clip is **local and exact**, with no
//! parity anchor and no sampled chart polygon:
//!
//! 1. Each path carries a scalar **functional** whose zero set contains it: a
//!    PLANE distance for every path but a torus branch (the cylinder's and
//!    cone's rulings lie in a plane through the axis; the sphere's great circle
//!    and the torus's coordinate circles are plane sections), and `n·w` itself
//!    for a torus branch.
//! 2. Every boundary half-edge is intersected with that functional. For the
//!    plane functional against the analytic curve vocabulary — line, circle,
//!    arc, ellipse arc, hyperbola arc — the roots are **closed form**
//!    (`C + A·cos t + B·sin t = 0`, or a quadratic in `eˆt` for the
//!    hyperbola). The remaining pairs (a surface-pair curve, or any edge
//!    against the torus functional) are bracketed on a dense parameter sample
//!    and bisected to float precision.
//! 3. A root that is not ON this path — the other ruling of the same axial
//!    plane, the other profile circle of the same meridian plane — is dropped
//!    by a distance test. This is what makes "a silhouette line on a partial
//!    cylinder may be absent or a sub-segment" come out right.
//! 4. Each surviving crossing is classified ENTER or EXIT from the sign of
//!    `S · (N × T)`, where `S` is the path's tangent, `T` the half-edge's
//!    tangent in the face's own traversal direction and `N` the face's outward
//!    normal: the loop walk puts the face's material to its left, so `N × T`
//!    points into the face. A crossing whose sign vanishes is a TANGENCY and
//!    does not toggle.
//! 5. The crossings are sorted along the path and paired. An open path (a
//!    ruling) that starts or ends inside the face is clamped to its boundary's
//!    own parameter extent — which is how a cone face containing the apex
//!    keeps the segment from the apex to its single rim crossing.
//!
//! A **seam** — a half-edge whose twin belongs to the SAME face — is not a
//! boundary at all: the face lies on both sides of it (the closed sphere's
//! meridian, the closed torus's profile circle). Seams are dropped before the
//! clip, which is why a closed surface's silhouette comes out whole.
//!
//! ## Known boundaries, loud rather than guessed
//!
//! - A **closed path with no crossings** on a face that DOES have a boundary
//!   is either wholly inside or wholly outside, and nothing local decides
//!   which. It is declined (no curve emitted) and censused under
//!   `KV2_SILHOUETTE_CENSUS`. A face with no boundary but its seams is the
//!   whole closed surface, so the path is wholly inside and is emitted.
//! - A crossing sequence that does not ALTERNATE enter/exit along a closed
//!   path is a degenerate clip (a tangency the sign test did not catch, or a
//!   boundary running along the silhouette). Declined and censused rather than
//!   paired arbitrarily.
//! - A surface-pair boundary edge is crossed on its render polyline, so the
//!   crossing carries that polyline's chord error. Documented, not hidden: the
//!   same chord band every other kernel-v2 consumer of that curve carries.
//!
//! Silhouette curves are tagged [`Visibility::Visible`] at this increment;
//! hidden-line classification is D1c.

use std::f64::consts::{PI, TAU};

use cad_primitives::{Point2, Point3};
use waffle_types::kernel::projection::{Curve2, CurveKind, ProjectedCurve, ViewBasis, Visibility};
use waffle_types::kernel::units::{TAU_MODEL, TAU_NORMALIZE};

use crate::arena::{BrepArena, Curve, FaceId, HalfEdgeId, SolidId, Surface, UnitVector3};
use crate::error::KernelV2Error;

/// Below this, `|w·a|` counts as "the line of sight is perpendicular to the
/// axis" and a torus's silhouette is its four coordinate circles rather than
/// two branches. At `1e-12` the two descriptions differ by `~1e-12·r`, far
/// under every tolerance downstream, and the branch form stays usable
/// everywhere above it (the adaptive chord refinement resolves the steep
/// stretch; the branch's total length is bounded by `2π(R + 2r)` for EVERY
/// `w·a`, so no sampling blow-up is possible).
const TORUS_AXIAL_DEGENERACY: f64 = 1e-12;

/// Relative slack at which a crossing's enter/exit sign counts as a TANGENCY
/// and stops toggling. `S·(N×T)/(|S||T|)` is the sine of the angle between the
/// path and the boundary, so this is a 1e-9-radian grazing band.
const TANGENCY_REL: f64 = 1e-9;

/// Every curved face's silhouette in `solid`, projected into `basis`.
///
/// Faces in shell walk order, the same order [`crate::tessellate::tessellate`]
/// uses; within a face, paths in the order [`silhouette_paths`] builds them.
/// Planar faces contribute nothing.
pub(crate) fn solid_silhouettes(
    arena: &BrepArena,
    solid: SolidId,
    basis: &ViewBasis,
    n_seg: u32,
) -> Result<Vec<ProjectedCurve>, KernelV2Error> {
    let mut out = Vec::new();
    for &sh in &arena.solid(solid)?.shells {
        for &fid in &arena.shell(sh)?.faces {
            for geometry in face_silhouettes(arena, fid, basis, n_seg)? {
                out.push(ProjectedCurve {
                    geometry,
                    visibility: Visibility::Visible,
                    kind: CurveKind::Silhouette,
                    source: Some(crate::adapter::encode_face(fid)),
                });
            }
        }
    }
    Ok(out)
}

/// One face's silhouette curves, in the view plane. Empty for a planar face,
/// for a view along a cylinder's or cone's axis, and wherever the clip
/// declines (see the module docs).
pub(crate) fn face_silhouettes(
    arena: &BrepArena,
    fid: FaceId,
    basis: &ViewBasis,
    n_seg: u32,
) -> Result<Vec<Curve2>, KernelV2Error> {
    let mut out = Vec::new();
    for (path, intervals) in clipped_paths(arena, fid, basis.w, n_seg)? {
        for (s0, s1) in intervals {
            if let Some(curve) = project_interval(basis, &path, s0, s1, n_seg) {
                out.push(curve);
            }
        }
    }
    Ok(out)
}

/// A silhouette path and the parameter intervals of it that lie on the face.
type ClippedPath = (Path, Vec<(f64, f64)>);

/// One face's silhouette paths and the parameter intervals of each that lie on
/// the face — the projection-free half of [`face_silhouettes`], which the
/// oracles check against a brute-force sign sweep of the surface's normals.
fn clipped_paths(
    arena: &BrepArena,
    fid: FaceId,
    w: [f64; 3],
    n_seg: u32,
) -> Result<Vec<ClippedPath>, KernelV2Error> {
    let face = arena.face(fid)?;
    let Some(surface) = face.surface else {
        return Ok(Vec::new());
    };
    if matches!(surface, Surface::Plane(_)) {
        return Ok(Vec::new());
    }
    let paths = silhouette_paths(&surface, w);
    if paths.is_empty() {
        return Ok(Vec::new());
    }

    // The face's real boundary: every loop's half-edges MINUS the seams, whose
    // twin lies in this same face and across which the face continues.
    let mut loops = vec![face.outer_loop];
    loops.extend(face.inner_loops.iter().copied());
    let mut hes: Vec<HalfEdgeId> = Vec::new();
    for lid in loops {
        for h in arena.loop_half_edges(lid)? {
            let twin = arena.half_edge(h)?.twin;
            let twin_face = arena.loop_(arena.half_edge(twin)?.loop_id)?.face;
            if twin_face == fid {
                continue;
            }
            hes.push(h);
        }
    }

    let scale = face_scale(arena, fid, &surface)?;
    let mut out = Vec::new();
    for (path, functional) in paths {
        let intervals = clip_path(arena, fid, &surface, &path, &functional, &hes, scale, n_seg)?;
        if !intervals.is_empty() {
            out.push((path, intervals));
        }
    }
    Ok(out)
}

// ---------------------------------------------------------------------------
// the loci
// ---------------------------------------------------------------------------

/// A parameterized silhouette path on a surface.
#[derive(Debug, Clone)]
enum Path {
    /// A straight ruling: `p(s) = base + s·dir`, `s` an arc length.
    Ruling { base: [f64; 3], dir: [f64; 3] },
    /// A circle on the surface: `p(ψ) = center + R(cos ψ·e₁ + sin ψ·e₂)`,
    /// closed, traversed counter-clockwise about `e₁ × e₂`.
    Circle {
        center: [f64; 3],
        e1: [f64; 3],
        e2: [f64; 3],
        radius: f64,
    },
    /// A torus silhouette branch `φ(θ)`, closed in `θ`.
    Branch {
        center: [f64; 3],
        a: [f64; 3],
        u1: [f64; 3],
        u2: [f64; 3],
        major: f64,
        minor: f64,
        m: f64,
        w_axial: f64,
        flip: bool,
    },
}

/// The scalar whose zero set, restricted to the surface, carries a path.
#[derive(Debug, Clone)]
enum Functional {
    /// Signed distance to a plane — closed-form roots on every analytic curve.
    Plane { o: [f64; 3], n: [f64; 3] },
    /// `r·(n·w)` on a torus: the silhouette condition itself.
    TorusDot {
        center: [f64; 3],
        a: [f64; 3],
        major: f64,
        w: [f64; 3],
    },
}

impl Functional {
    fn at(&self, p: Point3) -> f64 {
        match self {
            Functional::Plane { o, n } => dot(sub(p.as_array(), *o), *n),
            Functional::TorusDot {
                center,
                a,
                major,
                w,
            } => {
                let v = sub(p.as_array(), *center);
                let tau = dot(v, *a);
                let rv = [v[0] - tau * a[0], v[1] - tau * a[1], v[2] - tau * a[2]];
                let rho = norm(rv);
                if rho.is_nan() || rho <= TAU_NORMALIZE {
                    return f64::NAN;
                }
                (rho - major) * dot(rv, *w) / rho + tau * dot(*a, *w)
            }
        }
    }
}

/// The silhouette paths of `surface` for the line of sight `w`, each with the
/// functional that locates it, in a deterministic order.
fn silhouette_paths(surface: &Surface, w: [f64; 3]) -> Vec<(Path, Functional)> {
    match *surface {
        Surface::Plane(_) => Vec::new(),
        Surface::Cylinder {
            axis_point,
            axis_dir,
            radius,
            ..
        } => {
            let a = unit_of(axis_dir);
            let Some((.., u2)) = axial_frame(a, w) else {
                // Seen along the axis: every normal is already perpendicular
                // to the line of sight, so there is no CURVE of them — the
                // rims are the outline (spec §5.2).
                return Vec::new();
            };
            let q = axis_point.as_array();
            let mut out = Vec::new();
            for sgn in [1.0, -1.0] {
                let rad = scaled(u2, sgn);
                let Some(n_plane) = unit(cross(a, rad)) else {
                    continue;
                };
                out.push((
                    Path::Ruling {
                        base: add(q, scaled(rad, radius)),
                        dir: a,
                    },
                    Functional::Plane { o: q, n: n_plane },
                ));
            }
            out
        }
        Surface::Cone {
            apex,
            axis_dir,
            half_angle,
            ..
        } => {
            let a = unit_of(axis_dir);
            let Some((w_axial, m, u1, u2)) = axial_frame(a, w) else {
                return Vec::new();
            };
            let tan = half_angle.tan();
            let c = tan * w_axial / m;
            if !c.is_finite() || c.abs() > 1.0 {
                // The whole nappe faces one way: the viewer is inside the
                // cone's own shadow, and there is no silhouette on it.
                return Vec::new();
            }
            let sq = (1.0 - c * c).max(0.0).sqrt();
            let p0 = apex.as_array();
            let mut out = Vec::new();
            for sgn in [1.0, -1.0] {
                let rad = [
                    c * u1[0] + sgn * sq * u2[0],
                    c * u1[1] + sgn * sq * u2[1],
                    c * u1[2] + sgn * sq * u2[2],
                ];
                let (Some(dir), Some(n_plane)) =
                    (unit(add(a, scaled(rad, tan))), unit(cross(a, rad)))
                else {
                    continue;
                };
                out.push((
                    Path::Ruling { base: p0, dir },
                    Functional::Plane { o: p0, n: n_plane },
                ));
                if sq <= TAU_NORMALIZE {
                    // Grazing: the two rulings have merged into one.
                    break;
                }
            }
            out
        }
        Surface::Sphere { center, radius, .. } => {
            let Some(e1) = unit(any_perpendicular(w)) else {
                return Vec::new();
            };
            let e2 = cross(w, e1);
            vec![(
                Path::Circle {
                    center: center.as_array(),
                    e1,
                    e2,
                    radius,
                },
                Functional::Plane {
                    o: center.as_array(),
                    n: w,
                },
            )]
        }
        Surface::Torus {
            center,
            axis_dir,
            major_radius,
            minor_radius,
            ..
        } => torus_paths(
            center.as_array(),
            unit_of(axis_dir),
            major_radius,
            minor_radius,
            w,
        ),
    }
}

/// The torus's three cases (see the module docs' table).
fn torus_paths(
    c: [f64; 3],
    a: [f64; 3],
    major: f64,
    minor: f64,
    w: [f64; 3],
) -> Vec<(Path, Functional)> {
    let w_axial = dot(w, a);
    match axial_frame(a, w) {
        // Seen along the axis: the two equator circles `ρ = R ± r`, both in
        // the plane `τ = 0`.
        None => {
            let Some(e1) = unit(any_perpendicular(a)) else {
                return Vec::new();
            };
            let e2 = cross(a, e1);
            let mut out = Vec::new();
            for sgn in [1.0, -1.0] {
                out.push((
                    Path::Circle {
                        center: c,
                        e1,
                        e2,
                        radius: major + sgn * minor,
                    },
                    Functional::Plane { o: c, n: a },
                ));
            }
            out
        }
        Some((_, _, u1, u2)) if w_axial.abs() <= TORUS_AXIAL_DEGENERACY => {
            let mut out = Vec::new();
            // The two latitude circles `φ = ±π/2`: radius R, at `τ = ±r`.
            for sgn in [1.0, -1.0] {
                let centre = add(c, scaled(a, sgn * minor));
                out.push((
                    Path::Circle {
                        center: centre,
                        e1: u1,
                        e2: u2,
                        radius: major,
                    },
                    Functional::Plane { o: centre, n: a },
                ));
            }
            // The two profile circles `θ = ±π/2`: radius r, in the meridian
            // plane through the axis with normal `u₁`.
            for sgn in [1.0, -1.0] {
                let rad = scaled(u2, sgn);
                out.push((
                    Path::Circle {
                        center: add(c, scaled(rad, major)),
                        e1: rad,
                        e2: a,
                        radius: minor,
                    },
                    Functional::Plane { o: c, n: u1 },
                ));
            }
            out
        }
        Some((_, m, u1, u2)) => {
            let mut out = Vec::new();
            for flip in [false, true] {
                out.push((
                    Path::Branch {
                        center: c,
                        a,
                        u1,
                        u2,
                        major,
                        minor,
                        m,
                        w_axial,
                        flip,
                    },
                    Functional::TorusDot {
                        center: c,
                        a,
                        major,
                        w,
                    },
                ));
            }
            out
        }
    }
}

/// `(w·a, |w⊥|, û⊥, a × û⊥)` for an axial surface, or `None` when the line of
/// sight runs along the axis and there is no perpendicular component to frame.
fn axial_frame(a: [f64; 3], w: [f64; 3]) -> Option<(f64, f64, [f64; 3], [f64; 3])> {
    let w_axial = dot(w, a);
    let wp = [
        w[0] - w_axial * a[0],
        w[1] - w_axial * a[1],
        w[2] - w_axial * a[2],
    ];
    let m = norm(wp);
    if m.is_nan() || m <= TAU_NORMALIZE {
        return None;
    }
    let u1 = scaled(wp, 1.0 / m);
    Some((w_axial, m, u1, cross(a, u1)))
}

// ---------------------------------------------------------------------------
// path evaluation
// ---------------------------------------------------------------------------

impl Path {
    fn closed(&self) -> bool {
        !matches!(self, Path::Ruling { .. })
    }

    fn eval(&self, s: f64) -> Point3 {
        match *self {
            Path::Ruling { base, dir } => pt(add(base, scaled(dir, s))),
            Path::Circle {
                center,
                e1,
                e2,
                radius,
            } => {
                let (sn, cs) = s.sin_cos();
                pt(add(
                    center,
                    add(scaled(e1, radius * cs), scaled(e2, radius * sn)),
                ))
            }
            Path::Branch {
                center, a, u1, u2, ..
            } => {
                let (rho, tau, sn, cs) = self.branch_at(s);
                pt(add(
                    center,
                    add(
                        add(scaled(u1, rho * cs), scaled(u2, rho * sn)),
                        scaled(a, tau),
                    ),
                ))
            }
        }
    }

    /// `(ρ, τ, sin θ, cos θ)` of a branch at `θ = s`.
    fn branch_at(&self, s: f64) -> (f64, f64, f64, f64) {
        let Path::Branch {
            major,
            minor,
            m,
            w_axial,
            flip,
            ..
        } = *self
        else {
            unreachable!("branch_at is only called on a branch");
        };
        let (sn, cs) = s.sin_cos();
        let mut phi = (-m * cs).atan2(w_axial);
        if flip {
            phi += PI;
        }
        let (sp, cp) = phi.sin_cos();
        (major + minor * cp, minor * sp, sn, cs)
    }

    fn tangent(&self, s: f64) -> [f64; 3] {
        match *self {
            Path::Ruling { dir, .. } => dir,
            Path::Circle { e1, e2, radius, .. } => {
                let (sn, cs) = s.sin_cos();
                add(scaled(e1, -radius * sn), scaled(e2, radius * cs))
            }
            Path::Branch {
                a,
                u1,
                u2,
                minor,
                m,
                w_axial,
                flip,
                ..
            } => {
                let (sn, cs) = s.sin_cos();
                // `φ = atan2(−m cos θ, w∥)` ⟹ `dφ/dθ = w∥·m·sin θ / (w∥² + m²cos²θ)`.
                let den = w_axial * w_axial + m * m * cs * cs;
                let dphi = if den > 0.0 {
                    w_axial * m * sn / den
                } else {
                    0.0
                };
                let (rho, _, _, _) = self.branch_at(s);
                let mut phi = (-m * cs).atan2(w_axial);
                if flip {
                    phi += PI;
                }
                let (sp, cp) = phi.sin_cos();
                let drho = -minor * sp * dphi;
                let dtau = minor * cp * dphi;
                let radial = add(scaled(u1, cs), scaled(u2, sn));
                let dradial = add(scaled(u1, -sn), scaled(u2, cs));
                add(
                    add(scaled(radial, drho), scaled(dradial, rho)),
                    scaled(a, dtau),
                )
            }
        }
    }

    /// The path parameter nearest `p`.
    fn param_of(&self, p: Point3) -> f64 {
        match *self {
            Path::Ruling { base, dir } => dot(sub(p.as_array(), base), dir),
            Path::Circle { center, e1, e2, .. } => {
                let v = sub(p.as_array(), center);
                wrap_tau(dot(v, e2).atan2(dot(v, e1)))
            }
            Path::Branch { center, u1, u2, .. } => {
                let v = sub(p.as_array(), center);
                wrap_tau(dot(v, u2).atan2(dot(v, u1)))
            }
        }
    }

    /// Distance from `p` to the path — the test that separates one ruling from
    /// the other in the same axial plane, or one profile circle from its twin.
    fn off_path(&self, p: Point3) -> f64 {
        match *self {
            Path::Ruling { base, dir } => {
                let v = sub(p.as_array(), base);
                norm(cross(v, dir))
            }
            Path::Circle {
                center,
                e1,
                e2,
                radius,
            } => {
                let v = sub(p.as_array(), center);
                let n = cross(e1, e2);
                let tau = dot(v, n);
                let rho = norm([v[0] - tau * n[0], v[1] - tau * n[1], v[2] - tau * n[2]]);
                tau.hypot(rho - radius)
            }
            Path::Branch { .. } => {
                let q = self.eval(self.param_of(p));
                norm(sub(p.as_array(), q.as_array()))
            }
        }
    }
}

// ---------------------------------------------------------------------------
// the clip
// ---------------------------------------------------------------------------

struct Crossing {
    s: f64,
    enter: bool,
}

/// The parameter intervals of `path` that lie on the face.
#[allow(clippy::too_many_arguments)]
fn clip_path(
    arena: &BrepArena,
    fid: FaceId,
    surface: &Surface,
    path: &Path,
    functional: &Functional,
    hes: &[HalfEdgeId],
    scale: f64,
    n_seg: u32,
) -> Result<Vec<(f64, f64)>, KernelV2Error> {
    let tol_on_path = 1e-6 * scale + TAU_MODEL;
    let mut xs: Vec<Crossing> = Vec::new();
    let (mut s_lo, mut s_hi) = (f64::INFINITY, f64::NEG_INFINITY);

    for &h in hes {
        if !path.closed() {
            let o = arena.vertex(arena.half_edge(h)?.origin)?.point;
            let s = path.param_of(o);
            s_lo = s_lo.min(s);
            s_hi = s_hi.max(s);
        }
        for (p, tangent) in edge_crossings(arena, h, functional, n_seg)? {
            if path.off_path(p) > tol_on_path {
                continue;
            }
            let Some(n) = crate::signature::outward_normal_at(surface, p) else {
                continue; // the normal is undefined here (a cone's apex)
            };
            let s = path.param_of(p);
            let sdir = path.tangent(s);
            let inward = cross(n, tangent);
            let d = dot(sdir, inward);
            let mag = norm(sdir) * norm(tangent);
            if mag.is_nan() || mag <= 0.0 || d.abs() <= TANGENCY_REL * mag {
                continue; // a tangency does not toggle
            }
            if !path.closed() {
                s_lo = s_lo.min(s);
                s_hi = s_hi.max(s);
            }
            xs.push(Crossing { s, enter: d > 0.0 });
        }
    }

    xs.sort_by(|a, b| a.s.total_cmp(&b.s));
    // Two half-edges meeting at a vertex ON the silhouette, and a full circle
    // whose closed-form roots land on both ends of its parameter window, each
    // report the same crossing twice.
    let eps = if path.closed() { 1e-9 } else { 1e-9 * scale };
    xs.dedup_by(|b, a| (b.s - a.s).abs() <= eps && b.enter == a.enter);

    if path.closed() {
        if xs.is_empty() {
            // Wholly inside or wholly outside, and nothing local decides
            // which — unless the face has no boundary but its seams, in which
            // case it IS the whole closed surface.
            if hes.is_empty() {
                return Ok(vec![(0.0, TAU)]);
            }
            census(fid, "closed path with no crossings on a bounded face");
            return Ok(Vec::new());
        }
        let n = xs.len();
        let mut out = Vec::new();
        for i in 0..n {
            let j = (i + 1) % n;
            if xs[i].enter == xs[j].enter {
                census(fid, "closed path crossings do not alternate");
                return Ok(Vec::new());
            }
            if xs[i].enter {
                let mut s1 = xs[j].s;
                if s1 <= xs[i].s {
                    s1 += TAU;
                }
                out.push((xs[i].s, s1));
            }
        }
        return Ok(out);
    }

    // An open path: clamp an unbalanced run to the boundary's own extent.
    let mut out = Vec::new();
    let mut open: Option<f64> = None;
    if xs.first().is_some_and(|c| !c.enter) {
        open = Some(s_lo.min(xs[0].s));
    }
    for c in &xs {
        if c.enter {
            if open.is_none() {
                open = Some(c.s);
            }
        } else if let Some(a) = open.take() {
            if c.s > a {
                out.push((a, c.s));
            }
        }
    }
    if let Some(a) = open.take() {
        let b = s_hi.max(a);
        if b > a {
            out.push((a, b));
        }
    }
    Ok(out)
}

/// Print-only census of a declined clip, under `KV2_SILHOUETTE_CENSUS`.
fn census(fid: FaceId, reason: &str) {
    if std::env::var_os("KV2_SILHOUETTE_CENSUS").is_some() {
        println!("[silhouette] face {}: declined — {reason}", fid.0);
    }
}

/// The face's geometric scale: its surface's own radius, grown to its
/// boundary's spread, so a tolerance relative to it is meaningful for both a
/// tiny patch of a huge cylinder and a full primitive.
fn face_scale(arena: &BrepArena, fid: FaceId, surface: &Surface) -> Result<f64, KernelV2Error> {
    let mut scale = match *surface {
        Surface::Plane(_) => 0.0,
        Surface::Cylinder { radius, .. } | Surface::Sphere { radius, .. } => radius,
        Surface::Cone { .. } => 0.0,
        Surface::Torus {
            major_radius,
            minor_radius,
            ..
        } => major_radius + minor_radius,
    };
    let face = arena.face(fid)?;
    let mut loops = vec![face.outer_loop];
    loops.extend(face.inner_loops.iter().copied());
    let anchor = match *surface {
        Surface::Cone { apex, .. } => Some(apex),
        Surface::Cylinder { axis_point, .. } => Some(axis_point),
        Surface::Sphere { center, .. } => Some(center),
        Surface::Torus { center, .. } => Some(center),
        Surface::Plane(_) => None,
    };
    if let Some(anchor) = anchor {
        for lid in loops {
            for p in arena.loop_points(lid)? {
                scale = scale.max(norm(sub(p.as_array(), anchor.as_array())));
            }
        }
    }
    Ok(if scale > 0.0 { scale } else { 1.0 })
}

// ---------------------------------------------------------------------------
// crossings of a boundary edge with a functional
// ---------------------------------------------------------------------------

/// The edge's own parameterization over `t ∈ [0, 1]`, in the half-edge's
/// traversal direction.
enum EdgeParam {
    Line {
        a: [f64; 3],
        b: [f64; 3],
    },
    /// `center + r₁·cos ϑ·f₁ + r₂·sin ϑ·f₂`, `ϑ = t0 + t·sweep` — a circle, a
    /// circular arc and an elliptical arc all at once.
    Conic {
        center: [f64; 3],
        f1: [f64; 3],
        f2: [f64; 3],
        r1: f64,
        r2: f64,
        t0: f64,
        sweep: f64,
    },
    /// `center + a·cosh ϑ·f₁ + b·sinh ϑ·f₂`, `ϑ = t0 + t·(t1 − t0)`.
    Hyper {
        center: [f64; 3],
        f1: [f64; 3],
        f2: [f64; 3],
        a: f64,
        b: f64,
        t0: f64,
        t1: f64,
    },
    /// The render polyline, for the curve with no closed form of its own.
    Poly {
        points: Vec<Point3>,
    },
}

impl EdgeParam {
    fn eval(&self, t: f64) -> Point3 {
        match self {
            EdgeParam::Line { a, b } => pt([
                a[0] + t * (b[0] - a[0]),
                a[1] + t * (b[1] - a[1]),
                a[2] + t * (b[2] - a[2]),
            ]),
            EdgeParam::Conic {
                center,
                f1,
                f2,
                r1,
                r2,
                t0,
                sweep,
            } => {
                let (sn, cs) = (t0 + t * sweep).sin_cos();
                pt(add(
                    *center,
                    add(scaled(*f1, r1 * cs), scaled(*f2, r2 * sn)),
                ))
            }
            EdgeParam::Hyper {
                center,
                f1,
                f2,
                a,
                b,
                t0,
                t1,
            } => {
                let v = t0 + t * (t1 - t0);
                pt(add(
                    *center,
                    add(scaled(*f1, a * v.cosh()), scaled(*f2, b * v.sinh())),
                ))
            }
            EdgeParam::Poly { points } => {
                if points.len() < 2 {
                    return points
                        .first()
                        .copied()
                        .unwrap_or(Point3::new(0.0, 0.0, 0.0));
                }
                let n = points.len() - 1;
                let x = (t.clamp(0.0, 1.0)) * n as f64;
                let i = (x.floor() as usize).min(n - 1);
                let f = x - i as f64;
                let (a, b) = (points[i].as_array(), points[i + 1].as_array());
                pt([
                    a[0] + f * (b[0] - a[0]),
                    a[1] + f * (b[1] - a[1]),
                    a[2] + f * (b[2] - a[2]),
                ])
            }
        }
    }

    /// Tangent in the traversal direction at `t` (not normalized).
    fn tangent(&self, t: f64) -> [f64; 3] {
        match self {
            EdgeParam::Line { a, b } => sub(*b, *a),
            EdgeParam::Conic {
                f1,
                f2,
                r1,
                r2,
                t0,
                sweep,
                ..
            } => {
                let (sn, cs) = (t0 + t * sweep).sin_cos();
                scaled(
                    add(scaled(*f1, -r1 * sn), scaled(*f2, r2 * cs)),
                    sweep.signum(),
                )
            }
            EdgeParam::Hyper {
                f1,
                f2,
                a,
                b,
                t0,
                t1,
                ..
            } => {
                let v = t0 + t * (t1 - t0);
                scaled(
                    add(scaled(*f1, a * v.sinh()), scaled(*f2, b * v.cosh())),
                    (t1 - t0).signum(),
                )
            }
            EdgeParam::Poly { points } => {
                if points.len() < 2 {
                    return [0.0; 3];
                }
                let n = points.len() - 1;
                let x = (t.clamp(0.0, 1.0)) * n as f64;
                let i = (x.floor() as usize).min(n - 1);
                sub(points[i + 1].as_array(), points[i].as_array())
            }
        }
    }
}

/// The half-edge's curve as a parameterization over `[0, 1]`.
fn edge_param(arena: &BrepArena, h: HalfEdgeId, n_seg: u32) -> Result<EdgeParam, KernelV2Error> {
    let he = arena.half_edge(h)?;
    let start = arena.vertex(he.origin)?.point;
    let end = arena.vertex(arena.half_edge(he.next)?.origin)?.point;
    Ok(match he.curve {
        Curve::LineSegment => EdgeParam::Line {
            a: start.as_array(),
            b: end.as_array(),
        },
        Curve::Circle {
            center,
            normal,
            radius,
        }
        | Curve::Arc {
            center,
            normal,
            radius,
        } => {
            let Some((f1, f2)) = crate::tessellate::circle_frame(center, normal, start) else {
                return Ok(EdgeParam::Poly {
                    points: crate::introspect::edge_polyline(arena, h, n_seg)?,
                });
            };
            let sweep = if matches!(he.curve, Curve::Circle { .. }) {
                TAU
            } else {
                match crate::geom::ccw_sweep(center, [normal.x, normal.y, normal.z], start, end) {
                    Some(s) => s,
                    None => {
                        return Ok(EdgeParam::Poly {
                            points: crate::introspect::edge_polyline(arena, h, n_seg)?,
                        })
                    }
                }
            };
            EdgeParam::Conic {
                center: center.as_array(),
                f1,
                f2,
                r1: radius,
                r2: radius,
                t0: 0.0,
                sweep,
            }
        }
        Curve::EllipseArc {
            center,
            normal,
            major_axis,
            major_radius,
            minor_radius,
        } => {
            let f1 = unit_of(major_axis);
            let f2 = cross(unit_of(normal), f1);
            let angle = |p: Point3| -> f64 {
                let v = sub(p.as_array(), center.as_array());
                (dot(v, f2) / minor_radius).atan2(dot(v, f1) / major_radius)
            };
            let t0 = angle(start);
            let mut sweep = angle(end) - t0;
            while sweep <= 1e-12 {
                sweep += TAU;
            }
            EdgeParam::Conic {
                center: center.as_array(),
                f1,
                f2,
                r1: major_radius,
                r2: minor_radius,
                t0,
                sweep,
            }
        }
        Curve::HyperbolaArc {
            center,
            normal,
            major_axis,
            semi_transverse,
            semi_conjugate,
        } => {
            let f1 = unit_of(major_axis);
            let f2 = cross(unit_of(normal), f1);
            let param = |p: Point3| -> f64 {
                let v = sub(p.as_array(), center.as_array());
                (dot(v, f2) / semi_conjugate).asinh()
            };
            EdgeParam::Hyper {
                center: center.as_array(),
                f1,
                f2,
                a: semi_transverse,
                b: semi_conjugate,
                t0: param(start),
                t1: param(end),
            }
        }
        Curve::SurfacePair { .. } => EdgeParam::Poly {
            points: crate::introspect::edge_polyline(arena, h, n_seg)?,
        },
    })
}

/// `(point, traversal tangent)` at every crossing of the half-edge with the
/// functional's zero set.
fn edge_crossings(
    arena: &BrepArena,
    h: HalfEdgeId,
    functional: &Functional,
    n_seg: u32,
) -> Result<Vec<(Point3, [f64; 3])>, KernelV2Error> {
    let param = edge_param(arena, h, n_seg)?;
    let roots = match (functional, &param) {
        (Functional::Plane { o, n }, _) => plane_roots(&param, *o, *n),
        (Functional::TorusDot { .. }, _) => None,
    }
    .unwrap_or_else(|| numeric_roots(&param, functional, n_seg));
    Ok(roots
        .into_iter()
        .map(|t| (param.eval(t), param.tangent(t)))
        .collect())
}

/// Closed-form roots of a plane's signed distance along the edge, as `t`
/// parameters in `[0, 1]`, or `None` when this arm has no closed form.
fn plane_roots(param: &EdgeParam, o: [f64; 3], n: [f64; 3]) -> Option<Vec<f64>> {
    match *param {
        EdgeParam::Line { a, b } => {
            let (ha, hb) = (dot(sub(a, o), n), dot(sub(b, o), n));
            let d = ha - hb;
            if d == 0.0 {
                return Some(Vec::new());
            }
            let t = ha / d;
            Some(if (-1e-12..=1.0 + 1e-12).contains(&t) {
                vec![t.clamp(0.0, 1.0)]
            } else {
                Vec::new()
            })
        }
        EdgeParam::Conic {
            center,
            f1,
            f2,
            r1,
            r2,
            t0,
            sweep,
        } => {
            // `C + A cos ϑ + B sin ϑ = 0`.
            let c = dot(sub(center, o), n);
            let a = r1 * dot(f1, n);
            let b = r2 * dot(f2, n);
            let rho = a.hypot(b);
            if rho.is_nan() || rho <= 0.0 {
                return Some(Vec::new());
            }
            let ratio = -c / rho;
            if !ratio.is_finite() || ratio.abs() > 1.0 {
                return Some(Vec::new());
            }
            let psi = b.atan2(a);
            let delta = ratio.clamp(-1.0, 1.0).acos();
            let mut out = Vec::new();
            for base in [psi + delta, psi - delta] {
                // Lift into the edge's own parameter window.
                let k0 = ((t0 - base) / TAU).floor();
                for k in [k0, k0 + 1.0, k0 + 2.0] {
                    let theta = base + k * TAU;
                    let t = (theta - t0) / sweep;
                    if (-1e-12..1.0 - 1e-12).contains(&t)
                        || (sweep < TAU - 1e-9 && (1.0 - 1e-12..=1.0 + 1e-12).contains(&t))
                    {
                        out.push(t.clamp(0.0, 1.0));
                    }
                }
            }
            Some(out)
        }
        EdgeParam::Hyper {
            center,
            f1,
            f2,
            a,
            b,
            t0,
            t1,
        } => {
            // `C + A cosh ϑ + B sinh ϑ = 0`, with `X = eˆϑ`:
            // `(A + B)X² + 2C X + (A − B) = 0`.
            let c = dot(sub(center, o), n);
            let aa = a * dot(f1, n);
            let bb = b * dot(f2, n);
            let (p, q, r) = (aa + bb, 2.0 * c, aa - bb);
            let mut xs: Vec<f64> = Vec::new();
            if p.abs() <= f64::MIN_POSITIVE {
                if q != 0.0 {
                    xs.push(-r / q);
                }
            } else {
                let disc = q * q - 4.0 * p * r;
                if disc >= 0.0 {
                    let s = disc.sqrt();
                    xs.push((-q + s) / (2.0 * p));
                    xs.push((-q - s) / (2.0 * p));
                }
            }
            let (lo, hi) = (t0.min(t1), t0.max(t1));
            let span = t1 - t0;
            let mut out = Vec::new();
            for x in xs {
                if !x.is_finite() || x <= 0.0 {
                    continue;
                }
                let theta = x.ln();
                if theta < lo - 1e-12 || theta > hi + 1e-12 || span == 0.0 {
                    continue;
                }
                out.push(((theta - t0) / span).clamp(0.0, 1.0));
            }
            Some(out)
        }
        EdgeParam::Poly { .. } => None,
    }
}

/// Bracketed, bisected roots — for the pairs with no closed form (a
/// surface-pair boundary edge, and any edge against the torus functional).
fn numeric_roots(param: &EdgeParam, functional: &Functional, n_seg: u32) -> Vec<f64> {
    let n = match param {
        EdgeParam::Poly { points } => points.len().saturating_sub(1).max(1),
        _ => (n_seg as usize).max(256),
    };
    let f = |t: f64| functional.at(param.eval(t));
    let mut out = Vec::new();
    let mut prev_t = 0.0;
    let mut prev = f(0.0);
    for i in 1..=n {
        let t = i as f64 / n as f64;
        let cur = f(t);
        if prev.is_finite() && cur.is_finite() {
            if prev == 0.0 {
                out.push(prev_t);
            } else if (prev < 0.0) != (cur < 0.0) {
                // 80 halvings takes any bracket well below f64 resolution.
                let (mut lo, mut hi, mut flo) = (prev_t, t, prev);
                for _ in 0..80 {
                    let mid = 0.5 * (lo + hi);
                    let fm = f(mid);
                    if !fm.is_finite() {
                        break;
                    }
                    if (flo < 0.0) != (fm < 0.0) {
                        hi = mid;
                    } else {
                        lo = mid;
                        flo = fm;
                    }
                }
                out.push(0.5 * (lo + hi));
            }
        }
        prev_t = t;
        prev = cur;
    }
    out
}

// ---------------------------------------------------------------------------
// projecting a clipped interval
// ---------------------------------------------------------------------------

/// The projection of `path` restricted to `[s0, s1]`, analytic where it can
/// be. `None` when the interval projects to a single point, which adds nothing
/// to a drawing the rims do not already carry.
fn project_interval(
    basis: &ViewBasis,
    path: &Path,
    s0: f64,
    s1: f64,
    n_seg: u32,
) -> Option<Curve2> {
    match *path {
        Path::Ruling { .. } => {
            let a = super::project_point(basis, path.eval(s0));
            let b = super::project_point(basis, path.eval(s1));
            match super::line_or_point(a, b) {
                Curve2::Point(_) => None,
                c => Some(c),
            }
        }
        Path::Circle {
            center,
            e1,
            e2,
            radius,
        } => {
            let normal = unit(cross(e1, e2))?;
            let nu = UnitVector3 {
                x: normal[0],
                y: normal[1],
                z: normal[2],
            };
            let full = s1 - s0 >= TAU - 1e-12;
            let end = if full { None } else { Some(path.eval(s1)) };
            super::project_circle(basis, pt(center), nu, radius, path.eval(s0), end)
                .filter(|c| !matches!(c, Curve2::Point(_)))
                .or_else(|| sampled(basis, path, s0, s1, radius, n_seg))
        }
        Path::Branch { minor, .. } => sampled(basis, path, s0, s1, minor, n_seg),
    }
}

/// A path interval as a projected polyline, refined until every chord's
/// midpoint is within the render sagitta of the curve.
///
/// The sagitta target is the render band's own: an `n_seg`-gon inscribed in a
/// circle of radius `scale` deviates by `scale·(1 − cos(π/n_seg))`, so a
/// silhouette polyline and the rendered surface agree to the same order. The
/// refinement is adaptive rather than uniform because a torus branch's speed
/// in `θ` is wildly uneven as `w·a → 0` — uniform sampling would miss the
/// steep stretch entirely while over-sampling the flat one.
fn sampled(
    basis: &ViewBasis,
    path: &Path,
    s0: f64,
    s1: f64,
    scale: f64,
    n_seg: u32,
) -> Option<Curve2> {
    const SEED: usize = 24;
    const MAX_DEPTH: u32 = 22;
    let sag = scale * (1.0 - (PI / f64::from(n_seg.max(3))).cos());
    let sag = if sag > 0.0 { sag } else { TAU_MODEL };

    let mut params: Vec<f64> = (0..=SEED)
        .map(|i| s0 + (s1 - s0) * (i as f64) / (SEED as f64))
        .collect();
    let mut refined: Vec<f64> = Vec::with_capacity(params.len());
    // Depth-first refinement of each seed interval.
    let mut stack: Vec<(f64, f64, u32)> = Vec::new();
    refined.push(params[0]);
    for win in params.windows(2) {
        stack.clear();
        stack.push((win[0], win[1], 0));
        while let Some((a, b, depth)) = stack.pop() {
            let mid = 0.5 * (a + b);
            let deviation = {
                let (pa, pb, pm) = (path.eval(a), path.eval(b), path.eval(mid));
                let chord = [
                    0.5 * (pa.x() + pb.x()),
                    0.5 * (pa.y() + pb.y()),
                    0.5 * (pa.z() + pb.z()),
                ];
                norm(sub(pm.as_array(), chord))
            };
            if depth < MAX_DEPTH && deviation > sag {
                stack.push((mid, b, depth + 1));
                stack.push((a, mid, depth + 1));
            } else {
                refined.push(b);
            }
        }
    }
    params = refined;

    let closed = s1 - s0 >= TAU - 1e-12;
    let mut points: Vec<Point2> = params
        .iter()
        .map(|s| super::project_point(basis, path.eval(*s)))
        .collect();
    if closed && points.len() > 2 {
        points.pop();
    }
    if points.len() < 2 {
        return None;
    }
    Some(Curve2::Polyline { points, closed })
}

// ---------------------------------------------------------------------------
// small vector helpers
// ---------------------------------------------------------------------------

fn pt(a: [f64; 3]) -> Point3 {
    Point3::new(a[0], a[1], a[2])
}

fn unit_of(v: UnitVector3) -> [f64; 3] {
    [v.x, v.y, v.z]
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

fn norm(a: [f64; 3]) -> f64 {
    dot(a, a).sqrt()
}

fn unit(a: [f64; 3]) -> Option<[f64; 3]> {
    let n = norm(a);
    (n.is_finite() && n > TAU_NORMALIZE).then(|| [a[0] / n, a[1] / n, a[2] / n])
}

fn add(a: [f64; 3], b: [f64; 3]) -> [f64; 3] {
    [a[0] + b[0], a[1] + b[1], a[2] + b[2]]
}

fn sub(a: [f64; 3], b: [f64; 3]) -> [f64; 3] {
    [a[0] - b[0], a[1] - b[1], a[2] - b[2]]
}

fn scaled(a: [f64; 3], s: f64) -> [f64; 3] {
    [a[0] * s, a[1] * s, a[2] * s]
}

/// Any unit-length direction perpendicular to `v`.
fn any_perpendicular(v: [f64; 3]) -> [f64; 3] {
    let seed = if v[0].abs() < 0.9 {
        [1.0, 0.0, 0.0]
    } else {
        [0.0, 1.0, 0.0]
    };
    let t = dot(seed, v);
    [seed[0] - t * v[0], seed[1] - t * v[1], seed[2] - t * v[2]]
}

/// `x` wrapped into `[0, 2π)`.
fn wrap_tau(x: f64) -> f64 {
    let y = x % TAU;
    if y < 0.0 {
        y + TAU
    } else {
        y
    }
}

#[cfg(test)]
mod tests;
