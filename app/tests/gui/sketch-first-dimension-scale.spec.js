/**
 * The FIRST driving length dimension on an undimensioned sketch scales the
 * whole sketch proportionally (about the origin); later dimensions do not.
 * Undo reverts the scale (positions AND circle radii); the setting turns it
 * off.
 */
import { test, expect } from './helpers/waffle-test.js';
import { clickSketch, clickRectangle, clickCircle } from './helpers/toolbar.js';
import { drawRectangle, drawCircle } from './helpers/canvas.js';
import { waitForEntityCount, getEntities } from './helpers/state.js';

async function snapshot(page) {
	const [entities, positions] = await Promise.all([
		getEntities(page),
		// getPositions() is a Map — serialize it explicitly for the round-trip.
		page.evaluate(() => [...window.__waffle.getPositions()].map(([id, p]) => [id, { x: p.x, y: p.y }])),
	]);
	return { entities, pos: new Map(positions) };
}

function lineLength(snap, line) {
	const a = snap.pos.get(line.start_id), b = snap.pos.get(line.end_id);
	return Math.hypot(b.x - a.x, b.y - a.y);
}

async function applyLineDimension(page, line, value) {
	await page.evaluate(({ id, v }) => {
		window.__waffle.showDimensionPopup({ entityA: id, entityB: null, sketchX: 0, sketchY: 0, dimType: 'distance', defaultValue: v });
		window.__waffle.applyDimensionFromPopup(v);
	}, { id: line.id, v: value });
	await page.waitForTimeout(600);
}

async function drawFixture(page) {
	await clickSketch(page);
	await clickRectangle(page);
	await drawRectangle(page, -120, -80, 40, 60);
	await waitForEntityCount(page, 8, 5000);
	await clickCircle(page);
	await drawCircle(page, 120, 0, 150, 0);
	await waitForEntityCount(page, 10, 5000);
}

test.describe('first dimension scales the sketch', () => {
	test('first length dimension scales every point and radius; the second does not', async ({ waffle }) => {
		const page = waffle.page;
		await drawFixture(page);
		const before = await snapshot(page);
		const line = before.entities.find((e) => e.type === 'Line');
		const circle = before.entities.find((e) => e.type === 'Circle');
		const len0 = lineLength(before, line);
		expect(len0).toBeGreaterThan(0);

		await applyLineDimension(page, line, len0 * 2);
		const after = await snapshot(page);
		for (const [id, p0] of before.pos) {
			const p1 = after.pos.get(id);
			expect(p1.x).toBeCloseTo(p0.x * 2, 8);
			expect(p1.y).toBeCloseTo(p0.y * 2, 8);
		}
		const circleAfter = after.entities.find((e) => e.id === circle.id);
		expect(circleAfter.radius).toBeCloseTo(circle.radius * 2, 10);
		expect(lineLength(after, line)).toBeCloseTo(len0 * 2, 8);

		// A second dimension on another line only moves what it constrains.
		const other = after.entities.find((e) => e.type === 'Line' && e.id !== line.id
			&& Math.abs(lineLength(after, e) - lineLength(after, line)) > 1e-9);
		const otherLen = lineLength(after, other);
		await applyLineDimension(page, other, otherLen * 1.5);
		const third = await snapshot(page);
		const c3 = third.entities.find((e) => e.id === circle.id);
		expect(c3.radius).toBeCloseTo(circleAfter.radius, 10);
		expect(third.pos.get(circle.center_id).x).toBeCloseTo(after.pos.get(circle.center_id).x, 8);

		// Undo both: back to the drawn geometry, radius included.
		await page.keyboard.press('Control+z');
		await page.waitForTimeout(300);
		await page.keyboard.press('Control+z');
		await page.waitForTimeout(600);
		const undone = await snapshot(page);
		for (const [id, p0] of before.pos) {
			const p1 = undone.pos.get(id);
			expect(p1.x).toBeCloseTo(p0.x, 8);
			expect(p1.y).toBeCloseTo(p0.y, 8);
		}
		expect(undone.entities.find((e) => e.id === circle.id).radius).toBeCloseTo(circle.radius, 10);
	});

	test('disabled by the setting: only the dimensioned line changes', async ({ waffle }) => {
		const page = waffle.page;
		await page.evaluate(() => window.__waffle.updateSettings({ sketchScaleOnFirstDimension: false }));
		await drawFixture(page);
		const before = await snapshot(page);
		const line = before.entities.find((e) => e.type === 'Line');
		const circle = before.entities.find((e) => e.type === 'Circle');
		await applyLineDimension(page, line, lineLength(before, line) * 2);
		const after = await snapshot(page);
		expect(after.entities.find((e) => e.id === circle.id).radius).toBeCloseTo(circle.radius, 10);
		expect(lineLength(after, line)).toBeCloseTo(lineLength(before, line) * 2, 6);
	});
});
