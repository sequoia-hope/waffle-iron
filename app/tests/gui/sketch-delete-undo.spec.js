/**
 * Deleting sketch geometry is undoable (found 2026-10-30 while building the
 * constraint list: Delete, then Ctrl+Z, left the sketch empty).
 *
 * Deletions were recorded as add/remove-list undo entries, and undo of such
 * an entry REMOVES its items, so undoing a deletion "removed" what was
 * already gone. Deletions now record a snapshot pair, which undo and redo
 * restore exactly (constraints included). Real pointer and keyboard input.
 */
import { test, expect } from './helpers/waffle-test.js';
import { clickSketch, clickSelect, clickRectangle } from './helpers/toolbar.js';
import { drawLine, drawRectangle, clickAt } from './helpers/canvas.js';
import { waitForEntityCount } from './helpers/state.js';
import { getConstraintCount } from './helpers/constraint.js';

const count = (page) => page.evaluate(() => window.__waffle.getEntities().length);

test('Delete a line, Ctrl+Z restores it, Ctrl+Shift+Z deletes it again', async ({ waffle }) => {
	const page = waffle.page;
	await clickSketch(page);
	await drawLine(page, -100, 0, 100, 0);
	await page.keyboard.press('Escape');
	await waitForEntityCount(page, 3, 3000);
	const constraints = await getConstraintCount(page); // the snap's Horizontal

	await clickSelect(page);
	await clickAt(page, 0, 0);
	await page.keyboard.press('Delete');
	await expect.poll(() => count(page)).toBe(0);

	await page.keyboard.press('Control+z');
	await expect.poll(() => count(page)).toBe(3);
	expect(await getConstraintCount(page)).toBe(constraints);

	await page.keyboard.press('Control+Shift+z');
	await expect.poll(() => count(page)).toBe(0);
});

test('deleting one rectangle edge undoes with its constraints', async ({ waffle }) => {
	const page = waffle.page;
	await clickSketch(page);
	await clickRectangle(page);
	await drawRectangle(page, -100, -60, 100, 60);
	await waitForEntityCount(page, 8, 5000);
	await page.keyboard.press('Escape');
	const before = { n: await count(page), c: await getConstraintCount(page) };

	await clickSelect(page);
	await clickAt(page, 0, -60);
	await page.keyboard.press('Delete');
	await expect.poll(() => getConstraintCount(page)).toBeLessThan(before.c);

	await page.keyboard.press('Control+z');
	await expect.poll(() => count(page)).toBe(before.n);
	expect(await getConstraintCount(page)).toBe(before.c);
});
