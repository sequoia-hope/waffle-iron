/**
 * B-spline evaluation, sampling, and global interpolation for 2D sketch splines.
 *
 * Implements Piegl & Tiller, *The NURBS Book* (2nd ed., 1997):
 * - Algorithm A2.1 (`FindSpan`, `refs/text/piegl_tiller1997_nurbs_book.txt:3240-3255`)
 * - Algorithm A2.2 (`BasisFuns`, `:3318-3337`)
 * - Algorithm A3.1 (`CurvePoint`, `:3800-3810`)
 * - Algorithm A9.1 (`GlobalCurveInterp`, `:15961-15984`, with chord-length
 *   parameters Eqs. 9.4–9.5 `:15690-15707` and averaged knots Eq. 9.8 `:15726-15730`)
 *
 * Mirrors `crates/nurbs-rs/src/bspline.rs` (`nurbs_rs::bspline`) and
 * `crates/waffle-types/src/regions.rs` (`sample_sketch_spline`) with exact
 * endpoint evaluation (no `1 - 1e-10` or `1e-10` clamps), no silent degree
 * clamping inside library routines, and loud `Error` failures on invalid or
 * singular inputs.
 */

const PIVOT_FLOOR = 1e-14;

/**
 * Validate an array of 2D points `{ x, y }`.
 *
 * @param {Array<{x: number, y: number}>} points
 * @param {string} label
 */
function validatePoints2D(points, label) {
	if (!Array.isArray(points)) {
		throw new Error(`${label} must be an array of {x, y} points`);
	}
	for (let i = 0; i < points.length; i++) {
		const pt = points[i];
		if (!pt || !Number.isFinite(pt.x) || !Number.isFinite(pt.y)) {
			throw new Error(`${label}[${i}] has non-finite coordinates`);
		}
	}
}

/**
 * Validate a clamped non-decreasing knot vector of length `numCtrl + degree + 1`.
 *
 * @param {number} degree
 * @param {number} numCtrl
 * @param {number[]} knots
 */
function validateClampedKnots(degree, numCtrl, knots) {
	const expected = numCtrl + degree + 1;
	if (!Array.isArray(knots) || knots.length !== expected) {
		throw new Error(
			`knot vector length mismatch: expected ${expected} for ${numCtrl} control points of degree ${degree}, got ${knots?.length}`
		);
	}
	for (let i = 0; i < knots.length; i++) {
		if (!Number.isFinite(knots[i])) {
			throw new Error(`knot[${i}] is not finite (${knots[i]})`);
		}
	}
	for (let i = 0; i < knots.length - 1; i++) {
		if (knots[i] > knots[i + 1]) {
			throw new Error(`knot vector is not non-decreasing at index ${i}: ${knots[i]} > ${knots[i + 1]}`);
		}
	}
	const uMin = knots[degree];
	const uMax = knots[numCtrl];
	if (!(uMax > uMin)) {
		throw new Error(`degenerate knot domain [${uMin}, ${uMax}]`);
	}
	for (let i = 0; i <= degree; i++) {
		if (knots[i] !== uMin) {
			throw new Error(`knot vector is not clamped at start (index ${i}: ${knots[i]} !== ${uMin})`);
		}
		const endIdx = expected - 1 - i;
		if (knots[endIdx] !== uMax) {
			throw new Error(`knot vector is not clamped at end (index ${endIdx}: ${knots[endIdx]} !== ${uMax})`);
		}
	}
}

/**
 * Generate a uniform clamped knot vector on `[0, 1]` for `n` control points
 * and polynomial `degree`.
 *
 * Total knots = `n + degree + 1`. First `degree + 1` knots are `0`, last
 * `degree + 1` knots are `1`, interior knots are evenly spaced on `(0, 1)`.
 *
 * @param {number} n - Number of control points (`n >= degree + 1`)
 * @param {number} degree - Polynomial degree (`degree >= 1`)
 * @returns {number[]} Clamped knot vector of length `n + degree + 1`
 */
export function clampedKnotVector(n, degree) {
	if (!Number.isInteger(degree) || degree < 1) {
		throw new Error(`B-spline degree must be an integer >= 1 (got ${degree})`);
	}
	if (!Number.isInteger(n) || n < degree + 1) {
		throw new Error(
			`B-spline of degree ${degree} requires at least ${degree + 1} control points (got ${n})`
		);
	}
	const totalKnots = n + degree + 1;
	const knots = new Array(totalKnots);
	for (let i = 0; i <= degree; i++) {
		knots[i] = 0;
	}
	const spans = n - degree;
	for (let i = degree + 1; i < n; i++) {
		knots[i] = (i - degree) / spans;
	}
	for (let i = n; i < totalKnots; i++) {
		knots[i] = 1;
	}
	return knots;
}

/**
 * Piegl & Tiller Algorithm A2.1 (`FindSpan`, `:3240-3255`).
 *
 * Returns knot span index `span` in `[p, n]` for `n + 1` control points (`0..=n`),
 * degree `p`, and parameter `u` in `[knots[p], knots[n + 1]]`.
 * Maps the right endpoint `u === knots[n + 1]` to the last non-empty span (`n`
 * for standard clamped knots).
 *
 * @param {number} n - Highest control point index (`numCtrl - 1`)
 * @param {number} p - Degree (`>= 1`)
 * @param {number} u - Parameter value
 * @param {number[]} knots - Knot vector of length `n + p + 2`
 * @returns {number}
 */
export function findSpan(n, p, u, knots) {
	const uMin = knots[p];
	const uMax = knots[n + 1];
	if (!Number.isFinite(u) || u < uMin || u > uMax) {
		throw new Error(`B-spline parameter ${u} is outside domain [${uMin}, ${uMax}]`);
	}
	if (u === uMax) {
		let span = n;
		while (span > p && knots[span] === uMax) {
			span -= 1;
		}
		return span;
	}
	let low = p;
	let high = n + 1;
	let mid = Math.trunc((low + high) / 2);
	while (u < knots[mid] || u >= knots[mid + 1]) {
		if (u < knots[mid]) {
			high = mid;
		} else {
			low = mid;
		}
		const nextMid = Math.trunc((low + high) / 2);
		if (nextMid === mid) {
			throw new Error(`zero or non-monotonic knot span at index ${mid}`);
		}
		mid = nextMid;
	}
	return mid;
}

/**
 * Piegl & Tiller Algorithm A2.2 (`BasisFuns`, `:3318-3337`).
 *
 * Computes the `p + 1` non-vanishing basis functions
 * `N[0..p] = [N_{span-p, p}(u), ..., N_{span, p}(u)]`.
 *
 * @param {number} span - Knot span index from `findSpan`
 * @param {number} u - Parameter value
 * @param {number} p - Degree
 * @param {number[]} knots - Knot vector
 * @returns {number[]}
 */
export function basisFuns(span, u, p, knots) {
	if (knots[span + 1] <= knots[span]) {
		throw new Error(`zero knot span at index ${span}`);
	}
	const nVals = new Array(p + 1).fill(0);
	const left = new Array(p + 1).fill(0);
	const right = new Array(p + 1).fill(0);
	nVals[0] = 1.0;
	for (let j = 1; j <= p; j++) {
		left[j] = u - knots[span + 1 - j];
		right[j] = knots[span + j] - u;
		let saved = 0.0;
		for (let r = 0; r < j; r++) {
			const denom = right[r + 1] + left[j - r];
			if (!(denom > 0.0) || !Number.isFinite(denom)) {
				throw new Error(`zero knot span denominator at span ${span}`);
			}
			const nr = nVals[r];
			nVals[r] = saved + (nr * right[r + 1]) / denom;
			saved = (nr * left[j - r]) / denom;
		}
		nVals[j] = saved;
	}
	return nVals;
}

/**
 * Evaluate a 2D B-spline curve at parameter `t` via Piegl & Tiller Algorithm A3.1
 * (`CurvePoint`, `:3800-3810`).
 *
 * Does NOT clamp `t` away from `0` or `1`, and does NOT silently reduce `degree`
 * when `controlPoints.length < degree + 1` — callers pass the explicit degree cap.
 *
 * @param {Array<{x: number, y: number}>} controlPoints - Control points (`length >= degree + 1`)
 * @param {number} t - Parameter inside `[u_min, u_max]` (`[0, 1]` for default knots)
 * @param {number} [degree=3] - Polynomial degree (`>= 1`)
 * @param {number[]} [knots] - Optional clamped knot vector (uniform clamped on `[0, 1]` if omitted)
 * @returns {{x: number, y: number}}
 */
export function evaluateBSpline(controlPoints, t, degree = 3, knots) {
	if (!Number.isInteger(degree) || degree < 1) {
		throw new Error(`B-spline degree must be an integer >= 1 (got ${degree})`);
	}
	validatePoints2D(controlPoints, 'controlPoints');
	const numCtrl = controlPoints.length;
	if (numCtrl < degree + 1) {
		throw new Error(
			`B-spline of degree ${degree} requires at least ${degree + 1} control points (got ${numCtrl})`
		);
	}
	const kv = knots !== undefined ? knots : clampedKnotVector(numCtrl, degree);
	if (knots !== undefined) {
		validateClampedKnots(degree, numCtrl, kv);
	}

	const n = numCtrl - 1;
	const span = findSpan(n, degree, t, kv);
	const basis = basisFuns(span, t, degree, kv);
	let x = 0.0;
	let y = 0.0;
	for (let j = 0; j <= degree; j++) {
		const nj = basis[j];
		const pt = controlPoints[span - degree + j];
		x += nj * pt.x;
		y += nj * pt.y;
	}
	return { x, y };
}

/**
 * Sample an open clamped 2D B-spline curve into `numSamples + 1` points on `[0, 1]`.
 *
 * Does NOT silently clamp `degree` — callers pass the explicit degree cap
 * (e.g. `Math.min(3, ctrlPts.length - 1)`).
 *
 * @param {Array<{x: number, y: number}>} controlPoints - Control points (`length >= degree + 1`)
 * @param {number} [numSamples=32] - Number of uniform spans (`>= 1`)
 * @param {number} [degree=3] - Polynomial degree (`>= 1`)
 * @returns {Array<{x: number, y: number}>} `numSamples + 1` points on the curve
 */
export function sampleBSpline(controlPoints, numSamples = 32, degree = 3) {
	if (!Number.isInteger(numSamples) || numSamples < 1) {
		throw new Error(`numSamples must be an integer >= 1 (got ${numSamples})`);
	}
	if (!Number.isInteger(degree) || degree < 1) {
		throw new Error(`B-spline degree must be an integer >= 1 (got ${degree})`);
	}
	validatePoints2D(controlPoints, 'controlPoints');
	const numCtrl = controlPoints.length;
	if (numCtrl < degree + 1) {
		throw new Error(
			`B-spline of degree ${degree} requires at least ${degree + 1} control points (got ${numCtrl})`
		);
	}

	const knots = clampedKnotVector(numCtrl, degree);
	const n = numCtrl - 1;
	const points = new Array(numSamples + 1);
	for (let i = 0; i <= numSamples; i++) {
		const t = i / numSamples;
		const span = findSpan(n, degree, t, knots);
		const basis = basisFuns(span, t, degree, knots);
		let x = 0.0;
		let y = 0.0;
		for (let j = 0; j <= degree; j++) {
			const nj = basis[j];
			const pt = controlPoints[span - degree + j];
			x += nj * pt.x;
			y += nj * pt.y;
		}
		points[i] = { x, y };
	}
	return points;
}

/**
 * Solve `A * X = B` for `m x m` matrix `A` and `m x 2` right-hand side `B` via
 * Gaussian elimination with partial pivoting (mirrors `nurbs_rs::bspline::solve_linear_system`).
 *
 * @param {number[][]} a
 * @param {number[][]} b
 * @returns {number[][]}
 */
function solveLinearSystem2D(a, b) {
	const m = a.length;
	for (let k = 0; k < m; k++) {
		let pivotRow = k;
		let maxAbs = Math.abs(a[k][k]);
		for (let i = k + 1; i < m; i++) {
			const cand = Math.abs(a[i][k]);
			if (cand > maxAbs) {
				maxAbs = cand;
				pivotRow = i;
			}
		}
		if (!Number.isFinite(maxAbs) || maxAbs <= PIVOT_FLOOR) {
			throw new Error('singular B-spline interpolation system');
		}
		if (pivotRow !== k) {
			[a[k], a[pivotRow]] = [a[pivotRow], a[k]];
			[b[k], b[pivotRow]] = [b[pivotRow], b[k]];
		}
		const pivA = a[k];
		const pivB = b[k];
		const piv = pivA[k];
		for (let i = k + 1; i < m; i++) {
			const rowA = a[i];
			const rowB = b[i];
			const factor = rowA[k] / piv;
			if (factor !== 0.0) {
				rowA[k] = 0.0;
				for (let j = k + 1; j < m; j++) {
					rowA[j] -= factor * pivA[j];
				}
				rowB[0] -= factor * pivB[0];
				rowB[1] -= factor * pivB[1];
			}
		}
	}

	const x = Array.from({ length: m }, () => [0.0, 0.0]);
	for (let i = m - 1; i >= 0; i--) {
		const diag = a[i][i];
		for (let c = 0; c < 2; c++) {
			let sum = b[i][c];
			for (let j = i + 1; j < m; j++) {
				sum -= a[i][j] * x[j][c];
			}
			const val = sum / diag;
			if (!Number.isFinite(val)) {
				throw new Error('singular B-spline interpolation system');
			}
			x[i][c] = val;
		}
	}
	return x;
}

/**
 * Fit an open clamped B-spline curve of `degree` through `points` via Piegl &
 * Tiller Algorithm A9.1 (`GlobalCurveInterp`, `:15961-15984`).
 *
 * Uses chord-length parameterization (Eqs. 9.4–9.5, `:15690-15707`) and knot
 * averaging (Eq. 9.8, `:15726-15730`). Solves `points.length === degree + 1`
 * cleanly and throws an `Error` if `points.length < degree + 1`, consecutive
 * points coincide, or the collocation system is singular — never falls back to
 * returning `points`.
 *
 * @param {Array<{x: number, y: number}>} points - Data points to interpolate (`length >= degree + 1`)
 * @param {number} [degree=3] - Polynomial degree (`>= 1`)
 * @returns {Array<{x: number, y: number}>} Fitted control points (`length === points.length`)
 */
export function fitBSplineToPoints(points, degree = 3) {
	if (!Number.isInteger(degree) || degree < 1) {
		throw new Error(`B-spline degree must be an integer >= 1 (got ${degree})`);
	}
	validatePoints2D(points, 'points');
	if (points.length < degree + 1) {
		throw new Error(
			`B-spline interpolation of degree ${degree} requires at least ${degree + 1} data points (got ${points.length})`
		);
	}

	const n = points.length - 1;
	const chords = new Array(n);
	let total = 0.0;
	for (let k = 0; k < n; k++) {
		const dx = points[k + 1].x - points[k].x;
		const dy = points[k + 1].y - points[k].y;
		const len = Math.sqrt(dx * dx + dy * dy);
		if (!Number.isFinite(len) || len === 0.0) {
			throw new Error(`zero chord length between interpolation points ${k} and ${k + 1}`);
		}
		chords[k] = len;
		total += len;
	}
	if (!Number.isFinite(total) || total <= 0.0) {
		throw new Error('zero total chord length in B-spline interpolation');
	}

	const uk = new Array(n + 1).fill(0.0);
	let accum = 0.0;
	for (let k = 1; k < n; k++) {
		accum += chords[k - 1];
		const val = accum / total;
		if (val <= uk[k - 1] || val >= 1.0) {
			throw new Error(`invalid chord-length parameter sequence at index ${k}`);
		}
		uk[k] = val;
	}
	uk[n] = 1.0;

	// Knot averaging (Piegl & Tiller Eq. 9.8).
	const m = n + degree + 1;
	const knots = new Array(m + 1).fill(0.0);
	for (let j = 1; j <= n - degree; j++) {
		let sum = 0.0;
		for (let i = j; i < j + degree; i++) {
			sum += uk[i];
		}
		knots[j + degree] = sum / degree;
	}
	for (let i = m - degree; i <= m; i++) {
		knots[i] = 1.0;
	}

	// Collocation matrix A[i][j] = N_{j, degree}(uk[i]) via exact FindSpan + BasisFuns.
	const dim = n + 1;
	const mat = Array.from({ length: dim }, () => new Array(dim).fill(0.0));
	const rhs = new Array(dim);
	for (let i = 0; i <= n; i++) {
		const span = findSpan(n, degree, uk[i], knots);
		const basis = basisFuns(span, uk[i], degree, knots);
		for (let j = 0; j <= degree; j++) {
			mat[i][span - degree + j] = basis[j];
		}
		rhs[i] = [points[i].x, points[i].y];
	}

	const sol = solveLinearSystem2D(mat, rhs);
	return sol.map((row) => ({ x: row[0], y: row[1] }));
}
