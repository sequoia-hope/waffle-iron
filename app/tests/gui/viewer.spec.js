/**
 * The viewer (specs/waffle_server_mode.md §4, oracles V1/V3/V5): the REAL relay in
 * `--kernel host` with the REAL native host, and the REAL `/view` page.
 *
 * - `waffle_connect` hands out a viewer link; the page attaches with no engine
 *   in the browser and draws the body the host builds.
 * - A reload (what iOS does to a discarded tab) paints the last snapshot from
 *   the cache before the host answers, resumes the session, and asks for no
 *   blob it already holds.
 * - An edit that leaves a body unchanged sends no blob; a new document empties
 *   the view.
 * - Changes arrive as keyed `update` frames (§4.3) behind `rebuild` frames,
 *   the geometry rides in the compact `mq/1` encoding (§4.5), and the viewer
 *   answers `viewport_view`, `viewport_capture` and `selection_get` for the
 *   agent (§4.3, §4.9) — the three tools the host itself cannot serve.
 *
 * Needs `target/release/waffle-host` (or `$WAFFLE_HOST_BIN`); skipped otherwise.
 */
import fs from 'node:fs';
import os from 'node:os';
import path from 'node:path';
import { test, expect } from '@playwright/test';
import { McpRelay, REPO_ROOT, relayTestPort } from './helpers/mcp-relay.js';
import { collectCrashErrors, expectNoAnyCrash } from './helpers/state.js';

const HOST_BIN = process.env.WAFFLE_HOST_BIN || path.join(REPO_ROOT, 'target', 'release', 'waffle-host');
const AGENT = 'viewer-gui-test';
const P = (id, x, y) => ({ type: 'Point', id, x, y });
const L = (id, a, b) => ({ type: 'Line', id, start_id: a, end_id: b });
const XY = { origin: [0, 0, 0], normal: [0, 0, 1] };
const RECT = [P(1, 0, 0), P(2, 0.02, 0), P(3, 0.02, 0.01), P(4, 0, 0.01), L(5, 1, 2), L(6, 2, 3), L(7, 3, 4), L(8, 4, 1)];

/** @param {any} result */
function ok(result) {
	expect(result.isError, JSON.stringify(result.structuredContent)).toBe(false);
	return result.structuredContent;
}

/** @param {import('@playwright/test').Page} page */
const viewerState = (page) => page.evaluate(() => window.__waffleViewer.state());
/** @param {import('@playwright/test').Page} page */
const viewerStats = (page) => page.evaluate(() => window.__waffleViewer.stats());

/** Sketch a rectangle and extrude it: one body on the host. */
async function buildBox(relay) {
	const sketch = ok(await relay.callTool('sketch_create', { plane: XY, entities: RECT }));
	ok(
		await relay.callTool('feature_add', {
			operation: {
				type: 'Extrude',
				params: { sketch_id: sketch.feature_id, profile_index: 0, profile_entity_ids: [5, 6, 7, 8], depth: 0.005, symmetric: false, cut: false }
			}
		})
	);
	return sketch;
}

test.describe('Viewer (server mode P-D)', () => {
	test.skip(!fs.existsSync(HOST_BIN), `no waffle-host binary at ${HOST_BIN} (cargo build -p waffle-host --release)`);

	/** @type {McpRelay} */
	let relay;
	/** @type {string} */
	let docs;

	test.beforeEach(async ({ baseURL }) => {
		docs = fs.mkdtempSync(path.join(os.tmpdir(), 'waffle-viewer-docs-'));
		const origin = new URL(/** @type {string} */ (baseURL)).origin;
		relay = new McpRelay({
			port: await relayTestPort(),
			appUrl: `${origin}/`,
			allowOrigin: origin,
			extraArgs: ['--kernel', 'host', '--host-binary', HOST_BIN, '--documents', docs]
		});
		await relay.waitListening();
		await relay.initialize(AGENT);
	});

	test.afterEach(async () => {
		expect(await relay.close()).toBe(0);
		fs.rmSync(docs, { recursive: true, force: true });
	});

	test('draws what the host builds; a reload paints from the cache with no rebuild and no refetch', async ({ page }) => {
		const crashes = collectCrashErrors(page);
		const connect = ok(await relay.callTool('waffle_connect'));
		expect(connect.viewer).toBe(true);
		expect(connect.pairing_url).toContain('/view?host=');

		await page.goto(connect.pairing_url);
		await page.waitForFunction(() => window.__waffleViewer?.state().state === 'attached', null, { timeout: 30000 });
		// No engine in this browser: the editor's test API never appears.
		expect(await page.evaluate(() => typeof window.__waffle)).toBe('undefined');
		// The code was spent; the address no longer carries it.
		expect(new URL(page.url()).searchParams.get('code')).toBeNull();
		expect(ok(await relay.callTool('waffle_status')).viewers).toBe(1);
		// §4.5: this browser decodes the compact encoding, so it asked for it.
		expect((await viewerState(page)).encoding).toBe('mq/1');

		const sketch = await buildBox(relay);
		await expect.poll(async () => (await viewerState(page)).bodies, { timeout: 20000 }).toBe(1);
		await expect(page.getByTestId('viewer-status')).toHaveAttribute('data-state', 'attached');
		const built = await viewerStats(page);
		expect(built.blobsReceived).toBe(1);
		const revision = (await viewerState(page)).revision;
		expect(revision).toBeGreaterThan(0);

		// §4.6 steps 3–5: the tab comes back. The last snapshot paints from the
		// cache before the host answers; the session resumes; the one blob is a
		// cache hit, never a request.
		await page.reload();
		await page.waitForFunction(() => window.__waffleViewer?.stats().cachedPaint === true, null, { timeout: 30000 });
		await page.waitForFunction(() => window.__waffleViewer?.state().state === 'attached', null, { timeout: 30000 });
		await expect.poll(async () => (await viewerState(page)).bodies, { timeout: 20000 }).toBe(1);
		const resumed = await viewerStats(page);
		expect(resumed.blobRequests).toBe(0);
		expect(resumed.cacheHits).toBe(1);
		const state = await viewerState(page);
		expect(state.revision).toBe(revision);
		expect(state.stale).toBe(false);
		expect(await page.evaluate(() => typeof window.__waffle)).toBe('undefined');

		// A rename changes no geometry: a keyed update, no blob.
		ok(await relay.callTool('feature_rename', { feature_id: sketch.feature_id, new_name: 'Base' }));
		await expect.poll(async () => (await viewerState(page)).revision, { timeout: 20000 }).toBe(revision + 1);
		const renamed = await viewerStats(page);
		expect(renamed.blobRequests).toBe(0);
		expect(renamed.updates).toBeGreaterThan(0);
		// The tree the page shows came with that update.
		await expect(page.locator('.feature-tree')).toContainText('Base');

		// A new document is a new epoch: the view empties and names it.
		ok(await relay.callTool('document_new', { name: 'Empty' }));
		await expect.poll(async () => (await viewerState(page)).bodies, { timeout: 20000 }).toBe(0);
		await expect(page.getByTestId('viewer-document')).toHaveText('Empty');
		expectNoAnyCrash(crashes);
	});

	test('serves the viewport and selection tools the host cannot, and shows what the engine is building', async ({ page }) => {
		const crashes = collectCrashErrors(page);

		// §4.3: with no viewer attached the three tools are refused, loudly.
		const refused = await relay.callTool('selection_get');
		expect(refused.isError).toBe(true);
		expect(refused.structuredContent.error.code).toBe('ViewerUnavailable');

		const connect = ok(await relay.callTool('waffle_connect'));
		await page.goto(connect.pairing_url);
		await page.waitForFunction(() => window.__waffleViewer?.state().state === 'attached', null, { timeout: 30000 });
		await buildBox(relay);
		await expect.poll(async () => (await viewerState(page)).bodies, { timeout: 20000 }).toBe(1);

		// viewport_view drives THIS tab's camera and answers with it.
		const viewed = ok(await relay.callTool('viewport_view', { view: 'iso', fit: true }));
		expect(viewed.camera).toBeTruthy();
		expect(viewed.viewer_id).toBe((await viewerState(page)).viewerId);

		// viewport_capture renders this tab's viewport.
		const captured = await relay.callTool('viewport_capture', { max_edge_px: 320 });
		expect(captured.isError).toBe(false);
		const image = captured.content.find((c) => c.type === 'image');
		expect(image?.mimeType).toBe('image/png');
		expect(Buffer.from(image.data, 'base64').subarray(0, 4).toString('hex')).toBe('89504e47');
		expect(captured.structuredContent.width).toBeLessThanOrEqual(320);

		// A click in the viewer is a selection the agent can read.
		const box = await page.getByTestId('viewport').boundingBox();
		await page.mouse.click(box.x + box.width / 2, box.y + box.height / 2);
		await expect
			.poll(async () => ok(await relay.callTool('selection_get')).selection.length, { timeout: 15000 })
			.toBeGreaterThan(0);
		const selection = ok(await relay.callTool('selection_get'));
		expect(selection.selection[0].kind).toBe('Face');
		expect(selection.selection[0].body_id).toBeTruthy();

		// A rebuild is announced while the engine runs and closed after: the
		// banner is a pure derivation of those frames, and a tool this small
		// finishes between two polls, so the frames are what is asserted.
		const before = await viewerStats(page);
		ok(await relay.callTool('feature_add', { operation: { type: 'UnionAll', params: {} } }));
		await expect
			.poll(async () => (await viewerStats(page)).rebuildsDone, { timeout: 20000 })
			.toBeGreaterThan(before.rebuildsDone);
		const after = await viewerStats(page);
		expect(after.rebuildsStarted).toBeGreaterThan(before.rebuildsStarted);
		expect(after.lastRebuildTool).toBe('feature_add');
		// Nothing is left spinning, and the agent the banner would name is known.
		await expect(page.getByTestId('viewer-rebuilding')).toBeHidden();
		expect((await viewerState(page)).agent).toBe(AGENT);
		expectNoAnyCrash(crashes);
	});
});
