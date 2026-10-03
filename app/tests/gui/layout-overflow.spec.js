/**
 * Layout overflow — nothing interactive may ever leave the window.
 *
 * The page cannot scroll (`html, body { overflow: hidden }`), so chrome that
 * outgrows its box is clipped and unreachable. This spec drives the shell
 * into its widest states (sketch mode, dialogs, overlay browsers, both side
 * panels dragged to their limits, banners) across a sweep of window widths
 * and asserts, with the element-level oracle in helpers/layout.js, that every
 * button and input is inside the window and the 3D view keeps a usable size.
 *
 * Runs in the fast tier: a new toolbar button that does not fit fails here.
 */
import { test, expect } from './helpers/waffle-test.js';
import { clickSketch, clickRectangle, clickFinishSketch, clickExtrude, clickToolbarAction } from './helpers/toolbar.js';
import { drawRectangle } from './helpers/canvas.js';
import { waitForEntityCount, waitForFeatureCount } from './helpers/state.js';
import { expectNothingOffscreen, expectViewportUsable, dragDivider } from './helpers/layout.js';

/** Desktop widths the shell must survive (the mobile projects cover ≤768). */
const WIDTHS = [1920, 1600, 1366, 1280, 1100, 1024, 900, 800];

/**
 * Resize the window and wait for the toolbar's collapse ladder to have walked
 * for it: a resize re-measures the toolbar in a microtask after a
 * ResizeObserver tick, and the toolbar counts completed walks in
 * `data-relayout-seq`. A fixed 150 ms wait lost that race on the loaded
 * four-core CI runner (four Chromiums rendering through SwiftShader): the
 * oracle then measured a toolbar one or two rungs short of the new width and
 * reported its right end outside the window. The wait also requires the
 * in-flow content to fit, so a ladder that runs out of rungs still fails
 * here, with a timeout naming the state.
 */
async function resizeTo(page, size) {
	const toolbar = page.getByTestId('toolbar');
	const before = Number(await toolbar.getAttribute('data-relayout-seq'));
	const current = page.viewportSize();
	await page.setViewportSize(size);
	const widthChanged = !current || current.width !== size.width;
	await page.waitForFunction(
		([before, widthChanged]) => {
			const el = document.querySelector('[data-testid="toolbar"]');
			if (!el) return false;
			if (widthChanged && Number(el.dataset.relayoutSeq) <= before) return false;
			// Same sum as the toolbar's own contentFits(): in-flow children only.
			const cs = getComputedStyle(el);
			const avail = el.clientWidth - parseFloat(cs.paddingLeft) - parseFloat(cs.paddingRight);
			const gap = parseFloat(cs.columnGap) || 0;
			let needed = 0;
			let count = 0;
			for (const child of el.children) {
				const ccs = getComputedStyle(child);
				if (ccs.display === 'none' || ccs.position === 'fixed' || ccs.position === 'absolute') continue;
				count++;
				if (child.classList.contains('toolbar-spacer')) continue;
				needed += child.getBoundingClientRect().width + (parseFloat(ccs.marginLeft) || 0) + (parseFloat(ccs.marginRight) || 0);
			}
			needed += gap * Math.max(0, count - 1);
			return needed <= avail + 0.5;
		},
		[before, widthChanged],
		{ timeout: 15000 }
	);
	// One frame for the settled level to paint before the oracle measures.
	await page.waitForTimeout(50);
}

test.describe('Layout overflow', () => {
	test('modeling toolbar fits at every desktop width', async ({ waffle }) => {
		const page = waffle.page;
		for (const width of WIDTHS) {
			await resizeTo(page, { width, height: 720 });
			await expectNothingOffscreen(page, expect, `modeling @${width}`);
			await expectViewportUsable(page, expect, `modeling @${width}`);
			// The settings gear is the LAST in-flow item: it is the one that
			// falls off first, so it must always be visible.
			await expect(page.getByTestId('toolbar-btn-settings'), `settings @${width}`).toBeVisible();
		}
	});

	test('toolbar re-expands when the window grows back', async ({ waffle }) => {
		const page = waffle.page;
		const toolbar = page.getByTestId('toolbar');
		await resizeTo(page, { width: 900, height: 720 });
		const narrow = Number(await toolbar.getAttribute('data-collapse-level'));
		expect(narrow).toBeGreaterThan(0);
		await resizeTo(page, { width: 2400, height: 720 });
		expect(Number(await toolbar.getAttribute('data-collapse-level'))).toBe(0);
		await expect(page.getByTestId('toolbar-btn-export-step')).toBeVisible();
	});

	test('sketch toolbar with constraints open fits at every desktop width', async ({ waffle }) => {
		const page = waffle.page;
		await clickSketch(page);
		for (const width of WIDTHS) {
			await resizeTo(page, { width, height: 720 });
			await expectNothingOffscreen(page, expect, `sketch @${width}`);
			await page.getByTestId('toolbar-btn-constraints-dropdown').click();
			await expect(page.getByTestId('constraints-dropdown')).toBeVisible();
			await expectNothingOffscreen(page, expect, `sketch + constraints @${width}`);
			// Close via the backdrop — Escape with the Select tool active would
			// FINISH the sketch, not close the menu.
			await page.locator('.dropdown-backdrop').first().click({ position: { x: 5, y: 5 } });
			await expect(page.getByTestId('constraints-dropdown')).toHaveCount(0);
		}
	});

	test('extrude dialog stays inside a short window', async ({ waffle }) => {
		const page = waffle.page;
		await clickSketch(page);
		await clickRectangle(page);
		await drawRectangle(page, -80, -60, 80, 60);
		await waitForEntityCount(page, 8, 5000);
		await clickFinishSketch(page);
		await waitForFeatureCount(page, 1, 10000);
		await clickExtrude(page);
		for (const size of [{ width: 1280, height: 720 }, { width: 1024, height: 500 }, { width: 900, height: 400 }]) {
			await resizeTo(page, size);
			await expectNothingOffscreen(page, expect, `extrude dialog @${size.width}x${size.height}`);
			const dialog = await page.getByTestId('extrude-dialog').boundingBox();
			expect(dialog.y + dialog.height, `extrude dialog bottom @${size.height}`).toBeLessThanOrEqual(size.height + 1);
		}
	});

	test('overlay browsers leave the view usable', async ({ waffle }) => {
		const page = waffle.page;
		for (const width of [1280, 1024, 800]) {
			await resizeTo(page, { width, height: 720 });
			await clickToolbarAction(page, 'examples');
			await expect(page.getByTestId('examples-browser')).toBeVisible();
			await expectNothingOffscreen(page, expect, `examples browser @${width}`);
			const panel = await page.getByTestId('examples-browser').boundingBox();
			expect(panel.width, `examples browser width @${width}`).toBeLessThanOrEqual(width * 0.4 + 1);
			await clickToolbarAction(page, 'examples');
			await expect(page.getByTestId('examples-browser')).toHaveCount(0);
		}
	});

	test('the drawing tab fits at every desktop width', async ({ waffle }) => {
		const page = waffle.page;
		// D4a's state: a Drawing tab replaces the sidebar AND the viewport, so
		// neither of this file's other states exercises it. The sheet is an A3
		// at 1:1 — wider than the window at every width here — which is
		// exactly the case the "scroll or collapse, never overflow" rule
		// exists for.
		const drawingTab = await page.evaluate(() => window.__waffle.addTab('Drawing'));
		expect(drawingTab, 'the engine added a Drawing tab').toBeTruthy();
		await page.evaluate((id) => window.__waffle.switchTab(id), drawingTab);
		// Read AFTER the add: `activeTabId` is null until the store has
		// mirrored a document, so the id has to come from the mirrored list.
		const partTab = await page.evaluate(() => {
			const tabs = window.__waffle.getDocumentState().documentTabs ?? [];
			return (tabs.find((t) => t.kind === 'Part') ?? tabs[0])?.id ?? null;
		});
		expect(partTab, 'the document has a Part tab to draw').toBeTruthy();
		await expect(page.getByTestId('drawing-panel')).toBeVisible();
		await expect(page.getByTestId('drawing-sheet')).toBeVisible();

		// The panel's WIDEST state, built BEFORE the sweep rather than after
		// it: an empty drawing panel has no view rows at all, so sweeping it
		// first measured none of the per-view controls — the name field, the
		// scale and placement numbers, the checkboxes, the parent select —
		// at any width. Two views, one with a long name, so the add form's
		// "projected from" select has an option wide enough to push a row.
		const first = await page.evaluate(
			(tab) => window.__waffle.addDrawingView(tab, { view: 'Top' }),
			partTab
		);
		await page.evaluate(
			(tab) => window.__waffle.addDrawingView(tab, { view: 'Front' }),
			partTab
		);
		await page.evaluate(
			(id) =>
				window.__waffle.editDrawingView(id, {
					name: 'Plan view of the left-hand mounting bracket'
				}),
			first
		);
		const toggles = page.locator('[data-testid^="dwg-view-toggle-"]');
		await expect(toggles).toHaveCount(2);
		for (let i = 0; i < (await toggles.count()); i += 1) await toggles.nth(i).click();

		for (const width of WIDTHS) {
			await resizeTo(page, { width, height: 720 });
			await expectNothingOffscreen(page, expect, `drawing tab @${width}`);
			// There is no canvas to measure (`expectViewportUsable` would find
			// none): the sheet is the main region, and it must keep a usable
			// size of its own.
			const sheet = await page.getByTestId('drawing-sheet').boundingBox();
			expect(sheet.width, `sheet width @${width}`).toBeGreaterThanOrEqual(320);
			expect(sheet.height, `sheet height @${width}`).toBeGreaterThanOrEqual(200);
			// And the paper stays INSIDE its region rather than pushing the
			// chrome out. `svg.wi-sheet` is `max-width: 100%`, so an A3 at
			// 1:1 is scaled down to fit; the region is an `overflow: auto`
			// scroller as the fallback for anything that still does not.
			// Either way nothing of the sheet may stick out sideways, which
			// is what is actually asserted — `overflow !== 'visible'` would
			// only have re-read the stylesheet.
			const paper = await page.evaluate(() => {
				const el = document.querySelector('[data-testid="drawing-sheet"]');
				const svg = el.querySelector('svg.wi-sheet');
				return svg ? { svg: svg.getBoundingClientRect().width, box: el.clientWidth } : null;
			});
			if (paper) {
				expect(paper.svg, `the paper fits its region @${width}`).toBeLessThanOrEqual(
					paper.box + 1
				);
			}
		}
	});

	test('side panels cannot squeeze the view out', async ({ waffle }) => {
		const page = waffle.page;
		await resizeTo(page, { width: 1024, height: 640 });
		// Drag both dividers far past their limits.
		await dragDivider(page, 'left', 1000);
		await dragDivider(page, 'right', 0);
		await expectNothingOffscreen(page, expect, 'both panels at max @1024');
		await expectViewportUsable(page, expect, 'both panels at max @1024');
		// Shrinking the window afterwards re-clamps the panels.
		await resizeTo(page, { width: 800, height: 640 });
		await expectNothingOffscreen(page, expect, 'both panels at max, window shrunk to 800');
		await expectViewportUsable(page, expect, 'both panels at max, window shrunk to 800');
		// Growing it back keeps everything in place.
		await resizeTo(page, { width: 1600, height: 640 });
		await expectNothingOffscreen(page, expect, 'window grown to 1600');
	});
});
