/**
 * Ring profiles finish + extrude — regression for the step_extrude.waffle
 * failure class (task #139 follow-up).
 *
 * A closed line/arc ring whose entities are stored in CW walk order used to
 * break extrude twice over:
 *  - extractProfiles kept BOTH traversal twins of the ring, and
 *  - finishSketch's densifier always sampled the first entity forward
 *    (duplicating the shared vertex → kernel ProfileRepeatedVertex) and
 *    sampled reversed arcs CCW (the complement arc → NewellMismatch).
 * Staging validates ALL profiles, so extruding even a clean profile failed.
 */
import { test, expect } from './helpers/waffle-test.js';
import { clickSketch, clickExtrude } from './helpers/toolbar.js';
import { clickAt, getCanvasBounds } from './helpers/canvas.js';
import { getEntities, waitForFeatureCount } from './helpers/state.js';

const setTool = (page, t) => page.evaluate((tool) => window.__waffle.setTool(tool), t);

/** Extrude via the dialog UI (default region → analytical profile path). */
async function extrudeViaDialog(page, depthMm) {
	await clickExtrude(page);
	const depth = page.locator('[data-testid="extrude-depth"]');
	if (await depth.isVisible()) await depth.fill(String(depthMm));
	await page.locator('[data-testid="extrude-apply"]').click();
	await expect(page.locator('[data-testid="extrude-dialog"]')).not.toBeVisible();
}

function collectFeatureErrors(page, sink) {
	page.on('console', (m) => {
		if (m.type() === 'error' && m.text().includes('failed:')) sink.push(m.text());
	});
}

/**
 * Test SETUP: author a rounded square wound CLOCKWISE — 4 lines + 4 corner
 * arcs, lines in walk order, arcs stored CCW start→end (entity convention),
 * i.e. endpoints swapped relative to the ring direction. Mirrors the Rust
 * fixture in crates/waffle-types/tests/profile_ring_twin_dedup.rs.
 */
async function addCwRoundedSquare(page) {
	await page.evaluate(() => {
		const r = 0.002;
		const h = 0.01;
		const add = (e) => window.__waffle.addSketchEntity(e);
		const pts = [
			[h - r, h], [-h + r, h], [-h, h - r], [-h, -h + r],
			[-h + r, -h], [h - r, -h], [h, -h + r], [h, h - r],
		];
		pts.forEach(([x, y], i) => add({ type: 'Point', id: i + 1, x, y, construction: false }));
		const centers = [
			[-h + r, h - r], [-h + r, -h + r], [h - r, -h + r], [h - r, h - r],
		];
		centers.forEach(([x, y], i) => add({ type: 'Point', id: i + 11, x, y, construction: false }));
		add({ type: 'Line', id: 21, start_id: 1, end_id: 2, construction: false });
		add({ type: 'Arc', id: 22, center_id: 11, start_id: 3, end_id: 2, construction: false });
		add({ type: 'Line', id: 23, start_id: 3, end_id: 4, construction: false });
		add({ type: 'Arc', id: 24, center_id: 12, start_id: 5, end_id: 4, construction: false });
		add({ type: 'Line', id: 25, start_id: 5, end_id: 6, construction: false });
		add({ type: 'Arc', id: 26, center_id: 13, start_id: 7, end_id: 6, construction: false });
		add({ type: 'Line', id: 27, start_id: 7, end_id: 8, construction: false });
		add({ type: 'Arc', id: 28, center_id: 14, start_id: 1, end_id: 8, construction: false });
	});
	await page.waitForTimeout(300);
}

test.describe('ring profile finish + extrude', () => {
	test('CW-wound line/arc ring: one profile, clean densification, extrude succeeds', async ({ waffle }) => {
		const page = waffle.page;
		const errors = [];
		collectFeatureErrors(page, errors);

		await clickSketch(page, 'front');
		await addCwRoundedSquare(page);

		// Twin dedup: exactly ONE profile for the ring.
		const profiles = await page.evaluate(() =>
			window.__waffle.getProfiles ? window.__waffle.getProfiles() : null
		);
		if (profiles) expect(profiles.length).toBe(1);

		await page.evaluate(() => window.__waffle.finishSketch());
		await waitForFeatureCount(page, 1, 10000);

		// The persisted densified profile has no consecutive duplicate
		// vertices (the exact condition kernel-v2 rejects).
		const dup = await page.evaluate(() => {
			const f = window.__waffle.getFeatureTree().features.find(
				(x) => x.operation?.type === 'Sketch'
			);
			const sk = f.operation.sketch;
			const pos = sk.solved_positions;
			let dups = 0;
			for (const p of sk.solved_profiles ?? []) {
				const v = p.vertex_ids ?? [];
				for (let i = 0; i < v.length; i++) {
					const a = pos[v[i]];
					const b = pos[v[(i + 1) % v.length]];
					if (a && b && a[0] === b[0] && a[1] === b[1]) dups++;
				}
			}
			return { dups, profiles: (sk.solved_profiles ?? []).length };
		});
		expect(dup.profiles).toBe(1);
		expect(dup.dups).toBe(0);

		// Extrude the ring — used to fail with ProfileRepeatedVertex /
		// NewellMismatch before the densifier + twin-dedup fixes.
		await extrudeViaDialog(page, 5);
		await waitForFeatureCount(page, 2, 10000);
		await page.waitForTimeout(1500);

		expect(errors).toEqual([]);
		const meshes = await page.evaluate(() =>
			window.__waffle.getMeshes().map((m) => m.triangleCount)
		);
		expect(meshes.length).toBe(1);
		expect(meshes[0]).toBeGreaterThan(0);
	});

	test('offset ring from a drawn rectangle finishes and extrudes cleanly', async ({ waffle }) => {
		const page = waffle.page;
		const errors = [];
		collectFeatureErrors(page, errors);

		await clickSketch(page, 'front');
		await setTool(page, 'rectangle');
		await clickAt(page, -60, -40);
		await clickAt(page, 60, 40);
		await page.waitForTimeout(300);

		// Offset outward 5 mm with the real tool (ring of 4 lines + 4 arcs).
		await setTool(page, 'offset');
		await clickAt(page, 0, -40);
		const bounds = await getCanvasBounds(page);
		await page.mouse.move(bounds.centerX, bounds.centerY - 100, { steps: 3 });
		await page.waitForTimeout(100);
		await clickAt(page, 0, -100);
		const input = page.locator('.dimension-input');
		await expect(input).toBeVisible({ timeout: 3000 });
		await input.fill('5');
		await page.keyboard.press('Enter');
		await page.waitForTimeout(300);
		expect((await getEntities(page)).filter((e) => e.type === 'Arc').length).toBe(4);

		await page.evaluate(() => window.__waffle.finishSketch());
		await waitForFeatureCount(page, 1, 10000);

		// Extruding ANY profile stages them all — this is what failed on the
		// user's file even though the chosen profile was clean.
		await extrudeViaDialog(page, 5);
		await waitForFeatureCount(page, 2, 10000);
		await page.waitForTimeout(1500);

		expect(errors).toEqual([]);
		const meshes = await page.evaluate(() =>
			window.__waffle.getMeshes().map((m) => m.triangleCount)
		);
		expect(meshes.length).toBe(1);
		expect(meshes[0]).toBeGreaterThan(0);
	});
});
