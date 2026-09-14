/**
 * Agent link, Phase 0 (specs/waffle_mcp_server.md §8): the REAL relay, spoken to
 * over MCP stdio, paired with the REAL page by clicking Allow.
 *
 * - P2 + model_summary: waffle_connect -> open the pairing link -> Allow ->
 *   editor with the agent bar -> model_summary equals the page's feature tree.
 * - O14 / I7: opening a valid pairing link WITHOUT clicking opens no relay socket.
 * - O23 (Chromium, dev server on http://localhost -> ws://127.0.0.1 relay): the
 *   first test records whether the socket opened.
 */
import { test, expect } from '@playwright/test';
import { McpRelay, relayTestPort } from './helpers/mcp-relay.js';
import { collectCrashErrors, expectNoAnyCrash } from './helpers/state.js';
import { createExtrudedBox } from './helpers/geometry.js';

const CLIENT_NAME = 'agent-link-gui-test';

test.describe('Agent link (Phase 0)', () => {
	/** @type {McpRelay | null} */
	let relay = null;

	test.beforeEach(async ({ baseURL }) => {
		const origin = new URL(/** @type {string} */ (baseURL)).origin;
		relay = new McpRelay({
			port: await relayTestPort(),
			appUrl: `${origin}/`,
			allowOrigin: origin
		});
		await relay.waitListening();
		const init = await relay.initialize(CLIENT_NAME);
		expect(init.result.serverInfo.name).toBe('waffle-mcp-relay');
	});

	test.afterEach(async () => {
		if (relay) {
			const code = await relay.close();
			for (const line of relay.stdoutLines) expect(JSON.parse(line).jsonrpc).toBe('2.0');
			expect(code).toBe(0);
			relay = null;
		}
	});

	test('pair by clicking Allow, then model_summary matches the feature tree', async ({ page }) => {
		const crashes = collectCrashErrors(page);
		/** @type {{url: string, opened: boolean, closed: boolean}[]} */
		const sockets = [];
		const relayPrefix = `ws://127.0.0.1:${relay.port}`;
		page.on('websocket', (ws) => {
			if (!ws.url().startsWith(relayPrefix)) return; // the Vite HMR socket is not ours
			const entry = { url: ws.url(), opened: true, closed: false };
			sockets.push(entry);
			ws.on('close', () => (entry.closed = true));
		});

		const unpaired = await relay.callTool('model_summary');
		expect(unpaired.isError).toBe(true);
		expect(unpaired.structuredContent.error.code).toBe('NotPaired');

		const connect = await relay.callTool('waffle_connect');
		const pairingUrl = connect.structuredContent.pairing_url;
		expect(new URL(pairingUrl).pathname).toBe('/agent');
		expect(new URL(pairingUrl).searchParams.get('name')).toBe(CLIENT_NAME);

		await page.goto(pairingUrl);
		await expect(page.getByTestId('agent-consent-name')).toHaveText(CLIENT_NAME);
		await expect(page.getByTestId('agent-consent-relay')).toHaveText(relayPrefix);
		expect(sockets).toHaveLength(0);

		await page.getByTestId('agent-consent-allow').click();
		await page.waitForURL((url) => url.pathname === '/', { timeout: 30000 });
		await expect(page.getByTestId('agent-bar-label')).toHaveText(`Agent connected — ${CLIENT_NAME}`);

		// O23 cell (Chromium, http://localhost page -> ws://127.0.0.1 relay).
		expect(sockets).toHaveLength(1);
		expect(sockets[0].closed).toBe(false);
		// The agent bar needs `welcome`, which only arrives over an OPEN socket.
		console.log(
			`O23 chromium, page ${new URL(page.url()).origin} -> ${relayPrefix}: socket opened and welcome received ` +
				`(headless: a prompt could not be observed, but nothing blocked the socket); still open=${!sockets[0].closed}`
		);

		const status = await relay.callTool('waffle_status');
		expect(status.structuredContent.state).toBe('ready');

		// The editor loads in the same tab; the link survived the navigation. Build geometry.
		await page.waitForFunction(() => window.__waffle?.getState()?.engineReady === true, null, { timeout: 30000 });
		await createExtrudedBox(page);

		const summary = await relay.callTool('model_summary');
		expect(summary.isError).toBe(false);
		const tree = await page.evaluate(() => JSON.parse(JSON.stringify(window.__waffle.getFeatureTree())));
		const s = summary.structuredContent;
		expect(s.features.map((f) => f.id)).toEqual(tree.features.map((f) => f.id));
		expect(s.features.map((f) => f.kind)).toEqual(tree.features.map((f) => f.operation.type));
		expect(s.features.map((f) => f.kind)).toEqual(['Sketch', 'Extrude']);
		expect(s.rollback_index).toBe(tree.active_index ?? null);
		expect(s.bodies.length).toBeGreaterThanOrEqual(1);
		expect(s.bodies.every((b) => tree.features.some((f) => f.id === b.feature_id))).toBe(true);
		expect(s.features.every((f) => f.provenance.type === 'User')).toBe(true);
		expect(JSON.parse(summary.content[0].text)).toEqual(s);

		// Disconnect from the bar: the relay goes back to unpaired.
		await page.getByTestId('agent-disconnect').click();
		await expect(page.getByTestId('agent-bar')).toHaveCount(0);
		await expect
			.poll(async () => (await relay.callTool('waffle_status')).structuredContent.state, { timeout: 5000 })
			.toBe('unpaired');

		expectNoAnyCrash(crashes);
	});

	test('O14: a pairing link opens no relay socket without a click', async ({ page }) => {
		const crashes = collectCrashErrors(page);
		/** @type {string[]} */
		const relaySockets = [];
		/** @type {string[]} */
		const allSockets = [];
		const relayPrefix = `ws://127.0.0.1:${relay.port}`;
		page.on('websocket', (ws) => {
			allSockets.push(ws.url());
			if (ws.url().startsWith(relayPrefix)) relaySockets.push(ws.url());
		});

		const connect = await relay.callTool('waffle_connect');
		await page.goto(connect.structuredContent.pairing_url);
		await expect(page.getByTestId('agent-consent-allow')).toBeVisible();

		await page.waitForTimeout(10000); // spec O14 bound: after 10 s
		expect(relaySockets).toHaveLength(0);
		// Nothing else in the page may reach the relay port either (only Vite's HMR socket is expected).
		expect(allSockets.filter((u) => u.includes(`:${relay.port}`))).toHaveLength(0);
		expect((await relay.callTool('waffle_status')).structuredContent.state).toBe('awaiting_consent');

		expectNoAnyCrash(crashes);
	});
});
