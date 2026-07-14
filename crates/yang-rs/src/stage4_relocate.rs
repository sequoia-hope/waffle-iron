//! Stage 4 — relocation of mesh intersection points onto exact curves
//! (Yang §4.4.1) + §4.5.3 reversed-intersection correction primitives
//! (extracted verbatim from lib.rs — spec
//! `specs/yang_rs_lib_decomposition.md`, increment 5).

#[allow(clippy::wildcard_imports)]
use crate::*;

// =========================================================================
// PR-YR10 — Stage 4: relocate mesh intersection points onto exact curves
// (Yang §4.4.1 mesh updating) + §4.5.3 reversed-intersection correction.
// =========================================================================

/// PR-YR10 (spec §4.3): closed-form radial projection of `p` onto the exact
/// `Circle { center, normal, radius }`. Returns `(proj, t)` where `t` is the
/// angle in the circle's `ortho_basis(normal)` frame — the SAME frame Stage-1
/// sampling and [`BRep::eval_source`] use, so a relocated vertex tagged
/// `BRepEdge { edge, t }` round-trips exactly.
///
/// `Err(OnAxis)` when the point's radial component is below `MIN_FEATURE_SIZE`
/// (the projection direction is undefined on the axis). No Newton, no tolerance
/// widening (P9).
pub(crate) fn project_onto_circle(
    p: Point3,
    center: Point3,
    normal: Vector3,
    radius: f64,
) -> Result<(Point3, f64), Stage4InvalidReason> {
    let (e1, e2) = ortho_basis(normal);
    let e1a = e1.as_array();
    let e2a = e2.as_array();
    let c = center.as_array();
    let x = p.as_array();
    let w = [x[0] - c[0], x[1] - c[1], x[2] - c[2]];
    let u = w[0] * e1a[0] + w[1] * e1a[1] + w[2] * e1a[2];
    let v = w[0] * e2a[0] + w[1] * e2a[1] + w[2] * e2a[2];
    let rho = u.hypot(v);
    if rho < cad_primitives::MIN_FEATURE_SIZE {
        return Err(Stage4InvalidReason::OnAxis);
    }
    let t = v.atan2(u);
    let (ct, st) = (t.cos(), t.sin());
    let proj = Point3::new(
        c[0] + radius * (ct * e1a[0] + st * e2a[0]),
        c[1] + radius * (ct * e1a[1] + st * e2a[1]),
        c[2] + radius * (ct * e1a[2] + st * e2a[2]),
    );
    Ok((proj, t))
}

/// KV6d Tier B: the implicit value `F(x)` and UNIT gradient (the surface's
/// analytic unit normal) of a `Surface` at `x`. `F` matches
/// [`signed_distance_to_surface`] byte-for-byte (so the residual gate is
/// shared), and the gradient is `∇F / |∇F|`. Returns `None` where the normal is
/// undefined — a point on a cylinder/torus axis, a cone apex, a sphere centre,
/// or (torus) a point on the tube centre circle — which the caller treats as a
/// loud STOP, never a guess (P9).
/// Increment 4 §4d (spec `yang_rim_junction_insertion`): scale-aware
/// exactness certificate band for a signed surface distance evaluated at
/// `p`. f64 evaluation of `surface_value_and_normal` at coordinate
/// magnitude L carries O(ε·L) rounding, so an ABSOLUTE `TAU_WORK = 1e-12`
/// is ~2 ULP at magnitude 4000 — unreachable by ANY correct evaluation
/// (R0017's already-exact junctions measured 1.36e-12 ≈ 1.2·ε·L). The
/// band certifies "exact to evaluation precision": `max(TAU_WORK,
/// 8·ε·L)` with L = |p|∞ + |surface reference|∞ — never narrower than the
/// shipped increment-3 band (spec I7), and ≥10 orders below the Stage-1
/// chord bound (d_ε = 1e-2·diag) at the same scale, so chord-sagitta
/// inexactness can never certify. NOT a tolerance widening (P9): the
/// property witnessed is the strongest float arithmetic can express.
pub(crate) fn junction_certificate_band(p: [f64; 3], s: Surface) -> f64 {
    let mag3 = |v: [f64; 3]| v[0].abs().max(v[1].abs()).max(v[2].abs());
    let refmag = match s {
        Surface::Plane { normal, d } => {
            let n = normal.as_array();
            let nl = (n[0] * n[0] + n[1] * n[1] + n[2] * n[2]).sqrt();
            if nl > 0.0 {
                d.abs() / nl
            } else {
                d.abs()
            }
        }
        Surface::Sphere { center, radius } => mag3(center.as_array()) + radius.abs(),
        Surface::Cylinder {
            axis_point, radius, ..
        } => mag3(axis_point.as_array()) + radius.abs(),
        Surface::Cone { apex, .. } => mag3(apex.as_array()),
        Surface::Torus {
            center,
            major_radius,
            minor_radius,
            ..
        } => mag3(center.as_array()) + major_radius.abs() + minor_radius.abs(),
    };
    let l = mag3(p) + refmag;
    cad_primitives::TAU_WORK.max(8.0 * f64::EPSILON * l)
}

/// Tangency cutoff (unit sine) for the CIRCLE-PAIR membership
/// amplification: sin α at/below this is tangent-grade — the constraint
/// band diverges — so the amplification returns `None` and the caller
/// falls back to the tangent-direction discriminator (the SAFE direction).
pub(crate) const AMP_TANGENCY_MIN_SIN_CIRCLE_PAIR: f64 = cad_primitives::MIN_FEATURE_SIZE;

/// Tangency cutoff (unit sine) for the CYL×CYL radial-gradient
/// amplification. F8 HONEST NOTE (design review 2026-07-12): this is 1000×
/// coarser than [`AMP_TANGENCY_MIN_SIN_CIRCLE_PAIR`]. Both cutoffs trigger
/// the SAFE fallback (`None` → tangent-direction discriminator), so the
/// spread is a conservatism difference between two amplification forms,
/// not a correctness fudge — but it is UNJUSTIFIED spread; unifying the two
/// requires a full-assay measurement (banked debt, deviations ledger).
pub(crate) const AMP_TANGENCY_MIN_SIN_CYL_CYL: f64 = 1e-3;

/// Work floor for the torus-block relocation convergence/certificate
/// band. F8 HONEST NOTE (design review 2026-07-12): `1e-13`, i.e. 10×
/// TIGHTER than the crate's `TAU_WORK` (1e-12) — the value was chosen when
/// the torus-block relocation shipped so that behavior stayed
/// byte-identical (see the comment at the use site). Unifying it up to
/// `TAU_WORK` is a behavior change requiring a full-assay measurement
/// (banked debt, deviations ledger). Never lower it further to pass a case
/// (P9).
pub(crate) const TORUS_RELOC_WORK_FLOOR: f64 = 1e-13;

pub(crate) fn surface_value_and_normal(s: Surface, x: [f64; 3]) -> Option<(f64, [f64; 3])> {
    let eps = cad_primitives::MIN_FEATURE_SIZE;
    match s {
        Surface::Plane { normal, d } => {
            let n = normal.as_array();
            let f = n[0] * x[0] + n[1] * x[1] + n[2] * x[2] + d;
            Some((f, normalize3(n)))
        }
        Surface::Sphere { center, radius } => {
            let c = center.as_array();
            let w = [x[0] - c[0], x[1] - c[1], x[2] - c[2]];
            let l = (w[0] * w[0] + w[1] * w[1] + w[2] * w[2]).sqrt();
            if l < eps {
                return None;
            }
            Some((l - radius, [w[0] / l, w[1] / l, w[2] / l]))
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
            let rad = [
                w[0] - along * au[0],
                w[1] - along * au[1],
                w[2] - along * au[2],
            ];
            let l = (rad[0] * rad[0] + rad[1] * rad[1] + rad[2] * rad[2]).sqrt();
            if l < eps {
                return None;
            }
            Some((l - radius, [rad[0] / l, rad[1] / l, rad[2] / l]))
        }
        Surface::Cone {
            apex,
            axis_dir,
            half_angle,
        } => {
            let au = normalize3(axis_dir.as_array());
            let a = apex.as_array();
            let w = [x[0] - a[0], x[1] - a[1], x[2] - a[2]];
            let h = w[0] * au[0] + w[1] * au[1] + w[2] * au[2];
            let rad = [w[0] - h * au[0], w[1] - h * au[1], w[2] - h * au[2]];
            let l = (rad[0] * rad[0] + rad[1] * rad[1] + rad[2] * rad[2]).sqrt();
            if l < eps {
                return None;
            }
            let f = l - h.abs() * half_angle.tan();
            // ∇F = r̂ − sign(h)·tanα·â ; its unit form is the cone normal
            // cosα·r̂ − sign(h)·sinα·â.
            let sgn = if h >= 0.0 { 1.0 } else { -1.0 };
            let (sa, ca) = half_angle.sin_cos();
            let g = [
                ca * rad[0] / l - sgn * sa * au[0],
                ca * rad[1] / l - sgn * sa * au[1],
                ca * rad[2] / l - sgn * sa * au[2],
            ];
            Some((f, normalize3(g)))
        }
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
            let rad = [w[0] - tau * au[0], w[1] - tau * au[1], w[2] - tau * au[2]];
            let rho = (rad[0] * rad[0] + rad[1] * rad[1] + rad[2] * rad[2]).sqrt();
            if rho < eps {
                return None; // on the torus axis: radial direction undefined
            }
            let rhat = [rad[0] / rho, rad[1] / rho, rad[2] / rho];
            // Nearest tube-centre-circle point q = c + R·r̂; normal = (x − q)/|x − q|.
            let q = [
                c[0] + major_radius * rhat[0],
                c[1] + major_radius * rhat[1],
                c[2] + major_radius * rhat[2],
            ];
            let xq = [x[0] - q[0], x[1] - q[1], x[2] - q[2]];
            let l = (xq[0] * xq[0] + xq[1] * xq[1] + xq[2] * xq[2]).sqrt();
            if l < eps {
                return None; // on the tube centre circle: normal undefined
            }
            Some((l - minor_radius, [xq[0] / l, xq[1] / l, xq[2] / l]))
        }
    }
}

/// KV6d Tier B: relocate `p` onto the exact intersection curve of two surfaces
/// by Gauss–Newton on the implicit system `{F0(x)=0, F1(x)=0}` — the degree-4
/// analog of the closed-form conic projectors, used when a torus is one of the
/// pair (a torus's intersections are not conics, so there is no closed form).
/// Each step is the least-norm solution of `J·dx = −[F0; F1]`, with
/// `J = [n̂0; n̂1]` the 2×3 unit-normal Jacobian; for unit rows
/// `J Jᵀ = [[1, b], [b, 1]]`, `b = n̂0·n̂1`.
///
/// Returns the relocated point with both residuals ≤ `TAU_MODEL`, or `None` for
/// a loud STOP (P9 — never a partial move or a guessed root):
/// - a TANGENTIAL pair (`sin²θ = 1 − b² ≤ MIN_FEATURE_SIZE²`): `J` is rank-
///   deficient and the intersection root is ill-posed;
/// - an UNDEFINED normal at the iterate (axis / apex / centre circle);
/// - NON-CONVERGENCE within `MAX_ITERS`.
pub(crate) fn relocate_onto_implicit_pair(p: Point3, s0: Surface, s1: Surface) -> Option<Point3> {
    const MAX_ITERS: usize = 32;
    // Converge tightly (well below the 1e-12 on-surface validation band; the
    // torus residual is ~2·minor·|F|): Newton is quadratic so this is a few
    // extra cheap steps. Absolute tol suits the unit-scale model corpus.
    let tau = 1e-13_f64;
    let rank_eps = cad_primitives::MIN_FEATURE_SIZE * cad_primitives::MIN_FEATURE_SIZE;
    let mut x = p.as_array();
    for _ in 0..=MAX_ITERS {
        let (f0, n0) = surface_value_and_normal(s0, x)?;
        let (f1, n1) = surface_value_and_normal(s1, x)?;
        let b = n0[0] * n1[0] + n0[1] * n1[1] + n0[2] * n1[2];
        let det = 1.0 - b * b; // sin²θ between the two unit normals
                               // Tangential / parallel normals → no transversal 1D intersection curve
                               // to relocate onto (the contact is a point or a higher-order tangency).
                               // STOP whether or not the residual is already small — a tangent point IS
                               // on both surfaces but is not a curve a mesh edge can lie along.
        if det <= rank_eps {
            return None;
        }
        if f0.abs() <= tau && f1.abs() <= tau {
            return Some(Point3::new(x[0], x[1], x[2]));
        }
        // (J Jᵀ)⁻¹ [f0; f1] = 1/det · [[1, −b], [−b, 1]] [f0; f1]
        let m0 = (f0 - b * f1) / det;
        let m1 = (f1 - b * f0) / det;
        // dx = −Jᵀ m = −(n̂0·m0 + n̂1·m1)
        x = [
            x[0] - (n0[0] * m0 + n1[0] * m1),
            x[1] - (n0[1] * m0 + n1[1] * m1),
            x[2] - (n0[2] * m0 + n1[2] * m1),
        ];
    }
    None
}

/// KV6d Tier B junction: relocate `p` onto the common point of THREE surfaces
/// `{F0=0, F1=0, F2=0}` by Newton on the square system (3×3 Jacobian of unit
/// normals). The torus analog of the conic line+circle / ellipse×ellipse
/// junctions — e.g. a box EDGE (two planes) piercing the torus: the shared
/// vertex must land on the torus AND both planes, else relocating it onto only
/// one pair slides it off the third. `None` is a loud STOP: a degenerate
/// (near-coplanar normals → `|det J|` below the rank floor) junction, an
/// undefined normal, or non-convergence (P9 — no partial move).
pub(crate) fn relocate_onto_implicit_triple(
    p: Point3,
    s0: Surface,
    s1: Surface,
    s2: Surface,
) -> Option<Point3> {
    const MAX_ITERS: usize = 32;
    // Converge tightly (well below the 1e-12 on-surface validation band; the
    // torus residual is ~2·minor·|F|): Newton is quadratic so this is a few
    // extra cheap steps. Increment 5 (spec `yang_stage4_conic_triple_junction`
    // wiring amendment): the absolute 1e-13 floor is sub-ULP at coordinate
    // magnitude ~4000 (the R0017 corpus scale) and could never converge
    // there — take the max with the same 8·ε·L evaluation-precision term as
    // the increment-4 certificate band. At unit scale 8·ε·L ≈ 5e-15 < 1e-13,
    // so the shipped torus-block behavior is byte-identical.
    let mag3 = |v: [f64; 3]| v[0].abs().max(v[1].abs()).max(v[2].abs());
    let l = mag3(p.as_array());
    let tau = TORUS_RELOC_WORK_FLOOR.max(8.0 * f64::EPSILON * l);
    let rank_eps = cad_primitives::MIN_FEATURE_SIZE;
    let cross = |a: [f64; 3], b: [f64; 3]| {
        [
            a[1] * b[2] - a[2] * b[1],
            a[2] * b[0] - a[0] * b[2],
            a[0] * b[1] - a[1] * b[0],
        ]
    };
    // KV16: the Newton system pairs each residual with the UNIT surface
    // normal, so every residual must be the TRUE signed distance along it.
    // `surface_value_and_normal`'s cone arm returns the radial-deviation
    // form `l − |h|·tanα` = distance × sec α — fine for its band-audit
    // consumers (conservative) but an overshooting Newton step here: at
    // half-angle 60° (sec α ≈ 2) the iteration bounces without converging
    // (the R0017 v47 prism-edge × 60°-band pierce). Rescale the cone
    // residual to the true distance `l·cosα − |h|·sinα` (the kernel-v2
    // `pair_surface_residual_gradient` convention; plane/sphere/cylinder
    // residuals are already true distances along their unit gradients).
    let dist_and_normal = |s: Surface, x: [f64; 3]| -> Option<(f64, [f64; 3])> {
        let (f, n) = surface_value_and_normal(s, x)?;
        match s {
            Surface::Cone { half_angle, .. } => Some((f * half_angle.cos(), n)),
            _ => Some((f, n)),
        }
    };
    let mut x = p.as_array();
    for _ in 0..=MAX_ITERS {
        let (f0, n0) = dist_and_normal(s0, x)?;
        let (f1, n1) = dist_and_normal(s1, x)?;
        let (f2, n2) = dist_and_normal(s2, x)?;
        let c12 = cross(n1, n2);
        let det = n0[0] * c12[0] + n0[1] * c12[1] + n0[2] * c12[2];
        if det.abs() <= rank_eps {
            return None; // coplanar normals → ill-posed junction
        }
        if f0.abs() <= tau && f1.abs() <= tau && f2.abs() <= tau {
            return Some(Point3::new(x[0], x[1], x[2]));
        }
        // dx = −J⁻¹ f = −(1/det)[ f0·(n1×n2) + f1·(n2×n0) + f2·(n0×n1) ]
        let c20 = cross(n2, n0);
        let c01 = cross(n0, n1);
        for i in 0..3 {
            x[i] -= (f0 * c12[i] + f1 * c20[i] + f2 * c01[i]) / det;
        }
    }
    None
}

/// PR-YR10 (spec §4.4): per-component residual `(|axial|, |radial − r|)` of `pt`
/// to an exact circle. This is the spec §4.5 classification residual the Stage-4
/// relocation drives ≤ `TAU_MODEL`. The legacy combined form
/// `ρ = max(|axial|, |radial − r|)` (PR-YR10) is recovered as `axial.max(radial_dev)`.
///
/// PR-YR19: the Stage-4 circle relocation guard splits the residual so the
/// in-plane RADIAL band can be the propagated `(R/r_c)·d_ε` for a sphere section
/// circle while the AXIAL band stays `d_ε` (spec §2/§4 Site 2, N11). Non-sphere
/// callers fold it back to the combined max, so behavior there is byte-identical.
pub(crate) fn circle_residual_split(
    pt: Point3,
    center: Point3,
    normal: Vector3,
    radius: f64,
) -> (f64, f64) {
    let n = normalize3(normal.as_array());
    let c = center.as_array();
    let x = pt.as_array();
    let w = [x[0] - c[0], x[1] - c[1], x[2] - c[2]];
    let axial = (w[0] * n[0] + w[1] * n[1] + w[2] * n[2]).abs();
    let radial_vec = [
        w[0] - (w[0] * n[0] + w[1] * n[1] + w[2] * n[2]) * n[0],
        w[1] - (w[0] * n[0] + w[1] * n[1] + w[2] * n[2]) * n[1],
        w[2] - (w[0] * n[0] + w[1] * n[1] + w[2] * n[2]) * n[2],
    ];
    let radial = (radial_vec[0] * radial_vec[0]
        + radial_vec[1] * radial_vec[1]
        + radial_vec[2] * radial_vec[2])
        .sqrt();
    (axial, (radial - radius).abs())
}

/// M8 disc∩disc CROSSING: the exact intersection of two COPLANAR circles
/// `(c_a, n_a, r_a)` and `(c_b, n_b, r_b)`, picking the root nearest `near`.
/// Two coplanar circles meet in ≤ 2 points (the lens corners); closed-form 2D
/// in their shared plane. Returns `None` (→ a loud Stage-4 STOP) when the
/// circles are NOT coplanar (parallel normals + co-planar centers), are
/// concentric, or do not actually cross — none of which is a disc∩disc lens
/// corner, so we never guess.
pub(crate) fn coplanar_circle_circle_intersection(
    c_a: Point3,
    n_a: Vector3,
    r_a: f64,
    c_b: Point3,
    n_b: Vector3,
    r_b: f64,
    near: Point3,
) -> Option<Point3> {
    let n = normalize3(n_a.as_array());
    let nb = normalize3(n_b.as_array());
    let ca = c_a.as_array();
    let cb = c_b.as_array();
    // Coplanarity: normals parallel AND c_b in c_a's plane.
    let cross_n = [
        n[1] * nb[2] - n[2] * nb[1],
        n[2] * nb[0] - n[0] * nb[2],
        n[0] * nb[1] - n[1] * nb[0],
    ];
    let cross_mag =
        (cross_n[0] * cross_n[0] + cross_n[1] * cross_n[1] + cross_n[2] * cross_n[2]).sqrt();
    let u = [cb[0] - ca[0], cb[1] - ca[1], cb[2] - ca[2]];
    let off_plane = (u[0] * n[0] + u[1] * n[1] + u[2] * n[2]).abs();
    if cross_mag > cad_primitives::MIN_FEATURE_SIZE || off_plane > cad_primitives::MIN_FEATURE_SIZE
    {
        return None; // not coplanar → not a disc∩disc lens corner
    }
    let d = (u[0] * u[0] + u[1] * u[1] + u[2] * u[2]).sqrt();
    if d < cad_primitives::MIN_FEATURE_SIZE {
        return None; // concentric
    }
    let uh = [u[0] / d, u[1] / d, u[2] / d];
    // Distance from c_a to the radical line along û.
    let a = (d * d + r_a * r_a - r_b * r_b) / (2.0 * d);
    let h2 = r_a * r_a - a * a;
    if h2 <= 0.0 {
        return None; // circles do not cross (tangent/disjoint)
    }
    let h = h2.sqrt();
    let m = [ca[0] + a * uh[0], ca[1] + a * uh[1], ca[2] + a * uh[2]];
    // In-plane perpendicular: v̂ = n × û (unit by construction).
    let vh = [
        n[1] * uh[2] - n[2] * uh[1],
        n[2] * uh[0] - n[0] * uh[2],
        n[0] * uh[1] - n[1] * uh[0],
    ];
    let q = near.as_array();
    let mut best: Option<(Point3, f64)> = None;
    for s in [h, -h] {
        let x = [m[0] + s * vh[0], m[1] + s * vh[1], m[2] + s * vh[2]];
        let dd = (x[0] - q[0]).powi(2) + (x[1] - q[1]).powi(2) + (x[2] - q[2]).powi(2);
        if best.map(|(_, b)| dd < b).unwrap_or(true) {
            best = Some((Point3::new(x[0], x[1], x[2]), dd));
        }
    }
    best.map(|(p, _)| p)
}

/// PR-F3 (KV6b-F3): the exact ruling LINE for one plane∥axis × cylinder
/// intersection edge (`ssi` plane_cylinder C3a/C3b), carried per-vertex like
/// `vert_circle`. The stored `Curve::LineSegment` carries no analytic data, so
/// Stage 4 recomputes the line from the edge's incidence (cylinder + plane)
/// and re-selects the unique candidate through both endpoints — the SAME
/// matching rule Stage 3 used, so selection here cannot disagree with it.
#[derive(Clone, Copy)]
pub(crate) struct LineReloc {
    pub(crate) point: Point3,
    pub(crate) dir: Vector3,
    /// PR-F3b/PR-KV9: the ABSOLUTE residual budget for the line-distance
    /// metric — the owner chord band(s) propagated through the
    /// [`line_band_amplification`] metric conversion. Cylinder×plane:
    /// `amp · d_ε(cylinder owner)`; cylinder×cylinder: `amp · (d_ε(A) +
    /// d_ε(B))` (both meshes' chords contribute to the crossing).
    pub(crate) band_budget: f64,
}

/// Per-vertex circle assignment `(center, normal, radius, source_sphere_radius)`
/// — the `vert_circle` value tuple, shared by the PR-F3 line+circle junction map.
pub(crate) type CircleAssign = (Point3, Vector3, f64, Option<f64>);

/// PR-F3b: band amplification for a ruling-LINE membership / residual test on
/// a `cylinder × plane` pair. A mesh point on a Stage-1 facet chord is off the
/// cylinder RADIALLY by `ρ ≤ d_ε` (the Stage-1 contract), but its
/// perpendicular distance to the C3a intersection line — measured IN the
/// cutting plane — is `ρ·r/√(r² − d²)` where `d` is the axis-to-plane
/// distance: the radial deficit divides by the cosine between the radial
/// direction at the line and the in-plane direction. This is the line analog
/// of the PR-YR19 sphere section-circle `(R/r_c)` propagation (see
/// `chord_band_propagates_into_section_metric`): a DERIVED metric conversion,
/// not tolerance widening. Surface-normal backstops (the on-both-surfaces
/// gate) stay UNSCALED. Returns `None` for non-cylinder/plane pairs and for
/// near-tangent planes (`d → r`, where the factor diverges — such a pair
/// fails loud upstream rather than matching everything).
pub(crate) fn line_band_amplification(surf0: Surface, surf1: Surface) -> Option<f64> {
    // PR-KV9: PARALLEL cylinder × cylinder ruling lines (ssi cyl∥cyl secant/
    // tangent). The same gradient-geometry derivation applies with BOTH
    // constraint gradients radial: at a crossing point X of the two
    // cross-section circles (radii r1, r2, inter-axis distance d), the angle
    // α between the radial directions r̂ᵢ = (X − cᵢ)/rᵢ follows the law of
    // cosines in the triangle (c1, c2, X):
    //   cos α = (r1² + r2² − d²) / (2·r1·r2)
    // (symmetric for both crossing lines), and the membership band is
    // 1/sin α — the general 1/‖ĝ1 × ĝ2‖ form the cylinder×plane case is a
    // special case of (there ĝ2 = the plane normal and 1/sin α reduces to
    // r/√(r² − d²)). Near-tangent (sin α → 0) returns None: no finite
    // propagated band, the pair fails loud upstream.
    if let (
        Surface::Cylinder {
            axis_point: p1,
            axis_dir: d1,
            radius: r1,
        },
        Surface::Cylinder {
            axis_point: p2,
            axis_dir: d2,
            radius: r2,
        },
    ) = (surf0, surf1)
    {
        let u1 = normalize3(d1.as_array());
        let u2 = normalize3(d2.as_array());
        let cx = [
            u1[1] * u2[2] - u1[2] * u2[1],
            u1[2] * u2[0] - u1[0] * u2[2],
            u1[0] * u2[1] - u1[1] * u2[0],
        ];
        let cross_norm = (cx[0] * cx[0] + cx[1] * cx[1] + cx[2] * cx[2]).sqrt();
        if cross_norm > cad_primitives::TAU_MODEL || r1 <= 0.0 || r2 <= 0.0 {
            return None; // non-parallel axes: no ruling lines from this pair
        }
        let q1a = p1.as_array();
        let q2a = p2.as_array();
        let rel = [q2a[0] - q1a[0], q2a[1] - q1a[1], q2a[2] - q1a[2]];
        let along = rel[0] * u1[0] + rel[1] * u1[1] + rel[2] * u1[2];
        let perp = [
            rel[0] - along * u1[0],
            rel[1] - along * u1[1],
            rel[2] - along * u1[2],
        ];
        let d = (perp[0] * perp[0] + perp[1] * perp[1] + perp[2] * perp[2]).sqrt();
        let cos_a = ((r1 * r1 + r2 * r2 - d * d) / (2.0 * r1 * r2)).clamp(-1.0, 1.0);
        let sin_a = (1.0 - cos_a * cos_a).max(0.0).sqrt();
        if sin_a <= AMP_TANGENCY_MIN_SIN_CIRCLE_PAIR {
            return None; // tangent-grade crossing: band diverges
        }
        return Some(1.0 / sin_a);
    }
    let (cyl, pl) = match (surf0, surf1) {
        (c @ Surface::Cylinder { .. }, p @ Surface::Plane { .. }) => (c, p),
        (p @ Surface::Plane { .. }, c @ Surface::Cylinder { .. }) => (c, p),
        _ => return None,
    };
    let (
        Surface::Cylinder {
            axis_point, radius, ..
        },
        Surface::Plane { normal, d },
    ) = (cyl, pl)
    else {
        return None;
    };
    let n = normal.as_array();
    let nn = (n[0] * n[0] + n[1] * n[1] + n[2] * n[2]).sqrt();
    // NaN-safe: a non-finite `nn` fails the `>=` and returns None.
    if nn < cad_primitives::MIN_FEATURE_SIZE || !nn.is_finite() || radius <= 0.0 {
        return None;
    }
    let p = axis_point.as_array();
    let dist = ((n[0] * p[0] + n[1] * p[1] + n[2] * p[2] + d) / nn).abs();
    let half_sep_sq = radius * radius - dist * dist;
    if half_sep_sq <= (cad_primitives::MIN_FEATURE_SIZE * radius).powi(2) {
        return None; // near-tangent: no finite propagated band.
    }
    Some(radius / half_sep_sq.sqrt())
}

/// PR-KV9: per-point membership amplification for a curve on TWO cylinders
/// — `1/‖ĝ₁×ĝ₂‖` with `ĝᵢ` the unit radial gradients of the two cylinders
/// at `x`. The constraint-band intersection at angle α has diameter
/// `(ρ₁+ρ₂)/sin α`; near surface tangency (the Steinmetz ellipse crossing
/// points, where both radials align) the band legitimately diverges —
/// `None` there, and the caller falls back to the tangent-direction
/// discriminator.
pub(crate) fn cyl_cyl_point_amplification(
    x: Point3,
    c1: (Point3, Vector3),
    c2: (Point3, Vector3),
) -> Option<f64> {
    let grad = |(ap, ad): (Point3, Vector3)| -> Option<[f64; 3]> {
        let a = normalize3(ad.as_array());
        let p = ap.as_array();
        let w = [x.x() - p[0], x.y() - p[1], x.z() - p[2]];
        let h = w[0] * a[0] + w[1] * a[1] + w[2] * a[2];
        let r = [w[0] - h * a[0], w[1] - h * a[1], w[2] - h * a[2]];
        let rl = (r[0] * r[0] + r[1] * r[1] + r[2] * r[2]).sqrt();
        if rl < cad_primitives::MIN_FEATURE_SIZE {
            return None;
        }
        Some([r[0] / rl, r[1] / rl, r[2] / rl])
    };
    let g1 = grad(c1)?;
    let g2 = grad(c2)?;
    let cx = [
        g1[1] * g2[2] - g1[2] * g2[1],
        g1[2] * g2[0] - g1[0] * g2[2],
        g1[0] * g2[1] - g1[1] * g2[0],
    ];
    let sin_a = (cx[0] * cx[0] + cx[1] * cx[1] + cx[2] * cx[2]).sqrt();
    if sin_a < AMP_TANGENCY_MIN_SIN_CYL_CYL {
        return None; // tangency-grade: no finite band
    }
    Some(1.0 / sin_a)
}

/// N38 follow-up (task #161): per-point membership amplification for a
/// transversal intersection curve of ANY two surfaces — `1/sin α` with α the
/// angle between the two surfaces' UNIT GRADIENTS (`surface_normal_at`) at `x`.
///
/// A mesh point on a Stage-1 facet chord is off each surface by ρ ≤ its d_ε
/// (the chord sagitta). Its perpendicular distance to the intersection CURVE
/// (which lies on BOTH surfaces) is the chord band measured in the metric of
/// the band INTERSECTION of the two surfaces: two half-spaces of half-width ρ
/// meeting at gradient-angle α have an intersection slab of half-width
/// `ρ/sin α`. So the curve-membership band is `d_ε/sin α` — the general form
/// the `cyl_cyl_point_amplification` (two RADIAL gradients) and
/// `line_band_amplification` (`r/√(r²−d²) = 1/sin α` for cyl∩plane) are both
/// special cases of. For a CONE∩PLANE conic the cone gradient tilts ⟂ the
/// generator (`surface_normal_at`'s cone arm), so a grazing plane-∥-axis
/// hyperbola — small α — legitimately needs a larger band than the raw cone
/// chord sagitta; the conic membership arms previously omitted this factor
/// (deviation N39).
///
/// `None` at a gradient singularity (cone apex, cylinder/sphere axis) or at
/// tangency (`sin α → 0`, the band diverges), where the caller keeps the flat
/// band and the tangent-direction discriminator decides — the SAFE fallback,
/// never a silent everything-matches.
pub(crate) fn surface_pair_point_amplification(
    x: Point3,
    surf0: Surface,
    surf1: Surface,
) -> Option<f64> {
    let g0 = surface_normal_at(surf0, x)?;
    let g1 = surface_normal_at(surf1, x)?;
    let cx = [
        g0[1] * g1[2] - g0[2] * g1[1],
        g0[2] * g1[0] - g0[0] * g1[2],
        g0[0] * g1[1] - g0[1] * g1[0],
    ];
    let sin_a = (cx[0] * cx[0] + cx[1] * cx[1] + cx[2] * cx[2]).sqrt();
    if sin_a < AMP_TANGENCY_MIN_SIN_CYL_CYL {
        return None; // tangency-grade: no finite band
    }
    Some(1.0 / sin_a)
}

/// PR-KV9: unit tangent of an ssi candidate curve at (the projection of)
/// `x` — the tangent-direction discriminator for multi-matched candidates.
/// `None` for curve types without a closed-form tangent here.
pub(crate) fn curve_tangent_at(curve: &ssi_rs::SsiCurve, x: Point3) -> Option<[f64; 3]> {
    match curve {
        ssi_rs::SsiCurve::Line { dir, .. } => Some(normalize3(dir.as_array())),
        ssi_rs::SsiCurve::Ellipse {
            center,
            normal,
            major_axis,
            major_radius,
            minor_radius,
        } => {
            let n = normalize3(normal.as_array());
            let m = normalize3(major_axis.as_array());
            let w = [
                n[1] * m[2] - n[2] * m[1],
                n[2] * m[0] - n[0] * m[2],
                n[0] * m[1] - n[1] * m[0],
            ];
            let c = center.as_array();
            let dxv = [x.x() - c[0], x.y() - c[1], x.z() - c[2]];
            let u = (dxv[0] * m[0] + dxv[1] * m[1] + dxv[2] * m[2]) / major_radius;
            let v = (dxv[0] * w[0] + dxv[1] * w[1] + dxv[2] * w[2]) / minor_radius;
            let t = v.atan2(u);
            // dP/dt = −a·sin t·m̂ + b·cos t·ŵ
            let (st, ct) = t.sin_cos();
            let tan = [
                -major_radius * st * m[0] + minor_radius * ct * w[0],
                -major_radius * st * m[1] + minor_radius * ct * w[1],
                -major_radius * st * m[2] + minor_radius * ct * w[2],
            ];
            Some(normalize3(tan))
        }
        // M5 (Y3): the tangent of a transversal surface-pair curve at `x` is
        // `n̂_a × n̂_b` (perpendicular to both surface normals). Parallel
        // normals (tangency) → no finite tangent → `None`, so the candidate
        // stays non-tie-breakable and the loud `AmbiguousCurve` stop stands.
        ssi_rs::SsiCurve::SurfacePair { a, b } => {
            let quadric_normal = |q: &ssi_rs::QuadricSurface| -> Option<[f64; 3]> {
                match q {
                    ssi_rs::QuadricSurface::Cylinder {
                        axis_point,
                        axis_dir,
                        ..
                    } => {
                        let ap = axis_point.as_array();
                        let ad = normalize3(axis_dir.as_array());
                        let w = [x.x() - ap[0], x.y() - ap[1], x.z() - ap[2]];
                        let along = w[0] * ad[0] + w[1] * ad[1] + w[2] * ad[2];
                        let radial = [
                            w[0] - along * ad[0],
                            w[1] - along * ad[1],
                            w[2] - along * ad[2],
                        ];
                        let rl =
                            (radial[0] * radial[0] + radial[1] * radial[1] + radial[2] * radial[2])
                                .sqrt();
                        (rl > cad_primitives::MIN_FEATURE_SIZE)
                            .then(|| [radial[0] / rl, radial[1] / rl, radial[2] / rl])
                    }
                    // Cone normal: cosα·r̂ − sign(h)·sinα·â (the unit gradient of
                    // `radial − |h|·tanα`; matches `surface_value_and_normal`).
                    ssi_rs::QuadricSurface::Cone {
                        apex,
                        axis_dir,
                        half_angle,
                    } => {
                        let ap = apex.as_array();
                        let ad = normalize3(axis_dir.as_array());
                        let w = [x.x() - ap[0], x.y() - ap[1], x.z() - ap[2]];
                        let h = w[0] * ad[0] + w[1] * ad[1] + w[2] * ad[2];
                        let radial = [w[0] - h * ad[0], w[1] - h * ad[1], w[2] - h * ad[2]];
                        let rl =
                            (radial[0] * radial[0] + radial[1] * radial[1] + radial[2] * radial[2])
                                .sqrt();
                        (rl > cad_primitives::MIN_FEATURE_SIZE).then(|| {
                            let sgn = if h >= 0.0 { 1.0 } else { -1.0 };
                            let (sa, ca) = half_angle.sin_cos();
                            normalize3([
                                ca * radial[0] / rl - sgn * sa * ad[0],
                                ca * radial[1] / rl - sgn * sa * ad[1],
                                ca * radial[2] / rl - sgn * sa * ad[2],
                            ])
                        })
                    }
                    _ => None,
                }
            };
            let (na, nb) = (quadric_normal(a)?, quadric_normal(b)?);
            let cx = [
                na[1] * nb[2] - na[2] * nb[1],
                na[2] * nb[0] - na[0] * nb[2],
                na[0] * nb[1] - na[1] * nb[0],
            ];
            let cl = (cx[0] * cx[0] + cx[1] * cx[1] + cx[2] * cx[2]).sqrt();
            (cl > 1e-3).then(|| normalize3(cx))
        }
        _ => None,
    }
}

/// Perpendicular distance of `p` to the line `(point, dir)` (`dir` need not be
/// unit). PR-F3 line membership / residual metric.
pub(crate) fn line_perp_distance(p: Point3, point: Point3, dir: Vector3) -> f64 {
    let d = normalize3(dir.as_array());
    let pt = point.as_array();
    let x = p.as_array();
    let w = [x[0] - pt[0], x[1] - pt[1], x[2] - pt[2]];
    let along = w[0] * d[0] + w[1] * d[1] + w[2] * d[2];
    let perp = [
        w[0] - along * d[0],
        w[1] - along * d[1],
        w[2] - along * d[2],
    ];
    (perp[0] * perp[0] + perp[1] * perp[1] + perp[2] * perp[2]).sqrt()
}

/// R0072: position tie-break among near-coincident PARALLEL line candidates.
///
/// A near-tangent `plane ∩ cylinder` secant yields two near-coincident parallel
/// generators; both pass the chord-band containment test, so a `matched == 1`
/// selector deadlocks (`AmbiguousCurve`). The tangent-direction discriminator
/// cannot help — parallel lines share a direction. But the mesh edge lies on
/// exactly ONE generator, which is nearer to BOTH endpoints.
///
/// Given matched line candidates `(point, unit dir)` and the edge endpoints,
/// return the index whose endpoint-distance interval `[min, max]` lies strictly
/// below every other candidate's (`hi_w < lo_j ∀ j≠w`): the winner's worst
/// endpoint still beats every rival's best, so the endpoints unambiguously lie
/// on it. Margin-free and scale-free. Returns `None` (caller keeps its loud
/// stop) when there are < 2 candidates, the candidates are NOT mutually parallel
/// (a transversal multi-match the tangent pass owns), or the intervals overlap
/// (generators merged below mesh resolution — true-tangency territory). P9: a
/// proximity tie-break on geometry already gate-verified on both surfaces, never
/// a band widening; genuine ambiguity is preserved. Spec
/// `specs/yr_r0072_parallel_line_position_tiebreak.md`.
pub(crate) fn select_disjoint_parallel_line(
    cands: &[(Point3, Vector3)],
    p_s: Point3,
    p_e: Point3,
) -> Option<usize> {
    if cands.len() < 2 {
        return None;
    }
    // Every candidate must be mutually parallel — the case the tangent
    // discriminator structurally cannot resolve.
    let dirs: Vec<[f64; 3]> = cands.iter().map(|c| normalize3(c.1.as_array())).collect();
    for w in dirs.windows(2) {
        let (a, b) = (w[0], w[1]);
        let c = [
            a[1] * b[2] - a[2] * b[1],
            a[2] * b[0] - a[0] * b[2],
            a[0] * b[1] - a[1] * b[0],
        ];
        if (c[0] * c[0] + c[1] * c[1] + c[2] * c[2]).sqrt() >= cad_primitives::TAU_MODEL {
            return None;
        }
    }
    let ivs: Vec<(f64, f64)> = cands
        .iter()
        .map(|c| {
            let ds = line_perp_distance(p_s, c.0, c.1);
            let de = line_perp_distance(p_e, c.0, c.1);
            (ds.min(de), ds.max(de))
        })
        .collect();
    let wk = (0..ivs.len()).min_by(|&i, &j| {
        ivs[i]
            .1
            .partial_cmp(&ivs[j].1)
            .unwrap_or(std::cmp::Ordering::Equal)
    })?;
    let hi_w = ivs[wk].1;
    if (0..ivs.len()).all(|k| k == wk || hi_w < ivs[k].0) {
        Some(wk)
    } else {
        None
    }
}

/// PR-YR11 (spec §1): the true cylinder + cutting plane for one oblique ellipse
/// edge, carried per-vertex (analogous to `vert_circle`'s `(center, normal,
/// radius)`). The cylinder fields are `Surface::Cylinder`; the plane fields are
/// the cutting `Surface::Plane` (`n·x + d = 0`); the ellipse fields are the
/// stored `Curve::Ellipse` (for the relocation parameter `t` + the round-trip).
#[derive(Clone, Copy)]
pub(crate) struct EllipseReloc {
    pub(crate) axis_point: Point3,
    pub(crate) axis_dir: Vector3,
    pub(crate) radius: f64,
    pub(crate) plane_n: Vector3,
    pub(crate) plane_d: f64,
    pub(crate) center: Point3,
    pub(crate) normal: Vector3,
    pub(crate) major_axis: Vector3,
    pub(crate) major_radius: f64,
    pub(crate) minor_radius: f64,
    /// PR-KV9: `Some((other_axis_point, other_axis_dir, combined_band))`
    /// for a cylinder×CYLINDER section — the residual gate then uses the
    /// per-point gradient amplification against the combined band instead
    /// of the global `d_ε` (the metric conversion diverges at surface
    /// tangency, where the Stage-3 surface-membership gate remains the
    /// backstop). `None` keeps the cylinder×plane path byte-identical.
    pub(crate) second_cyl: Option<(Point3, Vector3, f64)>,
}

/// PR-YR11 (spec §2): relocate `p` onto the exact ellipse via the CYLINDER
/// parameterization (Yang §4.3.2) — closed-form, NO quartic. The relocated point
/// lies on BOTH the cylinder (radius `r` about its axis) AND the cutting plane
/// (`n·x + d = 0`), hence exactly on `plane ∩ cylinder` = the ellipse. Returns
/// `(proj, t)` where `t` is the ellipse parameter in the shared
/// [`ellipse_point`] frame, so a relocated vertex tagged `BRepEdge { edge, t }`
/// round-trips exactly.
///
/// LOUD STOPs (P9/P10), never a silent snap / divide-by-~0:
/// - `Err(OnAxis)` when the radial component `ρ < MIN_FEATURE_SIZE`.
/// - `Err(LocalRefinementRequired)` for the out-of-scope axis-parallel section
///   `|n·â| < MIN_FEATURE_SIZE` (the linear axial solve is degenerate there).
pub(crate) fn project_onto_ellipse_via_cylinder(
    p: Point3,
    er: &EllipseReloc,
) -> Result<(Point3, f64), Stage4InvalidReason> {
    let q = er.axis_point.as_array();
    let a_hat = normalize3(er.axis_dir.as_array());
    let n = normalize3(er.plane_n.as_array());
    // The plane offset `d` must be expressed for the UNIT normal `n`. The stored
    // `Surface::Plane` normals in the corpus are already unit, but normalize the
    // offset defensively against the same scale used for `n`.
    let n_raw = er.plane_n.as_array();
    let n_len = (n_raw[0] * n_raw[0] + n_raw[1] * n_raw[1] + n_raw[2] * n_raw[2]).sqrt();
    let d = if n_len < cad_primitives::TAU_WORK {
        er.plane_d
    } else {
        er.plane_d / n_len
    };
    let r = er.radius;
    let x = p.as_array();

    let w = [x[0] - q[0], x[1] - q[1], x[2] - q[2]];
    let along = w[0] * a_hat[0] + w[1] * a_hat[1] + w[2] * a_hat[2];
    let radial = [
        w[0] - along * a_hat[0],
        w[1] - along * a_hat[1],
        w[2] - along * a_hat[2],
    ];
    let rho = (radial[0] * radial[0] + radial[1] * radial[1] + radial[2] * radial[2]).sqrt();
    if rho < cad_primitives::MIN_FEATURE_SIZE {
        return Err(Stage4InvalidReason::OnAxis);
    }
    let rdir = [radial[0] / rho, radial[1] / rho, radial[2] / rho];

    let n_dot_a = n[0] * a_hat[0] + n[1] * a_hat[1] + n[2] * a_hat[2];
    if n_dot_a.abs() < cad_primitives::MIN_FEATURE_SIZE {
        // Axis-parallel / degenerate-line section: out of scope. Loud STOP rather
        // than dividing by ~0 (spec §6).
        return Err(Stage4InvalidReason::LocalRefinementRequired);
    }
    let n_dot_q = n[0] * q[0] + n[1] * q[1] + n[2] * q[2];
    let n_dot_rdir = n[0] * rdir[0] + n[1] * rdir[1] + n[2] * rdir[2];
    let s = -(n_dot_q + r * n_dot_rdir + d) / n_dot_a;

    let proj = Point3::new(
        q[0] + s * a_hat[0] + r * rdir[0],
        q[1] + s * a_hat[1] + r * rdir[1],
        q[2] + s * a_hat[2] + r * rdir[2],
    );
    let t = ellipse_param(
        proj,
        er.center,
        er.normal,
        er.major_axis,
        er.major_radius,
        er.minor_radius,
    );
    Ok((proj, t))
}

/// Task #145 mechanism 2 (spec `yang_453_mixed_cycle_conic_backtrack` §3b):
/// the IN-PLANE nearest point on the section ellipse. Unlike
/// [`project_onto_ellipse_via_cylinder`] — which preserves the vertex's
/// cylinder azimuth and amplifies any azimuthal offset by `1/(n·â)` ALONG a
/// near-tangent section — this projection is intrinsic to the ellipse and
/// stays a small multiple of the true off-curve residual everywhere (I6).
///
/// Method ([#1] Patrikalakis-Maekawa-Cho, point-to-curve projection, with the
/// standard first-quadrant symmetry reduction): drop `p` onto the section
/// plane, express it in the shared PR-YR11 ellipse frame as `(u, v)`, reduce
/// to `(|u|, |v|)` where the nearest parameter is UNIQUE on `[0, π/2]`, and
/// BISECT the distance stationarity
/// `f(t) = (a² − b²)·cos t·sin t − |u|·a·sin t + |v|·b·cos t`
/// (`f(0) = |v|·b ≥ 0`, `f(π/2) = −|u|·a ≤ 0` — a guaranteed bracket, so
/// convergence is unconditional; a plain Newton from the `atan2` seed
/// DIVERGES to a far stationary point on eccentric ellipses — the F0047
/// vertex-42 RED measurement). Signs map the solution back to the quadrant.
pub(crate) fn project_onto_ellipse_nearest(
    p: Point3,
    er: &EllipseReloc,
) -> Result<(Point3, f64), Stage4InvalidReason> {
    let n_raw = er.plane_n.as_array();
    let n_len = (n_raw[0] * n_raw[0] + n_raw[1] * n_raw[1] + n_raw[2] * n_raw[2]).sqrt();
    if n_len < cad_primitives::TAU_WORK {
        return Err(Stage4InvalidReason::LocalRefinementRequired);
    }
    let n = [n_raw[0] / n_len, n_raw[1] / n_len, n_raw[2] / n_len];
    let d = er.plane_d / n_len;
    let x = p.as_array();
    // Drop onto the section plane (move = the out-of-plane residual component).
    let h = n[0] * x[0] + n[1] * x[1] + n[2] * x[2] + d;
    let q = [x[0] - h * n[0], x[1] - h * n[1], x[2] - h * n[2]];
    // Shared PR-YR11 ellipse frame (byte-identical to `ellipse_param`).
    let maj = normalize3(er.major_axis.as_array());
    let mindir = crate::geom::ellipse_frame(er.normal, er.major_axis);
    let c = er.center.as_array();
    let w = [q[0] - c[0], q[1] - c[1], q[2] - c[2]];
    let u = w[0] * maj[0] + w[1] * maj[1] + w[2] * maj[2];
    let v = w[0] * mindir[0] + w[1] * mindir[1] + w[2] * mindir[2];
    let a = er.major_radius;
    let b = er.minor_radius;
    let (au, av) = (u.abs(), v.abs());
    let f = |t: f64| -> f64 {
        let (st, ct) = (t.sin(), t.cos());
        (a * a - b * b) * ct * st - au * a * st + av * b * ct
    };
    // Bisection on the guaranteed bracket [0, π/2]; ~80 halvings reach the
    // f64 resolution of the interval unconditionally (deterministic).
    // `f(0) = |v|·b ≥ 0` and `f(π/2) = −|u|·a ≤ 0` hold unconditionally, so
    // the bracket never needs a validity check; the axis-degenerate cases
    // (|u| = 0 or |v| = 0) converge to the correct endpoint or the interior
    // evolute root by the same iteration.
    let mut lo = 0.0_f64;
    let mut hi = std::f64::consts::FRAC_PI_2;
    for _ in 0..80 {
        let mid = 0.5 * (lo + hi);
        if f(mid) >= 0.0 {
            lo = mid;
        } else {
            hi = mid;
        }
    }
    let tq = 0.5 * (lo + hi);
    // Map the first-quadrant solution back through the sign symmetry (an
    // exactly-zero coordinate keeps the +quadrant representative).
    let ct = tq.cos().copysign(if u == 0.0 { 1.0 } else { u });
    let st = tq.sin().copysign(if v == 0.0 { 1.0 } else { v });
    let proj = Point3::new(
        c[0] + a * ct * maj[0] + b * st * mindir[0],
        c[1] + a * ct * maj[1] + b * st * mindir[1],
        c[2] + a * ct * maj[2] + b * st * mindir[2],
    );
    // Parameter in the `ellipse_param` convention (−π, π].
    let t_out = st.atan2(ct);
    Ok((proj, t_out))
}

/// Task #146 (spec `yang_stage4_circle_pp_line_junction`): the closed-form
/// LINE of two intersecting planes `n1·x + d1 = 0`, `n2·x + d2 = 0` —
/// `dir = n1 × n2`, `point = ((−d1)(n2 × dir) + (−d2)(dir × n1)) / |dir|²`.
/// `None` for (near-)parallel planes (`|n1 × n2|² < TAU_WORK²` — no unique
/// line; the caller STOPs loudly).
pub(crate) fn pp_line(n1: Vector3, d1: f64, n2: Vector3, d2: f64) -> Option<(Point3, Vector3)> {
    let a = normalize3(n1.as_array());
    let b = normalize3(n2.as_array());
    // Normalize the offsets with the same scale as the unit normals.
    let l1 = {
        let r = n1.as_array();
        (r[0] * r[0] + r[1] * r[1] + r[2] * r[2]).sqrt()
    };
    let l2 = {
        let r = n2.as_array();
        (r[0] * r[0] + r[1] * r[1] + r[2] * r[2]).sqrt()
    };
    if l1 < cad_primitives::TAU_WORK || l2 < cad_primitives::TAU_WORK {
        return None;
    }
    let (e1, e2) = (d1 / l1, d2 / l2);
    let dir = [
        a[1] * b[2] - a[2] * b[1],
        a[2] * b[0] - a[0] * b[2],
        a[0] * b[1] - a[1] * b[0],
    ];
    let dd = dir[0] * dir[0] + dir[1] * dir[1] + dir[2] * dir[2];
    if dd < cad_primitives::TAU_WORK * cad_primitives::TAU_WORK {
        return None;
    }
    let bxd = [
        b[1] * dir[2] - b[2] * dir[1],
        b[2] * dir[0] - b[0] * dir[2],
        b[0] * dir[1] - b[1] * dir[0],
    ];
    let dxa = [
        dir[1] * a[2] - dir[2] * a[1],
        dir[2] * a[0] - dir[0] * a[2],
        dir[0] * a[1] - dir[1] * a[0],
    ];
    let point = Point3::new(
        ((-e1) * bxd[0] + (-e2) * dxa[0]) / dd,
        ((-e1) * bxd[1] + (-e2) * dxa[1]) / dd,
        ((-e1) * bxd[2] + (-e2) * dxa[2]) / dd,
    );
    Some((point, Vector3::new(dir[0], dir[1], dir[2])))
}

/// Task #146 (spec `yang_stage4_circle_pp_line_junction` branches 4–5): the
/// junction of an exact line with a circle — the line∩SPHERE(C, r) quadratic
/// (exact for BOTH the in-plane and the transversal configuration; a junction
/// point on the circle is on that sphere regardless of the line's
/// inclination), then the circle-plane residual certifies circle membership.
/// Returns the root nearest `current` whose plane residual is within `band`;
/// `None` when the line misses the sphere or no root is on the plane
/// (branch 5 — the caller STOPs loudly).
pub(crate) fn pp_line_circle_junction(
    point: Point3,
    dir: Vector3,
    center: Point3,
    normal: Vector3,
    radius: f64,
    current: Point3,
    band: f64,
) -> Option<Point3> {
    let d = normalize3(dir.as_array());
    let n = normalize3(normal.as_array());
    let p = point.as_array();
    let c = center.as_array();
    let w = [p[0] - c[0], p[1] - c[1], p[2] - c[2]];
    let b_half = w[0] * d[0] + w[1] * d[1] + w[2] * d[2];
    let c0 = w[0] * w[0] + w[1] * w[1] + w[2] * w[2] - radius * radius;
    let disc = b_half * b_half - c0;
    if disc < 0.0 {
        return None;
    }
    let sq = disc.sqrt();
    let cur = current.as_array();
    let mut best: Option<(f64, Point3)> = None;
    for t in [-b_half - sq, -b_half + sq] {
        let j = [p[0] + t * d[0], p[1] + t * d[1], p[2] + t * d[2]];
        let plane_res = (n[0] * (j[0] - c[0]) + n[1] * (j[1] - c[1]) + n[2] * (j[2] - c[2])).abs();
        if plane_res > band {
            continue;
        }
        let dist2 = (j[0] - cur[0]).powi(2) + (j[1] - cur[1]).powi(2) + (j[2] - cur[2]).powi(2);
        if best.is_none_or(|(bd, _)| dist2 < bd) {
            best = Some((dist2, Point3::new(j[0], j[1], j[2])));
        }
    }
    best.map(|(_, j)| j)
}

/// PR-YR21 (spec §3.1/§3.2): per-vertex Ellipse relocation data for a
/// `cone ∩ plane` oblique section — the cone analog of [`EllipseReloc`]. Carries
/// the true cone (apex / axis / half-angle), the cutting plane (`plane_n` /
/// `plane_d`), the stored ellipse params (for the `ellipse_param` round-trip),
/// and the cone's OWN Stage-1 chord budget `cone_d_eps`
/// (`cone_chord_bound(height, half_angle)`) — NOT the rim-AABB `d_eps`, so a
/// tall-thin cone's residual is gated against the honest cone bound.
#[derive(Clone, Copy)]
pub(crate) struct ConeEllipseReloc {
    pub(crate) apex: Point3,
    pub(crate) axis_dir: Vector3,
    pub(crate) half_angle: f64,
    pub(crate) plane_n: Vector3,
    pub(crate) plane_d: f64,
    pub(crate) center: Point3,
    pub(crate) normal: Vector3,
    pub(crate) major_axis: Vector3,
    pub(crate) major_radius: f64,
    pub(crate) minor_radius: f64,
    pub(crate) cone_d_eps: f64,
}

/// PR-YR22: per-vertex Parabola relocation data for a `cone ∩ plane` θ=α
/// (generator-parallel) section — the parabola sibling of [`ConeEllipseReloc`].
/// Carries the true cone (`apex` / `cone_axis_dir` / `half_angle`), the cutting
/// plane (`plane_n` / `plane_d`), and the stored parabola params (`vertex` /
/// parabola `normal` / `para_axis_dir` — these differ from the cone's
/// `cone_axis_dir`/normal, hence the unambiguous names), plus the cone's OWN
/// Stage-1 chord budget `cone_d_eps`. `focal_length` is not stored: the
/// relocation tag `t` is the conjugate-axis coordinate (needs only `vertex` /
/// `normal` / `para_axis_dir`), and `eval_source` / `is_reversed` recover the
/// full parabola from the output edge's own `Curve::Parabola` fields.
#[derive(Clone, Copy)]
pub(crate) struct ConeParabolaReloc {
    pub(crate) apex: Point3,
    pub(crate) cone_axis_dir: Vector3,
    pub(crate) half_angle: f64,
    pub(crate) plane_n: Vector3,
    pub(crate) plane_d: f64,
    pub(crate) vertex: Point3,
    pub(crate) normal: Vector3,
    pub(crate) para_axis_dir: Vector3,
    pub(crate) cone_d_eps: f64,
}

/// PR-YR23: per-vertex Hyperbola relocation data for a `cone ∩ plane`
/// axis-parallel (HYPE) section — the hyperbola sibling of [`ConeParabolaReloc`].
/// Carries the true cone (`apex` / `cone_axis_dir` / `half_angle`), the cutting
/// plane (`plane_n` / `plane_d`), and the stored hyperbola params (`center` /
/// hyperbola `normal` / `major_axis` / `semi_transverse` / `semi_conjugate`) plus
/// the cone's OWN Stage-1 chord budget `cone_d_eps`. The relocation tag `t` is
/// `asinh(v / b)` where `v` is the conjugate-axis coordinate (`(proj − center)·
/// (normal × major_axis)`) and `b = semi_conjugate` (the `sinh` coordinate is the
/// bijective one). `eval_source` / `is_reversed` recover the full hyperbola from
/// the output edge's own `Curve::Hyperbola` fields. (`semi_transverse` is NOT
/// stored: the relocation tag `t = asinh(v / b)` needs only `semi_conjugate`,
/// mirroring how [`ConeParabolaReloc`] omits `focal_length`.)
#[derive(Clone, Copy)]
pub(crate) struct ConeHyperbolaReloc {
    pub(crate) apex: Point3,
    pub(crate) cone_axis_dir: Vector3,
    pub(crate) half_angle: f64,
    pub(crate) plane_n: Vector3,
    pub(crate) plane_d: f64,
    pub(crate) center: Point3,
    pub(crate) normal: Vector3,
    pub(crate) major_axis: Vector3,
    pub(crate) semi_conjugate: f64,
    pub(crate) cone_d_eps: f64,
}

/// PR-YR21 (spec §3.1): relocate `p` onto the exact `cone ∩ plane` ellipse via
/// the CONE GENERATOR parameterization (Yang §4.3.2) — closed-form, NO quartic.
/// The cone analog of [`project_onto_ellipse_via_cylinder`]. The relocated point
/// is built on the cone generator at `p`'s azimuth (so it lies on the cone) and
/// solved to satisfy `n·x + d = 0` (so it lies on the plane), hence exactly on
/// `plane ∩ cone` = the ellipse. Returns only the relocated 3D point
/// (type-agnostic; the caller does its own conic param inversion — YR22/YR23
/// reuse this unchanged for parabola/hyperbola).
///
/// LOUD STOPs (P9/P10), never a silent snap / divide-by-~0:
/// - `Err(OnAxis)` when the radial component `ρ < MIN_FEATURE_SIZE`.
/// - `Err(LocalRefinementRequired)` when the generator is parallel to the plane
///   (`|n·g| < MIN_FEATURE_SIZE` — the asymptotic / parabola-tail direction,
///   out of scope) or the solved generator parameter `s ≤ 0` (apex-coincident /
///   wrong-nappe).
pub(crate) fn project_onto_cone_section(
    p: Point3,
    apex: Point3,
    axis_dir: Vector3,
    half_angle: f64,
    plane_n: Vector3,
    plane_d: f64,
) -> Result<Point3, Stage4InvalidReason> {
    let ap = apex.as_array();
    let a_hat = normalize3(axis_dir.as_array());
    let n = normalize3(plane_n.as_array());
    // The plane offset `d` must be expressed for the UNIT normal `n`. Stored
    // `Surface::Plane` normals in the corpus are already unit, but normalize the
    // offset defensively against the same scale used for `n` (same pattern as
    // `project_onto_ellipse_via_cylinder`).
    let n_raw = plane_n.as_array();
    let n_len = (n_raw[0] * n_raw[0] + n_raw[1] * n_raw[1] + n_raw[2] * n_raw[2]).sqrt();
    let d = if n_len < cad_primitives::TAU_WORK {
        plane_d
    } else {
        plane_d / n_len
    };
    let x = p.as_array();

    let w = [x[0] - ap[0], x[1] - ap[1], x[2] - ap[2]];
    let axial = w[0] * a_hat[0] + w[1] * a_hat[1] + w[2] * a_hat[2];
    let radial = [
        w[0] - axial * a_hat[0],
        w[1] - axial * a_hat[1],
        w[2] - axial * a_hat[2],
    ];
    let rho = (radial[0] * radial[0] + radial[1] * radial[1] + radial[2] * radial[2]).sqrt();
    if rho < cad_primitives::MIN_FEATURE_SIZE {
        return Err(Stage4InvalidReason::OnAxis);
    }
    let rdir = [radial[0] / rho, radial[1] / rho, radial[2] / rho];

    // Nappe sign from the axial component; the upper nappe (axial ≥ 0) uses
    // `+cosα·â`, the lower (`axial < 0`) uses `−cosα·â`. ρ ≥ MIN_FEATURE_SIZE so
    // the point is genuinely off-axis; the `|n·g|` / `s ≤ 0` guards below catch
    // any apex-plane degeneracy.
    let nappe = if axial < 0.0 { -1.0 } else { 1.0 };
    let (ca, sa) = (half_angle.cos(), half_angle.sin());
    // Unit generator at `p`'s azimuth (|g| = 1 by construction).
    let g = [
        nappe * ca * a_hat[0] + sa * rdir[0],
        nappe * ca * a_hat[1] + sa * rdir[1],
        nappe * ca * a_hat[2] + sa * rdir[2],
    ];

    let n_dot_g = n[0] * g[0] + n[1] * g[1] + n[2] * g[2];
    if n_dot_g.abs() < cad_primitives::MIN_FEATURE_SIZE {
        // Generator parallel to the plane: the asymptotic / parabola-tail
        // direction — out of scope (spec §6). Loud STOP rather than dividing by
        // ~0.
        return Err(Stage4InvalidReason::LocalRefinementRequired);
    }
    let n_dot_apex = n[0] * ap[0] + n[1] * ap[1] + n[2] * ap[2];
    let s = -(n_dot_apex + d) / n_dot_g;
    if s <= 0.0 {
        // Apex-coincident / wrong-nappe: the generator pierces the plane at or
        // behind the apex — out of scope. Loud STOP.
        return Err(Stage4InvalidReason::LocalRefinementRequired);
    }
    Ok(Point3::new(
        ap[0] + s * g[0],
        ap[1] + s * g[1],
        ap[2] + s * g[2],
    ))
}

/// PR-YR21 (spec §3.3): derive a cone's Stage-1 chord budget
/// `cone_chord_bound(height, half_angle)` from the cone OWNER's rim
/// `Curve::Circle`, using the SAME height derivation as `cone_chord_tol_for_owner`
/// / `tol_for`: `height = |(rim_center − apex)·â|`. A cone owner with no rim
/// Circle is a producer fault → `None` (the caller raises a loud STOP; NEVER a
/// `TAU_WORK` default for a curved relocation — P10).
pub(crate) fn cone_chord_budget_from_owner(
    apex: Point3,
    axis_dir: Vector3,
    half_angle: f64,
    owner: &BRep,
) -> Option<f64> {
    let au = normalize3(axis_dir.as_array());
    let ap = apex.as_array();
    for f in owner.faces() {
        if let Surface::Cone { .. } = f.surface {
            for &e_idx in &f.outer_loop {
                if let Curve::Circle { center, .. } = owner.edges()[e_idx as usize].curve {
                    let c = center.as_array();
                    let height =
                        ((c[0] - ap[0]) * au[0] + (c[1] - ap[1]) * au[1] + (c[2] - ap[2]) * au[2])
                            .abs();
                    return Some(cone_chord_bound(height, half_angle));
                }
            }
        }
    }
    None
}

/// PR-YR21 (spec §3.1/§4): the on-both-surfaces residual `max(cone radial,
/// plane)` of `pt` to an exact `cone ∩ plane` ellipse, recomputed from the
/// stored cone/plane. The cone analog of [`ellipse_residual`]. Cone radial
/// residual `|ρ − |axial|·tanα|` + plane residual `|n·x + d|` (plane offset
/// normalized to the unit normal).
pub(crate) fn cone_ellipse_residual(pt: Point3, cer: &ConeEllipseReloc) -> f64 {
    cone_plane_residual(
        pt,
        cer.apex,
        cer.axis_dir,
        cer.half_angle,
        cer.plane_n,
        cer.plane_d,
    )
}

/// PR-YR22: the on-both-surfaces residual `max(cone radial, plane)` of `pt` to an
/// exact `cone ∩ plane` section, recomputed from the stored cone/plane. Cone
/// radial residual `|ρ − |axial|·tanα|` + plane residual `|n·x + d|` (plane
/// offset normalized to the unit normal). Shared by [`cone_ellipse_residual`]
/// (ellipse) and the Stage-4 parabola loop — the conic TYPE does not change this
/// cone+plane residual (it only depends on the two surfaces, not the section
/// shape), so both call it (spec §3.1/§4).
pub(crate) fn cone_plane_residual(
    pt: Point3,
    apex: Point3,
    axis_dir: Vector3,
    half_angle: f64,
    plane_n: Vector3,
    plane_d: f64,
) -> f64 {
    let ap = apex.as_array();
    let a_hat = normalize3(axis_dir.as_array());
    let x = pt.as_array();
    let w = [x[0] - ap[0], x[1] - ap[1], x[2] - ap[2]];
    let axial = w[0] * a_hat[0] + w[1] * a_hat[1] + w[2] * a_hat[2];
    let radial = [
        w[0] - axial * a_hat[0],
        w[1] - axial * a_hat[1],
        w[2] - axial * a_hat[2],
    ];
    let rho = (radial[0] * radial[0] + radial[1] * radial[1] + radial[2] * radial[2]).sqrt();
    let cone_res = (rho - axial.abs() * half_angle.tan()).abs();

    let n_raw = plane_n.as_array();
    let n_len = (n_raw[0] * n_raw[0] + n_raw[1] * n_raw[1] + n_raw[2] * n_raw[2]).sqrt();
    let (n, d) = if n_len < cad_primitives::TAU_WORK {
        (n_raw, plane_d)
    } else {
        (
            [n_raw[0] / n_len, n_raw[1] / n_len, n_raw[2] / n_len],
            plane_d / n_len,
        )
    };
    let plane_res = (x[0] * n[0] + x[1] * n[1] + x[2] * n[2] + d).abs();
    cone_res.max(plane_res)
}

/// PR-YR11 (spec §4): the on-both-surfaces residual `max(|dist(x,axis)−r|,
/// |n·x+d|)` of `pt` to an exact oblique ellipse (cylinder ∩ plane). Matches the
/// RED Oracle-1 contract. The plane offset is normalized to the unit normal.
pub(crate) fn ellipse_residual(pt: Point3, er: &EllipseReloc) -> f64 {
    let q = er.axis_point.as_array();
    let a_hat = normalize3(er.axis_dir.as_array());
    let x = pt.as_array();
    let w = [x[0] - q[0], x[1] - q[1], x[2] - q[2]];
    let along = w[0] * a_hat[0] + w[1] * a_hat[1] + w[2] * a_hat[2];
    let radial = [
        w[0] - along * a_hat[0],
        w[1] - along * a_hat[1],
        w[2] - along * a_hat[2],
    ];
    let dist = (radial[0] * radial[0] + radial[1] * radial[1] + radial[2] * radial[2]).sqrt();
    let radial_res = (dist - er.radius).abs();

    let n_raw = er.plane_n.as_array();
    let n_len = (n_raw[0] * n_raw[0] + n_raw[1] * n_raw[1] + n_raw[2] * n_raw[2]).sqrt();
    let (n, d) = if n_len < cad_primitives::TAU_WORK {
        (n_raw, er.plane_d)
    } else {
        (
            [n_raw[0] / n_len, n_raw[1] / n_len, n_raw[2] / n_len],
            er.plane_d / n_len,
        )
    };
    let plane_res = (x[0] * n[0] + x[1] * n[1] + x[2] * n[2] + d).abs();
    radial_res.max(plane_res)
}

/// PR-YR10 (spec §4.4): the explicit Stage-4 watertightness gate (§4.4.3).
/// Every directed half-edge `(a, b)` must have exactly one opposite `(b, a)`
/// (a watertight 2-manifold), and each connected shell must be a closed
/// orientable 2-manifold with Euler characteristic
/// `χ = V − E + F = 2 − 2g` for genus `g ≥ 0` (χ even and ≤ 2); odd χ or
/// χ > 2 is impossible for such a shell and is rejected. Returns
/// `Err(NonManifoldOutput)` on failure.
/// KV9-F1 Increment 0c probe (kept env-gated, like the Stage-4 probes): name
/// the specific non-manifold gate that fired via `NONMANIFOLD_SITE_PROBE` so a
/// `NonManifoldOutput` wall self-localizes, then construct the error.
pub(crate) fn non_manifold_at(site: &str, detail: std::fmt::Arguments<'_>) -> YangError {
    if std::env::var("NONMANIFOLD_SITE_PROBE").is_ok() {
        eprintln!("NONMANIFOLD_SITE_PROBE {site}: {detail}");
    }
    YangError::NonManifoldOutput
}

/// Orientation of `tri` relative to its ASCENDING-sorted key: `+1` for an
/// even permutation, `−1` for an odd one. `key` must be `tri` sorted (all
/// three vertices distinct). Two triangles with the same key and OPPOSITE
/// sign are a doubled membrane (§ [`remove_doubled_membranes`]).
pub(crate) fn membrane_orientation_sign(tri: [u32; 3], key: [u32; 3]) -> i8 {
    let idx = |v: u32| key.iter().position(|&k| k == v).unwrap();
    let perm = (idx(tri[0]), idx(tri[1]), idx(tri[2]));
    // Even permutations of (0,1,2): (0,1,2),(1,2,0),(2,0,1).
    if matches!(perm, (0, 1, 2) | (1, 2, 0) | (2, 0, 1)) {
        1
    } else {
        -1
    }
}

/// DOUBLED-MEMBRANE removal (spec `yang_doubled_membrane_removal.md`, task
/// #146 χ=3 sub-layer). A DOUBLED MEMBRANE is a pair of triangles with the
/// IDENTICAL vertex set and OPPOSITE winding — a zero-thickness "fin" the
/// mesh boolean mints when a backtrack-spike / near-tangent junction leaves a
/// spur vertex just off a real edge (F0064 op-4 membrane {1237,1282,1290};
/// R0051 op-3 membrane {116,117,132} — the spur apex is used by NOTHING but
/// the two fin copies). The two coincident triangles carry opposite normals,
/// so the pair contributes NOTHING to the represented point-set, yet each of
/// its three shared edges gains one surplus `fwd` + one surplus `rev`
/// directed half-edge. The union shell then reads the topologically
/// IMPOSSIBLE odd Euler characteristic (χ=3: exactly one double-cover edge)
/// and [`check_watertight_2manifold`]'s shell gate stops loud.
///
/// Removing BOTH triangles of every opposite-winding pair is:
/// - **volume / point-set preserving** — a membrane is a zero-volume fin;
/// - **balance preserving** — each of the 3 shared edges loses exactly one
///   `fwd` and one `rev`, so the `fwd == rev` watertight invariant is
///   maintained and an edge that drops to zero simply vanishes; the pass can
///   NEVER open a new boundary or unbalance an edge (this is why it is safe
///   without re-checking the surrounding star);
/// - a strict **no-op on any valid 2-manifold**, which never contains two
///   triangles sharing a vertex set, so the entire green corpus is
///   BYTE-IDENTICAL through the pass (I5).
///
/// Purely combinatorial — NO positional tolerance (P9/P10), exactly like
/// [`split_pinch_vertices`]. A SAME-winding duplicate (a distinct defect, not
/// a cancelling fin) is deliberately left for the loud gate. The spur apex
/// vertex is left dangling here and dropped by the caller's
/// `compact_unreferenced_verts` (the pass returns `> 0`, which the caller
/// treats like a §4.5.3 collapse). Deterministic: sorted-triple + triangle
/// index order (I7). Returns the number of triangles removed.
pub(crate) fn remove_doubled_membranes(mesh: &mut Mesh) -> usize {
    use std::collections::BTreeMap;
    // Group triangles by ascending-sorted vertex triple → (index, sign).
    let mut groups: BTreeMap<[u32; 3], Vec<(usize, i8)>> = BTreeMap::new();
    for (ti, &tri) in mesh.tris.iter().enumerate() {
        let mut key = tri;
        key.sort_unstable();
        // A repeated vertex is a degenerate triangle, not a membrane — leave
        // it for the loud gate.
        if key[0] == key[1] || key[1] == key[2] {
            continue;
        }
        groups
            .entry(key)
            .or_default()
            .push((ti, membrane_orientation_sign(tri, key)));
    }
    let mut remove = vec![false; mesh.tris.len()];
    for members in groups.values() {
        if members.len() < 2 {
            continue;
        }
        // Cancel opposite-winding pairs (both directions of the same fin).
        // Any same-winding surplus is a DIFFERENT defect (two coincident
        // like-oriented triangles) and is left untouched (P9 — do not mask an
        // unexplained defect).
        let pos: Vec<usize> = members
            .iter()
            .filter(|&&(_, s)| s > 0)
            .map(|&(i, _)| i)
            .collect();
        let neg: Vec<usize> = members
            .iter()
            .filter(|&&(_, s)| s < 0)
            .map(|&(i, _)| i)
            .collect();
        let k = pos.len().min(neg.len());
        for i in 0..k {
            remove[pos[i]] = true;
            remove[neg[i]] = true;
        }
    }
    let removed = remove.iter().filter(|&&r| r).count();
    if removed > 0 {
        let kept: Vec<[u32; 3]> = mesh
            .tris
            .iter()
            .enumerate()
            .filter(|&(i, _)| !remove[i])
            .map(|(_, &t)| t)
            .collect();
        *mesh = Mesh::new(std::mem::take(&mut mesh.verts), kept);
    }
    removed
}

/// Tangency PINCH-VERTEX split (spec `yang_tangency_pinch_split.md`, task
/// #86): a vertex whose triangle star decomposes into ≥ 2 edge-connected
/// components, EACH a closed fan, is the mesh weld of a tangency pinch —
/// the union of two tangentially-touching solids (C0058's equal-R 30°
/// cylinders) self-touches at isolated points, and the standard manifold
/// B-Rep representation is one vertex PER SHEET at the same position
/// (Mäntylä [#23]). Without the split the shell-Euler accounting is a
/// bit-level lottery: an asymmetric weld reads the impossible χ=1 (loud
/// stop), a symmetric one reads χ=0 and SILENTLY masquerades as genus-1.
///
/// Split rule (I1, P9): a vertex splits ONLY when every v-incident edge of
/// its star has exactly 2 incident star triangles (so each component is a
/// closed disk). Open fans, isolated triangles, and non-manifold EDGES
/// (≠ 2 incident triangles — the perpendicular-Steinmetz EDGE-pinch class)
/// leave the vertex untouched and today's gates stay in charge. Split
/// copies carry IDENTICAL position bits (I2) and duplicate the vertex's
/// Stage-4 relocation tags (same curve parameter — the copies sit at the
/// same on-curve point). Deterministic: vertices, triangles, and fan
/// components in index order (I7).
pub(crate) fn split_pinch_vertices(mesh: &mut Mesh, relocations: &mut Vec<(u32, f64)>) -> usize {
    use std::collections::BTreeMap;
    let n = mesh.verts.len();
    let mut incident: Vec<Vec<u32>> = vec![Vec::new(); n];
    for (ti, tri) in mesh.tris.iter().enumerate() {
        for &v in tri {
            incident[v as usize].push(ti as u32);
        }
    }
    fn find(parent: &mut [usize], x: usize) -> usize {
        let mut r = x;
        while parent[r] != r {
            r = parent[r];
        }
        let mut cur = x;
        while parent[cur] != r {
            let next = parent[cur];
            parent[cur] = r;
            cur = next;
        }
        r
    }
    let mut splits = 0usize;
    for v in 0..n as u32 {
        let star = &incident[v as usize];
        if star.len() < 2 {
            continue;
        }
        // Star triangles connect when they share a v-incident edge (v, u).
        let mut by_u: BTreeMap<u32, Vec<usize>> = BTreeMap::new();
        for (li, &ti) in star.iter().enumerate() {
            for &u in &mesh.tris[ti as usize] {
                if u != v {
                    by_u.entry(u).or_default().push(li);
                }
            }
        }
        // Closed-fan precondition over the WHOLE star: every v-incident
        // edge has exactly 2 star triangles. Anything else (boundary fan,
        // non-manifold edge) → leave the vertex alone, loud gates unchanged.
        if by_u.values().any(|l| l.len() != 2) {
            continue;
        }
        let mut parent: Vec<usize> = (0..star.len()).collect();
        for l in by_u.values() {
            let (ra, rb) = (find(&mut parent, l[0]), find(&mut parent, l[1]));
            if ra != rb {
                parent[ra] = rb;
            }
        }
        let mut comps: BTreeMap<usize, Vec<usize>> = BTreeMap::new();
        for li in 0..star.len() {
            let r = find(&mut parent, li);
            comps.entry(r).or_default().push(li);
        }
        if comps.len() < 2 {
            continue;
        }
        // Every component is a closed fan (its v-edges are 2-valent and both
        // incident triangles landed in the same component by the union).
        // Component containing the lowest triangle index keeps v; each
        // further component gets a fresh vertex with v's position bits.
        let mut comp_list: Vec<Vec<usize>> = comps.into_values().collect();
        comp_list.sort_by_key(|c| c.iter().map(|&li| star[li]).min());
        for comp in comp_list.into_iter().skip(1) {
            let nv = mesh.verts.len() as u32;
            let pos = mesh.verts[v as usize];
            mesh.verts.push(pos);
            for &li in &comp {
                let ti = star[li] as usize;
                for slot in mesh.tris[ti].iter_mut() {
                    if *slot == v {
                        *slot = nv;
                    }
                }
            }
            let dup: Vec<(u32, f64)> = relocations
                .iter()
                .filter(|&&(rv, _)| rv == v)
                .map(|&(_, t)| (nv, t))
                .collect();
            relocations.extend(dup);
            splits += 1;
        }
    }
    splits
}

pub(crate) fn check_watertight_2manifold(mesh: &Mesh) -> Result<(), YangError> {
    use std::collections::{BTreeMap, BTreeSet};
    // Directed half-edge multiset: every (a,b) must be paired by one (b,a).
    let mut dir: BTreeMap<(u32, u32), i32> = BTreeMap::new();
    for tri in &mesh.tris {
        for (i, j) in [(0usize, 1usize), (1, 2), (2, 0)] {
            *dir.entry((tri[i], tri[j])).or_insert(0) += 1;
        }
    }
    for (&(s, e), &fwd) in &dir {
        let rev = dir.get(&(e, s)).copied().unwrap_or(0);
        if fwd != rev {
            if std::env::var("NONMANIFOLD_SITE_PROBE").is_ok() {
                eprintln!(
                    "NONMANIFOLD_SITE_PROBE s4-halfedge-pairing coords: \
                     v{s}={:?} v{e}={:?}",
                    mesh.verts[s as usize], mesh.verts[e as usize]
                );
            }
            return Err(non_manifold_at(
                "s4-halfedge-pairing",
                format_args!("edge ({s},{e}) fwd={fwd} rev={rev}"),
            ));
        }
        // NOTE (P10 record, re-confirmed 2026-07-08): do NOT tighten this
        // gate to reject `fwd == rev > 1` double covers — the kept set is
        // LEGITIMATELY edge-doubled at surface-tangency seams (the Steinmetz
        // subtract carries fwd=rev=2 along the tangency line and is a
        // CORRECT solid). This re-measures the standing
        // `yang_kept_mesh_manifold_gate` §2b record: no mesh-level
        // watertight/manifold invariant survives the kept set.
    }

    // Euler χ = 2 − 2g per connected shell (g ≥ 0). Connectivity via undirected
    // edges; the whole mesh is a union of disjoint closed orientable shells,
    // each of which has χ = 2 − 2g (sphere g=0 / through-hole g=1 / …).
    let n_verts = mesh.num_verts();
    if n_verts == 0 {
        return Ok(());
    }
    // Union-find over vertices through triangle edges.
    let mut parent: Vec<u32> = (0..n_verts as u32).collect();
    fn find(parent: &mut [u32], x: u32) -> u32 {
        let mut r = x;
        while parent[r as usize] != r {
            r = parent[r as usize];
        }
        // Path compression.
        let mut cur = x;
        while parent[cur as usize] != r {
            let next = parent[cur as usize];
            parent[cur as usize] = r;
            cur = next;
        }
        r
    }
    let union = |parent: &mut Vec<u32>, a: u32, b: u32| {
        let ra = find(parent, a);
        let rb = find(parent, b);
        if ra != rb {
            parent[ra as usize] = rb;
        }
    };
    for tri in &mesh.tris {
        union(&mut parent, tri[0], tri[1]);
        union(&mut parent, tri[1], tri[2]);
    }
    // Per-shell V, E (undirected), F.
    let mut shell_v: BTreeMap<u32, BTreeSet<u32>> = BTreeMap::new();
    let mut shell_e: BTreeMap<u32, BTreeSet<(u32, u32)>> = BTreeMap::new();
    let mut shell_f: BTreeMap<u32, i64> = BTreeMap::new();
    for tri in &mesh.tris {
        let root = find(&mut parent, tri[0]);
        let v_set = shell_v.entry(root).or_default();
        for &vi in tri {
            v_set.insert(vi);
        }
        let e_set = shell_e.entry(root).or_default();
        for (i, j) in [(0usize, 1usize), (1, 2), (2, 0)] {
            let (a, b) = (tri[i], tri[j]);
            e_set.insert(if a < b { (a, b) } else { (b, a) });
        }
        *shell_f.entry(root).or_insert(0) += 1;
    }
    for (root, v_set) in &shell_v {
        let v = v_set.len() as i64;
        let e = shell_e.get(root).map(|s| s.len()).unwrap_or(0) as i64;
        let f = shell_f.get(root).copied().unwrap_or(0);
        let chi = v - e + f;
        // A closed orientable 2-manifold shell has χ = 2 − 2g for integer genus
        // g ≥ 0, so χ is EVEN and ≤ 2. Accept any such χ (sphere χ=2 / g=0;
        // through-hole χ=0 / g=1; …). Reject odd χ or χ > 2 — impossible for a
        // closed orientable manifold → a real defect (NOT a tolerance/fallback
        // relaxation; P9/P10).
        if chi > 2 || chi.rem_euclid(2) != 0 {
            return Err(non_manifold_at(
                "s4-shell-euler",
                format_args!("shell root {root} chi={chi} v={v} e={e} f={f}"),
            ));
        }
    }
    Ok(())
}
