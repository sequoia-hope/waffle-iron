/**
 * Dimensioning on the sheet (`specs/drawings_and_mbd.md` §8, D4d).
 *
 * The half no Rust test and no renderer test can reach: a drafter picks two
 * edges WITH REAL POINTER EVENTS and the document ends up carrying a dimension
 * anchored on the two entities that were under the cursor. Rust stops at the
 * measurement, `drawing-dimension-svg.spec.js` starts from a hand-written
 * layout, and `drawing-tab.spec.js` authors through the store door — only here
 * does a click become an annotation.
 *
 * On the DOM and on the DOCUMENT, never on pixels: what is asserted is which
 * pids the annotation names and what the sheet printed, not what the sheet
 * looked like.
 */
import { test, expect } from './helpers/waffle-test.js';
import { collectCrashErrors, expectNoAnyCrash } from './helpers/state.js';
import { clickTool } from './helpers/toolbar.js';
import {
	PLATE_D_MM,
	PLATE_W_MM,
	addView,
	anchorPids,
	anchorScreenPoints,
	authoredAnnotations,
	clickAt,
	crossWallPair,
	dimensionPair,
	plateAndDrawing,
	waitForAnnotationCount,
	wallPair
} from './helpers/drawing.js';

/**
 * Every test here drives the kernel (a projection, then a measurement), so
 * every test gets the strict crash oracle — `drawing-tab.spec.js`'s own
 * argument: a panic inside a projection does not fail a DOM assertion loudly,
 * the sheet simply renders without that view, and the test then fails with a
 * message about anchors rather than about the crash.
 */
let crashes = null;
test.beforeEach(({ waffle }) => {
	crashes = collectCrashErrors(waffle.page);
});

/** The dimension values the sheet PRINTS, as strings. */
async function printedValues(page) {
	return page.evaluate(() => {
		const host = document.querySelector('[data-testid="drawing-sheet"] svg.wi-sheet');
		return Array.from(host.querySelectorAll('text.wi-dim-value')).map((t) => t.textContent);
	});
}
test.afterEach(() => {
	const tracker = crashes;
	crashes = null;
	if (tracker) expectNoAnyCrash(tracker);
});

test.describe('Dimensioning on the sheet (D4d)', () => {
	test('the drawing tab offers the dimension tools, at any width', async ({ waffle }) => {
		const page = waffle.page;
		await waffle.waitForReady();
		await plateAndDrawing(page);
		// The drawing toolbar REPLACED the modelling one and kept its testid,
		// so every width helper still reaches it.
		await expect(page.getByTestId('toolbar')).toHaveAttribute('data-toolbar', 'drawing');
		await expect(page.getByTestId('toolbar-btn-extrude')).toHaveCount(0);
		for (const tool of ['select', 'dim-distance', 'dim-radius', 'dim-angle', 'note', 'datum']) {
			await clickTool(page, tool);
		}
		// The last tool clicked is the active mode, read off the toolbar rather
		// than out of the store: the button and the mode must agree.
		await expect(page.getByTestId('toolbar')).toHaveAttribute('data-sheet-mode', 'datum');
	});

	test('two clicks and a placement click dimension a wall pair, and the annotation names those pids', async ({
		waffle
	}) => {
		const page = waffle.page;
		await waffle.waitForReady();
		const { viewId } = await plateAndDrawing(page);
		const points = await anchorScreenPoints(page, viewId);
		expect(points, 'the view reports anchors with screen positions').toBeTruthy();
		const pair = wallPair(points);
		expect(pair, 'the top view offers two opposite walls').toBeTruthy();

		await clickTool(page, 'dim-distance');
		await expect(page.getByTestId('sheet-prompt')).toHaveText('pick 1 of 2');

		// Hovering the first wall says what it would bind to BEFORE the click —
		// which is the whole point of the hint: the user sees the anchor kind,
		// not a surprise afterwards.
		await page.mouse.move(pair[0].x, pair[0].y);
		await expect(page.getByTestId('sheet-hint')).toHaveText('edge');
		await expect(page.getByTestId('sheet-overlay')).toHaveAttribute('data-hover-kind', 'edge');

		await clickAt(page, pair[0].x, pair[0].y);
		await expect(page.getByTestId('sheet-prompt')).toHaveText('pick 2 of 2');
		await clickAt(page, pair[1].x, pair[1].y);
		await expect(page.getByTestId('sheet-prompt')).toHaveText('click to place');

		// The placement click: 10 mm of PAPER clear of the first wall, away
		// from the part.
		const place = { x: pair[0].x, y: pair[0].y - 10 * points.pxPerMm };
		await clickAt(page, place.x, place.y);
		await waitForAnnotationCount(page, viewId, 1);

		const annotations = await authoredAnnotations(page, viewId);
		expect(annotations).toHaveLength(1);
		const a = annotations[0];
		expect(a.type).toBe('Dimension');
		expect(a.kind?.type).toBe('Distance');
		// The pids the annotation NAMES are the pids of the two anchors that
		// were under the cursor. They cross as decimal STRINGS in both
		// directions (`Selector::Pid` is `#[serde(with = pid_str)]`), so this is
		// an exact comparison rather than one through an `f64`.
		const named = a.anchors.map((r) => r.selector?.pid);
		expect(named).toHaveLength(2);
		expect(new Set(named)).toEqual(new Set([pair[0].pid, pair[1].pid]));
		for (const r of a.anchors) expect(r.selector?.type).toBe('Pid');

		// And the number the sheet prints is the plate's own extent — the
		// engine measured it; nothing typed a value, and there is no door to
		// type one through.
		const printed = await page.evaluate(() => {
			const host = document.querySelector('[data-testid="drawing-sheet"] svg.wi-sheet');
			return Array.from(host.querySelectorAll('text.wi-dim-value')).map((t) => t.textContent);
		});
		expect(printed).toHaveLength(1);
		expect([PLATE_W_MM, PLATE_D_MM]).toContain(Math.round(Number(printed[0])));
	});

	test('a tie inside the pick radius is refused, and nothing is authored', async ({ waffle }) => {
		const page = waffle.page;
		await waffle.waitForReady();
		const { viewId } = await plateAndDrawing(page);
		// A 1:50 view draws the 40 × 25 mm plate 0.8 × 0.5 mm on PAPER, so the
		// whole part is inside one 2 mm pick radius and the two walls across
		// its short axis are exactly equidistant from its centre. That is a
		// genuine tie — the condition the refusal exists for — and reaching it
		// by SCALE rather than by a contrived fixture is also the point: the
		// radius is a paper quantity, so a small enough view makes any pair
		// ambiguous.
		await page.evaluate((id) => window.__waffle.editDrawingView(id, { scale: 0.02 }), viewId);
		await page.waitForFunction(
			(id) => {
				const sheets = window.__waffle?.getDrawingStatus()?.drawing?.sheets ?? [];
				return (sheets.flatMap((s) => s.views ?? []).find((v) => v.id === id)?.scale ?? 0) === 0.02;
			},
			viewId,
			{ timeout: 10000 }
		);
		const points = await anchorScreenPoints(page, viewId);
		const centre = points.centrePaper;
		// The part's centre in client pixels, via the same CTM the anchors used.
		const at = await page.evaluate(
			([id, c]) => {
				const svg = document
					.querySelector(`g.wi-sheet-view[data-view-id="${id}"]`)
					.querySelector('svg');
				const p = new DOMPoint(c[0], c[1]).matrixTransform(svg.getScreenCTM());
				return { x: p.x, y: p.y };
			},
			[viewId, centre]
		);

		await clickTool(page, 'dim-distance');
		await page.mouse.move(at.x, at.y);
		// The hint NAMES the candidates, so the refusal is actionable rather
		// than a dead click.
		await expect(page.getByTestId('sheet-hint')).toContainText('equally near');
		await clickAt(page, at.x, at.y);
		// Nothing was picked and nothing was authored: a dimension bound to the
		// wrong edge prints a plausible number for the wrong feature, which is
		// the silent wrong the whole spec exists to prevent.
		await expect(page.getByTestId('toolbar')).toHaveAttribute('data-sheet-picks', '0');
		expect(await authoredAnnotations(page, viewId)).toHaveLength(0);
	});

	test('a second anchor on a different view is refused — a dimension has one frame', async ({
		waffle
	}) => {
		const page = waffle.page;
		await waffle.waitForReady();
		const { partTab, viewId } = await plateAndDrawing(page);
		// The placement is EXPLICIT, and it has to be: the engine's auto-layout
		// steps a view clear of its PARENT, and a second NAMED view has no
		// parent — so both land on the sheet's centre, exactly on top of each
		// other, and every anchor of one is within the pick radius of an anchor
		// of the other. (Measured: hovering a wall of the overlapped pair gives
		// the tie refusal, which is the pick rule being right about a sheet
		// that is wrong. A default for an unparented second view is a D4a/D4b
		// layout gap, recorded as an open item.)
		const second = await addView(page, partTab, { view: 'Front', placementMm: [90, 80] });
		const top = await anchorScreenPoints(page, viewId);
		const front = await anchorScreenPoints(page, second);
		const a = wallPair(top)[0];
		const b = front.anchors.find((x) => x.shape === 'Line');
		expect(b, 'the front view offers an edge too').toBeTruthy();

		await clickTool(page, 'dim-distance');
		await clickAt(page, a.x, a.y);
		await expect(page.getByTestId('toolbar')).toHaveAttribute('data-sheet-picks', '1');
		await clickAt(page, b.x, b.y);
		await expect(page.getByTestId('sheet-hint')).toContainText('same view');
		// Still one pick, and no annotation anywhere: the second was refused,
		// not silently measured in whichever view's transform came first.
		await expect(page.getByTestId('toolbar')).toHaveAttribute('data-sheet-picks', '1');
		expect(await authoredAnnotations(page, viewId)).toHaveLength(0);
		expect(await authoredAnnotations(page, second)).toHaveLength(0);
	});

	test('Escape backs out one pick at a time, then leaves the tool', async ({ waffle }) => {
		const page = waffle.page;
		await waffle.waitForReady();
		const { viewId } = await plateAndDrawing(page);
		const pair = wallPair(await anchorScreenPoints(page, viewId));
		await clickTool(page, 'dim-distance');
		await clickAt(page, pair[0].x, pair[0].y);
		await expect(page.getByTestId('toolbar')).toHaveAttribute('data-sheet-picks', '1');
		await page.keyboard.press('Escape');
		await expect(page.getByTestId('toolbar')).toHaveAttribute('data-sheet-picks', '0');
		await expect(page.getByTestId('toolbar')).toHaveAttribute(
			'data-sheet-mode',
			'dimension-distance'
		);
		await page.keyboard.press('Escape');
		await expect(page.getByTestId('toolbar')).toHaveAttribute('data-sheet-mode', 'select');
	});

	test('dragging a dimension moves its placement, and one undo puts it back', async ({ waffle }) => {
		const page = waffle.page;
		await waffle.waitForReady();
		const { viewId } = await plateAndDrawing(page);
		const points = await anchorScreenPoints(page, viewId);
		const pair = wallPair(points);
		await clickTool(page, 'dim-distance');
		await clickAt(page, pair[0].x, pair[0].y);
		await clickAt(page, pair[1].x, pair[1].y);
		await clickAt(page, pair[0].x, pair[0].y - 10 * points.pxPerMm);
		await waitForAnnotationCount(page, viewId, 1);
		const before = (await authoredAnnotations(page, viewId))[0].placement ?? { dx: 0, dy: 0 };

		// Grab the dimension LINE itself, which is what a drafter reaches for.
		const line = await page.evaluate(() => {
			const el = document.querySelector(
				'[data-testid="drawing-sheet"] svg.wi-sheet line.wi-dim-dimension'
			);
			const r = el.getBoundingClientRect();
			return { x: r.x + r.width / 2, y: r.y + r.height / 2 };
		});
		await clickTool(page, 'select');
		await page.mouse.move(line.x, line.y);
		await page.mouse.down();
		// 8 paper mm further from the part, in two steps so a `pointermove`
		// actually fires between down and up.
		await page.mouse.move(line.x, line.y - 4 * points.pxPerMm);
		await page.mouse.move(line.x, line.y - 8 * points.pxPerMm);
		await page.mouse.up();
		await page.waitForFunction(
			([id, dy]) => {
				const sheets = window.__waffle?.getDrawingStatus()?.drawing?.sheets ?? [];
				const view = sheets.flatMap((s) => s.views ?? []).find((v) => v.id === id);
				const p = view?.annotations?.[0]?.placement;
				return !!p && Math.abs((p.dy ?? 0) - dy) > 1e-6;
			},
			[viewId, before.dy ?? 0],
			{ timeout: 20000 }
		);
		const moved = (await authoredAnnotations(page, viewId))[0].placement;
		// Dragging AWAY from the part along paper −y is +v in view space, so
		// the placement's `dy` grew by about the 8 mm of paper that was dragged,
		// divided by the view scale (a placement is view-space meters, §7).
		expect(moved.dy - (before.dy ?? 0)).toBeGreaterThan(0.004);
		expect(moved.dy - (before.dy ?? 0)).toBeLessThan(0.012);

		// ONE undo restores it. The move is a delete and an add through the
		// existing doors (there is no `EditAnnotation`), recorded as one
		// history entry for exactly this reason — and the drawing's history is
		// the page's, because `UiToEngine::Undo` pops the FEATURE engine's
		// stack and a drawing's edits are not on it.
		await page.keyboard.press('Control+z');
		await page.waitForFunction(
			([id, dy]) => {
				const sheets = window.__waffle?.getDrawingStatus()?.drawing?.sheets ?? [];
				const view = sheets.flatMap((s) => s.views ?? []).find((v) => v.id === id);
				if ((view?.annotations?.length ?? 0) !== 1) return false;
				return Math.abs((view.annotations[0].placement?.dy ?? 0) - dy) < 1e-9;
			},
			[viewId, before.dy ?? 0],
			{ timeout: 20000 }
		);
		// The dimension is still there, still anchored on the same two pids.
		const after = await authoredAnnotations(page, viewId);
		expect(after).toHaveLength(1);
		expect(new Set(after[0].anchors.map((r) => r.selector?.pid))).toEqual(
			new Set([pair[0].pid, pair[1].pid])
		);
	});

	test('Delete removes the selected dimension, and undo brings it back', async ({ waffle }) => {
		const page = waffle.page;
		await waffle.waitForReady();
		const { viewId } = await plateAndDrawing(page);
		const points = await anchorScreenPoints(page, viewId);
		const pair = wallPair(points);
		await clickTool(page, 'dim-distance');
		await clickAt(page, pair[0].x, pair[0].y);
		await clickAt(page, pair[1].x, pair[1].y);
		await clickAt(page, pair[0].x, pair[0].y - 10 * points.pxPerMm);
		await waitForAnnotationCount(page, viewId, 1);

		const line = await page.evaluate(() => {
			const el = document.querySelector(
				'[data-testid="drawing-sheet"] svg.wi-sheet line.wi-dim-dimension'
			);
			const r = el.getBoundingClientRect();
			return { x: r.x + r.width / 2, y: r.y + r.height / 2 };
		});
		await clickTool(page, 'select');
		await clickAt(page, line.x, line.y);
		await expect(page.getByTestId('sheet-overlay')).toHaveAttribute('data-selected', '0');
		await page.keyboard.press('Delete');
		await waitForAnnotationCount(page, viewId, 0);
		await page.keyboard.press('Control+z');
		await waitForAnnotationCount(page, viewId, 1);
		// Re-AUTHORED from the stored annotation, so it names the same pids.
		const back = await authoredAnnotations(page, viewId);
		expect(new Set(back[0].anchors.map((r) => r.selector?.pid))).toEqual(
			new Set([pair[0].pid, pair[1].pid])
		);
	});

	test('the panel shows the selected dimension and changes how it PRINTS, never its value', async ({
		waffle
	}) => {
		const page = waffle.page;
		await waffle.waitForReady();
		const { viewId } = await plateAndDrawing(page);
		const points = await anchorScreenPoints(page, viewId);
		const pair = wallPair(points);
		await clickTool(page, 'dim-distance');
		await clickAt(page, pair[0].x, pair[0].y);
		await clickAt(page, pair[1].x, pair[1].y);
		await clickAt(page, pair[0].x, pair[0].y - 10 * points.pxPerMm);
		await waitForAnnotationCount(page, viewId, 1);

		// Authoring selects what it just made, so the panel is already showing
		// it — the drafter's next act is usually to set its precision.
		await expect(page.getByTestId('dwg-annotation')).toBeVisible();
		await expect(page.getByTestId('dwg-annotation-value')).toContainText('measured');
		const printedBefore = await printedValues(page);

		await page.getByTestId('dwg-annotation-precision').fill('3');
		await page.getByTestId('dwg-annotation-precision').blur();
		await page.waitForFunction(
			(id) => {
				const sheets = window.__waffle?.getDrawingStatus()?.drawing?.sheets ?? [];
				const view = sheets.flatMap((s) => s.views ?? []).find((v) => v.id === id);
				return view?.annotations?.[0]?.precision === 3;
			},
			viewId,
			{ timeout: 20000 }
		);
		const printedAfter = await printedValues(page);
		// Three places where there were two — the SAME measured number, printed
		// differently. That is the only kind of change this panel can make.
		expect(printedAfter[0].split('.')[1]).toHaveLength(3);
		expect(Number(printedAfter[0])).toBeCloseTo(Number(printedBefore[0]), 2);
	});

	test('a redo deletes what the delete deleted, on a view with TWO dimensions', async ({
		waffle
	}) => {
		const page = waffle.page;
		await waffle.waitForReady();
		const { viewId } = await plateAndDrawing(page);
		const points = await anchorScreenPoints(page, viewId);
		const across = wallPair(points);
		const along = crossWallPair(points);
		expect(across, 'the top view offers an opposite wall pair').toBeTruthy();
		expect(along, 'the top view offers the other wall pair').toBeTruthy();

		await dimensionPair(page, viewId, points, across, { y: -10 });
		await waitForAnnotationCount(page, viewId, 1);
		await dimensionPair(page, viewId, points, along, { x: 12, y: 0 });
		await waitForAnnotationCount(page, viewId, 2);

		const both = await authoredAnnotations(page, viewId);
		const first = anchorPids(both[0]);
		const second = anchorPids(both[1]);
		expect(first, 'the two dimensions name different entities').not.toEqual(second);

		// Delete the FIRST, undo, redo. This is the property an inverse built
		// out of `DrawingEdit` cannot have: `AddAnnotation` only appends, so
		// the undo puts the restored dimension LAST, and a recorded forward
		// step that deletes "index 0" then deletes the OTHER dimension. It
		// was measured doing exactly that before the drawing's history moved
		// into the engine as whole-drawing snapshots
		// (`DocumentSession::drawing_histories`).
		await clickTool(page, 'select');
		await page.evaluate((v) => window.__waffle.setSheetSelection({ viewId: v, index: 0 }), viewId);
		await page.keyboard.press('Delete');
		await waitForAnnotationCount(page, viewId, 1);
		expect(anchorPids((await authoredAnnotations(page, viewId))[0])).toEqual(second);

		await page.keyboard.press('Control+z');
		await waitForAnnotationCount(page, viewId, 2);
		// The undo restores the ORDER too, not just the count: a snapshot has
		// no index in it, so there is nothing to shift.
		const back = await authoredAnnotations(page, viewId);
		expect(anchorPids(back[0])).toEqual(first);
		expect(anchorPids(back[1])).toEqual(second);

		await page.keyboard.press('Control+Shift+z');
		await waitForAnnotationCount(page, viewId, 1);
		const survivor = await authoredAnnotations(page, viewId);
		expect(
			anchorPids(survivor[0]),
			'the redo must delete the dimension the delete deleted'
		).toEqual(second);
	});

	test('one Ctrl+Z undoes a placement drag, which the engine takes as one step', async ({
		waffle
	}) => {
		const page = waffle.page;
		await waffle.waitForReady();
		const { viewId } = await plateAndDrawing(page);
		const points = await anchorScreenPoints(page, viewId);
		const across = wallPair(points);
		const along = crossWallPair(points);
		await dimensionPair(page, viewId, points, across, { y: -10 });
		await waitForAnnotationCount(page, viewId, 1);
		await dimensionPair(page, viewId, points, along, { x: 12, y: 0 });
		await waitForAnnotationCount(page, viewId, 2);
		const before = await authoredAnnotations(page, viewId);
		const pidsBefore = before.map(anchorPids).sort();
		const placedBefore = before.map((a) => a.placement?.dy ?? 0).sort();

		// The first dimension's line is the HORIZONTAL one (its two walls
		// share a `v`); the second's is vertical. Grab it by that, which is
		// how a drafter tells them apart as well.
		const line = await page.evaluate(() => {
			const els = Array.from(
				document.querySelectorAll(
					'[data-testid="drawing-sheet"] svg.wi-sheet line.wi-dim-dimension'
				)
			).map((el) => el.getBoundingClientRect());
			const r = els.find((b) => b.width > b.height);
			return r ? { x: r.x + r.width / 2, y: r.y + r.height / 2 } : null;
		});
		expect(line, 'the first dimension drew a horizontal line to grab').toBeTruthy();

		// A move is a delete and an add — two edits, ONE `Batch`, so ONE
		// snapshot. Sent singly they would be two steps and the single Ctrl+Z
		// below would leave the dimension deleted.
		await clickTool(page, 'select');
		await page.mouse.move(line.x, line.y);
		await page.mouse.down();
		await page.mouse.move(line.x, line.y - 4 * points.pxPerMm);
		await page.mouse.move(line.x, line.y - 8 * points.pxPerMm);
		await page.mouse.up();
		await page.waitForFunction(
			([id, was]) => {
				const sheets = window.__waffle?.getDrawingStatus()?.drawing?.sheets ?? [];
				const view = sheets.flatMap((s) => s.views ?? []).find((v) => v.id === id);
				if ((view?.annotations?.length ?? 0) !== 2) return false;
				const now = view.annotations.map((a) => a.placement?.dy ?? 0).sort();
				return now.some((d, i) => Math.abs(d - was[i]) > 1e-6);
			},
			[viewId, placedBefore],
			{ timeout: 20000 }
		);

		await page.keyboard.press('Control+z');
		await page.waitForFunction(
			([id, was]) => {
				const sheets = window.__waffle?.getDrawingStatus()?.drawing?.sheets ?? [];
				const view = sheets.flatMap((s) => s.views ?? []).find((v) => v.id === id);
				if ((view?.annotations?.length ?? 0) !== 2) return false;
				const now = view.annotations.map((a) => a.placement?.dy ?? 0).sort();
				return now.every((d, i) => Math.abs(d - was[i]) < 1e-9);
			},
			[viewId, placedBefore],
			{ timeout: 20000 }
		);
		// BOTH dimensions still there, naming the same entities — the move's
		// delete and add did not survive as half a step.
		const after = await authoredAnnotations(page, viewId);
		expect(after).toHaveLength(2);
		expect(after.map(anchorPids).sort()).toEqual(pidsBefore);
	});

	test("editing a dimension keeps D4c's expression, and a literal is refused", async ({
		waffle
	}) => {
		const page = waffle.page;
		await waffle.waitForReady();
		const { viewId } = await plateAndDrawing(page);
		const points = await anchorScreenPoints(page, viewId);
		const pair = wallPair(points);

		// An EXPRESSION dimension, which is what D4c made authorable. Nothing
		// in the sheet's tools authors one (there is no door for a number and
		// none for an expression either), so it is created through the store
		// door, the way an agent's `drawing_annotation_add` would.
		await page.evaluate(
			([v, a, b]) =>
				window.__waffle.addDrawingAnnotation(v, {
					kind: 'Distance',
					anchors: [a, b],
					expr: '2 * 3 mm'
				}),
			[viewId, pair[0].pid, pair[1].pid]
		);
		await waitForAnnotationCount(page, viewId, 1);
		const authored = (await authoredAnnotations(page, viewId))[0];
		expect(authored.value?.type, 'the annotation reads an expression').toBe('Expr');
		expect(authored.value?.expr).toBe('2 * 3 mm');

		// Every edit to an existing annotation is a delete and a re-add, so a
		// field the re-author does not carry is a field the edit DROPS. An
		// expression dropped here would leave the same label printing the
		// part's own width instead of the expression's value: a different
		// number on the same drawing, silently.
		await page.evaluate(
			(v) => window.__waffle.setSheetSelection({ viewId: v, index: 0 }),
			viewId
		);
		await expect(page.getByTestId('dwg-annotation')).toBeVisible();
		await page.getByTestId('dwg-annotation-precision').fill('3');
		await page.getByTestId('dwg-annotation-precision').blur();
		await page.waitForFunction(
			(id) => {
				const sheets = window.__waffle?.getDrawingStatus()?.drawing?.sheets ?? [];
				const view = sheets.flatMap((s) => s.views ?? []).find((v) => v.id === id);
				return view?.annotations?.[0]?.precision === 3;
			},
			viewId,
			{ timeout: 20000 }
		);
		const edited = (await authoredAnnotations(page, viewId))[0];
		expect(edited.value?.type, 'the expression survived the re-author').toBe('Expr');
		expect(edited.value?.expr).toBe('2 * 3 mm');
		expect(edited.precision).toBe(3);
	});

	test('Delete and Escape do nothing while a panel text field has the focus', async ({
		waffle
	}) => {
		const page = waffle.page;
		await waffle.waitForReady();
		const { viewId } = await plateAndDrawing(page);
		const points = await anchorScreenPoints(page, viewId);
		const pair = wallPair(points);
		await dimensionPair(page, viewId, points, pair, { y: -10 });
		await waitForAnnotationCount(page, viewId, 1);
		// Authoring selects what it made, so the panel is showing it and
		// Delete is armed.
		await expect(page.getByTestId('dwg-annotation')).toBeVisible();

		// The shortcuts are on a WINDOW keydown, so a field that happens to
		// contain the text "3" would have its Delete key eat the dimension
		// instead of a character. The handler's own guard is what stops that;
		// this is the oracle for it.
		const field = page.getByTestId('dwg-annotation-precision');
		await field.focus();
		await page.keyboard.press('Delete');
		await page.keyboard.press('Backspace');
		await page.keyboard.press('Escape');
		expect(
			await authoredAnnotations(page, viewId),
			'the dimension survived a Delete typed into a panel field'
		).toHaveLength(1);

		// And with the focus back on the page, the same key does delete it —
		// so the test above is a guard working, not a shortcut that never ran.
		await page.getByTestId('drawing-sheet').click({ position: { x: 4, y: 4 } });
		await page.evaluate((v) => window.__waffle.setSheetSelection({ viewId: v, index: 0 }), viewId);
		await page.keyboard.press('Delete');
		await waitForAnnotationCount(page, viewId, 0);
	});

	test('the radius tool refuses a straight edge by name, before anything is authored', async ({
		waffle
	}) => {
		const page = waffle.page;
		await waffle.waitForReady();
		const { viewId } = await plateAndDrawing(page);
		const pair = wallPair(await anchorScreenPoints(page, viewId));
		await clickTool(page, 'dim-radius');
		await page.mouse.move(pair[0].x, pair[0].y);
		// The refusal is read under the cursor as an instruction, not reported
		// as a toast after the annotation was authored and rolled back.
		await expect(page.getByTestId('sheet-hint')).toContainText('circular rim');
		await clickAt(page, pair[0].x, pair[0].y);
		await expect(page.getByTestId('toolbar')).toHaveAttribute('data-sheet-picks', '0');
		expect(await authoredAnnotations(page, viewId)).toHaveLength(0);
	});
});
