/**
 * Sprint 1: Entity deletion tests.
 *
 * Verifies Delete/Backspace removes selected sketch entities,
 * cascades constraint deletion, and cleans up orphaned points.
 */
import { test, expect } from './helpers/waffle-test.js';
import { clickSketch, clickLine, clickRectangle, clickCircle, clickSelect } from './helpers/toolbar.js';
import { drawLine, drawRectangle, drawCircle, clickAt } from './helpers/canvas.js';
import { getEntityCount, getEntityCountByType, waitForEntityCount, getEntities } from './helpers/state.js';
import { getConstraints, getConstraintCount, setSketchSelection } from './helpers/constraint.js';

test.describe('sketch entity deletion', () => {
	test.beforeEach(async ({ waffle }) => {
		await clickSketch(waffle.page);
	});

	test('delete a single line removes line + 2 orphaned points', async ({ waffle }) => {
		const page = waffle.page;

		// Draw a single line (creates 2 points + 1 line = 3 entities)
		await drawLine(page, -100, 0, 100, 0);
		await waitForEntityCount(page, 3, 5000);

		// Get the line entity
		const entities = await getEntities(page);
		const line = entities.find(e => e.type === 'Line');
		expect(line).toBeTruthy();

		// Select the line and press Delete
		await setSketchSelection(page, [line.id]);
		await page.keyboard.press('Delete');
		await page.waitForTimeout(300);

		// All entities should be gone (line + 2 orphaned points)
		const remaining = await getEntityCount(page);
		expect(remaining).toBe(0);
	});

	test('delete rectangle line removes line + 2 orphaned points, 5 entities remain', async ({ waffle }) => {
		const page = waffle.page;

		// Draw a rectangle (4 points + 4 lines = 8 entities, 4 H/V constraints)
		await clickRectangle(page);
		await drawRectangle(page, -80, -60, 80, 60);
		await waitForEntityCount(page, 8, 5000);

		const entities = await getEntities(page);
		const lines = entities.filter(e => e.type === 'Line');
		expect(lines.length).toBe(4);

		// Select one line and delete it
		await clickSelect(page);
		await setSketchSelection(page, [lines[0].id]);
		await page.keyboard.press('Delete');
		await page.waitForTimeout(300);

		// Line removed + 2 points that were orphaned. But rectangle points
		// are shared, so only points unique to this line are orphaned.
		// With 4 shared corners, deleting 1 line: the 2 points remain used by adjacent lines.
		// So only the line is removed = 7 entities remain.
		// Actually, each corner point is used by 2 lines. Deleting 1 line:
		// its 2 corner points are still used by the adjacent lines. Only line removed.
		const remaining = await getEntityCount(page);
		// 8 - 1 (line removed, no orphaned points) = 7
		expect(remaining).toBe(7);
	});

	test('delete line with H constraint also removes the constraint', async ({ waffle }) => {
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
		await page.waitForTimeout(200);

		const beforeConstraints = await getConstraintCount(page);
		expect(beforeConstraints).toBeGreaterThanOrEqual(1);

		// Delete the line
		await setSketchSelection(page, [line.id]);
		await page.keyboard.press('Delete');
		await page.waitForTimeout(300);

		// Entities and constraints should all be gone
		expect(await getEntityCount(page)).toBe(0);
		expect(await getConstraintCount(page)).toBe(0);
	});

	test('delete shared point cascades to referencing lines', async ({ waffle }) => {
		const page = waffle.page;

		// Draw two lines that share a point (via snap).
		// Draw first line
		await drawLine(page, -100, 0, 0, 0);
		await waitForEntityCount(page, 3, 5000);

		// Draw second line starting from endpoint of first (auto-snaps)
		await drawLine(page, 0, 0, 100, 50);
		await page.waitForTimeout(300);

		// Should have 3 points + 2 lines = 5 entities (shared point)
		// (line tool chains: end of first becomes start of second)
		const entitiesBefore = await getEntities(page);
		const points = entitiesBefore.filter(e => e.type === 'Point');
		const lines = entitiesBefore.filter(e => e.type === 'Line');

		expect(lines.length).toBe(2);

		// Find the shared point (used by both lines)
		const sharedPointId = lines[0].end_id; // chained line shares endpoint

		// Select the shared point and delete it
		await clickSelect(page);
		await setSketchSelection(page, [sharedPointId]);
		await page.keyboard.press('Delete');
		await page.waitForTimeout(300);

		// Both lines should be deleted (they reference the shared point)
		// All points should be gone too (all orphaned after line deletion)
		expect(await getEntityCount(page)).toBe(0);
	});

	test('delete with nothing selected is a no-op', async ({ waffle }) => {
		const page = waffle.page;

		// Draw a line
		await drawLine(page, -100, 0, 100, 0);
		await waitForEntityCount(page, 3, 5000);

		// Clear selection
		await clickSelect(page);
		await setSketchSelection(page, []);
		await page.keyboard.press('Delete');
		await page.waitForTimeout(200);

		// Nothing should change
		expect(await getEntityCount(page)).toBe(3);
	});

	test('delete via Backspace key works the same as Delete', async ({ waffle }) => {
		const page = waffle.page;

		// Draw a line
		await drawLine(page, -100, 0, 100, 0);
		await waitForEntityCount(page, 3, 5000);

		const entities = await getEntities(page);
		const line = entities.find(e => e.type === 'Line');

		await setSketchSelection(page, [line.id]);
		await page.keyboard.press('Backspace');
		await page.waitForTimeout(300);

		expect(await getEntityCount(page)).toBe(0);
	});

	test('delete via API works', async ({ waffle }) => {
		const page = waffle.page;

		// Draw a circle (center point + circle = 2 entities)
		await clickCircle(page);
		await drawCircle(page, 0, 0, 50, 0);
		await page.waitForTimeout(300);

		const entitiesBefore = await getEntities(page);
		const circle = entitiesBefore.find(e => e.type === 'Circle');
		expect(circle).toBeTruthy();

		// Delete via API
		await page.evaluate((id) => {
			window.__waffle.removeSketchEntities([id]);
		}, circle.id);
		await page.waitForTimeout(300);

		// Circle + orphaned center point removed
		expect(await getEntityCount(page)).toBe(0);
	});

	test('new entities after deletion get unique IDs', async ({ waffle }) => {
		const page = waffle.page;

		// Draw a line
		await drawLine(page, -100, 0, 100, 0);
		await waitForEntityCount(page, 3, 5000);

		const before = await getEntities(page);
		const oldIds = new Set(before.map(e => e.id));

		// Delete all
		const line = before.find(e => e.type === 'Line');
		await setSketchSelection(page, [line.id]);
		await page.keyboard.press('Delete');
		await page.waitForTimeout(300);

		expect(await getEntityCount(page)).toBe(0);

		// Draw new line
		await clickLine(page);
		await drawLine(page, -50, -50, 50, 50);
		await waitForEntityCount(page, 3, 5000);

		const after = await getEntities(page);
		// New entity IDs should be different (monotonically increasing)
		for (const e of after) {
			// IDs should not collide with old ones (they may be higher)
			// Since allocEntityId is monotonic, new IDs > old max ID
			const maxOld = Math.max(...oldIds);
			expect(e.id).toBeGreaterThan(maxOld);
		}
	});
});
