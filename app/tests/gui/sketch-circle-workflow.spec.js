/**
 * End-to-end circle sketch workflow:
 *   select plane → start sketch → draw circle → finish sketch → feature tree
 */
import { test, expect } from './helpers/waffle-test.js';
import { clickSketch, clickCircle, clickFinishSketch } from './helpers/toolbar.js';
import { drawCircle } from './helpers/canvas.js';
import {
	isSketchActive,
	getActiveTool,
	getEntityCountByType,
	getFeatureCount,
	hasFeatureOfType,
	waitForEntityCount,
	waitForFeatureCount,
} from './helpers/state.js';

test.describe('circle sketch end-to-end workflow', () => {
	test('draw circle then finish sketch creates a Sketch feature', async ({ waffle }) => {
		// 1. Enter sketch mode
		await clickSketch(waffle.page);
		const sketchActive = await isSketchActive(waffle.page);
		expect(sketchActive).toBe(true);

		// 2. Switch to circle tool
		await clickCircle(waffle.page);
		const tool = await getActiveTool(waffle.page);
		expect(tool).toBe('circle');

		// 3. Draw circle: click center (0,0), then edge point (60,0)
		await drawCircle(waffle.page, 0, 0, 60, 0);

		// 4. Wait for entities: 1 Point (center) + 1 Circle
		try {
			await waitForEntityCount(waffle.page, 2, 3000);
		} catch {
			await waffle.dumpState('circle-workflow-draw-failed');
		}

		const circles = await getEntityCountByType(waffle.page, 'Circle');
		expect(circles).toBeGreaterThanOrEqual(1);

		const points = await getEntityCountByType(waffle.page, 'Point');
		expect(points).toBeGreaterThanOrEqual(1);

		// 5. Click Finish Sketch — this is where the circle profile bug manifested
		await clickFinishSketch(waffle.page);

		// 6. Verify sketch mode exited
		const activeAfter = await isSketchActive(waffle.page);
		expect(activeAfter).toBe(false);

		// 7. Verify feature tree has a Sketch feature
		try {
			await waitForFeatureCount(waffle.page, 1, 10000);
		} catch {
			await waffle.dumpState('circle-workflow-feature-failed');
		}

		const featureCount = await getFeatureCount(waffle.page);
		expect(featureCount).toBeGreaterThanOrEqual(1);

		const hasSketch = await hasFeatureOfType(waffle.page, 'Sketch');
		expect(hasSketch).toBe(true);
	});
});
