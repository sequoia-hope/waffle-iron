/**
 * Plane data model — generic plane abstraction for datum planes.
 *
 * Supports multiple definition methods:
 * - point-normal: origin + normal vector
 * - offset: parallel offset from another plane
 * - three-points: through three points
 */

// --- Stable UUIDs for built-in planes ---

export const FRONT_PLANE_ID = '00000000-0000-0000-0000-000000000001';
export const TOP_PLANE_ID = '00000000-0000-0000-0000-000000000002';
export const RIGHT_PLANE_ID = '00000000-0000-0000-0000-000000000003';

/** Half-size for plane geometry (36x36 total). */
export const PLANE_HALF_SIZE = 18;

/**
 * @typedef {{ method: 'point-normal', origin: [number,number,number], normal: [number,number,number] }} PointNormalDef
 * @typedef {{ method: 'offset', basePlaneId: string, distance: number }} OffsetDef
 * @typedef {{ method: 'three-points', points: [[number,number,number],[number,number,number],[number,number,number]] }} ThreePointsDef
 * @typedef {PointNormalDef | OffsetDef | ThreePointsDef} PlaneDefinition
 */

/**
 * @typedef {object} Plane
 * @property {string} id - Stable UUID
 * @property {string} name - Display name (e.g. "Front")
 * @property {PlaneDefinition} definition
 * @property {number} color - Base hex color
 * @property {number} hoverColor - Hover hex color
 * @property {number} selectedColor - Selected hex color
 * @property {number} borderColor - Border hex color
 * @property {boolean} builtin - Whether this is a built-in plane
 */

/** @type {Plane[]} */
export const BUILTIN_PLANES = [
	{
		id: FRONT_PLANE_ID,
		name: 'Front',
		definition: { method: 'point-normal', origin: [0, 0, 0], normal: [0, 0, 1] },
		color: 0x4444aa,
		hoverColor: 0x6666dd,
		selectedColor: 0x8888ff,
		borderColor: 0x6666cc,
		builtin: true,
	},
	{
		id: TOP_PLANE_ID,
		name: 'Top',
		definition: { method: 'point-normal', origin: [0, 0, 0], normal: [0, 1, 0] },
		color: 0x44aa44,
		hoverColor: 0x66dd66,
		selectedColor: 0x88ff88,
		borderColor: 0x66cc66,
		builtin: true,
	},
	{
		id: RIGHT_PLANE_ID,
		name: 'Right',
		definition: { method: 'point-normal', origin: [0, 0, 0], normal: [1, 0, 0] },
		color: 0xaa4444,
		hoverColor: 0xdd6666,
		selectedColor: 0xff8888,
		borderColor: 0xcc6666,
		builtin: true,
	},
];

// Legacy plane name → ID mapping (for backward compatibility)
const LEGACY_NAME_TO_ID = {
	XY: FRONT_PLANE_ID,
	XZ: TOP_PLANE_ID,
	YZ: RIGHT_PLANE_ID,
};

/**
 * Resolve a PlaneDefinition to origin + normal.
 * @param {PlaneDefinition} definition
 * @returns {{ origin: [number,number,number], normal: [number,number,number] }}
 */
export function resolvePlane(definition) {
	if (definition.method === 'point-normal') {
		return { origin: definition.origin, normal: definition.normal };
	}
	if (definition.method === 'offset') {
		const base = getPlaneById(definition.basePlaneId);
		if (!base) throw new Error(`Base plane ${definition.basePlaneId} not found`);
		const resolved = resolvePlane(base.definition);
		const d = definition.distance;
		return {
			origin: [
				resolved.origin[0] + resolved.normal[0] * d,
				resolved.origin[1] + resolved.normal[1] * d,
				resolved.origin[2] + resolved.normal[2] * d,
			],
			normal: resolved.normal,
		};
	}
	if (definition.method === 'three-points') {
		const [p0, p1, p2] = definition.points;
		const e1 = [p1[0] - p0[0], p1[1] - p0[1], p1[2] - p0[2]];
		const e2 = [p2[0] - p0[0], p2[1] - p0[1], p2[2] - p0[2]];
		const nx = e1[1] * e2[2] - e1[2] * e2[1];
		const ny = e1[2] * e2[0] - e1[0] * e2[2];
		const nz = e1[0] * e2[1] - e1[1] * e2[0];
		const len = Math.sqrt(nx * nx + ny * ny + nz * nz);
		if (len < 1e-12) throw new Error('Degenerate plane: three points are collinear');
		return {
			origin: /** @type {[number,number,number]} */ ([...p0]),
			normal: /** @type {[number,number,number]} */ ([nx / len, ny / len, nz / len]),
		};
	}
	throw new Error(`Unknown plane definition method: ${/** @type {any} */ (definition).method}`);
}

/**
 * Look up a plane by ID.
 * @param {string} id
 * @returns {Plane | undefined}
 */
export function getPlaneById(id) {
	return BUILTIN_PLANES.find((p) => p.id === id);
}

/**
 * Build a GeomRef for a datum plane.
 * @param {string} id - Plane UUID
 * @returns {{ kind: { type: string }, anchor: { type: string, id: string } }}
 */
export function makePlaneRef(id) {
	return { kind: { type: 'Face' }, anchor: { type: 'DatumPlane', id } };
}

/**
 * Check if a GeomRef refers to a datum plane.
 * @param {any} ref
 * @returns {boolean}
 */
export function isDatumPlaneRef(ref) {
	return ref?.anchor?.type === 'DatumPlane';
}

/**
 * Extract plane ID from a GeomRef, supporting both old and new formats.
 * Old format: `{ anchor: { type: 'DatumPlane', plane: 'XY' } }`
 * New format: `{ anchor: { type: 'DatumPlane', id: '00000000-...' } }`
 * @param {any} ref
 * @returns {string | null}
 */
export function getPlaneIdFromRef(ref) {
	if (!isDatumPlaneRef(ref)) return null;
	if (ref.anchor.id) return ref.anchor.id;
	if (ref.anchor.plane) return LEGACY_NAME_TO_ID[ref.anchor.plane] ?? null;
	return null;
}
