/**
 * The Drawing tab (`specs/drawings_and_mbd.md` §8, increment D4a).
 *
 * Where `drawing-dimension-svg.spec.js` feeds the D3 renderer a hand-written
 * `ViewLayout`, this spec drives the WHOLE path: build a part, add a drawing
 * tab, add a view of the part, and assert on the SVG the sheet actually
 * renders — the curves the kernel projected and the number the engine
 * measured. It is the half neither the Rust tests nor the D3 spec can reach:
 * Rust stops at the layout record, D3 starts from one, and only here does the
 * engine's own projection reach the DOM.
 *
 * On the DOM, never on pixels, for the D3 reason: a screenshot baseline rots
 * on a font change and says nothing about whether the number is right.
 */
import { test, expect } from './helpers/waffle-test.js';
import { collectCrashErrors, expectNoAnyCrash } from './helpers/state.js';

/**
 * Every test in this file gets the strict crash oracle (CLAUDE.md, "WASM crash
 * detection"), because every test in it drives the kernel: a projection, and
 * since D4b a section cut and a detail crop.
 *
 * It is here rather than per test for the reason the rule exists. A panic
 * inside a section cut does not fail a DOM assertion loudly — the sheet simply
 * renders without that view, and a test asserting "the cap is one loop with one
 * hole" then fails with a message about loops, pointing at the hatch rather
 * than at the crash. `engineReady` is NOT usable as the oracle here: it is not
 * reliably reset on a crash, which is exactly what the rule says.
 */
let crashes = null;
test.beforeEach(({ waffle }) => {
	crashes = collectCrashErrors(waffle.page);
});
test.afterEach(() => {
	const tracker = crashes;
	crashes = null;
	if (tracker) expectNoAnyCrash(tracker);
});

/**
 * Call one agent tool through the page's own executor, the way
 * `agent-rust-tools.spec.js` does — no relay, the real routing.
 */
async function callTool(page, tool, args = {}) {
	return page.evaluate(
		({ tool, args }) =>
			window.__waffleAgentExecutor.executeTool(tool, args, {
				agentName: 'drawing-tab-test',
				isPaused: () => false,
				pause: () => {},
				isCancelled: () => false
			}),
		{ tool, args }
	);
}

/**
 * A 40 × 25 mm plate, 10 mm thick — the D3 harness's fixture.
 *
 * In METERS, because that is what `__waffle.addSketchEntity` and
 * `applyExtrude` take: the document's own unit, with the display conversion
 * at the UI boundary. Authoring `40` here built a forty-METRE plate and the
 * sheet printed `25000` mm, which is the dimension pipeline being right about
 * a fixture that was wrong.
 */
const PLATE_W = 0.04;
const PLATE_D = 0.025;
const PLATE_T = 0.01;

/** The same extents in millimetres, which is what the sheet prints. */
const PLATE_W_MM = 40;
const PLATE_D_MM = 25;

/**
 * The document's first Part tab.
 *
 * NOT `getDocumentState().activeTabId`, which is null until the store has
 * mirrored a document — on a fresh page the engine's session has a Part tab
 * and the store has not heard of it yet, so reading the id before any engine
 * round trip hands back nothing. Called after one.
 */
async function partTabId(page) {
	return page.evaluate(() => {
		const tabs = window.__waffle.getDocumentState().documentTabs ?? [];
		return (tabs.find((t) => t.kind === 'Part') ?? tabs[0])?.id ?? null;
	});
}

/**
 * Build the plate on the active Part tab, then a Drawing tab with a top view
 * of it. Returns `{ partTab, drawingTab, viewId }`.
 */
async function plateAndDrawing(page) {
	await page.evaluate(() => window.__waffle.enterSketch([0, 0, 0], [0, 0, 1]));
	await page.waitForFunction(() => window.__waffle?.getState()?.sketchMode?.active === true, null, {
		timeout: 5000
	});
	// Setup, not a drawing test: the sketch is a fixture (CLAUDE.md allows
	// `addSketchEntity` for exactly this).
	await page.evaluate(
		([w, d]) => {
			const waffle = window.__waffle;
			const corners = [
				[1, 0, 0],
				[2, w, 0],
				[3, w, d],
				[4, 0, d]
			];
			for (const [id, x, y] of corners) {
				waffle.addSketchEntity({ type: 'Point', id, x, y, construction: false });
			}
			for (const [id, a, b] of [
				[10, 1, 2],
				[11, 2, 3],
				[12, 3, 4],
				[13, 4, 1]
			]) {
				waffle.addSketchEntity({ type: 'Line', id, start_id: a, end_id: b, construction: false });
			}
		},
		[PLATE_W, PLATE_D]
	);
	await page.evaluate(() => window.__waffle.finishSketch());
	await page.waitForFunction(() => (window.__waffle?.getFeatureTree()?.features?.length ?? 0) >= 1, null, {
		timeout: 10000
	});
	await page.evaluate(() => window.__waffle.showExtrudeDialog());
	await page.evaluate((t) => window.__waffle.applyExtrude(t, 0, false), PLATE_T);
	await page.waitForFunction(() => (window.__waffle?.getMeshes() ?? []).some((m) => m.triangleCount > 0), null, {
		timeout: 20000
	});

	// After the extrude, so the store has mirrored the session's tab list.
	const partTab = await partTabId(page);
	expect(partTab, 'the document has a Part tab to draw').toBeTruthy();

	const drawingTab = await page.evaluate(() => window.__waffle.addTab('Drawing'));
	expect(drawingTab, 'the engine added a Drawing tab').toBeTruthy();
	await page.evaluate((id) => window.__waffle.switchTab(id), drawingTab);
	await page.waitForFunction(() => window.__waffle?.getDrawingStatus() !== null, null, { timeout: 15000 });

	const viewId = await page.evaluate(
		(tab) => window.__waffle.addDrawingView(tab, { view: 'Top' }),
		partTab
	);
	expect(viewId, 'the view was added').toBeTruthy();
	await page.waitForFunction(
		(id) => {
			const sheet = window.__waffle?.getDrawingStatus()?.drawing?.sheets?.[0];
			return (sheet?.views ?? []).some((v) => v.id === id && (v.cache?.curves?.length ?? 0) > 0);
		},
		viewId,
		{ timeout: 20000 }
	);
	return { partTab, drawingTab, viewId };
}

/** The rendered sheet's SVG, parsed, with the counts and texts asked for. */
async function sheetDom(page, query) {
	return page.evaluate((q) => {
		const host = document.querySelector('[data-testid="drawing-sheet"] svg.wi-sheet');
		if (!host) throw new Error('no sheet SVG is mounted');
		const sel = (s) => Array.from(host.querySelectorAll(s));
		return {
			views: Number(host.dataset.views),
			widthMm: host.getAttribute('width'),
			viewBox: host.getAttribute('viewBox'),
			counts: Object.fromEntries(Object.entries(q.counts ?? {}).map(([k, s]) => [k, sel(s).length])),
			texts: sel(q.texts ?? 'text').map((t) => t.textContent),
			attrs: Object.fromEntries(
				Object.entries(q.attrs ?? {}).map(([k, [s, a]]) => [k, sel(s).map((e) => e.getAttribute(a))])
			),
			// Every attribute in the sheet, for the NaN tripwire.
			allAttrs: sel('*').flatMap((e) => Array.from(e.attributes).map((a) => `${a.name}=${a.value}`))
		};
	}, query ?? {});
}

test.describe('Drawing tab', () => {
	test('a drawing tab replaces the sidebar and the viewport with the sheet', async ({ waffle }) => {
		const page = waffle.page;
		await waffle.waitForReady();
		// A Part tab shows the feature tree and the 3D view…
		await expect(page.locator('.left-panel .feature-tree')).toBeVisible();
		const drawingTab = await page.evaluate(() => window.__waffle.addTab('Drawing'));
		await page.evaluate((id) => window.__waffle.switchTab(id), drawingTab);
		// …and a Drawing tab shows neither: the drawing is what the tab is
		// for, and the viewport would show a different tab's part.
		await expect(page.getByTestId('drawing-panel')).toBeVisible();
		await expect(page.getByTestId('drawing-sheet')).toBeVisible();
		await expect(page.locator('.left-panel .feature-tree')).toHaveCount(0);
		await expect(page.locator('.viewport-area canvas')).toHaveCount(0);
		// The sheet is A3 landscape by default: one piece of paper, 420 mm.
		const sheet = page.locator('[data-testid="drawing-sheet"] svg.wi-sheet');
		await expect(sheet).toHaveAttribute('width', '420mm');
		await expect(sheet).toHaveAttribute('viewBox', '0 0 420 297');
	});

	test('a top view of a plate draws the four edges the kernel projected', async ({ waffle }) => {
		const page = waffle.page;
		await waffle.waitForReady();
		await plateAndDrawing(page);
		const dom = await sheetDom(page, {
			counts: {
				paper: 'rect.wi-paper',
				views: 'g.wi-sheet-view',
				curves: 'path.wi-curve',
				visible: 'path.wi-curve-visible'
			}
		});
		expect(dom.views).toBe(1);
		expect(dom.counts.views).toBe(1);
		// One piece of paper, not one per view: a rectangle behind each view
		// would read as a stack of cards rather than as a drawing.
		expect(dom.counts.paper).toBe(1);
		// Eight curves, all visible: the plate's four WALLS as lines, plus its
		// four vertical edges seen end-on, which the projection reports as
		// `Curve2::Point` and the renderer draws as a round dot (a zero-length
		// path with a butt cap would render nothing).
		//
		// No HIDDEN line, and that is D1c being right rather than missing
		// something: the far face's four edges are coincident in (u, v) with
		// the near ones, and the ray from them leaves through the near face's
		// own BOUNDARY — it grazes, and a face the ray grazes separates
		// nothing — so they are visible and the coincident-merge leaves one
		// line each. Four lines is also what a drafter draws.
		// `crates/wasm-bridge/tests/tool_drawing.rs` pins the same four with
		// the `ray_grazes_face` count that explains them.
		expect(dom.counts.curves).toBe(8);
		expect(dom.counts.visible).toBe(8);
		// And the four of them that are the outline are LINES, read off the
		// layout record the sheet drew from.
		const kinds = await page.evaluate(() => {
			const sheet = window.__waffle.getDrawingStatus().drawing.sheets[0];
			const counts = {};
			for (const c of sheet.views[0].cache.curves) {
				counts[c.geometry.type] = (counts[c.geometry.type] ?? 0) + 1;
			}
			return counts;
		});
		expect(kinds).toEqual({ Line: 4, Point: 4 });
		// Nothing non-finite reached the output.
		const bad = dom.allAttrs.filter((a) => /NaN|Infinity/.test(a));
		expect(bad, 'non-finite attributes in the sheet').toEqual([]);
	});

	test('a linear dimension on the sheet prints the width the part was built with', async ({ waffle }) => {
		const page = waffle.page;
		await waffle.waitForReady();
		const { viewId } = await plateAndDrawing(page);

		// The two walls across one view axis. Their persistent ids come from
		// the ENGINE's own anchor list (`drawingStatus.anchors`), which is
		// beside the layout for exactly this: the layout carries no model
		// reference at all, so a UI picking an edge to dimension reads the
		// anchors instead. Nothing here invents an id.
		const pair = await page.evaluate((id) => {
			const anchors = (window.__waffle.getDrawingStatus().anchors ?? {})[id] ?? [];
			// The two opposite WALLS: straight edges whose witness points share
			// a v and differ in u. `shape` is what makes this selectable — the
			// plate's top view offers eight anchors, four walls and four
			// corners, and picking by position alone takes two corners and
			// measures the diagonal (which is how this test found that the
			// anchor list needed a shape at all).
			const walls = anchors.filter((a) => a.shape?.type === 'Line' && Array.isArray(a.at));
			for (const a of walls) {
				for (const b of walls) {
					if (Math.abs(a.at[1] - b.at[1]) > 1e-12) continue;
					const span = Math.abs(b.at[0] - a.at[0]);
					if (span > 1e-9) {
						return { pids: [a.pid, b.pid], span_mm: span * 1000 };
					}
				}
			}
			return null;
		}, viewId);
		expect(pair, 'the view offers a wall pair to dimension').toBeTruthy();
		expect(pair.span_mm).toBeGreaterThan(1);

		await page.evaluate(
			([id, anchors]) => window.__waffle.addDrawingAnnotation(id, { kind: 'Distance', anchors }),
			[viewId, pair.pids]
		);
		await page.waitForFunction(
			(id) => {
				const sheet = window.__waffle?.getDrawingStatus()?.drawing?.sheets?.[0];
				const view = (sheet?.views ?? []).find((v) => v.id === id);
				return (view?.cache?.annotations?.length ?? 0) === 1;
			},
			viewId,
			{ timeout: 20000 }
		);

		const dom = await sheetDom(page, {
			texts: 'text.wi-dim-value',
			counts: { arrows: 'polygon.wi-dim-arrow', extensions: 'line.wi-dim-extension' },
			attrs: {
				x1: ['line.wi-dim-dimension', 'x1'],
				y1: ['line.wi-dim-dimension', 'y1'],
				x2: ['line.wi-dim-dimension', 'x2'],
				y2: ['line.wi-dim-dimension', 'y2']
			}
		});
		// The printed number is the plate's own extent, to the stated places.
		const printed = Number(dom.texts[0]);
		expect([PLATE_W_MM, PLATE_D_MM]).toContain(Math.round(printed));
		expect(printed).toBeCloseTo(pair.span_mm, 6);
		// Two arrowheads and two extension lines, as a linear dimension has.
		expect(dom.counts.arrows).toBe(2);
		expect(dom.counts.extensions).toBe(2);
		// And the dimension LINE that is drawn is as long as the number that
		// is printed — the cross-check that keeps the measurement and the
		// drawing honest about each other (the D3 spec's own assertion, now
		// over a number the kernel produced).
		const drawn = Math.hypot(
			Number(dom.attrs.x2[0]) - Number(dom.attrs.x1[0]),
			Number(dom.attrs.y2[0]) - Number(dom.attrs.y1[0])
		);
		expect(drawn).toBeCloseTo(printed, 3);
	});

	test('a view at 1:2 draws half the size and still prints the true dimension', async ({ waffle }) => {
		const page = waffle.page;
		await waffle.waitForReady();
		const { viewId } = await plateAndDrawing(page);
		const before = await sheetDom(page, { attrs: { w: ['g.wi-sheet-view > svg', 'width'] } });
		await page.evaluate((id) => window.__waffle.editDrawingView(id, { scale: 0.5 }), viewId);
		await page.waitForFunction(
			(id) => {
				const sheet = window.__waffle?.getDrawingStatus()?.drawing?.sheets?.[0];
				return (sheet?.views ?? []).some((v) => v.id === id && v.scale === 0.5);
			},
			viewId,
			{ timeout: 15000 }
		);
		const after = await sheetDom(page, { attrs: { w: ['g.wi-sheet-view > svg', 'width'] } });
		// The drawn view shrinks. Not by exactly a half: the margin round it
		// is a PEN quantity (a 0.5 mm line prints 0.5 mm at any scale, the D3
		// finding), so it does not scale with the drawing.
		expect(Number(after.attrs.w[0])).toBeLessThan(Number(before.attrs.w[0]));
	});

	test('the sheet says which view failed instead of looking finished without it', async ({ waffle }) => {
		const page = waffle.page;
		await waffle.waitForReady();
		const drawingTab = await page.evaluate(() => window.__waffle.addTab('Drawing'));
		await page.evaluate((id) => window.__waffle.switchTab(id), drawingTab);
		await page.waitForFunction(() => window.__waffle?.getDrawingStatus() !== null, null, { timeout: 15000 });
		// A view of a tab that is not in the document: the engine reports it
		// per view, and the sheet shows the report rather than blank paper.
		await page.evaluate(() => window.__waffle.addDrawingView('a-tab-that-is-gone', { view: 'Top' }));
		await page.waitForFunction(
			() => (window.__waffle?.getDrawingStatus()?.errors?.length ?? 0) > 0,
			null,
			{ timeout: 15000 }
		);
		await expect(page.getByTestId('drawing-sheet-errors')).toBeVisible();
		await expect(page.getByTestId('dwg-error').first()).toContainText('a-tab-that-is-gone');
	});

	test('export_svg hands back the same markup the sheet shows', async ({ waffle }) => {
		const page = waffle.page;
		await waffle.waitForReady();
		await plateAndDrawing(page);
		const answer = await callTool(page, 'export_svg', { deliver: 'agent' });
		expect(answer.isError, JSON.stringify(answer.structuredContent)).toBe(false);
		expect(answer.structuredContent.mime_type).toBe('image/svg+xml');
		expect(answer.structuredContent.bytes).toBeGreaterThan(500);
		const exported = answer.content.find((c) => c.type === 'resource')?.resource?.text;
		const shown = await page.evaluate(
			() => document.querySelector('[data-testid="drawing-sheet"] svg.wi-sheet').outerHTML
		);
		// Byte-identical but for what the browser normalizes in `outerHTML`
		// (attribute quoting and the xmlns it drops): the curve paths and the
		// dimension texts must match exactly, which is the property that
		// makes the export the sheet rather than a second rendering of it.
		const paths = (s) => [...s.matchAll(/ d="([^"]+)"/g)].map((m) => m[1]);
		expect(paths(exported)).toEqual(paths(shown));
		expect(exported).toContain('class="wi-sheet"');

		// And the file OPENS. The export is consumed as `image/svg+xml`,
		// which is XML, not HTML: a duplicate attribute on an element is a
		// fatal well-formedness error there and a silently-dropped one in
		// the HTML tokenizer, so `outerHTML` above cannot see it. Composing
		// the sheet by nesting each view's own `<svg>` is exactly where such
		// a pair comes from, so the parse is the oracle that keeps the
		// deliverable openable.
		const parseError = await page.evaluate((text) => {
			const doc = new DOMParser().parseFromString(text, 'image/svg+xml');
			return doc.querySelector('parsererror')?.textContent ?? null;
		}, exported);
		expect(parseError, `the exported sheet must be well-formed XML: ${parseError}`).toBeNull();
	});

	test('export_svg is refused on a part tab, by name', async ({ waffle }) => {
		const page = waffle.page;
		await waffle.waitForReady();
		const answer = await callTool(page, 'export_svg', { deliver: 'agent' });
		expect(answer.isError).toBe(true);
		expect(answer.structuredContent.error.code).toBe('TabKindNotSupported');
	});
});

// ══════════════════════════════════════════════════════ D4b: sections, details,
// the title block, the sheet PDF.

test.describe('the drawing tab, D4b', () => {
	test('a section view hatches its cap and the parent carries the cutting line', async ({
		waffle
	}) => {
		const page = waffle.page;
		await waffle.waitForReady();
		const { partTab, viewId } = await plateAndDrawing(page);

		// The cutting line is drawn on the TOP view the fixture made, in that
		// view's own plane, in METERS (the store's unit; the MCP tool takes
		// millimetres — each says which).
		//
		// Across the middle of the view's OWN reported box rather than across
		// the authored plate: where the plate lands in a view's (u, v) depends
		// on the sketch basis the app derived, which is not this test's
		// subject, and a line that missed the part came back as the typed "the
		// cut keeps no material at all" — correct, and not what is being
		// measured. It is also what a drafter does: the line is drawn across
		// the view in front of them.
		const box = await page.evaluate(
			(id) =>
				(window.__waffle.getDrawingStatus()?.drawing?.sheets?.[0]?.views ?? []).find(
					(v) => v.id === id
				)?.cache?.bbox ?? null,
			viewId
		);
		expect(box, 'the parent view drew something to cut').toBeTruthy();
		const [[minU, minV], [maxU, maxV]] = box;
		const midV = (minV + maxV) / 2;
		const pad = (maxU - minU) / 10;
		const section = await page.evaluate(
			([tab, parent, from, to]) =>
				window.__waffle.addDrawingView(tab, { parent, section: { from, to } }),
			[partTab, viewId, [minU - pad, midV], [maxU + pad, midV]]
		);
		expect(section, 'the engine added a section view').toBeTruthy();
		await page.waitForFunction(
			(id) => {
				const sheet = window.__waffle?.getDrawingStatus()?.drawing?.sheets?.[0];
				return (sheet?.views ?? []).some((v) => v.id === id && v.cache);
			},
			section,
			{ timeout: 20000 }
		);

		// The cap is HATCHED: the lines are in the SVG, which is the whole
		// point of a section view — a cut face that is not filled reads as a
		// hole.
		const hatched = await page.locator('[data-testid="drawing-sheet"] line.wi-hatch').count();
		expect(hatched, 'the cap must be hatched').toBeGreaterThan(0);

		// And the PARENT carries the cutting line with its arrows and the
		// letter, so a reader can match the two views.
		const marks = page.locator('[data-testid="drawing-sheet"] .wi-marks');
		await expect(marks.locator('line.wi-mark-cut').first()).toBeAttached();
		expect(
			await marks.locator('polygon.wi-mark-arrow').count(),
			'one arrow at each end of the cutting line'
		).toBe(2);
		const labels = await marks.locator('text.wi-mark-label').allTextContents();
		expect(labels, 'the letter prints at both arrows').toEqual(['A', 'A']);

		// The view is titled the way the standard titles it.
		const names = await page.evaluate(() =>
			(window.__waffle.getDrawingStatus()?.drawing?.sheets?.[0]?.views ?? []).map((v) => v.name)
		);
		expect(names).toContain('SECTION A-A');

		// And that title is DRAWN on the paper, not only carried as data
		// (D4b review). It used to reach the markup as an SVG `<title>`, which
		// no painter renders, the PDF writer skips by name and the DXF has
		// nowhere to put — so the sheet showed a hatched view with the letters
		// on its cutting line and nothing on the view they produced. ISO
		// 128-30 wants both.
		const captions = page.locator('[data-testid="drawing-sheet"] text.wi-view-caption');
		await expect(captions).toHaveCount(1);
		await expect(captions.first()).toHaveText('SECTION A-A');
		// The parent is NOT captioned: an orthographic view is identified by
		// where it sits, and labelling it too would bury the one label that
		// carries information.
		expect(
			await captions.count(),
			'only the derived view is captioned, not its parent'
		).toBe(1);
	});

	test('a detail view crops its parent to the disc and says so in the markup', async ({
		waffle
	}) => {
		const page = waffle.page;
		await waffle.waitForReady();
		const { partTab, viewId } = await plateAndDrawing(page);
		// On the view's OWN box, for the reason the section test states.
		const box = await page.evaluate(
			(id) =>
				(window.__waffle.getDrawingStatus()?.drawing?.sheets?.[0]?.views ?? []).find(
					(v) => v.id === id
				)?.cache?.bbox ?? null,
			viewId
		);
		expect(box, 'the parent view drew something to crop').toBeTruthy();
		const centre = [(box[0][0] + box[1][0]) / 2, (box[0][1] + box[1][1]) / 2];
		const detail = await page.evaluate(
			([tab, parent, center]) =>
				window.__waffle.addDrawingView(tab, {
					parent,
					scale: 2,
					detail: { center, radius: 0.004 }
				}),
			[partTab, viewId, centre]
		);
		expect(detail).toBeTruthy();
		await page.waitForFunction(
			(id) => {
				const sheet = window.__waffle?.getDrawingStatus()?.drawing?.sheets?.[0];
				return (sheet?.views ?? []).some((v) => v.id === id && v.cache);
			},
			detail,
			{ timeout: 20000 }
		);
		// The crop is a clip path in the view's OWN namespace — two details on
		// one sheet must not clip each other (the ids carry the view's uuid).
		const clip = await page.evaluate(
			(id) =>
				document.querySelector(`[data-testid="drawing-sheet"] clipPath[id="wi-crop-${id}"]`) !== null,
			detail
		);
		expect(clip, 'the detail declares its own crop clip path').toBe(true);
		// And the parent is marked with the circle and the letter.
		await expect(
			page.locator('[data-testid="drawing-sheet"] circle.wi-mark-detail').first()
		).toBeAttached();

		// The detail prints its own designation AND its own scale (D4b
		// review). A detail is the one view that does not share the sheet's
		// ratio — the title block's `Scale` row excludes it for exactly that
		// reason — so a detail whose scale is nowhere on the paper is a view a
		// reader cannot measure off.
		const caption = page.locator('[data-testid="drawing-sheet"] text.wi-view-caption');
		await expect(caption).toHaveCount(1);
		const captionText = await caption.first().textContent();
		expect(captionText).toContain('DETAIL');
		expect(captionText, 'the enlargement is printed where it is read').toContain('2:1');
		// Drawn OUTSIDE the crop clip, or the caption describing the crop
		// would be the first thing the crop removed.
		const clipped = await page.evaluate(
			() =>
				document
					.querySelector('[data-testid="drawing-sheet"] text.wi-view-caption')
					?.closest('[clip-path]') !== null
		);
		expect(clipped, 'the caption is not inside the crop it names').toBe(false);
	});

	test('the title block prints the document, the sheet number, the scale and the standard', async ({
		waffle
	}) => {
		const page = waffle.page;
		await waffle.waitForReady();
		await plateAndDrawing(page);
		const block = page.locator('[data-testid="drawing-sheet"] .wi-title-block');
		await expect(block).toBeAttached();
		const labels = await block.locator('text.wi-title-label').allTextContents();
		expect(labels).toEqual(['Title', 'Sheet', 'Scale', 'Projection', 'Date', 'Drawn by']);
		const values = await block.locator('text.wi-title-value').allTextContents();
		expect(values[1], 'the sheet number is its POSITION, not a stored string').toBe('1 / 1');
		expect(values[2]).toBe('1:1');
		// The standard prints as WORDS: the ISO 5456-2 cone symbol is a mirror
		// pair whose handedness is a convention, and a symbol that might be
		// the wrong way round is worse than none (see `renderTitleBlock`).
		expect(values[3]).toBe('Third angle');

		// Flipping the standard moves the row, which is what makes it the
		// document's setting rather than a label.
		await page.evaluate(() => window.__waffle.editDrawingSheet({ projectionAngle: 'First' }));
		await page.waitForFunction(
			() => window.__waffle?.getDrawing()?.projection_angle?.type === 'First',
			null,
			{ timeout: 15000 }
		);
		await expect(block.locator('text.wi-title-value').nth(3)).toHaveText('First angle');

		// And turning it off takes the whole block off the paper.
		await page.evaluate(() => window.__waffle.editDrawingSheet({ titleBlock: false }));
		await expect(block).toHaveCount(0);
	});

	test('an expression row prints its value on the paper and its source nowhere', async ({
		waffle
	}) => {
		const page = waffle.page;
		await waffle.waitForReady();
		await plateAndDrawing(page);

		// D4c: `TitleBlockField.expr` holds the SOURCE and the sheet shows what
		// it evaluates to — "never the same record, which is what stops a
		// reopened file printing last week's number". The renderer is handed the
		// whole sheet, so the source IS one property away from it
		// (`title_block.fields[i].expr`); keeping it off the paper is a
		// discipline rather than an impossibility, which is why it is measured
		// here instead of argued in a comment.
		const source = '7mm * 1.1';
		await page.evaluate(
			(expr) =>
				window.__waffle.editDrawingSheet({
					titleBlockFields: [{ label: 'Stock', expr }]
				}),
			source
		);
		const block = page.locator('[data-testid="drawing-sheet"] .wi-title-block');
		await expect(block.locator('text.wi-title-label')).toHaveText(['Stock']);
		// The EVALUATED text, with the unit the expression's own dimension
		// carries — which is the thing a reader can check against a rule.
		await expect(block.locator('text.wi-title-value')).toHaveText('7.7 mm');

		// And the source is nowhere in the sheet's markup, nor in the SVG the
		// export writes from the same renderer. Both, because the screen and the
		// file are the same bytes by construction and a test of only one would
		// not notice the construction changing.
		const drawn = await page.locator('[data-testid="drawing-sheet"]').innerHTML();
		expect(drawn).toContain('7.7 mm');
		expect(drawn, 'the source must not reach the paper').not.toContain(source);
		const answer = await callTool(page, 'export_svg', { deliver: 'agent' });
		expect(answer.isError, JSON.stringify(answer.structuredContent)).toBe(false);
		const svg = answer.content.find((c) => c.type === 'resource')?.resource?.text ?? '';
		expect(svg.length, 'the export returned no markup').toBeGreaterThan(500);
		expect(svg).toContain('7.7 mm');
		expect(svg, 'the source must not reach the exported file').not.toContain(source);
	});

	test('export_pdf writes a one-page PDF of the sheet', async ({ waffle }) => {
		const page = waffle.page;
		await waffle.waitForReady();
		await plateAndDrawing(page);
		const answer = await callTool(page, 'export_pdf', { deliver: 'agent' });
		expect(answer.isError, JSON.stringify(answer.structuredContent)).toBe(false);
		expect(answer.structuredContent.mime_type).toBe('application/pdf');
		expect(answer.structuredContent.pages).toBe(1);
		expect(answer.structuredContent.bytes).toBeGreaterThan(500);

		const blob = answer.content.find((c) => c.type === 'resource')?.resource?.blob;
		expect(blob, 'the PDF rides back base64 for deliver:agent').toBeTruthy();
		const file = await page.evaluate((b64) => {
			const binary = atob(b64);
			const bytes = new Uint8Array(binary.length);
			for (let i = 0; i < binary.length; i++) bytes[i] = binary.charCodeAt(i);
			const text = new TextDecoder('latin1').decode(bytes);
			// Every xref entry must point at the `N 0 obj` it names. The
			// offsets are counted in BYTES, so an off-by-one here is a file no
			// reader opens — and the only check that catches it is following
			// each one. (Verified against pypdf, pdfminer.six and PyMuPDF; this
			// is the dependency-free form of the same walk.)
			const startxref = Number(/startxref\s+(\d+)/.exec(text)?.[1] ?? NaN);
			const table = text.slice(startxref);
			const entries = [...table.matchAll(/^(\d{10}) (\d{5}) ([nf]) $/gm)];
			const badOffsets = entries
				.map((m, i) => ({ i, at: Number(m[1]), free: m[3] === 'f' }))
				.filter((e) => !e.free && !text.startsWith(`${e.i} 0 obj`, e.at))
				.map((e) => e.i);
			return {
				header: text.slice(0, 8),
				length: bytes.length,
				pageObjects: (text.match(/\/Type\s*\/Page[^s]/g) ?? []).length,
				count: /\/Count\s+(\d+)/.exec(text)?.[1] ?? null,
				tail: text.slice(-6),
				startxref,
				xrefHeader: /xref\s+0 (\d+)/.exec(table)?.[1] ?? null,
				entries: entries.length,
				badOffsets,
				mediaBox: /\/MediaBox \[([^\]]*)\]/.exec(text)?.[1] ?? null,
				cm: /q ([0-9.]+) 0 0 (-[0-9.]+) 0 ([0-9.]+) cm/.exec(text)?.slice(1) ?? null,
				// Each operator is its own line in the content stream, so the
				// rectangle is anchored at a line start, not after a space.
				borderRect: /^10 10 ([0-9.]+) ([0-9.]+) re$/m.exec(text)?.slice(1) ?? null
			};
		}, blob);
		expect(file.header).toBe('%PDF-1.4');
		expect(file.count, 'one sheet is one page').toBe('1');
		expect(file.pageObjects, 'exactly one /Type /Page object').toBe(1);
		expect(file.tail).toBe('%%EOF\n');
		expect(Number(file.startxref)).toBeGreaterThan(0);
		expect(Number(file.startxref)).toBeLessThan(file.length);
		expect(file.length).toBe(answer.structuredContent.bytes);
		expect(file.badOffsets, 'every xref offset points at its own object').toEqual([]);
		expect(file.entries, 'the xref table is as long as it claims').toBe(
			Number(file.xrefHeader)
		);

		// The page is the PAPER, and one user unit is one paper millimetre.
		//
		// Both are measurable from the file alone and both were wrong at the
		// same place: the `cm` scale was written through the 3-decimal
		// coordinate formatter, so `72/25.4` became `2.835` — 12.5 ppm high,
		// which drew a 100 mm dimension at 100.0125 mm and pushed an A3 sheet
		// 52 µm past the MediaBox that bounds it. A scale error is invisible in
		// every structural assertion above, which is why these two are here.
		// The paper size comes from the mounted sheet's own viewBox, so what is
		// compared is the PDF against the SVG on the screen — which is the
		// claim this writer makes by scanning that SVG in the first place.
		const PT_PER_MM = 72 / 25.4;
		const onScreen = await sheetDom(page);
		const vb = String(onScreen.viewBox).split(/\s+/).map(Number);
		expect(vb.slice(0, 2), 'the sheet viewBox starts at the paper corner').toEqual([0, 0]);
		const [paperW, paperH] = [vb[2], vb[3]];
		const box = String(file.mediaBox).split(/\s+/).map(Number);
		expect(box.slice(0, 2)).toEqual([0, 0]);
		expect(box[2]).toBeCloseTo(paperW * PT_PER_MM, 3);
		expect(box[3]).toBeCloseTo(paperH * PT_PER_MM, 3);
		expect(file.cm, 'the page opens with one millimetre transform').toBeTruthy();
		expect(Number(file.cm[0])).toBeCloseTo(PT_PER_MM, 5);
		expect(Number(file.cm[1])).toBeCloseTo(-PT_PER_MM, 5);
		expect(Number(file.cm[2])).toBeCloseTo(paperH * PT_PER_MM, 3);
		// The sheet border is inset 10 mm on every side (ISO 5457), so in a
		// space whose unit is one millimetre it is written literally. Any
		// scaling of user space moves these two numbers.
		expect(file.borderRect, 'the border frame is written in millimetres').toBeTruthy();
		expect(Number(file.borderRect[0])).toBeCloseTo(paperW - 20, 6);
		expect(Number(file.borderRect[1])).toBeCloseTo(paperH - 20, 6);
	});

	test('export_pdf is refused on a part tab, by name', async ({ waffle }) => {
		const page = waffle.page;
		await waffle.waitForReady();
		const answer = await callTool(page, 'export_pdf', { deliver: 'agent' });
		expect(answer.isError).toBe(true);
		expect(answer.structuredContent.error.code).toBe('TabKindNotSupported');
	});
});
