/**
 * Sketch undo: drag repositions are undoable, and undo restores the VIEWPORT
 * along with the geometry (feature request from the drag-explosion report:
 * the growth-gated auto-fit zooms out when geometry balloons, but nothing
 * ever zooms back in — undo must return the camera too).
 *
 * Drags go through __waffle.dragSketchPoint/finalizeDrag — the production
 * drag lifecycle (drawing uses real pointer events per GUI test rules).
 */
import { test, expect } from './helpers/waffle-test.js';
import { clickSketch, clickSelect, clickRectangle } from './helpers/toolbar.js';
import { drawRectangle } from './helpers/canvas.js';
import { waitForEntityCount, getEntities, collectCrashErrors, expectNoAnyCrash } from './helpers/state.js';

const getPositions = (page) =>
	page.evaluate(() => Object.fromEntries(window.__waffle.getPositions()));
const getCamera = (page) => page.evaluate(() => window.__waffle.getCameraState());

async function dragPointTo(page, pointId, x, y, steps = 6) {
	for (let i = 1; i <= steps; i++) {
		await page.evaluate(
			([id, px, py]) => window.__waffle.dragSketchPoint(id, px, py),
			[pointId, x * (i / steps), y * (i / steps)]
		);
		await page.waitForTimeout(80);
	}
	await page.evaluate(() => window.__waffle.finalizeDrag());
	await page.waitForTimeout(400);
}

test.describe('sketch undo: drag + viewport restore', () => {
	test.beforeEach(async ({ waffle }) => {
		await clickSketch(waffle.page);
	});

	test('a point drag is undoable and redoable as its own action', async ({ waffle }) => {
		const page = waffle.page;
		const crashes = collectCrashErrors(page);

		await clickRectangle(page);
		await drawRectangle(page, -60, -40, 60, 40);
		await waitForEntityCount(page, 8, 5000);
		await clickSelect(page);

		const entities = await getEntities(page);
		const corner = entities.filter((e) => e.type === 'Point')[2];
		const before = await getPositions(page);
		const c0 = before[corner.id];

		await dragPointTo(page, corner.id, c0.x * 2, c0.y * 2);
		const after = await getPositions(page);
		const c1 = after[corner.id];
		expect(Math.hypot(c1.x - c0.x, c1.y - c0.y)).toBeGreaterThan(Math.abs(c0.x) * 0.5);

		// Undo restores the pre-drag position (drags previously left no record).
		await page.evaluate(() => window.__waffle.undo());
		await page.waitForTimeout(400);
		const undone = await getPositions(page);
		expect(undone[corner.id].x).toBeCloseTo(c0.x, 6);
		expect(undone[corner.id].y).toBeCloseTo(c0.y, 6);

		// Redo re-applies the drag's end state.
		await page.evaluate(() => window.__waffle.redo());
		await page.waitForTimeout(400);
		const redone = await getPositions(page);
		expect(redone[corner.id].x).toBeCloseTo(c1.x, 6);
		expect(redone[corner.id].y).toBeCloseTo(c1.y, 6);

		expectNoAnyCrash(crashes);
	});

	test('undo restores the camera after an auto-fit zoom-out', async ({ waffle }) => {
		const page = waffle.page;
		const crashes = collectCrashErrors(page);

		await clickRectangle(page);
		await drawRectangle(page, -60, -40, 60, 40);
		await waitForEntityCount(page, 8, 5000);
		await clickSelect(page);

		const cam0 = await getCamera(page);
		expect(cam0).toBeTruthy();

		const entities = await getEntities(page);
		const corner = entities.filter((e) => e.type === 'Point')[2];
		const before = await getPositions(page);
		const c0 = before[corner.id];

		// Drag the corner far enough that the sketch outgrows the view and the
		// growth-gated auto-fit zooms out (premise-guarded below).
		await dragPointTo(page, corner.id, c0.x * 40, c0.y * 40, 10);
		const cam1 = await getCamera(page);
		const zoomedOut =
			(cam1.frustumTop ?? 0) > (cam0.frustumTop ?? 0) * 1.5 ||
			Math.hypot(
				cam1.position[0] - cam0.position[0],
				cam1.position[1] - cam0.position[1],
				cam1.position[2] - cam0.position[2]
			) > 1e-6;
		expect(zoomedOut, 'auto-fit should have moved the camera out').toBe(true);

		// Undo: geometry AND viewport must come back.
		await page.evaluate(() => window.__waffle.undo());
		await page.waitForTimeout(400);

		const undone = await getPositions(page);
		expect(undone[corner.id].x).toBeCloseTo(c0.x, 6);
		expect(undone[corner.id].y).toBeCloseTo(c0.y, 6);

		const cam2 = await getCamera(page);
		if (cam0.frustumTop != null) {
			expect(cam2.frustumTop).toBeCloseTo(cam0.frustumTop, 6);
		}
		for (let i = 0; i < 3; i++) {
			expect(cam2.position[i]).toBeCloseTo(cam0.position[i], 6);
			expect(cam2.target[i]).toBeCloseTo(cam0.target[i], 6);
		}

		expectNoAnyCrash(crashes);
	});
});
