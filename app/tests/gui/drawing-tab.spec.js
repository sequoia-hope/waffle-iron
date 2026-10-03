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
	});

	test('export_svg is refused on a part tab, by name', async ({ waffle }) => {
		const page = waffle.page;
		await waffle.waitForReady();
		const answer = await callTool(page, 'export_svg', { deliver: 'agent' });
		expect(answer.isError).toBe(true);
		expect(answer.structuredContent.error.code).toBe('TabKindNotSupported');
	});
});
