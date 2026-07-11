/**
 * Offset tool — chain offset at an exact distance.
 * Flow: click a chain (whole connected run) → move to pick side/magnitude →
 * click → exact-value popup → Enter commits real geometry.
 * See /specs/sketch_chain_offset.md (branch table 4-13, invariants O1-O5).
 *
 * Display unit is mm, internal is meters: typing "5" commits 0.005 m.
 */
import { test, expect } from './helpers/waffle-test.js';
import { clickSketch } from './helpers/toolbar.js';
import { clickAt, getCanvasBounds } from './helpers/canvas.js';
import { getEntities } from './helpers/state.js';

const setTool = (page, t) => page.evaluate((tool) => window.__waffle.setTool(tool), t);
const getPositions = (page) =>
	page.evaluate(() => Object.fromEntries(window.__waffle.getPositions()));
const OFFSET_M = 0.005; // "5" typed in the popup, display unit mm

/** Move the pointer to a canvas-center offset (drives the armed preview). */
async function moveAt(page, xOffset, yOffset) {
	const bounds = await getCanvasBounds(page);
	if (!bounds) throw new Error('Canvas not visible');
	await page.mouse.move(bounds.centerX + xOffset, bounds.centerY + yOffset, { steps: 3 });
	await page.waitForTimeout(100);
}

/** Arm on an entity, move to the offset side, click, type mm value, Enter. */
async function runOffset(page, armAt, sideAt, mmValue) {
	await setTool(page, 'offset');
	await clickAt(page, armAt[0], armAt[1]);
	await moveAt(page, sideAt[0], sideAt[1]);
	await clickAt(page, sideAt[0], sideAt[1]);
	const input = page.locator('.dimension-input');
	await expect(input).toBeVisible({ timeout: 3000 });
	await input.fill(String(mmValue));
	await page.keyboard.press('Enter');
	await page.waitForTimeout(300);
}

/** Bounding box of a set of point positions. */
function bbox(points) {
	const xs = points.map((p) => p.x);
	const ys = points.map((p) => p.y);
	return {
		minX: Math.min(...xs), maxX: Math.max(...xs),
		minY: Math.min(...ys), maxY: Math.max(...ys),
	};
}

async function drawCenteredRectangle(page) {
	await setTool(page, 'rectangle');
	await clickAt(page, -60, -40);
	await clickAt(page, 60, 40);
	await page.waitForTimeout(300);
}

test.describe('offset tool', () => {
	test.beforeEach(async ({ waffle }) => {
		await clickSketch(waffle.page);
	});

	test('activates via toolbar button and O shortcut', async ({ waffle }) => {
		const page = waffle.page;
		await page.locator('[data-testid="toolbar-btn-offset"]').click();
		await page.waitForFunction(
			() => window.__waffle?.getState()?.activeTool === 'offset',
			{ timeout: 3000 }
		);

		await setTool(page, 'select');
		await page.keyboard.press('o');
		await page.waitForFunction(
			() => window.__waffle?.getState()?.activeTool === 'offset',
			{ timeout: 3000 }
		);
		expect(await page.evaluate(() => window.__waffle.getState().activeTool)).toBe('offset');
	});

	test('closed rectangle offsets outward: 4 lines + 4 corner arcs at exact distance', async ({ waffle }) => {
		const page = waffle.page;
		await drawCenteredRectangle(page);

		const before = await getEntities(page);
		const rectLines = before.filter((e) => e.type === 'Line');
		expect(rectLines.length).toBe(4);
		const positionsBefore = await getPositions(page);
		const srcBox = bbox(
			before.filter((e) => e.type === 'Point').map((e) => positionsBefore[e.id])
		);

		// Arm on the top edge, offset toward the outside (above), commit 5 mm.
		await runOffset(page, [0, -40], [0, -100], 5);

		const after = await getEntities(page);
		const newLines = after.filter(
			(e) => e.type === 'Line' && !before.some((b) => b.id === e.id)
		);
		const newArcs = after.filter(
			(e) => e.type === 'Arc' && !before.some((b) => b.id === e.id)
		);
		expect(newLines.length).toBe(4);
		expect(newArcs.length).toBe(4);
		expect(newLines.every((e) => !e.construction)).toBe(true);

		// Offset lines sit exactly 5 mm outside the source rectangle: the
		// joint points' bbox grows by OFFSET_M on every side.
		const positions = await getPositions(page);
		const jointIds = new Set();
		for (const l of newLines) {
			jointIds.add(l.start_id);
			jointIds.add(l.end_id);
		}
		const outBox = bbox([...jointIds].map((id) => positions[id]));
		expect(outBox.minX).toBeCloseTo(srcBox.minX - OFFSET_M, 6);
		expect(outBox.maxX).toBeCloseTo(srcBox.maxX + OFFSET_M, 6);
		expect(outBox.minY).toBeCloseTo(srcBox.minY - OFFSET_M, 6);
		expect(outBox.maxY).toBeCloseTo(srcBox.maxY + OFFSET_M, 6);

		// Corner arcs have radius = the offset distance (true outside offset).
		for (const arc of newArcs) {
			const c = positions[arc.center_id];
			const s = positions[arc.start_id];
			expect(Math.hypot(s.x - c.x, s.y - c.y)).toBeCloseTo(OFFSET_M, 6);
		}
	});

	test('closed rectangle offsets inward: 4 trimmed lines, no arcs', async ({ waffle }) => {
		const page = waffle.page;
		await drawCenteredRectangle(page);
		const before = await getEntities(page);
		const positionsBefore = await getPositions(page);
		const srcBox = bbox(
			before.filter((e) => e.type === 'Point').map((e) => positionsBefore[e.id])
		);

		// Arm on the top edge, offset toward the inside (center), commit 1 mm
		// (the rectangle is only ~7.6 mm tall — 5 mm inward would cross the
		// middle and self-intersect, the documented v1 limit).
		await runOffset(page, [0, -40], [0, 0], 1);

		const after = await getEntities(page);
		const newLines = after.filter(
			(e) => e.type === 'Line' && !before.some((b) => b.id === e.id)
		);
		const newArcs = after.filter(
			(e) => e.type === 'Arc' && !before.some((b) => b.id === e.id)
		);
		expect(newLines.length).toBe(4);
		expect(newArcs.length).toBe(0);

		const positions = await getPositions(page);
		const jointIds = new Set();
		for (const l of newLines) {
			jointIds.add(l.start_id);
			jointIds.add(l.end_id);
		}
		const IN_M = 0.001;
		const inBox = bbox([...jointIds].map((id) => positions[id]));
		expect(inBox.minX).toBeCloseTo(srcBox.minX + IN_M, 6);
		expect(inBox.maxX).toBeCloseTo(srcBox.maxX - IN_M, 6);
		expect(inBox.minY).toBeCloseTo(srcBox.minY + IN_M, 6);
		expect(inBox.maxY).toBeCloseTo(srcBox.maxY - IN_M, 6);
	});

	test('circle offsets outward to a concentric circle at radius + d', async ({ waffle }) => {
		const page = waffle.page;
		await setTool(page, 'circle');
		await clickAt(page, 0, 0);
		await clickAt(page, 50, 0);
		await page.waitForTimeout(300);

		const before = await getEntities(page);
		const src = before.find((e) => e.type === 'Circle');

		// Arm on the rim, cursor outside the circle, commit 5 mm.
		await runOffset(page, [50, 0], [100, 0], 5);

		const after = await getEntities(page);
		const created = after.find(
			(e) => e.type === 'Circle' && !before.some((b) => b.id === e.id)
		);
		expect(created).toBeTruthy();
		expect(created.radius).toBeCloseTo(src.radius + OFFSET_M, 6);
		expect(created.construction).toBe(false);

		// Concentric with the source.
		const positions = await getPositions(page);
		const c0 = positions[src.center_id];
		const c1 = positions[created.center_id];
		expect(c1.x).toBeCloseTo(c0.x, 6);
		expect(c1.y).toBeCloseTo(c0.y, 6);
	});

	test('tangent line-arc chain (filleted corner) offsets welded: arc radius grows by d', async ({ waffle }) => {
		const page = waffle.page;

		// L-shape, then fillet the corner → line + arc + line tangent chain.
		// Long legs + a 10 mm radius keep the tangent points outside the
		// fillet tool's ~8 mm point-merge zone (hard-coded findOrCreatePoint
		// threshold) — smaller radii degenerate into a start==end arc.
		await setTool(page, 'line');
		await clickAt(page, -300, 0);
		await clickAt(page, 0, 0);
		await clickAt(page, 0, -300);
		await page.keyboard.press('Escape');
		await page.waitForTimeout(200);
		await setTool(page, 'sketch-fillet');
		await clickAt(page, 0, 0);
		const filletInput = page.locator('.dimension-input');
		await expect(filletInput).toBeVisible({ timeout: 3000 });
		await filletInput.fill('10');
		await page.keyboard.press('Enter');
		await page.waitForTimeout(300);

		const before = await getEntities(page);
		const srcArc = before.find((e) => e.type === 'Arc');
		expect(srcArc).toBeTruthy();
		const positionsBefore = await getPositions(page);
		const srcC = positionsBefore[srcArc.center_id];
		const srcS = positionsBefore[srcArc.start_id];
		const srcR = Math.hypot(srcS.x - srcC.x, srcS.y - srcC.y);

		// Arm on the horizontal leg, offset away from the corner (below/left
		// side of the L's outside), commit 5 mm.
		await runOffset(page, [-200, 0], [-200, 60], 5);

		const after = await getEntities(page);
		const newLines = after.filter(
			(e) => e.type === 'Line' && !before.some((b) => b.id === e.id)
		);
		const newArcs = after.filter(
			(e) => e.type === 'Arc' && !before.some((b) => b.id === e.id)
		);
		// Tangent joints weld — exactly the source topology, no corner arcs.
		expect(newLines.length).toBe(2);
		expect(newArcs.length).toBe(1);

		const positions = await getPositions(page);
		const c = positions[newArcs[0].center_id];
		const s = positions[newArcs[0].start_id];
		const newR = Math.hypot(s.x - c.x, s.y - c.y);
		// Offset on the convex (outside) side of the fillet grows the radius.
		expect(newR).toBeCloseTo(srcR + OFFSET_M, 6);
		// Same center: a parallel arc is concentric.
		expect(c.x).toBeCloseTo(srcC.x, 6);
		expect(c.y).toBeCloseTo(srcC.y, 6);
	});

	test('branching chain refuses to arm (typed error, no popup)', async ({ waffle }) => {
		const page = waffle.page;

		// A T-junction: three lines sharing one endpoint.
		await setTool(page, 'line');
		await clickAt(page, -80, 0);
		await clickAt(page, 0, 0);
		await clickAt(page, 80, 0);
		await page.keyboard.press('Escape');
		await setTool(page, 'select');
		await setTool(page, 'line');
		await clickAt(page, 0, 0);
		await clickAt(page, 0, -80);
		await page.keyboard.press('Escape');
		await page.waitForTimeout(300);

		const entities = await getEntities(page);
		const lineIds = entities.filter((e) => e.type === 'Line').map((e) => e.id);
		expect(lineIds.length).toBe(3);

		// Pure query reports the branch.
		const result = await page.evaluate(
			(ids) => window.__waffle.computeChainOffset(ids, 0.005),
			lineIds
		);
		expect(result.error).toBe('branching');

		// Tool click on a branch member does not arm: the follow-up click
		// opens no popup and creates nothing.
		await setTool(page, 'offset');
		await clickAt(page, -40, 0);
		await moveAt(page, -40, 40);
		await clickAt(page, -40, 40);
		await page.waitForTimeout(300);
		expect(await page.evaluate(() => window.__waffle.getDimensionPopup() != null)).toBe(false);
		const after = await getEntities(page);
		expect(after.length).toBe(entities.length);
	});

	test('select-first: chain select + O seeds the offset tool', async ({ waffle }) => {
		const page = waffle.page;
		await drawCenteredRectangle(page);
		const before = await getEntities(page);

		// Chain-select via double-click, then press O.
		await setTool(page, 'select');
		await clickAt(page, 0, -40);
		await clickAt(page, 0, -40);
		await page.keyboard.press('o');
		await page.waitForFunction(
			() => window.__waffle?.getState()?.activeTool === 'offset',
			{ timeout: 3000 }
		);

		// Already armed: move + click goes straight to the popup.
		await moveAt(page, 0, -100);
		await clickAt(page, 0, -100);
		const input = page.locator('.dimension-input');
		await expect(input).toBeVisible({ timeout: 3000 });
		await input.fill('5');
		await page.keyboard.press('Enter');
		await page.waitForTimeout(300);

		const after = await getEntities(page);
		const newLines = after.filter(
			(e) => e.type === 'Line' && !before.some((b) => b.id === e.id)
		);
		expect(newLines.length).toBe(4);
	});

	test('Escape disarms without creating geometry', async ({ waffle }) => {
		const page = waffle.page;
		await drawCenteredRectangle(page);
		const before = await getEntities(page);

		await setTool(page, 'offset');
		await clickAt(page, 0, -40); // arm
		await page.keyboard.press('Escape');
		await page.waitForTimeout(200);
		expect(await page.evaluate(() => window.__waffle.getState().activeTool)).toBe('select');

		const after = await getEntities(page);
		expect(after.length).toBe(before.length);
	});
});
