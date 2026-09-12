/**
 * Size readout while drawing, and the diameter dimension UI:
 *  - dragging a circle out shows "⌀ <value> mm" beside the cursor
 *  - a line shows its length + angle; a rectangle W × H
 *  - the Dimension tool on a circle opens a popup edited as a DIAMETER with
 *    a ⌀ prefix and the unit shown, and stores a Diameter constraint
 *  - hovering a point with the Dimension tool marks it hovered
 */
import { test, expect } from './helpers/waffle-test.js';
import { clickSketch, clickCircle, clickLine, clickRectangle, clickDimension } from './helpers/toolbar.js';
import { clickAt, moveTo, drawCircle, getCanvasBounds } from './helpers/canvas.js';
import { waitForEntityCount, getEntities } from './helpers/state.js';

const READOUT = '[data-testid="sketch-draw-readout"]';

test.describe('sketch draw readout', () => {
	test.beforeEach(async ({ waffle }) => {
		await clickSketch(waffle.page);
	});

	test('circle readout shows diameter with unit', async ({ waffle }) => {
		const page = waffle.page;
		await clickCircle(page);
		await clickAt(page, 0, 0);
		await moveTo(page, 80, 0);
		await expect(page.locator(READOUT)).toBeVisible();
		const text = await page.locator(READOUT).textContent();
		expect(text).toMatch(/^⌀ \d+\.\d+ mm$/);
		const dia = parseFloat(text.replace('⌀', ''));
		// 80 px of radius at the default sketch zoom is a few tens of mm.
		expect(dia).toBeGreaterThan(1);
		await clickAt(page, 80, 0);
		await waitForEntityCount(page, 2, 5000);
		const circle = (await getEntities(page)).find((e) => e.type === 'Circle');
		expect(circle.radius * 2 * 1000).toBeCloseTo(dia, 1);
		await expect(page.locator(READOUT)).not.toBeVisible();
	});

	test('line and rectangle readouts', async ({ waffle }) => {
		const page = waffle.page;
		await clickLine(page);
		await clickAt(page, -50, 0);
		await moveTo(page, 50, 0);
		await expect(page.locator(READOUT)).toHaveText(/mm\s+∠ (0\.0|360\.0)°$/);
		await page.keyboard.press('Escape');
		await clickRectangle(page);
		await clickAt(page, -60, -40);
		await moveTo(page, 60, 40);
		await expect(page.locator(READOUT)).toHaveText(/^\d+\.\d+ mm × \d+\.\d+ mm$/);
	});

	test('dimension tool on a circle edits the diameter with prefix and unit', async ({ waffle }) => {
		const page = waffle.page;
		await clickCircle(page);
		await drawCircle(page, 0, 0, 70, 0);
		await waitForEntityCount(page, 2, 5000);
		const circle = (await getEntities(page)).find((e) => e.type === 'Circle');

		await clickDimension(page);
		await clickAt(page, 70, 0); // on the circle's rim
		const input = page.locator('.dimension-input');
		await expect(input).toBeVisible();
		await expect(page.locator('[data-testid="dimension-input-prefix"]')).toHaveText('⌀');
		await expect(page.locator('[data-testid="dimension-input-unit"]')).toHaveText('mm');
		const shown = parseFloat(await input.inputValue());
		expect(shown).toBeCloseTo(circle.radius * 2 * 1000, 2);

		await input.fill('50');
		await input.press('Enter');
		await page.waitForFunction(() => window.__waffle.getConstraints().some((c) => c.type === 'Diameter'));
		const c = (await page.evaluate(() => window.__waffle.getConstraints())).find((c) => c.type === 'Diameter');
		expect(c.value).toBeCloseTo(0.05, 9);
		// The label reads as a diameter.
		await expect(page.locator('text=/⌀ 50\\.00 mm/')).toBeVisible();
	});

	test('dimension tool hover highlights a point', async ({ waffle }) => {
		const page = waffle.page;
		await clickLine(page);
		await clickAt(page, -80, 0);
		await clickAt(page, 80, 0);
		await page.keyboard.press('Escape');
		await waitForEntityCount(page, 3, 5000);
		const ents = await getEntities(page);
		const line = ents.find((e) => e.type === 'Line');
		await clickDimension(page);
		await moveTo(page, 80, 0);
		const hover = await page.evaluate(() => window.__waffle.getSketchHover());
		expect(hover).toBe(line.end_id);
		await moveTo(page, 0, 120);
		expect(await page.evaluate(() => window.__waffle.getSketchHover())).toBeNull();
		const b = await getCanvasBounds(page);
		expect(b).toBeTruthy();
	});
});
