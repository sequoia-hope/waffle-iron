/**
 * Project-then-offset — the "housing around an imported part" flow (SI3).
 *
 * projectFace previously skipped curved in-plane boundary edges, so a face
 * with a curved outline (any imported STEP part with rounded geometry)
 * projected nothing or a gappy loop. It now projects curved edges as static
 * construction polylines sharing the bound corner points, so the boundary is
 * ONE connected chain (invariant O4), which the offset tool then offsets
 * outward at an exact distance — real, extrudable geometry.
 * See /specs/sketch_chain_offset.md.
 */
import fs from 'fs';
import { test, expect } from './helpers/waffle-test.js';
import { waitForFeatureCount } from './helpers/state.js';

const CYLINDER_STEP = fs.readFileSync(
	new URL('./fixtures/cylinder.step', import.meta.url),
	'utf8'
);

const entities = (page) => page.evaluate(() => window.__waffle.getEntities());
const positions = (page) =>
	page.evaluate(() => Object.fromEntries(window.__waffle.getPositions()));
const setTool = (page, t) => page.evaluate((tool) => window.__waffle.setTool(tool), t);

/** Click at a sketch-plane 2D coordinate via the camera mapping. */
async function clickSketchPoint(page, x, y) {
	const px = await page.evaluate(
		([sx, sy]) => window.__waffle.sketchPointToScreen(sx, sy),
		[x, y]
	);
	expect(px).toBeTruthy();
	await page.mouse.move(px.x, px.y, { steps: 3 });
	await page.waitForTimeout(100);
	await page.mouse.click(px.x, px.y);
	await page.waitForTimeout(150);
}

test('imported STEP face: curved boundary projects as one loop and offsets outward', async ({ waffle }) => {
	const page = waffle.page;

	// Import the committed cylinder fixture (test-setup entry; real file
	// pickers can't be driven from Playwright).
	const ok = await page.evaluate(
		(text) => window.__waffle.importStepFromText('cylinder.step', text),
		CYLINDER_STEP
	);
	expect(ok).toBe(true);
	await waitForFeatureCount(page, 1, 10000);
	const dialog = page.locator('[data-testid="import-step-dialog"]');
	await dialog.waitFor({ state: 'visible', timeout: 5000 });
	await page.locator('[data-testid="import-apply"]').click();
	await dialog.waitFor({ state: 'hidden', timeout: 10000 });

	// Find the top circular cap (planar face, normal ≈ ±Z, highest along it).
	const cap = await page.evaluate(() => {
		const meshes = window.__waffle.getMeshes();
		let best = null;
		for (const mesh of meshes) {
			for (const r of mesh.faceRanges || []) {
				const plane = window.__waffle.computeFacePlane(r.geom_ref);
				if (!plane) continue;
				if (Math.abs(plane.normal[2]) > 0.99) {
					const z = plane.origin[2] * Math.sign(plane.normal[2]);
					if (!best || z > best.z) best = { ref: r.geom_ref, plane, z };
				}
			}
		}
		return best;
	});
	expect(cap).toBeTruthy();

	// Sketch on the cap and project the face (same projectFace the Proj
	// button runs). The boundary is a curved rim — the old straight-only
	// projectFace produced ZERO lines here.
	await page.evaluate(
		({ origin, normal }) => window.__waffle.enterSketch(origin, normal),
		{ origin: cap.plane.origin, normal: cap.plane.normal }
	);
	await page.waitForTimeout(300);
	const linesMade = await page.evaluate((ref) => window.__waffle.projectFace(ref), cap.ref);
	expect(linesMade).toBeGreaterThan(8);

	const ents = await entities(page);
	const projLines = ents.filter((e) => e.type === 'Line');
	expect(projLines.length).toBe(linesMade);
	expect(projLines.every((e) => e.construction)).toBe(true);

	// O4: one connected chain spanning the whole projected boundary.
	const chain = await page.evaluate(
		(seed) => window.__waffle.findConnectedChain(seed),
		projLines[0].id
	);
	expect(chain.length).toBe(linesMade);

	// Closed-loop offset works as a pure query at 0.5 mm.
	const query = await page.evaluate(
		(ids) => window.__waffle.computeChainOffset(ids, 0.0005),
		chain
	);
	expect(query.error).toBeUndefined();
	expect(query.closed).toBe(true);

	// Real-pointer flow: arm the offset on the projected rim, pull outward,
	// commit 0.5 mm — the 3D-printed-housing gesture.
	// "Outside" is relative to the LOOP's centroid — the sketch-plane origin
	// is just some point on the face and may sit on the rim itself.
	const pos = await positions(page);
	const loopPts = [];
	for (const l of projLines) {
		const a = pos[l.start_id];
		if (a) loopPts.push(a);
	}
	const cx = loopPts.reduce((s, p) => s + p.x, 0) / loopPts.length;
	const cy = loopPts.reduce((s, p) => s + p.y, 0) / loopPts.length;
	let rim = null; // segment midpoint farthest from the centroid
	for (const l of projLines) {
		const a = pos[l.start_id];
		const b = pos[l.end_id];
		if (!a || !b) continue;
		const m = { x: (a.x + b.x) / 2, y: (a.y + b.y) / 2 };
		if (!rim || Math.hypot(m.x - cx, m.y - cy) > Math.hypot(rim.x - cx, rim.y - cy)) rim = m;
	}
	const rimR = Math.hypot(rim.x - cx, rim.y - cy);
	const ux = (rim.x - cx) / rimR;
	const uy = (rim.y - cy) / rimR;

	// Select-first (branch 12): at import zoom the rim segments are only a
	// few px long, so a tool-first line click would land on the 8px-priority
	// endpoint Points. Seed from the selected chain via the real O shortcut;
	// tool-first pointer arming is covered in sketch-offset-tool.spec.js.
	await setTool(page, 'select');
	await page.evaluate((ids) => window.__waffle.setSketchSelection(ids), chain);
	await page.keyboard.press('o');
	await page.waitForTimeout(200);
	const armed = await page.evaluate(() => window.__waffle.getOffsetToolState());
	expect(armed.armed).toBe(true);
	await clickSketchPoint(page, rim.x + ux * rimR * 0.3, rim.y + uy * rimR * 0.3); // outside
	const input = page.locator('.dimension-input');
	await expect(input).toBeVisible({ timeout: 3000 });
	await input.fill('0.5');
	await page.keyboard.press('Enter');
	await page.waitForTimeout(300);

	const after = await entities(page);
	const newLines = after.filter(
		(e) => e.type === 'Line' && !ents.some((b) => b.id === e.id)
	);
	// Shallow polyline turns miter: one offset line per boundary line, all
	// REAL (extrudable) geometry.
	expect(newLines.length).toBe(linesMade);
	expect(newLines.every((e) => !e.construction)).toBe(true);

	// Every offset joint sits outside the source rim by ~0.5 mm (within
	// miter error), measured from the loop centroid.
	const srcR = Math.max(...loopPts.map((p) => Math.hypot(p.x - cx, p.y - cy)));
	const pos2 = await positions(page);
	for (const l of newLines) {
		for (const id of [l.start_id, l.end_id]) {
			const r = Math.hypot(pos2[id].x - cx, pos2[id].y - cy);
			expect(r).toBeGreaterThan(srcR + 0.0003);
			expect(r).toBeLessThan(srcR + 0.0008);
		}
	}
});
