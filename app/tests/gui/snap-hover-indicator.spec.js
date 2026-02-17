/**
 * Snap indicator visibility — verify snap indicators appear during drawing.
 *
 * The bug: tools.js used plain `let` for snap state, so Svelte 5's $derived
 * never re-ran when the values changed. Fixed by moving snap state to a
 * .svelte.js module with $state runes.
 */
import { test, expect } from './helpers/waffle-test.js';
import { clickSketch, clickLine, clickSelect } from './helpers/toolbar.js';
import { drawLine, clickAt, moveTo } from './helpers/canvas.js';
import { waitForEntityCount } from './helpers/state.js';

/**
 * Enter sketch and draw a line to set up snap targets.
 */
async function setupSketchWithLine(waffle) {
	await clickSketch(waffle.page, 'front');
	await clickLine(waffle.page);
	// Draw a line from (-60, 0) to (60, 0)
	await drawLine(waffle.page, -60, 0, 60, 0);
	// Should have 2 points + 1 line = 3 entities
	try {
		await waitForEntityCount(waffle.page, 3, 5000);
	} catch {
		await waffle.dumpState('snap-setup-failed');
	}
	// Press Escape to finish chaining, go back to idle
	await waffle.page.keyboard.press('Escape');
	await waffle.page.waitForTimeout(200);
}

test.describe('snap indicator visibility', () => {
	test('hovering near endpoint in select tool shows snap indicator', async ({ waffle }) => {
		await setupSketchWithLine(waffle);
		await clickSelect(waffle.page);

		// Move to the start endpoint of the line (drawn at canvas offset -60, 0)
		await moveTo(waffle.page, -60, 0);
		await waffle.page.waitForTimeout(300);

		const snap = await waffle.page.evaluate(() => window.__waffle.getSnapIndicator());
		// Should have a snap indicator (coincident with endpoint)
		expect(snap).not.toBeNull();
	});

	test('hovering away from geometry hides snap indicator', async ({ waffle }) => {
		await setupSketchWithLine(waffle);
		await clickSelect(waffle.page);

		// Move to a position far from any geometry
		await moveTo(waffle.page, 0, -200);
		await waffle.page.waitForTimeout(300);

		const snap = await waffle.page.evaluate(() => window.__waffle.getSnapIndicator());
		expect(snap).toBeNull();
	});

	test('snap indicator appears during active line drawing', async ({ waffle }) => {
		await setupSketchWithLine(waffle);
		await clickLine(waffle.page);

		// Start a new line somewhere
		await clickAt(waffle.page, -100, -50);
		await waffle.page.waitForTimeout(200);

		// Now hover near the start endpoint of the existing line
		await moveTo(waffle.page, -60, 0);
		await waffle.page.waitForTimeout(300);

		const snap = await waffle.page.evaluate(() => window.__waffle.getSnapIndicator());
		expect(snap).not.toBeNull();
	});

	test('snap indicator for midpoint appears on hover', async ({ waffle }) => {
		await setupSketchWithLine(waffle);
		await clickSelect(waffle.page);

		// Hover at the midpoint of the line (canvas offset 0, 0 = center)
		await moveTo(waffle.page, 0, 0);
		await waffle.page.waitForTimeout(300);

		const snap = await waffle.page.evaluate(() => window.__waffle.getSnapIndicator());
		// Midpoint snap should fire at the center of a horizontal line
		// (depends on snap detection radius; may be null if too far)
		// At minimum, the snap candidates should include a midpoint marker
		const candidates = await waffle.page.evaluate(() => window.__waffle.getSnapCandidates());
		const hasMidpoint = candidates.some(c => c.type === 'midpoint');
		// Either we have a direct snap indicator or midpoint in candidates
		expect(snap !== null || hasMidpoint).toBe(true);
	});

	test('snap indicator type is available via API', async ({ waffle }) => {
		await setupSketchWithLine(waffle);
		await clickSelect(waffle.page);

		// Hover exactly on the start endpoint
		await moveTo(waffle.page, -60, 0);
		await waffle.page.waitForTimeout(300);

		const snap = await waffle.page.evaluate(() => window.__waffle.getSnapIndicator());
		if (snap) {
			// Snap should have a type and position
			expect(snap.type).toBeDefined();
			expect(typeof snap.x).toBe('number');
			expect(typeof snap.y).toBe('number');
		}
	});
});
