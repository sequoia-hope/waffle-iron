/**
 * World→screen projection for GUI tests.
 *
 * There is no engine hook that maps an arbitrary 3D world point to an absolute
 * screen pixel, so we reconstruct three.js's `Camera.project()` from the two
 * read-only snapshots the store already exposes:
 *   - `__waffle.getCameraState()`      → { position (eye), target, up }
 *   - `__waffle.viewportDebug().camera.projectionMatrix` → the exact projection
 *     matrix three.js is rendering with (column-major, ortho or perspective,
 *     zoom already baked in).
 *
 * The view matrix is rebuilt with the same lookAt basis three.js uses (camera
 * looks down -Z). The result is identical, to floating-point, to the screen
 * pixel the engine raycasts against — so hovering the returned coordinate hits
 * the same geometry the user would hit. Nothing here mutates app state.
 */

import { getCanvasBounds } from './canvas.js';

function sub(a, b) { return [a[0] - b[0], a[1] - b[1], a[2] - b[2]]; }
function cross(a, b) {
	return [
		a[1] * b[2] - a[2] * b[1],
		a[2] * b[0] - a[0] * b[2],
		a[0] * b[1] - a[1] * b[0],
	];
}
function dot(a, b) { return a[0] * b[0] + a[1] * b[1] + a[2] * b[2]; }
function norm(a) {
	const l = Math.hypot(a[0], a[1], a[2]) || 1;
	return [a[0] / l, a[1] / l, a[2] / l];
}

/**
 * Project a world point to NDC using camera state + projection matrix.
 * Mirrors THREE.Vector3.project(camera).
 * @param {number[]} world - [x, y, z]
 * @param {{position:number[], target:number[], up:number[]}} cam
 * @param {number[]} proj - 16-element column-major projection matrix
 * @returns {{x:number, y:number, z:number, behind:boolean}}
 */
export function projectToNdc(world, cam, proj) {
	// Camera basis (three.js: camera looks down -Z, +Z points eye←target).
	const z = norm(sub(cam.position, cam.target));
	const x = norm(cross(cam.up, z));
	const y = cross(z, x);

	// View transform: camera-space coords = Rᵀ (world - eye).
	const rel = sub(world, cam.position);
	const cx = dot(x, rel);
	const cy = dot(y, rel);
	const cz = dot(z, rel);

	// Apply the projection matrix (column-major elements) to [cx, cy, cz, 1].
	const clipX = proj[0] * cx + proj[4] * cy + proj[8] * cz + proj[12];
	const clipY = proj[1] * cx + proj[5] * cy + proj[9] * cz + proj[13];
	const clipZ = proj[2] * cx + proj[6] * cy + proj[10] * cz + proj[14];
	const clipW = proj[3] * cx + proj[7] * cy + proj[11] * cz + proj[15];

	const w = clipW === 0 ? 1e-12 : clipW;
	return { x: clipX / w, y: clipY / w, z: clipZ / w, behind: w < 0 };
}

/**
 * Map a world point to absolute screen pixels (for page.mouse.move/click).
 * @param {import('@playwright/test').Page} page
 * @param {number[]} world - [x, y, z]
 * @returns {Promise<{x:number, y:number, behind:boolean, ndc:{x:number,y:number}}>}
 */
export async function worldToScreen(page, world) {
	const { cam, proj } = await page.evaluate(() => ({
		cam: window.__waffle.getCameraState(),
		proj: window.__waffle.viewportDebug().camera.projectionMatrix,
	}));
	if (!cam || !proj) throw new Error('worldToScreen: camera state unavailable');

	const bounds = await getCanvasBounds(page);
	if (!bounds) throw new Error('worldToScreen: canvas not visible');

	const ndc = projectToNdc(world, cam, proj);
	return {
		x: bounds.x + (ndc.x * 0.5 + 0.5) * bounds.width,
		y: bounds.y + (-ndc.y * 0.5 + 0.5) * bounds.height,
		behind: ndc.behind,
		ndc: { x: ndc.x, y: ndc.y },
	};
}

/**
 * Move the real mouse to a world point.
 * @param {import('@playwright/test').Page} page
 * @param {number[]} world
 */
export async function moveToWorld(page, world) {
	const s = await worldToScreen(page, world);
	await page.mouse.move(s.x, s.y);
	await page.waitForTimeout(60);
	return s;
}

/**
 * Real-click at a world point.
 * @param {import('@playwright/test').Page} page
 * @param {number[]} world
 */
export async function clickWorld(page, world) {
	const s = await worldToScreen(page, world);
	await page.mouse.click(s.x, s.y);
	await page.waitForTimeout(120);
	return s;
}
