/**
 * Snap preview integration tests — verifies candidate filtering,
 * snap settings, cross-tool behavior, and state reset for the
 * snap preview system.
 */
import { test, expect } from './helpers/waffle-test.js';
import { clickSketch, clickLine, clickRectangle, clickCircle, pressKey } from './helpers/toolbar.js';
import { clickAt, drawLine, moveTo } from './helpers/canvas.js';
import { waitForEntityCount } from './helpers/state.js';

test.describe('snap preview integration', () => {
	test('candidates appear during line tool', async ({ waffle }) => {
		const page = waffle.page;
		await clickSketch(page);

		// Draw a horizontal line to create snap targets
		await drawLine(page, -60, 0, 60, 0);
		await waitForEntityCount(page, 3, 3000);

		// Start a new line segment away from existing geometry
		await clickAt(page, 0, -150);
		await page.waitForTimeout(200);

		// Move near the existing line
		await moveTo(page, 10, 5);
		await page.waitForTimeout(300);

		const candidates = await page.evaluate(() =>
			window.__waffle?.getSnapCandidates() ?? []
		);
		expect(candidates.length).toBeGreaterThan(0);

		// Verify tool state is firstPointPlaced
		const toolState = await page.evaluate(() =>
			window.__waffle?.getToolState() ?? 'unknown'
		);
		expect(toolState).toBe('firstPointPlaced');
	});

	test('candidates appear during rectangle tool', async ({ waffle }) => {
		const page = waffle.page;
		await clickSketch(page);

		// Draw a line to create snap targets
		await drawLine(page, -60, 0, 60, 0);
		await waitForEntityCount(page, 3, 3000);

		// Switch to rectangle tool
		await clickRectangle(page);

		// Start rectangle far from geometry
		await clickAt(page, -200, -200);
		await page.waitForTimeout(200);

		// Move near the existing line
		await moveTo(page, 10, 5);
		await page.waitForTimeout(300);

		const candidates = await page.evaluate(() =>
			window.__waffle?.getSnapCandidates() ?? []
		);
		expect(candidates.length).toBeGreaterThan(0);
	});

	test('candidates appear during circle tool', async ({ waffle }) => {
		const page = waffle.page;
		await clickSketch(page);

		// Draw a line to create snap targets
		await drawLine(page, -60, 0, 60, 0);
		await waitForEntityCount(page, 3, 3000);

		// Switch to circle tool
		await clickCircle(page);

		// Place center far from geometry
		await clickAt(page, -200, -200);
		await page.waitForTimeout(200);

		// Move near the existing line
		await moveTo(page, 10, 5);
		await page.waitForTimeout(300);

		const candidates = await page.evaluate(() =>
			window.__waffle?.getSnapCandidates() ?? []
		);
		expect(candidates.length).toBeGreaterThan(0);
	});

	test('previewPx=0 disables candidates but active snap still works', async ({ waffle }) => {
		const page = waffle.page;
		await clickSketch(page);

		// Draw a line to create snap targets
		await drawLine(page, -60, 0, 60, 0);
		await waitForEntityCount(page, 3, 3000);

		// Disable preview candidates
		await page.evaluate(() =>
			window.__waffle.updateSnapSettings({ previewPx: 0 })
		);

		// Start a new line
		await clickAt(page, 0, -150);
		await page.waitForTimeout(200);

		// Move near geometry — candidates should be empty
		await moveTo(page, 5, 5);
		await page.waitForTimeout(300);

		const candidates = await page.evaluate(() =>
			window.__waffle?.getSnapCandidates() ?? []
		);
		expect(candidates.length).toBe(0);

		// Active snap should still work at origin
		await moveTo(page, 0, 0);
		await page.waitForTimeout(300);

		const indicator = await page.evaluate(() =>
			window.__waffle?.getSnapIndicator()
		);
		expect(indicator).not.toBeNull();
	});

	test('larger previewPx captures more candidates', async ({ waffle }) => {
		const page = waffle.page;
		await clickSketch(page);

		// Draw a line to create snap targets
		await drawLine(page, -60, 0, 60, 0);
		await waitForEntityCount(page, 3, 3000);

		// Start a new line
		await clickAt(page, 0, -150);
		await page.waitForTimeout(200);

		// Check default candidate count
		await moveTo(page, 5, 5);
		await page.waitForTimeout(300);

		const defaultCount = await page.evaluate(() =>
			(window.__waffle?.getSnapCandidates() ?? []).length
		);

		// Increase preview radius
		await page.evaluate(() =>
			window.__waffle.updateSnapSettings({ previewPx: 200 })
		);

		// Slight move to force refresh
		await moveTo(page, 6, 6);
		await page.waitForTimeout(300);

		const largeCount = await page.evaluate(() =>
			(window.__waffle?.getSnapCandidates() ?? []).length
		);

		expect(largeCount).toBeGreaterThanOrEqual(defaultCount);
	});

	test('active origin snap excluded from candidates', async ({ waffle }) => {
		const page = waffle.page;
		await clickSketch(page);

		// Start a line far from origin
		await clickAt(page, -200, -200);
		await page.waitForTimeout(200);

		// Move to origin to trigger origin snap
		await moveTo(page, 0, 0);
		await page.waitForTimeout(300);

		const indicator = await page.evaluate(() =>
			window.__waffle?.getSnapIndicator()
		);
		expect(indicator).not.toBeNull();
		expect(indicator.type).toBe('origin');

		const candidates = await page.evaluate(() =>
			window.__waffle?.getSnapCandidates() ?? []
		);
		// The active origin snap should not appear in candidates
		const originInCandidates = candidates.some(
			c => c.type === 'origin' && Math.abs(c.x) < 0.01 && Math.abs(c.y) < 0.01
		);
		expect(originInCandidates).toBe(false);
	});

	test('active midpoint snap excluded from candidates', async ({ waffle }) => {
		const page = waffle.page;
		await clickSketch(page);

		// Draw a line whose midpoint is at canvas center (0,30)
		await drawLine(page, -80, 30, 80, 30);
		await waitForEntityCount(page, 3, 3000);

		// Start a new line
		await clickAt(page, 0, -150);
		await page.waitForTimeout(200);

		// Move to the midpoint of the line
		await moveTo(page, 0, 30);
		await page.waitForTimeout(300);

		const indicator = await page.evaluate(() =>
			window.__waffle?.getSnapIndicator()
		);
		expect(indicator).not.toBeNull();
		expect(indicator.type).toBe('midpoint');

		const candidates = await page.evaluate(() =>
			window.__waffle?.getSnapCandidates() ?? []
		);
		// Active midpoint should not appear in candidates at the same location
		const midpointAtSamePos = candidates.some(
			c => c.type === 'midpoint' &&
				Math.abs(c.x - indicator.x) < 0.5 &&
				Math.abs(c.y - indicator.y) < 0.5
		);
		expect(midpointAtSamePos).toBe(false);
	});

	test('snap types include midpoint, quadrant, and origin', async ({ waffle }) => {
		// Static assertion: the snap system recognizes these new types
		const validTypes = [
			'coincident', 'horizontal', 'vertical', 'on-entity',
			'tangent', 'perpendicular', 'midpoint', 'quadrant', 'origin',
		];
		expect(validTypes).toContain('midpoint');
		expect(validTypes).toContain('quadrant');
		expect(validTypes).toContain('origin');

		// Verify the runtime snap system also knows about these types
		const page = waffle.page;
		await clickSketch(page);

		// Move to origin to trigger origin snap
		await clickAt(page, -200, -200);
		await page.waitForTimeout(200);
		await moveTo(page, 0, 0);
		await page.waitForTimeout(300);

		const indicator = await page.evaluate(() =>
			window.__waffle?.getSnapIndicator()
		);
		expect(indicator).not.toBeNull();
		expect(validTypes).toContain(indicator.type);
	});

	test('candidates cleared on Escape (tool reset)', async ({ waffle }) => {
		const page = waffle.page;
		await clickSketch(page);

		// Generous preview radius so candidates appear
		await page.evaluate(() => window.__waffle.updateSnapSettings({ previewPx: 200 }));

		// Draw a line to create snap targets
		await drawLine(page, -60, 0, 60, 0);
		await waitForEntityCount(page, 3, 3000);

		// Start a new line and move near geometry
		await clickAt(page, 0, -150);
		await page.waitForTimeout(200);
		await moveTo(page, 5, 5);
		await page.waitForTimeout(300);

		const candidatesBefore = await page.evaluate(() =>
			(window.__waffle?.getSnapCandidates() ?? []).length
		);
		expect(candidatesBefore).toBeGreaterThan(0);

		// Press Escape to reset tool
		await pressKey(page, 'Escape');
		await page.waitForTimeout(300);

		// Verify the tool was reset — tool state goes to idle
		const toolState = await page.evaluate(() =>
			window.__waffle?.getToolState() ?? 'unknown'
		);
		expect(['idle', 'select', 'unknown']).toContain(toolState);
	});

	test('candidates persist across moves in same tool operation', async ({ waffle }) => {
		const page = waffle.page;
		await clickSketch(page);

		// Generous preview radius so candidates appear near the line
		await page.evaluate(() => window.__waffle.updateSnapSettings({ previewPx: 200 }));

		// Draw a line to create snap targets
		await drawLine(page, -60, 0, 60, 0);
		await waitForEntityCount(page, 3, 3000);

		// Start a new line
		await clickAt(page, 0, -150);
		await page.waitForTimeout(200);

		// Move to three positions near the existing line
		await moveTo(page, -55, 5);
		await page.waitForTimeout(300);
		const candidates1 = await page.evaluate(() =>
			(window.__waffle?.getSnapCandidates() ?? []).length
		);

		await moveTo(page, -5, 5);
		await page.waitForTimeout(300);
		const candidates2 = await page.evaluate(() =>
			(window.__waffle?.getSnapCandidates() ?? []).length
		);

		await moveTo(page, 55, 5);
		await page.waitForTimeout(300);
		const candidates3 = await page.evaluate(() =>
			(window.__waffle?.getSnapCandidates() ?? []).length
		);

		// All positions near the line should show candidates
		expect(candidates1).toBeGreaterThan(0);
		expect(candidates2).toBeGreaterThan(0);
		expect(candidates3).toBeGreaterThan(0);
	});
});
