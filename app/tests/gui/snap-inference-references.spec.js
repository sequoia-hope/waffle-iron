/**
 * Alignment inference against FIXED sources — the sketch origin and body
 * vertices — and the temporary nature of an armed source.
 * Contract: specs/snap_inference_and_priority.md §"Addendum 2026-10-31".
 *
 * REAL POINTER EVENTS ONLY (canvas.js clickAt/moveTo). Positions of the
 * origin and of body-vertex projections are read through
 * sketchToScreenOffset / getBodyVertexSnapPoints (coordinate discovery only).
 */
import { test, expect } from './helpers/waffle-test.js';
import { clickSketch } from './helpers/toolbar.js';
import { clickAt, moveTo } from './helpers/canvas.js';
import { createExtrudedBox } from './helpers/geometry.js';
import { getConstraints } from './helpers/constraint.js';

const indicator = (p) => p.evaluate(() => window.__waffle.getSnapIndicator());
const pxSize = (p) => p.evaluate(() => window.__waffle.getSketchPixelSize());
const screenOff = (p, sx, sy) => p.evaluate(({ sx, sy }) => window.__waffle.sketchToScreenOffset(sx, sy), { sx, sy });
const entities = (p) => p.evaluate(() => window.__waffle.getEntities());
const pointPos = async (p) =>
	(await entities(p)).filter((e) => e.type === 'Point').map((e) => ({ id: e.id, x: e.x, y: e.y, construction: !!e.construction }));
const cbt = async (p, t) => (await getConstraints(p)).filter((c) => c.type === t);

/**
 * The aligned point Q and the anchor its point-pair constraint names: the
 * anchor must be a construction point pinned (WhereDragged) at `at`.
 */
async function expectAnchoredAlignment(page, type, before, at) {
	const after = await pointPos(page);
	const q = after.find((p) => !before.some((b) => b.id === p.id) && !p.construction);
	expect(q, 'aligned point Q placed').toBeTruthy();
	const pair = (await cbt(page, type)).find((c) => c.point_b === q.id);
	expect(pair, `${type} names Q as point_b`).toBeTruthy();
	const anchor = after.find((p) => p.id === pair.point_a);
	expect(anchor?.construction, 'the anchor is a construction point').toBe(true);
	const pin = (await cbt(page, 'WhereDragged')).find((c) => c.point === anchor.id);
	expect(pin, 'the anchor is pinned').toBeTruthy();
	expect(pin.x).toBeCloseTo(at.x, 9);
	expect(pin.y).toBeCloseTo(at.y, 9);
	return q;
}

test.describe('snap inference — fixed sources and release', () => {
	test('R1: hovering the origin arms it; align-h pins Q to y = 0 through a construction anchor', async ({ waffle }) => {
		const page = waffle.page;
		await clickSketch(page, 'front', { tool: 'point' });

		const o = await screenOff(page, 0, 0);
		await moveTo(page, o.x + 60, o.y - 60);
		await moveTo(page, o.x, o.y);
		await page.waitForTimeout(150);
		expect((await indicator(page))?.type, 'hovering the origin').toBe('origin');

		await moveTo(page, o.x + 60, o.y - 60);
		await moveTo(page, o.x + 120, o.y - 3);
		await page.waitForTimeout(150);
		expect((await indicator(page))?.type, "on the origin's horizontal").toBe('align-h');

		const before = await pointPos(page);
		await clickAt(page, o.x + 120, o.y - 3);
		await page.waitForTimeout(300);
		const q = await expectAnchoredAlignment(page, 'HorizontalPoints', before, { x: 0, y: 0 });
		expect(Math.abs(q.y), 'solved Q.y = 0').toBeLessThan(2 * (await pxSize(page)));
	});

	test('R2: an armed source is released once the cursor is far away and off both its axes', async ({ waffle }) => {
		const page = waffle.page;
		await clickSketch(page, 'front', { tool: 'point' });
		await clickAt(page, -200, 150); // P
		await page.waitForTimeout(150);

		// Control: armed, P's horizontal band 300px away still aligns.
		await moveTo(page, -150, 100);
		await moveTo(page, -200, 150);
		await page.waitForTimeout(120);
		expect((await indicator(page))?.type, 'P armed').toBe('coincident');
		await moveTo(page, 100, 154);
		await page.waitForTimeout(120);
		expect((await indicator(page))?.type, 'armed P aligns at 300px along its axis').toBe('align-h');

		// Running along the axis past the release distance keeps P …
		await moveTo(page, 220, 153);
		await page.waitForTimeout(120);
		expect((await indicator(page))?.type, 'still on the axis 420px out').toBe('align-h');

		// … leaving both axes that far out releases it.
		await moveTo(page, 220, -150);
		await page.waitForTimeout(120);
		await moveTo(page, 100, 154);
		await page.waitForTimeout(120);
		expect((await indicator(page))?.type === 'align-h', 'released P no longer aligns').toBe(false);
		expect(await page.evaluate(() => window.__waffle.getInferenceSources().length)).toBe(0);
	});

	test('R3: every body vertex offers its projection; aligning to one off the plane pins an anchor there', async ({ waffle }) => {
		const page = waffle.page;
		// 60×60×60 cube on the front plane (XY, z from 0 to 60), then a sketch
		// on the top plane (y = 0), which cuts the cube through its middle: no
		// cube vertex lies on the sketch plane.
		await createExtrudedBox(page);
		await clickSketch(page, 'top', { tool: 'point' });

		const verts = await page.evaluate(() => window.__waffle.getBodyVertexSnapPoints());
		expect(verts.length, 'eight corners project to four distinct points').toBe(4);
		const planeNormal = await page.evaluate(() => window.__waffle.getState().sketchMode.normal);
		for (const v of verts) {
			const off = v.worldPos[0] * planeNormal[0] + v.worldPos[1] * planeNormal[1] + v.worldPos[2] * planeNormal[2];
			expect(Math.abs(off), 'the vertex is off the sketch plane').toBeGreaterThan(1e-6);
		}

		// The corner drawn lowest on screen; align on its vertical, below the cube.
		const withScreen = [];
		for (const v of verts) withScreen.push({ v, s: await screenOff(page, v.x, v.y) });
		withScreen.sort((a, b) => b.s.y - a.s.y || a.s.x - b.s.x);
		const { v, s } = withScreen[0];

		await moveTo(page, s.x + 40, s.y + 40);
		await moveTo(page, s.x, s.y);
		await page.waitForTimeout(150);
		expect((await indicator(page))?.type, 'hovering the corner projection').toBe('reference');

		await moveTo(page, s.x + 40, s.y + 40);
		await moveTo(page, s.x + 3, s.y + 100);
		await page.waitForTimeout(150);
		expect((await indicator(page))?.type, "on the corner's vertical").toBe('align-v');

		const before = await pointPos(page);
		await clickAt(page, s.x + 3, s.y + 100);
		await page.waitForTimeout(300);
		const q = await expectAnchoredAlignment(page, 'VerticalPoints', before, { x: v.x, y: v.y });
		expect(Math.abs(q.x - v.x), 'solved Q.x = corner x').toBeLessThan(2 * (await pxSize(page)));
	});
});
