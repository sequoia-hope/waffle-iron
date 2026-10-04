/**
 * Bodies-list tests — verifies the Bodies section under the feature tree:
 * listing solid bodies, selecting a body (whole-body highlight), and renaming.
 *
 * A "body" is the mesh produced by a renderable feature; its name is the
 * producing feature's name, so renaming a body renames that feature.
 *
 * Uses real DOM clicks. No assertion-swallowing — waits throw on timeout.
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
	collectCrashErrors,
	expectNoAnyCrash,
	getFeatureTree,
	waitForEntityCount,
	waitForFeatureCount,
} from './helpers/state.js';

const BODY_ITEM = '.body-item';

/** Create a sketch + extrude, producing exactly one solid body. */
async function createBody(waffle) {
	await clickSketch(waffle.page);
	await clickRectangle(waffle.page);
	await drawRectangle(waffle.page, -80, -60, 80, 60);
	await waitForEntityCount(waffle.page, 8, 5000);
	await clickFinishSketch(waffle.page);
	await waitForFeatureCount(waffle.page, 1, 10000);

	await clickExtrude(waffle.page);
	await waffle.page.locator('[data-testid="extrude-depth"]').fill('10');
	await waffle.page.locator('[data-testid="extrude-apply"]').click();
	await waitForFeatureCount(waffle.page, 2, 10000);
}

/** Sketch a rectangle (screen coords) and extrude it (merge default). */
async function sketchAndExtrudeRect(waffle, x1, y1, x2, y2, expectFeatures) {
	await clickSketch(waffle.page);
	await clickRectangle(waffle.page);
	await drawRectangle(waffle.page, x1, y1, x2, y2);
	await waitForEntityCount(waffle.page, 8, 5000);
	await clickFinishSketch(waffle.page);
	await clickExtrude(waffle.page);
	await waffle.page.locator('[data-testid="extrude-depth"]').fill('10');
	await waffle.page.locator('[data-testid="extrude-apply"]').click();
	await waitForFeatureCount(waffle.page, expectFeatures, 10000);
}

test.describe('bodies list', () => {
	test('a single extrude that yields two disjoint bodies lists two', async ({ waffle }) => {
		const crashes = collectCrashErrors(waffle.page);
		// Two far-apart rectangles; the second extrude (merge) auto-unions with
		// the disjoint first, producing one solid with two disjoint lumps that
		// kernel-v2 splits into two bodies (the F0015-class fix).
		await sketchAndExtrudeRect(waffle, -160, -60, -60, 60, 2);
		await sketchAndExtrudeRect(waffle, 60, -60, 160, 60, 4);

		await expect(waffle.page.locator('[data-testid="bodies-toggle"]')).toContainText(
			'Bodies (2)'
		);
		await expect(waffle.page.locator(BODY_ITEM)).toHaveCount(2);
		expectNoAnyCrash(crashes);
	});

	test('Bodies section lists one body after an extrude', async ({ waffle }) => {
		const crashes = collectCrashErrors(waffle.page);
		await createBody(waffle);

		const section = waffle.page.locator('[data-testid="bodies-toggle"]');
		await expect(section).toBeVisible();
		await expect(section).toContainText('Bodies (1)');

		await expect(waffle.page.locator(BODY_ITEM)).toHaveCount(1);
		expectNoAnyCrash(crashes);
	});

	test('clicking a body highlights it (selected class), re-click clears', async ({ waffle }) => {
		const crashes = collectCrashErrors(waffle.page);
		await createBody(waffle);

		const body = waffle.page.locator('[data-testid="body-item-0"]');
		await body.click();
		await expect(body).toHaveClass(/selected/);

		// Clicking the selected body again toggles it off.
		await body.click();
		await expect(body).not.toHaveClass(/selected/);
		expectNoAnyCrash(crashes);
	});

	test('double-click renames the body independently of its feature', async ({ waffle }) => {
		const crashes = collectCrashErrors(waffle.page);
		await createBody(waffle);

		// Capture the producing feature's name before renaming the body.
		const before = await getFeatureTree(waffle.page);
		const featureNames = before.features.map((f) => f.name);

		const body = waffle.page.locator('[data-testid="body-item-0"]');
		await body.dblclick();

		const input = body.locator('.body-rename-input');
		await expect(input).toBeVisible();
		await input.fill('Main Body');
		await waffle.page.keyboard.press('Enter');

		await expect(waffle.page.locator('[data-testid="body-item-0"]')).toContainText('Main Body');

		// Step 2: body names are independent — the feature name is UNCHANGED.
		// (Persistence across save/load is covered by the file-format Rust test
		// `body_names_survive_round_trip`.)
		const after = await getFeatureTree(waffle.page);
		expect(after.features.map((f) => f.name)).toEqual(featureNames);
		expect(featureNames).not.toContain('Main Body');
		expectNoAnyCrash(crashes);
	});

	/**
	 * M1 — "the properties panel shows mass and centre of mass"
	 * (`specs/drawings_and_mbd.md` §9).
	 *
	 * There is no properties panel; the numbers are a disclosure on the body's
	 * own row, like the assembly panel's per-instance `pos`. What these tests
	 * own is the half the engine cannot: that the panel prints the numbers the
	 * engine reported, in the document's unit, and that it never dresses an
	 * absent material up as a mass.
	 */
	test.describe('body properties', () => {
		test('the disclosure prints the volume and area the engine measured', async ({ waffle }) => {
			const crashes = collectCrashErrors(waffle.page);
			await createBody(waffle);

			const props = waffle.page.locator('[data-testid="body-props-0"]');
			await expect(props).toHaveCount(0);
			await waffle.page.locator('[data-testid="body-props-toggle-0"]').click();
			await expect(props).toBeVisible();

			// The engine's own answer for the same body, through the same
			// accessor the panel uses. Comparing the two is what pins the unit
			// conversion: m³ → mm³ is a factor of 1e9, and a wrong exponent is
			// invisible in a number read on its own.
			const bodyId = (await waffle.page.evaluate(() => window.__waffle.getMeshes()))[0].bodyId;
			const measured = await waffle.page.evaluate(
				(id) => window.__waffle.measureBodyMass(id),
				bodyId
			);
			expect(measured.volume_m3).toBeGreaterThan(0);

			const number = (text) => Number(String(text).split(' ')[0]);
			const volumeText = await props.locator('[data-testid="body-prop-volume-0"]').textContent();
			const areaText = await props.locator('[data-testid="body-prop-area-0"]').textContent();
			expect(volumeText).toContain('mm³');
			expect(areaText).toContain('mm²');
			// Six significant digits, so a relative comparison at 1e-5.
			expect(number(volumeText)).toBeCloseTo(measured.volume_m3 * 1e9, -1);
			expect(
				Math.abs(number(volumeText) / (measured.volume_m3 * 1e9) - 1),
				'the printed volume is the measured one in mm³'
			).toBeLessThan(1e-5);
			expect(
				Math.abs(number(areaText) / (measured.surface_area_m2 * 1e6) - 1),
				'the printed area is the measured one in mm²'
			).toBeLessThan(1e-5);

			// The tier the numbers came from is always stated: an exact answer
			// and a mesh approximation must never look alike.
			const method = await props.locator('[data-testid="body-prop-method-0"]').textContent();
			expect(method.trim() === 'exact' || method.includes('mesh')).toBe(true);
			expect(method.trim()).toBe(measured.method === 'exact' ? 'exact' : method.trim());
			if (measured.method !== 'exact') expect(method).toContain('±');

			// Clicking again puts it away.
			await waffle.page.locator('[data-testid="body-props-toggle-0"]').click();
			await expect(props).toHaveCount(0);
			expectNoAnyCrash(crashes);
		});

		test('a body with no material names the absence instead of printing a mass', async ({
			waffle
		}) => {
			const crashes = collectCrashErrors(waffle.page);
			await createBody(waffle);
			await waffle.page.locator('[data-testid="body-props-toggle-0"]').click();
			const props = waffle.page.locator('[data-testid="body-props-0"]');
			await expect(props).toBeVisible();

			// M1's review settled which answer the engine gives: a body with
			// no material has NO mass. `density_kg_m3`, `mass_kg`,
			// `inertia_at_centroid` and `principal_moments` all come back
			// null, with `mass_unavailable` naming the remedy — rather than
			// measured at a default density of 1, where `mass_kg` is
			// numerically the volume in m³ and is not the mass of anything.
			// The geometry is unaffected, so the volume row still reads.
			const measured = await waffle.page.evaluate(async () => {
				const id = window.__waffle.getMeshes()[0].bodyId;
				try {
					return { ok: await window.__waffle.measureBodyMass(id) };
				} catch (err) {
					return { refused: String(err?.message ?? err) };
				}
			});
			const mass = (
				await props.locator('[data-testid="body-prop-mass-0"]').textContent()
			).trim();

			expect(measured.refused, 'the measurement itself still succeeds').toBeFalsy();
			// The engine's answer, on the wire.
			expect(measured.ok.density_kg_m3, 'no density').toBeNull();
			expect(measured.ok.mass_kg, 'no mass').toBeNull();
			expect(measured.ok.inertia_at_centroid, 'no tensor').toBeNull();
			expect(measured.ok.principal_moments, 'no moments').toBeNull();
			expect(measured.ok.mass_unavailable).toContain('no material');
			// ...and the density-FREE quantities are there, which is why the
			// panel can still fill its volume and area rows.
			expect(measured.ok.volume_m3).toBeGreaterThan(0);
			expect(measured.ok.surface_area_m2).toBeGreaterThan(0);
			expect(Array.isArray(measured.ok.principal_axes)).toBe(true);

			// The panel NAMES the absence; it never prints a bare number that
			// a reader would take for a mass.
			expect(mass).toBe('no material assigned');
			expect(mass).not.toMatch(/^[\d.]+\s*k?g$/);
			const material = (
				await props.locator('[data-testid="body-prop-material-0"]').textContent()
			).trim();
			expect(material).toBe('none');
			// The volume is still measured: a body with no material still has
			// one, and losing it because the mass is unavailable would be the
			// worse answer.
			const volume = await props.locator('[data-testid="body-prop-volume-0"]').textContent();
			expect(Number(volume.split(' ')[0])).toBeGreaterThan(0);
			expectNoAnyCrash(crashes);
		});

		test('assigning a material from the picker turns the volume into a mass', async ({
			waffle
		}) => {
			const crashes = collectCrashErrors(waffle.page);
			await createBody(waffle);

			// The material table starts EMPTY, and the picker lists the
			// ENGINE's table — so the fixture has to add a material the way
			// anything else does, through the engine. `material_set` is the
			// agent tool for it, run in this page's own executor.
			const added = await waffle.page.evaluate(async () => {
				const api = window.__waffleAgentExecutor;
				const ctx = {
					agentName: 'body-props-test',
					isPaused: () => false,
					pause: () => {},
					isCancelled: () => false
				};
				return api.executeTool(
					'material_set',
					{ name: 'Aluminium', density_kg_m3: 2700 },
					ctx
				);
			});
			expect(added.isError, JSON.stringify(added.structuredContent ?? {})).toBeFalsy();

			await waffle.page.locator('[data-testid="body-props-toggle-0"]').click();
			const props = waffle.page.locator('[data-testid="body-props-0"]');
			await expect(props).toBeVisible();

			const select = props.locator('[data-testid="body-material-select-0"]');
			await expect(select).toBeVisible();
			// `none` plus the one material in the engine's table.
			await expect(select.locator('option')).toHaveCount(2);
			await select.selectOption('Aluminium');

			// The mass follows from the volume and the density, both of which
			// the engine reported — so the check is the product, not a
			// hard-coded number: the body's size comes from the screen-space
			// rectangle this fixture drew.
			await expect(props.locator('[data-testid="body-prop-material-0"]')).toContainText(
				'Aluminium'
			);
			const massText = await props.locator('[data-testid="body-prop-mass-0"]').textContent();
			const measured = await waffle.page.evaluate(async () => {
				const id = window.__waffle.getMeshes()[0].bodyId;
				return window.__waffle.measureBodyMass(id);
			});
			expect(measured.density_kg_m3).toBe(2700);
			const grams = measured.volume_m3 * 2700 * 1000;
			const printed = Number(massText.trim().split(' ')[0]);
			const printedGrams = massText.trim().endsWith('kg') ? printed * 1000 : printed;
			expect(Math.abs(printedGrams / grams - 1)).toBeLessThan(1e-4);
			expectNoAnyCrash(crashes);
		});

		test('the centre of mass is three coordinates in the display unit', async ({ waffle }) => {
			const crashes = collectCrashErrors(waffle.page);
			await createBody(waffle);
			await waffle.page.locator('[data-testid="body-props-toggle-0"]').click();
			const props = waffle.page.locator('[data-testid="body-props-0"]');
			await expect(props).toBeVisible();

			const measured = await waffle.page.evaluate(async () => {
				const id = window.__waffle.getMeshes()[0].bodyId;
				return window.__waffle.measureBodyMass(id);
			});
			const text = (
				await props.locator('[data-testid="body-prop-centroid-0"]').textContent()
			).trim();
			expect(text).toContain('mm');
			const printed = text.replace('mm', '').split(',').map(Number);
			expect(printed).toHaveLength(3);
			// Each coordinate is the engine's, in millimetres, at the
			// document's two places — so the comparison is to 0.005 mm.
			for (let i = 0; i < 3; i++) {
				expect(printed[i]).toBeCloseTo(measured.centroid[i] * 1000, 2);
			}
			expectNoAnyCrash(crashes);
		});
	});
});
