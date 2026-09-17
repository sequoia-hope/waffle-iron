/**
 * Agent link, Phase 1 documents and storage (specs/waffle_mcp_server.md §2.5,
 * §3.2 G5/G7, §3.4 S1–S4): the REAL relay over MCP stdio, paired with the REAL page.
 */
import { test, expect } from '@playwright/test';
import { McpRelay, pairAgent, relayTestPort } from './helpers/mcp-relay.js';
import { collectCrashErrors, expectNoAnyCrash } from './helpers/state.js';

const AGENT = 'agent-documents-test';
const P = (id, x, y) => ({ type: 'Point', id, x, y });
const L = (id, a, b) => ({ type: 'Line', id, start_id: a, end_id: b });
const XY = { origin: [0, 0, 0], normal: [0, 0, 1] };
const RECT = [P(1, 0, 0), P(2, 0.02, 0), P(3, 0.02, 0.01), P(4, 0, 0.01), L(5, 1, 2), L(6, 2, 3), L(7, 3, 4), L(8, 4, 1)];

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

test.describe('Agent link documents (Phase 1)', () => {
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

	/** A box in the open document, then saved: nothing is left pending. */
	async function buildAndSave() {
		const sketch = ok(await relay.callTool('sketch_create', { plane: XY, entities: RECT }));
		ok(
			await relay.callTool('feature_add', {
				operation: {
					type: 'Extrude',
					params: { sketch_id: sketch.feature_id, profile_index: 0, profile_entity_ids: [5, 6, 7, 8], depth: 0.005, symmetric: false, cut: false }
				}
			})
		);
		return ok(await relay.callTool('document_save'));
	}

	test('S1/S4: new, save, list and reopen a document through the storage provider', async ({ page }) => {
		const crashes = collectCrashErrors(page);
		await pairAgent(page, relay, AGENT);

		const a = ok(await relay.callTool('document_new', { name: 'Agent Doc A' }));
		expect(a.name).toBe('Agent Doc A');
		expect(a.tabs).toHaveLength(1);
		expect(a.read_only).toBe(false);
		expect(a.storage_provider.id).toBe('local');
		expect(ok(await relay.callTool('model_summary')).features).toEqual([]);

		const saved = await buildAndSave();
		expect(saved).toMatchObject({ provider: 'local', id: a.storage_id });
		expect(ok(await relay.callTool('document_info')).unsaved).toBe(false);
		const stored = await page.evaluate(async (id) => {
			const { getStore } = await import('/src/lib/storage/index.js');
			return (await getStore().get(id))?.json ?? null;
		}, a.storage_id);
		expect(JSON.parse(stored).document.name).toBe('Agent Doc A');
		expect(stored).toContain('"Extrude"');

		const listed = ok(await relay.callTool('storage_list'));
		expect(listed.documents.find((d) => d.id === a.storage_id)?.name).toBe('Agent Doc A');

		const b = ok(await relay.callTool('document_new', { name: 'Agent Doc B' }));
		expect(b.storage_id).not.toBe(a.storage_id);
		expect(ok(await relay.callTool('model_summary')).features).toEqual([]);

		const reopened = ok(await relay.callTool('document_open', { id: a.storage_id }));
		expect(reopened.name).toBe('Agent Doc A');
		const summary = ok(await relay.callTool('model_summary'));
		expect(summary.features.map((f) => f.kind)).toEqual(['Sketch', 'Extrude']);
		const m = ok(await relay.callTool('body_measure', { body_id: summary.bodies[0].body_id }));
		expect(Math.abs(m.volume_m3 - 1e-6)).toBeLessThanOrEqual(1e-15);

		refused(await relay.callTool('document_open', { id: 'no-such-document' }), 'DocumentNotFound');
		refused(await relay.callTool('storage_list', { provider: 'no-such-provider' }), 'ProviderNotFound');
		expectNoAnyCrash(crashes);
	});

	test('S3: pending changes refuse leaving; the user confirms or declines a discard', async ({ page }) => {
		const crashes = collectCrashErrors(page);
		await pairAgent(page, relay, AGENT);
		const start = ok(await relay.callTool('document_new', { name: 'Pending' }));
		ok(await relay.callTool('sketch_create', { plane: XY, entities: RECT }));
		expect(ok(await relay.callTool('document_info')).unsaved).toBe(true);

		refused(await relay.callTool('document_new', {}), 'UnsavedChanges');

		page.once('dialog', (dialog) => dialog.dismiss());
		refused(await relay.callTool('document_new', { discard_unsaved: true }), 'UserDeclined');
		const still = ok(await relay.callTool('document_info'));
		expect(still.storage_id).toBe(start.storage_id);
		expect(ok(await relay.callTool('model_summary')).features).toHaveLength(1);

		page.once('dialog', (dialog) => dialog.accept());
		const fresh = ok(await relay.callTool('document_new', { name: 'Fresh', discard_unsaved: true }));
		expect(fresh.storage_id).not.toBe(start.storage_id);
		expect(ok(await relay.callTool('model_summary')).features).toEqual([]);
		expectNoAnyCrash(crashes);
	});

	test('tab_switch and G7: tabs switch; the feature tools refuse an Assembly tab', async ({ page }) => {
		const crashes = collectCrashErrors(page);
		await pairAgent(page, relay, AGENT);
		ok(await relay.callTool('document_new', { name: 'Tabs' }));
		ok(await relay.callTool('sketch_create', { plane: XY, entities: RECT }));
		const first = ok(await relay.callTool('document_info')).active_tab;

		// Test SETUP: tabs are added the way the tab bar's + menu does.
		const part2 = await page.evaluate(() => window.__waffle.addTab('Part'));
		const assembly = await page.evaluate(() => window.__waffle.addTab('Assembly'));

		const switched = ok(await relay.callTool('tab_switch', { tab_id: part2 }));
		expect(switched.active_tab).toBe(part2);
		expect(ok(await relay.callTool('model_summary')).features).toEqual([]);
		ok(await relay.callTool('tab_switch', { tab_id: first }));
		expect(ok(await relay.callTool('model_summary')).features).toHaveLength(1);

		refused(await relay.callTool('tab_switch', { tab_id: 'no-such-tab' }), 'TabNotFound');

		// An Assembly tab switches too (the assembly tools work there,
		// agent-assembly.spec.js); G7 still keeps the feature tools off it.
		expect(ok(await relay.callTool('tab_switch', { tab_id: assembly })).active_tab).toBe(assembly);
		await page.evaluate(() => window.__waffle.recordEngineSends(true));
		const g7 = refused(await relay.callTool('sketch_create', { plane: XY, entities: RECT }), 'TabKindNotSupported');
		expect(g7.details.kind).toBe('Assembly');
		expect(await page.evaluate(() => window.__waffle.getEngineSendLog().filter((s) => s.origin === 'agent'))).toEqual([]);
		expectNoAnyCrash(crashes);
	});

	test('G5/S2: a linked read-only document refuses edits and saves', async ({ page }) => {
		const crashes = collectCrashErrors(page);
		await pairAgent(page, relay, AGENT);
		// Test SETUP: a share-link record, as /open stores one.
		const id = await page.evaluate(async () => {
			const { getStore } = await import('/src/lib/storage/index.js');
			const { newDocumentRecord } = await import('/src/lib/storage/newDocument.js');
			const record = newDocumentRecord({ name: 'linked.waffle' });
			record.link = {
				locator: { type: 'Url', url: 'https://example.invalid/linked.waffle' },
				resolved: null,
				contentHash: '',
				readOnly: true,
				name: 'linked.waffle'
			};
			await getStore().put(record);
			return record.id;
		});
		const opened = ok(await relay.callTool('document_open', { id }));
		expect(opened.read_only).toBe(true);

		await page.evaluate(() => window.__waffle.recordEngineSends(true));
		refused(await relay.callTool('sketch_create', { plane: XY, entities: RECT }), 'DocumentReadOnly');
		refused(await relay.callTool('document_save'), 'DocumentReadOnly');
		expect(await page.evaluate(() => window.__waffle.getEngineSendLog().filter((s) => s.origin === 'agent'))).toEqual([]);
		expectNoAnyCrash(crashes);
	});
});
