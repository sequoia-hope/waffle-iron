/**
 * Studio PBR Renderer & Real Material Appearance suite
 * (`specs/studio_pbr_renderer.md`).
 *
 * Exercises:
 *   1. Default Shaded mode invariants & settings persistence
 *   2. Curated engineering material presets -> engine `FeatureTree.materials`,
 *      `MeasureMass`, and real RGB material appearance in Shaded & Studio modes
 *   3. Studio PBR pipeline: procedural PMREM IBL on `Polished Chrome`,
 *      GTAO + GTSO cavity occlusion (Ortho & Perspective), SSR inter-body
 *      reflection, Z-up ground contact shadow catcher, and progressive Halton
 *      idle accumulation (`settled` convergence + reset on orbit)
 *   4. ViewCube dropdown & Viewport context menu Studio toggles
 */
import { test, expect } from './helpers/waffle-test.js';
import {
	clickSketch,
	clickRectangle,
	clickFinishSketch,
	clickExtrude,
	clickToolbarAction
} from './helpers/toolbar.js';
import { drawRectangle, orbitDrag } from './helpers/canvas.js';
import {
	collectCrashErrors,
	expectNoAnyCrash,
	waitForEntityCount,
	waitForFeatureCount
} from './helpers/state.js';

/** Create a solid extruded block on the Top (XY, Z-up) plane and hide datum planes/axes. */
async function createCleanBlock(waffle, depthMm = '25') {
	const page = waffle.page;
	await clickSketch(page, 'top');
	await clickRectangle(page);
	await drawRectangle(page, -90, -70, 90, 70);
	await waitForEntityCount(page, 8, 5000);
	await clickFinishSketch(page);
	await waitForFeatureCount(page, 1, 10000);

	await clickExtrude(page);
	await expect(page.locator('[data-testid="extrude-region-0"]')).toBeVisible({ timeout: 5000 });
	await page.locator('[data-testid="extrude-depth"]').fill(depthMm);
	await page.locator('[data-testid="extrude-apply"]').click();
	await waitForFeatureCount(page, 2, 10000);

	// Hide datum planes & origin triad so translucent datum quads do not tint sampled pixels.
	await clickToolbarAction(page, 'toggle-planes');
	await clickToolbarAction(page, 'toggle-axes');
	await page.evaluate(() => {
		window.dispatchEvent(new CustomEvent('waffle-snap-view-and-fit', { detail: { view: 'iso' } }));
	});
	await page.waitForTimeout(250);
}

/**
 * Capture the viewport `<canvas>` via Playwright's compositor screenshot and
 * return patch-averaged RGBA [0..255] values at normalized (nx, ny) coordinates.
 */
async function sampleCanvasPoints(page, points, radius = 2) {
	const buf = await page.locator('[data-testid="viewport"] canvas').screenshot();
	const b64 = buf.toString('base64');
	return page.evaluate(
		async ({ b64, pts, rad }) => {
			const img = new Image();
			img.src = `data:image/png;base64,${b64}`;
			await img.decode();
			const c = document.createElement('canvas');
			c.width = img.width;
			c.height = img.height;
			const ctx = c.getContext('2d');
			ctx.drawImage(img, 0, 0);
			const full = ctx.getImageData(0, 0, img.width, img.height).data;
			return pts.map(({ x: nx, y: ny }) => {
				const cx = Math.min(img.width - 1, Math.max(0, Math.round(nx * img.width)));
				const cy = Math.min(img.height - 1, Math.max(0, Math.round(ny * img.height)));
				let r = 0,
					g = 0,
					b = 0,
					a = 0,
					n = 0;
				for (let dy = -rad; dy <= rad; dy++) {
					for (let dx = -rad; dx <= rad; dx++) {
						const px = Math.min(img.width - 1, Math.max(0, cx + dx));
						const py = Math.min(img.height - 1, Math.max(0, cy + dy));
						const o = (py * img.width + px) * 4;
						r += full[o];
						g += full[o + 1];
						b += full[o + 2];
						a += full[o + 3];
						n++;
					}
				}
				return {
					r: r / n,
					g: g / n,
					b: b / n,
					a: a / n,
					lum: (0.2126 * r + 0.7152 * g + 0.0722 * b) / n
				};
			});
		},
		{ b64, pts: points, rad: radius }
	);
}

/** Wait until the Studio progressive accumulator settles on the current view. */
async function waitForStudioSettled(page, timeout = 20000) {
	await page.waitForFunction(
		() => {
			const st = window.__waffle?.getStudioStats?.();
			return st && st.active === true && st.environmentLoaded === true && st.settled === true;
		},
		null,
		{ timeout }
	);
}

test.describe('Studio PBR Renderer & Real Materials', () => {
	test('defaults to classic Shaded mode and persists Studio settings across reload', async ({
		waffle
	}) => {
		const page = waffle.page;
		const crashes = collectCrashErrors(page);

		const initial = await page.evaluate(() => ({
			settings: window.__waffle.getSettings(),
			stats: window.__waffle.getStudioStats()
		}));
		expect(initial.settings.viewportShading).toBe('shaded');
		expect(initial.settings.studioAO).toBe(true);
		expect(initial.settings.studioSSR).toBe(true);
		expect(initial.settings.studioGroundShadow).toBe(true);
		expect(initial.settings.studioToneMapping).toBe('neutral');
		expect(initial.stats.active).toBe(false);

		// Toggle via ViewCube dropdown
		await page.locator('[data-testid="viewcube-overlay"] .dropdown-toggle').click();
		const vcToggle = page.locator('[data-testid="viewcube-studio-toggle"]');
		await expect(vcToggle).toBeVisible();
		await expect(vcToggle).not.toBeChecked();
		await vcToggle.click();
		await expect(vcToggle).toBeChecked();

		await waitForStudioSettled(page);

		// Verify Settings -> Appearance reflects Studio mode and persists across reload
		await page.locator('[data-testid="toolbar-btn-settings"]').click();
		await page.locator('[data-testid="settings-section-appearance"]').click();
		const shadingSelect = page.locator('[data-testid="setting-viewport-shading"]');
		await expect(shadingSelect).toHaveValue('studio');
		await page.locator('[data-testid="setting-studio-tonemapping"]').selectOption('agx');
		await page.locator('[data-testid="settings-close"]').click();

		await page.reload();
		await waffle.waitForReady();
		const reloaded = await page.evaluate(() => window.__waffle.getSettings());
		expect(reloaded.viewportShading).toBe('studio');
		expect(reloaded.studioToneMapping).toBe('agx');
		await waitForStudioSettled(page);

		expectNoAnyCrash(crashes);
	});

	test('engineering material presets populate engine table, compute SI mass, and render real appearance colors', async ({
		waffle
	}) => {
		const page = waffle.page;
		const crashes = collectCrashErrors(page);
		await createCleanBlock(waffle, '20');

		// Verify curated engineering presets exist
		const presets = await page.evaluate(() => window.__waffle.getMaterialPresets());
		expect(presets.length).toBeGreaterThanOrEqual(12);
		const redPreset = presets.find((p) => p.name === 'Anodized Red');
		const bluePreset = presets.find((p) => p.name === 'Anodized Blue');
		expect(redPreset?.density_kg_m3).toBe(2700);
		expect(bluePreset?.density_kg_m3).toBe(2700);

		// Open Materials section in FeatureTree and add Anodized Red + Anodized Blue via UI
		const matToggle = page.locator('[data-testid="materials-toggle"]');
		await expect(matToggle).toBeVisible();
		await matToggle.click();

		const presetSelect = page.locator('[data-testid="material-preset-select"]');
		await expect(presetSelect).toBeVisible();
		await presetSelect.selectOption('Anodized Red');
		await expect(page.locator('[data-testid="material-item-0"]')).toContainText('Anodized Red');

		await presetSelect.selectOption('Anodized Blue');
		await expect(page.locator('[data-testid="material-item-1"]')).toContainText('Anodized Blue');

		// Assign Anodized Red to body 0 from Bodies properties picker
		await page.locator('[data-testid="body-props-toggle-0"]').click();
		const bodyMatSelect = page.locator('[data-testid="body-material-select-0"]');
		await expect(bodyMatSelect).toBeVisible();
		await bodyMatSelect.selectOption('Anodized Red');

		// Mass must now be positive and match 2700 kg/m³
		const measured = await page.evaluate(async () => {
			const id = window.__waffle.getMeshes()[0].bodyId;
			return window.__waffle.measureBodyMass(id);
		});
		expect(measured.density_kg_m3).toBe(2700);
		expect(measured.mass_kg).toBeGreaterThan(0);

		// Enable Studio mode and compare center pixel under Anodized Red vs Anodized Blue
		await page.evaluate(() => window.__waffle.updateSettings({ viewportShading: 'studio' }));
		await waitForStudioSettled(page);
		const [redPx] = await sampleCanvasPoints(page, [{ x: 0.5, y: 0.5 }], 4);

		await bodyMatSelect.selectOption('Anodized Blue');
		await waitForStudioSettled(page);
		const [bluePx] = await sampleCanvasPoints(page, [{ x: 0.5, y: 0.5 }], 4);

		expect(redPx.r).toBeGreaterThan(redPx.b + 25);
		expect(bluePx.b).toBeGreaterThan(bluePx.r + 25);

		expectNoAnyCrash(crashes);
	});

	test('Polished Chrome metalness=1.0 reflects procedural studio PMREM instead of rendering black', async ({
		waffle
	}) => {
		const page = waffle.page;
		const crashes = collectCrashErrors(page);
		await createCleanBlock(waffle, '30');

		await page.evaluate(async () => {
			const presets = window.__waffle.getMaterialPresets();
			const chrome = presets.find((p) => p.name === 'Polished Chrome');
			await window.__waffle.upsertMaterial({
				name: chrome.name,
				density_kg_m3: chrome.density_kg_m3,
				appearance: chrome.appearance
			});
			const bodyId = window.__waffle.getMeshes()[0].bodyId;
			await window.__waffle.setBodyMaterial(bodyId, chrome.name);
			window.__waffle.updateSettings({ viewportShading: 'studio' });
		});

		await waitForStudioSettled(page);
		const [centerPx] = await sampleCanvasPoints(page, [{ x: 0.5, y: 0.5 }], 4);
		// Pure metalness=1.0 without an environment map renders near-zero black (~0..15);
		// with our Studio PMREM softbox rig it reflects bright studio illumination.
		expect(centerPx.lum).toBeGreaterThan(75);

		expectNoAnyCrash(crashes);
	});

	test('GTAO + GTSO darkens concave interior corners in both Orthographic and Perspective cameras', async ({
		waffle
	}) => {
		const page = waffle.page;
		const crashes = collectCrashErrors(page);

		await createCleanBlock(waffle, '60');

		// Sketch a central shaft on Top (XY) and cut through the block to create a deep interior cavity
		await clickSketch(page, 'top');
		await clickRectangle(page);
		await drawRectangle(page, -35, -35, 35, 35);
		await waitForEntityCount(page, 8, 5000);
		await clickFinishSketch(page);
		await waitForFeatureCount(page, 3, 10000);

		await clickExtrude(page);
		await expect(page.locator('[data-testid="extrude-region-0"]')).toBeVisible({ timeout: 5000 });
		await page.locator('[data-testid="extrude-depth-mode"]').selectOption('ThroughAll');
		await page.locator('[data-testid="extrude-combine"]').selectOption('Cut');
		await page.locator('[data-testid="extrude-apply"]').click();
		await waitForFeatureCount(page, 4, 10000);

		await page.evaluate(() => {
			window.dispatchEvent(new CustomEvent('waffle-snap-view-and-fit', { detail: { view: 'iso' } }));
			window.__waffle.updateSettings({
				viewportShading: 'studio',
				studioAO: false,
				studioSSR: false,
				studioGroundShadow: false
			});
		});
		await waitForStudioSettled(page);

		for (const proj of ['orthographic', 'perspective']) {
			await page.evaluate((p) => {
				if (window.__waffle.getCameraProjection() !== p) {
					window.__waffle.toggleCameraProjection();
				}
				window.__waffle.updateSettings({ studioAO: false });
			}, proj);
			await waitForStudioSettled(page);

			// Project points inside the visible concave inner corner of the cut shaft
			// (plus a flat outer face control and a pure background pixel) for this projection!
			const gridPts = await page.evaluate(() => {
				const v = window.__waffle.getMeshes()[0].vertices;
				let minX = Infinity, maxX = -Infinity;
				let minY = Infinity, maxY = -Infinity;
				let minZ = Infinity, maxZ = -Infinity;
				for (let i = 0; i < v.length; i += 3) {
					if (v[i] < minX) minX = v[i];
					if (v[i] > maxX) maxX = v[i];
					if (v[i + 1] < minY) minY = v[i + 1];
					if (v[i + 1] > maxY) maxY = v[i + 1];
					if (v[i + 2] < minZ) minZ = v[i + 2];
					if (v[i + 2] > maxZ) maxZ = v[i + 2];
				}
				// Find inner cut-hole bounds (strictly inside outer bbox)
				let inMinX = Infinity, inMaxX = -Infinity;
				let inMinZ = Infinity, inMaxZ = -Infinity;
				const epsX = (maxX - minX) * 0.05;
				const epsZ = (maxZ - minZ) * 0.05;
				for (let i = 0; i < v.length; i += 3) {
					const x = v[i], z = v[i + 2];
					if (x > minX + epsX && x < maxX - epsX && z > minZ + epsZ && z < maxZ - epsZ) {
						if (x < inMinX) inMinX = x;
						if (x > inMaxX) inMaxX = x;
						if (z < inMinZ) inMinZ = z;
						if (z > inMaxZ) inMaxZ = z;
					}
				}
				const holeW = inMaxX - inMinX;
				const holeH = inMaxZ - inMinZ;
				const depthIn = Math.min(holeW, holeH) * 0.25;
				// Visible interior bottom/left walls near the concave corner of the shaft
				return [
					window.__waffle.projectWorldPoint([inMinX + holeW * 0.18, minY + depthIn, inMinZ + 1e-4]),
					window.__waffle.projectWorldPoint([inMinX + 1e-4, minY + depthIn, inMinZ + holeH * 0.22]),
					window.__waffle.projectWorldPoint([inMinX + holeW * 0.30, minY + depthIn * 0.8, inMinZ + 1e-4]),
					{ x: 0.5, y: 0.5 }, // flat outer face control (must NOT self-occlude)
					{ x: 0.06, y: 0.06 } // pure background pixel
				];
			});

			const noAoSamples = await sampleCanvasPoints(page, gridPts, 1);

			await page.evaluate(() => window.__waffle.updateSettings({ studioAO: true }));
			await waitForStudioSettled(page);
			const withAoSamples = await sampleCanvasPoints(page, gridPts, 1);

			const avgNoAo =
				noAoSamples.slice(0, 3).reduce((s, p) => s + p.lum, 0) / 3;
			const avgWithAo =
				withAoSamples.slice(0, 3).reduce((s, p) => s + p.lum, 0) / 3;

			expect(
				avgWithAo,
				`GTAO should darken pocket cavity in ${proj} (noAO=${avgNoAo.toFixed(2)}, withAO=${avgWithAo.toFixed(2)})`
			).toBeLessThan(avgNoAo - 2.0);

			// Flat outer face must stay free of false self-occlusion
			expect(Math.abs(withAoSamples[3].lum - noAoSamples[3].lum)).toBeLessThan(1.5);
			// Background pixel must stay untouched by tone mapping / AO (Invariant I6)
			expect(Math.abs(withAoSamples[4].lum - noAoSamples[4].lum)).toBeLessThan(1.5);
		}

		expectNoAnyCrash(crashes);
	});

	test('Studio ground contact shadow darkens floor beneath Z-min without intercepting clicks, and progressive settle halts idle passes', async ({
		waffle
	}) => {
		const page = waffle.page;
		const crashes = collectCrashErrors(page);
		await createCleanBlock(waffle, '30');

		await page.evaluate(() => {
			window.dispatchEvent(new CustomEvent('waffle-snap-view-and-fit', { detail: { view: 'iso' } }));
			window.__waffle.updateSettings({
				viewportShading: 'studio',
				studioGroundShadow: false
			});
		});
		await waitForStudioSettled(page);

		// Project a point just outside the front-right base edge (+X, -Y, minZ) in iso view,
		// so the body itself does not occlude the floor apron point from (+X, -Y, +Z)!
		const floorProbe = await page.evaluate(() => {
			const meshes = window.__waffle.getMeshes();
			const v = meshes[0].vertices;
			let minX = Infinity,
				maxX = -Infinity,
				minY = Infinity,
				maxY = -Infinity,
				minZ = Infinity;
			for (let i = 0; i < v.length; i += 3) {
				if (v[i] < minX) minX = v[i];
				if (v[i] > maxX) maxX = v[i];
				if (v[i + 1] < minY) minY = v[i + 1];
				if (v[i + 1] > maxY) maxY = v[i + 1];
				if (v[i + 2] < minZ) minZ = v[i + 2];
			}
			const spanX = maxX - minX;
			const spanY = maxY - minY;
			return window.__waffle.projectWorldPoint([
				maxX + spanX * 0.05,
				minY - spanY * 0.05,
				minZ
			]);
		});

		const [noShadowFloor, noShadowSky] = await sampleCanvasPoints(
			page,
			[floorProbe, { x: 0.05, y: 0.05 }],
			2
		);

		await page.evaluate(() => window.__waffle.updateSettings({ studioGroundShadow: true }));
		await waitForStudioSettled(page);

		const [withShadowFloor, withShadowSky] = await sampleCanvasPoints(
			page,
			[floorProbe, { x: 0.05, y: 0.05 }],
			2
		);

		expect(
			withShadowFloor.lum,
			`Contact shadow should darken floor apron (off=${noShadowFloor.lum.toFixed(2)}, on=${withShadowFloor.lum.toFixed(2)})`
		).toBeLessThan(noShadowFloor.lum - 2.0);
		expect(Math.abs(withShadowSky.lum - noShadowSky.lum)).toBeLessThan(1.5);

		// Clicking the ground shadow apron must NOT select any CAD entity (Invariant I8)
		const canvasBox = await page.locator('[data-testid="viewport"] canvas').boundingBox();
		await page.mouse.click(
			Math.round(canvasBox.x + floorProbe.x * canvasBox.width),
			Math.round(canvasBox.y + floorProbe.y * canvasBox.height)
		);
		const selCount = await page.evaluate(() => window.__waffle.getSelectedRefs().length);
		expect(selCount).toBe(0);

		// Progressive idle convergence oracle (Invariant I7):
		// Once settled, passCount must remain constant over 300ms of idle time,
		// then reset and re-accumulate when the camera orbits.
		await waitForStudioSettled(page);
		const statsBeforeIdle = await page.evaluate(() => window.__waffle.getStudioStats());
		expect(statsBeforeIdle.settled).toBe(true);
		expect(statsBeforeIdle.sampleCount).toBeGreaterThanOrEqual(statsBeforeIdle.targetSamples);

		await page.waitForTimeout(300);
		const statsAfterIdle = await page.evaluate(() => window.__waffle.getStudioStats());
		expect(statsAfterIdle.passCount).toBe(statsBeforeIdle.passCount);

		await orbitDrag(page, 0, 0, 60, -30);
		await waitForStudioSettled(page);
		const statsAfterOrbit = await page.evaluate(() => window.__waffle.getStudioStats());
		expect(statsAfterOrbit.passCount).toBeGreaterThan(statsAfterIdle.passCount);
		expect(statsAfterOrbit.settled).toBe(true);

		expectNoAnyCrash(crashes);
	});

	test('SSR reflects bright neighboring geometry onto a shiny metallic face', async ({
		waffle
	}) => {
		const page = waffle.page;
		const crashes = collectCrashErrors(page);

		// Body 0: Tall mirror-chrome wall on the left (-110..-65)
		await clickSketch(page, 'top');
		await clickRectangle(page);
		await drawRectangle(page, -110, -80, -65, 80);
		await waitForEntityCount(page, 8, 5000);
		await clickFinishSketch(page);
		await waitForFeatureCount(page, 1, 10000);

		await clickExtrude(page);
		await expect(page.locator('[data-testid="extrude-region-0"]')).toBeVisible({ timeout: 5000 });
		await page.locator('[data-testid="extrude-depth"]').fill('75');
		await page.locator('[data-testid="extrude-combine"]').selectOption('NewBody');
		await page.locator('[data-testid="extrude-apply"]').click();
		await waitForFeatureCount(page, 2, 10000);

		// Body 1: Disjoint bright Signal Red slab right beside it (-55..60)
		await clickSketch(page, 'top');
		await clickRectangle(page);
		await drawRectangle(page, -55, -80, 60, 80);
		await waitForEntityCount(page, 8, 5000);
		await clickFinishSketch(page);
		await waitForFeatureCount(page, 3, 10000);

		await clickExtrude(page);
		await expect(page.locator('[data-testid="extrude-region-0"]')).toBeVisible({ timeout: 5000 });
		await page.locator('[data-testid="extrude-depth"]').fill('30');
		await page.locator('[data-testid="extrude-combine"]').selectOption('NewBody');
		await page.locator('[data-testid="extrude-apply"]').click();
		await waitForFeatureCount(page, 4, 10000);

		await clickToolbarAction(page, 'toggle-planes');
		await clickToolbarAction(page, 'toggle-axes');

		const probeInfo = await page.evaluate(async () => {
			await window.__waffle.upsertMaterial({
				name: 'Polished Chrome',
				density_kg_m3: 7190,
				appearance: { color: [0.92, 0.92, 0.94], metalness: 1.0, roughness: 0.03 }
			});
			await window.__waffle.upsertMaterial({
				name: 'Signal Red',
				density_kg_m3: 1200,
				appearance: { color: [1.0, 0.04, 0.04], metalness: 0.0, roughness: 0.25 }
			});
			const meshes = window.__waffle.getMeshes();
			const avgX = (m) => {
				let s = 0;
				for (let i = 0; i < m.vertices.length; i += 3) s += m.vertices[i];
				return s / Math.max(1, m.vertices.length / 3);
			};
			// Assign Chrome to the -X body so its +X face looks directly at both the
			// (+X, -Y, +Z) iso camera AND the +X Signal Red neighbor across the gap!
			const [chromeBody, redBody] =
				avgX(meshes[0]) < avgX(meshes[1]) ? [meshes[0], meshes[1]] : [meshes[1], meshes[0]];
			await window.__waffle.setBodyMaterial(chromeBody.bodyId, 'Polished Chrome');
			await window.__waffle.setBodyMaterial(redBody.bodyId, 'Signal Red');
			window.dispatchEvent(new CustomEvent('waffle-snap-view-and-fit', { detail: { view: 'iso' } }));
			return { meshCount: meshes.length, chromeBodyId: chromeBody.bodyId };
		});
		expect(probeInfo.meshCount).toBe(2);
		await page.waitForTimeout(250);

		const innerWallPt = await page.evaluate((chromeBodyId) => {
			const mChrome = window.__waffle.getMeshes().find((m) => m.bodyId === chromeBodyId);
			const v = mChrome.vertices;
			let maxX = -Infinity,
				minY = Infinity,
				maxY = -Infinity,
				minZ = Infinity,
				maxZ = -Infinity;
			for (let i = 0; i < v.length; i += 3) {
				if (v[i] > maxX) maxX = v[i];
				if (v[i + 1] < minY) minY = v[i + 1];
				if (v[i + 1] > maxY) maxY = v[i + 1];
				if (v[i + 2] < minZ) minZ = v[i + 2];
				if (v[i + 2] > maxZ) maxZ = v[i + 2];
			}
			return window.__waffle.projectWorldPoint([
				maxX,
				minY + (maxY - minY) * 0.45,
				minZ + (maxZ - minZ) * 0.5
			]);
		}, probeInfo.chromeBodyId);

		await page.evaluate(() =>
			window.__waffle.updateSettings({
				viewportShading: 'studio',
				studioSSR: false,
				studioAO: true
			})
		);
		await waitForStudioSettled(page);
		const [noSsrPx] = await sampleCanvasPoints(page, [innerWallPt], 3);

		await page.evaluate(() => window.__waffle.updateSettings({ studioSSR: true }));
		await waitForStudioSettled(page);
		const [withSsrPx] = await sampleCanvasPoints(page, [innerWallPt], 3);

		const redExcessNoSsr = noSsrPx.r - noSsrPx.b;
		const redExcessWithSsr = withSsrPx.r - withSsrPx.b;
		expect(
			redExcessWithSsr,
			`SSR should reflect Signal Red neighbor onto Chrome wall (noSSR R-B=${redExcessNoSsr.toFixed(2)}, withSSR R-B=${redExcessWithSsr.toFixed(2)})`
		).toBeGreaterThan(redExcessNoSsr + 2.0);

		expectNoAnyCrash(crashes);
	});
});

test.describe('Studio PBR adversarial & cross-mode invariants', () => {
	test('handles empty scene, invalid material refusals, rig cycling, context-menu toggle, sketch transparency, section cap, and assembly instances cleanly', async ({
		waffle
	}) => {
		const page = waffle.page;
		const crashes = collectCrashErrors(page);

		// 1. Empty scene in Studio mode settles cleanly with 0 meshes
		await page.evaluate(() => window.__waffle.updateSettings({ viewportShading: 'studio' }));
		await waitForStudioSettled(page);

		// 2. Viewport right-click context menu toggles Studio -> Shaded -> Studio
		const canvasBox = await page.locator('[data-testid="viewport"] canvas').boundingBox();
		await page.mouse.click(
			Math.round(canvasBox.x + canvasBox.width * 0.5),
			Math.round(canvasBox.y + canvasBox.height * 0.5),
			{ button: 'right' }
		);
		const ctxBtn = page.locator('[data-testid="ctx-studio-shading"]');
		await expect(ctxBtn).toBeVisible();
		await ctxBtn.click();
		expect((await page.evaluate(() => window.__waffle.getSettings())).viewportShading).toBe('shaded');

		await page.mouse.click(
			Math.round(canvasBox.x + canvasBox.width * 0.5),
			Math.round(canvasBox.y + canvasBox.height * 0.5),
			{ button: 'right' }
		);
		await expect(ctxBtn).toBeVisible();
		await ctxBtn.click();
		await waitForStudioSettled(page);

		// 3. Pathological / invalid material inputs are refused loudly without corrupting table
		const badNegDensity = await page.evaluate(() =>
			window.__waffle.upsertMaterial({
				name: 'AntiMatter',
				density_kg_m3: -100,
				appearance: { color: [0.5, 0.5, 0.5], metalness: 0.5, roughness: 0.5 }
			})
		);
		expect(badNegDensity).toBe(false);

		const badColor = await page.evaluate(() =>
			window.__waffle.upsertMaterial({
				name: 'HyperColor',
				density_kg_m3: 2700,
				appearance: { color: [2.5, -0.1, 0.5], metalness: 0.5, roughness: 0.5 }
			})
		);
		expect(badColor).toBe(false);
		expect((await page.evaluate(() => window.__waffle.getMaterials())).length).toBe(0);

		// 4. Build a body, assign Gold (24k), cycle all 3 procedural HDR rigs
		await createCleanBlock(waffle, '25');
		await page.evaluate(async () => {
			const gold = window.__waffle.getMaterialPresets().find((p) => p.name === 'Gold (24k)');
			await window.__waffle.upsertMaterial({
				name: gold.name,
				density_kg_m3: gold.density_kg_m3,
				appearance: gold.appearance
			});
			await window.__waffle.setBodyMaterial(window.__waffle.getMeshes()[0].bodyId, gold.name);
		});
		for (const rig of ['studio-softbox', 'daylight-workshop', 'darkroom-rim']) {
			await page.evaluate((r) => window.__waffle.updateSettings({ studioEnvironment: r }), rig);
			await waitForStudioSettled(page);
		}
		const [goldPx] = await sampleCanvasPoints(page, [{ x: 0.5, y: 0.5 }], 3);
		expect(goldPx.r).toBeGreaterThan(goldPx.b + 15);

		// 5. Sketch mode transparency & Capped Section View in Studio mode
		await clickSketch(page, 'front');
		await waitForStudioSettled(page);
		await clickFinishSketch(page);
		await waitForStudioSettled(page);

		await page.locator('[data-testid="origin-plane-front"]').click();
		await clickToolbarAction(page, 'section');
		await waitForStudioSettled(page);
		await clickToolbarAction(page, 'section-clear');
		await waitForStudioSettled(page);

		// 6. Assembly tab projects leaf Part's Gold (24k) appearance onto placed instances
		const asmMeta = await page.evaluate(async () => {
			const partTabId = window.__waffle.getDocumentTabs()[0].id;
			await window.__waffle.addTab('Assembly');
			await window.__waffle.addInstance({ tabId: partTabId, name: 'Gold Block' });
			return window.__waffle.getMeshes().map((m) => ({
				material: m.material,
				appearance: m.appearance
			}));
		});
		expect(asmMeta).toHaveLength(1);
		expect(asmMeta[0].material).toBe('Gold (24k)');
		expect(asmMeta[0].appearance?.metalness).toBe(1.0);
		await waitForStudioSettled(page);

		expectNoAnyCrash(crashes);
	});

	test('Real 1K HDRI presets load Z-up (sky +Z vs ground -Z), rotate, and render backdrop in Ortho & Perspective', async ({
		waffle
	}) => {
		const page = waffle.page;
		const crashes = collectCrashErrors(page);
		await createCleanBlock(waffle, '30');

		await page.evaluate(async () => {
			const chrome = window.__waffle
				.getMaterialPresets()
				.find((p) => p.name === 'Polished Chrome');
			await window.__waffle.upsertMaterial({
				name: chrome.name,
				density_kg_m3: chrome.density_kg_m3,
				appearance: chrome.appearance
			});
			await window.__waffle.setBodyMaterial(
				window.__waffle.getMeshes()[0].bodyId,
				chrome.name
			);
		});

		// Enable Studio via ViewCube dropdown and verify quick HDRI selector & backdrop toggle
		await page.locator('[data-testid="viewcube-overlay"] .dropdown-toggle').click();
		await page.locator('[data-testid="viewcube-studio-toggle"]').check();
		const envSelect = page.locator('[data-testid="viewcube-studio-env"]');
		await expect(envSelect).toBeVisible();
		await expect(page.locator('[data-testid="viewcube-studio-backdrop"]')).toBeVisible();

		// Cycle all 5 bundled CC0 1K HDRIs (outside, workbench, machine shop, office)
		for (const envId of [
			'hdri-outdoor-sunny',
			'hdri-outdoor-plaza',
			'hdri-workbench',
			'hdri-machine-shop',
			'hdri-office'
		]) {
			await envSelect.selectOption(envId);
			await waitForStudioSettled(page);
			const stats = await page.evaluate(() => window.__waffle.getStudioStats());
			expect(stats.environmentId).toBe(envId);
			expect(stats.environmentLoaded).toBe(true);
			const [px] = await sampleCanvasPoints(page, [{ x: 0.5, y: 0.5 }], 3);
			expect(px.lum, `${envId} should illuminate Polished Chrome`).toBeGreaterThan(40);
		}
		// Close ViewCube dropdown
		await page.locator('[data-testid="viewcube-overlay"] .dropdown-toggle').click();

		// Invariant I9: Z-up orientation on Outdoor Sunny Park — +Z top face reflects blue sky,
		// whereas -Z bottom face reflects darker earth/grass
		await page.evaluate(() => {
			window.__waffle.updateSettings({
				studioEnvironment: 'hdri-outdoor-sunny',
				studioGroundShadow: false,
				studioEnvRotation: 0
			});
			window.dispatchEvent(new CustomEvent('waffle-snap-view-and-fit', { detail: { view: 'top' } }));
		});
		await waitForStudioSettled(page);
		const [skyReflectPx] = await sampleCanvasPoints(page, [{ x: 0.5, y: 0.5 }], 4);

		await page.evaluate(() => {
			window.dispatchEvent(
				new CustomEvent('waffle-snap-view-and-fit', { detail: { view: 'bottom' } })
			);
		});
		await waitForStudioSettled(page);
		const [groundReflectPx] = await sampleCanvasPoints(page, [{ x: 0.5, y: 0.5 }], 4);

		expect(
			skyReflectPx.lum,
			`+Z top face should reflect bright sky (${skyReflectPx.lum.toFixed(1)}) vs -Z bottom ground (${groundReflectPx.lum.toFixed(1)})`
		).toBeGreaterThan(groundReflectPx.lum + 25);
		expect(skyReflectPx.b).toBeGreaterThan(groundReflectPx.b + 20);

		// Environment rotation around +Z shifts reflections in ISO view
		await page.evaluate(() => {
			window.dispatchEvent(new CustomEvent('waffle-snap-view-and-fit', { detail: { view: 'iso' } }));
			window.__waffle.updateSettings({
				studioEnvironment: 'hdri-office',
				studioEnvRotation: 0
			});
		});
		await waitForStudioSettled(page);
		const [rot0Px] = await sampleCanvasPoints(page, [{ x: 0.5, y: 0.5 }], 4);

		await page.evaluate(() => window.__waffle.updateSettings({ studioEnvRotation: 135 }));
		await waitForStudioSettled(page);
		const [rot135Px] = await sampleCanvasPoints(page, [{ x: 0.5, y: 0.5 }], 4);

		const rotDelta =
			Math.abs(rot0Px.r - rot135Px.r) +
			Math.abs(rot0Px.g - rot135Px.g) +
			Math.abs(rot0Px.b - rot135Px.b);
		expect(rotDelta, 'Rotating HDRI around +Z should shift reflected radiance').toBeGreaterThan(4);

		// Environment Backdrop: off => exact CSS --viewport-bg parity with Classic Shaded (Invariant I6);
		// on => visible Z-up sky/horizon HDRI dome in BOTH Orthographic & Perspective cameras
		await page.evaluate(() => window.__waffle.updateSettings({ viewportShading: 'shaded' }));
		await page.waitForTimeout(150);
		const [shadedBgTop] = await sampleCanvasPoints(page, [{ x: 0.08, y: 0.08 }], 2);

		await page.evaluate(() =>
			window.__waffle.updateSettings({
				viewportShading: 'studio',
				studioEnvironment: 'hdri-outdoor-sunny',
				studioBackdrop: false
			})
		);
		await waitForStudioSettled(page);
		const [bgOffTop] = await sampleCanvasPoints(page, [{ x: 0.08, y: 0.08 }], 2);
		expect(Math.abs(bgOffTop.r - shadedBgTop.r)).toBeLessThanOrEqual(1);
		expect(Math.abs(bgOffTop.g - shadedBgTop.g)).toBeLessThanOrEqual(1);
		expect(Math.abs(bgOffTop.b - shadedBgTop.b)).toBeLessThanOrEqual(1);

		for (const proj of ['orthographic', 'perspective']) {
			await page.evaluate((p) => {
				if (window.__waffle.getCameraProjection() !== p) {
					window.__waffle.toggleCameraProjection();
				}
				window.__waffle.updateSettings({ studioBackdrop: true });
			}, proj);
			await waitForStudioSettled(page);
			const [bgSky, bgLow] = await sampleCanvasPoints(
				page,
				[
					{ x: 0.08, y: 0.08 },
					{ x: 0.08, y: 0.92 }
				],
				2
			);
			const shiftFromCssBg =
				Math.abs(bgSky.r - shadedBgTop.r) +
				Math.abs(bgSky.g - shadedBgTop.g) +
				Math.abs(bgSky.b - shadedBgTop.b);
			expect(shiftFromCssBg, `Backdrop visible in ${proj}`).toBeGreaterThan(25);
			expect(
				Math.abs(bgSky.lum - bgLow.lum) + Math.abs(bgSky.b - bgLow.b),
				`Backdrop in ${proj} should vary vertically rather than stretching 1 pixel`
			).toBeGreaterThan(8);
		}

		// Custom .hdr upload in Settings -> Appearance decodes and activates 'custom' environment
		await page.locator('[data-testid="toolbar-btn-settings"]').click();
		await page.locator('[data-testid="settings-section-appearance"]').click();
		await page
			.locator('[data-testid="settings-modal"] input[type="file"][accept=".hdr,.exr"]')
			.setInputFiles('static/hdri/indoor_workbench.hdr');
		await page.locator('[data-testid="settings-close"]').click();
		await waitForStudioSettled(page);
		const customStats = await page.evaluate(() => window.__waffle.getStudioStats());
		expect(customStats.environmentId).toBe('custom');
		expect(customStats.environmentLoaded).toBe(true);

		expectNoAnyCrash(crashes);
	});
});
