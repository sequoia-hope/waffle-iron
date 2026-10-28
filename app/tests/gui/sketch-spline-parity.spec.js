/**
 * GUI-free JS ↔ Rust B-spline parity & exactness suite (`app/src/lib/sketch/bspline.js`
 * vs `crates/waffle-types/src/regions.rs` / `nurbs_rs::bspline`).
 *
 * Imports `@playwright/test` directly (no `waffle` browser fixture), mirroring
 * `agent-executor-pure.spec.js`. Hard-coded expected arrays are paired with
 * `sketch_spline_js_rust_parity_oracle` in `crates/waffle-types/src/regions.rs`.
 */
import { test, expect } from '@playwright/test';
import {
	clampedKnotVector,
	evaluateBSpline,
	sampleBSpline,
	fitBSplineToPoints
} from '../../src/lib/sketch/bspline.js';

const EVAL_TS = [0.0, 0.125, 0.25, 1.0 / 3.0, 0.5, 0.7, 1.0];

/**
 * Paired open control polygons (2, 3, 4, 7 points) and hard-coded Rust oracle
 * outputs from `nurbs_rs::bspline` / `waffle_types::regions::sample_sketch_spline`.
 */
const PARITY_CASES = [
	{
		name: 'open_2pt',
		ctrl: [
			[-1.25, 2.75],
			[4.5, -0.875]
		],
		evalExpected: [
			[-1.25, 2.75],
			[-0.53125, 2.296875],
			[0.1875, 1.84375],
			[0.6666666666666665, 1.541666666666667],
			[1.625, 0.9375],
			[2.775, 0.21250000000000024],
			[4.5, -0.875]
		],
		sampled8Expected: [
			[-1.25, 2.75],
			[-0.53125, 2.296875],
			[0.1875, 1.84375],
			[0.90625, 1.390625],
			[1.625, 0.9375],
			[2.34375, 0.484375],
			[3.0625, 0.03125],
			[3.78125, -0.421875],
			[4.5, -0.875]
		]
	},
	{
		name: 'open_3pt',
		ctrl: [
			[-2.3, 1.1],
			[0.7, 3.9],
			[3.4, -0.6]
		],
		evalExpected: [
			[-2.3, 1.1],
			[-1.5546874999999998, 1.6859375],
			[-0.81875, 2.0437499999999997],
			[-0.3333333333333335, 2.1555555555555554],
			[0.625, 2.075],
			[1.7529999999999997, 1.443],
			[3.4, -0.6]
		],
		sampled8Expected: [
			[-2.3, 1.1],
			[-1.5546874999999998, 1.6859375],
			[-0.81875, 2.0437499999999997],
			[-0.09218749999999992, 2.1734375],
			[0.625, 2.075],
			[1.3328125, 1.7484375],
			[2.03125, 1.19375],
			[2.7203125, 0.41093750000000007],
			[3.4, -0.6]
		]
	},
	{
		name: 'open_4pt',
		ctrl: [
			[0.25, -1.5],
			[1.2, 2.8],
			[3.1, -0.4],
			[4.75, 1.625]
		],
		evalExpected: [
			[0.25, -1.5],
			[0.6484375, -0.21420898437500008],
			[1.121875, 0.5175781249999999],
			[1.4722222222222223, 0.7712962962962964],
			[2.2375, 0.9156249999999998],
			[3.2298999999999998, 0.869675],
			[4.75, 1.625]
		],
		sampled8Expected: [
			[0.25, -1.5],
			[0.6484375, -0.21420898437500008],
			[1.121875, 0.5175781249999999],
			[1.65625, 0.844482421875],
			[2.2375, 0.9156249999999998],
			[2.8515625, 0.880126953125],
			[3.484375, 0.887109375],
			[4.121875, 1.085693359375],
			[4.75, 1.625]
		],
		fitCubicExpected: [
			[0.25, -1.5],
			[0.2456017136603659, 11.331486010701294],
			[2.404982670764962, -6.233210883651551],
			[4.75, 1.625]
		]
	},
	{
		name: 'open_7pt',
		ctrl: [
			[-3.2, -1.1],
			[-2.1, 2.4],
			[-0.6, 3.1],
			[0.9, -1.7],
			[2.3, 0.8],
			[3.8, 2.9],
			[5.125, -0.375]
		],
		evalExpected: [
			[-3.2, -1.1],
			[-1.784375, 2.0593749999999997],
			[-0.725, 2.125],
			[-0.13765432098765437, 1.2561728395061729],
			[0.8833333333333333, -0.4833333333333334],
			[2.0923999999999996, 0.3651999999999995],
			[5.125, -0.375]
		],
		sampled8Expected: [
			[-3.2, -1.1],
			[-1.784375, 2.0593749999999997],
			[-0.725, 2.125],
			[0.1322916666666667, 0.7302083333333333],
			[0.8833333333333333, -0.4833333333333334],
			[1.6156249999999999, -0.284375],
			[2.4416666666666664, 0.9083333333333332],
			[3.5145833333333334, 1.8479166666666669],
			[5.125, -0.375]
		],
		fitCubicExpected: [
			[-3.2, -1.1],
			[-4.729716779950341, 0.31198347058694076],
			[1.0019898594184158, 6.67627985142584],
			[0.3528458094435299, -3.6639248742658324],
			[2.6878425088990032, 2.1412302960432226],
			[4.823206532897456, 4.7653700320251176],
			[5.125, -0.375]
		]
	}
];

test.describe('sketch spline JS ↔ Rust parity and Piegl & Tiller exactness', () => {
	for (const c of PARITY_CASES) {
		test(`${c.name}: exact endpoints and <= 1e-12 parity with Rust`, () => {
			const ctrlPts = c.ctrl.map(([x, y]) => ({ x, y }));
			const degree = Math.min(3, ctrlPts.length - 1);

			// 1. Direct evaluation at EVAL_TS (including t = 0 and t = 1).
			const evalActual = EVAL_TS.map((t) => evaluateBSpline(ctrlPts, t, degree));
			expect(evalActual[0].x).toBe(c.ctrl[0][0]);
			expect(evalActual[0].y).toBe(c.ctrl[0][1]);
			const lastIn = c.ctrl[c.ctrl.length - 1];
			expect(evalActual[evalActual.length - 1].x).toBe(lastIn[0]);
			expect(evalActual[evalActual.length - 1].y).toBe(lastIn[1]);

			for (let i = 0; i < EVAL_TS.length; i++) {
				expect(Math.abs(evalActual[i].x - c.evalExpected[i][0])).toBeLessThanOrEqual(1e-12);
				expect(Math.abs(evalActual[i].y - c.evalExpected[i][1])).toBeLessThanOrEqual(1e-12);
			}

			// 2. Uniform sampling (`sampleBSpline` vs `waffle_types::regions::sample_sketch_spline`).
			const sampled = sampleBSpline(ctrlPts, 8, degree);
			expect(sampled.length).toBe(9);
			expect(sampled[0].x).toBe(c.ctrl[0][0]);
			expect(sampled[0].y).toBe(c.ctrl[0][1]);
			expect(sampled[8].x).toBe(lastIn[0]);
			expect(sampled[8].y).toBe(lastIn[1]);

			for (let i = 0; i <= 8; i++) {
				expect(Math.abs(sampled[i].x - c.sampled8Expected[i][0])).toBeLessThanOrEqual(1e-12);
				expect(Math.abs(sampled[i].y - c.sampled8Expected[i][1])).toBeLessThanOrEqual(1e-12);
			}

			// 3. Global interpolation (`fitBSplineToPoints` vs `nurbs_rs::bspline::global_curve_interp`).
			if (c.fitCubicExpected) {
				const fitCtrl = fitBSplineToPoints(ctrlPts, 3);
				expect(fitCtrl.length).toBe(ctrlPts.length);
				expect(fitCtrl[0].x).toBe(c.ctrl[0][0]);
				expect(fitCtrl[0].y).toBe(c.ctrl[0][1]);
				expect(fitCtrl[fitCtrl.length - 1].x).toBe(lastIn[0]);
				expect(fitCtrl[fitCtrl.length - 1].y).toBe(lastIn[1]);

				for (let i = 0; i < fitCtrl.length; i++) {
					expect(Math.abs(fitCtrl[i].x - c.fitCubicExpected[i][0])).toBeLessThanOrEqual(1e-12);
					expect(Math.abs(fitCtrl[i].y - c.fitCubicExpected[i][1])).toBeLessThanOrEqual(1e-12);
				}
			}
		});
	}

	test('endpoints equal first/last control point exactly even on non-power-of-two knot spans', () => {
		// 24 control points at degree 3 gives 21 spans (knot step 1/21, non-dyadic).
		const ctrl = Array.from({ length: 24 }, (_, i) => ({
			x: -3.2 + i * 1.13,
			y: 2.7 - i * 0.91
		}));
		const s0 = evaluateBSpline(ctrl, 0.0, 3);
		const s1 = evaluateBSpline(ctrl, 1.0, 3);
		expect(s0.x).toBe(ctrl[0].x);
		expect(s0.y).toBe(ctrl[0].y);
		expect(s1.x).toBe(ctrl[23].x);
		expect(s1.y).toBe(ctrl[23].y);
	});

	test('refuses invalid degree, out-of-domain t, silent degree clamp, and degenerate fits loudly', () => {
		const twoPts = [
			{ x: 0, y: 0 },
			{ x: 1, y: 1 }
		];
		const threePts = [
			{ x: 0, y: 0 },
			{ x: 1, y: 2 },
			{ x: 2, y: 0 }
		];
		const fourPts = [
			{ x: 0, y: 0 },
			{ x: 1, y: 2 },
			{ x: 2, y: -1 },
			{ x: 3, y: 1 }
		];

		// No silent degree clamping inside clampedKnotVector / evaluateBSpline / sampleBSpline.
		expect(() => clampedKnotVector(2, 3)).toThrow(/requires at least 4 control points/);
		expect(() => clampedKnotVector(3, 0)).toThrow(/degree must be an integer >= 1/);
		expect(() => evaluateBSpline(twoPts, 0.5, 3)).toThrow(/requires at least 4 control points/);
		expect(() => sampleBSpline(twoPts, 16)).toThrow(/requires at least 4 control points/);
		expect(() => sampleBSpline(threePts, 16, 3)).toThrow(/requires at least 4 control points/);

		// Out-of-domain parameters fail loudly instead of clamping.
		expect(() => evaluateBSpline(fourPts, -1e-12, 3)).toThrow(/outside domain/);
		expect(() => evaluateBSpline(fourPts, 1 + 1e-12, 3)).toThrow(/outside domain/);

		// Interpolation solves n == degree + 1 (4 pts at degree 3) and throws on < degree + 1 or zero chord.
		const solved4 = fitBSplineToPoints(fourPts, 3);
		expect(solved4.length).toBe(4);
		expect(solved4[0]).toEqual(fourPts[0]);
		expect(solved4[3]).toEqual(fourPts[3]);

		expect(() => fitBSplineToPoints(threePts, 3)).toThrow(/requires at least 4 data points/);
		expect(() =>
			fitBSplineToPoints(
				[
					{ x: 0, y: 0 },
					{ x: 1, y: 1 },
					{ x: 1, y: 1 },
					{ x: 2, y: 0 }
				],
				3
			)
		).toThrow(/zero chord length/);
	});
});
