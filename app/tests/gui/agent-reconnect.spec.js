/**
 * Agent link reconnect (specs/waffle_mcp_server.md §2.2, P7, P16, P17): the
 * REAL relay over MCP stdio, the REAL page.
 *
 * - A dropped socket resumes by itself; a call made meanwhile waits for it.
 * - A reloaded tab (what iOS does to a discarded background tab) reopens its
 *   work from the draft, THEN resumes the session; the agent is told once.
 * - `--persistent-link`: the same link pairs again after Disconnect.
 * - A mutating tool answers only once its edit is stored (draft + record), so
 *   a tab discarded a moment later loses nothing that was answered.
 * - After a reload the link reopens the document it was working on, whatever
 *   the tab's restoreOnReload policy left open.
 */
import fs from 'node:fs';
import os from 'node:os';
import path from 'node:path';
import { test, expect } from '@playwright/test';
import { McpRelay, pairAgent, relayTestPort } from './helpers/mcp-relay.js';
import { collectCrashErrors, expectNoAnyCrash } from './helpers/state.js';
import { createExtrudedBox } from './helpers/geometry.js';

const CLIENT_NAME = 'agent-reconnect-gui-test';
const P = (id, x, y) => ({ type: 'Point', id, x, y });
const L = (id, a, b) => ({ type: 'Line', id, start_id: a, end_id: b });
const XY = { origin: [0, 0, 0], normal: [0, 0, 1] };
const RECT = [P(1, 0, 0), P(2, 0.02, 0), P(3, 0.02, 0.01), P(4, 0, 0.01), L(5, 1, 2), L(6, 2, 3), L(7, 3, 4), L(8, 4, 1)];

/** @param {import('@playwright/test').Page} page */
async function draftFeatureCount(page) {
	return page.evaluate(
		() =>
			new Promise((resolve) => {
				const open = indexedDB.open('waffle-iron-drafts');
				open.onerror = () => resolve(-1);
				open.onsuccess = () => {
					const db = open.result;
					if (!db.objectStoreNames.contains('drafts')) return resolve(-1);
					const req = db.transaction('drafts', 'readonly').objectStore('drafts').getAll();
					req.onsuccess = () => {
						const counts = req.result.map((d) => JSON.parse(d.json).tabs?.[0]?.kind?.features?.features?.length ?? 0);
						resolve(counts.length ? Math.max(...counts) : -1);
					};
				};
			})
	);
}

test.describe('Agent link reconnect', () => {
	/** @type {McpRelay | null} */
	let relay = null;

	async function startRelay(baseURL, extraArgs = []) {
		const origin = new URL(/** @type {string} */ (baseURL)).origin;
		relay = new McpRelay({ port: await relayTestPort(), appUrl: `${origin}/`, allowOrigin: origin, extraArgs });
		await relay.waitListening();
		await relay.initialize(CLIENT_NAME);
		return relay;
	}

	test.afterEach(async () => {
		if (relay) {
			const code = await relay.close();
			for (const line of relay.stdoutLines) expect(JSON.parse(line).jsonrpc).toBe('2.0');
			expect(code).toBe(0);
			relay = null;
		}
	});

	test('a dropped link resumes by itself; a call made meanwhile waits for it', async ({ page, baseURL }) => {
		const crashes = collectCrashErrors(page);
		const r = await startRelay(baseURL);
		await pairAgent(page, r, CLIENT_NAME);
		await createExtrudedBox(page);

		await page.evaluate(() => window.__waffleAgentLink.dropConnection());
		await expect(page.getByTestId('agent-bar-reconnecting')).toBeVisible();

		const summary = await r.callTool('model_summary');
		expect(summary.isError).toBe(false);
		expect(summary.structuredContent.features.map((f) => f.kind)).toEqual(['Sketch', 'Extrude']);
		expect(summary.content.map((c) => c.text).join('\n')).not.toContain('reloaded');
		await expect(page.getByTestId('agent-bar-label')).toHaveText(`Agent connected — ${CLIENT_NAME}`);
		expect((await r.callTool('waffle_status')).structuredContent.state).toBe('ready');
		expectNoAnyCrash(crashes);
	});

	test('a hidden tab does not retry; becoming visible reconnects at once', async ({ page, baseURL }) => {
		const crashes = collectCrashErrors(page);
		const r = await startRelay(baseURL);
		await pairAgent(page, r, CLIENT_NAME);

		/** @param {'hidden' | 'visible'} state */
		const setVisibility = (state) =>
			page.evaluate((s) => {
				Object.defineProperty(document, 'visibilityState', { value: s, configurable: true });
				document.dispatchEvent(new Event('visibilitychange'));
			}, state);

		await setVisibility('hidden');
		await page.evaluate(() => window.__waffleAgentLink.dropConnection());
		await expect.poll(async () => (await r.callTool('waffle_status')).structuredContent.state, { timeout: 5000 }).toBe('page_away');
		// Well past the first backoff step (1 s): still away, nothing retried while hidden.
		await page.waitForTimeout(3000);
		expect((await r.callTool('waffle_status')).structuredContent.state).toBe('page_away');
		await expect(page.getByTestId('agent-bar-reconnecting')).toBeVisible();

		await setVisibility('visible');
		await expect(page.getByTestId('agent-bar-label')).toHaveText(`Agent connected — ${CLIENT_NAME}`, { timeout: 5000 });
		expect((await r.callTool('waffle_status')).structuredContent.state).toBe('ready');
		expectNoAnyCrash(crashes);
	});

	test('a reloaded tab reopens its work, then resumes; the agent is told once', async ({ page, baseURL }) => {
		const crashes = collectCrashErrors(page);
		const r = await startRelay(baseURL);
		await pairAgent(page, r, CLIENT_NAME);
		await createExtrudedBox(page);
		await expect.poll(() => draftFeatureCount(page), { timeout: 15000 }).toBe(2);

		await page.reload();
		await expect(page.getByTestId('agent-bar-label')).toHaveText(`Agent connected — ${CLIENT_NAME}`, { timeout: 30000 });

		const summary = await r.callTool('model_summary');
		expect(summary.isError).toBe(false);
		expect(summary.structuredContent.features.map((f) => f.kind)).toEqual(['Sketch', 'Extrude']);
		const texts = summary.content.map((c) => c.text);
		expect(texts[texts.length - 1]).toContain('reloaded');

		const next = await r.callTool('model_summary');
		expect(next.content.map((c) => c.text).join('\n')).not.toContain('reloaded');
		expectNoAnyCrash(crashes);
	});

	test('a mutating tool is stored before it answers; nothing is left pending', async ({ page, baseURL }) => {
		const crashes = collectCrashErrors(page);
		const r = await startRelay(baseURL);
		await pairAgent(page, r, CLIENT_NAME);

		const sketch = await r.callTool('sketch_create', { plane: XY, entities: RECT });
		expect(sketch.isError).toBe(false);
		const extrude = await r.callTool('feature_add', {
			operation: {
				type: 'Extrude',
				params: { sketch_id: sketch.structuredContent.feature_id, profile_index: 0, profile_entity_ids: [5, 6, 7, 8], depth: 0.005, symmetric: false, cut: false }
			}
		});
		expect(extrude.isError).toBe(false);
		expect(extrude.structuredContent.autosave_error).toBeUndefined();
		// Read at once, not polled: the autosave delay is 3 s, and the answer
		// means the edit is already in this tab's draft and its storage record.
		expect(await draftFeatureCount(page)).toBe(2);
		expect((await r.callTool('document_info')).structuredContent.unsaved).toBe(false);
		expectNoAnyCrash(crashes);
	});

	test('with restoreOnReload "never", a reloaded tab still lands on the agent\'s document', async ({ page, baseURL }) => {
		const crashes = collectCrashErrors(page);
		// The policy is read at boot, so it is planted before the first navigation.
		await page.addInitScript(() => {
			try {
				if (!localStorage.getItem('waffle:settings')) {
					localStorage.setItem('waffle:settings', JSON.stringify({ restoreOnReload: 'never' }));
				}
			} catch {}
		});
		const r = await startRelay(baseURL);
		await pairAgent(page, r, CLIENT_NAME);
		const sketch = await r.callTool('sketch_create', { plane: XY, entities: RECT });
		expect(sketch.isError).toBe(false);
		const extrude = await r.callTool('feature_add', {
			operation: {
				type: 'Extrude',
				params: { sketch_id: sketch.structuredContent.feature_id, profile_index: 0, profile_entity_ids: [5, 6, 7, 8], depth: 0.005, symmetric: false, cut: false }
			}
		});
		expect(extrude.isError).toBe(false);
		const before = (await r.callTool('document_info')).structuredContent;

		await page.reload();
		await expect(page.getByTestId('agent-bar-label')).toHaveText(`Agent connected — ${CLIENT_NAME}`, { timeout: 30000 });

		// The tab's own policy left the blank startup document; the link
		// reopened the agent's one before resuming.
		const info = (await r.callTool('document_info')).structuredContent;
		expect(info.storage_id).toBe(before.storage_id);
		expect(info.tabs.map((t) => t.id)).toEqual(before.tabs.map((t) => t.id));
		const summary = await r.callTool('model_summary');
		expect(summary.isError).toBe(false);
		expect(summary.structuredContent.features.map((f) => f.kind)).toEqual(['Sketch', 'Extrude']);
		expectNoAnyCrash(crashes);
	});

	test('a relay whose allocated port was taken meanwhile comes up on a fresh one', async ({ baseURL }) => {
		// Parallel workers race `relayTestPort()` (bind 0, release, hand over):
		// CI run 35291254557 lost that race with `address already in use`.
		test.skip(Boolean(process.env.AGENT_RELAY_PORT), 'the port is pinned by the environment');
		const r = await startRelay(baseURL);
		const origin = new URL(/** @type {string} */ (baseURL)).origin;
		const contender = new McpRelay({ port: r.port, appUrl: `${origin}/`, allowOrigin: origin });
		try {
			await contender.waitListening();
			expect(contender.port).not.toBe(r.port);
			await contender.initialize(`${CLIENT_NAME}-contender`);
			expect((await contender.callTool('waffle_status')).structuredContent.state).toBe('unpaired');
		} finally {
			expect(await contender.close()).toBe(0);
		}
	});

	test('a persistent link pairs again after Disconnect', async ({ page, baseURL }) => {
		const crashes = collectCrashErrors(page);
		const dir = fs.mkdtempSync(path.join(os.tmpdir(), 'waffle-persistent-link-'));
		try {
			const r = await startRelay(baseURL, ['--persistent-link', path.join(dir, 'link.code')]);
			const first = await r.callTool('waffle_connect');
			const second = await r.callTool('waffle_connect');
			expect(first.structuredContent.expires_at).toBeNull();
			expect(second.structuredContent.pairing_url).toBe(first.structuredContent.pairing_url);

			for (let round = 0; round < 2; round++) {
				await page.goto(first.structuredContent.pairing_url);
				await page.getByTestId('agent-consent-allow').click();
				await expect(page.getByTestId('agent-bar-label')).toHaveText(`Agent connected — ${CLIENT_NAME}`, { timeout: 30000 });
				expect((await r.callTool('waffle_status')).structuredContent.state).toBe('ready');
				await page.getByTestId('agent-disconnect').click();
				await expect
					.poll(async () => (await r.callTool('waffle_status')).structuredContent.state, { timeout: 5000 })
					.toBe('unpaired');
			}
		} finally {
			fs.rmSync(dir, { recursive: true, force: true });
		}
		expectNoAnyCrash(crashes);
	});
});
