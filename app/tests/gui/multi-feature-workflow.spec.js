/**
 * Multi-feature part-building workflow tests — end-to-end GUI tests.
 * Tests complex workflows: boss-on-boss, cut pockets, multi-plane sketches,
 * and feature tree verification across multi-feature parts.
 */
import { test, expect } from './helpers/waffle-test.js';
import {
	clickSketch,
	clickRectangle,
	clickFinishSketch,
	clickExtrude,
} from './helpers/toolbar.js';
import { drawRectangle } from './helpers/canvas.js';
import {
	waitForEntityCount,
	waitForFeatureCount,
	getFeatureCount,
	hasFeatureOfType,
	hasMeshWithGeometry,
	getMeshes,
	getFeatureTree,
	waitForMeshWithGeometry,
} from './helpers/state.js';
import { getFirstFaceRef } from './helpers/geometry.js';

/**
 * Helper: select a face ref programmatically.
 */
async function selectFaceRef(page, ref) {
	await page.evaluate((r) => window.__waffle.selectRef(r), ref);
	await page.waitForTimeout(200);
}

/**
 * Helper: create a base sketch + extrude via real GUI events.
 */
async function sketchAndExtrude(waffle, { plane = 'front', depth = '10', cut = false } = {}) {
	await clickSketch(waffle.page, plane);
	await clickRectangle(waffle.page);
	await drawRectangle(waffle.page, -80, -60, 80, 60);
	try { await waitForEntityCount(waffle.page, 8, 3000); } catch {
		await waffle.dumpState('mfw-sketch-draw-failed');
	}
	await clickFinishSketch(waffle.page);
	try { await waitForFeatureCount(waffle.page, 1, 10000); } catch {
		await waffle.dumpState('mfw-sketch-finish-failed');
	}
	await clickExtrude(waffle.page);
	await waffle.page.locator('[data-testid="extrude-depth"]').fill(depth);
	if (cut) {
		await waffle.page.locator('[data-testid="extrude-cut"]').check();
	}
	await waffle.page.locator('[data-testid="extrude-apply"]').click();
	try { await waitForFeatureCount(waffle.page, 2, 10000); } catch {
		await waffle.dumpState('mfw-extrude-failed');
	}
}

/**
 * Helper: sketch on an existing face and extrude.
 * Assumes a base body already exists. Returns after extrude is verified.
 * @param {object} waffle - WafflePage fixture
 * @param {number} expectedFeaturesBefore - features before this operation
 * @param {object} opts - drawing and extrude options
 */
async function sketchOnFaceAndExtrude(waffle, expectedFeaturesBefore, {
	rectCoords = [-40, -30, 40, 30],
	depth = '5',
	cut = false,
	label = 'sof',
} = {}) {
	// Wait for mesh to have geometry before getting face ref
	await waitForMeshWithGeometry(waffle.page);

	const faceRef = await getFirstFaceRef(waffle.page);
	expect(faceRef).toBeTruthy();

	await selectFaceRef(waffle.page, faceRef);
	await clickSketch(waffle.page);

	await clickRectangle(waffle.page);
	await drawRectangle(waffle.page, ...rectCoords);
	try { await waitForEntityCount(waffle.page, 8, 3000); } catch {
		await waffle.dumpState(`mfw-${label}-draw-failed`);
	}

	await clickFinishSketch(waffle.page);
	try { await waitForFeatureCount(waffle.page, expectedFeaturesBefore + 1, 10000); } catch {
		await waffle.dumpState(`mfw-${label}-finish-failed`);
	}

	await clickExtrude(waffle.page);
	await waffle.page.locator('[data-testid="extrude-depth"]').fill(depth);
	if (cut) {
		await waffle.page.locator('[data-testid="extrude-cut"]').check();
	}
	await waffle.page.locator('[data-testid="extrude-apply"]').click();
	try { await waitForFeatureCount(waffle.page, expectedFeaturesBefore + 2, 10000); } catch {
		await waffle.dumpState(`mfw-${label}-extrude-failed`);
	}
}

test.describe('multi-feature workflows', () => {
	test('boss on boss: extrude -> sketch on face -> extrude again', async ({ waffle }) => {
		// Step 1: Create base box
		await sketchAndExtrude(waffle);

		// Step 2: Sketch on a face and extrude a boss
		await sketchOnFaceAndExtrude(waffle, 2, {
			rectCoords: [-40, -30, 40, 30],
			depth: '5',
			label: 'boss',
		});

		// Verify: 4 features (Sketch, Extrude, Sketch, Extrude)
		const count = await getFeatureCount(waffle.page);
		expect(count).toBe(4);

		const hasMesh = await hasMeshWithGeometry(waffle.page);
		expect(hasMesh).toBe(true);
	});

	test('base extrude plus cut: feature count increases', async ({ waffle }) => {
		// Step 1: Create base box
		await sketchAndExtrude(waffle);

		// Step 2: Sketch on face and extrude as cut
		await sketchOnFaceAndExtrude(waffle, 2, {
			rectCoords: [-40, -30, 40, 30],
			depth: '3',
			cut: true,
			label: 'cut',
		});

		// Verify: 4 features
		const count = await getFeatureCount(waffle.page);
		expect(count).toBe(4);
	});

	test('three features: box + boss + cut pocket', async ({ waffle }) => {
		// Step 1: Base box
		await sketchAndExtrude(waffle, { depth: '15' });

		// Step 2: Boss on face
		await sketchOnFaceAndExtrude(waffle, 2, {
			rectCoords: [-40, -30, 40, 30],
			depth: '5',
			label: 'boss',
		});

		// Step 3: Cut pocket on boss face
		await sketchOnFaceAndExtrude(waffle, 4, {
			rectCoords: [-20, -15, 20, 15],
			depth: '3',
			cut: true,
			label: 'pocket',
		});

		// Verify: 6 features (3 Sketch + 3 Extrude)
		const count = await getFeatureCount(waffle.page);
		expect(count).toBe(6);
	});

	test('sketch on XZ plane -> extrude', async ({ waffle }) => {
		await clickSketch(waffle.page, 'top');
		await clickRectangle(waffle.page);
		await drawRectangle(waffle.page, -80, -60, 80, 60);
		try { await waitForEntityCount(waffle.page, 8, 3000); } catch {
			await waffle.dumpState('mfw-xz-draw-failed');
		}

		await clickFinishSketch(waffle.page);
		try { await waitForFeatureCount(waffle.page, 1, 10000); } catch {
			await waffle.dumpState('mfw-xz-finish-failed');
		}

		await clickExtrude(waffle.page);
		await waffle.page.locator('[data-testid="extrude-depth"]').fill('10');
		await waffle.page.locator('[data-testid="extrude-apply"]').click();
		try { await waitForFeatureCount(waffle.page, 2, 10000); } catch {
			await waffle.dumpState('mfw-xz-extrude-failed');
		}

		const count = await getFeatureCount(waffle.page);
		expect(count).toBe(2);

		const hasMesh = await hasMeshWithGeometry(waffle.page);
		expect(hasMesh).toBe(true);
	});

	test('sketch on YZ plane -> extrude', async ({ waffle }) => {
		await clickSketch(waffle.page, 'right');
		await clickRectangle(waffle.page);
		await drawRectangle(waffle.page, -80, -60, 80, 60);
		try { await waitForEntityCount(waffle.page, 8, 3000); } catch {
			await waffle.dumpState('mfw-yz-draw-failed');
		}

		await clickFinishSketch(waffle.page);
		try { await waitForFeatureCount(waffle.page, 1, 10000); } catch {
			await waffle.dumpState('mfw-yz-finish-failed');
		}

		await clickExtrude(waffle.page);
		await waffle.page.locator('[data-testid="extrude-depth"]').fill('10');
		await waffle.page.locator('[data-testid="extrude-apply"]').click();
		try { await waitForFeatureCount(waffle.page, 2, 10000); } catch {
			await waffle.dumpState('mfw-yz-extrude-failed');
		}

		const count = await getFeatureCount(waffle.page);
		expect(count).toBe(2);

		const hasMesh = await hasMeshWithGeometry(waffle.page);
		expect(hasMesh).toBe(true);
	});

	test('two separate rects in one sketch', async ({ waffle }) => {
		await clickSketch(waffle.page);
		await clickRectangle(waffle.page);

		// First rectangle (left side)
		await drawRectangle(waffle.page, -120, -60, -40, 60);
		try { await waitForEntityCount(waffle.page, 8, 3000); } catch {
			await waffle.dumpState('mfw-2rect-first-failed');
		}

		// Second rectangle (right side)
		await drawRectangle(waffle.page, 40, -60, 120, 60);
		try { await waitForEntityCount(waffle.page, 16, 3000); } catch {
			await waffle.dumpState('mfw-2rect-second-failed');
		}

		await clickFinishSketch(waffle.page);
		try { await waitForFeatureCount(waffle.page, 1, 10000); } catch {
			await waffle.dumpState('mfw-2rect-finish-failed');
		}

		await clickExtrude(waffle.page);
		await waffle.page.locator('[data-testid="extrude-depth"]').fill('10');
		await waffle.page.locator('[data-testid="extrude-apply"]').click();
		try { await waitForFeatureCount(waffle.page, 2, 10000); } catch {
			await waffle.dumpState('mfw-2rect-extrude-failed');
		}

		const count = await getFeatureCount(waffle.page);
		expect(count).toBe(2);
	});

	test('five feature part end-to-end', async ({ waffle }) => {
		// Step 1: Base box
		await sketchAndExtrude(waffle, { depth: '20' });

		// Step 2: Boss on face
		await sketchOnFaceAndExtrude(waffle, 2, {
			rectCoords: [-40, -30, 40, 30],
			depth: '8',
			label: 'boss',
		});

		// Step 3: Cut on boss face
		await sketchOnFaceAndExtrude(waffle, 4, {
			rectCoords: [-20, -15, 20, 15],
			depth: '4',
			cut: true,
			label: 'cut',
		});

		// Verify: 6 features in tree (3 sketches + 3 extrude ops)
		const count = await getFeatureCount(waffle.page);
		expect(count).toBe(6);

		const hasMesh = await hasMeshWithGeometry(waffle.page);
		expect(hasMesh).toBe(true);
	});

	test('feature tree shows correct types in multi-feature part', async ({ waffle }) => {
		// Build base + boss
		await sketchAndExtrude(waffle, { depth: '10' });
		await sketchOnFaceAndExtrude(waffle, 2, {
			rectCoords: [-40, -30, 40, 30],
			depth: '5',
			label: 'tree-check',
		});

		// Verify feature tree structure
		const tree = await getFeatureTree(waffle.page);
		expect(tree.features.length).toBe(4);

		// Should alternate: Sketch, Extrude, Sketch, Extrude
		const types = tree.features.map(f => f.operation?.type);
		expect(types[0]).toBe('Sketch');
		expect(types[1]).toBe('Extrude');
		expect(types[2]).toBe('Sketch');
		expect(types[3]).toBe('Extrude');

		// Feature tree items should be visible in the DOM
		const treeItems = waffle.page.locator('.tree-item:not(.origin-item)');
		const itemCount = await treeItems.count();
		expect(itemCount).toBeGreaterThanOrEqual(4);
	});
});
