/**
 * Involute gear profile geometry generation.
 *
 * Pure math — no framework dependencies. Generates sketch entities
 * (points, splines, lines, arcs) for a complete involute spur gear profile.
 *
 * Per tooth profile:
 *   root_right → line(rootR→baseR) → right_involute(baseR→addendumR) →
 *   tip_arc(addendumR) → left_involute(addendumR→baseR) → line(baseR→rootR)
 *   → root_left
 * Between teeth:
 *   root_left → root_arc(rootR) → next_root_right
 *
 * Radial lines from rootR to baseR break the root circle's continuity.
 * The tip arcs are already non-continuous (separated by involutes + lines).
 */

import { fitBSplineToPoints } from './bspline.js';

/**
 * Involute function: inv(α) = tan(α) - α
 * @param {number} alpha - Pressure angle in radians
 * @returns {number}
 */
export function involute(alpha) {
	return Math.tan(alpha) - alpha;
}

/**
 * Compute a point on the involute curve of a base circle.
 * @param {number} baseRadius - Base circle radius
 * @param {number} rollAngle - Roll angle (parameter along involute)
 * @returns {{x: number, y: number}}
 */
export function involutePoint(baseRadius, rollAngle) {
	const x = baseRadius * (Math.cos(rollAngle) + rollAngle * Math.sin(rollAngle));
	const y = baseRadius * (Math.sin(rollAngle) - rollAngle * Math.cos(rollAngle));
	return { x, y };
}

/**
 * @typedef {Object} GearParams
 * @property {number} toothCount - Number of teeth (N >= 4)
 * @property {number} module - Gear module (m > 0)
 * @property {number} [pressureAngle=20] - Pressure angle in degrees
 * @property {number} [backlash=0] - Backlash offset in mm
 * @property {number} [centerX=0] - Center X in sketch coordinates
 * @property {number} [centerY=0] - Center Y in sketch coordinates
 * @property {number} [rotationOffset=0] - Rotation offset in radians
 */

/**
 * @typedef {Object} GearProfileResult
 * @property {Array<{x: number, y: number}>} points - All point positions
 * @property {Array<{pointIndices: number[]}>} splines - Spline entity definitions
 * @property {Array<{startIndex: number, endIndex: number}>} lines - Line entity definitions
 * @property {Array<{centerIndex: number, startIndex: number, endIndex: number}>} arcs - Arc entity definitions
 * @property {number} pitchRadius
 * @property {number} baseRadius
 * @property {number} addendumRadius
 * @property {number} dedendumRadius
 */

/**
 * Generate a complete involute gear profile.
 *
 * @param {GearParams} params
 * @returns {GearProfileResult}
 */
export function generateGearProfile(params) {
	const {
		toothCount: N,
		module: m,
		pressureAngle: pressAngleDeg = 20,
		backlash = 0,
		centerX: cx = 0,
		centerY: cy = 0,
		rotationOffset = 0
	} = params;

	const alpha = pressAngleDeg * Math.PI / 180;
	const pitchR = (N * m) / 2;
	const baseR = pitchR * Math.cos(alpha);
	const addendumR = pitchR + m;
	const dedendumR = pitchR - 1.25 * m;
	const rootR = Math.max(dedendumR, baseR * 0.5);

	const angularPitch = (2 * Math.PI) / N;
	const invAlpha = involute(alpha);
	const halfToothAngle = angularPitch / 4 + invAlpha;
	const backlashAngle = backlash / (2 * pitchR);

	const points = [];
	const splines = [];
	const lines = [];
	const arcs = [];

	function transform(x, y) {
		const ca = Math.cos(rotationOffset);
		const sa = Math.sin(rotationOffset);
		return {
			x: cx + x * ca - y * sa,
			y: cy + x * sa + y * ca
		};
	}

	function addPoint(x, y) {
		const idx = points.length;
		points.push({ x, y });
		return idx;
	}

	// Single center point for all arcs
	const centerIdx = addPoint(cx, cy);

	const maxRollAngle = Math.sqrt((addendumR / baseR) ** 2 - 1);
	const numInvSamples = 12;

	// Pre-create root points (on rootR) for each tooth — right and left sides
	const toothRightRootIdx = [];
	const toothLeftRootIdx = [];

	for (let tooth = 0; tooth < N; tooth++) {
		const toothAngle = tooth * angularPitch;
		const rightAngle = toothAngle + halfToothAngle - backlashAngle;
		const leftAngle = toothAngle - halfToothAngle + backlashAngle;

		const rr = transform(rootR * Math.cos(rightAngle), rootR * Math.sin(rightAngle));
		toothRightRootIdx.push(addPoint(rr.x, rr.y));

		const lr = transform(rootR * Math.cos(leftAngle), rootR * Math.sin(leftAngle));
		toothLeftRootIdx.push(addPoint(lr.x, lr.y));
	}

	for (let tooth = 0; tooth < N; tooth++) {
		const toothAngle = tooth * angularPitch;
		const rightStartAngle = toothAngle + halfToothAngle - backlashAngle;
		const leftStartAngle = toothAngle - halfToothAngle + backlashAngle;

		// Trace counterclockwise: left side up → tip → right side down → root gap
		// This ensures root arcs span the SHORT gap between adjacent teeth.

		// === Radial line: rootR → baseR (left side) ===
		const leftInvolutePoints = [];
		for (let i = 0; i <= numInvSamples; i++) {
			const t = i / numInvSamples;
			const roll = t * maxRollAngle;
			const pt = involutePoint(baseR, roll);
			const invAngle = Math.atan2(pt.y, pt.x);
			const r = Math.sqrt(pt.x * pt.x + pt.y * pt.y);
			const adjustedAngle = leftStartAngle + invAngle;
			leftInvolutePoints.push(transform(
				r * Math.cos(adjustedAngle),
				r * Math.sin(adjustedAngle)
			));
		}

		const leftBaseIdx = addPoint(leftInvolutePoints[0].x, leftInvolutePoints[0].y);
		lines.push({ startIndex: toothLeftRootIdx[tooth], endIndex: leftBaseIdx });

		// === Left involute spline (baseR → addendumR) ===
		const leftMidIndices = [];
		for (let i = 1; i < leftInvolutePoints.length - 1; i++) {
			leftMidIndices.push(addPoint(leftInvolutePoints[i].x, leftInvolutePoints[i].y));
		}
		const leftTipIdx = addPoint(
			leftInvolutePoints[leftInvolutePoints.length - 1].x,
			leftInvolutePoints[leftInvolutePoints.length - 1].y
		);
		splines.push({
			pointIndices: [leftBaseIdx, ...leftMidIndices, leftTipIdx],
			controlPoints: fitBSplineToPoints(leftInvolutePoints)
		});

		// === Tip arc (addendumR): left tip → right tip ===
		const rightInvolutePoints = [];
		for (let i = numInvSamples; i >= 0; i--) {
			const t = i / numInvSamples;
			const roll = t * maxRollAngle;
			const pt = involutePoint(baseR, roll);
			const invAngle = Math.atan2(pt.y, pt.x);
			const r = Math.sqrt(pt.x * pt.x + pt.y * pt.y);
			const adjustedAngle = rightStartAngle - invAngle;
			rightInvolutePoints.push(transform(
				r * Math.cos(adjustedAngle),
				r * Math.sin(adjustedAngle)
			));
		}

		const rightTipIdx = addPoint(rightInvolutePoints[0].x, rightInvolutePoints[0].y);
		arcs.push({ centerIndex: centerIdx, startIndex: leftTipIdx, endIndex: rightTipIdx });

		// === Right involute spline (addendumR → baseR) ===
		const rightMidIndices = [];
		for (let i = 1; i < rightInvolutePoints.length - 1; i++) {
			rightMidIndices.push(addPoint(rightInvolutePoints[i].x, rightInvolutePoints[i].y));
		}
		const rightBaseIdx = addPoint(
			rightInvolutePoints[rightInvolutePoints.length - 1].x,
			rightInvolutePoints[rightInvolutePoints.length - 1].y
		);
		splines.push({
			pointIndices: [rightTipIdx, ...rightMidIndices, rightBaseIdx],
			controlPoints: fitBSplineToPoints(rightInvolutePoints)
		});

		// === Radial line: baseR → rootR (right side) ===
		lines.push({ startIndex: rightBaseIdx, endIndex: toothRightRootIdx[tooth] });

		// === Root arc (rootR) to next tooth: right root → next left root ===
		const nextTooth = (tooth + 1) % N;
		arcs.push({
			centerIndex: centerIdx,
			startIndex: toothRightRootIdx[tooth],
			endIndex: toothLeftRootIdx[nextTooth]
		});
	}

	return {
		points,
		splines,
		lines,
		arcs,
		pitchRadius: pitchR,
		baseRadius: baseR,
		addendumRadius: addendumR,
		dedendumRadius: dedendumR
	};
}

/**
 * Generate a flat polyline approximation of a gear profile for live preview.
 *
 * @param {GearParams} params
 * @returns {Array<{x: number, y: number}>}
 */
export function generateGearPreviewPolyline(params) {
	const {
		toothCount: N,
		module: m,
		pressureAngle: pressAngleDeg = 20,
		backlash = 0,
		centerX: cx = 0,
		centerY: cy = 0,
		rotationOffset = 0
	} = params;

	const alpha = pressAngleDeg * Math.PI / 180;
	const pitchR = (N * m) / 2;
	const baseR = pitchR * Math.cos(alpha);
	const addendumR = pitchR + m;
	const dedendumR = pitchR - 1.25 * m;
	const rootR = Math.max(dedendumR, baseR * 0.5);

	const angularPitch = (2 * Math.PI) / N;
	const invAlpha = involute(alpha);
	const halfToothAngle = angularPitch / 4 + invAlpha;
	const backlashAngle = backlash / (2 * pitchR);

	function transform(x, y) {
		const ca = Math.cos(rotationOffset);
		const sa = Math.sin(rotationOffset);
		return {
			x: cx + x * ca - y * sa,
			y: cy + x * sa + y * ca
		};
	}

	const polyline = [];
	const samplesPerInvolute = 8;
	const maxRollAngle = Math.sqrt((addendumR / baseR) ** 2 - 1);

	for (let tooth = 0; tooth < N; tooth++) {
		const toothAngle = tooth * angularPitch;
		const rightStartAngle = toothAngle + halfToothAngle - backlashAngle;
		const leftStartAngle = toothAngle - halfToothAngle + backlashAngle;

		// Left root point
		polyline.push(transform(rootR * Math.cos(leftStartAngle), rootR * Math.sin(leftStartAngle)));

		// Left involute (base to tip)
		for (let i = 0; i <= samplesPerInvolute; i++) {
			const t = i / samplesPerInvolute;
			const roll = t * maxRollAngle;
			const pt = involutePoint(baseR, roll);
			const invAngle = Math.atan2(pt.y, pt.x);
			const r = Math.sqrt(pt.x * pt.x + pt.y * pt.y);
			const adjustedAngle = leftStartAngle + invAngle;
			polyline.push(transform(r * Math.cos(adjustedAngle), r * Math.sin(adjustedAngle)));
		}

		// Right involute (tip to base)
		for (let i = samplesPerInvolute; i >= 0; i--) {
			const t = i / samplesPerInvolute;
			const roll = t * maxRollAngle;
			const pt = involutePoint(baseR, roll);
			const invAngle = Math.atan2(pt.y, pt.x);
			const r = Math.sqrt(pt.x * pt.x + pt.y * pt.y);
			const adjustedAngle = rightStartAngle - invAngle;
			polyline.push(transform(r * Math.cos(adjustedAngle), r * Math.sin(adjustedAngle)));
		}

		// Right root point
		polyline.push(transform(rootR * Math.cos(rightStartAngle), rootR * Math.sin(rightStartAngle)));
	}

	if (polyline.length > 0) {
		polyline.push({ ...polyline[0] });
	}

	return polyline;
}

/**
 * Compute standard gear dimensions from parameters.
 * @param {number} toothCount
 * @param {number} module
 * @param {number} [pressureAngle=20]
 * @returns {{ pitchDiameter: number, baseDiameter: number, addendumDiameter: number, dedendumDiameter: number }}
 */
export function gearDimensions(toothCount, module, pressureAngle = 20) {
	const alpha = pressureAngle * Math.PI / 180;
	const pitchR = (toothCount * module) / 2;
	return {
		pitchDiameter: pitchR * 2,
		baseDiameter: pitchR * Math.cos(alpha) * 2,
		addendumDiameter: (pitchR + module) * 2,
		dedendumDiameter: (pitchR - 1.25 * module) * 2
	};
}
