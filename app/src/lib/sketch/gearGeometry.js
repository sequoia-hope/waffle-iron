/**
 * Involute gear profile geometry generation.
 *
 * Pure math — no framework dependencies. Generates sketch entities
 * (points, splines, arcs) for a complete involute spur gear profile.
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
 * @property {Array<{pointIndices: number[]}>} splines - Spline entity definitions (indices into points)
 * @property {Array<{centerIndex: number, startIndex: number, endIndex: number}>} arcs - Arc entity definitions
 * @property {number} pitchRadius - Pitch circle radius
 * @property {number} baseRadius - Base circle radius
 * @property {number} addendumRadius - Addendum (tip) circle radius
 * @property {number} dedendumRadius - Dedendum (root) circle radius
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
	const rootR = Math.max(dedendumR, baseR * 0.95); // clamp so root doesn't go below base

	// Angular pitch
	const angularPitch = (2 * Math.PI) / N;

	// Involute at pitch circle: inv(α) = tan(α) - α
	const invAlpha = involute(alpha);

	// Half tooth thickness at pitch circle (in angular terms)
	const halfToothAngle = angularPitch / 4 + invAlpha;

	// Backlash angular offset
	const backlashAngle = backlash / (2 * pitchR);

	const points = [];
	const splines = [];
	const arcs = [];

	// Helper: transform point by rotation and translation
	function transform(x, y, angle) {
		const ca = Math.cos(angle + rotationOffset);
		const sa = Math.sin(angle + rotationOffset);
		return {
			x: cx + x * ca - y * sa,
			y: cy + x * sa + y * ca
		};
	}

	// Helper: add a point and return its index
	function addPoint(x, y) {
		const idx = points.length;
		points.push({ x, y });
		return idx;
	}

	// Single center point shared by all arcs
	const centerIdx = addPoint(cx, cy);

	// Roll angle range: 0 at base circle, increases outward
	const maxRollAngle = Math.sqrt((addendumR / baseR) ** 2 - 1);
	const startRollAngle = baseR > rootR ? 0 : Math.sqrt(Math.max(0, (rootR / baseR) ** 2 - 1));
	const numInvSamples = 12;

	// Compute the right-involute start position for a given tooth
	function rightInvoluteStartPos(toothAngle) {
		const rightStartAngle = toothAngle + halfToothAngle - backlashAngle;
		const pt = involutePoint(baseR, startRollAngle);
		const invAngle = Math.atan2(pt.y, pt.x);
		const r = Math.sqrt(pt.x * pt.x + pt.y * pt.y);
		const adjustedAngle = rightStartAngle - invAngle;
		return transform(r * Math.cos(adjustedAngle), r * Math.sin(adjustedAngle), 0);
	}

	// Pre-create the right-start point for each tooth so cross-tooth arcs
	// share the same point ID (no duplicate points at the same position).
	const toothRightStartIdx = [];
	for (let tooth = 0; tooth < N; tooth++) {
		const pos = rightInvoluteStartPos(tooth * angularPitch);
		toothRightStartIdx.push(addPoint(pos.x, pos.y));
	}

	// Generate profile for each tooth
	for (let tooth = 0; tooth < N; tooth++) {
		const toothAngle = tooth * angularPitch;

		// Right involute flank (from base to addendum)
		const rightInvolutePoints = [];
		const rightStartAngle = toothAngle + halfToothAngle - backlashAngle;

		for (let i = 0; i <= numInvSamples; i++) {
			const t = i / numInvSamples;
			const roll = startRollAngle + t * (maxRollAngle - startRollAngle);
			const pt = involutePoint(baseR, roll);
			const invAngle = Math.atan2(pt.y, pt.x);
			const r = Math.sqrt(pt.x * pt.x + pt.y * pt.y);
			const adjustedAngle = rightStartAngle - invAngle;
			rightInvolutePoints.push(transform(
				r * Math.cos(adjustedAngle),
				r * Math.sin(adjustedAngle),
				0
			));
		}

		// Left involute flank (mirror of right, from addendum to base)
		const leftInvolutePoints = [];
		const leftStartAngle = toothAngle - halfToothAngle + backlashAngle;

		for (let i = numInvSamples; i >= 0; i--) {
			const t = i / numInvSamples;
			const roll = startRollAngle + t * (maxRollAngle - startRollAngle);
			const pt = involutePoint(baseR, roll);
			const invAngle = Math.atan2(pt.y, pt.x);
			const r = Math.sqrt(pt.x * pt.x + pt.y * pt.y);
			const adjustedAngle = leftStartAngle + invAngle;
			leftInvolutePoints.push(transform(
				r * Math.cos(adjustedAngle),
				r * Math.sin(adjustedAngle),
				0
			));
		}

		// Right involute: first point is the pre-created shared point
		const rightStartIdx = toothRightStartIdx[tooth];
		const rightMidIndices = [];
		for (let i = 1; i < rightInvolutePoints.length - 1; i++) {
			rightMidIndices.push(addPoint(rightInvolutePoints[i].x, rightInvolutePoints[i].y));
		}
		const rightEndIdx = addPoint(
			rightInvolutePoints[rightInvolutePoints.length - 1].x,
			rightInvolutePoints[rightInvolutePoints.length - 1].y
		);

		// Fit B-spline to right involute and create spline entity
		const rightCtrlPts = fitBSplineToPoints(rightInvolutePoints);
		splines.push({
			pointIndices: [rightStartIdx, ...rightMidIndices, rightEndIdx],
			controlPoints: rightCtrlPts
		});

		// Tip arc: from right involute end to left involute start
		const leftStartIdx = addPoint(leftInvolutePoints[0].x, leftInvolutePoints[0].y);

		arcs.push({
			centerIndex: centerIdx,
			startIndex: rightEndIdx,
			endIndex: leftStartIdx
		});

		// Create points for left involute
		const leftMidIndices = [];
		for (let i = 1; i < leftInvolutePoints.length - 1; i++) {
			leftMidIndices.push(addPoint(leftInvolutePoints[i].x, leftInvolutePoints[i].y));
		}
		const leftEndIdx = addPoint(
			leftInvolutePoints[leftInvolutePoints.length - 1].x,
			leftInvolutePoints[leftInvolutePoints.length - 1].y
		);

		// Fit B-spline to left involute
		const leftCtrlPts = fitBSplineToPoints(leftInvolutePoints);
		splines.push({
			pointIndices: [leftStartIdx, ...leftMidIndices, leftEndIdx],
			controlPoints: leftCtrlPts
		});

		// Root arc: from left involute end to next tooth's shared right-start point
		const nextTooth = (tooth + 1) % N;

		arcs.push({
			centerIndex: centerIdx,
			startIndex: leftEndIdx,
			endIndex: toothRightStartIdx[nextTooth]
		});
	}

	return {
		points,
		splines,
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
	const rootR = Math.max(dedendumR, baseR * 0.95);

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
	const samplesPerArc = 4;

	for (let tooth = 0; tooth < N; tooth++) {
		const toothAngle = tooth * angularPitch;
		const rightStartAngle = toothAngle + halfToothAngle - backlashAngle;
		const leftStartAngle = toothAngle - halfToothAngle + backlashAngle;

		const maxRollAngle = Math.sqrt((addendumR / baseR) ** 2 - 1);
		const startRollAngle = baseR > rootR ? 0 : Math.sqrt(Math.max(0, (rootR / baseR) ** 2 - 1));

		// Right involute (base to tip)
		for (let i = 0; i <= samplesPerInvolute; i++) {
			const t = i / samplesPerInvolute;
			const roll = startRollAngle + t * (maxRollAngle - startRollAngle);
			const pt = involutePoint(baseR, roll);
			const invAngle = Math.atan2(pt.y, pt.x);
			const r = Math.sqrt(pt.x * pt.x + pt.y * pt.y);
			const adjustedAngle = rightStartAngle - invAngle;
			polyline.push(transform(
				r * Math.cos(adjustedAngle),
				r * Math.sin(adjustedAngle)
			));
		}

		// Tip arc
		const tipStartAngle = rightStartAngle - Math.sqrt((addendumR / baseR) ** 2 - 1) +
			Math.atan2(...(() => { const p = involutePoint(baseR, maxRollAngle); return [p.y, p.x]; })());
		const rightTipAngle = Math.atan2(
			polyline[polyline.length - 1].y - cy,
			polyline[polyline.length - 1].x - cx
		);

		// Left involute tip point
		const leftTipPt = involutePoint(baseR, maxRollAngle);
		const leftTipInvAngle = Math.atan2(leftTipPt.y, leftTipPt.x);
		const leftTipR = Math.sqrt(leftTipPt.x * leftTipPt.x + leftTipPt.y * leftTipPt.y);
		const leftTipAdjAngle = leftStartAngle + leftTipInvAngle;
		const leftTipWorld = transform(
			leftTipR * Math.cos(leftTipAdjAngle),
			leftTipR * Math.sin(leftTipAdjAngle)
		);
		const leftTipAngle = Math.atan2(leftTipWorld.y - cy, leftTipWorld.x - cx);

		// Tip arc from right tip to left tip
		let tipSweep = leftTipAngle - rightTipAngle;
		if (tipSweep < 0) tipSweep += 2 * Math.PI;
		if (tipSweep > Math.PI) tipSweep -= 2 * Math.PI;

		for (let i = 1; i <= samplesPerArc; i++) {
			const t = i / samplesPerArc;
			const angle = rightTipAngle + t * tipSweep;
			polyline.push(transform(
				addendumR * Math.cos(angle),
				addendumR * Math.sin(angle)
			));
		}

		// Left involute (tip to base)
		for (let i = samplesPerInvolute; i >= 0; i--) {
			const t = i / samplesPerInvolute;
			const roll = startRollAngle + t * (maxRollAngle - startRollAngle);
			const pt = involutePoint(baseR, roll);
			const invAngle = Math.atan2(pt.y, pt.x);
			const r = Math.sqrt(pt.x * pt.x + pt.y * pt.y);
			const adjustedAngle = leftStartAngle + invAngle;
			polyline.push(transform(
				r * Math.cos(adjustedAngle),
				r * Math.sin(adjustedAngle)
			));
		}

		// Root arc to next tooth
		const leftRootAngle = Math.atan2(
			polyline[polyline.length - 1].y - cy,
			polyline[polyline.length - 1].x - cx
		);

		const nextToothAngle = (tooth + 1) * angularPitch;
		const nextRightStartAngle = nextToothAngle + halfToothAngle - backlashAngle;
		const nextRightRoll = startRollAngle;
		const nextPt = involutePoint(baseR, nextRightRoll);
		const nextInvAngle = Math.atan2(nextPt.y, nextPt.x);
		const nextR = Math.sqrt(nextPt.x * nextPt.x + nextPt.y * nextPt.y);
		const nextAdjAngle = nextRightStartAngle - nextInvAngle;
		const nextRightStart = transform(
			nextR * Math.cos(nextAdjAngle),
			nextR * Math.sin(nextAdjAngle)
		);
		const nextRightAngle = Math.atan2(nextRightStart.y - cy, nextRightStart.x - cx);

		let rootSweep = nextRightAngle - leftRootAngle;
		if (rootSweep < 0) rootSweep += 2 * Math.PI;
		if (rootSweep > Math.PI) rootSweep -= 2 * Math.PI;

		for (let i = 1; i <= samplesPerArc; i++) {
			const t = i / samplesPerArc;
			const angle = leftRootAngle + t * rootSweep;
			polyline.push(transform(
				rootR * Math.cos(angle),
				rootR * Math.sin(angle)
			));
		}
	}

	// Close the loop
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
