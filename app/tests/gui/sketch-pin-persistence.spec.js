/**
 * Pin persistence across sketch finish/re-edit (specs/pinned_constraint.md
 * B6). FinishSketch used to filter ALL WhereDragged constraints out of the
 * persisted feature — origin/reference pins silently vanished on re-edit
 * (the user-supplied repro document had lost its center pin this way).
 *
 * Persistence contract: save lowers persistent (non-_isDrag) WhereDragged →
 * Rust Pinned{point,x,y}; re-edit upconverts Pinned → WhereDragged so the
 * single in-session format keeps badges/snaps/deletion working unchanged.
 */
import { test, expect } from './helpers/waffle-test.js';
import { clickSketch, clickSelect, clickCenterRectangle } from './helpers/toolbar.js';
import { drawRectangle } from './helpers/canvas.js';
import { waitForEntityCount, getEntities, collectCrashErrors, expectNoAnyCrash } from './helpers/state.js';

const getConstraints = (page) => page.evaluate(() => window.__waffle.getConstraints());
const getPositions = (page) =>
	page.evaluate(() => Object.fromEntries(window.__waffle.getPositions()));

test('origin pin survives finish + re-edit and still locks the center', async ({ waffle }) => {
	test.setTimeout(120000);
	const page = waffle.page;
	const crashes = collectCrashErrors(page);

	// Draw an origin-centered centerpoint rectangle — the snap pins the center.
	await clickSketch(page);
	await clickCenterRectangle(page);
	await drawRectangle(page, 0, 0, 80, 60);
	await waitForEntityCount(page, 11, 5000);

	const consBefore = await getConstraints(page);
	const pinBefore = consBefore.find((c) => c.type === 'WhereDragged' && !c._isDrag);
	expect(pinBefore, 'origin snap should emit a pin').toBeTruthy();
	const nConsBefore = consBefore.length;

	const entities = await getEntities(page);
	const points = entities.filter((e) => e.type === 'Point');
	const centerId = points[0].id;
	const cornerId = points[3].id;

	// Persist + re-open.
	await page.evaluate(() => window.__waffle.finishSketch());
	await page.waitForTimeout(1000);
	const featureId = await page.evaluate(() => {
		const feats = window.__waffle.getFeatureTree()?.features ?? [];
		return feats.find((f) => f.operation?.type === 'Sketch')?.id;
	});
	expect(featureId).toBeTruthy();

	// The persisted feature must carry the pin as Rust-format Pinned{x,y}.
	const storedPin = await page.evaluate((id) => {
		const feats = window.__waffle.getFeatureTree()?.features ?? [];
		const sk = feats.find((f) => f.id === id)?.operation?.sketch;
		return (sk?.constraints ?? []).find((c) => c.type === 'Pinned') ?? null;
	}, featureId);
	expect(storedPin, 'feature must persist the pin as Pinned').toBeTruthy();
	expect(storedPin.point).toBe(centerId);
	expect(Math.abs(storedPin.x)).toBeLessThan(1e-9);
	expect(Math.abs(storedPin.y)).toBeLessThan(1e-9);

	await page.evaluate((id) => window.__waffle.enterSketchEditMode(id), featureId);
	await page.waitForTimeout(1500);

	// In-session it must come back as WhereDragged (badges/snap logic format)…
	const consAfter = await getConstraints(page);
	const pinAfter = consAfter.find((c) => c.type === 'WhereDragged');
	expect(pinAfter, 're-edit must restore the pin').toBeTruthy();
	expect(pinAfter.point).toBe(centerId);
	expect(Math.abs(pinAfter.x)).toBeLessThan(1e-9);
	expect(Math.abs(pinAfter.y)).toBeLessThan(1e-9);
	expect(consAfter.length).toBe(nConsBefore);

	// …and it must still LOCK: drag the corner away, center stays at origin.
	await clickSelect(page);
	const c0 = (await getPositions(page))[cornerId];
	for (let i = 1; i <= 8; i++) {
		await page.evaluate(
			([id, x, y]) => window.__waffle.dragSketchPoint(id, x, y),
			[cornerId, c0.x * (1 + 0.2 * i), c0.y * (1 + 0.05 * i)]
		);
		await page.waitForTimeout(80);
	}
	await page.evaluate(() => window.__waffle.finalizeDrag());
	await page.waitForTimeout(400);

	const pos = await getPositions(page);
	const center = pos[centerId];
	expect(
		Math.hypot(center.x, center.y),
		'persisted pin must still hold the center at the origin'
	).toBeLessThan(Math.abs(c0.x) * 0.01);
	const corner = pos[cornerId];
	expect(Math.hypot(corner.x - c0.x, corner.y - c0.y)).toBeGreaterThan(Math.abs(c0.x) * 0.3);

	// Second round-trip must not duplicate or drop the pin.
	await page.evaluate(() => window.__waffle.finishSketch());
	await page.waitForTimeout(1000);
	const storedPins2 = await page.evaluate((id) => {
		const feats = window.__waffle.getFeatureTree()?.features ?? [];
		const sk = feats.find((f) => f.id === id)?.operation?.sketch;
		return (sk?.constraints ?? []).filter((c) => c.type === 'Pinned');
	}, featureId);
	expect(storedPins2.length).toBe(1);

	expectNoAnyCrash(crashes);
});

test('documents without pins load unchanged (backward compat)', async ({ waffle }) => {
	const page = waffle.page;
	const crashes = collectCrashErrors(page);
	// The pre-fix repro document has 17 constraints and no pin of any kind.
	const fs = await import('fs');
	const json = fs.readFileSync(
		new URL('./fixtures/two-center-rects-equal.waffle', import.meta.url),
		'utf8'
	);
	await page.evaluate(async (data) => {
		await window.__waffle.loadProject(data);
	}, json);
	await page.waitForTimeout(1000);
	const featureId = await page.evaluate(() => {
		const feats = window.__waffle.getFeatureTree()?.features ?? [];
		return feats.find((f) => f.operation?.type === 'Sketch')?.id;
	});
	await page.evaluate((id) => window.__waffle.enterSketchEditMode(id), featureId);
	await page.waitForTimeout(1500);
	const cons = await getConstraints(page);
	expect(cons.length).toBe(17);
	expect(cons.some((c) => c.type === 'WhereDragged' || c.type === 'Pinned')).toBe(false);
	expectNoAnyCrash(crashes);
});
