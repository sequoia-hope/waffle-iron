/**
 * Sketch snap click bug diagnostic tests — isolates each possible failure
 * point in the click pipeline to determine where snap-click breaks down.
 *
 * 7 tests covering: DOM layer stack, event delivery, origin click reliability,
 * endpoint snap reliability, hover-verify-click diagnostics, timing sensitivity,
 * and direct dispatchEvent on canvas.
 */
import { test, expect } from './helpers/waffle-test.js';
import { clickSketch, clickLine, pressKey } from './helpers/toolbar.js';
import { clickAt, moveTo, getCanvasBounds } from './helpers/canvas.js';
import {
	getToolState,
	getToolEventLog,
	clearToolEventLog,
	getDrawingState,
	waitForEntityCount,
} from './helpers/state.js';

/**
 * Helper: get screen offset from sketch coordinates via __waffle API.
 * Returns { x, y } pixel offset from canvas center, or null.
 */
async function sketchToOffset(page, sx, sy) {
	return page.evaluate(([x, y]) => {
		return window.__waffle?.sketchToScreenOffset?.(x, y) ?? null;
	}, [sx, sy]);
}

test.describe('sketch snap click diagnostics', () => {
	test.beforeEach(async ({ waffle }) => {
		await clickSketch(waffle.page);
	});

	test('Test 1: DOM layer stack analysis — no element blocks canvas clicks', async ({ waffle }) => {
		const page = waffle.page;

		await clickLine(page);

		// Add a point at (5, 0) via API
		await page.evaluate(() => {
			window.__waffle.addSketchEntity({ type: 'Point', id: 999, x: 5, y: 0 });
		});
		await waitForEntityCount(page, 1, 3000);

		// Get screen offset for (5, 0)
		const offset = await sketchToOffset(page, 5, 0);
		expect(offset, 'sketchToScreenOffset(5, 0) returned null').not.toBeNull();

		// Compute absolute position
		const bounds = await getCanvasBounds(page);
		expect(bounds, 'Canvas not visible').not.toBeNull();
		const absX = bounds.centerX + offset.x;
		const absY = bounds.centerY + offset.y;

		// Move to that position to trigger any hover overlays
		await moveTo(page, offset.x, offset.y);
		await page.waitForTimeout(300);

		// Get all elements at that absolute position
		const elementsAtPoint = await page.evaluate(([x, y]) => {
			return document.elementsFromPoint(x, y).map(el => ({
				tag: el.tagName,
				class: el.className,
				pointerEvents: getComputedStyle(el).pointerEvents,
			}));
		}, [absX, absY]);

		// The canvas should be in the element stack
		const canvasIndex = elementsAtPoint.findIndex(el => el.tag === 'CANVAS');
		expect(canvasIndex, 'Canvas not found in elements at point').toBeGreaterThanOrEqual(0);

		// No element above the canvas should have pointer-events: auto
		// that would intercept clicks (except known interactive elements like
		// dim-label, dim-input, snap indicators which are expected)
		const allowedClasses = ['dim-label', 'dim-input', 'dimension-input', 'snap-indicator'];
		const blockers = elementsAtPoint.slice(0, canvasIndex).filter(el => {
			if (el.pointerEvents === 'none') return false;
			// Allow known interactive overlay elements
			if (allowedClasses.some(cls => el.class && el.class.includes(cls))) return false;
			// SVG and svg-related elements used for overlays typically have pointer-events: none
			// but if they don't, they are blockers
			return el.pointerEvents === 'auto' || el.pointerEvents === '';
		});

		// Log the full stack for diagnostics
		if (blockers.length > 0) {
			console.log('DOM elements at point:', JSON.stringify(elementsAtPoint, null, 2));
			console.log('Potential blockers:', JSON.stringify(blockers, null, 2));
		}

		expect(
			blockers,
			`Found ${blockers.length} element(s) above canvas with pointer-events that could block clicks: ${JSON.stringify(blockers)}`
		).toHaveLength(0);
	});

	test('Test 2: Event delivery verification — pointerdown reaches handleToolEvent', async ({ waffle }) => {
		const page = waffle.page;

		await clickLine(page);
		await clearToolEventLog(page);

		// Click at canvas center (origin)
		await clickAt(page, 0, 0);

		// Read event log
		const log = await getToolEventLog(page);

		// Assert a pointerdown entry exists for the 'line' tool
		const pointerdownEntries = log.filter(e => e.event === 'pointerdown');
		expect(
			pointerdownEntries.length,
			`Expected at least 1 pointerdown in event log, got ${pointerdownEntries.length}. Full log: ${JSON.stringify(log)}`
		).toBeGreaterThanOrEqual(1);

		// Verify it was dispatched with the line tool active
		const linePointerdown = pointerdownEntries.find(e => e.tool === 'line');
		expect(
			linePointerdown,
			`No pointerdown for 'line' tool found. Entries: ${JSON.stringify(pointerdownEntries)}`
		).toBeDefined();
	});

	test('Test 3: Origin click 20x reliability', async ({ waffle }) => {
		const page = waffle.page;

		await clickLine(page);

		let passCount = 0;

		for (let i = 0; i < 20; i++) {
			// Move to origin to trigger snap near (0,0)
			await moveTo(page, 0, 0);
			await page.waitForTimeout(200);

			// Click at origin
			await clickAt(page, 0, 0);
			await page.waitForTimeout(200);

			// Read tool state
			const toolState = await getToolState(page);
			if (toolState === 'firstPointPlaced') {
				passCount++;
			} else {
				console.log(`Iteration ${i}: expected 'firstPointPlaced', got '${toolState}'`);
			}

			// Press Escape to reset
			await pressKey(page, 'Escape');
			await page.waitForTimeout(100);

			// Re-select line tool via API
			await page.evaluate(() => window.__waffle.setTool('line'));
			await page.waitForTimeout(100);
		}

		expect(
			passCount,
			`Origin click reliability: ${passCount}/20 passed (expected 20/20)`
		).toBe(20);
	});

	test('Test 4: Endpoint snap click 20x reliability', async ({ waffle }) => {
		const page = waffle.page;

		// Create a line via API: 2 points + 1 line from (-5, 0) to (5, 0)
		await page.evaluate(() => {
			const w = window.__waffle;
			w.addSketchEntity({ type: 'Point', id: 201, x: -5, y: 0 });
			w.addSketchEntity({ type: 'Point', id: 202, x: 5, y: 0 });
			w.addSketchEntity({ type: 'Line', id: 203, start_id: 201, end_id: 202, construction: false });
		});
		await waitForEntityCount(page, 3, 3000);

		// Get screen offset for endpoint (5, 0)
		const offset = await sketchToOffset(page, 5, 0);
		expect(offset, 'sketchToScreenOffset(5, 0) returned null').not.toBeNull();

		let passCount = 0;

		for (let i = 0; i < 20; i++) {
			// Press Escape to ensure clean state
			await pressKey(page, 'Escape');
			await page.waitForTimeout(100);

			// Set tool to 'line' via API
			await page.evaluate(() => window.__waffle.setTool('line'));
			await page.waitForTimeout(100);

			// Move to the endpoint screen position and wait for snap detection
			await moveTo(page, offset.x, offset.y);
			await page.waitForTimeout(300);

			// Check snap indicator shows coincident
			const snap = await page.evaluate(() => window.__waffle?.getSnapIndicator?.() ?? null);
			const snapDetected = snap !== null && snap.type === 'coincident';

			// Click at endpoint position
			await clickAt(page, offset.x, offset.y);
			await page.waitForTimeout(200);

			// Read tool state
			const toolState = await getToolState(page);
			if (toolState === 'firstPointPlaced') {
				passCount++;
			} else {
				console.log(
					`Iteration ${i}: expected 'firstPointPlaced', got '${toolState}' ` +
					`(snap=${snapDetected ? snap.type : 'none'}, offset=(${offset.x.toFixed(1)}, ${offset.y.toFixed(1)}))`
				);
			}
		}

		expect(
			passCount,
			`Endpoint snap click reliability: ${passCount}/20 passed (expected 20/20)`
		).toBe(20);
	});

	test('Test 5: Hover-verify-click with full diagnostics', async ({ waffle }) => {
		const page = waffle.page;

		await clickLine(page);
		await clearToolEventLog(page);

		// Move to origin (0, 0) and wait for snap detection
		await moveTo(page, 0, 0);
		await page.waitForTimeout(300);

		// Get snap indicator — proves hover detection works
		const snapIndicator = await page.evaluate(() => window.__waffle?.getSnapIndicator?.() ?? null);
		expect(
			snapIndicator,
			'Snap indicator is null after hovering at origin — hover detection may be broken'
		).not.toBeNull();

		// Click at origin
		await clickAt(page, 0, 0);
		await page.waitForTimeout(200);

		// Gather full diagnostics
		const eventLog = await getToolEventLog(page);
		const toolState = await getToolState(page);
		const drawingState = await getDrawingState(page);

		// Assert pointerdown appears in log
		const hasPointerdown = eventLog.some(e => e.event === 'pointerdown');
		expect(
			hasPointerdown,
			`No pointerdown in event log. Log has ${eventLog.length} entries: ${JSON.stringify(eventLog.map(e => e.event))}`
		).toBe(true);

		// Assert tool state is firstPointPlaced
		expect(
			toolState,
			`Tool state should be 'firstPointPlaced' but is '${toolState}'. ` +
			`Drawing state: ${JSON.stringify(drawingState)}. ` +
			`Event log events: ${JSON.stringify(eventLog.map(e => ({ event: e.event, tool: e.tool, state: e.toolState })))}`
		).toBe('firstPointPlaced');
	});

	test('Test 6: Timing sensitivity — immediate vs delayed click', async ({ waffle }) => {
		const page = waffle.page;

		// Test A: Enter line tool and immediately click (0ms extra wait)
		await clickLine(page);
		await clickAt(page, 0, 0);
		const resultA = await getToolState(page);

		// Reset
		await pressKey(page, 'Escape');
		await page.waitForTimeout(100);

		// Re-select line tool
		await page.evaluate(() => window.__waffle.setTool('line'));
		await page.waitForTimeout(100);

		// Test B: Wait 500ms before clicking
		await page.waitForTimeout(500);
		await clickAt(page, 0, 0);
		const resultB = await getToolState(page);

		// Both should succeed
		expect(
			resultA,
			`Immediate click (Test A): expected 'firstPointPlaced', got '${resultA}' — possible timing issue with tool activation`
		).toBe('firstPointPlaced');

		expect(
			resultB,
			`Delayed click (Test B): expected 'firstPointPlaced', got '${resultB}' — possible timing issue after wait`
		).toBe('firstPointPlaced');
	});

	test('Test 7: Direct dispatchEvent on canvas reaches tool handler', async ({ waffle }) => {
		const page = waffle.page;

		await clickLine(page);
		await clearToolEventLog(page);

		// Manually dispatch a PointerEvent on the canvas
		await page.evaluate(() => {
			const canvas = document.querySelector('canvas');
			const rect = canvas.getBoundingClientRect();
			const pointerdownEvent = new PointerEvent('pointerdown', {
				clientX: rect.left + rect.width / 2,
				clientY: rect.top + rect.height / 2,
				bubbles: true,
				isPrimary: true,
				pointerType: 'mouse',
				button: 0,
			});
			canvas.dispatchEvent(pointerdownEvent);
		});

		await page.waitForTimeout(200);

		// Read event log
		const log = await getToolEventLog(page);
		const hasPointerdown = log.some(e => e.event === 'pointerdown');

		expect(
			hasPointerdown,
			`Direct dispatchEvent pointerdown not in event log. ` +
			`Log entries: ${JSON.stringify(log.map(e => e.event))}. ` +
			`This means the canvas event listener is not receiving synthetic PointerEvents.`
		).toBe(true);

		// Dispatch pointerup to clean up state
		await page.evaluate(() => {
			const canvas = document.querySelector('canvas');
			const rect = canvas.getBoundingClientRect();
			const pointerupEvent = new PointerEvent('pointerup', {
				clientX: rect.left + rect.width / 2,
				clientY: rect.top + rect.height / 2,
				bubbles: true,
				isPrimary: true,
				pointerType: 'mouse',
				button: 0,
			});
			canvas.dispatchEvent(pointerupEvent);
		});

		await page.waitForTimeout(100);
	});
});
