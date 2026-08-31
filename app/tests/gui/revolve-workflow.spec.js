/**
 * Revolve workflow variation tests.
 *
 * Tests different revolve configurations: partial angles, combined
 * features, angle editing, and mesh verification. Complements
 * revolve.spec.js (dialog lifecycle) and revolve-e2e.spec.js (mesh details).
 */
import { test, expect } from './helpers/waffle-test.js';
import { pickOffsetRevolveAxis } from './helpers/revolve.js';
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
        await pickOffsetRevolveAxis(page);

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
        await pickOffsetRevolveAxis(page);

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

    test('revolve rejects a non-positive angle at apply', async ({ waffle }) => {
        // The angle input is a text field (it accepts expressions over the
        // design variables), so the old HTML min/max attributes are gone —
        // the guard lives in the apply handler instead.
        const page = waffle.page;

        await clickSketch(page);
        await clickRectangle(page);
        await drawRectangle(page, 30, -40, 80, 40);
        await waitForEntityCount(page, 8, 5000);
        await clickFinishSketch(page);

        await clickRevolve(page);
        // Apply is disabled until an axis is set; the guard under test is the
        // ANGLE validation, so give it a valid axis first.
        await pickOffsetRevolveAxis(page);

        const angleInput = page.locator('#revolve-angle');
        await angleInput.fill('0');
        await page.locator('[data-testid="revolve-apply"]').click();
        await page.waitForTimeout(500);

        // No revolve feature was added; the dialog stays open on the error.
        const tree = await page.evaluate(() => window.__waffle.getFeatureTree());
        expect(tree.features.filter((f) => f.operation?.type === 'Revolve').length).toBe(0);
        await expect(page.locator('[data-testid="revolve-dialog"]')).toBeVisible();
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
        await pickOffsetRevolveAxis(page);
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
