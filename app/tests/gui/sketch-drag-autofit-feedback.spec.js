/**
 * Regression: the drag ↔ auto-fit camera feedback loop (2026-07-05).
 *
 * Dragging a corner outward grows the sketch; when the extent crossed 80% of
 * the visible range the growth-gated auto-fit zoomed out MID-DRAG, which
 * rescaled the pointer→sketch mapping, which teleported the drag target
 * outward, which grew the sketch again — an exponential runaway (~1.2x per
 * frame) that blew a 26mm sketch to meters while the solver stayed perfectly
 * stable (geometry arrives intact, just enormous). Reproduced with the
 * user-supplied sketch.waffle.
 *
 * Invariant: the camera (and therefore the pointer→sketch mapping) must not
 * change while a drag gesture is active. Auto-fit may run on release.
 */
import { test, expect } from './helpers/waffle-test.js';
import { clickSelect } from './helpers/toolbar.js';
import { getCanvasBounds } from './helpers/canvas.js';
import { collectCrashErrors, expectNoAnyCrash } from './helpers/state.js';
import fs from 'fs';

const getPositions = (page) =>
	page.evaluate(() => Object.fromEntries(window.__waffle.getPositions()));
const getCamera = (page) => page.evaluate(() => window.__waffle.getCameraState());

test('auto-fit must not rescale the pointer mapping mid-drag', async ({ waffle }) => {
	test.setTimeout(120000);
	const page = waffle.page;
	const crashes = collectCrashErrors(page);

	// The user's actual document (fixtures/two-center-rects-equal.waffle): two origin-centered centerpoint rects,
	// Equal on the inner top+left edges, no center pin.
	const json = fs.readFileSync(new URL('./fixtures/two-center-rects-equal.waffle', import.meta.url), 'utf8');
	await page.evaluate(async (data) => {
		await window.__waffle.loadProject(data);
	}, json);
	await page.waitForTimeout(1000);
	const featureId = await page.evaluate(() => {
		const feats = window.__waffle.getFeatureTree()?.features ?? [];
		return feats.find((f) => f.operation?.type === 'Sketch')?.id ?? feats[0]?.id;
	});
	await page.evaluate((id) => window.__waffle.enterSketchEditMode(id), featureId);
	await page.waitForTimeout(1500);
	await clickSelect(page);

	const bounds = await getCanvasBounds(page);
	const cam0 = await getCamera(page);
	const pos0 = await getPositions(page);
	const extent0 = Math.max(
		...Object.values(pos0).flatMap((p) => [Math.abs(p.x), Math.abs(p.y)])
	);

	// Project p15 (upper-left inner corner) to the screen via the camera basis.
	const sub = (a, b) => [a[0] - b[0], a[1] - b[1], a[2] - b[2]];
	const cross = (a, b) => [
		a[1] * b[2] - a[2] * b[1],
		a[2] * b[0] - a[0] * b[2],
		a[0] * b[1] - a[1] * b[0],
	];
	const norm = (a) => {
		const l = Math.hypot(...a);
		return [a[0] / l, a[1] / l, a[2] / l];
	};
	const dot = (a, b) => a[0] * b[0] + a[1] * b[1] + a[2] * b[2];
	const zA = norm(sub(cam0.position, cam0.target));
	const xA = norm(cross(cam0.up, zA));
	const yA = cross(zA, xA);
	const ppu = bounds.height / (2 * cam0.frustumTop);
	const p15 = pos0['15'];
	const w = sub([p15.x, p15.y, 0], cam0.target);
	const sx = bounds.centerX + dot(w, xA) * ppu;
	const sy = bounds.centerY - dot(w, yA) * ppu;

	// Whip the corner across the full canvas in one gesture (the proven soak
	// trigger): wide horizontal excursions exceed the 80%-of-view auto-fit
	// threshold, and each mid-drag fit rescales the mapping → runaway.
	const stops = [
		[0.45, 0.1], [-0.45, -0.1], [0.45, 0.3], [-0.45, -0.3],
		[0.45, -0.2], [-0.45, 0.2], [0.45, 0.4], [-0.45, -0.4],
	];
	await page.mouse.move(sx, sy);
	await page.mouse.down();
	let frustumChangedMidDrag = false;
	for (const [fx, fy] of stops) {
		const nx = bounds.centerX + fx * bounds.width;
		const ny = bounds.centerY + fy * bounds.height;
		await page.mouse.move(nx, ny, { steps: 5 });
		await page.waitForTimeout(60);
		const camNow = await getCamera(page);
		if (Math.abs(camNow.frustumTop - cam0.frustumTop) > 1e-12) {
			frustumChangedMidDrag = true;
		}
	}
	await page.mouse.up();
	await page.waitForTimeout(500);

	const pos1 = await getPositions(page);
	const extent1 = Math.max(
		...Object.values(pos1).flatMap((p) => [Math.abs(p.x), Math.abs(p.y)])
	);

	// The camera must have been frozen for the whole gesture…
	expect(frustumChangedMidDrag, 'camera zoomed mid-drag (mapping feedback loop)').toBe(false);
	// …which bounds the geometry by what the drag-start view can express:
	// the mouse stayed inside the canvas, so nothing can exceed the visible
	// half-range (frustumTop * aspect) plus slack. Without the fix this blows
	// through 1 m within a single gesture.
	const visibleHalf = cam0.frustumTop * (bounds.width / bounds.height);
	expect(extent1).toBeLessThan(visibleHalf * 2);
	console.log(
		`extent ${extent0.toFixed(4)} -> ${extent1.toFixed(4)} m (visible half-range ${visibleHalf.toFixed(4)})`
	);

	expectNoAnyCrash(crashes);
});
