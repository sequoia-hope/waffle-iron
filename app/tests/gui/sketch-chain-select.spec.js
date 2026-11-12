/**
 * Chain select — double-click on a sketch entity selects its whole connected
 * chain (entities sharing endpoints, transitively). Shift+double-click unions
 * the chain into the existing selection. See /specs/sketch_chain_offset.md
 * branch table rows 1-3.
 *
 * Every double-click here is ONE gesture (`doubleClickAt`). Two `clickAt`
 * calls are not: each re-measures the canvas and sleeps 150 ms, and with the
 * suite running in parallel the gap between the two presses passes the app's
 * 400 ms double-click window (`handleSelectTool` in tools.js). The second
 * press then reads as a plain click — selecting one line instead of the
 * chain, or, with Shift held, toggling the just-added line back OUT (3
 * selected instead of 4).
 */
import { test, expect } from './helpers/waffle-test.js';
import { clickSketch } from './helpers/toolbar.js';
import { clickAt, doubleClickAt } from './helpers/canvas.js';
import { getEntities } from './helpers/state.js';

const getSelection = (page) => page.evaluate(() => window.__waffle.getSketchSelection());
const setTool = (page, t) => page.evaluate((tool) => window.__waffle.setTool(tool), t);

/**
 * Wait for the chain walk to land. The double-click asks the engine for the
 * connected chain (`connectedChain`, a promise) and sets the selection when it
 * answers, so the selection is not final when the gesture returns. Throws on
 * timeout — that IS the failure.
 */
async function waitForSelectedLines(page, n) {
	await page.waitForFunction(
		(want) => {
			const sel = window.__waffle.getSketchSelection();
			const lines = window.__waffle.getEntities().filter((e) => e.type === 'Line' && sel.includes(e.id));
			return lines.length === want;
		},
		n,
		{ timeout: 5000 }
	);
}

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
		await doubleClickAt(page, -60, 30);
		await waitForSelectedLines(page, 3);

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
		await doubleClickAt(page, -60, 30);
		await waitForSelectedLines(page, 3);
		await page.keyboard.down('Shift');
		await doubleClickAt(page, 130, -80);
		await page.keyboard.up('Shift');
		await waitForSelectedLines(page, 4);

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
		// Double-click the circle's rim (radius 50px from center).
		await doubleClickAt(page, 50, 0);

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
