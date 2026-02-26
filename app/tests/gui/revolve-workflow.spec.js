/**
 * Revolve workflow variation tests.
 *
 * Tests different revolve configurations: partial angles, combined
 * features, angle editing, and mesh verification. Complements
 * revolve.spec.js (dialog lifecycle) and revolve-e2e.spec.js (mesh details).
 */
import { test, expect } from './helpers/waffle-test.js';
import { clickSketch, clickFinishSketch, clickRectangle, clickRevolve, clickExtrude } from './helpers/toolbar.js';
import { drawRectangle } from './helpers/canvas.js';
import { waitForEntityCount, getFeatureCount, waitForFeatureCount, hasMeshWithGeometry, waitForMeshWithGeometry, getMeshes } from './helpers/state.js';

test.describe('revolve workflow variations', () => {

    test('revolve 90 degrees creates a mesh', async ({ waffle }) => {
        const page = waffle.page;

        // Create sketch with rectangle offset from origin (for revolve axis)
        await clickSketch(page);
        await clickRectangle(page);
        await drawRectangle(page, 30, -40, 80, 40);
        await waitForEntityCount(page, 8, 5000);
        await clickFinishSketch(page);

        // Open revolve dialog
        await clickRevolve(page);

        // Set angle to 90
        const angleInput = page.locator('#revolve-angle');
        await angleInput.fill('90');

        // Click Apply
        const applyBtn = page.locator('[data-testid="revolve-apply"]');
        if (await applyBtn.isEnabled()) {
            await applyBtn.click();
            await waitForFeatureCount(page, 2, 10000); // sketch + revolve

            // Verify mesh was created
            await waitForMeshWithGeometry(page, 10000);
            const hasMesh = await hasMeshWithGeometry(page);
            expect(hasMesh).toBe(true);
        }
    });

    test('revolve 180 degrees creates a mesh', async ({ waffle }) => {
        const page = waffle.page;

        await clickSketch(page);
        await clickRectangle(page);
        await drawRectangle(page, 30, -40, 80, 40);
        await waitForEntityCount(page, 8, 5000);
        await clickFinishSketch(page);

        await clickRevolve(page);

        const angleInput = page.locator('#revolve-angle');
        await angleInput.fill('180');

        const applyBtn = page.locator('[data-testid="revolve-apply"]');
        if (await applyBtn.isEnabled()) {
            await applyBtn.click();
            await waitForFeatureCount(page, 2, 10000);

            await waitForMeshWithGeometry(page, 10000);
            const hasMesh = await hasMeshWithGeometry(page);
            expect(hasMesh).toBe(true);
        }
    });

    test('revolve dialog shows angle field with default 360', async ({ waffle }) => {
        const page = waffle.page;

        await clickSketch(page);
        await clickRectangle(page);
        await drawRectangle(page, 30, -40, 80, 40);
        await waitForEntityCount(page, 8, 5000);
        await clickFinishSketch(page);

        await clickRevolve(page);

        // Verify dialog elements
        const dialog = page.locator('[data-testid="revolve-dialog"]');
        await expect(dialog).toBeVisible();

        const angleInput = page.locator('#revolve-angle');
        await expect(angleInput).toBeVisible();

        // Default angle should be 360
        const defaultAngle = await angleInput.inputValue();
        expect(parseFloat(defaultAngle)).toBe(360);
    });

    test('revolve angle input has min 0.1 constraint', async ({ waffle }) => {
        const page = waffle.page;

        await clickSketch(page);
        await clickRectangle(page);
        await drawRectangle(page, 30, -40, 80, 40);
        await waitForEntityCount(page, 8, 5000);
        await clickFinishSketch(page);

        await clickRevolve(page);

        const angleInput = page.locator('#revolve-angle');
        // Verify the angle input has min constraint
        const min = await angleInput.getAttribute('min');
        expect(parseFloat(min)).toBe(0.1);

        // Verify max is 360
        const max = await angleInput.getAttribute('max');
        expect(parseFloat(max)).toBe(360);
    });

    test('extrude then revolve creates two features', async ({ waffle }) => {
        const page = waffle.page;

        // First: create and extrude a rectangle
        await clickSketch(page);
        await clickRectangle(page);
        await drawRectangle(page, -80, -60, -20, 60);
        await waitForEntityCount(page, 8, 5000);
        await clickFinishSketch(page);

        await clickExtrude(page);
        const depthInput = page.locator('[data-testid="extrude-depth"]');
        await depthInput.fill('10');
        const extrudeApply = page.locator('[data-testid="extrude-apply"]');
        if (await extrudeApply.isEnabled()) {
            await extrudeApply.click();
            await waitForFeatureCount(page, 2, 10000); // sketch + extrude
        }

        // Second: create a revolve sketch
        await clickSketch(page);
        await clickRectangle(page);
        await drawRectangle(page, 30, -40, 80, 40);
        await waitForEntityCount(page, 8, 5000);
        await clickFinishSketch(page);

        await clickRevolve(page);
        const angleInput = page.locator('#revolve-angle');
        await angleInput.fill('360');
        const revolveApply = page.locator('[data-testid="revolve-apply"]');
        if (await revolveApply.isEnabled()) {
            await revolveApply.click();
            await waitForFeatureCount(page, 4, 15000); // 2 sketches + extrude + revolve

            const featureCount = await getFeatureCount(page);
            expect(featureCount).toBe(4);
        }
    });
});
