/**
 * Agent link tab and viewport tools (specs/waffle_mcp_server.md §2.5 `tab_add`,
 * `tab_move`, `tab_rename`, `viewport_view`, `viewport_capture`, Q4): the REAL relay
 * over MCP stdio, paired with the REAL page. Also the tab bar's drag-to-reorder,
 * which `tab_move` mirrors.
 */
import { test, expect } from '@playwright/test';
import { McpRelay, pairAgent, relayTestPort } from './helpers/mcp-relay.js';
import { collectCrashErrors, expectNoAnyCrash } from './helpers/state.js';

const AGENT = 'agent-tabs-viewport-test';
const P = (id, x, y) => ({ type: 'Point', id, x, y });
const L = (id, a, b) => ({ type: 'Line', id, start_id: a, end_id: b });
const XY = { origin: [0, 0, 0], normal: [0, 0, 1] };
/** A square of side `s` (m) centred on the origin. */
const SQUARE = (s) => [
	P(1, -s / 2, -s / 2), P(2, s / 2, -s / 2), P(3, s / 2, s / 2), P(4, -s / 2, s / 2),
	L(5, 1, 2), L(6, 2, 3), L(7, 3, 4), L(8, 4, 1)
];

/** @param {any} result */
function ok(result) {
	expect(result.isError, JSON.stringify(result.structuredContent)).toBe(false);
	return result.structuredContent;
}

/** @param {any} result @param {string} code */
function refused(result, code) {
	expect(result.isError, JSON.stringify(result.structuredContent)).toBe(true);
	expect(result.structuredContent.error.code, JSON.stringify(result.structuredContent.error)).toBe(code);
	return result.structuredContent.error;
}

/** Tab names in tab-bar order, as the user sees them. @param {import('@playwright/test').Page} page */
const tabBarNames = (page) => page.locator('[data-testid="tab-bar"] .tab .tab-name').allTextContents();

/** @param {number[]} v */
const unit = (v) => {
	const n = Math.hypot(...v);
	return v.map((c) => c / n);
};

test.describe('Agent link tabs and viewport', () => {
	/** @type {McpRelay} */
	let relay;

	test.beforeEach(async ({ baseURL }) => {
		const origin = new URL(/** @type {string} */ (baseURL)).origin;
		relay = new McpRelay({ port: await relayTestPort(), appUrl: `${origin}/`, allowOrigin: origin });
		await relay.waitListening();
		await relay.initialize(AGENT);
	});

	test.afterEach(async () => {
		expect(await relay.close()).toBe(0);
	});

	test('tab_add, tab_move and tab_rename edit the tab list, and the result is saved', async ({ page }) => {
		const crashes = collectCrashErrors(page);
		await pairAgent(page, relay, AGENT);
		const doc = ok(await relay.callTool('document_new', { name: 'Tabs' }));
		const first = doc.active_tab;

		const part = ok(await relay.callTool('tab_add', { name: 'Bracket' }));
		expect(part.active_tab).toBe(part.tab_id);
		expect(part.tabs.map((t) => [t.name, t.kind])).toEqual([['Part 1', 'Part'], ['Bracket', 'Part']]);
		expect(ok(await relay.callTool('model_summary')).features).toEqual([]);

		const asm = ok(await relay.callTool('tab_add', { kind: 'Assembly', activate: false }));
		expect(asm.active_tab).toBe(part.tab_id);
		expect(asm.tabs.map((t) => t.kind)).toEqual(['Part', 'Part', 'Assembly']);

		let info = ok(await relay.callTool('tab_move', { tab_id: asm.tab_id, index: 0 }));
		expect(info.tabs.map((t) => t.id)).toEqual([asm.tab_id, first, part.tab_id]);
		info = ok(await relay.callTool('tab_move', { tab_id: asm.tab_id, index: 99 }));
		expect(info.tabs.map((t) => t.id)).toEqual([first, part.tab_id, asm.tab_id]);
		info = ok(await relay.callTool('tab_rename', { tab_id: first, name: 'Base' }));
		expect(info.tabs.map((t) => t.name)).toEqual(['Base', 'Bracket', 'Assembly 1']);
		expect(await tabBarNames(page)).toEqual(['Base', 'Bracket', 'Assembly 1']);

		refused(await relay.callTool('tab_move', { tab_id: 'no-such-tab', index: 0 }), 'TabNotFound');
		refused(await relay.callTool('tab_rename', { tab_id: 'no-such-tab', name: 'x' }), 'TabNotFound');

		// The order and the names are part of the stored document.
		const saved = ok(await relay.callTool('document_save'));
		ok(await relay.callTool('document_new', { name: 'Elsewhere' }));
		const reopened = ok(await relay.callTool('document_open', { id: saved.id }));
		expect(reopened.tabs.map((t) => [t.id, t.name])).toEqual([
			[first, 'Base'],
			[part.tab_id, 'Bracket'],
			[asm.tab_id, 'Assembly 1']
		]);
		expectNoAnyCrash(crashes);
	});

	test('dragging a tab in the tab bar reorders the document tabs', async ({ page }) => {
		const crashes = collectCrashErrors(page);
		await pairAgent(page, relay, AGENT);
		ok(await relay.callTool('document_new', { name: 'Drag' }));
		// Test SETUP: tabs are added the way the tab bar's + button does.
		const second = await page.evaluate(() => window.__waffle.addTab('Part'));
		const third = await page.evaluate(() => window.__waffle.addTab('Part'));
		const first = ok(await relay.callTool('document_info')).tabs[0].id;
		expect(await tabBarNames(page)).toEqual(['Part 1', 'Part 2', 'Part 3']);

		// Drop Part 3 on the LEFT half of Part 1: it becomes the first tab.
		const target = page.getByTestId(`tab-${first}`);
		const box = /** @type {{width: number, height: number}} */ (await target.boundingBox());
		await page.getByTestId(`tab-${third}`).dragTo(target, { targetPosition: { x: 3, y: box.height / 2 } });
		expect(await tabBarNames(page)).toEqual(['Part 3', 'Part 1', 'Part 2']);
		expect(ok(await relay.callTool('document_info')).tabs.map((t) => t.id)).toEqual([third, first, second]);

		// Drop Part 3 on the RIGHT half of Part 2 (the last tab): it becomes the last tab.
		const last = page.getByTestId(`tab-${second}`);
		const lastBox = /** @type {{width: number, height: number}} */ (await last.boundingBox());
		await page.getByTestId(`tab-${third}`).dragTo(last, { targetPosition: { x: lastBox.width - 3, y: lastBox.height / 2 } });
		expect(await tabBarNames(page)).toEqual(['Part 1', 'Part 2', 'Part 3']);
		expectNoAnyCrash(crashes);
	});

	test('viewport_view frames a body larger than the view; viewport_capture returns that view', async ({ page }) => {
		const crashes = collectCrashErrors(page);
		await pairAgent(page, relay, AGENT);
		ok(await relay.callTool('document_new', { name: 'Big' }));
		// A 1 m cube: far outside the default camera's view.
		const sketch = ok(await relay.callTool('sketch_create', { plane: XY, entities: SQUARE(1) }));
		ok(
			await relay.callTool('feature_add', {
				operation: {
					type: 'Extrude',
					params: { sketch_id: sketch.feature_id, profile_index: 0, profile_entity_ids: [5, 6, 7, 8], depth: 1, symmetric: false, cut: false }
				}
			})
		);

		const iso = ok(await relay.callTool('viewport_view', { view: 'iso' }));
		expect(iso.view).toBe('iso');
		expect(iso.fitted).toBe(true);
		const { position, target } = iso.camera;
		// Fit centres the cube's bounding box...
		expect(target[0]).toBeCloseTo(0, 6);
		expect(target[1]).toBeCloseTo(0, 6);
		expect(target[2]).toBeCloseTo(0.5, 6);
		// ...looks along the iso direction...
		const dir = unit(position.map((c, i) => c - target[i]));
		for (const c of dir) expect(c).toBeCloseTo(1 / Math.sqrt(3), 6);
		// ...from far enough that the cube's largest extent fills at most the 50° view.
		const distance = Math.hypot(...position.map((c, i) => c - target[i]));
		expect(distance).toBeGreaterThan(1 / (2 * Math.tan((25 * Math.PI) / 180)));

		// Snapping without fit keeps the distance and the target.
		const top = ok(await relay.callTool('viewport_view', { view: 'top', fit: false }));
		expect(top.fitted).toBe(false);
		const topDir = unit(top.camera.position.map((c, i) => c - top.camera.target[i]));
		expect(topDir.map((c) => Math.round(c * 1e6) / 1e6)).toEqual([0, 1, 0]);
		expect(Math.hypot(...top.camera.position.map((c, i) => c - top.camera.target[i]))).toBeCloseTo(distance, 6);

		const capture = await relay.callTool('viewport_capture', { max_edge_px: 256 });
		const meta = ok(capture);
		expect(meta.mime_type).toBe('image/png');
		expect(Math.max(meta.width, meta.height)).toBe(256);
		const image = capture.content.find((c) => c.type === 'image');
		expect(image.mimeType).toBe('image/png');
		// The PNG decodes to the declared size and is not a blank frame.
		const decoded = await page.evaluate(async (data) => {
			const img = new Image();
			img.src = `data:image/png;base64,${data}`;
			await img.decode();
			const c = document.createElement('canvas');
			c.width = img.width;
			c.height = img.height;
			const ctx = /** @type {CanvasRenderingContext2D} */ (c.getContext('2d'));
			ctx.drawImage(img, 0, 0);
			const px = ctx.getImageData(0, 0, c.width, c.height).data;
			const colors = new Set();
			for (let i = 0; i < px.length; i += 4) colors.add((px[i] << 16) | (px[i + 1] << 8) | px[i + 2]);
			return { width: img.width, height: img.height, colors: colors.size, opaque: px[3] === 255 };
		}, image.data);
		expect([decoded.width, decoded.height]).toEqual([meta.width, meta.height]);
		expect(decoded.opaque).toBe(true);
		expect(decoded.colors).toBeGreaterThan(16);

		// Q4: a backgrounded tab does not render, so neither tool answers from it.
		await page.evaluate(() => Object.defineProperty(document, 'visibilityState', { configurable: true, get: () => 'hidden' }));
		refused(await relay.callTool('viewport_capture', {}), 'ViewportUnavailable');
		refused(await relay.callTool('viewport_view', { view: 'front' }), 'ViewportUnavailable');
		expectNoAnyCrash(crashes);
	});
});
