/**
 * Rubber-band box selection in the sketch select tool.
 *   left→right: entities fully inside the box
 *   right→left: entities the box touches (crossing)
 * Real pointer events; the selection is read back from the store.
 */
import { test, expect } from './helpers/waffle-test.js';
import { clickSketch, clickLine, clickCircle, clickSelect } from './helpers/toolbar.js';
import { drawLine, drawCircle, getCanvasBounds } from './helpers/canvas.js';
import { waitForEntityCount, getEntities } from './helpers/state.js';

async function boxDrag(page, x1, y1, x2, y2) {
	const b = await getCanvasBounds(page);
	await page.mouse.move(b.centerX + x1, b.centerY + y1);
	await page.mouse.down();
	const steps = 8;
	for (let i = 1; i <= steps; i++) {
		const t = i / steps;
		await page.mouse.move(b.centerX + x1 + (x2 - x1) * t, b.centerY + y1 + (y2 - y1) * t);
		await page.waitForTimeout(20);
	}
	await page.mouse.up();
	await page.waitForTimeout(150);
}

test.describe('sketch box select', () => {
	test.beforeEach(async ({ waffle }) => {
		const page = waffle.page;
		await clickSketch(page);
		await clickLine(page);
		await drawLine(page, -160, -60, -60, -60);
		await waitForEntityCount(page, 3, 5000);
		await page.keyboard.press('Escape');
		await clickCircle(page);
		await drawCircle(page, 100, 40, 130, 40);
		await waitForEntityCount(page, 5, 5000);
		await clickSelect(page);
	});

	test('left→right selects only what is fully inside', async ({ waffle }) => {
		const page = waffle.page;
		const ents = await getEntities(page);
		const line = ents.find((e) => e.type === 'Line');
		const circle = ents.find((e) => e.type === 'Circle');

		// A box around the line only.
		await boxDrag(page, -200, -110, -20, -10);
		let sel = await page.evaluate(() => window.__waffle.getSketchSelection());
		expect(sel).toContain(line.id);
		expect(sel).toContain(line.start_id);
		expect(sel).toContain(line.end_id);
		expect(sel).not.toContain(circle.id);

		// A box that clips the circle but does not enclose it selects nothing new.
		await boxDrag(page, 40, -10, 110, 100);
		sel = await page.evaluate(() => window.__waffle.getSketchSelection());
		expect(sel).not.toContain(circle.id);
		expect(sel).not.toContain(line.id);

		// A box around everything selects everything.
		await boxDrag(page, -220, -130, 200, 120);
		sel = await page.evaluate(() => window.__waffle.getSketchSelection());
		expect(sel).toContain(line.id);
		expect(sel).toContain(circle.id);
		expect(sel).toContain(circle.center_id);
		expect(sel.length).toBe(ents.length);
	});

	test('right→left is a crossing select', async ({ waffle }) => {
		const page = waffle.page;
		const ents = await getEntities(page);
		const line = ents.find((e) => e.type === 'Line');
		const circle = ents.find((e) => e.type === 'Circle');

		// Dragged from the right: clips the circle's edge and the line's end.
		await boxDrag(page, 110, 10, -80, -80);
		const sel = await page.evaluate(() => window.__waffle.getSketchSelection());
		expect(sel).toContain(circle.id);
		expect(sel).toContain(line.id);
		// The line's far start point (-160) lies outside the box.
		expect(sel).not.toContain(line.start_id);
	});

	test('shift extends a box selection; a box from empty space replaces it', async ({ waffle }) => {
		const page = waffle.page;
		const ents = await getEntities(page);
		const line = ents.find((e) => e.type === 'Line');
		const circle = ents.find((e) => e.type === 'Circle');

		await boxDrag(page, -200, -110, -20, -10);
		await page.keyboard.down('Shift');
		await boxDrag(page, 50, -10, 160, 90);
		await page.keyboard.up('Shift');
		let sel = await page.evaluate(() => window.__waffle.getSketchSelection());
		expect(sel).toContain(line.id);
		expect(sel).toContain(circle.id);

		await boxDrag(page, 50, -10, 160, 90);
		sel = await page.evaluate(() => window.__waffle.getSketchSelection());
		expect(sel).toContain(circle.id);
		expect(sel).not.toContain(line.id);
	});
});
