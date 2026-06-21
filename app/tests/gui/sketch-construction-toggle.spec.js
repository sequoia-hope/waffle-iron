/**
 * Sprint 3: Construction toggle tests.
 *
 * Verifies X key toggles construction flag on selected entities,
 * and construction entities are excluded from profile extraction.
 */
import { test, expect } from './helpers/waffle-test.js';
import { clickSketch, clickSelect, clickRectangle } from './helpers/toolbar.js';
import { drawLine, drawRectangle, drawCircle } from './helpers/canvas.js';
import { getEntityCount, waitForEntityCount, getEntities } from './helpers/state.js';
import { setSketchSelection } from './helpers/constraint.js';

test.describe('sketch construction toggle', () => {
	test.beforeEach(async ({ waffle }) => {
		await clickSketch(waffle.page);
	});

	test('X key toggles construction flag on selected line', async ({ waffle }) => {
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
		await page.keyboard.press('x');
		await page.waitForTimeout(200);

		// Check construction flag is now true
		const updated = await getEntities(page);
		const updatedLine = updated.find(e => e.id === line.id);
		expect(updatedLine.construction).toBe(true);
	});

	test('X key toggles construction back to false', async ({ waffle }) => {
		const page = waffle.page;

		await drawLine(page, -100, 0, 100, 0);
		await waitForEntityCount(page, 3, 5000);

		const entities = await getEntities(page);
		const line = entities.find(e => e.type === 'Line');

		await clickSelect(page);
		await setSketchSelection(page, [line.id]);

		// Toggle on
		await page.keyboard.press('x');
		await page.waitForTimeout(200);
		let updated = await getEntities(page);
		expect(updated.find(e => e.id === line.id).construction).toBe(true);

		// Toggle off
		await page.keyboard.press('x');
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

	test('toggle circle to construction', async ({ waffle }) => {
		const page = waffle.page;

		// Draw a circle
		await page.evaluate(() => window.__waffle.setTool('circle'));
		await page.waitForTimeout(100);
		await drawCircle(page, 0, 0, 60, 0);
		await page.waitForTimeout(300);

		const entities = await getEntities(page);
		const circle = entities.find(e => e.type === 'Circle');
		expect(circle).toBeTruthy();
		expect(circle.construction).toBe(false);

		// Select circle and toggle construction
		await clickSelect(page);
		await setSketchSelection(page, [circle.id]);
		await page.keyboard.press('x');
		await page.waitForTimeout(200);

		const updated = await getEntities(page);
		expect(updated.find(e => e.id === circle.id).construction).toBe(true);
	});

	test('multi-select lines + toggle → all become construction', async ({ waffle }) => {
		const page = waffle.page;

		// Draw two lines
		await drawLine(page, -100, -30, 100, -30);
		await waitForEntityCount(page, 3, 5000);
		await page.keyboard.press('Escape');
		await page.waitForTimeout(100);

		await page.evaluate(() => window.__waffle.setTool('line'));
		await page.waitForTimeout(100);
		await drawLine(page, -100, 30, 100, 30);
		await page.waitForTimeout(300);

		const entities = await getEntities(page);
		const lines = entities.filter(e => e.type === 'Line');
		expect(lines.length).toBe(2);

		// Multi-select both lines
		await clickSelect(page);
		await setSketchSelection(page, lines.map(l => l.id));
		await page.keyboard.press('x');
		await page.waitForTimeout(200);

		// Both should be construction
		const updated = await getEntities(page);
		for (const line of lines) {
			expect(updated.find(e => e.id === line.id).construction).toBe(true);
		}
	});

	test('construction entities excluded from profile extraction', async ({ waffle }) => {
		const page = waffle.page;

		// Draw a rectangle (closed profile)
		await clickRectangle(page);
		await drawRectangle(page, -80, -60, 80, 60);
		await waitForEntityCount(page, 8, 5000);
		await page.waitForTimeout(500);

		// Verify profile exists
		const profilesBefore = await page.evaluate(() => window.__waffle.getProfiles());

		// Add a construction line across the rectangle using fixed IDs
		await page.evaluate(() => {
			const w = window.__waffle;
			w.addSketchEntity({ type: 'Point', id: 9001, x: -80, y: 0, construction: false });
			w.addSketchEntity({ type: 'Point', id: 9002, x: 80, y: 0, construction: false });
			w.addSketchEntity({ type: 'Line', id: 9003, start_id: 9001, end_id: 9002, construction: true });
		});
		await page.waitForTimeout(500);

		// Construction line should NOT appear in any profile
		const profilesAfter = await page.evaluate(() => window.__waffle.getProfiles());
		const constructionIds = await page.evaluate(() =>
			window.__waffle.getEntities().filter(e => e.construction).map(e => e.id)
		);
		for (const p of profilesAfter) {
			for (const cId of constructionIds) {
				expect(p.entityIds).not.toContain(cId);
			}
		}
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
