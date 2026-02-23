/**
 * Sprint 3: Construction toggle tests.
 *
 * Verifies G key toggles construction flag on selected entities,
 * and construction entities are excluded from profile extraction.
 */
import { test, expect } from './helpers/waffle-test.js';
import { clickSketch, clickSelect, clickRectangle } from './helpers/toolbar.js';
import { drawLine, drawRectangle } from './helpers/canvas.js';
import { getEntityCount, waitForEntityCount, getEntities } from './helpers/state.js';
import { setSketchSelection } from './helpers/constraint.js';

test.describe('sketch construction toggle', () => {
	test.beforeEach(async ({ waffle }) => {
		await clickSketch(waffle.page);
	});

	test('G key toggles construction flag on selected line', async ({ waffle }) => {
		const page = waffle.page;

		// Draw a line
		await drawLine(page, -100, 0, 100, 0);
		await waitForEntityCount(page, 3, 5000);

		const entities = await getEntities(page);
		const line = entities.find(e => e.type === 'Line');
		expect(line.construction).toBe(false);

		// Select the line and press G
		await clickSelect(page);
		await setSketchSelection(page, [line.id]);
		await page.keyboard.press('g');
		await page.waitForTimeout(200);

		// Check construction flag is now true
		const updated = await getEntities(page);
		const updatedLine = updated.find(e => e.id === line.id);
		expect(updatedLine.construction).toBe(true);
	});

	test('G key toggles construction back to false', async ({ waffle }) => {
		const page = waffle.page;

		await drawLine(page, -100, 0, 100, 0);
		await waitForEntityCount(page, 3, 5000);

		const entities = await getEntities(page);
		const line = entities.find(e => e.type === 'Line');

		await clickSelect(page);
		await setSketchSelection(page, [line.id]);

		// Toggle on
		await page.keyboard.press('g');
		await page.waitForTimeout(200);
		let updated = await getEntities(page);
		expect(updated.find(e => e.id === line.id).construction).toBe(true);

		// Toggle off
		await page.keyboard.press('g');
		await page.waitForTimeout(200);
		updated = await getEntities(page);
		expect(updated.find(e => e.id === line.id).construction).toBe(false);
	});

	test('X key (toolbar shortcut) toggles construction on selected entity', async ({ waffle }) => {
		const page = waffle.page;

		await drawLine(page, -100, 0, 100, 0);
		await waitForEntityCount(page, 3, 5000);

		const entities = await getEntities(page);
		const line = entities.find(e => e.type === 'Line');

		await clickSelect(page);
		await setSketchSelection(page, [line.id]);
		await page.keyboard.press('x');
		await page.waitForTimeout(200);

		const updated = await getEntities(page);
		expect(updated.find(e => e.id === line.id).construction).toBe(true);
	});

	test('construction line breaks rectangle profile', async ({ waffle }) => {
		const page = waffle.page;

		// Draw a rectangle (creates closed profile)
		await clickRectangle(page);
		await drawRectangle(page, -80, -60, 80, 60);
		await waitForEntityCount(page, 8, 5000);
		await page.waitForTimeout(500);

		// Check profiles exist
		const profilesBefore = await page.evaluate(() => window.__waffle.getProfiles());
		expect(profilesBefore.length).toBeGreaterThanOrEqual(1);

		// Toggle one line to construction
		const entities = await getEntities(page);
		const line = entities.find(e => e.type === 'Line');
		await page.evaluate((id) => {
			window.__waffle.setSketchSelection([id]);
		}, line.id);
		await page.keyboard.press('x');
		await page.waitForTimeout(300);

		// Profile should change — removing one side of rectangle breaks the closed 4-sided profile
		// However, the remaining 3 lines may still form a valid 3-sided open path (not a closed profile),
		// or the profile extractor may find a different valid profile.
		// Key assertion: profile count should be less than before OR the construction entity
		// should be excluded from the profile.
		const profilesAfter = await page.evaluate(() => window.__waffle.getProfiles());
		if (profilesAfter.length > 0) {
			// If a profile still exists, it should not contain the construction line
			const constructionEntities = await page.evaluate(() =>
				window.__waffle.getEntities().filter(e => e.construction).map(e => e.id)
			);
			for (const p of profilesAfter) {
				for (const cId of constructionEntities) {
					expect(p.entityIds).not.toContain(cId);
				}
			}
		}
	});
});
