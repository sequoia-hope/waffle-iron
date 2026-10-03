/**
 * Trim tool tests.
 *
 * Verifies trim tool can detect intersections, highlight segments,
 * and split/remove entities at intersection points.
 */
import { test, expect } from './helpers/waffle-test.js';
import { clickSketch, clickLine, clickTool, isToolOffered } from './helpers/toolbar.js';
import { clickAt, drawLine } from './helpers/canvas.js';
import { getEntities, getEntityCountByType, waitForEntityCount } from './helpers/state.js';

test.describe('sketch trim tool', () => {
	test.beforeEach(async ({ waffle }) => {
		await clickSketch(waffle.page);
	});

	test('trim tool activates via toolbar button', async ({ waffle }) => {
		const page = waffle.page;

		expect(await isToolOffered(page, 'trim')).toBe(true);

		await clickTool(page, 'trim');
		await page.waitForFunction(
			() => window.__waffle?.getState()?.activeTool === 'trim',
			{ timeout: 3000 }
		);

		const activeTool = await page.evaluate(
			() => window.__waffle.getState().activeTool
		);
		expect(activeTool).toBe('trim');
	});

	test('trim tool clicks on crossing lines changes entity structure', async ({ waffle }) => {
		const page = waffle.page;

		// Draw two crossing lines forming an X using the API for precise positioning
		await page.evaluate(() => {
			// Line 1: (-5, -5) to (5, 5)
			const p1 = window.__waffle.addSketchEntity({ type: 'Point', x: -5, y: -5, construction: false });
			const p2 = window.__waffle.addSketchEntity({ type: 'Point', x: 5, y: 5, construction: false });
			window.__waffle.addSketchEntity({ type: 'Line', start_id: p1, end_id: p2 });
			// Line 2: (-5, 5) to (5, -5)
			const p3 = window.__waffle.addSketchEntity({ type: 'Point', x: -5, y: 5, construction: false });
			const p4 = window.__waffle.addSketchEntity({ type: 'Point', x: 5, y: -5, construction: false });
			window.__waffle.addSketchEntity({ type: 'Line', start_id: p3, end_id: p4 });
		});
		await page.waitForTimeout(500);

		const entitiesBefore = await getEntities(page);
		const lineCountBefore = entitiesBefore.filter(e => e.type === 'Line').length;
		const totalCountBefore = entitiesBefore.length;
		expect(lineCountBefore).toBe(2);

		// Activate trim tool
		await clickTool(page, 'trim');
		await page.waitForFunction(
			() => window.__waffle?.getState()?.activeTool === 'trim',
			{ timeout: 3000 }
		);

		// Click near the upper-right segment of line 1 (between intersection and endpoint)
		// The intersection is at (0,0), so clicking near (3,3) should be in the upper-right segment
		await clickAt(page, 30, -30);
		await page.waitForTimeout(500);

		// Trimming the upper-right piece of line 1 cuts it at the (0, 0)
		// crossing and drops that piece: the lower-left half survives, so the
		// (5, 5) endpoint goes and a new vertex appears on the crossing. The
		// entity COUNT is unchanged by that (one point out, one point in),
		// which is why this asserts the geometry instead.
		//
		// This assertion used to be `expect(totalCountAfter)
		// .toBeGreaterThanOrEqual(0)` — tautologically true. It passed while
		// the fixture was not reaching the engine at all (`addSketchEntity`
		// returned no id, so every line here was built on `undefined`
		// endpoints and the engine refused it), so the trim tool had never
		// actually been exercised by this spec.
		const entitiesAfter = await getEntities(page);
		const pointsAfter = entitiesAfter.filter((e) => e.type === 'Point');
		const near = (e, x, y) => Math.hypot(e.x - x, e.y - y) < 0.3;

		expect(
			pointsAfter.some((e) => near(e, 0, 0)),
			`a vertex lands on the crossing: ${JSON.stringify(pointsAfter)}`
		).toBe(true);
		expect(
			pointsAfter.some((e) => near(e, 5, 5)),
			`the trimmed-away (5, 5) endpoint is gone: ${JSON.stringify(pointsAfter)}`
		).toBe(false);
		// Both lines are still lines: the crossing line is untouched and the
		// trimmed one was shortened, not deleted.
		expect(entitiesAfter.filter((e) => e.type === 'Line').length).toBe(2);
	});

	test('trim tool does not crash on click with no nearby entities', async ({ waffle }) => {
		const page = waffle.page;

		// Draw a single line far from where we'll click
		await page.evaluate(() => {
			const p1 = window.__waffle.addSketchEntity({ type: 'Point', x: -10, y: -10, construction: false });
			const p2 = window.__waffle.addSketchEntity({ type: 'Point', x: -8, y: -8, construction: false });
			window.__waffle.addSketchEntity({ type: 'Line', start_id: p1, end_id: p2 });
		});
		await page.waitForTimeout(300);

		const countBefore = await page.evaluate(
			() => window.__waffle.getState().entityCount
		);

		// Activate trim tool
		await clickTool(page, 'trim');
		await page.waitForFunction(
			() => window.__waffle?.getState()?.activeTool === 'trim',
			{ timeout: 3000 }
		);

		// Click far away from any entity (upper right corner of canvas)
		await clickAt(page, 200, -200);
		await page.waitForTimeout(500);

		// Entity count should not change — no entity near click
		const countAfter = await page.evaluate(
			() => window.__waffle.getState().entityCount
		);
		expect(countAfter).toBe(countBefore);
	});

	test('trim tool returns to idle state after trim operation', async ({ waffle }) => {
		const page = waffle.page;

		// Draw two crossing lines
		await clickLine(page);
		await drawLine(page, -80, -60, 80, 60);
		await waitForEntityCount(page, 3, 5000);
		await page.keyboard.press('Escape');
		await page.waitForTimeout(200);

		await clickLine(page);
		await drawLine(page, -80, 60, 80, -60);
		await waitForEntityCount(page, 6, 5000);
		await page.keyboard.press('Escape');
		await page.waitForTimeout(200);

		// Activate trim tool
		await clickTool(page, 'trim');
		await page.waitForFunction(
			() => window.__waffle?.getState()?.activeTool === 'trim',
			{ timeout: 3000 }
		);

		// Click at intersection
		await clickAt(page, 40, 30);
		await page.waitForTimeout(500);

		// Tool should still be trim (stays active for multiple trims)
		const activeTool = await page.evaluate(
			() => window.__waffle.getState().activeTool
		);
		expect(activeTool).toBe('trim');
	});
});
