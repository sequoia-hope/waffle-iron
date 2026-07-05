/**
 * Snap preview candidate tests — verifies collectSnapCandidates() returns
 * faint preview markers for snap targets near the cursor via the
 * window.__waffle.getSnapCandidates() API.
 *
 * Candidates are all snap targets within the preview radius. The active
 * snap (from getSnapIndicator()) is filtered OUT of the candidate list.
 */
import { test, expect } from './helpers/waffle-test.js';
import { clickSketch, clickLine, clickSelect, clickCircle } from './helpers/toolbar.js';
import { clickAt, drawLine, drawCircle, moveTo } from './helpers/canvas.js';
import { waitForEntityCount } from './helpers/state.js';

test.describe('snap preview candidates', () => {
	test('candidates include origin near (0,0)', async ({ waffle }) => {
		const page = waffle.page;
		await clickSketch(page);

		// Increase preview radius so candidates are reliably found
		await page.evaluate(() => window.__waffle.updateSnapSettings({ previewPx: 100 }));

		// Start a line far away from origin
		await clickAt(page, -200, -200);
		await page.waitForTimeout(200);

		// Move near origin (canvas center = sketch origin)
		await moveTo(page, 15, 15);
		await page.waitForTimeout(300);

		const candidates = await page.evaluate(() =>
			window.__waffle.getSnapCandidates()
		);
		expect(Array.isArray(candidates)).toBe(true);
		expect(candidates.some(c => c.type === 'origin' && c.x === 0 && c.y === 0)).toBe(true);
	});

	test('candidates include point type for existing points', async ({ waffle }) => {
		const page = waffle.page;
		await clickSketch(page);

		// Increase preview radius so candidates are reliably found
		await page.evaluate(() => window.__waffle.updateSnapSettings({ previewPx: 100 }));

		// Draw a horizontal line to create endpoints
		await drawLine(page, -80, 0, 80, 0);
		await waitForEntityCount(page, 3, 3000);

		// Start a new line far from existing geometry
		await clickAt(page, 0, -150);
		await page.waitForTimeout(200);

		// Move between the two endpoints — should be near both + midpoint + origin
		// We use an offset from center to be near one endpoint but not exactly on it
		await moveTo(page, -60, 15);
		await page.waitForTimeout(300);

		const { candidates, indicator } = await page.evaluate(() => ({
			candidates: window.__waffle.getSnapCandidates(),
			indicator: window.__waffle.getSnapIndicator(),
		}));
		expect(Array.isArray(candidates)).toBe(true);
		// Point candidates should exist (line endpoints are within the preview radius)
		// Note: if the active snap is coincident, the snapped-to point is filtered out,
		// but other point candidates should still be present
		const hasPoint = candidates.some(c => c.type === 'point' && typeof c.entityId === 'number');
		// If active snap is coincident to a point, we still expect OTHER points as candidates
		expect(hasPoint).toBe(true);
	});

	test('candidates include midpoint for lines', async ({ waffle }) => {
		const page = waffle.page;
		await clickSketch(page);

		// Increase preview radius so candidates are reliably found
		await page.evaluate(() => window.__waffle.updateSnapSettings({ previewPx: 100 }));

		// Draw a horizontal line at y=40
		await drawLine(page, -80, 40, 80, 40);
		await waitForEntityCount(page, 3, 3000);

		// Start a new line away from the drawn line
		await clickAt(page, 0, -150);
		await page.waitForTimeout(200);

		// Move near the midpoint of the line (0, 40)
		await moveTo(page, 10, 40);
		await page.waitForTimeout(300);

		const candidates = await page.evaluate(() =>
			window.__waffle.getSnapCandidates()
		);
		expect(Array.isArray(candidates)).toBe(true);
		expect(candidates.some(c => c.type === 'midpoint')).toBe(true);
	});

	test('candidates include quadrant for circles', async ({ waffle }) => {
		const page = waffle.page;
		await clickSketch(page);

		// Increase preview radius so candidates are reliably found
		await page.evaluate(() => window.__waffle.updateSnapSettings({ previewPx: 100 }));

		// Draw a circle centered at origin with radius 50
		await clickCircle(page);
		await drawCircle(page, 0, 0, 50, 0);
		await waitForEntityCount(page, 2, 3000);

		// Switch to line tool and start far away
		await clickLine(page);
		await clickAt(page, -200, -200);
		await page.waitForTimeout(200);

		// Move near the 0-degree quadrant point (50, 0)
		await moveTo(page, 45, 5);
		await page.waitForTimeout(300);

		const candidates = await page.evaluate(() =>
			window.__waffle.getSnapCandidates()
		);
		expect(Array.isArray(candidates)).toBe(true);
		expect(candidates.some(c => c.type === 'quadrant')).toBe(true);
	});

	test('candidates empty far from all targets', async ({ waffle }) => {
		const page = waffle.page;
		await clickSketch(page);

		// Start a line far from center
		await clickAt(page, -200, -200);
		await page.waitForTimeout(200);

		// Move even further from any geometry
		await moveTo(page, -250, 250);
		await page.waitForTimeout(300);

		const candidates = await page.evaluate(() =>
			window.__waffle.getSnapCandidates()
		);
		expect(Array.isArray(candidates)).toBe(true);
		expect(candidates.length).toBe(0);
	});

	// O8 (adjudicated repair — specs/snap_inference_and_priority.md §"Candidate-
	// dedup + spec:143 repair"). The original used a 0.5 sketch-unit tolerance,
	// ~300× the whole drawing, so it flagged every candidate as the active
	// marker. Intent preserved with PIXEL-DERIVED tolerances (CANDIDATE_DEDUP_PX
	// = 4px, via getSketchPixelSize): the active snap's own marker is absent from
	// the candidates, while DISTINCT nearby candidates (a point 6px away, plus
	// origin and midpoint) are retained. RED on current code: the hardcoded
	// 0.001-sketch-unit filter (~10.5px at this zoom) OVER-filters, wrongly
	// discarding the distinct 6px point.
	test('active snap filtered from candidates', async ({ waffle }) => {
		const page = waffle.page;
		await clickSketch(page);

		// Increase preview radius
		await page.evaluate(() => window.__waffle.updateSnapSettings({ previewPx: 100 }));

		// Draw a horizontal line to create endpoints (fixes the sketch zoom).
		await drawLine(page, -80, 0, 80, 0);
		await waitForEntityCount(page, 3, 3000);

		// A DISTINCT snap target 6px to the right of the left endpoint — beyond the
		// 4px dedup radius, so it must be retained. (Inside the line's bbox → no
		// re-fit, so the zoom/pixel size is unchanged.)
		const ps = await page.evaluate(() => window.__waffle.getSketchPixelSize());
		const leftX = -80 * ps;
		const nearbyX = leftX + 6 * ps;
		await page.evaluate((x) => {
			window.__waffle.addSketchEntity({ type: 'Point', id: 90001, x, y: 0, construction: false });
		}, nearbyX);
		await page.waitForTimeout(150);

		// Start a new line far away, then move onto the left endpoint → active snap.
		await clickAt(page, 0, -150);
		await page.waitForTimeout(200);
		await moveTo(page, -80, 0);
		await page.waitForTimeout(300);

		const { indicator, candidates, pxSize } = await page.evaluate(() => ({
			indicator: window.__waffle.getSnapIndicator(),
			candidates: window.__waffle.getSnapCandidates(),
			pxSize: window.__waffle.getSketchPixelSize(),
		}));

		expect(indicator, 'an active snap is shown at the left endpoint').not.toBeNull();
		expect(indicator.type).toBe('coincident');
		expect(Array.isArray(candidates)).toBe(true);

		// Pixel-derived dedup radius (CANDIDATE_DEDUP_PX = 4).
		const dedupTol = 4 * pxSize;

		// (1) The active snap's OWN marker is absent (deduped).
		const activeMarkerPresent = candidates.some(
			(c) => Math.hypot(c.x - indicator.x, c.y - indicator.y) < dedupTol
		);
		expect(activeMarkerPresent, "active snap's own marker is not duplicated in candidates").toBe(false);

		// (2) The DISTINCT 6px-away point is retained (it is not the active marker).
		const nearbyRetained = candidates.some(
			(c) => c.type === 'point' && Math.hypot(c.x - nearbyX, c.y - 0) < dedupTol
		);
		expect(nearbyRetained, 'a distinct candidate 6px from the active snap is retained').toBe(true);

		// (3) The origin and the line midpoint remain candidates.
		expect(candidates.some((c) => c.type === 'origin'), 'origin candidate retained').toBe(true);
		expect(candidates.some((c) => c.type === 'midpoint'), 'midpoint candidate retained').toBe(true);
	});

	test('multiple candidate types simultaneously', async ({ waffle }) => {
		const page = waffle.page;
		await clickSketch(page);

		// Use a generous preview radius to capture multiple types
		await page.evaluate(() => window.__waffle.updateSnapSettings({ previewPx: 200 }));

		// Draw a line near origin — creates points, midpoint, AND is near origin
		await drawLine(page, -20, -20, 20, 20);
		await waitForEntityCount(page, 3, 3000);

		// Start a new line far away
		await clickAt(page, -200, -200);
		await page.waitForTimeout(200);

		// Move near origin and line geometry — multiple candidate types expected
		await moveTo(page, 5, 5);
		await page.waitForTimeout(300);

		const candidates = await page.evaluate(() =>
			window.__waffle.getSnapCandidates()
		);
		expect(Array.isArray(candidates)).toBe(true);
		const types = new Set(candidates.map(c => c.type));
		expect(types.size).toBeGreaterThanOrEqual(2);
	});

	test('every candidate has valid shape', async ({ waffle }) => {
		const page = waffle.page;
		await clickSketch(page);

		// Increase preview radius to get candidates
		await page.evaluate(() => window.__waffle.updateSnapSettings({ previewPx: 100 }));

		// Draw a line to create snap targets
		await drawLine(page, -60, 0, 60, 0);
		await waitForEntityCount(page, 3, 3000);

		// Start a new line far away
		await clickAt(page, -200, -200);
		await page.waitForTimeout(200);

		// Move near geometry
		await moveTo(page, 5, 5);
		await page.waitForTimeout(300);

		const candidates = await page.evaluate(() =>
			window.__waffle.getSnapCandidates()
		);
		expect(Array.isArray(candidates)).toBe(true);
		// With generous preview, we should have at least origin candidate
		expect(candidates.length).toBeGreaterThan(0);

		const validTypes = ['origin', 'point', 'midpoint', 'quadrant'];
		for (const c of candidates) {
			expect(typeof c.type).toBe('string');
			expect(validTypes).toContain(c.type);
			expect(typeof c.x).toBe('number');
			expect(typeof c.y).toBe('number');
		}
	});

	test('candidates update reactively on move', async ({ waffle }) => {
		const page = waffle.page;
		await clickSketch(page);

		// Use generous preview radius so near-geometry positions find candidates
		await page.evaluate(() => window.__waffle.updateSnapSettings({ previewPx: 100 }));

		// Draw a line to create snap targets
		await drawLine(page, -60, 0, 60, 0);
		await waitForEntityCount(page, 3, 3000);

		// Start a new line far away
		await clickAt(page, -200, -200);
		await page.waitForTimeout(200);

		// Move near origin — should find candidates (origin + nearby points/midpoint)
		await moveTo(page, 5, 5);
		await page.waitForTimeout(300);
		const first = await page.evaluate(() =>
			window.__waffle.getSnapCandidates()
		);

		// Move far from everything — should have fewer or no candidates
		await moveTo(page, -300, 300);
		await page.waitForTimeout(300);
		const second = await page.evaluate(() =>
			window.__waffle.getSnapCandidates()
		);

		expect(Array.isArray(first)).toBe(true);
		expect(Array.isArray(second)).toBe(true);
		expect(first.length).toBeGreaterThan(second.length);
	});

	test('candidates work in select tool', async ({ waffle }) => {
		const page = waffle.page;
		await clickSketch(page);

		// Use generous preview radius so candidates are found in select mode
		await page.evaluate(() => window.__waffle.updateSnapSettings({ previewPx: 200 }));

		// Draw a line to create snap targets
		await drawLine(page, -60, 0, 60, 0);
		await waitForEntityCount(page, 3, 3000);

		// Switch to select tool
		await clickSelect(page);

		// Move near origin — should find origin + nearby point candidates
		await moveTo(page, 5, 5);
		await page.waitForTimeout(300);

		const candidates = await page.evaluate(() =>
			window.__waffle.getSnapCandidates()
		);
		expect(Array.isArray(candidates)).toBe(true);
		expect(candidates.length).toBeGreaterThan(0);
	});
});
