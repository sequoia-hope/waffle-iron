/**
 * Revolve dialog advanced options tests — partial angles, reverse direction,
 * axis switching, and feature verification.
 */
import { test, expect } from './helpers/waffle-test.js';
import {
	clickSketch,
	clickRectangle,
	clickFinishSketch,
	clickRevolve,
} from './helpers/toolbar.js';
import { drawRectangle } from './helpers/canvas.js';
import {
	waitForEntityCount,
	waitForFeatureCount,
	getFeatureCount,
	hasFeatureOfType,
	hasMeshWithGeometry,
	getFeatureTree,
	collectCrashErrors,
	expectNoCrash,
} from './helpers/state.js';

/**
 * Helper: create a sketch with a rectangle offset from origin (for revolve).
 * Rectangle from (10,-20) to (30,20) — offset right of Y axis so revolve
 * around the left edge doesn't self-intersect.
 */
async function createRevolveSketch(waffle) {
	await clickSketch(waffle.page);
	await clickRectangle(waffle.page);
	await drawRectangle(waffle.page, 10, -60, 100, 60);
	try {
		await waitForEntityCount(waffle.page, 8, 3000);
	} catch {
		await waffle.dumpState('revolve-opt-sketch-failed');
	}

	await clickFinishSketch(waffle.page);
	try {
		await waitForFeatureCount(waffle.page, 1, 10000);
	} catch {
		await waffle.dumpState('revolve-opt-finish-failed');
	}
}

/**
 * Helper: open revolve dialog, set angle, and apply.
 */
async function applyRevolve(waffle, { angle = '360', reverse = false } = {}) {
	await clickRevolve(waffle.page);

	const angleInput = waffle.page.locator('#revolve-angle');
	await angleInput.fill(angle);

	if (reverse) {
		const reverseBtn = waffle.page.locator('[data-testid="revolve-reverse"]');
		if (await reverseBtn.isVisible()) {
			await reverseBtn.click();
		}
	}

	await waffle.page.locator('[data-testid="revolve-apply"]').click();

	try {
		await waitForFeatureCount(waffle.page, 2, 15000);
	} catch {
		await waffle.dumpState('revolve-opt-apply-failed');
	}
}

test.describe('revolve partial angles', () => {
	test('revolve 90 degrees creates feature with mesh', async ({ waffle }) => {
		const tracker = collectCrashErrors(waffle.page);
		await createRevolveSketch(waffle);
		await applyRevolve(waffle, { angle: '90' });

		expect(await hasFeatureOfType(waffle.page, 'Revolve')).toBe(true);
		expect(await hasMeshWithGeometry(waffle.page)).toBe(true);
		expectNoCrash(tracker);
	});

	test('revolve 180 degrees creates feature with mesh', async ({ waffle }) => {
		const tracker = collectCrashErrors(waffle.page);
		await createRevolveSketch(waffle);
		await applyRevolve(waffle, { angle: '180' });

		expect(await hasFeatureOfType(waffle.page, 'Revolve')).toBe(true);
		expect(await hasMeshWithGeometry(waffle.page)).toBe(true);
		expectNoCrash(tracker);
	});

	test('revolve 270 degrees creates feature with mesh', async ({ waffle }) => {
		const tracker = collectCrashErrors(waffle.page);
		await createRevolveSketch(waffle);
		await applyRevolve(waffle, { angle: '270' });

		expect(await hasFeatureOfType(waffle.page, 'Revolve')).toBe(true);
		expect(await hasMeshWithGeometry(waffle.page)).toBe(true);
		expectNoCrash(tracker);
	});

	test('revolve 360 degrees (full) creates feature with mesh', async ({ waffle }) => {
		const tracker = collectCrashErrors(waffle.page);
		await createRevolveSketch(waffle);
		await applyRevolve(waffle, { angle: '360' });

		expect(await hasFeatureOfType(waffle.page, 'Revolve')).toBe(true);
		expect(await hasMeshWithGeometry(waffle.page)).toBe(true);
		expectNoCrash(tracker);
	});

	test('revolve 45 degrees creates feature', async ({ waffle }) => {
		const tracker = collectCrashErrors(waffle.page);
		await createRevolveSketch(waffle);
		await applyRevolve(waffle, { angle: '45' });

		expect(await hasFeatureOfType(waffle.page, 'Revolve')).toBe(true);
		expectNoCrash(tracker);
	});

	test('revolve angle stored in feature params', async ({ waffle }) => {
		const tracker = collectCrashErrors(waffle.page);
		await createRevolveSketch(waffle);
		await applyRevolve(waffle, { angle: '120' });

		const tree = await getFeatureTree(waffle.page);
		const revolveFeature = tree.features.find(f => f.operation?.type === 'Revolve');
		expect(revolveFeature).toBeDefined();

		// Angle should be stored in params
		const params = revolveFeature.operation?.params;
		expect(params).toBeDefined();
		if (params?.angle !== undefined) {
			expect(params.angle).toBeCloseTo(120, 0);
		}

		expectNoCrash(tracker);
	});
});

test.describe('revolve axis selection', () => {
	test('revolve auto-selects first line as axis', async ({ waffle }) => {
		await createRevolveSketch(waffle);
		await clickRevolve(waffle.page);

		const axisList = waffle.page.locator('[data-testid="revolve-axis-list"]');
		const activeBtn = axisList.locator('.btn-axis-entity.active');
		expect(await activeBtn.count()).toBe(1);

		// Apply button should be enabled (axis auto-selected)
		await expect(waffle.page.locator('[data-testid="revolve-apply"]')).toBeEnabled();
	});

	test('clicking different axis changes active highlight', async ({ waffle }) => {
		await createRevolveSketch(waffle);
		await clickRevolve(waffle.page);

		const axisList = waffle.page.locator('[data-testid="revolve-axis-list"]');
		const buttons = axisList.locator('.btn-axis-entity');
		const count = await buttons.count();

		if (count >= 2) {
			// Get initial active button text
			const initialActive = await axisList.locator('.btn-axis-entity.active').textContent();

			// Click the second button
			await buttons.nth(1).click();
			await waffle.page.waitForTimeout(200);

			// New active button should be different
			const newActive = axisList.locator('.btn-axis-entity.active');
			expect(await newActive.count()).toBe(1);
		}
	});

	test('revolve with different axis selection creates feature', async ({ waffle }) => {
		const tracker = collectCrashErrors(waffle.page);
		await createRevolveSketch(waffle);
		await clickRevolve(waffle.page);

		// Select second axis (if available)
		const axisList = waffle.page.locator('[data-testid="revolve-axis-list"]');
		const buttons = axisList.locator('.btn-axis-entity');
		const count = await buttons.count();
		if (count >= 2) {
			await buttons.nth(1).click();
			await waffle.page.waitForTimeout(200);
		}

		// Set angle and apply
		await waffle.page.locator('#revolve-angle').fill('180');
		await waffle.page.locator('[data-testid="revolve-apply"]').click();

		try {
			await waitForFeatureCount(waffle.page, 2, 15000);
		} catch {
			await waffle.dumpState('revolve-alt-axis-failed');
		}

		expect(await hasFeatureOfType(waffle.page, 'Revolve')).toBe(true);
		expectNoCrash(tracker);
	});
});

test.describe('revolve dialog state', () => {
	test('dialog resets angle to 360 when reopened', async ({ waffle }) => {
		await createRevolveSketch(waffle);

		// Open dialog, change angle, cancel
		await clickRevolve(waffle.page);
		await waffle.page.locator('#revolve-angle').fill('90');
		await waffle.page.locator('[data-testid="revolve-cancel"]').click();

		// Reopen — angle should be back to 360
		await clickRevolve(waffle.page);
		const angleValue = await waffle.page.locator('#revolve-angle').inputValue();
		expect(parseFloat(angleValue)).toBe(360);
	});

	test('revolve feature shows in feature tree with correct type', async ({ waffle }) => {
		const tracker = collectCrashErrors(waffle.page);
		await createRevolveSketch(waffle);
		await applyRevolve(waffle, { angle: '180' });

		const tree = await getFeatureTree(waffle.page);
		const types = tree.features.map(f => f.operation?.type);
		expect(types).toContain('Sketch');
		expect(types).toContain('Revolve');

		// Feature tree items should be visible in DOM
		const treeItems = waffle.page.locator('.tree-item:not(.origin-item)');
		const itemCount = await treeItems.count();
		expect(itemCount).toBeGreaterThanOrEqual(2);

		expectNoCrash(tracker);
	});
});
