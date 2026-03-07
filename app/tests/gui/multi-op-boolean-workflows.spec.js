/**
 * Multi-operation boolean workflow tests — verify complex multi-step
 * CAD operations (boss-on-box, boss chains, ring+cut, stacked union)
 * produce valid results without WASM crashes.
 *
 * B24: Extends GUI coverage for boolean scenarios that previously had
 * only Rust-level test-harness tests.
 */
import { test, expect } from './helpers/waffle-test.js';
import {
	clickSketch,
	clickRectangle,
	clickCircle,
	clickFinishSketch,
	clickExtrude,
} from './helpers/toolbar.js';
import { drawRectangle, drawCircle } from './helpers/canvas.js';
import {
	waitForEntityCount,
	waitForFeatureCount,
	hasMeshWithGeometry,
	waitForMeshWithGeometry,
	collectCrashErrors,
	expectNoAnyCrash,
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
 * Helper: create a base box via sketch rectangle + extrude.
 */
async function createBaseBox(waffle, { depth = '10' } = {}) {
	await clickSketch(waffle.page, 'front');
	await clickRectangle(waffle.page);
	await drawRectangle(waffle.page, -80, -60, 80, 60);
	try {
		await waitForEntityCount(waffle.page, 8, 3000);
	} catch {
		await waffle.dumpState('mob-base-sketch-failed');
	}
	await clickFinishSketch(waffle.page);
	try {
		await waitForFeatureCount(waffle.page, 1, 10000);
	} catch {
		await waffle.dumpState('mob-base-finish-failed');
	}
	await clickExtrude(waffle.page);
	await waffle.page.locator('[data-testid="extrude-depth"]').fill(depth);
	await waffle.page.locator('[data-testid="extrude-apply"]').click();
	try {
		await waitForFeatureCount(waffle.page, 2, 10000);
	} catch {
		await waffle.dumpState('mob-base-extrude-failed');
	}
}

/**
 * Helper: create a base cylinder via NURBS circle sketch + extrude.
 */
async function createBaseCylinder(waffle, { radius = 60, depth = '10' } = {}) {
	await clickSketch(waffle.page, 'front');
	await clickCircle(waffle.page);
	await drawCircle(waffle.page, 0, 0, radius, 0);
	try {
		await waitForEntityCount(waffle.page, 2, 3000);
	} catch {
		await waffle.dumpState('mob-cyl-sketch-failed');
	}
	await clickFinishSketch(waffle.page);
	try {
		await waitForFeatureCount(waffle.page, 1, 10000);
	} catch {
		await waffle.dumpState('mob-cyl-finish-failed');
	}
	await clickExtrude(waffle.page);
	await waffle.page.locator('[data-testid="extrude-depth"]').fill(depth);
	await waffle.page.locator('[data-testid="extrude-apply"]').click();
	try {
		await waitForFeatureCount(waffle.page, 2, 15000);
	} catch {
		await waffle.dumpState('mob-cyl-extrude-failed');
	}
}

test.describe('multi-op boolean workflows', () => {
	test('circle boss on box: no crash', async ({ waffle }) => {
		const crashTracker = collectCrashErrors(waffle.page);

		// Step 1: Create base box
		await createBaseBox(waffle);
		await waitForMeshWithGeometry(waffle.page);

		// Step 2: Select top face and start sketch
		const faceRef = await getFirstFaceRef(waffle.page);
		expect(faceRef).toBeTruthy();
		await selectFaceRef(waffle.page, faceRef);
		await clickSketch(waffle.page);

		// Step 3: Draw circle on the face
		await clickCircle(waffle.page);
		await drawCircle(waffle.page, 0, 0, 30, 0);
		try {
			await waitForEntityCount(waffle.page, 2, 3000);
		} catch {
			await waffle.dumpState('mob-boss-circle-failed');
		}

		// Step 4: Finish sketch
		await clickFinishSketch(waffle.page);
		try {
			await waitForFeatureCount(waffle.page, 3, 10000);
		} catch {
			await waffle.dumpState('mob-boss-finish-failed');
		}

		// Step 5: Extrude as BOSS (NOT cut) — depth 5
		await clickExtrude(waffle.page);
		await waffle.page.locator('[data-testid="extrude-depth"]').fill('5');
		// Do NOT check the cut checkbox — this is a boss (union)
		await waffle.page.locator('[data-testid="extrude-apply"]').click();
		try {
			await waitForFeatureCount(waffle.page, 4, 15000);
		} catch {
			await waffle.dumpState('mob-boss-extrude-failed');
		}

		// Verify
		const hasMesh = await hasMeshWithGeometry(waffle.page);
		expect(hasMesh).toBe(true);
		expectNoAnyCrash(crashTracker);
	});

	test('box + rect boss + circle cut (3-op chain): no crash', async ({ waffle }) => {
		const crashTracker = collectCrashErrors(waffle.page);

		// Step 1: Create base box
		await createBaseBox(waffle);
		await waitForMeshWithGeometry(waffle.page);

		// Step 2: Select face → sketch smaller rectangle → extrude as boss
		const faceRef1 = await getFirstFaceRef(waffle.page);
		expect(faceRef1).toBeTruthy();
		await selectFaceRef(waffle.page, faceRef1);
		await clickSketch(waffle.page);

		await clickRectangle(waffle.page);
		await drawRectangle(waffle.page, -40, -30, 40, 30);
		try {
			await waitForEntityCount(waffle.page, 8, 3000);
		} catch {
			await waffle.dumpState('mob-chain-rect-failed');
		}

		await clickFinishSketch(waffle.page);
		try {
			await waitForFeatureCount(waffle.page, 3, 10000);
		} catch {
			await waffle.dumpState('mob-chain-rect-finish-failed');
		}

		await clickExtrude(waffle.page);
		await waffle.page.locator('[data-testid="extrude-depth"]').fill('5');
		await waffle.page.locator('[data-testid="extrude-apply"]').click();
		try {
			await waitForFeatureCount(waffle.page, 4, 15000);
		} catch {
			await waffle.dumpState('mob-chain-rect-extrude-failed');
		}

		// Step 3: Select face → sketch circle → extrude as cut
		const faceRef2 = await getFirstFaceRef(waffle.page);
		expect(faceRef2).toBeTruthy();
		await selectFaceRef(waffle.page, faceRef2);
		await clickSketch(waffle.page);

		await clickCircle(waffle.page);
		await drawCircle(waffle.page, 0, 0, 20, 0);
		try {
			await waitForEntityCount(waffle.page, 2, 3000);
		} catch {
			await waffle.dumpState('mob-chain-circle-failed');
		}

		await clickFinishSketch(waffle.page);
		try {
			await waitForFeatureCount(waffle.page, 5, 10000);
		} catch {
			await waffle.dumpState('mob-chain-circle-finish-failed');
		}

		await clickExtrude(waffle.page);
		await waffle.page.locator('[data-testid="extrude-depth"]').fill('3');
		await waffle.page.locator('[data-testid="extrude-cut"]').check();
		await waffle.page.locator('[data-testid="extrude-apply"]').click();
		try {
			await waitForFeatureCount(waffle.page, 6, 30000);
		} catch {
			await waffle.dumpState('mob-chain-circle-cut-failed');
		}

		// Verify
		const hasMesh = await hasMeshWithGeometry(waffle.page);
		expect(hasMesh).toBe(true);
		expectNoAnyCrash(crashTracker);
	});

	test('ring + quadrant cut → C-shape: no crash', async ({ waffle }) => {
		const crashTracker = collectCrashErrors(waffle.page);

		// Step 1: Create cylinder via NURBS circle
		await createBaseCylinder(waffle, { radius: 60, depth: '10' });
		await waitForMeshWithGeometry(waffle.page);

		// Step 2: Select top face → draw smaller circle → extrude cut (ring)
		const faceRef1 = await getFirstFaceRef(waffle.page);
		expect(faceRef1).toBeTruthy();
		await selectFaceRef(waffle.page, faceRef1);
		await clickSketch(waffle.page);

		await clickCircle(waffle.page);
		await drawCircle(waffle.page, 0, 0, 30, 0);
		try {
			await waitForEntityCount(waffle.page, 2, 3000);
		} catch {
			await waffle.dumpState('mob-ring-inner-circle-failed');
		}

		await clickFinishSketch(waffle.page);
		try {
			await waitForFeatureCount(waffle.page, 3, 10000);
		} catch {
			await waffle.dumpState('mob-ring-inner-finish-failed');
		}

		await clickExtrude(waffle.page);
		await waffle.page.locator('[data-testid="extrude-depth"]').fill('10');
		await waffle.page.locator('[data-testid="extrude-cut"]').check();
		await waffle.page.locator('[data-testid="extrude-apply"]').click();
		try {
			await waitForFeatureCount(waffle.page, 4, 30000);
		} catch {
			await waffle.dumpState('mob-ring-cut-failed');
		}

		// Step 3: Select top face → draw small circle at offset → extrude cut (notch)
		const faceRef2 = await getFirstFaceRef(waffle.page);
		expect(faceRef2).toBeTruthy();
		await selectFaceRef(waffle.page, faceRef2);
		await clickSketch(waffle.page);

		await clickCircle(waffle.page);
		// Draw circle offset to the right (at quadrant position on ring wall)
		await drawCircle(waffle.page, 35, 0, 45, 0);
		try {
			await waitForEntityCount(waffle.page, 2, 3000);
		} catch {
			await waffle.dumpState('mob-ring-notch-circle-failed');
		}

		await clickFinishSketch(waffle.page);
		try {
			await waitForFeatureCount(waffle.page, 5, 10000);
		} catch {
			await waffle.dumpState('mob-ring-notch-finish-failed');
		}

		await clickExtrude(waffle.page);
		await waffle.page.locator('[data-testid="extrude-depth"]').fill('10');
		await waffle.page.locator('[data-testid="extrude-cut"]').check();
		await waffle.page.locator('[data-testid="extrude-apply"]').click();
		try {
			await waitForFeatureCount(waffle.page, 6, 30000);
		} catch {
			await waffle.dumpState('mob-ring-notch-cut-failed');
		}

		// Verify: mesh present, no crash
		const hasMesh = await hasMeshWithGeometry(waffle.page);
		expect(hasMesh).toBe(true);
		expectNoAnyCrash(crashTracker);
	});

	test('stacked coplanar union: no crash', async ({ waffle }) => {
		const crashTracker = collectCrashErrors(waffle.page);

		// Step 1: Create base box
		await createBaseBox(waffle);
		await waitForMeshWithGeometry(waffle.page);

		// Step 2: Select top face → sketch rectangle → extrude (stacked box, auto-union)
		const faceRef = await getFirstFaceRef(waffle.page);
		expect(faceRef).toBeTruthy();
		await selectFaceRef(waffle.page, faceRef);
		await clickSketch(waffle.page);

		await clickRectangle(waffle.page);
		await drawRectangle(waffle.page, -60, -40, 60, 40);
		try {
			await waitForEntityCount(waffle.page, 8, 3000);
		} catch {
			await waffle.dumpState('mob-stack-sketch-failed');
		}

		await clickFinishSketch(waffle.page);
		try {
			await waitForFeatureCount(waffle.page, 3, 10000);
		} catch {
			await waffle.dumpState('mob-stack-finish-failed');
		}

		await clickExtrude(waffle.page);
		await waffle.page.locator('[data-testid="extrude-depth"]').fill('10');
		await waffle.page.locator('[data-testid="extrude-apply"]').click();
		try {
			await waitForFeatureCount(waffle.page, 4, 15000);
		} catch {
			await waffle.dumpState('mob-stack-extrude-failed');
		}

		// Verify
		const hasMesh = await hasMeshWithGeometry(waffle.page);
		expect(hasMesh).toBe(true);
		expectNoAnyCrash(crashTracker);
	});
});
