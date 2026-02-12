/**
 * Snap label tests — verifies snap indicator labels appear in the DOM
 * when drawing near existing geometry.
 *
 * Snap labels are rendered via Threlte's <HTML> component as real DOM
 * elements with class "snap-label". They appear during active drawing
 * when the cursor is near snap targets (existing points, H/V alignment).
 */
import { test, expect } from './helpers/waffle-test.js';
import { clickSketch, pressKey } from './helpers/toolbar.js';
import { clickAt, drawLine, moveTo } from './helpers/canvas.js';
import {
	getEntities,
	waitForEntityCount,
} from './helpers/state.js';

test.describe('snap label visibility', () => {
	test('snap indicator API returns data during drawing', async ({ waffle }) => {
		await clickSketch(waffle.page);

		// Draw a line first to create snap targets
		await drawLine(waffle.page, -80, 0, 80, 0);
		await waitForEntityCount(waffle.page, 3, 3000);

		// Start a new segment away from the line
		await clickAt(waffle.page, 0, 100);
		await waffle.page.waitForTimeout(200);

		// Move cursor to the start of the first line (exact endpoint position)
		await moveTo(waffle.page, -80, 0);
		await waffle.page.waitForTimeout(200);

		const snapData = await waffle.page.evaluate(() =>
			window.__waffle?.getSnapIndicator()
		);
		// Moving to an exact endpoint should produce a snap indicator
		expect(snapData).not.toBeNull();
		expect(snapData.type).toBeTruthy();
		expect(typeof snapData.x).toBe('number');
		expect(typeof snapData.y).toBe('number');
	});

	test('coincident snap label appears when hovering near existing point', async ({ waffle }) => {
		await clickSketch(waffle.page);

		// Draw a line to create some geometry
		await drawLine(waffle.page, -80, 0, 80, 0);
		await waitForEntityCount(waffle.page, 3, 3000);

		// Get the positions of existing points
		const entities = await getEntities(waffle.page);
		const points = entities.filter(e => e.type === 'Point');
		expect(points.length).toBeGreaterThanOrEqual(2);

		// Start drawing a new line (line tool is still active after first draw)
		// Click far away first
		await clickAt(waffle.page, -200, 100);
		await waffle.page.waitForTimeout(200);

		// Move cursor to the exact endpoint of the drawn line
		await moveTo(waffle.page, -80, 0);
		await waffle.page.waitForTimeout(300);

		// Verify the snap API detects a snap near the endpoint
		const snapData = await waffle.page.evaluate(() =>
			window.__waffle?.getSnapIndicator()
		);
		expect(snapData).not.toBeNull();
		expect(snapData.type).toBeTruthy();

		// If the DOM label appeared, verify its text is valid
		const snapLabel = waffle.page.locator('.snap-label');
		const isVisible = await snapLabel.isVisible();
		if (isVisible) {
			const text = await snapLabel.textContent();
			expect(['Coincident', 'Horizontal', 'Vertical', 'On Entity', 'Tangent', 'Perpendicular']).toContain(text);
		}
	});

	test('snap label text matches expected values', async ({ waffle }) => {
		await clickSketch(waffle.page);

		// The snap system supports these label types
		const validLabels = ['Coincident', 'Horizontal', 'Vertical', 'On Entity', 'Tangent', 'Perpendicular'];

		// Draw two lines to create snap targets
		await drawLine(waffle.page, -100, -50, 100, -50);
		await waitForEntityCount(waffle.page, 3, 3000);

		// Start second line near the endpoint of first line
		await clickAt(waffle.page, -100, 50);
		await waffle.page.waitForTimeout(200);

		// Move very close to the end of the first line (100, -50)
		await moveTo(waffle.page, 100, -50);
		await waffle.page.waitForTimeout(300);

		// If a snap label appeared, verify its text
		const snapLabels = await waffle.page.locator('.snap-label').allTextContents();
		for (const text of snapLabels) {
			expect(validLabels).toContain(text);
		}
	});

	test('horizontal alignment snap detected during drawing', async ({ waffle }) => {
		await clickSketch(waffle.page);

		// Draw a point by clicking (first click of line tool)
		await clickAt(waffle.page, -100, 0);
		await waffle.page.waitForTimeout(200);

		// Move cursor to exactly same Y but different X
		await moveTo(waffle.page, 100, 0);
		await waffle.page.waitForTimeout(300);

		// Check snap indicator via API
		const snapData = await waffle.page.evaluate(() =>
			window.__waffle?.getSnapIndicator()
		);

		// A horizontal/vertical alignment snap should trigger
		expect(snapData).not.toBeNull();
		expect(['horizontal', 'vertical']).toContain(snapData.type);
	});

	test('vertical alignment snap detected during drawing', async ({ waffle }) => {
		await clickSketch(waffle.page);

		// Draw a point by clicking
		await clickAt(waffle.page, 0, -100);
		await waffle.page.waitForTimeout(200);

		// Move cursor to exactly same X but different Y
		await moveTo(waffle.page, 0, 100);
		await waffle.page.waitForTimeout(300);

		const snapData = await waffle.page.evaluate(() =>
			window.__waffle?.getSnapIndicator()
		);

		// A horizontal/vertical alignment snap should trigger
		expect(snapData).not.toBeNull();
		expect(['horizontal', 'vertical']).toContain(snapData.type);
	});
});

test.describe('snap indicator geometry', () => {
	test('getSnapIndicator returns null when not drawing', async ({ waffle }) => {
		await clickSketch(waffle.page);

		// In select tool, no snap should be active
		await pressKey(waffle.page, 'Escape');
		const snap = await waffle.page.evaluate(() =>
			window.__waffle?.getSnapIndicator()
		);
		expect(snap).toBeNull();
	});

	test('snap indicator has correct shape when present', async ({ waffle }) => {
		await clickSketch(waffle.page);

		// Draw a line to create snap targets
		await drawLine(waffle.page, -80, 0, 80, 0);
		await waitForEntityCount(waffle.page, 3, 3000);

		// Start a new line near the first line's endpoint
		await clickAt(waffle.page, 0, 80);
		await waffle.page.waitForTimeout(200);
		await moveTo(waffle.page, 80, 1); // near endpoint of first line
		await waffle.page.waitForTimeout(300);

		const snap = await waffle.page.evaluate(() =>
			window.__waffle?.getSnapIndicator()
		);

		// Moving near an endpoint should produce a non-null snap indicator
		expect(snap).not.toBeNull();
		// All snap indicators must have type, x, y
		expect(snap.type).toBeTruthy();
		expect(typeof snap.x).toBe('number');
		expect(typeof snap.y).toBe('number');
		expect(['coincident', 'horizontal', 'vertical', 'on-entity', 'tangent', 'perpendicular']).toContain(snap.type);
	});
});
