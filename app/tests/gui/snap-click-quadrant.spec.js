/**
 * Snap click quadrant tests — verifies that clicking at a detected quadrant
 * snap point actually places the circle center at the snapped coordinates.
 *
 * This is a 10x repeat test to expose intermittent precision issues in the
 * screen→sketch coordinate round-trip when clicking on quadrant snap points.
 */
import { test, expect } from './helpers/waffle-test.js';
import { clickSketch, clickCircle, pressKey } from './helpers/toolbar.js';
import { clickAt, moveTo } from './helpers/canvas.js';
import { waitForEntityCount, getEntities } from './helpers/state.js';

/** Quadrant points for a circle at (0,0) with radius 5 */
const QUADRANTS = [
	{ label: '0° (right)',   sx: 5,  sy: 0 },
	{ label: '90° (top)',    sx: 0,  sy: 5 },
	{ label: '180° (left)',  sx: -5, sy: 0 },
	{ label: '270° (bottom)', sx: 0,  sy: -5 },
];

/**
 * Helper: get screen offset from sketch coordinates via __waffle API.
 * Returns { x, y } pixel offset from canvas center, or null.
 */
async function sketchToOffset(page, sx, sy) {
	return page.evaluate(([x, y]) => {
		return window.__waffle?.sketchToScreenOffset?.(x, y) ?? null;
	}, [sx, sy]);
}

/**
 * Helper: get positions map as a plain object.
 */
async function getPositions(page) {
	return page.evaluate(() => {
		const posMap = window.__waffle.getPositions();
		const result = {};
		for (const [id, pos] of posMap) {
			result[id] = { x: pos.x, y: pos.y };
		}
		return result;
	});
}

test.describe('quadrant snap click — 10x repetition', () => {
	test('circle center placed at quadrant snap point — 10 reps', async ({ waffle }) => {
		const page = waffle.page;

		// 1. Enter sketch on front plane (XY, normal=[0,0,1])
		await clickSketch(page);

		// 2. Create a reference circle via API: center=(0,0), radius=5
		await page.evaluate(() => {
			const w = window.__waffle;
			w.addSketchEntity({ type: 'Point', id: 9001, x: 0, y: 0 });
			w.addSketchEntity({ type: 'Circle', id: 9002, center_id: 9001, radius: 5, construction: false });
		});
		await waitForEntityCount(page, 2, 3000);

		// Verify the reference circle exists
		const refEntities = await getEntities(page);
		const refCircle = refEntities.find(e => e.type === 'Circle' && e.id === 9002);
		expect(refCircle).toBeDefined();

		// 3. Run 10 iterations, cycling through 4 quadrants
		const failures = [];
		const TOLERANCE = 0.15; // sketch units — generous for screen→sketch round-trip

		for (let i = 0; i < 10; i++) {
			const q = QUADRANTS[i % 4];
			const rep = Math.floor(i / 4) + 1;
			const iterLabel = `rep ${rep}, ${q.label}`;

			// a. Switch to circle tool
			await clickCircle(page);

			// b. Get screen offset for the quadrant point
			const offset = await sketchToOffset(page, q.sx, q.sy);
			if (!offset) {
				failures.push({ iter: i, label: iterLabel, error: 'sketchToScreenOffset returned null' });
				await pressKey(page, 'Escape');
				continue;
			}

			// c. Move to snap point position and wait for snap detection
			await moveTo(page, offset.x, offset.y);
			await page.waitForTimeout(250);

			// d. Check snap indicator
			const snapBefore = await page.evaluate(() => window.__waffle?.getSnapIndicator());
			const snapDetected = snapBefore && snapBefore.type === 'quadrant';

			// e. Click at the snap point to place circle center
			await clickAt(page, offset.x, offset.y);
			await page.waitForTimeout(100);

			// Record entities before second click
			const entitiesAfterCenter = await getEntities(page);

			// f. Click somewhere else to set circle radius (20px away)
			await clickAt(page, offset.x + 30, offset.y);
			await page.waitForTimeout(200);

			// g. Get all entities — find the newest circle (highest id)
			const entitiesAfter = await getEntities(page);
			const circles = entitiesAfter.filter(e => e.type === 'Circle' && e.id !== 9002);

			if (circles.length === 0) {
				failures.push({
					iter: i,
					label: iterLabel,
					error: 'No new circle created',
					snapBefore,
					entityCount: entitiesAfter.length,
				});
				await pressKey(page, 'Escape');
				continue;
			}

			// Get the newest circle (highest ID)
			const newCircle = circles.reduce((a, b) => a.id > b.id ? a : b);

			// h. Get the center point position
			const positions = await getPositions(page);
			const centerPos = positions[newCircle.center_id];

			if (!centerPos) {
				failures.push({
					iter: i,
					label: iterLabel,
					error: `Center point ${newCircle.center_id} not in positions`,
					snapBefore,
				});
				await pressKey(page, 'Escape');
				continue;
			}

			// i. Verify the center is at the quadrant snap point
			const dx = Math.abs(centerPos.x - q.sx);
			const dy = Math.abs(centerPos.y - q.sy);
			const dist = Math.sqrt(dx * dx + dy * dy);

			if (dist > TOLERANCE) {
				failures.push({
					iter: i,
					label: iterLabel,
					error: `Center at (${centerPos.x.toFixed(3)}, ${centerPos.y.toFixed(3)}) — expected (${q.sx}, ${q.sy}), dist=${dist.toFixed(4)}`,
					snapDetected,
					snapBefore,
					offset,
				});
			}

			// j. Press Escape to reset tool for next iteration
			await pressKey(page, 'Escape');
		}

		// 4. Report all failures
		if (failures.length > 0) {
			const report = failures.map(f =>
				`[${f.label}] ${f.error}` +
				(f.snapDetected !== undefined ? ` (snap=${f.snapDetected})` : '') +
				(f.offset ? ` offset=(${f.offset.x.toFixed(1)}, ${f.offset.y.toFixed(1)})` : '')
			).join('\n');
			expect(failures, `${failures.length}/10 iterations failed:\n${report}`).toHaveLength(0);
		}
	});

	test('quadrant snap detected before click — all 4 quadrants', async ({ waffle }) => {
		const page = waffle.page;

		await clickSketch(page);

		// Create reference circle at origin with radius 5
		await page.evaluate(() => {
			const w = window.__waffle;
			w.addSketchEntity({ type: 'Point', id: 8001, x: 0, y: 0 });
			w.addSketchEntity({ type: 'Circle', id: 8002, center_id: 8001, radius: 5, construction: false });
		});
		await waitForEntityCount(page, 2, 3000);

		// Activate circle tool so snap detection runs on pointermove
		await clickCircle(page);

		for (const q of QUADRANTS) {
			const offset = await sketchToOffset(page, q.sx, q.sy);
			expect(offset, `sketchToScreenOffset failed for ${q.label}`).not.toBeNull();

			await moveTo(page, offset.x, offset.y);
			await page.waitForTimeout(300);

			const snap = await page.evaluate(() => window.__waffle?.getSnapIndicator());
			expect(snap, `No snap indicator for ${q.label} at offset (${offset.x.toFixed(1)}, ${offset.y.toFixed(1)})`).not.toBeNull();
			expect(snap.type, `Snap type for ${q.label} should be 'quadrant'`).toBe('quadrant');
			expect(snap.x).toBeCloseTo(q.sx, 1);
			expect(snap.y).toBeCloseTo(q.sy, 1);
		}
	});

	test('snap point reused when clicking exactly at quadrant', async ({ waffle }) => {
		const page = waffle.page;

		await clickSketch(page);

		// Create reference circle at origin with radius 5
		await page.evaluate(() => {
			const w = window.__waffle;
			w.addSketchEntity({ type: 'Point', id: 7001, x: 0, y: 0 });
			w.addSketchEntity({ type: 'Circle', id: 7002, center_id: 7001, radius: 5, construction: false });
		});
		await waitForEntityCount(page, 2, 3000);

		// Use line tool: draw a line starting from the right quadrant (5, 0)
		const offset = await sketchToOffset(page, 5, 0);
		expect(offset).not.toBeNull();

		await page.evaluate(() => window.__waffle?.setTool('line'));
		await page.waitForTimeout(200);

		// Move first to trigger snap detection
		await moveTo(page, offset.x, offset.y);
		await page.waitForTimeout(250);

		// Verify snap is detected
		const snap = await page.evaluate(() => window.__waffle?.getSnapIndicator());
		expect(snap).not.toBeNull();
		expect(snap.type).toBe('quadrant');

		// Click to place first point
		await clickAt(page, offset.x, offset.y);
		await page.waitForTimeout(200);

		// Click somewhere else to finish the line
		await clickAt(page, offset.x + 50, offset.y);
		await waitForEntityCount(page, 5, 5000); // 2 original + 2 points + 1 line

		// Verify the start point of the new line is at (5, 0)
		const positions = await getPositions(page);
		const entities = await getEntities(page);
		const lines = entities.filter(e => e.type === 'Line');
		expect(lines.length).toBeGreaterThanOrEqual(1);

		const newLine = lines[lines.length - 1];
		const startPos = positions[newLine.start_id];
		expect(startPos).toBeDefined();
		expect(startPos.x).toBeCloseTo(5, 0);
		expect(startPos.y).toBeCloseTo(0, 0);
	});
});
