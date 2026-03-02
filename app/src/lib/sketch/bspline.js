/**
 * B-spline math utilities for sketch spline entities.
 *
 * Pure math — no framework dependencies.
 */

/**
 * Generate a clamped (open) knot vector for a B-spline.
 * @param {number} n - Number of control points
 * @param {number} degree - Spline degree
 * @returns {number[]} Knot vector of length n + degree + 1
 */
export function clampedKnotVector(n, degree) {
	const m = n + degree + 1;
	const knots = new Array(m);
	for (let i = 0; i < m; i++) {
		if (i <= degree) {
			knots[i] = 0;
		} else if (i >= m - degree - 1) {
			knots[i] = 1;
		} else {
			knots[i] = (i - degree) / (n - degree);
		}
	}
	return knots;
}

/**
 * Evaluate a B-spline at parameter t using De Boor's algorithm.
 * @param {Array<{x: number, y: number}>} controlPoints
 * @param {number} t - Parameter in [0, 1]
 * @param {number} [degree=3] - Spline degree
 * @param {number[]} [knots] - Knot vector (generated if not provided)
 * @returns {{x: number, y: number}}
 */
export function evaluateBSpline(controlPoints, t, degree = 3, knots) {
	const n = controlPoints.length;
	if (n === 0) return { x: 0, y: 0 };
	if (n === 1) return { ...controlPoints[0] };

	// Clamp degree to at most n-1
	const p = Math.min(degree, n - 1);
	if (!knots) {
		knots = clampedKnotVector(n, p);
	}

	// Clamp t to valid range
	t = Math.max(0, Math.min(1, t));
	// Handle t=1 edge case
	if (t >= 1) t = 1 - 1e-10;

	// Find knot span k such that knots[k] <= t < knots[k+1]
	let k = p;
	for (let i = p; i < n; i++) {
		if (t >= knots[i] && t < knots[i + 1]) {
			k = i;
			break;
		}
	}

	// De Boor's algorithm
	const d = [];
	for (let i = 0; i <= p; i++) {
		const idx = k - p + i;
		if (idx >= 0 && idx < n) {
			d.push({ x: controlPoints[idx].x, y: controlPoints[idx].y });
		} else {
			d.push({ x: 0, y: 0 });
		}
	}

	for (let r = 1; r <= p; r++) {
		for (let j = p; j >= r; j--) {
			const i = j + k - p;
			const denom = knots[i + p - r + 1] - knots[i];
			const alpha = denom < 1e-14 ? 0 : (t - knots[i]) / denom;
			d[j] = {
				x: (1 - alpha) * d[j - 1].x + alpha * d[j].x,
				y: (1 - alpha) * d[j - 1].y + alpha * d[j].y
			};
		}
	}

	return d[p];
}

/**
 * Sample a B-spline curve at evenly spaced parameters.
 * @param {Array<{x: number, y: number}>} controlPoints
 * @param {number} [numSamples=32] - Number of sample points
 * @param {number} [degree=3] - Spline degree
 * @returns {Array<{x: number, y: number}>}
 */
export function sampleBSpline(controlPoints, numSamples = 32, degree = 3) {
	if (controlPoints.length === 0) return [];
	if (controlPoints.length === 1) return [{ ...controlPoints[0] }];

	const p = Math.min(degree, controlPoints.length - 1);
	const knots = clampedKnotVector(controlPoints.length, p);
	const points = [];

	for (let i = 0; i <= numSamples; i++) {
		const t = i / numSamples;
		points.push(evaluateBSpline(controlPoints, t, p, knots));
	}
	return points;
}

/**
 * Fit a cubic B-spline that interpolates through the given points.
 *
 * Uses chord-length parameterization and solves the tridiagonal system
 * for the control points of a clamped cubic B-spline.
 *
 * @param {Array<{x: number, y: number}>} points - Points to interpolate
 * @param {number} [degree=3] - Spline degree
 * @returns {Array<{x: number, y: number}>} Control points for the B-spline
 */
export function fitBSplineToPoints(points, degree = 3) {
	const n = points.length;
	if (n <= 2) return points.map(p => ({ ...p }));

	// For degree >= n-1, just return the points as control points
	if (degree >= n - 1) return points.map(p => ({ ...p }));

	// Chord-length parameterization
	const chords = [0];
	let totalLength = 0;
	for (let i = 1; i < n; i++) {
		const dx = points[i].x - points[i - 1].x;
		const dy = points[i].y - points[i - 1].y;
		totalLength += Math.sqrt(dx * dx + dy * dy);
		chords.push(totalLength);
	}
	if (totalLength < 1e-14) return points.map(p => ({ ...p }));

	const params = chords.map(c => c / totalLength);

	// Generate knot vector using averaging method
	const numCtrl = n; // same number of control points as data points
	const p = Math.min(degree, n - 1);
	const m = numCtrl + p + 1;
	const knots = new Array(m);

	// Clamped ends
	for (let i = 0; i <= p; i++) knots[i] = 0;
	for (let i = m - p - 1; i < m; i++) knots[i] = 1;

	// Interior knots by averaging
	for (let j = 1; j < numCtrl - p; j++) {
		let sum = 0;
		for (let i = j; i < j + p; i++) {
			sum += params[i];
		}
		knots[j + p] = sum / p;
	}

	// Build basis function matrix N[i][j] = N_j,p(params[i])
	const N = [];
	for (let i = 0; i < n; i++) {
		N.push(basisRow(params[i], knots, numCtrl, p));
	}

	// Solve N * P = D for control points P (least squares, but n=numCtrl so exact)
	// Use Gaussian elimination
	const ctrlX = solveLinearSystem(N, points.map(p => p.x));
	const ctrlY = solveLinearSystem(N, points.map(p => p.y));

	if (!ctrlX || !ctrlY) {
		// Fallback: return original points
		return points.map(p => ({ ...p }));
	}

	const result = [];
	for (let i = 0; i < numCtrl; i++) {
		result.push({ x: ctrlX[i], y: ctrlY[i] });
	}
	return result;
}

/**
 * Compute a row of B-spline basis function values at parameter t.
 * @param {number} t
 * @param {number[]} knots
 * @param {number} numCtrl
 * @param {number} degree
 * @returns {number[]}
 */
function basisRow(t, knots, numCtrl, degree) {
	// Clamp t
	if (t <= 0) t = 1e-10;
	if (t >= 1) t = 1 - 1e-10;

	const row = new Array(numCtrl).fill(0);

	// Find knot span
	let k = degree;
	for (let i = degree; i < numCtrl; i++) {
		if (t >= knots[i] && t < knots[i + 1]) {
			k = i;
			break;
		}
	}

	// Cox-de Boor recursion
	const N = new Array(degree + 1).fill(0);
	N[0] = 1;

	for (let d = 1; d <= degree; d++) {
		const saved = new Array(d + 1).fill(0);
		for (let j = 0; j < d; j++) {
			const left = knots[k - d + 1 + j];
			const right = knots[k + 1 + j];
			const denom = right - left;
			if (denom < 1e-14) {
				saved[j + 1] = (saved[j + 1] || 0);
				continue;
			}
			const alpha = (t - left) / denom;
			saved[j + 1] = (saved[j + 1] || 0) + alpha * N[j];
			saved[j] += (1 - alpha) * N[j];
		}
		for (let j = 0; j <= d; j++) N[j] = saved[j];
	}

	for (let j = 0; j <= degree; j++) {
		const idx = k - degree + j;
		if (idx >= 0 && idx < numCtrl) {
			row[idx] = N[j];
		}
	}
	return row;
}

/**
 * Solve a linear system Ax = b using Gaussian elimination with partial pivoting.
 * @param {number[][]} A - Coefficient matrix (will be modified)
 * @param {number[]} b - Right-hand side (will be modified)
 * @returns {number[] | null} Solution vector, or null if singular
 */
function solveLinearSystem(A, b) {
	const n = A.length;
	// Create augmented matrix
	const aug = A.map((row, i) => [...row, b[i]]);

	for (let col = 0; col < n; col++) {
		// Find pivot
		let maxVal = Math.abs(aug[col][col]);
		let maxRow = col;
		for (let row = col + 1; row < n; row++) {
			if (Math.abs(aug[row][col]) > maxVal) {
				maxVal = Math.abs(aug[row][col]);
				maxRow = row;
			}
		}
		if (maxVal < 1e-14) return null;

		// Swap rows
		if (maxRow !== col) {
			[aug[col], aug[maxRow]] = [aug[maxRow], aug[col]];
		}

		// Eliminate below
		for (let row = col + 1; row < n; row++) {
			const factor = aug[row][col] / aug[col][col];
			for (let j = col; j <= n; j++) {
				aug[row][j] -= factor * aug[col][j];
			}
		}
	}

	// Back substitution
	const x = new Array(n);
	for (let i = n - 1; i >= 0; i--) {
		let sum = aug[i][n];
		for (let j = i + 1; j < n; j++) {
			sum -= aug[i][j] * x[j];
		}
		x[i] = sum / aug[i][i];
	}
	return x;
}
