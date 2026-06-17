/**
 * Ring-gear hover/region: a gear with a concentric bore must expose the BODY
 * (gear minus bore) as a selectable ring region, not the whole gear.
 *
 * Regression: region computation skipped Gear entities (they are stored
 * compact, unexpanded), so a gear sketch produced no/incorrect regions and the
 * hover shaded the whole entity. The fix expands gears before ComputeRegions.
 */
import { test, expect } from './helpers/waffle-test.js';
import { clickSketch, clickFinishSketch, clickExtrude } from './helpers/toolbar.js';
import { clickAt } from './helpers/canvas.js';
import {
	collectCrashErrors,
	expectNoAnyCrash,
	waitForFeatureCount,
} from './helpers/state.js';

async function pressKey(page, key) {
	await page.keyboard.press(key);
}

test('a gear with a bore exposes a ring region (gear minus bore)', async ({ waffle }) => {
	const page = waffle.page;
	const crashes = collectCrashErrors(page);

	await clickSketch(page);

	// Create a gear at the canvas center via the gear dialog.
	await pressKey(page, 'g');
	await page.waitForFunction(() => window.__waffle?.getState()?.activeTool === 'gear', {
		timeout: 3000,
	});
	await clickAt(page, 0, 0);
	const dialog = page.locator('[data-testid="gear-dialog"]');
	await dialog.waitFor({ state: 'visible', timeout: 5000 });
	await page.locator('[data-testid="gear-teeth-input"]').fill('12');
	await page.waitForTimeout(150);
	await page.locator('[data-testid="gear-apply-btn"]').click();
	await dialog.waitFor({ state: 'hidden', timeout: 5000 });

	// Add a concentric bore well inside the gear (setup via API — the feature
	// under test is region selection, not drawing). Center = mean of the gear's
	// expansion points; radius = 0.3·pitch radius.
	const bore = await page.evaluate(() => {
		const disp = Object.values(window.__waffle.getGearDisplay())[0];
		const pts = disp.entities.filter((e) => e.type === 'Point');
		const cx = pts.reduce((s, p) => s + p.x, 0) / pts.length;
		const cy = pts.reduce((s, p) => s + p.y, 0) / pts.length;
		window.__waffle.addSketchEntity({ type: 'Point', id: 800001, x: cx, y: cy, construction: false });
		window.__waffle.addSketchEntity({
			type: 'Circle',
			id: 800000,
			center_id: 800001,
			radius: disp.pitchRadius * 0.3,
			construction: false,
		});
		return { cx, cy };
	});
	expect(Number.isFinite(bore.cx)).toBe(true);

	await clickFinishSketch(page);
	await waitForFeatureCount(page, 1, 10000);

	// Entering extrude triggers region computation; the gear sketch must yield a
	// ring region (a face WITH a hole) — proving the gear was expanded.
	await clickExtrude(page);
	await page.waitForFunction(
		() => {
			const tree = window.__waffle.getFeatureTree();
			const sketch = tree?.features?.find((f) => f.operation?.type === 'Sketch');
			if (!sketch) return false;
			const regions = window.__waffle.getSketchRegions(sketch.id);
			return Array.isArray(regions) && regions.some((r) => (r.holes?.length ?? 0) > 0);
		},
		{ timeout: 6000 }
	);

	expectNoAnyCrash(crashes);
});
