//! B-spline math utilities for sketch spline entities.
//!
//! Port of `app/src/lib/sketch/bspline.js` — pure math, no framework dependencies.

/// Generate a clamped (open) knot vector for a B-spline.
///
/// Returns a knot vector of length `n + degree + 1`.
pub fn clamped_knot_vector(n: usize, degree: usize) -> Vec<f64> {
    let m = n + degree + 1;
    (0..m)
        .map(|i| {
            if i <= degree {
                0.0
            } else if i >= m - degree - 1 {
                1.0
            } else {
                (i - degree) as f64 / (n - degree) as f64
            }
        })
        .collect()
}

/// Evaluate a B-spline at parameter `t` using De Boor's algorithm.
///
/// `ctrl` are control points, `t` is in [0, 1], `degree` is the spline degree.
/// If `knots` is `None`, a clamped knot vector is generated automatically.
pub fn evaluate_bspline(
    ctrl: &[(f64, f64)],
    t: f64,
    degree: usize,
    knots: Option<&[f64]>,
) -> (f64, f64) {
    let n = ctrl.len();
    if n == 0 {
        return (0.0, 0.0);
    }
    if n == 1 {
        return ctrl[0];
    }

    let p = degree.min(n - 1);
    let generated_knots;
    let knots = match knots {
        Some(k) => k,
        None => {
            generated_knots = clamped_knot_vector(n, p);
            &generated_knots
        }
    };

    // Clamp t to valid range
    let mut t = t.clamp(0.0, 1.0);
    if t >= 1.0 {
        t = 1.0 - 1e-10;
    }

    // Find knot span k such that knots[k] <= t < knots[k+1]
    let mut k = p;
    for i in p..n {
        if t >= knots[i] && t < knots[i + 1] {
            k = i;
            break;
        }
    }

    // De Boor's algorithm
    let mut d: Vec<(f64, f64)> = (0..=p)
        .map(|i| {
            let idx = (k as isize - p as isize + i as isize) as usize;
            if idx < n {
                ctrl[idx]
            } else {
                (0.0, 0.0)
            }
        })
        .collect();

    for r in 1..=p {
        for j in (r..=p).rev() {
            let i = j + k - p;
            let denom = knots[i + p - r + 1] - knots[i];
            let alpha = if denom < 1e-14 {
                0.0
            } else {
                (t - knots[i]) / denom
            };
            d[j] = (
                (1.0 - alpha) * d[j - 1].0 + alpha * d[j].0,
                (1.0 - alpha) * d[j - 1].1 + alpha * d[j].1,
            );
        }
    }

    d[p]
}

/// Fit a B-spline that interpolates through the given points.
///
/// Uses chord-length parameterization, averaged interior knots, and Gaussian
/// elimination with partial pivoting. Same thresholds (`1e-14`) as JS.
pub fn fit_bspline_to_points(points: &[(f64, f64)], degree: usize) -> Vec<(f64, f64)> {
    let n = points.len();
    if n <= 2 {
        return points.to_vec();
    }
    if degree >= n - 1 {
        return points.to_vec();
    }

    // Chord-length parameterization
    let mut total_length = 0.0;
    let mut chords = vec![0.0; n];
    for i in 1..n {
        let dx = points[i].0 - points[i - 1].0;
        let dy = points[i].1 - points[i - 1].1;
        total_length += (dx * dx + dy * dy).sqrt();
        chords[i] = total_length;
    }
    if total_length < 1e-14 {
        return points.to_vec();
    }

    let params: Vec<f64> = chords.iter().map(|c| c / total_length).collect();

    // Generate knot vector using averaging method
    let num_ctrl = n;
    let p = degree.min(n - 1);
    let m = num_ctrl + p + 1;
    let mut knots = vec![0.0; m];

    // Clamped ends — first p+1 are 0, last p+1 are 1
    for knot in knots.iter_mut().take(p + 1) {
        *knot = 0.0;
    }
    for knot in knots.iter_mut().skip(m - p - 1) {
        *knot = 1.0;
    }

    // Interior knots by averaging
    for j in 1..(num_ctrl - p) {
        let sum: f64 = params[j..(j + p)].iter().sum();
        knots[j + p] = sum / p as f64;
    }

    // Build basis function matrix N[i][j] = N_j,p(params[i])
    let basis_matrix: Vec<Vec<f64>> = params
        .iter()
        .map(|&t| basis_row(t, &knots, num_ctrl, p))
        .collect();

    // Solve N * P = D for control points
    let rhs_x: Vec<f64> = points.iter().map(|p| p.0).collect();
    let rhs_y: Vec<f64> = points.iter().map(|p| p.1).collect();

    let ctrl_x = solve_linear_system(&basis_matrix, &rhs_x);
    let ctrl_y = solve_linear_system(&basis_matrix, &rhs_y);

    match (ctrl_x, ctrl_y) {
        (Some(cx), Some(cy)) => cx.into_iter().zip(cy).collect(),
        _ => points.to_vec(),
    }
}

/// Compute a row of B-spline basis function values at parameter t.
fn basis_row(t: f64, knots: &[f64], num_ctrl: usize, degree: usize) -> Vec<f64> {
    let mut t = t;
    if t <= 0.0 {
        t = 1e-10;
    }
    if t >= 1.0 {
        t = 1.0 - 1e-10;
    }

    let mut row = vec![0.0; num_ctrl];

    // Find knot span
    let mut k = degree;
    for i in degree..num_ctrl {
        if t >= knots[i] && t < knots[i + 1] {
            k = i;
            break;
        }
    }

    // Cox-de Boor recursion
    let mut basis = vec![0.0; degree + 1];
    basis[0] = 1.0;

    for d in 1..=degree {
        let mut saved = vec![0.0; d + 1];
        for j in 0..d {
            let left = knots[k - d + 1 + j];
            let right = knots[k + 1 + j];
            let denom = right - left;
            if denom < 1e-14 {
                continue;
            }
            let alpha = (t - left) / denom;
            saved[j + 1] += alpha * basis[j];
            saved[j] += (1.0 - alpha) * basis[j];
        }
        basis[..=d].copy_from_slice(&saved);
    }

    for (j, &basis_val) in basis.iter().enumerate().take(degree + 1) {
        let idx = k as isize - degree as isize + j as isize;
        if idx >= 0 && (idx as usize) < num_ctrl {
            row[idx as usize] = basis_val;
        }
    }
    row
}

/// Solve a linear system Ax = b using Gaussian elimination with partial pivoting.
fn solve_linear_system(a: &[Vec<f64>], b: &[f64]) -> Option<Vec<f64>> {
    let n = a.len();
    // Create augmented matrix
    let mut aug: Vec<Vec<f64>> = a
        .iter()
        .enumerate()
        .map(|(i, row)| {
            let mut r = row.clone();
            r.push(b[i]);
            r
        })
        .collect();

    for col in 0..n {
        // Find pivot
        let mut max_val = aug[col][col].abs();
        let mut max_row = col;
        for (row_idx, aug_row) in aug.iter().enumerate().skip(col + 1) {
            let v = aug_row[col].abs();
            if v > max_val {
                max_val = v;
                max_row = row_idx;
            }
        }
        if max_val < 1e-14 {
            return None;
        }

        // Swap rows
        if max_row != col {
            aug.swap(col, max_row);
        }

        // Eliminate below
        for row in (col + 1)..n {
            let factor = aug[row][col] / aug[col][col];
            // Cache pivot row to avoid borrow conflict
            let pivot_row: Vec<f64> = aug[col][col..=n].to_vec();
            for (j_idx, &pv) in pivot_row.iter().enumerate() {
                aug[row][col + j_idx] -= factor * pv;
            }
        }
    }

    // Back substitution
    let mut x = vec![0.0; n];
    for i in (0..n).rev() {
        let mut sum = aug[i][n];
        for j in (i + 1)..n {
            sum -= aug[i][j] * x[j];
        }
        x[i] = sum / aug[i][i];
    }
    Some(x)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn knot_vector_shape() {
        let knots = clamped_knot_vector(5, 3);
        assert_eq!(knots.len(), 9); // 5 + 3 + 1
        assert_eq!(knots[0], 0.0);
        assert_eq!(knots[1], 0.0);
        assert_eq!(knots[2], 0.0);
        assert_eq!(knots[3], 0.0);
        assert_eq!(knots[5], 1.0);
        assert_eq!(knots[6], 1.0);
        assert_eq!(knots[7], 1.0);
        assert_eq!(knots[8], 1.0);
        assert!(knots[4] > 0.0 && knots[4] < 1.0);
    }

    #[test]
    fn endpoint_interpolation() {
        let ctrl = vec![(0.0, 0.0), (1.0, 2.0), (3.0, 1.0), (4.0, 0.0)];
        let start = evaluate_bspline(&ctrl, 0.0, 3, None);
        let end = evaluate_bspline(&ctrl, 1.0, 3, None);
        assert!((start.0).abs() < 1e-10);
        assert!((start.1).abs() < 1e-10);
        assert!((end.0 - 4.0).abs() < 1e-6);
        assert!((end.1).abs() < 1e-6);
    }

    #[test]
    fn round_trip_fit_evaluate() {
        let points = vec![(0.0, 0.0), (1.0, 2.0), (2.0, 1.5), (3.0, 3.0), (4.0, 0.5)];
        let ctrl = fit_bspline_to_points(&points, 3);
        let n = points.len();
        let p = 3usize;

        // Build the same averaged knot vector used during fitting
        let mut chords = vec![0.0; n];
        let mut total = 0.0;
        for i in 1..n {
            let dx = points[i].0 - points[i - 1].0;
            let dy = points[i].1 - points[i - 1].1;
            total += (dx * dx + dy * dy).sqrt();
            chords[i] = total;
        }
        let params: Vec<f64> = chords.iter().map(|c| c / total).collect();

        let m = n + p + 1;
        let mut knots = vec![0.0; m];
        for knot in knots.iter_mut().take(p + 1) {
            *knot = 0.0;
        }
        for knot in knots.iter_mut().skip(m - p - 1) {
            *knot = 1.0;
        }
        for j in 1..(n - p) {
            let sum: f64 = params[j..(j + p)].iter().sum();
            knots[j + p] = sum / p as f64;
        }

        for (i, &param) in params.iter().enumerate() {
            let result = evaluate_bspline(&ctrl, param, 3, Some(&knots));
            assert!(
                (result.0 - points[i].0).abs() < 1e-6,
                "x mismatch at point {i}: {} vs {}",
                result.0,
                points[i].0
            );
            assert!(
                (result.1 - points[i].1).abs() < 1e-6,
                "y mismatch at point {i}: {} vs {}",
                result.1,
                points[i].1
            );
        }
    }

    #[test]
    fn degenerate_two_points() {
        let points = vec![(1.0, 2.0), (3.0, 4.0)];
        let ctrl = fit_bspline_to_points(&points, 3);
        assert_eq!(ctrl.len(), 2);
        assert_eq!(ctrl[0], (1.0, 2.0));
        assert_eq!(ctrl[1], (3.0, 4.0));
    }

    #[test]
    fn degenerate_single_point() {
        let ctrl = fit_bspline_to_points(&[(5.0, 7.0)], 3);
        assert_eq!(ctrl.len(), 1);
        assert_eq!(ctrl[0], (5.0, 7.0));
    }

    #[test]
    fn degenerate_empty() {
        let result = evaluate_bspline(&[], 0.5, 3, None);
        assert_eq!(result, (0.0, 0.0));
    }

    #[test]
    fn single_control_point() {
        let result = evaluate_bspline(&[(3.0, 4.0)], 0.5, 3, None);
        assert_eq!(result, (3.0, 4.0));
    }

    #[test]
    fn fit_collinear_points() {
        let points: Vec<(f64, f64)> = (0..6).map(|i| (i as f64, 2.0 * i as f64)).collect();
        let ctrl = fit_bspline_to_points(&points, 3);
        let mid = evaluate_bspline(&ctrl, 0.5, 3, None);
        assert!(
            (mid.1 - 2.0 * mid.0).abs() < 1e-6,
            "Midpoint should be on y=2x: ({}, {})",
            mid.0,
            mid.1
        );
    }

    #[test]
    fn high_degree_fallback() {
        let points = vec![(0.0, 0.0), (1.0, 1.0), (2.0, 0.0)];
        let ctrl = fit_bspline_to_points(&points, 3);
        assert_eq!(ctrl, points);
    }
}
