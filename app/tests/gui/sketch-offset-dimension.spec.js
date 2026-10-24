/**
 * Offset tool — the committed offset is ONE parametric dimension.
 * See /specs/sketch_chain_offset.md §"Cycle 4".
 *
 * Contract:
 *  - the click that opens the value box FREEZES the ghost (moving the pointer
 *    while typing does not move it);
 *  - the typed value lands as an `Offset` constraint shown like any sketch
 *    dimension;
 *  - editing that one label moves every offset curve to the new distance,
 *    the corners still joined.
 *
 * Display unit is mm, internal is meters.
 */
import { test, expect } from './helpers/waffle-test.js';
import { clickSketch } from './helpers/toolbar.js';
import { clickAt, getCanvasBounds } from './helpers/canvas.js';
import { getEntities, collectCrashErrors, expectNoAnyCrash } from './helpers/state.js';

const setTool = (page, t) => page.evaluate((tool) => window.__waffle.setTool(tool), t);
const getPositions = (page) =>
	page.evaluate(() => Object.fromEntries(window.__waffle.getPositions()));
const getConstraints = (page) => page.evaluate(() => window.__waffle.getConstraints());
const getPreview = (page) =>
	page.evaluate(() => JSON.parse(JSON.stringify(window.__waffle.getPreview() ?? null)));

async function moveAt(page, xOffset, yOffset) {
	const bounds = await getCanvasBounds(page);
	if (!bounds) throw new Error('Canvas not visible');
	await page.mouse.move(bounds.centerX + xOffset, bounds.centerY + yOffset, { steps: 3 });
	await page.waitForTimeout(150);
}

function bbox(points) {
	const xs = points.map((p) => p.x);
	const ys = points.map((p) => p.y);
	return { minX: Math.min(...xs), maxX: Math.max(...xs), minY: Math.min(...ys), maxY: Math.max(...ys) };
}

/** Bounding box of the joints of the lines NOT in `beforeIds`. */
async function offsetLineBox(page, beforeIds) {
	const ents = await getEntities(page);
	const positions = await getPositions(page);
	const ids = new Set();
	for (const l of ents.filter((e) => e.type === 'Line' && !beforeIds.has(e.id))) {
		ids.add(l.start_id);
		ids.add(l.end_id);
	}
	return bbox([...ids].map((id) => positions[id]));
}

test.describe('offset dimension', () => {
	test.beforeEach(async ({ waffle }) => {
		await clickSketch(waffle.page);
	});

	test('the ghost freezes when the value box opens', async ({ waffle }) => {
		const page = waffle.page;
		await setTool(page, 'rectangle');
		await clickAt(page, -60, -40);
		await clickAt(page, 60, 40);
		await page.waitForTimeout(300);

		await setTool(page, 'offset');
		await clickAt(page, 0, -40); // arm on the top edge
		await moveAt(page, 0, -100);
		await clickAt(page, 0, -100); // freeze + open the box
		await expect(page.locator('.dimension-input')).toBeVisible({ timeout: 3000 });
		const frozen = await getPreview(page);
		expect(frozen?.type, 'an offset ghost is showing').toBe('offset-preview');

		// Wander far off: the ghost must not follow.
		await moveAt(page, 0, -200);
		await moveAt(page, 150, -250);
		expect(await getPreview(page), 'ghost unchanged while the box is open').toEqual(frozen);

		// Dismissed without a value, the ghost follows the cursor again.
		await page.locator('.dimension-input').press('Escape');
		await page.waitForTimeout(150);
	});

	test('the typed offset is one dimension that drives the whole chain', async ({ waffle }) => {
		const page = waffle.page;
		const crashes = collectCrashErrors(page);
		await setTool(page, 'rectangle');
		await clickAt(page, -60, -40);
		await clickAt(page, 60, 40);
		await page.waitForTimeout(300);

		const before = await getEntities(page);
		const beforeIds = new Set(before.map((e) => e.id));
		const posBefore = await getPositions(page);
		const srcBox = bbox(before.filter((e) => e.type === 'Point').map((e) => posBefore[e.id]));

		await setTool(page, 'offset');
		await clickAt(page, 0, -40);
		await moveAt(page, 0, -100);
		await clickAt(page, 0, -100);
		const input = page.locator('.dimension-input');
		await expect(input).toBeVisible({ timeout: 3000 });
		await input.fill('5');
		await page.keyboard.press('Enter');
		await page.waitForTimeout(400);

		// One Offset constraint carrying the typed distance.
		const offsets = (await getConstraints(page)).filter((c) => c.type === 'Offset');
		expect(offsets.length).toBe(1);
		expect(offsets[0].value).toBeCloseTo(0.005, 9);
		expect(offsets[0].links.length, '4 lines + 4 corner arcs').toBe(8);

		// Shown like any dimension.
		const label = page.locator('.dim-label');
		await expect(label).toHaveCount(1);
		await expect(label).toContainText('5');

		let box = await offsetLineBox(page, beforeIds);
		expect(box.minX).toBeCloseTo(srcBox.minX - 0.005, 6);
		expect(box.maxY).toBeCloseTo(srcBox.maxY + 0.005, 6);

		// Edit the one label: every offset line moves to 8 mm.
		await setTool(page, 'select');
		await label.click();
		const edit = page.locator('.dim-input');
		await expect(edit).toBeVisible({ timeout: 3000 });
		await edit.fill('8');
		await edit.press('Enter');
		await page.waitForFunction(
			() => window.__waffle.getConstraints().some((c) => c.type === 'Offset' && Math.abs(c.value - 0.008) < 1e-9),
			null,
			{ timeout: 5000 }
		);
		await page.waitForTimeout(500); // the solve lands after the value

		box = await offsetLineBox(page, beforeIds);
		expect(box.minX).toBeCloseTo(srcBox.minX - 0.008, 6);
		expect(box.maxX).toBeCloseTo(srcBox.maxX + 0.008, 6);
		expect(box.minY).toBeCloseTo(srcBox.minY - 0.008, 6);
		expect(box.maxY).toBeCloseTo(srcBox.maxY + 0.008, 6);

		// The corners are still joined: every corner arc has radius 8 mm and
		// shares its endpoints with the offset lines.
		const ents = await getEntities(page);
		const positions = await getPositions(page);
		const lineEnds = new Set(
			ents.filter((e) => e.type === 'Line' && !beforeIds.has(e.id)).flatMap((l) => [l.start_id, l.end_id])
		);
		const arcs = ents.filter((e) => e.type === 'Arc' && !beforeIds.has(e.id));
		expect(arcs.length).toBe(4);
		for (const a of arcs) {
			const c = positions[a.center_id];
			for (const id of [a.start_id, a.end_id]) {
				const p = positions[id];
				expect(Math.hypot(p.x - c.x, p.y - c.y)).toBeCloseTo(0.008, 6);
				expect(lineEnds.has(id), 'arc end shared with an offset line').toBe(true);
			}
		}

		// The source did not move.
		const srcAfter = bbox(before.filter((e) => e.type === 'Point').map((e) => positions[e.id]));
		expect(srcAfter.minX).toBeCloseTo(srcBox.minX, 9);
		expect(srcAfter.maxY).toBeCloseTo(srcBox.maxY, 9);

		expectNoAnyCrash(crashes);
	});
});
