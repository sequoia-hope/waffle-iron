/**
 * Extrude workflow — dialog, apply, cancel, feature creation.
 * Covers: basic extrude, flip direction, cut mode, depth modes,
 * second direction options, keyboard shortcuts, and dialog state.
 */
import { test, expect } from './helpers/waffle-test.js';
import { clickSketch, clickRectangle, clickFinishSketch, clickExtrude } from './helpers/toolbar.js';
import { drawRectangle } from './helpers/canvas.js';
import {
	waitForEntityCount,
	waitForFeatureCount,
	getFeatureCount,
	hasFeatureOfType,
	hasMeshWithGeometry,
	getMeshes,
	getFeatureTree,
} from './helpers/state.js';

/**
 * Helper: complete a sketch with a rectangle.
 */
async function sketchRectangle(waffle) {
	await clickSketch(waffle.page);
	await clickRectangle(waffle.page);
	await drawRectangle(waffle.page, -80, -60, 80, 60);

	try {
		await waitForEntityCount(waffle.page, 8, 3000);
	} catch {
		await waffle.dumpState('extrude-sketch-draw-failed');
	}

	await clickFinishSketch(waffle.page);

	try {
		await waitForFeatureCount(waffle.page, 1, 10000);
	} catch {
		await waffle.dumpState('extrude-sketch-finish-failed');
	}
}

/**
 * Helper: open extrude dialog and apply with given options.
 * Returns after the extrude feature is created.
 */
async function applyExtrudeViaDialog(waffle, { depth = '10', cut = false, flipDirection = false, depthMode = null, secondDir = null, secondDepth = null } = {}) {
	await clickExtrude(waffle.page);

	// Set depth mode if specified
	if (depthMode) {
		await waffle.page.locator('[data-testid="extrude-depth-mode"]').selectOption(depthMode);
	}

	// Set depth if in Blind mode and depth input is visible
	const depthInput = waffle.page.locator('[data-testid="extrude-depth"]');
	if (await depthInput.isVisible()) {
		await depthInput.fill(depth);
	}

	// Cut: the legacy checkbox became the combine SELECT in the
	// optional-booleans overhaul.
	if (cut) {
		await waffle.page.locator('[data-testid="extrude-combine"]').selectOption('Cut');
	}

	// Set second direction if specified
	if (secondDir) {
		await waffle.page.locator('[data-testid="extrude-second-dir"]').selectOption(secondDir);
	}

	// Set second depth if specified and visible
	if (secondDepth) {
		const secondDepthInput = waffle.page.locator('[data-testid="extrude-second-depth"]');
		if (await secondDepthInput.isVisible()) {
			await secondDepthInput.fill(secondDepth);
		}
	}

	// Click flip direction button if needed
	if (flipDirection) {
		await waffle.page.locator('[data-testid="extrude-flip-direction"]').click();
	}

	// Click Apply
	await waffle.page.locator('[data-testid="extrude-apply"]').click();

	// Wait for dialog to close
	await expect(waffle.page.locator('[data-testid="extrude-dialog"]')).not.toBeVisible();

	// Wait for extrude feature
	try {
		await waitForFeatureCount(waffle.page, 2, 10000);
	} catch {
		await waffle.dumpState('extrude-apply-failed');
	}
}

test.describe('extrude dialog basics', () => {
	test('after finishing sketch, clicking Extrude shows dialog', async ({ waffle }) => {
		await sketchRectangle(waffle);

		await clickExtrude(waffle.page);

		const dialog = waffle.page.locator('[data-testid="extrude-dialog"]');
		await expect(dialog).toBeVisible();

		// Depth input should be visible with default value
		const depthInput = waffle.page.locator('[data-testid="extrude-depth"]');
		await expect(depthInput).toBeVisible();

		// Apply and Cancel buttons should be visible
		await expect(waffle.page.locator('[data-testid="extrude-apply"]')).toBeVisible();
		await expect(waffle.page.locator('[data-testid="extrude-cancel"]')).toBeVisible();
	});

	test('extrude dialog Cancel closes without creating feature', async ({ waffle }) => {
		await sketchRectangle(waffle);

		const featuresBefore = await getFeatureCount(waffle.page);

		await clickExtrude(waffle.page);
		await waffle.page.locator('[data-testid="extrude-cancel"]').click();

		// Dialog should be gone
		await expect(waffle.page.locator('[data-testid="extrude-dialog"]')).not.toBeVisible();

		// Feature count should not have changed
		const featuresAfter = await getFeatureCount(waffle.page);
		expect(featuresAfter).toBe(featuresBefore);
	});

	test('extrude dialog Apply creates Extrude feature', async ({ waffle }) => {
		await sketchRectangle(waffle);

		await clickExtrude(waffle.page);

		// Set depth value
		const depthInput = waffle.page.locator('[data-testid="extrude-depth"]');
		await depthInput.fill('10');

		// Click Apply
		await waffle.page.locator('[data-testid="extrude-apply"]').click();

		// Dialog should close
		await expect(waffle.page.locator('[data-testid="extrude-dialog"]')).not.toBeVisible();

		// Wait for feature to be added
		try {
			await waitForFeatureCount(waffle.page, 2, 10000);
		} catch {
			await waffle.dumpState('extrude-apply-failed');
		}

		// Feature tree should have Sketch + Extrude
		const hasSketch = await hasFeatureOfType(waffle.page, 'Sketch');
		const hasExtrude = await hasFeatureOfType(waffle.page, 'Extrude');
		expect(hasSketch).toBe(true);
		expect(hasExtrude).toBe(true);
	});

	test('extrude creates 3D mesh with triangles', async ({ waffle }) => {
		await sketchRectangle(waffle);
		await clickExtrude(waffle.page);

		const depthInput = waffle.page.locator('[data-testid="extrude-depth"]');
		await depthInput.fill('10');
		await waffle.page.locator('[data-testid="extrude-apply"]').click();

		// Wait for mesh to appear
		try {
			await waffle.page.waitForFunction(
				() => {
					const meshes = window.__waffle?.getMeshes() ?? [];
					return meshes.some(m => m.triangleCount > 0);
				},
				{ timeout: 10000 }
			);
		} catch {
			await waffle.dumpState('extrude-mesh-failed');
		}

		const hasMesh = await hasMeshWithGeometry(waffle.page);
		expect(hasMesh).toBe(true);

		const meshes = await getMeshes(waffle.page);
		const extrudeMesh = meshes.find(m => m.triangleCount > 0);
		expect(extrudeMesh).toBeDefined();
		expect(extrudeMesh.triangleCount).toBeGreaterThan(0);
	});

	test('close button (X) cancels dialog', async ({ waffle }) => {
		await sketchRectangle(waffle);
		await clickExtrude(waffle.page);

		// Click the X close button
		await waffle.page.locator('.close-btn').click();

		await expect(waffle.page.locator('[data-testid="extrude-dialog"]')).not.toBeVisible();

		// Feature count unchanged (still just the sketch)
		const count = await getFeatureCount(waffle.page);
		expect(count).toBe(1);
	});
});

test.describe('extrude keyboard shortcuts', () => {
	test('Enter key in extrude dialog applies', async ({ waffle }) => {
		await sketchRectangle(waffle);
		await clickExtrude(waffle.page);

		const depthInput = waffle.page.locator('[data-testid="extrude-depth"]');
		await depthInput.fill('5');

		// Press Enter to apply
		await waffle.page.keyboard.press('Enter');

		// Dialog should close
		await expect(waffle.page.locator('[data-testid="extrude-dialog"]')).not.toBeVisible();

		// Feature should be created
		try {
			await waitForFeatureCount(waffle.page, 2, 10000);
		} catch {
			await waffle.dumpState('extrude-enter-apply-failed');
		}
		const hasExtrude = await hasFeatureOfType(waffle.page, 'Extrude');
		expect(hasExtrude).toBe(true);
	});

	test('Escape key in extrude dialog cancels', async ({ waffle }) => {
		await sketchRectangle(waffle);
		await clickExtrude(waffle.page);

		// Press Escape to cancel
		await waffle.page.keyboard.press('Escape');

		// Dialog should close
		await expect(waffle.page.locator('[data-testid="extrude-dialog"]')).not.toBeVisible();

		// No new feature
		const count = await getFeatureCount(waffle.page);
		expect(count).toBe(1);
	});
});

test.describe('extrude flip direction', () => {
	test('flip direction button is visible and shows Normal by default', async ({ waffle }) => {
		await sketchRectangle(waffle);
		await clickExtrude(waffle.page);

		const flipBtn = waffle.page.locator('[data-testid="extrude-flip-direction"]');
		await expect(flipBtn).toBeVisible();
		await expect(flipBtn).toHaveText('Normal');
	});

	test('clicking flip direction toggles button text to Flipped', async ({ waffle }) => {
		await sketchRectangle(waffle);
		await clickExtrude(waffle.page);

		const flipBtn = waffle.page.locator('[data-testid="extrude-flip-direction"]');
		await expect(flipBtn).toHaveText('Normal');

		await flipBtn.click();
		await expect(flipBtn).toHaveText('Flipped');

		// Click again to toggle back
		await flipBtn.click();
		await expect(flipBtn).toHaveText('Normal');
	});

	test('flip direction button has flipped class when active', async ({ waffle }) => {
		await sketchRectangle(waffle);
		await clickExtrude(waffle.page);

		const flipBtn = waffle.page.locator('[data-testid="extrude-flip-direction"]');

		// Initially no flipped class
		await expect(flipBtn).not.toHaveClass(/flipped/);

		await flipBtn.click();
		await expect(flipBtn).toHaveClass(/flipped/);

		await flipBtn.click();
		await expect(flipBtn).not.toHaveClass(/flipped/);
	});

	test('extrude with flipped direction creates feature successfully', async ({ waffle }) => {
		await sketchRectangle(waffle);
		await applyExtrudeViaDialog(waffle, { depth: '15', flipDirection: true });

		const hasExtrude = await hasFeatureOfType(waffle.page, 'Extrude');
		expect(hasExtrude).toBe(true);

		const hasMesh = await hasMeshWithGeometry(waffle.page);
		expect(hasMesh).toBe(true);
	});

	test('extrude with normal direction creates feature successfully', async ({ waffle }) => {
		await sketchRectangle(waffle);
		await applyExtrudeViaDialog(waffle, { depth: '20', flipDirection: false });

		const hasExtrude = await hasFeatureOfType(waffle.page, 'Extrude');
		expect(hasExtrude).toBe(true);

		const hasMesh = await hasMeshWithGeometry(waffle.page);
		expect(hasMesh).toBe(true);
	});

	test('flip direction passes negated normal to engine', async ({ waffle }) => {
		await sketchRectangle(waffle);
		await clickExtrude(waffle.page);

		// Flip the direction
		await waffle.page.locator('[data-testid="extrude-flip-direction"]').click();

		// Apply via __waffle API to inspect the params sent
		// The extrude feature's params.direction should be negated normal
		await waffle.page.locator('[data-testid="extrude-apply"]').click();
		await expect(waffle.page.locator('[data-testid="extrude-dialog"]')).not.toBeVisible();

		try {
			await waitForFeatureCount(waffle.page, 2, 10000);
		} catch {
			await waffle.dumpState('extrude-flip-direction-failed');
		}

		// Verify the extrude feature was created
		const tree = await getFeatureTree(waffle.page);
		const extrudeFeature = tree.features.find(f => f.operation?.type === 'Extrude');
		expect(extrudeFeature).toBeDefined();

		// The direction should be negated (original sketch on front/XY plane: normal [0,0,1])
		// So flipped direction should be [0,0,-1]
		const dir = extrudeFeature.operation?.params?.direction;
		expect(dir).toBeDefined();
		expect(dir[2]).toBeLessThan(0);
	});

	test('normal direction passes null direction to engine (uses sketch normal)', async ({ waffle }) => {
		await sketchRectangle(waffle);
		await clickExtrude(waffle.page);

		// Don't flip — leave as Normal
		await waffle.page.locator('[data-testid="extrude-apply"]').click();
		await expect(waffle.page.locator('[data-testid="extrude-dialog"]')).not.toBeVisible();

		try {
			await waitForFeatureCount(waffle.page, 2, 10000);
		} catch {
			await waffle.dumpState('extrude-normal-dir-failed');
		}

		const tree = await getFeatureTree(waffle.page);
		const extrudeFeature = tree.features.find(f => f.operation?.type === 'Extrude');
		expect(extrudeFeature).toBeDefined();

		// Direction should be null (engine defaults to sketch plane normal)
		const dir = extrudeFeature.operation?.params?.direction;
		expect(dir).toBeNull();
	});

	test('flip direction resets when dialog reopens', async ({ waffle }) => {
		await sketchRectangle(waffle);

		// Open dialog, flip, cancel
		await clickExtrude(waffle.page);
		await waffle.page.locator('[data-testid="extrude-flip-direction"]').click();
		await expect(waffle.page.locator('[data-testid="extrude-flip-direction"]')).toHaveText('Flipped');
		await waffle.page.locator('[data-testid="extrude-cancel"]').click();

		// Reopen — should be reset to Normal
		await clickExtrude(waffle.page);
		await expect(waffle.page.locator('[data-testid="extrude-flip-direction"]')).toHaveText('Normal');
		await expect(waffle.page.locator('[data-testid="extrude-flip-direction"]')).not.toHaveClass(/flipped/);
	});

	test('direction vector input is NOT shown (old UI removed)', async ({ waffle }) => {
		await sketchRectangle(waffle);
		await clickExtrude(waffle.page);

		// Old direction override checkbox should not exist
		await expect(waffle.page.locator('[data-testid="extrude-dir-override"]')).not.toBeVisible();

		// Old X/Y/Z vector inputs should not exist
		await expect(waffle.page.locator('[data-testid="extrude-dir-x"]')).not.toBeVisible();
		await expect(waffle.page.locator('[data-testid="extrude-dir-y"]')).not.toBeVisible();
		await expect(waffle.page.locator('[data-testid="extrude-dir-z"]')).not.toBeVisible();
	});
});

test.describe('extrude cut mode', () => {
	test('combine select is visible and defaults to Add (not Cut)', async ({ waffle }) => {
		// The legacy cut checkbox became the combine SELECT in the
		// optional-booleans overhaul; "not cut by default" now means the
		// select defaults to Add.
		await sketchRectangle(waffle);
		await clickExtrude(waffle.page);

		const combineSelect = waffle.page.locator('[data-testid="extrude-combine"]');
		await expect(combineSelect).toBeVisible();
		await expect(combineSelect).toHaveValue('Add');
	});

	test('extrude with cut creates feature', async ({ waffle }) => {
		await sketchRectangle(waffle);
		await applyExtrudeViaDialog(waffle, { depth: '10', cut: true });

		const hasExtrude = await hasFeatureOfType(waffle.page, 'Extrude');
		expect(hasExtrude).toBe(true);
	});

	test('extrude cut with flipped direction creates feature', async ({ waffle }) => {
		await sketchRectangle(waffle);
		await applyExtrudeViaDialog(waffle, { depth: '10', cut: true, flipDirection: true });

		const hasExtrude = await hasFeatureOfType(waffle.page, 'Extrude');
		expect(hasExtrude).toBe(true);
	});
});

test.describe('extrude depth modes', () => {
	test('Blind mode shows depth input', async ({ waffle }) => {
		await sketchRectangle(waffle);
		await clickExtrude(waffle.page);

		// Mode should be Blind by default
		const modeSelect = waffle.page.locator('[data-testid="extrude-depth-mode"]');
		await expect(modeSelect).toHaveValue('Blind');

		// Depth input should be visible
		await expect(waffle.page.locator('[data-testid="extrude-depth"]')).toBeVisible();
	});

	test('Through All mode hides depth input', async ({ waffle }) => {
		await sketchRectangle(waffle);
		await clickExtrude(waffle.page);

		// Switch to Through All
		await waffle.page.locator('[data-testid="extrude-depth-mode"]').selectOption('ThroughAll');

		// Depth input should be hidden
		await expect(waffle.page.locator('[data-testid="extrude-depth"]')).not.toBeVisible();
	});

	test('extrude with Through All mode creates feature', async ({ waffle }) => {
		await sketchRectangle(waffle);
		await applyExtrudeViaDialog(waffle, { depthMode: 'ThroughAll' });

		const hasExtrude = await hasFeatureOfType(waffle.page, 'Extrude');
		expect(hasExtrude).toBe(true);
	});

	test('Through All with flipped direction creates feature', async ({ waffle }) => {
		await sketchRectangle(waffle);
		await applyExtrudeViaDialog(waffle, { depthMode: 'ThroughAll', flipDirection: true });

		const hasExtrude = await hasFeatureOfType(waffle.page, 'Extrude');
		expect(hasExtrude).toBe(true);
	});

	test('switching from Through All back to Blind restores depth input', async ({ waffle }) => {
		await sketchRectangle(waffle);
		await clickExtrude(waffle.page);

		// Switch to Through All
		await waffle.page.locator('[data-testid="extrude-depth-mode"]').selectOption('ThroughAll');
		await expect(waffle.page.locator('[data-testid="extrude-depth"]')).not.toBeVisible();

		// Switch back to Blind
		await waffle.page.locator('[data-testid="extrude-depth-mode"]').selectOption('Blind');
		await expect(waffle.page.locator('[data-testid="extrude-depth"]')).toBeVisible();
	});
});

test.describe('extrude second direction', () => {
	test('second direction defaults to None', async ({ waffle }) => {
		await sketchRectangle(waffle);
		await clickExtrude(waffle.page);

		const secondDirSelect = waffle.page.locator('[data-testid="extrude-second-dir"]');
		await expect(secondDirSelect).toHaveValue('None');
	});

	test('Symmetric second direction changes depth label', async ({ waffle }) => {
		await sketchRectangle(waffle);
		await clickExtrude(waffle.page);

		// Select Symmetric
		await waffle.page.locator('[data-testid="extrude-second-dir"]').selectOption('Symmetric');

		// Depth label should say "Depth (each side) (<unit>)"
		const depthLabel = waffle.page.locator('label[for="extrude-depth"]');
		await expect(depthLabel).toContainText('Depth (each side)');
	});

	test('extrude with Symmetric creates feature', async ({ waffle }) => {
		await sketchRectangle(waffle);
		await applyExtrudeViaDialog(waffle, { depth: '10', secondDir: 'Symmetric' });

		const hasExtrude = await hasFeatureOfType(waffle.page, 'Extrude');
		expect(hasExtrude).toBe(true);
	});

	test('Two Depths second direction shows second depth input', async ({ waffle }) => {
		await sketchRectangle(waffle);
		await clickExtrude(waffle.page);

		// Select Two Depths (value="Blind")
		await waffle.page.locator('[data-testid="extrude-second-dir"]').selectOption('Blind');

		// Second depth input should be visible
		await expect(waffle.page.locator('[data-testid="extrude-second-depth"]')).toBeVisible();
	});

	test('extrude with Two Depths creates feature', async ({ waffle }) => {
		await sketchRectangle(waffle);
		await applyExtrudeViaDialog(waffle, { depth: '10', secondDir: 'Blind', secondDepth: '5' });

		const hasExtrude = await hasFeatureOfType(waffle.page, 'Extrude');
		expect(hasExtrude).toBe(true);
	});

	test('Through All second direction creates feature', async ({ waffle }) => {
		await sketchRectangle(waffle);
		await applyExtrudeViaDialog(waffle, { depth: '10', secondDir: 'ThroughAll' });

		const hasExtrude = await hasFeatureOfType(waffle.page, 'Extrude');
		expect(hasExtrude).toBe(true);
	});

	test('Symmetric with flipped direction creates feature', async ({ waffle }) => {
		await sketchRectangle(waffle);
		await applyExtrudeViaDialog(waffle, { depth: '10', secondDir: 'Symmetric', flipDirection: true });

		const hasExtrude = await hasFeatureOfType(waffle.page, 'Extrude');
		expect(hasExtrude).toBe(true);
	});

	test('second depth input hidden when switching back to None', async ({ waffle }) => {
		await sketchRectangle(waffle);
		await clickExtrude(waffle.page);

		// Show second depth
		await waffle.page.locator('[data-testid="extrude-second-dir"]').selectOption('Blind');
		await expect(waffle.page.locator('[data-testid="extrude-second-depth"]')).toBeVisible();

		// Switch back to None
		await waffle.page.locator('[data-testid="extrude-second-dir"]').selectOption('None');
		await expect(waffle.page.locator('[data-testid="extrude-second-depth"]')).not.toBeVisible();
	});
});

test.describe('extrude combined options', () => {
	test('cut + Through All creates feature', async ({ waffle }) => {
		await sketchRectangle(waffle);
		await applyExtrudeViaDialog(waffle, { cut: true, depthMode: 'ThroughAll' });

		const hasExtrude = await hasFeatureOfType(waffle.page, 'Extrude');
		expect(hasExtrude).toBe(true);
	});

	test('cut + flipped + Symmetric creates feature', async ({ waffle }) => {
		await sketchRectangle(waffle);
		await applyExtrudeViaDialog(waffle, { depth: '10', cut: true, flipDirection: true, secondDir: 'Symmetric' });

		const hasExtrude = await hasFeatureOfType(waffle.page, 'Extrude');
		expect(hasExtrude).toBe(true);
	});

	test('Through All + flipped + cut creates feature', async ({ waffle }) => {
		await sketchRectangle(waffle);
		await applyExtrudeViaDialog(waffle, { depthMode: 'ThroughAll', flipDirection: true, cut: true });

		const hasExtrude = await hasFeatureOfType(waffle.page, 'Extrude');
		expect(hasExtrude).toBe(true);
	});

	test('Two Depths + flipped creates feature', async ({ waffle }) => {
		await sketchRectangle(waffle);
		await applyExtrudeViaDialog(waffle, { depth: '15', secondDir: 'Blind', secondDepth: '8', flipDirection: true });

		const hasExtrude = await hasFeatureOfType(waffle.page, 'Extrude');
		expect(hasExtrude).toBe(true);
	});
});

test.describe('extrude dialog state management', () => {
	test('dialog resets all fields when reopened', async ({ waffle }) => {
		// The legacy cut CHECKBOX became the combine SELECT (NewBody/Add/Cut/
		// Intersect) in the optional-booleans overhaul; assert on that control.
		await sketchRectangle(waffle);

		// Open, modify fields, cancel
		await clickExtrude(waffle.page);
		await waffle.page.locator('[data-testid="extrude-depth"]').fill('99');
		await waffle.page.locator('[data-testid="extrude-combine"]').selectOption('Cut');
		await waffle.page.locator('[data-testid="extrude-depth-mode"]').selectOption('ThroughAll');
		await waffle.page.locator('[data-testid="extrude-flip-direction"]').click();
		await waffle.page.locator('[data-testid="extrude-cancel"]').click();

		// Reopen — all fields should be at defaults
		await clickExtrude(waffle.page);
		await expect(waffle.page.locator('[data-testid="extrude-depth-mode"]')).toHaveValue('Blind');
		await expect(waffle.page.locator('[data-testid="extrude-depth"]')).toBeVisible();
		await expect(waffle.page.locator('[data-testid="extrude-combine"]')).toHaveValue('Add');
		await expect(waffle.page.locator('[data-testid="extrude-second-dir"]')).toHaveValue('None');
		await expect(waffle.page.locator('[data-testid="extrude-flip-direction"]')).toHaveText('Normal');
		await expect(waffle.page.locator('[data-testid="extrude-flip-direction"]')).not.toHaveClass(/flipped/);
	});

	test('extrude via __waffle API (programmatic) works', async ({ waffle }) => {
		await sketchRectangle(waffle);

		// Use the programmatic API to extrude
		await waffle.page.evaluate(() => window.__waffle.showExtrudeDialog());
		await waffle.page.waitForTimeout(100);
		await waffle.page.evaluate(() => window.__waffle.applyExtrude(20, 0, false));

		try {
			await waitForFeatureCount(waffle.page, 2, 10000);
		} catch {
			await waffle.dumpState('extrude-api-failed');
		}

		const hasExtrude = await hasFeatureOfType(waffle.page, 'Extrude');
		expect(hasExtrude).toBe(true);
	});

	test('extrude via __waffle API with flipDirection works', async ({ waffle }) => {
		await sketchRectangle(waffle);

		await waffle.page.evaluate(() => window.__waffle.showExtrudeDialog());
		await waffle.page.waitForTimeout(100);
		await waffle.page.evaluate(() => window.__waffle.applyExtrude(20, 0, false, { flipDirection: true }));

		try {
			await waitForFeatureCount(waffle.page, 2, 10000);
		} catch {
			await waffle.dumpState('extrude-api-flip-failed');
		}

		const tree = await getFeatureTree(waffle.page);
		const extrudeFeature = tree.features.find(f => f.operation?.type === 'Extrude');
		expect(extrudeFeature).toBeDefined();

		// Direction should be negated since we flipped
		const dir = extrudeFeature.operation?.params?.direction;
		expect(dir).toBeDefined();
		expect(dir[2]).toBeLessThan(0);
	});
});
