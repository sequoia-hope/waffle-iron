//! Shared B-spline and NURBS curve/surface types and Piegl & Tiller algorithms.
//!
//! Primary reference: Les Piegl & Wayne Tiller, *The NURBS Book*, 2nd ed.,
//! Springer, 1997 (`refs/text/piegl_tiller1997_nurbs_book.txt`).
//!
//! All constructors and algorithms validate their inputs strictly and return a
//! typed [`BSplineError`] on invalid degrees, knot vectors, control grids,
//! non-positive weights, out-of-range parameters, or failed interpolations —
//! never panicking and never clamping or falling back silently (`specs/loft.md`
//! Reviewer amendment & §6 user decisions).

use std::fmt;

use cad_primitives::{Point3, Vector3};

/// Typed error returned by B-spline / NURBS validation, evaluation, knot, and
/// interpolation operations.
#[derive(Debug, Clone, PartialEq)]
pub enum BSplineError {
    /// Curve or surface degree must be at least 1.
    ZeroDegree,
    /// Control point count must be at least `degree + 1`.
    TooFewControlPoints { degree: usize, count: usize },
    /// Knot vector length must equal `num_control_points + degree + 1`.
    KnotCountMismatch { expected: usize, actual: usize },
    /// A knot value is `NaN` or infinite.
    NonFiniteKnot { index: usize, value: f64 },
    /// Knot vector is not non-decreasing (`knots[index] > knots[index + 1]`).
    DecreasingKnots { index: usize, prev: f64, next: f64 },
    /// Knot vector domain `[u_min, u_max]` has zero or negative span.
    DegenerateDomain { u_min: f64, u_max: f64 },
    /// Clamped knot vector start multiplicity `< degree + 1`.
    UnclampedStart {
        index: usize,
        expected: f64,
        actual: f64,
    },
    /// Clamped knot vector end multiplicity `< degree + 1`.
    UnclampedEnd {
        index: usize,
        expected: f64,
        actual: f64,
    },
    /// An interior knot equals `u_min` or `u_max` (making endpoint multiplicity `> degree + 1`).
    InteriorKnotAtBoundary { index: usize, knot: f64 },
    /// An interior knot has multiplicity exceeding `degree`.
    InteriorKnotMultiplicityExceedsDegree {
        knot: f64,
        multiplicity: usize,
        degree: usize,
    },
    /// A control point or data point has a `NaN` or infinite coordinate.
    NonFiniteControlPoint { index: usize },
    /// Rational weight count does not match control point count.
    WeightCountMismatch { expected: usize, actual: usize },
    /// A rational weight is `NaN`, infinite, or `<= 0.0`.
    NonPositiveWeight { index: usize, weight: f64 },
    /// Surface control point grid has zero rows or zero columns.
    EmptySurfaceControlGrid,
    /// Surface control point grid rows do not all have the same column count.
    RaggedControlGrid {
        row: usize,
        expected_cols: usize,
        actual_cols: usize,
    },
    /// Surface weight grid dimensions do not match the control point grid.
    RaggedWeightGrid {
        row: usize,
        expected_cols: usize,
        actual_cols: usize,
    },
    /// Parameter `param` lies outside `[min, max]` or is non-finite.
    ParameterOutOfRange { param: f64, min: f64, max: f64 },
    /// Span index passed to basis evaluation is out of bounds for `(degree, knot_len)`.
    InvalidSpan {
        span: usize,
        degree: usize,
        knot_len: usize,
    },
    /// Knot span `[knots[span], knots[span + 1]]` has zero width.
    ZeroKnotSpan { span: usize },
    /// Homogeneous derivative arrays passed to rational quotient evaluation are too short.
    InvalidDerivativeInputs {
        expected_len: usize,
        actual_aders: usize,
        actual_wders: usize,
    },
    /// Knot refinement vector `X` is empty.
    EmptyRefinementKnots,
    /// A knot to be inserted lies outside the open interior `(min, max)` or is non-finite.
    RefinementKnotOutOfRange { knot: f64, min: f64, max: f64 },
    /// Knot refinement vector `X` is not non-decreasing.
    UnsortedRefinementKnots { index: usize, prev: f64, next: f64 },
    /// Inserting `X` would raise an interior knot's multiplicity above `degree`.
    RefinementMultiplicityExceedsDegree {
        knot: f64,
        total_multiplicity: usize,
        degree: usize,
    },
    /// Too few interpolation points for the requested degree.
    InterpolationTooFewPoints { count: usize, min_required: usize },
    /// Parameter count does not match expected data point count.
    ParameterCountMismatch { expected: usize, actual: usize },
    /// Parameter sequence is not strictly increasing inside `[0, 1]`.
    InvalidParameterSequence { index: usize },
    /// Two consecutive interpolation points coincide (zero chord length).
    ZeroChordLength { index: usize },
    /// Global interpolation linear system is singular or ill-conditioned.
    SingularInterpolationSystem,
}

impl fmt::Display for BSplineError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::ZeroDegree => write!(f, "B-spline degree must be >= 1"),
            Self::TooFewControlPoints { degree, count } => write!(
                f,
                "B-spline of degree {degree} requires at least {} control points, got {count}",
                degree + 1
            ),
            Self::KnotCountMismatch { expected, actual } => {
                write!(f, "expected {expected} knots, got {actual}")
            }
            Self::NonFiniteKnot { index, value } => {
                write!(f, "knot at index {index} is non-finite ({value})")
            }
            Self::DecreasingKnots { index, prev, next } => write!(
                f,
                "knot vector decreases at index {index}: {prev} > {next}"
            ),
            Self::DegenerateDomain { u_min, u_max } => {
                write!(f, "degenerate knot domain [{u_min}, {u_max}]")
            }
            Self::UnclampedStart {
                index,
                expected,
                actual,
            } => write!(
                f,
                "unclamped start knot at index {index}: expected {expected}, got {actual}"
            ),
            Self::UnclampedEnd {
                index,
                expected,
                actual,
            } => write!(
                f,
                "unclamped end knot at index {index}: expected {expected}, got {actual}"
            ),
            Self::InteriorKnotAtBoundary { index, knot } => write!(
                f,
                "interior knot at index {index} equals domain boundary ({knot})"
            ),
            Self::InteriorKnotMultiplicityExceedsDegree {
                knot,
                multiplicity,
                degree,
            } => write!(
                f,
                "interior knot {knot} has multiplicity {multiplicity} > degree {degree}"
            ),
            Self::NonFiniteControlPoint { index } => {
                write!(f, "point at index {index} has non-finite coordinates")
            }
            Self::WeightCountMismatch { expected, actual } => {
                write!(f, "expected {expected} weights, got {actual}")
            }
            Self::NonPositiveWeight { index, weight } => write!(
                f,
                "rational weight at index {index} must be finite and > 0, got {weight}"
            ),
            Self::EmptySurfaceControlGrid => write!(f, "surface control grid is empty"),
            Self::RaggedControlGrid {
                row,
                expected_cols,
                actual_cols,
            } => write!(
                f,
                "surface control row {row} has {actual_cols} columns, expected {expected_cols}"
            ),
            Self::RaggedWeightGrid {
                row,
                expected_cols,
                actual_cols,
            } => write!(
                f,
                "surface weight row {row} has {actual_cols} columns, expected {expected_cols}"
            ),
            Self::ParameterOutOfRange { param, min, max } => {
                write!(f, "parameter {param} is outside domain [{min}, {max}]")
            }
            Self::InvalidSpan {
                span,
                degree,
                knot_len,
            } => write!(
                f,
                "span {span} is invalid for degree {degree} and knot vector length {knot_len}"
            ),
            Self::ZeroKnotSpan { span } => {
                write!(f, "knot span at index {span} has zero width")
            }
            Self::InvalidDerivativeInputs {
                expected_len,
                actual_aders,
                actual_wders,
            } => write!(
                f,
                "rational derivative inputs require length >= {expected_len}, got aders={actual_aders}, wders={actual_wders}"
            ),
            Self::EmptyRefinementKnots => write!(f, "refinement knot vector X is empty"),
            Self::RefinementKnotOutOfRange { knot, min, max } => write!(
                f,
                "refinement knot {knot} must lie strictly inside ({min}, {max})"
            ),
            Self::UnsortedRefinementKnots { index, prev, next } => write!(
                f,
                "refinement knots decrease at index {index}: {prev} > {next}"
            ),
            Self::RefinementMultiplicityExceedsDegree {
                knot,
                total_multiplicity,
                degree,
            } => write!(
                f,
                "refining knot {knot} produces multiplicity {total_multiplicity} > degree {degree}"
            ),
            Self::InterpolationTooFewPoints {
                count,
                min_required,
            } => write!(
                f,
                "global curve interpolation requires at least {min_required} points, got {count}"
            ),
            Self::ParameterCountMismatch { expected, actual } => write!(
                f,
                "expected {expected} interpolation parameters, got {actual}"
            ),
            Self::InvalidParameterSequence { index } => write!(
                f,
                "interpolation parameter sequence is invalid at index {index}"
            ),
            Self::ZeroChordLength { index } => write!(
                f,
                "consecutive interpolation points at index {index} have zero chord length"
            ),
            Self::SingularInterpolationSystem => {
                write!(f, "global curve interpolation system is singular")
            }
        }
    }
}

impl std::error::Error for BSplineError {}

/// Validated clamped polynomial or rational 3D B-spline (NURBS) curve.
///
/// Follows Piegl & Tiller Chapters 3–4 (`refs/text/piegl_tiller1997_nurbs_book.txt:3755-3820, 5350-5440`):
/// - `degree = p >= 1`
/// - `control_points.len() = n + 1 >= p + 1`
/// - `knots.len() = n + p + 2`, non-decreasing, clamped with multiplicity `p + 1`
///   at both ends (`knots[0..=p] == u_min`, `knots[n+1..=n+p+1] == u_max`, `u_min < u_max`)
///   and interior knot multiplicities `<= p`
/// - `weights`: `None` for non-rational polynomial curves, or `Some(w)` with
///   `w.len() == n + 1` and all `w[i]` finite and strictly `> 0.0`.
#[derive(Debug, Clone, PartialEq)]
pub struct BSplineCurve {
    degree: usize,
    knots: Vec<f64>,
    control_points: Vec<Point3>,
    weights: Option<Vec<f64>>,
}

/// Validated clamped tensor-product polynomial or rational 3D B-spline (NURBS) surface.
///
/// Follows Piegl & Tiller Chapters 3–4 (`refs/text/piegl_tiller1997_nurbs_book.txt:4595-4642, 5718-5746`):
/// - `degree_u = p >= 1`, `degree_v = q >= 1`
/// - `control_points` has `(n_u + 1)` rows by `(n_v + 1)` columns with
///   `n_u + 1 >= p + 1` and `n_v + 1 >= q + 1`
/// - `knots_u` (`len = n_u + p + 2`) and `knots_v` (`len = n_v + q + 2`) are
///   clamped non-decreasing knot vectors with interior multiplicities `<= p` and `<= q`
/// - `weights`: `None` for polynomial surfaces, or `Some(w)` matching `(n_u + 1) x (n_v + 1)`
///   with every `w[i][j]` finite and strictly `> 0.0`.
#[derive(Debug, Clone, PartialEq)]
pub struct BSplineSurface {
    degree_u: usize,
    degree_v: usize,
    knots_u: Vec<f64>,
    knots_v: Vec<f64>,
    control_points: Vec<Vec<Point3>>,
    weights: Option<Vec<Vec<f64>>>,
}

fn is_finite_point(p: Point3) -> bool {
    p.x().is_finite() && p.y().is_finite() && p.z().is_finite()
}

fn validate_clamped_knots(
    degree: usize,
    num_ctrl: usize,
    knots: &[f64],
) -> Result<(), BSplineError> {
    if degree == 0 {
        return Err(BSplineError::ZeroDegree);
    }
    if num_ctrl < degree + 1 {
        return Err(BSplineError::TooFewControlPoints {
            degree,
            count: num_ctrl,
        });
    }
    let expected_knots = num_ctrl + degree + 1;
    if knots.len() != expected_knots {
        return Err(BSplineError::KnotCountMismatch {
            expected: expected_knots,
            actual: knots.len(),
        });
    }
    for (idx, &u) in knots.iter().enumerate() {
        if !u.is_finite() {
            return Err(BSplineError::NonFiniteKnot {
                index: idx,
                value: u,
            });
        }
    }
    for idx in 0..(knots.len() - 1) {
        if knots[idx] > knots[idx + 1] {
            return Err(BSplineError::DecreasingKnots {
                index: idx,
                prev: knots[idx],
                next: knots[idx + 1],
            });
        }
    }
    let m = expected_knots - 1;
    let u_min = knots[0];
    let u_max = knots[m];
    if u_max <= u_min {
        return Err(BSplineError::DegenerateDomain { u_min, u_max });
    }
    for (idx, &val) in knots.iter().enumerate().take(degree + 1).skip(1) {
        if val != u_min {
            return Err(BSplineError::UnclampedStart {
                index: idx,
                expected: u_min,
                actual: val,
            });
        }
    }
    for (idx, &val) in knots.iter().enumerate().take(m).skip(m - degree) {
        if val != u_max {
            return Err(BSplineError::UnclampedEnd {
                index: idx,
                expected: u_max,
                actual: val,
            });
        }
    }
    // Interior knots occupy indices `(degree + 1) .. (m - degree)`.
    let interior_start = degree + 1;
    let interior_end = m - degree;
    let mut idx = interior_start;
    while idx < interior_end {
        let val = knots[idx];
        if val <= u_min || val >= u_max {
            return Err(BSplineError::InteriorKnotAtBoundary {
                index: idx,
                knot: val,
            });
        }
        let mut mult = 1usize;
        while idx + mult < interior_end && knots[idx + mult] == val {
            mult += 1;
        }
        if mult > degree {
            return Err(BSplineError::InteriorKnotMultiplicityExceedsDegree {
                knot: val,
                multiplicity: mult,
                degree,
            });
        }
        idx += mult;
    }
    Ok(())
}

/// Piegl & Tiller **Algorithm A2.1** (`FindSpan`,
/// `refs/text/piegl_tiller1997_nurbs_book.txt:3240-3255`).
///
/// Determines the knot span index `i` (`p <= i <= n`) such that `u` lies in
/// `[knots[i], knots[i + 1])`, with the right-endpoint convention `u == knots[n + 1]`
/// mapped to `n` (or the last non-empty span `< knots[n + 1]`).
///
/// Returns [`BSplineError::ParameterOutOfRange`] rather than clamping when
/// `u < knots[p]` or `u > knots[n + 1]`.
pub fn find_span(n: usize, p: usize, u: f64, knots: &[f64]) -> Result<usize, BSplineError> {
    if n < p {
        return Err(BSplineError::TooFewControlPoints {
            degree: p,
            count: n + 1,
        });
    }
    let expected_knots = n + p + 2;
    if knots.len() != expected_knots {
        return Err(BSplineError::KnotCountMismatch {
            expected: expected_knots,
            actual: knots.len(),
        });
    }
    let u_min = knots[p];
    let u_max = knots[n + 1];
    if !u_min.is_finite() {
        return Err(BSplineError::NonFiniteKnot {
            index: p,
            value: u_min,
        });
    }
    if !u_max.is_finite() {
        return Err(BSplineError::NonFiniteKnot {
            index: n + 1,
            value: u_max,
        });
    }
    if u_max <= u_min {
        return Err(BSplineError::DegenerateDomain { u_min, u_max });
    }
    if !u.is_finite() || u < u_min || u > u_max {
        return Err(BSplineError::ParameterOutOfRange {
            param: u,
            min: u_min,
            max: u_max,
        });
    }
    if u == u_max {
        let mut span = n;
        while span > p && knots[span] == u_max {
            span -= 1;
        }
        return Ok(span);
    }
    let mut low = p;
    let mut high = n + 1;
    let mut mid = (low + high) / 2;
    while u < knots[mid] || u >= knots[mid + 1] {
        if !knots[mid].is_finite() {
            return Err(BSplineError::NonFiniteKnot {
                index: mid,
                value: knots[mid],
            });
        }
        if !knots[mid + 1].is_finite() {
            return Err(BSplineError::NonFiniteKnot {
                index: mid + 1,
                value: knots[mid + 1],
            });
        }
        if u < knots[mid] {
            high = mid;
        } else {
            low = mid;
        }
        let next_mid = (low + high) / 2;
        if next_mid == mid {
            // Only reachable if `knots` violates monotonicity.
            for idx in 0..(knots.len() - 1) {
                if knots[idx] > knots[idx + 1] {
                    return Err(BSplineError::DecreasingKnots {
                        index: idx,
                        prev: knots[idx],
                        next: knots[idx + 1],
                    });
                }
            }
            return Err(BSplineError::ZeroKnotSpan { span: mid });
        }
        mid = next_mid;
    }
    Ok(mid)
}

/// Piegl & Tiller **Algorithm A2.2** (`BasisFuns`,
/// `refs/text/piegl_tiller1997_nurbs_book.txt:3318-3337`).
///
/// Computes the `p + 1` non-vanishing B-spline basis functions
/// `N[0..=p] = [N_{span-p, p}(u), ..., N_{span, p}(u)]`.
pub fn basis_funs(span: usize, u: f64, p: usize, knots: &[f64]) -> Result<Vec<f64>, BSplineError> {
    if span < p || span + p >= knots.len() || span + 1 >= knots.len() {
        return Err(BSplineError::InvalidSpan {
            span,
            degree: p,
            knot_len: knots.len(),
        });
    }
    if !u.is_finite() {
        return Err(BSplineError::ParameterOutOfRange {
            param: u,
            min: knots[p],
            max: knots[knots.len() - p - 1],
        });
    }
    if knots[span + 1] <= knots[span] {
        return Err(BSplineError::ZeroKnotSpan { span });
    }
    let mut n_vals = vec![0.0; p + 1];
    let mut left = vec![0.0; p + 1];
    let mut right = vec![0.0; p + 1];
    n_vals[0] = 1.0;
    for j in 1..=p {
        left[j] = u - knots[span + 1 - j];
        right[j] = knots[span + j] - u;
        let mut saved = 0.0;
        for r in 0..j {
            let denom = right[r + 1] + left[j - r];
            if denom <= 0.0 || !denom.is_finite() {
                return Err(BSplineError::ZeroKnotSpan { span });
            }
            let nr = n_vals[r];
            n_vals[r] = saved + (nr * right[r + 1]) / denom;
            saved = (nr * left[j - r]) / denom;
        }
        n_vals[j] = saved;
    }
    Ok(n_vals)
}

/// Piegl & Tiller **Algorithm A2.3** (`DersBasisFuns`,
/// `refs/text/piegl_tiller1997_nurbs_book.txt:3431-3499`).
///
/// Computes the non-vanishing basis functions and their derivatives up to order
/// `d`. Returns `ders` of shape `(d + 1) x (p + 1)` where `ders[k][j]` is the
/// `k`-th derivative `N^{(k)}_{span - p + j, p}(u)`. Orders `k > p` vanish
/// identically and are filled with `0.0`.
pub fn ders_basis_funs(
    span: usize,
    u: f64,
    p: usize,
    d: usize,
    knots: &[f64],
) -> Result<Vec<Vec<f64>>, BSplineError> {
    if span < p || span + p >= knots.len() || span + 1 >= knots.len() {
        return Err(BSplineError::InvalidSpan {
            span,
            degree: p,
            knot_len: knots.len(),
        });
    }
    if !u.is_finite() {
        return Err(BSplineError::ParameterOutOfRange {
            param: u,
            min: knots[p],
            max: knots[knots.len() - p - 1],
        });
    }
    if knots[span + 1] <= knots[span] {
        return Err(BSplineError::ZeroKnotSpan { span });
    }

    let du = d.min(p);
    let mut ders = vec![vec![0.0; p + 1]; d + 1];
    let mut ndu = vec![vec![0.0; p + 1]; p + 1];
    let mut left = vec![0.0; p + 1];
    let mut right = vec![0.0; p + 1];
    ndu[0][0] = 1.0;

    for j in 1..=p {
        left[j] = u - knots[span + 1 - j];
        right[j] = knots[span + j] - u;
        let mut saved = 0.0;
        for r in 0..j {
            let denom = right[r + 1] + left[j - r];
            if denom <= 0.0 || !denom.is_finite() {
                return Err(BSplineError::ZeroKnotSpan { span });
            }
            ndu[j][r] = denom;
            let nr = ndu[r][j - 1];
            ndu[r][j] = saved + (nr * right[r + 1]) / denom;
            saved = (nr * left[j - r]) / denom;
        }
        ndu[j][j] = saved;
    }

    for (dst, row) in ders[0].iter_mut().zip(ndu.iter()) {
        *dst = row[p];
    }

    let mut a = vec![vec![0.0; p + 1]; 2];
    for r in 0..=p {
        let mut s1 = 0usize;
        let mut s2 = 1usize;
        a[0][0] = 1.0;
        for k in 1..=du {
            let mut d_val = 0.0;
            let rk = (r as isize) - (k as isize);
            let pk = (p as isize) - (k as isize);
            let pk_u = pk as usize;
            if r >= k {
                let rk_u = rk as usize;
                a[s2][0] = a[s1][0] / ndu[pk_u + 1][rk_u];
                d_val = a[s2][0] * ndu[rk_u][pk_u];
            }
            let j1: usize = if rk >= -1 { 1 } else { (-rk) as usize };
            let j2: usize = if (r as isize) - 1 <= pk { k - 1 } else { p - r };
            for j in j1..=j2 {
                let rkj = (rk + (j as isize)) as usize;
                a[s2][j] = (a[s1][j] - a[s1][j - 1]) / ndu[pk_u + 1][rkj];
                d_val += a[s2][j] * ndu[rkj][pk_u];
            }
            if (r as isize) <= pk {
                a[s2][k] = -a[s1][k - 1] / ndu[pk_u + 1][r];
                d_val += a[s2][k] * ndu[r][pk_u];
            }
            ders[k][r] = d_val;
            std::mem::swap(&mut s1, &mut s2);
        }
    }

    let mut factor = p as f64;
    for (k, ders_k) in ders.iter_mut().enumerate().take(du + 1).skip(1) {
        for val in ders_k {
            *val *= factor;
        }
        factor *= (p - k) as f64;
    }

    Ok(ders)
}

/// Binomial coefficient `C(n, k)` computed in exact `f64` recurrence without integer overflow.
fn binomial(n: usize, k: usize) -> f64 {
    if k > n {
        return 0.0;
    }
    let k = k.min(n - k);
    let mut acc = 1.0f64;
    for i in 0..k {
        acc = acc * ((n - i) as f64) / ((i + 1) as f64);
    }
    acc
}

/// Piegl & Tiller **Algorithm A4.2** (`RatCurveDerivs`,
/// `refs/text/piegl_tiller1997_nurbs_book.txt:5685-5697`).
///
/// Computes `CK[0..=d]` (`CK[0]` = point coordinates, `CK[k]` = `k`-th
/// derivative `C^{(k)}(u)`) from the homogeneous derivatives `Aders[0..=d]`
/// and weight derivatives `wders[0..=d]`.
pub fn rat_curve_derivs(
    aders: &[Vector3],
    wders: &[f64],
    d: usize,
) -> Result<Vec<Vector3>, BSplineError> {
    if aders.len() <= d || wders.len() <= d {
        return Err(BSplineError::InvalidDerivativeInputs {
            expected_len: d + 1,
            actual_aders: aders.len(),
            actual_wders: wders.len(),
        });
    }
    let w0 = wders[0];
    if !w0.is_finite() || w0 <= 0.0 {
        return Err(BSplineError::NonPositiveWeight {
            index: 0,
            weight: w0,
        });
    }
    let mut ck = vec![Vector3::new(0.0, 0.0, 0.0); d + 1];
    for k in 0..=d {
        let mut v = aders[k].as_array();
        for i in 1..=k {
            let scale = binomial(k, i) * wders[i];
            let prev = ck[k - i].as_array();
            v[0] -= scale * prev[0];
            v[1] -= scale * prev[1];
            v[2] -= scale * prev[2];
        }
        ck[k] = Vector3::new(v[0] / w0, v[1] / w0, v[2] / w0);
    }
    Ok(ck)
}

/// Piegl & Tiller **Algorithm A4.4** (`RatSurfaceDerivs`,
/// `refs/text/piegl_tiller1997_nurbs_book.txt:6155-6188`).
///
/// Computes partial derivatives `SKL[k][l]` (`0 <= k + l <= d`, where
/// `SKL[0][0]` holds the 3D surface point coordinates) from the homogeneous
/// numerator partial derivatives `Aders` and weight partial derivatives `wders`.
pub fn rat_surface_derivs(
    aders: &[Vec<Vector3>],
    wders: &[Vec<f64>],
    d: usize,
) -> Result<Vec<Vec<Vector3>>, BSplineError> {
    if aders.len() <= d || wders.len() <= d {
        return Err(BSplineError::InvalidDerivativeInputs {
            expected_len: d + 1,
            actual_aders: aders.len(),
            actual_wders: wders.len(),
        });
    }
    for k in 0..=d {
        let needed = d - k + 1;
        if aders[k].len() < needed || wders[k].len() < needed {
            return Err(BSplineError::InvalidDerivativeInputs {
                expected_len: needed,
                actual_aders: aders[k].len(),
                actual_wders: wders[k].len(),
            });
        }
    }
    let w00 = wders[0][0];
    if !w00.is_finite() || w00 <= 0.0 {
        return Err(BSplineError::NonPositiveWeight {
            index: 0,
            weight: w00,
        });
    }

    let mut skl = vec![vec![Vector3::new(0.0, 0.0, 0.0); d + 1]; d + 1];
    for k in 0..=d {
        for l in 0..=(d - k) {
            let mut v = aders[k][l].as_array();
            for j in 1..=l {
                let c = binomial(l, j) * wders[0][j];
                let prev = skl[k][l - j].as_array();
                v[0] -= c * prev[0];
                v[1] -= c * prev[1];
                v[2] -= c * prev[2];
            }
            for i in 1..=k {
                let bin_ki = binomial(k, i);
                let c0 = bin_ki * wders[i][0];
                let prev0 = skl[k - i][l].as_array();
                v[0] -= c0 * prev0[0];
                v[1] -= c0 * prev0[1];
                v[2] -= c0 * prev0[2];

                let mut v2 = [0.0; 3];
                for j in 1..=l {
                    let c_lj = binomial(l, j) * wders[i][j];
                    let prev_ij = skl[k - i][l - j].as_array();
                    v2[0] += c_lj * prev_ij[0];
                    v2[1] += c_lj * prev_ij[1];
                    v2[2] += c_lj * prev_ij[2];
                }
                v[0] -= bin_ki * v2[0];
                v[1] -= bin_ki * v2[1];
                v[2] -= bin_ki * v2[2];
            }
            skl[k][l] = Vector3::new(v[0] / w00, v[1] / w00, v[2] / w00);
        }
    }
    Ok(skl)
}

#[inline]
fn lerp4(alpha: f64, a: [f64; 4], b: [f64; 4]) -> [f64; 4] {
    let om = 1.0 - alpha;
    [
        alpha * a[0] + om * b[0],
        alpha * a[1] + om * b[1],
        alpha * a[2] + om * b[2],
        alpha * a[3] + om * b[3],
    ]
}

impl BSplineCurve {
    /// Construct and validate a clamped polynomial or rational B-spline curve.
    pub fn new(
        degree: usize,
        knots: Vec<f64>,
        control_points: Vec<Point3>,
        weights: Option<Vec<f64>>,
    ) -> Result<Self, BSplineError> {
        validate_clamped_knots(degree, control_points.len(), &knots)?;
        for (idx, &pt) in control_points.iter().enumerate() {
            if !is_finite_point(pt) {
                return Err(BSplineError::NonFiniteControlPoint { index: idx });
            }
        }
        if let Some(ref w) = weights {
            if w.len() != control_points.len() {
                return Err(BSplineError::WeightCountMismatch {
                    expected: control_points.len(),
                    actual: w.len(),
                });
            }
            for (idx, &wi) in w.iter().enumerate() {
                if !wi.is_finite() || wi <= 0.0 {
                    return Err(BSplineError::NonPositiveWeight {
                        index: idx,
                        weight: wi,
                    });
                }
            }
        }
        Ok(Self {
            degree,
            knots,
            control_points,
            weights,
        })
    }

    /// Curve polynomial degree `p >= 1`.
    pub fn degree(&self) -> usize {
        self.degree
    }

    /// Clamped non-decreasing knot vector `U` of length `n + p + 2`.
    pub fn knots(&self) -> &[f64] {
        &self.knots
    }

    /// Control polygon vertices `P_0 ..= P_n`.
    pub fn control_points(&self) -> &[Point3] {
        &self.control_points
    }

    /// Optional positive rational weights `w_0 ..= w_n`.
    pub fn weights(&self) -> Option<&[f64]> {
        self.weights.as_deref()
    }

    /// Whether this curve carries explicit rational weights.
    pub fn is_rational(&self) -> bool {
        self.weights.is_some()
    }

    /// Parameter domain `(u_min, u_max)`.
    pub fn domain(&self) -> (f64, f64) {
        (
            self.knots[self.degree],
            self.knots[self.control_points.len()],
        )
    }

    /// Consume the curve and return `(degree, knots, control_points, weights)`.
    pub fn into_parts(self) -> (usize, Vec<f64>, Vec<Point3>, Option<Vec<f64>>) {
        (self.degree, self.knots, self.control_points, self.weights)
    }

    fn to_homogeneous(&self) -> Vec<[f64; 4]> {
        match &self.weights {
            None => self
                .control_points
                .iter()
                .map(|p| [p.x(), p.y(), p.z(), 1.0])
                .collect(),
            Some(w) => self
                .control_points
                .iter()
                .zip(w.iter())
                .map(|(p, &wi)| [wi * p.x(), wi * p.y(), wi * p.z(), wi])
                .collect(),
        }
    }

    fn from_homogeneous(
        degree: usize,
        knots: Vec<f64>,
        qw: &[[f64; 4]],
        rational: bool,
    ) -> Result<Self, BSplineError> {
        if rational {
            let mut pts = Vec::with_capacity(qw.len());
            let mut weights = Vec::with_capacity(qw.len());
            for (idx, c) in qw.iter().enumerate() {
                let w = c[3];
                if !w.is_finite() || w <= 0.0 {
                    return Err(BSplineError::NonPositiveWeight {
                        index: idx,
                        weight: w,
                    });
                }
                pts.push(Point3::new(c[0] / w, c[1] / w, c[2] / w));
                weights.push(w);
            }
            Self::new(degree, knots, pts, Some(weights))
        } else {
            let pts = qw.iter().map(|c| Point3::new(c[0], c[1], c[2])).collect();
            Self::new(degree, knots, pts, None)
        }
    }

    /// Evaluate the 3D point `C(u)` on the curve.
    ///
    /// Implements Piegl & Tiller **Algorithm A3.1** (`CurvePoint`,
    /// `refs/text/piegl_tiller1997_nurbs_book.txt:3800-3810`) for polynomial
    /// curves and **Algorithm A4.1** (`CurvePoint` rational,
    /// `refs/text/piegl_tiller1997_nurbs_book.txt:5578-5589`) for rational curves.
    pub fn point_at(&self, u: f64) -> Result<Point3, BSplineError> {
        let n = self.control_points.len() - 1;
        let p = self.degree;
        let span = find_span(n, p, u, &self.knots)?;
        let basis = basis_funs(span, u, p, &self.knots)?;
        match &self.weights {
            None => {
                let mut c = [0.0; 3];
                for (j, &nj) in basis.iter().enumerate() {
                    let pt = self.control_points[span - p + j].as_array();
                    c[0] += nj * pt[0];
                    c[1] += nj * pt[1];
                    c[2] += nj * pt[2];
                }
                Ok(Point3::new(c[0], c[1], c[2]))
            }
            Some(weights) => {
                let mut cw = [0.0; 4];
                for (j, &nj) in basis.iter().enumerate() {
                    let idx = span - p + j;
                    let pt = self.control_points[idx].as_array();
                    let w = weights[idx];
                    let nw = nj * w;
                    cw[0] += nw * pt[0];
                    cw[1] += nw * pt[1];
                    cw[2] += nw * pt[2];
                    cw[3] += nw;
                }
                Ok(Point3::new(cw[0] / cw[3], cw[1] / cw[3], cw[2] / cw[3]))
            }
        }
    }

    /// Evaluate the curve point (`CK[0]`) and derivatives `C^{(k)}(u)` (`CK[1..=d]`).
    ///
    /// Implements Piegl & Tiller **Algorithm A3.2** (`CurveDerivsAlg1`,
    /// `refs/text/piegl_tiller1997_nurbs_book.txt:4236-4251`) and, when rational,
    /// **Algorithm A4.2** (`RatCurveDerivs`,
    /// `refs/text/piegl_tiller1997_nurbs_book.txt:5685-5697`).
    pub fn derivs(&self, u: f64, d: usize) -> Result<Vec<Vector3>, BSplineError> {
        let n = self.control_points.len() - 1;
        let p = self.degree;
        let du = d.min(p);
        let span = find_span(n, p, u, &self.knots)?;
        let nders = ders_basis_funs(span, u, p, du, &self.knots)?;

        match &self.weights {
            None => {
                let mut ck = vec![Vector3::new(0.0, 0.0, 0.0); d + 1];
                for (ck_k, nders_k) in ck.iter_mut().zip(nders.iter()) {
                    let mut sum = [0.0; 3];
                    for (j, &nd) in nders_k.iter().enumerate() {
                        let pt = self.control_points[span - p + j].as_array();
                        sum[0] += nd * pt[0];
                        sum[1] += nd * pt[1];
                        sum[2] += nd * pt[2];
                    }
                    *ck_k = Vector3::new(sum[0], sum[1], sum[2]);
                }
                Ok(ck)
            }
            Some(weights) => {
                let mut aders = vec![Vector3::new(0.0, 0.0, 0.0); d + 1];
                let mut wders = vec![0.0; d + 1];
                for ((ader_k, wder_k), nders_k) in
                    aders.iter_mut().zip(wders.iter_mut()).zip(nders.iter())
                {
                    let mut sum = [0.0; 3];
                    let mut wsum = 0.0;
                    for (j, &nd) in nders_k.iter().enumerate() {
                        let idx = span - p + j;
                        let pt = self.control_points[idx].as_array();
                        let nw = nd * weights[idx];
                        sum[0] += nw * pt[0];
                        sum[1] += nw * pt[1];
                        sum[2] += nw * pt[2];
                        wsum += nw;
                    }
                    *ader_k = Vector3::new(sum[0], sum[1], sum[2]);
                    *wder_k = wsum;
                }
                rat_curve_derivs(&aders, &wders, d)
            }
        }
    }

    /// Insert the sorted interior knot vector `x` simultaneously without altering
    /// curve geometry.
    ///
    /// Implements Piegl & Tiller **Algorithm A5.4** (`RefineKnotVectCurve`,
    /// `refs/text/piegl_tiller1997_nurbs_book.txt:7235-7347`).
    pub fn refine_knots(&self, x: &[f64]) -> Result<Self, BSplineError> {
        if x.is_empty() {
            return Err(BSplineError::EmptyRefinementKnots);
        }
        let (u_min, u_max) = self.domain();
        for &val in x {
            if !val.is_finite() || val <= u_min || val >= u_max {
                return Err(BSplineError::RefinementKnotOutOfRange {
                    knot: val,
                    min: u_min,
                    max: u_max,
                });
            }
        }
        for idx in 0..(x.len() - 1) {
            if x[idx] > x[idx + 1] {
                return Err(BSplineError::UnsortedRefinementKnots {
                    index: idx,
                    prev: x[idx],
                    next: x[idx + 1],
                });
            }
        }
        let p = self.degree;
        let mut idx = 0usize;
        while idx < x.len() {
            let val = x[idx];
            let mut ins_mult = 1usize;
            while idx + ins_mult < x.len() && x[idx + ins_mult] == val {
                ins_mult += 1;
            }
            let existing_mult = self.knots.iter().filter(|&&uk| uk == val).count();
            let total_multiplicity = existing_mult + ins_mult;
            if total_multiplicity > p {
                return Err(BSplineError::RefinementMultiplicityExceedsDegree {
                    knot: val,
                    total_multiplicity,
                    degree: p,
                });
            }
            idx += ins_mult;
        }

        let n = self.control_points.len() - 1;
        let m = n + p + 1;
        let r = x.len() - 1;
        let u = &self.knots;
        let pw = self.to_homogeneous();

        let a = find_span(n, p, x[0], u)?;
        let b = find_span(n, p, x[r], u)? + 1;

        let mut qw = vec![[0.0; 4]; n + r + 2];
        let mut ubar = vec![0.0; m + r + 2];

        qw[..=(a - p)].copy_from_slice(&pw[..=(a - p)]);
        qw[(b + r)..=(n + r + 1)].copy_from_slice(&pw[(b - 1)..=n]);
        ubar[..=a].copy_from_slice(&u[..=a]);
        ubar[(b + p + r + 1)..=(m + r + 1)].copy_from_slice(&u[(b + p)..=m]);

        let mut i = b + p - 1;
        let mut k = b + p + r;
        for j in (0..=r).rev() {
            while x[j] <= u[i] && i > a {
                qw[k - p - 1] = pw[i - p - 1];
                ubar[k] = u[i];
                k -= 1;
                i -= 1;
            }
            qw[k - p - 1] = qw[k - p];
            for l in 1..=p {
                let ind = k - p + l;
                let alfa = ubar[k + l] - x[j];
                if alfa.abs() == 0.0 {
                    qw[ind - 1] = qw[ind];
                } else {
                    let alfa = alfa / (ubar[k + l] - u[i - p + l]);
                    qw[ind - 1] = lerp4(alfa, qw[ind - 1], qw[ind]);
                }
            }
            ubar[k] = x[j];
            k -= 1;
        }

        Self::from_homogeneous(p, ubar, &qw, self.is_rational())
    }

    /// Decompose the curve into its constituent clamped Bézier segments, one per
    /// non-empty knot span `[u_a, u_b]`.
    ///
    /// Implements Piegl & Tiller **Algorithm A5.6** (`DecomposeCurve`,
    /// `refs/text/piegl_tiller1997_nurbs_book.txt:7620-7715`). Each returned
    /// segment is a degree-`p` [`BSplineCurve`] with `p + 1` control points on
    /// `[u_a, ..., u_a, u_b, ..., u_b]`, so evaluating segment `s` at any
    /// `u in s.domain()` agrees directly with `self.point_at(u)`.
    pub fn decompose(&self) -> Result<Vec<Self>, BSplineError> {
        let n = self.control_points.len() - 1;
        let p = self.degree;
        let u = &self.knots;
        let pw = self.to_homogeneous();

        let (qw_segs, domains) = decompose_homogeneous_1d(n, p, u, &pw);
        let rational = self.is_rational();
        let mut segments = Vec::with_capacity(qw_segs.len());
        for (seg_qw, (ua, ub)) in qw_segs.iter().zip(domains) {
            let mut seg_knots = vec![ua; 2 * (p + 1)];
            for knot in seg_knots.iter_mut().skip(p + 1) {
                *knot = ub;
            }
            segments.push(Self::from_homogeneous(p, seg_knots, seg_qw, rational)?);
        }
        Ok(segments)
    }

    /// Elevate the curve degree from `p` to `p + t` without changing its geometry
    /// or parameterization.
    ///
    /// Implements Piegl & Tiller **Algorithm A5.9** (`DegreeElevateCurve`,
    /// `refs/text/piegl_tiller1997_nurbs_book.txt:8933-9080`). When `t == 0`,
    /// returns an exact clone of `self`.
    pub fn degree_elevate(&self, t: usize) -> Result<Self, BSplineError> {
        if t == 0 {
            return Ok(self.clone());
        }
        let n = self.control_points.len() - 1;
        let p = self.degree;
        let m = n + p + 1;
        let u = &self.knots;
        let pw = self.to_homogeneous();

        let ph = p + t;
        let ph2 = ph / 2;

        // Precompute Bezier degree-elevation coefficients (Eq. 5.36, `:8941-8985`).
        let mut bezalfs = vec![vec![0.0; p + 1]; ph + 1];
        bezalfs[0][0] = 1.0;
        bezalfs[ph][p] = 1.0;
        for (i, row) in bezalfs.iter_mut().enumerate().take(ph2 + 1).skip(1) {
            let inv = 1.0 / binomial(ph, i);
            let mpi = p.min(i);
            let j_start = i.saturating_sub(t);
            for (j, cell) in row.iter_mut().enumerate().take(mpi + 1).skip(j_start) {
                *cell = inv * binomial(p, j) * binomial(t, i - j);
            }
        }
        for i in (ph2 + 1)..ph {
            let mpi = p.min(i);
            let j_start = i.saturating_sub(t);
            for j in j_start..=mpi {
                bezalfs[i][j] = bezalfs[ph - i][p - j];
            }
        }

        // Count distinct interior knots to size `Uh` and `Qw` exactly (`:8999`).
        let num_distinct_interior = {
            let mut cnt = 0usize;
            let mut idx = p + 1;
            while idx < m - p {
                let val = u[idx];
                cnt += 1;
                while idx < m - p && u[idx] == val {
                    idx += 1;
                }
            }
            cnt
        };
        // Every distinct interior knot gains `+t` multiplicity, and both endpoints gain `+t`.
        let mh_expected = m + (num_distinct_interior + 2) * t;
        let nh_expected = mh_expected - ph - 1;

        let mut uh = vec![0.0; mh_expected + 1];
        let mut qw = vec![[0.0; 4]; nh_expected + 1];
        let mut bpts = vec![[0.0; 4]; p + 1];
        let mut ebpts = vec![[0.0; 4]; ph + 1];
        let mut next_bpts = vec![[0.0; 4]; p];
        let mut alfs = vec![0.0; p];

        let mut mh = ph;
        let mut kind = ph + 1;
        let mut r: isize = -1;
        let mut a = p;
        let mut b = p + 1;
        let mut cind = 1usize;
        let mut ua = u[0];

        qw[0] = pw[0];
        for knot in uh.iter_mut().take(ph + 1) {
            *knot = ua;
        }
        bpts[..(p + 1)].copy_from_slice(&pw[..(p + 1)]);

        while b < m {
            let i_start = b;
            while b < m && u[b] == u[b + 1] {
                b += 1;
            }
            let mul = b - i_start + 1;
            mh += mul + t;
            let ub = u[b];
            let oldr = r;
            r = (p as isize) - (mul as isize);

            let lbz: usize = if oldr > 0 {
                ((oldr + 2) / 2) as usize
            } else {
                1
            };
            let rbz: usize = if r > 0 {
                ph - (((r + 1) / 2) as usize)
            } else {
                ph
            };

            if r > 0 {
                let r_u = r as usize;
                let numer = ub - ua;
                for k in ((mul + 1)..=p).rev() {
                    alfs[k - mul - 1] = numer / (u[a + k] - ua);
                }
                for j in 1..=r_u {
                    let save = r_u - j;
                    let s = mul + j;
                    for k in (s..=p).rev() {
                        bpts[k] = lerp4(alfs[k - s], bpts[k], bpts[k - 1]);
                    }
                    next_bpts[save] = bpts[p];
                }
            }

            for (i, ebpt) in ebpts.iter_mut().enumerate().take(ph + 1).skip(lbz) {
                let mut sum = [0.0; 4];
                let mpi = p.min(i);
                let j_start = i.saturating_sub(t);
                for (&c, bpt) in bezalfs[i][j_start..=mpi]
                    .iter()
                    .zip(bpts[j_start..=mpi].iter())
                {
                    sum[0] += c * bpt[0];
                    sum[1] += c * bpt[1];
                    sum[2] += c * bpt[2];
                    sum[3] += c * bpt[3];
                }
                *ebpt = sum;
            }

            if oldr > 1 {
                let oldr_u = oldr as usize;
                let den = ub - ua;
                let bet = (ub - uh[kind - 1]) / den;
                for tr in 1..oldr_u {
                    let tr_i = tr as isize;
                    let mut i_idx = (kind as isize) - 1 - tr_i;
                    let mut j_idx = (kind as isize) - 1 + tr_i;
                    let mut kj = j_idx - (kind as isize) + 1;
                    while j_idx - i_idx > tr_i {
                        let iu = i_idx as usize;
                        if iu < cind {
                            let alf = (ub - uh[iu]) / (ua - uh[iu]);
                            qw[iu] = lerp4(alf, qw[iu], qw[iu - 1]);
                        }
                        if j_idx >= (lbz as isize) {
                            let kju = kj as usize;
                            let threshold = (kind as isize) - (ph as isize) + oldr;
                            if j_idx - tr_i <= threshold {
                                let gam = (ub - uh[(j_idx - tr_i) as usize]) / den;
                                ebpts[kju] = lerp4(gam, ebpts[kju], ebpts[kju + 1]);
                            } else {
                                ebpts[kju] = lerp4(bet, ebpts[kju], ebpts[kju + 1]);
                            }
                        }
                        i_idx += 1;
                        j_idx -= 1;
                        kj -= 1;
                    }
                }
            }

            if a != p {
                let load_count = ((ph as isize) - oldr) as usize;
                uh[kind..(kind + load_count)].fill(ua);
                kind += load_count;
            }

            if rbz >= lbz {
                let count = rbz - lbz + 1;
                qw[cind..(cind + count)].copy_from_slice(&ebpts[lbz..=rbz]);
                cind += count;
            }

            if b < m {
                let r_u = r.max(0) as usize;
                bpts[..r_u].copy_from_slice(&next_bpts[..r_u]);
                bpts[r_u..=p].copy_from_slice(&pw[(b - p + r_u)..=b]);
                a = b;
                b += 1;
                ua = ub;
            } else {
                uh[kind..=(kind + ph)].fill(ub);
            }
        }

        debug_assert_eq!(mh, mh_expected);
        debug_assert_eq!(cind, nh_expected + 1);
        Self::from_homogeneous(ph, uh, &qw, self.is_rational())
    }
}

type HomogeneousBezier1d = (Vec<Vec<[f64; 4]>>, Vec<(f64, f64)>);

/// Shared 1D homogeneous Bézier decomposition kernel (`Algorithm A5.6`,
/// `refs/text/piegl_tiller1997_nurbs_book.txt:7620-7715`).
fn decompose_homogeneous_1d(n: usize, p: usize, u: &[f64], pw: &[[f64; 4]]) -> HomogeneousBezier1d {
    let m = n + p + 1;
    let num_spans = (p..=n).filter(|&idx| u[idx] < u[idx + 1]).count();
    let mut qw = vec![vec![[0.0; 4]; p + 1]; num_spans];
    let mut domains = Vec::with_capacity(num_spans);
    let mut alphas = vec![0.0; p];

    let mut a = p;
    let mut b = p + 1;
    let mut nb = 0usize;
    qw[0][..(p + 1)].copy_from_slice(&pw[..(p + 1)]);

    while b < m {
        let i = b;
        while b < m && u[b + 1] == u[b] {
            b += 1;
        }
        let mult = b - i + 1;
        domains.push((u[a], u[b]));
        if mult < p {
            let numer = u[b] - u[a];
            for j in ((mult + 1)..=p).rev() {
                alphas[j - mult - 1] = numer / (u[a + j] - u[a]);
            }
            let r = p - mult;
            for j in 1..=r {
                let save = r - j;
                let s = mult + j;
                for k in (s..=p).rev() {
                    let alpha = alphas[k - s];
                    qw[nb][k] = lerp4(alpha, qw[nb][k], qw[nb][k - 1]);
                }
                if b < m {
                    qw[nb + 1][save] = qw[nb][p];
                }
            }
        }
        nb += 1;
        if b < m {
            qw[nb][(p - mult)..=p].copy_from_slice(&pw[(b - mult)..=b]);
            a = b;
            b += 1;
        }
    }

    (qw, domains)
}

impl BSplineSurface {
    /// Construct and validate a clamped tensor-product polynomial or rational B-spline surface.
    pub fn new(
        degree_u: usize,
        degree_v: usize,
        knots_u: Vec<f64>,
        knots_v: Vec<f64>,
        control_points: Vec<Vec<Point3>>,
        weights: Option<Vec<Vec<f64>>>,
    ) -> Result<Self, BSplineError> {
        if degree_u == 0 || degree_v == 0 {
            return Err(BSplineError::ZeroDegree);
        }
        if control_points.is_empty() || control_points[0].is_empty() {
            return Err(BSplineError::EmptySurfaceControlGrid);
        }
        let rows = control_points.len();
        let cols = control_points[0].len();
        for (r, row) in control_points.iter().enumerate() {
            if row.len() != cols {
                return Err(BSplineError::RaggedControlGrid {
                    row: r,
                    expected_cols: cols,
                    actual_cols: row.len(),
                });
            }
            for (c, &pt) in row.iter().enumerate() {
                if !is_finite_point(pt) {
                    return Err(BSplineError::NonFiniteControlPoint {
                        index: r * cols + c,
                    });
                }
            }
        }

        validate_clamped_knots(degree_u, rows, &knots_u)?;
        validate_clamped_knots(degree_v, cols, &knots_v)?;

        if let Some(ref w_grid) = weights {
            if w_grid.len() != rows {
                return Err(BSplineError::WeightCountMismatch {
                    expected: rows * cols,
                    actual: w_grid.iter().map(Vec::len).sum(),
                });
            }
            for (r, w_row) in w_grid.iter().enumerate() {
                if w_row.len() != cols {
                    return Err(BSplineError::RaggedWeightGrid {
                        row: r,
                        expected_cols: cols,
                        actual_cols: w_row.len(),
                    });
                }
                for (c, &wij) in w_row.iter().enumerate() {
                    if !wij.is_finite() || wij <= 0.0 {
                        return Err(BSplineError::NonPositiveWeight {
                            index: r * cols + c,
                            weight: wij,
                        });
                    }
                }
            }
        }

        Ok(Self {
            degree_u,
            degree_v,
            knots_u,
            knots_v,
            control_points,
            weights,
        })
    }

    /// Surface degree `p >= 1` in the `u` direction.
    pub fn degree_u(&self) -> usize {
        self.degree_u
    }

    /// Surface degree `q >= 1` in the `v` direction.
    pub fn degree_v(&self) -> usize {
        self.degree_v
    }

    /// Clamped knot vector `U` in the `u` direction.
    pub fn knots_u(&self) -> &[f64] {
        &self.knots_u
    }

    /// Clamped knot vector `V` in the `v` direction.
    pub fn knots_v(&self) -> &[f64] {
        &self.knots_v
    }

    /// Control net `P[0..=n_u][0..=n_v]`.
    pub fn control_points(&self) -> &[Vec<Point3>] {
        &self.control_points
    }

    /// Optional positive rational weights `w[0..=n_u][0..=n_v]`.
    pub fn weights(&self) -> Option<&[Vec<f64>]> {
        self.weights.as_deref()
    }

    /// Whether this surface carries explicit rational weights.
    pub fn is_rational(&self) -> bool {
        self.weights.is_some()
    }

    /// Parameter domain `(u_min, u_max)` in `u`.
    pub fn domain_u(&self) -> (f64, f64) {
        (
            self.knots_u[self.degree_u],
            self.knots_u[self.control_points.len()],
        )
    }

    /// Parameter domain `(v_min, v_max)` in `v`.
    pub fn domain_v(&self) -> (f64, f64) {
        (
            self.knots_v[self.degree_v],
            self.knots_v[self.control_points[0].len()],
        )
    }

    /// Consume the surface and return `(degree_u, degree_v, knots_u, knots_v, control_points, weights)`.
    #[allow(clippy::type_complexity)]
    pub fn into_parts(
        self,
    ) -> (
        usize,
        usize,
        Vec<f64>,
        Vec<f64>,
        Vec<Vec<Point3>>,
        Option<Vec<Vec<f64>>>,
    ) {
        (
            self.degree_u,
            self.degree_v,
            self.knots_u,
            self.knots_v,
            self.control_points,
            self.weights,
        )
    }

    fn to_homogeneous_grid(&self) -> Vec<Vec<[f64; 4]>> {
        match &self.weights {
            None => self
                .control_points
                .iter()
                .map(|row| row.iter().map(|p| [p.x(), p.y(), p.z(), 1.0]).collect())
                .collect(),
            Some(w) => self
                .control_points
                .iter()
                .zip(w.iter())
                .map(|(row, w_row)| {
                    row.iter()
                        .zip(w_row.iter())
                        .map(|(p, &wij)| [wij * p.x(), wij * p.y(), wij * p.z(), wij])
                        .collect()
                })
                .collect(),
        }
    }

    fn from_homogeneous_grid(
        degree_u: usize,
        degree_v: usize,
        knots_u: Vec<f64>,
        knots_v: Vec<f64>,
        qw: &[Vec<[f64; 4]>],
        rational: bool,
    ) -> Result<Self, BSplineError> {
        let rows = qw.len();
        let cols = qw[0].len();
        if rational {
            let mut pts = Vec::with_capacity(rows);
            let mut weights = Vec::with_capacity(rows);
            for (r, row) in qw.iter().enumerate() {
                let mut pt_row = Vec::with_capacity(cols);
                let mut w_row = Vec::with_capacity(cols);
                for (c, &h) in row.iter().enumerate() {
                    let w = h[3];
                    if !w.is_finite() || w <= 0.0 {
                        return Err(BSplineError::NonPositiveWeight {
                            index: r * cols + c,
                            weight: w,
                        });
                    }
                    pt_row.push(Point3::new(h[0] / w, h[1] / w, h[2] / w));
                    w_row.push(w);
                }
                pts.push(pt_row);
                weights.push(w_row);
            }
            Self::new(degree_u, degree_v, knots_u, knots_v, pts, Some(weights))
        } else {
            let pts = qw
                .iter()
                .map(|row| row.iter().map(|h| Point3::new(h[0], h[1], h[2])).collect())
                .collect();
            Self::new(degree_u, degree_v, knots_u, knots_v, pts, None)
        }
    }

    /// Evaluate the 3D surface point `S(u, v)`.
    ///
    /// Implements Piegl & Tiller **Algorithm A3.5** (`SurfacePoint`,
    /// `refs/text/piegl_tiller1997_nurbs_book.txt:4668-4687`) for polynomial
    /// surfaces and **Algorithm A4.3** (`SurfacePoint` rational,
    /// `refs/text/piegl_tiller1997_nurbs_book.txt:6005-6024`) for rational surfaces.
    pub fn point_at(&self, u: f64, v: f64) -> Result<Point3, BSplineError> {
        let n_u = self.control_points.len() - 1;
        let n_v = self.control_points[0].len() - 1;
        let p = self.degree_u;
        let q = self.degree_v;

        let uspan = find_span(n_u, p, u, &self.knots_u)?;
        let nu = basis_funs(uspan, u, p, &self.knots_u)?;
        let vspan = find_span(n_v, q, v, &self.knots_v)?;
        let nv = basis_funs(vspan, v, q, &self.knots_v)?;
        let uind = uspan - p;

        match &self.weights {
            None => {
                let mut s = [0.0; 3];
                for (l, &nvl) in nv.iter().enumerate() {
                    let mut temp = [0.0; 3];
                    let vind = vspan - q + l;
                    for (k, &nuk) in nu.iter().enumerate() {
                        let pt = self.control_points[uind + k][vind].as_array();
                        temp[0] += nuk * pt[0];
                        temp[1] += nuk * pt[1];
                        temp[2] += nuk * pt[2];
                    }
                    s[0] += nvl * temp[0];
                    s[1] += nvl * temp[1];
                    s[2] += nvl * temp[2];
                }
                Ok(Point3::new(s[0], s[1], s[2]))
            }
            Some(weights) => {
                let mut sw = [0.0; 4];
                for (l, &nvl) in nv.iter().enumerate() {
                    let mut temp = [0.0; 4];
                    let vind = vspan - q + l;
                    for (k, &nuk) in nu.iter().enumerate() {
                        let r = uind + k;
                        let pt = self.control_points[r][vind].as_array();
                        let nw = nuk * weights[r][vind];
                        temp[0] += nw * pt[0];
                        temp[1] += nw * pt[1];
                        temp[2] += nw * pt[2];
                        temp[3] += nw;
                    }
                    sw[0] += nvl * temp[0];
                    sw[1] += nvl * temp[1];
                    sw[2] += nvl * temp[2];
                    sw[3] += nvl * temp[3];
                }
                Ok(Point3::new(sw[0] / sw[3], sw[1] / sw[3], sw[2] / sw[3]))
            }
        }
    }

    /// Evaluate the surface point (`SKL[0][0]`) and all partial derivatives
    /// `SKL[k][l] = d^{k+l} S(u, v) / (du^k dv^l)` for `0 <= k + l <= d`.
    ///
    /// Implements Piegl & Tiller **Algorithm A3.6** (`SurfaceDerivsAlg1`,
    /// `refs/text/piegl_tiller1997_nurbs_book.txt:4980-5013`) and, when rational,
    /// **Algorithm A4.4** (`RatSurfaceDerivs`,
    /// `refs/text/piegl_tiller1997_nurbs_book.txt:6155-6188`).
    pub fn derivs(&self, u: f64, v: f64, d: usize) -> Result<Vec<Vec<Vector3>>, BSplineError> {
        let n_u = self.control_points.len() - 1;
        let n_v = self.control_points[0].len() - 1;
        let p = self.degree_u;
        let q = self.degree_v;

        let du = d.min(p);
        let dv = d.min(q);
        let uspan = find_span(n_u, p, u, &self.knots_u)?;
        let nu = ders_basis_funs(uspan, u, p, du, &self.knots_u)?;
        let vspan = find_span(n_v, q, v, &self.knots_v)?;
        let nv = ders_basis_funs(vspan, v, q, dv, &self.knots_v)?;

        match &self.weights {
            None => {
                let mut skl = vec![vec![Vector3::new(0.0, 0.0, 0.0); d + 1]; d + 1];
                let mut temp = vec![[0.0; 3]; q + 1];
                for (k, (skl_k, nu_k)) in skl.iter_mut().zip(nu.iter()).enumerate() {
                    for (s, temp_s) in temp.iter_mut().enumerate() {
                        let mut acc = [0.0; 3];
                        let vind = vspan - q + s;
                        for (r, &nukr) in nu_k.iter().enumerate() {
                            let pt = self.control_points[uspan - p + r][vind].as_array();
                            acc[0] += nukr * pt[0];
                            acc[1] += nukr * pt[1];
                            acc[2] += nukr * pt[2];
                        }
                        *temp_s = acc;
                    }
                    let dd = (d - k).min(dv);
                    for (skl_kl, nv_l) in skl_k.iter_mut().zip(nv.iter()).take(dd + 1) {
                        let mut acc = [0.0; 3];
                        for (&nvls, temp_s) in nv_l.iter().zip(temp.iter()) {
                            acc[0] += nvls * temp_s[0];
                            acc[1] += nvls * temp_s[1];
                            acc[2] += nvls * temp_s[2];
                        }
                        *skl_kl = Vector3::new(acc[0], acc[1], acc[2]);
                    }
                }
                Ok(skl)
            }
            Some(weights) => {
                let mut aders = vec![vec![Vector3::new(0.0, 0.0, 0.0); d + 1]; d + 1];
                let mut wders = vec![vec![0.0; d + 1]; d + 1];
                let mut temp = vec![[0.0; 4]; q + 1];
                for (k, ((aders_k, wders_k), nu_k)) in aders
                    .iter_mut()
                    .zip(wders.iter_mut())
                    .zip(nu.iter())
                    .enumerate()
                {
                    for (s, temp_s) in temp.iter_mut().enumerate() {
                        let mut acc = [0.0; 4];
                        let vind = vspan - q + s;
                        for (r, &nukr) in nu_k.iter().enumerate() {
                            let uind = uspan - p + r;
                            let pt = self.control_points[uind][vind].as_array();
                            let nw = nukr * weights[uind][vind];
                            acc[0] += nw * pt[0];
                            acc[1] += nw * pt[1];
                            acc[2] += nw * pt[2];
                            acc[3] += nw;
                        }
                        *temp_s = acc;
                    }
                    let dd = (d - k).min(dv);
                    for ((ader_kl, wder_kl), nv_l) in aders_k
                        .iter_mut()
                        .zip(wders_k.iter_mut())
                        .zip(nv.iter())
                        .take(dd + 1)
                    {
                        let mut acc = [0.0; 4];
                        for (&nvls, temp_s) in nv_l.iter().zip(temp.iter()) {
                            acc[0] += nvls * temp_s[0];
                            acc[1] += nvls * temp_s[1];
                            acc[2] += nvls * temp_s[2];
                            acc[3] += nvls * temp_s[3];
                        }
                        *ader_kl = Vector3::new(acc[0], acc[1], acc[2]);
                        *wder_kl = acc[3];
                    }
                }
                rat_surface_derivs(&aders, &wders, d)
            }
        }
    }

    /// Decompose the surface into a `(nb_u x nb_v)` grid of clamped Bézier patches.
    ///
    /// Implements Piegl & Tiller **Algorithm A5.7** (`DecomposeSurface`,
    /// `refs/text/piegl_tiller1997_nurbs_book.txt:7751-7823`), executing the
    /// `U_DIRECTION` pass into Bézier strips followed by the `V_DIRECTION` pass
    /// into `(p, q)` Bézier patches on `[u_a, u_b] x [v_c, v_d]`.
    pub fn decompose(&self) -> Result<Vec<Vec<Self>>, BSplineError> {
        let n_u = self.control_points.len() - 1;
        let n_v = self.control_points[0].len() - 1;
        let p = self.degree_u;
        let q = self.degree_v;
        let pw = self.to_homogeneous_grid();

        // Pass 1 (`U_DIRECTION`, `:7756-7817`): decompose along `u` across all `n_v + 1` columns.
        let r_u = n_u + p + 1;
        let num_u_spans = (p..=n_u)
            .filter(|&idx| self.knots_u[idx] < self.knots_u[idx + 1])
            .count();
        let mut u_strips = vec![vec![vec![[0.0; 4]; n_v + 1]; p + 1]; num_u_spans];
        let mut u_domains = Vec::with_capacity(num_u_spans);
        let mut alphas_u = vec![0.0; p];

        let mut a = p;
        let mut b = p + 1;
        let mut nb_u = 0usize;
        for (dst_row, src_row) in u_strips[0].iter_mut().zip(pw.iter().take(p + 1)) {
            dst_row.copy_from_slice(src_row);
        }
        while b < r_u {
            let i_start = b;
            while b < r_u && self.knots_u[b + 1] == self.knots_u[b] {
                b += 1;
            }
            let mult = b - i_start + 1;
            u_domains.push((self.knots_u[a], self.knots_u[b]));
            if mult < p {
                let numer = self.knots_u[b] - self.knots_u[a];
                for j in ((mult + 1)..=p).rev() {
                    alphas_u[j - mult - 1] = numer / (self.knots_u[a + j] - self.knots_u[a]);
                }
                let r = p - mult;
                for j in 1..=r {
                    let save = r - j;
                    let s = mult + j;
                    for k in (s..=p).rev() {
                        let alfa = alphas_u[k - s];
                        let (prev_rows, cur_and_after) = u_strips[nb_u].split_at_mut(k);
                        for (cur_cell, prev_cell) in
                            cur_and_after[0].iter_mut().zip(prev_rows[k - 1].iter())
                        {
                            *cur_cell = lerp4(alfa, *cur_cell, *prev_cell);
                        }
                    }
                    if b < r_u {
                        let (done, rest) = u_strips.split_at_mut(nb_u + 1);
                        rest[0][save].copy_from_slice(&done[nb_u][p]);
                    }
                }
            }
            nb_u += 1;
            if b < r_u {
                for (dst_row, src_row) in u_strips[nb_u][(p - mult)..=p]
                    .iter_mut()
                    .zip(pw[(b - mult)..=b].iter())
                {
                    dst_row.copy_from_slice(src_row);
                }
                a = b;
                b += 1;
            }
        }

        // Pass 2 (`V_DIRECTION`, `:7818-7822`): decompose each u-strip along `v`.
        let r_v = n_v + q + 1;
        let num_v_spans = (q..=n_v)
            .filter(|&idx| self.knots_v[idx] < self.knots_v[idx + 1])
            .count();
        let rational = self.is_rational();
        let mut patches = Vec::with_capacity(num_u_spans);
        let mut alphas_v = vec![0.0; q];

        for (strip_idx, strip) in u_strips.iter().enumerate() {
            let (ua, ub) = u_domains[strip_idx];
            let mut knots_u_bez = vec![ua; 2 * (p + 1)];
            for knot in knots_u_bez.iter_mut().skip(p + 1) {
                *knot = ub;
            }

            let mut v_patches_qw = vec![vec![vec![[0.0; 4]; q + 1]; p + 1]; num_v_spans];
            let mut v_domains = Vec::with_capacity(num_v_spans);

            let mut av = q;
            let mut bv = q + 1;
            let mut nb_v = 0usize;
            for (dst_col, src_col) in v_patches_qw[0].iter_mut().zip(strip.iter()) {
                dst_col.copy_from_slice(&src_col[..=q]);
            }
            while bv < r_v {
                let i_start = bv;
                while bv < r_v && self.knots_v[bv + 1] == self.knots_v[bv] {
                    bv += 1;
                }
                let mult = bv - i_start + 1;
                v_domains.push((self.knots_v[av], self.knots_v[bv]));
                if mult < q {
                    let numer = self.knots_v[bv] - self.knots_v[av];
                    for j in ((mult + 1)..=q).rev() {
                        alphas_v[j - mult - 1] = numer / (self.knots_v[av + j] - self.knots_v[av]);
                    }
                    let r = q - mult;
                    for j in 1..=r {
                        let save = r - j;
                        let s = mult + j;
                        for k in (s..=q).rev() {
                            let alfa = alphas_v[k - s];
                            for patch_col in v_patches_qw[nb_v].iter_mut() {
                                patch_col[k] = lerp4(alfa, patch_col[k], patch_col[k - 1]);
                            }
                        }
                        if bv < r_v {
                            let (done, rest) = v_patches_qw.split_at_mut(nb_v + 1);
                            for (dst_col, src_col) in rest[0].iter_mut().zip(done[nb_v].iter()) {
                                dst_col[save] = src_col[q];
                            }
                        }
                    }
                }
                nb_v += 1;
                if bv < r_v {
                    for (dst_col, src_col) in v_patches_qw[nb_v].iter_mut().zip(strip.iter()) {
                        dst_col[(q - mult)..=q].copy_from_slice(&src_col[(bv - mult)..=bv]);
                    }
                    av = bv;
                    bv += 1;
                }
            }

            let mut patch_row = Vec::with_capacity(num_v_spans);
            for (patch_qw, (vc, vd)) in v_patches_qw.iter().zip(v_domains) {
                let mut knots_v_bez = vec![vc; 2 * (q + 1)];
                for knot in knots_v_bez.iter_mut().skip(q + 1) {
                    *knot = vd;
                }
                patch_row.push(Self::from_homogeneous_grid(
                    p,
                    q,
                    knots_u_bez.clone(),
                    knots_v_bez,
                    patch_qw,
                    rational,
                )?);
            }
            patches.push(patch_row);
        }

        Ok(patches)
    }
}

/// Chord-length parameterization (`\bar{u}_0 = 0`, `\bar{u}_n = 1`) per Piegl & Tiller
/// Eqs. (9.4)–(9.5) (`refs/text/piegl_tiller1997_nurbs_book.txt:15690-15707`).
///
/// Refuses duplicate consecutive points (`ZeroChordLength`) with no silent fallback.
pub fn chord_length_params(points: &[Point3]) -> Result<Vec<f64>, BSplineError> {
    if points.len() < 2 {
        return Err(BSplineError::InterpolationTooFewPoints {
            count: points.len(),
            min_required: 2,
        });
    }
    for (idx, &pt) in points.iter().enumerate() {
        if !is_finite_point(pt) {
            return Err(BSplineError::NonFiniteControlPoint { index: idx });
        }
    }
    let n = points.len() - 1;
    let mut chords = Vec::with_capacity(n);
    let mut total = 0.0f64;
    for k in 0..n {
        let a = points[k].as_array();
        let b = points[k + 1].as_array();
        let dx = b[0] - a[0];
        let dy = b[1] - a[1];
        let dz = b[2] - a[2];
        let len = (dx * dx + dy * dy + dz * dz).sqrt();
        if !len.is_finite() || len == 0.0 {
            return Err(BSplineError::ZeroChordLength { index: k });
        }
        chords.push(len);
        total += len;
    }
    if !total.is_finite() || total <= 0.0 {
        return Err(BSplineError::ZeroChordLength { index: 0 });
    }

    let mut uk = vec![0.0; n + 1];
    let mut accum = 0.0f64;
    for k in 1..n {
        accum += chords[k - 1];
        let val = accum / total;
        if val <= uk[k - 1] || val >= 1.0 {
            return Err(BSplineError::InvalidParameterSequence { index: k });
        }
        uk[k] = val;
    }
    uk[n] = 1.0;
    Ok(uk)
}

/// Knot averaging per Piegl & Tiller Eq. (9.8)
/// (`refs/text/piegl_tiller1997_nurbs_book.txt:15726-15730`).
pub fn averaged_knots(params: &[f64], degree: usize) -> Result<Vec<f64>, BSplineError> {
    if degree == 0 {
        return Err(BSplineError::ZeroDegree);
    }
    if params.len() < degree + 1 {
        return Err(BSplineError::InterpolationTooFewPoints {
            count: params.len(),
            min_required: degree + 1,
        });
    }
    let n = params.len() - 1;
    let u0 = params[0];
    let un = params[n];
    if !u0.is_finite() || !un.is_finite() || un <= u0 {
        return Err(BSplineError::InvalidParameterSequence { index: 0 });
    }
    for idx in 0..n {
        if !params[idx + 1].is_finite() || params[idx + 1] <= params[idx] {
            return Err(BSplineError::InvalidParameterSequence { index: idx + 1 });
        }
    }

    let m = n + degree + 1;
    let mut knots = vec![u0; m + 1];
    for j in 1..=(n - degree) {
        let sum: f64 = params[j..(j + degree)].iter().sum();
        knots[j + degree] = sum / (degree as f64);
    }
    for knot in knots.iter_mut().skip(m - degree) {
        *knot = un;
    }
    Ok(knots)
}

/// Solve `A * X = B` for `m x m` matrix `A` and `m x cols` right-hand side `B`
/// via Gaussian elimination with partial pivoting.
fn solve_linear_system(
    mut a: Vec<Vec<f64>>,
    mut b: Vec<Vec<f64>>,
) -> Result<Vec<Vec<f64>>, BSplineError> {
    let m = a.len();
    let cols = b[0].len();
    const PIVOT_FLOOR: f64 = 1e-14;

    for k in 0..m {
        let mut pivot_row = k;
        let mut max_abs = a[k][k].abs();
        for (i, row) in a.iter().enumerate().skip(k + 1) {
            let cand = row[k].abs();
            if cand > max_abs {
                max_abs = cand;
                pivot_row = i;
            }
        }
        if !max_abs.is_finite() || max_abs <= PIVOT_FLOOR {
            return Err(BSplineError::SingularInterpolationSystem);
        }
        if pivot_row != k {
            a.swap(k, pivot_row);
            b.swap(k, pivot_row);
        }
        let (piv_rows, below_rows) = a.split_at_mut(k + 1);
        let piv_a = &piv_rows[k];
        let (piv_b_rows, below_b_rows) = b.split_at_mut(k + 1);
        let piv_b = &piv_b_rows[k];
        let piv = piv_a[k];
        for (row_a, row_b) in below_rows.iter_mut().zip(below_b_rows.iter_mut()) {
            let factor = row_a[k] / piv;
            if factor != 0.0 {
                row_a[k] = 0.0;
                for (aij, &akj) in row_a[(k + 1)..m].iter_mut().zip(piv_a[(k + 1)..m].iter()) {
                    *aij -= factor * akj;
                }
                for (bic, &bkc) in row_b.iter_mut().zip(piv_b.iter()) {
                    *bic -= factor * bkc;
                }
            }
        }
    }

    let mut x = vec![vec![0.0; cols]; m];
    for i in (0..m).rev() {
        let diag = a[i][i];
        for c in 0..cols {
            let mut sum = b[i][c];
            for j in (i + 1)..m {
                sum -= a[i][j] * x[j][c];
            }
            let val = sum / diag;
            if !val.is_finite() {
                return Err(BSplineError::SingularInterpolationSystem);
            }
            x[i][c] = val;
        }
    }
    Ok(x)
}

/// Global B-spline curve interpolation through `points` using supplied parameters
/// and clamped knot vector.
///
/// Implements the linear collocation solve of Piegl & Tiller **Algorithm A9.1**
/// (`GlobalCurveInterp`, `refs/text/piegl_tiller1997_nurbs_book.txt:15961-15984`).
pub fn global_curve_interp_with_knots(
    points: &[Point3],
    degree: usize,
    params: &[f64],
    knots: &[f64],
) -> Result<BSplineCurve, BSplineError> {
    if degree == 0 {
        return Err(BSplineError::ZeroDegree);
    }
    if points.len() < degree + 1 {
        return Err(BSplineError::InterpolationTooFewPoints {
            count: points.len(),
            min_required: degree + 1,
        });
    }
    if params.len() != points.len() {
        return Err(BSplineError::ParameterCountMismatch {
            expected: points.len(),
            actual: params.len(),
        });
    }
    for (idx, &pt) in points.iter().enumerate() {
        if !is_finite_point(pt) {
            return Err(BSplineError::NonFiniteControlPoint { index: idx });
        }
    }
    validate_clamped_knots(degree, points.len(), knots)?;

    let n = points.len() - 1;
    let u_min = knots[degree];
    let u_max = knots[n + 1];
    if params[0] != u_min {
        return Err(BSplineError::InvalidParameterSequence { index: 0 });
    }
    if params[n] != u_max {
        return Err(BSplineError::InvalidParameterSequence { index: n });
    }
    for idx in 0..n {
        if !params[idx + 1].is_finite() || params[idx + 1] <= params[idx] {
            return Err(BSplineError::InvalidParameterSequence { index: idx + 1 });
        }
    }

    let dim = n + 1;
    let mut mat = vec![vec![0.0; dim]; dim];
    let mut rhs = Vec::with_capacity(dim);
    for i in 0..=n {
        let span = find_span(n, degree, params[i], knots)?;
        let basis = basis_funs(span, params[i], degree, knots)?;
        mat[i][(span - degree)..=span].copy_from_slice(&basis);
        let q = points[i].as_array();
        rhs.push(vec![q[0], q[1], q[2]]);
    }

    let sol = solve_linear_system(mat, rhs)?;
    let ctrl: Vec<Point3> = sol
        .into_iter()
        .map(|row| Point3::new(row[0], row[1], row[2]))
        .collect();
    BSplineCurve::new(degree, knots.to_vec(), ctrl, None)
}

/// Piegl & Tiller **Algorithm A9.1** (`GlobalCurveInterp`,
/// `refs/text/piegl_tiller1997_nurbs_book.txt:15961-15984`).
///
/// Computes an open clamped degree-`degree` B-spline curve on `[0, 1]` passing
/// through `points[0..=n]` using chord-length parameterization (Eq. 9.5,
/// `:15699-15707`) and knot averaging (Eq. 9.8, `:15726-15730`).
///
/// Refuses duplicate consecutive points or singular systems with a typed
/// [`BSplineError`] — no automatic centripetal retry or silent fallback
/// (`specs/loft.md` §6 user decisions).
pub fn global_curve_interp(points: &[Point3], degree: usize) -> Result<BSplineCurve, BSplineError> {
    if degree == 0 {
        return Err(BSplineError::ZeroDegree);
    }
    if points.len() < degree + 1 {
        return Err(BSplineError::InterpolationTooFewPoints {
            count: points.len(),
            min_required: degree + 1,
        });
    }
    let params = chord_length_params(points)?;
    let knots = averaged_knots(&params, degree)?;
    global_curve_interp_with_knots(points, degree, &params, &knots)
}

/// Periodic chord-length parameters `0 = \bar{u}_0 < \bar{u}_1 < ... < \bar{u}_m = 1`
/// for a closed ring of `m >= 3` distinct points (`Q_m := Q_0`).
pub fn periodic_chord_length_params(points: &[Point3]) -> Result<Vec<f64>, BSplineError> {
    let m = points.len();
    if m < 3 {
        return Err(BSplineError::InterpolationTooFewPoints {
            count: m,
            min_required: 3,
        });
    }
    for (idx, &pt) in points.iter().enumerate() {
        if !is_finite_point(pt) {
            return Err(BSplineError::NonFiniteControlPoint { index: idx });
        }
    }
    let mut chords = Vec::with_capacity(m);
    let mut total = 0.0f64;
    for k in 0..m {
        let a = points[k].as_array();
        let b = points[(k + 1) % m].as_array();
        let dx = b[0] - a[0];
        let dy = b[1] - a[1];
        let dz = b[2] - a[2];
        let len = (dx * dx + dy * dy + dz * dz).sqrt();
        if !len.is_finite() || len == 0.0 {
            return Err(BSplineError::ZeroChordLength { index: k });
        }
        chords.push(len);
        total += len;
    }
    if !total.is_finite() || total <= 0.0 {
        return Err(BSplineError::ZeroChordLength { index: 0 });
    }

    let mut params = vec![0.0; m + 1];
    let mut accum = 0.0f64;
    for k in 1..m {
        accum += chords[k - 1];
        let val = accum / total;
        if val <= params[k - 1] || val >= 1.0 {
            return Err(BSplineError::InvalidParameterSequence { index: k });
        }
        params[k] = val;
    }
    params[m] = 1.0;
    Ok(params)
}

/// Insert a single knot `u` into an unclamped B-spline curve via Boehm's rule
/// (Piegl & Tiller Eq. 5.10–5.11 & §12.2, `refs/text/piegl_tiller1997_nurbs_book.txt:6363-6380, 24614-24619`).
fn insert_knot_unclamped(
    p: usize,
    knots: &[f64],
    ctrl: &[Point3],
    u: f64,
) -> Result<(Vec<f64>, Vec<Point3>), BSplineError> {
    let n = ctrl.len() - 1;
    // Locate `k` in `p..=n` with `knots[k] <= u < knots[k + 1]`.
    let k = find_span(n, p, u, knots)?;
    let mut new_knots = Vec::with_capacity(knots.len() + 1);
    new_knots.extend_from_slice(&knots[..=k]);
    new_knots.push(u);
    new_knots.extend_from_slice(&knots[(k + 1)..]);

    let mut new_ctrl = Vec::with_capacity(ctrl.len() + 1);
    new_ctrl.extend_from_slice(&ctrl[..(k - p + 1)]);
    for i in (k - p + 1)..=k {
        let denom = knots[i + p] - knots[i];
        if denom <= 0.0 || !denom.is_finite() {
            return Err(BSplineError::ZeroKnotSpan { span: i });
        }
        let alpha = (u - knots[i]) / denom;
        let om = 1.0 - alpha;
        let a = ctrl[i].as_array();
        let b = ctrl[i - 1].as_array();
        new_ctrl.push(Point3::new(
            alpha * a[0] + om * b[0],
            alpha * a[1] + om * b[1],
            alpha * a[2] + om * b[2],
        ));
    }
    new_ctrl.extend_from_slice(&ctrl[k..]);
    Ok((new_knots, new_ctrl))
}

/// Periodic closed global B-spline curve interpolation with caller-supplied
/// periodic parameters `0 = params[0] < params[1] < ... < params[m] = 1`.
///
/// Constructs the `C^{degree - 1}` periodic B-spline interpolant through the `m`
/// distinct points `points[0..m]` (`m >= max(3, degree + 1)`) using cyclic
/// unclamped knots (Piegl & Tiller **Algorithm A9.1** periodic variant & §12.2,
/// `refs/text/piegl_tiller1997_nurbs_book.txt:15961-15984, 24614-24622`;
/// `specs/loft.md` §2.2 `:130`), then clamps the fundamental period `[0, 1]`
/// exactly via Boehm knot insertion so the returned [`BSplineCurve`] is a
/// standard clamped curve on `[0, 1]` satisfying `C(params[k]) = points[k]`,
/// `C(0) = C(1) = points[0]`, and `C^{(r)}(0) = C^{(r)}(1)` for `1 <= r < degree`.
pub fn global_curve_interp_periodic_with_params(
    points: &[Point3],
    degree: usize,
    params: &[f64],
) -> Result<BSplineCurve, BSplineError> {
    if degree == 0 {
        return Err(BSplineError::ZeroDegree);
    }
    let m = points.len();
    let min_required = (degree + 1).max(3);
    if m < min_required {
        return Err(BSplineError::InterpolationTooFewPoints {
            count: m,
            min_required,
        });
    }
    if params.len() != m + 1 {
        return Err(BSplineError::ParameterCountMismatch {
            expected: m + 1,
            actual: params.len(),
        });
    }
    for (idx, &pt) in points.iter().enumerate() {
        if !is_finite_point(pt) {
            return Err(BSplineError::NonFiniteControlPoint { index: idx });
        }
    }
    if params[0] != 0.0 {
        return Err(BSplineError::InvalidParameterSequence { index: 0 });
    }
    if params[m] != 1.0 {
        return Err(BSplineError::InvalidParameterSequence { index: m });
    }
    for idx in 0..m {
        if !params[idx + 1].is_finite() || params[idx + 1] <= params[idx] {
            return Err(BSplineError::InvalidParameterSequence { index: idx + 1 });
        }
    }

    // Base periodic knots in `[0, 1)`:
    // - Odd degree (`p = 1, 3, ...`): knots at data parameters `params[0..m]`
    //   place each collocation site at the central knot of its `(p + 1)`-span support.
    // - Even degree (`p = 2, 4, ...`): midpoint knots `0.5 * (params[k] + params[k + 1])`
    //   place each collocation site strictly inside a span, avoiding the classical
    //   even-`m` circulant singularity when `p` is even.
    let base_knots: Vec<f64> = if degree % 2 == 1 {
        params[..m].to_vec()
    } else {
        (0..m).map(|k| 0.5 * (params[k] + params[k + 1])).collect()
    };

    // Unclamped wrapped knot vector `T` with `p + 1` knots from period `-1`,
    // `m` knots from period `0`, and `p + 2` knots from period `+1`, ensuring
    // `[0, 1]` lies strictly inside `(T[p], T[n_unclamped + 1])`.
    let mut t_knots = Vec::with_capacity(m + 2 * degree + 3);
    for &bk in &base_knots[(m - degree - 1)..m] {
        t_knots.push(bk - 1.0);
    }
    t_knots.extend_from_slice(&base_knots);
    for idx in 0..(degree + 2) {
        t_knots.push(base_knots[idx % m] + ((idx / m) as f64) + 1.0);
    }
    let n_unclamped = t_knots.len() - degree - 2; // == m + degree + 1

    let mut mat = vec![vec![0.0; m]; m];
    let mut rhs = Vec::with_capacity(m);
    for k in 0..m {
        let span = find_span(n_unclamped, degree, params[k], &t_knots)?;
        let basis = basis_funs(span, params[k], degree, &t_knots)?;
        for (r, &br) in basis.iter().enumerate() {
            let col = (span - degree + r) % m;
            mat[k][col] += br;
        }
        let q = points[k].as_array();
        rhs.push(vec![q[0], q[1], q[2]]);
    }

    let sol = solve_linear_system(mat, rhs)?;
    let periodic_ctrl: Vec<Point3> = sol
        .into_iter()
        .map(|row| Point3::new(row[0], row[1], row[2]))
        .collect();

    let mut unclamped_ctrl: Vec<Point3> = (0..=n_unclamped).map(|i| periodic_ctrl[i % m]).collect();

    // Clamp `[0.0, 1.0]` by inserting `0.0` and `1.0` up to multiplicity `degree`
    // (Piegl & Tiller §12.2, `:24614-24622`).
    let mult_start = t_knots.iter().filter(|&&u| u == 0.0).count();
    for _ in mult_start..degree {
        let (next_t, next_c) = insert_knot_unclamped(degree, &t_knots, &unclamped_ctrl, 0.0)?;
        t_knots = next_t;
        unclamped_ctrl = next_c;
    }
    let mult_end = t_knots.iter().filter(|&&u| u == 1.0).count();
    for _ in mult_end..degree {
        let (next_t, next_c) = insert_knot_unclamped(degree, &t_knots, &unclamped_ctrl, 1.0)?;
        t_knots = next_t;
        unclamped_ctrl = next_c;
    }

    let idx_a = t_knots
        .iter()
        .rposition(|&u| u == 0.0)
        .ok_or(BSplineError::SingularInterpolationSystem)?;
    let idx_b = t_knots
        .iter()
        .position(|&u| u == 1.0)
        .ok_or(BSplineError::SingularInterpolationSystem)?;

    let clamped_ctrl = unclamped_ctrl[(idx_a - degree)..idx_b].to_vec();
    let mut clamped_knots = Vec::with_capacity(clamped_ctrl.len() + degree + 1);
    clamped_knots.push(0.0);
    clamped_knots.extend_from_slice(&t_knots[(idx_a - degree + 1)..(idx_b + degree)]);
    clamped_knots.push(1.0);

    BSplineCurve::new(degree, clamped_knots, clamped_ctrl, None)
}

/// Periodic closed global B-spline curve interpolation through `points` on `[0, 1]`
/// using periodic chord-length parameterization (`specs/loft.md` §2.2 `:130` & §4.1 `:286`).
///
/// Requires `points.len() >= max(3, degree + 1)` distinct consecutive points
/// (including `points[last] != points[0]`, as the loop closes back to `points[0]`
/// automatically).
pub fn global_curve_interp_periodic(
    points: &[Point3],
    degree: usize,
) -> Result<BSplineCurve, BSplineError> {
    if degree == 0 {
        return Err(BSplineError::ZeroDegree);
    }
    let min_required = (degree + 1).max(3);
    if points.len() < min_required {
        return Err(BSplineError::InterpolationTooFewPoints {
            count: points.len(),
            min_required,
        });
    }
    let params = periodic_chord_length_params(points)?;
    global_curve_interp_periodic_with_params(points, degree, &params)
}
