/**
 * Snap detection tests for origin, midpoint, and quadrant snap types.
 *
 * Tests that detectSnaps() produces the correct active snap indicators
 * for origin (0,0), line midpoints, and circle quadrant points.
 */
import { test, expect } from './helpers/waffle-test.js';
import { clickSketch, clickLine, clickCircle } from './helpers/toolbar.js';
import { clickAt, drawLine, drawCircle, moveTo } from './helpers/canvas.js';
import { waitForEntityCount, getEntities } from './helpers/state.js';

test.describe('snap detect new types', () => {
	test('origin snap activates near (0,0)', async ({ waffle }) => {
		const page = waffle.page;
		await clickSketch(page);

		// Start a line far from origin
		await clickAt(page, -200, -200);
		await page.waitForTimeout(200);

		// Move to canvas center (sketch origin)
		await moveTo(page, 0, 0);
		await page.waitForTimeout(300);

		const snap = await page.evaluate(() => window.__waffle?.getSnapIndicator());
		expect(snap).not.toBeNull();
		expect(snap.type).toBe('origin');
		expect(snap.x).toBe(0);
		expect(snap.y).toBe(0);
	});

	test('origin snap wins over on-entity when at origin', async ({ waffle }) => {
		const page = waffle.page;
		await clickSketch(page);

		// Draw a horizontal line through origin
		await drawLine(page, -100, 0, 100, 0);
		await waitForEntityCount(page, 3, 3000);

		// Start a new line far away
		await clickAt(page, 0, -150);
		await page.waitForTimeout(200);

		// Move to origin — the line passes through (0,0) but has no point there
		await moveTo(page, 0, 0);
		await page.waitForTimeout(300);

		const snap = await page.evaluate(() => window.__waffle?.getSnapIndicator());
		expect(snap).not.toBeNull();
		// Origin (priority 1b) should beat on-entity (priority 3)
		expect(snap.type).toBe('origin');
	});

	test('midpoint snap activates at line midpoint', async ({ waffle }) => {
		const page = waffle.page;
		await clickSketch(page);

		// Draw a horizontal line from (-80, 30) to (80, 30) — midpoint at (0, 30)
		await drawLine(page, -80, 30, 80, 30);
		await waitForEntityCount(page, 3, 3000);

		// Start a new line away from the midpoint
		await clickAt(page, 0, -100);
		await page.waitForTimeout(200);

		// Move to the midpoint of the line
		await moveTo(page, 0, 30);
		await page.waitForTimeout(300);

		const snap = await page.evaluate(() => window.__waffle?.getSnapIndicator());
		expect(snap).not.toBeNull();
		expect(snap.type).toBe('midpoint');
	});

	test('midpoint snap includes entityId', async ({ waffle }) => {
		const page = waffle.page;
		await clickSketch(page);

		// Draw a line and wait for entities
		await drawLine(page, -80, 30, 80, 30);
		await waitForEntityCount(page, 3, 3000);

		// Find the line entity
		const entities = await getEntities(page);
		const line = entities.find(e => e.type === 'Line');
		expect(line).toBeTruthy();

		// Start a new line away
		await clickAt(page, 0, -100);
		await page.waitForTimeout(200);

		// Move to the midpoint
		await moveTo(page, 0, 30);
		await page.waitForTimeout(300);

		const snap = await page.evaluate(() => window.__waffle?.getSnapIndicator());
		expect(snap).not.toBeNull();
		expect(snap.type).toBe('midpoint');
		expect(typeof snap.entityId).toBe('number');
	});

	test('quadrant snap activates on circle', async ({ waffle }) => {
		const page = waffle.page;
		await clickSketch(page);

		// Switch to circle tool, draw a circle centered at origin with radius 50
		await clickCircle(page);
		await drawCircle(page, 0, 0, 50, 0);
		await waitForEntityCount(page, 2, 3000);

		// Switch to line tool
		await clickLine(page);

		// Start a line far away
		await clickAt(page, -200, -200);
		await page.waitForTimeout(200);

		// Move to the right quadrant point (0-degree, at x=50 from center)
		await moveTo(page, 50, 0);
		await page.waitForTimeout(300);

		const snap = await page.evaluate(() => window.__waffle?.getSnapIndicator());
		expect(snap).not.toBeNull();
		expect(snap.type).toBe('quadrant');
	});

	test('quadrant snap includes entityId', async ({ waffle }) => {
		const page = waffle.page;
		await clickSketch(page);

		// Draw a circle
		await clickCircle(page);
		await drawCircle(page, 0, 0, 50, 0);
		await waitForEntityCount(page, 2, 3000);

		// Get the circle entity
		const entities = await getEntities(page);
		const circle = entities.find(e => e.type === 'Circle');
		expect(circle).toBeTruthy();

		// Switch to line, start far away
		await clickLine(page);
		await clickAt(page, -200, -200);
		await page.waitForTimeout(200);

		// Move to the right quadrant
		await moveTo(page, 50, 0);
		await page.waitForTimeout(300);

		const snap = await page.evaluate(() => window.__waffle?.getSnapIndicator());
		expect(snap).not.toBeNull();
		expect(snap.type).toBe('quadrant');
		expect(typeof snap.entityId).toBe('number');
	});

	test('origin snap returns exactly x=0 y=0', async ({ waffle }) => {
		const page = waffle.page;
		await clickSketch(page);

		// Start a line far from origin
		await clickAt(page, -200, -200);
		await page.waitForTimeout(200);

		// Move to origin
		await moveTo(page, 0, 0);
		await page.waitForTimeout(300);

		const snap = await page.evaluate(() => window.__waffle?.getSnapIndicator());
		expect(snap).not.toBeNull();
		expect(snap.type).toBe('origin');
		// Strict equality — origin must return exact zeros
		expect(snap.x).toBe(0);
		expect(snap.y).toBe(0);
	});

	test('coincident wins over midpoint at endpoint', async ({ waffle }) => {
		const page = waffle.page;
		await clickSketch(page);

		// Draw a line from (-60, 0) to (60, 0)
		await drawLine(page, -60, 0, 60, 0);
		await waitForEntityCount(page, 3, 3000);

		// Start a new line far away
		await clickAt(page, 0, -150);
		await page.waitForTimeout(200);

		// Move to the start endpoint of the drawn line
		await moveTo(page, -60, 0);
		await page.waitForTimeout(300);

		const snap = await page.evaluate(() => window.__waffle?.getSnapIndicator());
		expect(snap).not.toBeNull();
		// Coincident (priority 1) should beat midpoint (priority 1c)
		expect(snap.type).toBe('coincident');
	});

	test('snap label shows Origin text', async ({ waffle }) => {
		const page = waffle.page;
		await clickSketch(page);

		// Start a line far from origin
		await clickAt(page, -200, -200);
		await page.waitForTimeout(200);

		// Move to origin
		await moveTo(page, 0, 0);
		await page.waitForTimeout(300);

		// Verify via API
		const snap = await page.evaluate(() => window.__waffle?.getSnapIndicator());
		expect(snap).not.toBeNull();
		expect(snap.type).toBe('origin');

		// Check DOM label if visible
		const snapLabel = page.locator('.snap-label');
		const isVisible = await snapLabel.isVisible();
		if (isVisible) {
			const text = await snapLabel.textContent();
			expect(text).toBe('Origin');
		}
	});

	test('snap label shows Midpoint text', async ({ waffle }) => {
		const page = waffle.page;
		await clickSketch(page);

		// Draw a line with midpoint at (0, 30)
		await drawLine(page, -80, 30, 80, 30);
		await waitForEntityCount(page, 3, 3000);

		// Start a new line away
		await clickAt(page, 0, -100);
		await page.waitForTimeout(200);

		// Move to the midpoint
		await moveTo(page, 0, 30);
		await page.waitForTimeout(300);

		// Verify via API
		const snap = await page.evaluate(() => window.__waffle?.getSnapIndicator());
		expect(snap).not.toBeNull();
		expect(snap.type).toBe('midpoint');

		// Check DOM label if visible
		const snapLabel = page.locator('.snap-label');
		const isVisible = await snapLabel.isVisible();
		if (isVisible) {
			const text = await snapLabel.textContent();
			expect(text).toBe('Midpoint');
		}
	});
});
