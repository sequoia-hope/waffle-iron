/**
 * Visual view placement on the sheet (`specs/drawings_and_mbd.md` §8, D4e).
 *
 * The two tools that put a view where the pointer is: the place-view dialog
 * with its hover ghost, and the projected-view tool with its eight sectors
 * around a parent view. Driven with REAL pointer events against the rendered
 * sheet, and asserted on the DOM — the ghost carries its own centre and extent
 * in `data-` attributes for exactly this, where a screenshot baseline would rot
 * on a font change and say nothing about whether the box is the right size.
 *
 * What each test is actually pinning:
 *
 * - the ghost box is the view's drawn extent at the chosen scale, so what the
 *   user sees before clicking is what lands;
 * - the placement the click makes is the ghost's own, snapped;
 * - the eight sectors map to the eight `ProjectedDirection`s, corners included
 *   (the isometrics D4e added to the engine);
 * - the projection standard changes what the ghost SAYS the view shows without
 *   the tool deciding anything — the label comes back from the engine's probe.
 */
import { test, expect } from './helpers/waffle-test.js';
import { collectCrashErrors, expectNoAnyCrash } from './helpers/state.js';

/** Strict crash oracle throughout: every test here drives a projection. */
let crashes = null;
test.beforeEach(({ waffle }) => {
	crashes = collectCrashErrors(waffle.page);
});
test.afterEach(() => {
	const tracker = crashes;
	crashes = null;
	if (tracker) expectNoAnyCrash(tracker);
});

/** A 40 × 25 × 10 mm plate, in METERS (the document's own unit). */
const PLATE_W = 0.04;
const PLATE_D = 0.025;
const PLATE_T = 0.01;

async function partTabId(page) {
	return page.evaluate(() => {
		const tabs = window.__waffle.getDocumentState().documentTabs ?? [];
		return (tabs.find((t) => t.kind === 'Part') ?? tabs[0])?.id ?? null;
	});
}

/**
 * The plate on the active Part tab plus an empty Drawing tab, open.
 * Returns `{ partTab, drawingTab }`. The fixture build uses
 * `addSketchEntity`, which CLAUDE.md allows for setup.
 */
async function plateAndDrawingTab(page) {
	await page.evaluate(() => window.__waffle.enterSketch([0, 0, 0], [0, 0, 1]));
	await page.waitForFunction(() => window.__waffle?.getState()?.sketchMode?.active === true, null, {
		timeout: 5000
	});
	await page.evaluate(
		([w, d]) => {
			const waffle = window.__waffle;
			for (const [id, x, y] of [
				[1, 0, 0],
				[2, w, 0],
				[3, w, d],
				[4, 0, d]
			]) {
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
	await page.waitForFunction(
		() => (window.__waffle?.getFeatureTree()?.features?.length ?? 0) >= 1,
		null,
		{ timeout: 10000 }
	);
	await page.evaluate(() => window.__waffle.showExtrudeDialog());
	await page.evaluate((t) => window.__waffle.applyExtrude(t, 0, false), PLATE_T);
	await page.waitForFunction(
		() => (window.__waffle?.getMeshes() ?? []).some((m) => m.triangleCount > 0),
		null,
		{ timeout: 20000 }
	);

	const partTab = await partTabId(page);
	expect(partTab, 'the document has a Part tab to draw').toBeTruthy();
	const drawingTab = await page.evaluate(() => window.__waffle.addTab('Drawing'));
	await page.evaluate((id) => window.__waffle.switchTab(id), drawingTab);
	await page.waitForFunction(() => window.__waffle?.getDrawingStatus() !== null, null, {
		timeout: 15000
	});
	return { partTab, drawingTab };
}

/** Add a named view through the store and wait for its curves. */
async function addView(page, partTab, view) {
	const id = await page.evaluate(
		([tab, v]) => window.__waffle.addDrawingView(tab, { view: v }),
		[partTab, view]
	);
	expect(id, `the ${view} view was added`).toBeTruthy();
	await page.waitForFunction(
		(viewId) => {
			const sheet = window.__waffle?.getDrawingStatus()?.drawing?.sheets?.[0];
			return (sheet?.views ?? []).some(
				(v) => v.id === viewId && (v.cache?.curves?.length ?? 0) > 0
			);
		},
		id,
		{ timeout: 20000 }
	);
	return id;
}

/**
 * Screen coordinates of a point on the paper, in sheet millimetres measured
 * from the BOTTOM-left corner — the inverse of `viewPlacement.js`'s
 * `paperPointMm`, so the test speaks the sheet's own units.
 */
async function screenAt(page, mm) {
	const at = await page.evaluate((target) => {
		const el = document.querySelector('[data-testid="drawing-sheet"] svg.wi-sheet');
		if (!el) throw new Error('no sheet SVG is mounted');
		const box = el.getBoundingClientRect();
		const vb = el.getAttribute('viewBox').split(/\s+/).map(Number);
		const [wMm, hMm] = [vb[2], vb[3]];
		return {
			x: box.left + (target[0] / wMm) * box.width,
			y: box.top + ((hMm - target[1]) / hMm) * box.height
		};
	}, mm);
	return at;
}

/** Move the pointer to a paper point and wait for the ghost (or its absence). */
async function hover(page, mm, { expectGhost = true } = {}) {
	const at = await screenAt(page, mm);
	await page.mouse.move(at.x, at.y);
	const ghost = page.getByTestId('dwg-ghost');
	if (expectGhost) {
		await expect(ghost).toHaveCount(1, { timeout: 10000 });
		return page.evaluate(() => {
			const g = document.querySelector('[data-testid="dwg-ghost"]');
			return {
				label: g.dataset.label,
				centre: g.dataset.centreMm.split(',').map(Number),
				extent: g.dataset.extentMm ? g.dataset.extentMm.split(',').map(Number) : null,
				aligned: g.dataset.aligned === 'yes'
			};
		});
	}
	await expect(ghost).toHaveCount(0, { timeout: 10000 });
	return null;
}

/** The sheet's views as the store has them. */
async function views(page) {
	return page.evaluate(
		() => window.__waffle.getDrawingStatus()?.drawing?.sheets?.[0]?.views ?? []
	);
}

/** Open the dialog, fill it in and press OK. */
async function openPlaceView(page, { view = 'Top', scale = '1' } = {}) {
	await page.getByTestId('dwg-tool-place-view').click();
	await expect(page.getByTestId('dwg-place-view-dialog')).toBeVisible();
	await page.getByTestId('dwg-place-view-direction').selectOption(view);
	await page.getByTestId('dwg-place-scale').selectOption(scale);
	await page.getByTestId('dwg-place-ok').click();
	await expect(page.getByTestId('sheet-surface')).toHaveAttribute(
		'data-sheet-mode',
		'place-view'
	);
}

test.describe('D4e place-view tool', () => {
	test('the ghost box is the view at its scale, and the click places it there', async ({
		waffle
	}) => {
		const page = waffle.page;
		await waffle.waitForReady();
		const { partTab } = await plateAndDrawingTab(page);

		// 1:1 — the plate's top view is its 40 × 25 mm outline, and the ghost
		// box is that extent (projected from the source's bounds, so an upper
		// bound: for a prismatic plate the two coincide).
		//
		// Compared as a SET of the two sides, because which authored extent
		// lands on paper u and which on v is the SKETCH's derived in-plane
		// basis talking, not this tool's — the D3 harness's finding, and
		// pinning it here would make this spec fail on a change to that
		// derivation rather than to the ghost.
		await openPlaceView(page, { view: 'Top', scale: '1' });
		const at = [150, 200];
		const ghost = await hover(page, at);
		expect([...ghost.extent].sort((a, b) => a - b)[0]).toBeCloseTo(25, 3);
		expect([...ghost.extent].sort((a, b) => a - b)[1]).toBeCloseTo(40, 3);
		expect(ghost.label).toContain('Top');
		// Snapped to the 5 mm sheet grid — the hover point is already on it.
		expect(ghost.centre[0] % 5).toBeCloseTo(0, 6);
		expect(ghost.centre[1] % 5).toBeCloseTo(0, 6);

		const before = (await views(page)).length;
		const screen = await screenAt(page, at);
		await page.mouse.click(screen.x, screen.y);
		await page.waitForFunction(
			(n) => (window.__waffle.getDrawingStatus()?.drawing?.sheets?.[0]?.views ?? []).length > n,
			before,
			{ timeout: 20000 }
		);
		const placed = (await views(page)).at(-1);
		expect(placed.projection.type).toBe('Named');
		expect(placed.projection.view.type).toBe('Top');
		// The placement IS the ghost's centre: what was shown is what landed.
		// To the ghost attribute's own three decimals (a thousandth of a
		// millimetre, which is what `viewPlacement.js` writes into markup).
		expect(placed.placement_mm[0]).toBeCloseTo(ghost.centre[0], 3);
		expect(placed.placement_mm[1]).toBeCloseTo(ghost.centre[1], 3);
		// And the mode is over — one click, one view.
		await expect(page.getByTestId('sheet-surface')).toHaveAttribute('data-sheet-mode', 'select');

		// The ghost's extent equals the view's own drawn extent, to within the
		// mesh-vs-analytic deficit the spec allows (zero for a plate: every
		// extreme is on an edge).
		const drawn = placed.cache?.bbox;
		expect(drawn, 'the placed view has a drawn bbox').toBeTruthy();
		expect((drawn[1][0] - drawn[0][0]) * 1000).toBeCloseTo(ghost.extent[0], 3);
		expect((drawn[1][1] - drawn[0][1]) * 1000).toBeCloseTo(ghost.extent[1], 3);
	});

	test('the ghost halves with the scale, and Escape cancels without placing', async ({
		waffle
	}) => {
		const page = waffle.page;
		await waffle.waitForReady();
		const { partTab } = await plateAndDrawingTab(page);
		await openPlaceView(page, { view: 'Top', scale: '0.5' });
		const ghost = await hover(page, [200, 150]);
		expect([...ghost.extent].sort((a, b) => a - b)[0]).toBeCloseTo(12.5, 3);
		expect([...ghost.extent].sort((a, b) => a - b)[1]).toBeCloseTo(20, 3);

		const before = (await views(page)).length;
		await page.keyboard.press('Escape');
		await expect(page.getByTestId('sheet-surface')).toHaveAttribute('data-sheet-mode', 'select');
		await expect(page.getByTestId('dwg-ghost')).toHaveCount(0);
		expect((await views(page)).length, 'Escape placed nothing').toBe(before);
		expect(partTab).toBeTruthy();
	});

	test('a scale off the series is taken, and garbage is refused in words', async ({ waffle }) => {
		// D4e review. The free field exists because 3:1 is a scale people draw;
		// what it must not do is refuse QUIETLY. `<input type="number">` hands
		// back an empty string for anything it could not parse, so "3x", a
		// pasted "1:2" and a blank field all arrive identically — and a greyed
		// OK with no reason is a refusal the user has to guess at.
		const page = waffle.page;
		await waffle.waitForReady();
		const { partTab } = await plateAndDrawingTab(page);

		await page.getByTestId('dwg-tool-place-view').click();
		await expect(page.getByTestId('dwg-place-view-dialog')).toBeVisible();
		await page.getByTestId('dwg-place-view-direction').selectOption('Top');
		await page.getByTestId('dwg-place-scale').selectOption('free');
		const free = page.getByTestId('dwg-place-scale-free');

		// Unparseable: named, and OK is closed. Typed as an EMPTY field,
		// because that is what the control delivers for anything it could not
		// parse — the browser will not even let a test put "1:2" in a number
		// input, and a user who types it sees the same blank value. Which is
		// the whole reason the blank case has to be named rather than just
		// greying the button.
		await free.fill('');
		await expect(page.getByTestId('dwg-place-scale-refusal')).toContainText('A ratio is a number');
		await expect(page.getByTestId('dwg-place-ok')).toBeDisabled();
		// Parses and is still not a scale.
		await free.fill('0');
		await expect(page.getByTestId('dwg-place-scale-refusal')).toContainText(
			'greater than zero'
		);
		await expect(page.getByTestId('dwg-place-ok')).toBeDisabled();

		// 3:1, which is off the ISO 5455 series and is taken anyway.
		await free.fill('3');
		await expect(page.getByTestId('dwg-place-scale-refusal')).toHaveCount(0);
		await page.getByTestId('dwg-place-ok').click();
		await expect(page.getByTestId('sheet-surface')).toHaveAttribute(
			'data-sheet-mode',
			'place-view'
		);
		const ghost = await hover(page, [200, 150]);
		// Three times the plate's 40 × 25 mm top view.
		expect([...ghost.extent].sort((a, b) => a - b)[0]).toBeCloseTo(75, 3);
		expect([...ghost.extent].sort((a, b) => a - b)[1]).toBeCloseTo(120, 3);
		const screen = await screenAt(page, [200, 150]);
		await page.mouse.click(screen.x, screen.y);
		await page.waitForFunction(
			() => (window.__waffle.getDrawingStatus()?.drawing?.sheets?.[0]?.views ?? []).length > 0,
			null,
			{ timeout: 20000 }
		);
		expect((await views(page)).at(-1).scale).toBeCloseTo(3, 9);
		expect(partTab).toBeTruthy();
	});

	test('the ghost snaps onto an existing view to keep a projection group aligned', async ({
		waffle
	}) => {
		const page = waffle.page;
		await waffle.waitForReady();
		const { partTab } = await plateAndDrawingTab(page);
		const first = await addView(page, partTab, 'Front');
		const anchor = (await views(page)).find((v) => v.id === first).placement_mm;

		await openPlaceView(page, { view: 'Top', scale: '1' });
		// Three millimetres off the existing view's column: inside the 8 mm
		// alignment band, so the ghost takes the column and keeps the row.
		const ghost = await hover(page, [anchor[0] + 3, anchor[1] + 60]);
		expect(ghost.aligned, 'the ghost reports the alignment').toBe(true);
		expect(ghost.centre[0]).toBeCloseTo(anchor[0], 3);
		expect(ghost.centre[1]).not.toBeCloseTo(anchor[1], 1);
	});
});

test.describe('D4e projected-view tool', () => {
	/** Enter the tool and pick `viewId` as the parent. */
	async function pickParent(page, viewId) {
		await page.getByTestId('dwg-tool-project-view').click();
		await expect(page.getByTestId('sheet-surface')).toHaveAttribute(
			'data-sheet-mode',
			'project-view'
		);
		const parent = (await views(page)).find((v) => v.id === viewId);
		const at = await screenAt(page, parent.placement_mm);
		await page.mouse.move(at.x, at.y);
		await page.mouse.click(at.x, at.y);
		return parent;
	}

	test('the eight sectors around a view are the four sides and the four isometrics', async ({
		waffle
	}) => {
		const page = waffle.page;
		await waffle.waitForReady();
		const { partTab } = await plateAndDrawingTab(page);
		const front = await addView(page, partTab, 'Front');
		const parent = await pickParent(page, front);
		const [cx, cy] = parent.placement_mm;

		// Well clear of the parent's 40 × 10 mm box in each sector.
		const sectors = [
			[[cx + 60, cy], 'Right'],
			[[cx - 60, cy], 'Left'],
			[[cx, cy + 50], 'Up'],
			[[cx, cy - 50], 'Down'],
			[[cx + 60, cy + 50], 'Iso (up-right)'],
			[[cx - 60, cy + 50], 'Iso (up-left)'],
			[[cx + 60, cy - 50], 'Iso (down-right)'],
			[[cx - 60, cy - 50], 'Iso (down-left)']
		];
		for (const [mm, label] of sectors) {
			const ghost = await hover(page, mm);
			expect(ghost.label, `the sector at ${mm}`).toContain(label);
			expect(ghost.label).toContain('Front');
		}
		// Inside the parent's own box there is no direction, so no ghost.
		await hover(page, [cx, cy], { expectGhost: false });
	});

	test('clicking a corner sector adds the isometric the ghost previewed', async ({ waffle }) => {
		const page = waffle.page;
		await waffle.waitForReady();
		const { partTab } = await plateAndDrawingTab(page);
		const front = await addView(page, partTab, 'Front');
		const parent = await pickParent(page, front);
		const [cx, cy] = parent.placement_mm;

		const ghost = await hover(page, [cx + 60, cy + 50]);
		expect(ghost.label).toContain('Iso (up-right)');
		const before = (await views(page)).length;
		const screen = await screenAt(page, [cx + 60, cy + 50]);
		await page.mouse.click(screen.x, screen.y);
		await page.waitForFunction(
			(n) => (window.__waffle.getDrawingStatus()?.drawing?.sheets?.[0]?.views ?? []).length > n,
			before,
			{ timeout: 20000 }
		);
		const added = (await views(page)).at(-1);
		expect(added.projection.type).toBe('ProjectedFrom');
		expect(added.projection.direction.type).toBe('UpRight');
		expect(added.projection.parent).toBe(front);
		// The engine placed it, and the ghost had shown that placement — the
		// visual add and a panel add are the same view.
		expect(added.placement_mm[0]).toBeCloseTo(ghost.centre[0], 3);
		expect(added.placement_mm[1]).toBeCloseTo(ghost.centre[1], 3);
		// It is an ISOMETRIC: all three of the plate's extents are on the
		// paper, so the drawn box is wider AND taller than the front view's
		// 40 × 10.
		await page.waitForFunction(
			(id) => {
				const v = (
					window.__waffle.getDrawingStatus()?.drawing?.sheets?.[0]?.views ?? []
				).find((x) => x.id === id);
				return (v?.cache?.curves?.length ?? 0) > 0;
			},
			added.id,
			{ timeout: 20000 }
		);
		const drawn = (await views(page)).find((v) => v.id === added.id).cache.bbox;
		expect((drawn[1][1] - drawn[0][1]) * 1000).toBeGreaterThan(10);
	});

	test('the ghost follows an edit made while the tool is running', async ({ waffle }) => {
		// D4e review. The probe's answer is cached per hover — it has to be,
		// since a pointer move fires per frame and the answer is a function of
		// the document rather than of the cursor. What it must NOT be is cached
		// across a change to that document: nothing leaves placement mode when
		// the document moves, so without a revision in the cache key the ghost
		// goes on describing geometry that no longer exists. Measured by moving
		// the PARENT view, which is what the projected placement is computed
		// from: before the fix the ghost stayed where the parent had been.
		const page = waffle.page;
		await waffle.waitForReady();
		const { partTab } = await plateAndDrawingTab(page);
		const front = await addView(page, partTab, 'Front');
		const parent = await pickParent(page, front);
		const [cx, cy] = parent.placement_mm;
		const before = await hover(page, [cx + 60, cy]);

		const moved = [cx - 30, cy];
		await page.evaluate(
			([id, at]) => window.__waffle.editDrawingView(id, { placementMm: at }),
			[front, moved]
		);
		await page.waitForFunction(
			([id, x]) => {
				const v = (
					window.__waffle.getDrawingStatus()?.drawing?.sheets?.[0]?.views ?? []
				).find((w) => w.id === id);
				return Math.abs((v?.placement_mm?.[0] ?? 0) - x) < 1e-6;
			},
			[front, moved[0]],
			{ timeout: 20000 }
		);

		// Still in the tool, still the same sector relative to the view's NEW
		// position — and the ghost has to have come with it.
		await expect(page.getByTestId('sheet-surface')).toHaveAttribute(
			'data-sheet-mode',
			'project-view'
		);
		const after = await hover(page, [moved[0] + 60, moved[1]]);
		expect(after.label).toContain('Right of Front');
		expect(after.centre[0]).toBeCloseTo(before.centre[0] - 30, 3);
		expect(after.centre[1]).toBeCloseTo(before.centre[1], 3);
	});

	test('two moves make one probe, and the older answer does not win', async ({ waffle }) => {
		// D4e review, pinning the race fix by DECIDING when the answer lands
		// rather than by racing the machine.
		//
		// A probe is a round trip and a pointer move fires per frame, so two
		// moves are in the air at once routinely. Two properties hold it
		// together, and both are measured here:
		//
		//  - two moves asking the same question (the same source and named
		//    view — which is every move of one place-view gesture) make ONE
		//    round trip: the second AWAITS the first rather than bailing out
		//    with whatever was cached, which is the bug that left the ghost
		//    with no box at all under load;
		//  - when it lands, both continuations run, and the one whose turn has
		//    passed must not draw. The ghost is the LAST cursor's.
		//
		// Driven through `window.__wafflePlacement`, the sheet component's own
		// import of the module — a spec that reached the module with its own
		// dynamic import drove a different instance whenever the two
		// specifiers resolved apart, and then measured an empty ghost.
		const page = waffle.page;
		await waffle.waitForReady();
		const { partTab } = await plateAndDrawingTab(page);
		await expect(page.getByTestId('drawing-sheet')).toBeVisible();

		const result = await page.evaluate(async (tab) => {
			const mode = window.__wafflePlacement;
			if (!mode) throw new Error('the sheet published no placement door');
			const pending = [];
			mode.setProbeForTest(() => new Promise((resolve) => pending.push(resolve)));
			try {
				mode.startPlaceView({ sourceTab: tab, view: 'Top', scale: 1 });
				const first = mode.placementPointerMove([100, 100]);
				const second = mode.placementPointerMove([200, 150]);
				const asked = pending.length;
				for (const resolve of pending) {
					resolve({
						bounds: [
							[0, 0, 0],
							[0.04, 0.025, 0.01]
						],
						views: [
							{
								placement_mm: [0, 0],
								dir: [0, 0, -1],
								up: [0, 1, 0],
								name: 'Top of plate',
								error: null
							}
						],
						warnings: []
					});
				}
				await Promise.all([first, second]);
				const ghost = mode.placementGhostSvg();
				return {
					asked,
					centre: /data-centre-mm="([^"]+)"/.exec(ghost)?.[1] ?? null,
					extent: /data-extent-mm="([^"]*)"/.exec(ghost)?.[1] ?? null
				};
			} finally {
				mode.setProbeForTest();
				mode.cancelPlacement();
			}
		}, partTab);

		expect(result.asked, 'two moves, one round trip').toBe(1);
		// The second move's cursor, snapped to the 5 mm grid — not the first's.
		expect(result.centre).toBe('200,150');
		// And it was sized from the answer, so the surviving ghost is the
		// complete one rather than a placement with no box.
		expect(result.extent).toBe('40,25');
	});

	test('a view the tool adds is byte-identical to the one the panel adds', async ({ waffle }) => {
		// D4e review: the claim §8 makes in so many words — "a visually placed
		// projected view and a panel-added one are byte-identical". Pinned on
		// the SAVED record rather than on the placement alone, because the two
		// paths also decide a name and a scale, and the probe answers all
		// three. Only the uuid may differ, and it is minted per add.
		const page = waffle.page;
		await waffle.waitForReady();
		const { partTab } = await plateAndDrawingTab(page);
		const front = await addView(page, partTab, 'Front');
		const parent = await pickParent(page, front);
		const [cx, cy] = parent.placement_mm;

		const ghost = await hover(page, [cx + 60, cy + 50]);
		expect(ghost.label).toContain('Iso (up-right)');
		const screen = await screenAt(page, [cx + 60, cy + 50]);
		await page.mouse.click(screen.x, screen.y);
		await page.waitForFunction(
			(n) => (window.__waffle.getDrawingStatus()?.drawing?.sheets?.[0]?.views ?? []).length > n,
			1,
			{ timeout: 20000 }
		);
		const byTool = (await views(page)).at(-1);
		await page.evaluate((id) => window.__waffle.deleteDrawingView(id), byTool.id);
		await page.waitForFunction(
			(n) => (window.__waffle.getDrawingStatus()?.drawing?.sheets?.[0]?.views ?? []).length === n,
			1,
			{ timeout: 20000 }
		);

		// The panel's own door, with the same three answers the tool gave it.
		const panelId = await page.evaluate(
			([tab, id]) =>
				window.__waffle.addDrawingView(tab, { parent: id, direction: 'UpRight', scale: 1 }),
			[partTab, front]
		);
		await page.waitForFunction(
			(id) => {
				const v = (
					window.__waffle.getDrawingStatus()?.drawing?.sheets?.[0]?.views ?? []
				).find((x) => x.id === id);
				return (v?.cache?.curves?.length ?? 0) > 0;
			},
			panelId,
			{ timeout: 20000 }
		);
		const byPanel = (await views(page)).find((v) => v.id === panelId);

		const normalize = (v) => JSON.stringify({ ...v, id: '<id>', cache_key: '<key>' });
		expect(normalize(byTool)).toBe(normalize(byPanel));
	});

	test('the projection standard changes what the ghost says it shows, not the sector', async ({
		waffle
	}) => {
		const page = waffle.page;
		await waffle.waitForReady();
		const { partTab } = await plateAndDrawingTab(page);
		const front = await addView(page, partTab, 'Front');

		const parent = await pickParent(page, front);
		const [cx, cy] = parent.placement_mm;
		const third = await hover(page, [cx + 60, cy]);
		// Third angle: the view placed right IS the right-hand view, so the
		// ghost has nothing extra to say.
		expect(third.label).toBe('Right of Front');

		await page.keyboard.press('Escape');
		await page.getByTestId('dwg-projection-angle').selectOption('First');
		await page.waitForFunction(
			() => window.__waffle.getDrawingStatus()?.drawing?.projection_angle?.type === 'First',
			null,
			{ timeout: 20000 }
		);
		await pickParent(page, front);
		const first = await hover(page, [cx + 60, cy]);
		// First angle: the same sector, the same placement — and the engine
		// says the view there shows the LEFT side. The tool did not decide
		// that and does not know the rule; it reads `shows` off the probe.
		expect(first.label).toContain('Right of Front');
		expect(first.label).toContain('shows left');
		expect(first.centre[0]).toBeCloseTo(third.centre[0], 3);
		expect(first.centre[1]).toBeCloseTo(third.centre[1], 3);
	});
});
