/**
 * Dimension select / delete / move tests.
 *
 * A driving dimension is just a constraint, so it must be selectable (sets the
 * shared selectedConstraintIndex), deletable (the global Delete handler honors
 * that index), and movable (a drag offsets the label cosmetically). A clean
 * click still opens the value editor (alter) — that path is covered elsewhere;
 * here we verify the new select/delete/move behavior layered on top of it.
 *
 * The dimension itself is API-added (test SETUP — allowed; we are NOT testing
 * drawing). The interactions under test use real pointer events on the label.
 */
import { test, expect } from './helpers/waffle-test.js';
import { clickSketch, clickLine, pressKey } from './helpers/toolbar.js';
import { drawLine } from './helpers/canvas.js';
import { getEntities, waitForEntityCount } from './helpers/state.js';
import { getConstraints, getConstraintCount } from './helpers/constraint.js';

/** Draw a horizontal line and add a Distance dimension to its endpoints. */
async function lineWithDistance(page) {
	await clickSketch(page);
	await clickLine(page);
	await drawLine(page, -80, 0, 80, 0);
	await waitForEntityCount(page, 3, 5000);

	const entities = await getEntities(page);
	const line = entities.find((e) => e.type === 'Line');
	expect(line).toBeTruthy();

	await page.evaluate((l) => {
		window.__waffle.addSketchConstraint({
			type: 'Distance', entity_a: l.start_id, entity_b: l.end_id, value: 5
		});
	}, { start_id: line.start_id, end_id: line.end_id });

	const label = page.locator('.dim-label').first();
	await expect(label).toBeVisible({ timeout: 4000 });
	return label;
}

const selectedIndex = (page) =>
	page.evaluate(() => window.__waffle?.getSelectedConstraintIndex?.() ?? null);

test.describe('dimension select / delete', () => {
	test('clicking a dimension selects it (shared constraint selection)', async ({ waffle }) => {
		const page = waffle.page;
		const label = await lineWithDistance(page);

		expect(await selectedIndex(page)).toBeNull();

		// A driving dimension opens the editor on click (button → input), so the
		// highlight class lives on the now-hidden button; the shared selection
		// index is still set — that is the contract Delete relies on.
		await label.click();
		await page.waitForTimeout(150);
		expect(await selectedIndex(page)).not.toBeNull();

		// A reference dimension does NOT edit on click, so it stays a button and
		// shows the selection highlight. Toggle to reference via the API (the
		// right-click contextmenu path is covered by sketch-reference-dimensions).
		await page.keyboard.press('Escape');
		const idx = await page.evaluate(() => {
			const cs = window.__waffle.getConstraints();
			const i = cs.findIndex((c) => c.type === 'Distance');
			window.__waffle.toggleConstraintReference(i);
			return i;
		});
		expect(idx).toBeGreaterThanOrEqual(0);
		await page.waitForTimeout(150);
		await expect(label).toHaveClass(/dim-reference/);

		await label.click();
		await page.waitForTimeout(150);
		await expect(label).toHaveClass(/dim-selected/);
		expect(await selectedIndex(page)).not.toBeNull();
	});

	test('Delete removes a selected dimension', async ({ waffle }) => {
		const page = waffle.page;
		const label = await lineWithDistance(page);

		const before = await getConstraintCount(page);
		expect(before).toBeGreaterThanOrEqual(1);

		// Click selects + opens the editor; Escape closes the editor but keeps
		// the selection; Delete then removes the dimension.
		await label.click();
		await page.waitForTimeout(100);
		await page.keyboard.press('Escape');
		await page.waitForTimeout(100);
		expect(await selectedIndex(page)).not.toBeNull();

		await pressKey(page, 'Delete');
		await page.waitForTimeout(200);

		const after = await getConstraintCount(page);
		expect(after).toBe(before - 1);
		const dist = (await getConstraints(page)).find((c) => c.type === 'Distance');
		expect(dist).toBeFalsy();
	});

	test('dragging a dimension label moves it without editing', async ({ waffle }) => {
		const page = waffle.page;
		const label = await lineWithDistance(page);

		const box = await label.boundingBox();
		expect(box).toBeTruthy();
		const cx = box.x + box.width / 2;
		const cy = box.y + box.height / 2;

		// Press, drag well past the 5px threshold, release.
		await page.mouse.move(cx, cy);
		await page.mouse.down();
		await page.mouse.move(cx + 40, cy - 30, { steps: 5 });
		await page.mouse.up();
		await page.waitForTimeout(150);

		// A drag must NOT open the value editor.
		const editorVisible = await page
			.locator('.dim-input')
			.isVisible({ timeout: 500 })
			.catch(() => false);
		expect(editorVisible).toBe(false);

		// The label moved on screen (cosmetic offset applied).
		const moved = await label.boundingBox();
		const dx = Math.abs(moved.x - box.x);
		const dy = Math.abs(moved.y - box.y);
		expect(dx + dy).toBeGreaterThan(5);
	});
});
