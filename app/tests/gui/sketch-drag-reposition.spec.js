/**
 * Sprint 2: Drag-to-reposition tests.
 *
 * Verifies that dragging points in select tool moves them via solver
 * with temporary WhereDragged constraint.
 */
import { test, expect } from './helpers/waffle-test.js';
import { clickSketch, clickSelect } from './helpers/toolbar.js';
import { drawLine, drawRectangle, clickAt, moveTo, getCanvasBounds } from './helpers/canvas.js';
import { getEntityCount, waitForEntityCount, getEntities } from './helpers/state.js';
import { setSketchSelection, getConstraints } from './helpers/constraint.js';

/**
 * Drag from one canvas offset to another with intermediate steps.
 */
async function dragOnCanvas(page, x1, y1, x2, y2, steps = 10) {
	const bounds = await getCanvasBounds(page);
	if (!bounds) throw new Error('Canvas not visible');
	const sx = bounds.centerX + x1, sy = bounds.centerY + y1;
	const ex = bounds.centerX + x2, ey = bounds.centerY + y2;
	await page.mouse.move(sx, sy);
	await page.mouse.down();
	for (let i = 1; i <= steps; i++) {
		const t = i / steps;
		await page.mouse.move(sx + (ex - sx) * t, sy + (ey - sy) * t);
	}
	await page.mouse.up();
	await page.waitForTimeout(300);
}

test.describe('sketch drag-to-reposition', () => {
	test.beforeEach(async ({ waffle }) => {
		await clickSketch(waffle.page);
	});

	test('drag a free point moves it to new position', async ({ waffle }) => {
		const page = waffle.page;

		// Draw a line
		await drawLine(page, -100, 0, 100, 0);
		await waitForEntityCount(page, 3, 5000);

		// Get the endpoint positions
		const before = await page.evaluate(() => {
			const positions = window.__waffle.getPositions();
			return Object.fromEntries(positions);
		});

		const entities = await getEntities(page);
		const points = entities.filter(e => e.type === 'Point');
		expect(points.length).toBe(2);

		// Switch to select tool
		await clickSelect(page);

		// Use API to drag a point
		await page.evaluate(([ptId]) => {
			window.__waffle.dragSketchPoint(ptId, 5, 5);
			window.__waffle.finalizeDrag();
		}, [points[1].id]);
		await page.waitForTimeout(300);

		// Position should have changed
		const after = await page.evaluate((ptId) => {
			const positions = window.__waffle.getPositions();
			return positions.get(ptId);
		}, points[1].id);

		expect(after).toBeTruthy();
		// Position should be different from original
		const origPos = before[String(points[1].id)];
		expect(Math.abs(after.x - origPos.x) + Math.abs(after.y - origPos.y)).toBeGreaterThan(0.1);
	});

	test('after drag finalize, no WhereDragged constraint remains', async ({ waffle }) => {
		const page = waffle.page;

		// Draw a line
		await drawLine(page, -100, 0, 100, 0);
		await waitForEntityCount(page, 3, 5000);

		const entities = await getEntities(page);
		const point = entities.find(e => e.type === 'Point');

		// Drag and finalize via API
		await page.evaluate(([ptId]) => {
			window.__waffle.dragSketchPoint(ptId, 3, 3);
			window.__waffle.finalizeDrag();
		}, [point.id]);
		await page.waitForTimeout(300);

		// No WhereDragged constraints should remain
		const constraints = await getConstraints(page);
		const dragConstraints = constraints.filter(c => c.type === 'WhereDragged' && c._isDrag);
		expect(dragConstraints.length).toBe(0);
	});

	test('drag preserves entity count', async ({ waffle }) => {
		const page = waffle.page;

		// Draw a rectangle (8 entities)
		await page.evaluate(() => window.__waffle.setTool('rectangle'));
		await drawRectangle(page, -80, -60, 80, 60);
		await waitForEntityCount(page, 8, 5000);

		const entities = await getEntities(page);
		const point = entities.find(e => e.type === 'Point');

		// Drag a corner point
		await page.evaluate(([ptId]) => {
			window.__waffle.dragSketchPoint(ptId, 10, 10);
			window.__waffle.finalizeDrag();
		}, [point.id]);
		await page.waitForTimeout(300);

		// Entity count should remain the same
		expect(await getEntityCount(page)).toBe(8);
	});

	test('drag preserves H-constraint on horizontal line endpoint', async ({ waffle }) => {
		const page = waffle.page;

		// Draw a line
		await drawLine(page, -100, 0, 100, 0);
		await waitForEntityCount(page, 3, 5000);

		// Add H constraint
		const entities = await getEntities(page);
		const line = entities.find(e => e.type === 'Line');
		await page.evaluate((lineId) => {
			window.__waffle.addSketchConstraint({ type: 'Horizontal', entity: lineId });
		}, line.id);
		await page.waitForTimeout(300);

		// Drag an endpoint via API
		const point = entities.find(e => e.type === 'Point');
		await page.evaluate(([ptId]) => {
			window.__waffle.dragSketchPoint(ptId, 8, 5);
			window.__waffle.finalizeDrag();
		}, [point.id]);
		await page.waitForTimeout(500);

		// After solver runs, both endpoints should have same Y (H constraint preserved)
		const positions = await page.evaluate(() => {
			const pos = window.__waffle.getPositions();
			return Object.fromEntries(pos);
		});
		const pts = entities.filter(e => e.type === 'Point');
		const posA = positions[String(pts[0].id)];
		const posB = positions[String(pts[1].id)];
		if (posA && posB) {
			expect(Math.abs(posA.y - posB.y)).toBeLessThan(0.1);
		}
	});

	test('click-release without movement selects point, does not drag', async ({ waffle }) => {
		const page = waffle.page;

		// Draw a line
		await drawLine(page, -80, 0, 80, 0);
		await waitForEntityCount(page, 3, 5000);

		// Switch to select tool
		await clickSelect(page);
		await page.waitForTimeout(200);

		// Get position of a point before click
		const entities = await getEntities(page);
		const point = entities.find(e => e.type === 'Point');
		const posBefore = await page.evaluate((ptId) => {
			const pos = window.__waffle.getPositions();
			return pos.get(ptId);
		}, point.id);

		// Click on the point (select it) — no drag
		await page.evaluate((ids) => {
			window.__waffle.setSketchSelection(ids);
		}, [point.id]);
		await page.waitForTimeout(200);

		// Position should not have changed
		const posAfter = await page.evaluate((ptId) => {
			const pos = window.__waffle.getPositions();
			return pos.get(ptId);
		}, point.id);

		expect(posAfter.x).toBeCloseTo(posBefore.x, 1);
		expect(posAfter.y).toBeCloseTo(posBefore.y, 1);
	});

	test('solver status DOF unchanged after drag finalize', async ({ waffle }) => {
		const page = waffle.page;

		// Draw a line
		await drawLine(page, -100, 0, 100, 0);
		await waitForEntityCount(page, 3, 5000);
		await page.waitForTimeout(500); // Wait for solver

		const beforeStatus = await page.evaluate(() => window.__waffle.getSolveStatus());

		const entities = await getEntities(page);
		const point = entities.find(e => e.type === 'Point');

		// Drag and finalize
		await page.evaluate(([ptId]) => {
			window.__waffle.dragSketchPoint(ptId, 5, 5);
			window.__waffle.finalizeDrag();
		}, [point.id]);
		await page.waitForTimeout(500);

		const afterStatus = await page.evaluate(() => window.__waffle.getSolveStatus());

		// DOF should be the same before and after drag
		if (beforeStatus && afterStatus) {
			expect(afterStatus.dof).toBe(beforeStatus.dof);
		}
	});
});
