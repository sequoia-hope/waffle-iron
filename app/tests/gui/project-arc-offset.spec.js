/**
 * True-arc projection from native curved body edges (task #141, user case
 * step_extrude.waffle round 2).
 *
 * A rounded-square plate's cap boundary used to project as straight CHORDS
 * (kernel edge export was endpoint-only), so the offset lost the corner
 * radii. Circular edges now export sampled polylines + analytic descriptors,
 * and projectFace mints TRUE construction Arcs — offsetting the projected
 * boundary keeps a real radius at EVERY corner.
 */
import { test, expect } from './helpers/waffle-test.js';
import { clickSketch, clickExtrude } from './helpers/toolbar.js';
import { waitForFeatureCount } from './helpers/state.js';

const entities = (page) => page.evaluate(() => window.__waffle.getEntities());
const positions = (page) =>
	page.evaluate(() => Object.fromEntries(window.__waffle.getPositions()));
const setTool = (page, t) => page.evaluate((tool) => window.__waffle.setTool(tool), t);

const CORNER_R = 0.002; // authored rounded-square corner radius (2 mm)
const OFFSET_M = 0.002; // typed "2" (mm) in the offset popup

/**
 * Author the rounded square (4 lines + 4 corner arcs) — test SETUP.
 * The ring 1→2→…→8 winds CCW, and each corner arc walked ring-forward goes
 * CCW around its center, so arc entities (CCW start→end by convention)
 * follow the walk. Swapped endpoints would describe the 270° COMPLEMENT
 * lobe — a legal but self-intersecting profile.
 */
async function addRoundedSquare(page) {
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
		add({ type: 'Arc', id: 22, center_id: 11, start_id: 2, end_id: 3, construction: false });
		add({ type: 'Line', id: 23, start_id: 3, end_id: 4, construction: false });
		add({ type: 'Arc', id: 24, center_id: 12, start_id: 4, end_id: 5, construction: false });
		add({ type: 'Line', id: 25, start_id: 5, end_id: 6, construction: false });
		add({ type: 'Arc', id: 26, center_id: 13, start_id: 6, end_id: 7, construction: false });
		add({ type: 'Line', id: 27, start_id: 7, end_id: 8, construction: false });
		add({ type: 'Arc', id: 28, center_id: 14, start_id: 8, end_id: 1, construction: false });
	});
	await page.waitForTimeout(300);
}

test('rounded plate: face projects TRUE arcs; offset keeps a radius at every corner', async ({ waffle }) => {
	const page = waffle.page;

	// Rounded-square plate.
	await clickSketch(page, 'front');
	await addRoundedSquare(page);
	await page.evaluate(() => window.__waffle.finishSketch());
	await waitForFeatureCount(page, 1, 10000);
	await clickExtrude(page);
	const depth = page.locator('[data-testid="extrude-depth"]');
	if (await depth.isVisible()) await depth.fill('5');
	await page.locator('[data-testid="extrude-apply"]').click();
	await expect(page.locator('[data-testid="extrude-dialog"]')).not.toBeVisible();
	await waitForFeatureCount(page, 2, 10000);

	// The body's arc edges now carry analytic descriptors + sampled polylines.
	const arcRanges = await page.evaluate(() => {
		const m = window.__waffle.getMeshes()[0];
		return m.edgeRanges
			.filter((r) => r.curve?.kind === 'arc')
			.map((r) => ({ n: r.end_index - r.start_index, radius: r.curve.radius }));
	});
	expect(arcRanges.length).toBeGreaterThanOrEqual(8); // 4 corners × 2 caps
	for (const r of arcRanges) {
		expect(r.n).toBeGreaterThan(3); // sampled polyline, not a chord
		expect(r.radius).toBeCloseTo(CORNER_R, 9);
	}

	// Sketch on a cap and project the face.
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
	await page.evaluate(
		({ origin, normal }) => window.__waffle.enterSketch(origin, normal),
		{ origin: cap.plane.origin, normal: cap.plane.normal }
	);
	await page.waitForTimeout(300);
	await page.evaluate((ref) => window.__waffle.projectFace(ref), cap.ref);
	await page.waitForTimeout(300);

	// TRUE arcs, not chord lines: 4 construction Arcs + 4 construction
	// Lines, one connected chain — and every arc's sketch sweep is the
	// true 90° corner (not a complement lobe).
	const ents = await entities(page);
	const projArcs = ents.filter((e) => e.type === 'Arc' && e.construction);
	const projLines = ents.filter((e) => e.type === 'Line' && e.construction);
	expect(projArcs.length).toBe(4);
	expect(projLines.length).toBe(4);

	const sweeps = await page.evaluate(() => {
		const pos2 = Object.fromEntries(window.__waffle.getPositions());
		return window.__waffle.getEntities()
			.filter((e) => e.type === 'Arc' && e.construction)
			.map((e) => {
				const c = pos2[e.center_id];
				const a0 = Math.atan2(pos2[e.start_id].y - c.y, pos2[e.start_id].x - c.x);
				const a1 = Math.atan2(pos2[e.end_id].y - c.y, pos2[e.end_id].x - c.x);
				let sw = a1 - a0;
				while (sw <= 0) sw += 2 * Math.PI;
				return (sw * 180) / Math.PI;
			});
	});
	for (const sw of sweeps) expect(sw).toBeCloseTo(90, 1);

	const chain = await page.evaluate(
		(seed) => window.__waffle.findConnectedChain(seed),
		projLines[0].id
	);
	expect(chain.length).toBe(8);


	// Offset outward 2 mm: seed from the chain (select-first O), pull outward
	// through a point beyond the top edge, commit.
	await setTool(page, 'select');
	await page.evaluate((ids) => window.__waffle.setSketchSelection(ids), chain);
	await page.keyboard.press('o');
	await page.waitForTimeout(200);
	expect((await page.evaluate(() => window.__waffle.getOffsetToolState())).armed).toBe(true);

	// "Outside" = beyond the chain's farthest point from its own centroid
	// (the cap plane's sketch origin is just some point on the face).
	const outPt = await page.evaluate((ids) => {
		const ents2 = window.__waffle.getEntities();
		const pos2 = Object.fromEntries(window.__waffle.getPositions());
		const pts = [];
		for (const id of ids) {
			const e = ents2.find((x) => x.id === id);
			for (const pid of [e.start_id, e.end_id]) {
				if (pos2[pid]) pts.push(pos2[pid]);
			}
		}
		const cx = pts.reduce((a, p) => a + p.x, 0) / pts.length;
		const cy = pts.reduce((a, p) => a + p.y, 0) / pts.length;
		let far = pts[0];
		for (const p2 of pts) {
			if (Math.hypot(p2.x - cx, p2.y - cy) > Math.hypot(far.x - cx, far.y - cy)) far = p2;
		}
		return { x: cx + (far.x - cx) * 1.3, y: cy + (far.y - cy) * 1.3 };
	}, chain);
	const outside = await page.evaluate(
		([x, y]) => window.__waffle.sketchPointToScreen(x, y),
		[outPt.x, outPt.y]
	);
	await page.mouse.move(outside.x, outside.y, { steps: 3 });
	await page.waitForTimeout(150);
	await page.mouse.click(outside.x, outside.y);
	const input = page.locator('.dimension-input');
	await expect(input).toBeVisible({ timeout: 3000 });
	await input.fill('2');
	await page.keyboard.press('Enter');
	await page.waitForTimeout(300);

	// EVERY corner keeps a radius: one REAL arc per projected arc piece
	// (tangent joints weld), one real line per projected line.
	const after = await entities(page);
	const newArcs = after.filter(
		(e) => e.type === 'Arc' && !e.construction && !ents.some((b) => b.id === e.id)
	);
	const newLines = after.filter(
		(e) => e.type === 'Line' && !e.construction && !ents.some((b) => b.id === e.id)
	);
	expect(newArcs.length).toBe(projArcs.length);
	expect(newLines.length).toBe(projLines.length);

	const pos = await positions(page);
	for (const arc of newArcs) {
		const c = pos[arc.center_id];
		const s = pos[arc.start_id];
		expect(Math.hypot(s.x - c.x, s.y - c.y)).toBeCloseTo(CORNER_R + OFFSET_M, 6);
	}
});
