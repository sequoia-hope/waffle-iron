/**
 * Chain select — double-click on a sketch entity selects its whole connected
 * chain (entities sharing endpoints, transitively). Shift+double-click unions
 * the chain into the existing selection. See /specs/sketch_chain_offset.md
 * branch table rows 1-3.
 */
import { test, expect } from './helpers/waffle-test.js';
import { clickSketch } from './helpers/toolbar.js';
import { clickAt } from './helpers/canvas.js';
import { getEntities } from './helpers/state.js';

const getSelection = (page) => page.evaluate(() => window.__waffle.getSketchSelection());
const setTool = (page, t) => page.evaluate((tool) => window.__waffle.setTool(tool), t);

/** Draw a 3-segment open polyline (staircase) with the line tool's chaining. */
async function drawStaircase(page) {
	await setTool(page, 'line');
	await clickAt(page, -120, 60);
	await clickAt(page, -60, 60);
	await clickAt(page, -60, 0);
	await clickAt(page, 0, 0);
	await page.keyboard.press('Escape');
	await page.waitForTimeout(300);
}

test.describe('chain select', () => {
	test.beforeEach(async ({ waffle }) => {
		await clickSketch(waffle.page);
	});

	test('double-click selects the connected chain, not unconnected geometry', async ({ waffle }) => {
		const page = waffle.page;
		await drawStaircase(page);

		// A separate, unconnected line far from the staircase.
		await setTool(page, 'select');
		await setTool(page, 'line');
		await clickAt(page, 100, -80);
		await clickAt(page, 160, -80);
		await page.keyboard.press('Escape');
		await page.waitForTimeout(300);

		const entities = await getEntities(page);
		const lines = entities.filter((e) => e.type === 'Line');
		expect(lines.length).toBe(4);

		// Double-click the middle staircase segment (vertical at x=-60).
		await setTool(page, 'select');
		await clickAt(page, -60, 30);
		await clickAt(page, -60, 30);

		const selection = await getSelection(page);
		const selectedLines = entities.filter(
			(e) => e.type === 'Line' && selection.includes(e.id)
		);
		expect(selectedLines.length).toBe(3);

		// The unconnected line stays unselected.
		const lonely = lines.find((e) => !selectedLines.some((s) => s.id === e.id));
		expect(selection).not.toContain(lonely.id);

		// Pure query agrees with the gesture (same connectivity walk).
		const chain = await page.evaluate(
			(seed) => window.__waffle.findConnectedChain(seed),
			selectedLines[0].id
		);
		expect(chain.sort()).toEqual(selectedLines.map((e) => e.id).sort());
	});

	test('shift+double-click unions a second chain into the selection', async ({ waffle }) => {
		const page = waffle.page;
		await drawStaircase(page);

		await setTool(page, 'select');
		await setTool(page, 'line');
		await clickAt(page, 100, -80);
		await clickAt(page, 160, -80);
		await page.keyboard.press('Escape');
		await page.waitForTimeout(300);

		// Chain-select the staircase, then shift+double-click the lone line.
		await setTool(page, 'select');
		await clickAt(page, -60, 30);
		await clickAt(page, -60, 30);
		await page.keyboard.down('Shift');
		await clickAt(page, 130, -80);
		await clickAt(page, 130, -80);
		await page.keyboard.up('Shift');

		const selection = await getSelection(page);
		const entities = await getEntities(page);
		const selectedLines = entities.filter(
			(e) => e.type === 'Line' && selection.includes(e.id)
		);
		expect(selectedLines.length).toBe(4);
	});

	test('double-click on a circle selects just the circle', async ({ waffle }) => {
		const page = waffle.page;
		await setTool(page, 'circle');
		await clickAt(page, 0, 0);
		await clickAt(page, 50, 0);
		await page.waitForTimeout(300);

		await setTool(page, 'select');
		// Click the circle's rim (radius 50px from center).
		await clickAt(page, 50, 0);
		await clickAt(page, 50, 0);

		const selection = await getSelection(page);
		const entities = await getEntities(page);
		const circle = entities.find((e) => e.type === 'Circle');
		expect(selection).toContain(circle.id);
		const selectedNonPoint = entities.filter(
			(e) => e.type !== 'Point' && selection.includes(e.id)
		);
		expect(selectedNonPoint.length).toBe(1);
	});
});
