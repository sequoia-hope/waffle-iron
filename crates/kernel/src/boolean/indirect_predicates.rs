//! Indirect geometric predicates for mesh arrangement.
//! Ref [#9] Cherchi et al. 2020, Sections 4.1-4.3.
//!
//! Intersection points are represented implicitly as unevaluated combinations
//! of input vertices. Predicates operate on these implicit representations
//! using multi-stage filtering (float → expansion) to guarantee exact results
//! without materializing coordinates.
//!
//! ## True Indirect Predicates
//!
//! The orient2d and pointCompare predicates avoid dividing λ by d_L/d_T.
//! Instead they work with the homogeneous representation (d, λx, λy, λz)
//! directly, using a two-stage filter:
//!
//! 1. **Float filter**: Compute the expression in f64 with a semi-static
//!    error bound (Cherchi 2020 Table 1). If the result exceeds the bound,
//!    the sign is guaranteed correct.
//! 2. **Exact expansion**: Use Shewchuk-style expansion arithmetic
//!    (via `geometry_predicates::predicates`) to compute the exact sign.

use geometry_predicates::predicates as gp;

/// Projection axis for 2D orientation tests.
/// Drop one coordinate to project 3D points onto a 2D plane.
#[derive(Debug, Clone, Copy)]
pub(crate) enum ProjectionAxis {
    XY,
    YZ,
    ZX,
}

/// Coordinate axis for point comparison.
#[derive(Debug, Clone, Copy)]
pub(crate) enum Axis {
    X,
    Y,
    Z,
}

/// Implicit point types per Cherchi 2020 Section 4.1.
///
/// - **Explicit**: Input vertex with known f64 coordinates.
/// - **LPI**: Line-Plane Intersection — edge (q1,q2) intersects plane (r,s,t).
///   Defined by 5 explicit points. Undefined when d_L = 0 (edge parallel to plane).
/// - **TPI**: Three-Plane Intersection — 3 non-coplanar triangles meet at a point.
///   Defined by 9 explicit points. Undefined when d_T = 0 (planes not independent).
#[derive(Debug, Clone)]
pub(crate) enum ImplicitPoint {
    /// Known explicit coordinates.
    Explicit([f64; 3]),

    /// Line-Plane Intersection: edge (q1→q2) crosses plane of triangle (r, s, t).
    LPI {
        q1: [f64; 3],
        q2: [f64; 3],
        r: [f64; 3],
        s: [f64; 3],
        t: [f64; 3],
    },

    /// Three-Plane Intersection: planes of triangles (v1,v2,v3), (w1,w2,w3), (u1,u2,u3).
    TPI {
        v1: [f64; 3],
        v2: [f64; 3],
        v3: [f64; 3],
        w1: [f64; 3],
        w2: [f64; 3],
        w3: [f64; 3],
        u1: [f64; 3],
        u2: [f64; 3],
        u3: [f64; 3],
    },
}

impl ImplicitPoint {
    /// Compute explicit coordinates for this implicit point.
    ///
    /// Returns `None` if the point is undefined (d_L = 0 for LPI, d_T = 0 for TPI).
    pub(crate) fn materialize(&self) -> Option<[f64; 3]> {
        match self {
            ImplicitPoint::Explicit(coords) => Some(*coords),
            ImplicitPoint::LPI { q1, q2, r, s, t } => materialize_lpi(q1, q2, r, s, t),
            ImplicitPoint::TPI {
                v1,
                v2,
                v3,
                w1,
                w2,
                w3,
                u1,
                u2,
                u3,
            } => materialize_tpi(v1, v2, v3, w1, w2, w3, u1, u2, u3),
        }
    }

    /// Check if this implicit point is well-defined.
    ///
    /// - Explicit: always defined
    /// - LPI: defined when d_L ≠ 0 (edge not parallel to plane)
    /// - TPI: defined when d_T ≠ 0 (planes linearly independent)
    pub(crate) fn is_defined(&self) -> bool {
        match self {
            ImplicitPoint::Explicit(_) => true,
            ImplicitPoint::LPI { q1, q2, r, s, t } => {
                let d_l = det3x3_lpi(q1, q2, r, s, t);
                d_l.abs() > 0.0
            }
            ImplicitPoint::TPI {
                v1,
                v2,
                v3,
                w1,
                w2,
                w3,
                u1,
                u2,
                u3,
            } => {
                let d_t = det3x3_tpi(v1, v2, v3, w1, w2, w3, u1, u2, u3);
                d_t.abs() > 0.0
            }
        }
    }
}

// ── LPI coordinate formulas (Cherchi 2020 Section 4.1) ───────────────────

/// Compute d_L = det|(q1-q2), (s-r), (t-r)| for an LPI point.
fn det3x3_lpi(q1: &[f64; 3], q2: &[f64; 3], r: &[f64; 3], s: &[f64; 3], t: &[f64; 3]) -> f64 {
    let a = [q1[0] - q2[0], q1[1] - q2[1], q1[2] - q2[2]];
    let b = [s[0] - r[0], s[1] - r[1], s[2] - r[2]];
    let c = [t[0] - r[0], t[1] - r[1], t[2] - r[2]];
    det3x3(&a, &b, &c)
}

/// Materialize LPI: edge (q1,q2) intersects plane(r,s,t).
///
/// ```text
/// d_L = det|(q1-q2), (s-r), (t-r)|
/// n   = det|(q1-r),  (s-r), (t-r)|
/// λ_Lx = d_L·q1x + n·(q2x - q1x)
/// λ_Ly = d_L·q1y + n·(q2y - q1y)
/// λ_Lz = d_L·q1z + n·(q2z - q1z)
/// coords = (λ_Lx/d_L, λ_Ly/d_L, λ_Lz/d_L)
/// ```
fn materialize_lpi(
    q1: &[f64; 3],
    q2: &[f64; 3],
    r: &[f64; 3],
    s: &[f64; 3],
    t: &[f64; 3],
) -> Option<[f64; 3]> {
    let d_l = det3x3_lpi(q1, q2, r, s, t);
    if d_l == 0.0 {
        return None; // Edge parallel to plane
    }

    let a_n = [q1[0] - r[0], q1[1] - r[1], q1[2] - r[2]];
    let b_n = [s[0] - r[0], s[1] - r[1], s[2] - r[2]];
    let c_n = [t[0] - r[0], t[1] - r[1], t[2] - r[2]];
    let n = det3x3(&a_n, &b_n, &c_n);

    let lambda_x = d_l * q1[0] + n * (q2[0] - q1[0]);
    let lambda_y = d_l * q1[1] + n * (q2[1] - q1[1]);
    let lambda_z = d_l * q1[2] + n * (q2[2] - q1[2]);

    Some([lambda_x / d_l, lambda_y / d_l, lambda_z / d_l])
}

// ── TPI coordinate formulas (Cherchi 2020 Section 4.1) ───────────────────

/// Compute d_T for a TPI point: determinant of the 3 triangle normals.
fn det3x3_tpi(
    v1: &[f64; 3],
    v2: &[f64; 3],
    v3: &[f64; 3],
    w1: &[f64; 3],
    w2: &[f64; 3],
    w3: &[f64; 3],
    u1: &[f64; 3],
    u2: &[f64; 3],
    u3: &[f64; 3],
) -> f64 {
    let nv = cross_sub(v1, v2, v3);
    let nw = cross_sub(w1, w2, w3);
    let nu = cross_sub(u1, u2, u3);
    det3x3(&nv, &nw, &nu)
}

/// Materialize TPI: intersection of three planes.
///
/// ```text
/// nv = (v2-v1) × (v3-v2),  nw = (w2-w1) × (w3-w2),  nu = (u2-u1) × (u3-u2)
/// d_T = det|nv, nw, nu|
/// pv = nv · v1,  pw = nw · w1,  pu = nu · u1
/// λ_Tx = det|pv nvy nvz; pw nwy nwz; pu nuy nuz|
/// λ_Ty = det|nvx pv nvz; nwx pw nwz; nux pu nuz|
/// λ_Tz = det|nvx nvy pv; nwx nwy pw; nux nuy pu|
/// coords = (λ_Tx/d_T, λ_Ty/d_T, λ_Tz/d_T)
/// ```
fn materialize_tpi(
    v1: &[f64; 3],
    v2: &[f64; 3],
    v3: &[f64; 3],
    w1: &[f64; 3],
    w2: &[f64; 3],
    w3: &[f64; 3],
    u1: &[f64; 3],
    u2: &[f64; 3],
    u3: &[f64; 3],
) -> Option<[f64; 3]> {
    let nv = cross_sub(v1, v2, v3);
    let nw = cross_sub(w1, w2, w3);
    let nu = cross_sub(u1, u2, u3);

    let d_t = det3x3(&nv, &nw, &nu);
    if d_t == 0.0 {
        return None; // Planes not linearly independent
    }

    let pv = dot(&nv, v1);
    let pw = dot(&nw, w1);
    let pu = dot(&nu, u1);

    // Cramer's rule columns
    let lambda_x = det3x3(
        &[pv, nv[1], nv[2]],
        &[pw, nw[1], nw[2]],
        &[pu, nu[1], nu[2]],
    );
    let lambda_y = det3x3(
        &[nv[0], pv, nv[2]],
        &[nw[0], pw, nw[2]],
        &[nu[0], pu, nu[2]],
    );
    let lambda_z = det3x3(
        &[nv[0], nv[1], pv],
        &[nw[0], nw[1], pw],
        &[nu[0], nu[1], pu],
    );

    Some([lambda_x / d_t, lambda_y / d_t, lambda_z / d_t])
}

// ── Linear algebra helpers ───────────────────────────────────────────────

/// 3×3 determinant: det|row0, row1, row2| (rows are 3-element slices).
fn det3x3(r0: &[f64; 3], r1: &[f64; 3], r2: &[f64; 3]) -> f64 {
    r0[0] * (r1[1] * r2[2] - r1[2] * r2[1]) - r0[1] * (r1[0] * r2[2] - r1[2] * r2[0])
        + r0[2] * (r1[0] * r2[1] - r1[1] * r2[0])
}

/// Cross product of (b-a) × (c-b): triangle normal from vertices a, b, c.
fn cross_sub(a: &[f64; 3], b: &[f64; 3], c: &[f64; 3]) -> [f64; 3] {
    let ab = [b[0] - a[0], b[1] - a[1], b[2] - a[2]];
    let bc = [c[0] - b[0], c[1] - b[1], c[2] - b[2]];
    [
        ab[1] * bc[2] - ab[2] * bc[1],
        ab[2] * bc[0] - ab[0] * bc[2],
        ab[0] * bc[1] - ab[1] * bc[0],
    ]
}

/// Dot product of two 3-vectors.
fn dot(a: &[f64; 3], b: &[f64; 3]) -> f64 {
    a[0] * b[0] + a[1] * b[1] + a[2] * b[2]
}

// ── True indirect orient2d predicates ────────────────────────────────────
//
// These predicates compute orient2d WITHOUT dividing λ by d_L/d_T.
// The key identity:
//
//   d_L * orient2d(LPI, e1, e2) = λi*(e1j-e2j) + λj*(e2i-e1i) + d_L*(e1i*e2j-e1j*e2i)
//
// So sign(orient2d) = sign(det) * sign(d_L) where det is the RHS above.
// This avoids the precision-losing division that caused 55 non-conformal
// edges on three_cubes.stl.

/// Compute LPI lambda values WITHOUT division.
/// Returns `(d_L, λx, λy, λz)` or `None` if degenerate (d_L == 0).
///
/// Ref: Cherchi 2020 Section 4.1, Eq. for pL.
fn lpi_lambda(
    q1: &[f64; 3],
    q2: &[f64; 3],
    r: &[f64; 3],
    s: &[f64; 3],
    t: &[f64; 3],
) -> Option<(f64, f64, f64, f64)> {
    let d_l = det3x3_lpi(q1, q2, r, s, t);
    if d_l == 0.0 {
        return None;
    }
    let a_n = [q1[0] - r[0], q1[1] - r[1], q1[2] - r[2]];
    let b_n = [s[0] - r[0], s[1] - r[1], s[2] - r[2]];
    let c_n = [t[0] - r[0], t[1] - r[1], t[2] - r[2]];
    let n = det3x3(&a_n, &b_n, &c_n);

    let lx = d_l * q1[0] + n * (q2[0] - q1[0]);
    let ly = d_l * q1[1] + n * (q2[1] - q1[1]);
    let lz = d_l * q1[2] + n * (q2[2] - q1[2]);
    Some((d_l, lx, ly, lz))
}

/// Max absolute value across all coordinates of the 5 LPI-defining points
/// plus 2 explicit points (21 coordinates total).
fn max_abs_coords_lee(
    q1: &[f64; 3],
    q2: &[f64; 3],
    r: &[f64; 3],
    s: &[f64; 3],
    t: &[f64; 3],
    e1: &[f64; 3],
    e2: &[f64; 3],
) -> f64 {
    let mut m: f64 = 0.0;
    for v in [q1, q2, r, s, t, e1, e2] {
        for &c in v {
            m = m.max(c.abs());
        }
    }
    m.max(f64::MIN_POSITIVE) // avoid zero
}

/// True indirect orient2d for (LPI, Explicit, Explicit).
///
/// Computes `sign(orient2d(LPI, e1, e2))` on the `(i,j)` projection
/// without materializing the LPI point.
///
/// Two-stage filter:
/// - Stage 1: f64 with semi-static error bound (Cherchi 2020 Table 1)
/// - Stage 2: Shewchuk expansion arithmetic (exact)
fn orient2d_lee(
    q1: &[f64; 3],
    q2: &[f64; 3],
    r: &[f64; 3],
    s: &[f64; 3],
    t: &[f64; 3],
    e1: &[f64; 3],
    e2: &[f64; 3],
    i: usize,
    j: usize,
) -> f64 {
    // ── Stage 1: float filter ──────────────────────────────────────────
    let (d_l, lx, ly, lz) = match lpi_lambda(q1, q2, r, s, t) {
        Some(v) => v,
        None => return 0.0,
    };
    let lambda = [lx, ly, lz];

    // det = λ_i*(e1_j - e2_j) + λ_j*(e2_i - e1_i) + d_L*(e1_i*e2_j - e1_j*e2_i)
    // This equals d_L * orient2d(LPI_materialized, e1, e2).
    let t1 = e1[j] - e2[j];
    let t2 = e2[i] - e1[i];
    let e = lambda[i] * t1 + lambda[j] * t2;
    let pr = e1[i] * e2[j] - e1[j] * e2[i];
    let dpr = d_l * pr;
    let det = dpr + e;

    let delta = max_abs_coords_lee(q1, q2, r, s, t, e1, e2);
    // Filter for det: degree 5, epsilon = 4.75e-14 (Cherchi 2020 Table 1)
    let d5 = delta * delta * delta * delta * delta;
    let eps_det = 4.75e-14 * d5;
    // Filter for d_L: degree 3, epsilon ≈ 5e-15
    let d3 = delta * delta * delta;
    let eps_dl = 5.0e-15 * d3;

    if det.abs() > eps_det && d_l.abs() > eps_dl {
        // Both signs are reliable
        return if (det > 0.0) == (d_l > 0.0) {
            1.0
        } else {
            -1.0
        };
    }

    // ── Stage 2: exact expansion arithmetic ────────────────────────────
    orient2d_lee_exact(q1, q2, r, s, t, e1, e2, i, j)
}

/// Exact orient2d_LEE using Shewchuk expansion arithmetic.
///
/// Computes sign(det) * sign(d_L) where:
///   det = λ_i*(e1_j-e2_j) + λ_j*(e2_i-e1_i) + d_L*(e1_i*e2_j-e1_j*e2_i)
///
/// All intermediate values are computed as exact expansions.
fn orient2d_lee_exact(
    q1: &[f64; 3],
    q2: &[f64; 3],
    r: &[f64; 3],
    s: &[f64; 3],
    t: &[f64; 3],
    e1: &[f64; 3],
    e2: &[f64; 3],
    i: usize,
    j: usize,
) -> f64 {
    // Compute d_L and n as exact expansions via 3×3 determinant.
    // d_L = det|(q1-q2), (s-r), (t-r)|
    // n   = det|(q1-r),  (s-r), (t-r)|
    let d_l_exp = det3x3_exact(
        q1[0] - q2[0],
        q1[1] - q2[1],
        q1[2] - q2[2],
        s[0] - r[0],
        s[1] - r[1],
        s[2] - r[2],
        t[0] - r[0],
        t[1] - r[1],
        t[2] - r[2],
    );
    let n_exp = det3x3_exact(
        q1[0] - r[0],
        q1[1] - r[1],
        q1[2] - r[2],
        s[0] - r[0],
        s[1] - r[1],
        s[2] - r[2],
        t[0] - r[0],
        t[1] - r[1],
        t[2] - r[2],
    );

    let d_l_sign = expansion_sign(&d_l_exp);
    if d_l_sign == 0 {
        return 0.0; // degenerate
    }

    // λ_i = d_L * q1[i] + n * (q2[i] - q1[i])
    // λ_j = d_L * q1[j] + n * (q2[j] - q1[j])
    let li = expansion_add(
        &expansion_scale(&d_l_exp, q1[i]),
        &expansion_scale(&n_exp, q2[i] - q1[i]),
    );
    let lj = expansion_add(
        &expansion_scale(&d_l_exp, q1[j]),
        &expansion_scale(&n_exp, q2[j] - q1[j]),
    );

    // det = λ_i*(e1[j]-e2[j]) + λ_j*(e2[i]-e1[i]) + d_L*(e1[i]*e2[j]-e1[j]*e2[i])
    let term1 = expansion_scale(&li, e1[j] - e2[j]);
    let term2 = expansion_scale(&lj, e2[i] - e1[i]);

    // e1[i]*e2[j] - e1[j]*e2[i] needs exact two_product then subtraction
    let [pr_hi, pr_lo] = gp::two_product(e1[i], e2[j]);
    let [nr_hi, nr_lo] = gp::two_product(e1[j], e2[i]);
    let cross_e = gp::two_two_diff(pr_hi, pr_lo, nr_hi, nr_lo);
    let term3 = expansion_mul_expansion(&d_l_exp, &cross_e);

    let sum12 = expansion_add(&term1, &term2);
    let det_exp = expansion_add(&sum12, &term3);

    let det_sign = expansion_sign(&det_exp);
    // sign(orient2d) = sign(det) * sign(d_L)
    let combined = det_sign * d_l_sign;
    combined as f64
}

// ── Expansion arithmetic helpers ────────────────────────────────────────
// Wrappers around geometry_predicates::predicates for Vec<f64> expansions.

/// Compute exact 3×3 determinant as an expansion.
/// det = a0*(b1*c2 - b2*c1) - a1*(b0*c2 - b2*c0) + a2*(b0*c1 - b1*c0)
fn det3x3_exact(
    a0: f64,
    a1: f64,
    a2: f64,
    b0: f64,
    b1: f64,
    b2: f64,
    c0: f64,
    c1: f64,
    c2: f64,
) -> Vec<f64> {
    // Compute three 2×2 minors as exact expansions
    let m0 = cross_product_2d(b1, c2, b2, c1); // b1*c2 - b2*c1
    let m1 = cross_product_2d(b0, c2, b2, c0); // b0*c2 - b2*c0
    let m2 = cross_product_2d(b0, c1, b1, c0); // b0*c1 - b1*c0

    // Scale each minor by the corresponding a component
    let t0 = expansion_scale(&m0, a0);
    let t1 = expansion_scale(&m1, -a1);
    let t2 = expansion_scale(&m2, a2);

    expansion_add(&expansion_add(&t0, &t1), &t2)
}

/// Exact 2D cross product: a*b - c*d as an expansion.
fn cross_product_2d(a: f64, b: f64, c: f64, d: f64) -> Vec<f64> {
    let [ab_hi, ab_lo] = gp::two_product(a, b);
    let [cd_hi, cd_lo] = gp::two_product(c, d);
    let result = gp::two_two_diff(ab_hi, ab_lo, cd_hi, cd_lo);
    result.to_vec()
}

/// Scale an expansion by a scalar (exact).
fn expansion_scale(e: &[f64], b: f64) -> Vec<f64> {
    if e.is_empty() || b == 0.0 {
        return vec![0.0];
    }
    let mut h = vec![0.0; e.len() * 2];
    let len = gp::scale_expansion_zeroelim(e, b, &mut h);
    h.truncate(len);
    if h.is_empty() {
        h.push(0.0);
    }
    h
}

/// Add two expansions (exact).
fn expansion_add(e: &[f64], f: &[f64]) -> Vec<f64> {
    let mut h = vec![0.0; e.len() + f.len()];
    let len = gp::fast_expansion_sum_zeroelim(e, f, &mut h);
    h.truncate(len);
    if h.is_empty() {
        h.push(0.0);
    }
    h
}

/// Multiply two expansions (exact).
/// Uses the identity: e * f = sum_i(f_i * e) accumulated.
fn expansion_mul_expansion(e: &[f64], f: &[f64]) -> Vec<f64> {
    if e.is_empty() || f.is_empty() {
        return vec![0.0];
    }
    let mut result = expansion_scale(e, f[0]);
    for &fi in &f[1..] {
        let term = expansion_scale(e, fi);
        result = expansion_add(&result, &term);
    }
    result
}

/// Sign of an expansion: +1, -1, or 0.
/// The most significant (last) nonzero component determines the sign.
fn expansion_sign(e: &[f64]) -> i32 {
    for &v in e.iter().rev() {
        if v > 0.0 {
            return 1;
        }
        if v < 0.0 {
            return -1;
        }
    }
    0
}

// ── Orient2d dispatch ───────────────────────────────────────────────────

/// Orient2d for implicit points projected onto a coordinate plane.
///
/// Returns the sign of the orientation determinant:
/// - Positive (+1.0) → counter-clockwise (CCW)
/// - Negative (-1.0) → clockwise (CW)
/// - Zero (0.0) → collinear
///
/// Uses true indirect predicates for LEE (LPI, Explicit, Explicit) — the
/// most common case in mesh arrangement. Avoids the precision-losing
/// materialization (division by d_L) that caused non-conformal edges.
///
/// For other combinations (LLE, LLL, TEE, etc.), falls back to
/// materialize + Shewchuk exact orient2d.
pub(crate) fn orient2d_indirect(
    a: &ImplicitPoint,
    b: &ImplicitPoint,
    c: &ImplicitPoint,
    proj: ProjectionAxis,
) -> f64 {
    let (i, j) = match proj {
        ProjectionAxis::XY => (0, 1),
        ProjectionAxis::YZ => (1, 2),
        ProjectionAxis::ZX => (2, 0),
    };

    // Dispatch based on point type combination.
    // Use true indirect for LEE; materialize fallback for others.
    match (a, b, c) {
        // LEE: one LPI, two explicit
        (
            ImplicitPoint::LPI { q1, q2, r, s, t },
            ImplicitPoint::Explicit(e1),
            ImplicitPoint::Explicit(e2),
        ) => orient2d_lee(q1, q2, r, s, t, e1, e2, i, j),

        // ELE → -orient2d(L, E, E) via antisymmetry: swap first two args
        (
            ImplicitPoint::Explicit(e1),
            ImplicitPoint::LPI { q1, q2, r, s, t },
            ImplicitPoint::Explicit(e2),
        ) => -orient2d_lee(q1, q2, r, s, t, e1, e2, i, j),

        // EEL → orient2d(L, E, E) via cyclic permutation (even permutation)
        (
            ImplicitPoint::Explicit(e1),
            ImplicitPoint::Explicit(e2),
            ImplicitPoint::LPI { q1, q2, r, s, t },
        ) => orient2d_lee(q1, q2, r, s, t, e1, e2, i, j),

        // EEE: all explicit — use Shewchuk directly
        (ImplicitPoint::Explicit(ea), ImplicitPoint::Explicit(eb), ImplicitPoint::Explicit(ec)) => {
            geometry_predicates::orient2d([ea[i], ea[j]], [eb[i], eb[j]], [ec[i], ec[j]])
        }

        // All other combinations: materialize and delegate
        _ => orient2d_materialize_fallback(a, b, c, i, j),
    }
}

/// Fallback: materialize implicit points and use Shewchuk orient2d.
/// Used for LLE, LLL, TEE, TTE, TTT, etc.
fn orient2d_materialize_fallback(
    a: &ImplicitPoint,
    b: &ImplicitPoint,
    c: &ImplicitPoint,
    i: usize,
    j: usize,
) -> f64 {
    let a_c = match a.materialize() {
        Some(c) => c,
        None => return 0.0,
    };
    let b_c = match b.materialize() {
        Some(c) => c,
        None => return 0.0,
    };
    let c_c = match c.materialize() {
        Some(c) => c,
        None => return 0.0,
    };
    geometry_predicates::orient2d([a_c[i], a_c[j]], [b_c[i], b_c[j]], [c_c[i], c_c[j]])
}

// ── Orient3d: dispatch + implementation ──────────────────────────────────

/// Orient3d for implicit points.
///
/// Returns the sign of the orientation determinant for 4 points in 3D:
/// - Positive → d is below the plane of (a,b,c)
/// - Negative → d is above
/// - Zero → coplanar
#[allow(dead_code)] // Will be used in Phase 2 cell extraction
pub(crate) fn orient3d_indirect(
    a: &ImplicitPoint,
    b: &ImplicitPoint,
    c: &ImplicitPoint,
    d: &ImplicitPoint,
) -> f64 {
    let a_c = match a.materialize() {
        Some(c) => c,
        None => return 0.0,
    };
    let b_c = match b.materialize() {
        Some(c) => c,
        None => return 0.0,
    };
    let c_c = match c.materialize() {
        Some(c) => c,
        None => return 0.0,
    };
    let d_c = match d.materialize() {
        Some(c) => c,
        None => return 0.0,
    };

    geometry_predicates::orient3d(a_c, b_c, c_c, d_c)
}

// ── Point comparison ─────────────────────────────────────────────────────

/// Compare two implicit points along a single coordinate axis.
///
/// Returns `Ordering` for lexicographic sorting of intersection points
/// along edges (Cherchi 2020 Section 4.3).
///
/// Uses true indirect comparison for LE (LPI vs Explicit) — no division:
///   LPI.x = λ_Lx / d_L  vs  e_x
///   ⟺ sign(d_L) * sign(λ_Lx - d_L * e_x)
///
/// For other combinations (LL, LT, TT, etc.), falls back to materialize.
pub(crate) fn point_compare_on_axis(
    a: &ImplicitPoint,
    b: &ImplicitPoint,
    axis: Axis,
) -> std::cmp::Ordering {
    let idx = match axis {
        Axis::X => 0,
        Axis::Y => 1,
        Axis::Z => 2,
    };

    // Dispatch on point types
    match (a, b) {
        // EE: exact subtraction, no indirect needed
        (ImplicitPoint::Explicit(ea), ImplicitPoint::Explicit(eb)) => ea[idx].total_cmp(&eb[idx]),

        // LE: LPI vs Explicit — true indirect
        (ImplicitPoint::LPI { q1, q2, r, s, t }, ImplicitPoint::Explicit(e)) => {
            point_compare_le(q1, q2, r, s, t, e, idx)
        }

        // EL: Explicit vs LPI — reverse the comparison
        (ImplicitPoint::Explicit(e), ImplicitPoint::LPI { q1, q2, r, s, t }) => {
            point_compare_le(q1, q2, r, s, t, e, idx).reverse()
        }

        // All others: materialize fallback
        _ => {
            let a_c = match a.materialize() {
                Some(c) => c,
                None => return std::cmp::Ordering::Equal,
            };
            let b_c = match b.materialize() {
                Some(c) => c,
                None => return std::cmp::Ordering::Equal,
            };
            a_c[idx].total_cmp(&b_c[idx])
        }
    }
}

/// True indirect comparison: LPI.axis vs Explicit.axis, no division.
///
/// LPI[idx] = λ[idx] / d_L
/// Compare with e[idx]: sign(λ[idx] / d_L - e[idx]) = sign(d_L) * sign(λ[idx] - d_L * e[idx])
///
/// Ref: Cherchi 2020 Section 4.3, pointCompare_on_X_LE.
fn point_compare_le(
    q1: &[f64; 3],
    q2: &[f64; 3],
    r: &[f64; 3],
    s: &[f64; 3],
    t: &[f64; 3],
    e: &[f64; 3],
    idx: usize,
) -> std::cmp::Ordering {
    // Stage 1: float filter
    let (d_l, lx, ly, lz) = match lpi_lambda(q1, q2, r, s, t) {
        Some(v) => v,
        None => return std::cmp::Ordering::Equal,
    };
    let lambda_idx = [lx, ly, lz][idx];
    let kx = lambda_idx - d_l * e[idx];

    // Filter: epsilon = 1.93e-14 * delta^4 (Cherchi 2020 Table 1)
    let mut delta: f64 = 0.0;
    for v in [q1, q2, r, s, t, e] {
        for &c in v {
            delta = delta.max(c.abs());
        }
    }
    delta = delta.max(f64::MIN_POSITIVE);
    let d4 = delta * delta * delta * delta;
    let eps = 1.93e-14 * d4;

    if kx.abs() > eps && d_l.abs() > 5.0e-15 * delta * delta * delta {
        // sign(LPI[idx] - e[idx]) = sign(d_L) * sign(kx)
        let s = if (kx > 0.0) == (d_l > 0.0) { 1 } else { -1 };
        return match s {
            x if x > 0 => std::cmp::Ordering::Greater,
            x if x < 0 => std::cmp::Ordering::Less,
            _ => std::cmp::Ordering::Equal,
        };
    }

    // Stage 2: exact expansion
    point_compare_le_exact(q1, q2, r, s, t, e, idx)
}

/// Exact point comparison using expansion arithmetic.
fn point_compare_le_exact(
    q1: &[f64; 3],
    q2: &[f64; 3],
    r: &[f64; 3],
    s: &[f64; 3],
    t: &[f64; 3],
    e: &[f64; 3],
    idx: usize,
) -> std::cmp::Ordering {
    // d_L as exact expansion
    let d_l_exp = det3x3_exact(
        q1[0] - q2[0],
        q1[1] - q2[1],
        q1[2] - q2[2],
        s[0] - r[0],
        s[1] - r[1],
        s[2] - r[2],
        t[0] - r[0],
        t[1] - r[1],
        t[2] - r[2],
    );
    let d_l_sign = expansion_sign(&d_l_exp);
    if d_l_sign == 0 {
        return std::cmp::Ordering::Equal;
    }

    // n as exact expansion
    let n_exp = det3x3_exact(
        q1[0] - r[0],
        q1[1] - r[1],
        q1[2] - r[2],
        s[0] - r[0],
        s[1] - r[1],
        s[2] - r[2],
        t[0] - r[0],
        t[1] - r[1],
        t[2] - r[2],
    );

    // λ[idx] = d_L * q1[idx] + n * (q2[idx] - q1[idx])
    let lambda_exp = expansion_add(
        &expansion_scale(&d_l_exp, q1[idx]),
        &expansion_scale(&n_exp, q2[idx] - q1[idx]),
    );

    // kx = λ[idx] - d_L * e[idx]
    let d_l_e = expansion_scale(&d_l_exp, e[idx]);
    let kx_exp = expansion_add(&lambda_exp, &expansion_negate(&d_l_e));

    let kx_sign = expansion_sign(&kx_exp);
    // sign(LPI[idx] - e[idx]) = sign(d_L) * sign(kx)
    let combined = d_l_sign * kx_sign;
    match combined {
        x if x > 0 => std::cmp::Ordering::Greater,
        x if x < 0 => std::cmp::Ordering::Less,
        _ => std::cmp::Ordering::Equal,
    }
}

/// Negate an expansion.
fn expansion_negate(e: &[f64]) -> Vec<f64> {
    e.iter().map(|&v| -v).collect()
}

#[cfg(test)]
mod tests {
    use super::*;

    /// Test 1: orient2d with 3 explicit points must match Shewchuk orient2d.
    #[test]
    fn test_orient2d_eee_matches_shewchuk() {
        let a = ImplicitPoint::Explicit([0.0, 0.0, 0.0]);
        let b = ImplicitPoint::Explicit([1.0, 0.0, 0.0]);
        let c = ImplicitPoint::Explicit([0.0, 1.0, 0.0]);

        let ccw = orient2d_indirect(&a, &b, &c, ProjectionAxis::XY);
        assert!(ccw > 0.0, "EEE CCW should be positive, got {ccw}");

        let cw = orient2d_indirect(&a, &c, &b, ProjectionAxis::XY);
        assert!(cw < 0.0, "EEE CW should be negative, got {cw}");
    }

    /// Test 2: orient2d with one LPI point and two explicit points (LEE).
    #[test]
    fn test_orient2d_lee_basic() {
        let lpi = ImplicitPoint::LPI {
            q1: [0.0, 0.0, -1.0],
            q2: [0.0, 0.0, 1.0],
            r: [1.0, 0.0, 0.0],
            s: [0.0, 1.0, 0.0],
            t: [-1.0, 0.0, 0.0],
        };
        let e1 = ImplicitPoint::Explicit([1.0, 0.0, 0.0]);
        let e2 = ImplicitPoint::Explicit([0.0, 1.0, 0.0]);

        let result = orient2d_indirect(&lpi, &e1, &e2, ProjectionAxis::XY);
        assert!(
            result > 0.0,
            "LEE orient2d should be positive (CCW), got {result}"
        );
    }

    /// Test 3: point_compare_on_axis must sort LPI points correctly.
    #[test]
    fn test_point_compare_sorts_correctly() {
        let lpi_x2 = ImplicitPoint::LPI {
            q1: [0.0, 0.0, 0.0],
            q2: [10.0, 0.0, 0.0],
            r: [2.0, 1.0, 0.0],
            s: [2.0, 0.0, 1.0],
            t: [2.0, -1.0, 0.0],
        };
        let lpi_x5 = ImplicitPoint::LPI {
            q1: [0.0, 0.0, 0.0],
            q2: [10.0, 0.0, 0.0],
            r: [5.0, 1.0, 0.0],
            s: [5.0, 0.0, 1.0],
            t: [5.0, -1.0, 0.0],
        };
        let lpi_x8 = ImplicitPoint::LPI {
            q1: [0.0, 0.0, 0.0],
            q2: [10.0, 0.0, 0.0],
            r: [8.0, 1.0, 0.0],
            s: [8.0, 0.0, 1.0],
            t: [8.0, -1.0, 0.0],
        };

        let mut points = vec![lpi_x8.clone(), lpi_x2.clone(), lpi_x5.clone()];
        points.sort_by(|a, b| point_compare_on_axis(a, b, Axis::X));

        assert_eq!(
            point_compare_on_axis(&lpi_x2, &lpi_x5, Axis::X),
            std::cmp::Ordering::Less
        );
        assert_eq!(
            point_compare_on_axis(&lpi_x5, &lpi_x8, Axis::X),
            std::cmp::Ordering::Less
        );
        assert_eq!(
            point_compare_on_axis(&lpi_x2, &lpi_x8, Axis::X),
            std::cmp::Ordering::Less
        );
    }

    /// Test 4: orient2d with near-collinear LPI point — expansion fallback.
    #[test]
    fn test_lpi_orient2d_degenerate() {
        let lpi_near_line = ImplicitPoint::LPI {
            q1: [0.5, 1e-15, -1.0],
            q2: [0.5, 1e-15, 1.0],
            r: [1.0, 0.0, 0.0],
            s: [0.0, 1.0, 0.0],
            t: [-1.0, 0.0, 0.0],
        };
        let e1 = ImplicitPoint::Explicit([0.0, 0.0, 0.0]);
        let e2 = ImplicitPoint::Explicit([1.0, 0.0, 0.0]);

        let result = orient2d_indirect(&lpi_near_line, &e1, &e2, ProjectionAxis::XY);
        assert!(
            result != 0.0,
            "Near-collinear LPI orient2d must not return 0; got {result}"
        );
    }

    /// Test 5: LEE orient2d gives same sign as materialized orient2d.
    /// Verifies the true indirect formula matches the geometric definition.
    #[test]
    fn test_orient2d_lee_no_materialize() {
        // LPI with negative d_L — tests the sign correction logic.
        // Edge (0,0,-1)→(0,0,1), plane through (1,0,0),(0,1,0),(-1,0,0).
        // d_L = -4, LPI materializes at (0,0,0).
        let lpi = ImplicitPoint::LPI {
            q1: [0.0, 0.0, -1.0],
            q2: [0.0, 0.0, 1.0],
            r: [1.0, 0.0, 0.0],
            s: [0.0, 1.0, 0.0],
            t: [-1.0, 0.0, 0.0],
        };
        let materialized = lpi.materialize().unwrap();
        assert!((materialized[0]).abs() < 1e-15);
        assert!((materialized[1]).abs() < 1e-15);

        for &(e1, e2) in &[
            ([1.0, 0.0, 0.0], [0.0, 1.0, 0.0]),
            ([0.5, 0.5, 0.0], [-0.3, 0.7, 0.0]),
            ([1.0, 1.0, 0.0], [2.0, 0.0, 0.0]),
        ] {
            let e1p = ImplicitPoint::Explicit(e1);
            let e2p = ImplicitPoint::Explicit(e2);

            let indirect = orient2d_indirect(&lpi, &e1p, &e2p, ProjectionAxis::XY);
            let direct = geometry_predicates::orient2d(
                [materialized[0], materialized[1]],
                [e1[0], e1[1]],
                [e2[0], e2[1]],
            );

            assert_eq!(
                indirect.signum(),
                direct.signum(),
                "Indirect and materialized orient2d must agree for e1={e1:?}, e2={e2:?}: \
                 indirect={indirect}, direct={direct}"
            );
        }
    }

    /// Test 6: orient2d with ELE and EEL permutations.
    /// Verifies antisymmetry dispatch is correct.
    #[test]
    fn test_orient2d_lee_permutations() {
        let lpi = ImplicitPoint::LPI {
            q1: [0.3, 0.2, -1.0],
            q2: [0.3, 0.2, 1.0],
            r: [1.0, 0.0, 0.0],
            s: [0.0, 1.0, 0.0],
            t: [-1.0, 0.0, 0.0],
        };
        let e1 = ImplicitPoint::Explicit([1.0, 0.0, 0.0]);
        let e2 = ImplicitPoint::Explicit([0.0, 1.0, 0.0]);

        let lee = orient2d_indirect(&lpi, &e1, &e2, ProjectionAxis::XY);
        let ele = orient2d_indirect(&e1, &lpi, &e2, ProjectionAxis::XY);
        let eel = orient2d_indirect(&e1, &e2, &lpi, ProjectionAxis::XY);

        // orient2d(L,E1,E2) = -orient2d(E1,L,E2)
        assert_eq!(
            lee.signum(),
            -ele.signum(),
            "LEE and ELE should have opposite signs: lee={lee}, ele={ele}"
        );
        // orient2d(E1,E2,L) = orient2d(L,E1,E2) (cyclic permutation)
        assert_eq!(
            lee.signum(),
            eel.signum(),
            "LEE and EEL should have same sign: lee={lee}, eel={eel}"
        );
    }

    /// Test 7: point_compare LE — LPI vs Explicit on each axis.
    #[test]
    fn test_point_compare_le_no_materialize() {
        // LPI at (0.3, 0.2, 0) approximately
        let lpi = ImplicitPoint::LPI {
            q1: [0.3, 0.2, -1.0],
            q2: [0.3, 0.2, 1.0],
            r: [1.0, 0.0, 0.0],
            s: [0.0, 1.0, 0.0],
            t: [-1.0, 0.0, 0.0],
        };
        let materialized = lpi.materialize().unwrap();

        // Compare against explicit points that are clearly less/greater
        let less_x = ImplicitPoint::Explicit([0.1, 0.0, 0.0]);
        let more_x = ImplicitPoint::Explicit([0.5, 0.0, 0.0]);

        assert_eq!(
            point_compare_on_axis(&lpi, &less_x, Axis::X),
            std::cmp::Ordering::Greater,
            "LPI(x≈0.3) > Explicit(x=0.1)"
        );
        assert_eq!(
            point_compare_on_axis(&lpi, &more_x, Axis::X),
            std::cmp::Ordering::Less,
            "LPI(x≈0.3) < Explicit(x=0.5)"
        );

        // Test Y axis
        let less_y = ImplicitPoint::Explicit([0.0, 0.1, 0.0]);
        let more_y = ImplicitPoint::Explicit([0.0, 0.4, 0.0]);
        assert_eq!(
            point_compare_on_axis(&lpi, &less_y, Axis::Y),
            std::cmp::Ordering::Greater,
            "LPI(y≈0.2) > Explicit(y=0.1)"
        );
        assert_eq!(
            point_compare_on_axis(&lpi, &more_y, Axis::Y),
            std::cmp::Ordering::Less,
            "LPI(y≈0.2) < Explicit(y=0.4)"
        );

        // Compare with the materialized value itself — should be Equal
        let exact_pt = ImplicitPoint::Explicit(materialized);
        assert_eq!(
            point_compare_on_axis(&lpi, &exact_pt, Axis::X),
            std::cmp::Ordering::Equal,
            "LPI compared to its own materialized X should be Equal"
        );
    }

    /// Test 8: orient2d_lee on YZ and ZX projections.
    #[test]
    fn test_orient2d_lee_all_projections() {
        // LPI at (0,0,0), compare orient2d on all three projections
        let lpi = ImplicitPoint::LPI {
            q1: [0.0, 0.0, -1.0],
            q2: [0.0, 0.0, 1.0],
            r: [1.0, 0.0, 0.0],
            s: [0.0, 1.0, 0.0],
            t: [-1.0, 0.0, 0.0],
        };
        let materialized = lpi.materialize().unwrap();

        let pairs = [
            ([1.0, 0.5, 0.3], [0.0, 1.0, 0.7]),
            ([0.0, 0.0, 1.0], [1.0, 1.0, 0.0]),
        ];

        for proj in [ProjectionAxis::XY, ProjectionAxis::YZ, ProjectionAxis::ZX] {
            let (i, j) = match proj {
                ProjectionAxis::XY => (0, 1),
                ProjectionAxis::YZ => (1, 2),
                ProjectionAxis::ZX => (2, 0),
            };

            for &(e1, e2) in &pairs {
                let indirect = orient2d_indirect(
                    &lpi,
                    &ImplicitPoint::Explicit(e1),
                    &ImplicitPoint::Explicit(e2),
                    proj,
                );
                let direct = geometry_predicates::orient2d(
                    [materialized[i], materialized[j]],
                    [e1[i], e1[j]],
                    [e2[i], e2[j]],
                );
                assert_eq!(
                    indirect.signum(),
                    direct.signum(),
                    "Projection {proj:?}: indirect={indirect}, direct={direct}, e1={e1:?}, e2={e2:?}"
                );
            }
        }
    }

    /// Test 9: expansion arithmetic exact fallback produces correct results.
    #[test]
    fn test_orient2d_lee_degenerate_filter_fallback() {
        // Very near-collinear case that should force expansion fallback.
        // LPI at approximately (0.5, 1e-16, 0), line from (0,0) to (1,0).
        let lpi = ImplicitPoint::LPI {
            q1: [0.5, 1e-16, -1.0],
            q2: [0.5, 1e-16, 1.0],
            r: [1.0, 0.0, 0.0],
            s: [0.0, 1.0, 0.0],
            t: [-1.0, 0.0, 0.0],
        };
        let e1 = ImplicitPoint::Explicit([0.0, 0.0, 0.0]);
        let e2 = ImplicitPoint::Explicit([1.0, 0.0, 0.0]);

        let result = orient2d_indirect(&lpi, &e1, &e2, ProjectionAxis::XY);
        // The LPI is slightly above the line, so the sign should be nonzero
        assert!(
            result != 0.0,
            "Degenerate case must not return 0, got {result}"
        );

        // Verify it matches materialized result
        let mat = lpi.materialize().unwrap();
        let direct = geometry_predicates::orient2d([mat[0], mat[1]], [0.0, 0.0], [1.0, 0.0]);
        assert_eq!(
            result.signum(),
            direct.signum(),
            "Expansion fallback must match materialized: indirect={result}, direct={direct}"
        );
    }
}
