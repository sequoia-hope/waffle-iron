/**
 * Snap hover indicator DOM tests — verifies .snap-label DOM element renders
 * when getSnapIndicator() returns non-null during active drawing.
 *
 * Bug: Old code imported getSnapIndicator from tools.js (plain .js) instead
 * of sketchToolState.svelte.js (.svelte.js with $state runes). This meant
 * Svelte 5's $derived.by(() => getSnapIndicator()) never re-ran, so
 * snapLabelData was always null and .snap-label never rendered in the DOM.
 *
 * These tests check DOM visibility (not just API data) to verify the
 * reactivity fix. The key discriminating assertion: .snap-label must be
 * visible in the DOM when the API reports a snap — with old code it never was.
 */
import { test, expect } from './helpers/waffle-test.js';
import { clickSketch } from './helpers/toolbar.js';
import { clickAt, drawLine, moveTo } from './helpers/canvas.js';
import {
	waitForEntityCount,
} from './helpers/state.js';

test.describe('snap hover indicator DOM visibility', () => {
	test('coincident snap label appears in DOM when hovering endpoint', async ({ waffle }) => {
		await clickSketch(waffle.page);

		// Draw a line to create snap targets (2 points + 1 line = 3 entities)
		await drawLine(waffle.page, -80, 0, 80, 0);
		await waitForEntityCount(waffle.page, 3, 3000);

		// Start a new line segment by clicking far away
		await clickAt(waffle.page, 0, 100);
		await waffle.page.waitForTimeout(200);

		// Hover over the start endpoint of the first line at (-80, 0)
		await moveTo(waffle.page, -80, 0);
		await waffle.page.waitForTimeout(500);

		// Check that API detects a snap
		const snapData = await waffle.page.evaluate(() =>
			window.__waffle?.getSnapIndicator()
		);

		// If the API detects a snap, the DOM label MUST be visible.
		// With old buggy code: API returns data but DOM never renders.
		if (snapData && snapData.type) {
			const snapLabel = waffle.page.locator('.snap-label');
			// This is THE key discriminating assertion:
			// Old code: .snap-label never visible (import from tools.js breaks reactivity)
			// New code: .snap-label visible when API returns snap data
			await expect(snapLabel).toBeVisible({ timeout: 2000 });

			const text = await snapLabel.textContent();
			expect(text).toBeTruthy();
			expect(['Coincident', 'Horizontal', 'Vertical', 'On Entity', 'Tangent', 'Perpendicular']).toContain(text);
		}
	});

	test('API non-null AND DOM visible are both true simultaneously', async ({ waffle }) => {
		await clickSketch(waffle.page);

		// Draw a line
		await drawLine(waffle.page, -80, 0, 80, 0);
		await waitForEntityCount(waffle.page, 3, 3000);

		// Start a second line
		await clickAt(waffle.page, -200, 100);
		await waffle.page.waitForTimeout(200);

		// Move to the endpoint of the first line
		await moveTo(waffle.page, 80, 0);
		await waffle.page.waitForTimeout(500);

		// Cross-check: both API and DOM must agree
		const result = await waffle.page.evaluate(() => {
			const snap = window.__waffle?.getSnapIndicator();
			const label = document.querySelector('.snap-label');
			return {
				apiHasSnap: snap !== null && snap.type !== undefined,
				apiType: snap?.type ?? null,
				domVisible: label !== null && label.offsetParent !== null,
				domText: label?.textContent ?? null,
			};
		});

		// If API reports a snap, DOM must also show it
		if (result.apiHasSnap) {
			expect(result.domVisible).toBe(true);
			expect(result.domText).toBeTruthy();
		}
		// If DOM shows a label, API must also have data
		if (result.domVisible) {
			expect(result.apiHasSnap).toBe(true);
		}
	});

	test('snap label disappears when moving away from targets', async ({ waffle }) => {
		await clickSketch(waffle.page);

		// Draw a line
		await drawLine(waffle.page, -80, 0, 80, 0);
		await waitForEntityCount(waffle.page, 3, 3000);

		// Start a second line
		await clickAt(waffle.page, 0, 100);
		await waffle.page.waitForTimeout(200);

		// Move near endpoint to trigger snap
		await moveTo(waffle.page, -80, 0);
		await waffle.page.waitForTimeout(500);

		const snapBefore = await waffle.page.evaluate(() =>
			window.__waffle?.getSnapIndicator()
		);

		// Move far away from all snap targets
		await moveTo(waffle.page, 0, -200);
		await waffle.page.waitForTimeout(500);

		// After moving away, snap should be gone
		const snapAfter = await waffle.page.evaluate(() =>
			window.__waffle?.getSnapIndicator()
		);

		// If snap was active before move, it should be cleared now
		if (snapBefore && snapBefore.type === 'coincident') {
			// API should return null or different type
			const isStillCoincident = snapAfter?.type === 'coincident';
			// The snap indicator at (-80, 0) should not still be active at (0, -200)
			expect(isStillCoincident).toBe(false);

			// DOM label should also disappear
			const labelVisible = await waffle.page.evaluate(() => {
				const label = document.querySelector('.snap-label');
				if (!label) return false;
				const text = label.textContent;
				return text === 'Coincident' && label.offsetParent !== null;
			});
			expect(labelVisible).toBe(false);
		}
	});

	test('no snap label in DOM when far from any targets', async ({ waffle }) => {
		await clickSketch(waffle.page);

		// Draw a line
		await drawLine(waffle.page, -80, 0, 80, 0);
		await waitForEntityCount(waffle.page, 3, 3000);

		// Start a second line far from existing geometry
		await clickAt(waffle.page, 0, 200);
		await waffle.page.waitForTimeout(200);

		// Move to a location far from any existing points
		await moveTo(waffle.page, 0, -200);
		await waffle.page.waitForTimeout(300);

		const snapData = await waffle.page.evaluate(() =>
			window.__waffle?.getSnapIndicator()
		);

		// If API says no coincident snap, DOM should have no Coincident label
		if (!snapData || snapData.type !== 'coincident') {
			const hasCoincidentLabel = await waffle.page.evaluate(() => {
				const label = document.querySelector('.snap-label');
				return label !== null && label.textContent === 'Coincident';
			});
			expect(hasCoincidentLabel).toBe(false);
		}
	});

	test('snap label text matches snap type from API', async ({ waffle }) => {
		await clickSketch(waffle.page);

		// Draw a line
		await drawLine(waffle.page, -80, 0, 80, 0);
		await waitForEntityCount(waffle.page, 3, 3000);

		// Start second line
		await clickAt(waffle.page, -200, 100);
		await waffle.page.waitForTimeout(200);

		// Move to endpoint
		await moveTo(waffle.page, -80, 0);
		await waffle.page.waitForTimeout(500);

		const result = await waffle.page.evaluate(() => {
			const snap = window.__waffle?.getSnapIndicator();
			const label = document.querySelector('.snap-label');
			const snapLabelMap = {
				'coincident': 'Coincident',
				'horizontal': 'Horizontal',
				'vertical': 'Vertical',
				'on-entity': 'On Entity',
				'tangent': 'Tangent',
				'perpendicular': 'Perpendicular',
			};
			return {
				apiType: snap?.type ?? null,
				expectedText: snap ? snapLabelMap[snap.type] : null,
				actualText: label?.textContent ?? null,
			};
		});

		// If API reports a snap with a known type, DOM text must match
		if (result.apiType && result.expectedText) {
			expect(result.actualText).toBe(result.expectedText);
		}
	});
});
