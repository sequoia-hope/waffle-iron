/**
 * Explicit body-geometry chains for the Project + Offset tools (task #140).
 *
 * - Hovering a body EDGE ghosts the connected coplanar edge chain and says
 *   so in the status bar; clicking projects the WHOLE chain (Alt: single
 *   edge). Offset additionally arms on the projected chain.
 * - Tool-first FACE clicks work: CadModel.handleClick (the only reliable
 *   face resolution point) delegates to the active project/offset tool.
 *
 * Uses the worldToScreen infrastructure from projection-select-first.spec.js.
 */
import { test, expect } from './helpers/waffle-test.js';
import { clickSketch, clickRectangle, clickFinishSketch, clickExtrude } from './helpers/toolbar.js';
import { drawRectangle } from './helpers/canvas.js';
import { waitForEntityCount, waitForFeatureCount } from './helpers/state.js';
import { worldToScreen } from './helpers/worldToScreen.js';

const entities = (page) => page.evaluate(() => window.__waffle.getEntities());
const setTool = (page, t) => page.evaluate((tool) => window.__waffle.setTool(tool), t);
const statusText = (page) => page.locator('[data-testid="status-message"]').textContent();

/** Deterministic pre-hover at a world point (see projection-select-first). */
async function settleHoverAtWorld(page, world) {
	const s = await worldToScreen(page, world);
	await page.mouse.move(s.x - 15, s.y - 15);
	await page.waitForTimeout(40);
	await page.mouse.move(s.x, s.y);
	await page.waitForTimeout(150);
	return s;
}

/** Box on the front plane, then a fresh sketch on the front plane. */
async function buildBoxAndReSketch(waffle) {
	const page = waffle.page;
	await clickSketch(page, 'front');
	await clickRectangle(page);
	await drawRectangle(page, -80, -60, 80, 60);
	await waitForEntityCount(page, 8, 5000);
	await clickFinishSketch(page);
	await waitForFeatureCount(page, 1, 10000);
	await clickExtrude(page);
	const depth = page.locator('[data-testid="extrude-depth"]');
	if (await depth.isVisible()) await depth.fill('20');
	await page.locator('[data-testid="extrude-apply"]').click();
	await expect(page.locator('[data-testid="extrude-dialog"]')).not.toBeVisible();
	await waitForFeatureCount(page, 2, 10000);

	const aabb = await page.evaluate(() => window.__waffle.getMeshBoundingBox());
	expect(aabb).not.toBeNull();
	await clickSketch(page, 'front');

	const [mnx, mny, mnz] = aabb.min;
	const [mxx, mxy, mxz] = aabb.max;
	return {
		// Midpoint of the top-front edge — part of the front face's 4-edge rim.
		edgeMidWorld: [(mnx + mxx) / 2, mxy, mxz],
		faceCenterWorld: [(mnx + mxx) / 2, (mny + mxy) / 2, mxz],
	};
}

test.describe('project/offset on body geometry — explicit chains', () => {
	test('project tool: edge hover ghosts the closed 4-edge loop, click projects it all', async ({ waffle }) => {
		const page = waffle.page;
		const t = await buildBoxAndReSketch(waffle);
		await setTool(page, 'project');

		const before = await entities(page);
		const s = await settleHoverAtWorld(page, t.edgeMidWorld);

		// Explicit: the status bar says exactly what the click will do.
		const hint = await statusText(page);
		expect(hint).toContain('closed loop of 4 edges');
		expect(hint).toContain('Alt-click');

		await page.mouse.click(s.x, s.y);
		await page.waitForTimeout(300);

		const after = await entities(page);
		const newLines = after.filter(
			(e) => e.type === 'Line' && !before.some((b) => b.id === e.id)
		);
		expect(newLines.length).toBe(4);
		expect(newLines.every((e) => e.construction)).toBe(true);

		// The projected boundary is one connected chain of 4.
		const chain = await page.evaluate(
			(seed) => window.__waffle.findConnectedChain(seed),
			newLines[0].id
		);
		expect(chain.length).toBe(4);
	});

	test('project tool: Alt-click projects the single hovered edge only', async ({ waffle }) => {
		const page = waffle.page;
		const t = await buildBoxAndReSketch(waffle);
		await setTool(page, 'project');

		const before = await entities(page);
		const s = await settleHoverAtWorld(page, t.edgeMidWorld);
		await page.keyboard.down('Alt');
		await page.mouse.move(s.x + 1, s.y); // refresh hover state under Alt
		await page.waitForTimeout(100);
		await page.mouse.click(s.x + 1, s.y);
		await page.keyboard.up('Alt');
		await page.waitForTimeout(300);

		const after = await entities(page);
		const newLines = after.filter(
			(e) => e.type === 'Line' && !before.some((b) => b.id === e.id)
		);
		expect(newLines.length).toBe(1);
	});

	test('project tool: tool-first FACE click projects the face boundary', async ({ waffle }) => {
		const page = waffle.page;
		const t = await buildBoxAndReSketch(waffle);
		await setTool(page, 'project');

		const before = await entities(page);
		const s = await settleHoverAtWorld(page, t.faceCenterWorld);

		// Hover ghost + hint for the face boundary.
		expect(await statusText(page)).toContain('face boundary');

		await page.mouse.click(s.x, s.y);
		await page.waitForTimeout(400);

		const after = await entities(page);
		const newLines = after.filter(
			(e) => e.type === 'Line' && !before.some((b) => b.id === e.id)
		);
		expect(newLines.length).toBe(4);
		expect(newLines.every((e) => e.construction)).toBe(true);

		// The click was consumed by projection — no body face got SELECTED.
		const sel = await page.evaluate(() => window.__waffle.getSelectedRefs());
		expect(sel.length).toBe(0);
	});

	test('offset tool: body edge click projects the loop and arms; commit makes a real ring', async ({ waffle }) => {
		const page = waffle.page;
		const t = await buildBoxAndReSketch(waffle);
		await setTool(page, 'offset');

		const before = await entities(page);
		const s = await settleHoverAtWorld(page, t.edgeMidWorld);
		expect(await statusText(page)).toContain('closed loop of 4 edges');

		await page.mouse.click(s.x, s.y);
		await page.waitForTimeout(400);

		const armed = await page.evaluate(() => window.__waffle.getOffsetToolState());
		expect(armed.armed).toBe(true);
		expect(armed.closed).toBe(true);
		expect(armed.segmentCount).toBe(4);

		// Pull outward perpendicular to the hovered edge and commit 5 mm.
		// The front view renders with up=[1,0,0], so the box's world-top edge
		// draws VERTICALLY on screen left of center — outward = screen-left.
		await page.mouse.move(s.x - 60, s.y, { steps: 3 });
		await page.waitForTimeout(150);
		await page.mouse.click(s.x - 60, s.y);
		const input = page.locator('.dimension-input');
		await expect(input).toBeVisible({ timeout: 3000 });
		await input.fill('5');
		await page.keyboard.press('Enter');
		await page.waitForTimeout(300);

		const after = await entities(page);
		const newReal = after.filter(
			(e) => !e.construction && (e.type === 'Line' || e.type === 'Arc') && !before.some((b) => b.id === e.id)
		);
		// Outward rectangle offset: 4 lines + 4 corner arcs, all real.
		expect(newReal.filter((e) => e.type === 'Line').length).toBe(4);
		expect(newReal.filter((e) => e.type === 'Arc').length).toBe(4);
	});

	test('offset tool: tool-first FACE click projects the boundary and arms', async ({ waffle }) => {
		const page = waffle.page;
		const t = await buildBoxAndReSketch(waffle);
		await setTool(page, 'offset');

		const s = await settleHoverAtWorld(page, t.faceCenterWorld);
		expect(await statusText(page)).toContain('face boundary');
		await page.mouse.click(s.x, s.y);
		await page.waitForTimeout(400);

		const armed = await page.evaluate(() => window.__waffle.getOffsetToolState());
		expect(armed.armed).toBe(true);
		expect(armed.closed).toBe(true);
		expect(armed.segmentCount).toBe(4);
	});
});
