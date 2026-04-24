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

impl Eq for ImplicitPoint {}

impl PartialEq for ImplicitPoint {
    fn eq(&self, other: &Self) -> bool {
        less_than_indirect(self, other) == std::cmp::Ordering::Equal
    }
}

impl PartialOrd for ImplicitPoint {
    fn partial_cmp(&self, other: &Self) -> Option<std::cmp::Ordering> {
        Some(self.cmp(other))
    }
}

impl Ord for ImplicitPoint {
    fn cmp(&self, other: &Self) -> std::cmp::Ordering {
        less_than_indirect(self, other)
    }
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

/// Compute LPI lambda parameters using full expansion arithmetic.
/// Returns `(d_L_sign, d_L_expansion, λx, λy, λz)` or `None` if degenerate.
///
/// Uses `two_diff` for ALL input subtractions — no f64 precision loss.
/// Ref: Cherchi 2020 Section 4.1, Shewchuk 1997 [#4].
fn lpi_lambda_expansion(
    q1: &[f64; 3],
    q2: &[f64; 3],
    r: &[f64; 3],
    s: &[f64; 3],
    t: &[f64; 3],
) -> Option<(i32, Vec<f64>, Vec<f64>, Vec<f64>, Vec<f64>)> {
    // d_L = det|(q1-q2), (s-r), (t-r)| — all subtractions exact via two_diff
    let d_l_exp = det3x3_exact_pairs(
        q1[0], q2[0], q1[1], q2[1], q1[2], q2[2], s[0], r[0], s[1], r[1], s[2], r[2], t[0], r[0],
        t[1], r[1], t[2], r[2],
    );
    let d_l_sign = expansion_sign(&d_l_exp);
    if d_l_sign == 0 {
        return None;
    }

    // n = det|(q1-r), (s-r), (t-r)| — all subtractions exact
    let n_exp = det3x3_exact_pairs(
        q1[0], r[0], q1[1], r[1], q1[2], r[2], s[0], r[0], s[1], r[1], s[2], r[2], t[0], r[0],
        t[1], r[1], t[2], r[2],
    );

    // λ_x = d_L * q1[0] + n * (q2[0] - q1[0])  — (q2-q1) via two_diff
    let lx = expansion_add(
        &expansion_scale(&d_l_exp, q1[0]),
        &expansion_mul_expansion(&n_exp, &two_diff_exp(q2[0], q1[0])),
    );
    let ly = expansion_add(
        &expansion_scale(&d_l_exp, q1[1]),
        &expansion_mul_expansion(&n_exp, &two_diff_exp(q2[1], q1[1])),
    );
    let lz = expansion_add(
        &expansion_scale(&d_l_exp, q1[2]),
        &expansion_mul_expansion(&n_exp, &two_diff_exp(q2[2], q1[2])),
    );

    Some((d_l_sign, d_l_exp, lx, ly, lz))
}

/// Max absolute value across all coordinates of the 5 LPI-defining points
/// plus 2 explicit points (21 coordinates total).
#[allow(dead_code)] // Used by disabled float filter in orient2d_lee
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
    // Float filter disabled: our two-step lambda computation (separate
    // lpi_lambda then formula) introduces intermediate rounding that
    // exceeds the Cherchi 2020 Table 1 error bound. The C++ reference
    // computes everything inline in a single pass. Until we replicate the
    // exact evaluation DAG, always use exact expansion arithmetic.
    // TODO: implement single-pass evaluation to re-enable the float filter.
    orient2d_lee_exact(q1, q2, r, s, t, e1, e2, i, j)
}

/// Exact orient2d_LEE using Shewchuk expansion arithmetic.
///
/// Computes sign(det) * sign(d_L) where:
///   det = λ_i*(e1_j-e2_j) + λ_j*(e2_i-e1_i) + d_L*(e1_i*e2_j-e1_j*e2_i)
///
/// All intermediate values are computed as exact expansions.
/// Uses `lpi_lambda_expansion` with `two_diff` for all subtractions.
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
    let (d_l_sign, d_l_exp, lx, ly, lz) = match lpi_lambda_expansion(q1, q2, r, s, t) {
        Some(v) => v,
        None => return 0.0,
    };
    let lambda = [lx, ly, lz];

    // det = λ_i*(e1[j]-e2[j]) + λ_j*(e2[i]-e1[i]) + d_L*(e1[i]*e2[j]-e1[j]*e2[i])
    // All subtractions via two_diff for exactness.
    let term1 = expansion_mul_expansion(&lambda[i], &two_diff_exp(e1[j], e2[j]));
    let term2 = expansion_mul_expansion(&lambda[j], &two_diff_exp(e2[i], e1[i]));

    // e1[i]*e2[j] - e1[j]*e2[i] — exact cross product
    let [pr_lo, pr_hi] = gp::two_product(e1[i], e2[j]);
    let [nr_lo, nr_hi] = gp::two_product(e1[j], e2[i]);
    let cross_e = gp::two_two_diff(pr_hi, pr_lo, nr_hi, nr_lo);
    let term3 = expansion_mul_expansion(&d_l_exp, &cross_e);

    let sum12 = expansion_add(&term1, &term2);
    let det_exp = expansion_add(&sum12, &term3);

    let det_sign = expansion_sign(&det_exp);
    (det_sign * d_l_sign) as f64
}

/// Compute exact 3×3 determinant using two_diff for input subtractions.
/// Each pair (a_pos, a_neg) represents the difference a_pos - a_neg,
/// computed exactly via two_diff.
/// det = (a0p-a0n)*((b1p-b1n)*(c2p-c2n) - (b2p-b2n)*(c1p-c1n))
///     - (a1p-a1n)*((b0p-b0n)*(c2p-c2n) - (b2p-b2n)*(c0p-c0n))
///     + (a2p-a2n)*((b0p-b0n)*(c1p-c1n) - (b1p-b1n)*(c0p-c0n))
fn det3x3_exact_pairs(
    a0p: f64,
    a0n: f64,
    a1p: f64,
    a1n: f64,
    a2p: f64,
    a2n: f64,
    b0p: f64,
    b0n: f64,
    b1p: f64,
    b1n: f64,
    b2p: f64,
    b2n: f64,
    c0p: f64,
    c0n: f64,
    c1p: f64,
    c1n: f64,
    c2p: f64,
    c2n: f64,
) -> Vec<f64> {
    // Exact subtractions via two_diff → 2-component expansions
    let a0 = two_diff_exp(a0p, a0n);
    let a1 = two_diff_exp(a1p, a1n);
    let a2 = two_diff_exp(a2p, a2n);
    let b0 = two_diff_exp(b0p, b0n);
    let b1 = two_diff_exp(b1p, b1n);
    let b2 = two_diff_exp(b2p, b2n);
    let c0 = two_diff_exp(c0p, c0n);
    let c1 = two_diff_exp(c1p, c1n);
    let c2 = two_diff_exp(c2p, c2n);

    // 2×2 minors: m0 = b1*c2 - b2*c1, etc.
    let m0 = expansion_add(
        &expansion_mul_expansion(&b1, &c2),
        &expansion_negate(&expansion_mul_expansion(&b2, &c1)),
    );
    let m1 = expansion_add(
        &expansion_mul_expansion(&b0, &c2),
        &expansion_negate(&expansion_mul_expansion(&b2, &c0)),
    );
    let m2 = expansion_add(
        &expansion_mul_expansion(&b0, &c1),
        &expansion_negate(&expansion_mul_expansion(&b1, &c0)),
    );

    // det = a0*m0 - a1*m1 + a2*m2
    let t0 = expansion_mul_expansion(&a0, &m0);
    let t1 = expansion_negate(&expansion_mul_expansion(&a1, &m1));
    let t2 = expansion_mul_expansion(&a2, &m2);

    expansion_add(&expansion_add(&t0, &t1), &t2)
}

/// Create a 2-component expansion from two_diff.
fn two_diff_exp(a: f64, b: f64) -> Vec<f64> {
    let [lo, hi] = gp::two_diff(a, b);
    if lo == 0.0 {
        vec![hi]
    } else {
        vec![lo, hi]
    }
}

/// Negate an expansion.
fn expansion_negate(e: &[f64]) -> Vec<f64> {
    e.iter().map(|&v| -v).collect()
}

// ── Expansion arithmetic helpers ────────────────────────────────────────
// Wrappers around geometry_predicates::predicates for Vec<f64> expansions.

/// Compute exact 3×3 determinant as an expansion.
/// det = a0*(b1*c2 - b2*c1) - a1*(b0*c2 - b2*c0) + a2*(b0*c1 - b1*c0)
#[allow(dead_code)] // Superseded by det3x3_exact_pairs which uses two_diff for inputs
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
#[allow(dead_code)] // Used by det3x3_exact
fn cross_product_2d(a: f64, b: f64, c: f64, d: f64) -> Vec<f64> {
    let [ab_lo, ab_hi] = gp::two_product(a, b);
    let [cd_lo, cd_hi] = gp::two_product(c, d);
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

// ── True indirect orient2d: LLE (degree-11) ────────────────────────────
//
// orient2d(LPI_a, LPI_b, Explicit_c) without materializing either LPI.
//
// Substitute homogeneous coords into orient2d determinant and multiply
// through by d_a * d_b:
//   det = (λax - cx*da)(λby - cy*db) - (λay - cy*da)(λbx - cx*db)
// sign(orient2d) = sign(da) * sign(db) * sign(det)

/// True indirect orient2d for (LPI_a, LPI_b, Explicit_c).
fn orient2d_lle(
    a: &ImplicitPoint,
    b: &ImplicitPoint,
    c: &ImplicitPoint,
    i: usize,
    j: usize,
) -> f64 {
    let (q1a, q2a, ra, sa, ta) = match a {
        ImplicitPoint::LPI { q1, q2, r, s, t } => (q1, q2, r, s, t),
        _ => unreachable!(),
    };
    let (q1b, q2b, rb, sb, tb) = match b {
        ImplicitPoint::LPI { q1, q2, r, s, t } => (q1, q2, r, s, t),
        _ => unreachable!(),
    };
    let ec = match c {
        ImplicitPoint::Explicit(e) => e,
        _ => unreachable!(),
    };

    orient2d_lle_exact(q1a, q2a, ra, sa, ta, q1b, q2b, rb, sb, tb, ec, i, j)
}

/// Exact orient2d_LLE using expansion arithmetic.
///
/// det = (λa_i - c_i*da)(λb_j - c_j*db) - (λa_j - c_j*da)(λb_i - c_i*db)
/// sign(orient2d) = sign(da) * sign(db) * sign(det)
///
/// Uses `lpi_lambda_expansion` with `two_diff` for all subtractions.
fn orient2d_lle_exact(
    q1a: &[f64; 3],
    q2a: &[f64; 3],
    ra: &[f64; 3],
    sa: &[f64; 3],
    ta: &[f64; 3],
    q1b: &[f64; 3],
    q2b: &[f64; 3],
    rb: &[f64; 3],
    sb: &[f64; 3],
    tb: &[f64; 3],
    ec: &[f64; 3],
    i: usize,
    j: usize,
) -> f64 {
    let (da_sign, da_exp, lax, lay, laz) = match lpi_lambda_expansion(q1a, q2a, ra, sa, ta) {
        Some(v) => v,
        None => return 0.0,
    };
    let (db_sign, db_exp, lbx, lby, lbz) = match lpi_lambda_expansion(q1b, q2b, rb, sb, tb) {
        Some(v) => v,
        None => return 0.0,
    };

    let la = [lax, lay, laz];
    let lb = [lbx, lby, lbz];

    // p1 = λa_i - c_i * da
    let p1 = expansion_add(&la[i], &expansion_negate(&expansion_scale(&da_exp, ec[i])));
    // p2 = λb_j - c_j * db
    let p2 = expansion_add(&lb[j], &expansion_negate(&expansion_scale(&db_exp, ec[j])));
    // p3 = λa_j - c_j * da
    let p3 = expansion_add(&la[j], &expansion_negate(&expansion_scale(&da_exp, ec[j])));
    // p4 = λb_i - c_i * db
    let p4 = expansion_add(&lb[i], &expansion_negate(&expansion_scale(&db_exp, ec[i])));

    // det = p1*p2 - p3*p4
    let term1 = expansion_mul_expansion(&p1, &p2);
    let term2 = expansion_mul_expansion(&p3, &p4);
    let det_exp = expansion_add(&term1, &expansion_negate(&term2));

    let det_sign = expansion_sign(&det_exp);
    (da_sign * db_sign * det_sign) as f64
}

// ── True indirect orient2d: LLL (degree-14) ────────────────────────────
//
// orient2d(LPI_a, LPI_b, LPI_c) without materializing any LPI.
//
// Substitute and multiply through by da * db * dc^2:
//   det = (λa_i*dc - λc_i*da)(λb_j*dc - λc_j*db)
//       - (λa_j*dc - λc_j*da)(λb_i*dc - λc_i*db)
// sign(orient2d) = sign(da) * sign(db) * sign(det)
// (dc^2 is always positive, drops out of sign)

/// True indirect orient2d for (LPI_a, LPI_b, LPI_c).
fn orient2d_lll(
    a: &ImplicitPoint,
    b: &ImplicitPoint,
    c: &ImplicitPoint,
    i: usize,
    j: usize,
) -> f64 {
    let (q1a, q2a, ra, sa, ta) = match a {
        ImplicitPoint::LPI { q1, q2, r, s, t } => (q1, q2, r, s, t),
        _ => unreachable!(),
    };
    let (q1b, q2b, rb, sb, tb) = match b {
        ImplicitPoint::LPI { q1, q2, r, s, t } => (q1, q2, r, s, t),
        _ => unreachable!(),
    };
    let (q1c, q2c, rc, sc, tc) = match c {
        ImplicitPoint::LPI { q1, q2, r, s, t } => (q1, q2, r, s, t),
        _ => unreachable!(),
    };

    orient2d_lll_exact(
        q1a, q2a, ra, sa, ta, q1b, q2b, rb, sb, tb, q1c, q2c, rc, sc, tc, i, j,
    )
}

/// Exact orient2d_LLL using expansion arithmetic.
///
/// Uses `lpi_lambda_expansion` with `two_diff` for all subtractions.
#[allow(clippy::too_many_arguments)]
fn orient2d_lll_exact(
    q1a: &[f64; 3],
    q2a: &[f64; 3],
    ra: &[f64; 3],
    sa: &[f64; 3],
    ta: &[f64; 3],
    q1b: &[f64; 3],
    q2b: &[f64; 3],
    rb: &[f64; 3],
    sb: &[f64; 3],
    tb: &[f64; 3],
    q1c: &[f64; 3],
    q2c: &[f64; 3],
    rc: &[f64; 3],
    sc: &[f64; 3],
    tc: &[f64; 3],
    i: usize,
    j: usize,
) -> f64 {
    let (da_sign, da_exp, lax, lay, laz) = match lpi_lambda_expansion(q1a, q2a, ra, sa, ta) {
        Some(v) => v,
        None => return 0.0,
    };
    let (db_sign, db_exp, lbx, lby, lbz) = match lpi_lambda_expansion(q1b, q2b, rb, sb, tb) {
        Some(v) => v,
        None => return 0.0,
    };
    let (_, dc_exp, lcx, lcy, lcz) = match lpi_lambda_expansion(q1c, q2c, rc, sc, tc) {
        Some(v) => v,
        None => return 0.0,
    };

    let la = [lax, lay, laz];
    let lb = [lbx, lby, lbz];
    let lc = [lcx, lcy, lcz];

    // p1 = λa_i*dc - λc_i*da
    let p1 = expansion_add(
        &expansion_mul_expansion(&la[i], &dc_exp),
        &expansion_negate(&expansion_mul_expansion(&lc[i], &da_exp)),
    );
    // p2 = λb_j*dc - λc_j*db
    let p2 = expansion_add(
        &expansion_mul_expansion(&lb[j], &dc_exp),
        &expansion_negate(&expansion_mul_expansion(&lc[j], &db_exp)),
    );
    // p3 = λa_j*dc - λc_j*da
    let p3 = expansion_add(
        &expansion_mul_expansion(&la[j], &dc_exp),
        &expansion_negate(&expansion_mul_expansion(&lc[j], &da_exp)),
    );
    // p4 = λb_i*dc - λc_i*db
    let p4 = expansion_add(
        &expansion_mul_expansion(&lb[i], &dc_exp),
        &expansion_negate(&expansion_mul_expansion(&lc[i], &db_exp)),
    );

    // det = p1*p2 - p3*p4
    let term1 = expansion_mul_expansion(&p1, &p2);
    let term2 = expansion_mul_expansion(&p3, &p4);
    let det_exp = expansion_add(&term1, &expansion_negate(&term2));

    let det_sign = expansion_sign(&det_exp);
    (da_sign * db_sign * det_sign) as f64
}

// ── True indirect point comparison: LL ──────────────────────────────────
//
// Compare two LPI points on a single axis without materializing:
//   LPI_a[idx] = λa[idx] / da  vs  LPI_b[idx] = λb[idx] / db
//   sign(diff) = sign(da) * sign(db) * sign(λa[idx]*db - λb[idx]*da)

/// True indirect comparison: LPI_a vs LPI_b on a single axis.
fn point_compare_ll(
    q1a: &[f64; 3],
    q2a: &[f64; 3],
    ra: &[f64; 3],
    sa: &[f64; 3],
    ta: &[f64; 3],
    q1b: &[f64; 3],
    q2b: &[f64; 3],
    rb: &[f64; 3],
    sb: &[f64; 3],
    tb: &[f64; 3],
    idx: usize,
) -> std::cmp::Ordering {
    point_compare_ll_exact(q1a, q2a, ra, sa, ta, q1b, q2b, rb, sb, tb, idx)
}

/// Exact LL point comparison using expansion arithmetic.
///
/// Uses `lpi_lambda_expansion` with `two_diff` for all subtractions.
fn point_compare_ll_exact(
    q1a: &[f64; 3],
    q2a: &[f64; 3],
    ra: &[f64; 3],
    sa: &[f64; 3],
    ta: &[f64; 3],
    q1b: &[f64; 3],
    q2b: &[f64; 3],
    rb: &[f64; 3],
    sb: &[f64; 3],
    tb: &[f64; 3],
    idx: usize,
) -> std::cmp::Ordering {
    let (da_sign, da_exp, lax, lay, laz) = match lpi_lambda_expansion(q1a, q2a, ra, sa, ta) {
        Some(v) => v,
        None => return std::cmp::Ordering::Equal,
    };
    let (db_sign, db_exp, lbx, lby, lbz) = match lpi_lambda_expansion(q1b, q2b, rb, sb, tb) {
        Some(v) => v,
        None => return std::cmp::Ordering::Equal,
    };

    let la = [lax, lay, laz];
    let lb = [lbx, lby, lbz];

    // diff = λa[idx]*db - λb[idx]*da
    let diff_exp = expansion_add(
        &expansion_mul_expansion(&la[idx], &db_exp),
        &expansion_negate(&expansion_mul_expansion(&lb[idx], &da_exp)),
    );

    let diff_sign = expansion_sign(&diff_exp);
    let combined = da_sign * db_sign * diff_sign;
    match combined {
        x if x > 0 => std::cmp::Ordering::Greater,
        x if x < 0 => std::cmp::Ordering::Less,
        _ => std::cmp::Ordering::Equal,
    }
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

        // LLE: two LPIs + one explicit
        (ImplicitPoint::LPI { .. }, ImplicitPoint::LPI { .. }, ImplicitPoint::Explicit(_)) => {
            orient2d_lle(a, b, c, i, j)
        }
        // LLE permutations via antisymmetry
        (ImplicitPoint::LPI { .. }, ImplicitPoint::Explicit(_), ImplicitPoint::LPI { .. }) => {
            -orient2d_lle(a, c, b, i, j) // swap b,c → negate
        }
        (ImplicitPoint::Explicit(_), ImplicitPoint::LPI { .. }, ImplicitPoint::LPI { .. }) => {
            orient2d_lle(b, c, a, i, j) // rotate: (E,L,L) → orient2d(L,L,E)
        }

        // LLL: three LPIs
        (ImplicitPoint::LPI { .. }, ImplicitPoint::LPI { .. }, ImplicitPoint::LPI { .. }) => {
            orient2d_lll(a, b, c, i, j)
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
///
/// Uses true indirect predicates for combinations involving LPI points,
/// avoiding precision-losing materialization (division by d_L).
///
/// Ref: Cherchi 2020 Section 4.2, genericPoint::orient3D.
pub(crate) fn orient3d_indirect(
    a: &ImplicitPoint,
    b: &ImplicitPoint,
    c: &ImplicitPoint,
    d: &ImplicitPoint,
) -> f64 {
    // Count LPI points and their positions to dispatch
    let types = (
        matches!(a, ImplicitPoint::LPI { .. }),
        matches!(b, ImplicitPoint::LPI { .. }),
        matches!(c, ImplicitPoint::LPI { .. }),
        matches!(d, ImplicitPoint::LPI { .. }),
    );

    match types {
        // EEEE: all explicit — use Shewchuk directly
        (false, false, false, false) => {
            let ea = match a {
                ImplicitPoint::Explicit(e) => e,
                _ => unreachable!(),
            };
            let eb = match b {
                ImplicitPoint::Explicit(e) => e,
                _ => unreachable!(),
            };
            let ec = match c {
                ImplicitPoint::Explicit(e) => e,
                _ => unreachable!(),
            };
            let ed = match d {
                ImplicitPoint::Explicit(e) => e,
                _ => unreachable!(),
            };
            geometry_predicates::orient3d(*ea, *eb, *ec, *ed)
        }

        // LEEE: one LPI + three explicit
        (true, false, false, false) => orient3d_leee(a, b, c, d),
        // Permutations: use orient3d antisymmetry (swap two args → negate)
        (false, true, false, false) => -orient3d_leee(b, a, c, d),
        (false, false, true, false) => -orient3d_leee(c, b, a, d),
        (false, false, false, true) => -orient3d_leee(d, b, c, a),

        // LLEE: two LPIs + two explicit (6 permutations)
        (true, true, false, false) => orient3d_llee(a, b, c, d),
        (true, false, true, false) => -orient3d_llee(a, c, b, d),
        (true, false, false, true) => -orient3d_llee(a, d, c, b),
        (false, true, true, false) => orient3d_llee(b, c, a, d),
        (false, true, false, true) => orient3d_llee(b, d, a, c),
        (false, false, true, true) => orient3d_llee(c, d, a, b),

        // LLLE, LLLL: materialize fallback (TPI combinations also)
        _ => orient3d_materialize_fallback(a, b, c, d),
    }
}

/// Fallback: materialize implicit points and use Shewchuk orient3d.
fn orient3d_materialize_fallback(
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

/// True indirect orient3d for (LPI, Explicit, Explicit, Explicit).
///
/// The 4×4 orient3d determinant with one LPI point p_L = (λx/d, λy/d, λz/d):
///
/// ```text
/// orient3d(pL, e1, e2, e3) = sign(d_L) * sign(
///   (λx - e1x*d) * cofactor_x(e1,e2,e3)
/// + (λy - e1y*d) * cofactor_y(e1,e2,e3)
/// + (λz - e1z*d) * cofactor_z(e1,e2,e3)
/// )
/// ```
///
/// where cofactor_x/y/z are the 2×2 minors of the explicit-only 3×3 submatrix
/// formed by rows (e2-e1, e3-e1).
///
/// Ref: Cherchi 2020 Section 4.2, orient3D_LEEE (indirect predicates C++).
fn orient3d_leee(
    l: &ImplicitPoint,
    e1: &ImplicitPoint,
    e2: &ImplicitPoint,
    e3: &ImplicitPoint,
) -> f64 {
    let (q1, q2, r, s, t) = match l {
        ImplicitPoint::LPI { q1, q2, r, s, t } => (q1, q2, r, s, t),
        _ => unreachable!(),
    };
    let e1c = match e1 {
        ImplicitPoint::Explicit(e) => e,
        _ => unreachable!(),
    };
    let e2c = match e2 {
        ImplicitPoint::Explicit(e) => e,
        _ => unreachable!(),
    };
    let e3c = match e3 {
        ImplicitPoint::Explicit(e) => e,
        _ => unreachable!(),
    };

    orient3d_leee_exact(q1, q2, r, s, t, e1c, e2c, e3c)
}

/// Exact orient3d_LEEE using expansion arithmetic.
///
/// Substitutes LPI homogeneous coords into the 4×4 orient3d determinant:
///   Row 0: (λx/d - e1x, λy/d - e1y, λz/d - e1z)
///   Row 1: (e2x - e1x, e2y - e1y, e2z - e1z)
///   Row 2: (e3x - e1x, e3y - e1y, e3z - e1z)
///
/// Multiply through by d_L to avoid division:
///   det_scaled = (λx - e1x*d)(cofY) - (λy - e1y*d)(cofX) + (λz - e1z*d)(cofZ)
///   sign(orient3d) = sign(d_L) * sign(det_scaled)
///
/// where cofX, cofY, cofZ are the cofactors from the explicit-only rows.
#[allow(clippy::too_many_arguments)]
fn orient3d_leee_exact(
    q1: &[f64; 3],
    q2: &[f64; 3],
    r: &[f64; 3],
    s: &[f64; 3],
    t: &[f64; 3],
    e1: &[f64; 3],
    e2: &[f64; 3],
    e3: &[f64; 3],
) -> f64 {
    let (d_l_sign, d_l_exp, lx, ly, lz) = match lpi_lambda_expansion(q1, q2, r, s, t) {
        Some(v) => v,
        None => return 0.0,
    };

    // Row 1: e2 - e1 (exact via two_diff)
    let r1x = two_diff_exp(e2[0], e1[0]);
    let r1y = two_diff_exp(e2[1], e1[1]);
    let r1z = two_diff_exp(e2[2], e1[2]);

    // Row 2: e3 - e1 (exact via two_diff)
    let r2x = two_diff_exp(e3[0], e1[0]);
    let r2y = two_diff_exp(e3[1], e1[1]);
    let r2z = two_diff_exp(e3[2], e1[2]);

    // Cofactors (2×2 minors of the explicit rows):
    // cofX = r1y*r2z - r1z*r2y  (cofactor for x column)
    // cofY = r1x*r2z - r1z*r2x  (cofactor for y column, note sign absorbed below)
    // cofZ = r1x*r2y - r1y*r2x  (cofactor for z column)
    let cof_x = expansion_add(
        &expansion_mul_expansion(&r1y, &r2z),
        &expansion_negate(&expansion_mul_expansion(&r1z, &r2y)),
    );
    let cof_y = expansion_add(
        &expansion_mul_expansion(&r1x, &r2z),
        &expansion_negate(&expansion_mul_expansion(&r1z, &r2x)),
    );
    let cof_z = expansion_add(
        &expansion_mul_expansion(&r1x, &r2y),
        &expansion_negate(&expansion_mul_expansion(&r1y, &r2x)),
    );

    // Row 0 (scaled by d_L): (λ_k - e1_k * d_L) for k in {x, y, z}
    let p0x = expansion_add(&lx, &expansion_negate(&expansion_scale(&d_l_exp, e1[0])));
    let p0y = expansion_add(&ly, &expansion_negate(&expansion_scale(&d_l_exp, e1[1])));
    let p0z = expansion_add(&lz, &expansion_negate(&expansion_scale(&d_l_exp, e1[2])));

    // det_scaled = p0x * cofX - p0y * cofY + p0z * cofZ
    let term_x = expansion_mul_expansion(&p0x, &cof_x);
    let term_y = expansion_negate(&expansion_mul_expansion(&p0y, &cof_y));
    let term_z = expansion_mul_expansion(&p0z, &cof_z);

    let det_exp = expansion_add(&expansion_add(&term_x, &term_y), &term_z);

    let det_sign = expansion_sign(&det_exp);
    (d_l_sign * det_sign) as f64
}

/// True indirect orient3d for (LPI_a, LPI_b, Explicit, Explicit).
///
/// Substitutes two LPI homogeneous coords into the 4×4 orient3d determinant.
/// Multiply through by d_a * d_b to avoid division:
///
/// ```text
/// Row 0: (λax - e1x*da, λay - e1y*da, λaz - e1z*da)     [scaled by da]
/// Row 1: (λbx*da - λax*db, λby*da - λay*db, λbz*da - λaz*db)  [cross terms]
/// Row 2: (e2x - e1x, e2y - e1y, e2z - e1z) * da * db    [scaled]
/// ```
///
/// But more simply: translate so e1 is origin, scale rows by denominators.
///
/// Ref: Cherchi 2020 Section 4.2, orient3D_LLEE.
fn orient3d_llee(
    la: &ImplicitPoint,
    lb: &ImplicitPoint,
    e1: &ImplicitPoint,
    e2: &ImplicitPoint,
) -> f64 {
    let (q1a, q2a, ra, sa, ta) = match la {
        ImplicitPoint::LPI { q1, q2, r, s, t } => (q1, q2, r, s, t),
        _ => unreachable!(),
    };
    let (q1b, q2b, rb, sb, tb) = match lb {
        ImplicitPoint::LPI { q1, q2, r, s, t } => (q1, q2, r, s, t),
        _ => unreachable!(),
    };
    let e1c = match e1 {
        ImplicitPoint::Explicit(e) => e,
        _ => unreachable!(),
    };
    let e2c = match e2 {
        ImplicitPoint::Explicit(e) => e,
        _ => unreachable!(),
    };

    orient3d_llee_exact(q1a, q2a, ra, sa, ta, q1b, q2b, rb, sb, tb, e1c, e2c)
}

/// Exact orient3d_LLEE using expansion arithmetic.
///
/// orient3d(La, Lb, e1, e2) with La = (λax/da, λay/da, λaz/da),
///                                Lb = (λbx/db, λby/db, λbz/db)
///
/// Translate to e1 as origin. The 3×3 determinant (rows = La-e1, Lb-e1, e2-e1):
///   det_scaled = sign(da) * sign(db) * sign(
///     | λax-e1x*da  λay-e1y*da  λaz-e1z*da |
///     | λbx-e1x*db  λby-e1y*db  λbz-e1z*db |
///     | (e2x-e1x)*da*db  (e2y-e1y)*da*db  (e2z-e1z)*da*db |
///   )
///
/// Since da*db multiplies the entire third row, it factors out, and its sign
/// is already tracked by da_sign * db_sign. The determinant becomes:
///   | p_ax  p_ay  p_az |
///   | p_bx  p_by  p_bz |
///   | r2x   r2y   r2z  |
/// where p_ak = λak - e1k*da, p_bk = λbk - e1k*db, r2k = e2k - e1k.
#[allow(clippy::too_many_arguments)]
fn orient3d_llee_exact(
    q1a: &[f64; 3],
    q2a: &[f64; 3],
    ra: &[f64; 3],
    sa: &[f64; 3],
    ta: &[f64; 3],
    q1b: &[f64; 3],
    q2b: &[f64; 3],
    rb: &[f64; 3],
    sb: &[f64; 3],
    tb: &[f64; 3],
    e1: &[f64; 3],
    e2: &[f64; 3],
) -> f64 {
    let (da_sign, da_exp, lax, lay, laz) = match lpi_lambda_expansion(q1a, q2a, ra, sa, ta) {
        Some(v) => v,
        None => return 0.0,
    };
    let (db_sign, db_exp, lbx, lby, lbz) = match lpi_lambda_expansion(q1b, q2b, rb, sb, tb) {
        Some(v) => v,
        None => return 0.0,
    };

    // Row 0: p_a = (λa - e1 * da)
    let pax = expansion_add(&lax, &expansion_negate(&expansion_scale(&da_exp, e1[0])));
    let pay = expansion_add(&lay, &expansion_negate(&expansion_scale(&da_exp, e1[1])));
    let paz = expansion_add(&laz, &expansion_negate(&expansion_scale(&da_exp, e1[2])));

    // Row 1: p_b = (λb - e1 * db)
    let pbx = expansion_add(&lbx, &expansion_negate(&expansion_scale(&db_exp, e1[0])));
    let pby = expansion_add(&lby, &expansion_negate(&expansion_scale(&db_exp, e1[1])));
    let pbz = expansion_add(&lbz, &expansion_negate(&expansion_scale(&db_exp, e1[2])));

    // Row 2: r2 = (e2 - e1) (exact via two_diff)
    let r2x = two_diff_exp(e2[0], e1[0]);
    let r2y = two_diff_exp(e2[1], e1[1]);
    let r2z = two_diff_exp(e2[2], e1[2]);

    // 3×3 determinant via cofactor expansion along row 2:
    // det = r2x*(pay*pbz - paz*pby) - r2y*(pax*pbz - paz*pbx) + r2z*(pax*pby - pay*pbx)
    let m0 = expansion_add(
        &expansion_mul_expansion(&pay, &pbz),
        &expansion_negate(&expansion_mul_expansion(&paz, &pby)),
    );
    let m1 = expansion_add(
        &expansion_mul_expansion(&pax, &pbz),
        &expansion_negate(&expansion_mul_expansion(&paz, &pbx)),
    );
    let m2 = expansion_add(
        &expansion_mul_expansion(&pax, &pby),
        &expansion_negate(&expansion_mul_expansion(&pay, &pbx)),
    );

    let t0 = expansion_mul_expansion(&r2x, &m0);
    let t1 = expansion_negate(&expansion_mul_expansion(&r2y, &m1));
    let t2 = expansion_mul_expansion(&r2z, &m2);

    let det_exp = expansion_add(&expansion_add(&t0, &t1), &t2);

    let det_sign = expansion_sign(&det_exp);
    (da_sign * db_sign * det_sign) as f64
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

        // LL: two LPIs — true indirect comparison
        (
            ImplicitPoint::LPI {
                q1: q1a,
                q2: q2a,
                r: ra,
                s: sa,
                t: ta,
            },
            ImplicitPoint::LPI {
                q1: q1b,
                q2: q2b,
                r: rb,
                s: sb,
                t: tb,
            },
        ) => point_compare_ll(q1a, q2a, ra, sa, ta, q1b, q2b, rb, sb, tb, idx),

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

/// Lexicographic comparison of two implicit points (X, then Y, then Z).
///
/// Equivalent to Cherchi's `genericPoint::lessThan` — used for total
/// ordering of intersection points in sorting and deduplication.
///
/// Ref: Cherchi 2020 Section 4.3, genericPoint::lessThan.
pub(crate) fn less_than_indirect(a: &ImplicitPoint, b: &ImplicitPoint) -> std::cmp::Ordering {
    let x = point_compare_on_axis(a, b, Axis::X);
    if x != std::cmp::Ordering::Equal {
        return x;
    }
    let y = point_compare_on_axis(a, b, Axis::Y);
    if y != std::cmp::Ordering::Equal {
        return y;
    }
    point_compare_on_axis(a, b, Axis::Z)
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
///
/// Uses `lpi_lambda_expansion` with `two_diff` for all subtractions.
fn point_compare_le_exact(
    q1: &[f64; 3],
    q2: &[f64; 3],
    r: &[f64; 3],
    s: &[f64; 3],
    t: &[f64; 3],
    e: &[f64; 3],
    idx: usize,
) -> std::cmp::Ordering {
    let (d_l_sign, d_l_exp, lx, ly, lz) = match lpi_lambda_expansion(q1, q2, r, s, t) {
        Some(v) => v,
        None => return std::cmp::Ordering::Equal,
    };
    let lambda = [lx, ly, lz];

    // kx = λ[idx] - d_L * e[idx]
    let d_l_e = expansion_scale(&d_l_exp, e[idx]);
    let kx_exp = expansion_add(&lambda[idx], &expansion_negate(&d_l_e));

    let kx_sign = expansion_sign(&kx_exp);
    let combined = d_l_sign * kx_sign;
    match combined {
        x if x > 0 => std::cmp::Ordering::Greater,
        x if x < 0 => std::cmp::Ordering::Less,
        _ => std::cmp::Ordering::Equal,
    }
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

    /// Validates exact collinearity detection for large-coordinate LPI.
    /// Edge (q1→q2) crossing the plane x=4294967296. In ZX projection, the LPI's
    /// x-coordinate is exactly 4294967296, collinear with explicit points.
    /// Fixed by switching to det3x3_exact_pairs + lpi_lambda_expansion.
    #[test]
    fn test_orient2d_lee_exact_edge_crossing_collinear() {
        // LPI point from edge-edge crossing in three_cubes mesh arrangement.
        // The LPI is the intersection of a line with the plane x=4294967296.
        // In ZX projection, the LPI's x-coordinate is exactly 4294967296,
        // so it's collinear with the edge endpoints (which also have x=4294967296).
        let lpi = ImplicitPoint::LPI {
            q1: [376565552.644096, 70325794.504704, -436720864.591872],
            q2: [8966500144.644096, 70325794.504704, 8153213727.408128],
            r: [4294967296.0, -4294967296.0, 4294967296.0],
            s: [4294967296.0, -4294967296.0, -4294967296.0],
            t: [4294967296.0, 4294967296.0, 4294967296.0],
        };
        let ev0 = ImplicitPoint::Explicit([4294967296.0, -4294967296.0, -4294967296.0]);
        let ev1 = ImplicitPoint::Explicit([4294967296.0, -4294967296.0, 4294967296.0]);

        // EEL case: orient2d(ev0, ev1, lpi) in ZX projection
        let result = orient2d_indirect(&ev0, &ev1, &lpi, ProjectionAxis::ZX);

        // Verify with materialized coordinates
        let mat = lpi.materialize().unwrap();
        let mat_result = geometry_predicates::orient2d(
            [ev0.materialize().unwrap()[2], ev0.materialize().unwrap()[0]],
            [ev1.materialize().unwrap()[2], ev1.materialize().unwrap()[0]],
            [mat[2], mat[0]],
        );

        // The LPI's exact x-coordinate is exactly 4294967296 (intersection
        // of a line with the plane x=4294967296). All three points share
        // x=4294967296 in ZX projection, so orient2d must be 0.
        assert_eq!(
            result, 0.0,
            "orient2d_lee_exact must return 0 for collinear edge-crossing LPI, got {result}"
        );
    }

    // ── Oracle validation tests ──────────────────────────────────────
    // These test vectors were generated by running the LGPL C++ reference
    // implementation (MarcoAttene/Indirect_Predicates) on known inputs.
    // Our Rust code is NOT derived from the LGPL code — it implements
    // the same published mathematical formulas from Cherchi 2020.
    // These tests validate our implementation produces identical results.

    /// Validate LPI materialization matches C++ oracle.
    /// Oracle: edge (0,0,-1)→(0,0,1) crossing plane(1,0,0),(0,1,0),(-1,-1,0) → (0,0,0)
    #[test]
    fn test_oracle_lpi_materialize_origin() {
        let lpi = ImplicitPoint::LPI {
            q1: [0.0, 0.0, -1.0],
            q2: [0.0, 0.0, 1.0],
            r: [1.0, 0.0, 0.0],
            s: [0.0, 1.0, 0.0],
            t: [-1.0, -1.0, 0.0],
        };
        let m = lpi.materialize().expect("should materialize");
        assert!((m[0]).abs() < 1e-12, "x should be 0, got {}", m[0]);
        assert!((m[1]).abs() < 1e-12, "y should be 0, got {}", m[1]);
        assert!((m[2]).abs() < 1e-12, "z should be 0, got {}", m[2]);
    }

    /// Validate LPI materialization: edge (1,1,0)→(1,1,2) crossing z=1 plane → (1,1,1)
    #[test]
    fn test_oracle_lpi_materialize_unit() {
        let lpi = ImplicitPoint::LPI {
            q1: [1.0, 1.0, 0.0],
            q2: [1.0, 1.0, 2.0],
            r: [0.0, 0.0, 1.0],
            s: [10.0, 0.0, 1.0],
            t: [0.0, 10.0, 1.0],
        };
        let m = lpi.materialize().expect("should materialize");
        assert!((m[0] - 1.0).abs() < 1e-12, "x should be 1, got {}", m[0]);
        assert!((m[1] - 1.0).abs() < 1e-12, "y should be 1, got {}", m[1]);
        assert!((m[2] - 1.0).abs() < 1e-12, "z should be 1, got {}", m[2]);
    }

    /// Validate orient2d_LEE XY matches C++ oracle: LPI at origin + (1,0,0) + (0,1,0) → +1 (CCW)
    #[test]
    fn test_oracle_orient2d_lee_xy_origin() {
        let lpi = ImplicitPoint::LPI {
            q1: [0.0, 0.0, -1.0],
            q2: [0.0, 0.0, 1.0],
            r: [1.0, 0.0, 0.0],
            s: [0.0, 1.0, 0.0],
            t: [-1.0, -1.0, 0.0],
        };
        let e1 = ImplicitPoint::Explicit([1.0, 0.0, 0.0]);
        let e2 = ImplicitPoint::Explicit([0.0, 1.0, 0.0]);
        let result = orient2d_indirect(&lpi, &e1, &e2, ProjectionAxis::XY);
        // C++ oracle: xy=1
        assert!(
            result > 0.0,
            "orient2d_LEE XY should be positive (CCW), got {}",
            result
        );
    }

    /// Validate orient2d_LEE XY: LPI at (1,1,1) + (0.5,0.5,1) + (2,0.5,1) → +1
    #[test]
    fn test_oracle_orient2d_lee_xy_unit() {
        let lpi = ImplicitPoint::LPI {
            q1: [1.0, 1.0, 0.0],
            q2: [1.0, 1.0, 2.0],
            r: [0.0, 0.0, 1.0],
            s: [10.0, 0.0, 1.0],
            t: [0.0, 10.0, 1.0],
        };
        let e1 = ImplicitPoint::Explicit([0.5, 0.5, 1.0]);
        let e2 = ImplicitPoint::Explicit([2.0, 0.5, 1.0]);
        let result = orient2d_indirect(&lpi, &e1, &e2, ProjectionAxis::XY);
        // C++ oracle: xy=1
        assert!(
            result > 0.0,
            "orient2d_LEE XY should be positive, got {}",
            result
        );
    }

    /// Validate orient2d_LEE near-degenerate: LPI near-origin on XY, collinear with ±x → 0
    #[test]
    fn test_oracle_orient2d_lee_collinear() {
        let lpi = ImplicitPoint::LPI {
            q1: [0.0, 0.0, -1e-15],
            q2: [0.0, 0.0, 1e-15],
            r: [1.0, 0.0, 0.0],
            s: [0.0, 1.0, 0.0],
            t: [-1.0, -1.0, 0.0],
        };
        let e1 = ImplicitPoint::Explicit([1.0, 0.0, 0.0]);
        let e2 = ImplicitPoint::Explicit([-1.0, 0.0, 0.0]);
        let result = orient2d_indirect(&lpi, &e1, &e2, ProjectionAxis::XY);
        // C++ oracle: xy=0 (collinear)
        assert_eq!(
            result, 0.0,
            "orient2d_LEE XY should be 0 (collinear), got {}",
            result
        );
    }

    /// Validate lessThanOnX: LPI at origin vs (0.5,0.5,0.5) → -1 (LPI < explicit)
    #[test]
    fn test_oracle_less_than_le_origin() {
        let lpi = ImplicitPoint::LPI {
            q1: [0.0, 0.0, -1.0],
            q2: [0.0, 0.0, 1.0],
            r: [1.0, 0.0, 0.0],
            s: [0.0, 1.0, 0.0],
            t: [-1.0, -1.0, 0.0],
        };
        let e = ImplicitPoint::Explicit([0.5, 0.5, 0.5]);
        // C++ oracle: lx=-1 (LPI.x=0 < 0.5)
        let cmp = point_compare_on_axis(&lpi, &e, Axis::X);
        assert_eq!(
            cmp,
            std::cmp::Ordering::Less,
            "LPI at x=0 should be < explicit at x=0.5"
        );
    }

    /// Validate lessThan: LPI at (1,1,1) vs (1,1,1) → 0 (equal)
    #[test]
    fn test_oracle_less_than_le_equal() {
        let lpi = ImplicitPoint::LPI {
            q1: [1.0, 1.0, 0.0],
            q2: [1.0, 1.0, 2.0],
            r: [0.0, 0.0, 1.0],
            s: [10.0, 0.0, 1.0],
            t: [0.0, 10.0, 1.0],
        };
        let e = ImplicitPoint::Explicit([1.0, 1.0, 1.0]);
        // C++ oracle: lx=0, ly=0, lz=0, lt=0 (equal on all axes)
        assert_eq!(
            point_compare_on_axis(&lpi, &e, Axis::X),
            std::cmp::Ordering::Equal
        );
        assert_eq!(
            point_compare_on_axis(&lpi, &e, Axis::Y),
            std::cmp::Ordering::Equal
        );
        assert_eq!(
            point_compare_on_axis(&lpi, &e, Axis::Z),
            std::cmp::Ordering::Equal
        );
    }

    /// Validate orient2d_LEE: box-box intersection case (C++ oracle: xy=-1)
    #[test]
    fn test_oracle_orient2d_lee_box_intersection() {
        let lpi = ImplicitPoint::LPI {
            q1: [2.0, 0.0, -1.0],
            q2: [2.0, 0.0, 1.0],
            r: [1.0, 0.0, 0.0],
            s: [3.0, 0.0, 0.0],
            t: [3.0, 2.0, 0.0],
        };
        let e1 = ImplicitPoint::Explicit([1.0, 0.0, 0.0]);
        let e2 = ImplicitPoint::Explicit([3.0, 2.0, 0.0]);
        let result = orient2d_indirect(&lpi, &e1, &e2, ProjectionAxis::XY);
        // C++ oracle: xy=-1
        assert!(
            result < 0.0,
            "orient2d_LEE XY for box case should be negative, got {}",
            result
        );
    }

    // ── LLE oracle tests ────────────────────────────────────────────

    /// Validate orient2d_LLE: two LPIs at z-crossings + explicit point.
    #[test]
    fn test_oracle_orient2d_lle_two_z_crossings() {
        let r = [1.0, 0.0, 0.0];
        let s = [0.0, 1.0, 0.0];
        let t = [-1.0, -1.0, 0.0];
        let lpi1 = ImplicitPoint::LPI {
            q1: [0.0, 0.0, -1.0],
            q2: [0.0, 0.0, 1.0],
            r,
            s,
            t,
        };
        let lpi2 = ImplicitPoint::LPI {
            q1: [1.0, 0.0, -1.0],
            q2: [1.0, 0.0, 1.0],
            r,
            s,
            t,
        };
        let e = ImplicitPoint::Explicit([0.5, 1.0, 0.0]);
        let result = orient2d_indirect(&lpi1, &lpi2, &e, ProjectionAxis::XY);
        assert!(
            result > 0.0,
            "LLE xy should be positive (oracle=1), got {}",
            result
        );
    }

    /// Validate orient2d_LLE agrees with materialized orient2d.
    #[test]
    fn test_oracle_orient2d_lle_matches_materialized() {
        let r = [1.0, 0.0, 0.0];
        let s = [0.0, 1.0, 0.0];
        let t = [-1.0, -1.0, 0.0];
        let lpi1 = ImplicitPoint::LPI {
            q1: [0.3, 0.2, -1.0],
            q2: [0.3, 0.2, 1.0],
            r,
            s,
            t,
        };
        let lpi2 = ImplicitPoint::LPI {
            q1: [0.7, 0.1, -1.0],
            q2: [0.7, 0.1, 1.0],
            r,
            s,
            t,
        };
        let e = ImplicitPoint::Explicit([0.5, 0.5, 0.0]);

        let indirect = orient2d_indirect(&lpi1, &lpi2, &e, ProjectionAxis::XY);
        let m1 = lpi1.materialize().unwrap();
        let m2 = lpi2.materialize().unwrap();
        let direct = geometry_predicates::orient2d([m1[0], m1[1]], [m2[0], m2[1]], [0.5, 0.5]);
        assert_eq!(
            indirect.signum(),
            direct.signum(),
            "LLE indirect={indirect} vs materialized={direct}"
        );
    }

    /// Validate orient2d_LLE permutations via antisymmetry.
    #[test]
    fn test_oracle_orient2d_lle_permutations() {
        let r = [1.0, 0.0, 0.0];
        let s = [0.0, 1.0, 0.0];
        let t = [-1.0, -1.0, 0.0];
        let l1 = ImplicitPoint::LPI {
            q1: [0.0, 0.0, -1.0],
            q2: [0.0, 0.0, 1.0],
            r,
            s,
            t,
        };
        let l2 = ImplicitPoint::LPI {
            q1: [1.0, 0.0, -1.0],
            q2: [1.0, 0.0, 1.0],
            r,
            s,
            t,
        };
        let e = ImplicitPoint::Explicit([0.5, 1.0, 0.0]);

        let lle = orient2d_indirect(&l1, &l2, &e, ProjectionAxis::XY);
        let lel = orient2d_indirect(&l1, &e, &l2, ProjectionAxis::XY);
        let ell = orient2d_indirect(&e, &l1, &l2, ProjectionAxis::XY);

        // swap b,c → negate
        assert_eq!(
            lle.signum(),
            -lel.signum(),
            "LLE and LEL should have opposite signs: lle={lle}, lel={lel}"
        );
        // cyclic permutation preserves sign
        assert_eq!(
            lle.signum(),
            ell.signum(),
            "LLE and ELL should have same sign: lle={lle}, ell={ell}"
        );
    }

    // ── LLL oracle tests ────────────────────────────────────────────

    /// Validate orient2d_LLL: three LPIs at z-crossings.
    #[test]
    fn test_oracle_orient2d_lll_three_z_crossings() {
        let r = [1.0, 0.0, 0.0];
        let s = [0.0, 1.0, 0.0];
        let t = [-1.0, -1.0, 0.0];
        let lpi1 = ImplicitPoint::LPI {
            q1: [0.0, 0.0, -1.0],
            q2: [0.0, 0.0, 1.0],
            r,
            s,
            t,
        };
        let lpi2 = ImplicitPoint::LPI {
            q1: [1.0, 0.0, -1.0],
            q2: [1.0, 0.0, 1.0],
            r,
            s,
            t,
        };
        let lpi3 = ImplicitPoint::LPI {
            q1: [0.0, 1.0, -1.0],
            q2: [0.0, 1.0, 1.0],
            r,
            s,
            t,
        };
        let result = orient2d_indirect(&lpi1, &lpi2, &lpi3, ProjectionAxis::XY);
        assert!(
            result > 0.0,
            "LLL xy should be positive (oracle=1), got {}",
            result
        );
    }

    /// Validate orient2d_LLL: collinear case → 0.
    #[test]
    fn test_oracle_orient2d_lll_collinear() {
        let r = [1.0, 0.0, 0.0];
        let s = [0.0, 1.0, 0.0];
        let t = [-1.0, -1.0, 0.0];
        let lpi1 = ImplicitPoint::LPI {
            q1: [0.0, 0.0, -1.0],
            q2: [0.0, 0.0, 1.0],
            r,
            s,
            t,
        };
        let lpi2 = ImplicitPoint::LPI {
            q1: [2.0, 0.0, -1.0],
            q2: [2.0, 0.0, 1.0],
            r,
            s,
            t,
        };
        let lpi3 = ImplicitPoint::LPI {
            q1: [4.0, 0.0, -1.0],
            q2: [4.0, 0.0, 1.0],
            r,
            s,
            t,
        };
        let result = orient2d_indirect(&lpi1, &lpi2, &lpi3, ProjectionAxis::XY);
        assert_eq!(result, 0.0, "LLL collinear xy should be 0, got {}", result);
    }

    /// Validate orient2d_LLL matches materialized.
    #[test]
    fn test_oracle_orient2d_lll_matches_materialized() {
        let r = [1.0, 0.0, 0.0];
        let s = [0.0, 1.0, 0.0];
        let t = [-1.0, -1.0, 0.0];
        let lpi1 = ImplicitPoint::LPI {
            q1: [0.3, 0.2, -1.0],
            q2: [0.3, 0.2, 1.0],
            r,
            s,
            t,
        };
        let lpi2 = ImplicitPoint::LPI {
            q1: [0.7, 0.1, -1.0],
            q2: [0.7, 0.1, 1.0],
            r,
            s,
            t,
        };
        let lpi3 = ImplicitPoint::LPI {
            q1: [0.1, 0.8, -1.0],
            q2: [0.1, 0.8, 1.0],
            r,
            s,
            t,
        };

        let indirect = orient2d_indirect(&lpi1, &lpi2, &lpi3, ProjectionAxis::XY);
        let m1 = lpi1.materialize().unwrap();
        let m2 = lpi2.materialize().unwrap();
        let m3 = lpi3.materialize().unwrap();
        let direct = geometry_predicates::orient2d([m1[0], m1[1]], [m2[0], m2[1]], [m3[0], m3[1]]);
        assert_eq!(
            indirect.signum(),
            direct.signum(),
            "LLL indirect={indirect} vs materialized={direct}"
        );
    }

    // ── LL point_compare oracle tests ───────────────────────────────

    /// Validate point_compare_LL: LPI at origin vs LPI at (1,0,0).
    #[test]
    fn test_oracle_point_compare_ll() {
        let r = [1.0, 0.0, 0.0];
        let s = [0.0, 1.0, 0.0];
        let t = [-1.0, -1.0, 0.0];
        let lpi1 = ImplicitPoint::LPI {
            q1: [0.0, 0.0, -1.0],
            q2: [0.0, 0.0, 1.0],
            r,
            s,
            t,
        };
        let lpi2 = ImplicitPoint::LPI {
            q1: [1.0, 0.0, -1.0],
            q2: [1.0, 0.0, 1.0],
            r,
            s,
            t,
        };
        assert_eq!(
            point_compare_on_axis(&lpi1, &lpi2, Axis::X),
            std::cmp::Ordering::Less
        );
        assert_eq!(
            point_compare_on_axis(&lpi1, &lpi2, Axis::Y),
            std::cmp::Ordering::Equal
        );
    }

    /// Validate point_compare_LL: same geometric point from different edges.
    #[test]
    fn test_oracle_point_compare_ll_same_point() {
        let r = [1.0, 0.0, 0.0];
        let s = [0.0, 1.0, 0.0];
        let t = [-1.0, -1.0, 0.0];
        let lpi1 = ImplicitPoint::LPI {
            q1: [0.0, 0.0, -1.0],
            q2: [0.0, 0.0, 1.0],
            r,
            s,
            t,
        };
        let lpi2 = ImplicitPoint::LPI {
            q1: [0.0, 0.0, -2.0],
            q2: [0.0, 0.0, 2.0],
            r,
            s,
            t,
        };
        assert_eq!(
            point_compare_on_axis(&lpi1, &lpi2, Axis::X),
            std::cmp::Ordering::Equal
        );
        assert_eq!(
            point_compare_on_axis(&lpi1, &lpi2, Axis::Y),
            std::cmp::Ordering::Equal
        );
        assert_eq!(
            point_compare_on_axis(&lpi1, &lpi2, Axis::Z),
            std::cmp::Ordering::Equal
        );
    }

    /// Validate point_compare_LL: sorting three LPIs by X.
    #[test]
    fn test_oracle_point_compare_ll_sorting() {
        let r = [1.0, 0.0, 0.0];
        let s = [0.0, 1.0, 0.0];
        let t = [-1.0, -1.0, 0.0];
        let lpi_x0 = ImplicitPoint::LPI {
            q1: [0.0, 0.0, -1.0],
            q2: [0.0, 0.0, 1.0],
            r,
            s,
            t,
        };
        let lpi_x1 = ImplicitPoint::LPI {
            q1: [1.0, 0.0, -1.0],
            q2: [1.0, 0.0, 1.0],
            r,
            s,
            t,
        };
        let lpi_x3 = ImplicitPoint::LPI {
            q1: [3.0, 0.0, -1.0],
            q2: [3.0, 0.0, 1.0],
            r,
            s,
            t,
        };
        let mut points = vec![lpi_x3.clone(), lpi_x0.clone(), lpi_x1.clone()];
        points.sort_by(|a, b| point_compare_on_axis(a, b, Axis::X));
        // After sort: x0, x1, x3
        assert_eq!(
            point_compare_on_axis(&points[0], &lpi_x0, Axis::X),
            std::cmp::Ordering::Equal
        );
        assert_eq!(
            point_compare_on_axis(&points[1], &lpi_x1, Axis::X),
            std::cmp::Ordering::Equal
        );
        assert_eq!(
            point_compare_on_axis(&points[2], &lpi_x3, Axis::X),
            std::cmp::Ordering::Equal
        );
    }

    // ── Orient3d indirect oracle tests ──────────────────────────────

    /// Oracle: orient3d_LEEE with LPI at origin of standard tet.
    /// LPI: edge along Z axis through origin, plane z=0 → intersection at origin.
    /// Validates against Shewchuk orient3d on materialized coordinates.
    #[test]
    fn test_oracle_orient3d_leee_origin_tet() {
        let lpi = ImplicitPoint::LPI {
            q1: [0.0, 0.0, -1.0],
            q2: [0.0, 0.0, 1.0],
            r: [1.0, 0.0, 0.0],
            s: [0.0, 1.0, 0.0],
            t: [-1.0, -1.0, 0.0],
        };
        let e1 = ImplicitPoint::Explicit([1.0, 0.0, 0.0]);
        let e2 = ImplicitPoint::Explicit([0.0, 1.0, 0.0]);
        let e3 = ImplicitPoint::Explicit([0.0, 0.0, 1.0]);
        let result = orient3d_indirect(&lpi, &e1, &e2, &e3);
        let expected = geometry_predicates::orient3d(
            lpi.materialize().unwrap(),
            [1.0, 0.0, 0.0],
            [0.0, 1.0, 0.0],
            [0.0, 0.0, 1.0],
        );
        assert_ne!(
            result, 0.0,
            "orient3d_LEEE origin_tet: should not be coplanar"
        );
        assert_eq!(
            result.signum(),
            expected.signum(),
            "orient3d_LEEE origin_tet: indirect={}, shewchuk={}",
            result,
            expected
        );
    }

    /// Oracle: orient3d_LEEE with unit tet reversed winding — sign should negate.
    #[test]
    fn test_oracle_orient3d_leee_unit_tet() {
        let lpi = ImplicitPoint::LPI {
            q1: [0.0, 0.0, -1.0],
            q2: [0.0, 0.0, 1.0],
            r: [1.0, 0.0, 0.0],
            s: [0.0, 1.0, 0.0],
            t: [-1.0, -1.0, 0.0],
        };
        let e1 = ImplicitPoint::Explicit([0.0, 1.0, 0.0]);
        let e2 = ImplicitPoint::Explicit([1.0, 0.0, 0.0]);
        let e3 = ImplicitPoint::Explicit([0.0, 0.0, 1.0]);
        let result = orient3d_indirect(&lpi, &e1, &e2, &e3);
        let expected = geometry_predicates::orient3d(
            lpi.materialize().unwrap(),
            [0.0, 1.0, 0.0],
            [1.0, 0.0, 0.0],
            [0.0, 0.0, 1.0],
        );
        assert_ne!(
            result, 0.0,
            "orient3d_LEEE unit_tet: should not be coplanar"
        );
        assert_eq!(
            result.signum(),
            expected.signum(),
            "orient3d_LEEE unit_tet: indirect={}, shewchuk={}",
            result,
            expected
        );
    }

    /// Oracle: orient3d_LEEE coplanar case.
    /// LPI at origin, three explicit points in z=0 plane → coplanar (0).
    #[test]
    fn test_oracle_orient3d_leee_coplanar() {
        let lpi = ImplicitPoint::LPI {
            q1: [0.0, 0.0, -1.0],
            q2: [0.0, 0.0, 1.0],
            r: [1.0, 0.0, 0.0],
            s: [0.0, 1.0, 0.0],
            t: [-1.0, -1.0, 0.0],
        };
        let e1 = ImplicitPoint::Explicit([1.0, 0.0, 0.0]);
        let e2 = ImplicitPoint::Explicit([0.0, 1.0, 0.0]);
        let e3 = ImplicitPoint::Explicit([-1.0, -1.0, 0.0]);
        let result = orient3d_indirect(&lpi, &e1, &e2, &e3);
        assert_eq!(
            result, 0.0,
            "orient3d_LEEE coplanar: expected 0, got {}",
            result
        );
    }

    /// Oracle: orient3d_LLEE with two LPI points.
    /// LPI_a at origin (z-axis ∩ z=0), LPI_b at (0.5, 0.5, 0) (diagonal edge ∩ z=0).
    /// orient3d(La, Lb, (0,0,1), (0,0,-1)) — should give well-defined sign.
    #[test]
    fn test_oracle_orient3d_llee_two_lpi() {
        // LPI_a: origin (edge along z, plane z=0)
        let lpi_a = ImplicitPoint::LPI {
            q1: [0.0, 0.0, -1.0],
            q2: [0.0, 0.0, 1.0],
            r: [1.0, 0.0, 0.0],
            s: [0.0, 1.0, 0.0],
            t: [-1.0, -1.0, 0.0],
        };
        // LPI_b: edge from (1,0,-1) to (0,1,1) crossing plane z=0
        // This edge crosses z=0 at (0.5, 0.5, 0)
        let lpi_b = ImplicitPoint::LPI {
            q1: [1.0, 0.0, -1.0],
            q2: [0.0, 1.0, 1.0],
            r: [1.0, 0.0, 0.0],
            s: [0.0, 1.0, 0.0],
            t: [-1.0, -1.0, 0.0],
        };
        let e1 = ImplicitPoint::Explicit([0.0, 0.0, 1.0]);
        let e2 = ImplicitPoint::Explicit([0.0, 0.0, -1.0]);
        let result = orient3d_indirect(&lpi_a, &lpi_b, &e1, &e2);
        // Verify against materialized orient3d
        let ma = lpi_a.materialize().unwrap();
        let mb = lpi_b.materialize().unwrap();
        let expected = geometry_predicates::orient3d(ma, mb, [0.0, 0.0, 1.0], [0.0, 0.0, -1.0]);
        assert!(
            (result > 0.0) == (expected > 0.0)
                && (result < 0.0) == (expected < 0.0)
                && (result == 0.0) == (expected == 0.0),
            "orient3d_LLEE two_lpi: indirect={}, materialized={}",
            result,
            expected
        );
    }

    /// Oracle: orient3d_indirect matches Shewchuk for all-Explicit inputs.
    #[test]
    fn test_orient3d_eeee_matches_shewchuk() {
        let a = ImplicitPoint::Explicit([0.0, 0.0, 0.0]);
        let b = ImplicitPoint::Explicit([1.0, 0.0, 0.0]);
        let c = ImplicitPoint::Explicit([0.0, 1.0, 0.0]);
        let d = ImplicitPoint::Explicit([0.0, 0.0, 1.0]);
        let result = orient3d_indirect(&a, &b, &c, &d);
        let expected = geometry_predicates::orient3d(
            [0.0, 0.0, 0.0],
            [1.0, 0.0, 0.0],
            [0.0, 1.0, 0.0],
            [0.0, 0.0, 1.0],
        );
        assert_eq!(result.signum(), expected.signum());
    }

    /// Oracle: orient3d_LEEE permutation consistency (antisymmetry).
    /// Swapping two points should negate the result.
    #[test]
    fn test_orient3d_leee_antisymmetry() {
        let lpi = ImplicitPoint::LPI {
            q1: [0.0, 0.0, -1.0],
            q2: [0.0, 0.0, 1.0],
            r: [1.0, 0.0, 0.0],
            s: [0.0, 1.0, 0.0],
            t: [-1.0, -1.0, 0.0],
        };
        let e1 = ImplicitPoint::Explicit([1.0, 0.0, 0.0]);
        let e2 = ImplicitPoint::Explicit([0.0, 1.0, 0.0]);
        let e3 = ImplicitPoint::Explicit([0.0, 0.0, 1.0]);
        // orient3d(L, e1, e2, e3) should negate when we swap e1 and e2
        let fwd = orient3d_indirect(&lpi, &e1, &e2, &e3);
        let rev = orient3d_indirect(&lpi, &e2, &e1, &e3);
        assert_eq!(
            fwd.signum(),
            -rev.signum(),
            "antisymmetry: fwd={}, rev={}",
            fwd,
            rev
        );
    }

    // ── lessThan_indirect oracle tests ──────────────────────────────

    /// Oracle: lessThan_LL_full — lexicographic comparison of two LPIs.
    #[test]
    fn test_oracle_less_than_ll_full() {
        let r = [1.0, 0.0, 0.0];
        let s = [0.0, 1.0, 0.0];
        let t = [-1.0, -1.0, 0.0];
        // LPI at origin
        let lpi_origin = ImplicitPoint::LPI {
            q1: [0.0, 0.0, -1.0],
            q2: [0.0, 0.0, 1.0],
            r,
            s,
            t,
        };
        // LPI at (1, 0, 0)
        let lpi_x1 = ImplicitPoint::LPI {
            q1: [1.0, 0.0, -1.0],
            q2: [1.0, 0.0, 1.0],
            r,
            s,
            t,
        };
        // origin < (1,0,0) lexicographically
        assert_eq!(
            less_than_indirect(&lpi_origin, &lpi_x1),
            std::cmp::Ordering::Less,
            "origin should be less than (1,0,0)"
        );
        assert_eq!(
            less_than_indirect(&lpi_x1, &lpi_origin),
            std::cmp::Ordering::Greater,
        );
        assert_eq!(
            less_than_indirect(&lpi_origin, &lpi_origin),
            std::cmp::Ordering::Equal,
        );
    }
}
