/**
 * The framing math the viewport and the agent capture SHARE
 * (`specs/agent_mechanical_design.md` §9.2). `CameraControls` moves the user's
 * camera with it; `capture.js` builds a throwaway camera with the same numbers,
 * so a captured `view` + `fit` frames the model exactly as the View Cube and
 * the F key do. Duplicating it would have let the picture disagree with the
 * viewport it claims to be a picture of.
 *
 * Pure: no Svelte, no store, no renderer. Scene traversal reads only
 * `userData.waffleType`, which `CadModel` / `EdgeOverlay` / the sketch
 * renderers stamp.
 */
import * as THREE from 'three';

/**
 * Standard views in MODEL space, where up is +Z. This used to be the stock
 * three.js Y-up table, which is a legal camera for every name and the promised
 * view for none: on a Z-up part `front` gave a plan, `top` an upside-down
 * elevation and `iso` laid the model on its side. Every model in this repo (and
 * everything an agent authors) stands along +Z, so the names are defined
 * against that: `front` looks along +Y at an elevation, `top` looks down −Z,
 * and `iso` is the front-right-top three-quarter. (The built-in datum planes
 * keep their SolidWorks Y-up names — "Front" is still the XY plane — which is a
 * separate inconsistency, not this one.)
 */
export const STANDARD_VIEWS = {
	front:  { pos: [0, -1, 0], up: [0, 0, 1] },
	back:   { pos: [0, 1, 0],  up: [0, 0, 1] },
	top:    { pos: [0, 0, 1],  up: [0, 1, 0] },
	bottom: { pos: [0, 0, -1], up: [0, -1, 0] },
	left:   { pos: [-1, 0, 0], up: [0, 0, 1] },
	right:  { pos: [1, 0, 0],  up: [0, 0, 1] },
	iso:    { pos: [1, -1, 1], up: [0, 0, 1] }
};

/** The names `viewport_view` and `viewport_capture` accept, in table order. */
export const VIEW_NAMES = Object.keys(STANDARD_VIEWS);

/** Slack a fit leaves around the framed box (1.0 = the box fills the frame). */
export const FIT_MARGIN = 1.5;

/**
 * Camera distance (and, in orthographic, half-height of the frustum) that frames
 * a box of largest dimension `maxDim`.
 *
 * The orthographic distance is deliberately outside the box — its half-diagonal
 * is below `maxDim` — so picking and occlusion rays start in front of the part
 * rather than inside it.
 *
 * @param {{ maxDim: number, projection: 'perspective' | 'orthographic', fov?: number }} opts
 * @returns {{ distance: number, frustumHalf: number }}
 */
export function fitDistance({ maxDim, projection, fov = 50 }) {
	const dim = Number.isFinite(maxDim) && maxDim > 0 ? maxDim : 0.01;
	if (projection === 'orthographic') {
		return { distance: dim * 2, frustumHalf: (dim * FIT_MARGIN) / 2 };
	}
	const rad = fov * (Math.PI / 180);
	return { distance: (dim / (2 * Math.tan(rad / 2))) * FIT_MARGIN, frustumHalf: (dim * FIT_MARGIN) / 2 };
}

/**
 * The box Fit All frames: the visible model, else the sketches, else null.
 * Datum planes and other decorations carry no `waffleType`, so they never
 * dominate the box for small geometry.
 * @param {THREE.Object3D | null | undefined} scene
 * @returns {THREE.Box3 | null}
 */
export function fitBoxFor(scene) {
	if (!scene) return null;

	const modelBox = new THREE.Box3();
	const sketchBox = new THREE.Box3();

	scene.traverse((obj) => {
		if (!obj.visible) return;
		const type = /** @type {any} */ (obj).userData?.waffleType;
		if (type === 'model') {
			modelBox.expandByObject(obj);
		} else if (type === 'sketch') {
			sketchBox.expandByObject(obj);
		}
	});

	if (!modelBox.isEmpty()) return modelBox;
	if (!sketchBox.isEmpty()) return sketchBox;
	return null;
}

/**
 * The box of the named bodies (visible model meshes only), plus the ids that
 * matched nothing — so a caller framing a body it cannot see is told which one,
 * instead of getting some other body's frame.
 * @param {THREE.Object3D | null | undefined} scene
 * @param {string[]} bodyIds
 * @returns {{ box: THREE.Box3 | null, missing: string[] }}
 */
export function bodiesBoxFor(scene, bodyIds) {
	const want = new Set(bodyIds);
	const seen = new Set();
	const box = new THREE.Box3();
	if (scene) {
		scene.traverse((obj) => {
			if (!obj.visible) return;
			const ud = /** @type {any} */ (obj).userData;
			if (ud?.waffleType !== 'model' || !want.has(ud.bodyId)) return;
			seen.add(ud.bodyId);
			box.expandByObject(obj);
		});
	}
	return {
		box: box.isEmpty() ? null : box,
		missing: bodyIds.filter((id) => !seen.has(id))
	};
}

/**
 * The box a `frame` argument names, without moving anything.
 * `point` + `radius` is a cube of half-size `radius`; `body_ids` is the union
 * of those bodies' boxes.
 * @param {THREE.Object3D | null | undefined} scene
 * @param {{ body_ids?: string[], point?: number[], radius?: number } | null} frame
 * @returns {{ box: THREE.Box3 | null, missing: string[] }}
 */
export function frameBoxFor(scene, frame) {
	if (frame?.body_ids?.length) return bodiesBoxFor(scene, frame.body_ids);
	if (frame?.point) {
		const c = new THREE.Vector3(frame.point[0], frame.point[1], frame.point[2]);
		const r = /** @type {number} */ (frame.radius);
		return { box: new THREE.Box3(c.clone().subScalar(r), c.clone().addScalar(r)), missing: [] };
	}
	return { box: null, missing: [] };
}
