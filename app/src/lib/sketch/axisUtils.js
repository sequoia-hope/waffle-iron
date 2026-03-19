/**
 * Shared axis computation utilities for revolve operations.
 * Extracts axis origin + direction from sketch entities or 3D edge vertices.
 */

/**
 * Compute plane basis vectors from a normal vector.
 * Matches buildSketchPlane() in sketchCoords.js and tangent_x_from_normal in rebuild.rs.
 * @param {number[]} pn - plane normal [x, y, z]
 * @returns {{ right: number[], up: number[] }}
 */
export function computePlaneBasis(pn) {
	const nDotZ = Math.abs(pn[2]);
	const ref = nDotZ < 0.99 ? [0, 0, 1] : [1, 0, 0];

	const rx = ref[1] * pn[2] - ref[2] * pn[1];
	const ry = ref[2] * pn[0] - ref[0] * pn[2];
	const rz = ref[0] * pn[1] - ref[1] * pn[0];
	const rlen = Math.sqrt(rx * rx + ry * ry + rz * rz);
	const right = rlen > 1e-10 ? [rx / rlen, ry / rlen, rz / rlen] : [1, 0, 0];

	const ux = pn[1] * right[2] - pn[2] * right[1];
	const uy = pn[2] * right[0] - pn[0] * right[2];
	const uz = pn[0] * right[1] - pn[1] * right[0];
	const up = [ux, uy, uz];

	return { right, up };
}

/**
 * Compute 3D axis from a sketch Line entity.
 * @param {{ type: 'Line', start: number[], end: number[] }} entity
 * @param {number[]} planeOrigin - [x, y, z]
 * @param {number[]} planeNormal - [x, y, z]
 * @returns {{ origin: number[], direction: number[] } | null}
 */
export function computeAxisFromSketchLine(entity, planeOrigin, planeNormal) {
	const { right, up } = computePlaneBasis(planeNormal);
	const po = planeOrigin;

	const dx2d = entity.end[0] - entity.start[0];
	const dy2d = entity.end[1] - entity.start[1];
	const len = Math.sqrt(dx2d * dx2d + dy2d * dy2d);
	if (len < 1e-10) return null;

	const nx = dx2d / len;
	const ny = dy2d / len;

	return {
		direction: [
			right[0] * nx + up[0] * ny,
			right[1] * nx + up[1] * ny,
			right[2] * nx + up[2] * ny
		],
		origin: [
			po[0] + right[0] * entity.start[0] + up[0] * entity.start[1],
			po[1] + right[1] * entity.start[0] + up[1] * entity.start[1],
			po[2] + right[2] * entity.start[0] + up[2] * entity.start[1]
		]
	};
}

/**
 * Compute 3D axis from a sketch Circle entity (axis = plane normal through circle center).
 * @param {{ type: 'Circle', center: number[] }} entity
 * @param {number[]} planeOrigin - [x, y, z]
 * @param {number[]} planeNormal - [x, y, z]
 * @returns {{ origin: number[], direction: number[] }}
 */
export function computeAxisFromSketchCircle(entity, planeOrigin, planeNormal) {
	const { right, up } = computePlaneBasis(planeNormal);
	const po = planeOrigin;

	return {
		direction: [planeNormal[0], planeNormal[1], planeNormal[2]],
		origin: [
			po[0] + right[0] * entity.center[0] + up[0] * entity.center[1],
			po[1] + right[1] * entity.center[0] + up[1] * entity.center[1],
			po[2] + right[2] * entity.center[0] + up[2] * entity.center[1]
		]
	};
}

/**
 * Compute axis from two 3D edge vertex positions (start → end defines direction).
 * @param {number[]} startPos - [x, y, z]
 * @param {number[]} endPos - [x, y, z]
 * @returns {{ origin: number[], direction: number[] } | null}
 */
export function computeAxisFromEdgeVertices(startPos, endPos) {
	const dx = endPos[0] - startPos[0];
	const dy = endPos[1] - startPos[1];
	const dz = endPos[2] - startPos[2];
	const len = Math.sqrt(dx * dx + dy * dy + dz * dz);
	if (len < 1e-10) return null;

	return {
		origin: [...startPos],
		direction: [dx / len, dy / len, dz / len]
	};
}
