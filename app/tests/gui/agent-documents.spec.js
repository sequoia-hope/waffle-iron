/**
 * Agent link, Phase 1 documents and storage (specs/waffle_mcp_server.md §2.5,
 * §3.2 G5/G7, §3.4 S1–S4): the REAL relay over MCP stdio, paired with the REAL page.
 */
import { test, expect } from '@playwright/test';
import { McpRelay, pairAgent, relayTestPort } from './helpers/mcp-relay.js';
import { collectCrashErrors, expectNoAnyCrash } from './helpers/state.js';
import { createExtrudedBox } from './helpers/geometry.js';

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

	test('document_save refuses the empty startup document unless allow_empty is set', async ({ page }) => {
		const crashes = collectCrashErrors(page);
		await pairAgent(page, relay, AGENT);
		// A freshly booted tab holds an empty document — what an agent sees after
		// an unnoticed reload. Saving it only litters the storage list.
		refused(await relay.callTool('document_save'), 'EmptyDocument');
		const saved = ok(await relay.callTool('document_save', { allow_empty: true }));
		const listed = ok(await relay.callTool('storage_list'));
		expect(listed.documents.map((d) => d.id)).toContain(saved.id);
		expectNoAnyCrash(crashes);
	});

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

	test('opening a document frames its model', async ({ page }) => {
		const crashes = collectCrashErrors(page);
		await pairAgent(page, relay, AGENT);
		// A 20 mm part is nowhere near the 200 mm datum framing the default
		// camera starts on, so an open that does not frame leaves it a speck.
		const doc = ok(await relay.callTool('document_new', { name: 'Framed' }));
		await buildAndSave();
		// Move the camera far off the part, then open a different document and
		// come back: the open must frame the model, not keep this camera.
		ok(await relay.callTool('viewport_view', { frame: { point: [5, 5, 5], radius: 0.5 } }));
		ok(await relay.callTool('document_new', { name: 'Elsewhere' }));
		ok(await relay.callTool('document_open', { id: doc.storage_id }));
		// The engine answers before the fit's rebuild lands in the viewport.
		await page.waitForFunction(
			() => Math.abs(window.__waffle.getCameraState()?.target?.[2] ?? 9) < 0.01,
			null,
			{ timeout: 10000 }
		);
		const camera = ok(await relay.callTool('viewport_view', { fit: false })).camera;
		// Target is the box's centre (the sketch's u/v are not world x/y, so
		// take the centre from the body rather than from the sketch)...
		const body = ok(await relay.callTool('model_summary')).bodies[0].body_id;
		const { bbox_min, bbox_max } = ok(await relay.callTool('body_measure', { body_id: body }));
		for (let i = 0; i < 3; i++) {
			expect(camera.target[i]).toBeCloseTo((bbox_min[i] + bbox_max[i]) / 2, 4);
		}
		// ...and the camera is close enough that the part fills the view.
		const distance = Math.hypot(...camera.position.map((c, i) => c - camera.target[i]));
		expect(distance).toBeLessThan(0.2);
		expectNoAnyCrash(crashes);
	});

	test('S3: pending changes refuse leaving; the user confirms or declines a discard', async ({ page }) => {
		const crashes = collectCrashErrors(page);
		await pairAgent(page, relay, AGENT);
		const start = ok(await relay.callTool('document_new', { name: 'Pending' }));
		// The pending change is the USER's: an agent tool stores its edit before
		// it answers, so only the user's own edits can be waiting for autosave.
		await createExtrudedBox(page);
		expect(ok(await relay.callTool('document_info')).unsaved).toBe(true);

		refused(await relay.callTool('document_new', {}), 'UnsavedChanges');

		page.once('dialog', (dialog) => dialog.dismiss());
		refused(await relay.callTool('document_new', { discard_unsaved: true }), 'UserDeclined');
		const still = ok(await relay.callTool('document_info'));
		expect(still.storage_id).toBe(start.storage_id);
		expect(ok(await relay.callTool('model_summary')).features).toHaveLength(2);

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

	test('document_import: a .waffle file\'s text is stored under its own identity, opened and named after the file', async ({ page }) => {
		const crashes = collectCrashErrors(page);
		await pairAgent(page, relay, AGENT);
		// A real file: compose one with the engine, read its stored record back.
		const source = ok(await relay.callTool('document_new', { name: 'Source' }));
		await buildAndSave();
		const text = await page.evaluate(async (id) => {
			const { getStore } = await import('/src/lib/storage/index.js');
			return (await getStore().get(id)).json;
		}, source.storage_id);
		ok(await relay.callTool('document_new', { name: 'Elsewhere' }));

		const imported = ok(await relay.callTool('document_import', { file_name: 'Pinwheel.waffle', text }));
		expect(imported.document_id).toBe(source.document_id);
		expect(imported.storage_id).toBe(source.storage_id);
		expect(imported.name).toBe('Pinwheel');
		expect(imported.unsaved).toBe(false);
		const summary = ok(await relay.callTool('model_summary', {}));
		expect(summary.features.length).toBe(2);
		expect(summary.bodies.length).toBe(1);
		const listed = ok(await relay.callTool('storage_list', {}));
		const record = listed.documents.find((d) => d.id === source.storage_id);
		expect(record.name).toBe('Pinwheel');
		expect(listed.documents.filter((d) => d.name === 'Source')).toEqual([]);

		const named = ok(await relay.callTool('document_import', { file_name: 'x.json', text, name: 'Given' }));
		expect(named.name).toBe('Given');
		refused(await relay.callTool('document_import', { file_name: 'bad.waffle', text: '{not json' }), 'InvalidDocument');
		refused(
			await relay.callTool('document_import', {
				file_name: 'future.waffle',
				text: JSON.stringify({ format: 'waffle-iron', version: 999, min_reader_version: 999 })
			}),
			'FormatTooNew'
		);
		expect(ok(await relay.callTool('document_info', {})).name).toBe('Given');
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
