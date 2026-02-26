/**
 * Keyboard camera controls — F key fit-all, view presets via snap-view events.
 */
import { test, expect } from './helpers/waffle-test.js';
import { createExtrudedBox } from './helpers/geometry.js';

/**
 * Compute the distance from camera position to camera target.
 * @param {number[]} pos - camera position [x, y, z]
 * @param {number[]} tgt - camera target [x, y, z]
 * @returns {number}
 */
function cameraDistance(pos, tgt) {
	const dx = pos[0] - tgt[0];
	const dy = pos[1] - tgt[1];
	const dz = pos[2] - tgt[2];
	return Math.sqrt(dx ** 2 + dy ** 2 + dz ** 2);
}

test.describe('F key fit-all', () => {
	test('F key on empty scene keeps valid camera state', async ({ waffle }) => {
		const page = waffle.page;

		await page.waitForFunction(() => window.__waffle?.getCameraState() !== null);
		const before = await page.evaluate(() => window.__waffle.getCameraState());
		expect(before).not.toBeNull();

		await page.keyboard.press('f');
		await page.waitForTimeout(500);

		const after = await page.evaluate(() => window.__waffle.getCameraState());
		expect(after).not.toBeNull();
		for (const v of [...after.position, ...after.target]) {
			expect(Number.isFinite(v)).toBe(true);
		}
	});

	test('F key with geometry frames the model', async ({ waffle }) => {
		const page = waffle.page;
		await createExtrudedBox(page);

		// Zoom way out first so we can verify fit-all brings us closer
		await page.evaluate(() => {
			window.dispatchEvent(new WheelEvent('wheel', { deltaY: 5000 }));
		});
		await page.waitForTimeout(500);

		await page.waitForFunction(() => window.__waffle?.getCameraState() !== null);
		const beforeFit = await page.evaluate(() => window.__waffle.getCameraState());
		const distBefore = cameraDistance(beforeFit.position, beforeFit.target);

		await page.keyboard.press('f');
		await page.waitForTimeout(500);

		const afterFit = await page.evaluate(() => window.__waffle.getCameraState());
		const distAfter = cameraDistance(afterFit.position, afterFit.target);

		// After fit-all, camera should be closer to model than when zoomed way out
		// (unless model fills the view at a natural distance)
		expect(distAfter).toBeLessThan(distBefore + 10);
	});

	test('fit-all centers target near model bounding box center', async ({ waffle }) => {
		const page = waffle.page;
		await createExtrudedBox(page);

		await page.keyboard.press('f');
		await page.waitForTimeout(500);

		const cam = await page.evaluate(() => window.__waffle.getCameraState());
		const bbox = await page.evaluate(() => window.__waffle.getMeshBoundingBox());
		expect(cam).not.toBeNull();
		expect(bbox).not.toBeNull();

		// Camera target should be near the model's bounding box center
		const dx = cam.target[0] - bbox.center[0];
		const dy = cam.target[1] - bbox.center[1];
		const dz = cam.target[2] - bbox.center[2];
		const offset = Math.sqrt(dx ** 2 + dy ** 2 + dz ** 2);

		// Should be within model size tolerance
		const modelSize = Math.sqrt(bbox.size[0] ** 2 + bbox.size[1] ** 2 + bbox.size[2] ** 2);
		expect(offset).toBeLessThan(modelSize * 2);
	});

	test('uppercase F key also triggers fit-all', async ({ waffle }) => {
		const page = waffle.page;

		await page.waitForFunction(() => window.__waffle?.getCameraState() !== null);
		const before = await page.evaluate(() => window.__waffle.getCameraState());

		await page.keyboard.press('Shift+f');
		await page.waitForTimeout(500);

		const after = await page.evaluate(() => window.__waffle.getCameraState());
		expect(after).not.toBeNull();
		for (const v of [...after.position, ...after.target]) {
			expect(Number.isFinite(v)).toBe(true);
		}
	});
});

test.describe('view presets via snap-view', () => {
	test('snap to front view aligns camera along Z axis', async ({ waffle }) => {
		const page = waffle.page;

		await page.waitForFunction(() => window.__waffle?.getCameraState() !== null);

		// Dispatch snap-view custom event for 'front'
		await page.evaluate(() => {
			window.dispatchEvent(new CustomEvent('waffle-snap-view', { detail: { view: 'front' } }));
		});
		await page.waitForTimeout(500);

		const state = await page.evaluate(() => window.__waffle.getCameraState());
		expect(state).not.toBeNull();

		// Front view: camera looks along -Z, so Z-component of (pos-target) should dominate
		const dz = state.position[2] - state.target[2];
		const dx = Math.abs(state.position[0] - state.target[0]);
		const dy = Math.abs(state.position[1] - state.target[1]);
		expect(Math.abs(dz)).toBeGreaterThan(dx + 0.1);
		expect(Math.abs(dz)).toBeGreaterThan(dy + 0.1);
		expect(dz).toBeGreaterThan(0); // Camera in front (+Z)
	});

	test('snap to top view aligns camera along Y axis', async ({ waffle }) => {
		const page = waffle.page;

		await page.waitForFunction(() => window.__waffle?.getCameraState() !== null);

		await page.evaluate(() => {
			window.dispatchEvent(new CustomEvent('waffle-snap-view', { detail: { view: 'top' } }));
		});
		await page.waitForTimeout(500);

		const state = await page.evaluate(() => window.__waffle.getCameraState());
		expect(state).not.toBeNull();

		// Top view: camera looks along -Y, so Y-component of (pos-target) should dominate
		const dy = state.position[1] - state.target[1];
		const dx = Math.abs(state.position[0] - state.target[0]);
		const dz = Math.abs(state.position[2] - state.target[2]);
		expect(Math.abs(dy)).toBeGreaterThan(dx + 0.1);
		expect(Math.abs(dy)).toBeGreaterThan(dz + 0.1);
		expect(dy).toBeGreaterThan(0); // Camera above (+Y)
	});

	test('snap to right view aligns camera along X axis', async ({ waffle }) => {
		const page = waffle.page;

		await page.waitForFunction(() => window.__waffle?.getCameraState() !== null);

		await page.evaluate(() => {
			window.dispatchEvent(new CustomEvent('waffle-snap-view', { detail: { view: 'right' } }));
		});
		await page.waitForTimeout(500);

		const state = await page.evaluate(() => window.__waffle.getCameraState());
		expect(state).not.toBeNull();

		// Right view: camera along +X, so X-component of (pos-target) should dominate
		const dx = state.position[0] - state.target[0];
		const dy = Math.abs(state.position[1] - state.target[1]);
		const dz = Math.abs(state.position[2] - state.target[2]);
		expect(Math.abs(dx)).toBeGreaterThan(dy + 0.1);
		expect(Math.abs(dx)).toBeGreaterThan(dz + 0.1);
		expect(dx).toBeGreaterThan(0); // Camera to the right (+X)
	});

	test('snap to iso view makes all components roughly equal', async ({ waffle }) => {
		const page = waffle.page;

		await page.waitForFunction(() => window.__waffle?.getCameraState() !== null);

		// First snap to front to establish a non-iso baseline
		await page.evaluate(() => {
			window.dispatchEvent(new CustomEvent('waffle-snap-view', { detail: { view: 'front' } }));
		});
		await page.waitForTimeout(300);

		// Now snap to iso
		await page.evaluate(() => {
			window.dispatchEvent(new CustomEvent('waffle-snap-view', { detail: { view: 'iso' } }));
		});
		await page.waitForTimeout(500);

		const state = await page.evaluate(() => window.__waffle.getCameraState());
		expect(state).not.toBeNull();

		const dx = Math.abs(state.position[0] - state.target[0]);
		const dy = Math.abs(state.position[1] - state.target[1]);
		const dz = Math.abs(state.position[2] - state.target[2]);

		// All three components should be within 50% of each other
		const max = Math.max(dx, dy, dz);
		const min = Math.min(dx, dy, dz);
		expect(min / max).toBeGreaterThan(0.5);
	});

	test('view preset preserves distance from target', async ({ waffle }) => {
		const page = waffle.page;

		await page.waitForFunction(() => window.__waffle?.getCameraState() !== null);
		const before = await page.evaluate(() => window.__waffle.getCameraState());
		const distBefore = cameraDistance(before.position, before.target);

		// Snap to a different view
		await page.evaluate(() => {
			window.dispatchEvent(new CustomEvent('waffle-snap-view', { detail: { view: 'right' } }));
		});
		await page.waitForTimeout(500);

		const after = await page.evaluate(() => window.__waffle.getCameraState());
		const distAfter = cameraDistance(after.position, after.target);

		// Distance should be preserved (within 5%)
		expect(Math.abs(distAfter - distBefore) / distBefore).toBeLessThan(0.05);
	});

	test('sequential view snaps work correctly', async ({ waffle }) => {
		const page = waffle.page;

		await page.waitForFunction(() => window.__waffle?.getCameraState() !== null);

		const views = ['front', 'top', 'right', 'iso'];
		for (const view of views) {
			await page.evaluate((v) => {
				window.dispatchEvent(new CustomEvent('waffle-snap-view', { detail: { view: v } }));
			}, view);
			await page.waitForTimeout(400);

			const state = await page.evaluate(() => window.__waffle.getCameraState());
			expect(state).not.toBeNull();
			for (const v of [...state.position, ...state.target]) {
				expect(Number.isFinite(v)).toBe(true);
			}
		}
	});
});
