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

/// Compute TPI lambda parameters using full expansion arithmetic.
///
/// Returns `(d_T_sign, d_T_expansion, λ_Tx, λ_Ty, λ_Tz)` or `None` if the
/// three planes are not linearly independent (`d_T_sign == 0`).
///
/// Cherchi 2020 §4.1 — Type T (TPI) point representation from 9 explicit
/// vertices defining three triangles.
///
/// ```text
/// nv = (v2-v1) × (v3-v2),  nw = (w2-w1) × (w3-w2),  nu = (u2-u1) × (u3-u2)
/// d_T = det |nv; nw; nu|
/// pv = nv · v1,  pw = nw · w1,  pu = nu · u1
/// λ_Tx = det |pv nvy nvz; pw nwy nwz; pu nuy nuz|     (Cramer column 0)
/// λ_Ty = det |nvx pv nvz; nwx pw nwz; nux pu nuz|     (Cramer column 1)
/// λ_Tz = det |nvx nvy pv; nwx nwy pw; nux nuy pu|     (Cramer column 2)
/// ```
///
/// All input subtractions use `two_diff` (exact). Cross products use
/// `two_product` + `two_two_diff` (exact). All multiplications and
/// additions use Shewchuk expansion arithmetic.
///
/// Ref: Cherchi 2020 §4.1 (eq. for pT); Shewchuk 1997 [#4] for expansion algebra.
#[allow(clippy::too_many_arguments)]
#[allow(clippy::type_complexity)]
#[allow(dead_code)] // Used by orient2d/orient3d/pointCompare TPI variants (Phases B–D).
fn tpi_lambda_expansion(
    v1: &[f64; 3],
    v2: &[f64; 3],
    v3: &[f64; 3],
    w1: &[f64; 3],
    w2: &[f64; 3],
    w3: &[f64; 3],
    u1: &[f64; 3],
    u2: &[f64; 3],
    u3: &[f64; 3],
) -> Option<(i32, Vec<f64>, Vec<f64>, Vec<f64>, Vec<f64>)> {
    // Triangle normals as exact expansion vectors (one expansion per coord).
    let nv = cross_sub_expansion(v1, v2, v3);
    let nw = cross_sub_expansion(w1, w2, w3);
    let nu = cross_sub_expansion(u1, u2, u3);

    // d_T = det|nv; nw; nu| with rows of expansions.
    let d_t_exp = det3x3_expansion(&nv, &nw, &nu);
    let d_t_sign = expansion_sign(&d_t_exp);
    if d_t_sign == 0 {
        return None;
    }

    // pv = nv · v1,  pw = nw · w1,  pu = nu · u1 (each an expansion).
    let pv = dot_exp_with_explicit(&nv, v1);
    let pw = dot_exp_with_explicit(&nw, w1);
    let pu = dot_exp_with_explicit(&nu, u1);

    // λ_Tx = det |pv nv[1] nv[2]; pw nw[1] nw[2]; pu nu[1] nu[2]|
    let lambda_x = det3x3_expansion(
        &[pv.clone(), nv[1].clone(), nv[2].clone()],
        &[pw.clone(), nw[1].clone(), nw[2].clone()],
        &[pu.clone(), nu[1].clone(), nu[2].clone()],
    );
    // λ_Ty = det |nv[0] pv nv[2]; nw[0] pw nw[2]; nu[0] pu nu[2]|
    let lambda_y = det3x3_expansion(
        &[nv[0].clone(), pv.clone(), nv[2].clone()],
        &[nw[0].clone(), pw.clone(), nw[2].clone()],
        &[nu[0].clone(), pu.clone(), nu[2].clone()],
    );
    // λ_Tz = det |nv[0] nv[1] pv; nw[0] nw[1] pw; nu[0] nu[1] pu|
    let lambda_z = det3x3_expansion(
        &[nv[0].clone(), nv[1].clone(), pv],
        &[nw[0].clone(), nw[1].clone(), pw],
        &[nu[0].clone(), nu[1].clone(), pu],
    );

    Some((d_t_sign, d_t_exp, lambda_x, lambda_y, lambda_z))
}

/// Compute (b-a) × (c-b) as a 3-component expansion vector.
/// Each component is an exact Shewchuk expansion built from the input f64s
/// via `two_diff` + `two_product` + `two_two_diff`.
#[allow(dead_code)] // Used by tpi_lambda_expansion (consumed in Phases B–D).
fn cross_sub_expansion(a: &[f64; 3], b: &[f64; 3], c: &[f64; 3]) -> [Vec<f64>; 3] {
    // ab = b - a, bc = c - b — each component a 2-component expansion.
    let ab_x = two_diff_exp(b[0], a[0]);
    let ab_y = two_diff_exp(b[1], a[1]);
    let ab_z = two_diff_exp(b[2], a[2]);
    let bc_x = two_diff_exp(c[0], b[0]);
    let bc_y = two_diff_exp(c[1], b[1]);
    let bc_z = two_diff_exp(c[2], b[2]);

    // cross = (ab_y*bc_z - ab_z*bc_y, ab_z*bc_x - ab_x*bc_z, ab_x*bc_y - ab_y*bc_x)
    let cx = expansion_add(
        &expansion_mul_expansion(&ab_y, &bc_z),
        &expansion_negate(&expansion_mul_expansion(&ab_z, &bc_y)),
    );
    let cy = expansion_add(
        &expansion_mul_expansion(&ab_z, &bc_x),
        &expansion_negate(&expansion_mul_expansion(&ab_x, &bc_z)),
    );
    let cz = expansion_add(
        &expansion_mul_expansion(&ab_x, &bc_y),
        &expansion_negate(&expansion_mul_expansion(&ab_y, &bc_x)),
    );
    [cx, cy, cz]
}

/// Dot product of an expansion-row vector with an explicit f64 vector.
/// Returns `r[0]*v[0] + r[1]*v[1] + r[2]*v[2]` as an exact expansion.
#[allow(dead_code)] // Used by tpi_lambda_expansion (consumed in Phases B–D).
fn dot_exp_with_explicit(r: &[Vec<f64>; 3], v: &[f64; 3]) -> Vec<f64> {
    let t0 = expansion_scale(&r[0], v[0]);
    let t1 = expansion_scale(&r[1], v[1]);
    let t2 = expansion_scale(&r[2], v[2]);
    expansion_add(&expansion_add(&t0, &t1), &t2)
}

/// Compute exact 3×3 determinant where each row is a 3-component expansion vector.
///
/// `det = r0[0]*(r1[1]*r2[2] - r1[2]*r2[1])
///      - r0[1]*(r1[0]*r2[2] - r1[2]*r2[0])
///      + r0[2]*(r1[0]*r2[1] - r1[1]*r2[0])`
#[allow(dead_code)] // Used by tpi_lambda_expansion (consumed in Phases B–D).
fn det3x3_expansion(r0: &[Vec<f64>; 3], r1: &[Vec<f64>; 3], r2: &[Vec<f64>; 3]) -> Vec<f64> {
    let m0 = expansion_add(
        &expansion_mul_expansion(&r1[1], &r2[2]),
        &expansion_negate(&expansion_mul_expansion(&r1[2], &r2[1])),
    );
    let m1 = expansion_add(
        &expansion_mul_expansion(&r1[0], &r2[2]),
        &expansion_negate(&expansion_mul_expansion(&r1[2], &r2[0])),
    );
    let m2 = expansion_add(
        &expansion_mul_expansion(&r1[0], &r2[1]),
        &expansion_negate(&expansion_mul_expansion(&r1[1], &r2[0])),
    );
    let t0 = expansion_mul_expansion(&r0[0], &m0);
    let t1 = expansion_negate(&expansion_mul_expansion(&r0[1], &m1));
    let t2 = expansion_mul_expansion(&r0[2], &m2);
    expansion_add(&expansion_add(&t0, &t1), &t2)
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

// ── True indirect orient2d: TPI variants (Phase B) ──────────────────────
//
// The 6 TPI-bearing orient2d variants substitute `tpi_lambda_expansion`
// into the same homogeneous-coordinate framework used for LPI variants.
//
// Common derivation (Cherchi 2020 §4.4 — generalized for any pair of
// homogeneous points (α/A), (β/B), (γ/C)):
//
//     sign(orient2d(a,b,c)) = sign(A) * sign(B) * sign(det)
//     det = (α_i*C - γ_i*A)(β_j*C - γ_j*B) - (α_j*C - γ_j*A)(β_i*C - γ_i*B)
//
// because the C² factor that appears after expansion is always positive.
//
// For the single-implicit case (TEE) the cofactor form mirrors LEE:
//
//     sign(orient2d(T,e1,e2)) = sign(d_T) * sign(det)
//     det = λ_i*(e1_j-e2_j) + λ_j*(e2_i-e1_i) + d_T*(e1_i*e2_j - e1_j*e2_i)

/// Stub dispatcher — TPI, Explicit, Explicit. Mirrors `orient2d_lee`.
///
/// TEE filter: 9.06e-13 * δ⁸ (Cherchi 2020 Table 1, degree 8). Float filter
/// is deferred to a follow-up PR; always uses exact expansion arithmetic.
#[allow(clippy::too_many_arguments)]
#[allow(dead_code)]
fn orient2d_tee(
    v1: &[f64; 3],
    v2: &[f64; 3],
    v3: &[f64; 3],
    w1: &[f64; 3],
    w2: &[f64; 3],
    w3: &[f64; 3],
    u1: &[f64; 3],
    u2: &[f64; 3],
    u3: &[f64; 3],
    e1: &[f64; 3],
    e2: &[f64; 3],
    i: usize,
    j: usize,
) -> f64 {
    orient2d_tee_exact(v1, v2, v3, w1, w2, w3, u1, u2, u3, e1, e2, i, j)
}

/// Exact orient2d for (TPI, Explicit, Explicit).
///
/// Substitutes TPI homogeneous coords into the LEE cofactor form:
///   det = λ_i*(e1[j]-e2[j]) + λ_j*(e2[i]-e1[i]) + d_T*(e1[i]*e2[j]-e1[j]*e2[i])
///   sign(orient2d) = sign(d_T) * sign(det)
///
/// Ref: Cherchi 2020 §4.2, Table 1 row TEE (filter 9.06e-13 * δ⁸).
#[allow(clippy::too_many_arguments)]
fn orient2d_tee_exact(
    v1: &[f64; 3],
    v2: &[f64; 3],
    v3: &[f64; 3],
    w1: &[f64; 3],
    w2: &[f64; 3],
    w3: &[f64; 3],
    u1: &[f64; 3],
    u2: &[f64; 3],
    u3: &[f64; 3],
    e1: &[f64; 3],
    e2: &[f64; 3],
    i: usize,
    j: usize,
) -> f64 {
    let (d_t_sign, d_t_exp, lx, ly, lz) =
        match tpi_lambda_expansion(v1, v2, v3, w1, w2, w3, u1, u2, u3) {
            Some(v) => v,
            None => return 0.0,
        };
    let lambda = [lx, ly, lz];

    // det = λ_i*(e1[j]-e2[j]) + λ_j*(e2[i]-e1[i]) + d_T*(e1[i]*e2[j]-e1[j]*e2[i])
    let term1 = expansion_mul_expansion(&lambda[i], &two_diff_exp(e1[j], e2[j]));
    let term2 = expansion_mul_expansion(&lambda[j], &two_diff_exp(e2[i], e1[i]));

    // e1[i]*e2[j] - e1[j]*e2[i] — exact cross product
    let [pr_lo, pr_hi] = gp::two_product(e1[i], e2[j]);
    let [nr_lo, nr_hi] = gp::two_product(e1[j], e2[i]);
    let cross_e = gp::two_two_diff(pr_hi, pr_lo, nr_hi, nr_lo);
    let term3 = expansion_mul_expansion(&d_t_exp, &cross_e);

    let sum12 = expansion_add(&term1, &term2);
    let det_exp = expansion_add(&sum12, &term3);
    let det_sign = expansion_sign(&det_exp);
    (det_sign * d_t_sign) as f64
}

/// Stub dispatcher — LPI, TPI, Explicit. Mirrors `orient2d_lle` pattern.
///
/// LTE filter: 2.18e-10 * δ¹⁴ (Cherchi 2020 Table 1, degree 14). Float
/// filter is deferred to a follow-up PR; always uses exact expansion arithmetic.
#[allow(dead_code)]
fn orient2d_lte(
    la: &ImplicitPoint,
    tb: &ImplicitPoint,
    ec: &ImplicitPoint,
    i: usize,
    j: usize,
) -> f64 {
    let (q1a, q2a, ra, sa, ta) = match la {
        ImplicitPoint::LPI { q1, q2, r, s, t } => (q1, q2, r, s, t),
        _ => unreachable!("orient2d_lte: arg 0 must be LPI"),
    };
    let (v1, v2, v3, w1, w2, w3, u1, u2, u3) = match tb {
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
        } => (v1, v2, v3, w1, w2, w3, u1, u2, u3),
        _ => unreachable!("orient2d_lte: arg 1 must be TPI"),
    };
    let e = match ec {
        ImplicitPoint::Explicit(e) => e,
        _ => unreachable!("orient2d_lte: arg 2 must be Explicit"),
    };
    orient2d_lte_exact(
        q1a, q2a, ra, sa, ta, v1, v2, v3, w1, w2, w3, u1, u2, u3, e, i, j,
    )
}

/// Exact orient2d for (LPI, TPI, Explicit).
///
/// Substitutes LPI and TPI homogeneous coords; explicit point is C with C=1.
///   det = (α_i*C - γ_i*A)(β_j*C - γ_j*B) - (α_j*C - γ_j*A)(β_i*C - γ_i*B)
///   with C=1, γ=e, A=d_L, α=λ_L, B=d_T, β=λ_T
///   sign(orient2d) = sign(d_L) * sign(d_T) * sign(det)
///
/// Ref: Cherchi 2020 §4.2, Table 1 row LTE (filter 2.18e-10 * δ¹⁴).
#[allow(clippy::too_many_arguments)]
fn orient2d_lte_exact(
    q1a: &[f64; 3],
    q2a: &[f64; 3],
    ra: &[f64; 3],
    sa: &[f64; 3],
    ta: &[f64; 3],
    v1: &[f64; 3],
    v2: &[f64; 3],
    v3: &[f64; 3],
    w1: &[f64; 3],
    w2: &[f64; 3],
    w3: &[f64; 3],
    u1: &[f64; 3],
    u2: &[f64; 3],
    u3: &[f64; 3],
    e: &[f64; 3],
    i: usize,
    j: usize,
) -> f64 {
    let (da_sign, da_exp, lax, lay, laz) = match lpi_lambda_expansion(q1a, q2a, ra, sa, ta) {
        Some(v) => v,
        None => return 0.0,
    };
    let (db_sign, db_exp, lbx, lby, lbz) =
        match tpi_lambda_expansion(v1, v2, v3, w1, w2, w3, u1, u2, u3) {
            Some(v) => v,
            None => return 0.0,
        };
    let la = [lax, lay, laz];
    let lb = [lbx, lby, lbz];

    // p1 = α_i - γ_i*A = λa[i] - e[i]*da
    let p1 = expansion_add(&la[i], &expansion_negate(&expansion_scale(&da_exp, e[i])));
    // p2 = β_j - γ_j*B = λb[j] - e[j]*db
    let p2 = expansion_add(&lb[j], &expansion_negate(&expansion_scale(&db_exp, e[j])));
    // p3 = α_j - γ_j*A = λa[j] - e[j]*da
    let p3 = expansion_add(&la[j], &expansion_negate(&expansion_scale(&da_exp, e[j])));
    // p4 = β_i - γ_i*B = λb[i] - e[i]*db
    let p4 = expansion_add(&lb[i], &expansion_negate(&expansion_scale(&db_exp, e[i])));

    let term1 = expansion_mul_expansion(&p1, &p2);
    let term2 = expansion_mul_expansion(&p3, &p4);
    let det_exp = expansion_add(&term1, &expansion_negate(&term2));
    let det_sign = expansion_sign(&det_exp);
    (da_sign * db_sign * det_sign) as f64
}

/// Stub dispatcher — LPI, LPI, TPI.
///
/// LLT filter: 2.14e-9 * δ¹⁷ (Cherchi 2020 Table 1, degree 17). Float
/// filter is deferred; always uses exact expansion arithmetic.
#[allow(dead_code)]
fn orient2d_llt(
    la: &ImplicitPoint,
    lb: &ImplicitPoint,
    tc: &ImplicitPoint,
    i: usize,
    j: usize,
) -> f64 {
    let (q1a, q2a, ra, sa, ta) = match la {
        ImplicitPoint::LPI { q1, q2, r, s, t } => (q1, q2, r, s, t),
        _ => unreachable!("orient2d_llt: arg 0 must be LPI"),
    };
    let (q1b, q2b, rb, sb, tb) = match lb {
        ImplicitPoint::LPI { q1, q2, r, s, t } => (q1, q2, r, s, t),
        _ => unreachable!("orient2d_llt: arg 1 must be LPI"),
    };
    let (v1, v2, v3, w1, w2, w3, u1, u2, u3) = match tc {
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
        } => (v1, v2, v3, w1, w2, w3, u1, u2, u3),
        _ => unreachable!("orient2d_llt: arg 2 must be TPI"),
    };
    orient2d_llt_exact(
        q1a, q2a, ra, sa, ta, q1b, q2b, rb, sb, tb, v1, v2, v3, w1, w2, w3, u1, u2, u3, i, j,
    )
}

/// Exact orient2d for (LPI_a, LPI_b, TPI_c). Same difference-form derivation as LLL.
///   sign(orient2d) = sign(d_La) * sign(d_Lb) * sign(det)
///
/// Ref: Cherchi 2020 §4.2, Table 1 row LLT (filter 2.14e-9 * δ¹⁷).
#[allow(clippy::too_many_arguments)]
fn orient2d_llt_exact(
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
    v1: &[f64; 3],
    v2: &[f64; 3],
    v3: &[f64; 3],
    w1: &[f64; 3],
    w2: &[f64; 3],
    w3: &[f64; 3],
    u1: &[f64; 3],
    u2: &[f64; 3],
    u3: &[f64; 3],
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
    let (_, dc_exp, lcx, lcy, lcz) = match tpi_lambda_expansion(v1, v2, v3, w1, w2, w3, u1, u2, u3)
    {
        Some(v) => v,
        None => return 0.0,
    };

    let la = [lax, lay, laz];
    let lb = [lbx, lby, lbz];
    let lc = [lcx, lcy, lcz];

    let p1 = expansion_add(
        &expansion_mul_expansion(&la[i], &dc_exp),
        &expansion_negate(&expansion_mul_expansion(&lc[i], &da_exp)),
    );
    let p2 = expansion_add(
        &expansion_mul_expansion(&lb[j], &dc_exp),
        &expansion_negate(&expansion_mul_expansion(&lc[j], &db_exp)),
    );
    let p3 = expansion_add(
        &expansion_mul_expansion(&la[j], &dc_exp),
        &expansion_negate(&expansion_mul_expansion(&lc[j], &da_exp)),
    );
    let p4 = expansion_add(
        &expansion_mul_expansion(&lb[i], &dc_exp),
        &expansion_negate(&expansion_mul_expansion(&lc[i], &db_exp)),
    );

    let term1 = expansion_mul_expansion(&p1, &p2);
    let term2 = expansion_mul_expansion(&p3, &p4);
    let det_exp = expansion_add(&term1, &expansion_negate(&term2));
    let det_sign = expansion_sign(&det_exp);
    (da_sign * db_sign * det_sign) as f64
}

/// Stub dispatcher — LPI, TPI, TPI.
///
/// LTT filter: 2.54e-8 * δ²⁰ (Cherchi 2020 Table 1, degree 20). Float
/// filter is deferred; always uses exact expansion arithmetic.
#[allow(dead_code)]
fn orient2d_ltt(
    la: &ImplicitPoint,
    tb: &ImplicitPoint,
    tc: &ImplicitPoint,
    i: usize,
    j: usize,
) -> f64 {
    let (q1a, q2a, ra, sa, ta) = match la {
        ImplicitPoint::LPI { q1, q2, r, s, t } => (q1, q2, r, s, t),
        _ => unreachable!("orient2d_ltt: arg 0 must be LPI"),
    };
    let (bv1, bv2, bv3, bw1, bw2, bw3, bu1, bu2, bu3) = match tb {
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
        } => (v1, v2, v3, w1, w2, w3, u1, u2, u3),
        _ => unreachable!("orient2d_ltt: arg 1 must be TPI"),
    };
    let (cv1, cv2, cv3, cw1, cw2, cw3, cu1, cu2, cu3) = match tc {
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
        } => (v1, v2, v3, w1, w2, w3, u1, u2, u3),
        _ => unreachable!("orient2d_ltt: arg 2 must be TPI"),
    };
    orient2d_ltt_exact(
        q1a, q2a, ra, sa, ta, bv1, bv2, bv3, bw1, bw2, bw3, bu1, bu2, bu3, cv1, cv2, cv3, cw1, cw2,
        cw3, cu1, cu2, cu3, i, j,
    )
}

/// Exact orient2d for (LPI_a, TPI_b, TPI_c). Same difference-form derivation as LLT.
///
/// Ref: Cherchi 2020 §4.2, Table 1 row LTT (filter 2.54e-8 * δ²⁰).
#[allow(clippy::too_many_arguments)]
fn orient2d_ltt_exact(
    q1a: &[f64; 3],
    q2a: &[f64; 3],
    ra: &[f64; 3],
    sa: &[f64; 3],
    ta: &[f64; 3],
    bv1: &[f64; 3],
    bv2: &[f64; 3],
    bv3: &[f64; 3],
    bw1: &[f64; 3],
    bw2: &[f64; 3],
    bw3: &[f64; 3],
    bu1: &[f64; 3],
    bu2: &[f64; 3],
    bu3: &[f64; 3],
    cv1: &[f64; 3],
    cv2: &[f64; 3],
    cv3: &[f64; 3],
    cw1: &[f64; 3],
    cw2: &[f64; 3],
    cw3: &[f64; 3],
    cu1: &[f64; 3],
    cu2: &[f64; 3],
    cu3: &[f64; 3],
    i: usize,
    j: usize,
) -> f64 {
    let (da_sign, da_exp, lax, lay, laz) = match lpi_lambda_expansion(q1a, q2a, ra, sa, ta) {
        Some(v) => v,
        None => return 0.0,
    };
    let (db_sign, db_exp, lbx, lby, lbz) =
        match tpi_lambda_expansion(bv1, bv2, bv3, bw1, bw2, bw3, bu1, bu2, bu3) {
            Some(v) => v,
            None => return 0.0,
        };
    let (_, dc_exp, lcx, lcy, lcz) =
        match tpi_lambda_expansion(cv1, cv2, cv3, cw1, cw2, cw3, cu1, cu2, cu3) {
            Some(v) => v,
            None => return 0.0,
        };

    let la = [lax, lay, laz];
    let lb = [lbx, lby, lbz];
    let lc = [lcx, lcy, lcz];

    let p1 = expansion_add(
        &expansion_mul_expansion(&la[i], &dc_exp),
        &expansion_negate(&expansion_mul_expansion(&lc[i], &da_exp)),
    );
    let p2 = expansion_add(
        &expansion_mul_expansion(&lb[j], &dc_exp),
        &expansion_negate(&expansion_mul_expansion(&lc[j], &db_exp)),
    );
    let p3 = expansion_add(
        &expansion_mul_expansion(&la[j], &dc_exp),
        &expansion_negate(&expansion_mul_expansion(&lc[j], &da_exp)),
    );
    let p4 = expansion_add(
        &expansion_mul_expansion(&lb[i], &dc_exp),
        &expansion_negate(&expansion_mul_expansion(&lc[i], &db_exp)),
    );

    let term1 = expansion_mul_expansion(&p1, &p2);
    let term2 = expansion_mul_expansion(&p3, &p4);
    let det_exp = expansion_add(&term1, &expansion_negate(&term2));
    let det_sign = expansion_sign(&det_exp);
    (da_sign * db_sign * det_sign) as f64
}

/// Stub dispatcher — TPI, TPI, Explicit.
///
/// TTE filter: 3.31e-8 * δ²⁰ (Cherchi 2020 Table 1, degree 20). Float
/// filter is deferred; always uses exact expansion arithmetic.
#[allow(dead_code)]
fn orient2d_tte(
    ta: &ImplicitPoint,
    tb: &ImplicitPoint,
    ec: &ImplicitPoint,
    i: usize,
    j: usize,
) -> f64 {
    let (av1, av2, av3, aw1, aw2, aw3, au1, au2, au3) = match ta {
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
        } => (v1, v2, v3, w1, w2, w3, u1, u2, u3),
        _ => unreachable!("orient2d_tte: arg 0 must be TPI"),
    };
    let (bv1, bv2, bv3, bw1, bw2, bw3, bu1, bu2, bu3) = match tb {
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
        } => (v1, v2, v3, w1, w2, w3, u1, u2, u3),
        _ => unreachable!("orient2d_tte: arg 1 must be TPI"),
    };
    let e = match ec {
        ImplicitPoint::Explicit(e) => e,
        _ => unreachable!("orient2d_tte: arg 2 must be Explicit"),
    };
    orient2d_tte_exact(
        av1, av2, av3, aw1, aw2, aw3, au1, au2, au3, bv1, bv2, bv3, bw1, bw2, bw3, bu1, bu2, bu3,
        e, i, j,
    )
}

/// Exact orient2d for (TPI_a, TPI_b, Explicit_c).
///
/// γ=e, C=1; α=λ_Ta, A=d_Ta; β=λ_Tb, B=d_Tb. Same difference-form derivation as LTE.
///
/// Ref: Cherchi 2020 §4.2, Table 1 row TTE (filter 3.31e-8 * δ²⁰).
#[allow(clippy::too_many_arguments)]
fn orient2d_tte_exact(
    av1: &[f64; 3],
    av2: &[f64; 3],
    av3: &[f64; 3],
    aw1: &[f64; 3],
    aw2: &[f64; 3],
    aw3: &[f64; 3],
    au1: &[f64; 3],
    au2: &[f64; 3],
    au3: &[f64; 3],
    bv1: &[f64; 3],
    bv2: &[f64; 3],
    bv3: &[f64; 3],
    bw1: &[f64; 3],
    bw2: &[f64; 3],
    bw3: &[f64; 3],
    bu1: &[f64; 3],
    bu2: &[f64; 3],
    bu3: &[f64; 3],
    e: &[f64; 3],
    i: usize,
    j: usize,
) -> f64 {
    let (da_sign, da_exp, lax, lay, laz) =
        match tpi_lambda_expansion(av1, av2, av3, aw1, aw2, aw3, au1, au2, au3) {
            Some(v) => v,
            None => return 0.0,
        };
    let (db_sign, db_exp, lbx, lby, lbz) =
        match tpi_lambda_expansion(bv1, bv2, bv3, bw1, bw2, bw3, bu1, bu2, bu3) {
            Some(v) => v,
            None => return 0.0,
        };

    let la = [lax, lay, laz];
    let lb = [lbx, lby, lbz];

    // C=1, γ=e.
    let p1 = expansion_add(&la[i], &expansion_negate(&expansion_scale(&da_exp, e[i])));
    let p2 = expansion_add(&lb[j], &expansion_negate(&expansion_scale(&db_exp, e[j])));
    let p3 = expansion_add(&la[j], &expansion_negate(&expansion_scale(&da_exp, e[j])));
    let p4 = expansion_add(&lb[i], &expansion_negate(&expansion_scale(&db_exp, e[i])));

    let term1 = expansion_mul_expansion(&p1, &p2);
    let term2 = expansion_mul_expansion(&p3, &p4);
    let det_exp = expansion_add(&term1, &expansion_negate(&term2));
    let det_sign = expansion_sign(&det_exp);
    (da_sign * db_sign * det_sign) as f64
}

/// Stub dispatcher — TPI, TPI, TPI.
///
/// TTT filter: 3.10e-6 * δ²⁶ (Cherchi 2020 Table 1, degree 26). Float
/// filter is deferred; always uses exact expansion arithmetic.
#[allow(dead_code)]
fn orient2d_ttt(
    ta: &ImplicitPoint,
    tb: &ImplicitPoint,
    tc: &ImplicitPoint,
    i: usize,
    j: usize,
) -> f64 {
    let (av1, av2, av3, aw1, aw2, aw3, au1, au2, au3) = match ta {
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
        } => (v1, v2, v3, w1, w2, w3, u1, u2, u3),
        _ => unreachable!("orient2d_ttt: arg 0 must be TPI"),
    };
    let (bv1, bv2, bv3, bw1, bw2, bw3, bu1, bu2, bu3) = match tb {
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
        } => (v1, v2, v3, w1, w2, w3, u1, u2, u3),
        _ => unreachable!("orient2d_ttt: arg 1 must be TPI"),
    };
    let (cv1, cv2, cv3, cw1, cw2, cw3, cu1, cu2, cu3) = match tc {
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
        } => (v1, v2, v3, w1, w2, w3, u1, u2, u3),
        _ => unreachable!("orient2d_ttt: arg 2 must be TPI"),
    };
    orient2d_ttt_exact(
        av1, av2, av3, aw1, aw2, aw3, au1, au2, au3, bv1, bv2, bv3, bw1, bw2, bw3, bu1, bu2, bu3,
        cv1, cv2, cv3, cw1, cw2, cw3, cu1, cu2, cu3, i, j,
    )
}

/// Exact orient2d for (TPI_a, TPI_b, TPI_c). Same difference form as LLT/LTT.
///
/// Ref: Cherchi 2020 §4.2, Table 1 row TTT (filter 3.10e-6 * δ²⁶).
#[allow(clippy::too_many_arguments)]
fn orient2d_ttt_exact(
    av1: &[f64; 3],
    av2: &[f64; 3],
    av3: &[f64; 3],
    aw1: &[f64; 3],
    aw2: &[f64; 3],
    aw3: &[f64; 3],
    au1: &[f64; 3],
    au2: &[f64; 3],
    au3: &[f64; 3],
    bv1: &[f64; 3],
    bv2: &[f64; 3],
    bv3: &[f64; 3],
    bw1: &[f64; 3],
    bw2: &[f64; 3],
    bw3: &[f64; 3],
    bu1: &[f64; 3],
    bu2: &[f64; 3],
    bu3: &[f64; 3],
    cv1: &[f64; 3],
    cv2: &[f64; 3],
    cv3: &[f64; 3],
    cw1: &[f64; 3],
    cw2: &[f64; 3],
    cw3: &[f64; 3],
    cu1: &[f64; 3],
    cu2: &[f64; 3],
    cu3: &[f64; 3],
    i: usize,
    j: usize,
) -> f64 {
    let (da_sign, da_exp, lax, lay, laz) =
        match tpi_lambda_expansion(av1, av2, av3, aw1, aw2, aw3, au1, au2, au3) {
            Some(v) => v,
            None => return 0.0,
        };
    let (db_sign, db_exp, lbx, lby, lbz) =
        match tpi_lambda_expansion(bv1, bv2, bv3, bw1, bw2, bw3, bu1, bu2, bu3) {
            Some(v) => v,
            None => return 0.0,
        };
    let (_, dc_exp, lcx, lcy, lcz) =
        match tpi_lambda_expansion(cv1, cv2, cv3, cw1, cw2, cw3, cu1, cu2, cu3) {
            Some(v) => v,
            None => return 0.0,
        };

    let la = [lax, lay, laz];
    let lb = [lbx, lby, lbz];
    let lc = [lcx, lcy, lcz];

    let p1 = expansion_add(
        &expansion_mul_expansion(&la[i], &dc_exp),
        &expansion_negate(&expansion_mul_expansion(&lc[i], &da_exp)),
    );
    let p2 = expansion_add(
        &expansion_mul_expansion(&lb[j], &dc_exp),
        &expansion_negate(&expansion_mul_expansion(&lc[j], &db_exp)),
    );
    let p3 = expansion_add(
        &expansion_mul_expansion(&la[j], &dc_exp),
        &expansion_negate(&expansion_mul_expansion(&lc[j], &da_exp)),
    );
    let p4 = expansion_add(
        &expansion_mul_expansion(&lb[i], &dc_exp),
        &expansion_negate(&expansion_mul_expansion(&lc[i], &db_exp)),
    );

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
/// Uses true indirect predicates for all 27 ordered combinations of
/// (Explicit, LPI, TPI) point types via the 10 base multisets in Cherchi
/// 2020 §4.2 (EEE, LEE, LLE, LLL, TEE, LTE, LLT, LTT, TTE, TTT).
/// Permutations are routed by antisymmetry (swap → negate) and
/// cyclic-rotation parity (3-cycle preserves sign).
///
/// Avoids the precision-losing materialization (division by d_L / d_T) by
/// keeping all arithmetic in homogeneous coords + Shewchuk expansions.
///
/// Materialize fallback retained as a defensive safety net (every case
/// should now be covered explicitly).
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

    match (a, b, c) {
        // ── EEE ──────────────────────────────────────────────────────
        (ImplicitPoint::Explicit(ea), ImplicitPoint::Explicit(eb), ImplicitPoint::Explicit(ec)) => {
            geometry_predicates::orient2d([ea[i], ea[j]], [eb[i], eb[j]], [ec[i], ec[j]])
        }

        // ── LEE multiset (canonical: L, E, E) ────────────────────────
        // (L, E, E): identity
        (
            ImplicitPoint::LPI { q1, q2, r, s, t },
            ImplicitPoint::Explicit(e1),
            ImplicitPoint::Explicit(e2),
        ) => orient2d_lee(q1, q2, r, s, t, e1, e2, i, j),
        // (E, L, E): swap arg 0,1 → negate
        (
            ImplicitPoint::Explicit(e1),
            ImplicitPoint::LPI { q1, q2, r, s, t },
            ImplicitPoint::Explicit(e2),
        ) => -orient2d_lee(q1, q2, r, s, t, e1, e2, i, j),
        // (E, E, L): even cyclic rotation (E,E,L)→(L,E,E)
        (
            ImplicitPoint::Explicit(e1),
            ImplicitPoint::Explicit(e2),
            ImplicitPoint::LPI { q1, q2, r, s, t },
        ) => orient2d_lee(q1, q2, r, s, t, e1, e2, i, j),

        // ── LLE multiset (canonical: L, L, E) ────────────────────────
        // (L, L, E): identity
        (ImplicitPoint::LPI { .. }, ImplicitPoint::LPI { .. }, ImplicitPoint::Explicit(_)) => {
            orient2d_lle(a, b, c, i, j)
        }
        // (L, E, L): swap arg 1,2 → negate
        (ImplicitPoint::LPI { .. }, ImplicitPoint::Explicit(_), ImplicitPoint::LPI { .. }) => {
            -orient2d_lle(a, c, b, i, j)
        }
        // (E, L, L): even cyclic rotation
        (ImplicitPoint::Explicit(_), ImplicitPoint::LPI { .. }, ImplicitPoint::LPI { .. }) => {
            orient2d_lle(b, c, a, i, j)
        }

        // ── LLL multiset (only 1 ordering) ───────────────────────────
        (ImplicitPoint::LPI { .. }, ImplicitPoint::LPI { .. }, ImplicitPoint::LPI { .. }) => {
            orient2d_lll(a, b, c, i, j)
        }

        // ── TEE multiset (canonical: T, E, E) ────────────────────────
        // (T, E, E): identity
        (
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
            },
            ImplicitPoint::Explicit(e1),
            ImplicitPoint::Explicit(e2),
        ) => orient2d_tee(v1, v2, v3, w1, w2, w3, u1, u2, u3, e1, e2, i, j),
        // (E, T, E): swap arg 0,1 → negate
        (
            ImplicitPoint::Explicit(e1),
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
            },
            ImplicitPoint::Explicit(e2),
        ) => -orient2d_tee(v1, v2, v3, w1, w2, w3, u1, u2, u3, e1, e2, i, j),
        // (E, E, T): even cyclic rotation (E,E,T)→(T,E,E)
        (
            ImplicitPoint::Explicit(e1),
            ImplicitPoint::Explicit(e2),
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
            },
        ) => orient2d_tee(v1, v2, v3, w1, w2, w3, u1, u2, u3, e1, e2, i, j),

        // ── LTE multiset (canonical: L, T, E) ────────────────────────
        // (L, T, E): identity
        (ImplicitPoint::LPI { .. }, ImplicitPoint::TPI { .. }, ImplicitPoint::Explicit(_)) => {
            orient2d_lte(a, b, c, i, j)
        }
        // (L, E, T): swap arg 1,2 → negate
        (ImplicitPoint::LPI { .. }, ImplicitPoint::Explicit(_), ImplicitPoint::TPI { .. }) => {
            -orient2d_lte(a, c, b, i, j)
        }
        // (T, L, E): swap arg 0,1 → negate
        (ImplicitPoint::TPI { .. }, ImplicitPoint::LPI { .. }, ImplicitPoint::Explicit(_)) => {
            -orient2d_lte(b, a, c, i, j)
        }
        // (T, E, L): even cyclic rotation (T,E,L)→(L,T,E)
        (ImplicitPoint::TPI { .. }, ImplicitPoint::Explicit(_), ImplicitPoint::LPI { .. }) => {
            orient2d_lte(c, a, b, i, j)
        }
        // (E, L, T): even cyclic rotation (E,L,T)→(L,T,E)
        (ImplicitPoint::Explicit(_), ImplicitPoint::LPI { .. }, ImplicitPoint::TPI { .. }) => {
            orient2d_lte(b, c, a, i, j)
        }
        // (E, T, L): odd permutation (swap 0,2) → negate
        (ImplicitPoint::Explicit(_), ImplicitPoint::TPI { .. }, ImplicitPoint::LPI { .. }) => {
            -orient2d_lte(c, b, a, i, j)
        }

        // ── LLT multiset (canonical: L, L, T) ────────────────────────
        // (L, L, T): identity
        (ImplicitPoint::LPI { .. }, ImplicitPoint::LPI { .. }, ImplicitPoint::TPI { .. }) => {
            orient2d_llt(a, b, c, i, j)
        }
        // (L, T, L): swap arg 1,2 → negate
        (ImplicitPoint::LPI { .. }, ImplicitPoint::TPI { .. }, ImplicitPoint::LPI { .. }) => {
            -orient2d_llt(a, c, b, i, j)
        }
        // (T, L, L): even cyclic rotation (T,L,L)→(L,L,T)
        (ImplicitPoint::TPI { .. }, ImplicitPoint::LPI { .. }, ImplicitPoint::LPI { .. }) => {
            orient2d_llt(b, c, a, i, j)
        }

        // ── LTT multiset (canonical: L, T, T) ────────────────────────
        // (L, T, T): identity
        (ImplicitPoint::LPI { .. }, ImplicitPoint::TPI { .. }, ImplicitPoint::TPI { .. }) => {
            orient2d_ltt(a, b, c, i, j)
        }
        // (T, L, T): swap arg 0,1 → negate
        (ImplicitPoint::TPI { .. }, ImplicitPoint::LPI { .. }, ImplicitPoint::TPI { .. }) => {
            -orient2d_ltt(b, a, c, i, j)
        }
        // (T, T, L): even cyclic rotation (T,T,L)→(L,T,T)
        (ImplicitPoint::TPI { .. }, ImplicitPoint::TPI { .. }, ImplicitPoint::LPI { .. }) => {
            orient2d_ltt(c, a, b, i, j)
        }

        // ── TTE multiset (canonical: T, T, E) ────────────────────────
        // (T, T, E): identity
        (ImplicitPoint::TPI { .. }, ImplicitPoint::TPI { .. }, ImplicitPoint::Explicit(_)) => {
            orient2d_tte(a, b, c, i, j)
        }
        // (T, E, T): swap arg 1,2 → negate
        (ImplicitPoint::TPI { .. }, ImplicitPoint::Explicit(_), ImplicitPoint::TPI { .. }) => {
            -orient2d_tte(a, c, b, i, j)
        }
        // (E, T, T): even cyclic rotation (E,T,T)→(T,T,E)
        (ImplicitPoint::Explicit(_), ImplicitPoint::TPI { .. }, ImplicitPoint::TPI { .. }) => {
            orient2d_tte(b, c, a, i, j)
        }

        // ── TTT multiset ─────────────────────────────────────────────
        (ImplicitPoint::TPI { .. }, ImplicitPoint::TPI { .. }, ImplicitPoint::TPI { .. }) => {
            orient2d_ttt(a, b, c, i, j)
        }

        // Defensive safety net — every (E,L,T)³ combination is covered above.
        // Reachable only if a future point type is added to ImplicitPoint.
        #[allow(unreachable_patterns)]
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
    // Type code: 0 = Explicit, 1 = LPI, 2 = TPI. Canonical multiset order
    // sorts by ascending code so the dispatch only matches 15 base cases.
    let type_code = |p: &ImplicitPoint| -> u8 {
        match p {
            ImplicitPoint::Explicit(_) => 0,
            ImplicitPoint::LPI { .. } => 1,
            ImplicitPoint::TPI { .. } => 2,
        }
    };
    let mut codes = [type_code(a), type_code(b), type_code(c), type_code(d)];
    let mut args: [&ImplicitPoint; 4] = [a, b, c, d];

    // Bubble-sort ascending by code, tracking parity. Each adjacent swap of
    // an orient3d argument flips the determinant's sign.
    let mut parity: i32 = 1;
    for i in 0..4 {
        for j in 0..(3 - i) {
            if codes[j] > codes[j + 1] {
                codes.swap(j, j + 1);
                args.swap(j, j + 1);
                parity = -parity;
            }
        }
    }

    let raw = match codes {
        // EEEE — direct Shewchuk on the original args (no implicit points,
        // sorting was a no-op so order is preserved).
        [0, 0, 0, 0] => {
            let ea = match args[0] {
                ImplicitPoint::Explicit(e) => e,
                _ => unreachable!(),
            };
            let eb = match args[1] {
                ImplicitPoint::Explicit(e) => e,
                _ => unreachable!(),
            };
            let ec = match args[2] {
                ImplicitPoint::Explicit(e) => e,
                _ => unreachable!(),
            };
            let ed = match args[3] {
                ImplicitPoint::Explicit(e) => e,
                _ => unreachable!(),
            };
            geometry_predicates::orient3d(*ea, *eb, *ec, *ed)
        }
        // LEEE — 1 LPI + 3 Explicit. Canonical order: (L, E, E, E).
        [0, 0, 0, 1] => {
            // After sorting ascending, the LPI is at position 3. Move it to
            // position 0 with three adjacent swaps (parity * (-1)^3 = -parity)
            // — equivalently call leee with (L, E, E, E) and absorb -1 here.
            -orient3d_leee(args[3], args[0], args[1], args[2])
        }
        // TEEE — 1 TPI + 3 Explicit. Canonical order: (T, E, E, E).
        [0, 0, 0, 2] => -orient3d_teee(args[3], args[0], args[1], args[2]),
        // LLEE — 2 LPI + 2 Explicit. Canonical order: (L, L, E, E).
        [0, 0, 1, 1] => orient3d_llee(args[2], args[3], args[0], args[1]),
        // LTEE — 1 LPI + 1 TPI + 2 Explicit. Canonical order: (L, T, E, E).
        [0, 0, 1, 2] => orient3d_ltee(args[2], args[3], args[0], args[1]),
        // TTEE — 2 TPI + 2 Explicit. Canonical order: (T, T, E, E).
        [0, 0, 2, 2] => orient3d_ttee(args[2], args[3], args[0], args[1]),
        // LLLE — 3 LPI + 1 Explicit. Canonical order: (L, L, L, E).
        [0, 1, 1, 1] => -orient3d_llle(args[1], args[2], args[3], args[0]),
        // LLTE — 2 LPI + 1 TPI + 1 Explicit. Canonical order: (L, L, T, E).
        [0, 1, 1, 2] => -orient3d_llte(args[1], args[2], args[3], args[0]),
        // LTTE — 1 LPI + 2 TPI + 1 Explicit. Canonical order: (L, T, T, E).
        [0, 1, 2, 2] => -orient3d_ltte(args[1], args[2], args[3], args[0]),
        // TTTE — 3 TPI + 1 Explicit. Canonical order: (T, T, T, E).
        [0, 2, 2, 2] => -orient3d_ttte(args[1], args[2], args[3], args[0]),
        // LLLL — 4 LPI.
        [1, 1, 1, 1] => orient3d_llll(args[0], args[1], args[2], args[3]),
        // LLLT — 3 LPI + 1 TPI. Canonical order: (L, L, L, T).
        [1, 1, 1, 2] => orient3d_lllt(args[0], args[1], args[2], args[3]),
        // LLTT — 2 LPI + 2 TPI. Canonical order: (L, L, T, T).
        [1, 1, 2, 2] => orient3d_lltt(args[0], args[1], args[2], args[3]),
        // LTTT — 1 LPI + 3 TPI. Canonical order: (L, T, T, T).
        [1, 2, 2, 2] => orient3d_lttt(args[0], args[1], args[2], args[3]),
        // TTTT — 4 TPI.
        [2, 2, 2, 2] => orient3d_tttt(args[0], args[1], args[2], args[3]),
        // Defensive: bubble-sort always yields one of the 15 ascending codes.
        _ => orient3d_materialize_fallback(a, b, c, d),
    };
    raw * (parity as f64)
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

// ── Orient3d TPI variants (Cherchi 2020 §4.2) ────────────────────────────
//
// Twelve true-indirect orient3d functions covering every multiset of point
// types that contains at least one TPI or three-or-more LPIs (the LEEE/LLEE
// cases above are the LPI-only ones already in place; EEEE goes through
// `geometry_predicates::orient3d` directly in the dispatch).
//
// All variants share the same homogeneous 4×4 strategy. The orient3d sign is
// the sign of:
//
// ```text
//     | ax  ay  az  1 |
//     | bx  by  bz  1 |
//     | cx  cy  cz  1 |
//     | dx  dy  dz  1 |
// ```
//
// For each implicit point `pi = (λi_x/di, λi_y/di, λi_z/di)` we multiply the
// corresponding row by `di`, which scales the determinant by `di`. Explicit
// rows stay unscaled (their fourth column is the literal `1`). After
// substitution the determinant becomes a 4×4 over exact expansions, and
//
// ```text
//     sign(orient3d) = sign(det_scaled) · ∏ sign(d_i)
// ```
//
// Every multiplication, addition and subtraction is done with Shewchuk-style
// expansion arithmetic, so the result is exact regardless of input
// magnitude.
//
// Float-filter (Cherchi 2020 Table 1) constants for orient3d TPI variants
// are not in our reference doc — they live only in the C++ reference at
// github.com/gcherchi/FastAndRobustMeshArrangements. Until those constants
// are ported, these dispatchers always evaluate the exact path; the stub
// names mirror the LEEE/LLEE pattern so a future PR can slot the float
// filter in without touching call sites.

/// Build the 4-component homogeneous expansion row for an Explicit point.
fn orient3d_row_explicit(e: &[f64; 3]) -> [Vec<f64>; 4] {
    [vec![e[0]], vec![e[1]], vec![e[2]], vec![1.0]]
}

/// Build the 4-component homogeneous expansion row for an LPI point.
/// Returns `(d_sign, row)` where the row is `(λx, λy, λz, d_L)`.
/// Returns `None` if d_L = 0.
fn orient3d_row_lpi(
    q1: &[f64; 3],
    q2: &[f64; 3],
    r: &[f64; 3],
    s: &[f64; 3],
    t: &[f64; 3],
) -> Option<(i32, [Vec<f64>; 4])> {
    let (d_sign, d_exp, lx, ly, lz) = lpi_lambda_expansion(q1, q2, r, s, t)?;
    Some((d_sign, [lx, ly, lz, d_exp]))
}

/// Build the 4-component homogeneous expansion row for a TPI point.
/// Returns `(d_sign, row)` where the row is `(λx, λy, λz, d_T)`.
/// Returns `None` if d_T = 0.
#[allow(clippy::too_many_arguments)]
fn orient3d_row_tpi(
    v1: &[f64; 3],
    v2: &[f64; 3],
    v3: &[f64; 3],
    w1: &[f64; 3],
    w2: &[f64; 3],
    w3: &[f64; 3],
    u1: &[f64; 3],
    u2: &[f64; 3],
    u3: &[f64; 3],
) -> Option<(i32, [Vec<f64>; 4])> {
    let (d_sign, d_exp, lx, ly, lz) = tpi_lambda_expansion(v1, v2, v3, w1, w2, w3, u1, u2, u3)?;
    Some((d_sign, [lx, ly, lz, d_exp]))
}

/// Exact 4×4 determinant where each row is a 4-component expansion vector.
///
/// Cofactor expansion along row 0:
///   det = + r0[0] · |minor(0,0)|
///         - r0[1] · |minor(0,1)|
///         + r0[2] · |minor(0,2)|
///         - r0[3] · |minor(0,3)|
///
/// Each minor is a 3×3 determinant over expansions, evaluated by the existing
/// `det3x3_expansion` helper.
fn det4x4_expansion(
    r0: &[Vec<f64>; 4],
    r1: &[Vec<f64>; 4],
    r2: &[Vec<f64>; 4],
    r3: &[Vec<f64>; 4],
) -> Vec<f64> {
    let cols_for = |skip: usize| -> [usize; 3] {
        match skip {
            0 => [1, 2, 3],
            1 => [0, 2, 3],
            2 => [0, 1, 3],
            _ => [0, 1, 2],
        }
    };

    let mut acc: Vec<f64> = vec![0.0];
    for j in 0..4 {
        let cols = cols_for(j);
        let m1 = [
            r1[cols[0]].clone(),
            r1[cols[1]].clone(),
            r1[cols[2]].clone(),
        ];
        let m2 = [
            r2[cols[0]].clone(),
            r2[cols[1]].clone(),
            r2[cols[2]].clone(),
        ];
        let m3 = [
            r3[cols[0]].clone(),
            r3[cols[1]].clone(),
            r3[cols[2]].clone(),
        ];
        let minor = det3x3_expansion(&m1, &m2, &m3);
        let term = expansion_mul_expansion(&r0[j], &minor);
        let signed = if j % 2 == 0 {
            term
        } else {
            expansion_negate(&term)
        };
        acc = expansion_add(&acc, &signed);
    }
    acc
}

/// Combine 4 row-builder results into the orient3d sign.
///
/// `signs` are the sign-of-d for each row (1 for Explicit, sign(d_L)/sign(d_T)
/// for implicit). The final orient3d sign is `sign(det_4x4) · ∏ signs`.
fn orient3d_combine(rows: [&[Vec<f64>; 4]; 4], signs: [i32; 4]) -> f64 {
    let det_exp = det4x4_expansion(rows[0], rows[1], rows[2], rows[3]);
    let det_sign = expansion_sign(&det_exp);
    let prod = signs[0] * signs[1] * signs[2] * signs[3];
    (det_sign * prod) as f64
}

// ── orient3d_LLLE: 3 LPI + 1 Explicit ───────────────────────────────────

/// True indirect orient3d for (LPI_a, LPI_b, LPI_c, Explicit).
///
/// Filter constants for orient3d TPI/multi-LPI variants are not in our
/// reference doc — filter deferred — always exact for now.
///
/// Ref: Cherchi 2020 §4.2, orient3D_LLLE.
fn orient3d_llle(
    la: &ImplicitPoint,
    lb: &ImplicitPoint,
    lc: &ImplicitPoint,
    e: &ImplicitPoint,
) -> f64 {
    let (q1a, q2a, ra, sa, ta) = match la {
        ImplicitPoint::LPI { q1, q2, r, s, t } => (q1, q2, r, s, t),
        _ => unreachable!(),
    };
    let (q1b, q2b, rb, sb, tb) = match lb {
        ImplicitPoint::LPI { q1, q2, r, s, t } => (q1, q2, r, s, t),
        _ => unreachable!(),
    };
    let (q1c, q2c, rc, sc, tc) = match lc {
        ImplicitPoint::LPI { q1, q2, r, s, t } => (q1, q2, r, s, t),
        _ => unreachable!(),
    };
    let ec = match e {
        ImplicitPoint::Explicit(coords) => coords,
        _ => unreachable!(),
    };
    orient3d_llle_exact(
        q1a, q2a, ra, sa, ta, q1b, q2b, rb, sb, tb, q1c, q2c, rc, sc, tc, ec,
    )
}

/// Exact orient3d_LLLE — 3 LPI rows + 1 Explicit row.
#[allow(clippy::too_many_arguments)]
fn orient3d_llle_exact(
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
    e: &[f64; 3],
) -> f64 {
    let (sa_d, ra_row) = match orient3d_row_lpi(q1a, q2a, ra, sa, ta) {
        Some(v) => v,
        None => return 0.0,
    };
    let (sb_d, rb_row) = match orient3d_row_lpi(q1b, q2b, rb, sb, tb) {
        Some(v) => v,
        None => return 0.0,
    };
    let (sc_d, rc_row) = match orient3d_row_lpi(q1c, q2c, rc, sc, tc) {
        Some(v) => v,
        None => return 0.0,
    };
    let re_row = orient3d_row_explicit(e);
    orient3d_combine([&ra_row, &rb_row, &rc_row, &re_row], [sa_d, sb_d, sc_d, 1])
}

// ── orient3d_LLLL: 4 LPI ────────────────────────────────────────────────

/// True indirect orient3d for (LPI, LPI, LPI, LPI).
///
/// Filter constants for orient3d TPI/multi-LPI variants are not in our
/// reference doc — filter deferred — always exact for now.
///
/// Ref: Cherchi 2020 §4.2, orient3D_LLLL.
fn orient3d_llll(
    la: &ImplicitPoint,
    lb: &ImplicitPoint,
    lc: &ImplicitPoint,
    ld: &ImplicitPoint,
) -> f64 {
    let (q1a, q2a, ra, sa, ta) = match la {
        ImplicitPoint::LPI { q1, q2, r, s, t } => (q1, q2, r, s, t),
        _ => unreachable!(),
    };
    let (q1b, q2b, rb, sb, tb) = match lb {
        ImplicitPoint::LPI { q1, q2, r, s, t } => (q1, q2, r, s, t),
        _ => unreachable!(),
    };
    let (q1c, q2c, rc, sc, tc) = match lc {
        ImplicitPoint::LPI { q1, q2, r, s, t } => (q1, q2, r, s, t),
        _ => unreachable!(),
    };
    let (q1d, q2d, rd, sd, td) = match ld {
        ImplicitPoint::LPI { q1, q2, r, s, t } => (q1, q2, r, s, t),
        _ => unreachable!(),
    };
    orient3d_llll_exact(
        q1a, q2a, ra, sa, ta, q1b, q2b, rb, sb, tb, q1c, q2c, rc, sc, tc, q1d, q2d, rd, sd, td,
    )
}

/// Exact orient3d_LLLL — 4 LPI rows.
#[allow(clippy::too_many_arguments)]
fn orient3d_llll_exact(
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
    q1d: &[f64; 3],
    q2d: &[f64; 3],
    rd: &[f64; 3],
    sd: &[f64; 3],
    td: &[f64; 3],
) -> f64 {
    let (sa_d, ra_row) = match orient3d_row_lpi(q1a, q2a, ra, sa, ta) {
        Some(v) => v,
        None => return 0.0,
    };
    let (sb_d, rb_row) = match orient3d_row_lpi(q1b, q2b, rb, sb, tb) {
        Some(v) => v,
        None => return 0.0,
    };
    let (sc_d, rc_row) = match orient3d_row_lpi(q1c, q2c, rc, sc, tc) {
        Some(v) => v,
        None => return 0.0,
    };
    let (sd_d, rd_row) = match orient3d_row_lpi(q1d, q2d, rd, sd, td) {
        Some(v) => v,
        None => return 0.0,
    };
    orient3d_combine(
        [&ra_row, &rb_row, &rc_row, &rd_row],
        [sa_d, sb_d, sc_d, sd_d],
    )
}

// ── orient3d_TEEE: 1 TPI + 3 Explicit ───────────────────────────────────

/// True indirect orient3d for (TPI, Explicit, Explicit, Explicit).
///
/// Filter constants for orient3d TPI variants are not in our reference doc —
/// filter deferred — always exact for now.
///
/// Ref: Cherchi 2020 §4.2, orient3D_TEEE.
fn orient3d_teee(
    t: &ImplicitPoint,
    e1: &ImplicitPoint,
    e2: &ImplicitPoint,
    e3: &ImplicitPoint,
) -> f64 {
    let (v1, v2, v3, w1, w2, w3, u1, u2, u3) = match t {
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
        } => (v1, v2, v3, w1, w2, w3, u1, u2, u3),
        _ => unreachable!(),
    };
    let e1c = match e1 {
        ImplicitPoint::Explicit(coords) => coords,
        _ => unreachable!(),
    };
    let e2c = match e2 {
        ImplicitPoint::Explicit(coords) => coords,
        _ => unreachable!(),
    };
    let e3c = match e3 {
        ImplicitPoint::Explicit(coords) => coords,
        _ => unreachable!(),
    };
    orient3d_teee_exact(v1, v2, v3, w1, w2, w3, u1, u2, u3, e1c, e2c, e3c)
}

/// Exact orient3d_TEEE — 1 TPI row + 3 Explicit rows.
#[allow(clippy::too_many_arguments)]
fn orient3d_teee_exact(
    v1: &[f64; 3],
    v2: &[f64; 3],
    v3: &[f64; 3],
    w1: &[f64; 3],
    w2: &[f64; 3],
    w3: &[f64; 3],
    u1: &[f64; 3],
    u2: &[f64; 3],
    u3: &[f64; 3],
    e1: &[f64; 3],
    e2: &[f64; 3],
    e3: &[f64; 3],
) -> f64 {
    let (st_d, rt_row) = match orient3d_row_tpi(v1, v2, v3, w1, w2, w3, u1, u2, u3) {
        Some(v) => v,
        None => return 0.0,
    };
    let r1_row = orient3d_row_explicit(e1);
    let r2_row = orient3d_row_explicit(e2);
    let r3_row = orient3d_row_explicit(e3);
    orient3d_combine([&rt_row, &r1_row, &r2_row, &r3_row], [st_d, 1, 1, 1])
}

// ── orient3d_LTEE: 1 LPI + 1 TPI + 2 Explicit ───────────────────────────

/// True indirect orient3d for (LPI, TPI, Explicit, Explicit).
///
/// Filter constants for orient3d TPI variants are not in our reference doc —
/// filter deferred — always exact for now.
///
/// Ref: Cherchi 2020 §4.2, orient3D_LTEE.
fn orient3d_ltee(
    l: &ImplicitPoint,
    t: &ImplicitPoint,
    e1: &ImplicitPoint,
    e2: &ImplicitPoint,
) -> f64 {
    let (q1, q2, r, s, tt) = match l {
        ImplicitPoint::LPI { q1, q2, r, s, t } => (q1, q2, r, s, t),
        _ => unreachable!(),
    };
    let (v1, v2, v3, w1, w2, w3, u1, u2, u3) = match t {
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
        } => (v1, v2, v3, w1, w2, w3, u1, u2, u3),
        _ => unreachable!(),
    };
    let e1c = match e1 {
        ImplicitPoint::Explicit(coords) => coords,
        _ => unreachable!(),
    };
    let e2c = match e2 {
        ImplicitPoint::Explicit(coords) => coords,
        _ => unreachable!(),
    };
    orient3d_ltee_exact(
        q1, q2, r, s, tt, v1, v2, v3, w1, w2, w3, u1, u2, u3, e1c, e2c,
    )
}

/// Exact orient3d_LTEE — 1 LPI + 1 TPI + 2 Explicit rows.
#[allow(clippy::too_many_arguments)]
fn orient3d_ltee_exact(
    q1: &[f64; 3],
    q2: &[f64; 3],
    r: &[f64; 3],
    s: &[f64; 3],
    tt: &[f64; 3],
    v1: &[f64; 3],
    v2: &[f64; 3],
    v3: &[f64; 3],
    w1: &[f64; 3],
    w2: &[f64; 3],
    w3: &[f64; 3],
    u1: &[f64; 3],
    u2: &[f64; 3],
    u3: &[f64; 3],
    e1: &[f64; 3],
    e2: &[f64; 3],
) -> f64 {
    let (sl_d, rl_row) = match orient3d_row_lpi(q1, q2, r, s, tt) {
        Some(v) => v,
        None => return 0.0,
    };
    let (st_d, rt_row) = match orient3d_row_tpi(v1, v2, v3, w1, w2, w3, u1, u2, u3) {
        Some(v) => v,
        None => return 0.0,
    };
    let r1_row = orient3d_row_explicit(e1);
    let r2_row = orient3d_row_explicit(e2);
    orient3d_combine([&rl_row, &rt_row, &r1_row, &r2_row], [sl_d, st_d, 1, 1])
}

// ── orient3d_LLTE: 2 LPI + 1 TPI + 1 Explicit ───────────────────────────

/// True indirect orient3d for (LPI, LPI, TPI, Explicit).
///
/// Filter constants for orient3d TPI variants are not in our reference doc —
/// filter deferred — always exact for now.
///
/// Ref: Cherchi 2020 §4.2, orient3D_LLTE.
fn orient3d_llte(
    la: &ImplicitPoint,
    lb: &ImplicitPoint,
    t: &ImplicitPoint,
    e: &ImplicitPoint,
) -> f64 {
    let (q1a, q2a, ra, sa, ta) = match la {
        ImplicitPoint::LPI { q1, q2, r, s, t } => (q1, q2, r, s, t),
        _ => unreachable!(),
    };
    let (q1b, q2b, rb, sb, tb) = match lb {
        ImplicitPoint::LPI { q1, q2, r, s, t } => (q1, q2, r, s, t),
        _ => unreachable!(),
    };
    let (v1, v2, v3, w1, w2, w3, u1, u2, u3) = match t {
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
        } => (v1, v2, v3, w1, w2, w3, u1, u2, u3),
        _ => unreachable!(),
    };
    let ec = match e {
        ImplicitPoint::Explicit(coords) => coords,
        _ => unreachable!(),
    };
    orient3d_llte_exact(
        q1a, q2a, ra, sa, ta, q1b, q2b, rb, sb, tb, v1, v2, v3, w1, w2, w3, u1, u2, u3, ec,
    )
}

/// Exact orient3d_LLTE — 2 LPI + 1 TPI + 1 Explicit rows.
#[allow(clippy::too_many_arguments)]
fn orient3d_llte_exact(
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
    v1: &[f64; 3],
    v2: &[f64; 3],
    v3: &[f64; 3],
    w1: &[f64; 3],
    w2: &[f64; 3],
    w3: &[f64; 3],
    u1: &[f64; 3],
    u2: &[f64; 3],
    u3: &[f64; 3],
    e: &[f64; 3],
) -> f64 {
    let (sa_d, ra_row) = match orient3d_row_lpi(q1a, q2a, ra, sa, ta) {
        Some(v) => v,
        None => return 0.0,
    };
    let (sb_d, rb_row) = match orient3d_row_lpi(q1b, q2b, rb, sb, tb) {
        Some(v) => v,
        None => return 0.0,
    };
    let (st_d, rt_row) = match orient3d_row_tpi(v1, v2, v3, w1, w2, w3, u1, u2, u3) {
        Some(v) => v,
        None => return 0.0,
    };
    let re_row = orient3d_row_explicit(e);
    orient3d_combine([&ra_row, &rb_row, &rt_row, &re_row], [sa_d, sb_d, st_d, 1])
}

// ── orient3d_TTEE: 2 TPI + 2 Explicit ───────────────────────────────────

/// True indirect orient3d for (TPI, TPI, Explicit, Explicit).
///
/// Filter constants for orient3d TPI variants are not in our reference doc —
/// filter deferred — always exact for now.
///
/// Ref: Cherchi 2020 §4.2, orient3D_TTEE.
fn orient3d_ttee(
    ta: &ImplicitPoint,
    tb: &ImplicitPoint,
    e1: &ImplicitPoint,
    e2: &ImplicitPoint,
) -> f64 {
    let (va1, va2, va3, wa1, wa2, wa3, ua1, ua2, ua3) = match ta {
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
        } => (v1, v2, v3, w1, w2, w3, u1, u2, u3),
        _ => unreachable!(),
    };
    let (vb1, vb2, vb3, wb1, wb2, wb3, ub1, ub2, ub3) = match tb {
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
        } => (v1, v2, v3, w1, w2, w3, u1, u2, u3),
        _ => unreachable!(),
    };
    let e1c = match e1 {
        ImplicitPoint::Explicit(coords) => coords,
        _ => unreachable!(),
    };
    let e2c = match e2 {
        ImplicitPoint::Explicit(coords) => coords,
        _ => unreachable!(),
    };
    orient3d_ttee_exact(
        va1, va2, va3, wa1, wa2, wa3, ua1, ua2, ua3, vb1, vb2, vb3, wb1, wb2, wb3, ub1, ub2, ub3,
        e1c, e2c,
    )
}

/// Exact orient3d_TTEE — 2 TPI + 2 Explicit rows.
#[allow(clippy::too_many_arguments)]
fn orient3d_ttee_exact(
    va1: &[f64; 3],
    va2: &[f64; 3],
    va3: &[f64; 3],
    wa1: &[f64; 3],
    wa2: &[f64; 3],
    wa3: &[f64; 3],
    ua1: &[f64; 3],
    ua2: &[f64; 3],
    ua3: &[f64; 3],
    vb1: &[f64; 3],
    vb2: &[f64; 3],
    vb3: &[f64; 3],
    wb1: &[f64; 3],
    wb2: &[f64; 3],
    wb3: &[f64; 3],
    ub1: &[f64; 3],
    ub2: &[f64; 3],
    ub3: &[f64; 3],
    e1: &[f64; 3],
    e2: &[f64; 3],
) -> f64 {
    let (sa_d, ra_row) = match orient3d_row_tpi(va1, va2, va3, wa1, wa2, wa3, ua1, ua2, ua3) {
        Some(v) => v,
        None => return 0.0,
    };
    let (sb_d, rb_row) = match orient3d_row_tpi(vb1, vb2, vb3, wb1, wb2, wb3, ub1, ub2, ub3) {
        Some(v) => v,
        None => return 0.0,
    };
    let r1_row = orient3d_row_explicit(e1);
    let r2_row = orient3d_row_explicit(e2);
    orient3d_combine([&ra_row, &rb_row, &r1_row, &r2_row], [sa_d, sb_d, 1, 1])
}

// ── orient3d_LTTE: 1 LPI + 2 TPI + 1 Explicit ───────────────────────────

/// True indirect orient3d for (LPI, TPI, TPI, Explicit).
///
/// Filter constants for orient3d TPI variants are not in our reference doc —
/// filter deferred — always exact for now.
///
/// Ref: Cherchi 2020 §4.2, orient3D_LTTE.
fn orient3d_ltte(
    l: &ImplicitPoint,
    ta: &ImplicitPoint,
    tb: &ImplicitPoint,
    e: &ImplicitPoint,
) -> f64 {
    let (q1, q2, r, s, tt) = match l {
        ImplicitPoint::LPI { q1, q2, r, s, t } => (q1, q2, r, s, t),
        _ => unreachable!(),
    };
    let (va1, va2, va3, wa1, wa2, wa3, ua1, ua2, ua3) = match ta {
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
        } => (v1, v2, v3, w1, w2, w3, u1, u2, u3),
        _ => unreachable!(),
    };
    let (vb1, vb2, vb3, wb1, wb2, wb3, ub1, ub2, ub3) = match tb {
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
        } => (v1, v2, v3, w1, w2, w3, u1, u2, u3),
        _ => unreachable!(),
    };
    let ec = match e {
        ImplicitPoint::Explicit(coords) => coords,
        _ => unreachable!(),
    };
    orient3d_ltte_exact(
        q1, q2, r, s, tt, va1, va2, va3, wa1, wa2, wa3, ua1, ua2, ua3, vb1, vb2, vb3, wb1, wb2,
        wb3, ub1, ub2, ub3, ec,
    )
}

/// Exact orient3d_LTTE — 1 LPI + 2 TPI + 1 Explicit rows.
#[allow(clippy::too_many_arguments)]
fn orient3d_ltte_exact(
    q1: &[f64; 3],
    q2: &[f64; 3],
    r: &[f64; 3],
    s: &[f64; 3],
    tt: &[f64; 3],
    va1: &[f64; 3],
    va2: &[f64; 3],
    va3: &[f64; 3],
    wa1: &[f64; 3],
    wa2: &[f64; 3],
    wa3: &[f64; 3],
    ua1: &[f64; 3],
    ua2: &[f64; 3],
    ua3: &[f64; 3],
    vb1: &[f64; 3],
    vb2: &[f64; 3],
    vb3: &[f64; 3],
    wb1: &[f64; 3],
    wb2: &[f64; 3],
    wb3: &[f64; 3],
    ub1: &[f64; 3],
    ub2: &[f64; 3],
    ub3: &[f64; 3],
    e: &[f64; 3],
) -> f64 {
    let (sl_d, rl_row) = match orient3d_row_lpi(q1, q2, r, s, tt) {
        Some(v) => v,
        None => return 0.0,
    };
    let (sa_d, ra_row) = match orient3d_row_tpi(va1, va2, va3, wa1, wa2, wa3, ua1, ua2, ua3) {
        Some(v) => v,
        None => return 0.0,
    };
    let (sb_d, rb_row) = match orient3d_row_tpi(vb1, vb2, vb3, wb1, wb2, wb3, ub1, ub2, ub3) {
        Some(v) => v,
        None => return 0.0,
    };
    let re_row = orient3d_row_explicit(e);
    orient3d_combine([&rl_row, &ra_row, &rb_row, &re_row], [sl_d, sa_d, sb_d, 1])
}

// ── orient3d_LLTT: 2 LPI + 2 TPI ────────────────────────────────────────

/// True indirect orient3d for (LPI, LPI, TPI, TPI).
///
/// Filter constants for orient3d TPI variants are not in our reference doc —
/// filter deferred — always exact for now.
///
/// Ref: Cherchi 2020 §4.2, orient3D_LLTT.
fn orient3d_lltt(
    la: &ImplicitPoint,
    lb: &ImplicitPoint,
    ta: &ImplicitPoint,
    tb: &ImplicitPoint,
) -> f64 {
    let (q1a, q2a, ra, sa, ta_pl) = match la {
        ImplicitPoint::LPI { q1, q2, r, s, t } => (q1, q2, r, s, t),
        _ => unreachable!(),
    };
    let (q1b, q2b, rb, sb, tb_pl) = match lb {
        ImplicitPoint::LPI { q1, q2, r, s, t } => (q1, q2, r, s, t),
        _ => unreachable!(),
    };
    let (va1, va2, va3, wa1, wa2, wa3, ua1, ua2, ua3) = match ta {
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
        } => (v1, v2, v3, w1, w2, w3, u1, u2, u3),
        _ => unreachable!(),
    };
    let (vb1, vb2, vb3, wb1, wb2, wb3, ub1, ub2, ub3) = match tb {
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
        } => (v1, v2, v3, w1, w2, w3, u1, u2, u3),
        _ => unreachable!(),
    };
    orient3d_lltt_exact(
        q1a, q2a, ra, sa, ta_pl, q1b, q2b, rb, sb, tb_pl, va1, va2, va3, wa1, wa2, wa3, ua1, ua2,
        ua3, vb1, vb2, vb3, wb1, wb2, wb3, ub1, ub2, ub3,
    )
}

/// Exact orient3d_LLTT — 2 LPI + 2 TPI rows.
#[allow(clippy::too_many_arguments)]
fn orient3d_lltt_exact(
    q1a: &[f64; 3],
    q2a: &[f64; 3],
    ra: &[f64; 3],
    sa: &[f64; 3],
    ta_pl: &[f64; 3],
    q1b: &[f64; 3],
    q2b: &[f64; 3],
    rb: &[f64; 3],
    sb: &[f64; 3],
    tb_pl: &[f64; 3],
    va1: &[f64; 3],
    va2: &[f64; 3],
    va3: &[f64; 3],
    wa1: &[f64; 3],
    wa2: &[f64; 3],
    wa3: &[f64; 3],
    ua1: &[f64; 3],
    ua2: &[f64; 3],
    ua3: &[f64; 3],
    vb1: &[f64; 3],
    vb2: &[f64; 3],
    vb3: &[f64; 3],
    wb1: &[f64; 3],
    wb2: &[f64; 3],
    wb3: &[f64; 3],
    ub1: &[f64; 3],
    ub2: &[f64; 3],
    ub3: &[f64; 3],
) -> f64 {
    let (sla_d, rla_row) = match orient3d_row_lpi(q1a, q2a, ra, sa, ta_pl) {
        Some(v) => v,
        None => return 0.0,
    };
    let (slb_d, rlb_row) = match orient3d_row_lpi(q1b, q2b, rb, sb, tb_pl) {
        Some(v) => v,
        None => return 0.0,
    };
    let (sta_d, rta_row) = match orient3d_row_tpi(va1, va2, va3, wa1, wa2, wa3, ua1, ua2, ua3) {
        Some(v) => v,
        None => return 0.0,
    };
    let (stb_d, rtb_row) = match orient3d_row_tpi(vb1, vb2, vb3, wb1, wb2, wb3, ub1, ub2, ub3) {
        Some(v) => v,
        None => return 0.0,
    };
    orient3d_combine(
        [&rla_row, &rlb_row, &rta_row, &rtb_row],
        [sla_d, slb_d, sta_d, stb_d],
    )
}

// ── orient3d_LTTT: 1 LPI + 3 TPI ────────────────────────────────────────

/// True indirect orient3d for (LPI, TPI, TPI, TPI).
///
/// Filter constants for orient3d TPI variants are not in our reference doc —
/// filter deferred — always exact for now.
///
/// Ref: Cherchi 2020 §4.2, orient3D_LTTT.
fn orient3d_lttt(
    l: &ImplicitPoint,
    ta: &ImplicitPoint,
    tb: &ImplicitPoint,
    tc: &ImplicitPoint,
) -> f64 {
    let (q1, q2, r, s, tt) = match l {
        ImplicitPoint::LPI { q1, q2, r, s, t } => (q1, q2, r, s, t),
        _ => unreachable!(),
    };
    let (va1, va2, va3, wa1, wa2, wa3, ua1, ua2, ua3) = match ta {
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
        } => (v1, v2, v3, w1, w2, w3, u1, u2, u3),
        _ => unreachable!(),
    };
    let (vb1, vb2, vb3, wb1, wb2, wb3, ub1, ub2, ub3) = match tb {
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
        } => (v1, v2, v3, w1, w2, w3, u1, u2, u3),
        _ => unreachable!(),
    };
    let (vc1, vc2, vc3, wc1, wc2, wc3, uc1, uc2, uc3) = match tc {
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
        } => (v1, v2, v3, w1, w2, w3, u1, u2, u3),
        _ => unreachable!(),
    };
    orient3d_lttt_exact(
        q1, q2, r, s, tt, va1, va2, va3, wa1, wa2, wa3, ua1, ua2, ua3, vb1, vb2, vb3, wb1, wb2,
        wb3, ub1, ub2, ub3, vc1, vc2, vc3, wc1, wc2, wc3, uc1, uc2, uc3,
    )
}

/// Exact orient3d_LTTT — 1 LPI + 3 TPI rows.
#[allow(clippy::too_many_arguments)]
fn orient3d_lttt_exact(
    q1: &[f64; 3],
    q2: &[f64; 3],
    r: &[f64; 3],
    s: &[f64; 3],
    tt: &[f64; 3],
    va1: &[f64; 3],
    va2: &[f64; 3],
    va3: &[f64; 3],
    wa1: &[f64; 3],
    wa2: &[f64; 3],
    wa3: &[f64; 3],
    ua1: &[f64; 3],
    ua2: &[f64; 3],
    ua3: &[f64; 3],
    vb1: &[f64; 3],
    vb2: &[f64; 3],
    vb3: &[f64; 3],
    wb1: &[f64; 3],
    wb2: &[f64; 3],
    wb3: &[f64; 3],
    ub1: &[f64; 3],
    ub2: &[f64; 3],
    ub3: &[f64; 3],
    vc1: &[f64; 3],
    vc2: &[f64; 3],
    vc3: &[f64; 3],
    wc1: &[f64; 3],
    wc2: &[f64; 3],
    wc3: &[f64; 3],
    uc1: &[f64; 3],
    uc2: &[f64; 3],
    uc3: &[f64; 3],
) -> f64 {
    let (sl_d, rl_row) = match orient3d_row_lpi(q1, q2, r, s, tt) {
        Some(v) => v,
        None => return 0.0,
    };
    let (sa_d, ra_row) = match orient3d_row_tpi(va1, va2, va3, wa1, wa2, wa3, ua1, ua2, ua3) {
        Some(v) => v,
        None => return 0.0,
    };
    let (sb_d, rb_row) = match orient3d_row_tpi(vb1, vb2, vb3, wb1, wb2, wb3, ub1, ub2, ub3) {
        Some(v) => v,
        None => return 0.0,
    };
    let (sc_d, rc_row) = match orient3d_row_tpi(vc1, vc2, vc3, wc1, wc2, wc3, uc1, uc2, uc3) {
        Some(v) => v,
        None => return 0.0,
    };
    orient3d_combine(
        [&rl_row, &ra_row, &rb_row, &rc_row],
        [sl_d, sa_d, sb_d, sc_d],
    )
}

// ── orient3d_TTTE: 3 TPI + 1 Explicit ───────────────────────────────────

/// True indirect orient3d for (TPI, TPI, TPI, Explicit).
///
/// Filter constants for orient3d TPI variants are not in our reference doc —
/// filter deferred — always exact for now.
///
/// Ref: Cherchi 2020 §4.2, orient3D_TTTE.
fn orient3d_ttte(
    ta: &ImplicitPoint,
    tb: &ImplicitPoint,
    tc: &ImplicitPoint,
    e: &ImplicitPoint,
) -> f64 {
    let (va1, va2, va3, wa1, wa2, wa3, ua1, ua2, ua3) = match ta {
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
        } => (v1, v2, v3, w1, w2, w3, u1, u2, u3),
        _ => unreachable!(),
    };
    let (vb1, vb2, vb3, wb1, wb2, wb3, ub1, ub2, ub3) = match tb {
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
        } => (v1, v2, v3, w1, w2, w3, u1, u2, u3),
        _ => unreachable!(),
    };
    let (vc1, vc2, vc3, wc1, wc2, wc3, uc1, uc2, uc3) = match tc {
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
        } => (v1, v2, v3, w1, w2, w3, u1, u2, u3),
        _ => unreachable!(),
    };
    let ec = match e {
        ImplicitPoint::Explicit(coords) => coords,
        _ => unreachable!(),
    };
    orient3d_ttte_exact(
        va1, va2, va3, wa1, wa2, wa3, ua1, ua2, ua3, vb1, vb2, vb3, wb1, wb2, wb3, ub1, ub2, ub3,
        vc1, vc2, vc3, wc1, wc2, wc3, uc1, uc2, uc3, ec,
    )
}

/// Exact orient3d_TTTE — 3 TPI + 1 Explicit rows.
#[allow(clippy::too_many_arguments)]
fn orient3d_ttte_exact(
    va1: &[f64; 3],
    va2: &[f64; 3],
    va3: &[f64; 3],
    wa1: &[f64; 3],
    wa2: &[f64; 3],
    wa3: &[f64; 3],
    ua1: &[f64; 3],
    ua2: &[f64; 3],
    ua3: &[f64; 3],
    vb1: &[f64; 3],
    vb2: &[f64; 3],
    vb3: &[f64; 3],
    wb1: &[f64; 3],
    wb2: &[f64; 3],
    wb3: &[f64; 3],
    ub1: &[f64; 3],
    ub2: &[f64; 3],
    ub3: &[f64; 3],
    vc1: &[f64; 3],
    vc2: &[f64; 3],
    vc3: &[f64; 3],
    wc1: &[f64; 3],
    wc2: &[f64; 3],
    wc3: &[f64; 3],
    uc1: &[f64; 3],
    uc2: &[f64; 3],
    uc3: &[f64; 3],
    e: &[f64; 3],
) -> f64 {
    let (sa_d, ra_row) = match orient3d_row_tpi(va1, va2, va3, wa1, wa2, wa3, ua1, ua2, ua3) {
        Some(v) => v,
        None => return 0.0,
    };
    let (sb_d, rb_row) = match orient3d_row_tpi(vb1, vb2, vb3, wb1, wb2, wb3, ub1, ub2, ub3) {
        Some(v) => v,
        None => return 0.0,
    };
    let (sc_d, rc_row) = match orient3d_row_tpi(vc1, vc2, vc3, wc1, wc2, wc3, uc1, uc2, uc3) {
        Some(v) => v,
        None => return 0.0,
    };
    let re_row = orient3d_row_explicit(e);
    orient3d_combine([&ra_row, &rb_row, &rc_row, &re_row], [sa_d, sb_d, sc_d, 1])
}

// ── orient3d_TTTT: 4 TPI ────────────────────────────────────────────────

/// True indirect orient3d for (TPI, TPI, TPI, TPI).
///
/// Filter constants for orient3d TPI variants are not in our reference doc —
/// filter deferred — always exact for now.
///
/// Ref: Cherchi 2020 §4.2, orient3D_TTTT.
fn orient3d_tttt(
    ta: &ImplicitPoint,
    tb: &ImplicitPoint,
    tc: &ImplicitPoint,
    td: &ImplicitPoint,
) -> f64 {
    let (va1, va2, va3, wa1, wa2, wa3, ua1, ua2, ua3) = match ta {
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
        } => (v1, v2, v3, w1, w2, w3, u1, u2, u3),
        _ => unreachable!(),
    };
    let (vb1, vb2, vb3, wb1, wb2, wb3, ub1, ub2, ub3) = match tb {
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
        } => (v1, v2, v3, w1, w2, w3, u1, u2, u3),
        _ => unreachable!(),
    };
    let (vc1, vc2, vc3, wc1, wc2, wc3, uc1, uc2, uc3) = match tc {
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
        } => (v1, v2, v3, w1, w2, w3, u1, u2, u3),
        _ => unreachable!(),
    };
    let (vd1, vd2, vd3, wd1, wd2, wd3, ud1, ud2, ud3) = match td {
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
        } => (v1, v2, v3, w1, w2, w3, u1, u2, u3),
        _ => unreachable!(),
    };
    orient3d_tttt_exact(
        va1, va2, va3, wa1, wa2, wa3, ua1, ua2, ua3, vb1, vb2, vb3, wb1, wb2, wb3, ub1, ub2, ub3,
        vc1, vc2, vc3, wc1, wc2, wc3, uc1, uc2, uc3, vd1, vd2, vd3, wd1, wd2, wd3, ud1, ud2, ud3,
    )
}

/// Exact orient3d_TTTT — 4 TPI rows.
#[allow(clippy::too_many_arguments)]
fn orient3d_tttt_exact(
    va1: &[f64; 3],
    va2: &[f64; 3],
    va3: &[f64; 3],
    wa1: &[f64; 3],
    wa2: &[f64; 3],
    wa3: &[f64; 3],
    ua1: &[f64; 3],
    ua2: &[f64; 3],
    ua3: &[f64; 3],
    vb1: &[f64; 3],
    vb2: &[f64; 3],
    vb3: &[f64; 3],
    wb1: &[f64; 3],
    wb2: &[f64; 3],
    wb3: &[f64; 3],
    ub1: &[f64; 3],
    ub2: &[f64; 3],
    ub3: &[f64; 3],
    vc1: &[f64; 3],
    vc2: &[f64; 3],
    vc3: &[f64; 3],
    wc1: &[f64; 3],
    wc2: &[f64; 3],
    wc3: &[f64; 3],
    uc1: &[f64; 3],
    uc2: &[f64; 3],
    uc3: &[f64; 3],
    vd1: &[f64; 3],
    vd2: &[f64; 3],
    vd3: &[f64; 3],
    wd1: &[f64; 3],
    wd2: &[f64; 3],
    wd3: &[f64; 3],
    ud1: &[f64; 3],
    ud2: &[f64; 3],
    ud3: &[f64; 3],
) -> f64 {
    let (sa_d, ra_row) = match orient3d_row_tpi(va1, va2, va3, wa1, wa2, wa3, ua1, ua2, ua3) {
        Some(v) => v,
        None => return 0.0,
    };
    let (sb_d, rb_row) = match orient3d_row_tpi(vb1, vb2, vb3, wb1, wb2, wb3, ub1, ub2, ub3) {
        Some(v) => v,
        None => return 0.0,
    };
    let (sc_d, rc_row) = match orient3d_row_tpi(vc1, vc2, vc3, wc1, wc2, wc3, uc1, uc2, uc3) {
        Some(v) => v,
        None => return 0.0,
    };
    let (sd_d, rd_row) = match orient3d_row_tpi(vd1, vd2, vd3, wd1, wd2, wd3, ud1, ud2, ud3) {
        Some(v) => v,
        None => return 0.0,
    };
    orient3d_combine(
        [&ra_row, &rb_row, &rc_row, &rd_row],
        [sa_d, sb_d, sc_d, sd_d],
    )
}

// ── orient3d_LLLT: 3 LPI + 1 TPI ────────────────────────────────────────

/// True indirect orient3d for (LPI, LPI, LPI, TPI).
///
/// Filter constants for orient3d TPI variants are not in our reference doc —
/// filter deferred — always exact for now.
///
/// Ref: Cherchi 2020 §4.2, orient3D_LLLT.
fn orient3d_lllt(
    la: &ImplicitPoint,
    lb: &ImplicitPoint,
    lc: &ImplicitPoint,
    t: &ImplicitPoint,
) -> f64 {
    let (q1a, q2a, ra, sa, ta) = match la {
        ImplicitPoint::LPI { q1, q2, r, s, t } => (q1, q2, r, s, t),
        _ => unreachable!(),
    };
    let (q1b, q2b, rb, sb, tb) = match lb {
        ImplicitPoint::LPI { q1, q2, r, s, t } => (q1, q2, r, s, t),
        _ => unreachable!(),
    };
    let (q1c, q2c, rc, sc, tc) = match lc {
        ImplicitPoint::LPI { q1, q2, r, s, t } => (q1, q2, r, s, t),
        _ => unreachable!(),
    };
    let (v1, v2, v3, w1, w2, w3, u1, u2, u3) = match t {
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
        } => (v1, v2, v3, w1, w2, w3, u1, u2, u3),
        _ => unreachable!(),
    };
    orient3d_lllt_exact(
        q1a, q2a, ra, sa, ta, q1b, q2b, rb, sb, tb, q1c, q2c, rc, sc, tc, v1, v2, v3, w1, w2, w3,
        u1, u2, u3,
    )
}

/// Exact orient3d_LLLT — 3 LPI + 1 TPI rows.
#[allow(clippy::too_many_arguments)]
fn orient3d_lllt_exact(
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
    let (sla_d, rla_row) = match orient3d_row_lpi(q1a, q2a, ra, sa, ta) {
        Some(v) => v,
        None => return 0.0,
    };
    let (slb_d, rlb_row) = match orient3d_row_lpi(q1b, q2b, rb, sb, tb) {
        Some(v) => v,
        None => return 0.0,
    };
    let (slc_d, rlc_row) = match orient3d_row_lpi(q1c, q2c, rc, sc, tc) {
        Some(v) => v,
        None => return 0.0,
    };
    let (st_d, rt_row) = match orient3d_row_tpi(v1, v2, v3, w1, w2, w3, u1, u2, u3) {
        Some(v) => v,
        None => return 0.0,
    };
    orient3d_combine(
        [&rla_row, &rlb_row, &rlc_row, &rt_row],
        [sla_d, slb_d, slc_d, st_d],
    )
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

    // Dispatch on point types. The 9 (E,L,T) × (E,L,T) ordered cases below
    // are exhaustive, so the trailing `_` materialize fallback is unreachable
    // by structural matching today. It is kept intentionally as a safety net
    // during the cutover from materialize-fallback to true-indirect dispatch:
    // if a future ImplicitPoint variant is added, callers fall back to
    // materialize rather than panicking. Deletion is deferred to PR2 once
    // assay reports zero fallback hits — see specs/cherchi_indirect_predicates.md.
    #[allow(unreachable_patterns)]
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

        // TE: TPI vs Explicit — true indirect (Cherchi 2020 §4.3, Phase D)
        (
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
            },
            ImplicitPoint::Explicit(e),
        ) => point_compare_te(v1, v2, v3, w1, w2, w3, u1, u2, u3, e, idx),

        // ET: Explicit vs TPI — reverse the comparison
        (
            ImplicitPoint::Explicit(e),
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
            },
        ) => point_compare_te(v1, v2, v3, w1, w2, w3, u1, u2, u3, e, idx).reverse(),

        // LT: LPI vs TPI — true indirect (Cherchi 2020 §4.3, Phase D)
        (
            ImplicitPoint::LPI { q1, q2, r, s, t },
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
            },
        ) => point_compare_lt(q1, q2, r, s, t, v1, v2, v3, w1, w2, w3, u1, u2, u3, idx),

        // TL: TPI vs LPI — reverse the comparison
        (
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
            },
            ImplicitPoint::LPI { q1, q2, r, s, t },
        ) => point_compare_lt(q1, q2, r, s, t, v1, v2, v3, w1, w2, w3, u1, u2, u3, idx).reverse(),

        // TT: two TPIs — true indirect (Cherchi 2020 §4.3, Phase D)
        (
            ImplicitPoint::TPI {
                v1: va1,
                v2: va2,
                v3: va3,
                w1: wa1,
                w2: wa2,
                w3: wa3,
                u1: ua1,
                u2: ua2,
                u3: ua3,
            },
            ImplicitPoint::TPI {
                v1: vb1,
                v2: vb2,
                v3: vb3,
                w1: wb1,
                w2: wb2,
                w3: wb3,
                u1: ub1,
                u2: ub2,
                u3: ub3,
            },
        ) => point_compare_tt(
            va1, va2, va3, wa1, wa2, wa3, ua1, ua2, ua3, vb1, vb2, vb3, wb1, wb2, wb3, ub1, ub2,
            ub3, idx,
        ),

        // All remaining cases: materialize fallback (kept as a safety net while
        // T-point dispatch matures; deletion deferred to PR2 once assay reports
        // zero fallback hits — see specs/cherchi_indirect_predicates.md).
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

// ── Point comparison: T-point variants (Cherchi 2020 §4.3) ──────────────
//
// The three predicates below are direct analogues of `point_compare_le` and
// `point_compare_ll`, with one or both LPI points swapped for TPI points.
// All three are expansion-only (no Stage 1 float filter); the Cherchi 2020
// Table 1 filter constants are noted in each function's doc comment and are
// reserved for a future PR (see `specs/cherchi_indirect_predicates.md`).

/// True indirect comparison: TPI vs Explicit on a single coordinate axis.
///
/// `pT[idx] = λ_T[idx] / d_T` compared with `e[idx]`. The sign of the
/// difference is `sign(d_T) * sign(λ_T[idx] - d_T * e[idx])`.
///
/// Filter constant (deferred): epsilon = 3.98e-13 · delta^7 (Cherchi 2020
/// Table 1, pointCompare_TE). This stub always uses the exact path.
///
/// Ref: Cherchi 2020 §4.3, pointCompare_on_X_TE.
#[allow(clippy::too_many_arguments)]
fn point_compare_te(
    v1: &[f64; 3],
    v2: &[f64; 3],
    v3: &[f64; 3],
    w1: &[f64; 3],
    w2: &[f64; 3],
    w3: &[f64; 3],
    u1: &[f64; 3],
    u2: &[f64; 3],
    u3: &[f64; 3],
    e: &[f64; 3],
    idx: usize,
) -> std::cmp::Ordering {
    point_compare_te_exact(v1, v2, v3, w1, w2, w3, u1, u2, u3, e, idx)
}

/// Exact TE point comparison using expansion arithmetic.
///
/// Uses `tpi_lambda_expansion` so all subtractions and products are exact.
#[allow(clippy::too_many_arguments)]
fn point_compare_te_exact(
    v1: &[f64; 3],
    v2: &[f64; 3],
    v3: &[f64; 3],
    w1: &[f64; 3],
    w2: &[f64; 3],
    w3: &[f64; 3],
    u1: &[f64; 3],
    u2: &[f64; 3],
    u3: &[f64; 3],
    e: &[f64; 3],
    idx: usize,
) -> std::cmp::Ordering {
    let (d_t_sign, d_t_exp, lx, ly, lz) =
        match tpi_lambda_expansion(v1, v2, v3, w1, w2, w3, u1, u2, u3) {
            Some(v) => v,
            None => return std::cmp::Ordering::Equal,
        };
    let lambda = [lx, ly, lz];

    // kx = λ[idx] - d_T * e[idx]
    let d_t_e = expansion_scale(&d_t_exp, e[idx]);
    let kx_exp = expansion_add(&lambda[idx], &expansion_negate(&d_t_e));

    let kx_sign = expansion_sign(&kx_exp);
    let combined = d_t_sign * kx_sign;
    match combined {
        x if x > 0 => std::cmp::Ordering::Greater,
        x if x < 0 => std::cmp::Ordering::Less,
        _ => std::cmp::Ordering::Equal,
    }
}

/// True indirect comparison: LPI vs TPI on a single coordinate axis.
///
/// `pL[idx] = λ_L[idx] / d_L` vs `pT[idx] = λ_T[idx] / d_T`. The sign of the
/// difference is `sign(d_L) * sign(d_T) * sign(d_T * λ_L[idx] - d_L * λ_T[idx])`.
///
/// Filter constant (deferred): epsilon = 4.32e-12 · delta^10 (Cherchi 2020
/// Table 1, pointCompare_LT). This stub always uses the exact path.
///
/// Ref: Cherchi 2020 §4.3, pointCompare_on_X_LT.
#[allow(clippy::too_many_arguments)]
fn point_compare_lt(
    q1: &[f64; 3],
    q2: &[f64; 3],
    r: &[f64; 3],
    s: &[f64; 3],
    t: &[f64; 3],
    v1: &[f64; 3],
    v2: &[f64; 3],
    v3: &[f64; 3],
    w1: &[f64; 3],
    w2: &[f64; 3],
    w3: &[f64; 3],
    u1: &[f64; 3],
    u2: &[f64; 3],
    u3: &[f64; 3],
    idx: usize,
) -> std::cmp::Ordering {
    point_compare_lt_exact(q1, q2, r, s, t, v1, v2, v3, w1, w2, w3, u1, u2, u3, idx)
}

/// Exact LT point comparison using expansion arithmetic.
#[allow(clippy::too_many_arguments)]
fn point_compare_lt_exact(
    q1: &[f64; 3],
    q2: &[f64; 3],
    r: &[f64; 3],
    s: &[f64; 3],
    t: &[f64; 3],
    v1: &[f64; 3],
    v2: &[f64; 3],
    v3: &[f64; 3],
    w1: &[f64; 3],
    w2: &[f64; 3],
    w3: &[f64; 3],
    u1: &[f64; 3],
    u2: &[f64; 3],
    u3: &[f64; 3],
    idx: usize,
) -> std::cmp::Ordering {
    let (d_l_sign, d_l_exp, lx, ly, lz) = match lpi_lambda_expansion(q1, q2, r, s, t) {
        Some(v) => v,
        None => return std::cmp::Ordering::Equal,
    };
    let (d_t_sign, d_t_exp, tx, ty, tz) =
        match tpi_lambda_expansion(v1, v2, v3, w1, w2, w3, u1, u2, u3) {
            Some(v) => v,
            None => return std::cmp::Ordering::Equal,
        };

    let l = [lx, ly, lz];
    let t_lambda = [tx, ty, tz];

    // diff = d_T * λ_L[idx] - d_L * λ_T[idx]
    let diff_exp = expansion_add(
        &expansion_mul_expansion(&d_t_exp, &l[idx]),
        &expansion_negate(&expansion_mul_expansion(&d_l_exp, &t_lambda[idx])),
    );

    let diff_sign = expansion_sign(&diff_exp);
    let combined = d_l_sign * d_t_sign * diff_sign;
    match combined {
        x if x > 0 => std::cmp::Ordering::Greater,
        x if x < 0 => std::cmp::Ordering::Less,
        _ => std::cmp::Ordering::Equal,
    }
}

/// True indirect comparison: TPI_a vs TPI_b on a single coordinate axis.
///
/// `pTa[idx] = λ_Ta[idx] / d_Ta` vs `pTb[idx] = λ_Tb[idx] / d_Tb`. The sign
/// of the difference is `sign(d_Ta) * sign(d_Tb) * sign(d_Tb * λ_Ta[idx] -
/// d_Ta * λ_Tb[idx])`.
///
/// Filter constant (deferred): epsilon = 5.50e-11 · delta^13 (Cherchi 2020
/// Table 1, pointCompare_TT). This stub always uses the exact path.
///
/// Ref: Cherchi 2020 §4.3, pointCompare_on_X_TT.
#[allow(clippy::too_many_arguments)]
fn point_compare_tt(
    va1: &[f64; 3],
    va2: &[f64; 3],
    va3: &[f64; 3],
    wa1: &[f64; 3],
    wa2: &[f64; 3],
    wa3: &[f64; 3],
    ua1: &[f64; 3],
    ua2: &[f64; 3],
    ua3: &[f64; 3],
    vb1: &[f64; 3],
    vb2: &[f64; 3],
    vb3: &[f64; 3],
    wb1: &[f64; 3],
    wb2: &[f64; 3],
    wb3: &[f64; 3],
    ub1: &[f64; 3],
    ub2: &[f64; 3],
    ub3: &[f64; 3],
    idx: usize,
) -> std::cmp::Ordering {
    point_compare_tt_exact(
        va1, va2, va3, wa1, wa2, wa3, ua1, ua2, ua3, vb1, vb2, vb3, wb1, wb2, wb3, ub1, ub2, ub3,
        idx,
    )
}

/// Exact TT point comparison using expansion arithmetic.
#[allow(clippy::too_many_arguments)]
fn point_compare_tt_exact(
    va1: &[f64; 3],
    va2: &[f64; 3],
    va3: &[f64; 3],
    wa1: &[f64; 3],
    wa2: &[f64; 3],
    wa3: &[f64; 3],
    ua1: &[f64; 3],
    ua2: &[f64; 3],
    ua3: &[f64; 3],
    vb1: &[f64; 3],
    vb2: &[f64; 3],
    vb3: &[f64; 3],
    wb1: &[f64; 3],
    wb2: &[f64; 3],
    wb3: &[f64; 3],
    ub1: &[f64; 3],
    ub2: &[f64; 3],
    ub3: &[f64; 3],
    idx: usize,
) -> std::cmp::Ordering {
    let (da_sign, da_exp, ax, ay, az) =
        match tpi_lambda_expansion(va1, va2, va3, wa1, wa2, wa3, ua1, ua2, ua3) {
            Some(v) => v,
            None => return std::cmp::Ordering::Equal,
        };
    let (db_sign, db_exp, bx, by, bz) =
        match tpi_lambda_expansion(vb1, vb2, vb3, wb1, wb2, wb3, ub1, ub2, ub3) {
            Some(v) => v,
            None => return std::cmp::Ordering::Equal,
        };

    let la = [ax, ay, az];
    let lb = [bx, by, bz];

    // diff = d_Tb * λ_Ta[idx] - d_Ta * λ_Tb[idx]
    let diff_exp = expansion_add(
        &expansion_mul_expansion(&db_exp, &la[idx]),
        &expansion_negate(&expansion_mul_expansion(&da_exp, &lb[idx])),
    );

    let diff_sign = expansion_sign(&diff_exp);
    let combined = da_sign * db_sign * diff_sign;
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

    // ── TPI lambda expansion tests (Phase A) ────────────────────────

    /// Validate `tpi_lambda_expansion` against `materialize_tpi` on three
    /// perpendicular planes meeting at (1,0,0):
    ///   plane z=0 (triangle in XY through origin)
    ///   plane x=1 (triangle in YZ at x=1)
    ///   plane y=0 (triangle in XZ through origin)
    ///
    /// Expected exact intersection: (1, 0, 0). With this construction,
    /// d_T = ±1 and λ = (±1, 0, 0); ratio λ/d_T = (1, 0, 0).
    #[test]
    fn test_tpi_lambda_expansion_matches_materialize() {
        // Triangle (v) on plane z=0
        let v1 = [0.0, 0.0, 0.0];
        let v2 = [1.0, 0.0, 0.0];
        let v3 = [0.0, 1.0, 0.0];
        // Triangle (w) on plane x=1
        let w1 = [1.0, 0.0, 0.0];
        let w2 = [1.0, 1.0, 0.0];
        let w3 = [1.0, 0.0, 1.0];
        // Triangle (u) on plane y=0
        let u1 = [0.0, 0.0, 0.0];
        let u2 = [0.0, 0.0, 1.0];
        let u3 = [1.0, 0.0, 0.0];

        let (d_sign, d_exp, lx, ly, lz) =
            tpi_lambda_expansion(&v1, &v2, &v3, &w1, &w2, &w3, &u1, &u2, &u3)
                .expect("three perpendicular planes must yield non-zero d_T");
        assert_ne!(d_sign, 0, "d_T sign must be nonzero for independent planes");

        // Materialize via division and compare with tpi materialize.
        let mat = materialize_tpi(&v1, &v2, &v3, &w1, &w2, &w3, &u1, &u2, &u3)
            .expect("materialize_tpi must succeed for independent planes");

        // Reconstruct each coordinate from expansions: λ_k summed / d_T summed.
        // For Shewchuk expansions, the sum of components is the exact value
        // representable in f64 — for these small-integer inputs the result is exact.
        let d_t: f64 = d_exp.iter().sum();
        let lx_val: f64 = lx.iter().sum();
        let ly_val: f64 = ly.iter().sum();
        let lz_val: f64 = lz.iter().sum();
        let recon = [lx_val / d_t, ly_val / d_t, lz_val / d_t];

        // Recon should match materialize within tight tolerance, and both
        // should be (1, 0, 0).
        for k in 0..3 {
            assert!(
                (recon[k] - mat[k]).abs() < 1e-12,
                "axis {k}: recon={}, mat={}",
                recon[k],
                mat[k]
            );
        }
        assert!(
            (recon[0] - 1.0).abs() < 1e-12,
            "x should be 1, got {}",
            recon[0]
        );
        assert!(recon[1].abs() < 1e-12, "y should be 0, got {}", recon[1]);
        assert!(recon[2].abs() < 1e-12, "z should be 0, got {}", recon[2]);

        // Sign of d_T from expansion must agree with sign of f64 d_T.
        let d_t_f64 = det3x3_tpi(&v1, &v2, &v3, &w1, &w2, &w3, &u1, &u2, &u3);
        assert_eq!(
            d_sign,
            if d_t_f64 > 0.0 {
                1
            } else if d_t_f64 < 0.0 {
                -1
            } else {
                0
            },
            "d_T sign from expansion must match f64 sign",
        );
    }

    /// `tpi_lambda_expansion` returns `None` for three coplanar (parallel) triangles.
    /// All three on z=0 → d_T = 0.
    #[test]
    fn test_tpi_lambda_expansion_degenerate() {
        let v1 = [0.0, 0.0, 0.0];
        let v2 = [1.0, 0.0, 0.0];
        let v3 = [0.0, 1.0, 0.0];
        let w1 = [0.0, 0.0, 0.0];
        let w2 = [2.0, 0.0, 0.0];
        let w3 = [0.0, 2.0, 0.0];
        let u1 = [1.0, 1.0, 0.0];
        let u2 = [3.0, 1.0, 0.0];
        let u3 = [1.0, 3.0, 0.0];
        assert!(tpi_lambda_expansion(&v1, &v2, &v3, &w1, &w2, &w3, &u1, &u2, &u3).is_none());
    }

    /// Cross-check `tpi_lambda_expansion` on a tilted/offset case where
    /// materialize is the only ground truth. Verifies sign(d_T) and
    /// reconstructs coords approximately.
    #[test]
    fn test_tpi_lambda_expansion_tilted() {
        let v1 = [0.5, 0.5, 0.0];
        let v2 = [1.5, 0.5, 0.0];
        let v3 = [0.5, 1.5, 0.0];
        // Tilted plane through (0,0,1): triangle with normal (1,1,1)/√3 area
        let w1 = [1.0, 0.0, 0.0];
        let w2 = [0.0, 1.0, 0.0];
        let w3 = [0.0, 0.0, 1.0];
        let u1 = [0.0, -1.0, 0.0];
        let u2 = [1.0, -1.0, 0.0];
        let u3 = [0.0, -1.0, 1.0];

        let (d_sign, d_exp, lx, ly, lz) =
            tpi_lambda_expansion(&v1, &v2, &v3, &w1, &w2, &w3, &u1, &u2, &u3)
                .expect("tilted independent planes");
        let mat = materialize_tpi(&v1, &v2, &v3, &w1, &w2, &w3, &u1, &u2, &u3).expect("");

        let d_t: f64 = d_exp.iter().sum();
        let recon = [
            lx.iter().sum::<f64>() / d_t,
            ly.iter().sum::<f64>() / d_t,
            lz.iter().sum::<f64>() / d_t,
        ];
        for k in 0..3 {
            assert!(
                (recon[k] - mat[k]).abs() < 1e-10,
                "axis {k}: recon={}, mat={}",
                recon[k],
                mat[k]
            );
        }
        let d_t_f64 = det3x3_tpi(&v1, &v2, &v3, &w1, &w2, &w3, &u1, &u2, &u3);
        let expected_sign = if d_t_f64 > 0.0 {
            1
        } else if d_t_f64 < 0.0 {
            -1
        } else {
            0
        };
        assert_eq!(d_sign, expected_sign);
    }

    /// `two_diff_exp` returns `vec![hi]` (1 component) when `lo == 0`,
    /// otherwise `vec![lo, hi]`. Verify `expansion_mul_expansion` handles
    /// this canonicality variation correctly: a 1-component expansion that
    /// happens to be zero (i.e. `vec![0.0]`) and a non-trivial one must
    /// agree with reference computations.
    #[test]
    fn test_two_diff_exp_canonicality_in_mul() {
        // Case 1: equal inputs produce vec![0.0] — multiplying by anything
        // must yield vec![0.0] (a sign-zero expansion).
        let zero_exp = two_diff_exp(3.5, 3.5);
        assert_eq!(
            zero_exp,
            vec![0.0],
            "two_diff(a,a) should canonicalize to vec![0.0]"
        );

        let nontrivial = two_diff_exp(7.0, 1.0); // = 6.0, single component
        let prod = expansion_mul_expansion(&zero_exp, &nontrivial);
        assert_eq!(expansion_sign(&prod), 0, "0 * nontrivial = 0");

        // Case 2: 1-component (lo=0) × 1-component must agree with f64 product.
        // two_diff(7, 1) = [0, 6] → vec![6.0] (1 element).
        let a = two_diff_exp(7.0, 1.0);
        let b = two_diff_exp(5.0, 2.0); // = vec![3.0]
        assert_eq!(a, vec![6.0]);
        assert_eq!(b, vec![3.0]);
        let prod = expansion_mul_expansion(&a, &b);
        let val: f64 = prod.iter().sum();
        assert_eq!(val, 18.0, "6*3 = 18 exactly");
        assert_eq!(expansion_sign(&prod), 1);

        // Case 3: 2-component × 2-component (lo != 0 cases) — exercises
        // the full Shewchuk path.
        // two_diff(0.1, 0.2) requires multiple bits → 2-component.
        let p = two_diff_exp(0.1, 0.2);
        let q = two_diff_exp(0.3, 0.5);
        // Expected: p ≈ -0.1, q ≈ -0.2, p*q ≈ 0.02 — but with expansion exactness.
        let prod = expansion_mul_expansion(&p, &q);
        let val: f64 = prod.iter().sum();
        // p*q is positive (negative × negative), close to 0.02.
        assert!(val > 0.0, "(-0.1)*(-0.2) = positive");
        assert!((val - 0.02).abs() < 1e-15, "got {val}");
        assert_eq!(expansion_sign(&prod), 1);

        // Case 4: 1-component (lo=0) × 2-component — mixing canonicality forms.
        let one_comp = two_diff_exp(10.0, 4.0); // = vec![6.0]
        let two_comp = two_diff_exp(0.1, 0.2); // 2 components, ≈ -0.1
        let prod = expansion_mul_expansion(&one_comp, &two_comp);
        let val: f64 = prod.iter().sum();
        assert!((val - (-0.6)).abs() < 1e-14, "6 * -0.1 ≈ -0.6, got {val}");
        assert_eq!(expansion_sign(&prod), -1);
    }

    /// Sanity check for `cross_sub_expansion` and `det3x3_expansion`:
    /// helper subroutines used by `tpi_lambda_expansion`.
    #[test]
    fn test_cross_sub_expansion_basic() {
        // (v1, v2, v3) = (0,0,0), (1,0,0), (0,1,0) — normal should be (0,0,1).
        let v1 = [0.0, 0.0, 0.0];
        let v2 = [1.0, 0.0, 0.0];
        let v3 = [0.0, 1.0, 0.0];
        let n = cross_sub_expansion(&v1, &v2, &v3);
        let nx: f64 = n[0].iter().sum();
        let ny: f64 = n[1].iter().sum();
        let nz: f64 = n[2].iter().sum();
        // (b-a) × (c-b) = (1,0,0) × (-1,1,0) = (0*0-0*1, 0*(-1)-1*0, 1*1-0*(-1)) = (0,0,1)
        assert_eq!(nx, 0.0);
        assert_eq!(ny, 0.0);
        assert_eq!(nz, 1.0);
    }

    // ── Phase F: property tests + red-phase TPI stress ───────────────
    // Author: test-author. These exercise the dispatch table for orient2d,
    // orient3d, and less_than_indirect across implicit-point type mixes
    // (E/L/T) using a deterministic seeded LCG. The red test
    // (`test_perturbed_coplanar_tpi_exact_is_zero`) is the FIP §8 failing
    // case: it must FAIL with the materialize fallback (current code) and
    // PASS once Phases B/C/D land exact TPI predicates.

    /// Deterministic linear congruential generator for property tests.
    /// Numerical Recipes parameters; not for cryptography, but reproducible
    /// across compilers/platforms (no `rand` dependency needed).
    struct Lcg(u64);
    impl Lcg {
        fn new(seed: u64) -> Self {
            Self(seed)
        }
        fn next_u64(&mut self) -> u64 {
            self.0 = self
                .0
                .wrapping_mul(6364136223846793005)
                .wrapping_add(1442695040888963407);
            self.0
        }
        /// Random integer in `[-range, range]`.
        fn next_int(&mut self, range: i32) -> i32 {
            let n = self.next_u64() % ((2 * range as u64) + 1);
            (n as i32) - range
        }
        /// Uniform f64 in `[-1.0, 1.0]`.
        fn next_unit(&mut self) -> f64 {
            let bits = self.next_u64();
            // Map top 53 bits to [0, 1), then shift to [-1, 1).
            let u = (bits >> 11) as f64 / (1u64 << 53) as f64;
            2.0 * u - 1.0
        }
    }

    /// Build a non-degenerate explicit point with small-integer coords.
    fn rand_explicit(rng: &mut Lcg) -> ImplicitPoint {
        ImplicitPoint::Explicit([
            rng.next_int(5) as f64,
            rng.next_int(5) as f64,
            rng.next_int(5) as f64,
        ])
    }

    /// Build an LPI from a Z-axis edge and a non-vertical plane.
    /// d_L is guaranteed non-zero by construction.
    fn rand_lpi(rng: &mut Lcg) -> ImplicitPoint {
        let qx = rng.next_int(3) as f64;
        let qy = rng.next_int(3) as f64;
        // Edge along z, varying xy
        let q1 = [qx, qy, -1.0 - rng.next_unit().abs()];
        let q2 = [qx, qy, 1.0 + rng.next_unit().abs()];
        // A non-vertical plane (z varies, so the edge is not parallel)
        let r = [1.0, 0.0, 0.0];
        let s = [0.0, 1.0, 0.0];
        let t = [-1.0, -1.0, 0.0];
        ImplicitPoint::LPI { q1, q2, r, s, t }
    }

    /// Build a TPI from three small-integer triangles meeting at a known
    /// (cx, cy, cz). Three axis-aligned planes guarantee d_T != 0.
    fn rand_tpi(rng: &mut Lcg) -> ImplicitPoint {
        let cx = rng.next_int(2) as f64;
        let cy = rng.next_int(2) as f64;
        let cz = rng.next_int(2) as f64;
        let v1 = [cx, 0.0, 0.0];
        let v2 = [cx, 1.0, 0.0];
        let v3 = [cx, 0.0, 1.0];
        let w1 = [0.0, cy, 0.0];
        let w2 = [1.0, cy, 0.0];
        let w3 = [0.0, cy, 1.0];
        let u1 = [0.0, 0.0, cz];
        let u2 = [1.0, 0.0, cz];
        let u3 = [0.0, 1.0, cz];
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
        }
    }

    /// FIP §8 RED TEST — fails with current materialize fallback; must pass
    /// with exact TPI orient3d (Phase C).
    ///
    /// Construction: three planes with linearly independent normals all
    /// passing through P = (1/3, 1/3, 1/3) (the rational point that is NOT
    /// f64-exact). A 4th explicit plane with normal (1,1,-2) also contains P.
    /// Rational arithmetic on the integer-coord input gives the exact answer
    /// orient3d(TPI, e1, e2, e3) = 0.
    ///
    /// With the current materialize fallback: TPI is computed as
    /// (λx/d_T, λy/d_T, λz/d_T) ≈ (0.333..., 0.333..., 0.333...). The cofactor
    /// expansion of the orient3d 4×4 determinant on those f64 values yields a
    /// small nonzero number — the sign is whatever the rounding cascade
    /// produces. The exact path (Phase C) must return 0.
    ///
    /// Plane equations:
    ///   T1: x + y + z = 1     (vertices (1,0,0), (0,1,0), (0,0,1))
    ///   T2: x - y = 0         (vertices (0,0,0), (1,1,0), (0,0,1))
    ///   T3: y - z = 0         (vertices (0,0,0), (1,0,0), (0,1,1))
    /// solving: y = z, x = y, x + y + z = 1 → P = (1/3, 1/3, 1/3).
    ///
    /// 4th plane (containing P): x + y - 2z = 0, vertices
    ///   e1 = (1,1,1)  → 1+1-2 = 0  ✓
    ///   e2 = (2,0,1)  → 2+0-2 = 0  ✓
    ///   e3 = (0,0,0)  → 0          ✓
    /// Rationally, P=(1/3,1/3,1/3) gives 1/3+1/3-2/3 = 0 ✓ — coplanar.
    #[test]
    fn test_perturbed_coplanar_tpi_exact_is_zero() {
        // T1: plane x+y+z=1
        let v1 = [1.0, 0.0, 0.0];
        let v2 = [0.0, 1.0, 0.0];
        let v3 = [0.0, 0.0, 1.0];
        // T2: plane x-y=0
        let w1 = [0.0, 0.0, 0.0];
        let w2 = [1.0, 1.0, 0.0];
        let w3 = [0.0, 0.0, 1.0];
        // T3: plane y-z=0
        let u1 = [0.0, 0.0, 0.0];
        let u2 = [1.0, 0.0, 0.0];
        let u3 = [0.0, 1.0, 1.0];

        let tpi = ImplicitPoint::TPI {
            v1,
            v2,
            v3,
            w1,
            w2,
            w3,
            u1,
            u2,
            u3,
        };
        // 4th plane: x+y-2z = 0, passes through P=(1/3,1/3,1/3).
        let e1 = ImplicitPoint::Explicit([1.0, 1.0, 1.0]);
        let e2 = ImplicitPoint::Explicit([2.0, 0.0, 1.0]);
        let e3 = ImplicitPoint::Explicit([0.0, 0.0, 0.0]);

        // Sanity: TPI is well-defined (d_T != 0).
        assert!(tpi.is_defined(), "TPI must be defined");

        // Sanity: e1, e2, e3 must not be collinear (otherwise orient3d is
        // identically 0 and the test is vacuous).
        let n_xy = orient2d_indirect(&e1, &e2, &e3, ProjectionAxis::XY);
        let n_yz = orient2d_indirect(&e1, &e2, &e3, ProjectionAxis::YZ);
        let n_zx = orient2d_indirect(&e1, &e2, &e3, ProjectionAxis::ZX);
        assert!(
            n_xy != 0.0 || n_yz != 0.0 || n_zx != 0.0,
            "e1, e2, e3 must not be collinear (would make test vacuous)"
        );

        // materialize_tpi gives a near-(1/3, 1/3, 1/3) f64 — this is the
        // ROOT CAUSE of why materialize fallback fails.
        let mat = tpi.materialize().expect("TPI materializes");
        assert!(
            (mat[0] - 1.0 / 3.0).abs() < 1e-15
                && (mat[1] - 1.0 / 3.0).abs() < 1e-15
                && (mat[2] - 1.0 / 3.0).abs() < 1e-15,
            "materialize_tpi ≈ (1/3, 1/3, 1/3): got {mat:?}"
        );

        // The exact answer: orient3d(P, e1, e2, e3) = 0 — TPI is exactly
        // on the 4th plane in rational arithmetic.
        let result = orient3d_indirect(&tpi, &e1, &e2, &e3);
        assert_eq!(
            result, 0.0,
            "orient3d_indirect(TPI=(1/3,1/3,1/3), e1, e2, e3) on 4th plane \
             x+y-2z=0 must be exactly 0 (TPI lies on plane). \
             Materialize fallback rounds to ±epsilon. Got {result}. \
             This is the FIP §8 red test — must pass when Phase C lands \
             exact TPI orient3d predicates."
        );
    }

    /// Property: orient2d antisymmetry over the (E, L, T) dispatch.
    ///
    /// `orient2d(a, b, c) == -orient2d(b, a, c)` for all defined points.
    /// Random tuples with independently chosen point types so that every
    /// multiset (EEE, LEE, TEE, LLE, LTE, TTE, LLL, LLT, LTT, TTT) is
    /// statistically exercised across the runs.
    #[test]
    fn test_orient2d_antisymmetry() {
        let mut rng = Lcg::new(0xCAFE_F00D_DEAD_BEEF);
        const N: usize = 80;
        let mut tested = 0usize;
        for trial in 0..N {
            let a = match rng.next_u64() % 3 {
                0 => rand_explicit(&mut rng),
                1 => rand_lpi(&mut rng),
                _ => rand_tpi(&mut rng),
            };
            let b = match rng.next_u64() % 3 {
                0 => rand_explicit(&mut rng),
                1 => rand_lpi(&mut rng),
                _ => rand_tpi(&mut rng),
            };
            let c = match rng.next_u64() % 3 {
                0 => rand_explicit(&mut rng),
                1 => rand_lpi(&mut rng),
                _ => rand_tpi(&mut rng),
            };
            if !a.is_defined() || !b.is_defined() || !c.is_defined() {
                continue;
            }
            // Use a tri-state sign that handles ±0 correctly. f64::signum
            // returns ±1.0 even for ±0.0, which makes naive antisymmetry
            // checks spurious when the value is exactly zero (collinear).
            let sign3 = |x: f64| -> i32 {
                if x > 0.0 {
                    1
                } else if x < 0.0 {
                    -1
                } else {
                    0
                }
            };
            for proj in [ProjectionAxis::XY, ProjectionAxis::YZ, ProjectionAxis::ZX] {
                let abc = orient2d_indirect(&a, &b, &c, proj);
                let bac = orient2d_indirect(&b, &a, &c, proj);
                assert_eq!(
                    sign3(abc),
                    -sign3(bac),
                    "trial {trial}, proj {proj:?}: orient2d(a,b,c)={abc}, \
                     orient2d(b,a,c)={bac}; must be antisymmetric"
                );
            }
            tested += 1;
        }
        assert!(
            tested > N / 2,
            "expected most trials defined; got {tested}/{N}"
        );
    }

    /// Property: orient2d(p, p, c) == 0 for any defined p, c.
    /// Two coincident inputs make the orientation determinant degenerate.
    #[test]
    fn test_orient2d_self_coincidence() {
        let mut rng = Lcg::new(0xBEEF_F00D);
        const N: usize = 40;
        for trial in 0..N {
            let p = match rng.next_u64() % 3 {
                0 => rand_explicit(&mut rng),
                1 => rand_lpi(&mut rng),
                _ => rand_tpi(&mut rng),
            };
            let c = match rng.next_u64() % 3 {
                0 => rand_explicit(&mut rng),
                1 => rand_lpi(&mut rng),
                _ => rand_tpi(&mut rng),
            };
            if !p.is_defined() || !c.is_defined() {
                continue;
            }
            for proj in [ProjectionAxis::XY, ProjectionAxis::YZ, ProjectionAxis::ZX] {
                let r = orient2d_indirect(&p, &p, &c, proj);
                assert_eq!(
                    r, 0.0,
                    "trial {trial}, proj {proj:?}: orient2d(p,p,c) must be 0; got {r}"
                );
            }
        }
    }

    /// Property: when all 3 inputs are Explicit, `orient2d_indirect` must
    /// equal `geometry_predicates::orient2d` (the EEE path is direct
    /// Shewchuk and must agree exactly).
    #[test]
    fn test_orient2d_eee_exact_matches_shewchuk() {
        let mut rng = Lcg::new(0xF00D_BEEF);
        const N: usize = 60;
        for trial in 0..N {
            let make = |rng: &mut Lcg, integer: bool| -> [f64; 3] {
                if integer {
                    [
                        rng.next_int(10) as f64,
                        rng.next_int(10) as f64,
                        rng.next_int(10) as f64,
                    ]
                } else {
                    [rng.next_unit(), rng.next_unit(), rng.next_unit()]
                }
            };
            let integer = trial % 2 == 0;
            let a = make(&mut rng, integer);
            let b = make(&mut rng, integer);
            let c = make(&mut rng, integer);
            let pa = ImplicitPoint::Explicit(a);
            let pb = ImplicitPoint::Explicit(b);
            let pc = ImplicitPoint::Explicit(c);
            for proj in [ProjectionAxis::XY, ProjectionAxis::YZ, ProjectionAxis::ZX] {
                let (i, j) = match proj {
                    ProjectionAxis::XY => (0, 1),
                    ProjectionAxis::YZ => (1, 2),
                    ProjectionAxis::ZX => (2, 0),
                };
                let indirect = orient2d_indirect(&pa, &pb, &pc, proj);
                let direct =
                    geometry_predicates::orient2d([a[i], a[j]], [b[i], b[j]], [c[i], c[j]]);
                assert_eq!(
                    indirect, direct,
                    "trial {trial}, proj {proj:?}: indirect={indirect}, direct={direct} — \
                     EEE path must equal Shewchuk exactly"
                );
            }
        }
    }

    /// Property: `less_than_indirect` is a strict total order.
    ///
    /// 1. Antisymmetric: cmp(a,b) == cmp(b,a).reverse().
    /// 2. Reflexive equality: cmp(a, a) == Equal.
    /// 3. Transitive: if cmp(a,b)=Less and cmp(b,c)=Less, then cmp(a,c)=Less.
    #[test]
    fn test_less_than_total_order() {
        use std::cmp::Ordering;
        let mut rng = Lcg::new(0xDEAD_BEEF_CAFE_F00D);
        const N: usize = 30;

        // Build a sample population mixing Explicit, LPI, TPI.
        let mut sample: Vec<ImplicitPoint> = Vec::with_capacity(N);
        while sample.len() < N {
            let p = match rng.next_u64() % 3 {
                0 => rand_explicit(&mut rng),
                1 => rand_lpi(&mut rng),
                _ => rand_tpi(&mut rng),
            };
            if p.is_defined() {
                sample.push(p);
            }
        }

        // (1) Antisymmetry + (2) reflexivity.
        for (i, p) in sample.iter().enumerate() {
            assert_eq!(
                less_than_indirect(p, p),
                Ordering::Equal,
                "i={i}: less_than_indirect(p, p) must be Equal"
            );
            for (j, q) in sample.iter().enumerate().skip(i + 1) {
                let pq = less_than_indirect(p, q);
                let qp = less_than_indirect(q, p);
                assert_eq!(
                    pq,
                    qp.reverse(),
                    "i={i}, j={j}: cmp(p,q)={pq:?}, cmp(q,p)={qp:?} — must be reversed"
                );
            }
        }

        // (3) Transitivity — exhaustive over the sample.
        for (i, a) in sample.iter().enumerate() {
            for (j, b) in sample.iter().enumerate() {
                if i == j {
                    continue;
                }
                let ab = less_than_indirect(a, b);
                if ab != Ordering::Less {
                    continue;
                }
                for (k, c) in sample.iter().enumerate() {
                    if k == i || k == j {
                        continue;
                    }
                    let bc = less_than_indirect(b, c);
                    if bc != Ordering::Less {
                        continue;
                    }
                    let ac = less_than_indirect(a, c);
                    assert_eq!(
                        ac,
                        Ordering::Less,
                        "transitivity: i={i}, j={j}, k={k}: a<b<c but a vs c = {ac:?}"
                    );
                }
            }
        }
    }

    // ── Phase D: pointCompare TPI variants (Cherchi 2020 §4.3) ───────

    /// TPI built from the three axis planes x=cx, y=cy, z=cz. Materializes
    /// to (cx, cy, cz). d_T is non-zero by construction (the three normals
    /// are the standard basis).
    fn axis_tpi(cx: f64, cy: f64, cz: f64) -> ImplicitPoint {
        ImplicitPoint::TPI {
            v1: [cx, 0.0, 0.0],
            v2: [cx, 1.0, 0.0],
            v3: [cx, 0.0, 1.0],
            w1: [0.0, cy, 0.0],
            w2: [1.0, cy, 0.0],
            w3: [0.0, cy, 1.0],
            u1: [0.0, 0.0, cz],
            u2: [1.0, 0.0, cz],
            u3: [0.0, 1.0, cz],
        }
    }

    /// Phase D, TE: TPI(1,2,3) vs Explicit(5,5,5). On every axis TPI < E.
    /// Equal case: TPI(1,2,3) vs Explicit(1,2,3) → Equal on every axis.
    /// Verifies the dispatch in `point_compare_on_axis` reaches the new
    /// `point_compare_te` arm (was previously the materialize fallback).
    #[test]
    fn test_point_compare_te_canonical() {
        let tpi = axis_tpi(1.0, 2.0, 3.0);
        let e_far = ImplicitPoint::Explicit([5.0, 5.0, 5.0]);
        assert_eq!(
            point_compare_on_axis(&tpi, &e_far, Axis::X),
            std::cmp::Ordering::Less,
            "TPI.x = 1 < e.x = 5"
        );
        assert_eq!(
            point_compare_on_axis(&tpi, &e_far, Axis::Y),
            std::cmp::Ordering::Less,
            "TPI.y = 2 < e.y = 5"
        );
        assert_eq!(
            point_compare_on_axis(&tpi, &e_far, Axis::Z),
            std::cmp::Ordering::Less,
            "TPI.z = 3 < e.z = 5"
        );
        // Reverse direction (ET arm): explicit > TPI on every axis.
        assert_eq!(
            point_compare_on_axis(&e_far, &tpi, Axis::X),
            std::cmp::Ordering::Greater
        );

        // Equal-on-all-axes regression case.
        let e_eq = ImplicitPoint::Explicit([1.0, 2.0, 3.0]);
        assert_eq!(
            point_compare_on_axis(&tpi, &e_eq, Axis::X),
            std::cmp::Ordering::Equal
        );
        assert_eq!(
            point_compare_on_axis(&tpi, &e_eq, Axis::Y),
            std::cmp::Ordering::Equal
        );
        assert_eq!(
            point_compare_on_axis(&tpi, &e_eq, Axis::Z),
            std::cmp::Ordering::Equal
        );
    }

    /// Phase D, LT: LPI vs TPI. Use the existing oracle LPI at (0,0,0)
    /// (edge along z, plane through (1,0,0)/(0,1,0)/(-1,-1,0)) vs TPI(1,2,3).
    /// LPI < TPI on every axis. Equal case: LPI at (1,1,1) (existing oracle)
    /// vs TPI(1,1,1) → Equal.
    #[test]
    fn test_point_compare_lt_canonical() {
        let lpi_origin = ImplicitPoint::LPI {
            q1: [0.0, 0.0, -1.0],
            q2: [0.0, 0.0, 1.0],
            r: [1.0, 0.0, 0.0],
            s: [0.0, 1.0, 0.0],
            t: [-1.0, -1.0, 0.0],
        };
        let tpi_far = axis_tpi(1.0, 2.0, 3.0);
        assert_eq!(
            point_compare_on_axis(&lpi_origin, &tpi_far, Axis::X),
            std::cmp::Ordering::Less,
            "LPI.x = 0 < TPI.x = 1"
        );
        assert_eq!(
            point_compare_on_axis(&lpi_origin, &tpi_far, Axis::Y),
            std::cmp::Ordering::Less,
            "LPI.y = 0 < TPI.y = 2"
        );
        assert_eq!(
            point_compare_on_axis(&lpi_origin, &tpi_far, Axis::Z),
            std::cmp::Ordering::Less,
            "LPI.z = 0 < TPI.z = 3"
        );
        // Reverse direction (TL arm).
        assert_eq!(
            point_compare_on_axis(&tpi_far, &lpi_origin, Axis::X),
            std::cmp::Ordering::Greater
        );

        // Equal-on-all-axes regression case: LPI(1,1,1) vs TPI(1,1,1).
        let lpi_unit = ImplicitPoint::LPI {
            q1: [1.0, 1.0, 0.0],
            q2: [1.0, 1.0, 2.0],
            r: [0.0, 0.0, 1.0],
            s: [10.0, 0.0, 1.0],
            t: [0.0, 10.0, 1.0],
        };
        let tpi_unit = axis_tpi(1.0, 1.0, 1.0);
        assert_eq!(
            point_compare_on_axis(&lpi_unit, &tpi_unit, Axis::X),
            std::cmp::Ordering::Equal
        );
        assert_eq!(
            point_compare_on_axis(&lpi_unit, &tpi_unit, Axis::Y),
            std::cmp::Ordering::Equal
        );
        assert_eq!(
            point_compare_on_axis(&lpi_unit, &tpi_unit, Axis::Z),
            std::cmp::Ordering::Equal
        );
    }

    /// Phase D, TT: two TPIs at known points. (0,0,0) < (1,2,3) on all axes.
    /// Equal case: two distinct TPI constructions both materializing to the
    /// same point → Equal.
    #[test]
    fn test_point_compare_tt_canonical() {
        let tpi_origin = axis_tpi(0.0, 0.0, 0.0);
        let tpi_far = axis_tpi(1.0, 2.0, 3.0);
        assert_eq!(
            point_compare_on_axis(&tpi_origin, &tpi_far, Axis::X),
            std::cmp::Ordering::Less,
            "TPI.x = 0 < TPI.x = 1"
        );
        assert_eq!(
            point_compare_on_axis(&tpi_origin, &tpi_far, Axis::Y),
            std::cmp::Ordering::Less
        );
        assert_eq!(
            point_compare_on_axis(&tpi_origin, &tpi_far, Axis::Z),
            std::cmp::Ordering::Less
        );
        // Reverse direction.
        assert_eq!(
            point_compare_on_axis(&tpi_far, &tpi_origin, Axis::X),
            std::cmp::Ordering::Greater
        );

        // Equal: two TPIs both at the origin, different defining triangles.
        // First uses the standard axis planes; second uses the slanted plane
        // x+y+z=0 with the yz and xz axis planes — also materializes to (0,0,0).
        let tpi_origin_alt = ImplicitPoint::TPI {
            v1: [0.0, 0.0, 0.0],
            v2: [0.0, 1.0, 0.0],
            v3: [0.0, 0.0, 1.0],
            w1: [0.0, 0.0, 0.0],
            w2: [1.0, 0.0, 0.0],
            w3: [0.0, 0.0, 1.0],
            u1: [0.0, 0.0, 0.0],
            u2: [1.0, -1.0, 0.0],
            u3: [1.0, 0.0, -1.0],
        };
        assert_eq!(
            point_compare_on_axis(&tpi_origin, &tpi_origin_alt, Axis::X),
            std::cmp::Ordering::Equal
        );
        assert_eq!(
            point_compare_on_axis(&tpi_origin, &tpi_origin_alt, Axis::Y),
            std::cmp::Ordering::Equal
        );
        assert_eq!(
            point_compare_on_axis(&tpi_origin, &tpi_origin_alt, Axis::Z),
            std::cmp::Ordering::Equal
        );
    }

    // ── Phase B: orient2d TPI variants — canonical-case tests ─────────

    /// Helper: TPI fixture for three perpendicular planes meeting at (1,0,0).
    fn tpi_at_100() -> ImplicitPoint {
        ImplicitPoint::TPI {
            // plane z=0 through origin
            v1: [0.0, 0.0, 0.0],
            v2: [1.0, 0.0, 0.0],
            v3: [0.0, 1.0, 0.0],
            // plane x=1 (offset)
            w1: [1.0, 0.0, 0.0],
            w2: [1.0, 1.0, 0.0],
            w3: [1.0, 0.0, 1.0],
            // plane y=0 through origin
            u1: [0.0, 0.0, 0.0],
            u2: [0.0, 0.0, 1.0],
            u3: [1.0, 0.0, 0.0],
        }
    }

    /// Helper: TPI fixture for three perpendicular planes meeting at (0,0,0).
    fn tpi_at_origin() -> ImplicitPoint {
        ImplicitPoint::TPI {
            v1: [0.0, 0.0, 0.0],
            v2: [1.0, 0.0, 0.0],
            v3: [0.0, 1.0, 0.0],
            w1: [0.0, 0.0, 0.0],
            w2: [0.0, 1.0, 0.0],
            w3: [0.0, 0.0, 1.0],
            u1: [0.0, 0.0, 0.0],
            u2: [0.0, 0.0, 1.0],
            u3: [1.0, 0.0, 0.0],
        }
    }

    /// orient2d_TEE: TPI at (1,0,0) + (0,0,0) + (0,1,0) in XY.
    /// Materialized: orient2d((1,0), (0,0), (0,1)) = 1*0 - 0*1 - 1*0 + 0*0 + 0*1 - 0*0 ... compute:
    /// Standard: (1-0)*(0-1) - (0-1)*(0-0) = -1 - 0 = -1 (CW).
    #[test]
    fn test_orient2d_tee_basic() {
        let t = tpi_at_100();
        let e1 = ImplicitPoint::Explicit([0.0, 0.0, 0.0]);
        let e2 = ImplicitPoint::Explicit([0.0, 1.0, 0.0]);
        let result = orient2d_indirect(&t, &e1, &e2, ProjectionAxis::XY);
        // Verify against materialized
        let mat = t.materialize().unwrap();
        let direct = geometry_predicates::orient2d([mat[0], mat[1]], [0.0, 0.0], [0.0, 1.0]);
        assert_eq!(
            result.signum(),
            direct.signum(),
            "TEE indirect={} vs materialized={}",
            result,
            direct
        );
        assert!(result < 0.0, "TPI(1,0,0)→(0,0,0)→(0,1,0) is CW");
    }

    /// orient2d_TEE permutations: antisymmetry across all 3 positions.
    #[test]
    fn test_orient2d_tee_permutations() {
        let t = tpi_at_100();
        let e1 = ImplicitPoint::Explicit([0.5, -0.5, 0.0]);
        let e2 = ImplicitPoint::Explicit([0.0, 1.0, 0.0]);

        let tee = orient2d_indirect(&t, &e1, &e2, ProjectionAxis::XY);
        let ete = orient2d_indirect(&e1, &t, &e2, ProjectionAxis::XY);
        let eet = orient2d_indirect(&e1, &e2, &t, ProjectionAxis::XY);

        assert_ne!(tee, 0.0);
        assert_eq!(tee.signum(), -ete.signum(), "swap → negate");
        assert_eq!(tee.signum(), eet.signum(), "cyclic → preserve");
    }

    /// orient2d_LTE: LPI at (0,0,0) + TPI at (1,0,0) + Explicit (0,1,0).
    /// orient2d((0,0), (1,0), (0,1)) = (0-0)*(0-1) - (0-1)*(1-0) = 0 - (-1) = 1 (CCW).
    #[test]
    fn test_orient2d_lte_basic() {
        let l = ImplicitPoint::LPI {
            q1: [0.0, 0.0, -1.0],
            q2: [0.0, 0.0, 1.0],
            r: [1.0, 0.0, 0.0],
            s: [0.0, 1.0, 0.0],
            t: [-1.0, -1.0, 0.0],
        };
        let t = tpi_at_100();
        let e = ImplicitPoint::Explicit([0.0, 1.0, 0.0]);
        let result = orient2d_indirect(&l, &t, &e, ProjectionAxis::XY);
        let lm = l.materialize().unwrap();
        let tm = t.materialize().unwrap();
        let direct = geometry_predicates::orient2d([lm[0], lm[1]], [tm[0], tm[1]], [0.0, 1.0]);
        assert_eq!(
            result.signum(),
            direct.signum(),
            "LTE indirect={} vs materialized={}",
            result,
            direct
        );
        assert!(result > 0.0, "(0,0)→(1,0)→(0,1) is CCW");
    }

    /// orient2d_LLT: two LPIs + one TPI in XY.
    #[test]
    fn test_orient2d_llt_basic() {
        let l1 = ImplicitPoint::LPI {
            q1: [0.0, 0.0, -1.0],
            q2: [0.0, 0.0, 1.0],
            r: [1.0, 0.0, 0.0],
            s: [0.0, 1.0, 0.0],
            t: [-1.0, -1.0, 0.0],
        }; // materializes (0,0,0)
        let l2 = ImplicitPoint::LPI {
            q1: [2.0, 0.0, -1.0],
            q2: [2.0, 0.0, 1.0],
            r: [1.0, 0.0, 0.0],
            s: [0.0, 1.0, 0.0],
            t: [-1.0, -1.0, 0.0],
        }; // materializes (2,0,0)
        let t = tpi_at_100(); // (1,0,0)
                              // orient2d((0,0),(2,0),(1,0)) = (0-0)(0-0) - (0-0)(2-1) = 0 (collinear on y=0)
        let result = orient2d_indirect(&l1, &l2, &t, ProjectionAxis::XY);
        let m1 = l1.materialize().unwrap();
        let m2 = l2.materialize().unwrap();
        let mt = t.materialize().unwrap();
        let direct = geometry_predicates::orient2d([m1[0], m1[1]], [m2[0], m2[1]], [mt[0], mt[1]]);
        assert_eq!(
            result, 0.0,
            "all three on y=0 → collinear; got {} (direct {})",
            result, direct
        );
    }

    /// orient2d_LLT non-collinear: should match materialized.
    #[test]
    fn test_orient2d_llt_matches_materialize() {
        let l1 = ImplicitPoint::LPI {
            q1: [0.3, 0.2, -1.0],
            q2: [0.3, 0.2, 1.0],
            r: [1.0, 0.0, 0.0],
            s: [0.0, 1.0, 0.0],
            t: [-1.0, -1.0, 0.0],
        };
        let l2 = ImplicitPoint::LPI {
            q1: [0.7, 0.1, -1.0],
            q2: [0.7, 0.1, 1.0],
            r: [1.0, 0.0, 0.0],
            s: [0.0, 1.0, 0.0],
            t: [-1.0, -1.0, 0.0],
        };
        let tpi = tpi_at_100();
        let result = orient2d_indirect(&l1, &l2, &tpi, ProjectionAxis::XY);
        let m1 = l1.materialize().unwrap();
        let m2 = l2.materialize().unwrap();
        let mt = tpi.materialize().unwrap();
        let direct = geometry_predicates::orient2d([m1[0], m1[1]], [m2[0], m2[1]], [mt[0], mt[1]]);
        assert_eq!(
            result.signum(),
            direct.signum(),
            "LLT indirect={} vs materialized={}",
            result,
            direct
        );
    }

    /// orient2d_LTT: LPI + two TPIs.
    #[test]
    fn test_orient2d_ltt_matches_materialize() {
        let l = ImplicitPoint::LPI {
            q1: [0.5, 0.5, -1.0],
            q2: [0.5, 0.5, 1.0],
            r: [1.0, 0.0, 0.0],
            s: [0.0, 1.0, 0.0],
            t: [-1.0, -1.0, 0.0],
        };
        let t1 = tpi_at_100();
        let t2 = tpi_at_origin();
        let result = orient2d_indirect(&l, &t1, &t2, ProjectionAxis::XY);
        let lm = l.materialize().unwrap();
        let t1m = t1.materialize().unwrap();
        let t2m = t2.materialize().unwrap();
        let direct =
            geometry_predicates::orient2d([lm[0], lm[1]], [t1m[0], t1m[1]], [t2m[0], t2m[1]]);
        assert_eq!(
            result.signum(),
            direct.signum(),
            "LTT indirect={} vs materialized={}",
            result,
            direct
        );
    }

    /// orient2d_TTE: two TPIs + one explicit.
    #[test]
    fn test_orient2d_tte_matches_materialize() {
        let t1 = tpi_at_100(); // (1, 0, 0)
        let t2 = tpi_at_origin(); // (0, 0, 0)
        let e = ImplicitPoint::Explicit([0.5, 1.0, 0.0]);
        let result = orient2d_indirect(&t1, &t2, &e, ProjectionAxis::XY);
        let m1 = t1.materialize().unwrap();
        let m2 = t2.materialize().unwrap();
        let direct = geometry_predicates::orient2d([m1[0], m1[1]], [m2[0], m2[1]], [0.5, 1.0]);
        assert_eq!(
            result.signum(),
            direct.signum(),
            "TTE indirect={} vs materialized={}",
            result,
            direct
        );
    }

    /// orient2d_TTT: three TPIs.
    #[test]
    fn test_orient2d_ttt_matches_materialize() {
        let t1 = tpi_at_origin(); // (0, 0, 0)
        let t2 = tpi_at_100(); // (1, 0, 0)
                               // Construct another TPI at (0, 1, 0): planes z=0, x=0, y=1.
        let t3 = ImplicitPoint::TPI {
            // z=0 through origin
            v1: [0.0, 0.0, 0.0],
            v2: [1.0, 0.0, 0.0],
            v3: [0.0, 1.0, 0.0],
            // x=0 through origin
            w1: [0.0, 0.0, 0.0],
            w2: [0.0, 1.0, 0.0],
            w3: [0.0, 0.0, 1.0],
            // y=1
            u1: [0.0, 1.0, 0.0],
            u2: [1.0, 1.0, 0.0],
            u3: [0.0, 1.0, 1.0],
        };
        let result = orient2d_indirect(&t1, &t2, &t3, ProjectionAxis::XY);
        let m1 = t1.materialize().unwrap();
        let m2 = t2.materialize().unwrap();
        let m3 = t3.materialize().unwrap();
        let direct = geometry_predicates::orient2d([m1[0], m1[1]], [m2[0], m2[1]], [m3[0], m3[1]]);
        // Expected: orient2d((0,0),(1,0),(0,1)) = 1 (CCW).
        assert!(
            result > 0.0,
            "TTT (0,0)→(1,0)→(0,1) should be CCW, got {result}"
        );
        assert_eq!(
            result.signum(),
            direct.signum(),
            "TTT indirect={} vs materialized={}",
            result,
            direct
        );
    }

    /// Phase B sanity: every (E,L,T)³ combination dispatches without panicking.
    /// Self-coincidence: orient2d(p, p, q) must be 0 for any types.
    #[test]
    fn test_orient2d_dispatch_self_coincidence() {
        let e = ImplicitPoint::Explicit([0.5, 0.5, 0.5]);
        let l = ImplicitPoint::LPI {
            q1: [0.0, 0.0, -1.0],
            q2: [0.0, 0.0, 1.0],
            r: [1.0, 0.0, 0.0],
            s: [0.0, 1.0, 0.0],
            t: [-1.0, -1.0, 0.0],
        };
        let t = tpi_at_100();

        for (a, b) in [(&e, &l), (&e, &t), (&l, &t), (&l, &l), (&t, &t)] {
            // orient2d(a, a, b) should be 0 (two points coincide).
            assert_eq!(
                orient2d_indirect(a, a, b, ProjectionAxis::XY),
                0.0,
                "orient2d(a,a,b) must be 0"
            );
        }
    }

    // ── Phase C: orient3d TPI variants — canonical-case tests ─────────
    //
    // Strategy: build implicit (LPI/TPI) representations of a known explicit
    // point, then plug them into the orient3d unit-tetrahedron formula
    //
    //     orient3d((0,0,0), (1,0,0), (0,1,0), (0,0,1)) > 0  (CCW tet)
    //
    // The expected sign is +1 in every variant. Each test replaces a specific
    // subset of the four vertices with an implicit point that materializes to
    // the same coordinate.

    /// Helper: LPI fixture that materializes to a given (x, y, z) — uses a
    /// vertical edge through (x, y, *) and the z=0 plane (so the plane is the
    /// xy-plane and the edge intersects at (x, y, 0)).
    fn lpi_at(x: f64, y: f64, z: f64) -> ImplicitPoint {
        // Edge from (x, y, z-1) to (x, y, z+1); plane z = z (defined by three
        // distinct points at height z).
        ImplicitPoint::LPI {
            q1: [x, y, z - 1.0],
            q2: [x, y, z + 1.0],
            r: [0.0, 0.0, z],
            s: [1.0, 0.0, z],
            t: [0.0, 1.0, z],
        }
    }

    /// Helper: TPI fixture for three perpendicular planes meeting at (x, y, z).
    fn tpi_at(x: f64, y: f64, z: f64) -> ImplicitPoint {
        ImplicitPoint::TPI {
            // plane x = x
            v1: [x, 0.0, 0.0],
            v2: [x, 1.0, 0.0],
            v3: [x, 0.0, 1.0],
            // plane y = y
            w1: [0.0, y, 0.0],
            w2: [1.0, y, 0.0],
            w3: [0.0, y, 1.0],
            // plane z = z
            u1: [0.0, 0.0, z],
            u2: [1.0, 0.0, z],
            u3: [0.0, 1.0, z],
        }
    }

    /// Reference unit-tet orient3d sign — expected positive on
    /// ((0,0,0), (1,0,0), (0,1,0), (0,0,1)).
    fn expected_unit_tet_sign() -> f64 {
        geometry_predicates::orient3d(
            [0.0, 0.0, 0.0],
            [1.0, 0.0, 0.0],
            [0.0, 1.0, 0.0],
            [0.0, 0.0, 1.0],
        )
        .signum()
    }

    /// orient3d_LLLE — three LPIs (at the first three tet vertices) + one Explicit.
    #[test]
    fn test_orient3d_llle_unit_tet() {
        let a = lpi_at(0.0, 0.0, 0.0);
        let b = lpi_at(1.0, 0.0, 0.0);
        let c = lpi_at(0.0, 1.0, 0.0);
        let d = ImplicitPoint::Explicit([0.0, 0.0, 1.0]);
        let result = orient3d_indirect(&a, &b, &c, &d);
        assert_eq!(result.signum(), expected_unit_tet_sign(), "LLLE: {result}");
    }

    /// orient3d_LLLL — four LPIs, one per tet vertex.
    #[test]
    fn test_orient3d_llll_unit_tet() {
        let a = lpi_at(0.0, 0.0, 0.0);
        let b = lpi_at(1.0, 0.0, 0.0);
        let c = lpi_at(0.0, 1.0, 0.0);
        let d = lpi_at(0.0, 0.0, 1.0);
        let result = orient3d_indirect(&a, &b, &c, &d);
        assert_eq!(result.signum(), expected_unit_tet_sign(), "LLLL: {result}");
    }

    /// orient3d_TEEE — one TPI + three Explicit.
    #[test]
    fn test_orient3d_teee_unit_tet() {
        let a = tpi_at(0.0, 0.0, 0.0);
        let b = ImplicitPoint::Explicit([1.0, 0.0, 0.0]);
        let c = ImplicitPoint::Explicit([0.0, 1.0, 0.0]);
        let d = ImplicitPoint::Explicit([0.0, 0.0, 1.0]);
        let result = orient3d_indirect(&a, &b, &c, &d);
        assert_eq!(result.signum(), expected_unit_tet_sign(), "TEEE: {result}");
    }

    /// orient3d_LTEE — one LPI + one TPI + two Explicit.
    #[test]
    fn test_orient3d_ltee_unit_tet() {
        let a = lpi_at(0.0, 0.0, 0.0);
        let b = tpi_at(1.0, 0.0, 0.0);
        let c = ImplicitPoint::Explicit([0.0, 1.0, 0.0]);
        let d = ImplicitPoint::Explicit([0.0, 0.0, 1.0]);
        let result = orient3d_indirect(&a, &b, &c, &d);
        assert_eq!(result.signum(), expected_unit_tet_sign(), "LTEE: {result}");
    }

    /// orient3d_LLTE — two LPI + one TPI + one Explicit.
    #[test]
    fn test_orient3d_llte_unit_tet() {
        let a = lpi_at(0.0, 0.0, 0.0);
        let b = lpi_at(1.0, 0.0, 0.0);
        let c = tpi_at(0.0, 1.0, 0.0);
        let d = ImplicitPoint::Explicit([0.0, 0.0, 1.0]);
        let result = orient3d_indirect(&a, &b, &c, &d);
        assert_eq!(result.signum(), expected_unit_tet_sign(), "LLTE: {result}");
    }

    /// orient3d_TTEE — two TPI + two Explicit.
    #[test]
    fn test_orient3d_ttee_unit_tet() {
        let a = tpi_at(0.0, 0.0, 0.0);
        let b = tpi_at(1.0, 0.0, 0.0);
        let c = ImplicitPoint::Explicit([0.0, 1.0, 0.0]);
        let d = ImplicitPoint::Explicit([0.0, 0.0, 1.0]);
        let result = orient3d_indirect(&a, &b, &c, &d);
        assert_eq!(result.signum(), expected_unit_tet_sign(), "TTEE: {result}");
    }

    /// orient3d_LTTE — one LPI + two TPI + one Explicit.
    #[test]
    fn test_orient3d_ltte_unit_tet() {
        let a = lpi_at(0.0, 0.0, 0.0);
        let b = tpi_at(1.0, 0.0, 0.0);
        let c = tpi_at(0.0, 1.0, 0.0);
        let d = ImplicitPoint::Explicit([0.0, 0.0, 1.0]);
        let result = orient3d_indirect(&a, &b, &c, &d);
        assert_eq!(result.signum(), expected_unit_tet_sign(), "LTTE: {result}");
    }

    /// orient3d_LLTT — two LPI + two TPI.
    #[test]
    fn test_orient3d_lltt_unit_tet() {
        let a = lpi_at(0.0, 0.0, 0.0);
        let b = lpi_at(1.0, 0.0, 0.0);
        let c = tpi_at(0.0, 1.0, 0.0);
        let d = tpi_at(0.0, 0.0, 1.0);
        let result = orient3d_indirect(&a, &b, &c, &d);
        assert_eq!(result.signum(), expected_unit_tet_sign(), "LLTT: {result}");
    }

    /// orient3d_LTTT — one LPI + three TPI.
    #[test]
    fn test_orient3d_lttt_unit_tet() {
        let a = lpi_at(0.0, 0.0, 0.0);
        let b = tpi_at(1.0, 0.0, 0.0);
        let c = tpi_at(0.0, 1.0, 0.0);
        let d = tpi_at(0.0, 0.0, 1.0);
        let result = orient3d_indirect(&a, &b, &c, &d);
        assert_eq!(result.signum(), expected_unit_tet_sign(), "LTTT: {result}");
    }

    /// orient3d_TTTE — three TPI + one Explicit.
    #[test]
    fn test_orient3d_ttte_unit_tet() {
        let a = tpi_at(0.0, 0.0, 0.0);
        let b = tpi_at(1.0, 0.0, 0.0);
        let c = tpi_at(0.0, 1.0, 0.0);
        let d = ImplicitPoint::Explicit([0.0, 0.0, 1.0]);
        let result = orient3d_indirect(&a, &b, &c, &d);
        assert_eq!(result.signum(), expected_unit_tet_sign(), "TTTE: {result}");
    }

    /// orient3d_TTTT — four TPI, one per tet vertex.
    #[test]
    fn test_orient3d_tttt_unit_tet() {
        let a = tpi_at(0.0, 0.0, 0.0);
        let b = tpi_at(1.0, 0.0, 0.0);
        let c = tpi_at(0.0, 1.0, 0.0);
        let d = tpi_at(0.0, 0.0, 1.0);
        let result = orient3d_indirect(&a, &b, &c, &d);
        assert_eq!(result.signum(), expected_unit_tet_sign(), "TTTT: {result}");
    }

    /// orient3d_LLLT — three LPI + one TPI.
    #[test]
    fn test_orient3d_lllt_unit_tet() {
        let a = lpi_at(0.0, 0.0, 0.0);
        let b = lpi_at(1.0, 0.0, 0.0);
        let c = lpi_at(0.0, 1.0, 0.0);
        let d = tpi_at(0.0, 0.0, 1.0);
        let result = orient3d_indirect(&a, &b, &c, &d);
        assert_eq!(result.signum(), expected_unit_tet_sign(), "LLLT: {result}");
    }
}
