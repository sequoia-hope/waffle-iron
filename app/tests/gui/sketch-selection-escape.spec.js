/**
 * Sketch selection gestures:
 *  - body picks (vertex / edge / face; edges exercised here) ACCUMULATE without Shift; clicking a
 *    picked one again drops just that one; empty space or Escape drops all;
 *  - Escape never leaves the sketch — it backs out one level (tool → Select,
 *    then selection → empty), so repeated taps are harmless.
 * REAL POINTER EVENTS for every pick.
 */
import { test, expect } from './helpers/waffle-test.js';
import {
	clickSketch,
	clickRectangle,
	clickFinishSketch,
	clickExtrude,
	clickSelect,
} from './helpers/toolbar.js';
import { drawRectangle, clickAt, getCanvasBounds } from './helpers/canvas.js';
import {
	waitForEntityCount,
	waitForFeatureCount,
	getActiveTool,
	collectCrashErrors,
	expectNoAnyCrash,
} from './helpers/state.js';
import { moveToWorld } from './helpers/worldToScreen.js';

const selectedRefs = (page) => page.evaluate(() => window.__waffle.getSelectedRefs());
const resetHover = (page) => page.evaluate(() => window.__waffle.setHoveredRef(null));
const refKind = (r) => r?.kind?.type ?? null;
const finishBtn = (page) => page.locator('[data-testid="toolbar-btn-finish-sketch"]');

async function buildBoxAndReSketch(waffle) {
	const page = waffle.page;
	await clickSketch(page, 'front');
	await clickRectangle(page);
	await drawRectangle(page, -80, -60, 80, 60);
	await waitForEntityCount(page, 8, 5000);
	await clickFinishSketch(page);
	await waitForFeatureCount(page, 1, 10000);
	await clickExtrude(page);
	await page.locator('[data-testid="extrude-apply"]').click();
	await expect(page.locator('[data-testid="extrude-dialog"]')).not.toBeVisible();
	await waitForFeatureCount(page, 2, 10000);
	const aabb = await page.evaluate(() => window.__waffle.getMeshBoundingBox());
	await clickSketch(page, 'front');
	const [mnx, mny, mnz] = aabb.min;
	const [mxx, mxy, mxz] = aabb.max;
	return {
		topEdgeWorld: [(mnx + mxx) / 2, mxy, mxz],
		rightEdgeWorld: [mxx, (mny + mxy) / 2, mxz],
	};
}

/**
 * Plain (no Shift) click on a body edge: re-hover first and WAIT until the
 * edge is the hovered ref, so a miss is reported as a hover failure rather
 * than looking like an empty-space click (which, correctly, clears).
 */
async function pick(page, world) {
	await resetHover(page);
	const s = await moveToWorld(page, world);
	await expect
		.poll(async () => {
			await page.mouse.move(s.x + 1, s.y);
			await page.mouse.move(s.x, s.y);
			return (await page.evaluate(() => window.__waffle.getHoveredRef()))?.kind?.type ?? null;
		}, { timeout: 3000, message: `edge hovered at ${JSON.stringify(world)}` })
		.toBe('Edge');
	await page.mouse.click(s.x, s.y);
	await page.waitForTimeout(150);
}

test.describe('sketch selection and Escape', () => {
	test('body picks accumulate without Shift; repeat drops one; empty space and Escape drop all', async ({ waffle }) => {
		const page = waffle.page;
		const crashes = collectCrashErrors(page);
		const t = await buildBoxAndReSketch(waffle);
		await clickSelect(page);

		await pick(page, t.topEdgeWorld);
		await pick(page, t.rightEdgeWorld);
		let sel = await selectedRefs(page);
		expect(sel.map(refKind), 'two plain clicks on two edges, two picks').toEqual(['Edge', 'Edge']);
		expect(JSON.stringify(sel[0]), 'two distinct edges').not.toBe(JSON.stringify(sel[1]));

		// Clicking a picked edge again drops just that edge.
		await pick(page, t.rightEdgeWorld);
		sel = await selectedRefs(page);
		expect(sel.length, 'the repeat click dropped one').toBe(1);
		await pick(page, t.rightEdgeWorld);
		expect((await selectedRefs(page)).length, 'and a third click adds it back').toBe(2);

		// Escape drops everything and stays in the sketch.
		await page.keyboard.press('Escape');
		await page.waitForTimeout(150);
		expect((await selectedRefs(page)).length).toBe(0);
		await expect(finishBtn(page)).toBeVisible();

		// Empty space drops everything too.
		await pick(page, t.topEdgeWorld);
		await pick(page, t.rightEdgeWorld);
		expect((await selectedRefs(page)).length).toBe(2);
		const bounds = await getCanvasBounds(page);
		await resetHover(page);
		await page.mouse.move(bounds.x + 30, bounds.y + bounds.height - 30);
		await page.waitForTimeout(120);
		await page.mouse.click(bounds.x + 30, bounds.y + bounds.height - 30);
		await page.waitForTimeout(150);
		expect((await selectedRefs(page)).length, 'empty-space click clears').toBe(0);

		expectNoAnyCrash(crashes);
	});

	test('Escape tapped repeatedly never leaves the sketch', async ({ waffle }) => {
		const page = waffle.page;
		await clickSketch(page, 'front', { tool: null });
		expect(await getActiveTool(page), 'a new sketch starts on Select').toBe('select');
		await page.evaluate(() => window.__waffle.setTool('rectangle'));
		await clickAt(page, -40, -30);
		await clickAt(page, 40, 30);
		await waitForEntityCount(page, 8, 5000);

		for (let i = 0; i < 4; i++) {
			await page.keyboard.press('Escape');
			await page.waitForTimeout(100);
		}
		expect(await getActiveTool(page)).toBe('select');
		await expect(finishBtn(page), 'still sketching after four Escapes').toBeVisible();
		await waitForEntityCount(page, 8, 2000);
	});
});
